//! R4.7b — the lane-backed context adapters, hermetic half (BL-31's
//! adapter member; ADR-0253/ADR-0313 triggers):
//!
//! - `LaneEmbedder` over `openai-compatible@1`/`openai@1` — recorded
//!   `POST {endpoint}/embeddings` responses answered by a loopback
//!   `TcpListener` fixture (the r4_5b/r4_5d shape): request-shape,
//!   quantization, ordering, observation, usage and error legs.
//! - `LaneSummarizer` over `GatewayModel` — the SSE lane end to end,
//!   plus the port's typed `Overflow`/`Empty`/`Error`/`unresolved ref`
//!   rows.
//! - `LaneProviderCompaction` — the `anthropic-messages@1` compaction
//!   call's recorded document (beta header + `compaction` member on the
//!   wire), and the typed `Unsupported` arms for the families without a
//!   lane-native compaction API (verified capability matrix in
//!   `context_adapters`' module doc).
//! - The benchset retrieval re-run over `RecordedEmbedder` + the
//!   committed `tests/fixtures/r4_7b/embedding_cache.json` — the live
//!   leg's recorded vectors, replayable offline.
//!
//! Secrets are names-only even here: `env:HH_R47B_*` coordinates point at
//! planted test values — asserted on the wire header, never a real
//! credential.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;

use hharness_context::compact::{
    SummarizeInput, SummarizeItem, Summarizer, SummarizerError, SummarizerFailure,
};
use hharness_context::compact_family::{ProviderCompactInput, ProviderCompaction};
use hharness_context::events::CollectSink;
use hharness_context::memory::{MemoryDraft, MemoryStore, WriteContext};
use hharness_context::plan::ValidityPolicy;
use hharness_context::retrieve::{
    Embedder, RetrievalBudget, RetrievalIndexes, RetrievalRequest, SlotConstraints,
    SIMILARITY_RERANK,
};
use hharness_context::vocab::{
    ConflictPolicy, Layer, LifecycleStateKind, MemoryContent, MemoryKind, RetrievalQuery,
};
use hharness_embed::context_adapters::{
    AdapterLane, LaneEmbedder, LaneProviderCompaction, LaneSummarizer,
};
use hharness_embed_schema::errors::EmbedError;
use hharness_hir::leaves::Text;
use hharness_identity::names::ResolveMode;
use hharness_provenance::authority::AuthorityClass;
use hharness_provenance::label::Label;
use hharness_provenance::origin::Origin;
use hharness_provenance::record::ProvenanceRecord;
use hharness_provenance::PersistenceScope;
use hharness_wire::json::Json;

// ── the loopback fake provider (r4_5d's shape — head + body captured) ──

struct FakeProvider {
    endpoint: String,
    got: std::sync::Arc<std::sync::Mutex<Option<Vec<u8>>>>,
    count: std::sync::Arc<std::sync::Mutex<usize>>,
}

fn fake_provider(response: String) -> FakeProvider {
    fake_provider_n(response, 1)
}

/// `fake_provider_n` — the loopback serving `n` sequential requests
/// (the wiring leg's per-text `embed` calls each POST once); the last
/// request's head+body is what `request()` reports.
fn fake_provider_n(response: String, n: usize) -> FakeProvider {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let got = std::sync::Arc::new(std::sync::Mutex::new(None));
    let count = std::sync::Arc::new(std::sync::Mutex::new(0usize));
    let got2 = got.clone();
    let count2 = count.clone();
    std::thread::spawn(move || {
        for _ in 0..n {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
                loop {
                    if head_complete(&buf) || std::time::Instant::now() > deadline {
                        break;
                    }
                    match sock.read(&mut tmp) {
                        Ok(0) => break,
                        Ok(n) => buf.extend_from_slice(&tmp[..n]),
                        Err(_) => break,
                    }
                }
                *got2.lock().unwrap() = Some(buf);
                *count2.lock().unwrap() += 1;
                let _ = sock.write_all(response.as_bytes());
                let _ = sock.flush();
            }
        }
    });
    FakeProvider {
        endpoint: format!("http://127.0.0.1:{port}"),
        got,
        count,
    }
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

