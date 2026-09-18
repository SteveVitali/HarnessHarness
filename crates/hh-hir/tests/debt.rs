//! `AssumptionDebtRecord/1` + `validate_removal_test`/`validate_for_home`
//! deterministic battery (R-2.9.6⁰ᵃ; S1.24): per-home completeness and
//! `UnknownDebtHome` (AC-R-2.9.6-1), the member-level + context-parameterized
//! removal-test refusal set (AC-R-2.9.6-2 static half), `InsufficientRunway`/
//! `OwnerUnreachable` (AC-R-2.9.6-8), the legacy decode mappings, the derived
//! `evidence_grade`, and the verdict/retirement codecs.

mod common;

use std::collections::BTreeSet;

use common::*;

use hh_hir::debt::{
    validate_for_home, validate_removal_test, DebtError, RemovalTestContext, RetirementRecord,
    TemplateView,
};
use hh_hir::{AssumptionDebtRecord, RemovalTest, RemovalTestKind};
use hh_ontology::debt::{
    debt_home, evidence_grade, DebtClass, DebtPolicy, DebtScope, DebtStatus, EvidenceGrade,
    EvidenceKind, EvidenceRef, ExpiryCondition, ExpiryKind, RemovalVerdict, UnexecutableReason,
    Verdict,
};
use hh_wire::Json;

fn policy() -> DebtPolicy {
    DebtPolicy::default()
}

fn ctx() -> RemovalTestContext<'static> {
    RemovalTestContext::member_level()
}

fn harness_home() -> &'static hh_ontology::debt::DebtHome {
    debt_home("harness_rule", "assumption_debt").expect("the harness_rule home exists")
}

// ── AC-R-2.9.6-1 — per-home completeness + UnknownDebtHome ───────────────────

#[test]
fn unknown_debt_home_refuses() {
    let d = debt_record("rule:1", 0);
    assert!(matches!(
        validate_for_home(&d, "not_a_home", "debt", &policy(), &ctx()),
        Err(DebtError::UnknownDebtHome { .. })
    ));
    // The `harness_rule.assumption_debt` home is listed (DebtHomes/1 id 1).
    validate_for_home(&d, "harness_rule", "assumption_debt", &policy(), &ctx()).unwrap();
}

#[test]
fn per_home_required_fields_refuse() {
    let home = harness_home();
    let missing = |d: &AssumptionDebtRecord| -> String {
        match validate_for_home(d, "harness_rule", "assumption_debt", &policy(), &ctx()) {
            Err(DebtError::MissingField { home: h, field }) => {
                assert_eq!(h, "harness_rule");
                field
            }
            other => panic!("expected MissingField, got {other:?} (home {home:?})"),
        }
    };

    // The ratified base members.
    let mut d = debt_record("rule:1", 0);
    d.rule_id = String::new();
    assert_eq!(missing(&d), "rule_id");
    let mut d = debt_record("rule:1", 0);
    d.evidence_refs = vec![];
    assert_eq!(missing(&d), "evidence_refs");
    let mut d = debt_record("rule:1", 0);
    d.removal_test_ref = String::new();
    assert_eq!(missing(&d), "removal_test_ref");

    // The `/1` per-home members (`debt_class`, `removal_test`, and —
    // `needs_model_scope` — `scope.model_selectors`).
    let mut d = debt_record("rule:1", 0);
    d.debt_class = None;
    assert_eq!(missing(&d), "debt_class");
    let mut d = debt_record("rule:1", 0);
    d.removal_test = None;
    assert_eq!(missing(&d), "removal_test");
    let mut d = debt_record("rule:1", 0);
    d.scope = None;
    assert_eq!(missing(&d), "scope.model_selectors");
}

// ── AC-R-2.9.6-2 (static half) — the removal-test refusal battery ────────────

