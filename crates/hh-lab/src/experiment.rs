//! The experiment dialect `hh-experiment/1` (spec §6.3; R-2.10.3⁰ᵃ + the
//! R-2.10.5⁰ row-key fields; S1.24; ADR-0154…0156).
//!
//! Records: [`ExperimentSpec`] (with [`FactorSpec`], [`LevelSpec`],
//! [`ArmSpec`], [`SuiteBinding`], [`SchedulingPolicy`], [`ReattemptPolicy`],
//! [`ValidationStrategy`], [`ExperimentBudgets`], [`BundlePolicy`]),
//! [`CellPlan`]/[`PlanCell`]/[`RunPlan`]/[`OrderPlan`], and the pure
//! [`run_plan_id`] derivation.
//!
//! [`ExperimentSpec::register`] is the schema-checkable subset of the §6.3
//! `register` refusal set, over a [`SpecContext`] view: member-level checks
//! always run; context-parameterized checks (budget resolution, artifact
//! sealing, split assignment, capability drift, the retirement diff) run
//! only where the context supplies the resolver — the engine's E-1 gate at
//! Stage 3 supplies all of them.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_budget::{
    validate_match, ArmSpec as BudgetArmSpec, BudgetEnforcement, BudgetSpec, CachePolicy,
    MatchError, MatchMode, MatchRefusal, MatchSpec,
};
use hh_identity::idp::{identify_bytes, idp_id};
use hh_identity::kinds::RecordKind;
use hh_ontology::config::Ref;
use hh_ontology::dimensions::DimensionId;
use hh_ontology::eval::{Design, DesignKind, PreRegistration, SeedPolicy};
use hh_ontology::lab::SplitLabel;
use hh_ontology::participant::{Granularity, ParticipantClass};
use hh_ontology::FactorKind;
use hh_wire::Json;

use crate::json_util::*;

// ── ExperimentKind ──────────────────────────────────────────────────────────

/// `kind ∈ {comparative, exploratory, equivalence, retirement, reproduction}`
/// — the experiment kinds are constraint tables over one engine, not separate
/// engines (ADR-0156).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExperimentKind {
    /// A paired-Δ comparison.
    Comparative,
    /// No pre-registration / match spec; every row `comparable = false`.
    Exploratory,
    /// `equivalence_run` under a declared margin (T-LCD-03/-04).
    Equivalence,
    /// A single-rule removal diff (T-LCD-05).
    Retirement,
    /// A reproduction of a published bundle (ADR-0140 R2/R3).
    Reproduction,
}

impl ExperimentKind {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            ExperimentKind::Comparative => "comparative",
            ExperimentKind::Exploratory => "exploratory",
            ExperimentKind::Equivalence => "equivalence",
            ExperimentKind::Retirement => "retirement",
            ExperimentKind::Reproduction => "reproduction",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<ExperimentKind> {
        match s {
            "comparative" => Some(ExperimentKind::Comparative),
            "exploratory" => Some(ExperimentKind::Exploratory),
            "equivalence" => Some(ExperimentKind::Equivalence),
            "retirement" => Some(ExperimentKind::Retirement),
            "reproduction" => Some(ExperimentKind::Reproduction),
            _ => None,
        }
    }

    /// Whether the kind requires `pre_registration` (all but `exploratory`).
    pub fn requires_pre_registration(self) -> bool {
        self != ExperimentKind::Exploratory
    }

    /// Whether the kind requires matched budgets / `match_spec` on every arm
    /// (all but `exploratory`; `equivalence` additionally refuses unequal
    /// budgets — enforced through `validate_match`).
    pub fn requires_match(self) -> bool {
        self != ExperimentKind::Exploratory
    }
}

// ── FactorSpec / LevelSpec ──────────────────────────────────────────────────

/// `LevelSpec{level_id, ref, overrides?, label, class ∈ {native, hosted},
/// non_portable?}` — one level of an experimental factor (§6.3).
#[derive(Debug, Clone, PartialEq)]
pub struct LevelSpec {
    /// The level id (unique within the factor).
    pub level_id: String,
    /// The level's pinned ref (a snapshot/definition/configuration id).
    pub ref_: String,
    /// Level-local overrides (schema-opaque at C0).
    pub overrides: Option<Json>,
    /// A display label.
    pub label: String,
    /// The participant class of this level.
    pub class: ParticipantClass,
    /// `non_portable = true` marks a pinned-profile level (OQ-078/J3: a
    /// definition whose `profile_binding` pins one profile is admissible only
    /// with `non_portable = true`, and only in arms where the profile is not
    /// the varied factor — ADR-0154 D5).
    pub non_portable: bool,
}

impl LevelSpec {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("level_id".into(), Json::str(&self.level_id));
        m.insert("ref".into(), Json::str(&self.ref_));
        if let Some(o) = &self.overrides {
            m.insert("overrides".into(), o.clone());
        }
        m.insert("label".into(), Json::str(&self.label));
        m.insert("class".into(), Json::str(self.class.as_str()));
        if self.non_portable {
            m.insert("non_portable".into(), Json::Bool(true));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<LevelSpec, SchemaError> {
        const REC: &str = "LevelSpec";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "level_id",
                "ref",
                "overrides",
                "label",
                "class",
                "non_portable",
            ],
            REC,
        )?;
        Ok(LevelSpec {
            level_id: str_at(m, "level_id", REC)?.to_string(),
            ref_: str_at(m, "ref", REC)?.to_string(),
            overrides: m
                .get("overrides")
                .filter(|j| !matches!(j, Json::Null))
                .cloned(),
            label: str_at(m, "label", REC)?.to_string(),
            class: ParticipantClass::parse(str_at(m, "class", REC)?)
                .ok_or_else(|| SchemaError::v("class", "unknown participant class"))?,
            non_portable: opt_bool_at(m, "non_portable")?.unwrap_or(false),
        })
    }
}

/// `FactorSpec{kind ∈ {model_snapshot, harness, environment, task, budget,
/// replicate}, granularity?, role?, levels[]}` — a declared experimental
/// factor (§6.3; the kind sum is `hh_ontology::FactorKind`).
#[derive(Debug, Clone, PartialEq)]
pub struct FactorSpec {
    /// The factor name (unique within the spec).
    pub name: String,
    /// The factor kind.
    pub kind: FactorKind,
    /// The granularity at which the factor varies (`harness` factors carry
    /// one — §2.7.4).
    pub granularity: Option<Granularity>,
    /// The factor's role (`primary`/`blocking`/… — free-form at C0, closed at
    /// dialect bump).
    pub role: Option<String>,
    /// The declared levels (≥ 2 for a varied factor; ≥ 1 total).
    pub levels: Vec<LevelSpec>,
}

impl FactorSpec {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("name".into(), Json::str(&self.name));
        m.insert("kind".into(), Json::str(self.kind.as_str()));
        if let Some(g) = &self.granularity {
            m.insert("granularity".into(), Json::str(g.as_str()));
        }
        if let Some(r) = &self.role {
            m.insert("role".into(), Json::str(r));
        }
        m.insert(
            "levels".into(),
            Json::Arr(self.levels.iter().map(LevelSpec::to_json).collect()),
        );
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<FactorSpec, SchemaError> {
        const REC: &str = "FactorSpec";
        let m = expect_obj(j, REC)?;
        reject_unknown(m, &["name", "kind", "granularity", "role", "levels"], REC)?;
        Ok(FactorSpec {
            name: str_at(m, "name", REC)?.to_string(),
            kind: FactorKind::parse(str_at(m, "kind", REC)?)
                .ok_or_else(|| SchemaError::v("kind", "unknown factor kind"))?,
            granularity: opt_str_at(m, "granularity")?
                .map(|s| {
                    Granularity::parse(s)
                        .ok_or_else(|| SchemaError::v("granularity", "unknown granularity"))
                })
                .transpose()?,
            role: opt_str_at(m, "role")?.map(str::to_string),
            levels: enum_vec_at(m, "levels", REC, |j| LevelSpec::from_json(j).ok())?,
        })
    }
}

// ── ArmSpec ─────────────────────────────────────────────────────────────────

/// `ArmSpec{arm_id, hypothesis, level_assignment, eval_budget, search_budget,
/// match_spec, artifact_ref, limits_enforced, model_role_table_ref}` (§6.3).
///
/// `eval_budget`/`search_budget` are the pinned budget vector *refs* (the
/// §8.2-owned bodies live on the budget plane); `match_spec` is the
/// `hh-budget` `MatchSpec` (with `cache_policy`); `limits_enforced` is the
/// derived enforced-limit stamp (`full | partial | none` — data at C0).
/// `response_cache` — the arm's response-cache declaration (ADR-0129 d.2).
/// A closed sum: `k5` is the exact-match response cache (the only kind the
/// dialect admits at C0; K6's semantic cache is C2 and never lands here).
/// Absent member = no response cache enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseCacheDecl {
    /// `k5` — the exact-match response cache (ADR-0129 d.2).
    K5,
}

impl ResponseCacheDecl {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ResponseCacheDecl::K5 => "k5",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArmSpec {
    /// The arm id.
    pub arm_id: String,
    /// The arm's hypothesis.
    pub hypothesis: String,
    /// `level_assignment: map<factor_name, level_id>` — the arm's point in the
    /// factor space.
    pub level_assignment: BTreeMap<String, String>,
    /// The pinned `eval_budget` ref.
    pub eval_budget: String,
    /// The pinned `search_budget` ref (`None` = no search budget — legal only
    /// for `exploratory`/product-level rows; the refusal is `UnbudgetedArm`).
    pub search_budget: Option<String>,
    /// The pinned `inference_budget` ref — `matched_total` (M3) requires all
    /// three terms (`search + eval + inference`); absent on other modes
    /// (`UnbudgetedArm` under `matched_total`, additive at the wire).
    pub inference_budget: Option<String>,
    /// The arm's `MatchSpec` (mandatory on matched kinds — `MissingMatchSpec`).
    pub match_spec: Option<MatchSpec>,
    /// The arm's frozen artifact — a sealed definition or product version.
    pub artifact_ref: Ref,
    /// `limits_enforced` — the derived enforced-limit stamp
    /// (`full | partial | none` at C0; hosted arms whose token/spend limits
    /// are reported carry `partial`, ADR-0046 (d)).
    pub limits_enforced: String,
    /// The model role table the arm binds, where any.
    pub model_role_table_ref: Option<String>,
    /// `response_cache` — `k5` when the arm enables the K5 response cache
    /// (ADR-0129 d.2). Absent = no response cache. A K5-enabled arm on a
    /// non-`reproduction` spec requires `cache` declared as a factor
    /// (`PreRegistrationInvalid`; AC-R-2.3.4-11).
    pub response_cache: Option<ResponseCacheDecl>,
}

impl ArmSpec {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("arm_id".into(), Json::str(&self.arm_id));
        m.insert("hypothesis".into(), Json::str(&self.hypothesis));
        m.insert(
            "level_assignment".into(),
            Json::Obj(
                self.level_assignment
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v)))
                    .collect(),
            ),
        );
        m.insert("eval_budget".into(), Json::str(&self.eval_budget));
        if let Some(s) = &self.search_budget {
            m.insert("search_budget".into(), Json::str(s));
        }
        if let Some(s) = &self.inference_budget {
            m.insert("inference_budget".into(), Json::str(s));
        }
        if let Some(ms) = &self.match_spec {
            m.insert("match_spec".into(), ms.to_json());
        }
        m.insert("artifact_ref".into(), self.artifact_ref.to_json());
        m.insert("limits_enforced".into(), Json::str(&self.limits_enforced));
        if let Some(r) = &self.model_role_table_ref {
            m.insert("model_role_table_ref".into(), Json::str(r));
        }
        if let Some(rc) = self.response_cache {
            m.insert("response_cache".into(), Json::str(rc.as_str()));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ArmSpec, SchemaError> {
        const REC: &str = "ArmSpec";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "arm_id",
                "hypothesis",
                "level_assignment",
                "eval_budget",
                "search_budget",
                "inference_budget",
                "match_spec",
                "artifact_ref",
                "limits_enforced",
                "model_role_table_ref",
                "response_cache",
            ],
            REC,
        )?;
        let mut level_assignment = BTreeMap::new();
        match member_at(m, "level_assignment", REC)? {
            Json::Obj(lm) => {
                for (k, v) in lm {
                    let lv = v.as_str().ok_or_else(|| {
                        SchemaError::v("level_assignment", "level ids must be strings")
                    })?;
                    level_assignment.insert(k.clone(), lv.to_string());
                }
            }
            _ => return Err(SchemaError::v("level_assignment", "must be an object")),
        }
        let artifact = expect_obj(member_at(m, "artifact_ref", REC)?, "Ref")?;
        Ok(ArmSpec {
            arm_id: str_at(m, "arm_id", REC)?.to_string(),
            hypothesis: str_at(m, "hypothesis", REC)?.to_string(),
            level_assignment,
            eval_budget: str_at(m, "eval_budget", REC)?.to_string(),
            search_budget: opt_str_at(m, "search_budget")?.map(str::to_string),
            inference_budget: opt_str_at(m, "inference_budget")?.map(str::to_string),
            match_spec: match m.get("match_spec") {
                None | Some(Json::Null) => None,
                Some(ms) => Some(
                    MatchSpec::from_json(ms)
                        .ok_or_else(|| SchemaError::v("match_spec", "invalid MatchSpec"))?,
                ),
            },
            artifact_ref: Ref::new(
                str_at(artifact, "semantic_id", "Ref")?,
                str_at(artifact, "version_id", "Ref")?,
            ),
            limits_enforced: str_at(m, "limits_enforced", REC)?.to_string(),
            model_role_table_ref: opt_str_at(m, "model_role_table_ref")?.map(str::to_string),
            response_cache: match opt_str_at(m, "response_cache")? {
                None => None,
                Some("k5") => Some(ResponseCacheDecl::K5),
                Some(other) => {
                    return Err(SchemaError::v(
                        "response_cache",
                        format!("unknown response-cache kind `{other}`"),
                    ))
                }
            },
        })
    }
}

