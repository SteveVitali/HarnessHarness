//! S3.8 procedure-test instrument evidence — `test_procedure` (§5c.5).
//!
//! Covered acceptance ids:
//!
//! - **AC-R-2.4.5-6** — tests under budget: `test_procedure` refuses an arm
//!   without `MatchSpec` (`MissingMatchSpec`) and an arm without an
//!   `eval_budget` (`UnbudgetedArm`); a valid procedure whose fixture's tool
//!   version is revoked reports `stale-by-dependency` and validity
//!   `unknown`, never `valid`.

use std::collections::BTreeMap;

use hh_budget::{DimensionId, MatchSpec};
use hh_hir::document::Node;
use hh_hir::kinds::EntityKind;
use hh_hir::leaves::Text;
use hh_hir::procedure::{CompilationTarget, SelectCtx};
use hh_hir::records::{KindRecord, ProcedureRecord, ProcedureStep};
use hh_lab::experiment::ExperimentRefusal;
use hh_lab::procedure_test::{
    test_procedure, ConformanceValidity, FixtureValidity, ProcedureTestSpec, ValidatorRun,
};
use hh_wire::json::Json;

/// A minimal procedure node (one instruction step).
fn proc() -> Node {
    let mut n = Node::new(
        EntityKind::Procedure,
        KindRecord::Procedure(ProcedureRecord {
            preconditions: Json::Null,
            steps: vec![ProcedureStep::Instruction(Text::new(
                "do the thing",
                "test",
                hh_provenance::ProvenanceRecord::kernel("test", 0),
            ))],
            expected_evidence: Json::Arr(vec![Json::str("ok")]),
            allowed_capabilities: vec![],
            failure_handlers: Json::Arr(vec![]),
        }),
        hh_provenance::ProvenanceRecord::kernel("test", 0),
    );
    n.version.semantic_id = Some("test:proc".into());
    n
}

fn matched() -> MatchSpec {
    MatchSpec::matched_cap(&[DimensionId::ModelCalls])
}

fn budgeted() -> BTreeMap<String, u64> {
    [("model_calls".to_string(), 10)].into_iter().collect()
}

fn spec(p: &Node) -> ProcedureTestSpec<'_> {
    ProcedureTestSpec {
        procedure: p,
        profile: None,
        target: Some(CompilationTarget::Instruction),
        validators: vec![ValidatorRun {
            validator_ref: "test:validator".into(),
            verdict: Json::obj([("pass", Json::Bool(true))]),
            run_id: "run:1".into(),
            cost: [("model_calls".to_string(), 2)].into_iter().collect(),
        }],
        activated: true,
        followed: true,
        match_spec: Some(matched()),
        eval_budget: budgeted(),
        fixture_validity: FixtureValidity::Fresh,
        select_ctx: SelectCtx::default(),
    }
}

#[test]
fn ac_r_2_4_5_6_refuses_an_arm_without_match_spec() {
    let p = proc();
    let mut s = spec(&p);
    s.match_spec = None;
    assert!(matches!(
        test_procedure(&s, &BTreeMap::new()),
        Err(ExperimentRefusal::MissingMatchSpec { .. })
    ));
}

#[test]
fn ac_r_2_4_5_6_refuses_an_unbudgeted_arm() {
    let p = proc();
    let mut s = spec(&p);
    s.eval_budget = BTreeMap::new();
    assert!(matches!(
        test_procedure(&s, &BTreeMap::new()),
        Err(ExperimentRefusal::UnbudgetedArm { .. })
    ));
}

#[test]
fn ac_r_2_4_5_6_revoked_fixture_is_stale_by_dependency_never_valid() {
    let p = proc();
    let mut s = spec(&p);
    s.fixture_validity = FixtureValidity::Revoked {
        dependency: "tool:test@1.2.3".into(),
    };
    let (report, conformance) =
        test_procedure(&s, &BTreeMap::new()).expect("staleness is a report, not a refusal");
    assert_eq!(report.outcome_class, "stale-by-dependency");
    assert_eq!(conformance.validity, ConformanceValidity::Unknown);
    assert_eq!(
        conformance.stale_reason.as_deref(),
        Some("stale-by-dependency:tool:test@1.2.3")
    );
    assert_ne!(
        conformance.validity.as_str(),
        "valid",
        "a revoked fixture can never report valid"
    );
}

#[test]
fn ac_r_2_4_5_6_fresh_fixture_reports_valid_with_cost() {
    let p = proc();
    let s = spec(&p);
    let (report, conformance) = test_procedure(&s, &BTreeMap::new()).unwrap();
    assert_eq!(report.outcome_class, "completed");
    assert_eq!(conformance.validity, ConformanceValidity::Valid);
    assert!(report.activated && report.followed);
    assert_eq!(report.per_validator.len(), 1);
    assert_eq!(report.cost.get("model_calls"), Some(&2));
    assert_eq!(report.target, CompilationTarget::Instruction);
}

#[test]
fn ac_r_2_4_5_6_undecidable_fixture_is_unknown() {
    let p = proc();
    let mut s = spec(&p);
    s.fixture_validity = FixtureValidity::Undecidable {
        detail: "ref unresolved at seq 12".into(),
    };
    let (_, conformance) = test_procedure(&s, &BTreeMap::new()).unwrap();
    assert_eq!(conformance.validity, ConformanceValidity::Unknown);
}
