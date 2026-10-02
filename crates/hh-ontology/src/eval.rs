//! `eval` — the C0/Stage-1 evaluation-framework vocabulary (spec §5h.2; R-2.9.2
//! whole item; ADR-0045/0046/0047).
//!
//! This module lands the *schema* half of the measurement backbone — never the
//! Stage-3 operations (`compare`, `artifact_benefit`, `render_scorecard`, the
//! interval estimators, deterministic-oracle wiring):
//!
//! - the **shared verdict/oracle vocabulary lowered here** so P7 records may
//!   name it without reaching above the telemetry layer: [`ChargedTo`] (from
//!   `hh-hir::kinds`), [`OracleClass`], [`LatticeValue`], [`VerdictType`] and
//!   [`EvidenceKind`] (from `hh-verification::vocab`). Those crates re-export
//!   the definitions — one spelling per concept, the `Detector`/`ChargedTo`
//!   precedent (CC7);
//! - the full-`MetricDeclaration` support sums: [`Dimension`], [`MetricLevel`],
//!   [`MetricValueType`], [`Direction`], [`ReplicateReducer`],
//!   [`IntervalMethod`], [`OutcomePolicy`]/[`OutcomeClassPolicy`],
//!   [`MediationChannel`]/[`MediationRequirement`], [`EstimatorSelection`];
//! - [`MetricValue`] with the typed `n/a{reason}` value arm — never 0, never a
//!   proxy (T-LCD-15);
//! - [`OracleDeclaration`] — the `validator`-kind registry object (ADR-0047 D1);
//! - [`Design`], [`PreRegistration`], [`Arm`], [`Cell`], [`RunOutcome`] — the
//!   experiment schemas with the `eval_budget` precondition as a schema
//!   constraint (ADR-0046 D1/(e));
//! - [`derive_outcome_class`] — the outcome-class derivation rule over
//!   stop-reason + oracle-failure ledger facts (ADR-0045 D3; ADR-0047 D4).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_provenance::ProvenanceRecord;
use hh_wire::Json;

use crate::compliance::{Detector, NaReason};
use crate::config::{Configuration, Ref};
use crate::control::{OutcomeClass, StopReason};
use crate::participant::{Granularity, Observability};

// ─────────────────────────────────────────────────────────────────────────────
// Shared vocabulary lowered to the ontology layer (CC7 — one spelling each).
// `hh-verification::vocab` and `hh-hir::kinds` re-export these definitions.
// ─────────────────────────────────────────────────────────────────────────────

/// `charged_to ∈ {subject, instrument}` (CF-109) — who a measurement/verdict's
/// spend is billed against. Moved here from `hh-hir::kinds` (the canonical
/// definition); `hh-hir` and `hh-verification` re-export it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChargedTo {
    /// Charged to the subject under test.
    Subject,
    /// Charged to the measuring instrument.
    Instrument,
}

impl ChargedTo {
    /// Canonical name.
    pub fn name(self) -> &'static str {
        self.as_str()
    }

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ChargedTo::Subject => "subject",
            ChargedTo::Instrument => "instrument",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<ChargedTo> {
        match s {
            "subject" => Some(ChargedTo::Subject),
            "instrument" => Some(ChargedTo::Instrument),
            _ => None,
        }
    }
}

/// `OracleClass` — the nine-class oracle taxonomy (ADR-0047 D1). Headline
/// capability at C0 admits only the deterministic set
/// ([`OracleClass::is_c0_headline`]). Moved here from
/// `hh-verification::vocab` (the canonical definition).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OracleClass {
    /// `executable` — an executable oracle.
    Executable,
    /// `end_state` — an end-state oracle.
    EndState,
    /// `output_check` — an output-check oracle.
    OutputCheck,
    /// `trace_predicate` — a trace-predicate oracle.
    TracePredicate,
    /// `protocol_check` — a protocol-check oracle.
    ProtocolCheck,
    /// `judge` — a model judge (C2).
    Judge,
    /// `human` — a human oracle.
    Human,
    /// `teacher_relative` — teacher-relative lift (C4; `provisional`).
    TeacherRelative,
    /// `reference_relative` — the derived three-valued `equivalence_run` class.
    ReferenceRelative,
}

impl OracleClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            OracleClass::Executable => "executable",
            OracleClass::EndState => "end_state",
            OracleClass::OutputCheck => "output_check",
            OracleClass::TracePredicate => "trace_predicate",
            OracleClass::ProtocolCheck => "protocol_check",
            OracleClass::Judge => "judge",
            OracleClass::Human => "human",
            OracleClass::TeacherRelative => "teacher_relative",
            OracleClass::ReferenceRelative => "reference_relative",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<OracleClass> {
        [
            OracleClass::Executable,
            OracleClass::EndState,
            OracleClass::OutputCheck,
            OracleClass::TracePredicate,
            OracleClass::ProtocolCheck,
            OracleClass::Judge,
            OracleClass::Human,
            OracleClass::TeacherRelative,
            OracleClass::ReferenceRelative,
        ]
        .into_iter()
        .find(|c| c.as_str() == s)
    }

    /// The C0 headline set (ADR-0047 D2): the five deterministic classes.
    pub fn is_c0_headline(self) -> bool {
        matches!(
            self,
            OracleClass::Executable
                | OracleClass::EndState
                | OracleClass::OutputCheck
                | OracleClass::TracePredicate
                | OracleClass::ProtocolCheck
        )
    }

    /// The detector class the oracle's verdicts carry.
    pub fn detector(self) -> Detector {
        match self {
            OracleClass::Judge => Detector::Judged,
            OracleClass::Human => Detector::Human,
            _ => Detector::Deterministic,
        }
    }
}

/// `VerdictType` — a declaration's `verdict_type` (ADR-0110 D1; ADR-0047 D1).
/// Moved here from `hh-verification::vocab`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictType {
    /// `bool`.
    Bool,
    /// `graded`.
    Graded,
    /// `lattice{C,I,P,N}` (the `verdict_lattice`).
    Lattice,
    /// `three_valued{pass, fail, inconclusive}`.
    ThreeValued,
    /// `vector`.
    Vector,
}

impl VerdictType {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            VerdictType::Bool => "bool",
            VerdictType::Graded => "graded",
            VerdictType::Lattice => "lattice",
            VerdictType::ThreeValued => "three_valued",
            VerdictType::Vector => "vector",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<VerdictType> {
        [
            VerdictType::Bool,
            VerdictType::Graded,
            VerdictType::Lattice,
            VerdictType::ThreeValued,
            VerdictType::Vector,
        ]
        .into_iter()
        .find(|t| t.as_str() == s)
    }
}

/// `LatticeValue` — the `verdict_lattice{C,I,P,N}` values. Moved here from
/// `hh-verification::vocab`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatticeValue {
    /// `C` — contradicted.
    C,
    /// `I` — inconclusive.
    I,
    /// `P` — partially supported.
    P,
    /// `N` — supported (non-negative).
    N,
}

impl LatticeValue {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            LatticeValue::C => "C",
            LatticeValue::I => "I",
            LatticeValue::P => "P",
            LatticeValue::N => "N",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<LatticeValue> {
        match s {
            "C" => Some(LatticeValue::C),
            "I" => Some(LatticeValue::I),
            "P" => Some(LatticeValue::P),
            "N" => Some(LatticeValue::N),
            _ => None,
        }
    }
}

/// `EvidenceKind` — the closed evidence-kind sum (ADR-0110 D2). `model_io` is
/// judges-only (I-V4). Moved here from `hh-verification::vocab`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceKind {
    /// `end_state` — an environment handle's end state.
    EndState,
    /// `snapshot` — a content-addressed snapshot.
    Snapshot,
    /// `artifact` — a content-addressed artifact.
    Artifact,
    /// `effect_record` — an `action.effect.*` chain.
    EffectRecord,
    /// `observation` — an `Observation` event ref.
    Observation,
    /// `ledger_range` — a ledger range `(run_id, from, to)`.
    LedgerRange,
    /// `world_state_patch` — a content-addressed world-state patch.
    WorldStatePatch,
    /// `external_probe` — a read-only `ToolCapability` probe.
    ExternalProbe,
    /// `model_io` — a model-io event ref — **judges only** (I-V4).
    ModelIo,
    /// `human_attestation` — a human attestation.
    HumanAttestation,
}

impl EvidenceKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EvidenceKind::EndState => "end_state",
            EvidenceKind::Snapshot => "snapshot",
            EvidenceKind::Artifact => "artifact",
            EvidenceKind::EffectRecord => "effect_record",
            EvidenceKind::Observation => "observation",
            EvidenceKind::LedgerRange => "ledger_range",
            EvidenceKind::WorldStatePatch => "world_state_patch",
            EvidenceKind::ExternalProbe => "external_probe",
            EvidenceKind::ModelIo => "model_io",
            EvidenceKind::HumanAttestation => "human_attestation",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<EvidenceKind> {
        [
            EvidenceKind::EndState,
            EvidenceKind::Snapshot,
            EvidenceKind::Artifact,
            EvidenceKind::EffectRecord,
            EvidenceKind::Observation,
            EvidenceKind::LedgerRange,
            EvidenceKind::WorldStatePatch,
            EvidenceKind::ExternalProbe,
            EvidenceKind::ModelIo,
            EvidenceKind::HumanAttestation,
        ]
        .into_iter()
        .find(|k| k.as_str() == s)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The full-`MetricDeclaration` support sums (ADR-0045 D1 + Phase-2/3 logs).
// ─────────────────────────────────────────────────────────────────────────────

/// `Dimension` — the scorecard dimension a metric rolls up to (ADR-0045 D1:
/// the eight dimensions plus `compliance`). Adding a dimension is an ADR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dimension {
    /// `capability`.
    Capability,
    /// `reliability`.
    Reliability,
    /// `grounding`.
    Grounding,
    /// `efficiency`.
    Efficiency,
    /// `autonomy`.
    Autonomy,
    /// `security`.
    Security,
    /// `portability`.
    Portability,
    /// `evolvability`.
    Evolvability,
    /// `compliance`.
    Compliance,
}

impl Dimension {
    /// Every dimension (the fixed nine).
    pub const ALL: [Dimension; 9] = [
        Dimension::Capability,
        Dimension::Reliability,
        Dimension::Grounding,
        Dimension::Efficiency,
        Dimension::Autonomy,
        Dimension::Security,
        Dimension::Portability,
        Dimension::Evolvability,
        Dimension::Compliance,
    ];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Dimension::Capability => "capability",
            Dimension::Reliability => "reliability",
            Dimension::Grounding => "grounding",
            Dimension::Efficiency => "efficiency",
            Dimension::Autonomy => "autonomy",
            Dimension::Security => "security",
            Dimension::Portability => "portability",
            Dimension::Evolvability => "evolvability",
            Dimension::Compliance => "compliance",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<Dimension> {
        Dimension::ALL.into_iter().find(|d| d.as_str() == s)
    }
}

/// `MetricLevel` — the aggregation-ladder level a metric reads at
/// (`replicate → task → configuration → arm → comparison`; ADR-0045 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MetricLevel {
    /// `run`.
    Run,
    /// `task`.
    Task,
    /// `configuration`.
    Configuration,
    /// `arm`.
    Arm,
    /// `comparison`.
    Comparison,
    /// `suite` — a fold over the whole suite's runs (§5g.3 §4
    /// `secret_detector_miss_rate{level: suite}`; S3.11b — additive variant).
    Suite,
}

impl MetricLevel {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            MetricLevel::Run => "run",
            MetricLevel::Task => "task",
            MetricLevel::Configuration => "configuration",
            MetricLevel::Arm => "arm",
            MetricLevel::Comparison => "comparison",
            MetricLevel::Suite => "suite",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<MetricLevel> {
        match s {
            "run" => Some(MetricLevel::Run),
            "task" => Some(MetricLevel::Task),
            "configuration" => Some(MetricLevel::Configuration),
            "arm" => Some(MetricLevel::Arm),
            "comparison" => Some(MetricLevel::Comparison),
            "suite" => Some(MetricLevel::Suite),
            _ => None,
        }
    }
}

/// `MetricValueType` — the declared `value_type` sum
/// `{bool, decimal, verdict{C,I,P,N}, vector, n/a{reason}}` (ADR-0045 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricValueType {
    /// `bool`.
    Bool,
    /// `decimal` — an integer quantity in the declared `unit` (canonical JSON
    /// carries no floats; fractional precision is expressed through the unit —
    /// `ppm`, `micro_units`, `ms`).
    Decimal,
    /// `verdict{C,I,P,N}` — the verdict lattice.
    Verdict,
    /// `vector` — a structured vector (opaque members).
    Vector,
    /// `n/a{reason}` — the metric only ever produces typed `n/a` cells.
    Na,
}

impl MetricValueType {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            MetricValueType::Bool => "bool",
            MetricValueType::Decimal => "decimal",
            MetricValueType::Verdict => "verdict",
            MetricValueType::Vector => "vector",
            MetricValueType::Na => "n/a",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<MetricValueType> {
        match s {
            "bool" => Some(MetricValueType::Bool),
            "decimal" => Some(MetricValueType::Decimal),
            "verdict" => Some(MetricValueType::Verdict),
            "vector" => Some(MetricValueType::Vector),
            "n/a" => Some(MetricValueType::Na),
            _ => None,
        }
    }
}

