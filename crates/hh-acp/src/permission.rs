//! The permission-transport records (§5d.4 P) + the Π seam
//! (AC-R-2.5.4-6 — the transport never approves: the adapter
//! round-trips the request to the kernel's `Π`/`PermissionGate`, and
//! only a resolved allow executes the tool call; a deny lands the
//! terminal `permission_refused` and the tool call never runs —
//! never fall back to `always`/`never`).

use hh_wire::json::Json;

/// `PermissionRequest` — the inbound `session/request_permission`
/// the serve loop hands Π. The verbatim request body is preserved
/// (`params` — Π decides on the tool call's declared surface, never
/// an adapter-summarized view).
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    /// The JSON-RPC request id the response must carry.
    pub request_id: Json,
    /// The session the request binds to.
    pub session_id: String,
    /// The request params verbatim (`{toolCall{…}, options[]}`).
    pub params: Json,
}

impl PermissionRequest {
    /// The tool call descriptor Π evaluates (`params.toolCall`).
    pub fn tool_call(&self) -> Option<&Json> {
        self.params.get("toolCall")
    }

    /// The losses the inbound request carries — agent-side members the
    /// kernel's permission vocabulary cannot express
    /// (AC-R-2.8.7-10's lift leg; named, never silently dropped).
    pub fn losses(&self) -> Vec<String> {
        inbound_losses(&self.params)
    }
}

/// `PermissionOutcome` — what Π decided, mapped back to the wire.
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionOutcome {
    /// Π allowed (optionally under a selected option — `outcome.
    /// outcome: "selected"` + `optionId` on the wire).
    Allow { option_id: Option<String> },
    /// Π denied — the tool call never runs (the terminal
    /// `permission_refused` lands driver-side; the wire response is
    /// `outcome{outcome:"denied"}`).
    Deny,
    /// The session was cancelled while the request stood
    /// (`outcome{outcome:"cancelled"}`).
    Cancelled,
}

impl PermissionOutcome {
    /// The `session/request_permission` response member.
    pub fn to_wire(&self) -> Json {
        match self {
            PermissionOutcome::Allow { option_id } => {
                let mut outcome = BTreeMapForWire::new();
                outcome.insert(
                    "outcome".to_string(),
                    Json::str(if option_id.is_some() {
                        "selected"
                    } else {
                        "allowed"
                    }),
                );
                if let Some(id) = option_id {
                    outcome.insert("optionId".to_string(), Json::str(id.clone()));
                }
                Json::obj([("outcome", Json::Obj(outcome))])
            }
            PermissionOutcome::Deny => {
                Json::obj([("outcome", Json::obj([("outcome", Json::str("denied"))]))])
            }
            PermissionOutcome::Cancelled => {
                Json::obj([("outcome", Json::obj([("outcome", Json::str("cancelled"))]))])
            }
        }
    }

    /// Parse the client's `session/request_permission` response back
    /// (`{outcome{outcome, optionId?}}` — anything else is a deny:
    /// the transport never defaults to allow).
    pub fn from_wire(j: &Json) -> PermissionOutcome {
        let outcome = j
            .get("outcome")
            .and_then(|o| o.get("outcome"))
            .and_then(Json::as_str)
            .unwrap_or("denied");
        match outcome {
            "selected" => PermissionOutcome::Allow {
                option_id: j
                    .get("outcome")
                    .and_then(|o| o.get("optionId"))
                    .and_then(Json::as_str)
                    .map(String::from),
            },
            "allowed" => PermissionOutcome::Allow { option_id: None },
            "cancelled" => PermissionOutcome::Cancelled,
            _ => PermissionOutcome::Deny,
        }
    }
}

type BTreeMapForWire = std::collections::BTreeMap<String, Json>;

// ── AC-R-2.8.7-10 — the permission round-trip's preserved members and
// structured losses (S4.14a) ────────────────────────────────────────────

/// The kernel-request members ACP's `session/request_permission` cannot
/// express — a request carrying any of them lowers with a **named loss**
/// (never a silent narrowing, never a refusal-by-omission): the loss rides
/// the wire under `params._hh.lossReport` so the peer sees exactly which
/// semantics did not round-trip.
pub const UNSUPPORTED_REQUEST_MEMBERS: [&str; 5] =
    ["modify", "escalate", "abort_run", "max_uses", "deadline"];

/// The members the round-trip preserves verbatim under `params._hh` —
/// `permission_id` (the durable ask's coordinate), `effect_id`,
/// `effective_risk_class`, `reason_code`. The lift side correlates on the
/// durable `permission_id` (the pending queue), so these members make the
/// preservation *visible on the wire* — a client never has to guess which
/// kernel ask an ACP request projects.
pub const PRESERVED_REQUEST_MEMBERS: [(&str, &str); 4] = [
    ("permission_id", "permissionId"),
    ("effect_id", "effectId"),
    ("effective_risk_class", "effectiveRiskClass"),
    ("reason_code", "reasonCode"),
];

