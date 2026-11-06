//! `hh-value-withheld/1` — the withheld bounded day-30/+90 manual
//! value-review and adoption/contribution follow-up disposition
//! record (R5.55; GATE-G16 PASSED — **out-of-scope disposition** +
//! HUMAN-H11 recorded WITHHELD; evidence spec §8.3's manual
//! day-30/day-90 value review stage and §8.4's rubric/denominator
//! rules — withheld this round: nothing published so no review
//! dates exist, no observation was compiled and no contacts or
//! raw observations were ever created — nothing to delete;
//! DF-H9-1 stays open; §8.4: "Missing follow-up is unknown, not
//! no use" — and absence of adoption remains an observed outcome,
//! not hidden success).
//!
//! Records-in only: every leg evaluator folds *supplied* facts —
//! the gate's verbatim decision and the withheld human records,
//! the pinned-input digest counts, the §8.3/§8.4 review/rubric
//! vocabulary the frozen decls declare, the verbatim typed
//! outcomes the exercised refusal/inflation cases produced, the
//! pinned predecessor packets' adverse + unmet + absent members
//! carried verbatim (`adoption:unavailable{not_started}` — the
//! recorded observed outcome, never inflated), the absent
//! observation counts and the withheld-claims fold — and never
//! manufactures a date, a review, an observation, a denominator,
//! a contact, a deletion or a claim. `withheld` is the honest
//! terminal state of the voided stage; a definitive failure
//! folds `failed`, an undetermined leg `inconclusive` — never an
//! invented review record.

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
pub const VERSION: &str = "hh-value-withheld/1";
const DOMAIN: &str = "value_withheld";

/// The gate whose out-of-scope decision this packet records.
pub const GATE: &str = "GATE-G16";

/// The pinned study report's honest status, carried verbatim —
/// never inflated (REQ-G-209: adverse results/corrections retained).
pub const STUDY_STATUS: &str = "unmet{efficacy_unavailable;parity_withheld}";

/// The study's recorded adoption-family status, carried verbatim —
/// the observed outcome of record ("Missing follow-up is unknown,
/// not no use"; a voided cohort reports `unavailable{not_started}`,
/// never a promoted success).
pub const ADOPTION_STATUS: &str = "unavailable{not_started}";

/// The honest adoption-absence record — absence of adoption
/// remains an observed outcome, not hidden success (skeleton
/// scope verbatim).
pub const ADOPTION_ABSENCE: &str = "observed_not_achieved";

/// The seven legs (frozen vocabulary — the withheld scope).
pub const LEGS: [&str; 7] = [
    "gate_readout",
    "pins_frozen",
    "review_vocabulary",
    "refusal_vocabulary",
    "predecessors_carried",
    "observation_absent",
    "claims_withheld",
];

// ── leg 1 — gate_readout ─────────────────────────────────────────
//
// GATE-G16's verbatim decision facts are the R5.50 contract — the
// `plugin::GateFacts`/`evaluate_gate` predecessor is consumed
// unchanged (one gate, one evaluator, never a second copy). The
// withheld human records ride the same leg through the R5.54
// `publication::HumanFacts`/`evaluate_humans` contract — HUMAN-H10
// and HUMAN-H11 both read `withheld`, `none` acts approved; H11's
// verbatim record: no day-30/+90 dates exist (the calendar
// derives from actual publication, which did not occur).

// ── leg 2 — pins_frozen ──────────────────────────────────────────

/// The pinned-input count — the nine H8 hash-verified upstream
/// artifacts plus the committed `hh-study-report/1` packet (the
/// internal claim→receipt spine; its recorded
/// `adoption:unavailable{not_started}` is the observed outcome
/// carried verbatim) and the committed `hh-publication-withheld/1`
/// packet (the frozen dated record that no publication occurred —
/// the causal premise that no day-30/+90 dates derive). Both
/// packets pin their committed byte-copy fixtures (the CI readers
/// gate bars gated test sources from reading committed
/// documentation content). The GATE-G16/HUMAN-H10/HUMAN-H11
/// readouts are echoed verbatim as leg facts, never pinned —
/// living append-only records.
pub const PINS_TOTAL: u64 = 11;

