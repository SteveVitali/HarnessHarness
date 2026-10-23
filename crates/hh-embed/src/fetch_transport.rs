//! `fetch_transport` — the `https:` GET transport for `kernel.fetch`'s
//! remote `locations[]` members (R4.15d; OQ-332; DF-S4.2-1; the
//! GATE-G7-adopted §trust-fields set).
//!
//! The adopted field set is explicit: "the fetch itself is a
//! `net_egress` effect under Π mediation (`hharness-env` egress rules; an
//! unmediated fetch refuses typed)". Every remote GET therefore rides
//! the one [`EgressMediator`] — `security.egress.requested` →
//! attribution → `decide_egress` → `security.egress.decided` → wire →
//! charge — never a side-channel dial. The wire itself is the R4.5c
//! home: [`LocalTlsTransport`] (resolved-address dial +
//! `hharness_tls::ClientTls` handshake + the one `hharness_wire::http` codec —
//! ADR-0364 D1, ADR-0365).
//!
//! The containment half is *derived*, never invented: the declared
//! `FetchTrustPolicy` lowers to a `mediated`-mode `ContainmentPolicy`
//! whose allow rules are exactly `admitted_origins[]` (plus the
//! `signed_url` carrier's wildcard GET — the carrier's own signature +
//! `expires` arm is the credential; the containment layer does not
//! re-verify it). `default_unmatched = deny` holds the floor — a host
//! admission didn't admit answers typed `denied` at `decided`, and
//! `non_public_destinations = allow_listed` bounds SSRF pivots to the
//! declared allow rules (a hermetic loopback fixture must name its
//! host; an undeclared private answer still dies).
//!
//! `kernel.fetch` is a kernel-side act, not a tool call — the leg's
//! attribution mints under `env_handle = "kernel"` /
//! `effect_id = "bundle-fetch"` (the same spelling R4.5c's
//! `dispatch_revocations` kernel egress leg carries).
//!
//! The trust half lives in `hh-bundle::trust` (`FetchTrustPolicy`,
//! `admit`) — this file is the wire only: a refused admission never
//! reaches it, and every `MediatedOutcome` maps to a typed error the
//! `fetch` fold records on the `bundle.member.fetched` row — never a
//! fabricated body.

use hharness_containment::egress::{ApprovalCache, EgressRequest};
use hharness_containment::policy::{
    kernel_default, ContainmentPolicy, DefaultUnmatched, EgressProtocol, EgressRule, HostPattern,
    NetMode, NonPublic, RuleDecision,
};
use hharness_env::egress::{
    EgressMediator, EgressResolver, LocalTlsTransport, MediatedOutcome, SystemResolver,
};
use hharness_env::events::ScopeChain;
use hharness_env::tokens::TokenMinter;
use hharness_ledger::store::{Lease, Store};
use hharness_secrets::CredentialBroker;

/// The producer component the fetch leg's audit attributes to.
const COMPONENT: &str = "hh-embed.fetch";

/// `FetchTransport` — the remote half of `kernel.fetch`'s fetcher
/// composition: it builds the [`FetchLeg`] (mediator + attribution +
/// scope spellings) one fetch call rides.
pub struct FetchTransport {
    /// The client TLS (webpki roots plus any declared extra roots —
    /// verification is always on; a private issuer needs the explicit
    /// PEM file, never an insecure skip).
    tls: hharness_tls::ClientTls,
    /// The resolver the mediator installs (`SystemResolver` for the
    /// live leg; a pinned table for hermetic fixtures).
    resolver: Box<dyn EgressResolver>,
    /// The attribution minter — one token per fetch call, bound
    /// `("bundle-fetch", attempt, "kernel")`.
    minter: TokenMinter,
}

impl Default for FetchTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl FetchTransport {
    /// `new()` — the public-Internet transport (Mozilla roots, system
    /// DNS, the mediator's defaults).
    pub fn new() -> FetchTransport {
        FetchTransport {
            tls: hharness_tls::ClientTls::webpki_roots(),
            resolver: Box::new(SystemResolver),
            // A per-service seed — the token is an opaque run-local
            // spelling; determinism here is a property of the seed +
            // counter, not a security boundary.
            minter: TokenMinter::new(*b"hh-embed-fetch-transport-seed-01"),
        }
    }

