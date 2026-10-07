//! The kernel-side ports the [`Driver`](hh_control::driver::Driver) drives
//! (§7.4 §2.5 — the embed boundary runs the canonical control loop, not a
//! shadow of it):
//!
//! - [`KernelSink`] — `LedgerSink` over `Store::append` under the
//!   session's fenced writer lease.
//! - [`EmbedModel`] — the Stage-1 deterministic model port: the
//!   production `hh-gateway` boundary lands with it; at Stage 1 the
//!   embed layer's model is scripted by the submitted input (a
//!   `{kind:"invoke"}` input block issues that capability call;
//!   anything else issues the `hh.submit` completion call — the run's
//!   work is the host's submission, honestly recorded).
//! - [`EmbedGate`] — the effect boundary: `hh.submit` dispatches
//!   `observed` carrying the `submission_ref` the driver's
//!   `stop_rule = submit` marker reads; an intent bound to a declared
//!   host-executor capability settles `unknown{awaiting_host}` and
//!   records the ask the service turns into an
//!   `upcall.invoke_host_capability` (the host's `report_host_effect`
//!   mints the terminal — `unknown` is probe-able, INV-2).
//! - [`KernelAssembler`] — the context assembler: runs the real
//!   `hh_context::assemble` builder over the run's durable prefix
//!   (DF-S2.8-1) — the appended `context.assembled` is the builder's
//!   canonical plan record.

use hh_context::{
    default_layout, AssemblyRequest, Candidate, CandidateKind, CandidateState, CollectSink,
    ContextBudget, DefaultPolicy, DerivedFrom, Estimate, PriorityClass, Retention,
};
use hh_control::driver::{
    AssembleInputs, AssembledRequest, AssemblerPort, EffectGate, GateOutcome, LedgerSink,
    ModelOutcome, ModelPort,
};
use hh_control::output::ParsedCall;
use hh_control::vocab::SettledOutcome;
use hh_identity::idp::idp_id;
use hh_ledger::event::{Event, EventEnvelope};
use hh_ledger::manifest::EventRef;
use hh_ledger::store::{Lease, Store};
use hh_ledger::views::context_view;
use hh_wire::json::Json;
use std::collections::BTreeSet;

/// `LedgerSink` over the run's `Store` under the session's writer lease.
pub struct KernelSink<'a> {
    pub store: &'a mut Store,
    pub run_id: String,
    pub lease: Lease,
}

impl LedgerSink for KernelSink<'_> {
    fn append(&mut self, mut events: Vec<Event>) -> Result<(), String> {
        // The sink owns the kernel provenance — the driver emits rows
        // either bare (its plain `append`) or carrying an explicit
        // delegate origin (verification claims — kept verbatim, never
        // overwritten). Provenance-mandatory classes refuse a bare row
        // at the append gate, so the sink stamps the kernel record.
        let now = self.store.now_ms();
        for ev in events.iter_mut() {
            if ev.provenance.is_none() {
                ev.provenance = Some(hh_provenance::ProvenanceRecord::kernel("hh-embed", now));
            }
        }
        self.store
            .append(&self.run_id, &self.lease, events)
            .map(|_| ())
            .map_err(|e| format!("{e:?}"))
    }
    fn prefix(&self) -> &[EventEnvelope] {
        self.store.events(&self.run_id).unwrap_or(&[])
    }
}

/// The `hh.submit` completion capability's surface id — the one
/// kernel-dispatched surface the scripted model calls.
pub const SUBMIT_SURFACE: &str = "hh.submit";

/// The Stage-1 model port — deterministic and offline. The submitted
/// input decides the call: a `{kind:"invoke", capability, args}` block
/// issues that capability's call (the host-executor round trip);
/// anything else issues `hh.submit` — the run completes when the host
/// says it is done.
#[derive(Debug, Default)]
pub struct EmbedModel {
    /// The capability surface the next call invokes, and its args.
    pub invoke: Option<(String, Json)>,
    /// The completion text the `hh.submit` call carries.
    pub completion: String,
    /// The response ref the next completed call reports.
    pub response_ref: String,
    /// Calls issued (drives `tool_call_id` allocation).
    pub calls_made: u64,
}

impl ModelPort for EmbedModel {
    fn call(&mut self, _model_call_id: &str, _request: &Json) -> ModelOutcome {
        self.calls_made += 1;
        let (surface, args_raw) = match &self.invoke {
            Some((cap, args)) => (cap.clone(), args.to_canonical_string()),
            None => (
                SUBMIT_SURFACE.to_string(),
                Json::obj([("text", Json::str(self.completion.clone()))]).to_canonical_string(),
            ),
        };
        ModelOutcome {
            stop_reason: hh_gateway::vocab::StopReason::ToolUse,
            response_ref: self.response_ref.clone(),
            text_empty: false,
            calls: vec![ParsedCall {
                tool_call_id: format!("tc-{}", self.calls_made),
                surface,
                args_raw,
            }],
            error_class: None,
            retry_after_ms: None,
        }
    }
}

