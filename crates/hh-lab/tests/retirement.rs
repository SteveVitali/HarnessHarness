//! `hh-lab` Stage-3 retirement fixtures — R-2.9.6⁰ᵇ (§5h.6;
//! AC-R-2.9.6-2/-4/-5): a `retirement` `ExperimentSpec` per conditioned
//! rule of the two minimal profiles registers on the held-out split and
//! yields a `ComparisonReport{benefit_kind: artifact_benefit,
//! budget_match.status}` → `RemovalVerdict`; `settle_removal_test` /
//! `retire` produce the §2 transitions; `expired_used` exclusion and the
//! minimal `DebtReport` round out the slice.

use std::collections::BTreeMap;

use hh_budget::matchspec::MatchSpec;
use hh_hir::debt::RetirementRecord;
use hh_hir::leaves::Text;
use hh_ontology::config::Ref;
use hh_ontology::debt::{DebtStatus, OwnerRef, RemovalTestKind, RemovalVerdict, Verdict};
use hh_ontology::eval::{
    Design, DesignKind, FactorKind, Pairing, PreRegistration, RoutingPolicy, SeedPolicy,
};
use hh_ontology::lab::SplitLabel;
use hh_ontology::participant::ParticipantClass;
use hh_provenance::{HumanRole, Origin, ProvenanceRecord};
use hh_wire::Json;

use hh_compiler::profile::minimal_profiles;
use hh_lab::debt::{
    expired_used_payload, headline_admitted, retire, settle_removal_test, AssumptionDebtHealth,
    DebtIndexRow, DebtReport, RetireError, SettleEffect,
};
use hh_lab::experiment::{
    ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentBudgets, ExperimentKind,
    ExperimentRefusal, ExperimentSpec, FactorSpec, LevelSpec, OrderKind, ReattemptPolicy,
    SchedulingPolicy, SpecContext, SuiteBinding, ValidationStrategy,
};

// ── fixtures ────────────────────────────────────────────────────────────────

fn human(author: &str) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human(author, HumanRole::Principal),
        hh_provenance::PersistenceScope::Run,
        1,
    )
}

fn seed_policy() -> SeedPolicy {
    SeedPolicy {
        harness_rng: true,
        requested_sampling_seed: true,
        seed_honoured_required: true,
    }
}

fn pre_registration() -> PreRegistration {
    PreRegistration {
        registered_at: 1,
        hypothesis: "removing the conditioned rule is non-inferior".into(),
        primary_metrics: vec!["task_success".into()],
        equivalence_margin: Some(Json::str("margin:ni")),
        min_n: 1,
        analysis_plan_ref: "analysis:plan".into(),
        task_split_hash: "sha256:cc33".into(),
        interactions: vec![],
    }
}

fn design() -> Design {
    Design {
        id: "design:retirement".into(),
        kind: DesignKind::Paired,
        factors: vec![],
        blocking: vec!["task".into()],
        replicates_per_cell: 5,
        pairing: Pairing::ByTask,
        seed_policy: seed_policy(),
        held_out_split_ref: Some("split:held_out".into()),
        pre_registration: pre_registration(),
        registry_snapshot_id: None,
        generators: None,
        resolution: None,
        routing_policy: RoutingPolicy::FailFast,
        deviation_policy: None,
        cache_na_stratified: false,
    }
}

fn level(id: &str) -> LevelSpec {
    LevelSpec {
        level_id: id.into(),
        ref_: "sha256:dd44".into(),
        overrides: None,
        label: id.into(),
        class: ParticipantClass::Native,
        non_portable: false,
    }
}

/// `arm A` (the conditioned harness) / `arm A−R` (the same definition minus
/// the conditioned rule) — the removal diff the `retirement` kind checks.
fn retirement_arm(id: &str, lid: &str) -> ArmSpec {
    ArmSpec {
        arm_id: id.into(),
        hypothesis: "the arm holds".into(),
        level_assignment: [("model".to_string(), lid.to_string())]
            .into_iter()
            .collect(),
        // Equal eval_budget across arms is the `validate_match` half the
        // retirement match shape requires (AC-R-2.9.6-11).
        eval_budget: "budget:eval".into(),
        search_budget: Some("budget:search".into()),
        match_spec: Some(MatchSpec::matched_cap(&[])),
        artifact_ref: Ref::new("def:x", "sha256:ee55"),
        limits_enforced: "limits:declared".into(),
        model_role_table_ref: None,
        response_cache: None,
    }
}

