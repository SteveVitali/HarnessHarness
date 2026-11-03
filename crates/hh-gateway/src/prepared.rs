//! `prepared` — the prepared gateway call: the retained-view→wire call
//! ordering (PADR-R5-2 B.1 item 5 / B.2 owner map "retained-view→wire |
//! gateway"; ticket R5.19; REQ-E-207 / REQ-E-006 / REQ-D-006
//! contributing).
//!
//! One `PreparedCall` runs the fixed order over the landed owners —
//! **retained view → existing serialization + primary bounds →
//! whole-vector Account reservation → sealed provider plan → durable
//! request → deadline-bounded dispatch → durable actual/unknown usage →
//! charge/release → guard** (the guard leg is the driver's existing
//! envelope guard downstream; this unit produces the durable facts it
//! consumes). Nothing here re-implements a codec, a store, an attempt
//! loop, or an accounting fold: serialization is the canonical JSON the
//! dialect codec already owns, the reservation/charge/release legs call
//! `hharness_budget::Account` (R5.12 whole-vector admission; R5.13
//! physical-use fold), the deadline is the run's persisted
//! `created_at_ms` origin (R5.10) plus the budget path's tightest
//! `time.wall_ms` hard cap — the same durable inputs on every read, so
//! resume, approval waits and committed drain all enforce the identical
//! deadline, never session arithmetic.
//!
//! Refusals are typed, never silent: an unbounded or unfunded call
//! dispatches zero bytes; a presented plan that no longer serializes to
//! the sealed request refuses `PreparedBytesMutated`; a transport
//! `timeout{…}` stays `timeout{…}` — a socket timeout is never re-spelled
//! `cancelled` nor read as remote cancellation; a terminal without usage
//! is unknown exposure (the `model.call.failed`/`completed` rows the
//! R5.13 fold reads), never zero.

use std::collections::BTreeMap;

use hharness_budget::account::{Account, ChargeRequest};
use hharness_budget::attribution::Attribution;
use hharness_budget::errors::BudgetError;
use hharness_budget::quantity::{ResourceQuantity, ResourceVector};
use hharness_budget::{DimensionId, DimensionKey};
use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::manifest::EventRef;
use hharness_ledger::store::{Lease, Store};
use hharness_provenance::ProvenanceRecord;
use hharness_telemetry::TokenVector;
use hharness_wire::json::Json;

use crate::attempts::{run_attempts, CollectSink, Sleeper};
use crate::codec::{self, WireFrame};
use crate::dialect::WireDialect;
use crate::events;
use crate::gateway::{CredentialHandle, CredentialPort, EndpointAllowlist, Transport};
use crate::grammar::{self, DecodeState, ModelEventKind};
use crate::message::ModelMessage;
use crate::plan::{InferenceRequest, ModelRef, ProviderRequestPlan, RequestClass};
use crate::vocab::{AttemptPolicy, ModelError, ModelErrorClass, Purpose, RetryClass};

/// The kernel component spelling on this module's rows — same convention
/// as `hh-budget/1` (`Producer::kernel`/`ProvenanceRecord::kernel`).
pub const COMPONENT: &str = "hh-gateway/1";

/// The primary token roles + call/overhead counters a bound vector may
/// name — the admitted categories of REQ-E-207's cap match
/// (`tokens.input.total`/`tokens.output.total` stay derived, never
/// charged; `model_calls`/`retries`/`time.wall_ms` are the call and
/// attempt overheads). Every *required* role must carry a bound — an
/// absent bound is an unbounded call and dispatches zero.
const REQUIRED_BOUND_DIMS: [DimensionId; 6] = [
    DimensionId::TokensInputUncached,
    DimensionId::TokensInputCacheRead,
    DimensionId::TokensInputCacheWrite,
    DimensionId::TokensOutputVisible,
    DimensionId::TokensOutputReasoning,
    DimensionId::ModelCalls,
];
const OPTIONAL_BOUND_DIMS: [DimensionId; 2] = [DimensionId::Retries, DimensionId::TimeWallMs];

fn admitted(dim: DimensionId) -> bool {
    REQUIRED_BOUND_DIMS.contains(&dim) || OPTIONAL_BOUND_DIMS.contains(&dim)
}

