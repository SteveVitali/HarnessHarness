//! Parity admission — authentic paired records and policy
//! applicability (spec §4.4/§4.5; REQ-D-004/301/302/303/308;
//! AC-4-3/AC-4-4/AC-5-1; the GATE-G9-admitted §9.3 rows: Packet E
//! (PADR-R5-5*) H/O first-attempt `(task, repeat, pair_id)` grouping
//! and Packet F (PADR-R5-6*) margin/equivalence authority; methods
//! partition M-F).
//!
//! `parity_admission` validates **supplied** evidence — it never
//! manufactures authentic runs. Admission requires the authentic
//! first-attempt H/O pair manifest (actual run/verdict/replay
//! identities, equal `T`/`R`/weights, adapter ≥3 and S3 ≥5
//! attempts/task floors, distinct subjects, the declared phase/
//! stratum/policy) **and** the signed pre-data
//! dimension→metric/estimand/population/range/unit/margin/
//! applicability/evidence map **and** applicability covering both
//! actual policies (per-policy evidence) or a predeclared
//! identical-bridge reuse proof. The admitted pair ids are the
//! records R5.38's `directional_kl_equivalence` consumes unchanged.
//!
//! Refusals are typed, never warnings: recorded model-I/O corpus
//! fixtures, an `artifact_benefit` wrapper alone, equal empty model
//! snapshot maps, oracle transport equality and seed presence cannot
//! establish authentic admission (`InauthenticEvidence`). Baseline-only
//! parity cannot admit the changed policy (`PolicyScope`); baseline
//! bias alone leaves treatment bias unknown — the `b_A = −.10` /
//! `b_B = .10` counterexample — so baseline-only or mixed-uncertainty
//! transfer claims fail (`BaselineOnlyTransfer`/`MixedUncertainty`);
//! no transfer certificate is admitted here. Missing required replay
//! leaves the pair unmatched (`MissingReplay`), never a pass; a
//! dropped required endpoint blocks the conjunction
//! (`MissingDimension`); a typed `inapplicable` entry records its
//! reason and never counts as passed (`RequiredInapplicable` when the
//! dimension is required). The narrow grouping exception grants no
//! honoured-seed/CRN/R2/component-headline claim (`UnsupportedClaim`).

use std::collections::{BTreeMap, BTreeSet};

use hharness_wire::Json;

use crate::benefits::RegisteredMargin;
use crate::population::MethodMetadata;

/// The analysis kind spelling.
pub const KIND: &str = "parity_admission";
/// The durable payload version.
pub const VERSION: &str = "parity-admission/1";
/// The adapter floor — ≥3 attempts per task per adapter (spec §4.4).
pub const ADAPTER_FLOOR: u32 = 3;
/// The S3 floor — ≥5 attempts per task per arm (spec §4.4).
pub const S3_FLOOR: u32 = 5;
/// The only admitted evidence shape.
pub const REGISTERED_SOURCE: &str = "authentic_first_attempt";
/// The only admitted dependence model (Packet E: across-pair
/// independence; honoured-seed/CRN/carryover claims refuse).
pub const REGISTERED_DEPENDENCE: &str = "independent_pairs";
/// The only admitted weighting (`w = 1/N`).
pub const REGISTERED_WEIGHTS: &str = "equal";
/// The admitted report verdict.
pub const ADMITTED: &str = "admitted";

/// The seven contract dimensions (spec §4.4 table — a declared
/// dimension outside this closed set is a `DimensionIdentity`
/// refusal; which are *required* is the declared `required` list,
/// never every dimension and never a dropped difficult one).
pub const DIMENSIONS: [&str; 7] = [
    "capability",
    "reliability",
    "grounding",
    "efficiency",
    "autonomy",
    "security_compliance",
    "portability_evolvability",
];

/// Estimand spellings whose inference is admitted here.
const ESTIMAND_KINDS: [&str; 3] = ["binary", "bounded", "relative"];
/// Estimand spellings recorded `unavailable` — nonlinear pass^k /
/// conditional eligible-only outcomes lack an admitted inference and
/// stay unavailable, never silently dropped (spec §4.4).
const UNAVAILABLE_ESTIMANDS: [&str; 2] = ["pass^k", "conditional_eligible"];
/// Downstream-use spellings this report never carries — the
/// grouping exception grants no honoured-seed/CRN/R2/headline claim.
const FORBIDDEN_USES: [&str; 6] = [
    "component_headline",
    "headline",
    "crn",
    "common_random_numbers",
    "r2",
    "s3_claim",
];

// ── declared inputs (records-in — the caller parses the spec; this
// module never sees filters/JSON spec members) ─────────────────────

/// One authentic first-attempt H/O pair — actual identities, never
/// manufactured: the preassigned `(task, repeat, pair_id)` grouping
/// binds one H and one O first attempt under the declared policy.
#[derive(Debug, Clone, PartialEq)]
pub struct ManifestPair {
    /// The fixed task.
    pub task_id: String,
    /// The replicate slot (`0..repeats`).
    pub repeat: u32,
    /// The preassigned pair identity (unique).
    pub pair_id: String,
    /// The participant/subject identity — distinct across pairs
    /// (reused subjects break across-pair independence).
    pub subject: String,
    /// The design phase — must equal the manifest's declared phase.
    pub phase: String,
    /// The stratum — must equal the manifest's declared stratum.
    pub stratum: String,
    /// The policy this pair's participant ran under — must be a
    /// declared policy.
    pub policy: String,
    /// The H attempt's actual run identity.
    pub h_run_ref: String,
    /// The O attempt's actual run identity.
    pub o_run_ref: String,
    /// The H attempt's verdict identity.
    pub h_verdict_ref: String,
    /// The O attempt's verdict identity.
    pub o_verdict_ref: String,
    /// The replay evidence identity — `None` leaves the pair
    /// unmatched (`MissingReplay`), never a pass.
    pub replay_ref: Option<String>,
    /// The attempt index — must be `1` (first attempts only).
    pub attempt: u32,
    /// The requested seed (recorded — presence alone never admits).
    pub seed_requested: Option<i64>,
    /// The honoured seed (recorded — grants no honoured-seed
    /// coupling or CRN claim).
    pub seed_honoured: Option<i64>,
    /// The H outcome (ppm — the admitted pair feeds M-E unchanged).
    pub h: Option<i64>,
    /// The O outcome (ppm).
    pub o: Option<i64>,
}