/// The pin-verification facts — `check_pins` over the eleven
/// declared inputs, plus the two packet digests echoed verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct PinsFacts {
    /// The pinned-input count (`11` — [`PINS_TOTAL`]).
    pub pins: u64,
    /// The `check_pins` verdict name (`verified`).
    pub pins_verdict: String,
    /// The committed study-report sha256 (64-hex, echoed — never
    /// hard-coded into the evaluator).
    pub study_sha256: String,
    /// The committed publication-withheld packet sha256 (64-hex,
    /// echoed).
    pub publication_sha256: String,
}

/// `evaluate_pins` — the pin check: eleven inputs, `check_pins`
/// verified and two 64-hex packet digests supplied. A drifted pin
/// refuses; an unreadable pin is inconclusive upstream — an
/// empty/short digest here is inconclusive, never guessed.
pub fn evaluate_pins(f: &PinsFacts) -> LegVerdict {
    if f.pins != PINS_TOTAL {
        return LegVerdict::Refused(format!("pins:{}", f.pins));
    }
    if f.pins_verdict != "verified" {
        return LegVerdict::Refused(format!("pins_verdict:{}", f.pins_verdict));
    }
    for d in [&f.study_sha256, &f.publication_sha256] {
        let ok = d.len() == 64 && d.chars().all(|c| c.is_ascii_hexdigit());
        if !ok {
            return LegVerdict::Inconclusive("packet_sha256_absent".to_string());
        }
    }
    LegVerdict::Verified
}

// ── leg 3 — review_vocabulary ────────────────────────────────────

// The fixed §8.3 publication stage order (ending
// `manual_day30_day90_value_review` — the stage this packet records
// withheld) is declared once in `publication` — referenced as
// `crate::publication::EXPECTED_STAGE_ORDER`, never re-declared.

/// The seven §8.4 measure names comma-joined in table order,
/// verbatim — "Visibility; Access; Useful task; Understanding/
/// procedure; Independent own-task use; Repeat use; Contribution"
/// (denominators visible; "Missing follow-up is unknown, not no
/// use").
pub const EXPECTED_MEASURES: &str =
    "Visibility,Access,Useful task,Understanding/procedure,Independent own-task use,Repeat use,Contribution";
/// The §8.4 manual-review outcome vocabulary, verbatim — "yields
/// continue/repair/narrow/stop with evidence/unknowns/owner".
pub const EXPECTED_REVIEW_OUTCOMES: &str = "continue,repair,narrow,stop";
/// The §8.4 review checkpoints, verbatim — "Manual review at day
/// 30 and 90 after capability publication".
pub const EXPECTED_CHECKPOINTS: &str = "day30,day90";
/// The §8.4 total maintainer ceiling, verbatim — "Total maintainer
/// ceiling ≤24 hours includes preparation/sessions/triage/
/// follow-ups/reviews; record actual, exhaustion=incomplete/
/// requote."
pub const EXPECTED_CEILING: &str = "total_ceiling_hours:24";
/// The §8.4 follow-up rules, verbatim — "At most one optional
/// request per checkpoint to prior consenting participants, no
/// chasing."
pub const EXPECTED_FOLLOWUP_RULES: &str = "at_most_one_optional_request_per_checkpoint,no_chasing";
/// The §8.4 deletion rule, verbatim — "Delete contacts/raw
/// observations 30 days after day-90 review or earlier withdrawal
/// where possible, actual dates disclosed before consent…
/// Scientific receipt retention is separate."
pub const EXPECTED_DELETION_RULE: &str =
    "contacts_raw_observations_deleted:30_days_after_day90_or_earlier_withdrawal_where_possible,receipt_retention_separate";
/// The §8.4 honesty rules, verbatim — "Missing follow-up is
/// unknown, not no use"; "no traffic/fixture/assisted-only
/// inference of adoption" (AC-8-2); "Convenience volunteers
/// cannot estimate population adoption or causal improvement";
/// "Suppress identifying tiny strata publicly"; "Only consented
/// non-identifying summaries publish".
pub const EXPECTED_HONESTY_RULES: &str = "missing_followup_unknown_not_no_use,no_traffic_fixture_assisted_only_inference_of_adoption,convenience_volunteers_cannot_estimate_population_adoption,suppress_identifying_tiny_strata,consented_nonidentifying_summaries_only";

