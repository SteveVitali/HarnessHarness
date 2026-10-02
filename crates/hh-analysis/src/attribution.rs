//! `attribution` — the designed causal-attribution machinery of §5h.7
//! (R-2.9.7, C4/Stage-6 slice; ticket S6.3b; ADR-0199…0201): M2
//! trajectory-local contrastive, M3 coalition credit, M4 paired-replay
//! interaction and M5 mediation-split estimation over *executed*
//! counterfactual arms, plus `locus`, `estimate_rollouts`,
//! `attribution_quality`, `intervention_for`, the `AttributionDesign`
//! codec and the V1–V12 refusal surface.
//!
//! Records-in / records-out like every §6.4 kernel half: the *plan* is a
//! list of `ArmCell`s the boundary executes through `hh-embed/1` Group W
//! `counterfactual` (every branch `branch_kind = counterfactual`,
//! `charged_to = instrument`); the *outcomes* come back as `ArmOutcome`
//! records (the driver — host executor or the hermetic known-effect
//! suite — produces them; the kernel never fabricates one). `attribute`
//! folds the joined cells into an `AttributionReport/1`; the report is
//! pure over `(design, cells, seed)` — identical inputs mint an
//! identical `report_id` (I5, KA-I7-11).
//!
//! Estimand floors (ADR-0199 D2): `LOI/LOO` suffice at `re_executed`;
//! `TE_marg`/`shapley` need `re_executed` + a coherent fork point;
//! `TE_crn`/`DE`/`ME` need `deterministic` + `noise_coupling = crn` on
//! every post-fork nondeterminism source (V10 — declared
//! `coupled_sources[]` with measured agreement, never assumed).
//! `TE_marg` alone never supports `confirmatory`; `degraded` ⇒
//! `exploratory`; `invalid` ⇒ `ReplayInvalid`.

use std::collections::{BTreeMap, BTreeSet};

use hh_eval::stats::{self, Interval, XorShift64, PPM};
use hh_identity::idp_id;
use hh_lab::json_util::{
    bool_at, expect_obj, int_at, member_at, opt_bool_at, opt_int_at, opt_str_at, reject_unknown,
    str_at, SchemaError,
};
use hh_wire::json::Json;

/// `hh-attribution-design/1` — the design document spelling (M1's S5.4
/// producer uses the same schema tag; the M2–M5 members are additive).
pub const DESIGN_SCHEMA: &str = "hh-attribution-design/1";
/// `hh-attribution/1` — the report spelling (M1's S5.4 rows carry the
/// same tag; the M2–M5 members are additive — CC8).
pub const REPORT_SCHEMA: &str = "hh-attribution/1";
/// `hh-attribution-hypothesis/1` — the M0 record.
pub const HYPOTHESIS_SCHEMA: &str = "hh-attribution-hypothesis/1";

/// The coalition `target_cap` (OQ-450 MUST-data placeholder —
/// ADR-0200 D3 / ADR-0214: unruled until the §05e pricing rule lands).
pub const TARGET_CAP: usize = 8;
/// OQ-323 k floors (MUST-data placeholders — ADR-0199 D6; OQ-448's Stage
/// 6 power re-check is owed by A13 on the Stage-3 suite).
pub const K_REEXECUTED: u32 = 8;
/// `deterministic` without coupling.
pub const K_DETERMINISTIC: u32 = 4;
/// `deterministic` + `crn` — the decision-sequence claim.
pub const K_CRN_SEQUENCE: u32 = 1;
/// `deterministic` + `crn` — outcome claims.
pub const K_CRN_OUTCOME: u32 = 4;
/// The nominal interval level (95 %).
pub const CONFIDENCE_PPM: i64 = 950_000;

/// The `ComponentTarget.kind` closed set (§5h.7 §3).
pub const TARGET_KINDS: &[&str] = &[
    "slot",
    "variant",
    "rule",
    "parameter",
    "procedure",
    "text_leaf",
    "decision_point",
];
/// The method catalogue (closed — growth by dialect bump; ADR-0200 D1).
pub const METHODS: &[&str] = &["M1", "M2", "M3", "M4", "M5"];
/// The `fork_policy` closed set (`every_decision_point{kind}` encodes as
/// `every_decision_point` + `fork_kind`).
pub const FORK_POLICIES: &[&str] = &[
    "latest_coherent_before",
    "run_start",
    "every_decision_point",
];
/// `noise_coupling` (exogenous-draw coupling — CF-432; distinct from
/// `coupling_assumption`'s edit/side-effect coupling — CF-435).
pub const NOISE_COUPLINGS: &[&str] = &["none", "crn"];
/// `coupling_assumption` — the intervention's edit/side-effect coupling.
pub const COUPLING_ASSUMPTIONS: &[&str] = &["weak", "declared_strong"];
/// The replay modes a design may request.
pub const REPLAY_MODES: &[&str] = &["re_executed", "deterministic"];
/// The `claim_kind` closed set — OQ-323's k rule keys on it
/// (`decision_sequence` admits k = 1 under `deterministic` + `crn`).
pub const CLAIM_KINDS: &[&str] = &["outcome", "decision_sequence"];
/// The report `label` closed set.
pub const REPORT_LABELS: &[&str] = &["confirmatory", "exploratory", "preview"];
/// The `attribution_label` closed set (the R-2.9.5 label gate's ladder —
/// `designed_ablation < causal_interventional < causal_coupled`).
pub const ATTRIBUTION_LABELS: &[&str] = &[
    "designed_ablation",
    "causal_interventional",
    "causal_coupled",
];
/// The intervention kinds `intervention_for` may emit (ADR-0135's closed
/// sum — no new kernel intervention kind; ADR-0199 D3).
pub const INTERVENTION_KINDS: &[&str] = &[
    "definition_diff",
    "profile_diff",
    "response_substitution",
    "observation_substitution",
    "decision_override",
    "budget_change",
    "permission_change",
];

// ── errors ──────────────────────────────────────────────────────────────────

/// The V1–V12 + plan refusal surface (typed — never a warning;
/// ADR-0200 D2).
#[derive(Debug, Clone, PartialEq)]
pub enum AttributionError {
    /// The design/outcome record failed strict decode.
    Schema(String),
    /// `method` outside the closed catalogue.
    UnknownMethod(String),
    /// `fork_policy` outside the closed set.
    UnknownForkPolicy(String),
    /// The requested replay mode sits below the method's estimand floor
    /// (V1).
    EstimandBelowFloor {
        /// The method.
        method: String,
        /// The floor's spelling.
        floor: String,
    },
    /// M3's coalition size exceeds `target_cap` (OQ-450 placeholder 8).
    TooManyTargets {
        /// The requested count.
        n: usize,
        /// The cap.
        cap: usize,
    },
    /// `k` below the replay mode's floor (OQ-323).
    Underpowered {
        /// Declared k.
        k: u32,
        /// The floor for the mode.
        required: u32,
    },
    /// No `match` member (V3 — one MatchSpec, always).
    MissingMatchSpec,
    /// The match declaration is not a single matched-budget spec.
    UnmatchedBudget {
        /// Detail.
        detail: String,
    },
    /// An arm lacks its budget slice.
    UnbudgetedArm,
    /// V4 — `change_rate = 0` at the named fork point (nothing is
    /// estimable there).
    PolicyCollapsed {
        /// The collapsed fork point.
        seq: i64,
    },
    /// V10 — `TE_crn`/`DE`/`ME` requested while a consumed post-fork
    /// source is not in `coupled_sources[]` with measured agreement.
    CouplingUnavailable {
        /// The uncoupled source.
        source: String,
    },
    /// V9 — a hosted row named a component-level target outside the
    /// `observation_substitution`/`configuration`/`model_io` exception.
    ClassInadmissible {
        /// Detail.
        detail: String,
    },
    /// The outcome metric names no deterministic-oracle declaration (V5).
    OutcomeOracleUndeclared {
        /// The outcome ref.
        outcome: String,
    },
    /// The walk's spend would exceed the reservation (I4 — refuse at the
    /// plan boundary; `budget_truncated` is the partial-report path).
    BudgetExceeded {
        /// The reservation.
        reserved: i64,
        /// The requested spend.
        requested: i64,
    },
    /// A branch's replay validity is `invalid` (V1).
    ReplayInvalid {
        /// The recorded reason.
        reason: String,
    },
    /// The intervention's `earliest_affected_seq` precedes the requested
    /// fork point (V2 — fork must sit ≤ it).
    InterventionPrecedesForkPoint {
        /// The requested fork.
        at: i64,
        /// The earliest affected seq.
        earliest: i64,
    },
    /// No coherent fork point exists at/below the requested cut (V2 —
    /// e.g. inside an open model-call scope).
    NoCoherentForkPoint {
        /// The requested fork.
        point: i64,
    },
    /// `intervention_for` was handed a widening/loosening diff (§03
    /// ADR-0017; the governance half of AC-R-2.9.7-14).
    AuthorityWidening,
    /// The budget half of the same refusal.
    BudgetLoosening,
    /// `targets[]` is empty where the method needs ≥ 1.
    MissingTargets,
    /// A cell named a `(fork_point, target)` outside the plan, or a
    /// required member is absent/mistyped.
    CellOutOfPlan {
        /// The offending key.
        key: String,
    },
}

impl std::fmt::Display for AttributionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttributionError::Schema(d) => write!(f, "SchemaViolation: {d}"),
            AttributionError::UnknownMethod(m) => write!(f, "UnknownMethod: {m}"),
            AttributionError::UnknownForkPolicy(p) => write!(f, "UnknownForkPolicy: {p}"),
            AttributionError::EstimandBelowFloor { method, floor } => {
                write!(f, "EstimandBelowFloor: {method} needs ≥ {floor}")
            }
            AttributionError::TooManyTargets { n, cap } => {
                write!(f, "TooManyTargets: {n} > target_cap {cap}")
            }
            AttributionError::Underpowered { k, required } => {
                write!(f, "Underpowered: k {k} < {required}")
            }
            AttributionError::MissingMatchSpec => write!(f, "MissingMatchSpec"),
            AttributionError::UnmatchedBudget { detail } => {
                write!(f, "UnmatchedBudget: {detail}")
            }
            AttributionError::UnbudgetedArm => write!(f, "UnbudgetedArm"),
            AttributionError::PolicyCollapsed { seq } => {
                write!(f, "PolicyCollapsed: fork {seq}")
            }
            AttributionError::CouplingUnavailable { source } => {
                write!(f, "CouplingUnavailable: {source}")
            }
            AttributionError::ClassInadmissible { detail } => {
                write!(f, "ClassInadmissible: {detail}")
            }
            AttributionError::OutcomeOracleUndeclared { outcome } => {
                write!(f, "OutcomeOracleUndeclared: {outcome}")
            }
            AttributionError::BudgetExceeded { reserved, requested } => {
                write!(f, "BudgetExceeded: requested {requested} > reserved {reserved}")
            }
            AttributionError::ReplayInvalid { reason } => {
                write!(f, "ReplayInvalid: {reason}")
            }
            AttributionError::InterventionPrecedesForkPoint { at, earliest } => write!(
                f,
                "InterventionPrecedesForkPoint: at {at} > earliest_affected {earliest}"
            ),
            AttributionError::NoCoherentForkPoint { point } => {
                write!(f, "NoCoherentForkPoint: {point}")
            }
            AttributionError::AuthorityWidening => write!(f, "AuthorityWidening"),
            AttributionError::BudgetLoosening => write!(f, "BudgetLoosening"),
            AttributionError::MissingTargets => write!(f, "MissingTargets"),
            AttributionError::CellOutOfPlan { key } => write!(f, "CellOutOfPlan: {key}"),
        }
    }
}

