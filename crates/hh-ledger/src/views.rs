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
    /// `trace_view` — the measurement plane's span projection (§5h.1 §2.1;
    /// ADR-0042 D1/D2). Folded by `hh-telemetry` (the owner).
    TraceView,
    /// `cost_view` — the per-scope/per-attribution spend projection (§5h.1 §2.1;
    /// ADR-0043 D4/D6). Folded by `hh-telemetry`.
    CostView,
    /// `metric_view` — declared metrics folded to `MetricValue`s (§5h.1 §2.1;
    /// ADR-0044 D5). Folded by `hh-telemetry`.
    MetricView,
    /// `audit_view` — the §5g.6 audit projection (Rule-O obligations,
    /// producer/partition rechecks, content-ref accounting, the completeness
    /// vector). Folded by `hh-ledger` itself — the audit trail *is* the ledger
    /// (ADR-0066 D1: no second store).
    AuditView,
    /// `lexical_index` — the `R-2.4.3⁰` lexical index over the `MemoryStore`
    /// (§5c.3; S1.19). Folded by `hh-context`; `lexical` queries serve from it
    /// (`IndexUnavailable` until materialized).
    LexicalIndex,
    /// `memory_stale_index` — `MemoryStaleIndex[version_id] →
    /// [stale_member_ids]` (§5c.4; S1.19). Folded by `hh-context`.
    MemoryStaleIndex,
    /// `memory_usage` — `{version_id → {created_at, last_read_at,
    /// read_count}}` (§5c.3; S1.19). Folded by `hh-context`.
    MemoryUsage,
    /// `branch_tree` — the derived branch index (§5a.1 §5; R-2.2.4; ADR-0271):
    /// this run's fork record (when it is a child), its `rolled_back` rewind
    /// history, and its children. A pure WAL fold — rebuildable, never stored.
    BranchTree,
    /// `effects_by_key` — the §5a.2 C0 dedup projection (`idempotency_key →
    /// {effect_id, phase, outcome}`; R-2.2.4⁰ᵇ; ADR-0102 §10): the commit path's
    /// durable-side consult — a repeated commit under a key that reached
    /// `observed(applied)` serves the stored observation; a duplicate committed
    /// key is a veto, never a counter (ADR-0047 §5).
    EffectsByKey,
    /// `compact` — the **declared-lossy** CLI projection (§7.1; R-2.5.3¹;
    /// ADR-0169 D3): the durable prefix folded to `message`/`tool`/`effect`/
    /// `approval`/`cost` items only, shipped with a `loss_report` enumerating
    /// every dropped class. Optional and never the machine default — the
    /// identity CLI projection is `cli_stream_view` (§5a.1 §4).
    Compact,
}

