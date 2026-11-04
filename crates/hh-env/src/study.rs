//! `hh-study-report/1` — the admitted-study recompute report (R5.49;
//! GATE-G14 PASSED — **quote-only confirmation**; evidence spec §3's
//! denominators/missingness + AC-3-2's family separation, §4.2's
//! mechanism denominators, §9.2's cost rows and AC-9-3's
//! layer-separate closure).
//!
//! Records-in only: every leg evaluator folds *supplied* facts — the
//! nine committed packets' members echoed verbatim, the re-derived
//! denominators/tails/bounds/maxima and the freeze/`protocol_id`
//! identities — and never manufactures a stage, an execution or a
//! verdict. Recompute means: re-fold each committed packet's
//! disposition from its supplied leg verdicts, re-derive each reported
//! figure from the supplied subject facts, and assert byte/consistency
//! equality — never re-pick, never fabricate. `reported` is the honest
//! terminal state (the recompute matched the committed records); a
//! definitive mismatch folds `failed`, an undetermined fact
//! `inconclusive` — never a fabricated stage. Unavailable families
//! (finite-manifest efficacy, the sharp-null family, adoption) stay
//! unavailable — there is no confirmation evidence to report.

use hharness_identity::idp::idp_digest;
use hharness_wire::json::Json;

use crate::preflight::{Leg, LegVerdict};

/// The record version.
pub const VERSION: &str = "hh-study-report/1";
const DOMAIN: &str = "study_report";

/// The ten legs (frozen vocabulary — the recompute scope).
pub const LEGS: [&str; 10] = [
    "freeze_identities",
    "pre_flight",
    "useful_s1",
    "ops_live",
    "activation",
    "pilot",
    "parity",
    "confirmation",
    "families",
    "costs",
];

// ── the shared supplied-packet echo + refold ─────────────────────────

/// One committed packet's members, supplied verbatim — the report
/// recomputes over them; it never re-picks, never fabricates and
/// performs no committed-record checkout read.
#[derive(Debug, Clone, PartialEq)]
pub struct PacketEcho {
    /// The committed record's version (`hh-activation/1` …).
    pub version: String,
    /// Its recorded disposition.
    pub disposition: String,
    /// Its admission member.
    pub admission: String,
    /// Its `packet_id`/`preflight_id` digest (64-hex).
    pub packet_id: String,
    /// `leg → verdict-name` echo, in committed order.
    pub legs: Vec<(String, String)>,
    /// `leg → detail` echo for legs carrying one.
    pub details: Vec<(String, String)>,
}

impl PacketEcho {
    /// The supplied verdict name for `leg`.
    pub fn verdict_of(&self, leg: &str) -> Option<&str> {
        self.legs
            .iter()
            .find(|(n, _)| n == leg)
            .map(|(_, v)| v.as_str())
    }
    /// The supplied detail for `leg` (non-verified legs only).
    pub fn detail_of(&self, leg: &str) -> Option<&str> {
        self.details
            .iter()
            .find(|(n, _)| n == leg)
            .map(|(_, d)| d.as_str())
    }
}

/// `check_echo(echo, vocabulary, all_verified, definitive)` — the
/// byte-equality recompute over one committed packet: the supplied
/// leg set must equal the packet's frozen vocabulary (unique +
/// complete), every verdict name must sit in the closed codec, a
/// `refused`/`unsupported` leg re-folds `definitive`, an
/// `inconclusive` leg (none definitive) re-folds `inconclusive`, and
/// an all-verified set re-folds `all_verified` — the recomputed
/// disposition must equal the recorded disposition byte-for-byte.
/// `packet_id` must be a 64-hex digest; an absent member is
/// `inconclusive`, never relabelled.
pub fn check_echo(
    echo: &PacketEcho,
    vocabulary: &[&str],
    all_verified: &str,
    definitive: &str,
) -> LegVerdict {
    if echo.packet_id.len() != 64 || !echo.packet_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return LegVerdict::Inconclusive("packet_id_absent".to_string());
    }
    if echo.version.is_empty() || echo.admission.is_empty() || echo.disposition.is_empty() {
        return LegVerdict::Inconclusive("packet_member_absent".to_string());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut definitive_hit = false;
    let mut undetermined = false;
    for (name, verdict) in &echo.legs {
        if !vocabulary.contains(&name.as_str()) {
            return LegVerdict::Refused(format!("echo_leg_vocabulary:{name}"));
        }
        if !seen.insert(name.as_str()) {
            return LegVerdict::Refused(format!("echo_leg_duplicate:{name}"));
        }
        match verdict.as_str() {
            "refused" | "unsupported" => definitive_hit = true,
            "inconclusive" => undetermined = true,
            "verified" => {}
            other => return LegVerdict::Refused(format!("echo_verdict:{other}")),
        }
    }
    for name in vocabulary {
        if !seen.contains(name) {
            return LegVerdict::Inconclusive(format!("echo_leg_absent:{name}"));
        }
    }
    for (name, detail) in &echo.details {
        if detail.is_empty() {
            return LegVerdict::Refused(format!("echo_detail_empty:{name}"));
        }
        match echo.verdict_of(name) {
            Some(v) if v != "verified" => {}
            _ => return LegVerdict::Refused(format!("echo_detail_orphan:{name}")),
        }
    }
    let recomputed = if definitive_hit {
        definitive
    } else if undetermined {
        "inconclusive"
    } else {
        all_verified
    };
    if recomputed != echo.disposition {
        return LegVerdict::Refused(format!(
            "disposition:{}!=recomputed:{}",
            echo.disposition, recomputed
        ));
    }
    LegVerdict::Verified
}

