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
/// registrar_origin}` — the kind-specific class for capability registration
/// (ADR-0088 D8; CF-327 — kind-specific classes under ADR-0151 D9).
pub const CAPABILITY_REGISTERED: &str = "lifecycle.capability.registered";
/// `lifecycle.registry.imported{version_id, system, admission,
/// registrar_origin}` — a `foreign_import` record landed (S4.1; §6.2 event
/// family; the kind-specific class under ADR-0151 D9).
pub const IMPORTED: &str = "lifecycle.registry.imported";
/// `lifecycle.registry.pin_subject{version_id, admission}` — the subject anchor
/// a `security.label.endorsed` pin row references (S4.1). The row carries the
/// *subject-side* provenance (the record's quarantined standing) so the ledger's
/// append-time `check_endorsement` recomputes `from` off it — the subject's
/// label is a fact the endorsement re-derives, never a claim.
pub const PIN_SUBJECT: &str = "lifecycle.registry.pin_subject";
/// `security.label.endorsed{subject_ref, from, to, endorser, basis, basis_ref}`
/// — the `pin` endorsement that lifts a signature-required quarantine (§6.2;
/// §8.1 #3; the ledger re-verifies legitimacy at append — ADR-0035 §1).
pub const LABEL_ENDORSED: &str = "security.label.endorsed";

/// A pending registry audit row — the class plus its content-free payload.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistryEvent {
    /// The `lifecycle.registry.*` class spelling.
    pub class: &'static str,
    /// The payload (ids/spellings only — audit rows are content-free).
    pub payload: Json,
    /// The event's provenance override — `None` (the default) stamps the kernel
    /// provenance `emit` supplies (the audit-grade fenced-writer rule). The
    /// `pin_subject` anchor carries the *subject-side* provenance — a
    /// non-kernel standing the endorsement check reads as the `from` label.
    pub provenance: Option<ProvenanceRecord>,
    /// A tag binding this row to its allocated `event_id`: `emit` commits
    /// anchor rows in a first batch, then rewrites a later payload's
    /// `subject_ref = "@anchor:<tag>"` to the committed id (the ledger's
    /// endorsement check resolves committed events only).
    pub anchor_tag: Option<String>,
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
            provenance: None,
            anchor_tag: None,
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
                ("registrar_origin", Json::str(registrar_origin)),
            ]),
            provenance: None,
            anchor_tag: None,
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
            provenance: None,
            anchor_tag: None,
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
            provenance: None,
            anchor_tag: None,
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
            provenance: None,
            anchor_tag: None,
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
            provenance: None,
            anchor_tag: None,
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
            provenance: None,
            anchor_tag: None,
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
            provenance: None,
            anchor_tag: None,
        }
    }

    /// A `lifecycle.registry.imported` row — `{version_id, system, admission,
    /// registrar_origin}` (S4.1; the foreign-system spelling is a closed value,
    /// never content).
    pub fn imported(
        version_id: &str,
        system: &str,
        admission: &str,
        registrar_origin: &str,
    ) -> RegistryEvent {
        RegistryEvent {
            class: IMPORTED,
            payload: Json::obj([
                ("version_id", Json::str(version_id)),
                ("system", Json::str(system)),
                ("admission", Json::str(admission)),
                ("registrar_origin", Json::str(registrar_origin)),
            ]),
            provenance: None,
            anchor_tag: None,
        }
    }

    /// The `lifecycle.registry.pin_subject` anchor — carries the record's
    /// *subject-side* provenance (the quarantined standing the pin lifts from)
    /// so the ledger's append-time `check_endorsement` resolves the `from`
    /// label off a committed event, never a claim. `anchor_tag` binds the
    /// allocated `event_id` for the paired `pin_endorsed` row.
    pub fn pin_subject(
        version_id: &str,
        anchor_tag: &str,
        subject: ProvenanceRecord,
    ) -> RegistryEvent {
        RegistryEvent {
            class: PIN_SUBJECT,
            payload: Json::obj([
                ("version_id", Json::str(version_id)),
                ("admission", Json::str("quarantined")),
            ]),
            provenance: Some(subject),
            anchor_tag: Some(anchor_tag.to_string()),
        }
    }

    /// The `security.label.endorsed` pin row — the `LabelEndorsed` payload
    /// partitioned over `LABEL_FIELDS` (`subject_ref` is the `@anchor:<tag>`
    /// placeholder `emit` rewrites to the committed anchor's event id).
    pub fn pin_endorsed(anchor_tag: &str, ev: &hh_provenance::LabelEndorsed) -> RegistryEvent {
        let label = |l: &hh_provenance::Label| {
            Json::obj([
                ("authority", Json::str(l.authority.as_str())),
                (
                    "taint",
                    Json::Arr(l.taint.iter().map(|t| Json::str(t.as_string())).collect()),
                ),
            ])
        };
        RegistryEvent {
            class: LABEL_ENDORSED,
            payload: Json::obj([
                ("subject_ref", Json::str(format!("@anchor:{anchor_tag}"))),
                ("from", label(&ev.from)),
                ("to", label(&ev.to)),
                ("endorser", ev.endorser.to_json()),
                ("basis", Json::str(ev.basis.as_str())),
                (
                    "basis_ref",
                    ev.basis_ref.clone().map_or(Json::Null, Json::Str),
                ),
            ]),
            provenance: None,
            anchor_tag: None,
        }
    }
}