/// The §8.3/§8.4 review/rubric vocabulary facts — the declared
/// vocabulary the frozen decls carry (supplied verbatim — the
/// rubric stands declared, never exercised).
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewFacts {
    /// The fixed six-stage order ending
    /// `manual_day30_day90_value_review`
    /// (`publication::EXPECTED_STAGE_ORDER` — one decl).
    pub stage_order: String,
    /// The seven §8.4 measure names comma-joined.
    pub measures: String,
    /// The `continue,repair,narrow,stop` review outcomes.
    pub review_outcomes: String,
    /// The `day30,day90` checkpoints.
    pub checkpoints: String,
    /// The ≤24-hour total maintainer ceiling decl.
    pub maintainer_ceiling: String,
    /// The at-most-one-optional-request/no-chasing follow-up
    /// rules.
    pub followup_rules: String,
    /// The contacts/raw-observations deletion rule with separate
    /// receipt retention.
    pub deletion_rule: String,
    /// The §8.4 honesty rules comma-joined.
    pub honesty_rules: String,
}

/// `evaluate_review` — the declared-vocabulary check: the supplied
/// facts equal the frozen §8.3/§8.4 decls verbatim. A drifted fact
/// is a different vocabulary — refused, never relabelled; an
/// absent vocabulary is inconclusive.
pub fn evaluate_review(f: &ReviewFacts) -> LegVerdict {
    if f.stage_order.is_empty() {
        return LegVerdict::Inconclusive("vocabulary_absent".to_string());
    }
    let checks: [(&str, &str, &str); 8] = [
        (
            "stage_order",
            &f.stage_order,
            crate::publication::EXPECTED_STAGE_ORDER,
        ),
        ("measures", &f.measures, EXPECTED_MEASURES),
        (
            "review_outcomes",
            &f.review_outcomes,
            EXPECTED_REVIEW_OUTCOMES,
        ),
        ("checkpoints", &f.checkpoints, EXPECTED_CHECKPOINTS),
        (
            "maintainer_ceiling",
            &f.maintainer_ceiling,
            EXPECTED_CEILING,
        ),
        ("followup_rules", &f.followup_rules, EXPECTED_FOLLOWUP_RULES),
        ("deletion_rule", &f.deletion_rule, EXPECTED_DELETION_RULE),
        ("honesty_rules", &f.honesty_rules, EXPECTED_HONESTY_RULES),
    ];
    for (name, got, want) in checks {
        if got != want {
            return LegVerdict::Refused(format!("{name}:{got}"));
        }
    }
    LegVerdict::Verified
}

// ── leg 4 — refusal_vocabulary ───────────────────────────────────

/// The exercised case set — `exercise → expected verbatim outcome`
/// (the landed `study::` evaluators', the R5.54
/// `publication::evaluate_publication` absence guard's and
/// `AuthorityClass::parse`'s spellings, driven on exercised cases
/// only). `family_adoption_inflated` is the no-hidden-success gate
/// itself — an adoption promoted past `unavailable{not_started}`
/// refuses; `echo_*` covers the tamper refusals on the carried
/// spine; `cost_review_spend` is the $0 gate (any review spend
/// refuses); `publication_nonzero_member` exercises the
/// predecessor's own absence guard — a fabricated day-30 review
/// refuses; the `authority_*` cases are the closed-order
/// never-guesses rules.
pub const EXERCISES: [(&str, &str); 8] = [
    ("family_adoption_inflated", "adoption:demonstrated"),
    ("family_claim_present", "claim_present"),
    (
        "echo_disposition_inflated",
        "disposition:satisfied!=recomputed:reported",
    ),
    ("echo_leg_tampered", "echo_leg_vocabulary:families_inflated"),
    ("cost_review_spend", "exposure:value_review:1"),
    ("authority_followup_parse", "none"),
    ("authority_scheduled_parse", "none"),
    ("publication_nonzero_member", "day30_reviews:1"),
];

