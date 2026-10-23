//! The OTLP/HTTP (JSON) wire form (R4.7c; ADR-0346 revisit trigger (a);
//! BL-31's live-sink member): the lowered convention spans ride an OTLP
//! `ExportTraceServiceRequest` document through the egress-mediated sink —
//! the subscriber's one `net_egress` request is unchanged, only the body
//! shape widens from the batch `Json::Arr` to the collector's native
//! dialect.
//!
//! The lowering is honest loss like every convention projection
//! (ADR-0042): a span without its `hh.run_id`/`hh.event_id` lift
//! coordinates lands as a named `unrepresentable_span` loss — identity is
//! never fabricated; an unparseable anchor `ts` is a loss, never a
//! guessed instant ([`crate::clocks::ts_ms`]'s rule). Trace/span identity
//! derives through [`crate::propagation`]'s provisional derivation
//! (`trace_id = H(run_id)[0:16]`, `span_id = H(event_id)[0:8]`) —
//! ADR-0346 trigger (b) stays as written: the carrier rendering and the
//! `hh` member spelling are untouched.
//!
//! The emitted document follows the OTLP/JSON mapping: `traceId`/`spanId`
//! are hex strings, `startTimeUnixNano`/`endTimeUnixNano` and `intValue`
//! are decimal strings (the protobuf-JSON 64-bit spelling), attribute
//! values lower to `AnyValue` (`stringValue`/`intValue`/`boolValue`/
//! `arrayValue`/`kvlistValue`; a `null` member is a named loss, never an
//! empty value).

use std::collections::BTreeMap;

use hharness_wire::json::Json;

use crate::clocks::ts_ms;
use crate::export::LossEntry;
use crate::lift::hh_attr;
use crate::propagation::{span_id, trace_id};

/// OTLP `SpanKind` — the closed proto enum.
mod kind {
    /// `SPAN_KIND_INTERNAL` — the default for spans whose convention
    /// names no client leg.
    pub const INTERNAL: i64 = 1;
    /// `SPAN_KIND_CLIENT` — a call to a remote service (`gen_ai.chat`,
    /// `llm.chat` — the model-call spans both conventions emit).
    pub const CLIENT: i64 = 3;
}

/// `STATUS_CODE_ERROR` — the span carries an error-class attribute.
const STATUS_ERROR: i64 = 2;

/// `any_value(v)` — one `Json` member → the OTLP `AnyValue` spelling.
/// `Null` has no `AnyValue` form — `None` (the caller names the loss),
/// never an empty value.
fn any_value(v: &Json, path: &str, loss: &mut Vec<LossEntry>) -> Option<Json> {
    match v {
        Json::Str(s) => Some(Json::obj([("stringValue", Json::str(s.clone()))])),
        Json::Int(i) => Some(Json::obj([("intValue", Json::str(i.to_string()))])),
        Json::Bool(b) => Some(Json::obj([("boolValue", Json::Bool(*b))])),
        Json::Arr(items) => Some(Json::obj([(
            "arrayValue",
            Json::obj([(
                "values",
                Json::Arr(
                    items
                        .iter()
                        .enumerate()
                        .filter_map(|(i, v)| any_value(v, &format!("{path}[{i}]"), loss))
                        .collect(),
                ),
            )]),
        )])),
        Json::Obj(m) => Some(Json::obj([(
            "kvlistValue",
            Json::obj([(
                "values",
                Json::Arr(
                    m.iter()
                        .filter_map(|(k, v)| {
                            any_value(v, &format!("{path}.{k}"), loss).map(|value| {
                                Json::obj([("key", Json::str(k.clone())), ("value", value)])
                            })
                        })
                        .collect(),
                ),
            )]),
        )])),
        Json::Null => {
            loss.push(LossEntry {
                kind: "unrepresentable_attribute",
                detail: path.to_string(),
            });
            None
        }
    }
}

/// `kv(key, v)` — one `KeyValue{key, value}` for `attributes[]`.
fn key_value(key: &str, v: &Json, path: &str, loss: &mut Vec<LossEntry>) -> Option<Json> {
    any_value(v, &format!("{path}.{key}"), loss)
        .map(|value| Json::obj([("key", Json::str(key)), ("value", value)]))
}