/// The pair manifest — the authentic first-attempt design records.
#[derive(Debug, Clone, PartialEq)]
pub struct PairManifest {
    /// Planned task count `T`.
    pub tasks: u32,
    /// Planned equal repeats per task `R` (`N = T·R`).
    pub repeats: u32,
    /// The declared weighting — `None`/`"equal"` only.
    pub weights: Option<String>,
    /// The declared design phase every pair must carry.
    pub phase: String,
    /// The declared stratum every pair must carry.
    pub stratum: String,
    /// The declared dependence model — `None`/
    /// [`REGISTERED_DEPENDENCE`] only.
    pub dependence: Option<String>,
    /// The declared independence rationale identity (a design
    /// assumption, never proved by reset logs or ids — required,
    /// non-empty).
    pub independence_rationale_ref: String,
    /// The declared pair records.
    pub pairs: Vec<ManifestPair>,
}

/// The submitted evidence shape — only the authentic manifest is
/// admissible; the five known-inadequate shapes and unknown
/// spellings are typed `InauthenticEvidence` refusals.
#[derive(Debug, Clone, PartialEq)]
pub enum AdmissionEvidence {
    /// The authentic first-attempt H/O pair manifest.
    AuthenticFirstAttempt(PairManifest),
    /// A recorded model-I/O corpus replay (the s3_12b fixture
    /// shape — fixture evidence, never authentic admission).
    RecordedModelIo {
        /// The corpus identity.
        corpus_ref: String,
    },
    /// An `artifact_benefit` comparison/`ParityReport` wrapper
    /// alone — a report, not a manifest.
    ArtifactBenefitWrapper {
        /// The report identity.
        report_ref: String,
    },
    /// Model snapshot maps — equal empty maps are never agreement.
    ModelSnapshotMaps {
        /// The H snapshot map identity.
        h_map_ref: String,
        /// The O snapshot map identity.
        o_map_ref: String,
    },
    /// Oracle transport equality alone — transports match says
    /// nothing about paired authentic outcomes.
    OracleTransportEquality {
        /// The transport identity.
        transport_ref: String,
    },
    /// Seed presence records alone.
    SeedPresence {
        /// The seed record identities.
        seed_refs: Vec<String>,
    },
    /// Any other declared spelling — refused by shape.
    Other(String),
}

impl AdmissionEvidence {
    /// The shape spelling recorded on the refusal.
    pub fn shape(&self) -> &str {
        match self {
            AdmissionEvidence::AuthenticFirstAttempt(_) => REGISTERED_SOURCE,
            AdmissionEvidence::RecordedModelIo { .. } => "recorded_model_io",
            AdmissionEvidence::ArtifactBenefitWrapper { .. } => "artifact_benefit_wrapper",
            AdmissionEvidence::ModelSnapshotMaps { .. } => "model_snapshot_maps",
            AdmissionEvidence::OracleTransportEquality { .. } => "oracle_transport_equality",
            AdmissionEvidence::SeedPresence { .. } => "seed_presence",
            AdmissionEvidence::Other(s) => s.as_str(),
        }
    }
}

/// One dimension-map entry — the signed pre-data
/// dimension→metric/estimand/population/range/unit/margin/
/// applicability/evidence binding.
#[derive(Debug, Clone, PartialEq)]
pub struct DimensionEntry {
    /// The contract dimension ([`DIMENSIONS`]).
    pub dimension: String,
    /// The required metric identity.
    pub metric_ref: String,
    /// The estimand/target identity.
    pub estimand: String,
    /// The estimand kind — `binary`/`bounded`/`relative` admitted;
    /// `pass^k`/`conditional_eligible` stay unavailable.
    pub estimand_kind: String,
    /// The declared population.
    pub population: String,
    /// The declared physical range `[a,b]` — required for
    /// `bounded`/`relative` estimands.
    pub range: Option<(i64, i64)>,
    /// The declared unit.
    pub unit: String,
    /// The pre-registered margin dimension this entry binds.
    pub margin_ref: String,
    /// `applicable` | `inapplicable` (typed inapplicability —
    /// records its reason, never counts as passed).
    pub applicability: String,
    /// The inapplicability reason — required when `inapplicable`.
    pub inapplicable_reason: Option<String>,
    /// The endpoint's evidence binding identity.
    pub evidence_ref: String,
}

/// The signed pre-data dimension map — `signature_ref` binds the
/// map content; `map_seq < first_outcome_seq` is the declared
/// pre-data ordering (records-in; the temporal authority is the
/// producer's).
#[derive(Debug, Clone, PartialEq)]
pub struct DimensionMap {
    /// The map's signature/registration identity (required).
    pub signature_ref: String,
    /// The map's registration sequence.
    pub map_seq: u64,
    /// The first pair-outcome sequence the map must precede.
    pub first_outcome_seq: u64,
    /// The required dimensions — coverage is checked, never
    /// demanded of every dimension and never dropped.
    pub required: Vec<String>,
    /// The dimension entries (one per dimension).
    pub entries: Vec<DimensionEntry>,
}

