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
        ("budget:inference-zero".to_string(), caps(0)),
        // The `bandit` warm-up — the searched arm's `search_budget > 0`,
        // legal only under `matched_total` (M3).
        ("budget:warmup".to_string(), caps(50)),
        // A searched arm's budget for the union-dims test — `model_calls`
        // matches the recipe's zero baseline while `tool_calls` (not a
        // declared match dim) carries positive headroom.
        (
            "budget:search-tools".to_string(),
            BudgetSpec::hard_caps(
                BudgetMode::Pool,
                &[
                    (DimensionKey::Primary(DimensionId::ModelCalls), 0),
                    (DimensionKey::Primary(DimensionId::ToolCalls), 5),
                ],
            ),
        ),
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

fn context_builder_pins() -> ContextBuilderPins {
    ContextBuilderPins {
        rp_on_ref: pinned("variant.rp_on"),
        banner_only_ref: pinned("variant.banner_only"),
        handle_only_ref: pinned("variant.handle_only"),
        expanded_catalogs_ref: pinned("variant.expanded_catalogs"),
        clearing_on_ref: pinned("variant.clearing_on"),
        clearing_off_ref: pinned("variant.clearing_off"),
        model_level_ref: pinned("model.fixed"),
        environment_level_ref: pinned("env.tb2"),
        artifacts: (0..8)
            .map(|i| Ref::new("definition:context", pinned(&format!("artifact.cb.{i}"))))
            .collect(),
    }
}

fn compliance_pins() -> MemoryCompliancePins {
    MemoryCompliancePins {
        external_slot_ref: pinned("variant.external_slot"),
        promotion_endorsed_ref: pinned("variant.promotion_endorsed"),
        model_level_ref: pinned("model.fixed"),
        environment_level_ref: pinned("env.tb2"),
        artifacts: (
            Ref::new("definition:memory", pinned("artifact.ext")),
            Ref::new("definition:memory", pinned("artifact.promo")),
        ),
    }
}

fn validity_pins() -> MemoryValidityPins {
    MemoryValidityPins {
        validity_filter_refs: (pinned("variant.vf_off"), pinned("variant.vf_on")),
        justification_scope_refs: (
            pinned("variant.js_delivered"),
            pinned("variant.js_subject_overlap"),
        ),
        conflict_policy_refs: (pinned("variant.cp_deliver"), pinned("variant.cp_withhold")),
        model_level_ref: pinned("model.fixed"),
        environment_level_ref: pinned("env.tb2"),
        artifacts: (0..8)
            .map(|i| Ref::new("definition:memory", pinned(&format!("artifact.mv.{i}"))))
            .collect(),
    }
}

fn procedure_pins() -> ProcedureExecutionPins {
    ProcedureExecutionPins {
        target_refs: (
            pinned("variant.target.instruction"),
            pinned("variant.target.workflow_node"),
        ),
        profile_refs: (pinned("variant.profile.p1"), pinned("variant.profile.p2")),
        model_level_ref: pinned("model.fixed"),
        environment_level_ref: pinned("env.tb2"),
        artifacts: (
            Ref::new("definition:procedure", pinned("artifact.i.p1")),
            Ref::new("definition:procedure", pinned("artifact.i.p2")),
            Ref::new("definition:procedure", pinned("artifact.w.p1")),
            Ref::new("definition:procedure", pinned("artifact.w.p2")),
        ),
    }
}

fn retrieval_pins() -> RetrievalIndexPins {
    RetrievalIndexPins {
        deterministic_default_ref: pinned("variant.ranker.det_default"),
        structural_pagerank_ref: pinned("variant.ranker.structural_pagerank"),
        model_level_ref: pinned("model.fixed"),
        environment_level_ref: pinned("env.tb2"),
        artifacts: (
            Ref::new("definition:retrieval", pinned("artifact.det")),
            Ref::new("definition:retrieval", pinned("artifact.sp")),
        ),
    }
}