#[test]
fn removal_test_member_level_battery() {
    let home = harness_home();

    // `missing_payload` — a kind without its mandatory member never
    // instantiates (`RetirementExperiment` needs `template_ref`).
    let mut d = debt_record("rule:1", 0);
    d.removal_test = Some(RemovalTest::new(RemovalTestKind::RetirementExperiment));
    assert!(matches!(
        validate_removal_test(&d, home, &ctx()),
        Err(DebtError::UnexecutableRemovalTest {
            reason: UnexecutableReason::MissingPayload,
            ..
        })
    ));

    // `missing_snapshot_scope` — a model-conditioned debt with no
    // `scope.model_selectors`.
    let mut d = debt_record("rule:1", 0);
    d.debt_class = Some(DebtClass::ModelConditioned);
    d.scope = Some(DebtScope::default());
    assert!(matches!(
        validate_removal_test(&d, home, &ctx()),
        Err(DebtError::UnexecutableRemovalTest {
            reason: UnexecutableReason::MissingSnapshotScope,
            ..
        })
    ));

    // `beneficiaries_outside_scope` — a claimed beneficiary the scope does
    // not cover.
    let mut d = debt_record("rule:1", 0);
    d.removal_test = Some(RemovalTest {
        beneficiaries: vec!["task:other_class".into()],
        ..RemovalTest::new(RemovalTestKind::Inspection)
    });
    d.removal_test.as_mut().unwrap().criteria = Some("check".into());
    assert!(matches!(
        validate_removal_test(&d, home, &ctx()),
        Err(DebtError::UnexecutableRemovalTest {
            reason: UnexecutableReason::BeneficiariesOutsideScope,
            ..
        })
    ));
}

#[test]
fn removal_test_context_refusals() {
    let home = harness_home();

    // A `retirement_experiment` test with a template ref.
    let retire = |d: &mut AssumptionDebtRecord| {
        d.removal_test = Some(RemovalTest {
            template_ref: Some("template:1".into()),
            ..RemovalTest::new(RemovalTestKind::RetirementExperiment)
        });
    };

    // `unresolved_template` — the resolver reports nothing.
    let mut d = debt_record("rule:1", 0);
    retire(&mut d);
    let resolve_none = |_r: &str| -> Option<TemplateView> { None };
    let ctx = RemovalTestContext {
        resolve_template: Some(&resolve_none),
        ..RemovalTestContext::member_level()
    };
    assert!(matches!(
        validate_removal_test(&d, home, &ctx),
        Err(DebtError::UnexecutableRemovalTest {
            reason: UnexecutableReason::UnresolvedTemplate,
            ..
        })
    ));

    // `missing_match_spec` — the resolved template's arms carry none.
    let mut d = debt_record("rule:1", 0);
    retire(&mut d);
    let no_match = |_r: &str| -> Option<TemplateView> {
        Some(TemplateView {
            experiment_ref: "exp:1".into(),
            has_match_spec: false,
        })
    };
    let ctx = RemovalTestContext {
        resolve_template: Some(&no_match),
        ..RemovalTestContext::member_level()
    };
    assert!(matches!(
        validate_removal_test(&d, home, &ctx),
        Err(DebtError::UnexecutableRemovalTest {
            reason: UnexecutableReason::MissingMatchSpec,
            ..
        })
    ));

    // `not_a_retirement_diff` — the candidate diff removes more/less than
    // `{rule_id}`.
    let mut d = debt_record("rule:1", 0);
    retire(&mut d);
    let diff: BTreeSet<String> = ["rule:other".to_string()].into_iter().collect();
    let ctx = RemovalTestContext {
        diff_removed_rules: Some(&diff),
        ..RemovalTestContext::member_level()
    };
    assert!(matches!(
        validate_removal_test(&d, home, &ctx),
        Err(DebtError::UnexecutableRemovalTest {
            reason: UnexecutableReason::NotARetirementDiff,
            ..
        })
    ));
    let expected: BTreeSet<String> = ["rule:1".to_string()].into_iter().collect();
    let ctx_ok = RemovalTestContext {
        diff_removed_rules: Some(&expected),
        ..RemovalTestContext::member_level()
    };
    validate_removal_test(&d, home, &ctx_ok).unwrap();

    // `missing_split_assignment`.
    let mut d = debt_record("rule:1", 0);
    retire(&mut d);
    let ctx = RemovalTestContext {
        split_assigned: Some(false),
        ..RemovalTestContext::member_level()
    };
    assert!(matches!(
        validate_removal_test(&d, home, &ctx),
        Err(DebtError::UnexecutableRemovalTest {
            reason: UnexecutableReason::MissingSplitAssignment,
            ..
        })
    ));

    // `unresolved_probe_ref` — a probe the context does not know.
    let mut d = debt_record("rule:1", 0);
    d.removal_test = Some(RemovalTest {
        probe_refs: vec!["probe:ghost".into()],
        ..RemovalTest::new(RemovalTestKind::ProbeRun)
    });
    let unknown = |_r: &str| false;
    let ctx = RemovalTestContext {
        probe_ref_known: Some(&unknown),
        ..RemovalTestContext::member_level()
    };
    assert!(matches!(
        validate_removal_test(&d, home, &ctx),
        Err(DebtError::UnexecutableRemovalTest {
            reason: UnexecutableReason::UnresolvedProbeRef,
            ..
        })
    ));

    // `unresolved_zero_use_scope` — the `zero_uses` scope ref does not
    // resolve.
    let mut d = debt_record("rule:1", 0);
    d.removal_test = Some(RemovalTest {
        scope_ref: Some("scope:ghost".into()),
        ..RemovalTest::new(RemovalTestKind::ZeroUses)
    });
    let unresolved = |_r: &str| false;
    let ctx = RemovalTestContext {
        zero_use_scope_resolved: Some(&unresolved),
        ..RemovalTestContext::member_level()
    };
    assert!(matches!(
        validate_removal_test(&d, home, &ctx),
        Err(DebtError::UnexecutableRemovalTest {
            reason: UnexecutableReason::UnresolvedZeroUseScope,
            ..
        })
    ));

    // `unsealed_artifact` — the context reports an unsealed artifact.
    let d = debt_record("rule:1", 0);
    let ctx = RemovalTestContext {
        artifact_sealed: Some(false),
        ..RemovalTestContext::member_level()
    };
    assert!(matches!(
        validate_removal_test(&d, home, &ctx),
        Err(DebtError::UnexecutableRemovalTest {
            reason: UnexecutableReason::UnsealedArtifact,
            ..
        })
    ));
}