impl std::error::Error for AttributionError {}

impl From<SchemaError> for AttributionError {
    fn from(e: SchemaError) -> Self {
        AttributionError::Schema(format!("{e:?}"))
    }
}

/// The machine-checkable refusal code (the boundary renders it verbatim).
impl AttributionError {
    /// The closed refusal code.
    pub fn code(&self) -> &'static str {
        match self {
            AttributionError::Schema(_) => "schema_violation",
            AttributionError::UnknownMethod(_) => "unknown_method",
            AttributionError::UnknownForkPolicy(_) => "unknown_fork_policy",
            AttributionError::EstimandBelowFloor { .. } => "estimand_below_floor",
            AttributionError::TooManyTargets { .. } => "too_many_targets",
            AttributionError::Underpowered { .. } => "underpowered",
            AttributionError::MissingMatchSpec => "missing_match_spec",
            AttributionError::UnmatchedBudget { .. } => "unmatched_budget",
            AttributionError::UnbudgetedArm => "unbudgeted_arm",
            AttributionError::PolicyCollapsed { .. } => "policy_collapsed",
            AttributionError::CouplingUnavailable { .. } => "coupling_unavailable",
            AttributionError::ClassInadmissible { .. } => "class_inadmissible",
            AttributionError::OutcomeOracleUndeclared { .. } => "outcome_oracle_undeclared",
            AttributionError::BudgetExceeded { .. } => "budget_exceeded",
            AttributionError::ReplayInvalid { .. } => "replay_invalid",
            AttributionError::InterventionPrecedesForkPoint { .. } => {
                "intervention_precedes_fork_point"
            }
            AttributionError::NoCoherentForkPoint { .. } => "no_coherent_fork_point",
            AttributionError::AuthorityWidening => "authority_widening",
            AttributionError::BudgetLoosening => "budget_loosening",
            AttributionError::MissingTargets => "missing_targets",
            AttributionError::CellOutOfPlan { .. } => "cell_out_of_plan",
        }
    }
}

// ── records ─────────────────────────────────────────────────────────────────

/// `ComponentTarget` — the typed attribution target (§5h.7 §3; the M1
/// projection spells `ref`, never `target_ref` — one spelling, CC1).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ComponentTarget {
    /// `slot | variant | rule | parameter | procedure | text_leaf |
    /// decision_point`.
    pub kind: String,
    /// The `semantic_id` (or `(run_id, seq)` decision-point spelling).
    pub ref_: String,
}

impl ComponentTarget {
    /// The canonical `{kind, ref}` member.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("kind", Json::str(&self.kind)),
            ("ref", Json::str(&self.ref_)),
        ])
    }

    /// Strict decode (bare strings decode as `{kind: "parameter"}` — the
    /// M1 factor-name compatibility the kernel already publishes).
    pub fn from_json(j: &Json) -> Result<ComponentTarget, AttributionError> {
        if let Some(s) = j.as_str() {
            return Ok(ComponentTarget {
                kind: "parameter".into(),
                ref_: s.to_string(),
            });
        }
        let m = expect_obj(j, "ComponentTarget")?;
        reject_unknown(m, &["kind", "ref"], "ComponentTarget")?;
        let kind = str_at(m, "kind", "ComponentTarget")?.to_string();
        if !TARGET_KINDS.contains(&kind.as_str()) {
            return Err(AttributionError::Schema(format!(
                "target kind `{kind}` not in {TARGET_KINDS:?}"
            )));
        }
        Ok(ComponentTarget {
            kind,
            ref_: str_at(m, "ref", "ComponentTarget")?.to_string(),
        })
    }
}

/// `coupled_sources[]` — a post-fork nondeterminism source actually
/// coupled, with its measured agreement (V10 — declared and measured,
/// never assumed; ADR-0199 D7).
#[derive(Debug, Clone, PartialEq)]
pub struct CoupledSource {
    /// The source spelling (`environment_state`, `provider_sampling`, …).
    pub source: String,
    /// The measured agreement (`false` ⇒ the source is not coupled and
    /// `TE_crn`/`DE`/`ME` over cells consuming it refuse).
    pub coupling_agreement: bool,
}

impl CoupledSource {
    /// Canonical member.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("source", Json::str(&self.source)),
            (
                "coupling_agreement",
                Json::Bool(self.coupling_agreement),
            ),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<CoupledSource, AttributionError> {
        let m = expect_obj(j, "coupled_source")?;
        reject_unknown(m, &["source", "coupling_agreement"], "coupled_source")?;
        Ok(CoupledSource {
            source: str_at(m, "source", "coupled_source")?.to_string(),
            coupling_agreement: bool_at(m, "coupling_agreement", "coupled_source")?,
        })
    }
}

/// `AttributionDesign` — the M2–M5 design record (desugars to an
/// `ExperimentSpec` at the boundary — `attribution_design` in `ops.rs`
/// is the M1 spelling; this record is the additive extension).
#[derive(Debug, Clone, PartialEq)]
pub struct AttributionDesign {
    /// The desugared experiment ref (content-addressed).
    pub design_ref: Option<String>,
    /// `configuration_id` the design attributes over.
    pub configuration_id: String,
    /// The subject run set (`[]` = configuration granularity).
    pub run_ids: Vec<String>,
    /// The subject task set (fork points ride `task` granularity).
    pub task_ids: Vec<String>,
    /// `M1 | M2 | M3 | M4 | M5`.
    pub method: String,
    /// The typed targets.
    pub targets: Vec<ComponentTarget>,
    /// `latest_coherent_before | run_start | every_decision_point`.
    pub fork_policy: String,
    /// The `every_decision_point{kind}` member (the decision-point kind —
    /// `None` otherwise).
    pub fork_kind: Option<String>,
    /// Replicates per arm per fork point (OQ-323 floors enforced).
    pub k: u32,
    /// `re_executed | deterministic`.
    pub replay_mode_requested: String,
    /// `none | crn` (CF-432 seed-coupling).
    pub noise_coupling: String,
    /// `weak | declared_strong` (CF-435 — `declared_strong` forces
    /// `run_start`, degrading the design to M1-equivalent semantics).
    pub coupling_assumption: String,
    /// The one `MatchSpec` (V3) — canonical JSON (the boundary decodes
    /// it into `hh_budget`'s record; the kernel only requires presence
    /// + the `mode` member's spelling when declared).
    pub match_spec: Json,
    /// The `MetricDeclaration` ref the effects report against (V5 —
    /// deterministic headline classes or `judged` + calibration at the
    /// boundary; the kernel records the declared `oracle_class` member).
    pub outcome: String,
    /// The outcome metric's oracle class — `judge` requires
    /// `calibration_ref` (V5's judged path; `exploratory`-only).
    pub outcome_oracle_class: Option<String>,
    /// The judged oracle's active calibration ref (V5).
    pub outcome_calibration_ref: Option<String>,
    /// The rollout reservation (I4 — reserve-before-spend;
    /// `estimate_rollouts` must not exceed it).
    pub budget_reserved: i64,
    /// The design seed (everything derives from it — the report is pure).
    pub seed: i64,
    /// `outcome | decision_sequence` (OQ-323's k rule).
    pub claim_kind: String,
    /// The pre-registration ref (V12 — Holm over the pre-registered
    /// family, BH otherwise).
    pub pre_registration_ref: Option<String>,
    /// The declared + measured couplings (V10).
    pub coupled_sources: Vec<CoupledSource>,
    /// M3 walk parameters.
    pub n_permutations: u32,
    /// M3 samples per coalition evaluation.
    pub samples_per_eval: u32,
    /// M3 antithetic pairing (V11 — mandatory true).
    pub antithetic: bool,
    /// M4's second factor levels (`{harness (T on/off)}` is fixed — the
    /// probed cells cross these levels; `model_snapshot | environment`).
    pub m4_factor: Option<String>,
    /// M4's second factor levels.
    pub m4_levels: Vec<String>,
    /// The TOST margin for "does not matter" verdicts (OQ-323 — never
    /// non-significance).
    pub equivalence_margin: Option<i64>,
    /// The matched configuration-level Δ (`unattributed_share` = Δ − Σ
    /// single-target effects — the share, never a decomposition claim).
    pub delta_total: Option<i64>,
    /// The requested report label (`confirmatory` is an admissibility
    /// ceiling — the estimator downgrades to `exploratory`/`preview`,
    /// never upgrades).
    pub label: String,
    /// Class admissibility (V9): the subject carries hosted rows.
    pub hosted: bool,
    /// The hosted exception — `observation_substitution` at
    /// `configuration` granularity with `model_io` interception.
    pub intervention_kind: Option<String>,
    /// The hosted exception granularity.
    pub granularity: Option<String>,
    /// The hosted exception interception declaration.
    pub interception: Option<String>,
}

