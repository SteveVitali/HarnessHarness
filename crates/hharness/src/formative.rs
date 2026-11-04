//! `hh-formative-withheld/1` — the withheld formative evaluator/
//! contributor observation disposition record (R5.53; GATE-G15
//! PASSED — **out-of-scope disposition**; evidence spec §8.4's
//! formative cohort — withheld this round: no cohort exists under
//! withheld HUMAN-H9 — DF-H9-1 stays open; §8.4: "Convenience
//! volunteers cannot estimate population adoption or causal
//! improvement" — still less a voided cohort).
//!
//! Records-in only: every leg evaluator folds *supplied* facts —
//! the gate's verbatim decision, the pinned-input digest counts,
//! the §7/§8 formative-surface vocabulary the landed `examples`/
//! `recipe`/checker machinery declares, the verbatim typed
//! outcomes the exercised refusal/green cases produced, the
//! absent cohort counts and the withheld-claims fold — and never
//! manufactures a participant, a session, an observation or a
//! claim. `withheld` is the honest terminal state of the voided
//! stage; a definitive failure folds `failed`, an undetermined
//! leg `inconclusive` — never an invented session.
//!
//! Placement: this module lands in the `hharness` facade (not
//! `hh-env`) — the exercised R5.45/R5.28 surfaces (`examples`,
//! `recipe`) live here and `hh-env` cannot reach them without a
//! dev-graph cycle (facade→env is the shipping edge). The packet
//! contract — closed legs, records-in evaluators, the
//! `Leg`/`LegVerdict` verdict codec, the `confirm::Disposition`
//! fold, `packet_id` minted last — is the R5.48–R5.52 pattern
//! consumed through `hharness_env` unchanged.

use std::collections::BTreeMap;

use hharness_env::preflight::{Leg, LegVerdict};
use hharness_identity::idp::idp_digest;
use hharness_wire::json::Json;

/// The packet disposition — the `withheld | failed | inconclusive`
/// fold is the R5.48 contract consumed unchanged
/// (`confirm::Disposition` re-exported: one closed vocabulary,
/// never a second implementation).
pub use hharness_env::confirm::Disposition;

/// The record version.
pub const VERSION: &str = "hh-formative-withheld/1";
const DOMAIN: &str = "formative_withheld";

/// The gate whose out-of-scope decision this packet records.
pub const GATE: &str = "GATE-G15";

/// The six legs (frozen vocabulary — the withheld scope).
pub const LEGS: [&str; 6] = [
    "gate_readout",
    "pins_frozen",
    "surface_vocabulary",
    "refusal_vocabulary",
    "cohort_absent",
    "claims_withheld",
];

// ── leg 1 — gate_readout ─────────────────────────────────────────
//
// GATE-G15's verbatim decision facts are the R5.50 contract — the
// `plugin::GateFacts`/`evaluate_gate` predecessor is consumed
// unchanged (one gate, one evaluator, never a second copy).

// ── leg 2 — pins_frozen ──────────────────────────────────────────

/// The pinned-input count — the nine H8 hash-verified upstream
/// artifacts plus the committed `scripts/examples/ex-03.json` and
/// `ex-04.json` family cards (the cleared checked-task artifacts
/// the cohort's branches would have exercised — pins 10–11,
/// completing the four-card pin set with H8's ex-01/ex-02;
/// declared, never consumed by a participant).
pub const PINS_TOTAL: u64 = 11;

/// The pin-verification facts — `check_pins` over the eleven
/// declared inputs, plus the two family-card digests echoed
/// verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct PinsFacts {
    /// The pinned-input count (`11`).
    pub pins: u64,
    /// The `check_pins` verdict name (`verified`).
    pub pins_verdict: String,
    /// The committed `ex-03.json` sha256 (64-hex, echoed — never
    /// hard-coded into the evaluator).
    pub ex03_sha256: String,
    /// The committed `ex-04.json` sha256 (64-hex, echoed).
    pub ex04_sha256: String,
}

