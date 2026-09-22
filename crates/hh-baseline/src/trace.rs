//! A single-file trace with M1/M3/M7 payloads + minimal `trace_view`/`cost_view` (R-2.9.1;
//! §5h.1). **Throwaway Stage-0 subset — NOT the durable run ledger.**
//!
//! §9.1 R-2.9.1 slice: "M1/M3/M7 payloads in a single-file trace; minimal `trace_view` and
//! `cost_view`". The scope kinds are (§5h measurement points): **M1** = run scope
//! (`lifecycle.run.*`, `stop_reason`), **M3** = model_call scope (`model.route.decided`,
//! `model.call.completed` with usage), **M7** = tool_call scope (`action.tool.*` with
//! `executor_ms`/`observation_bytes`). The event store / run ledger with `seq`, envelopes,
//! writer leases and projections lands at S1.5; here events are appended as canonical JSON
//! lines to one file, and `run events` reads them back byte-for-byte.
//!
//! CC3 (nothing unaccounted): `cost_view` totals are read from the [`crate::budget::BudgetNode`]
//! account, and [`Trace::charged_totals`] sums the trace's own `control.budget.consumed`
//! events — a test asserts the two agree, so no producing event goes unaccounted.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use hh_wire::json::Json;

use crate::budget::{BudgetNode, Dimension};

/// The measurement scope a trace event belongs to (the Stage-0 subset M1/M3/M7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// M1 — run scope.
    Run,
    /// M3 — model_call scope.
    ModelCall,
    /// M7 — tool_call scope.
    ToolCall,
    /// Context/permission events (no M-point of their own at Stage 0; carried for the view).
    Control,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Run => "M1",
            Scope::ModelCall => "M3",
            Scope::ToolCall => "M7",
            Scope::Control => "control",
        }
    }
}

/// One trace event. `seq` is a per-run monotonic counter (there is no writer lease at Stage 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEvent {
    pub seq: i64,
    pub event: String,
    pub scope: Scope,
    pub payload: Json,
}

impl TraceEvent {
    /// The canonical one-line JSON encoding — the exact bytes `run events` and `run start
    /// --jsonl` both emit (AC-R-2.11.1-3 byte-equality).
    pub fn to_line(&self) -> String {
        Json::obj([
            ("seq", Json::Int(self.seq)),
            ("event", Json::str(self.event.clone())),
            ("scope", Json::str(self.scope.as_str())),
            ("payload", self.payload.clone()),
        ])
        .to_canonical_string()
    }
}

/// The single-file trace: an in-memory event list mirrored to one file on disk.
#[derive(Debug)]
pub struct Trace {
    path: PathBuf,
    events: Vec<TraceEvent>,
    next_seq: i64,
}

impl Trace {
    /// Open (truncate) the single trace file for a run.
    pub fn open(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, b"")?;
        Ok(Self {
            path,
            events: Vec::new(),
            next_seq: 0,
        })
    }

    /// Append one event, assigning the next `seq`, and flush its canonical line to the file.
    pub fn append(&mut self, event: &str, scope: Scope, payload: Json) -> std::io::Result<i64> {
        let seq = self.next_seq;
        self.next_seq += 1;
        let ev = TraceEvent {
            seq,
            event: event.to_string(),
            scope,
            payload,
        };
        let mut f = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(f, "{}", ev.to_line())?;
        self.events.push(ev);
        Ok(seq)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn events(&self) -> &[TraceEvent] {
        &self.events
    }

    /// Events from `seq` onward (backs `run events --from N`).
    pub fn events_from(&self, from: i64) -> Vec<&TraceEvent> {
        self.events.iter().filter(|e| e.seq >= from).collect()
    }

    /// The minimal `trace_view`: one span row per event with its scope. Deterministic — two
    /// rebuilds yield the same rows (the Stage-0 stand-in for `view_hash` stability).
    pub fn trace_view(&self) -> Vec<(i64, Scope, String)> {
        self.events
            .iter()
            .map(|e| (e.seq, e.scope, e.event.clone()))
            .collect()
    }

    /// The minimal `cost_view`: per-dimension totals read from the budget account.
    pub fn cost_view(&self, budget: &BudgetNode) -> BTreeMap<Dimension, i64> {
        let mut m = BTreeMap::new();
        for d in Dimension::all() {
            m.insert(d, budget.consumed(d));
        }
        m
    }

    /// The totals the trace itself charged, summed from its `control.budget.consumed` events.
    /// Used to prove CC3: this equals [`Trace::cost_view`] on a well-formed run.
    pub fn charged_totals(&self) -> BTreeMap<Dimension, i64> {
        let mut m = BTreeMap::new();
        for d in Dimension::all() {
            m.insert(d, 0);
        }
        for e in &self.events {
            if e.event == "control.budget.consumed" {
                let dim = e.payload.get("dimension").and_then(Json::as_str);
                let amount = e.payload.get("amount").and_then(Json::as_int).unwrap_or(0);
                for d in Dimension::all() {
                    if dim == Some(d.as_str()) {
                        *m.get_mut(&d).unwrap() += amount;
                    }
                }
            }
        }
        m
    }

    /// A declared-lossy `compact` projection: message/tool/cost items only, plus the loss
    /// report enumerating the event classes it dropped (AC-R-2.11.1-3). Never the default.
    pub fn compact_view(&self) -> (Vec<TraceEvent>, LossReport) {
        const KEPT: &[&str] = &[
            "model.call.completed",
            "action.tool.completed",
            "control.budget.consumed",
            "lifecycle.run.finished",
        ];
        let mut kept = Vec::new();
        let mut dropped: BTreeMap<String, i64> = BTreeMap::new();
        for e in &self.events {
            if KEPT.contains(&e.event.as_str()) {
                kept.push(e.clone());
            } else {
                *dropped.entry(e.event.clone()).or_insert(0) += 1;
            }
        }
        (
            kept,
            LossReport {
                dropped_classes: dropped,
            },
        )
    }
}