impl FakeProvider {
    fn request(&self) -> (String, Json) {
        for _ in 0..50 {
            if let Some(buf) = self.got.lock().unwrap().as_ref() {
                let p = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
                let head = String::from_utf8_lossy(&buf[..p]).to_string();
                let body =
                    hharness_wire::json::parse(&String::from_utf8_lossy(&buf[p + 4..])).unwrap();
                return (head, body);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("provider saw no request");
    }

    fn never_dialed(&self) -> bool {
        std::thread::sleep(std::time::Duration::from_millis(200));
        self.got.lock().unwrap().is_none()
    }

    fn dialed(&self, n: usize) -> bool {
        for _ in 0..100 {
            if *self.count.lock().unwrap() >= n {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        false
    }
}

fn json_response(doc: Json) -> String {
    let body = doc.to_canonical_string();
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

fn sse_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

/// A chat-completions SSE stream answering `stop` text (r4_5d's shape —
/// usage rides the trailing chunk).
fn openai_sse_text(model: &str, text: &str) -> String {
    let chunk = |delta: Json, finish: Json, usage: Json| {
        let mut c = BTreeMap::new();
        c.insert("id".into(), Json::str("chatcmpl-1"));
        c.insert("object".into(), Json::str("chat.completion.chunk"));
        c.insert("model".into(), Json::str(model));
        c.insert(
            "choices".into(),
            Json::Arr(vec![Json::obj([
                ("index", Json::Int(0)),
                ("delta", delta),
                ("finish_reason", finish),
            ])]),
        );
        if !matches!(usage, Json::Null) {
            c.insert("usage".into(), usage);
        }
        format!("data: {}\n\n", Json::Obj(c).to_canonical_string())
    };
    format!(
        "{}{}data: [DONE]\n\n",
        chunk(
            Json::obj([
                ("role", Json::str("assistant")),
                ("content", Json::str(text)),
            ]),
            Json::Null,
            Json::Null,
        ),
        chunk(
            Json::obj([]),
            Json::str("stop"),
            Json::obj([
                ("prompt_tokens", Json::Int(11)),
                ("completion_tokens", Json::Int(4)),
                ("total_tokens", Json::Int(15)),
            ]),
        ),
    )
}

// ── lanes ────────────────────────────────────────────────────────────────

fn lane(dialect: &str, endpoint: &str, model: &str, cred: Option<&str>) -> AdapterLane {
    AdapterLane {
        dialect_ref: dialect.to_string(),
        endpoint_ref: endpoint.to_string(),
        provider_model_id: model.to_string(),
        credential_ref: cred.map(str::to_string),
        profile_ref: "sha256:profile".to_string(),
        ..AdapterLane::default()
    }
}

fn item(id: &str, text: &str, tokens: u64) -> SummarizeItem {
    SummarizeItem {
        context_item_id: id.to_string(),
        text: text.to_string(),
        tokens,
    }
}

// ── LaneEmbedder ──────────────────────────────────────────────────────────

/// A recorded OpenAI-compatible embeddings document (the wire's float
/// lexemes are `Json::Str` under `parse_provider` — spelled out here
/// exactly as the provider writes them).
fn embeddings_doc(model: &str, vectors: &[Vec<&str>], usage: Json) -> Json {
    Json::obj([
        ("object", Json::str("list")),
        (
            "data",
            Json::Arr(
                vectors
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        Json::obj([
                            ("object", Json::str("embedding")),
                            (
                                "embedding",
                                Json::Arr(v.iter().map(|c| Json::str(c.to_string())).collect()),
                            ),
                            ("index", Json::Int(i as i64)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("model", Json::str(model)),
        ("usage", usage),
    ])
}

#[test]
fn embedder_arms_posts_and_quantizes_recorded_response() {
    std::env::set_var("HH_R47B_KEY", "sk-test-planted-not-a-key");
    let provider = fake_provider(json_response(embeddings_doc(
        "text-embedding-3-small",
        &[vec!["0.5", "-0.25", "2"]],
        Json::obj([
            ("prompt_tokens", Json::Int(7)),
            ("total_tokens", Json::Int(7)),
        ]),
    )));
    let emb = LaneEmbedder::new(
        lane(
            "openai@1",
            &format!("{}/v1", provider.endpoint),
            "text-embedding-3-small",
            Some("env:HH_R47B_KEY"),
        ),
        "profile/embedder-pinned",
    )
    .unwrap();
    let v = emb.embed("alpha beta").unwrap();
    // ppm quantization: 0.5→500000, -0.25→-250000, integer 2→2_000_000.
    assert_eq!(v, vec![500_000i64, -250_000, 2_000_000]);
    let obs = emb.observed().expect("observation recorded");
    assert_eq!(obs.dimension, 3);
    assert_eq!(
        obs.model_reported.as_deref(),
        Some("text-embedding-3-small")
    );
    assert_eq!(
        emb.last_usage().get("prompt_tokens").and_then(Json::as_int),
        Some(7),
        "the provider's own usage member rides verbatim"
    );
    // The wire leg: POST /v1/embeddings, Bearer auth, the documented body.
    let (head, body) = provider.request();
    assert!(head.starts_with("POST /v1/embeddings"), "{head}");
    assert!(
        head.contains("authorization: Bearer sk-test-planted-not-a-key")
            || head.contains("Authorization: Bearer sk-test-planted-not-a-key"),
        "the credential rides the header only: {head}"
    );
    assert_eq!(
        body.get("model").and_then(Json::as_str),
        Some("text-embedding-3-small")
    );
    assert_eq!(
        body.get("input").and_then(|i| match i {
            Json::Arr(a) => a.first().and_then(Json::as_str),
            _ => None,
        }),
        Some("alpha beta")
    );
    std::env::remove_var("HH_R47B_KEY");
}

#[test]
fn embedder_batch_preserves_request_order() {
    // The provider answers rows out of order — `index` re-orders (never
    // positional when the provider reports it).
    let mut doc_rows = vec![
        Json::obj([
            ("object", Json::str("embedding")),
            ("index", Json::Int(2)),
            ("embedding", Json::Arr(vec![Json::str("0.9")])),
        ]),
        Json::obj([
            ("object", Json::str("embedding")),
            ("index", Json::Int(0)),
            ("embedding", Json::Arr(vec![Json::str("0.1")])),
        ]),
        Json::obj([
            ("object", Json::str("embedding")),
            ("index", Json::Int(1)),
            ("embedding", Json::Arr(vec![Json::str("0.5")])),
        ]),
    ];
    let provider = fake_provider(json_response(Json::obj([
        ("object", Json::str("list")),
        ("data", Json::Arr(std::mem::take(&mut doc_rows))),
        ("model", Json::str("local-embed")),
    ])));
    let emb = LaneEmbedder::new(
        lane(
            "openai-compatible@1",
            &provider.endpoint,
            "local-embed",
            None,
        ),
        "profile/embedder-pinned",
    )
    .unwrap();
    let out = emb
        .embed_batch(&["t0".to_string(), "t1".to_string(), "t2".to_string()])
        .unwrap();
    assert_eq!(out, vec![vec![100_000], vec![500_000], vec![900_000]]);
}

#[test]
fn embedder_refuses_the_family_without_an_embeddings_api() {
    // Verified 2026-10-22: Anthropic publishes no embeddings API — the
    // arm is a typed `Unsupported`, never a fabricated vector.
    let err = match LaneEmbedder::new(
        lane(
            "anthropic-messages@1",
            "https://api.anthropic.com/v1",
            "claude-haiku-4-5-20251001",
            None,
        ),
        "profile/embedder-pinned",
    ) {
        Ok(_) => panic!("the embeddings-less family must refuse at arm"),
        Err(e) => e,
    };
    assert!(
        matches!(err, EmbedError::Unsupported { .. }),
        "typed Unsupported, got {err:?}"
    );
}

#[test]
fn embedder_typed_failures_never_fabricate() {
    // A partial answer (fewer rows than inputs) fails typed — no padding.
    let provider = fake_provider(json_response(embeddings_doc(
        "m",
        &[vec!["0.1"]],
        Json::Null,
    )));
    let emb = LaneEmbedder::new(
        lane("openai-compatible@1", &provider.endpoint, "m", None),
        "profile/embedder-pinned",
    )
    .unwrap();
    let err = emb
        .embed_batch(&["a".to_string(), "b".to_string()])
        .unwrap_err();
    assert!(err.contains("embeddings:"), "{err}");
    // A non-2xx answers the dialect's typed error, not a vector.
    let errdoc = Json::obj([(
        "error",
        Json::obj([
            ("message", Json::str("model does not embed")),
            ("type", Json::str("invalid_request_error")),
        ]),
    )]);
    let body = errdoc.to_canonical_string();
    let provider2 = fake_provider(format!(
        "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    ));
    let emb2 = LaneEmbedder::new(
        lane("openai-compatible@1", &provider2.endpoint, "m", None),
        "profile/embedder-pinned",
    )
    .unwrap();
    let err = emb2.embed("x").unwrap_err();
    assert!(err.contains("embeddings:"), "{err}");
    // An empty batch makes no call.
    assert!(emb.embed_batch(&[]).unwrap().is_empty());
}

// ── LaneSummarizer ────────────────────────────────────────────────────────

#[test]
fn summarizer_rides_gateway_model_and_records_usage() {
    let provider = fake_provider(sse_response(&openai_sse_text(
        "local-summ",
        "The settled decision was X.",
    )));
    let sum = LaneSummarizer::new(
        lane(
            "openai-compatible@1",
            &provider.endpoint,
            "local-summ",
            None,
        ),
        BTreeMap::new(),
        100_000,
        256,
    )
    .unwrap();
    let out = sum
        .summarize(&SummarizeInput {
            items: vec![
                item("i1", "the model decided X", 40),
                item("i2", "tests pass", 20),
            ],
            instructions_ref: None,
            max_summary_tokens: Some(64),
            previous_summary_ref: None,
        })
        .unwrap();
    assert_eq!(out.text, "The settled decision was X.");
    assert_eq!(out.tokens, 4, "the provider's own output_tokens");
    // The charge record: normalized view + raw doc + the model_io ref.
    assert!(out.usage.get("raw").is_some());
    assert!(out.usage.get("view").is_some());
    assert!(
        matches!(out.usage.get("model_io_refs"), Some(Json::Arr(a)) if !a.is_empty()),
        "the call's model_io blob is referenced"
    );
    // The wire leg: streaming chat completions, max_completion_tokens bound.
    let (head, body) = provider.request();
    assert!(head.starts_with("POST /chat/completions"), "{head}");
    assert_eq!(
        body.get("max_tokens").and_then(Json::as_int),
        Some(64),
        "the variant's bound lands on the member the dialect admits"
    );
    assert_eq!(
        body.get("stream").and_then(|s| match s {
            Json::Bool(b) => Some(*b),
            _ => None,
        }),
        Some(true)
    );
}

#[test]
fn summarizer_typed_failures_hold() {
    let provider = fake_provider(sse_response(&openai_sse_text("m", "x")));
    let sum = LaneSummarizer::new(
        lane("openai-compatible@1", &provider.endpoint, "m", None),
        BTreeMap::new(),
        50,
        256,
    )
    .unwrap();
    // Offered tokens over the summarizer's own cap → `Overflow`, and the
    // wire is never touched (the caller's InputReduction decides).
    let err = sum
        .summarize(&SummarizeInput {
            items: vec![item("i1", "x", 60)],
            instructions_ref: None,
            max_summary_tokens: None,
            previous_summary_ref: None,
        })
        .unwrap_err();
    assert_eq!(
        err,
        SummarizerError::Overflow {
            input_tokens: 60,
            cap: 50
        }
    );
    // A declared `instructions_ref` with no armed text is a typed
    // failure — the compaction prompt is never silently dropped (CC3).
    let err = sum
        .summarize(&SummarizeInput {
            items: vec![item("i1", "x", 10)],
            instructions_ref: Some("text:prompt-a".to_string()),
            max_summary_tokens: None,
            previous_summary_ref: None,
        })
        .unwrap_err();
    assert!(matches!(
        err,
        SummarizerError::Failed {
            kind: SummarizerFailure::Error,
            ..
        }
    ));
    assert!(
        provider.never_dialed(),
        "typed refusals never touch the wire"
    );
    // An empty-text completion maps to `Failed{empty}`.
    let empty = fake_provider(sse_response(&openai_sse_text("m", "")));
    let sum2 = LaneSummarizer::new(
        lane("openai-compatible@1", &empty.endpoint, "m", None),
        BTreeMap::new(),
        100_000,
        256,
    )
    .unwrap();
    let err = sum2
        .summarize(&SummarizeInput {
            items: vec![item("i1", "x", 10)],
            ..SummarizeInput::default()
        })
        .unwrap_err();
    assert!(
        matches!(
            err,
            SummarizerError::Failed {
                kind: SummarizerFailure::Empty,
                ..
            }
        ),
        "empty completion is Failed{{empty}}, got {err:?}"
    );
}

#[test]
fn summarizer_uses_the_armed_instruction_text() {
    let provider = fake_provider(sse_response(&openai_sse_text("m", "done")));
    let mut texts = BTreeMap::new();
    texts.insert(
        "text:prompt-a".to_string(),
        "Summarize tersely.".to_string(),
    );
    texts.insert(
        "text:prev-1".to_string(),
        "Prior: phase 1 done.".to_string(),
    );
    let sum = LaneSummarizer::new(
        lane("openai-compatible@1", &provider.endpoint, "m", None),
        texts,
        100_000,
        256,
    )
    .unwrap();
    sum.summarize(&SummarizeInput {
        items: vec![item("i1", "x", 10)],
        instructions_ref: Some("text:prompt-a".to_string()),
        max_summary_tokens: None,
        previous_summary_ref: Some("text:prev-1".to_string()),
    })
    .unwrap();
    let (_head, body) = provider.request();
    let prompt = body
        .get("messages")
        .and_then(|m| match m {
            Json::Arr(ms) => ms.first().cloned(),
            _ => None,
        })
        .and_then(|m| m.get("content").and_then(Json::as_str).map(str::to_string))
        .expect("prompt renders");
    assert!(prompt.contains("Summarize tersely."), "{prompt}");
    assert!(prompt.contains("Prior: phase 1 done."), "{prompt}");
}

// ── LaneProviderCompaction ────────────────────────────────────────────────

/// The recorded `compaction` answer (Anthropic on-demand compaction,
/// verified 2026-10-22 — `stop_reason:"compaction"`, one
/// `content[]{type:"compaction", content, signature}` block, usage with
/// `iterations`).
fn compaction_doc(model: &str, summary: &str) -> Json {
    Json::obj([
        ("id", Json::str("msg_compact_1")),
        ("type", Json::str("message")),
        ("role", Json::str("assistant")),
        ("model", Json::str(model)),
        (
            "content",
            Json::Arr(vec![Json::obj([
                ("type", Json::str("compaction")),
                ("content", Json::str(summary)),
                ("signature", Json::str("EqYBCkQY-recorded")),
            ])]),
        ),
        ("stop_reason", Json::str("compaction")),
        (
            "usage",
            Json::obj([
                ("input_tokens", Json::Int(1400)),
                ("output_tokens", Json::Int(96)),
                (
                    "iterations",
                    Json::Arr(vec![Json::obj([
                        ("type", Json::str("compaction")),
                        ("input_tokens", Json::Int(1400)),
                        ("output_tokens", Json::Int(96)),
                    ])]),
                ),
            ]),
        ),
    ])
}

#[test]
fn provider_compaction_binds_the_messages_family_api() {
    std::env::set_var("HH_R47B_ANT", "sk-ant-planted-not-a-key");
    let provider = fake_provider(json_response(compaction_doc(
        "claude-haiku-4-5-20251001",
        "Earlier turns settled the data model: Recipe, Ingredient, Step.",
    )));
    let comp = LaneProviderCompaction::new(
        lane(
            "anthropic-messages@1",
            &format!("{}/v1", provider.endpoint),
            "claude-haiku-4-5-20251001",
            Some("env:HH_R47B_ANT"),
        ),
        "cap/provider_compaction@1",
    )
    .unwrap();
    let out = comp
        .provider_compact(&ProviderCompactInput {
            items: vec![item("i1", "turn one", 700), item("i2", "turn two", 700)],
            capability_ref: "cap/provider_compaction@1".to_string(),
        })
        .unwrap();
    assert_eq!(
        out.summary_text,
        "Earlier turns settled the data model: Recipe, Ingredient, Step."
    );
    assert_eq!(out.summary_tokens, 96, "the provider's own output count");
    // `None` — the API reports no structured forgotten ranges; the
    // kernel's `all_prior` leg and lowering-loss apply verbatim.
    assert_eq!(out.forgotten_ids, None);
    assert_eq!(
        out.usage
            .get("usage")
            .and_then(|u| u.get("input_tokens"))
            .and_then(Json::as_int),
        Some(1400),
        "the provider's own usage is the charge record"
    );
    assert_eq!(
        out.usage.get("signature").and_then(Json::as_str),
        Some("EqYBCkQY-recorded")
    );
    // The wire leg: POST /v1/messages, beta header, `compaction` member,
    // items as user text blocks.
    let (head, body) = provider.request();
    assert!(head.starts_with("POST /v1/messages"), "{head}");
    assert!(
        head.contains("anthropic-beta: compact-2026-09-04"),
        "the beta header rides the request: {head}"
    );
    assert!(
        head.contains("x-api-key: sk-ant-planted-not-a-key"),
        "the credential rides the header only: {head}"
    );
    assert_eq!(
        body.get("compaction")
            .and_then(|c| c.get("type"))
            .and_then(Json::as_str),
        Some("summarize")
    );
    assert_eq!(
        body.get("max_tokens").and_then(Json::as_int),
        Some(4096),
        "the compaction call's own bound"
    );
    assert_eq!(
        body.get("messages").and_then(|m| match m {
            Json::Arr(v) => Some(v.len() as i64),
            _ => None,
        }),
        Some(2)
    );
    std::env::remove_var("HH_R47B_ANT");
}

#[test]
fn provider_compaction_typed_refusals_hold() {
    // A distinct env name — tests share the process env in parallel; a
    // sibling's `remove_var` must never land on this test's coordinate.
    std::env::set_var("HH_R47B_ANT_REFUSALS", "sk-ant-planted-not-a-key");
    // The families without a lane-native compaction API refuse
    // `Unsupported` at arm — never a fabricated summary (the verified
    // matrix in context_adapters' module doc: openai's compact endpoint
    // returns an opaque Responses item; the local family has none).
    for dialect in ["openai@1", "openai-compatible@1"] {
        let err = match LaneProviderCompaction::new(
            lane(dialect, "http://127.0.0.1:9", "m", None),
            "cap/provider_compaction@1",
        ) {
            Ok(_) => panic!("{dialect}: no lane-native compaction — arm must refuse"),
            Err(e) => e,
        };
        assert!(
            matches!(err, EmbedError::Unsupported { .. }),
            "{dialect}: typed Unsupported, got {err:?}"
        );
    }
    // A `capability_ref` naming anything but the armed ref is refused.
    let provider = fake_provider(json_response(compaction_doc("m", "s")));
    let comp = LaneProviderCompaction::new(
        lane(
            "anthropic-messages@1",
            &provider.endpoint,
            "m",
            Some("env:HH_R47B_ANT_REFUSALS"),
        ),
        "cap/provider_compaction@1",
    )
    .unwrap();
    let err = comp
        .provider_compact(&ProviderCompactInput {
            items: vec![item("i1", "x", 10)],
            capability_ref: "cap/other@1".to_string(),
        })
        .unwrap_err();
    assert!(err.contains("unsupported{"), "{err}");
    assert!(provider.never_dialed());
    // A non-`compaction` stop_reason is a typed failure — never coerced.
    let mut doc = compaction_doc("m", "s");
    if let Json::Obj(ref mut m) = doc {
        m.insert("stop_reason".into(), Json::str("end_turn"));
    }
    let provider2 = fake_provider(json_response(doc));
    let comp2 = LaneProviderCompaction::new(
        lane(
            "anthropic-messages@1",
            &provider2.endpoint,
            "m",
            Some("env:HH_R47B_ANT_REFUSALS"),
        ),
        "cap/provider_compaction@1",
    )
    .unwrap();
    let err = comp2
        .provider_compact(&ProviderCompactInput {
            items: vec![item("i1", "x", 10)],
            capability_ref: "cap/provider_compaction@1".to_string(),
        })
        .unwrap_err();
    assert!(err.contains("stop_reason"), "{err}");
    std::env::remove_var("HH_R47B_ANT_REFUSALS");
}

// ── the benchset retrieval re-run over the recorded embedding cache ──────

/// `RecordedEmbedder` — the committed `embedding_cache.json` replayed
/// offline (the live leg recorded the vectors once; a text absent from
/// the cache is a typed miss, never a fabricated vector).
struct RecordedEmbedder {
    embedder_ref: String,
    vectors: BTreeMap<String, Vec<i64>>,
}

impl Embedder for RecordedEmbedder {
    fn embedder_ref(&self) -> &str {
        &self.embedder_ref
    }
    fn embed(&self, text: &str) -> Result<Vec<i64>, String> {
        self.vectors
            .get(text)
            .cloned()
            .ok_or_else(|| format!("embedder_failed{{cache-miss: `{text}` — no recorded vector}}"))
    }
}

fn load_cache() -> (Json, BTreeMap<String, Vec<i64>>) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/r4_7b/embedding_cache.json");
    let doc = hharness_wire::json::parse(&std::fs::read_to_string(&path).expect("cache fixture"))
        .expect("cache parses");
    let mut vectors = BTreeMap::new();
    let entries = match doc.get("entries") {
        Some(Json::Arr(v)) => v.clone(),
        _ => panic!("cache carries entries[]"),
    };
    for e in entries {
        let text = e.get("text").and_then(Json::as_str).unwrap().to_string();
        let packed = e.get("vector_packed").and_then(Json::as_str).unwrap();
        let v: Vec<i64> = packed
            .split(' ')
            .map(|s| s.parse::<i64>().unwrap())
            .collect();
        vectors.insert(text, v);
    }
    (doc, vectors)
}

/// The benchset corpus slice — committed task instructions across the
/// strata (the hermetic benchset, `benchmarkSet: benchset.stage3.v1`).
fn benchset_texts() -> Vec<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../hh-bench/fixtures/benchset/stage3_v1");
    let files = [
        "suite.tb2/tasks/a-sum.json",
        "suite.tb2/tasks/a-upper.json",
        "suite.agentdojo/tasks/e-benign.json",
        "suite.swe_fresh/tasks/b-resolve-1.json",
        "suite.swebench/tasks/c-patch-1.json",
    ];
    files
        .iter()
        .map(|f| {
            let d = hharness_wire::json::parse(
                &std::fs::read_to_string(root.join(f)).expect("benchset task"),
            )
            .unwrap();
            d.get("instruction")
                .and_then(Json::as_str)
                .unwrap()
                .to_string()
        })
        .collect()
}

fn mem_store(texts: &[String]) -> MemoryStore {
    let mut store = MemoryStore::new("ms");
    for (i, t) in texts.iter().enumerate() {
        let g = store.lease(PersistenceScope::Run);
        let ctx = WriteContext {
            context_label: Label::top(),
            lease_generation: g,
            at_seq: i as u64 + 1,
            run_id: "run1".to_string(),
        };
        store
            .put(
                MemoryDraft {
                    kind: MemoryKind::Fact,
                    subject_key: None,
                    content: MemoryContent::Text(Box::new(Text::new(
                        t,
                        "owner",
                        ProvenanceRecord::minted(
                            Origin::human("test:author", hharness_provenance::HumanRole::Author),
                            PersistenceScope::Run,
                            i as u64,
                        ),
                    ))),
                    contract: None,
                    scope: PersistenceScope::Run,
                    declared_inputs: Vec::new(),
                    justifications: Vec::new(),
                    supersedes: None,
                    validity: None,
                    provenance: Some(ProvenanceRecord::minted(
                        Origin::model("m1", "run1", "r1"),
                        PersistenceScope::Run,
                        i as u64,
                    )),
                    semantic_id: None,
                    validator_endorsed: false,
                },
                &ctx,
            )
            .unwrap();
    }
    store
}

#[test]
fn benchset_retrieval_rerank_over_the_recorded_cache() {
    let (doc, vectors) = load_cache();
    assert_eq!(
        doc.get("schema").and_then(Json::as_str),
        Some("hh-embedding-cache/1")
    );
    let dim = doc
        .get("dimension")
        .and_then(Json::as_int)
        .expect("dimension recorded") as usize;
    let texts = benchset_texts();
    for t in &texts {
        let v = vectors
            .get(t)
            .unwrap_or_else(|| panic!("cache covers every corpus text: {t}"));
        assert_eq!(v.len(), dim, "uniform dimension");
        // Self-similarity is exact under integer ppm math (I-DET).
        assert_eq!(hharness_context::retrieve::cosine_ppm(v, v), 1_000_000);
    }
    let embedder = RecordedEmbedder {
        embedder_ref: "profile/embedder-recorded".to_string(),
        vectors,
    };
    let mut store = mem_store(&texts);
    // The probe is the first corpus instruction verbatim — its own doc
    // must be served among the hits with a perfect self-score.
    let mut sink = CollectSink::default();
    let (items, report) = hharness_context::retrieve::retrieve_indexed(
        &mut store,
        &RetrievalRequest {
            model_call_id: "mc1".to_string(),
            at: ("run1".to_string(), texts.len() as u64),
            layers: [
                Layer::Episodic,
                Layer::Procedural,
                Layer::Session,
                Layer::Artifact,
            ]
            .into_iter()
            .collect(),
            query: RetrievalQuery::Similarity {
                text: texts[0].clone(),
                embedder: "profile/embedder-recorded".to_string(),
            },
            constraints: SlotConstraints {
                slot_min_authority: AuthorityClass::Unverified,
                validity_policy: ValidityPolicy {
                    admitted_states: [
                        LifecycleStateKind::Valid,
                        LifecycleStateKind::StaleByDependency,
                        LifecycleStateKind::Unknown,
                    ]
                    .into_iter()
                    .collect(),
                    conflict_policy: ConflictPolicy::DeliverAllAnnotated,
                    max_stale: None,
                },
                readers_required: None,
            },
            reader: "model".to_string(),
            budget: RetrievalBudget {
                tokens: 10_000,
                k: 100,
            },
            ranker: SIMILARITY_RERANK.to_string(),
            mode: ResolveMode::Execute,
        },
        &mut sink,
        &RetrievalIndexes {
            lexical: None,
            structural: None,
            embedder: Some(&embedder),
        },
        || 1,
    )
    .unwrap();
    assert_eq!(report.ranker_ref, SIMILARITY_RERANK);
    assert!(!report.deterministic, "the C2 arm marks non-deterministic");
    assert!(
        report.cost.embedder_calls > texts.len() as u64,
        "every port call is accounted (probe + corpus): {}",
        report.cost.embedder_calls
    );
    assert!(
        !items.is_empty(),
        "the recorded vectors serve the corpus slice"
    );
}

// ── the boundary wiring (R4.7b) ───────────────────────────────────────────

/// The armed `model_port` lane rides the session fold: a `Similarity`
/// `memory.retrieve` through `KernelMemory` dials the lane's real
/// `embeddings` op (probe + admitted corpus item — two POSTs) instead
/// of the unbound-port unresolved row.
#[test]
fn armed_lane_serves_similarity_retrieval_through_kernel_memory() {
    let provider = fake_provider_n(
        json_response(embeddings_doc("m", &[vec!["0.9"]], Json::Null)),
        2,
    );
    let mut ctx = hharness_embed::runtime::KernelContext::for_policy(
        "run1",
        hharness_context::policy_family::DEFAULT_POLICY_REF,
    )
    .unwrap();
    ctx.adapter_lane = Some(lane("openai-compatible@1", &provider.endpoint, "m", None));
    if let hharness_embed::runtime::KernelStore::Local(store) = &mut ctx.store {
        let g = store.lease(PersistenceScope::Run);
        let wc = WriteContext {
            context_label: Label::top(),
            lease_generation: g,
            at_seq: 1,
            run_id: "run1".to_string(),
        };
        store
            .put(
                MemoryDraft {
                    kind: MemoryKind::Fact,
                    subject_key: None,
                    content: MemoryContent::Text(Box::new(Text::new(
                        "the wired fact",
                        "owner",
                        ProvenanceRecord::kernel("kernel:mem", 0),
                    ))),
                    contract: None,
                    scope: PersistenceScope::Run,
                    declared_inputs: Vec::new(),
                    justifications: Vec::new(),
                    supersedes: None,
                    validity: None,
                    provenance: Some(ProvenanceRecord::minted(
                        Origin::model("m1", "run1", "r1"),
                        PersistenceScope::Run,
                        0,
                    )),
                    semantic_id: None,
                    validator_endorsed: false,
                },
                &wc,
            )
            .unwrap();
    } else {
        panic!("local store");
    }
    let mut mem =
        hharness_embed::runtime::KernelMemory::new(std::rc::Rc::new(std::cell::RefCell::new(ctx)));
    use hharness_control::driver::MemoryPort;
    let evs = mem
        .retrieve(
            &Json::obj([
                ("kind", Json::str("similarity")),
                ("text", Json::str("the wired fact")),
                ("embedder", Json::str("profile/embedder-pinned")),
                ("deterministic", Json::Bool(false)),
            ]),
            "mc1",
            ("run1".to_string(), 1),
        )
        .unwrap();
    assert!(
        provider.dialed(2),
        "probe + corpus item each POST embeddings"
    );
    assert!(!evs.is_empty(), "the retrieve legs emit their durable rows");
}
