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
    Design, DesignKind, FactorDeclaration, FactorLevel, Pairing, PreRegistration, RoutingPolicy,
    SeedPolicy,
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
        inference_budget: None,
        match_spec: Some(match_spec),
        artifact_ref: artifact.clone(),
        limits_enforced: "full".to_string(),
        model_role_table_ref: None,
        response_cache: None,
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
        utilization_floor_ppm: None,
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
                    levels: vec![decl_level(
                        "model:fixed",
                        &own.model_level_ref,
                        "fixed model",
                    )],
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
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
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
        utilization_floor_ppm: None,
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
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
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
                    level(
                        "react/steerable",
                        &own.react_steerable_ref,
                        "react · steerable",
                    ),
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
                &[
                    ("control_strategy", "react/steerable"),
                    ("model_snapshot", "family-a"),
                ],
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

// ── lab/context-builder-v1 (AC-R-2.4.1-12) ───────────────────────────────────

/// The level pins `lab/context-builder-v1` binds.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextBuilderPins {
    /// `rp_on` level ref (the full relevance-policy builder).
    pub rp_on_ref: String,
    /// `banner_only` level ref (the banner-only RP arm).
    pub banner_only_ref: String,
    /// `handle_only` catalogs level ref.
    pub handle_only_ref: String,
    /// `expanded` catalogs level ref.
    pub expanded_catalogs_ref: String,
    /// `clearing_on` level ref (clearing on).
    pub clearing_on_ref: String,
    /// `clearing_off` level ref.
    pub clearing_off_ref: String,
    /// The single model level ref.
    pub model_level_ref: String,
    /// The single environment level ref.
    pub environment_level_ref: String,
    /// The sealed artifacts each cell binds, in the arm order below.
    pub artifacts: Vec<Ref>,
}

fn context_builder_match() -> MatchSpec {
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
        utilization_floor_ppm: None,
    }
}

