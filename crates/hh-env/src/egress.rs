//! The **egress mediator** — the Stage-2 runtime half of §5g.4's
//! `decide_egress` + credential mediation (R-2.8.3/R-2.8.4; S2.4;
//! ADR-0266 D1). The *decision* is `hh_containment::egress::decide_egress`
//! (pure); this module owns the runtime: attribution-token resolution
//! (`TokenResolver`), the durable `security.egress.{requested,decided}` rows,
//! the decide-once-consume-once re-resolution ([`EgressResolver`] +
//! `recheck_resolved` — the SSRF pivot check), the credential-binding slot
//! (`CredentialBroker::mediate_sentinel` → `drain_claim`), the
//! `network.calls` charge ([`hh_budget::Account`]), and the ask/endorse
//! path (`resolve_ask` + `amend` + `ApprovalCache`).
//!
//! Ordering contract (SV-5 + §5g.4 §6):
//!
//! 1. `security.egress.requested` is durable **before** any decision is
//!    computed — an unrecorded request never reaches the wire.
//! 2. `security.egress.decided` is durable **before** the outcome is
//!    visible — a denial is durable before the refusal surfaces; an allow
//!    is durable before the wire fires.
//! 3. A denied request never resolves, never mediates, never forwards.
//! 4. `network.calls` charges on the *forwarded* call only, attributed to
//!    the decided row's `effect_id` — the producing event is the decided
//!    row itself.
//! 5. Every refusal is fail-closed: ledger failure, broker failure,
//!    resolver failure and budget exhaustion all refuse; nothing degrades.
//!
//! The mediator is a kernel component — it never runs inside an
//! environment, and the `mh_secret:` sentinels it substitutes are drained
//! consume-once (the value crosses exactly one wire, kernel-side only).

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use hh_budget::account::{Account, AskOutcome, ChargeRequest};
use hh_budget::attribution::{default_charged_to, Attribution};
use hh_budget::quantity::ResourceQuantity;
use hh_containment::amend::{amend, AmendError};
use hh_containment::egress::{
    decide_egress, recheck_resolved, ApprovalCache, CacheScope, EgressDecision, EgressReason,
    EgressRequest, EgressSource, EgressVerdict,
};
use hh_containment::events::{
    amended_payload, egress_decided_payload, egress_requested_payload, request_ref, DecidedBy,
};
use hh_containment::policy::{AmendmentBasis, ContainmentPolicy};
use hh_ledger::event::Event;
use hh_ledger::manifest::EventRef;
use hh_ledger::store::{Lease, Store};
use hh_monitor::approval::{self, ApprovalRequest, ApprovalResponse, ResponseChoice};
use hh_ontology::dimensions::DimensionId;
use hh_provenance::{PersistenceScope, ProvenanceRecord};
use hh_secrets::{CredentialBroker, MediationOutcome, Placeholder, RefusedCode, RequestDescriptor};
use hh_wire::json::Json;

use crate::events::{EventMinter, ScopeChain};
use crate::tokens::{ResolveOutcome, TokenResolver};

/// The kernel component tag for the mediator's audit rows.
pub const COMPONENT: &str = "hh-env.egress";

// ── the resolver / transport seams ────────────────────────────────────────────

/// The mediator's DNS seam (ADR-0061 D3 — resolution is *part of the egress
/// path*): the caller's claimed `resolved_addrs` are never trusted; the
/// mediator resolves `host_norm` itself and `recheck_resolved` re-checks
/// every fresh answer against the deny rules + the non-public guard.
pub trait EgressResolver {
    /// Resolve `host_norm` to addresses. An error refuses closed.
    fn resolve(&self, host_norm: &str) -> Result<Vec<IpAddr>, String>;
}

/// The system resolver — `ToSocketAddrs` (an IP literal short-circuits; a
/// name uses the platform resolver — for the hermetic S2.4 fixtures this
/// resolves `localhost`/literals only; real external DNS is the ticket's
/// deferred edge).
pub struct SystemResolver;

impl EgressResolver for SystemResolver {
    fn resolve(&self, host_norm: &str) -> Result<Vec<IpAddr>, String> {
        if let Ok(ip) = host_norm.parse::<IpAddr>() {
            return Ok(vec![ip]);
        }
        let mut out = Vec::new();
        for sa in (host_norm, 0u16)
            .to_socket_addrs()
            .map_err(|e| format!("resolve {host_norm}: {e}"))?
        {
            if !out.contains(&sa.ip()) {
                out.push(sa.ip());
            }
        }
        Ok(out)
    }
}