/// What `prepare` needs — the call's own facts, all references, never
/// credentials or secret material.
pub struct PrepareInputs {
    /// `model_call_id` — the call's ledger scope and reservation holder.
    pub model_call_id: String,
    /// The open turn the call scopes under (`turn ⊃ model_call`; the
    /// driver owns the turn lifecycle — this unit names it).
    pub turn_id: String,
    /// The budget node the whole-vector reservation projects against.
    pub budget_id: String,
    /// The owned retained input — the `retained_selection/1` member the
    /// context owner minted (`RetainedSelection::to_json()`, consumed
    /// unchanged). `prepare` serializes it through the one canonical
    /// codec into `view_bytes`/`view_ref` — the "retained view →
    /// serialized bytes" durable fact.
    pub retained_view: Json,
    /// The provider plan the sealed request is built on.
    pub plan: ProviderRequestPlan,
    /// Bounds over every admitted input/output role and overhead
    /// (every `REQUIRED_BOUND_DIMS` member present;
    /// `OPTIONAL_BOUND_DIMS` admitted when declared; anything else
    /// refuses `UnadmittedBound`).
    pub bounds: ResourceVector,
    /// The `model_ref` stamped on `model.call.requested` and matched
    /// against the served model.
    pub model_ref: ModelRef,
    /// The `ModelRole` spelling the call serves.
    pub role: String,
    /// The call's purpose (`cache.purpose` member).
    pub purpose: Purpose,
    /// The binding coordinate the request row names.
    pub binding_ref: String,
    /// The credential *coordinate* (`env:NAME` or absent) — the bind
    /// resolves it through the credential port, never here.
    pub credential_ref: Option<String>,
    /// The binding's substitution clause (`Some(false)` ⇒ a served-model
    /// drift is a terminal `failed{served_model_mismatch}`).
    pub substitution_allowed: Option<bool>,
    /// The hosted/native stamp on emitted rows.
    pub participant_class: Option<String>,
    /// The reservation's declared hold horizon (ms).
    pub reservation_ttl_ms: u64,
}

/// The typed prepare refusal — every variant is a *pre-dispatch*
/// verdict: zero bytes, and no ledger rows beyond the ones the owners
/// themselves ledger (a refused `reserve` writes `reserved{refused}`).
#[derive(Debug)]
pub enum PrepareRefusal {
    /// A required admitted role carries no bound, or a bound is
    /// incoherent (`model_calls` ≥ 1 — a call dispatches at least once).
    Unbounded {
        /// The refused dimension spelling.
        dimension: &'static str,
        /// Why it refused.
        detail: String,
    },
    /// A bound names a dimension outside the admitted set.
    UnadmittedBound {
        /// The offending spelling.
        dimension: String,
    },
    /// The declared input-role bounds sit under the existing
    /// serialization's `estimate_tokens` floor — the reservation cannot
    /// honestly cover the serialized request.
    Underbounded {
        /// The estimate floor.
        needed: u64,
        /// The declared input-role total.
        declared: i64,
    },
    /// The whole-vector reservation refused (the `Account` typed error —
    /// `InsufficientBudget`/`UnknownBudget`/`DimensionNotBudgetable`;
    /// `reserved{outcome:refused}` already ledgered by the owner).
    Unfunded {
        /// The typed refusal detail.
        detail: String,
    },
    /// `codec::validate_plan` / endpoint admission refused the plan.
    InvalidPlan {
        /// The typed refusal detail.
        detail: String,
    },
    /// The credential coordinate refused at bind.
    CredentialUnavailable {
        /// The typed refusal detail.
        detail: String,
    },
    /// A `time.wall_ms` cap exists but the run's seq-0
    /// `lifecycle.run.created.created_at_ms` origin is absent — the
    /// deadline cannot be proven (never a fabricated origin).
    OriginMissing,
    /// `now < created_at_ms` — the recorded origin cannot prove the
    /// span (`n/a{untrustworthy_origin}` semantics: refuse, never
    /// backdate).
    UntrustworthyOrigin {
        /// The recorded origin.
        origin_ms: u64,
        /// The clock reading.
        now_ms: u64,
    },
    /// `created_at_ms + wall cap ≤ now` at prepare — expiry blocks new
    /// dispatch before any row lands.
    DeadlineExpired {
        /// The enforced deadline.
        deadline_ms: u64,
        /// The clock reading.
        now_ms: u64,
    },
    /// A ledger read failed.
    Ledger {
        /// The typed refusal detail.
        detail: String,
    },
}

