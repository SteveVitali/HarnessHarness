//! `hh-study-report/1` — the admitted-study recompute report (R5.49;
//! recomputed 2026-11-06 over the executed parity + the completed
//! confirmation cohort — GATE-G13's N=0 record and GATE-G14's
//! **quote-only confirmation** record stand as committed history;
//! evidence spec §3's denominators/missingness + AC-3-2's family
//! separation, §4.2's mechanism denominators, §4.3's finite-manifest
//! contrast, §9.2's cost rows and AC-9-3's layer-separate closure).
//!
//! Records-in only: every leg evaluator folds *supplied* facts — the
//! committed packets' members echoed verbatim, the re-derived
//! denominators/tails/bounds/maxima and the freeze/`protocol_id`
//! identities — and never manufactures a stage, an execution or a
//! verdict. Recompute means: re-fold each committed packet's
//! disposition from its supplied leg verdicts, re-derive each reported
//! figure from the supplied subject facts, and assert byte/consistency
//! equality — never re-pick, never fabricate. `reported` is the honest
//! terminal state (the recompute matched the committed records); a
//! definitive mismatch folds `failed`, an undetermined fact
//! `inconclusive` — never a fabricated stage. The parity tranche
//! verified 30/30 directional agreement (`hh-parity-tranche-report/1`
//! — the two earlier confounded tranches stand verbatim, never
//! deleted); the confirmation cohort executed **300/300** — four
//! named epochs (the frozen run + the egress, s4-completion-gce and
//! s4-30slot-repairs-gce supplements) each covering exactly the prior
//! residue, all four packets standing byte-untouched. Under GATE-G19's
//! complete-cohort admission the efficacy leg carries the registered
//! finite-manifest contrast (REQ-D-107/109) per declared stratum —
//! its supplied cell table re-adds to the committed receipt fold;
//! the sharp-null sign-flip stays `unavailable{not_computed}` (not
//! part of the admitted fold).

use hharness_identity::idp::idp_digest;
use hharness_wire::json::Json;

use crate::preflight::{Leg, LegVerdict};

/// The record version.
pub const VERSION: &str = "hh-study-report/1";
const DOMAIN: &str = "study_report";

/// The eleven legs (frozen vocabulary — the recompute scope; the
/// `efficacy` leg lands with the completed cohort — GATE-G19's
/// complete-cohort admission, R5.59).
pub const LEGS: [&str; 11] = [
    "freeze_identities",
    "pre_flight",
    "useful_s1",
    "ops_live",
    "activation",
    "pilot",
    "parity",
    "confirmation",
    "efficacy",
    "families",
    "costs",
];

