//! `hh-gateway-stub` — the out-of-process recording/fake gateway
//! (AC-R-2.3.1-11): the same `decode`/attempt path runs out-of-process over a
//! canonical JSON stdin/stdout boundary (the `hh-compile` pattern). A request
//! names a dialect descriptor, an endpoint allow-list, and the wire frames a
//! fake transport replays; the response carries every emitted `model.*`
//! payload and the terminal — a deterministic recording of the exact event
//! stream a live run would append.
//!
//! Input:
//! ```json
//! {"command": "run_call",
//!  "dialect": {<WireDialect/1>},
//!  "endpoint_allowlist": ["ep.test"],
//!  "model_call_id": "mc-1",
//!  "normalizer_ref": "profile:x#usage_mapping",
//!  "attempt_policy": {"max_attempts": 3, ...},
//!  "plan": {<ProviderRequestPlan/1-ish>},
//!  "frames": [{"at_ms": 0, "frame": {"type": "message_start", ...}}, ...]}
//! ```
//! Output:
//! ```json
//! {"events": [{"class": "model.call.requested", "payload": {...}}, ...],
//!  "result": "completed" | "failed" | "refused",
//!  "message": {...} | "error": {...} | "refusal": "..."}
//! ```

use std::io::Read;

use hh_gateway::attempts::CollectSink;
use hh_gateway::codec::WireFrame;
use hh_gateway::dialect::WireDialect;
use hh_gateway::gateway::{
    CredentialHandle, CredentialPort, EndpointAllowlist, ModelGateway, Transport,
};
use hh_gateway::plan::{CachePlan, Capabilities, Extensions, ProviderRequestPlan};
use hh_gateway::vocab::{AttemptPolicy, ModelError, Purpose};
use hh_wire::json::{parse, Json};

/// The fake transport — replays the request's `frames` verbatim (the
/// recording fixture).
struct FakeTransport {
    frames: Vec<(u64, WireFrame)>,
}

impl Transport for FakeTransport {
    fn send(
        &mut self,
        _bytes: &[u8],
        _credential: &CredentialHandle,
        _endpoint_ref: &str,
    ) -> Result<Vec<(u64, WireFrame)>, ModelError> {
        Ok(self.frames.clone())
    }
}

/// The stub credential port — `credential_ref` present ⇒ `provided` (the
/// handle is a reference; no secret ever crosses the boundary).
struct StubCredentials;

impl CredentialPort for StubCredentials {
    fn bind(
        &mut self,
        credential_ref: Option<&str>,
        _endpoint_ref: &str,
        auth: &[hh_gateway::dialect::AuthKind],
    ) -> Result<CredentialHandle, hh_gateway::errors::GatewayError> {
        let needs = auth
            .iter()
            .any(|a| *a != hh_gateway::dialect::AuthKind::None);
        if needs && credential_ref.is_none() {
            return Err(hh_gateway::errors::GatewayError::CredentialUnavailable {
                detail: "dialect requires an auth kind; no credential_ref".into(),
            });
        }
        Ok(CredentialHandle {
            binding_id: credential_ref.map(str::to_string),
            provided: credential_ref.is_some(),
        })
    }
}

fn str_member(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(Json::as_str).map(str::to_string)
}

