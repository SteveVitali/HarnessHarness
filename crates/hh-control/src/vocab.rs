//! The closed control vocabularies (§5e.1 data model; ADR-0103 D1–D3, CF-107,
//! CF-218/229/281/395/450/481): `Cue`, `ControlDecision`, the guard-point set,
//! `Decider`, scope kinds, `AttemptDelta`, and the small member sums.
//!
//! Every sum here is closed at the HIR/1 dialect — growth is a dialect bump,
//! never an open string. `DecisionPoint`/`Owner`/`StopReason` are reused from
//! `hh_ontology::control` (CC7); `model_completed.stop_reason` is the
//! *gateway's* closed sum (R-2.3.1, ADR-0119 D1) — no cue carries a third
//! spelling.

use hh_ontology::control::{DecisionPoint, OutcomeClass, Owner, StopReason};
use hh_wire::json::Json;

/// `Cue` — the closed 13-member sum (§5e.1; ADR-0103 Phase 2/4 logs, CF-281,
/// CF-395, CF-450). `woken` carries every wakeup incl. peer messages — there
/// is no separate peer-message cue kind.
#[derive(Debug, Clone, PartialEq)]
pub enum Cue {
    /// `run_opened{goal_ref, inputs}` — the run's first cue.
    RunOpened {
        /// The sealed goal reference.
        goal_ref: String,
        /// The declared inputs (data — never `Text` read by a guard).
        inputs: Json,
    },
    /// `resumed{last_durable, recovery_decision}` — the post-restore cue
    /// (ADR-0130 recovery table).
    Resumed {
        /// The greatest durable seq at crash.
        last_durable: u64,
        /// The restore's typed decision record (`RestoreReport` as JSON).
        recovery_decision: Json,
    },
    /// `model_completed{model_call_id, response_ref, stop_reason}` — the
    /// gateway's closed `StopReason` sum (R-2.3.1; the F1 table is read in
    /// these spellings — CF-481).
    ModelCompleted {
        /// The model call scope.
        model_call_id: String,
        /// The response blob ref.
        response_ref: String,
        /// The gateway stop reason.
        stop_reason: hh_gateway::vocab::StopReason,
    },
    /// `effects_settled{[{effect_id, outcome}], all_terminal, submission_ref?}`
    /// — every open effect reached a terminal/`unknown`/`refused`/`abandoned`.
    /// `submission_ref` carries the driver's `stop_rule = submit` detection
    /// (the marker is detected by the driver, never the environment —
    /// ADR-0104; the F1 table reads `effects_settled{all_terminal,
    /// submission}`).
    EffectsSettled {
        /// The newly settled effects.
        settled: Vec<EffectOutcome>,
        /// Whether every open effect is now settled.
        all_terminal: bool,
        /// The detected submission ref, if any settled effect carried the
        /// marker.
        submission_ref: Option<String>,
    },
    /// `verification_completed` — a verification pass finished.
    VerificationCompleted,
    /// `retrieval_completed` — a `retrieve` decision's fetch finished.
    RetrievalCompleted,
    /// `compaction_completed{view_hash}` — a `compact` decision's rebuild
    /// finished (the new context view hash).
    CompactionCompleted {
        /// The post-compaction `context_view` hash.
        view_hash: String,
    },
    /// `delegation_completed{child_run_id, result_ref, outcome_class}` — a
    /// `delegate` decision's child run finished.
    DelegationCompleted {
        /// The child run.
        child_run_id: String,
        /// The child result artifact ref.
        result_ref: String,
        /// The child's outcome class.
        outcome_class: OutcomeClass,
    },
    /// `human_input{steer | follow_up | approval{effect_id, allow|deny} |
    /// interrupt}` — principal input, delivered ledger-ordered (I7).
    HumanInput(HumanInput),
    /// `envelope_signal{soft_threshold | refused{decision_ref, reason} |
    /// retryable_error{class, attempt} | cancel_requested}` — the F2 seam's
    /// feedback cue.
    EnvelopeSignal(EnvelopeSignal),
    /// `guard_fired{decision_point, guard_id}` — a definition stop-rule or a
    /// refused completion fired (`control.guard.fired` is the ledgered
    /// spelling — CF-229).
    GuardFired {
        /// The decision point the guard covers.
        decision_point: DecisionPoint,
        /// The guard/rule identifier.
        guard_id: String,
    },
    /// `woken{trigger, payload_ref, delivery_mode}` — every wakeup (peer
    /// messages included) arrives through this one member.
    Woken {
        /// The subscription trigger (ADR-0131 §1 closed sum).
        trigger: WokenTrigger,
        /// The occurrence payload ref.
        payload_ref: String,
        /// `steer` | `follow_up`.
        delivery_mode: DeliveryMode,
    },
}

