//! `hh-activation/1` — the bounded one-task activation execution packet
//! (R5.46; GATE-G12 Reading 1's admitted scope; evidence spec §4.2's
//! activation/fidelity row + §9.2's activation quote row).
//!
//! Records-in only: every leg evaluator folds *supplied* facts — the
//! frozen-set declaration, the two first subjects' actual podman cell
//! drives, the real `compact()` outcomes per frozen arm, the
//! all-first/all-attempt cost facts, the declared §9.2 worksheet and
//! the withheld pilot leg — and never manufactures an execution, a
//! compaction or a pass. `freeze_id` mints over the canonical freeze
//! record *before* any subject fact enters the packet; `packet_id`
//! mints last. The pilot leg is `withheld{reading_2_pending}` — its
//! execution would pre-empt the operator's Reading-2 review, never an
//! executed stage.

use hharness_identity::idp::idp_digest;
use hharness_wire::json::Json;

use crate::preflight::{Leg, LegVerdict};

/// The record version.
pub const VERSION: &str = "hh-activation/1";
const DOMAIN: &str = "activation";
const DOMAIN_FREEZE: &str = "activation.freeze";

/// The admitted leg vocabulary (frozen — GATE-G12 Reading 1's scope).
pub const LEGS: [&str; 7] = [
    "freeze",
    "subject_arm_a",
    "subject_arm_b",
    "activation_fidelity",
    "costs",
    "worksheet",
    "pilot_withheld",
];

// ── freeze ───────────────────────────────────────────────────────────

/// The declared frozen set — supplied before data (§4.1/G12). Every
/// member is an observed/declared fact the caller verified; the
/// evaluator never re-derives or forgives a false one.
#[derive(Debug, Clone, PartialEq)]
pub struct FreezeFacts {
    /// The bound task (`a-hello`).
    pub task_id: String,
    /// The task's committed bytes re-hashed to the declared pin.
    pub task_sha256_ok: bool,
    /// The declared split (`dev` — the activation/pilot access pool).
    pub split: String,
    /// The split assignment predates access.
    pub split_predates_access: bool,
    /// The declared arm strategies (verbatim).
    pub arms: Vec<String>,
    /// The declared arms equal the frozen [`ARM_STRATEGIES`] set, each
    /// with a bound artifact ref and the fixed model roles.
    pub arms_ok: bool,
    /// The §2.4 four caps declared.
    pub caps_declared: bool,
    /// The unchanged oracle pin bound.
    pub oracle_ref_bound: bool,
    /// The detector ids the packet applies (non-empty).
    pub detectors: Vec<String>,
    /// The analysis-family identity (non-empty).
    pub family_id: String,
    /// The stop-rule identities (non-empty).
    pub stops: Vec<String>,
    /// Zero search admitted.
    pub search_budget_zero: bool,
    /// The phase identity — `activation` required.
    pub phase: String,
    /// Warmth declared (never `unknown`).
    pub warmth: String,
    /// No artificial pressure declared.
    pub artificial_pressure_absent: bool,
}

/// `evaluate_freeze` — the frozen set was bound before data.
pub fn evaluate_freeze(f: &FreezeFacts) -> LegVerdict {
    if !f.task_sha256_ok {
        return LegVerdict::Refused("freeze:pin_mismatch".to_string());
    }
    if f.split != "dev" || !f.split_predates_access {
        return LegVerdict::Refused("freeze:late_split".to_string());
    }
    if !f.arms_ok {
        return LegVerdict::Refused("freeze:arm_set".to_string());
    }
    if !f.caps_declared {
        return LegVerdict::Refused("freeze:caps_undeclared".to_string());
    }
    if !f.oracle_ref_bound {
        return LegVerdict::Refused("freeze:oracle_unbound".to_string());
    }
    if f.detectors.is_empty() || f.family_id.is_empty() || f.stops.is_empty() {
        return LegVerdict::Refused("freeze:identity_unfrozen".to_string());
    }
    if !f.search_budget_zero {
        return LegVerdict::Refused("freeze:policy_search".to_string());
    }
    if f.phase != "activation" {
        return LegVerdict::Refused(format!("freeze:phase:{}", f.phase));
    }
    if f.warmth == "unknown" {
        return LegVerdict::Refused("freeze:unknown_warmth".to_string());
    }
    if !f.artificial_pressure_absent {
        return LegVerdict::Refused("freeze:artificial_pressure".to_string());
    }
    LegVerdict::Verified
}

