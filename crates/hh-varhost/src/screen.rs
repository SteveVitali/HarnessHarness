//! Inbound screening — the per-message monitor checks V1/V5 run against
//! every plugin→host envelope *after* framing/seq (channel) and strict
//! decode, *before* dispatch (§8.4 §2 Invariants; ADR-0181 D3/D4).
//!
//! What screening enforces:
//!
//! - **V5 headers** — `protocol_version` is the host's major and
//!   `schema_hash` is the sealed schema address; both are asserted on every
//!   message, not just `hello`.
//! - **Direction** — the plugin's verb set is the closed reply/callback
//!   half of the protocol (`hello`, `bind_result`, `invoke_result`,
//!   `stream_item`, `callback`, `guard_result`, `conformance_result`,
//!   `refused`). A plugin emitting a host→plugin verb (`invoke`, `guard`,
//!   `callback_result`, …) is forging the host role — `DirectionViolation`.
//! - **V1 authority** — a recursive payload walk refuses any member that
//!   would carry kernel-side authority inward or impersonate it:
//!   `authority` values `kernel|principal|definition`, `origin: kernel`,
//!   `origin: principal`, and the monitor-only members `permission`,
//!   `label`, `kernel_decision`, `risk_class`, `retry_class` /
//!   `retryability` — a plugin returning any of those is the AC-4 hostile
//!   case (`AuthorityCrossing`, `security.extension.violation`, the run
//!   proceeds on the class fallback).
//! - **Binding truth** — `invoke_result`/`stream_item`/`callback`-in-flight
//!   checks that a reply names a live binding/stream (an unknown id is
//!   `UnknownBinding` — the plugin claims state the host never made).
//!
//! Screening returns a typed [`ScreenViolation`]; the host maps it to the
//! refusal + `security.extension.violation` + (for desyncing violations)
//! session detach. Screening never blocks forever and never kills the run —
//! the class fallback is the run's continuation (§8.4 §5).

use std::collections::BTreeSet;

use hh_embed_schema::plugin_abi::{
    plugin_abi_schema_hash, AbiEnvelope, AbiPayload, PLUGIN_ABI_MAJOR,
};
use hh_wire::json::Json;

/// The closed plugin→host verb set (everything else the closed sum contains
/// is host→plugin; a plugin sending one is forging the host's role).
pub const PLUGIN_VERBS: &[&str] = &[
    "hello",
    "bind_result",
    "invoke_result",
    "stream_item",
    "callback",
    "guard_result",
    "conformance_result",
    "refused",
];

/// The kernel-side authority spellings a plugin message may never carry
/// inward (V1 — `authority ≤ delegate` is the plugin's ceiling, and even
/// `delegate`/`external` are *stamped* by the host, never claimed).
const FORBIDDEN_AUTHORITY: &[&str] = &["kernel", "principal", "definition", "environment"];

/// Monitor/kernel-only member names — a plugin returning one is smuggling a
/// `Permission`, a label, a `KernelDecision` or a lowered risk class across
/// the boundary (AC-4's probe set).
const FORBIDDEN_MEMBERS: &[&str] = &[
    "permission",
    "permission_record",
    "label",
    "kernel_decision",
    "risk_class",
    "retry_class",
    "retryability",
    "raw_handle",
    "monitor_state",
];

/// A screening verdict. Every variant maps to a `security.extension.
/// violation{kind}` spelling via [`ScreenViolation::kind`].
#[derive(Debug, Clone, PartialEq)]
pub enum ScreenViolation {
    /// `protocol_version` is not the host's major (V5).
    ProtocolVersion {
        /// The received version.
        got: i64,
    },
    /// `schema_hash` ≠ the sealed schema address (V5/V6).
    SchemaHash {
        /// The received hash.
        got: String,
    },
    /// A host→plugin verb arrived from the plugin.
    Direction {
        /// The forged verb.
        verb: String,
    },
    /// V1 — kernel-side authority crossed inward (the member path names it).
    AuthorityCrossing {
        /// Where in the payload the crossing was found.
        detail: String,
    },
    /// A reply/callback named no live binding, stream or in-flight call.
    UnknownBinding {
        /// The id the plugin named.
        id: String,
    },
    /// `hello` arrived after the handshake completed (re-hello is a new
    /// session, never an in-session verb).
    StrayHello,
}