    /// `with_tls(tls)` — a configured client TLS (the fetch site's
    /// `tls_extra_roots_file` hook).
    pub fn with_tls(tls: hharness_tls::ClientTls) -> FetchTransport {
        FetchTransport {
            tls,
            ..FetchTransport::new()
        }
    }

    /// `with_resolver(tls, resolver)` — the hermetic arm: a pinned
    /// `host → addrs` table (loopback fixtures) under the configured
    /// TLS.
    pub fn with_resolver(
        tls: hharness_tls::ClientTls,
        resolver: Box<dyn EgressResolver>,
    ) -> FetchTransport {
        FetchTransport {
            tls,
            resolver,
            ..FetchTransport::new()
        }
    }

    /// The `mediated`-mode `ContainmentPolicy` the declared
    /// `FetchTrustPolicy` lowers to: one `allow` rule per
    /// `admitted_origins[]` entry (GET only — fetch is read-only), the
    /// `signed_url` carrier's wildcard GET rule when the class is
    /// declared (its signature + expiry is the admitted credential),
    /// `default_unmatched = deny`, `non_public_destinations =
    /// allow_listed` (SSRF pivots bound to the declared rules), TLS
    /// termination left to the transport (always-on verification).
    fn egress_policy(trust: &hharness_bundle::FetchTrustPolicy, at: u64) -> ContainmentPolicy {
        let mut p = kernel_default(at);
        p.net.mode = NetMode::Mediated;
        p.net.default_unmatched = DefaultUnmatched::Deny;
        p.net.non_public_destinations = NonPublic::AllowListed;
        p.net.rules = trust
            .admitted_origins
            .iter()
            .filter_map(|o| {
                // An origin row is `host` or `host:port` — the rule's
                // `ports` member carries an explicit port; a bare host
                // leaves ports unconstrained (the GET itself pins 443).
                // The parse mirrors `origin_admitted`: `host:port`
                // splits only when the port parses — `host:junk` stays a
                // (never-matching) exact host, fail-closed rather than
                // port-unconstrained.
                let (host, port) = match o.rsplit_once(':') {
                    Some((h, prt))
                        if !h.is_empty()
                            && h.matches(':').count() == 0
                            && prt.parse::<u16>().is_ok() =>
                    {
                        (h.to_string(), prt.parse::<u16>().ok())
                    }
                    _ => (o.clone(), None),
                };
                Some(EgressRule {
                    host: HostPattern::parse(&host).ok()?,
                    ports: port.into_iter().collect(),
                    protocols: vec![EgressProtocol::HttpsConnect],
                    methods: vec!["GET".to_string()],
                    decision: RuleDecision::Allow,
                    credential_bindings: vec![],
                    justification: None,
                    provenance: None,
                })
            })
            .collect();
        if trust.admits(hharness_bundle::AdmissionClass::SignedUrl) {
            p.net.rules.push(EgressRule {
                host: HostPattern::Global,
                ports: vec![443],
                protocols: vec![EgressProtocol::HttpsConnect],
                methods: vec!["GET".to_string()],
                decision: RuleDecision::Allow,
                credential_bindings: vec![],
                justification: None,
                provenance: None,
            });
        }
        p.compute_ids();
        p
    }

