//! `hh-outside-withheld/1` — the withheld outside clean-recipient
//! verify/recompute disposition record (R5.52; GATE-G15 PASSED —
//! **out-of-scope disposition**; evidence spec §6.3's clean-recipient
//! procedures and AC-6-2's "one consenting outside reviewer must
//! actually complete both promised verify/recompute procedures" —
//! withheld this round: no outside reviewer exists under withheld
//! HUMAN-H9 — DF-H9-1 stays open; AC-6-2: "Neither author-only
//! testing nor a declaration awards independence").
//!
//! Records-in only: every leg evaluator folds *supplied* facts —
//! the gate's verbatim decision, the pinned-input digest counts,
//! the §6.3 procedure vocabulary the landed `ReproLevel`/
//! `FreshLevel`/`cards::code`/record-schema/`within_tolerance`
//! machinery declares, the verbatim refusal/independence outcomes
//! the landed `admit_fresh`/`admit_replay`/`compute_independence`
//! paths produced on exercised cases, the absent procedure counts
//! and the withheld-claims fold — and never manufactures a
//! reviewer, a verify/recompute run, an admission or a claim.
//! `withheld` is the honest terminal state of the voided stage; a
//! definitive failure folds `failed`, an undetermined leg
//! `inconclusive` — never an invented outside run.

use std::collections::BTreeMap;

use hharness_identity::idp::idp_digest;
use hharness_wire::json::Json;

use crate::preflight::{Leg, LegVerdict};

/// The packet disposition — the `withheld | failed | inconclusive`
/// fold is the R5.48 contract consumed unchanged
/// (`confirm::Disposition` re-exported: one closed vocabulary,
/// never a second implementation).
pub use crate::confirm::Disposition;

/// The record version.
pub const VERSION: &str = "hh-outside-withheld/1";
const DOMAIN: &str = "outside_withheld";

/// The gate whose out-of-scope decision this packet records.
pub const GATE: &str = "GATE-G15";

/// The six legs (frozen vocabulary — the withheld scope).
pub const LEGS: [&str; 6] = [
    "gate_readout",
    "pins_frozen",
    "procedure_vocabulary",
    "refusal_vocabulary",
    "procedure_absent",
    "claims_withheld",
];

// ── leg 1 — gate_readout ─────────────────────────────────────────
//
// GATE-G15's verbatim decision facts are the R5.50 contract — the
// `plugin::GateFacts`/`evaluate_gate` predecessor is consumed
// unchanged (one gate, one evaluator, never a second copy).

// ── leg 2 — pins_frozen ──────────────────────────────────────────

/// The pinned-input count — the nine H8 hash-verified upstream
/// artifacts plus the committed `crates/hh-xcheck/fixtures/
/// cross-impl/bundle.json` native bundle manifest (the cleared-
/// artifact substrate the verify/recompute handoff would carry —
/// pin 10, declared never consumed).
pub const PINS_TOTAL: u64 = 10;

/// The pin-verification facts — `check_pins` over the ten declared
/// inputs, plus the bundle manifest's digest echoed verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct PinsFacts {
    /// The pinned-input count (`10`).
    pub pins: u64,
    /// The `check_pins` verdict name (`verified`).
    pub pins_verdict: String,
    /// The committed cross-impl `bundle.json` sha256 (64-hex,
    /// echoed — never hard-coded into the evaluator).
    pub bundle_sha256: String,
}

/// `evaluate_pins` — the pin check: ten inputs, `check_pins`
/// verified and a 64-hex bundle digest supplied. A drifted pin
/// refuses; an unreadable pin is inconclusive upstream — an
/// empty/short digest here is inconclusive, never guessed.
pub fn evaluate_pins(f: &PinsFacts) -> LegVerdict {
    if f.pins != PINS_TOTAL {
        return LegVerdict::Refused(format!("pins:{}", f.pins));
    }
    if f.pins_verdict != "verified" {
        return LegVerdict::Refused(format!("pins_verdict:{}", f.pins_verdict));
    }
    let ok_digest =
        f.bundle_sha256.len() == 64 && f.bundle_sha256.chars().all(|c| c.is_ascii_hexdigit());
    if !ok_digest {
        return LegVerdict::Inconclusive("bundle_sha256_absent".to_string());
    }
    LegVerdict::Verified
}

