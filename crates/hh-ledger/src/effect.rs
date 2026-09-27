//! The effect lifecycle machine (§5a.2 R-2.2.2; ADR-0030/0031/0032) — the typed phase
//! transition table, the rebuildable per-effect fold, the write-ahead `committed`
//! barrier (I-1), the `unknown`/`probe` cycle, risk-class monotonicity, the derived
//! idempotency key, and the `CommitToken` the executor seam checks.
//!
//! # Phases
//!
//! `intended → authorized | refused → prepared → deferred? → committed →
//! observed(applied | not_applied | partial) | unknown → probed → …`; terminal set
//! `{refused, observed(applied|not_applied), compensated, reverted, abandoned}`;
//! non-terminal `{intended, authorized, prepared, deferred, committed, unknown,
//! observed(partial), probed(undeterminable)}` (ADR-0030 §1 as amended).
//!
//! `probed{applied|not_applied}` *is* the `observed` transition (a late executor report
//! after `unknown` is `probed`, never a second `observed` — ADR-0030 §3; ADR-0100 I-4);
//! `probed{undeterminable}` returns to `unknown` with `probe_count + 1`.
//!
//! `observed`/`probed` outcomes are **attempt-terminal**; whether they are
//! *effect*-terminal depends on invariant 4 (ADR-0030 §1; the `compensable`
//! delivery row "not_applied ⇒ retry same key" — the key is a function of
//! `effect_id`, so a retry is the same effect at `attempt_no + 1`):
//! `applied` is always terminal; `not_applied` is terminal only for
//! `irreversible` classes and for non-`idempotent` classes without a
//! `not_applied` probe — every other `not_applied` leaves the effect open for
//! `committed{attempt_no + 1}` (ADR-0238 §1).
//!
//! # Scope closing is payload-conditional
//!
//! The effect scope (`scope.effect_id`, opened by `intended`) closes on the first
//! **terminal** marker only: `refused`, `observed{applied|not_applied}`,
//! `probed{applied|not_applied}`, in-scope `compensated`/`reverted`, `abandoned`.
//! `observed{partial}` and `probed{undeterminable}` keep the scope open —
//! [`scope_close_fires`] is the one predicate the validation fold (`check_scopes`),
//! the commit fold and the rebuild fold all share (CC1).
//!
//! `compensated`/`reverted` also have a **marker form**: with `scope.effect_id` absent
//! they name the original in `payload.original_effect_id` — the only way to mark an
//! already-terminal (scope-closed) effect compensated after a saga; the in-scope form
//! is refused `ScopeNotOpen` once the scope has closed. The marker form is refused
//! while the original is still non-terminal (use the in-scope form) and is idempotent
//! on an already-`compensated`/`reverted` original (a saga re-run applies no extra
//! effect — ADR-0032 §3).
//!
//! # Fencing (invariant 6)
//!
//! Every post-`prepared` phase event carries `payload.fencing_token` equal to the
//! writer lease's live generation — the durable half of AC-R-2.2.2-2. The token a
//! helper presents is a [`CommitToken`] minted by [`Store::commit_effect`];
//! [`Store::validate_commit_token`] refuses `NotCommitted`/`Fenced`.

use std::collections::{BTreeMap, BTreeSet};

use hh_identity::idp::idp_id;
use hh_ontology::risk::{RepeatSafety, RiskClass, RiskReversibility};
use hh_wire::json::Json;

use crate::errors::LedgerError;
use crate::event::{Event, EventEnvelope, Scope};
use crate::store::{Lease, Store};

/// `idp/1` domain for the idempotency key (ADR-0031 §5).
pub const IDEMPOTENCY_DOMAIN: &str = "ledger.effect.idempotency";

/// `idempotency_key = H(run_id, effect_id, canonical(args), capability_version)` over
/// the canonical form — stable across attempts and restarts; a function of
/// `effect_id`, never of `attempt_no` (ADR-0031 §5; AC-R-2.2.2-7).
pub fn idempotency_key(
    run_id: &str,
    effect_id: &str,
    args_canonical_hash: &str,
    capability_version: &str,
) -> String {
    let preimage = Json::obj([
        ("run_id", Json::str(run_id)),
        ("effect_id", Json::str(effect_id)),
        ("args_canonical_hash", Json::str(args_canonical_hash)),
        ("capability_version", Json::str(capability_version)),
    ]);
    idp_id(
        IDEMPOTENCY_DOMAIN,
        preimage.to_canonical_string().as_bytes(),
    )
}

/// The observed outcome `∈ {applied, not_applied, partial}` (ADR-0030 §5). `partial`
/// is admitted only for capabilities that can express partial application — the
/// capability-side gate is §05d's; here it is a non-terminal state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedOutcome {
    /// Fully applied.
    Applied,
    /// Provably not applied.
    NotApplied,
    /// Partially applied — non-terminal (compensation/revert still owed).
    Partial,
}

impl ObservedOutcome {
    /// Canonical name.
    pub fn as_str(self) -> &'static str {
        match self {
            ObservedOutcome::Applied => "applied",
            ObservedOutcome::NotApplied => "not_applied",
            ObservedOutcome::Partial => "partial",
        }
    }

    fn parse(s: &str) -> Option<ObservedOutcome> {
        match s {
            "applied" => Some(ObservedOutcome::Applied),
            "not_applied" => Some(ObservedOutcome::NotApplied),
            "partial" => Some(ObservedOutcome::Partial),
            _ => None,
        }
    }
}

/// `ProbeVerdict ∈ {applied, not_applied, undeterminable}` (ADR-0030 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeVerdict {
    /// The world shows the effect applied.
    Applied,
    /// The world shows the effect did not apply.
    NotApplied,
    /// The probe could not tell — back to `unknown`, `probe_count + 1`.
    Undeterminable,
}

impl ProbeVerdict {
    /// Canonical name.
    pub fn as_str(self) -> &'static str {
        match self {
            ProbeVerdict::Applied => "applied",
            ProbeVerdict::NotApplied => "not_applied",
            ProbeVerdict::Undeterminable => "undeterminable",
        }
    }

    fn parse(s: &str) -> Option<ProbeVerdict> {
        match s {
            "applied" => Some(ProbeVerdict::Applied),
            "not_applied" => Some(ProbeVerdict::NotApplied),
            "undeterminable" => Some(ProbeVerdict::Undeterminable),
            _ => None,
        }
    }
}

/// `unknown` causes (ADR-0030 §3; ADR-0136 L3).
pub const UNKNOWN_CAUSES: [&str; 6] = [
    "cancelled",
    "lease_expired",
    "worker_lost",
    "timeout",
    "executor_error",
    "environment_replaced",
];

/// `deferred` reasons (ADR-0134 §2).
pub const DEFER_REASONS: [&str; 2] = ["speculative_branch", "awaiting_promotion"];