/// One policy's applicability evidence binding.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyEvidence {
    /// The policy id.
    pub policy: String,
    /// The applicability evidence identity.
    pub evidence_ref: String,
}

/// The policy-applicability declaration — native first-wave
/// efficacy needs adapter admission applicable to **both** actual
/// policies, or preregistered justified reuse of identical bridge
/// semantics (spec §4.5).
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyApplicability {
    /// Per-policy admission evidence covering every declared
    /// policy.
    BothPolicies {
        /// The per-policy evidence bindings.
        evidence: Vec<PolicyEvidence>,
    },
    /// Predeclared identical-bridge reuse — `bridge_ref` names the
    /// bridge-semantics proof and `bridge_seq < first_outcome_seq`
    /// is the predeclared ordering.
    IdenticalBridge {
        /// The identical-bridge proof identity.
        bridge_ref: String,
        /// The bridge declaration sequence.
        bridge_seq: u64,
    },
    /// Baseline-only parity — covers one policy, never admits the
    /// changed one.
    BaselineOnly {
        /// The covered policy ids.
        covered: Vec<String>,
    },
}

/// A declared original-runner transfer claim — transfer is a
/// separately admitted design (spec §4.5); this module refuses
/// every claim spelling, distinguishing the failure shape.
#[derive(Debug, Clone, PartialEq)]
pub struct TransferDecl {
    /// The claim — `"none"` asserts no transfer (admitted); any
    /// other spelling is checked and refused.
    pub claim: String,
    /// The baseline-policy bias bound (ppm).
    pub bound_a_ppm: Option<i64>,
    /// The changed-policy bias bound (ppm) — absent leaves
    /// treatment bias unknown (`b_A` alone).
    pub bound_b_ppm: Option<i64>,
    /// The registered calibration budget κ (ppm).
    pub kappa_ppm: Option<i64>,
    /// The declared uncertainty allocation — `"mixed"` (e.g. a
    /// Bayesian efficacy lower endpoint plus frequentist
    /// calibration intervals) is never a joint certificate.
    pub uncertainty: String,
}

/// The actual source/verifier/rights pins (the D/R gate's named
/// prerequisite records — required, non-empty).
#[derive(Debug, Clone, PartialEq)]
pub struct SourcePins {
    /// The evidence source identity.
    pub source_ref: String,
    /// The verifier identity.
    pub verifier_ref: String,
    /// The rights/consent identity.
    pub rights_ref: String,
}

/// The declared admission input (records-in).
#[derive(Debug, Clone, PartialEq)]
pub struct ParityAdmissionDecl {
    /// The actual participant/configuration identity.
    pub participant_ref: String,
    /// The declared policy set (e.g. `["A", "B"]`).
    pub policies: Vec<String>,
    /// The submitted evidence.
    pub evidence: AdmissionEvidence,
    /// The signed pre-data dimension map — `None` is `MissingMap`.
    pub dimension_map: Option<DimensionMap>,
    /// The policy-applicability declaration.
    pub applicability: PolicyApplicability,
    /// The declared transfer claim, if any.
    pub transfer: Option<TransferDecl>,
    /// The actual source/verifier/rights pins.
    pub pins: SourcePins,
    /// The declared downstream use — headline/CRN/R2 spellings
    /// refuse (`FORBIDDEN_USES`).
    pub use_as: Option<String>,
}

// ── the closed refusal set ────────────────────────────────────────