impl Cue {
    /// The member tag (canonical spelling for the `cue_kind` member and the
    /// `wait{until: cue_kind}` target).
    pub fn kind(&self) -> &'static str {
        match self {
            Cue::RunOpened { .. } => "run_opened",
            Cue::Resumed { .. } => "resumed",
            Cue::ModelCompleted { .. } => "model_completed",
            Cue::EffectsSettled { .. } => "effects_settled",
            Cue::VerificationCompleted => "verification_completed",
            Cue::RetrievalCompleted => "retrieval_completed",
            Cue::CompactionCompleted { .. } => "compaction_completed",
            Cue::DelegationCompleted { .. } => "delegation_completed",
            Cue::HumanInput(_) => "human_input",
            Cue::EnvelopeSignal(_) => "envelope_signal",
            Cue::GuardFired { .. } => "guard_fired",
            Cue::Woken { .. } => "woken",
        }
    }
}

/// One settled effect in an `effects_settled` cue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectOutcome {
    /// The effect.
    pub effect_id: String,
    /// The terminal it reached.
    pub outcome: SettledOutcome,
}

/// The terminal an effect settled at (the effect lifecycle's terminal set —
/// `observed` carries its `ObservedOutcome` spelling).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettledOutcome {
    /// `action.effect.observed{outcome ∈ {applied, not_applied, partial}}`.
    Observed {
        /// `applied` | `not_applied` | `partial`.
        outcome: String,
    },
    /// `action.effect.refused`.
    Refused,
    /// `action.effect.unknown{cause}` — terminal-or-visible-unknown (INV-2).
    Unknown {
        /// The unknown cause (`timeout`, `worker_lost`, `drain_timeout`, …).
        cause: String,
    },
    /// `action.effect.abandoned` — terminal; listed in `unresolved_effects[]`.
    Abandoned,
}

impl SettledOutcome {
    /// The terminal-class spelling.
    pub fn as_str(&self) -> &str {
        match self {
            SettledOutcome::Observed { outcome } => outcome.as_str(),
            SettledOutcome::Refused => "refused",
            SettledOutcome::Unknown { .. } => "unknown",
            SettledOutcome::Abandoned => "abandoned",
        }
    }

    /// Whether this terminal blocks a `stop{completed}` admission (G-DECIDE:
    /// non-terminal effects incl. `unknown` block; `abandoned` does not —
    /// OQ-092 resolved).
    pub fn blocks_completion(&self) -> bool {
        matches!(self, SettledOutcome::Unknown { .. })
    }
}

/// `human_input{…}` — the closed principal-input sum.
#[derive(Debug, Clone, PartialEq)]
pub enum HumanInput {
    /// `steer` — a mid-turn course correction (delivered per `steer_mode`).
    Steer {
        /// The steer payload ref (a `Text` leaf or artifact — the strategy
        /// receives the ref, never the bytes, I2).
        payload_ref: String,
    },
    /// `follow_up` — queued input for the next turn.
    FollowUp {
        /// The payload ref.
        payload_ref: String,
    },
    /// `approval{effect_id, allow|deny}` — a permission response.
    Approval {
        /// The effect the approval covers.
        effect_id: String,
        /// `allow` | `deny`.
        allow: bool,
    },
    /// `interrupt` — the principal's cancel.
    Interrupt,
}

/// `envelope_signal{…}` — the F2 seam's feedback sum (a refused `check`
/// returns as `envelope_signal{refused}` — the driver-facing view).
#[derive(Debug, Clone, PartialEq)]
pub enum EnvelopeSignal {
    /// `soft_threshold{dimension}` — a soft (advise) ceiling crossed.
    SoftThreshold {
        /// The dimension spelling.
        dimension: String,
    },
    /// `refused{decision_ref, reason}` — `envelope.check` refused a decision.
    Refused {
        /// The refused `control.decision` event ref.
        decision_ref: String,
        /// The typed refusal reason (`InsufficientBudget`,
        /// `MissingDelegationReason`, …).
        reason: String,
    },
    /// `retryable_error{class, attempt}` — an error the `RetryPolicy` admits.
    RetryableError {
        /// The `ModelErrorClass`/`ErrorClass` spelling (CF-470 sums).
        class: String,
        /// The attempt that failed.
        attempt: u64,
    },
    /// `cancel_requested{by}` — a cancel arrived through a declared channel.
    CancelRequested {
        /// The `CancelledBy` spelling.
        by: String,
    },
}