impl AttributionDesign {
    /// The canonical `hh-attribution-design/1` member set.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str(DESIGN_SCHEMA));
        if let Some(d) = &self.design_ref {
            m.insert("design_ref".into(), Json::str(d));
        }
        m.insert(
            "subject".into(),
            Json::obj([
                (
                    "configuration_id",
                    Json::str(&self.configuration_id),
                ),
                (
                    "run_ids",
                    Json::Arr(self.run_ids.iter().map(Json::str).collect()),
                ),
                (
                    "task_ids",
                    Json::Arr(self.task_ids.iter().map(Json::str).collect()),
                ),
            ]),
        );
        m.insert("method".into(), Json::str(&self.method));
        m.insert(
            "targets".into(),
            Json::Arr(self.targets.iter().map(|t| t.to_json()).collect()),
        );
        m.insert("fork_policy".into(), Json::str(&self.fork_policy));
        if let Some(k) = &self.fork_kind {
            m.insert("fork_kind".into(), Json::str(k));
        }
        m.insert("k".into(), Json::Int(self.k as i64));
        m.insert(
            "replay_mode_requested".into(),
            Json::str(&self.replay_mode_requested),
        );
        m.insert(
            "noise_coupling".into(),
            Json::str(&self.noise_coupling),
        );
        m.insert(
            "coupling_assumption".into(),
            Json::str(&self.coupling_assumption),
        );
        m.insert("match".into(), self.match_spec.clone());
        m.insert("outcome".into(), Json::str(&self.outcome));
        if let Some(o) = &self.outcome_oracle_class {
            m.insert("outcome_oracle_class".into(), Json::str(o));
        }
        if let Some(c) = &self.outcome_calibration_ref {
            m.insert("outcome_calibration_ref".into(), Json::str(c));
        }
        m.insert(
            "budget".into(),
            Json::obj([("reserved", Json::Int(self.budget_reserved))]),
        );
        m.insert("seed".into(), Json::Int(self.seed));
        m.insert("claim_kind".into(), Json::str(&self.claim_kind));
        if let Some(p) = &self.pre_registration_ref {
            m.insert("pre_registration_ref".into(), Json::str(p));
        }
        m.insert(
            "coupled_sources".into(),
            Json::Arr(self.coupled_sources.iter().map(|c| c.to_json()).collect()),
        );
        if self.method == "M3" {
            m.insert(
                "shapley".into(),
                Json::obj([
                    ("n_permutations", Json::Int(self.n_permutations as i64)),
                    (
                        "samples_per_eval",
                        Json::Int(self.samples_per_eval as i64),
                    ),
                    ("antithetic", Json::Bool(self.antithetic)),
                ]),
            );
        }
        if self.method == "M4" {
            if let Some(f) = &self.m4_factor {
                m.insert("m4_factor".into(), Json::str(f));
            }
            m.insert(
                "m4_levels".into(),
                Json::Arr(self.m4_levels.iter().map(Json::str).collect()),
            );
        }
        if let Some(e) = self.equivalence_margin {
            m.insert("equivalence_margin".into(), Json::Int(e));
        }
        if let Some(d) = self.delta_total {
            m.insert("delta_total".into(), Json::Int(d));
        }
        m.insert("label".into(), Json::str(&self.label));
        if self.hosted {
            m.insert("hosted".into(), Json::Bool(true));
        }
        if let Some(i) = &self.intervention_kind {
            m.insert("intervention_kind".into(), Json::str(i));
        }
        if let Some(g) = &self.granularity {
            m.insert("granularity".into(), Json::str(g));
        }
        if let Some(i) = &self.interception {
            m.insert("interception".into(), Json::str(i));
        }
        Json::Obj(m)
    }

    /// Strict decode — unknown members refused (CC3); absent optional
    /// members take their MUST-data defaults.
    pub fn from_json(j: &Json) -> Result<AttributionDesign, AttributionError> {
        let m = expect_obj(j, "attribution_design")?;
        reject_unknown(
            m,
            &[
                "schema",
                "design_ref",
                "subject",
                "method",
                "targets",
                "fork_policy",
                "fork_kind",
                "k",
                "replay_mode_requested",
                "noise_coupling",
                "coupling_assumption",
                "match",
                "outcome",
                "outcome_oracle_class",
                "outcome_calibration_ref",
                "budget",
                "seed",
                "claim_kind",
                "pre_registration_ref",
                "coupled_sources",
                "shapley",
                "m4_factor",
                "m4_levels",
                "equivalence_margin",
                "delta_total",
                "label",
                "hosted",
                "intervention_kind",
                "granularity",
                "interception",
            ],
            "attribution_design",
        )?;
        let subject = member_at(m, "subject", "attribution_design")?;
        let sm = expect_obj(subject, "subject")?;
        let arr_str = |j: Option<&Json>| -> Vec<String> {
            match j {
                Some(Json::Arr(a)) => a
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
                _ => Vec::new(),
            }
        };
        let budget = member_at(m, "budget", "attribution_design")?;
        let bm = expect_obj(budget, "budget")?;
        let targets = match member_at(m, "targets", "attribution_design")? {
            Json::Arr(a) => a
                .iter()
                .map(ComponentTarget::from_json)
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(AttributionError::Schema("targets must be an array".into())),
        };
        let coupled = match m.get("coupled_sources") {
            Some(Json::Arr(a)) => a
                .iter()
                .map(CoupledSource::from_json)
                .collect::<Result<Vec<_>, _>>()?,
            _ => Vec::new(),
        };
        let mut n_permutations = 0;
        let mut samples_per_eval = 0;
        let mut antithetic = false;
        if let Some(s) = m.get("shapley") {
            let shm = expect_obj(s, "shapley")?;
            n_permutations = int_at(shm, "n_permutations", "shapley")? as u32;
            samples_per_eval = int_at(shm, "samples_per_eval", "shapley")? as u32;
            antithetic = opt_bool_at(shm, "antithetic")?.unwrap_or(false);
        }
        Ok(AttributionDesign {
            design_ref: opt_str_at(m, "design_ref")?.map(str::to_string),
            configuration_id: str_at(sm, "configuration_id", "subject")?.to_string(),
            run_ids: arr_str(sm.get("run_ids")),
            task_ids: arr_str(sm.get("task_ids")),
            method: str_at(m, "method", "attribution_design")?.to_string(),
            targets,
            fork_policy: str_at(m, "fork_policy", "attribution_design")?.to_string(),
            fork_kind: opt_str_at(m, "fork_kind")?.map(str::to_string),
            k: int_at(m, "k", "attribution_design")? as u32,
            replay_mode_requested: str_at(
                m,
                "replay_mode_requested",
                "attribution_design",
            )?
            .to_string(),
            noise_coupling: str_at(m, "noise_coupling", "attribution_design")?.to_string(),
            coupling_assumption: opt_str_at(m, "coupling_assumption")?
                .unwrap_or("weak")
                .to_string(),
            match_spec: match m.get("match") {
                None | Some(Json::Null) => return Err(AttributionError::MissingMatchSpec),
                Some(j) => j.clone(),
            },
            outcome: str_at(m, "outcome", "attribution_design")?.to_string(),
            outcome_oracle_class: opt_str_at(m, "outcome_oracle_class")?
                .map(str::to_string),
            outcome_calibration_ref: opt_str_at(m, "outcome_calibration_ref")?
                .map(str::to_string),
            budget_reserved: int_at(bm, "reserved", "budget")?,
            seed: int_at(m, "seed", "attribution_design")?,
            claim_kind: opt_str_at(m, "claim_kind")?.unwrap_or("outcome").to_string(),
            pre_registration_ref: opt_str_at(m, "pre_registration_ref")?
                .map(str::to_string),
            coupled_sources: coupled,
            n_permutations,
            samples_per_eval,
            antithetic,
            m4_factor: opt_str_at(m, "m4_factor")?.map(str::to_string),
            m4_levels: arr_str(m.get("m4_levels")),
            equivalence_margin: opt_int_at(m, "equivalence_margin")?,
            delta_total: opt_int_at(m, "delta_total")?,
            label: opt_str_at(m, "label")?.unwrap_or("exploratory").to_string(),
            hosted: opt_bool_at(m, "hosted")?.unwrap_or(false),
            intervention_kind: opt_str_at(m, "intervention_kind")?.map(str::to_string),
            granularity: opt_str_at(m, "granularity")?.map(str::to_string),
            interception: opt_str_at(m, "interception")?.map(str::to_string),
        })
    }
}

// ── validate (the static V-legs) ────────────────────────────────────────────

/// `validate_design` — the statically decidable half of V1–V12
/// (ADR-0200 D2: refusal or `n/a`, never a warning). The dynamic legs
/// (`change_rate > 0`, coherent fork points, consumed-source coupling)
/// run at `attribute` over the executed cells.
pub fn validate_design(d: &AttributionDesign) -> Result<(), AttributionError> {
    if !METHODS.contains(&d.method.as_str()) {
        return Err(AttributionError::UnknownMethod(d.method.clone()));
    }
    if !FORK_POLICIES.contains(&d.fork_policy.as_str()) {
        return Err(AttributionError::UnknownForkPolicy(d.fork_policy.clone()));
    }
    if !NOISE_COUPLINGS.contains(&d.noise_coupling.as_str()) {
        return Err(AttributionError::Schema(format!(
            "noise_coupling `{}` not in {NOISE_COUPLINGS:?}",
            d.noise_coupling
        )));
    }
    if !COUPLING_ASSUMPTIONS.contains(&d.coupling_assumption.as_str()) {
        return Err(AttributionError::Schema(format!(
            "coupling_assumption `{}` not in {COUPLING_ASSUMPTIONS:?}",
            d.coupling_assumption
        )));
    }
    if !REPLAY_MODES.contains(&d.replay_mode_requested.as_str()) {
        return Err(AttributionError::Schema(format!(
            "replay_mode_requested `{}` not in {REPLAY_MODES:?}",
            d.replay_mode_requested
        )));
    }
    if !CLAIM_KINDS.contains(&d.claim_kind.as_str()) {
        return Err(AttributionError::Schema(format!(
            "claim_kind `{}` not in {CLAIM_KINDS:?}",
            d.claim_kind
        )));
    }
    if !REPORT_LABELS.contains(&d.label.as_str()) {
        return Err(AttributionError::Schema(format!(
            "label `{}` not in {REPORT_LABELS:?}",
            d.label
        )));
    }
    if d.targets.is_empty() && d.method != "M1" {
        return Err(AttributionError::MissingTargets);
    }
    // M3's coalition cap (OQ-450 MUST-data placeholder).
    if d.method == "M3" && d.targets.len() > TARGET_CAP {
        return Err(AttributionError::TooManyTargets {
            n: d.targets.len(),
            cap: TARGET_CAP,
        });
    }
    // V9 — hosted rows: only `observation_substitution` at
    // `configuration` granularity under `model_io` interception; M2–M5
    // component targets are component-level → `n/a{class}` territory, a
    // refusal at design time (never a coerced native read).
    if d.hosted {
        let exception = d.intervention_kind.as_deref() == Some("observation_substitution")
            && d.granularity.as_deref() == Some("configuration")
            && d.interception.as_deref() == Some("model_io");
        if !exception {
            return Err(AttributionError::ClassInadmissible {
                detail: "hosted subjects admit only observation_substitution at \
                         configuration granularity with model_io interception"
                    .into(),
            });
        }
    }
    // V3 — one MatchSpec, always (`mode` spelled when the caller records
    // it; the boundary's MatchSpec decode is the second leg).
    if matches!(d.match_spec, Json::Null) {
        return Err(AttributionError::MissingMatchSpec);
    }
    if let Some(mode) = d.match_spec.get("mode").and_then(Json::as_str) {
        if mode != "matched_total" && d.method != "M1" {
            return Err(AttributionError::UnmatchedBudget {
                detail: format!(
                    "match mode `{mode}` — attribution claims ride matched_total"
                ),
            });
        }
    }
    // The estimand floor (V1): TE_crn/DE/ME require `deterministic` +
    // `crn`; TE_marg/shapley floor at `re_executed`.
    let needs_crn = d.method == "M5"
        || (d.method == "M2" && d.replay_mode_requested == "deterministic")
        || d.noise_coupling == "crn";
    if needs_crn {
        if d.replay_mode_requested != "deterministic" {
            return Err(AttributionError::EstimandBelowFloor {
                method: d.method.clone(),
                floor: "deterministic + crn".into(),
            });
        }
        if d.noise_coupling != "crn" {
            return Err(AttributionError::EstimandBelowFloor {
                method: d.method.clone(),
                floor: "noise_coupling = crn".into(),
            });
        }
        // V10 — every consumed source must be declared with measured
        // agreement (declared here; the consumed-set check is dynamic).
        if d.coupled_sources.is_empty() {
            return Err(AttributionError::CouplingUnavailable {
                source: "(no coupled_sources declared)".into(),
            });
        }
        if let Some(c) = d.coupled_sources.iter().find(|c| !c.coupling_agreement) {
            return Err(AttributionError::CouplingUnavailable {
                source: c.source.clone(),
            });
        }
    }
    // V11 — Shapley: antithetic pairing is mandatory; the walk is
    // reserved before the first rollout (I4).
    if d.method == "M3" {
        if !d.antithetic {
            return Err(AttributionError::Schema(
                "M3 requires antithetic = true (V11 — no asymmetric walks)".into(),
            ));
        }
        if d.n_permutations == 0 || d.samples_per_eval == 0 {
            return Err(AttributionError::Underpowered {
                k: d.n_permutations.min(d.samples_per_eval),
                required: 1,
            });
        }
    }
    if d.method == "M4" && d.m4_levels.len() < 2 {
        return Err(AttributionError::Schema(
            "M4 needs ≥ 2 probed levels of the second factor (unprobed ⇒ \
             unknown, never interpolated — ADR-0012)"
                .into(),
        ));
    }
    // OQ-323's k floors.
    let floor = match d.replay_mode_requested.as_str() {
        "re_executed" => K_REEXECUTED,
        _ if d.noise_coupling == "crn" => {
            if d.claim_kind == "decision_sequence" {
                K_CRN_SEQUENCE
            } else {
                K_CRN_OUTCOME
            }
        }
        _ => K_DETERMINISTIC,
    };
    if d.method != "M3" && d.k < floor {
        return Err(AttributionError::Underpowered {
            k: d.k,
            required: floor,
        });
    }
    // V5's judged path — a judged outcome oracle names its active
    // calibration (the effect then never confirms).
    if d.outcome_oracle_class.as_deref() == Some("judge")
        && d.outcome_calibration_ref.is_none()
    {
        return Err(AttributionError::OutcomeOracleUndeclared {
            outcome: d.outcome.clone(),
        });
    }
    Ok(())
}

