//! The §5h.1 §3 views — `trace_view`, `cost_view`, `metric_view` — plus the
//! exporter's `sink_deliveries` readback. Every view is a pure fold over the
//! durable prefix (`Store::read` output), stamped `derived_from_seq` +
//! `view_policy_version` + `view_hash` by [`View::stamped`] — identical
//! rebuilds agree byte-for-byte (INV-3; AC-R-2.9.1-13). No telemetry store
//! exists; a view is a projection, never a query against second state.
//!
//! `n/a` is a first-class cell value — `{"na":"<reason>"}` — never 0, never a
//! proxy (T-LCD-15).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_budget::pricing::SpendRow;
use hh_ledger::classes::ScopeKind as LedgerScope;
use hh_ledger::effect::{self, EffectFold};
use hh_ledger::event::EventEnvelope;
use hh_ledger::ids::ROOT_EVENT;
use hh_ledger::views::{View, ViewKind};
use hh_ontology::compliance::NaReason;
use hh_ontology::participant::Observability;
use hh_wire::json::Json;

use crate::catalogue::{self, FoldStatus};
use crate::clocks::{self, skewed, ts_span_ms};
use crate::events::{export_delivered_from_json, ExportDelivered};
use crate::scope::{self, MeasurementPoint, ScopeKind};
use crate::tokens::TokenVector;

/// The `n/a` spelling for a `NaReason` (`{"na":"<reason>"}` cell values).
pub fn na_str(r: NaReason) -> &'static str {
    match r {
        NaReason::Class => "class",
        NaReason::Observability => "observability",
        NaReason::Capability => "capability",
        NaReason::Mediation => "mediation",
        NaReason::EstimatorUndefined => "estimator_undefined",
        NaReason::NotRun => "not_run",
        NaReason::NoDetector => "no_detector",
    }
}

/// The ontology `Observability` spelling (participant.rs has no `as_str`).
pub fn obs_str(o: Observability) -> &'static str {
    match o {
        Observability::Events => "events",
        Observability::ModelIo => "model_io",
        Observability::EndState => "end_state",
        Observability::Ledger => "ledger",
    }
}

/// Payload members a point copies onto its span's `attrs` — content-free
/// members only (`usage`/`timing`/closed tags/counters; never raw IO — the
/// content-class rules of §7 apply to the view's consumers).
const CARRY: &[&str] = &[
    "usage",
    "timing",
    "tokens_before",
    "tokens_after",
    "decision",
    "decider",
    "attempt_no",
    "wall_ms",
    "boundary_overhead_ms",
    "approval_wait_ms",
    "stop_reason",
];

/// Whether a span closed inside the folded prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanStatus {
    /// The close class never fired within the prefix — open (the honest
    /// residue of an unfinished scope, never coerced closed).
    Open,
    /// Closed.
    Closed,
}

/// One rendered span — `trace_view`'s row shape `{scope_id, scope_kind,
/// measurement_point, parent_scope, start/end, duration_ms, events[]}` per §3,
/// extended with `duration_source`/`skew_ms`/`clock_skew_flag` (AC-R-2.9.1-13 —
/// the clock a duration came from is carried, never silently one or the
/// other).
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    /// The scope coordinate.
    pub scope_id: String,
    /// The closed scope kind (§5h.1 §2.4).
    pub scope_kind: ScopeKind,
    /// The measurement point that produced the span (§2.5).
    pub point_id: &'static str,
    /// The containing span (`"<kind>:<scope_id>"` — the innermost still-open
    /// ancestor-kind span at open time), when any.
    pub parent_scope: Option<String>,
    /// The opening envelope's `parent_event_id`, when it names a real parent.
    pub parent_event: Option<String>,
    /// First seq covered.
    pub start_seq: u64,
    /// Last seq covered (closed spans).
    pub end_seq: Option<u64>,
    /// The opening envelope `ts` (ordering stamp — never duration input).
    pub start_ts: String,
    /// The closing envelope `ts`.
    pub end_ts: Option<String>,
    /// The measured duration — the point's declared payload field or `Timing`
    /// pair (`duration_ms` is payload-measured, `measured_at`-stamped; never a
    /// `ts` difference — §2.6).
    pub duration_ms: Option<i64>,
    /// The duration's declared source (`payload_field:<member>` |
    /// `timing:<a>−<b>` | `unmeasured`).
    pub duration_source: &'static str,
    /// `duration_ms − ts_span_ms` when both exist — honest clock disagreement,
    /// reported never hidden.
    pub skew_ms: Option<i64>,
    /// `|skew| > clock_tolerance_ms` (§2.6's `clock_skew_flag`).
    pub clock_skew_flag: bool,
    /// Open or closed.
    pub status: SpanStatus,
    /// The event ids covered (open + close; one for a point event).
    pub events: Vec<String>,
    /// Carried content-free payload members.
    pub attrs: BTreeMap<String, Json>,
}

impl Span {
    /// The canonical JSON rendering.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("scope_id".into(), Json::str(&self.scope_id));
        m.insert("scope_kind".into(), Json::str(self.scope_kind.as_str()));
        m.insert("measurement_point".into(), Json::str(self.point_id));
        m.insert(
            "parent_scope".into(),
            self.parent_scope
                .as_deref()
                .map(Json::str)
                .unwrap_or(Json::Null),
        );
        m.insert(
            "parent_event".into(),
            self.parent_event
                .as_deref()
                .map(Json::str)
                .unwrap_or(Json::Null),
        );
        m.insert("start_seq".into(), Json::Int(self.start_seq as i64));
        m.insert(
            "end_seq".into(),
            self.end_seq
                .map(|s| Json::Int(s as i64))
                .unwrap_or(Json::Null),
        );
        m.insert("start_ts".into(), Json::str(&self.start_ts));
        m.insert(
            "end_ts".into(),
            self.end_ts.as_deref().map(Json::str).unwrap_or(Json::Null),
        );
        m.insert(
            "duration_ms".into(),
            self.duration_ms.map(Json::Int).unwrap_or(Json::Null),
        );
        m.insert("duration_source".into(), Json::str(self.duration_source));
        m.insert(
            "skew_ms".into(),
            self.skew_ms.map(Json::Int).unwrap_or(Json::Null),
        );
        m.insert("clock_skew_flag".into(), Json::Bool(self.clock_skew_flag));
        m.insert(
            "status".into(),
            Json::str(match self.status {
                SpanStatus::Open => "open",
                SpanStatus::Closed => "closed",
            }),
        );
        m.insert(
            "events".into(),
            Json::Arr(self.events.iter().map(|e| Json::str(e.clone())).collect()),
        );
        if !self.attrs.is_empty() {
            m.insert("attrs".into(), Json::Obj(self.attrs.clone()));
        }
        Json::Obj(m)
    }
}

/// The fold's working span.
struct SpanWork {
    scope_kind: ScopeKind,
    scope_id: String,
    point_id: &'static str,
    duration_source: &'static str,
    parent_event: Option<String>,
    parent_scope: Option<String>,
    start_seq: u64,
    start_ts: String,
    start_event: String,
    end_seq: Option<u64>,
    end_ts: Option<String>,
    end_event: Option<String>,
    duration_ms: Option<i64>,
    attrs: BTreeMap<String, Json>,
    closed: bool,
}

impl SpanWork {
    fn finish(self, tolerance_ms: i64) -> Span {
        let (skew_ms, flag) = match (self.duration_ms, self.end_ts.as_deref()) {
            (Some(d), Some(end)) => match ts_span_ms(&self.start_ts, end) {
                Some(span) => {
                    let skew = d - span;
                    (Some(skew), skewed(d, span, tolerance_ms))
                }
                None => (None, false),
            },
            _ => (None, false),
        };
        let mut events = vec![self.start_event.clone()];
        if let Some(e) = &self.end_event {
            events.push(e.clone());
        }
        Span {
            scope_id: self.scope_id,
            scope_kind: self.scope_kind,
            point_id: self.point_id,
            parent_scope: self.parent_scope,
            parent_event: self.parent_event,
            start_seq: self.start_seq,
            end_seq: self.end_seq,
            start_ts: self.start_ts,
            end_ts: self.end_ts,
            duration_ms: self.duration_ms,
            duration_source: self.duration_source,
            skew_ms,
            clock_skew_flag: flag,
            status: if self.closed {
                SpanStatus::Closed
            } else {
                SpanStatus::Open
            },
            events,
            attrs: self.attrs,
        }
    }
}

