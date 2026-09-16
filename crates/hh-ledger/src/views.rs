//! The rebuildable projections (§5a.1 §4; ADR-0026 §2): every view is a **pure** fold
//! over the durable prefix, stamped `derived_from = (run_id, seq)` watermark +
//! `view_policy_version` + `view_hash`; any two rebuilds agree byte-for-byte
//! (AC-R-2.2.1-3). At Stage 1 the registered kinds are `context_view` and
//! `run_summary`; the rest of §5a.1 §4's list lands with its owners (`cli_stream_view`
//! §07, `audit` §05g, `resume_set` §05a.3, …).
//!
//! - `context_view` — the ordered context-plane items a model's working set is drawn
//!   from (the Stage-1 form; §05c's slot/budget assembly lands at S1.19): every durable
//!   `observation`-plane event in seq order, each with its authority stamp.
//! - `run_summary` — the run's folded shape: head, event/class/plane histograms, open
//!   scopes, terminal status, first/last `ts`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_identity::idp::idp_id;
use hh_wire::json::Json;

use crate::event::{EventEnvelope, EventPlane};
use crate::schema::VIEW_HASH_DOMAIN;

/// The projection policy version — a bump re-derives every view (`view_hash` changes).
pub const VIEW_POLICY_VERSION: &str = "view/1";

/// The registered view kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    /// `context_view` — ordered context-plane items with authority stamps.
    ContextView,
    /// `run_summary` — the folded run shape.
    RunSummary,
}

impl ViewKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ViewKind::ContextView => "context_view",
            ViewKind::RunSummary => "run_summary",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<ViewKind> {
        match s {
            "context_view" => Some(ViewKind::ContextView),
            "run_summary" => Some(ViewKind::RunSummary),
            _ => None,
        }
    }
}

/// A projected view — `{derived_from, view_policy_version, view_hash, payload}`.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    /// The run the view was derived from.
    pub run_id: String,
    /// The kind.
    pub kind: ViewKind,
    /// The watermark — the greatest durable seq folded (or `-1`-equivalent: `None`
    /// when the run has no committed events).
    pub derived_from_seq: Option<u64>,
    /// The projection policy version.
    pub view_policy_version: &'static str,
    /// `idp/1` digest over `{run_id, watermark, view_policy_version, payload}` —
    /// identical across rebuilds (AC-3).
    pub view_hash: String,
    /// The view body.
    pub payload: Json,
}

impl View {
    fn build(run_id: &str, kind: ViewKind, watermark: Option<u64>, payload: Json) -> View {
        let preimage = Json::Obj(BTreeMap::from([
            ("run_id".to_string(), Json::str(run_id)),
            (
                "watermark".to_string(),
                watermark.map(|s| Json::Int(s as i64)).unwrap_or(Json::Null),
            ),
            (
                "view_policy_version".to_string(),
                Json::str(VIEW_POLICY_VERSION),
            ),
            ("payload".to_string(), payload.clone()),
        ]));
        View {
            run_id: run_id.to_string(),
            kind,
            derived_from_seq: watermark,
            view_policy_version: VIEW_POLICY_VERSION,
            view_hash: idp_id(VIEW_HASH_DOMAIN, preimage.to_canonical_string().as_bytes()),
            payload,
        }
    }
}