/// `effective_fork_policy` — `declared_strong` coupling forces
/// `run_start` (the degradation is recorded on the report's
/// `assumptions`, never silent; ADR-0199 D4).
pub fn effective_fork_policy(d: &AttributionDesign) -> &str {
    if d.coupling_assumption == "declared_strong" {
        "run_start"
    } else {
        d.fork_policy.as_str()
    }
}

/// `estimate_rollouts(design, n_fork_points)` — the scheduler's cost
/// input *before* any attribution spend is reserved (§05e ADR-0188):
/// M3 — `n_permutations · (2 antithetic) · (|targets| + 1) ·
/// samples_per_eval`; M2/M5 — `(|targets| · |fork_points| + 1) · 2k`
/// (the +1 is the factual/noise-floor arm's extra cell per fork point);
/// M4 — `|levels| · |fork_points| · 2k`.
pub fn estimate_rollouts(d: &AttributionDesign, n_fork_points: usize) -> i64 {
    let fp = n_fork_points.max(1) as i64;
    match d.method.as_str() {
        "M3" => {
            d.n_permutations as i64
                * if d.antithetic { 2 } else { 1 }
                * (d.targets.len() as i64 + 1)
                * d.samples_per_eval as i64
        }
        "M4" => d.m4_levels.len().max(1) as i64 * fp * 2 * d.k as i64,
        "M5" => (d.targets.len() as i64 * fp + 1) * 3 * d.k as i64,
        _ => (d.targets.len() as i64 * fp + 1) * 2 * d.k as i64,
    }
}

// ── the plan (desugaring to counterfactual arms) ────────────────────────────

/// `branch_seed` — the CF-432 amended derivation (ADR-0199 D7): under
/// `noise_coupling = crn` the factual and counterfactual branch of one
/// replicate share `H(config_seed ∥ replicate_index ∥ fork_point)`;
/// otherwise the per-branch `H(seed ∥ target ∥ fork ∥ role ∥ i)` form
/// stands (ADR-0135 D2).
pub fn branch_seed(d: &AttributionDesign, fork_point: i64, role: &str, replicate: u32) -> String {
    let material = if d.noise_coupling == "crn" {
        format!("crn:{}:{replicate}:{fork_point}", d.seed)
    } else {
        format!("arm:{}:{fork_point}:{role}:{replicate}", d.seed)
    };
    format!(
        "sha256:{}",
        hh_wire::sha256::sha256_hex(material.as_bytes())
    )
}

/// One planned arm cell — the records-out half the boundary executes
/// through Group W `counterfactual` (`branch_kind = counterfactual`,
/// `charged_to = instrument`; `role ∈ {factual, counterfactual, direct}`
/// — `direct` is M5's mediation arm).
#[derive(Debug, Clone, PartialEq)]
pub struct ArmCell {
    /// The join key (`target|subset-hash` for M3).
    pub target: String,
    /// The fork point (task granularity).
    pub fork_point: i64,
    /// `factual | counterfactual | direct`.
    pub role: String,
    /// The replicate index.
    pub replicate_index: u32,
    /// M3's held-factual coalition (the complement is re-executed).
    pub subset: Vec<String>,
    /// The derived branch seed.
    pub seed: String,
    /// The intervention kind the arm replays under (`do_resample` on the
    /// factual arm — the null intervention).
    pub intervention_kind: String,
}

impl ArmCell {
    /// The join key — `(target, fork_point, role, replicate_index)`;
    /// `subset` is reported beside for M3.
    pub fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.target, self.fork_point, self.role, self.replicate_index
        )
    }

    /// The canonical member.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("target", Json::str(&self.target)),
            ("fork_point", Json::Int(self.fork_point)),
            ("role", Json::str(&self.role)),
            ("replicate_index", Json::Int(self.replicate_index as i64)),
            (
                "subset",
                Json::Arr(self.subset.iter().map(Json::str).collect()),
            ),
            ("seed", Json::str(&self.seed)),
            ("intervention_kind", Json::str(&self.intervention_kind)),
        ])
    }
}

/// `attribution_plan(design, fork_points) → [ArmCell]` — desugar the
/// design into the counterfactual arm set: M2/M5 per target × fork ×
/// role × k; M3 per walk × prefix-coalition × samples; M4 per level ×
/// fork × role × k. The plan is truncated at `budget_reserved` — the
/// truncation is reported (`budget_truncated`), never overspent (I4).
pub fn attribution_plan(
    d: &AttributionDesign,
    fork_points: &[i64],
) -> Result<Vec<ArmCell>, AttributionError> {
    validate_design(d)?;
    let policy = effective_fork_policy(d);
    let fps: Vec<i64> = match policy {
        "run_start" => vec![0],
        "latest_coherent_before" => {
            if fork_points.is_empty() {
                return Err(AttributionError::NoCoherentForkPoint { point: 0 });
            }
            vec![*fork_points.iter().max().expect("non-empty")]
        }
        _ => {
            if fork_points.is_empty() {
                return Err(AttributionError::NoCoherentForkPoint { point: 0 });
            }
            fork_points.to_vec()
        }
    };
    let mut cells: Vec<ArmCell> = Vec::new();
    let mut push = |target: &str, fp: i64, role: &str, i: u32, subset: Vec<String>, kind: &str| {
        cells.push(ArmCell {
            target: target.to_string(),
            fork_point: fp,
            role: role.to_string(),
            replicate_index: i,
            subset,
            seed: branch_seed(d, fp, role, i),
            intervention_kind: kind.to_string(),
        });
    };
    match d.method.as_str() {
        "M3" => {
            // Permutation walks — seeded by (design.seed ∥ walk): each
            // walk evaluates v(S) for its prefix coalitions; the
            // antithetic partner is the reversed walk (V11). The
            // complement coalition is re-executed (`definition_diff`
            // over the held-out set); the held members stay factual.
            let names: Vec<String> = d.targets.iter().map(|t| t.ref_.clone()).collect();
            let mut walks: Vec<Vec<String>> = Vec::new();
            for w in 0..d.n_permutations {
                let mut rng = XorShift64::seeded(&format!("walk:{}:{w}", d.seed));
                let mut perm = names.clone();
                // Fisher–Yates over the seeded stream.
                for i in (1..perm.len()).rev() {
                    let j = rng.below(i + 1);
                    perm.swap(i, j);
                }
                walks.push(perm.clone());
                let mut rev = perm;
                rev.reverse();
                walks.push(rev);
            }
            let fp = fps[0];
            for perm in &walks {
                let mut held: Vec<String> = Vec::new();
                // The ∅-coalition and each prefix — |targets| + 1 subsets.
                let mut subsets: Vec<Vec<String>> = vec![Vec::new()];
                for t in perm {
                    held.push(t.clone());
                    subsets.push(held.clone());
                }
                for s in &subsets {
                    let key = s.join("+");
                    for i in 0..d.samples_per_eval {
                        push(&key, fp, "counterfactual", i, s.clone(), "definition_diff");
                    }
                }
            }
        }
        "M4" => {
            // {harness on/off} × m4_levels × fork — fully crossed; an
            // unprobed cell is `unknown`, never fabricated.
            for level in &d.m4_levels {
                for &fp in &fps {
                    for i in 0..d.k {
                        push(level, fp, "factual", i, Vec::new(), "do_resample");
                        push(level, fp, "counterfactual", i, Vec::new(), "definition_diff");
                    }
                }
            }
        }
        "M5" => {
            // factual + counterfactual (TE_crn) + `direct` (the mediation
            // arm — the mechanism substituted, the downstream resampled).
            for t in &d.targets {
                for &fp in &fps {
                    for i in 0..d.k {
                        push(&t.ref_, fp, "factual", i, Vec::new(), "do_resample");
                        push(
                            &t.ref_,
                            fp,
                            "counterfactual",
                            i,
                            Vec::new(),
                            "definition_diff",
                        );
                        push(&t.ref_, fp, "direct", i, Vec::new(), "decision_override");
                    }
                }
            }
        }
        _ => {
            // M2 — per (target, fork): the factual arm is the null
            // intervention (`do_resample`), the counterfactual the
            // target's intervention.
            for t in &d.targets {
                for &fp in &fps {
                    for i in 0..d.k {
                        push(&t.ref_, fp, "factual", i, Vec::new(), "do_resample");
                        push(
                            &t.ref_,
                            fp,
                            "counterfactual",
                            i,
                            Vec::new(),
                            "definition_diff",
                        );
                    }
                }
            }
        }
    }
    // I4 — reserve-before-spend: truncate at the reservation; the
    // truncation count is reported by `attribute` (`budget_truncated`).
    if d.budget_reserved > 0 && cells.len() as i64 > d.budget_reserved {
        cells.truncate(d.budget_reserved as usize);
    }
    Ok(cells)
}

/// `intervention_for(target, change, classification) → InterventionRecord`
/// — the ADR-0135 record for one `ComponentTarget` change (ADR-0199
/// D3/D4): `change.kind` names the change (`slot_disable`, `variant_swap`,
/// `parameter_change`, `rule_removal`, `procedure_edit`, `leaf_ablation`,
/// `observation`, `response`, `decision`, `budget`, `permission`,
/// `model_swap`); `classification{authority_delta, budget_delta}` — a
/// widening/loosening diff refuses (KA-I7-14's governance leg).
pub fn intervention_for(
    target: &ComponentTarget,
    change: &Json,
    classification: Option<&Json>,
) -> Result<Json, AttributionError> {
    if let Some(c) = classification {
        if c.get("authority_delta").and_then(Json::as_str) == Some("widening") {
            return Err(AttributionError::AuthorityWidening);
        }
        if c.get("budget_delta").and_then(Json::as_str) == Some("loosening") {
            return Err(AttributionError::BudgetLoosening);
        }
    }
    let cm = expect_obj(change, "change")?;
    let kind = str_at(cm, "kind", "change")?;
    let intervention = match kind {
        "slot_disable" | "variant_swap" | "parameter_change" | "rule_removal"
        | "procedure_edit" | "leaf_ablation" => "definition_diff",
        "observation" | "observation_substitution" => "observation_substitution",
        "response" | "response_substitution" => "response_substitution",
        "decision" | "decision_override" => "decision_override",
        "budget" | "budget_change" => "budget_change",
        "permission" | "permission_change" => "permission_change",
        "model_swap" => "profile_diff",
        other => {
            return Err(AttributionError::Schema(format!(
                "change.kind `{other}` is not a named intervention"
            )))
        }
    };
    if !INTERVENTION_KINDS.contains(&intervention) {
        return Err(AttributionError::Schema(format!(
            "intervention kind `{intervention}` not in {INTERVENTION_KINDS:?}"
        )));
    }
    // `permission_change` narrows only (§5h.7 §6) — the change's
    // `direction` member must spell `narrowing`.
    if intervention == "permission_change" {
        match cm.get("direction").and_then(Json::as_str) {
            Some("narrowing") | Some("none") => {}
            _ => return Err(AttributionError::AuthorityWidening),
        }
    }
    let earliest = opt_int_at(cm, "earliest_affected_seq")?.unwrap_or(0);
    Ok(Json::obj([
        ("schema", Json::str("hh-intervention/1")),
        ("kind", Json::str(intervention)),
        ("target", Json::str(&target.ref_)),
        ("target_kind", Json::str(&target.kind)),
        ("earliest_affected_seq", Json::Int(earliest)),
        ("change", change.clone()),
    ]))
}

// ── the outcome fold ────────────────────────────────────────────────────────