// ── SuiteBinding / scheduling / reattempt / validation / budgets / bundle ───

/// `SuiteBinding{suite_ref, split_labels_used, split_assignment_ref?}` (§6.3).
#[derive(Debug, Clone, PartialEq)]
pub struct SuiteBinding {
    /// The pinned suite manifest ref.
    pub suite_ref: String,
    /// The split labels the experiment runs over.
    pub split_labels_used: Vec<SplitLabel>,
    /// The pinned `SplitAssignmentRecord` (mandatory for matched kinds and
    /// for any `adaptive_search` arm — `SplitUnassigned`/`LeakedSplit` check it).
    pub split_assignment_ref: Option<String>,
}

/// `Pool{key ∈ {model_snapshot(ref), environment_class, instrument,
/// participant(ref)}, limit}` — a scheduling pool (ADR-0155 D3).
#[derive(Debug, Clone, PartialEq)]
pub struct Pool {
    /// The pool key (`model_snapshot:<ref>` / `environment_class:<name>` /
    /// `instrument` / `participant:<ref>`).
    pub key: String,
    /// The pool's concurrent-run limit (≤ `max_concurrent_runs`).
    pub limit: u32,
}

/// `OrderKind` — the run-order policy (`interleaved_blocked` is the engine
/// default; ADR-0154 D7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OrderKind {
    /// Runs execute serially.
    Serial,
    /// Runs interleave freely.
    Interleaved,
    /// Interleaved within replicate blocks (the default).
    InterleavedBlocked,
    /// A seeded permutation.
    RandomPermuted,
}

impl OrderKind {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            OrderKind::Serial => "serial",
            OrderKind::Interleaved => "interleaved",
            OrderKind::InterleavedBlocked => "interleaved_blocked",
            OrderKind::RandomPermuted => "random_permuted",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<OrderKind> {
        match s {
            "serial" => Some(OrderKind::Serial),
            "interleaved" => Some(OrderKind::Interleaved),
            "interleaved_blocked" => Some(OrderKind::InterleavedBlocked),
            "random_permuted" => Some(OrderKind::RandomPermuted),
            _ => None,
        }
    }
}

/// `SchedulingPolicy{max_concurrent_runs, pools[], order, permutation_seed,
/// start_stagger_ms, deadline?, priority?}` (§6.3 §2.2; ADR-0155 D3).
#[derive(Debug, Clone, PartialEq)]
pub struct SchedulingPolicy {
    /// The experiment's concurrent-run cap.
    pub max_concurrent_runs: u32,
    /// The scheduling pools.
    pub pools: Vec<Pool>,
    /// The order policy.
    pub order: OrderKind,
    /// The permutation seed (`S-6`: the order must be reproducible from it).
    pub permutation_seed: String,
    /// The stagger between run launches (ms).
    pub start_stagger_ms: u64,
    /// The experiment deadline (a `seq`, never a wall clock).
    pub deadline: Option<u64>,
    /// The scheduling priority (data at C0).
    pub priority: Option<String>,
}

/// `backoff{min_ms, multiplier, max_ms}` — the reattempt backoff schedule.
#[derive(Debug, Clone, PartialEq)]
pub struct Backoff {
    /// The minimum backoff (ms).
    pub min_ms: u64,
    /// The multiplier (ppm of 1.0 — `1_000_000` = ×1).
    pub multiplier_ppm: u64,
    /// The maximum backoff (ms).
    pub max_ms: u64,
}

/// `on_cancel ∈ {replan, final}` — what a cancellation does to a run plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CancelPolicy {
    /// Re-plan the run (operator/hosting cancellations).
    Replan,
    /// The plan is final (principal/parent cancellations).
    Final,
}

impl CancelPolicy {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            CancelPolicy::Replan => "replan",
            CancelPolicy::Final => "final",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<CancelPolicy> {
        match s {
            "replan" => Some(CancelPolicy::Replan),
            "final" => Some(CancelPolicy::Final),
            _ => None,
        }
    }
}

/// `ReattemptPolicy{max_per_plan, max_fraction_of_plans, backoff{…},
/// error_classes_included?, on_cancel}` (§6.3 §2.3; ADR-0155 D5;
/// OQ-360 defaults `max_per_plan = 2`, `max_fraction_of_plans = 0.10`).
#[derive(Debug, Clone, PartialEq)]
pub struct ReattemptPolicy {
    /// The maximum re-attempts per run plan.
    pub max_per_plan: u32,
    /// The maximum fraction of plans that may be re-attempted (ppm of 1.0;
    /// exceeding it ⇒ `paused{infrastructure_suspected}`).
    pub max_fraction_of_plans_ppm: u64,
    /// The backoff schedule.
    pub backoff: Backoff,
    /// The `error_class` values re-attempts apply to (`None` = all).
    pub error_classes_included: Option<Vec<String>>,
    /// The cancellation policy.
    pub on_cancel: CancelPolicy,
}

/// `ValidationStrategy ∈ {full_set, disagreement_weighted{lambda,
/// min_inclusion_fraction, estimator ∈ {hajek, anchored_difference}},
/// successive_halving{eta, min_budget, brackets}, voi_weighted{estimator ∈
/// {disagreement, expected_information_gain}, lambda, min_inclusion_fraction,
/// min_replicates}}` (§6.3 §2.5; ADR-0156/0190).
///
/// Any strategy other than `full_set` is admissible **only** when
/// `design.kind = adaptive_search`, on arms with `search_budget > 0`, over
/// `split_labels ⊆ {search, dev}`, after the `SplitAssignmentRecord` exists —
/// [`ExperimentRefusal::AdaptiveOutsideSearch`] otherwise.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationStrategy {
    /// The default — every planned run executes.
    FullSet,
    /// Disagreement-weighted sampling (ADR-0156).
    DisagreementWeighted {
        /// The weighting parameter.
        lambda: u64,
        /// The minimum inclusion fraction (ppm).
        min_inclusion_fraction_ppm: u64,
        /// The estimator (`hajek | anchored_difference`).
        estimator: String,
    },
    /// Successive-halving (ADR-0156).
    SuccessiveHalving {
        /// The halving factor (`eta = 3` default).
        eta: u32,
        /// The minimum per-bracket budget.
        min_budget: u64,
        /// The bracket count.
        brackets: u32,
    },
    /// Value-of-information weighting (dialect bump — ADR-0190 D7).
    VoiWeighted {
        /// The estimator (`disagreement | expected_information_gain`).
        estimator: String,
        /// The weighting parameter.
        lambda: u64,
        /// The minimum inclusion fraction (ppm).
        min_inclusion_fraction_ppm: u64,
        /// The minimum replicates.
        min_replicates: u32,
    },
}

impl ValidationStrategy {
    /// Whether the strategy is a search-only adaptive strategy (anything but
    /// `full_set`).
    pub fn is_adaptive(&self) -> bool {
        !matches!(self, ValidationStrategy::FullSet)
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        match self {
            ValidationStrategy::FullSet => Json::str("full_set"),
            ValidationStrategy::DisagreementWeighted {
                lambda,
                min_inclusion_fraction_ppm,
                estimator,
            } => Json::obj([(
                "disagreement_weighted",
                Json::obj([
                    ("lambda", Json::Int(*lambda as i64)),
                    (
                        "min_inclusion_fraction",
                        Json::Int(*min_inclusion_fraction_ppm as i64),
                    ),
                    ("estimator", Json::str(estimator)),
                ]),
            )]),
            ValidationStrategy::SuccessiveHalving {
                eta,
                min_budget,
                brackets,
            } => Json::obj([(
                "successive_halving",
                Json::obj([
                    ("eta", Json::Int(*eta as i64)),
                    ("min_budget", Json::Int(*min_budget as i64)),
                    ("brackets", Json::Int(*brackets as i64)),
                ]),
            )]),
            ValidationStrategy::VoiWeighted {
                estimator,
                lambda,
                min_inclusion_fraction_ppm,
                min_replicates,
            } => Json::obj([(
                "voi_weighted",
                Json::obj([
                    ("estimator", Json::str(estimator)),
                    ("lambda", Json::Int(*lambda as i64)),
                    (
                        "min_inclusion_fraction",
                        Json::Int(*min_inclusion_fraction_ppm as i64),
                    ),
                    ("min_replicates", Json::Int(*min_replicates as i64)),
                ]),
            )]),
        }
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ValidationStrategy, SchemaError> {
        match j {
            Json::Str(s) if s == "full_set" => Ok(ValidationStrategy::FullSet),
            Json::Obj(m) if m.len() == 1 => {
                let (k, v) = m.iter().next().expect("len checked");
                let vm = expect_obj(v, "ValidationStrategy")?;
                match k.as_str() {
                    "disagreement_weighted" => {
                        reject_unknown(
                            vm,
                            &["lambda", "min_inclusion_fraction", "estimator"],
                            "DisagreementWeighted",
                        )?;
                        let est = str_at(vm, "estimator", "DisagreementWeighted")?;
                        if !matches!(est, "hajek" | "anchored_difference") {
                            return Err(SchemaError::v(
                                "estimator",
                                format!("unknown disagreement_weighted estimator `{est}`"),
                            ));
                        }
                        Ok(ValidationStrategy::DisagreementWeighted {
                            lambda: int_at(vm, "lambda", "DisagreementWeighted")? as u64,
                            min_inclusion_fraction_ppm: int_at(
                                vm,
                                "min_inclusion_fraction",
                                "DisagreementWeighted",
                            )? as u64,
                            estimator: est.to_string(),
                        })
                    }
                    "successive_halving" => {
                        reject_unknown(
                            vm,
                            &["eta", "min_budget", "brackets"],
                            "SuccessiveHalving",
                        )?;
                        Ok(ValidationStrategy::SuccessiveHalving {
                            eta: int_at(vm, "eta", "SuccessiveHalving")? as u32,
                            min_budget: int_at(vm, "min_budget", "SuccessiveHalving")? as u64,
                            brackets: int_at(vm, "brackets", "SuccessiveHalving")? as u32,
                        })
                    }
                    "voi_weighted" => {
                        reject_unknown(
                            vm,
                            &[
                                "estimator",
                                "lambda",
                                "min_inclusion_fraction",
                                "min_replicates",
                            ],
                            "VoiWeighted",
                        )?;
                        let est = str_at(vm, "estimator", "VoiWeighted")?;
                        if !matches!(est, "disagreement" | "expected_information_gain") {
                            return Err(SchemaError::v(
                                "estimator",
                                format!("unknown voi_weighted estimator `{est}`"),
                            ));
                        }
                        Ok(ValidationStrategy::VoiWeighted {
                            estimator: est.to_string(),
                            lambda: int_at(vm, "lambda", "VoiWeighted")? as u64,
                            min_inclusion_fraction_ppm: int_at(
                                vm,
                                "min_inclusion_fraction",
                                "VoiWeighted",
                            )? as u64,
                            min_replicates: int_at(vm, "min_replicates", "VoiWeighted")? as u32,
                        })
                    }
                    _ => Err(SchemaError::v(
                        "validation_strategy",
                        format!("unknown strategy `{k}`"),
                    )),
                }
            }
            _ => Err(SchemaError::v(
                "validation_strategy",
                "must be `full_set` or a one-key strategy object",
            )),
        }
    }
}

/// `budgets{experiment, instrument}` — the experiment run's own budgets
/// (§6.3; distinct from the per-arm `eval_budget`/`search_budget`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExperimentBudgets {
    /// The experiment budget ref (the sweep's own spend).
    pub experiment: String,
    /// The instrument budget ref (graders, validators, sweeps).
    pub instrument: String,
}

