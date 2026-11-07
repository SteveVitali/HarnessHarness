//! R6.10 — PROXY-CORE: `hh-model-proxy` acceptance battery
//! (ticket 263; experiments spec §4.1–§4.3; REQ-XC-101/103/105/106/
//! 107/108/109/208; AR-08/AR-10).
//!
//! Every call hits a fixture upstream over loopback (REQ-XC-125 —
//! offline only). The battery covers: the slot/token gate (AC-2/AC-4),
//! the §4.2 route table + field policy + pins (AC-1/AC-3), the §4.3
//! ledger order + durable-before-visible release (AC-5/AC-6/AC-8),
//! receipt leak-scan (AC-9, the R6.10 leg of DF-R6.7-1), and the Json
//! control channel over the real binary (AC-7).

use std::collections::BTreeMap;
use std::io::{BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use hharness_hosting::proxy::budget::ProxyAuthorization;
use hharness_hosting::proxy::lane::{ProxyLane, Reservation};
use hharness_hosting::proxy::{Proxy, ProxyConfig};
use hharness_ledger::store::Store;
use hharness_wire::http::read_response;
use hharness_wire::json::{parse, parse_provider, Json};

const MODEL: &str = "glm-5.3-flash";

/// A fixture authorization granting `cap` µ$ (m = 0 keeps the R6.10
/// numbers; REQ-XC-004 — the verbatim citation rides the manifest).
fn auth(cap: i64) -> ProxyAuthorization {
    ProxyAuthorization {
        verbatim: "fixture authorization — R6.10 battery".into(),
        gate_record_ref: "gate.fixture".into(),
        authorized_micros: cap,
        available_credit_micros: cap,
        m_ppm: 0,
        prior_windows: vec![],
    }
}

/// A scratch store root (per-test, unique).
fn root(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "hh-r6-10-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// One upstream exchange the fixture recorded.
#[derive(Debug, Clone)]
struct Seen {
    method: String,
    target: String,
    body: String,
}

/// The fixture upstream — a canned per-path JSON responder that
/// records every exchange for assertions.
fn fixture_upstream() -> (String, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::<Seen>::new()));
    let seen_w = seen.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut stream) = conn else { return };
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let Ok(Some((head, body))) = hharness_wire::http::read_request(&mut r, 4 << 20) else {
                return;
            };
            seen_w.lock().unwrap().push(Seen {
                method: head.method.clone(),
                target: head.target.clone(),
                body: String::from_utf8_lossy(&body).to_string(),
            });
            let (status, payload): (u16, &str) = match head.target.as_str() {
                "/v1/chat/completions" => (
                    200,
                    "{\"id\":\"fx\",\"object\":\"chat.completion\",\"model\":\"glm-5.3-flash\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":40,\"completion_tokens\":10,\"total_tokens\":50}}",
                ),
                "/v1/messages" => (
                    200,
                    "{\"id\":\"fx\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"glm-5.3-flash\",\"content\":[{\"type\":\"text\",\"text\":\"ok\"}],\"stop_reason\":\"end_turn\",\"usage\":{\"input_tokens\":30,\"output_tokens\":12}}",
                ),
                "/v1/models" => (200, "{\"object\":\"list\",\"data\":[{\"id\":\"glm-5.3-flash\"}]}"),
                _ => (404, "{\"error\":{\"type\":\"not_found\"}}"),
            };
            let resp = format!(
                "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\nx-request-id: fx-1\r\nset-cookie: drop=me\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    (format!("http://{addr}/v1"), seen)
}

/// One proxy over a fresh root + fixture upstream.
fn setup(tag: &str, reservation: Reservation) -> (Proxy, String, Arc<Mutex<Vec<Seen>>>, PathBuf) {
    let (endpoint, seen) = fixture_upstream();
    let store_root = root(tag);
    let mut lane = ProxyLane::program(
        "lane.fixture",
        &endpoint,
        "fixture.local",
        MODEL,
        reservation,
    );
    // The R6.11 catalog probe is its own battery — this fixture lane
    // opts out with a dead base (the probe records `probe_error`,
    // never an upstream exchange).
    lane.catalog_base = Some("http://127.0.0.1:1".to_string());
    let cfg = ProxyConfig {
        store_root: store_root.clone(),
        run_id: RUN.to_string(),
        lane,
        listen: "127.0.0.1:0".into(),
        credential_env: None,
        authorization: auth(100_000_000),
        mode: hharness_hosting::proxy::ProxyMode::Live,
    };
    let (mut proxy, _bound) = Proxy::configure(cfg).unwrap();
    let token = proxy.register_slot("slot-A", "high", 50_000_000).unwrap();
    (proxy, token, seen, store_root)
}

