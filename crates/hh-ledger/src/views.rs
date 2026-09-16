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
    /// `checkpoint` — the `R-2.2.3⁰ᵃ` restore input: open scopes, per-effect
    /// phase/risk, live retry timers, open model calls and pending permissions —
    /// a *view* (a pure fold), never a second store (ADR-0130 §1).
    Checkpoint,
    /// `effect_ledger` — the per-effect fold in full (§5a.2's effect ledger).
    EffectLedger,
}

impl ViewKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ViewKind::ContextView => "context_view",
            ViewKind::RunSummary => "run_summary",
            ViewKind::Checkpoint => "checkpoint",
            ViewKind::EffectLedger => "effect_ledger",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<ViewKind> {
        match s {
            "context_view" => Some(ViewKind::ContextView),
            "run_summary" => Some(ViewKind::RunSummary),
            "checkpoint" => Some(ViewKind::Checkpoint),
            "effect_ledger" => Some(ViewKind::EffectLedger),
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

// ─────────────────────────────────────────────────────────────────────────────
// R-2.2.3⁰ᵃ — checkpoint + effect ledger
// ─────────────────────────────────────────────────────────────────────────────

/// Fold the live `control.retry.*` rows into timers: `scheduled` opens a timer,
/// `fired`/`skipped` consume it by `schedule_event_id` (the durable timer — process
/// memory is never the only copy, ADR-0130 §5).
pub(crate) fn fold_retry_timers(events: &[EventEnvelope], until: Option<u64>) -> Vec<Json> {
    let mut open: BTreeMap<String, Json> = BTreeMap::new();
    for e in events {
        if let Some(u) = until {
            if e.seq > u {
                continue;
            }
        }
        match e.class.as_str() {
            "control.retry.scheduled" => {
                open.insert(
                    e.event_id.clone(),
                    Json::Obj(BTreeMap::from([
                        ("schedule_event_id".to_string(), Json::str(&e.event_id)),
                        (
                            "scope_id".to_string(),
                            e.payload.get("scope_id").cloned().unwrap_or(Json::Null),
                        ),
                        (
                            "kind".to_string(),
                            e.payload.get("kind").cloned().unwrap_or(Json::Null),
                        ),
                        (
                            "attempt_no".to_string(),
                            e.payload.get("attempt_no").cloned().unwrap_or(Json::Null),
                        ),
                        (
                            "not_before".to_string(),
                            e.payload.get("not_before").cloned().unwrap_or(Json::Null),
                        ),
                        (
                            "reason".to_string(),
                            e.payload.get("reason").cloned().unwrap_or(Json::Null),
                        ),
                    ])),
                );
            }
            "control.retry.fired" | "control.retry.skipped" => {
                if let Some(id) = e.payload.get("schedule_event_id").and_then(Json::as_str) {
                    open.remove(id);
                }
            }
            _ => {}
        }
    }
    open.into_values().collect()
}

/// One `EffectFold` rendered to JSON (the `checkpoint`/`effect_ledger` row).
fn effect_fold_json(f: &crate::effect::EffectFold) -> Json {
    Json::Obj(BTreeMap::from([
        ("effect_id".to_string(), Json::str(&f.effect_id)),
        ("phase".to_string(), Json::str(f.phase.as_str())),
        (
            "outcome".to_string(),
            f.outcome
                .map(|o| Json::str(o.as_str()))
                .unwrap_or(Json::Null),
        ),
        ("risk_class".to_string(), f.risk_class.to_json()),
        (
            "declared_risk_class".to_string(),
            f.declared_risk_class
                .map(|r| r.to_json())
                .unwrap_or(Json::Null),
        ),
        ("attempt_no".to_string(), Json::Int(f.attempt_no as i64)),
        ("probe_count".to_string(), Json::Int(f.probe_count as i64)),
        ("terminal".to_string(), Json::Bool(f.is_terminal())),
        (
            "idempotency_key".to_string(),
            f.idempotency_key
                .as_deref()
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
    ]))
}

/// The `action.effect.*` fold over the durable prefix — shared by both views.
fn fold_effects(
    events: &[EventEnvelope],
    until: Option<u64>,
) -> BTreeMap<String, crate::effect::EffectFold> {
    let mut effects = BTreeMap::new();
    for e in events {
        if let Some(u) = until {
            if e.seq > u {
                continue;
            }
        }
        crate::effect::fold_event(&mut effects, e);
    }
    effects
}

/// `project(effect_ledger)` — every effect's fold, in `effect_id` order.
pub fn effect_ledger(run_id: &str, events: &[EventEnvelope], until: Option<u64>) -> View {
    let effects = fold_effects(events, until);
    let watermark = events
        .iter()
        .rfind(|e| until.map(|u| e.seq <= u).unwrap_or(true))
        .map(|e| e.seq);
    let payload = Json::Obj(BTreeMap::from([
        ("kind".to_string(), Json::str("effect_ledger")),
        ("effect_count".to_string(), Json::Int(effects.len() as i64)),
        (
            "effects".to_string(),
            Json::Arr(effects.values().map(effect_fold_json).collect()),
        ),
    ]));
    View::build(run_id, ViewKind::EffectLedger, watermark, payload)
}

/// `project(checkpoint)` — the deterministic restore input (AC-R-2.2.3-2): the
/// open scopes, per-effect phase/risk/attempt, live retry timers, open model
/// calls and unanswered permission requests — every field a pure fold of the
/// durable prefix, so a rebuilt `Store` and a live one agree byte-for-byte.
pub fn checkpoint(run_id: &str, events: &[EventEnvelope], until: Option<u64>) -> View {
    let mut open_scopes: BTreeMap<String, &'static str> = BTreeMap::new();
    let mut effects = BTreeMap::new();
    let mut head = None;
    let mut watermark = None;
    let mut requested: BTreeMap<String, u64> = BTreeMap::new();
    let mut decided: BTreeSet<String> = BTreeSet::new();
    for e in events {
        if let Some(u) = until {
            if e.seq > u {
                continue;
            }
        }
        if let Some(spec) = crate::classes::lookup(&e.class) {
            if let Some(kind) = spec.opens_scope {
                if let Some(id) = e.scope.get(kind) {
                    open_scopes.insert(id.to_string(), kind.field());
                }
            }
            if let Some(kind) = spec.closes_scope {
                if let Some(id) = e.scope.get(kind) {
                    let fold = if kind == crate::classes::ScopeKind::Effect {
                        effects.get(id)
                    } else {
                        None
                    };
                    if crate::effect::scope_close_fires(&e.class, &e.payload, fold) {
                        open_scopes.remove(id);
                    }
                }
            }
        }
        crate::effect::fold_event(&mut effects, e);
        match e.class.as_str() {
            "security.permission.requested" => {
                if let Some(id) = e.payload.get("permission_id").and_then(Json::as_str) {
                    requested.insert(id.to_string(), e.seq);
                }
            }
            "security.permission.decided" => {
                if let Some(id) = e.payload.get("permission_id").and_then(Json::as_str) {
                    decided.insert(id.to_string());
                }
            }
            _ => {}
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
    let pending_permissions: Vec<Json> = requested
        .keys()
        .filter(|id| !decided.contains(*id))
        .map(|id| {
            Json::Obj(BTreeMap::from([
                ("permission_id".to_string(), Json::str(id.clone())),
                ("requested_seq".to_string(), Json::Int(requested[id] as i64)),
            ]))
        })
        .collect();
    let open_model_calls: Vec<Json> = open_scopes
        .iter()
        .filter(|(_, k)| **k == "model_call_id")
        .map(|(id, _)| Json::str(id.clone()))
        .collect();
    let payload = Json::Obj(BTreeMap::from([
        ("kind".to_string(), Json::str("checkpoint")),
        ("head".to_string(), head_j),
        (
            "open_scopes".to_string(),
            Json::Arr(
                open_scopes
                    .iter()
                    .map(|(id, k)| {
                        Json::Obj(BTreeMap::from([
                            ("id".to_string(), Json::str(id.clone())),
                            ("kind".to_string(), Json::str(*k)),
                        ]))
                    })
                    .collect(),
            ),
        ),
        (
            "effects".to_string(),
            Json::Arr(effects.values().map(effect_fold_json).collect()),
        ),
        (
            "retry_timers".to_string(),
            Json::Arr(fold_retry_timers(events, until)),
        ),
        ("open_model_calls".to_string(), Json::Arr(open_model_calls)),
        (
            "pending_permissions".to_string(),
            Json::Arr(pending_permissions),
        ),
    ]));
    View::build(run_id, ViewKind::Checkpoint, watermark, payload)
}