/// The vocabulary at the 2026-11-04 and 2026-11-06 committed packets
/// (the `efficacy` leg landed later, R5.59) — a committed packet
/// refolds its own era's legs, never the current spine's.
pub const LEGS_V1: [&str; 10] = [
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
/// `backend_candidate`/`observer_probe` stay
/// `inconclusive{backend_unavailable:podman}`/`inconclusive{probe_absent}`
/// — the committed detail spellings, byte-echoed.
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
    if f.first.verdict_of("backend_candidate") != Some("inconclusive")
        || f.first.detail_of("backend_candidate") != Some("backend_unavailable:podman")
        || f.first.detail_of("observer_probe") != Some("probe_absent")
    {
        return LegVerdict::Refused("first_record:unexpected".to_string());
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

/// The S1 useful stage after the executed re-execution — R6.3 (P1-D,
/// REQ-XA-117): the committed `failed` record (R5.43) stands as
/// history beside the executed DF-CAP5.1-1 `hh-s1-useful/1` packet
/// (`disposition: executed`, `useful_acceptance: marked`, packet_id
/// `7bbd15a2…`). `s1_unmet` records the S1 *objective* — the bounded
/// `bounded_s1_execution` admission discharges only its bounded slice
/// (full-target predicates stay owed per the coverage register; the
/// useful-task card promotion is a gated operator decision), so the
/// objective stays unmet and the flag stays `true`.
#[derive(Debug, Clone, PartialEq)]
pub struct UsefulReexecFacts {
    /// The committed failed record (history — refolds `failed`).
    pub failed: PacketEcho,
    /// The committed executed record (refolds `executed`; every leg
    /// verified).
    pub executed: PacketEcho,
    /// The executed packet's `useful_acceptance` member (`marked`).
    pub acceptance: String,
    /// The two carried machinery predicates, verbatim.
    pub carried: Vec<String>,
    /// The S1 objective stays unmet (`true` — bounded slice only; the
    /// promotion is a gated decision).
    pub s1_unmet: bool,
}

/// `evaluate_useful_reexec` — the failed record still refolds `failed`
/// (committed history); the executed record refolds `executed` with
/// every leg verified, its acceptance is `marked`, both carried
/// predicates are named verbatim, and the objective flag stays `true`.
pub fn evaluate_useful_reexec(f: &UsefulReexecFacts) -> LegVerdict {
    for (which, echo) in [("failed", &f.failed), ("executed", &f.executed)] {
        if echo.version != crate::useful::VERSION {
            return LegVerdict::Refused(format!("{which}_version:{}", echo.version));
        }
        let v = check_echo(echo, &crate::useful::LEGS, "executed", "failed");
        if !v.is_verified() {
            return v;
        }
    }
    if f.failed.disposition != "failed" {
        return LegVerdict::Refused(format!("failed_disposition:{}", f.failed.disposition));
    }
    if f.executed.disposition != "executed" {
        return LegVerdict::Refused(format!("executed_disposition:{}", f.executed.disposition));
    }
    if !f.executed.legs.iter().all(|(_, v)| v == "verified") {
        return LegVerdict::Refused("executed_legs_unverified".to_string());
    }
    if f.acceptance != "marked" {
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

// ── legs 7–8 — parity / confirmation (the executed records) ─────────

/// The parity-tranche packet version (`hh-parity-tranche-report/1` —
/// the committed tranche-3 artifact `2026-11-05-parity-tranche-3.json`).
pub const PARITY_TRANCHE_VERSION: &str = "hh-parity-tranche-report/1";

/// The confirmation packet version (`hh-s4-confirmation/1` — the
/// committed frozen-run artifact `2026-11-05-s4-confirmation.json`).
pub const CONFIRM_VERSION: &str = "hh-s4-confirmation/1";

/// The egress-supplement packet version (`hh-s4-egress-supplement/1`
/// — the committed named supplement `2026-11-05-s4-egress-cohort.json`).
pub const CONFIRM_SUPPLEMENT_VERSION: &str = "hh-s4-egress-supplement/1";

/// The completion-epoch packet version (`hh-s4-completion/1` — the
/// committed `2026-11-06-s4-completion.json`, epoch `s4-completion-gce`).
pub const CONFIRM_COMPLETION_VERSION: &str = "hh-s4-completion/1";

/// The repairs-epoch packet version (`hh-s4-30slot-repairs/1` — the
/// committed `2026-11-06-s4-30slot-repairs.json`, epoch
/// `s4-30slot-repairs-gce`).
pub const CONFIRM_REPAIRS_VERSION: &str = "hh-s4-30slot-repairs/1";

/// The admitted supplement-epoch schemas — each a named second-attempt
/// epoch over exactly the prior residue (the original records stand;
/// denominators re-add verbatim, never inflated).
const EPOCH_VERSIONS: [&str; 3] = [
    CONFIRM_SUPPLEMENT_VERSION,
    CONFIRM_COMPLETION_VERSION,
    CONFIRM_REPAIRS_VERSION,
];

/// The parity stage — the committed `hh-parity-n0/1` withheld record
/// (history — refolds `withheld`, discharged by real evidence, never
/// rewritten) plus the executed tranche packet's supplied members.
/// The two earlier confounded tranches
/// (`infeasible{margin}` — disk confound / ANSI parse break) stand
/// committed verbatim; the tranche-3 packet is the admitted one.
#[derive(Debug, Clone, PartialEq)]
pub struct ParityStageFacts {
    /// The committed `hh-parity-n0/1` record (`withheld` — history).
    pub echo: PacketEcho,
    /// The tranche packet's `schema` member
    /// (`hh-parity-tranche-report/1`).
    pub tranche_version: String,
    /// Its recorded `verdict` (`verified`).
    pub tranche_verdict: String,
    /// `counts.n_pairs` (30 — the frozen N).
    pub n_pairs: u64,
    /// `counts.agree` (30 — directional agreement).
    pub agree: u64,
    /// `counts.discordant` (0 — none dropped).
    pub discordant: u64,
    /// The tranche's `protocol_id` — the frozen manifest's id,
    /// byte-equal to the confirmation cohort's.
    pub protocol_id: String,
    /// `n_declared` — 30 pairs declared + executed (was 0 at the
    /// withheld record).
    pub n_declared: u64,
    /// `s3_unmet` — `false` (the tranche verified parity; S3 met).
    pub s3_unmet: bool,
    /// `confirmation_blocked` — `false` (the dependent stage ran).
    pub confirmation_blocked: bool,
    /// The recorded gate (`GATE-G13` — the N=0 record's gate).
    pub gate: String,
}

/// `evaluate_parity_stage` — the withheld record still refolds
/// `withheld` (committed history); the tranche's members re-check:
/// `verified` only under exact directional agreement (`agree ==
/// n_pairs`, `discordant == 0`, N = the frozen 30), `n_declared`
/// equals the executed N, the protocol id is a real digest, S3
/// discharged and the dependent block lifted — no inflated claim,
/// no dropped discordance.
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
    if f.tranche_version != PARITY_TRANCHE_VERSION {
        return LegVerdict::Refused(format!("tranche_version:{}", f.tranche_version));
    }
    if f.protocol_id.len() != 64 || !f.protocol_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return LegVerdict::Inconclusive("tranche_protocol_id_absent".to_string());
    }
    if f.agree + f.discordant != f.n_pairs {
        return LegVerdict::Refused("pair_count".to_string());
    }
    if f.n_pairs != crate::confirm::FROZEN_TASKS {
        return LegVerdict::Refused(format!("n_pairs:{}", f.n_pairs));
    }
    // `verified` is admitted only under exact directional agreement
    // — never a margin, never a dropped discordance.
    if f.tranche_verdict != "verified" || f.discordant != 0 || f.agree != f.n_pairs {
        return LegVerdict::Refused(format!(
            "tranche_verdict:{}:agree:{}:discordant:{}",
            f.tranche_verdict, f.agree, f.discordant
        ));
    }
    if f.n_declared != f.n_pairs {
        return LegVerdict::Refused(format!("n_declared:{}", f.n_declared));
    }
    if f.s3_unmet || f.confirmation_blocked {
        return LegVerdict::Refused("upstream_block_still_recorded".to_string());
    }
    LegVerdict::Verified
}

/// One named supplement epoch's committed members, verbatim — a
/// second-attempt epoch over exactly the prior residue (the original
/// records stand; denominators re-add, never inflated).
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmEpochFacts {
    /// The epoch packet's `schema` (an `EPOCH_VERSIONS` member).
    pub version: String,
    /// Its recorded `verdict` — `partial` while a residue remains,
    /// `executed` when the epoch's slots all carry receipts.
    pub verdict: String,
    /// `counts.planned` — exactly the prior epoch's inconclusive set.
    pub planned: u64,
    /// `counts.executed` — real receipts.
    pub executed: u64,
    /// `counts.inconclusive` — the residue the next epoch covers.
    pub inconclusive: u64,
    /// `first_subject_denominators.executed`.
    pub first_executed: u64,
    /// `first_subject_denominators.planned` — the first subjects the
    /// epoch's slot set carries.
    pub first_total: u64,
}

/// The confirmation stage — the committed `hh-confirm-withheld/1`
/// withheld record (history — refolds `withheld`, discharged by the
/// executed cohort, never rewritten) plus the frozen-run packet and
/// the named supplement epochs' supplied members.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmStageFacts {
    /// The committed `hh-confirm-withheld/1` record (`withheld` —
    /// history).
    pub echo: PacketEcho,
    /// The frozen-run packet's `schema` (`hh-s4-confirmation/1`).
    pub confirm_version: String,
    /// Its recorded `verdict` (`partial`).
    pub confirm_verdict: String,
    /// `counts.planned` (300 — the frozen slots).
    pub planned: u64,
    /// `counts.executed` (160 — real receipts).
    pub executed: u64,
    /// `counts.inconclusive` (140 — honest bounds, verbatim).
    pub inconclusive: u64,
    /// `counts.failed` (0).
    pub failed: u64,
    /// `first_subject_denominators.executed` (32).
    pub first_executed: u64,
    /// `first_subject_denominators.planned` (60).
    pub first_total: u64,
    /// The named supplement epochs, in commit order (egress →
    /// completion → repairs): each covers exactly the prior residue.
    pub epochs: Vec<ConfirmEpochFacts>,
    /// The parity tranche's `protocol_id` — must byte-equal the
    /// cohort's (the same frozen manifest, never re-picked).
    pub parity_protocol_id: String,
    /// The cohort's `protocol_id` (every epoch shares it).
    pub protocol_id: String,
    /// Combined executed first subjects (32 + 18 + 4 + 6 = 60).
    pub subjects_first: u64,
    /// `tranche_absent` — `false` (the cohort exists).
    pub tranche_absent: bool,
    /// `parity_absent` — `false` (parity verified).
    pub parity_absent: bool,
    /// `confirmation_blocked` — `false` (the cohort ran).
    pub confirmation_blocked: bool,
    /// `s4_unmet` — `true` while any residue remains; `false` at the
    /// completed cohort (every slot carries a real receipt).
    pub s4_unmet: bool,
    /// The recorded gate (`GATE-G14` — the withheld record's gate).
    pub gate: String,
}

/// `evaluate_confirm_stage` — the withheld record still refolds
/// `withheld` (committed history); the cohort's members re-check:
/// the frozen run's dispositions re-add to its planned slots, every
/// named supplement epoch covers exactly the prior residue and
/// re-adds to its own plan, an epoch's verdict is `executed` only
/// when its slots all carry receipts (`partial` otherwise), the
/// combined first-subject count re-adds, the protocol ids byte-equal
/// the parity tranche's (one frozen manifest), and S4 stays unmet
/// while any residue remains — never inflated, never a dropped
/// denominator.
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
    if f.confirm_version != CONFIRM_VERSION {
        return LegVerdict::Refused(format!("confirm_version:{}", f.confirm_version));
    }
    for (member, id) in [
        ("protocol_id", &f.protocol_id),
        ("parity_protocol_id", &f.parity_protocol_id),
    ] {
        if id.len() != 64 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
            return LegVerdict::Inconclusive(format!("{member}_absent"));
        }
    }
    if f.protocol_id != f.parity_protocol_id {
        return LegVerdict::Refused("protocol_id_drift".to_string());
    }
    if f.planned != crate::confirm::FROZEN_SLOTS
        || f.first_total != crate::confirm::FROZEN_FIRST_SLOTS
    {
        return LegVerdict::Refused("manifest_denominators".to_string());
    }
    if f.executed + f.inconclusive + f.failed != f.planned {
        return LegVerdict::Refused("dispositions_ne_planned".to_string());
    }
    // `partial` is the honest verdict for a non-zero/non-full
    // executed cohort — zero executed would be a different record,
    // full executed would be `executed`, never `partial`.
    if f.executed == 0 || f.executed == f.planned || f.confirm_verdict != "partial" {
        return LegVerdict::Refused(format!(
            "confirm_verdict:{}:executed:{}",
            f.confirm_verdict, f.executed
        ));
    }
    if f.first_executed > f.first_total {
        return LegVerdict::Refused("first_denominators".to_string());
    }
    // The epoch chain — each named supplement covers exactly the
    // prior residue, re-adds to its own plan and carries its own
    // first-subject denominators; the residue folds forward.
    let mut residual = f.inconclusive;
    let mut subjects_first = f.first_executed;
    for e in &f.epochs {
        if !EPOCH_VERSIONS.contains(&e.version.as_str()) {
            return LegVerdict::Refused(format!("epoch_version:{}", e.version));
        }
        if e.planned != residual {
            return LegVerdict::Refused(format!("epoch_scope:{}:{}", e.version, e.planned));
        }
        if e.executed + e.inconclusive != e.planned {
            return LegVerdict::Refused(format!("epoch_dispositions:{}", e.version));
        }
        if e.executed == 0 {
            return LegVerdict::Refused(format!("epoch_empty:{}", e.version));
        }
        // `executed` only when every one of the epoch's slots carries
        // a receipt — any residue keeps the honest `partial`.
        if e.inconclusive == 0 {
            if e.verdict != "executed" {
                return LegVerdict::Refused(format!("epoch_verdict:{}", e.verdict));
            }
        } else if e.verdict != "partial" {
            return LegVerdict::Refused(format!("epoch_verdict:{}", e.verdict));
        }
        if e.first_executed > e.first_total || e.first_total > e.planned {
            return LegVerdict::Refused(format!("epoch_first_denominators:{}", e.version));
        }
        subjects_first += e.first_executed;
        residual = e.inconclusive;
    }
    if subjects_first != f.subjects_first {
        return LegVerdict::Refused(format!("subjects_first:{}", f.subjects_first));
    }
    // S4 stays unmet while any residue remains — at the completed
    // cohort (300/300 receipts) `s4_unmet` folds `false` verbatim.
    if f.s4_unmet != (residual > 0) {
        return LegVerdict::Refused(format!("s4_unmet:{}", f.s4_unmet));
    }
    if f.tranche_absent || f.parity_absent || f.confirmation_blocked {
        return LegVerdict::Refused("upstream_block_still_recorded".to_string());
    }
    LegVerdict::Verified
}