/// `Direction` — whether a higher or lower reading is better (`direction =
/// higher | lower`; ADR-0045 D1 + the §05g declaration examples).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// `higher` — higher is better.
    Higher,
    /// `lower` — lower is better.
    Lower,
}

impl Direction {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Higher => "higher",
            Direction::Lower => "lower",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<Direction> {
        match s {
            "higher" => Some(Direction::Higher),
            "lower" => Some(Direction::Lower),
            _ => None,
        }
    }
}

/// `ReplicateReducer` — `replicate_reducer ∈ {mean, median, max, at_least(n),
/// pass_at(k), pass_k(k), collect}` (ADR-0045 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicateReducer {
    /// `mean`.
    Mean,
    /// `median`.
    Median,
    /// `max`.
    Max,
    /// `at_least(n)` — at least n replicates pass.
    AtLeast(u32),
    /// `pass_at(k)` — the pass@k estimator (Chen 2021).
    PassAt(u32),
    /// `pass_k(k)` — the pass^k estimator (τ-bench `C(c,k)/C(n,k)`).
    PassK(u32),
    /// `collect` — keep the replicate values (no reduction).
    Collect,
}

impl ReplicateReducer {
    /// The canonical JSON form.
    pub fn to_json(self) -> Json {
        match self {
            ReplicateReducer::Mean => Json::str("mean"),
            ReplicateReducer::Median => Json::str("median"),
            ReplicateReducer::Max => Json::str("max"),
            ReplicateReducer::AtLeast(n) => Json::obj([("at_least", Json::Int(n as i64))]),
            ReplicateReducer::PassAt(k) => Json::obj([("pass_at", Json::Int(k as i64))]),
            ReplicateReducer::PassK(k) => Json::obj([("pass_k", Json::Int(k as i64))]),
            ReplicateReducer::Collect => Json::str("collect"),
        }
    }

    /// Strict decode; `None`/unknown forms refuse.
    pub fn from_json(j: &Json) -> Option<ReplicateReducer> {
        match j {
            Json::Str(s) => match s.as_str() {
                "mean" => Some(ReplicateReducer::Mean),
                "median" => Some(ReplicateReducer::Median),
                "max" => Some(ReplicateReducer::Max),
                "collect" => Some(ReplicateReducer::Collect),
                _ => None,
            },
            Json::Obj(m) if m.len() == 1 => {
                let (k, v) = m.iter().next()?;
                let n = v.as_int()? as u32;
                match k.as_str() {
                    "at_least" => Some(ReplicateReducer::AtLeast(n)),
                    "pass_at" => Some(ReplicateReducer::PassAt(n)),
                    "pass_k" => Some(ReplicateReducer::PassK(n)),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// `BootstrapPairedMethod` — `bootstrap_paired{bca | percentile}` (CF-337).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapPairedMethod {
    /// `bca` — bias-corrected accelerated.
    Bca,
    /// `percentile`.
    Percentile,
}

impl BootstrapPairedMethod {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            BootstrapPairedMethod::Bca => "bca",
            BootstrapPairedMethod::Percentile => "percentile",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<BootstrapPairedMethod> {
        match s {
            "bca" => Some(BootstrapPairedMethod::Bca),
            "percentile" => Some(BootstrapPairedMethod::Percentile),
            _ => None,
        }
    }
}

/// `IntervalMethod` — `interval_method ∈ {clt, clustered_clt(task), wilson,
/// bootstrap, bootstrap_paired{bca | percentile}, bayesian_beta{prior}}`
/// (ADR-0045 D1 + CF-337 growth). `clt` is admissible only via the ADR-0158
/// selection rule and **never** for `unit ∈ {tokens, money, ms}` — the schema
/// half is [`crate::compliance::MetricDeclaration::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntervalMethod {
    /// `clt` — a central-limit interval (restricted: see `validate`).
    Clt,
    /// `clustered_clt(task)` — clustered by task (the task-level default).
    ClusteredClt,
    /// `wilson`.
    Wilson,
    /// `bootstrap`.
    Bootstrap,
    /// `bootstrap_paired{bca | percentile}`.
    BootstrapPaired(BootstrapPairedMethod),
    /// `bayesian_beta{prior}` — a named/pinned prior ref.
    BayesianBeta(String),
}

impl IntervalMethod {
    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        match self {
            IntervalMethod::Clt => Json::str("clt"),
            IntervalMethod::ClusteredClt => Json::obj([("clustered_clt", Json::str("task"))]),
            IntervalMethod::Wilson => Json::str("wilson"),
            IntervalMethod::Bootstrap => Json::str("bootstrap"),
            IntervalMethod::BootstrapPaired(m) => {
                Json::obj([("bootstrap_paired", Json::str(m.as_str()))])
            }
            IntervalMethod::BayesianBeta(prior) => Json::obj([("bayesian_beta", Json::str(prior))]),
        }
    }

    /// Strict decode; unknown forms refuse.
    pub fn from_json(j: &Json) -> Option<IntervalMethod> {
        match j {
            Json::Str(s) => match s.as_str() {
                "clt" => Some(IntervalMethod::Clt),
                "wilson" => Some(IntervalMethod::Wilson),
                "bootstrap" => Some(IntervalMethod::Bootstrap),
                _ => None,
            },
            Json::Obj(m) if m.len() == 1 => {
                let (k, v) = m.iter().next()?;
                match k.as_str() {
                    "clustered_clt" => {
                        (v.as_str()? == "task").then_some(IntervalMethod::ClusteredClt)
                    }
                    "bootstrap_paired" => BootstrapPairedMethod::parse(v.as_str()?)
                        .map(IntervalMethod::BootstrapPaired),
                    "bayesian_beta" => Some(IntervalMethod::BayesianBeta(v.as_str()?.to_string())),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// `EstimatorSelection` — the record every computed interval carries
/// (CF-337; OQ-129; the §6.4 additive bump — S1.24): `declared` (the selected
/// method — the member renamed from `method` at the bump; legacy bodies
/// carrying `method` still decode), `selection_rule`, `floors`,
/// `fallback_chain`, `substituted?{from, to, reason}` (ADR-0158's
/// `T_clt`/`T_bca` floors are the Stage-3 parameters; the schema records
/// which rule ran, the floor values it applied, and any fallback the rule
/// took).
#[derive(Debug, Clone, PartialEq)]
pub struct EstimatorSelection {
    /// The declared (selected) interval method.
    pub method: IntervalMethod,
    /// The selection rule's id (e.g. `adr-0158.clt_floor`).
    pub selection_rule: String,
    /// The floor values the rule applied (`name → threshold`).
    pub floors: BTreeMap<String, u64>,
    /// `fallback_chain` — the methods the rule may fall back through, in
    /// order (§6.4 bump).
    pub fallback_chain: Vec<IntervalMethod>,
    /// `substituted{from, to, reason}` — set when the rule substituted a
    /// fallback for the declared method (§6.4 bump).
    pub substituted: Option<Substitution>,
}

/// `substituted{from, to, reason}` — a selection-rule substitution record
/// (§6.4's `EstimatorSelection` bump; S1.24).
#[derive(Debug, Clone, PartialEq)]
pub struct Substitution {
    /// The method the rule declared.
    pub from: IntervalMethod,
    /// The method it substituted.
    pub to: IntervalMethod,
    /// Why the substitution ran (`floor_unmet`, …).
    pub reason: String,
}

impl EstimatorSelection {
    /// The canonical JSON form — the §6.4 bump emits `declared` (the
    /// renamed `method` member) plus `fallback_chain`/`substituted` when
    /// populated.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("declared".into(), self.method.to_json());
        m.insert("selection_rule".into(), Json::str(&self.selection_rule));
        m.insert(
            "floors".into(),
            Json::Obj(
                self.floors
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                    .collect(),
            ),
        );
        if !self.fallback_chain.is_empty() {
            m.insert(
                "fallback_chain".into(),
                Json::Arr(self.fallback_chain.iter().map(|f| f.to_json()).collect()),
            );
        }
        if let Some(sub) = &self.substituted {
            m.insert(
                "substituted".into(),
                Json::obj([
                    ("from", sub.from.to_json()),
                    ("to", sub.to.to_json()),
                    ("reason", Json::str(&sub.reason)),
                ]),
            );
        }
        Json::Obj(m)
    }

    /// Strict decode — [`EvalError::SchemaViolation`] on missing/unknown
    /// members. `declared` (the bump's name) and the legacy `method` member
    /// both decode; both present must agree.
    pub fn from_json(j: &Json) -> Result<EstimatorSelection, EvalError> {
        const REC: &str = "EstimatorSelection";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "declared",
                "method",
                "selection_rule",
                "floors",
                "fallback_chain",
                "substituted",
            ],
            REC,
        )?;
        let mut floors = BTreeMap::new();
        match m.get("floors") {
            Some(Json::Obj(fm)) => {
                for (k, v) in fm {
                    floors.insert(
                        k.clone(),
                        v.as_int().ok_or_else(|| EvalError::SchemaViolation {
                            member: "floors".into(),
                            detail: "floor values must be ints".into(),
                        })? as u64,
                    );
                }
            }
            _ => {
                return Err(EvalError::SchemaViolation {
                    member: "floors".into(),
                    detail: "must be an object".into(),
                })
            }
        }
        let declared = m.get("declared").or_else(|| m.get("method"));
        let method = declared
            .and_then(|d| {
                IntervalMethod::from_json(d).or({
                    // `declared`/`method` present but not a valid method — the
                    // caller reports the violation below via `None`.
                    None
                })
            })
            .ok_or_else(|| EvalError::SchemaViolation {
                member: "declared".into(),
                detail: "missing or unknown interval method".into(),
            })?;
        // Both spellings present must agree (a record claiming two different
        // methods is malformed).
        if let (Some(d), Some(legacy)) = (m.get("declared"), m.get("method")) {
            let dj = IntervalMethod::from_json(d);
            let lj = IntervalMethod::from_json(legacy);
            if dj != lj {
                return Err(EvalError::SchemaViolation {
                    member: "declared".into(),
                    detail: "`declared` and `method` disagree".into(),
                });
            }
        }
        let fallback_chain = match m.get("fallback_chain") {
            None | Some(Json::Null) => Vec::new(),
            Some(Json::Arr(a)) => {
                let mut chain = Vec::with_capacity(a.len());
                for item in a {
                    chain.push(IntervalMethod::from_json(item).ok_or_else(|| {
                        EvalError::SchemaViolation {
                            member: "fallback_chain".into(),
                            detail: "unknown interval method".into(),
                        }
                    })?);
                }
                chain
            }
            Some(_) => {
                return Err(EvalError::SchemaViolation {
                    member: "fallback_chain".into(),
                    detail: "must be an array".into(),
                })
            }
        };
        let substituted = match m.get("substituted") {
            None | Some(Json::Null) => None,
            Some(sub) => {
                let sm = expect_obj(sub, "substituted")?;
                reject_unknown(sm, &["from", "to", "reason"], "substituted")?;
                Some(Substitution {
                    from: IntervalMethod::from_json(sm.get("from").ok_or_else(|| {
                        EvalError::SchemaViolation {
                            member: "from".into(),
                            detail: "missing member".into(),
                        }
                    })?)
                    .ok_or_else(|| EvalError::SchemaViolation {
                        member: "from".into(),
                        detail: "unknown interval method".into(),
                    })?,
                    to: IntervalMethod::from_json(sm.get("to").ok_or_else(|| {
                        EvalError::SchemaViolation {
                            member: "to".into(),
                            detail: "missing member".into(),
                        }
                    })?)
                    .ok_or_else(|| EvalError::SchemaViolation {
                        member: "to".into(),
                        detail: "unknown interval method".into(),
                    })?,
                    reason: str_at(sm, "reason", "substituted")?.to_string(),
                })
            }
        };
        Ok(EstimatorSelection {
            method,
            selection_rule: str_at(m, "selection_rule", REC)?.to_string(),
            floors,
            fallback_chain,
            substituted,
        })
    }
}

/// `OutcomePolicy` — what a metric's `outcome_class_policy` does with an
/// outcome class (`count_as_failure | exclude | report_beside`; ADR-0045 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutcomePolicy {
    /// `count_as_failure` — in the denominator as a failure.
    CountAsFailure,
    /// `exclude` — out of the denominator entirely.
    Exclude,
    /// `report_beside` — out of the denominator, counted beside it.
    ReportBeside,
}

impl OutcomePolicy {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            OutcomePolicy::CountAsFailure => "count_as_failure",
            OutcomePolicy::Exclude => "exclude",
            OutcomePolicy::ReportBeside => "report_beside",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<OutcomePolicy> {
        match s {
            "count_as_failure" => Some(OutcomePolicy::CountAsFailure),
            "exclude" => Some(OutcomePolicy::Exclude),
            "report_beside" => Some(OutcomePolicy::ReportBeside),
            _ => None,
        }
    }
}

/// `OutcomeClassPolicy` — `map<outcome_class, count_as_failure | exclude |
/// report_beside>` (ADR-0045 D1). `scored` is the denominator itself and never
/// carries an entry; each *non-scored* class's entry decides whether its runs
/// enter the denominator as failures, are excluded, or are counted beside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutcomeClassPolicy {
    /// Per-outcome-class policy. An absent class defaults to [`OutcomePolicy::ReportBeside`]
    /// (excluded and counted beside — the ADR-0045 D3 "other classes" default).
    pub policy: BTreeMap<OutcomeClass, OutcomePolicy>,
}

impl OutcomeClassPolicy {
    /// The capability default (ADR-0045 D3): `scored ∪ budget_exhausted` in the
    /// denominator — `budget_exhausted` counts as failure — with every other
    /// class excluded and counted beside.
    pub fn for_capability() -> OutcomeClassPolicy {
        let mut policy = BTreeMap::new();
        policy.insert(OutcomeClass::BudgetExhausted, OutcomePolicy::CountAsFailure);
        policy.insert(
            OutcomeClass::InfrastructureFailure,
            OutcomePolicy::ReportBeside,
        );
        policy.insert(OutcomeClass::OracleFailure, OutcomePolicy::ReportBeside);
        policy.insert(OutcomeClass::Refused, OutcomePolicy::ReportBeside);
        policy.insert(OutcomeClass::Cancelled, OutcomePolicy::ReportBeside);
        OutcomeClassPolicy { policy }
    }

    /// The efficiency default (ADR-0045 D3): `scored` only — every non-scored
    /// class excluded.
    pub fn for_efficiency() -> OutcomeClassPolicy {
        let mut policy = BTreeMap::new();
        for c in OutcomeClass::ALL {
            if c != OutcomeClass::Scored {
                policy.insert(c, OutcomePolicy::Exclude);
            }
        }
        OutcomeClassPolicy { policy }
    }

    /// The effective policy for a class (`scored` is always in the denominator —
    /// it is never a failure and never excluded).
    pub fn policy_for(&self, class: OutcomeClass) -> OutcomePolicy {
        if class == OutcomeClass::Scored {
            return OutcomePolicy::CountAsFailure;
        }
        self.policy
            .get(&class)
            .copied()
            .unwrap_or(OutcomePolicy::ReportBeside)
    }

    /// `refusal_is_failure` — the metric's declared rule (ADR-0045 D3: `refused`
    /// counts as failure only when the metric declares it).
    pub fn refusal_is_failure(&self) -> bool {
        self.policy_for(OutcomeClass::Refused) == OutcomePolicy::CountAsFailure
    }

    /// Whether a run of `class` enters this metric's denominator.
    pub fn in_denominator(&self, class: OutcomeClass) -> bool {
        class == OutcomeClass::Scored || self.policy_for(class) == OutcomePolicy::CountAsFailure
    }

    /// The canonical JSON form — `map<outcome_class, policy>` as an object.
    pub fn to_json(&self) -> Json {
        Json::Obj(
            self.policy
                .iter()
                .map(|(c, p)| (c.as_str().to_string(), Json::str(p.as_str())))
                .collect(),
        )
    }

    /// Strict decode — [`EvalError::SchemaViolation`] on unknown spellings.
    /// `scored` may not carry an entry (it *is* the denominator).
    pub fn from_json(j: &Json) -> Result<OutcomeClassPolicy, EvalError> {
        const REC: &str = "OutcomeClassPolicy";
        let m = expect_obj(j, REC)?;
        let mut policy = BTreeMap::new();
        for (k, v) in m {
            let class = OutcomeClass::parse(k).ok_or_else(|| EvalError::SchemaViolation {
                member: k.clone(),
                detail: "unknown outcome_class".into(),
            })?;
            if class == OutcomeClass::Scored {
                return Err(EvalError::SchemaViolation {
                    member: k.clone(),
                    detail: "scored never carries an outcome_class_policy entry".into(),
                });
            }
            let p = OutcomePolicy::parse(v.as_str().ok_or_else(|| EvalError::SchemaViolation {
                member: k.clone(),
                detail: "policy must be a string".into(),
            })?)
            .ok_or_else(|| EvalError::SchemaViolation {
                member: k.clone(),
                detail: "unknown outcome policy".into(),
            })?;
            policy.insert(class, p);
        }
        Ok(OutcomeClassPolicy { policy })
    }
}

/// `MediationChannel` — the channels a `mediated(...)` requirement may name
/// (`effects | egress | model_calls`; ADR-0165 D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MediationChannel {
    /// `effects` — effect mediation.
    Effects,
    /// `egress` — egress mediation (the credential broker / EP3 canaries).
    Egress,
    /// `model_calls` — model-call mediation.
    ModelCalls,
}

impl MediationChannel {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            MediationChannel::Effects => "effects",
            MediationChannel::Egress => "egress",
            MediationChannel::ModelCalls => "model_calls",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<MediationChannel> {
        match s {
            "effects" => Some(MediationChannel::Effects),
            "egress" => Some(MediationChannel::Egress),
            "model_calls" => Some(MediationChannel::ModelCalls),
            _ => None,
        }
    }
}

/// `MediationRequirement` — `requires_mediation ∈ {any, observed,
/// mediated(effects | egress | model_calls)}` (default `any`; ADR-0165 D6).
/// `observed` requires some mediation to have been observed on the run;
/// `mediated(ch)` requires channel `ch` to have been mediated — else the cell
/// renders `n/a{mediation}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediationRequirement {
    /// `any` — no mediation requirement (the default).
    Any,
    /// `observed` — at least one channel must have been mediated.
    Observed,
    /// `mediated(ch)` — channel `ch` must have been mediated.
    Mediated(MediationChannel),
}

