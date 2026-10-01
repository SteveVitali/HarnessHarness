//! The `ModelGateway` (§5b.1 §2; ADR-0118 d.2) — a boundary component,
//! `home = model_boundary`. Given a sealed `ProviderRequestPlan` plus its
//! `InferenceRequest` control envelope, it validates the plan, binds the
//! credential, admits the endpoint, requires the budget reservation — then
//! dispatch happens. An `InferenceRequest` never touches a wire; the gateway
//! never branches on a model identifier.
//!
//! The gateway is the **sole emitter** of usage, cost provenance, attempt
//! timing, served-model drift and transport capability facts — it produces the
//! `model.*` payloads ([`crate::events`]); the kernel's `Store::append` is the
//! durable writer. Transport, credentials and the clock are ports — the C0
//! in-process wiring and the recording/fake out-of-process gateway both
//! satisfy them (AC-R-2.3.1-11).

use hh_wire::json::Json;

use crate::attempts::{run_attempts, CollectSink, EventSink, NoSleep, SharedSink};
use crate::codec::{self, WireFrame};
use crate::dialect::WireDialect;
use crate::errors::GatewayError;
use crate::events;
use crate::grammar::{decode_frame, DecodeState, ModelEventKind};
use crate::message::ModelMessage;
use crate::plan::{InferenceRequest, ProviderRequestPlan};
use crate::vocab::{ModelError, ModelErrorClass};

/// The transport port — the wire boundary. `send` takes the serialized plan
/// bytes and the credential **handle** (never the secret); it returns the
/// wire frames in order (a streaming dialect's frames interleave with
/// arrival times for the G7 idle check).
pub trait Transport {
    /// `send(bytes, credential, endpoint_ref) -> frames` — one attempt's
    /// frames with arrival ms (the C0 fake returns a fixed fixture; a live
    /// transport is kernel-side wiring).
    fn send(
        &mut self,
        bytes: &[u8],
        credential: &CredentialHandle,
        endpoint_ref: &str,
    ) -> Result<Vec<(u64, WireFrame)>, ModelError>;

    /// `discover(endpoint_ref)` — the `model_listing` probe (the endpoint's
    /// declared listing member; Stage-3 probing is deferred — C0 reads the
    /// member).
    fn discover(&mut self, _endpoint_ref: &str) -> Result<Json, ModelError> {
        Err(ModelError::new(
            ModelErrorClass::UnsupportedFeature,
            "transport has no discover primitive",
        ))
    }

    /// `probe_capability(capability, endpoint_ref)` — the §5b.3 transport
    /// probe: a request that elicits the named claim. The returned Json is
    /// the endpoint's *observed* value for the capability
    /// (`run_transport_probes` folds it against the pin). The default
    /// `UnsupportedFeature` means "no probe primitive" — the conformance
    /// row's observed is `skipped`, never fabricated.
    fn probe_capability(
        &mut self,
        _capability: &str,
        _endpoint_ref: &str,
    ) -> Result<Json, ModelError> {
        Err(ModelError::new(
            ModelErrorClass::UnsupportedFeature,
            "transport has no probe primitive",
        ))
    }

    /// `cancel(call_id)` — a best-effort cancel signal.
    fn cancel(&mut self, _call_id: &str) {}
}

/// A credential **handle** — `{binding_id, provided}`, never secret material
/// (the kernel-held broker resolves the value; the gateway moves the handle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialHandle {
    /// The broker binding id.
    pub binding_id: Option<String>,
    /// `provided: yes/no` — the only secret fact the ledger may carry.
    pub provided: bool,
}

/// The credential port — the kernel-held broker's seam (the gateway asks for
/// a binding; the broker's resolver does the resolution).
pub trait CredentialPort {
    /// `bind(credential_ref, endpoint_ref, auth) -> handle` — refused when the
    /// broker has no live binding (`CredentialUnavailable`).
    fn bind(
        &mut self,
        credential_ref: Option<&str>,
        endpoint_ref: &str,
        auth: &[crate::dialect::AuthKind],
    ) -> Result<CredentialHandle, GatewayError>;
}

/// A `CredentialPort` for the no-credential dialect (`auth_kinds = [none]`) —
/// `provided = false` is a fact, not a failure.
#[derive(Debug, Default)]
pub struct NoCredentials;

