//! `l2_wire` — the R6.46/K7 L2 packet and analysis wiring.
//!
//! Thin composition over the registered machinery — never a second
//! estimator, veto table, or admission codec:
//!
//! - **REQ-XD-102** — [`l2_packet`] refuses typed when the declared
//!   record lacks the committed G-L1 pass id for the exact H version;
//!   no L2 pair exists before it.
//! - **REQ-XD-110/112** — [`participant_equal`] lists the non-harness
//!   participant fields that differ between the H and O records; a
//!   non-empty diff makes the pair `uncertain`, never an exclusion.
//! - **REQ-XD-113** — [`typed_unknowns`] declares served weights,
//!   cache, seeds, determinism and the support gate;
//!   [`check_no_coupling_claim`] refuses a CRN/seed-coupling spelling.
//! - **REQ-XD-114** — [`InfraEnvelope`] — classes declared before
//!   data, symmetric across arms; an observed hit marks the pair
//!   `uncertain` (never reattempted).
//! - **REQ-XD-116** — [`dimension_map_record`] — capability and
//!   reliability are the required binary endpoints at one absolute ε;
//!   grounding, efficiency, autonomy, security/compliance and
//!   portability/evolvability carry `inapplicable{reason}`.
//! - **REQ-XD-117** — [`veto_layer`] runs [`crate::vetoes::evaluate_vetoes`]
//!   and appends `not_checked{reason}` rows for undeterminable
//!   predicates — the parity claim folds over both.
//! - **REQ-XD-118** — [`evaluate_stop`] — operational triggers only
//!   (catalog change, budget exhaustion, infra threshold at declared
//!   checkpoints); remaining pairs go `uncertain`, planned N is
//!   unchanged, and there is no outcome-based trigger.
//! - **REQ-XD-101** — [`parity_claim`] — the claim is exactly
//!   `L1 gate pass ∧ L2 IUT conjunction equivalent ∧ empty veto list`;
//!   any false conjunct withholds. The first-pass label carries ε and
//!   N and contains no banned terminology.

use std::collections::{BTreeMap, BTreeSet};

use hharness_wire::Json;

use crate::directional_kl::ConjunctionOutcome;
use crate::facts::LedgerFacts;
use crate::vetoes::{evaluate_vetoes, VetoContext, VetoTrip};

/// The L2 packet schema.
pub const L2_SCHEMA: &str = "hh-fp-l2/1";
/// The claim the fold may emit — exactly REQ-XD-101.
pub const CLAIM_ID: &str = "P3-L2";

/// The typed unknowns REQ-XD-113 requires on the packet.
pub const UNKNOWN_KINDS: [&str; 5] = [
    "served_weights",
    "cache_state",
    "seeds",
    "determinism",
    "support_gate",
];

/// The operational stop triggers — §6.6's three kinds at declared
/// checkpoints. No outcome-based trigger exists.
pub const STOP_TRIGGERS: [&str; 3] = ["catalog_change", "budget_exhaustion", "infra_threshold"];

/// The participant fields REQ-XD-112 requires to match — every
/// non-harness field. `harness` itself is the treatment and is never
/// compared.
pub const PARTICIPANT_FIELDS: [&str; 9] = [
    "model_ref",
    "provider_model_id",
    "effort",
    "task",
    "repeat",
    "pair_id",
    "policy_set",
    "seed_record_ref",
    "environment_ref",
];

/// Phrases that may not appear in a first-pass label — equivalence
/// verdicts, never superiority (P3-L2 is a parity claim).
pub const BANNED_LABEL_PHRASES: [&str; 6] = [
    "superior",
    "better than",
    "outperform",
    "wins",
    "beats",
    "significant improvement",
];

/// The closed refusal set.
#[derive(Debug, Clone, PartialEq)]
pub enum L2Err {
    /// Malformed input.
    Schema(String),
    /// No committed G-L1 pass id for the exact H version — the pair
    /// cannot exist (REQ-XD-102).
    MissingGl1Pass,
    /// A CRN/seed-coupling claim was declared — unsupported
    /// (REQ-XD-113).
    CouplingClaim { declared: String },
    /// A stop trigger outside the operational set — an outcome-based
    /// stop is not a configuration (REQ-XD-118).
    OutcomeStop { declared: String },
    /// An undeterminable veto predicate without its `not_checked`
    /// reason (REQ-XD-117).
    UncheckedWithoutReason { veto_id: String },
    /// A banned label phrase (REQ-XD-101).
    BannedLabel { phrase: String },
}

