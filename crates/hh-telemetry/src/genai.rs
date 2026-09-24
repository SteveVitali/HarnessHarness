//! The OTel GenAI semantic-convention lowering of a run's model-plane rows
//! (AC-R-2.3.4-13; ADR-0042's honest-loss contract; §5b.4 "Export: roles lower
//! to OTel `gen_ai.usage.cache_read/cache_write.input_tokens`; `expected_state`,
//! `affinity_key`, `miss_reason`, `model.cache.resolved` and avoided cost are
//! declared loss").
//!
//! `lower_run` folds `(seq, event_class, payload)` triples — the caller's
//! decoded durable prefix (the crate never re-parses envelopes and never
//! touches a store). `model.call.completed`/`model.call.failed` become
//! `gen_ai.chat` client spans; `model.cache.resolved` has no GenAI projection
//! and lands as a named loss; every member a span cannot carry lands as a
//! `unrepresentable_member` loss — nothing is silently dropped (CC3).
//!
//! The lowering knows the model-plane payload contract by *member name* only
//! — no provider/model identifier ever branches the output (K1-1).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_identity::idp::idp_digest;
use hh_wire::json::Json;

use crate::export::{LossEntry, LOSS_REPORT_DOMAIN};

/// A `(seq, event_class, payload)` durable-prefix row.
pub type Envelope<'a> = (u64, &'a str, &'a Json);

/// The GenAI attribute spellings the lowering emits (the OTel semantic
/// conventions the spec names).
pub mod attr {
    /// `gen_ai.operation.name`.
    pub const OPERATION: &str = "gen_ai.operation.name";
    /// `gen_ai.request.model`.
    pub const REQUEST_MODEL: &str = "gen_ai.request.model";
    /// `gen_ai.response.model`.
    pub const RESPONSE_MODEL: &str = "gen_ai.response.model";
    /// `gen_ai.response.id`.
    pub const RESPONSE_ID: &str = "gen_ai.response.id";
    /// `gen_ai.response.finish_reasons`.
    pub const FINISH_REASONS: &str = "gen_ai.response.finish_reasons";
    /// `gen_ai.usage.input_tokens`.
    pub const INPUT_TOKENS: &str = "gen_ai.usage.input_tokens";
    /// `gen_ai.usage.output_tokens`.
    pub const OUTPUT_TOKENS: &str = "gen_ai.usage.output_tokens";
    /// `gen_ai.usage.cache_read.input_tokens`.
    pub const CACHE_READ: &str = "gen_ai.usage.cache_read.input_tokens";
    /// `gen_ai.usage.cache_write.input_tokens`.
    pub const CACHE_WRITE: &str = "gen_ai.usage.cache_write.input_tokens";
    /// `gen_ai.error.type`.
    pub const ERROR_TYPE: &str = "gen_ai.error.type";
    /// `gen_ai.evaluation.name` — the evaluated criterion/target.
    pub const EVAL_NAME: &str = "gen_ai.evaluation.name";
    /// `gen_ai.evaluation.score.value` — the numeric score.
    pub const EVAL_SCORE_VALUE: &str = "gen_ai.evaluation.score.value";
    /// `gen_ai.evaluation.score.label` — the categorical score.
    pub const EVAL_SCORE_LABEL: &str = "gen_ai.evaluation.score.label";
    /// `gen_ai.evaluation.explanation` — the verdict's explanation string.
    pub const EVAL_EXPLANATION: &str = "gen_ai.evaluation.explanation";
    /// `gen_ai.evaluation.status` — decided/inconclusive/oracle_failure.
    pub const EVAL_STATUS: &str = "gen_ai.evaluation.status";
}

/// The `model.call.completed` members the span carries — everything else on
/// the payload is a named loss.
const COMPLETED_CARRIED: &[&str] = &[
    "model_call_id",
    "served_model",
    "surface_ids",
    "stop_reason",
    "usage",
    "timing",
];

