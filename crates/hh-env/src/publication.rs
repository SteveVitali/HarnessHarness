//! `hh-publication-withheld/1` — the withheld public availability/
//! claims/corrections disposition record (R5.54; GATE-G16 PASSED —
//! **out-of-scope disposition**; evidence spec §8.3's publication
//! stages and report spine — withheld this round: no public acts/
//! contacts/content were authorized, HUMAN-H10/H11 are recorded
//! WITHHELD and nothing published — DF-H9-1 stays open; §8.3: "No
//! fixture, hash, tag or distribution creates a scientific/public
//! evidence badge").
//!
//! Records-in only: every leg evaluator folds *supplied* facts —
//! the gate's verbatim decision and the withheld human records, the
//! pinned-input digest counts, the §8.3 spine/claim/correction
//! vocabulary the landed `study`/rights/`AuthorityClass` machinery
//! and the frozen §8.3 decls declare, the verbatim typed outcomes
//! the exercised refusal/inflation cases produced, the committed
//! `hh-study-report/1` packet's adverse + unmet members carried
//! verbatim (never inflated), the absent publication counts and
//! the withheld-claims fold — and never manufactures an artifact,
//! a URL, a channel act, a correction, a date or a claim.
//! `withheld` is the honest terminal state of the voided stage; a
//! definitive failure folds `failed`, an undetermined leg
//! `inconclusive` — never an invented publication record.

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
pub const VERSION: &str = "hh-publication-withheld/1";
const DOMAIN: &str = "publication_withheld";

/// The gate whose out-of-scope decision this packet records.
pub const GATE: &str = "GATE-G16";

/// The committed study report's honest status, carried verbatim —
/// never inflated (REQ-G-209: adverse results/corrections retained).
pub const STUDY_STATUS: &str = "unmet{efficacy_unavailable;parity_withheld}";

/// The seven legs (frozen vocabulary — the withheld scope).
pub const LEGS: [&str; 7] = [
    "gate_readout",
    "pins_frozen",
    "spine_vocabulary",
    "refusal_vocabulary",
    "study_carried",
    "publication_absent",
    "claims_withheld",
];

// ── leg 1 — gate_readout ─────────────────────────────────────────
//
// GATE-G16's verbatim decision facts are the R5.50 contract — the
// `plugin::GateFacts`/`evaluate_gate` predecessor is consumed
// unchanged (one gate, one evaluator, never a second copy). The
// withheld human records ride the same leg.

/// The withheld human records — HUMAN-H10/H11 read `withheld` (no
/// acts approved, none performed; no day-30/+90 dates exist — the
/// calendar derives from actual publication, which did not occur).
#[derive(Debug, Clone, PartialEq)]
pub struct HumanFacts {
    /// HUMAN-H10's recorded readout status (`withheld`).
    pub h10_status: String,
    /// HUMAN-H11's recorded readout status (`withheld`).
    pub h11_status: String,
    /// Publication/outreach acts approved at GATE-G16 (`none`).
    pub acts_approved: String,
}

/// `evaluate_humans` — the withheld-records check: both readouts
/// `withheld`, no acts approved. A `performed`/`approved` fact
/// refuses — an act belongs in a different packet.
pub fn evaluate_humans(f: &HumanFacts) -> LegVerdict {
    if f.h10_status != "withheld" {
        return LegVerdict::Refused(format!("h10_status:{}", f.h10_status));
    }
    if f.h11_status != "withheld" {
        return LegVerdict::Refused(format!("h11_status:{}", f.h11_status));
    }
    if f.acts_approved != "none" {
        return LegVerdict::Refused(format!("acts_approved:{}", f.acts_approved));
    }
    LegVerdict::Verified
}

// ── leg 2 — pins_frozen ──────────────────────────────────────────

