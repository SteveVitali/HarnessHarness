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

use hh_compiler::exposure as tool_exposure;
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

// ── R-2.7 — the §5b.2 router seam (ADR-0121/ADR-0122) ────────────────────────
//
// The bound `router` slot arms a [`RoutingPort`]; when one is wired the
// driver mints the model-plane rows the spec orders (`model.route.decided`
// before the scope opens, `model.call.attempt.{started,completed,failed}`
// per attempt, `model.surface.relowered` → `model.rerouted` → restarted
// `model.call.attempt.started` across a profile boundary, the consult's
// `control.budget.{reserved,released}`/`model.profile.status.changed` side
// rows) — all through the run's fenced writer, durable-before-visible.
// With no port wired the legacy scripted lane runs byte-identical (no
// `route.decided`/`attempt.*` rows land — the AC's honest asymmetry).

/// The lane the bound `router` slot declares — the sealed data the driver
/// folds into every `RoutingRequest`: role, capability axes, the budget the
/// G-4 reserve charges, and the realized `RoutingPolicy` + `ModelRoleTable`.
/// Pure data (sealed-document decode is the boundary's job); the port owns
/// the views + the account.
#[derive(Debug, Clone, PartialEq)]
pub struct RoutingLane {
    /// `role` — the `ModelRole` the driver calls under (`"primary"`).
    pub role: String,
    /// `required_capabilities[]` — the axes the call needs (G-2).
    pub required_capabilities: Vec<String>,
    /// `budget_id` — the budget the reservation charges.
    pub budget_id: String,
    /// `task_class?` — the `bandit` cell axis.
    pub task_class: Option<String>,
    /// `latency_target_ms?` — the `latency_cap` target.
    pub latency_target_ms: Option<u64>,
    /// `effort?` — the requested effort rung.
    pub effort: Option<String>,
    /// `intent_ref?` — the recorded intent an `expired` bind needs (G-1).
    pub intent_ref: Option<String>,
    /// The bound `RoutingPolicy` document.
    pub policy: hh_gateway::router::RoutingPolicy,
    /// The realized `ModelRoleTable` (the manifest's `profile_binding`
    /// projection — same table the boundary realized at open).
    pub table: hh_gateway::router::ModelRoleTable,
}

/// The router port (R-2.7) — the boundary's half of §5b.2: the profile
/// environment (`SelectorView`), the budget account (`BudgetPort`), the
/// health/C1 views, and the profile-boundary `relower` producer are the
/// port's; the driver owns ordering + the appends. Side rows the consults
/// produce (reservation hold/release, `model.profile.status.changed`)
/// drain through [`RoutingPort::take_rows`] — the producer computes, the
/// fenced writer lands (CC3, the `AssembleOutcome.side_events`
/// convention).
pub trait RoutingPort {
    /// The bound lane declaration.
    fn lane(&self) -> &RoutingLane;
    /// The §5b.2 select — `select_with` over the port's declared views.
    /// `attempted` is the forward-only consumption set the durable fold
    /// rebuilt; `prefix` is the driver's durable tail.
    fn select(
        &mut self,
        request: &hh_gateway::router::RoutingRequest,
        decision_id: &str,
        now_ms: u64,
        attempted: &std::collections::BTreeSet<String>,
        prefix: &[EventEnvelope],
    ) -> Result<hh_gateway::router::RoutingDecision, hh_gateway::router::RoutingRefusal>;
    /// The ADR-0122 consult — `on_attempt_failed` over the port's views;
    /// `state` is the driver-rebuilt `AttemptState` cursor.
    #[allow(clippy::too_many_arguments)] // the consult arity is the contract's.
    fn attempt_failed(
        &mut self,
        request: &hh_gateway::router::RoutingRequest,
        prior: &hh_gateway::router::RoutingDecision,
        error: &hh_gateway::vocab::ModelErrorClass,
        attempts_on_target: u32,
        retry_after_ms: Option<u64>,
        state: &mut hh_gateway::router::AttemptState,
        decision_id: &str,
        now_ms: u64,
        prefix: &[EventEnvelope],
    ) -> hh_gateway::router::AttemptDisposition;
    /// The `model.surface.relowered` payload a cross-profile reroute mints
    /// (`old_profile_ref`/`new_profile_ref`/`dropped_items[]`/
    /// `rewritten_items[]`/`reason`/`model_call_id` members — the
    /// `check_reroute_order` derivation set). `Err` is a typed refusal —
    /// the boundary cannot project the move, the driver closes the call
    /// `select_refused` (never a silent skip).
    fn relower(
        &mut self,
        from_profile_ref: &str,
        to_profile_ref: &str,
        reason: &str,
        model_call_id: &str,
    ) -> Result<Json, String>;
    /// Rows the consults produced, in emit order — drained by the driver
    /// into the durable tail before the caller-visible cue.
    fn take_rows(&mut self) -> Vec<(String, Json)>;
}

/// `CacheBinding` — the `K5Key` members the `response_cache` slot's binding
/// declares; the driver composes `plan_hash` from the assembled request.
#[derive(Debug, Clone, PartialEq)]
pub struct CacheBinding {
    /// The scripted model's `provider_model_id` (the key's request axis).
    pub provider_model_id: String,
    /// The pinned `ModelSnapshotRecord` id (`None` ⇒ `"none"` in the key).
    pub snapshot_id: Option<String>,
    /// The pinned profile version id the entry's contract stamps.
    pub profile_version_id: String,
    /// The replicate index (cached responses never fake variance).
    pub replicate: u64,
    /// The run's `configuration_version_id`.
    pub configuration_version_id: String,
    /// The `idp/1` plan domain (`provider_request_plan.1`).
    pub plan_domain: String,
}

/// A served cache entry — the recorded response document + the entry ref
/// the terminal row's `served_from_cache` member stamps.
#[derive(Debug, Clone, PartialEq)]
pub struct ServedEntry {
    /// The content-addressed entry ref.
    pub entry_ref: String,
    /// The recorded response document (`{stop_reason, response_ref,
    /// text_empty, calls[], retry_after_ms?}` — the scripted lane's
    /// canonical `ModelOutcome` record).
    pub message: Json,
}

/// What a cache `resolve` produced — the `model.cache.resolved` payload the
/// driver lands plus the recorded `ModelOutcome` document when the outcome
/// serves (`hit`; a withheld/annotated entry never silently serves under
/// `mode = execute`).
#[derive(Debug, Clone, PartialEq)]
pub struct CacheResolution {
    /// The `model.cache.resolved` payload (one row per lookup — hits,
    /// misses, withhelds alike; ADR-0128 d.3).
    pub payload: Json,
    /// The served entry on a servable outcome.
    pub serve: Option<ServedEntry>,
}

/// The response-cache port (R-2.7's K5 store leg — §5b.4) — the scripted
/// lane's `K5Cache` adapter. The driver owns the `model.cache.resolved`
/// append (scoped to the call it serves) and the serve/record ordering.
pub trait ResponseCachePort {
    /// The declared binding members.
    fn binding(&self) -> &CacheBinding;
    /// `resolve(plan_hash)` — the lookup; returns the resolved payload +
    /// the recorded message when servable (K5 `hit`).
    fn resolve(&mut self, plan_hash: &str) -> CacheResolution;
    /// `record(plan_hash, message, usage, timing)` — a completed call's
    /// served artifacts write the entry; idempotent under the key.
    fn record(&mut self, plan_hash: &str, message: Json, usage: Json, timing: Json) -> String;
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
    /// The boundary's live clock (ms), when it carries one — R2.14
    /// (§5h.1 §2.6): `KernelSink` answers the `Store`'s clock so the
    /// driver's stamped `*_ms` members measure the runtime's wall; a
    /// `None` answer falls back to the driver's deterministic tick
    /// clock (the measured value lands durable either way — a replay
    /// reads the member, never the clock).
    fn now_ms(&self) -> Option<u64> {
        None
    }
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
    /// A `belief_probe` `ProfileRule` on the sealed profile is malformed
    /// (R2.15/ADR-0316 — a declared probe without its assumption-debt
    /// record refuses `open`/`resume` typed; the rule is never silently
    /// dropped).
    MalformedProfileRule {
        /// The offending rule.
        rule_id: String,
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
            DriverError::MalformedProfileRule { rule_id } => {
                write!(f, "malformed_profile_rule{{{rule_id}}}")
            }
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

/// The R2.8 exposure runtime binding (§5d.3; DF-S1.17-3) — `Some` arms the
/// exposure-aware lane: per `propose`, `select_surfaces` runs ahead of
/// `assemble` and `action.tool.exposure.planned` lands durable (the spec's
/// `DecisionPoint = retrieve`); the plan's `direct` set becomes the
/// delivered surface set G-INTERPRET/`validate` read; `check_callable`
/// runs on every parsed call (`action.tool.call.refused` on a refusal);
/// a call on the `discover_surfaces` capability executes in-kernel
/// (`action.tool.discovery.searched` + `action.tool.surface.revealed` per
/// hit — the kernel executor, never the effect gate); retention
/// boundaries mint `action.tool.surface.evicted`; a host source sync
/// (`list_changed`/TTL/`reconnect`/…) lands through
/// `sync_exposure_source` (`catalog.delta`/`epoch`/`built`).
/// `None` ⇒ the C0 lane — `config.surfaces` is the delivered set verbatim
/// and no exposure row emits (a disarmed run is byte-identical to a
/// pre-R2.8 run).
pub struct ExposureRuntime {
    /// The opening catalog — the compiled `SurfaceBinding`s projected at
    /// `seal`. `adopt`ed epochs advance the run-state copy (the config
    /// member is the epoch-0 opening, never mutated).
    pub catalog: tool_exposure::Catalog,
    /// The profile's admitted `definition_modes` (the meet's profile leg).
    pub profile_modes: std::collections::BTreeSet<hh_hir::tools::ExposureMode>,
    /// The MUST-data `ExposurePolicyParams` (ADR-0093 D3).
    pub params: tool_exposure::ExposurePolicyParams,
    /// The bound `tool_exposure_policy` rank leg — `None` ⇒ `direct_all`
    /// (the kernel admit/enforce legs still run).
    pub policy: Option<std::sync::Arc<dyn tool_exposure::ExposurePolicy>>,
    /// The selection gates (the attestation leg — `index_unverified`).
    pub gates: tool_exposure::SelectionGates,
    /// The C2 index executor legs (`embedding`/`model_ranked`/`filesystem`
    /// answer `ExecutorUnavailable` without one — never a fake ranking).
    pub executor: tool_exposure::IndexExecutor,
}

impl std::fmt::Debug for ExposureRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExposureRuntime")
            .field("catalog_id", &self.catalog.catalog_id)
            .field("entries", &self.catalog.entries.len())
            .field("policy", &self.policy.is_some())
            .field("gates", &self.gates)
            .field("executor", &self.executor)
            .finish()
    }
}

impl Clone for ExposureRuntime {
    fn clone(&self) -> Self {
        ExposureRuntime {
            catalog: self.catalog.clone(),
            profile_modes: self.profile_modes.clone(),
            params: self.params.clone(),
            policy: self.policy.clone(),
            gates: self.gates,
            executor: self.executor,
        }
    }
}

/// `DriverConfig` — the injected facts the driver needs that are not the
/// strategy's or the policy's (the logical clock's start, the compiled
/// surface set for G-INTERPRET, the gauge caps, the budget-conservation
/// view source, attendance).
#[derive(Debug, Clone)]
pub struct DriverConfig {
    /// The compiled surface set (`ModelSurface` projection — G-INTERPRET).
    pub surfaces: Vec<SurfaceSpec>,
    /// The exposure runtime (R2.8) — `None` ⇒ the C0 all-direct lane.
    pub exposure: Option<ExposureRuntime>,
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
            exposure: None,
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

/// The exposure run state (R2.8) — the per-run fold the emitters read and
/// `adopt` advances. `None` until the armed lane's first `propose`; the
/// durable-fold twin is `fold_exposure_state` (resume-by-leaf rebuilds
/// it from `catalog.built`/`delta`/`surface.revealed`/`evicted` rows —
/// CC3: run state is the ledger's projection, never a second record).
#[derive(Debug, Clone)]
struct ExposureRunState {
    /// The live catalog (the epoch-0 opening until `adopt` advances it).
    catalog: tool_exposure::Catalog,
    /// The revealed set (`action.tool.surface.revealed`/`evicted`'s fold).
    revealed: tool_exposure::RevealedSet,
    /// The last plan's `order` (I-ORDER's prefix input).
    prior_order: Vec<String>,
    /// The recent-call window (`turn_state.recent_calls`).
    recent_calls: Vec<String>,
    /// The last minted plan (`check_callable`'s plan-of-the-call read).
    plan: Option<tool_exposure::ExposurePlan>,
    /// `surface_id → delivery_id` — the `context.artefact.delivered{kind:
    /// tool_surface}` rows minted this run (first delivery only; a
    /// re-delivery of the same surface is not a new artefact row).
    delivered: std::collections::BTreeMap<String, String>,
    /// The plan's delivered `SurfaceSpec` set for the current call (the
    /// `direct`-mode members resolved against `config.surfaces`).
    delivered_surfaces: Vec<SurfaceSpec>,
    /// Whether the opening `catalog.built` row is durable.
    announced: bool,
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
    /// The router port (R-2.7 — the bound `router` slot's arm). `None` ⇒
    /// the legacy scripted lane runs byte-identical (no `route.decided` /
    /// `attempt.*` rows); `Some` ⇒ the §5b.2 emission set lands.
    routing_port: Option<Box<dyn RoutingPort>>,
    /// The response-cache port (R-2.7's K5 store leg). `None` ⇒ no
    /// `model.cache.resolved` rows (the lane declared no cache).
    cache_port: Option<Box<dyn ResponseCachePort>>,
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
    /// The decoded `belief_probe` `ProfileRule`s off the sealed profile
    /// projection (R2.15/ADR-0316 — the rule *is* the emitter: each
    /// `model.call.completed` fires the deterministic rules over the
    /// recorded `calls[].args_raw` belief fields; judged rules are
    /// declared-only here — the judged stratum needs the critic leg,
    /// DF-S1.21-3). A malformed rule refuses `open` typed, never a
    /// silent drop.
    belief_probe_rules: Vec<hh_verification::belief_probe::ProbeRule>,
    /// The exposure run state (R2.8) — `Some` only when
    /// `config.exposure` is armed; minted lazily at the first `propose`
    /// by folding the durable prefix (`fold_exposure_state` — open,
    /// resume and replay share the one init path).
    exposure_state: Option<ExposureRunState>,
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
        // R2.15 (ADR-0316) — decode the profile's `belief_probe` rules once;
        // a malformed rule refuses the constructor typed (a conditioned
        // rule is never silently dropped).
        let belief_probe_rules =
            hh_verification::belief_probe::probe_rules(&ctx.profile).map_err(|e| {
                DriverError::MalformedProfileRule {
                    rule_id: match e {
                        hh_verification::belief_probe::ProbeRuleError::DebtMissing { rule_id } => {
                            rule_id
                        }
                    },
                }
            })?;
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
                // R2.14 (§5h.1 §2.2) — the turn's member-stamped open:
                // `turn_phase_profile`'s t0 and `turn_e2e_ms`'s base.
                // `started_at_ms` is a measured instant (`ts` orders
                // nothing — §2.6).
                (
                    "started_at_ms",
                    Json::obj([
                        ("value", Json::Int(sink.now_ms().unwrap_or(0) as i64)),
                        ("measured_at", Json::str("runtime")),
                    ]),
                ),
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
            routing_port: None,
            cache_port: None,
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
            belief_probe_rules,
            exposure_state: None,
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
        // R2.15 (ADR-0316) — decode the profile's `belief_probe` rules once;
        // a malformed rule refuses the constructor typed (a conditioned
        // rule is never silently dropped).
        let belief_probe_rules =
            hh_verification::belief_probe::probe_rules(&ctx.profile).map_err(|e| {
                DriverError::MalformedProfileRule {
                    rule_id: match e {
                        hh_verification::belief_probe::ProbeRuleError::DebtMissing { rule_id } => {
                            rule_id
                        }
                    },
                }
            })?;

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
            routing_port: None,
            cache_port: None,
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
            belief_probe_rules,
            // Re-folded lazily from the durable prefix at the first armed
            // `propose` (`exposure_plan_call` — CC3).
            exposure_state: None,
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
        // R2.15 (ADR-0316) — decode the profile's `belief_probe` rules once;
        // a malformed rule refuses the constructor typed (a conditioned
        // rule is never silently dropped).
        let belief_probe_rules =
            hh_verification::belief_probe::probe_rules(&ctx.profile).map_err(|e| {
                DriverError::MalformedProfileRule {
                    rule_id: match e {
                        hh_verification::belief_probe::ProbeRuleError::DebtMissing { rule_id } => {
                            rule_id
                        }
                    },
                }
            })?;
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
            routing_port: None,
            cache_port: None,
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
            belief_probe_rules,
            exposure_state: None,
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