/// What the GenAI lowering produced — the spans plus the loss record.
#[derive(Debug, Clone, PartialEq)]
pub struct GenAiExport {
    /// The lowered spans — canonical JSON `{name, seq, span_id, duration_ms?,
    /// attributes{…}}`.
    pub spans: Vec<Json>,
    /// The lowering loss entries (`unrepresentable_event` /
    /// `unrepresentable_member` — one per dropped thing).
    pub loss: Vec<LossEntry>,
    /// The durable seq range folded.
    pub seq_range: (u64, u64),
}

impl GenAiExport {
    /// The `TelemetryLossReport` digest — same `telemetry.loss_report` domain
    /// and rule as [`crate::export::ExportBatch::loss_report_ref`]: absent on
    /// a zero-loss export, never serialized as an empty list.
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

/// `lower_model_call(seq, payload)` — one `model.call.completed` payload →
/// one `gen_ai.chat` client span. The usage/cache roles the OTel conventions
/// carry are emitted as attributes; every other member — `expected_state`,
/// `affinity_key`, `static_hash`, `observed_state`, `hit_ratio_ppm`,
/// `miss_reason`, `markers_*`, `snapshot_id`, `substitution`, `cost`,
/// `served_from_cache`, the usage `raw`/`normalizer_ref`/`arrival` and the
/// un-carried view members, the extra `timing` fields — is a named loss.
pub fn lower_model_call(seq: u64, payload: &Json) -> (Json, Vec<LossEntry>) {
    let mut loss = Vec::new();
    let mut attrs = BTreeMap::new();
    attrs.insert(attr::OPERATION.into(), Json::str("chat"));
    let mut consumed: BTreeSet<&str> = COMPLETED_CARRIED.iter().copied().collect();
    let mut span = BTreeMap::new();
    span.insert("name".into(), Json::str("gen_ai.chat"));
    span.insert("seq".into(), Json::Int(seq as i64));

    if let Some(id) = payload.get("model_call_id").and_then(Json::as_str) {
        span.insert("span_id".into(), Json::str(id));
    }
    if let Some(served) = payload.get("served_model").and_then(Json::as_str) {
        attrs.insert(attr::RESPONSE_MODEL.into(), Json::str(served));
    }
    if let Some(Json::Obj(ids)) = payload.get("surface_ids") {
        if let Some(rid) = ids.get("response_id").and_then(Json::as_str) {
            attrs.insert(attr::RESPONSE_ID.into(), Json::str(rid));
        }
        for (k, _) in ids.iter().filter(|(k, _)| k.as_str() != "response_id") {
            member_loss(&mut loss, &format!("model.call.completed.surface_ids.{k}"));
        }
    }
    if let Some(sr) = payload.get("stop_reason").and_then(Json::as_str) {
        attrs.insert(attr::FINISH_REASONS.into(), Json::Arr(vec![Json::str(sr)]));
    }
    // `usage{available, record{raw, view, normalizer_ref}, arrival}` — the
    // `hh-inclusive/1` view's four carried roles lower; the raw record, the
    // normalizer provenance, the arrival mode and the un-carried view
    // members are named loss.
    if let Some(u) = payload.get("usage") {
        if u.get("arrival").is_some() {
            member_loss(&mut loss, "model.call.completed.usage.arrival");
        }
        if let Some(rec) = u.get("record") {
            if rec.get("raw").is_some() {
                member_loss(&mut loss, "model.call.completed.usage.record.raw");
            }
            if rec.get("normalizer_ref").is_some() {
                member_loss(
                    &mut loss,
                    "model.call.completed.usage.record.normalizer_ref",
                );
            }
            if let Some(view) = rec.get("view") {
                for (member, attribute) in [
                    ("input_total", attr::INPUT_TOKENS),
                    ("output_total", attr::OUTPUT_TOKENS),
                    ("cache_read", attr::CACHE_READ),
                    ("cache_write", attr::CACHE_WRITE),
                ] {
                    if let Some(v) = view.get(member).and_then(Json::as_int) {
                        attrs.insert(attribute.into(), Json::Int(v));
                    }
                }
                if let Json::Obj(vm) = view {
                    for k in vm.keys() {
                        if !["input_total", "output_total", "cache_read", "cache_write"]
                            .contains(&k.as_str())
                        {
                            member_loss(
                                &mut loss,
                                &format!("model.call.completed.usage.record.view.{k}"),
                            );
                        }
                    }
                }
            }
        }
    }
    // `timing.latency_ms` is the span duration; the finer members
    // (ttft/attempts/queue_wait/measured_at) have no GenAI spelling.
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
    // `cache_observation` — `read` reaches the sink as
    // `gen_ai.usage.cache_read.input_tokens`; every other member
    // (`expected_state`, `affinity_key`, `static_hash`, `observed_state`,
    // `hit_ratio_ppm`, `write_ratio_ppm`, `write_by_class`, `agreement`,
    // `miss_reason`, `markers_dropped`, `markers_substituted`) is named
    // loss (AC-R-2.3.4-13).
    if let Some(Json::Obj(cm)) = payload.get("cache_observation") {
        consumed.insert("cache_observation");
        // The carried roles lower to the GenAI cache-token attributes —
        // anything else would be a silent drop (CC3).
        for (member, attribute) in [
            ("read", attr::CACHE_READ),
            ("cache_read", attr::CACHE_READ),
            ("write", attr::CACHE_WRITE),
            ("cache_write", attr::CACHE_WRITE),
        ] {
            if let Some(v) = cm.get(member).and_then(Json::as_int) {
                attrs.insert(attribute.into(), Json::Int(v));
            }
        }
        for k in cm.keys() {
            if !["read", "cache_read", "write", "cache_write"].contains(&k.as_str()) {
                member_loss(
                    &mut loss,
                    &format!("model.call.completed.cache_observation.{k}"),
                );
            }
        }
    }
    // Every remaining member is a named loss — the set is enumerated from
    // the payload, never hard-coded, so a future member can never slip
    // through silently (CC3).
    if let Json::Obj(m) = payload {
        for k in m.keys() {
            if !consumed.contains(k.as_str()) {
                member_loss(&mut loss, &format!("model.call.completed.{k}"));
            }
        }
    }
    span.insert("attributes".into(), Json::Obj(attrs));
    (Json::Obj(span), loss)
}

/// `lower_call_failed(seq, payload)` — one `model.call.failed` payload → a
/// `gen_ai.chat` span with `gen_ai.error.type`; every other member is a
/// named loss.
pub fn lower_call_failed(seq: u64, payload: &Json) -> (Json, Vec<LossEntry>) {
    let mut loss = Vec::new();
    let mut attrs = BTreeMap::new();
    attrs.insert(attr::OPERATION.into(), Json::str("chat"));
    let consumed: BTreeSet<&str> = ["model_call_id", "error", "usage", "timing"]
        .into_iter()
        .collect();
    let mut span = BTreeMap::new();
    span.insert("name".into(), Json::str("gen_ai.chat"));
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
    if let Some(Json::Obj(um)) = payload.get("usage") {
        for (member, attribute) in [
            ("input_total", attr::INPUT_TOKENS),
            ("output_total", attr::OUTPUT_TOKENS),
            ("cache_read", attr::CACHE_READ),
            ("cache_write", attr::CACHE_WRITE),
        ] {
            if let Some(v) = um.get(member).and_then(Json::as_int) {
                attrs.insert(attribute.into(), Json::Int(v));
            }
        }
        for k in um.keys() {
            if !["input_total", "output_total", "cache_read", "cache_write"].contains(&k.as_str()) {
                member_loss(&mut loss, &format!("model.call.failed.usage.{k}"));
            }
        }
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

/// `lower_verdict(seq, payload)` — one `verification.validator.verdict`
/// payload → a `gen_ai.evaluation.result` event (OTel eval-event
/// conventions; AC-R-2.7.3-10). The carried members round-trip:
///
/// - `gen_ai.evaluation.name` ← `criterion_ref` (else `target`)
/// - `gen_ai.evaluation.score.value` ← `value.value` numeric
/// - `gen_ai.evaluation.score.label` ← `value.value` categorical/boolean
/// - `gen_ai.evaluation.explanation` ← `findings[].code` + `status_detail`
/// - `gen_ai.evaluation.status` ← `status`
/// - `gen_ai.response.id` ← `response_ref` (else the `verdict_id`)
///
/// Every other member is a named loss — the AC's five named classes ride
/// the loss report: `validator_ref`/`detector`/`detector_ref`/
/// `oracle_class` (judge identity), `evidence_refs`/`inputs_digest`/
/// `evidence_head_seq`/`freshness_ok` (evidence), `calibration`/
/// `calibration_ref`/`calibration_id` (calibration), `independence`/
/// `independence_vector` (independence), `cost_ppm`/`charged_to` (cost) —
/// plus the enumeration sweep for anything else.
pub fn lower_verdict(seq: u64, payload: &Json) -> (Json, Vec<LossEntry>) {
    let mut loss = Vec::new();
    let mut attrs = BTreeMap::new();
    let mut event = BTreeMap::new();
    event.insert("name".into(), Json::str("gen_ai.evaluation.result"));
    event.insert("seq".into(), Json::Int(seq as i64));
    let mut consumed: BTreeSet<&str> = BTreeSet::new();

    // name ← criterion_ref | target.
    if let Some(n) = payload
        .get("criterion_ref")
        .and_then(Json::as_str)
        .or_else(|| payload.get("target").and_then(Json::as_str))
    {
        attrs.insert(attr::EVAL_NAME.into(), Json::str(n));
        consumed.insert("criterion_ref");
        consumed.insert("target");
    }
    // score ← value{kind,value} — ints/floats to score.value, bools and
    // strings to score.label.
    if let Some(v) = payload.get("value") {
        consumed.insert("value");
        let inner = v.get("value").unwrap_or(v);
        match inner {
            Json::Int(i) => {
                attrs.insert(attr::EVAL_SCORE_VALUE.into(), Json::Int(*i));
            }
            Json::Bool(bv) => {
                attrs.insert(
                    attr::EVAL_SCORE_LABEL.into(),
                    Json::str(if *bv { "pass" } else { "fail" }),
                );
            }
            Json::Str(sv) => {
                attrs.insert(attr::EVAL_SCORE_LABEL.into(), Json::str(sv));
            }
            _ => {
                member_loss(&mut loss, "verification.validator.verdict.value.value");
            }
        }
    }
    // status + explanation.
    if let Some(s) = payload.get("status").and_then(Json::as_str) {
        attrs.insert(attr::EVAL_STATUS.into(), Json::str(s));
        consumed.insert("status");
    }
    let mut explanation: Vec<String> = Vec::new();
    if let Some(d) = payload.get("status_detail").and_then(Json::as_str) {
        explanation.push(d.to_string());
        consumed.insert("status_detail");
    }
    if let Some(Json::Arr(fs)) = payload.get("findings") {
        consumed.insert("findings");
        for f in fs {
            if let Some(c) = f.get("code").and_then(Json::as_str) {
                explanation.push(c.to_string());
            }
            if let Json::Obj(fm) = f {
                for k in fm.keys() {
                    if k != "code" {
                        member_loss(
                            &mut loss,
                            &format!("verification.validator.verdict.findings.{k}"),
                        );
                    }
                }
            }
        }
    }
    if !explanation.is_empty() {
        attrs.insert(
            attr::EVAL_EXPLANATION.into(),
            Json::str(explanation.join(";")),
        );
    }
    // response.id ← response_ref | verdict_id.
    if let Some(r) = payload
        .get("response_ref")
        .and_then(Json::as_str)
        .or_else(|| payload.get("verdict_id").and_then(Json::as_str))
    {
        attrs.insert(attr::RESPONSE_ID.into(), Json::str(r));
        consumed.insert("response_ref");
        consumed.insert("verdict_id");
    }
    // The named-loss classes (AC-R-2.7.3-10): judge identity, evidence,
    // calibration, independence, cost — each an explicit member_loss so the
    // loss report names the class even when the sweep would spell it.
    for m in [
        "validator_ref",
        "detector",
        "detector_ref",
        "oracle_class",
        "evidence_refs",
        "inputs_digest",
        "evidence_head_seq",
        "freshness_ok",
        "calibration",
        "calibration_ref",
        "calibration_id",
        "independence",
        "independence_vector",
        "cost_ppm",
        "charged_to",
    ] {
        if payload.get(m).is_some() && !consumed.contains(m) {
            member_loss(
                &mut loss,
                &format!("verification.validator.verdict.{m}"),
            );
            consumed.insert(m);
        }
    }
    if let Json::Obj(m) = payload {
        for k in m.keys() {
            if !consumed.contains(k.as_str()) {
                member_loss(
                    &mut loss,
                    &format!("verification.validator.verdict.{k}"),
                );
            }
        }
    }
    event.insert("attributes".into(), Json::Obj(attrs));
    (Json::Obj(event), loss)
}

/// `lower_run(envelopes)` — fold a run's model-plane prefix. `model.call.*`
/// terminal rows become spans; `model.cache.resolved` is a named event loss
/// plus member losses (`key`, `scope`, `outcome`, `reason`, `entry_ref`,
/// `avoided` — `avoided.spend` spelled out when the estimate carries one —
/// `attribution`, `purpose`, `lookup_duration_ms`, `served_by`,
/// `cache_kind`); every other `model.*` class is an `unrepresentable_event`
/// loss. Non-`model.*` envelopes are outside this projection's scope and
/// pass silently — the caller composes the run's other lowerings.
pub fn lower_run(events: &[Envelope<'_>]) -> GenAiExport {
    let mut spans = Vec::new();
    let mut loss = Vec::new();
    let mut lo = u64::MAX;
    let mut hi = 0u64;
    for (seq, class, payload) in events {
        if !class.starts_with("model.") {
            continue;
        }
        lo = lo.min(*seq);
        hi = hi.max(*seq);
        match *class {
            "model.call.completed" => {
                let (span, mut l) = lower_model_call(*seq, payload);
                spans.push(span);
                loss.append(&mut l);
            }
            "model.call.failed" => {
                let (span, mut l) = lower_call_failed(*seq, payload);
                spans.push(span);
                loss.append(&mut l);
            }
            "model.cache.resolved" => {
                loss.push(LossEntry {
                    kind: "unrepresentable_event",
                    detail: "model.cache.resolved".to_string(),
                });
                if let Json::Obj(m) = payload {
                    for (k, v) in m {
                        let detail = match k.as_str() {
                            // `reason` is the closed `MissReason` member —
                            // the AC names it `miss_reason`.
                            "reason" => "model.cache.resolved.miss_reason".to_string(),
                            "avoided" => {
                                if v.get("spend").is_some() {
                                    "model.cache.resolved.avoided.spend".to_string()
                                } else {
                                    "model.cache.resolved.avoided".to_string()
                                }
                            }
                            other => format!("model.cache.resolved.{other}"),
                        };
                        member_loss(&mut loss, &detail);
                    }
                }
            }
            other => loss.push(LossEntry {
                kind: "unrepresentable_event",
                detail: other.to_string(),
            }),
        }
    }
    GenAiExport {
        spans,
        loss,
        seq_range: if events.is_empty() { (0, 0) } else { (lo, hi) },
    }
}
