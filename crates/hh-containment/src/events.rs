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

// ── the egress rows (S2.4; ADR-0266) ───────────────────────────────────────

use crate::admit::ContainmentDiff;
use crate::amend::diff_json;
use crate::amend::AmendOutcome;
use crate::egress::{token_hash, EgressDecision, EgressRequest};
use crate::policy::AmendmentBasis;
use hh_provenance::{PersistenceScope, ProvenanceRecord};

/// The `decision_request`'s content address — `request_ref` is resolvable to
/// the exact record the mediator decided on (CC3).
pub fn request_ref(req: &EgressRequest) -> String {
    blob_ref(&req.to_json())
}

/// The `security.egress.requested` payload — content-free: the request's id
/// and destination spellings, the token's *hash*, and the sentinel refs
/// (channel coordinates, never material). No header values, no body.
pub fn egress_requested_payload(req: &EgressRequest, sentinel_refs: &[String]) -> Json {
    let mut m = Json::obj([
        ("request_ref", Json::str(request_ref(req))),
        ("token_hash", Json::str(token_hash(&req.token))),
        ("tool_call_id", Json::str(req.tool_call_id.clone())),
        ("env_handle", Json::str(req.env_handle.clone())),
        ("protocol", Json::str(req.protocol.as_str())),
        ("host_raw", Json::str(req.host_raw.clone())),
        ("host_norm", Json::str(req.host_norm())),
        ("port", Json::Int(req.port as i64)),
        (
            "resolved_addrs",
            Json::Arr(
                req.resolved_addrs
                    .iter()
                    .map(|a| Json::str(a.clone()))
                    .collect(),
            ),
        ),
        (
            "sentinel_refs",
            Json::Arr(sentinel_refs.iter().map(|s| Json::str(s.clone())).collect()),
        ),
    ]);
    if let (Json::Obj(ref mut map), Some(e)) = (&mut m, &req.effect_id) {
        map.insert("effect_id".to_string(), Json::str(e.clone()));
    }
    if let (Json::Obj(ref mut map), Some(mth)) = (&mut m, &req.method) {
        map.insert("method".to_string(), Json::str(mth.clone()));
    }
    if let (Json::Obj(ref mut map), Some(p)) = (&mut m, &req.path) {
        map.insert("path".to_string(), Json::str(p.clone()));
    }
    m
}

/// `decided_by` — how the final verdict was reached (the decided row's
/// `decided_by` member): `policy` (the rule grammar decided),
/// `approval_cache`, `monitor` (an endorsement resolved an ask), or
/// `default` (`default_unmatched` deny).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecidedBy {
    /// A policy rule decided.
    Policy,
    /// The approval cache hit.
    ApprovalCache,
    /// The monitor's endorsement resolved an ask.
    Monitor,
    /// `default_unmatched` denied.
    Default,
}

impl DecidedBy {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DecidedBy::Policy => "policy",
            DecidedBy::ApprovalCache => "approval_cache",
            DecidedBy::Monitor => "monitor",
            DecidedBy::Default => "default",
        }
    }
}