    /// `set_routing_port` — arm the §5b.2 router seam (R-2.7; the bound
    /// `router` slot's port). The boundary re-arms it per `drive` (the
    /// port's profile/pricing views are declared snapshots read at
    /// consult; between-drive registry republishes are observed on the
    /// next consult — durable rows, never a cached lie).
    pub fn set_routing_port(&mut self, port: Box<dyn RoutingPort>) {
        self.routing_port = Some(port);
    }

    /// `set_cache_port` — arm the `response_cache` slot's K5 store leg
    /// (R-2.7). `None`/unset ⇒ no `model.cache.resolved` rows land.
    pub fn set_cache_port(&mut self, port: Box<dyn ResponseCachePort>) {
        self.cache_port = Some(port);
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
                // R2.8 — `select_surfaces → action.tool.exposure.planned`
                // runs ahead of `assemble` (§5d.3: `DecisionPoint =
                // retrieve`; durable-before-visible — the plan lands
                // before the context it shapes).
                self.exposure_plan_call(sink, &mc)?;
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
                    // R-2.7 — the routed lane's reroute execution: a
                    // pending `model.route.decided` (the failure consult's
                    // fresh decision, minted durable inside `model_round`)
                    // mints `model.surface.relowered` on a cross-profile
                    // move then `model.rerouted` — both scope-free: the
                    // failed terminal already closed the `mc` scope (the
                    // payload's `model_call_id` member is the join).
                    if self.routing_port.is_some() {
                        if let Some((fresh, prior)) = pending_reroute(sink.prefix(), &model_call_id)
                        {
                            let mut relower_ref: Option<String> = None;
                            if fresh.relower_required {
                                let payload =
                                    self.routing_port.as_mut().expect("routed lane").relower(
                                        &prior.selected.profile_ref,
                                        &fresh.selected.profile_ref,
                                        "model_retry",
                                        &model_call_id,
                                    );
                                match payload {
                                    Ok(p) => {
                                        let ev_id = self.alloc("e");
                                        self.append_with_id(
                                            sink,
                                            &ev_id,
                                            "model.surface.relowered",
                                            p,
                                            None,
                                        )?;
                                        relower_ref = Some(ev_id);
                                    }
                                    Err(e) => {
                                        // The boundary cannot project the
                                        // move — a typed refusal, never a
                                        // skipped leg: the audit row +
                                        // the `Refused` cue, no re-drive.
                                        let mut fired = match crate::events::guard_fired_payload(
                                            DecisionPoint::Retry,
                                            "route_reroute",
                                        ) {
                                            Json::Obj(m) => m,
                                            other => unreachable!("guard_fired_payload: {other:?}"),
                                        };
                                        fired.insert("verdict".into(), Json::str("refused"));
                                        fired
                                            .insert("required".into(), Json::str("relower_failed"));
                                        fired.insert("detail".into(), Json::str(&e));
                                        fired.insert(
                                            "model_call_id".into(),
                                            Json::str(&model_call_id),
                                        );
                                        self.append(
                                            sink,
                                            "control.guard.fired",
                                            Json::Obj(fired),
                                            None,
                                        )?;
                                        self.inbox.push_back(Cue::EnvelopeSignal(
                                            EnvelopeSignal::Refused {
                                                decision_ref: model_call_id.clone(),
                                                reason: format!("route_reroute:relower_failed:{e}"),
                                            },
                                        ));
                                        return Ok(());
                                    }
                                }
                            }
                            let policy = self
                                .routing_port
                                .as_ref()
                                .expect("routed lane")
                                .lane()
                                .policy
                                .clone();
                            let reason = reroute_reason(sink.prefix(), &model_call_id, &policy);
                            self.append(
                                sink,
                                "model.rerouted",
                                hh_gateway::events::rerouted(
                                    &model_call_id,
                                    &prior.selected.provider_model_id,
                                    &fresh.selected.provider_model_id,
                                    &reason,
                                    next_attempt as u32,
                                    fresh.relower_required,
                                    relower_ref.as_deref(),
                                    &fresh.decision_id,
                                ),
                                None,
                            )?;
                            if let Json::Obj(m) = &mut request {
                                m.insert(
                                    "profile_override".to_string(),
                                    Json::str(&fresh.selected.profile_ref),
                                );
                            }
                        }
                    } else {
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
                            // Scope-free: the failed terminal already closed
                            // the `mc` scope — the payload's `model_call_id`
                            // member is the join (the `Some` scope id would
                            // trip `ScopeNotOpen` under the real writer).
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
                                None,
                            )?;
                            if let Json::Obj(m) = &mut request {
                                m.insert("profile_override".to_string(), Json::str(profile_ref));
                            }
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
                self.compact_round(sink, &reason, None)?;
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
        // R-2.7 — the routed lane: the §5b.2 select runs *before* the
        // request opens the call scope (the route decision lands durable
        // ahead of the call it binds — durable-before-visible). A pending
        // reroute's fresh decision was minted at the failure consult —
        // the durable fold is the cursor, so the select runs only when no
        // decision exists for `mc` yet (INV-5's shared identity).
        if self.routing_port.is_some()
            && route_decisions(sink.prefix(), &mc).is_empty()
            && !self.route_select(sink, &mc)?
        {
            return Ok(());
        }
        let active_decision = route_decisions(sink.prefix(), &mc)
            .last()
            .map(|(_, d)| d.clone());
        let mut requested_members = vec![
            ("model_call_id", Json::str(&mc)),
            ("attempt_no", Json::Int(attempt as i64)),
            ("request_ref", Json::str(format!("req-{mc}-a{attempt}"))),
        ];
        if let Some(d) = &active_decision {
            // The bound `ModelRef` + the profile the attempt runs under —
            // the health view's join key and the reroute's derivation.
            requested_members.push(("model_ref", d.selected.to_json()));
            requested_members.push(("profile_ref", Json::str(&d.selected.profile_ref)));
            requested_members.push(("route_decision_ref", Json::str(&d.decision_id)));
        }
        // R2.14 — the M3 open boundary's member stamp (the phase
        // profile's sampling intervals open here).
        requested_members.push(("at_ms", self.stamp_ms(sink)));
        self.append(
            sink,
            "model.call.requested",
            Json::obj(requested_members),
            Some(&mc),
        )?;
        // The K5 lookup — one `model.cache.resolved` row per lookup (the
        // row lands scoped to the call it serves; a `hit` replays the
        // recorded response verbatim — §5b.4).
        let k5 = self.k5_resolve(sink, &mc, request)?;
        let served = k5.as_ref().and_then(|(_, sv)| sv.clone());
        // The attempt span opens only when the call actually runs — a
        // served hit is no attempt (the honest row set: requested →
        // cache.resolved → completed{served_from_cache}).
        let outcome = match &served {
            Some(entry) => Self::served_outcome(&entry.message).ok_or_else(|| {
                DriverError::Append(format!("k5 serve decode failed for {}", entry.entry_ref))
            })?,
            None => {
                if self.routing_port.is_some() {
                    self.append(
                        sink,
                        "model.call.attempt.started",
                        hh_gateway::events::attempt_started(&mc, attempt as u32, 0),
                        Some(&mc),
                    )?;
                }
                model.call(&mc, request)
            }
        };
        // G-INTERPRET — output validation + loop detectors + empty ladder.
        let interp = self.envelope.guard(
            sink.prefix(),
            GuardPoint::Interpret,
            &GuardInput::Interpret {
                model_call_id: mc.clone(),
                stop_reason: outcome.stop_reason,
                text_empty: outcome.text_empty,
                calls: outcome.calls.clone(),
                // R2.8 — the armed lane validates against the plan's
                // `direct` set (a deferred/unrevealed name is
                // `unknown_surface` to G-INTERPRET; `check_callable`'s
                // `call.refused` row is the §5d.3 leg beside it).
                surfaces: self.delivered_surfaces(),
            },
            &self.guard_ctx(sink.prefix()),
        );
        let mut proposed_tool_calls: Vec<String> = vec![];
        // R2.8 — `check_callable` before the monitor (ADR-0093 D7): every
        // parsed call on a surface that was not `direct` in this call's
        // plan mints `action.tool.call.refused` — the §5d.3 row beside
        // the interpret pipeline's `unknown_surface`/`surface_rejected`.
        for c in &outcome.calls {
            self.exposure_check_call(sink, &mc, c)?;
        }
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
                &self.delivered_surfaces(),
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
                    let proposed_at = self.stamp_ms(sink);
                    self.append(
                        sink,
                        "action.tool.proposed",
                        Json::obj([
                            ("tool_call_id", Json::str(&c.tool_call_id)),
                            ("surface_id", Json::str(&c.surface_id)),
                            ("loop_key", Json::str(&c.loop_key)),
                            // R2.14 — the tool-blocking interval's open
                            // stamp (the phase profile reads it).
                            ("at_ms", proposed_at),
                        ]),
                        Some(&mc),
                    )?;
                    // AC-E3-7 / T-LCD-13 (R2.8): a call to a revealed
                    // surface IS its `context.artefact.activated` —
                    // `artefact_id = surface_id`, minted per call beside
                    // `action.tool.proposed` (§5d.3 §7).
                    self.exposure_emit_activation(sink, &mc, &c.surface_id, &c.tool_call_id)?;
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
        // R2.8 — the `call` retention boundary: reveals whose retention
        // ended with this model call mint `action.tool.surface.evicted`
        // (ADR-0093 D6; `turn`/`run` retentions never end mid-run — the
        // driver models one turn per run, so `TurnEnd` coincides with the
        // projections' `RunEnd` fold).
        self.exposure_expire(sink, tool_exposure::RevealBoundary::CallEnd, &mc)?;
        // The completed/failed row then the cue.
        match outcome.stop_reason {
            hh_gateway::vocab::StopReason::Error | hh_gateway::vocab::StopReason::Unknown => {
                let class = outcome
                    .error_class
                    .clone()
                    .unwrap_or_else(|| "unknown".into());
                if self.routing_port.is_some() {
                    // R-2.7 — the ADR-0122 consult decides retry-vs-reroute
                    // off the sealed `error_actions` table; the row set is
                    // `attempt.failed → call.failed → [side rows] →
                    // [route.decided] → [compact] → retry.scheduled`.
                    self.routed_failure(sink, &mc, attempt, &outcome, &class)?;
                } else {
                    let mut failed_members = vec![
                        ("model_call_id", Json::str(&mc)),
                        ("attempt_no", Json::Int(attempt as i64)),
                        ("error", Json::obj([("class", Json::str(&class))])),
                    ];
                    if let Some(ms) = outcome.retry_after_ms {
                        failed_members.push(("retry_after_ms", Json::Int(ms as i64)));
                    }
                    failed_members.push(("at_ms", self.stamp_ms(sink)));
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
                            self.inbox.push_back(Cue::EnvelopeSignal(
                                EnvelopeSignal::RetryableError {
                                    class,
                                    attempt: s.attempt_no - 1,
                                },
                            ));
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
                // §5b.4 — a served K5 hit stamps `{served_from_cache:
                // entry_ref, timing: n/a{not_run}}` on the terminal row;
                // the recorded artifacts replayed verbatim (never the
                // entry's timing re-read as the serve's).
                if let Some(entry) = &served {
                    if let Json::Obj(stamp) =
                        hh_context::k5::K5Cache::served_terminal_stamp(&entry.entry_ref)
                    {
                        for (k, v) in stamp {
                            // The stamp's declared member set is closed
                            // (`served_from_cache`/`timing`) — named
                            // statically so `completed_members` keeps its
                            // `&'static` member spelling.
                            let key: &'static str = match k.as_str() {
                                "served_from_cache" => "served_from_cache",
                                "timing" => "timing",
                                _ => continue,
                            };
                            completed_members.push((key, v));
                        }
                    }
                }
                // R-2.7 — the attempt terminal closes ahead of the call's
                // (`attempt.completed → call.completed`; a served hit ran
                // no attempt, so no attempt row lands).
                if self.routing_port.is_some() && served.is_none() {
                    self.append(
                        sink,
                        "model.call.attempt.completed",
                        hh_gateway::events::attempt_completed(&mc, attempt as u32, 0, None),
                        Some(&mc),
                    )?;
                }
                // R2.14 — the M3 close boundary's member stamp.
                completed_members.push(("at_ms", self.stamp_ms(sink)));
                self.append(
                    sink,
                    "model.call.completed",
                    Json::obj(completed_members),
                    Some(&mc),
                )?;
                // R2.15 (ADR-0316) — the `belief_probe` ProfileRule firing:
                // the step's declared belief fields ride the *recorded*
                // model_io (`calls[].args_raw` on the completed row), so
                // `requires_observability = model_io` holds exactly when
                // the parsed calls are ledgered — which they are here.
                self.emit_belief_probes(sink, &mc, &outcome)?;
                // K5 — a completed call writes its served artifacts under
                // the key (idempotent — the canonical key fixes the value;
                // a served hit never re-records).
                if served.is_none() {
                    if let (Some(port), Some((plan_hash, _))) = (self.cache_port.as_mut(), &k5) {
                        port.record(
                            plan_hash,
                            Self::outcome_record(&outcome),
                            Json::obj([]),
                            Json::obj([]),
                        );
                    }
                }
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

    /// R2.15 (ADR-0316) — the `belief_probe` emitter: fire each decoded
    /// deterministic probe rule over the step's recorded `calls[].args_raw`
    /// belief fields and append one `verification.belief.probe` row per
    /// elicitation (`provisional` is constitutive on the record;
    /// `charged_to = subject` — the elicitation rides the model's own
    /// output). `judged` probe rules are skipped here — their comparison
    /// needs the critic leg (DF-S1.21-3); a run without probe rules mints
    /// nothing (the class stays ablatable).
    fn emit_belief_probes(
        &mut self,
        sink: &mut dyn LedgerSink,
        mc: &str,
        outcome: &ModelOutcome,
    ) -> Result<(), DriverError> {
        use hh_verification::belief_probe as probes;
        if self
            .belief_probe_rules
            .iter()
            .all(|r| r.detector != "deterministic")
        {
            return Ok(());
        }
        // Rules are cloned up front so `self` is free for the appends.
        let rules: Vec<probes::ProbeRule> = self
            .belief_probe_rules
            .iter()
            .filter(|r| r.detector == "deterministic")
            .cloned()
            .collect();
        let mut index = 0usize;
        for call in &outcome.calls {
            let Ok(args) = hh_wire::json::parse(&call.args_raw) else {
                continue;
            };
            for rule in &rules {
                for item in probes::elicit(&args, &rule.field) {
                    let observed =
                        Self::resolve_belief_subject(sink.prefix(), &item.subject, &item.member);
                    let rec = probes::fire(rule, mc, index, &item, observed);
                    index += 1;
                    self.append(sink, probes::BELIEF_PROBE_CLASS, rec.to_json(), Some(mc))?;
                }
            }
        }
        Ok(())
    }

    /// Resolve a probe `subject` spelling against the durable prefix —
    /// `effect:<id>` → the latest `action.effect.observed` row scoped to
    /// the effect; `call:<tool_call_id>` → the same, two hops through the
    /// call's `intended`/`committed` `intent.tool_call_id` join (the model
    /// knows the call id it minted, never the kernel's `ef-N`); `verdict:
    /// <id>` → the latest `verification.validator.verdict` row naming the
    /// id. The resolved handle is the durable *row* (`row:<event_id>` —
    /// kernel-authoritative, never model text, F7); the compared value is
    /// `payload[member]`. Unknown spellings, unmatched rows and absent
    /// members all resolve `None` — an unresolved handle is no divergence
    /// (the record's `observed_ref` stays absent rather than fabricating a
    /// comparison).
    fn resolve_belief_subject(
        prefix: &[EventEnvelope],
        subject: &str,
        member: &str,
    ) -> Option<(Json, String)> {
        let observed_for = |effect_id: &str| {
            prefix.iter().rev().find(|e| {
                e.class == "action.effect.observed"
                    && e.scope.effect_id.as_deref() == Some(effect_id)
            })
        };
        let row = if let Some(id) = subject.strip_prefix("effect:") {
            observed_for(id)
        } else if let Some(id) = subject.strip_prefix("call:") {
            let effect_id = prefix
                .iter()
                .rev()
                .find(|e| {
                    matches!(
                        e.class.as_str(),
                        "action.effect.intended" | "action.effect.committed"
                    ) && e
                        .payload
                        .get("intent")
                        .and_then(|i| i.get("tool_call_id"))
                        .and_then(Json::as_str)
                        == Some(id)
                })
                .and_then(|e| {
                    e.scope.effect_id.clone().or_else(|| {
                        e.payload
                            .get("effect_id")
                            .and_then(Json::as_str)
                            .map(str::to_string)
                    })
                })?;
            observed_for(&effect_id)
        } else if let Some(id) = subject.strip_prefix("verdict:") {
            prefix.iter().rev().find(|e| {
                e.class == "verification.validator.verdict"
                    && e.payload.get("verdict_id").and_then(Json::as_str) == Some(id)
            })
        } else {
            None
        }?;
        let value = row.payload.get(member)?.clone();
        Some((value, format!("row:{}", row.event_id)))
    }

    /// The routed lane's failure consult (R-2.7; ADR-0122 d.1–d.3): the
    /// consult runs before the attempt terminal lands so
    /// `attempt.failed{will_retry, next_delay_ms}` carries the policy's
    /// answer; then `model.call.failed` closes the scope, the consult's
    /// side rows drain (reservation release/hold, `model.profile.status.
    /// changed`), a reroute mints its fresh `model.route.decided`, the
    /// `compact_then_retry` leg runs its compaction, and the F2
    /// `control.retry.scheduled` + `RetryableError` cue hand the strategy
    /// the re-drive (the envelope's retry bound still governs). `GiveUp`
    /// mints the refused audit row + the `Refused` cue — the honest
    /// terminal is the strategy's `stop{refused}`.
    fn routed_failure(
        &mut self,
        sink: &mut dyn LedgerSink,
        mc: &str,
        attempt: u64,
        outcome: &ModelOutcome,
        class: &str,
    ) -> Result<(), DriverError> {
        let err_class = hh_gateway::vocab::ModelErrorClass::parse(class)
            .unwrap_or(hh_gateway::vocab::ModelErrorClass::Unknown);
        let err = hh_gateway::vocab::ModelError::new(
            err_class,
            format!("model call {mc} attempt {attempt} failed: {class}"),
        );
        let lane = self
            .routing_port
            .as_ref()
            .expect("routed_failure on a routed lane")
            .lane()
            .clone();
        let disposition = {
            let prior = route_decisions(sink.prefix(), mc)
                .last()
                .map(|(_, d)| d.clone());
            let Some(prior) = prior else {
                // No decision exists for the call (unreachable under the
                // select-first arm — never mint a consult against a
                // missing decision; the call closes `policy_give_up`).
                self.append(
                    sink,
                    "model.call.attempt.failed",
                    hh_gateway::events::attempt_failed(mc, attempt as u32, &err, false, None),
                    Some(mc),
                )?;
                let mut failed_members = vec![
                    ("model_call_id", Json::str(mc)),
                    ("attempt_no", Json::Int(attempt as i64)),
                    ("error", Json::obj([("class", Json::str(class))])),
                ];
                if let Some(ms) = outcome.retry_after_ms {
                    failed_members.push(("retry_after_ms", Json::Int(ms as i64)));
                }
                failed_members.push(("at_ms", self.stamp_ms(sink)));
                self.append(
                    sink,
                    "model.call.failed",
                    Json::obj(failed_members),
                    Some(mc),
                )?;
                self.inbox
                    .push_back(Cue::EnvelopeSignal(EnvelopeSignal::Refused {
                        decision_ref: mc.to_string(),
                        reason: "route_attempt:no_decision".to_string(),
                    }));
                return Ok(());
            };
            let mut state = attempt_state(sink.prefix(), mc);
            let req = self.routing_request(&lane, mc, None);
            let decision_id = self.alloc("d");
            let port = self.routing_port.as_mut().expect("routed lane");
            port.attempt_failed(
                &req,
                &prior,
                &err_class,
                attempts_on_target(sink.prefix(), mc),
                outcome.retry_after_ms,
                &mut state,
                &decision_id,
                self.now_ms,
                sink.prefix(),
            )
        };
        let (will_retry, next_delay_ms) = match &disposition {
            hh_gateway::router::AttemptDisposition::Continue { not_before_ms, .. } => {
                (true, Some(*not_before_ms))
            }
            hh_gateway::router::AttemptDisposition::Reroute(_) => (true, None),
            hh_gateway::router::AttemptDisposition::GiveUp(_) => (false, None),
        };
        self.append(
            sink,
            "model.call.attempt.failed",
            hh_gateway::events::attempt_failed(mc, attempt as u32, &err, will_retry, next_delay_ms),
            Some(mc),
        )?;
        let mut failed_members = vec![
            ("model_call_id", Json::str(mc)),
            ("attempt_no", Json::Int(attempt as i64)),
            ("error", Json::obj([("class", Json::str(class))])),
        ];
        if let Some(ms) = outcome.retry_after_ms {
            failed_members.push(("retry_after_ms", Json::Int(ms as i64)));
        }
        failed_members.push(("at_ms", self.stamp_ms(sink)));
        self.append(
            sink,
            "model.call.failed",
            Json::obj(failed_members),
            Some(mc),
        )?;
        // The consult's side rows land durable in emit order (CC3 — the
        // producer computed them; the fenced writer lands them).
        for (class, payload) in self.routing_port.as_mut().expect("routed lane").take_rows() {
            self.append(sink, &class, payload, None)?;
        }
        match disposition {
            hh_gateway::router::AttemptDisposition::Continue { compact_first, .. } => {
                // `compact_then_retry` — the compaction runs under the old
                // profile before the retry (ADR-0122 d.3 ordering).
                if compact_first {
                    self.compact_round(sink, "model_retry", Some(mc))?;
                }
            }
            hh_gateway::router::AttemptDisposition::Reroute(d) => {
                // The fresh decision is durable before the reroute
                // consumes it — the Retry arm folds `pending_reroute`.
                self.append(
                    sink,
                    "model.route.decided",
                    hh_gateway::events::route_decided(&d),
                    None,
                )?;
            }
            hh_gateway::router::AttemptDisposition::GiveUp(reason) => {
                let mut fired =
                    match crate::events::guard_fired_payload(DecisionPoint::Retry, "route_attempt")
                    {
                        Json::Obj(m) => m,
                        other => unreachable!("guard_fired_payload is an object: {other:?}"),
                    };
                fired.insert("verdict".into(), Json::str("refused"));
                fired.insert("required".into(), Json::str(reason.as_str()));
                fired.insert("detail".into(), Json::str(format!("{reason:?}")));
                fired.insert("model_call_id".into(), Json::str(mc));
                self.append(sink, "control.guard.fired", Json::Obj(fired), None)?;
                self.inbox
                    .push_back(Cue::EnvelopeSignal(EnvelopeSignal::Refused {
                        decision_ref: mc.to_string(),
                        reason: format!("route_attempt:{}", reason.as_str()),
                    }));
                return Ok(());
            }
        }
        // The F2 seam — `schedule_retry` decides `retry` vs `give_up` (the
        // envelope bound still governs a router-admitted retry).
        let retries_used = crate::views::fold_envelope_view(sink.prefix()).retries_scheduled;
        match self.envelope.schedule_retry(
            sink.prefix(),
            ScopeKind::ModelCall,
            mc,
            class,
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
                        mc,
                        s.attempt_no,
                        class,
                        s.delay_ms,
                        s.not_before,
                        &self.envelope.policy.policy_id,
                        s.attempt_delta.as_ref(),
                    ),
                    Some(mc),
                )?;
                self.inbox
                    .push_back(Cue::EnvelopeSignal(EnvelopeSignal::RetryableError {
                        class: class.to_string(),
                        attempt: s.attempt_no - 1,
                    }));
            }
            crate::retry::RetryOutcome::GiveUp(_)
            | crate::retry::RetryOutcome::BudgetExhausted { .. } => {
                self.inbox
                    .push_back(Cue::EnvelopeSignal(EnvelopeSignal::Refused {
                        decision_ref: mc.to_string(),
                        reason: "retry_budget_exhausted".into(),
                    }));
            }
        }
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
                    // R2.14 — the effect open's member stamp (M9's
                    // intended boundary; the phase profile's linked
                    // tool-block open).
                    ("at_ms", self.stamp_ms(sink)),
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
                // R2.8 — the kernel `discover_surfaces` capability
                // executes in-kernel (never the effect gate): the driver
                // runs `discover` over the armed catalog, mints
                // `discovery.searched`, reveals the hits (`surface.
                // revealed` per hit) and settles `observed` with the hit
                // list as the tool result.
                let out = match self.exposure_dispatch(sink, &ef, dispatch_intent)? {
                    Some(out) => out,
                    None => gate.dispatch(&ef, 1, dispatch_intent),
                };
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
                    // R2.14 — the effect terminal's member stamp (the
                    // phase profile's linked tool-block close).
                    m.insert("at_ms".to_string(), self.stamp_ms(sink));
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

    // ── R-2.7 routed-lane consults ────────────────────────────────────────

    /// The `RoutingRequest` the lane's consults run under.
    fn routing_request(
        &self,
        lane: &RoutingLane,
        mc: &str,
        source_profile_ref: Option<String>,
    ) -> hh_gateway::router::RoutingRequest {
        hh_gateway::router::RoutingRequest {
            role: lane.role.clone(),
            required_capabilities: lane.required_capabilities.clone(),
            budget_id: lane.budget_id.clone(),
            holder: self.run_id.clone(),
            model_call_id: Some(mc.to_string()),
            intent_ref: lane.intent_ref.clone(),
            effort: lane.effort.clone(),
            latency_target_ms: lane.latency_target_ms,
            quality_prior_ref: None,
            preferences: None,
            source_profile_ref,
            // G-3's honest fold — a proposed-but-unresolved effect in the
            // durable fold parks the cross-profile move (ADR-0030).
            in_flight_effect: !self.state.open_effects.is_empty(),
            task_class: lane.task_class.clone(),
        }
    }

    /// The §5b.2 select for a fresh logical call — mints the durable
    /// `model.route.decided` (scope-free; the call scope opens with
    /// `model.call.requested`), or on refusal the `control.guard.fired`
    /// audit row + the `Refused` envelope cue the strategy answers
    /// `stop{refused}` with. Returns `false` when the call never opened.
    fn route_select(&mut self, sink: &mut dyn LedgerSink, mc: &str) -> Result<bool, DriverError> {
        let lane = match self.routing_port.as_ref() {
            Some(p) => p.lane().clone(),
            None => return Ok(true),
        };
        let req = self.routing_request(&lane, mc, None);
        let decision_id = self.alloc("d");
        let port = self.routing_port.as_mut().expect("lane checked");
        match port.select(
            &req,
            &decision_id,
            self.now_ms,
            &Default::default(),
            sink.prefix(),
        ) {
            Ok(d) => {
                for (class, payload) in port.take_rows() {
                    self.append(sink, &class, payload, None)?;
                }
                self.append(
                    sink,
                    "model.route.decided",
                    hh_gateway::events::route_decided(&d),
                    None,
                )?;
                Ok(true)
            }
            Err(r) => {
                for (class, payload) in port.take_rows() {
                    self.append(sink, &class, payload, None)?;
                }
                let mut fired =
                    match crate::events::guard_fired_payload(DecisionPoint::Plan, "route_select") {
                        Json::Obj(m) => m,
                        other => unreachable!("guard_fired_payload is an object: {other:?}"),
                    };
                fired.insert("verdict".into(), Json::str("refused"));
                fired.insert("required".into(), Json::str(r.as_str()));
                fired.insert("detail".into(), Json::str(r.to_string()));
                fired.insert("model_call_id".into(), Json::str(mc));
                self.append(sink, "control.guard.fired", Json::Obj(fired), None)?;
                self.inbox
                    .push_back(Cue::EnvelopeSignal(EnvelopeSignal::Refused {
                        decision_ref: mc.to_string(),
                        reason: format!("route_select:{}", r.as_str()),
                    }));
                Ok(false)
            }
        }
    }

    /// The K5 lookup between `model.call.requested` and the attempt —
    /// `Some((plan_hash, served))` when the lane bound a `response_cache`;
    /// the `model.cache.resolved` row lands scoped to the call, one per
    /// lookup (ADR-0128 d.3). `served` is the recorded response document
    /// on a `hit` — the caller replays it verbatim and stamps
    /// `served_from_cache` + `timing = n/a{not_run}` on the terminal row.
    fn k5_resolve(
        &mut self,
        sink: &mut dyn LedgerSink,
        mc: &str,
        request: &Json,
    ) -> Result<Option<(String, Option<ServedEntry>)>, DriverError> {
        let Some(port) = self.cache_port.as_mut() else {
            return Ok(None);
        };
        let plan_hash = hh_identity::idp_id(
            &port.binding().plan_domain,
            request.to_canonical_string().as_bytes(),
        );
        let res = port.resolve(&plan_hash);
        let mut payload = res.payload;
        if let Json::Obj(m) = &mut payload {
            m.insert("model_call_id".to_string(), Json::str(mc));
        }
        self.append(sink, "model.cache.resolved", payload, Some(mc))?;
        Ok(Some((plan_hash, res.serve)))
    }

    /// Decode a served entry's recorded response document back into the
    /// `ModelOutcome` it records (the scripted lane's symmetric codec —
    /// `record`/`served` are one spelling).
    fn served_outcome(doc: &Json) -> Option<ModelOutcome> {
        let mut calls = Vec::new();
        if let Some(Json::Arr(items)) = doc.get("calls") {
            for c in items {
                calls.push(ParsedCall {
                    tool_call_id: c.get("tool_call_id")?.as_str()?.to_string(),
                    surface: c.get("surface")?.as_str()?.to_string(),
                    args_raw: c.get("args_raw")?.as_str()?.to_string(),
                });
            }
        }
        Some(ModelOutcome {
            stop_reason: hh_gateway::vocab::StopReason::parse(doc.get("stop_reason")?.as_str()?)?,
            response_ref: doc
                .get("response_ref")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
            text_empty: matches!(doc.get("text_empty"), Some(Json::Bool(true))),
            calls,
            error_class: doc
                .get("error_class")
                .and_then(Json::as_str)
                .map(str::to_string),
            retry_after_ms: doc
                .get("retry_after_ms")
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64),
        })
    }