/// `woken.trigger` — the ADR-0131 §1 `Trigger` closed sum (HIR/1 dialect).
#[derive(Debug, Clone, PartialEq)]
pub enum WokenTrigger {
    /// `timer{at}` — an ISO instant.
    Timer {
        /// The instant.
        at: String,
    },
    /// `schedule{expression, timezone, kind}` — a cron/interval schedule.
    Schedule {
        /// The schedule expression.
        expression: String,
        /// The timezone.
        timezone: String,
        /// `cron` | `interval`.
        kind: String,
    },
    /// `permission_decided{permission_id}`.
    PermissionDecided {
        /// The permission.
        permission_id: String,
    },
    /// `child_terminal{child_run_id}`.
    ChildTerminal {
        /// The child run.
        child_run_id: String,
    },
    /// `effect_terminal{effect_id}`.
    EffectTerminal {
        /// The effect.
        effect_id: String,
    },
    /// `environment_ready{handle}`.
    EnvironmentReady {
        /// The environment handle id.
        handle: String,
    },
    /// `retry_due{scope_id}` — a durable `control.retry.scheduled` came due.
    RetryDue {
        /// The scope the retry re-drives.
        scope_id: String,
    },
    /// `external{source_ref, filter}` — a declared external source.
    External {
        /// The versioned source ref.
        source_ref: String,
        /// The declared filter.
        filter: String,
    },
    /// `manual{principal}` — an operator wakeup.
    Manual {
        /// The principal id.
        principal: String,
    },
    /// `peer_message{from}` — the only path a peer message takes (AC-10).
    PeerMessage {
        /// The sender run.
        from: String,
    },
}

/// `delivery_mode ∈ {steer, follow_up}` (ADR-0131 `WakeupPolicy`; OQ-316 —
/// `follow_up` only at C0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryMode {
    /// `steer` — delivered before the next model call (C1 admission).
    Steer,
    /// `follow_up` — queued for the next turn (the C0 mode).
    FollowUp,
}

impl DeliveryMode {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DeliveryMode::Steer => "steer",
            DeliveryMode::FollowUp => "follow_up",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<DeliveryMode> {
        Some(match s {
            "steer" => DeliveryMode::Steer,
            "follow_up" => DeliveryMode::FollowUp,
            _ => return None,
        })
    }
}

/// `ControlDecision` — the closed 10-member sum (§5e.1; `continue ≡ propose`,
/// `ask ≡ escalate` — CF-218). Every decision is stamped
/// `{decision_point, owner, rationale_ref?}` (I1/I8 — the stamp is what makes
/// `boundary_observed` computable).
#[derive(Debug, Clone, PartialEq)]
pub struct ControlDecision {
    /// The stamp — which decision point decided and who owned it.
    pub stamp: DecisionStamp,
    /// The decision itself.
    pub kind: DecisionKind,
}

/// The per-decision stamp `{decision_point, owner, rationale_ref?}`.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionStamp {
    /// The decision point.
    pub decision_point: DecisionPoint,
    /// The owner β assigns the point (`code` for every react/minimal
    /// envelope-reserved point; `model` for plan/act).
    pub owner: Owner,
    /// An optional rationale artifact ref.
    pub rationale_ref: Option<String>,
}