#[test]
fn context_builder_v1_registers_and_expands() {
    // AC-R-2.4.1-12 — utility under matched budget: rp × catalogs × clearing.
    let spec = context_builder_v1(&pins(), &context_builder_pins(), 1);
    assert_eq!(spec.design.id, "lab/context-builder-v1");
    assert_eq!(spec.design.kind, DesignKind::FullFactorial);
    assert_eq!(spec.arms.len(), 8);
    assert!(spec
        .arms
        .iter()
        .all(|a| a.match_spec.as_ref().unwrap().mode == MatchMode::MatchedCap));
    spec.register(&ctx()).unwrap();
    assert_eq!(
        spec.experiment_id,
        context_builder_v1(&pins(), &context_builder_pins(), 1).experiment_id
    );
    let t = tasks(3);
    let plan = expand(&spec, &t, &expand_ctx()).unwrap();
    assert_eq!(plan.cells.len(), 8 * 3);
    assert_eq!(plan.run_plans.len(), 8 * 3 * EXEMPLAR_REPLICATES as usize);
}

#[test]
fn memory_compliance_v1_registers_and_expands() {
    // AC-R-2.4.3-9 — external slot vs promotion-endorsed under matched budget.
    let spec = memory_compliance_v1(&pins(), &compliance_pins(), 1);
    assert_eq!(spec.design.id, "lab/memory-compliance-v1");
    assert_eq!(spec.design.kind, DesignKind::Paired);
    assert_eq!(spec.arms.len(), 2);
    spec.register(&ctx()).unwrap();
    let t = tasks(4);
    let plan = expand(&spec, &t, &expand_ctx()).unwrap();
    assert_eq!(plan.cells.len(), 8);
    assert_eq!(plan.run_plans.len(), 40);
}

#[test]
fn memory_validity_v1_registers_and_expands() {
    // AC-R-2.4.4-12 — validity_filter × justification_scope × conflict_policy.
    let spec = memory_validity_v1(&pins(), &validity_pins(), 1);
    assert_eq!(spec.design.id, "lab/memory-validity-v1");
    assert_eq!(spec.design.kind, DesignKind::FullFactorial);
    assert_eq!(spec.arms.len(), 8);
    spec.register(&ctx()).unwrap();
    let t = tasks(2);
    let plan = expand(&spec, &t, &expand_ctx()).unwrap();
    assert_eq!(plan.cells.len(), 16);
    assert_eq!(plan.run_plans.len(), 16 * EXEMPLAR_REPLICATES as usize);
}

#[test]
fn procedure_execution_v1_registers_and_expands() {
    // AC-R-2.4.5-10 — target × profile, interaction reported.
    let spec = procedure_execution_v1(&pins(), &procedure_pins(), 1);
    assert_eq!(spec.design.id, "lab/procedure-execution-v1");
    assert_eq!(spec.design.kind, DesignKind::FullFactorial);
    assert_eq!(spec.arms.len(), 4);
    assert!(spec
        .design
        .pre_registration
        .interactions
        .contains(&"target:profile".to_string()));
    spec.register(&ctx()).unwrap();
    let t = tasks(3);
    let plan = expand(&spec, &t, &expand_ctx()).unwrap();
    assert_eq!(plan.cells.len(), 12);
    assert_eq!(plan.run_plans.len(), 60);
}

#[test]
fn retrieval_index_v1_registers_structural_index_arm() {
    // §5c.3 — `structural_index` is a Lab arm: the structural_pagerank ranker
    // level pairs against the deterministic default under MatchSpec.
    let spec = retrieval_index_v1(&pins(), &retrieval_pins(), 1);
    assert_eq!(spec.design.id, "lab/retrieval-index-v1");
    assert_eq!(spec.arms.len(), 2);
    let ranker = spec.factors.iter().find(|f| f.name == "ranker").unwrap();
    assert!(ranker
        .levels
        .iter()
        .any(|l| l.level_id == "structural_pagerank"));
    spec.register(&ctx()).unwrap();
    let t = tasks(2);
    let plan = expand(&spec, &t, &expand_ctx()).unwrap();
    assert_eq!(plan.cells.len(), 4);
    assert_eq!(plan.run_plans.len(), 20);
}

// ── S4.2 Phase-4 recipes ────────────────────────────────────────────────────

