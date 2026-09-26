//! `hh-subagent` — the C1/Stage-4 kernel slice (spec §5e.3, R-2.6.3¹ +
//! R-2.1.6¹; ticket S4.6).
//!
//! The one kernel operation `spawn` composes `delegate` + `allocate` +
//! `derive` + the child writer lease atomically over a typed
//! [`types::SubagentSpec`], idempotent on `(decision, H(spec))`. Results are
//! artifacts and events — one `subagent_result` candidate at `≤ delegate`
//! with union taint, never a transcript (M-1). `merge` is the parent's own
//! baselined effects under the one [`types::MergePolicy`] sum with the
//! G-1/G-2/G-5 vetoes. Containment C-1…C-8 is the parent-side machinery here
//! (parent stop drain, child-terminal wakeup, revocation cascade, root-first
//! exhaustion, crash windows KP-14/16–21).
//!
//! MUST-code (TCB); never model-facing. No `Text` leaf is read on any path —
//! `SubagentSpec.goal.statement` travels as its content-addressed leaf and
//! child output re-enters as `Ref`s (SP-3).
//!
//! The C3 `orchestrator` class lives in `hh-orchestrator` — its only
//! consumer; nothing in `hh-subagent` names it (removability(1) keeps C1
//! while C3 may be absent).

pub mod types;

pub mod merge;
pub mod messaging;
pub mod ownership;
pub mod recovery;
pub mod result;
pub mod spawn;

pub use types::*;
