//! `condenser` — the deterministic `transcript.jsonl` writer
//! (experiments spec §4.10 step 1; R6.12 / ARCHIVE (a); REQ-XF-124;
//! design/39 F11: "one transcript.jsonl per trial … each message is
//! stored once rather than resent per call; each step carries call
//! seq, tokens, latency, queue_wait_ms, the tool command, exit code
//! and output excerpt, and the disposition").
//!
//! The input is the trial's `hh.model_io/1` receipts in call order —
//! the proxy receipt is the transcript of record (C4 F10). The output
//! is one JSONL document: a `trial` header, `messages` lines carrying
//! only the not-yet-emitted tail of each request's `messages[]`
//! (positional dedup — the resent prefix is never rewritten), and one
//! `call` line per receipt.
//!
//! The tombstone hook (X-05): every emitted line passes through
//! `hharness-secrets`' `redact` with the caller's `DetectorSet` —
//! the R6.23 scan feeds it the revoked slot tokens' mask set, so a
//! token that survives in raw receipts is tombstoned in every derived
//! file. The scan itself is R6.23's; this is the hook it uses.

use hharness_secrets::redact::{redact, DetectorSet, Hit};
use hharness_wire::json::Json;

fn as_arr(j: &Json) -> Option<&Vec<Json>> {
    match j {
        Json::Arr(a) => Some(a),
        _ => None,
    }
}

/// The condenser's output.
#[derive(Debug)]
pub struct Condensed {
    /// The `transcript.jsonl` bytes (trailing newline).
    pub bytes: Vec<u8>,
    /// The tombstone hits `redact` applied across all emitted lines
    /// (the R6.23 scan's per-file record).
    pub hits: Vec<Hit>,
}

fn pget<'a>(receipt: &'a Json, path: &[&str]) -> Option<&'a Json> {
    let mut cur = receipt;
    for k in path {
        cur = cur.get(k)?;
    }
    Some(cur)
}

fn jint(receipt: &Json, path: &[&str]) -> Json {
    pget(receipt, path).cloned().unwrap_or(Json::Null)
}

/// `condense(trial_id, receipts, observations, tombstones)` — one
/// deterministic `transcript.jsonl`. `observations`, when supplied,
/// zips by position: each entry `{command, exit_code, output_excerpt}`
/// merges onto the call line (the native trajectory's tool step; the
/// receipts alone carry no tool commands).
pub fn condense(
    trial_id: &str,
    receipts: &[Json],
    observations: Option<&[Json]>,
    tombstones: &DetectorSet,
) -> Condensed {
    let mut lines: Vec<String> = Vec::new();
    let mut hits: Vec<Hit> = Vec::new();
    let emit = |doc: Json, lines: &mut Vec<String>, hits: &mut Vec<Hit>| {
        let (text, h) = redact(&doc.to_canonical_string(), tombstones);
        hits.extend(h);
        lines.push(text);
    };
    emit(
        Json::obj([
            ("type", Json::str("trial")),
            ("schema", Json::str("hh.condensed_transcript/1")),
            ("trial_id", Json::str(trial_id)),
            ("calls", Json::Int(receipts.len() as i64)),
        ]),
        &mut lines,
        &mut hits,
    );
    let mut emitted = 0usize;
    for (k, r) in receipts.iter().enumerate() {
        // Positional dedup — a request's `messages[]` resends the whole
        // prefix; only the tail lands (each message stored once).
        if let Some(msgs) = pget(r, &["request_body", "messages"]).and_then(as_arr) {
            if msgs.len() > emitted {
                emit(
                    Json::obj([
                        ("type", Json::str("messages")),
                        ("k", Json::Int(k as i64)),
                        ("messages", Json::Arr(msgs[emitted..].to_vec())),
                    ]),
                    &mut lines,
                    &mut hits,
                );
                emitted = msgs.len();
            }
        }
        let mut tool = Json::Null;
        if let Some(obs) = observations.and_then(|o| o.get(k)) {
            tool = obs.clone();
        }
        emit(
            Json::obj([
                ("type", Json::str("call")),
                ("k", Json::Int(k as i64)),
                ("model_call_id", jint(r, &["model_call_id"])),
                ("disposition", jint(r, &["outcome", "disposition"])),
                (
                    "usage",
                    Json::obj([
                        (
                            "input_total",
                            jint(r, &["proxy", "usage", "view", "tokens.input.total"]),
                        ),
                        (
                            "output_total",
                            jint(r, &["proxy", "usage", "view", "tokens.output.total"]),
                        ),
                        (
                            "cache_read",
                            jint(r, &["proxy", "usage", "view", "cache_read"]),
                        ),
                    ]),
                ),
                ("latency_ms", jint(r, &["proxy", "timing", "latency_ms"])),
                (
                    "queue_wait_ms",
                    jint(r, &["proxy", "timing", "queue_wait_ms"]),
                ),
                ("released_status", jint(r, &["proxy", "released", "status"])),
                ("tool", tool),
            ]),
            &mut lines,
            &mut hits,
        );
    }
    let mut bytes = lines.join("\n").into_bytes();
    bytes.push(b'\n');
    Condensed { bytes, hits }
}