/// `parity_admission`'s refusal vocabulary — typed facts, never
/// warnings.
#[derive(Debug, Clone, PartialEq)]
pub enum AdmissionError {
    /// The submitted evidence shape cannot establish authentic
    /// admission (recorded model-I/O fixtures, wrapper reports,
    /// equal empty snapshot maps, transport equality, seed
    /// presence, or an unknown spelling).
    InauthenticEvidence {
        /// The submitted shape.
        shape: String,
    },
    /// A required declared key is absent or empty.
    MissingKey {
        /// The missing member.
        member: String,
    },
    /// No signed pre-data dimension map was supplied.
    MissingMap,
    /// The dimension map is not pre-data (`map_seq` ≥
    /// `first_outcome_seq` — a late map admits nothing).
    LateMap {
        /// The map's registration sequence.
        map_seq: u64,
        /// The first pair-outcome sequence.
        first_outcome_seq: u64,
    },
    /// A declared dimension or estimand spelling is outside the
    /// closed vocabulary, or a dimension is bound twice.
    DimensionIdentity {
        /// The detail.
        detail: String,
    },
    /// A required dimension has no map entry — a dropped
    /// difficult endpoint blocks the whole conjunction.
    MissingDimension {
        /// The required dimension.
        dimension: String,
    },
    /// A required dimension is typed `inapplicable` — the reason
    /// is recorded, it never counts as passed.
    RequiredInapplicable {
        /// The dimension.
        dimension: String,
        /// The recorded reason.
        reason: String,
    },
    /// An estimand whose inference is not admitted (nonlinear
    /// pass^k, conditional eligible-only) — stays unavailable,
    /// never silently dropped.
    UnavailableMethod {
        /// The dimension.
        dimension: String,
        /// The estimand kind.
        estimand: String,
    },
    /// A `bounded`/`relative` estimand lacks a declared physical
    /// range.
    UnknownRange {
        /// The dimension.
        dimension: String,
    },
    /// No registered margin binds the entry's `margin_ref`
    /// (margins come only from the pre-registration — never the
    /// caller).
    MissingMargin {
        /// The unbound margin ref.
        margin_ref: String,
    },
    /// The registered margin's kind does not match the estimand
    /// kind (`absolute` for `binary`/`bounded`, `relative` for
    /// `relative`).
    MarginKindMismatch {
        /// The dimension.
        dimension: String,
        /// The registered margin kind.
        margin_kind: String,
        /// The estimand kind.
        estimand_kind: String,
    },
    /// The planned design is unequal — pairs ≠ `T·R`, a task
    /// carrying other than `R` repeats, or a non-equal weighting.
    UnequalDesign {
        /// The detail.
        detail: String,
    },
    /// The declared repeats fall under the adapter floor (≥3
    /// attempts/task).
    AdapterFloor {
        /// The declared repeats.
        repeats: u32,
    },
    /// The declared repeats fall under the S3 floor (≥5
    /// attempts/task/arm).
    S3Floor {
        /// The declared repeats.
        repeats: u32,
    },
    /// Pair identity is malformed — duplicate `pair_id`, a
    /// `(task, repeat)` collision, an out-of-range repeat index,
    /// or a phase/stratum that does not match its manifest.
    PairIdentity {
        /// The detail.
        detail: String,
    },
    /// A pair attempt index is not `1` — only first attempts are
    /// authentic.
    NotFirstAttempt {
        /// The pair.
        pair_id: String,
    },
    /// A pair's actual identity members are empty (run/verdict
    /// refs, subject, policy) or its policy is undeclared.
    MissingIdentity {
        /// The pair.
        pair_id: String,
        /// The missing member.
        member: String,
    },
    /// One subject identity appears on more than one pair.
    ReusedSubject {
        /// The reused subject.
        subject: String,
    },
    /// Pairs lacking required replay evidence — they stay
    /// unmatched, never a pass.
    MissingReplay {
        /// The unmatched pair ids.
        pairs: Vec<String>,
    },
    /// A dependence model other than across-pair independence was
    /// declared (honoured-seed coupling, CRN, shared shocks,
    /// carryover are unsupported).
    UnsupportedDependence {
        /// The declared dependence.
        declared: String,
    },
    /// A declared policy lacks applicability coverage —
    /// baseline-only parity never admits the changed policy.
    PolicyScope {
        /// The uncovered policy.
        policy: String,
    },
    /// An applicability evidence binding names an undeclared
    /// policy.
    UnknownPolicy {
        /// The policy.
        policy: String,
    },
    /// The identical-bridge proof was not predeclared
    /// (`bridge_seq` ≥ `first_outcome_seq`).
    LateBridge {
        /// The bridge declaration sequence.
        bridge_seq: u64,
        /// The first pair-outcome sequence.
        first_outcome_seq: u64,
    },
    /// A transfer claim declaring mixed uncertainty — a Bayesian
    /// efficacy lower endpoint plus frequentist calibration
    /// intervals is never a joint 95% certificate.
    MixedUncertainty {
        /// The detail.
        detail: String,
    },
    /// A transfer claim with only the baseline-policy bound —
    /// `b_A` alone leaves treatment bias unknown (the
    /// `b_A = −.10` / `b_B = .10` counterexample).
    BaselineOnlyTransfer {
        /// The detail.
        detail: String,
    },
    /// A transfer claim whose implied bias bounds exceed the
    /// calibration budget (`|b_A| + |b_B| > κ`).
    TransferBudget {
        /// `|b_A| + |b_B|` (ppm).
        bound_ppm: i64,
        /// κ (ppm).
        kappa_ppm: i64,
    },
    /// Any other transfer claim — original-runner transfer is a
    /// separately admitted design; no certificate is admitted
    /// here.
    UnsupportedTransfer {
        /// The declared claim.
        declared: String,
    },
    /// A downstream use the grouping exception never grants —
    /// honoured-seed/CRN/R2/component-headline claims.
    UnsupportedClaim {
        /// The declared use.
        declared: String,
    },
    /// The declared method is not `parity_admission`.
    UnknownMethod {
        /// The declared method.
        declared: String,
    },
    /// The `parity_admission` method declared on a kind that does
    /// not compute it (a metadata-only decoration, never a
    /// silent no-op).
    MetadataOnly {
        /// The offending kind.
        kind: String,
    },
    /// The pre-registration margin record refused decode (the
    /// margin authority's own refusal, wrapped).
    MarginAuthority {
        /// The detail.
        detail: String,
    },
}