    /// The canonical record a completed call writes into the cache — the
    /// scripted lane's `ModelOutcome` document (the `served_outcome`
    /// decoder's input spelling).
    fn outcome_record(outcome: &ModelOutcome) -> Json {
        let mut m = vec![
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
            m.push(("retry_after_ms", Json::Int(ms as i64)));
        }
        Json::obj(m)
    }

    /// `compact{reason}` — the compaction boundary: the port runs the
    /// I-FALLBACK ladder (its own `context.compaction.started/completed`
    /// rows ride its sink); `CompactionImpossible` or an absent port is the
    /// exhausted ladder → envelope-owned `stop{context_exhausted}`
    /// (CF-225; `stop_pending` — `run` mints the decision row).
    /// `for_call` stamps the `model_call_id` payload member the
    /// `compact_then_retry` consult's `compact_tried` fold reads (R-2.7 —
    /// the compaction that ran *for* a failed call is a ledger fact, never
    /// an inferred span).
    fn compact_round(
        &mut self,
        sink: &mut dyn LedgerSink,
        reason: &str,
        for_call: Option<&str>,
    ) -> Result<(), DriverError> {
        // R2.14 — the compaction window's measured open (one clock for
        // both stamps — `t_close` lands beside the emitted rows below).
        // Taken before the port borrow: the producing clock is the
        // sink's when it carries one, the tick clock otherwise.
        let comp_open = self.clock_ms(sink);
        match &mut self.compaction_port {
            Some(port) => {
                match port.compact(reason) {
                    Ok(done) => {
                        let t_close = self.clock_ms(sink);
                        // The port's `context.compaction.started`/`completed`
                        // rows land durable in emit order, then the cue —
                        // durable-before-visible, same convention as the
                        // assembler's side bands (DF-S2.8-1: the rows are
                        // minted from the loop, not a test-side stub).
                        for (class, payload) in done.emitted {
                            let payload = match (for_call, payload) {
                                (Some(mc), Json::Obj(mut m)) => {
                                    m.insert("model_call_id".to_string(), Json::str(mc));
                                    Json::Obj(m)
                                }
                                (_, p) => p,
                            };
                            // R2.14 (M6) — the runtime measures the port's
                            // `compact` call on its own clock: `started`
                            // carries `at_ms{comp_open}`, `completed`
                            // carries `at_ms{close}` and the measured
                            // `duration_ms{value, measured_at}` (§2.6's
                            // stamped shape; the port's pure `duration_ms`
                            // placeholder is replaced, never left as a
                            // fabricated zero).
                            let payload = match (class.as_str(), payload) {
                                ("context.compaction.started", Json::Obj(mut m)) => {
                                    m.insert(
                                        "at_ms".to_string(),
                                        Json::obj([
                                            ("value", Json::Int(comp_open as i64)),
                                            ("measured_at", Json::str("runtime")),
                                        ]),
                                    );
                                    Json::Obj(m)
                                }
                                ("context.compaction.completed", Json::Obj(mut m)) => {
                                    m.insert(
                                        "at_ms".to_string(),
                                        Json::obj([
                                            ("value", Json::Int(t_close as i64)),
                                            ("measured_at", Json::str("runtime")),
                                        ]),
                                    );
                                    m.insert(
                                        "duration_ms".to_string(),
                                        Json::obj([
                                            (
                                                "value",
                                                Json::Int(t_close.saturating_sub(comp_open) as i64),
                                            ),
                                            ("measured_at", Json::str("runtime")),
                                        ]),
                                    );
                                    Json::Obj(m)
                                }
                                (_, p) => p,
                            };
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
                }
            }
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
        let mut turn_finished = crate::events::turn_finished_payload(&final_reason);
        if let Json::Obj(m) = &mut turn_finished {
            // R2.14 (§5h.1 §2.2) — M2's measured close: `turn_e2e_ms`
            // reads the `started_at_ms` member the opener recorded on
            // the same clock (a resume-path run without the stamp
            // reports the typed `n/a`, never a guessed span).
            m.insert(
                "turn_e2e_ms".to_string(),
                self.duration_stamp_ms(sink, "lifecycle.turn.started", "started_at_ms"),
            );
        }
        self.append(sink, "lifecycle.turn.finished", turn_finished, None)?;
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
        // R2.14 — M1's `wall_ms` on the same producing clock as the
        // turn's recorded open (the C0 runtime is single-turn — the
        // turn-1 `started_at_ms` member is the run's measured open;
        // an unstamped prefix reports `n/a{observability}`).
        if let Json::Obj(m) = &mut finished {
            m.insert(
                "wall_ms".to_string(),
                self.duration_stamp_ms(sink, "lifecycle.turn.started", "started_at_ms"),
            );
        }
        // R2.15 (DF-S1.21-2) — computed metrics emit on the live run: the
        // deterministic per-run fold (`evalfold::compute` over the durable
        // prefix — claim/reconciled/gate/decided rows all landed by now)
        // appends one `measurement.metric.emitted` per registered ADR-0114
        // D1 metric in the typed `MetricValue` form, `n/a{reason}`
        // first-class (AC-R-2.7.2a-7 — never a silent zero).
        // `detector = deterministic`, `oracle_ref = oracle/evalfold`,
        // `applies_to` the run; the rows precede `run.finished` so the
        // finished row stays the run's terminal.
        self.emit_computed_metrics(sink)?;
        self.append(sink, "lifecycle.run.finished", finished, None)?;
        Ok(FinishOutcome::Done(RunResult {
            report,
            drain,
            decision_events: self.decision_events.clone(),
        }))
    }

    /// R2.15 (DF-S1.21-2) — the live-run computed-metric emission: fold the
    /// durable prefix through `hh_verification::evalfold::compute` and append
    /// one `measurement.metric.emitted` per registered metric (the typed
    /// `hh_ontology::eval::MetricValue` — `n/a{reason}` is a recorded value,
    /// not an omission). `Profile` folds render as `vector` values; the
    /// evalfold's free-form NA reasons map onto the closed `NaReason` sum
    /// (`observability`/`no_evidence` → `observability` — the evidence
    /// observability the metric required was absent on this run).
    fn emit_computed_metrics(&mut self, sink: &mut dyn LedgerSink) -> Result<(), DriverError> {
        use hh_ontology::compliance::NaReason;
        use hh_ontology::eval::{MetricValue, MetricValueKind};
        use hh_verification::evalfold;
        let rows: Vec<hh_verification::bind::RowView> = sink
            .prefix()
            .iter()
            .map(|e| hh_verification::bind::RowView {
                seq: e.seq,
                class: e.class.as_str(),
                payload: &e.payload,
                authority: e
                    .provenance
                    .as_ref()
                    .map(|p| p.authority)
                    .unwrap_or_else(|| {
                        if e.producer.component_class == hh_ledger::event::KERNEL_COMPONENT {
                            hh_provenance::authority::AuthorityClass::Kernel
                        } else {
                            hh_provenance::authority::AuthorityClass::Unverified
                        }
                    }),
                scope_effect_id: e.scope.effect_id.as_deref(),
            })
            .collect();
        let run_ref = format!("run:{}", self.ledger_run_id);
        for m in evalfold::compute(&rows) {
            let value = match &m.value {
                evalfold::MetricValue::Ppm(p) => MetricValueKind::Decimal(*p),
                evalfold::MetricValue::Count(c) => MetricValueKind::Decimal(*c as i64),
                evalfold::MetricValue::Profile(prof) => {
                    let mut m = std::collections::BTreeMap::new();
                    for (k, v) in prof {
                        m.insert(k.clone(), Json::Int(*v));
                    }
                    MetricValueKind::Vector(Json::Obj(m))
                }
                evalfold::MetricValue::NA(reason) => {
                    MetricValueKind::Na(NaReason::parse(reason).unwrap_or(NaReason::Observability))
                }
            };
            let emitted = MetricValue {
                metric_ref: m.name.clone(),
                value,
                applies_to: run_ref.clone(),
                oracle_ref: "oracle/evalfold".to_string(),
                detector: hh_ontology::compliance::Detector::Deterministic,
                confidence: None,
                evidence_ref: None,
                calibration_ref: None,
                exploratory: None,
            };
            self.append(sink, "measurement.metric.emitted", emitted.to_json(), None)?;
        }
        Ok(())
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
                // The §5d.3 exposure classes are call-scoped rows — the
                // `surface_rejected` dual (underscore spelling) predates
                // R2.8 and keeps its unscoped shape (a landed row's scope
                // set is append-only).
                model_call_id: scope_id
                    .filter(|_| {
                        class.starts_with("model.")
                            || class == "action.tool.proposed"
                            || class == "action.tool.call.refused"
                            || class.starts_with("action.tool.exposure.")
                            || class.starts_with("action.tool.discovery.")
                            || class.starts_with("action.tool.surface.")
                            || class.starts_with("action.tool.catalog.")
                    })
                    .map(String::from),
                tool_call_id: if matches!(
                    class,
                    "action.tool.proposed"
                        | "action.tool.call.refused"
                        | "action.tool.discovery.searched"
                ) {
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
                // The §5d.3 exposure classes are call-scoped rows — the
                // `surface_rejected` dual (underscore spelling) predates
                // R2.8 and keeps its unscoped shape (a landed row's scope
                // set is append-only).
                model_call_id: scope_id
                    .filter(|_| {
                        class.starts_with("model.")
                            || class == "action.tool.proposed"
                            || class == "action.tool.call.refused"
                            || class.starts_with("action.tool.exposure.")
                            || class.starts_with("action.tool.discovery.")
                            || class.starts_with("action.tool.surface.")
                            || class.starts_with("action.tool.catalog.")
                    })
                    .map(String::from),
                tool_call_id: if matches!(
                    class,
                    "action.tool.proposed"
                        | "action.tool.call.refused"
                        | "action.tool.discovery.searched"
                ) {
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
                // `measurement.metric.emitted` is the same case — the
                // computed fold emits at `finish`, after the turn scope
                // closed (the class is registered unscoped; `applies_to`
                // carries the run ref, not the scope set).
                turn_id: if class.starts_with("lifecycle.run.")
                    || class == "measurement.metric.emitted"
                {
                    None
                } else {
                    Some("turn-1".into())
                },
                effect_id: scope_id
                    .filter(|_| class.starts_with("action.effect."))
                    .map(String::from),
                // The §5d.3 exposure classes are call-scoped rows — the
                // `surface_rejected` dual (underscore spelling) predates
                // R2.8 and keeps its unscoped shape (a landed row's scope
                // set is append-only).
                model_call_id: scope_id
                    .filter(|_| {
                        class.starts_with("model.")
                            || class == "action.tool.proposed"
                            || class == "action.tool.call.refused"
                            || class.starts_with("action.tool.exposure.")
                            || class.starts_with("action.tool.discovery.")
                            || class.starts_with("action.tool.surface.")
                            || class.starts_with("action.tool.catalog.")
                    })
                    .map(String::from),
                tool_call_id: if matches!(
                    class,
                    "action.tool.proposed"
                        | "action.tool.call.refused"
                        | "action.tool.discovery.searched"
                ) {
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

    /// The producing clock for stamped `*_ms` members — the sink's live
    /// clock when the boundary carries one (§5h.1 §2.6), the driver's
    /// deterministic tick clock otherwise. One source per stamp pair —
    /// a `turn_e2e_ms`/`wall_ms` reads the opener's recorded member off
    /// the same clock (mixed-source arithmetic is the skew the flag
    /// exists for, never a hidden subtraction).
    fn clock_ms(&self, sink: &dyn LedgerSink) -> u64 {
        sink.now_ms().unwrap_or(self.now_ms)
    }

    /// A `{value, measured_at: "runtime"}` member stamp on the producing
    /// clock — the §2.6 shape the folds' `measured_value` reads.
    fn stamp_ms(&self, sink: &dyn LedgerSink) -> Json {
        Json::obj([
            ("value", Json::Int(self.clock_ms(sink) as i64)),
            ("measured_at", Json::str("runtime")),
        ])
    }

    /// `now − <open class>.<member>.value` as a `{value, measured_at}`
    /// duration — the opener's recorded member is the base, never a `ts`
    /// difference. An unstamped opener (a pre-R2.14 row, a resume path)
    /// yields the typed `{"na": "observability"}` — the duration is
    /// honestly unmeasurable, never a fabricated zero.
    fn duration_stamp_ms(&self, sink: &dyn LedgerSink, open_class: &str, member: &str) -> Json {
        let open = sink.prefix().iter().find(|e| {
            e.class == open_class
                && e.payload
                    .get(member)
                    .and_then(|v| v.get("value"))
                    .and_then(Json::as_int)
                    .is_some()
        });
        match open {
            Some(e) => {
                let base = e
                    .payload
                    .get(member)
                    .and_then(|v| v.get("value"))
                    .and_then(Json::as_int)
                    .unwrap_or(0);
                Json::obj([
                    (
                        "value",
                        Json::Int((self.clock_ms(sink) as i64).saturating_sub(base)),
                    ),
                    ("measured_at", Json::str("runtime")),
                ])
            }
            None => Json::obj([("na", Json::str("observability"))]),
        }
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

// ── R2.8 — the exposure-aware turn loop (§5d.3; DF-S1.17-3) ─────────────────
//
// The armed lane (`config.exposure = Some`) runs the §5d.3 selection
// contract inside the durable loop:
//
// - per `propose`, `select_surfaces` mints the `ExposurePlan` *before*
//   `assemble` (`DecisionPoint = retrieve`; durable-before-visible — the
//   `action.tool.exposure.planned` row lands ahead of the context it
//   shaped), once-per-run `action.tool.catalog.built` opens the run;
// - the plan's `direct` members are the delivered surface set —
//   G-INTERPRET and `validate` read it (a name outside the plan is
//   `unknown_surface` to the §5e.2 pipeline, and `check_callable`'s
//   `action.tool.call.refused` is the §5d.3 row beside it — ADR-0093 D7);
// - a validated call on the `discover_surfaces` capability executes
//   in-kernel (`discovery.searched` + `surface.revealed` per hit, the
//   hit list is the `action.effect.observed` outcome — the kernel
//   capability, never the effect gate);
// - `retention = call` reveals expire at each call's end
//   (`surface.evicted{cause: policy}`; `turn`/`run` never end mid-run —
//   the driver models a single turn, so `TurnEnd` coincides with the
//   projections' `RunEnd` fold);
// - a delivered `tool_surface` mints `context.artefact.delivered` once
//   per run; a call to it mints `context.artefact.activated{artefact_id =
//   surface_id, detector: deterministic, signal: invoked}` per call
//   (AC-E3-7; T-LCD-13);
// - `sync_exposure_source` is the host's mutable-source entry
//   (`catalog.delta` always, `catalog.epoch` + `catalog.built` on
//   `adopt`, `security.permission.requested` on `ask`).
impl<S: ControlStrategy> Driver<S> {
    /// The delivered surface set for the current call — the armed lane's
    /// `direct`-plan members resolved against the compiled `SurfaceSpec`
    /// table; the disarmed lane answers `config.surfaces` verbatim.
    fn delivered_surfaces(&self) -> Vec<SurfaceSpec> {
        match &self.exposure_state {
            Some(st) => st.delivered_surfaces.clone(),
            None => self.config.surfaces.clone(),
        }
    }

    /// `exposure_plan_call` — the per-`propose` leg: lazy state init (the
    /// durable fold on resume/replay — `fold_exposure_state`), the
    /// once-per-run `catalog.built`, `select_surfaces` →
    /// `action.tool.exposure.planned`, then the first-delivery
    /// `context.artefact.delivered{kind: tool_surface}` rows for the
    /// plan's `direct` members. A `SelectError` propagates as a typed
    /// driver failure — the exposure meet refuses, it never degrades.
    fn exposure_plan_call(
        &mut self,
        sink: &mut dyn LedgerSink,
        mc: &str,
    ) -> Result<(), DriverError> {
        if self.config.exposure.is_none() {
            return Ok(());
        }
        if self.exposure_state.is_none() {
            let run_id = self.run_id.clone();
            let opening = self.config.exposure.as_ref().map(|cfg| cfg.catalog.clone());
            self.exposure_state = Some(fold_exposure_state(sink.prefix(), &run_id).unwrap_or_else(
                || ExposureRunState {
                    catalog: opening.clone().expect("armed"),
                    revealed: tool_exposure::RevealedSet {
                        run_id,
                        entries: vec![],
                    },
                    prior_order: vec![],
                    recent_calls: vec![],
                    plan: None,
                    delivered: std::collections::BTreeMap::new(),
                    delivered_surfaces: vec![],
                    announced: false,
                },
            ));
        }
        // The state comes out owned for the leg's duration — appends
        // borrow `self` freely while `st` is a value (restored before
        // every return).
        let cfg = self.config.exposure.clone().expect("armed");
        let mut st = self.exposure_state.take().expect("armed");
        // The opening `catalog.built` — `catalog.epoch` rows mint only on
        // `adopt` (the opening epoch is the built row's `epoch` member).
        if !st.announced {
            let payload = tool_exposure::catalog_built_payload(&st.catalog);
            st.announced = true;
            self.append(sink, "action.tool.catalog.built", payload, Some(mc))?;
        }
        let turn_state = tool_exposure::TurnState {
            model_call_id: mc.to_string(),
            revealed: st.revealed.clone(),
            recent_calls: st.recent_calls.clone(),
            goal_ref: Some(self.run_id.clone()),
            window_cap: self.config.window_cap_tokens,
            prior_order: st.prior_order.clone(),
        };
        let plan = match tool_exposure::select_surfaces_with(
            &st.catalog,
            &turn_state,
            &cfg.profile_modes,
            &cfg.params,
            cfg.policy.as_deref(),
            &cfg.gates,
        ) {
            Ok(p) => p,
            Err(e) => {
                self.exposure_state = Some(st);
                return Err(DriverError::Port {
                    port: "exposure",
                    detail: e.to_string(),
                });
            }
        };
        let payload = tool_exposure::exposure_planned_payload(&plan, st.plan.as_ref());
        // The delivered `SurfaceSpec` set — `direct` members resolved by
        // the catalog's model-facing `name` against the compiled table.
        let direct: std::collections::BTreeSet<&str> = plan
            .entries
            .iter()
            .filter(|(_, m)| *m == hh_hir::tools::ExposureMode::Direct)
            .map(|(sid, _)| sid.as_str())
            .collect();
        let delivered_specs: Vec<SurfaceSpec> = st
            .catalog
            .entries
            .iter()
            .filter(|e| direct.contains(e.surface_id.as_str()))
            .filter_map(|e| {
                self.config
                    .surfaces
                    .iter()
                    .find(|s| s.surface_id == e.name || s.surface_id == e.surface_id)
                    .cloned()
            })
            .collect();
        let new_deliveries: Vec<(String, String)> = st
            .catalog
            .entries
            .iter()
            .filter(|e| {
                direct.contains(e.surface_id.as_str()) && !st.delivered.contains_key(&e.name)
            })
            .map(|e| (e.name.clone(), e.surface_id.clone()))
            .collect();
        self.append(sink, "action.tool.exposure.planned", payload, Some(mc))?;
        for (name, catalog_sid) in new_deliveries {
            let delivery_id = self.alloc("del");
            let mut payload = match hh_context::events::artefact_delivered_payload(
                &name,
                &delivery_id,
                "tool_surface",
                None,
                false,
            ) {
                Json::Obj(m) => m,
                other => {
                    unreachable!("artefact_delivered_payload is an object: {other:?}")
                }
            };
            // The catalog's identity half — `artefact_id` is the driver-
            // plane `surface_id` (the model-facing name; T-LCD-13's join
            // to the call), `catalog_surface_id` is the idp/1 binding id.
            payload.insert("catalog_surface_id".to_string(), Json::str(catalog_sid));
            self.append(
                sink,
                "context.artefact.delivered",
                Json::Obj(payload),
                Some(mc),
            )?;
            st.delivered.insert(name, delivery_id);
        }
        st.delivered_surfaces = delivered_specs;
        st.prior_order = plan.order.clone();
        st.plan = Some(plan);
        self.exposure_state = Some(st);
        Ok(())
    }

    /// `check_callable` on one parsed call (ADR-0093 D7 — the pre-monitor
    /// gate): a refusal mints `action.tool.call.refused` scoped to the
    /// producing call. Unarmed and plan-less calls mint nothing — the
    /// C0 lane is unchanged.
    fn exposure_check_call(
        &mut self,
        sink: &mut dyn LedgerSink,
        mc: &str,
        call: &ParsedCall,
    ) -> Result<(), DriverError> {
        let refusal = match &self.exposure_state {
            Some(st) => match &st.plan {
                // `plan_of(model_call_id)` — the producing call's plan
                // only (a retried call re-reads its own plan — INV-5's
                // shared identity).
                Some(plan) if plan.model_call_id == mc => tool_exposure::check_callable(
                    plan,
                    &st.catalog,
                    &tool_exposure::CallProposal {
                        surface_name: call.surface.clone(),
                        // `from_code = false` — every call reaching this
                        // seam is a model-emitted call; program-originated
                        // `code_mode` proposals arrive through the
                        // plan-execute lane.
                        from_code: false,
                    },
                ),
                _ => return Ok(()),
            },
            None => return Ok(()),
        };
        if let Err(r) = refusal {
            let mut payload = match tool_exposure::call_refused_payload(&r) {
                Json::Obj(m) => m,
                other => unreachable!("call_refused_payload is an object: {other:?}"),
            };
            payload.insert("tool_call_id".to_string(), Json::str(&call.tool_call_id));
            self.append(
                sink,
                "action.tool.call.refused",
                Json::Obj(payload),
                Some(mc),
            )?;
        }
        Ok(())
    }

    /// AC-E3-7 — a call to a delivered `tool_surface` mints
    /// `context.artefact.activated{artefact_id = surface_id, detector:
    /// deterministic, signal: invoked}` (T-LCD-13; §5d.3 §7). The call
    /// IS the activation evidence — `delivery_id` joins the surface's
    /// `delivered` row; an undelivered surface mints nothing (the
    /// `call.refused` leg already recorded it).
    fn exposure_emit_activation(
        &mut self,
        sink: &mut dyn LedgerSink,
        mc: &str,
        surface_id: &str,
        tool_call_id: &str,
    ) -> Result<(), DriverError> {
        let delivery_id = match self.exposure_state.as_mut() {
            Some(st) => {
                st.recent_calls.push(surface_id.to_string());
                st.recent_calls.truncate(64);
                st.delivered.get(surface_id).cloned()
            }
            None => None,
        };
        let Some(delivery_id) = delivery_id else {
            return Ok(());
        };
        let mut payload = match hh_context::events::artefact_activated_payload(
            surface_id,
            &delivery_id,
            "deterministic",
            "invoked",
        ) {
            Json::Obj(m) => m,
            other => unreachable!("artefact_activated_payload is an object: {other:?}"),
        };
        // `tool_call_id` is the call's join (the evidence ref — the same
        // member `verification.artefact.followed` reads).
        payload.insert("tool_call_id".to_string(), Json::str(tool_call_id));
        self.append(
            sink,
            "context.artefact.activated",
            Json::Obj(payload),
            Some(mc),
        )
    }

    /// The retention boundary fold — `expire_reveals` drops the reveals
    /// whose `retention` ends at `boundary`; each dropped surface mints
    /// `action.tool.surface.evicted{cause: policy}` (ADR-0093 D6 —
    /// expiry is a policy decision the retention record declared; the
    /// compactor's own evictions mint `compaction` at its call sites).
    /// Pinned surfaces never expire (`expire_reveals`' kernel rule).
    fn exposure_expire(
        &mut self,
        sink: &mut dyn LedgerSink,
        boundary: tool_exposure::RevealBoundary,
        mc: &str,
    ) -> Result<(), DriverError> {
        let expired = match self.exposure_state.as_mut() {
            Some(st) => {
                let (next, expired) =
                    tool_exposure::expire_reveals(&st.revealed, &st.catalog, boundary);
                st.revealed = next;
                expired
            }
            None => vec![],
        };
        for sid in expired {
            self.append(
                sink,
                "action.tool.surface.evicted",
                tool_exposure::surface_evicted_payload(&sid, tool_exposure::EvictCause::Policy),
                Some(mc),
            )?;
        }
        Ok(())
    }

    /// The kernel `discover_surfaces` executor (§5d.3 §5; ADR-0094 D4):
    /// when the dispatch intent's surface resolves to the catalog's
    /// `is_discovery` entry the call executes in-kernel — `discover`
    /// over the armed catalog at the query's granularity, the
    /// `action.tool.discovery.searched` row, one
    /// `action.tool.surface.revealed` per hit, and the hit list as the
    /// `observed` outcome. `None` ⇒ not a discovery call — the effect
    /// gate dispatches as usual.
    fn exposure_dispatch(
        &mut self,
        sink: &mut dyn LedgerSink,
        ef: &str,
        intent: &Json,
    ) -> Result<Option<GateOutcome>, DriverError> {
        let surface = intent
            .get("surface_id")
            .or_else(|| intent.get("surface"))
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        let armed = match &self.exposure_state {
            Some(st) => st
                .catalog
                .entries
                .iter()
                .any(|e| e.is_discovery && (e.name == surface || e.surface_id == surface)),
            None => false,
        };
        if !armed {
            return Ok(None);
        }
        let tool_call_id = intent
            .get("tool_call_id")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        let mc = self
            .last_model_call_id
            .clone()
            .unwrap_or_else(|| ef.to_string());
        let refused = |reason: String| -> Option<GateOutcome> {
            Some(GateOutcome {
                outcome: SettledOutcome::Refused,
                submission_ref: None,
                error_class: Some(reason),
                emitted: vec![],
            })
        };
        let args = intent.get("args").cloned().unwrap_or(Json::Null);
        let query = match tool_exposure::discovery_query_from_args(&args) {
            Ok(q) => q,
            Err(e) => return Ok(refused(format!("discovery_query_invalid:{e:?}"))),
        };
        let cfg = self.config.exposure.clone().expect("armed");
        let st = self.exposure_state.take().expect("armed");
        // The serving decl — the bound index at the query's granularity
        // that declares the form (the first matching decl wins; none ⇒
        // the unbound `exact_name` surface-granularity default — the C0
        // floor, §5d.2.8).
        let decl = st
            .catalog
            .indexes
            .iter()
            .find(|d| {
                d.granularity == query.granularity
                    && tool_exposure::form_kinds(&d.index)
                        .contains(&tool_exposure::DiscoveryFormKind::of(&query.form))
            })
            .or_else(|| {
                st.catalog
                    .indexes
                    .iter()
                    .find(|d| d.granularity == query.granularity)
            })
            .cloned()
            .unwrap_or(tool_exposure::IndexDecl {
                index: tool_exposure::CatalogIndex::ExactName,
                granularity: tool_exposure::IndexGranularity::Surface,
            });
        let Some(plan) = st.plan.clone() else {
            // No plan for the producing call — the discovery capability
            // was never `direct` (unreachable through `check_callable`;
            // the honest refusal anyway).
            self.exposure_state = Some(st);
            return Ok(refused("discovery_query_invalid:no_plan".to_string()));
        };
        let catalog = st.catalog.clone();
        self.exposure_state = Some(st);
        let result = match tool_exposure::discover_with(
            &plan,
            &decl,
            &catalog,
            &query,
            &cfg.params,
            &cfg.executor,
        ) {
            Ok(r) => r,
            Err(e) => return Ok(refused(format!("discovery_query_invalid:{e:?}"))),
        };
        let payload =
            tool_exposure::discovery_searched_payload(&tool_call_id, &query, &decl.index, &result);
        self.append(sink, "action.tool.discovery.searched", payload, Some(&mc))?;
        // Reveal the hits — `reveal` is the one entry into the revealed
        // set (I-CLOSED membership is its check).
        let ids: Vec<String> = result.hits.iter().map(|h| h.surface_id.clone()).collect();
        let at_seq = sink.prefix().last().map(|e| e.seq).unwrap_or(0);
        let mut st = self.exposure_state.take().expect("armed");
        let before: std::collections::BTreeSet<String> = st
            .revealed
            .entries
            .iter()
            .map(|e| e.surface_id.clone())
            .collect();
        let next = match tool_exposure::reveal(
            &st.revealed,
            &catalog,
            &ids,
            tool_exposure::RevealCause::Discovery,
            cfg.params.retention,
            at_seq,
        ) {
            Ok(n) => n,
            Err(_) => {
                // `NotInCatalog` is unreachable — `discover` returns
                // catalog members only; the honest refusal anyway (never
                // a fabricated reveal).
                self.exposure_state = Some(st);
                return Ok(refused("reveal_failed:not_in_catalog".to_string()));
            }
        };
        let fresh: Vec<tool_exposure::RevealedEntry> = next
            .entries
            .iter()
            .filter(|e| !before.contains(&e.surface_id))
            .cloned()
            .collect();
        st.revealed = next;
        self.exposure_state = Some(st);
        for e in &fresh {
            let mode_from = plan
                .entries
                .iter()
                .find(|(s, _)| *s == e.surface_id)
                .map(|(_, m)| *m)
                .unwrap_or(hh_hir::tools::ExposureMode::Deferred);
            self.append(
                sink,
                "action.tool.surface.revealed",
                tool_exposure::surface_revealed_payload(e, mode_from),
                Some(&mc),
            )?;
        }
        let outcome = Json::obj([
            (
                "hits",
                Json::Arr(
                    result
                        .hits
                        .iter()
                        .map(|h| Json::str(h.surface_id.clone()))
                        .collect(),
                ),
            ),
            ("truncated", Json::Bool(result.truncated)),
        ]);
        Ok(Some(GateOutcome {
            outcome: SettledOutcome::Observed {
                outcome: outcome.to_canonical_string(),
            },
            submission_ref: None,
            error_class: None,
            emitted: vec![],
        }))
    }

    /// `sync_exposure_source(source_ref, cause, listing, source_state)` —
    /// the host's mutable-source entry (§5d.3 §2; `sync_source`): the
    /// `action.tool.catalog.delta` row always lands (an empty delta is a
    /// synchronization record), then the drift policy's outcome —
    /// `freeze` marks removals `unavailable`, `adopt` mints the
    /// `catalog.epoch` + rebuilt `catalog.built` rows, `ask` mints
    /// `security.permission.requested` and the catalog stays (H7's grant
    /// leg is the principal's answer, never a defaulted adoption).
    /// Returns the adopted epoch when one minted. `Err` on an unarmed
    /// lane — a sync without a catalog is a typed refusal, not a no-op.
    pub fn sync_exposure_source(
        &mut self,
        sink: &mut dyn LedgerSink,
        source_ref: &str,
        cause: tool_exposure::SyncTrigger,
        listing: &[tool_exposure::CatalogEntry],
        source_state: &tool_exposure::SourceState,
    ) -> Result<Option<u64>, DriverError> {
        if self.config.exposure.is_none() {
            return Err(DriverError::UnbackedPort { kind: "exposure" });
        }
        let drift = self
            .config
            .exposure
            .as_ref()
            .expect("armed")
            .params
            .drift_policy;
        let out = match self.exposure_state.as_ref() {
            Some(st) => tool_exposure::sync_source(
                &st.catalog,
                source_ref,
                cause,
                listing,
                source_state,
                drift,
                "kernel",
            ),
            None => {
                return Err(DriverError::Port {
                    port: "exposure",
                    detail: "sync before the first propose".to_string(),
                })
            }
        };
        let mc = self
            .last_model_call_id
            .clone()
            .unwrap_or_else(|| "mc-0".to_string());
        self.append(
            sink,
            "action.tool.catalog.delta",
            out.delta_event.clone(),
            Some(&mc),
        )?;
        match out.outcome {
            Ok(tool_exposure::AdoptOutcome::Frozen { catalog }) => {
                self.exposure_state.as_mut().expect("armed").catalog = catalog;
                Ok(None)
            }
            Ok(tool_exposure::AdoptOutcome::Adopted { catalog, epoch }) => {
                let epoch_no = epoch.epoch;
                if let Some(p) = out.epoch_event {
                    self.append(sink, "action.tool.catalog.epoch", p, Some(&mc))?;
                }
                // `adopt` rebuilds the index — `catalog.built` is the
                // rebuilt catalog's durable row (§5d.3 §2).
                let built = tool_exposure::catalog_built_payload(&catalog);
                self.append(sink, "action.tool.catalog.built", built, Some(&mc))?;
                self.exposure_state.as_mut().expect("armed").catalog = catalog;
                Ok(Some(epoch_no))
            }
            Err(e) => {
                // `ask` (and every drift refusal) mints the permission
                // request — the principal's grant is the adoption leg.
                let request = Json::obj([
                    ("request_id", Json::str(self.alloc("perm"))),
                    ("permission", Json::str("catalog_drift")),
                    ("subject", Json::str(source_ref)),
                    ("reason", Json::str(e.to_string())),
                ]);
                self.append(sink, "security.permission.requested", request, Some(&mc))?;
                Ok(None)
            }
        }
    }
}

/// `fold_exposure_state(prefix, run_id)` — the CC3 projection rebuilding
/// the exposure run state from the durable prefix (resume-by-leaf): the
/// live catalog is the last `catalog.built` row plus any later
/// `catalog.delta` rows (freeze marks `removed` `unavailable`; adopted
/// epochs re-emit `built`, so a delta applies its `removed` marks and its
/// descriptor entries), the revealed set folds
/// `surface.revealed`/`surface.evicted` in order, `delivered` folds the
/// `tool_surface` deliveries, `prior_order` reads the last
/// `exposure.planned`'s order member. `None` when no `catalog.built` is
/// durable — the caller mints the opening state off the config catalog.
fn fold_exposure_state(prefix: &[EventEnvelope], run_id: &str) -> Option<ExposureRunState> {
    let mut catalog: Option<tool_exposure::Catalog> = None;
    let mut revealed = tool_exposure::RevealedSet {
        run_id: run_id.to_string(),
        entries: vec![],
    };
    let mut prior_order: Vec<String> = Vec::new();
    let mut recent_calls: Vec<String> = Vec::new();
    let mut delivered: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for e in prefix {
        match e.class.as_str() {
            "action.tool.catalog.built" => {
                catalog = tool_exposure::catalog_built_from_payload(&e.payload);
            }
            "action.tool.catalog.delta" => {
                if let Some(cat) = &mut catalog {
                    if let Some(Json::Arr(removed)) = e.payload.get("removed") {
                        for r in removed {
                            if let Some(sid) = r.as_str() {
                                if let Some(en) =
                                    cat.entries.iter_mut().find(|en| en.surface_id == sid)
                                {
                                    en.availability = tool_exposure::Availability::Unavailable(
                                        "removed".to_string(),
                                    );
                                }
                            }
                        }
                    }
                    for member in ["added", "changed"] {
                        if let Some(Json::Arr(items)) = e.payload.get(member) {
                            for it in items {
                                if let Some(en) = it
                                    .get("entry")
                                    .and_then(tool_exposure::catalog_entry_from_json)
                                {
                                    match cat
                                        .entries
                                        .iter_mut()
                                        .find(|x| x.surface_id == en.surface_id)
                                    {
                                        Some(slot) => *slot = en,
                                        None => cat.entries.push(en),
                                    }
                                }
                            }
                        }
                    }
                    cat.entries.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
                }
            }
            "action.tool.exposure.planned" => {
                if let Some(Json::Arr(order)) = e.payload.get("order") {
                    prior_order = order
                        .iter()
                        .filter_map(|s| s.as_str().map(str::to_string))
                        .collect();
                }
            }
            "action.tool.surface.revealed" => {
                let p = &e.payload;
                if let Some(sid) = p.get("surface_id").and_then(Json::as_str) {
                    if !revealed.entries.iter().any(|x| x.surface_id == sid) {
                        revealed.entries.push(tool_exposure::RevealedEntry {
                            surface_id: sid.to_string(),
                            mode: p
                                .get("mode_to")
                                .and_then(Json::as_str)
                                .and_then(hh_hir::tools::ExposureMode::parse)
                                .unwrap_or(hh_hir::tools::ExposureMode::Direct),
                            revealed_at: e.seq,
                            cause: p
                                .get("cause")
                                .and_then(Json::as_str)
                                .and_then(tool_exposure::RevealCause::parse)
                                .unwrap_or(tool_exposure::RevealCause::Discovery),
                            retention: p
                                .get("retention")
                                .and_then(Json::as_str)
                                .and_then(tool_exposure::Retention::parse)
                                .unwrap_or(tool_exposure::Retention::Run),
                        });
                    }
                }
            }
            "action.tool.surface.evicted" => {
                if let Some(sid) = e.payload.get("surface_id").and_then(Json::as_str) {
                    revealed.entries.retain(|x| x.surface_id != sid);
                }
            }
            "context.artefact.delivered" => {
                if e.payload.get("kind").and_then(Json::as_str) == Some("tool_surface") {
                    if let (Some(a), Some(d)) = (
                        e.payload.get("artefact_id").and_then(Json::as_str),
                        e.payload.get("delivery_id").and_then(Json::as_str),
                    ) {
                        delivered.insert(a.to_string(), d.to_string());
                    }
                }
            }
            "action.tool.proposed" => {
                if let Some(sid) = e.payload.get("surface_id").and_then(Json::as_str) {
                    recent_calls.push(sid.to_string());
                    recent_calls.truncate(64);
                }
            }
            _ => {}
        }
    }
    catalog.map(|cat| ExposureRunState {
        catalog: cat,
        revealed,
        prior_order,
        recent_calls,
        plan: None,
        delivered,
        delivered_surfaces: vec![],
        announced: true,
    })
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

// ── R-2.7 — the durable-prefix folds the routed lane reads ─────────────────
// (the folds are the cursor — a resume replays them identically; nothing
// routing-shaped lives only in process memory, CC3.)

/// The run's `model.route.decided` decisions for `mc` — `(seq, decision)`
/// pairs in seq order.
fn route_decisions(
    prefix: &[EventEnvelope],
    mc: &str,
) -> Vec<(u64, hh_gateway::router::RoutingDecision)> {
    prefix
        .iter()
        .filter(|e| {
            e.class == "model.route.decided"
                && e.payload.get("model_call_id").and_then(Json::as_str) == Some(mc)
        })
        .filter_map(|e| {
            hh_gateway::router::RoutingDecision::from_json(&e.payload).map(|d| (e.seq, d))
        })
        .collect()
}

/// Attempts already made on the currently-active target — the
/// `model.call.attempt.started` rows for `mc` landed after the active
/// decision's seq (a reroute resets the count — ADR-0122
/// `attempts_on_target` is per-target, not per-call).
fn attempts_on_target(prefix: &[EventEnvelope], mc: &str) -> u32 {
    let since = route_decisions(prefix, mc)
        .last()
        .map(|(s, _)| *s)
        .unwrap_or(0);
    prefix
        .iter()
        .filter(|e| {
            e.class == "model.call.attempt.started"
                && e.seq > since
                && e.payload.get("model_call_id").and_then(Json::as_str) == Some(mc)
        })
        .count() as u32
}

/// The `AttemptState` cursor rebuilt from the durable prefix — `attempted`
/// = every selected `model_ref` the call's decisions bound (the
/// forward-only chain never re-selects a tried target), `reroutes_used`
/// = the call's `model.rerouted` count, `compact_tried` = a
/// `context.compaction.completed` row stamped `model_call_id = mc`.
fn attempt_state(prefix: &[EventEnvelope], mc: &str) -> hh_gateway::router::AttemptState {
    let mut st = hh_gateway::router::AttemptState::default();
    for (_, d) in route_decisions(prefix, mc) {
        st.attempted
            .insert(hh_gateway::router::model_ref_spelling(&d.selected));
        st.attempted.insert(d.selected.provider_model_id.clone());
    }
    st.reroutes_used = prefix
        .iter()
        .filter(|e| {
            e.class == "model.rerouted"
                && e.payload.get("model_call_id").and_then(Json::as_str) == Some(mc)
        })
        .count() as u32;
    st.compact_tried = prefix.iter().any(|e| {
        e.class == "context.compaction.completed"
            && e.payload.get("model_call_id").and_then(Json::as_str) == Some(mc)
    });
    st
}

/// A fresh `model.route.decided` the failure consult minted that the Retry
/// arm has not yet consumed with a `model.rerouted` — `(fresh, prior)` or
/// `None` (a same-target `Continue` leaves the last decision in place).
fn pending_reroute(
    prefix: &[EventEnvelope],
    mc: &str,
) -> Option<(
    hh_gateway::router::RoutingDecision,
    hh_gateway::router::RoutingDecision,
)> {
    let ds = route_decisions(prefix, mc);
    if ds.len() < 2 {
        return None;
    }
    let fresh = ds.last().map(|(_, d)| d.clone())?;
    let prior = ds[ds.len() - 2].1.clone();
    let consumed = prefix.iter().any(|e| {
        e.class == "model.rerouted"
            && e.payload.get("model_call_id").and_then(Json::as_str) == Some(mc)
            && e.payload.get("decision_ref").and_then(Json::as_str)
                == Some(fresh.decision_id.as_str())
    });
    (!consumed).then_some((fresh, prior))
}

/// The `RerouteReason` a pending reroute spells — read off the policy's
/// `error_actions` row for the last failed class: a `reroute` leaf entry
/// is `permanent{class}`; a chain that *landed* on reroute after bounded
/// same-target retries is `transient_exhausted` (§5b.2's closed set).
fn reroute_reason(
    prefix: &[EventEnvelope],
    mc: &str,
    policy: &hh_gateway::router::RoutingPolicy,
) -> hh_gateway::vocab::RerouteReason {
    let class = prefix
        .iter()
        .rev()
        .find(|e| {
            e.class == "model.call.attempt.failed"
                && e.payload.get("model_call_id").and_then(Json::as_str) == Some(mc)
        })
        .and_then(|e| {
            e.payload
                .get("error")
                .and_then(|er| er.get("class"))
                .and_then(Json::as_str)
        })
        .and_then(hh_gateway::vocab::ModelErrorClass::parse);
    match class {
        Some(c) => match hh_gateway::router::error_action(policy, &c) {
            hh_gateway::router::ErrorAction::Reroute => {
                hh_gateway::vocab::RerouteReason::Permanent(c)
            }
            _ => hh_gateway::vocab::RerouteReason::TransientExhausted,
        },
        None => hh_gateway::vocab::RerouteReason::TransientExhausted,
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

    // ── R2.15 — belief-probe emitters (ADR-0316) + live-run metrics ──────

    fn profile_with_belief_probe(detector: &str, debt: bool) -> Json {
        let mut rule = vec![
            ("kind", Json::str("belief_probe")),
            ("rule_id", Json::str("rule/bp-1")),
            (
                "params",
                Json::obj([
                    ("field", Json::str("beliefs")),
                    ("detector", Json::str(detector)),
                ]),
            ),
        ];
        if debt {
            rule.push(("debt", Json::obj([("id", Json::str("debt/bp-1"))])));
        }
        Json::obj([
            ("profile_id", Json::str("profile/test")),
            ("version", Json::str("1")),
            (
                "rules",
                Json::Arr(vec![Json::Obj(
                    rule.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
                )]),
            ),
        ])
    }

    fn probe_ctx(profile: Json) -> ControlContext {
        let mut c = ctx();
        c.profile = profile;
        c
    }

    /// ADR-0316 — a `belief_probe` `ProfileRule` fires on the recorded
    /// `model_io`: the step's elicited beliefs resolve against the durable
    /// prefix and emit `verification.belief.probe` rows (`provisional`
    /// constitutive). A resolved mismatch is `divergent`; an unresolved
    /// handle carries no `observed_ref` — never a fabricated comparison.
    #[test]
    fn belief_probe_rules_emit_rows_over_recorded_model_io() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver = Driver::open_react(
            &probe_ctx(profile_with_belief_probe("deterministic", true)),
            policy,
            &mut sink,
            DriverConfig {
                surfaces: vec![SurfaceSpec {
                    surface_id: "fs.read".into(),
                    semantic_id: "sem/fs.read".into(),
                    params: [
                        (
                            "path".into(),
                            crate::output::ParamSpec {
                                required: true,
                                kind: crate::output::ParamKind::Str,
                                enum_values: vec![],
                                domain: vec![],
                            },
                        ),
                        // The per-step belief-probe field is a *declared*
                        // surface member (profile-owned `ProfileRule`s
                        // compile onto the surface — §5f §2's claim-surface
                        // row); an undeclared member is a schema violation,
                        // never silently elicited.
                        (
                            "beliefs".into(),
                            crate::output::ParamSpec {
                                required: false,
                                kind: crate::output::ParamKind::Arr,
                                enum_values: vec![],
                                domain: vec![],
                            },
                        ),
                    ]
                    .into_iter()
                    .collect(),
                    risk_class: None,
                }],
                ..DriverConfig::default()
            },
        )
        .unwrap();
        // Call 1: a real tool call plus beliefs that *cannot* resolve yet
        // (the probe fires at `model.call.completed`, before `ef-1` is
        // observed — `observed_ref` absent, never a fabricated comparison).
        // Call 2 — after `ef-1`'s `observed{outcome: applied}` landed —
        // asserts a *mismatched* member (divergent, citing the durable
        // row). Call 3 ends the turn.
        let early = r#"{"path":"/a","beliefs":[
            {"subject":"effect:ef-1","member":"outcome","value":"ok"},
            {"subject":"effect:none","member":"outcome","value":"ok"}
        ]}"#;
        let late = r#"{"path":"/b","beliefs":[
            {"subject":"call:tc-1","member":"outcome","value":"error"}
        ]}"#;
        let mut model = ScriptedModel {
            script: [
                outcome(
                    hh_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-1".into(),
                        surface: "fs.read".into(),
                        args_raw: early.into(),
                    }],
                ),
                outcome(
                    hh_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-2".into(),
                        surface: "fs.read".into(),
                        args_raw: late.into(),
                    }],
                ),
                outcome(hh_gateway::vocab::StopReason::EndTurn, vec![]),
            ]
            .into_iter()
            .collect(),
        };
        // `submission_ref: None` — the observed effects never complete the
        // run; the scripted `EndTurn` owns the stop so all three calls
        // reach the probe emitter.
        let mut gate = ScriptedGate {
            out: GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: None,
                error_class: None,
                emitted: Vec::new(),
            },
            finish: None,
        };
        let mut asm = NullAssembler;
        let _ = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        let probes: Vec<&Json> = sink
            .events
            .iter()
            .filter(|e| e.class == "verification.belief.probe")
            .map(|e| &e.payload)
            .collect();
        assert_eq!(probes.len(), 3, "one row per elicited belief item");
        let divergent = probes
            .iter()
            .find(|p| p.get("divergent") == Some(&Json::Bool(true)))
            .expect("the mismatched elicitation resolves divergent");
        assert_eq!(
            divergent.get("rule_id").and_then(Json::as_str),
            Some("rule/bp-1")
        );
        assert_eq!(
            divergent.get("conditioned_on").and_then(Json::as_str),
            Some("profile/test@1")
        );
        assert_eq!(
            divergent.get("detector").and_then(Json::as_str),
            Some("deterministic")
        );
        assert!(
            divergent
                .get("observed_ref")
                .and_then(Json::as_str)
                .is_some_and(|r| r.starts_with("row:")),
            "a resolved probe cites the durable row, never model text"
        );
        let unresolved: Vec<&&Json> = probes
            .iter()
            .filter(|p| p.get("divergent") == Some(&Json::Bool(false)))
            .collect();
        assert_eq!(
            unresolved.len(),
            2,
            "the pre-observation and absent-handle items emit unresolved"
        );
        for u in &unresolved {
            assert_eq!(
                u.get("observed_ref"),
                Some(&Json::Null),
                "an unresolved handle fabricates no comparison — observed_ref stays null"
            );
        }
        for p in &probes {
            assert_eq!(
                p.get("provisional"),
                Some(&Json::Bool(true)),
                "provisional is constitutive"
            );
        }
    }

    /// A `belief_probe` rule without its assumption-debt record refuses
    /// `open` typed — a conditioned rule is never silently dropped.
    #[test]
    fn a_debtless_belief_probe_rule_refuses_open() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let r = Driver::open_react(
            &probe_ctx(profile_with_belief_probe("deterministic", false)),
            policy,
            &mut sink,
            DriverConfig::default(),
        );
        assert!(
            matches!(
                r,
                Err(DriverError::MalformedProfileRule { ref rule_id })
                    if rule_id == "rule/bp-1"
            ),
            "got {:?}",
            r.map(|_| ())
        );
    }

