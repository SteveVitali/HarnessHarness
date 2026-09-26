//! `hh-acp` — the ACP session boundary crate (§5d.4 C1; S4.5b).
//!
//! Modules:
//! - [`protocol`] — the ACP pin set, the v2/v1 era negotiation, the
//!   `acp` `ProtocolBinding` record.
//! - [`artifact`] — `AcpArtifact` (the parsed `hh-acp-target/1`),
//!   `render_session`, the v1 compatibility projection, `ext`
//!   preservation.
//! - [`transport`] — `SessionTransport` + the in-memory duplex
//!   fixture channel (the separate-channel requirement — never an
//!   in-process call).
//! - [`driver`] — `SessionDriver` (records-in/records-out) + the
//!   scripted `FixtureDriver`.
//! - [`serve`] — `serve_session` — the agent-side session loop:
//!   `initialize`, `session/*`, `_hh/*`, `session/request_permission`,
//!   `subscriptions/listen`.
//! - [`client`] — `AcpClient` — the binding-(c) `attach_session`
//!   client half.
//! - [`permission`] — the permission-transport records + the Π seam.
//! - [`a2a`] — the C2 A2A edge: `AgentCard`, signatures under a
//!   `TrustRootPolicy` fixture, `delegate_task`, `TaskState`.

pub mod a2a;
pub mod artifact;
pub mod client;
pub mod driver;
pub mod permission;
pub mod protocol;
pub mod serve;
pub mod transport;

pub use artifact::{render_session, AcpArtifact, SessionUpdate};
pub use client::{AcpClient, ClientError};
pub use driver::{FixtureDriver, SessionDriver, SessionInfo, TurnDrive};
pub use hh_compiler::acp::{AcpDialect, TaskState};
pub use permission::{PermissionOutcome, PermissionRequest, PiGate, StaticPi};
pub use protocol::{acp_binding, acp_pin_set, negotiate_era, AcpEra, ACP_VERSION};
pub use serve::{serve_session, ServeError};
pub use transport::{channel, MemorySessionTransport, SessionTransport};