// ── leg 3 — procedure_vocabulary ─────────────────────────────────

/// The noncumulative `ReproLevel` closed sum, verbatim —
/// `ReproLevel::all().name()` comma-joined in declaration order
/// (§6.3: the levels stay distinct, never cumulative).
pub const EXPECTED_REPRO_LEVELS: &str = "R0,R1,R2,R3";

/// The `FreshLevel` spellings, verbatim — `r2`/`r3`/`transfer`
/// (R3 is not cumulative R2; transfer is its own procedure, never
/// an R level).
pub const EXPECTED_FRESH_LEVELS: &str = "r2,r3,transfer";

/// The closed `cards::code` refusal set, verbatim — the eleven
/// `BundleError::CardRefused` codes comma-joined in declaration
/// order (the card refusal vocabulary — typed refusals, never a
/// silent verdict).
pub const EXPECTED_CARD_CODES: &str = "missing_input,unsupported,isolation_disabled,unfunded,not_fresh,charging_attribution,unavailable,not_independent,outside_relationship,no_new_phase,new_question";

/// The prior-engineering record schemas, verbatim — the landed
/// procedure surfaces' schema labels comma-joined: `hh-repro/1`
/// (the R5.24 verify path), `analysis-recompute/1` (the R5.25
/// recompute owner), `replay-card/1` + `fresh-card/1` (the R5.26
/// admission cards), `rights-record/1` + `handoff-report/1` +
/// `rights-supersession/1` + `rights-tombstone/1` (the R5.27
/// rights/handoff owner).
pub const EXPECTED_SCHEMAS: &str = "hh-repro/1,analysis-recompute/1,replay-card/1,fresh-card/1,rights-record/1,handoff-report/1,rights-supersession/1,rights-tombstone/1";

/// The only classes a replay may append to the disposable copy —
/// `lifecycle.replay.` markers; anything else is suffix confusion
/// or an attempted live dispatch.
pub const EXPECTED_MARKER_PREFIX: &str = "lifecycle.replay.";

/// The §6.3 procedure vocabulary facts — the declared vocabulary
/// the landed machinery carries (read, never re-declared: the
/// battery supplies what the machinery actually says).
#[derive(Debug, Clone, PartialEq)]
pub struct VocabularyFacts {
    /// The `ReproLevel` closed sum comma-joined (`R0,R1,R2,R3`).
    pub repro_levels: String,
    /// Every `ReproLevel` spelling round-trips `parse` (`true`).
    pub repro_parse_roundtrip: bool,
    /// Outside spellings parse to `None` — `transfer` is not an R
    /// level; the grammar never guesses (`true`).
    pub repro_parse_never_guesses: bool,
    /// The `FreshLevel` spellings comma-joined (`r2,r3,transfer`).
    pub fresh_levels: String,
    /// The closed `cards::code` refusal set comma-joined (eleven
    /// codes).
    pub card_codes: String,
    /// The prior-engineering record schemas comma-joined (the
    /// eight landed procedure-record labels).
    pub record_schemas: String,
    /// `cards::REPLAY_MARKER_PREFIX` (`lifecycle.replay.`).
    pub replay_marker_prefix: String,
    /// The frozen `within_tolerance` scalar rule demonstrated on
    /// supplied cases (`true` — §6.3: `abs(new−ref) ≤ 1e-10 ×
    /// max(1,abs(ref))`, never widened).
    pub tolerance_holds: bool,
}

/// `evaluate_vocabulary` — the declared-vocabulary check: the
/// supplied facts equal the landed procedure decls verbatim. A
/// drifted fact is a different vocabulary — refused, never
/// relabelled; an absent sum is inconclusive.
pub fn evaluate_vocabulary(f: &VocabularyFacts) -> LegVerdict {
    if f.repro_levels.is_empty() {
        return LegVerdict::Inconclusive("vocabulary_absent".to_string());
    }
    if f.repro_levels != EXPECTED_REPRO_LEVELS {
        return LegVerdict::Refused(format!("repro_levels:{}", f.repro_levels));
    }
    if !f.repro_parse_roundtrip {
        return LegVerdict::Refused("repro_parse_roundtrip:false".to_string());
    }
    if !f.repro_parse_never_guesses {
        return LegVerdict::Refused("repro_parse_never_guesses:false".to_string());
    }
    if f.fresh_levels != EXPECTED_FRESH_LEVELS {
        return LegVerdict::Refused(format!("fresh_levels:{}", f.fresh_levels));
    }
    if f.card_codes != EXPECTED_CARD_CODES {
        return LegVerdict::Refused(format!("card_codes:{}", f.card_codes));
    }
    if f.record_schemas != EXPECTED_SCHEMAS {
        return LegVerdict::Refused(format!("record_schemas:{}", f.record_schemas));
    }
    if f.replay_marker_prefix != EXPECTED_MARKER_PREFIX {
        return LegVerdict::Refused(format!("replay_marker_prefix:{}", f.replay_marker_prefix));
    }
    if !f.tolerance_holds {
        return LegVerdict::Refused("tolerance_holds:false".to_string());
    }
    LegVerdict::Verified
}