// ── leg 1 — freeze_identities ────────────────────────────────────────

/// The re-materialized manifest + cross-packet freeze consistency —
/// the live recompute (`check_id`) and the byte-equality assertions,
/// never a re-pick.
#[derive(Debug, Clone, PartialEq)]
pub struct FreezeFacts {
    /// `check_id()` held on the re-materialized `golden()` decl (the
    /// live recompute, not an echo).
    pub check_id: bool,
    /// The recomputed `protocol_id` (echoed — never hard-coded).
    pub protocol_id: String,
    /// Recomputed identities (30/300/60/6 incl. `confirmation`).
    pub selected_tasks: u64,
    /// The interleaved slot count (300).
    pub schedule_slots: u64,
    /// The first-slot bindings (60).
    pub first_slots: u64,
    /// The phase-identity count (6).
    pub phase_identities: u64,
    /// `confirmation` among the phases.
    pub confirmation_phase: bool,
    /// The committed confirm packet's `manifest_frozen` `protocol_id`
    /// fact — the byte-equality target.
    pub committed_protocol_id: String,
    /// The committed `hh-activation/1` `freeze_id` (64-hex).
    pub activation_freeze_id: String,
    /// The committed `hh-pilot/1` `inherits_freeze_id` — must
    /// byte-equal the activation freeze.
    pub pilot_inherits_freeze_id: String,
    /// The committed `hh-pilot/1` `freeze_id` — a distinct 64-hex
    /// mint (never the activation freeze).
    pub pilot_freeze_id: String,
}

/// `evaluate_freeze` — the frozen identities recompute: `check_id`
/// holds, the recomputed identities equal their frozen literals, the
/// recomputed `protocol_id` byte-equals the committed fact, the
/// pilot's `inherits_freeze_id` byte-equals the activation
/// `freeze_id` and the pilot's own `freeze_id` is a distinct mint.
pub fn evaluate_freeze(f: &FreezeFacts) -> LegVerdict {
    for (member, id) in [
        ("protocol_id", &f.protocol_id),
        ("committed_protocol_id", &f.committed_protocol_id),
        ("activation_freeze_id", &f.activation_freeze_id),
        ("pilot_inherits_freeze_id", &f.pilot_inherits_freeze_id),
        ("pilot_freeze_id", &f.pilot_freeze_id),
    ] {
        if id.len() != 64 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
            return LegVerdict::Inconclusive(format!("{member}_absent"));
        }
    }
    if !f.check_id {
        return LegVerdict::Refused("protocol_id_mismatch".to_string());
    }
    for (member, have, need) in [
        (
            "selected_tasks",
            f.selected_tasks,
            crate::confirm::FROZEN_TASKS,
        ),
        (
            "schedule_slots",
            f.schedule_slots,
            crate::confirm::FROZEN_SLOTS,
        ),
        (
            "first_slots",
            f.first_slots,
            crate::confirm::FROZEN_FIRST_SLOTS,
        ),
        (
            "phase_identities",
            f.phase_identities,
            crate::confirm::FROZEN_PHASES,
        ),
    ] {
        if have != need {
            return LegVerdict::Refused(format!("{member}:{have}"));
        }
    }
    if !f.confirmation_phase {
        return LegVerdict::Refused("confirmation_phase_absent".to_string());
    }
    if f.protocol_id != f.committed_protocol_id {
        return LegVerdict::Refused("protocol_id_drift".to_string());
    }
    if f.pilot_inherits_freeze_id != f.activation_freeze_id {
        return LegVerdict::Refused("freeze_inherits_mismatch".to_string());
    }
    if f.pilot_freeze_id == f.activation_freeze_id {
        return LegVerdict::Refused("freeze_id_collision".to_string());
    }
    LegVerdict::Verified
}

// ── leg 2 — pre_flight ───────────────────────────────────────────────

/// The backend pre-flight stage — two committed records: the first
/// `inconclusive{backend_unavailable:podman}`, the reprobe
/// `inconclusive{probe_absent}` with `backend_candidate` verified
/// (the backend verified; the instrument probe stayed absent — the
/// carried machinery predicate, never relabelled).
#[derive(Debug, Clone, PartialEq)]
pub struct PreflightStageFacts {
    /// The first probe record (`inconclusive`).
    pub first: PacketEcho,
    /// The reprobe record (`inconclusive` — backend verified,
    /// `probe_absent`).
    pub reprobe: PacketEcho,
}

/// `evaluate_preflight` — both records refold `inconclusive`, the
/// reprobe's `backend_candidate` verifies and its `observer_probe`
/// stays `inconclusive{probe_absent}`; the first record's
/// `backend_candidate` stays `inconclusive{backend_unavailable}`.
pub fn evaluate_preflight(f: &PreflightStageFacts) -> LegVerdict {
    for (which, echo) in [("first", &f.first), ("reprobe", &f.reprobe)] {
        if echo.version != crate::preflight::VERSION {
            return LegVerdict::Refused(format!("{which}_version:{}", echo.version));
        }
        let v = check_echo(echo, &crate::preflight::LEGS, "feasible", "infeasible");
        if !v.is_verified() {
            return v;
        }
    }
    if f.first.verdict_of("backend_candidate") != Some("inconclusive") {
        return LegVerdict::Refused("first_backend:unexpected".to_string());
    }
    if f.reprobe.verdict_of("backend_candidate") != Some("verified") {
        return LegVerdict::Refused("reprobe_backend:unverified".to_string());
    }
    if f.reprobe.verdict_of("observer_probe") != Some("inconclusive")
        || f.reprobe.detail_of("observer_probe") != Some("probe_absent")
    {
        return LegVerdict::Refused("reprobe_probe:unexpected".to_string());
    }
    LegVerdict::Verified
}

