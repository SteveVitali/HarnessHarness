//! The ACP edge's pin set + era negotiation + binding record (§5d.4
//! D2/D4 — `ProtocolBinding` machinery is shared with `hh-mcp`; only
//! the pin *contents* are ACP's).

use std::collections::BTreeMap;

use hh_mcp::protocol::{NegotiateError, ProtocolBinding};
use hh_wire::json::Json;

/// The pinned ACP dialect version (v2).
pub const ACP_VERSION: &str = "0.1.0";
/// The v1 compatibility profile label the negotiation accepts.
pub const ACP_LEGACY_LABEL: &str = "v1-compat";

/// The supported version set the pin advertises.
pub const SUPPORTED_VERSIONS: [&str; 2] = [ACP_VERSION, ACP_LEGACY_LABEL];

/// `ACP_PIN_SET` — the schema-family pin digest (`spec_pin.
/// schema_content_hash` for protocol `acp` — derivation-key input).
pub fn acp_pin_set() -> String {
    hh_mcp::protocol::schema_content_hash("acp", &SUPPORTED_VERSIONS)
}

/// The digest as a `&'static`-style constant — computed lazily by
/// callers; the function form is the honest API (the pin is a hash,
/// never a constant to typo).
pub const ACP_PIN_SET_LABEL: &str = "acp@pin-set";

/// `AcpEra` — the negotiated dialect era (`v2` pinned, `v1` the
/// compatibility profile).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpEra {
    /// The pinned dialect.
    V2,
    /// The v1 compatibility profile.
    V1,
}

impl AcpEra {
    /// The canonical label.
    pub fn as_str(self) -> &'static str {
        match self {
            AcpEra::V2 => "v2",
            AcpEra::V1 => "v1",
        }
    }

    /// The compiler-side dialect view (the artefact's projection).
    pub fn dialect(self) -> hh_compiler::acp::AcpDialect {
        match self {
            AcpEra::V2 => hh_compiler::acp::AcpDialect::V2,
            AcpEra::V1 => hh_compiler::acp::AcpDialect::V1,
        }
    }
}

/// `negotiate_era(offered)` — probe-first version negotiation: the
/// peer's `protocolVersion` is checked against the pinned support set.
/// A version outside the set is the typed
/// [`NegotiateError::ProtocolVersionMismatch`] — never coerced, never
/// silently mapped (N2; AC-R-2.5.4-1's pin half).
pub fn negotiate_era(offered: &str) -> Result<AcpEra, NegotiateError> {
    match offered {
        ACP_VERSION => Ok(AcpEra::V2),
        ACP_LEGACY_LABEL | "v1" => Ok(AcpEra::V1),
        other => Err(NegotiateError::ProtocolVersionMismatch {
            requested: other.to_string(),
            supported: SUPPORTED_VERSIONS.iter().map(|v| v.to_string()).collect(),
        }),
    }
}

/// `acp_binding(era, transport, peer_info, capabilities_declared,
/// capabilities_observed, request_target)` — the minted
/// [`ProtocolBinding`] for the ACP edge (`protocol: "acp"`, `role:
/// "client"` — the artefact is a client, binding (c)). `negotiated_at`
/// stays `None` — the caller stamps the `EventRef`.
pub fn acp_binding(
    era: AcpEra,
    transport: &str,
    peer_info: Json,
    capabilities_declared: Json,
    capabilities_observed: BTreeMap<String, hh_mcp::protocol::CapabilitySupport>,
    request_target: Option<String>,
) -> ProtocolBinding {
    ProtocolBinding {
        protocol: "acp".to_string(),
        role: "client".to_string(),
        spec_version_label: "acp".to_string(),
        spec_schema_content_hash: acp_pin_set(),
        era: match era {
            AcpEra::V2 => hh_mcp::protocol::Era::Modern,
            AcpEra::V1 => hh_mcp::protocol::Era::Legacy,
        },
        negotiated_version: match era {
            AcpEra::V2 => ACP_VERSION.to_string(),
            AcpEra::V1 => ACP_LEGACY_LABEL.to_string(),
        },
        peer_info,
        capabilities_declared,
        capabilities_observed,
        extensions: Vec::new(),
        transport: transport.to_string(),
        request_target,
        negotiated_at: None,
    }
}
