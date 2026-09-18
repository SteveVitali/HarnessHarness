//! `events` — the canonical payload builders for the verification-plane
//! event classes (spec §5f.1 §3 "Ledger events", §5f.2 §3, §5f.4 §3;
//! ADR-0110 D7, ADR-0112 D6, ADR-0113 D1, ADR-0115 D8). Builders are pure —
//! appending is the ledger's; the class table (hh-ledger) owns admission.
//! Every class here carries mandatory provenance (ADR-0035 §4 ∪ §5f.1 §6).

use hh_wire::Json;

use crate::claims::{Claim, ReconciliationRecord};
use crate::gate::{CompletionDecision, GateResult};
use crate::validators::Verdict;
use crate::vocab::{Agreement, VerdictStatus};

/// `verification.claim.recorded` —
/// `{claim | ref, kind, subject, extracted_by, extraction_confidence,
/// criteria_status?, evidence_refs[]}` (ADR-0112 D6). `claim` inline for a
/// recorded claim; `ref` for a claim recorded earlier.
pub fn claim_recorded(claim: &Claim) -> Json {
    let mut j = claim.to_json();
    if let Json::Obj(m) = &mut j {
        m.insert(
            "class".to_string(),
            Json::str("verification.claim.recorded"),
        );
    }
    j
}

/// `verification.claim.reconciled` —
/// `{claim_ref, agreement, divergence_class?, severity, detector,
/// detector_ref, confidence, evidence_refs[], probe_effect_ids[], mode,
/// charged_to}` (ADR-0112 D6).
pub fn claim_reconciled(record: &ReconciliationRecord) -> Json {
    let (agreement, class) = match &record.agreement {
        Agreement::Agree => (Json::str("agree"), Json::Null),
        Agreement::Diverge(c) => (Json::str("diverge"), Json::str(c.as_str())),
        Agreement::Unverifiable => (Json::str("unverifiable"), Json::Null),
        Agreement::Stale => (Json::str("stale"), Json::Null),
    };
    Json::obj([
        ("agreement", agreement),
        ("charged_to", Json::str(record.charged_to.name())),
        ("claim_ref", Json::str(record.claim_id.clone())),
        ("confidence_ppm", Json::Int(record.confidence_ppm as i64)),
        ("detector", Json::str(detector_name(record.detector))),
        ("detector_ref", Json::str(record.detector_ref.clone())),
        ("divergence_class", class),
        (
            "evidence_refs",
            Json::Arr(record.evidence_refs.iter().map(Json::str).collect()),
        ),
        ("mode", Json::str(record.mode.as_str())),
        (
            "probe_effect_ids",
            Json::Arr(record.probe_effect_ids.iter().map(Json::str).collect()),
        ),
        ("record_id", Json::str(record.record_id.clone())),
        (
            "reconciled_at_seq",
            Json::Int(record.reconciled_at_seq as i64),
        ),
        ("severity", Json::str(record.severity.as_str())),
    ])
}

/// `verification.gate.evaluated` —
/// `{completion_claim_ref, verdict, hold_count, divergences[],
/// required_actions[], budget_ref}` (ADR-0113 D1). Every evaluation appends
/// this event — the count is a ledger fact so re-entry loops are impossible.
pub fn gate_evaluated(result: &GateResult, completion_claim_ref: &str, budget_ref: &str) -> Json {
    let mut j = result.to_json();
    if let Json::Obj(m) = &mut j {
        m.insert(
            "completion_claim_ref".to_string(),
            Json::str(completion_claim_ref.to_string()),
        );
        m.insert("budget_ref".to_string(), Json::str(budget_ref.to_string()));
        m.insert(
            "budget_dimension".to_string(),
            Json::str(crate::RECONCILIATION_HOLDS),
        );
    }
    j
}

/// `verification.validator.invoked` —
/// `{validator_ref, target, phase, criterion_ref?, contract_id?,
/// evidence_refs[], inputs_digest, isolation}` (ADR-0110 D7). Appended at
/// `collect`; the invocation is charged (`dimension = evaluator_calls`).
pub fn validator_invoked(verdict_target: &Verdict, isolation: &crate::vocab::Isolation) -> Json {
    let isolation_json = match isolation {
        crate::vocab::Isolation::Kernel => Json::str("kernel"),
        crate::vocab::Isolation::Sandbox(h) => Json::obj([("sandbox", Json::str(h.clone()))]),
        crate::vocab::Isolation::External => Json::str("external"),
    };
    Json::obj([
        (
            "validator_ref",
            Json::str(verdict_target.validator_ref.version_id.clone()),
        ),
        ("target", Json::str(verdict_target.target.clone())),
        ("phase", Json::str(verdict_target.phase.as_str())),
        (
            "criterion_ref",
            verdict_target
                .criterion_ref
                .as_ref()
                .map_or(Json::Null, |r| Json::str(r.clone())),
        ),
        (
            "contract_id",
            verdict_target
                .contract_id
                .as_ref()
                .map_or(Json::Null, |r| Json::str(r.clone())),
        ),
        (
            "evidence_refs",
            Json::Arr(verdict_target.evidence_refs.iter().map(Json::str).collect()),
        ),
        (
            "inputs_digest",
            Json::str(verdict_target.inputs_digest.clone()),
        ),
        ("isolation", isolation_json),
        ("charged_to", Json::str(verdict_target.charged_to.name())),
    ])
}