/// The telemetry scope kind's ledger `Scope` member, when the kind rides the
/// envelope scope (ADR-0043 D2 — the telemetry taxonomy extends the ledger's;
/// kinds the envelope scope doesn't carry resolve from payload members).
fn ledger_scope(kind: ScopeKind) -> Option<LedgerScope> {
    Some(match kind {
        ScopeKind::Turn => LedgerScope::Turn,
        ScopeKind::ModelCall => LedgerScope::ModelCall,
        ScopeKind::ToolCall => LedgerScope::ToolCall,
        ScopeKind::Effect => LedgerScope::Effect,
        ScopeKind::Subagent => LedgerScope::ChildRun,
        _ => return None,
    })
}

/// A payload string member.
fn member<'a>(env: &'a EventEnvelope, k: &str) -> Option<&'a str> {
    env.payload.get(k)?.as_str()
}

/// The scope coordinate an *opening* event gives a point.
fn open_scope_id(point: &MeasurementPoint, env: &EventEnvelope) -> Option<String> {
    match point.scope? {
        ScopeKind::Run => Some(env.run_id.clone()),
        // M4 — `model_attempt` is not an envelope-scope kind; the coordinate is
        // `{model_call_id}/a{attempt_no}` (the attempt under its call).
        ScopeKind::ModelAttempt => {
            let mc = env.scope.model_call_id.as_deref()?;
            let n = env.payload.get("attempt_no")?.as_int()?;
            Some(format!("{mc}/a{n}"))
        }
        ScopeKind::ToolAttempt => {
            let eff = member(env, "effect_id")?;
            let n = env.payload.get("attempt_no")?.as_int()?;
            Some(format!("{eff}/a{n}"))
        }
        // M10 — opened by the durable proposal, closed by the decision naming
        // it (`decided.proposal`); the coordinate is the opening event id.
        ScopeKind::Permission => Some(env.event_id.clone()),
        ScopeKind::Validation => member(env, "validator_invocation_id")
            .map(str::to_string)
            .or_else(|| Some(env.event_id.clone())),
        ScopeKind::Subagent => member(env, "child_run_id")
            .or_else(|| member(env, "subagent_run_id"))
            .map(str::to_string)
            .or_else(|| Some(env.event_id.clone())),
        ScopeKind::Compaction => member(env, "compaction_id")
            .map(str::to_string)
            .or_else(|| Some(env.event_id.clone())),
        ScopeKind::EnvironmentOp => member(env, "env_id")
            .map(str::to_string)
            .or_else(|| Some(env.event_id.clone())),
        ScopeKind::Wakeup => member(env, "wakeup_id")
            .map(str::to_string)
            .or_else(|| Some(env.event_id.clone())),
        ScopeKind::ComponentCall => member(env, "invocation_id")
            .map(str::to_string)
            .or_else(|| Some(env.event_id.clone())),
        // Point events (M5 `context.assembled`, M14 `context.memory.*`) — one
        // span per row.
        ScopeKind::ContextAssembly | ScopeKind::MemoryOp => Some(env.event_id.clone()),
        k => ledger_scope(k).and_then(|lk| env.scope.get(lk).map(str::to_string)),
    }
}

/// The scope coordinate a *closing* event gives a point — the opener's
/// coordinate where the close names it (`decided.proposal`), the payload
/// derivation otherwise.
fn close_scope_id(point: &MeasurementPoint, env: &EventEnvelope) -> Option<String> {
    match point.scope? {
        ScopeKind::Permission => member(env, "proposal").map(str::to_string),
        _ => open_scope_id(point, env),
    }
}

/// The measured duration a *closing* payload carries — the point's declared
/// `DurationSource` (payload field or `Timing` pair; never a `ts` difference).
fn duration_at_close(point: &MeasurementPoint, env: &EventEnvelope) -> Option<i64> {
    use crate::scope::DurationSource as DS;
    match point.duration {
        DS::PayloadField(f) => env.payload.get(f)?.as_int(),
        DS::TimingPair { start, end } => {
            let t = env.payload.get("timing")?;
            Some(t.get(end)?.as_int()? - t.get(start)?.as_int()?)
        }
        DS::Unmeasured => None,
    }
}

/// The `duration_source` spelling for a point.
fn duration_source(point: &MeasurementPoint) -> &'static str {
    use crate::scope::DurationSource as DS;
    match point.duration {
        DS::PayloadField(_) => "payload_field",
        DS::TimingPair { .. } => "timing_pair",
        DS::Unmeasured => "unmeasured",
    }
}

/// Copy the carried content-free members onto the span.
fn carry_attrs(attrs: &mut BTreeMap<String, Json>, env: &EventEnvelope) {
    for k in CARRY {
        if let Some(v) = env.payload.get(k) {
            attrs.insert((*k).to_string(), v.clone());
        }
    }
}

/// The innermost still-open ancestor-kind span — the structural parent under
/// the closed kind taxonomy.
fn parent_scope(
    spans: &[SpanWork],
    open: &BTreeMap<(ScopeKind, String), usize>,
    kind: ScopeKind,
) -> Option<String> {
    let _ = spans;
    for want in scope::ancestors(kind) {
        for (k, sid) in open.keys().rev() {
            if k == want {
                return Some(format!("{}:{}", k.as_str(), sid));
            }
        }
    }
    None
}