// ── leg 3 — useful_s1 ────────────────────────────────────────────────

/// The S1 useful stage — the committed `failed` record (both
/// subjects `dispatch_outcome:unknown:executor_error`, the evidence
/// join refused, acceptance withheld).
#[derive(Debug, Clone, PartialEq)]
pub struct UsefulFacts {
    /// The committed `hh-s1-useful/1` record (`failed`).
    pub echo: PacketEcho,
    /// The committed `useful_acceptance` member
    /// (`withheld{no_independent_correct_artifact}`).
    pub acceptance: String,
    /// The two carried machinery predicates, verbatim.
    pub carried: Vec<String>,
    /// S1 stays unmet (`true`).
    pub s1_unmet: bool,
}

/// `evaluate_useful` — the record refolds `failed`, the acceptance
/// stays withheld (never a marked pass), both carried predicates are
/// named verbatim and S1 stays unmet.
pub fn evaluate_useful(f: &UsefulFacts) -> LegVerdict {
    if f.echo.version != crate::useful::VERSION {
        return LegVerdict::Refused(format!("version:{}", f.echo.version));
    }
    let v = check_echo(&f.echo, &crate::useful::LEGS, "executed", "failed");
    if !v.is_verified() {
        return v;
    }
    if f.echo.disposition != "failed" {
        return LegVerdict::Refused(format!("disposition:{}", f.echo.disposition));
    }
    if f.acceptance != "withheld{no_independent_correct_artifact}" {
        return LegVerdict::Refused(format!("acceptance:{}", f.acceptance));
    }
    for p in [
        crate::useful::PROBE_CARRIED,
        crate::useful::EXEC_BACKEND_CLOBBER,
    ] {
        if !f.carried.iter().any(|c| c == p) {
            return LegVerdict::Refused(format!("carried_absent:{p}"));
        }
    }
    if !f.s1_unmet {
        return LegVerdict::Refused("s1_met".to_string());
    }
    LegVerdict::Verified
}

// ── leg 4 — ops_live ─────────────────────────────────────────────────

/// The bounded operational stage — the first committed record
/// `failed` (o3/o6/o8/stage_report/repairs refused; retained, never
/// rewritten) and the second `executed` (11/11 verified, both
/// carried machinery predicates discharged).
#[derive(Debug, Clone, PartialEq)]
pub struct OpsFacts {
    /// The first record (`failed`).
    pub first: PacketEcho,
    /// The second record (`executed`).
    pub second: PacketEcho,
    /// The second packet's `discharged_predicates` (verbatim).
    pub discharged: Vec<String>,
    /// `stage_report.acceptance_blocked` in the second packet
    /// (`false`).
    pub acceptance_blocked: bool,
    /// Retained attempt identities in the second packet (24 —
    /// 8 cells × 3 reps).
    pub attempts: u64,
    /// The frozen permutation seed.
    pub seed: u64,
}

/// `evaluate_ops` — the first record refolds `failed`, the second
/// refolds `executed` with every leg verified, both discharged
/// predicates verbatim, acceptance unblocked, 24 attempts, the
/// frozen seed echoed.
pub fn evaluate_ops(f: &OpsFacts) -> LegVerdict {
    for (which, echo) in [("first", &f.first), ("second", &f.second)] {
        if echo.version != crate::opslive::VERSION {
            return LegVerdict::Refused(format!("{which}_version:{}", echo.version));
        }
        let v = check_echo(echo, &crate::opslive::LEGS, "executed", "failed");
        if !v.is_verified() {
            return v;
        }
    }
    if f.first.disposition != "failed" {
        return LegVerdict::Refused(format!("first_disposition:{}", f.first.disposition));
    }
    if f.second.disposition != "executed" {
        return LegVerdict::Refused(format!("second_disposition:{}", f.second.disposition));
    }
    if !f.second.legs.iter().all(|(_, v)| v == "verified") {
        return LegVerdict::Refused("second_legs_unverified".to_string());
    }
    let mut discharged = f.discharged.clone();
    discharged.sort();
    let mut expected: Vec<String> = crate::opslive::DISCHARGED
        .iter()
        .map(|s| s.to_string())
        .collect();
    expected.sort();
    if discharged != expected {
        return LegVerdict::Refused(format!("discharged:{}", discharged.join(",")));
    }
    if f.acceptance_blocked {
        return LegVerdict::Refused("acceptance_blocked:true".to_string());
    }
    if f.attempts != 24 {
        return LegVerdict::Refused(format!("attempts:{}", f.attempts));
    }
    if f.seed == 0 {
        return LegVerdict::Inconclusive("seed_absent".to_string());
    }
    LegVerdict::Verified
}

// ── legs 5–6 — activation / pilot (the mechanism recompute) ─────────