/// One executed arm outcome — the records-in half the driver produces
/// (host executor or the known-effect suite). `outcome` is the
/// metric's scalar (ppm rates or the unit's raw scale);
/// `change_rate_ppm` is the factual arm's action-changed share at the
/// fork point (V4); `consumed_sources` the post-fork nondeterminism the
/// arm consumed (V10).
#[derive(Debug, Clone, PartialEq)]
pub struct ArmOutcome {
    /// The join key (`target|subset-hash`).
    pub target: String,
    /// The fork point.
    pub fork_point: i64,
    /// `factual | counterfactual | direct`.
    pub role: String,
    /// The replicate index.
    pub replicate_index: u32,
    /// The measured outcome (`None` = not executed → `n/a{not_run}`).
    pub outcome: Option<i64>,
    /// The arm's `ReplayValidityReport.mode`.
    pub validity: String,
    /// V4 — the factual arm's change rate at the fork (`Some(0)` ⇒ the
    /// cell's policy collapsed).
    pub change_rate_ppm: Option<i64>,
    /// V6 — the counterfactual suffix reached a deferred effect: the
    /// branch paused `awaiting_promotion`; the outcome is `n/a{not_run}`
    /// and the process effect is still reported.
    pub deferred_paused: bool,
    /// The post-fork sources the arm consumed (V10).
    pub consumed_sources: Vec<String>,
    /// The arm's instrument spend (the `budget.spend` sum).
    pub spend: i64,
}

impl ArmOutcome {
    /// The join key — must equal the planned cell's.
    pub fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.target, self.fork_point, self.role, self.replicate_index
        )
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ArmOutcome, AttributionError> {
        let m = expect_obj(j, "arm_outcome")?;
        reject_unknown(
            m,
            &[
                "target",
                "fork_point",
                "role",
                "replicate_index",
                "outcome",
                "validity",
                "change_rate_ppm",
                "deferred_paused",
                "consumed_sources",
                "spend",
            ],
            "arm_outcome",
        )?;
        Ok(ArmOutcome {
            target: str_at(m, "target", "arm_outcome")?.to_string(),
            fork_point: int_at(m, "fork_point", "arm_outcome")?,
            role: str_at(m, "role", "arm_outcome")?.to_string(),
            replicate_index: int_at(m, "replicate_index", "arm_outcome")? as u32,
            outcome: opt_int_at(m, "outcome")?,
            validity: opt_str_at(m, "validity")?.unwrap_or("re_executed").to_string(),
            change_rate_ppm: opt_int_at(m, "change_rate_ppm")?,
            deferred_paused: opt_bool_at(m, "deferred_paused")?.unwrap_or(false),
            consumed_sources: match m.get("consumed_sources") {
                Some(Json::Arr(a)) => a
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
                _ => Vec::new(),
            },
            spend: opt_int_at(m, "spend")?.unwrap_or(0),
        })
    }
}

/// The Newcombe–Wilson score interval for a difference of proportions
/// (OQ-323 — binary outcomes at one fork point), in ppm.
fn newcombe_diff(c1: i64, n1: i64, c0: i64, n0: i64, confidence_ppm: i64) -> Option<Interval> {
    if n1 <= 0 || n0 <= 0 {
        return None;
    }
    let w1 = stats::wilson(c1, n1, confidence_ppm)?;
    let w0 = stats::wilson(c0, n0, confidence_ppm)?;
    let p1 = (c1 * PPM) / n1;
    let p0 = (c0 * PPM) / n0;
    let d = p1 - p0;
    let lo_term = ((p1 - w1.lo) as i128).pow(2) + ((w0.hi - p0) as i128).pow(2);
    let hi_term = ((w1.hi - p1) as i128).pow(2) + ((p0 - w0.lo) as i128).pow(2);
    Some(Interval {
        lo: d - stats::isqrt(lo_term as u128) as i64,
        hi: d + stats::isqrt(hi_term as u128) as i64,
    })
}

/// The unit-outcome interval over paired per-fork diffs
/// (`bootstrap_paired`, cluster = fork point/task — OQ-323's unit arm).
fn paired_interval(diffs: &[i64], seed: &str) -> Option<Interval> {
    if diffs.is_empty() {
        return None;
    }
    if diffs.len() == 1 {
        // One fork point: the per-replicate pairs can't cluster — CLT
        // over the replicate differences (the design's suffix-paired
        // replicates are independent draws, honest at k ≥ the floor).
        return None;
    }
    stats::bootstrap_paired(diffs, CONFIDENCE_PPM, false, seed)
}

/// Whether every measured outcome is binary (0/PPM) — the Newcombe arm.
fn is_binary(xs: &[i64]) -> bool {
    xs.iter().all(|&x| x == 0 || x == PPM)
}