/// `trace_view` — the §5h.1 §3 span projection. `events` is the prefix to fold
/// (ordinarily `Store::read`'s durable vector); `until_seq` bounds it.
/// `clock_tolerance_ms` is the manifest's `clock_tolerance_ms`
/// ([`crate::clocks::clock_tolerance`]). Spans open at the prefix end render
/// `status: open`.
pub fn trace_view(
    run_id: &str,
    root_run_id: &str,
    events: &[EventEnvelope],
    until_seq: Option<u64>,
    clock_tolerance_ms: i64,
) -> View {
    let mut spans: Vec<SpanWork> = Vec::new();
    let mut open: BTreeMap<(ScopeKind, String), usize> = BTreeMap::new();
    let mut effects: BTreeMap<String, EffectFold> = BTreeMap::new();
    let mut watermark: Option<u64> = None;

    for env in events {
        if let Some(u) = until_seq {
            if env.seq > u {
                break;
            }
        }
        watermark = Some(env.seq);
        // The effect fold tracks phase so `probed`/`observed` closes honour the
        // §05a conditional-close rule — the ledger's own fold, never a
        // reimplementation (CC7).
        effect::fold_event(&mut effects, env);

        for p in scope::openers(&env.class).chain(scope::closers(&env.class)) {
            let Some(kind) = p.scope else { continue };
            let is_point_event =
                p.opens.contains(&env.class.as_str()) && p.closes.contains(&env.class.as_str());

            if is_point_event {
                let Some(scope_id) = open_scope_id(p, env) else {
                    continue;
                };
                let mut attrs = BTreeMap::new();
                carry_attrs(&mut attrs, env);
                spans.push(SpanWork {
                    scope_kind: kind,
                    scope_id,
                    point_id: p.id,
                    duration_source: duration_source(p),
                    parent_event: parent_event(env),
                    parent_scope: parent_scope(&spans, &open, kind),
                    start_seq: env.seq,
                    start_ts: env.ts.clone(),
                    start_event: env.event_id.clone(),
                    end_seq: Some(env.seq),
                    end_ts: Some(env.ts.clone()),
                    end_event: None,
                    duration_ms: duration_at_close(p, env),
                    attrs,
                    closed: true,
                });
                continue;
            }

            if p.closes.contains(&env.class.as_str()) {
                let sid = close_scope_id(p, env).or_else(|| {
                    // `observed`/`unknown` may not name `attempt_no` — fall back
                    // to the unique open attempt span on the same effect.
                    if kind == ScopeKind::ToolAttempt {
                        member(env, "effect_id").and_then(|eff| {
                            let prefix = format!("{eff}/a");
                            let mut hits = open
                                .iter()
                                .filter(|((k, s), _)| {
                                    *k == ScopeKind::ToolAttempt && s.starts_with(&prefix)
                                })
                                .map(|((_, s), _)| s.clone());
                            match (hits.next(), hits.next()) {
                                (Some(s), None) => Some(s),
                                _ => None,
                            }
                        })
                    } else {
                        None
                    }
                });
                let may_close = match kind {
                    ScopeKind::Effect => sid
                        .as_deref()
                        .map(|s| {
                            effect::scope_close_fires(&env.class, &env.payload, effects.get(s))
                        })
                        .unwrap_or(false),
                    _ => true,
                };
                if may_close {
                    if let Some(sid) = sid {
                        if let Some(i) = open.remove(&(kind, sid)) {
                            let w = &mut spans[i];
                            w.closed = true;
                            w.end_seq = Some(env.seq);
                            w.end_ts = Some(env.ts.clone());
                            w.end_event = Some(env.event_id.clone());
                            w.duration_ms = duration_at_close(p, env);
                            carry_attrs(&mut w.attrs, env);
                        }
                    }
                }
            }

            if p.opens.contains(&env.class.as_str()) {
                let Some(scope_id) = open_scope_id(p, env) else {
                    continue;
                };
                // A re-opened coordinate keeps the first span — the attempt
                // history lives in the point rows, not a guessed merge.
                if open.contains_key(&(kind, scope_id.clone())) {
                    continue;
                }
                let mut attrs = BTreeMap::new();
                carry_attrs(&mut attrs, env);
                spans.push(SpanWork {
                    scope_kind: kind,
                    scope_id: scope_id.clone(),
                    point_id: p.id,
                    duration_source: duration_source(p),
                    parent_event: parent_event(env),
                    parent_scope: parent_scope(&spans, &open, kind),
                    start_seq: env.seq,
                    start_ts: env.ts.clone(),
                    start_event: env.event_id.clone(),
                    end_seq: None,
                    end_ts: None,
                    end_event: None,
                    duration_ms: None,
                    attrs,
                    closed: false,
                });
                open.insert((kind, scope_id), spans.len() - 1);
            }
        }
    }

    let rendered: Vec<Span> = spans
        .into_iter()
        .map(|w| w.finish(clock_tolerance_ms))
        .collect();
    let closed = rendered
        .iter()
        .filter(|s| s.status == SpanStatus::Closed)
        .count();
    let payload = Json::obj([
        ("kind", Json::str("trace_view")),
        ("run_id", Json::str(run_id)),
        ("root_run_id", Json::str(root_run_id)),
        ("clock_tolerance_ms", Json::Int(clock_tolerance_ms)),
        (
            "spans",
            Json::Arr(rendered.iter().map(Span::to_json).collect()),
        ),
        ("span_count", Json::Int(rendered.len() as i64)),
        ("closed_count", Json::Int(closed as i64)),
        ("open_count", Json::Int((rendered.len() - closed) as i64)),
    ]);
    View::stamped(run_id, ViewKind::TraceView, watermark, payload)
}

/// The opening envelope's parent event id — `None` for `root`.
fn parent_event(env: &EventEnvelope) -> Option<String> {
    if env.parent_event_id == ROOT_EVENT {
        None
    } else {
        Some(env.parent_event_id.clone())
    }
}

/// The confidence minimum's spelling (exact > bounded > estimate > unknown —
/// `cost_view` reports the minimum, never an average).
fn confidence_label(rank: u8) -> &'static str {
    match rank {
        3 => "exact",
        2 => "bounded",
        1 => "estimate",
        _ => "unknown",
    }
}

/// Render a `currency → micro_units` map as a JSON object of ints.
fn micro_map_json(m: &BTreeMap<String, i64>) -> Json {
    Json::Obj(
        m.iter()
            .map(|(k, v)| (k.clone(), Json::Int(*v)))
            .collect(),
    )
}

/// `cost_view` — the §3 cost projection: `measurement.cost.attributed` rows
/// folded per charge subject (`attribution.budget_id`) with per-currency
/// `Money` micro-units (never summed across), `provenance_mix` by class,
/// `confidence_min` (the minimum — never an average), and `coverage_min_ppm`
/// (OQ-033 — coverage under 1.0 is never summed as complete). Plus the §3
/// counters (`tool_commit_totals`, `permission_wait_ms`) and
/// `contention_waits_ms` — `n/a{capability}` until `control.queue.contended`
/// lands with its owning slice.
pub fn cost_view(
    run_id: &str,
    root_run_id: &str,
    events: &[EventEnvelope],
    until_seq: Option<u64>,
) -> View {
    let fold = cost_fold(events, until_seq);
    let payload = Json::obj([
        ("kind", Json::str("cost_view")),
        ("run_id", Json::str(run_id)),
        ("root_run_id", Json::str(root_run_id)),
        ("subjects", Json::Obj(fold.subjects_json())),
        (
            "total_spend_micro",
            micro_map_json(&fold.totals_map()),
        ),
        ("total_rows", Json::Int(fold.total_rows())),
        ("tool_commit_totals", fold.tool_totals_json()),
        ("permission_wait_ms", Json::Int(fold.permission_wait_ms)),
        (
            "contention_waits_ms",
            Json::obj([("na", Json::str(na_str(NaReason::Capability)))]),
        ),
    ]);
    View::stamped(run_id, ViewKind::CostView, fold.watermark, payload)
}

// ── M12 — the run-tree rollup (AC-R-2.9.1-9; S4.15) ──────────────────────
//
// A `subagent` scope's measurement point (M12) closes on
// `control.subagent.{result,cancelled}` and rolls the *child run's* vector
// into the parent's view — counted exactly once. `cost_view_run_tree`
// is the tree extension of `cost_view`: the caller supplies the tree's
// members (each run's durable prefix plus its parent link, the manifest's
// `parent_run_id`/`spawn_event` coordinate); the view folds each run with
// the same [`cost_fold`] the single-run view uses, sums every member's
// totals into the tree totals (each run's rows enter the sum exactly once
// — a duplicated member id is a typed refusal, never a silent merge), and
// renders each child under its parent's `subagent` scope.

/// One member of a run tree — the run's durable prefix plus its parent
/// link (`None` on the root only).
#[derive(Debug, Clone, Copy)]
pub struct RunTreeSlice<'a> {
    /// The member's `run_id`.
    pub run_id: &'a str,
    /// The parent's `run_id` — `None` iff this member is the root.
    pub parent_run_id: Option<&'a str>,
    /// The member's durable event prefix.
    pub events: &'a [EventEnvelope],
}

/// `cost_view_run_tree` refusals — structural defects of the supplied tree,
/// never a partial rollup (a tree that cannot be walked honestly renders
/// `n/a`-free totals only when it is well-formed).
#[derive(Debug, Clone, PartialEq)]
pub enum CostTreeError {
    /// The tree names no member at `root_run_id`.
    RootMissing {
        /// The requested root.
        root_run_id: String,
    },
    /// A member other than the root carries no parent link.
    RootMismatch {
        /// The offending member.
        run_id: String,
    },
    /// A member's `parent_run_id` names no member of the tree.
    UnknownParent {
        /// The orphan member.
        run_id: String,
        /// The missing parent.
        parent_run_id: String,
    },
    /// A `run_id` appears twice — counting it once is the AC's whole point;
    /// a duplicated member is refused, never silently deduped.
    DuplicateRun {
        /// The duplicated id.
        run_id: String,
    },
}