/// `bundle_policy` — how the experiment's runs bundle (§6.3; the §05c bundle
/// kinds).
#[derive(Debug, Clone, PartialEq)]
pub enum BundlePolicy {
    /// An ad-hoc bundle keyed by a salt.
    Adhoc {
        /// The ad-hoc salt.
        salt: String,
    },
    /// A named bundle.
    Named {
        /// The bundle name.
        name: String,
    },
    /// A published bundle — a pinned ref.
    Published {
        /// The bundle ref.
        bundle_ref: String,
    },
}

impl BundlePolicy {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        match self {
            BundlePolicy::Adhoc { salt } => {
                Json::obj([("adhoc", Json::obj([("salt", Json::str(salt))]))])
            }
            BundlePolicy::Named { name } => {
                Json::obj([("named", Json::obj([("name", Json::str(name))]))])
            }
            BundlePolicy::Published { bundle_ref } => {
                Json::obj([("published", Json::obj([("ref", Json::str(bundle_ref))]))])
            }
        }
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<BundlePolicy, SchemaError> {
        let m = expect_obj(j, "BundlePolicy")?;
        if m.len() != 1 {
            return Err(SchemaError::v(
                "bundle_policy",
                "must be a one-key {adhoc|named|published} object",
            ));
        }
        let (k, v) = m.iter().next().expect("len checked");
        let vm = expect_obj(v, "BundlePolicy")?;
        match k.as_str() {
            "adhoc" => {
                reject_unknown(vm, &["salt"], "adhoc")?;
                Ok(BundlePolicy::Adhoc {
                    salt: str_at(vm, "salt", "adhoc")?.to_string(),
                })
            }
            "named" => {
                reject_unknown(vm, &["name"], "named")?;
                Ok(BundlePolicy::Named {
                    name: str_at(vm, "name", "named")?.to_string(),
                })
            }
            "published" => {
                reject_unknown(vm, &["ref"], "published")?;
                Ok(BundlePolicy::Published {
                    bundle_ref: str_at(vm, "ref", "published")?.to_string(),
                })
            }
            _ => Err(SchemaError::v(
                "bundle_policy",
                format!("unknown bundle policy `{k}`"),
            )),
        }
    }
}

// ── ExperimentSpec ──────────────────────────────────────────────────────────

/// `ExperimentSpec` (dialect `hh-experiment/1`; §6.3; ADR-0154 D1) — MUST-data;
/// immutable; amendment by supersession. `experiment_id =
/// H(canonical(spec))` under the `experiment` domain.
#[derive(Debug, Clone, PartialEq)]
pub struct ExperimentSpec {
    /// `experiment_id = H(canonical(spec))` — computed, never authored
    /// ([`ExperimentSpec::experiment_id`]).
    pub experiment_id: String,
    /// The experiment kind.
    pub kind: ExperimentKind,
    /// The design (ADR-0045's `Design` — carries the mandatory
    /// `pre_registration` at the design level).
    pub design: Design,
    /// The experiment-level pre-registration (required unless `exploratory`;
    /// distinct from `design.pre_registration` — the §6.3 member).
    pub pre_registration: Option<PreRegistration>,
    /// The declared factors.
    pub factors: Vec<FactorSpec>,
    /// The arms.
    pub arms: Vec<ArmSpec>,
    /// The suite binding.
    pub suite: SuiteBinding,
    /// `replicates_per_cell ≥ 1` (engine default `5` for Stage-3 exemplars).
    pub replicates_per_cell: u32,
    /// The seed policy.
    pub seed_policy: SeedPolicy,
    /// The validation strategy.
    pub validation_strategy: ValidationStrategy,
    /// The scheduling policy.
    pub scheduling: SchedulingPolicy,
    /// The reattempt policy.
    pub reattempt: ReattemptPolicy,
    /// The experiment/instrument budgets.
    pub budgets: ExperimentBudgets,
    /// The bundle policy.
    pub bundle_policy: BundlePolicy,
    /// Dialect extension surface.
    pub ext: BTreeMap<String, Json>,
}

/// The closed `register` refusal set (§6.3 §2.1; E-1; T-LCD-14 — refusal,
/// never a warning). The schema-checkable subset runs member-level; the
/// context-parameterized members run against [`SpecContext`].
#[derive(Debug, Clone, PartialEq)]
pub enum ExperimentRefusal {
    /// An arm on a matched kind carries no `match_spec`.
    MissingMatchSpec {
        /// The offending arm.
        arm: String,
    },
    /// An arm on a matched kind carries no `eval_budget`/`search_budget` —
    /// or `eval_budget` is empty.
    UnbudgetedArm {
        /// The offending arm.
        arm: String,
    },
    /// The matched-budget precondition failed dimension-wise (engine-side
    /// consumption check is Stage 3; the schema half is the resolved-body
    /// `validate_match` failure).
    UnmatchedBudget {
        /// The failure detail.
        detail: String,
    },
    /// The match is incommensurable (projected `MatchError` from
    /// `hh_budget::matchspec::validate_match`, or a `matched_cap` dimension
    /// not `enforced` on every arm — ADR-0041 as amended).
    IncommensurableMatch {
        /// The failure detail.
        detail: String,
    },
    /// A `cross_model`/`cross_product` match names no `pricing_table_ref`.
    MissingPricingTable {
        /// The failure detail.
        detail: String,
    },
    /// A factor level is outside the admissible set: a `non_portable` level
    /// assigned where the profile is the varied factor (ADR-0154 D5), or a
    /// `non_portable` level on a `reproduction`/`equivalence` spec.
    InadmissibleFactor {
        /// The factor.
        factor: String,
        /// The reason.
        reason: String,
    },
    /// `kind ≠ exploratory` and `pre_registration` absent.
    MissingPreRegistration,
    /// The experiment runs over a suite with no registered
    /// `SplitAssignmentRecord` (`suite.split_assignment_ref` absent) on a
    /// matched kind or an `adaptive_search` design.
    SplitUnassigned {
        /// The suite ref.
        suite_ref: String,
    },
    /// The experiment's `split_labels_used` include a label not
    /// `search_admissible` for the design's search side, or a `search`/`dev`
    /// split is used by an arm whose artifact is the frozen eval artifact
    /// (a held-out leak at register). The schema half: `adaptive_search`
    /// designs must confine themselves to `search|dev`; a `held_out` label
    /// on a `search`-split arm is refused.
    LeakedSplit {
        /// The failure detail.
        detail: String,
    },
    /// `artifact_ref` is not a sealed definition/product version — its
    /// `version_id` is not a pinned content address (`<algo>:<hex>`), or the
    /// context's `artifact_sealed` view reports unsealed.
    UnsealedArtifact {
        /// The offending artifact ref.
        artifact_ref: String,
    },
    /// `replicates_per_cell` below the context's floor (engine default 5 for
    /// Stage-3 exemplars; the schema floor is 1 — the context supplies the
    /// policy floor).
    InsufficientReplicates {
        /// The cell/detail.
        detail: String,
        /// The declared count.
        have: u32,
        /// The required count.
        need: u32,
    },
    /// `design.kind = fractional_factorial` and the pre-registration names a
    /// two-factor interaction the declared `resolution` cannot estimate
    /// unconfounded (any 2FI needs ≥ IV; mutually unconfounded 2FIs need V).
    /// The schema half: `fractional_factorial` requires `generators[]` and a
    /// `resolution` member on the design's `ext`.
    ResolutionInsufficient {
        /// The failure detail.
        detail: String,
    },
    /// A pinned-profile level (`non_portable`) is varied in an arm where the
    /// profile is the varied factor — ADR-0154 D5's "only in arms where the
    /// profile is not the varied factor".
    ProfilePinnedAcrossProfiles {
        /// The failure detail.
        detail: String,
    },
    /// A level's ref resolves to a capability the context reports drifted
    /// (ctx-dependent).
    DependsOnDriftedCapability {
        /// The drifted capability/level ref.
        capability: String,
    },
    /// `kind = retirement` and the context's retirement-diff view is `false`
    /// (the arms are not a single-rule removal diff — ADR-0156).
    NotARetirementDiff {
        /// The failure detail.
        detail: String,
    },
    /// `kind = retirement` and an arm's `MatchSpec` is not the removal test's
    /// required `{mode: matched_cap, cache_policy: cold_start}`
    /// (AC-R-2.3.3-8 — the removal experiment is defined only under that
    /// match shape).
    NotARetirementMatch {
        /// The offending arm and what it declared.
        detail: String,
    },
    /// `PreRegistrationInvalid` — a K5-enabled arm on a non-`reproduction`
    /// spec without `cache` declared as a factor (AC-R-2.3.4-11;
    /// ADR-0129 d.2 — the same refusal name `hh_context::k5` uses).
    PreRegistrationInvalid {
        /// The failure detail.
        detail: String,
    },
    /// `validation_strategy ≠ full_set` while `design.kind ≠
    /// adaptive_search`, or over non-`search|dev` split labels, or before the
    /// `SplitAssignmentRecord` exists (ADR-0156/0190).
    AdaptiveOutsideSearch {
        /// The failure detail.
        detail: String,
    },
    /// `declare` after a `run_bound` on the experiment run — the producer
    /// contract's timing refusal (§6.5 §2.3; ADR-0162 D2). A second
    /// `declared` *without* bound runs is `SchemaViolation` (`AlreadyOpen`
    /// at the engine layer); with bound runs the refusal is this member.
    DeclarationLate {
        /// The failure detail.
        detail: String,
    },
    /// `PreRegistration.registered_at` postdates a bound run — the S9
    /// mirror refusal at the producer boundary (§6.5 §2.3; ADR-0162 D4).
    PreRegistrationLate {
        /// The failure detail.
        detail: String,
    },
    /// `record_analysis` on a record claiming `pre_registered = true` whose
    /// `registered_analysis_ref` does not match the experiment's pinned
    /// plan by identity (§6.5 §2.3; ADR-0162 D3).
    NotPreRegistered {
        /// The failure detail.
        detail: String,
    },
    /// A member-level schema violation (decode).
    Schema(SchemaError),
}

