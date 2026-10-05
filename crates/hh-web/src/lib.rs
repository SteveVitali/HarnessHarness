//! `hh-web` — the C1 read-only web instrument (spec §7.2, R-2.11.2¹;
//! ticket S4.10; ADR-0301/0302).
//!
//! The surface is a **pure projection client** over `hh-embed/1` binding
//! (c) (`local_network` — the kernel's `serve --http`): it holds no
//! state of record, every view assembles canonical op results through
//! the session manager's attach sessions, and every consequential write
//! is a named kernel/Lab op (`respond_permission`, `amend`,
//! `kernel.reproduce`) with responder provenance + a request id (P12 —
//! "every consequential write must land as a ledgered action with an
//! identifiable requester, an idempotency/request id, and a named
//! surface channel").
//!
//! The P1–P13 posture lives in [`gate`]; the op admission table in
//! [`ops`]; the per-run session ownership in [`session`]; the declared
//! SinkPolicy's serving half in [`sink`]; the V1–V11 catalogue in
//! [`views`]; the serial server + route pipeline in [`server`]; the
//! static shell in [`ui`].

pub mod gate;
pub mod ops;
pub mod server;
pub mod session;
pub mod sink;
pub mod ui;
pub mod views;