impl CredentialPort for NoCredentials {
    fn bind(
        &mut self,
        credential_ref: Option<&str>,
        _endpoint_ref: &str,
        auth: &[crate::dialect::AuthKind],
    ) -> Result<CredentialHandle, GatewayError> {
        if credential_ref.is_some() && auth.iter().any(|a| *a != crate::dialect::AuthKind::None) {
            // A credential was supplied but the port has no resolver — the
            // kernel wires a real broker; refuse rather than drop it.
            return Err(GatewayError::CredentialUnavailable {
                detail: "credential_ref supplied to a resolver-less port".into(),
            });
        }
        Ok(CredentialHandle {
            binding_id: credential_ref.map(str::to_string),
            provided: credential_ref.is_some(),
        })
    }
}

/// `EndpointAllowlist{policy_ref, endpoints}` — the H4 admission set (the
/// `endpoint_allowlist_ref` on the dialect names the policy row; the set is
/// the admitted endpoint coordinates). An endpoint outside the set is
/// `EndpointNotAllowed` — refused **before any bytes leave** (AC-R-2.3.1-13).
#[derive(Debug, Clone, Default)]
pub struct EndpointAllowlist {
    /// The policy row reference.
    pub policy_ref: String,
    /// The admitted endpoint coordinates.
    pub endpoints: std::collections::BTreeSet<String>,
}

impl EndpointAllowlist {
    /// `admit(endpoint_ref)` — the H4 check.
    pub fn admit(&self, endpoint_ref: &str) -> Result<(), GatewayError> {
        if self.endpoints.contains(endpoint_ref) {
            Ok(())
        } else {
            Err(GatewayError::EndpointNotAllowed {
                endpoint: endpoint_ref.to_string(),
            })
        }
    }
}

/// `CallHandle{call_id, plan, credential, opened_at_ms}` — the
/// `open_call` result; `stream`/`complete`/`cancel` take it.
#[derive(Debug)]
pub struct CallHandle {
    /// `model_call_id` — the call's ledger scope.
    pub call_id: String,
    /// The sealed plan.
    pub plan: ProviderRequestPlan,
    /// The control envelope (the `attempt_policy`, `purpose`, `role`).
    pub request: InferenceRequest,
    /// The credential handle (a reference — never a secret).
    pub credential: CredentialHandle,
    /// The dialect coordinate this call runs under.
    pub dialect_id: String,
    /// The dialect version.
    pub dialect_version: String,
    /// When the call opened (the duration accounting input).
    pub opened_at_ms: u64,
}

/// `ModelGateway` — the dialect-parameterized boundary. Holds the loaded
/// `WireDialect`s (keyed `{dialect_id}@{version}`), the endpoint allow-list,
/// and the ports.
pub struct ModelGateway<'a> {
    /// The loaded dialects (`"{id}@{version}"` keys).
    pub dialects: std::collections::BTreeMap<String, WireDialect>,
    /// The endpoint allow-list (H4).
    pub endpoints: EndpointAllowlist,
    /// The credential port (kernel-held broker).
    pub credentials: &'a mut dyn CredentialPort,
    /// The transport port.
    pub transport: &'a mut dyn Transport,
    /// The clock — `now_ms()` is a port (deterministic replay).
    pub now_ms: Box<dyn Fn() -> u64 + 'a>,
    /// The `normalizer_ref` stamped on usage vectors (the Model Profile's
    /// `usage_mapping` ref).
    pub normalizer_ref: String,
}

impl<'a> ModelGateway<'a> {
    /// The dialect key.
    fn key(id: &str, version: &str) -> String {
        format!("{id}@{version}")
    }

    /// `load_dialect(d)` — register a descriptor (the document is already
    /// validated — `WireDialect::from_json` refuses incomplete debt).
    pub fn load_dialect(&mut self, d: WireDialect) {
        self.dialects
            .insert(Self::key(&d.dialect_id, &d.version), d);
    }