fn delegation_pins() -> DelegationPins {
    DelegationPins {
        topology_refs: (
            pinned("topology.t0"),
            pinned("topology.t1f2"),
            pinned("topology.t1f4"),
            pinned("topology.t2"),
            pinned("topology.t3"),
        ),
        model_family_refs: (pinned("model.family_a"), pinned("model.family_b")),
        environment_level_ref: pinned("env.coding"),
        research_environment_level_ref: pinned("env.research_longctx"),
        artifacts: (
            Ref::new("definition:delegation", pinned("artifact.t0")),
            Ref::new("definition:delegation", pinned("artifact.t1f2")),
            Ref::new("definition:delegation", pinned("artifact.t1f4")),
            Ref::new("definition:delegation", pinned("artifact.t2")),
            Ref::new("definition:delegation", pinned("artifact.t3")),
        ),
        pricing_table_ref: PricingTableRef {
            table_id: "pricing:test".to_string(),
            version: "1".to_string(),
            pin: Some(pinned("pricing.table")),
        },
        inference_budget_ref: "budget:inference-zero".to_string(),
    }
}

fn voc_pins() -> ValueOfComputePins {
    ValueOfComputePins {
        policy_refs: (
            pinned("policy.static"),
            pinned("policy.uniform"),
            pinned("policy.rules"),
        ),
        bandit_policy_ref: pinned("policy.bandit"),
        model_family_refs: (pinned("model.family_a"), pinned("model.family_b")),
        environment_level_ref: pinned("env.coding"),
        terminal_environment_level_ref: pinned("env.terminal"),
        artifacts: (
            Ref::new("definition:compute", pinned("artifact.static")),
            Ref::new("definition:compute", pinned("artifact.uniform")),
            Ref::new("definition:compute", pinned("artifact.rules")),
        ),
        bandit_artifact: Ref::new("definition:compute", pinned("artifact.bandit")),
        iso_artifact: Ref::new("definition:compute", pinned("artifact.rules")),
        pricing_table_ref: PricingTableRef {
            table_id: "pricing:test".to_string(),
            version: "1".to_string(),
            pin: Some(pinned("pricing.table")),
        },
        eval_budget_wide: "budget:eval".to_string(),
        warmup_search_budget_ref: "budget:warmup".to_string(),
        inference_budget_ref: "budget:inference-zero".to_string(),
    }
}

fn coord_pins() -> CoordinationTopologyPins {
    CoordinationTopologyPins {
        topology_refs: (
            pinned("topology.single"),
            pinned("topology.ow_iso"),
            pinned("topology.ow_share"),
            pinned("topology.peer"),
        ),
        merge_policy_refs: (
            pinned("merge.parent_effects"),
            pinned("merge.three_way_text"),
        ),
        isolation_refs: (pinned("iso.fork_snapshot"), pinned("iso.scoped_subtree")),
        model_family_ref: pinned("model.family_a"),
        environment_level_ref: pinned("env.coding"),
        artifacts: (
            Ref::new("definition:coord", pinned("artifact.single")),
            Ref::new("definition:coord", pinned("artifact.ow_iso")),
            Ref::new("definition:coord", pinned("artifact.ow_share")),
            Ref::new("definition:coord", pinned("artifact.peer")),
        ),
        pricing_table_ref: PricingTableRef {
            table_id: "pricing:test".to_string(),
            version: "1".to_string(),
            pin: Some(pinned("pricing.table")),
        },
        inference_budget_ref: "budget:inference-zero".to_string(),
    }
}