/// The `security.egress.decided` payload — one per request (§5g.4 §6);
/// `checked_addrs` are the addresses the mediator itself resolved and
/// re-checked (the consume-once answer), `credential_binding_applied` the
/// bindings actually substituted (the rule's declared candidates filtered
/// by destination coverage — ADR-0266 D2).
#[allow(clippy::too_many_arguments)] // a table row is a row — the arity is the payload's.
pub fn egress_decided_payload(
    req: &EgressRequest,
    policy_version_id: &str,
    decision: &EgressDecision,
    decided_by: DecidedBy,
    checked_addrs: &[String],
    credential_binding_applied: &[String],
    effect_id: &str,
    latency_ms: u64,
) -> Json {
    let mut m = Json::obj([
        ("request_ref", Json::str(request_ref(req))),
        ("token_hash", Json::str(token_hash(&req.token))),
        ("effect_id", Json::str(effect_id)),
        ("tool_call_id", Json::str(req.tool_call_id.clone())),
        ("env_handle", Json::str(req.env_handle.clone())),
        ("protocol", Json::str(req.protocol.as_str())),
        ("host_raw", Json::str(req.host_raw.clone())),
        ("host_norm", Json::str(req.host_norm())),
        ("port", Json::Int(req.port as i64)),
        ("policy_version_id", Json::str(policy_version_id)),
        ("decision", Json::str(decision.decision.as_str())),
        ("source", Json::str(decision.source.as_str())),
        ("decided_by", Json::str(decided_by.as_str())),
        (
            "checked_addrs",
            Json::Arr(checked_addrs.iter().map(|a| Json::str(a.clone())).collect()),
        ),
        (
            "credential_binding_applied",
            Json::Arr(
                credential_binding_applied
                    .iter()
                    .map(|s| Json::str(s.clone()))
                    .collect(),
            ),
        ),
        ("latency_ms", Json::Int(latency_ms as i64)),
    ]);
    if let (Json::Obj(ref mut map), Some(r)) = (&mut m, &decision.rule_ref) {
        map.insert("rule_ref".to_string(), Json::str(r.clone()));
    }
    if let (Json::Obj(ref mut map), Some(r)) = (&mut m, &decision.reason) {
        map.insert("reason".to_string(), Json::str(r.as_str()));
    }
    if let (Json::Obj(ref mut map), Some(mth)) = (&mut m, &req.method) {
        map.insert("method".to_string(), Json::str(mth.clone()));
    }
    m
}

/// The `endorser` member's canonical tag — `<origin-tag>:<coordinate>
/// @<authority>` (an identity coordinate spelling, never record content).
pub fn endorser_tag(p: &ProvenanceRecord) -> String {
    let coord = match &p.origin {
        hh_provenance::Origin::Human { author_ref, .. } => format!("human:{author_ref}"),
        hh_provenance::Origin::Kernel { component_ref } => format!("kernel:{component_ref}"),
        hh_provenance::Origin::Model { model_ref, .. } => format!("model:{model_ref}"),
        hh_provenance::Origin::Tool { capability, .. } => format!("tool:{capability}"),
        hh_provenance::Origin::Evolution { candidate_id, .. } => {
            format!("evolution:{candidate_id}")
        }
        hh_provenance::Origin::Import { source_system, .. } => format!("import:{source_system}"),
        hh_provenance::Origin::Migration { from_dialect } => format!("migration:{from_dialect}"),
        hh_provenance::Origin::Participant {
            participant_ref, ..
        } => {
            format!("participant:{participant_ref}")
        }
        hh_provenance::Origin::Cache { entry_ref } => format!("cache:{entry_ref}"),
    };
    format!("{coord}@{}", p.authority.as_str())
}

/// `security.containment.amended{policy_version_id, from_version_id,
/// to_version_id, diff, basis, endorser, scope, effect_id?}` — the `amend`
/// verb's row (success only; a refused amend surfaces through the ask's
/// permission rows).
pub fn amended_payload(
    outcome: &AmendOutcome,
    diff: &ContainmentDiff,
    basis: AmendmentBasis,
    endorser: &ProvenanceRecord,
    scope: PersistenceScope,
    effect_id: Option<&str>,
) -> Json {
    let mut m = Json::obj([
        (
            "policy_version_id",
            Json::str(outcome.to_version_id.clone()),
        ),
        (
            "from_version_id",
            Json::str(outcome.from_version_id.clone()),
        ),
        ("to_version_id", Json::str(outcome.to_version_id.clone())),
        ("diff", diff_json(diff)),
        ("basis", Json::str(basis.as_str())),
        ("endorser", Json::str(endorser_tag(endorser))),
        ("scope", Json::str(scope.as_str())),
    ]);
    if let (Json::Obj(ref mut map), Some(e)) = (&mut m, effect_id) {
        map.insert("effect_id".to_string(), Json::str(e));
    }
    m
}
