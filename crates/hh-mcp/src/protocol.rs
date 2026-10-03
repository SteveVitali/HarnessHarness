//! The protocol-edge negotiation vocabulary (§5d.4; ADR-0099 N1–N3;
//! ticket S3.9): the pinned version set, the `Era` sum, the typed
//! `NegotiateError` refusal, the omitted→`unknown` capability reconcile
//! and the `ProtocolBinding` runtime record.
//!
//! `ProtocolBinding` is a *runtime record* — a run-manifest member
//! referenced by `lifecycle.run.created`, never an IR entity and never
//! a ledger event class (the dependency-direction invariant,
//! AC-R-2.5.4-8). It carries the peer's declared capabilities verbatim
//! (the wire form, preserved) plus the reconciled `capabilities_observed`
//! tri-state map; secrets never cross an edge in it (T5 — it holds
//! references only).

use std::collections::{BTreeMap, BTreeSet};

use hh_wire::json::Json;

/// The pinned modern MCP revision (§5d.4 D2 — `spec_pin.version_label`).
pub const PINNED_MODERN: &str = "2026-07-28";

/// The one pinned legacy revision — the era the compatibility probe
/// detects and drives (ADR-0099 N1; the fixture's legacy spelling is
/// [`crate::artifact::PROTOCOL_VERSION`]).
pub const PINNED_LEGACY: &str = crate::artifact::PROTOCOL_VERSION;

/// The pinned support set, modern-first preference order (N2: anything
/// outside this set is a typed error, never a silent fallback).
pub const PINNED_VERSIONS: [&str; 2] = [PINNED_MODERN, PINNED_LEGACY];

/// The record's schema id.
pub const BINDING_SCHEMA: &str = "hh-protocol-binding/1";

/// `era ∈ {modern, legacy}` (ADR-0099; an era is an expiring thing —
/// N5's assumption-debt record is attached by the caller, not the edge).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Era {
    /// The pinned modern revision.
    Modern,
    /// The pinned legacy revision.
    Legacy,
}

impl Era {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Era::Modern => "modern",
            Era::Legacy => "legacy",
        }
    }

    /// The era a negotiated version belongs to (`None` outside the
    /// pinned set).
    pub fn of_version(version: &str) -> Option<Era> {
        match version {
            PINNED_MODERN => Some(Era::Modern),
            PINNED_LEGACY => Some(Era::Legacy),
            _ => None,
        }
    }
}

/// `capabilities_observed[field] ∈ {supported, unsupported, unknown}` —
/// N3: the wire's "omitted = unsupported" default re-maps to `unknown`
/// at analysis (CF-027 generalised; a probe that was never answered is
/// not evidence of absence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CapabilitySupport {
    /// Declared (and, where probed, confirmed).
    Supported,
    /// Declared absent/disabled (`false`) by the peer.
    Unsupported,
    /// Never declared — the honest middle state.
    Unknown,
}

impl CapabilitySupport {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CapabilitySupport::Supported => "supported",
            CapabilitySupport::Unsupported => "unsupported",
            CapabilitySupport::Unknown => "unknown",
        }
    }
}

/// `negotiate`'s typed refusal sum (§5d.4 §2; ADR-0099 N2 — never a
/// warning, never a silent fallback).
#[derive(Debug, Clone, PartialEq)]
pub enum NegotiateError {
    /// The peer offered nothing in the pinned support set.
    ProtocolVersionMismatch {
        /// What the client requested.
        requested: String,
        /// What the peer reported supporting.
        supported: Vec<String>,
    },
    /// A capability the client requires reconciled to `unsupported` or
    /// `unknown` (omitted).
    RequiredExtensionUnsupported {
        /// The capability field (`tools`, `tools.listChanged`, …).
        uri: String,
    },
    /// The peer never answered a probe — EOF, a transport fault, or a
    /// frame that never decoded.
    PeerUnreachable {
        /// What happened.
        detail: String,
    },
}

impl NegotiateError {
    /// The typed spelling (ledger-safe — `control.protocol.mismatch`
    /// payloads carry this).
    pub fn refusal(&self) -> &'static str {
        match self {
            NegotiateError::ProtocolVersionMismatch { .. } => "ProtocolVersionMismatch",
            NegotiateError::RequiredExtensionUnsupported { .. } => "RequiredExtensionUnsupported",
            NegotiateError::PeerUnreachable { .. } => "PeerUnreachable",
        }
    }
}