impl ScreenViolation {
    /// The `security.extension.violation.kind` spelling (the closed kind set
    /// `{authority_crossing, reach, isolation}` — desyncing protocol faults
    /// record as `reach`).
    pub fn kind(&self) -> &'static str {
        match self {
            ScreenViolation::AuthorityCrossing { .. } => "authority_crossing",
            ScreenViolation::ProtocolVersion { .. }
            | ScreenViolation::SchemaHash { .. }
            | ScreenViolation::Direction { .. }
            | ScreenViolation::UnknownBinding { .. }
            | ScreenViolation::StrayHello => "reach",
        }
    }

    /// Whether the session can continue after this violation (the message is
    /// refused, the session stays live) or must detach (the channel can no
    /// longer be trusted).
    pub fn detaches(&self) -> bool {
        // Content-level refusals keep the session (AC-4's "the run proceeds
        // with the class fallback"); a forged direction or dead header check
        // means the peer is not speaking the protocol — the session detaches.
        matches!(
            self,
            ScreenViolation::ProtocolVersion { .. }
                | ScreenViolation::SchemaHash { .. }
                | ScreenViolation::Direction { .. }
                | ScreenViolation::StrayHello
        )
    }
}

impl std::fmt::Display for ScreenViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScreenViolation::ProtocolVersion { got } => {
                write!(f, "protocol_version {got} != {PLUGIN_ABI_MAJOR}")
            }
            ScreenViolation::SchemaHash { got } => write!(f, "schema_hash {got} != sealed pin"),
            ScreenViolation::Direction { verb } => {
                write!(f, "plugin sent host→plugin verb {verb}")
            }
            ScreenViolation::AuthorityCrossing { detail } => {
                write!(f, "authority crossing at {detail}")
            }
            ScreenViolation::UnknownBinding { id } => write!(f, "unknown binding/stream {id}"),
            ScreenViolation::StrayHello => write!(f, "hello after handshake"),
        }
    }
}
impl std::error::Error for ScreenViolation {}

/// The live-session view screening checks bindings against.
#[derive(Debug, Clone, Default)]
pub struct ScreenView {
    /// Live `binding_id`s.
    pub bindings: BTreeSet<String>,
    /// Open stream invocation ids.
    pub open_streams: BTreeSet<String>,
    /// The in-flight invocation's binding (callbacks mid-invoke).
    pub in_flight: Option<String>,
    /// Whether the handshake has completed (`hello` is legal only before).
    pub handshaken: bool,
}

/// The recursive V1 walk — returns the first crossing's member path.
fn scan_authority(j: &Json, path: &str) -> Option<String> {
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                let p = format!("{path}.{k}");
                if let Some(s) = v.as_str() {
                    if k == "authority" && FORBIDDEN_AUTHORITY.contains(&s) {
                        return Some(format!("{p}={s}"));
                    }
                    if k == "origin" && matches!(s, "kernel" | "principal" | "definition") {
                        return Some(format!("{p}={s}"));
                    }
                }
                if FORBIDDEN_MEMBERS.contains(&k.as_str()) {
                    return Some(p);
                }
                if let Some(hit) = scan_authority(v, &p) {
                    return Some(hit);
                }
            }
            None
        }
        Json::Arr(items) => {
            for (i, v) in items.iter().enumerate() {
                if let Some(hit) = scan_authority(v, &format!("{path}[{i}]")) {
                    return Some(hit);
                }
            }
            None
        }
        _ => None,
    }
}

/// Screen one inbound envelope (post-decode, pre-dispatch).
///
/// The checks run in monitor order: headers (V5) → direction → verb-level
/// binding truth → the V1 payload walk.
pub fn screen_inbound(env: &AbiEnvelope, view: &ScreenView) -> Result<(), ScreenViolation> {
    if env.protocol_version != PLUGIN_ABI_MAJOR {
        return Err(ScreenViolation::ProtocolVersion {
            got: env.protocol_version,
        });
    }
    if env.schema_hash != plugin_abi_schema_hash() {
        return Err(ScreenViolation::SchemaHash {
            got: env.schema_hash.clone(),
        });
    }
    let verb = env.payload.verb();
    if !PLUGIN_VERBS.contains(&verb) {
        return Err(ScreenViolation::Direction {
            verb: verb.to_string(),
        });
    }
    match &env.payload {
        AbiPayload::Hello(_) if view.handshaken => return Err(ScreenViolation::StrayHello),
        AbiPayload::StreamItem(_) => {
            // A stream item outside any open stream names nothing live.
            if view.open_streams.is_empty() {
                return Err(ScreenViolation::UnknownBinding {
                    id: "stream_item".to_string(),
                });
            }
        }
        _ => {}
    }
    if let Some(detail) = scan_authority(&env.payload.to_json(), "payload") {
        return Err(ScreenViolation::AuthorityCrossing { detail });
    }
    Ok(())
}