    /// `leg(store, run, lease, broker, trust, at)` — the one fetch
    /// call's mediated leg: the derived policy, a fresh attribution
    /// token, the TLS transport and the resolver installed on a real
    /// [`EgressMediator`]. Every remote `locations[]` GET of the call
    /// rides `leg.get(url)` — `requested`/`decided` rows land on the
    /// fetch run durable before the wire fires.
    pub fn leg<'a>(
        &mut self,
        store: &'a mut Store,
        run_id: &str,
        lease: &'a Lease,
        broker: &'a mut CredentialBroker,
        trust: &hharness_bundle::FetchTrustPolicy,
        at: u64,
    ) -> FetchLeg<'a> {
        // The token binds the derived effect scope id — `intended`'s
        // `effect_id` validates against the same derivation on the run.
        let eid = Store::effect_id(run_id, "kernel-fetch-mc", "kernel-fetch-tc", 0);
        let token = self.minter.mint(&eid, 1, "kernel");
        let resolver = std::mem::replace(&mut self.resolver, Box::new(SystemResolver));
        let transport = Box::new(LocalTlsTransport {
            tls: self.tls.clone(),
            timeout: std::time::Duration::from_secs(15),
        });
        FetchLeg {
            token: token.token,
            chain: ScopeChain {
                // The kernel scope chain `ensure_fetch_run` opens on the
                // fetch run — ids are unique across kinds (the scope
                // table keys on the id itself).
                turn_id: "kernel-fetch-turn".to_string(),
                model_call_id: "kernel-fetch-mc".to_string(),
                tool_call_id: "kernel-fetch-tc".to_string(),
            },
            mediator: EgressMediator {
                store,
                run_id: run_id.to_string(),
                lease,
                tokens: self.minter.resolver(),
                broker,
                policy: Self::egress_policy(trust, at),
                cache: ApprovalCache::default(),
                budget_id: None,
                participant_ref: COMPONENT.to_string(),
                resolver,
                transport,
                inspect_hooks: std::collections::BTreeMap::new(),
            },
        }
    }
}

/// `FetchLeg` — one `kernel.fetch` call's mediated wire: the mediator,
/// its attribution token and the kernel scope spellings. `get(url)` is
/// the `LocationFetcher` the `fetch` fold drives.
pub struct FetchLeg<'a> {
    /// The mediated wire (durable `security.egress.*` rows on
    /// `run_id` before any outcome).
    pub mediator: EgressMediator<'a>,
    /// The minted attribution token (`token_hash` lands on the
    /// `decision_request` — the capability itself stays out of the row).
    pub token: String,
    /// The kernel scope spellings the rows carry.
    pub chain: ScopeChain,
}

impl FetchLeg<'_> {
    /// `get(url)` — one mediated `https:` GET → the body bytes. Every
    /// outcome maps to a typed error string the `fetch` fold records
    /// (`error:`/`refused:` on the `bundle.member.fetched` row): a
    /// non-`https` URL is refused here too (defense in depth —
    /// `admit` never passes one through); a deny/ask/credential
    /// refusal is typed; a non-2xx answer is `http_<status>`.
    pub fn get(&mut self, url: &str) -> Result<Vec<u8>, String> {
        let parsed =
            hharness_wire::http::parse_http_url(url).map_err(|e| format!("bad_url:{e}"))?;
        if parsed.scheme != hharness_wire::http::Scheme::Https {
            return Err("scheme_refused:fetch_transport_is_https_only".to_string());
        }
        let req = EgressRequest {
            token: self.token.clone(),
            effect_id: None,
            tool_call_id: "bundle-fetch".to_string(),
            env_handle: "kernel".to_string(),
            // The `https:` leg spells `https_connect` — the mediator's
            // TLS-terminated leg (R4.5c's `dispatch_revocations`
            // spelling).
            protocol: EgressProtocol::HttpsConnect,
            host_raw: parsed.host.clone(),
            resolved_addrs: vec![],
            port: parsed.port,
            method: Some("GET".to_string()),
            path: Some(parsed.target.clone()),
            headers: vec![(
                "user-agent".to_string(),
                "hh-fetch/1 (HarnessHarness)".to_string(),
            )],
            body: None,
            body_readers: None,
            credential_sentinels: vec![],
        };
        match self
            .mediator
            .handle(&req, &self.chain)
            .map_err(|e| format!("mediator:{e:?}"))?
        {
            MediatedOutcome::Forwarded { response, .. } => {
                if !(200..300).contains(&response.status) {
                    return Err(format!("http_{}", response.status));
                }
                Ok(response.body)
            }
            MediatedOutcome::Refused { reason, .. } => {
                Err(format!("egress_denied:{}", reason.as_str()))
            }
            MediatedOutcome::RefusedCredential { code, .. } => {
                Err(format!("credential_denied:{code:?}"))
            }
            MediatedOutcome::Asked { .. } => {
                // The fetch site never asks — `default_unmatched` is
                // deny; an ask surfacing here is a typed refusal, never
                // a pending wire.
                Err("egress_asked:fetch_never_asks".to_string())
            }
        }
    }
}