/// `verification.validator.verdict` — the `Verdict` record payload
/// (ADR-0110 D3; the critic extensions `bundle_id`, `independence_summary`,
/// `calibration_ref`, `uncited_findings` ride the optional members).
pub fn validator_verdict(v: &Verdict) -> Json {
    let (status, status_detail) = match &v.status {
        VerdictStatus::Decided => (Json::str("decided"), Json::Null),
        VerdictStatus::Inconclusive(r) => (Json::str("inconclusive"), Json::str(r.as_str())),
        VerdictStatus::OracleFailure(c) => (Json::str("oracle_failure"), Json::str(c.as_str())),
    };
    Json::obj([
        ("verdict_id", Json::str(v.verdict_id.clone())),
        (
            "validator_ref",
            Json::str(v.validator_ref.version_id.clone()),
        ),
        ("oracle_class", Json::str(v.oracle_class.as_str())),
        ("target", Json::str(v.target.clone())),
        (
            "criterion_ref",
            v.criterion_ref
                .as_ref()
                .map_or(Json::Null, |r| Json::str(r.clone())),
        ),
        (
            "contract_id",
            v.contract_id
                .as_ref()
                .map_or(Json::Null, |r| Json::str(r.clone())),
        ),
        ("phase", Json::str(v.phase.as_str())),
        ("role", Json::str(v.role.as_str())),
        ("value", verdict_value_json(&v.value)),
        ("status", status),
        ("status_detail", status_detail),
        ("detector", Json::str(detector_name(v.detector))),
        (
            "evidence_refs",
            Json::Arr(v.evidence_refs.iter().map(Json::str).collect()),
        ),
        ("inputs_digest", Json::str(v.inputs_digest.clone())),
        ("evidence_head_seq", Json::Int(v.evidence_head_seq as i64)),
        ("freshness_ok", Json::Bool(v.freshness_ok)),
        (
            "findings",
            Json::Arr(
                v.findings
                    .iter()
                    .map(|f| {
                        Json::obj([
                            ("code", Json::str(f.code.clone())),
                            ("severity", Json::str(f.severity.as_str())),
                            (
                                "location",
                                f.location
                                    .as_ref()
                                    .map_or(Json::Null, |l| Json::str(l.clone())),
                            ),
                            (
                                "evidence_ref",
                                f.evidence_ref
                                    .as_ref()
                                    .map_or(Json::Null, |r| Json::str(r.clone())),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("cost_ppm", Json::Int(v.cost_ppm as i64)),
        ("charged_to", Json::str(v.charged_to.name())),
        (
            "veto_tripped",
            Json::Arr(v.veto_tripped.iter().map(Json::str).collect()),
        ),
        ("measured_at", Json::Int(v.measured_at as i64)),
    ])
}

/// `verification.completion.proposed` — `{by, decision_point}` (ADR-0014/
/// ADR-0110 D7).
pub fn completion_proposed(by: &str, decision_point: &str) -> Json {
    Json::obj([
        ("by", Json::str(by)),
        ("decision_point", Json::str(decision_point)),
    ])
}

/// `verification.completion.decided` — `{status, summary}` (ADR-0110 D7).
pub fn completion_decided(decision: &CompletionDecision) -> Json {
    Json::obj([
        ("run_id", Json::str(decision.run_id.clone())),
        ("status", Json::str(decision.status.clone())),
        (
            "stratum",
            decision
                .stratum
                .as_ref()
                .map_or(Json::Null, |s| Json::str(s.clone())),
        ),
        ("gate_ref", Json::str(decision.gate_ref.clone())),
        (
            "contract_id",
            Json::str(decision.summary.contract_id.clone()),
        ),
        (
            "required",
            Json::obj([
                ("pass", Json::Int(decision.summary.required_pass as i64)),
                ("fail", Json::Int(decision.summary.required_fail as i64)),
                (
                    "inconclusive",
                    Json::Int(decision.summary.required_inconclusive as i64),
                ),
                (
                    "not_run",
                    Json::Int(decision.summary.required_not_run as i64),
                ),
            ]),
        ),
        (
            "invariants_tripped",
            Json::Arr(
                decision
                    .summary
                    .invariants_tripped
                    .iter()
                    .map(Json::str)
                    .collect(),
            ),
        ),
        (
            "evidence_head_seq",
            Json::Int(decision.summary.evidence_head_seq as i64),
        ),
        ("freshness_ok", Json::Bool(decision.summary.freshness_ok)),
    ])
}

/// `verification.artefact.followed` — `{detector, detector_ref =
/// validator_ref, verdict, confidence, evidence_ref}` (ADR-0014 as amended;
/// the deterministic `followed` detector table is §5f.1 §2.6).
pub fn artefact_followed(
    detector: crate::vocab::Detector,
    detector_ref: &str,
    verdict: &Verdict,
    confidence_ppm: u64,
    evidence_ref: &str,
) -> Json {
    Json::obj([
        ("detector", Json::str(detector_name(detector))),
        ("detector_ref", Json::str(detector_ref)),
        ("verdict", verdict_value_json(&verdict.value)),
        ("confidence_ppm", Json::Int(confidence_ppm as i64)),
        ("evidence_ref", Json::str(evidence_ref)),
    ])
}

/// The `Detector` canonical spelling (`deterministic | judged | human` —
/// one sum for `Verdict` and the chain events, CF-483).
pub fn detector_name(d: crate::vocab::Detector) -> &'static str {
    match d {
        crate::vocab::Detector::Deterministic => "deterministic",
        crate::vocab::Detector::Judged => "judged",
        crate::vocab::Detector::Human => "human",
    }
}

/// The `VerdictValue` payload rendering.
pub fn verdict_value_json(v: &crate::vocab::VerdictValue) -> Json {
    match v {
        crate::vocab::VerdictValue::Bool(b) => Json::Bool(*b),
        crate::vocab::VerdictValue::Graded(g) => Json::Int(*g as i64),
        crate::vocab::VerdictValue::Lattice(l) => Json::str(l.as_str()),
        crate::vocab::VerdictValue::ThreeValued(t) => Json::str(t.as_str()),
        crate::vocab::VerdictValue::Vector(j) => j.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claims::{Claim, ReconciliationRecord};
    use crate::gate::{evaluate_gate, GateFacts, OpenEffect};
    use crate::vocab::{
        Agreement, ClaimKind, ExtractedBy, ReconcileMode, SeverityLevel, SubjectRef,
    };
    use hh_provenance::authority::PersistenceScope;
    use hh_provenance::origin::Origin;
    use hh_provenance::record::ProvenanceRecord;

    fn prov() -> ProvenanceRecord {
        ProvenanceRecord::minted(Origin::model("m", "r", "call:1"), PersistenceScope::Run, 1)
    }

    #[test]
    fn claim_recorded_payload_is_canonical() {
        let c = Claim {
            claim_id: "c1".into(),
            run_id: "run:1".into(),
            model_call_id: "call:1".into(),
            at_seq: 10,
            kind: ClaimKind::Achieved,
            subject: SubjectRef::Run,
            predicate: "done".into(),
            asserted: Json::Bool(true),
            evidence_refs: vec!["evt:1".into()],
            extracted_by: ExtractedBy::Structured("f".into()),
            extraction_confidence_ppm: 1_000_000,
            criteria_status: vec![],
            provenance: prov(),
        };
        let s = claim_recorded(&c).to_canonical_string();
        assert!(s.contains("\"kind\":\"achieved\""));
        assert!(s.contains("\"evidence_class\":\"claimed\""));
        assert!(s.contains("\"extracted_by\":{\"kind\":\"structured\""));
    }

    #[test]
    fn reconciled_payload_carries_the_record_fields() {
        let r = ReconciliationRecord {
            record_id: "recon:c1".into(),
            claim_id: "c1".into(),
            agreement: Agreement::Diverge(crate::vocab::DivergenceClass::PhantomEffect),
            severity: SeverityLevel::High,
            detector: crate::vocab::Detector::Deterministic,
            detector_ref: "hir/kernel/reconcile:0".into(),
            confidence_ppm: 1_000_000,
            evidence_refs: vec!["evt:e1".into()],
            probe_effect_ids: vec![],
            reconciled_at_seq: 20,
            mode: ReconcileMode::LedgerOnly,
            charged_to: crate::vocab::ChargedTo::Subject,
            provenance: prov(),
        };
        let s = claim_reconciled(&r).to_canonical_string();
        assert!(s.contains("\"agreement\":\"diverge\""));
        assert!(s.contains("\"divergence_class\":\"phantom_effect\""));
        assert!(s.contains("\"detector\":\"deterministic\""));
        assert!(s.contains("\"mode\":\"ledger_only\""));
    }

    #[test]
    fn gate_evaluated_payload_is_durable_and_replayable() {
        let mut f = GateFacts {
            completion_claim_kind: ClaimKind::Achieved,
            completion_claim_agreement: Agreement::Agree,
            claim_evidence_refs: vec![],
            open_effects: vec![OpenEffect {
                effect_id: "e:1".into(),
                state: "committed".into(),
                detachable: false,
            }],
            abandoned_effects: vec![],
            required_criteria: vec![],
            claim_evidence_divergences: vec![],
            holds_consumed: 0,
            holds_cap: 3,
        };
        let r = evaluate_gate(&f);
        let s = gate_evaluated(&r, "claim:c1", "budget:run1").to_canonical_string();
        assert!(s.contains("\"verdict\":\"hold\""));
        assert!(s.contains("\"budget_dimension\":\"reconciliation.holds\""));
        // Determinism: identical facts ⇒ identical payload bytes.
        f.holds_consumed = 0;
        let r2 = evaluate_gate(&f);
        assert_eq!(
            s,
            gate_evaluated(&r2, "claim:c1", "budget:run1").to_canonical_string()
        );
    }
}