/// The Stage-1 effect boundary. `hh.submit` completes; declared
/// host-executor surfaces defer to the host (`unknown{awaiting_host}` —
/// probe-able, never terminal); anything else is refused (the surface
/// table is closed — an intent reaching the gate off-table is a
/// defect, refused loudly).
#[derive(Debug, Default)]
pub struct EmbedGate {
    /// The surfaces bound to host-executed capabilities.
    pub host_surfaces: BTreeSet<String>,
    /// Asks recorded during dispatch — the service drains them into
    /// `upcall.invoke_host_capability` notifications + pending
    /// `report_host_effect` entries after each drive.
    pub host_asks: Vec<(String, u64, Json)>,
}

impl EffectGate for EmbedGate {
    fn dispatch(&mut self, effect_id: &str, attempt_no: u64, intent: &Json) -> GateOutcome {
        let surface = intent
            .get("surface_id")
            .or_else(|| intent.get("surface"))
            .and_then(Json::as_str)
            .unwrap_or("");
        if self.host_surfaces.contains(surface) {
            self.host_asks
                .push((effect_id.to_string(), attempt_no, intent.clone()));
            return GateOutcome {
                outcome: SettledOutcome::Unknown {
                    cause: "awaiting_host".to_string(),
                },
                submission_ref: None,
                error_class: None,
            };
        }
        if surface == SUBMIT_SURFACE {
            return GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".to_string(),
                },
                submission_ref: Some(format!("sub-{effect_id}")),
                error_class: None,
            };
        }
        GateOutcome {
            outcome: SettledOutcome::Refused,
            submission_ref: None,
            error_class: Some("surface_not_bound".to_string()),
        }
    }
}

/// The context assembler (DF-S2.8-1) — the embed boundary runs the real
/// `hh_context::assemble` builder on every live `propose`: the durable
/// prefix folds into `context_view`, `context.observation.recorded` items
/// project to `Observation` candidates, and `DefaultPolicy` over
/// `default_layout` assembles the plan the driver appends as
/// `context.assembled`. Retrieval/compaction producers are the deferred
/// halves of DF-S2.8-1 — the candidate set here is exactly what the
/// observation plane records, nothing invented.
#[derive(Debug, Default)]
pub struct KernelAssembler;

/// The pinned C0 estimator the embed-boundary plan records
/// (`canonical_bytes / 4`, floor 1 — deterministic; the ref is a hash
/// input, so a swapped estimator moves `plan_id` — I-DET).
const ASSEMBLER_ESTIMATOR_REF: &str = "est/canonical_bytes_div4@1";
/// The layout the kernel assembler runs under (the spec's default slot
/// declaration set).
const ASSEMBLER_LAYOUT_REF: &str = "layout/default";
/// `idp/1` domain for the projection's `context_item_id`s — the content
/// address of the observation payload.
const OBSERVATION_ITEM_IDP: &str = "hh.embed.context_item/1";
/// `DefaultPolicy::old_after_turns` — mirrored here for the declared
/// `Optional` class (the policy derives age only for `Required`; an
/// evictable observation declares its own class).
const OBSERVATION_OLD_AFTER: u64 = 8;

/// `context.observation.recorded` → the `Observation` candidate the item
/// projects to (§5c.1 data model): the event IS the context item — its
/// seq is the source order, its provenance the item's (a bare row falls
/// back to a kernel-minted record, never a fabricated origin), its
/// canonical payload the content address the estimate charges.
fn observation_candidate(
    run_id: &str,
    model_call_id: &str,
    e: &EventEnvelope,
    at_seq: u64,
) -> Candidate {
    let canonical = e.payload.to_canonical_string();
    let tokens = ((canonical.len() / 4) as u64).max(1);
    let provenance = e
        .provenance
        .clone()
        .unwrap_or_else(|| hh_provenance::ProvenanceRecord::kernel("hh-embed/kernel", e.seq));
    let authority = provenance.authority;
    let readers = match &provenance.readers {
        hh_provenance::ReaderSet::Public => None,
        hh_provenance::ReaderSet::Restricted(rs) => {
            Some(hh_provenance::ReaderSet::Restricted(rs.clone()))
        }
    };
    Candidate {
        candidate_id: format!("{model_call_id}:obs:{}", e.event_id),
        context_item_id: Some(idp_id(OBSERVATION_ITEM_IDP, canonical.as_bytes())),
        kind: CandidateKind::Observation,
        state: CandidateState::Expanded,
        retention: Retention::Optional(if at_seq.saturating_sub(e.seq) > OBSERVATION_OLD_AFTER {
            PriorityClass::ObservationOld
        } else {
            PriorityClass::ObservationRecent
        }),
        estimate: Estimate {
            tokens,
            estimator_ref: ASSEMBLER_ESTIMATOR_REF.to_string(),
        },
        source_event: Some(EventRef {
            run_id: run_id.to_string(),
            event_id: e.event_id.clone(),
        }),
        source_seq: e.seq,
        label: hh_provenance::label::Label::at(authority),
        provenance,
        validity: hh_hir::records::Validity::open_from(e.seq),
        readers,
        slot_hint: None,
        volatile: false,
        paired_with: None,
        batch_id: None,
        artefact_id: None,
        handle: None,
    }
}