// ── leg 4 — refusal_vocabulary ───────────────────────────────────

/// The exercised case set — `exercise → expected verbatim outcome`
/// (the landed `admit_fresh`/`admit_replay`/`compute_independence`
/// machinery's spellings, driven on exercised cases only: the
/// refusals fire before any admission card is minted or any
/// execution is admitted). The `admit_fresh` cases cover the
/// F-205 fresh gates and the F-206 outside-relationship gate —
/// `outside_relationship` is the reviewer-absent refusal itself
/// (outside authority/assistance/attestation undeclared); the
/// `admit_replay` cases cover the F-204 isolation/recorded-input
/// gates (any attempted live dispatch fails, even if transport
/// blocks it); the `compute_independence` cases cover the
/// independence comparison — `same_signer` is the same-author
/// negative (§6.3: "Same-author directory/key … proves neither
/// independence"), the undeclared case fails closed on absent
/// material, and `distinct_signer_and_installation` is the
/// machinery's declared-material comparison standing (recorded
/// output, never a relationship claim).
pub const EXERCISES: [(&str, &str); 9] = [
    ("fresh_unfunded", "unfunded"),
    ("fresh_not_fresh", "not_fresh"),
    ("transfer_outside_undeclared", "outside_relationship"),
    ("transfer_new_question", "new_question"),
    ("replay_isolation_enabled", "isolation_disabled"),
    ("replay_responses_absent", "missing_input"),
    ("independence_same_signer", "same_signer"),
    ("independence_undeclared", "reproducer_signer_undeclared"),
    ("independence_distinct", "distinct_signer_and_installation"),
];

/// `evaluate_refusals` — the machinery-stands check: every
/// declared exercise produced its verbatim typed outcome. A
/// missing exercise is `inconclusive`; a wrong outcome — or
/// `admitted`/`card_minted`, the guard broken — is `refused`,
/// never reclassified.
pub fn evaluate_refusals(observed: &BTreeMap<String, String>) -> LegVerdict {
    for (exercise, expected) in EXERCISES {
        match observed.get(exercise).map(String::as_str) {
            Some(o) if o == expected => {}
            Some("admitted") | Some("card_minted") | Some("executed") => {
                return LegVerdict::Refused(format!("produced:{exercise}"));
            }
            Some(o) => return LegVerdict::Refused(format!("{exercise}:{o}")),
            None => return LegVerdict::Inconclusive(format!("exercise_absent:{exercise}")),
        }
    }
    LegVerdict::Verified
}

// ── leg 5 — procedure_absent ─────────────────────────────────────

/// The absent outside verify/recompute, verbatim — every
/// reviewer/procedure/evidence count a declared zero, no reviewer
/// contacted, no handoff, no producer cleanup.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureFacts {
    /// `member → count` — every [`PROCEDURE_MEMBERS`] member
    /// declared `0`.
    pub counts: BTreeMap<String, u64>,
    /// An outside reviewer was contacted or engaged (`false`).
    pub reviewer_contacted: bool,
    /// A cleared-artifact handoff/materialization was attempted
    /// (`false`).
    pub handoff_attempted: bool,
    /// A producer cleanup was performed or attempted (`false` —
    /// the "after producer cleanup" precondition never ran).
    pub cleanup_attempted: bool,
}