impl MediationRequirement {
    /// Whether the requirement is satisfied by the set of channels actually
    /// mediated on the run (the applicability half — the other half is the
    /// `n/a{mediation}` the cell renders, never 0).
    pub fn satisfied_by(&self, mediated: &BTreeSet<MediationChannel>) -> bool {
        match self {
            MediationRequirement::Any => true,
            MediationRequirement::Observed => !mediated.is_empty(),
            MediationRequirement::Mediated(ch) => mediated.contains(ch),
        }
    }

    /// The canonical JSON form.
    pub fn to_json(self) -> Json {
        match self {
            MediationRequirement::Any => Json::str("any"),
            MediationRequirement::Observed => Json::str("observed"),
            MediationRequirement::Mediated(ch) => Json::obj([("mediated", Json::str(ch.as_str()))]),
        }
    }

    /// Strict decode; unknown forms refuse.
    pub fn from_json(j: &Json) -> Option<MediationRequirement> {
        match j {
            Json::Str(s) => match s.as_str() {
                "any" => Some(MediationRequirement::Any),
                "observed" => Some(MediationRequirement::Observed),
                _ => None,
            },
            Json::Obj(m) if m.len() == 1 => {
                let (k, v) = m.iter().next()?;
                (k == "mediated")
                    .then(|| {
                        MediationChannel::parse(v.as_str()?).map(MediationRequirement::Mediated)
                    })
                    .flatten()
            }
            _ => None,
        }
    }
}

/// `ModelRole` — the closed role vocabulary a `model_snapshot` factor binds
/// (§3.2.3 per-role binding; ADR-0121 D2/CF-468: `role? ∈ ModelRole` on the
/// experimental-factor record). Moved here from `hh-compiler::profile` (the
/// canonical definition); `hh-compiler` re-exports it — one spelling (CC7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModelRole {
    /// The main loop model.
    Primary,
    /// A cheap auxiliary call.
    Utility,
    /// Context compaction.
    Compaction,
    /// A sub-agent delegation.
    Subagent,
    /// A judge/verifier model.
    Judge,
    /// Router prediction.
    RouterPredictor,
}

impl ModelRole {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ModelRole::Primary => "primary",
            ModelRole::Utility => "utility",
            ModelRole::Compaction => "compaction",
            ModelRole::Subagent => "subagent",
            ModelRole::Judge => "judge",
            ModelRole::RouterPredictor => "router_predictor",
        }
    }

    /// `name` alias — the `hh-compiler` spelling of the same accessor.
    pub fn name(self) -> &'static str {
        self.as_str()
    }

    /// Parse a canonical spelling; `None` on any other input.
    pub fn parse(s: &str) -> Option<ModelRole> {
        match s {
            "primary" => Some(ModelRole::Primary),
            "utility" => Some(ModelRole::Utility),
            "compaction" => Some(ModelRole::Compaction),
            "subagent" => Some(ModelRole::Subagent),
            "judge" => Some(ModelRole::Judge),
            "router_predictor" => Some(ModelRole::RouterPredictor),
            _ => None,
        }
    }
}

/// `FactorKind` — the kind axis of a declared experimental factor
/// (`kind ∈ {model_snapshot, harness, environment, task, budget, replicate}`;
/// spec §5h.2 §3). `replicate` is the seed axis — seed material (harness RNG
/// seed, requested sampling seed, image digest) is recorded per CF-095.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactorKind {
    /// `model_snapshot`.
    ModelSnapshot,
    /// `harness` — carries a `granularity` on the factor declaration.
    Harness,
    /// `environment` — image + fault/perturbation profile.
    Environment,
    /// `task` — with split label.
    Task,
    /// `budget`.
    Budget,
    /// `replicate` — the seed axis.
    Replicate,
}

impl FactorKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            FactorKind::ModelSnapshot => "model_snapshot",
            FactorKind::Harness => "harness",
            FactorKind::Environment => "environment",
            FactorKind::Task => "task",
            FactorKind::Budget => "budget",
            FactorKind::Replicate => "replicate",
        }
    }

    /// Parse a canonical spelling; `None` on any other input.
    pub fn parse(s: &str) -> Option<FactorKind> {
        match s {
            "model_snapshot" => Some(FactorKind::ModelSnapshot),
            "harness" => Some(FactorKind::Harness),
            "environment" => Some(FactorKind::Environment),
            "task" => Some(FactorKind::Task),
            "budget" => Some(FactorKind::Budget),
            "replicate" => Some(FactorKind::Replicate),
            _ => None,
        }
    }
}

/// `FactorLevel{id, ref, label}` — one level of a declared experimental
/// factor (the spec's `Level` record — named `FactorLevel` here so it does
/// not collide with `structure::Level`, the object/instrument standing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactorLevel {
    /// The level id.
    pub id: String,
    /// The content address the level pins.
    pub content_ref: String,
    /// The level's display label.
    pub label: String,
}

impl FactorLevel {
    /// The canonical JSON form `{id, ref, label}`.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("id", Json::str(&self.id)),
            ("ref", Json::str(&self.content_ref)),
            ("label", Json::str(&self.label)),
        ])
    }

    /// Strict decode; `None` on any malformed input.
    pub fn from_json(j: &Json) -> Option<FactorLevel> {
        let m = match j {
            Json::Obj(m) => m,
            _ => return None,
        };
        if m.len() != 3 {
            return None;
        }
        Some(FactorLevel {
            id: m.get("id")?.as_str()?.to_string(),
            content_ref: m.get("ref")?.as_str()?.to_string(),
            label: m.get("label")?.as_str()?.to_string(),
        })
    }
}

/// `FactorDeclaration{name, kind, granularity?, levels[], role?}` — the spec's
/// experimental-`Factor` record (§5h.2 §3; ADR-0045 D2). `granularity` is
/// present only on `kind = harness`; `role` only on `kind = model_snapshot`.
/// (Distinct from [`Factor`](crate::config::Factor), the κ axis sum.)
#[derive(Debug, Clone, PartialEq)]
pub struct FactorDeclaration {
    /// The factor name.
    pub name: String,
    /// The factor kind.
    pub kind: FactorKind,
    /// The granularity a `harness` factor varies at.
    pub granularity: Option<Granularity>,
    /// The declared levels.
    pub levels: Vec<FactorLevel>,
    /// The model role a `model_snapshot` factor binds.
    pub role: Option<ModelRole>,
}

/// `FactorDeclaration` schema failures (typed — never a warning).
#[derive(Debug, Clone, PartialEq)]
pub enum FactorDeclError {
    /// `granularity` on a non-`harness` kind, or `role` on a
    /// non-`model_snapshot` kind.
    MemberOnWrongKind {
        /// The misplaced member name.
        member: String,
    },
    /// `levels[]` is empty — a declared factor carries at least one level.
    NoLevels,
    /// A duplicate level id.
    DuplicateLevel {
        /// The duplicated id.
        level_id: String,
    },
}

