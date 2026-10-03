//! Group L — `lab.hosting.{describe,probe,attach}` (S4.5a; spec §6.6;
//! R-2.10.6; ADR-0164/0165/0166 (e)).
//!
//! The boundary is **records-in/records-out** over the registry and the run
//! ledger — it never loads a `hh-hosting` implementation (`hosting_edges =
//! []`, CC5/CC6):
//!
//! - `describe` reads the `participant` record and reconciles the declared
//!   capability map against the conformance entries the registry holds —
//!   the §6.6 §9.2 `capability_vector` with `stale`/`drift`/`quarantined`
//!   surfaced as data.
//! - `probe` registers a `conformance_report{subject_kind: participant}`
//!   (entries supplied verbatim, or collected through the [`HostingPlane`]
//!   seam when `drive: true`), then applies the quarantine rule: a DRIFT on
//!   a P0 dimension flips the participant's admission to `quarantined`
//!   (`lifecycle.registry.quarantined`); DRIFT elsewhere is annotation.
//! - `attach` writes the hosted-session record onto a run's ledger:
//!   `lifecycle.hosted.attached` + `lifecycle.component.bound{class_id =
//!   hosting_adapter}` (kernel-minted audit rows), lifted observational
//!   `rows[]` under **participant** provenance (vouched `version_identity`
//!   → `delegate`, else `unverified` — DF-S1.3-2's vouch path), unliftable
//!   rows as `lifecycle.hosted.native_record` leaves (CC3 — nothing silently
//!   drops), `drift_entries` as `lifecycle.hosted.drift_observed`, and the
//!   seven hosted metric folds as `measurement.metric.emitted` rows.
//!
//! [`HostingPlane`]: the removable seam (design ruling D2): a pure-Json
//! driver the caller wires (`hh_hosting::Service::handle` satisfies it in
//! tests). When a call needs the plane and none is wired the op refuses
//! `hosting_plane_absent` — honest tier absence, never a faked report.

use std::collections::BTreeMap;

use hh_embed_schema::errors::EmbedError;
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::store::Lease;
use hh_provenance::{
    Attestation, AttestationAnchor, AttestationKind, AuthorityClass, MintingContext, Origin,
    PersistenceScope, ProvenanceRecord,
};
use hh_registry::hosted::reconcile_capability_vector;
use hh_registry::kinds::{RecordKind, SubjectKind};
use hh_registry::records::{ConformanceRecord, RegistryRecord};
use hh_registry::schema;
use hh_wire::json::Json;

use crate::registry_ops::{bad, opt_str, reg_err, registrar_of, req, req_str};
use crate::service::{ledger_err, EmbedService};

/// The removable Hosting Plane seam (D2): `plane(verb, params) -> Result<Json>`.
/// Verbs today: `"drive_probe"` — everything else is the boundary's own
/// records work. A wired `hh_hosting::Service` satisfies this in-process;
/// a hosted-service client satisfies it over the wire. The seam keeps
/// `hosting_edges = []` (no crate names `hh-hosting` as a dependency).
pub type HostingPlane = Box<dyn FnMut(&str, &Json) -> Result<Json, String>>;

/// The observational native classes a hosted `rows[]` entry may claim —
/// the projection's lifted surface (ADR-0164's session-ABI table). A row
/// naming any other class is preserved as a `lifecycle.hosted.native_record`
/// leaf, never coerced (participant-reported tool events stay `action.tool.*`
/// — never `action.effect.*`; the `lifecycle.hosted.*` family is Lab-minted).
const LIFTED_ROW_CLASSES: &[&str] = &[
    "lifecycle.turn.started",
    "lifecycle.turn.finished",
    "model.call.requested",
    "model.call.completed",
    "model.call.failed",
    "model.call.attempt.started",
    "model.call.attempt.completed",
    "model.call.attempt.failed",
    "model.stream.delta",
    "action.tool.proposed",
    "action.tool.completed",
    "action.tool.rejected",
    "action.tool.surface_rejected",
    "action.tool.call.refused",
    "security.permission.pending",
    "security.permission.decided",
    "measurement.cost.attributed",
    "context.artefact.delivered",
    "context.compaction.started",
    "context.compaction.completed",
];

