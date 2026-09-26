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