/// A hermetic resolver — an explicit `host → addrs` table (the test fixtures;
/// an unknown host resolves to nothing, which the caller treats as
/// unroutable).
pub struct StaticResolver {
    /// The table.
    pub map: std::collections::BTreeMap<String, Vec<IpAddr>>,
}

impl EgressResolver for StaticResolver {
    fn resolve(&self, host_norm: &str) -> Result<Vec<IpAddr>, String> {
        if let Ok(ip) = host_norm.parse::<IpAddr>() {
            return Ok(vec![ip]);
        }
        Ok(self.map.get(host_norm).cloned().unwrap_or_default())
    }
}

/// The wire response the transport returns (the mediator records only the
/// status line — bodies stay out of the ledger).
#[derive(Debug, Clone, PartialEq)]
pub struct WireResponse {
    /// The HTTP status code.
    pub status: u16,
    /// The response headers.
    pub headers: Vec<(String, String)>,
    /// The response body.
    pub body: Vec<u8>,
}

/// The transport seam — the *only* wire the mediator writes (mediated envs
/// have no other network path). Implementations dial a **resolved address**
/// (the consume-once answer), never re-resolve the name themselves.
pub trait EgressTransport {
    /// Forward the (credential-substituted) request to `addr:port`.
    #[allow(clippy::too_many_arguments)] // the arity is the wire request's.
    fn forward(
        &self,
        addr: IpAddr,
        port: u16,
        host: &str,
        method: &str,
        path: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Result<WireResponse, String>;
}

/// The local HTTP/1.1 transport — a plain `TcpStream` to the resolved
/// address (hermetic: localhost fixtures; no TLS — TLS-terminating
/// inspection is a deferred row, `tls.terminate` is always `false` at this
/// stage, so mediated traffic is plain HTTP to a checked address).
pub struct LocalHttpTransport {
    /// The dial/read timeout.
    pub timeout: Duration,
}

impl Default for LocalHttpTransport {
    fn default() -> Self {
        LocalHttpTransport {
            timeout: Duration::from_secs(5),
        }
    }
}

impl EgressTransport for LocalHttpTransport {
    fn forward(
        &self,
        addr: IpAddr,
        port: u16,
        host: &str,
        method: &str,
        path: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Result<WireResponse, String> {
        let mut s =
            TcpStream::connect((addr, port)).map_err(|e| format!("connect {addr}:{port}: {e}"))?;
        s.set_read_timeout(Some(self.timeout)).ok();
        s.set_write_timeout(Some(self.timeout)).ok();
        let body_bytes = body.unwrap_or(&[]);
        let mut req = format!(
            "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\nContent-Length: {}\r\n",
            body_bytes.len()
        );
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        req.push_str("\r\n");
        s.write_all(req.as_bytes())
            .and_then(|_| s.write_all(body_bytes))
            .map_err(|e| format!("write {addr}:{port}: {e}"))?;
        let mut buf = Vec::new();
        s.read_to_end(&mut buf)
            .map_err(|e| format!("read {addr}:{port}: {e}"))?;
        parse_response(&buf)
    }
}

/// A minimal HTTP/1.1 response parse (status line + headers + the rest as
/// body — enough for the hermetic fixtures).
fn parse_response(buf: &[u8]) -> Result<WireResponse, String> {
    let text = String::from_utf8_lossy(buf);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| "malformed response (no header/body split)".to_string())?;
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| format!("malformed status line: {status_line}"))?;
    let mut headers = Vec::new();
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    Ok(WireResponse {
        status,
        headers,
        body: body.as_bytes().to_vec(),
    })
}

// ── outcomes / errors ─────────────────────────────────────────────────────────