fn main() {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        eprintln!("stdin read failed");
        std::process::exit(2);
    }
    let req = match parse(&input) {
        Ok(j) => j,
        Err(e) => {
            println!("{{\"result\":\"refused\",\"refusal\":\"bad json: {e}\"}}");
            std::process::exit(0);
        }
    };
    if str_member(&req, "command").as_deref() != Some("run_call") {
        println!("{{\"result\":\"refused\",\"refusal\":\"unknown command\"}}");
        std::process::exit(0);
    }
    // Load the dialect (strict — incomplete debt / unknown members refuse).
    let dialect = match req.get("dialect").map(WireDialect::from_json) {
        Some(Ok(d)) => d,
        Some(Err(e)) => {
            println!(
                "{}",
                Json::obj([
                    ("result", Json::str("refused")),
                    ("refusal", Json::str(format!("{e}"))),
                ])
                .to_canonical_string()
            );
            std::process::exit(0);
        }
        None => {
            println!("{{\"result\":\"refused\",\"refusal\":\"missing dialect\"}}");
            std::process::exit(0);
        }
    };
    let mut endpoints = EndpointAllowlist::default();
    if let Some(Json::Arr(eps)) = req.get("endpoint_allowlist") {
        for e in eps {
            if let Some(s) = e.as_str() {
                endpoints.endpoints.insert(s.to_string());
            }
        }
    }
    // The frames the fake transport replays.
    let frames: Vec<(u64, WireFrame)> = req
        .get("frames")
        .and_then(|f| match f {
            Json::Arr(items) => Some(items.clone()),
            _ => None,
        })
        .unwrap_or_default()
        .iter()
        .filter_map(|it| {
            let at = it.get("at_ms").and_then(Json::as_int).unwrap_or(0) as u64;
            let f = it.get("frame").unwrap_or(it);
            WireFrame::from_json(f).ok().map(|f| (at, f))
        })
        .collect();
    // The request + plan (minimal members — the stub exercises the call path,
    // not plan-schema edge cases; the strict record codecs are unit-tested).
    let plan_j = req.get("plan").cloned().unwrap_or(Json::Null);
    let model_call_id = str_member(&req, "model_call_id").unwrap_or_else(|| "mc-1".into());
    let model_ref = hh_gateway::plan::ModelRef {
        profile_ref: str_member(&req, "profile_ref").unwrap_or_else(|| "prof.test".into()),
        provider_model_id: str_member(&plan_j, "model_ref").unwrap_or_else(|| "m.test".into()),
        snapshot_id: str_member(&plan_j, "snapshot_id"),
        serving_route: str_member(&plan_j, "serving_route"),
        effort: str_member(&plan_j, "effort"),
    };
    let plan = ProviderRequestPlan {
        dialect_id: dialect.dialect_id.clone(),
        dialect_version: dialect.version.clone(),
        endpoint_ref: str_member(&plan_j, "endpoint_ref").unwrap_or_else(|| "ep.test".into()),
        model_ref: model_ref.provider_model_id.clone(),
        body: plan_j.get("body").cloned().unwrap_or(Json::Null),
        provider_params: Extensions::default(),
        capabilities: Capabilities {
            streaming: true,
            tool_use: true,
            vision: None,
            deferred: None,
        },
        cache: CachePlan::default(),
        budget_reservation_id: str_member(&plan_j, "budget_reservation_id"),
        canonical_len: 0,
    };
    let request = hh_gateway::plan::InferenceRequest {
        model_call_id: model_call_id.clone(),
        model_ref,
        plan,
        role: str_member(&req, "role").unwrap_or_else(|| "primary".into()),
        purpose: Purpose::parse(&str_member(&req, "purpose").unwrap_or_else(|| "main".into()))
            .unwrap_or(Purpose::Main),
        context_label: None,
        view_hash: None,
        binding_ref: str_member(&plan_j, "binding_ref").unwrap_or_else(|| "b.test".into()),
        credential_ref: str_member(&plan_j, "credential_ref"),
        budget_reservation_id: str_member(&plan_j, "budget_reservation_id"),
        attempt_policy: AttemptPolicy::c0(),
        deadline_ms: None,
        cache_state_hint: hh_gateway::cache::ExpectedState::Unknown,
        seed_request: None,
        observability: hh_gateway::plan::CallObservability::default(),
        request_class: hh_gateway::plan::RequestClass::Interactive,
        deferred_deadline_ms: None,
        stream: true,
        identity: None,
    };
    let mut transport = FakeTransport { frames };
    let mut credentials = StubCredentials;
    let mut gateway = ModelGateway {
        dialects: Default::default(),
        endpoints,
        credentials: &mut credentials,
        transport: &mut transport,
        now_ms: Box::new(|| 0),
        normalizer_ref: str_member(&req, "normalizer_ref")
            .unwrap_or_else(|| "profile:stub#usage_mapping".into()),
    };
    gateway.load_dialect(dialect);
    let mut sink = CollectSink::default();
    let (result, message_or_error) = match gateway.open_call(request, &mut sink) {
        Err(e) => ("refused", Json::str(format!("{e}"))),
        Ok(handle) => match gateway.complete(&handle, &mut sink) {
            Ok(m) => (
                "completed",
                Json::obj([
                    ("stop_reason", Json::str(m.stop_reason.as_str())),
                    ("blocks", Json::Int(m.blocks.len() as i64)),
                    ("text", Json::str(m.text())),
                ]),
            ),
            Err(e) => ("failed", Json::str(format!("{e}"))),
        },
    };
    let events: Vec<Json> = sink
        .events
        .iter()
        .map(|(class, payload)| {
            Json::obj([
                ("class", Json::str(class.clone())),
                ("payload", payload.clone()),
            ])
        })
        .collect();
    let out_key = if result == "completed" {
        "message"
    } else {
        "error"
    };
    println!(
        "{}",
        Json::obj([
            ("events", Json::Arr(events)),
            ("result", Json::str(result)),
            (out_key, message_or_error),
        ])
        .to_canonical_string()
    );
}