impl FactorDeclaration {
    /// The schema checks: `granularity`/`role` only on the kinds that carry
    /// them; at least one level; no duplicate level ids.
    pub fn validate(&self) -> Result<(), FactorDeclError> {
        if self.granularity.is_some() && self.kind != FactorKind::Harness {
            return Err(FactorDeclError::MemberOnWrongKind {
                member: "granularity".into(),
            });
        }
        if self.role.is_some() && self.kind != FactorKind::ModelSnapshot {
            return Err(FactorDeclError::MemberOnWrongKind {
                member: "role".into(),
            });
        }
        if self.levels.is_empty() {
            return Err(FactorDeclError::NoLevels);
        }
        let mut seen = BTreeSet::new();
        for l in &self.levels {
            if !seen.insert(l.id.as_str()) {
                return Err(FactorDeclError::DuplicateLevel {
                    level_id: l.id.clone(),
                });
            }
        }
        Ok(())
    }

    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("name".into(), Json::str(&self.name));
        m.insert("kind".into(), Json::str(self.kind.as_str()));
        if let Some(g) = self.granularity {
            m.insert("granularity".into(), Json::str(g.as_str()));
        }
        m.insert(
            "levels".into(),
            Json::Arr(self.levels.iter().map(FactorLevel::to_json).collect()),
        );
        if let Some(r) = self.role {
            m.insert("role".into(), Json::str(r.as_str()));
        }
        Json::Obj(m)
    }

    /// Strict decode — [`EvalError::SchemaViolation`] on missing/unknown members.
    pub fn from_json(j: &Json) -> Result<FactorDeclaration, EvalError> {
        const REC: &str = "FactorDeclaration";
        let m = expect_obj(j, REC)?;
        reject_unknown(m, &["name", "kind", "granularity", "levels", "role"], REC)?;
        let granularity = match opt_str_at(m, "granularity")? {
            Some(s) => Some(
                Granularity::parse(s).ok_or_else(|| EvalError::SchemaViolation {
                    member: "granularity".into(),
                    detail: "unknown granularity".into(),
                })?,
            ),
            None => None,
        };
        let role = match opt_str_at(m, "role")? {
            Some(s) => Some(
                ModelRole::parse(s).ok_or_else(|| EvalError::SchemaViolation {
                    member: "role".into(),
                    detail: "unknown model role".into(),
                })?,
            ),
            None => None,
        };
        let mut levels = Vec::new();
        for lv in arr_at(m, "levels", REC)? {
            levels.push(
                FactorLevel::from_json(lv).ok_or_else(|| EvalError::SchemaViolation {
                    member: "levels".into(),
                    detail: "malformed level".into(),
                })?,
            );
        }
        Ok(FactorDeclaration {
            name: str_at(m, "name", REC)?.to_string(),
            kind: FactorKind::parse(str_at(m, "kind", REC)?).ok_or_else(|| {
                EvalError::SchemaViolation {
                    member: "kind".into(),
                    detail: "unknown factor kind".into(),
                }
            })?,
            granularity,
            levels,
            role,
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `MetricValue` + the typed `n/a{reason}` value (ADR-0045 D1/D9; ADR-0047 D4).
// ─────────────────────────────────────────────────────────────────────────────

/// `MetricValueKind` — the typed `value` member of a [`MetricValue`]
/// (`bool | decimal | verdict | vector | n/a{reason}`; ADR-0045 D1). The `n/a`
/// arm is a first-class typed value — never 0, never a configuration-level
/// proxy (T-LCD-15).
#[derive(Debug, Clone, PartialEq)]
pub enum MetricValueKind {
    /// `bool`.
    Bool(bool),
    /// `decimal` — an integer quantity in the declaration's `unit` (canonical
    /// JSON carries no floats; precision is expressed through the unit).
    Decimal(i64),
    /// `verdict{C,I,P,N}` — a verdict-lattice value.
    Verdict(LatticeValue),
    /// `vector` — a structured vector (opaque members).
    Vector(Json),
    /// `n/a{reason}` — the typed not-applicable value.
    Na(NaReason),
}

impl MetricValueKind {
    /// The `value_type` tag this kind realises.
    pub fn value_type(&self) -> MetricValueType {
        match self {
            MetricValueKind::Bool(_) => MetricValueType::Bool,
            MetricValueKind::Decimal(_) => MetricValueType::Decimal,
            MetricValueKind::Verdict(_) => MetricValueType::Verdict,
            MetricValueKind::Vector(_) => MetricValueType::Vector,
            MetricValueKind::Na(_) => MetricValueType::Na,
        }
    }

    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        match self {
            MetricValueKind::Bool(b) => {
                Json::obj([("kind", Json::str("bool")), ("value", Json::Bool(*b))])
            }
            MetricValueKind::Decimal(d) => {
                Json::obj([("kind", Json::str("decimal")), ("value", Json::Int(*d))])
            }
            MetricValueKind::Verdict(v) => Json::obj([
                ("kind", Json::str("verdict")),
                ("value", Json::str(v.as_str())),
            ]),
            MetricValueKind::Vector(v) => {
                Json::obj([("kind", Json::str("vector")), ("value", v.clone())])
            }
            MetricValueKind::Na(r) => {
                Json::obj([("kind", Json::str("na")), ("reason", Json::str(r.as_str()))])
            }
        }
    }

    /// Strict decode; unknown kinds refuse.
    pub fn from_json(j: &Json) -> Option<MetricValueKind> {
        let m = expect_obj(j, "MetricValueKind").ok()?;
        let kind = str_at(m, "kind", "MetricValueKind").ok()?;
        Some(match kind {
            "bool" => MetricValueKind::Bool(bool_at(m, "value", "MetricValueKind").ok()?),
            "decimal" => MetricValueKind::Decimal(int_at(m, "value", "MetricValueKind").ok()?),
            "verdict" => MetricValueKind::Verdict(LatticeValue::parse(
                str_at(m, "value", "MetricValueKind").ok()?,
            )?),
            "vector" => MetricValueKind::Vector(m.get("value").cloned().unwrap_or(Json::Null)),
            "na" => MetricValueKind::Na(NaReason::parse(
                str_at(m, "reason", "MetricValueKind").ok()?,
            )?),
            _ => return None,
        })
    }
}

/// `MetricValue` — `measurement.metric.emitted{metric_ref, value, applies_to,
/// oracle_ref, detector, confidence, evidence_ref}` (spec §5h.2 §3; ADR-0045
/// D9 scorer boundary). One per `(run, metric)`; every value references the
/// oracle that produced it (ADR-0047 D1) and carries its `detector` provenance
/// (a judged value is never merged with a deterministic one — ADR-0047 D2).
/// `outcome_class` is derived into the `results_row` projection (R-2.10.5),
/// never stored on the value.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricValue {
    /// The `MetricDeclaration`'s registry ref/name.
    pub metric_ref: String,
    /// The typed value.
    pub value: MetricValueKind,
    /// What the value applies to — the `(run, metric)` cell coordinate (a
    /// run/task/configuration ref; the §5h.1 `subject` member's §5h.2 name).
    pub applies_to: String,
    /// The oracle that produced the value.
    pub oracle_ref: String,
    /// The detector provenance.
    pub detector: Detector,
    /// The value's confidence in ppm, when declared (judged/estimated values).
    pub confidence: Option<u64>,
    /// The evidence ref the value cites, when one exists.
    pub evidence_ref: Option<String>,
}

impl MetricValue {
    /// Whether the declaration admits this value's oracle class (the
    /// `oracle_classes_allowed` bound — ADR-0047 D1). An empty admitted set
    /// admits nothing (the declaration is invalid — `validate` refuses it).
    pub fn oracle_admitted(
        oracle_classes_allowed: &BTreeSet<OracleClass>,
        class: OracleClass,
    ) -> bool {
        oracle_classes_allowed.contains(&class)
    }

    /// The canonical JSON form (the `measurement.metric.emitted` core).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("metric_ref".into(), Json::str(&self.metric_ref));
        m.insert("value".into(), self.value.to_json());
        m.insert("applies_to".into(), Json::str(&self.applies_to));
        m.insert("oracle_ref".into(), Json::str(&self.oracle_ref));
        m.insert("detector".into(), Json::str(self.detector.as_str()));
        if let Some(c) = self.confidence {
            m.insert("confidence".into(), Json::Int(c as i64));
        }
        if let Some(e) = &self.evidence_ref {
            m.insert("evidence_ref".into(), Json::str(e));
        }
        Json::Obj(m)
    }

    /// Strict decode — [`EvalError::SchemaViolation`] on missing/unknown members.
    pub fn from_json(j: &Json) -> Result<MetricValue, EvalError> {
        const REC: &str = "MetricValue";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "metric_ref",
                "value",
                "applies_to",
                "oracle_ref",
                "detector",
                "confidence",
                "evidence_ref",
            ],
            REC,
        )?;
        let detector = Detector::parse(str_at(m, "detector", REC)?).ok_or_else(|| {
            EvalError::SchemaViolation {
                member: "detector".to_string(),
                detail: "unknown detector spelling".to_string(),
            }
        })?;
        Ok(MetricValue {
            metric_ref: str_at(m, "metric_ref", REC)?.to_string(),
            value: MetricValueKind::from_json(m.get("value").ok_or_else(|| {
                EvalError::SchemaViolation {
                    member: "value".to_string(),
                    detail: "missing member".to_string(),
                }
            })?)
            .ok_or_else(|| EvalError::SchemaViolation {
                member: "value".to_string(),
                detail: "unknown value kind".to_string(),
            })?,
            applies_to: str_at(m, "applies_to", REC)?.to_string(),
            oracle_ref: str_at(m, "oracle_ref", REC)?.to_string(),
            detector,
            confidence: opt_int_at(m, "confidence")?.map(|c| c as u64),
            evidence_ref: opt_str_at(m, "evidence_ref")?.map(str::to_string),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `Design` / `PreRegistration` / `Arm` / `Cell` / `RunOutcome` (ADR-0045 D2;
// ADR-0046 D1).
// ─────────────────────────────────────────────────────────────────────────────

/// `DesignKind` — `kind ∈ {paired, full_factorial, fractional_factorial,
/// one_factor_at_a_time, adaptive_search}` (ADR-0045 D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesignKind {
    /// `paired` — a paired Δ design.
    Paired,
    /// `full_factorial`.
    FullFactorial,
    /// `fractional_factorial`.
    FractionalFactorial,
    /// `one_factor_at_a_time`.
    OneFactorAtATime,
    /// `adaptive_search` — admissible only inside `search_budget` (ADR-0046 D6).
    AdaptiveSearch,
}

impl DesignKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DesignKind::Paired => "paired",
            DesignKind::FullFactorial => "full_factorial",
            DesignKind::FractionalFactorial => "fractional_factorial",
            DesignKind::OneFactorAtATime => "one_factor_at_a_time",
            DesignKind::AdaptiveSearch => "adaptive_search",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<DesignKind> {
        match s {
            "paired" => Some(DesignKind::Paired),
            "full_factorial" => Some(DesignKind::FullFactorial),
            "fractional_factorial" => Some(DesignKind::FractionalFactorial),
            "one_factor_at_a_time" => Some(DesignKind::OneFactorAtATime),
            "adaptive_search" => Some(DesignKind::AdaptiveSearch),
            _ => None,
        }
    }
}

/// `resolution ∈ {III, IV, V}` — the fractional-factorial resolution class
/// (§6.3 §2.1; OQ-361's ratified default admits two-level `l^(k−p)` generators
/// only). Resolution III admits main effects only (a pre-registered two-factor
/// interaction under III is `ResolutionInsufficient`); IV admits 2FIs aliased
/// with higher-order terms; V is mutually unconfounded 2FIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FractionalResolution {
    /// `III` — main effects only.
    III,
    /// `IV` — two-factor interactions estimable (aliased with ≥3-factor terms).
    IV,
    /// `V` — two-factor interactions mutually unconfounded.
    V,
}

impl FractionalResolution {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            FractionalResolution::III => "III",
            FractionalResolution::IV => "IV",
            FractionalResolution::V => "V",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<FractionalResolution> {
        match s {
            "III" => Some(FractionalResolution::III),
            "IV" => Some(FractionalResolution::IV),
            "V" => Some(FractionalResolution::V),
            _ => None,
        }
    }

    /// Whether the resolution estimates a two-factor interaction unconfounded
    /// enough to satisfy a pre-registered 2FI (≥ IV per §6.3 §2.1).
    pub fn admits_two_factor_interaction(self) -> bool {
        self >= FractionalResolution::IV
    }
}

/// `Pairing` — `pairing ∈ {by_task, by_task_and_replicate}` (ADR-0045 D2;
/// pairing across arms is by task, by replicate only where all participants
/// declare `seed_honoured` — CF-095).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pairing {
    /// `by_task`.
    ByTask,
    /// `by_task_and_replicate`.
    ByTaskAndReplicate,
}

impl Pairing {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Pairing::ByTask => "by_task",
            Pairing::ByTaskAndReplicate => "by_task_and_replicate",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<Pairing> {
        match s {
            "by_task" => Some(Pairing::ByTask),
            "by_task_and_replicate" => Some(Pairing::ByTaskAndReplicate),
            _ => None,
        }
    }
}

/// `SeedPolicy{harness_rng, requested_sampling_seed, seed_honoured_required}` —
/// the design's declared seed sources and whether `by_task_and_replicate`
/// pairing may be claimed (ADR-0045 D2; CF-095).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedPolicy {
    /// Whether replicate seed material includes the harness RNG seed.
    pub harness_rng: bool,
    /// Whether replicate seed material includes the requested sampling seed.
    pub requested_sampling_seed: bool,
    /// Whether by-replicate pairing requires all participants to honour seeds.
    pub seed_honoured_required: bool,
}