/// The `ExperimentSpec{kind: retirement}` a rule's `retirement_experiment`
/// removal test instantiates — `A` / `A − R` under
/// `MatchSpec{matched_cap, cold_start}` on the held-out split.
fn retirement_spec(rule_id: &str) -> ExperimentSpec {
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Retirement,
        design: design(),
        pre_registration: Some(pre_registration()),
        factors: vec![FactorSpec {
            name: "model".into(),
            kind: FactorKind::ModelSnapshot,
            granularity: None,
            role: None,
            levels: vec![level("with-rule"), level("without-rule")],
        }],
        arms: vec![
            retirement_arm("a", "with-rule"),
            retirement_arm("a-minus-r", "without-rule"),
        ],
        suite: SuiteBinding {
            suite_ref: "suite:heldout".into(),
            // The Stage-3 held-out split — retirement evidence never
            // measures on the surface the rule was tuned against.
            split_labels_used: vec![SplitLabel::HeldOut],
            split_assignment_ref: Some("split:1".into()),
        },
        replicates_per_cell: 5,
        seed_policy: seed_policy(),
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: SchedulingPolicy {
            max_concurrent_runs: 4,
            pools: vec![],
            order: OrderKind::RandomPermuted,
            permutation_seed: "seed:1".into(),
            start_stagger_ms: 0,
            deadline: None,
            priority: None,
        },
        reattempt: ReattemptPolicy {
            max_per_plan: 2,
            max_fraction_of_plans_ppm: 100_000,
            backoff: Backoff {
                min_ms: 100,
                multiplier_ppm: 2_000_000,
                max_ms: 10_000,
            },
            error_classes_included: None,
            on_cancel: CancelPolicy::Replan,
        },
        budgets: ExperimentBudgets {
            experiment: "budget:exp".into(),
            instrument: "budget:inst".into(),
        },
        bundle_policy: BundlePolicy::Adhoc {
            salt: "salt:1".into(),
        },
        ext: BTreeMap::from([("debt_rule_id".to_string(), Json::str(rule_id))]),
    };
    s.experiment_id = s.experiment_id();
    s
}

fn retirement_ctx() -> SpecContext<'static> {
    SpecContext {
        retirement_diff: Some(true),
        ..SpecContext::member_level()
    }
}

fn retirement_record(
    report_ref: &str,
    verdict_ref: &str,
    decided_by: ProvenanceRecord,
) -> RetirementRecord {
    RetirementRecord {
        removal_test_report_ref: report_ref.to_string(),
        verdict_ref: verdict_ref.to_string(),
        decided_by,
        rationale: Text::new("the removal test passed", "test:owner", human("test:owner")),
    }
}

// ── AC-R-2.9.6-2 — a retirement experiment per conditioned rule ─────────────

/// Every conditioned rule of `minimal-patch` and `minimal-string-replace`
/// carries a `retirement` experiment that registers on the held-out split
/// (`retirement_diff = true` — the arms are the single-rule removal diff).
#[test]
fn every_minimal_profile_rule_registers_a_retirement_experiment() {
    let (patch, replace) = minimal_profiles();
    let mut seen = Vec::new();
    for profile in [&patch, &replace] {
        for rule in &profile.rules {
            let s = retirement_spec(&rule.rule_id);
            s.register(&retirement_ctx()).unwrap_or_else(|e| {
                panic!("rule {} retirement experiment refused: {e:?}", rule.rule_id)
            });
            // The same spec under a non-removal diff view refuses —
            // `NotARetirementDiff` is the shape check, not a warning.
            let not_diff = SpecContext {
                retirement_diff: Some(false),
                ..SpecContext::member_level()
            };
            assert!(matches!(
                s.register(&not_diff),
                Err(ExperimentRefusal::NotARetirementDiff { .. })
            ));
            seen.push(rule.rule_id.clone());
        }
    }
    // Four conditioned rules across the two minimal profiles
    // (`{profile}.naming` + `{profile}.tool_shape.edit_file`).
    assert_eq!(
        seen,
        vec![
            "minimal-patch.naming",
            "minimal-patch.tool_shape.edit_file",
            "minimal-string-replace.naming",
            "minimal-string-replace.tool_shape.edit_file",
        ]
    );
}