// ── leg 9 — efficacy (the registered finite-manifest contrast) ──────

/// One supplied cohort slot's outcome — the committed packet's
/// `task_id`/`arm_id` and the manifest's declared `stratum` member,
/// verbatim, plus `receipt.verdict`.
#[derive(Debug, Clone, PartialEq)]
pub struct EfficacySlot {
    /// The committed slot's `task_id`.
    pub task_id: String,
    /// Its `arm_id` — the declared test/control arms only.
    pub arm_id: String,
    /// The task's declared stratum (the manifest selection member).
    pub stratum: String,
    /// The committed `receipt.verdict` (`resolved`/`unresolved`) —
    /// anything else is a missing receipt, and the complete-cohort
    /// admission fails `incomplete_cohort`, never a dropped member.
    pub receipt_verdict: String,
}

/// One per-task cell the estimator consumed — `{s,n}` per arm.
#[derive(Debug, Clone, PartialEq)]
pub struct EfficacyCell {
    /// The task.
    pub task_id: String,
    /// Test-arm successes (`resolved` receipts).
    pub test_c: u64,
    /// Test-arm trials.
    pub test_n: u64,
    /// Control-arm successes.
    pub control_c: u64,
    /// Control-arm trials.
    pub control_n: u64,
}

/// One declared stratum's supplied result — the estimator's reported
/// members verbatim, or the typed refusal it produced.
#[derive(Debug, Clone, PartialEq)]
pub struct EfficacyStratum {
    /// The stratum.
    pub stratum: String,
    /// `true` — the estimator computed the contrast; `false` — a
    /// typed refusal stands.
    pub computed: bool,
    /// The per-task cells the estimator consumed — re-adds to the
    /// supplied slot fold (the estimator read the committed records,
    /// never a re-pick).
    pub cells: Vec<EfficacyCell>,
    /// `observed_contrast_ppm` — the equal-task observed contrast.
    pub observed_contrast_ppm: i64,
    /// `posterior_mean_ppm` — the analytic posterior mean.
    pub posterior_mean_ppm: i64,
    /// The reported 95% equal-tail credible interval (ppm).
    pub interval_lo_ppm: i64,
    /// The reported interval's upper endpoint.
    pub interval_hi_ppm: i64,
    /// `draws_used` — the full doubling-ladder draw count.
    pub draws_used: u64,
    /// The typed refusal when not computed (empty when computed).
    pub refusal: String,
}

