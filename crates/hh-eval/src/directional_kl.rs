//! `directional_kl` — directional KL and correctly labelled bounded
//! equivalence (R5.38; REQ-D-304/305/306/307 primary — REQ-D-108's
//! canonical supersession target is D-304 only, with D-309/310
//! dependencies; GATE-G9-admitted Packet F / PADR-R5-6*, dossier
//! S.1–S.6; partition memo M-E).
//!
//! For one participant/policy/stratum with T fixed tasks and equal
//! repeats R, N = T·R first-attempt pairs `(task, repeat, pair_id)`;
//! `W+ = 1{Y_H=1,Y_O=0}` and `W− = 1{Y_H=0,Y_O=1}` carry counts
//! c+/c−. `K_η(x,N)=[L,U]` is the continuous Bernoulli KL inversion —
//! **concentration**, never CP/binomial inversion — with ppm
//! endpoints rounded outward. The TOST α=.05 uses `η=α/2=.025` per
//! directional endpoint (`L_Δ = L+−U−`, `U_Δ = U+−L−`; each Δ-tail
//! error ≤ α, joint interval ≥ .90) and `equivalent` holds only for
//! **strictly** `L_Δ > −ε ∧ U_Δ < ε`; touching/overlap is
//! `inconclusive`. By S11 an inferential `not_equivalent` needs the
//! separate **95% reporting interval** (`η=.0125`) wholly outside the
//! band under the applicable selected-claim allocation — a disjoint
//! .90 interval alone is descriptive. `u` uncertain pairs preserve
//! planned N through the completion envelope `[K_η(c/N).L,
//! K_η((c+u)/N).U]` — never missing-at-random. Bounded absolute
//! differences normalize `(D−a)/(b−a)` and transform back; relative
//! margins target the ratio of finite-manifest means through the
//! `C−=E_H−(1−ρ)E_O` / `C+=E_H−(1+ρ)E_O` contrasts with two negative
//! branches at `.025/m` each. One preregistered all-endpoints
//! conjunction uses the IUT (no endpoint Bonferroni); required
//! unknowns stay listed and can never be removed or averaged away.
//! No omnibus certificate, no credible-as-TOST, no observed-reference
//! division, mean-of-task-ratios, clipping or invented bound.

use std::collections::{BTreeMap, BTreeSet};

use hharness_wire::Json;

use crate::benefits::RegisteredMargin;
use crate::population::MethodMetadata;
use crate::stats::{kl_directional_bound, Interval, PPM};

/// The analysis `kind` spelling that routes to this estimator.
pub const KIND: &str = "directional_kl_equivalence";
/// The method contract version (report `metadata.version`).
pub const VERSION: &str = "directional-kl/1";
/// The frozen TOST per-directional-endpoint η — `α/2 = .025` (each
/// Δ-tail ≤ .05; four endpoints ⇒ joint coverage ≥ .90).
pub const ETA_TOST_PPM: i64 = 25_000;
/// The frozen reporting per-endpoint η — `α = .025` (joint .95; the
/// S11 inferential-negative interval).
pub const ETA_REPORT_PPM: i64 = 12_500;
/// The frozen per-negative-branch one-sided error `.025` for a
/// relative endpoint's S11 verdict (union controls .05).
pub const NEG_BRANCH_PPM: i64 = 25_000;
/// The declared dependence the pair design requires (across-pair
/// independence; within-pair dependence is allowed).
pub const REGISTERED_DEPENDENCE: &str = "independent_pairs";

/// One declared H/O first-attempt pair (records-in — the authentic
/// `(task, repeat, pair_id)` binding lands with Packet E / R5.39;
/// synthetic declarations are admissible inputs, never proof of
/// authentic pairing).
#[derive(Debug, Clone, PartialEq)]
pub struct PairDecl {
    /// The fixed task.
    pub task_id: String,
    /// The replicate slot (`0..repeats`).
    pub repeat: u32,
    /// The preassigned pair identity.
    pub pair_id: String,
    /// The subject/participant identity — distinct across pairs.
    pub subject: String,
    /// The design phase (diagnostic/pilot/parity/…).
    pub phase: String,
    /// The pair's stratum — must equal the endpoint's.
    pub stratum: String,
    /// The H outcome (ppm in the endpoint's declared unit —
    /// `0`|`PPM` for `binary`; within `range_h` otherwise).
    /// `None` = uncertain (missingness, never zero).
    pub h: Option<i64>,
    /// The O outcome (same contract).
    pub o: Option<i64>,
}

/// The endpoint's estimand kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointKind {
    /// Binary first-attempt success — the c+/c− directional
    /// disagreement construction (absolute margin).
    Binary,
    /// Bounded absolute difference `D = X_H − X_O ∈ [a,b]` —
    /// normalized-mean construction (absolute margin).
    Bounded,
    /// Relative margin ρ on the ratio of finite-manifest means —
    /// the `C−`/`C+` contrast construction (relative margin).
    Relative,
}

/// One declared endpoint: margin dimension, estimand kind, planned
/// `tasks × repeats` pair design and declared physical ranges
/// (ppm-of-declared-unit — one canonical scale; `a=b` is exact,
/// zero sample variance never proves zero range).
#[derive(Debug, Clone)]
pub struct EndpointDecl {
    /// The endpoint/dimension id — binds a registered margin.
    pub endpoint: String,
    /// The single stratum this endpoint reads (never pooled).
    pub stratum: String,
    /// A required endpoint (frozen conjunction member when the
    /// conjunction is declared — can never be removed).
    pub required: bool,
    /// The estimand kind.
    pub kind: EndpointKind,
    /// Declared physical range of `X_H` `[a,b]` ppm — required for
    /// `bounded`/`relative` (absent/unbounded ⇒ the required
    /// inference is unavailable).
    pub range_h: Option<(i64, i64)>,
    /// Declared physical range of `X_O` — required for
    /// `bounded`/`relative`.
    pub range_o: Option<(i64, i64)>,
    /// The frozen zero/near-zero-reference convention — only
    /// `"absolute"` is admitted (falls back to the bounded-absolute
    /// test under a separately registered `{endpoint}:absolute`
    /// margin); absent ⇒ zero reference refuses.
    pub zero_reference: Option<String>,
    /// Planned task count `T` and equal repeats `R` (`N = T·R` —
    /// unequal weights/repeats are a different design).
    pub tasks: u32,
    /// Planned equal repeats per task.
    pub repeats: u32,
    /// The declared pair records.
    pub pairs: Vec<PairDecl>,
}

/// The one preregistered all-endpoints/all-strata **equivalent**
/// conjunction (the narrow IUT — no additional endpoint Bonferroni).
#[derive(Debug, Clone)]
pub struct ConjunctionDecl {
    /// The preregistered claim reference.
    pub claim_ref: String,
    /// The frozen member set (endpoint ids; absent members are
    /// unknown, listed, and block the conjunction — never removed).
    pub members: Vec<String>,
}

/// The declared method inputs (records-in — the caller parses the
/// spec; this module never sees filters/JSON spec members).
#[derive(Debug, Clone)]
pub struct KlEquivalenceDecl {
    /// The declared endpoints.
    pub endpoints: Vec<EndpointDecl>,
    /// The one preregistered IUT conjunction, when declared.
    pub conjunction: Option<ConjunctionDecl>,
    /// The selected-claim family size `m ≥ 1` — every non-conjunction
    /// endpoint's bound levels allocate `η/m`; a declared `m` below
    /// the actual selected count is a family-identity refusal.
    pub selected_claims: u32,
    /// The declared dependence model — `None`/[`REGISTERED_DEPENDENCE`]
    /// only (shared shocks/carryover/reused subjects are unsupported).
    pub dependence: Option<String>,
    /// The declared weighting — `None`/`"equal"` only (`w = 1/N`
    /// identifies the finite target).
    pub weights: Option<String>,
    /// The declared downstream use — omnibus/efficacy-correction/
    /// credible/TOST-family spellings refuse.
    pub use_as: Option<String>,
    /// The declared reporting level — must equal `.95`
    /// (`950_000` ppm; the frozen reporting interval).
    pub level_ppm: i64,
}