/// The settled report → `RemovalVerdict` → `retire` chain (AC-R-2.9.6-2/-4):
/// a `ComparisonReport{benefit_kind: artifact_benefit, budget_match.status:
/// matched}` settles `pass`; `settle_removal_test` records the immutable
/// verdict and marks the rule retirement-eligible (status unchanged until
/// sealed); `retire` under a human-sealed `RetirementRecord` produces the
/// `→ retired` transition with `supersedes{reason: expiry}`.
#[test]
fn settle_and_retire_chain_over_the_minimal_profiles() {
    let (patch, _) = minimal_profiles();
    let rule = &patch.rules[0];
    let debt_ref = format!("debt:{}", rule.rule_id);
    let report_ref = "report:comparison-1";

    // The retirement experiment's outcome — a `pass` verdict citing the
    // `artifact_benefit` ComparisonReport.
    let (verdict, effect, transitions) = settle_removal_test(
        &debt_ref,
        RemovalTestKind::RetirementExperiment,
        report_ref,
        Verdict::Pass,
        None,
        100,
        DebtStatus::Expired,
    );
    assert_eq!(verdict.debt_ref, debt_ref);
    assert_eq!(verdict.verdict, Verdict::Pass);
    assert_eq!(verdict.report_ref, report_ref);
    // `pass` ⇒ eligible — never a status transition (D-5/D7).
    assert_eq!(effect, SettleEffect::RetirementEligible);
    assert!(transitions.is_empty());

    // `retire` — human-sealed, verdict-backed → `expired → retired` with
    // `supersedes{reason: expiry}`.
    let outcome = retire(
        &debt_ref,
        DebtStatus::Expired,
        &[verdict.clone()],
        &retirement_record(report_ref, "verdict:1", human("reviewer:1")),
    )
    .expect("pass verdict + human seal retires");
    assert_eq!(outcome.supersedes_reason, "expiry");
    assert_eq!(outcome.transition.from, DebtStatus::Expired);
    assert_eq!(outcome.transition.to, DebtStatus::Retired);
    assert_eq!(outcome.transition.evidence_ref.as_deref(), Some(report_ref));

    // A second retire never rewrites history.
    assert_eq!(
        retire(
            &debt_ref,
            DebtStatus::Retired,
            &[verdict.clone()],
            &retirement_record(report_ref, "verdict:1", human("reviewer:1")),
        ),
        Err(RetireError::AlreadyRetired {
            debt_ref: debt_ref.clone()
        })
    );
}

/// `settle_removal_test` — `fail` revalidates (the report becomes the
/// `comparison_report` evidence ref; `expired → active`); `inconclusive`
/// reschedules with no transition.
#[test]
fn settle_fail_revalidates_and_inconclusive_reschedules() {
    let debt_ref = "debt:rule-1";
    // fail on an expired record — the rule proved still-needed.
    let (v, effect, transitions) = settle_removal_test(
        debt_ref,
        RemovalTestKind::RetirementExperiment,
        "report:2",
        Verdict::Fail,
        None,
        7,
        DebtStatus::Expired,
    );
    assert_eq!(v.verdict, Verdict::Fail);
    assert_eq!(
        effect,
        SettleEffect::Revalidated {
            evidence_ref: "report:2".into()
        }
    );
    assert_eq!(transitions.len(), 1);
    assert_eq!(transitions[0].from, DebtStatus::Expired);
    assert_eq!(transitions[0].to, DebtStatus::Active);
    assert_eq!(transitions[0].trigger, "removal_test_failed");
    assert_eq!(transitions[0].evidence_ref.as_deref(), Some("report:2"));

    // fail on an already-active record — evidence refreshes, no status move.
    let (_, _, transitions) = settle_removal_test(
        debt_ref,
        RemovalTestKind::RetirementExperiment,
        "report:3",
        Verdict::Fail,
        None,
        8,
        DebtStatus::Active,
    );
    assert!(transitions.is_empty());

    // inconclusive — the test reschedules; the verdict carries the reason.
    let (v, effect, transitions) = settle_removal_test(
        debt_ref,
        RemovalTestKind::RetirementExperiment,
        "report:4",
        Verdict::Inconclusive,
        Some("flaky suite".into()),
        9,
        DebtStatus::Expiring,
    );
    assert_eq!(v.reason.as_deref(), Some("flaky suite"));
    assert_eq!(effect, SettleEffect::Reschedule);
    assert!(transitions.is_empty());
}