impl std::fmt::Display for L2Err {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            L2Err::Schema(d) => write!(f, "hh-fp-l2/1: {d}"),
            L2Err::MissingGl1Pass => {
                write!(f, "no L2 pair before the committed G-L1 pass — REQ-XD-102")
            }
            L2Err::CouplingClaim { declared } => {
                write!(
                    f,
                    "coupling claim {declared} — CRN/seed coupling unsupported"
                )
            }
            L2Err::OutcomeStop { declared } => {
                write!(f, "stop trigger {declared} — operational triggers only")
            }
            L2Err::UncheckedWithoutReason { veto_id } => {
                write!(
                    f,
                    "veto {veto_id} undeterminable — not_checked needs its reason"
                )
            }
            L2Err::BannedLabel { phrase } => {
                write!(
                    f,
                    "label phrase '{phrase}' — parity claims carry no superiority wording"
                )
            }
        }
    }
}

impl std::error::Error for L2Err {}

/// `participant_equal(h, o)` — REQ-XD-112. Returns the differing
/// non-harness fields; non-empty ⇒ the pair is `uncertain`.
pub fn participant_equal(
    h: &BTreeMap<String, String>,
    o: &BTreeMap<String, String>,
) -> Vec<String> {
    PARTICIPANT_FIELDS
        .iter()
        .filter(|f| h.get(**f) != o.get(**f))
        .map(|f| (*f).to_string())
        .collect()
}

/// `typed_unknowns()` — REQ-XD-113's declared unknowns. Values are
/// the declaration spellings, never resolved at L2.
pub fn typed_unknowns() -> Json {
    Json::Arr(
        UNKNOWN_KINDS
            .iter()
            .map(|k| {
                Json::obj([
                    ("kind", Json::str(*k)),
                    ("status", Json::str("unknown")),
                    (
                        "note",
                        Json::str(match *k {
                            "served_weights" => "provider-served weights are not observed",
                            "cache_state" => "provider-side cache state is not observed",
                            "seeds" => "provider sampling seeds are not observed",
                            "determinism" => "provider decode determinism is not observed",
                            "support_gate" => "support-gate state is not observed",
                            _ => unreachable!(),
                        }),
                    ),
                ])
            })
            .collect(),
    )
}

/// `check_no_coupling_claim(declared)` — refuse CRN/seed-coupling
/// spellings (REQ-XD-113). `None` and `independent_pairs` pass.
pub fn check_no_coupling_claim(declared: Option<&str>) -> Result<(), L2Err> {
    const BANNED: [&str; 5] = [
        "crn",
        "common_random_numbers",
        "seed_coupling",
        "coupled_seeds",
        "shared_seed",
    ];
    if let Some(d) = declared {
        if BANNED.contains(&d) {
            return Err(L2Err::CouplingClaim {
                declared: d.to_string(),
            });
        }
    }
    Ok(())
}

/// The pre-data infrastructure envelope (REQ-XD-114) — classes
/// declared before data and symmetric across arms.
#[derive(Debug, Clone, PartialEq)]
pub struct InfraEnvelope {
    /// The declared classes (`infra:<cause>` spellings).
    pub classes: BTreeSet<String>,
    /// The pre-data declaration sequence — envelope precedes data.
    pub declared_before_data_seq: u64,
}

