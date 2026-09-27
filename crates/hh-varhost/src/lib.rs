//! `hh-varhost` — the C0/Stage-2 out-of-process variant host (spec §8.4,
//! R-2.12.2; ticket S2.2; DF-S1.9-1).
//!
//! The kernel side of `plugin_abi/1` for `subprocess_confined` variants:
//! session lifecycle over the live `hh-helper` boundary (V3 — one plugin,
//! one process, one policy; V4 — cleared environment, the ABI socket as
//! the only ambient channel), the probe-first `hello` with pin/schema/
//! protocol checks (V5/V6), `bind`/`invoke`/`stream`/`cancel`/`unbind`/
//! `close`/`guard`/`run_conformance`, the closed host-callback set
//! mediated under grant+budget checks, inbound screening + output stamping
//! (V1), and `lower_requests` — manifest claims to `Permission` +
//! `ContainmentPolicy`.
//!
//! Modules: [`channel`] (framed seq'd transport), [`screen`] (V1/V5 inbound
//! checks), [`lower`] (claims → permission + policy), [`ports`] (the
//! callback surface), [`session`] (spawn/handshake/state), [`host`] (the
//! verbs + mediation), [`package`] (sealed package load/verify).

pub mod channel;
pub mod host;
pub mod lower;
pub mod package;
pub mod ports;
pub mod screen;
pub mod session;

pub use channel::{AbiChannel, ChannelError, FrameIo, MemIo, SocketIo, MAX_FRAME_BYTES};
pub use host::{digest_docs, InvokeOutcome, SessionEvents, VariantHost, VecEvents};
pub use lower::{
    lower_requests, stamp_json, view_kinds_for, LowerContext, LowerError, LoweredRequests,
};
pub use package::{load as load_package, seal as seal_package, variant_class, VariantPackage};
pub use ports::{CallbackCtx, HostPorts, NullPorts, RecordingPorts};
pub use screen::{screen_inbound, ScreenView, ScreenViolation, PLUGIN_VERBS};
pub use session::{attach, spawn, BindingRecord, HelperGuard, SpawnSpec, VariantSession};

use hh_embed_schema::plugin_abi::{AbiError, BindFailure};

/// The host's failure sum — every variant maps to an `AbiError` spelling
/// or a typed `BindFailure`; nothing untyped crosses the boundary.
#[derive(Debug)]
pub enum HostError {
    /// The ABI layer refused (typed).
    Abi(AbiError),
    /// `bind` returned a `BindFailure`.
    BindFailed(BindFailure),
    /// The channel failed (framing, seq, timeout, eof).
    Channel(ChannelError),
    /// Screening refused a message.
    Screen(ScreenViolation),
    /// The helper boundary failed.
    Helper(String),
    /// Package load/verify failed.
    Package(String),
    /// `requests` lowering failed (`RequestsExceedCap`/unrepresentable).
    Lower(LowerError),
    /// Local IO.
    Io(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::Abi(e) => write!(f, "abi: {}", e.as_str()),
            HostError::BindFailed(e) => write!(f, "bind: {}", e.as_str()),
            HostError::Channel(e) => write!(f, "channel: {e}"),
            HostError::Screen(e) => write!(f, "screen: {e}"),
            HostError::Helper(e) => write!(f, "helper: {e}"),
            HostError::Package(e) => write!(f, "package: {e}"),
            HostError::Lower(e) => write!(f, "lower: {e}"),
            HostError::Io(e) => write!(f, "io: {e}"),
        }
    }
}
impl std::error::Error for HostError {}

impl HostError {
    /// The `AbiError` spelling where one exists (the ledger's `failure`
    /// member; bind/lower/package failures name their own kinds).
    pub fn abi_kind(&self) -> &'static str {
        match self {
            HostError::Abi(e) => e.as_str(),
            HostError::BindFailed(e) => e.as_str(),
            HostError::Channel(ChannelError::Schema(e)) => e.as_str(),
            HostError::Channel(ChannelError::Timeout) => AbiError::InvocationTimeout.as_str(),
            HostError::Channel(ChannelError::Eof) | HostError::Channel(ChannelError::Io(_)) => {
                AbiError::PluginCrashed.as_str()
            }
            HostError::Channel(ChannelError::SeqViolation { .. })
            | HostError::Channel(ChannelError::Oversized(_)) => AbiError::SessionDetached.as_str(),
            HostError::Screen(_) => "ScreenViolation",
            HostError::Helper(_) => "HelperError",
            HostError::Package(_) => "PackageError",
            HostError::Lower(_) => "RequestsExceedCap",
            HostError::Io(_) => "IoError",
        }
    }
}