impl std::fmt::Display for NegotiateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NegotiateError::ProtocolVersionMismatch {
                requested,
                supported,
            } => write!(
                f,
                "ProtocolVersionMismatch{{requested: {requested}, supported: {supported:?}}}"
            ),
            NegotiateError::RequiredExtensionUnsupported { uri } => {
                write!(f, "RequiredExtensionUnsupported{{{uri}}}")
            }
            NegotiateError::PeerUnreachable { detail } => {
                write!(f, "PeerUnreachable{{{detail}}}")
            }
        }
    }
}

impl std::error::Error for NegotiateError {}

/// Flatten a wire `capabilities` object into dotted fields —
/// `{tools: {listChanged: true}}` → `{"tools", "tools.listChanged"}`.
/// Leaf `false` values map to `Unsupported` (declared-absent); every
/// other present member maps to `Supported`.
fn flatten(declared: &Json, prefix: &str, out: &mut BTreeMap<String, CapabilitySupport>) {
    if let Json::Obj(m) = declared {
        for (k, v) in m {
            let field = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            if matches!(v, Json::Bool(false)) {
                out.insert(field, CapabilitySupport::Unsupported);
            } else {
                out.insert(field.clone(), CapabilitySupport::Supported);
                flatten(v, &field, out);
            }
        }
    }
}

/// `reconcile(capabilities_declared, probed)` — the N3 map: every
/// declared field is `supported` (or `unsupported` on a declared
/// `false`); every probed field the peer never declared is `unknown`
/// (omitted ≠ unsupported). `probed` is the set of dotted fields the
/// client actually exercised during negotiation.
pub fn reconcile_capabilities(
    declared: &Json,
    probed: &BTreeSet<String>,
) -> BTreeMap<String, CapabilitySupport> {
    let mut out = BTreeMap::new();
    flatten(declared, "", &mut out);
    for p in probed {
        out.entry(p.clone()).or_insert(CapabilitySupport::Unknown);
    }
    out
}

/// `ProtocolBinding` (§5d.4 §3; ADR-0099 D1) — the runtime record every
/// peer connection mints: `{protocol, role, spec_pin{version_label,
/// schema_content_hash}, era, negotiated_version, peer_info,
/// capabilities_declared, capabilities_observed, extensions[],
/// transport, negotiated_at}`.
///
/// `negotiated_at` is an `EventRef` the *caller* stamps (the edge holds
/// no ledger); `peer_info` is the peer's `serverInfo`/`clientInfo`
/// verbatim — an `Implementation` claim, display-only (T2/D5).
#[derive(Debug, Clone, PartialEq)]
pub struct ProtocolBinding {
    /// `mcp` at this edge (the sum admits `acp`/`a2a` at later stages).
    pub protocol: String,
    /// `client` | `server` (`agent`/`peer` belong to other edges).
    pub role: String,
    /// `spec_pin.version_label` — the schema family this binding pins.
    pub spec_version_label: String,
    /// `spec_pin.schema_content_hash` — enters the bundle derivation key.
    pub spec_schema_content_hash: String,
    /// The negotiated era.
    pub era: Era,
    /// `negotiated_version ∈ spec_pin.support_set` — always pinned.
    pub negotiated_version: String,
    /// The peer's `Implementation` claim (`serverInfo`/`clientInfo`,
    /// verbatim — never read for a decision).
    pub peer_info: Json,
    /// `capabilities_declared` — the wire form, preserved.
    pub capabilities_declared: Json,
    /// `capabilities_observed` — the reconciled tri-state map.
    pub capabilities_observed: BTreeMap<String, CapabilitySupport>,
    /// Negotiated extensions.
    pub extensions: Vec<String>,
    /// `stdio` at this edge (`streamable_http` is a C1 transport).
    pub transport: String,
    /// `request_target` — the transport's request URI (§5d.4 D2's
    /// `binding.request_target`; RFC 8707's resource indicator when the
    /// edge runs OAuth). `None` on stdio.
    pub request_target: Option<String>,
    /// `negotiated_at: EventRef` — stamped by the caller; `None` at the
    /// edge itself.
    pub negotiated_at: Option<String>,
}

