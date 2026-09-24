//! The `security.containment.*` payload builders (§5g.4 §3; ADR-0061 D5).
//! Every event is audit-grade — kernel producer, mandatory provenance,
//! `durability = ledger`, `observability_level ⊇ events` — and carries
//! content-free fields only (CC3): closed tags, identity coordinates and
//! content-addressed blob refs, never record text.
//!
//! ```text
//! security.containment.applied{env_handle, policy_version_id,
//!     effective_policy_hash, backend, isolation_class,
//!     enforcement_evidence, lowering_loss_ref, probes_ref}
//! security.containment.violated{point, backend, kind, subject,
//!     evidence_kind, effect_id?}
//! security.containment.unverified{field_group, reason}
//! ```
//!
//! `lowering_loss_ref`/`probes_ref` are `idp/1` content addresses over the
//! canonical JSON the caller persists with `put_blob` (CC3 — the ref is
//! resolvable to the exact bytes); [`lowering_loss_json`]/[`probes_json`]
//! produce the addressed records. `security.containment.amended` and the
//! `security.egress.*` rows are Stage-2 (`amend()`/the mediator) — declared
//! in `hh_ledger::classes`, built there.
//!
//! The `unverified.field_group` member spells a `FieldGroup` when the
//! failure is group-scoped; the whole-attach failures (helper absent,
//! backend unsupported, ref mismatch, stale report) spell the closed
//! non-group tags `attach`/`report` — the member is the *subject* of the
//! missing evidence, and "no group could be evidenced" is a first-class
//! answer (ADR-0243 D6).

use hh_wire::json::Json;

use crate::backend::ViolationKind;
use crate::policy::ContainmentPolicy;
use crate::report::{ContainmentReport, EvidenceKind, ProbeResult};

/// The `unverified.field_group` spelling for a whole-attach failure (no
/// single group was evidenced).
pub const ATTACH_GROUP: &str = "attach";
/// The `unverified.field_group` spelling for a stale report on resume.
pub const REPORT_GROUP: &str = "report";

/// The content-addressed blob ref of a canonical JSON record — the
/// `*_ref` members' value (`sha256:<hex>` under `idp/1`'s `blob` domain).
pub fn blob_ref(j: &Json) -> String {
    hh_identity::idp::address(j.to_canonical_string().as_bytes(), "application/json").id()
}

/// The `lowering_loss` blob — `[{field, backend, reason}]`.
pub fn lowering_loss_json(report: &ContainmentReport) -> Json {
    Json::Arr(
        report
            .lowering_loss
            .iter()
            .map(|l| {
                Json::obj([
                    ("field", Json::str(l.field.clone())),
                    ("backend", Json::str(l.backend.clone())),
                    ("reason", Json::str(l.reason.clone())),
                ])
            })
            .collect(),
    )
}

/// The `probes` blob — `[{kind, expected, observed, evidence_kind}]`.
pub fn probes_json(report: &ContainmentReport) -> Json {
    Json::Arr(report.probes.iter().map(ProbeResult::to_json).collect())
}

/// `security.containment.applied{…}` — appended at every attach (the
/// `action.environment.attached` row references it, CF-138). The
/// `effective_policy_hash` is the canonical record's `idp/1` blob address;
/// `policy_version_id` is the typed identity coordinate — both are carried
/// so a reader resolving either lands on the same policy (ADR-0036).
pub fn applied_payload(
    env_handle: &str,
    policy: &ContainmentPolicy,
    report: &ContainmentReport,
) -> Json {
    Json::obj([
        ("env_handle", Json::str(env_handle)),
        ("policy_version_id", Json::str(policy.version_id.clone())),
        (
            "effective_policy_hash",
            Json::str(blob_ref(&policy.to_json())),
        ),
        ("backend", Json::str(report.backend.clone())),
        (
            "isolation_class",
            Json::str(report.isolation_class.as_str()),
        ),
        (
            "enforcement_evidence",
            Json::Obj(
                report
                    .enforcement_evidence
                    .iter()
                    .map(|(g, e)| (g.as_str().to_string(), Json::str(e.as_str())))
                    .collect(),
            ),
        ),
        (
            "lowering_loss_ref",
            Json::str(blob_ref(&lowering_loss_json(report))),
        ),
        ("probes_ref", Json::str(blob_ref(&probes_json(report)))),
    ])
}

/// `security.containment.violated{…}` — a boundary denial observed through
/// `evidence_kind` (`kernel_log` for the EP2 gate's own record;
/// `heuristic_output` detections are labelled and never authorise).
/// `point` is the enforcement-point spelling (`ep2` at Stage 1).
pub fn violated_payload(
    point: &str,
    backend: &str,
    kind: ViolationKind,
    subject: &str,
    evidence_kind: EvidenceKind,
    effect_id: Option<&str>,
) -> Json {
    let mut m = Json::obj([
        ("point", Json::str(point)),
        ("backend", Json::str(backend)),
        ("kind", Json::str(kind.as_str())),
        ("subject", Json::str(subject)),
        ("evidence_kind", Json::str(evidence_kind.as_str())),
    ]);
    if let (Json::Obj(ref mut map), Some(e)) = (&mut m, effect_id) {
        map.insert("effect_id".to_string(), Json::str(e));
    }
    m
}

/// `security.containment.unverified{field_group, reason}` — appended on the
/// fail-closed path (helper absent, backend unsupported, stale report, or
/// `unknown` evidence for a relied-on group). `field_group` spells a
/// `FieldGroup`, [`ATTACH_GROUP`] or [`REPORT_GROUP`]; `reason` is a closed
/// tag (`helper_unavailable`, `backend_unsupported:<tag>`,
/// `report_stale`, `policy_ref_mismatch`, `evidence_unknown:<group>`,
/// `lab_degrade_declared`).
pub fn unverified_payload(field_group: &str, reason: &str) -> Json {
    Json::obj([
        ("field_group", Json::str(field_group)),
        ("reason", Json::str(reason)),
    ])
}