/// The nineteen absent-procedure members (frozen vocabulary).
pub const PROCEDURE_MEMBERS: [&str; 19] = [
    "outside_reviewers_engaged",
    "artifacts_cleared",
    "producer_cleanups",
    "verify_procedures_run",
    "recompute_procedures_run",
    "replay_cards_minted",
    "fresh_cards_minted",
    "r1_rederivations",
    "fresh_r2_attempts",
    "fresh_r3_attempts",
    "transfer_admissions",
    "installations_checked",
    "signers_checked",
    "relationship_records",
    "assistance_records",
    "verdicts_received",
    "losses_recorded",
    "reports_received",
    "participant_understanding_records",
];

/// `evaluate_procedure` — the absent-procedure check: every member
/// zero, no reviewer contact, no handoff, no cleanup. Any nonzero
/// fact is a real procedure's evidence — it belongs in a different
/// packet and refuses here, never a fabricated absence.
pub fn evaluate_procedure(f: &ProcedureFacts) -> LegVerdict {
    for member in PROCEDURE_MEMBERS {
        match f.counts.get(member) {
            Some(0) => {}
            Some(n) => return LegVerdict::Refused(format!("{member}:{n}")),
            None => return LegVerdict::Inconclusive(format!("{member}_absent")),
        }
    }
    if f.reviewer_contacted {
        return LegVerdict::Refused("reviewer_contacted:true".to_string());
    }
    if f.handoff_attempted {
        return LegVerdict::Refused("handoff_attempted:true".to_string());
    }
    if f.cleanup_attempted {
        return LegVerdict::Refused("cleanup_attempted:true".to_string());
    }
    LegVerdict::Verified
}

// ── leg 6 — claims_withheld ──────────────────────────────────────

/// The withheld-claims facts — DF-H9-1 stays open, every
/// participant/outreach predicate withheld and no claim of any
/// sign is present (AC-6-2: "Neither author-only testing nor a
/// declaration awards independence"; portability unmet until
/// repair/recheck; I-08: named prerequisites stay named).
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimsFacts {
    /// DF-H9-1's recorded state (`open`).
    pub df_h9_1: String,
    /// The participant/outreach predicate set (`withheld`).
    pub participant_predicates: String,
    /// Any outreach performed (`none`).
    pub outreach: String,
    /// A claim awarded by the withheld disposition (`false`).
    pub claim_awarded: bool,
    /// An outside-review completion claim (`false` — no reviewer
    /// ran).
    pub outside_review_claim: bool,
    /// An independence claim (`false` — AC-6-2: a declaration
    /// awards none; the same-author directory proves none).
    pub independence_claim: bool,
    /// A relationship/assistance claim (`false` — undeclared).
    pub relationship_claim: bool,
    /// Any reproducibility level awarded (`false`).
    pub r_level_awarded: bool,
    /// An R1 replay/rederivation claim (`false`).
    pub r1_replay_claim: bool,
    /// A fresh R2/R3 execution claim (`false`).
    pub fresh_claim: bool,
    /// An independent-replication/transfer claim (`false`).
    pub transfer_claim: bool,
    /// A portability claim (`false` — AC-6-2: unmet until
    /// repair/recheck).
    pub portability_claim: bool,
    /// A publication claim (`false`).
    pub publication_claim: bool,
    /// A capability claim (`false`).
    pub capability_claim: bool,
    /// An adoption claim (`false`).
    pub adoption_claim: bool,
}

/// `evaluate_claims` — the withheld check: DF-H9-1 `open`,
/// participant predicates `withheld`, outreach `none`, no claim
/// of any sign. A claim — or a discharged deferral — refuses,
/// never relabels.
pub fn evaluate_claims(f: &ClaimsFacts) -> LegVerdict {
    if f.df_h9_1 != "open" {
        return LegVerdict::Refused(format!("df_h9_1:{}", f.df_h9_1));
    }
    if f.participant_predicates != "withheld" {
        return LegVerdict::Refused(format!(
            "participant_predicates:{}",
            f.participant_predicates
        ));
    }
    if f.outreach != "none" {
        return LegVerdict::Refused(format!("outreach:{}", f.outreach));
    }
    if f.claim_awarded
        || f.outside_review_claim
        || f.independence_claim
        || f.relationship_claim
        || f.r_level_awarded
        || f.r1_replay_claim
        || f.fresh_claim
        || f.transfer_claim
        || f.portability_claim
        || f.publication_claim
        || f.capability_claim
        || f.adoption_claim
    {
        return LegVerdict::Refused("claim_present".to_string());
    }
    LegVerdict::Verified
}