/// `attribute(design, outcomes, planned)` — fold the executed cells into
/// `AttributionReport/1`. `planned` is `attribution_plan`'s output (the
/// reservation + truncation accounting); `outcomes` join by cell key.
///
/// Per-target effects aggregate across fork points as a paired effect
/// clustered by task (= fork point): binary outcomes take the
/// Newcombe–Wilson score interval per fork point and
/// `bootstrap_paired` over the per-fork diffs; unit outcomes take the
/// paired bootstrap directly (OQ-323).
pub fn attribute(
    d: &AttributionDesign,
    outcomes: &[ArmOutcome],
    planned: &[ArmCell],
) -> Result<Json, AttributionError> {
    validate_design(d)?;
    let mut reasons: Vec<String> = Vec::new();
    if d.coupling_assumption == "declared_strong" {
        reasons.push("declared_strong_forces_run_start".into());
    }
    let planned_keys: BTreeSet<String> = planned.iter().map(|c| c.key()).collect();
    let planned_rollouts = planned.len() as i64;
    // I4 — the plan is truncated against the reservation; the truncation
    // is a reported fact, never an overspend.
    let truncated = {
        let distinct_fps: BTreeSet<i64> = planned.iter().map(|c| c.fork_point).collect();
        d.budget_reserved > 0
            && estimate_rollouts(d, distinct_fps.len().max(1)) > planned_rollouts
    };
    let mut by_cell: BTreeMap<String, &ArmOutcome> = BTreeMap::new();
    for o in outcomes {
        if !planned_keys.is_empty() && !planned_keys.contains(&o.key()) {
            return Err(AttributionError::CellOutOfPlan { key: o.key() });
        }
        by_cell.insert(o.key(), o);
    }
    // The label ceiling: `TE_marg` alone never confirms; `degraded`
    // forces exploratory; M5 stays exploratory pending OQ-449; a judged
    // outcome oracle never confirms.
    let estimand = match d.method.as_str() {
        "M3" => "shapley",
        "M5" => "DE",
        _ => {
            if d.replay_mode_requested == "deterministic" {
                "TE_crn"
            } else {
                "TE_marg"
            }
        }
    };
    let mut label_ceiling = if estimand == "TE_marg" { "exploratory" } else { d.label.as_str() };
    if d.method == "M5" || d.outcome_oracle_class.as_deref() == Some("judge") {
        label_ceiling = "exploratory";
    }
    if d.label == "preview" {
        label_ceiling = "preview";
    }
    let mut any_degraded = false;
    let mut any_invalid = false;
    let mut effects: Vec<Json> = Vec::new();
    let mut collapsed_seqs: Vec<i64> = Vec::new();
    let mut effect_deltas: Vec<Vec<i64>> = Vec::new();
    let spend: i64 = outcomes.iter().map(|o| o.spend).sum();

    // Per (target) effect computation — M2/M5's per-target-per-fork fold.
    // `per_fork_effect(t, fp, roles, outcomes) → (effect, diff, collapsed)`
    // — one `effects[]` row per (target, fork-point) as the spec
    // spells; `diff` is the per-cell point (for the multiplicity fold),
    // `collapsed` marks V4. Binary outcomes take the Newcombe–Wilson
    // score interval; unit outcomes take the CLT over the per-replicate
    // paired diffs (seed-paired under `crn`, honest otherwise — OQ-323).
    let mut per_fork_effect = |t: &ComponentTarget,
                               fp: i64,
                               roles: &[&str],
                               outcomes: &BTreeMap<String, &ArmOutcome>|
     -> (Json, Option<i64>, bool) {
        let mut n_fact = 0i64;
        let mut n_cf = 0i64;
        let mut not_run = 0i64;
        let mut deferred = false;
        let mut collapsed = false;
        let mut degraded = false;
        let mut min_validity_rank: usize = 0;
        let order = ["deterministic", "re_executed", "degraded", "invalid"];
        let rank = |v: &str| order.iter().position(|&x| x == v).unwrap_or(1);
        let mut cell_collapsed = true;
        let mut fact_vals: Vec<i64> = Vec::new();
        let mut cf_vals: Vec<i64> = Vec::new();
        let mut direct_vals: Vec<i64> = Vec::new();
        let mut rates: Vec<i64> = Vec::new();
        for i in 0..d.k.max(1) {
            for (role, sink) in [
                ("factual", &mut fact_vals),
                ("counterfactual", &mut cf_vals),
                ("direct", &mut direct_vals),
            ]
            .into_iter()
            .filter(|(r, _)| roles.contains(r))
            {
                let key = format!("{}|{}|{}|{}", t.ref_, fp, role, i);
                match outcomes.get(&key) {
                    Some(o) => {
                        if rank(&o.validity) > min_validity_rank {
                            min_validity_rank = rank(&o.validity);
                        }
                        if o.validity == "degraded" {
                            degraded = true;
                        }
                        if role == "factual" {
                            // V4 — the collapse is the *factual* arm's
                            // change rate at the fork; a counterfactual
                            // cell's unmeasured rate never clears it.
                            if let Some(cr) = o.change_rate_ppm {
                                rates.push(cr);
                                if cr > 0 {
                                    cell_collapsed = false;
                                }
                            } else {
                                cell_collapsed = false;
                            }
                        }
                        if o.deferred_paused {
                            deferred = true;
                        }
                        match o.outcome {
                            Some(v) => {
                                sink.push(v);
                                if role == "factual" {
                                    n_fact += 1;
                                } else if role == "counterfactual" {
                                    n_cf += 1;
                                }
                            }
                            None => not_run += 1,
                        }
                    }
                    None => not_run += 1,
                }
            }
        }
        if cell_collapsed && !fact_vals.is_empty() {
            collapsed = true;
            collapsed_seqs.push(fp);
        }
        let diff = if fact_vals.is_empty() || cf_vals.is_empty() {
            None
        } else {
            Some(stats::mean(&cf_vals).unwrap_or(0) - stats::mean(&fact_vals).unwrap_or(0))
        };
        let mut e = BTreeMap::new();
        e.insert("target".into(), t.to_json());
        e.insert("fork_seq".into(), Json::Int(fp));
        e.insert("estimand".into(), Json::str(estimand));
        e.insert("statistic".into(), Json::str("paired_diff"));
        if let Some(point) = diff {
            e.insert("point".into(), Json::Int(point));
        }
        // The per-cell interval: binary ⇒ Newcombe–Wilson on the two
        // proportions; unit ⇒ CLT over the per-replicate paired diffs.
        let iv = if is_binary(&fact_vals) && is_binary(&cf_vals) && !fact_vals.is_empty()
        {
            let c1 = cf_vals.iter().filter(|&&v| v == PPM).count() as i64;
            let c0 = fact_vals.iter().filter(|&&v| v == PPM).count() as i64;
            newcombe_diff(
                c1,
                cf_vals.len() as i64,
                c0,
                fact_vals.len() as i64,
                CONFIDENCE_PPM,
            )
            .map(|i| {
                Json::obj([
                    ("method", Json::str("newcombe_wilson")),
                    ("level", Json::Int(CONFIDENCE_PPM)),
                    ("lo", Json::Int(i.lo)),
                    ("hi", Json::Int(i.hi)),
                ])
            })
        } else {
            let diffs: Vec<i64> = cf_vals
                .iter()
                .zip(fact_vals.iter())
                .map(|(c, f)| c - f)
                .collect();
            if diffs.len() >= 2 {
                stats::clt(&diffs, CONFIDENCE_PPM).map(|i| {
                    Json::obj([
                        ("method", Json::str("clt_paired")),
                        ("level", Json::Int(CONFIDENCE_PPM)),
                        ("lo", Json::Int(i.lo)),
                        ("hi", Json::Int(i.hi)),
                    ])
                })
            } else {
                None
            }
        };
        if let Some(iv) = iv {
            e.insert("interval".into(), iv);
        }
        e.insert(
            "n".into(),
            Json::obj([
                ("factual_branches", Json::Int(n_fact)),
                ("counterfactual_branches", Json::Int(n_cf)),
                ("not_run", Json::Int(not_run)),
            ]),
        );
        e.insert("replay_mode".into(), Json::str(&d.replay_mode_requested));
        e.insert(
            "validity_mode".into(),
            Json::str(order[min_validity_rank]),
        );
        e.insert(
            "change_rate".into(),
            stats::mean(&rates).map(Json::Int).unwrap_or(Json::Null),
        );
        // `noise_floor` — the factual arm's dispersion (hi−lo; 0 on a
        // constant arm — I3).
        let dispersion = if fact_vals.len() > 1 {
            *fact_vals.iter().max().unwrap_or(&0) - *fact_vals.iter().min().unwrap_or(&0)
        } else {
            0
        };
        e.insert("noise_floor".into(), Json::Int(dispersion));
        if collapsed {
            e.insert("policy_collapsed".into(), Json::Bool(true));
            e.insert("verdict".into(), Json::str("policy_collapsed"));
        }
        if deferred {
            e.insert("deferred_paused".into(), Json::Bool(true));
        }
        if degraded {
            e.insert("degraded".into(), Json::Bool(true));
        }
        (Json::Obj(e), diff, collapsed)
    };

    // `per_target_effect` — the pooled form M5 needs (its three-arm
    // fold recomputes DE/ME below).
    let mut per_target_effect = |t: &ComponentTarget,
                             fps: &[i64],
                             roles: &[&str],
                             outcomes: &BTreeMap<String, &ArmOutcome>|
     -> (Json, Vec<i64>, bool) {
        let mut fork_diffs: Vec<i64> = Vec::new();
        let mut effects_rows: Vec<Json> = Vec::new();
        let mut collapsed_any = false;
        for &fp in fps {
            let (row, diff, collapsed) = per_fork_effect(t, fp, roles, outcomes);
            if let Some(x) = diff {
                fork_diffs.push(x);
            }
            collapsed_any |= collapsed;
            effects_rows.push(row);
        }
        let collapsed = collapsed_any;
        let mut fork_points_seen: Vec<i64> = Vec::new();
        let mut n_fact = 0i64;
        let mut n_cf = 0i64;
        let mut not_run = 0i64;
        let mut deferred = false;
        let mut min_validity_rank: usize = 0;
        let order = ["deterministic", "re_executed", "degraded", "invalid"];
        let rank = |v: &str| order.iter().position(|&x| x == v).unwrap_or(1);
        for &fp in fps {
            let mut fact_vals: Vec<i64> = Vec::new();
            let mut cf_vals: Vec<i64> = Vec::new();
            let mut direct_vals: Vec<i64> = Vec::new();
            for i in 0..d.k.max(1) {
                for (role, sink) in [
                    ("factual", &mut fact_vals),
                    ("counterfactual", &mut cf_vals),
                    ("direct", &mut direct_vals),
                ]
                .into_iter()
                .filter(|(r, _)| roles.contains(r))
                {
                    let key = format!("{}|{}|{}|{}", t.ref_, fp, role, i);
                    match outcomes.get(&key) {
                        Some(o) => {
                            if rank(&o.validity) > min_validity_rank {
                                min_validity_rank = rank(&o.validity);
                            }
                            if o.deferred_paused {
                                deferred = true;
                            }
                            match o.outcome {
                                Some(v) => {
                                    sink.push(v);
                                    if role == "factual" {
                                        n_fact += 1;
                                    } else if role == "counterfactual" {
                                        n_cf += 1;
                                    }
                                }
                                None => not_run += 1,
                            }
                        }
                        None => not_run += 1,
                    }
                }
            }
            if !fact_vals.is_empty() {
                fork_points_seen.push(fp);
            }
        }
        (
            {
                let mut e = BTreeMap::new();
                e.insert("target".into(), t.to_json());
                e.insert("estimand".into(), Json::str(estimand));
                e.insert("statistic".into(), Json::str("paired_diff"));
                if let Some(point) = stats::mean(&fork_diffs) {
                    e.insert("point".into(), Json::Int(point));
                }
                let iv = if !fork_diffs.is_empty() {
                    paired_interval(&fork_diffs, &format!("attr:{}:{}", d.seed, t.ref_))
                } else {
                    None
                };
                if let Some(iv) = iv {
                    e.insert(
                        "interval".into(),
                        Json::obj([
                            ("method", Json::str("bootstrap_paired")),
                            ("level", Json::Int(CONFIDENCE_PPM)),
                            ("lo", Json::Int(iv.lo)),
                            ("hi", Json::Int(iv.hi)),
                        ]),
                    );
                }
                e.insert(
                    "n".into(),
                    Json::obj([
                        ("factual_branches", Json::Int(n_fact)),
                        ("counterfactual_branches", Json::Int(n_cf)),
                        (
                            "fork_points",
                            Json::Int(fork_points_seen.len() as i64),
                        ),
                        ("not_run", Json::Int(not_run)),
                    ]),
                );
                e.insert("replay_mode".into(), Json::str(&d.replay_mode_requested));
                e.insert(
                    "validity_mode".into(),
                    Json::str(order[min_validity_rank]),
                );
                e.insert(
                    "cells".into(),
                    Json::Arr(effects_rows),
                );
                if collapsed {
                    e.insert("policy_collapsed".into(), Json::Bool(true));
                }
                if deferred {
                    e.insert("deferred_paused".into(), Json::Bool(true));
                }
                Json::Obj(e)
            },
            fork_diffs,
            collapsed,
        )
    };

    // The design's fork-point universe — from the plan (all planned
    // points, not only answered ones).
    let all_fps: Vec<i64> = {
        let mut v: Vec<i64> = planned.iter().map(|c| c.fork_point).collect();
        v.sort_unstable();
        v.dedup();
        v
    };

    match d.method.as_str() {
        "M3" => {
            // Coalition credit — v(S) = mean counterfactual outcome over
            // the cells whose held set is exactly S.
            let mut v_of: BTreeMap<Vec<String>, Vec<i64>> = BTreeMap::new();
            for cell in planned.iter().filter(|c| c.role == "counterfactual") {
                let key = cell.key();
                if let Some(o) = by_cell.get(&key) {
                    if let Some(v) = o.outcome {
                        v_of.entry(cell.subset.clone()).or_default().push(v);
                    }
                    if o.validity == "degraded" {
                        any_degraded = true;
                    }
                    if o.validity == "invalid" {
                        any_invalid = true;
                    }
                }
            }
            if any_invalid {
                return Err(AttributionError::ReplayInvalid {
                    reason: "a counterfactual branch is invalid".into(),
                });
            }
            let v = |s: &[String]| -> Option<i64> {
                let mut key: Vec<String> = s.to_vec();
                key.sort();
                // Pool every cell whose *set* equals S (walk prefixes
                // differ in order — v(S) is a set function).
                let mut all: Vec<i64> = Vec::new();
                for (k, xs) in &v_of {
                    let mut kk = k.clone();
                    kk.sort();
                    if kk == key {
                        all.extend(xs.iter().copied());
                    }
                }
                stats::mean(&all)
            };
            // Regenerate the walks (the plan's order is the walk order —
            // deterministic under the same seed).
            let names: Vec<String> = d.targets.iter().map(|t| t.ref_.clone()).collect();
            let mut walks: Vec<Vec<String>> = Vec::new();
            for w in 0..d.n_permutations {
                let mut rng = XorShift64::seeded(&format!("walk:{}:{w}", d.seed));
                let mut perm = names.clone();
                for i in (1..perm.len()).rev() {
                    let j = rng.below(i + 1);
                    perm.swap(i, j);
                }
                walks.push(perm.clone());
                let mut rev = perm;
                rev.reverse();
                walks.push(rev);
            }
            let walks_executed = walks
                .iter()
                .filter(|w| {
                    // A walk is complete when every prefix subset has ≥ 1
                    // sample — truncated walks are counted, not dropped.
                    let mut held: Vec<String> = Vec::new();
                    let mut subsets: Vec<Vec<String>> = vec![Vec::new()];
                    for t in w.iter() {
                        held.push(t.clone());
                        subsets.push(held.clone());
                    }
                    subsets.iter().all(|s| v(s).is_some())
                })
                .count() as i64;
            // φ_i over the *complete* walks; incomplete walks contribute
            // their measured prefixes only when both S and S∪{i} landed.
            let mut phi: BTreeMap<String, Vec<i64>> = BTreeMap::new();
            let mut pairs: BTreeMap<(String, String), Vec<i64>> = BTreeMap::new();
            for w in &walks {
                let mut held: Vec<String> = Vec::new();
                for (pos, t) in w.iter().enumerate() {
                    let next = {
                        let mut h = held.clone();
                        h.push(t.clone());
                        h
                    };
                    if let (Some(vs), Some(vs1)) = (v(&held), v(&next)) {
                        phi.entry(t.clone()).or_default().push(vs1 - vs);
                        // Pairwise interaction with the predecessor.
                        if pos > 0 {
                            let prev = w[pos - 1].clone();
                            let without_prev: Vec<String> =
                                held.iter().filter(|x| **x != prev).cloned().collect();
                            let mut with_prev = without_prev.clone();
                            with_prev.push(prev.clone());
                            let mut with_both = with_prev.clone();
                            with_both.push(t.clone());
                            if let (Some(a), Some(b), Some(c), Some(dv)) = (
                                v(&without_prev),
                                v(&with_prev),
                                v(&{
                                    let mut h = without_prev.clone();
                                    h.push(t.clone());
                                    h
                                }),
                                v(&with_both),
                            ) {
                                let key = if prev < *t {
                                    (prev.clone(), t.clone())
                                } else {
                                    (t.clone(), prev.clone())
                                };
                                pairs
                                    .entry(key)
                                    .or_default()
                                    .push(dv - c - b + a);
                            }
                
                        }
                    }
                    held = next;
                }
            }
            let v_full = v(&names).unwrap_or(0);
            let v_empty = v(&[]).unwrap_or(0);
            let delta_set = v_full - v_empty;
            let mut values: Vec<Json> = Vec::new();
            let mut phi_sum = 0i64;
            for t in &names {
                let diffs = phi.get(t).cloned().unwrap_or_default();
                let point = stats::mean(&diffs).unwrap_or(0);
                phi_sum += point;
                let iv = paired_interval(&diffs, &format!("shap:{}:{t}", d.seed));
                values.push(Json::obj([
                    ("target", Json::str(t)),
                    ("point", Json::Int(point)),
                    (
                        "interval",
                        iv.map(|i| {
                            Json::obj([
                                ("lo", Json::Int(i.lo)),
                                ("hi", Json::Int(i.hi)),
                            ])
                        })
                        .unwrap_or(Json::Null),
                    ),
                    ("n_walks", Json::Int(diffs.len() as i64)),
                ]));
            }
            let efficiency_check = phi_sum - delta_set;
            let interaction: Vec<Json> = pairs
                .iter()
                .map(|((a, b), xs)| {
                    Json::Arr(vec![
                        Json::str(a),
                        Json::str(b),
                        Json::Int(stats::mean(xs).unwrap_or(0)),
                    ])
                })
                .collect();
            let permutations_completed = walks_executed;
            let budget_truncated = permutations_completed < walks.len() as i64;
            let shapley = Json::obj([
                ("values", Json::Arr(values)),
                ("efficiency_check", Json::Int(efficiency_check)),
                ("interaction_index", Json::Arr(interaction)),
                (
                    "permutations_completed",
                    Json::Int(permutations_completed),
                ),
                ("permutations_requested", Json::Int(walks.len() as i64)),
                ("antithetic", Json::Bool(d.antithetic)),
                ("budget_truncated", Json::Bool(budget_truncated)),
                ("v_empty", Json::Int(v_empty)),
                ("v_full", Json::Int(v_full)),
            ]);
            let (ceil, mut rs) = if any_degraded {
                ("exploratory", reasons.clone())
            } else {
                (label_ceiling, reasons.clone())
            };
            if any_degraded {
                rs.push("degraded_validity".into());
            }
            let mut report =
                base_report(d, estimand, ceil, &rs, spend, planned_rollouts, truncated);
            if let Json::Obj(m) = &mut report {
                m.insert("shapley".into(), shapley);
                if let Some(dt) = d.delta_total {
                    m.insert(
                        "unattributed_share".into(),
                        Json::Int(dt - phi_sum),
                    );
                }
                m.insert("effects".into(), Json::Arr(effects));
            }
            return Ok(finish_report(report));
        }
        "M4" => {
            // Paired-replay interaction — the DiD of per-fork diffs across
            // the second factor's levels, clustered by fork point.
            let mut per_level: BTreeMap<String, Vec<i64>> = BTreeMap::new();
            for level in &d.m4_levels {
                let mut diffs: Vec<i64> = Vec::new();
                for &fp in &all_fps {
                    let mut f_vals: Vec<i64> = Vec::new();
                    let mut c_vals: Vec<i64> = Vec::new();
                    for i in 0..d.k {
                        for (role, sink) in
                            [("factual", &mut f_vals), ("counterfactual", &mut c_vals)]
                        {
                            let key = format!("{level}|{fp}|{role}|{i}");
                            if let Some(o) = by_cell.get(&key) {
                                if let Some(v) = o.outcome {
                                    sink.push(v);
                                }
                                if o.validity == "degraded" {
                                    any_degraded = true;
                                }
                                if o.validity == "invalid" {
                                    any_invalid = true;
                                }
                            }
                        }
                    }
                    if !f_vals.is_empty() && !c_vals.is_empty() {
                        diffs.push(
                            stats::mean(&c_vals).unwrap_or(0) - stats::mean(&f_vals).unwrap_or(0),
                        );
                    }
                }
                per_level.insert(level.clone(), diffs);
            }
            if any_invalid {
                return Err(AttributionError::ReplayInvalid {
                    reason: "an M4 arm is invalid".into(),
                });
            }
            // Fully crossed? An unprobed cell is `unknown` (ADR-0012).
            let probed: Vec<&String> = per_level
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(k, _)| k)
                .collect();
            let unprobed: Vec<&String> = d
                .m4_levels
                .iter()
                .filter(|l| !per_level.get(*l).map(|v| !v.is_empty()).unwrap_or(false))
                .collect();
            let mut interaction_diffs: Vec<i64> = Vec::new();
            if probed.len() >= 2 {
                let a = per_level[probed[0]].clone();
                let b = per_level[probed[1]].clone();
                for (x, y) in a.iter().zip(b.iter()) {
                    interaction_diffs.push(y - x);
                }
            }
            let iv = paired_interval(&interaction_diffs, &format!("m4:{}", d.seed));
            let (ceil, mut rs) = if any_degraded {
                ("exploratory", reasons.clone())
            } else {
                (label_ceiling, reasons.clone())
            };
            if any_degraded {
                rs.push("degraded_validity".into());
            }
            let mut report =
                base_report(d, estimand, ceil, &rs, spend, planned_rollouts, truncated);
            if let Json::Obj(m) = &mut report {
                m.insert(
                    "interaction".into(),
                    Json::obj([
                        ("factor_a", Json::str("harness")),
                        (
                            "factor_b",
                            Json::str(d.m4_factor.as_deref().unwrap_or("environment")),
                        ),
                        ("levels", Json::Arr(d.m4_levels.iter().map(Json::str).collect())),
                        (
                            "point",
                            stats::mean(&interaction_diffs)
                                .map(Json::Int)
                                .unwrap_or(Json::Null),
                        ),
                        (
                            "interval",
                            iv.map(|i| {
                                Json::obj([
                                    ("lo", Json::Int(i.lo)),
                                    ("hi", Json::Int(i.hi)),
                                ])
                            })
                            .unwrap_or(Json::Null),
                        ),
                        (
                            "unprobed",
                            Json::Arr(unprobed.iter().map(|l| Json::str(*l)).collect()),
                        ),
                        ("cluster", Json::str("fork_point")),
                    ]),
                );
                m.insert("effects".into(), Json::Arr(effects));
            }
            return Ok(finish_report(report));
        }
        "M5" => {
            // Mediation split — DE (direct arm) and ME = TE_crn − DE.
            let mut ranked: Vec<(String, i64)> = Vec::new();
            let mut excluded = 0usize;
            let mut total = 0usize;
            for t in &d.targets {
                let (effect, diffs, collapsed) =
                    per_target_effect(t, &all_fps, &["factual", "counterfactual", "direct"], &by_cell);
                let mut e = effect;
                // Recompute DE/ME from the three-arm fold.
                let mut de_vals: Vec<i64> = Vec::new();
                let mut te_vals: Vec<i64> = Vec::new();
                for &fp in &all_fps {
                    let mut f: Vec<i64> = Vec::new();
                    let mut c: Vec<i64> = Vec::new();
                    let mut dir: Vec<i64> = Vec::new();
                    for i in 0..d.k {
                        for (role, sink) in [
                            ("factual", &mut f),
                            ("counterfactual", &mut c),
                            ("direct", &mut dir),
                        ] {
                            let key = format!("{}|{fp}|{role}|{i}", t.ref_);
                            if let Some(o) = by_cell.get(&key) {
                                if let Some(v) = o.outcome {
                                    sink.push(v);
                                }
                                if o.validity == "degraded" {
                                    any_degraded = true;
                                }
                                if o.validity == "invalid" {
                                    any_invalid = true;
                                }
                            }
                        }
                    }
                    if !f.is_empty() && !c.is_empty() && !dir.is_empty() {
                        let fm = stats::mean(&f).unwrap_or(0);
                        de_vals.push(stats::mean(&dir).unwrap_or(0) - fm);
                        te_vals.push(stats::mean(&c).unwrap_or(0) - fm);
                    }
                }
                let de = stats::mean(&de_vals).unwrap_or(0);
                let te = stats::mean(&te_vals).unwrap_or(0);
                let me = te - de;
                if let Json::Obj(m) = &mut e {
                    m.insert("DE".into(), Json::Int(de));
                    m.insert("ME".into(), Json::Int(me));
                    m.insert("TE_crn".into(), Json::Int(te));
                    // Opposite-signed DE/TE ⇒ excluded from the mediated-
                    // share ranking (the degeneracy rule — `inconclusive`,
                    // never `inert`).
                    let opposite = (de > 0 && te < 0) || (de < 0 && te > 0);
                    if opposite {
                        m.insert("mediated_share_excluded".into(), Json::Bool(true));
                        m.insert("verdict".into(), Json::str("inconclusive"));
                        excluded += 1;
                    }
                    total += 1;
                }
                effect_deltas.push(diffs);
                effects.push(e);
                let _ = collapsed;
                ranked.push((t.ref_.clone(), de));
            }
            if any_invalid {
                return Err(AttributionError::ReplayInvalid {
                    reason: "an M5 arm is invalid".into(),
                });
            }
            let (ceil, mut rs) = if any_degraded {
                ("exploratory", reasons.clone())
            } else {
                (label_ceiling, reasons.clone())
            };
            if any_degraded {
                rs.push("degraded_validity".into());
            }
            let mut report =
                base_report(d, estimand, ceil, &rs, spend, planned_rollouts, truncated);
            if let Json::Obj(m) = &mut report {
                m.insert("effects".into(), Json::Arr(effects));
                m.insert(
                    "exclusion_rate".into(),
                    Json::Int(if total == 0 {
                        0
                    } else {
                        (excluded as i64 * PPM) / total as i64
                    }),
                );
            }
            return Ok(finish_report(report));
        }
        _ => {
            // M2 — one `effects[]` row per (target, fork-point); the V10
            // coupling check runs once per target over its cells.
            for t in &d.targets {
                // V10 — a `TE_crn`/`DE` cell consuming an uncoupled source
                // refuses (never silently drops the estimand).
                if estimand == "TE_crn" || estimand == "DE" {
                    let declared: BTreeSet<&str> = d
                        .coupled_sources
                        .iter()
                        .map(|c| c.source.as_str())
                        .collect();
                    for cell in planned.iter().filter(|c| c.target == t.ref_) {
                        if let Some(o) = by_cell.get(&cell.key()) {
                            for src in &o.consumed_sources {
                                if !declared.contains(src.as_str()) {
                                    return Err(AttributionError::CouplingUnavailable {
                                        source: src.clone(),
                                    });
                                }
                            }
                            if o.validity == "invalid" {
                                any_invalid = true;
                            }
                            if o.validity == "degraded" {
                                any_degraded = true;
                            }
                        }
                    }
                } else {
                    for cell in planned.iter().filter(|c| c.target == t.ref_) {
                        if let Some(o) = by_cell.get(&cell.key()) {
                            if o.validity == "invalid" {
                                any_invalid = true;
                            }
                            if o.validity == "degraded" {
                                any_degraded = true;
                            }
                        }
                    }
                }
                let mut diffs: Vec<i64> = Vec::new();
                for &fp in &all_fps {
                    let (mut row, diff, _collapsed) = per_fork_effect(
                        t,
                        fp,
                        &["factual", "counterfactual"],
                        &by_cell,
                    );
                    if let Json::Obj(m) = &mut row {
                        m.insert(
                            "coupled_sources".into(),
                            Json::Arr(
                                d.coupled_sources.iter().map(|c| c.to_json()).collect(),
                            ),
                        );
                    }
                    if let Some(x) = diff {
                        diffs.push(x);
                    }
                    effects.push(row);
                }
                effect_deltas.push(diffs);
            }
        }
    }

    if any_invalid {
        return Err(AttributionError::ReplayInvalid {
            reason: "a branch is invalid".into(),
        });
    }
    if !collapsed_seqs.is_empty()
        && !effects.is_empty()
        && effects
            .iter()
            .all(|e| matches!(e.get("policy_collapsed"), Some(Json::Bool(true))))
    {
        return Err(AttributionError::PolicyCollapsed {
            seq: collapsed_seqs[0],
        });
    }
    if any_degraded {
        label_ceiling = "exploratory";
        reasons.push("degraded_validity".into());
    }
    // `unattributed_share` — the residual against the configuration Δ
    // (never a decomposition claim; only M3 values sum).
    let effect_sum: i64 = effects
        .iter()
        .filter_map(|e| e.get("point").and_then(Json::as_int))
        .sum();
    let unattributed = d.delta_total.map(|dt| dt - effect_sum);
    // Multiplicity — Holm over the pre-registered family, BH otherwise
    // (V12); the p-values ride the per-target sign-flip deltas.
    let raw_ps: Vec<i64> = effect_deltas
        .iter()
        .enumerate()
        .map(|(i, diffs)| {
            stats::permutation_signflip_p(
                diffs,
                500,
                &format!("attr.mult:{}:{i}", d.seed),
            )
            .unwrap_or(PPM)
        })
        .collect();
    let adjusted = if d.pre_registration_ref.is_some() {
        stats::holm(&raw_ps)
    } else {
        stats::benjamini_hochberg(&raw_ps)
    };
    let mut report = base_report(d, estimand, label_ceiling, &reasons, spend, planned_rollouts, truncated);
    if let Json::Obj(m) = &mut report {
        m.insert("effects".into(), Json::Arr(effects));
        if let Some(u) = unattributed {
            m.insert("unattributed_share".into(), Json::Int(u));
        }
        m.insert(
            "multiplicity".into(),
            Json::obj([
                (
                    "family",
                    Json::Int(d.targets.len() as i64),
                ),
                (
                    "adjusted",
                    Json::str(if d.pre_registration_ref.is_some() {
                        "holm"
                    } else {
                        "benjamini_hochberg"
                    }),
                ),
                (
                    "adjusted_ppm",
                    Json::Arr(adjusted.iter().map(|p| Json::Int(*p)).collect()),
                ),
            ]),
        );
        // `locus` — M2 over `every_decision_point`: max{seq : interval
        // excludes 0} under `point_of_commitment` (the `earliest_direct`
        // alternative rides `locus` on TE_crn designs — computed by
        // `locus()`, embedded here for the single-report read).
        if let Some(l) = locus(&Json::Obj(m.clone())) {
            m.insert("locus".into(), l);
        }
    }
    Ok(finish_report(report))
}