/// `lab/context-builder-v1` (AC-R-2.4.1-12): the utility-under-matched-budget
/// recipe — `rp ∈ {on, banner_only}` × `catalogs ∈ {handle_only, expanded}` ×
/// `clearing ∈ {on, off}` as a full-factorial 8-arm design under
/// `MatchSpec{matched_cap, tolerance 0.10, same_snapshot, cold_start}`,
/// reporting success, compliance, approvals and cost (the `external`-slot
/// compliance cost is measured, never assumed).
pub fn context_builder_v1(
    pins: &ExemplarPins,
    own: &ContextBuilderPins,
    registered_at: u64,
) -> ExperimentSpec {
    let prereg = pre_registration(
        registered_at,
        "RP/catalog/clearing utility under matched budget — reporting success, \
         compliance, approvals and cost per cell (AC-R-2.4.1-12)",
        &[
            "capability.task_success",
            "security.compliance_rate",
            "efficiency.spend",
        ],
        &["rp:catalogs", "rp:clearing", "catalogs:clearing"],
        pins,
    );
    let factor = |name: &str, levels: Vec<FactorLevel>| FactorDeclaration {
        name: name.to_string(),
        kind: FactorKind::Harness,
        granularity: Some(Granularity::ComponentLevel),
        levels,
        role: None,
    };
    let rp_levels = vec![
        decl_level("on", &own.rp_on_ref, "relevance policy on"),
        decl_level("banner_only", &own.banner_only_ref, "banner only"),
    ];
    let cat_levels = vec![
        decl_level("handle_only", &own.handle_only_ref, "handle-only catalogs"),
        decl_level("expanded", &own.expanded_catalogs_ref, "expanded catalogs"),
    ];
    let clr_levels = vec![
        decl_level("on", &own.clearing_on_ref, "clearing on"),
        decl_level("off", &own.clearing_off_ref, "clearing off"),
    ];
    let mut arms = Vec::new();
    let mut i = 0usize;
    for rp in ["on", "banner_only"] {
        for cat in ["handle_only", "expanded"] {
            for clr in ["on", "off"] {
                arms.push(arm(
                    &format!("arm:rp-{rp}.cat-{cat}.clr-{clr}"),
                    &format!("rp={rp} catalogs={cat} clearing={clr}"),
                    &[
                        ("rp", rp),
                        ("catalogs", cat),
                        ("clearing", clr),
                        ("model_snapshot", "model:fixed"),
                        ("environment", "terminal-bench-2.0"),
                    ],
                    context_builder_match(),
                    &own.artifacts[i],
                    pins,
                ));
                i += 1;
            }
        }
    }
    let mut spec = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: "lab/context-builder-v1".to_string(),
            kind: DesignKind::FullFactorial,
            factors: vec![
                factor("rp", rp_levels.clone()),
                factor("catalogs", cat_levels.clone()),
                factor("clearing", clr_levels.clone()),
                FactorDeclaration {
                    name: "model_snapshot".to_string(),
                    kind: FactorKind::ModelSnapshot,
                    granularity: None,
                    levels: vec![decl_level(
                        "model:fixed",
                        &own.model_level_ref,
                        "fixed model",
                    )],
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
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: Some(prereg),
        factors: vec![
            FactorSpec {
                name: "rp".to_string(),
                kind: FactorKind::Harness,
                granularity: Some(Granularity::ComponentLevel),
                role: Some("primary".to_string()),
                levels: rp_levels
                    .iter()
                    .map(|l| level(&l.id, &l.content_ref, &l.label))
                    .collect(),
            },
            FactorSpec {
                name: "catalogs".to_string(),
                kind: FactorKind::Harness,
                granularity: Some(Granularity::ComponentLevel),
                role: Some("primary".to_string()),
                levels: cat_levels
                    .iter()
                    .map(|l| level(&l.id, &l.content_ref, &l.label))
                    .collect(),
            },
            FactorSpec {
                name: "clearing".to_string(),
                kind: FactorKind::Harness,
                granularity: Some(Granularity::ComponentLevel),
                role: Some("primary".to_string()),
                levels: clr_levels
                    .iter()
                    .map(|l| level(&l.id, &l.content_ref, &l.label))
                    .collect(),
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
        arms,
        suite: SuiteBinding {
            suite_ref: pins.suite_ref.clone(),
            split_labels_used: vec![SplitLabel::HeldOut],
            split_assignment_ref: Some(pins.split_assignment_ref.clone()),
        },
        replicates_per_cell: EXEMPLAR_REPLICATES,
        seed_policy: seed_policy(false),
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: scheduling("perm:lab.context-builder-v1"),
        reattempt: reattempt(),
        budgets: budgets(pins),
        bundle_policy: BundlePolicy::Named {
            name: "lab/context-builder-v1".to_string(),
        },
        ext: BTreeMap::new(),
    };
    spec.experiment_id = spec.experiment_id();
    spec
}

// ── lab/memory-compliance-v1 (AC-R-2.4.3-9) ──────────────────────────────────

/// The level pins `lab/memory-compliance-v1` binds.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryCompliancePins {
    /// `external_slot` arm ref — memories delivered in an `external` slot.
    pub external_slot_ref: String,
    /// `promotion_endorsed` arm ref — the same memories promotion-endorsed.
    pub promotion_endorsed_ref: String,
    /// The single model level ref.
    pub model_level_ref: String,
    /// The single environment level ref.
    pub environment_level_ref: String,
    /// The sealed artifacts the two arms bind.
    pub artifacts: (Ref, Ref),
}

/// `lab/memory-compliance-v1` (AC-R-2.4.3-9): the compliance-chain arm —
/// "memories in an `external` slot vs the same memories `promotion`-endorsed"
/// under matched budget; primaries report P(activated | delivered) per
/// profile plus the compliance rows.
pub fn memory_compliance_v1(
    pins: &ExemplarPins,
    own: &MemoryCompliancePins,
    registered_at: u64,
) -> ExperimentSpec {
    let prereg = pre_registration(
        registered_at,
        "promotion-endorsed memory delivery ≥ external-slot on \
         P(activated|delivered) at matched budget (AC-R-2.4.3-9)",
        &[
            "memory.activated_given_delivered",
            "memory.followed",
            "capability.task_success",
        ],
        &[],
        pins,
    );
    let levels = vec![
        decl_level("external_slot", &own.external_slot_ref, "external slot"),
        decl_level(
            "promotion_endorsed",
            &own.promotion_endorsed_ref,
            "promotion endorsed",
        ),
    ];
    let mut spec = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: "lab/memory-compliance-v1".to_string(),
            kind: DesignKind::Paired,
            factors: vec![
                FactorDeclaration {
                    name: "memory_delivery".to_string(),
                    kind: FactorKind::Harness,
                    granularity: Some(Granularity::ComponentLevel),
                    levels: levels.clone(),
                    role: None,
                },
                FactorDeclaration {
                    name: "model_snapshot".to_string(),
                    kind: FactorKind::ModelSnapshot,
                    granularity: None,
                    levels: vec![decl_level(
                        "model:fixed",
                        &own.model_level_ref,
                        "fixed model",
                    )],
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
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: Some(prereg),
        factors: vec![
            FactorSpec {
                name: "memory_delivery".to_string(),
                kind: FactorKind::Harness,
                granularity: Some(Granularity::ComponentLevel),
                role: Some("primary".to_string()),
                levels: levels
                    .iter()
                    .map(|l| level(&l.id, &l.content_ref, &l.label))
                    .collect(),
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
                "arm:external_slot",
                "external-slot delivery",
                &[
                    ("memory_delivery", "external_slot"),
                    ("model_snapshot", "model:fixed"),
                    ("environment", "terminal-bench-2.0"),
                ],
                context_builder_match(),
                &own.artifacts.0,
                pins,
            ),
            arm(
                "arm:promotion_endorsed",
                "promotion-endorsed delivery",
                &[
                    ("memory_delivery", "promotion_endorsed"),
                    ("model_snapshot", "model:fixed"),
                    ("environment", "terminal-bench-2.0"),
                ],
                context_builder_match(),
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
        scheduling: scheduling("perm:lab.memory-compliance-v1"),
        reattempt: reattempt(),
        budgets: budgets(pins),
        bundle_policy: BundlePolicy::Named {
            name: "lab/memory-compliance-v1".to_string(),
        },
        ext: BTreeMap::new(),
    };
    spec.experiment_id = spec.experiment_id();
    spec
}

// ── lab/memory-validity-v1 (AC-R-2.4.4-12) ───────────────────────────────────

/// The level pins `lab/memory-validity-v1` binds.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryValidityPins {
    /// `validity_filter` level refs `(off, on)`.
    pub validity_filter_refs: (String, String),
    /// `justification_scope` level refs `(delivered, subject_overlap)`.
    pub justification_scope_refs: (String, String),
    /// `conflict_policy` level refs `(deliver_all_annotated, withhold_all)`.
    pub conflict_policy_refs: (String, String),
    /// The single model level ref.
    pub model_level_ref: String,
    /// The single environment level ref.
    pub environment_level_ref: String,
    /// The sealed artifacts the 8 cells bind, in arm order.
    pub artifacts: Vec<Ref>,
}

/// `lab/memory-validity-v1` (AC-R-2.4.4-12): `validity_filter ∈ {off, on}` ×
/// `justification_scope ∈ {delivered, subject_overlap}` × `conflict_policy ∈
/// {deliver_all_annotated, withhold_all}` under `MatchSpec` — reporting
/// success, memories withheld, over-invalidation, approvals and cost. The
/// C2 narrowing (`subject_overlap`) is admissible only from this report.
pub fn memory_validity_v1(
    pins: &ExemplarPins,
    own: &MemoryValidityPins,
    registered_at: u64,
) -> ExperimentSpec {
    let prereg = pre_registration(
        registered_at,
        "validity narrowing utility — subject_overlap admitted only on this \
         ComparisonReport's evidence (AC-R-2.4.4-12)",
        &[
            "capability.task_success",
            "memory.withheld",
            "memory.over_invalidation",
            "efficiency.spend",
        ],
        &["validity_filter:justification_scope"],
        pins,
    );
    let vf = vec![
        decl_level("off", &own.validity_filter_refs.0, "validity filter off"),
        decl_level("on", &own.validity_filter_refs.1, "validity filter on"),
    ];
    let js = vec![
        decl_level("delivered", &own.justification_scope_refs.0, "delivered"),
        decl_level(
            "subject_overlap",
            &own.justification_scope_refs.1,
            "subject overlap",
        ),
    ];
    let cp = vec![
        decl_level(
            "deliver_all_annotated",
            &own.conflict_policy_refs.0,
            "deliver all annotated",
        ),
        decl_level("withhold_all", &own.conflict_policy_refs.1, "withhold all"),
    ];
    let mut arms = Vec::new();
    let mut i = 0usize;
    for v in ["off", "on"] {
        for j in ["delivered", "subject_overlap"] {
            for c in ["deliver_all_annotated", "withhold_all"] {
                arms.push(arm(
                    &format!("arm:vf-{v}.js-{j}.cp-{c}"),
                    &format!("vf={v} js={j} cp={c}"),
                    &[
                        ("validity_filter", v),
                        ("justification_scope", j),
                        ("conflict_policy", c),
                        ("model_snapshot", "model:fixed"),
                        ("environment", "terminal-bench-2.0"),
                    ],
                    context_builder_match(),
                    &own.artifacts[i],
                    pins,
                ));
                i += 1;
            }
        }
    }
    let factor = |name: &str, levels: Vec<FactorLevel>| FactorDeclaration {
        name: name.to_string(),
        kind: FactorKind::Harness,
        granularity: Some(Granularity::ComponentLevel),
        levels,
        role: None,
    };
    let spec_f = |name: &str, levels: Vec<FactorLevel>| FactorSpec {
        name: name.to_string(),
        kind: FactorKind::Harness,
        granularity: Some(Granularity::ComponentLevel),
        role: Some("primary".to_string()),
        levels: levels
            .iter()
            .map(|l| level(&l.id, &l.content_ref, &l.label))
            .collect(),
    };
    let mut spec = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: "lab/memory-validity-v1".to_string(),
            kind: DesignKind::FullFactorial,
            factors: vec![
                factor("validity_filter", vf.clone()),
                factor("justification_scope", js.clone()),
                factor("conflict_policy", cp.clone()),
                FactorDeclaration {
                    name: "model_snapshot".to_string(),
                    kind: FactorKind::ModelSnapshot,
                    granularity: None,
                    levels: vec![decl_level(
                        "model:fixed",
                        &own.model_level_ref,
                        "fixed model",
                    )],
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
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: Some(prereg),
        factors: vec![
            spec_f("validity_filter", vf),
            spec_f("justification_scope", js),
            spec_f("conflict_policy", cp),
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
        arms,
        suite: SuiteBinding {
            suite_ref: pins.suite_ref.clone(),
            split_labels_used: vec![SplitLabel::HeldOut],
            split_assignment_ref: Some(pins.split_assignment_ref.clone()),
        },
        replicates_per_cell: EXEMPLAR_REPLICATES,
        seed_policy: seed_policy(false),
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: scheduling("perm:lab.memory-validity-v1"),
        reattempt: reattempt(),
        budgets: budgets(pins),
        bundle_policy: BundlePolicy::Named {
            name: "lab/memory-validity-v1".to_string(),
        },
        ext: BTreeMap::new(),
    };
    spec.experiment_id = spec.experiment_id();
    spec
}

// ── lab/procedure-execution-v1 (AC-R-2.4.5-10) ───────────────────────────────

/// The level pins `lab/procedure-execution-v1` binds.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureExecutionPins {
    /// `target` level refs `(instruction, workflow_node)`.
    pub target_refs: (String, String),
    /// `profile` level refs `(P1, P2)`.
    pub profile_refs: (String, String),
    /// The single model level ref.
    pub model_level_ref: String,
    /// The single environment level ref.
    pub environment_level_ref: String,
    /// The sealed artifacts the 4 cells bind, in arm order
    /// `(instruction×P1, instruction×P2, workflow_node×P1, workflow_node×P2)`.
    pub artifacts: (Ref, Ref, Ref, Ref),
}

/// `lab/procedure-execution-v1` (AC-R-2.4.5-10): `target ∈ {instruction,
/// workflow_node} × profile ∈ {P1, P2}` as a two-factor `full_factorial`
/// design under `MatchSpec`, reporting the `target:profile` interaction.
pub fn procedure_execution_v1(
    pins: &ExemplarPins,
    own: &ProcedureExecutionPins,
    registered_at: u64,
) -> ExperimentSpec {
    let prereg = pre_registration(
        registered_at,
        "target × profile interaction on procedure execution \
         (AC-R-2.4.5-10; ADR-0085 d8)",
        &[
            "capability.task_success",
            "memory.activated_given_delivered",
            "efficiency.spend",
        ],
        &["target:profile"],
        pins,
    );
    let tgt = vec![
        decl_level("instruction", &own.target_refs.0, "instruction"),
        decl_level("workflow_node", &own.target_refs.1, "workflow node"),
    ];
    let prof = vec![
        decl_level("P1", &own.profile_refs.0, "profile P1"),
        decl_level("P2", &own.profile_refs.1, "profile P2"),
    ];
    let artifacts = [
        &own.artifacts.0,
        &own.artifacts.1,
        &own.artifacts.2,
        &own.artifacts.3,
    ];
    let mut arms = Vec::new();
    let mut i = 0usize;
    for t in ["instruction", "workflow_node"] {
        for p in ["P1", "P2"] {
            arms.push(arm(
                &format!("arm:{t}-{p}"),
                &format!("target={t} profile={p}"),
                &[
                    ("target", t),
                    ("profile", p),
                    ("model_snapshot", "model:fixed"),
                    ("environment", "terminal-bench-2.0"),
                ],
                context_builder_match(),
                artifacts[i],
                pins,
            ));
            i += 1;
        }
    }
    let mut spec = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: "lab/procedure-execution-v1".to_string(),
            kind: DesignKind::FullFactorial,
            factors: vec![
                FactorDeclaration {
                    name: "target".to_string(),
                    kind: FactorKind::Harness,
                    granularity: Some(Granularity::ComponentLevel),
                    levels: tgt.clone(),
                    role: None,
                },
                FactorDeclaration {
                    name: "profile".to_string(),
                    kind: FactorKind::Harness,
                    granularity: Some(Granularity::ComponentLevel),
                    levels: prof.clone(),
                    role: None,
                },
                FactorDeclaration {
                    name: "model_snapshot".to_string(),
                    kind: FactorKind::ModelSnapshot,
                    granularity: None,
                    levels: vec![decl_level(
                        "model:fixed",
                        &own.model_level_ref,
                        "fixed model",
                    )],
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
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: Some(prereg),
        factors: vec![
            FactorSpec {
                name: "target".to_string(),
                kind: FactorKind::Harness,
                granularity: Some(Granularity::ComponentLevel),
                role: Some("primary".to_string()),
                levels: tgt
                    .iter()
                    .map(|l| level(&l.id, &l.content_ref, &l.label))
                    .collect(),
            },
            FactorSpec {
                name: "profile".to_string(),
                kind: FactorKind::Harness,
                granularity: Some(Granularity::ComponentLevel),
                role: Some("primary".to_string()),
                levels: prof
                    .iter()
                    .map(|l| level(&l.id, &l.content_ref, &l.label))
                    .collect(),
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
        arms,
        suite: SuiteBinding {
            suite_ref: pins.suite_ref.clone(),
            split_labels_used: vec![SplitLabel::HeldOut],
            split_assignment_ref: Some(pins.split_assignment_ref.clone()),
        },
        replicates_per_cell: EXEMPLAR_REPLICATES,
        seed_policy: seed_policy(false),
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: scheduling("perm:lab.procedure-execution-v1"),
        reattempt: reattempt(),
        budgets: budgets(pins),
        bundle_policy: BundlePolicy::Named {
            name: "lab/procedure-execution-v1".to_string(),
        },
        ext: BTreeMap::new(),
    };
    spec.experiment_id = spec.experiment_id();
    spec
}

// ── lab/retrieval-index-v1 (structural_index as a Lab arm) ───────────────────

/// The level pins `lab/retrieval-index-v1` binds — `structural_index` as a
/// registered component-level arm (§5c.3; the ranker/index factor).
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalIndexPins {
    /// `deterministic_default` ranker arm ref.
    pub deterministic_default_ref: String,
    /// `structural_pagerank` over `structural_index` arm ref.
    pub structural_pagerank_ref: String,
    /// The single model level ref.
    pub model_level_ref: String,
    /// The single environment level ref.
    pub environment_level_ref: String,
    /// The sealed artifacts the two arms bind.
    pub artifacts: (Ref, Ref),
}

/// `lab/retrieval-index-v1` — the `structural_index` Lab arm (§5c.3):
/// `ranker ∈ {deterministic_default, structural_pagerank}` paired on task
/// under `MatchSpec`, reporting retrieval quality and cost.
pub fn retrieval_index_v1(
    pins: &ExemplarPins,
    own: &RetrievalIndexPins,
    registered_at: u64,
) -> ExperimentSpec {
    let prereg = pre_registration(
        registered_at,
        "structural_pagerank over structural_index vs deterministic_default \
         on retrieval quality at matched budget",
        &[
            "grounding.reacquisition_rate",
            "capability.task_success",
            "efficiency.tokens_total",
        ],
        &[],
        pins,
    );
    let levels = vec![
        decl_level(
            "deterministic_default",
            &own.deterministic_default_ref,
            "deterministic default",
        ),
        decl_level(
            "structural_pagerank",
            &own.structural_pagerank_ref,
            "structural pagerank",
        ),
    ];
    let mut spec = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: "lab/retrieval-index-v1".to_string(),
            kind: DesignKind::Paired,
            factors: vec![
                FactorDeclaration {
                    name: "ranker".to_string(),
                    kind: FactorKind::Harness,
                    granularity: Some(Granularity::ComponentLevel),
                    levels: levels.clone(),
                    role: None,
                },
                FactorDeclaration {
                    name: "model_snapshot".to_string(),
                    kind: FactorKind::ModelSnapshot,
                    granularity: None,
                    levels: vec![decl_level(
                        "model:fixed",
                        &own.model_level_ref,
                        "fixed model",
                    )],
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
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: Some(prereg),
        factors: vec![
            FactorSpec {
                name: "ranker".to_string(),
                kind: FactorKind::Harness,
                granularity: Some(Granularity::ComponentLevel),
                role: Some("primary".to_string()),
                levels: levels
                    .iter()
                    .map(|l| level(&l.id, &l.content_ref, &l.label))
                    .collect(),
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
                "arm:deterministic_default",
                "deterministic_default baseline",
                &[
                    ("ranker", "deterministic_default"),
                    ("model_snapshot", "model:fixed"),
                    ("environment", "terminal-bench-2.0"),
                ],
                context_builder_match(),
                &own.artifacts.0,
                pins,
            ),
            arm(
                "arm:structural_pagerank",
                "structural_pagerank over structural_index",
                &[
                    ("ranker", "structural_pagerank"),
                    ("model_snapshot", "model:fixed"),
                    ("environment", "terminal-bench-2.0"),
                ],
                context_builder_match(),
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
        scheduling: scheduling("perm:lab.retrieval-index-v1"),
        reattempt: reattempt(),
        budgets: budgets(pins),
        bundle_policy: BundlePolicy::Named {
            name: "lab/retrieval-index-v1".to_string(),
        },
        ext: BTreeMap::new(),
    };
    spec.experiment_id = spec.experiment_id();
    spec
}

// ── lab/delegation-v1 (ADR-0186 D6; §6.3's Phase-4 recipe) ──────────────────

/// The level pins `lab/delegation-v1` binds, beyond [`ExemplarPins`].
/// `topology` is a component-level factor (`Harness` kind,
/// `component_level` granularity) — the `TopologyPreset` catalogue refs
/// the sealed definition's goal/procedure resolves against.
#[derive(Debug, Clone, PartialEq)]
pub struct DelegationPins {
    /// `topology` level refs — `(T0 solo, T1 fan_out 2, T1 fan_out 4,
    /// T2, T3 depth-2)` order (ADR-0186 D6's level set).
    pub topology_refs: (String, String, String, String, String),
    /// The two pinned model-family level refs.
    pub model_family_refs: (String, String),
    /// The coding-suite environment level ref (Stage-4 first
    /// execution).
    pub environment_level_ref: String,
    /// The sealed artifacts the five topology arms bind (same order as
    /// `topology_refs`).
    pub artifacts: (Ref, Ref, Ref, Ref, Ref),
    /// The pinned pricing table — `spend` is a matched dimension.
    pub pricing_table_ref: PricingTableRef,
    /// The pinned `inference_budget` ref — `matched_total` (M3) sums
    /// `search + eval + inference` per arm.
    pub inference_budget_ref: String,
}

/// `lab/delegation-v1`'s primary contrast — `matched_total` on
/// `{tokens.* constituents, spend}` (ADR-0186 D6; child spend is subject
/// spend — ADR-0041 M3). The time companion arm set (`matched_cap` on
/// `time.wall_ms`) is a second `MatchSpec` on the same arms — a
/// cross-mode `compare` is `IncommensurableMatch`.
fn delegation_match(pricing: &PricingTableRef) -> MatchSpec {
    use hh_ontology::dimensions::DimensionId::*;
    MatchSpec {
        dimensions: vec![
            TokensInputUncached,
            TokensInputCacheRead,
            TokensInputCacheWrite,
            TokensOutputVisible,
            TokensOutputReasoning,
            Spend,
        ],
        mode: MatchMode::MatchedTotal,
        tolerance_ppm: EXEMPLAR_TOLERANCE_PPM,
        pricing_table_ref: Some(pricing.clone()),
        model_scope: ModelScope::SameSnapshot,
        cache_policy: CachePolicy::ColdStart,
        utilization_floor_ppm: None,
    }
}

/// `lab/delegation-v1` at its Stage-4 first-execution size (ADR-0186
/// D6): `comparative`, `full_factorial` over `topology ∈ {T0, T1(2),
/// T1(4), T2, T3(2)}` (component-level) × two model families, one coding
/// suite held-out split, `replicates_per_cell = 5` (≥ 5 seeds),
/// `MatchSpec{matched_total, tolerance 0.10}` — the negative-literature
/// null hypothesis H0 (T0 vs every delegating topology at matched
/// total) with primaries `{capability.task_success,
/// coordination.lost_write_count, efficiency.cost_of_pass}`.
pub fn delegation_v1(
    pins: &ExemplarPins,
    own: &DelegationPins,
    registered_at: u64,
) -> ExperimentSpec {
    let prereg = pre_registration(
        registered_at,
        "H0 — no headline difference between T0 and any delegating topology at \
         matched total on the coding suite; H1 — T1 lowers time.wall_ms at equal \
         tokens where subtasks are independent",
        &[
            "capability.task_success",
            "coordination.lost_write_count",
            "efficiency.cost_of_pass",
        ],
        &["model_snapshot × topology"],
        pins,
    );
    let topology_levels: &[(&str, &str, &str)] = &[
        ("t0", &own.topology_refs.0, "T0 — solo"),
        ("t1_fan2", &own.topology_refs.1, "T1 — fan_out 2"),
        ("t1_fan4", &own.topology_refs.2, "T1 — fan_out 4"),
        ("t2", &own.topology_refs.3, "T2 — orchestrator/worker"),
        ("t3_depth2", &own.topology_refs.4, "T3 — recursive depth 2"),
    ];
    let artifact_for = |tid: &str| -> Ref {
        match tid {
            "t0" => own.artifacts.0.clone(),
            "t1_fan2" => own.artifacts.1.clone(),
            "t1_fan4" => own.artifacts.2.clone(),
            "t2" => own.artifacts.3.clone(),
            _ => own.artifacts.4.clone(),
        }
    };
    // `full_factorial` — the arms are the complete varied-factor
    // product: topology(5) × model_snapshot(2) (the single-level
    // environment factor rides every arm's assignment).
    let model_levels: &[(&str, &str)] = &[
        ("model:a", &own.model_family_refs.0),
        ("model:b", &own.model_family_refs.1),
    ];
    let mut arms: Vec<ArmSpec> = Vec::new();
    for (tid, _r, tlabel) in topology_levels {
        for (mid, _mr) in model_levels {
            arms.push(arm(
                &format!("arm:{tid}:{mid}"),
                &format!("delegation {tlabel} × {mid}"),
                &[
                    ("topology", tid),
                    ("model_snapshot", mid),
                    ("environment", "coding-suite"),
                ],
                delegation_match(&own.pricing_table_ref),
                &artifact_for(tid),
                pins,
            ));
            arms.last_mut().unwrap().inference_budget = Some(own.inference_budget_ref.clone());
        }
    }
    let mut spec = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: "lab/delegation-v1".to_string(),
            kind: DesignKind::FullFactorial,
            factors: vec![
                FactorDeclaration {
                    name: "topology".to_string(),
                    kind: FactorKind::Harness,
                    granularity: Some(Granularity::ComponentLevel),
                    levels: topology_levels
                        .iter()
                        .map(|(id, r, label)| decl_level(id, r, label))
                        .collect(),
                    role: None,
                },
                FactorDeclaration {
                    name: "model_snapshot".to_string(),
                    kind: FactorKind::ModelSnapshot,
                    granularity: None,
                    levels: vec![
                        decl_level("model:a", &own.model_family_refs.0, "family A"),
                        decl_level("model:b", &own.model_family_refs.1, "family B"),
                    ],
                    role: None,
                },
                FactorDeclaration {
                    name: "environment".to_string(),
                    kind: FactorKind::Environment,
                    granularity: None,
                    levels: vec![decl_level(
                        "coding-suite",
                        &own.environment_level_ref,
                        "Stage-3 coding suite",
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
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: Some(prereg),
        factors: vec![
            FactorSpec {
                name: "topology".to_string(),
                kind: FactorKind::Harness,
                granularity: Some(Granularity::ComponentLevel),
                role: Some("primary".to_string()),
                levels: topology_levels
                    .iter()
                    .map(|(id, r, label)| level(id, r, label))
                    .collect(),
            },
            FactorSpec {
                name: "model_snapshot".to_string(),
                kind: FactorKind::ModelSnapshot,
                granularity: None,
                role: Some("blocking".to_string()),
                levels: vec![
                    level("model:a", &own.model_family_refs.0, "family A"),
                    level("model:b", &own.model_family_refs.1, "family B"),
                ],
            },
            FactorSpec {
                name: "environment".to_string(),
                kind: FactorKind::Environment,
                granularity: None,
                role: Some("blocking".to_string()),
                levels: vec![level(
                    "coding-suite",
                    &own.environment_level_ref,
                    "Stage-3 coding suite",
                )],
            },
        ],
        arms,
        suite: SuiteBinding {
            suite_ref: pins.suite_ref.clone(),
            split_labels_used: vec![SplitLabel::HeldOut],
            split_assignment_ref: Some(pins.split_assignment_ref.clone()),
        },
        replicates_per_cell: EXEMPLAR_REPLICATES,
        seed_policy: seed_policy(false),
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: scheduling("perm:lab.delegation-v1"),
        reattempt: reattempt(),
        budgets: budgets(pins),
        bundle_policy: BundlePolicy::Named {
            name: "lab/delegation-v1".to_string(),
        },
        ext: BTreeMap::new(),
    };
    spec.experiment_id = spec.experiment_id();
    spec
}

// ── lab/value-of-compute-v1 (ADR-0190 D1–D6; §6.3's Phase-4 recipe) ─────────

/// The level pins `lab/value-of-compute-v1` binds, beyond
/// [`ExemplarPins`]. `compute_policy` is a component-level factor; the
/// `bandit`/`predictor` levels land at Stage 5 (the recipe's
/// `matched_total` rule — ADR-0041 M3 — makes a `search_budget > 0` arm
/// `IncommensurableMatch` under `matched_cap`, never silently admitted).
#[derive(Debug, Clone, PartialEq)]
pub struct ValueOfComputePins {
    /// `compute_policy` level refs — `(static, uniform, rules)` order.
    pub policy_refs: (String, String, String),
    /// The two pinned model-family level refs.
    pub model_family_refs: (String, String),
    /// The coding-suite environment level ref.
    pub environment_level_ref: String,
    /// The sealed artifacts the three policy arms bind (same order).
    pub artifacts: (Ref, Ref, Ref),
    /// The artifact the `iso_cost` arm binds.
    pub iso_artifact: Ref,
    /// The pinned pricing table — mandatory on the `iso_cost` arm and
    /// the matched-total dims' `spend` leg.
    pub pricing_table_ref: PricingTableRef,
    /// The second `eval_budget` ref — the recipe's `budget ∈ {tight,
    /// wide}` axis.
    pub eval_budget_wide: String,
}

/// `lab/value-of-compute-v1`'s primary contrast — `matched_cap` on
/// `{model_calls, tokens.output.visible, time.wall_ms, spend}`
/// (ADR-0190 D2) with `budget_utilization` reported.
fn value_of_compute_match(pricing: &PricingTableRef) -> MatchSpec {
    use hh_ontology::dimensions::DimensionId::*;
    MatchSpec {
        dimensions: vec![ModelCalls, TokensOutputVisible, TimeWallMs, Spend],
        mode: MatchMode::MatchedCap,
        tolerance_ppm: EXEMPLAR_TOLERANCE_PPM,
        pricing_table_ref: Some(pricing.clone()),
        model_scope: ModelScope::SameSnapshot,
        cache_policy: CachePolicy::ColdStart,
        utilization_floor_ppm: None,
    }
}

/// `lab/value-of-compute-v1` at its Stage-4 first-execution size
/// (ADR-0190 D1–D6): `comparative`, `full_factorial` over
/// `compute_policy ∈ {static, uniform, rules}` (component-level) × two
/// model families × `eval_budget ∈ {tight, wide}` on the coding suite
/// held-out split, `replicates_per_cell = 5`, primaries
/// `{capability.task_success, efficiency.cost_of_pass,
/// scheduling.overhead}` (Holm-controlled), plus one `iso_cost` arm for
/// the `cost_of_pass` frontier (ADR-0159). H0 — `rules` vs `static` no
/// headline difference at matched cap; H2 — the budget × policy
/// contrast is the registered interaction.
pub fn value_of_compute_v1(
    pins: &ExemplarPins,
    own: &ValueOfComputePins,
    registered_at: u64,
) -> ExperimentSpec {
    let prereg = pre_registration(
        registered_at,
        "H0 — no headline difference rules vs static at matched cap; \
         H1 — rules spends less than uniform at equal task_success; \
         H2 — larger rules effect at the tight budget (budget × policy contrast)",
        &[
            "capability.task_success",
            "efficiency.cost_of_pass",
            "scheduling.overhead",
        ],
        &["eval_budget × compute_policy"],
        pins,
    );
    let policy_levels: &[(&str, &str, &str)] = &[
        ("static", &own.policy_refs.0, "static"),
        ("uniform", &own.policy_refs.1, "uniform"),
        ("rules", &own.policy_refs.2, "rules"),
    ];
    let artifact_for = |pid: &str| -> Ref {
        match pid {
            "static" => own.artifacts.0.clone(),
            "uniform" => own.artifacts.1.clone(),
            _ => own.artifacts.2.clone(),
        }
    };
    // `full_factorial` — the matched_cap arms are the complete product:
    // compute_policy(3) × model_snapshot(2) × eval_budget(2).
    let model_levels: &[(&str, &str)] = &[
        ("model:a", &own.model_family_refs.0),
        ("model:b", &own.model_family_refs.1),
    ];
    let mut arms: Vec<ArmSpec> = Vec::new();
    for (pid, _r, label) in policy_levels {
        for (mid, _mr) in model_levels {
            for (bid, budget_ref) in [
                ("tight", &pins.eval_budget),
                ("wide", &own.eval_budget_wide),
            ] {
                let mut a = arm(
                    &format!("arm:{pid}:{mid}:{bid}"),
                    &format!("{label} × {mid} at {bid} budget"),
                    &[
                        ("compute_policy", pid),
                        ("model_snapshot", mid),
                        ("eval_budget", bid),
                        ("environment", "coding-suite"),
                    ],
                    value_of_compute_match(&own.pricing_table_ref),
                    &artifact_for(pid),
                    pins,
                );
                a.eval_budget = budget_ref.clone();
                arms.push(a);
            }
        }
    }
    // The `iso_cost` companion arm — the frontier read (ADR-0159); it
    // shares `rules × model:a × tight`'s design point under its own
    // comparand group (ADR-0156 D2 — a different-mode arm at an occupied
    // point; a direct compare across modes is `IncommensurableMatch`).
    let mut iso = arm(
        "arm:rules:model:a:tight:iso",
        "rules at iso_cost — the frontier read",
        &[
            ("compute_policy", "rules"),
            ("model_snapshot", "model:a"),
            ("eval_budget", "tight"),
            ("environment", "coding-suite"),
        ],
        MatchSpec {
            dimensions: vec![hh_ontology::dimensions::DimensionId::Spend],
            mode: MatchMode::IsoCost,
            tolerance_ppm: EXEMPLAR_TOLERANCE_PPM,
            pricing_table_ref: Some(own.pricing_table_ref.clone()),
            model_scope: ModelScope::SameSnapshot,
            cache_policy: CachePolicy::ColdStart,
            utilization_floor_ppm: None,
        },
        &own.iso_artifact,
        pins,
    );
    iso.eval_budget = pins.eval_budget.clone();
    arms.push(iso);

    let mut spec = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: "lab/value-of-compute-v1".to_string(),
            kind: DesignKind::FullFactorial,
            factors: vec![
                FactorDeclaration {
                    name: "compute_policy".to_string(),
                    kind: FactorKind::Harness,
                    granularity: Some(Granularity::ComponentLevel),
                    levels: policy_levels
                        .iter()
                        .map(|(id, r, label)| decl_level(id, r, label))
                        .collect(),
                    role: None,
                },
                FactorDeclaration {
                    name: "model_snapshot".to_string(),
                    kind: FactorKind::ModelSnapshot,
                    granularity: None,
                    levels: vec![
                        decl_level("model:a", &own.model_family_refs.0, "family A"),
                        decl_level("model:b", &own.model_family_refs.1, "family B"),
                    ],
                    role: None,
                },
                FactorDeclaration {
                    name: "environment".to_string(),
                    kind: FactorKind::Environment,
                    granularity: None,
                    levels: vec![decl_level(
                        "coding-suite",
                        &own.environment_level_ref,
                        "Stage-3 coding suite",
                    )],
                    role: None,
                },
                FactorDeclaration {
                    name: "eval_budget".to_string(),
                    kind: FactorKind::Budget,
                    granularity: None,
                    levels: vec![
                        decl_level("tight", &pins.eval_budget, "tight budget"),
                        decl_level("wide", &own.eval_budget_wide, "wide budget"),
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
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: Some(prereg),
        factors: vec![
            FactorSpec {
                name: "compute_policy".to_string(),
                kind: FactorKind::Harness,
                granularity: Some(Granularity::ComponentLevel),
                role: Some("primary".to_string()),
                levels: policy_levels
                    .iter()
                    .map(|(id, r, label)| level(id, r, label))
                    .collect(),
            },
            FactorSpec {
                name: "model_snapshot".to_string(),
                kind: FactorKind::ModelSnapshot,
                granularity: None,
                role: Some("blocking".to_string()),
                levels: vec![
                    level("model:a", &own.model_family_refs.0, "family A"),
                    level("model:b", &own.model_family_refs.1, "family B"),
                ],
            },
            FactorSpec {
                name: "environment".to_string(),
                kind: FactorKind::Environment,
                granularity: None,
                role: Some("blocking".to_string()),
                levels: vec![level(
                    "coding-suite",
                    &own.environment_level_ref,
                    "Stage-3 coding suite",
                )],
            },
            FactorSpec {
                name: "eval_budget".to_string(),
                kind: FactorKind::Budget,
                granularity: None,
                role: Some("contrast".to_string()),
                levels: vec![
                    level("tight", &pins.eval_budget, "tight budget"),
                    level("wide", &own.eval_budget_wide, "wide budget"),
                ],
            },
        ],
        arms,
        suite: SuiteBinding {
            suite_ref: pins.suite_ref.clone(),
            split_labels_used: vec![SplitLabel::HeldOut],
            split_assignment_ref: Some(pins.split_assignment_ref.clone()),
        },
        replicates_per_cell: EXEMPLAR_REPLICATES,
        seed_policy: seed_policy(false),
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: scheduling("perm:lab.value-of-compute-v1"),
        reattempt: reattempt(),
        budgets: budgets(pins),
        bundle_policy: BundlePolicy::Named {
            name: "lab/value-of-compute-v1".to_string(),
        },
        ext: BTreeMap::new(),
    };
    spec.experiment_id = spec.experiment_id();
    spec
}

// ── lab/coordination-topology-v1 (ADR-0193 D6; §6.3's Phase-4 recipe) ───────

/// The level pins `lab/coordination-topology-v1` binds, beyond
/// [`ExemplarPins`]. All three factors are component-level: `topology`,
/// `merge_policy` and `default_isolation` are C3 contract axes whose
/// value is a matched-budget Lab question (AC-R-2.6.5-7).
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinationTopologyPins {
    /// `topology` level refs — `(single_agent,
    /// orchestrator_worker_isolated, orchestrator_worker_share,
    /// peer_messaging)` order.
    pub topology_refs: (String, String, String, String),
    /// `merge_policy` level refs — `(parent_effects, three_way_text)`
    /// order.
    pub merge_policy_refs: (String, String),
    /// `default_isolation` level refs — `(fork_snapshot, scoped_subtree)`
    /// order.
    pub isolation_refs: (String, String),
    /// The pinned model-family level ref.
    pub model_family_ref: String,
    /// The coding-suite environment level ref.
    pub environment_level_ref: String,
    /// The sealed artifacts the topology arms bind (same order as
    /// `topology_refs`; merge/isolation stay definition-level pins on
    /// the artifact).
    pub artifacts: (Ref, Ref, Ref, Ref),
    /// The pinned pricing table — `spend` is a matched dimension.
    pub pricing_table_ref: PricingTableRef,
    /// The pinned `inference_budget` ref — `matched_total` (M3) sums
    /// `search + eval + inference` per arm.
    pub inference_budget_ref: String,
}

/// `lab/coordination-topology-v1`'s `MatchSpec{matched_total}` — child
/// spend is `charged_to = subject` (AC-R-2.6.5-7; ADR-0041 M3).
fn coordination_match(pricing: &PricingTableRef) -> MatchSpec {
    use hh_ontology::dimensions::DimensionId::*;
    MatchSpec {
        dimensions: vec![
            TokensInputUncached,
            TokensInputCacheRead,
            TokensInputCacheWrite,
            TokensOutputVisible,
            TokensOutputReasoning,
            Spend,
            Spawns,
        ],
        mode: MatchMode::MatchedTotal,
        tolerance_ppm: EXEMPLAR_TOLERANCE_PPM,
        pricing_table_ref: Some(pricing.clone()),
        model_scope: ModelScope::SameSnapshot,
        cache_policy: CachePolicy::ColdStart,
        utilization_floor_ppm: None,
    }
}

/// `lab/coordination-topology-v1` at its Stage-4 first-execution size
/// (ADR-0193 D6; AC-R-2.6.5-7): `comparative`, `full_factorial` over
/// `topology ∈ {single_agent, orchestrator_worker_isolated,
/// orchestrator_worker_share, peer_messaging}` × `merge_policy` ×
/// `default_isolation` (all component-level) on the coding suite,
/// `MatchSpec{matched_total}`, `replicates_per_cell = 5`, reporting
/// `capability.task_success`, the two veto metrics
/// (`coordination.lost_write_count`, `coordination
/// .silent_overwrite_count`), `conflicts_open_at_completion` and cost —
/// the model × topology interaction as a `contrast`.
pub fn coordination_topology_v1(
    pins: &ExemplarPins,
    own: &CoordinationTopologyPins,
    registered_at: u64,
) -> ExperimentSpec {
    let prereg = pre_registration(
        registered_at,
        "H0 — no headline difference between single_agent and any coordination \
         topology at matched total; the merge/isolation levels are evidence for \
         the single_writer/three_way_text defaults (OQ-427), never defaults on \
         provisional evidence",
        &[
            "capability.task_success",
            "coordination.lost_write_count",
            "coordination.silent_overwrite_count",
            "efficiency.cost_of_pass",
        ],
        &["model_snapshot × topology"],
        pins,
    );
    let topology_levels: &[(&str, &str, &str)] = &[
        ("single_agent", &own.topology_refs.0, "single agent"),
        (
            "orchestrator_worker_isolated",
            &own.topology_refs.1,
            "orchestrator/worker — isolated children",
        ),
        (
            "orchestrator_worker_share",
            &own.topology_refs.2,
            "orchestrator/worker — shared subtree",
        ),
        ("peer_messaging", &own.topology_refs.3, "peer messaging"),
    ];
    let merge_levels: &[(&str, &str, &str)] = &[
        ("parent_effects", &own.merge_policy_refs.0, "parent effects"),
        ("three_way_text", &own.merge_policy_refs.1, "three_way_text"),
    ];
    let isolation_levels: &[(&str, &str, &str)] = &[
        ("fork_snapshot", &own.isolation_refs.0, "fork_snapshot"),
        ("scoped_subtree", &own.isolation_refs.1, "scoped_subtree"),
    ];
    let artifact_for = |tid: &str| -> Ref {
        match tid {
            "single_agent" => own.artifacts.0.clone(),
            "orchestrator_worker_isolated" => own.artifacts.1.clone(),
            "orchestrator_worker_share" => own.artifacts.2.clone(),
            _ => own.artifacts.3.clone(),
        }
    };
    let mut arms: Vec<ArmSpec> = Vec::new();
    for (tid, _r, tlabel) in topology_levels {
        for (mid, _r, _mlabel) in merge_levels {
            for (iid, _r, _ilabel) in isolation_levels {
                arms.push(arm(
                    &format!("arm:{tid}:{mid}:{iid}"),
                    &format!("{tlabel} / {mid} / {iid}"),
                    &[
                        ("topology", tid),
                        ("merge_policy", mid),
                        ("default_isolation", iid),
                        ("model_snapshot", "model:a"),
                        ("environment", "coding-suite"),
                    ],
                    coordination_match(&own.pricing_table_ref),
                    &artifact_for(tid),
                    pins,
                ));
                arms.last_mut().unwrap().inference_budget = Some(own.inference_budget_ref.clone());
            }
        }
    }
    let mut spec = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: "lab/coordination-topology-v1".to_string(),
            kind: DesignKind::FullFactorial,
            factors: vec![
                FactorDeclaration {
                    name: "topology".to_string(),
                    kind: FactorKind::Harness,
                    granularity: Some(Granularity::ComponentLevel),
                    levels: topology_levels
                        .iter()
                        .map(|(id, r, label)| decl_level(id, r, label))
                        .collect(),
                    role: None,
                },
                FactorDeclaration {
                    name: "merge_policy".to_string(),
                    kind: FactorKind::Harness,
                    granularity: Some(Granularity::ComponentLevel),
                    levels: merge_levels
                        .iter()
                        .map(|(id, r, label)| decl_level(id, r, label))
                        .collect(),
                    role: None,
                },
                FactorDeclaration {
                    name: "default_isolation".to_string(),
                    kind: FactorKind::Harness,
                    granularity: Some(Granularity::ComponentLevel),
                    levels: isolation_levels
                        .iter()
                        .map(|(id, r, label)| decl_level(id, r, label))
                        .collect(),
                    role: None,
                },
                FactorDeclaration {
                    name: "model_snapshot".to_string(),
                    kind: FactorKind::ModelSnapshot,
                    granularity: None,
                    levels: vec![decl_level("model:a", &own.model_family_ref, "family A")],
                    role: None,
                },
                FactorDeclaration {
                    name: "environment".to_string(),
                    kind: FactorKind::Environment,
                    granularity: None,
                    levels: vec![decl_level(
                        "coding-suite",
                        &own.environment_level_ref,
                        "Stage-3 coding suite",
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
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: Some(prereg),
        factors: vec![
            FactorSpec {
                name: "topology".to_string(),
                kind: FactorKind::Harness,
                granularity: Some(Granularity::ComponentLevel),
                role: Some("primary".to_string()),
                levels: topology_levels
                    .iter()
                    .map(|(id, r, label)| level(id, r, label))
                    .collect(),
            },
            FactorSpec {
                name: "merge_policy".to_string(),
                kind: FactorKind::Harness,
                granularity: Some(Granularity::ComponentLevel),
                role: Some("primary".to_string()),
                levels: merge_levels
                    .iter()
                    .map(|(id, r, label)| level(id, r, label))
                    .collect(),
            },
            FactorSpec {
                name: "default_isolation".to_string(),
                kind: FactorKind::Harness,
                granularity: Some(Granularity::ComponentLevel),
                role: Some("primary".to_string()),
                levels: isolation_levels
                    .iter()
                    .map(|(id, r, label)| level(id, r, label))
                    .collect(),
            },
            FactorSpec {
                name: "model_snapshot".to_string(),
                kind: FactorKind::ModelSnapshot,
                granularity: None,
                role: Some("blocking".to_string()),
                levels: vec![level("model:a", &own.model_family_ref, "family A")],
            },
            FactorSpec {
                name: "environment".to_string(),
                kind: FactorKind::Environment,
                granularity: None,
                role: Some("blocking".to_string()),
                levels: vec![level(
                    "coding-suite",
                    &own.environment_level_ref,
                    "Stage-3 coding suite",
                )],
            },
        ],
        arms,
        suite: SuiteBinding {
            suite_ref: pins.suite_ref.clone(),
            split_labels_used: vec![SplitLabel::HeldOut],
            split_assignment_ref: Some(pins.split_assignment_ref.clone()),
        },
        replicates_per_cell: EXEMPLAR_REPLICATES,
        seed_policy: seed_policy(false),
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: scheduling("perm:lab.coordination-topology-v1"),
        reattempt: reattempt(),
        budgets: budgets(pins),
        bundle_policy: BundlePolicy::Named {
            name: "lab/coordination-topology-v1".to_string(),
        },
        ext: BTreeMap::new(),
    };
    spec.experiment_id = spec.experiment_id();
    spec
}