/// `evaluate_pins` — the pin check: eleven inputs, `check_pins`
/// verified and two 64-hex card digests supplied. A drifted pin
/// refuses; an unreadable pin is inconclusive upstream — an
/// empty/short digest here is inconclusive, never guessed.
pub fn evaluate_pins(f: &PinsFacts) -> LegVerdict {
    if f.pins != PINS_TOTAL {
        return LegVerdict::Refused(format!("pins:{}", f.pins));
    }
    if f.pins_verdict != "verified" {
        return LegVerdict::Refused(format!("pins_verdict:{}", f.pins_verdict));
    }
    for d in [&f.ex03_sha256, &f.ex04_sha256] {
        let ok = d.len() == 64 && d.chars().all(|c| c.is_ascii_hexdigit());
        if !ok {
            return LegVerdict::Inconclusive("card_sha256_absent".to_string());
        }
    }
    LegVerdict::Verified
}

// ── leg 3 — surface_vocabulary ───────────────────────────────────

/// The committed example-card schema label (`examples::CARD_SCHEMA`).
pub const EXPECTED_CARD_SCHEMA: &str = "example-card/1";
/// The EX-02 fixture schema label (`examples::EX02_FIXTURE_SCHEMA`).
pub const EXPECTED_FIXTURE_SCHEMA: &str = "ex-02-fixture/1";
/// The card runner's version (`examples::RUNNER_VERSION`).
pub const EXPECTED_RUNNER_VERSION: &str = "hh-examples/1";
/// The recipe's version (`recipe::RECIPE_VERSION`).
pub const EXPECTED_RECIPE_VERSION: &str = "hh-recipe/1";
/// The four-family closed case set, verbatim — the only `case`
/// spellings `ExampleCard::from_json` admits.
pub const EXPECTED_FAMILIES: &str = "ex-01,ex-02,ex-03,ex-04";
/// The committed family-card count — every committed card decodes.
pub const EXPECTED_CARDS_DECODED: u64 = 4;
/// The closed `examples::code` refusal set, verbatim — the six
/// `ExampleError::Refused` codes comma-joined in declaration order.
pub const EXPECTED_EXAMPLE_CODES: &str =
    "stale_pin,unknown_case,unsupported_shape,missing_field,type_mismatch,check_missing";
/// The closed `recipe::code` refusal set, verbatim — the seven
/// `RecipeError::Refused` codes comma-joined in declaration order.
pub const EXPECTED_RECIPE_CODES: &str = "unsupported_shape,missing_field,type_mismatch,execution_incomplete,no_submission,grade_unspecified,drain_stalled";
/// The `CheckRecord` verdict vocabulary, verbatim — `pass` only
/// when every check is `pass | expected_refusal` (an expected
/// refusal recorded verbatim never counts as a pass of the
/// refused operation).
pub const EXPECTED_CHECK_VERDICTS: &str = "pass,fail,expected_refusal";
/// The twelve `check-contributing.sh` check ids, verbatim — the
/// §8.1 contribution/failure-report routes' static checker
/// vocabulary in emission order.
pub const EXPECTED_CONTRIB_CHECK_IDS: &str = "lanes,no_run,receipt,privacy,verifyfixed,no_ritual,gated,form_fields,commands_real,blocked,codeowners,links";
/// The seven §8.1 contribution lanes, verbatim.
pub const EXPECTED_CONTRIB_LANES: &str =
    "no-run,receipt,attach-evidence,propose,verifyfixed,gated-routes,starter-map";