/// The retirement gate refusals (AC-R-2.9.6-4): no `pass` verdict ⇒
/// `RetirementNotEvidenced`; a non-human `decided_by` never seals.
#[test]
fn retire_refuses_unevidenced_and_non_human() {
    let debt_ref = "debt:rule-2";
    let fail_verdict = RemovalVerdict {
        debt_ref: debt_ref.into(),
        kind: RemovalTestKind::RetirementExperiment,
        verdict: Verdict::Fail,
        reason: None,
        report_ref: "report:f".into(),
        settled_at: 3,
    };
    assert_eq!(
        retire(
            debt_ref,
            DebtStatus::Expired,
            &[fail_verdict],
            &retirement_record("report:f", "verdict:f", human("reviewer:1")),
        ),
        Err(RetireError::RetirementNotEvidenced {
            debt_ref: debt_ref.into()
        })
    );
    assert_eq!(
        retire(
            debt_ref,
            DebtStatus::Expired,
            &[],
            &retirement_record("report:f", "verdict:f", human("reviewer:1"))
        ),
        Err(RetireError::RetirementNotEvidenced {
            debt_ref: debt_ref.into()
        })
    );
    // A kernel-origin `decided_by` is not a human seal.
    let pass = RemovalVerdict {
        debt_ref: debt_ref.into(),
        kind: RemovalTestKind::RetirementExperiment,
        verdict: Verdict::Pass,
        reason: None,
        report_ref: "report:p".into(),
        settled_at: 4,
    };
    assert_eq!(
        retire(
            debt_ref,
            DebtStatus::Expired,
            &[pass],
            &retirement_record(
                "report:p",
                "verdict:p",
                ProvenanceRecord::kernel("evolution", 1)
            ),
        ),
        Err(RetireError::NotHumanSealed {
            debt_ref: debt_ref.into()
        })
    );
}

/// AC-R-2.9.6-5 — `expired_used` exclusion: the event payload shape, and the
/// headline-admission rule (excluded unless the design declares dead-weight
/// purpose).
#[test]
fn expired_used_payload_and_headline_exclusion() {
    let p = expired_used_payload("debt:rule-3", "intent:compile-under-expired");
    assert_eq!(
        p.get("debt_ref").and_then(Json::as_str),
        Some("debt:rule-3")
    );
    assert_eq!(
        p.get("intent_ref").and_then(Json::as_str),
        Some("intent:compile-under-expired")
    );
    // The exclusion matrix — expired-used rows never headline unless the
    // design declares the dead-weight purpose.
    assert!(!headline_admitted(true, false));
    assert!(headline_admitted(true, true));
    assert!(headline_admitted(false, false));
    assert!(headline_admitted(false, true));
}

/// The minimal `DebtReport` — index rows → `expired_used`/`expiring`/`open`/
/// `not_yet_testable` buckets + the per-kind histogram; codec round-trip.
#[test]
fn minimal_debt_report_buckets_and_round_trip() {
    let health = AssumptionDebtHealth {
        warn_within_ms: 1_000,
    };
    let row =
        |rule: &str, status: DebtStatus, kind: Option<RemovalTestKind>, until: Option<u64>| {
            DebtIndexRow {
                rule_id: rule.into(),
                owner: OwnerRef::principal("test:owner"),
                status,
                removal_test_kind: kind,
                expiry_condition: hh_ontology::debt::ExpiryCondition {
                    kind: hh_ontology::debt::ExpiryKind::Date,
                    value: None,
                },
                created_at: 0,
                expiry_at: until,
            }
        };
    let index = vec![
        row(
            "r.expired",
            DebtStatus::Expired,
            Some(RemovalTestKind::RetirementExperiment),
            Some(10),
        ),
        row(
            "r.expiring",
            DebtStatus::Expiring,
            Some(RemovalTestKind::RetirementExperiment),
            Some(500),
        ),
        row(
            "r.open",
            DebtStatus::Active,
            Some(RemovalTestKind::Inspection),
            None,
        ),
        row("r.untestable", DebtStatus::Active, None, None),
    ];
    let report = health.report(&index, 100);
    assert_eq!(report.expired_used, vec!["r.expired"]);
    assert_eq!(report.expiring, vec!["r.expiring"]);
    assert_eq!(
        report.open,
        vec!["r.expiring", "r.open"],
        "expiring rows stay open while their test is executable"
    );
    assert_eq!(report.not_yet_testable, vec!["r.untestable"]);
    assert_eq!(
        report
            .by_removal_test_kind
            .get(&RemovalTestKind::RetirementExperiment),
        Some(&1)
    );
    assert_eq!(
        DebtReport::from_json(&report.to_json()).unwrap(),
        report,
        "minimal DebtReport round-trips losslessly"
    );
}
