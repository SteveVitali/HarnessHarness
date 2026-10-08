//! The OpenInference semantic-convention lowering (R2.14, DF-S1.14-2 —
//! the second convention-shaped sink fixture; AC-R-2.9.1's "two
//! convention-shaped sinks … with the lift-back round-trip" cell).
//!
//! `lower_run` folds `(seq, event_class, payload)` triples — the same
//! caller-decoded durable prefix [`crate::genai`] reads; `lower_run_refs`
//! stamps the `hh.run_id`/`hh.event_id` liftable coordinates
//! ([`crate::lift`]). `model.call.{completed,failed}` become
//! `openinference.span.kind = LLM` spans; every member the convention
//! cannot spell is a named `unrepresentable_member` loss — the same
//! honest-loss contract as the `gen_ai.*` lowering (CC3, ADR-0042).
//!
//! OpenInference and OTel-GenAI spell the same usage facts differently
//! (`llm.token_count.prompt` vs `gen_ai.usage.input_tokens`); the
//! lowering never maps provider/model identifiers — the payload contract
//! is by member name only (K1-1).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_identity::idp::idp_digest;
use hh_wire::json::Json;

use crate::export::{LossEntry, LOSS_REPORT_DOMAIN};
use crate::genai::{Envelope, RefEnvelope};

/// The OpenInference attribute spellings the lowering emits.
pub mod attr {
    /// `openinference.span.kind` — `LLM` for model calls.
    pub const SPAN_KIND: &str = "openinference.span.kind";
    /// `llm.model_name` — the served model.
    pub const MODEL_NAME: &str = "llm.model_name";
    /// `llm.invocation_parameters` — provider request metadata.
    pub const INVOCATION: &str = "llm.invocation_parameters";
    /// `llm.token_count.prompt`.
    pub const TOKENS_PROMPT: &str = "llm.token_count.prompt";
    /// `llm.token_count.completion`.
    pub const TOKENS_COMPLETION: &str = "llm.token_count.completion";
    /// `llm.token_count.total`.
    pub const TOKENS_TOTAL: &str = "llm.token_count.total";
    /// `output.mime_type` / `output.value` are content members — never
    /// carried (the sink's content class is the caller's gate; named loss).
    /// `session.id` — the run's session coordinate.
    pub const SESSION_ID: &str = "session.id";
    /// `error.type` — the failure class member.
    pub const ERROR_TYPE: &str = "error.type";
}

/// What the OpenInference lowering produced — the spans plus the loss
/// record (the same shape `GenAiExport` has; a distinct type so a caller
/// can't mix convention outputs silently).
#[derive(Debug, Clone, PartialEq)]
pub struct OiExport {
    /// The lowered spans — `{name, seq, span_id, duration_ms?,
    /// attributes{…}}`.
    pub spans: Vec<Json>,
    /// The lowering loss entries.
    pub loss: Vec<LossEntry>,
    /// The durable seq range folded.
    pub seq_range: (u64, u64),
}

impl OiExport {
    /// The `TelemetryLossReport` digest — the same `telemetry.loss_report`
    /// domain and zero-loss-absent rule as every export record.
    pub fn loss_report_ref(&self) -> Option<String> {
        if self.loss.is_empty() {
            return None;
        }
        let report = Json::Arr(self.loss.iter().map(LossEntry::to_json).collect());
        Some(format!(
            "sha256:{}",
            idp_digest(LOSS_REPORT_DOMAIN, report.to_canonical_string().as_bytes())
        ))
    }
}

fn member_loss(loss: &mut Vec<LossEntry>, path: &str) {
    loss.push(LossEntry {
        kind: "unrepresentable_member",
        detail: path.to_string(),
    });
}

/// The members `model.call.completed` consumes — the rest is named loss.
const CARRIED: &[&str] = &[
    "model_call_id",
    "served_model",
    "stop_reason",
    "usage",
    "timing",
    "cache_observation",
];