/// What a mediated request resolved to.
#[derive(Debug, Clone)]
pub enum MediatedOutcome {
    /// The request was forwarded — `decided{allow}` is durable, the
    /// `network.calls` charge is appended, the wire response is here.
    Forwarded {
        /// The `decision_request` ref.
        request_ref: String,
        /// The `security.egress.decided` event id.
        decided_ref: String,
        /// The `budget.charge.appended` source (the decided event).
        charge_source: String,
        /// The wire response.
        response: WireResponse,
    },
    /// Refused by policy — `decided{deny}` is durable before this is
    /// visible.
    Refused {
        /// The `decision_request` ref.
        request_ref: String,
        /// The `security.egress.decided` event id.
        decided_ref: String,
        /// The closed deny reason.
        reason: EgressReason,
    },
    /// The egress decision was `allow` but a credential sentinel refused
    /// (its own `security.credential.denied` row is durable; the decided
    /// row spells `deny{source: monitor, reason: denied}` — the precise
    /// refusal code lives on the credential row, keyed by the shared
    /// `effect_id`; ADR-0266 D3).
    RefusedCredential {
        /// The `decision_request` ref.
        request_ref: String,
        /// The `security.egress.decided` event id.
        decided_ref: String,
        /// The credential refusal code.
        code: RefusedCode,
    },
    /// `default_unmatched = ask` — the pending/requested rows and the
    /// `decided{decision: ask}` row are durable; the operator's
    /// `ApprovalResponse` resolves through [`EgressMediator::endorse_asked`].
    Asked {
        /// The `decision_request` ref.
        request_ref: String,
        /// The `security.egress.decided` event id.
        decided_ref: String,
        /// The pending `permission_id` (the ask's coordinate).
        permission_id: String,
    },
}

/// The mediator's failure modes — typed, never a warning.
#[derive(Debug)]
pub enum EgressError {
    /// A ledger append failed — fail closed, nothing proceeds.
    Ledger {
        /// What failed.
        detail: String,
    },
    /// The budget op failed.
    Budget {
        /// What failed.
        detail: String,
    },
    /// The mediator's own resolution failed (the wire never fired —
    /// `decided{allow}` stands, no charge was made).
    Resolve {
        /// What failed.
        detail: String,
    },
    /// The transport failed (the wire errored after `decided{allow}`).
    Transport {
        /// What failed.
        detail: String,
    },
    /// An amendment refusal (`endorse_asked`).
    Amend(AmendError),
    /// The broker refused/denied outside the `MediationOutcome` path.
    Broker {
        /// What failed.
        detail: String,
    },
}

// ── the mediator ─────────────────────────────────────────────────────────────

/// `EgressMediator` — the kernel-side runtime composing the pure decision
/// (`decide_egress`), the broker, the budget and the wire. One per
/// mediated environment session; `policy`/`cache` evolve on `amend`.
pub struct EgressMediator<'a> {
    /// The store (the writer lease's target).
    pub store: &'a mut Store,
    /// The run.
    pub run_id: String,
    /// The writer lease.
    pub lease: &'a Lease,
    /// The attribution-token resolver (resolve-only).
    pub tokens: TokenResolver,
    /// The credential broker (kernel-held).
    pub broker: &'a mut CredentialBroker,
    /// The effective containment policy (swapped on `amend`).
    pub policy: ContainmentPolicy,
    /// The narrowing-only approval cache (version-pinned).
    pub cache: ApprovalCache,
    /// The budget node `network.calls`/`approvals.requested` charge against.
    pub budget_id: Option<String>,
    /// The participant the charges attribute to.
    pub participant_ref: String,
    /// The resolver (the mediator's own DNS).
    pub resolver: Box<dyn EgressResolver>,
    /// The transport (the only wire).
    pub transport: Box<dyn EgressTransport>,
}

impl<'a> EgressMediator<'a> {
    /// Emit one audit event (durable before return — the ordering contract).
    fn emit(
        &mut self,
        class: &str,
        payload: Json,
        effect_id: Option<&str>,
        chain: &ScopeChain,
    ) -> Result<Event, EgressError> {
        let minter = EventMinter::new(self.store, &self.run_id);
        let ev = match effect_id {
            // An empty/None effect scope mints unscoped — an unattributed
            // request must still leave a durable row (fail-closed audit).
            Some(e) if !e.is_empty() => minter.mint_effect(class, payload, e, chain),
            _ => minter.mint(class, payload),
        }
        .map_err(|e| EgressError::Ledger {
            detail: format!("mint {class}: {e}"),
        })?;
        self.store
            .append(&self.run_id, self.lease, vec![ev.clone()])
            .map_err(|e| EgressError::Ledger {
                detail: format!("append {class}: {e}"),
            })?;
        // Durable before return — the ordering contract.
        Ok(ev)
    }