/// The decision payload sum.
#[derive(Debug, Clone, PartialEq)]
pub enum DecisionKind {
    /// `propose{decision_point ∈ {plan, act}, context_request,
    /// expected_output}` — ask the model for the next step.
    Propose {
        /// `plan` | `act` — the step the proposal is for.
        decision_point: DecisionPoint,
        /// The context assembly request (the `propose.context_request`
        /// member — data the assembler interprets).
        context_request: Json,
        /// `free` | `schema(Ref<Validator>)`.
        expected_output: ExpectedOutput,
    },
    /// `act{intents[], mode, on_partial}` — dispatch the proposed effect
    /// intents (the driver maps each intent to the
    /// `action.effect.intended → …` lifecycle).
    Act {
        /// The effect intents (`action.effect.intended` payloads as data —
        /// the driver/gate owns the lifecycle).
        intents: Vec<Json>,
        /// `sequential` | `parallel`.
        mode: ActMode,
        /// Partial-failure policy (the `on_partial` attribute — MUST-data).
        on_partial: OnPartial,
    },
    /// `retrieve{query}` — a memory/retrieval request.
    Retrieve {
        /// The retrieval query (data — the retrieve port interprets it).
        query: Json,
    },
    /// `compact{reason}` — a compaction request.
    Compact {
        /// Why (a `CompactionRequired`-class reason spelling).
        reason: String,
    },
    /// `verify{validator_refs, subject}` — run validators over a subject.
    Verify {
        /// The validators to run.
        validator_refs: Vec<String>,
        /// The subject (data).
        subject: Json,
    },
    /// `delegate{spec, budget_slice, permissions, delegation_reason}` — a
    /// `model`-owned delegate without `delegation_reason` is refused
    /// `MissingDelegationReason` (ADR-0186 D4; AC-10).
    Delegate {
        /// The `SubagentSpec` (data — R-2.6.3's record).
        spec: Json,
        /// The budget slice for the child.
        budget_slice: Json,
        /// The delegated permissions.
        permissions: Json,
        /// The `model_claim` delegation reason (mandatory for
        /// `owner = model`; `None` ⇒ `MissingDelegationReason`).
        delegation_reason: Option<Json>,
    },
    /// `retry{target, attempt, not_before?}` — re-drive a failed scope
    /// (shared attempt identity — INV-5; `pause_turn` resubmits as
    /// `retry{target: model_call}`, never a gateway retry — ADR-0119 D1).
    Retry {
        /// What to re-drive.
        target: RetryTarget,
        /// The new attempt number (must strictly increment — INV-5).
        attempt: u64,
        /// Earliest fire time (retry-after honoured).
        not_before: Option<u64>,
    },
    /// `escalate{ask}` — a question/approval/handoff to the principal.
    Escalate {
        /// The ask shape.
        ask: EscalateAsk,
    },
    /// `wait{until}` — park until a cue kind or a deadline.
    Wait {
        /// The wait condition.
        until: WaitUntil,
    },
    /// `stop{proposed_reason, submission_ref?}` — propose run end; the
    /// envelope admits or converts it (I6).
    Stop {
        /// The proposed `StopReason` (the envelope's verdict may differ —
        /// the recorded `control.decision{stop}.reason` is the admitted one).
        proposed_reason: StopReason,
        /// The submission artifact ref (`stop_rule = submit`).
        submission_ref: Option<String>,
    },
}

impl DecisionKind {
    /// The member tag (the `kind` member of `control.decision`).
    pub fn as_str(&self) -> &'static str {
        match self {
            DecisionKind::Propose { .. } => "propose",
            DecisionKind::Act { .. } => "act",
            DecisionKind::Retrieve { .. } => "retrieve",
            DecisionKind::Compact { .. } => "compact",
            DecisionKind::Verify { .. } => "verify",
            DecisionKind::Delegate { .. } => "delegate",
            DecisionKind::Retry { .. } => "retry",
            DecisionKind::Escalate { .. } => "escalate",
            DecisionKind::Wait { .. } => "wait",
            DecisionKind::Stop { .. } => "stop",
        }
    }
}

/// `expected_output ∈ {free, schema(Ref<Validator>)}`.
#[derive(Debug, Clone, PartialEq)]
pub enum ExpectedOutput {
    /// Free-form output.
    Free,
    /// Output validated by a `Validator{kind: schema}`.
    Schema {
        /// The validator ref.
        validator_ref: String,
    },
}

/// `mode ∈ {sequential, parallel}` for `act`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActMode {
    /// Intents run one at a time (the `react/minimal` mode).
    Sequential,
    /// Intents may run concurrently (`parallel_effects` capability required).
    Parallel,
}

impl ActMode {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ActMode::Sequential => "sequential",
            ActMode::Parallel => "parallel",
        }
    }
}

/// `on_partial` — the act batch's partial-failure policy (MUST-data; the
/// RuntimePlan `step.on_partial` attribute).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnPartial {
    /// A settled failure abandons the rest of the batch.
    FailBatch,
    /// The batch continues; partial failures are reported.
    ContinueBatch,
}