/// `lower_model_call(seq, payload)` — one `model.call.completed` payload
/// → one `openinference.span.kind=LLM` span. Usage lowers to the
/// `llm.token_count.*` spellings (`input_total → prompt`,
/// `output_total → completion`, `input+output → total`; cache roles have
/// no OpenInference spelling — named loss); `timing.latency_ms` is the
/// span duration; every other member is a named loss.
pub fn lower_model_call(seq: u64, payload: &Json) -> (Json, Vec<LossEntry>) {
    let mut loss = Vec::new();
    let mut attrs = BTreeMap::new();
    attrs.insert(attr::SPAN_KIND.into(), Json::str("LLM"));
    let mut span = BTreeMap::new();
    span.insert("name".into(), Json::str("llm.chat"));
    span.insert("seq".into(), Json::Int(seq as i64));
    if let Some(id) = payload.get("model_call_id").and_then(Json::as_str) {
        span.insert("span_id".into(), Json::str(id));
    }
    if let Some(served) = payload.get("served_model").and_then(Json::as_str) {
        attrs.insert(attr::MODEL_NAME.into(), Json::str(served));
    }
    if let Some(sr) = payload.get("stop_reason").and_then(Json::as_str) {
        attrs.insert("llm.stop_reason".into(), Json::str(sr));
    }
    if let Some(u) = payload.get("usage") {
        // `hh-inclusive/1` view members — `prompt`/`completion`/`total`
        // carry; the cache roles and record/arrival/normalizer members
        // have no OpenInference spelling (named loss).
        if let Some(rec) = u.get("record") {
            if let Some(view) = rec.get("view") {
                if let Some(i) = view.get("input_total").and_then(Json::as_int) {
                    attrs.insert(attr::TOKENS_PROMPT.into(), Json::Int(i));
                }
                if let Some(o) = view.get("output_total").and_then(Json::as_int) {
                    attrs.insert(attr::TOKENS_COMPLETION.into(), Json::Int(o));
                }
                let i = view.get("input_total").and_then(Json::as_int).unwrap_or(0);
                let o = view.get("output_total").and_then(Json::as_int).unwrap_or(0);
                if i + o > 0 {
                    attrs.insert(attr::TOKENS_TOTAL.into(), Json::Int(i + o));
                }
                for k in ["cache_read", "cache_write"] {
                    if view.get(k).and_then(Json::as_int).unwrap_or(0) > 0 {
                        member_loss(
                            &mut loss,
                            &format!("model.call.completed.usage.record.view.{k}"),
                        );
                    }
                }
            }
            if let Json::Obj(rm) = rec {
                for k in rm.keys() {
                    if k != "view" {
                        member_loss(&mut loss, &format!("model.call.completed.usage.record.{k}"));
                    }
                }
            }
        }
        if let Json::Obj(um) = u {
            for k in um.keys() {
                if k != "record" && k != "available" {
                    member_loss(&mut loss, &format!("model.call.completed.usage.{k}"));
                }
            }
        }
    }
    if let Some(t) = payload.get("timing") {
        if let Some(lat) = t.get("latency_ms").and_then(Json::as_int) {
            span.insert("duration_ms".into(), Json::Int(lat));
        }
        if let Json::Obj(tm) = t {
            for k in tm.keys() {
                if k != "latency_ms" {
                    member_loss(&mut loss, &format!("model.call.completed.timing.{k}"));
                }
            }
        }
    }
    if payload.get("cache_observation").is_some() {
        member_loss(&mut loss, "model.call.completed.cache_observation");
    }
    if let Json::Obj(m) = payload {
        let consumed: BTreeSet<&str> = CARRIED.iter().copied().collect();
        for k in m.keys() {
            if !consumed.contains(k.as_str()) {
                member_loss(&mut loss, &format!("model.call.completed.{k}"));
            }
        }
    }
    span.insert("attributes".into(), Json::Obj(attrs));
    (Json::Obj(span), loss)
}