/// One activation/pilot subject's supplied §4.2 facts — the
/// recompute input, verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct SubjectEcho {
    /// `pre_dispatch` outcome verbatim (`observed:applied` ⇒ an
    /// eligible pressure opportunity).
    pub pre_dispatch: String,
    /// `compaction_status` verbatim (`applied` ⇒ started +
    /// completed).
    pub compaction_status: String,
    /// `request_changed == Some(true)` with a next dispatch (⇒ a
    /// changed request / triggered first subject).
    pub request_changed: bool,
}

/// The §4.2 activation/fidelity denominators — one shape shared by
/// the reported fold and the recompute.
#[derive(Debug, Clone, PartialEq)]
pub struct FidelityFold {
    /// Eligible pressure opportunities.
    pub opportunities: u64,
    /// Compactions started.
    pub starts: u64,
    /// Compactions completed (`applied`).
    pub completions: u64,
    /// Completions whose next request changed with a next dispatch.
    pub changed_requests: u64,
    /// First subjects that triggered.
    pub triggered_first: u64,
    /// All first subjects.
    pub all_first: u64,
}

/// `recompute_fidelity` — re-derive the §4.2 denominators from the
/// supplied subject facts: opportunities = `observed:applied`
/// pre-dispatches; starts = a non-`none` compaction status;
/// completions = `applied`; changed/triggered = the
/// request-changed-with-dispatch conjunction; `all_first` = the
/// supplied subject count.
pub fn recompute_fidelity(subjects: &[SubjectEcho]) -> FidelityFold {
    FidelityFold {
        opportunities: subjects
            .iter()
            .filter(|s| s.pre_dispatch == "observed:applied")
            .count() as u64,
        starts: subjects
            .iter()
            .filter(|s| !s.compaction_status.is_empty() && s.compaction_status != "none")
            .count() as u64,
        completions: subjects
            .iter()
            .filter(|s| s.compaction_status == "applied")
            .count() as u64,
        changed_requests: subjects.iter().filter(|s| s.request_changed).count() as u64,
        triggered_first: subjects.iter().filter(|s| s.request_changed).count() as u64,
        all_first: subjects.len() as u64,
    }
}

/// The activation stage — the committed `executed` record, its
/// reported `activation_fidelity` fold and the withheld-pilot
/// member.
#[derive(Debug, Clone, PartialEq)]
pub struct ActivationFacts {
    /// The committed `hh-activation/1` record (`executed`).
    pub echo: PacketEcho,
    /// The reported `activation_fidelity` fold, verbatim.
    pub reported: FidelityFold,
    /// The two subjects' supplied facts.
    pub subjects: Vec<SubjectEcho>,
    /// The committed `pilot` member
    /// (`withheld{reading_2_pending}`).
    pub pilot_member: String,
    /// The costs leg's `attempts` (2).
    pub attempts: u64,
    /// The costs leg's `exposure_micros` (0).
    pub exposure_micros: i64,
}

/// `evaluate_activation` — the record refolds `executed`; the §4.2
/// denominators re-derived from the supplied subjects equal the
/// reported fold; the pilot member stays
/// `withheld{reading_2_pending}`; attempts cover every subject at
/// $0.00.
pub fn evaluate_activation(f: &ActivationFacts) -> LegVerdict {
    if f.echo.version != crate::activation::VERSION {
        return LegVerdict::Refused(format!("version:{}", f.echo.version));
    }
    let v = check_echo(&f.echo, &crate::activation::LEGS, "executed", "failed");
    if !v.is_verified() {
        return v;
    }
    let recomputed = recompute_fidelity(&f.subjects);
    if recomputed != f.reported {
        return LegVerdict::Refused(format!(
            "denominators:{}:{}:{}:{}:{}:{}",
            recomputed.opportunities,
            recomputed.starts,
            recomputed.completions,
            recomputed.changed_requests,
            recomputed.triggered_first,
            recomputed.all_first
        ));
    }
    if f.pilot_member != "withheld{reading_2_pending}" {
        return LegVerdict::Refused(format!("pilot_member:{}", f.pilot_member));
    }
    if f.attempts != f.subjects.len() as u64 {
        return LegVerdict::Refused(format!("attempts:{}", f.attempts));
    }
    if f.exposure_micros != 0 {
        return LegVerdict::Refused(format!("exposure:{}", f.exposure_micros));
    }
    LegVerdict::Verified
}

// ── leg 6 — pilot ────────────────────────────────────────────────────

/// One pilot subject's supplied facts — the §4.2 fidelity fields
/// plus the feasibility/tail measures, verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct PilotSubjectEcho {
    /// `arm:a` | `arm:b`.
    pub arm: String,
    /// The §4.2 fidelity facts (reused shape).
    pub fidelity: SubjectEcho,
    /// §3.1 outcome class (`completed` …).
    pub outcome_class: String,
    /// Wall ms (`None` right-censored — a lower bound).
    pub wall_ms: Option<u64>,
    /// `tokens_freed`.
    pub tokens_freed: u64,
    /// Mediated dispatches.
    pub calls: u64,
    /// Input tokens.
    pub input_tokens: u64,
    /// Output bytes.
    pub output_bytes: u64,
    /// Setup retries.
    pub setup_retries: u32,
}

