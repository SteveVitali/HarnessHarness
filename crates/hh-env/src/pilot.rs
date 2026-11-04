//! `hh-pilot/1` — the fixed pilot execution packet (R5.46; GATE-G12
//! Reading 2's admitted scope: the declared N=10 floor — 2·T·R≥10T at
//! T=1, R=5, inside the ≤60 ceiling — on the same frozen set).
//!
//! §4.1: a pilot may estimate feasibility, activation, failure,
//! variance, tails and cost — it cannot tune policies under zero
//! search, select favorable tasks or become confirmation. Records-in
//! only: every leg evaluator folds *supplied* facts — the pilot freeze
//! declaration (same members as the activation freeze plus the pilot
//! phase/replicate identity and the honored `activation_freeze_id`),
//! the ten first subjects' actual podman cell drives reusing the
//! activation `ArmOutcome` record, the §4.2 fidelity denominators over
//! all ten, the §3.2 missingness bound `[s/M,(s+u)/M]`, the
//! nearest-rank tail/censoring measures, the §9.2 pilot-envelope
//! feasibility facts, the all-first/all-attempt costs and the declared
//! worksheet — and never manufactures an execution, a compaction, a
//! grade or a pass. `freeze_id` mints over the canonical pilot freeze
//! record *before* any subject fact enters the packet; `packet_id`
//! mints last.

use hharness_identity::idp::idp_digest;
use hharness_wire::json::Json;

use crate::activation::{evaluate_arm, ArmOutcome, Disposition};
use crate::preflight::{Leg, LegVerdict};

/// The record version.
pub const VERSION: &str = "hh-pilot/1";
const DOMAIN: &str = "pilot";
const DOMAIN_FREEZE: &str = "pilot.freeze";

/// The admitted leg vocabulary (frozen — GATE-G12 Reading 2's scope:
/// 1 freeze + 2·T·R = 10 first subjects + 6 evidence folds).
pub const LEGS: [&str; 17] = [
    "freeze",
    "subject_a_r1",
    "subject_a_r2",
    "subject_a_r3",
    "subject_a_r4",
    "subject_a_r5",
    "subject_b_r1",
    "subject_b_r2",
    "subject_b_r3",
    "subject_b_r4",
    "subject_b_r5",
    "fidelity",
    "variance_tail",
    "missingness",
    "feasibility",
    "costs",
    "worksheet",
];

/// The declared pilot first-subject count (the N=10 floor at T=1, R=5).
pub const DECLARED_FIRST: u64 = 10;
/// The §9.2 pilot ceiling the authorization respected.
pub const CEILING_FIRST: u64 = 60;

// ── pilot freeze ─────────────────────────────────────────────────────

/// The pilot's frozen set — the *same* members as the activation
/// freeze (task/splits/policies/caps/oracle/detectors/analysis/stops)
/// under the pilot phase identity, plus the declared replicate count
/// and the honored parent `freeze_id`. Every member is an
/// observed/declared fact the caller verified; the evaluator never
/// re-derives or forgives a false one.
#[derive(Debug, Clone, PartialEq)]
pub struct PilotFreezeFacts {
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
    /// The declared arms equal the frozen [`ARM_STRATEGIES`] set.
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
    /// The phase identity — `pilot` required.
    pub phase: String,
    /// Warmth declared (never `unknown`).
    pub warmth: String,
    /// No artificial pressure declared.
    pub artificial_pressure_absent: bool,
    /// The declared task count T (1).
    pub tasks: u64,
    /// The declared repeats per arm R (5).
    pub repeats_per_arm: u64,
    /// The declared first-subject count 2·T·R (10).
    pub declared_first: u64,
    /// The floor holds: 2·T·R ≥ 10·T.
    pub floor_met: bool,
    /// The ≤60 ceiling respected.
    pub ceiling_respected: bool,
    /// GATE-G12 Reading 2's recorded state (`passed` required).
    pub reading_2: String,
    /// The honored activation `freeze_id` (verbatim, non-empty hex).
    pub activation_freeze_id: String,
    /// The caller verified `activation_freeze_id` against the
    /// committed `hh-activation/1` packet.
    pub activation_freeze_verified: bool,
}