// ── AC-R-2.9.6-8 — InsufficientRunway / OwnerUnreachable ─────────────────────

#[test]
fn insufficient_runway_and_owner_unreachable() {
    let home = harness_home();

    // `InsufficientRunway` — `until − created_at < policy.min_runway` on a
    // `date`-bounded `expiry`.
    let mut d = debt_record("rule:1", 0);
    let mut p = policy();
    p.min_runway_ms = 1_000;
    d.expiry = Some(hh_ontology::debt::DebtExpiry {
        condition: ExpiryKind::Date,
        params: hh_ontology::debt::ExpiryParams {
            until: Some(500),
            ..Default::default()
        },
    });
    d.created_at = Some(0);
    let ctx = RemovalTestContext {
        policy: Some(&p),
        ..RemovalTestContext::member_level()
    };
    assert!(matches!(
        validate_removal_test(&d, home, &ctx),
        Err(DebtError::InsufficientRunway { .. })
    ));
    // Enough runway passes.
    let mut d2 = d.clone();
    d2.expiry = Some(hh_ontology::debt::DebtExpiry {
        condition: ExpiryKind::Date,
        params: hh_ontology::debt::ExpiryParams {
            until: Some(5_000),
            ..Default::default()
        },
    });
    validate_removal_test(&d2, home, &ctx).unwrap();

    // `OwnerUnreachable` — `reach_via` sinks disjoint from the declared set.
    let mut d = debt_record("rule:1", 0);
    d.owner.reach_via = vec!["pager".into()];
    let sinks: BTreeSet<String> = ["email".to_string()].into_iter().collect();
    let ctx = RemovalTestContext {
        declared_sinks: Some(&sinks),
        ..RemovalTestContext::member_level()
    };
    assert!(matches!(
        validate_removal_test(&d, home, &ctx),
        Err(DebtError::OwnerUnreachable { .. })
    ));
    // A reachable owner passes.
    let sinks2: BTreeSet<String> = ["pager".to_string()].into_iter().collect();
    let ctx2 = RemovalTestContext {
        declared_sinks: Some(&sinks2),
        ..RemovalTestContext::member_level()
    };
    validate_removal_test(&d, home, &ctx2).unwrap();
}