/// One arm's reported tail measures — nearest-rank P50/P95/max wall
/// and the tokens_freed spread.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmTailEcho {
    /// Measured subjects (≤R).
    pub n: u64,
    /// Nearest-rank P50 wall_ms.
    pub wall_p50_ms: u64,
    /// Nearest-rank P95 wall_ms.
    pub wall_p95_ms: u64,
    /// Observed max wall_ms.
    pub wall_max_ms: u64,
    /// Min tokens_freed.
    pub tokens_min: u64,
    /// Max tokens_freed.
    pub tokens_max: u64,
}

/// The reported §3.2 missingness fold, verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct MissingnessEcho {
    /// Planned first slots M (10).
    pub planned: u64,
    /// s — completed.
    pub completed: u64,
    /// f — infrastructure failures.
    pub failed: u64,
    /// u — attempted but unresolved.
    pub unresolved: u64,
    /// Retained lower bound s/M in ppm.
    pub lower_ppm: u64,
    /// Retained upper bound (s+u)/M in ppm.
    pub upper_ppm: u64,
    /// Unplanned subjects attempted (0).
    pub unplanned_attempted: u64,
}

/// The reported §9.2 feasibility fold, verbatim — the measured
/// maxima and the success counts.
#[derive(Debug, Clone, PartialEq)]
pub struct FeasibilityEcho {
    /// First subjects run.
    pub subjects_run: u64,
    /// Cell attempts.
    pub cells_attempted: u64,
    /// Cells provisioned.
    pub cells_provisioned: u64,
    /// `observed:applied` dispatches.
    pub dispatches_applied: u64,
    /// `sealed` workspaces.
    pub seals: u64,
    /// `applied` compactions.
    pub compactions_applied: u64,
    /// Worst per-subject setup retries.
    pub max_setup_retries: u32,
    /// Worst per-subject dispatches.
    pub max_calls: u64,
    /// Worst per-subject input tokens.
    pub max_input_tokens: u64,
    /// Worst per-subject output bytes.
    pub max_output_bytes: u64,
    /// Worst per-subject wall ms.
    pub max_wall_ms: u64,
    /// Peak concurrent subjects.
    pub max_concurrent: u64,
}

/// The pilot stage — the committed `executed` record and every
/// reported fold, verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct PilotFacts {
    /// The committed `hh-pilot/1` record (`executed`).
    pub echo: PacketEcho,
    /// The reported `fidelity` fold.
    pub fidelity: FidelityFold,
    /// The reported `variance_tail` arm measures.
    pub arm_a: ArmTailEcho,
    /// Arm B's reported tail.
    pub arm_b: ArmTailEcho,
    /// Right-censored wall subjects.
    pub censored_wall: u64,
    /// Sign counts over `tokens_freed`.
    pub sign_pos: u64,
    /// Zero `tokens_freed` subjects.
    pub sign_zero: u64,
    /// The reported `missingness` fold.
    pub missingness: MissingnessEcho,
    /// The reported `feasibility` fold.
    pub feasibility: FeasibilityEcho,
    /// Costs: attempts (10).
    pub attempts: u64,
    /// Costs: exposure micros (0).
    pub exposure_micros: i64,
    /// The worksheet's pilot state (`ready_for_gate`).
    pub pilot_state: String,
    /// Any contrast/effect claim attempted (`false`).
    pub contrast_claimed: bool,
    /// Any policy tuning/search performed (`false`).
    pub tuning_performed: bool,
    /// The ten subjects' supplied facts.
    pub subjects: Vec<PilotSubjectEcho>,
}

/// `nearest_rank(sorted, rank_ppm)` — the nearest-rank order
/// statistic: `ceil(rank·n)`-th element of the ascending samples
/// (1-indexed; `0` on empty).
fn nearest_rank(sorted: &[u64], rank_ppm: u64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let n = sorted.len() as u64;
    let k = (rank_ppm * n).div_ceil(1_000_000).max(1);
    sorted[(k - 1) as usize]
}

/// `recompute_arm_tail(subjects)` — the nearest-rank P50/P95/max
/// wall measures and the `tokens_freed` spread over one arm's
/// measured subjects.
pub fn recompute_arm_tail<'a>(subjects: impl Iterator<Item = &'a PilotSubjectEcho>) -> ArmTailEcho {
    let mut walls: Vec<u64> = Vec::new();
    let mut tokens: Vec<u64> = Vec::new();
    for s in subjects {
        if let Some(w) = s.wall_ms {
            walls.push(w);
            tokens.push(s.tokens_freed);
        }
    }
    walls.sort_unstable();
    tokens.sort_unstable();
    ArmTailEcho {
        n: walls.len() as u64,
        wall_p50_ms: nearest_rank(&walls, 500_000),
        wall_p95_ms: nearest_rank(&walls, 950_000),
        wall_max_ms: walls.last().copied().unwrap_or(0),
        tokens_min: tokens.first().copied().unwrap_or(0),
        tokens_max: tokens.last().copied().unwrap_or(0),
    }
}

/// `recompute_missingness(subjects, planned)` — the §3.2
/// `[s/M,(s+u)/M]` all-planned bound re-derived from the supplied
/// outcome classes.
pub fn recompute_missingness(subjects: &[PilotSubjectEcho], planned: u64) -> MissingnessEcho {
    let count = |class: &str| subjects.iter().filter(|s| s.outcome_class == class).count() as u64;
    let completed = count("completed");
    let failed = count("infrastructure_failure");
    let unresolved = count("unresolved");
    let unplanned_attempted = (subjects.len() as u64).saturating_sub(planned);
    let (lower_ppm, upper_ppm) = if planned == 0 {
        (0, 0)
    } else {
        (
            completed * 1_000_000 / planned,
            (completed + unresolved) * 1_000_000 / planned,
        )
    };
    MissingnessEcho {
        planned,
        completed,
        failed,
        unresolved,
        lower_ppm,
        upper_ppm,
        unplanned_attempted,
    }
}