/// The pinned-input count — the nine H8 hash-verified upstream
/// artifacts plus the committed `hh-study-report/1` packet (a
/// frozen dated report — the claim→receipt spine that stays
/// internal, the adverse + unmet results this record carries;
/// pin 10, declared never promoted). The GATE-G16/HUMAN-H10/
/// HUMAN-H11 readouts are echoed verbatim as leg facts, never
/// pinned — living append-only records.
pub const PINS_TOTAL: u64 = 10;

/// The pin-verification facts — `check_pins` over the ten
/// declared inputs, plus the study-report digest echoed verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct PinsFacts {
    /// The pinned-input count (`10` — [`PINS_TOTAL`]).
    pub pins: u64,
    /// The `check_pins` verdict name (`verified`).
    pub pins_verdict: String,
    /// The committed study-report sha256 (64-hex, echoed — never
    /// hard-coded into the evaluator).
    pub study_sha256: String,
}

/// `evaluate_pins` — the pin check: ten inputs, `check_pins`
/// verified and a 64-hex spine digest supplied. A drifted pin
/// refuses; an unreadable pin is inconclusive upstream — an
/// empty/short digest here is inconclusive, never guessed.
pub fn evaluate_pins(f: &PinsFacts) -> LegVerdict {
    if f.pins != PINS_TOTAL {
        return LegVerdict::Refused(format!("pins:{}", f.pins));
    }
    if f.pins_verdict != "verified" {
        return LegVerdict::Refused(format!("pins_verdict:{}", f.pins_verdict));
    }
    let ok = f.study_sha256.len() == 64 && f.study_sha256.chars().all(|c| c.is_ascii_hexdigit());
    if !ok {
        return LegVerdict::Inconclusive("study_sha256_absent".to_string());
    }
    LegVerdict::Verified
}

// ── leg 3 — spine_vocabulary ─────────────────────────────────────

/// The committed study-report schema (`study::VERSION`).
pub const EXPECTED_STUDY_VERSION: &str = "hh-study-report/1";
/// The ten-leg study-report vocabulary, verbatim.
pub const EXPECTED_STUDY_LEGS: &str = "freeze_identities,pre_flight,useful_s1,ops_live,activation,pilot,parity,confirmation,families,costs";
/// The `study::Disposition` closed codec, verbatim — `reported`
/// only when the recompute matched (never `satisfied`/`complete`).
pub const EXPECTED_STUDY_DISPOSITIONS: &str = "reported,failed,inconclusive";
/// The nine-family `study::REQUIRED_FAMILIES` separation, verbatim —
/// the adverse `task_truth:failed{s1_useful}` and the unavailable
/// families retained as-is (REQ-D-009/AC-3-2; REQ-G-209 adverse
/// retention).
pub const EXPECTED_STUDY_FAMILIES: &str = "efficacy:unavailable{confirmation_tranche_absent},task_mixture:descriptive{pilot_recomputed},sharp_null:unavailable{confirmation_tranche_absent},parity:withheld{n_declared:0},mechanism:executed{activation,pilot},engineering:executed{ops_live;backend_verified},task_truth:failed{s1_useful},operations:executed{ops_live},adoption:unavailable{not_started}";
/// The landed rights correction/supersession/freshness record
/// schemas, verbatim (`hh-bundle::rights`) — REQ-G-202's
/// version/loss/correction spine and REQ-G-209's
/// supersession/freshness vocabulary stand declared, never
/// performed.
pub const EXPECTED_CORRECTION_SCHEMAS: &str =
    "rights-record/1,handoff-report/1,rights-supersession/1,rights-tombstone/1";
/// The closed `AuthorityClass` order, verbatim — the claim-limit
/// vocabulary: `delegate` may propose but never confer; a label
/// never guesses its class.
pub const EXPECTED_AUTHORITY_CLASSES: &str =
    "unverified,external,environment,delegate,principal,definition,kernel";