impl ProtocolBinding {
    /// Canonical JSON (the run-manifest member form).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(BINDING_SCHEMA)),
            ("protocol", Json::str(self.protocol.clone())),
            ("role", Json::str(self.role.clone())),
            (
                "spec_pin",
                Json::obj([
                    ("version_label", Json::str(self.spec_version_label.clone())),
                    (
                        "schema_content_hash",
                        Json::str(self.spec_schema_content_hash.clone()),
                    ),
                ]),
            ),
            ("era", Json::str(self.era.as_str())),
            (
                "negotiated_version",
                Json::str(self.negotiated_version.clone()),
            ),
            ("peer_info", self.peer_info.clone()),
            ("capabilities_declared", self.capabilities_declared.clone()),
            (
                "capabilities_observed",
                Json::Obj(
                    self.capabilities_observed
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::str(v.as_str())))
                        .collect(),
                ),
            ),
            (
                "extensions",
                Json::Arr(
                    self.extensions
                        .iter()
                        .map(|e| Json::str(e.clone()))
                        .collect(),
                ),
            ),
            ("transport", Json::str(self.transport.clone())),
            (
                "request_target",
                self.request_target.clone().map_or(Json::Null, Json::Str),
            ),
            (
                "negotiated_at",
                self.negotiated_at.clone().map_or(Json::Null, Json::Str),
            ),
        ])
    }
}

/// `negotiate`'s version-selection half (pure — the probe order lives
/// in [`crate::client`]): pick the highest-preference pinned member the
/// peer's `supported` set admits; `ProtocolVersionMismatch` when the
/// intersection is empty.
pub fn pin_version(supported: &[String]) -> Result<String, NegotiateError> {
    for pinned in PINNED_VERSIONS {
        if supported.iter().any(|s| s == pinned) {
            return Ok(pinned.to_string());
        }
    }
    Err(NegotiateError::ProtocolVersionMismatch {
        requested: PINNED_MODERN.to_string(),
        supported: supported.to_vec(),
    })
}

/// The `spec_pin.schema_content_hash` for an edge — `idp` over the
/// pinned version set (a content address, never a guess).
pub fn schema_content_hash(protocol: &str, pinned: &[&str]) -> String {
    let doc = Json::obj([
        ("protocol", Json::str(protocol.to_string())),
        (
            "pinned",
            Json::Arr(pinned.iter().map(|v| Json::str(v.to_string())).collect()),
        ),
    ]);
    hh_identity::idp_id("protocol.spec_pin", doc.to_canonical_string().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_prefers_modern_then_legacy_then_mismatch() {
        let both = vec![PINNED_LEGACY.to_string(), PINNED_MODERN.to_string()];
        assert_eq!(pin_version(&both).unwrap(), PINNED_MODERN);
        let legacy = vec![PINNED_LEGACY.to_string()];
        assert_eq!(pin_version(&legacy).unwrap(), PINNED_LEGACY);
        let err = pin_version(&["1999-01-01".to_string()]).unwrap_err();
        assert!(matches!(
            err,
            NegotiateError::ProtocolVersionMismatch { .. }
        ));
    }

    #[test]
    fn reconcile_marks_omitted_unknown_not_unsupported() {
        let declared = Json::obj([("tools", Json::obj([("listChanged", Json::Bool(true))]))]);
        let probed: BTreeSet<String> =
            ["tools", "tools.listChanged", "elicitation.form", "logging"]
                .iter()
                .map(|s| s.to_string())
                .collect();
        let rec = reconcile_capabilities(&declared, &probed);
        assert_eq!(rec["tools"], CapabilitySupport::Supported);
        assert_eq!(rec["tools.listChanged"], CapabilitySupport::Supported);
        // N3 — never declared: unknown, never unsupported.
        assert_eq!(rec["elicitation.form"], CapabilitySupport::Unknown);
        assert_eq!(rec["logging"], CapabilitySupport::Unknown);
        // A declared `false` is the unsupported spelling.
        let d2 = Json::obj([("tools", Json::Bool(false))]);
        let rec2 = reconcile_capabilities(&d2, &probed);
        assert_eq!(rec2["tools"], CapabilitySupport::Unsupported);
    }

    #[test]
    fn binding_is_canonical_and_secret_free() {
        let b = ProtocolBinding {
            protocol: "mcp".to_string(),
            role: "client".to_string(),
            spec_version_label: "mcp".to_string(),
            spec_schema_content_hash: schema_content_hash("mcp", &PINNED_VERSIONS),
            era: Era::Modern,
            negotiated_version: PINNED_MODERN.to_string(),
            peer_info: Json::obj([("name", Json::str("srv"))]),
            capabilities_declared: Json::obj([]),
            capabilities_observed: BTreeMap::new(),
            extensions: vec![],
            transport: "stdio".to_string(),
            request_target: None,
            negotiated_at: None,
        };
        let j = b.to_json();
        assert_eq!(
            j.get("negotiated_version").and_then(Json::as_str),
            Some(PINNED_MODERN)
        );
        assert_eq!(j.get("era").and_then(Json::as_str), Some("modern"));
    }
}