#[test]
fn delegation_v1_registers_and_expands() {
    let spec = delegation_v1(&pins(), &delegation_pins(), 1);
    assert_eq!(spec.design.id, "lab/delegation-v1");
    assert_eq!(spec.design.kind, DesignKind::FullFactorial);
    // Stage 5: the matched_total product topology(5) × model_snapshot(2)
    // × environment(2 — coding + research/long-context) plus the
    // recipe's second arm set (matched_cap on time.wall_ms) over the
    // same points (ADR-0186 D6; §5e.3's recipe shape).
    assert_eq!(spec.arms.len(), 40);
    assert_eq!(
        spec.arms
            .iter()
            .filter(|a| a.match_spec.as_ref().unwrap().mode == MatchMode::MatchedTotal)
            .count(),
        20
    );
    assert_eq!(
        spec.arms
            .iter()
            .filter(|a| a.match_spec.as_ref().unwrap().mode == MatchMode::MatchedCap)
            .count(),
        20
    );
    assert!(spec
        .arms
        .iter()
        .filter(|a| a.match_spec.as_ref().unwrap().mode == MatchMode::MatchedCap)
        .all(|a| a.match_spec.as_ref().unwrap().dimensions
            == vec![hh_ontology::dimensions::DimensionId::TimeWallMs]));
    assert_eq!(spec.replicates_per_cell, EXEMPLAR_REPLICATES);
    let topo = spec
        .factors
        .iter()
        .find(|f| f.name == "topology")
        .expect("topology factor");
    assert_eq!(
        topo.granularity,
        Some(hh_ontology::participant::Granularity::ComponentLevel)
    );
    assert_eq!(topo.levels.len(), 5);
    let env = spec
        .factors
        .iter()
        .find(|f| f.name == "environment")
        .expect("environment factor");
    assert_eq!(env.levels.len(), 2);
    // Every arm carries inference_budget so the M3 total is well-formed.
    assert!(spec.arms.iter().all(|a| a.inference_budget.is_some()));
    spec.register(&ctx()).unwrap();
    let again = delegation_v1(&pins(), &delegation_pins(), 1);
    assert_eq!(spec.experiment_id, again.experiment_id);
    let t = tasks(3);
    let plan = expand(&spec, &t, &expand_ctx()).unwrap();
    assert_eq!(plan.cells.len(), 40 * 3);
    assert_eq!(plan.run_plans.len(), 40 * 3 * EXEMPLAR_REPLICATES as usize);
}

#[test]
fn value_of_compute_v1_registers_with_iso_companion() {
    let spec = value_of_compute_v1(&pins(), &voc_pins(), 1);
    assert_eq!(spec.design.id, "lab/value-of-compute-v1");
    assert_eq!(spec.design.kind, DesignKind::FullFactorial);
    // Stage 5: the matched_cap product is the covering group —
    // {static, uniform, rules, bandit} × 2 models × {coding, terminal}
    // × {tight, wide} = 32 — plus the iso_cost companion and the
    // searched `matched_total` arm set ({rules, bandit} × 2 models × 2
    // suites at tight = 8, the H3 warm-up contrast).
    assert_eq!(spec.arms.len(), 41);
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
        32
    );
    assert_eq!(
        modes
            .iter()
            .filter(|m| **m == MatchMode::MatchedTotal)
            .count(),
        8
    );
    assert_eq!(
        modes.iter().filter(|m| **m == MatchMode::IsoCost).count(),
        1
    );
    // AC-R-2.6.4-12/13: a *searched* arm (`search_budget > 0`) binds
    // `matched_total` only — every `matched_total` arm carries the
    // funded warm-up; the `matched_cap` bandit rows carry the shared
    // zero-cap search budget (an unfunded bandit is not searched).
    for a in &spec.arms {
        let searched = a.search_budget.as_deref() == Some("budget:warmup");
        assert_eq!(
            searched,
            a.match_spec.as_ref().unwrap().mode == MatchMode::MatchedTotal,
            "arm {} — searched ⇒ matched_total, unsearched ⇒ not",
            a.arm_id
        );
    }
    assert!(spec
        .arms
        .iter()
        .filter(|a| a.match_spec.as_ref().unwrap().mode == MatchMode::MatchedTotal)
        .all(|a| ["rules", "bandit"].contains(&a.level_assignment["compute_policy"].as_str())));
    // `bandit` is a declared factor level (the recipe's fourth value).
    let policy = spec
        .factors
        .iter()
        .find(|f| f.name == "compute_policy")
        .expect("compute_policy factor");
    assert_eq!(policy.levels.len(), 4);
    assert!(policy.levels.iter().any(|l| l.level_id == "bandit"));
    // The recipe's `{coding, terminal}` suite axis is declared.
    let env = spec
        .factors
        .iter()
        .find(|f| f.name == "environment")
        .expect("environment factor");
    assert_eq!(env.levels.len(), 2);
    // The budget factor is declared with both levels.
    let budget = spec
        .factors
        .iter()
        .find(|f| f.name == "eval_budget")
        .expect("eval_budget factor");
    assert_eq!(budget.levels.len(), 2);
    spec.register(&ctx()).unwrap();
    let again = value_of_compute_v1(&pins(), &voc_pins(), 1);
    assert_eq!(spec.experiment_id, again.experiment_id);
    let t = tasks(2);
    let plan = expand(&spec, &t, &expand_ctx()).unwrap();
    assert_eq!(plan.cells.len(), 41 * 2);
}