/// The fixed §8.3 publication stage order, verbatim.
pub const EXPECTED_STAGE_ORDER: &str = "internal_readiness,outside_clean_verify_recompute,formative_reviews_and_repair_recheck,operator_capability_publication_decision,separately_gated_scientific_report,manual_day30_day90_value_review";
/// The §8.3 release-note spine fields, verbatim — "A release note
/// states audience/job, change/revision, try/inspect prerequisites,
/// earned/unavailable claims, evidence, report/contribute route
/// and correction owner/date."
pub const EXPECTED_RELEASE_NOTE_FIELDS: &str = "audience,job,change_revision,try_inspect_prerequisites,earned_unavailable_claims,evidence,report_contribute_route,correction_owner,correction_date";
/// The §8.3 report-spine field groups, verbatim — "A report states
/// question/decision/scope/class/date and claim→receipt; protocol/
/// deviations/population/arms/assignment/oracle/pins/caps/admission;
/// planned→attempt→grade→analysis counts and adverse states;
/// effects/uncertainty/margins/families/cost or operational
/// verdicts; procedure cards/outside reports/assistance/access/
/// losses; limitations/unsupported conclusions/corrections/rights/
/// retention."
pub const EXPECTED_REPORT_SPINE_FIELDS: &str = "question_decision_scope_class_date,claim_to_receipt,protocol_deviations_population_arms_assignment_oracle_pins_caps_admission,planned_attempt_grade_analysis_counts_and_adverse_states,effects_uncertainty_margins_families_cost_or_operational_verdicts,procedure_cards_outside_reports_assistance_access_losses,limitations_unsupported_conclusions_corrections_rights_retention";
/// The §8.3 channel rules, verbatim — "GitHub docs/releases/issues/
/// PRs first, then authorized direct review. At most one optional
/// wider channel after current-rule review and operator approval…
/// No report-only launch, scraped lists/bulk mail, vote
/// solicitation, promotional issues or implied endorsement… No
/// outreach or automation is authorized here."
pub const EXPECTED_CHANNEL_RULES: &str = "github_first,at_most_one_wider_channel_after_current_rule_review_and_operator_approval,no_report_only_launch,no_scraped_lists_or_bulk_mail,no_vote_solicitation,no_promotional_issues_or_implied_endorsement,upstream_engagement_needs_concrete_failing_fixture_and_separate_authorization,no_outreach_or_automation_authorized";

/// The §8.3 spine/claim/correction vocabulary facts — the declared
/// vocabulary the landed machinery and the frozen §8.3 decls carry
/// (read, never re-declared: the battery supplies what the
/// machinery actually says).
#[derive(Debug, Clone, PartialEq)]
pub struct SpineFacts {
    /// `study::VERSION` (`hh-study-report/1`).
    pub study_version: String,
    /// `study::LEGS` comma-joined (ten legs).
    pub study_legs: String,
    /// The `study::Disposition` spellings comma-joined.
    pub study_dispositions: String,
    /// `study::REQUIRED_FAMILIES` `name:status` comma-joined.
    pub study_families: String,
    /// The four rights record schemas comma-joined.
    pub correction_schemas: String,
    /// `AuthorityClass::ALL` `as_str` comma-joined (seven).
    pub authority_classes: String,
    /// `AuthorityClass::parse` refused every non-class spelling on
    /// supplied cases (`true` — the closed order never guesses).
    pub authority_parse_never_guesses: bool,
    /// The fixed §8.3 stage order.
    pub stage_order: String,
    /// The release-note spine fields.
    pub release_note_fields: String,
    /// The report-spine field groups.
    pub report_spine_fields: String,
    /// The channel rules.
    pub channel_rules: String,
}