/// `PreRegistration{registered_at, hypothesis, primary_metrics[],
/// equivalence_margin?, min_n, analysis_plan_ref, task_split_hash,
/// interactions?[]}` (spec §5h.2 §3; ADR-0045 D2) — first-class and immutable
/// once the experiment's first run opens; the only source of margins for
/// `reproduce`/`retirement`.
///
/// `interactions[]` (S3.4a; additive) names the pre-registered effect terms a
/// fractional design must estimate unconfounded — each entry is an effect
/// spelling (`"factor_a:factor_b"` for a two-factor interaction). A
/// `fractional_factorial` design whose declared `resolution` cannot estimate a
/// named interaction unconfounded is refused `ResolutionInsufficient` at
/// `register` (§6.3 §2.1 — any 2FI needs ≥ IV; mutually unconfounded 2FIs
/// need V).
#[derive(Debug, Clone, PartialEq)]
pub struct PreRegistration {
    /// The registration's logical time (the transaction `seq`, never a wall clock).
    pub registered_at: u64,
    /// The registered hypothesis.
    pub hypothesis: String,
    /// The pre-registered primary metric names.
    pub primary_metrics: Vec<String>,
    /// The pre-registered equivalence margin (a typed margin — the record
    /// carries it as data; the estimator interprets it at Stage 3).
    pub equivalence_margin: Option<Json>,
    /// The minimum n per cell.
    pub min_n: u32,
    /// The pinned analysis-plan ref.
    pub analysis_plan_ref: String,
    /// The `sha256:` task-split hash — it must predate the search (ADR-0143 L3).
    pub task_split_hash: String,
    /// The pre-registered interaction terms (`["a:b", …]`; S3.4a — the
    /// `ResolutionInsufficient` check's input).
    pub interactions: Vec<String>,
}

/// `Design{id, kind, factors[], blocking, replicates_per_cell, pairing,
/// seed_policy, held_out_split_ref?, pre_registration, registry_snapshot_id?,
/// generators?, resolution?}` (spec §5h.2 §3; ADR-0045 D2) —
/// content-addressed, immutable once the experiment's first run opens.
///
/// S3.4a additions (additive, optional): `registry_snapshot_id` pins the one
/// registry snapshot the design resolves levels against (§6.2 "one snapshot
/// per `Design`"; §6.3 consumes it for `DependsOnDriftedCapability` and every
/// bundle carries it); `generators[]` + `resolution` are the
/// `fractional_factorial` parameters (mandatory for that kind —
/// `ResolutionInsufficient`/`Schema` at `register` otherwise).
#[derive(Debug, Clone, PartialEq)]
pub struct Design {
    /// The design id.
    pub id: String,
    /// The design kind.
    pub kind: DesignKind,
    /// The declared experimental factors the design varies.
    pub factors: Vec<FactorDeclaration>,
    /// The blocking factors (`[task]` by default — ADR-0045 D2).
    pub blocking: Vec<String>,
    /// `replicates_per_cell ≥ 1` (a zero replicate count is refused).
    pub replicates_per_cell: u32,
    /// The pairing rule.
    pub pairing: Pairing,
    /// The seed policy.
    pub seed_policy: SeedPolicy,
    /// The held-out split ref, when one is declared.
    pub held_out_split_ref: Option<String>,
    /// The mandatory pre-registration.
    pub pre_registration: PreRegistration,
    /// The pinned registry snapshot the design resolves against (§6.2).
    pub registry_snapshot_id: Option<String>,
    /// The `fractional_factorial` generators (`["E=ABCD"]` — two-level
    /// `l^(k−p)` spellings; mandatory on `fractional_factorial`).
    pub generators: Option<Vec<String>>,
    /// The declared `fractional_factorial` resolution (mandatory on
    /// `fractional_factorial`; refused on other kinds).
    pub resolution: Option<FractionalResolution>,
    /// `routing_policy` — `fail_fast` (the pre-registered default: every
    /// role's `fallback_chain = []`, so a provider failure yields
    /// `outcome_class = infrastructure_failure` rather than a silent model
    /// change — AC-R-2.3.2-9; ADR-0122 d.5) or `route{policy_ref}` naming
    /// the routing policy whose fallback chains the arms bind.
    pub routing_policy: RoutingPolicy,
    /// `deviation_policy ∈ {exclude_deviated, stratify, pool_with_flag}` —
    /// the pre-registered pooling policy for `routing.deviation` rows
    /// (AC-R-2.3.2-8; ADR-0122 d.5); `None` = no declared policy, and the
    /// report generator refuses to pool deviated and non-deviated rows.
    pub deviation_policy: Option<DeviationPolicy>,
    /// `cache_na_stratified` — the design declares that cache metrics
    /// stratify `n/a` rows (admits a `natural` arm on a dialect with
    /// `cache_state_visible = unsupported`; AC-R-2.3.4-10; ADR-0128 d.5).
    pub cache_na_stratified: bool,
}

/// `routing_policy` — the design's declared routing posture (§5h.2;
/// ADR-0122 d.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingPolicy {
    /// `fail_fast` — every role's `fallback_chain = []`; a provider failure
    /// surfaces as `outcome_class = infrastructure_failure`, never a silent
    /// model change. The pre-registered default.
    FailFast,
    /// `route{policy_ref}` — the design binds a named routing policy (its
    /// fallback chains may produce `routing.deviation` rows).
    Route(String),
}

impl RoutingPolicy {
    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        match self {
            RoutingPolicy::FailFast => Json::str("fail_fast"),
            RoutingPolicy::Route(r) => Json::obj([("route", Json::str(r))]),
        }
    }

    /// Strict decode; `None` member input decodes the `fail_fast` default.
    pub fn from_json(j: Option<&Json>) -> Result<RoutingPolicy, EvalError> {
        match j {
            None | Some(Json::Null) => Ok(RoutingPolicy::FailFast),
            Some(Json::Str(s)) if s == "fail_fast" => Ok(RoutingPolicy::FailFast),
            Some(Json::Obj(m)) if m.len() == 1 => {
                if let Some(Json::Str(r)) = m.get("route") {
                    Ok(RoutingPolicy::Route(r.clone()))
                } else {
                    Err(EvalError::SchemaViolation {
                        member: "routing_policy".into(),
                        detail: "expected `route{policy_ref}`".into(),
                    })
                }
            }
            Some(_) => Err(EvalError::SchemaViolation {
                member: "routing_policy".into(),
                detail: "expected `fail_fast` or `route{policy_ref}`".into(),
            }),
        }
    }
}

/// `deviation_policy ∈ {exclude_deviated, stratify, pool_with_flag}` — the
/// pre-registered pooling policy for `routing.deviation` rows (ADR-0122 d.5;
/// AC-R-2.3.2-8). Absent = the report generator refuses to pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviationPolicy {
    /// Deviated runs are excluded from the pooled estimate (counted beside —
    /// never silently dropped).
    ExcludeDeviated,
    /// The comparison stratifies: one estimate per deviation stratum, no
    /// pooled estimate.
    Stratify,
    /// Deviated and non-deviated rows pool under an explicit flag.
    PoolWithFlag,
}

impl DeviationPolicy {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DeviationPolicy::ExcludeDeviated => "exclude_deviated",
            DeviationPolicy::Stratify => "stratify",
            DeviationPolicy::PoolWithFlag => "pool_with_flag",
        }
    }

    /// Parse a canonical spelling; `None` on any other input.
    pub fn parse(s: &str) -> Option<DeviationPolicy> {
        match s {
            "exclude_deviated" => Some(DeviationPolicy::ExcludeDeviated),
            "stratify" => Some(DeviationPolicy::Stratify),
            "pool_with_flag" => Some(DeviationPolicy::PoolWithFlag),
            _ => None,
        }
    }
}

/// `Design`/`PreRegistration` schema failures (typed — never a warning).
#[derive(Debug, Clone, PartialEq)]
pub enum DesignError {
    /// `replicates_per_cell = 0`.
    ReplicatesZero,
    /// `min_n = 0` in the pre-registration.
    MinNZero,
    /// A duplicate factor name in `factors[]`.
    DuplicateFactor {
        /// The duplicated factor name.
        factor: String,
    },
    /// A factor declaration that failed its own schema checks.
    FactorInvalid {
        /// The factor name.
        factor: String,
        /// The failure detail.
        detail: String,
    },
    /// `pre_registration` absent (the field is mandatory — the arm is declared
    /// defensively against a malformed body).
    MissingPreRegistration,
}

impl Design {
    /// The schema checks: `replicates_per_cell ≥ 1`, `min_n ≥ 1`, every factor
    /// declaration valid, no duplicate factor names.
    pub fn validate(&self) -> Result<(), DesignError> {
        if self.replicates_per_cell == 0 {
            return Err(DesignError::ReplicatesZero);
        }
        if self.pre_registration.min_n == 0 {
            return Err(DesignError::MinNZero);
        }
        let mut seen = BTreeSet::new();
        for f in &self.factors {
            f.validate().map_err(|e| DesignError::FactorInvalid {
                factor: f.name.clone(),
                detail: format!("{e:?}"),
            })?;
            if !seen.insert(f.name.as_str()) {
                return Err(DesignError::DuplicateFactor {
                    factor: f.name.clone(),
                });
            }
        }
        Ok(())
    }
}

/// `SearchBudgetRecord{rollouts, model_calls, tokens, feedback_labels_used,
/// judge_calls, wall_clock, allocation: map<component|slot, share>,
/// adaptive_selection?}` (ADR-0046 D1) — covers *everything* spent to produce
/// an arm's frozen artifact. `allocation` shares are ppm and must sum to
/// `1_000_000` for the record to be *complete*.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchBudgetRecord {
    /// Rollouts spent.
    pub rollouts: u64,
    /// Model calls spent.
    pub model_calls: u64,
    /// Tokens spent.
    pub tokens: u64,
    /// Feedback labels used.
    pub feedback_labels_used: u64,
    /// Judge calls spent.
    pub judge_calls: u64,
    /// Wall-clock ms spent.
    pub wall_clock: u64,
    /// `component|slot → share` (ppm; must sum to `1_000_000`).
    pub allocation: BTreeMap<String, u64>,
    /// Whether adaptive/reduced selection was used inside the budget
    /// (ADR-0046 D6 — admissible only inside `search_budget`).
    pub adaptive_selection: bool,
}

/// Parts-per-million scale (the `allocation` share unit).
pub const PPM_SCALE: u64 = 1_000_000;

impl SearchBudgetRecord {
    /// Whether the record is *complete*: every spend member populated and the
    /// `allocation` shares summing to `1_000_000` ppm (ADR-0046 D1's
    /// "`search_budget` is complete or `unknown`").
    pub fn is_complete(&self) -> bool {
        !self.allocation.is_empty() && self.allocation.values().sum::<u64>() == PPM_SCALE
    }
}

/// `Arm{id, hypothesis, design_ref, configurations[], eval_budget,
/// search_budget, artifact_ref, provenance}` (spec §5h.2 §3; ADR-0046 D1).
///
/// `eval_budget` carries the pinned budget vector's **identity** (a [`Ref`]) —
/// the §8.2-owned vector body lives on the budget plane; the arm pins it the
/// way `Configuration.b` does. The `eval_budget` precondition is a *schema
/// constraint* at Stage 1: [`Arm::validate`] refuses an arm whose
/// configurations' budget vectors are not identical to `eval_budget`
/// (`BudgetMismatchWithinArm`). The engine precondition over live spend is
/// Stage 3 (`UnmatchedBudget`).
#[derive(Debug, Clone, PartialEq)]
pub struct Arm {
    /// The arm id.
    pub id: String,
    /// The arm's hypothesis.
    pub hypothesis: String,
    /// The design the arm belongs to.
    pub design_ref: String,
    /// The arm's configurations.
    pub configurations: Vec<Configuration>,
    /// `eval_budget` — the pinned per-run hard-limits budget vector.
    pub eval_budget: Ref,
    /// `search_budget` — complete or `unknown` (`None`; then the arm appears
    /// only in product-level comparisons flagged `search_unknown`).
    pub search_budget: Option<SearchBudgetRecord>,
    /// The arm's frozen artifact — a sealed definition or a product version.
    pub artifact_ref: Ref,
    /// The arm's provenance.
    pub provenance: ProvenanceRecord,
}

/// `declare_arm` failures (spec §5h.2 §2: `BudgetMissing |
/// BudgetMismatchWithinArm | ArtifactUnsealed`).
#[derive(Debug, Clone, PartialEq)]
pub enum ArmError {
    /// `search_budget` absent (or incomplete) outside a product-level
    /// comparison.
    BudgetMissing,
    /// A configuration's `budget_vector` is not `eval_budget` (dimension-wise
    /// equality over the §08 kernel dimension list — identity equality on the
    /// pinned vector).
    BudgetMismatchWithinArm {
        /// The offending configuration's budget semantic id.
        configuration_budget: String,
    },
    /// `artifact_ref` is not a sealed definition / product version — its
    /// `version_id` is not a pinned content address.
    ArtifactUnsealed,
}

/// Whether `version_id` is a pinned content address (`<algorithm>:<hex>` —
/// the idp/N form). A selector or display name is unsealed.
fn is_pinned_version_id(version_id: &str) -> bool {
    match version_id.split_once(':') {
        Some((algo, digest)) => !algo.is_empty() && !digest.is_empty(),
        None => false,
    }
}

impl Arm {
    /// The `declare_arm` schema checks (spec §5h.2 §2's postcondition):
    /// `artifact_ref` sealed; `search_budget` complete or `unknown` (legal
    /// only for `product`-level comparisons — `granularity`); every
    /// configuration's `budget_vector` equals `eval_budget` dimension-wise
    /// (identity equality — the pinned vector's semantic id).
    pub fn validate(&self, granularity: Granularity) -> Result<(), ArmError> {
        if !is_pinned_version_id(&self.artifact_ref.version_id) {
            return Err(ArmError::ArtifactUnsealed);
        }
        match &self.search_budget {
            Some(sb) if !sb.is_complete() => return Err(ArmError::BudgetMissing),
            None if granularity != Granularity::ProductLevel => {
                return Err(ArmError::BudgetMissing);
            }
            _ => {}
        }
        for cfg in &self.configurations {
            if cfg.b.semantic_id != self.eval_budget.semantic_id {
                return Err(ArmError::BudgetMismatchWithinArm {
                    configuration_budget: cfg.b.semantic_id.clone(),
                });
            }
        }
        Ok(())
    }
}