/// `freeze_id(freeze_json)` — the before-data commitment: `idp_digest`
/// over the canonical freeze record (minted inside the packet before
/// any subject leg; `None` never produces an id).
pub fn freeze_id(freeze_json: &Json) -> String {
    idp_digest(DOMAIN_FREEZE, freeze_json.to_canonical_string().as_bytes())
}

// ── arm subjects ─────────────────────────────────────────────────────

/// One first subject's retained record — every field an observed fact,
/// verbatim: the real podman cell drive plus the arm's real `compact()`
/// outcome under an eligible pressure opportunity.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmOutcome {
    /// The arm (`arm:a` | `arm:b`).
    pub arm_id: String,
    /// The intended strategy (`evict_oldest` | `clear_tool_results`).
    pub strategy: String,
    /// The cell drive ran at all.
    pub cell_attempted: bool,
    /// The backend that carried the cell (`podman` required).
    pub backend: String,
    /// The pre-compaction dispatch's terminal outcome.
    pub pre_dispatch: Option<String>,
    /// The backend string the executor's session reported verbatim.
    pub executor_backend: Option<String>,
    /// Observer intervals durably recorded (3 required).
    pub intervals: u64,
    /// The denied/read-only sentinel re-read intact.
    pub sentinel_intact: Option<bool>,
    /// Observer rows still readable after executor exit.
    pub observer_survived: bool,
    /// The instrument `probe()` result verbatim.
    pub probe: Option<String>,
    /// `seal_workspace`'s status (`sealed` required).
    pub seal: Option<String>,
    /// The collected artifact's sha256 (retained; a scored fact, not a
    /// leg failure — activation is mechanism evidence).
    pub artifact_sha256: Option<String>,
    /// An eligible pressure opportunity existed (`occupancy > cap`).
    pub pressure_opportunity: bool,
    /// `context.compaction.started` emitted (a start).
    pub started: bool,
    /// `CompactionStatus::Applied` (a completion).
    pub completed: bool,
    /// The compaction's status verbatim (`applied|ineffective|failed`).
    pub compaction_status: Option<String>,
    /// The served variant (`record.variant_ref` — may differ from the
    /// intended arm on a fallback; recorded, never relabelled).
    pub served_variant: Option<String>,
    /// The recorded `fallback_variant`, when the arm fell back.
    pub fallback_variant: Option<String>,
    /// Forgotten item count.
    pub forgotten_count: u64,
    /// Tokens freed.
    pub tokens_freed: u64,
    /// The post-compaction view's item set differs from the plan's
    /// (the next request's context changed).
    pub request_changed: Option<bool>,
    /// A subsequent mediated dispatch landed on the same subject run
    /// after the compaction rows (the with-next-dispatch join).
    pub next_dispatch_after: Option<bool>,
    /// This first subject triggered (started under an opportunity).
    pub triggered: bool,
    /// Setup retries consumed (≤1 bound; new identity each).
    pub setup_retries: u32,
    /// Every attempt identity, in order (all retained).
    pub attempts: Vec<String>,
    /// The verbatim setup/drive failure, when one occurred.
    pub failure: Option<String>,
}

