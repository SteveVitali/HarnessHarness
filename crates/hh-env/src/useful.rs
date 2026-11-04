//! `hh-s1-useful/1` — the bounded S1 useful-task execution packet
//! (R5.43; GATE-G11's admitted scope; evidence spec §5.1/§5.3 + §2/§6).
//!
//! Records-in only: every leg evaluator folds *supplied* facts — the two
//! frozen subjects' actual cell drives, the real oracle chain's durable
//! rows, the three controls, the surviving observer, the carried probe
//! predicate, the cross-run evidence join and the declared §9.2
//! worksheet — and never manufactures an execution, a grade or a pass.
//! `useful_acceptance` is `marked` only when a subject's artifact is
//! genuinely independently `Decided 1e6`; every other state is withheld
//! or a typed inconclusive/failed — honest dispositions, never an S4 /
//! study / R2 / R3 / efficacy claim (§5.1).

use hharness_identity::idp::idp_digest;
use hharness_wire::json::Json;

use crate::preflight::{Leg, LegVerdict};

/// The record version.
pub const VERSION: &str = "hh-s1-useful/1";
const DOMAIN: &str = "s1_useful";

/// The admitted leg vocabulary (frozen — GATE-G11's S1 scope).
pub const LEGS: [&str; 7] = [
    "subject_a",
    "subject_b",
    "oracle_chain",
    "controls",
    "observer_probe",
    "evidence_join",
    "worksheet",
];

/// The GATE-G11 carried predicate, recorded verbatim: the instrument
/// `probe()` verb is ENOENT on both lanes — a machinery repair owed to a
/// named owner; until it verifies at bind, no cell's probe evidence
/// counts (`withheld`, never a simulated pass).
pub const PROBE_CARRIED: &str = "probe_verb_enoent";

// ── subject cells ────────────────────────────────────────────────────

/// One subject's actual cell outcome — supplied by the live drive
/// (provision/attach → mediated write → observer → probe attempt →
/// seal → collect). Every field is an observed fact, verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct SubjectOutcome {
    /// The frozen bind's id (`subject_a` | `subject_b`).
    pub subject_id: String,
    /// The bound task (`a-hello` | `a-sum` — pin-verified upstream).
    pub task: String,
    /// The cell drive ran at all.
    pub attempted: bool,
    /// The backend that carried the cell (`podman`; a substitute is
    /// never admissible — an absent backend leaves it unattempted).
    pub backend: String,
    /// The mediated dispatch's terminal outcome (`observed:applied`).
    pub dispatch: Option<String>,
    /// The collected artifact's sha256 (`None` — a scored failure,
    /// retained, not infrastructure).
    pub artifact_sha256: Option<String>,
    /// The collected artifact's byte length.
    pub artifact_bytes: Option<u64>,
    /// Observer intervals durably recorded (3 required).
    pub intervals: u64,
    /// The denied/read-only sentinel re-read intact.
    pub sentinel_intact: Option<bool>,
    /// The observer rows still readable after executor exit.
    pub observer_survived: bool,
    /// The instrument `probe()` result verbatim — `error:…` values are
    /// recorded facts that never count (carried predicate).
    pub probe: Option<String>,
    /// `seal_workspace`'s status (`sealed` required).
    pub seal: Option<String>,
    /// The `submission_seal` row's event id on the subject run.
    pub seal_event_id: Option<String>,
    /// The `meters_sampled` row's event id on the subject run.
    pub meters_event_id: Option<String>,
    /// Diagnostic setup retries consumed (≤1 bound; new identity each).
    pub setup_retries: u32,
    /// Every cell/setup attempt identity, in order (all retained).
    pub attempts: Vec<String>,
    /// The verbatim setup/drive failure, when one occurred.
    pub failure: Option<String>,
}

