//! Executable coverage for the S3.7 model-plane slices: the second dialect
//! family + the 13-shape golden stream corpus, the drift probes, the
//! substitution gate, the reroute-order check, the hosted normalization
//! parity, and the invalidation-driven cold projection.
//! AC-R-2.3.1-{2,9,12,14,15} · AC-R-2.3.2-4 · AC-R-2.3.4-{7,9,14}.
//! Every test fails if the behaviour it names is removed.

use std::collections::{BTreeMap, VecDeque};

use hh_gateway::probes::{
    discovery_conformance, fingerprint_basis, fingerprint_probe_verdict, run_fingerprint_probe,
    run_transport_probes, FingerprintProbeSpec,
};
use hh_gateway::snapshot::{
    fingerprint_response, FingerprintVerdict, ModelSnapshotRecord, SnapshotClaimKind,
};
use hh_gateway::*;
use hh_provenance::origin::Origin;
use hh_provenance::{PersistenceScope, ProvenanceRecord};
use hh_registry::kinds::ConformanceVerdict;
use hh_wire::json::Json;

// ─────────────────────────────────────────────────────────────────────────────
// Fixtures (self-contained — each integration test file is its own crate)
// ─────────────────────────────────────────────────────────────────────────────

fn retention() -> Vec<RetentionClass> {
    vec![RetentionClass {
        class_id: "short".into(),
        nominal_ms: 60_000,
        guaranteed: false,
    }]
}

/// A complete `DebtRecord` for a dialect rule (T-LCD-05 — every textual rule
/// carries one; `unknown` in no member).
fn dialect_debt(what: &str) -> DebtRecord {
    DebtRecord {
        what_we_assume: what.to_string(),
        how_we_could_be_wrong: "the provider changes the spelling".into(),
        evidence: vec![DebtEvidence {
            reference: "fixture:family".into(),
            accessed: "2025-01-01".into(),
        }],
        detection_signal: "decode violation".into(),
        removal_test: "drop the rule; the corpus test fails".into(),
    }
}

/// Family A (`d.test@1`) — the Anthropic-style fixture (the same spelling set
/// `model_plane.rs`'s `dialect()` pins) plus the `ping` metadata frame and the
/// `cancelled` error code the corpus walks.
fn dialect_a() -> WireDialect {
    WireDialect {
        dialect_id: "d.test".into(),
        version: "1".into(),
        framing: Framing::HttpJson,
        plan_schema: PlanSchema {
            allowed_keys: vec!["thinking_budget".into(), "fingerprint_probe".into()],
            required_keys: vec![],
        },
        envelope_fields: vec![],
        auth_kinds: vec![AuthKind::Bearer],
        endpoint_allowlist_ref: "policy:test".into(),
        streaming: true,
        usage_arrival: UsageArrival::FinalOnly,
        usage_mapping_default: UsageMappingDefault::PreferProvider,
        usage_mapping: None,
        cache_semantics: CacheSemantics::ExplicitBreakpoints {
            max_markers: 2,
            lookback_positions: Some(8),
            position_rule: MarkerPositionRule::Block,
            retention_classes: retention(),
            min_cacheable_tokens: Some(1_024),
            isolation: IsolationScope::Workspace,
            eligible_carriers: vec![BlockKind::Text],
        },
        cache_state_visible: true,
        count_tokens: None,
        deferred_requests: false,
        model_listing: None,
        snapshot_id_exposed: None,
        retry_after_sources: vec![RetryAfterSource::Header],
        retry_after_cap_ms: 60_000,
        stream_idle_timeout_ms: 30_000,
        prefix_affecting_params: vec![],
        rules: vec![
            DialectRule::StopReasonMap {
                provider: "end_turn".into(),
                reason: StopReason::EndTurn,
            },
            DialectRule::StopReasonMap {
                provider: "tool_use".into(),
                reason: StopReason::ToolUse,
            },
            DialectRule::StopReasonMap {
                provider: "max_tokens".into(),
                reason: StopReason::MaxOutput,
            },
            DialectRule::StopReasonMap {
                provider: "stop_sequence".into(),
                reason: StopReason::StopSequence,
            },
            DialectRule::StopReasonMap {
                provider: "refusal".into(),
                reason: StopReason::Refusal,
            },
            DialectRule::StopReasonMap {
                provider: "content_filter".into(),
                reason: StopReason::ContentFilter,
            },
            DialectRule::MetadataFrame {
                kind: "ping".into(),
                debt: dialect_debt("family-A interleaves `ping` frames"),
            },
            DialectRule::ErrorCodeToClass {
                code: "cancelled".into(),
                class: ModelErrorClass::Cancelled,
                debt: dialect_debt("family-A cancels with `cancelled`"),
            },
        ],
    }
}

/// AC-R-2.3.1-2 — the second fixture family, `fixture-budget-tools-v1`
/// (`d.budget@1`): `stream_*`/`block_*` frame spellings, `stop`/`length`/
/// `tool_calls` stop-reason spellings, a `request_cancelled` error code, a
/// `meta` metadata frame, `details_nested` usage, `implicit_prefix` cache
/// semantics, `cache_state_visible = false`, `model_listing = "models"`.
fn dialect_b() -> WireDialect {
    let mut d = dialect_a();
    d.dialect_id = "d.budget".into();
    d.cache_semantics = CacheSemantics::ImplicitPrefix {
        strictness: ImplicitStrictness::Strict,
        affinity_key: AffinitySupport::Supported { max_key_length: 64 },
        retention_classes: retention(),
        min_cacheable_tokens: Some(1_024),
        reporting_granularity_tokens: Some(128),
    };
    d.cache_state_visible = false;
    d.model_listing = Some("models".into());
    d.rules = vec![
        DialectRule::StopReasonMap {
            provider: "stop".into(),
            reason: StopReason::EndTurn,
        },
        DialectRule::StopReasonMap {
            provider: "tool_calls".into(),
            reason: StopReason::ToolUse,
        },
        DialectRule::StopReasonMap {
            provider: "length".into(),
            reason: StopReason::MaxOutput,
        },
        DialectRule::StopReasonMap {
            provider: "content_filter".into(),
            reason: StopReason::ContentFilter,
        },
        DialectRule::ErrorCodeToClass {
            code: "request_cancelled".into(),
            class: ModelErrorClass::Cancelled,
            debt: dialect_debt("family-B cancels with `request_cancelled`"),
        },
        DialectRule::MetadataFrame {
            kind: "meta".into(),
            debt: dialect_debt("family-B interleaves `meta` housekeeping frames"),
        },
    ];
    d
}

fn frame(kind: &str, data: Json) -> WireFrame {
    let mut j = match data {
        Json::Obj(m) => m,
        _ => BTreeMap::new(),
    };
    j.insert("type".into(), Json::str(kind));
    WireFrame::from_json(&Json::Obj(j)).unwrap()
}

