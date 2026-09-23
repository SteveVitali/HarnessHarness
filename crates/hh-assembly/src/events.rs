//! The assembly lifecycle events (§3.3.4): `lifecycle.component.bound` per binding at
//! `instantiate`, `lifecycle.definition.changed` recording an accepted resume change —
//! appended through the run's **single fenced writer** (`hh_ledger::Store::append` under
//! the writer `Lease`), mirroring `hh_registry::events::emit` (CC7). Payloads carry ids
//! and spellings only — never implementation bodies, never secrets (CC3/CC2).

use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::EventRef;
use hh_ledger::store::{Lease, Store};
use hh_ledger::LedgerError;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

/// `lifecycle.component.bound` — emitted per binding attempt at `instantiate`.
pub const COMPONENT_BOUND: &str = "lifecycle.component.bound";
/// `lifecycle.definition.changed` — emitted for accepted resume changes.
pub const DEFINITION_CHANGED: &str = "lifecycle.definition.changed";
/// `context.artefact.delivered` — the context builder's delivery record
/// (`verify_resume`'s removal witness; §5a.1/§3.3.4).
pub const ARTEFACT_DELIVERED: &str = "context.artefact.delivered";

/// A pending assembly lifecycle row — the class plus its id/spelling-only payload.
#[derive(Debug, Clone, PartialEq)]
pub struct AssemblyEvent {
    /// The registered class spelling.
    pub class: &'static str,
    /// The payload (ids/spellings only — content-free).
    pub payload: Json,
}

/// A `lifecycle.component.bound` row — `{slot, class_id, variant_ref, placement,
/// host_id?, bind_result}` (§3.3.4; `bind_result ∈ {bound | not_installed |
/// trust_denied | locality_unsupported | contract_mismatch | isolation_unavailable}`).
pub fn component_bound(
    slot: &str,
    class_id: &str,
    variant_semantic_id: Option<&str>,
    variant_version_id: &str,
    placement: &str,
    host_id: Option<&str>,
    bind_result: &str,
) -> AssemblyEvent {
    AssemblyEvent {
        class: COMPONENT_BOUND,
        payload: Json::obj([
            ("slot", Json::str(slot)),
            ("class_id", Json::str(class_id)),
            (
                "variant_ref",
                Json::obj([
                    (
                        "semantic_id",
                        variant_semantic_id.map_or(Json::Null, Json::str),
                    ),
                    ("version_id", Json::str(variant_version_id)),
                ]),
            ),
            ("placement", Json::str(placement)),
            ("host_id", host_id.map_or(Json::Null, Json::str)),
            ("bind_result", Json::str(bind_result)),
        ]),
    }
}

/// A `lifecycle.definition.changed` row — `{definition_ref, diff_ref, reasons[]}`
/// recording an accepted resume change (§3.3.4).
pub fn definition_changed(
    semantic_id: &str,
    version_id: &str,
    diff_ref: &str,
    reasons: &[String],
) -> AssemblyEvent {
    AssemblyEvent {
        class: DEFINITION_CHANGED,
        payload: Json::obj([
            (
                "definition_ref",
                Json::obj([
                    ("semantic_id", Json::str(semantic_id)),
                    ("version_id", Json::str(version_id)),
                ]),
            ),
            ("diff_ref", Json::str(diff_ref)),
            (
                "reasons",
                Json::Arr(reasons.iter().map(Json::str).collect()),
            ),
        ]),
    }
}

/// Append pending assembly rows through the run's single fenced writer — the
/// `kernel_origin` classes carry the kernel provenance (CC2; the emitting component is
/// `kernel:assembly`).
pub fn emit(
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    kernel: &ProvenanceRecord,
    events: Vec<AssemblyEvent>,
) -> Result<(), LedgerError> {
    if events.is_empty() {
        return Ok(());
    }
    let mut batch = Vec::with_capacity(events.len());
    let mut parent = store.head_event_id(run_id)?;
    for ev in events {
        let event_id = store.alloc_id("event");
        batch.push(Event {
            event_id: event_id.clone(),
            class: ev.class.to_string(),
            ts: store.ts_now(),
            hlc: None,
            producer: Producer::kernel("assembly"),
            scope: Scope::default(),
            parent_event_id: parent,
            causes: Vec::<EventRef>::new(),
            refs: Vec::new(),
            ir_refs: Vec::new(),
            surface_ids: Default::default(),
            provenance: Some(kernel.clone()),
            content_kind: None,
            payload: ev.payload,
        });
        parent = event_id;
    }
    store.append(run_id, lease, batch)?;
    Ok(())
}
