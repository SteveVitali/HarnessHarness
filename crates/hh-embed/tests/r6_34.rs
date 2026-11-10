//! R6.34 acceptance battery (manifest 292; REQ-XE-107/108 + REQ-XE-106
//! contributing) — the model port's opt-in wire-fidelity members:
//! AC-3 non-stream request golden + recorded `chat.completion` decode;
//! AC-4 BM18 + BM4–BM10 replay goldens, spaced-arguments round-trip,
//! `id_normalisation` provider ids, blob-sourced bytes (`provenance:
//! hand_derived` until the O-recorded corpus lands — REQ-XE-114);
//! AC-5 a definition declaring none of the new members renders
//! byte-identically. USD $0, loopback only.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

use hharness_control::driver::ModelPort;
use hharness_embed::gateway_model::{decode_model_lane, GatewayModel};
use hharness_gateway::http_transport::{decode_exchange_frames, Family, RawExchange};
use hharness_gateway::recording::Recording;
use hharness_gateway::vocab::StopReason;
use hharness_identity::idp;
use hharness_ledger::classes::Durability;
use hharness_ledger::event::{EventEnvelope, EventPlane, Producer, Scope};
use hharness_ledger::manifest::ParticipantClass;
use hharness_wire::json::{parse, Json};

fn ev(seq: u64, class: &str, payload: Json) -> EventEnvelope {
    EventEnvelope {
        event_id: format!("e{seq}"),
        run_id: "run".into(),
        seq,
        ts: "2026-11-10T00:00:00.000Z".into(),
        hlc: None,
        plane: EventPlane::of_class(class).unwrap_or(EventPlane::Control),
        class: class.into(),
        schema_version: 1,
        producer: Producer::kernel("kernel:test"),
        participant_class: ParticipantClass::Native,
        observability_level: Default::default(),
        durability: Durability::Ledger,
        scope: Scope::default(),
        lease_generation: 1,
        parent_event_id: hharness_ledger::ids::ROOT_EVENT.into(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: Default::default(),
        provenance: None,
        payload,
        prev_hash: "h".into(),
        hash: format!("h{seq}"),
    }
}

// ── loopback fake provider (the r5_20 pattern — literal socket bytes) ────────

struct FakeProvider {
    endpoint: String,
    got: std::sync::Arc<std::sync::Mutex<Option<Vec<u8>>>>,
}

fn head_complete(buf: &[u8]) -> bool {
    let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return false;
    };
    let head = String::from_utf8_lossy(&buf[..p]);
    let mut want = 0usize;
    for line in head.split("\r\n") {
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            want = v.trim().parse().unwrap_or(0);
        }
    }
    buf.len() >= p + 4 + want
}

fn fake_provider(response: String) -> FakeProvider {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let got = std::sync::Arc::new(std::sync::Mutex::new(None));
    let got2 = got.clone();
    std::thread::spawn(move || {
        if let Ok((mut sock, _)) = listener.accept() {
            let mut buf = Vec::new();
            let mut tmp = [0u8; 8192];
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            while !head_complete(&buf) && std::time::Instant::now() <= deadline {
                match sock.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => buf.extend_from_slice(&tmp[..n]),
                    Err(_) => break,
                }
            }
            *got2.lock().unwrap() = Some(buf);
            let _ = sock.write_all(response.as_bytes());
            let _ = sock.flush();
        }
    });
    FakeProvider {
        endpoint: format!("http://127.0.0.1:{port}"),
        got,
    }
}