    /// Collect the sentinel spellings — `req.credential_sentinels` plus every
    /// `mh_secret:`-prefixed header value and path/body occurrence.
    fn collect_sentinels(req: &EgressRequest) -> Vec<String> {
        let mut out: BTreeSet<String> = req.credential_sentinels.iter().cloned().collect();
        for (_, v) in &req.headers {
            for w in v.split(|c: char| c.is_whitespace() || c == ',') {
                if Placeholder::is_placeholder(w) {
                    out.insert(w.to_string());
                }
            }
        }
        for text in [req.path.as_deref(), req.body.as_deref()]
            .into_iter()
            .flatten()
        {
            for w in
                text.split(|c: char| !(c.is_alphanumeric() || c == ':' || c == '_' || c == '-'))
            {
                if Placeholder::is_placeholder(w) {
                    out.insert(w.to_string());
                }
            }
        }
        out.into_iter().collect()
    }

    /// `handle(request, chain)` — the mediated path: requested → attributed
    /// → decide → recheck → mediate → decided → wire → charge.
    pub fn handle(
        &mut self,
        req: &EgressRequest,
        chain: &ScopeChain,
    ) -> Result<MediatedOutcome, EgressError> {
        let started = self.store.now_ms();
        let sentinel_refs = Self::collect_sentinels(req);

        // 1. `requested` is durable before the decision runs.
        self.emit(
            "security.egress.requested",
            egress_requested_payload(req, &sentinel_refs),
            req.effect_id.as_deref(),
            chain,
        )?;

        // 2. Attribution — the token's binding must resolve AND match the
        //    env handle AND the claimed effect (unattributed ⇒ deny).
        let (attributed, resolved_effect) = match self.tokens.resolve(&req.token) {
            ResolveOutcome::Resolved {
                effect_id,
                env_handle_id,
                ..
            } => {
                let ok = env_handle_id == req.env_handle
                    && req
                        .effect_id
                        .as_deref()
                        .is_none_or(|claimed| claimed == effect_id);
                (ok, Some(effect_id))
            }
            ResolveOutcome::Unknown => (false, None),
        };
        // The decided row's effect_id: the token's binding when resolved,
        // else the claim (the unattributed deny still names what was claimed).
        let effect_id = resolved_effect
            .clone()
            .or_else(|| req.effect_id.clone())
            .unwrap_or_default();

        // 3. The pure decision.
        let decision = decide_egress(&self.policy, req, attributed, &self.cache);

        match decision.decision {
            EgressVerdict::Deny => {
                let ev = self.decide_row(
                    req,
                    &decision,
                    DecidedBy::Policy,
                    &[],
                    &[],
                    &effect_id,
                    started,
                    chain,
                )?;
                return Ok(MediatedOutcome::Refused {
                    request_ref: request_ref(req),
                    decided_ref: ev.event_id,
                    reason: decision.reason.unwrap_or(EgressReason::Denied),
                });
            }
            EgressVerdict::Ask => {
                return self.ask(req, &decision, &effect_id, started, chain);
            }
            EgressVerdict::Allow => {}
        }

        self.forward(
            req,
            &decision,
            DecidedBy::Policy,
            &effect_id,
            started,
            chain,
        )
    }

    /// The decided row (durable before its outcome is visible).
    #[allow(clippy::too_many_arguments)] // the arity is the row material's.
    fn decide_row(
        &mut self,
        req: &EgressRequest,
        decision: &EgressDecision,
        decided_by: DecidedBy,
        checked_addrs: &[String],
        applied: &[String],
        effect_id: &str,
        started: u64,
        chain: &ScopeChain,
    ) -> Result<Event, EgressError> {
        let latency = self.store.now_ms().saturating_sub(started);
        self.emit(
            "security.egress.decided",
            egress_decided_payload(
                req,
                &self.policy.version_id,
                decision,
                decided_by,
                checked_addrs,
                applied,
                effect_id,
                latency,
            ),
            Some(effect_id),
            chain,
        )
    }