/// The shared report skeleton (every member the spec names; method arms
/// add their own sections).
fn base_report(
    d: &AttributionDesign,
    estimand: &str,
    label_ceiling: &str,
    reasons: &[String],
    spend: i64,
    rollouts: i64,
    truncated: bool,
) -> Json {
    let attribution_label = if estimand == "TE_crn" || estimand == "DE" {
        "causal_coupled"
    } else if d.method == "M1" {
        "designed_ablation"
    } else {
        "causal_interventional"
    };
    let mut m = BTreeMap::new();
    m.insert("schema".into(), Json::str(REPORT_SCHEMA));
    m.insert("kind".into(), Json::str("attribution"));
    m.insert("method".into(), Json::str(&d.method));
    m.insert(
        "subject".into(),
        Json::obj([
            (
                "configuration_id",
                Json::str(&d.configuration_id),
            ),
            (
                "run_ids",
                Json::Arr(d.run_ids.iter().map(Json::str).collect()),
            ),
            (
                "task_ids",
                Json::Arr(d.task_ids.iter().map(Json::str).collect()),
            ),
        ]),
    );
    if let Some(r) = &d.design_ref {
        m.insert("design_ref".into(), Json::str(r));
    }
    m.insert(
        "targets".into(),
        Json::Arr(d.targets.iter().map(|t| t.to_json()).collect()),
    );
    m.insert("estimand".into(), Json::str(estimand));
    m.insert(
        "budget".into(),
        Json::obj([
            ("reserved", Json::Int(d.budget_reserved)),
            ("rollouts", Json::Int(rollouts)),
            ("spend", Json::Int(spend)),
            ("charged_to", Json::str("instrument")),
            (
                "budget_truncated",
                Json::Bool(truncated),
            ),
        ]),
    );
    m.insert(
        "assumptions".into(),
        Json::obj([
            (
                "coupling_assumption",
                Json::str(&d.coupling_assumption),
            ),
            (
                "fork_policy",
                Json::str(effective_fork_policy(d)),
            ),
            ("noise_coupling", Json::str(&d.noise_coupling)),
        ]),
    );
    m.insert("label".into(), Json::str(label_ceiling));
    m.insert("attribution_label".into(), Json::str(attribution_label));
    m.insert(
        "provenance".into(),
        Json::obj([("origin", Json::str("instrument"))]),
    );
    m.insert(
        "reasons".into(),
        Json::Arr(reasons.iter().map(Json::str).collect()),
    );
    Json::Obj(m)
}