impl ViewKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ViewKind::ContextView => "context_view",
            ViewKind::RunSummary => "run_summary",
            ViewKind::Checkpoint => "checkpoint",
            ViewKind::EffectLedger => "effect_ledger",
            ViewKind::TraceView => "trace_view",
            ViewKind::CostView => "cost_view",
            ViewKind::MetricView => "metric_view",
            ViewKind::AuditView => "audit_view",
            ViewKind::LexicalIndex => "lexical_index",
            ViewKind::MemoryStaleIndex => "memory_stale_index",
            ViewKind::MemoryUsage => "memory_usage",
            ViewKind::BranchTree => "branch_tree",
            ViewKind::EffectsByKey => "effects_by_key",
            ViewKind::Compact => "compact",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<ViewKind> {
        match s {
            "context_view" => Some(ViewKind::ContextView),
            "run_summary" => Some(ViewKind::RunSummary),
            "checkpoint" => Some(ViewKind::Checkpoint),
            "effect_ledger" => Some(ViewKind::EffectLedger),
            "trace_view" => Some(ViewKind::TraceView),
            "cost_view" => Some(ViewKind::CostView),
            "metric_view" => Some(ViewKind::MetricView),
            "audit_view" => Some(ViewKind::AuditView),
            "lexical_index" => Some(ViewKind::LexicalIndex),
            "memory_stale_index" => Some(ViewKind::MemoryStaleIndex),
            "memory_usage" => Some(ViewKind::MemoryUsage),
            "branch_tree" => Some(ViewKind::BranchTree),
            "effects_by_key" => Some(ViewKind::EffectsByKey),
            "compact" => Some(ViewKind::Compact),
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

    /// Stamp a view folded by an owning crate (`hh-telemetry`'s
    /// `trace_view`/`cost_view`/`metric_view`) — the identical `derived_from`
    /// watermark + `view_policy_version` + `idp/1` `view_hash` discipline, so a
    /// projection is a `View` no matter which crate folds it (CC7 — one view
    /// shape, one hashing domain).
    pub fn stamped(run_id: &str, kind: ViewKind, watermark: Option<u64>, payload: Json) -> View {
        View::build(run_id, kind, watermark, payload)
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

/// `project(effects_by_key)` — the §5a.2 dedup projection
/// (`idempotency_key → {effect_id, phase, outcome}`; R-2.2.4⁰ᵇ; ADR-0102
/// §10): the commit path's durable-side consult — a repeated commit under
/// a key that reached `observed(applied)` serves the stored observation;
/// a duplicate committed key is a veto, never a counter (ADR-0047 §5).
pub fn effects_by_key_view(run_id: &str, events: &[EventEnvelope], until: Option<u64>) -> View {
    let effects = fold_effects(events, until);
    let watermark = events
        .iter()
        .rfind(|e| until.map(|u| e.seq <= u).unwrap_or(true))
        .map(|e| e.seq);
    let mut by_key = BTreeMap::new();
    let mut duplicate_committed: Vec<String> = Vec::new();
    let mut committed_keys: BTreeMap<String, u64> = BTreeMap::new();
    for (effect_id, f) in &effects {
        if let Some(key) = &f.idempotency_key {
            by_key.insert(
                key.clone(),
                Json::obj([
                    ("effect_id", Json::str(effect_id)),
                    ("phase", Json::str(f.phase.as_str())),
                    (
                        "outcome",
                        f.outcome
                            .map(|o| Json::str(o.as_str()))
                            .unwrap_or(Json::Null),
                    ),
                ]),
            );
            // The veto surface — a second durable `committed` under one key
            // (per `commits[]`, the write-ahead records) is the duplicate the
            // battery asserts never appears (AC-R-2.2.3-3).
            for (_ev, seq) in f.commits.values() {
                let n = committed_keys.entry(key.clone()).or_insert(0);
                *n += 1;
                if *n > 1 {
                    duplicate_committed.push(format!("{key}@{seq}"));
                }
            }
        }
    }
    let payload = Json::Obj(BTreeMap::from([
        ("kind".to_string(), Json::str("effects_by_key")),
        ("key_count".to_string(), Json::Int(by_key.len() as i64)),
        ("effects".to_string(), Json::Obj(by_key)),
        (
            "duplicate_committed".to_string(),
            Json::Arr(
                duplicate_committed
                    .iter()
                    .map(|k| Json::str(k.clone()))
                    .collect(),
            ),
        ),
    ]));
    View::build(run_id, ViewKind::EffectsByKey, watermark, payload)
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
    // The durable owed-decision source is `security.permission.pending`
    // (S1.23 — `requested` is the ephemeral prompt rendering; the view's
    // `pending_permissions` projects the durable rows).
    // `permission_id → (opened_seq, attached effect_ids)` — coalesced pendings
    // merge at the fold (the identical-request rule).
    let mut requested: BTreeMap<String, (u64, BTreeSet<String>)> = BTreeMap::new();
    let mut decided: BTreeSet<String> = BTreeSet::new();
    let mut terminated_effects: BTreeSet<String> = BTreeSet::new();
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
            "security.permission.pending" => {
                if let Some(id) = e.payload.get("permission_id").and_then(Json::as_str) {
                    let row = requested
                        .entry(id.to_string())
                        .or_insert((e.seq, BTreeSet::new()));
                    if let Some(eid) = e
                        .scope
                        .effect_id
                        .as_deref()
                        .or_else(|| e.payload.get("effect_id").and_then(Json::as_str))
                    {
                        row.1.insert(eid.to_string());
                    }
                    if let Some(Json::Arr(ids)) = e.payload.get("effect_ids") {
                        row.1
                            .extend(ids.iter().filter_map(Json::as_str).map(str::to_string));
                    }
                }
            }
            "security.permission.decided" => {
                // Only a *final* verdict resolves a pending — a
                // `decision = ask` row is the verdict that opened it.
                let final_verdict = e
                    .payload
                    .get("decision")
                    .and_then(Json::as_str)
                    .is_some_and(|d| d == "allow" || d == "deny");
                if final_verdict {
                    if let Some(id) = e.payload.get("permission_id").and_then(Json::as_str) {
                        decided.insert(id.to_string());
                    }
                }
            }
            "action.effect.refused" | "action.effect.unknown" => {
                // A refused/unknowned effect resolves its pending `cancelled`
                // (§5g.7 §5 — never `unknown`).
                if let Some(eid) = e
                    .scope
                    .effect_id
                    .as_deref()
                    .or_else(|| e.payload.get("effect_id").and_then(Json::as_str))
                {
                    terminated_effects.insert(eid.to_string());
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
        .iter()
        .filter(|(id, (_, effects))| {
            !decided.contains(*id)
                && (effects.is_empty() || !effects.iter().all(|e| terminated_effects.contains(e)))
        })
        .map(|(id, (seq, _))| {
            Json::Obj(BTreeMap::from([
                ("permission_id".to_string(), Json::str(id.clone())),
                ("requested_seq".to_string(), Json::Int(*seq as i64)),
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

/// `branch_tree` — the per-run slice of the derived branch index (§5a.1 §5;
/// ADR-0271): this run's `BranchRecord` (when it is a forked child), its
/// `rolled_back` rewind history, and its children. Pure over the WALs —
/// [`Store::branch_tree`](crate::store::Store::branch_tree) is the fold; the
/// view is the stamped projection.
pub fn branch_tree(
    run_id: &str,
    info: Option<&crate::branch::BranchInfo>,
    until: Option<u64>,
) -> View {
    let watermark = until;
    let payload = match info {
        Some(i) => Json::obj([
            ("kind", Json::str("branch_tree")),
            ("run_id", Json::str(run_id)),
            (
                "branch",
                i.record.as_ref().map(|r| r.to_json()).unwrap_or(Json::Null),
            ),
            (
                "children",
                Json::Arr(i.children.iter().map(Json::str).collect()),
            ),
            ("rolled_back", Json::Arr(i.rolled_back.clone())),
            (
                "head_seq",
                i.head_seq
                    .map(|s| Json::Int(s as i64))
                    .unwrap_or(Json::Null),
            ),
        ]),
        None => Json::obj([
            ("kind", Json::str("branch_tree")),
            ("run_id", Json::str(run_id)),
            ("branch", Json::Null),
            ("children", Json::Arr(vec![])),
            ("rolled_back", Json::Arr(vec![])),
            ("head_seq", Json::Null),
        ]),
    };
    View::build(run_id, ViewKind::BranchTree, watermark, payload)
}

/// The `compact` item-kind map — class (prefix) → CLI item kind. Every
/// mapped class contributes one item per event; every other class is a
/// declared loss (ADR-0169 D3 — the report is the contract, nothing
/// vanishes silently).
const COMPACT_MAP: &[(&str, &str)] = &[
    ("model.call.requested", "message"),
    ("model.call.completed", "message"),
    ("model.call.failed", "message"),
    ("action.tool.completed", "tool"),
    ("action.tool.rejected", "tool"),
    ("action.tool.call.refused", "tool"),
    ("action.effect.", "effect"),
    ("security.permission.", "approval"),
    ("control.budget.", "cost"),
    ("measurement.cost.attributed", "cost"),
];

/// The payload members the compact item copies verbatim (a bounded
/// hand-picked set — the event's own summary fields, never the whole
/// payload, which is the point of the lowering).
const COMPACT_FIELDS: &[&str] = &[
    "effect_id",
    "tool_name",
    "permission_id",
    "decision",
    "status",
    "model",
    "dimension",
    "ceiling",
    "turn",
];

/// `project(compact)` — the declared-lossy CLI view (§7.1; ADR-0169 D3;
/// R-2.5.3¹). A pure fold like every other view: `items[]` carry
/// `{seq, event_id, kind, class, summary}` for the mapped classes;
/// `loss_report.dropped[]` enumerates every class the fold omits with
/// its event count — AC-R-2.11.1: "the `compact` view ships a loss
/// report enumerating dropped classes".
pub fn compact(run_id: &str, events: &[EventEnvelope], until: Option<u64>) -> View {
    let item_kind = |class: &str| -> Option<&'static str> {
        COMPACT_MAP.iter().find_map(|(prefix, kind)| {
            if prefix.ends_with('.') {
                class.starts_with(prefix).then_some(*kind)
            } else {
                (class == *prefix).then_some(*kind)
            }
        })
    };
    let mut items = Vec::new();
    let mut dropped: BTreeMap<String, u64> = BTreeMap::new();
    let mut watermark = None;
    for e in events {
        if let Some(u) = until {
            if e.seq > u {
                continue;
            }
        }
        watermark = Some(e.seq);
        match item_kind(&e.class) {
            Some(kind) => {
                let mut summary = BTreeMap::new();
                for f in COMPACT_FIELDS {
                    if let Some(v) = e.payload.get(f) {
                        summary.insert(f.to_string(), v.clone());
                    }
                }
                items.push(Json::Obj(BTreeMap::from([
                    ("seq".to_string(), Json::Int(e.seq as i64)),
                    ("event_id".to_string(), Json::str(&e.event_id)),
                    ("kind".to_string(), Json::str(kind)),
                    ("class".to_string(), Json::str(&e.class)),
                    ("summary".to_string(), Json::Obj(summary)),
                ])));
            }
            None => *dropped.entry(e.class.clone()).or_insert(0) += 1,
        }
    }
    let loss_report = Json::obj([
        ("view", Json::str("compact")),
        (
            "mapped",
            Json::Arr(
                COMPACT_MAP
                    .iter()
                    .map(|(class, kind)| {
                        Json::obj([
                            ("class", Json::str(*class)),
                            ("item_kind", Json::str(*kind)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "dropped",
            Json::Arr(
                dropped
                    .iter()
                    .map(|(class, count)| {
                        Json::obj([
                            ("class", Json::str(class.clone())),
                            ("count", Json::Int(*count as i64)),
                            (
                                "reason",
                                Json::str(
                                    "no compact item kind — the class is outside \
                                     message/tool/effect/approval/cost",
                                ),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    let payload = Json::Obj(BTreeMap::from([
        ("kind".to_string(), Json::str("compact")),
        ("item_count".to_string(), Json::Int(items.len() as i64)),
        ("items".to_string(), Json::Arr(items)),
        ("loss_report".to_string(), loss_report),
    ]));
    View::build(run_id, ViewKind::Compact, watermark, payload)
}