    /// `open_call(request)` — validate → credential → endpoint →
    /// reservation → emit `model.call.requested` → `CallHandle` (§5b.1 §5).
    /// Every refusal is a typed `GatewayError`, before any bytes leave.
    pub fn open_call(
        &mut self,
        request: InferenceRequest,
        sink: &mut dyn EventSink,
    ) -> Result<CallHandle, GatewayError> {
        let plan = request.plan.clone();
        let dialect = self
            .dialects
            .get(&Self::key(&plan.dialect_id, &plan.dialect_version))
            .ok_or_else(|| GatewayError::UnknownDialect {
                dialect: Self::key(&plan.dialect_id, &plan.dialect_version),
            })?;
        codec::validate_plan(dialect, &plan)?;
        self.endpoints.admit(&plan.endpoint_ref)?;
        let credential = self.credentials.bind(
            request.credential_ref.as_deref(),
            &plan.endpoint_ref,
            &dialect.auth_kinds,
        )?;
        if request.budget_reservation_id.is_none() {
            return Err(GatewayError::MissingReservation {
                model_call_id: request.model_call_id.clone(),
            });
        }
        let plan_hash = plan.content_id();
        let estimate = codec::estimate_tokens(dialect, &plan);
        sink.emit(
            "model.call.requested",
            stamp_participant(
                events::call_requested(
                    &request,
                    &dialect.cache_semantics,
                    credential.binding_id.as_deref(),
                    Some(plan_hash),
                    None,
                    Some(estimate.count),
                    None,
                ),
                request.participant_class.as_deref(),
            ),
        )
        .map_err(|detail| GatewayError::Ledger { detail })?;
        Ok(CallHandle {
            call_id: request.model_call_id.clone(),
            plan,
            request,
            credential,
            dialect_id: dialect.dialect_id.clone(),
            dialect_version: dialect.version.clone(),
            opened_at_ms: (self.now_ms)(),
        })
    }

