//! `compact` — the participant-native compaction spellings → the canonical
//! `compaction.observed` [`HostedEvent`] (spec §5c.2's hosted-lift row;
//! AC-R-2.4.2-11; ticket S4.16b).
//!
//! Two hosted spellings are in scope; both denote the same participant-side
//! fact — *the transcript was compacted behind a summary boundary* — so the
//! normalizer is a pure member mapping, never a synthesis:
//!
//! - **Inspect-style `CompactionEvent`** — the transcript event Inspect
//!   emits when a `compact` edit/summarise reduced the message history
//!   (`{type: "CompactionEvent" | "compaction", summary?, ops?|operations?,
//!   tokens_before?, tokens_after?, …}`).
//! - **Claude-Code-style `compact_boundary`** — the session boundary marker
//!   (`{type: "compact_boundary", compact_metadata|compactMetadata{trigger,
//!   pre_tokens|preTokens}}`) — summarisation is *the* operation the
//!   boundary records.
//!
//! The normalized payload (the `compaction.observed` shape the lift reads):
//!
//! ```text
//! {phase: "completed",
//!  applied_ops: [{kind}],               // reported ops, else [{kind:"summarize"}]
//!  applied_ops_source: "reported" | "format",
//!  tokens_before?, tokens_after?,       // integer members, only when reported
//!  trigger?,                            // compact_boundary's trigger, verbatim
//!  components: {                        // the component metrics the host
//!    input_reduction: {"n/a": "hosted_opaque"},   // cannot observe — typed
//!    summariser_usage: {"n/a": "hosted_opaque"},  // n/a, never fabricated
//!  }}
//! ```
//!
//! Envelope: `provenance{origin = participant, authority = unverified}` —
//! the row is the participant's own report, never a kernel fact; the raw
//! record rides `ext["_raw"]` (CC3 — nothing unreported is synthesized and
//! nothing reported is dropped).

use hh_provenance::AuthorityClass;
use hh_wire::Json;

use crate::events::{
    EventChannel, HostedError, HostedEvent, HostedOrigin, HostedProvenance, Mediation,
};

/// The participant-native compaction spelling — the closed set this
/// normalizer accepts (anything else is the caller's `native_record` leaf,
/// never a guess).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionSpelling {
    /// Inspect's transcript `CompactionEvent`.
    InspectCompactionEvent,
    /// Claude Code's `compact_boundary` session marker.
    ClaudeCompactBoundary,
}

impl CompactionSpelling {
    /// Parse the participant's own record/type spelling.
    pub fn parse(s: &str) -> Option<CompactionSpelling> {
        match s {
            "CompactionEvent" | "compaction" | "compaction_event" => {
                Some(CompactionSpelling::InspectCompactionEvent)
            }
            "compact_boundary" | "compactBoundary" => {
                Some(CompactionSpelling::ClaudeCompactBoundary)
            }
            _ => None,
        }
    }

    /// The canonical spelling (recorded on `applied_ops_source` provenance).
    pub fn as_str(self) -> &'static str {
        match self {
            CompactionSpelling::InspectCompactionEvent => "CompactionEvent",
            CompactionSpelling::ClaudeCompactBoundary => "compact_boundary",
        }
    }
}

/// The `n/a{class}` member — the same shape `hh-analysis` stamps
/// (`{"n/a": "<class>"}` — a typed absence, never a zero or an estimate).
pub fn na(class: &str) -> Json {
    Json::obj([("n/a", Json::str(class))])
}

/// The component-metrics block a hosted compaction cannot report — the
/// §5c.2 executor's `input_reduction`/`summariser_usage` live inside the
/// host; the lift stamps `n/a{hosted_opaque}` (AC-R-2.4.2-11).
pub fn opaque_components() -> Json {
    Json::obj([
        ("input_reduction", na("hosted_opaque")),
        ("summariser_usage", na("hosted_opaque")),
    ])
}

/// The idempotent payload completion the lift applies to every
/// `compaction.observed` row — absent members fill to the honest defaults
/// (`applied_ops = []` when nothing was reported, `components` to the
/// typed `n/a` block); reported members pass through verbatim.
pub fn complete_payload(payload: &Json) -> Json {
    let Json::Obj(m) = payload else {
        return payload.clone();
    };
    let mut m = m.clone();
    m.entry("phase".into())
        .or_insert_with(|| Json::str("completed"));
    if !m.contains_key("applied_ops") {
        m.insert("applied_ops".into(), Json::Arr(vec![]));
        m.entry("applied_ops_source".into())
            .or_insert_with(|| Json::str("unreported"));
    }
    m.entry("components".into())
        .or_insert_with(opaque_components);
    Json::Obj(m)
}

/// `int_member` — an integer payload member (the `tokens_*` contract is
/// integer-only — a float/string is unreported, never coerced).
fn int_member<'a>(raw: &'a Json, keys: &[&str]) -> Option<&'a Json> {
    keys.iter()
        .find_map(|k| raw.get(k))
        .and_then(|v| v.as_int().map(|_| v))
}

