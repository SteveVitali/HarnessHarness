//! `hh-lab` exemplar tests — the two canonical `ExperimentSpec` documents at
//! Stage-3 size (ADR-0156 D1/D2; spec §6.3 §2.5; AC-R-2.10.3-8/-9 document
//! half). The engine-level execution evidence lives in
//! `hh-experiment/tests/engine.rs`.

use std::collections::BTreeMap;

use hh_budget::matchspec::MatchSpec;
use hh_budget::pricing::PricingTableRef;
use hh_budget::spec::{BudgetMode, BudgetSpec};
use hh_budget::{DimensionId, DimensionKey, MatchMode};
use hh_identity::idp::idp_id;
use hh_ontology::config::Ref;
use hh_ontology::eval::DesignKind;
use hh_ontology::lab::SplitLabel;

use hh_lab::exemplars::*;
use hh_lab::expand::{expand, ArmConfiguration, ExpandContext, ExpandTask};
use hh_lab::experiment::{ExperimentRefusal, ExperimentSpec, SpecContext};

fn pinned(tag: &str) -> String {
    idp_id(&format!("test.{tag}"), tag.as_bytes())
}

fn pins() -> ExemplarPins {
    ExemplarPins {
        suite_ref: pinned("suite.tb2"),
        held_out_split_ref: pinned("split.held_out"),
        split_assignment_ref: pinned("split.assign"),
        registry_snapshot_id: pinned("registry.snap"),
        eval_budget: "budget:eval".to_string(),
        search_budget: "budget:search-zero".to_string(),
        experiment_budget: "budget:experiment".to_string(),
        instrument_budget: "budget:instrument".to_string(),
        analysis_plan_ref: pinned("analysis.plan"),
        task_split_hash: pinned("split.hash"),
    }
}

fn compaction_pins() -> CompactionFamilyPins {
    CompactionFamilyPins {
        evict_oldest_ref: pinned("variant.evict_oldest"),
        clear_tool_results_ref: pinned("variant.clear_tool_results"),
        model_level_ref: pinned("model.fixed"),
        environment_level_ref: pinned("env.tb2"),
        artifacts: (
            Ref::new("definition:compaction", pinned("artifact.evict")),
            Ref::new("definition:compaction", pinned("artifact.clear")),
        ),
    }
}

fn control_pins() -> ControlStrategyFamilyPins {
    ControlStrategyFamilyPins {
        react_minimal_ref: pinned("variant.react_minimal"),
        react_steerable_ref: pinned("variant.react_steerable"),
        model_family_a_ref: pinned("model.family_a"),
        model_family_b_ref: pinned("model.family_b"),
        artifacts: (
            Ref::new("definition:control", pinned("artifact.min_a")),
            Ref::new("definition:control", pinned("artifact.min_b")),
            Ref::new("definition:control", pinned("artifact.steer_a")),
            Ref::new("definition:control", pinned("artifact.steer_b")),
        ),
        // The iso_cost companion shares `arm:steerable-a`'s point and (under
        // the OQ-363 interim rule) its configuration — the engine duplicates
        // the subject runs.
        iso_artifact: Ref::new("definition:control", pinned("artifact.steer_a")),
        pricing_table_ref: PricingTableRef {
            table_id: "pricing:test".to_string(),
            version: "1".to_string(),
            pin: Some(pinned("pricing.table")),
        },
    }
}

/// The `resolve_budget` map every exemplar register/expand runs against —
/// `budget:search-zero` is the registered zero-cap search budget
/// (`search_budget: 0`).
fn budget_map() -> BTreeMap<String, BudgetSpec> {
    let caps = |n: i64| {
        BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[(DimensionKey::Primary(DimensionId::ModelCalls), n)],
        )
    };
    BTreeMap::from([
        ("budget:eval".to_string(), caps(200)),
        ("budget:search-zero".to_string(), caps(0)),
        ("budget:experiment".to_string(), caps(100_000)),
        ("budget:instrument".to_string(), caps(100_000)),
    ])
}