/// One staged prepared call — produced by [`prepare`], dispatched by
/// [`PreparedCall::dispatch`]. Holds the serialized retained view, the
/// sealed request bytes and refs, the held reservation and the
/// created-origin deadline.
#[derive(Debug)]
pub struct PreparedCall {
    /// `model_call_id`.
    pub model_call_id: String,
    /// The open turn the call scopes under.
    pub turn_id: String,
    /// The charged budget node.
    pub budget_id: String,
    /// The serialized retained view bytes.
    pub view_bytes: Vec<u8>,
    /// The view's content address (`selected_view_ref`).
    pub view_ref: String,
    /// The sealed request bytes — exactly what reaches the transport.
    pub request_bytes: Vec<u8>,
    /// The request bytes' content address (`request_ref` /
    /// `dispatched_bytes_ref` — one object, one address).
    pub request_ref: String,
    /// `plan.content_id()` — the sealed provider plan ref.
    pub sealed_plan_ref: String,
    /// The held whole-vector reservation.
    pub reservation_id: String,
    /// `created_at_ms + tightest path `time.wall_ms` hard cap` —
    /// `None` when no wall cap is configured on the path.
    pub deadline_ms: Option<u64>,
    /// The estimate floor the bounds were checked against.
    pub estimate_count: u64,
    plan: ProviderRequestPlan,
    model_ref: ModelRef,
    role: String,
    purpose: Purpose,
    binding_ref: String,
    credential_ref: Option<String>,
    credential: CredentialHandle,
    substitution_allowed: Option<bool>,
    participant_class: Option<String>,
    normalizer_ref: String,
    dialect: WireDialect,
    /// Whether the durable `model.call.requested` already landed (the
    /// scope opens exactly once — retries reuse it).
    requested_emitted: bool,
    /// The terminal row landed — a second dispatch is `AlreadyTerminal`.
    terminal_landed: bool,
    /// The reservation was released (expiry drain or reconcile).
    released: bool,
}

/// The dispatch outcome — the durable rows it staged are named per
/// variant; every refusal is typed and dispatches zero.
#[derive(Debug)]
pub enum DispatchVerdict {
    /// `model.call.completed` landed; actual usage charged against the
    /// reservation (per-dimension excess released by `charge`).
    Completed {
        /// The terminal message.
        message: Box<ModelMessage>,
        /// The actual usage (`None` ⇒ `usage{available:false}` — the
        /// R5.13 fold's `usage_unavailable` unknown exposure).
        usage: Option<TokenVector>,
        /// Attempts run.
        attempts: u32,
    },
    /// `model.call.failed` landed with the provider's typed class —
    /// `timeout{…}` stays `timeout{…}` (never remote cancellation);
    /// known usage was still charged, absent usage stays unknown
    /// exposure.
    Failed {
        /// The typed error.
        error: ModelError,
        /// Whether actual usage was known and charged.
        usage_charged: bool,
        /// Attempts run.
        attempts: u32,
    },
    /// The created-origin deadline blocked or stopped the dispatch —
    /// before `requested` (zero call rows) or mid-call
    /// (`failed{cancelled}` — a genuine local cancellation, never a
    /// socket timeout). The outstanding claim is released — committed
    /// work drains to a known state; incurred use stays charged.
    DeadlineExpired {
        /// The enforced deadline (`created_at_ms + wall cap`).
        deadline_ms: u64,
        /// The clock reading that breached it.
        now_ms: u64,
    },
    /// The presented plan no longer serializes to the sealed request
    /// bytes — mutation between prepare and dispatch refuses.
    PreparedBytesMutated {
        /// The sealed request ref.
        sealed_ref: String,
        /// The observed serialization's ref.
        observed_ref: String,
    },
    /// The reservation is no longer outstanding at dispatch (released,
    /// lease-lost) — an unfunded call dispatches zero.
    MissingReservation {
        /// The detail.
        detail: String,
    },
    /// A terminal already landed for this `model_call_id` — the call's
    /// durable scope is closed; retries happen inside one dispatch.
    AlreadyTerminal,
    /// A durable append failed.
    Ledger {
        /// The detail.
        detail: String,
    },
}

/// `created_at_ms` — the seq-0 `lifecycle.run.created` origin stamp
/// (R5.10; `payload.created_at_ms.value` on the store's own clock).
fn created_origin_ms(account: &Account) -> Result<Option<u64>, PrepareRefusal> {
    let events = account
        .store
        .events(&account.run_id)
        .map_err(|e| PrepareRefusal::Ledger {
            detail: format!("events: {e}"),
        })?;
    Ok(events
        .iter()
        .find(|e| e.class == "lifecycle.run.created")
        .and_then(|e| e.payload.get("created_at_ms"))
        .and_then(|c| c.get("value"))
        .and_then(Json::as_int)
        .map(|v| v.max(0) as u64))
}