/// `recompute_maxima(subjects)` — the per-subject envelope maxima
/// re-derived from the supplied facts.
pub fn recompute_maxima(subjects: &[PilotSubjectEcho]) -> (u32, u64, u64, u64, u64) {
    (
        subjects.iter().map(|s| s.setup_retries).max().unwrap_or(0),
        subjects.iter().map(|s| s.calls).max().unwrap_or(0),
        subjects.iter().map(|s| s.input_tokens).max().unwrap_or(0),
        subjects.iter().map(|s| s.output_bytes).max().unwrap_or(0),
        subjects.iter().filter_map(|s| s.wall_ms).max().unwrap_or(0),
    )
}

/// `evaluate_pilot` — the record refolds `executed`; the §4.2
/// denominators, both arms' nearest-rank tails, the sign/censor
/// counts, the §3.2 bound and every envelope maximum re-derived from
/// the supplied subjects equal the reported folds — and every
/// maximum sits inside the §9.2 envelope; no contrast claim, no
/// tuning, $0.00.
pub fn evaluate_pilot(f: &PilotFacts) -> LegVerdict {
    if f.echo.version != crate::pilot::VERSION {
        return LegVerdict::Refused(format!("version:{}", f.echo.version));
    }
    let v = check_echo(&f.echo, &crate::pilot::LEGS, "executed", "failed");
    if !v.is_verified() {
        return v;
    }
    if f.contrast_claimed || f.tuning_performed {
        return LegVerdict::Refused("pilot_became_inference".to_string());
    }
    let subjects: Vec<SubjectEcho> = f.subjects.iter().map(|s| s.fidelity.clone()).collect();
    let fidelity = recompute_fidelity(&subjects);
    if fidelity != f.fidelity || fidelity.all_first != crate::pilot::DECLARED_FIRST {
        return LegVerdict::Refused("pilot_denominators".to_string());
    }
    for (name, arm, reported) in [("arm_a", "arm:a", &f.arm_a), ("arm_b", "arm:b", &f.arm_b)] {
        let recomputed = recompute_arm_tail(f.subjects.iter().filter(|s| s.arm == arm));
        if recomputed != *reported {
            return LegVerdict::Refused(format!("{name}:tail"));
        }
    }
    let censored = f.subjects.iter().filter(|s| s.wall_ms.is_none()).count() as u64;
    let sign_pos = f.subjects.iter().filter(|s| s.tokens_freed > 0).count() as u64;
    let sign_zero = f.subjects.iter().filter(|s| s.tokens_freed == 0).count() as u64;
    if censored != f.censored_wall || sign_pos != f.sign_pos || sign_zero != f.sign_zero {
        return LegVerdict::Refused("tail_counts".to_string());
    }
    if f.subjects.iter().any(|s| s.outcome_class == "scored") {
        return LegVerdict::Refused("outcome_class:scored".to_string());
    }
    let missingness = recompute_missingness(&f.subjects, crate::pilot::DECLARED_FIRST);
    if missingness != f.missingness {
        return LegVerdict::Refused("missingness_bound".to_string());
    }
    let (mr, mc, mi, mo, mw) = recompute_maxima(&f.subjects);
    let fe = &f.feasibility;
    if mr != fe.max_setup_retries
        || mc != fe.max_calls
        || mi != fe.max_input_tokens
        || mo != fe.max_output_bytes
        || mw != fe.max_wall_ms
    {
        return LegVerdict::Refused("envelope_maxima".to_string());
    }
    use crate::pilot::envelope as env;
    if fe.subjects_run != crate::pilot::DECLARED_FIRST
        || fe.max_setup_retries > env::SETUP_RETRIES
        || fe.max_calls > env::CALLS_PER_SUBJECT
        || fe.max_input_tokens > env::INPUT_TOKENS
        || fe.max_output_bytes > env::OUTPUT_TOKENS
        || fe.max_wall_ms > env::WALL_MS
        || fe.max_concurrent > env::CONCURRENT
    {
        return LegVerdict::Refused("envelope_breach".to_string());
    }
    if f.attempts != crate::pilot::DECLARED_FIRST || f.exposure_micros != 0 {
        return LegVerdict::Refused("pilot_costs".to_string());
    }
    if f.pilot_state != "ready_for_gate" {
        return LegVerdict::Refused(format!("pilot_state:{}", f.pilot_state));
    }
    LegVerdict::Verified
}

// ── legs 7–8 — parity / confirmation (the withheld records) ─────────

/// The withheld parity stage — the committed `hh-parity-n0/1`
/// record.
#[derive(Debug, Clone, PartialEq)]
pub struct ParityStageFacts {
    /// The committed `hh-parity-n0/1` record (`withheld`).
    pub echo: PacketEcho,
    /// `n_declared` (0).
    pub n_declared: u64,
    /// `s3_unmet` (`true`).
    pub s3_unmet: bool,
    /// `confirmation_blocked` (`true`).
    pub confirmation_blocked: bool,
    /// The recorded gate (`GATE-G13`).
    pub gate: String,
}

