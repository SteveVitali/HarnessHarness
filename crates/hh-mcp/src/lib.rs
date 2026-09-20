//! `hh-mcp` — the Stage-3 fixture MCP plane (spec §7.3; R-2.11.3⁰;
//! ticket S3.1; ADR-0097 D7/ADR-0173/ADR-0174).
//!
//! The crate is three pure pieces, all functions of records — never of
//! caller `_meta`, never of live state:
//!
//! - [`artifact`] — the `hh-mcp-target/1` served-artifact record and the
//!   compiled-bundle → canonical tool-catalogue lowering. The catalogue
//!   is a pure function of `bundle_id` (ADR-0097 D1): `server/discover`
//!   and `tools/list` are byte-identical for every connection of one
//!   bundle (AC-R-2.11.3-1).
//! - [`binding`] — the `stdio_launch` `CallerBinding` record. At Stage 3
//!   the caller is the spawner the record names and the principal is
//!   fixed to the test principal (R-2.11.3⁰ stage row; ADR-0174 D1/D2).
//! - [`server`] — the newline-delimited JSON-RPC `serve` loop the
//!   `hh-mcp-serve` binary wraps. It holds no state the artifact does
//!   not carry: no ledger, no kernel link, no authority from caller
//!   `_meta` (R-3 — claims never decide).
//!
//! The server serves tools **only** from a bundle's compiled
//! `target:mcp` member — the artifact file is the whole world. Tool
//! execution binds at Stage 4 (`hh-lab/1`); at Stage 3 `tools/call`
//! answers a typed `isError` refusal, and handle-bearing arguments
//! answer `NoCoveringGrant` (AC-R-2.11.3-2's fixture half).

pub mod artifact;
pub mod binding;
pub mod server;

pub use artifact::{
    lower_mcp_target, McpError, HH_META_KEY, MCP_ARTIFACT_SCHEMA, PROTOCOL_VERSION,
};
pub use binding::{stdio_launch_binding, TEST_PRINCIPAL};
pub use server::serve;