/// The tightest `time.wall_ms` hard cap on the budget path (leaf→root —
/// an ancestor cap constrains the same elapsed clock, so the minimum
/// wins); `None` when no wall cap is configured.
fn wall_cap_ms(account: &Account, budget_id: &str) -> Result<Option<u64>, PrepareRefusal> {
    let key = DimensionKey::Primary(DimensionId::TimeWallMs);
    let path = account
        .tree
        .path(budget_id)
        .map_err(|e| PrepareRefusal::Unfunded {
            detail: format!("budget path: {e}"),
        })?;
    let mut cap: Option<i64> = None;
    for id in path {
        if let Some(hard) = account.tree.nodes[&id].node.spec.hard(key) {
            cap = Some(cap.map_or(hard, |c| c.min(hard)));
        }
    }
    Ok(cap.map(|c| c.max(0) as u64))
}

/// The created-origin deadline — one function of durable inputs
/// (`created_at_ms`, the wall cap), identical at prepare, at dispatch,
/// after resume and during drain. `now` is the store's clock — the same
/// authority every row stamps with.
fn created_origin_deadline(
    account: &Account,
    budget_id: &str,
) -> Result<Option<u64>, PrepareRefusal> {
    let Some(cap) = wall_cap_ms(account, budget_id)? else {
        return Ok(None);
    };
    let Some(origin) = created_origin_ms(account)? else {
        return Err(PrepareRefusal::OriginMissing);
    };
    let now = account.store.now_ms();
    if now < origin {
        return Err(PrepareRefusal::UntrustworthyOrigin {
            origin_ms: origin,
            now_ms: now,
        });
    }
    Ok(Some(origin.saturating_add(cap)))
}

/// Mint one durable row on the run's fenced writer path (same shape as
/// `Account::mint` — the producer is this module's component).
fn mint(
    store: &Store,
    run_id: &str,
    class: &str,
    payload: Json,
    turn_id: &str,
    model_call_id: &str,
) -> Result<Event, DispatchVerdict> {
    Ok(Event {
        event_id: store.alloc_id("evt"),
        class: class.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel(COMPONENT),
        scope: Scope {
            turn_id: Some(turn_id.to_string()),
            model_call_id: Some(model_call_id.to_string()),
            ..Scope::default()
        },
        parent_event_id: store
            .head_event_id(run_id)
            .map_err(|e| DispatchVerdict::Ledger {
                detail: format!("head: {e}"),
            })?,
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel(COMPONENT, store.now_ms())),
        content_kind: None,
        payload,
    })
}

fn append(account: &mut Account, lease: &Lease, events: Vec<Event>) -> Result<(), DispatchVerdict> {
    account
        .store
        .append(&account.run_id, lease, events)
        .map(|_| ())
        .map_err(|e| DispatchVerdict::Ledger {
            detail: format!("append: {e}"),
        })
}

/// The deadline-stopped attempt's sentinel — a genuine local
/// cancellation (`cancelled` is the control class for "the run's own
/// authority stopped the call"; it is *not* a socket timeout and is
/// never applied to one).
fn deadline_error(deadline_ms: u64, now_ms: u64) -> ModelError {
    let mut e = ModelError::new(
        ModelErrorClass::Cancelled,
        format!("deadline_expired: now {now_ms} >= deadline {deadline_ms}"),
    );
    e.retry_class = RetryClass::Permanent;
    e
}

fn is_deadline_stop(e: &ModelError) -> bool {
    e.class == ModelErrorClass::Cancelled && e.message.starts_with("deadline_expired:")
}

