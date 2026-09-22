//! `hh-lab` exemplars — the two canonical `ExperimentSpec` documents
//! (ADR-0156 D1/D2; spec §6.3 §2.5 "the registered recipe catalogue";
//! S3.4a): `lab/compaction-family-v1` and `lab/control-strategy-family-v1`
//! instantiated at their Stage-3 sizes.
//!
//! An exemplar is a *document shape*: every member the ADR fixes is fixed
//! here, and every environment-dependent pin (suite/split/snapshot refs,
//! budget refs, artifact refs, level refs) arrives as a parameter — the
//! content address `experiment_id = H(canonical(spec))` then commits the
//! document to the lab's pins. Both constructors produce a spec that
//! `ExperimentSpec::register` admits under a `SpecContext` that can resolve
//! the named refs.
//!
//! `lab/control-strategy-family-v1` carries the ADR-0156 D2 companion arm:
//! one `iso_cost` arm sharing a design point with a `matched_cap` arm (the
//! OQ-363 run-sharing question — ADR-0213's interim rule duplicates the
//! subject runs; a cross-mode `compare` is refused `IncommensurableMatch`).

use std::collections::BTreeMap;

use hh_budget::matchspec::{CachePolicy, MatchSpec, ModelScope};
use hh_budget::pricing::PricingTableRef;
use hh_budget::MatchMode;
use hh_ontology::config::Ref;
use hh_ontology::eval::{
    Design, DesignKind, FactorDeclaration, FactorLevel, Pairing, PreRegistration, SeedPolicy,
};
use hh_ontology::lab::SplitLabel;
use hh_ontology::participant::{Granularity, ParticipantClass};
use hh_ontology::FactorKind;

use crate::experiment::{
    ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentBudgets, ExperimentKind,
    ExperimentSpec, FactorSpec, LevelSpec, OrderKind, ReattemptPolicy, SchedulingPolicy,
    SuiteBinding, ValidationStrategy,
};

/// `tolerance = 0.10` — the packaged-default match tolerance (OQ-124
/// placeholder; §6.3 §2.1 "every default overridable per experiment"), ppm
/// of 1.0.
pub const EXEMPLAR_TOLERANCE_PPM: i64 = 100_000;

/// `replicates_per_cell = 5` — the Stage-3 exemplar replicate count
/// (OQ-121/OQ-339 input; §6.3 §2.1).
pub const EXEMPLAR_REPLICATES: u32 = 5;

/// The pins an exemplar document binds — every ref a pinned content
/// address the registering context can resolve.
#[derive(Debug, Clone, PartialEq)]
pub struct ExemplarPins {
    /// The pinned suite manifest (`suite.suite_ref`).
    pub suite_ref: String,
    /// The pinned held-out split (`design.held_out_split_ref`).
    pub held_out_split_ref: String,
    /// The pinned `SplitAssignmentRecord` (`suite.split_assignment_ref`).
    pub split_assignment_ref: String,
    /// The pinned registry snapshot the `Design` resolves against (§6.2
    /// "one snapshot per `Design`").
    pub registry_snapshot_id: String,
    /// The pinned `eval_budget` ref every arm shares.
    pub eval_budget: String,
    /// The pinned `search_budget` ref — `search_budget: 0` (a registered
    /// zero-cap budget document; matched kinds require the member).
    pub search_budget: String,
    /// The experiment-run pool budget ref (`budgets.experiment`).
    pub experiment_budget: String,
    /// The instrument budget ref (`budgets.instrument`).
    pub instrument_budget: String,
    /// The pinned analysis-plan ref (`pre_registration.analysis_plan_ref`).
    pub analysis_plan_ref: String,
    /// The `sha256:` task-split hash (`pre_registration.task_split_hash` —
    /// predates any search, ADR-0143 L3).
    pub task_split_hash: String,
}

fn seed_policy(honoured_required: bool) -> SeedPolicy {
    SeedPolicy {
        harness_rng: true,
        requested_sampling_seed: false,
        seed_honoured_required: honoured_required,
    }
}

fn scheduling(permutation_seed: &str) -> SchedulingPolicy {
    SchedulingPolicy {
        max_concurrent_runs: 4,
        pools: Vec::new(),
        order: OrderKind::InterleavedBlocked,
        permutation_seed: permutation_seed.to_string(),
        start_stagger_ms: 0,
        deadline: None,
        priority: None,
    }
}