/// `evaluate_refusals` — the machinery-stands check: every
/// declared exercise produced its verbatim typed outcome. A
/// missing exercise is `inconclusive`; a wrong outcome — or a
/// `verified`/`admitted`/`report_emitted`/`published`/`reviewed`
/// produce, the guard broken — is `refused`, never reclassified.
pub fn evaluate_refusals(observed: &BTreeMap<String, String>) -> LegVerdict {
    for (exercise, expected) in EXERCISES {
        match observed.get(exercise).map(String::as_str) {
            Some(o) if o == expected => {}
            Some("verified")
            | Some("admitted")
            | Some("report_emitted")
            | Some("published")
            | Some("reviewed") => {
                return LegVerdict::Refused(format!("produced:{exercise}"));
            }
            Some(o) => return LegVerdict::Refused(format!("{exercise}:{o}")),
            None => return LegVerdict::Inconclusive(format!("exercise_absent:{exercise}")),
        }
    }
    LegVerdict::Verified
}

// ── leg 5 — predecessors_carried ─────────────────────────────────

/// The pinned predecessor packets' adverse + unmet + absent
/// members, carried verbatim — the `hh-study-report/1` packet
/// re-echoed through `check_echo` with its `study`/`adoption`/
/// unmet members asserted byte-for-byte (the recorded
/// `adoption:unavailable{not_started}` is the observed outcome —
/// never inflated to a hidden success), plus the pinned
/// `hh-publication-withheld/1` members asserted verbatim and its
/// gate/publication/claims legs re-folded `verified` through the
/// real predecessor evaluators (the frozen dated record that no
/// publication occurred — the causal premise that no review
/// dates derive).
#[derive(Debug, Clone, PartialEq)]
pub struct CarriedFacts {
    /// `check_echo` over the pinned study packet (`verified`).
    pub study_echo_verdict: String,
    /// The pinned study packet's `study` member
    /// (`unmet{efficacy_unavailable;parity_withheld}` — carried
    /// as-is).
    pub study_status: String,
    /// The study's recorded adoption-family status
    /// (`unavailable{not_started}` — the observed outcome, never
    /// promoted).
    pub study_adoption: String,
    /// `s1_unmet` (`true`).
    pub s1_unmet: bool,
    /// `s3_unmet` (`true`).
    pub s3_unmet: bool,
    /// `s4_unmet` (`true`).
    pub s4_unmet: bool,
    /// The pinned publication packet's `disposition` (`withheld`).
    pub publication_disposition: String,
    /// Its `admission` (`withheld_stage_voided`).
    pub publication_admission: String,
    /// Its `gate` (`GATE-G16`).
    pub publication_gate: String,
    /// Its `packet_id` (64-hex).
    pub publication_packet_id: String,
    /// Its `withheld` member (`true`).
    pub publication_withheld: bool,
    /// Its `stage_voided` member (`true`).
    pub publication_stage_voided: bool,
    /// Its `no_publication` member (`true` — nothing authorized,
    /// nothing performed).
    pub publication_no_publication: bool,
    /// Its `df_h9_1_open` member (`true`).
    pub publication_df_h9_1_open: bool,
    /// Its `public_acts` member (`0`).
    pub publication_public_acts: u64,
    /// The pinned publication packet's gate/publication/claims
    /// legs re-folded through the real `evaluate_gate`/
    /// `evaluate_humans`/`evaluate_publication`/`evaluate_claims`
    /// evaluators (`verified` — the predecessor record re-computes).
    pub publication_refold: String,
}