/// `project(context_view)`: the durable `observation`-plane events in seq order —
/// `{items: [{seq, event_id, class, authority, readers, content}], item_count}` where
/// `content` is the inline payload and `authority`/`readers` come from the event's
/// provenance label (absent ⇒ `unverified`/public — never read from content, CC2).
pub fn context_view(run_id: &str, events: &[EventEnvelope], until: Option<u64>) -> View {
    let mut items = Vec::new();
    let mut watermark = None;
    for e in events {
        if e.plane != EventPlane::Observation {
            continue;
        }
        if let Some(u) = until {
            if e.seq > u {
                continue;
            }
        }
        let (authority, readers) = match &e.provenance {
            Some(p) => (
                p.authority.as_str().to_string(),
                match &p.readers {
                    hh_provenance::ReaderSet::Public => Json::str("public"),
                    hh_provenance::ReaderSet::Restricted(rs) => {
                        Json::Arr(rs.iter().map(|r| Json::str(r.clone())).collect())
                    }
                },
            ),
            None => ("unverified".to_string(), Json::str("public")),
        };
        items.push(Json::Obj(BTreeMap::from([
            ("seq".to_string(), Json::Int(e.seq as i64)),
            ("event_id".to_string(), Json::str(&e.event_id)),
            ("class".to_string(), Json::str(&e.class)),
            ("authority".to_string(), Json::str(authority)),
            ("readers".to_string(), readers),
            ("content".to_string(), e.payload.clone()),
            (
                "refs".to_string(),
                Json::Arr(
                    e.refs
                        .iter()
                        .map(|a| Json::str(format!("{}:{}", a.algorithm, a.digest)))
                        .collect(),
                ),
            ),
        ])));
        watermark = Some(e.seq);
    }
    let payload = Json::Obj(BTreeMap::from([
        ("kind".to_string(), Json::str("context_view")),
        ("item_count".to_string(), Json::Int(items.len() as i64)),
        ("items".to_string(), Json::Arr(items)),
    ]));
    View::build(run_id, ViewKind::ContextView, watermark, payload)
}

/// `project(run_summary)`: the folded run shape over the durable prefix.
pub fn run_summary(
    manifest_run_kind: &str,
    participant_class: &str,
    observability: &BTreeSet<crate::manifest::ObservabilityLevel>,
    events: &[EventEnvelope],
    open_scopes: &BTreeSet<String>,
    run_id: &str,
    until: Option<u64>,
) -> View {
    let mut classes: BTreeMap<String, u64> = BTreeMap::new();
    let mut planes: BTreeMap<String, u64> = BTreeMap::new();
    let mut watermark = None;
    let mut first_ts = None;
    let mut last_ts = None;
    let mut finished = false;
    let mut head = None;
    for e in events {
        if let Some(u) = until {
            if e.seq > u {
                continue;
            }
        }
        *classes.entry(e.class.clone()).or_insert(0) += 1;
        *planes.entry(e.plane.as_str().to_string()).or_insert(0) += 1;
        if first_ts.is_none() {
            first_ts = Some(e.ts.clone());
        }
        last_ts = Some(e.ts.clone());
        if e.class == "lifecycle.run.finished" {
            finished = true;
        }
        head = Some((e.seq, e.event_id.clone(), e.hash.clone()));
        watermark = Some(e.seq);
    }
    let head_j = match head {
        Some((seq, id, hash)) => Json::Obj(BTreeMap::from([
            ("seq".to_string(), Json::Int(seq as i64)),
            ("event_id".to_string(), Json::str(id)),
            ("hash".to_string(), Json::str(hash)),
        ])),
        None => Json::Null,
    };
    let payload = Json::Obj(BTreeMap::from([
        ("kind".to_string(), Json::str("run_summary")),
        ("run_id".to_string(), Json::str(run_id)),
        ("run_kind".to_string(), Json::str(manifest_run_kind)),
        (
            "participant_class".to_string(),
            Json::str(participant_class),
        ),
        (
            "observability_level".to_string(),
            Json::Arr(
                observability
                    .iter()
                    .map(|l| Json::str(l.as_str()))
                    .collect(),
            ),
        ),
        ("head".to_string(), head_j),
        (
            "event_count".to_string(),
            Json::Int(classes.values().sum::<u64>() as i64),
        ),
        (
            "classes".to_string(),
            Json::Obj(
                classes
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                    .collect(),
            ),
        ),
        (
            "planes".to_string(),
            Json::Obj(
                planes
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                    .collect(),
            ),
        ),
        (
            "open_scopes".to_string(),
            Json::Arr(open_scopes.iter().map(Json::str).collect()),
        ),
        ("finished".to_string(), Json::Bool(finished)),
        (
            "first_ts".to_string(),
            first_ts.map(Json::str).unwrap_or(Json::Null),
        ),
        (
            "last_ts".to_string(),
            last_ts.map(Json::str).unwrap_or(Json::Null),
        ),
    ]));
    View::build(run_id, ViewKind::RunSummary, watermark, payload)
}