/// The closed refusal set — typed facts, never warnings.
#[derive(Debug, Clone, PartialEq)]
pub enum KlEquivalenceError {
    /// A required declared key is absent or empty (endpoint/stratum/
    /// task/pair_id/subject/phase keys — empty pins refuse).
    MissingKey {
        /// The missing member.
        member: String,
    },
    /// Pair identity is malformed — duplicate `pair_id`, a
    /// `(task, repeat)` collision, an out-of-range repeat index or a
    /// stratum that does not match its endpoint.
    PairIdentity {
        /// The detail.
        detail: String,
    },
    /// The planned design is unequal — `pairs ≠ T·R`, a task carrying
    /// other than `R` repeats, or a declared non-equal weighting.
    UnequalDesign {
        /// The detail.
        detail: String,
    },
    /// One subject identity appears on more than one pair (reused
    /// subjects break the across-pair independence assumption).
    ReusedSubject {
        /// The reused subject.
        subject: String,
    },
    /// A dependence model other than across-pair independence was
    /// declared (shared shocks/carryover are unsupported).
    UnsupportedDependence {
        /// The declared dependence.
        declared: String,
    },
    /// A count/value outside the admitted shape — a binary outcome
    /// other than `0`/`PPM`, a negative/overflowing count or `N < 1`.
    InvalidCounts {
        /// The detail.
        detail: String,
    },
    /// An observed pair outcome lies outside the declared physical
    /// range (the ranges are binding, never clipped).
    OutsideDeclaredRange {
        /// The detail.
        detail: String,
    },
    /// A `bounded`/`relative` endpoint lacks declared physical
    /// ranges (unknown bounds ⇒ the required inference is
    /// unavailable — never an invented bound).
    UnknownRange {
        /// The endpoint.
        endpoint: String,
    },
    /// A declared range is malformed or unbounded (`a > b`,
    /// unbounded billing/intensity or a censored measure).
    UnboundedMeasure {
        /// The detail.
        detail: String,
    },
    /// The reference range contains zero and no frozen
    /// `"absolute"` convention was declared for the relative
    /// endpoint (or the `{endpoint}:absolute` margin is absent).
    ZeroReference {
        /// The detail.
        detail: String,
    },
    /// No registered margin binds the endpoint's dimension (margins
    /// come only from the pre-registration — never the caller).
    MissingMargin {
        /// The unbound dimension.
        dimension: String,
    },
    /// The registered margin's kind does not match the endpoint's
    /// estimand (absolute for `binary`/`bounded`, relative for
    /// `relative`).
    MarginKindMismatch {
        /// The dimension.
        dimension: String,
        /// The registered margin kind.
        margin_kind: String,
        /// The endpoint kind.
        endpoint_kind: String,
    },
    /// The pre-registration margin record refused decode (absent or
    /// malformed — the margin authority's own refusal, wrapped).
    MarginAuthority {
        /// The detail.
        detail: String,
    },
    /// A downstream use that would fuse this report into an
    /// omnibus/efficacy/credible certificate or apply the IUT to a
    /// selected/efficacy family (the D2 Holm is unchanged).
    UnsupportedVerdict {
        /// The declared use.
        declared: String,
    },
    /// A declared reporting level other than the frozen `.95`.
    UnsupportedLevel {
        /// The declared level (ppm).
        declared: i64,
    },
    /// A declared numeric/bound input outside its finite range.
    NonFiniteInput {
        /// The detail.
        detail: String,
    },
    /// A method name outside the admitted set was declared.
    UnknownMethod {
        /// The declared method.
        declared: String,
    },
    /// The method was declared on an analysis `kind` that does not
    /// compute it — a metadata-only declaration fails.
    MetadataOnly {
        /// The kind the declaration rode on.
        kind: String,
    },
    /// The frozen claim/family declaration is malformed — an empty
    /// conjunction ref/member set, a required endpoint absent from
    /// the declared conjunction (a required unknown can never be
    /// removed), or `selected_claims` below the actual selected
    /// count.
    FamilyIdentity {
        /// The detail.
        detail: String,
    },
}

impl std::fmt::Display for KlEquivalenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use KlEquivalenceError as E;
        match self {
            E::MissingKey { member } => write!(f, "missing declared key: {member}"),
            E::PairIdentity { detail } => write!(f, "pair identity malformed: {detail}"),
            E::UnequalDesign { detail } => write!(f, "unequal planned design: {detail}"),
            E::ReusedSubject { subject } => {
                write!(f, "reused subject across pairs: {subject}")
            }
            E::UnsupportedDependence { declared } => {
                write!(f, "unsupported dependence declared: {declared}")
            }
            E::InvalidCounts { detail } => write!(f, "invalid counts: {detail}"),
            E::OutsideDeclaredRange { detail } => {
                write!(f, "observed value outside declared range: {detail}")
            }
            E::UnknownRange { endpoint } => {
                write!(f, "unknown physical range: {endpoint}")
            }
            E::UnboundedMeasure { detail } => write!(f, "unbounded measure: {detail}"),
            E::ZeroReference { detail } => write!(f, "zero/near-zero reference: {detail}"),
            E::MissingMargin { dimension } => {
                write!(f, "no registered margin binds dimension {dimension}")
            }
            E::MarginKindMismatch {
                dimension,
                margin_kind,
                endpoint_kind,
            } => write!(
                f,
                "margin kind {margin_kind} does not match {endpoint_kind} endpoint {dimension}"
            ),
            E::MarginAuthority { detail } => {
                write!(f, "margin authority refused: {detail}")
            }
            E::UnsupportedVerdict { declared } => {
                write!(
                    f,
                    "unsupported verdict use: {declared} — no omnibus/efficacy fusion"
                )
            }
            E::UnsupportedLevel { declared } => {
                write!(
                    f,
                    "unsupported level {declared} — reporting frozen at 950_000 ppm"
                )
            }
            E::NonFiniteInput { detail } => write!(f, "nonfinite input: {detail}"),
            E::UnknownMethod { declared } => write!(f, "unknown method declared: {declared}"),
            E::MetadataOnly { kind } => {
                write!(f, "metadata-only method switch refused on kind {kind}")
            }
            E::FamilyIdentity { detail } => {
                write!(f, "frozen claim/family identity malformed: {detail}")
            }
        }
    }
}

impl std::error::Error for KlEquivalenceError {}

/// Integer floor/ceil division on `i128` (sign-correct — the
/// outward-rounding helpers for the affine transform-backs).
fn floor_div(a: i128, b: i128) -> i64 {
    debug_assert!(b > 0);
    let q = a / b;
    (if a % b != 0 && a < 0 { q - 1 } else { q }) as i64
}

fn ceil_div(a: i128, b: i128) -> i64 {
    debug_assert!(b > 0);
    let q = a / b;
    (if a % b != 0 && a > 0 { q + 1 } else { q }) as i64
}

/// `a/b` rounded to the nearest integer, ties away from zero (the
/// `stats::mean` rounding rule — one rounding rule, deterministic).
fn round_div(a: i128, b: i128) -> i64 {
    debug_assert!(b > 0);
    ((if a >= 0 { a + b / 2 } else { a - b / 2 }) / b) as i64
}

/// One computed directional bound as it lands on the report
/// (`kl_bound{eta_ppm, n, x_ppm, lo_ppm, hi_ppm}` — F.3).
#[derive(Debug, Clone, PartialEq)]
pub struct KlBound {
    /// The leg this bound serves (`w_plus`/`w_minus` for `binary`;
    /// `mean`/`c_minus`/`c_plus`/`absolute_fallback` otherwise).
    pub leg: String,
    /// The η (ppm) this inversion ran at.
    pub eta_ppm: i64,
    /// The preserved planned N.
    pub n: i64,
    /// The normalized `x` position the bound inverts (ppm —
    /// envelope-adjusted for uncertain pairs).
    pub x_ppm: i64,
    /// The outward-rounded endpoint.
    pub interval: Interval,
}