impl InfraEnvelope {
    /// Decode from `{classes: [...], declared_before_data_seq}`.
    pub fn from_json(j: &Json) -> Result<InfraEnvelope, L2Err> {
        let classes = match j.get("classes") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect(),
            _ => return Err(L2Err::Schema("infra.classes[] required".into())),
        };
        Ok(InfraEnvelope {
            classes,
            declared_before_data_seq: j
                .get("declared_before_data_seq")
                .and_then(Json::as_int)
                .ok_or_else(|| L2Err::Schema("declared_before_data_seq required".into()))?
                as u64,
        })
    }

    /// A pair observing `cause` (when it is a declared class) becomes
    /// `uncertain` — never reattempted. Undeclared classes refuse:
    /// the envelope was fixed before data.
    pub fn pair_status(&self, observed: Option<&str>) -> Result<PairStatus, L2Err> {
        match observed {
            None => Ok(PairStatus::Certain),
            Some(c) if self.classes.contains(c) => Ok(PairStatus::Uncertain {
                cause: c.to_string(),
            }),
            Some(c) => Err(L2Err::Schema(format!(
                "infra cause {c} not in the pre-data envelope — uncertain anyway, envelope drift"
            ))),
        }
    }
}

/// A pair's certainty after the envelope read.
#[derive(Debug, Clone, PartialEq)]
pub enum PairStatus {
    /// No infra observation — the pair stays in the analysis set.
    Certain,
    /// Infra-affected — uncertain, never reattempted.
    Uncertain {
        /// The declared class.
        cause: String,
    },
}

/// `dimension_map_record(epsilon_ppm)` — REQ-XD-116's map: required
/// binary endpoints at one absolute ε; the remaining five carry
/// `inapplicable{reason}`.
pub fn dimension_map_record(epsilon_ppm: i64) -> Json {
    let req = |dimension: &str| {
        Json::obj([
            ("dimension", Json::str(dimension)),
            ("status", Json::str("required")),
            ("kind", Json::str("binary")),
            (
                "margin",
                Json::obj([
                    ("kind", Json::str("absolute")),
                    ("margin_ppm", Json::Int(epsilon_ppm)),
                ]),
            ),
        ])
    };
    let inapp = |dimension: &str, reason: &str| {
        Json::obj([
            ("dimension", Json::str(dimension)),
            ("status", Json::str("inapplicable")),
            ("reason", Json::str(reason)),
        ])
    };
    Json::Arr(vec![
        req("capability"),
        req("reliability"),
        inapp(
            "grounding",
            "hosting-set equivalence gate — not a parity endpoint",
        ),
        inapp("efficiency", "L2 compares binary pair outcomes only"),
        inapp("autonomy", "single-shot pairs carry no autonomy endpoint"),
        inapp(
            "security_compliance",
            "checked by the veto layer, not the endpoint family",
        ),
        inapp(
            "portability_evolvability",
            "not an L2 estimand — first-pass scope",
        ),
    ])
}

