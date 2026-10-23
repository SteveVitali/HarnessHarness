//! R4.7b — the `context-adapters` *live* leg (ADR-0363; BL-31's adapter
//! member; ADR-0253/ADR-0313 fired-answered): the lane-backed
//! `Embedder`/`Summarizer`/`ProviderCompaction` adapters exercised
//! against real providers — local first (the Ollama lane,
//! `openai-compatible@1`), hosted where the provider offers the
//! capability (`openai@1` embeddings; the Messages family's
//! `anthropic-beta: compact-2026-09-04` call).
//!
//! `scripts/live-verify.sh context-adapters` drives these `#[ignore]`d
//! cells. Credentials ride `env:NAME` *coordinates* — the broker binds
//! `launch_env:NAME` inside `BrokerCredentialPort` and the value lands in
//! the request header only; nothing here prints, records or stores a
//! secret (`provided` markers are names-only). A name that is unprovided
//! or an unreachable endpoint prints `unprovided — skipped` /
//! `LIVE_SKIP:` — the leg records `skipped(<reason>)`, never a pass.
//!
//! Marker lines the leg parser reads:
//!   * `LIVE_EMBED_USAGE: <json>` — the hosted embeddings call's usage
//!     (input-only; priced `openai/text-embedding-3-small`);
//!   * `LIVE_COMPACT_USAGE: <json>` — the Messages compaction call's
//!     usage (priced `anthropic/claude-haiku-4-5-20251001`);
//!   * `LIVE_MODEL: <id>` — a provider-reported served model;
//!   * `LIVE_EMBED: …` / `LIVE_SUMM: …` / `LIVE_COMPACT: …` —
//!     per-cell outcome lines (the record's assertion view).
//!
//! `r47b_live_local_embed_batch` also *records* the hermetic half's
//! replayable cache: `tests/fixtures/r4_7b/embedding_cache.json`
//! (`hh-embedding-cache/1` — the benchset corpus slice's vectors,
//! quantized i64-ppm; texts + integers only, never a credential).

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use hh_context::compact::{SummarizeInput, SummarizeItem, Summarizer};
use hh_context::compact_family::{ProviderCompactInput, ProviderCompaction};
use hh_embed::context_adapters::{
    AdapterLane, LaneEmbedder, LaneProviderCompaction, LaneSummarizer, EMBED_SCALE_PPM,
};
use hh_wire::json::Json;

fn lane(dialect: &str, endpoint: &str, model: &str, cred: Option<&str>) -> AdapterLane {
    AdapterLane {
        dialect_ref: dialect.to_string(),
        endpoint_ref: endpoint.to_string(),
        provider_model_id: model.to_string(),
        credential_ref: cred.map(str::to_string),
        profile_ref: "profile/context-adapters-live".to_string(),
        ..AdapterLane::default()
    }
}

/// `env_or(name)` — the provisioned value (never printed); absent → the
/// leg's `unprovided — skipped` marker + `None`.
fn env_or(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => {
            println!("unprovided — skipped: {name}");
            None
        }
    }
}

/// The benchset corpus slice — *the same* committed task instructions the
/// hermetic `r4_7b` battery reads (the recorded cache must cover exactly
/// this set).
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
            let d = hh_wire::json::parse(
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

fn item(id: &str, text: &str, tokens: u64) -> SummarizeItem {
    SummarizeItem {
        context_item_id: id.to_string(),
        text: text.to_string(),
        tokens,
    }
}

/// `context-adapters` cell 1 — the local lane's embedding *batch*:
/// `POST {HH_LOCAL_BASE_URL}/embeddings` (the coordinate names the
/// base, `/v1` included) for the benchset corpus
/// slice in one request, vectors quantized i64-ppm, dimension +
/// provider-reported model recorded — then the replayable cache written
/// (`HH_EMBED_CACHE_OUT`, the committed fixture path).
///
/// The embedding model is `HH_LOCAL_EMBED_MODEL` when provisioned, else
/// `HH_LOCAL_MODEL` — a lane that names no embedding-capable model fails
/// typed (`LIVE_EMBED: skipped(<typed-error>)`), never a fabricated
/// vector.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh context-adapters"]
fn r47b_live_local_embed_batch() {
    let Some(base) = env_or("HH_LOCAL_BASE_URL") else {
        return;
    };
    let model = std::env::var("HH_LOCAL_EMBED_MODEL")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| env_or("HH_LOCAL_MODEL"))
        .unwrap();
    let emb = LaneEmbedder::new(
        lane("openai-compatible@1", &base, &model, None),
        "profile/embedder-pinned",
    )
    .unwrap();
    let texts = benchset_texts();
    let vectors = match emb.embed_batch(&texts) {
        Ok(v) => v,
        Err(e) => {
            // The provisioned local endpoint may name a capability the
            // server does not serve (llama.cpp started without
            // `--embeddings` answers the OpenAI-compatible refusal
            // verbatim) — the typed refusal IS the honest leg outcome,
            // never a fabricated vector.
            println!("LIVE_EMBED: skipped(local-endpoint-refused: {e})");
            return;
        }
    };
    let obs = emb.observed().expect("observation recorded");
    assert_eq!(vectors.len(), texts.len(), "one vector per input");
    assert!(
        obs.dimension > 0,
        "the provider's reported dimension is real"
    );
    record_cache("local-ollama", &model, &obs, &texts, &vectors);
    println!(
        "LIVE_EMBED: local model={} dim={} texts={} usage={}",
        obs.model_reported.as_deref().unwrap_or(&model),
        obs.dimension,
        texts.len(),
        emb.last_usage().to_canonical_string()
    );
    if let Some(m) = &obs.model_reported {
        println!("LIVE_MODEL: {m}");
    }
}

