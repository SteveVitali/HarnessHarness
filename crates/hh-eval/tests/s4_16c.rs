//! S4.16c — `interventions_experiment` (AC-R-2.7.2b-4/-5; §5f.3): the
//! `{interventions_off, interventions_on}` matched comparison over the
//! pinned metrics (`task_success`, `false_completion_rate`,
//! `gate_hold_count`, `reconciliation_cost`), the intervention-cost
//! MatchSpec enforcement (`MatchUnderScoped`), the H-G2-1
//! `false_completion_rate` model × harness interaction, and the
//! `removal_test` handle every Γ row / probe rule binds (CC9/T-LCD-05).

use std::collections::{BTreeMap, BTreeSet};

use hh_budget::matchspec::{ArmSpec, MatchSpec};
use hh_budget::spec::{BudgetMode, BudgetSpec};
use hh_eval::catalogue;
use hh_eval::interventions::{
    interventions_experiment, InterventionsError, InterventionsExperimentInput, InterventionsTier,
    ARM_OFF, ARM_ON, H_G2_1_INTERACTION, INTERVENTIONS_METRICS, INTERVENTION_MATCH_DIMS,
};
use hh_eval::runs::{CacheState, EvalRun, TaskContext};
use hh_ontology::compliance::{Detector, MetricDeclaration};
use hh_ontology::control::OutcomeClass;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_ontology::eval::{
    Design, DesignKind, MetricValue, MetricValueKind, Pairing, PreRegistration, RoutingPolicy,
    SeedPolicy,
};
use hh_ontology::lab::{ContaminationStratum, EnvironmentFamily, SplitLabel};
use hh_ontology::participant::{Observability, ParticipantClass};

fn metric(name: &str) -> MetricDeclaration {
    catalogue::metric(name).expect("catalogue metric")
}

fn run(arm: &str, task: &str, rep: u64, metric_name: &str, value: MetricValueKind) -> EvalRun {
    EvalRun {
        run_id: format!("r-{arm}-{task}-{rep}-{metric_name}"),
        arm_id: arm.into(),
        cell_id: Some(format!("{arm}:{task}")),
        configuration_id: format!("cfg-{arm}"),
        participant_class: ParticipantClass::Native,
        observability_level: [
            Observability::Events,
            Observability::ModelIo,
            Observability::EndState,
            Observability::Ledger,
        ]
        .into_iter()
        .collect(),
        mediation: BTreeSet::new(),
        capability_vector: BTreeMap::new(),
        task_id: task.into(),
        suite_id: "suite-test".into(),
        split_label: SplitLabel::HeldOut,
        replicate_index: rep,
        attempt_no: 1,
        seed: Some(42 + rep),
        seed_honoured: true,
        cache_state: CacheState::ColdStart,
        comparable: true,
        outcome_class: OutcomeClass::Scored,
        budget_consumed: INTERVENTION_MATCH_DIMS.iter().map(|d| (*d, 5)).collect(),
        veto_tripped: vec![],
        values: vec![MetricValue {
            metric_ref: metric_name.into(),
            value,
            applies_to: format!("r-{arm}-{task}-{rep}-{metric_name}"),
            oracle_ref: "oracle/executable".into(),
            detector: Detector::Deterministic,
            confidence: None,
            evidence_ref: None,
            calibration_ref: None,
            exploratory: None,
        }],
        environment_version_id: Some("env-1".into()),
        environment_family: EnvironmentFamily::CodingTerminal,
        fault_profile: None,
        perturbation_profile: None,
        model_snapshots: [("default".to_string(), "snap-1".to_string())]
            .into_iter()
            .collect(),
        stratum: ContaminationStratum::PrivateHeldOut,
        eval_search_spend: 0,
        routing_deviation: false,
        replayed_trajectory: false,
        served_from_cache_count: 0,
        cache_prefix_hit_ratio: None,
        split_hash: Some("sha256:split-1".into()),
        facts: Default::default(),
    }
}

/// A tier's runs — `{off, on}` × 2 tasks × 2 replicates × 4 metrics. The
/// on-arm lifts `task_success` and lowers `false_completion_rate` (by
/// `fcr_delta` ppm — the H-G2-1 effect size the contrast reads).
fn tier_runs(fcr_delta: i64) -> Vec<EvalRun> {
    let mut runs = Vec::new();
    for m in INTERVENTIONS_METRICS {
        for rep in 0..2u64 {
            for task in ["t1", "t2"] {
                for arm in [ARM_OFF, ARM_ON] {
                    let on = arm == ARM_ON;
                    let value = match m {
                        "task_success" => MetricValueKind::Bool(!(arm == ARM_OFF && task == "t2")),
                        "false_completion_rate" => {
                            MetricValueKind::Decimal(if on { 300_000 - fcr_delta } else { 300_000 })
                        }
                        "gate_hold_count" => MetricValueKind::Decimal(if on { 2 } else { 0 }),
                        _ => MetricValueKind::Decimal(if on { 60_000 } else { 40_000 }),
                    };
                    runs.push(run(arm, task, rep, m, value));
                }
            }
        }
    }
    runs
}

fn design(interactions: &[&str]) -> Design {
    Design {
        id: "design-1".into(),
        kind: DesignKind::Paired,
        factors: vec![],
        blocking: vec!["task".into()],
        replicates_per_cell: 2,
        pairing: Pairing::ByTaskAndReplicate,
        seed_policy: SeedPolicy {
            harness_rng: true,
            requested_sampling_seed: true,
            seed_honoured_required: true,
        },
        held_out_split_ref: Some("split-1".into()),
        pre_registration: PreRegistration {
            registered_at: 1,
            hypothesis: "interventions ↓ false_completion_rate".into(),
            primary_metrics: vec!["false_completion_rate".into()],
            equivalence_margin: None,
            min_n: 1,
            analysis_plan_ref: "plan-1".into(),
            task_split_hash: "sha256:split-1".into(),
            interactions: interactions.iter().map(|s| s.to_string()).collect(),
        },
        registry_snapshot_id: None,
        generators: None,
        resolution: None,
        routing_policy: RoutingPolicy::FailFast,
        deviation_policy: None,
        cache_na_stratified: false,
    }
}