impl OnPartial {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            OnPartial::FailBatch => "fail_batch",
            OnPartial::ContinueBatch => "continue_batch",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<OnPartial> {
        Some(match s {
            "fail_batch" => OnPartial::FailBatch,
            "continue_batch" => OnPartial::ContinueBatch,
            _ => return None,
        })
    }
}

/// `retry.target ∈ {model_call | effect_id | plan_node | delegation}`.
#[derive(Debug, Clone, PartialEq)]
pub enum RetryTarget {
    /// Re-issue the model call (same `model_call_id`, attempt + 1 — INV-5;
    /// `pause_turn` resubmits here).
    ModelCall {
        /// The model call scope.
        model_call_id: String,
    },
    /// Re-dispatch an effect (per ADR-0031 class rules — INV-8).
    Effect {
        /// The effect id.
        effect_id: String,
    },
    /// Re-run a plan node (plan_execute).
    PlanNode {
        /// The plan node.
        node_id: String,
    },
    /// Re-drive a delegation.
    Delegation {
        /// The child run / delegation id.
        delegation_id: String,
    },
}

/// `escalate.ask ∈ {approval{effect_id}, question{Text}, handoff}`.
#[derive(Debug, Clone, PartialEq)]
pub enum EscalateAsk {
    /// `approval{effect_id}` — ask permission for an effect.
    Approval {
        /// The effect awaiting approval.
        effect_id: String,
    },
    /// `question{payload_ref}` — a question for the principal (the `Text`
    /// leaf is a ref — the strategy never holds the bytes, I2).
    Question {
        /// The question payload ref.
        payload_ref: String,
    },
    /// `handoff` — hand the run to the principal.
    Handoff,
}

/// `wait.until ∈ {cue_kind, deadline}`.
#[derive(Debug, Clone, PartialEq)]
pub enum WaitUntil {
    /// Wait for a cue kind (`model_completed` for a `deferred` call — the
    /// ADR-0119 D1 rule).
    CueKind {
        /// The cue kind spelling (`Cue::kind`).
        cue_kind: String,
    },
    /// Wait until a deadline (wall-ms).
    Deadline {
        /// The deadline.
        at_ms: u64,
    },
}

/// The six enforcement points (§5e.2; ADR-0106 D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GuardPoint {
    /// `pre_call` — before every model call (budget check, deadlines, gauge
    /// caps, reservation, ladder state).
    PreCall,
    /// `interpret` — after the model response, before `action.tool.proposed`
    /// (output validation, loop detectors, empty-response ladder).
    Interpret,
    /// `pre_dispatch` — after `authorize = allow`, before `commit` (retry
    /// eligibility, deadline assignment, reservation, INV-3).
    PreDispatch,
    /// `post_effect` — after every effect/model-call terminal (ceiling
    /// exhaustion, INV-1/2/4/8/9, no-progress evidence, timeout bookkeeping).
    PostEffect,
    /// `decide` — before β's `decide` and on any `stop` proposal (kernel stop
    /// rules by priority; completion admissibility).
    Decide,
    /// `resume` — at `lifecycle.run.resumed` (re-arm; recompute counters;
    /// INV-1 on past-deadline scopes; resume a pre-crash drain).
    Resume,
}

impl GuardPoint {
    /// The full closed set (six points).
    pub const ALL: [GuardPoint; 6] = [
        GuardPoint::PreCall,
        GuardPoint::Interpret,
        GuardPoint::PreDispatch,
        GuardPoint::PostEffect,
        GuardPoint::Decide,
        GuardPoint::Resume,
    ];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            GuardPoint::PreCall => "pre_call",
            GuardPoint::Interpret => "interpret",
            GuardPoint::PreDispatch => "pre_dispatch",
            GuardPoint::PostEffect => "post_effect",
            GuardPoint::Decide => "decide",
            GuardPoint::Resume => "resume",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<GuardPoint> {
        Some(match s {
            "pre_call" => GuardPoint::PreCall,
            "interpret" => GuardPoint::Interpret,
            "pre_dispatch" => GuardPoint::PreDispatch,
            "post_effect" => GuardPoint::PostEffect,
            "decide" => GuardPoint::Decide,
            "resume" => GuardPoint::Resume,
            _ => return None,
        })
    }
}