fn ctx() -> SpecContext<'static> {
    let budgets: &'static BTreeMap<String, BudgetSpec> = Box::leak(Box::new(budget_map()));
    let resolve: Box<hh_lab::experiment::BudgetResolver<'static>> =
        Box::new(move |r: &str| budgets.get(r).cloned());
    let sealed: Box<dyn Fn(&str) -> bool> = Box::new(|_: &str| true);
    SpecContext {
        resolve_budget: Some(Box::leak(resolve)),
        artifact_sealed: Some(Box::leak(sealed)),
        min_replicates: EXEMPLAR_REPLICATES,
        ..SpecContext::member_level()
    }
}

fn tasks(n: usize) -> Vec<ExpandTask> {
    (0..n)
        .map(|i| ExpandTask {
            task_id: pinned(&format!("task.held_out.{i}")),
            split_label: SplitLabel::HeldOut,
        })
        .collect()
}

fn arm_cfg(
    a: &hh_lab::experiment::ArmSpec,
) -> Result<ArmConfiguration, hh_lab::expand::ExpandError> {
    Ok(ArmConfiguration {
        configuration_id: pinned(&format!("cfg.{}", a.arm_id)),
        configuration_version_id: pinned(&format!("cfgv.{}", a.arm_id)),
    })
}

fn expand_ctx() -> ExpandContext<'static> {
    ExpandContext {
        arm_config: &arm_cfg,
        level_ineligible: None,
    }
}

#[test]
fn compaction_family_v1_registers_and_expands_at_stage3_size() {
    let spec = compaction_family_v1(&pins(), &compaction_pins(), 1);
    // The document shape (ADR-0156 D1).
    assert_eq!(spec.design.id, "lab/compaction-family-v1");
    assert_eq!(spec.design.kind, DesignKind::Paired);
    assert_eq!(spec.arms.len(), 2);
    assert_eq!(spec.replicates_per_cell, 5);
    assert_eq!(
        spec.design.registry_snapshot_id.as_deref(),
        Some(pinned("registry.snap").as_str())
    );
    spec.register(&ctx()).unwrap();
    // `experiment_id` is the content address — stable under a rebuild.
    let again = compaction_family_v1(&pins(), &compaction_pins(), 1);
    assert_eq!(spec.experiment_id, again.experiment_id);

    let t = tasks(6); // a held-out task axis
    let plan = expand(&spec, &t, &expand_ctx()).unwrap();
    // cells = 2 arms × 6 tasks; plans = cells × 5 replicates.
    assert_eq!(plan.cells.len(), 12);
    assert_eq!(plan.run_plans.len(), 60);
    let plan2 = expand(&spec, &t, &expand_ctx()).unwrap();
    assert_eq!(plan.plan_id, plan2.plan_id);
    assert_eq!(plan, plan2);
    // Every run plan carries the expansion-derived identity members.
    assert!(plan.run_plans.iter().all(|p| p.replicate_index < 5));
}