fn tasks() -> Vec<TaskContext> {
    ["t1", "t2"]
        .iter()
        .map(|t| TaskContext {
            task_id: t.to_string(),
            suite_id: "suite-test".into(),
            split_label: SplitLabel::HeldOut,
            split_hash: "sha256:split-1".into(),
            stratum: ContaminationStratum::PrivateHeldOut,
        })
        .collect()
}

/// An arm spec covering the four intervention-cost dims — the matched
/// budget AC-R-2.7.2b-4 requires (probe + notice tokens ride
/// `tokens.output.visible`/`model_calls`; validator-run cost rides
/// `evaluator_calls`; hold cost rides `reconciliation_holds`).
fn arm_spec() -> ArmSpec {
    let caps = BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[(DimensionKey::Primary(DimensionId::ModelCalls), 100)],
    );
    ArmSpec::native(
        caps.clone(),
        caps,
        MatchSpec::matched_cap(&INTERVENTION_MATCH_DIMS),
    )
}

fn decls() -> Vec<MetricDeclaration> {
    INTERVENTIONS_METRICS.iter().map(|m| metric(m)).collect()
}

#[test]
fn interventions_experiment_reports_four_deltas() {
    let runs = tier_runs(200_000);
    let ts = tasks();
    let d = design(&[]);
    let arms = vec![arm_spec(), arm_spec()];
    let out = interventions_experiment(&InterventionsExperimentInput {
        design: &d,
        arm_specs: &arms,
        runs: &runs,
        tasks: &ts,
        declarations: &decls(),
        confidence_ppm: 950_000,
        benefit_kind: hh_lab::analysis::BenefitKind::ArtifactBenefit,
        held_out: true,
        removal_subject: "gamma:table:1",
        higher_tier: None,
    })
    .unwrap();
    // Four reports, in pinned catalogue order.
    let names: Vec<&str> = out.reports.iter().map(|r| r.metric.as_str()).collect();
    assert_eq!(names, INTERVENTIONS_METRICS);
    // `false_completion_rate` fell under interventions (delta −200_000).
    let fcr = out.reports[1]
        .paired_effect
        .point
        .as_ref()
        .and_then(hh_wire::Json::as_int)
        .unwrap();
    assert_eq!(fcr, -200_000);
    // The removal-test handle names the bound Γ/probe subject —
    // "disable Γ, keep the experiment" refuses to resolve (CC9).
    assert_eq!(out.removal_test, "interventions_experiment:gamma:table:1");
    // No higher tier ⇒ no interaction (typed absence, never a zero).
    assert!(out.interaction.is_none());
}

#[test]
fn under_scoped_match_is_refused() {
    let runs = tier_runs(200_000);
    let ts = tasks();
    let d = design(&[]);
    // An arm whose MatchSpec covers only `model_calls` omits the
    // intervention-cost dims — refused, never silently compared.
    let caps = BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[(DimensionKey::Primary(DimensionId::ModelCalls), 100)],
    );
    let thin = ArmSpec::native(
        caps.clone(),
        caps,
        MatchSpec::matched_cap(&[DimensionId::ModelCalls]),
    );
    let err = interventions_experiment(&InterventionsExperimentInput {
        design: &d,
        arm_specs: &[thin],
        runs: &runs,
        tasks: &ts,
        declarations: &decls(),
        confidence_ppm: 950_000,
        benefit_kind: hh_lab::analysis::BenefitKind::ArtifactBenefit,
        held_out: true,
        removal_subject: "gamma:table:1",
        higher_tier: None,
    })
    .unwrap_err();
    match err {
        InterventionsError::MatchUnderScoped { missing, .. } => {
            assert!(missing
                .iter()
                .any(|m: &String| m.contains("EvaluatorCalls")));
            assert!(missing
                .iter()
                .any(|m: &String| m.contains("ReconciliationHolds")));
        }
        other => panic!("expected MatchUnderScoped, got {other:?}"),
    }
}

#[test]
fn h_g2_1_interaction_reads_the_did() {
    // The pre-registration carries the interaction spelling.
    let d = design(&[H_G2_1_INTERACTION]);
    let lower = tier_runs(200_000); // interventions help the lower tier more
    let upper = tier_runs(50_000); // the gap shrinks with capability
    let ts = tasks();
    let arms = vec![arm_spec(), arm_spec()];
    let out = interventions_experiment(&InterventionsExperimentInput {
        design: &d,
        arm_specs: &arms,
        runs: &lower,
        tasks: &ts,
        declarations: &decls(),
        confidence_ppm: 950_000,
        benefit_kind: hh_lab::analysis::BenefitKind::ArtifactBenefit,
        held_out: true,
        removal_subject: "gamma:table:1",
        higher_tier: Some(InterventionsTier {
            label: "high",
            design: &d,
            arm_specs: &arms,
            runs: &upper,
        }),
    })
    .unwrap();
    // H-G2-1: DiD = delta_lower − delta_higher = −200_000 − (−50_000)
    // = −150_000 ppm per task (interventions reduce fcr more at the
    // lower tier — negative delta means *more* reduction).
    let est = out.interaction.expect("two tiers yield the contrast");
    assert_eq!(est.point, -150_000);
    assert_eq!(est.n_tasks, 2);
}