impl KlBound {
    fn to_json(&self) -> Json {
        Json::obj([
            ("leg", Json::str(&self.leg)),
            ("eta_ppm", Json::Int(self.eta_ppm)),
            ("n", Json::Int(self.n)),
            ("x_ppm", Json::Int(self.x_ppm)),
            ("lo_ppm", Json::Int(self.interval.lo)),
            ("hi_ppm", Json::Int(self.interval.hi)),
        ])
    }
}

/// The verdict vocabulary (`equivalent | not_equivalent |
/// inconclusive` — a third value, never a collapse).
pub const EQUIVALENT: &str = "equivalent";
/// `not_equivalent` — inferential only under S11 allocation.
pub const NOT_EQUIVALENT: &str = "not_equivalent";
/// `inconclusive` — touching margins and descriptive negatives.
pub const INCONCLUSIVE: &str = "inconclusive";

/// One endpoint's computed outcome.
#[derive(Debug, Clone, PartialEq)]
pub struct EndpointOutcome {
    /// The endpoint id.
    pub endpoint: String,
    /// Its stratum.
    pub stratum: String,
    /// `required` flag (frozen conjunction member candidate).
    pub required: bool,
    /// The estimand kind spelling.
    pub kind: String,
    /// `true` when this endpoint is a declared conjunction member
    /// (unallocated levels); `false` = selected-claim allocation.
    pub conjunction_member: bool,
    /// The registered margin member that bound (echoed).
    pub margin: Json,
    /// Pair accounting `(planned_n, resolved, w_plus, w_minus,
    /// concordant, uncertain)` — `w_*`/`concordant` are the binary
    /// legs (0 for continuous endpoints).
    pub pairs: (i64, i64, i64, i64, i64, i64),
    /// The directional/normalized bounds (TOST level).
    pub kl_bounds: Vec<KlBound>,
    /// The reporting-level bounds (S11).
    pub reporting_bounds: Vec<KlBound>,
    /// `point` on the endpoint's difference scale (ppm).
    pub point_ppm: i64,
    /// The TOST interval on the difference scale.
    pub tost: Interval,
    /// The 95% reporting interval on the difference scale.
    pub reporting: Interval,
    /// The verdict.
    pub verdict: String,
    /// The η per endpoint/branch the reporting interval ran at (ppm —
    /// already ÷m for selected claims).
    pub reporting_eta_ppm: i64,
    /// The frozen negative allocation actually used (ppm per
    /// endpoint/branch — already ÷m for selected claims).
    pub allocated: Json,
    /// A descriptive-only note (e.g. `.90` disjoint without the .95
    /// interval wholly outside — never an inferential claim).
    pub note: Option<String>,
}

impl EndpointOutcome {
    fn to_json(&self) -> Json {
        Json::obj([
            ("endpoint", Json::str(&self.endpoint)),
            ("stratum", Json::str(&self.stratum)),
            ("required", Json::Bool(self.required)),
            ("kind", Json::str(&self.kind)),
            ("conjunction_member", Json::Bool(self.conjunction_member)),
            ("margin", self.margin.clone()),
            (
                "pairs",
                Json::obj([
                    ("planned_n", Json::Int(self.pairs.0)),
                    ("resolved", Json::Int(self.pairs.1)),
                    ("w_plus", Json::Int(self.pairs.2)),
                    ("w_minus", Json::Int(self.pairs.3)),
                    ("concordant", Json::Int(self.pairs.4)),
                    ("uncertain", Json::Int(self.pairs.5)),
                ]),
            ),
            (
                "completion_envelope",
                Json::obj([
                    ("c", Json::Int(self.pairs.2 + self.pairs.3 + self.pairs.4)),
                    ("u", Json::Int(self.pairs.5)),
                    ("n", Json::Int(self.pairs.0)),
                ]),
            ),
            (
                "kl_bound",
                Json::Arr(self.kl_bounds.iter().map(KlBound::to_json).collect()),
            ),
            (
                "equivalence_verdict",
                Json::obj([
                    ("point_delta_ppm", Json::Int(self.point_ppm)),
                    ("lo_ppm", Json::Int(self.tost.lo)),
                    ("hi_ppm", Json::Int(self.tost.hi)),
                    ("verdict", Json::str(&self.verdict)),
                    (
                        "reporting",
                        Json::obj([
                            ("lo_ppm", Json::Int(self.reporting.lo)),
                            ("hi_ppm", Json::Int(self.reporting.hi)),
                            ("eta_ppm", Json::Int(self.reporting_eta_ppm)),
                        ]),
                    ),
                ]),
            ),
            (
                "reporting_bounds",
                Json::Arr(self.reporting_bounds.iter().map(KlBound::to_json).collect()),
            ),
            ("allocated", self.allocated.clone()),
            (
                "note",
                self.note.as_ref().map(Json::str).unwrap_or(Json::Null),
            ),
        ])
    }
}

/// The estimator's result — every member a fact on the report.
#[derive(Debug, Clone, PartialEq)]
pub struct KlEquivalence {
    /// The computed endpoints.
    pub endpoints: Vec<EndpointOutcome>,
    /// The IUT conjunction outcome `(claim_ref, verdict, members,
    /// unknown_members)` when declared.
    pub conjunction: Option<(String, String, Vec<(String, String)>, Vec<String>)>,
    /// Population/class counts (the declared pair universe —
    /// canonical/raw/joint read the same declared records here;
    /// authentic-population separation is R5.39's).
    pub populations: Json,
    /// The S.1 method-metadata record.
    pub metadata: MethodMetadata,
}

impl KlEquivalence {
    /// The canonical JSON member — the report body payload.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(VERSION)),
            (
                "endpoints",
                Json::Arr(
                    self.endpoints
                        .iter()
                        .map(EndpointOutcome::to_json)
                        .collect(),
                ),
            ),
            (
                "iut_conjunction",
                match &self.conjunction {
                    None => Json::obj([("status", Json::str("not_declared"))]),
                    Some((claim_ref, verdict, members, unknown)) => Json::obj([
                        ("claim_ref", Json::str(claim_ref)),
                        ("verdict", Json::str(verdict)),
                        (
                            "members",
                            Json::Arr(
                                members
                                    .iter()
                                    .map(|(e, v)| {
                                        Json::obj([
                                            ("endpoint", Json::str(e)),
                                            ("verdict", Json::str(v)),
                                        ])
                                    })
                                    .collect(),
                            ),
                        ),
                        (
                            "unknown_members",
                            Json::Arr(unknown.iter().map(Json::str).collect()),
                        ),
                    ]),
                },
            ),
            ("populations", self.populations.clone()),
            ("metadata", self.metadata.to_json()),
        ])
    }
}

/// The η (ppm per endpoint) a claim's bound level uses — conjunction
/// members run at the frozen base (IUT); selected claims allocate
/// `base/m` (Bonferroni over the declared selected family).
fn allocated_eta(base_ppm: i64, selected: bool, m: u32) -> i64 {
    if selected {
        base_ppm / m.max(1) as i64
    } else {
        base_ppm
    }
}