#[test]
fn control_strategy_family_v1_registers_with_iso_companion_arm() {
    let spec = control_strategy_family_v1(&pins(), &control_pins(), 1);
    assert_eq!(spec.design.id, "lab/control-strategy-family-v1");
    assert_eq!(spec.design.kind, DesignKind::FullFactorial);
    assert_eq!(spec.arms.len(), 5);
    assert_eq!(spec.replicates_per_cell, 5);
    let modes: Vec<MatchMode> = spec
        .arms
        .iter()
        .map(|a| a.match_spec.as_ref().unwrap().mode)
        .collect();
    assert_eq!(
        modes
            .iter()
            .filter(|m| **m == MatchMode::MatchedCap)
            .count(),
        4
    );
    assert_eq!(
        modes.iter().filter(|m| **m == MatchMode::IsoCost).count(),
        1
    );
    // Register admits the mixed-mode arm set — commensurability is a
    // within-group precondition (CF-334); cross-mode `compare` refuses
    // downstream.
    spec.register(&ctx()).unwrap();

    let t = tasks(4);
    let plan = expand(&spec, &t, &expand_ctx()).unwrap();
    // cells = 5 arms × 4 tasks; plans = cells × 5 replicates — the iso arm
    // gets its own subject runs (ADR-0213's interim duplication).
    assert_eq!(plan.cells.len(), 20);
    assert_eq!(plan.run_plans.len(), 100);
    // The companion's plans are distinct run_plan_ids from the matched arm
    // at the same design point — no run sharing (OQ-363 unruled).
    let matched_cells: std::collections::BTreeSet<_> = plan
        .cells
        .iter()
        .filter(|c| c.arm_id == "arm:steerable-a")
        .map(|c| &c.cell_id)
        .collect();
    let matched: Vec<_> = plan
        .run_plans
        .iter()
        .filter(|p| matched_cells.contains(&p.cell_id))
        .collect();
    assert_eq!(matched.len(), 4 * 5);
    let iso_cells: std::collections::BTreeSet<_> = plan
        .cells
        .iter()
        .filter(|c| c.arm_id == "arm:steerable-a-iso")
        .map(|c| &c.cell_id)
        .collect();
    let iso_plans: Vec<_> = plan
        .run_plans
        .iter()
        .filter(|p| iso_cells.contains(&p.cell_id))
        .collect();
    assert_eq!(iso_cells.len(), 4);
    assert_eq!(iso_plans.len(), 4 * 5);
    let matched_ids: std::collections::BTreeSet<_> =
        matched.iter().map(|p| &p.run_plan_id).collect();
    assert!(iso_plans
        .iter()
        .all(|p| !matched_ids.contains(&p.run_plan_id)));
}

#[test]
fn exemplar_specs_round_trip() {
    for spec in [
        compaction_family_v1(&pins(), &compaction_pins(), 1),
        control_strategy_family_v1(&pins(), &control_pins(), 1),
    ] {
        let decoded = ExperimentSpec::from_json(&spec.to_json()).unwrap();
        assert_eq!(decoded.experiment_id, spec.experiment_id);
        assert_eq!(decoded, spec);
    }
}

#[test]
fn exemplar_shape_guards_hold() {
    let p = pins();
    // A `mode: none` arm on a comparative spec is refused MissingMatchSpec.
    let mut bad = compaction_family_v1(&p, &compaction_pins(), 1);
    bad.arms[0].match_spec = Some(MatchSpec {
        mode: MatchMode::None,
        ..MatchSpec::matched_cap(&[DimensionId::ModelCalls])
    });
    bad.experiment_id = bad.experiment_id();
    assert!(matches!(
        bad.register(&ctx()),
        Err(ExperimentRefusal::MissingMatchSpec { .. })
    ));
    // An `iso_cost` arm with no pricing table is refused MissingPricingTable
    // even when it is its own comparand group.
    let mut bad2 = compaction_family_v1(&p, &compaction_pins(), 1);
    bad2.arms[0].match_spec = Some(MatchSpec {
        mode: MatchMode::IsoCost,
        pricing_table_ref: None,
        ..control_strategy_family_v1(&p, &control_pins(), 1).arms[4]
            .match_spec
            .clone()
            .unwrap()
    });
    bad2.experiment_id = bad2.experiment_id();
    assert!(matches!(
        bad2.register(&ctx()),
        Err(ExperimentRefusal::MissingPricingTable { .. })
    ));
    // A same-mode duplicate at an occupied design point is refused — the
    // companion rule admits only cross-mode companions.
    let mut bad3 = control_strategy_family_v1(&p, &control_pins(), 1);
    bad3.arms[4].match_spec = Some(control_matched_for_test());
    bad3.experiment_id = bad3.experiment_id();
    assert!(matches!(
        bad3.register(&ctx()),
        Err(ExperimentRefusal::InadmissibleFactor { .. })
    ));
}

fn control_matched_for_test() -> MatchSpec {
    control_strategy_family_v1(&pins(), &control_pins(), 1).arms[0]
        .match_spec
        .clone()
        .unwrap()
}
