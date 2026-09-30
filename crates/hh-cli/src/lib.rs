//! `hh-cli` — the attended terminal surface for `hh-kernel`.
//!
//! The CLI is a generated client + renderer over `hh-embed/1` and nothing
//! else (K-2): every command maps its arguments onto typed boundary
//! records, invokes exactly one boundary-operation sequence, and renders
//! the ledger projections, prompts and terminal chrome the boundary
//! returns. No kernel semantics, no authority decisions, no independent
//! run state — the ledger the kernel writes is the only truth the CLI
//! reports.
//!
//! Modules:
//! - [`cli`] — argument mapping, the command surface (`run start`,
//!   `run resume|fork|cancel|inspect|amend|submit|events|status`,
//!   `approval list|show|respond`, `version`, `doctor`) and the attended
//!   loop.
//! - [`invocation`] — the attendance declaration, the `InvocationRecord`
//!   + derived idempotency key, the typed-stdin rule, and the pre-ledger
//!     `invocation_error` checks.
//! - [`boundary`] — the `Boundary` trait plus binding (b): the generated
//!   client over a spawned `hh-kernel serve` child.
//! - [`exit_class`] — the closed `ExitClass` sum, the total derivation
//!   from `stop_reason × outcome_class`, and the interim numeral band.

pub mod boundary;
pub mod cli;
pub mod exit_class;
pub mod invocation;
mod lab;
pub mod presets;
pub mod trust;