/// The six `check-trace.sh` leg ids, verbatim — the §7 reader-
/// route/evidence surfaces' static checker vocabulary.
pub const EXPECTED_TRACE_LEGS: &str = "excerpts,packets,families,honesty,links,nav";
/// The frozen §8.4 comprehension rubric — the four items,
/// verbatim clauses ` | `-joined (rubric/version/scorer is frozen
/// *before* sessions — a declaration surface, never a score).
pub const EXPECTED_RUBRIC_ITEMS: &str = "runtime executes/records while Lab plans/analyzes | terminal success differs from independent grade | verify/recompute neither solves a fresh task nor proves efficacy | named rights/access/limitations constrain claims";
/// The frozen §8.4 measure/denominator table, verbatim — the
/// seven measures comma-joined in table order.
pub const EXPECTED_MEASURES: &str = "Visibility,Access,Useful task,Understanding/procedure,Independent own-task use,Repeat use,Contribution";
/// The §8.4 session shape, verbatim — ≈5 consent/context + 40
/// participant-led tasks + 15 interpretation/debrief ≤60 total;
/// optional consented recheck ≤15 per affected participant.
pub const EXPECTED_SESSION_SHAPE: &str =
    "consent_context:5,tasks:40,debrief:15,total_cap:60,recheck_cap:15";
/// The §8.4 cohort shape, verbatim — 4–6 technical evaluators +
/// one consenting outside verifier, overlap allowed counted once,
/// ≥2 new to HarnessHarness, receipt-first + missing-live-
/// prerequisite branches.
pub const EXPECTED_COHORT_SHAPE: &str =
    "evaluators:4-6,verifier:1,overlap_counted_once,new_to_hh:>=2,branches:receipt_first+missing_live_prereq";

/// The §7/§8 formative-surface vocabulary facts — the declared
/// vocabulary the landed machinery carries (read, never
/// re-declared: the battery supplies what the machinery/scripts
/// actually say).
#[derive(Debug, Clone, PartialEq)]
pub struct VocabularyFacts {
    /// `examples::CARD_SCHEMA` (`example-card/1`).
    pub card_schema: String,
    /// `examples::EX02_FIXTURE_SCHEMA` (`ex-02-fixture/1`).
    pub fixture_schema: String,
    /// `examples::RUNNER_VERSION` (`hh-examples/1`).
    pub runner_version: String,
    /// `recipe::RECIPE_VERSION` (`hh-recipe/1`).
    pub recipe_version: String,
    /// The four decoded family cases comma-joined
    /// (`ex-01,ex-02,ex-03,ex-04`).
    pub families: String,
    /// Committed cards decoded via `ExampleCard::load` (`4`).
    pub cards_decoded: u64,
    /// The closed `examples::code` refusal set comma-joined (six).
    pub example_codes: String,
    /// The closed `recipe::code` refusal set comma-joined (seven).
    pub recipe_codes: String,
    /// The `CheckRecord` verdict vocabulary (`pass,fail,
    /// expected_refusal`).
    pub check_verdicts: String,
    /// The twelve `check-contributing.sh` check ids comma-joined.
    pub contrib_check_ids: String,
    /// The seven §8.1 lane names comma-joined.
    pub contrib_lanes: String,
    /// The six `check-trace.sh` leg ids comma-joined.
    pub trace_legs: String,
    /// The four §8.4 rubric items ` | `-joined, verbatim.
    pub rubric_items: String,
    /// The seven §8.4 measure names comma-joined, verbatim.
    pub measures: String,
    /// The §8.4 session shape decl.
    pub session_shape: String,
    /// The §8.4 cohort shape decl.
    pub cohort_shape: String,
    /// A minimal valid `RecipePlan` decodes via `from_json`
    /// (`true`).
    pub plan_decoded: bool,
}