impl std::fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use AdmissionError as E;
        match self {
            E::InauthenticEvidence { shape } => {
                write!(f, "inauthentic evidence shape: {shape}")
            }
            E::MissingKey { member } => write!(f, "missing declared key: {member}"),
            E::MissingMap => write!(f, "no signed pre-data dimension map"),
            E::LateMap {
                map_seq,
                first_outcome_seq,
            } => write!(
                f,
                "dimension map not pre-data: map_seq {map_seq} ≥ first_outcome_seq {first_outcome_seq}"
            ),
            E::DimensionIdentity { detail } => {
                write!(f, "dimension map identity malformed: {detail}")
            }
            E::MissingDimension { dimension } => {
                write!(f, "required dimension dropped: {dimension}")
            }
            E::RequiredInapplicable { dimension, reason } => {
                write!(f, "required dimension {dimension} inapplicable: {reason}")
            }
            E::UnavailableMethod {
                dimension,
                estimand,
            } => write!(
                f,
                "estimand {estimand} on {dimension} has no admitted inference — stays unavailable"
            ),
            E::UnknownRange { dimension } => {
                write!(f, "no declared physical range: {dimension}")
            }
            E::MissingMargin { margin_ref } => {
                write!(f, "no registered margin binds {margin_ref}")
            }
            E::MarginKindMismatch {
                dimension,
                margin_kind,
                estimand_kind,
            } => write!(
                f,
                "margin kind {margin_kind} does not match {estimand_kind} estimand on {dimension}"
            ),
            E::UnequalDesign { detail } => write!(f, "unequal planned design: {detail}"),
            E::AdapterFloor { repeats } => {
                write!(f, "adapter floor unmet: {repeats} repeats < {ADAPTER_FLOOR}")
            }
            E::S3Floor { repeats } => {
                write!(f, "S3 floor unmet: {repeats} repeats < {S3_FLOOR} per task/arm")
            }
            E::PairIdentity { detail } => write!(f, "pair identity malformed: {detail}"),
            E::NotFirstAttempt { pair_id } => {
                write!(f, "pair {pair_id} is not a first attempt")
            }
            E::MissingIdentity { pair_id, member } => {
                write!(f, "pair {pair_id} missing identity member: {member}")
            }
            E::ReusedSubject { subject } => {
                write!(f, "reused subject across pairs: {subject}")
            }
            E::MissingReplay { pairs } => {
                write!(f, "required replay absent — unmatched pairs: {}", pairs.join(","))
            }
            E::UnsupportedDependence { declared } => {
                write!(f, "unsupported dependence declared: {declared}")
            }
            E::PolicyScope { policy } => {
                write!(f, "policy {policy} lacks applicability coverage")
            }
            E::UnknownPolicy { policy } => {
                write!(f, "applicability evidence names undeclared policy {policy}")
            }
            E::LateBridge {
                bridge_seq,
                first_outcome_seq,
            } => write!(
                f,
                "identical-bridge proof not predeclared: bridge_seq {bridge_seq} ≥ first_outcome_seq {first_outcome_seq}"
            ),
            E::MixedUncertainty { detail } => {
                write!(f, "mixed-uncertainty certificate: {detail}")
            }
            E::BaselineOnlyTransfer { detail } => {
                write!(f, "baseline-only transfer bound: {detail}")
            }
            E::TransferBudget {
                bound_ppm,
                kappa_ppm,
            } => write!(
                f,
                "transfer bias bounds {bound_ppm} ppm exceed calibration budget {kappa_ppm} ppm"
            ),
            E::UnsupportedTransfer { declared } => {
                write!(f, "transfer claim unadmitted here: {declared}")
            }
            E::UnsupportedClaim { declared } => {
                write!(f, "unsupported claim: {declared} — no honoured-seed/CRN/R2/headline")
            }
            E::UnknownMethod { declared } => write!(f, "unknown method declared: {declared}"),
            E::MetadataOnly { kind } => {
                write!(f, "metadata-only method switch refused on kind {kind}")
            }
            E::MarginAuthority { detail } => {
                write!(f, "margin authority refused: {detail}")
            }
        }
    }
}

impl std::error::Error for AdmissionError {}

// ── the computed report ───────────────────────────────────────────

/// One dimension's mapped outcome on the report.
#[derive(Debug, Clone, PartialEq)]
pub struct DimensionOutcome {
    /// The dimension.
    pub dimension: String,
    /// `mapped` | `inapplicable` (the reason is a member — never a
    /// silent pass).
    pub status: String,
    /// The estimand kind.
    pub estimand_kind: String,
    /// The bound registered margin (echoed).
    pub margin: Json,
    /// The evidence binding.
    pub evidence_ref: String,
    /// The inapplicability reason (`None` when `mapped`).
    pub reason: Option<String>,
}

impl DimensionOutcome {
    fn to_json(&self) -> Json {
        Json::obj([
            ("dimension", Json::str(&self.dimension)),
            ("status", Json::str(&self.status)),
            ("estimand_kind", Json::str(&self.estimand_kind)),
            ("margin", self.margin.clone()),
            ("evidence_ref", Json::str(&self.evidence_ref)),
            (
                "reason",
                self.reason.as_ref().map(Json::str).unwrap_or(Json::Null),
            ),
        ])
    }
}

/// The admission result — every member a fact on the report. The
/// `admitted_pairs` are the pair ids R5.38's
/// `directional_kl_equivalence` consumes unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct ParityAdmission {
    /// The actual participant/configuration.
    pub participant_ref: String,
    /// The declared policy set.
    pub policies: Vec<String>,
    /// The admitted (matched) pair ids.
    pub admitted_pairs: Vec<String>,
    /// Planned `N = T·R`.
    pub planned_n: i64,
    /// Planned `T`/`R`.
    pub design: (u32, u32),
    /// The satisfied floors.
    pub floors: Vec<String>,
    /// `(requested, honoured)` seed-record counts (recorded, never
    /// an admission basis).
    pub seed_records: (i64, i64),
    /// The dimension outcomes (map order).
    pub dimensions: Vec<DimensionOutcome>,
    /// The applicability mode that bound
    /// (`both_policies`|`identical_bridge`).
    pub applicability: String,
    /// The verdict — `admitted` only (every failure is a typed
    /// refusal on the error path).
    pub admission: String,
    /// The S.1 method-metadata record.
    pub metadata: MethodMetadata,
}

impl ParityAdmission {
    /// The canonical JSON member — the report body payload.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(VERSION)),
            ("participant_ref", Json::str(&self.participant_ref)),
            (
                "policies",
                Json::Arr(self.policies.iter().map(Json::str).collect()),
            ),
            ("admission", Json::str(&self.admission)),
            ("applicability", Json::str(&self.applicability)),
            (
                "design",
                Json::obj([
                    ("tasks", Json::Int(self.design.0 as i64)),
                    ("repeats", Json::Int(self.design.1 as i64)),
                    ("planned_n", Json::Int(self.planned_n)),
                ]),
            ),
            (
                "floors",
                Json::Arr(self.floors.iter().map(Json::str).collect()),
            ),
            (
                "seed_records",
                Json::obj([
                    ("requested", Json::Int(self.seed_records.0)),
                    ("honoured", Json::Int(self.seed_records.1)),
                ]),
            ),
            (
                "admitted_pairs",
                Json::Arr(self.admitted_pairs.iter().map(Json::str).collect()),
            ),
            (
                "dimensions",
                Json::Arr(
                    self.dimensions
                        .iter()
                        .map(DimensionOutcome::to_json)
                        .collect(),
                ),
            ),
            ("metadata", self.metadata.to_json()),
        ])
    }
}