/// The efficacy stage — GATE-G19's complete-cohort admission: the
/// registered finite-manifest contrast (REQ-D-107/109, §4.3) over the
/// completed cohort. Supplied members: the registered declaration
/// verbatim, the 300 slots' receipt fold and the per-stratum results
/// the estimator produced — the evaluator re-adds the cells from the
/// supplied fold and folds the honest status; it never re-estimates.
/// The complete-cohort admission holds only while every slot carries
/// a real receipt — one missing verdict folds
/// `unavailable{incomplete_cohort}` verbatim, never a dropped member.
#[derive(Debug, Clone, PartialEq)]
pub struct EfficacyFacts {
    /// `population` — the named set (`canonical`).
    pub population: String,
    /// `test_arm` — the B arm (`arm:h-podman-oracle`,
    /// clear_tool_results).
    pub test_arm: String,
    /// `control_arm` — the A arm (`arm:o-tb-oracle`, evict_oldest).
    pub control_arm: String,
    /// `prior` — the registered `beta{1,1}`.
    pub prior: String,
    /// `direction` — `test_minus_control`.
    pub direction: String,
    /// `level_ppm` — the frozen 95% (950000).
    pub level_ppm: i64,
    /// `draws_requested` — the frozen 8000.
    pub draws_requested: u64,
    /// The declared seed (0 — the frozen declaration).
    pub seed: u64,
    /// The cohort's `protocol_id` — the frozen manifest's id.
    pub protocol_id: String,
    /// The cohort's receipt fold — every slot, verbatim.
    pub slots: Vec<EfficacySlot>,
    /// Per declared stratum — the estimator's output or refusal.
    pub strata: Vec<EfficacyStratum>,
    /// The honest status spelling.
    pub status: String,
}