/// `veto_layer(facts, ctx, undeterminable)` — REQ-XD-117. The
/// evaluate pass runs the registered predicates; undeterminable
/// predicates append `not_checked{reason}` rows (a reason is
/// required, never a silent pass).
pub fn veto_layer(
    facts: &LedgerFacts,
    ctx: &VetoContext,
    undeterminable: &[(String, Option<String>)],
) -> Result<Json, L2Err> {
    let trips = evaluate_vetoes(facts, ctx);
    let mut not_checked = Vec::new();
    for (veto_id, reason) in undeterminable {
        match reason {
            Some(r) if !r.is_empty() => not_checked.push(Json::obj([
                ("veto_id", Json::str(veto_id.as_str())),
                ("status", Json::str("not_checked")),
                ("reason", Json::str(r.as_str())),
            ])),
            _ => {
                return Err(L2Err::UncheckedWithoutReason {
                    veto_id: veto_id.clone(),
                })
            }
        }
    }
    Ok(Json::obj([
        (
            "tripped",
            Json::Arr(
                trips
                    .iter()
                    .map(|t| {
                        Json::obj([
                            ("veto_id", Json::str(t.veto_id.as_str())),
                            ("evidence", t.evidence.clone()),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("not_checked", Json::Arr(not_checked)),
    ]))
}

/// The REQ-XD-118 operational stop evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum StopVerdict {
    /// Continue — no operational trigger fired.
    Continue,
    /// An operational trigger fired at a declared checkpoint — the
    /// remaining pairs become `uncertain`; planned N is unchanged.
    Stopped {
        /// The fired trigger (a [`STOP_TRIGGERS`] member).
        trigger: String,
    },
}

/// `evaluate_stop(trigger)` — only the operational set triggers a
/// stop; every other spelling (including outcome-based stops)
/// refuses typed.
pub fn evaluate_stop(trigger: Option<&str>) -> Result<StopVerdict, L2Err> {
    match trigger {
        None => Ok(StopVerdict::Continue),
        Some(t) if STOP_TRIGGERS.contains(&t) => Ok(StopVerdict::Stopped {
            trigger: t.to_string(),
        }),
        Some(t) => Err(L2Err::OutcomeStop {
            declared: t.to_string(),
        }),
    }
}

/// `stopped_packet(remaining_pairs, planned_n)` — a stop marks the
/// remaining pairs `uncertain` and leaves planned N unchanged.
pub fn stopped_packet(remaining: &[String], planned_n: i64) -> Json {
    Json::obj([
        (
            "uncertain_pairs",
            Json::Arr(remaining.iter().map(Json::str).collect()),
        ),
        ("planned_n", Json::Int(planned_n)),
        ("planned_n_unchanged", Json::Bool(true)),
    ])
}

/// The first-pass label — carries ε and N and never a banned
/// superiority phrase (REQ-XD-101).
pub fn first_pass_label(epsilon_ppm: i64, planned_n: i64) -> Result<String, L2Err> {
    let label = format!(
        "first-pass parity — IUT conjunction equivalent at ε = {} ppm over N = {} pairs",
        epsilon_ppm, planned_n
    );
    for p in BANNED_LABEL_PHRASES {
        if label.to_lowercase().contains(p) {
            return Err(L2Err::BannedLabel {
                phrase: p.to_string(),
            });
        }
    }
    Ok(label)
}

/// `parity_claim(l1_passed, admitted, conjunction, vetoes, eps, n)` —
/// REQ-XD-101: the claim is exactly
/// `L1 gate pass ∧ L2 IUT conjunction equivalent ∧ empty veto list`.
/// `admitted` is the `ParityAdmission.admission == "admitted"` fold;
/// `conjunction` is the `KlEquivalence.conjunction` outcome — any
/// false conjunct withholds.
pub fn parity_claim(
    l1_passed: bool,
    admitted: bool,
    conjunction: Option<&ConjunctionOutcome>,
    vetoes: &[VetoTrip],
    epsilon_ppm: i64,
    planned_n: i64,
) -> Result<Json, L2Err> {
    let conjunction_equiv = conjunction
        .map(|c| c.1.as_str() == "equivalent")
        .unwrap_or(false);
    let vetoes_empty = vetoes.is_empty();
    let n = planned_n;
    let label = first_pass_label(epsilon_ppm, n)?;
    let verdict = if l1_passed && admitted && conjunction_equiv && vetoes_empty {
        "admitted"
    } else {
        "withheld{incomplete_conjunction}"
    };
    Ok(Json::obj([
        ("schema", Json::str(L2_SCHEMA)),
        ("claim", Json::str(CLAIM_ID)),
        (
            "conjunction",
            Json::obj([
                ("l1_gate_pass", Json::Bool(l1_passed)),
                ("admission", Json::Bool(admitted)),
                ("iut_conjunction_equivalent", Json::Bool(conjunction_equiv)),
                ("vetoes_empty", Json::Bool(vetoes_empty)),
            ]),
        ),
        ("verdict", Json::str(verdict)),
        ("epsilon_ppm", Json::Int(epsilon_ppm)),
        ("planned_n", Json::Int(n)),
        ("label", Json::str(label)),
    ]))
}

/// `l2_packet(gl1_pass_id)` — REQ-XD-102's precondition on the
/// packet itself: the record refuses to exist without the committed
/// G-L1 pass id for the exact H version.
pub fn l2_packet(gl1_pass_id: Option<&str>, h_identity_digest: &str) -> Result<Json, L2Err> {
    let pass = gl1_pass_id.ok_or(L2Err::MissingGl1Pass)?;
    if h_identity_digest.is_empty() {
        return Err(L2Err::Schema("h_identity_digest required".into()));
    }
    Ok(Json::obj([
        ("schema", Json::str(L2_SCHEMA)),
        ("gl1_pass_id", Json::str(pass)),
        ("h_identity_digest", Json::str(h_identity_digest)),
        ("unknowns", typed_unknowns()),
        ("dimension_map", dimension_map_record(0)),
    ]))
}
