//! The control driver (§5e.1 driver obligations — Core MUST-code): the cue
//! inbox, the `next_cue → observe → decide → bind → check →
//! control.decision → execute` loop, `checkpoint_ref` on every
//! `control.decision`, the F2 seam (`envelope.check → admitted |
//! refused{reason}` returned to β as `envelope_signal{refused}`), the
//! run/turn lifecycle rows, and **resume-by-leaf** (`restore` the leaf
//! checkpoint + `observe` the durable tail — never a re-run).
//!
//! The driver owns *orchestration*, never I/O: the model boundary is a
//! [`ModelPort`], effect dispatch an [`EffectGate`], context assembly an
//! [`AssemblerPort`], durability a [`LedgerSink`]. Every port is an
//! injected seam — the Stage-1 tests drive the whole loop offline with
//! scripted ports (determinism: the logical clock and id allocator are
//! `u64` counters the driver owns, never `Instant`/`random`).

use hh_ledger::effect::{fold_event, EffectFold, EffectPhase};
use hh_ledger::event::{Event, EventEnvelope, Scope};
use hh_ledger::manifest::EventRef;
use hh_ontology::control::{CancelledBy, DecisionPoint, Owner, StopReason};
use hh_provenance::authority::PersistenceScope;
use hh_provenance::origin::{HumanRole, Origin};
use hh_provenance::record::ProvenanceRecord;
use hh_wire::json::Json;

use crate::envelope::{CheckVerdict, Envelope, EnvelopeState};
use crate::guards::{GuardContext, GuardInput, GuardVerdict};
use crate::output::{ParsedCall, SurfaceSpec};
use crate::policy::EnvelopePolicy;
use crate::react::ReactMinimal;
use crate::state::ControlState;
use crate::stop::DrainReport;
use crate::strategy::{ControlContext, ControlError, ControlStrategy, FinalReport};
use crate::vocab::{
    Cue, Decider, DecisionKind, EffectOutcome, EnvelopeSignal, GuardPoint, HumanInput, ScopeKind,
    SettledOutcome,
};

// ─────────────────────────────────────────────────────────────────────────────
// Ports — the injected seams (the driver never does I/O).
// ─────────────────────────────────────────────────────────────────────────────

/// The model boundary — `call` issues one model call and returns its parsed
/// outcome (the gateway's record: the closed gateway `StopReason`, the
/// response ref, the parsed calls). At Stage 1 the port is scripted in
/// tests; in production it is the `hh-gateway` boundary.
pub trait ModelPort {
    /// Issue the call (`model_call_id` is the scope the driver allocated).
    fn call(&mut self, model_call_id: &str, request: &Json) -> ModelOutcome;
}

/// A parsed model outcome — the members G-INTERPRET and the cue read.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelOutcome {
    /// The gateway stop reason (R-2.3.1's sum — never the control-plane's).
    pub stop_reason: hh_gateway::vocab::StopReason,
    /// The response blob ref (`model_completed.response_ref`).
    pub response_ref: String,
    /// Whether the response carried no text.
    pub text_empty: bool,
    /// The parsed tool calls (empty on text-only responses).
    pub calls: Vec<ParsedCall>,
    /// The `ModelErrorClass` spelling for `error`/`unknown` terminals.
    pub error_class: Option<String>,
    /// A provider `retry-after` hint (ms) — honoured per the `RetrySpec`.
    pub retry_after_ms: Option<u64>,
}

/// The effect boundary — `dispatch` runs one intent through the
/// `intended → prepared → committed → observed` lifecycle (the gate
/// appends its own rows through the sink the driver hands it — the
/// `authorize`/`commit`/`observe` internals are the effect subsystem's,
/// R-2.2.2). `read_only` and the effect class ride the intent record.
pub trait EffectGate {
    /// Dispatch one intent; return the settled outcome (+ the driver's
    /// `stop_rule = submit` detection — the marker is detected here,
    /// never by the environment, ADR-0104).
    fn dispatch(&mut self, effect_id: &str, attempt_no: u64, intent: &Json) -> GateOutcome;
    /// The structured `finish` record the model submitted through the
    /// completion capability (the claim surface — ADR-0112 D2:
    /// `finish{completion?, criteria_status[], artifacts_claimed[],
    /// open_items[]}`). Defaulted `None` — additive for Stage-1 gates that
    /// predate the claim surface; the driver still emits a completion claim
    /// for every `stop{completed}` (AC-R-2.7.2a-1).
    fn finish_record(&self) -> Option<Json> {
        None
    }

    /// The component id the `emitted` rows carry as
    /// `producer.component_variant_ref` (INV-7: `security.permission.
    /// decided{allow}` minted under `hh-control` is an envelope grant —
    /// refused as a violation; the gate is the effect boundary, its
    /// records carry its own identity). Defaulted for Stage-1 scripted
    /// gates; real boundaries override with their own ref.
    fn producer_component(&self) -> &'static str {
        "effect-gate"
    }
}

/// The gate's terminal report.
#[derive(Debug, Clone, PartialEq)]
pub struct GateOutcome {
    /// The settled outcome.
    pub outcome: SettledOutcome,
    /// The detected submission ref (`stop_rule = submit` marker).
    pub submission_ref: Option<String>,
    /// The `ErrorClass` spelling for retryable failures.
    pub error_class: Option<String>,
    /// The dispatch lifecycle rows the gate computed
    /// (`security.permission.decided`, `action.effect.{authorized,
    /// prepared, committed}` — the §5a.2 chain up to dispatch). The
    /// driver appends them in order before the terminal, under the run's
    /// writer lease — the producer computes, the fenced writer lands
    /// (CC3; the `AssembleOutcome.side_events` convention).
    pub emitted: Vec<(String, Json)>,
}

/// The kernel-owned inputs `assemble` reads beyond the strategy's
/// `context_request` — the driver supplies them (DF-S2.8-1): the `mc`
/// the plan records as `model_call_id`, the durable prefix the
/// `context_view` projection folds over, and the configured context
/// window cap (`0` ⇒ the assembler treats the window as unbounded).
#[derive(Debug, Clone)]
pub struct AssembleInputs<'a> {
    /// The call's `model_call_id` — the driver's `mc` on a `propose`,
    /// the retried call's id on a `retry` re-assembly.
    pub model_call_id: &'a str,
    /// The run's durable event prefix (`LedgerSink::prefix`) — the
    /// `context_view` source.
    pub prefix: &'a [EventEnvelope],
    /// `context.window_cap_tokens` — the occupancy gauge's cap.
    pub window_cap_tokens: u64,
}

/// The context assembler — `assemble` turns a `propose.context_request`
/// into the request record the `ModelPort` consumes plus the canonical
/// `context.assembled` payload the `hh-context` builder produced (the
/// driver owns the `append` — DF-S1.19-1's Stage-1 half: the emitter call
/// site is the turn loop's). The strategy never sees the bytes — I2.
pub trait AssemblerPort {
    /// Assemble the next request.
    fn assemble(&mut self, inputs: &AssembleInputs<'_>, context_request: &Json)
        -> AssembledRequest;
}

/// What `assemble` returns.
#[derive(Debug, Clone, PartialEq)]
pub struct AssembledRequest {
    /// The request record the `ModelPort` consumes.
    pub request: Json,
    /// The `context.assembled` payload (the builder's canonical record —
    /// `None` only for a port that does not assemble).
    pub assembled_payload: Option<Json>,
    /// The builder's side-band rows — `context.artefact.delivered` and
    /// any other rows the assembler emitted besides `context.assembled`,
    /// in emit order. The driver appends them after `context.assembled`
    /// under the same call scope (DF-S2.8-1 — a builder's emissions are
    /// durable or they never ran; nothing is dropped silently).
    pub side_events: Vec<(String, Json)>,
    /// Rows the context pipeline emitted *before* the assembled row
    /// (`context.procedure.selected` — the §5c.5 selector runs ahead of
    /// assembly; R2.5 / DF-S2.8-1). The driver appends them first, under
    /// the same call scope, preserving the producer's emit order.
    pub pre_events: Vec<(String, Json)>,
}

/// Durability — the sink the driver appends through (the `hh-ledger`
/// `Store::append` in production; a vec in tests). `prefix()` is the
/// durable prefix every pure fold reads — the driver never keeps a second
/// copy of ledger state (the ledger is the sole durable source of truth).
pub trait LedgerSink {
    /// Append the batch (atomic; `Fenced`/`RunFinished` are the sink's
    /// typed errors as strings for the driver's typed `DriverError`).
    fn append(&mut self, events: Vec<Event>) -> Result<(), String>;
    /// The durable prefix.
    fn prefix(&self) -> &[EventEnvelope];
}

/// The compaction boundary (R-2.4.2's `compact` driver — §5c.2; the S2.11
/// seam for `react/steerable`'s `compact` routing). The port owns
/// `context.compaction.started/completed` emission through its own sink —
/// the driver sees only the outcome. `Err(CompactionImpossible)` is the
/// exhausted I-FALLBACK ladder; the driver then stops `context_exhausted`
/// (CF-225) as an envelope-owned stop, never a strategy proposal.
pub trait CompactionPort {
    /// Run the compaction ladder for `reason` (a `CompactionRequired`-class
    /// spelling); `Ok` carries the post-compaction `context_view` hash the
    /// `compaction_completed` cue reports.
    fn compact(&mut self, reason: &str) -> Result<CompactionDone, CompactionImpossible>;
}

/// What a successful compaction reports.
#[derive(Debug, Clone, PartialEq)]
pub struct CompactionDone {
    /// The post-compaction `context_view` hash.
    pub view_hash: String,
    /// The `context.compaction.started`/`completed` (and any side-band)
    /// rows the compaction produced, in emit order. The driver appends
    /// them under the run's writer lease — the producer computes, the
    /// fenced writer lands (CC3; the `AssembleOutcome.side_events`
    /// convention — R2.5 / DF-S2.8-1's durable-from-the-loop leg).
    pub emitted: Vec<(String, Json)>,
}

/// The context/memory producer boundary (R-2.4.3/§5c.3-4 — DF-S2.8-1's
/// `retrieve`, `resume_set`, `trigger` and `mark_scope_ended` legs). The
/// port owns the memory store and the retrieval pipeline; the driver owns
/// the appends — every return vector is `(class, payload)` rows in emit
/// order, landed durable-before-visible under the run's writer lease.
pub trait MemoryPort {
    /// A strategy-admitted `retrieve{query}` (the model-owned `retrieve`
    /// decision point). The port interprets the query member and runs the
    /// retrieval pipeline at `watermark` (`(run_id, seq)` — the read's
    /// `until` point), returning `context.retrieval.completed` +
    /// `context.memory.read` in emit order. `Err` is the port's typed
    /// failure — the run fails, never silently skipped.
    fn retrieve(
        &mut self,
        query: &Json,
        model_call_id: &str,
        watermark: (String, u64),
    ) -> Result<Vec<(String, Json)>, String>;
    /// The `trigger{path_touched}` leg (AC-R-2.4.3-12) — a settled effect
    /// named `path`; the port retrieves the memory/procedure versions
    /// watching it (the next `assemble` delivers their index candidates).
    fn trigger_retrieve(
        &mut self,
        path: &str,
        model_call_id: &str,
        watermark: (String, u64),
    ) -> Result<Vec<(String, Json)>, String>;
    /// `resume_set` consumption (§5c.4; AC-R-2.4.3-11) — each carried
    /// head is read `by_name`; the emitted `context.memory.read` rows
    /// record `delivered[]`/`withheld[]` for the continuation's members.
    fn resume_set_read(
        &mut self,
        heads: &[String],
        watermark: (String, u64),
    ) -> Result<Vec<(String, Json)>, String>;
    /// `mark_scope_ended(scope)` — the unconditional expiry floor
    /// (§5c.4). Returns the `context.memory.invalidated{reason: expired,
    /// fired_stamp: scope_ended}` rows the floor produced, in store
    /// order (empty when nothing the scope covered remained live).
    fn mark_scope_ended(&mut self, scope: PersistenceScope, at_seq: u64) -> Vec<(String, Json)>;
    /// A store-backed procedure artefact's declared
    /// `allowed_capabilities` — the `context.artefact.activated` member
    /// the deterministic `followed` detector reads. `None` ⇒ the artefact
    /// is not a store-backed procedure (the activation row still lands —
    /// capless, so no `followed` can mint from it).
    fn procedure_capabilities(&self, artefact_id: &str) -> Option<Vec<String>>;
}

/// `CompactionImpossible{required_tokens, cap}` — the exhausted ladder's
/// report (§5c.2; the driver stops `context_exhausted` with these numbers).
#[derive(Debug, Clone, PartialEq)]
pub struct CompactionImpossible {
    /// The tokens the next step required.
    pub required_tokens: u64,
    /// The context window cap.
    pub cap: u64,
}

/// The decision-point verification boundary (R-2.7.1's `verify` execution
/// seam — §5e.1 `verify{validator_refs, subject}`). The port binds the
/// declared `Ref<Validator>`s and returns the `Verdict`s it actually ran —
/// never a pass it did not compute. The driver appends
/// `verification.validator.invoked` + `verification.validator.verdict` per
/// verdict and cues `verification_completed`.
pub trait VerifyPort {
    /// Run `validator_refs` over `subject`; returns the verdicts it
    /// actually produced (an unbound declared validator yields an
    /// `inconclusive` verdict, never a silent skip — R-2.7.1).
    fn verify(
        &mut self,
        validator_refs: &[String],
        subject: &Json,
    ) -> Vec<hh_verification::validators::Verdict>;
}

/// A guard-fired nudge `HarnessRule` awaiting its T-LCD-13 `followed` row —
/// `delivered`/`activated` land when the nudge fires; `followed` lands with
/// the strategy's next admitted decision (AC-R-2.6.1-8).
#[derive(Debug, Clone, PartialEq)]
pub struct NudgePending {
    /// The minted `HarnessRule`'s `rule_id` (the artefact id).
    pub rule_id: String,
    /// The `delivery_id` the `delivered`/`activated` rows share.
    pub delivery_id: String,
    /// The nudge kind (`loop_nudge | continue_nudge` — §5e.1 ledger).
    pub kind: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// DriverError / RunResult
// ─────────────────────────────────────────────────────────────────────────────

/// `DriverError` — the driver's typed failures (never panics on malformed
/// input; a port error surfaces as `Port{class, detail}`).
#[derive(Debug, Clone, PartialEq)]
pub enum DriverError {
    /// `strategy.open` refused (`IncompatibleBoundary`, `MissingProcedure`,
    /// `UnsupportedPlanNode`).
    Open(ControlError),
    /// The envelope refused to arm.
    Arm(String),
    /// The sink refused an append (`fenced`, `run_finished`, …).
    Append(String),
    /// A port returned a malformed record.
    Port {
        /// The port.
        port: &'static str,
        /// What was malformed.
        detail: String,
    },
    /// `restore` refused the checkpoint.
    Restore(crate::strategy::RestoreError),
    /// A decision named a boundary no port is wired for (`verify` without a
    /// `VerifyPort`, `retrieve`/`delegate` likewise) — the declared step
    /// fails typed, never silently skipped (R-2.7.1).
    UnbackedPort {
        /// The decision kind that needed the port.
        kind: &'static str,
    },
    /// A kill-point fault fired (R-2.2.3⁰ᶜ KP-8 — runtime death after the
    /// settled cue was visible, before the next `control.decision`): the
    /// battery's `inject` seam; every durable row before it is already down.
    FaultInjected {
        /// The kill point that fired.
        at: String,
    },
}

impl std::fmt::Display for DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriverError::Open(e) => write!(f, "open{{{e}}}"),
            DriverError::Arm(e) => write!(f, "arm{{{e}}}"),
            DriverError::Append(e) => write!(f, "append{{{e}}}"),
            DriverError::Port { port, detail } => write!(f, "port{{{port}:{detail}}}"),
            DriverError::Restore(e) => write!(f, "restore{{{e}}}"),
            DriverError::UnbackedPort { kind } => write!(f, "unbacked_port{{{kind}}}"),
            DriverError::FaultInjected { at } => write!(f, "fault_injected{{{at}}}"),
        }
    }
}

impl std::error::Error for DriverError {}

/// What `finish` resolved to — `Done` is the terminal `RunResult`; `Held`
/// means the completion gate queued a `completion_refused` cue and the
/// strategy loop continues (S3.10; §5f.2).
#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // `Held` is the common arm; boxing `RunResult` would allocate on every finish.
enum FinishOutcome {
    /// The run finished.
    Done(RunResult),
    /// The completion gate held — the strategy is re-invoked.
    Held,
}

/// The completion gate's disposition (S3.10).
#[allow(clippy::large_enum_variant)] // `Held` is the common arm; boxing the decided arm would allocate per decision.
enum CompletionFlow {
    /// `gate.evaluated{hold}` within the cap — feedback queued.
    Held,
    /// A terminal `completion.decided` was emitted.
    Decided(CompletionGateDecision),
}

/// The gate's decided half — the `CompletionDecision` plus the
/// `StopReason` override the finish path applies (`budget_exhausted`
/// on holds-cap exhaustion).
struct CompletionGateDecision {
    /// The emitted decision record.
    decision: hh_verification::gate::CompletionDecision,
    /// The finish-path stop reason override (holds exhaustion).
    stop_reason: Option<StopReason>,
}

impl CompletionGateDecision {
    /// Attach the finish-path stop reason (cap exhaustion).
    fn with_stop(mut self, reason: StopReason) -> Self {
        self.stop_reason = Some(reason);
        self
    }
}

/// `RunResult` — what `run()` returns (the terminal `FinalReport` plus the
/// `DrainReport` the stop protocol produced).
#[derive(Debug, Clone, PartialEq)]
pub struct RunResult {
    /// The strategy's `terminate` report.
    pub report: FinalReport,
    /// The drain record (`drain_report_ref` content-addresses its JSON).
    pub drain: DrainReport,
    /// The decision event ids emitted (the `control.decision` audit list).
    pub decision_events: Vec<String>,
}

/// `DriverConfig` — the injected facts the driver needs that are not the
/// strategy's or the policy's (the logical clock's start, the compiled
/// surface set for G-INTERPRET, the gauge caps, the budget-conservation
/// view source, attendance).
#[derive(Debug, Clone)]
pub struct DriverConfig {
    /// The compiled surface set (`ModelSurface` projection — G-INTERPRET).
    pub surfaces: Vec<SurfaceSpec>,
    /// The gauge caps (`context.occupancy`, `delegation_depth`, `fan_out`).
    pub gauge_caps: crate::guards::GaugeCaps,
    /// `remaining(dimension)` — the caller-maintained budget view for
    /// dimensions the driver does not itself count (outside-run
    /// reservations, `spend`, `approvals.requested`).
    pub remaining: std::collections::BTreeMap<String, i64>,
    /// The declared hard ceilings for the dimensions the driver folds
    /// itself — `model_calls` (`model.call.requested` count), `turns`
    /// (`control.decision` count), `retries` (`control.retry.scheduled`
    /// count), `time.working_ms`/`time.wall_ms` (the logical clock).
    /// `remaining` presented to the guards is `ceiling − consumed`;
    /// exhaustion is `remaining ≤ 0` (AC-R-2.6.2-2).
    pub budget_ceiling: std::collections::BTreeMap<String, i64>,
    /// The `retries` hard ceiling (INV-6).
    pub retries_ceiling: u64,
    /// `interactive` attendance (`escalate` on exhaustion — C1).
    pub interactive_attendance: bool,
    /// The profile's `max_output` bound (reservation sizing — ADR-0107 D6).
    pub profile_max_output_bound: u64,
    /// The context `window_cap` in tokens — the `context.occupancy` gauge's
    /// denominator (I-BUDGET's cap; §5c.1). `0` ⇒ unfurnished: the occupancy
    /// gauge reads 0 and the `compaction_required` cap never fires.
    pub window_cap_tokens: u64,
    /// The `TaskContract` the completion gate reads (§5f.2; S3.10) —
    /// projected at `seal`/`open` and stamped `manifest.task_contract_id`.
    /// `None` ⇒ the gate reads open effects + the claim's own divergences
    /// only (no contract criteria — the T-LCD-03 anchor shape).
    pub task_contract: Option<hh_verification::gate::TaskContract>,
    /// The `reconciliation.holds` cap (F4; ADR-0113 D4 — default
    /// [`hh_verification::gate::DEFAULT_HOLDS_CAP`]; per-suite override
    /// OQ-279).
    pub holds_cap: u64,
    /// The model-emitted-plan surface (`plan_execute`'s `hh.plan` — S3.10):
    /// a validated call on this surface emits `control.plan.emitted` (the
    /// plan is schema-validated + closed-world-checked, never dispatched
    /// as an effect). `None` ⇒ no plan surface is declared.
    pub plan_surface_id: Option<String>,
    /// The plan-schema validator ref (`control.plan.emitted`'s
    /// `schema_validator_ref` member).
    pub plan_schema_ref: String,
    /// Whether the binding declares the R-2.6.3 delegation capability
    /// (ADR-0186 D4; the `capabilities_available` binding — `false` ⇒ a
    /// `delegate` decision refuses `DelegationUnavailable`, T0). Default
    /// `false` — a profile declares the capability or it is absent.
    pub delegation_available: bool,
    /// The `compute_policy` variant bound in
    /// `AgentProcess.native.slots["compute_policy"]` (§5e.4): `static` (the
    /// default — the slot binds nothing and the run stays byte-identical
    /// to one without it), `uniform`, `rules`. Any other ref fails
    /// `policy_for`'s `VariantNotAdmitted` at the first bind.
    pub compute_policy_ref: String,
    /// The declared compute facts the driver cannot fold itself
    /// (`ensemble`, `parallel`/`subagent_task` declarations,
    /// `profile.capabilities`, `role_table`, `placements`, `priors`,
    /// `cost_model`, `task_value`, `delegation_depth`) — sealed at
    /// `open`/`resume` from the manifest/profile projection. Everything
    /// else in the `ComputeContext` folds from the durable prefix.
    pub compute_facts: crate::compute::ComputeFacts,
    /// The definition-declared `RulesConfig` — the conditioned thresholds
    /// and `delegation_rules[]` the sealed `compute_policy` slot params
    /// project (S5.5's profile-conditioned rules; `None` = the default
    /// config — `bandit`/`surface_prior` still run, with no rules'
    /// delegation conditioning).
    pub compute_rules: Option<crate::compute::RulesConfig>,
    /// The `judged` loop detector's `Validator{kind: judge}` port
    /// (Stage-4; binds the `Validator` the `LoopPolicy.judged.validator_ref`
    /// names — `None` with a declared spec means the detector never
    /// fires, never gets guessed at).
    pub judge: Option<std::sync::Arc<dyn crate::loops::JudgePort>>,
    /// The `execution_alignment` C2 reconciler declaration (R-2.7.2b;
    /// S4.16c): `Some` binds the belief-state reconciler — `completion_gate`
    /// runs `reconcile_c2` over the projected `ReconcileContext` (D1/D4/
    /// D7–D10 on top of the C0 classes). `None` (the default) keeps the
    /// byte-identical C0 `ledger_only` fold.
    pub reconciler: Option<hh_verification::reconciler::ReconcilerDeclaration>,
    /// The §5a.3 `carried.resume_set_heads` this activation inherited —
    /// projected at `open`/`resume` from the manifest's durable `carried`
    /// member (CC3 — the manifest is the record; the driver consumes,
    /// never re-derives). Non-empty ⇒ the run's first `run` drains the
    /// set through the wired `MemoryPort` (one `context.retrieval.
    /// completed` plus one `context.memory.read` per head —
    /// AC-R-2.4.3-11) before the first `decide`; an armed set with no
    /// memory boundary fails `UnbackedPort{kind: "resume_set"}`, never a
    /// silent skip.
    pub resume_set_heads: Vec<String>,
}

impl Default for DriverConfig {
    fn default() -> Self {
        DriverConfig {
            surfaces: vec![],
            gauge_caps: crate::guards::GaugeCaps {
                occupancy_ppm: 900_000,
                delegation_depth: 4,
                fan_out: 8,
            },
            remaining: std::collections::BTreeMap::new(),
            budget_ceiling: std::collections::BTreeMap::new(),
            retries_ceiling: 64,
            interactive_attendance: false,
            profile_max_output_bound: 8_192,
            window_cap_tokens: 0,
            task_contract: None,
            holds_cap: hh_verification::gate::DEFAULT_HOLDS_CAP,
            plan_surface_id: None,
            plan_schema_ref: crate::plan_exec::PLAN_SCHEMA_REF.to_string(),
            delegation_available: false,
            compute_policy_ref: "static".to_string(),
            compute_facts: crate::compute::ComputeFacts::default(),
            compute_rules: None,
            judge: None,
            reconciler: None,
            resume_set_heads: Vec::new(),
        }
    }
}