/// Pair accounting — validates identity/floors/keys and returns
/// `(resolved, w_plus, w_minus, concordant, uncertain)` plus the
/// resolved value pairs for the continuous constructions.
#[allow(clippy::type_complexity)]
fn account(
    ep: &EndpointDecl,
) -> Result<(i64, i64, i64, i64, i64, Vec<(i64, i64)>), KlEquivalenceError> {
    use KlEquivalenceError as E;
    if ep.endpoint.is_empty() {
        return Err(E::MissingKey {
            member: "endpoint".into(),
        });
    }
    if ep.stratum.is_empty() {
        return Err(E::MissingKey {
            member: "stratum".into(),
        });
    }
    let n = ep.tasks as i64 * ep.repeats as i64;
    if ep.tasks == 0 || ep.repeats == 0 || ep.pairs.len() as i64 != n {
        return Err(E::UnequalDesign {
            detail: format!(
                "declared T={} R={} ⇒ N={} but {} pairs declared",
                ep.tasks,
                ep.repeats,
                n,
                ep.pairs.len()
            ),
        });
    }
    let mut pair_ids = BTreeSet::new();
    let mut subjects = BTreeSet::new();
    let mut slots = BTreeSet::new();
    let mut per_task: BTreeMap<&str, u32> = BTreeMap::new();
    let mut resolved = Vec::with_capacity(ep.pairs.len());
    let (mut wp, mut wm, mut concordant, mut uncertain) = (0i64, 0i64, 0i64, 0i64);
    for p in &ep.pairs {
        for (member, v) in [
            ("task_id", p.task_id.as_str()),
            ("pair_id", p.pair_id.as_str()),
            ("subject", p.subject.as_str()),
            ("phase", p.phase.as_str()),
            ("stratum", p.stratum.as_str()),
        ] {
            if v.is_empty() {
                return Err(E::MissingKey {
                    member: member.into(),
                });
            }
        }
        if p.stratum != ep.stratum {
            return Err(E::PairIdentity {
                detail: format!(
                    "pair {} stratum {} != endpoint stratum {}",
                    p.pair_id, p.stratum, ep.stratum
                ),
            });
        }
        if p.repeat >= ep.repeats {
            return Err(E::PairIdentity {
                detail: format!(
                    "pair {} repeat {} outside 0..{}",
                    p.pair_id, p.repeat, ep.repeats
                ),
            });
        }
        if !pair_ids.insert(p.pair_id.as_str()) {
            return Err(E::PairIdentity {
                detail: format!("duplicate pair_id {}", p.pair_id),
            });
        }
        if !slots.insert((p.task_id.as_str(), p.repeat)) {
            return Err(E::PairIdentity {
                detail: format!("(task,repeat) collision {}:{}", p.task_id, p.repeat),
            });
        }
        if !subjects.insert(p.subject.as_str()) {
            return Err(E::ReusedSubject {
                subject: p.subject.clone(),
            });
        }
        *per_task.entry(p.task_id.as_str()).or_insert(0) += 1;
        for (member, v) in [("h", p.h), ("o", p.o)] {
            if let Some(v) = v {
                match ep.kind {
                    EndpointKind::Binary => {
                        if v != 0 && v != PPM {
                            return Err(E::InvalidCounts {
                                detail: format!("{member}={v} — binary outcomes are 0/{PPM}"),
                            });
                        }
                    }
                    EndpointKind::Bounded | EndpointKind::Relative => {
                        let (a, b) = if member == "h" {
                            ep.range_h.unwrap()
                        } else {
                            ep.range_o.unwrap()
                        };
                        if v < a || v > b {
                            return Err(E::OutsideDeclaredRange {
                                detail: format!("{member}={v} outside [{a},{b}]"),
                            });
                        }
                    }
                }
            }
        }
        match (p.h, p.o) {
            (Some(h), Some(o)) => {
                resolved.push((h, o));
                if ep.kind == EndpointKind::Binary {
                    match (h, o) {
                        (PPM, 0) => wp += 1,
                        (0, PPM) => wm += 1,
                        _ => concordant += 1,
                    }
                }
            }
            _ => uncertain += 1,
        }
    }
    for (t, c) in &per_task {
        if *c != ep.repeats {
            return Err(E::UnequalDesign {
                detail: format!("task {t} carries {c} repeats, planned {}", ep.repeats),
            });
        }
    }
    if per_task.len() != ep.tasks as usize {
        return Err(E::UnequalDesign {
            detail: format!("{} distinct tasks, planned {}", per_task.len(), ep.tasks),
        });
    }
    // `resolved.len()` already covers every fully observed pair —
    // `wp`/`wm`/`concordant` decompose it for the binary legs.
    Ok((
        resolved.len() as i64,
        wp,
        wm,
        concordant,
        uncertain,
        resolved,
    ))
}

/// The bound-and-verdict kernel over one normalized `[0,1]`-scale
/// mean position with an uncertain-pair envelope. `x_lo`/`x_hi` are
/// the conservative normalized extremes (ppm). Returns
/// `((tost_lo,tost_hi) normalized-ppm, (rep_lo,rep_hi) normalized-ppm,
/// bounds, reporting_bounds)`.
fn envelope_bounds(
    leg: &str,
    x_lo: i64,
    x_hi: i64,
    n: i64,
    eta_tost: i64,
    eta_rep: i64,
) -> Result<(Interval, Interval, Vec<KlBound>, Vec<KlBound>), KlEquivalenceError> {
    let k = |x, eta, leg: &str| -> Result<KlBound, KlEquivalenceError> {
        kl_directional_bound(x, n, eta)
            .map(|interval| KlBound {
                leg: leg.to_string(),
                eta_ppm: eta,
                n,
                x_ppm: x,
                interval,
            })
            .ok_or_else(|| KlEquivalenceError::NonFiniteInput {
                detail: format!("kl inversion refused at x={x} n={n} eta={eta}"),
            })
    };
    // The lower endpoint inverts the most adverse (smallest)
    // normalized position; the upper inverts the largest — the
    // spec's `[K(c/N).L, K((c+u)/N).U]` envelope at preserved N.
    let lo_t = k(x_lo, eta_tost, leg)?;
    let hi_t = k(x_hi, eta_tost, leg)?;
    let lo_r = k(x_lo, eta_rep, leg)?;
    let hi_r = k(x_hi, eta_rep, leg)?;
    let tost = Interval {
        lo: lo_t.interval.lo,
        hi: hi_t.interval.hi,
    };
    let rep = Interval {
        lo: lo_r.interval.lo,
        hi: hi_r.interval.hi,
    };
    Ok((
        tost,
        rep,
        vec![
            KlBound {
                leg: format!("{leg}.lower"),
                ..lo_t
            },
            KlBound {
                leg: format!("{leg}.upper"),
                ..hi_t
            },
        ],
        vec![
            KlBound {
                leg: format!("{leg}.lower"),
                ..lo_r
            },
            KlBound {
                leg: format!("{leg}.upper"),
                ..hi_r
            },
        ],
    ))
}