/// One synchronous downstream call — write the request, `tick` the
/// proxy once, read the response. `(status, body, headers)`.
fn call(
    proxy: &mut Proxy,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &str,
    extra: &[(&str, &str)],
) -> (u16, String, BTreeMap<String, String>) {
    let bound = proxy.bound_addr();
    let mut s = TcpStream::connect(bound).unwrap();
    if let Some(t) = token {
        let req = format!(
            "{method} {path} HTTP/1.1\r\nhost: proxy.local\r\ncontent-type: application/json\r\nauthorization: Bearer {t}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        s.write_all(req.as_bytes()).unwrap();
    } else {
        let mut req = format!(
            "{method} {path} HTTP/1.1\r\nhost: proxy.local\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n",
            body.len()
        );
        for (k, v) in extra {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        req.push_str(&format!("\r\n{body}"));
        s.write_all(req.as_bytes()).unwrap();
    }
    // The listener is nonblocking — poll until the queued
    // connection is accepted and fully handled (bounded).
    for _ in 0..2000 {
        if proxy.tick() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    s.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    let mut r = BufReader::new(s);
    let (head, rbody) = read_response(&mut r, 8 << 20).unwrap();
    (
        head.status,
        String::from_utf8_lossy(&rbody).to_string(),
        head.headers,
    )
}

/// The run's committed event classes in order.
fn classes(store_root: &PathBuf, run_id: &str) -> Vec<String> {
    let store = Store::open(store_root).unwrap();
    store
        .envelopes(run_id)
        .unwrap()
        .iter()
        .map(|e| e.class.clone())
        .collect()
}

fn chat_body(extra: &str) -> String {
    format!(
        "{{\"model\":\"anything\",\"messages\":[{{\"role\":\"user\",\"content\":\"hi\"}}]{extra}}}"
    )
}

const RUN: &str = "phase-proxy";

// ─── AC-1/AC-2: the happy path — forwarded, pinned, ledgered ───

#[test]
fn forwarded_call_pins_records_and_ledgers() {
    let (mut p, token, seen, root) =
        setup("happy", Reservation::FixtureBound { micros: 1_000_000 });
    let (status, body, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some(&token),
        &chat_body(",\"model\":\"vendor-x\",\"reasoning_effort\":\"low\",\"temperature\":0.7"),
        &[],
    );
    assert_eq!(status, 200);
    assert!(body.contains("\"ok\""));
    // upstream saw the pinned model + effort (overwrite recorded) and
    // the forwarded temperature
    let up = seen.lock().unwrap()[0].body.clone();
    let upj = parse_provider(&up).unwrap();
    assert_eq!(upj.get("model").and_then(Json::as_str), Some(MODEL));
    assert_eq!(
        upj.get("reasoning_effort").and_then(Json::as_str),
        Some("high")
    );
    assert!(up.contains("\"temperature\":0.7"));
    // the ledger carries the §4.3 order
    let cs = classes(&root, RUN);
    let pos = |c: &str| cs.iter().position(|x| x == c).unwrap();
    assert!(pos("model.call.requested") < pos("control.budget.reserved"));
    assert!(pos("control.budget.reserved") < pos("model.call.attempt.started"));
    assert!(pos("model.call.attempt.started") < pos("model.call.attempt.completed"));
    assert!(pos("model.call.attempt.completed") < pos("model.call.completed"));
    assert!(pos("model.call.completed") < pos("measurement.cost.attributed"));
    assert!(pos("measurement.cost.attributed") < pos("control.budget.released"));
}

// ─── AC-2/AC-4: slot-token gate ───

#[test]
fn unknown_and_revoked_tokens_refuse_401() {
    let (mut p, token, seen, _r) = setup("gate", Reservation::FixtureBound { micros: 1_000_000 });
    // no token → refused_auth
    let (s, b, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        None,
        &chat_body(""),
        &[],
    );
    assert_eq!(s, 401);
    assert!(b.contains("refused_auth"));
    // wrong token → refused_auth
    let (s, _b, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some("wrong-token"),
        &chat_body(""),
        &[],
    );
    assert_eq!(s, 401);
    // live token → forwards
    let (s, _, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some(&token),
        &chat_body(""),
        &[],
    );
    assert_eq!(s, 200);
    // revoke → the same token now refuses
    p.revoke_slot("slot-A").unwrap();
    let (s, b, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some(&token),
        &chat_body(""),
        &[],
    );
    assert_eq!(s, 401);
    assert!(b.contains("refused_auth"));
    assert!(seen.lock().unwrap().len() == 1); // only the live-token call went upstream
}

#[test]
fn second_slot_isolated() {
    let (mut p, token_a, _seen, _r) = setup("iso", Reservation::FixtureBound { micros: 1_000_000 });
    let token_b = p.register_slot("slot-B", "low", 10_000_000).unwrap();
    assert_ne!(token_a, token_b);
    // slot A's token cannot be confused with B's (constant-time match
    // across all live slots)
    let (s, _, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some(&token_b),
        &chat_body(""),
        &[],
    );
    assert_eq!(s, 200);
}

// ─── AC-3: the §4.2 route table ───

#[test]
fn route_table() {
    let (mut p, token, seen, _r) = setup("routes", Reservation::FixtureBound { micros: 1_000_000 });
    // unknown route → 404 refused_route
    let (s, b, _) = call(&mut p, "GET", "/v1/unknown", Some(&token), "", &[]);
    assert_eq!(s, 404);
    assert!(b.contains("refused_route"));
    // count_tokens → typed Anthropic 404, never forwarded
    let (s, b, _) = call(
        &mut p,
        "POST",
        "/v1/messages/count_tokens",
        Some(&token),
        &chat_body(""),
        &[],
    );
    assert_eq!(s, 404);
    assert!(b.contains("not_found_error"));
    assert!(b.contains("unsupported_upstream"));
    // GET /v1/models forwards verbatim
    let (s, b, h) = call(&mut p, "GET", "/v1/models", Some(&token), "", &[]);
    assert_eq!(s, 200);
    assert!(b.contains("glm-5.3-flash"));
    assert!(h.contains_key("x-request-id"));
    assert!(!h.contains_key("set-cookie")); // D3 outbound allowlist
    let seen = seen.lock().unwrap();
    assert!(seen.iter().all(|e| e.target != "/v1/messages/count_tokens"));
    assert_eq!(seen[0].method, "GET");
}

// ─── AC-3: field policy + streaming refusal ───

#[test]
fn field_policy_strips_and_refuses() {
    let (mut p, token, seen, _r) = setup("policy", Reservation::FixtureBound { micros: 1_000_000 });
    // tool_choice "auto" → stripped with notice, call forwards
    let (s, _, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some(&token),
        &chat_body(",\"tool_choice\":\"auto\",\"n\":1"),
        &[],
    );
    assert_eq!(s, 200);
    let up = seen.lock().unwrap()[0].body.clone();
    assert!(!up.contains("tool_choice"));
    assert!(!up.contains("\"n\""));
    // tool_choice non-auto → 400 refused_policy
    let (s, b, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some(&token),
        &chat_body(",\"tool_choice\":{\"type\":\"function\"}"),
        &[],
    );
    assert_eq!(s, 400);
    assert!(b.contains("invalid_request_error"));
    // n>1 → refused
    let (s, _, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some(&token),
        &chat_body(",\"n\":2"),
        &[],
    );
    assert_eq!(s, 400);
    // Responses-style continuity members → refused
    let (s, _, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some(&token),
        &chat_body(",\"previous_response_id\":\"r_1\""),
        &[],
    );
    assert_eq!(s, 400);
    // cache_control strips recursively on the Messages surface
    let (s, _, _) = call(
        &mut p,
        "POST",
        "/v1/messages",
        Some(&token),
        "{\"model\":\"x\",\"messages\":[{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"hi\",\"cache_control\":{\"type\":\"ephemeral\"}}]}],\"cache_control\":{\"type\":\"ephemeral\"},\"metadata\":{\"a\":1}}",
        &[],
    );
    assert_eq!(s, 200);
    let up = seen.lock().unwrap()[1].body.clone();
    assert!(!up.contains("cache_control"));
    assert!(!up.contains("metadata"));
    // streaming — R6.11's `buffered_heartbeat` serves it (the R6.10
    // `unsupported{release_mode}` refusal is gone); this fixture's
    // non-SSE body receipts `stream_broken` as the in-stream error.
    let (s, b, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some(&token),
        &chat_body(",\"stream\":true"),
        &[],
    );
    assert_eq!(s, 200); // the committed SSE head
    assert!(b.contains("stream_broken"), "{b}");
}

// ─── AC-5/AC-8: non-fixture lane + budget refusal ───

#[test]
fn lane_without_formula_refuses_at_step2() {
    let (mut p, token, seen, _r) = setup("nobound", Reservation::Unsupported);
    let (s, b, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some(&token),
        &chat_body(""),
        &[],
    );
    assert_eq!(s, 402);
    assert!(b.contains("reservation_formula"));
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn slot_budget_exhaustion_refuses_402() {
    let (mut p, token, _seen, _r) =
        setup("budget", Reservation::FixtureBound { micros: 5_000_000 });
    // slice of 4M < 5M bound → every reserve refuses
    let tiny = p.register_slot("slot-tiny", "high", 4_000_000).unwrap();
    let (s, b, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some(&tiny),
        &chat_body(""),
        &[],
    );
    assert_eq!(s, 402);
    assert!(b.contains("budget_refused"));
    // the healthy slot still forwards
    let (s, _, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some(&token),
        &chat_body(""),
        &[],
    );
    assert_eq!(s, 200);
}

// ─── AC-6/AC-8: durable-before-visible at the release boundary ───

#[test]
fn no_released_byte_without_durable_terminal() {
    let (mut p, token, _seen, root) =
        setup("killpoint", Reservation::FixtureBound { micros: 1_000_000 });
    // Arm the next durable write (the terminal row + blobs region):
    // downstream blob deposit is first — a fault there means nothing
    // lands and nothing is released.
    p.inject_durability_faults(1);
    let bound = p.bound_addr();
    let mut s = TcpStream::connect(bound).unwrap();
    let body = chat_body("");
    s.write_all(
        format!(
            "POST /v1/chat/completions HTTP/1.1\r\nhost: p\r\nauthorization: Bearer {token}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
        .as_bytes(),
    )
    .unwrap();
    for _ in 0..2000 {
        if p.tick() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    s.set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    let mut r = BufReader::new(s);
    // The refused deposit answers 500 or closes — either way no
    // upstream-shaped success byte is released, and the ledger has no
    // model.call.completed.
    let outcome = read_response(&mut r, 1 << 20);
    if let Ok((head, _)) = outcome {
        assert_eq!(head.status, 500);
    }
    let cs = classes(&root, RUN);
    assert!(!cs.contains(&"model.call.completed".to_string()));
    assert!(!cs.contains(&"model.call.requested".to_string()));
}

// ─── AC-9 (R6.10 leg of DF-R6.7-1): receipt leak_scan ───

#[test]
fn ollama_key_in_receipt_refuses_and_stops_phase() {
    // The upstream echoes back a body containing the credential —
    // depositing it must refuse + stop the phase.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let key = "sk-ollama-fixture-leak-12345";
    std::thread::spawn(move || {
        if let Ok(mut stream) = listener.accept().map(|(s, _)| s) {
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let _ = hharness_wire::http::read_request(&mut r, 4 << 20);
            let payload = format!("{{\"leak\":\"{key}\"}}");
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                payload.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    std::env::set_var("HH_R610_KEY", key);
    let store_root = root("leak");
    let mut lane = ProxyLane::program(
        "lane.leak",
        &format!("http://{addr}/v1"),
        "fixture.local",
        MODEL,
        Reservation::FixtureBound { micros: 1_000_000 },
    );
    lane.catalog_base = Some("http://127.0.0.1:1".to_string());
    let cfg = ProxyConfig {
        store_root,
        run_id: "phase-proxy-leak".into(),
        lane,
        listen: "127.0.0.1:0".into(),
        credential_env: Some("HH_R610_KEY".into()),
        authorization: auth(100_000_000),
        mode: hharness_hosting::proxy::ProxyMode::Live,
    };
    let (mut p, _bound) = Proxy::configure(cfg).unwrap();
    let token = p.register_slot("slot-A", "high", 50_000_000).unwrap();
    let (s, _b, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some(&token),
        &chat_body(""),
        &[],
    );
    assert_eq!(s, 500); // the key-bearing blob's deposit refused
                        // the phase is now stopped — the next admit refuses typed
    let (s, b, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some(&token),
        &chat_body(""),
        &[],
    );
    assert_eq!(s, 402);
    assert!(b.contains("refused_phase_stopped"));
}

// ─── receipt content ───

#[test]
fn receipt_records_mutations_usage_and_identity() {
    let (mut p, token, _seen, root) =
        setup("receipt", Reservation::FixtureBound { micros: 1_000_000 });
    let (s, _, _) = call(
        &mut p,
        "POST",
        "/v1/chat/completions",
        Some(&token),
        &chat_body(",\"model\":\"vendor-x\",\"reasoning_effort\":\"low\""),
        &[],
    );
    assert_eq!(s, 200);
    let store = Store::open(&root).unwrap();
    let envs = store.envelopes(RUN).unwrap().to_vec();
    let completed = envs
        .iter()
        .find(|e| e.class == "model.call.completed")
        .unwrap();
    let resp_ref = completed
        .payload
        .get("response_ref")
        .and_then(Json::as_str)
        .unwrap();
    // response_ref → the receipt blob
    let digest = resp_ref.strip_prefix("sha256:").unwrap();
    let bytes = store
        .get_blob(&hharness_identity::idp::ContentAddress {
            idp: "idp/1",
            algorithm: "sha256",
            digest: digest.to_string(),
            media_type: "application/json".into(),
            size: 0,
        })
        .unwrap();
    let doc = parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
    assert_eq!(
        doc.get("schema").and_then(Json::as_str),
        Some("hh.model_io/1")
    );
    let proxy = doc.get("proxy").unwrap();
    assert_eq!(
        proxy.get("schema").and_then(Json::as_str),
        Some("hh.model_proxy.receipt/1")
    );
    assert_eq!(
        proxy.get("proxy_version").and_then(Json::as_str),
        Some("hh-model-proxy/0.1.0")
    );
    assert_eq!(proxy.get("slot_id").and_then(Json::as_str), Some("slot-A"));
    assert_eq!(
        proxy.get("surface").and_then(Json::as_str),
        Some("openai.chat")
    );
    assert_eq!(
        proxy.get("disposition").and_then(Json::as_str),
        Some("forwarded")
    );
    let m = proxy.get("mutations").unwrap();
    let model = m.get("model").unwrap();
    assert_eq!(
        model.get("requested").and_then(Json::as_str),
        Some("vendor-x")
    );
    assert_eq!(model.get("applied").and_then(Json::as_str), Some(MODEL));
    let effort = m.get("effort").unwrap();
    assert_eq!(effort.get("requested").and_then(Json::as_str), Some("low"));
    assert_eq!(effort.get("applied").and_then(Json::as_str), Some("high"));
    assert_eq!(
        effort.get("path").and_then(Json::as_str),
        Some("reasoning_effort")
    );
    // usage + cost recorded (cache_read honest: provider sent no
    // cached_tokens → available:false, never 0)
    let usage = proxy.get("usage").unwrap();
    let view = usage.get("view").unwrap();
    assert_eq!(
        view.get("tokens.input.total").and_then(Json::as_int),
        Some(40)
    );
    let cost = usage.get("cost").unwrap();
    assert!(cost.get("micros").and_then(Json::as_int).is_some());
    assert_eq!(
        cost.get("cache_read").unwrap().get("available"),
        Some(&Json::Bool(false))
    );
    // downstream headers: authorization never recorded
    let dn = proxy.get("downstream").unwrap();
    let names: Vec<&str> = match dn.get("header_names").unwrap() {
        Json::Arr(v) => v.iter().filter_map(Json::as_str).collect(),
        _ => vec![],
    };
    assert!(!names.contains(&"authorization"));
    let kept = dn.get("headers_kept").unwrap();
    assert!(kept.get("authorization").is_none());
    // no secret anywhere in the ledger
    let all = envs
        .iter()
        .map(|e| e.payload.to_canonical_string())
        .collect::<String>();
    assert!(!all.contains(&token));
}

// ─── AC-7: the control channel over the real binary ───

#[test]
fn control_channel_over_the_binary() {
    use std::process::{Command, Stdio};
    let bin = env!("CARGO_BIN_EXE_hh-model-proxy");
    let store_root = root("chan");
    let mut child = Command::new(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let send = |stdin: &mut std::process::ChildStdin,
                stdout: &mut BufReader<std::process::ChildStdout>,
                op: &Json|
     -> Json {
        writeln!(stdin, "{}", op.to_canonical_string()).unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        std::io::BufRead::read_line(stdout, &mut line).unwrap();
        parse(line.trim()).unwrap()
    };
    // configure — the lane names the fixture upstream
    let upstream = fixture_upstream().0;
    let reply = send(
        &mut stdin,
        &mut stdout,
        &Json::obj([
            ("op", Json::str("configure")),
            ("store_root", Json::str(store_root.to_str().unwrap())),
            ("run_id", Json::str("phase-proxy-chan")),
            ("listen", Json::str("127.0.0.1:0")),
            (
                "authorization",
                Json::obj([
                    (
                        "verbatim",
                        Json::str("fixture authorization — R6.10 battery"),
                    ),
                    ("gate_record_ref", Json::str("gate.fixture")),
                    ("authorized_micros", Json::Int(100_000_000)),
                    ("available_credit_micros", Json::Int(100_000_000)),
                    ("m_ppm", Json::Int(0)),
                ]),
            ),
            (
                "lane",
                Json::obj([
                    ("lane_id", Json::str("lane.chan")),
                    ("endpoint_ref", Json::str(&upstream)),
                    ("upstream_host", Json::str("fixture.local")),
                    ("model", Json::str(MODEL)),
                    (
                        "reservation",
                        Json::obj([
                            ("kind", Json::str("fixture_bound")),
                            ("micros", Json::Int(1_000_000)),
                        ]),
                    ),
                    ("catalog_base", Json::str("http://127.0.0.1:1")),
                ]),
            ),
        ]),
    );
    let listen = reply
        .get("ok")
        .and_then(|o| o.get("listen"))
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let reply = send(
        &mut stdin,
        &mut stdout,
        &Json::obj([
            ("op", Json::str("register_slot")),
            ("slot_id", Json::str("slot-A")),
            ("effort", Json::str("high")),
            ("slice_micros", Json::Int(50_000_000)),
        ]),
    );
    let token = reply
        .get("ok")
        .and_then(|o| o.get("token"))
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    // a real HTTP call through the spawned binary
    let body = chat_body("");
    let mut s = TcpStream::connect(&listen).unwrap();
    s.write_all(
        format!(
            "POST /v1/chat/completions HTTP/1.1\r\nhost: p\r\nauthorization: Bearer {token}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
        .as_bytes(),
    )
    .unwrap();
    let mut r = BufReader::new(s);
    let (head, rbody) = read_response(&mut r, 1 << 20).unwrap();
    assert_eq!(head.status, 200);
    assert!(String::from_utf8_lossy(&rbody).contains("\"ok\""));
    let reply = send(
        &mut stdin,
        &mut stdout,
        &Json::obj([("op", Json::str("status"))]),
    );
    let slots_len = match reply.get("ok").unwrap().get("slots").unwrap() {
        Json::Arr(v) => v.len(),
        _ => 0,
    };
    assert_eq!(slots_len, 1);
    let _ = send(
        &mut stdin,
        &mut stdout,
        &Json::obj([("op", Json::str("stop"))]),
    );
    let _ = child.wait();
}