/// `evaluate_efficacy` — the declared inputs must equal the registered
/// declaration verbatim; every supplied slot carries a receipt verdict
/// on a declared arm (one missing/foreign member folds
/// `incomplete_cohort`/`refused` — never a dropped member); the
/// supplied strata cover exactly the strata the slots span; each
/// computed stratum's cells re-add to the supplied fold task-for-task
/// (the estimator consumed the committed records) with both arms paired
/// and the R≥5 floor holding; a refused stratum carries its typed
/// refusal verbatim; the status spelling folds the results — all
/// computed → `computed{strata:…}`, all refused → `unavailable{…}`,
/// mixed → `partial{computed:…;refused:…}`.
pub fn evaluate_efficacy(f: &EfficacyFacts) -> LegVerdict {
    // ── the registered declaration, verbatim ──────────────────────
    if f.population != "canonical" {
        return LegVerdict::Refused(format!("population:{}", f.population));
    }
    if f.direction != "test_minus_control" {
        return LegVerdict::Refused(format!("direction:{}", f.direction));
    }
    if f.test_arm != "arm:h-podman-oracle" || f.control_arm != "arm:o-tb-oracle" {
        return LegVerdict::Refused(format!("arms:{}/{}", f.test_arm, f.control_arm));
    }
    if f.prior != "beta{1,1}" {
        return LegVerdict::Refused(format!("prior:{}", f.prior));
    }
    if f.level_ppm != 950_000 {
        return LegVerdict::Refused(format!("level_ppm:{}", f.level_ppm));
    }
    if f.draws_requested != 8_000 {
        return LegVerdict::Refused(format!("draws_requested:{}", f.draws_requested));
    }
    if f.protocol_id.len() != 64 || !f.protocol_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return LegVerdict::Inconclusive("protocol_id_absent".to_string());
    }
    // ── the cohort fold — the complete-cohort admission ───────────
    if f.slots.is_empty() {
        return LegVerdict::Inconclusive("cohort_absent".to_string());
    }
    if f.slots.len() as u64 != crate::confirm::FROZEN_SLOTS {
        return LegVerdict::Refused(format!("cohort_slots:{}", f.slots.len()));
    }
    // Per-stratum per-(task,arm) counts re-added from the fold.
    let mut fold: std::collections::BTreeMap<String, std::collections::BTreeMap<String, [u64; 2]>> =
        std::collections::BTreeMap::new();
    let mut cell_n: std::collections::BTreeMap<(String, String, String), [u64; 2]> =
        std::collections::BTreeMap::new();
    for s in &f.slots {
        if s.receipt_verdict != "resolved" && s.receipt_verdict != "unresolved" {
            return LegVerdict::Inconclusive("incomplete_cohort".to_string());
        }
        if s.task_id.is_empty() || s.stratum.is_empty() {
            return LegVerdict::Refused("slot_member_absent".to_string());
        }
        if s.arm_id != f.test_arm && s.arm_id != f.control_arm {
            return LegVerdict::Refused(format!("slot_arm:{}", s.arm_id));
        }
        let idx = (s.arm_id == f.test_arm) as usize;
        let c = cell_n
            .entry((s.stratum.clone(), s.task_id.clone(), s.arm_id.clone()))
            .or_insert([0; 2]);
        c[1] += 1;
        if s.receipt_verdict == "resolved" {
            c[0] += 1;
        }
        fold.entry(s.stratum.clone())
            .or_default()
            .entry(s.task_id.clone())
            .or_insert([0; 2])[idx] = 1;
    }
    // ── per-stratum: coverage, the re-added cells, the fold's verdict ─
    if f.strata.len() != fold.len() {
        return LegVerdict::Refused(format!("strata:{}", f.strata.len()));
    }
    let mut computed_csv: Vec<String> = Vec::new();
    let mut refused_csv: Vec<String> = Vec::new();
    for st in &f.strata {
        let tasks = match fold.get(&st.stratum) {
            Some(t) => t,
            None => return LegVerdict::Refused(format!("stratum_foreign:{}", st.stratum)),
        };
        if !st.computed {
            if st.refusal.is_empty() {
                return LegVerdict::Refused(format!("stratum_refusal_absent:{}", st.stratum));
            }
            refused_csv.push(format!("{}:{}", st.stratum, st.refusal));
            continue;
        }
        // The cells the estimator consumed must re-add to the fold —
        // task-for-task, both arms paired, the R≥5 first-slot floor.
        if st.cells.len() != tasks.len() || tasks.len() < 2 {
            return LegVerdict::Refused(format!("stratum_cells:{}", st.stratum));
        }
        let mut seen = std::collections::BTreeSet::new();
        for c in &st.cells {
            if !tasks.contains_key(&c.task_id) || !seen.insert(c.task_id.clone()) {
                return LegVerdict::Refused(format!("cell_task:{}", c.task_id));
            }
            for (arm, (sc, sn)) in [
                (&f.test_arm, (c.test_c, c.test_n)),
                (&f.control_arm, (c.control_c, c.control_n)),
            ] {
                let want = cell_n
                    .get(&(st.stratum.clone(), c.task_id.clone(), arm.clone()))
                    .copied()
                    .unwrap_or([0; 2]);
                if (sc, sn) != (want[0], want[1]) {
                    return LegVerdict::Refused(format!("cell_counts:{}", c.task_id));
                }
                if sn < 5 {
                    return LegVerdict::Refused(format!("cell_floor:{}", c.task_id));
                }
            }
        }
        if st.interval_lo_ppm > st.interval_hi_ppm || st.draws_used < f.draws_requested {
            return LegVerdict::Refused(format!("stratum_members:{}", st.stratum));
        }
        computed_csv.push(st.stratum.clone());
    }
    computed_csv.sort();
    refused_csv.sort();
    let expected = if computed_csv.len() == f.strata.len() && !f.strata.is_empty() {
        format!("computed{{strata:{}}}", computed_csv.join(","))
    } else if computed_csv.is_empty() {
        format!("unavailable{{{}}}", refused_csv.join(";"))
    } else {
        format!(
            "partial{{computed:{};refused:{}}}",
            computed_csv.join(","),
            refused_csv.join(";")
        )
    };
    if f.status != expected {
        return LegVerdict::Refused(format!("status:{}!={}", f.status, expected));
    }
    LegVerdict::Verified
}

