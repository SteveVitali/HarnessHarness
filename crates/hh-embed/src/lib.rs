//! `hh-embed` — the `hh-embed/1` embedding boundary (spec §7.4, R-2.11.4⁰ᵇ;
//! ticket S1.25).
//!
//! Two bindings, one dispatch:
//!
//! - **Binding (a)** — [`EmbedService`], the in-process service. `handle`
//!   maps a parsed JSON-RPC [`Request`](hh_wire::jsonrpc::Request) to the
//!   response `Json`; `drain_notifications` yields pending `stream.frame`
//!   and `upcall.*` notifications.
//! - **Binding (b)** — [`stdio::run`], the newline-delimited JSON-RPC 2.0
//!   loop over stdin/stdout. It calls the *same* `EmbedService::handle`,
//!   so the bindings are byte-identical by construction (AC-R-2.11.4-9).
//!
//! A session is a ledger run under a fenced writer lease (`open_session`
//! `new`/`resume`); `attach` sessions are read-only by construction — no
//! lease is taken and no append path exists for them (I6). `open_session`
//! `new` performs the resolve → validate → seal chain over the submitted
//! definition (`hh-assembly` + the Stage-1 catalog + the embedded
//! `RegistryStore`), provisions a `local_host` environment through
//! `EnvDriver`, and drives the run through the canonical
//! [`Driver`](hh_control::driver::Driver) (`ReactMinimal`) with kernel
//! ports implemented in this crate ([`runtime`]).

pub(crate) mod bundle_ops;
pub mod envops;
pub(crate) mod eval_ops;
pub(crate) mod experiment_ops;
pub mod frames;
pub mod inject;
pub mod open;
pub mod overrides;
pub(crate) mod registry_ops;
pub mod runtime;
pub mod service;
pub mod stdio;
pub mod work;

pub use service::{EmbedService, ServiceConfig};
pub use stdio::run as serve_stdio;