/// `evaluate_parity_stage` — the record refolds `withheld`; N=0,
/// S3 unmet and the dependent-confirmation block echoed; no claim
/// of either sign.
pub fn evaluate_parity_stage(f: &ParityStageFacts) -> LegVerdict {
    if f.echo.version != crate::parity::VERSION {
        return LegVerdict::Refused(format!("version:{}", f.echo.version));
    }
    let v = check_echo(&f.echo, &crate::parity::LEGS, "withheld", "failed");
    if !v.is_verified() {
        return v;
    }
    if f.gate != crate::parity::GATE {
        return LegVerdict::Refused(format!("gate:{}", f.gate));
    }
    if f.n_declared != 0 {
        return LegVerdict::Refused(format!("n_declared:{}", f.n_declared));
    }
    if !f.s3_unmet || !f.confirmation_blocked {
        return LegVerdict::Refused("upstream_block_unrecorded".to_string());
    }
    LegVerdict::Verified
}

/// The withheld confirmation stage — the committed
/// `hh-confirm-withheld/1` record.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmStageFacts {
    /// The committed `hh-confirm-withheld/1` record (`withheld`).
    pub echo: PacketEcho,
    /// `subjects_first` (0).
    pub subjects_first: u64,
    /// `tranche_absent` (`true`).
    pub tranche_absent: bool,
    /// `parity_absent` (`true`).
    pub parity_absent: bool,
    /// `confirmation_blocked` (`true`).
    pub confirmation_blocked: bool,
    /// `s4_unmet` (`true`).
    pub s4_unmet: bool,
    /// The recorded gate (`GATE-G14`).
    pub gate: String,
}

/// `evaluate_confirm_stage` — the record refolds `withheld`; the
/// absent tranche, the parity-absent dependent block and S4 unmet
/// echoed.
pub fn evaluate_confirm_stage(f: &ConfirmStageFacts) -> LegVerdict {
    if f.echo.version != crate::confirm::VERSION {
        return LegVerdict::Refused(format!("version:{}", f.echo.version));
    }
    let v = check_echo(&f.echo, &crate::confirm::LEGS, "withheld", "failed");
    if !v.is_verified() {
        return v;
    }
    if f.gate != crate::confirm::GATE {
        return LegVerdict::Refused(format!("gate:{}", f.gate));
    }
    if f.subjects_first != 0 {
        return LegVerdict::Refused(format!("subjects_first:{}", f.subjects_first));
    }
    if !f.tranche_absent || !f.parity_absent || !f.confirmation_blocked || !f.s4_unmet {
        return LegVerdict::Refused("member_unrecorded".to_string());
    }
    LegVerdict::Verified
}

// ── leg 9 — families (the REQ-D-009 separation) ─────────────────────

/// The required honest family statuses (the committed-evidence
/// state): a missing margin, opportunity or admission is never a
/// pass — the unavailable families stay unavailable.
pub const REQUIRED_FAMILIES: [(&str, &str); 9] = [
    ("efficacy", "unavailable{confirmation_tranche_absent}"),
    ("task_mixture", "descriptive{pilot_recomputed}"),
    ("sharp_null", "unavailable{confirmation_tranche_absent}"),
    ("parity", "withheld{n_declared:0}"),
    ("mechanism", "executed{activation,pilot}"),
    ("engineering", "executed{ops_live;backend_verified}"),
    ("task_truth", "failed{s1_useful}"),
    ("operations", "executed{ops_live}"),
    ("adoption", "unavailable{not_started}"),
];

/// The supplied family statuses — engineered capability, task
/// truth, bounded operation, scientific effect and adoption
/// distinguished (REQ-D-009 / AC-3-2), the unavailable families
/// retained verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyFacts {
    /// Finite-manifest efficacy.
    pub efficacy: String,
    /// Task-mixture sensitivity.
    pub task_mixture: String,
    /// The sharp-null family.
    pub sharp_null: String,
    /// Parity/equivalence.
    pub parity: String,
    /// Mechanism.
    pub mechanism: String,
    /// Engineered capability.
    pub engineering: String,
    /// Task truth (S1).
    pub task_truth: String,
    /// Bounded operation.
    pub operations: String,
    /// Adoption.
    pub adoption: String,
    /// Any claim of either sign anywhere in the record (`false`).
    pub claim_present: bool,
}

/// `evaluate_families` — every family's supplied status equals its
/// required honest spelling; no pass/met/claim wording may appear in
/// an unavailable family's status; no claim of either sign is
/// present.
pub fn evaluate_families(f: &FamilyFacts) -> LegVerdict {
    let supplied: [(&str, &str); 9] = [
        ("efficacy", &f.efficacy),
        ("task_mixture", &f.task_mixture),
        ("sharp_null", &f.sharp_null),
        ("parity", &f.parity),
        ("mechanism", &f.mechanism),
        ("engineering", &f.engineering),
        ("task_truth", &f.task_truth),
        ("operations", &f.operations),
        ("adoption", &f.adoption),
    ];
    for (name, required) in REQUIRED_FAMILIES {
        let have = supplied
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, s)| *s)
            .unwrap_or("");
        if have.is_empty() {
            return LegVerdict::Inconclusive(format!("family_absent:{name}"));
        }
        for claim in [
            "passed",
            "satisfied",
            "met{",
            "positive",
            "negative",
            "equivalent",
        ] {
            if have.contains(claim) && !required.contains(claim) {
                return LegVerdict::Refused(format!("{name}:claim_spelling:{have}"));
            }
        }
        if have != required {
            return LegVerdict::Refused(format!("{name}:{have}"));
        }
    }
    if f.claim_present {
        return LegVerdict::Refused("claim_present".to_string());
    }
    LegVerdict::Verified
}