// ── the validator ─────────────────────────────────────────────────

fn missing(member: &str) -> AdmissionError {
    AdmissionError::MissingKey {
        member: member.to_string(),
    }
}

fn nonempty(v: &str, member: &str) -> Result<(), AdmissionError> {
    if v.is_empty() {
        return Err(missing(member));
    }
    Ok(())
}

/// `parity_admission(decl, margins)` — the admission validator.
/// Every check is a typed refusal; `Ok` is emitted only when the
/// authentic manifest, the signed pre-data map and the
/// policy-applicability declaration all hold. `margins` come only
/// from the pre-registration (the caller resolves them — never a
/// member of `decl`).
pub fn parity_admission(
    decl: &ParityAdmissionDecl,
    margins: &[RegisteredMargin],
) -> Result<ParityAdmission, AdmissionError> {
    use AdmissionError as E;

    // ── downstream-use spellings (cheap first) ────────────────────
    if let Some(u) = &decl.use_as {
        if FORBIDDEN_USES.contains(&u.as_str()) {
            return Err(E::UnsupportedClaim {
                declared: u.clone(),
            });
        }
    }

    // ── identity pins ─────────────────────────────────────────────
    nonempty(&decl.participant_ref, "participant_ref")?;
    nonempty(&decl.pins.source_ref, "pins.source_ref")?;
    nonempty(&decl.pins.verifier_ref, "pins.verifier_ref")?;
    nonempty(&decl.pins.rights_ref, "pins.rights_ref")?;
    if decl.policies.is_empty() || decl.policies.iter().any(|p| p.is_empty()) {
        return Err(missing("policies"));
    }
    let declared_policies: BTreeSet<&str> = decl.policies.iter().map(String::as_str).collect();

    // ── evidence shape ────────────────────────────────────────────
    let manifest = match &decl.evidence {
        AdmissionEvidence::AuthenticFirstAttempt(m) => m,
        other => {
            return Err(E::InauthenticEvidence {
                shape: other.shape().to_string(),
            })
        }
    };

    // ── design equality / dependence / rationale ──────────────────
    if let Some(w) = &manifest.weights {
        if w != REGISTERED_WEIGHTS {
            return Err(E::UnequalDesign {
                detail: format!("declared weights {w} — only equal identifies the finite target"),
            });
        }
    }
    if let Some(d) = &manifest.dependence {
        if d != REGISTERED_DEPENDENCE {
            return Err(E::UnsupportedDependence {
                declared: d.clone(),
            });
        }
    }
    nonempty(
        &manifest.independence_rationale_ref,
        "manifest.independence_rationale_ref",
    )?;
    if manifest.tasks == 0 || manifest.repeats == 0 {
        return Err(E::UnequalDesign {
            detail: "tasks/repeats must be ≥ 1".into(),
        });
    }
    let planned = manifest.tasks as usize * manifest.repeats as usize;
    if manifest.pairs.len() != planned {
        return Err(E::UnequalDesign {
            detail: format!(
                "pairs {} ≠ T·R {} — the planned universe is never shrunk",
                manifest.pairs.len(),
                planned
            ),
        });
    }

    // ── floors ────────────────────────────────────────────────────
    if manifest.repeats < ADAPTER_FLOOR {
        return Err(E::AdapterFloor {
            repeats: manifest.repeats,
        });
    }
    if manifest.repeats < S3_FLOOR {
        return Err(E::S3Floor {
            repeats: manifest.repeats,
        });
    }

    // ── pair identities, first-attempt, replay, subjects ──────────
    let mut pair_ids = BTreeSet::new();
    let mut task_repeats: BTreeMap<&str, BTreeSet<u32>> = BTreeMap::new();
    let mut subjects = BTreeSet::new();
    let mut unmatched = Vec::new();
    let mut seed_req = 0i64;
    let mut seed_hon = 0i64;
    for p in &manifest.pairs {
        if p.pair_id.is_empty() || !pair_ids.insert(p.pair_id.as_str()) {
            return Err(E::PairIdentity {
                detail: format!("empty or duplicate pair_id {:?}", p.pair_id),
            });
        }
        if p.task_id.is_empty() {
            return Err(E::PairIdentity {
                detail: format!("pair {} empty task_id", p.pair_id),
            });
        }
        if p.repeat >= manifest.repeats {
            return Err(E::PairIdentity {
                detail: format!(
                    "pair {} repeat {} ≥ repeats {}",
                    p.pair_id, p.repeat, manifest.repeats
                ),
            });
        }
        if !task_repeats
            .entry(p.task_id.as_str())
            .or_default()
            .insert(p.repeat)
        {
            return Err(E::PairIdentity {
                detail: format!("(task, repeat) collision at {}:{}", p.task_id, p.repeat),
            });
        }
        if p.phase.is_empty() || p.phase != manifest.phase {
            return Err(E::PairIdentity {
                detail: format!(
                    "pair {} phase {:?} ≠ declared {:?}",
                    p.pair_id, p.phase, manifest.phase
                ),
            });
        }
        if p.stratum.is_empty() || p.stratum != manifest.stratum {
            return Err(E::PairIdentity {
                detail: format!(
                    "pair {} stratum {:?} ≠ declared {:?}",
                    p.pair_id, p.stratum, manifest.stratum
                ),
            });
        }
        if p.attempt != 1 {
            return Err(E::NotFirstAttempt {
                pair_id: p.pair_id.clone(),
            });
        }
        for (v, member) in [
            (&p.subject, "subject"),
            (&p.h_run_ref, "h_run_ref"),
            (&p.o_run_ref, "o_run_ref"),
            (&p.h_verdict_ref, "h_verdict_ref"),
            (&p.o_verdict_ref, "o_verdict_ref"),
            (&p.policy, "policy"),
        ] {
            if v.is_empty() {
                return Err(E::MissingIdentity {
                    pair_id: p.pair_id.clone(),
                    member: member.to_string(),
                });
            }
        }
        if !declared_policies.contains(p.policy.as_str()) {
            return Err(E::MissingIdentity {
                pair_id: p.pair_id.clone(),
                member: format!("policy {} not declared", p.policy),
            });
        }
        if !subjects.insert(p.subject.as_str()) {
            return Err(E::ReusedSubject {
                subject: p.subject.clone(),
            });
        }
        if p.replay_ref.is_none() {
            unmatched.push(p.pair_id.clone());
        }
        if p.seed_requested.is_some() {
            seed_req += 1;
        }
        if p.seed_honoured.is_some() {
            seed_hon += 1;
        }
    }
    for (task, reps) in &task_repeats {
        if reps.len() != manifest.repeats as usize {
            return Err(E::UnequalDesign {
                detail: format!(
                    "task {task} carries {} repeats ≠ R {}",
                    reps.len(),
                    manifest.repeats
                ),
            });
        }
    }
    if task_repeats.len() != manifest.tasks as usize {
        return Err(E::UnequalDesign {
            detail: format!(
                "{} tasks ≠ declared T {}",
                task_repeats.len(),
                manifest.tasks
            ),
        });
    }
    if !unmatched.is_empty() {
        return Err(E::MissingReplay { pairs: unmatched });
    }

    // ── the signed pre-data dimension map ─────────────────────────
    let map = decl.dimension_map.as_ref().ok_or(E::MissingMap)?;
    nonempty(&map.signature_ref, "dimension_map.signature_ref")?;
    if map.map_seq >= map.first_outcome_seq {
        return Err(E::LateMap {
            map_seq: map.map_seq,
            first_outcome_seq: map.first_outcome_seq,
        });
    }
    let known: BTreeSet<&str> = DIMENSIONS.iter().copied().collect();
    let mut seen_dims = BTreeSet::new();
    for r in &map.required {
        if !known.contains(r.as_str()) {
            return Err(E::DimensionIdentity {
                detail: format!("required dimension {r} outside the closed set"),
            });
        }
    }
    for e in &map.entries {
        if !known.contains(e.dimension.as_str()) {
            return Err(E::DimensionIdentity {
                detail: format!("entry dimension {} outside the closed set", e.dimension),
            });
        }
        if !seen_dims.insert(e.dimension.as_str()) {
            return Err(E::DimensionIdentity {
                detail: format!("dimension {} bound twice", e.dimension),
            });
        }
    }
    for r in &map.required {
        if !seen_dims.contains(r.as_str()) {
            return Err(E::MissingDimension {
                dimension: r.clone(),
            });
        }
    }

    // ── per-entry bindings ────────────────────────────────────────
    let mut outcomes = Vec::with_capacity(map.entries.len());
    for e in &map.entries {
        // Typed inapplicability is a declaration — its reason is
        // recorded and it never counts as a passed endpoint; the
        // metric bindings are not invoked (a required dimension
        // still blocks the whole conjunction).
        if e.applicability == "inapplicable" {
            let reason = e
                .inapplicable_reason
                .clone()
                .filter(|r| !r.is_empty())
                .ok_or_else(|| missing("dimension_map.entries[].inapplicable_reason"))?;
            if map.required.iter().any(|r| r == &e.dimension) {
                return Err(E::RequiredInapplicable {
                    dimension: e.dimension.clone(),
                    reason,
                });
            }
            outcomes.push(DimensionOutcome {
                dimension: e.dimension.clone(),
                status: "inapplicable".into(),
                estimand_kind: e.estimand_kind.clone(),
                margin: Json::Null,
                evidence_ref: e.evidence_ref.clone(),
                reason: Some(reason),
            });
            continue;
        }
        if e.applicability != "applicable" {
            return Err(E::DimensionIdentity {
                detail: format!(
                    "unknown applicability {} on {}",
                    e.applicability, e.dimension
                ),
            });
        }
        nonempty(&e.metric_ref, "dimension_map.entries[].metric_ref")?;
        nonempty(&e.estimand, "dimension_map.entries[].estimand")?;
        nonempty(&e.population, "dimension_map.entries[].population")?;
        nonempty(&e.unit, "dimension_map.entries[].unit")?;
        nonempty(&e.margin_ref, "dimension_map.entries[].margin_ref")?;
        nonempty(&e.evidence_ref, "dimension_map.entries[].evidence_ref")?;
        if UNAVAILABLE_ESTIMANDS.contains(&e.estimand_kind.as_str())
            || e.estimand_kind.starts_with("pass^")
        {
            return Err(E::UnavailableMethod {
                dimension: e.dimension.clone(),
                estimand: e.estimand_kind.clone(),
            });
        }
        if !ESTIMAND_KINDS.contains(&e.estimand_kind.as_str()) {
            return Err(E::DimensionIdentity {
                detail: format!(
                    "unknown estimand kind {} on {}",
                    e.estimand_kind, e.dimension
                ),
            });
        }
        if matches!(e.estimand_kind.as_str(), "bounded" | "relative") {
            match e.range {
                None => {
                    return Err(E::UnknownRange {
                        dimension: e.dimension.clone(),
                    })
                }
                Some((a, b)) if a > b => {
                    return Err(E::DimensionIdentity {
                        detail: format!("malformed range [{a},{b}] on {}", e.dimension),
                    })
                }
                _ => {}
            }
        }
        let margin = margins
            .iter()
            .find(|m| m.dimension == e.margin_ref)
            .ok_or_else(|| E::MissingMargin {
                margin_ref: e.margin_ref.clone(),
            })?;
        let required_kind = if e.estimand_kind == "relative" {
            "relative"
        } else {
            "absolute"
        };
        if margin.kind != required_kind {
            return Err(E::MarginKindMismatch {
                dimension: e.dimension.clone(),
                margin_kind: margin.kind.clone(),
                estimand_kind: e.estimand_kind.clone(),
            });
        }
        outcomes.push(DimensionOutcome {
            dimension: e.dimension.clone(),
            status: "mapped".into(),
            estimand_kind: e.estimand_kind.clone(),
            margin: Json::obj([
                ("dimension", Json::str(&margin.dimension)),
                ("margin_ppm", Json::Int(margin.margin_ppm)),
                ("kind", Json::str(&margin.kind)),
            ]),
            evidence_ref: e.evidence_ref.clone(),
            reason: None,
        });
    }

    // ── policy applicability ──────────────────────────────────────
    let applicability = match &decl.applicability {
        PolicyApplicability::BothPolicies { evidence } => {
            let mut covered = BTreeSet::new();
            for pe in evidence {
                if pe.policy.is_empty() || pe.evidence_ref.is_empty() {
                    return Err(missing("applicability.evidence[]"));
                }
                if !declared_policies.contains(pe.policy.as_str()) {
                    return Err(E::UnknownPolicy {
                        policy: pe.policy.clone(),
                    });
                }
                covered.insert(pe.policy.as_str());
            }
            for p in &decl.policies {
                if !covered.contains(p.as_str()) {
                    return Err(E::PolicyScope { policy: p.clone() });
                }
            }
            "both_policies"
        }
        PolicyApplicability::IdenticalBridge {
            bridge_ref,
            bridge_seq,
        } => {
            nonempty(bridge_ref, "applicability.bridge_ref")?;
            if *bridge_seq >= map.first_outcome_seq {
                return Err(E::LateBridge {
                    bridge_seq: *bridge_seq,
                    first_outcome_seq: map.first_outcome_seq,
                });
            }
            "identical_bridge"
        }
        PolicyApplicability::BaselineOnly { covered } => {
            let uncovered = decl
                .policies
                .iter()
                .find(|p| !covered.contains(p))
                .cloned()
                .unwrap_or_else(|| decl.policies[0].clone());
            return Err(E::PolicyScope { policy: uncovered });
        }
    };

    // ── transfer claims (none admitted here) ─────────────────────
    if let Some(t) = &decl.transfer {
        if t.claim != "none" {
            if t.uncertainty == "mixed" {
                return Err(E::MixedUncertainty {
                    detail: format!(
                        "claim {} mixes Bayesian and frequentist legs — never a joint certificate",
                        t.claim
                    ),
                });
            }
            if t.bound_b_ppm.is_none() {
                return Err(E::BaselineOnlyTransfer {
                    detail: format!(
                        "claim {} carries only the baseline bound — treatment bias stays unknown",
                        t.claim
                    ),
                });
            }
            if t.uncertainty != "joint" {
                return Err(E::UnsupportedTransfer {
                    declared: t.claim.clone(),
                });
            }
            let bound = t
                .bound_a_ppm
                .map(i64::abs)
                .unwrap_or(0)
                .saturating_add(t.bound_b_ppm.map(i64::abs).unwrap_or(0));
            match t.kappa_ppm {
                Some(k) if bound > k => {
                    return Err(E::TransferBudget {
                        bound_ppm: bound,
                        kappa_ppm: k,
                    })
                }
                _ => {
                    return Err(E::UnsupportedTransfer {
                        declared: t.claim.clone(),
                    })
                }
            }
        }
    }

    // ── the admitted report ───────────────────────────────────────
    let admitted_pairs: Vec<String> = pair_ids.iter().map(|s| s.to_string()).collect();
    let mut keys = BTreeMap::new();
    keys.insert("population".into(), "planned_pairs".into());
    keys.insert("participant".into(), decl.participant_ref.clone());
    Ok(ParityAdmission {
        participant_ref: decl.participant_ref.clone(),
        policies: decl.policies.clone(),
        admitted_pairs,
        planned_n: planned as i64,
        design: (manifest.tasks, manifest.repeats),
        floors: vec![
            format!("adapter>={ADAPTER_FLOOR}"),
            format!("s3>={S3_FLOOR}_per_task_arm"),
        ],
        seed_records: (seed_req, seed_hon),
        dimensions: outcomes,
        applicability: applicability.to_string(),
        admission: ADMITTED.to_string(),
        metadata: MethodMetadata {
            estimator: KIND.to_string(),
            version: VERSION.to_string(),
            target: "authentic_parity_admission".into(),
            assumptions: vec![
                "first_attempt_pairs".into(),
                REGISTERED_DEPENDENCE.into(),
                "equal_weights".into(),
                "pre_data_signed_map".into(),
            ],
            prior: None,
            keys,
            stratum_membership: vec![manifest.stratum.clone()],
            seed: 0,
            draws_requested: 0,
            draws_used: 0,
            floors: vec![
                format!("adapter>={ADAPTER_FLOOR}"),
                format!("s3>={S3_FLOOR}"),
            ],
            interval: ("descriptive".into(), None),
            refusal: None,
        },
    })
}