/// `evaluate_vocabulary` — the declared-vocabulary check: the
/// supplied facts equal the landed formative-surface decls
/// verbatim. A drifted fact is a different vocabulary — refused,
/// never relabelled; an absent vocabulary is inconclusive.
pub fn evaluate_vocabulary(f: &VocabularyFacts) -> LegVerdict {
    if f.families.is_empty() {
        return LegVerdict::Inconclusive("vocabulary_absent".to_string());
    }
    let checks: [(&str, &str, &str); 15] = [
        ("card_schema", &f.card_schema, EXPECTED_CARD_SCHEMA),
        ("fixture_schema", &f.fixture_schema, EXPECTED_FIXTURE_SCHEMA),
        ("runner_version", &f.runner_version, EXPECTED_RUNNER_VERSION),
        ("recipe_version", &f.recipe_version, EXPECTED_RECIPE_VERSION),
        ("families", &f.families, EXPECTED_FAMILIES),
        ("example_codes", &f.example_codes, EXPECTED_EXAMPLE_CODES),
        ("recipe_codes", &f.recipe_codes, EXPECTED_RECIPE_CODES),
        ("check_verdicts", &f.check_verdicts, EXPECTED_CHECK_VERDICTS),
        (
            "contrib_check_ids",
            &f.contrib_check_ids,
            EXPECTED_CONTRIB_CHECK_IDS,
        ),
        ("contrib_lanes", &f.contrib_lanes, EXPECTED_CONTRIB_LANES),
        ("trace_legs", &f.trace_legs, EXPECTED_TRACE_LEGS),
        ("rubric_items", &f.rubric_items, EXPECTED_RUBRIC_ITEMS),
        ("measures", &f.measures, EXPECTED_MEASURES),
        ("session_shape", &f.session_shape, EXPECTED_SESSION_SHAPE),
        ("cohort_shape", &f.cohort_shape, EXPECTED_COHORT_SHAPE),
    ];
    for (name, got, want) in checks {
        if got != want {
            return LegVerdict::Refused(format!("{name}:{got}"));
        }
    }
    if f.cards_decoded != EXPECTED_CARDS_DECODED {
        return LegVerdict::Refused(format!("cards_decoded:{}", f.cards_decoded));
    }
    if !f.plan_decoded {
        return LegVerdict::Refused("plan_decoded:false".to_string());
    }
    LegVerdict::Verified
}

// ── leg 4 — refusal_vocabulary ───────────────────────────────────

/// The exercised case set — `exercise → expected verbatim outcome`
/// (the landed `ExampleCard::from_json`/`run_example`/
/// `RecipePlan::from_json` machinery's spellings, plus the two
/// landed static checkers' green verdicts, driven on exercised
/// cases only). The card decode refusals cover the closed-shape
/// guards; `card_stale_pin` is the pin gate firing before any
/// execution; the plan refusals cover the records-in honesty gate
/// itself (`unsupported_shape` — an authored member never rides
/// the plan); `example_run` is the committed ex-03 card's own
/// scripted lane running green on a scratch store (the cleared
/// checked-task artifact stands — an in-process machinery
/// exercise, never a participant act); `contrib_checker`/
/// `trace_checker` are the landed static checks running green
/// (the public guides/forms stand).
pub const EXERCISES: [(&str, &str); 11] = [
    ("card_extra_member", "unsupported_shape"),
    ("card_absent_member", "missing_field"),
    ("card_wrong_type", "type_mismatch"),
    ("card_unknown_case", "unknown_case"),
    ("card_stale_pin", "stale_pin"),
    ("plan_authored_member", "unsupported_shape"),
    ("plan_absent_member", "missing_field"),
    ("plan_wrong_type", "type_mismatch"),
    ("example_run", "pass:6"),
    ("contrib_checker", "green:12"),
    ("trace_checker", "green:6"),
];