// ── the report ───────────────────────────────────────────────────

/// The `hh-outside-withheld/1` record — the honest
/// `withheld{stage_voided}`/reviewer-absent disposition: no
/// outside reviewer exists, every verify/recompute phase absent,
/// all participant/outreach/procedure claims withheld and DF-H9-1
/// open; `packet_id` is minted last over the canonical record.
#[derive(Debug, Clone, PartialEq)]
pub struct OutsideWithheldReport {
    /// `H(canonical(record minus packet_id))` under
    /// `outside_withheld`.
    pub packet_id: String,
    /// The record version.
    pub version: String,
    /// The gate whose decision is recorded (`GATE-G15`).
    pub gate: String,
    /// The outside verify/recompute is withheld (`true`).
    pub withheld: bool,
    /// The participation stage is voided this round (`true`).
    pub stage_voided: bool,
    /// No outside reviewer exists (`true` — HUMAN-H9 withheld).
    pub reviewer_absent: bool,
    /// DF-H9-1 stays open (`true` — recorded honestly).
    pub df_h9_1_open: bool,
    /// Outside verify/recompute procedures completed (`0`).
    pub procedures_completed: u64,
    /// The six legs.
    pub legs: Vec<Leg>,
    /// The folded disposition.
    pub disposition: Disposition,
    /// The admission shape — `withheld_stage_voided`, never an
    /// execution.
    pub admission: String,
}

impl OutsideWithheldReport {
    /// `to_json` — the canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(&self.version)),
            ("packet_id", Json::str(&self.packet_id)),
            ("gate", Json::str(&self.gate)),
            ("withheld", Json::Bool(self.withheld)),
            ("stage_voided", Json::Bool(self.stage_voided)),
            ("reviewer_absent", Json::Bool(self.reviewer_absent)),
            ("df_h9_1_open", Json::Bool(self.df_h9_1_open)),
            (
                "procedures_completed",
                Json::Int(self.procedures_completed as i64),
            ),
            ("disposition", Json::str(self.disposition.name())),
            ("admission", Json::str(&self.admission)),
            (
                "legs",
                Json::Arr(
                    self.legs
                        .iter()
                        .map(|l| {
                            Json::obj([
                                ("name", Json::str(&l.name)),
                                ("verdict", l.verdict.to_json()),
                                (
                                    "facts",
                                    Json::Obj(
                                        l.facts
                                            .iter()
                                            .map(|(k, v)| (k.clone(), Json::str(v)))
                                            .collect(),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}

/// `assemble(legs)` — fold the six leg verdicts into the
/// disposition and mint `packet_id` last over the canonical
/// record. Every leg name must be a `LEGS` member, unique and
/// complete — a malformed leg set is a `refused` assembly, never a
/// partial packet.
pub fn assemble(legs: Vec<Leg>) -> Result<OutsideWithheldReport, String> {
    let mut seen = std::collections::BTreeSet::new();
    for l in &legs {
        if !LEGS.contains(&l.name.as_str()) {
            return Err(format!("leg vocabulary: {}", l.name));
        }
        if !seen.insert(l.name.as_str()) {
            return Err(format!("leg duplicate: {}", l.name));
        }
    }
    for name in LEGS {
        if !seen.contains(name) {
            return Err(format!("leg missing: {name}"));
        }
    }
    let definitive = legs.iter().any(|l| {
        matches!(
            l.verdict,
            LegVerdict::Unsupported(_) | LegVerdict::Refused(_)
        )
    });
    let undetermined = legs
        .iter()
        .any(|l| matches!(l.verdict, LegVerdict::Inconclusive(_)));
    let disposition = if definitive {
        Disposition::Failed
    } else if undetermined {
        Disposition::Inconclusive
    } else {
        Disposition::Withheld
    };
    let mut out = OutsideWithheldReport {
        packet_id: String::new(),
        version: VERSION.to_string(),
        gate: GATE.to_string(),
        withheld: true,
        stage_voided: true,
        reviewer_absent: true,
        df_h9_1_open: true,
        procedures_completed: 0,
        legs,
        disposition,
        admission: "withheld_stage_voided".to_string(),
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("packet_id");
    }
    out.packet_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}