/// `Cell = (Arm, Configuration, Task)` — one experiment cell (ADR-0045 D2).
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// The arm id.
    pub arm_id: String,
    /// The cell's configuration.
    pub configuration: Configuration,
    /// The task ref.
    pub task_ref: String,
}

/// `RunOutcome{run_id, outcome_class, stop_reason, budget_consumed,
/// veto_tripped[]}` (spec §5h.2 §3) — a view over `lifecycle.run.finished`,
/// `control.budget.*`, `security.*`, `action.effect.*`.
#[derive(Debug, Clone, PartialEq)]
pub struct RunOutcome {
    /// The run id.
    pub run_id: String,
    /// The derived outcome class (derived, never authored — see
    /// [`derive_outcome_class`]).
    pub outcome_class: OutcomeClass,
    /// The run's stop reason.
    pub stop_reason: StopReason,
    /// The consumed budget vector (`dimension → consumed`; a §8.2 quantity map).
    pub budget_consumed: BTreeMap<String, i64>,
    /// The veto invariants tripped (metric names; empty for a clean run).
    pub veto_tripped: Vec<String>,
}

/// `derive_outcome_class(stop_reason, oracle_failed)` — the outcome-class
/// derivation rule (ADR-0045 D3; ADR-0047 D4): `outcome_class` is derived
/// from ledger facts for every run — an oracle failure (crash, timeout,
/// unparseable, non-finite, refused) yields `oracle_failure`, never a score;
/// otherwise the class projects the stop reason (`StopReason::outcome_class`).
pub fn derive_outcome_class(stop_reason: &StopReason, oracle_failed: bool) -> OutcomeClass {
    if oracle_failed {
        OutcomeClass::OracleFailure
    } else {
        stop_reason.outcome_class()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Canonical codecs for the experiment records (CC3 — content-addressed
// records round-trip byte-exact; unknown members refuse).
// ─────────────────────────────────────────────────────────────────────────────

impl SeedPolicy {
    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("harness_rng", Json::Bool(self.harness_rng)),
            (
                "requested_sampling_seed",
                Json::Bool(self.requested_sampling_seed),
            ),
            (
                "seed_honoured_required",
                Json::Bool(self.seed_honoured_required),
            ),
        ])
    }

    /// Strict decode — [`EvalError::SchemaViolation`] on missing/unknown members.
    pub fn from_json(j: &Json) -> Result<SeedPolicy, EvalError> {
        const REC: &str = "SeedPolicy";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "harness_rng",
                "requested_sampling_seed",
                "seed_honoured_required",
            ],
            REC,
        )?;
        Ok(SeedPolicy {
            harness_rng: bool_at(m, "harness_rng", REC)?,
            requested_sampling_seed: bool_at(m, "requested_sampling_seed", REC)?,
            seed_honoured_required: bool_at(m, "seed_honoured_required", REC)?,
        })
    }
}

impl PreRegistration {
    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("registered_at".into(), Json::Int(self.registered_at as i64));
        m.insert("hypothesis".into(), Json::str(&self.hypothesis));
        m.insert(
            "primary_metrics".into(),
            Json::Arr(self.primary_metrics.iter().map(Json::str).collect()),
        );
        if let Some(em) = &self.equivalence_margin {
            m.insert("equivalence_margin".into(), em.clone());
        }
        m.insert("min_n".into(), Json::Int(self.min_n as i64));
        m.insert(
            "analysis_plan_ref".into(),
            Json::str(&self.analysis_plan_ref),
        );
        m.insert("task_split_hash".into(), Json::str(&self.task_split_hash));
        if !self.interactions.is_empty() {
            m.insert(
                "interactions".into(),
                Json::Arr(self.interactions.iter().map(Json::str).collect()),
            );
        }
        Json::Obj(m)
    }

    /// Strict decode — [`EvalError::SchemaViolation`] on missing/unknown members.
    pub fn from_json(j: &Json) -> Result<PreRegistration, EvalError> {
        const REC: &str = "PreRegistration";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "registered_at",
                "hypothesis",
                "primary_metrics",
                "equivalence_margin",
                "min_n",
                "analysis_plan_ref",
                "task_split_hash",
                "interactions",
            ],
            REC,
        )?;
        Ok(PreRegistration {
            registered_at: int_at(m, "registered_at", REC)? as u64,
            hypothesis: str_at(m, "hypothesis", REC)?.to_string(),
            primary_metrics: str_vec_at(m, "primary_metrics", REC)?,
            equivalence_margin: m.get("equivalence_margin").cloned(),
            min_n: int_at(m, "min_n", REC)? as u32,
            analysis_plan_ref: str_at(m, "analysis_plan_ref", REC)?.to_string(),
            task_split_hash: str_at(m, "task_split_hash", REC)?.to_string(),
            interactions: match m.get("interactions") {
                None | Some(Json::Null) => Vec::new(),
                Some(Json::Arr(items)) => items
                    .iter()
                    .map(|j| {
                        j.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| EvalError::SchemaViolation {
                                member: "interactions".into(),
                                detail: "entries must be strings".into(),
                            })
                    })
                    .collect::<Result<Vec<String>, EvalError>>()?,
                Some(_) => {
                    return Err(EvalError::SchemaViolation {
                        member: "interactions".into(),
                        detail: "must be an array of effect spellings".into(),
                    })
                }
            },
        })
    }
}

impl Design {
    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("id".into(), Json::str(&self.id));
        m.insert("kind".into(), Json::str(self.kind.as_str()));
        m.insert(
            "factors".into(),
            Json::Arr(
                self.factors
                    .iter()
                    .map(FactorDeclaration::to_json)
                    .collect(),
            ),
        );
        m.insert(
            "blocking".into(),
            Json::Arr(self.blocking.iter().map(Json::str).collect()),
        );
        m.insert(
            "replicates_per_cell".into(),
            Json::Int(self.replicates_per_cell as i64),
        );
        m.insert("pairing".into(), Json::str(self.pairing.as_str()));
        m.insert("seed_policy".into(), self.seed_policy.to_json());
        if let Some(h) = &self.held_out_split_ref {
            m.insert("held_out_split_ref".into(), Json::str(h));
        }
        m.insert("pre_registration".into(), self.pre_registration.to_json());
        if let Some(s) = &self.registry_snapshot_id {
            m.insert("registry_snapshot_id".into(), Json::str(s));
        }
        if let Some(g) = &self.generators {
            m.insert(
                "generators".into(),
                Json::Arr(g.iter().map(Json::str).collect()),
            );
        }
        if let Some(r) = &self.resolution {
            m.insert("resolution".into(), Json::str(r.as_str()));
        }
        m.insert("routing_policy".into(), self.routing_policy.to_json());
        if let Some(d) = &self.deviation_policy {
            m.insert("deviation_policy".into(), Json::str(d.as_str()));
        }
        if self.cache_na_stratified {
            m.insert("cache_na_stratified".into(), Json::Bool(true));
        }
        Json::Obj(m)
    }

    /// Strict decode — [`EvalError::SchemaViolation`] on missing/unknown members.
    pub fn from_json(j: &Json) -> Result<Design, EvalError> {
        const REC: &str = "Design";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "id",
                "kind",
                "factors",
                "blocking",
                "replicates_per_cell",
                "pairing",
                "seed_policy",
                "held_out_split_ref",
                "pre_registration",
                "registry_snapshot_id",
                "generators",
                "resolution",
                "routing_policy",
                "deviation_policy",
                "cache_na_stratified",
            ],
            REC,
        )?;
        let mut factors = Vec::new();
        for f in arr_at(m, "factors", REC)? {
            factors.push(FactorDeclaration::from_json(f)?);
        }
        Ok(Design {
            id: str_at(m, "id", REC)?.to_string(),
            kind: DesignKind::parse(str_at(m, "kind", REC)?).ok_or_else(|| {
                EvalError::SchemaViolation {
                    member: "kind".into(),
                    detail: "unknown design kind".into(),
                }
            })?,
            factors,
            blocking: str_vec_at(m, "blocking", REC)?,
            replicates_per_cell: int_at(m, "replicates_per_cell", REC)? as u32,
            pairing: Pairing::parse(str_at(m, "pairing", REC)?).ok_or_else(|| {
                EvalError::SchemaViolation {
                    member: "pairing".into(),
                    detail: "unknown pairing".into(),
                }
            })?,
            seed_policy: SeedPolicy::from_json(m.get("seed_policy").ok_or_else(|| {
                EvalError::SchemaViolation {
                    member: "seed_policy".into(),
                    detail: "missing member".into(),
                }
            })?)?,
            held_out_split_ref: opt_str_at(m, "held_out_split_ref")?.map(str::to_string),
            pre_registration: PreRegistration::from_json(m.get("pre_registration").ok_or_else(
                || EvalError::SchemaViolation {
                    member: "pre_registration".into(),
                    detail: "missing member".into(),
                },
            )?)?,
            registry_snapshot_id: opt_str_at(m, "registry_snapshot_id")?.map(str::to_string),
            generators: match m.get("generators") {
                None | Some(Json::Null) => None,
                Some(Json::Arr(items)) => Some(
                    items
                        .iter()
                        .map(|j| {
                            j.as_str().map(str::to_string).ok_or_else(|| {
                                EvalError::SchemaViolation {
                                    member: "generators".into(),
                                    detail: "entries must be strings".into(),
                                }
                            })
                        })
                        .collect::<Result<Vec<String>, EvalError>>()?,
                ),
                Some(_) => {
                    return Err(EvalError::SchemaViolation {
                        member: "generators".into(),
                        detail: "must be an array of generator spellings".into(),
                    })
                }
            },
            resolution: match opt_str_at(m, "resolution")? {
                None => None,
                Some(s) => Some(FractionalResolution::parse(s).ok_or_else(|| {
                    EvalError::SchemaViolation {
                        member: "resolution".into(),
                        detail: format!("unknown resolution `{s}`"),
                    }
                })?),
            },
            routing_policy: RoutingPolicy::from_json(m.get("routing_policy"))?,
            deviation_policy: match opt_str_at(m, "deviation_policy")? {
                None => None,
                Some(s) => {
                    Some(
                        DeviationPolicy::parse(s).ok_or_else(|| EvalError::SchemaViolation {
                            member: "deviation_policy".into(),
                            detail: format!("unknown deviation policy `{s}`"),
                        })?,
                    )
                }
            },
            cache_na_stratified: match m.get("cache_na_stratified") {
                None | Some(Json::Null) => false,
                Some(Json::Bool(b)) => *b,
                Some(_) => {
                    return Err(EvalError::SchemaViolation {
                        member: "cache_na_stratified".into(),
                        detail: "must be a bool".into(),
                    })
                }
            },
        })
    }
}

impl SearchBudgetRecord {
    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("rollouts".into(), Json::Int(self.rollouts as i64));
        m.insert("model_calls".into(), Json::Int(self.model_calls as i64));
        m.insert("tokens".into(), Json::Int(self.tokens as i64));
        m.insert(
            "feedback_labels_used".into(),
            Json::Int(self.feedback_labels_used as i64),
        );
        m.insert("judge_calls".into(), Json::Int(self.judge_calls as i64));
        m.insert("wall_clock".into(), Json::Int(self.wall_clock as i64));
        m.insert(
            "allocation".into(),
            Json::Obj(
                self.allocation
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                    .collect(),
            ),
        );
        m.insert(
            "adaptive_selection".into(),
            Json::Bool(self.adaptive_selection),
        );
        Json::Obj(m)
    }

    /// Strict decode — [`EvalError::SchemaViolation`] on missing/unknown members.
    pub fn from_json(j: &Json) -> Result<SearchBudgetRecord, EvalError> {
        const REC: &str = "SearchBudgetRecord";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "rollouts",
                "model_calls",
                "tokens",
                "feedback_labels_used",
                "judge_calls",
                "wall_clock",
                "allocation",
                "adaptive_selection",
            ],
            REC,
        )?;
        let mut allocation = BTreeMap::new();
        match m.get("allocation") {
            Some(Json::Obj(am)) => {
                for (k, v) in am {
                    allocation.insert(
                        k.clone(),
                        v.as_int().ok_or_else(|| EvalError::SchemaViolation {
                            member: "allocation".into(),
                            detail: "shares must be ints".into(),
                        })? as u64,
                    );
                }
            }
            _ => {
                return Err(EvalError::SchemaViolation {
                    member: "allocation".into(),
                    detail: "must be an object".into(),
                })
            }
        }
        Ok(SearchBudgetRecord {
            rollouts: int_at(m, "rollouts", REC)? as u64,
            model_calls: int_at(m, "model_calls", REC)? as u64,
            tokens: int_at(m, "tokens", REC)? as u64,
            feedback_labels_used: int_at(m, "feedback_labels_used", REC)? as u64,
            judge_calls: int_at(m, "judge_calls", REC)? as u64,
            wall_clock: int_at(m, "wall_clock", REC)? as u64,
            allocation,
            adaptive_selection: bool_at(m, "adaptive_selection", REC)?,
        })
    }
}