/// The required honest family statuses (the committed-evidence
/// state): a missing margin, opportunity or admission is never a
/// pass — the unavailable families stay unavailable. The parity
/// family records the verified tranche; the efficacy and sharp-null
/// families are the state-dependent members — `REQUIRED_EFFICACY`
////`REQUIRED_SHARP_NULL` hold the completed-cohort spellings.
pub const REQUIRED_FAMILIES: [(&str, &str); 7] = [
    ("task_mixture", "descriptive{pilot_recomputed}"),
    ("parity", "executed{verified_30_30}"),
    ("mechanism", "executed{activation,pilot}"),
    ("engineering", "executed{ops_live;backend_verified}"),
    ("task_truth", "failed{s1_useful}"),
    ("operations", "executed{ops_live}"),
    ("adoption", "unavailable{not_started}"),
];

/// The sharp-null family's honest spelling at the completed cohort —
/// the registered lane-randomization sign-flip test is not part of
/// GATE-G19's admitted fold: unavailable, never computed here.
pub const REQUIRED_SHARP_NULL: &str = "unavailable{not_computed}";

/// The supplied family statuses — engineered capability, task
/// truth, bounded operation, scientific effect and adoption
/// distinguished (REQ-D-009 / AC-3-2), the unavailable families
/// retained verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyFacts {
    /// Finite-manifest efficacy — must equal `required_efficacy`,
    /// the efficacy leg's computed status spelling.
    pub efficacy: String,
    /// The efficacy leg's status the family must carry verbatim.
    pub required_efficacy: String,
    /// Task-mixture sensitivity.
    pub task_mixture: String,
    /// The sharp-null family — `REQUIRED_SHARP_NULL` (the sign-flip
    /// test is not part of the admitted fold).
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