/// `prepare` — legs 1–5 of the fixed order: serialize the owned retained
/// input → bound every admitted role/overhead → reserve the whole vector
/// → seal the provider plan → hold the durable request facts + the
/// created-origin deadline. Any refusal leaves zero call rows and zero
/// dispatched bytes; a refused reserve is already ledgered
/// `reserved{outcome:refused}` by the owning `Account`.
pub fn prepare(
    account: &mut Account,
    lease: &Lease,
    dialect: &WireDialect,
    endpoints: &EndpointAllowlist,
    credentials: &mut dyn CredentialPort,
    input: PrepareInputs,
) -> Result<PreparedCall, PrepareRefusal> {
    // ── 1. retained view → existing serialization ─────────────────────
    let view_bytes = input.retained_view.to_canonical_string().into_bytes();
    let view_ref = hharness_identity::idp::address(&view_bytes, "application/json").id();

    // ── 2. primary bounds — every admitted role/overhead bound ────────
    for (d, _) in input.bounds.iter() {
        if !admitted(d) {
            return Err(PrepareRefusal::UnadmittedBound {
                dimension: d.as_str().to_string(),
            });
        }
    }
    for d in REQUIRED_BOUND_DIMS {
        if !input.bounds.iter().any(|(k, _)| k == d) {
            return Err(PrepareRefusal::Unbounded {
                dimension: d.as_str(),
                detail: "no bound declared".to_string(),
            });
        }
        if input.bounds.get(d) < 0 {
            return Err(PrepareRefusal::Unbounded {
                dimension: d.as_str(),
                detail: format!("negative bound {}", input.bounds.get(d)),
            });
        }
    }
    if input.bounds.get(DimensionId::ModelCalls) < 1 {
        return Err(PrepareRefusal::Unbounded {
            dimension: "model_calls",
            detail: "a call dispatches at least once (bound ≥ 1)".to_string(),
        });
    }
    // The existing serialization's estimate is the input floor — the
    // declared input roles must cover it (conservative bound).
    let estimate = codec::estimate_tokens(dialect, &input.plan);
    let declared_input: i64 = [
        DimensionId::TokensInputUncached,
        DimensionId::TokensInputCacheRead,
        DimensionId::TokensInputCacheWrite,
    ]
    .iter()
    .map(|d| input.bounds.get(*d))
    .sum();
    if (declared_input as u64) < estimate.count {
        return Err(PrepareRefusal::Underbounded {
            needed: estimate.count,
            declared: declared_input,
        });
    }

    // ── created-origin deadline (recomputed, never session arithmetic) ─
    let deadline_ms = created_origin_deadline(account, &input.budget_id)?;
    if let Some(d) = deadline_ms {
        let now = account.store.now_ms();
        if now >= d {
            return Err(PrepareRefusal::DeadlineExpired {
                deadline_ms: d,
                now_ms: now,
            });
        }
    }

    // ── 3. whole-vector Account reservation ───────────────────────────
    let reservation_id = account
        .reserve(
            lease,
            &input.budget_id,
            &input.bounds,
            &input.model_call_id,
            input.reservation_ttl_ms,
        )
        .map_err(|e: BudgetError| PrepareRefusal::Unfunded {
            detail: e.to_string(),
        })?;

    // ── 4. sealed provider plan → sealed request bytes ────────────────
    codec::validate_plan(dialect, &input.plan).map_err(|e| PrepareRefusal::InvalidPlan {
        detail: format!("plan: {e}"),
    })?;
    endpoints
        .admit(&input.plan.endpoint_ref)
        .map_err(|e| PrepareRefusal::InvalidPlan {
            detail: format!("{e}"),
        })?;
    let credential = credentials
        .bind(
            input.credential_ref.as_deref(),
            &input.plan.endpoint_ref,
            &dialect.auth_kinds,
        )
        .map_err(|e| PrepareRefusal::CredentialUnavailable {
            detail: format!("{e}"),
        })?;
    // The reservation rides inside the sealed plan — the gateway's
    // reserve-before-spend check reads `budget_reservation_id` off the
    // plan itself.
    let mut plan = input.plan.clone();
    plan.budget_reservation_id = Some(reservation_id.clone());
    let request_bytes = codec::serialize(&plan);
    let request_ref = hharness_identity::idp::address(&request_bytes, "application/json").id();
    let sealed_plan_ref = plan.content_id();

    Ok(PreparedCall {
        model_call_id: input.model_call_id,
        turn_id: input.turn_id,
        budget_id: input.budget_id,
        view_bytes,
        view_ref,
        request_bytes,
        request_ref,
        sealed_plan_ref,
        reservation_id,
        deadline_ms,
        estimate_count: estimate.count,
        plan,
        model_ref: input.model_ref,
        role: input.role,
        purpose: input.purpose,
        binding_ref: input.binding_ref,
        credential_ref: input.credential_ref,
        credential,
        substitution_allowed: input.substitution_allowed,
        participant_class: input.participant_class,
        normalizer_ref: dialect.dialect_id.clone(),
        dialect: dialect.clone(),
        requested_emitted: false,
        terminal_landed: false,
        released: false,
    })
}

impl PreparedCall {
    /// Release the reservation's remaining claim once — the committed
    /// drain on expiry (`released{reason:release}`; incurred charges
    /// stay — a release never refunds physical use).
    fn drain_reservation(&mut self, account: &mut Account, lease: &Lease) {
        if self.released {
            return;
        }
        self.released = true;
        // A release under an expired/fenced lease is a typed skip — the
        // hold dies with the holder's lease (B.5); the verdict stands.
        let _ = account.release(lease, &self.reservation_id);
    }