// ── legacy decode mappings + derived evidence_grade ──────────────────────────

#[test]
fn legacy_spellings_decode_with_the_documented_mapping() {
    // `DebtStatus` legacy spellings (CC8 decode tolerance; canonical emit).
    assert_eq!(DebtStatus::parse_legacy("open"), Some(DebtStatus::Active));
    assert_eq!(
        DebtStatus::parse_legacy("discharged"),
        Some(DebtStatus::Retired)
    );
    assert_eq!(
        DebtStatus::parse_legacy("violated"),
        Some(DebtStatus::Expired)
    );
    assert_eq!(DebtStatus::parse_legacy("active"), Some(DebtStatus::Active));
    assert_eq!(DebtStatus::parse_legacy("bogus"), None);

    // `ExpiryCondition` — `{kind, value?}` and the bare `"{kind}"` spelling.
    let bare = ExpiryCondition::from_json(&Json::str("date"), "exp").unwrap();
    assert_eq!(bare.kind, ExpiryKind::Date);
    let full = ExpiryCondition::from_json(
        &Json::obj([("kind", Json::str("model_version_change"))]),
        "exp",
    )
    .unwrap();
    assert_eq!(full.kind, ExpiryKind::ModelVersionChange);
    assert!(ExpiryCondition::from_json(&Json::str("never"), "exp").is_err());
}

#[test]
fn evidence_grade_derives_from_instrument_refs() {
    let instrument = EvidenceRef {
        kind: EvidenceKind::ConformanceReport,
        ..EvidenceRef::legacy("sha256:i1")
    };
    let instrument2 = EvidenceRef {
        kind: EvidenceKind::ExperimentReport,
        ..EvidenceRef::legacy("sha256:i2")
    };
    let prose = EvidenceRef::legacy("sha256:note");
    assert_eq!(evidence_grade(&[]), EvidenceGrade::Hypothesized);
    assert_eq!(
        evidence_grade(std::slice::from_ref(&prose)),
        EvidenceGrade::Hypothesized
    );
    assert_eq!(
        evidence_grade(std::slice::from_ref(&instrument)),
        EvidenceGrade::Evidenced
    );
    assert_eq!(
        evidence_grade(&[instrument, instrument2]),
        EvidenceGrade::Confirmed
    );
}

// ── verdict / retirement codecs ──────────────────────────────────────────────

#[test]
fn verdict_and_retirement_round_trip() {
    for v in [Verdict::Pass, Verdict::Fail, Verdict::Inconclusive] {
        assert_eq!(Verdict::parse(v.name()), Some(v));
    }
    assert_eq!(Verdict::parse("maybe"), None);

    let rv = RemovalVerdict {
        debt_ref: "debt:1".into(),
        kind: RemovalTestKind::RetirementExperiment,
        verdict: Verdict::Inconclusive,
        reason: Some("suite unavailable".into()),
        report_ref: "report:1".into(),
        settled_at: 12,
    };
    let j = rv.to_json();
    assert_eq!(RemovalVerdict::from_json(&j, "verdict").unwrap(), rv);

    let ret = RetirementRecord {
        removal_test_report_ref: "report:1".into(),
        verdict_ref: "verdict:1".into(),
        decided_by: hh_provenance::ProvenanceRecord::minted(
            hh_provenance::Origin::human("a", hh_provenance::HumanRole::Principal),
            hh_provenance::PersistenceScope::Run,
            0,
        ),
        rationale: text("the removal test passed twice", 3),
    };
    let j = ret.to_json(false);
    // The retirement record re-encodes its declared members.
    let Json::Obj(m) = &j else { panic!("object") };
    for k in [
        "removal_test_report_ref",
        "verdict_ref",
        "decided_by",
        "rationale",
    ] {
        assert!(m.contains_key(k), "{k}");
    }
}