/// `evaluate_subject` — the cell leg: real attempted drive on the
/// admitted backend, the retry bound held, observed mediation, intact
/// sentinel, surviving observer and a landed seal. A missing artifact
/// is a retained scored-failure outcome, not a leg failure; probe
/// evidence never counts (carried predicate).
pub fn evaluate_subject(s: &SubjectOutcome) -> LegVerdict {
    if !s.attempted {
        return LegVerdict::Inconclusive(format!("cell_not_attempted:{}", s.subject_id));
    }
    if s.backend != "podman" {
        return LegVerdict::Refused(format!("substitute_backend:{}", s.backend));
    }
    if s.setup_retries > 1 {
        return LegVerdict::Refused(format!("retry_bound:{}", s.setup_retries));
    }
    if let Some(f) = &s.failure {
        return LegVerdict::Inconclusive(format!("cell_setup_failed:{f}"));
    }
    match s.dispatch.as_deref() {
        Some("observed:applied") => {}
        Some("refused") => return LegVerdict::Refused("mediation_denied".to_string()),
        Some(o) => return LegVerdict::Inconclusive(format!("dispatch_outcome:{o}")),
        None => return LegVerdict::Inconclusive("dispatch_unobserved".to_string()),
    }
    if s.intervals < 3 {
        return LegVerdict::Unsupported(format!("observer_incomplete:{}", s.intervals));
    }
    if s.sentinel_intact == Some(false) {
        return LegVerdict::Refused("sentinel_violated".to_string());
    }
    if !s.observer_survived {
        return LegVerdict::Unsupported("observer_not_surviving".to_string());
    }
    match s.seal.as_deref() {
        Some("sealed") => LegVerdict::Verified,
        Some(t) => LegVerdict::Unsupported(format!("seal:{t}")),
        None => LegVerdict::Inconclusive("seal_unobserved".to_string()),
    }
}

// ── oracle chain ─────────────────────────────────────────────────────

/// One subject's retained oracle attempt — supplied from the real
/// `OracleRunner` drive (invoked durable before verdict; the executed
/// `hh-bench-adapter` oracle's mapped outcome).
#[derive(Debug, Clone, PartialEq)]
pub struct OracleOutcome {
    /// The subject this attempt grades.
    pub subject_id: String,
    /// The attempt ran (invoked + verdict rows landed).
    pub attempted: bool,
    /// `invoked` seq precedes `verdict` seq on the instrument run.
    pub invoked_before_verdict: Option<bool>,
    /// The mapped outcome (`decided` | `oracle_failure`).
    pub outcome: Option<String>,
    /// The decided reward in ppm (`None` on oracle failure).
    pub reward_ppm: Option<i64>,
    /// The attempt's content ref.
    pub attempt_ref: Option<String>,
    /// The `verification.oracle.verdict` row coordinate.
    pub verdict_ref: Option<String>,
    /// The charge attribution (`instrument` required — never subject).
    pub charged_to: Option<String>,
    /// The verbatim attempt error (`OracleError`), when one occurred.
    pub failure: Option<String>,
}

/// `evaluate_oracle` — the chain's integrity, not the grade's sign:
/// invoked-before-verdict durable ordering and a typed outcome
/// (`decided`/`oracle_failure`) charged to the instrument.
pub fn evaluate_oracle(o: &OracleOutcome) -> LegVerdict {
    if !o.attempted {
        return LegVerdict::Inconclusive(format!("oracle_not_attempted:{}", o.subject_id));
    }
    if let Some(f) = &o.failure {
        return LegVerdict::Unsupported(format!("oracle_error:{f}"));
    }
    if o.invoked_before_verdict == Some(false) {
        return LegVerdict::Refused("invocation_not_first".to_string());
    }
    match o.charged_to.as_deref() {
        Some("instrument") => {}
        Some(c) => return LegVerdict::Refused(format!("charged_to:{c}")),
        None => return LegVerdict::Inconclusive("charge_unobserved".to_string()),
    }
    match o.outcome.as_deref() {
        Some("decided") | Some("oracle_failure") => LegVerdict::Verified,
        Some(x) => return LegVerdict::Unsupported(format!("oracle_outcome:{x}")),
        None => LegVerdict::Inconclusive("verdict_unobserved".to_string()),
    }
}

// ── controls ─────────────────────────────────────────────────────────

