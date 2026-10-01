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
//! - **Binding (c)** — [`net::serve_net`], the `local_network` transport
//!   (§7.2; S4.10): JSON-RPC over loopback HTTP/1.1 with the per-launch
//!   bearer + Host/Origin posture (P1–P9's kernel-side end). It calls the
//!   *same* `EmbedService::handle`; `GET /hh-embed/1/events` drains the
//!   notification channel as NDJSON.
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

pub(crate) mod analysis_ops;
mod assembly_ops;
pub(crate) mod bundle_ops;
pub(crate) mod durability_ops;
pub mod envops;
pub(crate) mod eval_ops;
pub(crate) mod experiment_ops;
pub(crate) mod fleet_ops;
pub mod frames;
pub(crate) mod hosting_ops;
pub mod inject;
pub(crate) mod lab_c1_ops;
pub mod net;
pub mod open;
pub mod overrides;
pub(crate) mod registry_ops;
pub(crate) mod replay_ops;
pub mod runtime;
pub mod service;
pub mod stdio;
pub mod work;

pub use service::{EmbedService, ServiceConfig};
pub use stdio::run as serve_stdio;