fn reattempt() -> ReattemptPolicy {
    // The packaged defaults (OQ-360 placeholders; §6.3 §2.1).
    ReattemptPolicy {
        max_per_plan: 2,
        max_fraction_of_plans_ppm: 100_000,
        backoff: Backoff {
            min_ms: 1_000,
            multiplier_ppm: 2_000_000,
            max_ms: 60_000,
        },
        error_classes_included: None,
        on_cancel: CancelPolicy::Replan,
    }
}

fn budgets(pins: &ExemplarPins) -> ExperimentBudgets {
    ExperimentBudgets {
        experiment: pins.experiment_budget.clone(),
        instrument: pins.instrument_budget.clone(),
    }
}

fn level(level_id: &str, ref_: &str, label: &str) -> LevelSpec {
    LevelSpec {
        level_id: level_id.to_string(),
        ref_: ref_.to_string(),
        overrides: None,
        label: label.to_string(),
        class: ParticipantClass::Native,
        non_portable: false,
    }
}

fn decl_level(level_id: &str, content_ref: &str, label: &str) -> FactorLevel {
    FactorLevel {
        id: level_id.to_string(),
        content_ref: content_ref.to_string(),
        label: label.to_string(),
    }
}

fn pre_registration(
    registered_at: u64,
    hypothesis: &str,
    primary_metrics: &[&str],
    interactions: &[&str],
    pins: &ExemplarPins,
) -> PreRegistration {
    PreRegistration {
        registered_at,
        hypothesis: hypothesis.to_string(),
        primary_metrics: primary_metrics.iter().map(|s| s.to_string()).collect(),
        equivalence_margin: None,
        min_n: EXEMPLAR_REPLICATES,
        analysis_plan_ref: pins.analysis_plan_ref.clone(),
        task_split_hash: pins.task_split_hash.clone(),
        interactions: interactions.iter().map(|s| s.to_string()).collect(),
    }
}

fn arm(
    arm_id: &str,
    hypothesis: &str,
    levels: &[(&str, &str)],
    match_spec: MatchSpec,
    artifact: &Ref,
    pins: &ExemplarPins,
) -> ArmSpec {
    ArmSpec {
        arm_id: arm_id.to_string(),
        hypothesis: hypothesis.to_string(),
        level_assignment: levels
            .iter()
            .map(|(f, l)| (f.to_string(), l.to_string()))
            .collect(),
        eval_budget: pins.eval_budget.clone(),
        search_budget: Some(pins.search_budget.clone()),
        match_spec: Some(match_spec),
        artifact_ref: artifact.clone(),
        limits_enforced: "full".to_string(),
        model_role_table_ref: None,
    }
}

// ── lab/compaction-family-v1 (ADR-0156 D1; ADR-0077 executed) ───────────────

/// The level pins `lab/compaction-family-v1` binds, beyond [`ExemplarPins`].
#[derive(Debug, Clone, PartialEq)]
pub struct CompactionFamilyPins {
    /// The `evict_oldest` compaction-strategy variant ref.
    pub evict_oldest_ref: String,
    /// The `clear_tool_results` compaction-strategy variant ref.
    pub clear_tool_results_ref: String,
    /// The single model level ref (roles fixed incl. `summarizer_profile`).
    pub model_level_ref: String,
    /// The single environment level ref (Terminal-Bench 2.0 family,
    /// `fault_profile = none`).
    pub environment_level_ref: String,
    /// The sealed artifact (definition version) each strategy arm binds —
    /// `(evict_oldest, clear_tool_results)` order.
    pub artifacts: (Ref, Ref),
}

/// The matched dims — `tokens.input.total`/`tokens.output.total` are derived
/// names (§8.2), so the match pins the kernel constituents: input =
/// `{uncached, cache_read, cache_write}`, output = `{visible, reasoning}`;
/// equal constituents imply equal totals.
fn compaction_match() -> MatchSpec {
    use hh_ontology::dimensions::DimensionId::*;
    MatchSpec {
        dimensions: vec![
            TokensInputUncached,
            TokensInputCacheRead,
            TokensInputCacheWrite,
            TokensOutputVisible,
            TokensOutputReasoning,
            ModelCalls,
            TimeWallMs,
        ],
        mode: MatchMode::MatchedCap,
        tolerance_ppm: EXEMPLAR_TOLERANCE_PPM,
        pricing_table_ref: None,
        model_scope: ModelScope::SameSnapshot,
        cache_policy: CachePolicy::ColdStart,
    }
}