/// `record_cache(provider, model, obs, texts, vectors)` — the replayable
/// `hh-embedding-cache/1` fixture under `HH_EMBED_CACHE_OUT` (the leg
/// points it at the committed `tests/fixtures/r4_7b/` path): canonical
/// bytes, entries sorted by text — texts + quantized i64 components
/// only, never a credential.
fn record_cache(
    provider: &str,
    model: &str,
    obs: &hh_embed::context_adapters::EmbedObservation,
    texts: &[String],
    vectors: &[Vec<i64>],
) {
    let Ok(out) = std::env::var("HH_EMBED_CACHE_OUT") else {
        return;
    };
    assert_eq!(texts.len(), vectors.len());
    let mut entries: Vec<(String, Vec<i64>)> =
        texts.iter().cloned().zip(vectors.iter().cloned()).collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let doc = Json::obj([
        ("schema", Json::str("hh-embedding-cache/1")),
        (
            "recorded_by",
            Json::str("scripts/live-verify.sh context-adapters"),
        ),
        ("provider", Json::str(provider)),
        ("dialect", Json::str("openai-compatible@1")),
        ("model", Json::str(model)),
        (
            "model_reported",
            obs.model_reported
                .as_deref()
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
        ("embedder_ref", Json::str("profile/embedder-recorded")),
        ("scale_ppm", Json::Int(EMBED_SCALE_PPM)),
        ("dimension", Json::Int(obs.dimension as i64)),
        (
            "entries",
            Json::Arr(
                entries
                    .iter()
                    .map(|(t, v)| {
                        Json::obj([
                            ("text", Json::str(t.clone())),
                            (
                                "vector_packed",
                                Json::str(
                                    v.iter().map(i64::to_string).collect::<Vec<_>>().join(" "),
                                ),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    if let Some(parent) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&out, format!("{}\n", doc.to_canonical_string())).unwrap();
    println!("LIVE_EMBED: cache-recorded {out}");
}

/// `context-adapters` cell 2 — the local lane's *summarization*:
/// `LaneSummarizer` over `GatewayModel`'s one-shot arm (the lane's own
/// streaming SSE call, usage folded). The provider's own output count
/// rides `tokens`; the usage document is the charge record.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh context-adapters"]
fn r47b_live_local_summarize() {
    let Some(base) = env_or("HH_LOCAL_BASE_URL") else {
        return;
    };
    let Some(model) = env_or("HH_LOCAL_MODEL") else {
        return;
    };
    let sum = LaneSummarizer::new(
        lane("openai-compatible@1", &base, &model, None),
        BTreeMap::new(),
        100_000,
        256,
    )
    .unwrap();
    let out = sum
        .summarize(&SummarizeInput {
            items: vec![
                item(
                    "i1",
                    "Sprint review: the team merged the retrieval slice; \
                     the adapter ticket still owes its live leg.",
                    32,
                ),
                item(
                    "i2",
                    "Decision: local-first lanes; hosted only where the \
                     provider offers the capability.",
                    20,
                ),
            ],
            instructions_ref: None,
            max_summary_tokens: Some(64),
            previous_summary_ref: None,
        })
        .unwrap();
    assert!(!out.text.is_empty(), "a real summary, never fabricated");
    println!(
        "LIVE_SUMM: local chars={} tokens={} usage={}",
        out.text.len(),
        out.tokens,
        out.usage
            .get("view")
            .unwrap_or(&Json::Null)
            .to_canonical_string()
    );
}

/// `context-adapters` cell 3 — the hosted `openai@1` embedding:
/// `POST https://api.openai.com/v1/embeddings`, `text-embedding-3-small`,
/// the `env:OPENAI_API_KEY` coordinate bound to `launch_env:` inside the
/// broker (the value rides `Authorization:` only). Usage is input-only —
/// `LIVE_EMBED_USAGE` prices `openai/text-embedding-3-small`.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh context-adapters"]
fn r47b_live_openai_embed() {
    if env_or("OPENAI_API_KEY").is_none() {
        return;
    }
    let emb = LaneEmbedder::new(
        lane(
            "openai@1",
            "https://api.openai.com/v1",
            "text-embedding-3-small",
            Some("env:OPENAI_API_KEY"),
        ),
        "profile/embedder-pinned",
    )
    .unwrap();
    // The batch is the benchset corpus slice — the recorded vectors are
    // the hermetic battery's replayable cache (one real call, twice
    // served).
    let texts = benchset_texts();
    let vectors = emb.embed_batch(&texts).unwrap();
    let obs = emb.observed().expect("observation recorded");
    assert_eq!(vectors.len(), texts.len());
    assert_eq!(obs.dimension, 1536, "text-embedding-3-small's full dim");
    record_cache("openai", "text-embedding-3-small", &obs, &texts, &vectors);
    let usage = emb.last_usage();
    let input = usage
        .get("prompt_tokens")
        .and_then(Json::as_int)
        .unwrap_or(0);
    assert!(input > 0, "the provider's own usage reported");
    println!(
        "LIVE_EMBED_USAGE: {}",
        Json::obj([
            ("input_total", Json::Int(input)),
            ("output_total", Json::Int(0)),
        ])
        .to_canonical_string()
    );
    println!(
        "LIVE_EMBED: hosted model={} dim={} usage={}",
        obs.model_reported
            .as_deref()
            .unwrap_or("text-embedding-3-small"),
        obs.dimension,
        usage.to_canonical_string()
    );
    if let Some(m) = &obs.model_reported {
        println!("LIVE_MODEL: {m}");
    }
}

/// `context-adapters` cell 4 — the provider-native compaction binding
/// over a *real* lane: `anthropic-messages@1`'s on-demand compaction
/// (`POST /v1/messages` + `compaction:{type:"summarize"}` +
/// `anthropic-beta: compact-2026-09-04`). The cell proves the API
/// shape the hermetic battery records: `stop_reason:"compaction"`, one
/// `content[]{type:"compaction"}` block, `forgotten_ids: None` (the
/// API reports no structured ranges — `all_prior` stays verbatim;
/// ADR-0313 (b)).
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh context-adapters"]
fn r47b_live_anthropic_compact() {
    if env_or("ANTHROPIC_API_KEY").is_none() {
        return;
    }
    let comp = LaneProviderCompaction::new(
        lane(
            "anthropic-messages@1",
            "https://api.anthropic.com/v1",
            "claude-haiku-5-5",
            Some("env:ANTHROPIC_API_KEY"),
        ),
        "cap/provider_compaction@1",
    )
    .unwrap();
    let out = comp
        .provider_compact(&ProviderCompactInput {
            items: vec![
                item(
                    "i1",
                    "Design note: the context plane keeps five layers; \
                     semantic memory carries the durable facts.",
                    140,
                ),
                item(
                    "i2",
                    "Decision: compaction is model-port work; the ledger \
                     records the forgotten range and the provider's usage.",
                    120,
                ),
            ],
            capability_ref: "cap/provider_compaction@1".to_string(),
        })
        .unwrap();
    assert!(
        !out.summary_text.is_empty(),
        "a real compaction, never fabricated"
    );
    assert_eq!(
        out.forgotten_ids, None,
        "the API reports no structured forgotten ranges"
    );
    // The billed counts live in `usage.iterations[type:"compaction"]` —
    // top-level input/output are 0 on a compaction answer (the verbatim
    // usage document rides `out.usage.usage`).
    let iter = |key: &str| -> i64 {
        out.usage
            .get("usage")
            .and_then(|u| u.get("iterations"))
            .and_then(|it| match it {
                Json::Arr(rows) => Some(rows.iter().fold(0i64, |t, r| {
                    if r.get("type").and_then(Json::as_str) == Some("compaction") {
                        t + r.get(key).and_then(Json::as_int).unwrap_or(0)
                    } else {
                        t
                    }
                })),
                _ => None,
            })
            .unwrap_or(0)
    };
    let (in_tok, out_tok) = (iter("input_tokens"), iter("output_tokens"));
    assert!(
        in_tok > 0 && out_tok > 0,
        "the compaction iterations bill real tokens"
    );
    println!(
        "LIVE_COMPACT_USAGE: {}",
        Json::obj([
            ("input_total", Json::Int(in_tok)),
            ("output_total", Json::Int(out_tok)),
        ])
        .to_canonical_string()
    );
    println!(
        "LIVE_COMPACT: anthropic chars={} tokens={} stop={}",
        out.summary_text.len(),
        out.summary_tokens,
        out.usage
            .get("stop_reason")
            .and_then(Json::as_str)
            .unwrap_or("?")
    );
}