/// `int_member` over a nested object path (`compact_metadata.pre_tokens`
/// / `compactMetadata.preTokens`).
fn nested_int<'a>(raw: &'a Json, obj_keys: &[&str], leaf_keys: &[&str]) -> Option<&'a Json> {
    obj_keys
        .iter()
        .find_map(|k| raw.get(k))
        .and_then(|o| int_member(o, leaf_keys))
}

/// One reported op → its `{kind}` record. A bare string op spells
/// `{kind: s}`; an object op's members pass through verbatim (its `kind`
/// is required — an op without a kind is a `PayloadShape` refusal, never
/// a guessed kind).
fn op_kind(v: &Json, spelling: CompactionSpelling) -> Result<Json, HostedError> {
    match v {
        Json::Str(s) => Ok(Json::obj([("kind", Json::str(s.clone()))])),
        Json::Obj(_) if v.get("kind").and_then(Json::as_str).is_some() => Ok(v.clone()),
        _ => Err(HostedError::PayloadShape {
            kind: format!("{}:applied_ops", spelling.as_str()),
            detail: "each op must be a string kind or an object carrying `kind`".into(),
        }),
    }
}

/// `normalize_compaction(spelling, raw, env)` — one participant compaction
/// record → the canonical `compaction.observed` hosted event
/// (AC-R-2.4.2-11).
#[allow(clippy::too_many_arguments)] // the members are the hosted record's shape.
pub fn normalize_compaction(
    spelling: CompactionSpelling,
    raw: &Json,
    seq: u64,
    session: &str,
    at: u64,
    mediation: Mediation,
    channel: EventChannel,
    raw_ref: Option<String>,
) -> Result<HostedEvent, HostedError> {
    let Json::Obj(_) = raw else {
        return Err(HostedError::PayloadShape {
            kind: spelling.as_str().into(),
            detail: "the compaction record is an object".into(),
        });
    };
    let mut m = std::collections::BTreeMap::new();
    m.insert("phase".into(), Json::str("completed"));
    m.insert("spelling".into(), Json::str(spelling.as_str()));
    // applied_ops — reported ops verbatim; absent → the format default:
    // both spellings *are* summary-boundary records, so the op set is
    // `[{kind: summarize}]` with `applied_ops_source = "format"` (honest —
    // the inference rides in its own member, never disguised as reported).
    let ops_key = ["applied_ops", "ops", "operations", "ops_applied"]
        .iter()
        .find(|k| raw.get(k).is_some());
    match ops_key {
        Some(k) => {
            let Json::Arr(ops) = raw.get(k).unwrap() else {
                return Err(HostedError::PayloadShape {
                    kind: format!("{}:{k}", spelling.as_str()),
                    detail: "ops must be an array".into(),
                });
            };
            let ops = ops
                .iter()
                .map(|v| op_kind(v, spelling))
                .collect::<Result<Vec<_>, _>>()?;
            m.insert("applied_ops".into(), Json::Arr(ops));
            m.insert("applied_ops_source".into(), Json::str("reported"));
        }
        None => {
            m.insert(
                "applied_ops".into(),
                Json::Arr(vec![Json::obj([("kind", Json::str("summarize"))])]),
            );
            m.insert("applied_ops_source".into(), Json::str("format"));
        }
    }
    // tokens_before/after — the three spellings each format uses; absent
    // stays absent (never a fabricated count).
    let before = int_member(
        raw,
        &["tokens_before", "pre_tokens", "preTokens", "tokensBefore"],
    )
    .or_else(|| {
        nested_int(
            raw,
            &["compact_metadata", "compactMetadata"],
            &["pre_tokens", "preTokens"],
        )
    });
    if let Some(t) = before {
        m.insert("tokens_before".into(), t.clone());
    }
    let after = int_member(
        raw,
        &["tokens_after", "post_tokens", "postTokens", "tokensAfter"],
    );
    if let Some(t) = after {
        m.insert("tokens_after".into(), t.clone());
    }
    // trigger — the compact_boundary member, verbatim when present.
    let trigger = raw.get("trigger").and_then(Json::as_str).or_else(|| {
        ["compact_metadata", "compactMetadata"]
            .iter()
            .find_map(|k| raw.get(k))
            .and_then(|o| o.get("trigger"))
            .and_then(Json::as_str)
    });
    if let Some(t) = trigger {
        m.insert("trigger".into(), Json::str(t));
    }
    // Component metrics — the host cannot observe them; typed n/a.
    m.insert("components".into(), opaque_components());

    let mut ext = std::collections::BTreeMap::new();
    ext.insert("_raw".into(), raw.clone());
    Ok(HostedEvent {
        seq,
        session: session.to_string(),
        at,
        kind: "compaction.observed".to_string(),
        payload: Json::Obj(m),
        provenance: HostedProvenance {
            origin: HostedOrigin::Participant,
            authority: AuthorityClass::Unverified,
        },
        mediation,
        event_channel: channel,
        raw_ref,
        ext,
    })
}