    /// `dispatch` — legs 5–8 of the fixed order: deadline re-check →
    /// sealed-byte verify → durable `model.call.requested` (first
    /// dispatch only) → deadline-bounded attempts over the *prepared
    /// bytes* → durable actual/unknown usage → charge/release through
    /// the owning `Account`.
    pub fn dispatch(
        &mut self,
        account: &mut Account,
        lease: &Lease,
        presented_plan: &ProviderRequestPlan,
        transport: &mut dyn Transport,
        sleeper: &mut dyn Sleeper,
        attempt_policy: &AttemptPolicy,
    ) -> DispatchVerdict {
        if self.terminal_landed {
            return DispatchVerdict::AlreadyTerminal;
        }
        // The same created-origin deadline — recomputed on durable
        // inputs; resume/approval delay cannot move it.
        if let Some(d) = self.deadline_ms {
            let now = account.store.now_ms();
            if now >= d {
                self.drain_reservation(account, lease);
                return DispatchVerdict::DeadlineExpired {
                    deadline_ms: d,
                    now_ms: now,
                };
            }
        }
        // The presented plan must serialize to exactly the sealed
        // request — mutated prepared bytes refuse before the wire.
        let observed = codec::serialize(presented_plan);
        if observed != self.request_bytes {
            return DispatchVerdict::PreparedBytesMutated {
                sealed_ref: self.request_ref.clone(),
                observed_ref: hharness_identity::idp::address(&observed, "application/json").id(),
            };
        }
        // An unfunded call dispatches zero — the reservation must still
        // be outstanding (released/lease-lost holds fund nothing).
        let funded = account
            .tree
            .reservation(&self.reservation_id)
            .map(|r| r.outstanding)
            .unwrap_or(false);
        if !funded {
            return DispatchVerdict::MissingReservation {
                detail: format!("reservation {} not outstanding", self.reservation_id),
            };
        }
        // ── durable request (the scope opens once — retries reuse it) ──
        if !self.requested_emitted {
            let req = InferenceRequest {
                model_call_id: self.model_call_id.clone(),
                model_ref: self.model_ref.clone(),
                plan: self.plan.clone(),
                role: self.role.clone(),
                purpose: self.purpose.clone(),
                context_label: None,
                view_hash: Some(self.view_ref.clone()),
                binding_ref: self.binding_ref.clone(),
                credential_ref: self.credential_ref.clone(),
                budget_reservation_id: Some(self.reservation_id.clone()),
                attempt_policy: attempt_policy.clone(),
                deadline_ms: self.deadline_ms,
                cache_state_hint: crate::cache::ExpectedState::Unknown,
                seed_request: None,
                observability: crate::plan::CallObservability {
                    capture_request: true,
                    capture_response: true,
                },
                request_class: RequestClass::Interactive,
                deferred_deadline_ms: None,
                stream: true,
                substitution_allowed: self.substitution_allowed,
                participant_class: self.participant_class.clone(),
                identity: None,
            };
            let payload = events::call_requested(
                &req,
                &self.dialect.cache_semantics,
                self.credential.binding_id.as_deref(),
                Some(self.sealed_plan_ref.clone()),
                Some(self.view_ref.as_str()),
                Some(self.estimate_count),
                Some(self.request_ref.as_str()),
            );
            let ev = match mint(
                account.store,
                &account.run_id,
                "model.call.requested",
                payload,
                &self.turn_id,
                &self.model_call_id,
            ) {
                Ok(e) => e,
                Err(e) => return e,
            };
            if let Err(e) = append(account, lease, vec![ev]) {
                return e;
            }
            self.requested_emitted = true;
        }
        // ── deadline-bounded dispatch — the prepared bytes only ────────
        let deadline = self.deadline_ms;
        let bytes = self.request_bytes.clone();
        let credential = self.credential.clone();
        let endpoint = self.plan.endpoint_ref.clone();
        let mc = self.model_call_id.clone();
        let sent_at = std::cell::Cell::new(0u64);
        let mut sink = CollectSink::default();
        let now_fn = || account.store.now_ms();
        let outcome = match run_attempts(
            attempt_policy,
            &mc,
            &now_fn,
            &mut sink,
            sleeper,
            |_attempt_no| {
                if let Some(d) = deadline {
                    let now = now_fn();
                    if now >= d {
                        return Err(deadline_error(d, now));
                    }
                }
                sent_at.set(now_fn());
                transport.send(&bytes, &credential, &endpoint)
            },
        ) {
            Ok(o) => o,
            Err(detail) => return DispatchVerdict::Ledger { detail },
        };
        // The attempt/delta rows drain durable in emission order before
        // the terminal row (the `ModelGateway::stream` precedent — the
        // interleaved cell preserves row order, the writer appends).
        {
            let drained: Vec<(String, Json)> = sink.events.drain(..).collect();
            let mut rows = Vec::new();
            for (class, payload) in drained {
                match mint(
                    account.store,
                    &account.run_id,
                    &class,
                    payload,
                    &self.turn_id,
                    &mc,
                ) {
                    Ok(e) => rows.push(e),
                    Err(e) => return e,
                }
            }
            if let Err(e) = append(account, lease, rows) {
                return e;
            }
        }
        // ── terminal row — durable actual/unknown usage ───────────────
        let duration = account.store.now_ms().saturating_sub(sent_at.get());
        let timing = crate::grammar::Timing {
            latency_ms: duration,
            attempts: outcome.attempts,
            ttft_ms: None,
            queue_wait_ms: outcome.delays_ms.iter().sum(),
            measured_at: "adapter".to_string(),
        };
        match outcome.result {
            Ok(frames) => self.finish_frames(account, lease, frames, &timing),
            Err(err) => {
                if is_deadline_stop(&err) {
                    // Mid-call expiry: the durable terminal records a
                    // genuine local cancellation (never a socket
                    // timeout); the outstanding claim drains.
                    let f_payload = events::call_failed(
                        &mc,
                        &err,
                        None,
                        &timing,
                        self.credential.binding_id.as_deref(),
                    );
                    let ev = match mint(
                        account.store,
                        &account.run_id,
                        "model.call.failed",
                        f_payload,
                        &self.turn_id,
                        &mc,
                    ) {
                        Ok(e) => e,
                        Err(e) => return e,
                    };
                    if let Err(e) = append(account, lease, vec![ev]) {
                        return e;
                    }
                    self.terminal_landed = true;
                    self.drain_reservation(account, lease);
                    return DispatchVerdict::DeadlineExpired {
                        deadline_ms: deadline.unwrap_or(0),
                        now_ms: account.store.now_ms(),
                    };
                }
                self.finish_failed(account, lease, err, None, &timing)
            }
        }
    }