/// The three controls' mapped outcomes through the same real oracle
/// path — they must diverge (§5.1: a failed negative control blocks
/// acceptance).
#[derive(Debug, Clone, PartialEq)]
pub struct ControlsOutcome {
    /// The control drives ran.
    pub attempted: bool,
    /// Sanctioned correct output's reward (must be `1_000_000`).
    pub correct_reward: Option<i64>,
    /// Plausible wrong contents' reward (must be `0` — a valid zero,
    /// never oracle_failure).
    pub wrong_reward: Option<i64>,
    /// Broken oracle's outcome (must be `oracle_failure`).
    pub broken_outcome: Option<String>,
}

/// `evaluate_controls` — the grade chain diverges for real.
pub fn evaluate_controls(c: &ControlsOutcome) -> LegVerdict {
    if !c.attempted {
        return LegVerdict::Inconclusive("controls_not_attempted".to_string());
    }
    match c.correct_reward {
        Some(1_000_000) => {}
        Some(r) => return LegVerdict::Refused(format!("control_correct:{r}")),
        None => return LegVerdict::Refused("control_correct:undecided".to_string()),
    }
    match c.wrong_reward {
        Some(0) => {}
        Some(r) => return LegVerdict::Refused(format!("control_wrong:{r}")),
        None => return LegVerdict::Refused("control_wrong:undecided".to_string()),
    }
    match c.broken_outcome.as_deref() {
        Some("oracle_failure") => LegVerdict::Verified,
        Some(o) => LegVerdict::Refused(format!("control_broken:{o}")),
        None => LegVerdict::Refused("control_broken:undecided".to_string()),
    }
}

// ── observer ─────────────────────────────────────────────────────────

/// The surviving-observer fold across the cells — the GATE-G11-accepted
/// confinement/observer proof. The probe verb is the carried predicate:
/// its verbatim result is recorded in facts, never counted.
#[derive(Debug, Clone, PartialEq)]
pub struct ObserverCell {
    /// The drive ran at all.
    pub attempted: bool,
    /// Minimum intervals recorded across cells (3 required each).
    pub min_intervals: u64,
    /// Every cell's sentinel intact.
    pub sentinels_intact: bool,
    /// Observer rows readable after executor exit on every cell.
    pub survived: bool,
}

/// `evaluate_observer_cell` — surviving-observer proof only; probe
/// evidence withheld under the carried predicate regardless of content.
pub fn evaluate_observer_cell(o: &ObserverCell) -> LegVerdict {
    if !o.attempted {
        return LegVerdict::Inconclusive("observer_not_attempted".to_string());
    }
    if o.min_intervals < 3 {
        return LegVerdict::Unsupported(format!("observer_incomplete:{}", o.min_intervals));
    }
    if !o.sentinels_intact {
        return LegVerdict::Refused("sentinel_violated".to_string());
    }
    if !o.survived {
        return LegVerdict::Unsupported("observer_not_surviving".to_string());
    }
    LegVerdict::Verified
}

// ── evidence join ────────────────────────────────────────────────────

/// The cross-run join facts — subject seal/detach/meters refs,
/// instrument invoked/verdict refs, artifact digest ↔ submission_ref,
/// charge attribution.
#[derive(Debug, Clone, PartialEq)]
pub struct JoinFacts {
    /// The join was checked.
    pub attempted: bool,
    /// The seal ref resolves to `action.environment.snapshot{purpose:
    /// "submission_seal"}` on the subject run.
    pub seal_ref_resolves: bool,
    /// The artifact sha256 equals the graded `submission_ref`.
    pub submission_digest_matches: bool,
    /// Invoked/verdict rows exist on the instrument run with the frozen
    /// subject/submission/oracle/policy/cost pins and cause refs.
    pub attempt_rows_join: bool,
    /// The subject run's `meters_sampled` usage row exists.
    pub meters_present: bool,
    /// `charged_to` attribution holds per row (instrument ↔ subject).
    pub cost_attribution_ok: bool,
}