/// The lifecycle phase (ADR-0030 §1). `Probed` is not a rest state: a probe event
/// either transitions to `observed` (`applied`/`not_applied`) or returns to `unknown`
/// (`undeterminable`) — the fold represents it as `Unknown` with `probe_count`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EffectPhase {
    /// `action.effect.intended` — scope open, pre-authorization.
    Intended,
    /// `action.effect.authorized` — decision recorded.
    Authorized,
    /// `action.effect.refused` — terminal.
    Refused,
    /// `action.effect.prepared` — idempotency key + class resources bound.
    Prepared,
    /// `action.effect.deferred` — speculative-branch hold; leaves only to
    /// `committed` (after fresh authorization on promotion) or `refused` —
    /// recovery may also mark it `unknown{worker_lost}` (ADR-0130 recovery table:
    /// "`prepared` (or `deferred`), not `committed`").
    Deferred,
    /// `action.effect.committed` — the write-ahead record is durable; dispatch may
    /// proceed under the `CommitToken`.
    Committed,
    /// `action.effect.observed` — `applied`/`not_applied` terminal, `partial`
    /// non-terminal (see `outcome`).
    Observed,
    /// `action.effect.unknown` — outcome unknowable; must be probed, decided by
    /// class policy or `abandoned` — never silently redispatched.
    Unknown,
    /// `action.effect.compensated` — terminal.
    Compensated,
    /// `action.effect.reverted` — terminal.
    Reverted,
    /// `action.effect.abandoned` — terminal, always escalated.
    Abandoned,
}

impl EffectPhase {
    /// Canonical name.
    pub fn as_str(self) -> &'static str {
        match self {
            EffectPhase::Intended => "intended",
            EffectPhase::Authorized => "authorized",
            EffectPhase::Refused => "refused",
            EffectPhase::Prepared => "prepared",
            EffectPhase::Deferred => "deferred",
            EffectPhase::Committed => "committed",
            EffectPhase::Observed => "observed",
            EffectPhase::Unknown => "unknown",
            EffectPhase::Compensated => "compensated",
            EffectPhase::Reverted => "reverted",
            EffectPhase::Abandoned => "abandoned",
        }
    }

    /// The payload-blind terminal check — for callers that only have the phase;
    /// the authoritative test is [`EffectFold::is_terminal`] (a `not_applied`
    /// settlement is terminal only when the class forecloses retry —
    /// ADR-0238 §1).
    pub fn is_terminal(self, outcome: Option<ObservedOutcome>) -> bool {
        match self {
            EffectPhase::Refused
            | EffectPhase::Compensated
            | EffectPhase::Reverted
            | EffectPhase::Abandoned => true,
            EffectPhase::Observed => outcome != Some(ObservedOutcome::Partial),
            _ => false,
        }
    }

    /// Phase is at-or-past the write-ahead barrier (the I-1 check).
    pub fn past_commit(self) -> bool {
        matches!(
            self,
            EffectPhase::Committed
                | EffectPhase::Observed
                | EffectPhase::Unknown
                | EffectPhase::Compensated
                | EffectPhase::Reverted
                | EffectPhase::Abandoned
        )
    }
}

/// The per-effect fold — the rebuildable record `project(effect_ledger)` renders
/// (ADR-0030 §8). Fields absent from a legacy/early event default conservatively
/// (`risk_class = RiskClass::UNKNOWN` — the most dangerous projection).
#[derive(Debug, Clone)]
pub struct EffectFold {
    /// The effect id (the scope id).
    pub effect_id: String,
    /// The lifecycle phase.
    pub phase: EffectPhase,
    /// The `observed` outcome (`partial` keeps the phase non-terminal).
    pub outcome: Option<ObservedOutcome>,
    /// The effective risk class — `max_by_danger` monotone over `authorized` raises.
    pub risk_class: RiskClass,
    /// The declared risk class carried on `intended` (when declared).
    pub declared_risk_class: Option<RiskClass>,
    /// The current attempt counter (`committed`/`observed`/`unknown` payloads agree).
    pub attempt_no: u64,
    /// `probe_count` — `probed{undeterminable}` increments (the ledger is the count).
    pub probe_count: u64,
    /// The last probe verdict, if any.
    pub last_probe_verdict: Option<ProbeVerdict>,
    /// Attempts with an `observed` record — exactly one per `(effect_id, attempt_no)`.
    pub observed_attempts: BTreeSet<u64>,
    /// `attempt_no → (commit event_id, seq)` — every durable write-ahead record.
    pub commits: BTreeMap<u64, (String, u64)>,
    /// The enclosing scopes.
    pub tool_call_id: Option<String>,
    /// The enclosing model call.
    pub model_call_id: Option<String>,
    /// The enclosing turn.
    pub turn_id: Option<String>,
    /// `capability_id` from `intended`.
    pub capability_id: Option<String>,
    /// `capability_version` from `intended` (idempotency-key input).
    pub capability_version: Option<String>,
    /// `args_canonical_hash` from `intended` (idempotency-key input).
    pub args_canonical_hash: Option<String>,
    /// `parent_effect_id` (fan-out edge).
    pub parent_effect_id: Option<String>,
    /// The prepared idempotency key.
    pub idempotency_key: Option<String>,
    /// The deferred branch (payload `branch_id` — branch *scopes* land at Stage 2).
    pub branch_id: Option<String>,
    /// The seq of the terminal marker, when terminal.
    pub terminal_seq: Option<u64>,
}

impl EffectFold {
    fn new(effect_id: &str) -> EffectFold {
        EffectFold {
            effect_id: effect_id.to_string(),
            phase: EffectPhase::Intended,
            outcome: None,
            risk_class: RiskClass::UNKNOWN,
            declared_risk_class: None,
            attempt_no: 0,
            probe_count: 0,
            last_probe_verdict: None,
            observed_attempts: BTreeSet::new(),
            commits: BTreeMap::new(),
            tool_call_id: None,
            model_call_id: None,
            turn_id: None,
            capability_id: None,
            capability_version: None,
            args_canonical_hash: None,
            parent_effect_id: None,
            idempotency_key: None,
            branch_id: None,
            terminal_seq: None,
        }
    }

    /// Retryable after a `not_applied` settlement (invariant 4): any class but
    /// `irreversible`, and either `repeat_safety = idempotent` or a probe
    /// returned `not_applied`.
    pub fn retryable(&self) -> bool {
        self.risk_class.reversibility != RiskReversibility::Irreversible
            && (self.risk_class.repeat_safety == RepeatSafety::Idempotent
                || self.last_probe_verdict == Some(ProbeVerdict::NotApplied))
    }

