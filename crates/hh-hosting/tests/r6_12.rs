//! Executable coverage for manifest ticket 265 / R6.12 — REPLAY
//! (experiments spec §4.9; REQ-XC-122 absorbs REQ-XD-108; design/33
//! F15; design/36 §1.3, §1.6 P1–P9). Every upstream leg is a loopback
//! fixture or the recorded sequence itself (REQ-XC-125 — offline
//! only). Every test fails if the behaviour it names is removed.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hharness_hosting::proxy::budget::ProxyAuthorization;
use hharness_hosting::proxy::lane::{ProxyLane, Reservation};
use hharness_hosting::proxy::replay::{
    rechunk, Keying, LatencyMode, ProxyMode, RecordedCall, RecordedReleased, RecordedResponse,
    Recording, ReplayConfig,
};
use hharness_hosting::proxy::{Proxy, ProxyConfig};
use hharness_ledger::store::Store;
use hharness_wire::http::{read_response, BodyReader};
use hharness_wire::json::{parse, Json};

const MODEL: &str = "glm-5.3-flash";
const RUN: &str = "phase-proxy";

fn auth(cap: i64) -> ProxyAuthorization {
    ProxyAuthorization {
        verbatim: "fixture authorization — R6.12 battery".into(),
        gate_record_ref: "gate.fixture".into(),
        authorized_micros: cap,
        available_credit_micros: cap,
        m_ppm: 0,
        prior_windows: vec![],
    }
}