/// The participant-report mark the lifted kernel mint stamps — the
/// fold's dual-provenance read (`provenance: participant_reported` on
/// the payload, never on the event's own provenance record — the Lab
/// records the observation; the participant's claim is data, CC2).
const PARTICIPANT_REPORTED: &str = "participant_reported";

/// The boundary's provenance component tag — the Lab side mints its own
/// audit rows (`attached`/`detached`/`drift_observed`/`native_record`) under
/// kernel origin; participant-origin provenance rides only the lifted
/// observational rows.
const COMPONENT: &str = "hh-embed/hosting";

/// The participant record's decoded boundary facts — the opaque body read
/// only for the members the ops need (records-in/records-out: the body is
/// `hh-hosting`'s schema; the boundary reads, never re-derives it).
struct ParticipantFacts {
    /// The registry `version_id` (envelope coordinate).
    version_id: String,
    /// `participant_version_identity` — the vouch/entry coordinate.
    version_identity: String,
    /// `descriptor.hosting_mechanism` (`session_abi` / … — recorded, never
    /// trusted as fact).
    hosting_mechanism: String,
    /// `ext hh.hosting/1 .abi_versions[]` — the advertised handshake set.
    abi_versions: Vec<String>,
    /// `capability_declaration` — the declared map the reconcile folds with.
    declaration: BTreeMap<String, Json>,
    /// `descriptor.observability_level[]` — the observability claim.
    observability_level: Vec<String>,
    /// The envelope's admission — `quarantined` excludes from attach.
    quarantined: bool,
}