/// Scan a `security.permission.requested`/`pending` payload for members
/// the ACP permission vocabulary cannot express — each loss is named as
/// `"<json-pointer>: unsupported member <name>"` (top-level members plus
/// `options_presented[i]` member/id spellings). The ACP option kinds a
/// kernel option may carry — `allow_once`, `allow_lease`, `deny`,
/// `more_info` — all lower; anything else is a named loss.
pub fn permission_losses(payload: &Json) -> Vec<String> {
    fn scan(j: &Json, path: &str, losses: &mut Vec<String>) {
        if let Json::Obj(m) = j {
            for name in UNSUPPORTED_REQUEST_MEMBERS {
                if m.contains_key(name) {
                    losses.push(format!("{path}/{name}: unsupported member"));
                }
            }
        }
    }
    let mut losses = Vec::new();
    scan(payload, "", &mut losses);
    if let Some(Json::Arr(options)) = payload.get("options_presented") {
        for (i, o) in options.iter().enumerate() {
            scan(o, &format!("/options_presented/{i}"), &mut losses);
            if let Some(id) = o.get("id").and_then(Json::as_str) {
                if UNSUPPORTED_REQUEST_MEMBERS.contains(&id) {
                    losses.push(format!(
                        "/options_presented/{i}/id: unsupported option kind {id}"
                    ));
                }
            }
        }
    }
    losses
}

/// The structured loss report for a lowered `session/request_permission` —
/// a `hh-edge-loss/1`-shaped record naming every member that did not
/// survive the projection (`None` when the lower lost nothing).
pub fn permission_loss_report(payload: &Json) -> Option<Json> {
    let losses = permission_losses(payload);
    if losses.is_empty() {
        return None;
    }
    Some(Json::obj([
        ("schema", Json::str(hh_compiler::acp::EDGE_LOSS_SCHEMA)),
        (
            "binding",
            Json::obj([("update", Json::str("session/request_permission"))]),
        ),
        (
            "permission_id",
            payload.get("permission_id").cloned().unwrap_or(Json::Null),
        ),
        (
            "losses",
            Json::Arr(losses.iter().map(|l| Json::str(l.clone())).collect()),
        ),
    ]))
}

/// The `params._hh` member for the lowered wire request — the preserved
/// kernel members plus the structured loss report. ACP peers ignore the
/// namespaced member; honest clients can read exactly what the projection
/// kept and what it lost (AC-R-2.8.7-10).
pub fn preserved_wire_member(payload: &Json) -> Json {
    let mut hh = BTreeMapForWire::new();
    for (kernel, wire) in PRESERVED_REQUEST_MEMBERS {
        // `reason_code` is a documented spelling; the request payload's
        // canonical member is `reason` — preserve it under the wire name.
        let v = payload.get(kernel).or_else(|| {
            (kernel == "reason_code")
                .then(|| payload.get("reason"))
                .flatten()
        });
        if let Some(v) = v {
            hh.insert(wire.to_string(), v.clone());
        }
    }
    if let Some(report) = permission_loss_report(payload) {
        hh.insert("lossReport".to_string(), report);
    }
    Json::Obj(hh)
}

/// The losses an inbound `session/request_permission`'s params carry —
/// the agent-side members the kernel's `PermissionRequest` cannot express
/// (`options[].kind` spellings outside the Π vocabulary, `maxUses`,
/// `deadline`). The seam surfaces them on the request so the Π gate can
/// name — never silently drop — what it could not express back.
pub fn inbound_losses(params: &Json) -> Vec<String> {
    let mut losses = Vec::new();
    if let Some(Json::Arr(options)) = params.get("options") {
        for (i, o) in options.iter().enumerate() {
            if let Json::Obj(m) = o {
                for name in ["maxUses", "deadline"] {
                    if m.contains_key(name) {
                        losses.push(format!("/options/{i}/{name}: unsupported member"));
                    }
                }
                if let Some(kind) = o
                    .get("kind")
                    .and_then(Json::as_str)
                    .or_else(|| o.get("optionId").and_then(Json::as_str))
                {
                    if UNSUPPORTED_REQUEST_MEMBERS.contains(&kind) {
                        losses.push(format!("/options/{i}: unsupported option kind {kind}"));
                    }
                }
            }
        }
    }
    losses
}

/// `PiGate` — the kernel's `Π`/`PermissionGate` surface the
/// permission transport consults (the seam — `hh-env`'s
/// `PermissionGate` implements this through the embed layer; tests
/// use [`StaticPi`]). The adapter calls `decide` and forwards the
/// outcome verbatim — it never decides.
pub trait PiGate {
    /// Decide the request — `Allow`/`Deny`/`Cancelled` (a `Deny`
    /// lands the terminal `permission_refused` driver-side; the tool
    /// call never executes).
    fn decide(&mut self, request: &PermissionRequest) -> PermissionOutcome;
}

/// `StaticPi` — the fixture Π: a default outcome plus optional
/// per-tool-call-kind overrides. Deny-by-default on an empty map is
/// the safe posture; tests set `default` explicitly.
pub struct StaticPi {
    /// The outcome when no override matches.
    pub default: PermissionOutcome,
}

impl StaticPi {
    /// `new(default)` — the single-outcome fixture.
    pub fn new(default: PermissionOutcome) -> Self {
        Self { default }
    }
}

impl PiGate for StaticPi {
    fn decide(&mut self, _request: &PermissionRequest) -> PermissionOutcome {
        self.default.clone()
    }
}