/// `lab/compaction-family-v1` at its Stage-3 size (ADR-0156 D1):
/// `comparative`, `paired` on `harness` at `component-level`
/// (`compaction_strategy ∈ {evict_oldest, clear_tool_results}`), one model
/// level, one environment level, suite-A held-out split,
/// `MatchSpec{matched_cap, tolerance 0.10, same_snapshot, cold_start}`,
/// `replicates_per_cell: 5`, `search_budget: 0`, H2 pre-registered with
/// primaries `{capability.task_success, grounding.reacquisition_rate,
/// reliability.repeated_action_rate}`, `validation_strategy: full_set`,
/// `scheduling{order: interleaved_blocked}`.
///
/// `registered_at` is the transaction seq the pre-registration commits at
/// (a `seq`, never a wall clock).
pub fn compaction_family_v1(
    pins: &ExemplarPins,
    own: &CompactionFamilyPins,
    registered_at: u64,
) -> ExperimentSpec {
    let prereg = pre_registration(
        registered_at,
        "H2 — clear_tool_results ≥ evict_oldest on capability.task_success at matched budget \
         (margins per OQ-113 interim)",
        &[
            "capability.task_success",
            "grounding.reacquisition_rate",
            "reliability.repeated_action_rate",
        ],
        &[],
        pins,
    );
    let mut spec = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: "lab/compaction-family-v1".to_string(),
            kind: DesignKind::Paired,
            factors: vec![
                FactorDeclaration {
                    name: "compaction_strategy".to_string(),
                    kind: FactorKind::Harness,
                    granularity: Some(Granularity::ComponentLevel),
                    levels: vec![
                        decl_level("evict_oldest", &own.evict_oldest_ref, "evict oldest"),
                        decl_level(
                            "clear_tool_results",
                            &own.clear_tool_results_ref,
                            "clear tool results",
                        ),
                    ],
                    role: None,
                },
                FactorDeclaration {
                    name: "model_snapshot".to_string(),
                    kind: FactorKind::ModelSnapshot,
                    granularity: None,
                    levels: vec![decl_level("model:fixed", &own.model_level_ref, "fixed model")],
                    role: None,
                },
                FactorDeclaration {
                    name: "environment".to_string(),
                    kind: FactorKind::Environment,
                    granularity: None,
                    levels: vec![decl_level(
                        "terminal-bench-2.0",
                        &own.environment_level_ref,
                        "Terminal-Bench 2.0 (fault_profile = none)",
                    )],
                    role: None,
                },
            ],
            blocking: vec!["task".to_string()],
            replicates_per_cell: EXEMPLAR_REPLICATES,
            pairing: Pairing::ByTask,
            seed_policy: seed_policy(false),
            held_out_split_ref: Some(pins.held_out_split_ref.clone()),
            pre_registration: prereg.clone(),
            registry_snapshot_id: Some(pins.registry_snapshot_id.clone()),
            generators: None,
            resolution: None,
        },
        pre_registration: Some(prereg),
        factors: vec![
            FactorSpec {
                name: "compaction_strategy".to_string(),
                kind: FactorKind::Harness,
                granularity: Some(Granularity::ComponentLevel),
                role: Some("primary".to_string()),
                levels: vec![
                    level("evict_oldest", &own.evict_oldest_ref, "evict oldest"),
                    level(
                        "clear_tool_results",
                        &own.clear_tool_results_ref,
                        "clear tool results",
                    ),
                ],
            },
            FactorSpec {
                name: "model_snapshot".to_string(),
                kind: FactorKind::ModelSnapshot,
                granularity: None,
                role: Some("blocking".to_string()),
                levels: vec![level("model:fixed", &own.model_level_ref, "fixed model")],
            },
            FactorSpec {
                name: "environment".to_string(),
                kind: FactorKind::Environment,
                granularity: None,
                role: Some("blocking".to_string()),
                levels: vec![level(
                    "terminal-bench-2.0",
                    &own.environment_level_ref,
                    "Terminal-Bench 2.0 (fault_profile = none)",
                )],
            },
        ],
        arms: vec![
            arm(
                "arm:evict_oldest",
                "evict_oldest baseline",
                &[("compaction_strategy", "evict_oldest")],
                compaction_match(),
                &own.artifacts.0,
                pins,
            ),
            arm(
                "arm:clear_tool_results",
                "clear_tool_results does better",
                &[("compaction_strategy", "clear_tool_results")],
                compaction_match(),
                &own.artifacts.1,
                pins,
            ),
        ],
        suite: SuiteBinding {
            suite_ref: pins.suite_ref.clone(),
            split_labels_used: vec![SplitLabel::HeldOut],
            split_assignment_ref: Some(pins.split_assignment_ref.clone()),
        },
        replicates_per_cell: EXEMPLAR_REPLICATES,
        seed_policy: seed_policy(false),
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: scheduling("perm:lab.compaction-family-v1"),
        reattempt: reattempt(),
        budgets: budgets(pins),
        bundle_policy: BundlePolicy::Named {
            name: "lab/compaction-family-v1".to_string(),
        },
        ext: BTreeMap::new(),
    };
    spec.experiment_id = spec.experiment_id();
    spec
}