impl std::fmt::Display for CostTreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CostTreeError::RootMissing { root_run_id } => {
                write!(f, "RootMissing({root_run_id})")
            }
            CostTreeError::RootMismatch { run_id } => {
                write!(f, "RootMismatch({run_id}: non-root member without a parent)")
            }
            CostTreeError::UnknownParent {
                run_id,
                parent_run_id,
            } => write!(f, "UnknownParent({run_id} → {parent_run_id})"),
            CostTreeError::DuplicateRun { run_id } => {
                write!(f, "DuplicateRun({run_id})")
            }
        }
    }
}

impl std::error::Error for CostTreeError {}

/// `cost_view(run_tree)` — the M12 rollup (AC-R-2.9.1-9). `members` is the
/// tree walked from `root_run_id`; each member folds through the same
/// [`cost_fold`] the single-run view uses. The tree `totals` are the sum of
/// the members' totals — a grandchild is counted once, under *its* parent's
/// `subagent` scope; `subagents[parent][child]` carries each child's own-run
/// totals exactly once (never the subtree sum — nesting composes the same
/// way the scope tree does).
pub fn cost_view_run_tree(
    root_run_id: &str,
    members: &[RunTreeSlice<'_>],
    until_seq: Option<u64>,
) -> Result<View, CostTreeError> {
    // ── tree shape checks (typed refusals, never partial sums) ─────────
    let mut ids: BTreeSet<&str> = BTreeSet::new();
    for m in members {
        if !ids.insert(m.run_id) {
            return Err(CostTreeError::DuplicateRun {
                run_id: m.run_id.to_string(),
            });
        }
    }
    if !ids.contains(root_run_id) {
        return Err(CostTreeError::RootMissing {
            root_run_id: root_run_id.to_string(),
        });
    }
    for m in members {
        match m.parent_run_id {
            None => {
                if m.run_id != root_run_id {
                    return Err(CostTreeError::RootMismatch {
                        run_id: m.run_id.to_string(),
                    });
                }
            }
            Some(p) => {
                if m.run_id == root_run_id {
                    return Err(CostTreeError::RootMismatch {
                        run_id: m.run_id.to_string(),
                    });
                }
                if !ids.contains(p) {
                    return Err(CostTreeError::UnknownParent {
                        run_id: m.run_id.to_string(),
                        parent_run_id: p.to_string(),
                    });
                }
            }
        }
    }

    // ── the per-run folds (one fold, one home — CC1) ───────────────────
    let events_by_id: BTreeMap<&str, &[EventEnvelope]> = members
        .iter()
        .map(|m| (m.run_id, m.events))
        .collect();
    let mut runs_json = BTreeMap::new();
    let mut subagents: BTreeMap<String, BTreeMap<String, Json>> = BTreeMap::new();
    let mut watermarks = BTreeMap::new();
    let mut totals: BTreeMap<String, i64> = BTreeMap::new();
    let mut total_rows = 0i64;
    let mut tool_rows = 0i64;
    let (mut tw, mut te, mut tc, mut tn) = (0i64, 0i64, 0i64, 0i64);
    let mut permission_wait_ms = 0i64;
    let mut root_watermark = None;

    for m in members {
        let fold = cost_fold(m.events, until_seq);
        if m.run_id == root_run_id {
            root_watermark = fold.watermark;
        }
        watermarks.insert(m.run_id.to_string(), fold.watermark.map(|w| Json::Int(w as i64)));
        let run_totals = fold.totals_map();
        let run_rows = fold.total_rows();
        runs_json.insert(
            m.run_id.to_string(),
            Json::obj([
                ("subjects", Json::Obj(fold.subjects_json())),
                ("total_spend_micro", micro_map_json(&run_totals)),
                ("total_rows", Json::Int(run_rows)),
                ("tool_commit_totals", fold.tool_totals_json()),
                ("permission_wait_ms", Json::Int(fold.permission_wait_ms)),
            ]),
        );
        // Each run's totals enter the tree sum exactly once.
        for (ccy, micro) in &run_totals {
            *totals.entry(ccy.clone()).or_insert(0) += micro;
        }
        total_rows += run_rows;
        tool_rows += fold.tool_rows;
        tw += fold.tool_wall_ms;
        te += fold.tool_env_ms;
        tc += fold.tool_cpu_ms;
        tn += fold.tool_net_bytes;
        permission_wait_ms += fold.permission_wait_ms;
        // The child is recorded once under its parent's `subagent` scope —
        // the spawn/close rows on the parent's own ledger supply the scope's
        // link, budget slice, and delegation time (M12's full row, §3).
        if let Some(p) = m.parent_run_id {
            let mut row = vec![
                ("total_spend_micro", micro_map_json(&run_totals)),
                ("total_rows", Json::Int(run_rows)),
            ];
            let parent_events = events_by_id.get(p).copied().unwrap_or(&[]);
            let spawned = parent_events.iter().find(|e| {
                e.class == "control.subagent.spawned"
                    && (e.payload.get("child_run_id").and_then(Json::as_str)
                        == Some(m.run_id)
                        || e.scope.child_run_id.as_deref() == Some(m.run_id))
            });
            let closed = parent_events.iter().find(|e| {
                matches!(
                    e.class.as_str(),
                    "control.subagent.result" | "control.subagent.cancelled"
                ) && (e.payload.get("child_run_id").and_then(Json::as_str)
                    == Some(m.run_id)
                    || e.scope.child_run_id.as_deref() == Some(m.run_id))
            });
            if let Some(sp) = spawned {
                row.push((
                    "spawn_event",
                    Json::str(format!("{}:{}", p, sp.event_id)),
                ));
                for field in ["delegation_ref", "budget_id", "reservation_id", "mode"] {
                    if let Some(v) = sp.payload.get(field) {
                        row.push((field, v.clone()));
                    }
                }
            }
            match closed {
                Some(cl) => {
                    row.push(("status", Json::str("closed")));
                    row.push((
                        "closed_by",
                        Json::str(cl.class.rsplit('.').next().unwrap_or("closed")),
                    ));
                    row.push((
                        "close_event",
                        Json::str(format!("{}:{}", p, cl.event_id)),
                    ));
                    // The delegation time — spawned.ts → close.ts, `n/a` when
                    // either stamp cannot be parsed (typed, never 0).
                    let delegation_ms = spawned
                        .and_then(|sp| clocks::ts_ms(&sp.ts))
                        .zip(clocks::ts_ms(&cl.ts))
                        .map(|(a, b)| b.saturating_sub(a));
                    row.push((
                        "delegation_ms",
                        delegation_ms.map_or_else(
                            || Json::obj([("na", Json::str("unparseable_ts"))]),
                            Json::Int,
                        ),
                    ));
                }
                None => row.push(("status", Json::str("open"))),
            }
            subagents
                .entry(p.to_string())
                .or_default()
                .insert(m.run_id.to_string(), Json::obj(row));
        }
    }

    let payload = Json::obj([
        ("kind", Json::str("cost_view")),
        ("scope", Json::str("run_tree")),
        ("run_id", Json::str(root_run_id)),
        ("root_run_id", Json::str(root_run_id)),
        ("runs", Json::Obj(runs_json)),
        (
            "subagents",
            Json::Obj(
                subagents
                    .into_iter()
                    .map(|(p, cs)| (p, Json::Obj(cs)))
                    .collect(),
            ),
        ),
        (
            "total_spend_micro",
            micro_map_json(&totals),
        ),
        ("total_rows", Json::Int(total_rows)),
        (
            "tool_commit_totals",
            Json::obj([
                ("rows", Json::Int(tool_rows)),
                ("wall_ms", Json::Int(tw)),
                ("env_ms", Json::Int(te)),
                ("cpu_ms", Json::Int(tc)),
                ("net_bytes", Json::Int(tn)),
            ]),
        ),
        ("permission_wait_ms", Json::Int(permission_wait_ms)),
        (
            "contention_waits_ms",
            Json::obj([("na", Json::str(na_str(NaReason::Capability)))]),
        ),
        (
            "watermarks",
            Json::Obj(
                watermarks
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone().unwrap_or(Json::Null)))
                    .collect(),
            ),
        ),
    ]);
    Ok(View::stamped(
        root_run_id,
        ViewKind::CostView,
        root_watermark,
        payload,
    ))
}