/// The loss report a lossy projection must ship (the Stage-0 stand-in for `LoweringLossReport`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LossReport {
    /// Dropped event class → count.
    pub dropped_classes: BTreeMap<String, i64>,
}

impl LossReport {
    pub fn to_json(&self) -> Json {
        let mut pairs: Vec<Json> = self
            .dropped_classes
            .iter()
            .map(|(k, v)| Json::obj([("class", Json::str(k.clone())), ("count", Json::Int(*v))]))
            .collect();
        pairs.sort_by_key(|a| a.to_canonical_string());
        Json::obj([("dropped_classes", Json::Arr(pairs))])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "hh-baseline-trace-{tag}-{}.jsonl",
            std::process::id()
        ))
    }

    #[test]
    fn events_serialize_canonically_and_read_back() {
        let mut t = Trace::open(tmp("rw")).unwrap();
        t.append(
            "lifecycle.run.created",
            Scope::Run,
            Json::obj([("run_id", Json::str("r1"))]),
        )
        .unwrap();
        t.append(
            "model.call.completed",
            Scope::ModelCall,
            Json::obj([("blended", Json::Int(15))]),
        )
        .unwrap();
        let lines: Vec<String> = t.events().iter().map(|e| e.to_line()).collect();
        // canonical, sorted-key: keys appear in alphabetical order
        assert!(lines[0].starts_with("{\"event\":"));
        assert_eq!(t.events_from(1).len(), 1);
        assert_eq!(t.events_from(0).len(), 2);
    }

    #[test]
    fn charged_totals_match_cost_view() {
        let mut b = BudgetNode::root(100, 5, 10_000);
        let mut t = Trace::open(tmp("cost")).unwrap();
        b.charge(Dimension::TokensBlended, 15);
        t.append(
            "control.budget.consumed",
            Scope::Control,
            Json::obj([
                ("dimension", Json::str("tokens.blended")),
                ("amount", Json::Int(15)),
            ]),
        )
        .unwrap();
        // CC3: what the trace charged equals what the cost_view reports.
        assert_eq!(
            t.charged_totals().get(&Dimension::TokensBlended),
            t.cost_view(&b).get(&Dimension::TokensBlended)
        );
    }

    #[test]
    fn compact_view_ships_a_loss_report() {
        let mut t = Trace::open(tmp("compact")).unwrap();
        t.append("lifecycle.run.created", Scope::Run, Json::Null)
            .unwrap();
        t.append("context.assembled", Scope::Control, Json::Null)
            .unwrap();
        t.append("action.tool.completed", Scope::ToolCall, Json::Null)
            .unwrap();
        let (kept, loss) = t.compact_view();
        assert!(kept.iter().any(|e| e.event == "action.tool.completed"));
        // dropped classes are enumerated
        assert!(loss.dropped_classes.contains_key("lifecycle.run.created"));
        assert!(loss.dropped_classes.contains_key("context.assembled"));
    }
}