impl Cell {
    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("arm_id", Json::str(&self.arm_id)),
            ("configuration", self.configuration.to_json()),
            ("task_ref", Json::str(&self.task_ref)),
        ])
    }

    /// Strict decode — [`EvalError::SchemaViolation`] on missing/unknown members.
    pub fn from_json(j: &Json) -> Result<Cell, EvalError> {
        const REC: &str = "Cell";
        let m = expect_obj(j, REC)?;
        reject_unknown(m, &["arm_id", "configuration", "task_ref"], REC)?;
        Ok(Cell {
            arm_id: str_at(m, "arm_id", REC)?.to_string(),
            configuration: Configuration::from_json(m.get("configuration").ok_or_else(|| {
                EvalError::SchemaViolation {
                    member: "configuration".into(),
                    detail: "missing member".into(),
                }
            })?)
            .ok_or_else(|| EvalError::SchemaViolation {
                member: "configuration".into(),
                detail: "malformed configuration".into(),
            })?,
            task_ref: str_at(m, "task_ref", REC)?.to_string(),
        })
    }
}

impl Arm {
    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("id".into(), Json::str(&self.id));
        m.insert("hypothesis".into(), Json::str(&self.hypothesis));
        m.insert("design_ref".into(), Json::str(&self.design_ref));
        m.insert(
            "configurations".into(),
            Json::Arr(
                self.configurations
                    .iter()
                    .map(Configuration::to_json)
                    .collect(),
            ),
        );
        m.insert("eval_budget".into(), self.eval_budget.to_json());
        match &self.search_budget {
            Some(sb) => {
                m.insert("search_budget".into(), sb.to_json());
            }
            None => {
                // `unknown` is an explicit marker, never an absent member (CC3 —
                // a product-level arm's `search_unknown` flag reads this).
                m.insert("search_budget".into(), Json::str("unknown"));
            }
        }
        m.insert("artifact_ref".into(), self.artifact_ref.to_json());
        m.insert("provenance".into(), self.provenance.to_json());
        Json::Obj(m)
    }

    /// Strict decode — [`EvalError::SchemaViolation`] on missing/unknown members.
    pub fn from_json(j: &Json) -> Result<Arm, EvalError> {
        const REC: &str = "Arm";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "id",
                "hypothesis",
                "design_ref",
                "configurations",
                "eval_budget",
                "search_budget",
                "artifact_ref",
                "provenance",
            ],
            REC,
        )?;
        let mut configurations = Vec::new();
        for c in arr_at(m, "configurations", REC)? {
            configurations.push(Configuration::from_json(c).ok_or_else(|| {
                EvalError::SchemaViolation {
                    member: "configurations".into(),
                    detail: "malformed configuration".into(),
                }
            })?);
        }
        let search_budget = match m.get("search_budget") {
            Some(Json::Str(s)) if s == "unknown" => None,
            Some(j) => Some(SearchBudgetRecord::from_json(j)?),
            None => {
                return Err(EvalError::SchemaViolation {
                    member: "search_budget".into(),
                    detail: "missing member".into(),
                })
            }
        };
        Ok(Arm {
            id: str_at(m, "id", REC)?.to_string(),
            hypothesis: str_at(m, "hypothesis", REC)?.to_string(),
            design_ref: str_at(m, "design_ref", REC)?.to_string(),
            configurations,
            eval_budget: Ref::from_json(m.get("eval_budget").ok_or_else(|| {
                EvalError::SchemaViolation {
                    member: "eval_budget".into(),
                    detail: "missing member".into(),
                }
            })?)
            .ok_or_else(|| EvalError::SchemaViolation {
                member: "eval_budget".into(),
                detail: "malformed ref".into(),
            })?,
            search_budget,
            artifact_ref: Ref::from_json(m.get("artifact_ref").ok_or_else(|| {
                EvalError::SchemaViolation {
                    member: "artifact_ref".into(),
                    detail: "missing member".into(),
                }
            })?)
            .ok_or_else(|| EvalError::SchemaViolation {
                member: "artifact_ref".into(),
                detail: "malformed ref".into(),
            })?,
            provenance: ProvenanceRecord::from_json(m.get("provenance").ok_or_else(|| {
                EvalError::SchemaViolation {
                    member: "provenance".into(),
                    detail: "missing member".into(),
                }
            })?)
            .map_err(|e| EvalError::SchemaViolation {
                member: "provenance".into(),
                detail: format!("{e:?}"),
            })?,
        })
    }
}

impl RunOutcome {
    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("run_id", Json::str(&self.run_id)),
            ("outcome_class", Json::str(self.outcome_class.as_str())),
            ("stop_reason", self.stop_reason.to_json()),
            (
                "budget_consumed",
                Json::Obj(
                    self.budget_consumed
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::Int(*v)))
                        .collect(),
                ),
            ),
            (
                "veto_tripped",
                Json::Arr(self.veto_tripped.iter().map(Json::str).collect()),
            ),
        ])
    }

    /// Strict decode — [`EvalError::SchemaViolation`] on missing/unknown members.
    pub fn from_json(j: &Json) -> Result<RunOutcome, EvalError> {
        const REC: &str = "RunOutcome";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "run_id",
                "outcome_class",
                "stop_reason",
                "budget_consumed",
                "veto_tripped",
            ],
            REC,
        )?;
        let mut budget_consumed = BTreeMap::new();
        match m.get("budget_consumed") {
            Some(Json::Obj(bm)) => {
                for (k, v) in bm {
                    budget_consumed.insert(
                        k.clone(),
                        v.as_int().ok_or_else(|| EvalError::SchemaViolation {
                            member: "budget_consumed".into(),
                            detail: "values must be ints".into(),
                        })?,
                    );
                }
            }
            _ => {
                return Err(EvalError::SchemaViolation {
                    member: "budget_consumed".into(),
                    detail: "must be an object".into(),
                })
            }
        }
        Ok(RunOutcome {
            run_id: str_at(m, "run_id", REC)?.to_string(),
            outcome_class: OutcomeClass::parse(str_at(m, "outcome_class", REC)?).ok_or_else(
                || EvalError::SchemaViolation {
                    member: "outcome_class".into(),
                    detail: "unknown outcome_class".into(),
                },
            )?,
            stop_reason: StopReason::from_json(m.get("stop_reason").ok_or_else(|| {
                EvalError::SchemaViolation {
                    member: "stop_reason".into(),
                    detail: "missing member".into(),
                }
            })?)
            .ok_or_else(|| EvalError::SchemaViolation {
                member: "stop_reason".into(),
                detail: "malformed stop_reason".into(),
            })?,
            budget_consumed,
            veto_tripped: str_vec_at(m, "veto_tripped", REC)?,
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `OracleDeclaration` — the `validator`-kind registry object (ADR-0047 D1).
// ─────────────────────────────────────────────────────────────────────────────

/// `OracleDeclaration{oracle_id, class, deterministic, requires_observability,
/// verdict_type, evidence_out, charged_to, calibration_ref?, provenance}` —
/// the C0/Stage-1 registry object every `MetricValue`'s `oracle_ref` resolves
/// to (ADR-0047 D1). A `judge` oracle is never deterministic and charges to
/// the instrument; `reference_relative` verdicts are three-valued.
#[derive(Debug, Clone, PartialEq)]
pub struct OracleDeclaration {
    /// The oracle id (the registry name).
    pub oracle_id: String,
    /// The oracle class (ADR-0047's nine-class taxonomy).
    pub class: OracleClass,
    /// Whether the oracle is deterministic — `judge ⇒ false` (ADR-0110).
    pub deterministic: bool,
    /// The observability the oracle requires (`requires_observability` scopes
    /// availability — T-LCD-15).
    pub requires_observability: BTreeSet<Observability>,
    /// The verdict type the oracle emits.
    pub verdict_type: VerdictType,
    /// The evidence kinds the verdict emits (`evidence_out` — ADR-0109 D2's
    /// member, shared with `ValidatorDeclaration`).
    pub evidence_out: Vec<EvidenceKind>,
    /// Who the oracle's spend is charged to — `judge ⇒ instrument` (ADR-0047 D3).
    pub charged_to: ChargedTo,
    /// The calibration ref (a judge records agreement with a deterministic
    /// oracle on a labelled subset; `none` renders `n/a{no_detector}` for
    /// gating and is reported `exploratory` — ADR-0047(c)(ii)).
    pub calibration_ref: Option<String>,
    /// The declaration's provenance.
    pub provenance: ProvenanceRecord,
}

/// `OracleDeclaration` validation failures (typed — never a warning).
#[derive(Debug, Clone, PartialEq)]
pub enum OracleError {
    /// `judge ⇒ deterministic = false` (ADR-0110).
    JudgeDeterministic,
    /// `judge ⇒ charged_to = instrument` (ADR-0047 D3).
    JudgeNotInstrument,
    /// `reference_relative ⇒ verdict_type = three_valued` (ADR-0047 D7 — the
    /// derived three-valued `equivalence_run` class).
    ReferenceVerdictType,
    /// An empty `oracle_id`, or `judge`/`teacher_relative` without the
    /// `requires_observability` their evidence needs.
    IncompleteDeclaration {
        /// Why the declaration is incomplete.
        reason: String,
    },
}

impl OracleDeclaration {
    /// The declaration's schema checks (the Stage-1 enforceable half of
    /// ADR-0047's per-class obligations; judge independence/calibration are
    /// runtime checks — Stage 3+/C2).
    pub fn validate(&self) -> Result<(), OracleError> {
        if self.oracle_id.is_empty() {
            return Err(OracleError::IncompleteDeclaration {
                reason: "empty oracle_id".into(),
            });
        }
        if self.class == OracleClass::Judge && self.deterministic {
            return Err(OracleError::JudgeDeterministic);
        }
        if self.class == OracleClass::Judge && self.charged_to != ChargedTo::Instrument {
            return Err(OracleError::JudgeNotInstrument);
        }
        if self.class == OracleClass::ReferenceRelative
            && self.verdict_type != VerdictType::ThreeValued
        {
            return Err(OracleError::ReferenceVerdictType);
        }
        Ok(())
    }

    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("oracle_id".into(), Json::str(&self.oracle_id));
        m.insert("class".into(), Json::str(self.class.as_str()));
        m.insert("deterministic".into(), Json::Bool(self.deterministic));
        m.insert(
            "requires_observability".into(),
            Json::Arr(
                self.requires_observability
                    .iter()
                    .map(|o| Json::str(o.as_str()))
                    .collect(),
            ),
        );
        m.insert("verdict_type".into(), Json::str(self.verdict_type.as_str()));
        m.insert(
            "evidence_out".into(),
            Json::Arr(
                self.evidence_out
                    .iter()
                    .map(|k| Json::str(k.as_str()))
                    .collect(),
            ),
        );
        m.insert("charged_to".into(), Json::str(self.charged_to.as_str()));
        if let Some(c) = &self.calibration_ref {
            m.insert("calibration_ref".into(), Json::str(c));
        }
        m.insert("provenance".into(), self.provenance.to_json());
        Json::Obj(m)
    }

    /// Strict decode — [`EvalError::SchemaViolation`] on missing/unknown
    /// members or spellings.
    pub fn from_json(j: &Json) -> Result<OracleDeclaration, EvalError> {
        const REC: &str = "OracleDeclaration";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "oracle_id",
                "class",
                "deterministic",
                "requires_observability",
                "verdict_type",
                "evidence_out",
                "charged_to",
                "calibration_ref",
                "provenance",
            ],
            REC,
        )?;
        Ok(OracleDeclaration {
            oracle_id: str_at(m, "oracle_id", REC)?.to_string(),
            class: OracleClass::parse(str_at(m, "class", REC)?).ok_or_else(|| {
                EvalError::SchemaViolation {
                    member: "class".to_string(),
                    detail: "unknown oracle class".to_string(),
                }
            })?,
            deterministic: bool_at(m, "deterministic", REC)?,
            requires_observability: enum_set_at(
                m,
                "requires_observability",
                Observability::parse,
                REC,
            )?,
            verdict_type: VerdictType::parse(str_at(m, "verdict_type", REC)?).ok_or_else(|| {
                EvalError::SchemaViolation {
                    member: "verdict_type".to_string(),
                    detail: "unknown verdict_type".to_string(),
                }
            })?,
            evidence_out: enum_vec_at(m, "evidence_out", EvidenceKind::parse, REC)?,
            charged_to: ChargedTo::parse(str_at(m, "charged_to", REC)?).ok_or_else(|| {
                EvalError::SchemaViolation {
                    member: "charged_to".to_string(),
                    detail: "unknown charged_to".to_string(),
                }
            })?,
            calibration_ref: opt_str_at(m, "calibration_ref")?.map(str::to_string),
            provenance: ProvenanceRecord::from_json(m.get("provenance").ok_or_else(|| {
                EvalError::SchemaViolation {
                    member: "provenance".to_string(),
                    detail: "missing member".to_string(),
                }
            })?)
            .map_err(|e| EvalError::SchemaViolation {
                member: "provenance".to_string(),
                detail: format!("{e:?}"),
            })?,
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Errors + codec helpers.
// ─────────────────────────────────────────────────────────────────────────────

/// `EvalError` — the eval-framework decode/validation failures (typed — never
/// a warning).
#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    /// A canonical-form violation — a missing member, an unknown member or a
    /// spelling outside a closed sum.
    SchemaViolation {
        /// The member path.
        member: String,
        /// What was wrong.
        detail: String,
    },
}

/// `expect_obj` — the strict object check.
pub(crate) fn expect_obj<'a>(
    j: &'a Json,
    rec: &str,
) -> Result<&'a BTreeMap<String, Json>, EvalError> {
    match j {
        Json::Obj(m) => Ok(m),
        _ => Err(EvalError::SchemaViolation {
            member: rec.to_string(),
            detail: format!("{rec} must be an object"),
        }),
    }
}