/// `evaluate_carried` — the carry check: the pinned study packet
/// re-echoes `verified` with its adverse + unmet + adoption
/// members byte-for-byte, and the pinned publication packet's
/// members equal their committed spellings with its legs
/// re-folding `verified`. A drifted, relabeled or inflated fact
/// — including an `adoption` promoted past
/// `unavailable{not_started}`, hidden success — refuses; an
/// absent member is inconclusive, never guessed.
pub fn evaluate_carried(f: &CarriedFacts) -> LegVerdict {
    if f.publication_packet_id.is_empty() || f.study_status.is_empty() {
        return LegVerdict::Inconclusive("carried_absent".to_string());
    }
    let ok = f.publication_packet_id.len() == 64
        && f.publication_packet_id
            .chars()
            .all(|c| c.is_ascii_hexdigit());
    if !ok {
        return LegVerdict::Inconclusive("packet_id_absent".to_string());
    }
    if f.study_echo_verdict != "verified" {
        return LegVerdict::Refused(format!("study_echo_verdict:{}", f.study_echo_verdict));
    }
    if f.publication_refold != "verified" {
        return LegVerdict::Refused(format!("publication_refold:{}", f.publication_refold));
    }
    let checks: [(&str, &str, &str); 5] = [
        ("study_status", &f.study_status, STUDY_STATUS),
        ("study_adoption", &f.study_adoption, ADOPTION_STATUS),
        (
            "publication_disposition",
            &f.publication_disposition,
            "withheld",
        ),
        (
            "publication_admission",
            &f.publication_admission,
            "withheld_stage_voided",
        ),
        ("publication_gate", &f.publication_gate, GATE),
    ];
    for (name, got, want) in checks {
        if got != want {
            return LegVerdict::Refused(format!("{name}:{got}"));
        }
    }
    if !(f.s1_unmet && f.s3_unmet && f.s4_unmet) {
        return LegVerdict::Refused("unmet_dropped".to_string());
    }
    if !(f.publication_withheld
        && f.publication_stage_voided
        && f.publication_no_publication
        && f.publication_df_h9_1_open)
    {
        return LegVerdict::Refused("absent_member_dropped".to_string());
    }
    if f.publication_public_acts != 0 {
        return LegVerdict::Refused(format!(
            "publication_public_acts:{}",
            f.publication_public_acts
        ));
    }
    LegVerdict::Verified
}

// ── leg 6 — observation_absent ───────────────────────────────────

/// The absent observation record, verbatim — every publication-
/// date/review-date/review/observation/funnel/outcome/limitation/
/// cost/scope-decision/recommendation/unknown/follow-up/contact/
/// raw-observation/deletion/withdrawal count a declared zero, no
/// review authorized, no observation performed, no contact or raw
/// observation created, no telemetry.
#[derive(Debug, Clone, PartialEq)]
pub struct ObservationFacts {
    /// `member → count` — every [`OBSERVATION_MEMBERS`] member
    /// declared `0`.
    pub counts: BTreeMap<String, u64>,
    /// A day-30/+90 review was authorized (`false` — GATE-G16
    /// voided the stage; HUMAN-H11 withheld).
    pub review_authorized: bool,
    /// Any observation was compiled/performed (`false`).
    pub observation_performed: bool,
    /// Any participant contact or raw observation was ever
    /// created (`false` — nothing exists to delete).
    pub contact_or_observation_created: bool,
    /// Any telemetry ran (`false`).
    pub telemetry: bool,
}

/// The twenty-two absent-observation members (frozen vocabulary) —
/// every date/review/observation/denominator/outcome/limitation/
/// cost/decision/recommendation/unknown/follow-up/contact/
/// deletion/withdrawal axis of the §8.3 day-30/+90 stage and §8.4
/// rubric, absent.
pub const OBSERVATION_MEMBERS: [&str; 22] = [
    "publication_dates_recorded",
    "day30_review_dates",
    "day90_review_dates",
    "manual_reviews_performed",
    "review_minutes_used",
    "maintainer_hours_used",
    "observations_compiled",
    "funnel_denominators_recorded",
    "useful_task_outcomes_recorded",
    "contribution_outcomes_recorded",
    "limitations_recorded",
    "cost_records",
    "assistance_missingness_records",
    "scope_decisions",
    "recommendations_issued",
    "unknowns_recorded",
    "followup_requests_sent",
    "contacts_created",
    "raw_observations_created",
    "deletion_acts",
    "earlier_withdrawals",
    "withdrawal_evidence",
];

