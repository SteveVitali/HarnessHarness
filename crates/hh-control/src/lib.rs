//! `hh-control` — the C0/Stage-1 control plane (spec §5e.1 R-2.6.1, §5e.2
//! R-2.6.2; ticket S1.20; ADR-0103…0108).
//!
//! Two halves under one roof:
//!
//! - **The `control_strategy` contract** ([`strategy`], [`react`], [`state`]):
//!   the pluggable owner of `decide`. A registered variant folds durable
//!   ledger events into a canonical-serialisable [`state::ControlState`] and,
//!   per closed-sum [`vocab::Cue`], returns one
//!   [`vocab::ControlDecision`] stamped `{decision_point, owner}` (I1–I9 —
//!   pure, total, bounded, cues are the only input). `checkpoint`/`restore`
//!   give resume-by-leaf (`RestoreError{VariantMismatch, DialectMismatch}`).
//! - **The control envelope** ([`envelope`], [`guards`], [`stop`], [`retry`],
//!   [`loops`], [`output`], [`invariants`], [`policy`], [`views`]): the Core
//!   MUST-code that interprets the sealed [`policy::EnvelopePolicy`] at six
//!   guard points — pure functions of the durable prefix, deny/tighten/
//!   delay/stop only (INV-7) — owns the stop protocol with barrier + drain
//!   ([`stop::DrainReport`]), shared attempt identity + derived deadlines +
//!   reservation sizing, INV-1…9 with the quarantine path, the deterministic
//!   `loop_detector` ladder (nudge→deny→stop), and strict output validation.
//! - **The driver** ([`driver`]): the cue inbox, the
//!   `next_cue → decide → bind → check → control.decision → execute` loop,
//!   `checkpoint_ref` on every `control.decision`, the F2 seam
//!   (`envelope.check → admitted | refused` as `envelope_signal`), and
//!   resume-by-leaf.
//!
//! The control-plane `StopReason`/`StopKind`/`OutcomeClass`/`CancelledBy`/
//! `LoopDetectorKind`/`InvariantId`/`KernelInfraCause` sums live in
//! `hh_ontology::control` (ADR-0106 D6 — the sum is owned there; the
//! gateway's `StopReason` at R-2.3.1 is a different sum and is never read
//! against it). `DecisionPoint`/`Owner`/`ControlBoundary` are reused from the
//! same module (S1.1; CC7 — no re-declaration).

pub mod driver;
pub mod envelope;
pub mod events;
pub mod guards;
pub mod invariants;
pub mod loops;
pub mod output;
pub mod plan_exec;
pub mod policy;
pub mod react;
pub mod replay;
pub mod retry;
pub mod state;
pub mod stop;
pub mod strategy;
pub mod views;
pub mod vocab;

pub use driver::{
    AssemblerPort, Driver, DriverConfig, EffectGate, ModelOutcome, ModelPort, RunResult,
};
pub use envelope::{CheckVerdict, Envelope, EnvelopeState};
pub use guards::GuardVerdict;
pub use invariants::{InvariantViolation, QuarantineReport};
pub use loops::{LadderPosition, LoopHit, LoopState};
pub use output::{OutputFailure, ValidationState};
pub use policy::EnvelopePolicy;
pub use state::{ControlState, PlanCursor};
pub use stop::DrainReport;
pub use strategy::{
    ControlCapabilities, ControlContext, ControlError, ControlStrategy, FinalReport, RestoreError,
};
pub use vocab::{
    ActMode, ControlDecision, Cue, DecisionKind, DecisionStamp, DeliveryMode, EffectOutcome,
    EnvelopeSignal, EscalateAsk, ExpectedOutput, GuardPoint, HumanInput, OnPartial, RetryTarget,
    ScopeKind, WaitUntil, WokenTrigger,
};
