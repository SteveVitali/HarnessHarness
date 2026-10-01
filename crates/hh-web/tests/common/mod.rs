//! Shared test support — a stub `hh-embed/1` binding-(c) kernel (pure
//! std TCP: `Send`-safe, unlike the real `EmbedService`) that answers
//! canonical JSON-RPC requests on `POST /hh-embed/1` and records every
//! `(method, params)` it sees, plus the surface fixture builders.

use std::io::BufReader;
use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc::{channel, Receiver};
use std::thread::JoinHandle;

use hh_embed_client_generated::NetClient;
use hh_secrets::{Canary, DetectorSet, MaskSet};
use hh_telemetry::sinks::{ContentClass, SinkPolicy};
use hh_web::gate::GateConfig;
use hh_web::session::Sessions;
use hh_wire::http::{read_request, write_response};
use hh_wire::json::Json;

#[allow(dead_code)]
pub const TOKEN: &str = "surface-token-32-hex-chars-0000000";
#[allow(dead_code)]
pub const KERNEL_TOKEN: &str = "kernel-token-32-hex-chars-00000000";

/// A stub kernel: accepts up to `max` connections; each POST body's
/// `method`/`params` routes to `handler`; `(method, params)` records on
/// the returned receiver in arrival order.
#[allow(dead_code)]
pub fn stub_kernel<F>(
    handler: F,
    max: usize,
) -> (SocketAddr, Receiver<(String, Json)>, JoinHandle<()>)
where
    F: Fn(&str, &Json) -> Json + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = channel();
    let h = std::thread::spawn(move || {
        let mut served = 0usize;
        for conn in listener.incoming() {
            if served >= max {
                break;
            }
            let mut stream = match conn {
                Ok(s) => s,
                Err(_) => continue,
            };
            let served_resp = {
                let mut r = BufReader::new(&stream);
                match read_request(&mut r, 8 * 1024 * 1024) {
                    Ok(Some((_head, body))) => {
                        let req = hh_wire::json::parse(&String::from_utf8_lossy(&body))
                            .unwrap_or(Json::Null);
                        let method = req
                            .get("method")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string();
                        let params = req.get("params").cloned().unwrap_or(Json::Null);
                        let id = req.get("id").cloned().unwrap_or(Json::Null);
                        let _ = tx.send((method.clone(), params.clone()));
                        let result = handler(&method, &params);
                        Json::obj([
                            ("jsonrpc", Json::str("2.0")),
                            ("id", id),
                            ("result", result),
                        ])
                    }
                    _ => continue,
                }
            };
            let body = served_resp.to_canonical_string();
            let _ = write_response(
                &mut stream,
                200,
                &[("Content-Type", "application/json")],
                body.as_bytes(),
            );
            served += 1;
        }
    });
    (addr, rx, h)
}

/// The surface fixture — gate + sessions + detectors against a stub
/// kernel (or a dead address: `kernel_addr` of `None` uses a closed
/// port so calls fail only when the request actually reaches dispatch).
#[allow(dead_code)]
pub fn fixture(kernel_addr: Option<SocketAddr>) -> (GateConfig, Sessions, DetectorSet) {
    let gate = GateConfig {
        authority: "127.0.0.1:7777".into(),
        loopback: true,
        token: TOKEN.into(),
        trusted_origins: Vec::new(),
        allow_missing_fetch_metadata: false,
    };
    let mut policy = SinkPolicy::accounting("hh-web-test");
    policy.content_classes.insert(ContentClass::Structural);
    policy.content_classes.insert(ContentClass::Content);
    policy.requires_consent = true;
    let client = NetClient::connect(
        kernel_addr.unwrap_or_else(|| "127.0.0.1:9".parse().unwrap()),
        KERNEL_TOKEN,
    );
    let svc = Sessions::new(client, policy, "human:principal");
    let detectors = DetectorSet::standard(MaskSet::default());
    (gate, svc, detectors)
}

/// A request head over the canonical gate vocabulary.
#[allow(dead_code)]
pub fn head(method: &str, target: &str, headers: &[(&str, &str)]) -> hh_wire::http::RequestHead {
    let mut h = hh_wire::http::RequestHead {
        method: method.into(),
        target: target.into(),
        headers: Default::default(),
    };
    h.headers.insert("host".into(), "127.0.0.1:7777".into());
    for (k, v) in headers {
        h.headers.insert(k.to_string(), v.to_string());
    }
    h
}

/// The fully-dressed API head (host + bearer + script header + JSON ct).
#[allow(dead_code)]
pub fn api_head() -> hh_wire::http::RequestHead {
    head(
        "POST",
        "/api",
        &[
            ("authorization", "Bearer surface-token-32-hex-chars-0000000"),
            ("x-hh-script", "1"),
            ("content-type", "application/json"),
            ("sec-fetch-site", "same-origin"),
        ],
    )
}

/// The API envelope body.
#[allow(dead_code)]
pub fn api_body(op: &str, params: Json) -> Vec<u8> {
    Json::obj([("op", Json::str(op)), ("params", params)])
        .to_canonical_string()
        .into_bytes()
}

/// A canary detector set — `id → value` tripwires (AC-6(g)).
#[allow(dead_code)]
pub fn canary_detectors(id: &str, value: &str) -> DetectorSet {
    let mut d = DetectorSet::standard(MaskSet::default());
    d.canaries.push(Canary {
        id: id.into(),
        value: value.into(),
    });
    d
}