/// Binary endpoint — the c+/c− directional-disagreement construction
/// (`L_Δ = L+−U−`, `U_Δ = U+−L−` on the preserved-N envelopes).
fn binary_endpoint(
    ep: &EndpointDecl,
    epsilon_ppm: i64,
    cj: bool,
    m: u32,
) -> Result<EndpointOutcome, KlEquivalenceError> {
    let (resolved, wp, wm, concordant, u, _) = account(ep)?;
    let n = resolved + u;
    let eta_t = allocated_eta(ETA_TOST_PPM, !cj, m);
    let eta_r = allocated_eta(ETA_REPORT_PPM, !cj, m);
    // The directional envelopes: L inverts `x = c/N` (floored — the
    // smallest normalized position), U inverts `x = (c+u)/N`
    // (ceiled — the largest); never missing-at-random.
    let bound = |c: i64, eta: i64, leg: &str| -> Result<(i64, i64), KlEquivalenceError> {
        let x_lo = floor_div(i128::from(c) * i128::from(PPM), i128::from(n));
        let x_hi = ceil_div(i128::from(c + u) * i128::from(PPM), i128::from(n));
        let _ = leg;
        let l = kl_directional_bound(x_lo, n, eta)
            .ok_or_else(|| KlEquivalenceError::NonFiniteInput {
                detail: format!("kl inversion refused at c={c} n={n} eta={eta}"),
            })?
            .lo;
        let u_ = kl_directional_bound(x_hi, n, eta)
            .ok_or_else(|| KlEquivalenceError::NonFiniteInput {
                detail: format!("kl inversion refused at c={c} n={n} eta={eta}"),
            })?
            .hi;
        Ok((l, u_))
    };
    let (lp, up) = bound(wp, eta_t, "w_plus")?;
    let (lm, um) = bound(wm, eta_t, "w_minus")?;
    let (lp_r, up_r) = bound(wp, eta_r, "w_plus")?;
    let (lm_r, um_r) = bound(wm, eta_r, "w_minus")?;
    let tost = Interval {
        lo: lp - um,
        hi: up - lm,
    };
    let rep = Interval {
        lo: lp_r - um_r,
        hi: up_r - lm_r,
    };
    let point = round_div(i128::from(wp - wm) * i128::from(PPM), i128::from(n));
    let (verdict, note) = verdict(tost, rep, -epsilon_ppm, epsilon_ppm);
    // Report the same normalized positions the bounds inverted —
    // `x = c/N` floored for the lower leg, `(c+u)/N` ceiled for the
    // upper (the exact envelope positions).
    let x_of = |c: i64, upper: bool| -> i64 {
        if upper {
            ceil_div(i128::from(c + u) * i128::from(PPM), i128::from(n))
        } else {
            floor_div(i128::from(c) * i128::from(PPM), i128::from(n))
        }
    };
    let bounds_at = |eta: i64| -> Vec<KlBound> {
        [
            ("w_plus.lower", x_of(wp, false)),
            ("w_plus.upper", x_of(wp, true)),
            ("w_minus.lower", x_of(wm, false)),
            ("w_minus.upper", x_of(wm, true)),
        ]
        .iter()
        .map(|(leg, x)| KlBound {
            leg: leg.to_string(),
            eta_ppm: eta,
            n,
            x_ppm: *x,
            interval: kl_directional_bound(*x, n, eta).unwrap_or(Interval { lo: 0, hi: 0 }),
        })
        .collect()
    };
    let kl_bounds = bounds_at(eta_t);
    let reporting_bounds = bounds_at(eta_r);
    Ok(EndpointOutcome {
        endpoint: ep.endpoint.clone(),
        stratum: ep.stratum.clone(),
        required: ep.required,
        kind: "binary".into(),
        conjunction_member: cj,
        margin: Json::obj([
            ("kind", Json::str("absolute")),
            ("epsilon_ppm", Json::Int(epsilon_ppm)),
        ]),
        pairs: (n, resolved, wp, wm, concordant, u),
        kl_bounds,
        reporting_bounds,
        point_ppm: point,
        tost,
        reporting: rep,
        verdict: verdict.to_string(),
        reporting_eta_ppm: eta_r,
        note,
        allocated: Json::obj([
            (
                "claim",
                Json::str(if cj { "conjunction_member" } else { "selected" }),
            ),
            ("selected_claims", Json::Int(m as i64)),
            ("eta_tost_ppm", Json::Int(eta_t)),
            ("eta_report_ppm", Json::Int(eta_r)),
        ]),
    })
}

/// The shared verdict rule: **strict** TOST inside the band ⇒
/// `equivalent`; the 95% reporting interval wholly outside ⇒
/// `not_equivalent` (S11); otherwise `inconclusive` — a `.90`
/// disjoint interval alone is recorded descriptive.
fn verdict(
    tost: Interval,
    rep: Interval,
    band_lo: i64,
    band_hi: i64,
) -> (&'static str, Option<String>) {
    if tost.lo > band_lo && tost.hi < band_hi {
        return (EQUIVALENT, None);
    }
    if rep.hi < band_lo || rep.lo > band_hi {
        return (NOT_EQUIVALENT, None);
    }
    let note = (tost.hi < band_lo || tost.lo > band_hi).then(|| {
        ".90 TOST interval disjoint from the band — descriptive only; the .95 reporting interval is not wholly outside (S11 negative stays inconclusive)".to_string()
    });
    (INCONCLUSIVE, note)
}

/// The normalized-mean construction shared by `bounded` and each
/// relative contrast leg: per-pair normalized values `r_i ∈ [0,1]`
/// (ppm) with the uncertain-pair envelope at planned `N`, K applied
/// to the mean position, then transformed back onto `[lo_scale,
/// hi_scale]` units. `per_pair` = resolved `(lo_ppm, hi_ppm)`
/// normalized values (outward-rounded per pair). Returns `(tost,
/// reporting, bounds, reporting_bounds)` on the output scale.
fn normalized_mean(
    leg: &str,
    per_pair: &[(i64, i64)],
    uncertain: i64,
    n: i64,
    lo_scale: i64,
    width: i64,
    eta_tost: i64,
    eta_rep: i64,
) -> Result<(Interval, Interval, Vec<KlBound>, Vec<KlBound>), KlEquivalenceError> {
    use KlEquivalenceError as E;
    if width <= 0 {
        return Err(E::InvalidCounts {
            detail: format!("degenerate normalization width {width} on leg {leg}"),
        });
    }
    let sum_lo: i128 = per_pair.iter().map(|(l, _)| *l as i128).sum();
    let sum_hi: i128 = per_pair.iter().map(|(_, h)| *h as i128).sum();
    // Envelope: uncertain members sit at the adverse end of each
    // direction (0 for the lower, 1 for the upper) — planned N kept.
    let x_lo = floor_div(sum_lo, i128::from(n)).clamp(0, PPM);
    let x_hi = ceil_div(
        sum_hi + i128::from(uncertain) * i128::from(PPM),
        i128::from(n),
    )
    .clamp(0, PPM);
    let (tost_n, rep_n, bounds, rep_bounds) =
        envelope_bounds(leg, x_lo, x_hi, n, eta_tost, eta_rep)?;
    // Transform back: D = lo_scale + x·width — outward (lower floored,
    // upper ceiled).
    let back = |i: Interval| Interval {
        lo: lo_scale + floor_div(i128::from(i.lo) * i128::from(width), i128::from(PPM)),
        hi: lo_scale + ceil_div(i128::from(i.hi) * i128::from(width), i128::from(PPM)),
    };
    Ok((back(tost_n), back(rep_n), bounds, rep_bounds))
}