fn prov() -> ProvenanceRecord {
    // `provenance = reported` — the provider's claim lands as an imported
    // origin (CC2: asserted-by-third-party, never self-certifying).
    ProvenanceRecord::minted(
        Origin::Import {
            source_system: "provider:test".into(),
            mapping_version: "served-fields/v1".into(),
        },
        PersistenceScope::Run,
        1_700_000_000,
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// The golden corpus (AC-R-2.3.1-2) — 13 shapes × 2 dialect families
// ─────────────────────────────────────────────────────────────────────────────

/// The per-family spellings — the only member that differs between the two
/// dialects' renderings of a corpus shape.
struct Sp {
    start: &'static str,
    bstart: &'static str,
    bdelta: &'static str,
    bstop: &'static str,
    /// The mid-stream stop carrier (`message_delta` in family A; family B
    /// carries the stop on `stream_end` itself — `None` here).
    mid: Option<&'static str>,
    term: &'static str,
    text_block: &'static str,
    tool_block: &'static str,
    reasoning_block: &'static str,
    redacted_block: &'static str,
    text_delta: &'static str,
    reasoning_delta: &'static str,
    json_delta: &'static str,
    json_member: &'static str,
    meta_frame: &'static str,
    stop_end: &'static str,
    stop_len: &'static str,
    stop_tool: &'static str,
    cancel_code: &'static str,
    usage: fn() -> Json,
}

/// Family A spellings + `exclusive_cached` usage `{input_tokens,
/// cache_read_input_tokens, cache_creation_input_tokens, output_tokens}` =
/// {100, 30, 10, 20} → `input_total 140, uncached 100, read 30, write 10,
/// out 20`.
const SP_A: Sp = Sp {
    start: "message_start",
    bstart: "content_block_start",
    bdelta: "content_block_delta",
    bstop: "content_block_stop",
    mid: Some("message_delta"),
    term: "message_stop",
    text_block: "text",
    tool_block: "tool_use",
    reasoning_block: "thinking",
    redacted_block: "redacted_thinking",
    text_delta: "text_delta",
    reasoning_delta: "thinking_delta",
    json_delta: "input_json_delta",
    json_member: "partial_json",
    meta_frame: "ping",
    stop_end: "end_turn",
    stop_len: "max_tokens",
    stop_tool: "tool_use",
    cancel_code: "cancelled",
    usage: || {
        Json::obj([
            ("input_tokens", Json::Int(100)),
            ("cache_read_input_tokens", Json::Int(30)),
            ("cache_creation_input_tokens", Json::Int(10)),
            ("output_tokens", Json::Int(20)),
        ])
    },
};

/// Family B spellings + `details_nested` usage `{prompt:{total,cached,
/// cache_write}, completion:{total,reasoning}}` = {140,30,10}/{20,0} —
/// normalizes to the *same* `TokenVector` as family A.
const SP_B: Sp = Sp {
    start: "stream_start",
    bstart: "block_start",
    bdelta: "block_delta",
    bstop: "block_stop",
    mid: None,
    term: "stream_end",
    text_block: "text",
    tool_block: "tool_call",
    reasoning_block: "reasoning",
    redacted_block: "redacted_reasoning",
    text_delta: "text_delta",
    reasoning_delta: "reasoning_delta",
    json_delta: "json_delta",
    json_member: "patch",
    meta_frame: "meta",
    stop_end: "stop",
    stop_len: "length",
    stop_tool: "tool_calls",
    cancel_code: "request_cancelled",
    usage: || {
        Json::obj([
            (
                "prompt",
                Json::obj([
                    ("total", Json::Int(140)),
                    ("cached", Json::Int(30)),
                    ("cache_write", Json::Int(10)),
                ]),
            ),
            (
                "completion",
                Json::obj([("total", Json::Int(20)), ("reasoning", Json::Int(0))]),
            ),
        ])
    },
};

fn dialect_for(sp: &Sp) -> WireDialect {
    if sp.start == "message_start" {
        dialect_a()
    } else {
        dialect_b()
    }
}

fn bstart(sp: &Sp, index: i64, kind: &str, extra: &[(&str, Json)]) -> WireFrame {
    let block_member = if sp.bstart == "content_block_start" {
        "content_block"
    } else {
        "block"
    };
    let mut block: BTreeMap<String, Json> = BTreeMap::new();
    block.insert("type".into(), Json::str(kind));
    for (k, v) in extra {
        block.insert((*k).to_string(), v.clone());
    }
    frame(
        sp.bstart,
        Json::obj([
            ("index", Json::Int(index)),
            (block_member, Json::Obj(block)),
        ]),
    )
}

fn delta_frame(sp: &Sp, index: i64, dtype: &str, member: &'static str, value: &str) -> WireFrame {
    frame(
        sp.bdelta,
        Json::obj([
            ("index", Json::Int(index)),
            (
                "delta",
                Json::obj([("type", Json::str(dtype)), (member, Json::str(value))]),
            ),
        ]),
    )
}

fn bstop(sp: &Sp, index: i64) -> WireFrame {
    frame(sp.bstop, Json::obj([("index", Json::Int(index))]))
}

/// The terminal frames — family A carries the stop on `message_delta` then
/// usage on `message_stop`; family B carries both on `stream_end`. The decoded
/// event tails are identical: `usage.updated` then the terminal.
fn terminal_frames(sp: &Sp, stop: &str, usage: Json) -> Vec<WireFrame> {
    match sp.mid {
        Some(mid) => vec![
            frame(
                mid,
                Json::obj([("delta", Json::obj([("stop_reason", Json::str(stop))]))]),
            ),
            frame(sp.term, Json::obj([("usage", usage)])),
        ],
        None => vec![frame(
            sp.term,
            Json::obj([("stop_reason", Json::str(stop)), ("usage", usage)]),
        )],
    }
}

/// The 13 canonical stream shapes the corpus walks (AC-R-2.3.1-2).
const SHAPES: [&str; 13] = [
    "happy",
    "split_json_tool_call",
    "parallel_calls",
    "reasoning_signature",
    "redacted_reasoning",
    "usage_only_trailing",
    "proxy_metadata",
    "in_stream_error",
    "truncated_tool_call",
    "server_substitution",
    "cancel_mid_stream",
    "zero_chunk",
    "idle_timeout",
];

/// `shape_frames(sp, shape)` — the golden wire transcript for one shape under
/// one family's spellings. Arrival ms is fixed at 10ms strides (the
/// `idle_timeout` shape strides past the 30s timeout on purpose).
#[allow(unused_assignments)] // each arm's last `push!` leaves a dead stride.
fn shape_frames(sp: &Sp, shape: &str) -> Vec<(u64, WireFrame)> {
    let mut at = 0u64;
    let mut out: Vec<(u64, WireFrame)> = Vec::new();
    macro_rules! push {
        ($f:expr) => {{
            out.push((at, $f));
            at += 10;
        }};
    }
    let start = |model: &str| {
        frame(
            sp.start,
            Json::obj([("id", Json::str("r-1")), ("model", Json::str(model))]),
        )
    };
    match shape {
        "happy" => {
            push!(start("m-1"));
            push!(bstart(sp, 0, sp.text_block, &[]));
            push!(delta_frame(sp, 0, sp.text_delta, "text", "Hel"));
            push!(delta_frame(sp, 0, sp.text_delta, "text", "lo"));
            push!(bstop(sp, 0));
            for f in terminal_frames(sp, sp.stop_end, (sp.usage)()) {
                push!(f);
            }
        }
        "split_json_tool_call" => {
            push!(start("m-1"));
            push!(bstart(
                sp,
                0,
                sp.tool_block,
                &[("name", Json::str("lookup")), ("id", Json::str("call_1"))],
            ));
            push!(delta_frame(
                sp,
                0,
                sp.json_delta,
                sp.json_member,
                "{\"n\":2,\"q\":\"ha"
            ));
            push!(delta_frame(sp, 0, sp.json_delta, sp.json_member, "rness\""));
            push!(delta_frame(sp, 0, sp.json_delta, sp.json_member, "}"));
            push!(bstop(sp, 0));
            for f in terminal_frames(sp, sp.stop_tool, (sp.usage)()) {
                push!(f);
            }
        }
        "parallel_calls" => {
            // The grammar is sequential per index (an open block must stop
            // before the next opens); the *parallel* call set lands as two
            // `tool_call` blocks on the message — the canonical form both
            // families converge to.
            push!(start("m-1"));
            push!(bstart(
                sp,
                0,
                sp.tool_block,
                &[("name", Json::str("lookup")), ("id", Json::str("call_1"))],
            ));
            push!(delta_frame(
                sp,
                0,
                sp.json_delta,
                sp.json_member,
                "{\"q\":\"a\"}"
            ));
            push!(bstop(sp, 0));
            push!(bstart(
                sp,
                1,
                sp.tool_block,
                &[("name", Json::str("lookup")), ("id", Json::str("call_2"))],
            ));
            push!(delta_frame(
                sp,
                1,
                sp.json_delta,
                sp.json_member,
                "{\"q\":\"b\"}"
            ));
            push!(bstop(sp, 1));
            for f in terminal_frames(sp, sp.stop_tool, (sp.usage)()) {
                push!(f);
            }
        }
        "reasoning_signature" => {
            push!(start("m-1"));
            push!(bstart(sp, 0, sp.reasoning_block, &[]));
            push!(delta_frame(
                sp,
                0,
                sp.reasoning_delta,
                "thinking",
                "thinking…"
            ));
            push!(frame(
                sp.bdelta,
                Json::obj([
                    ("index", Json::Int(0)),
                    (
                        "delta",
                        Json::obj([
                            ("type", Json::str("signature_delta")),
                            ("signature", Json::str("sig-bytes")),
                        ]),
                    ),
                ]),
            ));
            push!(bstop(sp, 0));
            push!(bstart(sp, 1, sp.text_block, &[]));
            push!(delta_frame(sp, 1, sp.text_delta, "text", "done"));
            push!(bstop(sp, 1));
            for f in terminal_frames(sp, sp.stop_end, (sp.usage)()) {
                push!(f);
            }
        }
        "redacted_reasoning" => {
            push!(start("m-1"));
            push!(bstart(
                sp,
                0,
                sp.redacted_block,
                &[("data", Json::str("opaque-provider-bytes"))],
            ));
            push!(bstop(sp, 0));
            push!(bstart(sp, 1, sp.text_block, &[]));
            push!(delta_frame(sp, 1, sp.text_delta, "text", "safe answer"));
            push!(bstop(sp, 1));
            for f in terminal_frames(sp, sp.stop_end, (sp.usage)()) {
                push!(f);
            }
        }
        "usage_only_trailing" => {
            push!(start("m-1"));
            push!(bstart(sp, 0, sp.text_block, &[]));
            push!(delta_frame(sp, 0, sp.text_delta, "text", "answer"));
            push!(bstop(sp, 0));
            for f in terminal_frames(sp, sp.stop_end, (sp.usage)()) {
                push!(f);
            }
        }
        "proxy_metadata" => {
            push!(start("m-1"));
            push!(frame(
                sp.meta_frame,
                Json::obj([("trace", Json::str("proxy-trace-id"))]),
            ));
            push!(bstart(sp, 0, sp.text_block, &[]));
            push!(delta_frame(sp, 0, sp.text_delta, "text", "hi"));
            push!(bstop(sp, 0));
            for f in terminal_frames(sp, sp.stop_end, (sp.usage)()) {
                push!(f);
            }
        }
        "in_stream_error" => {
            push!(start("m-1"));
            push!(bstart(sp, 0, sp.text_block, &[]));
            push!(delta_frame(sp, 0, sp.text_delta, "text", "par"));
            push!(frame(
                "error",
                Json::obj([(
                    "error",
                    Json::obj([
                        ("code", Json::str("overloaded")),
                        ("message", Json::str("overloaded")),
                        ("status", Json::Int(529)),
                    ]),
                )]),
            ));
        }
        "truncated_tool_call" => {
            // The tool block is still open when `max_output` lands — the
            // terminal completes it `truncated` (T3).
            push!(start("m-1"));
            push!(bstart(
                sp,
                0,
                sp.tool_block,
                &[("name", Json::str("lookup")), ("id", Json::str("call_1"))],
            ));
            push!(delta_frame(
                sp,
                0,
                sp.json_delta,
                sp.json_member,
                "{\"q\":\"un"
            ));
            for f in terminal_frames(sp, sp.stop_len, (sp.usage)()) {
                push!(f);
            }
        }
        "server_substitution" => {
            push!(start("m-2-substituted"));
            push!(bstart(sp, 0, sp.text_block, &[]));
            push!(delta_frame(sp, 0, sp.text_delta, "text", "hi"));
            push!(bstop(sp, 0));
            for f in terminal_frames(sp, sp.stop_end, (sp.usage)()) {
                push!(f);
            }
        }
        "cancel_mid_stream" => {
            push!(start("m-1"));
            push!(bstart(sp, 0, sp.text_block, &[]));
            push!(delta_frame(sp, 0, sp.text_delta, "text", "par"));
            push!(frame(
                "error",
                Json::obj([(
                    "error",
                    Json::obj([
                        ("code", Json::str(sp.cancel_code)),
                        ("message", Json::str("the caller cancelled")),
                    ]),
                )]),
            ));
        }
        "zero_chunk" => {}
        "idle_timeout" => {
            push!(start("m-1"));
            at += 31_000; // stride the 30s idle timeout on the next frame
            push!(bstart(sp, 0, sp.text_block, &[]));
        }
        other => panic!("unknown shape {other}"),
    }
    out
}

/// Decode a whole transcript; return the canonical event-kind sequence.
fn decode_all(d: &WireDialect, frames: &[(u64, WireFrame)]) -> (Vec<ModelEventKind>, DecodeState) {
    let mut st = DecodeState {
        model_call_id: "mc-corpus".into(),
        ..Default::default()
    };
    let mut kinds = Vec::new();
    for (at, f) in frames {
        for ev in decode_frame(d, f, &mut st, *at, "norm:test") {
            kinds.push(ev.kind);
        }
    }
    (kinds, st)
}

/// A canonical tag for cross-family comparison — the variant plus the payload
/// members that are *semantic* (never the provider-opaque bytes, which
/// legitimately differ between families — G6 preserves the provider's own).
fn tag(k: &ModelEventKind) -> Json {
    match k {
        ModelEventKind::AttemptStarted => Json::str("attempt.started"),
        ModelEventKind::MessageStarted {
            response_id,
            served_model,
        } => Json::obj([
            ("kind", Json::str("message.started")),
            (
                "response_id",
                response_id.clone().map_or(Json::Null, Json::Str),
            ),
            (
                "served_model",
                served_model.clone().map_or(Json::Null, Json::Str),
            ),
        ]),
        ModelEventKind::BlockStarted { index, kind } => Json::obj([
            ("kind", Json::str("block.started")),
            ("index", Json::Int(*index as i64)),
            ("block_kind", Json::str(kind.as_str())),
        ]),
        ModelEventKind::BlockDelta { index, delta } => Json::obj([
            ("kind", Json::str("block.delta")),
            ("index", Json::Int(*index as i64)),
            (
                "delta_kind",
                Json::str(match delta {
                    Delta::Text(_) => "text",
                    Delta::Reasoning(_) => "reasoning",
                    Delta::ToolArgs(_) => "tool_args",
                }),
            ),
        ]),
        ModelEventKind::BlockCompleted { index, .. } => Json::obj([
            ("kind", Json::str("block.completed")),
            ("index", Json::Int(*index as i64)),
        ]),
        ModelEventKind::UsageUpdated { usage, arrival } => Json::obj([
            ("kind", Json::str("usage.updated")),
            ("input_total", Json::Int(usage.input_total)),
            ("input_uncached", Json::Int(usage.input_uncached)),
            ("cache_read", Json::Int(usage.cache_read)),
            ("cache_write", Json::Int(usage.cache_write)),
            ("output_total", Json::Int(usage.output_total)),
            ("arrival", Json::str(arrival.as_str())),
        ]),
        ModelEventKind::ProviderNotice { kind, .. } => Json::obj([
            ("kind", Json::str("provider.notice")),
            ("notice", Json::str(kind.as_str())),
        ]),
        ModelEventKind::AttemptFailed { error, .. } => Json::obj([
            ("kind", Json::str("attempt.failed")),
            ("class", Json::str(error.class.as_str())),
        ]),
        ModelEventKind::Completed {
            stop_reason,
            usage,
            message,
            ..
        } => Json::obj([
            ("kind", Json::str("completed")),
            ("stop_reason", Json::str(stop_reason.as_str())),
            ("blocks", Json::Int(message.blocks.len() as i64)),
            (
                "usage_input",
                usage
                    .as_ref()
                    .map(|u| Json::Int(u.input_total))
                    .unwrap_or(Json::Null),
            ),
            (
                "served_model",
                message.served_model.clone().map_or(Json::Null, Json::Str),
            ),
        ]),
        ModelEventKind::Failed { error, .. } => Json::obj([
            ("kind", Json::str("failed")),
            ("class", Json::str(error.class.as_str())),
        ]),
    }
}

/// The golden canonical tag sequence per shape — the corpus's oracle: the
/// exact normalized event stream each shape produces under *both* families.
fn golden(shape: &str) -> Vec<Json> {
    let msg = |served: &str| {
        Json::obj([
            ("kind", Json::str("message.started")),
            ("response_id", Json::str("r-1")),
            ("served_model", Json::str(served)),
        ])
    };
    let bs = |index: i64, kind: &str| {
        Json::obj([
            ("kind", Json::str("block.started")),
            ("index", Json::Int(index)),
            ("block_kind", Json::str(kind)),
        ])
    };
    let bd = |index: i64, dk: &str| {
        Json::obj([
            ("kind", Json::str("block.delta")),
            ("index", Json::Int(index)),
            ("delta_kind", Json::str(dk)),
        ])
    };
    let bc = |index: i64| {
        Json::obj([
            ("kind", Json::str("block.completed")),
            ("index", Json::Int(index)),
        ])
    };
    let usage_ev = || {
        Json::obj([
            ("kind", Json::str("usage.updated")),
            ("input_total", Json::Int(140)),
            ("input_uncached", Json::Int(100)),
            ("cache_read", Json::Int(30)),
            ("cache_write", Json::Int(10)),
            ("output_total", Json::Int(20)),
            ("arrival", Json::str("cumulative")),
        ])
    };
    let completed = |stop: &str, blocks: i64, served: &str| {
        Json::obj([
            ("kind", Json::str("completed")),
            ("stop_reason", Json::str(stop)),
            ("blocks", Json::Int(blocks)),
            ("usage_input", Json::Int(140)),
            ("served_model", Json::str(served)),
        ])
    };
    let failed =
        |class: &str| Json::obj([("kind", Json::str("failed")), ("class", Json::str(class))]);
    match shape {
        "happy" => vec![
            msg("m-1"),
            bs(0, "text"),
            bd(0, "text"),
            bd(0, "text"),
            bc(0),
            usage_ev(),
            completed("end_turn", 1, "m-1"),
        ],
        "split_json_tool_call" => vec![
            msg("m-1"),
            bs(0, "tool_call"),
            bd(0, "tool_args"),
            bd(0, "tool_args"),
            bd(0, "tool_args"),
            bc(0),
            usage_ev(),
            completed("tool_use", 1, "m-1"),
        ],
        "parallel_calls" => vec![
            msg("m-1"),
            bs(0, "tool_call"),
            bd(0, "tool_args"),
            bc(0),
            bs(1, "tool_call"),
            bd(1, "tool_args"),
            bc(1),
            usage_ev(),
            completed("tool_use", 2, "m-1"),
        ],
        "reasoning_signature" => vec![
            msg("m-1"),
            bs(0, "reasoning"),
            bd(0, "reasoning"),
            bc(0),
            bs(1, "text"),
            bd(1, "text"),
            bc(1),
            usage_ev(),
            completed("end_turn", 2, "m-1"),
        ],
        "redacted_reasoning" => vec![
            msg("m-1"),
            bs(0, "reasoning"),
            bc(0),
            bs(1, "text"),
            bd(1, "text"),
            bc(1),
            usage_ev(),
            completed("end_turn", 2, "m-1"),
        ],
        "usage_only_trailing" => vec![
            msg("m-1"),
            bs(0, "text"),
            bd(0, "text"),
            bc(0),
            usage_ev(),
            completed("end_turn", 1, "m-1"),
        ],
        "proxy_metadata" => vec![
            msg("m-1"),
            Json::obj([
                ("kind", Json::str("provider.notice")),
                ("notice", Json::str("other")),
            ]),
            bs(0, "text"),
            bd(0, "text"),
            bc(0),
            usage_ev(),
            completed("end_turn", 1, "m-1"),
        ],
        "in_stream_error" => vec![
            msg("m-1"),
            bs(0, "text"),
            bd(0, "text"),
            failed("overloaded"),
        ],
        "truncated_tool_call" => vec![
            msg("m-1"),
            bs(0, "tool_call"),
            bd(0, "tool_args"),
            usage_ev(),
            completed("max_output", 1, "m-1"),
        ],
        "server_substitution" => vec![
            msg("m-2-substituted"),
            bs(0, "text"),
            bd(0, "text"),
            bc(0),
            usage_ev(),
            completed("end_turn", 1, "m-2-substituted"),
        ],
        "cancel_mid_stream" => vec![
            msg("m-1"),
            bs(0, "text"),
            bd(0, "text"),
            failed("cancelled"),
        ],
        "zero_chunk" => vec![],
        "idle_timeout" => vec![msg("m-1"), failed("timeout{stream_idle}")],
        other => panic!("unknown shape {other}"),
    }
}

/// AC-R-2.3.1-2 — the golden corpus: all 13 shapes × both dialect families
/// decode to the *same* canonical event sequence (the provider-opaque bytes
/// are the only member allowed to differ — they are the provider's own bytes,
/// preserved verbatim under G6).
#[test]
fn golden_corpus_thirteen_shapes_two_families() {
    for shape in SHAPES {
        let want = golden(shape);
        for sp in [&SP_A, &SP_B] {
            let d = dialect_for(sp);
            let (kinds, st) = decode_all(&d, &shape_frames(sp, shape));
            let got: Vec<Json> = kinds.iter().map(tag).collect();
            assert_eq!(got, want, "{shape} under {}", sp.start);
            // `idle_timeout` records the G7 violation under both families.
            if shape == "idle_timeout" {
                assert!(st
                    .violations
                    .iter()
                    .any(|v| matches!(v, GrammarViolation::IdleTimeout { .. })));
            }
        }
    }
}

/// Per-shape semantic payload assertions — the corpus's member-level checks
/// (split args parse, truncation flags, signature/redacted leaves, the
/// served-model propagation).
#[test]
fn golden_corpus_semantic_members() {
    for sp in [&SP_A, &SP_B] {
        let d = dialect_for(sp);
        let label = sp.start;
        let completed_msg = |shape: &str| -> ModelMessage {
            let (kinds, _) = decode_all(&d, &shape_frames(sp, shape));
            kinds
                .iter()
                .find_map(|k| match k {
                    ModelEventKind::Completed { message, .. } => Some((**message).clone()),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{label}/{shape}: no completed event"))
        };
        // split_json_tool_call — the reassembled args parse to the object.
        let msg = completed_msg("split_json_tool_call");
        match &msg.blocks[0] {
            ModelBlock::ToolCall(t) => {
                assert!(!t.truncated, "{label}");
                match &t.arguments {
                    ToolArgs::Parsed(value) => {
                        assert_eq!(value.get("q").and_then(Json::as_str), Some("harness"))
                    }
                    other => panic!("{label}: expected parsed args, got {other:?}"),
                }
                assert_eq!(t.surface_name, "lookup");
            }
            other => panic!("{label}: expected tool_call, got {other:?}"),
        }
        // parallel_calls — two ToolCall blocks, both parsed.
        let msg = completed_msg("parallel_calls");
        assert_eq!(msg.blocks.len(), 2, "{label}");
        assert!(msg
            .blocks
            .iter()
            .all(|b| matches!(b, ModelBlock::ToolCall(_))));
        // reasoning_signature — the signature leaf rode the block (T6).
        let msg = completed_msg("reasoning_signature");
        match &msg.blocks[0] {
            ModelBlock::Reasoning {
                signature,
                redacted,
                ..
            } => {
                assert!(!redacted, "{label}");
                assert_eq!(
                    signature.as_ref().map(|s| s.bytes.as_slice()),
                    Some(b"sig-bytes".as_slice()),
                    "{label}"
                );
            }
            other => panic!("{label}: expected reasoning, got {other:?}"),
        }
        // redacted_reasoning — opaque bytes preserved, `redacted` set.
        let msg = completed_msg("redacted_reasoning");
        match &msg.blocks[0] {
            ModelBlock::Reasoning { redacted, .. } => assert!(*redacted, "{label}"),
            other => panic!("{label}: expected redacted reasoning, got {other:?}"),
        }
        // truncated_tool_call — `max_output` with the block open ⇒
        // `truncated` + `Unparseable{truncated}` with the raw fragment kept.
        let msg = completed_msg("truncated_tool_call");
        match &msg.blocks[0] {
            ModelBlock::ToolCall(t) => {
                assert!(t.truncated, "{label}");
                match &t.arguments {
                    ToolArgs::Unparseable { raw, reason } => {
                        assert_eq!(*reason, UnparseableReason::Truncated, "{label}");
                        assert!(raw.contains("\"q\":\"un"), "{label}");
                    }
                    other => panic!("{label}: expected unparseable, got {other:?}"),
                }
            }
            other => panic!("{label}: expected tool_call, got {other:?}"),
        }
        // server_substitution — the served coordinate rides the message.
        let msg = completed_msg("server_substitution");
        assert_eq!(
            msg.served_model.as_deref(),
            Some("m-2-substituted"),
            "{label}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Scripted transport + gateway rig (the probes and the substitution gate)
// ─────────────────────────────────────────────────────────────────────────────

/// A transport that serves queued frame responses (one `Vec` per `send`) and
/// answers capability probes from a pinned table. `canary_ok` makes an
/// *undeclared* probe succeed — the AC-R-2.3.1-14 honesty case.
struct ScriptedTransport {
    responses: VecDeque<Vec<(u64, WireFrame)>>,
    probe_values: BTreeMap<String, Json>,
    canary_ok: bool,
    descriptor: Option<Json>,
    sends: usize,
}

impl ScriptedTransport {
    fn new() -> Self {
        ScriptedTransport {
            responses: VecDeque::new(),
            probe_values: BTreeMap::new(),
            canary_ok: false,
            descriptor: None,
            sends: 0,
        }
    }
}

impl Transport for ScriptedTransport {
    fn send(
        &mut self,
        _b: &[u8],
        _c: &CredentialHandle,
        _e: &str,
    ) -> Result<Vec<(u64, WireFrame)>, ModelError> {
        self.sends += 1;
        Ok(self.responses.pop_front().unwrap_or_default())
    }
    fn discover(&mut self, _e: &str) -> Result<Json, ModelError> {
        self.descriptor
            .clone()
            .ok_or_else(|| ModelError::new(ModelErrorClass::UnsupportedFeature, "no descriptor"))
    }
    fn probe_capability(&mut self, capability: &str, _e: &str) -> Result<Json, ModelError> {
        if self.canary_ok {
            return Ok(Json::str("answered"));
        }
        self.probe_values.get(capability).cloned().ok_or_else(|| {
            ModelError::new(
                ModelErrorClass::UnsupportedFeature,
                format!("no probe for {capability}"),
            )
        })
    }
}

struct StubCreds;
impl CredentialPort for StubCreds {
    fn bind(
        &mut self,
        credential_ref: Option<&str>,
        _endpoint: &str,
        _auth: &[AuthKind],
    ) -> Result<CredentialHandle, GatewayError> {
        Ok(CredentialHandle {
            binding_id: credential_ref.map(str::to_string),
            provided: credential_ref.is_some(),
        })
    }
}

/// The gateway over a scripted transport (families A + B loaded). The ports
/// are leaked into 'static fixtures — the test-scoped idiom. A call count is
/// asserted through the emitted `model.call.requested` rows (exactly one per
/// call).
fn scripted_gateway(t: ScriptedTransport) -> ModelGateway<'static> {
    let transport: &'static mut ScriptedTransport = Box::leak(Box::new(t));
    let creds: &'static mut StubCreds = Box::leak(Box::new(StubCreds));
    let mut g = ModelGateway {
        dialects: BTreeMap::new(),
        endpoints: EndpointAllowlist {
            policy_ref: "policy:test".into(),
            endpoints: ["ep.test".to_string()].into_iter().collect(),
        },
        credentials: creds,
        transport,
        now_ms: Box::new(|| 1_000u64),
        normalizer_ref: "norm:test".into(),
    };
    g.load_dialect(dialect_a());
    g.load_dialect(dialect_b());
    g
}

fn request_fixture(dialect_id: &str, reservation: Option<&str>) -> InferenceRequest {
    let plan = ProviderRequestPlan {
        dialect_id: dialect_id.into(),
        dialect_version: "1".into(),
        endpoint_ref: "ep.test".into(),
        model_ref: "m-1".into(),
        body: Json::obj([("messages", Json::Arr(vec![]))]),
        provider_params: Extensions::default(),
        capabilities: Capabilities {
            streaming: true,
            tool_use: false,
            vision: None,
            deferred: None,
        },
        cache: CachePlan::default(),
        budget_reservation_id: reservation.map(str::to_string),
        canonical_len: 0,
    };
    InferenceRequest {
        model_call_id: "mc-1".into(),
        model_ref: ModelRef {
            profile_ref: "prof.test@1".into(),
            provider_model_id: "m-1".into(),
            snapshot_id: None,
            serving_route: Some("route-a".into()),
            effort: None,
        },
        plan,
        role: "primary".into(),
        purpose: Purpose::Main,
        context_label: None,
        view_hash: None,
        binding_ref: "b.test".into(),
        credential_ref: Some("cred.test".into()),
        budget_reservation_id: reservation.map(str::to_string),
        attempt_policy: AttemptPolicy::c0(),
        deadline_ms: None,
        cache_state_hint: ExpectedState::Unknown,
        seed_request: None,
        observability: CallObservability::default(),
        request_class: RequestClass::Interactive,
        deferred_deadline_ms: None,
        stream: true,
        substitution_allowed: None,
        participant_class: None,
        identity: None,
    }
}

fn sink_events(sink: &CollectSink) -> Vec<(String, Json)> {
    sink.events.clone()
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.3.1-9 — the served-model substitution gate
// ─────────────────────────────────────────────────────────────────────────────

/// `substitution_allowed = false` + a drifted `served_model` ⇒ terminal
/// `failed{served_model_mismatch}` — never a silent completed.
#[test]
fn served_model_substitution_refused() {
    let mut t = ScriptedTransport::new();
    t.responses
        .push_back(shape_frames(&SP_A, "server_substitution"));
    let mut g = scripted_gateway(t);
    let mut sink = CollectSink::default();
    let mut req = request_fixture("d.test", Some("res-1"));
    req.substitution_allowed = Some(false);
    let handle = g.open_call(req, &mut sink).expect("open");
    let outcome = g.stream(&handle, &mut sink).expect("stream");
    let err = outcome.result.expect_err("substitution refused");
    assert_eq!(err.class, ModelErrorClass::ServedModelMismatch);
    let failed = sink_events(&sink)
        .into_iter()
        .find(|(c, _)| c == "model.call.failed")
        .expect("terminal failed row");
    assert_eq!(
        failed
            .1
            .get("error")
            .and_then(|e| e.get("class"))
            .and_then(Json::as_str),
        Some("served_model_mismatch")
    );
}

/// `substitution_allowed = true` + a drifted `served_model` ⇒ completed with
/// the served-model stamps; `charge_model` keys the charge at the *served*
/// coordinate.
#[test]
fn served_model_substitution_allowed_stamps() {
    let mut t = ScriptedTransport::new();
    t.responses
        .push_back(shape_frames(&SP_A, "server_substitution"));
    let mut g = scripted_gateway(t);
    let mut sink = CollectSink::default();
    let mut req = request_fixture("d.test", Some("res-1"));
    req.substitution_allowed = Some(true);
    let handle = g.open_call(req.clone(), &mut sink).expect("open");
    let outcome = g.stream(&handle, &mut sink).expect("stream");
    let message = outcome.result.expect("completed");
    assert_eq!(message.served_model.as_deref(), Some("m-2-substituted"));
    let completed = sink_events(&sink)
        .into_iter()
        .find(|(c, _)| c == "model.call.completed")
        .expect("terminal completed row");
    assert_eq!(
        completed.1.get("served_model").and_then(Json::as_str),
        Some("m-2-substituted")
    );
    // The charge keys at the served coordinate — never the requested one.
    let charged = charge_model(&message, &req);
    assert_eq!(charged.provider_model_id, "m-2-substituted");
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.3.1-12 — the fingerprint probe
// ─────────────────────────────────────────────────────────────────────────────

/// The fingerprint digest of one scripted response — the pin's
/// `observed_fingerprint` for a `match` run.
fn digest_of(d: &WireDialect, frames: &[(u64, WireFrame)]) -> String {
    let (kinds, _) = decode_all(d, frames);
    let msg = kinds
        .iter()
        .find_map(|k| match k {
            ModelEventKind::Completed { message, .. } => Some((**message).clone()),
            _ => None,
        })
        .expect("completed");
    fingerprint_response(fingerprint_basis(&msg).to_canonical_string().as_bytes())
}

fn pinned_snapshot(fp: Option<String>) -> ModelSnapshotRecord {
    ModelSnapshotRecord {
        provider: "test".into(),
        model_id: "m-1".into(),
        snapshot_id: Some("snap-1".into()),
        pinned: true,
        observed_fingerprint: fp,
        provenance: prov(),
    }
}

fn probe_spec(n: usize) -> FingerprintProbeSpec {
    FingerprintProbeSpec {
        probe_id: "probe-1".into(),
        model_id: "m-1".into(),
        snapshot_id: Some("snap-1".into()),
        prompts: (0..n).map(|i| format!("prompt-{i}")).collect(),
        prompt_band_min: 1.0,
        tokenizer_expected: None,
        semantic_expected: None,
        budget_max_units: Some(1_000),
        estimated_charge: Some(10),
    }
}

/// AC-R-2.3.1-12 — the fingerprint probe rides the normal call path with
/// `purpose = probe` (every prompt a measured call), agrees with the pin →
/// `match`, and mutates nothing on the pinned record.
#[test]
fn fingerprint_probe_match() {
    let happy = shape_frames(&SP_A, "happy");
    let digest = digest_of(&dialect_a(), &happy);
    let pinned = pinned_snapshot(Some(digest.clone()));
    let mut t = ScriptedTransport::new();
    for _ in 0..3 {
        t.responses.push_back(happy.clone());
    }
    let mut g = scripted_gateway(t);
    let mut sink = CollectSink::default();
    let make = |i: usize, prompt: &str| {
        let mut r = request_fixture("d.test", Some("res-probe"));
        r.model_call_id = format!("mc-probe-{i}");
        r.plan.body = Json::obj([("prompt", Json::str(prompt))]);
        r
    };
    let out = run_fingerprint_probe(&probe_spec(3), &pinned, &make, &mut g, &mut sink, prov())
        .expect("probe ran");
    assert_eq!(out.verdict, FingerprintVerdict::Match);
    assert_eq!(out.calls, 3);

    assert_eq!(out.per_prompt, vec![true, true, true]);
    assert!(out.claim.is_none(), "a match issues no claim");
    assert_eq!(
        pinned.observed_fingerprint.as_deref(),
        Some(digest.as_str())
    );
    // Every probe call emitted `model.call.requested` with `purpose = probe`
    // — the accounting projection's `charged_to = instrument` marker.
    let probe_requests = sink_events(&sink)
        .into_iter()
        .filter(|(c, p)| {
            c == "model.call.requested"
                && p.get("cache")
                    .and_then(|c| c.get("purpose"))
                    .and_then(Json::as_str)
                    == Some("probe")
        })
        .count();
    assert_eq!(probe_requests, 3);
}

/// AC-R-2.3.1-12 — a contradicted fingerprint ⇒ `Drift` + the `SnapshotClaim`
/// (`fingerprint_drift`); the pinned record itself is untouched.
#[test]
fn fingerprint_probe_drift_issues_claim() {
    let happy = shape_frames(&SP_A, "happy");
    // The pin names a fingerprint the wire contradicts.
    let pinned = pinned_snapshot(Some(fingerprint_response(b"someone-else")));
    let mut t = ScriptedTransport::new();
    for _ in 0..2 {
        t.responses.push_back(happy.clone());
    }
    let mut g = scripted_gateway(t);
    let mut sink = CollectSink::default();
    let make = |i: usize, _prompt: &str| {
        let mut r = request_fixture("d.test", Some("res-probe"));
        r.model_call_id = format!("mc-probe-{i}");
        r
    };
    let out = run_fingerprint_probe(&probe_spec(2), &pinned, &make, &mut g, &mut sink, prov())
        .expect("probe ran");
    assert_eq!(out.verdict, FingerprintVerdict::Drift);
    let claim = out.claim.expect("drift issues a claim");
    assert_eq!(claim.claim_kind, SnapshotClaimKind::FingerprintDrift);
    assert_eq!(claim.pinned_model_id, "m-1");
}

/// The pure verdict rule — no observation ⇒ `unknown`, never a fabricated
/// drift (T-LCD-07 on the probe surface).
#[test]
fn fingerprint_probe_verdict_honest_unknown() {
    let spec = probe_spec(2);
    let pinned = pinned_snapshot(None);
    let (v, _) = fingerprint_probe_verdict(&spec, &pinned, None, &[], None, None);
    assert_eq!(v, FingerprintVerdict::Unknown);
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.3.1-14 — transport probes + discovery → ConformanceRecord / DRIFT
// ─────────────────────────────────────────────────────────────────────────────

/// `run_transport_probes` — every declared capability row is probed; a pinned
/// value the transport contradicts produces a `DRIFT` row; an
/// *undeclared*-but-answering capability (the honesty canary) produces a
/// `DRIFT` row, never a silent pass.
#[test]
fn transport_probes_pinned_vs_observed() {
    let d = dialect_b(); // `cache_state_visible=false`, `model_listing=models`
    let mut t = ScriptedTransport::new();
    // The transport reports exactly the declared surface.
    for (cap, _) in hh_gateway::probes::describe(&d).entries {
        t.probe_values.insert(
            cap.clone(),
            match cap.as_str() {
                "cap.streaming" | "cap.model_listing" => Json::Bool(true),
                "cache.kind" => Json::str("implicit_prefix"),
                _ => Json::Bool(false),
            },
        );
    }
    // The pin agrees on everything except `cap.streaming` (drift) — and one
    // capability stays unpinned (`declared = null`).
    let mut pinned = BTreeMap::new();
    for (cap, v) in &t.probe_values {
        pinned.insert(cap.clone(), v.clone());
    }
    pinned.insert("cap.streaming".into(), Json::Bool(false)); // contradicts
    pinned.remove("cap.deferred_requests"); // unpinned → declared = null
    let records = run_transport_probes(&d, &mut t, "ep.test", &pinned, &[], 1_700_000_000)
        .expect("probe run");
    assert!(!records.is_empty());
    let streaming = records
        .iter()
        .find(|r| r.dimension == "cap.streaming")
        .expect("streaming row");
    assert_eq!(streaming.verdict, ConformanceVerdict::Drift);
    // The canary: an undeclared operation the transport answers ⇒ DRIFT.
    let mut t2 = ScriptedTransport::new();
    t2.canary_ok = true;
    let records = run_transport_probes(
        &d,
        &mut t2,
        "ep.test",
        &BTreeMap::new(),
        &["cap.batch_api".to_string()],
        1_700_000_000,
    )
    .expect("probe run");
    let canary = records
        .iter()
        .find(|r| r.dimension == "cap.batch_api")
        .expect("canary row");
    assert_eq!(canary.verdict, ConformanceVerdict::Drift);
}

/// `discovery_conformance` — a descriptor claim the pin contradicts is a
/// `DRIFT` row; unpinned claims project their honest verdict.
#[test]
fn discovery_drift_rows() {
    let descriptor = Json::obj([
        ("api_version", Json::str("v2")),
        ("capabilities", Json::obj([("streaming", Json::Bool(true))])),
        (
            "served_models",
            Json::Arr(vec![Json::str("m-1"), Json::str("m-9")]),
        ),
    ]);
    let mut pinned = BTreeMap::new();
    pinned.insert("api_version".into(), Json::str("v1")); // contradicts → DRIFT
    pinned.insert("capability.streaming".into(), Json::Bool(true)); // agrees
    pinned.insert("served_model.m-1".into(), Json::Bool(true)); // agrees
    pinned.insert("limit.rpm".into(), Json::Int(60)); // unobserved → unknown
    let records = discovery_conformance("provider:test/m-1", &descriptor, &pinned, 1_700_000_000);
    let by_id: BTreeMap<_, _> = records.iter().map(|r| (r.dimension.as_str(), r)).collect();
    assert_eq!(by_id["api_version"].verdict, ConformanceVerdict::Drift);
    assert_eq!(
        by_id["capability.streaming"].verdict,
        ConformanceVerdict::Supported
    );
    assert_eq!(
        by_id["served_model.m-1"].verdict,
        ConformanceVerdict::Supported
    );
    // `served_model.m-9` was observed but never pinned — the projection is
    // honest (declared = null), never a fabricated drift.
    let m9 = by_id["served_model.m-9"];
    assert_ne!(m9.verdict, ConformanceVerdict::Drift);
    assert_eq!(m9.declared, Json::Null);
    // A pinned claim the descriptor never observed is `unknown`, not drift.
    let rpm = by_id["limit.rpm"];
    assert_ne!(rpm.verdict, ConformanceVerdict::Drift);
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.3.2-4 — the reroute ordering check
// ─────────────────────────────────────────────────────────────────────────────

/// `check_reroute_order` — the ordering `attempt.failed →
/// model.surface.relowered → model.rerouted → attempt.started` is enforced;
/// a same-profile reroute carries no relower row, and a missing member fails
/// with the typed error.
#[test]
fn reroute_order_checked() {
    let row = |seq: u64, class: &str, p: Json| (seq, class.to_string(), p);
    let mc = Json::obj([("model_call_id", Json::str("mc-1"))]);
    let with = |extra: &[(&str, Json)]| {
        let mut m = match mc.clone() {
            Json::Obj(m) => m,
            _ => unreachable!(),
        };
        for (k, v) in extra {
            m.insert((*k).to_string(), v.clone());
        }
        Json::Obj(m)
    };
    let good = vec![
        row(1, "model.call.attempt.failed", mc.clone()),
        row(
            2,
            "model.surface.relowered",
            with(&[
                ("old_profile_ref", Json::str("prof.a@1")),
                ("new_profile_ref", Json::str("prof.b@1")),
                ("reason", Json::str("incompatible_dialect")),
            ]),
        ),
        row(
            3,
            "model.rerouted",
            with(&[("relowered", Json::Bool(true))]),
        ),
        row(4, "model.call.attempt.started", mc.clone()),
    ];
    assert_eq!(check_reroute_order(&good, "mc-1"), Ok(()));
    // Same-profile reroute — no relower row.
    let same_profile = vec![
        row(1, "model.call.attempt.failed", mc.clone()),
        row(
            3,
            "model.rerouted",
            with(&[("relowered", Json::Bool(false))]),
        ),
        row(4, "model.call.attempt.started", mc.clone()),
    ];
    assert_eq!(check_reroute_order(&same_profile, "mc-1"), Ok(()));
    // Cross-profile without the relower row ⇒ MissingRelower.
    let missing_relower = vec![
        row(1, "model.call.attempt.failed", mc.clone()),
        row(
            3,
            "model.rerouted",
            with(&[("relowered", Json::Bool(true))]),
        ),
        row(4, "model.call.attempt.started", mc.clone()),
    ];
    assert!(matches!(
        check_reroute_order(&missing_relower, "mc-1"),
        Err(RerouteOrderError::MissingRelower { .. })
    ));
    // A relower row under a same-profile reroute ⇒ UnexpectedRelower.
    let unexpected = vec![
        row(1, "model.call.attempt.failed", mc.clone()),
        row(
            2,
            "model.surface.relowered",
            with(&[
                ("old_profile_ref", Json::str("prof.a@1")),
                ("new_profile_ref", Json::str("prof.a@1")),
                ("reason", Json::str("x")),
            ]),
        ),
        row(
            3,
            "model.rerouted",
            with(&[("relowered", Json::Bool(false))]),
        ),
        row(4, "model.call.attempt.started", mc.clone()),
    ];
    assert!(matches!(
        check_reroute_order(&unexpected, "mc-1"),
        Err(RerouteOrderError::UnexpectedRelower { .. })
    ));
    // No restart after the reroute ⇒ MissingRestart.
    let no_restart = vec![
        row(1, "model.call.attempt.failed", mc.clone()),
        row(
            3,
            "model.rerouted",
            with(&[("relowered", Json::Bool(false))]),
        ),
    ];
    assert!(matches!(
        check_reroute_order(&no_restart, "mc-1"),
        Err(RerouteOrderError::MissingRestart { .. })
    ));
    // A reroute with no failed attempt ⇒ NoFailedAttempt.
    let no_failure = vec![
        row(
            3,
            "model.rerouted",
            with(&[("relowered", Json::Bool(false))]),
        ),
        row(4, "model.call.attempt.started", mc.clone()),
    ];
    assert!(matches!(
        check_reroute_order(&no_failure, "mc-1"),
        Err(RerouteOrderError::NoFailedAttempt { .. })
    ));
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.3.1-15 — hosted normalization parity
// ─────────────────────────────────────────────────────────────────────────────

/// A `participant_class = "hosted"` call emits the same event *classes* in the
/// same order as a native call, with `participant_class` stamped on every
/// payload — the hosted path lowers through the same normalization, never a
/// side channel.
#[test]
fn hosted_normalization_parity() {
    let classes = |participant: Option<&str>| {
        let mut t = ScriptedTransport::new();
        t.responses.push_back(shape_frames(&SP_A, "happy"));
        let mut g = scripted_gateway(t);
        let mut sink = CollectSink::default();
        let mut req = request_fixture("d.test", Some("res-1"));
        req.participant_class = participant.map(str::to_string);
        let handle = g.open_call(req, &mut sink).expect("open");
        let outcome = g.stream(&handle, &mut sink).expect("stream");
        assert!(outcome.result.is_ok());
        sink_events(&sink)
    };
    let native = classes(None);
    let hosted = classes(Some("hosted"));
    let native_classes: Vec<&str> = native.iter().map(|(c, _)| c.as_str()).collect();
    let hosted_classes: Vec<&str> = hosted.iter().map(|(c, _)| c.as_str()).collect();
    assert_eq!(native_classes, hosted_classes);
    for (_, payload) in &hosted {
        assert_eq!(
            payload.get("participant_class").and_then(Json::as_str),
            Some("hosted")
        );
    }
    for (_, payload) in &native {
        assert!(payload.get("participant_class").is_none());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-R-2.3.4-7/-9 — invalidation inputs drive the cold projection
// ─────────────────────────────────────────────────────────────────────────────

/// A committed invalidation makes the next same-key call `cold{reason}` —
/// the lease arithmetic never produces `warm` past a recorded invalidation;
/// the full `miss_reason` vocabulary round-trips and a malformed member is
/// refused.
#[test]
fn invalidation_drives_cold() {
    let semantics = match dialect_a().cache_semantics {
        s @ CacheSemantics::ExplicitBreakpoints { .. } => s,
        _ => unreachable!(),
    };
    let prior = CacheCallFact {
        affinity_key: "k".into(),
        static_hash: "h".into(),
        request_sent_ms: 0,
        completed: true,
        model_call_id: "mc-0".into(),
    };
    // Warm inside the lease without invalidations (the baseline).
    let e = expect_cache_state(
        std::slice::from_ref(&prior),
        "k",
        "h",
        5_000,
        Some(2_000),
        &semantics,
        1_000,
        &[],
    );
    assert_eq!(e.expected, ExpectedState::Warm);
    // Every recorded invalidation ⇒ cold with the reason recorded.
    for reason in [
        MissReason::Relower,
        MissReason::ContextEdit,
        MissReason::Compaction,
        MissReason::ProfileVersion,
        MissReason::DefinitionVersion,
        MissReason::ToolSetChange,
        MissReason::MarkerPolicy,
        MissReason::DialectChange,
        MissReason::ParameterChange,
        MissReason::ProviderEviction,
    ] {
        let e = expect_cache_state(
            std::slice::from_ref(&prior),
            "k",
            "h",
            5_000,
            Some(2_000),
            &semantics,
            1_000,
            &[reason],
        );
        assert_eq!(
            e.expected,
            ExpectedState::Cold,
            "{} must project cold",
            reason.as_str()
        );
        assert_eq!(e.basis.cold_reason, Some(reason));
    }
    // The closed vocabulary round-trips; a malformed member is refused.
    for r in [
        MissReason::TtlElapsed,
        MissReason::ConcurrentSibling,
        MissReason::ProviderEviction,
        MissReason::ParameterChange,
        MissReason::Compaction,
        MissReason::ContextEdit,
        MissReason::Relower,
        MissReason::ProfileVersion,
        MissReason::DefinitionVersion,
        MissReason::ToolSetChange,
        MissReason::MarkerPolicy,
        MissReason::DialectChange,
        MissReason::Unknown,
    ] {
        assert_eq!(MissReason::parse(r.as_str()), Some(r));
    }
    assert_eq!(MissReason::parse("vendor_special"), None);
}

/// AC-R-2.3.4-14 — the cache path under the second family: `implicit_prefix`
/// semantics resolve expected state, the usage observation projects warm,
/// and `cache_state_visible = false` is a declared member, never inferred.
#[test]
fn second_dialect_cache_path() {
    let d = dialect_b();
    assert_eq!(d.cache_semantics.kind(), "implicit_prefix");
    assert!(!d.cache_state_visible);
    let prior = CacheCallFact {
        affinity_key: "k".into(),
        static_hash: "h".into(),
        request_sent_ms: 0,
        completed: true,
        model_call_id: "mc-0".into(),
    };
    let e = expect_cache_state(
        &[prior],
        "k",
        "h",
        5_000,
        Some(2_000),
        &d.cache_semantics,
        1_000,
        &[],
    );
    assert_eq!(e.expected, ExpectedState::Warm);
    // The family's `details_nested` usage decodes to the same vector →
    // `observed = warm` (`cache_read = 30 > 0`); agreement holds.
    let u = (SP_B.usage)();
    let v = normalize_usage(&d, &u, true, "norm:test").expect("normalized");
    assert_eq!(v.input_total, 140);
    assert_eq!(v.cache_read, 30);
    let obs = observe_cache(Some(&v));
    assert_eq!(obs.observed, ObservedState::Warm);
    assert!(cache_agreement(e.expected, obs.observed));
}