fn root(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "hh-r6-12-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[derive(Debug, Clone)]
struct Seen {
    body: String,
}

fn fixture_upstream(payloads: Vec<String>) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::<Seen>::new()));
    let seen_w = seen.clone();
    std::thread::spawn(move || {
        let mut i = 0usize;
        for conn in listener.incoming() {
            let Ok(mut stream) = conn else { return };
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let Ok(Some((_head, body))) = hharness_wire::http::read_request(&mut r, 8 << 20) else {
                return;
            };
            seen_w.lock().unwrap().push(Seen {
                body: String::from_utf8_lossy(&body).to_string(),
            });
            let payload = payloads.get(i).cloned().unwrap_or_else(|| "{}".into());
            i += 1;
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                payload.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    (format!("http://{addr}/v1"), seen)
}

fn lane(endpoint: &str) -> ProxyLane {
    let mut l = ProxyLane::program(
        "lane.fixture",
        endpoint,
        "fixture.local",
        MODEL,
        Reservation::FixtureBound { micros: 1_000 },
    );
    l.catalog_base = Some("http://127.0.0.1:1".to_string());
    l
}

fn cfg(root: &PathBuf, endpoint: &str, mode: ProxyMode) -> ProxyConfig {
    ProxyConfig {
        store_root: root.clone(),
        run_id: RUN.to_string(),
        lane: lane(endpoint),
        listen: "127.0.0.1:0".into(),
        credential_env: None,
        authorization: auth(10_000_000),
        mode,
    }
}

fn call(proxy: &mut Proxy, token: &str, body: &str) -> (u16, String) {
    let bound = proxy.bound_addr();
    let mut s = TcpStream::connect(bound).unwrap();
    let req = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nhost: proxy.local\r\ncontent-type: application/json\r\nauthorization: Bearer {token}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).unwrap();
    for _ in 0..4000 {
        if proxy.tick() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut r = BufReader::new(s);
    let (head, rbody) = read_response(&mut r, 8 << 20).unwrap();
    (head.status, String::from_utf8_lossy(&rbody).to_string())
}

fn chat_body(i: usize) -> String {
    format!("{{\"model\":\"anything\",\"messages\":[{{\"role\":\"user\",\"content\":\"q{i}\"}}]}}")
}

fn resp_body(i: usize) -> String {
    format!(
        "{{\"id\":\"r{i}\",\"object\":\"chat.completion\",\"model\":\"{MODEL}\",\"choices\":[{{\"index\":0,\"message\":{{\"role\":\"assistant\",\"content\":\"a{i}\"}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":4,\"completion_tokens\":2,\"total_tokens\":6}}}}"
    )
}

fn events(store_root: &PathBuf) -> Vec<(String, Json)> {
    Store::open(store_root)
        .unwrap()
        .envelopes(RUN)
        .unwrap()
        .iter()
        .map(|e| (e.class.clone(), e.payload.clone()))
        .collect()
}

fn budget_rows(store_root: &PathBuf) -> usize {
    events(store_root)
        .iter()
        .filter(|(c, _)| c.starts_with("control.budget.") || c == "measurement.cost.attributed")
        .count()
}

fn receipts(store_root: &PathBuf) -> Vec<Json> {
    let store = Store::open(store_root).unwrap();
    events(store_root)
        .iter()
        .filter(|(c, _)| c == "model.call.completed" || c == "model.call.failed")
        .map(|(_, t)| {
            let r = t.get("response_ref").and_then(Json::as_str).unwrap();
            let addr = hharness_identity::idp::ContentAddress {
                idp: "idp/1".into(),
                algorithm: "sha256".into(),
                digest: r.strip_prefix("sha256:").unwrap().to_string(),
                media_type: "application/json".into(),
                size: 0,
            };
            let bytes = store.get_blob(&addr).unwrap();
            parse(&String::from_utf8(bytes).unwrap()).unwrap()
        })
        .collect()
}

fn synthetic(slot: &str, n: usize) -> Recording {
    let calls = (0..n)
        .map(|k| {
            let down = chat_body(k).into_bytes();
            let upstream = hharness_wire::json::parse_preserve(&chat_body(k))
                .map(|mut p| {
                    if let hharness_wire::json::PJson::Obj(m) = &mut p {
                        m.insert(
                            "model".into(),
                            hharness_wire::json::PJson::Str(MODEL.into()),
                        );
                        m.insert(
                            "reasoning_effort".into(),
                            hharness_wire::json::PJson::Str("high".into()),
                        );
                    }
                    p.to_provider_string().into_bytes()
                })
                .unwrap();
            let body = resp_body(k).into_bytes();
            let mut headers = BTreeMap::new();
            headers.insert("content-type".to_string(), "application/json".to_string());
            RecordedCall {
                k: k as u64,
                downstream_method: "POST".into(),
                downstream_path: "/v1/chat/completions".into(),
                downstream_body: down.clone(),
                upstream_request: Some(upstream),
                upstream_response: Some(RecordedResponse {
                    status: 200,
                    headers: headers.clone(),
                    body: body.clone(),
                    chunk_stamps: vec![(0, body.len())],
                }),
                failure: None,
                released: Some(RecordedReleased {
                    status: 200,
                    headers,
                    body,
                }),
                disposition: "forwarded".into(),
                latency_ms: 0,
            }
        })
        .collect();
    Recording {
        slot_id: slot.to_string(),
        provenance: "synthetic".to_string(),
        calls,
    }
}

fn replay_cfg(rec: Recording, strict: bool, canonicalizer: &str) -> ProxyMode {
    let mut recordings = BTreeMap::new();
    recordings.insert(rec.slot_id.clone(), rec);
    ProxyMode::Replay(ReplayConfig {
        recordings,
        canonicalizer_id: canonicalizer.to_string(),
        cell_facts: vec![],
        strict,
        keying: Keying::Sequence,
        latency: LatencyMode::Immediate,
    })
}

// ── AC-1/P3/P8 — a recorded slot replays byte-identically, no
// upstream contact, no budget row, receipts mode:replay, per-call
// verdicts, lane transforms applied in replay. ─────────────────────

#[test]
fn recorded_slot_replays_byte_identically_with_no_upstream_or_budget() {
    let rec = synthetic("slot-A", 3);
    let r = root("replay");
    // `replay:offline` is never dialed — replay has no upstream port.
    let (mut proxy, _b) =
        Proxy::configure(cfg(&r, "replay:offline", replay_cfg(rec, true, "identity"))).unwrap();
    let token = proxy.register_slot("slot-A", "high", 1_000_000).unwrap();
    for k in 0..3 {
        let (status, body) = call(&mut proxy, &token, &chat_body(k));
        assert_eq!(status, 200);
        assert_eq!(
            body,
            resp_body(k),
            "k={k} serves the recorded bytes verbatim"
        );
    }
    let evs = events(&r);
    assert!(
        evs.iter().all(|(c, _)| c != "measurement.cost.attributed"
            && c != "control.budget.reserved"
            && c != "control.budget.released"),
        "no budget charge rows in replay"
    );
    assert_eq!(budget_rows(&r), 0);
    for (k, rc) in receipts(&r).iter().enumerate() {
        let p = rc.get("proxy").unwrap();
        assert_eq!(p.get("mode").and_then(Json::as_str), Some("replay"));
        let v = p.get("replay").unwrap();
        assert_eq!(
            v.get("verdict").and_then(Json::as_str),
            Some("exact"),
            "k={k}"
        );
    }
}

#[test]
fn record_then_replay_round_trips() {
    // Record a live slot, seal it, replay the sealed manifest — the
    // replayed downstream bytes equal the recorded responses.
    let (ep, _seen) = fixture_upstream(vec![resp_body(0), resp_body(1)]);
    let rr = root("rec");
    let (mut proxy, _b) = Proxy::configure(cfg(&rr, &ep, ProxyMode::Record)).unwrap();
    let token = proxy.register_slot("slot-A", "high", 5_000_000).unwrap();
    for k in 0..2 {
        let (s, b) = call(&mut proxy, &token, &chat_body(k));
        assert_eq!((s, b.as_str()), (200, resp_body(k).as_str()));
    }
    let (_r_ref, doc) = proxy.seal_recording("slot-A").unwrap();
    assert_eq!(
        doc.get("schema").and_then(Json::as_str),
        Some("hh.model_proxy.recording/1")
    );
    let rec = Recording::parse(&doc).unwrap();
    assert_eq!(rec.calls.len(), 2);
    assert!(rec.calls[0].upstream_request.is_some());
    assert!(events(&rr)
        .iter()
        .any(|(c, _)| c == "model.proxy.recording.sealed"));
    let pr = root("play");
    let (mut proxy, _b) = Proxy::configure(cfg(
        &pr,
        "replay:offline",
        replay_cfg(rec, true, "identity"),
    ))
    .unwrap();
    let token = proxy.register_slot("slot-A", "high", 1_000_000).unwrap();
    for k in 0..2 {
        let (s, b) = call(&mut proxy, &token, &chat_body(k));
        assert_eq!(
            (s, b.as_str()),
            (200, resp_body(k).as_str()),
            "replay k={k}"
        );
    }
}

// ── AC-1/P4 — strict divergence stops with `replay_diverged` and
// `first_diff_path`; lenient serves and records. ────────────────────

#[test]
fn strict_divergence_stops_at_k_with_first_diff_path() {
    let rec = synthetic("slot-A", 4);
    let r = root("div");
    let (mut proxy, _b) =
        Proxy::configure(cfg(&r, "replay:offline", replay_cfg(rec, true, "identity"))).unwrap();
    let token = proxy.register_slot("slot-A", "high", 1_000_000).unwrap();
    for k in 0..2 {
        let (s, _) = call(&mut proxy, &token, &chat_body(k));
        assert_eq!(s, 200);
    }
    // k=2 — a different request body diverges at messages[0].content.
    let (s, body) = call(&mut proxy, &token, &chat_body(99));
    assert_eq!(s, 409, "typed 409 replay_diverged");
    assert!(body.contains("replay_diverged"), "{body}");
    let last = receipts(&r).pop().unwrap();
    let v = last.get("proxy").unwrap().get("replay").unwrap();
    assert_eq!(v.get("verdict").and_then(Json::as_str), Some("diverged"));
    assert_eq!(
        v.get("first_diff_path").and_then(Json::as_str),
        Some("messages[0].content")
    );
    // The slot is ended — a later request refuses the same way.
    let (s, body) = call(&mut proxy, &token, &chat_body(2));
    assert_eq!(s, 409);
    assert!(body.contains("replay_diverged"));
}

#[test]
fn lenient_mode_serves_and_records_the_verdict() {
    let rec = synthetic("slot-A", 2);
    let r = root("len");
    let (mut proxy, _b) = Proxy::configure(cfg(
        &r,
        "replay:offline",
        replay_cfg(rec, false, "identity"),
    ))
    .unwrap();
    let token = proxy.register_slot("slot-A", "high", 1_000_000).unwrap();
    let (s, b) = call(&mut proxy, &token, &chat_body(99));
    assert_eq!(
        (s, b.as_str()),
        (200, resp_body(0).as_str()),
        "lenient serves anyway"
    );
    let v = receipts(&r)[0]
        .get("proxy")
        .unwrap()
        .get("replay")
        .unwrap()
        .clone();
    assert_eq!(v.get("verdict").and_then(Json::as_str), Some("diverged"));
    let (s, _) = call(&mut proxy, &token, &chat_body(1));
    assert_eq!(s, 200, "lenient keeps the slot live");
}

// ── AC-2 — canonicalizer built-ins + the typed end-of-sequence
// refusal. ─────────────────────────────────────────────────────────

#[test]
fn canonicalizer_builtins_match_and_mismatch() {
    use hharness_gateway::replay::{canonicalizer_for, match_requests, MatchVerdict};
    let canon = canonicalizer_for("json_canonical", &[]).unwrap();
    let a = b"{\"a\":1,\"b\":2}";
    let same = b"{ \"b\":2, \"a\":1 }";
    let diff = b"{\"a\":1,\"b\":3}";
    assert_eq!(
        match_requests(canon.as_ref(), "", a, a).unwrap(),
        MatchVerdict::Exact
    );
    assert_eq!(
        match_requests(canon.as_ref(), "", a, same).unwrap(),
        MatchVerdict::CanonicalEqual
    );
    assert!(matches!(
        match_requests(canon.as_ref(), "", a, diff).unwrap(),
        MatchVerdict::Diverged { .. }
    ));
    let id = canonicalizer_for("identity", &[]).unwrap();
    assert!(matches!(
        match_requests(id.as_ref(), "", a, same).unwrap(),
        MatchVerdict::Diverged { .. }
    ));
    let l1 = canonicalizer_for("hh-l1-canon", &[]).unwrap();
    assert!(matches!(
        match_requests(l1.as_ref(), "", a, diff).unwrap(),
        MatchVerdict::Diverged { .. }
    ));
    // json_canonical rides the workspace canonicalizer — a
    // key-order-only difference is canonical_equal (CC1).
    let v = hharness_wire::json::parse_provider("{\"b\":2,\"a\":1}")
        .unwrap()
        .to_canonical_string();
    assert_eq!(v, "{\"a\":1,\"b\":2}");
    assert!(canonicalizer_for("bogus", &[]).is_err());
}

#[test]
fn hh_l1_canon_masks_stream_and_session_but_not_content() {
    use hharness_gateway::replay::{canonicalizer_for, match_requests, MatchVerdict};
    let l1 = canonicalizer_for("hh-l1-canon", &[]).unwrap();
    let a = br#"{"model":"m","stream":true,"session_id":"s1","messages":[{"role":"user","content":"x"}]}"#;
    let b = br#"{"model":"m","messages":[{"role":"user","content":"x"}],"session_id":"s2"}"#;
    assert_eq!(
        match_requests(l1.as_ref(), "", a, b).unwrap(),
        MatchVerdict::CanonicalEqual
    );
    let c = br#"{"model":"m","messages":[{"role":"user","content":"y"}]}"#;
    assert!(matches!(
        match_requests(l1.as_ref(), "", a, c).unwrap(),
        MatchVerdict::Diverged { .. }
    ));
}

#[test]
fn request_beyond_sequence_gets_typed_exhausted_refusal() {
    let rec = synthetic("slot-A", 1);
    let r = root("exh");
    let (mut proxy, _b) =
        Proxy::configure(cfg(&r, "replay:offline", replay_cfg(rec, true, "identity"))).unwrap();
    let token = proxy.register_slot("slot-A", "high", 1_000_000).unwrap();
    let (s, _) = call(&mut proxy, &token, &chat_body(0));
    assert_eq!(s, 200);
    let (s, body) = call(&mut proxy, &token, &chat_body(0));
    assert_eq!(s, 409);
    assert!(body.contains("replay_exhausted"), "{body}");
    let v = receipts(&r)[1]
        .get("proxy")
        .unwrap()
        .get("replay")
        .unwrap()
        .clone();
    assert_eq!(v.get("verdict").and_then(Json::as_str), Some("exhausted"));
}

// ── P7 — deterministic re-chunking when a harness requests
// streaming. ───────────────────────────────────────────────────────

#[test]
fn streaming_request_rechunks_by_recorded_stamps() {
    let chunks = rechunk(&[(0, 4), (2, 4), (5, 4)], b"0123456789ab");
    assert_eq!(
        chunks,
        vec![b"0123".to_vec(), b"4567".to_vec(), b"89ab".to_vec()]
    );
    let chunks = rechunk(&[], b"body");
    assert_eq!(chunks, vec![b"body".to_vec()]);
}

#[test]
fn replay_streaming_serves_sse_head_and_rechunked_body() {
    let sse = b"data: {\"a\":1}\n\ndata: [DONE]\n\n".to_vec();
    let mut rec = synthetic("slot-A", 1);
    let mut headers = BTreeMap::new();
    headers.insert("content-type".into(), "text/event-stream".into());
    rec.calls[0].upstream_response = Some(RecordedResponse {
        status: 200,
        headers: headers.clone(),
        body: sse.clone(),
        chunk_stamps: vec![(0, 14), (3, sse.len() - 14)],
    });
    rec.calls[0].released = Some(RecordedReleased {
        status: 200,
        headers,
        body: sse.clone(),
    });
    // The recorded upstream-bound request carries stream:true.
    rec.calls[0].upstream_request = Some(
        br#"{"messages":[{"content":"q0","role":"user"}],"model":"glm-5.3-flash","reasoning_effort":"high","stream":true}"#
            .to_vec(),
    );
    let r = root("sse");
    let (mut proxy, _b) =
        Proxy::configure(cfg(&r, "replay:offline", replay_cfg(rec, true, "identity"))).unwrap();
    let token = proxy.register_slot("slot-A", "high", 1_000_000).unwrap();
    let bound = proxy.bound_addr();
    let mut s = TcpStream::connect(bound).unwrap();
    let body = br#"{"model":"x","messages":[{"role":"user","content":"q0"}],"stream":true}"#;
    let req = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nhost: h\r\ncontent-type: application/json\r\nauthorization: Bearer {token}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        String::from_utf8_lossy(body)
    );
    s.write_all(req.as_bytes()).unwrap();
    for _ in 0..4000 {
        if proxy.tick() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut rdr = BufReader::new(s);
    let head = hharness_wire::http::read_response_head(&mut rdr).unwrap();
    if head.status != 200 {
        let mut br = BodyReader::new(&mut rdr, &head, 1 << 20);
        let mut b = Vec::new();
        while let Ok(Some(c)) = br.next_chunk() {
            b.extend(c);
        }
        panic!(
            "status {} body {}",
            head.status,
            String::from_utf8_lossy(&b)
        );
    }
    let mut br = BodyReader::new(&mut rdr, &head, 1 << 20);
    let mut got = Vec::new();
    while let Ok(Some(c)) = br.next_chunk() {
        got.push(c);
    }
    assert_eq!(got.len(), 2, "re-chunked by the recorded stamps");
    assert_eq!(got.concat(), sse);
}

// ── P6 — synthetic-sequence ingestion declares `provenance:
// synthetic` and ingests the same codec; malformed recordings refuse.
// ──────────────────────────────────────────────────────────────────

#[test]
fn synthetic_sequence_ingests_and_malformed_refuses() {
    let rec = synthetic("slot-A", 1);
    let doc = rec.to_json();
    assert_eq!(
        doc.get("provenance").and_then(Json::as_str),
        Some("synthetic")
    );
    let back = Recording::parse(&doc).unwrap();
    assert_eq!(back.calls.len(), 1);
    // Unknown member / bad provenance / missing members refuse.
    let mut bad = doc.clone();
    if let Json::Obj(m) = &mut bad {
        m.insert("extra".into(), Json::Int(1));
    }
    assert!(Recording::parse(&bad).is_err());
    let mut bad2 = doc.clone();
    if let Json::Obj(m) = &mut bad2 {
        m.insert("provenance".into(), Json::str("bogus"));
    }
    assert!(Recording::parse(&bad2).is_err());
    assert!(Recording::parse(&Json::obj([("schema", Json::str("x"))])).is_err());
}

// ── P1 — the slot-token gate binds replay requests to their
// recorded sequence. ────────────────────────────────────────────────

#[test]
fn slot_token_binds_the_replay_sequence() {
    let rec = synthetic("slot-A", 1);
    let r = root("p1");
    let (mut proxy, _b) =
        Proxy::configure(cfg(&r, "replay:offline", replay_cfg(rec, true, "identity"))).unwrap();
    let token = proxy.register_slot("slot-A", "high", 1_000_000).unwrap();
    let (s, _) = call(&mut proxy, "bad-token", &chat_body(0));
    assert_eq!(s, 401, "no token, no replay");
    let (s, _) = call(&mut proxy, &token, &chat_body(0));
    assert_eq!(s, 200);
}

// ── `sequence_then_hash` — an out-of-order request matches its
// canonical hash among outstanding calls. ───────────────────────────

#[test]
fn sequence_then_hash_serves_out_of_order_match() {
    let rec = synthetic("slot-A", 2);
    let mut recordings = BTreeMap::new();
    recordings.insert(rec.slot_id.clone(), rec);
    let r = root("hash");
    let (mut proxy, _b) = Proxy::configure(cfg(
        &r,
        "replay:offline",
        ProxyMode::Replay(ReplayConfig {
            recordings,
            canonicalizer_id: "json_canonical".to_string(),
            cell_facts: vec![],
            strict: true,
            keying: Keying::SequenceThenHash,
            latency: LatencyMode::Immediate,
        }),
    ))
    .unwrap();
    let token = proxy.register_slot("slot-A", "high", 1_000_000).unwrap();
    // Ask for call 1 first — the hash match finds calls[1].
    let (s, b) = call(&mut proxy, &token, &chat_body(1));
    assert_eq!((s, b.as_str()), (200, resp_body(1).as_str()));
    let (s, b) = call(&mut proxy, &token, &chat_body(0));
    assert_eq!((s, b.as_str()), (200, resp_body(0).as_str()));
}

// ── used-set bookkeeping ──────────────────────────────────────────

#[test]
fn replay_pick_exhausts_when_all_used() {
    let rec = synthetic("slot-A", 1);
    let mut used = BTreeSet::new();
    used.insert(0usize);
    let canon = hharness_gateway::replay::canonicalizer_for("identity", &[]).unwrap();
    match hharness_hosting::proxy::replay::replay_pick(
        &rec,
        &used,
        Keying::Sequence,
        canon.as_ref(),
        "openai.chat",
        b"{}",
    )
    .unwrap()
    {
        hharness_hosting::proxy::replay::ReplayPick::Exhausted => {}
        _ => panic!("expected exhausted"),
    }
}