/// `evaluate_arm` — real cell + real activation: podman drive,
/// observer/sentinel/seal, an eligible opportunity, a started and
/// completed compaction with durable rows before a landed next
/// dispatch. Honest partials name themselves.
pub fn evaluate_arm(a: &ArmOutcome) -> LegVerdict {
    if !a.cell_attempted {
        return LegVerdict::Inconclusive(format!("cell_not_attempted:{}", a.arm_id));
    }
    if a.backend != "podman" {
        return LegVerdict::Refused(format!("substitute_backend:{}", a.backend));
    }
    if a.setup_retries > 1 {
        return LegVerdict::Refused(format!("retry_bound:{}", a.setup_retries));
    }
    if let Some(f) = &a.failure {
        return LegVerdict::Inconclusive(format!("cell_setup_failed:{f}"));
    }
    match a.pre_dispatch.as_deref() {
        Some("observed:applied") => {}
        Some("refused") => return LegVerdict::Refused("mediation_denied".to_string()),
        Some(o) => return LegVerdict::Inconclusive(format!("pre_dispatch:{o}")),
        None => return LegVerdict::Inconclusive("pre_dispatch_unobserved".to_string()),
    }
    if a.intervals < 3 {
        return LegVerdict::Unsupported(format!("observer_incomplete:{}", a.intervals));
    }
    if a.sentinel_intact == Some(false) {
        return LegVerdict::Refused("sentinel_violated".to_string());
    }
    if !a.observer_survived {
        return LegVerdict::Unsupported("observer_not_surviving".to_string());
    }
    if !a.pressure_opportunity {
        return LegVerdict::Inconclusive("no_pressure_opportunity".to_string());
    }
    if !a.started {
        return LegVerdict::Inconclusive("compaction_not_started".to_string());
    }
    match a.compaction_status.as_deref() {
        Some("applied") => {}
        Some(s) => return LegVerdict::Inconclusive(format!("compaction:{s}")),
        None => return LegVerdict::Inconclusive("compaction_unobserved".to_string()),
    }
    if a.request_changed != Some(true) {
        return LegVerdict::Inconclusive("request_unchanged".to_string());
    }
    if a.next_dispatch_after != Some(true) {
        return LegVerdict::Inconclusive("next_dispatch_unjoined".to_string());
    }
    match a.seal.as_deref() {
        Some("sealed") => LegVerdict::Verified,
        Some(t) => LegVerdict::Unsupported(format!("seal:{t}")),
        None => LegVerdict::Inconclusive("seal_unobserved".to_string()),
    }
}

// ── activation/fidelity fold (§4.2 denominators, verbatim) ───────────

/// The §4.2 activation/fidelity fold over the two first subjects.
#[derive(Debug, Clone, PartialEq)]
pub struct FidelityFacts {
    /// Eligible pressure opportunities counted.
    pub opportunities: u64,
    /// Compactions started.
    pub starts: u64,
    /// Compactions completed (`applied`).
    pub completions: u64,
    /// Completions whose next request changed with a next dispatch.
    pub changed_requests: u64,
    /// First subjects that triggered.
    pub triggered_first: u64,
    /// All first subjects (the frozen count — 2 required).
    pub all_first: u64,
    /// Intended/served policy disagreements recorded (verbatim, never
    /// relabelled — informational, not a leg failure).
    pub served_deviations: u64,
}

/// `evaluate_fidelity` — the denominators are internally consistent
/// and every counted member is real; a zero result is honest evidence.
pub fn evaluate_fidelity(f: &FidelityFacts) -> LegVerdict {
    if f.all_first != 2 {
        return LegVerdict::Refused(format!("first_subjects:{}", f.all_first));
    }
    if f.starts > f.opportunities
        || f.completions > f.starts
        || f.changed_requests > f.completions
        || f.triggered_first > f.all_first
    {
        return LegVerdict::Refused("fidelity_counts_inconsistent".to_string());
    }
    if f.opportunities == 0 {
        return LegVerdict::Inconclusive("no_pressure_opportunities".to_string());
    }
    LegVerdict::Verified
}

// ── costs ────────────────────────────────────────────────────────────

/// The all-first/all-attempt cost facts.
#[derive(Debug, Clone, PartialEq)]
pub struct CostFacts {
    /// Declared == actual first-subject count (2).
    pub first_subjects_match: bool,
    /// `attempts[]` reconciles 1:1 with the retained attempt records.
    pub attempts_reconciled: bool,
    /// Every subject has seal + meters refs.
    pub seal_meters_present: bool,
    /// USD exposure micros (0 required).
    pub exposure_micros: i64,
    /// `provider_unknown_exposure` bounded at zero.
    pub unknown_exposure_bounded: bool,
    /// Instrument-charged rows never landed on a subject.
    pub charge_attribution_ok: bool,
}