/// `evaluate_observation` — the absent-observation check: every
/// member zero, no review authorized, no observation performed, no
/// contact or raw observation created, no telemetry. Any nonzero
/// fact is a real review's evidence — it belongs in a different
/// packet and refuses here, never a fabricated absence.
pub fn evaluate_observation(f: &ObservationFacts) -> LegVerdict {
    for member in OBSERVATION_MEMBERS {
        match f.counts.get(member) {
            Some(0) => {}
            Some(n) => return LegVerdict::Refused(format!("{member}:{n}")),
            None => return LegVerdict::Inconclusive(format!("{member}_absent")),
        }
    }
    if f.review_authorized {
        return LegVerdict::Refused("review_authorized:true".to_string());
    }
    if f.observation_performed {
        return LegVerdict::Refused("observation_performed:true".to_string());
    }
    if f.contact_or_observation_created {
        return LegVerdict::Refused("contact_or_observation_created:true".to_string());
    }
    if f.telemetry {
        return LegVerdict::Refused("telemetry:true".to_string());
    }
    LegVerdict::Verified
}

// ── leg 7 — claims_withheld ──────────────────────────────────────

/// The withheld-claims facts — DF-H9-1 stays open, the review
/// predicate set withheld, the adoption absence recorded
/// `observed_not_achieved` (an observed outcome, never hidden
/// success), the deletion obligation `none_arose` (no contact or
/// raw observation was ever created — nothing to delete, recorded
/// honestly rather than a fabricated deletion act), the study's
/// unmet record carried, and no claim of any sign is present
/// (I-08: named prerequisites stay named — a withheld stage
/// awards no claim).
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimsFacts {
    /// DF-H9-1's recorded state (`open`).
    pub df_h9_1: String,
    /// The day-30/+90 review predicate set (`withheld`).
    pub review_predicates: String,
    /// The adoption-absence record (`observed_not_achieved`).
    pub adoption_absence: String,
    /// The deletion obligation (`none_arose` — nothing was ever
    /// created; never a fabricated deletion).
    pub deletion_obligation: String,
    /// The study's unmet record is carried verbatim (`true`).
    pub study_unmet_carried: bool,
    /// A claim awarded by the withheld disposition (`false`).
    pub claim_awarded: bool,
    /// An adoption claim (`false` — the recorded status is
    /// `unavailable{not_started}`).
    pub adoption_claim: bool,
    /// A usefulness claim (`false` — no review ran).
    pub usefulness_claim: bool,
    /// A contribution claim (`false`).
    pub contribution_claim: bool,
    /// A capability claim (`false`).
    pub capability_claim: bool,
    /// A publication claim (`false` — the stage is voided).
    pub publication_claim: bool,
    /// A scientific-report claim (`false`).
    pub scientific_claim: bool,
    /// A causal-improvement claim (`false`).
    pub causal_claim: bool,
    /// A population-adoption estimate (`false` — §8.4 bars it
    /// even for a real convenience cohort).
    pub population_estimate: bool,
    /// Promotional inflation or hidden success (`false`).
    pub promotional_inflation: bool,
    /// A day-30/day-90 review claim (`false` — no review date
    /// exists).
    pub day_review_claim: bool,
}

/// `evaluate_claims` — the withheld check: DF-H9-1 `open`, the
/// review predicates `withheld`, the adoption absence
/// `observed_not_achieved`, the deletion obligation `none_arose`,
/// the unmet record carried, no claim of any sign. A claim — or
/// a discharged deferral, or adoption relabeled a success —
/// refuses, never relabels.
pub fn evaluate_claims(f: &ClaimsFacts) -> LegVerdict {
    if f.df_h9_1 != "open" {
        return LegVerdict::Refused(format!("df_h9_1:{}", f.df_h9_1));
    }
    if f.review_predicates != "withheld" {
        return LegVerdict::Refused(format!("review_predicates:{}", f.review_predicates));
    }
    if f.adoption_absence != ADOPTION_ABSENCE {
        return LegVerdict::Refused(format!("adoption_absence:{}", f.adoption_absence));
    }
    if f.deletion_obligation != "none_arose" {
        return LegVerdict::Refused(format!("deletion_obligation:{}", f.deletion_obligation));
    }
    if !f.study_unmet_carried {
        return LegVerdict::Refused("study_unmet_carried:false".to_string());
    }
    if f.claim_awarded
        || f.adoption_claim
        || f.usefulness_claim
        || f.contribution_claim
        || f.capability_claim
        || f.publication_claim
        || f.scientific_claim
        || f.causal_claim
        || f.population_estimate
        || f.promotional_inflation
        || f.day_review_claim
    {
        return LegVerdict::Refused("claim_present".to_string());
    }
    LegVerdict::Verified
}