/// The required family spellings once the executed S1 re-execution is
/// folded (R6.3, P1-D): identical to `REQUIRED_FAMILIES` except
/// `task_truth` — the bounded S1 execution executed and marked, so the
/// family's honest status is `executed{s1_useful}` (the objective stays
/// unmet — `s1_unmet` records it separately).
pub const REQUIRED_FAMILIES_P1D: [(&str, &str); 7] = [
    ("task_mixture", "descriptive{pilot_recomputed}"),
    ("parity", "executed{verified_30_30}"),
    ("mechanism", "executed{activation,pilot}"),
    ("engineering", "executed{ops_live;backend_verified}"),
    ("task_truth", "executed{s1_useful}"),
    ("operations", "executed{ops_live}"),
    ("adoption", "unavailable{not_started}"),
];

/// `evaluate_families` — every family's supplied status equals its
/// required honest spelling; efficacy carries the efficacy leg's
/// computed status verbatim; no pass/met/claim wording may appear in
/// an unavailable family's status; no claim of either sign is
/// present.
pub fn evaluate_families(f: &FamilyFacts) -> LegVerdict {
    evaluate_families_table(f, &REQUIRED_FAMILIES)
}

/// `evaluate_families_table` — the same honest-spelling fold over a
/// caller-supplied required table (the P1-D re-execution fold's
/// `REQUIRED_FAMILIES_P1D`).
pub fn evaluate_families_table(f: &FamilyFacts, table: &[(&str, &str)]) -> LegVerdict {
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
    for (name, have) in supplied {
        if have.is_empty() {
            return LegVerdict::Inconclusive(format!("family_absent:{name}"));
        }
        let required = match name {
            "efficacy" => f.required_efficacy.as_str(),
            "sharp_null" => REQUIRED_SHARP_NULL,
            _ => table
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, s)| *s)
                .unwrap_or(""),
        };
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
    /// Executed first subjects in the admitted packets
    /// (2 activation + 10 pilot = 12).
    pub subjects_first: u64,
    /// Gated-stage first subjects executed: 30 parity pairs + 60
    /// confirmation first subjects (32 frozen run + 18 egress +
    /// 4 completion + 6 repairs) = 90.
    pub gated_subjects: u64,
}