    /// The completed/failed frame leg — decode through the grammar,
    /// land the durable terminal, reconcile the reservation.
    fn finish_frames(
        &mut self,
        account: &mut Account,
        lease: &Lease,
        frames: Vec<(u64, WireFrame)>,
        timing: &crate::grammar::Timing,
    ) -> DispatchVerdict {
        let mc = self.model_call_id.clone();
        let mut state = DecodeState {
            model_call_id: mc.clone(),
            attempt_no: timing.attempts.max(1),
            ..DecodeState::default()
        };
        let mut events_out: Vec<crate::grammar::ModelEvent> = Vec::new();
        for (at, f) in &frames {
            for ev in grammar::decode_frame(&self.dialect, f, &mut state, *at, &self.normalizer_ref)
            {
                events_out.push(ev);
            }
        }
        let terminal = events_out.iter().rev().find_map(|e| match &e.kind {
            k @ (ModelEventKind::Completed { .. } | ModelEventKind::Failed { .. }) => Some(k),
            _ => None,
        });
        match terminal {
            Some(ModelEventKind::Completed { message, usage, .. }) => {
                // Served-model drift gate — identical semantics to the
                // live binding's substitution rule.
                if let (Some(served), Some(false)) =
                    (message.served_model.as_deref(), self.substitution_allowed)
                {
                    if served != self.model_ref.provider_model_id {
                        let err = ModelError::new(
                            ModelErrorClass::ServedModelMismatch,
                            format!(
                                "served `{served}` != bound `{}` (substitution_allowed=false)",
                                self.model_ref.provider_model_id
                            ),
                        );
                        return self.finish_failed(
                            account,
                            lease,
                            err,
                            usage.clone().or_else(|| state.final_usage.clone()),
                            timing,
                        );
                    }
                }
                let payload = events::call_completed(
                    &mc,
                    &**message,
                    usage.as_ref(),
                    state.raw_usage.as_ref(),
                    Some("final_frame"),
                    None,
                    None,
                    timing,
                    Some(&self.plan.cache),
                    None,
                    self.credential.binding_id.as_deref(),
                );
                let ev = match mint(
                    account.store,
                    &account.run_id,
                    "model.call.completed",
                    payload,
                    &self.turn_id,
                    &mc,
                ) {
                    Ok(e) => e,
                    Err(e) => return e,
                };
                let event_id = ev.event_id.clone();
                if let Err(e) = append(account, lease, vec![ev]) {
                    return e;
                }
                self.terminal_landed = true;
                let usage = usage.clone();
                self.reconcile(account, lease, &event_id, usage.as_ref(), timing.attempts);
                DispatchVerdict::Completed {
                    message: message.clone(),
                    usage,
                    attempts: timing.attempts,
                }
            }
            Some(ModelEventKind::Failed { error, usage, .. }) => self.finish_failed(
                account,
                lease,
                error.clone(),
                usage.clone().or_else(|| state.final_usage.clone()),
                timing,
            ),
            _ => self.finish_failed(
                account,
                lease,
                ModelError::new(
                    ModelErrorClass::StreamDecode,
                    "stream ended without a terminal event",
                ),
                None,
                timing,
            ),
        }
    }