/// The driver — owns the cue inbox, the logical clock, the id allocator
/// and the `decide → check → emit → execute` loop over injected ports.
pub struct Driver<S: ControlStrategy> {
    /// The armed envelope.
    pub envelope: Envelope,
    /// The armed envelope state.
    pub envelope_state: EnvelopeState,
    strategy: S,
    state: ControlState,
    /// The cue inbox (FIFO; `submit`/`next_cue` — the spec's queue).
    inbox: std::collections::VecDeque<Cue>,
    /// The logical clock (ms) — injectable, monotone.
    now_ms: u64,
    /// The id allocator (monotone — `mc-N`/`tc-N`/`ef-N`/`d-N`/`e-N`).
    next_id: u64,
    /// The recorded decision event ids.
    decision_events: Vec<String>,
    /// The `control.compute.decided` record ids this run emitted
    /// (`lifecycle.run.finished`'s `compute_decisions` member — empty for
    /// `static`, which never appends).
    compute_records: Vec<String>,
    /// The last `control.decision` event ref (`causes ∋` for `intended`).
    last_decision_ref: Option<EventRef>,
    /// Whether a submission has been detected (`stop_rule` reading).
    submission: Option<String>,
    /// The model-call retry counter for the F2 seam.
    config: DriverConfig,
    /// A guard's `stop` verdict pending the drain (the envelope owns the
    /// reason — `run` picks it up after `execute`).
    stop_pending: Option<StopReason>,
    /// The derived deadline table — `scope_id → deadline_ms` from the
    /// `TimeoutPolicy` at scope open (INV-1's input; the driver derives,
    /// never guesses).
    deadlines: std::collections::BTreeMap<String, u64>,
    /// The run id the claim ledger records (`verification.claim.recorded`'s
    /// `run_id` — the `AgentProcess` ref of this run).
    run_id: String,
    /// The *ledger* run id — the durable prefix's `run_id` member. `causes`
    /// refs resolve against it (a fenced sink refuses a dangling
    /// `UnresolvedEventRef`); `run_id` above is the AgentProcess ref the
    /// verification claim records, never a ledger coordinate.
    ledger_run_id: String,
    /// The most recent `model_call_id` scope (the claim's `model_call_id` —
    /// the call whose output the completion claim rides).
    last_model_call_id: Option<String>,
    /// The compaction port (`compact{reason}` — R-2.4.2; `None` ⇒ the
    /// occupancy cap escalates straight to `context_exhausted`, the
    /// portable definition's honest exhausted ladder).
    compaction_port: Option<Box<dyn CompactionPort>>,
    /// The verification port (`verify{validator_refs, subject}` — R-2.7.1;
    /// `None` ⇒ a `verify` decision fails `DriverError::UnbackedPort` — a
    /// declared verification never silently passes).
    verify_port: Option<Box<dyn VerifyPort>>,
    /// The memory/context producer port (R-2.4.3 — `retrieve{query}`,
    /// `trigger{path_touched}`, `resume_set` consumption and
    /// `mark_scope_ended` at the run's end). `None` ⇒ a `retrieve`
    /// decision or an armed `resume_set` fails `UnbackedPort` — a
    /// declared producer leg never silently skips.
    memory_port: Option<Box<dyn MemoryPort>>,
    /// Whether this armed driver already drained `config.resume_set_heads`
    /// (once per process-arm — the emitted `context.memory.read` rows are
    /// the durable consumption record; a re-armed session re-reads, which
    /// is honest re-projection, never a double count).
    resume_set_drained: bool,
    /// A guard-fired nudge `HarnessRule` awaiting its `followed` verdict —
    /// set when a `LadderAction::Nudge`/`RuleAction::Nudge`/`missing_
    /// submission` respond fires (the delivered/activated pair is emitted
    /// there), cleared when the strategy's next admitted decision lands the
    /// `verification.artefact.followed` row (T-LCD-13; AC-R-2.6.1-8).
    pending_nudge: Option<NudgePending>,
    /// The subject model ref (the claim provenance's `Origin::Model.model_ref`
    /// — read from the sealed profile projection, `model_ref` key).
    model_ref: String,
    /// The armed kill point (R-2.2.3⁰ᶜ KP-8 — the battery's `inject`):
    /// fires when the loop next pops the `effects_settled` cue — runtime
    /// death after the observation is visible, before the decision it
    /// would drive. Armed once, fires once; production never arms it.
    kill_point: Option<hh_ledger::fault::KillPoint>,
    /// The `control.random.read` draw ordinal — seeded from the durable
    /// prefix at resume/replay (a recorded draw reproduces; a fresh draw
    /// never collides with a pre-crash one).
    random_draws: u64,
    /// The completion claims `emit_completion_claims` recorded on this
    /// `stop{completed}` (the gate reconciles them — S3.10).
    completion_claims: Vec<hh_verification::claims::Claim>,
}

impl<S: ControlStrategy> Driver<S> {
    /// `open` — arm the envelope over the durable prefix, `strategy.open`
    /// the `ControlContext`, seed the inbox with `run_opened`.
    pub fn open(
        mut strategy: S,
        ctx: &ControlContext,
        policy: EnvelopePolicy,
        sink: &mut dyn LedgerSink,
        config: DriverConfig,
    ) -> Result<Driver<S>, DriverError> {
        let (envelope, envelope_state) =
            Envelope::arm(policy, sink.prefix()).map_err(|e| DriverError::Arm(e.to_string()))?;
        let state = strategy.open(ctx).map_err(DriverError::Open)?;
        let mut inbox = std::collections::VecDeque::new();
        inbox.push_back(Cue::RunOpened {
            goal_ref: ctx.process_ref.clone(),
            inputs: Json::Null,
        });
        sink.append(vec![crate::events::kernel_event(
            "e-0".into(),
            "lifecycle.turn.started",
            "1970-01-01T00:00:00.000Z".into(),
            Scope {
                turn_id: Some("turn-1".into()),
                ..scope_empty()
            },
            hh_ledger::ids::ROOT_EVENT.to_string(),
            vec![],
            Json::obj([
                ("turn_id", Json::str("turn-1")),
                ("process_ref", Json::str(&ctx.process_ref)),
            ]),
        )])
        .map_err(DriverError::Append)?;
        Ok(Driver {
            envelope,
            envelope_state,
            strategy,
            state,
            inbox,
            now_ms: 0,
            next_id: 0,
            decision_events: vec![],
            compute_records: vec![],
            last_decision_ref: None,
            submission: None,
            config,
            stop_pending: None,
            deadlines: std::collections::BTreeMap::new(),
            run_id: ctx.process_ref.clone(),
            ledger_run_id: sink
                .prefix()
                .first()
                .map(|e| e.run_id.clone())
                .unwrap_or_default(),
            last_model_call_id: None,
            compaction_port: None,
            verify_port: None,
            memory_port: None,
            resume_set_drained: false,
            pending_nudge: None,
            model_ref: ctx
                .profile
                .get("model_ref")
                .and_then(Json::as_str)
                .unwrap_or("model/subject")
                .to_string(),
            kill_point: None,
            random_draws: 0,
            completion_claims: vec![],
        })
    }

    /// **Resume-by-leaf constructor** (§5a.3; ADR-0130; S2.3) — the durable
    /// counterpart of `open`: builds the driver WITHOUT the `turn-1`/`e-0`
    /// opener (a resume never re-mints the opener — DuplicateEventId),
    /// restores `strategy` from the persisted leaf `checkpoint`, observes
    /// the durable tail past `last_cue_seq`, and re-arms the envelope over
    /// the committed prefix (G-RESUME). The inbox's first cue is
    /// `Cue.resumed{last_durable, recovery_decision}`.
    pub fn resume_from(
        mut strategy: S,
        ctx: &ControlContext,
        policy: EnvelopePolicy,
        checkpoint: &[u8],
        sink: &mut dyn LedgerSink,
        config: DriverConfig,
    ) -> Result<Driver<S>, DriverError> {
        // A placeholder state — `resume` overwrites it from the checkpoint.
        let state = strategy.open(ctx).map_err(DriverError::Open)?;
        let (envelope, envelope_state) = Envelope::arm(policy.clone(), sink.prefix())
            .map_err(|e| DriverError::Arm(e.to_string()))?;
        // The emitted compute records — re-folded from the prefix so a
        // resumed run's `run.finished` names the whole list.
        let compute_records: Vec<String> = sink
            .prefix()
            .iter()
            .filter(|e| e.class == "control.compute.decided")
            .filter_map(|e| {
                e.payload
                    .get("record_id")
                    .and_then(Json::as_str)
                    .map(str::to_string)
            })
            .collect();
        // The alloc watermark — `{tag}-{n}` ids share one counter; the
        // durable prefix's max `n` continues numbering so a resumed
        // driver's first `decision` does not re-mint `d-1` over the row
        // the pre-crash writer already committed (the same rule
        // `replay_from` applies over a seeded prefix — CC1).
        let mut next_id = 0u64;
        for e in sink.prefix() {
            if let Some((_, n)) = e.event_id.rsplit_once('-') {
                if let Ok(v) = n.parse::<u64>() {
                    next_id = next_id.max(v);
                }
            }
        }
        let decision_events: Vec<String> = sink
            .prefix()
            .iter()
            .filter(|e| e.class == "control.decision")
            .map(|e| e.event_id.clone())
            .collect();
        let last_decision_ref = decision_events.last().map(|id| EventRef {
            run_id: sink
                .prefix()
                .first()
                .map(|e| e.run_id.clone())
                .unwrap_or_default(),
            event_id: id.clone(),
        });
        let mut driver = Driver {
            envelope,
            envelope_state,
            strategy,
            state,
            inbox: std::collections::VecDeque::new(),
            now_ms: 0,
            next_id,
            decision_events,
            compute_records,
            last_decision_ref,
            submission: None,
            config,
            stop_pending: None,
            deadlines: std::collections::BTreeMap::new(),
            run_id: ctx.process_ref.clone(),
            ledger_run_id: sink
                .prefix()
                .first()
                .map(|e| e.run_id.clone())
                .unwrap_or_default(),
            last_model_call_id: None,
            compaction_port: None,
            verify_port: None,
            memory_port: None,
            resume_set_drained: false,
            pending_nudge: None,
            model_ref: ctx
                .profile
                .get("model_ref")
                .and_then(Json::as_str)
                .unwrap_or("model/subject")
                .to_string(),
            kill_point: None,
            random_draws: sink
                .prefix()
                .iter()
                .filter(|e| e.class == "control.random.read")
                .count() as u64,
            completion_claims: vec![],
        };
        driver.resume(ctx, checkpoint, sink)?;
        Ok(driver)
    }

    /// **Replay constructor** (R-2.2.4⁰ᵇ; §5a.4; the `deterministic`
    /// driver's branch half) — arm a driver over a *seeded* sink prefix:
    /// `Envelope::arm` + `strategy.open` + `observe(seed)` (the same fold
    /// `resume` applies to the durable tail — the deterministic-replay
    /// claim is that the fold reproduces the live driver's state at the
    /// cut), then the alloc watermark, decision refs and the submission
    /// marker seeded from the recorded rows so a re-driven suffix mints
    /// the same `d-N`/`mc-N` ids. The inbox starts empty — the replay
    /// feeds the recorded cues; `run_opened`/`resumed` never re-mint.
    /// A divergence from the record surfaces as a `control.decision`
    /// mismatch the caller reports `invalid` — never a silent reconcile.
    pub fn replay_from(
        mut strategy: S,
        ctx: &ControlContext,
        policy: EnvelopePolicy,
        sink: &mut dyn LedgerSink,
        config: DriverConfig,
    ) -> Result<Driver<S>, DriverError> {
        let seed: &[EventEnvelope] = sink.prefix();
        let (envelope, envelope_state) =
            Envelope::arm(policy.clone(), seed).map_err(|e| DriverError::Arm(e.to_string()))?;
        let mut state = strategy.open(ctx).map_err(DriverError::Open)?;
        strategy.observe(&mut state, seed);
        // The alloc watermark — `{tag}-{n}` ids share one counter; the
        // seed's max `n` continues numbering so decision ids reproduce.
        let mut next_id = 0u64;
        for e in seed {
            if let Some((_, n)) = e.event_id.rsplit_once('-') {
                if let Ok(v) = n.parse::<u64>() {
                    next_id = next_id.max(v);
                }
            }
        }
        let decision_events: Vec<String> = seed
            .iter()
            .filter(|e| e.class == "control.decision")
            .map(|e| e.event_id.clone())
            .collect();
        // The `causes` ref names the run's own id — the replayed
        // prefix's `run_id`, never a placeholder (a fenced sink resolves
        // every `causes` ref — `UnresolvedEventRef` on a dangling id).
        let seed_run_id = seed.first().map(|e| e.run_id.clone()).unwrap_or_default();
        let last_decision_ref = decision_events.last().map(|id| EventRef {
            run_id: seed_run_id.clone(),
            event_id: id.clone(),
        });
        // The submission marker — a recorded `stop` decision carrying
        // `submission_ref` is the `hh.submit` completion the seed reached.
        let submission = seed
            .iter()
            .find(|e| e.class == "control.decision" && e.payload.get("submission_ref").is_some())
            .and_then(|e| {
                e.payload
                    .get("submission_ref")
                    .and_then(Json::as_str)
                    .map(str::to_string)
            });
        let compute_records: Vec<String> = seed
            .iter()
            .filter(|e| e.class == "control.compute.decided")
            .filter_map(|e| {
                e.payload
                    .get("record_id")
                    .and_then(Json::as_str)
                    .map(str::to_string)
            })
            .collect();
        let last_model_call_id = seed
            .iter()
            .rev()
            .find(|e| e.class == "model.call.requested")
            .and_then(|e| {
                e.payload
                    .get("model_call_id")
                    .and_then(Json::as_str)
                    .map(str::to_string)
            });
        Ok(Driver {
            envelope,
            envelope_state,
            strategy,
            state,
            inbox: std::collections::VecDeque::new(),
            now_ms: 0,
            next_id,
            decision_events,
            compute_records,
            last_decision_ref,
            submission,
            config,
            stop_pending: None,
            deadlines: std::collections::BTreeMap::new(),
            run_id: ctx.process_ref.clone(),
            ledger_run_id: sink
                .prefix()
                .first()
                .map(|e| e.run_id.clone())
                .unwrap_or_default(),
            last_model_call_id,
            compaction_port: None,
            verify_port: None,
            memory_port: None,
            resume_set_drained: false,
            pending_nudge: None,
            model_ref: ctx
                .profile
                .get("model_ref")
                .and_then(Json::as_str)
                .unwrap_or("model/subject")
                .to_string(),
            kill_point: None,
            random_draws: 0,
            completion_claims: vec![],
        })
    }

    /// `submit(cue)` — enqueue a cue (the wakeup/ingress path; delivery is
    /// ledger-ordered — I7: cues are the strategy's only input).
    pub fn submit(&mut self, cue: Cue) {
        self.inbox.push_back(cue);
    }

    /// The folded `GuardContext` at `now` — `remaining` is the caller's
    /// pass-through view merged with `ceiling − consumed` for the
    /// driver-counted dimensions (the consumption fold is over the durable
    /// prefix — a pure read, never a widening).
    fn guard_ctx(&self, events: &[EventEnvelope]) -> GuardContext {
        let mut remaining = self.config.remaining.clone();
        if !self.config.budget_ceiling.is_empty() {
            let view = crate::views::fold_envelope_view(events);
            let model_calls = events
                .iter()
                .filter(|e| e.class == "model.call.requested")
                .count() as i64;
            for (dim, cap) in &self.config.budget_ceiling {
                let used = match dim.as_str() {
                    "model_calls" => model_calls,
                    "retries" => view.retries_scheduled as i64,
                    "turns" => view.decisions as i64,
                    "time.working_ms" | "time.wall_ms" => self.now_ms as i64,
                    _ => 0,
                };
                remaining.insert(dim.clone(), cap - used);
            }
        }
        // The context gauges — `context.occupancy_*` fold from the last
        // `context.assembled` row's `occupancy_estimate` against the
        // configured `window_cap` (I-BUDGET's cap; §5c.1). `0` cap ⇒ the
        // gauge reads 0 and the `compaction_required` cap never fires.
        let mut gauges: std::collections::BTreeMap<String, i64> = Default::default();
        if self.config.window_cap_tokens > 0 {
            let occupied = events
                .iter()
                .rev()
                .find(|e| e.class == "context.assembled")
                .and_then(|e| e.payload.get("occupancy_estimate").and_then(Json::as_int))
                .map(|t| t.max(0) as u64)
                .unwrap_or(0);
            gauges.insert("context.occupancy_tokens".into(), occupied as i64);
            gauges.insert(
                "context.window_cap_tokens".into(),
                self.config.window_cap_tokens as i64,
            );
            gauges.insert(
                "context.occupancy_ppm".into(),
                (occupied.saturating_mul(1_000_000) / self.config.window_cap_tokens) as i64,
            );
        }
        GuardContext {
            remaining,
            retries_ceiling: self.config.retries_ceiling,
            now_ms: self.now_ms,
            deadlines: self.deadlines.clone(),
            gauges,
            effect_classes: Default::default(),
            cancel_requested: None,
            interactive_attendance: self.config.interactive_attendance,
            delegation_available: self.config.delegation_available,
            judge: self.config.judge.clone(),
        }
    }

    /// `compute_policy.bind(d, ctx)` — the §5e.4 seam. `static` never
    /// reaches the estimator (empty capability set — the driver
    /// short-circuits, so a `static`-bound run appends zero
    /// `control.compute.decided` rows and is byte-identical modulo ids to
    /// a run without the slot). A provider-drift / profile-supersession
    /// row newer than the last reset clears the ctx's priors and lands
    /// `control.compute.prior_reset` first (AC-F4-9's rules-fallback
    /// evidence). A typed estimator degradation
    /// (`EstimatorBudgetExhausted`/`TaskValueMissing`) degrades to
    /// `Unchanged` with a degraded record — never a stop; a `PolicyInvalid`
    /// (an invalid bound member — `check_bound`'s post-check) degrades the
    /// same way rather than emitting an unverifiable binding.
    fn bind_compute(
        &mut self,
        sink: &mut dyn LedgerSink,
        decision: crate::vocab::ControlDecision,
    ) -> Result<
        (
            crate::vocab::ControlDecision,
            Option<crate::compute::ComputeDecisionRecord>,
        ),
        DriverError,
    > {
        let variant = self.config.compute_policy_ref.clone();
        if variant == "static" {
            return Ok((decision, None));
        }
        let policy =
            crate::compute::policy_for_configured(&variant, self.config.compute_rules.clone())
                .map_err(|e| DriverError::Port {
                    port: "compute_policy",
                    detail: e.to_string(),
                })?;
        // Prior reset — drift/supersession since the last reset (or open)
        // voids the cells the ctx reads; the reset row lands before the
        // bind's record so the empty `priors_used[]` has its evidence.
        {
            let prefix = sink.prefix();
            let reset_at = prefix.iter().rposition(|e| {
                matches!(
                    e.class.as_str(),
                    "model.rerouted" | "model.profile.expired_used"
                )
            });
            let last_reset = prefix
                .iter()
                .rposition(|e| e.class == "control.compute.prior_reset");
            if let Some(i) = reset_at {
                if last_reset.map(|r| r < i).unwrap_or(true) {
                    let trigger_ref = prefix[i].event_id.clone();
                    let trigger_class = prefix[i].class.clone();
                    let reason = if trigger_class == "model.rerouted" {
                        "provider_drift"
                    } else {
                        "profile_superseded"
                    };
                    self.append(
                        sink,
                        "control.compute.prior_reset",
                        Json::obj([
                            ("trigger_event_ref", Json::str(&trigger_ref)),
                            ("trigger_class", Json::str(&trigger_class)),
                            ("reason", Json::str(reason)),
                            ("policy_ref", Json::str(&variant)),
                        ]),
                        None,
                    )?;
                }
            }
        }
        let ctx = self.compute_ctx(sink.prefix());
        let outcome = match policy.bind(&decision, &ctx) {
            Ok(o) => o,
            Err(
                e @ (crate::compute::ComputeError::EstimatorBudgetExhausted
                | crate::compute::ComputeError::TaskValueMissing
                | crate::compute::ComputeError::PolicyInvalid { .. }),
            ) => crate::compute::BindOutcome::Unchanged {
                record: crate::compute::degraded_record(&decision, &ctx, policy.variant_ref(), &e),
            },
            Err(e) => {
                return Err(DriverError::Port {
                    port: "compute_policy",
                    detail: e.to_string(),
                })
            }
        };
        match outcome {
            crate::compute::BindOutcome::Bound {
                decision: d2,
                record,
            } => {
                // B-1/B-2's post-check — the bound decision must preserve
                // kind/point/owner and only tighten; an inadmissible
                // binding degrades to `Unchanged`, never lands.
                if let Err(e) = crate::compute::check_bound(&decision, &d2, &ctx) {
                    let record =
                        crate::compute::degraded_record(&decision, &ctx, policy.variant_ref(), &e);
                    return Ok((decision, Some(record)));
                }
                Ok((d2, Some(record)))
            }
            crate::compute::BindOutcome::Unchanged { record } => Ok((decision, Some(record))),
        }
    }

    /// The folded `ComputeContext` at `now` — the sealed `compute_facts`
    /// for what the definition declares, the durable prefix for everything
    /// the run observed (a pure read; a replayed prefix rebuilds the
    /// identical ctx — B-6).
    fn compute_ctx(&self, events: &[EventEnvelope]) -> crate::compute::ComputeContext {
        let facts = &self.config.compute_facts;
        // `remaining` — the same view the guards read (the caller's
        // pass-through merged with `ceiling − consumed`); unregistered
        // spellings drop (a `DimensionKey`-only bound is not a vector
        // member — the closed `DimensionId` set is what `ResourceVector`
        // stores).
        let gctx = self.guard_ctx(events);
        let mut remaining = hh_budget::quantity::ResourceVector::zero();
        for (dim, amt) in &gctx.remaining {
            if let Some(d) = hh_ontology::dimensions::DimensionId::parse(dim) {
                remaining.add(d, *amt);
            }
        }
        let occupancy_ppm = gctx
            .gauges
            .get("context.occupancy_ppm")
            .copied()
            .unwrap_or(0);
        // Live fan-out — spawned minus the three terminal rows.
        let spawned = events
            .iter()
            .filter(|e| e.class == "control.subagent.spawned")
            .count() as i64;
        let settled = events
            .iter()
            .filter(|e| {
                matches!(
                    e.class.as_str(),
                    "control.subagent.result"
                        | "control.subagent.cancelled"
                        | "control.subagent.detached"
                )
            })
            .count() as i64;
        // `samples_so_far` — one per `model.call.completed`, hashed by
        // `response_ref` (the content ref — identical outputs share it;
        // the body never crosses this seam).
        let samples: Vec<crate::compute::SampleFact> = events
            .iter()
            .filter(|e| e.class == "model.call.completed")
            .map(|e| crate::compute::SampleFact {
                output_hash: e
                    .payload
                    .get("response_ref")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
                verdicts: vec![],
            })
            .collect();
        // Verdict streaks — the trailing run of
        // `verification.validator.verdict` rows (bool values only; a
        // non-bool tail member stops the streak).
        let mut pass = 0u32;
        let mut fail = 0u32;
        for e in events
            .iter()
            .rev()
            .filter(|e| e.class == "verification.validator.verdict")
        {
            match e.payload.get("value") {
                Some(Json::Bool(true)) if fail == 0 => pass += 1,
                Some(Json::Bool(false)) if pass == 0 => fail += 1,
                _ => break,
            }
        }
        // Format-failure streak — the strategy's own counter
        // (`extension.format_error_streak`; a variant that does not track
        // it reads 0, never a guess).
        let format_failure = self
            .state
            .extension
            .get("format_error_streak")
            .and_then(Json::as_int)
            .unwrap_or(0)
            .max(0) as u32;
        // Priors — the AC-F4-9 read. A `control.compute.prior_reset` (or a
        // drift/supersession row whose reset lands in this bind) zeroes the
        // declared seeds (`n = 0`, `valid_until` = the cleared seq — the
        // `unknown` rendering) and hides pre-reset online cells; cells
        // newer than the reset rebuild from the [`prior_cells`] fold —
        // the kernel-derived materialized view over `control.compute.
        // decided` `cell` members (ADR-0189 D4/D7, ADR-0078).
        let reset_at = events.iter().rposition(|e| {
            matches!(
                e.class.as_str(),
                "model.rerouted" | "model.profile.expired_used"
            )
        });
        let last_reset = events
            .iter()
            .rposition(|e| e.class == "control.compute.prior_reset");
        let priors_reset = reset_at
            .map(|i| last_reset.map(|r| r < i).unwrap_or(true))
            .unwrap_or(false);
        let cleared_seq = {
            let landed = last_reset.map(|i| events[i].seq).unwrap_or(0);
            let pending = if priors_reset {
                reset_at.map(|i| events[i].seq).unwrap_or(0)
            } else {
                0
            };
            landed.max(pending)
        };
        let mut priors: Vec<crate::compute::PriorFact> = facts
            .priors
            .iter()
            .map(|p| crate::compute::PriorFact {
                n: if cleared_seq > 0 { 0 } else { p.n },
                interval: if cleared_seq > 0 { None } else { p.interval },
                valid_until: if cleared_seq > 0 {
                    Some(cleared_seq)
                } else {
                    p.valid_until
                },
                ..p.clone()
            })
            .collect();
        for (key, c) in crate::compute::prior_cells(events, cleared_seq) {
            match priors.iter_mut().find(|f| f.cell == key) {
                Some(f) => {
                    f.n = c.n;
                    if let Some(m) = c.mean_ppm {
                        f.mean_ppm = m;
                    }
                    f.interval = c.interval;
                    f.valid_until = None;
                }
                None => priors.push(crate::compute::PriorFact {
                    cell: key,
                    n: c.n,
                    mean_ppm: c.mean_ppm.unwrap_or(0),
                    interval: c.interval,
                    valid_until: None,
                }),
            }
        }
        // Health — two consecutive trailing attempt failures degrade the
        // model-plane view (conservative options only).
        let mut failed_tail = 0u32;
        for e in events.iter().rev() {
            match e.class.as_str() {
                "model.call.attempt.failed" => failed_tail += 1,
                "model.call.attempt.completed" => break,
                _ => {}
            }
        }
        crate::compute::ComputeContext {
            budget: crate::compute::BudgetView {
                remaining,
                reserved: hh_budget::quantity::ResourceVector::zero(),
                live_fan_out: spawned.saturating_sub(settled).max(0) as u32,
                fan_out_cap: self.config.gauge_caps.fan_out,
                delegation_depth: facts.delegation_depth,
                delegation_depth_cap: self.config.gauge_caps.delegation_depth,
                occupancy_ppm,
            },
            declared_parallel_steps: facts.declared_parallel_steps,
            subagent_task_targets: facts.subagent_task_targets.clone(),
            ensemble: facts.ensemble.clone(),
            samples,
            verifier: facts
                .verifier
                .clone()
                .or_else(|| self.verify_port.is_some().then(|| "executable".to_string())),
            validator_streaks: crate::compute::ValidatorStreaks {
                pass,
                fail,
                format_failure,
            },
            profile_capabilities: facts.profile_capabilities.clone(),
            role_table: facts.role_table.clone(),
            placements: facts.placements.clone(),
            priors,
            cost_model: facts.cost_model.clone(),
            health: if failed_tail >= 2 {
                crate::compute::HealthView::Degraded
            } else {
                crate::compute::HealthView::Ok
            },
            task_value: facts.task_value.clone(),
            context_label: facts.context_label.clone(),
            profile_ref: facts.profile_ref.clone(),
            snapshot_fingerprint: facts.snapshot_fingerprint.clone(),
            task_class: facts.task_class.clone(),
            now_seq: events.last().map(|e| e.seq).unwrap_or(0),
        }
    }