/// `evaluate_pilot_freeze` — the pilot's frozen set was bound before
/// data, honors the activation freeze, and the Reading-2 admission is
/// the only dispatch authority echoed.
pub fn evaluate_pilot_freeze(f: &PilotFreezeFacts) -> LegVerdict {
    if !f.task_sha256_ok {
        return LegVerdict::Refused("pilot_freeze:pin_mismatch".to_string());
    }
    if f.split != "dev" || !f.split_predates_access {
        return LegVerdict::Refused("pilot_freeze:late_split".to_string());
    }
    if !f.arms_ok {
        return LegVerdict::Refused("pilot_freeze:arm_set".to_string());
    }
    if !f.caps_declared {
        return LegVerdict::Refused("pilot_freeze:caps_undeclared".to_string());
    }
    if !f.oracle_ref_bound {
        return LegVerdict::Refused("pilot_freeze:oracle_unbound".to_string());
    }
    if f.detectors.is_empty() || f.family_id.is_empty() || f.stops.is_empty() {
        return LegVerdict::Refused("pilot_freeze:identity_unfrozen".to_string());
    }
    if !f.search_budget_zero {
        return LegVerdict::Refused("pilot_freeze:policy_search".to_string());
    }
    if f.phase != "pilot" {
        return LegVerdict::Refused(format!("pilot_freeze:phase:{}", f.phase));
    }
    if f.warmth == "unknown" {
        return LegVerdict::Refused("pilot_freeze:unknown_warmth".to_string());
    }
    if !f.artificial_pressure_absent {
        return LegVerdict::Refused("pilot_freeze:artificial_pressure".to_string());
    }
    if f.tasks != 1 || f.repeats_per_arm != 5 || f.declared_first != 2 * f.tasks * f.repeats_per_arm
    {
        return LegVerdict::Refused(format!(
            "pilot_freeze:declared:T{}R{}N{}",
            f.tasks, f.repeats_per_arm, f.declared_first
        ));
    }
    if !f.floor_met || f.declared_first != DECLARED_FIRST {
        return LegVerdict::Refused("pilot_freeze:floor_unmet".to_string());
    }
    if !f.ceiling_respected || f.declared_first > CEILING_FIRST {
        return LegVerdict::Refused(format!("pilot_freeze:ceiling:{}", f.declared_first));
    }
    if f.reading_2 != "passed" {
        return LegVerdict::Refused(format!("pilot_freeze:reading_2:{}", f.reading_2));
    }
    if f.activation_freeze_id.len() != 64
        || !f
            .activation_freeze_id
            .chars()
            .all(|c| c.is_ascii_hexdigit())
        || !f.activation_freeze_verified
    {
        return LegVerdict::Refused("pilot_freeze:inherits_unverified".to_string());
    }
    LegVerdict::Verified
}

/// `pilot_freeze_id(freeze_json)` — the before-data commitment for the
/// pilot record (minted before any subject leg).
pub fn pilot_freeze_id(freeze_json: &Json) -> String {
    idp_digest(DOMAIN_FREEZE, freeze_json.to_canonical_string().as_bytes())
}

// ── subjects (the activation ArmOutcome record, verbatim) ────────────

/// One pilot subject's full retained record — the reused activation
/// `ArmOutcome` (the real podman cell drive + real `compact()`
/// outcome) plus the pilot's feasibility/tail measures. Every field an
/// observed fact.
#[derive(Debug, Clone, PartialEq)]
pub struct PilotSubject {
    /// The leg name (`subject_a_r1` … `subject_b_r5`).
    pub subject_id: String,
    /// The reused activation subject record.
    pub outcome: ArmOutcome,
    /// §3.1 outcome class: `completed` | `infrastructure_failure` |
    /// `refused` | `unresolved` (never a fabricated `scored`).
    pub outcome_class: String,
    /// Real wall milliseconds for the whole cell drive (`None` is
    /// right-censored — a lower bound, never a pretend cap value).
    pub wall_ms: Option<u64>,
    /// Mediated dispatches attempted on the subject (≤100 cap).
    pub calls: u64,
    /// Estimated input tokens the plan carried (≪1M).
    pub input_tokens: u64,
    /// Output bytes the dispatches wrote (≪100k).
    pub output_bytes: u64,
    /// The collected artifact landed host-side.
    pub artifact_collected: bool,
}

/// `evaluate_pilot_subject` — the subject leg folds the reused
/// activation arm evaluator (same real-cell + real-activation
/// predicates) and names a fabricated outcome class.
pub fn evaluate_pilot_subject(s: &PilotSubject) -> LegVerdict {
    match s.outcome_class.as_str() {
        "completed" | "infrastructure_failure" | "refused" | "unresolved" => {}
        other => return LegVerdict::Refused(format!("outcome_class:{other}")),
    }
    evaluate_arm(&s.outcome)
}