/// The per-run cost fold — the single accumulation `cost_view` and
/// `cost_view_run_tree` share (CC1: one fold, one home).
struct CostFold {
    /// `budget_id → subject accumulator`.
    subjects: BTreeMap<String, SubjectAcc>,
    /// `action.tool.completed` rows folded.
    tool_rows: i64,
    /// Σ `commit_wall_ms`.
    tool_wall_ms: i64,
    /// Σ `commit_env_ms`.
    tool_env_ms: i64,
    /// Σ `commit_cpu_ms`.
    tool_cpu_ms: i64,
    /// Σ `commit_net_bytes`.
    tool_net_bytes: i64,
    /// Σ `approval_wait_ms` on `security.permission.decided`.
    permission_wait_ms: i64,
    /// The last seq folded.
    watermark: Option<u64>,
}

/// Fold one run's durable prefix into the cost accumulators.
fn cost_fold(events: &[EventEnvelope], until_seq: Option<u64>) -> CostFold {
    let mut fold = CostFold {
        subjects: BTreeMap::new(),
        tool_rows: 0,
        tool_wall_ms: 0,
        tool_env_ms: 0,
        tool_cpu_ms: 0,
        tool_net_bytes: 0,
        permission_wait_ms: 0,
        watermark: None,
    };
    for env in events {
        if let Some(u) = until_seq {
            if env.seq > u {
                break;
            }
        }
        fold.watermark = Some(env.seq);
        match env.class.as_str() {
            "measurement.cost.attributed" => {
                if let Some(row) = SpendRow::from_json(&env.payload) {
                    let acc = fold
                        .subjects
                        .entry(row.attribution.budget_id.clone())
                        .or_insert_with(|| SubjectAcc::new(&row.attribution.run_id));
                    acc.rows += 1;
                    *acc.spend.entry(row.money.currency.clone()).or_insert(0) +=
                        row.money.micro_units;
                    *acc.provenance_mix
                        .entry(row.provenance_class.as_str())
                        .or_insert(0) += 1;
                    acc.confidence_min = acc.confidence_min.min(row.confidence.rank());
                    acc.coverage_min_ppm = acc.coverage_min_ppm.min(row.coverage_ppm);
                }
            }
            "action.tool.completed" => {
                fold.tool_rows += 1;
                let get = |k: &str| env.payload.get(k).and_then(Json::as_int).unwrap_or(0);
                fold.tool_wall_ms += get("commit_wall_ms");
                fold.tool_env_ms += get("commit_env_ms");
                fold.tool_cpu_ms += get("commit_cpu_ms");
                fold.tool_net_bytes += get("commit_net_bytes");
            }
            "security.permission.decided" => {
                fold.permission_wait_ms += env
                    .payload
                    .get("approval_wait_ms")
                    .and_then(Json::as_int)
                    .unwrap_or(0);
            }
            _ => {}
        }
    }
    fold
}

impl CostFold {
    /// The per-subject JSON block (`budget_id → {run_id, spend, rows,
    /// provenance_mix, confidence_min, coverage_min_ppm}`).
    fn subjects_json(&self) -> BTreeMap<String, Json> {
        let mut out = BTreeMap::new();
        for (subject, acc) in &self.subjects {
            out.insert(
                subject.clone(),
                Json::obj([
                    ("run_id", Json::str(&acc.run_id)),
                    (
                        "spend",
                        Json::Obj(
                            acc.spend
                                .iter()
                                .map(|(k, v)| (k.clone(), Json::Int(*v)))
                                .collect(),
                        ),
                    ),
                    ("rows", Json::Int(acc.rows)),
                    (
                        "provenance_mix",
                        Json::Obj(
                            acc.provenance_mix
                                .iter()
                                .map(|(k, v)| (k.to_string(), Json::Int(*v)))
                                .collect(),
                        ),
                    ),
                    (
                        "confidence_min",
                        Json::str(confidence_label(acc.confidence_min)),
                    ),
                    ("coverage_min_ppm", Json::Int(acc.coverage_min_ppm)),
                ]),
            );
        }
        out
    }

    /// The per-currency totals map (`currency → Σ micro_units`).
    fn totals_map(&self) -> BTreeMap<String, i64> {
        let mut totals: BTreeMap<String, i64> = BTreeMap::new();
        for acc in self.subjects.values() {
            for (ccy, micro) in &acc.spend {
                *totals.entry(ccy.clone()).or_insert(0) += micro;
            }
        }
        totals
    }

    /// Σ spend rows across subjects.
    fn total_rows(&self) -> i64 {
        self.subjects.values().map(|a| a.rows).sum()
    }

    /// The `tool_commit_totals` block.
    fn tool_totals_json(&self) -> Json {
        Json::obj([
            ("rows", Json::Int(self.tool_rows)),
            ("wall_ms", Json::Int(self.tool_wall_ms)),
            ("env_ms", Json::Int(self.tool_env_ms)),
            ("cpu_ms", Json::Int(self.tool_cpu_ms)),
            ("net_bytes", Json::Int(self.tool_net_bytes)),
        ])
    }
}

struct SubjectAcc {
    run_id: String,
    rows: i64,
    spend: BTreeMap<String, i64>,
    provenance_mix: BTreeMap<&'static str, i64>,
    confidence_min: u8,
    coverage_min_ppm: i64,
}

impl SubjectAcc {
    fn new(run_id: &str) -> SubjectAcc {
        SubjectAcc {
            run_id: run_id.to_string(),
            rows: 0,
            spend: BTreeMap::new(),
            provenance_mix: BTreeMap::new(),
            confidence_min: 3,
            coverage_min_ppm: hh_budget::quantity::PPM_SCALE,
        }
    }
}

/// A `n/a` cell — `{"na":"<reason>"}`.
fn na(r: NaReason) -> Json {
    Json::obj([("na", Json::str(na_str(r)))])
}

/// The `q_ppm`-quantile of a **sorted** sample (nearest-rank — the result is
/// always a realised value, never interpolated; deterministic). The one
/// rounding rule mirrors `hh-eval::stats::quantile` (hh-telemetry does not
/// depend on hh-eval — the rule is duplicated, not shared).
fn nearest_rank(sorted: &[i64], q_ppm: i64) -> i64 {
    let ppm = hh_budget::quantity::PPM_SCALE;
    let n = sorted.len() as i64;
    let rank = ((q_ppm.clamp(0, ppm) as i128 * n as i128 + ppm as i128 - 1) / ppm as i128)
        .clamp(1, n as i128) as usize;
    sorted[rank - 1]
}

/// `metric_view` — the §3 metric projection over the catalogue (ADR-0044 D5).
/// `declared` is the run's stamped observability set; a metric whose
/// `requires_observability` the run doesn't meet renders `n/a{observability}`
/// — never 0, never a proxy (T-LCD-15). `fold: NotComputed` rows render their
/// declared `n/a{reason}`; the rest compute over the prefix.
pub fn metric_view(
    run_id: &str,
    root_run_id: &str,
    declared: &BTreeSet<Observability>,
    events: &[EventEnvelope],
    until_seq: Option<u64>,
) -> View {
    let ev: Vec<&EventEnvelope> = events
        .iter()
        .take_while(|e| until_seq.is_none_or(|u| e.seq <= u))
        .collect();
    let watermark = ev.last().map(|e| e.seq);

    let mut metrics = BTreeMap::new();
    for m in catalogue::PROCESS_METRICS {
        let required: BTreeSet<Observability> = m.requires_observability.iter().copied().collect();
        let cell = if !required.is_subset(declared) {
            na(NaReason::Observability)
        } else {
            match m.fold {
                FoldStatus::NotComputed(r) => na(r),
                FoldStatus::Computable => compute_metric(m.name, &ev),
            }
        };
        metrics.insert(m.name.to_string(), cell);
    }

    let payload = Json::obj([
        ("kind", Json::str("metric_view")),
        ("run_id", Json::str(run_id)),
        ("root_run_id", Json::str(root_run_id)),
        (
            "declared_observability",
            Json::Arr(declared.iter().map(|o| Json::str(obs_str(*o))).collect()),
        ),
        ("metrics", Json::Obj(metrics)),
    ]);
    View::stamped(run_id, ViewKind::MetricView, watermark, payload)
}