    /// `set_compaction_port` — wire the compaction boundary (R-2.4.2). The
    /// driver stops `context_exhausted` on `CompactionImpossible` or when
    /// no port is set — the exhausted ladder is a value, never a panic.
    pub fn set_compaction_port(&mut self, port: Box<dyn CompactionPort>) {
        self.compaction_port = Some(port);
    }

    /// `set_verify_port` — wire the decision-point verification boundary
    /// (R-2.7.1). A `verify` decision without the port fails
    /// `DriverError::UnbackedPort` (a declared verification never silently
    /// passes).
    pub fn set_verify_port(&mut self, port: Box<dyn VerifyPort>) {
        self.verify_port = Some(port);
    }

    /// `set_memory_port` — wire the context/memory producer boundary
    /// (R-2.4.3; DF-S2.8-1): `retrieve{query}` decisions,
    /// `trigger{path_touched}` reads on settled effects, the carried
    /// `resume_set` drain and `mark_scope_ended` at run end all dispatch
    /// through it — absent, each is a typed `UnbackedPort`, never a
    /// skipped leg.
    pub fn set_memory_port(&mut self, port: Box<dyn MemoryPort>) {
        self.memory_port = Some(port);
    }

    /// `clock_read(declaring)` — the runtime's wall-clock read seam
    /// (R-2.2.4⁰ᵇ recording rules; ADR-0135 §2; OQ-322's ratified default:
    /// durable only where the variant declares `deterministic_replay`).
    /// A declaring variant's read lands a `control.clock.read{value}` row
    /// the replay substitutes; a non-declaring variant's read is served
    /// ephemerally — the run never claims determinism it didn't record.
    /// The served value is the driver's logical working clock (the same
    /// counter `time.*` gauges read — confined to the event stream by
    /// construction, so a recorded read replays bit-for-bit).
    pub fn clock_read(
        &mut self,
        sink: &mut dyn LedgerSink,
        declaring: bool,
    ) -> Result<u64, DriverError> {
        let value = self.now_ms;
        if declaring {
            self.append(
                sink,
                "control.clock.read",
                Json::obj([
                    ("value", Json::Int(value as i64)),
                    ("kind", Json::str("working_ms")),
                ]),
                None,
            )?;
        }
        Ok(value)
    }

    /// `random_read(declaring)` — the runtime's randomness seam (the same
    /// recording rule): the draw is `H("control.random.read" ∥ run ∥
    /// ordinal)` — a deterministic stream a replay reproduces exactly —
    /// recorded `control.random.read{ordinal, value}` for a declaring
    /// variant, ephemeral otherwise.
    pub fn random_read(
        &mut self,
        sink: &mut dyn LedgerSink,
        declaring: bool,
    ) -> Result<String, DriverError> {
        let ordinal = self.random_draws;
        self.random_draws += 1;
        let value = format!(
            "sha256:{}",
            hh_wire::sha256::sha256_hex(
                format!("control.random.read:{}:{}", self.run_id, ordinal).as_bytes()
            )
        );
        if declaring {
            self.append(
                sink,
                "control.random.read",
                Json::obj([
                    ("ordinal", Json::Int(ordinal as i64)),
                    ("value", Json::str(&value)),
                ]),
                None,
            )?;
        }
        Ok(value)
    }

    /// Arm the KP-8 kill point (R-2.2.3⁰ᶜ; the battery's `inject`) — the
    /// next `effects_settled` pop returns `FaultInjected` instead of a
    /// decision: runtime death after the observation is visible, before
    /// the `control.decision` it would drive.
    pub fn inject_kill_point(&mut self, at: hh_ledger::fault::KillPoint) {
        self.kill_point = Some(at);
    }

    fn alloc(&mut self, tag: &str) -> String {
        self.next_id += 1;
        format!("{tag}-{}", self.next_id)
    }

    fn tick(&mut self, ms: u64) {
        self.now_ms = self.now_ms.saturating_add(ms.max(1));
    }

    /// `amend_budget_ceiling(dimension, new_cap)` — the runtime half of a
    /// ledgered `amend{budget}`: lift (or re-pin) the hard ceiling the
    /// guard fold reads. The boundary mints `control.budget.amended`
    /// first — this call only re-arms the in-memory ceiling so the next
    /// `guard_ctx` reports `new_cap − consumed` (the parked escalation's
    /// wake cue then re-drives `propose`; ADR-0168 D6). A `new_cap` still
    /// below consumed keeps `remaining ≤ 0` — the next guard crossing
    /// escalates again (attended) or stops (unattended).
    pub fn amend_budget_ceiling(&mut self, dimension: &str, new_cap: i64) {
        self.config
            .budget_ceiling
            .insert(dimension.to_string(), new_cap);
    }

    /// `checkpoint_ref` — the content address of the state's canonical
    /// checkpoint (`H("control.checkpoint" ∥ bytes)` — every
    /// `control.decision` carries it; resume-by-leaf restores it).
    pub fn checkpoint_ref(&self) -> String {
        let bytes = self.strategy.checkpoint(&self.state);
        format!(
            "sha256:{}",
            hh_wire::sha256::sha256_hex(
                [b"control.checkpoint".as_slice(), bytes.as_slice()]
                    .concat()
                    .as_slice()
            )
        )
    }

    /// The current `ControlState` (read-only — the caller never mutates).
    pub fn state(&self) -> &ControlState {
        &self.state
    }

    /// The checkpoint bytes (for `resume`/`suspend`).
    pub fn checkpoint(&self) -> Vec<u8> {
        self.strategy.checkpoint(&self.state)
    }

    /// **Resume-by-leaf** — `restore` the leaf checkpoint against `ctx`,
    /// then `observe` the durable tail past `state.last_cue_seq` (the fold
    /// is idempotent — the tail replays only what the leaf missed).
    /// `resumed` is the post-restore cue (ADR-0130).
    pub fn resume(
        &mut self,
        ctx: &ControlContext,
        checkpoint: &[u8],
        sink: &mut dyn LedgerSink,
    ) -> Result<(), DriverError> {
        self.state = self
            .strategy
            .restore(checkpoint, ctx)
            .map_err(DriverError::Restore)?;
        let tail: Vec<EventEnvelope> = sink
            .prefix()
            .iter()
            .filter(|e| e.seq > self.state.last_cue_seq)
            .cloned()
            .collect();
        self.strategy.observe(&mut self.state, &tail);
        // G-RESUME — re-arm the envelope over the durable prefix and
        // resume a pre-crash drain if the barrier is engaged.
        let (env, st) = Envelope::arm(self.envelope.policy.clone(), sink.prefix())
            .map_err(|e| DriverError::Arm(e.to_string()))?;
        self.envelope = env;
        self.envelope_state = st;
        // G-RESUME — the resume guard restates the barrier state: a
        // pre-crash drain (engaged barrier, no `lifecycle.run.finished`)
        // resumes as a `stop` with the recorded reason.
        let verdict = self.envelope.guard(
            sink.prefix(),
            GuardPoint::Resume,
            &GuardInput::Resume,
            &self.guard_ctx(sink.prefix()),
        );
        if let GuardVerdict::Stop { reason, events } = verdict {
            for ge in events {
                self.append(sink, &ge.class, ge.payload, ge.scope_id.as_deref())?;
            }
            self.stop_pending = Some(reason);
        }
        self.inbox.push_back(Cue::Resumed {
            last_durable: sink.prefix().last().map(|e| e.seq).unwrap_or(0),
            recovery_decision: Json::obj([("restored", Json::Bool(true))]),
        });
        Ok(())
    }

    /// `run` — drive the inbox until the run finishes (`stop` admitted +
    /// drain assessed) or the inbox empties (a `wait`/`escalate` parks the
    /// loop — the caller feeds the next cue).
    pub fn run(
        &mut self,
        model: &mut dyn ModelPort,
        gate: &mut dyn EffectGate,
        assembler: &mut dyn AssemblerPort,
        sink: &mut dyn LedgerSink,
    ) -> Result<RunResult, DriverError> {
        // §5c.4 `resume_set` consumption (DF-S2.8-1; AC-R-2.4.3-11) — a
        // continued activation reads its carried heads durable-before-
        // visible, once per arm, ahead of the first `decide`: the
        // `context.retrieval.completed`/`context.memory.read` rows are the
        // consumption record. An armed set with no memory boundary is
        // `UnbackedPort` — a declared resume set never silently skips.
        if !self.resume_set_drained {
            self.resume_set_drained = true;
            if !self.config.resume_set_heads.is_empty() {
                let port = self
                    .memory_port
                    .as_mut()
                    .ok_or(DriverError::UnbackedPort { kind: "resume_set" })?;
                let watermark = (
                    self.run_id.clone(),
                    sink.prefix().last().map(|e| e.seq).unwrap_or(0),
                );
                let heads = self.config.resume_set_heads.clone();
                for (class, payload) in
                    port.resume_set_read(&heads, watermark)
                        .map_err(|detail| DriverError::Port {
                            port: "memory",
                            detail,
                        })?
                {
                    self.append(sink, &class, payload, None)?;
                }
            }
        }
        loop {
            let cue = match self.inbox.pop_front() {
                Some(c) => c,
                None => {
                    // The inbox is empty — park (the caller submits the
                    // next cue; `run` returns a suspended result rather
                    // than spinning — offline determinism: no timers fire
                    // inside the loop).
                    return Err(DriverError::Port {
                        port: "inbox",
                        detail: "exhausted: run parked awaiting a cue".into(),
                    });
                }
            };
            self.tick(1);
            // KP-8 — runtime death after the settled cue is visible,
            // before the `control.decision` it would drive.
            if let Some(kp) = self.kill_point {
                if kp == hh_ledger::fault::KillPoint::Kp8
                    && matches!(cue, Cue::EffectsSettled { .. })
                {
                    self.kill_point = None;
                    return Err(DriverError::FaultInjected {
                        at: kp.as_str().to_string(),
                    });
                }
            }
            // DF-S2.8-1(e) — the `human` artefact detector: a principal's
            // `artefact_mark` mints `context.artefact.activated{detector:
            // human}` durable-before-visible, gated on the delivery it
            // names (a mark against an undelivered id mints nothing — the
            // cue still reaches `decide` as ordinary input). The row is
            // evidence, never a deterministic gate input.
            if let Cue::HumanInput(HumanInput::ArtefactMark {
                artefact_id,
                delivery_id,
                signal,
            }) = &cue
            {
                self.emit_human_artefact_activated(sink, artefact_id, delivery_id, signal)?;
            }
            let decision = self.strategy.decide(&mut self.state, &cue);
            // §5e.4 — `compute_policy.bind(d, ctx) → d′ | Unchanged`
            // between `decide` and `envelope.check`; `static` short-
            // circuits before the fold (a static run is byte-identical to
            // one without the slot — AC-F4-1).
            let (decision, compute_record) = self.bind_compute(sink, decision)?;
            // The F2 seam — `envelope.check(d′) → admitted | refused`.
            let verdict = self.envelope.check(
                sink.prefix(),
                &decision,
                &self.guard_ctx(sink.prefix()),
                self.submission.is_some(),
            );
            match verdict {
                CheckVerdict::Admitted { .. } => {
                    let ev_id = self.alloc("d");
                    // The bind record lands before the `control.decision`
                    // it advises — `decision_id` is stamped with the
                    // decision's own allocated id, then `record_id` is
                    // re-identified over the completed body (the content
                    // hash covers `decision_id`).
                    let mut compute_ref: Option<String> = None;
                    if let Some(mut record) = compute_record {
                        record.decision_id = ev_id.clone();
                        record.record_id = record.compute_record_id();
                        compute_ref = Some(record.record_id.clone());
                        let payload = record.to_json();
                        self.append(sink, "control.compute.decided", payload, None)?;
                        self.compute_records.push(record.record_id);
                    }
                    let mut payload = crate::events::decision_payload(
                        &ev_id,
                        &decision,
                        &self.state.cursor,
                        &self.checkpoint_ref(),
                        Decider::Strategy,
                        &[],
                        &Json::str("admitted"),
                        if let DecisionKind::Stop {
                            proposed_reason, ..
                        } = &decision.kind
                        {
                            Some(proposed_reason)
                        } else {
                            None
                        },
                    );
                    if let Some(r) = &compute_ref {
                        if let Json::Obj(m) = &mut payload {
                            m.insert("compute_decision_ref".to_string(), Json::str(r.clone()));
                        }
                    }
                    self.append_decision(sink, &ev_id, payload)?;
                    // T-LCD-13 `followed` — an admitted non-`wait` decision
                    // discharges a pending nudge: the strategy's response
                    // *is* the following observation (`evidence_ref` names
                    // the decision row; a `wait` parks — the cue was not
                    // consumed).
                    if !matches!(decision.kind, DecisionKind::Wait { .. }) {
                        if let Some(pending) = self.pending_nudge.take() {
                            self.emit_artefact_followed(sink, &pending, &ev_id)?;
                        }
                    }
                    if let DecisionKind::Stop {
                        proposed_reason, ..
                    } = &decision.kind
                    {
                        if let FinishOutcome::Done(r) =
                            self.finish(sink, gate, proposed_reason.clone())?
                        {
                            return Ok(r);
                        }
                        continue;
                    }
                    self.execute(sink, model, gate, assembler, decision)?;
                    if let Some(r) = self.stop_pending.take() {
                        self.append_envelope_stop(sink, &r)?;
                        if let FinishOutcome::Done(res) = self.finish(sink, gate, r)? {
                            return Ok(res);
                        }
                        continue;
                    }
                }
                CheckVerdict::Refused { reason, events } => {
                    // The guard's evidence rows land first (the spec order
                    // is `control.invariant.violated` → the stop decision
                    // → quarantine).
                    for ge in events {
                        self.append(sink, &ge.class, ge.payload, ge.scope_id.as_deref())?;
                    }
                    // The refusal returns to β as `envelope_signal{refused}`
                    // (the F2 seam's feedback cue — the strategy owns the
                    // next decision).
                    let is_kernel_stop = reason.starts_with("stop{");
                    if is_kernel_stop {
                        // Strip exactly the wrapper's `stop{` + `}` — the
                        // payload is the reason's canonical JSON whose own
                        // braces must survive (`trim_end_matches` would eat
                        // them).
                        let stop_kind = reason
                            .strip_prefix("stop{")
                            .and_then(|s| s.strip_suffix('}'))
                            .unwrap_or(&reason);
                        // A kernel stop rule fired during `check` — the
                        // envelope owns the stop (decider: envelope).
                        let sr = kernel_stop_reason(stop_kind, &self.envelope.policy);
                        self.append_envelope_stop(sink, &sr)?;
                        if let FinishOutcome::Done(res) = self.finish(sink, gate, sr)? {
                            return Ok(res);
                        }
                        continue;
                    }
                    self.inbox
                        .push_back(Cue::EnvelopeSignal(EnvelopeSignal::Refused {
                            decision_ref: self
                                .last_decision_ref
                                .as_ref()
                                .map(|r| r.event_id.clone())
                                .unwrap_or_default(),
                            reason,
                        }));
                }
            }
        }
    }

    /// The envelope-owned `stop` decision row — `decider: envelope`
    /// (the envelope's own stops never flow through β; INV-3's barrier is
    /// the row itself).
    fn append_envelope_stop(
        &mut self,
        sink: &mut dyn LedgerSink,
        reason: &StopReason,
    ) -> Result<(), DriverError> {
        let ev_id = self.alloc("d");
        let stop_decision = crate::vocab::ControlDecision {
            stamp: crate::vocab::DecisionStamp {
                decision_point: DecisionPoint::Stop,
                owner: Owner::Code,
                rationale_ref: None,
            },
            kind: DecisionKind::Stop {
                proposed_reason: reason.clone(),
                submission_ref: self.submission.clone(),
            },
        };
        let payload = crate::events::decision_payload(
            &ev_id,
            &stop_decision,
            &self.state.cursor,
            &self.checkpoint_ref(),
            Decider::Envelope,
            &[],
            &Json::str("stopped"),
            Some(reason),
        );
        self.append_decision(sink, &ev_id, payload)
    }

    /// Append the `control.decision` row (audit-grade; `checkpoint_ref`
    /// rides every row).
    fn append_decision(
        &mut self,
        sink: &mut dyn LedgerSink,
        ev_id: &str,
        payload: Json,
    ) -> Result<(), DriverError> {
        let ev = crate::events::kernel_event(
            ev_id.into(),
            "control.decision",
            self.ts(),
            Scope {
                turn_id: Some("turn-1".into()),
                ..scope_empty()
            },
            self.parent_id(sink),
            vec![],
            payload,
        );
        sink.append(vec![ev]).map_err(DriverError::Append)?;
        self.decision_events.push(ev_id.to_string());
        self.last_decision_ref = Some(EventRef {
            run_id: self.ledger_run_id.clone(),
            event_id: ev_id.to_string(),
        });
        // Fold the appended row into the strategy's view (idempotent —
        // `observe` skips seq ≤ last_cue_seq; the new row's seq is the
        // tail's max).
        let tail: Vec<EventEnvelope> = sink
            .prefix()
            .iter()
            .filter(|e| e.seq > self.state.last_cue_seq)
            .cloned()
            .collect();
        self.strategy.observe(&mut self.state, &tail);
        Ok(())
    }

    /// `execute` — dispatch the admitted decision (the driver owns the
    /// effect lifecycle rows the strategy may not emit — I1:
    /// `action.effect.intended`'s `causes` name the `control.decision`).
    fn execute(
        &mut self,
        sink: &mut dyn LedgerSink,
        model: &mut dyn ModelPort,
        gate: &mut dyn EffectGate,
        assembler: &mut dyn AssemblerPort,
        decision: crate::vocab::ControlDecision,
    ) -> Result<(), DriverError> {
        match decision.kind {
            DecisionKind::Propose {
                context_request, ..
            } => {
                let mc = self.alloc("mc");
                let assembled = assembler.assemble(
                    &AssembleInputs {
                        model_call_id: &mc,
                        prefix: sink.prefix(),
                        window_cap_tokens: self.config.window_cap_tokens,
                    },
                    &context_request,
                );
                // The `context.assembled` emitter call site (DF-S1.19-1's
                // Stage-1 half) — the builder produced the payload; the
                // driver owns the `append`, scoped to the call it feeds.
                // DF-S2.8-1: the selector's pre-assemble rows
                // (`context.procedure.selected`) land in emit order —
                // before the assembled record they fed.
                for (class, payload) in assembled.pre_events {
                    self.append(sink, &class, payload, Some(&mc))?;
                }
                // The `context.assembled` emitter call site (DF-S1.19-1's
                // Stage-1 half) — the builder produced the payload; the
                // driver owns the `append`, scoped to the call it feeds.
                if let Some(p) = assembled.assembled_payload {
                    self.append(sink, "context.assembled", p, Some(&mc))?;
                }
                // DF-S2.8-1: the builder's side-band rows
                // (`context.artefact.delivered`, …) are durable in emit
                // order under the same call scope — never dropped.
                for (class, payload) in assembled.side_events {
                    self.append(sink, &class, payload, Some(&mc))?;
                }
                self.model_round(sink, model, &assembled.request, Some((mc, 1)))?;
            }
            DecisionKind::Act { intents, .. } => {
                self.act_round(sink, gate, &intents)?;
            }
            DecisionKind::Retry {
                target,
                attempt: _,
                not_before,
            } => {
                if let crate::vocab::RetryTarget::ModelCall { model_call_id } = target {
                    let mut request = assembler
                        .assemble(
                            &AssembleInputs {
                                model_call_id: &model_call_id,
                                prefix: sink.prefix(),
                                window_cap_tokens: self.config.window_cap_tokens,
                            },
                            &Json::Null,
                        )
                        .request;
                    let next_attempt = crate::retry::attempt_no(sink.prefix(), &model_call_id) + 1;
                    // `model_fallback{profile_ref}` execution (§5e.2;
                    // AC-R-2.6.2-11): the last `control.retry.scheduled`
                    // for this call that carried the declared delta is
                    // the reroute's basis — the driver emits the
                    // `model.rerouted` audit row and stamps the
                    // `profile_override` member on the request (never a
                    // silent alteration: the delta is the recorded one,
                    // `attempt_delta_allowed` having admitted it).
                    let fallback = sink
                        .prefix()
                        .iter()
                        .rev()
                        .find(|e| {
                            e.class == "control.retry.scheduled"
                                && e.payload.get("scope_id").and_then(Json::as_str)
                                    == Some(model_call_id.as_str())
                                && e.payload
                                    .get("attempt_delta")
                                    .and_then(Json::as_str)
                                    .is_some_and(|d| d.starts_with("model_fallback{"))
                        })
                        .and_then(|e| {
                            e.payload
                                .get("attempt_delta")
                                .and_then(Json::as_str)
                                .map(|d| {
                                    (
                                        d["model_fallback{".len()..d.len() - 1].to_string(),
                                        e.event_id.clone(),
                                    )
                                })
                        });
                    if let Some((profile_ref, basis)) = fallback {
                        self.append(
                            sink,
                            "model.rerouted",
                            Json::obj([
                                ("model_call_id", Json::str(&model_call_id)),
                                ("to_profile_ref", Json::str(&profile_ref)),
                                ("cause", Json::str("model_fallback")),
                                ("basis", Json::str(&basis)),
                                ("charged_to", Json::str("subject")),
                            ]),
                            Some(&model_call_id),
                        )?;
                        if let Json::Obj(m) = &mut request {
                            m.insert("profile_override".to_string(), Json::str(profile_ref));
                        }
                    }
                    let payload = crate::events::retry_scheduled_payload(
                        ScopeKind::ModelCall,
                        &model_call_id,
                        next_attempt,
                        "resubmit",
                        0,
                        not_before.unwrap_or(self.now_ms),
                        &self.envelope.policy.policy_id,
                        None,
                    );
                    self.append(sink, "control.retry.scheduled", payload, None)?;
                    self.model_round(sink, model, &request, Some((model_call_id, next_attempt)))?;
                }
            }
            DecisionKind::Wait { until } => {
                // Park — the inbox stays empty until the declared cue kind
                // arrives (the caller submits it; `run` returns when the
                // inbox drains).
                let _ = until;
            }
            DecisionKind::Escalate { .. } => {
                // The escalation row lands with §5g's machinery — at
                // Stage 1 the decision row (already appended) is the
                // audit; the loop parks on `human_input`.
            }
            DecisionKind::Compact { reason } => {
                self.compact_round(sink, &reason)?;
            }
            DecisionKind::Verify {
                validator_refs,
                subject,
            } => {
                self.verify_round(sink, &validator_refs, &subject)?;
            }
            DecisionKind::Retrieve { query } => {
                // R-2.4.3 (DF-S2.8-1) — the strategy-admitted `retrieve`
                // dispatches to the memory boundary: the port runs the
                // retrieval pipeline, the driver lands its
                // `context.retrieval.completed`/`context.memory.read` rows
                // durable-before-visible, then the `retrieval_completed`
                // cue returns the fetch to β (F1). No wired boundary ⇒
                // `UnbackedPort` — a retrieve decision never silently
                // resolves to "nothing found".
                let port = self
                    .memory_port
                    .as_mut()
                    .ok_or(DriverError::UnbackedPort { kind: "retrieve" })?;
                let mc = self.last_model_call_id.clone().unwrap_or_default();
                let watermark = (
                    self.run_id.clone(),
                    sink.prefix().last().map(|e| e.seq).unwrap_or(0),
                );
                for (class, payload) in
                    port.retrieve(&query, &mc, watermark)
                        .map_err(|detail| DriverError::Port {
                            port: "retrieve",
                            detail,
                        })?
                {
                    self.append(
                        sink,
                        &class,
                        payload,
                        Some(mc.as_str()).filter(|s| !s.is_empty()),
                    )?;
                }
                self.inbox.push_back(Cue::RetrievalCompleted);
            }
            DecisionKind::Delegate { .. } => {
                // C1+ decision kinds — no Stage-2 variant emits them; a
                // staged variant parks before this point.
            }
            DecisionKind::Stop { .. } => {
                // Handled in `run` before `execute`.
            }
        }
        Ok(())
    }