// ── leg 10 — costs ───────────────────────────────────────────────────

/// The cost rollup — every committed packet's exposure plus the
/// executed/gated subject counts.
#[derive(Debug, Clone, PartialEq)]
pub struct CostFacts {
    /// `packet → exposure_micros` across the committed packets.
    pub per_packet: Vec<(String, i64)>,
    /// Executed first subjects (2 activation + 10 pilot = 12).
    pub subjects_first: u64,
    /// Parity + confirmation first subjects (0).
    pub gated_subjects: u64,
}

/// `evaluate_costs` — every committed packet's exposure is `0`, the
/// recomputed total is USD $0.00, 12 executed first subjects, zero
/// gated subjects.
pub fn evaluate_costs(f: &CostFacts) -> LegVerdict {
    let mut total: i64 = 0;
    for (packet, micros) in &f.per_packet {
        if *micros != 0 {
            return LegVerdict::Refused(format!("exposure:{packet}:{micros}"));
        }
        total += micros;
    }
    if f.per_packet.len() != 9 {
        return LegVerdict::Inconclusive(format!("packets:{}", f.per_packet.len()));
    }
    if total != 0 {
        return LegVerdict::Refused(format!("total_micros:{total}"));
    }
    if f.subjects_first != 12 {
        return LegVerdict::Refused(format!("subjects_first:{}", f.subjects_first));
    }
    if f.gated_subjects != 0 {
        return LegVerdict::Refused(format!("gated_subjects:{}", f.gated_subjects));
    }
    LegVerdict::Verified
}

// ── the report ───────────────────────────────────────────────────────

/// The packet disposition — `reported` only when every leg
/// verified: the recompute matched the committed records (never
/// `satisfied`/`complete` — the study stays unmet).
#[derive(Debug, Clone, PartialEq)]
pub enum Disposition {
    /// Every leg `verified` — the recompute report is honest.
    Reported,
    /// At least one `unsupported`/`refused` leg — a definitive
    /// mismatch with the committed records.
    Failed,
    /// At least one `inconclusive` leg, none failed — undetermined.
    Inconclusive,
}

impl Disposition {
    /// The closed vocabulary.
    pub fn name(&self) -> &'static str {
        match self {
            Disposition::Reported => "reported",
            Disposition::Failed => "failed",
            Disposition::Inconclusive => "inconclusive",
        }
    }
}

/// The `hh-study-report/1` record — the admitted-study recompute:
/// each stage's honest status refolded and re-derived, the family
/// separation with unavailable families retained, the $0.00 rollup
/// and the study's unmet status recorded; `packet_id` minted last
/// over the canonical record.
#[derive(Debug, Clone, PartialEq)]
pub struct StudyReport {
    /// `H(canonical(record minus packet_id))` under `study_report`.
    pub packet_id: String,
    /// The record version.
    pub version: String,
    /// The ten legs.
    pub legs: Vec<Leg>,
    /// The folded disposition.
    pub disposition: Disposition,
    /// Executed first subjects across admitted packets (12).
    pub subjects_first: u64,
    /// Total declared exposure in currency micros (0).
    pub exposure_micros: i64,
    /// S1 stays unmet (`true`).
    pub s1_unmet: bool,
    /// S3 stays unmet (`true`).
    pub s3_unmet: bool,
    /// S4 stays unmet (`true`).
    pub s4_unmet: bool,
    /// The study's honest status —
    /// `unmet{efficacy_unavailable;parity_withheld}`.
    pub study: String,
    /// The admission shape — `recompute_only`, never an execution.
    pub admission: String,
}

impl StudyReport {
    /// `to_json` — the canonical record.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("version", Json::str(&self.version)),
            ("packet_id", Json::str(&self.packet_id)),
            ("subjects_first", Json::Int(self.subjects_first as i64)),
            ("exposure_micros", Json::Int(self.exposure_micros)),
            ("s1_unmet", Json::Bool(self.s1_unmet)),
            ("s3_unmet", Json::Bool(self.s3_unmet)),
            ("s4_unmet", Json::Bool(self.s4_unmet)),
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

/// `assemble(legs)` — fold the ten leg verdicts into the
/// disposition and mint `packet_id` last over the canonical record.
/// Every leg name must be a `LEGS` member, unique and complete — a
/// malformed leg set is a `refused` assembly, never a partial
/// packet.
pub fn assemble(legs: Vec<Leg>) -> Result<StudyReport, String> {
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
        Disposition::Reported
    };
    let mut out = StudyReport {
        packet_id: String::new(),
        version: VERSION.to_string(),
        legs,
        subjects_first: 12,
        exposure_micros: 0,
        s1_unmet: true,
        s3_unmet: true,
        s4_unmet: true,
        study: "unmet{efficacy_unavailable;parity_withheld}".to_string(),
        disposition,
        admission: "recompute_only".to_string(),
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("packet_id");
    }
    out.packet_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}