/// `evaluate_spine` — the declared-vocabulary check: the supplied
/// facts equal the landed decls verbatim. A drifted fact is a
/// different vocabulary — refused, never relabelled; an absent
/// vocabulary is inconclusive.
pub fn evaluate_spine(f: &SpineFacts) -> LegVerdict {
    if f.study_version.is_empty() {
        return LegVerdict::Inconclusive("vocabulary_absent".to_string());
    }
    let checks: [(&str, &str, &str); 10] = [
        ("study_version", &f.study_version, EXPECTED_STUDY_VERSION),
        ("study_legs", &f.study_legs, EXPECTED_STUDY_LEGS),
        (
            "study_dispositions",
            &f.study_dispositions,
            EXPECTED_STUDY_DISPOSITIONS,
        ),
        ("study_families", &f.study_families, EXPECTED_STUDY_FAMILIES),
        (
            "correction_schemas",
            &f.correction_schemas,
            EXPECTED_CORRECTION_SCHEMAS,
        ),
        (
            "authority_classes",
            &f.authority_classes,
            EXPECTED_AUTHORITY_CLASSES,
        ),
        ("stage_order", &f.stage_order, EXPECTED_STAGE_ORDER),
        (
            "release_note_fields",
            &f.release_note_fields,
            EXPECTED_RELEASE_NOTE_FIELDS,
        ),
        (
            "report_spine_fields",
            &f.report_spine_fields,
            EXPECTED_REPORT_SPINE_FIELDS,
        ),
        ("channel_rules", &f.channel_rules, EXPECTED_CHANNEL_RULES),
    ];
    for (name, got, want) in checks {
        if got != want {
            return LegVerdict::Refused(format!("{name}:{got}"));
        }
    }
    if !f.authority_parse_never_guesses {
        return LegVerdict::Refused("authority_parse_guesses".to_string());
    }
    LegVerdict::Verified
}

// ── leg 4 — refusal_vocabulary ───────────────────────────────────

/// The exercised case set — `exercise → expected verbatim outcome`
/// (the landed `study::` evaluators' and `AuthorityClass::parse`'s
/// spellings, driven on exercised cases only). The
/// `family_*`/`echo_*` cases are the no-promotional-inflation and
/// no-tamper gates themselves (an inflated or relabeled spine
/// refuses); `useful_*`/`parity_*` cover relabel/re-sign refusals
/// on the carried stage records; `cost_publication_spend` is the
/// $0 gate (any spend refuses); the `authority_*` cases are the
/// closed-order never-guesses rules.
pub const EXERCISES: [(&str, &str); 10] = [
    ("family_efficacy_inflated", "efficacy:demonstrated"),
    ("family_claim_present", "claim_present"),
    (
        "echo_disposition_inflated",
        "disposition:satisfied!=recomputed:reported",
    ),
    ("echo_leg_tampered", "echo_leg_vocabulary:families_inflated"),
    ("useful_version_relabel", "version:hh-study-report/1"),
    ("useful_executed_relabel", "disposition:executed"),
    ("parity_gate_resign", "gate:GATE-G16"),
    ("cost_publication_spend", "exposure:publication:1"),
    ("authority_promotional_parse", "none"),
    ("authority_imported_parse", "none"),
];

/// `evaluate_refusals` — the machinery-stands check: every
/// declared exercise produced its verbatim typed outcome. A
/// missing exercise is `inconclusive`; a wrong outcome — or a
/// `verified`/`admitted`/`report_emitted`/`published` produce, the
/// guard broken — is `refused`, never reclassified.
pub fn evaluate_refusals(observed: &BTreeMap<String, String>) -> LegVerdict {
    for (exercise, expected) in EXERCISES {
        match observed.get(exercise).map(String::as_str) {
            Some(o) if o == expected => {}
            Some("verified") | Some("admitted") | Some("report_emitted") | Some("published") => {
                return LegVerdict::Refused(format!("produced:{exercise}"));
            }
            Some(o) => return LegVerdict::Refused(format!("{exercise}:{o}")),
            None => return LegVerdict::Inconclusive(format!("exercise_absent:{exercise}")),
        }
    }
    LegVerdict::Verified
}

// ── leg 5 — study_carried ────────────────────────────────────────