    /// One `propose`/`retry` round: `model.call.requested` → the port's
    /// call → G-INTERPRET → `action.tool.proposed` per validated call →
    /// `model.call.completed`/`failed` → the `model_completed` cue. A
    /// retry reuses the same `model_call_id` scope (INV-5 — the shared
    /// attempt identity); a fresh propose allocates a new one.
    fn model_round(
        &mut self,
        sink: &mut dyn LedgerSink,
        model: &mut dyn ModelPort,
        request: &Json,
        scope: Option<(String, u64)>,
    ) -> Result<(), DriverError> {
        let (mc, attempt) = scope.unwrap_or_else(|| (self.alloc("mc"), 1));
        self.last_model_call_id = Some(mc.clone());
        // G-PRE-CALL — budget/deadline/gauge/reservation/ladder.
        let pre = self.envelope.guard(
            sink.prefix(),
            GuardPoint::PreCall,
            &GuardInput::PreCall {
                model_call_id: mc.clone(),
                reservation_size: crate::retry::reserve_model_call_size(
                    self.config.profile_max_output_bound,
                    self.config
                        .remaining
                        .values()
                        .next()
                        .copied()
                        .unwrap_or(i64::MAX) as u64,
                ),
                gauge_caps: self.config.gauge_caps,
            },
            &self.guard_ctx(sink.prefix()),
        );
        if let Some(cue) = self.apply_guard_verdict(sink, pre, &mc)? {
            self.inbox.push_back(cue);
            return Ok(());
        }
        self.derive_deadline(ScopeKind::ModelCall, &mc);
        self.append(
            sink,
            "model.call.requested",
            Json::obj([
                ("model_call_id", Json::str(&mc)),
                ("attempt_no", Json::Int(attempt as i64)),
                ("request_ref", Json::str(format!("req-{mc}-a{attempt}"))),
            ]),
            Some(&mc),
        )?;
        let outcome = model.call(&mc, request);
        // G-INTERPRET — output validation + loop detectors + empty ladder.
        let interp = self.envelope.guard(
            sink.prefix(),
            GuardPoint::Interpret,
            &GuardInput::Interpret {
                model_call_id: mc.clone(),
                stop_reason: outcome.stop_reason,
                text_empty: outcome.text_empty,
                calls: outcome.calls.clone(),
                surfaces: self.config.surfaces.clone(),
            },
            &self.guard_ctx(sink.prefix()),
        );
        let mut proposed_tool_calls: Vec<String> = vec![];
        // A `nudge` is advisory — the call still lands (the loop window
        // accumulates it; a `deny`/`stop`/format rejection suppresses it).
        let suppressed = match &interp {
            GuardVerdict::Stop { .. } => true,
            GuardVerdict::Respond { observation, .. } => !matches!(
                observation.get("kind").and_then(Json::as_str),
                Some("loop_nudge")
            ),
            GuardVerdict::Pass { .. } => false,
        };
        if !suppressed {
            // `action.tool.proposed` per validated call — the strategy
            // folds them into `pending_intents`.
            if let crate::output::ValidationVerdict::Pass { calls } = crate::output::validate(
                &self.envelope.policy.output_validation,
                &self.config.surfaces,
                outcome.stop_reason,
                outcome.text_empty,
                &outcome.calls,
            ) {
                for c in calls {
                    // A call on the declared plan surface is a
                    // model-emitted *plan*, not an effect intent — the
                    // driver schema-validates it and ledgers
                    // `control.plan.emitted` (the closed-world check is the
                    // codec's, never the strategy's — S3.10).
                    if self
                        .config
                        .plan_surface_id
                        .as_deref()
                        .is_some_and(|p| p == c.surface_id.as_str())
                    {
                        let plan_ref = format!(
                            "sha256:{}",
                            hh_wire::sha256::sha256_hex(c.args.to_canonical_string().as_bytes())
                        );
                        let (valid, steps, reason) = match crate::plan_exec::plan_validate(
                            &c.args.to_canonical_string(),
                            &self.config.surfaces,
                        ) {
                            Ok(steps) => (true, steps, None),
                            Err(r) => (false, vec![], Some(r)),
                        };
                        self.append(
                            sink,
                            "control.plan.emitted",
                            crate::plan_exec::plan_emitted_payload(
                                &plan_ref,
                                &self.config.plan_schema_ref,
                                valid,
                                &steps,
                                reason,
                            ),
                            Some(&mc),
                        )?;
                        continue;
                    }
                    self.append(
                        sink,
                        "action.tool.proposed",
                        Json::obj([
                            ("tool_call_id", Json::str(&c.tool_call_id)),
                            ("surface_id", Json::str(&c.surface_id)),
                            ("loop_key", Json::str(&c.loop_key)),
                        ]),
                        Some(&mc),
                    )?;
                    // AC-R-2.7.1-9 — the deterministic `followed` pass: a
                    // validated call on a delivered `tool_surface` / an
                    // activated typed `procedure` emits
                    // `verification.artefact.followed{detector:
                    // deterministic}` (§02's detector table).
                    self.maybe_emit_artefact_followed(sink, &mc, &c.surface_id, &c.tool_call_id)?;
                    proposed_tool_calls.push(c.tool_call_id);
                }
            }
        }
        // The completed/failed row then the cue.
        match outcome.stop_reason {
            hh_gateway::vocab::StopReason::Error | hh_gateway::vocab::StopReason::Unknown => {
                let class = outcome
                    .error_class
                    .clone()
                    .unwrap_or_else(|| "unknown".into());
                let mut failed_members = vec![
                    ("model_call_id", Json::str(&mc)),
                    ("attempt_no", Json::Int(attempt as i64)),
                    ("error", Json::obj([("class", Json::str(&class))])),
                ];
                if let Some(ms) = outcome.retry_after_ms {
                    failed_members.push(("retry_after_ms", Json::Int(ms as i64)));
                }
                self.append(
                    sink,
                    "model.call.failed",
                    Json::obj(failed_members),
                    Some(&mc),
                )?;
                // The F2 seam — `schedule_retry` decides `retry` vs `give_up`.
                let retries_used =
                    crate::views::fold_envelope_view(sink.prefix()).retries_scheduled;
                match self.envelope.schedule_retry(
                    sink.prefix(),
                    ScopeKind::ModelCall,
                    &mc,
                    &class,
                    self.now_ms,
                    outcome.retry_after_ms,
                    None,
                    retries_used,
                    self.config.retries_ceiling,
                ) {
                    crate::retry::RetryOutcome::Retry(s) => {
                        self.append(
                            sink,
                            "control.retry.scheduled",
                            crate::events::retry_scheduled_payload(
                                ScopeKind::ModelCall,
                                &mc,
                                s.attempt_no,
                                &class,
                                s.delay_ms,
                                s.not_before,
                                &self.envelope.policy.policy_id,
                                s.attempt_delta.as_ref(),
                            ),
                            Some(&mc),
                        )?;
                        self.inbox
                            .push_back(Cue::EnvelopeSignal(EnvelopeSignal::RetryableError {
                                class,
                                attempt: s.attempt_no - 1,
                            }));
                    }
                    crate::retry::RetryOutcome::GiveUp(_)
                    | crate::retry::RetryOutcome::BudgetExhausted { .. } => {
                        self.inbox
                            .push_back(Cue::EnvelopeSignal(EnvelopeSignal::Refused {
                                decision_ref: mc.clone(),
                                reason: "retry_budget_exhausted".into(),
                            }));
                    }
                }
            }
            _ => {
                // The ADR-0135 §2 recording rule — the completed row carries
                // the parsed `calls`/`text_empty`/`retry_after_ms` so a
                // deterministic replay feeds the *recorded* interpretation
                // inputs (older runs without the member report
                // `re_executed`, never a silent deterministic claim).
                let mut completed_members = vec![
                    ("model_call_id", Json::str(&mc)),
                    ("attempt_no", Json::Int(attempt as i64)),
                    ("stop_reason", Json::str(outcome.stop_reason.as_str())),
                    ("response_ref", Json::str(&outcome.response_ref)),
                    ("text_empty", Json::Bool(outcome.text_empty)),
                    (
                        "calls",
                        Json::Arr(
                            outcome
                                .calls
                                .iter()
                                .map(|c| {
                                    Json::obj([
                                        ("tool_call_id", Json::str(&c.tool_call_id)),
                                        ("surface", Json::str(&c.surface)),
                                        ("args_raw", Json::str(&c.args_raw)),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                ];
                if let Some(ms) = outcome.retry_after_ms {
                    completed_members.push(("retry_after_ms", Json::Int(ms as i64)));
                }
                self.append(
                    sink,
                    "model.call.completed",
                    Json::obj(completed_members),
                    Some(&mc),
                )?;
                self.inbox.push_back(Cue::ModelCompleted {
                    model_call_id: mc,
                    response_ref: outcome.response_ref.clone(),
                    stop_reason: outcome.stop_reason,
                });
            }
        }
        // A guard verdict that was not pass (a nudge/deny/stop from
        // G-INTERPRET) rides the inbox as its cue form.
        if let Some(cue) = self.apply_guard_verdict(sink, interp, "")? {
            self.inbox.push_back(cue);
        }
        let _ = proposed_tool_calls;
        Ok(())
    }

    /// One `act` round (sequential): `action.effect.intended` (causes ∋
    /// the `control.decision` — I1) → G-PRE-DISPATCH → the gate's
    /// dispatch → the settled terminal → the `effects_settled` cue.
    ///
    /// The strategy's `act{intents}` name `tool_call_id`s only (I2 — never
    /// the arg bytes); the gate needs the routed surface (its
    /// `stop_rule = submit` detection reads `intent.surface_id`), so the
    /// dispatch intent is resolved from the durable prefix
    /// (`action.tool.proposed` → `surface_id`, `model.call.completed` →
    /// `args_raw`) — the ledgered intent stays minimal (S3.10).
    fn act_round(
        &mut self,
        sink: &mut dyn LedgerSink,
        gate: &mut dyn EffectGate,
        intents: &[Json],
    ) -> Result<(), DriverError> {
        let resolved = resolve_intents(sink.prefix(), intents);
        let mut settled: Vec<EffectOutcome> = vec![];
        for (intent, dispatch_intent) in intents.iter().zip(resolved.iter()) {
            let ef = self.alloc("ef");
            self.derive_deadline(ScopeKind::ToolAttempt, &ef);
            // `action.effect.intended` — `causes ∋ control.decision` (I1).
            let intended = crate::events::kernel_event(
                self.alloc("e"),
                "action.effect.intended",
                self.ts(),
                Scope {
                    turn_id: Some("turn-1".into()),
                    effect_id: Some(ef.clone()),
                    ..scope_empty()
                },
                self.parent_id(sink),
                self.last_decision_ref.clone().into_iter().collect(),
                Json::obj([
                    ("effect_id", Json::str(&ef)),
                    ("intent", intent.clone()),
                    (
                        "effect_class",
                        intent
                            .get("effect_class")
                            .cloned()
                            .unwrap_or_else(|| Json::str("reversible")),
                    ),
                    // `effective_risk_class` — the audit-grade dossier
                    // member (§5g.6 §3). The declared class resolves off
                    // the armed surface table (`SurfaceSpec.risk_class`);
                    // an undeclared surface stamps `UNKNOWN` — the most
                    // dangerous point (ADR-0031 §2), never a guessed-safe
                    // class.
                    (
                        "effective_risk_class",
                        dispatch_intent
                            .get("surface_id")
                            .or_else(|| dispatch_intent.get("surface"))
                            .and_then(Json::as_str)
                            .and_then(|sid| {
                                self.config.surfaces.iter().find(|s| s.surface_id == sid)
                            })
                            .and_then(|s| s.risk_class.as_ref())
                            .map(|rc| rc.to_json())
                            .unwrap_or_else(|| hh_ontology::risk::RiskClass::UNKNOWN.to_json()),
                    ),
                ]),
            );
            sink.append(vec![intended]).map_err(DriverError::Append)?;
            // G-PRE-DISPATCH — retry eligibility, deadline, INV-3.
            let pre = self.envelope.guard(
                sink.prefix(),
                GuardPoint::PreDispatch,
                &GuardInput::PreDispatch {
                    effect_id: ef.clone(),
                    attempt_no: 1,
                    effect_class: dispatch_intent
                        .get("effect_class")
                        .and_then(Json::as_str)
                        .unwrap_or("reversible")
                        .to_string(),
                    read_only: dispatch_intent
                        .get("read_only")
                        .map(|b| matches!(b, Json::Bool(true)))
                        .unwrap_or(false),
                    last_terminal_unknown: false,
                    probed_or_idempotent: false,
                },
                &self.guard_ctx(sink.prefix()),
            );
            if matches!(pre, GuardVerdict::Pass { .. }) {
                let out = gate.dispatch(&ef, 1, dispatch_intent);
                // The gate's dispatch-lifecycle rows land durable before
                // the terminal — `decided`/`authorized`/`prepared`/
                // `committed` in emit order (§5a.2; the sink stamps the
                // fencing token it owns on the post-`prepared` classes).
                // Producer: the gate's component — these are the
                // boundary's authorization records, not the envelope's
                // (INV-7).
                let gate_component = gate.producer_component().to_string();
                for (class, payload) in &out.emitted {
                    self.append_producer(sink, class, payload.clone(), Some(&ef), &gate_component)?;
                }
                if matches!(out.outcome, SettledOutcome::Pending) {
                    // Out-of-loop dispatch (a host capability): the
                    // write-ahead `committed` row is the record; the
                    // terminal arrives via `report_host_effect`. The
                    // effect stays open — `open_effects` parks every cue
                    // but `effects_settled` until it settles (I4).
                    settled.push(EffectOutcome {
                        effect_id: ef.clone(),
                        outcome: SettledOutcome::Pending,
                    });
                    continue;
                }
                let terminal_class = match &out.outcome {
                    SettledOutcome::Observed { .. } => "action.effect.observed",
                    SettledOutcome::Refused => "action.effect.refused",
                    SettledOutcome::Unknown { .. } => "action.effect.unknown",
                    SettledOutcome::Abandoned => "action.effect.abandoned",
                    SettledOutcome::Pending => unreachable!("pending handled above"),
                };
                // The `stop_rule = submit` detection is ledgered where it
                // happened (CC3 — the terminal row carries the ref so a
                // deterministic replay re-serves the *recorded* value,
                // never a re-minted one — ADR-0135 §2). `attempt_no` is a
                // declared dossier member; `refused` carries a `reason`
                // (the gate's error class when it named one).
                let mut terminal_payload = crate::events::settled_outcome_json(&out.outcome);
                if let Json::Obj(m) = &mut terminal_payload {
                    m.insert("attempt_no".to_string(), Json::Int(1));
                    if matches!(out.outcome, SettledOutcome::Refused) {
                        m.insert(
                            "reason".to_string(),
                            Json::str(
                                out.error_class
                                    .clone()
                                    .unwrap_or_else(|| "gate_refused".to_string()),
                            ),
                        );
                    }
                }
                if let Some(s) = &out.submission_ref {
                    if let Json::Obj(m) = &mut terminal_payload {
                        m.insert("submission_ref".to_string(), Json::str(s.clone()));
                    }
                }
                self.append(sink, terminal_class, terminal_payload, Some(&ef))?;
                // AC-R-2.4.3-12 (DF-S2.8-1) — the `trigger{path_touched}`
                // leg: an *observed* effect whose intent names `args.path`
                // runs a trigger retrieval over the memory boundary
                // (procedure_pointer `triggers[].path_glob` matches are
                // the §5c.3 hit kind). The port's rows land durable,
                // scoped to the model call that proposed the effect; the
                // next `assemble` delivers the hit indexes as candidates.
                if terminal_class == "action.effect.observed" {
                    if let Some(path) = dispatch_intent
                        .get("args")
                        .and_then(|a| a.get("path"))
                        .and_then(Json::as_str)
                    {
                        if let Some(port) = self.memory_port.as_mut() {
                            let path = path.to_string();
                            let mc = self.last_model_call_id.clone().unwrap_or_default();
                            let watermark = (
                                self.run_id.clone(),
                                sink.prefix().last().map(|e| e.seq).unwrap_or(0),
                            );
                            for (class, payload) in port
                                .trigger_retrieve(&path, &mc, watermark)
                                .map_err(|detail| DriverError::Port {
                                    port: "retrieve",
                                    detail,
                                })?
                            {
                                self.append(
                                    sink,
                                    &class,
                                    payload,
                                    Some(mc.as_str()).filter(|s| !s.is_empty()),
                                )?;
                            }
                        }
                    }
                }
                if let Some(s) = &out.submission_ref {
                    self.submission = Some(s.clone());
                }
                settled.push(EffectOutcome {
                    effect_id: ef,
                    outcome: out.outcome,
                });
            } else if let Some(cue) = self.apply_guard_verdict(sink, pre, &ef)? {
                self.inbox.push_back(cue);
                return Ok(());
            }
        }
        let all_terminal = settled
            .iter()
            .all(|o| !matches!(o.outcome, SettledOutcome::Pending));
        self.inbox.push_back(Cue::EffectsSettled {
            settled,
            all_terminal,
            submission_ref: self.submission.clone(),
        });
        Ok(())
    }

    /// Apply a non-`pass` guard verdict — append its rows and map it to the
    /// cue the inbox receives (`respond` → `guard_fired`-adjacent signal;
    /// `stop` → the finish path).
    fn apply_guard_verdict(
        &mut self,
        sink: &mut dyn LedgerSink,
        verdict: GuardVerdict,
        scope_id: &str,
    ) -> Result<Option<Cue>, DriverError> {
        match verdict {
            GuardVerdict::Pass { .. } => Ok(None),
            GuardVerdict::Respond {
                observation,
                events,
            } => {
                for ge in events {
                    self.append(
                        sink,
                        &ge.class,
                        ge.payload,
                        ge.scope_id
                            .as_deref()
                            .or(Some(scope_id).filter(|s| !s.is_empty())),
                    )?;
                }
                let guard_id = observation
                    .get("kind")
                    .and_then(Json::as_str)
                    .unwrap_or("guard")
                    .to_string();
                // The `guard_fired` cue's ledgered spelling (CF-229) — the
                // audit row carries `verdict: respond` plus the
                // observation's own members (`required_tokens`, `cap`,
                // `detector`, `blocking_effects`) so a pure fold recovers
                // the guard's numbers.
                let mut fired =
                    match crate::events::guard_fired_payload(DecisionPoint::Act, &guard_id) {
                        Json::Obj(m) => m,
                        other => unreachable!("guard_fired_payload is an object: {other:?}"),
                    };
                fired.insert("verdict".into(), Json::str("respond"));
                if let Json::Obj(obs) = &observation {
                    for (k, v) in obs {
                        fired.entry(k.clone()).or_insert_with(|| v.clone());
                    }
                }
                let prov = ProvenanceRecord::minted(
                    Origin::kernel("hh-control/guard"),
                    PersistenceScope::Run,
                    self.now_ms,
                );
                self.append_prov(
                    sink,
                    "control.guard.fired",
                    Json::Obj(fired),
                    Some(scope_id).filter(|s| !s.is_empty()),
                    prov,
                )?;
                // T-LCD-13 — a nudge is a conditioned `HarnessRule`
                // artefact (I9; AC-R-2.6.1-8): mint the rule with its
                // complete `AssumptionDebtRecord`, ledger `delivered` +
                // `activated`, and arm the `followed` discharge for the
                // strategy's next admitted decision.
                if let Some(nudge_kind) = nudge_kind(&guard_id) {
                    self.pending_nudge =
                        Some(self.emit_nudge_artefact(sink, nudge_kind, &guard_id)?);
                }
                Ok(Some(Cue::GuardFired {
                    decision_point: DecisionPoint::Act,
                    guard_id,
                }))
            }
            GuardVerdict::Stop { reason, events } => {
                for ge in events {
                    self.append(sink, &ge.class, ge.payload, ge.scope_id.as_deref())?;
                }
                // The envelope owns the stop — the `stop` decision row +
                // drain run on `run`'s next iteration (decider: envelope).
                self.stop_pending = Some(reason);
                Ok(None)
            }
        }
    }

    /// `compact{reason}` — the compaction boundary: the port runs the
    /// I-FALLBACK ladder (its own `context.compaction.started/completed`
    /// rows ride its sink); `CompactionImpossible` or an absent port is the
    /// exhausted ladder → envelope-owned `stop{context_exhausted}`
    /// (CF-225; `stop_pending` — `run` mints the decision row).
    fn compact_round(
        &mut self,
        sink: &mut dyn LedgerSink,
        reason: &str,
    ) -> Result<(), DriverError> {
        match &mut self.compaction_port {
            Some(port) => match port.compact(reason) {
                Ok(done) => {
                    // The port's `context.compaction.started`/`completed`
                    // rows land durable in emit order, then the cue —
                    // durable-before-visible, same convention as the
                    // assembler's side bands (DF-S2.8-1: the rows are
                    // minted from the loop, not a test-side stub).
                    for (class, payload) in done.emitted {
                        self.append(sink, &class, payload, None)?;
                    }
                    self.inbox.push_back(Cue::CompactionCompleted {
                        view_hash: done.view_hash,
                    });
                }
                Err(impossible) => {
                    self.stop_pending = Some(StopReason::ContextExhausted {
                        required_tokens: impossible.required_tokens,
                        cap: impossible.cap,
                    });
                }
            },
            None => {
                // No compaction boundary is wired — the occupancy cap is
                // the honest `stop{context_exhausted}` (the portable
                // definition's exhausted ladder; `react/minimal` declares
                // no `compact`).
                let (required_tokens, cap) = self.compaction_pressure(sink);
                self.stop_pending = Some(StopReason::ContextExhausted {
                    required_tokens,
                    cap,
                });
            }
        }
        Ok(())
    }

    /// The `(required_tokens, cap)` the last `compaction_required`
    /// `control.guard.fired` row recorded (0/0 when the cap fired without a
    /// ledgered observation — the driver never invents numbers).
    fn compaction_pressure(&self, sink: &dyn LedgerSink) -> (u64, u64) {
        let ev = sink.prefix().iter().rev().find(|e| {
            e.class == "control.guard.fired"
                && e.payload.get("guard_id").and_then(Json::as_str) == Some("compaction_required")
        });
        match ev {
            Some(e) => (
                e.payload
                    .get("required_tokens")
                    .and_then(Json::as_int)
                    .map(|v| v.max(0) as u64)
                    .unwrap_or(0),
                e.payload
                    .get("cap")
                    .and_then(Json::as_int)
                    .map(|v| v.max(0) as u64)
                    .unwrap_or(0),
            ),
            None => (0, 0),
        }
    }

    /// `verify{validator_refs, subject}` — the decision-point verification
    /// seam (R-2.7.1): the port runs the declared validators, the driver
    /// ledgers `validator.invoked` + `validator.verdict` per produced
    /// verdict (provenance-bearing — the verdict's own detector authority)
    /// then cues `verification_completed`.
    fn verify_round(
        &mut self,
        sink: &mut dyn LedgerSink,
        validator_refs: &[String],
        subject: &Json,
    ) -> Result<(), DriverError> {
        let port = self
            .verify_port
            .as_mut()
            .ok_or(DriverError::UnbackedPort { kind: "verify" })?;
        let verdicts = port.verify(validator_refs, subject);
        for v in &verdicts {
            let prov = ProvenanceRecord::minted(
                Origin::kernel("hh-control/verify"),
                PersistenceScope::Run,
                self.now_ms,
            );
            self.append_prov(
                sink,
                "verification.validator.invoked",
                hh_verification::events::validator_invoked(
                    v,
                    &hh_verification::vocab::Isolation::Kernel,
                ),
                None,
                prov.clone(),
            )?;
            self.append_prov(
                sink,
                "verification.validator.verdict",
                hh_verification::events::validator_verdict(v),
                None,
                prov,
            )?;
        }
        // DF-S2.8-1 (e) — the `judged`/`human` artefact-detector legs ride
        // the verify seam: `subject{kind: artefact_activation |
        // artefact_followed}` declares the artefact check, and a `decided`
        // verdict mints the chain row under the *verdict's own* detector
        // class (a judge's `judged`; a rater verdict riding the port is
        // `human`). The row is gated on the durable `delivered` it names —
        // an undelivered delivery mints nothing — and a non-affirmative /
        // inconclusive verdict mints nothing (a judged absence is an
        // absence, never a deterministic `false`). `confidence_ppm` carries
        // the verdict's own grade capped at the parsed/judged ceiling —
        // never `1_000_000` (judged evidence keeps its grade).
        match subject.get("kind").and_then(Json::as_str) {
            Some("artefact_activation") => {
                let artefact_id = subject
                    .get("artefact_id")
                    .and_then(Json::as_str)
                    .unwrap_or_default();
                let delivery_id = subject
                    .get("delivery_id")
                    .and_then(Json::as_str)
                    .unwrap_or_default();
                let signal = subject
                    .get("signal")
                    .and_then(Json::as_str)
                    .unwrap_or("judged");
                if self.delivered_artefact(sink.prefix(), artefact_id, delivery_id) {
                    for v in &verdicts {
                        if !matches!(v.status, hh_verification::vocab::VerdictStatus::Decided)
                            || !v.value.is_affirmative()
                        {
                            continue;
                        }
                        let mut payload = match hh_context::events::artefact_activated_payload(
                            artefact_id,
                            delivery_id,
                            v.detector.as_str(),
                            signal,
                        ) {
                            Json::Obj(m) => m,
                            other => {
                                unreachable!("artefact_activated_payload is an object: {other:?}")
                            }
                        };
                        payload.insert(
                            "detector_ref".to_string(),
                            Json::str(
                                v.validator_ref
                                    .semantic_id
                                    .clone()
                                    .unwrap_or_else(|| v.validator_ref.version_id.clone()),
                            ),
                        );
                        payload.insert(
                            "confidence_ppm".to_string(),
                            Json::Int(judged_confidence_ppm(v) as i64),
                        );
                        payload.insert("evidence_ref".to_string(), Json::str(v.verdict_id.clone()));
                        self.append(sink, "context.artefact.activated", Json::Obj(payload), None)?;
                    }
                }
            }
            Some("artefact_followed") => {
                let artefact_id = subject
                    .get("artefact_id")
                    .and_then(Json::as_str)
                    .unwrap_or_default();
                let delivery_id = subject
                    .get("delivery_id")
                    .and_then(Json::as_str)
                    .unwrap_or_default();
                let kind = subject
                    .get("artefact_kind")
                    .and_then(Json::as_str)
                    .unwrap_or("procedure");
                if self.delivered_artefact(sink.prefix(), artefact_id, delivery_id) {
                    for v in &verdicts {
                        if !matches!(v.status, hh_verification::vocab::VerdictStatus::Decided) {
                            continue;
                        }
                        self.append(
                            sink,
                            "verification.artefact.followed",
                            hh_verification::followed::followed_payload_det(
                                artefact_id,
                                delivery_id,
                                v.detector.as_str(),
                                &v.validator_ref
                                    .semantic_id
                                    .clone()
                                    .unwrap_or_else(|| v.validator_ref.version_id.clone()),
                                v.value.is_affirmative(),
                                judged_confidence_ppm(v),
                                &v.verdict_id,
                                kind,
                            ),
                            None,
                        )?;
                    }
                }
            }
            _ => {}
        }
        self.inbox.push_back(Cue::VerificationCompleted);
        Ok(())
    }

    /// Whether `delivery_id`/`artefact_id` name a durable
    /// `context.artefact.delivered` row — the shared gate the human/judged
    /// detector legs check before minting (a detector never fabricates its
    /// precondition).
    fn delivered_artefact(
        &self,
        prefix: &[EventEnvelope],
        artefact_id: &str,
        delivery_id: &str,
    ) -> bool {
        prefix.iter().any(|e| {
            e.class == "context.artefact.delivered"
                && e.payload.get("delivery_id").and_then(Json::as_str) == Some(delivery_id)
                && e.payload.get("artefact_id").and_then(Json::as_str) == Some(artefact_id)
        })
    }

    /// `context.artefact.activated{detector: human}` — the principal's
    /// `artefact_mark` leg (DF-S2.8-1 e). Gated on the durable `delivered`
    /// row the mark names; the row's provenance is principal-origin (the
    /// human's assertion is the evidence) and it never enters the
    /// deterministic `followed` fold — `maybe_emit_artefact_followed`
    /// reads `detector: deterministic` activations only.
    fn emit_human_artefact_activated(
        &mut self,
        sink: &mut dyn LedgerSink,
        artefact_id: &str,
        delivery_id: &str,
        signal: &str,
    ) -> Result<(), DriverError> {
        if !self.delivered_artefact(sink.prefix(), artefact_id, delivery_id) {
            return Ok(());
        }
        let prov = ProvenanceRecord::minted(
            Origin::human("principal", HumanRole::Principal),
            PersistenceScope::Run,
            self.now_ms,
        );
        self.append_prov(
            sink,
            "context.artefact.activated",
            hh_context::events::artefact_activated_payload(
                artefact_id,
                delivery_id,
                "human",
                signal,
            ),
            None,
            prov,
        )
    }

    /// Mint the nudge `HarnessRule` (conditioned on the run's profile — I9
    /// requires the complete `AssumptionDebtRecord`), ledger
    /// `context.artefact.delivered` + `context.artefact.activated`, and
    /// return the pending-following record (AC-R-2.6.1-8; T-LCD-13).
    fn emit_nudge_artefact(
        &mut self,
        sink: &mut dyn LedgerSink,
        kind: &str,
        trigger_kind: &str,
    ) -> Result<NudgePending, DriverError> {
        let delivery_id = self.alloc("nd");
        let rule_id = format!("hh/nudge/{kind}/{delivery_id}");
        let prov = ProvenanceRecord::minted(
            Origin::kernel("hh-control/nudge"),
            PersistenceScope::Run,
            self.now_ms,
        );
        // The nudge text is a `Text` leaf the rule's `insert_context_item`
        // action names — content-addressed (R-TEXT), never free bytes.
        let text = hh_hir::leaves::Text::new(nudge_text(kind), "hh-control/nudge", prov.clone());
        let text_ref =
            hh_hir::refs::Ref::pinned(format!("hh/nudge-text/{kind}"), text.content_hash.clone());
        let debt = hh_hir::records::AssumptionDebtRecord {
            rule_id: rule_id.clone(),
            hypothesis: hh_hir::leaves::Text::new(
                nudge_hypothesis(kind),
                "hh-control/nudge",
                prov.clone(),
            ),
            evidence_refs: vec![hh_hir::EvidenceRef::legacy(format!(
                "control.guard.fired{{{trigger_kind}}}"
            ))],
            owner: hh_hir::OwnerRef::principal("kernel"),
            expiry_condition: hh_hir::ExpiryCondition {
                kind: hh_hir::ExpiryKind::EvidenceRefreshDue,
                value: None,
            },
            removal_test_ref: format!("{rule_id}/removal_test"),
            status: hh_hir::DebtStatus::Active,
            debt_class: Some(hh_hir::DebtClass::ModelConditioned),
            hypothesis_typed: None,
            scope: Some(hh_hir::DebtScope {
                model_selectors: vec![],
                task_classes: vec![],
                roles: vec![],
            }),
            expiry: None,
            runway_ms: None,
            revalidation: None,
            removal_test: None,
            created_by: Some(prov.clone()),
            created_at: Some(self.now_ms),
            supersedes: None,
        };
        let trigger = match trigger_kind {
            "loop_nudge" => Json::obj([("kind", Json::str("loop_detected"))]),
            "format_error" => Json::obj([("kind", Json::str("response_without_action"))]),
            "missing_submission" => Json::obj([("kind", Json::str("stop_without_submission"))]),
            other => Json::obj([("kind", Json::str(other))]),
        };
        let rule = Json::obj([
            ("rule_id", Json::str(rule_id.clone())),
            ("trigger", trigger),
            (
                "action",
                Json::obj([("insert_context_item", text_ref.to_json())]),
            ),
            (
                "scope",
                Json::obj([("run", Json::str(self.run_id.clone()))]),
            ),
            (
                "conditioned_on",
                Json::obj([
                    ("profile", Json::str(self.model_ref.clone())),
                    ("pinned", Json::Bool(false)),
                ]),
            ),
            ("assumption_debt", hh_hir::debt_json(&debt, false)),
        ]);
        // `delivered` — the rule artefact enters the next context assembly
        // (by_reference: the assembled context expands the handle).
        let mut delivered = match hh_context::events::artefact_delivered_payload(
            &rule_id,
            &delivery_id,
            "harness_rule",
            Some(&format!("sha256:{}", text.content_hash)),
            true,
        ) {
            Json::Obj(m) => m,
            other => unreachable!("artefact_delivered_payload is an object: {other:?}"),
        };
        delivered.insert("rule".into(), rule);
        delivered.insert("nudge_kind".into(), Json::str(kind));
        self.append_prov(
            sink,
            "context.artefact.delivered",
            Json::Obj(delivered),
            None,
            prov.clone(),
        )?;
        // `activated` — the cue reaching `decide` is the activation signal
        // (deterministic detector; §5c.1).
        self.append_prov(
            sink,
            "context.artefact.activated",
            hh_context::events::artefact_activated_payload(
                &rule_id,
                &delivery_id,
                "deterministic",
                trigger_kind,
            ),
            None,
            prov,
        )?;
        Ok(NudgePending {
            rule_id,
            delivery_id,
            kind: kind.to_string(),
        })
    }

    /// The deterministic `followed` detector pass (AC-R-2.7.1-9; §02
    /// "Detectors per kind"). A validated `action.tool.proposed` is the
    /// args-conform evidence for a delivered `tool_surface`; a delivered +
    /// activated typed `procedure` is followed when the invoked surface is
    /// in the procedure's `allowed_capabilities` (carried on the
    /// `activated` row) and the index `delivery_id` rode the call's
    /// context — `context.assembled{model_call_id}.items[]` is the call's
    /// cause set (hh-context `detect_followed`'s rule, adapted to the
    /// driver plane: the assembled context *is* what caused the call).
    /// Prose kinds carry no deterministic detector — the run emits no row
    /// and the eval plane renders `n/a{no_detector}` (never a proxy).
    fn maybe_emit_artefact_followed(
        &mut self,
        sink: &mut dyn LedgerSink,
        model_call_id: &str,
        surface_id: &str,
        tool_call_id: &str,
    ) -> Result<(), DriverError> {
        // Delivered artefacts + activated procedures + this call's cause
        // set, folded from the durable prefix (ledger-only — CC3).
        struct Del {
            artefact_id: String,
            delivery_id: String,
            kind: String,
        }
        let mut delivered: Vec<Del> = Vec::new();
        let mut activated: Vec<(String, Option<std::collections::BTreeSet<String>>)> = Vec::new();
        let mut causes: Vec<String> = Vec::new();
        for e in sink.prefix() {
            match e.class.as_str() {
                "context.artefact.delivered" => delivered.push(Del {
                    artefact_id: e
                        .payload
                        .get("artefact_id")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    delivery_id: e
                        .payload
                        .get("delivery_id")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    kind: e
                        .payload
                        .get("kind")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_string(),
                }),
                "context.artefact.activated"
                    // Only the deterministic leg feeds the deterministic
                    // `followed` fold — a `judged`/`human` activation is
                    // its own evidence class (CF-483: never pooled).
                    if e.payload.get("detector").and_then(Json::as_str)
                        == Some("deterministic") =>
                {
                    let delivery_id = e
                        .payload
                        .get("delivery_id")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let caps = e.payload.get("allowed_capabilities").map(|c| match c {
                        Json::Arr(a) => a
                            .iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect(),
                        _ => std::collections::BTreeSet::new(),
                    });
                    activated.push((delivery_id, caps));
                }
                "context.assembled" => {
                    // The assembled row feeding this call is the most
                    // recent `context.assembled` in the prefix (the driver
                    // appends it immediately before `model_round`) —
                    // `payload.model_call_id` names it when the builder
                    // stamps the member; position is the fallback.
                    let matches = e
                        .payload
                        .get("model_call_id")
                        .and_then(Json::as_str)
                        .map(|id| id == model_call_id)
                        .unwrap_or(true);
                    if matches {
                        causes.clear();
                        if let Some(Json::Arr(items)) = e.payload.get("items") {
                            for it in items {
                                if let Some(d) = it.get("delivery_id").and_then(Json::as_str) {
                                    causes.push(d.to_string());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        let emit = |this: &mut Self,
                    sink: &mut dyn LedgerSink,
                    artefact_id: &str,
                    delivery_id: &str,
                    detector_ref: &str,
                    evidence_ref: &str,
                    kind: &str|
         -> Result<(), DriverError> {
            let prov = ProvenanceRecord::minted(
                Origin::kernel("hh-control/followed"),
                PersistenceScope::Run,
                this.now_ms,
            );
            this.append_prov(
                sink,
                "verification.artefact.followed",
                hh_verification::followed::followed_payload(
                    artefact_id,
                    delivery_id,
                    detector_ref,
                    true,
                    evidence_ref,
                    kind,
                ),
                None,
                prov,
            )
        };
        // The deterministic `activated` leg (§5c.1; AC-R-2.4.5-10):
        // a delivered `procedure_index`/`procedure` whose `delivery_id`
        // rode this call's assembled context activates —
        // `detector: deterministic, signal: cited`. The store's declared
        // `allowed_capabilities` ride the row when the memory boundary
        // resolves the artefact (capless ⇒ no `followed` can mint).
        for d in &delivered {
            if !matches!(d.kind.as_str(), "procedure" | "procedure_index") {
                continue;
            }
            if !causes.iter().any(|c| c == &d.delivery_id) {
                continue;
            }
            if activated.iter().any(|(id, _)| id == &d.delivery_id) {
                continue;
            }
            let caps = self
                .memory_port
                .as_ref()
                .and_then(|p| p.procedure_capabilities(&d.artefact_id))
                .map(|v| {
                    v.into_iter()
                        .collect::<std::collections::BTreeSet<String>>()
                });
            let mut payload = match hh_context::events::artefact_activated_payload(
                &d.artefact_id,
                &d.delivery_id,
                "deterministic",
                "cited",
            ) {
                Json::Obj(m) => m,
                other => unreachable!("artefact_activated_payload is an object: {other:?}"),
            };
            if let Some(c) = &caps {
                payload.insert(
                    "allowed_capabilities".to_string(),
                    Json::Arr(c.iter().map(|s| Json::str(s.clone())).collect()),
                );
            }
            self.append(sink, "context.artefact.activated", Json::Obj(payload), None)?;
            activated.push((d.delivery_id.clone(), caps));
        }
        // tool_surface — the validated call IS the args-conform evidence.
        for d in &delivered {
            if d.kind == "tool_surface"
                && d.artefact_id == surface_id
                && !delivered_is_followed(sink.prefix(), &d.delivery_id)
            {
                emit(
                    self,
                    sink,
                    &d.artefact_id,
                    &d.delivery_id,
                    hh_verification::followed::detector::ARGS_CONFORM,
                    tool_call_id,
                    "tool_surface",
                )?;
            }
        }
        // Typed procedure — `detect_followed`: capability allowed ∧ index
        // delivery in the call's causes.
        for d in &delivered {
            if !matches!(d.kind.as_str(), "procedure" | "procedure_index") {
                continue;
            }
            let caps = activated
                .iter()
                .find(|(delivery_id, _)| *delivery_id == d.delivery_id)
                .and_then(|(_, caps)| caps.clone());
            let Some(caps) = caps else { continue };
            if let Some(ev) =
                hh_context::procedure::detect_followed(&d.delivery_id, surface_id, &caps, &causes)
            {
                if delivered_is_followed(sink.prefix(), &d.delivery_id) {
                    continue;
                }
                emit(
                    self,
                    sink,
                    &d.artefact_id,
                    &d.delivery_id,
                    hh_verification::followed::detector::PROCEDURE_INVOKED,
                    &ev,
                    "procedure",
                )?;
            }
        }
        Ok(())
    }

    /// `verification.artefact.followed` — the pending nudge's `followed`
    /// verdict: the strategy's admitted decision row is the evidence
    /// (deterministic detector, `verdict: true` — the loop continued under
    /// the rule; §5e.1's `kind ∈ {loop_nudge, continue_nudge}` member).
    fn emit_artefact_followed(
        &mut self,
        sink: &mut dyn LedgerSink,
        pending: &NudgePending,
        decision_ev_id: &str,
    ) -> Result<(), DriverError> {
        let prov = ProvenanceRecord::minted(
            Origin::kernel("hh-control/nudge"),
            PersistenceScope::Run,
            self.now_ms,
        );
        self.append_prov(
            sink,
            "verification.artefact.followed",
            Json::obj([
                ("detector", Json::str("deterministic")),
                ("detector_ref", Json::str(pending.rule_id.clone())),
                ("verdict", Json::Bool(true)),
                ("confidence_ppm", Json::Int(1_000_000)),
                ("evidence_ref", Json::str(decision_ev_id)),
                ("artefact_id", Json::str(pending.rule_id.clone())),
                ("delivery_id", Json::str(pending.delivery_id.clone())),
                ("kind", Json::str(pending.kind.clone())),
            ]),
            None,
            prov,
        )
    }

    /// `finish` — the stop protocol: `control.decision{stop}` was already
    /// appended by the caller; the drain assessment decides the terminal
    /// reason, then `lifecycle.turn.finished` + `lifecycle.run.finished`.
    /// On `stop{completed}` the completion gate runs between the drain and
    /// `terminate` (§5f.2; S3.10): claims → `claim.reconciled` →
    /// `gate.evaluated` → `completion.decided`; a `hold` queues the
    /// `completion_refused` cue and returns [`FinishOutcome::Held`] — the
    /// strategy is re-invoked, never silently passed.
    fn finish(
        &mut self,
        sink: &mut dyn LedgerSink,
        gate: &mut dyn EffectGate,
        reason: StopReason,
    ) -> Result<FinishOutcome, DriverError> {
        self.tick(1);
        // S1.21 — the claim-ledger obligation on `stop{completed}`: emit
        // `verification.completion.proposed` + `verification.claim.recorded`
        // before the drain assessment (AC-R-2.7.2a-1: ≥1 claim, kind ∈
        // {achieved, unachievable}, criteria_status aligned, delegate
        // provenance). A missing/malformed finish surface still records the
        // completion claim itself — nothing silently lost (CC3).
        let is_completion = matches!(reason, StopReason::Completed);
        if is_completion {
            self.emit_completion_claims(sink, gate)?;
        }
        let drain = crate::stop::assess_drain(
            sink.prefix(),
            &reason,
            self.now_ms.saturating_add(30_000),
            self.now_ms,
            &[],
            &[],
        );
        // Drain timeout ⇒ `unknown{cause: drain_timeout}` per open effect.
        for ef in &drain.unknown_recorded {
            self.append(
                sink,
                "action.effect.unknown",
                Json::obj([("cause", Json::str("drain_timeout"))]),
                Some(ef),
            )?;
        }
        // R-2.2.1 (`unknown_escalated`; ADR-0333 D6) — every effect in
        // `unknown` phase at `finished` gets a `lifecycle.escalation.
        // raised` naming it (drain-timeout + `worker_lost` stragglers).
        // The fold reads the post-drain prefix; already-named effects
        // are skipped so a `Held`-completion re-entry never duplicates.
        {
            let mut effects: std::collections::BTreeMap<String, EffectFold> =
                std::collections::BTreeMap::new();
            let mut covered: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            let mut unknown_cause: std::collections::BTreeMap<String, String> =
                std::collections::BTreeMap::new();
            for e in sink.prefix() {
                fold_event(&mut effects, e);
                if e.class == "lifecycle.escalation.raised" {
                    if let Some(id) = e.payload.get("effect_id").and_then(Json::as_str) {
                        covered.insert(id.to_string());
                    }
                }
                if e.class == "action.effect.unknown" {
                    if let (Some(id), Some(c)) = (
                        e.scope.effect_id.as_deref(),
                        e.payload.get("cause").and_then(Json::as_str),
                    ) {
                        unknown_cause.insert(id.to_string(), c.to_string());
                    }
                }
            }
            let open_unknown: Vec<String> = effects
                .values()
                .filter(|f| f.phase == EffectPhase::Unknown && !covered.contains(&f.effect_id))
                .map(|f| f.effect_id.clone())
                .collect();
            for ef in open_unknown {
                let cause = unknown_cause
                    .get(&ef)
                    .cloned()
                    .unwrap_or_else(|| "open_at_finished".to_string());
                self.append_prov(
                    sink,
                    "lifecycle.escalation.raised",
                    Json::obj([
                        ("subject", Json::str(&ef)),
                        ("effect_id", Json::str(&ef)),
                        ("kind", Json::str("effect_unknown")),
                        ("reason", Json::str(&cause)),
                    ]),
                    None,
                    ProvenanceRecord::kernel("hh-control/driver", self.now_ms),
                )?;
            }
        }
        // S3.10 — the completion gate (§5f.2's `proposed → reconciled →
        // gate.evaluated → decided → finished` chain). The gate reads the
        // post-drain prefix so `drain_timeout`'s `unknown` rows are the
        // D6 facts.
        let mut run_status = "finished".to_string();
        let mut summary_ref: Option<String> = None;
        let mut final_reason = drain.final_reason.clone();
        if is_completion {
            match self.completion_gate(sink)? {
                CompletionFlow::Held => {
                    // The refused completion returns to β as a
                    // `guard_fired{completion_refused}` cue — the strategy
                    // owns the next decision (verify/act/stop again); the
                    // hold is durable so re-entry loops consume the
                    // `reconciliation.holds` budget (F4).
                    self.inbox.push_back(Cue::GuardFired {
                        decision_point: DecisionPoint::Stop,
                        guard_id: "completion_refused".into(),
                    });
                    return Ok(FinishOutcome::Held);
                }
                CompletionFlow::Decided(d) => {
                    run_status = d.decision.status.clone();
                    summary_ref = Some(d.decision.gate_ref.clone());
                    if let Some(r) = d.stop_reason {
                        final_reason = r;
                    }
                }
            }
        }
        let report = self.strategy.terminate(&self.state, &final_reason);
        let drain_ref = format!(
            "sha256:{}",
            hh_wire::sha256::sha256_hex(drain.to_json().to_canonical_string().as_bytes())
        );
        // §5c.4 scope floors (DF-S2.8-1): the turn is ending —
        // `turn`-scoped memories expire; the run is ending — `run`-scoped
        // the same. The memory boundary's `context.memory.invalidated`
        // rows land *inside* the closing turn scope (before
        // `lifecycle.turn.finished` seals it — a `turn_id` stamp on a
        // closed turn trips the ledger's scope gate; `Turn` then `Run` —
        // inner scope first; session/goal/user scopes survive the
        // activation by contract). `at_seq` is the durable tip — the
        // fold's `until` point.
        if self.memory_port.is_some() {
            let at_seq = sink.prefix().last().map(|e| e.seq).unwrap_or(0);
            let mut rows = Vec::new();
            for scope in [PersistenceScope::Turn, PersistenceScope::Run] {
                rows.extend(
                    self.memory_port
                        .as_mut()
                        .map(|port| port.mark_scope_ended(scope, at_seq))
                        .unwrap_or_default(),
                );
            }
            for (class, payload) in rows {
                self.append(sink, &class, payload, None)?;
            }
        }
        // AC-F2-03 — an `invariant_violation` stop quarantines: the
        // `security.audit.checkpoint{kind: quarantine}` row lands durable
        // before `run.finished`, naming the violated invariant + the
        // detection evidence (the run is `infrastructure_failure`, never
        // scored — ADR-0108 D3). It must precede `lifecycle.turn.finished`
        // — the row carries the `turn-1` stamp and the ledger refuses a
        // `turn_id` on a closed turn scope (`ScopeNotOpen`).
        if let StopReason::InvariantViolation { invariant_id } = &final_reason {
            let evidence: Vec<String> = sink
                .prefix()
                .iter()
                .filter(|e| e.class == "control.invariant.violated")
                .map(|e| e.event_id.clone())
                .collect();
            self.append(
                sink,
                "security.audit.checkpoint",
                crate::events::quarantine_payload(invariant_id, &evidence),
                None,
            )?;
        }
        self.append(
            sink,
            "lifecycle.turn.finished",
            crate::events::turn_finished_payload(&final_reason),
            None,
        )?;
        let mut finished = crate::events::run_finished_payload(
            &run_status,
            &final_reason,
            &report.unresolved_effects,
            &drain_ref,
        );
        if let Some(r) = &summary_ref {
            if let Json::Obj(m) = &mut finished {
                m.insert("verification_summary_ref".to_string(), Json::str(r.clone()));
            }
        }
        // §5e.4 — the run's `control.compute.decided` record refs ride the
        // finished row (`compute_decision_outcome` joins them with the
        // realized outcome). Empty for `static` — the member stays absent
        // so a static run's ledger is byte-identical (AC-F4-1).
        if !self.compute_records.is_empty() {
            if let Json::Obj(m) = &mut finished {
                m.insert(
                    "compute_decisions".to_string(),
                    Json::Arr(self.compute_records.iter().map(Json::str).collect()),
                );
            }
        }
        self.append(sink, "lifecycle.run.finished", finished, None)?;
        Ok(FinishOutcome::Done(RunResult {
            report,
            drain,
            decision_events: self.decision_events.clone(),
        }))
    }

    /// The completion gate call site (§5f.2 §3; DF-S1.21-1's emitter half —
    /// S3.10): bind + reconcile the completion claims, evaluate the gate
    /// over the durable prefix, ledger the rows. `Hold` within the
    /// `reconciliation.holds` cap emits the `completion_refused`
    /// `control.guard.fired` audit row and returns [`CompletionFlow::Held`];
    /// exhaustion (the cap consumed and the run still divergent) decides
    /// `budget_exhausted{reconciliation.holds}` without a further
    /// `gate.evaluated` row (the ledger fact `hold_count == cap` plus the
    /// decision row are the audit — AC-R-2.7.2a-5).
    fn completion_gate(
        &mut self,
        sink: &mut dyn LedgerSink,
    ) -> Result<CompletionFlow, DriverError> {
        use hh_verification::bind::{bind_claim, fold_effect_states, fold_verdicts};
        use hh_verification::claims::reconcile_ledger_only;
        use hh_verification::claims::CriterionState;
        use hh_verification::gate::{
            decision_stratum, evaluate_gate, GateCriterion, GateFacts, OpenEffect,
        };
        use hh_verification::vocab::{Agreement, ClaimKind, CompletionPolicy, GateVerdict};

        // Phase 1 — the pure fold over the durable prefix (the immutable
        // borrow ends before the first append; every fact is owned).
        let kernel_prov = ProvenanceRecord::kernel("hh-control/reconcile", self.now_ms);
        let claims = std::mem::take(&mut self.completion_claims);
        let (records, completion_claim_ref, facts): (
            Vec<hh_verification::claims::ReconciliationRecord>,
            String,
            GateFacts,
        ) = {
            // The row projection — authority from the row's provenance (CC2:
            // conferred, never read from content); a row without provenance
            // binds nothing (Unverified).
            let rows: Vec<hh_verification::bind::RowView> =
                sink.prefix()
                    .iter()
                    .map(|e| hh_verification::bind::RowView {
                        seq: e.seq,
                        class: e.class.as_str(),
                        payload: &e.payload,
                        // CC2 — authority is conferred by the producing
                        // component: the driver's own kernel rows carry
                        // `producer.component_class = "kernel"` (their authority
                        // is kernel by construction — a provenance record only
                        // narrows it); a non-kernel row without provenance stays
                        // `Unverified` and binds nothing in the C2 context.
                        authority: e.provenance.as_ref().map(|p| p.authority).unwrap_or_else(
                            || {
                                if e.producer.component_class == hh_ledger::event::KERNEL_COMPONENT
                                {
                                    hh_provenance::authority::AuthorityClass::Kernel
                                } else {
                                    hh_provenance::authority::AuthorityClass::Unverified
                                }
                            },
                        ),
                        scope_effect_id: e.scope.effect_id.as_deref(),
                    })
                    .collect();
            let head_seq = sink.prefix().last().map(|e| e.seq).unwrap_or(0);

            // Reconcile every recorded claim (kernel provenance — the
            // `claim.reconciled` rows land in phase 2). A bound C2
            // `execution_alignment` reconciler runs `reconcile_c2` over the
            // projected `ReconcileContext` (the D1/D4/D7–D10 detectors on
            // top of the C0 classes — R-2.7.2b); `None`/`enabled = false`
            // keeps the byte-identical C0 `ledger_only` fold.
            let recon_ctx = self
                .config
                .reconciler
                .as_ref()
                .map(|_| hh_verification::bind::fold_reconcile_context(&rows));
            let mut records = Vec::new();
            let mut completion_idx: Option<usize> = None;
            let mut completion_claim_ref = String::new();
            let mut completion_kind = ClaimKind::Achieved;
            let mut completion_evidence: Vec<String> = vec![];
            for claim in &claims {
                let handles = bind_claim(&rows, claim, self.config.task_contract.as_ref());
                let recs: Vec<hh_verification::claims::ReconciliationRecord> =
                    match (&self.config.reconciler, &recon_ctx) {
                        (Some(decl), Some(ctx)) => hh_verification::reconciler::reconcile_c2(
                            claim,
                            &handles,
                            ctx,
                            decl,
                            head_seq,
                            kernel_prov.clone(),
                        ),
                        _ => vec![reconcile_ledger_only(
                            claim,
                            &handles,
                            "hir/kernel/reconcile:1",
                            head_seq,
                            kernel_prov.clone(),
                        )],
                    };
                if completion_idx.is_none()
                    && matches!(claim.kind, ClaimKind::Achieved | ClaimKind::Unachievable)
                {
                    completion_kind = claim.kind;
                    completion_evidence = claim.evidence_refs.clone();
                    completion_claim_ref = claim.claim_id.clone();
                    completion_idx = Some(records.len());
                }
                records.extend(recs);
            }

            // The gate facts (deterministic-only by construction — the
            // binder never binds a `judged`/`delegate` row, F7).
            let effects = fold_effect_states(&rows);
            let verdicts = fold_verdicts(&rows);
            let open_effects: Vec<OpenEffect> = effects
                .values()
                .filter(|e| !e.terminal)
                .map(|e| OpenEffect {
                    effect_id: e.effect_id.clone(),
                    state: e.outcome.clone(),
                    detachable: false,
                })
                .collect();
            let abandoned: Vec<String> = effects
                .values()
                .filter(|e| e.terminal && e.outcome == "abandoned")
                .map(|e| e.effect_id.clone())
                .collect();
            let mut required: Vec<GateCriterion> = vec![];
            if let Some(contract) = &self.config.task_contract {
                for c in contract.required_criteria() {
                    let state = verdicts
                        .iter()
                        .filter(|v| v.criterion_ref.as_deref() == Some(c.criterion_id.as_str()))
                        .max_by_key(|v| v.seq)
                        .map(|v| {
                            if v.status == "decided" {
                                if v.affirmative {
                                    CriterionState::Met
                                } else {
                                    CriterionState::Unmet
                                }
                            } else {
                                CriterionState::Unverifiable
                            }
                        })
                        .unwrap_or(CriterionState::Unrun);
                    required.push(GateCriterion {
                        criterion_ref: c.criterion_id.clone(),
                        state,
                        bound_validators: vec![c.validator_ref.clone()],
                        unverifiable_reason: matches!(
                            contract.completion_policy,
                            CompletionPolicy::Unverifiable(_)
                        ),
                    });
                }
            }
            let holds_consumed = sink
                .prefix()
                .iter()
                .filter(|e| {
                    e.class == "verification.gate.evaluated"
                        && e.payload.get("verdict").and_then(Json::as_str) == Some("hold")
                })
                .count() as u64;
            let mut evidence_divergences = vec![];
            let mut completion_agreement = Agreement::Unverifiable;
            // The completion claim's records — the C2 fold may emit several
            // per claim (the D7+D10 pair); the gate reads every
            // hold-admissible divergence (deterministic only — F7; judged
            // records never reach `claim_evidence_divergences`).
            let completion_records: Vec<&hh_verification::claims::ReconciliationRecord> =
                completion_idx
                    .map(|_| {
                        records
                            .iter()
                            .filter(|r| r.claim_id == completion_claim_ref)
                            .collect()
                    })
                    .unwrap_or_default();
            for rec in &completion_records {
                if let Agreement::Diverge(class) = rec.agreement {
                    if rec.hold_admissible() {
                        completion_agreement = Agreement::Diverge(class);
                        // `contract_gap` is already F3's own verdict — the
                        // gate's `required_criteria` table decides it with
                        // `unverifiable_reason` awareness; re-adding the
                        // reconciler's class would hold a criterion the
                        // contract declared unverifiable (S3.10).
                        if class != hh_verification::vocab::DivergenceClass::ContractGap {
                            evidence_divergences.push(class);
                        }
                    }
                }
            }
            // The gate-visible agreement is deterministic-only: a judged
            // `diverge` annotates the record stream but never holds/vetoes
            // the completion claim on its own (AC-R-2.7.2b-3 — the fallback
            // therefore skips non-hold-admissible records entirely).
            if let Some(first) = completion_records.iter().find(|r| r.hold_admissible()) {
                if !matches!(completion_agreement, Agreement::Diverge(_)) {
                    completion_agreement = first.agreement;
                }
            }
            (
                records,
                completion_claim_ref,
                GateFacts {
                    completion_claim_kind: completion_kind,
                    completion_claim_agreement: completion_agreement,
                    claim_evidence_refs: completion_evidence,
                    open_effects,
                    abandoned_effects: abandoned,
                    required_criteria: required,
                    claim_evidence_divergences: evidence_divergences,
                    holds_consumed,
                    holds_cap: self.config.holds_cap,
                },
            )
        };
        // Phase 2 — the durable rows.
        for rec in &records {
            self.append_prov(
                sink,
                "verification.claim.reconciled",
                hh_verification::events::claim_reconciled(rec),
                None,
                kernel_prov.clone(),
            )?;
        }
        // F6 — the `kernel_notice` feed-back rows (AC-R-2.7.2b-2): one
        // notice per `(class, subject)` per diverging record, delivered as
        // `context.artefact.delivered{kind = kernel_notice}` at `kernel`
        // authority — a deterministic projection, never judged output
        // rewritten as kernel evidence (the notice names the refusal it
        // cites). Bound-reconciler runs only; the C0 fold stays
        // byte-identical.
        if self.config.reconciler.is_some() {
            let mut delivered: std::collections::BTreeSet<(
                hh_verification::vocab::DivergenceClass,
                String,
            )> = Default::default();
            for notice in hh_verification::reconciler::feed_back_notices(
                &records,
                claims.iter(),
                &mut delivered,
            ) {
                self.append_prov(
                    sink,
                    "context.artefact.delivered",
                    hh_verification::events::kernel_notice(&notice),
                    None,
                    kernel_prov.clone(),
                )?;
            }
        }
        let result = evaluate_gate(&facts);

        match &result.verdict {
            GateVerdict::Hold { .. } if result.holds_exhausted => {
                // F4 — the cap is consumed and the run still diverges:
                // `budget_exhausted{reconciliation.holds}` (stratum
                // `unreconciled_claims`). No fourth `gate.evaluated{hold}`
                // row — the consumed-cap ledger fact plus the decision row
                // carry the audit (AC-R-2.7.2a-5).
                let decided = self.emit_completion_decided(
                    sink,
                    "budget_exhausted",
                    Some(hh_verification::vocab::STRATUM_UNRECONCILED_CLAIMS),
                    &facts,
                    "gate:exhausted",
                )?;
                Ok(CompletionFlow::Decided(
                    decided.with_stop(StopReason::BudgetExhausted {
                        // The contract's budget ref first, else the run's
                        // bound budget (the cursor's `bound_ref` — the
                        // `reconciliation.holds` dimension charges against
                        // the run's own budget identity, never a literal).
                        budget_id: self
                            .config
                            .task_contract
                            .as_ref()
                            .map(|c| c.budget_ref.clone())
                            .unwrap_or_else(|| self.state.cursor.bound_ref.clone()),
                        dimension: hh_ontology::dimensions::DimensionId::ReconciliationHolds,
                    }),
                ))
            }
            GateVerdict::Hold {
                divergences,
                required_actions,
            } => {
                // Ledger `gate.evaluated{hold}` then the refused-completion
                // audit row — the strategy reads `required_actions` through
                // `observe` (the cue carries only the guard id).
                self.append(
                    sink,
                    "verification.gate.evaluated",
                    hh_verification::events::gate_evaluated(
                        &result,
                        &completion_claim_ref,
                        self.config
                            .task_contract
                            .as_ref()
                            .map(|c| c.budget_ref.as_str())
                            .unwrap_or("budget"),
                    ),
                    None,
                )?;
                let prov = ProvenanceRecord::kernel("hh-control/gate", self.now_ms);
                self.append_prov(
                    sink,
                    "control.guard.fired",
                    Json::obj([
                        ("decision_point", Json::str("stop")),
                        ("guard_id", Json::str("completion_refused")),
                        ("verdict", Json::str("hold")),
                        (
                            "divergences",
                            Json::Arr(divergences.iter().map(|d| Json::str(d.as_str())).collect()),
                        ),
                        (
                            "required_actions",
                            Json::Arr(required_actions.iter().map(Json::str).collect()),
                        ),
                    ]),
                    None,
                    prov,
                )?;
                Ok(CompletionFlow::Held)
            }
            GateVerdict::Pass | GateVerdict::Veto { .. } => {
                let gate_ev = self.alloc("e");
                self.append_with_id(
                    sink,
                    &gate_ev,
                    "verification.gate.evaluated",
                    hh_verification::events::gate_evaluated(
                        &result,
                        &completion_claim_ref,
                        self.config
                            .task_contract
                            .as_ref()
                            .map(|c| c.budget_ref.as_str())
                            .unwrap_or("budget"),
                    ),
                    None,
                )?;
                // The status mapping (the C0 rule — §5f.2 §3): honest
                // failure ⇒ `failed_honest`; `veto`/`abandoned` ⇒
                // `succeeded_with_veto`; the `unverifiable` policy ⇒
                // `succeeded_unverified` (never `succeeded`).
                let unverifiable = self.config.task_contract.as_ref().is_some_and(|c| {
                    matches!(c.completion_policy, CompletionPolicy::Unverifiable(_))
                });
                let (status, stratum) = if result.honest_failure {
                    (
                        "failed_honest",
                        decision_stratum(&result).map(str::to_string),
                    )
                } else if result.success_with_veto.is_some()
                    || matches!(result.verdict, GateVerdict::Veto { .. })
                {
                    ("succeeded_with_veto", None)
                } else if unverifiable {
                    ("succeeded_unverified", None)
                } else {
                    ("succeeded", None)
                };
                let decided = self.emit_completion_decided(
                    sink,
                    status,
                    stratum.as_deref(),
                    &facts,
                    &gate_ev,
                )?;
                Ok(CompletionFlow::Decided(decided))
            }
        }
    }

    /// `verification.completion.decided` — the decision row with the
    /// `VerificationSummary` the gate's facts measured.
    fn emit_completion_decided(
        &mut self,
        sink: &mut dyn LedgerSink,
        status: &str,
        stratum: Option<&str>,
        facts: &hh_verification::gate::GateFacts,
        gate_ref: &str,
    ) -> Result<CompletionGateDecision, DriverError> {
        use hh_verification::claims::CriterionState;
        use hh_verification::gate::{summarize, TaskContract};

        let empty_contract;
        let contract = match &self.config.task_contract {
            Some(c) => c,
            None => {
                empty_contract = TaskContract {
                    contract_id: "contract:none".into(),
                    goal_ref: String::new(),
                    criteria: vec![],
                    invariants: vec![],
                    completion_policy: hh_verification::vocab::CompletionPolicy::AllRequired,
                    evidence_kinds_required: vec![],
                    budget_ref: "budget".into(),
                    sealed: true,
                    task_value: None,
                };
                &empty_contract
            }
        };
        let states: std::collections::BTreeMap<String, CriterionState> = facts
            .required_criteria
            .iter()
            .map(|c| (c.criterion_ref.clone(), c.state))
            .collect();
        let summary = summarize(
            contract,
            &states,
            vec![],
            sink.prefix().last().map(|e| e.seq).unwrap_or(0),
        );
        let decision = hh_verification::gate::CompletionDecision {
            run_id: self.run_id.clone(),
            status: status.to_string(),
            stratum: stratum.map(str::to_string),
            gate_ref: gate_ref.to_string(),
            summary,
        };
        let ev = self.alloc("e");
        self.append_with_id(
            sink,
            &ev,
            "verification.completion.decided",
            hh_verification::events::completion_decided(&decision),
            None,
        )?;
        Ok(CompletionGateDecision {
            decision,
            stop_reason: None,
        })
    }

    /// The claim-ledger obligation on `stop{completed}` (S1.21 — ADR-0112
    /// D1/D2/D6; AC-R-2.7.2a-1): `verification.completion.proposed` then
    /// `verification.claim.recorded` per extracted claim. The model's
    /// structured `finish` record comes from the gate (`stop_rule = submit`
    /// detection owns it — the driver never reads the response bytes);
    /// `extract` runs the structured channel at confidence 1.0. A missing
    /// surface, a `NoClaimSurface`/`MalformedClaim` refusal, or a gate that
    /// pre-dates the record all still emit the completion claim itself (the
    /// `stop{completed}` decision *is* the claim) — the extract error rides
    /// the claim's `asserted` so nothing is silently lost (CC3).
    fn emit_completion_claims(
        &mut self,
        sink: &mut dyn LedgerSink,
        gate: &mut dyn EffectGate,
    ) -> Result<(), DriverError> {
        use hh_verification::claims::{self, Claim};
        use hh_verification::vocab::{ClaimKind, ExtractedBy, SubjectRef};

        let mc = self
            .last_model_call_id
            .clone()
            .unwrap_or_else(|| "mc-0".into());
        let prov = ProvenanceRecord::minted(
            Origin::model(self.model_ref.clone(), self.run_id.clone(), mc.clone()),
            PersistenceScope::Run,
            self.now_ms,
        );
        let at_seq = sink.prefix().last().map(|e| e.seq).unwrap_or(0);
        // `completion.proposed` — the model proposed completion at the stop
        // decision point (`by` names the model call).
        self.append_prov(
            sink,
            "verification.completion.proposed",
            hh_verification::events::completion_proposed(&mc, "stop"),
            None,
            prov.clone(),
        )?;
        // The claim surface — the gate's submitted `finish` record wrapped in
        // the response view `extract` reads (`{"finish": …}`).
        let response_view = match gate.finish_record() {
            Some(fields) => Json::obj([("finish", fields)]),
            None => Json::Null,
        };
        let claims = match claims::extract(
            &mc,
            &response_view,
            "finish",
            &self.run_id,
            at_seq,
            prov.clone(),
        ) {
            Ok(cs) => cs,
            Err(e) => {
                // The surface was absent or malformed — the completion claim
                // itself is still recorded (the admitted `stop{completed}`
                // is the claim; the refusal detail rides `asserted`).
                let c = Claim {
                    claim_id: format!("{mc}:claim:1"),
                    run_id: self.run_id.clone(),
                    model_call_id: mc.clone(),
                    at_seq,
                    kind: ClaimKind::Achieved,
                    subject: SubjectRef::Run,
                    predicate: "is_done".into(),
                    asserted: Json::obj([("extract_error", Json::str(e.to_string()))]),
                    evidence_refs: vec![],
                    extracted_by: ExtractedBy::Structured("finish".into()),
                    extraction_confidence_ppm: 1_000_000,
                    criteria_status: vec![],
                    provenance: prov.clone(),
                };
                // The delegate provenance is minted above — `validate`
                // cannot fail here, but honour the contract anyway.
                let _ = c.validate();
                vec![c]
            }
        };
        for claim in &claims {
            self.append_prov(
                sink,
                "verification.claim.recorded",
                hh_verification::events::claim_recorded(claim),
                Some(&mc),
                claim.provenance.clone(),
            )?;
        }
        self.completion_claims = claims;
        Ok(())
    }

    /// Append one kernel row carrying explicit provenance (the verification
    /// family is provenance-mandatory — the claim rows carry the claim's
    /// own `delegate` origin, never a kernel fact — ADR-0112 D6).
    fn append_prov(
        &mut self,
        sink: &mut dyn LedgerSink,
        class: &str,
        payload: Json,
        scope_id: Option<&str>,
        provenance: ProvenanceRecord,
    ) -> Result<(), DriverError> {
        let mut ev = crate::events::kernel_event(
            self.alloc("e"),
            class,
            self.ts(),
            Scope {
                turn_id: Some("turn-1".into()),
                model_call_id: scope_id.map(String::from),
                ..scope_empty()
            },
            self.parent_id(sink),
            vec![],
            payload,
        );
        ev.provenance = Some(provenance);
        sink.append(vec![ev]).map_err(DriverError::Append)?;
        let tail: Vec<EventEnvelope> = sink
            .prefix()
            .iter()
            .filter(|e| e.seq > self.state.last_cue_seq)
            .cloned()
            .collect();
        self.strategy.observe(&mut self.state, &tail);
        Ok(())
    }

    /// `append` with a caller-allocated event id (the gate needs the
    /// `gate.evaluated` ref on the `completion.decided` row).
    fn append_with_id(
        &mut self,
        sink: &mut dyn LedgerSink,
        ev_id: &str,
        class: &str,
        payload: Json,
        scope_id: Option<&str>,
    ) -> Result<(), DriverError> {
        let ev = crate::events::kernel_event(
            ev_id.into(),
            class,
            self.ts(),
            Scope {
                turn_id: if class.starts_with("lifecycle.run.") {
                    None
                } else {
                    Some("turn-1".into())
                },
                effect_id: scope_id
                    .filter(|_| class.starts_with("action.effect."))
                    .map(String::from),
                model_call_id: scope_id
                    .filter(|_| class.starts_with("model.") || class == "action.tool.proposed")
                    .map(String::from),
                tool_call_id: if class == "action.tool.proposed" {
                    payload
                        .get("tool_call_id")
                        .and_then(Json::as_str)
                        .map(String::from)
                } else {
                    None
                },
                ..scope_empty()
            },
            self.parent_id(sink),
            vec![],
            payload,
        );
        sink.append(vec![ev]).map_err(DriverError::Append)?;
        let tail: Vec<EventEnvelope> = sink
            .prefix()
            .iter()
            .filter(|e| e.seq > self.state.last_cue_seq)
            .cloned()
            .collect();
        self.strategy.observe(&mut self.state, &tail);
        Ok(())
    }

    /// `append` with an explicit producer component — the effect gate's
    /// `emitted` rows are the *boundary's* records (the §5a.2 chain's
    /// `decided`/`authorized`/`prepared`/`committed`), not the envelope's;
    /// minting them under `hh-control` reads as the envelope granting
    /// itself a permission (INV-7).
    fn append_producer(
        &mut self,
        sink: &mut dyn LedgerSink,
        class: &str,
        payload: Json,
        scope_id: Option<&str>,
        component: &str,
    ) -> Result<(), DriverError> {
        let mut ev = crate::events::kernel_event(
            self.alloc("e"),
            class,
            self.ts(),
            Scope {
                turn_id: if class.starts_with("lifecycle.run.") {
                    None
                } else {
                    Some("turn-1".into())
                },
                effect_id: scope_id
                    .filter(|_| class.starts_with("action.effect."))
                    .map(String::from),
                model_call_id: scope_id
                    .filter(|_| class.starts_with("model.") || class == "action.tool.proposed")
                    .map(String::from),
                tool_call_id: if class == "action.tool.proposed" {
                    payload
                        .get("tool_call_id")
                        .and_then(Json::as_str)
                        .map(String::from)
                } else {
                    None
                },
                ..scope_empty()
            },
            self.parent_id(sink),
            vec![],
            payload,
        );
        ev.producer.component_variant_ref = component.to_string();
        sink.append(vec![ev]).map_err(DriverError::Append)?;
        let tail: Vec<EventEnvelope> = sink
            .prefix()
            .iter()
            .filter(|e| e.seq > self.state.last_cue_seq)
            .cloned()
            .collect();
        self.strategy.observe(&mut self.state, &tail);
        Ok(())
    }

    /// Append one kernel row through the sink (scope id → `Scope.effect_id`
    /// — the driver maps it; the class registry validates).
    fn append(
        &mut self,
        sink: &mut dyn LedgerSink,
        class: &str,
        payload: Json,
        scope_id: Option<&str>,
    ) -> Result<(), DriverError> {
        let ev = crate::events::kernel_event(
            self.alloc("e"),
            class,
            self.ts(),
            Scope {
                // `lifecycle.run.*` rows are run-scope — they live beside
                // the turn chain, not inside it (`run.finished` lands after
                // `turn.finished` closed the turn scope, so a `turn_id`
                // stamp would trip the ledger's `ScopeNotOpen` gate).
                turn_id: if class.starts_with("lifecycle.run.") {
                    None
                } else {
                    Some("turn-1".into())
                },
                effect_id: scope_id
                    .filter(|_| class.starts_with("action.effect."))
                    .map(String::from),
                model_call_id: scope_id
                    .filter(|_| class.starts_with("model.") || class == "action.tool.proposed")
                    .map(String::from),
                tool_call_id: if class == "action.tool.proposed" {
                    payload
                        .get("tool_call_id")
                        .and_then(Json::as_str)
                        .map(String::from)
                } else {
                    None
                },
                ..scope_empty()
            },
            self.parent_id(sink),
            vec![],
            payload,
        );
        sink.append(vec![ev]).map_err(DriverError::Append)?;
        // Fold the tail into the strategy state (the `pending_intents`/
        // `open_effects` fold — idempotent on `last_cue_seq`).
        let tail: Vec<EventEnvelope> = sink
            .prefix()
            .iter()
            .filter(|e| e.seq > self.state.last_cue_seq)
            .cloned()
            .collect();
        self.strategy.observe(&mut self.state, &tail);
        Ok(())
    }

    /// Derive a scope deadline from the `TimeoutPolicy` (`started_at = now`
    ///   `default_ms`, never past `hard_max_ms` — the policy's own
    /// `deadline()` arithmetic).
    fn derive_deadline(&mut self, kind: ScopeKind, scope_id: &str) {
        let d =
            crate::retry::deadline(&self.envelope.policy.timeouts, kind, self.now_ms, &[], None);
        self.deadlines.insert(scope_id.to_string(), d.at);
    }

    fn ts(&self) -> String {
        format!("1970-01-01T00:00:{:02}.000Z", (self.now_ms / 1000) % 60)
    }

    fn parent_id(&self, sink: &dyn LedgerSink) -> String {
        sink.prefix()
            .last()
            .map(|e| e.event_id.clone())
            .unwrap_or_else(|| hh_ledger::ids::ROOT_EVENT.to_string())
    }
}

impl Driver<ReactMinimal> {
    /// `open` for the common `react/minimal` case.
    pub fn open_react(
        ctx: &ControlContext,
        policy: EnvelopePolicy,
        sink: &mut dyn LedgerSink,
        config: DriverConfig,
    ) -> Result<Driver<ReactMinimal>, DriverError> {
        Driver::open(ReactMinimal::new(), ctx, policy, sink, config)
    }

    /// `resume_from` for the `ReactMinimal` strategy — the boundary's
    /// durable-resume entry (hh-embed `open_session{resume}` past a service
    /// restart; DF-S1.25-2 closed at S2.3).
    pub fn resume_react(
        ctx: &ControlContext,
        policy: EnvelopePolicy,
        checkpoint: &[u8],
        sink: &mut dyn LedgerSink,
        config: DriverConfig,
    ) -> Result<Driver<ReactMinimal>, DriverError> {
        Driver::resume_from(ReactMinimal::new(), ctx, policy, checkpoint, sink, config)
    }
}

fn scope_empty() -> Scope {
    Scope {
        turn_id: None,
        model_call_id: None,
        tool_call_id: None,
        effect_id: None,
        child_run_id: None,
        component_call_id: None,
        branch_id: None,
    }
}

/// Resolve `act{intents}` into gate dispatch intents (S3.10): the strategy
/// names `tool_call_id`s only (I2 — the ledgered intent never carries the
/// arg bytes), but the gate's dispatch routes on `intent.surface_id`
/// (`stop_rule = submit` detection, host-capability routing). The surface
/// comes from the `action.tool.proposed` row; the args from the
/// `model.call.completed` calls entry — both read back from the durable
/// prefix, never from strategy state. An intent already carrying
/// `surface`/`surface_id` (`plan_execute`'s validated plan steps) passes
/// through unchanged.
fn resolve_intents(prefix: &[EventEnvelope], intents: &[Json]) -> Vec<Json> {
    let mut surfaces: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    let mut args: std::collections::BTreeMap<String, Json> = std::collections::BTreeMap::new();
    for e in prefix {
        match e.class.as_str() {
            "action.tool.proposed" => {
                if let (Some(tc), Some(s)) = (
                    e.payload.get("tool_call_id").and_then(Json::as_str),
                    e.payload.get("surface_id").and_then(Json::as_str),
                ) {
                    surfaces.insert(tc.to_string(), s.to_string());
                }
            }
            "model.call.completed" => {
                if let Some(Json::Arr(calls)) = e.payload.get("calls") {
                    for c in calls {
                        if let (Some(tc), Some(raw)) = (
                            c.get("tool_call_id").and_then(Json::as_str),
                            c.get("args_raw").and_then(Json::as_str),
                        ) {
                            if let Ok(a) = hh_wire::json::parse(raw) {
                                args.insert(tc.to_string(), a);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    intents
        .iter()
        .map(|intent| {
            if intent.get("surface_id").is_some() || intent.get("surface").is_some() {
                return intent.clone();
            }
            let Some(tc) = intent.get("tool_call_id").and_then(Json::as_str) else {
                return intent.clone();
            };
            let mut m = match intent {
                Json::Obj(m) => m.clone(),
                _ => std::collections::BTreeMap::new(),
            };
            if let Some(s) = surfaces.get(tc) {
                m.insert("surface_id".into(), Json::str(s.clone()));
            }
            if let Some(a) = args.get(tc) {
                m.insert("args".into(), a.clone());
            }
            Json::Obj(m)
        })
        .collect()
}

/// Map a kernel stop-rule payload to a `StopReason` (the `check` path's
/// `stop{<canonical StopReason json>}` spellings — the fired members
/// (`budget_exhausted`'s dimension, `cancelled`'s `by`,
/// `format_failure`'s count) survive verbatim; the bare-kind spellings
/// remain as a fallback for any hand-made refusal).
/// The `confidence_ppm` a judged/human chain row carries — the verdict's
/// own graded confidence when it declares one, else the parsed/judged
/// ceiling. Never `1_000_000` — judged evidence keeps its grade (ADR-0110;
/// the same cap `ExtractedBy::Judged` carries on the claim surface).
fn judged_confidence_ppm(v: &hh_verification::validators::Verdict) -> u64 {
    match v.value {
        hh_verification::vocab::VerdictValue::Graded(ppm) => {
            ppm.min(hh_verification::vocab::PARSED_CONFIDENCE_CAP_PPM)
        }
        _ => hh_verification::vocab::PARSED_CONFIDENCE_CAP_PPM,
    }
}

/// Whether the guard's `respond` observation is a nudge — the observation
/// kinds that mint a conditioned `HarnessRule` artefact (AC-R-2.6.1-8;
/// I9). Returns the T-LCD-13 `kind` member (`loop_nudge | continue_nudge`
/// — §5e.1's ledger); denials, refusals, `compaction_required` and
/// escalations are not nudges.
fn nudge_kind(guard_id: &str) -> Option<&'static str> {
    match guard_id {
        "loop_nudge" => Some("loop_nudge"),
        // I6's continue-nudges: the stop-rule nudge, the converted
        // `stop{completed}` (`missing_submission`) and the format-error
        // row all deliver "continue with this hint" artefacts.
        "nudge" | "missing_submission" | "format_error" => Some("continue_nudge"),
        _ => None,
    }
}

/// The canonical nudge `Text` content per kind (a `Text` leaf the minted
/// `HarnessRule`'s `insert_context_item` names — §5e.1 "nudge and reminder
/// texts are `Text` leaves in `HarnessRule`s").
fn nudge_text(kind: &str) -> &'static str {
    match kind {
        "loop_nudge" => {
            "A repeated call pattern was detected. Change approach: different \
             arguments, a different capability, or finish."
        }
        _ => {
            "The run cannot complete on the current trajectory. Continue with \
             an admissible call or submit through the declared finish surface."
        }
    }
}

/// The `AssumptionDebtRecord` hypothesis per nudge kind (I9 — a conditioned
/// rule states the claim it rests on).
fn nudge_hypothesis(kind: &str) -> &'static str {
    match kind {
        "loop_nudge" => {
            "a loop-nudge reminder steers the conditioned model off the \
             detected repeat pattern; revisit when per-profile compliance \
             evidence lands (AC-R-2.6.1-8)"
        }
        _ => {
            "a continue-nudge steers the conditioned model to an admissible \
             step or the finish surface; revisit when per-profile compliance \
             evidence lands (AC-R-2.6.1-8)"
        }
    }
}

fn kernel_stop_reason(kind: &str, policy: &EnvelopePolicy) -> StopReason {
    if let Ok(j) = hh_wire::json::parse(kind) {
        if let Some(r) = StopReason::from_json(&j) {
            return r;
        }
    }
    match kind {
        "completed" => StopReason::Completed,
        "format_failure" => StopReason::FormatFailure { count: 0 },
        "budget_exhausted" => StopReason::BudgetExhausted {
            budget_id: policy.budget_ref.clone(),
            dimension: hh_ontology::dimensions::DimensionId::Turns,
        },
        "cancelled" => StopReason::Cancelled {
            by: CancelledBy::Principal,
        },
        other => StopReason::InfrastructureFailure {
            error_class: hh_ontology::control::InfraError {
                family: hh_ontology::control::InfraErrorFamily::Kernel,
                class: other.to_string(),
            },
        },
    }
}

/// Whether a `verification.artefact.followed` row already names the
/// delivery (the detector fires once per delivery — the first conforming
/// call is the evidence).
fn delivered_is_followed(prefix: &[EventEnvelope], delivery_id: &str) -> bool {
    prefix.iter().any(|e| {
        e.class == "verification.artefact.followed"
            && e.payload.get("delivery_id").and_then(Json::as_str) == Some(delivery_id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::EnvelopePolicy;
    use crate::strategy::{ConcurrentInput, ControlCapabilities, SteerMode, StrategyParams};
    use crate::vocab::{ControlDecision, HumanInput};

    /// An in-memory sink (the test double — `prefix()` is the fold input).
    struct MemSink {
        events: Vec<EventEnvelope>,
        seq: u64,
    }

    impl LedgerSink for MemSink {
        fn append(&mut self, events: Vec<Event>) -> Result<(), String> {
            for e in events {
                self.seq += 1;
                self.events.push(EventEnvelope {
                    event_id: e.event_id,
                    run_id: "run".into(),
                    seq: self.seq,
                    ts: e.ts,
                    hlc: None,
                    plane: hh_ledger::event::EventPlane::Control,
                    class: e.class,
                    schema_version: 1,
                    producer: e.producer,
                    participant_class: hh_ledger::manifest::ParticipantClass::Native,
                    observability_level: Default::default(),
                    durability: hh_ledger::classes::Durability::Ledger,
                    scope: e.scope,
                    lease_generation: 1,
                    parent_event_id: e.parent_event_id,
                    causes: e.causes,
                    refs: vec![],
                    ir_refs: vec![],
                    surface_ids: Default::default(),
                    provenance: e.provenance,
                    payload: e.payload,
                    prev_hash: "h".into(),
                    hash: format!("h{}", self.seq),
                });
            }
            Ok(())
        }
        fn prefix(&self) -> &[EventEnvelope] {
            &self.events
        }
    }

    /// A scripted model port (the deterministic Stage-1 double).
    struct ScriptedModel {
        script: std::collections::VecDeque<ModelOutcome>,
    }

    impl ModelPort for ScriptedModel {
        fn call(&mut self, _id: &str, _req: &Json) -> ModelOutcome {
            self.script.pop_front().unwrap_or(ModelOutcome {
                stop_reason: hh_gateway::vocab::StopReason::EndTurn,
                response_ref: "r-empty".into(),
                text_empty: true,
                calls: vec![],
                error_class: None,
                retry_after_ms: None,
            })
        }
    }

    /// A scripted gate.
    struct ScriptedGate {
        out: GateOutcome,
        /// The structured `finish` record the gate "detected" (the claim
        /// surface — `None` exercises the synthesized-claim fallback).
        finish: Option<Json>,
    }

    impl EffectGate for ScriptedGate {
        fn dispatch(&mut self, _ef: &str, _a: u64, _i: &Json) -> GateOutcome {
            self.out.clone()
        }
        fn finish_record(&self) -> Option<Json> {
            self.finish.clone()
        }
    }

    struct NullAssembler;
    impl AssemblerPort for NullAssembler {
        fn assemble(&mut self, _inputs: &AssembleInputs<'_>, _req: &Json) -> AssembledRequest {
            AssembledRequest {
                request: Json::Null,
                assembled_payload: None,
                side_events: Vec::new(),
                pre_events: Vec::new(),
            }
        }
    }

    fn ctx() -> ControlContext {
        ControlContext {
            process_ref: "proc-1".into(),
            plan: vec![],
            boundary: crate::react::react_preset(),
            profile: Json::Null,
            account_ref: "acct".into(),
            budget_ref: "b-1".into(),
            envelope_ref: "env-1".into(),
            parameters: StrategyParams::default(),
            capabilities_available: vec![],
            steering: (SteerMode::Unsupported, ConcurrentInput::QueueOnly),
        }
    }

    fn outcome(sr: hh_gateway::vocab::StopReason, calls: Vec<ParsedCall>) -> ModelOutcome {
        ModelOutcome {
            stop_reason: sr,
            response_ref: "r-1".into(),
            text_empty: calls.is_empty(),
            calls,
            error_class: None,
            retry_after_ms: None,
        }
    }

    #[test]
    fn a_submission_run_drives_the_full_loop_to_completed() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver = Driver::open_react(
            &ctx(),
            policy,
            &mut sink,
            DriverConfig {
                surfaces: vec![SurfaceSpec {
                    surface_id: "fs.read".into(),
                    semantic_id: "sem/fs.read".into(),
                    params: [(
                        "path".into(),
                        crate::output::ParamSpec {
                            required: true,
                            kind: crate::output::ParamKind::Str,
                            enum_values: vec![],
                            domain: vec![],
                        },
                    )]
                    .into_iter()
                    .collect(),

                    risk_class: None,
                }],
                ..DriverConfig::default()
            },
        )
        .unwrap();
        let mut model = ScriptedModel {
            script: [outcome(
                hh_gateway::vocab::StopReason::ToolUse,
                vec![ParsedCall {
                    tool_call_id: "tc-1".into(),
                    surface: "fs.read".into(),
                    args_raw: r#"{"path":"/a"}"#.into(),
                }],
            )]
            .into_iter()
            .collect(),
        };
        let mut gate = ScriptedGate {
            out: GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: Some("sub-1".into()),
                error_class: None,

                emitted: Vec::new(),
            },
            finish: None,
        };
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert_eq!(r.report.stop_reason, StopReason::Completed);
        assert_eq!(r.report.submission_ref.as_deref(), Some("sub-1"));
        // The audit row set: decision rows carry checkpoint_ref; the run
        // finished row carries outcome_class + drain_report_ref.
        let classes: Vec<&str> = sink.events.iter().map(|e| e.class.as_str()).collect();
        assert!(classes.contains(&"control.decision"));
        assert!(classes.contains(&"action.tool.proposed"));
        assert!(classes.contains(&"action.effect.intended"));
        assert!(classes.contains(&"action.effect.observed"));
        assert!(classes.contains(&"lifecycle.run.finished"));
        let decision = sink
            .events
            .iter()
            .find(|e| e.class == "control.decision")
            .unwrap();
        assert!(decision.payload.get("checkpoint_ref").is_some());
        let finished = sink
            .events
            .iter()
            .find(|e| e.class == "lifecycle.run.finished")
            .unwrap();
        assert_eq!(
            finished.payload.get("outcome_class").and_then(Json::as_str),
            Some("scored")
        );
    }

    #[test]
    fn a_completed_stop_emits_completion_and_claim_rows() {
        // AC-R-2.7.2a-1: every `stop{completed}` emits
        // `verification.completion.proposed` + ≥1
        // `verification.claim.recorded` with `kind ∈ {achieved,
        // unachievable}`, the `criteria_status` table carried through, and
        // `authority = delegate` provenance on every row.
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver = Driver::open_react(
            &ctx(),
            policy,
            &mut sink,
            DriverConfig {
                surfaces: vec![SurfaceSpec {
                    surface_id: "fs.read".into(),
                    semantic_id: "sem/fs.read".into(),
                    params: [(
                        "path".into(),
                        crate::output::ParamSpec {
                            required: true,
                            kind: crate::output::ParamKind::Str,
                            enum_values: vec![],
                            domain: vec![],
                        },
                    )]
                    .into_iter()
                    .collect(),

                    risk_class: None,
                }],
                ..DriverConfig::default()
            },
        )
        .unwrap();
        let mut model = ScriptedModel {
            script: [outcome(
                hh_gateway::vocab::StopReason::ToolUse,
                vec![ParsedCall {
                    tool_call_id: "tc-1".into(),
                    surface: "fs.read".into(),
                    args_raw: r#"{"path":"/a"}"#.into(),
                }],
            )]
            .into_iter()
            .collect(),
        };
        let mut gate = ScriptedGate {
            out: GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: Some("sub-1".into()),
                error_class: None,

                emitted: Vec::new(),
            },
            finish: Some(Json::obj([
                ("completion", Json::str("achieved")),
                (
                    "criteria_status",
                    Json::Arr(vec![Json::obj([
                        ("criterion_ref", Json::str("c:read-done")),
                        ("status", Json::str("met")),
                        ("evidence_refs", Json::Arr(vec![Json::str("sha256:ev-1")])),
                    ])]),
                ),
                (
                    "artifacts_claimed",
                    Json::Arr(vec![Json::str("sha256:art-1")]),
                ),
            ])),
        };
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert_eq!(r.report.stop_reason, StopReason::Completed);
        // `completion.proposed` carries the proposing call at the `stop`
        // decision point.
        let proposed = sink
            .events
            .iter()
            .find(|e| e.class == "verification.completion.proposed")
            .expect("completion.proposed emitted");
        assert_eq!(
            proposed
                .payload
                .get("decision_point")
                .and_then(Json::as_str),
            Some("stop")
        );
        // ≥1 `claim.recorded` — the completion claim (kind = achieved,
        // criteria_status aligned) plus the per-artifact `effected` claim.
        let recorded: Vec<&EventEnvelope> = sink
            .events
            .iter()
            .filter(|e| e.class == "verification.claim.recorded")
            .collect();
        assert!(recorded.len() >= 2, "completion + artifact claims");
        let completion = &recorded[0];
        assert_eq!(
            completion.payload.get("kind").and_then(Json::as_str),
            Some("achieved")
        );
        assert_eq!(
            completion.payload.get("predicate").and_then(Json::as_str),
            Some("is_done")
        );
        // The criteria_status table is carried through verbatim.
        let cs = match completion.payload.get("criteria_status") {
            Some(Json::Arr(items)) => items,
            _ => panic!("criteria_status present"),
        };
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].get("status").and_then(Json::as_str), Some("met"));
        assert_eq!(
            cs[0].get("criterion_ref").and_then(Json::as_str),
            Some("c:read-done")
        );
        // Provenance on every claim row is `authority = delegate` — the
        // claim is the model's, never a kernel fact (AC-R-2.7.2a-1/CC2).
        for e in &recorded {
            let p = e.provenance.as_ref().expect("provenance mandatory");
            assert_eq!(
                p.authority,
                hh_provenance::authority::AuthorityClass::Delegate
            );
        }
        // The per-artifact claim is `effected` on `artifact{…}`.
        assert_eq!(
            recorded[1].payload.get("kind").and_then(Json::as_str),
            Some("effected")
        );
        // Claim rows precede `lifecycle.run.finished` — the gate reads them.
        let pos = |class: &str| sink.events.iter().position(|e| e.class == class).unwrap();
        assert!(pos("verification.claim.recorded") < pos("lifecycle.run.finished"));
    }

    #[test]
    fn an_error_model_call_retries_then_stops_infra() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver =
            Driver::open_react(&ctx(), policy, &mut sink, DriverConfig::default()).unwrap();
        let mut model = ScriptedModel {
            script: std::iter::repeat_n(
                ModelOutcome {
                    stop_reason: hh_gateway::vocab::StopReason::Error,
                    response_ref: "r".into(),
                    text_empty: true,
                    calls: vec![],
                    error_class: Some("network".into()),
                    retry_after_ms: None,
                },
                10,
            )
            .collect(),
        };
        let mut gate = ScriptedGate {
            out: GateOutcome {
                outcome: SettledOutcome::Abandoned,
                submission_ref: None,
                error_class: None,
                emitted: Vec::new(),
            },
            finish: None,
        };
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert!(
            matches!(
                r.report.stop_reason,
                StopReason::InfrastructureFailure { .. }
            ),
            "got {:?}",
            r.report.stop_reason
        );
        assert!(sink
            .events
            .iter()
            .any(|e| e.class == "control.retry.scheduled"));
    }

    #[test]
    fn an_interrupt_stops_cancelled() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver =
            Driver::open_react(&ctx(), policy, &mut sink, DriverConfig::default()).unwrap();
        driver.submit(Cue::HumanInput(crate::vocab::HumanInput::Interrupt));
        // First the run_opened cue fires a propose… park by giving no model
        // outcome → the inbox drains to the interrupt? No — run_opened is
        // first; decide→propose→model call → EndTurn empty → format_error →
        // propose → … To keep the test deterministic, drive the interrupt
        // as the *second* cue after the first propose completes.
        let mut model = ScriptedModel {
            script: [ModelOutcome {
                stop_reason: hh_gateway::vocab::StopReason::Cancelled,
                response_ref: "r".into(),
                text_empty: true,
                calls: vec![],
                error_class: None,
                retry_after_ms: None,
            }]
            .into_iter()
            .collect(),
        };
        let mut gate = ScriptedGate {
            out: GateOutcome {
                outcome: SettledOutcome::Abandoned,
                submission_ref: None,
                error_class: None,
                emitted: Vec::new(),
            },
            finish: None,
        };
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert!(matches!(r.report.stop_reason, StopReason::Cancelled { .. }));
    }

    #[test]
    fn resume_restores_the_leaf_and_observes_the_tail() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let driver =
            Driver::open_react(&ctx(), policy, &mut sink, DriverConfig::default()).unwrap();
        let ckpt = driver.checkpoint();
        let policy2 = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver2 =
            Driver::open_react(&ctx(), policy2, &mut sink, DriverConfig::default()).unwrap();
        driver2.resume(&ctx(), &ckpt, &mut sink).unwrap();
        assert_eq!(driver2.state().variant_ref, "hh/react-minimal@1");
        // The `resumed` cue is queued (the post-restore cue).
        assert!(driver2
            .inbox
            .iter()
            .any(|c| matches!(c, Cue::Resumed { .. })));
    }

    // ── AC-R-2.6.1-5 fault-injection battery (I4 — every stub family
    // terminates with a typed stop inside budget) ────────────────────────

    /// A surface spec for `fs.read` (the scripted-call target).
    fn fs_read_surface() -> SurfaceSpec {
        SurfaceSpec {
            surface_id: "fs.read".into(),
            semantic_id: "sem/fs.read".into(),
            params: [(
                "path".into(),
                crate::output::ParamSpec {
                    required: true,
                    kind: crate::output::ParamKind::Str,
                    enum_values: vec![],
                    domain: vec![],
                },
            )]
            .into_iter()
            .collect(),
            risk_class: None,
        }
    }

    fn tool_call_outcome(path: &str) -> ModelOutcome {
        outcome(
            hh_gateway::vocab::StopReason::ToolUse,
            vec![ParsedCall {
                tool_call_id: format!("tc-{path}"),
                surface: "fs.read".into(),
                args_raw: format!(r#"{{"path":"{path}"}}"#),
            }],
        )
    }

    fn observed_gate() -> ScriptedGate {
        ScriptedGate {
            out: GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: None,
                error_class: None,

                emitted: Vec::new(),
            },
            finish: None,
        }
    }

    #[test]
    fn ac5a_always_tool_calls_stops_budget_exhausted() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut cfg = DriverConfig {
            surfaces: vec![fs_read_surface()],
            ..DriverConfig::default()
        };
        cfg.budget_ceiling.insert("model_calls".into(), 4);
        let mut driver = Driver::open_react(&ctx(), policy, &mut sink, cfg).unwrap();
        let mut model = ScriptedModel {
            script: std::iter::repeat_n(tool_call_outcome("/a"), 20).collect(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert!(
            matches!(r.report.stop_reason, StopReason::BudgetExhausted { .. }),
            "got {:?}",
            r.report.stop_reason
        );
        // The stop decision row is decider:envelope (the envelope owns it).
        let stop_row = sink
            .events
            .iter()
            .find(|e| {
                e.class == "control.decision"
                    && e.payload.get("kind").and_then(Json::as_str) == Some("stop")
            })
            .expect("a stop decision row");
        assert_eq!(
            stop_row.payload.get("decider").and_then(Json::as_str),
            Some("envelope")
        );
    }

    #[test]
    fn ac5b_never_tool_calls_stops_format_failure() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver = Driver::open_react(
            &ctx(),
            policy,
            &mut sink,
            DriverConfig {
                surfaces: vec![fs_read_surface()],
                ..DriverConfig::default()
            },
        )
        .unwrap();
        // ScriptedModel's default outcome is `end_turn` text-empty.
        let mut model = ScriptedModel {
            script: [].into_iter().collect(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert!(matches!(
            r.report.stop_reason,
            StopReason::FormatFailure { .. }
        ));
        // Every rejection row carries `failure_class` + `repaired`, never
        // the rejected bytes (AC-R-2.6.2-5).
        let rejected: Vec<_> = sink
            .events
            .iter()
            .filter(|e| e.class == "control.output.rejected")
            .collect();
        assert!(!rejected.is_empty());
        for e in &rejected {
            assert!(e.payload.get("failure_class").is_some());
            assert_eq!(e.payload.get("repaired"), Some(&Json::Bool(false)));
        }
    }

    /// S3.9 / AC-R-2.5.2-7 (run half): a parser-detected `SurfaceFailure`
    /// (`unparseable`) lands **both** rows — the §5e.2 control-plane
    /// `control.output.rejected` (drives `format_failures_running`) and the
    /// §5d.2 action-plane `action.tool.surface_rejected` with the full
    /// payload — and no `Effect`/`action.tool.{started,completed}` exists.
    #[test]
    fn ac_e2_7_unparseable_call_dual_emits_surface_rejected() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver = Driver::open_react(
            &ctx(),
            policy,
            &mut sink,
            DriverConfig {
                surfaces: vec![fs_read_surface()],
                ..DriverConfig::default()
            },
        )
        .unwrap();
        let mut model = ScriptedModel {
            script: [
                outcome(
                    hh_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-bad".into(),
                        surface: "fs.read".into(),
                        args_raw: "{not json".into(),
                    }],
                ),
                outcome(hh_gateway::vocab::StopReason::EndTurn, vec![]),
            ]
            .into_iter()
            .collect(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        let _ = r;
        let sr: Vec<_> = sink
            .events
            .iter()
            .filter(|e| e.class == "action.tool.surface_rejected")
            .collect();
        assert_eq!(sr.len(), 1, "exactly one surface_rejected row: {sr:?}");
        let p = &sr[0].payload;
        assert_eq!(
            p.get("failure_class").and_then(Json::as_str),
            Some("unparseable")
        );
        assert_eq!(p.get("surface_id").and_then(Json::as_str), Some("fs.read"));
        assert_eq!(p.get("binding_ref").and_then(Json::as_str), Some("fs.read"));
        assert!(p.get("raw_call_hash").and_then(Json::as_str).is_some());
        assert!(p.get("model_call_id").and_then(Json::as_str).is_some());
        assert!(p.get("rendering_ref").is_some(), "member present (null)");
        // No rejected bytes ride the row (ADR-0066 Rule C).
        assert!(p.get("args").is_none() && p.get("args_raw").is_none());
        // The control-plane row still lands beside it.
        assert!(sink
            .events
            .iter()
            .any(|e| e.class == "control.output.rejected"
                && e.payload.get("failure_class").and_then(Json::as_str) == Some("unparseable")));
        // No `Effect` record, no tool-call execution rows.
        assert!(sink
            .events
            .iter()
            .all(|e| !e.class.starts_with("action.effect.")));
        assert!(sink
            .events
            .iter()
            .all(|e| e.class != "action.tool.started" && e.class != "action.tool.completed"));
    }

    #[test]
    fn ac5c_identical_calls_stop_loop_detected() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut cfg = DriverConfig {
            surfaces: vec![fs_read_surface()],
            ..DriverConfig::default()
        };
        cfg.budget_ceiling.insert("model_calls".into(), 30);
        let mut driver = Driver::open_react(&ctx(), policy, &mut sink, cfg).unwrap();
        // The same canonical args → the same `loop_key` → `exact_repeat`.
        let mut model = ScriptedModel {
            script: std::iter::repeat_n(tool_call_outcome("/same"), 30).collect(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert!(
            matches!(r.report.stop_reason, StopReason::LoopDetected { .. }),
            "got {:?}",
            r.report.stop_reason
        );
        // The ladder: nudge → deny → stop (each `control.loop.detected`).
        let actions: Vec<&str> = sink
            .events
            .iter()
            .filter(|e| e.class == "control.loop.detected")
            .filter_map(|e| e.payload.get("action").and_then(Json::as_str))
            .collect();
        assert_eq!(actions, ["nudge", "deny", "stop"]);
    }

    #[test]
    fn ac5d_length_is_format_failure() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver = Driver::open_react(
            &ctx(),
            policy,
            &mut sink,
            DriverConfig {
                surfaces: vec![fs_read_surface()],
                ..DriverConfig::default()
            },
        )
        .unwrap();
        let mut model = ScriptedModel {
            script: std::iter::repeat_n(
                outcome(hh_gateway::vocab::StopReason::MaxOutput, vec![]),
                10,
            )
            .collect(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert!(matches!(
            r.report.stop_reason,
            StopReason::FormatFailure { .. }
        ));
    }

    // ── AC-R-2.6.1-3 — every `action.effect.intended` names its
    // `control.decision` in `causes[]` (I1) ──────────────────────────────
    #[test]
    fn ac3_every_intent_names_its_decision() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver = Driver::open_react(
            &ctx(),
            policy,
            &mut sink,
            DriverConfig {
                surfaces: vec![fs_read_surface()],
                ..DriverConfig::default()
            },
        )
        .unwrap();
        let mut model = ScriptedModel {
            script: [outcome(
                hh_gateway::vocab::StopReason::ToolUse,
                vec![ParsedCall {
                    tool_call_id: "tc-1".into(),
                    surface: "fs.read".into(),
                    args_raw: r#"{"path":"/a"}"#.into(),
                }],
            )]
            .into_iter()
            .collect(),
        };
        let mut gate = ScriptedGate {
            out: GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: Some("sub-1".into()),
                error_class: None,

                emitted: Vec::new(),
            },
            finish: None,
        };
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert_eq!(r.report.stop_reason, StopReason::Completed);
        let decision_ids: Vec<&str> = sink
            .events
            .iter()
            .filter(|e| e.class == "control.decision")
            .map(|e| e.event_id.as_str())
            .collect();
        for e in sink
            .events
            .iter()
            .filter(|e| e.class == "action.effect.intended")
        {
            assert!(
                e.causes
                    .iter()
                    .any(|c| decision_ids.contains(&c.event_id.as_str())),
                "intended {} has no control.decision in causes",
                e.event_id
            );
        }
    }

    // ── AC-R-2.6.1-4 — re-driving against the same scripted model_io
    // reproduces the decision sequence (deterministic replay) ────────────
    #[test]
    fn ac4_replay_reproduces_the_decision_sequence() {
        fn one_run() -> Vec<(String, String)> {
            let mut sink = MemSink {
                events: vec![],
                seq: 0,
            };
            let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
            let mut driver = Driver::open_react(
                &ctx(),
                policy,
                &mut sink,
                DriverConfig {
                    surfaces: vec![fs_read_surface()],
                    ..DriverConfig::default()
                },
            )
            .unwrap();
            let mut model = ScriptedModel {
                script: [
                    tool_call_outcome("/a"),
                    outcome(hh_gateway::vocab::StopReason::EndTurn, vec![]),
                    tool_call_outcome("/b"),
                ]
                .into_iter()
                .collect(),
            };
            let mut gate = ScriptedGate {
                out: GateOutcome {
                    outcome: SettledOutcome::Observed {
                        outcome: "applied".into(),
                    },
                    submission_ref: Some("sub-1".into()),
                    error_class: None,

                    emitted: Vec::new(),
                },
                finish: None,
            };
            let mut asm = NullAssembler;
            let _ = driver
                .run(&mut model, &mut gate, &mut asm, &mut sink)
                .unwrap();
            sink.events
                .iter()
                .filter(|e| e.class == "control.decision")
                .map(|e| {
                    (
                        e.payload
                            .get("kind")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string(),
                        e.payload
                            .get("decision_point")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string(),
                    )
                })
                .collect()
        }
        assert_eq!(one_run(), one_run());
    }

    // ── S2.11 — compaction routing, nudge T-LCD-13, verify, steer ────────

    /// A scripted compaction port (R-2.4.2's `compact` boundary — the
    /// `context.compaction.*` rows are the port's own; the driver sees the
    /// outcome).
    struct ScriptedCompaction {
        results: std::collections::VecDeque<Result<CompactionDone, CompactionImpossible>>,
        calls: u32,
    }

    impl CompactionPort for ScriptedCompaction {
        fn compact(&mut self, _reason: &str) -> Result<CompactionDone, CompactionImpossible> {
            self.calls += 1;
            self.results.pop_front().unwrap_or(Ok(CompactionDone {
                view_hash: "cv-1".into(),
                emitted: Vec::new(),
            }))
        }
    }

    /// A scripted verify port (R-2.7.1's decision-point seam).
    struct ScriptedVerify {
        seen: Vec<String>,
        verdicts: Vec<hh_verification::validators::Verdict>,
    }

    impl VerifyPort for ScriptedVerify {
        fn verify(
            &mut self,
            validator_refs: &[String],
            _subject: &Json,
        ) -> Vec<hh_verification::validators::Verdict> {
            self.seen.extend(validator_refs.iter().cloned());
            self.verdicts.clone()
        }
    }

    /// An assembler that reports a fixed occupancy in `context.assembled`
    /// (the gauge fold reads `occupancy_estimate` — §5c.1).
    struct OccupiedAssembler {
        occupancy: u64,
    }

    impl AssemblerPort for OccupiedAssembler {
        fn assemble(&mut self, _inputs: &AssembleInputs<'_>, _req: &Json) -> AssembledRequest {
            AssembledRequest {
                request: Json::Null,
                assembled_payload: Some(Json::obj([(
                    "occupancy_estimate",
                    Json::Int(self.occupancy as i64),
                )])),
                side_events: Vec::new(),
                pre_events: Vec::new(),
            }
        }
    }

    /// A scripted strategy — `decide` pops the scripted kind (exercises the
    /// driver arms no registered variant emits yet — `verify`).
    struct ScriptedStrategy {
        caps: ControlCapabilities,
        point: DecisionPoint,
        script: std::cell::RefCell<std::collections::VecDeque<DecisionKind>>,
    }

    impl ScriptedStrategy {
        fn new(point: DecisionPoint, script: Vec<DecisionKind>) -> Self {
            ScriptedStrategy {
                caps: ControlCapabilities {
                    deterministic_replay: true,
                    steering: false,
                    follow_up: false,
                    parallel_effects: false,
                    delegation: false,
                    model_emitted_plan: false,
                    resumable_mid_effect: true,
                    decision_points_owned: vec![],
                    boundary_preset: crate::react::react_preset(),
                    requires: crate::strategy::VariantRequires {
                        goal: true,
                        procedure: false,
                    },
                },
                point,
                script: std::cell::RefCell::new(script.into()),
            }
        }
    }

    impl ControlStrategy for ScriptedStrategy {
        fn capabilities(&self) -> &ControlCapabilities {
            &self.caps
        }
        fn open(&mut self, ctx: &ControlContext) -> Result<ControlState, ControlError> {
            ctx.boundary
                .validate()
                .map_err(ControlError::IncompatibleBoundary)?;
            Ok(ControlState {
                variant_ref: "test/scripted@1".into(),
                cursor: crate::state::PlanCursor {
                    node_id: "s-0".into(),
                    iteration: 0,
                    bound_ref: ctx.budget_ref.clone(),
                },
                decision_count: 0,
                open_effects: vec![],
                last_cue_seq: 0,
                boundary_view: ctx.boundary.assignments.clone(),
                extension: Json::Null,
            })
        }
        fn observe(&self, state: &mut ControlState, events: &[EventEnvelope]) {
            for ev in events {
                if ev.seq > state.last_cue_seq {
                    state.last_cue_seq = ev.seq;
                }
            }
        }
        fn decide(&self, state: &mut ControlState, _cue: &Cue) -> ControlDecision {
            let kind = self.script.borrow_mut().pop_front().unwrap_or({
                DecisionKind::Stop {
                    proposed_reason: StopReason::Refused {
                        blocking_effect_id: "scripted-end".into(),
                    },
                    submission_ref: None,
                }
            });
            let d = ControlDecision {
                stamp: crate::strategy::stamp_for(state, self.point, None),
                kind,
            };
            state.record_decision(d.stamp.decision_point, d.stamp.owner);
            d
        }
        fn terminate(&self, state: &ControlState, reason: &StopReason) -> FinalReport {
            FinalReport {
                stop_reason: reason.clone(),
                submission_ref: None,
                unresolved_effects: state.open_effects.clone(),
                decisions: vec![],
                boundary_observed: state.boundary_view.clone(),
            }
        }
        fn restore(
            &mut self,
            _bytes: &[u8],
            _ctx: &ControlContext,
        ) -> Result<ControlState, crate::strategy::RestoreError> {
            Err(crate::strategy::RestoreError::Malformed)
        }
    }

    fn test_verdict() -> hh_verification::validators::Verdict {
        use hh_verification::vocab as vv;
        hh_verification::validators::Verdict {
            verdict_id: "verdict:test:1".into(),
            validator_ref: hh_identity::VersionedRef::pinned(
                hh_identity::RecordKind::Validator,
                "sha256:test-validator",
                ProvenanceRecord::minted(
                    Origin::kernel("hir/kernel/check"),
                    PersistenceScope::Definition,
                    1,
                ),
            ),
            oracle_class: vv::OracleClass::Executable,
            target: "run-1".into(),
            criterion_ref: None,
            contract_id: None,
            phase: vv::VerdictPhase::Global,
            role: vv::CriterionRole::Invariant,
            value: vv::VerdictValue::Bool(true),
            status: vv::VerdictStatus::Decided,
            detector: vv::Detector::Deterministic,
            evidence_refs: vec![],
            inputs_digest: "sha256:inputs".into(),
            evidence_head_seq: 0,
            freshness_ok: true,
            findings: vec![],
            cost_ppm: 0,
            charged_to: vv::ChargedTo::Subject,
            veto_tripped: vec![],
            bundle_id: None,
            calibration_ref: None,
            independence_summary: None,
            uncited_findings: 0,
            provenance: ProvenanceRecord::minted(
                Origin::kernel("hir/kernel/check"),
                PersistenceScope::Run,
                1,
            ),
            measured_at: 0,
        }
    }

    /// AC-R-2.6.1-5 — `react/minimal` declares no `compact`: an occupancy
    /// cap at G-PRE-CALL routes `compaction_required` to
    /// `stop{context_exhausted{required_tokens, cap}}` with the guard's
    /// ledgered numbers (bounded — no unbounded compaction loop).
    #[test]
    fn minimal_occupancy_cap_stops_context_exhausted() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let cfg = DriverConfig {
            surfaces: vec![fs_read_surface()],
            window_cap_tokens: 1_000,
            ..DriverConfig::default()
        };
        let mut driver = Driver::open_react(&ctx(), policy, &mut sink, cfg).unwrap();
        let mut model = ScriptedModel {
            script: [outcome(hh_gateway::vocab::StopReason::EndTurn, vec![])]
                .into_iter()
                .collect(),
        };
        let mut gate = observed_gate();
        // 950/1000 ⇒ 950_000 ppm ≥ the 900_000 cap.
        let mut asm = OccupiedAssembler { occupancy: 950 };
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert!(
            matches!(
                r.report.stop_reason,
                StopReason::ContextExhausted { cap: 1_000, .. }
            ),
            "got {:?}",
            r.report.stop_reason
        );
        // The `guard_fired` row carries the ledgered pressure.
        let fired = sink
            .events
            .iter()
            .find(|e| {
                e.class == "control.guard.fired"
                    && e.payload.get("guard_id").and_then(Json::as_str)
                        == Some("compaction_required")
            })
            .expect("a compaction_required guard.fired row");
        assert_eq!(fired.payload.get("cap").and_then(Json::as_int), Some(1_000));
        assert!(
            fired
                .payload
                .get("required_tokens")
                .and_then(Json::as_int)
                .unwrap_or(0)
                > 0
        );
    }

    /// AC-R-2.6.1 steerable half — `guard_fired{compaction_required}` →
    /// `compact{reason}` → the port runs → `compaction_completed` → the
    /// loop continues; `CompactionImpossible` → `context_exhausted`.
    #[test]
    fn steerable_compact_invokes_port_then_exhausts_on_impossible() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let cfg = DriverConfig {
            surfaces: vec![fs_read_surface()],
            window_cap_tokens: 1_000,
            ..DriverConfig::default()
        };
        let mut c = ctx();
        c.steering = (SteerMode::InterruptAtDecisionPoint, ConcurrentInput::Steer);
        let mut driver = Driver::open(
            crate::react::ReactSteerable::new(),
            &c,
            policy,
            &mut sink,
            cfg,
        )
        .unwrap();
        let port = ScriptedCompaction {
            results: [
                Ok(CompactionDone {
                    view_hash: "cv-1".into(),
                    emitted: Vec::new(),
                }),
                Err(CompactionImpossible {
                    required_tokens: 950,
                    cap: 1_000,
                }),
            ]
            .into_iter()
            .collect(),
            calls: 0,
        };
        driver.set_compaction_port(Box::new(port));
        let mut model = ScriptedModel {
            script: [outcome(hh_gateway::vocab::StopReason::EndTurn, vec![])]
                .into_iter()
                .collect(),
        };
        let mut gate = observed_gate();
        let mut asm = OccupiedAssembler { occupancy: 950 };
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        // The impossible ladder is the envelope-owned context_exhausted.
        assert!(
            matches!(
                r.report.stop_reason,
                StopReason::ContextExhausted {
                    required_tokens: 950,
                    cap: 1_000
                }
            ),
            "got {:?}",
            r.report.stop_reason
        );
        // A `compact` decision was admitted (kind: compact).
        assert!(sink.events.iter().any(|e| {
            e.class == "control.decision"
                && e.payload.get("kind").and_then(Json::as_str) == Some("compact")
        }));
    }

    /// A `compact` decision with no wired port is the exhausted ladder —
    /// `stop{context_exhausted}` (never a fabricated compaction).
    #[test]
    fn compact_without_port_stops_context_exhausted() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let strategy = ScriptedStrategy::new(
            DecisionPoint::Compact,
            vec![DecisionKind::Compact {
                reason: "compaction_required".into(),
            }],
        );
        let mut driver =
            Driver::open(strategy, &ctx(), policy, &mut sink, DriverConfig::default()).unwrap();
        let mut model = ScriptedModel {
            script: Default::default(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert!(
            matches!(r.report.stop_reason, StopReason::ContextExhausted { .. }),
            "got {:?}",
            r.report.stop_reason
        );
        // The stop decision is the envelope's (decider: envelope).
        let stop_row = sink
            .events
            .iter()
            .rev()
            .find(|e| {
                e.class == "control.decision"
                    && e.payload.get("kind").and_then(Json::as_str) == Some("stop")
            })
            .expect("a stop decision row");
        assert_eq!(
            stop_row.payload.get("decider").and_then(Json::as_str),
            Some("envelope")
        );
    }

    /// AC-R-2.7.1 — `verify{validator_refs, subject}` runs the port and
    /// ledgers `verification.validator.invoked` + `.verdict`, then cues
    /// `verification_completed`.
    #[test]
    fn verify_decision_invokes_port_and_ledgers_verdicts() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let strategy = ScriptedStrategy::new(
            DecisionPoint::Verify,
            vec![DecisionKind::Verify {
                validator_refs: vec!["hir/kernel/diff_sanity".into()],
                subject: Json::obj([("effect_id", Json::str("ef-1"))]),
            }],
        );
        let mut driver =
            Driver::open(strategy, &ctx(), policy, &mut sink, DriverConfig::default()).unwrap();
        driver.set_verify_port(Box::new(ScriptedVerify {
            seen: vec![],
            verdicts: vec![test_verdict()],
        }));
        let mut model = ScriptedModel {
            script: Default::default(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
        assert!(sink
            .events
            .iter()
            .any(|e| e.class == "verification.validator.invoked"));
        assert!(sink
            .events
            .iter()
            .any(|e| e.class == "verification.validator.verdict"));
    }

    /// A `verify` decision with no wired port fails `UnbackedPort` — a
    /// declared verification never silently passes (R-2.7.1).
    #[test]
    fn verify_without_port_fails_unbacked() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let strategy = ScriptedStrategy::new(
            DecisionPoint::Verify,
            vec![DecisionKind::Verify {
                validator_refs: vec!["hir/kernel/diff_sanity".into()],
                subject: Json::Null,
            }],
        );
        let mut driver =
            Driver::open(strategy, &ctx(), policy, &mut sink, DriverConfig::default()).unwrap();
        let mut model = ScriptedModel {
            script: Default::default(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
        assert!(
            matches!(r, Err(DriverError::UnbackedPort { kind: "verify" })),
            "got {r:?}"
        );
    }

    /// AC-R-2.6.1-8 — a `loop_nudge` respond mints the conditioned
    /// `HarnessRule` (complete `AssumptionDebtRecord`) and ledgers the
    /// T-LCD-13 chain: `delivered` → `activated` → `followed` (the next
    /// admitted decision).
    #[test]
    fn a_loop_nudge_ledgers_the_lcd13_chain() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut cfg = DriverConfig {
            surfaces: vec![fs_read_surface()],
            ..DriverConfig::default()
        };
        cfg.budget_ceiling.insert("model_calls".into(), 30);
        let mut driver = Driver::open_react(&ctx(), policy, &mut sink, cfg).unwrap();
        let mut model = ScriptedModel {
            script: std::iter::repeat_n(tool_call_outcome("/same"), 30).collect(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let _ = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        // `delivered` — the minted `HarnessRule` with its debt record.
        let delivered = sink
            .events
            .iter()
            .find(|e| e.class == "context.artefact.delivered")
            .expect("a nudge delivered row");
        let rule = delivered.payload.get("rule").expect("rule member");
        assert_eq!(
            rule.get("assumption_debt")
                .and_then(|d| d.get("status"))
                .and_then(Json::as_str),
            Some("active"),
            "the nudge rule carries a complete assumption-debt record"
        );
        assert!(rule.get("conditioned_on").is_some());
        // `activated` — deterministic detector.
        assert!(sink.events.iter().any(|e| {
            e.class == "context.artefact.activated"
                && e.payload.get("detector").and_then(Json::as_str) == Some("deterministic")
        }));
        // `followed` — discharged by the next admitted decision.
        let followed = sink
            .events
            .iter()
            .find(|e| e.class == "verification.artefact.followed")
            .expect("a followed row");
        assert_eq!(
            followed.payload.get("kind").and_then(Json::as_str),
            Some("loop_nudge")
        );
        assert_eq!(followed.payload.get("verdict"), Some(&Json::Bool(true)));
    }

    /// AC-R-2.6.1-10 — a steer cue under `react/steerable` proposes the
    /// next step with `steer_ref` riding the context request (I7: the ref,
    /// never the bytes).
    #[test]
    fn steerable_steer_cue_proposes_with_steer_ref() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut c = ctx();
        c.steering = (SteerMode::InterruptAtDecisionPoint, ConcurrentInput::Steer);
        let mut driver = Driver::open(
            crate::react::ReactSteerable::new(),
            &c,
            policy,
            &mut sink,
            DriverConfig::default(),
        )
        .unwrap();
        // Park the loop: `deferred` → `wait{until: model_completed}` → the
        // inbox drains and `run` returns parked, then steer.
        let mut model = ScriptedModel {
            script: [outcome(hh_gateway::vocab::StopReason::Deferred, vec![])]
                .into_iter()
                .collect(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
        driver.submit(Cue::HumanInput(HumanInput::Steer {
            payload_ref: "sha256:steer-1".into(),
        }));
        let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
        // The steer decision is a `propose` whose context_request names the
        // steer ref.
        let steer_decision = sink.events.iter().find(|e| {
            e.class == "control.decision"
                && e.payload
                    .get("context_request")
                    .and_then(|cr| cr.get("steer_ref"))
                    .and_then(Json::as_str)
                    == Some("sha256:steer-1")
        });
        assert!(
            steer_decision.is_some(),
            "a propose{{steer_ref}} decision row"
        );
    }

    /// AC-R-2.6.4-1/2/3/4 — the §5e.4 seam end-to-end: `compute_policy`
    /// `uniform` binds `propose.sample_k = k_max` between `decide` and
    /// `envelope.check`, the admitted `control.decision` keeps
    /// kind/decision_point/owner (B-1), cites `compute_decision_ref`, and
    /// the `control.compute.decided` row lands before it with every
    /// supported option in `options_considered[]` (B-4).
    #[test]
    fn uniform_compute_policy_binds_through_the_driver_seam() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let facts = crate::compute::ComputeFacts {
            ensemble: Some(crate::compute::EnsembleFact {
                k_max: 5,
                oracle: Some("executable".into()),
            }),
            ..Default::default()
        };
        let mut driver = Driver::open_react(
            &ctx(),
            policy,
            &mut sink,
            DriverConfig {
                compute_policy_ref: "uniform".into(),
                compute_facts: facts,
                remaining: [
                    ("model_calls".to_string(), 8i64),
                    ("tokens.output.visible".to_string(), 1_000_000i64),
                ]
                .into_iter()
                .collect(),
                ..DriverConfig::default()
            },
        )
        .unwrap();
        let mut model = ScriptedModel {
            script: [outcome(
                hh_gateway::vocab::StopReason::ToolUse,
                vec![ParsedCall {
                    tool_call_id: "tc-1".into(),
                    surface: "fs.read".into(),
                    args_raw: r#"{"path":"/a"}"#.into(),
                }],
            )]
            .into_iter()
            .collect(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
        // The record lands *before* the `control.decision` it advises.
        let classes: Vec<&str> = sink.events.iter().map(|e| e.class.as_str()).collect();
        let rpos = classes
            .iter()
            .position(|c| *c == "control.compute.decided")
            .expect("a compute.decided row");
        let dpos = classes
            .iter()
            .position(|c| *c == "control.decision")
            .expect("a decision row");
        assert!(rpos < dpos, "the record precedes its decision");
        let record = &sink.events[rpos];
        let decision = &sink.events[dpos];
        // B-1 — the advised decision's kind/point/owner are the
        // strategy's; the binding lives in `context_request`.
        assert_eq!(
            decision.payload.get("kind").and_then(Json::as_str),
            Some("propose")
        );
        assert_eq!(
            decision
                .payload
                .get("context_request")
                .and_then(|cr| cr.get("sample_k"))
                .and_then(Json::as_int),
            Some(5),
            "the uniform binding rides context_request"
        );
        // The decision cites the record; the record names the decision.
        assert_eq!(
            decision
                .payload
                .get("compute_decision_ref")
                .and_then(Json::as_str),
            record.payload.get("record_id").and_then(Json::as_str),
        );
        // B-4 — every supported option appears with an estimate or a
        // typed infeasibility.
        let considered = match record.payload.get("options_considered") {
            Some(Json::Arr(rows)) => rows,
            _ => panic!("options_considered present"),
        };
        assert_eq!(
            considered.len(),
            crate::compute::ComputeOptionKind::ALL.len()
        );
        for row in considered {
            assert!(
                row.get("estimate").is_some() || row.get("infeasible").is_some(),
                "each row carries an estimate or a typed infeasibility"
            );
        }
    }

    /// AC-R-2.6.4-1 — `static` is the packaged null: the run emits zero
    /// `control.compute.*` rows (byte-identical modulo ids to a run
    /// without the slot).
    #[test]
    fn static_compute_policy_appends_no_compute_rows() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver =
            Driver::open_react(&ctx(), policy, &mut sink, DriverConfig::default()).unwrap();
        let mut model = ScriptedModel {
            script: [outcome(
                hh_gateway::vocab::StopReason::ToolUse,
                vec![ParsedCall {
                    tool_call_id: "tc-1".into(),
                    surface: "fs.read".into(),
                    args_raw: r#"{"path":"/a"}"#.into(),
                }],
            )]
            .into_iter()
            .collect(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
        assert!(sink
            .events
            .iter()
            .all(|e| !e.class.starts_with("control.compute.")));
    }
    /// AC-R-2.6.4-9 — a `model.rerouted` trigger lands
    /// `control.compute.prior_reset` before the next bind; the decided
    /// row's `priors_used` show the declared seeds zeroed (`n = 0`, mean
    /// `null`, `valid_until` = the reset seq) and the bandit falls back
    /// to `rules` (`bandit.cold_start` in `rules_fired`).
    #[test]
    fn prior_reset_zeroes_declared_seeds_and_falls_back() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        // The drift trigger sits in the durable prefix — appended rows
        // chain after it (seq 1 is the seeded row's; the counter resumes
        // at 2).
        sink.events.push(EventEnvelope {
            event_id: "ev-drift".into(),
            run_id: "run".into(),
            seq: 1,
            ts: "t".into(),
            hlc: None,
            plane: hh_ledger::event::EventPlane::Control,
            class: "model.rerouted".into(),
            schema_version: 1,
            producer: hh_ledger::event::Producer::kernel("t"),
            participant_class: hh_ledger::manifest::ParticipantClass::Native,
            observability_level: Default::default(),
            durability: hh_ledger::classes::Durability::Ledger,
            scope: hh_ledger::event::Scope {
                turn_id: None,
                model_call_id: None,
                tool_call_id: None,
                effect_id: None,
                child_run_id: None,
                component_call_id: None,
                branch_id: None,
            },
            lease_generation: 1,
            parent_event_id: "root".into(),
            causes: vec![],
            refs: vec![],
            ir_refs: vec![],
            surface_ids: Default::default(),
            provenance: None,
            payload: Json::obj([("to", Json::str("model/b"))]),
            prev_hash: "h".into(),
            hash: "h".into(),
        });
        sink.seq = 1;
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let facts = crate::compute::ComputeFacts {
            profile_ref: Some("profile/a".into()),
            snapshot_fingerprint: Some("snap/1".into()),
            task_class: Some("task/coding".into()),
            // A declared seed that would score the bandit — the reset
            // zeroes it before the bind reads ctx.
            priors: vec![crate::compute::PriorFact {
                cell: "profile/a|snap/1|task/coding|extend_search".into(),
                n: 5,
                mean_ppm: 900_000,
                interval: Some((700_000, 990_000)),
                valid_until: None,
            }],
            ..Default::default()
        };
        let mut driver = Driver::open_react(
            &ctx(),
            policy,
            &mut sink,
            DriverConfig {
                compute_policy_ref: "bandit".into(),
                compute_facts: facts,
                remaining: [("model_calls".to_string(), 8i64)].into_iter().collect(),
                ..DriverConfig::default()
            },
        )
        .unwrap();
        let mut model = ScriptedModel {
            script: [outcome(
                hh_gateway::vocab::StopReason::ToolUse,
                vec![ParsedCall {
                    tool_call_id: "tc-1".into(),
                    surface: "fs.read".into(),
                    args_raw: r#"{"path":"/a"}"#.into(),
                }],
            )]
            .into_iter()
            .collect(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
        // The reset row lands once, before the first decided row.
        let reset = sink
            .events
            .iter()
            .find(|e| e.class == "control.compute.prior_reset")
            .expect("a prior_reset row");
        assert_eq!(
            reset.payload.get("trigger_class").and_then(Json::as_str),
            Some("model.rerouted")
        );
        assert_eq!(
            reset.payload.get("reason").and_then(Json::as_str),
            Some("provider_drift")
        );
        assert_eq!(
            reset
                .payload
                .get("trigger_event_ref")
                .and_then(Json::as_str),
            Some("ev-drift")
        );
        assert_eq!(
            reset.payload.get("policy_ref").and_then(Json::as_str),
            Some("bandit")
        );
        let decided = sink
            .events
            .iter()
            .find(|e| e.class == "control.compute.decided")
            .expect("a compute.decided row");
        assert!(
            decided.seq > reset.seq,
            "the reset precedes the decided row it clears"
        );
        // The declared seed reads zeroed — n = 0, no mean — and the
        // rules fallback fired.
        let used = match decided.payload.get("priors_used") {
            Some(Json::Arr(u)) => u,
            _ => panic!("priors_used present"),
        };
        let cell = used
            .iter()
            .find(|u| {
                u.get("cell").and_then(Json::as_str)
                    == Some("profile/a|snap/1|task/coding|extend_search")
            })
            .expect("the declared cell appears zeroed");
        assert_eq!(cell.get("n").and_then(Json::as_int), Some(0));
        assert!(
            cell.get("mean_ppm").is_none() || cell.get("mean_ppm") == Some(&Json::Null),
            "a cleared cell's mean renders absent/null (the unknown read)"
        );
        let fired: Vec<&str> = match decided.payload.get("rules_fired") {
            Some(Json::Arr(f)) => f.iter().filter_map(Json::as_str).collect(),
            _ => vec![],
        };
        assert!(
            fired.contains(&"bandit.cold_start"),
            "the cleared bandit falls back to rules"
        );
        assert_eq!(
            decided
                .payload
                .get("estimator_ref")
                .and_then(|e| e.get("variant_ref"))
                .and_then(Json::as_str),
            Some("rules")
        );
    }
}