/// `reject_unknown` — the strict member check (R2/CC3 — unknown members refuse).
pub(crate) fn reject_unknown(
    m: &BTreeMap<String, Json>,
    allowed: &[&str],
    rec: &str,
) -> Result<(), EvalError> {
    for k in m.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(EvalError::SchemaViolation {
                member: k.clone(),
                detail: format!("unknown member of {rec}"),
            });
        }
    }
    Ok(())
}

/// `str_at` — a required string member.
pub(crate) fn str_at<'a>(
    m: &'a BTreeMap<String, Json>,
    member: &str,
    rec: &str,
) -> Result<&'a str, EvalError> {
    m.get(member)
        .and_then(Json::as_str)
        .ok_or_else(|| EvalError::SchemaViolation {
            member: member.to_string(),
            detail: format!("{rec}.{member} must be a string"),
        })
}

/// `opt_str_at` — an optional string member.
pub(crate) fn opt_str_at<'a>(
    m: &'a BTreeMap<String, Json>,
    member: &str,
) -> Result<Option<&'a str>, EvalError> {
    match m.get(member) {
        None | Some(Json::Null) => Ok(None),
        Some(j) => j
            .as_str()
            .map(Some)
            .ok_or_else(|| EvalError::SchemaViolation {
                member: member.to_string(),
                detail: "must be a string".to_string(),
            }),
    }
}

/// `int_at` — a required integer member.
pub(crate) fn int_at(
    m: &BTreeMap<String, Json>,
    member: &str,
    rec: &str,
) -> Result<i64, EvalError> {
    m.get(member)
        .and_then(Json::as_int)
        .ok_or_else(|| EvalError::SchemaViolation {
            member: member.to_string(),
            detail: format!("{rec}.{member} must be an int"),
        })
}

/// `opt_int_at` — an optional integer member.
pub(crate) fn opt_int_at(
    m: &BTreeMap<String, Json>,
    member: &str,
) -> Result<Option<i64>, EvalError> {
    match m.get(member) {
        None | Some(Json::Null) => Ok(None),
        Some(j) => j
            .as_int()
            .map(Some)
            .ok_or_else(|| EvalError::SchemaViolation {
                member: member.to_string(),
                detail: "must be an int".to_string(),
            }),
    }
}

/// `bool_at` — a required bool member.
pub(crate) fn bool_at(
    m: &BTreeMap<String, Json>,
    member: &str,
    rec: &str,
) -> Result<bool, EvalError> {
    match m.get(member) {
        Some(Json::Bool(b)) => Ok(*b),
        _ => Err(EvalError::SchemaViolation {
            member: member.to_string(),
            detail: format!("{rec}.{member} must be a bool"),
        }),
    }
}

/// `arr_at` — a required array member.
pub(crate) fn arr_at<'a>(
    m: &'a BTreeMap<String, Json>,
    member: &str,
    rec: &str,
) -> Result<&'a Vec<Json>, EvalError> {
    match m.get(member) {
        Some(Json::Arr(a)) => Ok(a),
        _ => Err(EvalError::SchemaViolation {
            member: member.to_string(),
            detail: format!("{rec}.{member} must be an array"),
        }),
    }
}

/// `enum_set_at` — decode a `BTreeSet` of closed-sum members from a string
/// array; unknown spellings refuse.
pub(crate) fn enum_set_at<T: Ord>(
    m: &BTreeMap<String, Json>,
    member: &str,
    parse: impl Fn(&str) -> Option<T>,
    rec: &str,
) -> Result<BTreeSet<T>, EvalError> {
    let mut out = BTreeSet::new();
    for v in arr_at(m, member, rec)? {
        let s = v.as_str().ok_or_else(|| EvalError::SchemaViolation {
            member: member.to_string(),
            detail: "member must be a string".to_string(),
        })?;
        out.insert(parse(s).ok_or_else(|| EvalError::SchemaViolation {
            member: member.to_string(),
            detail: format!("unknown spelling {s}"),
        })?);
    }
    Ok(out)
}

/// `enum_vec_at` — decode a `Vec` of closed-sum members.
pub(crate) fn enum_vec_at<T>(
    m: &BTreeMap<String, Json>,
    member: &str,
    parse: impl Fn(&str) -> Option<T>,
    rec: &str,
) -> Result<Vec<T>, EvalError> {
    let mut out = Vec::new();
    for v in arr_at(m, member, rec)? {
        let s = v.as_str().ok_or_else(|| EvalError::SchemaViolation {
            member: member.to_string(),
            detail: "member must be a string".to_string(),
        })?;
        out.push(parse(s).ok_or_else(|| EvalError::SchemaViolation {
            member: member.to_string(),
            detail: format!("unknown spelling {s}"),
        })?);
    }
    Ok(out)
}

/// `str_vec_at` — decode a `Vec<String>` member.
pub(crate) fn str_vec_at(
    m: &BTreeMap<String, Json>,
    member: &str,
    rec: &str,
) -> Result<Vec<String>, EvalError> {
    let mut out = Vec::new();
    for v in arr_at(m, member, rec)? {
        out.push(
            v.as_str()
                .ok_or_else(|| EvalError::SchemaViolation {
                    member: member.to_string(),
                    detail: "member must be a string".to_string(),
                })?
                .to_string(),
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::StopReason;
    use hh_provenance::authority::AuthorityClass;

    fn prov() -> ProvenanceRecord {
        ProvenanceRecord::kernel("eval:test", 1)
    }

    fn cfg_with_budget(budget_sem: &str, seed: &str) -> Configuration {
        Configuration {
            m_set: Ref::new("model/m1", "model/m1@v1"),
            h: Ref::new("hir/h1", "hir/h1@v1"),
            p: Ref::new("profile/p1", "profile/p1@v1"),
            e: Ref::new("env/e1", "env/e1@sha256:aa"),
            b: Ref::new(budget_sem, format!("{budget_sem}@v1")),
            seed: seed.to_string(),
        }
    }

    fn arm(budget_sem: &str, sb: Option<SearchBudgetRecord>) -> Arm {
        Arm {
            id: "arm:a".into(),
            hypothesis: "V helps M1".into(),
            design_ref: "design:d1".into(),
            configurations: vec![
                cfg_with_budget(budget_sem, "s0"),
                cfg_with_budget(budget_sem, "s1"),
            ],
            eval_budget: Ref::new(budget_sem, format!("{budget_sem}@v1")),
            search_budget: sb,
            artifact_ref: Ref::new("def/d1", "sha256:abcd"),
            provenance: prov(),
        }
    }

    fn complete_sb() -> SearchBudgetRecord {
        let mut allocation = BTreeMap::new();
        allocation.insert("component:search".to_string(), 700_000);
        allocation.insert("slot:scorer".to_string(), 300_000);
        SearchBudgetRecord {
            rollouts: 12,
            model_calls: 40,
            tokens: 900_000,
            feedback_labels_used: 8,
            judge_calls: 0,
            wall_clock: 3_600_000,
            allocation,
            adaptive_selection: false,
        }
    }

    #[test]
    fn outcome_class_derivation_includes_oracle_failure() {
        // ADR-0047 D4: an oracle failure is an outcome class, never a score.
        let completed = StopReason::Completed;
        assert_eq!(
            derive_outcome_class(&completed, false),
            OutcomeClass::Scored
        );
        assert_eq!(
            derive_outcome_class(&completed, true),
            OutcomeClass::OracleFailure
        );
        let refused = StopReason::Refused {
            blocking_effect_id: "e1".into(),
        };
        assert_eq!(derive_outcome_class(&refused, false), OutcomeClass::Refused);
    }

    #[test]
    fn arm_validate_enforces_the_declare_arm_preconditions() {
        // BudgetMismatchWithinArm: a configuration whose budget differs from eval_budget.
        let mut bad = arm("budget/std", Some(complete_sb()));
        bad.configurations[1].b = Ref::new("budget/other", "budget/other@v1");
        assert!(matches!(
            bad.validate(Granularity::ConfigurationLevel),
            Err(ArmError::BudgetMismatchWithinArm { .. })
        ));
        // BudgetMissing: search_budget absent outside product level.
        let no_sb = arm("budget/std", None);
        assert_eq!(
            no_sb.validate(Granularity::ConfigurationLevel),
            Err(ArmError::BudgetMissing)
        );
        // …legal at product level.
        assert!(no_sb.validate(Granularity::ProductLevel).is_ok());
        // ArtifactUnsealed: a non-pinned artifact ref.
        let mut unsealed = arm("budget/std", Some(complete_sb()));
        unsealed.artifact_ref = Ref::new("def/d1", "latest");
        assert_eq!(
            unsealed.validate(Granularity::ConfigurationLevel),
            Err(ArmError::ArtifactUnsealed)
        );
        // The complete arm passes.
        assert!(arm("budget/std", Some(complete_sb()))
            .validate(Granularity::ConfigurationLevel)
            .is_ok());
    }

    #[test]
    fn search_budget_completeness_requires_full_allocation() {
        let mut sb = complete_sb();
        assert!(sb.is_complete());
        sb.allocation.insert("component:x".into(), 1);
        assert!(!sb.is_complete());
        let mut empty_alloc = complete_sb();
        empty_alloc.allocation.clear();
        assert!(!empty_alloc.is_complete());
    }

    #[test]
    fn metric_value_typed_na_is_never_zero() {
        // T-LCD-15: an n/a value round-trips as a typed value, never coerced.
        let v = MetricValue {
            metric_ref: "metric/task_success".into(),
            value: MetricValueKind::Na(NaReason::Observability),
            applies_to: "run:r1".into(),
            oracle_ref: "oracle:end_state".into(),
            detector: Detector::Deterministic,
            confidence: None,
            evidence_ref: None,
        };
        let j = v.to_json();
        let back = MetricValue::from_json(&j).unwrap();
        assert_eq!(back, v);
        assert!(matches!(
            back.value,
            MetricValueKind::Na(NaReason::Observability)
        ));
    }

    #[test]
    fn oracle_declaration_validation() {
        let mut o = OracleDeclaration {
            oracle_id: "oracle:end_state".into(),
            class: OracleClass::EndState,
            deterministic: true,
            requires_observability: [Observability::EndState].into_iter().collect(),
            verdict_type: VerdictType::Bool,
            evidence_out: vec![EvidenceKind::EndState],
            charged_to: ChargedTo::Subject,
            calibration_ref: None,
            provenance: prov(),
        };
        assert!(o.validate().is_ok());
        // judge ⇒ ¬deterministic ∧ charged_to = instrument.
        o.class = OracleClass::Judge;
        assert_eq!(o.validate(), Err(OracleError::JudgeDeterministic));
        o.deterministic = false;
        assert_eq!(o.validate(), Err(OracleError::JudgeNotInstrument));
        o.charged_to = ChargedTo::Instrument;
        assert!(o.validate().is_ok());
        // reference_relative ⇒ three_valued.
        o.class = OracleClass::ReferenceRelative;
        assert_eq!(o.validate(), Err(OracleError::ReferenceVerdictType));
        o.verdict_type = VerdictType::ThreeValued;
        assert!(o.validate().is_ok());
        // Canonical codec round-trips.
        assert_eq!(OracleDeclaration::from_json(&o.to_json()).unwrap(), o);
    }

    #[test]
    fn outcome_class_policy_defaults() {
        let cap = OutcomeClassPolicy::for_capability();
        assert!(cap.in_denominator(OutcomeClass::Scored));
        assert!(cap.in_denominator(OutcomeClass::BudgetExhausted));
        assert!(!cap.in_denominator(OutcomeClass::Refused));
        assert!(!cap.refusal_is_failure());
        let mut refusal = OutcomeClassPolicy::for_capability();
        refusal
            .policy
            .insert(OutcomeClass::Refused, OutcomePolicy::CountAsFailure);
        assert!(refusal.refusal_is_failure());
        let eff = OutcomeClassPolicy::for_efficiency();
        assert!(!eff.in_denominator(OutcomeClass::BudgetExhausted));
    }

    #[test]
    fn moved_vocabulary_keeps_its_spellings() {
        assert_eq!(
            OracleClass::parse("trace_predicate"),
            Some(OracleClass::TracePredicate)
        );
        assert!(OracleClass::TracePredicate.is_c0_headline());
        assert!(!OracleClass::Judge.is_c0_headline());
        assert_eq!(OracleClass::Judge.detector(), Detector::Judged);
        assert_eq!(ChargedTo::parse("instrument"), Some(ChargedTo::Instrument));
        assert_eq!(LatticeValue::parse("N"), Some(LatticeValue::N));
        assert_eq!(
            VerdictType::parse("three_valued"),
            Some(VerdictType::ThreeValued)
        );
        assert_eq!(EvidenceKind::parse("model_io"), Some(EvidenceKind::ModelIo));
        // Provenance helper sanity — the minted record carries the kernel authority.
        assert_eq!(prov().authority, AuthorityClass::Kernel);
    }
}