impl AssemblerPort for KernelAssembler {
    fn assemble(
        &mut self,
        inputs: &AssembleInputs<'_>,
        context_request: &Json,
    ) -> AssembledRequest {
        let run_id = inputs
            .prefix
            .first()
            .map(|e| e.run_id.as_str())
            .unwrap_or_default();
        let view = context_view(run_id, inputs.prefix, None);
        let at_seq = view.derived_from_seq.unwrap_or(0);
        let req = AssemblyRequest {
            model_call_id: inputs.model_call_id.to_string(),
            view: DerivedFrom {
                run_id: run_id.to_string(),
                seq: at_seq,
                view_hash: view.view_hash,
            },
            at_seq,
            min_view_seq: None,
            candidates: inputs
                .prefix
                .iter()
                .filter(|e| e.class == "context.observation.recorded")
                .map(|e| observation_candidate(run_id, inputs.model_call_id, e, at_seq))
                .collect(),
            advisories: Vec::new(),
            layout: default_layout(),
            budget: ContextBudget {
                // `0` ⇒ no configured cap — the builder sees an unbounded
                // window rather than a zero cap that refuses everything.
                window_cap: if inputs.window_cap_tokens == 0 {
                    u64::MAX
                } else {
                    inputs.window_cap_tokens
                },
                margin: 0,
                reservations: Vec::new(),
                hard: true,
            },
            estimator_ref: ASSEMBLER_ESTIMATOR_REF.to_string(),
            policy_params: Json::Null,
            demotion_wrappers: Vec::new(),
        };
        let policy = DefaultPolicy::default();
        // `assembly_ms` is the measured wall the record stamps — measured
        // on a discarded run so the stamped value is true; `assemble` is
        // pure (I-DET) so `plan_id` is identical across the pair.
        let mut probe_sink = CollectSink::default();
        let t0 = std::time::Instant::now();
        let _ = hh_context::assemble(
            &req,
            &policy,
            ASSEMBLER_LAYOUT_REF,
            "none",
            0,
            &mut probe_sink,
        );
        let assembly_ms = t0.elapsed().as_millis() as u64;
        let mut sink = CollectSink::default();
        match hh_context::assemble(
            &req,
            &policy,
            ASSEMBLER_LAYOUT_REF,
            "none",
            assembly_ms,
            &mut sink,
        ) {
            Ok(outcome) => {
                // The builder emits `context.assembled` + side-band rows
                // into the sink; the driver appends them — `assembled`
                // first, the rest in emit order.
                let mut assembled_payload = None;
                let mut side_events = Vec::new();
                for (class, payload) in sink.events {
                    if class == "context.assembled" {
                        assembled_payload = Some(payload);
                    } else {
                        side_events.push((class, payload));
                    }
                }
                AssembledRequest {
                    request: Json::obj([
                        ("context_request", context_request.clone()),
                        ("context_plan", outcome.plan.body_json()),
                    ]),
                    assembled_payload,
                    side_events,
                }
            }
            Err(err) => {
                // The port is infallible — an assembly error is recorded
                // on the row, never hidden, and the turn proceeds on the
                // raw context request. (Routing `ContextWindowExceeded`
                // into a `stop` outcome is the remaining DF-S2.8-1 half.)
                AssembledRequest {
                    request: Json::obj([("context_request", context_request.clone())]),
                    assembled_payload: Some(Json::obj([
                        ("assembler", Json::str("hh-context/assemble")),
                        ("assembly_error", Json::str(format!("{err}"))),
                        ("model_call_id", Json::str(inputs.model_call_id.to_string())),
                    ])),
                    side_events: Vec::new(),
                }
            }
        }
    }
}
