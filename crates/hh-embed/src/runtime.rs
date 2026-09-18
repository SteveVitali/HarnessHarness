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
//! - [`KernelAssembler`] — the context assembler: the request record is
//!   the context request verbatim (no context builder is wired at
//!   Stage 1 — the assembled payload is honestly minimal).

use hh_control::driver::{
    AssembledRequest, AssemblerPort, EffectGate, GateOutcome, LedgerSink, ModelOutcome, ModelPort,
};
use hh_control::output::ParsedCall;
use hh_control::vocab::SettledOutcome;
use hh_ledger::event::{Event, EventEnvelope};
use hh_ledger::store::{Lease, Store};
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

/// The context assembler — Stage 1 returns the context request as the
/// request record (no `hh-context` builder is wired at the embed
/// boundary; the assembled payload records that honestly).
#[derive(Debug, Default)]
pub struct KernelAssembler;

impl AssemblerPort for KernelAssembler {
    fn assemble(&mut self, context_request: &Json) -> AssembledRequest {
        AssembledRequest {
            request: Json::obj([("context_request", context_request.clone())]),
            assembled_payload: Some(Json::obj([
                ("assembler", Json::str("hh-embed/kernel")),
                ("context_request", context_request.clone()),
            ])),
        }
    }
}