/// `evaluate_refusals` — the machinery-stands check: every
/// declared exercise produced its verbatim typed outcome. A
/// missing exercise is `inconclusive`; a wrong outcome — or a
/// `decoded`/`accepted`/`report_emitted` produce, the guard
/// broken — is `refused`, never reclassified.
pub fn evaluate_refusals(observed: &BTreeMap<String, String>) -> LegVerdict {
    for (exercise, expected) in EXERCISES {
        match observed.get(exercise).map(String::as_str) {
            Some(o) if o == expected => {}
            Some("decoded") | Some("accepted") | Some("report_emitted") => {
                return LegVerdict::Refused(format!("produced:{exercise}"));
            }
            Some(o) => return LegVerdict::Refused(format!("{exercise}:{o}")),
            None => return LegVerdict::Inconclusive(format!("exercise_absent:{exercise}")),
        }
    }
    LegVerdict::Verified
}

// ── leg 5 — cohort_absent ────────────────────────────────────────

/// The absent formative cohort, verbatim — every participant/
/// session/observation/denominator count a declared zero, no
/// cohort authorized, no consent, no outreach, no telemetry.
#[derive(Debug, Clone, PartialEq)]
pub struct CohortFacts {
    /// `member → count` — every [`COHORT_MEMBERS`] member
    /// declared `0`.
    pub counts: BTreeMap<String, u64>,
    /// A 4–6-person cohort was authorized (`false` — GATE-G15
    /// voided the stage).
    pub cohort_authorized: bool,
    /// Any consent act was obtained (`false` — HUMAN-H9
    /// withheld).
    pub consent_obtained: bool,
    /// Any outreach/invitation was performed (`false` — the
    /// gate's verbatim criterion: "No outreach/telemetry/
    /// automatic review").
    pub outreach_performed: bool,
    /// Any telemetry ran (`false`).
    pub telemetry: bool,
}

/// The twenty-four absent-cohort members (frozen vocabulary) —
/// every invitation/consent/session/branch/task/assistance/
/// rubric/comprehension/useful-task/verdict/use/contribution/
/// follow-up/recheck/severe-defect/data axis of §8.4, absent.
pub const COHORT_MEMBERS: [&str; 24] = [
    "participants_invited",
    "participants_consented",
    "evaluators_engaged",
    "outside_verifiers_engaged",
    "sessions_run",
    "session_minutes_used",
    "branches_attempted",
    "tasks_completed_unaided",
    "tasks_completed_assisted",
    "assistance_events",
    "rubric_items_scored",
    "comprehension_records",
    "useful_task_completions",
    "verify_verdicts",
    "recompute_verdicts",
    "own_task_uses",
    "repeat_uses",
    "contributions_received",
    "reports_prepared",
    "followups_sent",
    "rechecks_run",
    "severe_defects",
    "severe_defects_repaired",
    "participant_data_fields",
];

/// `evaluate_cohort` — the absent-cohort check: every member
/// zero, no cohort authorized, no consent, no outreach, no
/// telemetry. Any nonzero fact is a real observation's evidence —
/// it belongs in a different packet and refuses here, never a
/// fabricated absence.
pub fn evaluate_cohort(f: &CohortFacts) -> LegVerdict {
    for member in COHORT_MEMBERS {
        match f.counts.get(member) {
            Some(0) => {}
            Some(n) => return LegVerdict::Refused(format!("{member}:{n}")),
            None => return LegVerdict::Inconclusive(format!("{member}_absent")),
        }
    }
    if f.cohort_authorized {
        return LegVerdict::Refused("cohort_authorized:true".to_string());
    }
    if f.consent_obtained {
        return LegVerdict::Refused("consent_obtained:true".to_string());
    }
    if f.outreach_performed {
        return LegVerdict::Refused("outreach_performed:true".to_string());
    }
    if f.telemetry {
        return LegVerdict::Refused("telemetry:true".to_string());
    }
    LegVerdict::Verified
}

// ── leg 6 — claims_withheld ──────────────────────────────────────