    /// `stream(handle, sink)` — run the attempt loop over the transport,
    /// decode frames through the grammar, emit `model.stream.delta`
    /// (ephemeral) + `model.call.attempt.*` + the terminal row. Returns the
    /// terminal `ModelEvent`s plus the outcome.
    pub fn stream(
        &mut self,
        handle: &CallHandle,
        sink: &mut dyn EventSink,
    ) -> Result<crate::attempts::AttemptOutcome<ModelMessage>, GatewayError> {
        let dialect = self
            .dialects
            .get(&Self::key(&handle.dialect_id, &handle.dialect_version))
            .ok_or_else(|| GatewayError::UnknownDialect {
                dialect: Self::key(&handle.dialect_id, &handle.dialect_version),
            })?
            .clone();
        let bytes = codec::serialize(&handle.plan);
        let policy = handle.request.attempt_policy.clone();
        let normalizer = self.normalizer_ref.clone();
        let mut sleeper = NoSleep::default();
        let transport = &mut *self.transport;
        let now_fn = &*self.now_ms;
        let credential = handle.credential.clone();
        let endpoint = handle.plan.endpoint_ref.clone();
        let model_call_id = handle.call_id.clone();
        let bytes_ref = bytes.clone();
        let mut state = DecodeState::default();
        let mut ttft_ms: Option<u64> = None;
        // The attempt loop and the delta emitter share one sink cell — the
        // interleaved order is the emitted order; drained to `sink` after.
        let cell = std::cell::RefCell::new(CollectSink::default());
        let outcome = run_attempts(
            &policy,
            &model_call_id,
            now_fn,
            &mut SharedSink { inner: &cell },
            &mut sleeper,
            |attempt_no| {
                state.begin_attempt(attempt_no);
                let sent_at = now_fn();
                let frames = transport.send(&bytes_ref, &credential, &endpoint)?;
                let mut message = None;
                let mut failure = None;
                for (at_ms, frame) in frames {
                    if ttft_ms.is_none() {
                        ttft_ms = Some(at_ms.saturating_sub(sent_at));
                    }
                    for ev in decode_frame(&dialect, &frame, &mut state, at_ms, &normalizer) {
                        // `model.stream.delta` — ephemeral stream facts.
                        if let ModelEventKind::BlockDelta { index, .. } = &ev.kind {
                            let _ = cell.borrow_mut().emit(
                                "model.stream.delta",
                                events::stream_delta(
                                    &model_call_id,
                                    attempt_no,
                                    ev.seq_in_attempt,
                                    Some(*index),
                                    "delta",
                                ),
                            );
                        }
                        match &ev.kind {
                            ModelEventKind::Completed {
                                message: m, usage, ..
                            } => {
                                if let Some(u) = usage {
                                    state.final_usage = Some(u.clone());
                                }
                                message = Some((**m).clone());
                            }
                            ModelEventKind::Failed { error, .. } => {
                                failure = Some(error.clone());
                            }
                            _ => {}
                        }
                    }
                }
                if let Some(e) = failure {
                    return Err(e);
                }
                message.ok_or_else(|| {
                    ModelError::new(
                        ModelErrorClass::InvalidResponse(
                            crate::vocab::InvalidResponseKind::NoStopReason,
                        ),
                        "stream ended without a terminal",
                    )
                })
            },
        )
        .map_err(|detail| GatewayError::Ledger { detail })?;
        // Drain the interleaved attempt/delta events into the caller's sink —
        // the emitted order is the cell's order. Every row carries the
        // participant stamp (AC-R-2.3.1-15 — a hosted call's *stream* rows are
        // hosted rows too, not just the request/terminal pair).
        for (class, payload) in cell.borrow_mut().events.drain(..) {
            sink.emit(
                &class,
                stamp_participant(payload, handle.request.participant_class.as_deref()),
            )
            .map_err(|detail| GatewayError::Ledger { detail })?;
        }
        // The terminal row — `model.call.completed` carries usage; `failed`
        // carries the classified error.
        let duration = (self.now_ms)().saturating_sub(handle.opened_at_ms);
        let timing = crate::grammar::Timing {
            latency_ms: duration,
            attempts: outcome.attempts,
            ttft_ms,
            queue_wait_ms: outcome.delays_ms.iter().sum(),
            measured_at: "adapter".to_string(),
        };
        match &outcome.result {
            Ok(message) => {
                // AC-R-2.3.1-9 — the substitution gate: `served_model ≠
                // provider_model_id` under `substitution_allowed = false` is a
                // terminal `failed{served_model_mismatch}`, never a silent
                // route. Allowed substitutions complete with the served-model
                // stamps (`served_model`, `substitution`) on `completed`.
                let served = message.served_model.as_deref();
                let requested = handle.request.model_ref.provider_model_id.as_str();
                if served.is_some()
                    && served != Some(requested)
                    && handle.request.substitution_allowed == Some(false)
                {
                    let error = ModelError::new(
                        ModelErrorClass::ServedModelMismatch,
                        format!(
                            "served {} but {} was requested and substitution is not allowed",
                            served.unwrap_or("<none>"),
                            requested
                        ),
                    );
                    sink.emit(
                        "model.call.failed",
                        stamp_participant(
                            events::call_failed(
                                &handle.call_id,
                                &error,
                                state.final_usage.as_ref(),
                                &timing,
                                handle.credential.binding_id.as_deref(),
                            ),
                            handle.request.participant_class.as_deref(),
                        ),
                    )
                    .map_err(|detail| GatewayError::Ledger { detail })?;
                    return Ok(crate::attempts::AttemptOutcome {
                        result: Err(error),
                        attempts: outcome.attempts,
                        delays_ms: outcome.delays_ms,
                    });
                }
                let observed = state
                    .final_usage
                    .as_ref()
                    .map(|u| crate::cache::observe_cache(Some(u)));
                sink.emit(
                    "model.call.completed",
                    stamp_participant(
                        events::call_completed(
                            &handle.call_id,
                            message,
                            state.final_usage.as_ref(),
                            state.raw_usage.as_ref(),
                            Some(dialect.usage_arrival.as_str()),
                            None,
                            None,
                            &timing,
                            Some(&handle.plan.cache),
                            observed.as_ref(),
                            handle.credential.binding_id.as_deref(),
                        ),
                        handle.request.participant_class.as_deref(),
                    ),
                )
                .map_err(|detail| GatewayError::Ledger { detail })?;
            }
            Err(error) => {
                sink.emit(
                    "model.call.failed",
                    stamp_participant(
                        events::call_failed(
                            &handle.call_id,
                            error,
                            state.final_usage.as_ref(),
                            &timing,
                            handle.credential.binding_id.as_deref(),
                        ),
                        handle.request.participant_class.as_deref(),
                    ),
                )
                .map_err(|detail| GatewayError::Ledger { detail })?;
            }
        }
        Ok(outcome)
    }