/// The committed `hh-study-report/1` packet's adverse + unmet
/// members, carried verbatim — the pinned packet re-echoed through
/// `check_echo` and its members asserted byte-for-byte (REQ-G-202:
/// the claim→receipt audit retains adverse/unknown entries;
/// REQ-G-209: adverse results retained — never inflated).
#[derive(Debug, Clone, PartialEq)]
pub struct CarriedFacts {
    /// `check_echo` over the pinned packet (`verified`).
    pub echo_verdict: String,
    /// The pinned packet's `version` (`hh-study-report/1`).
    pub version: String,
    /// Its recorded disposition (`reported`).
    pub disposition: String,
    /// Its admission member (`recompute_only`).
    pub admission: String,
    /// Its `packet_id` (64-hex).
    pub packet_id: String,
    /// Its `study` member (`unmet{efficacy_unavailable;
    /// parity_withheld}` — carried as-is).
    pub study_status: String,
    /// `s1_unmet` (`true`).
    pub s1_unmet: bool,
    /// `s3_unmet` (`true`).
    pub s3_unmet: bool,
    /// `s4_unmet` (`true`).
    pub s4_unmet: bool,
    /// `subjects_first` (`12`).
    pub subjects_first: u64,
    /// `exposure_micros` (`0` — USD $0.00).
    pub exposure_micros: i64,
    /// The `useful_s1` leg's recorded disposition (`failed` — the
    /// adverse result retained verbatim).
    pub s1_disposition: String,
    /// Its acceptance (`withheld{no_independent_correct_artifact}`).
    pub s1_acceptance: String,
    /// The `parity` leg's status (`withheld{n_declared:0}`).
    pub parity_status: String,
    /// The `confirmation` leg's status
    /// (`withheld{tranche_absent}`).
    pub confirmation_status: String,
}

/// `evaluate_carried` — the carry check: the pinned report
/// re-echoes `verified` and every adverse + unmet member equals
/// its committed spelling. A drifted, relabeled or inflated fact
/// refuses; an absent member is inconclusive — never guessed.
pub fn evaluate_carried(f: &CarriedFacts) -> LegVerdict {
    if f.packet_id.is_empty() || f.version.is_empty() {
        return LegVerdict::Inconclusive("carried_absent".to_string());
    }
    let ok = f.packet_id.len() == 64 && f.packet_id.chars().all(|c| c.is_ascii_hexdigit());
    if !ok {
        return LegVerdict::Inconclusive("packet_id_absent".to_string());
    }
    if f.echo_verdict != "verified" {
        return LegVerdict::Refused(format!("echo_verdict:{}", f.echo_verdict));
    }
    let checks: [(&str, &str, &str); 7] = [
        ("version", &f.version, EXPECTED_STUDY_VERSION),
        ("disposition", &f.disposition, "reported"),
        ("admission", &f.admission, "recompute_only"),
        ("study_status", &f.study_status, STUDY_STATUS),
        ("s1_disposition", &f.s1_disposition, "failed"),
        (
            "s1_acceptance",
            &f.s1_acceptance,
            "withheld{no_independent_correct_artifact}",
        ),
        ("parity_status", &f.parity_status, "withheld{n_declared:0}"),
    ];
    for (name, got, want) in checks {
        if got != want {
            return LegVerdict::Refused(format!("{name}:{got}"));
        }
    }
    if f.confirmation_status != "withheld{tranche_absent}" {
        return LegVerdict::Refused(format!("confirmation_status:{}", f.confirmation_status));
    }
    if !(f.s1_unmet && f.s3_unmet && f.s4_unmet) {
        return LegVerdict::Refused("unmet_dropped".to_string());
    }
    if f.subjects_first != 12 {
        return LegVerdict::Refused(format!("subjects_first:{}", f.subjects_first));
    }
    if f.exposure_micros != 0 {
        return LegVerdict::Refused(format!("exposure_micros:{}", f.exposure_micros));
    }
    LegVerdict::Verified
}

// ── leg 6 — publication_absent ───────────────────────────────────

