//! R6.12 — `RecordedTransport` + `RequestCanonicalizer` battery
//! (experiments spec §4.9; REQ-XC-122; design/33 F15). Offline only
//! (REQ-XC-125) — recordings, never a socket.

use std::collections::BTreeMap;

use hharness_gateway::codec::WireFrame;
use hharness_gateway::dialects;
use hharness_gateway::gateway::{CredentialHandle, Transport};
use hharness_gateway::http_transport::{decode_exchange_frames, family_of, RawExchange};
use hharness_gateway::replay::{
    canonicalizer_for, match_requests, recorded_call, MatchVerdict, RecordedTransport,
};

const BODY: &str = r#"{"id":"c1","model":"glm-5.3-flash","choices":[{"index":0,"message":{"role":"assistant","content":"hi"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":2}}"#;
const SSE: &str = "data: {\"id\":\"c1\",\"model\":\"glm-5.3-flash\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hi\"}}]}\n\ndata: {\"id\":\"c1\",\"model\":\"glm-5.3-flash\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2}}\n\ndata: [DONE]\n\n";

fn cred() -> CredentialHandle {
    CredentialHandle {
        binding_id: None,
        provided: false,
    }
}

fn exchange(ct: &str, body: &str, stamps: Vec<(u64, usize)>) -> RawExchange {
    let mut h = BTreeMap::new();
    h.insert("content-type".to_string(), ct.to_string());
    RawExchange {
        status: 200,
        headers: h,
        body: body.as_bytes().to_vec(),
        chunk_stamps: stamps,
    }
}

/// AC-1 leg 3 — `RecordedTransport` frames equal the family's decode
/// of the same recorded bytes (JSON + SSE).
#[test]
fn recorded_transport_frames_equal_the_decoded_bytes() {
    let d = dialects::ollama_chat();
    // JSON exchange.
    let ex = exchange("application/json", BODY, vec![(0, BODY.len())]);
    let want = decode_exchange_frames(family_of(&d), 1 << 20, &ex).unwrap();
    let mut t = RecordedTransport::new(
        &d,
        "openai.chat",
        vec![recorded_call(b"{}".to_vec(), ex, None)],
    );
    let got = t.send(b"{}", &cred(), "unused").unwrap();
    assert_eq!(got.len(), want.len());
    let gw: Vec<WireFrame> = got.iter().map(|(_, f)| f.clone()).collect();
    let ww: Vec<WireFrame> = want.iter().map(|(_, f)| f.clone()).collect();
    assert_eq!(gw, ww, "native frames equal the decoded proxy bytes");
    // SSE exchange — recorded chunk arrivals stamp the frames.
    let ex = exchange(
        "text/event-stream",
        SSE,
        vec![(0, 120), (4, SSE.len() - 120)],
    );
    let want = decode_exchange_frames(family_of(&d), 1 << 20, &ex).unwrap();
    let mut t = RecordedTransport::new(
        &d,
        "openai.chat",
        vec![recorded_call(b"{}".to_vec(), ex, None)],
    );
    let got = t.send(b"{}", &cred(), "unused").unwrap();
    let gw: Vec<WireFrame> = got.iter().map(|(_, f)| f.clone()).collect();
    let ww: Vec<WireFrame> = want.iter().map(|(_, f)| f.clone()).collect();
    assert_eq!(gw, ww);
    assert_eq!(
        got.last().map(|(ms, _)| *ms),
        want.last().map(|(ms, _)| *ms)
    );
}

/// AC-1/P5 — a send beyond the sequence is the typed end-of-sequence
/// refusal (never a fabricated frame).
#[test]
fn send_beyond_sequence_is_typed_refusal() {
    let d = dialects::ollama_chat();
    let mut t = RecordedTransport::new(&d, "openai.chat", vec![]);
    let e = t.send(b"{}", &cred(), "unused").unwrap_err();
    assert_eq!(e.provider_code.as_deref(), Some("replay_exhausted"));
}

/// The matcher records per-call verdicts; strict refuses divergence.
#[test]
fn transport_matcher_verdicts_and_strict_refusal() {
    let d = dialects::ollama_chat();
    let ex = exchange("application/json", BODY, vec![(0, BODY.len())]);
    let canon = canonicalizer_for("json_canonical", &[]).unwrap();
    let mut t = RecordedTransport::new(
        &d,
        "openai.chat",
        vec![recorded_call(
            br#"{"a":1,"b":2}"#.to_vec(),
            ex.clone(),
            None,
        )],
    )
    .with_matcher(canon, false);
    let _ = t.send(br#"{ "b":2, "a":1 }"#, &cred(), "unused").unwrap();
    assert_eq!(t.verdicts(), &[MatchVerdict::CanonicalEqual]);
    // Strict — a divergence refuses typed instead of serving.
    let canon = canonicalizer_for("identity", &[]).unwrap();
    let mut t = RecordedTransport::new(
        &d,
        "openai.chat",
        vec![recorded_call(br#"{"a":1}"#.to_vec(), ex, None)],
    )
    .with_matcher(canon, true);
    let e = t.send(br#"{"a":2}"#, &cred(), "unused").unwrap_err();
    assert_eq!(e.provider_code.as_deref(), Some("replay_diverged"));
    assert!(matches!(t.verdicts()[0], MatchVerdict::Diverged { .. }));
}

/// AC-2 — `match_requests` semantics on the pure function: exact →
/// canonical_equal → diverged{first_diff_path}.
#[test]
fn match_requests_verdict_ladder() {
    let id = canonicalizer_for("identity", &[]).unwrap();
    assert_eq!(
        match_requests(id.as_ref(), "", b"x", b"x").unwrap(),
        MatchVerdict::Exact
    );
    let jc = canonicalizer_for("json_canonical", &[]).unwrap();
    assert_eq!(
        match_requests(jc.as_ref(), "", b"{\"a\":1}", b"{ \"a\":1 }").unwrap(),
        MatchVerdict::CanonicalEqual
    );
    match match_requests(jc.as_ref(), "", b"{\"a\":1}", b"{\"a\":2}").unwrap() {
        MatchVerdict::Diverged { first_diff_path } => assert_eq!(first_diff_path, "a"),
        _ => panic!("diverged expected"),
    }
}