    /// `complete(handle)` — the non-streaming convenience: `open` + `stream`
    /// to the terminal `ModelMessage`.
    pub fn complete(
        &mut self,
        handle: &CallHandle,
        sink: &mut dyn EventSink,
    ) -> Result<ModelMessage, GatewayError> {
        let outcome = self.stream(handle, sink)?;
        outcome.result.map_err(GatewayError::ModelError)
    }

    /// `cancel(handle, reason)` — a cancel signal plus the terminal
    /// `cancelled` failure row (the class is `cancelled`, `permanent` —
    /// never retried).
    pub fn cancel(
        &mut self,
        handle: &CallHandle,
        reason: &str,
        sink: &mut dyn EventSink,
    ) -> Result<(), GatewayError> {
        self.transport.cancel(&handle.call_id);
        let error = ModelError::new(ModelErrorClass::Cancelled, reason);
        let duration = (self.now_ms)().saturating_sub(handle.opened_at_ms);
        let timing = crate::grammar::Timing {
            latency_ms: duration,
            attempts: 0,
            ttft_ms: None,
            queue_wait_ms: 0,
            measured_at: "adapter".to_string(),
        };
        sink.emit(
            "model.call.failed",
            stamp_participant(
                events::call_failed(
                    &handle.call_id,
                    &error,
                    None,
                    &timing,
                    handle.credential.binding_id.as_deref(),
                ),
                handle.request.participant_class.as_deref(),
            ),
        )
        .map_err(|detail| GatewayError::Ledger { detail })
    }

    /// `estimate_tokens(plan)` — the C0 `local_len` estimate (a `counting`
    /// estimate requires a live `count_tokens` call — kernel wiring).
    pub fn estimate_tokens(
        &self,
        dialect_id: &str,
        dialect_version: &str,
        plan: &ProviderRequestPlan,
    ) -> Result<codec::Estimate, GatewayError> {
        let dialect = self
            .dialects
            .get(&Self::key(dialect_id, dialect_version))
            .ok_or_else(|| GatewayError::UnknownDialect {
                dialect: Self::key(dialect_id, dialect_version),
            })?;
        Ok(codec::estimate_tokens(dialect, plan))
    }

    /// `discover(endpoint_ref)` — H4 admission, then the transport's
    /// `model_listing` read. Refuses endpoints outside the allow-list before
    /// any bytes leave.
    pub fn discover(&mut self, endpoint_ref: &str) -> Result<Json, GatewayError> {
        self.endpoints.admit(endpoint_ref)?;
        self.transport
            .discover(endpoint_ref)
            .map_err(GatewayError::ModelError)
    }
}

/// Stamp `participant_class` onto an emitted payload — the hosted
/// normalization's only observable member (AC-R-2.3.1-15). `None`/`"native"`
/// leaves the payload untouched; `"hosted"` (or any other class spelling) is
/// carried verbatim so the ledger row records how the bytes were reached.
pub fn stamp_participant(payload: Json, participant_class: Option<&str>) -> Json {
    match participant_class {
        Some(c) if c != "native" => {
            if let Json::Obj(mut m) = payload {
                m.insert("participant_class".to_string(), Json::str(c));
                Json::Obj(m)
            } else {
                payload
            }
        }
        _ => payload,
    }
}

/// `charge_model(message, request)` — the model coordinate the terminal's
/// charge keys at (AC-R-2.3.1-9): the *served* model when the provider
/// substituted, the requested `model_ref` otherwise. `profile_ref` is the
/// request's own (the binding never changes mid-call).
pub fn charge_model(
    message: &crate::message::ModelMessage,
    request: &crate::plan::InferenceRequest,
) -> crate::plan::ModelRef {
    let mut m = request.model_ref.clone();
    if let Some(served) = &message.served_model {
        if *served != request.model_ref.provider_model_id {
            m.provider_model_id = served.clone();
        }
    }
    m
}