/// The absent publication record, verbatim — every artifact/URL/
/// version/channel/contact/correction/date count a declared zero,
/// no publication authorized, no public act, no telemetry.
#[derive(Debug, Clone, PartialEq)]
pub struct PublicationFacts {
    /// `member → count` — every [`PUBLICATION_MEMBERS`] member
    /// declared `0`.
    pub counts: BTreeMap<String, u64>,
    /// Any public act was authorized (`false` — GATE-G16 admitted
    /// none).
    pub publication_authorized: bool,
    /// Any public act was performed (`false` — HUMAN-H10
    /// withheld).
    pub public_act_performed: bool,
    /// Any telemetry ran (`false`).
    pub telemetry: bool,
}

/// The sixteen absent-publication members (frozen vocabulary) —
/// every artifact/URL/version/report/channel/engagement/contact/
/// outreach/correction/supersession/freshness/review/date axis of
/// §8.3, absent.
pub const PUBLICATION_MEMBERS: [&str; 16] = [
    "public_artifacts",
    "urls_minted",
    "versions_released",
    "release_notes_published",
    "scientific_reports_published",
    "channels_used",
    "wider_channels_authorized",
    "upstream_engagements",
    "contacts_made",
    "outreach_acts",
    "corrections_published",
    "supersessions_published",
    "freshness_updates_published",
    "day30_reviews",
    "day90_reviews",
    "publication_dates_recorded",
];

/// `evaluate_publication` — the absent-publication check: every
/// member zero, no authorization, no act, no telemetry. Any nonzero
/// fact is a real publication's evidence — it belongs in a
/// different packet and refuses here, never a fabricated absence.
pub fn evaluate_publication(f: &PublicationFacts) -> LegVerdict {
    for member in PUBLICATION_MEMBERS {
        match f.counts.get(member) {
            Some(0) => {}
            Some(n) => return LegVerdict::Refused(format!("{member}:{n}")),
            None => return LegVerdict::Inconclusive(format!("{member}_absent")),
        }
    }
    if f.publication_authorized {
        return LegVerdict::Refused("publication_authorized:true".to_string());
    }
    if f.public_act_performed {
        return LegVerdict::Refused("public_act_performed:true".to_string());
    }
    if f.telemetry {
        return LegVerdict::Refused("telemetry:true".to_string());
    }
    LegVerdict::Verified
}

// ── leg 7 — claims_withheld ──────────────────────────────────────

/// The withheld-claims facts — DF-H9-1 stays open, the
/// publication/participant predicate sets withheld, the spine
/// stays internal, the study's unmet record is carried, and no
/// claim of any sign is present (§8.3: "No fixture, hash, tag or
/// distribution creates a scientific/public evidence badge";
/// I-08: named prerequisites stay named — a withheld stage awards
/// no claim).
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimsFacts {
    /// DF-H9-1's recorded state (`open`).
    pub df_h9_1: String,
    /// The publication predicate set (`withheld`).
    pub publication_predicates: String,
    /// The participant/outreach predicate set (`withheld`).
    pub participant_predicates: String,
    /// The claim→receipt spine's location (`internal`).
    pub spine: String,
    /// The study's unmet record is carried verbatim (`true`).
    pub study_unmet_carried: bool,
    /// A claim awarded by the withheld disposition (`false`).
    pub claim_awarded: bool,
    /// A capability claim (`false`).
    pub capability_claim: bool,
    /// A publication claim (`false`).
    pub publication_claim: bool,
    /// A scientific-report claim (`false`).
    pub scientific_claim: bool,
    /// An adoption claim (`false`).
    pub adoption_claim: bool,
    /// A portability claim (`false`).
    pub portability_claim: bool,
    /// An independence claim (`false`).
    pub independence_claim: bool,
    /// An efficacy claim (`false` — the study reports
    /// `unavailable{confirmation_tranche_absent}`).
    pub efficacy_claim: bool,
    /// Promotional inflation of any carried result (`false`).
    pub promotional_inflation: bool,
    /// A published-correction claim (`false`).
    pub correction_claim: bool,
    /// A day-30/day-90 review claim (`false` — no publication
    /// date exists).
    pub day_review_claim: bool,
}