#[test]
fn value_of_compute_v1_refuses_a_searched_arm_under_matched_cap() {
    // AC-R-2.6.4-12 (§5e.4; ADR-0190 D2): an arm doing search work —
    // `search_budget > 0` (the Stage-5 `bandit`/`predictor` levels'
    // warm-up/training spend, or evolution-produced rules) is comparable
    // only under `matched_total`; under the recipe's `matched_cap`
    // contrast it is `IncommensurableMatch`, never silently admitted.
    let mut spec = value_of_compute_v1(&pins(), &voc_pins(), 1);
    // Positive caps on a declared dim (`model_calls` — matched_cap's own
    // axis): refuses `UnequalCaps` against the group's zero baseline.
    spec.arms[0].search_budget = Some("budget:experiment".into());
    spec.experiment_id = spec.experiment_id();
    assert!(
        matches!(
            spec.register(&ctx()),
            Err(ExperimentRefusal::IncommensurableMatch { .. })
        ),
        "a searched arm in the matched_cap group — refused"
    );

    // The same refusal when the search spend shows only on an
    // *undeclared* dim — the union-dims guard (a searched arm cannot
    // evade by capping a dim the match spec does not name).
    let mut spec = value_of_compute_v1(&pins(), &voc_pins(), 1);
    spec.arms[1].search_budget = Some("budget:search-tools".into());
    spec.experiment_id = spec.experiment_id();
    assert!(
        matches!(
            spec.register(&ctx()),
            Err(ExperimentRefusal::IncommensurableMatch { .. })
        ),
        "search headroom on an undeclared dim — refused"
    );
}

#[test]
fn coordination_topology_v1_registers_and_expands() {
    let spec = coordination_topology_v1(&pins(), &coord_pins(), 1);
    assert_eq!(spec.design.id, "lab/coordination-topology-v1");
    assert_eq!(spec.design.kind, DesignKind::FullFactorial);
    // 4 topologies × 2 merge policies × 2 isolation defaults.
    assert_eq!(spec.arms.len(), 16);
    assert_eq!(spec.replicates_per_cell, EXEMPLAR_REPLICATES);
    // All three factors are component-level (AC-R-2.6.5-7).
    for name in ["topology", "merge_policy", "default_isolation"] {
        let f = spec
            .factors
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{name} factor"));
        assert_eq!(
            f.granularity,
            Some(hh_ontology::participant::Granularity::ComponentLevel),
            "{name}"
        );
    }
    // matched_total everywhere — child spend charges the subject.
    assert!(spec
        .arms
        .iter()
        .all(|a| a.match_spec.as_ref().unwrap().mode == MatchMode::MatchedTotal));
    spec.register(&ctx()).unwrap();
    let again = coordination_topology_v1(&pins(), &coord_pins(), 1);
    assert_eq!(spec.experiment_id, again.experiment_id);
    let t = tasks(2);
    let plan = expand(&spec, &t, &expand_ctx()).unwrap();
    assert_eq!(plan.cells.len(), 16 * 2);
    assert_eq!(plan.run_plans.len(), 16 * 2 * EXEMPLAR_REPLICATES as usize);
}

// ── lab/compaction-boundary-v1 (AC-R-2.4.2-12; ADR-0077 d7; S4.16b) ─────────

fn boundary_pins() -> CompactionBoundaryPins {
    CompactionBoundaryPins {
        proposal_variant_ref: pinned("variant.summarize_rolling"),
        model_level_ref: pinned("model.fixed"),
        environment_level_ref: pinned("env.tb2"),
        artifacts: (
            Ref::new("definition:compaction", pinned("artifact.applied")),
            Ref::new("definition:compaction", pinned("artifact.withheld")),
        ),
    }
}