/// Append pending registry rows through the run's **single fenced writer**
/// (`store.append(run_id, lease, …)` — never a second writer; ADR-0151 (e)).
/// `kernel` is the kernel provenance the `kernel_origin` classes require; the
/// registrar's own provenance is payload data (CC2 — authority is conferred by the
/// row's origin, never read from registry content).
///
/// Two-phase for anchor rows (S4.1 `pin`): a row carrying `anchor_tag` commits in
/// a first `append` so a later row's `subject_ref = "@anchor:<tag>"` resolves
/// through the ledger's *committed* event index — `check_endorsement` reads
/// committed events only (the subject's provenance supplies the `from` label).
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
    let (anchors, rest): (Vec<RegistryEvent>, Vec<RegistryEvent>) =
        events.into_iter().partition(|e| e.anchor_tag.is_some());
    let mut anchor_ids: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let build = |store: &mut Store,
                 kernel: &ProvenanceRecord,
                 anchor_ids: &std::collections::HashMap<String, String>,
                 batch: Vec<RegistryEvent>|
     -> Result<Vec<Event>, LedgerError> {
        let mut out = Vec::with_capacity(batch.len());
        let mut parent = store.head_event_id(run_id)?;
        for ev in batch {
            let event_id = store.alloc_id("event");
            let mut payload = ev.payload;
            // An `@anchor:<tag>` subject ref resolves to the committed anchor id.
            if let Json::Obj(m) = &mut payload {
                if let Some(Json::Str(s)) = m.get("subject_ref") {
                    if let Some(tag) = s.strip_prefix("@anchor:") {
                        if let Some(id) = anchor_ids.get(tag) {
                            m.insert("subject_ref".to_string(), Json::str(id.clone()));
                        }
                    }
                }
            }
            out.push(Event {
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
                provenance: Some(ev.provenance.unwrap_or_else(|| kernel.clone())),
                content_kind: None,
                payload,
            });
            parent = event_id;
        }
        Ok(out)
    };
    if !anchors.is_empty() {
        // The anchor ids are the store's allocator output — the second batch's
        // `@anchor:<tag>` rewrites name these committed ids, never a claim.
        let mut parent = store.head_event_id(run_id)?;
        let mut batch = Vec::with_capacity(anchors.len());
        for ev in anchors {
            let event_id = store.alloc_id("event");
            if let Some(tag) = &ev.anchor_tag {
                anchor_ids.insert(tag.clone(), event_id.clone());
            }
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
                provenance: Some(ev.provenance.unwrap_or_else(|| kernel.clone())),
                content_kind: None,
                payload: ev.payload,
            });
            parent = event_id;
        }
        store.append(run_id, lease, batch)?;
    }
    let batch = build(store, kernel, &anchor_ids, rest)?;
    if !batch.is_empty() {
        store.append(run_id, lease, batch)?;
    }
    Ok(())
}