// ── lab/control-strategy-family-v1 (ADR-0156 D2; ADR-0105 executed) ─────────

/// The level pins `lab/control-strategy-family-v1` binds, beyond
/// [`ExemplarPins`].
#[derive(Debug, Clone, PartialEq)]
pub struct ControlStrategyFamilyPins {
    /// The `react/minimal` control-strategy variant ref.
    pub react_minimal_ref: String,
    /// The `react/steerable` control-strategy variant ref.
    pub react_steerable_ref: String,
    /// The first pinned model family snapshot.
    pub model_family_a_ref: String,
    /// The second pinned model family snapshot.
    pub model_family_b_ref: String,
    /// The sealed artifacts the four product arms bind, keyed
    /// `(minimal×A, minimal×B, steerable×A, steerable×B)`.
    pub artifacts: (Ref, Ref, Ref, Ref),
    /// The artifact the `iso_cost` companion arm binds — the same sealed
    /// configuration as its shared design point under the OQ-363 interim
    /// rule (the engine duplicates the subject runs).
    pub iso_artifact: Ref,
    /// The pinned pricing table — mandatory on the `iso_cost` arm.
    pub pricing_table_ref: PricingTableRef,
}

/// The matched dims of the `matched_cap` group: `{model_calls,
/// tokens.output.visible, time.wall_ms}` (ADR-0156 D2).
fn control_matched_cap() -> MatchSpec {
    use hh_ontology::dimensions::DimensionId::*;
    MatchSpec {
        dimensions: vec![ModelCalls, TokensOutputVisible, TimeWallMs],
        mode: MatchMode::MatchedCap,
        tolerance_ppm: EXEMPLAR_TOLERANCE_PPM,
        pricing_table_ref: None,
        model_scope: ModelScope::SameSnapshot,
        cache_policy: CachePolicy::ColdStart,
    }
}