#[test]
fn compaction_boundary_v1_registers_and_stamps_fork_snapshot() {
    let spec = compaction_boundary_v1(&pins(), &boundary_pins(), 1);
    // The document shape (ADR-0077 d7): paired on `compaction_proposal`,
    // `environment_derivation = fork_snapshot`, `boundary_turns = 5`.
    assert_eq!(spec.design.id, "lab/compaction-boundary-v1");
    assert_eq!(spec.design.kind, DesignKind::Paired);
    assert_eq!(spec.arms.len(), 2);
    assert_eq!(
        spec.ext
            .get("environment_derivation")
            .and_then(|v| v.as_str()),
        Some("fork_snapshot")
    );
    assert_eq!(
        spec.ext.get("boundary_turns").and_then(|v| v.as_int()),
        Some(5)
    );
    spec.register(&ctx()).unwrap();
    assert_eq!(
        spec.experiment_id,
        compaction_boundary_v1(&pins(), &boundary_pins(), 1).experiment_id
    );
    // The spec round-trips byte-equal (the ext members are dialect).
    let decoded = ExperimentSpec::from_json(&spec.to_json()).unwrap();
    assert_eq!(decoded, spec);

    let t = tasks(4);
    let plan = expand(&spec, &t, &expand_ctx()).unwrap();
    // Every run plan carries `fork_snapshot` — the companion's task
    // coordinates are `fork_by_reference` cuts at `compaction.started`.
    assert!(plan.run_plans.iter().all(
        |p| p.environment_derivation == hh_lab::experiment::EnvironmentDerivation::ForkSnapshot
    ));
    // An unmarked spec still expands to `fresh_from_image` (the C0 default
    // is never re-keyed by the marker's absence).
    let plain = compaction_family_v1(&pins(), &compaction_pins(), 1);
    let plan2 = expand(&plain, &t, &expand_ctx()).unwrap();
    assert!(plan2
        .run_plans
        .iter()
        .all(|p| p.environment_derivation
            == hh_lab::experiment::EnvironmentDerivation::FreshFromImage));
    // An unknown spelling is a typed schema refusal, never a silent default.
    let mut bad = spec.clone();
    bad.ext.insert(
        "environment_derivation".to_string(),
        hh_wire::json::Json::str("borrowed_vm"),
    );
    assert!(matches!(
        expand(&bad, &t, &expand_ctx()),
        Err(hh_lab::expand::ExpandError::Refusal(
            ExperimentRefusal::Schema(_)
        ))
    ));
}

#[test]
fn summary_fidelity_judged_is_instrument_charged_and_never_headline() {
    let m = summary_fidelity_judged();
    m.validate().expect("the declaration is well-formed");
    assert_eq!(m.name, "grounding.summary_fidelity_judged");
    // ADR-0077 d4: judged fidelity is instrument-charged and never a
    // headline metric.
    assert_eq!(m.charged_to, hh_ontology::eval::ChargedTo::Instrument);
    assert!(!m.headline);
    // `detector = judged`, `oracle = judge` (ADR-0047) — no deterministic
    // path may produce the value.
    use hh_ontology::compliance::Detector;
    use hh_ontology::eval::OracleClass;
    assert_eq!(
        m.detector_classes_allowed,
        [Detector::Judged].into_iter().collect()
    );
    assert_eq!(
        m.oracle_classes_allowed,
        [OracleClass::Judge].into_iter().collect()
    );
    // Judged stages require `model_io`; native rows only (hosted rows read
    // `n/a{class}` — ADR-0075 d9).
    use hh_ontology::participant::Observability;
    assert!(m.requires_observability.contains(&Observability::ModelIo));
    assert_eq!(
        m.applies_to_classes,
        [hh_ontology::participant::ParticipantClass::Native]
            .into_iter()
            .collect()
    );
    // A judged value never enters the headline — flipping `headline` is a
    // typed `HeadlineAdmitsNonDeterministic`, not a warning (AC-R-2.9.2-13).
    let mut bad = m.clone();
    bad.headline = true;
    assert!(matches!(
        bad.validate(),
        Err(hh_ontology::compliance::MetricError::HeadlineAdmitsNonDeterministic { .. })
    ));
}