/// Read the participant record (`version_id` pin, or `name`/`namespace`
/// selector through `resolve(execute)`).
fn participant_facts(svc: &EmbedService, params: &Json) -> Result<ParticipantFacts, EmbedError> {
    let (env, record) = if let Some(vid) = opt_str(params, "participant_ref") {
        svc.registry
            .get(&vid)
            .ok_or_else(|| bad("/participant_ref", "unknown_participant"))?
    } else {
        let input = hh_registry::store::ResolveInput::Selector {
            namespace: req_str(params, "namespace")?.to_string(),
            name: req_str(params, "name")?.to_string(),
            label: opt_str(params, "label"),
            snapshot_id: opt_str(params, "snapshot_id"),
        };
        let resolved = svc
            .registry
            .resolve(
                &input,
                hh_identity::names::ResolveMode::Execute,
                &Default::default(),
            )
            .map_err(reg_err)?;
        svc.registry
            .get(&resolved.envelope.version_id)
            .ok_or_else(|| bad("/participant_ref", "unknown_participant"))?
    };
    let RegistryRecord::Participant(body) = record else {
        return Err(bad("/participant_ref", "kind_mismatch"));
    };
    let version_identity = body
        .get("version_identity")
        .and_then(Json::as_str)
        .ok_or_else(|| bad("/participant_ref.body", "missing_version_identity"))?
        .to_string();
    let hosting_mechanism = body
        .get("descriptor")
        .and_then(|d| d.get("hosting_mechanism"))
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let abi_versions = body
        .get("hosting_ext")
        .and_then(|x| x.get("abi_versions"))
        .and_then(|v| match v {
            Json::Arr(items) => Some(
                items
                    .iter()
                    .filter_map(|i| i.as_str().map(String::from))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    let declaration = match body.get("capability_declaration") {
        Some(Json::Obj(m)) => m.clone(),
        _ => BTreeMap::new(),
    };
    let observability_level = body
        .get("descriptor")
        .and_then(|d| d.get("observability_level"))
        .and_then(|v| match v {
            Json::Arr(items) => Some(
                items
                    .iter()
                    .filter_map(|i| i.as_str().map(String::from))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    Ok(ParticipantFacts {
        version_id: env.version_id.clone(),
        version_identity,
        hosting_mechanism,
        abi_versions,
        declaration,
        observability_level,
        quarantined: env.admission == hh_registry::kinds::Admission::Quarantined,
    })
}

/// The reconciled vector over the participant's conformance entries — one
/// read path for `describe`/`probe`/`attach` (CC1).
fn reconciled(
    svc: &EmbedService,
    facts: &ParticipantFacts,
) -> hh_registry::hosted::CapabilityVector {
    let entries: Vec<ConformanceRecord> = svc
        .registry
        .hosted_conformance_entries(&facts.version_id)
        .into_iter()
        .cloned()
        .collect();
    reconcile_capability_vector(&facts.declaration, &entries, &facts.version_identity)
}

fn vector_json(v: &hh_registry::hosted::CapabilityVector) -> Json {
    Json::obj([
        (
            "vector",
            Json::Obj(
                v.vector
                    .iter()
                    .map(|(k, x)| (k.clone(), x.clone()))
                    .collect(),
            ),
        ),
        (
            "verdicts",
            Json::Obj(
                v.verdicts
                    .iter()
                    .map(|(k, x)| (k.clone(), Json::str(x.as_str())))
                    .collect(),
            ),
        ),
        (
            "drift_dimensions",
            Json::Arr(v.drift_dimensions.iter().map(Json::str).collect()),
        ),
        (
            "stale_dimensions",
            Json::Arr(v.stale_dimensions.iter().map(Json::str).collect()),
        ),
    ])
}

/// Decode a caller-supplied writer lease (`{lease_id, holder, generation,
/// expires_at_ms}` — the same members `lab.experiment.launch` returns).
fn lease_of(params: &Json, run_id: &str) -> Result<Lease, EmbedError> {
    let l = req(params, "lease")?;
    let int_at = |k: &str| -> Result<u64, EmbedError> {
        l.get(k)
            .and_then(Json::as_int)
            .map(|v| v as u64)
            .ok_or_else(|| bad(&format!("/lease/{k}"), "missing_field"))
    };
    Ok(Lease {
        run_id: run_id.to_string(),
        lease_id: req_str(l, "lease_id")?.to_string(),
        holder: req_str(l, "holder")?.to_string(),
        generation: int_at("generation")?,
        expires_at_ms: int_at("expires_at_ms")?,
    })
}

/// Mint one event with an explicit provenance record — `EventMinter`'s
/// kernel stamp swapped for the participant provenance the boundary
/// confers (the vouched `version_identity` → `delegate`; unvouched →
/// `unverified`). Called only for non-kernel-origin classes — the
/// `lifecycle.hosted.*` audit family stays kernel-minted.
fn mint_with_provenance(
    svc: &EmbedService,
    run_id: &str,
    class: &str,
    payload: Json,
    provenance: ProvenanceRecord,
) -> Result<Event, EmbedError> {
    Ok(Event {
        event_id: svc.store.alloc_id("evt"),
        class: class.to_string(),
        ts: svc.store.ts_now(),
        hlc: None,
        producer: Producer::kernel(COMPONENT),
        scope: Scope::default(),
        parent_event_id: svc.store.head_event_id(run_id).map_err(ledger_err)?,
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(provenance),
        content_kind: None,
        payload,
    })
}

impl EmbedService {
    /// `lab.hosting.describe{participant_ref | namespace+name}` → the
    /// participant record + the reconciled `capability_vector` +
    /// `stale`/`drift`/`quarantined` stamps (§6.6 §9; the handshake's
    /// advertised `abi_versions` ride the record — negotiation is the
    /// plane's act, never the boundary's guess).
    pub(crate) fn lab_hosting_describe(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let facts = participant_facts(self, params)?;
        let v = reconciled(self, &facts);
        let entries: Vec<Json> = self
            .registry
            .hosted_conformance_entries(&facts.version_id)
            .iter()
            .map(|e| hh_registry::schema::hosted_entry_json(e))
            .collect();
        Ok(Json::obj([
            ("version_id", Json::str(facts.version_id)),
            ("version_identity", Json::str(facts.version_identity)),
            ("hosting_mechanism", Json::str(facts.hosting_mechanism)),
            (
                "abi_versions",
                Json::Arr(facts.abi_versions.iter().map(Json::str).collect()),
            ),
            (
                "observability_level",
                Json::Arr(facts.observability_level.iter().map(Json::str).collect()),
            ),
            ("quarantined", Json::Bool(facts.quarantined)),
            ("capability_vector", vector_json(&v)),
            ("conformance_entries", Json::Arr(entries)),
        ]))
    }

    /// `lab.hosting.probe` — two modes (D6):
    /// * `report{…}` — a `conformance_report{subject_kind: participant}`
    ///   body with `hosted_entries[]` supplied verbatim (records-in): the
    ///   boundary registers it and applies the quarantine rule.
    /// * `drive{probes[], adapter_version_id?}` — forwards `drive_probe`
    ///   through the Hosting Plane; the plane's returned `entries[]` pack
    ///   into the caller's `report_base` scaffolding and take the same path.
    ///   Absent plane → `Refused{hosting_plane_absent}`.
    ///
    /// Either way: a DRIFT on a P0 dimension (`hh_ledger::hosted::is_p0`)
    /// quarantines the participant version; `skipped`/`unknown` never
    /// establish DRIFT (`ConformanceRecord::derive_verdict` is the rule —
    /// the boundary never coerces).
    pub(crate) fn lab_hosting_probe(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let facts = participant_facts(self, params)?;
        let registrar = registrar_of(params)?;

        // Assemble the report body: verbatim, or base + plane-driven entries.
        let mut report_body = if let Some(r) = params.get("report") {
            r.clone()
        } else if params.get("drive").is_some() {
            let drive = req(params, "drive")?;
            // The plane-seam contract (D2/D6): one `drive_probe{dimension}`
            // call per requested dimension; the outcome Json (verdict,
            // observed, note, detail_ref) packs into a `hosted_entries[]`
            // member — the declared column comes from the record (the
            // probe never invents the declaration), the verdict is
            // `derive_verdict(declared, observed)` computed here, never
            // authored (AC-R-2.10.6-3).
            let probes: Vec<String> = drive
                .get("probes")
                .and_then(|p| match p {
                    Json::Arr(items) => Some(
                        items
                            .iter()
                            .filter_map(|i| i.as_str().map(String::from))
                            .collect(),
                    ),
                    _ => None,
                })
                .ok_or_else(|| bad("/drive/probes", "missing_field"))?;
            let adapter_version_id = drive
                .get("adapter_version_id")
                .and_then(Json::as_str)
                .ok_or_else(|| bad("/drive/adapter_version_id", "missing_field"))?
                .to_string();
            let mut entries = Vec::new();
            for (i, dim) in probes.iter().enumerate() {
                let plane = self
                    .hosting_plane
                    .as_mut()
                    .ok_or_else(|| EmbedError::Refused {
                        reason: "hosting_plane_absent".into(),
                    })?;
                let out = plane(
                    "drive_probe",
                    &Json::obj([
                        ("dimension", Json::str(dim)),
                        (
                            "participant_version_identity",
                            Json::str(facts.version_identity.clone()),
                        ),
                        ("adapter_version_id", Json::str(adapter_version_id.clone())),
                    ]),
                )
                .map_err(|e| EmbedError::Refused { reason: e })?;
                let declared = facts
                    .declaration
                    .get(dim)
                    .cloned()
                    .unwrap_or_else(|| Json::str("unknown"));
                let observed = out
                    .get("observed")
                    .cloned()
                    .unwrap_or_else(|| Json::str("unknown"));
                let verdict = ConformanceRecord::derive_verdict(&declared, &observed).as_str();
                entries.push(Json::obj([
                    (
                        "participant_version_identity",
                        Json::str(facts.version_identity.clone()),
                    ),
                    ("adapter_version_id", Json::str(adapter_version_id.clone())),
                    ("dimension", Json::str(dim)),
                    ("declared", declared),
                    ("observed", observed),
                    ("verdict", Json::str(verdict)),
                    ("observed_in", Json::str("probe")),
                    (
                        "evidence_ref",
                        out.get("evidence_ref")
                            .or_else(|| out.get("detail_ref"))
                            .cloned()
                            .unwrap_or(Json::Null),
                    ),
                    ("at", Json::Int(i as i64)),
                ]));
            }
            let mut base = req(params, "report_base")?.clone();
            if let Json::Obj(ref mut m) = base {
                m.insert("hosted_entries".into(), Json::Arr(entries));
            }
            base
        } else {
            return Err(bad("/report", "missing_field"));
        };
        // The subject pin is the participant's — a report naming another
        // subject refuses (a probe report's subject is its participant).
        if let Json::Obj(ref mut m) = report_body {
            m.insert("kind".into(), Json::str("conformance_report"));
            m.insert("subject_ref".into(), Json::str(facts.version_id.clone()));
            m.insert("subject_kind".into(), Json::str("participant"));
        }
        let record = schema::record_from_json(RecordKind::ConformanceReport, &report_body)
            .map_err(reg_err)?;
        let RegistryRecord::Report(report) = record else {
            return Err(bad("/report", "kind_mismatch"));
        };
        if report.subject_kind != SubjectKind::Participant {
            return Err(bad("/report/subject_kind", "kind_mismatch"));
        }
        // Durability (the `lab_registry_record_conformance` rule): a
        // `lab`/`registry_ci` report must name a run the ledger holds as
        // finished — the boundary marks it durable in the registry view,
        // then `record_conformance` enforces (`RunNotDurable` when the run
        // never sealed). `publisher_claim` skips the floor — and never
        // enters `hosted_conformance_entries`.
        if matches!(
            report.produced_by,
            hh_registry::kinds::ProducedBy::RegistryCi | hh_registry::kinds::ProducedBy::Lab
        ) {
            let durable = self
                .store
                .events(&report.run_id)
                .map(|evs| evs.iter().any(|e| e.class == "lifecycle.run.finished"))
                .unwrap_or(false);
            if durable {
                self.registry
                    .mark_run_durable(&report.run_id, &self.kernel_prov.clone())
                    .map_err(reg_err)?;
            }
        }
        let subject_ref = report.subject_ref.clone();
        let drift_p0: Vec<String> = report
            .hosted_entries
            .iter()
            .filter(|e| {
                e.verdict == hh_registry::kinds::ConformanceVerdict::Drift
                    && hh_ledger::hosted::is_p0(&e.dimension)
            })
            .map(|e| e.dimension.clone())
            .collect();
        let drift_other: Vec<String> = report
            .hosted_entries
            .iter()
            .filter(|e| {
                e.verdict == hh_registry::kinds::ConformanceVerdict::Drift
                    && !hh_ledger::hosted::is_p0(&e.dimension)
            })
            .map(|e| e.dimension.clone())
            .collect();
        let vr = self
            .registry
            .record_conformance(report, &registrar)
            .map_err(reg_err)?;
        self.flush_registry_events()?;

        // The quarantine rule (§6.6 §2.5): P0 drift → admission flips;
        // non-P0 drift annotates (`drift_dimensions` on the result + the
        // reconcile's stratum — never silently quarantined).
        let mut quarantined = false;
        if !drift_p0.is_empty() {
            self.registry
                .quarantine(
                    &subject_ref,
                    &registrar,
                    &format!("P0 conformance DRIFT on {}", drift_p0.join(",")),
                )
                .map_err(reg_err)?;
            self.flush_registry_events()?;
            quarantined = true;
        }

        let v = reconciled(self, &facts);
        Ok(Json::obj([
            ("report_version_id", Json::str(vr.version_id)),
            (
                "entries",
                Json::Int(
                    self.registry
                        .hosted_conformance_entries(&facts.version_id)
                        .len() as i64,
                ),
            ),
            ("quarantined", Json::Bool(quarantined)),
            (
                "drift_dimensions",
                Json::Arr(v.drift_dimensions.iter().map(Json::str).collect()),
            ),
            (
                "drift_p0",
                Json::Arr(drift_p0.iter().map(Json::str).collect()),
            ),
            (
                "drift_annotated",
                Json::Arr(drift_other.iter().map(Json::str).collect()),
            ),
            ("capability_vector", vector_json(&v)),
        ]))
    }

    /// `lab.hosting.attach` — the hosted-session write (D3). Params:
    /// `{run_id, lease{…}, participant_ref, adapter_version_id?,
    ///  session{session_ref?, abi_version?, mediation?,
    ///          capability_vector?, budget_enforcement?, placement?},
    ///  rows[]?, events[]?, end_state?, drift_entries[]?, conformance?}`.
    ///
    /// First attach mints `lifecycle.hosted.attached` +
    /// `lifecycle.component.bound{class_id: hosting_adapter}` — skipped
    /// when the run already carries an `attached` row for the session
    /// (the engine's launch path emits the same pair — D4's convergence).
    /// `rows[]` mint under participant provenance (vouched `delegate`);
    /// `events[]`/off-allowlist rows land as `native_record` leaves;
    /// `end_state` → `detached`; `drift_entries[]` → `drift_observed`
    /// (and an optional `conformance` report registers `observed_in: run`
    /// entries through the `lab.hosting.probe` path); the seven hosted
    /// metric folds mint `measurement.metric.emitted` rows.
    pub(crate) fn lab_hosting_attach(&mut self, params: &Json) -> Result<Json, EmbedError> {
        // The write authority is either a live writer session's lease
        // (`session_id` — the C0 path; the lease never leaves the
        // service) or an explicit `lease{lease_id, holder, generation,
        // expires_at_ms}` — the `lab.experiment.launch` `subject_writer`
        // hand-off (D4).
        let (run_id, lease) = if let Some(sess_id) = opt_str(params, "session_id") {
            let s = self.writer_session(&sess_id)?;
            let run_id = s.run_id.clone();
            let lease = s.lease.clone().ok_or(EmbedError::Refused {
                reason: "session has no writer lease".into(),
            })?;
            (run_id, lease)
        } else {
            let run_id = req_str(params, "run_id")?.to_string();
            (run_id.clone(), lease_of(params, &run_id)?)
        };
        let facts = participant_facts(self, params)?;
        if facts.quarantined {
            return Err(EmbedError::Refused {
                reason: format!(
                    "participant {} is quarantined (a new declaration re-admits)",
                    facts.version_identity
                ),
            });
        }
        let session = req(params, "session")?.clone();
        let session_ref = opt_str(&session, "session_ref").unwrap_or_else(|| self.alloc("hsess"));

        // The minting context — the Lab confers the vouch (the record was
        // validated by the registry read above; the content never vouches
        // itself — CC2). Delegate for the ABI-checked claim; unverified
        // for everything else (DF-S1.3-2).
        let mut ctx = MintingContext::default();
        ctx.vouched_participants
            .insert(facts.version_identity.clone());
        let mut participant_prov = ProvenanceRecord::minted_in(
            Origin::participant(
                facts.version_identity.clone(),
                facts.hosting_mechanism.clone(),
            ),
            PersistenceScope::Run,
            self.store.now_ms(),
            &ctx,
        );
        // The vouch rides the durable row as a `pin` attestation — the
        // boundary verified the participant's `version_id` (the registry's
        // own content hash) under the Lab's trust store at attach.
        // `minted_ceiling` grants the attested ceiling, so the `delegate`
        // claim passes append's `validate` — an unvouched row mints
        // `unverified` and needs no attestation (the honest floor; a
        // participant's own claim never elevates, CC2).
        if participant_prov.authority == AuthorityClass::Delegate {
            participant_prov.attestation = Some(Attestation {
                kind: AttestationKind::Pin,
                subject_hash: facts.version_id.clone(),
                anchor: AttestationAnchor::Signer("registry:lab-trust-store".to_string()),
                verified_by: COMPONENT.to_string(),
                verified_at: self
                    .store
                    .events(&run_id)
                    .map(|e| e.len() as u64)
                    .unwrap_or(0),
            });
        }

        // The engine may already have stamped `attached` for this session —
        // the paths converge on one row (D4).
        let already_attached = self
            .store
            .events(&run_id)
            .map(|evs| {
                evs.iter().any(|e| {
                    e.class == "lifecycle.hosted.attached"
                        && e.payload.get("session_ref").and_then(Json::as_str)
                            == Some(session_ref.as_str())
                })
            })
            .unwrap_or(false);

        let mut batch: Vec<Event> = Vec::new();
        if !already_attached {
            // `lifecycle.hosted.attached{session_ref, version_identity,
            // mechanism, abi_version, mediation}` — kernel-minted.
            let mut m = BTreeMap::new();
            m.insert("session_ref".into(), Json::str(&session_ref));
            m.insert(
                "participant_version_identity".into(),
                Json::str(&facts.version_identity),
            );
            m.insert("participant_ref".into(), Json::str(&facts.version_id));
            m.insert(
                "hosting_mechanism".into(),
                Json::str(&facts.hosting_mechanism),
            );
            for k in ["abi_version", "mediation", "placement"] {
                if let Some(v) = session.get(k) {
                    m.insert(k.into(), v.clone());
                }
            }
            batch.push(mint_with_provenance(
                self,
                &run_id,
                "lifecycle.hosted.attached",
                Json::Obj(m),
                ProvenanceRecord::kernel(COMPONENT, self.store.now_ms()),
            )?);
            // `lifecycle.component.bound{class_id: hosting_adapter}` — the
            // driver binding (§6.1 §2.5), keyed on the adapter's version_id.
            if let Some(adapter) = opt_str(&session, "adapter_version_id")
                .or_else(|| opt_str(params, "adapter_version_id"))
            {
                batch.push(mint_with_provenance(
                    self,
                    &run_id,
                    "lifecycle.component.bound",
                    Json::obj([
                        ("class_id", Json::str("hosting_adapter")),
                        ("component_ref", Json::str(adapter)),
                        ("session_ref", Json::str(&session_ref)),
                    ]),
                    ProvenanceRecord::kernel(COMPONENT, self.store.now_ms()),
                )?);
            }
        }

        // `rows[]` — lifted observational rows under participant provenance;
        // off-allowlist classes collapse to `native_record` leaves (kernel
        // provenance — the Lab's record that it could not lift).
        let mut lifted = 0u64;
        let mut leaves = 0u64;
        if let Some(Json::Arr(rows)) = params.get("rows") {
            for (i, r) in rows.iter().enumerate() {
                let class = r
                    .get("class")
                    .and_then(Json::as_str)
                    .ok_or_else(|| bad(&format!("/rows[{i}]/class"), "missing_field"))?;
                let payload = r.get("payload").cloned().unwrap_or(Json::obj([]));
                let spec = hh_ledger::classes::lookup(class);
                // The mint rule (D3/CC2): a class the table marks
                // participant-writable (`!kernel_origin && !audit_grade`)
                // takes the participant's own provenance — `delegate` when
                // vouched, `unverified` otherwise. Audit-grade and
                // kernel-origin classes are Lab facts — the boundary
                // kernel-mints the row and stamps
                // `provenance: participant_reported` on the payload, the
                // observation mark the hosted metric folds read.
                let participant_writable = spec
                    .map(|sp| !sp.kernel_origin && !sp.audit_grade)
                    .unwrap_or(false);
                if LIFTED_ROW_CLASSES.contains(&class) && participant_writable {
                    batch.push(mint_with_provenance(
                        self,
                        &run_id,
                        class,
                        payload,
                        participant_prov.clone(),
                    )?);
                    lifted += 1;
                } else if LIFTED_ROW_CLASSES.contains(&class) && spec.is_some() {
                    let mut payload = payload;
                    if let Json::Obj(ref mut m) = payload {
                        m.insert("provenance".to_string(), Json::str(PARTICIPANT_REPORTED));
                    }
                    batch.push(mint_with_provenance(
                        self,
                        &run_id,
                        class,
                        payload,
                        ProvenanceRecord::kernel(COMPONENT, self.store.now_ms()),
                    )?);
                    lifted += 1;
                } else {
                    batch.push(mint_with_provenance(
                        self,
                        &run_id,
                        "lifecycle.hosted.native_record",
                        Json::obj([
                            ("kind", Json::str(class)),
                            ("payload", payload),
                            ("session_ref", Json::str(&session_ref)),
                        ]),
                        ProvenanceRecord::kernel(COMPONENT, self.store.now_ms()),
                    )?);
                    leaves += 1;
                }
            }
        }

        // `events[]` — HostedEvent envelopes the boundary cannot lift (raw
        // records-in): one `native_record` leaf each, verbatim.
        if let Some(Json::Arr(events)) = params.get("events") {
            for e in events {
                let kind = e
                    .get("kind")
                    .and_then(Json::as_str)
                    .unwrap_or("event")
                    .to_string();
                batch.push(mint_with_provenance(
                    self,
                    &run_id,
                    "lifecycle.hosted.native_record",
                    Json::obj([
                        ("kind", Json::str(kind)),
                        ("payload", e.clone()),
                        ("session_ref", Json::str(&session_ref)),
                    ]),
                    ProvenanceRecord::kernel(COMPONENT, self.store.now_ms()),
                )?);
                leaves += 1;
            }
        }

        // `drift_entries[]` — run-time contradictions annotate
        // (`lifecycle.hosted.drift_observed{dimension, declared, observed}`
        // — never silently quarantined; quarantine is the probe path's act).
        if let Some(Json::Arr(entries)) = params.get("drift_entries") {
            for (i, e) in entries.iter().enumerate() {
                let dim = e.get("dimension").and_then(Json::as_str).ok_or_else(|| {
                    bad(&format!("/drift_entries[{i}]/dimension"), "missing_field")
                })?;
                batch.push(mint_with_provenance(
                    self,
                    &run_id,
                    "lifecycle.hosted.drift_observed",
                    Json::obj([
                        ("dimension", Json::str(dim)),
                        ("declared", e.get("declared").cloned().unwrap_or(Json::Null)),
                        ("observed", e.get("observed").cloned().unwrap_or(Json::Null)),
                        ("session_ref", Json::str(&session_ref)),
                    ]),
                    ProvenanceRecord::kernel(COMPONENT, self.store.now_ms()),
                )?);
            }
        }

        // `end_state` → `lifecycle.hosted.detached{session_ref, end_state}`.
        if let Some(end_state) = params.get("end_state") {
            batch.push(mint_with_provenance(
                self,
                &run_id,
                "lifecycle.hosted.detached",
                Json::obj([
                    ("session_ref", Json::str(&session_ref)),
                    ("end_state", end_state.clone()),
                ]),
                ProvenanceRecord::kernel(COMPONENT, self.store.now_ms()),
            )?);
        }

        let appended = batch.len() as i64;
        if !batch.is_empty() {
            self.store
                .append(&run_id, &lease, batch)
                .map_err(ledger_err)?;
        }

        // The hosted metric folds — read back the run's rows (the projection
        // reads payloads, the same truth a rebuild sees), fold, and emit one
        // `measurement.metric.emitted` per produced value. `None` members
        // stay unemitted — the cell renders `n/a{not_run}`, never 0.
        let rows: Vec<hh_eval::facts::FactRow> = self
            .store
            .events(&run_id)
            .map(|evs| {
                evs.iter()
                    .enumerate()
                    .map(|(i, e)| hh_eval::facts::FactRow {
                        seq: i as u64,
                        event_id: Some(e.event_id.clone()),
                        class: e.class.clone(),
                        payload: e.payload.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let folded = hh_eval::hosted_metrics::fold(&rows);
        let mut emitted = 0u64;
        let mut metric_batch = Vec::new();
        for (name, value) in hh_eval::hosted_metrics::emit(&folded) {
            let Some(v) = value else { continue };
            metric_batch.push(mint_with_provenance(
                self,
                &run_id,
                "measurement.metric.emitted",
                Json::obj([
                    ("metric_ref", Json::str(name)),
                    ("value", Json::obj([("decimal", Json::Int(v))])),
                    ("applies_to", Json::str(&run_id)),
                    ("oracle_ref", Json::str("oracle/executable")),
                    ("detector", Json::str("deterministic")),
                ]),
                ProvenanceRecord::kernel(COMPONENT, self.store.now_ms()),
            )?);
            emitted += 1;
        }
        if !metric_batch.is_empty() {
            self.store
                .append(&run_id, &lease, metric_batch)
                .map_err(ledger_err)?;
        }

        // An optional conformance report (`observed_in: run` entries —
        // the contradiction-record path; quarantine applies as for probe).
        let mut conformance_ref = Json::Null;
        if let Some(report) = params.get("conformance") {
            let mut probe_params = Json::obj([
                ("participant_ref", Json::str(&facts.version_id)),
                ("report", report.clone()),
            ]);
            if let Some(reg) = params.get("registrar") {
                if let Json::Obj(ref mut m) = probe_params {
                    m.insert("registrar".into(), reg.clone());
                }
            }
            conformance_ref = self.lab_hosting_probe(&probe_params)?;
        }

        Ok(Json::obj([
            ("run_id", Json::str(&run_id)),
            ("session_ref", Json::str(&session_ref)),
            ("attached", Json::Bool(!already_attached)),
            ("appended", Json::Int(appended)),
            ("lifted_rows", Json::Int(lifted as i64)),
            ("native_record_leaves", Json::Int(leaves as i64)),
            ("metrics_emitted", Json::Int(emitted as i64)),
            ("conformance", conformance_ref),
            (
                "minted_participant_authority",
                Json::str(participant_prov.authority.as_str()),
            ),
        ]))
    }
}