impl From<SchemaError> for ExperimentRefusal {
    fn from(e: SchemaError) -> ExperimentRefusal {
        ExperimentRefusal::Schema(e)
    }
}

/// The resolution view [`ExperimentSpec::register`] checks against — the
/// context-parameterized half of the refusal set (the engine's E-1 gate
/// supplies every resolver at Stage 3; `None` = the check cannot run at this
/// layer and is deferred).
/// The `resolve_budget` resolver signature — `ref → BudgetSpec` view (the
/// `Option` payload mirrors "resolvable at this validation point").
pub type BudgetResolver<'a> = dyn Fn(&str) -> Option<BudgetSpec> + 'a;

/// Resolves a participant coordinate's capability descriptor for hosted-level
/// admissibility (`capabilities[<factor name>]` → the ADR-0152 verdict).
pub type ParticipantDescriptor<'a> = dyn Fn(&str) -> Option<Json> + 'a;

#[derive(Default)]
pub struct SpecContext<'a> {
    /// Resolve a budget ref to its body (`None` = cannot resolve — the
    /// `validate_match` projection is skipped).
    pub resolve_budget: Option<&'a BudgetResolver<'a>>,
    /// Whether an artifact ref is sealed (in addition to the pinned-id
    /// member check, which always runs).
    pub artifact_sealed: Option<&'a dyn Fn(&str) -> bool>,
    /// Whether the arms form a single-rule removal diff (`kind =
    /// retirement` only; `None` = cannot check).
    pub retirement_diff: Option<bool>,
    /// Whether a level ref's capability has drifted.
    pub capability_drifted: Option<&'a dyn Fn(&str) -> bool>,
    /// The arm's per-dimension `budget_enforcement` view (ADR-0165 D3) —
    /// the E-1 `matched_cap` rule needs `enforced` on every matched
    /// dimension. `None` = every arm resolves `native` (trivially
    /// `enforced`; hosted arms supply their adapter-derived map).
    pub budget_enforcement: Option<&'a dyn Fn(&ArmSpec) -> BudgetEnforcement>,
    /// The level ref's `budget_relevant` parameter bindings — `param →
    /// {value, affects[]}` (`affects` names the dimensions the parameter
    /// drives). `None` = cannot resolve (the AC-R-2.10.2-12 coverage check
    /// is deferred to a context that can).
    pub budget_relevant_params: Option<&'a BudgetRelevantResolver<'a>>,
    /// The `replicates_per_cell` policy floor (default 1).
    pub min_replicates: u32,
    /// The arm's bound dialect's `cache_state_visible` view (ADR-0119 d.3;
    /// resolved through the arm's `model_role_table_ref`/artifact at the
    /// engine's E-1 gate). `Some(false)` = `cache_state_visible =
    /// unsupported`; `None` (resolver absent or arm unresolvable) = the
    /// AC-R-2.3.4-10 visibility check defers to a context that can run it.
    pub cache_state_visible: Option<&'a CacheVisibilityResolver<'a>>,
    /// The `participant` descriptor resolver — `level ref → the registry's
    /// `participant` record body` (ADR-0013/0151; §6.3 C1). The hosted-level
    /// admissibility check reads `capabilities[<factor name>]` off the
    /// descriptor (ADR-0152's verdict vocabulary): a `configuration-level`
    /// hosted level is admissible only where the varied coordinate resolves
    /// `SUPPORTED` (an unknown coordinate is never coerced — T-LCD-07).
    /// `None` = the check defers to a context that can resolve participants.
    pub participant_descriptor: Option<&'a ParticipantDescriptor<'a>>,
}

/// A `budget_relevant` parameter's bound value and the dimensions it drives
/// (the `param_schema` `affects[]` projection — ADR-0151 D5). The
/// AC-R-2.10.2-12 coverage check refuses an arm whose `MatchSpec` omits a
/// `budget_relevant` parameter that differs across arms (T-LCD-14).
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetRelevantParam {
    /// The parameter's bound value at the level (the variant's declared
    /// point; a `LevelSpec.overrides` member rebinds it).
    pub value: Json,
    /// The dimensions the parameter drives (`affects[]`).
    pub affects: Vec<DimensionId>,
}

/// The `budget_relevant` resolver — `level ref → {param → {value, affects[]}}`.
pub type BudgetRelevantResolver<'a> = dyn Fn(&str) -> BTreeMap<String, BudgetRelevantParam> + 'a;

/// The `cache_state_visible` resolver — `arm → the bound dialect's
/// visibility` (`Some(false)` = `unsupported`; `None` = unresolvable — the
/// AC-R-2.3.4-10 check defers to a context that can).
pub type CacheVisibilityResolver<'a> = dyn Fn(&ArmSpec) -> Option<bool> + 'a;

/// A level's bound `budget_relevant` parameters — `{param → (value,
/// affects)}` (the check_match working map).
type BoundParams = BTreeMap<String, (Json, BTreeSet<DimensionId>)>;

impl SpecContext<'_> {
    /// The member-level-only context — every resolver absent (the checks
    /// that need them defer to the engine).
    pub fn member_level() -> SpecContext<'static> {
        SpecContext {
            resolve_budget: None,
            artifact_sealed: None,
            retirement_diff: None,
            capability_drifted: None,
            budget_enforcement: None,
            budget_relevant_params: None,
            min_replicates: 1,
            cache_state_visible: None,
            participant_descriptor: None,
        }
    }
}

/// Whether `version_id` is a pinned content address (`<algorithm>:<hex>`).
fn is_pinned(version_id: &str) -> bool {
    match version_id.split_once(':') {
        Some((algo, digest)) => {
            !algo.is_empty() && !digest.is_empty() && digest.chars().all(|c| c.is_ascii_hexdigit())
        }
        None => false,
    }
}

