//! `hh-monitor` — the C0/Stage-1 **reference monitor**: the security kernel and
//! capability model of spec §5g.1 (R-2.8.1; ADR-0051/0052/0053).
//!
//! This crate is **TCB** (§5g.1 §2.6 — MUST-code): the handle-table projection,
//! `mint_root_handles`, `authorize`, `delegate`, `revoke`, the deterministic risk
//! assessors, the Π interpreter and the `SurfaceArgMap` evaluator. The boundary
//! rules this crate must never break:
//!
//! - **No `Text` leaf and no model call on any path** (AC-R-2.8.1-11). Every
//!   input type is a canonical structured record; the static scan in
//!   [`tcb`] and the runtime [`guard::DecisionGuard`] enforce it.
//! - **Authority is conferred, never read** (CC2): a `HandleId` is an opaque
//!   kernel-table key — a string that *looks* like one in a tool result, a
//!   workspace file or a forged `decided` payload confers nothing
//!   (AC-R-2.8.1-2; [`leak`] enforces I-H1).
//! - **Attenuation is downward-only** (I-H4): `delegate` can only narrow
//!   (`grant ⊆ parent`, `ceiling ≤ parent`, `parent.delegable`), and `revoke`
//!   cascades through descendants in one transaction.
//! - **Complete mediation** (I-H7): the ledger append rule — a
//!   `security.permission.decided{decision = allow}` for the same
//!   `(effect_id, attempt)` must precede `action.effect.committed` — lives in
//!   `hh-ledger` (`effect::validate_decision` / the `Undecided` gate), so no
//!   caller can bypass it.
//! - **`unknown ⇒ irreversible`** (2.3): unparseable assessor input projects to
//!   `RiskClass::UNKNOWN`; hints and self-reports are raise-only.

pub mod approval;
pub mod args;
pub mod assess;
pub mod context;
pub mod decision;
pub mod delegate;
pub mod events;
pub mod guard;
pub mod handle;
pub mod leak;
pub mod mint;
pub mod monitor;
pub mod policy;
pub mod table;
pub mod tcb;

pub use decision::{Decider, Decision, DenyReason, KernelDecision};
pub use handle::{AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis};
pub use monitor::{ContainmentGate, Monitor, MonitorError, Proposal};
pub use policy::{Mode, PiVerdict, PolicyTable};
pub use table::HandleTable;