/// Bounded absolute endpoint — `D = X_H − X_O ∈ [a,b]` with
/// `a = a_h − b_o`, `b = b_h − a_o` from the declared physical
/// ranges; `a = b` is exact. The normalized mean construction
/// transforms back to the declared unit — an absolute margin ε is
/// on that same ppm scale.
fn bounded_endpoint(
    ep: &EndpointDecl,
    epsilon_ppm: i64,
    cj: bool,
    m: u32,
) -> Result<EndpointOutcome, KlEquivalenceError> {
    use KlEquivalenceError as E;
    let (a_h, b_h) = ep.range_h.ok_or_else(|| E::UnknownRange {
        endpoint: ep.endpoint.clone(),
    })?;
    let (a_o, b_o) = ep.range_o.ok_or_else(|| E::UnknownRange {
        endpoint: ep.endpoint.clone(),
    })?;
    if a_h > b_h || a_o > b_o {
        return Err(E::UnboundedMeasure {
            detail: format!(
                "malformed range h=[{a_h},{b_h}] o=[{a_o},{b_o}] on {}",
                ep.endpoint
            ),
        });
    }
    let (resolved, _, _, _, u, pairs) = account(ep)?;
    let n = resolved + u;
    let a = a_h - b_o;
    let b = b_h - a_o;
    let eta_t = allocated_eta(ETA_TOST_PPM, !cj, m);
    let eta_r = allocated_eta(ETA_REPORT_PPM, !cj, m);
    // `a = b` is exact — a point interval, never an inversion.
    if a == b {
        let point = a;
        let (verdict, note) = verdict(
            Interval {
                lo: point,
                hi: point,
            },
            Interval {
                lo: point,
                hi: point,
            },
            -epsilon_ppm,
            epsilon_ppm,
        );
        return Ok(EndpointOutcome {
            endpoint: ep.endpoint.clone(),
            stratum: ep.stratum.clone(),
            required: ep.required,
            kind: "bounded".into(),
            conjunction_member: cj,
            margin: Json::obj([
                ("kind", Json::str("absolute")),
                ("epsilon_ppm", Json::Int(epsilon_ppm)),
                ("range", Json::Arr(vec![Json::Int(a), Json::Int(b)])),
            ]),
            pairs: (n, resolved, 0, 0, 0, u),
            kl_bounds: vec![],
            reporting_bounds: vec![],
            point_ppm: point,
            tost: Interval {
                lo: point,
                hi: point,
            },
            reporting: Interval {
                lo: point,
                hi: point,
            },
            verdict: verdict.to_string(),
            reporting_eta_ppm: eta_r,
            allocated: Json::obj([
                (
                    "claim",
                    Json::str(if cj { "conjunction_member" } else { "selected" }),
                ),
                ("selected_claims", Json::Int(m as i64)),
                ("eta_tost_ppm", Json::Int(eta_t)),
                ("eta_report_ppm", Json::Int(eta_r)),
                ("leg", Json::str("exact")),
            ]),
            note,
        });
    }
    // `r_i = (D_i − a)/(b − a)` — per-pair outward (floor for the
    // lower leg, ceil for the upper).
    let w = b - a;
    let per_pair: Vec<(i64, i64)> = pairs
        .iter()
        .map(|(h, o)| {
            let num = i128::from(*h - *o - a) * i128::from(PPM);
            (floor_div(num, i128::from(w)), ceil_div(num, i128::from(w)))
        })
        .collect();
    let (tost, rep, kl_bounds, reporting_bounds) =
        normalized_mean("mean", &per_pair, u, n, a, w, eta_t, eta_r)?;
    let point = {
        let num: i128 = pairs.iter().map(|(h, o)| i128::from(*h - *o)).sum();
        round_div(num, i128::from(pairs.len().max(1) as i64))
    };
    let (verdict, note) = verdict(tost, rep, -epsilon_ppm, epsilon_ppm);
    Ok(EndpointOutcome {
        endpoint: ep.endpoint.clone(),
        stratum: ep.stratum.clone(),
        required: ep.required,
        kind: "bounded".into(),
        conjunction_member: cj,
        margin: Json::obj([
            ("kind", Json::str("absolute")),
            ("epsilon_ppm", Json::Int(epsilon_ppm)),
            ("range", Json::Arr(vec![Json::Int(a), Json::Int(b)])),
        ]),
        pairs: (n, resolved, 0, 0, 0, u),
        kl_bounds,
        reporting_bounds,
        point_ppm: point,
        tost,
        reporting: rep,
        verdict: verdict.to_string(),
        reporting_eta_ppm: eta_r,
        allocated: Json::obj([
            (
                "claim",
                Json::str(if cj { "conjunction_member" } else { "selected" }),
            ),
            ("selected_claims", Json::Int(m as i64)),
            ("eta_tost_ppm", Json::Int(eta_t)),
            ("eta_report_ppm", Json::Int(eta_r)),
        ]),
        note,
    })
}

/// One affine contrast leg `Z = X_H − κ·X_O` (`κ = 1−ρ` for `C−`,
/// `1+ρ` for `C+`) — declared range
/// `[a_h − κ·b_o, b_h − κ·a_o]` (outward), per-pair values
/// normalized into `[0,1]`, K applied, transformed back. Returns
/// `(tost_on_Z, reporting_on_Z, bounds, reporting_bounds, point)`.
#[allow(clippy::too_many_arguments)]
fn contrast_leg(
    leg: &str,
    pairs: &[(i64, i64)],
    u: i64,
    n: i64,
    kappa_num: i64, // (PPM ∓ ρ)
    a_h: i64,
    b_h: i64,
    a_o: i64,
    b_o: i64,
    eta_lo: i64,
    eta_hi: i64,
) -> Result<(Interval, Interval, Vec<KlBound>, Vec<KlBound>, i64), KlEquivalenceError> {
    // Contrast range — outward-rounded bounds on `z = h·PPM − κ·o`
    // (`z` in ppm² numerator units for exact integer arithmetic).
    let cm_lo = floor_div(
        i128::from(a_h) * i128::from(PPM) - i128::from(kappa_num) * i128::from(b_o),
        i128::from(PPM),
    );
    let cm_hi = ceil_div(
        i128::from(b_h) * i128::from(PPM) - i128::from(kappa_num) * i128::from(a_o),
        i128::from(PPM),
    );
    let w = cm_hi - cm_lo;
    let z_of = |h: i64, o: i64| -> (i64, i64) {
        let num = i128::from(h) * i128::from(PPM) - i128::from(kappa_num) * i128::from(o);
        (
            floor_div(num, i128::from(PPM)),
            ceil_div(num, i128::from(PPM)),
        )
    };
    let point = {
        let s: i128 = pairs
            .iter()
            .map(|(h, o)| {
                i128::from(round_div(
                    i128::from(*h) * i128::from(PPM) - i128::from(kappa_num) * i128::from(*o),
                    i128::from(PPM),
                ))
            })
            .sum();
        round_div(s, i128::from(pairs.len().max(1) as i64))
    };
    if w <= 0 {
        // Degenerate contrast range — a frozen exact point (only
        // reachable when the declared ranges collapse).
        let i = Interval {
            lo: cm_lo,
            hi: cm_hi,
        };
        return Ok((i, i, vec![], vec![], point));
    }
    let per_pair: Vec<(i64, i64)> = pairs
        .iter()
        .map(|(h, o)| {
            let (zl, zh) = z_of(*h, *o);
            let num_lo = i128::from(zl - cm_lo) * i128::from(PPM);
            let num_hi = i128::from(zh - cm_lo) * i128::from(PPM);
            (
                floor_div(num_lo, i128::from(w)).clamp(0, PPM),
                ceil_div(num_hi, i128::from(w)).clamp(0, PPM),
            )
        })
        .collect();
    let (tost, rep, bounds, rep_bounds) =
        normalized_mean(leg, &per_pair, u, n, cm_lo, w, eta_lo, eta_hi)?;
    Ok((tost, rep, bounds, rep_bounds, point))
}