/// `control.decision.decider ∈ {strategy, envelope, principal, parent,
/// hosting}` (§5e.1 ledger row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Decider {
    /// The control strategy.
    Strategy,
    /// The envelope (kernel stop rules, guards).
    Envelope,
    /// The principal (human input).
    Principal,
    /// The parent run (subagent drain).
    Parent,
    /// The hosting boundary.
    Hosting,
}

impl Decider {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Decider::Strategy => "strategy",
            Decider::Envelope => "envelope",
            Decider::Principal => "principal",
            Decider::Parent => "parent",
            Decider::Hosting => "hosting",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<Decider> {
        Some(match s {
            "strategy" => Decider::Strategy,
            "envelope" => Decider::Envelope,
            "principal" => Decider::Principal,
            "parent" => Decider::Parent,
            "hosting" => Decider::Hosting,
            _ => return None,
        })
    }
}

/// The retry/timeout scope kinds (`RetryPolicy`/`TimeoutPolicy` map keys —
/// §5e.2; `model_call` keys on `ModelErrorClass`, every other kind on
/// `ErrorClass` — CF-314).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScopeKind {
    /// A model call.
    ModelCall,
    /// A tool attempt / effect dispatch.
    ToolAttempt,
    /// A compaction run.
    Compaction,
    /// A validator invocation.
    Validator,
    /// A subagent (child run).
    Subagent,
    /// A permission wait.
    Permission,
    /// A resource lock (R-2.6.5).
    Resource,
}

impl ScopeKind {
    /// The full closed set (seven kinds).
    pub const ALL: [ScopeKind; 7] = [
        ScopeKind::ModelCall,
        ScopeKind::ToolAttempt,
        ScopeKind::Compaction,
        ScopeKind::Validator,
        ScopeKind::Subagent,
        ScopeKind::Permission,
        ScopeKind::Resource,
    ];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ScopeKind::ModelCall => "model_call",
            ScopeKind::ToolAttempt => "tool_attempt",
            ScopeKind::Compaction => "compaction",
            ScopeKind::Validator => "validator",
            ScopeKind::Subagent => "subagent",
            ScopeKind::Permission => "permission",
            ScopeKind::Resource => "resource",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<ScopeKind> {
        Some(match s {
            "model_call" => ScopeKind::ModelCall,
            "tool_attempt" => ScopeKind::ToolAttempt,
            "compaction" => ScopeKind::Compaction,
            "validator" => ScopeKind::Validator,
            "subagent" => ScopeKind::Subagent,
            "permission" => ScopeKind::Permission,
            "resource" => ScopeKind::Resource,
            _ => return None,
        })
    }

    /// Whether this kind keys its `RetryPolicy` on `ModelErrorClass`
    /// (`model_call`) or `ErrorClass` (every other kind — CF-314).
    pub fn keys_model_error(self) -> bool {
        matches!(self, ScopeKind::ModelCall)
    }
}

/// `attempt_delta ∈ {transport, sampling_temperature,
/// model_fallback(profile_ref)}` (§5e.2 kernel constraints — a retry that
/// changes the request is recorded, never silent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaKind {
    /// A transport-level change.
    Transport,
    /// A sampling-temperature change.
    SamplingTemperature,
    /// A model fallback — the profile ref is recorded.
    ModelFallback {
        /// The fallback profile.
        profile_ref: String,
    },
}

impl DeltaKind {
    /// The canonical spelling.
    pub fn as_str(&self) -> String {
        match self {
            DeltaKind::Transport => "transport".into(),
            DeltaKind::SamplingTemperature => "sampling_temperature".into(),
            DeltaKind::ModelFallback { profile_ref } => {
                format!("model_fallback{{{profile_ref}}}")
            }
        }
    }

    /// The tag spelling (without the parameter).
    pub fn tag(&self) -> &'static str {
        match self {
            DeltaKind::Transport => "transport",
            DeltaKind::SamplingTemperature => "sampling_temperature",
            DeltaKind::ModelFallback { .. } => "model_fallback",
        }
    }
}