/// `evaluate_costs` — all-first counts and all-attempt costs held.
pub fn evaluate_costs(c: &CostFacts) -> LegVerdict {
    if !c.first_subjects_match {
        return LegVerdict::Refused("first_subject_count".to_string());
    }
    if !c.attempts_reconciled {
        return LegVerdict::Refused("attempts_unreconciled".to_string());
    }
    if !c.seal_meters_present {
        return LegVerdict::Refused("costs:seal_meters_absent".to_string());
    }
    if c.exposure_micros != 0 {
        return LegVerdict::Refused(format!("spend_declared:{}", c.exposure_micros));
    }
    if !c.unknown_exposure_bounded {
        return LegVerdict::Refused("unknown_exposure_unbounded".to_string());
    }
    if !c.charge_attribution_ok {
        return LegVerdict::Refused("charge_attribution".to_string());
    }
    LegVerdict::Verified
}

// ── pilot withheld ───────────────────────────────────────────────────

/// The withheld pilot leg's facts — verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct PilotFacts {
    /// GATE-G12 Reading 2's state (`pending` required).
    pub reading_2: String,
    /// Pilot subjects actually run (0 required).
    pub pilot_subjects_run: u64,
    /// Any pilot dispatch was attempted (false required).
    pub pilot_dispatch_attempted: bool,
}

/// `evaluate_pilot_withheld` — the pilot leg stayed withheld under
/// Reading 1's staged admission; an attempted pilot is a refusal,
/// never an executed stage.
pub fn evaluate_pilot_withheld(p: &PilotFacts) -> LegVerdict {
    if p.reading_2 != "pending" {
        return LegVerdict::Refused(format!("reading_2:{}", p.reading_2));
    }
    if p.pilot_dispatch_attempted || p.pilot_subjects_run > 0 {
        return LegVerdict::Refused("pilot_executed_without_reading_2".to_string());
    }
    LegVerdict::Verified
}

// ── the report ───────────────────────────────────────────────────────

/// The packet disposition — `executed` only when every leg verified.
#[derive(Debug, Clone, PartialEq)]
pub enum Disposition {
    /// Every leg verified — the bounded activation reading ran.
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

/// The `hh-activation/1` record.
#[derive(Debug, Clone, PartialEq)]
pub struct ActivationReport {
    /// `H(canonical(record minus packet_id))` under `activation`.
    pub packet_id: String,
    /// The record version.
    pub version: String,
    /// The before-data freeze commitment.
    pub freeze_id: String,
    /// The canonical freeze record.
    pub freeze: Json,
    /// The seven legs.
    pub legs: Vec<Leg>,
    /// The folded disposition.
    pub disposition: Disposition,
    /// Every retained attempt identity across cells/compactions.
    pub attempts: Vec<String>,
    /// The withheld pilot leg's verbatim state.
    pub pilot: String,
    /// The admission field — always `bounded_activation_execution`.
    pub admission: String,
}

impl ActivationReport {
    /// `to_json` — the canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(&self.version)),
            ("packet_id", Json::str(&self.packet_id)),
            ("freeze_id", Json::str(&self.freeze_id)),
            ("freeze", self.freeze.clone()),
            ("disposition", Json::str(self.disposition.name())),
            ("pilot", Json::str(&self.pilot)),
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

/// `assemble(legs, freeze, attempts)` — fold the leg verdicts into the
/// disposition, mint `freeze_id` over the supplied freeze record, then
/// `packet_id` last over the canonical record. Every leg name must be
/// a `LEGS` member, unique and complete — a malformed leg set is a
/// refused assembly, never a partial packet.
pub fn assemble(
    legs: Vec<Leg>,
    freeze: Json,
    attempts: Vec<String>,
) -> Result<ActivationReport, String> {
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
    let mut out = ActivationReport {
        packet_id: String::new(),
        version: VERSION.to_string(),
        freeze_id: freeze_id(&freeze),
        freeze,
        legs,
        pilot: "withheld{reading_2_pending}".to_string(),
        admission: "bounded_activation_execution".to_string(),
        attempts,
        disposition,
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("packet_id");
    }
    out.packet_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}