// ── the seal-time battery (AC-R-2.9.6-2 static half at `hh_hir::validate`) ───

#[test]
fn seal_refuses_an_uninstantiable_removal_test() {
    // A conditioned HarnessRule whose typed `removal_test` lacks its
    // mandatory payload refuses at `validate` — `RemovalTestRefusal`
    // (§5h.6 §4; the member-level battery runs at seal).
    let mut doc = valid_doc();
    let mut d = debt_record("test:crule.rule", 7);
    d.removal_test = Some(RemovalTest::new(RemovalTestKind::RetirementExperiment));
    doc.nodes
        .push(conditioned_rule_node("test:crule", 7, Some(d)));
    let es = hh_hir::validate(&doc).expect_err("uninstantiable removal test");
    assert!(es
        .iter()
        .any(|e| matches!(e, hh_hir::errors::HirError::RemovalTestRefusal { .. })));

    // A record missing a per-home required field stays
    // `ConditionedRuleIncomplete` (the completeness half).
    let mut doc = valid_doc();
    let mut d = debt_record("test:crule.rule", 7);
    d.scope = None;
    doc.nodes
        .push(conditioned_rule_node("test:crule", 7, Some(d)));
    let es = hh_hir::validate(&doc).expect_err("incomplete debt record");
    assert!(es.iter().any(|e| matches!(
        e,
        hh_hir::errors::HirError::ConditionedRuleIncomplete { .. }
    )));

    // The complete record validates.
    let mut doc = valid_doc();
    doc.nodes.push(conditioned_rule_node(
        "test:crule",
        7,
        Some(debt_record("test:crule.rule", 7)),
    ));
    hh_hir::validate(&doc).unwrap();
}

// ── the `debt_json`/`debt_from_json` codec (CC7 — one schema source) ─────────

#[test]
fn debt_codec_round_trips_and_decodes_legacy_bodies() {
    let d = debt_record("rule:1", 0);
    // The canonical encoding emits the `/1` additive members only when
    // present (CC8) and re-decodes to the same record.
    let j = hh_hir::debt_json(&d, false);
    let back = hh_hir::debt_from_json(&j, "debt").unwrap();
    // The `Text` leaf serializes hash-only — round-trip equality holds at
    // the JSON level (the decoded record re-encodes byte-identically).
    assert_eq!(hh_hir::debt_json(&back, false), j);
    // The semantic projection (identity hashing) is deterministic and
    // differs from the versioned encoding (content elided).
    let js = hh_hir::debt_json(&d, true);
    assert_eq!(js, hh_hir::debt_json(&d, true));

    // A legacy body — bare `owner`/`expiry_condition`/`evidence_refs`
    // spellings and a legacy `status` — decodes per the `/1` field notes.
    let legacy = Json::obj([
        ("rule_id", Json::str("rule:1")),
        ("hypothesis", d.hypothesis.to_json()),
        ("evidence_refs", Json::Arr(vec![Json::str("sha256:ev")])),
        ("owner", Json::str("test:owner")),
        ("expiry_condition", Json::str("date")),
        ("removal_test_ref", Json::str("sha256:test")),
        ("status", Json::str("open")),
    ]);
    let decoded = hh_hir::debt_from_json(&legacy, "debt").unwrap();
    assert_eq!(decoded.status, DebtStatus::Active);
    assert!(!decoded.owner.team);
    assert_eq!(decoded.expiry_condition.kind, ExpiryKind::Date);
    assert_eq!(decoded.evidence_refs.len(), 1);
    // An unknown status spelling refuses, never defaults.
    let mut bad = legacy.clone();
    if let Json::Obj(ref mut m) = bad {
        m.insert("status".into(), Json::str("bogus"));
    }
    assert!(hh_hir::debt_from_json(&bad, "debt").is_err());
}