    /// Is the fold in an *effect*-terminal state? `observed{applied}` always;
    /// `observed{not_applied}` only when the class forecloses retry
    /// ([`EffectFold::retryable`]); `observed{partial}` never (ADR-0238 §1).
    pub fn is_terminal(&self) -> bool {
        match self.phase {
            EffectPhase::Refused
            | EffectPhase::Compensated
            | EffectPhase::Reverted
            | EffectPhase::Abandoned => true,
            EffectPhase::Observed => match self.outcome {
                Some(ObservedOutcome::Applied) => true,
                Some(ObservedOutcome::NotApplied) => !self.retryable(),
                _ => false,
            },
            _ => false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Scope close predicate (shared by check_scopes, commit_envelopes, rebuild)
// ─────────────────────────────────────────────────────────────────────────────

/// Does this event close its `effect` scope? Conditional on the payload *and*
/// the pre-event fold (§5a.2 states; ADR-0238 §1):
///
/// - `observed{partial}` — never closes (settlement pending);
/// - `observed{not_applied}` — closes only when the class forecloses retry
///   (`!fold.retryable()`; a retryable `not_applied` leaves the scope open for
///   `committed{attempt_no + 1}`);
/// - `probed{applied}` — closes (terminal `observed{applied}`);
/// - `probed{not_applied}` — same retryable rule;
/// - `probed{undeterminable}` — never closes (back to `unknown`);
/// - every other `closes_scope = effect` row closes unconditionally.
///
/// `fold` is the effect's fold *before* this event; `None` (a defensive caller
/// or a torn replay) is conservative — `not_applied` then closes.
pub fn scope_close_fires(class: &str, payload: &Json, fold: Option<&EffectFold>) -> bool {
    match class {
        "action.effect.observed" => {
            match payload.get("outcome").and_then(Json::as_str) {
                Some("partial") => false,
                // A direct `not_applied` closes only when the class forecloses
                // retry — `retryable()` reads the fold's *history*.
                Some("not_applied") => match fold {
                    Some(f) => !f.retryable(),
                    None => true,
                },
                _ => true,
            }
        }
        "action.effect.probed" => match payload.get("verdict").and_then(Json::as_str) {
            Some("undeterminable") => false,
            // This probe *is* the `not_applied` proof — invariant 4 makes the
            // effect redispatchable unless `irreversible`.
            Some("not_applied") => match fold {
                Some(f) => f.risk_class.reversibility == RiskReversibility::Irreversible,
                None => true,
            },
            _ => true,
        },
        _ => true,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Fold (lenient — rebuild/commit path; the log is the record)
// ─────────────────────────────────────────────────────────────────────────────

/// Apply one committed envelope to the effect fold — **lenient**: the append path
/// already validated; rebuild trusts the durable record. Legacy events missing the
/// new payload members fold with conservative defaults.
pub fn fold_event(effects: &mut BTreeMap<String, EffectFold>, env: &EventEnvelope) {
    if !env.class.starts_with("action.effect.") {
        return;
    }
    apply_event_fields(
        effects,
        &env.class,
        &env.payload,
        &env.scope,
        &env.event_id,
        env.seq,
    );
}

/// The shared fold step over event-shaped fields (one table — CC1/CC7). Validation
/// calls this for caller `Event`s (seq unknown at that point — the durable fold is
/// re-applied at commit with the real seq).
fn apply_event_fields(
    effects: &mut BTreeMap<String, EffectFold>,
    class: &str,
    payload: &Json,
    scope: &Scope,
    event_id: &str,
    seq: u64,
) {
    // `unattributed` names no subject — non-attribution is the recorded fact;
    // it must never open a phantom fold entry.
    if class == "action.effect.unattributed" {
        return;
    }
    // The marker form names its subject in `payload.original_effect_id`.
    let subject = scope
        .effect_id
        .clone()
        .or_else(|| str_field(payload, "original_effect_id"))
        .or_else(|| str_field(payload, "effect_id"));
    let Some(id) = subject else { return };
    let f = effects
        .entry(id.clone())
        .or_insert_with(|| EffectFold::new(&id));
    match class {
        "action.effect.intended" => {
            if let Some(rc) = payload
                .get("effective_risk_class")
                .and_then(RiskClass::from_json)
            {
                f.risk_class = rc;
            }
            if let Some(rc) = payload
                .get("declared_risk_class")
                .and_then(RiskClass::from_json)
            {
                f.declared_risk_class = Some(rc);
            }
            if scope.tool_call_id.is_some() {
                f.tool_call_id = scope.tool_call_id.clone();
            }
            if scope.model_call_id.is_some() {
                f.model_call_id = scope.model_call_id.clone();
            }
            if scope.turn_id.is_some() {
                f.turn_id = scope.turn_id.clone();
            }
            if let Some(v) = str_field(payload, "capability_id") {
                f.capability_id = Some(v);
            }
            if let Some(v) = str_field(payload, "capability_version") {
                f.capability_version = Some(v);
            }
            if let Some(v) = str_field(payload, "args_canonical_hash") {
                f.args_canonical_hash = Some(v);
            }
            if let Some(v) = str_field(payload, "parent_effect_id") {
                f.parent_effect_id = Some(v);
            }
            f.phase = EffectPhase::Intended;
        }
        "action.effect.authorized" => {
            if let Some(rc) = payload
                .get("effective_risk_class")
                .and_then(RiskClass::from_json)
            {
                f.risk_class = RiskClass::max_by_danger(f.risk_class, rc);
            }
            f.phase = EffectPhase::Authorized;
        }
        "action.effect.refused" => {
            f.phase = EffectPhase::Refused;
            f.terminal_seq = Some(seq);
        }
        "action.effect.prepared" => {
            if let Some(k) = str_field(payload, "idempotency_key") {
                f.idempotency_key = Some(k);
            }
            f.phase = EffectPhase::Prepared;
        }
        "action.effect.deferred" => {
            if let Some(b) = str_field(payload, "branch_id") {
                f.branch_id = Some(b);
            }
            f.phase = EffectPhase::Deferred;
        }
        "action.effect.committed" => {
            if let Some(n) = payload.get("attempt_no").and_then(Json::as_int) {
                f.attempt_no = n as u64;
            }
            f.commits.insert(f.attempt_no, (event_id.to_string(), seq));
            f.phase = EffectPhase::Committed;
        }
        "action.effect.observed" => {
            if let Some(n) = payload.get("attempt_no").and_then(Json::as_int) {
                f.attempt_no = (n as u64).max(f.attempt_no);
                f.observed_attempts.insert(n as u64);
            }
            f.outcome = payload
                .get("outcome")
                .and_then(Json::as_str)
                .and_then(ObservedOutcome::parse);
            f.phase = EffectPhase::Observed;
            if f.is_terminal() {
                f.terminal_seq = Some(seq);
            }
        }
        "action.effect.unknown" => {
            f.phase = EffectPhase::Unknown;
            f.last_probe_verdict = None;
        }
        "action.effect.probed" => {
            let verdict = payload
                .get("verdict")
                .and_then(Json::as_str)
                .and_then(ProbeVerdict::parse);
            match verdict {
                Some(ProbeVerdict::Applied) => {
                    f.phase = EffectPhase::Observed;
                    f.outcome = Some(ObservedOutcome::Applied);
                    f.terminal_seq = Some(seq);
                }
                Some(ProbeVerdict::NotApplied) => {
                    f.phase = EffectPhase::Observed;
                    f.outcome = Some(ObservedOutcome::NotApplied);
                    // The verdict lands first — `is_terminal` reads it (this
                    // probe is the `not_applied` proof of invariant 4).
                    f.last_probe_verdict = verdict;
                    if f.is_terminal() {
                        f.terminal_seq = Some(seq);
                    }
                }
                _ => {
                    f.probe_count += 1;
                    f.phase = EffectPhase::Unknown;
                }
            }
            if !matches!(verdict, Some(ProbeVerdict::NotApplied)) {
                f.last_probe_verdict = verdict;
            }
        }
        "action.effect.compensated" => {
            f.phase = EffectPhase::Compensated;
            f.terminal_seq = Some(seq);
        }
        "action.effect.reverted" => {
            f.phase = EffectPhase::Reverted;
            f.terminal_seq = Some(seq);
        }
        "action.effect.abandoned" => {
            f.phase = EffectPhase::Abandoned;
            f.terminal_seq = Some(seq);
        }
        _ => {}
    }
}

fn str_field(p: &Json, k: &str) -> Option<String> {
    p.get(k).and_then(Json::as_str).map(|s| s.to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Complete mediation — the `security.permission.decided` gate fold (ADR-0052 D6;
// §5g.1 I-H7: "the ledger refuses `action.effect.committed` unless a
// `security.permission.decided{decision = allow}` for that `effect_id` and
// attempt cycle precedes it in `seq`; exactly one `decided` per attempt cycle")
// ─────────────────────────────────────────────────────────────────────────────

/// `(effect_id, attempt_no) → final gate decision` — the `allow`/`deny` verdicts
/// the committed-gate reads. `decided{ask}` and request-closure rows
/// (`timed_out`, `cancelled`, `allow_lease`, …) are non-final and never write
/// here; `decided` rows naming no `effect_id` (e.g. a budget-driven denial that
/// precedes any proposal) are outside the gate entirely.
pub type DecisionFolds = BTreeMap<(String, u64), String>;

/// The decided row's attempt: the payload's `attempt_no` when present, else the
/// effect's pending attempt (last committed + 1; `1` before any commit).
fn decision_attempt(
    effects: &BTreeMap<String, EffectFold>,
    payload: &Json,
    effect_id: &str,
) -> u64 {
    match payload.get("attempt_no").and_then(Json::as_int) {
        Some(n) if n >= 1 => n as u64,
        _ => effects
            .get(effect_id)
            .map(|f| f.attempt_no + 1)
            .unwrap_or(1),
    }
}

/// The decided row's effect: `scope.effect_id`, else `payload.effect_id`.
fn decision_effect(payload: &Json, scope: &Scope) -> Option<String> {
    scope
        .effect_id
        .clone()
        .or_else(|| str_field(payload, "effect_id"))
}

/// Lenient rebuild fold (the durable record was already validated): final gate
/// verdicts land; a second one keeps the first — the log is the record.
pub fn apply_decision(
    decisions: &mut DecisionFolds,
    effects: &BTreeMap<String, EffectFold>,
    env: &EventEnvelope,
) {
    if env.class != "security.permission.decided" {
        return;
    }
    let Some(effect_id) = decision_effect(&env.payload, &env.scope) else {
        return;
    };
    let Some(decision) = str_field(&env.payload, "decision") else {
        return;
    };
    if !matches!(decision.as_str(), "allow" | "deny") {
        return;
    }
    let attempt = decision_attempt(effects, &env.payload, &effect_id);
    decisions.entry((effect_id, attempt)).or_insert(decision);
}

/// Strict append-path fold — runs per event in `Store::append`'s validation loop
/// *after* the effect fold for that event. Enforces the "exactly one final
/// decided per attempt cycle" half of the append rule.
pub fn validate_decision(
    ev: &Event,
    decisions: &mut DecisionFolds,
    effects: &BTreeMap<String, EffectFold>,
) -> Result<(), LedgerError> {
    if ev.class != "security.permission.decided" {
        return Ok(());
    }
    let Some(effect_id) = decision_effect(&ev.payload, &ev.scope) else {
        return Ok(());
    };
    let Some(decision) = str_field(&ev.payload, "decision") else {
        return Ok(());
    };
    if !matches!(decision.as_str(), "allow" | "deny") {
        return Ok(());
    }
    let attempt = decision_attempt(effects, &ev.payload, &effect_id);
    let key = (effect_id.clone(), attempt);
    if decisions.contains_key(&key) {
        return Err(LedgerError::DuplicateDecision {
            effect_id,
            attempt_no: attempt,
        });
    }
    decisions.insert(key, decision);
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Validation (strict — the append path)
// ─────────────────────────────────────────────────────────────────────────────

/// The context append-time validation reads: the run id (identity derivation), the
/// live lease generation (the fencing token) and a class lookup for cross-event
/// references (committed ∪ earlier-in-batch).
pub struct EffectCtx<'a> {
    /// The run being appended to.
    pub run_id: &'a str,
    /// The lease generation this append runs under.
    pub generation: u64,
    /// `event_id → class` over committed events.
    pub committed_class_of: &'a dyn Fn(&str) -> Option<String>,
    /// `event_id → class` over earlier events of this batch.
    pub batch_class_of: &'a dyn Fn(&str) -> Option<String>,
}

impl EffectCtx<'_> {
    fn class_of(&self, event_id: &str) -> Option<String> {
        (self.committed_class_of)(event_id).or_else(|| (self.batch_class_of)(event_id))
    }
}

fn bad(detail: impl Into<String>) -> LedgerError {
    LedgerError::SchemaViolation {
        detail: detail.into(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.8.7-1 — the ordering invariant as a checkable fold (§5g.7 §5: "the
// decision precedes `action.effect.prepared`"; I-H7's committed gate is the
// enforcement half — this is the property the test battery/verifier replays)
// ─────────────────────────────────────────────────────────────────────────────

/// Fold a committed prefix and check the mediation ordering (AC-R-2.8.7-1):
/// every `action.effect.prepared`/`observed` is preceded in `seq` by a
/// `security.permission.decided{decision = allow}` **or a Π `allow`** — the
/// `action.effect.authorized` record — for the same `(effect_id, attempt)`;
/// every `action.effect.committed` requires the `decided{allow}` specifically
/// (I-H7 — the same verdict the strict append gate raises, so the offline
/// check and the append rule report one verdict, CC1). A final `deny` for the
/// attempt vetoes any later effect motion. `LedgerError::Undecided` on
/// violation.
pub fn check_mediation(events: &[EventEnvelope]) -> Result<(), LedgerError> {
    let mut effects: BTreeMap<String, EffectFold> = BTreeMap::new();
    let mut decisions = DecisionFolds::new();
    // `(effect_id, attempt)` the Π `allow` (`action.effect.authorized`) covers —
    // the policy decision record AC-R-2.8.7-1's "or a Π allow" half reads.
    let mut authorized_attempts: std::collections::BTreeSet<(String, u64)> =
        std::collections::BTreeSet::new();
    for env in events {
        match env.class.as_str() {
            "security.permission.decided" => {
                apply_decision(&mut decisions, &effects, env);
            }
            "action.effect.authorized" => {
                if let Some(effect_id) = decision_effect(&env.payload, &env.scope) {
                    let attempt = decision_attempt(&effects, &env.payload, &effect_id);
                    authorized_attempts.insert((effect_id, attempt));
                }
                apply_event_fields(
                    &mut effects,
                    env.class.as_str(),
                    &env.payload,
                    &env.scope,
                    &env.event_id,
                    env.seq,
                );
            }
            "action.effect.prepared" | "action.effect.committed" | "action.effect.observed" => {
                if let Some(effect_id) = decision_effect(&env.payload, &env.scope) {
                    let attempt = decision_attempt(&effects, &env.payload, &effect_id);
                    let mediated = match decisions.get(&(effect_id.clone(), attempt)) {
                        Some(d) => d == "allow",
                        // A Π `allow` mediates preparation/observation; the
                        // write-ahead gate for `committed` is `decided{allow}`
                        // specifically (I-H7).
                        None => {
                            env.class != "action.effect.committed"
                                && authorized_attempts.contains(&(effect_id.clone(), attempt))
                        }
                    };
                    if !mediated {
                        return Err(LedgerError::Undecided {
                            effect_id,
                            attempt_no: attempt,
                        });
                    }
                }
                apply_event_fields(
                    &mut effects,
                    env.class.as_str(),
                    &env.payload,
                    &env.scope,
                    &env.event_id,
                    env.seq,
                );
            }
            _ => {
                apply_event_fields(
                    &mut effects,
                    env.class.as_str(),
                    &env.payload,
                    &env.scope,
                    &env.event_id,
                    env.seq,
                );
            }
        }
    }
    Ok(())
}

fn transition_err(effect_id: &str, from: &str, to: &str) -> LedgerError {
    LedgerError::BadEffectTransition {
        effect_id: effect_id.to_string(),
        from: from.to_string(),
        to: to.to_string(),
    }
}

/// The classes that must carry `payload.fencing_token == lease.generation` — every
/// phase after `prepared` (invariant 6, ADR-0030 §6).
const FENCED_CLASSES: [&str; 7] = [
    "action.effect.committed",
    "action.effect.observed",
    "action.effect.unknown",
    "action.effect.probed",
    "action.effect.compensated",
    "action.effect.reverted",
    "action.effect.abandoned",
];

fn check_fencing(ev: &Event, ctx: &EffectCtx) -> Result<(), LedgerError> {
    if !FENCED_CLASSES.contains(&ev.class.as_str()) {
        return Ok(());
    }
    match ev.payload.get("fencing_token").and_then(Json::as_int) {
        Some(t) if t >= 0 && t as u64 == ctx.generation => Ok(()),
        Some(t) => Err(LedgerError::Fenced {
            lease_generation: t.max(0) as u64,
            current_generation: ctx.generation,
            detail: format!("{} fencing_token is stale", ev.class),
        }),
        None => Err(bad(format!(
            "{} must carry fencing_token (post-prepared invariant 6)",
            ev.class
        ))),
    }
}

/// Validate one `action.effect.*` event against the lifecycle machine and update the
/// batch-local fold on success. Called from `Store::append`'s validation loop —
/// *after* `check_scopes` (scope ids already verified open/opening).
///
/// Returns `Ok(())` for non-effect classes.
pub fn validate_event(
    ev: &Event,
    effects: &mut BTreeMap<String, EffectFold>,
    decisions: &DecisionFolds,
    ctx: &EffectCtx,
) -> Result<(), LedgerError> {
    if !ev.class.starts_with("action.effect.") {
        return Ok(());
    }
    let Some(effect_id) = ev.scope.effect_id.as_deref() else {
        return validate_marker(ev, effects, ctx);
    };
    let class = ev.class.as_str();
    // Every in-scope `action.effect.*` event may carry the id in the payload too;
    // the two must agree (one identity — CC4).
    if let Some(pid) = str_field(&ev.payload, "effect_id") {
        if pid != effect_id {
            return Err(bad(format!(
                "{class} payload.effect_id {pid} ≠ scope.effect_id {effect_id}"
            )));
        }
    }
    check_fencing(ev, ctx)?;
    match class {
        "action.effect.intended" => validate_intended(ev, effects, ctx, effect_id),
        "action.effect.authorized" => {
            let f = get_fold(effects, effect_id)?;
            phase_gate(f, effect_id, class, &[EffectPhase::Intended])?;
            if let Some(j) = ev.payload.get("effective_risk_class") {
                let rc = RiskClass::from_json(j)
                    .ok_or_else(|| bad("authorized.effective_risk_class is not a risk class"))?;
                // Raise-only (AC-R-2.2.2-5): a decision may raise the effective
                // class, never lower it.
                if !f.risk_class.leq_danger(&rc) {
                    return Err(bad(format!(
                        "{class} effective_risk_class {rc} lowers the recorded {prev}",
                        prev = f.risk_class
                    )));
                }
            }
            apply_event_fields(
                effects,
                class,
                &ev.payload,
                &ev.scope,
                &ev.event_id,
                u64::MAX,
            );
            Ok(())
        }
        "action.effect.refused" => {
            let f = get_fold(effects, effect_id)?;
            phase_gate(
                f,
                effect_id,
                class,
                &[
                    EffectPhase::Intended,
                    EffectPhase::Authorized,
                    EffectPhase::Deferred,
                ],
            )?;
            if str_field(&ev.payload, "reason").is_none() {
                return Err(bad("refused carries a reason"));
            }
            apply_event_fields(
                effects,
                class,
                &ev.payload,
                &ev.scope,
                &ev.event_id,
                u64::MAX,
            );
            Ok(())
        }
        "action.effect.prepared" => validate_prepared(ev, effects, ctx, effect_id),
        "action.effect.deferred" => {
            let f = get_fold(effects, effect_id)?;
            phase_gate(f, effect_id, class, &[EffectPhase::Prepared])?;
            match str_field(&ev.payload, "reason").as_deref() {
                Some(r) if DEFER_REASONS.contains(&r) => {}
                other => {
                    return Err(bad(format!(
                        "deferred.reason must be one of {DEFER_REASONS:?}, got {other:?}"
                    )))
                }
            }
            if str_field(&ev.payload, "branch_id").is_none() {
                return Err(bad("deferred carries branch_id"));
            }
            apply_event_fields(
                effects,
                class,
                &ev.payload,
                &ev.scope,
                &ev.event_id,
                u64::MAX,
            );
            Ok(())
        }
        "action.effect.committed" => validate_committed(ev, effects, decisions, effect_id),
        "action.effect.observed" => validate_observed(ev, effects, effect_id),
        "action.effect.unknown" => {
            let f = get_fold(effects, effect_id)?;
            // A non-terminal `observed` (`partial`, or a retryable `not_applied`
            // whose retry was lost) still goes `unknown` — the probe decides.
            let open_observed = f.phase == EffectPhase::Observed && !f.is_terminal();
            if !open_observed {
                phase_gate(
                    f,
                    effect_id,
                    class,
                    &[
                        EffectPhase::Prepared,
                        EffectPhase::Deferred,
                        EffectPhase::Committed,
                    ],
                )?;
            }
            match str_field(&ev.payload, "cause").as_deref() {
                Some(c) if UNKNOWN_CAUSES.contains(&c) => {}
                other => {
                    return Err(bad(format!(
                        "unknown.cause must be one of {UNKNOWN_CAUSES:?}, got {other:?}"
                    )))
                }
            }
            apply_event_fields(
                effects,
                class,
                &ev.payload,
                &ev.scope,
                &ev.event_id,
                u64::MAX,
            );
            Ok(())
        }
        "action.effect.probed" => {
            let f = get_fold(effects, effect_id)?;
            phase_gate(f, effect_id, class, &[EffectPhase::Unknown])?;
            match str_field(&ev.payload, "verdict").as_deref() {
                Some(v) if ProbeVerdict::parse(v).is_some() => {}
                other => {
                    return Err(bad(format!(
                        "probed.verdict must be applied|not_applied|undeterminable, got {other:?}"
                    )))
                }
            }
            apply_event_fields(
                effects,
                class,
                &ev.payload,
                &ev.scope,
                &ev.event_id,
                u64::MAX,
            );
            Ok(())
        }
        "action.effect.compensated" | "action.effect.reverted" => {
            let f = get_fold(effects, effect_id)?;
            phase_gate(
                f,
                effect_id,
                class,
                &[
                    EffectPhase::Committed,
                    EffectPhase::Unknown,
                    // `observed{partial}` / retryable `observed{not_applied}` —
                    // the non-terminal observed states (ADR-0238 §1).
                    EffectPhase::Observed,
                ],
            )?;
            if f.phase == EffectPhase::Observed && f.is_terminal() {
                return Err(transition_err(effect_id, "observed(terminal)", class));
            }
            apply_event_fields(
                effects,
                class,
                &ev.payload,
                &ev.scope,
                &ev.event_id,
                u64::MAX,
            );
            Ok(())
        }
        "action.effect.abandoned" => {
            let f = get_fold(effects, effect_id)?;
            if f.is_terminal() {
                return Err(transition_err(effect_id, f.phase.as_str(), class));
            }
            // `abandoned` is always escalated — the ref must resolve to a
            // `lifecycle.escalation.raised` row (ADR-0032 §4; ADR-0066 Rule O).
            let esc = str_field(&ev.payload, "escalation_ref")
                .ok_or_else(|| bad("abandoned carries escalation_ref (always escalated)"))?;
            match ctx.class_of(&esc).as_deref() {
                Some("lifecycle.escalation.raised") => {}
                Some(other) => {
                    return Err(bad(format!(
                        "abandoned.escalation_ref resolves to {other}, not \
                         lifecycle.escalation.raised"
                    )))
                }
                None => {
                    return Err(LedgerError::UnresolvedEventRef {
                        run_id: ctx.run_id.to_string(),
                        event_id: esc,
                    })
                }
            }
            if str_field(&ev.payload, "reason").is_none() {
                return Err(bad("abandoned carries a reason"));
            }
            apply_event_fields(
                effects,
                class,
                &ev.payload,
                &ev.scope,
                &ev.event_id,
                u64::MAX,
            );
            Ok(())
        }
        _ => Ok(()),
    }
}

/// The marker form — `compensated`/`reverted` without `scope.effect_id`. The original
/// must already be terminal-`observed` or already marked (idempotent re-mark).
fn validate_marker(
    ev: &Event,
    effects: &mut BTreeMap<String, EffectFold>,
    ctx: &EffectCtx,
) -> Result<(), LedgerError> {
    let class = ev.class.as_str();
    // `action.effect.unattributed` (ADR-0101 D4; S1.16) is the scope-free
    // marker for a capture-path signal that resolved to no effect — it mutates
    // no fold and carries `{signal_kind, evidence_ref, detection}` only.
    if class == "action.effect.unattributed" {
        if str_field(&ev.payload, "signal_kind").is_none() {
            return Err(bad("unattributed carries signal_kind"));
        }
        return Ok(());
    }
    if class != "action.effect.compensated" && class != "action.effect.reverted" {
        return Err(bad(format!(
            "{class} requires scope.effect_id (or original_effect_id for the \
             compensated/reverted marker form)"
        )));
    }
    check_fencing(ev, ctx)?;
    let original = str_field(&ev.payload, "original_effect_id")
        .ok_or_else(|| bad(format!("{class} marker form requires original_effect_id")))?;
    let mark_phase = if class == "action.effect.compensated" {
        EffectPhase::Compensated
    } else {
        EffectPhase::Reverted
    };
    let f = get_fold_mut(effects, &original)?;
    let ok = (f.phase == EffectPhase::Observed && f.outcome != Some(ObservedOutcome::Partial))
        || f.phase == mark_phase;
    if !ok {
        return Err(transition_err(
            &original,
            f.phase.as_str(),
            &format!("{class}(marker)"),
        ));
    }
    f.phase = mark_phase;
    Ok(())
}

fn validate_intended(
    ev: &Event,
    effects: &mut BTreeMap<String, EffectFold>,
    ctx: &EffectCtx,
    effect_id: &str,
) -> Result<(), LedgerError> {
    if effects.contains_key(effect_id) {
        return Err(bad(format!("effect {effect_id} already intended")));
    }
    let p = &ev.payload;
    let effective = p
        .get("effective_risk_class")
        .and_then(RiskClass::from_json)
        .ok_or_else(|| bad("intended requires a valid effective_risk_class"))?;
    if let Some(declared) = p.get("declared_risk_class") {
        let declared = RiskClass::from_json(declared)
            .ok_or_else(|| bad("intended.declared_risk_class is not a risk class"))?;
        // Classification monotonicity (ADR-0031 §2): effective ≥ declared.
        if !declared.leq_danger(&effective) {
            return Err(bad(format!(
                "intended effective_risk_class {effective} is below the declared \
                 {declared} — a class never lowers"
            )));
        }
    }
    // The identity tuple: when `ordinal` is carried the id must be the derived
    // `f(run_id, model_call_id, tool_call_id, ordinal)` (ADR-0027 §2) — a re-parsed
    // response never mints a duplicate intent.
    if let Some(ord) = p.get("ordinal").and_then(Json::as_int) {
        if let (Some(mc), Some(tc)) = (&ev.scope.model_call_id, &ev.scope.tool_call_id) {
            let derived = Store::effect_id(ctx.run_id, mc, tc, ord as u64);
            if derived != effect_id {
                return Err(bad(format!(
                    "intended effect_id {effect_id} ≠ derived {derived}"
                )));
            }
        }
    }
    if let Some(parent) = str_field(p, "parent_effect_id") {
        if !effects.contains_key(&parent) {
            return Err(LedgerError::UnknownEffect { effect_id: parent });
        }
    }
    apply_event_fields(
        effects,
        "action.effect.intended",
        &ev.payload,
        &ev.scope,
        &ev.event_id,
        u64::MAX,
    );
    Ok(())
}

fn validate_prepared(
    ev: &Event,
    effects: &mut BTreeMap<String, EffectFold>,
    ctx: &EffectCtx,
    effect_id: &str,
) -> Result<(), LedgerError> {
    let f = get_fold(effects, effect_id)?;
    // Re-prepare is legal only for `read_only` effects (the recovery table's
    // re-prepare path; every other class moves on to committed/deferred).
    let from_ok = f.phase == EffectPhase::Authorized
        || (f.phase == EffectPhase::Prepared && f.risk_class.is_read_only());
    if !from_ok {
        return Err(transition_err(
            effect_id,
            f.phase.as_str(),
            "action.effect.prepared",
        ));
    }
    let p = &ev.payload;
    let key =
        str_field(p, "idempotency_key").ok_or_else(|| bad("prepared requires idempotency_key"))?;
    // The key is derived, never minted — check it when the inputs are known
    // (AC-R-2.2.2-7).
    if let (Some(args), Some(ver)) = (&f.args_canonical_hash, &f.capability_version) {
        let expected = idempotency_key(ctx.run_id, effect_id, args, ver);
        if key != expected {
            return Err(bad(format!(
                "prepared.idempotency_key {key} ≠ derived {expected}"
            )));
        }
    }
    match f.risk_class.reversibility {
        RiskReversibility::Reversible => {
            if str_field(p, "baseline_ref").is_none() {
                return Err(bad(
                    "prepared on a reversible effect requires baseline_ref \
                     (BaselineUnavailable ⇒ downgrade the class first — ADR-0137 S5)",
                ));
            }
        }
        RiskReversibility::Compensable => {
            if str_field(p, "compensation_plan_id").is_none() {
                return Err(bad(
                    "prepared on a compensable effect requires compensation_plan_id \
                     (ADR-0032 §1)",
                ));
            }
        }
        _ => {}
    }
    apply_event_fields(
        effects,
        "action.effect.prepared",
        &ev.payload,
        &ev.scope,
        &ev.event_id,
        u64::MAX,
    );
    Ok(())
}

fn validate_committed(
    ev: &Event,
    effects: &mut BTreeMap<String, EffectFold>,
    decisions: &DecisionFolds,
    effect_id: &str,
) -> Result<(), LedgerError> {
    let f = get_fold(effects, effect_id)?;
    let attempt = ev
        .payload
        .get("attempt_no")
        .and_then(Json::as_int)
        .filter(|n| *n >= 1)
        .map(|n| n as u64)
        .ok_or_else(|| bad("committed requires attempt_no ≥ 1"))?;
    match f.phase {
        EffectPhase::Prepared | EffectPhase::Deferred => {
            if attempt != f.attempt_no + 1 {
                return Err(bad(format!(
                    "committed.attempt_no {attempt} ≠ next attempt {}",
                    f.attempt_no + 1
                )));
            }
        }
        EffectPhase::Unknown | EffectPhase::Observed => {
            // Redispatch — invariant 4: from `unknown` only `idempotent`; from an
            // `observed{not_applied}` settlement only a *retryable* class
            // (`idempotent`, or the probe itself returned `not_applied`;
            // `irreversible` never — ADR-0238 §1).
            let ok = if f.phase == EffectPhase::Unknown {
                f.risk_class.repeat_safety == RepeatSafety::Idempotent
            } else {
                f.outcome == Some(ObservedOutcome::NotApplied) && f.retryable()
            };
            if !ok {
                return Err(transition_err(
                    effect_id,
                    &format!("{}(not redispatchable)", f.phase.as_str()),
                    "action.effect.committed",
                ));
            }
            if attempt != f.attempt_no + 1 {
                return Err(bad(format!(
                    "retry committed.attempt_no {attempt} ≠ {}",
                    f.attempt_no + 1
                )));
            }
        }
        other => {
            return Err(transition_err(
                effect_id,
                other.as_str(),
                "action.effect.committed",
            ))
        }
    }
    // Complete mediation (ADR-0052 D6; §5g.1 I-H7): the ledger refuses a
    // `committed` no `security.permission.decided{decision = allow}` for this
    // `effect_id` and attempt precedes. This is Anderson's "always invoked" as a
    // durable-log invariant — the decision record is the write-ahead gate.
    match decisions.get(&(effect_id.to_string(), attempt)) {
        Some(d) if d == "allow" => {}
        _ => {
            return Err(LedgerError::Undecided {
                effect_id: effect_id.to_string(),
                attempt_no: attempt,
            })
        }
    }
    apply_event_fields(
        effects,
        "action.effect.committed",
        &ev.payload,
        &ev.scope,
        &ev.event_id,
        u64::MAX,
    );
    Ok(())
}

fn validate_observed(
    ev: &Event,
    effects: &mut BTreeMap<String, EffectFold>,
    effect_id: &str,
) -> Result<(), LedgerError> {
    let f = get_fold(effects, effect_id)?;
    let attempt = ev
        .payload
        .get("attempt_no")
        .and_then(Json::as_int)
        .filter(|n| *n >= 1)
        .map(|n| n as u64)
        .ok_or_else(|| bad("observed requires attempt_no ≥ 1"))?;
    if str_field(&ev.payload, "outcome")
        .and_then(|o| ObservedOutcome::parse(&o))
        .is_none()
    {
        return Err(bad("observed.outcome must be applied|not_applied|partial"));
    }
    // Exactly one `observed` per `(effect_id, attempt_no)` — checked before the
    // phase gate so a re-report is `AlreadyObserved`, not a transition error.
    if f.observed_attempts.contains(&attempt) {
        return Err(LedgerError::AlreadyObserved {
            effect_id: effect_id.to_string(),
            attempt_no: attempt,
        });
    }
    match f.phase {
        EffectPhase::Committed => {
            if attempt != f.attempt_no {
                return Err(bad(format!(
                    "observed.attempt_no {attempt} ≠ committed attempt {}",
                    f.attempt_no
                )));
            }
        }
        EffectPhase::Prepared if f.risk_class.is_read_only() => {
            // `read_only` has no write-ahead — `observed` may arrive without a
            // `committed` record; the attempt is the next one.
            if attempt != f.attempt_no + 1 {
                return Err(bad(format!(
                    "read_only observed.attempt_no {attempt} ≠ {}",
                    f.attempt_no + 1
                )));
            }
        }
        EffectPhase::Unknown => {
            // A late executor report after `unknown` is `probed`, never a second
            // `observed` (ADR-0100 I-4).
            return Err(transition_err(
                effect_id,
                "unknown",
                "action.effect.observed",
            ));
        }
        other => {
            return Err(transition_err(
                effect_id,
                other.as_str(),
                "action.effect.observed",
            ))
        }
    }
    apply_event_fields(
        effects,
        "action.effect.observed",
        &ev.payload,
        &ev.scope,
        &ev.event_id,
        u64::MAX,
    );
    Ok(())
}

fn get_fold_mut<'a>(
    effects: &'a mut BTreeMap<String, EffectFold>,
    effect_id: &str,
) -> Result<&'a mut EffectFold, LedgerError> {
    effects
        .get_mut(effect_id)
        .ok_or_else(|| LedgerError::UnknownEffect {
            effect_id: effect_id.to_string(),
        })
}

fn get_fold<'a>(
    effects: &'a BTreeMap<String, EffectFold>,
    effect_id: &str,
) -> Result<&'a EffectFold, LedgerError> {
    effects
        .get(effect_id)
        .ok_or_else(|| LedgerError::UnknownEffect {
            effect_id: effect_id.to_string(),
        })
}

fn phase_gate(
    f: &EffectFold,
    effect_id: &str,
    class: &str,
    allowed: &[EffectPhase],
) -> Result<(), LedgerError> {
    if allowed.contains(&f.phase) {
        Ok(())
    } else {
        Err(transition_err(effect_id, f.phase.as_str(), class))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The write-ahead seam — commit_effect / CommitToken / validate_commit_token
// ─────────────────────────────────────────────────────────────────────────────

/// The `CommitToken` — proof the write-ahead `committed` record is durable. The
/// helper/executor presents it at `exec`; it binds `(effect_id, attempt_no,
/// fencing_token = lease generation, commit seq)` (ADR-0100 I-1; AC-R-2.2.2-10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitToken {
    /// The effect.
    pub effect_id: String,
    /// The attempt this token admits.
    pub attempt_no: u64,
    /// The lease generation under which the commit record landed.
    pub fencing_token: u64,
    /// The `committed` event id.
    pub commit_event_id: String,
    /// Its seq.
    pub commit_seq: u64,
}

/// The `commit` result (ADR-0102 §10 — a repeated commit on an already-applied key
/// returns the stored observation without dispatch).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitOutcome {
    /// A fresh `committed` was appended; the token admits dispatch.
    Committed(CommitToken),
    /// The attempt was already durable — the same token returned (idempotent).
    AlreadyCommitted(CommitToken),
    /// The idempotency key already has `observed(applied)` — the stored observation
    /// event id; no dispatch happens.
    StoredObservation {
        /// The `observed{applied}` event.
        observed_event_id: String,
    },
}

impl Store {
    /// `commit(effect_id)` — the write-ahead helper: appends
    /// `action.effect.committed` (durable before return — `append` syncs before
    /// visibility) and returns the `CommitToken` the executor presents (ADR-0030 §2).
    ///
    /// Idempotent on the durable state: a repeat commit of the live attempt returns
    /// the same token; a commit on an already-`applied` effect returns the stored
    /// observation (ADR-0102 §10). A stale holder is `Fenced` by `append`.
    pub fn commit_effect(
        &mut self,
        run_id: &str,
        lease: &Lease,
        effect_id: &str,
        dispatched_at: Option<&str>,
    ) -> Result<CommitOutcome, LedgerError> {
        let f = self
            .effect_fold(run_id, effect_id)?
            .ok_or_else(|| LedgerError::UnknownEffect {
                effect_id: effect_id.to_string(),
            })?
            .clone();
        if f.phase == EffectPhase::Observed && f.outcome == Some(ObservedOutcome::Applied) {
            let obs = self.observed_event_id(run_id, effect_id, f.attempt_no);
            return Ok(CommitOutcome::StoredObservation {
                observed_event_id: obs.unwrap_or_default(),
            });
        }
        if f.phase == EffectPhase::Committed {
            // Idempotent re-commit of the live attempt — return the stored token.
            if let Some((event_id, seq)) = f.commits.get(&f.attempt_no) {
                return Ok(CommitOutcome::AlreadyCommitted(CommitToken {
                    effect_id: effect_id.to_string(),
                    attempt_no: f.attempt_no,
                    fencing_token: lease.generation,
                    commit_event_id: event_id.clone(),
                    commit_seq: *seq,
                }));
            }
        }
        let attempt = f.attempt_no + 1;
        let event_id = self.alloc_id("evt");
        let mut payload = BTreeMap::from([
            ("effect_id".to_string(), Json::str(effect_id)),
            ("attempt_no".to_string(), Json::Int(attempt as i64)),
            (
                "fencing_token".to_string(),
                Json::Int(lease.generation as i64),
            ),
        ]);
        if let Some(d) = dispatched_at {
            payload.insert("dispatched_at".to_string(), Json::str(d));
        }
        let scope = Scope {
            turn_id: f.turn_id.clone(),
            model_call_id: f.model_call_id.clone(),
            tool_call_id: f.tool_call_id.clone(),
            effect_id: Some(effect_id.to_string()),
            child_run_id: None,
            component_call_id: None,
            branch_id: None,
        };
        let ev = Event {
            event_id: event_id.clone(),
            class: "action.effect.committed".to_string(),
            ts: self.ts_now(),
            hlc: None,
            producer: crate::event::Producer::kernel(crate::store::KERNEL_EFFECT),
            scope,
            parent_event_id: self.head_event_id(run_id)?,
            causes: Vec::new(),
            refs: Vec::new(),
            ir_refs: Vec::new(),
            surface_ids: BTreeMap::new(),
            provenance: Some(hh_provenance::ProvenanceRecord::kernel(
                crate::store::KERNEL_EFFECT,
                self.now_ms(),
            )),
            content_kind: None,
            payload: Json::Obj(payload),
        };
        let range = self.append(run_id, lease, vec![ev])?;
        Ok(CommitOutcome::Committed(CommitToken {
            effect_id: effect_id.to_string(),
            attempt_no: attempt,
            fencing_token: lease.generation,
            commit_event_id: event_id,
            commit_seq: range.first,
        }))
    }

    /// The helper-side check (ADR-0100 I-1): the token must name a durable
    /// `committed` record for `(effect_id, attempt_no)` minted under the *current*
    /// lease generation — a token from before a takeover is `Fenced`; anything else
    /// is `NotCommitted`.
    pub fn validate_commit_token(
        &self,
        run_id: &str,
        token: &CommitToken,
    ) -> Result<(), LedgerError> {
        let f = self.effect_fold(run_id, &token.effect_id)?.ok_or_else(|| {
            LedgerError::UnknownEffect {
                effect_id: token.effect_id.clone(),
            }
        })?;
        let current_gen = self.current_lease_generation(run_id)?;
        if token.fencing_token != current_gen {
            return Err(LedgerError::Fenced {
                lease_generation: token.fencing_token,
                current_generation: current_gen,
                detail: "commit token predates a takeover".into(),
            });
        }
        match f.commits.get(&token.attempt_no) {
            Some((event_id, seq))
                if *event_id == token.commit_event_id && *seq == token.commit_seq =>
            {
                Ok(())
            }
            _ => Err(LedgerError::NotCommitted {
                effect_id: token.effect_id.clone(),
            }),
        }
    }

    /// `effect_state(effect_id) → Effect` — the fold for one effect.
    pub fn effect_fold(
        &self,
        run_id: &str,
        effect_id: &str,
    ) -> Result<Option<EffectFold>, LedgerError> {
        Ok(self.run(run_id)?.effects.get(effect_id).cloned())
    }

    /// `effects_in_state(run_id, phase) → [effect_id]` — the state-indexed query.
    pub fn effects_in_state(
        &self,
        run_id: &str,
        phase: EffectPhase,
    ) -> Result<Vec<String>, LedgerError> {
        Ok(self
            .run(run_id)?
            .effects
            .values()
            .filter(|f| f.phase == phase)
            .map(|f| f.effect_id.clone())
            .collect())
    }

    fn observed_event_id(&self, run_id: &str, effect_id: &str, attempt_no: u64) -> Option<String> {
        let st = self.run(run_id).ok()?;
        st.events
            .iter()
            .find(|e| {
                e.class == "action.effect.observed"
                    && e.scope.effect_id.as_deref() == Some(effect_id)
                    && e.payload.get("attempt_no").and_then(Json::as_int) == Some(attempt_no as i64)
                    && e.payload.get("outcome").and_then(Json::as_str) == Some("applied")
            })
            .map(|e| e.event_id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotency_key_is_stable_and_args_sensitive() {
        let k1 = idempotency_key("run-1", "eff-1", "sha256:aaa", "cap-v1");
        let k2 = idempotency_key("run-1", "eff-1", "sha256:aaa", "cap-v1");
        assert_eq!(k1, k2); // restart-stable — a pure function
        let k3 = idempotency_key("run-1", "eff-1", "sha256:bbb", "cap-v1");
        assert_ne!(k1, k3); // different args ⇒ different key
        let k4 = idempotency_key("run-1", "eff-2", "sha256:aaa", "cap-v1");
        assert_ne!(k1, k4);
        // No attempt input exists — stability across attempts is structural.
    }

    #[test]
    fn terminal_set_matches_the_spec() {
        use EffectPhase::*;
        for (p, o, t) in [
            (Refused, None, true),
            (Observed, Some(ObservedOutcome::Applied), true),
            (Observed, Some(ObservedOutcome::NotApplied), true),
            (Observed, Some(ObservedOutcome::Partial), false),
            (Compensated, None, true),
            (Reverted, None, true),
            (Abandoned, None, true),
            (Intended, None, false),
            (Authorized, None, false),
            (Prepared, None, false),
            (Deferred, None, false),
            (Committed, None, false),
            (Unknown, None, false),
        ] {
            assert_eq!(p.is_terminal(o), t, "{p:?}");
        }
    }

    #[test]
    fn scope_close_fires_on_terminals_only() {
        let partial = Json::obj([("outcome", Json::str("partial"))]);
        let applied = Json::obj([("outcome", Json::str("applied"))]);
        assert!(!scope_close_fires("action.effect.observed", &partial, None));
        assert!(scope_close_fires("action.effect.observed", &applied, None));
        let undet = Json::obj([("verdict", Json::str("undeterminable"))]);
        let det = Json::obj([("verdict", Json::str("applied"))]);
        assert!(!scope_close_fires("action.effect.probed", &undet, None));
        assert!(scope_close_fires("action.effect.probed", &det, None));
        assert!(scope_close_fires(
            "action.effect.refused",
            &Json::Null,
            None
        ));
        assert!(scope_close_fires(
            "action.effect.abandoned",
            &Json::Null,
            None
        ));
        // `not_applied` on a retryable fold keeps the scope open…
        let mut f = EffectFold::new("e1");
        f.phase = EffectPhase::Committed;
        f.risk_class = RiskClass {
            reversibility: RiskReversibility::Compensable,
            repeat_safety: RepeatSafety::NonIdempotent,
            scope: hh_ontology::risk::RiskScope::External,
        };
        f.last_probe_verdict = Some(ProbeVerdict::NotApplied);
        let na = Json::obj([("outcome", Json::str("not_applied"))]);
        assert!(!scope_close_fires("action.effect.observed", &na, Some(&f)));
        // …and closes it once the class forecloses (irreversible).
        f.risk_class.reversibility = RiskReversibility::Irreversible;
        assert!(scope_close_fires("action.effect.observed", &na, Some(&f)));
    }
}
