//! `proxy::replay` — the proxy's replay-mode machinery over
//! `hh.model_proxy.recording/1` (experiments spec §4.9; R6.12 / REPLAY;
//! REQ-XC-122, absorbs REQ-XD-108; design/33 F15; design/36 §1.3,
//! §1.6 P1–P9).
//!
//! A recording is the ordered set of raw upstream exchanges of a slot
//! plus the downstream request bytes: per call `k`, the downstream
//! request (capture point 1), the upstream-bound request after the
//! lane's transforms (capture point 2 — what the model conditions on),
//! the upstream response (status, headers, body, per-chunk arrival
//! stamps), the released downstream bytes and the §4.3 disposition.
//! `provenance` ∈ `recorded | synthetic` — a synthetic sequence is the
//! same document authored by hand (P6).
//!
//! The document codec's one home is [`hharness_gateway::recording`]
//! (CC1 — R6.28 moved it down to the boundary crate so the L1 corpus
//! and gate in `hharness-runner` consume the same codec without an
//! edge into hosting under REQ-XB-103); this module re-exports it and
//! keeps the proxy-side replay policy: `ProxyMode`, `ReplayConfig`,
//! `Keying`, `LatencyMode`, [`replay_pick`] and [`rechunk`]. The codec
//! is closed: unknown members refuse, never silently carry.

use std::collections::BTreeMap;

use hharness_gateway::replay::{Canonical, MatchVerdict};

pub use hharness_gateway::recording::{
    body_json, RecordedCall, RecordedReleased, RecordedResponse, Recording, RecordingError,
    RECORDING_SCHEMA,
};

/// The proxy's mode (§4.9; design/33 F15's `mode ∈ {live, record,
/// replay}`).
#[derive(Debug, Clone)]
pub enum ProxyMode {
    /// The R6.10/R6.11 live path — no recording capture.
    Live,
    /// `record` = `live` plus the per-slot capture `seal_recording`
    /// seals into a `hh.model_proxy.recording/1` manifest.
    Record,
    /// `replay` — no upstream port and no budget charge; the recorded
    /// responses serve the slot sequences with per-call match verdicts.
    Replay(ReplayConfig),
}

/// The replay leg's declared configuration (all members recorded on
/// receipts — a replay's evidence reads its own ruleset).
#[derive(Debug, Clone)]
pub struct ReplayConfig {
    /// Recordings keyed by slot id — bound at `configure`; a call on a
    /// slot with no recording refuses `replay_no_recording`.
    pub recordings: BTreeMap<String, Recording>,
    /// The `RequestCanonicalizer` spelling (`identity` |
    /// `json_canonical` | `hh-l1-canon`).
    pub canonicalizer_id: String,
    /// C04's recorded cell facts (`hh-l1-canon` only).
    pub cell_facts: Vec<(String, String)>,
    /// Strict stops on `diverged` (typed 409 `replay_diverged`, ends
    /// the slot); lenient serves anyway and records the verdict —
    /// never gate evidence (design/36 §1.3's `on_divergence ∈
    /// {stop, serve_recorded}`).
    pub strict: bool,
    /// `sequence` serves the k-th recorded call to the k-th request;
    /// `sequence_then_hash` first scans ahead for a canonical-hash
    /// match (the interleaved-connection repair — design/33 F15).
    pub keying: Keying,
    /// `immediate` serves at once (default); `recorded` sleeps the
    /// recorded per-call latency first (P9 — timing-sensitive legs).
    pub latency: LatencyMode,
}

/// The declared keying (design/33 F15; `sequence` is the honest default
/// — serial handling orders a slot's requests exactly).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keying {
    /// Position k → recorded call k.
    Sequence,
    /// Scan ahead from the position for a canonical-hash match, else
    /// fall back to position (interleaved-connection repair).
    SequenceThenHash,
}

