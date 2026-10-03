//! `AcpArtifact` — the parsed `hh-acp-target/1` session artefact the
//! compiler lowers (§5d.4 D2). `render_session` projects the durable
//! event stream to `session/update` notifications through the
//! single-source lowering table (`hh_compiler::acp`) — the renderer
//! is the consumer, never a second table.
//!
//! Unknown `_`-prefixed members, `_meta` subtrees, and extension ids
//! are preserved byte-for-byte in [`AcpArtifact::ext`] (T3 — nothing
//! silently dropped, nothing read for authority).

use std::collections::BTreeMap;

use hh_compiler::acp::{lower_event, AcpDialect, ACP_ARTEFACT_SCHEMA};
use hh_wire::json::Json;

/// `SessionUpdate` — one rendered `session/update` notification.
/// `source_seq` is the *durable* ledger seq the update renders from
/// (the replay/dedup coordinate — `session/resume{replayFrom}`
/// re-serves updates whose `source_seq > replay_from`).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionUpdate {
    /// The durable event's `seq` (`u64::MAX` for ephemeral renders
    /// that carry no durable source — the stream-delta classes).
    pub source_seq: u64,
    /// The `sessionUpdate` kind (`state_update`, `tool_call_update`,
    /// `agent_message_chunk`, `plan_update`, `usage_update`,
    /// `session/request_permission`, `session/fork`, …).
    pub kind: String,
    /// The update params (the narrowed payload).
    pub params: Json,
}

impl SessionUpdate {
    /// The wire notification frame (`session/update` — params carry
    /// `sessionId` + `update{sessionUpdate, ...}`).
    pub fn to_notification(&self, session_id: &str) -> Json {
        let mut update = BTreeMap::new();
        update.insert("sessionUpdate".to_string(), Json::str(self.kind.clone()));
        if let Json::Obj(m) = &self.params {
            for (k, v) in m {
                update.insert(k.clone(), v.clone());
            }
        }
        // `trace_map` hook (AC-R-2.5.4-5): the durable source seq rides
        // `_meta.hh.source_seq` — an extension member, never authority,
        // byte-preserved through dialects.
        {
            let mut hh = BTreeMap::new();
            hh.insert("source_seq".to_string(), Json::Int(self.source_seq as i64));
            let mut meta = match update.get("_meta") {
                Some(Json::Obj(m)) => m.clone(),
                _ => BTreeMap::new(),
            };
            meta.insert("hh".to_string(), Json::Obj(hh));
            update.insert("_meta".to_string(), Json::Obj(meta));
        }
        Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("method", Json::str("session/update")),
            (
                "params",
                Json::obj([
                    ("sessionId", Json::str(session_id.to_string())),
                    ("update", Json::Obj(update)),
                ]),
            ),
        ])
    }
}

/// `render_session(events, dialect)` — the §5d.4 D3 session render:
/// `events` is the *durable* ledger tail `[(seq, class, payload)]` in
/// seq order; each event lowers to at most one update through the
/// pinned table. The durable-before-visible rule holds by
/// construction: `state_update{idle}` renders only from a durable
/// `lifecycle.turn.finished` event present in the input — a visible
/// `idle` can never precede its durable record (the input *is* the
/// durable stream; speculative frames are never admitted).
///
/// `dialect` filters the update kinds the profile admits — a dropped
/// kind is a `narrowed`/`collapsed` loss the artefact's v1 loss list
/// reports (the projection reports, never silently coerces).
pub fn render_session(events: &[(u64, String, Json)], dialect: AcpDialect) -> Vec<SessionUpdate> {
    let mut out = Vec::new();
    for (seq, class, payload) in events {
        if let Some((kind, params)) = lower_event(class, payload) {
            if dialect.admits(&kind) {
                out.push(SessionUpdate {
                    source_seq: *seq,
                    kind,
                    params,
                });
            }
        }
    }
    out
}

/// `AcpArtifact` — the parsed `hh-acp-target/1` (strict member form —
/// an artefact that isn't the declared schema is refused, never
/// coerced).
#[derive(Debug, Clone)]
pub struct AcpArtifact {
    /// The negotiated-capable dialect versions.
    pub supported_versions: Vec<String>,
    /// The `initialize_response` member (verbatim — the serve loop
    /// answers it at `initialize`, the v1 profile projecting it).
    pub initialize_response: Json,
    /// `config_options[]` — the session-configurable surface.
    pub config_options: Json,
    /// The artefact's `binding` member.
    pub binding: Json,
    /// The `agent_semantic_id` (`agentCapabilities._meta.hh`).
    pub agent_semantic_id: String,
    /// The advertised `_hh/*` method set.
    pub hh_methods: Vec<String>,
    /// The advertised update kinds (v2 set).
    pub update_kinds: Vec<String>,
    /// `ext` — unknown `_`-prefixed members, `_meta` subtrees, and
    /// extension ids preserved byte-for-byte (T3).
    pub ext: BTreeMap<String, Json>,
    /// The artefact verbatim (the serve loop's `server/discover`-style
    /// echo member).
    pub raw: Json,
}