/// `report_id` — content address over the report sans the id member
/// (I5/KA-I7-11: identical `(design, cells, seed)` mint the same id).
fn finish_report(mut report: Json) -> Json {
    if let Json::Obj(m) = &report {
        let body = Json::Obj(m.clone());
        let id = idp_id(
            "attribution.report",
            body.to_canonical_string().as_bytes(),
        );
        if let Json::Obj(mm) = &mut report {
            mm.insert("report_id".into(), Json::str(id));
        }
    }
    report
}

/// `locus(report)` — the M2 locus (ADR-0200 D4): `point_of_commitment` =
/// the max fork seq whose effect interval excludes 0; under `TE_crn`
/// the `earliest_direct` alternative (the earliest target with a
/// non-zero effect) is computed beside it — disagreement is reported,
/// never resolved. `None` when no interval excludes 0.
pub fn locus(report: &Json) -> Option<Json> {
    let effects = match report.get("effects") {
        Some(Json::Arr(a)) => a,
        _ => return None,
    };
    let mut best: Option<(i64, String)> = None;
    let mut earliest_direct: Option<(i64, String)> = None;
    for e in effects {
        let excludes = e
            .get("interval")
            .and_then(|i| {
                let lo = i.get("lo").and_then(Json::as_int)?;
                let hi = i.get("hi").and_then(Json::as_int)?;
                Some(lo > 0 || hi < 0)
            })
            .unwrap_or(false);
        let target = e
            .get("target")
            .and_then(|t| t.get("ref"))
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        // The fork-point seq the effect's cells rode — the `fork_seq`
        // member (per-cell effects) or the target's `(run, seq)`
        // decision-point spelling.
        let seq = e
            .get("fork_seq")
            .and_then(Json::as_int)
            .or_else(|| {
                e.get("n")
                    .and_then(|n| n.get("max_fork_seq"))
                    .and_then(Json::as_int)
            })
            .or_else(|| {
                e.get("target")
                    .and_then(|t| t.get("ref"))
                    .and_then(Json::as_str)
                    .and_then(|r| r.rsplit(':').next())
                    .and_then(|s| s.parse::<i64>().ok())
            })
            .unwrap_or(0);
        if excludes && best.as_ref().map(|(s, _)| seq > *s).unwrap_or(true) {
            best = Some((seq, target.clone()));
        }
        let point = e.get("point").and_then(Json::as_int).unwrap_or(0);
        if point != 0
            && earliest_direct
                .as_ref()
                .map(|(s, _)| seq < *s)
                .unwrap_or(true)
        {
            earliest_direct = Some((seq, target));
        }
    }
    let (seq, target) = best?;
    let mut m = Json::obj([
        ("seq", Json::Int(seq)),
        ("target", Json::str(&target)),
        ("rule", Json::str("point_of_commitment")),
        ("label", Json::str("heuristic(TE_marg)")),
    ]);
    if let Some((es, et)) = earliest_direct {
        if let Json::Obj(mm) = &mut m {
            mm.insert(
                "earliest_direct".into(),
                Json::obj([
                    ("seq", Json::Int(es)),
                    ("target", Json::str(&et)),
                ]),
            );
            mm.insert("disagreement".into(), Json::Bool(es != seq || et != target));
        }
    }
    Some(m)
}

/// `attribution_quality(delta, reports) → MetricValue`-shaped JSON —
/// `min(1, Σ_targets |effect_t| / |Δ|)` using M3 values where present
/// (they sum by construction) or M1 `LOO`/per-target effects otherwise
/// (ADR-0201 D3): `n/a{not_run}` without reports; `n/a{estimator_undefined}`
/// when Δ's interval includes 0; `n/a{class}` on hosted subjects.
pub fn attribution_quality(delta: &Json, reports: &[Json], hosted: bool) -> Json {
    let na = |reason: &str| -> Json {
        Json::obj([
            ("metric", Json::str("attribution_quality")),
            ("n/a", Json::str(reason)),
        ])
    };
    if hosted {
        return na("class");
    }
    if reports.is_empty() {
        return na("not_run");
    }
    let point = delta.get("point").and_then(Json::as_int).unwrap_or(0);
    let includes_zero = delta
        .get("interval")
        .and_then(|i| {
            let lo = i.get("lo").and_then(Json::as_int)?;
            let hi = i.get("hi").and_then(Json::as_int)?;
            Some(lo <= 0 && hi >= 0)
        })
        .unwrap_or(false);
    if point == 0 || includes_zero {
        return na("estimator_undefined");
    }
    // M3 shapley values where present; else the per-target effect points.
    let mut share: i64 = 0;
    for r in reports {
        if let Some(Json::Arr(values)) = r.get("shapley").and_then(|s| s.get("values")) {
            for v in values {
                share += v
                    .get("point")
                    .and_then(Json::as_int)
                    .unwrap_or(0)
                    .abs();
            }
        } else if let Some(Json::Arr(effects)) = r.get("effects") {
            for e in effects {
                share += e
                    .get("point")
                    .and_then(Json::as_int)
                    .unwrap_or(0)
                    .abs();
            }
        }
    }
    let q = (share * PPM / point.abs().max(1)).min(PPM);
    Json::obj([
        ("metric", Json::str("attribution_quality")),
        ("point", Json::Int(q)),
        ("unit", Json::str("ppm")),
        ("n_reports", Json::Int(reports.len() as i64)),
        ("applies_to_classes", Json::Arr(vec![Json::str("native")])),
        ("headline", Json::Bool(false)),
        ("veto", Json::Bool(false)),
        ("charged_to", Json::str("instrument")),
    ])
}

/// `AttributionHypothesis` — the M0 record (ADR-0199 D1): never a value,
/// never an input to `attribution_quality`; `label = observational`.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributionHypothesis {
    /// The hypothesis id (content-derived).
    pub hypothesis_id: String,
    /// The targets the hypothesis names.
    pub targets: Vec<ComponentTarget>,
    /// `Text{authority ≤ delegate}` — the rationale.
    pub rationale: String,
    /// The `Validator{kind: judge}` or `human` source ref.
    pub source_ref: String,
    /// `judge | human`.
    pub source_kind: String,
}

impl AttributionHypothesis {
    /// `hypothesis(run, source)` — M0's mint; `hypothesis_id` derives
    /// from the content (never authored).
    pub fn mint(
        targets: Vec<ComponentTarget>,
        rationale: &str,
        source_ref: &str,
        source_kind: &str,
    ) -> AttributionHypothesis {
        let material = format!(
            "{:?}|{}|{}|{}",
            targets.iter().map(|t| &t.ref_).collect::<Vec<_>>(),
            rationale,
            source_ref,
            source_kind
        );
        AttributionHypothesis {
            hypothesis_id: idp_id("attribution.hypothesis", material.as_bytes()),
            targets,
            rationale: rationale.to_string(),
            source_ref: source_ref.to_string(),
            source_kind: source_kind.to_string(),
        }
    }

    /// The canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(HYPOTHESIS_SCHEMA)),
            ("hypothesis_id", Json::str(&self.hypothesis_id)),
            (
                "targets",
                Json::Arr(self.targets.iter().map(|t| t.to_json()).collect()),
            ),
            (
                "rationale",
                Json::obj([
                    ("text", Json::str(&self.rationale)),
                    ("authority_ceiling", Json::str("delegate")),
                ]),
            ),
            ("source_ref", Json::str(&self.source_ref)),
            ("source_kind", Json::str(&self.source_kind)),
            ("label", Json::str("observational")),
        ])
    }
}