    /// A `judged` probe rule is declared but never fired by the
    /// deterministic emitter (DF-S1.21-3's honest stratum) — the run
    /// completes with zero `verification.belief.probe` rows.
    #[test]
    fn a_judged_probe_rule_is_declared_not_fired() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver = Driver::open_react(
            &probe_ctx(profile_with_belief_probe("judged", true)),
            policy,
            &mut sink,
            DriverConfig::default(),
        )
        .unwrap();
        let mut model = ScriptedModel {
            script: [outcome(
                hh_gateway::vocab::StopReason::EndTurn,
                vec![ParsedCall {
                    tool_call_id: "tc-1".into(),
                    surface: "fs.read".into(),
                    args_raw: r#"{"beliefs":[{"subject":"effect:x","member":"o","value":1}]}"#
                        .into(),
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
        let _ = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert!(
            sink.events
                .iter()
                .all(|e| e.class != "verification.belief.probe"),
            "a judged probe never fires deterministically"
        );
    }

    /// R2.15 (DF-S1.21-2) — computed metrics emit on a live run: the
    /// deterministic `evalfold` fold appends `measurement.metric.emitted`
    /// rows in the typed `MetricValue` form before `run.finished` —
    /// `n/a{reason}` recorded, never a silent zero.
    #[test]
    fn a_finished_run_emits_computed_metrics() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver =
            Driver::open_react(&ctx(), policy, &mut sink, DriverConfig::default()).unwrap();
        let mut model = ScriptedModel {
            script: [outcome(hh_gateway::vocab::StopReason::EndTurn, vec![])]
                .into_iter()
                .collect(),
        };
        let mut gate = observed_gate();
        let mut asm = NullAssembler;
        let _ = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        let metrics: Vec<&Json> = sink
            .events
            .iter()
            .filter(|e| e.class == "measurement.metric.emitted")
            .map(|e| &e.payload)
            .collect();
        assert!(
            !metrics.is_empty(),
            "the registered ADR-0114 D1 metrics emit on the live run"
        );
        for m in &metrics {
            assert!(m.get("metric_ref").and_then(Json::as_str).is_some());
            assert_eq!(
                m.get("detector").and_then(Json::as_str),
                Some("deterministic")
            );
            assert_eq!(
                m.get("oracle_ref").and_then(Json::as_str),
                Some("oracle/evalfold")
            );
            let value = m.get("value").expect("typed value");
            assert!(
                value.get("kind").and_then(Json::as_str).is_some(),
                "the typed MetricValue form — never a bare number: {m:?}"
            );
        }
        // The honest absence: a metric with no evidence emits the typed
        // `n/a` cell, never zero.
        assert!(
            metrics.iter().any(|m| {
                m.get("value")
                    .and_then(|v| v.get("kind"))
                    .and_then(Json::as_str)
                    == Some("na")
            }),
            "a bare run carries n/a metrics — recorded, not silent"
        );
        // The metric rows precede the run's terminal.
        let finished_pos = sink
            .events
            .iter()
            .position(|e| e.class == "lifecycle.run.finished")
            .expect("run.finished");
        assert!(sink
            .events
            .iter()
            .take(finished_pos)
            .any(|e| e.class == "measurement.metric.emitted"));
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

    // ── R2.8 — the armed exposure lane's emitters (§5d.3; ADR-0093/0094/0095) ──

    /// A catalog-entry fixture — `modes` is the definition's admitted
    /// `definition_modes` (the select meet narrows it with the profile).
    fn exposure_entry(
        sid: &str,
        name: &str,
        modes: &[hh_hir::tools::ExposureMode],
    ) -> tool_exposure::CatalogEntry {
        tool_exposure::CatalogEntry {
            surface_id: sid.into(),
            capability: format!("cap:{name}"),
            version_id: "v1".into(),
            source: tool_exposure::CatalogSource::Harness(Some("test".into())),
            name: name.into(),
            namespace: None,
            admitted_modes: modes.iter().copied().collect(),
            pinned: false,
            hidden: false,
            index_form: tool_exposure::IndexForm {
                name: name.into(),
                title: None,
                summary_ref: format!("sum:{name}"),
                namespace: None,
                tags: vec![],
                search_text_fields: [
                    tool_exposure::SearchTextField::Name,
                    tool_exposure::SearchTextField::Description,
                ]
                .into_iter()
                .collect(),
                search_text: std::collections::BTreeMap::new(),
            },
            size_tokens: 8,
            estimator_ref: "bytes_div_4".into(),
            effect_summary: vec![],
            permission_coverage: tool_exposure::PermissionCoverage::Unknown,
            label: None,
            availability: tool_exposure::Availability::Available,
            is_discovery: false,
            lifted: false,
        }
    }

    /// The bound rank policy: `deep.search` stays `deferred` (the
    /// discovery-served leg), everything else `direct`. `rank` receives
    /// only admissible entries, and every returned mode is inside the
    /// narrowed meet — the enforce leg has nothing to refuse.
    struct DeferDeep;
    impl tool_exposure::ExposurePolicy for DeferDeep {
        fn rank(
            &self,
            entries: &[tool_exposure::CatalogEntry],
            _admitted: &std::collections::BTreeMap<
                String,
                std::collections::BTreeSet<hh_hir::tools::ExposureMode>,
            >,
            turn_state: &tool_exposure::TurnState,
            _params: &tool_exposure::ExposurePolicyParams,
        ) -> Vec<(String, hh_hir::tools::ExposureMode)> {
            entries
                .iter()
                .map(|e| {
                    let revealed = turn_state
                        .revealed
                        .entries
                        .iter()
                        .any(|r| r.surface_id == e.surface_id);
                    (
                        e.surface_id.clone(),
                        if e.surface_id == "surf:deep.search" && !revealed {
                            hh_hir::tools::ExposureMode::Deferred
                        } else {
                            hh_hir::tools::ExposureMode::Direct
                        },
                    )
                })
                .collect()
        }
    }

    /// The armed-lane config: `fs.read`/`pinned_tool`/`discover_surfaces`
    /// direct, `deep.search` deferred+indexed (discovery-served),
    /// `secret_probe` hidden. `retention` is the policy's default reveal
    /// retention — the `call` spelling exercises `surface.evicted`.
    fn exposure_config(retention: tool_exposure::Retention) -> DriverConfig {
        let mut deep = exposure_entry(
            "surf:deep.search",
            "deep.search",
            &[
                hh_hir::tools::ExposureMode::Direct,
                hh_hir::tools::ExposureMode::Indexed,
                hh_hir::tools::ExposureMode::Deferred,
            ],
        );
        deep.index_form.search_text.insert(
            tool_exposure::SearchTextField::Description,
            "deep semantic search".into(),
        );
        let mut hidden = exposure_entry(
            "surf:secret.probe",
            "secret_probe",
            &[hh_hir::tools::ExposureMode::Deferred],
        );
        hidden.hidden = true;
        let mut pinned = exposure_entry(
            "surf:pinned.tool",
            "pinned_tool",
            &[hh_hir::tools::ExposureMode::Direct],
        );
        pinned.pinned = true;
        let mut discover = exposure_entry(
            "surf:kernel.discover",
            "discover_surfaces",
            &[hh_hir::tools::ExposureMode::Direct],
        );
        discover.source = tool_exposure::CatalogSource::Kernel;
        discover.is_discovery = true;
        let mut entries = vec![
            exposure_entry(
                "surf:fs.read",
                "fs.read",
                &[hh_hir::tools::ExposureMode::Direct],
            ),
            deep,
            hidden,
            pinned,
            discover,
        ];
        entries.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
        let catalog = tool_exposure::Catalog {
            catalog_id: tool_exposure::catalog_id_of(&entries, 0, "bundle:r28"),
            epoch: 0,
            bundle_id: "bundle:r28".into(),
            sources: vec![],
            entries,
            indexes: vec![tool_exposure::IndexDecl {
                index: tool_exposure::CatalogIndex::LexicalRegex,
                granularity: tool_exposure::IndexGranularity::Surface,
            }],
            index_ref: None,
        };
        // `discover_surfaces`'s SurfaceSpec declares the capability's
        // argument grammar (closed-world — undeclared members are a
        // `schema_violation`).
        let discover_params: std::collections::BTreeMap<String, crate::output::ParamSpec> = [
            "query",
            "text",
            "form",
            "namespace",
            "effect_filter",
            "source",
            "granularity",
        ]
        .iter()
        .map(|n| {
            (
                (*n).to_string(),
                crate::output::ParamSpec {
                    required: *n == "query",
                    kind: crate::output::ParamKind::Str,
                    enum_values: vec![],
                    domain: vec![],
                },
            )
        })
        .chain([(
            "tags".to_string(),
            crate::output::ParamSpec {
                required: false,
                kind: crate::output::ParamKind::Arr,
                enum_values: vec![],
                domain: vec![],
            },
        )])
        .chain([(
            "limit".to_string(),
            crate::output::ParamSpec {
                required: false,
                kind: crate::output::ParamKind::Int,
                enum_values: vec![],
                domain: vec![],
            },
        )])
        .collect();
        let spec = |name: &str| SurfaceSpec {
            surface_id: name.into(),
            semantic_id: format!("sem/{name}"),
            params: if name == "discover_surfaces" {
                discover_params.clone()
            } else {
                Default::default()
            },
            risk_class: None,
        };
        DriverConfig {
            surfaces: ["fs.read", "deep.search", "pinned_tool", "discover_surfaces"]
                .iter()
                .map(|s| spec(s))
                .collect(),
            exposure: Some(ExposureRuntime {
                catalog,
                profile_modes: [
                    hh_hir::tools::ExposureMode::Direct,
                    hh_hir::tools::ExposureMode::Indexed,
                    hh_hir::tools::ExposureMode::Deferred,
                ]
                .into_iter()
                .collect(),
                params: tool_exposure::ExposurePolicyParams {
                    retention,
                    ..tool_exposure::ExposurePolicyParams::default()
                },
                policy: Some(std::sync::Arc::new(DeferDeep)),
                gates: tool_exposure::SelectionGates::default(),
                executor: tool_exposure::IndexExecutor::default(),
            }),
            window_cap_tokens: 1_000_000,
            ..DriverConfig::default()
        }
    }

    /// The scripted gate — every non-discovery dispatch settles
    /// `observed` carrying `sub-1` (`stop_rule = submit` — the settle is
    /// what completes the run; the in-kernel `discover_surfaces` outcome
    /// declares no submission ref and never ends the run).
    fn observing_gate() -> ScriptedGate {
        ScriptedGate {
            out: GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "ok".into(),
                },
                submission_ref: Some("sub-1".into()),
                error_class: None,
                emitted: vec![],
            },
            finish: None,
        }
    }

    /// The fixture ctx with a raised format-error bound — the scenario
    /// deliberately emits two `unknown_surface` rejections (the refused
    /// calls' C0 leg) and a stale `model_completed` cue can add one more
    /// streak count before `effects_settled` pops the submission; `8`
    /// keeps the refusal run honest without tripping the streak bound.
    fn exposure_ctx() -> ControlContext {
        let mut c = ctx();
        c.parameters.max_consecutive_format_errors = 8;
        c
    }

    /// R2.8 AC — the §5d.3 emitters fire from a real `run`, not a
    /// fixture-folded ledger: `catalog.built` once (the opening epoch), an
    /// `exposure.planned` per `propose`, `call.refused{surface_not_
    /// revealed, hint: search}` on the unrevealed `deep.search` call, the
    /// kernel `discover_surfaces` call minting `discovery.searched` +
    /// `surface.revealed` in that order, the revealed surface's first
    /// `context.artefact.delivered{tool_surface}`, and its call minting
    /// `context.artefact.activated{artefact_id = "deep.search"}` beside
    /// `action.tool.proposed`. Under `run` retention no `surface.evicted`
    /// fires mid-run.
    #[test]
    fn r2_8_exposure_emitters_fire_from_a_real_run() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        // The scenario deliberately emits `unknown_surface` rejections
        // (the refused calls' C0 leg) and stale `model_completed` cues can
        // pop further proposes before the submission's `effects_settled`
        // lands — the envelope's declared bound is raised for the
        // fixture, not bypassed.
        let mut envelope = EnvelopePolicy::stage1_default("b-1");
        envelope.output_validation.max_format_failures = 8;
        let policy = envelope.seal().unwrap();
        let mut driver = Driver::open_react(
            &exposure_ctx(),
            policy,
            &mut sink,
            exposure_config(tool_exposure::Retention::Run),
        )
        .unwrap();
        let mut model = ScriptedModel {
            script: [
                // mc-1 — a call naming no catalog entry: `check_callable`
                // refuses (`surface_not_revealed{surface_id: null,
                // hint: none}`) and G-INTERPRET reads it `unknown_surface`
                // off the delivered set — both rows, never a dispatch.
                outcome(
                    hh_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-1".into(),
                        surface: "nope.tool".into(),
                        args_raw: "{}".into(),
                    }],
                ),
                // mc-2 — the kernel `discover_surfaces` capability:
                // `exposure_dispatch` runs `discover` in-kernel.
                outcome(
                    hh_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-2".into(),
                        surface: "discover_surfaces".into(),
                        args_raw: r#"{"query":"deep","form":"regex"}"#.into(),
                    }],
                ),
                // mc-3 — a call on the deferred surface ahead of the
                // reveal (the mc-1 nudge's propose leg runs before the
                // pending intent's `act`): `surface_not_revealed{
                // hint: search}` beside the `unknown_surface` rejection.
                outcome(
                    hh_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-3".into(),
                        surface: "deep.search".into(),
                        args_raw: "{}".into(),
                    }],
                ),
                // mc-4 — the revealed surface is `direct` now: delivered,
                // proposed, activated, dispatched (the `sub-1` settle
                // completes the run). The `fs.read` tail absorbs any
                // further `propose` a queued cue drives — every dispatch
                // carries `sub-1`, so unspent entries are inert.
                outcome(
                    hh_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-4".into(),
                        surface: "deep.search".into(),
                        args_raw: "{}".into(),
                    }],
                ),
                outcome(
                    hh_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-5".into(),
                        surface: "fs.read".into(),
                        args_raw: "{}".into(),
                    }],
                ),
                outcome(
                    hh_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-6".into(),
                        surface: "fs.read".into(),
                        args_raw: "{}".into(),
                    }],
                ),
            ]
            .into_iter()
            .collect(),
        };
        let mut gate = observing_gate();
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert_eq!(r.report.stop_reason, StopReason::Completed);

        let classes: Vec<&str> = sink.events.iter().map(|e| e.class.as_str()).collect();
        // The opening `catalog.built` — exactly once per run.
        assert_eq!(
            classes
                .iter()
                .filter(|c| **c == "action.tool.catalog.built")
                .count(),
            1,
            "one catalog.built row"
        );
        // `exposure.planned` once per propose (>= 3 model calls).
        let planned: Vec<&EventEnvelope> = sink
            .events
            .iter()
            .filter(|e| e.class == "action.tool.exposure.planned")
            .collect();
        assert!(planned.len() >= 3, "planned per propose: {planned:?}");
        // The refused calls' `call.refused` rows — durable beside the
        // interpret pipeline's rejection rows: tc-1 names no catalog
        // entry (no `surface_id`, `hint: none` — the refusal leaks no
        // schema, AC-R-2.5.3-1), tc-3 names the unrevealed deferred
        // member (`hint: search` — the honest next step).
        let refused_rows: Vec<&EventEnvelope> = sink
            .events
            .iter()
            .filter(|e| e.class == "action.tool.call.refused")
            .collect();
        assert_eq!(refused_rows.len(), 2, "one refusal per refused call");
        let refused_unknown = refused_rows[0];
        assert_eq!(refused_unknown.payload.get("surface_id"), Some(&Json::Null));
        assert_eq!(
            refused_unknown.payload.get("reason").and_then(Json::as_str),
            Some("surface_not_revealed")
        );
        assert_eq!(
            refused_unknown.payload.get("hint").and_then(Json::as_str),
            Some("none")
        );
        assert_eq!(
            refused_unknown
                .payload
                .get("tool_call_id")
                .and_then(Json::as_str),
            Some("tc-1")
        );
        let refused = refused_rows[1];
        assert_eq!(
            refused.payload.get("surface_id").and_then(Json::as_str),
            Some("surf:deep.search")
        );
        assert_eq!(
            refused.payload.get("reason").and_then(Json::as_str),
            Some("surface_not_revealed")
        );
        assert_eq!(
            refused.payload.get("hint").and_then(Json::as_str),
            Some("search")
        );
        assert_eq!(
            refused.payload.get("tool_call_id").and_then(Json::as_str),
            Some("tc-3")
        );
        // `discovery.searched` precedes `surface.revealed`; the hit is the
        // deferred entry's surface id (the catalog idp id, not the name).
        let searched = sink
            .events
            .iter()
            .find(|e| e.class == "action.tool.discovery.searched")
            .expect("discovery.searched emitted");
        assert_eq!(
            searched.payload.get("tool_call_id").and_then(Json::as_str),
            Some("tc-2")
        );
        assert_eq!(
            searched.payload.get("query_form").and_then(Json::as_str),
            Some("regex")
        );
        assert_eq!(
            searched.payload.get("executed_by").and_then(Json::as_str),
            Some("kernel")
        );
        assert!(searched.payload.get("query_hash").is_some());
        let revealed = sink
            .events
            .iter()
            .find(|e| e.class == "action.tool.surface.revealed")
            .expect("surface.revealed emitted");
        assert!(revealed.seq > searched.seq, "revealed lands after searched");
        assert_eq!(
            revealed.payload.get("surface_id").and_then(Json::as_str),
            Some("surf:deep.search")
        );
        assert_eq!(
            revealed.payload.get("mode_to").and_then(Json::as_str),
            Some("direct")
        );
        assert_eq!(
            revealed.payload.get("cause").and_then(Json::as_str),
            Some("discovery")
        );
        // The revealed surface's call mints `context.artefact.activated`
        // with `artefact_id = surface_id` (the model-facing name).
        let activated = sink
            .events
            .iter()
            .find(|e| {
                e.class == "context.artefact.activated"
                    && e.payload.get("artefact_id").and_then(Json::as_str) == Some("deep.search")
            })
            .expect("artefact.activated emitted for the revealed surface");
        assert_eq!(
            activated.payload.get("tool_call_id").and_then(Json::as_str),
            Some("tc-4")
        );
        // First-delivery `tool_surface` rows exist for the direct set and
        // for the revealed surface once it joins `direct`.
        let delivered: Vec<&str> = sink
            .events
            .iter()
            .filter(|e| {
                e.class == "context.artefact.delivered"
                    && e.payload.get("kind").and_then(Json::as_str) == Some("tool_surface")
            })
            .filter_map(|e| e.payload.get("artefact_id").and_then(Json::as_str))
            .collect();
        assert!(delivered.contains(&"fs.read"));
        assert!(delivered.contains(&"discover_surfaces"));
        assert!(delivered.contains(&"deep.search"));
        assert!(
            !delivered.contains(&"secret_probe"),
            "a hidden surface is never delivered"
        );
        // `run` retention: no mid-run eviction.
        assert!(!classes.contains(&"action.tool.surface.evicted"));
        // The hidden surface never appears in a plan's order.
        for p in &planned {
            let hidden_in_plan = match p.payload.get("order") {
                Some(Json::Arr(order)) => order
                    .iter()
                    .any(|s| s.as_str() == Some("surf:secret.probe")),
                _ => false,
            };
            assert!(!hidden_in_plan, "hidden surfaces never enter a plan");
        }
    }

    /// `retention: call` — the reveal minted during the discovery call's
    /// model round expires at its `CallEnd` boundary:
    /// `surface.evicted{cause: policy}` lands after `surface.revealed`
    /// inside the same model call (the durable rows, not memory, are the
    /// revealed set).
    #[test]
    fn r2_8_call_retention_evicts_at_call_end() {
        let mut sink = MemSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver = Driver::open_react(
            &ctx(),
            policy,
            &mut sink,
            exposure_config(tool_exposure::Retention::Call),
        )
        .unwrap();
        let mut model = ScriptedModel {
            script: [
                outcome(
                    hh_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-1".into(),
                        surface: "discover_surfaces".into(),
                        args_raw: r#"{"query":"deep","form":"regex"}"#.into(),
                    }],
                ),
                // mc-2 — a delivered `fs.read` dispatch; its `sub-1`
                // settle completes the run.
                outcome(
                    hh_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-2".into(),
                        surface: "fs.read".into(),
                        args_raw: "{}".into(),
                    }],
                ),
            ]
            .into_iter()
            .collect(),
        };
        let mut gate = observing_gate();
        let mut asm = NullAssembler;
        let r = driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        assert_eq!(r.report.stop_reason, StopReason::Completed);
        let revealed = sink
            .events
            .iter()
            .find(|e| e.class == "action.tool.surface.revealed")
            .expect("surface.revealed emitted");
        let evicted = sink
            .events
            .iter()
            .find(|e| e.class == "action.tool.surface.evicted")
            .expect("surface.evicted emitted at CallEnd");
        assert_eq!(
            evicted.payload.get("surface_id").and_then(Json::as_str),
            Some("surf:deep.search")
        );
        assert_eq!(
            evicted.payload.get("cause").and_then(Json::as_str),
            Some("policy")
        );
        assert!(evicted.seq > revealed.seq, "eviction follows the reveal");
    }

    /// `sync_exposure_source` — the host-source sync seam (§5d.3 §6;
    /// ADR-0095 D1/D2): a `list_changed` listing under `adopt` mints
    /// `catalog.delta` then `catalog.epoch` (+ the rebuilt `catalog.built`)
    /// in that order and the run state advances; under `ask` the same
    /// listing mints `security.permission.requested` and adopts nothing
    /// (no `catalog.epoch` — the principal's grant is the adoption leg).
    #[test]
    fn r2_8_source_sync_mints_delta_epoch_and_ask_refuses() {
        for (drift, expect_epoch) in [
            (tool_exposure::DriftPolicy::Adopt, true),
            (tool_exposure::DriftPolicy::Ask, false),
        ] {
            let mut sink = MemSink {
                events: vec![],
                seq: 0,
            };
            let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
            let mut cfg = exposure_config(tool_exposure::Retention::Run);
            cfg.exposure.as_mut().unwrap().params.drift_policy = drift;
            let mut driver = Driver::open_react(&ctx(), policy, &mut sink, cfg).unwrap();
            let mut model = ScriptedModel {
                script: [outcome(
                    hh_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-1".into(),
                        surface: "fs.read".into(),
                        args_raw: "{}".into(),
                    }],
                )]
                .into_iter()
                .collect(),
            };
            let mut gate = observing_gate();
            let mut asm = NullAssembler;
            // A first run seeds the exposure state (the sync seam needs
            // the armed catalog — it refuses before the first propose).
            let r = driver
                .run(&mut model, &mut gate, &mut asm, &mut sink)
                .unwrap();
            assert_eq!(r.report.stop_reason, StopReason::Completed);
            // The source's new listing: `deep.search` gone (the `removed`
            // leg), `fresh_tool` added (the `added` leg) — the listing is
            // the source's complete new set.
            let mut fresh = exposure_entry(
                "surf:fresh.tool",
                "fresh_tool",
                &[hh_hir::tools::ExposureMode::Deferred],
            );
            fresh.source = tool_exposure::CatalogSource::Harness(Some("test".into()));
            let mut relisted_hidden = exposure_entry(
                "surf:secret.probe",
                "secret_probe",
                &[hh_hir::tools::ExposureMode::Deferred],
            );
            relisted_hidden.hidden = true;
            let mut relisted_pinned = exposure_entry(
                "surf:pinned.tool",
                "pinned_tool",
                &[hh_hir::tools::ExposureMode::Direct],
            );
            relisted_pinned.pinned = true;
            let listing = vec![
                exposure_entry(
                    "surf:fs.read",
                    "fs.read",
                    &[hh_hir::tools::ExposureMode::Direct],
                ),
                relisted_hidden,
                relisted_pinned,
                fresh,
            ];
            let epoch = driver
                .sync_exposure_source(
                    &mut sink,
                    "harness:test",
                    tool_exposure::SyncTrigger::ListChanged,
                    &listing,
                    &tool_exposure::SourceState {
                        source_ref: "harness:test".into(),
                        snapshot_hash: "snap:2".into(),
                        ttl: None,
                        listened: true,
                    },
                )
                .unwrap();
            let classes: Vec<&str> = sink.events.iter().map(|e| e.class.as_str()).collect();
            assert!(classes.contains(&"action.tool.catalog.delta"));
            if expect_epoch {
                assert_eq!(epoch, Some(1), "adopt advances the epoch");
                let delta = sink
                    .events
                    .iter()
                    .find(|e| e.class == "action.tool.catalog.delta")
                    .unwrap();
                let epoch_row = sink
                    .events
                    .iter()
                    .find(|e| e.class == "action.tool.catalog.epoch")
                    .expect("catalog.epoch emitted on adopt");
                assert!(epoch_row.seq > delta.seq, "epoch lands after delta");
                assert_eq!(
                    epoch_row.payload.get("epoch").and_then(Json::as_int),
                    Some(1)
                );
            } else {
                assert_eq!(epoch, None, "ask adopts nothing");
                assert!(!classes.contains(&"action.tool.catalog.epoch"));
                assert!(
                    classes.contains(&"security.permission.requested"),
                    "ask mints the permission request"
                );
            }
        }
    }
}