/// `otlp_span(span, ts)` — one convention-lowered span
/// (`{name, seq, span_id?, duration_ms?, attributes{hh.run_id,
/// hh.event_id, …}}`) → one OTLP span, or a named loss when the `hh.*`
/// lift coordinates or the anchor `ts` are absent/malformed.
///
/// `ts` is the source event's durable ordering stamp — it anchors
/// `startTimeUnixNano` and `endTimeUnixNano = start + duration_ms`
/// (the *measured* member, never a `ts` difference — §2.6 holds: the
/// OTLP wire spells the same stamps the ledger carries). A span with no
/// `duration_ms` is an honest instant (`end == start`).
fn otlp_span(span: &Json, ts: &str, loss: &mut Vec<LossEntry>) -> Option<Json> {
    let name = span.get("name").and_then(Json::as_str).unwrap_or("span");
    let tag = format!(
        "{name}@seq{}",
        span.get("seq").and_then(Json::as_int).unwrap_or(0)
    );
    let attrs = span.get("attributes");
    let (run_id, event_id) = (
        attrs
            .and_then(|a| a.get(hh_attr::RUN_ID))
            .and_then(Json::as_str),
        attrs
            .and_then(|a| a.get(hh_attr::EVENT_ID))
            .and_then(Json::as_str),
    );
    let (Some(run_id), Some(event_id)) = (run_id, event_id) else {
        loss.push(LossEntry {
            kind: "unrepresentable_span",
            detail: format!("{tag}: missing hh.run_id|hh.event_id"),
        });
        return None;
    };
    let Some(start_ms) = ts_ms(ts) else {
        loss.push(LossEntry {
            kind: "unrepresentable_span",
            detail: format!("{tag}: unparseable anchor ts"),
        });
        return None;
    };
    let start_ns = start_ms.saturating_mul(1_000_000);
    let end_ns = span
        .get("duration_ms")
        .and_then(Json::as_int)
        .map(|d| start_ns.saturating_add(d.saturating_mul(1_000_000)))
        .unwrap_or(start_ns);
    let mut s = BTreeMap::new();
    s.insert("traceId".into(), Json::str(trace_id(run_id)));
    s.insert("spanId".into(), Json::str(span_id(event_id)));
    s.insert("name".into(), Json::str(name));
    s.insert(
        "kind".into(),
        Json::Int(match name {
            "gen_ai.chat" | "llm.chat" => kind::CLIENT,
            _ => kind::INTERNAL,
        }),
    );
    s.insert("startTimeUnixNano".into(), Json::str(start_ns.to_string()));
    s.insert("endTimeUnixNano".into(), Json::str(end_ns.to_string()));
    // The attribute leg — every convention member lowers (the `hh.*`
    // lift carriers ride as attributes: the collector's copy of the
    // span names its ledger coordinates). The span's `seq` and its own
    // `span_id` member (the model-call id) are not OTLP span fields —
    // they lower as `hh.*` attributes so nothing is silently dropped.
    let mut out: Vec<Json> = Vec::new();
    if let Some(Json::Obj(m)) = attrs {
        for (k, v) in m {
            if let Some(kv) = key_value(k, v, &tag, loss) {
                out.push(kv);
            }
        }
    }
    for (k, v) in [
        ("hh.seq", span.get("seq").cloned().unwrap_or(Json::Null)),
        (
            "hh.span_id",
            span.get("span_id").cloned().unwrap_or(Json::Null),
        ),
    ] {
        if v != Json::Null {
            if let Some(kv) = key_value(k, &v, &tag, loss) {
                out.push(kv);
            }
        }
    }
    s.insert("attributes".into(), Json::Arr(out));
    // `status` — an error-class attribute marks `STATUS_CODE_ERROR`;
    // a clean span leaves `status` absent (`STATUS_CODE_UNSET`).
    if let Some(Json::Obj(m)) = attrs {
        if m.keys()
            .any(|k| k == "gen_ai.error.type" || k == "error.type")
        {
            s.insert(
                "status".into(),
                Json::obj([("code", Json::Int(STATUS_ERROR))]),
            );
        }
    }
    Some(Json::Obj(s))
}