/// `lab/control-strategy-family-v1` at its Stage-3 first execution
/// (ADR-0156 D2): `comparative`, `full_factorial` over `control_strategy ∈
/// {react/minimal, react/steerable}` × `model_snapshot` (two families), one
/// coding suite held-out split, `MatchSpec{matched_cap, cold_start}` on the
/// four product arms **plus one `iso_cost` companion arm** sharing the
/// `{react/steerable, family-a}` design point with its own `MatchSpec` — a
/// direct `compare` across the two modes is refused `IncommensurableMatch`
/// (CF-334; run-sharing is OQ-363 — ADR-0213's interim rule duplicates the
/// runs). `replicates_per_cell: 5`,
/// `seed_policy{seed_honoured_required: false}`, H0–H4 with primaries
/// `{capability.task_success, control.reproducibility,
/// efficiency.model_calls}`; an unmet `capabilities.requires` surfaces as
/// `n/a{class}` cells at `expand` (the `level_ineligible` view marks them —
/// T-LCD-15).
pub fn control_strategy_family_v1(
    pins: &ExemplarPins,
    own: &ControlStrategyFamilyPins,
    registered_at: u64,
) -> ExperimentSpec {
    let prereg = pre_registration(
        registered_at,
        "H0–H4 — react/steerable vs react/minimal on capability.task_success, \
         control.reproducibility and efficiency.model_calls, with the \
         control_strategy × model_snapshot interaction (ADR-0105)",
        &[
            "capability.task_success",
            "control.reproducibility",
            "efficiency.model_calls",
        ],
        &["control_strategy:model_snapshot"],
        pins,
    );
    let matched = control_matched_cap();
    let iso = MatchSpec {
        mode: MatchMode::IsoCost,
        pricing_table_ref: Some(own.pricing_table_ref.clone()),
        ..control_matched_cap()
    };
    let product_arm = |id: &str, hyp: &str, cs: &str, ms: &str, artifact: &Ref| {
        arm(
            id,
            hyp,
            &[("control_strategy", cs), ("model_snapshot", ms)],
            matched.clone(),
            artifact,
            pins,
        )
    };
    let mut spec = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: "lab/control-strategy-family-v1".to_string(),
            kind: DesignKind::FullFactorial,
            factors: vec![
                FactorDeclaration {
                    name: "control_strategy".to_string(),
                    kind: FactorKind::Harness,
                    granularity: Some(Granularity::ComponentLevel),
                    levels: vec![
                        decl_level("react/minimal", &own.react_minimal_ref, "react · minimal"),
                        decl_level(
                            "react/steerable",
                            &own.react_steerable_ref,
                            "react · steerable",
                        ),
                    ],
                    role: None,
                },
                FactorDeclaration {
                    name: "model_snapshot".to_string(),
                    kind: FactorKind::ModelSnapshot,
                    granularity: None,
                    levels: vec![
                        decl_level("family-a", &own.model_family_a_ref, "model family A"),
                        decl_level("family-b", &own.model_family_b_ref, "model family B"),
                    ],
                    role: None,
                },
            ],
            blocking: vec!["task".to_string()],
            replicates_per_cell: EXEMPLAR_REPLICATES,
            pairing: Pairing::ByTask,
            seed_policy: seed_policy(false),
            held_out_split_ref: Some(pins.held_out_split_ref.clone()),
            pre_registration: prereg.clone(),
            registry_snapshot_id: Some(pins.registry_snapshot_id.clone()),
            generators: None,
            resolution: None,
        },
        pre_registration: Some(prereg),
        factors: vec![
            FactorSpec {
                name: "control_strategy".to_string(),
                kind: FactorKind::Harness,
                granularity: Some(Granularity::ComponentLevel),
                role: Some("primary".to_string()),
                levels: vec![
                    level("react/minimal", &own.react_minimal_ref, "react · minimal"),
                    level("react/steerable", &own.react_steerable_ref, "react · steerable"),
                ],
            },
            FactorSpec {
                name: "model_snapshot".to_string(),
                kind: FactorKind::ModelSnapshot,
                granularity: None,
                role: Some("blocking".to_string()),
                levels: vec![
                    level("family-a", &own.model_family_a_ref, "model family A"),
                    level("family-b", &own.model_family_b_ref, "model family B"),
                ],
            },
        ],
        arms: vec![
            product_arm(
                "arm:minimal-a",
                "react/minimal on family A",
                "react/minimal",
                "family-a",
                &own.artifacts.0,
            ),
            product_arm(
                "arm:minimal-b",
                "react/minimal on family B",
                "react/minimal",
                "family-b",
                &own.artifacts.1,
            ),
            product_arm(
                "arm:steerable-a",
                "react/steerable on family A",
                "react/steerable",
                "family-a",
                &own.artifacts.2,
            ),
            product_arm(
                "arm:steerable-b",
                "react/steerable on family B",
                "react/steerable",
                "family-b",
                &own.artifacts.3,
            ),
            arm(
                "arm:steerable-a-iso",
                "react/steerable on family A at equal realized cost (iso_cost companion — \
                 no cross-mode compare; OQ-363 interim duplicates runs)",
                &[("control_strategy", "react/steerable"), ("model_snapshot", "family-a")],
                iso,
                &own.iso_artifact,
                pins,
            ),
        ],
        suite: SuiteBinding {
            suite_ref: pins.suite_ref.clone(),
            split_labels_used: vec![SplitLabel::HeldOut],
            split_assignment_ref: Some(pins.split_assignment_ref.clone()),
        },
        replicates_per_cell: EXEMPLAR_REPLICATES,
        seed_policy: seed_policy(false),
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: scheduling("perm:lab.control-strategy-family-v1"),
        reattempt: reattempt(),
        budgets: budgets(pins),
        bundle_policy: BundlePolicy::Named {
            name: "lab/control-strategy-family-v1".to_string(),
        },
        ext: BTreeMap::new(),
    };
    spec.experiment_id = spec.experiment_id();
    spec
}