    /// The failed-terminal leg — the typed class lands verbatim
    /// (`timeout{…}` stays `timeout{…}`); known usage still charges,
    /// absent usage leaves unknown exposure (the fold's
    /// `ambiguous_failure`), never zero.
    fn finish_failed(
        &mut self,
        account: &mut Account,
        lease: &Lease,
        error: ModelError,
        usage: Option<TokenVector>,
        timing: &crate::grammar::Timing,
    ) -> DispatchVerdict {
        let payload = events::call_failed(
            &self.model_call_id,
            &error,
            usage.as_ref(),
            timing,
            self.credential.binding_id.as_deref(),
        );
        let ev = match mint(
            account.store,
            &account.run_id,
            "model.call.failed",
            payload,
            &self.turn_id,
            &self.model_call_id,
        ) {
            Ok(e) => e,
            Err(e) => return e,
        };
        let event_id = ev.event_id.clone();
        if let Err(e) = append(account, lease, vec![ev]) {
            return e;
        }
        self.terminal_landed = true;
        let charged = usage.is_some();
        self.reconcile(account, lease, &event_id, usage.as_ref(), timing.attempts);
        DispatchVerdict::Failed {
            error,
            usage_charged: charged,
            attempts: timing.attempts,
        }
    }

    /// `charge/release` — reconcile the durable actual usage through
    /// the owning `Account`: every admitted primary category the call
    /// actually used, one charge per `(source_event, dimension)` (the
    /// landed dedup makes a replayed reconcile a no-op), the
    /// reservation consumed by `reservation_id`, the remainder
    /// released. Absent usage charges nothing — unknown exposure stays
    /// on the durable rows (never zero).
    fn reconcile(
        &mut self,
        account: &mut Account,
        lease: &Lease,
        terminal_event_id: &str,
        usage: Option<&TokenVector>,
        attempts: u32,
    ) {
        let source = EventRef {
            run_id: account.run_id.clone(),
            event_id: terminal_event_id.to_string(),
        };
        let mut vector = usage
            .map(|u| u.to_resource_vector())
            .unwrap_or_else(ResourceVector::zero);
        // The call itself is physical use — the dispatched call count is
        // known even when token usage is not.
        vector.add(DimensionId::ModelCalls, 1);
        if attempts > 1 {
            vector.add(DimensionId::Retries, (attempts - 1) as i64);
        }
        for (dim, amount) in vector.iter() {
            if amount <= 0 {
                continue;
            }
            let req = ChargeRequest {
                budget_id: self.budget_id.clone(),
                quantity: ResourceQuantity {
                    dimension: dim,
                    amount,
                    unit: dim.unit().to_string(),
                    model_ref: Some(self.model_ref.pricing_key()),
                    measured_at: source.clone(),
                },
                source: source.clone(),
                attribution: Attribution::subject(
                    account.run_id.clone(),
                    self.budget_id.clone(),
                    "participant:main",
                ),
                reservation_id: Some(self.reservation_id.clone()),
                cache_ttl: None,
            };
            // A charge failure is a typed skip — the durable terminal row
            // and the physical-use fold keep the exposure; the call never
            // silently re-charges (the `(source, dim)` dedup is landed).
            let _ = account.charge(lease, &req);
        }
        // Release the unclaimed remainder — `charge` already released
        // per-dim excess; `release` frees what no charge touched. An
        // already-consumed/released reservation is not an error.
        if account
            .tree
            .reservation(&self.reservation_id)
            .map(|r| r.outstanding)
            .unwrap_or(false)
        {
            let _ = account.release(lease, &self.reservation_id);
            self.released = true;
        }
    }
}
