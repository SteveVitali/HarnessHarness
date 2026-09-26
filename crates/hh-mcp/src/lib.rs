//! `hh-mcp` — the Stage-3 fixture MCP plane (spec §7.3; R-2.11.3⁰;
//! ticket S3.1; ADR-0097 D7/ADR-0173/ADR-0174) plus the R-2.5.4⁰
//! protocol-edge slice (§5d.4; ticket S3.9; ADR-0097 D2/ADR-0099).
//!
//! The crate is pure pieces, all functions of records — never of
//! caller `_meta`, never of live state:
//!
//! - [`artifact`] — the `hh-mcp-target/1` served-artifact record and the
//!   compiled-bundle → canonical tool-catalogue lowering. The catalogue
//!   is a pure function of `bundle_id` (ADR-0097 D1): `server/discover`
//!   and `tools/list` are byte-identical for every connection of one
//!   bundle (AC-R-2.11.3-1). The served `_meta` carries the whole HIR
//!   block (the minimum carried set — AC-R-2.5.1-5).
//! - [`binding`] — the `stdio_launch` `CallerBinding` record. At Stage 3
//!   the caller is the spawner the record names and the principal is
//!   fixed to the test principal (R-2.11.3⁰ stage row; ADR-0174 D1/D2).
//! - [`protocol`] — the edge vocabulary: `ProtocolBinding`, `Era`, the
//!   pinned version set, the typed `NegotiateError`, the omitted→
//!   `unknown` capability reconcile (ADR-0099 D1, N1–N3).
//! - [`client`] — the dual-era probe-first stdio client: `server/
//!   discover` before any other request, the compatibility probe into
//!   the legacy era, bounded `tools/list` (`IndexOverflow`), `tools/
//!   call` → the closed [`client::ToolOutcome`] sum.
//! - [`listing`] — `import_listing`'s `hh-mcp-listing/1` document and
//!   `refresh`'s name-keyed delta — wire tools verbatim, content hashes
//!   for the `pin`/`supersedes` seams.
//! - [`server`] — the newline-delimited JSON-RPC serve loop the
//!   `hh-mcp-serve` binary wraps, in both pinned eras
//!   ([`server::ServeMode`]); `serve_dynamic` emits
//!   `notifications/tools/list_changed` when the served bundle changes.
//!   It holds no state the artifact does not carry: no ledger, no
//!   kernel link, no authority from caller `_meta` (R-3 — claims never
//!   decide).
//!
//! The server serves tools **only** from a bundle's compiled
//! `target:mcp` member — the artifact file is the whole world. Tool
//! execution binds at Stage 4 (`hh-lab/1`); at Stage 3 `tools/call`
//! answers a typed `isError` refusal or the fixture's paused-effect
//! shape, and handle-bearing arguments answer `NoCoveringGrant`
//! (AC-R-2.11.3-2's fixture half).

pub mod artifact;
pub mod binding;
pub mod client;
pub mod executor;
pub mod http;
pub mod listing;
pub mod oauth;
pub mod protocol;
pub mod server;

pub use artifact::{
    lower_mcp_target, McpError, HH_META_KEY, INPUT_REQUIRED_META_KEY, MCP_ARTIFACT_SCHEMA,
    PROTOCOL_VERSION,
};
pub use binding::{stdio_launch_binding, TEST_PRINCIPAL};
pub use client::{ClientError, Listing, McpClient, StdioTransport, ToolOutcome, Transport};
pub use executor::McpExecutor;
pub use http::{
    serve_http, HttpResponse, HttpTransport, OriginPolicy, OriginRefusal, ServeHttpError,
};
pub use oauth::{
    parse_www_authenticate, AuthClaims, CredentialMediator, OAuthError, OAuthFlow, OAuthRequest,
    TokenGrant,
};
pub use protocol::{
    pin_version, reconcile_capabilities, CapabilitySupport, Era, NegotiateError, ProtocolBinding,
    PINNED_LEGACY, PINNED_MODERN, PINNED_VERSIONS,
};
pub use server::{serve, serve_dynamic, ServeMode};
