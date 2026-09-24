//! The `lifecycle.registry.*` audit events (ADR-0151 (e), ADR-0152 (e), ADR-0153 (e)).
//! Every mutation appends a content-free, audit-grade row — ids and spellings only —
//! through the run's **single fenced writer** (`hh_ledger::Store::append` under the
//! writer `Lease`). The store collects each event as a [`RegistryEvent`]; the caller
//! (the run's kernel seam) drains and appends them — [`emit`] is the one helper.
//!
//! The rows are `kernel_origin` + `audit_grade` + `ledger`-durable (the declared
//! persistence table in `hh-ledger` — CC7; ADR-0239): the emitting *component* is the
//! kernel-side registry (`producer.component_class = "kernel"`,
//! `component_variant_ref = "registry"`); the *registrar's* provenance is payload
//! data, never the row's authority (CC2).

use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::EventRef;
use hh_ledger::store::{Lease, Store};
use hh_ledger::LedgerError;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

/// The eight `lifecycle.registry.*` classes (the ADR-0151/0152/0153 (e) rows).
pub const REGISTERED: &str = "lifecycle.registry.registered";
/// `lifecycle.registry.admission_refused` — a refused op is audited, never silent.
pub const ADMISSION_REFUSED: &str = "lifecycle.registry.admission_refused";
/// `lifecycle.registry.published`.
pub const PUBLISHED: &str = "lifecycle.registry.published";
/// `lifecycle.registry.name_deprecated`.
pub const NAME_DEPRECATED: &str = "lifecycle.registry.name_deprecated";
/// `lifecycle.registry.name_yanked`.
pub const NAME_YANKED: &str = "lifecycle.registry.name_yanked";
/// `lifecycle.registry.version_revoked`.
pub const VERSION_REVOKED: &str = "lifecycle.registry.version_revoked";
/// `lifecycle.registry.snapshotted`.
pub const SNAPSHOTTED: &str = "lifecycle.registry.snapshotted";
/// `lifecycle.registry.conformance_recorded`.
pub const CONFORMANCE_RECORDED: &str = "lifecycle.registry.conformance_recorded";
/// `lifecycle.capability.registered{version_id, semantic_id, source_kind,
/// registrar}` — the kind-specific class for capability registration
/// (ADR-0088 D8; CF-327 — kind-specific classes under ADR-0151 D9).
pub const CAPABILITY_REGISTERED: &str = "lifecycle.capability.registered";

/// A pending registry audit row — the class plus its content-free payload.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistryEvent {
    /// The `lifecycle.registry.*` class spelling.
    pub class: &'static str,
    /// The payload (ids/spellings only — audit rows are content-free).
    pub payload: Json,
}

impl RegistryEvent {
    /// A `registered` row.
    pub fn registered(
        kind: &str,
        version_id: &str,
        semantic_id: Option<&str>,
        admission: &str,
        registrar_origin: &str,
    ) -> RegistryEvent {
        RegistryEvent {
            class: REGISTERED,
            payload: Json::obj([
                ("kind", Json::str(kind)),
                ("version_id", Json::str(version_id)),
                ("semantic_id", semantic_id.map_or(Json::Null, Json::str)),
                ("admission", Json::str(admission)),
                ("registrar_origin", Json::str(registrar_origin)),
            ]),
        }
    }

    /// A `lifecycle.capability.registered` row — the kind-specific class a
    /// capability `register` emits instead of `lifecycle.registry.registered`
    /// (ADR-0088 D8).
    pub fn capability_registered(
        version_id: &str,
        semantic_id: Option<&str>,
        source_kind: &str,
        registrar_origin: &str,
    ) -> RegistryEvent {
        RegistryEvent {
            class: CAPABILITY_REGISTERED,
            payload: Json::obj([
                ("version_id", Json::str(version_id)),
                ("semantic_id", semantic_id.map_or(Json::Null, Json::str)),
                ("source_kind", Json::str(source_kind)),
                ("registrar", Json::str(registrar_origin)),
            ]),
        }
    }

    /// An `admission_refused` row.
    pub fn refused(operation: &str, reason: &str, subject: Option<&str>) -> RegistryEvent {
        RegistryEvent {
            class: ADMISSION_REFUSED,
            payload: Json::obj([
                ("operation", Json::str(operation)),
                ("reason", Json::str(reason)),
                ("subject", subject.map_or(Json::Null, Json::str)),
            ]),
        }
    }

    /// A `published` row.
    pub fn published(
        namespace: &str,
        name: &str,
        version_id: &str,
        label: Option<&str>,
        supersedes: Option<&str>,
        registrar_origin: &str,
    ) -> RegistryEvent {
        RegistryEvent {
            class: PUBLISHED,
            payload: Json::obj([
                ("namespace", Json::str(namespace)),
                ("name", Json::str(name)),
                ("version_id", Json::str(version_id)),
                ("label", label.map_or(Json::Null, Json::str)),
                ("supersedes", supersedes.map_or(Json::Null, Json::str)),
                ("registrar_origin", Json::str(registrar_origin)),
            ]),
        }
    }

    /// A `name_deprecated` / `name_yanked` row.
    pub fn name_status(
        class: &'static str,
        namespace: &str,
        name: &str,
        registrar_origin: &str,
    ) -> RegistryEvent {
        RegistryEvent {
            class,
            payload: Json::obj([
                ("namespace", Json::str(namespace)),
                ("name", Json::str(name)),
                ("registrar_origin", Json::str(registrar_origin)),
            ]),
        }
    }

    /// A `version_revoked` row.
    pub fn revoked(version_id: &str, reason: &str, revoker_origin: &str) -> RegistryEvent {
        RegistryEvent {
            class: VERSION_REVOKED,
            payload: Json::obj([
                ("version_id", Json::str(version_id)),
                ("reason", Json::str(reason)),
                ("revoker_origin", Json::str(revoker_origin)),
            ]),
        }
    }

    /// A `snapshotted` row.
    pub fn snapshotted(snapshot_id: &str, member_count: usize, seq: u64) -> RegistryEvent {
        RegistryEvent {
            class: SNAPSHOTTED,
            payload: Json::obj([
                ("snapshot_id", Json::str(snapshot_id)),
                ("member_count", Json::Int(member_count as i64)),
                ("seq", Json::Int(seq as i64)),
            ]),
        }
    }

    /// A `conformance_recorded` row.
    pub fn conformance(
        report_id: &str,
        subject_ref: &str,
        suite_ref: &str,
        produced_by: &str,
    ) -> RegistryEvent {
        RegistryEvent {
            class: CONFORMANCE_RECORDED,
            payload: Json::obj([
                ("report_id", Json::str(report_id)),
                ("subject_ref", Json::str(subject_ref)),
                ("suite_ref", Json::str(suite_ref)),
                ("produced_by", Json::str(produced_by)),
            ]),
        }
    }
}

/// Append pending registry rows through the run's **single fenced writer**
/// (`store.append(run_id, lease, …)` — never a second writer; ADR-0151 (e)).
/// `kernel` is the kernel provenance the `kernel_origin` classes require; the
/// registrar's own provenance is payload data (CC2 — authority is conferred by the
/// row's origin, never read from registry content).
pub fn emit(
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    kernel: &ProvenanceRecord,
    events: Vec<RegistryEvent>,
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
            producer: Producer::kernel("registry"),
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
