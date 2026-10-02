//! `hh-mcp-lab` — the C1 Stage-4 `hh-lab/1` MCP instrument
//! (S4.11; R-2.11.3¹; ADR-0303).
//!
//! The instrument serves the Lab read/launch/experiment/results
//! groups over stdio and Streamable HTTP under a `CallerBinding`
//! (§7.3). Every `tools/call` is a surface-session turn: the
//! `lifecycle.turn.*` spine wraps an `action.effect.*` chain —
//! `intended → authorized (decided for mutating) → prepared →
//! committed? → observed|refused` — and posts
//! `control.budget.consumed{tool_calls: +1, charged_to: instrument}`
//! per call against the caller's pool node. All durable writes run
//! through `EmbedService`'s `surface_*` seam — the crate holds no
//! ledger internals, no kernel-private helpers, and no state of
//! record (the handle table, the turn counter, the `launched` resume
//! rows are all projections of durable rows).
//!
//! Modules:
//! - [`binding`] — `CallerBinding`/`CallerAuth`/`CallerKind` +
//!   `stdio_launch`/oauth/mTLS credential records;
//! - [`exposure`] — the `hh-lab/1` document, `ExposurePolicy`, the
//!   assumption-debt table, the `ServedArtifact` lowering;
//! - [`handles`] — the handle carrier (`surface_ids` names, typed
//!   refusals, the `launched` resume row);
//! - [`session`] — the `run_kind = surface` session + pool-root
//!   allocation + per-call charges;
//! - [`effects`] — the `action.effect.*` chain (write-ahead
//!   `committed` for mutating calls);
//! - [`launch`] — `launch_run` (`open_session` + `spawn_event`
//!   causality + handle mints) and the control-turn pass-throughs;
//! - [`reads`] — `read_ledger`/`run_status`/`get_trace`/
//!   `list_pending_approvals` under `ExposurePolicy` + the
//!   `measurement.export.delivered` mint;
//! - [`supply`] — the hosted-participant Π deny/ask/reveal path;
//! - [`dispatch`] — the `tools/call` orchestration + the typed
//!   `surface_error` refusal records;
//! - [`server`] — `LabServer`, the message dispatch, the stdio loop;
//! - [`http`] — the Streamable HTTP arm (`/mcp`, bearer auth,
//!   `Mcp-Session-Id`).

pub mod binding;
pub mod dispatch;
pub mod effects;
pub mod exposure;
pub mod handles;
pub mod http;
pub mod launch;
pub mod reads;
pub mod resources;
pub mod server;
pub mod session;
pub mod supply;
pub mod tasks;

pub use binding::{stdio_launch_binding, CallerAuth, CallerBinding, CallerKind};
pub use exposure::{default_exposure, parse_exposure, ExposureDef, ExposureError, ExposurePolicy};
pub use server::{serve_stdio, LabServer};