/// `trace_service_request(spans, service_name)` — fold the
/// convention-lowered spans into the OTLP `ExportTraceServiceRequest`
/// document the sink POSTs. `spans` is `(span, anchor_ts)` pairs — the
/// span's `seq` names its source event's position in the durable prefix.
/// One `resourceSpans` entry (the exporter's `service.name` resource),
/// one `scopeSpans` entry (the `hh.telemetry` instrumentation scope).
/// The lowering loss returns beside the document — it rides the
/// delivery row's `loss_report_ref` exactly like a batch export's.
pub fn trace_service_request(
    spans: &[(Json, String)],
    service_name: &str,
) -> (Json, Vec<LossEntry>) {
    let mut loss = Vec::new();
    let otlp_spans: Vec<Json> = spans
        .iter()
        .filter_map(|(s, ts)| otlp_span(s, ts, &mut loss))
        .collect();
    let body = Json::obj([(
        "resourceSpans",
        Json::Arr(vec![Json::obj([
            (
                "resource",
                Json::obj([(
                    "attributes",
                    Json::Arr(vec![Json::obj([
                        ("key", Json::str("service.name")),
                        (
                            "value",
                            Json::obj([("stringValue", Json::str(service_name))]),
                        ),
                    ])]),
                )]),
            ),
            (
                "scopeSpans",
                Json::Arr(vec![Json::obj([
                    (
                        "scope",
                        Json::obj([
                            ("name", Json::str("hh.telemetry")),
                            ("version", Json::str("1")),
                        ]),
                    ),
                    ("spans", Json::Arr(otlp_spans)),
                ])]),
            ),
        ])]),
    )]);
    (body, loss)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lift::stamp_liftable;

    fn get<'a>(j: &'a Json, path: &str) -> Option<&'a Json> {
        let mut cur = j;
        for seg in path.split('.') {
            cur = match cur {
                Json::Obj(_) => cur.get(seg)?,
                Json::Arr(items) => items.get(seg.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        Some(cur)
    }

    fn span(name: &str, seq: u64) -> Json {
        let mut s = Json::obj([
            ("name", Json::str(name)),
            ("seq", Json::Int(seq as i64)),
            ("span_id", Json::str("mc-1")),
            ("duration_ms", Json::Int(321)),
            (
                "attributes",
                Json::obj([
                    ("gen_ai.operation.name", Json::str("chat")),
                    ("gen_ai.usage.input_tokens", Json::Int(100)),
                    (
                        "gen_ai.response.finish_reasons",
                        Json::Arr(vec![Json::str("stop")]),
                    ),
                ]),
            ),
        ]);
        stamp_liftable(&mut s, "run-1", "event-7");
        s
    }

    /// `attr(body, key)` — the span's `attributes[]` member `key`.
    fn attr<'a>(span: &'a Json, key: &str) -> Option<&'a Json> {
        let Json::Arr(attrs) = span.get("attributes")? else {
            return None;
        };
        attrs
            .iter()
            .find(|kv| kv.get("key").and_then(Json::as_str) == Some(key))
            .and_then(|kv| kv.get("value"))
    }

    #[test]
    fn otlp_span_derives_w3c_ids_and_anyvalue_attributes() {
        let (body, loss) = trace_service_request(
            &[(
                span("gen_ai.chat", 7),
                "2026-01-01T00:00:00.000Z".to_string(),
            )],
            "hh.exporter",
        );
        assert!(loss.is_empty(), "{loss:?}");
        let spans = get(&body, "resourceSpans.0.scopeSpans.0.spans").unwrap();
        let Json::Arr(spans) = spans else {
            panic!("spans[]")
        };
        assert_eq!(spans.len(), 1);
        let s = &spans[0];
        assert_eq!(
            s.get("traceId").and_then(Json::as_str),
            Some(trace_id("run-1").as_str())
        );
        assert_eq!(
            s.get("spanId").and_then(Json::as_str),
            Some(span_id("event-7").as_str())
        );
        assert_eq!(s.get("name").and_then(Json::as_str), Some("gen_ai.chat"));
        assert_eq!(s.get("kind").and_then(Json::as_int), Some(3)); // CLIENT
                                                                   // Anchored at the event ts; end = start + measured 321 ms.
        assert_eq!(
            s.get("startTimeUnixNano").and_then(Json::as_str),
            Some("1767225600000000000")
        );
        assert_eq!(
            s.get("endTimeUnixNano").and_then(Json::as_str),
            Some("1767225600321000000")
        );
        // The convention members lower to the AnyValue spellings.
        assert_eq!(
            attr(s, "gen_ai.usage.input_tokens")
                .and_then(|v| v.get("intValue"))
                .and_then(Json::as_str),
            Some("100")
        );
        assert_eq!(
            attr(s, "gen_ai.response.finish_reasons")
                .and_then(|v| v.get("arrayValue"))
                .and_then(|v| v.get("values"))
                .and_then(|v| match v {
                    Json::Arr(a) => a.first(),
                    _ => None,
                })
                .and_then(|v| v.get("stringValue"))
                .and_then(Json::as_str),
            Some("stop")
        );
        assert_eq!(
            attr(s, "hh.run_id")
                .and_then(|v| v.get("stringValue"))
                .and_then(Json::as_str),
            Some("run-1")
        );
        assert_eq!(
            attr(s, "hh.event_id")
                .and_then(|v| v.get("stringValue"))
                .and_then(Json::as_str),
            Some("event-7")
        );
        // seq + the convention span_id lower as hh.* attributes — no
        // silent drops.
        assert!(attr(s, "hh.seq").is_some());
        assert_eq!(
            attr(s, "hh.span_id")
                .and_then(|v| v.get("stringValue"))
                .and_then(Json::as_str),
            Some("mc-1")
        );
        assert!(s.get("status").is_none(), "clean span → UNSET absent");
        // The resource carries the declared service.name.
        assert_eq!(
            get(
                &body,
                "resourceSpans.0.resource.attributes.0.value.stringValue"
            )
            .and_then(Json::as_str),
            Some("hh.exporter")
        );
    }

    #[test]
    fn missing_lift_coordinates_are_loss_never_fabricated() {
        let foreign = Json::obj([("name", Json::str("gen_ai.chat")), ("seq", Json::Int(1))]);
        let (body, loss) = trace_service_request(
            &[(foreign, "2026-01-01T00:00:00.000Z".to_string())],
            "hh.exporter",
        );
        assert!(loss
            .iter()
            .any(|l| l.detail.contains("missing hh.run_id|hh.event_id")));
        let spans = get(&body, "resourceSpans.0.scopeSpans.0.spans").unwrap();
        assert!(
            matches!(spans, Json::Arr(a) if a.is_empty()),
            "a foreign span is never exported"
        );
    }

    #[test]
    fn malformed_ts_is_loss_and_error_attr_marks_status() {
        let (body, loss) = trace_service_request(
            &[(span("gen_ai.chat", 7), "not-a-ts".to_string())],
            "hh.exporter",
        );
        assert!(loss.iter().any(|l| l.detail.contains("anchor ts")));
        let spans = get(&body, "resourceSpans.0.scopeSpans.0.spans").unwrap();
        assert!(matches!(spans, Json::Arr(a) if a.is_empty()));
        // A failed-call span carries the error class → STATUS_CODE_ERROR.
        let mut failed = span("gen_ai.chat", 8);
        let Json::Obj(m) = &mut failed else {
            unreachable!()
        };
        let Json::Obj(a) = m.get_mut("attributes").unwrap_or_else(|| unreachable!()) else {
            unreachable!()
        };
        a.insert("gen_ai.error.type".into(), Json::str("overloaded"));
        let (body, _) = trace_service_request(
            &[(failed, "2026-01-01T00:00:00.000Z".to_string())],
            "hh.exporter",
        );
        assert_eq!(
            get(&body, "resourceSpans.0.scopeSpans.0.spans.0.status.code").and_then(Json::as_int),
            Some(STATUS_ERROR)
        );
    }
}