    /// The ask path — `approvals.requested` reserve → pending/requested rows
    /// → `decided{decision: ask}`.
    fn ask(
        &mut self,
        req: &EgressRequest,
        decision: &EgressDecision,
        effect_id: &str,
        started: u64,
        chain: &ScopeChain,
    ) -> Result<MediatedOutcome, EgressError> {
        // The approvals-budget reserve — exhausted ⇒ deny{budget_exhausted}.
        if let Some(budget_id) = self.budget_id.clone() {
            let mut acct =
                Account::open(&mut *self.store, &self.run_id).map_err(|e| EgressError::Budget {
                    detail: format!("account open: {e:?}"),
                })?;
            match acct
                .resolve_ask(self.lease, &budget_id)
                .map_err(|e| EgressError::Budget {
                    detail: format!("resolve_ask: {e:?}"),
                })? {
                AskOutcome::Deny => {
                    let deny = EgressDecision {
                        decision: EgressVerdict::Deny,
                        source: EgressSource::DefaultUnmatched,
                        rule_ref: None,
                        reason: Some(EgressReason::BudgetExhausted),
                        credential_binding_candidates: vec![],
                        rule_index: None,
                    };
                    let ev = self.decide_row(
                        req,
                        &deny,
                        DecidedBy::Default,
                        &[],
                        &[],
                        effect_id,
                        started,
                        chain,
                    )?;
                    return Ok(MediatedOutcome::Refused {
                        request_ref: request_ref(req),
                        decided_ref: ev.event_id,
                        reason: EgressReason::BudgetExhausted,
                    });
                }
                AskOutcome::Allow => {}
            }
        }

        // Mint the pending coordinate + emit the owed-decision trail.
        let cap = hh_compiler::plan::PinnedRef {
            semantic_id: "net_egress".to_string(),
            version_id: format!("net_egress@{}", self.policy.version_id),
        };
        let args_hash = hh_identity::idp::idp_id(
            "egress.ask",
            format!(
                "{}\u{1f}{}\u{1f}{}\u{1f}{}",
                req.host_norm(),
                req.port,
                req.protocol.as_str(),
                req.method.as_deref().unwrap_or("")
            )
            .as_bytes(),
        );
        let permission_id = approval::mint_permission_id(&cap, &args_hash, false, effect_id);
        let request = ApprovalRequest {
            permission_id: permission_id.clone(),
            request: approval::PermissionRequest {
                subject_ref: self.participant_ref.clone(),
                capability_ref: cap,
                args_canonical_hash: args_hash,
                reason: format!("egress ask: {}", req.host_norm()),
            },
            options: vec![
                approval::ApprovalOption {
                    id: approval::ApprovalOptionId::AllowOnce,
                    label: "allow_once".to_string(),
                },
                approval::ApprovalOption {
                    id: approval::ApprovalOptionId::AllowLease,
                    label: "allow_lease".to_string(),
                },
                approval::ApprovalOption {
                    id: approval::ApprovalOptionId::Deny,
                    label: "deny".to_string(),
                },
            ],
            mode: approval::ApprovalMode::Sync,
            timeout: None,
            explanation: approval::Explanation {
                display: format!("egress to {}:{}", req.host_norm(), req.port),
                rows: vec![],
                model_justification: None,
            },
            batch_id: None,
        };
        let asked_at = self.store.now_ms();
        self.emit(
            "security.permission.pending",
            approval::pending_payload(&request, effect_id, asked_at),
            Some(effect_id),
            chain,
        )?;
        self.emit(
            "security.permission.requested",
            approval::requested_payload(&request, effect_id, asked_at),
            Some(effect_id),
            chain,
        )?;
        let ask_decision = EgressDecision {
            decision: EgressVerdict::Ask,
            source: decision.source,
            rule_ref: None,
            reason: None,
            credential_binding_candidates: vec![],
            rule_index: None,
        };
        let ev = self.decide_row(
            req,
            &ask_decision,
            DecidedBy::Default,
            &[],
            &[],
            effect_id,
            started,
            chain,
        )?;
        Ok(MediatedOutcome::Asked {
            request_ref: request_ref(req),
            decided_ref: ev.event_id,
            permission_id,
        })
    }