/// `evaluate_claims` — the withheld check: DF-H9-1 `open`, the
/// predicate sets `withheld`, spine `internal`, the unmet record
/// carried, no claim of any sign. A claim — or a discharged
/// deferral — refuses, never relabels.
pub fn evaluate_claims(f: &ClaimsFacts) -> LegVerdict {
    if f.df_h9_1 != "open" {
        return LegVerdict::Refused(format!("df_h9_1:{}", f.df_h9_1));
    }
    if f.publication_predicates != "withheld" {
        return LegVerdict::Refused(format!(
            "publication_predicates:{}",
            f.publication_predicates
        ));
    }
    if f.participant_predicates != "withheld" {
        return LegVerdict::Refused(format!(
            "participant_predicates:{}",
            f.participant_predicates
        ));
    }
    if f.spine != "internal" {
        return LegVerdict::Refused(format!("spine:{}", f.spine));
    }
    if !f.study_unmet_carried {
        return LegVerdict::Refused("study_unmet_carried:false".to_string());
    }
    if f.claim_awarded
        || f.capability_claim
        || f.publication_claim
        || f.scientific_claim
        || f.adoption_claim
        || f.portability_claim
        || f.independence_claim
        || f.efficacy_claim
        || f.promotional_inflation
        || f.correction_claim
        || f.day_review_claim
    {
        return LegVerdict::Refused("claim_present".to_string());
    }
    LegVerdict::Verified
}

// ── the report ───────────────────────────────────────────────────

/// The `hh-publication-withheld/1` record — the honest
/// `withheld{stage_voided}`/`no_publication` disposition: no
/// public acts were authorized, nothing published, every artifact/
/// URL/channel/correction/date phase absent, the adverse + unmet
/// study results carried verbatim, all claims withheld and
/// DF-H9-1 open; `packet_id` is minted last over the canonical
/// record.
#[derive(Debug, Clone, PartialEq)]
pub struct PublicationWithheldReport {
    /// `H(canonical(record minus packet_id))` under
    /// `publication_withheld`.
    pub packet_id: String,
    /// The record version.
    pub version: String,
    /// The gate whose decision is recorded (`GATE-G16`).
    pub gate: String,
    /// The publication record is withheld (`true`).
    pub withheld: bool,
    /// The publication stage is voided this round (`true`).
    pub stage_voided: bool,
    /// No publication occurred (`true` — nothing authorized,
    /// nothing performed).
    pub no_publication: bool,
    /// DF-H9-1 stays open (`true` — recorded honestly).
    pub df_h9_1_open: bool,
    /// Public acts performed (`0`).
    pub public_acts: u64,
    /// The carried study status
    /// (`unmet{efficacy_unavailable;parity_withheld}`).
    pub study: String,
    /// The seven legs.
    pub legs: Vec<Leg>,
    /// The folded disposition.
    pub disposition: Disposition,
    /// The admission shape — `withheld_stage_voided`, never an
    /// execution or a publication.
    pub admission: String,
}

impl PublicationWithheldReport {
    /// `to_json` — the canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(&self.version)),
            ("packet_id", Json::str(&self.packet_id)),
            ("gate", Json::str(&self.gate)),
            ("withheld", Json::Bool(self.withheld)),
            ("stage_voided", Json::Bool(self.stage_voided)),
            ("no_publication", Json::Bool(self.no_publication)),
            ("df_h9_1_open", Json::Bool(self.df_h9_1_open)),
            ("public_acts", Json::Int(self.public_acts as i64)),
            ("study", Json::str(&self.study)),
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
pub fn assemble(legs: Vec<Leg>) -> Result<PublicationWithheldReport, String> {
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
    let mut out = PublicationWithheldReport {
        packet_id: String::new(),
        version: VERSION.to_string(),
        gate: GATE.to_string(),
        withheld: true,
        stage_voided: true,
        no_publication: true,
        df_h9_1_open: true,
        public_acts: 0,
        study: STUDY_STATUS.to_string(),
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