fn payload_str<'a>(e: &'a EventEnvelope, member_name: &str) -> Option<&'a str> {
    e.payload.get(member_name)?.as_str()
}

fn payload_int(e: &EventEnvelope, member_name: &str) -> i64 {
    e.payload
        .get(member_name)
        .and_then(Json::as_int)
        .unwrap_or(0)
}

/// Sum a decoded `usage` member into a flat `TokenVector` accumulator.
fn add_usage(acc: &mut TokenVector, v: &TokenVector) {
    acc.input_total += v.input_total;
    acc.input_uncached += v.input_uncached;
    acc.cache_read += v.cache_read;
    acc.cache_write += v.cache_write;
    acc.output_total += v.output_total;
    acc.output_reasoning += v.output_reasoning;
    acc.output_text = Some(acc.output_text.unwrap_or(0) + v.output_text.unwrap_or(0));
}

/// The zero accumulator for `add_usage` — `normalizer_ref`/`convention` are
/// rendered only after real rows contribute; the accumulator's own values are
/// placeholders never serialized while empty.
fn token_acc() -> TokenVector {
    TokenVector {
        input_total: 0,
        input_uncached: 0,
        cache_read: 0,
        cache_write: 0,
        output_total: 0,
        output_reasoning: 0,
        output_text: Some(0),
        convention: crate::tokens::TOKEN_CONVENTION.to_string(),
        normalizer_ref: "sum".to_string(),
    }
}

/// The summed `usage` over `model.call.completed` rows.
fn token_sum(ev: &[&EventEnvelope]) -> (TokenVector, i64) {
    let mut t = token_acc();
    let mut n = 0i64;
    for e in ev.iter().filter(|e| e.class == "model.call.completed") {
        if let Some(u) = e.payload.get("usage") {
            if let Ok(v) = TokenVector::from_json(u) {
                add_usage(&mut t, &v);
                n += 1;
            }
        }
    }
    (t, n)
}

