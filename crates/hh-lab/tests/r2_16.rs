//! R2.16 — the `declare_*` engine preconditions and the `search_budget =
//! unknown` admission at the spec layer (DF-S1.22-1/DF-S1.22-2; ADR-0046 D1;
//! AC-R-2.9.2 §5h.2 §2). The compare/report halves run in `hh-eval`; the
//! recorded-corpus halves in `hh-bench`.
//!
//! - `declare_arm`'s engine precondition — a sealed configuration whose
//!   `budget_vector` is not the arm's `eval_budget` refuses
//!   `BudgetMismatchWithinArm` at `expand` (CF-099);
//! - `search_budget = unknown` — admissible at register only on a hosted
//!   arm (the level's `class` is the flag); a native arm without one
//!   refuses `UnbudgetedArm` — the unknown is never a silent zero.

use std::collections::BTreeMap;

use hh_budget::spec::{BudgetMode, BudgetSpec};
use hh_budget::{DimensionId, DimensionKey};
use hh_identity::idp::idp_id;
use hh_ontology::config::Ref;
use hh_ontology::lab::SplitLabel;
use hh_ontology::participant::ParticipantClass;

use hh_lab::exemplars::{compaction_family_v1, CompactionFamilyPins, ExemplarPins};
use hh_lab::expand::{expand, ArmConfiguration, ExpandContext, ExpandError, ExpandTask};
use hh_lab::experiment::{ExperimentRefusal, SpecContext};

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
        min_replicates: 1,
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

// ── `declare_arm`'s `BudgetMismatchWithinArm` engine precondition ────────────

/// `expand` enforces `declare_arm`'s CF-099 precondition: the sealed
/// configuration's `budget_vector` ref (the resolver's `budget_ref`) must
/// be the arm's `eval_budget` by pinned-ref identity — a resolver naming
/// another vector refuses `BudgetMismatchWithinArm`, never a dropped cell.
#[test]
fn expand_refuses_budget_mismatch_within_arm() {
    let spec = compaction_family_v1(&pins(), &compaction_pins(), 1);
    spec.register(&ctx()).unwrap();

    // The clean half — the resolver's `budget_ref` names the arm's
    // `eval_budget` exactly.
    let honest = |a: &hh_lab::experiment::ArmSpec| {
        Ok(ArmConfiguration {
            configuration_id: pinned(&format!("cfg.{}", a.arm_id)),
            configuration_version_id: pinned(&format!("cfgv.{}", a.arm_id)),
            budget_ref: Some(a.eval_budget.clone()),
        })
    };
    let t = tasks(2);
    expand(
        &spec,
        &t,
        &ExpandContext {
            arm_config: &honest,
            level_ineligible: None,
        },
    )
    .expect("a configuration bound to the arm's eval_budget expands");

    // The refusal — the sealed configuration's `budget_vector` names a
    // different budget than the arm declared.
    let drifted = |a: &hh_lab::experiment::ArmSpec| {
        Ok(ArmConfiguration {
            configuration_id: pinned(&format!("cfg.{}", a.arm_id)),
            configuration_version_id: pinned(&format!("cfgv.{}", a.arm_id)),
            budget_ref: Some("budget:NOT-eval".to_string()),
        })
    };
    match expand(
        &spec,
        &t,
        &ExpandContext {
            arm_config: &drifted,
            level_ineligible: None,
        },
    ) {
        Err(ExpandError::Refusal(ExperimentRefusal::BudgetMismatchWithinArm {
            arm,
            configuration_budget,
            eval_budget,
        })) => {
            assert_eq!(arm, "arm:evict_oldest");
            assert_eq!(configuration_budget, "budget:NOT-eval");
            assert_eq!(eval_budget, "budget:eval");
        }
        other => panic!("expected BudgetMismatchWithinArm, got {other:?}"),
    }

    // `budget_ref = None` — the resolver carries only ids; the check defers
    // to the record-level `Arm::validate` gate (the config ids still bind).
    let id_only = |a: &hh_lab::experiment::ArmSpec| {
        Ok(ArmConfiguration {
            configuration_id: pinned(&format!("cfg.{}", a.arm_id)),
            configuration_version_id: pinned(&format!("cfgv.{}", a.arm_id)),
            budget_ref: None,
        })
    };
    expand(
        &spec,
        &t,
        &ExpandContext {
            arm_config: &id_only,
            level_ineligible: None,
        },
    )
    .expect("id-only resolvers still expand (the record gate owns the check)");
}

// ── `search_budget = unknown` at the spec layer ──────────────────────────────

/// A hosted arm may carry the first-class `search_budget = unknown`
/// (absence is admissible where a `hosted`-class level is assigned —
/// §6.5 §2.5 A6/ADR-0159 D6); a native arm without one refuses
/// `UnbudgetedArm` at register.
#[test]
fn hosted_arm_admits_unknown_search_budget() {
    // The hosted half — arm B's level resolves `hosted`; the arm declares
    // no search budget and the honest `partial` limits stamp. (A hosted
    // level is admissible only on a product-granularity factor —
    // AC-R-2.10.3-3 — so the hosted participant rides `model_snapshot`,
    // not the component-level `compaction_strategy` axis.)
    let mut spec = compaction_family_v1(&pins(), &compaction_pins(), 1);
    let hosted_arm = &mut spec.arms[1];
    hosted_arm.search_budget = None;
    hosted_arm.limits_enforced = "partial".to_string();
    hosted_arm
        .level_assignment
        .insert("model_snapshot".to_string(), "model:fixed".to_string());
    // The level the arm assigns becomes the hosted one.
    for f in &mut spec.factors {
        for l in &mut f.levels {
            if l.level_id == "model:fixed" {
                l.class = ParticipantClass::Hosted;
            }
        }
    }
    spec.experiment_id = spec.experiment_id();
    spec.register(&ctx())
        .expect("a hosted arm admits search_budget = unknown");

    // The same arm *without* the hosted level — the unknown is inadmissible
    // (a native arm always knows its search spend; `unknown` is never a
    // silent zero).
    let mut spec = compaction_family_v1(&pins(), &compaction_pins(), 1);
    spec.arms[1].search_budget = None;
    spec.experiment_id = spec.experiment_id();
    match spec.register(&ctx()) {
        Err(ExperimentRefusal::UnbudgetedArm { arm }) => {
            assert_eq!(arm, "arm:clear_tool_results")
        }
        other => panic!("expected UnbudgetedArm, got {other:?}"),
    }
}