/// The withheld-claims facts — DF-H9-1 stays open, every
/// participant/outreach predicate withheld and no claim of any
/// sign is present (§8.4: convenience volunteers cannot estimate
/// population adoption or causal improvement — a voided cohort
/// less still; I-08: named prerequisites stay named).
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
    /// A usefulness claim (`false` — no session ran).
    pub usefulness_claim: bool,
    /// A comprehension claim (`false` — rubric items unscored).
    pub comprehension_claim: bool,
    /// An adoption claim (`false`).
    pub adoption_claim: bool,
    /// A causal-improvement claim (`false`).
    pub causal_claim: bool,
    /// A pooled cross-participant claim (`false`).
    pub pooled_claim: bool,
    /// A population-adoption estimate (`false` — §8.4 bars it
    /// even for a real convenience cohort).
    pub population_estimate: bool,
    /// A contribution-volume claim (`false`).
    pub contribution_claim: bool,
    /// A capability claim (`false`).
    pub capability_claim: bool,
    /// A publication claim (`false`).
    pub publication_claim: bool,
    /// A portability claim (`false`).
    pub portability_claim: bool,
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
        || f.usefulness_claim
        || f.comprehension_claim
        || f.adoption_claim
        || f.causal_claim
        || f.pooled_claim
        || f.population_estimate
        || f.contribution_claim
        || f.capability_claim
        || f.publication_claim
        || f.portability_claim
    {
        return LegVerdict::Refused("claim_present".to_string());
    }
    LegVerdict::Verified
}

// ── the report ───────────────────────────────────────────────────

/// The `hh-formative-withheld/1` record — the honest
/// `withheld{stage_voided}`/cohort-absent disposition: no cohort
/// exists, every session/participant/observation phase absent,
/// all participant/outreach/usefulness/adoption claims withheld
/// and DF-H9-1 open; `packet_id` is minted last over the
/// canonical record.
#[derive(Debug, Clone, PartialEq)]
pub struct FormativeWithheldReport {
    /// `H(canonical(record minus packet_id))` under
    /// `formative_withheld`.
    pub packet_id: String,
    /// The record version.
    pub version: String,
    /// The gate whose decision is recorded (`GATE-G15`).
    pub gate: String,
    /// The formative observation is withheld (`true`).
    pub withheld: bool,
    /// The participation stage is voided this round (`true`).
    pub stage_voided: bool,
    /// No cohort exists (`true` — HUMAN-H9 withheld).
    pub cohort_absent: bool,
    /// DF-H9-1 stays open (`true` — recorded honestly).
    pub df_h9_1_open: bool,
    /// Formative sessions completed (`0`).
    pub sessions_completed: u64,
    /// Participants observed (`0`).
    pub participants_observed: u64,
    /// The six legs.
    pub legs: Vec<Leg>,
    /// The folded disposition.
    pub disposition: Disposition,
    /// The admission shape — `withheld_stage_voided`, never an
    /// execution.
    pub admission: String,
}

impl FormativeWithheldReport {
    /// `to_json` — the canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(&self.version)),
            ("packet_id", Json::str(&self.packet_id)),
            ("gate", Json::str(&self.gate)),
            ("withheld", Json::Bool(self.withheld)),
            ("stage_voided", Json::Bool(self.stage_voided)),
            ("cohort_absent", Json::Bool(self.cohort_absent)),
            ("df_h9_1_open", Json::Bool(self.df_h9_1_open)),
            (
                "sessions_completed",
                Json::Int(self.sessions_completed as i64),
            ),
            (
                "participants_observed",
                Json::Int(self.participants_observed as i64),
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
/// complete — a malformed leg set is a `refused` assembly, never
/// a partial packet.
pub fn assemble(legs: Vec<Leg>) -> Result<FormativeWithheldReport, String> {
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
    let mut out = FormativeWithheldReport {
        packet_id: String::new(),
        version: VERSION.to_string(),
        gate: GATE.to_string(),
        withheld: true,
        stage_voided: true,
        cohort_absent: true,
        df_h9_1_open: true,
        sessions_completed: 0,
        participants_observed: 0,
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