/// The declared latency policy (P9; `immediate` is the default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatencyMode {
    /// Serve as soon as the receipt is durable.
    Immediate,
    /// Sleep the recorded per-call latency first.
    Recorded,
}

/// The replay leg's outcome for one request — either the recorded call
/// to serve (with its match verdict) or a typed refusal.
pub enum ReplayPick {
    /// Serve `calls[index]` — the match verdict rides the receipt.
    Serve {
        /// The recording index chosen (k under `sequence`; the matched
        /// index under `sequence_then_hash`).
        index: usize,
        /// The per-call match verdict.
        verdict: MatchVerdict,
        /// The canonical view of the incoming upstream-bound request
        /// (the receipt's `dropped_paths` carrier).
        canonical: Option<Canonical>,
    },
    /// The recorded sequence is exhausted — the typed
    /// end-of-sequence refusal (P5).
    Exhausted,
    /// The slot carries no recording.
    NoRecording,
}

/// `replay_pick(recording, used, keying, canon, surface,
/// upstream_body)` — the match + selection leg (P3/P4): the verdict
/// compares the recorded upstream-bound request (capture point 2 — or
/// the recorded downstream body when the recorded call refused before
/// dispatch) against the replayed request's canonical view.
/// `sequence` serves the smallest unserved index;
/// `sequence_then_hash` first scans the outstanding recorded calls
/// for a canonical-hash match (the interleaved-connection repair —
/// design/33 F15). A canonicalizer refusal on either side degrades to
/// `diverged{canon_error}`, never a silent pass.
pub fn replay_pick(
    recording: &Recording,
    used: &std::collections::BTreeSet<usize>,
    keying: Keying,
    canon: &dyn hharness_gateway::replay::RequestCanonicalizer,
    surface: &str,
    upstream_body: &[u8],
) -> Result<ReplayPick, hharness_gateway::replay::CanonRefusal> {
    let len = recording.calls.len();
    let expected_at = |i: usize| -> &[u8] {
        recording.calls[i]
            .upstream_request
            .as_deref()
            .unwrap_or(&recording.calls[i].downstream_body)
    };
    let Some(pos) = (0..len).find(|i| !used.contains(i)) else {
        return Ok(ReplayPick::Exhausted);
    };
    let index = match keying {
        Keying::Sequence => pos,
        Keying::SequenceThenHash => {
            let want = canon.canonicalize(surface, upstream_body).ok();
            let mut found = None;
            if let Some(want) = want {
                for i in pos..len {
                    if used.contains(&i) {
                        continue;
                    }
                    if canon
                        .canonicalize(surface, expected_at(i))
                        .map(|c| c.bytes == want.bytes)
                        .unwrap_or(false)
                    {
                        found = Some(i);
                        break;
                    }
                }
            }
            found.unwrap_or(pos)
        }
    };
    let expected = expected_at(index);
    let verdict = hharness_gateway::replay::match_requests(canon, surface, expected, upstream_body)
        .unwrap_or_else(|r| MatchVerdict::Diverged {
            first_diff_path: format!("canon_error:{}", r.detail),
        });
    let canonical = canon.canonicalize(surface, upstream_body).ok();
    Ok(ReplayPick::Serve {
        index,
        verdict,
        canonical,
    })
}

/// `rechunk(stamps, body)` — P7's declared deterministic re-chunking:
/// a streaming release writes one chunk per recorded `chunk_stamps`
/// length (recorded segmentation preserved); a recording without
/// stamps releases the whole body as one chunk.
pub fn rechunk(stamps: &[(u64, usize)], body: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut off = 0usize;
    for (_, len) in stamps {
        let end = (off + len).min(body.len());
        if end > off {
            out.push(body[off..end].to_vec());
        }
        off = end;
    }
    if off < body.len() {
        out.push(body[off..].to_vec());
    }
    if out.is_empty() && !body.is_empty() {
        out.push(body.to_vec());
    }
    out
}