/// The artefact's declared member set — anything else is `ext`
/// (byte-preserved, never authority-bearing).
const DECLARED: [&str; 8] = [
    "schema",
    "protocol",
    "protocol_version",
    "supported_versions",
    "binding",
    "initialize_response",
    "config_options",
    "lowering_table",
];

/// The artefact's *auxiliary* declared members — present at
/// `lower_target` but not part of the session-surface minimum.
const AUX_DECLARED: [&str; 3] = ["tool_kind_table", "spec_version", "target"];

impl AcpArtifact {
    /// Parse + validate the artefact (the strict-member rule — schema
    /// id + `initialize_response` required).
    pub fn from_json(j: &Json) -> Result<AcpArtifact, String> {
        if j.get("schema").and_then(Json::as_str) != Some(ACP_ARTEFACT_SCHEMA) {
            return Err("not a hh-acp-target/1 artefact".to_string());
        }
        let initialize_response = j
            .get("initialize_response")
            .cloned()
            .ok_or_else(|| "initialize_response missing".to_string())?;
        let hh_meta = initialize_response
            .get("agentCapabilities")
            .and_then(|c| c.get("_meta"))
            .and_then(|m| m.get("hh"))
            .cloned()
            .unwrap_or(Json::obj([]));
        let str_arr = |v: Option<&Json>| -> Vec<String> {
            match v {
                Some(Json::Arr(a)) => a
                    .iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect(),
                _ => Vec::new(),
            }
        };
        let mut ext = BTreeMap::new();
        if let Json::Obj(m) = j {
            for (k, v) in m {
                if !DECLARED.contains(&k.as_str()) && !AUX_DECLARED.contains(&k.as_str()) {
                    ext.insert(k.clone(), v.clone());
                }
            }
        }
        Ok(AcpArtifact {
            supported_versions: str_arr(j.get("supported_versions")),
            initialize_response,
            config_options: j
                .get("config_options")
                .cloned()
                .unwrap_or(Json::Arr(vec![])),
            binding: j.get("binding").cloned().unwrap_or(Json::obj([])),
            agent_semantic_id: hh_meta
                .get("agent_semantic_id")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
            hh_methods: str_arr(hh_meta.get("hh_methods")),
            update_kinds: str_arr(hh_meta.get("update_kinds")),
            ext,
            raw: j.clone(),
        })
    }

    /// Whether the `_hh/*` method is advertised (the serve loop's
    /// capability gate — a non-advertised `_hh` method is `-32601`
    /// even if the dialect knows the name).
    pub fn advertises(&self, method: &str) -> bool {
        self.hh_methods.iter().any(|m| m == method)
    }

    /// The `initialize` response under `dialect` — the v1 profile
    /// projects `agentCapabilities` to the collapsed set (drops the
    /// unstable `sessionCapabilities` surface + `agent_thought_chunk`
    /// advertisement; the dropped members report on the v1 loss
    /// list).
    pub fn initialize_response(&self, dialect: AcpDialect) -> Json {
        match dialect {
            AcpDialect::V2 => self.initialize_response.clone(),
            AcpDialect::V1 => {
                let mut resp = self.initialize_response.clone();
                if let Json::Obj(resp_m) = &mut resp {
                    if let Some(Json::Obj(caps)) = resp_m.get_mut("agentCapabilities") {
                        caps.remove("sessionCapabilities");
                        if let Some(Json::Obj(meta)) = caps.get_mut("_meta") {
                            if let Some(Json::Obj(hh)) = meta.get_mut("hh") {
                                hh.insert(
                                    "update_kinds".to_string(),
                                    Json::Arr(
                                        self.update_kinds
                                            .iter()
                                            .filter(|k| dialect.admits(k))
                                            .map(|k| Json::str(k.clone()))
                                            .collect(),
                                    ),
                                );
                            }
                        }
                    }
                    // The v1 profile answers with the negotiated
                    // label — never the v2 pin (the peer re-checks
                    // `negotiate_era` on the reported version).
                    resp_m.insert(
                        "protocolVersion".to_string(),
                        Json::str(crate::protocol::ACP_LEGACY_LABEL.to_string()),
                    );
                }
                resp
            }
        }
    }
}