/// Relative endpoint — the ratio-of-finite-manifest-means test via
/// `C− = E_H − (1−ρ)E_O` and `C+ = E_H − (1+ρ)E_O`. Equivalence
/// needs `L(C−) > 0 ∧ U(C+) < 0` at η per bound; the S11 negative
/// needs `U(C−) < 0` or `L(C+) > 0` at `.025/m` per branch. A
/// reference range containing zero requires the frozen `"absolute"`
/// convention — a separately registered `{endpoint}:absolute`
/// margin — or refuses (`ZeroReference`).
fn relative_endpoint(
    ep: &EndpointDecl,
    rho_ppm: i64,
    margins: &[RegisteredMargin],
    cj: bool,
    m: u32,
) -> Result<EndpointOutcome, KlEquivalenceError> {
    use KlEquivalenceError as E;
    let (a_h, b_h) = ep.range_h.ok_or_else(|| E::UnknownRange {
        endpoint: ep.endpoint.clone(),
    })?;
    let (a_o, b_o) = ep.range_o.ok_or_else(|| E::UnknownRange {
        endpoint: ep.endpoint.clone(),
    })?;
    if a_h > b_h || a_o > b_o {
        return Err(E::UnboundedMeasure {
            detail: format!(
                "malformed range h=[{a_h},{b_h}] o=[{a_o},{b_o}] on {}",
                ep.endpoint
            ),
        });
    }
    if !(0..=PPM).contains(&rho_ppm) {
        return Err(E::InvalidCounts {
            detail: format!("rho {rho_ppm} outside [0,{PPM}]"),
        });
    }
    let (resolved, _, _, _, u, pairs) = account(ep)?;
    let n = resolved + u;
    let eta_t = allocated_eta(ETA_TOST_PPM, !cj, m);
    let eta_neg = allocated_eta(NEG_BRANCH_PPM, !cj, m);

    // Zero/near-zero reference: the declared O-range containing zero
    // makes the ratio degenerate — the frozen `"absolute"`
    // convention falls back to the bounded-absolute test under a
    // separately registered `{endpoint}:absolute` margin; anything
    // else refuses.
    if a_o <= 0 && b_o >= 0 {
        if ep.zero_reference.as_deref() != Some("absolute") {
            return Err(E::ZeroReference {
                detail: format!(
                    "reference range [{a_o},{b_o}] contains zero on {} — declare the frozen absolute convention or refuse",
                    ep.endpoint
                ),
            });
        }
        let dim = format!("{}:absolute", ep.endpoint);
        let am = margins
            .iter()
            .find(|r| r.dimension == dim && r.kind == "absolute")
            .ok_or_else(|| E::MissingMargin {
                dimension: dim.clone(),
            })?;
        let mut out = bounded_absolute_leg(
            ep,
            a_h,
            b_h,
            a_o,
            b_o,
            am.margin_ppm,
            resolved,
            u,
            n,
            &pairs,
            cj,
            m,
            eta_t,
            eta_r(rho_ppm, cj, m),
        )?;
        out.note = Some(
            "zero/near-zero reference — frozen absolute fallback (ratio unavailable, no observed-reference division)".into(),
        );
        return Ok(out);
    }

    // C− at κ = (1−ρ); C+ at κ = (1+ρ).
    let (cminus_t, cminus_r, mut bounds, rep_bounds, cminus_pt) = contrast_leg(
        "c_minus",
        &pairs,
        u,
        n,
        PPM - rho_ppm,
        a_h,
        b_h,
        a_o,
        b_o,
        eta_t,
        eta_neg,
    )?;
    let (cplus_t, cplus_r, mut bounds2, rep_bounds2, _cplus_pt) = contrast_leg(
        "c_plus",
        &pairs,
        u,
        n,
        PPM + rho_ppm,
        a_h,
        b_h,
        a_o,
        b_o,
        eta_t,
        eta_neg,
    )?;
    bounds.append(&mut bounds2);
    let mut rep_all = rep_bounds;
    rep_all.extend(rep_bounds2);
    // Equivalence: strictly inside `E_H − (1−ρ)E_O > 0` and
    // `E_H − (1+ρ)E_O < 0`.
    let (verdict, note) = if cminus_t.lo > 0 && cplus_t.hi < 0 {
        (EQUIVALENT, None)
    } else if cminus_r.hi < 0 || cplus_r.lo > 0 {
        (NOT_EQUIVALENT, None)
    } else {
        (INCONCLUSIVE, None)
    };
    Ok(EndpointOutcome {
        endpoint: ep.endpoint.clone(),
        stratum: ep.stratum.clone(),
        required: ep.required,
        kind: "relative".into(),
        conjunction_member: cj,
        margin: Json::obj([
            ("kind", Json::str("relative")),
            ("rho_ppm", Json::Int(rho_ppm)),
            ("range_h", Json::Arr(vec![Json::Int(a_h), Json::Int(b_h)])),
            ("range_o", Json::Arr(vec![Json::Int(a_o), Json::Int(b_o)])),
        ]),
        pairs: (n, resolved, 0, 0, 0, u),
        kl_bounds: bounds,
        reporting_bounds: rep_all,
        point_ppm: cminus_pt,
        // The endpoint interval members report the two contrast
        // intervals — the verdict's own surfaces.
        tost: Interval {
            lo: cminus_t.lo,
            hi: cplus_t.hi,
        },
        reporting: Interval {
            lo: cminus_r.lo,
            hi: cplus_r.hi,
        },
        verdict: verdict.to_string(),
        reporting_eta_ppm: eta_neg,
        allocated: Json::obj([
            (
                "claim",
                Json::str(if cj { "conjunction_member" } else { "selected" }),
            ),
            ("selected_claims", Json::Int(m as i64)),
            ("eta_tost_ppm", Json::Int(eta_t)),
            ("eta_neg_branch_ppm", Json::Int(eta_neg)),
            (
                "negative_rule",
                Json::str("U(C−)<0 or L(C+)>0 at the per-branch allocation"),
            ),
        ]),
        note,
    })
}

fn eta_r(_rho: i64, cj: bool, m: u32) -> i64 {
    allocated_eta(ETA_REPORT_PPM, !cj, m)
}

/// The zero-reference frozen fallback: the bounded-absolute test on
/// `D = X_H − X_O` over `[a_h−b_o, b_h−a_o]` under the separately
/// registered absolute margin (leg `absolute_fallback`).
#[allow(clippy::too_many_arguments)]
fn bounded_absolute_leg(
    ep: &EndpointDecl,
    a_h: i64,
    b_h: i64,
    a_o: i64,
    b_o: i64,
    epsilon_ppm: i64,
    resolved: i64,
    u: i64,
    n: i64,
    pairs: &[(i64, i64)],
    cj: bool,
    m: u32,
    eta_t: i64,
    eta_rep: i64,
) -> Result<EndpointOutcome, KlEquivalenceError> {
    let a = a_h - b_o;
    let b = b_h - a_o;
    let w = b - a;
    if w <= 0 {
        return Err(KlEquivalenceError::UnboundedMeasure {
            detail: format!("degenerate fallback range [{a},{b}] on {}", ep.endpoint),
        });
    }
    let per_pair: Vec<(i64, i64)> = pairs
        .iter()
        .map(|(h, o)| {
            let num = i128::from(*h - *o - a) * i128::from(PPM);
            (
                floor_div(num, i128::from(w)).clamp(0, PPM),
                ceil_div(num, i128::from(w)).clamp(0, PPM),
            )
        })
        .collect();
    let (tost, rep, kl_bounds, reporting_bounds) =
        normalized_mean("absolute_fallback", &per_pair, u, n, a, w, eta_t, eta_rep)?;
    let (verdict, note) = verdict(tost, rep, -epsilon_ppm, epsilon_ppm);
    Ok(EndpointOutcome {
        endpoint: ep.endpoint.clone(),
        stratum: ep.stratum.clone(),
        required: ep.required,
        kind: "relative".into(),
        conjunction_member: cj,
        margin: Json::obj([
            ("kind", Json::str("relative")),
            (
                "absolute_fallback",
                Json::obj([
                    ("dimension", Json::str(&format!("{}:absolute", ep.endpoint))),
                    ("epsilon_ppm", Json::Int(epsilon_ppm)),
                    ("range", Json::Arr(vec![Json::Int(a), Json::Int(b)])),
                ]),
            ),
        ]),
        pairs: (n, resolved, 0, 0, 0, u),
        kl_bounds,
        reporting_bounds,
        point_ppm: round_div(
            pairs.iter().map(|(h, o)| i128::from(*h - *o)).sum::<i128>(),
            i128::from(pairs.len().max(1) as i64),
        ),
        tost,
        reporting: rep,
        verdict: verdict.to_string(),
        reporting_eta_ppm: eta_rep,
        allocated: Json::obj([
            (
                "claim",
                Json::str(if cj { "conjunction_member" } else { "selected" }),
            ),
            ("selected_claims", Json::Int(m as i64)),
            ("eta_tost_ppm", Json::Int(eta_t)),
            ("eta_report_ppm", Json::Int(eta_rep)),
            ("leg", Json::str("absolute_fallback")),
        ]),
        note,
    })
}