    /// `endorse_asked(req, response, endorser)` — the monitor's answer
    /// resolves the pending ask (ADR-0266 D6):
    ///
    /// - `deny`/`more_info` ⇒ `decided{deny, source: monitor}` + refuse.
    /// - `allow_once` ⇒ `decided{allow, source: monitor}` + forward — the
    ///   endorsement covers *this request only* (nothing cached, nothing
    ///   amended).
    /// - `allow_lease{session|persisted}` ⇒ `amend(AddEgressAllow)` at the
    ///   lease's scope (`session | run` — §5g.4 §2's bound), the
    ///   `security.containment.amended` row, the cache insert (when
    ///   `session_cache`), then the forward under `decided{allow, monitor}`.
    ///   An `amend` refusal falls back to a cache entry when
    ///   `session_cache` is on; otherwise the endorsement is refused typed.
    pub fn endorse_asked(
        &mut self,
        req: &EgressRequest,
        response: &ApprovalResponse,
        endorser: &ProvenanceRecord,
        chain: &ScopeChain,
    ) -> Result<MediatedOutcome, EgressError> {
        let started = self.store.now_ms();
        let effect_id = req.effect_id.clone().unwrap_or_default();
        match &response.choice {
            ResponseChoice::Deny { .. } | ResponseChoice::MoreInfo => {
                let deny = EgressDecision {
                    decision: EgressVerdict::Deny,
                    source: EgressSource::Monitor,
                    rule_ref: None,
                    reason: Some(EgressReason::Denied),
                    credential_binding_candidates: vec![],
                    rule_index: None,
                };
                let ev = self.decide_row(
                    req,
                    &deny,
                    DecidedBy::Monitor,
                    &[],
                    &[],
                    &effect_id,
                    started,
                    chain,
                )?;
                return Ok(MediatedOutcome::Refused {
                    request_ref: request_ref(req),
                    decided_ref: ev.event_id,
                    reason: EgressReason::Denied,
                });
            }
            ResponseChoice::AllowOnce => {}
            ResponseChoice::AllowLease(spec) => {
                // §5g.4 §2's bound — an approval hatch lives at
                // `session | run`; at this stage a `run` lease *is* the
                // run (`PersistenceScope::Session` would breach the `Run`
                // ceiling — the lease's run is the run, not a
                // cross-run surface). `session` (and anything wider — the
                // mint already bounds the lease at `≤ session`) asks beyond
                // the run — under the default ceiling the `amend` refuses
                // and the narrowing-only cache is the fallback (ADR-0266 D6).
                let (persistence, cache_scope) = match spec.scope {
                    approval::LeaseScope::Turn => (PersistenceScope::Turn, None),
                    approval::LeaseScope::Run => (PersistenceScope::Run, Some(CacheScope::Session)),
                    approval::LeaseScope::Session
                    | approval::LeaseScope::User
                    | approval::LeaseScope::Project
                    | approval::LeaseScope::Definition => {
                        (PersistenceScope::User, Some(CacheScope::Run))
                    }
                };
                let host = req.host_norm();
                let diff =
                    hh_containment::admit::ContainmentDiff::AddEgressAllow { host: host.clone() };
                match amend(
                    &self.policy,
                    &diff,
                    AmendmentBasis::Approval,
                    endorser,
                    persistence,
                ) {
                    Ok(outcome) => {
                        self.emit(
                            "security.containment.amended",
                            amended_payload(
                                &outcome,
                                &diff,
                                AmendmentBasis::Approval,
                                endorser,
                                persistence,
                                Some(&effect_id),
                            ),
                            Some(&effect_id),
                            chain,
                        )?;
                        self.policy = outcome.policy;
                        self.cache.retain_version(&self.policy.version_id);
                    }
                    Err(e) => {
                        // Fall back to a narrowing-only cache entry when the
                        // policy permits caching — the endorsement still
                        // narrows to the exact request shape (ADR-0266 D6).
                        if !self.policy.amendment.session_cache || cache_scope.is_none() {
                            return Err(EgressError::Amend(e));
                        }
                    }
                }
                if let Some(scope) = cache_scope {
                    if self.policy.amendment.session_cache {
                        self.cache.insert(
                            &host,
                            req.port,
                            req.protocol,
                            req.method.as_deref(),
                            scope,
                            &self.policy.version_id,
                            &effect_id,
                            &response.permission_id,
                        );
                    }
                }
            }
        }
        // The endorsement decided allow — decide/forward under
        // `source: monitor`.
        let allow = EgressDecision {
            decision: EgressVerdict::Allow,
            source: EgressSource::Monitor,
            rule_ref: None,
            reason: None,
            credential_binding_candidates: vec![],
            rule_index: None,
        };
        self.forward(req, &allow, DecidedBy::Monitor, &effect_id, started, chain)
    }