/// `evaluate_costs` — every committed packet's exposure is `0`, the
/// recomputed total is USD $0.00, 12 admitted first subjects, 90
/// gated-stage first subjects (30 parity + 60 confirmation across
/// the four epochs).
pub fn evaluate_costs(f: &CostFacts) -> LegVerdict {
    evaluate_costs_table(f, 14, 12, 90)
}

/// `evaluate_costs_table` — the same exposure fold with the packet,
/// admitted-first-subject and gated-subject counts supplied (the P1-D
/// fold's DF packet joins the rollup: 15 packets, 14 first subjects).
pub fn evaluate_costs_table(
    f: &CostFacts,
    packets: usize,
    subjects_first: u64,
    gated_subjects: u64,
) -> LegVerdict {
    let mut total: i64 = 0;
    for (packet, micros) in &f.per_packet {
        if *micros != 0 {
            return LegVerdict::Refused(format!("exposure:{packet}:{micros}"));
        }
        total += micros;
    }
    if f.per_packet.len() != packets {
        return LegVerdict::Inconclusive(format!("packets:{}", f.per_packet.len()));
    }
    if total != 0 {
        return LegVerdict::Refused(format!("total_micros:{total}"));
    }
    if f.subjects_first != subjects_first {
        return LegVerdict::Refused(format!("subjects_first:{}", f.subjects_first));
    }
    if f.gated_subjects != gated_subjects {
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
    /// S3 met — the parity tranche verified 30/30 (`false`).
    pub s3_unmet: bool,
    /// S4 met — the confirmation cohort executed 300/300 (`false`).
    pub s4_unmet: bool,
    /// The study's honest status — `unmet{s1_failed}`: S1's subjects
    /// failed (task truth unmet); parity, the cohort and the efficacy
    /// contrast landed.
    pub study: String,
    /// The admission shape — `recompute_only`, never an execution.
    pub admission: String,
    /// The structured repair register (REQ-XA-008) the record carries —
    /// empty on reports whose fold names no repairs (emitted only when
    /// non-empty, so prior emission bytes are unchanged).
    pub repairs: Vec<Json>,
}

impl StudyReport {
    /// `to_json` — the canonical record.
    pub fn to_json(&self) -> Json {
        let mut j = self.to_json_body();
        if !self.repairs.is_empty() {
            if let Json::Obj(ref mut m) = j {
                m.insert("repairs".into(), Json::Arr(self.repairs.clone()));
            }
        }
        j
    }

    fn to_json_body(&self) -> Json {
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

/// The folded members the caller binds — the report's honest state
/// is the fold's output, never hard-coded.
#[derive(Debug, Clone, PartialEq)]
pub struct StudyFold {
    /// Executed first subjects across admitted packets (12).
    pub subjects_first: u64,
    /// Total declared exposure in currency micros (0).
    pub exposure_micros: i64,
    /// S1 stays unmet (`true` — the useful stage's record is
    /// `failed`).
    pub s1_unmet: bool,
    /// S3 met — parity verified 30/30 (`false`).
    pub s3_unmet: bool,
    /// The confirmation fold's `s4_unmet`.
    pub s4_unmet: bool,
    /// The study's honest status spelling.
    pub study: String,
}

/// `assemble(legs, fold)` — fold the leg verdicts into the
/// disposition and mint `packet_id` last over the canonical record.
/// Every leg name must be a `LEGS` member, unique and complete — a
/// malformed leg set is a `refused` assembly, never a partial
/// packet.
pub fn assemble(legs: Vec<Leg>, fold: &StudyFold) -> Result<StudyReport, String> {
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
        subjects_first: fold.subjects_first,
        exposure_micros: fold.exposure_micros,
        s1_unmet: fold.s1_unmet,
        s3_unmet: fold.s3_unmet,
        s4_unmet: fold.s4_unmet,
        study: fold.study.clone(),
        disposition,
        admission: "recompute_only".to_string(),
        repairs: Vec::new(),
    };
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("packet_id");
    }
    out.packet_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}

/// `assemble_with_repairs` — `assemble` plus the REQ-XA-008 repair
/// register carried on the record (the P1-D recompute names the
/// repairs its fold covers); `packet_id` minted over the register.
pub fn assemble_with_repairs(
    legs: Vec<Leg>,
    fold: &StudyFold,
    repairs: Vec<Json>,
) -> Result<StudyReport, String> {
    let mut out = assemble(legs, fold)?;
    out.repairs = repairs;
    let mut j = out.to_json();
    if let Json::Obj(ref mut m) = j {
        m.remove("packet_id");
    }
    out.packet_id = idp_digest(DOMAIN, j.to_canonical_string().as_bytes());
    Ok(out)
}