/// `lower_call_failed(seq, payload)` — one `model.call.failed` payload →
/// an `LLM` span with `error.type`; every other member is named loss.
pub fn lower_call_failed(seq: u64, payload: &Json) -> (Json, Vec<LossEntry>) {
    let mut loss = Vec::new();
    let mut attrs = BTreeMap::new();
    attrs.insert(attr::SPAN_KIND.into(), Json::str("LLM"));
    let consumed: BTreeSet<&str> = ["model_call_id", "error", "usage", "timing"]
        .into_iter()
        .collect();
    let mut span = BTreeMap::new();
    span.insert("name".into(), Json::str("llm.chat"));
    span.insert("seq".into(), Json::Int(seq as i64));
    if let Some(id) = payload.get("model_call_id").and_then(Json::as_str) {
        span.insert("span_id".into(), Json::str(id));
    }
    if let Some(err) = payload.get("error") {
        if let Some(class) = err.get("class").and_then(Json::as_str) {
            attrs.insert(attr::ERROR_TYPE.into(), Json::str(class));
        }
        if let Json::Obj(em) = err {
            for k in em.keys() {
                if k != "class" {
                    member_loss(&mut loss, &format!("model.call.failed.error.{k}"));
                }
            }
        }
    }
    if let Some(t) = payload.get("timing") {
        if let Some(lat) = t.get("latency_ms").and_then(Json::as_int) {
            span.insert("duration_ms".into(), Json::Int(lat));
        }
        if let Json::Obj(tm) = t {
            for k in tm.keys() {
                if k != "latency_ms" {
                    member_loss(&mut loss, &format!("model.call.failed.timing.{k}"));
                }
            }
        }
    }
    if payload.get("usage").is_some() {
        member_loss(&mut loss, "model.call.failed.usage");
    }
    if let Json::Obj(m) = payload {
        for k in m.keys() {
            if !consumed.contains(k.as_str()) {
                member_loss(&mut loss, &format!("model.call.failed.{k}"));
            }
        }
    }
    span.insert("attributes".into(), Json::Obj(attrs));
    (Json::Obj(span), loss)
}

/// `lower_run(envelopes)` — fold the run's model-plane prefix under the
/// OpenInference convention; `model.*` rows without a projection are
/// named `unrepresentable_event` losses, non-`model.*` rows pass
/// silently (the caller composes the other lowerings).
pub fn lower_run(events: &[Envelope<'_>]) -> OiExport {
    lower_run_gen(
        events
            .iter()
            .map(|(seq, class, p)| (*seq, None, *class, *p)),
    )
}

/// `lower_run_refs(envelopes)` — the liftable leg: every produced span
/// carries `hh.run_id`/`hh.event_id` so [`crate::lift::lift_span`]
/// resolves the sink row back to its source `EventRef`.
pub fn lower_run_refs(events: &[RefEnvelope<'_>]) -> OiExport {
    lower_run_gen(
        events
            .iter()
            .map(|(seq, run, ev, class, p)| (*seq, Some((*run, *ev)), *class, *p)),
    )
}

fn lower_run_gen<'a>(
    it: impl Iterator<Item = (u64, Option<(&'a str, &'a str)>, &'a str, &'a Json)>,
) -> OiExport {
    let mut spans = Vec::new();
    let mut loss = Vec::new();
    let mut lo = u64::MAX;
    let mut hi = 0u64;
    for (seq, coords, class, payload) in it {
        if !class.starts_with("model.") {
            continue;
        }
        lo = lo.min(seq);
        hi = hi.max(seq);
        match class {
            "model.call.completed" => {
                let (mut span, mut l) = lower_model_call(seq, payload);
                if let Some((run, ev)) = coords {
                    crate::lift::stamp_liftable(&mut span, run, ev);
                }
                spans.push(span);
                loss.append(&mut l);
            }
            "model.call.failed" => {
                let (mut span, mut l) = lower_call_failed(seq, payload);
                if let Some((run, ev)) = coords {
                    crate::lift::stamp_liftable(&mut span, run, ev);
                }
                spans.push(span);
                loss.append(&mut l);
            }
            other => loss.push(LossEntry {
                kind: "unrepresentable_event",
                detail: other.to_string(),
            }),
        }
    }
    OiExport {
        spans,
        loss,
        seq_range: if lo == u64::MAX { (0, 0) } else { (lo, hi) },
    }
}