    /// The allow pipeline — recheck → sentinels → decided{allow} → wire →
    /// charge. Shared by the direct allow and the monitor-endorsed allow.
    fn forward(
        &mut self,
        req: &EgressRequest,
        decision: &EgressDecision,
        decided_by: DecidedBy,
        effect_id: &str,
        started: u64,
        chain: &ScopeChain,
    ) -> Result<MediatedOutcome, EgressError> {
        // (d) Consume-once re-resolution: the caller's `resolved_addrs` are
        //     never trusted — the mediator resolves and re-checks (the SSRF
        //     pivot — a public name resolving private dies here).
        let host_norm = req.host_norm();
        let fresh = self
            .resolver
            .resolve(&host_norm)
            .map_err(|e| EgressError::Resolve {
                detail: format!("resolve {host_norm}: {e}"),
            })?;
        if let Some(deny) = recheck_resolved(&self.policy, req, &fresh) {
            let ev = self.decide_row(
                req,
                &deny,
                decided_by,
                &fresh.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
                &[],
                effect_id,
                started,
                chain,
            )?;
            return Ok(MediatedOutcome::Refused {
                request_ref: request_ref(req),
                decided_ref: ev.event_id,
                reason: deny.reason.unwrap_or(EgressReason::Denied),
            });
        }

        // (e) The credential-binding slot — each sentinel mediates through
        //     the broker (denied row durable before the refusal is visible).
        let sentinel_refs = Self::collect_sentinels(req);
        let mut staged: Vec<(String, String)> = Vec::new(); // (sentinel, claim_id)
        let mut applied: Vec<String> = Vec::new();
        for sentinel in &sentinel_refs {
            let desc = RequestDescriptor {
                effect_id: effect_id.to_string(),
                env_handle: req.env_handle.clone(),
                destination: host_norm.clone(),
                method: req.method.clone(),
                path: req.path.clone(),
            };
            match self
                .broker
                .mediate_sentinel(&mut *self.store, &self.run_id, self.lease, sentinel, &desc)
                .map_err(|e| EgressError::Broker {
                    detail: format!("mediate_sentinel: {e:?}"),
                })? {
                MediationOutcome::Staged(d) => {
                    staged.push((sentinel.clone(), d.claim_id));
                    applied.push(d.binding_id);
                }
                MediationOutcome::Refused(r) => {
                    // The egress decision was allow; the credential leg
                    // refused — the decided row spells `deny{monitor}` and
                    // the credential.denied row carries the precise code.
                    let deny = EgressDecision {
                        decision: EgressVerdict::Deny,
                        source: EgressSource::Monitor,
                        rule_ref: None,
                        reason: Some(EgressReason::Denied),
                        credential_binding_candidates: vec![],
                        rule_index: None,
                    };
                    let ev = self.decide_row(
                        req,
                        &deny,
                        decided_by,
                        &fresh.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
                        &[],
                        effect_id,
                        started,
                        chain,
                    )?;
                    return Ok(MediatedOutcome::RefusedCredential {
                        request_ref: request_ref(req),
                        decided_ref: ev.event_id,
                        code: r.code,
                    });
                }
            }
        }

        // (f-pre) The `network.calls` gate — a call that can't be charged
        //     must not fire: when the dimension is already at/against its
        //     cap, the decided row spells `deny{monitor, budget_exhausted}`
        //     and nothing reaches the wire (§8.2's "deny before the
        //     authority is spent" — the ask path's `resolve_ask` is the
        //     same gate for `approvals.requested`).
        if let Some(budget_id) = self.budget_id.clone() {
            let acct =
                Account::open(&mut *self.store, &self.run_id).map_err(|e| EgressError::Budget {
                    detail: format!("account open: {e:?}"),
                })?;
            let exceeded = acct.check(&budget_id).map_err(|e| EgressError::Budget {
                detail: format!("check {budget_id}: {e:?}"),
            })?;
            let calls_exceeded = exceeded
                .iter()
                .any(|x| x.dimension.primary() == Some(DimensionId::NetworkCalls));
            if calls_exceeded {
                let deny = EgressDecision {
                    decision: EgressVerdict::Deny,
                    source: EgressSource::Monitor,
                    rule_ref: None,
                    reason: Some(EgressReason::BudgetExhausted),
                    credential_binding_candidates: vec![],
                    rule_index: None,
                };
                let ev = self.decide_row(
                    req,
                    &deny,
                    decided_by,
                    &fresh.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
                    &[],
                    effect_id,
                    started,
                    chain,
                )?;
                return Ok(MediatedOutcome::Refused {
                    request_ref: request_ref(req),
                    decided_ref: ev.event_id,
                    reason: EgressReason::BudgetExhausted,
                });
            }
        }

        // `decided{allow}` — durable before the wire fires.
        let decided_ev = self.decide_row(
            req,
            decision,
            decided_by,
            &fresh.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
            &applied,
            effect_id,
            started,
            chain,
        )?;
        let decided_ref = decided_ev.event_id.clone();

        // (f) Drain the claims consume-once and substitute into the headers.
        let mut headers = req.headers.clone();
        for (sentinel, claim_id) in &staged {
            let Some((_bid, value)) = self.broker.drain_claim(claim_id) else {
                return Err(EgressError::Broker {
                    detail: format!("claim {claim_id} vanished before drain"),
                });
            };
            for (k, v) in headers.iter_mut() {
                if v.contains(sentinel.as_str()) {
                    *v = v.replace(sentinel.as_str(), &value);
                    let _ = k;
                }
            }
        }

        // (g) The wire — dial the *resolved* address, never re-resolve.
        let addr = *fresh.first().ok_or_else(|| EgressError::Resolve {
            detail: format!("{host_norm} resolved to no addresses"),
        })?;
        let response = self
            .transport
            .forward(
                addr,
                req.port,
                &host_norm,
                req.method.as_deref().unwrap_or("GET"),
                req.path.as_deref().unwrap_or("/"),
                &headers,
                req.body.as_deref().map(|b| b.as_bytes()),
            )
            .map_err(|e| EgressError::Transport {
                detail: format!("forward {addr}:{}: {e}", req.port),
            })?;

        // (h) `network.calls` — one charge per forwarded call, attributed to
        //     the decided row's effect, source = the decided event.
        let mut charge_source = decided_ref.clone();
        if let Some(budget_id) = self.budget_id.clone() {
            let mut acct =
                Account::open(&mut *self.store, &self.run_id).map_err(|e| EgressError::Budget {
                    detail: format!("account open: {e:?}"),
                })?;
            let source = EventRef {
                run_id: self.run_id.clone(),
                event_id: decided_ref.clone(),
            };
            let mut attribution = Attribution::subject(
                self.run_id.clone(),
                budget_id.clone(),
                self.participant_ref.clone(),
            );
            attribution.charged_to = default_charged_to("security.egress.decided");
            attribution.component_class = Some(COMPONENT.to_string());
            acct.charge(
                self.lease,
                &ChargeRequest {
                    budget_id,
                    quantity: ResourceQuantity {
                        dimension: DimensionId::NetworkCalls,
                        amount: 1,
                        unit: "calls".to_string(),
                        model_ref: None,
                        measured_at: source.clone(),
                    },
                    source,
                    attribution,
                    reservation_id: None,
                    cache_ttl: None,
                },
            )
            .map_err(|e| EgressError::Budget {
                detail: format!("charge network.calls: {e:?}"),
            })?;
            charge_source = decided_ref.clone();
        }

        Ok(MediatedOutcome::Forwarded {
            request_ref: request_ref(req),
            decided_ref,
            charge_source,
            response,
        })
    }