// ── the report ───────────────────────────────────────────────────

/// The `hh-value-withheld/1` record — the honest
/// `withheld{stage_voided}`/`no_observation` disposition: nothing
/// published so no day-30/+90 review dates exist, no observation
/// was compiled, no contact or raw observation was ever created
/// (nothing to delete), the predecessor packets' adverse + unmet
/// + absent members carried verbatim (`adoption:
/// unavailable{not_started}` — observed-not-achieved, never
/// hidden success), all claims withheld and DF-H9-1 open;
/// `packet_id` is minted last over the canonical record.
#[derive(Debug, Clone, PartialEq)]
pub struct ValueWithheldReport {
    /// `H(canonical(record minus packet_id))` under
    /// `value_withheld`.
    pub packet_id: String,
    /// The record version.
    pub version: String,
    /// The gate whose decision is recorded (`GATE-G16`).
    pub gate: String,
    /// The value-review record is withheld (`true`).
    pub withheld: bool,
    /// The publication/review stage is voided this round (`true`).
    pub stage_voided: bool,
    /// No publication occurred (`true` — nothing authorized,
    /// nothing performed).
    pub no_publication: bool,
    /// No observation exists (`true` — no review dates derive, no
    /// observation was compiled).
    pub no_observation: bool,
    /// DF-H9-1 stays open (`true` — recorded honestly).
    pub df_h9_1_open: bool,
    /// Day-30/+90 reviews performed (`0`).
    pub reviews: u64,
    /// Observations compiled (`0`).
    pub observations: u64,
    /// The carried study status
    /// (`unmet{efficacy_unavailable;parity_withheld}`).
    pub study: String,
    /// The honest adoption-absence record
    /// (`observed_not_achieved` — an observed outcome, never
    /// hidden success).
    pub adoption_absence: String,
    /// The seven legs.
    pub legs: Vec<Leg>,
    /// The folded disposition.
    pub disposition: Disposition,
    /// The admission shape — `withheld_stage_voided`, never an
    /// execution or a review.
    pub admission: String,
}

impl ValueWithheldReport {
    /// `to_json` — the canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(&self.version)),
            ("packet_id", Json::str(&self.packet_id)),
            ("gate", Json::str(&self.gate)),
            ("withheld", Json::Bool(self.withheld)),
            ("stage_voided", Json::Bool(self.stage_voided)),
            ("no_publication", Json::Bool(self.no_publication)),
            ("no_observation", Json::Bool(self.no_observation)),
            ("df_h9_1_open", Json::Bool(self.df_h9_1_open)),
            ("reviews", Json::Int(self.reviews as i64)),
            ("observations", Json::Int(self.observations as i64)),
            ("study", Json::str(&self.study)),
            ("adoption_absence", Json::str(&self.adoption_absence)),
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

/// `assemble(legs)` — fold the seven leg verdicts into the
/// disposition and mint `packet_id` last over the canonical
/// record. Every leg name must be a `LEGS` member, unique and
/// complete — a malformed leg set is a `refused` assembly, never
/// a partial packet.
pub fn assemble(legs: Vec<Leg>) -> Result<ValueWithheldReport, String> {
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
    let mut out = ValueWithheldReport {
        packet_id: String::new(),
        version: VERSION.to_string(),
        gate: GATE.to_string(),
        withheld: true,
        stage_voided: true,
        no_publication: true,
        no_observation: true,
        df_h9_1_open: true,
        reviews: 0,
        observations: 0,
        study: STUDY_STATUS.to_string(),
        adoption_absence: ADOPTION_ABSENCE.to_string(),
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