/// `evaluate_join` — every declared join member must resolve on the
/// durable streams; a missing member names itself.
pub fn evaluate_join(j: &JoinFacts) -> LegVerdict {
    if !j.attempted {
        return LegVerdict::Inconclusive("join_not_attempted".to_string());
    }
    if !j.seal_ref_resolves {
        return LegVerdict::Refused("join:seal_ref".to_string());
    }
    if !j.submission_digest_matches {
        return LegVerdict::Refused("join:submission_digest".to_string());
    }
    if !j.attempt_rows_join {
        return LegVerdict::Refused("join:attempt_rows".to_string());
    }
    if !j.meters_present {
        return LegVerdict::Refused("join:meters".to_string());
    }
    if !j.cost_attribution_ok {
        return LegVerdict::Refused("join:cost_attribution".to_string());
    }
    LegVerdict::Verified
}

// ── the report ───────────────────────────────────────────────────────

/// The packet disposition — `executed` only when every leg verified.
#[derive(Debug, Clone, PartialEq)]
pub enum Disposition {
    /// Every leg verified — the bounded S1 execution ran and retained.
    Executed,
    /// At least one `refused`/`unsupported` leg — a definitive failure.
    Failed,
    /// At least one `inconclusive` leg, none failed — undetermined.
    Inconclusive,
}

impl Disposition {
    /// The closed vocabulary.
    pub fn name(&self) -> &'static str {
        match self {
            Disposition::Executed => "executed",
            Disposition::Failed => "failed",
            Disposition::Inconclusive => "inconclusive",
        }
    }
}

/// The `hh-s1-useful/1` record.
#[derive(Debug, Clone, PartialEq)]
pub struct UsefulReport {
    /// `H(canonical(record minus packet_id))` under `s1_useful`.
    pub packet_id: String,
    /// The record version.
    pub version: String,
    /// The seven legs.
    pub legs: Vec<Leg>,
    /// The folded disposition.
    pub disposition: Disposition,
    /// `marked` iff ≥1 subject artifact independently graded
    /// `Decided 1e6`; else `withheld{no_independent_correct_artifact}`.
    pub useful_acceptance: String,
    /// Carried unmet predicates, verbatim (the probe verb).
    pub carried_predicates: Vec<String>,
    /// Every retained attempt identity across cells/controls/oracle.
    pub attempts: Vec<String>,
    /// The admission field — always `bounded_s1_execution`.
    pub admission: String,
}

impl UsefulReport {
    /// `to_json` — the canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(&self.version)),
            ("packet_id", Json::str(&self.packet_id)),
            ("disposition", Json::str(self.disposition.name())),
            ("useful_acceptance", Json::str(&self.useful_acceptance)),
            (
                "carried_predicates",
                Json::Arr(
                    self.carried_predicates
                        .iter()
                        .map(|s| Json::str(s))
                        .collect(),
                ),
            ),
            ("admission", Json::str(&self.admission)),
            (
                "attempts",
                Json::Arr(self.attempts.iter().map(|s| Json::str(s)).collect()),
            ),
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

/// `assemble(legs, oracle_outcomes, attempts)` — fold the leg verdicts
/// into the disposition, derive `useful_acceptance` from the retained
/// oracle outcomes (a `Decided ≥ 1e6` on a *subject* attempt marks it —
/// controls never contribute), and mint `packet_id` last over the
/// canonical record. Every leg name must be a `LEGS` member, unique and
/// complete — a malformed leg set is a refused assembly, never a
/// partial packet.
pub fn assemble(
    legs: Vec<Leg>,
    oracle_outcomes: &[OracleOutcome],
    attempts: Vec<String>,
) -> Result<UsefulReport, String> {
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
        Disposition::Executed
    };
    let marked = oracle_outcomes
        .iter()
        .filter(|o| o.subject_id.starts_with("subject_"))
        .any(|o| o.outcome.as_deref() == Some("decided") && o.reward_ppm == Some(1_000_000));
    let mut out = UsefulReport {
        packet_id: String::new(),
        version: VERSION.to_string(),
        legs,
        useful_acceptance: if marked {
            "marked".to_string()
        } else {
            "withheld{no_independent_correct_artifact}".to_string()
        },
        carried_predicates: vec![PROBE_CARRIED.to_string()],
        attempts,
        admission: "bounded_s1_execution".to_string(),
        disposition,
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("packet_id");
    }
    out.packet_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}