/// The per-metric fold — one arm per `Computable` catalogue row.
fn compute_metric(name: &str, ev: &[&EventEnvelope]) -> Json {
    let ppm = hh_budget::quantity::PPM_SCALE;
    let count = |classes: &[&str]| -> i64 {
        ev.iter()
            .filter(|e| classes.contains(&e.class.as_str()))
            .count() as i64
    };
    match name {
        "tokens_total" => {
            let (t, _) = token_sum(ev);
            t.to_json()
        }
        "tokens_by_bucket" => {
            let (t, _) = token_sum(ev);
            Json::obj([
                (
                    "input",
                    Json::obj([
                        ("uncached", Json::Int(t.input_uncached)),
                        ("cache_read", Json::Int(t.cache_read)),
                        ("cache_write", Json::Int(t.cache_write)),
                        ("total", Json::Int(t.input_total)),
                    ]),
                ),
                (
                    "output",
                    Json::obj([
                        ("reasoning", Json::Int(t.output_reasoning)),
                        ("text", t.output_text.map(Json::Int).unwrap_or(Json::Null)),
                        ("total", Json::Int(t.output_total)),
                    ]),
                ),
            ])
        }
        "cache_hit_rate" => {
            let (t, _) = token_sum(ev);
            t.cache_hit_rate_ppm()
                .map(Json::Int)
                .unwrap_or_else(|| na(NaReason::EstimatorUndefined))
        }
        "cost_money" => {
            let mut totals: BTreeMap<String, i64> = BTreeMap::new();
            for e in ev
                .iter()
                .filter(|e| e.class == "measurement.cost.attributed")
            {
                if let Some(row) = SpendRow::from_json(&e.payload) {
                    *totals.entry(row.money.currency.clone()).or_insert(0) += row.money.micro_units;
                }
            }
            Json::Obj(
                totals
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Int(*v)))
                    .collect(),
            )
        }
        "model_calls" => Json::Int(count(&["model.call.requested"])),
        // §5e.2 process metrics (AC-R-2.6.2-11): `stop`-action
        // detections per model call; nudge detections that were the
        // run's *last* loop detection (the ladder recovered rather than
        // escalating) per nudge; format rejections per model call. An
        // empty denominator is `n/a{estimator_undefined}` — the hosted
        // arm's honest cell when no tool calls are observable (never 0).
        "loop_stop_rate" => {
            let den = count(&["model.call.requested"]);
            if den == 0 {
                na(NaReason::EstimatorUndefined)
            } else {
                let num = ev
                    .iter()
                    .filter(|e| {
                        e.class == "control.loop.detected"
                            && payload_str(e, "action") == Some("stop")
                    })
                    .count() as i64;
                Json::Int(num.saturating_mul(ppm) / den)
            }
        }
        "loop_nudge_recovery_rate" => {
            let nudges: Vec<&&EventEnvelope> = ev
                .iter()
                .filter(|e| {
                    e.class == "control.loop.detected" && payload_str(e, "action") == Some("nudge")
                })
                .collect();
            if nudges.is_empty() {
                na(NaReason::EstimatorUndefined)
            } else {
                // A nudge "recovered" when no later `control.loop.detected`
                // follows it (the pattern did not persist past the rung).
                let mut recovered = 0i64;
                for n in &nudges {
                    let later = ev
                        .iter()
                        .any(|e| e.class == "control.loop.detected" && e.seq > n.seq);
                    if !later {
                        recovered += 1;
                    }
                }
                Json::Int(recovered.saturating_mul(ppm) / nudges.len() as i64)
            }
        }
        "format_failure_rate" => {
            let den = count(&["model.call.requested"]);
            if den == 0 {
                na(NaReason::EstimatorUndefined)
            } else {
                let num = count(&["control.output.rejected"]);
                Json::Int(num.saturating_mul(ppm) / den)
            }
        }
        // §5e.4 — the scheduler's rows (`static` runs produce none, so a
        // static run's share cells stay honest `n/a`-less zeros only where
        // the metric is a count; share/rate over an empty set is
        // `n/a{estimator_undefined}`, never a fabricated 0).
        "scheduling.decision_count" => Json::Int(count(&["control.compute.decided"])),
        "scheduling.option_share" => {
            let mut kinds: BTreeMap<String, i64> = BTreeMap::new();
            for e in ev.iter().filter(|e| e.class == "control.compute.decided") {
                if let Some(k) = e
                    .payload
                    .get("chosen")
                    .and_then(|c| c.get("kind"))
                    .and_then(Json::as_str)
                {
                    *kinds.entry(k.to_string()).or_insert(0) += 1;
                }
            }
            Json::Obj(
                kinds
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::Int(*v)))
                    .collect(),
            )
        }
        "scheduling.prior_coverage" => {
            let rows: Vec<&&EventEnvelope> = ev
                .iter()
                .filter(|e| e.class == "control.compute.decided")
                .collect();
            if rows.is_empty() {
                na(NaReason::EstimatorUndefined)
            } else {
                // A record's coverage is live when some `priors_used`
                // cell carries `n > 0` (a post-reset `n = 0`/`unknown`
                // cell is recorded, not coverage — AC-F4-9's evidence).
                let covered = rows
                    .iter()
                    .filter(|e| {
                        matches!(
                            e.payload.get("priors_used"),
                            Some(Json::Arr(cells)) if cells.iter().any(|c| {
                                c.get("n").and_then(Json::as_int).unwrap_or(0) > 0
                            })
                        )
                    })
                    .count() as i64;
                Json::Int(covered * ppm / rows.len() as i64)
            }
        }
        "scheduling.overhead" => {
            let mut totals: BTreeMap<String, i64> = BTreeMap::new();
            let mut rows = 0i64;
            for e in ev.iter().filter(|e| e.class == "control.compute.decided") {
                rows += 1;
                if let Some(Json::Obj(m)) = e.payload.get("cost_of_estimation") {
                    for (d, a) in m {
                        if let Some(n) = a.as_int() {
                            *totals.entry(d.clone()).or_insert(0) += n;
                        }
                    }
                }
            }
            if rows == 0 {
                na(NaReason::EstimatorUndefined)
            } else {
                Json::Obj(
                    totals
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::Int(*v)))
                        .collect(),
                )
            }
        }
        "tool_calls" => Json::Int(count(&["action.tool.proposed"])),
        "retries" => {
            // `control.retry.fired` rows plus model-call attempts beyond the
            // first per call — the two surfaces a retry shows on (§2.5 M3/M4).
            let fired = count(&["control.retry.fired"]);
            let extra =
                (count(&["model.call.attempt.started"]) - count(&["model.call.requested"])).max(0);
            Json::Int(fired + extra)
        }
        "harness_overhead_share" => {
            // Σ non-model spend / Σ spend, per currency (never summed across).
            let mut totals: BTreeMap<String, (i64, i64)> = BTreeMap::new();
            for e in ev
                .iter()
                .filter(|e| e.class == "measurement.cost.attributed")
            {
                if let Some(row) = SpendRow::from_json(&e.payload) {
                    let ent = totals.entry(row.money.currency.clone()).or_default();
                    ent.1 += row.money.micro_units;
                    if row.attribution.component_class.as_deref() != Some("model") {
                        ent.0 += row.money.micro_units;
                    }
                }
            }
            if totals.is_empty() {
                return na(NaReason::EstimatorUndefined);
            }
            Json::Obj(
                totals
                    .iter()
                    .map(|(ccy, (over, total))| {
                        (
                            ccy.clone(),
                            if *total == 0 {
                                na(NaReason::EstimatorUndefined)
                            } else {
                                Json::Int(over * ppm / total)
                            },
                        )
                    })
                    .collect(),
            )
        }
        "latency_e2e_ms" => ev
            .iter()
            .rev()
            .find(|e| e.class == "lifecycle.run.finished")
            .map(|e| Json::Int(payload_int(e, "wall_ms")))
            .unwrap_or_else(|| na(NaReason::EstimatorUndefined)),
        "ttft_ms" | "ttfm_ms" => {
            let member = if name == "ttft_ms" {
                "first_token_ms"
            } else {
                "first_byte_ms"
            };
            let mut sum = 0i64;
            let mut n = 0i64;
            for e in ev
                .iter()
                .filter(|e| e.class == "model.call.attempt.completed")
            {
                if let Some(t) = e.payload.get("timing") {
                    let (rs, first) = (
                        t.get("request_sent_ms").and_then(Json::as_int),
                        t.get(member).and_then(Json::as_int),
                    );
                    if let (Some(rs), Some(f)) = (rs, first) {
                        sum += f - rs;
                        n += 1;
                    }
                }
            }
            if n == 0 {
                na(NaReason::EstimatorUndefined)
            } else {
                Json::Int(sum)
            }
        }
        "compaction_count" => Json::Int(count(&["context.compaction.completed"])),
        "tool_error_rate" => {
            const TERMINALS: &[&str] = &[
                "action.tool.completed",
                "action.tool.rejected",
                "action.tool.surface_rejected",
            ];
            const ERRORS: &[&str] = &["action.tool.rejected", "action.tool.surface_rejected"];
            let total = count(TERMINALS);
            if total == 0 {
                return na(NaReason::EstimatorUndefined);
            }
            let errors = count(ERRORS);
            // `tool_error_rate by class` — stratify on the payload's tool
            // class member when it names one.
            let mut by_class: BTreeMap<String, (i64, i64)> = BTreeMap::new();
            for e in ev.iter().filter(|e| TERMINALS.contains(&e.class.as_str())) {
                let cls = payload_str(e, "tool_class")
                    .or_else(|| payload_str(e, "tool"))
                    .unwrap_or("unknown")
                    .to_string();
                let ent = by_class.entry(cls).or_default();
                ent.1 += 1;
                if ERRORS.contains(&e.class.as_str()) {
                    ent.0 += 1;
                }
            }
            Json::obj([
                ("ppm", Json::Int(errors * ppm / total)),
                (
                    "by_class",
                    Json::Obj(
                        by_class
                            .iter()
                            .map(|(k, (e_, t_))| (k.clone(), Json::Int(e_ * ppm / (*t_).max(1))))
                            .collect(),
                    ),
                ),
            ])
        }
        "duplicate_effect_count" => {
            // A second `committed` row for the same `effect_id` is the vetoed
            // duplicate — the dedupe contract means `commit` returns the
            // stored observation rather than dispatching twice (§05d.3;
            // ADR-0102 §10), so a repeat `committed` never occurs honestly.
            let mut seen: BTreeMap<String, i64> = BTreeMap::new();
            let mut dupes = 0i64;
            for e in ev.iter().filter(|e| e.class == "action.effect.committed") {
                if let Some(id) = payload_str(e, "effect_id") {
                    let n = seen.entry(id.to_string()).or_insert(0);
                    *n += 1;
                    if *n > 1 {
                        dupes += 1;
                    }
                }
            }
            Json::Int(dupes)
        }
        "abandoned_effect_count" => Json::Int(count(&["action.effect.abandoned"])),
        "unknown_effect_count" => Json::Int(count(&["action.effect.unknown"])),
        "crash_recovery_count" => Json::Int(
            ev.iter()
                .filter(|e| {
                    e.class == "lifecycle.run.resumed"
                        && e.payload.get("worker_lost") == Some(&Json::Bool(true))
                })
                .count() as i64,
        ),
        "resume_count" => Json::Int(count(&["lifecycle.run.resumed"])),
        "fenced_writer_count" => Json::Int(count(&["lifecycle.lease.fenced"])),
        "rollback_count" => Json::Int(count(&["lifecycle.run.rolled_back"])),
        // ── the five recovery metrics (AC-R-2.2.3-12; §5a.3 §8) ──────────
        "recovery_latency_ms" => {
            // takeover `lease.acquired{scope: writer}` → `Cue.resumed` —
            // pair each `lifecycle.run.resumed{generation}` with the
            // writer `lease.acquired` row of the same generation;
            // `resumed_at_ms` is the delivery stamp (S3.6 member).
            let mut total = 0i64;
            let mut paired = 0i64;
            for r in ev.iter().filter(|e| e.class == "lifecycle.run.resumed") {
                let gen = payload_int(r, "generation");
                let resumed_at = payload_int(r, "resumed_at_ms");
                let acquired = ev
                    .iter()
                    .find(|e| {
                        e.class == "lifecycle.lease.acquired"
                            && payload_str(e, "scope") == Some("writer")
                            && payload_int(e, "generation") == gen
                    })
                    .map(|e| payload_int(e, "acquired_at_ms"));
                if let (Some(a), true) = (acquired, resumed_at > 0) {
                    total += (resumed_at - a).max(0);
                    paired += 1;
                }
            }
            if paired == 0 {
                if count(&["lifecycle.run.resumed"]) == 0 {
                    Json::Int(0)
                } else {
                    // resumes without a paired acquire are unmeasured —
                    // n/a, never 0 (T-LCD-15).
                    na(NaReason::Capability)
                }
            } else {
                Json::Int(total)
            }
        }
        "wasted_calls" => {
            // Calls the crash wasted: at each `resumed`, the
            // `model.call.requested` scopes at/below `from_seq` with no
            // `completed`/`failed` at/below it (the recovery table marks
            // them — the pre-crash attempt is the waste).
            let mut wasted = 0i64;
            for r in ev.iter().filter(|e| e.class == "lifecycle.run.resumed") {
                let from = payload_int(r, "from_seq");
                let mut open: BTreeMap<String, ()> = BTreeMap::new();
                for e in ev.iter().filter(|e| e.seq <= from as u64) {
                    if e.class == "model.call.requested" {
                        if let Some(mc) = e
                            .scope
                            .model_call_id
                            .as_deref()
                            .or_else(|| payload_str(e, "model_call_id"))
                        {
                            open.insert(mc.to_string(), ());
                        }
                    } else if matches!(
                        e.class.as_str(),
                        "model.call.completed" | "model.call.failed"
                    ) {
                        if let Some(mc) = e
                            .scope
                            .model_call_id
                            .as_deref()
                            .or_else(|| payload_str(e, "model_call_id"))
                        {
                            open.remove(mc);
                        }
                    }
                }
                wasted += open.len() as i64;
            }
            Json::Int(wasted)
        }
        "unknown_effects_per_resume" => {
            // Recovery-emitted `unknown` rows ÷ resumes — an `unknown`
            // whose seq sits between a `resumed` and the next
            // `control.decision`/`model.call.requested` is the restore's
            // own resolution output.
            let resumes: Vec<u64> = ev
                .iter()
                .filter(|e| e.class == "lifecycle.run.resumed")
                .map(|e| e.seq)
                .collect();
            if resumes.is_empty() {
                if count(&["action.effect.unknown"]) > 0 {
                    // unknowns with no resume — unpaired, n/a not 0.
                    return na(NaReason::EstimatorUndefined);
                }
                Json::Int(0)
            } else {
                let mut unknowns = 0i64;
                for (i, rseq) in resumes.iter().enumerate() {
                    let upper = resumes.get(i + 1).copied().unwrap_or(u64::MAX);
                    unknowns += ev
                        .iter()
                        .filter(|e| {
                            e.class == "action.effect.unknown"
                                && e.seq > *rseq
                                && e.seq < upper
                                && matches!(
                                    payload_str(e, "cause"),
                                    Some("worker_lost" | "helper_lost" | "scope_lost")
                                )
                        })
                        .count() as i64;
                }
                Json::Int(unknowns * ppm / resumes.len() as i64)
            }
        }
        "heal_count" => Json::Int(count(&["action.environment.healed"])),
        "wakeups_skipped" => {
            // `{reason: count}` — the `wakeups_skipped{reason}` map
            // (CountMap; empty map is the honest zero).
            let mut m = BTreeMap::new();
            for e in ev.iter().filter(|e| e.class == "control.wakeup.skipped") {
                let reason = payload_str(e, "reason").unwrap_or("unknown").to_string();
                *m.entry(reason).or_insert(0i64) += 1;
            }
            Json::Obj(m.into_iter().map(|(k, v)| (k, Json::Int(v))).collect())
        }
        "human_interventions" => Json::Int(
            ev.iter()
                .filter(|e| {
                    e.class == "security.permission.decided"
                        && payload_str(e, "decider") == Some("human")
                })
                .count() as i64,
        ),
        // `approval_requests` counts the durable owed-decision rows — the
        // `pending` class (I-P5; `requested` is the ephemeral prompt
        // rendering, never the count source).
        "approval_requests" => Json::Int(count(&["security.permission.pending"])),
        "approval_wait_ms" => Json::Int(
            ev.iter()
                .filter(|e| e.class == "security.permission.decided")
                .map(|e| payload_int(e, "approval_wait_ms"))
                .sum(),
        ),
        "approval_cache_hit_rate" => {
            let decided = count(&["security.permission.decided"]);
            if decided == 0 {
                return na(NaReason::EstimatorUndefined);
            }
            let cached = ev
                .iter()
                .filter(|e| {
                    e.class == "security.permission.decided"
                        && payload_str(e, "decider") == Some("cache")
                })
                .count() as i64;
            Json::Int(cached * ppm / decided)
        }
        "permission_denials" => Json::Int(
            ev.iter()
                .filter(|e| {
                    e.class == "security.permission.decided"
                        && payload_str(e, "decision") == Some("deny")
                })
                .count() as i64,
        ),
        "policy_evaluations" => {
            let mut m = BTreeMap::new();
            for tag in ["allow", "deny", "ask"] {
                m.insert(
                    tag.to_string(),
                    Json::Int(
                        ev.iter()
                            .filter(|e| {
                                e.class == "security.permission.decided"
                                    && payload_str(e, "decision") == Some(tag)
                            })
                            .count() as i64,
                    ),
                );
            }
            Json::Obj(m)
        }
        "credential_mediations" => Json::Int(count(&["security.credential.used"])),
        "taint_declassifications" => Json::Int(count(&["security.label.declassified"])),
        "budget_exceeded_count" => Json::Int(count(&["control.budget.exceeded"])),
        "attribution_coverage" => {
            let rows = count(&["measurement.cost.attributed"]);
            if rows == 0 {
                return na(NaReason::EstimatorUndefined);
            }
            let attributed = ev
                .iter()
                .filter(|e| e.class == "measurement.cost.attributed")
                .filter(|e| SpendRow::from_json(&e.payload).is_some())
                .count() as i64;
            Json::Int(attributed * ppm / rows)
        }
        // `harness_overhead.execution_ms` — the helper-overhead distribution
        // (AC-R-2.5.5-11; S3.9): each executed call's `execution_ms` M-point
        // grouped on `(executor_class, isolation_class)` — the capability's
        // own declared classes, never the executor's self-report. Cache-served
        // completions carry no M-point and contribute no sample (absent ≠ 0);
        // a prefix with no measured call is `n/a{estimator_undefined}`.
        "harness_overhead.execution_ms" => {
            let mut by_stratum: BTreeMap<String, Vec<i64>> = BTreeMap::new();
            for e in ev.iter().filter(|e| e.class == "action.tool.completed") {
                if let Some(ms) = e.payload.get("execution_ms").and_then(Json::as_int) {
                    let key = format!(
                        "{}/{}",
                        payload_str(e, "executor_class").unwrap_or("unknown"),
                        payload_str(e, "isolation_class").unwrap_or("unknown")
                    );
                    by_stratum.entry(key).or_default().push(ms);
                }
            }
            if by_stratum.is_empty() {
                return na(NaReason::EstimatorUndefined);
            }
            Json::Obj(
                by_stratum
                    .iter()
                    .map(|(k, xs)| {
                        let mut s = xs.clone();
                        s.sort_unstable();
                        (
                            k.clone(),
                            Json::obj([
                                ("n", Json::Int(s.len() as i64)),
                                ("min", Json::Int(s[0])),
                                ("p50", Json::Int(nearest_rank(&s, 500_000))),
                                ("p95", Json::Int(nearest_rank(&s, 950_000))),
                                ("max", Json::Int(s[s.len() - 1])),
                            ]),
                        )
                    })
                    .collect(),
            )
        }
        "boundary_overhead_ms" => Json::Int(
            ev.iter()
                .filter(|e| e.class == "lifecycle.component.invoked")
                .map(|e| payload_int(e, "boundary_overhead_ms"))
                .sum(),
        ),
        _ => na(NaReason::Capability),
    }
}

/// `sink_deliveries` — the exporter's readback (§2.2/§7): the durable
/// `measurement.export.delivered` rows in the prefix, decoded. An exporter
/// confirms delivery by *reading back* this row — never by trusting its own
/// side state.
pub fn sink_deliveries(events: &[EventEnvelope]) -> Vec<ExportDelivered> {
    events
        .iter()
        .filter(|e| e.class == "measurement.export.delivered")
        .filter_map(|e| export_delivered_from_json(&e.payload).ok())
        .collect()
}