impl FakeProvider {
    fn body_bytes(&self) -> Vec<u8> {
        for _ in 0..50 {
            if let Some(buf) = self.got.lock().unwrap().as_ref() {
                let p = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
                return buf[p + 4..].to_vec();
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("provider saw no request");
    }
}

/// A plain (non-SSE) `chat.completion` answer.
fn completion_ok(model: &str) -> String {
    let doc = Json::obj([
        ("id", Json::str("chatcmpl-1")),
        ("object", Json::str("chat.completion")),
        ("model", Json::str(model)),
        (
            "choices",
            Json::Arr(vec![Json::obj([
                ("index", Json::Int(0)),
                (
                    "message",
                    Json::obj([
                        ("role", Json::str("assistant")),
                        ("content", Json::str("done")),
                    ]),
                ),
                ("finish_reason", Json::str("stop")),
            ])]),
        ),
        (
            "usage",
            Json::obj([
                ("prompt_tokens", Json::Int(5)),
                ("completion_tokens", Json::Int(1)),
                ("total_tokens", Json::Int(6)),
            ]),
        ),
    ])
    .to_canonical_string();
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        doc.len(),
        doc
    )
}

// ── BM fixtures (R6.28's committed synthetic corpus) ─────────────────────────

fn bm_fixture(id: &str) -> Recording {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../hh-runner/tests/fixtures/r6_28")
        .join(format!("{id}.json"));
    let doc = parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    Recording::parse(&doc).unwrap()
}

fn bm_call(id: &str) -> (Vec<u8>, RawExchange) {
    let rec = bm_fixture(id);
    let c = rec.calls.into_iter().next().unwrap();
    let u = c.upstream_response.unwrap();
    (
        c.downstream_body,
        RawExchange {
            status: u.status,
            headers: u.headers,
            body: u.body,
            chunk_stamps: u.chunk_stamps,
        },
    )
}

/// The `hh.model_io/1` blob a completed call would have deposited —
/// `frames[]` from `completion_frames` over the recorded response bytes.
fn model_io_blob(mc: &str, exchange: &RawExchange) -> Vec<u8> {
    let frames = decode_exchange_frames(Family::ChatCompletions, 1 << 20, exchange).unwrap();
    let doc = Json::obj([
        ("schema", Json::str("hh.model_io/1")),
        ("model_call_id", Json::str(mc)),
        ("request_body", Json::Null),
        ("plan_ref", Json::str("sha256:00")),
        (
            "frames",
            Json::Arr(
                frames
                    .iter()
                    .map(|(at, f)| {
                        Json::obj([
                            ("at_ms", Json::Int(*at as i64)),
                            ("kind", Json::str(f.kind.clone())),
                            ("data", f.data.clone()),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("outcome", Json::obj([])),
    ]);
    doc.to_canonical_string().into_bytes()
}

/// A `model.call.completed` row carrying the call pair + `response_ref`
/// (the blob's `idp` address — the driver's own spelling).
fn completed_row(seq: u64, mc: &str, blob: &[u8], calls: &[(&str, &str, &str)]) -> EventEnvelope {
    let mut e = ev(
        seq,
        "model.call.completed",
        Json::obj([
            ("model_call_id", Json::str(mc)),
            ("attempt_no", Json::Int(1)),
            ("stop_reason", Json::str("stop")),
            ("text_empty", Json::Bool(false)),
            (
                "response_ref",
                Json::str(idp::address(blob, "application/json").id()),
            ),
            (
                "calls",
                Json::Arr(
                    calls
                        .iter()
                        .map(|(id, surface, args)| {
                            Json::obj([
                                ("tool_call_id", Json::str(*id)),
                                ("surface", Json::str(*surface)),
                                ("args_raw", Json::str(*args)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]),
    );
    e.scope.model_call_id = Some(mc.to_string());
    e
}

fn lane(params: &[(&str, Json)], endpoint: &str) -> hharness_embed::gateway_model::ModelLane {
    let mut m = BTreeMap::new();
    m.insert("dialect".to_string(), Json::str("openai-compatible@1"));
    m.insert("endpoint".to_string(), Json::str(endpoint));
    m.insert("provider_model_id".to_string(), Json::str("fixture-model"));
    for (k, v) in params {
        m.insert(k.to_string(), v.clone());
    }
    decode_model_lane(&Json::Obj(m), "profile:test").unwrap()
}

/// The plan body the port stages for `mc` over `prefix`.
fn arr<'a>(j: &'a Json) -> &'a Vec<Json> {
    match j {
        Json::Arr(v) => v,
        _ => panic!("not an array: {j:?}"),
    }
}

fn staged_body(
    params: &[(&str, Json)],
    prefix: &[EventEnvelope],
    blobs: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Json {
    let mut port = GatewayModel::new(
        lane(params, "http://127.0.0.1:1"),
        "do the thing".into(),
        None,
        vec![],
    )
    .unwrap();
    let ms = port.requested_members("mc.2", &Json::Null, prefix, blobs);
    port.pending_body("mc.2")
        .filter(|b| !matches!(b, Json::Null))
        .unwrap_or_else(|| panic!("plan refused: {ms:?}"))
        .clone()
}

// ── AC-3 (REQ-XE-107; AC-7.3-5) — non-stream transport ───────────────────────

#[test]
fn ac3_non_stream_golden_and_decode() {
    // Declared non-stream: no `stream`/`stream_options`/`tool_choice`
    // members on the wire; the plain `chat.completion` body decodes
    // through the existing completion decoder (no second decoder, CC1).
    let provider = fake_provider(completion_ok("fixture-model"));
    let params = [
        ("transport", Json::str("non_stream")),
        ("stream_member", Json::str("omit")),
        ("tool_choice", Json::str("omit")),
    ];
    let mut port = GatewayModel::new(
        lane(&params, &provider.endpoint),
        "do the thing".into(),
        None,
        vec![],
    )
    .unwrap();
    let members = port.requested_members("mc.1", &Json::Null, &[], &|_: &str| None);
    assert!(members.iter().all(|(k, _)| *k != "plan_error"));
    let outcome = port.call("mc.1", &Json::Null);
    assert_eq!(outcome.stop_reason, StopReason::EndTurn, "{outcome:?}");
    let body = parse(&String::from_utf8(provider.body_bytes()).unwrap()).unwrap();
    for k in ["stream", "stream_options", "tool_choice"] {
        assert!(
            body.get(k).is_none(),
            "non-stream body carries {k}: {body:?}"
        );
    }
    // The committed non-stream golden.
    let golden_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/r6_34/ac3_nonstream_request.json");
    let golden = Json::obj([
        ("schema", Json::str("hh-r6.34-request-golden/1")),
        ("provenance", Json::str("hand_derived")),
        (
            "declaration",
            Json::str("transport=non_stream, stream_member=omit, tool_choice=omit"),
        ),
        ("body", body.clone()),
    ]);
    if std::env::var("HH_R634_WRITE_GOLDENS").is_ok() {
        std::fs::create_dir_all(golden_path.parent().unwrap()).unwrap();
        std::fs::write(&golden_path, golden.to_canonical_string()).unwrap();
    }
    let committed = parse(&std::fs::read_to_string(&golden_path).unwrap()).unwrap();
    assert_eq!(
        committed, golden,
        "non-stream request drifts from the golden"
    );

    // The recorded `chat.completion` body decodes through
    // `decode_exchange_frames` — the one decoder (REQ-XE-107).
    let (_, exchange) = bm_call("bm18");
    let frames = decode_exchange_frames(Family::ChatCompletions, 1 << 20, &exchange).unwrap();
    let mut state = hharness_gateway::grammar::DecodeState {
        model_call_id: "mc.1".into(),
        attempt_no: 1,
        ..Default::default()
    };
    let mut terminal = None;
    for (at, f) in &frames {
        for e in hharness_gateway::grammar::decode_frame(
            &hharness_gateway::dialects::openai_compatible(),
            f,
            &mut state,
            *at,
            "openai-compatible@1",
        ) {
            if matches!(
                e.kind,
                hharness_gateway::grammar::ModelEventKind::Completed { .. }
                    | hharness_gateway::grammar::ModelEventKind::Failed { .. }
            ) {
                terminal = Some(e.kind);
            }
        }
    }
    let Some(hharness_gateway::grammar::ModelEventKind::Completed { message, .. }) = terminal
    else {
        panic!("recorded completion did not decode a terminal: {terminal:?}")
    };
    let tcs = message.tool_calls();
    assert_eq!(tcs.len(), 1);
    assert_eq!(
        tcs[0].surface_ids.get("provider_call_id").unwrap(),
        "call_0"
    );
    assert!(
        message
            .blocks
            .iter()
            .any(|b| matches!(b, hharness_gateway::message::ModelBlock::Reasoning { .. })),
        "reasoning_content survives the decode"
    );
}

#[test]
fn ac3_stream_member_omit_under_stream_refuses() {
    let mut m = BTreeMap::new();
    m.insert("dialect".to_string(), Json::str("openai-compatible@1"));
    m.insert("endpoint".to_string(), Json::str("http://127.0.0.1:1"));
    m.insert("provider_model_id".to_string(), Json::str("m"));
    m.insert("stream_member".to_string(), Json::str("omit"));
    let e = decode_model_lane(&Json::Obj(m), "p").unwrap_err();
    assert!(format!("{e:?}").contains("stream_member"));
}

// ── AC-4 (REQ-XE-108; AC-7.3-6) — the declared replay choices ────────────────

fn replay_params() -> [(&'static str, Json); 1] {
    [(
        "transcript_render",
        Json::obj([
            (
                "assistant_replay",
                Json::obj([
                    ("content", Json::str("verbatim")),
                    ("reasoning", Json::str("verbatim")),
                    ("extra_members", Json::Arr(vec![])),
                ]),
            ),
            ("id_normalisation", Json::str("provider_verbatim")),
            ("tool_arguments", Json::str("verbatim")),
            ("text_only_turn", Json::str("replay")),
            ("format_error_turn", Json::str("omit")),
        ]),
    )]
}

/// The blob seam the test drives — keyed by the row's `response_ref`,
/// recording every lookup (the pin that replayed bytes come from the
/// `hh.model_io/1` blob, never re-synthesized).
fn blob_seam<'a>(
    blobs: &'a BTreeMap<String, Vec<u8>>,
    hits: &'a std::sync::Mutex<Vec<String>>,
) -> impl Fn(&str) -> Option<Vec<u8>> + 'a {
    move |id: &str| {
        hits.lock().unwrap().push(id.to_string());
        blobs.get(id).cloned()
    }
}

#[test]
fn ac4_bm_replay_goldens() {
    // BM18 + BM4–BM10 under the declared replay choices — provider
    // ids, verbatim args, assistant content/reasoning replay.
    let mut goldens = Vec::new();
    for bm in [
        "bm18", "bm04", "bm05", "bm06", "bm07", "bm08", "bm09", "bm10",
    ] {
        let (_, exchange) = bm_call(bm);
        let blob = model_io_blob("mc.1", &exchange);
        // calls[] args_raw is the *canonicalized* arg — verbatim replay
        // must come from the blob (spaced-arguments pin below). The
        // row's call list derives from the recorded body's
        // `tool_calls[]` — the driver's own fold (text-only turns have
        // an empty list).
        let resp = parse(&String::from_utf8_lossy(&exchange.body)).unwrap();
        let calls: Vec<(String, String, String)> = resp
            .get("choices")
            .and_then(|c| arr(c).first())
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("tool_calls"))
            .map(|t| {
                arr(t)
                    .iter()
                    .enumerate()
                    .map(|(i, tc)| {
                        let raw = tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(Json::as_str)
                            .unwrap_or("{}");
                        let canon = parse(raw)
                            .map(|j| j.to_canonical_string())
                            .unwrap_or_else(|_| raw.to_string());
                        (
                            format!("mc.1.tc{i}"),
                            tc.get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(Json::as_str)
                                .unwrap_or("bash")
                                .to_string(),
                            canon,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let call_refs: Vec<(&str, &str, &str)> = calls
            .iter()
            .map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str()))
            .collect();
        let blob_ref = idp::address(&blob, "application/json").id();
        let prefix = vec![completed_row(1, "mc.1", &blob, &call_refs)];
        let hits = std::sync::Mutex::new(Vec::new());
        let mut blobs = BTreeMap::new();
        blobs.insert(blob_ref.clone(), blob);
        let body = staged_body(&replay_params(), &prefix, &blob_seam(&blobs, &hits));
        let assistant = arr(body.get("messages").unwrap())[1].clone();
        // Provider ids render in `tool_calls[].id` when a call exists.
        if let Some(tcs) = assistant.get("tool_calls").map(|j| arr(j)) {
            assert_eq!(
                tcs[0].get("id").unwrap().as_str().unwrap(),
                "call_0",
                "{bm}"
            );
        }
        goldens.push(Json::obj([
            ("fixture", Json::str(bm)),
            ("assistant", assistant.clone()),
        ]));
    }
    let golden_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/r6_34/ac4_replay_goldens.json");
    let golden = Json::obj([
        ("schema", Json::str("hh-r6.34-replay-golden/1")),
        ("provenance", Json::str("hand_derived")),
        ("turns", Json::Arr(goldens)),
    ]);
    if std::env::var("HH_R634_WRITE_GOLDENS").is_ok() {
        std::fs::create_dir_all(golden_path.parent().unwrap()).unwrap();
        std::fs::write(&golden_path, golden.to_canonical_string()).unwrap();
    }
    let committed = parse(&std::fs::read_to_string(&golden_path).unwrap()).unwrap();
    assert_eq!(committed, golden, "replay goldens drift");
}

#[test]
fn ac4_spaced_arguments_and_provider_ids_byte_exact() {
    // `{"command": "ls -la"}` — spaced provider args round-trip
    // byte-exact under `tool_arguments = verbatim`, and provider ids
    // render in both `tool_calls[].id` and `tool_call_id`.
    let spaced = "{\"command\": \"ls -la\"}";
    let resp = format!(
        "{{\"choices\":[{{\"finish_reason\":\"stop\",\"index\":0,\"message\":{{\"content\":\"\",\"role\":\"assistant\",\"tool_calls\":[{{\"function\":{{\"arguments\":{},\"name\":\"bash\"}},\"id\":\"call_9\",\"type\":\"function\"}}]}}}}],\"id\":\"chatcmpl-syn\",\"model\":\"m\",\"object\":\"chat.completion\"}}",
        Json::str(spaced).to_canonical_string()
    );
    let exchange = RawExchange {
        status: 200,
        headers: Default::default(),
        body: resp.into_bytes(),
        chunk_stamps: vec![(0, 0)],
    };
    let blob = model_io_blob("mc.1", &exchange);
    let blob_ref = idp::address(&blob, "application/json").id();
    // The row's canonical args_raw differs from the provider bytes —
    // only the blob carries the verbatim spelling.
    let prefix = vec![completed_row(
        1,
        "mc.1",
        &blob,
        &[("mc.1.tc0", "bash", "{\"command\":\"ls -la\"}")],
    )];
    let hits = std::sync::Mutex::new(Vec::new());
    let mut blobs = BTreeMap::new();
    blobs.insert(blob_ref.clone(), blob);
    let body = staged_body(&replay_params(), &prefix, &blob_seam(&blobs, &hits));
    assert_eq!(hits.lock().unwrap().as_slice(), &[blob_ref], "blob served");
    let msgs = arr(body.get("messages").unwrap());
    let assistant = &msgs[1];
    let tc = arr(assistant.get("tool_calls").unwrap())[0].clone();
    assert_eq!(tc.get("id").unwrap().as_str().unwrap(), "call_9");
    let args = tc
        .get("function")
        .unwrap()
        .get("arguments")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(args, spaced, "verbatim args byte-exact");
    // `tool_call_id` pairs the provider id under `id_normalisation`.
    let tool = &msgs[2];
    assert_eq!(
        tool.get("tool_call_id").unwrap().as_str().unwrap(),
        "call_9"
    );
}

#[test]
fn ac4_unresolved_blob_refuses_typed() {
    let mut port = GatewayModel::new(
        lane(&replay_params(), "http://127.0.0.1:1"),
        "do it".into(),
        None,
        vec![],
    )
    .unwrap();
    let blob = model_io_blob(
        "mc.1",
        &RawExchange {
            status: 200,
            headers: Default::default(),
            body: br#"{"choices":[{"finish_reason":"stop","index":0,"message":{"content":"","role":"assistant","tool_calls":[{"function":{"arguments":"{}","name":"bash"},"id":"c0","type":"function"}]}}],"id":"x","model":"m","object":"chat.completion"}"#
                .to_vec(),
            chunk_stamps: vec![],
        },
    );
    let prefix = vec![completed_row(
        1,
        "mc.1",
        &blob,
        &[("mc.1.tc0", "bash", "{}")],
    )];
    let members = port.requested_members("mc.2", &Json::Null, &prefix, &|_: &str| None);
    assert!(
        members.iter().any(|(k, _)| *k == "plan_error"),
        "a declared replay on an unresolvable blob refuses"
    );
}

// ── AC-5 — the opt-in rule (§7.3) ────────────────────────────────────────────

#[test]
fn ac5_undeclared_renders_byte_identically() {
    // A lane declaring none of the new members renders the same
    // request bytes as the pre-R6.34 path — pinned by the golden and
    // by equality with an explicit-defaults lane.
    let prefix: Vec<EventEnvelope> = vec![];
    let base = staged_body(&[], &prefix, &|_: &str| None);
    let explicit = staged_body(
        &[
            ("transport", Json::str("stream")),
            ("stream_member", Json::str("send")),
            ("tool_choice", Json::str("auto")),
        ],
        &prefix,
        &|_: &str| None,
    );
    assert_eq!(base, explicit);
    assert!(matches!(base.get("stream"), Some(Json::Bool(true))));
    assert!(base.get("stream_options").is_some());
    // The legacy completed-row render is unchanged (kernel ids,
    // canonical args, content null, no blob read — the seam records
    // no lookups).
    let blob = model_io_blob(
        "mc.1",
        &RawExchange {
            status: 200,
            headers: Default::default(),
            body: br#"{"choices":[{"finish_reason":"stop","index":0,"message":{"content":"","role":"assistant","tool_calls":[{"function":{"arguments":"{\"command\": \"x\"}","name":"bash"},"id":"call_7","type":"function"}]}}],"id":"x","model":"m","object":"chat.completion"}"#
                .to_vec(),
            chunk_stamps: vec![],
        },
    );
    let prefix = vec![completed_row(
        1,
        "mc.1",
        &blob,
        &[("mc.1.tc0", "bash", "{\"command\":\"x\"}")],
    )];
    let hits = std::sync::Mutex::new(Vec::new());
    let mut blobs = BTreeMap::new();
    let r = idp::address(&blob, "application/json").id();
    blobs.insert(r, blob);
    let body = staged_body(&[], &prefix, &blob_seam(&blobs, &hits));
    assert!(
        hits.lock().unwrap().is_empty(),
        "legacy render reads no blob"
    );
    let msgs = arr(body.get("messages").unwrap());
    let tc = &arr(msgs[1].get("tool_calls").unwrap())[0].clone();
    assert_eq!(tc.get("id").unwrap().as_str().unwrap(), "mc.1.tc0");
    assert_eq!(
        tc.get("function")
            .unwrap()
            .get("arguments")
            .unwrap()
            .as_str()
            .unwrap(),
        "{\"command\":\"x\"}"
    );
    assert!(matches!(msgs[1].get("content").unwrap(), Json::Null));
    assert_eq!(
        msgs[2].get("tool_call_id").unwrap().as_str().unwrap(),
        "mc.1.tc0"
    );
}