// ── fidelity fold (§4.2 denominators over the ten first subjects) ────

/// The §4.2 activation/fidelity fold over the pilot's ten first
/// subjects.
#[derive(Debug, Clone, PartialEq)]
pub struct PilotFidelityFacts {
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
    /// All first subjects (the declared count — 10 required).
    pub all_first: u64,
    /// The declared first-subject count this fold must cover.
    pub declared_first: u64,
    /// Intended/served policy disagreements recorded (verbatim, never
    /// relabelled — informational, not a leg failure).
    pub served_deviations: u64,
}

/// `evaluate_pilot_fidelity` — the denominators are internally
/// consistent and every counted member is real; a zero result is
/// honest evidence.
pub fn evaluate_pilot_fidelity(f: &PilotFidelityFacts) -> LegVerdict {
    if f.all_first != f.declared_first || f.declared_first != DECLARED_FIRST {
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

// ── missingness (§3.2's [s/M,(s+u)/M] bound) ─────────────────────────

/// The §3.2 missingness fold over the planned first slots.
#[derive(Debug, Clone, PartialEq)]
pub struct MissingnessFacts {
    /// Planned first slots M (10 required).
    pub planned: u64,
    /// s — subjects whose drive completed to collection.
    pub completed: u64,
    /// f — infrastructure failures.
    pub failed: u64,
    /// u — attempted but unresolved.
    pub unresolved: u64,
    /// The retained lower bound s/M in ppm.
    pub lower_ppm: u64,
    /// The retained upper bound (s+u)/M in ppm.
    pub upper_ppm: u64,
    /// Unplanned subjects attempted (0 — no n inflation).
    pub unplanned_attempted: u64,
}

/// `evaluate_missingness` — the all-planned bound is retained and
/// internally consistent; a conditional eligible-only point is never
/// the headline.
pub fn evaluate_missingness(m: &MissingnessFacts) -> LegVerdict {
    if m.planned != DECLARED_FIRST {
        return LegVerdict::Refused(format!("missingness:planned:{}", m.planned));
    }
    if m.completed + m.failed + m.unresolved != m.planned {
        return LegVerdict::Refused(format!(
            "missingness:reconcile:{}+{}+{}!={}",
            m.completed, m.failed, m.unresolved, m.planned
        ));
    }
    if m.unplanned_attempted != 0 {
        return LegVerdict::Refused(format!("missingness:unplanned:{}", m.unplanned_attempted));
    }
    if m.lower_ppm != m.completed * 1_000_000 / m.planned
        || m.upper_ppm != (m.completed + m.unresolved) * 1_000_000 / m.planned
    {
        return LegVerdict::Refused("missingness:bound_mismatch".to_string());
    }
    LegVerdict::Verified
}

// ── variance/tail (§3.2 nearest-rank + censoring, descriptive) ───────

/// The per-arm replicate measures — nearest-rank P50/P95/max over the
/// actual wall_ms samples and the tokens_freed spread, with censoring
/// retained. Descriptive only: the pilot never claims an effect.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmTail {
    /// Completed wall samples (≤R).
    pub n: u64,
    /// Nearest-rank P50 wall_ms (0 when n==0).
    pub wall_p50_ms: u64,
    /// Nearest-rank P95 wall_ms (0 when n==0; may equal max in small n).
    pub wall_p95_ms: u64,
    /// Observed max wall_ms (no population bound).
    pub wall_max_ms: u64,
    /// Min/max tokens_freed over measured subjects.
    pub tokens_min: u64,
    /// Max tokens_freed.
    pub tokens_max: u64,
}

/// The variance/tail fold across both arms.
#[derive(Debug, Clone, PartialEq)]
pub struct VarianceTailFacts {
    /// Arm A's replicate measures.
    pub arm_a: ArmTail,
    /// Arm B's replicate measures.
    pub arm_b: ArmTail,
    /// Subjects whose wall measure is right-censored (lower bound
    /// retained, never a pretend cap-valued completion).
    pub censored_wall: u64,
    /// Sign counts over per-subject tokens_freed.
    pub sign_pos: u64,
    /// Zero-valued tokens_freed subjects.
    pub sign_zero: u64,
    /// Negative tokens_freed subjects (impossible values are refused).
    pub sign_neg: u64,
    /// Any contrast/effect claim attempted (false required — §4.1).
    pub contrast_claimed: bool,
    /// Any policy tuning/search performed (false required — §4.1).
    pub tuning_performed: bool,
}

/// `evaluate_variance_tail` — the descriptive measures are internally
/// consistent and carry no scientific claim.
pub fn evaluate_variance_tail(v: &VarianceTailFacts) -> LegVerdict {
    if v.contrast_claimed || v.tuning_performed {
        return LegVerdict::Refused("pilot_became_inference".to_string());
    }
    for (name, a) in [("arm_a", &v.arm_a), ("arm_b", &v.arm_b)] {
        if a.n > 5 {
            return LegVerdict::Refused(format!("{name}:replicates:{}", a.n));
        }
        if a.n == 0 {
            continue;
        }
        if !(a.wall_p50_ms <= a.wall_p95_ms && a.wall_p95_ms <= a.wall_max_ms) {
            return LegVerdict::Refused(format!("{name}:percentiles_unordered"));
        }
        if a.tokens_min > a.tokens_max {
            return LegVerdict::Refused(format!("{name}:tokens_unordered"));
        }
    }
    if v.sign_neg > 0 {
        return LegVerdict::Refused("tokens_freed_negative".to_string());
    }
    let measured = v.arm_a.n + v.arm_b.n;
    if v.sign_pos + v.sign_zero != measured || v.censored_wall + measured > DECLARED_FIRST {
        return LegVerdict::Refused("tail_counts_inconsistent".to_string());
    }
    if measured == 0 {
        return LegVerdict::Inconclusive("no_measured_subjects".to_string());
    }
    LegVerdict::Verified
}

// ── feasibility (§9.2 pilot envelope, per-subject + tranche) ─────────

/// The §9.2 pilot envelope's declared ceilings.
pub mod envelope {
    /// ≤100 calls per subject.
    pub const CALLS_PER_SUBJECT: u64 = 100;
    /// ≤1M input tokens per subject.
    pub const INPUT_TOKENS: u64 = 1_000_000;
    /// ≤100k output tokens per subject.
    pub const OUTPUT_TOKENS: u64 = 100_000;
    /// ≤30 admitted wall minutes per subject.
    pub const WALL_MS: u64 = 30 * 60 * 1_000;
    /// ≤2 concurrent subjects.
    pub const CONCURRENT: u64 = 2;
    /// ≤1 setup retry per cell.
    pub const SETUP_RETRIES: u32 = 1;
}

/// The feasibility fold — provisioning/dispatch/seal/compaction
/// success counts plus every §9.2 envelope member verified against the
/// measured maxima.
#[derive(Debug, Clone, PartialEq)]
pub struct FeasibilityFacts {
    /// Declared first subjects (10).
    pub subjects_declared: u64,
    /// First subjects actually attempted (== declared required).
    pub subjects_run: u64,
    /// Cell attempts (all identities, incl. the ≤1 setup retry).
    pub cells_attempted: u64,
    /// Cells that reached provision+attach.
    pub cells_provisioned: u64,
    /// `observed:applied` dispatches counted.
    pub dispatches_applied: u64,
    /// `sealed` workspaces counted.
    pub seals: u64,
    /// `applied` compactions counted.
    pub compactions_applied: u64,
    /// Worst per-subject setup retries (≤1 required).
    pub max_setup_retries: u32,
    /// Worst per-subject dispatch count (≤100).
    pub max_calls: u64,
    /// Worst per-subject input tokens (≤1M).
    pub max_input_tokens: u64,
    /// Worst per-subject output bytes (≤100k).
    pub max_output_bytes: u64,
    /// Worst per-subject wall ms (≤30min).
    pub max_wall_ms: u64,
    /// Peak concurrent subjects (≤2 — the drive is sequential).
    pub max_concurrent: u64,
}

/// `evaluate_feasibility` — the tranche ran inside its declared
/// envelope; a ceiling breach stops (never a waived bound).
pub fn evaluate_feasibility(f: &FeasibilityFacts) -> LegVerdict {
    if f.cells_attempted == 0 {
        return LegVerdict::Inconclusive("backend_unavailable".to_string());
    }
    if f.subjects_declared != DECLARED_FIRST || f.subjects_run != f.subjects_declared {
        return LegVerdict::Refused(format!(
            "feasibility:subjects:{}/{}",
            f.subjects_run, f.subjects_declared
        ));
    }
    if f.cells_provisioned > f.cells_attempted {
        return LegVerdict::Refused("feasibility:attempts_inconsistent".to_string());
    }
    if f.max_setup_retries > envelope::SETUP_RETRIES {
        return LegVerdict::Refused(format!("envelope:setup_retries:{}", f.max_setup_retries));
    }
    if f.max_calls > envelope::CALLS_PER_SUBJECT {
        return LegVerdict::Refused(format!("envelope:calls:{}", f.max_calls));
    }
    if f.max_input_tokens > envelope::INPUT_TOKENS {
        return LegVerdict::Refused(format!("envelope:input_tokens:{}", f.max_input_tokens));
    }
    if f.max_output_bytes > envelope::OUTPUT_TOKENS {
        return LegVerdict::Refused(format!("envelope:output_tokens:{}", f.max_output_bytes));
    }
    if f.max_wall_ms > envelope::WALL_MS {
        return LegVerdict::Refused(format!("envelope:wall_ms:{}", f.max_wall_ms));
    }
    if f.max_concurrent > envelope::CONCURRENT {
        return LegVerdict::Refused(format!("envelope:concurrent:{}", f.max_concurrent));
    }
    if f.cells_provisioned == 0 {
        return LegVerdict::Inconclusive("no_cells_provisioned".to_string());
    }
    LegVerdict::Verified
}

// ── the report ───────────────────────────────────────────────────────

/// The `hh-pilot/1` record.
#[derive(Debug, Clone, PartialEq)]
pub struct PilotReport {
    /// `H(canonical(record minus packet_id))` under `pilot`.
    pub packet_id: String,
    /// The record version.
    pub version: String,
    /// The before-data pilot freeze commitment.
    pub freeze_id: String,
    /// The honored activation `freeze_id` (verbatim).
    pub activation_freeze_id: String,
    /// The canonical pilot freeze record.
    pub freeze: Json,
    /// The seventeen legs.
    pub legs: Vec<Leg>,
    /// The folded disposition.
    pub disposition: Disposition,
    /// Every retained attempt identity across cells/compactions.
    pub attempts: Vec<String>,
    /// GATE-G12 Reading 2's recorded state (verbatim — the admission,
    /// never inferred).
    pub reading_2: String,
    /// The admission field — always `bounded_pilot_execution`.
    pub admission: String,
    /// The declared §9.2 pilot envelope (verbatim).
    pub envelope: Json,
}

impl PilotReport {
    /// `to_json` — the canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(&self.version)),
            ("packet_id", Json::str(&self.packet_id)),
            ("freeze_id", Json::str(&self.freeze_id)),
            (
                "activation_freeze_id",
                Json::str(&self.activation_freeze_id),
            ),
            ("freeze", self.freeze.clone()),
            ("disposition", Json::str(self.disposition.name())),
            ("reading_2", Json::str(&self.reading_2)),
            ("admission", Json::str(&self.admission)),
            ("envelope", self.envelope.clone()),
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

/// `assemble_pilot(legs, freeze, attempts, activation_freeze_id)` —
/// fold the leg verdicts into the disposition, mint `freeze_id` over
/// the supplied pilot freeze record, then `packet_id` last over the
/// canonical record. Every leg name must be a `LEGS` member, unique
/// and complete — a malformed leg set is a refused assembly, never a
/// partial packet.
pub fn assemble_pilot(
    legs: Vec<Leg>,
    freeze: Json,
    attempts: Vec<String>,
    activation_freeze_id: &str,
) -> Result<PilotReport, String> {
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
    let envelope = Json::obj([
        ("subjects_first_ceiling", Json::Int(60)),
        ("calls_per_subject", Json::Int(100)),
        ("input_tokens_per_subject", Json::Int(1_000_000)),
        ("output_tokens_per_subject", Json::Int(100_000)),
        ("wall_minutes_per_subject", Json::Int(30)),
        ("verifier_minutes", Json::Int(15)),
        ("concurrent_subjects", Json::Int(2)),
        ("cores_per_subject", Json::Int(4)),
        ("ram_gib_per_subject", Json::Int(8)),
        ("cpu_core_hours", Json::Int(200)),
        ("storage_gib", Json::Int(50)),
    ]);
    let mut out = PilotReport {
        packet_id: String::new(),
        version: VERSION.to_string(),
        freeze_id: pilot_freeze_id(&freeze),
        activation_freeze_id: activation_freeze_id.to_string(),
        freeze,
        legs,
        reading_2: "passed".to_string(),
        admission: "bounded_pilot_execution".to_string(),
        envelope,
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