impl ExperimentSpec {
    /// `experiment_id = H(canonical(spec minus experiment_id))` under the
    /// `experiment` domain.
    pub fn experiment_id(&self) -> String {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("experiment_id");
        }
        identify_bytes(
            RecordKind::ExperimentSpec,
            j.to_canonical_string().as_bytes(),
        )
    }

    /// The `register` schema checks — the E-1 refusal set's C0 half.
    ///
    /// Order matters: the cheap member-level gates run first, then the
    /// context-parameterized checks; the first refusal wins (the refusal is
    /// total — one deterministic answer).
    pub fn register(&self, ctx: &SpecContext<'_>) -> Result<(), ExperimentRefusal> {
        // 1. `pre_registration` required unless exploratory.
        if self.kind.requires_pre_registration() && self.pre_registration.is_none() {
            return Err(ExperimentRefusal::MissingPreRegistration);
        }
        // 2. `replicates_per_cell` ≥ the policy floor.
        let floor = ctx.min_replicates.max(1);
        if self.replicates_per_cell < floor {
            return Err(ExperimentRefusal::InsufficientReplicates {
                detail: "replicates_per_cell".to_string(),
                have: self.replicates_per_cell,
                need: floor,
            });
        }
        // 3. Factor-level admissibility (ADR-0154 D5 + the kind table +
        //    the hosted-level granularity rule, AC-R-2.10.3-3).
        self.check_factors(ctx)?;
        // 3.5. The design shape (resolution/generators, kind-table coverage,
        //      named-interaction estimability) and the scheduling pools.
        self.check_design()?;
        // 4. The adaptive strategy confinement (ADR-0156/0190).
        self.check_validation_strategy()?;
        // 5. The suite/split binding.
        self.check_suite()?;
        // 6. The arm-level checks (budgets, match specs, sealed artifacts,
        //    pinned-profile confinement).
        self.check_arms(ctx)?;
        // 7. The resolved-body `validate_match` projection (when the context
        //    can resolve budget bodies).
        self.check_match(ctx)?;
        // 7.5. The cache-policy checks — AC-R-2.3.4-10 (`natural` on a
        //      cache-invisible dialect without the design's `n/a`
        //      stratification), AC-R-2.3.4-11 (K5 without a declared `cache`
        //      factor), AC-R-2.3.3-8 (the retirement match shape).
        self.check_cache(ctx)?;
        // 8. `kind`-specific context checks.
        if self.kind == ExperimentKind::Retirement && ctx.retirement_diff == Some(false) {
            return Err(ExperimentRefusal::NotARetirementDiff {
                detail: "arms are not a single-rule removal diff".to_string(),
            });
        }
        Ok(())
    }

    /// The cache-policy half of the `register` refusal set (T-LCD-14 —
    /// refusal, never a warning).
    fn check_cache(&self, ctx: &SpecContext<'_>) -> Result<(), ExperimentRefusal> {
        for arm in &self.arms {
            // AC-R-2.3.4-10 — a `natural` arm on a dialect with
            // `cache_state_visible = unsupported` needs the design's declared
            // `n/a` stratification (`cache_na_stratified`); the warm cache
            // may never silently contaminate a matched comparison. An absent
            // resolver or an unresolvable arm defers the check to the engine
            // layer that can see the dialect.
            let natural = arm
                .match_spec
                .as_ref()
                .is_some_and(|ms| ms.cache_policy == CachePolicy::Natural);
            if natural && !self.design.cache_na_stratified {
                if let Some(visible) = ctx.cache_state_visible {
                    if visible(arm) == Some(false) {
                        return Err(ExperimentRefusal::IncommensurableMatch {
                            detail: format!(
                                "arm `{}` declares cache_policy = natural on a dialect \
                                 with cache_state_visible = unsupported without the \
                                 design's cache_na_stratified n/a stratification",
                                arm.arm_id
                            ),
                        });
                    }
                }
            }
            // AC-R-2.3.4-11 — a K5-enabled arm on a non-`reproduction` spec
            // requires `cache` declared as a factor of the pre-registered
            // design (the same `PreRegistrationInvalid` `hh_context::k5`'s
            // `admit` reports).
            if arm.response_cache.is_some() && self.kind != ExperimentKind::Reproduction {
                let declared = self.factors.iter().any(|f| f.name == "cache");
                if !declared {
                    return Err(ExperimentRefusal::PreRegistrationInvalid {
                        detail: format!(
                            "arm `{}` enables the K5 response cache without `cache` \
                             declared as a factor of the design",
                            arm.arm_id
                        ),
                    });
                }
            }
        }
        // AC-R-2.3.3-8 — a `retirement` design's arms must carry the removal
        // test's match shape: `MatchSpec{mode: matched_cap, cache_policy:
        // cold_start}` (equal eval_budget is the `validate_match` half).
        if self.kind == ExperimentKind::Retirement {
            for arm in &self.arms {
                let Some(ms) = &arm.match_spec else {
                    continue; // `MissingMatchSpec` (check_arms) owns absence
                };
                if ms.mode != MatchMode::MatchedCap || ms.cache_policy != CachePolicy::ColdStart {
                    return Err(ExperimentRefusal::NotARetirementMatch {
                        detail: format!(
                            "arm `{}`: a retirement design requires \
                             MatchSpec{{mode: matched_cap, cache_policy: cold_start}}",
                            arm.arm_id
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    fn check_factors(&self, ctx: &SpecContext<'_>) -> Result<(), ExperimentRefusal> {
        let mut names = BTreeSet::new();
        for f in &self.factors {
            if !names.insert(f.name.as_str()) {
                return Err(ExperimentRefusal::InadmissibleFactor {
                    factor: f.name.clone(),
                    reason: "duplicate factor name".to_string(),
                });
            }
            if f.levels.is_empty() {
                return Err(ExperimentRefusal::InadmissibleFactor {
                    factor: f.name.clone(),
                    reason: "a factor must declare ≥ 1 level".to_string(),
                });
            }
            let mut level_ids = BTreeSet::new();
            for l in &f.levels {
                if !level_ids.insert(l.level_id.as_str()) {
                    return Err(ExperimentRefusal::InadmissibleFactor {
                        factor: f.name.clone(),
                        reason: format!("duplicate level id `{}`", l.level_id),
                    });
                }
                // A non-portable (pinned-profile) level is inadmissible on
                // `reproduction`/`equivalence` kinds — those kinds demand a
                // portable coordinate.
                if l.non_portable
                    && matches!(
                        self.kind,
                        ExperimentKind::Reproduction | ExperimentKind::Equivalence
                    )
                {
                    return Err(ExperimentRefusal::InadmissibleFactor {
                        factor: f.name.clone(),
                        reason: format!(
                            "non_portable level `{}` on a `{}` spec",
                            l.level_id,
                            self.kind.name()
                        ),
                    });
                }
                // Hosted-level granularity admissibility (§6.3 §2.1;
                // AC-R-2.10.3-3): a hosted participant admits only
                // configuration-level and product-level variation — a
                // component-level coordinate is refused `InadmissibleFactor`
                // at `register`, never a warning.
                if l.class == ParticipantClass::Hosted {
                    match f.granularity {
                        Some(Granularity::ComponentLevel) => {
                            return Err(ExperimentRefusal::InadmissibleFactor {
                                factor: f.name.clone(),
                                reason: format!(
                                    "hosted level `{}` is inadmissible on a \
                                     component-level factor",
                                    l.level_id
                                ),
                            });
                        }
                        Some(Granularity::ConfigurationLevel) => {
                            // Admissible only where the participant
                            // descriptor's capability vector declares the
                            // varied coordinate `SUPPORTED`; an unknown
                            // coordinate is never coerced (T-LCD-07). The
                            // check defers to a context that can resolve the
                            // participant record (`None` resolver ⇒ the
                            // refusal lands where the descriptor resolves).
                            if let Some(describe) = ctx.participant_descriptor {
                                let supported = describe(&l.ref_)
                                    .and_then(|d| {
                                        d.get("capabilities")
                                            .and_then(|c| c.get(&f.name))
                                            .and_then(Json::as_str)
                                            .map(str::to_string)
                                    })
                                    .map(|v| v == "SUPPORTED")
                                    .unwrap_or(false);
                                if !supported {
                                    return Err(ExperimentRefusal::InadmissibleFactor {
                                        factor: f.name.clone(),
                                        reason: format!(
                                            "hosted level `{}` on configuration-level factor \
                                             `{}` requires capability coordinate `{}` = \
                                             SUPPORTED on the participant descriptor",
                                            l.level_id, f.name, f.name
                                        ),
                                    });
                                }
                            }
                        }
                        // `product-level` (or absent — the product
                        // default) is admissible for hosted.
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    /// The design-shape checks (§6.3 §2.1's `ResolutionInsufficient` row plus
    /// the design kinds' fixed semantics — ADR-0154 D2) and the scheduling
    /// pool bound (`pool.limit ≤ max_concurrent_runs`; AC-R-2.10.3-10's
    /// register half).
    fn check_design(&self) -> Result<(), ExperimentRefusal> {
        use crate::expand::*;
        // A pool limit above the global cap is refused at `register`.
        for p in &self.scheduling.pools {
            if p.limit > self.scheduling.max_concurrent_runs {
                return Err(ExperimentRefusal::Schema(SchemaError::v(
                    "scheduling.pools",
                    format!(
                        "pool `{}` limit {} exceeds max_concurrent_runs {}",
                        p.key, p.limit, self.scheduling.max_concurrent_runs
                    ),
                )));
            }
        }
        let varied = varied_factors(self);
        match self.design.kind {
            DesignKind::FractionalFactorial => {
                // `generators[]` and `resolution` are mandatory members.
                let (Some(gen_spellings), Some(declared)) =
                    (&self.design.generators, self.design.resolution)
                else {
                    return Err(ExperimentRefusal::ResolutionInsufficient {
                        detail:
                            "fractional_factorial requires generators[] and resolution ∈ {III, IV, V}"
                                .to_string(),
                    });
                };
                if gen_spellings.is_empty() {
                    return Err(ExperimentRefusal::ResolutionInsufficient {
                        detail: "fractional_factorial requires ≥ 1 generator".to_string(),
                    });
                }
                // Two-level factors only (the OQ-361 ratified default).
                for f in &varied {
                    if f.levels.len() != 2 {
                        return Err(ExperimentRefusal::InadmissibleFactor {
                            factor: f.name.clone(),
                            reason:
                                "fractional_factorial admits two-level factors only (OQ-361 default)"
                                    .to_string(),
                        });
                    }
                }
                // The generators must parse, name declared factors, generate
                // each factor at most once, and not generate a free factor's
                // word member.
                let names: BTreeSet<&str> = varied.iter().map(|f| f.name.as_str()).collect();
                let mut generated = BTreeSet::new();
                let mut gens = Vec::with_capacity(gen_spellings.len());
                for g in gen_spellings {
                    let gen = parse_generator(g).ok_or_else(|| {
                        ExperimentRefusal::Schema(SchemaError::v(
                            "design.generators",
                            format!("malformed generator `{g}` (expected `F = A:B:…`)"),
                        ))
                    })?;
                    for member in gen.defining_word() {
                        if !names.contains(member.as_str()) {
                            return Err(ExperimentRefusal::InadmissibleFactor {
                                factor: member.clone(),
                                reason: format!(
                                    "generator `{g}` names an undeclared varied factor"
                                ),
                            });
                        }
                    }
                    if !generated.insert(gen.factor.clone()) {
                        return Err(ExperimentRefusal::InadmissibleFactor {
                            factor: gen.factor.clone(),
                            reason: format!("factor generated twice (`{g}`)"),
                        });
                    }
                    gens.push(gen);
                }
                // The declared resolution must equal what the defining
                // subgroup computes.
                let subgroup = defining_subgroup(&gens);
                let computed = resolution_of(&subgroup);
                let declared_n = match declared {
                    hh_ontology::eval::FractionalResolution::III => 3,
                    hh_ontology::eval::FractionalResolution::IV => 4,
                    hh_ontology::eval::FractionalResolution::V => 5,
                };
                if computed != declared_n {
                    return Err(ExperimentRefusal::ResolutionInsufficient {
                        detail: format!(
                            "declared resolution {} but the generators define {}",
                            declared.as_str(),
                            match computed {
                                3 => "III",
                                4 => "IV",
                                5 => "V",
                                n =>
                                    return Err(ExperimentRefusal::ResolutionInsufficient {
                                        detail: format!("generators define resolution {n}"),
                                    }),
                            }
                        ),
                    });
                }
                // The declared arms must be exactly the fraction's points.
                if !arms_cover(self, &fraction_points(self, &gens)) {
                    return Err(ExperimentRefusal::InadmissibleFactor {
                        factor: "<design>".to_string(),
                        reason: "arms do not cover the declared fraction's design points exactly"
                            .to_string(),
                    });
                }
                // Pre-registered two-factor interactions must be estimable:
                // any 2FI needs ≥ IV; two named 2FIs in one alias class need
                // V (mutually unconfounded).
                let named = self.named_interactions();
                if !declared.admits_two_factor_interaction() && !named.is_empty() {
                    return Err(ExperimentRefusal::ResolutionInsufficient {
                        detail: format!(
                            "pre-registered two-factor interaction(s) {} under resolution III",
                            named.join(", ")
                        ),
                    });
                }
                if declared < hh_ontology::eval::FractionalResolution::V {
                    // Two named 2FIs aliased to each other are confounded.
                    for (i, a) in named.iter().enumerate() {
                        for b in &named[i + 1..] {
                            let (Some(ea), Some(eb)) = (parse_effect(a), parse_effect(b)) else {
                                continue;
                            };
                            if ea.len() == 2
                                && eb.len() == 2
                                && alias_class(&ea, &subgroup).contains(&eb)
                            {
                                return Err(ExperimentRefusal::ResolutionInsufficient {
                                    detail: format!(
                                        "pre-registered interactions `{a}` and `{b}` are mutually aliased; resolution V required"
                                    ),
                                });
                            }
                        }
                    }
                }
            }
            DesignKind::Paired => {
                // `paired` = one varied factor, pairing by task.
                if varied.len() != 1 {
                    return Err(ExperimentRefusal::InadmissibleFactor {
                        factor: "<design>".to_string(),
                        reason: format!(
                            "paired design declares {} varied factors (exactly one)",
                            varied.len()
                        ),
                    });
                }
            }
            DesignKind::FullFactorial => {
                // `full_factorial` = the Cartesian product — the arms must be
                // exactly the product's points.
                if !arms_cover(self, &full_product(self)) {
                    return Err(ExperimentRefusal::InadmissibleFactor {
                        factor: "<design>".to_string(),
                        reason: "arms do not cover the full factor product exactly".to_string(),
                    });
                }
            }
            DesignKind::OneFactorAtATime => {
                // `one_factor_at_a_time` = the base point (each varied
                // factor's first declared level) plus the single-level
                // perturbations.
                let mut base = BTreeMap::new();
                for f in &varied {
                    if let Some(l) = f.levels.first() {
                        base.insert(f.name.clone(), l.level_id.clone());
                    }
                }
                let mut points = vec![base.clone()];
                for f in &varied {
                    for l in f.levels.iter().skip(1) {
                        let mut p = base.clone();
                        p.insert(f.name.clone(), l.level_id.clone());
                        points.push(p);
                    }
                }
                if !arms_cover(self, &points) {
                    return Err(ExperimentRefusal::InadmissibleFactor {
                        factor: "<design>".to_string(),
                        reason: "arms are not the base point plus its single-level perturbations"
                            .to_string(),
                    });
                }
            }
            DesignKind::AdaptiveSearch => {}
        }
        // `generators`/`resolution` are fractional-factorial members; declared
        // on another kind they are refused (the design's resolution claims
        // are incoherent, not merely inert).
        if self.design.kind != DesignKind::FractionalFactorial
            && (self.design.generators.is_some() || self.design.resolution.is_some())
        {
            return Err(ExperimentRefusal::ResolutionInsufficient {
                detail: format!(
                    "generators/resolution declared on design.kind = {}",
                    self.design.kind.as_str()
                ),
            });
        }
        Ok(())
    }

    /// The pre-registered interaction terms — the union of the spec-level
    /// `pre_registration.interactions` and the design-level record's
    /// (deterministic sorted order).
    fn named_interactions(&self) -> Vec<String> {
        let mut named: BTreeSet<String> = BTreeSet::new();
        if let Some(p) = &self.pre_registration {
            named.extend(p.interactions.iter().cloned());
        }
        named.extend(self.design.pre_registration.interactions.iter().cloned());
        named.into_iter().collect()
    }

    fn check_validation_strategy(&self) -> Result<(), ExperimentRefusal> {
        if !self.validation_strategy.is_adaptive() {
            return Ok(());
        }
        if self.design.kind != DesignKind::AdaptiveSearch {
            return Err(ExperimentRefusal::AdaptiveOutsideSearch {
                detail: format!(
                    "adaptive validation strategy with design.kind = {}",
                    self.design.kind.as_str()
                ),
            });
        }
        // Adaptive strategies confine to {search, dev} split labels and
        // require the registered split assignment.
        for l in &self.suite.split_labels_used {
            if !matches!(l, SplitLabel::Search | SplitLabel::Dev) {
                return Err(ExperimentRefusal::AdaptiveOutsideSearch {
                    detail: format!("adaptive strategy over split label `{}`", l.name()),
                });
            }
        }
        if self.suite.split_assignment_ref.is_none() {
            return Err(ExperimentRefusal::SplitUnassigned {
                suite_ref: self.suite.suite_ref.clone(),
            });
        }
        Ok(())
    }

    fn check_suite(&self) -> Result<(), ExperimentRefusal> {
        // Matched kinds and adaptive designs need the registered split
        // assignment.
        if (self.kind.requires_match() || self.design.kind == DesignKind::AdaptiveSearch)
            && self.suite.split_assignment_ref.is_none()
        {
            return Err(ExperimentRefusal::SplitUnassigned {
                suite_ref: self.suite.suite_ref.clone(),
            });
        }
        // `LeakedSplit` (schema half): an `adaptive_search` design touching a
        // non-search-admissible label, or any search-split label used by a
        // non-adaptive design's *search side* — the eval side legitimately
        // uses `held_out`/`private`/`control` (that is the point); the
        // refusal targets a `held_out`-family label inside `split_labels_used`
        // of an adaptive design (handled above) plus `search`/`dev` labels on
        // a kind that never searches — a `reproduction`/`equivalence` spec
        // must not run over `search`-admissible labels it would contaminate.
        if self.design.kind == DesignKind::AdaptiveSearch {
            for l in &self.suite.split_labels_used {
                if !matches!(l, SplitLabel::Search | SplitLabel::Dev) {
                    return Err(ExperimentRefusal::LeakedSplit {
                        detail: format!("adaptive design over `{}`", l.name()),
                    });
                }
            }
        }
        Ok(())
    }

    fn check_arms(&self, ctx: &SpecContext<'_>) -> Result<(), ExperimentRefusal> {
        let factor_names: BTreeSet<&str> = self.factors.iter().map(|f| f.name.as_str()).collect();
        for arm in &self.arms {
            // `UnbudgetedArm` — `eval_budget` always mandatory; `search_budget`
            // mandatory on matched kinds (exploratory arms may be search-less;
            // a product-level-only arm's absence is still flagged by the
            // engine — the schema half flags the missing eval budget and the
            // missing search budget on matched kinds).
            if arm.eval_budget.is_empty()
                || (self.kind.requires_match() && arm.search_budget.is_none())
            {
                return Err(ExperimentRefusal::UnbudgetedArm {
                    arm: arm.arm_id.clone(),
                });
            }
            // `MissingMatchSpec` — matched kinds require it on every arm.
            if self.kind.requires_match() && arm.match_spec.is_none() {
                return Err(ExperimentRefusal::MissingMatchSpec {
                    arm: arm.arm_id.clone(),
                });
            }
            // `UnsealedArtifact` — the pinned-id member check, plus the
            // context's view when it can resolve.
            if !is_pinned(&arm.artifact_ref.version_id) {
                return Err(ExperimentRefusal::UnsealedArtifact {
                    artifact_ref: arm.artifact_ref.version_id.clone(),
                });
            }
            if let Some(sealed) = ctx.artifact_sealed {
                if !sealed(&arm.artifact_ref.version_id) {
                    return Err(ExperimentRefusal::UnsealedArtifact {
                        artifact_ref: arm.artifact_ref.version_id.clone(),
                    });
                }
            }
            // `level_assignment` names known factors/levels and assigns a
            // non_portable level only where the profile is not the varied
            // factor (ADR-0154 D5).
            let mut hosted_level = false;
            for (fname, lid) in &arm.level_assignment {
                let f = self
                    .factors
                    .iter()
                    .find(|f| &f.name == fname)
                    .ok_or_else(|| ExperimentRefusal::InadmissibleFactor {
                        factor: fname.clone(),
                        reason: format!("arm `{}` assigns an undeclared factor", arm.arm_id),
                    })?;
                let level = f
                    .levels
                    .iter()
                    .find(|l| &l.level_id == lid)
                    .ok_or_else(|| ExperimentRefusal::InadmissibleFactor {
                        factor: fname.clone(),
                        reason: format!("arm `{}` assigns undeclared level `{lid}`", arm.arm_id),
                    })?;
                hosted_level = hosted_level || level.class == ParticipantClass::Hosted;
                if level.non_portable && f.kind == FactorKind::Harness {
                    // A pinned-profile level may not be the *varied* harness
                    // factor level — ADR-0154 D5's confinement.
                    return Err(ExperimentRefusal::ProfilePinnedAcrossProfiles {
                        detail: format!(
                            "arm `{}` varies a non_portable level `{}` on factor `{}`",
                            arm.arm_id, lid, fname
                        ),
                    });
                }
                if let Some(drifted) = ctx.capability_drifted {
                    if drifted(&level.ref_) {
                        return Err(ExperimentRefusal::DependsOnDriftedCapability {
                            capability: level.ref_.clone(),
                        });
                    }
                }
            }
            // every assigned factor name is declared — checked above; also
            // refuse assignments to factor names not in the spec at all.
            for fname in arm.level_assignment.keys() {
                if !factor_names.contains(fname.as_str()) {
                    return Err(ExperimentRefusal::InadmissibleFactor {
                        factor: fname.clone(),
                        reason: format!("arm `{}` assigns an undeclared factor", arm.arm_id),
                    });
                }
            }
            // `limits_enforced` honesty (§6.3 C1; ADR-0046 (d);
            // AC-R-2.10.3-3): the stamp is derived — a hosted arm claims
            // `full` only where its `budget_enforcement` map proves every
            // declared dimension enforced. A `full` claim the boundary
            // cannot prove is refused (never silently restamped, T-LCD-14);
            // `partial`/`none` claims are always admissible.
            if hosted_level && arm.limits_enforced == "full" {
                let proven_full = ctx
                    .budget_enforcement
                    .map(|enf| {
                        let e = enf(arm);
                        !e.levels.is_empty()
                            && e.levels
                                .values()
                                .all(|l| *l == hh_budget::errors::EnforcementLevel::Enforced)
                    })
                    .unwrap_or(false);
                if !proven_full {
                    return Err(ExperimentRefusal::InadmissibleFactor {
                        factor: arm.arm_id.clone(),
                        reason: "`limits_enforced = full` is unverifiable on a hosted arm \
                                 (hosted limits are reported, not enforced — the derived \
                                 stamp is `partial`/`none`)"
                            .to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    fn check_match(&self, ctx: &SpecContext<'_>) -> Result<(), ExperimentRefusal> {
        let Some(resolve) = ctx.resolve_budget else {
            return Ok(()); // bodies unresolvable at this layer — engine defers
        };
        if !self.kind.requires_match() {
            return Ok(());
        }
        // Per-arm gates on matched kinds (CC9 — matched-budget-or-refuse):
        // `mode: none` is not a match spec on a matched kind, and an
        // `iso_cost` arm needs its pinned pricing table even when it is its
        // own comparand group (a singleton group skips `validate_match`'s
        // pricing precondition — ADR-0156 D2).
        for arm in &self.arms {
            let ms = arm.match_spec.as_ref().expect("check_arms ran first");
            if ms.mode == MatchMode::None {
                return Err(ExperimentRefusal::MissingMatchSpec {
                    arm: arm.arm_id.clone(),
                });
            }
            if ms.mode == MatchMode::IsoCost && ms.pricing_table_ref.is_none() {
                return Err(ExperimentRefusal::MissingPricingTable {
                    detail: format!(
                        "arm `{}` declares iso_cost with no pricing_table_ref",
                        arm.arm_id
                    ),
                });
            }
        }
        // Arms partition into comparand groups by match mode (ADR-0156 D2 /
        // CF-334): `lab/control-strategy-family-v1` carries one `iso_cost`
        // arm beside the `matched_cap` contrast — commensurability is a
        // *within-group* precondition at register, and a cross-mode
        // `compare` is refused `IncommensurableMatch` downstream, not here.
        let mut groups: BTreeMap<MatchMode, Vec<(&ArmSpec, BudgetArmSpec)>> = BTreeMap::new();
        for arm in &self.arms {
            let eval = resolve(&arm.eval_budget);
            let search = arm.search_budget.as_deref().and_then(resolve);
            let inference = arm.inference_budget.as_deref().and_then(resolve);
            let mode = arm.match_spec.as_ref().expect("checked above").mode;
            let enforcement = ctx
                .budget_enforcement
                .map(|f| f(arm))
                .unwrap_or_else(BudgetEnforcement::native);
            groups.entry(mode).or_default().push((
                arm,
                BudgetArmSpec {
                    search_budget: search,
                    eval_budget: eval,
                    inference_budget: inference,
                    match_spec: arm.match_spec.clone(),
                    enforcement,
                    spend_confidence: None,
                    coverage_ppm: None,
                },
            ));
        }
        for group in groups.values() {
            let specs: Vec<BudgetArmSpec> = group.iter().map(|(_, s)| s.clone()).collect();
            validate_match(&specs).map_err(|e: MatchError| {
                let arm_id = |i: Option<usize>| {
                    i.and_then(|i| group.get(i))
                        .map(|(a, _)| a.arm_id.clone())
                        .unwrap_or_default()
                };
                match e.refusal {
                    MatchRefusal::MissingMatchSpec => {
                        ExperimentRefusal::MissingMatchSpec { arm: arm_id(e.arm) }
                    }
                    MatchRefusal::UnbudgetedArm => {
                        ExperimentRefusal::UnbudgetedArm { arm: arm_id(e.arm) }
                    }
                    MatchRefusal::MissingPricingTable => ExperimentRefusal::MissingPricingTable {
                        detail: format!("{e:?}"),
                    },
                    MatchRefusal::IncommensurableMatch { .. } => {
                        ExperimentRefusal::IncommensurableMatch {
                            detail: format!("{:?}", e.refusal),
                        }
                    }
                }
            })?;
        }
        // AC-R-2.10.2-12 — a `budget_relevant` parameter whose bound value
        // differs across a comparand group's arms (or binds on some arms
        // only) must be covered by the arm's `MatchSpec`: every `affects`
        // dimension it drives is a matched dimension (T-LCD-14; refusal
        // `UnmatchedBudget`, never a warning).
        if let Some(resolve_params) = ctx.budget_relevant_params {
            let level_of = |level_id: &str| -> Option<&LevelSpec> {
                self.factors
                    .iter()
                    .flat_map(|f| f.levels.iter())
                    .find(|l| l.level_id == level_id)
            };
            for group in groups.values() {
                let mut per_arm: Vec<(&ArmSpec, BoundParams)> = Vec::new();
                for (arm, _) in group {
                    let mut bound: BoundParams = BTreeMap::new();
                    for level_id in arm.level_assignment.values() {
                        let Some(level) = level_of(level_id) else {
                            continue;
                        };
                        for (name, param) in resolve_params(&level.ref_) {
                            // A `LevelSpec.overrides` member rebinds the
                            // declared value at this level.
                            let value = level
                                .overrides
                                .as_ref()
                                .and_then(|o| o.get(&name).cloned())
                                .unwrap_or(param.value);
                            let entry = bound
                                .entry(name)
                                .or_insert_with(|| (value.clone(), BTreeSet::new()));
                            entry.1.extend(param.affects);
                        }
                    }
                    per_arm.push((*arm, bound));
                }
                let names: BTreeSet<String> = per_arm
                    .iter()
                    .flat_map(|(_, p)| p.keys().cloned())
                    .collect();
                for name in names {
                    // The parameter "differs across arms" when the bound
                    // values disagree — or it binds on some arms only.
                    let distinct: BTreeSet<String> = per_arm
                        .iter()
                        .map(|(_, p)| {
                            p.get(&name)
                                .map(|(v, _)| v.to_canonical_string())
                                .unwrap_or_else(|| "(absent)".to_string())
                        })
                        .collect();
                    if distinct.len() <= 1 {
                        continue;
                    }
                    for (arm, params) in &per_arm {
                        let Some((_, affects)) = params.get(&name) else {
                            continue;
                        };
                        let ms = arm.match_spec.as_ref().expect("checked above");
                        for d in affects {
                            if !ms.dimensions.contains(d) {
                                return Err(ExperimentRefusal::UnmatchedBudget {
                                    detail: format!(
                                        "arm `{}` MatchSpec omits budget_relevant parameter `{name}` (drives dimension `{}`) that differs across arms",
                                        arm.arm_id,
                                        d.as_str()
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// The canonical JSON (`hh-experiment/1`).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("experiment_id".into(), Json::str(&self.experiment_id));
        m.insert("kind".into(), Json::str(self.kind.name()));
        m.insert("design".into(), self.design.to_json());
        if let Some(p) = &self.pre_registration {
            m.insert("pre_registration".into(), p.to_json());
        }
        m.insert(
            "factors".into(),
            Json::Arr(self.factors.iter().map(FactorSpec::to_json).collect()),
        );
        m.insert(
            "arms".into(),
            Json::Arr(self.arms.iter().map(ArmSpec::to_json).collect()),
        );
        let mut suite = BTreeMap::new();
        suite.insert("suite_ref".into(), Json::str(&self.suite.suite_ref));
        suite.insert(
            "split_labels_used".into(),
            Json::Arr(
                self.suite
                    .split_labels_used
                    .iter()
                    .map(|l| Json::str(l.name()))
                    .collect(),
            ),
        );
        if let Some(s) = &self.suite.split_assignment_ref {
            suite.insert("split_assignment_ref".into(), Json::str(s));
        }
        m.insert("suite".into(), Json::Obj(suite));
        m.insert(
            "replicates_per_cell".into(),
            Json::Int(self.replicates_per_cell as i64),
        );
        m.insert("seed_policy".into(), self.seed_policy.to_json());
        m.insert(
            "validation_strategy".into(),
            self.validation_strategy.to_json(),
        );
        m.insert("scheduling".into(), Self::scheduling_json(&self.scheduling));
        m.insert("reattempt".into(), Self::reattempt_json(&self.reattempt));
        m.insert(
            "budgets".into(),
            Json::obj([
                ("experiment", Json::str(&self.budgets.experiment)),
                ("instrument", Json::str(&self.budgets.instrument)),
            ]),
        );
        m.insert("bundle_policy".into(), self.bundle_policy.to_json());
        if !self.ext.is_empty() {
            m.insert("ext".into(), Json::Obj(self.ext.clone()));
        }
        Json::Obj(m)
    }

    fn scheduling_json(s: &SchedulingPolicy) -> Json {
        let mut m = BTreeMap::new();
        m.insert(
            "max_concurrent_runs".into(),
            Json::Int(s.max_concurrent_runs as i64),
        );
        m.insert(
            "pools".into(),
            Json::Arr(
                s.pools
                    .iter()
                    .map(|p| {
                        Json::obj([
                            ("key", Json::str(&p.key)),
                            ("limit", Json::Int(p.limit as i64)),
                        ])
                    })
                    .collect(),
            ),
        );
        m.insert("order".into(), Json::str(s.order.name()));
        m.insert("permutation_seed".into(), Json::str(&s.permutation_seed));
        m.insert(
            "start_stagger_ms".into(),
            Json::Int(s.start_stagger_ms as i64),
        );
        if let Some(d) = s.deadline {
            m.insert("deadline".into(), Json::Int(d as i64));
        }
        if let Some(p) = &s.priority {
            m.insert("priority".into(), Json::str(p));
        }
        Json::Obj(m)
    }

    fn reattempt_json(r: &ReattemptPolicy) -> Json {
        let mut m = BTreeMap::new();
        m.insert("max_per_plan".into(), Json::Int(r.max_per_plan as i64));
        m.insert(
            "max_fraction_of_plans".into(),
            Json::Int(r.max_fraction_of_plans_ppm as i64),
        );
        m.insert(
            "backoff".into(),
            Json::obj([
                ("min_ms", Json::Int(r.backoff.min_ms as i64)),
                ("multiplier", Json::Int(r.backoff.multiplier_ppm as i64)),
                ("max_ms", Json::Int(r.backoff.max_ms as i64)),
            ]),
        );
        if let Some(e) = &r.error_classes_included {
            m.insert(
                "error_classes_included".into(),
                Json::Arr(e.iter().map(Json::str).collect()),
            );
        }
        m.insert("on_cancel".into(), Json::str(r.on_cancel.name()));
        Json::Obj(m)
    }

    /// Strict decode (`hh-experiment/1`; `experiment_id` re-verified by
    /// [`ExperimentSpec::check_id`]).
    pub fn from_json(j: &Json) -> Result<ExperimentSpec, ExperimentRefusal> {
        const REC: &str = "ExperimentSpec";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "experiment_id",
                "kind",
                "design",
                "pre_registration",
                "factors",
                "arms",
                "suite",
                "replicates_per_cell",
                "seed_policy",
                "validation_strategy",
                "scheduling",
                "reattempt",
                "budgets",
                "bundle_policy",
                "ext",
            ],
            REC,
        )?;
        let suite = expect_obj(member_at(m, "suite", REC)?, "SuiteBinding")?;
        reject_unknown(
            suite,
            &["suite_ref", "split_labels_used", "split_assignment_ref"],
            "SuiteBinding",
        )?;
        let budgets = expect_obj(member_at(m, "budgets", REC)?, "budgets")?;
        reject_unknown(budgets, &["experiment", "instrument"], "budgets")?;
        let scheduling = expect_obj(member_at(m, "scheduling", REC)?, "SchedulingPolicy")?;
        reject_unknown(
            scheduling,
            &[
                "max_concurrent_runs",
                "pools",
                "order",
                "permutation_seed",
                "start_stagger_ms",
                "deadline",
                "priority",
            ],
            "SchedulingPolicy",
        )?;
        let reattempt = expect_obj(member_at(m, "reattempt", REC)?, "ReattemptPolicy")?;
        reject_unknown(
            reattempt,
            &[
                "max_per_plan",
                "max_fraction_of_plans",
                "backoff",
                "error_classes_included",
                "on_cancel",
            ],
            "ReattemptPolicy",
        )?;
        let backoff = expect_obj(
            member_at(reattempt, "backoff", "ReattemptPolicy")?,
            "Backoff",
        )?;
        reject_unknown(backoff, &["min_ms", "multiplier", "max_ms"], "Backoff")?;
        let ext = match m.get("ext") {
            None | Some(Json::Null) => BTreeMap::new(),
            Some(Json::Obj(e)) => e.clone(),
            Some(_) => {
                return Err(ExperimentRefusal::Schema(SchemaError::v(
                    "ext",
                    "must be an object",
                )))
            }
        };
        let pools = arr_at(scheduling, "pools", "SchedulingPolicy")?
            .iter()
            .map(|p| {
                let pm = expect_obj(p, "Pool")?;
                reject_unknown(pm, &["key", "limit"], "Pool")?;
                Ok(Pool {
                    key: str_at(pm, "key", "Pool")?.to_string(),
                    limit: int_at(pm, "limit", "Pool")? as u32,
                })
            })
            .collect::<Result<Vec<Pool>, SchemaError>>()?;
        Ok(ExperimentSpec {
            experiment_id: str_at(m, "experiment_id", REC)?.to_string(),
            kind: ExperimentKind::parse(str_at(m, "kind", REC)?).ok_or_else(|| {
                ExperimentRefusal::Schema(SchemaError::v("kind", "unknown experiment kind"))
            })?,
            design: Design::from_json(member_at(m, "design", REC)?).map_err(|e| {
                ExperimentRefusal::Schema(SchemaError::v("design", format!("{e:?}")))
            })?,
            pre_registration: match m.get("pre_registration") {
                None | Some(Json::Null) => None,
                Some(p) => Some(PreRegistration::from_json(p).map_err(|e| {
                    ExperimentRefusal::Schema(SchemaError::v("pre_registration", format!("{e:?}")))
                })?),
            },
            factors: enum_vec_at(m, "factors", REC, |f| FactorSpec::from_json(f).ok())?,
            arms: enum_vec_at(m, "arms", REC, |a| ArmSpec::from_json(a).ok())?,
            suite: SuiteBinding {
                suite_ref: str_at(suite, "suite_ref", "SuiteBinding")?.to_string(),
                split_labels_used: str_vec_at(suite, "split_labels_used", "SuiteBinding")?
                    .iter()
                    .map(|s| {
                        SplitLabel::parse(s).ok_or_else(|| {
                            SchemaError::v("split_labels_used", format!("unknown `{s}`"))
                        })
                    })
                    .collect::<Result<Vec<SplitLabel>, SchemaError>>()?,
                split_assignment_ref: opt_str_at(suite, "split_assignment_ref")?
                    .map(str::to_string),
            },
            replicates_per_cell: int_at(m, "replicates_per_cell", REC)? as u32,
            seed_policy: SeedPolicy::from_json(member_at(m, "seed_policy", REC)?).map_err(|e| {
                ExperimentRefusal::Schema(SchemaError::v("seed_policy", format!("{e:?}")))
            })?,
            validation_strategy: ValidationStrategy::from_json(member_at(
                m,
                "validation_strategy",
                REC,
            )?)?,
            scheduling: SchedulingPolicy {
                max_concurrent_runs: int_at(scheduling, "max_concurrent_runs", "SchedulingPolicy")?
                    as u32,
                pools,
                order: OrderKind::parse(str_at(scheduling, "order", "SchedulingPolicy")?)
                    .ok_or_else(|| SchemaError::v("order", "unknown order kind"))?,
                permutation_seed: str_at(scheduling, "permutation_seed", "SchedulingPolicy")?
                    .to_string(),
                start_stagger_ms: int_at(scheduling, "start_stagger_ms", "SchedulingPolicy")?
                    as u64,
                deadline: opt_int_at(scheduling, "deadline")?.map(|v| v as u64),
                priority: opt_str_at(scheduling, "priority")?.map(str::to_string),
            },
            reattempt: ReattemptPolicy {
                max_per_plan: int_at(reattempt, "max_per_plan", "ReattemptPolicy")? as u32,
                max_fraction_of_plans_ppm: int_at(
                    reattempt,
                    "max_fraction_of_plans",
                    "ReattemptPolicy",
                )? as u64,
                backoff: Backoff {
                    min_ms: int_at(backoff, "min_ms", "Backoff")? as u64,
                    multiplier_ppm: int_at(backoff, "multiplier", "Backoff")? as u64,
                    max_ms: int_at(backoff, "max_ms", "Backoff")? as u64,
                },
                error_classes_included: match opt_arr_at(reattempt, "error_classes_included")? {
                    None => None,
                    Some(a) => Some(
                        a.iter()
                            .map(|j| {
                                j.as_str().map(str::to_string).ok_or_else(|| {
                                    SchemaError::v(
                                        "error_classes_included",
                                        "entries must be strings",
                                    )
                                })
                            })
                            .collect::<Result<Vec<String>, SchemaError>>()?,
                    ),
                },
                on_cancel: CancelPolicy::parse(str_at(reattempt, "on_cancel", "ReattemptPolicy")?)
                    .ok_or_else(|| SchemaError::v("on_cancel", "unknown cancel policy"))?,
            },
            budgets: ExperimentBudgets {
                experiment: str_at(budgets, "experiment", "budgets")?.to_string(),
                instrument: str_at(budgets, "instrument", "budgets")?.to_string(),
            },
            bundle_policy: BundlePolicy::from_json(member_at(m, "bundle_policy", REC)?)?,
            ext,
        })
    }

    /// Whether `experiment_id` equals `H(canonical(spec minus experiment_id))`.
    pub fn check_id(&self) -> bool {
        self.experiment_id == self.experiment_id()
    }
}

// ── CellPlan / RunPlan / OrderPlan ──────────────────────────────────────────

/// `run_plan_id = H(experiment_id ∥ arm_id ∥ configuration_version_id ∥
/// task_id ∥ replicate_index)` — scheduling and transport fields never enter
/// it (§6.3 `expand`; CF-108: `bundle_id` never enters it).
pub fn run_plan_id(
    experiment_id: &str,
    arm_id: &str,
    configuration_version_id: &str,
    task_id: &str,
    replicate_index: u32,
) -> String {
    let parts = Json::Arr(vec![
        Json::str(experiment_id),
        Json::str(arm_id),
        Json::str(configuration_version_id),
        Json::str(task_id),
        Json::Int(replicate_index as i64),
    ]);
    idp_id(
        "experiment.run_plan",
        parts.to_canonical_string().as_bytes(),
    )
}

/// `Cell{cell_id, arm_id, configuration_id, configuration_version_id,
/// task_id, split_label, na_reason?}` — one planned cell (§6.3 `expand`).
/// `na_reason` carries the typed `n/a{reason}` (`hh_ontology::compliance::
/// NaReason`) for a cell the plan knows is ineligible before any run opens —
/// e.g. `capability` for a cell whose level requires a capability the pinned
/// `registry_snapshot_id` does not supply (§6.3's `slot_choices`-floor check;
/// T-LCD-15 — a typed `n/a`, never a skipped row). A `na_reason` cell is
/// planned but never scheduled.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanCell {
    /// The cell id (plan-local).
    pub cell_id: String,
    /// The arm id.
    pub arm_id: String,
    /// The configuration id (ADR-0036's seedless id).
    pub configuration_id: String,
    /// The configuration's exact-bytes version id.
    pub configuration_version_id: String,
    /// The task id.
    pub task_id: String,
    /// The task's split label.
    pub split_label: SplitLabel,
    /// The typed `n/a{reason}` for a planned-but-ineligible cell.
    pub na_reason: Option<hh_ontology::compliance::NaReason>,
}

/// `environment_derivation` — how the run's environment derives (§6.3:
/// `fresh_from_image` is the C0 value; `fork_snapshot` arrives with the
/// boundary companion, C2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EnvironmentDerivation {
    /// Provision a fresh environment from the image.
    FreshFromImage,
    /// Fork from a snapshot (C2 — reserved; decode accepts it so the dialect
    /// is forward-compatible, register refuses it at C0).
    ForkSnapshot,
}

impl EnvironmentDerivation {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            EnvironmentDerivation::FreshFromImage => "fresh_from_image",
            EnvironmentDerivation::ForkSnapshot => "fork_snapshot",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<EnvironmentDerivation> {
        match s {
            "fresh_from_image" => Some(EnvironmentDerivation::FreshFromImage),
            "fork_snapshot" => Some(EnvironmentDerivation::ForkSnapshot),
            _ => None,
        }
    }
}

/// `RunPlan{run_plan_id, cell_id, replicate_index, seed_material,
/// environment_derivation = fresh_from_image, cache_scope_salt}` (§6.3
/// `expand`; S-2: at most one accepted run per `run_plan_id`).
#[derive(Debug, Clone, PartialEq)]
pub struct RunPlan {
    /// `run_plan_id` — see [`run_plan_id`].
    pub run_plan_id: String,
    /// The cell this run belongs to.
    pub cell_id: String,
    /// The replicate index within the cell.
    pub replicate_index: u32,
    /// The seed material (the replicate's seed tuple — data at C0).
    pub seed_material: Json,
    /// The environment derivation (`fresh_from_image` at C0).
    pub environment_derivation: EnvironmentDerivation,
    /// The cache-scope salt (ADR-0128's `cold_start` partitioning).
    pub cache_scope_salt: String,
}

/// `OrderPlan{permutation_seed, blocks}` — the reproducible run order
/// (S-6: order is reproducible from `permutation_seed`).
#[derive(Debug, Clone, PartialEq)]
pub struct OrderPlan {
    /// The permutation seed.
    pub permutation_seed: String,
    /// The replicate blocks (`block_id` list).
    pub blocks: Vec<String>,
}

/// `CellPlan{plan_id, cells[], run_plans[], order, generators?,
/// aliasing_table?}` — `expand(spec)`'s output (§6.3; pure and total;
/// content-addressed under `cell_plan`).
#[derive(Debug, Clone, PartialEq)]
pub struct CellPlan {
    /// `plan_id = H(canonical(plan))` under the `cell_plan` domain.
    pub plan_id: String,
    /// The planned cells.
    pub cells: Vec<PlanCell>,
    /// The planned runs.
    pub run_plans: Vec<RunPlan>,
    /// The order plan.
    pub order: OrderPlan,
    /// The fractional-factorial generators (when `design.kind =
    /// fractional_factorial`).
    pub generators: Option<Vec<String>>,
    /// The aliasing table (part of the plan for fractional designs).
    pub aliasing_table: Option<Json>,
}

impl CellPlan {
    /// `plan_id = H(canonical(plan minus plan_id))` under the `cell_plan`
    /// domain.
    pub fn plan_id(&self) -> String {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("plan_id");
        }
        identify_bytes(RecordKind::CellPlan, j.to_canonical_string().as_bytes())
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("plan_id".into(), Json::str(&self.plan_id));
        m.insert(
            "cells".into(),
            Json::Arr(
                self.cells
                    .iter()
                    .map(|c| {
                        let mut cm = BTreeMap::from([
                            ("cell_id".to_string(), Json::str(&c.cell_id)),
                            ("arm_id".to_string(), Json::str(&c.arm_id)),
                            (
                                "configuration_id".to_string(),
                                Json::str(&c.configuration_id),
                            ),
                            (
                                "configuration_version_id".to_string(),
                                Json::str(&c.configuration_version_id),
                            ),
                            ("task_id".to_string(), Json::str(&c.task_id)),
                            ("split_label".to_string(), Json::str(c.split_label.name())),
                        ]);
                        if let Some(r) = c.na_reason {
                            cm.insert("na_reason".to_string(), Json::str(r.as_str()));
                        }
                        Json::Obj(cm)
                    })
                    .collect(),
            ),
        );
        m.insert(
            "run_plans".into(),
            Json::Arr(
                self.run_plans
                    .iter()
                    .map(|r| {
                        Json::obj([
                            ("run_plan_id", Json::str(&r.run_plan_id)),
                            ("cell_id", Json::str(&r.cell_id)),
                            ("replicate_index", Json::Int(r.replicate_index as i64)),
                            ("seed_material", r.seed_material.clone()),
                            (
                                "environment_derivation",
                                Json::str(r.environment_derivation.name()),
                            ),
                            ("cache_scope_salt", Json::str(&r.cache_scope_salt)),
                        ])
                    })
                    .collect(),
            ),
        );
        m.insert(
            "order".into(),
            Json::obj([
                ("permutation_seed", Json::str(&self.order.permutation_seed)),
                (
                    "blocks",
                    Json::Arr(self.order.blocks.iter().map(Json::str).collect()),
                ),
            ]),
        );
        if let Some(g) = &self.generators {
            m.insert(
                "generators".into(),
                Json::Arr(g.iter().map(Json::str).collect()),
            );
        }
        if let Some(a) = &self.aliasing_table {
            m.insert("aliasing_table".into(), a.clone());
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<CellPlan, SchemaError> {
        const REC: &str = "CellPlan";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "plan_id",
                "cells",
                "run_plans",
                "order",
                "generators",
                "aliasing_table",
            ],
            REC,
        )?;
        let cells = arr_at(m, "cells", REC)?
            .iter()
            .map(|c| {
                let cm = expect_obj(c, "PlanCell")?;
                reject_unknown(
                    cm,
                    &[
                        "cell_id",
                        "arm_id",
                        "configuration_id",
                        "configuration_version_id",
                        "task_id",
                        "split_label",
                        "na_reason",
                    ],
                    "PlanCell",
                )?;
                Ok(PlanCell {
                    cell_id: str_at(cm, "cell_id", "PlanCell")?.to_string(),
                    arm_id: str_at(cm, "arm_id", "PlanCell")?.to_string(),
                    configuration_id: str_at(cm, "configuration_id", "PlanCell")?.to_string(),
                    configuration_version_id: str_at(cm, "configuration_version_id", "PlanCell")?
                        .to_string(),
                    task_id: str_at(cm, "task_id", "PlanCell")?.to_string(),
                    split_label: SplitLabel::parse(str_at(cm, "split_label", "PlanCell")?)
                        .ok_or_else(|| SchemaError::v("split_label", "unknown split label"))?,
                    na_reason: opt_str_at(cm, "na_reason")?
                        .map(|s| {
                            hh_ontology::compliance::NaReason::parse(s).ok_or_else(|| {
                                SchemaError::v("na_reason", format!("unknown `n/a` reason `{s}`"))
                            })
                        })
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<PlanCell>, SchemaError>>()?;
        let run_plans = arr_at(m, "run_plans", REC)?
            .iter()
            .map(|r| {
                let rm = expect_obj(r, "RunPlan")?;
                reject_unknown(
                    rm,
                    &[
                        "run_plan_id",
                        "cell_id",
                        "replicate_index",
                        "seed_material",
                        "environment_derivation",
                        "cache_scope_salt",
                    ],
                    "RunPlan",
                )?;
                Ok(RunPlan {
                    run_plan_id: str_at(rm, "run_plan_id", "RunPlan")?.to_string(),
                    cell_id: str_at(rm, "cell_id", "RunPlan")?.to_string(),
                    replicate_index: int_at(rm, "replicate_index", "RunPlan")? as u32,
                    seed_material: member_at(rm, "seed_material", "RunPlan")?.clone(),
                    environment_derivation: EnvironmentDerivation::parse(str_at(
                        rm,
                        "environment_derivation",
                        "RunPlan",
                    )?)
                    .ok_or_else(|| {
                        SchemaError::v("environment_derivation", "unknown derivation")
                    })?,
                    cache_scope_salt: str_at(rm, "cache_scope_salt", "RunPlan")?.to_string(),
                })
            })
            .collect::<Result<Vec<RunPlan>, SchemaError>>()?;
        let order = expect_obj(member_at(m, "order", REC)?, "OrderPlan")?;
        reject_unknown(order, &["permutation_seed", "blocks"], "OrderPlan")?;
        Ok(CellPlan {
            plan_id: str_at(m, "plan_id", REC)?.to_string(),
            cells,
            run_plans,
            order: OrderPlan {
                permutation_seed: str_at(order, "permutation_seed", "OrderPlan")?.to_string(),
                blocks: str_vec_at(order, "blocks", "OrderPlan")?,
            },
            generators: match opt_arr_at(m, "generators")? {
                None => None,
                Some(a) => Some(
                    a.iter()
                        .map(|j| {
                            j.as_str().map(str::to_string).ok_or_else(|| {
                                SchemaError::v("generators", "entries must be strings")
                            })
                        })
                        .collect::<Result<Vec<String>, SchemaError>>()?,
                ),
            },
            aliasing_table: m
                .get("aliasing_table")
                .filter(|j| !matches!(j, Json::Null))
                .cloned(),
        })
    }

    /// Whether `plan_id` equals `H(canonical(plan minus plan_id))`.
    pub fn check_id(&self) -> bool {
        self.plan_id == self.plan_id()
    }
}