// Re-exports for ergonomic call sites — the canonical sums stay owned by
// `hh_ontology::control` (CC7); this module only re-exports.
pub use hh_ontology::control::{
    CancelledBy, InfraError, InfraErrorFamily, InvariantId, KernelInfraCause, LoopPattern,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_has_thirteen_members_with_stable_spellings() {
        let kinds = [
            Cue::RunOpened {
                goal_ref: "g".into(),
                inputs: Json::Null,
            }
            .kind(),
            Cue::Resumed {
                last_durable: 0,
                recovery_decision: Json::Null,
            }
            .kind(),
            Cue::ModelCompleted {
                model_call_id: "m".into(),
                response_ref: "r".into(),
                stop_reason: hh_gateway::vocab::StopReason::EndTurn,
            }
            .kind(),
            Cue::EffectsSettled {
                settled: vec![],
                all_terminal: true,
                submission_ref: None,
            }
            .kind(),
            Cue::VerificationCompleted.kind(),
            Cue::RetrievalCompleted.kind(),
            Cue::CompactionCompleted {
                view_hash: "h".into(),
            }
            .kind(),
            Cue::DelegationCompleted {
                child_run_id: "c".into(),
                result_ref: "r".into(),
                outcome_class: OutcomeClass::Scored,
            }
            .kind(),
            Cue::HumanInput(HumanInput::Interrupt).kind(),
            Cue::EnvelopeSignal(EnvelopeSignal::CancelRequested {
                by: "principal".into(),
            })
            .kind(),
            Cue::GuardFired {
                decision_point: DecisionPoint::Stop,
                guard_id: "g".into(),
            }
            .kind(),
            Cue::Woken {
                trigger: WokenTrigger::PeerMessage { from: "r".into() },
                payload_ref: "p".into(),
                delivery_mode: DeliveryMode::FollowUp,
            }
            .kind(),
        ];
        assert_eq!(kinds.len(), 12);
        assert_eq!(kinds[0], "run_opened");
        assert_eq!(kinds[11], "woken");
    }

    #[test]
    fn decision_kind_is_the_ten_member_sum() {
        let kinds = [
            DecisionKind::Propose {
                decision_point: DecisionPoint::Act,
                context_request: Json::Null,
                expected_output: ExpectedOutput::Free,
            },
            DecisionKind::Act {
                intents: vec![],
                mode: ActMode::Sequential,
                on_partial: OnPartial::FailBatch,
            },
            DecisionKind::Retrieve { query: Json::Null },
            DecisionKind::Compact { reason: "r".into() },
            DecisionKind::Verify {
                validator_refs: vec![],
                subject: Json::Null,
            },
            DecisionKind::Delegate {
                spec: Json::Null,
                budget_slice: Json::Null,
                permissions: Json::Null,
                delegation_reason: None,
            },
            DecisionKind::Retry {
                target: RetryTarget::ModelCall {
                    model_call_id: "m".into(),
                },
                attempt: 2,
                not_before: None,
            },
            DecisionKind::Escalate {
                ask: EscalateAsk::Handoff,
            },
            DecisionKind::Wait {
                until: WaitUntil::CueKind {
                    cue_kind: "model_completed".into(),
                },
            },
            DecisionKind::Stop {
                proposed_reason: StopReason::Completed,
                submission_ref: None,
            },
        ];
        let spellings: Vec<&str> = kinds.iter().map(DecisionKind::as_str).collect();
        assert_eq!(
            spellings,
            [
                "propose", "act", "retrieve", "compact", "verify", "delegate", "retry", "escalate",
                "wait", "stop"
            ]
        );
    }

    #[test]
    fn guard_point_is_the_six_member_set() {
        assert_eq!(GuardPoint::ALL.len(), 6);
        for p in GuardPoint::ALL {
            assert_eq!(GuardPoint::parse(p.as_str()), Some(p));
        }
    }

    #[test]
    fn only_unknown_settlements_block_completion() {
        // G-DECIDE completion admissibility (OQ-092 resolved): `unknown` and
        // `probed(undeterminable)` block; `abandoned` is terminal and does not.
        assert!(SettledOutcome::Unknown {
            cause: "timeout".into()
        }
        .blocks_completion());
        assert!(!SettledOutcome::Abandoned.blocks_completion());
        assert!(!SettledOutcome::Observed {
            outcome: "applied".into()
        }
        .blocks_completion());
        assert!(!SettledOutcome::Refused.blocks_completion());
    }

    #[test]
    fn model_call_keys_model_error_every_other_kind_keys_error_class() {
        for k in ScopeKind::ALL {
            assert_eq!(k.keys_model_error(), k == ScopeKind::ModelCall);
        }
    }
}