    /// Dispatch the destination-side revocation intents a `revoke` produced
    /// (LT-05) — each intent is itself a mediated egress (a POST to the
    /// destination's `revocation_path` through the same `handle` pipeline,
    /// so the dispatch is durably ledgered as `requested`/`decided`).
    pub fn dispatch_revocations(
        &mut self,
        intents: &[hh_secrets::RevokeIntent],
        token: &str,
        chain: &ScopeChain,
    ) -> Vec<Result<MediatedOutcome, EgressError>> {
        intents
            .iter()
            .map(|i| {
                let req = EgressRequest {
                    token: token.to_string(),
                    effect_id: None,
                    tool_call_id: "credential-revoke".to_string(),
                    env_handle: "kernel".to_string(),
                    protocol: hh_containment::policy::EgressProtocol::Http,
                    host_raw: i.host_pattern.clone(),
                    resolved_addrs: vec![],
                    port: i.port.unwrap_or(80),
                    method: Some("POST".to_string()),
                    path: Some(i.revocation_path.clone()),
                    headers: vec![],
                    body: Some(format!(
                        "{{\"binding_id\":\"{}\",\"channel_id\":\"{}\"}}",
                        i.binding_id, i.channel_id
                    )),
                    credential_sentinels: vec![],
                };
                self.handle(&req, chain)
            })
            .collect()
    }
}
