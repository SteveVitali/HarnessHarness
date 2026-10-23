//! `r4_7d` — the AC-R-2.7.3-10 lowering half over the emitted payload: the
//! canonical `verification.validator.verdict` payload
//! `hh_verification::events::validator_verdict` emits for a judged verdict
//! (with the critic extensions) lowers to `gen_ai.evaluation.result` and
//! the loss report names the judge-identity/evidence/calibration/
//! independence/cost classes. The reference-corpus halves live in
//! `crates/hh-verification/tests/r4_7d.rs`.

use hh_identity::kinds::RecordKind;
use hh_identity::refs::VersionedRef;
use hh_provenance::authority::PersistenceScope;
use hh_provenance::origin::Origin;
use hh_provenance::record::ProvenanceRecord;
use hh_telemetry::genai::lower_verdict;
use hh_verification::events::validator_verdict;
use hh_verification::validators::Verdict;
use hh_verification::vocab::{
    ChargedTo, CriterionRole, Detector, OracleClass, VerdictPhase, VerdictStatus, VerdictValue,
};
use hh_wire::Json;

/// AC-R-2.7.3-10 — a judged verdict emitted by the canonical payload
/// builder lowers to `gen_ai.evaluation.result` with name/score/label/
/// response-id round-trips; the loss report names judge identity,
/// evidence, calibration, independence and cost.
#[test]
fn ac_r_2_7_3_10_emitted_judged_verdict_lowers_with_named_loss_classes() {
    let verdict = Verdict {
        verdict_id: "v-j1".into(),
        validator_ref: VersionedRef::pinned(
            RecordKind::Validator,
            "sha256:crt",
            ProvenanceRecord::minted(
                Origin::kernel("hir/kernel/registry"),
                PersistenceScope::Definition,
                1,
            ),
        ),
        oracle_class: OracleClass::Judge,
        target: "task:refcorpus".into(),
        criterion_ref: Some("crit:1".into()),
        contract_id: Some("contract:1".into()),
        phase: VerdictPhase::Completion,
        role: CriterionRole::Acceptance,
        value: VerdictValue::Bool(true),
        status: VerdictStatus::Decided,
        detector: Detector::Judged,
        evidence_refs: vec!["it:1".into(), "it:2".into()],
        inputs_digest: "sha256:inputs".into(),
        evidence_head_seq: 9,
        freshness_ok: true,
        findings: vec![],
        cost_ppm: 1_000,
        charged_to: ChargedTo::Instrument,
        veto_tripped: vec![],
        bundle_id: Some("b:1".into()),
        calibration_ref: Some("cal:1".into()),
        independence_summary: Some("snapshot=different_family context=fresh".into()),
        uncited_findings: 1,
        provenance: ProvenanceRecord::minted(
            Origin::model("model/ref", "snap:ref", "call:1"),
            PersistenceScope::Run,
            9,
        ),
        measured_at: 9,
    };
    let payload = validator_verdict(&verdict);
    // The emitted payload carries the judge extensions the AC names.
    for member in [
        "bundle_id",
        "calibration_ref",
        "independence_summary",
        "uncited_findings",
    ] {
        assert!(payload.get(member).is_some(), "payload carries {member}");
    }
    let (event, loss) = lower_verdict(9, &payload);
    assert_eq!(
        event.get("name").and_then(Json::as_str),
        Some("gen_ai.evaluation.result")
    );
    let attrs = event.get("attributes").expect("attributes");
    assert_eq!(
        attrs
            .get("gen_ai.evaluation.score.label")
            .and_then(Json::as_str),
        Some("pass")
    );
    assert_eq!(
        attrs.get("gen_ai.response.id").and_then(Json::as_str),
        Some("v-j1")
    );
    // The loss report names the five classes the AC requires — the
    // emitted judged members surface as unrepresentable members, never
    // silently dropped.
    let details: Vec<String> = loss.iter().map(|l| l.detail.clone()).collect();
    for named in [
        "validator_ref",        // judge identity
        "evidence_refs",        // evidence
        "calibration_ref",      // calibration
        "independence_summary", // independence
        "cost_ppm",             // cost
    ] {
        assert!(
            details
                .iter()
                .any(|d| d == &format!("verification.validator.verdict.{named}")),
            "loss names {named}: {details:?}"
        );
    }
    // Determinism — identical input lowers identically.
    assert_eq!((event, loss), lower_verdict(9, &payload));
}