/// `kl_equivalence(decl, margins)` — the admitted estimator over
/// declared first-attempt pair records. `margins` are the
/// pre-registration's registered margins (the only margin source —
/// the caller already decoded `PreRegistration.equivalence_margin`;
/// every endpoint binds one by `dimension`). Every admitted-input
/// violation is a typed [`KlEquivalenceError`].
pub fn kl_equivalence(
    decl: &KlEquivalenceDecl,
    margins: &[RegisteredMargin],
) -> Result<KlEquivalence, KlEquivalenceError> {
    use KlEquivalenceError as E;
    // ── declared-input validation (all before any bound) ──────────
    if !(0..PPM).contains(&decl.level_ppm) {
        return Err(E::NonFiniteInput {
            detail: format!("level_ppm {} outside (0, {PPM})", decl.level_ppm),
        });
    }
    if decl.level_ppm != 950_000 {
        return Err(E::UnsupportedLevel {
            declared: decl.level_ppm,
        });
    }
    if decl.endpoints.is_empty() {
        return Err(E::FamilyIdentity {
            detail: "empty endpoint set — nothing registered".into(),
        });
    }
    if let Some(d) = &decl.dependence {
        if d != REGISTERED_DEPENDENCE && d != "independent" {
            return Err(E::UnsupportedDependence {
                declared: d.clone(),
            });
        }
    }
    if let Some(w) = &decl.weights {
        if w != "equal" {
            return Err(E::UnequalDesign {
                detail: format!(
                    "declared weights {w} — only equal 1/N identifies the finite target"
                ),
            });
        }
    }
    if let Some(v) = &decl.use_as {
        if matches!(
            v.as_str(),
            "omnibus"
                | "combined"
                | "combined_verdict"
                | "practical_margin"
                | "weak_mean"
                | "efficacy"
                | "efficacy_holm"
                | "holm"
                | "d2_efficacy"
                | "credible"
                | "iut_efficacy"
                | "any_endpoint"
        ) {
            return Err(E::UnsupportedVerdict {
                declared: v.clone(),
            });
        }
    }
    if let Some(c) = &decl.conjunction {
        if c.claim_ref.is_empty() || c.members.is_empty() {
            return Err(E::FamilyIdentity {
                detail: "conjunction claim_ref/member set missing".into(),
            });
        }
        let mut seen = BTreeSet::new();
        for mem in &c.members {
            if mem.is_empty() {
                return Err(E::FamilyIdentity {
                    detail: "empty conjunction member id".into(),
                });
            }
            if !seen.insert(mem.as_str()) {
                return Err(E::FamilyIdentity {
                    detail: format!("duplicate conjunction member {mem}"),
                });
            }
        }
        // A required endpoint absent from the declared conjunction is
        // a removed required unknown — refused, never dropped.
        for ep in &decl.endpoints {
            if ep.required && !c.members.contains(&ep.endpoint) {
                return Err(E::FamilyIdentity {
                    detail: format!(
                        "required endpoint {} absent from conjunction {} — required unknowns are never removed",
                        ep.endpoint, c.claim_ref
                    ),
                });
            }
        }
    }
    let selected: Vec<&EndpointDecl> = decl
        .endpoints
        .iter()
        .filter(|e| {
            !decl
                .conjunction
                .as_ref()
                .map(|c| c.members.contains(&e.endpoint))
                .unwrap_or(false)
        })
        .collect();
    if (decl.selected_claims as usize) < selected.len() {
        return Err(E::FamilyIdentity {
            detail: format!(
                "selected_claims {} below the actual selected count {}",
                decl.selected_claims,
                selected.len()
            ),
        });
    }

    // ── endpoints ─────────────────────────────────────────────────
    let mut outcomes = Vec::with_capacity(decl.endpoints.len());
    for ep in &decl.endpoints {
        let is_member = decl
            .conjunction
            .as_ref()
            .map(|c| c.members.contains(&ep.endpoint))
            .unwrap_or(false);
        let margin = margins
            .iter()
            .find(|r| r.dimension == ep.endpoint)
            .ok_or_else(|| E::MissingMargin {
                dimension: ep.endpoint.clone(),
            })?;
        let out = match (ep.kind, margin.kind.as_str()) {
            (EndpointKind::Binary, "absolute") => {
                binary_endpoint(ep, margin.margin_ppm, is_member, decl.selected_claims)?
            }
            (EndpointKind::Bounded, "absolute") => {
                bounded_endpoint(ep, margin.margin_ppm, is_member, decl.selected_claims)?
            }
            (EndpointKind::Relative, "relative") => relative_endpoint(
                ep,
                margin.margin_ppm,
                margins,
                is_member,
                decl.selected_claims,
            )?,
            (_, other) => {
                return Err(E::MarginKindMismatch {
                    dimension: ep.endpoint.clone(),
                    margin_kind: other.to_string(),
                    endpoint_kind: format!("{:?}", ep.kind).to_lowercase(),
                })
            }
        };
        outcomes.push(out);
    }

    // ── the one IUT conjunction ───────────────────────────────────
    let conjunction = decl.conjunction.as_ref().map(|c| {
        let mut members = Vec::new();
        let mut unknown = Vec::new();
        for mem in &c.members {
            match outcomes.iter().find(|o| &o.endpoint == mem) {
                Some(o) => members.push((mem.clone(), o.verdict.clone())),
                // A declared member with no supplied endpoint is a
                // required unknown — listed, never removed.
                None => unknown.push(mem.clone()),
            }
        }
        let verdict = if !unknown.is_empty() {
            INCONCLUSIVE
        } else if members.iter().all(|(_, v)| v == EQUIVALENT) {
            EQUIVALENT
        } else if members.iter().any(|(_, v)| v == NOT_EQUIVALENT) {
            NOT_EQUIVALENT
        } else {
            INCONCLUSIVE
        };
        (c.claim_ref.clone(), verdict.to_string(), members, unknown)
    });

    let populations = Json::obj([
        ("kind", Json::str("declared_pairs")),
        (
            "note",
            Json::str(
                "canonical/raw/joint class counts read the declared pair records — authentic-population separation is owed to R5.39/R5.47",
            ),
        ),
        (
            "endpoints",
            Json::Arr(
                outcomes
                    .iter()
                    .map(|o| {
                        Json::obj([
                            ("endpoint", Json::str(&o.endpoint)),
                            ("n", Json::Int(o.pairs.0)),
                            ("resolved", Json::Int(o.pairs.1)),
                            ("uncertain", Json::Int(o.pairs.5)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);

    let mut keys = BTreeMap::new();
    if let Some(c) = &decl.conjunction {
        keys.insert("conjunction".into(), c.claim_ref.clone());
    }
    keys.insert("selected_claims".into(), decl.selected_claims.to_string());
    Ok(KlEquivalence {
        endpoints: outcomes,
        conjunction,
        populations,
        metadata: MethodMetadata {
            estimator: KIND.to_string(),
            version: VERSION.to_string(),
            target: "directional_kl_equivalence".to_string(),
            assumptions: vec![
                "across-pair independence required; within-pair dependence allowed — a recorded design assumption, never proved by reset logs or ids".into(),
                "equal weights/R identify the finite target (w=1/N); uncertain pairs preserve planned N via the completion envelope".into(),
                "concentration bound — never CP/binomial inversion, a credible interval, a score/MOVER fallback or observed-reference division".into(),
                "margins come only from the pre-registration; unknown required members stay listed and can never be removed or averaged away".into(),
                "synthetic declared pairs are admissible inputs — they are not authentic H/O pair evidence (owed to R5.39/R5.47)".into(),
            ],
            prior: None,
            keys,
            stratum_membership: decl
                .endpoints
                .iter()
                .map(|e| e.stratum.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            seed: 0,
            draws_requested: 0,
            draws_used: 0,
            floors: vec![
                "planned N = T·R preserved; equal repeats per task".to_string(),
                format!("eta_tost>={ETA_TOST_PPM}/claim; eta_report>={ETA_REPORT_PPM}/claim; neg_branch>={NEG_BRANCH_PPM}/claim"),
                "adapter_floor>=3/task and s3_floor>=5/task/arm are the pairing design's prerequisites (declared)".to_string(),
            ],
            interval: ("concentration_bound".to_string(), Some(900_000)),
            refusal: None,
        },
    })
}
