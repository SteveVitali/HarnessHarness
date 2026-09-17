//! The §5h.1 §2.2 measurement-emission payload builders + strict decoders.
//! These are ledger payloads produced by the named components — never a
//! telemetry write path (ADR-0042 D2). `measurement.cost.attributed` payloads
//! are `hh_budget`'s (`events::cost_attributed_payload`) — reused, never
//! redefined (CC7).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_wire::json::Json;

use crate::codec::{arr_at, expect_obj, int_at, reject_unknown, str_at};
use crate::errors::CodecError;
use crate::sinks::ContentClass;

/// `Timing{request_sent_ms, first_byte_ms?, first_token_ms?, last_byte_ms}` —
/// the M4 per-attempt monotonic-clock stamps (`emit_call_timing`, §5h.1 §2.2).
/// Carried on `model.call.attempt.{started,completed,failed}` payloads under
/// `timing`; the member values are *offsets* in ms on the measuring
/// component's monotonic clock, so `last_byte_ms − request_sent_ms` is the
/// attempt duration (the wall `ts` never enters the arithmetic — AC-R-2.9.1-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// Request sent (monotonic ms).
    pub request_sent_ms: i64,
    /// First response byte, when observed.
    pub first_byte_ms: Option<i64>,
    /// First token, when observed.
    pub first_token_ms: Option<i64>,
    /// Last byte (monotonic ms).
    pub last_byte_ms: i64,
}

impl Timing {
    /// The attempt duration `last_byte − request_sent` (monotonic — the only
    /// duration arithmetic `trace_view` trusts for M4).
    pub fn attempt_ms(&self) -> i64 {
        self.last_byte_ms - self.request_sent_ms
    }

    /// `first_token − request_sent` — `ttft`, when observed.
    pub fn ttft_ms(&self) -> Option<i64> {
        self.first_token_ms.map(|t| t - self.request_sent_ms)
    }

    /// `first_byte − request_sent` — `ttfm`, when observed.
    pub fn ttfm_ms(&self) -> Option<i64> {
        self.first_byte_ms.map(|t| t - self.request_sent_ms)
    }

    /// The canonical JSON form (absent members omitted).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("request_sent_ms".into(), Json::Int(self.request_sent_ms));
        if let Some(b) = self.first_byte_ms {
            m.insert("first_byte_ms".into(), Json::Int(b));
        }
        if let Some(t) = self.first_token_ms {
            m.insert("first_token_ms".into(), Json::Int(t));
        }
        m.insert("last_byte_ms".into(), Json::Int(self.last_byte_ms));
        Json::Obj(m)
    }

    /// Strict decode — `BadMember` on unknown members.
    pub fn from_json(j: &Json) -> Result<Timing, CodecError> {
        let m = expect_obj(j, "Timing")?;
        reject_unknown(
            m,
            &[
                "request_sent_ms",
                "first_byte_ms",
                "first_token_ms",
                "last_byte_ms",
            ],
            "Timing",
        )?;
        Ok(Timing {
            request_sent_ms: int_at(m, "request_sent_ms", "Timing")?,
            first_byte_ms: crate::codec::opt_int_at(m, "first_byte_ms")?,
            first_token_ms: crate::codec::opt_int_at(m, "first_token_ms")?,
            last_byte_ms: int_at(m, "last_byte_ms", "Timing")?,
        })
    }
}

/// `measurement.export.delivered{sink_id, view_kind, seq_range{from_seq,
/// to_seq}, content_classes[], loss_report_ref}` (§5h.1 §2.2 — the only ledger
/// append an exporter may make; kernel-appended, provenance-mandatory,
/// content-free ⇒ audit-grade).
#[derive(Debug, Clone, PartialEq)]
pub struct ExportDelivered {
    /// The sink the delivery went to.
    pub sink_id: String,
    /// The exported view kind (`trace_view`/`metric_view`/…).
    pub view_kind: String,
    /// The durable seq range the delivery covered.
    pub seq_range: (u64, u64),
    /// The content classes the delivery carried.
    pub content_classes: BTreeSet<ContentClass>,
    /// The `TelemetryLossReport` blob ref (a `sha256:` id) — absent only when
    /// the lowering recorded zero loss.
    pub loss_report_ref: Option<String>,
}

/// The `measurement.export.delivered` payload.
pub fn export_delivered_payload(d: &ExportDelivered) -> Json {
    let mut m = BTreeMap::new();
    m.insert("sink_id".into(), Json::str(&d.sink_id));
    m.insert("view_kind".into(), Json::str(&d.view_kind));
    m.insert(
        "seq_range".into(),
        Json::obj([
            ("from_seq", Json::Int(d.seq_range.0 as i64)),
            ("to_seq", Json::Int(d.seq_range.1 as i64)),
        ]),
    );
    m.insert(
        "content_classes".into(),
        Json::Arr(
            d.content_classes
                .iter()
                .map(|c| Json::str(c.as_str()))
                .collect(),
        ),
    );
    if let Some(r) = &d.loss_report_ref {
        m.insert("loss_report_ref".into(), Json::str(r));
    }
    Json::Obj(m)
}

/// Strict decode of the `measurement.export.delivered` payload — `BadMember`
/// on unknown members.
pub fn export_delivered_from_json(j: &Json) -> Result<ExportDelivered, CodecError> {
    const REC: &str = "measurement.export.delivered";
    let m = expect_obj(j, REC)?;
    reject_unknown(
        m,
        &[
            "sink_id",
            "view_kind",
            "seq_range",
            "content_classes",
            "loss_report_ref",
        ],
        REC,
    )?;
    let sr = expect_obj(
        m.get("seq_range").ok_or(CodecError::MissingMember {
            member: "seq_range",
            record: REC,
        })?,
        REC,
    )?;
    reject_unknown(sr, &["from_seq", "to_seq"], REC)?;
    let mut classes = BTreeSet::new();
    for c in arr_at(m, "content_classes", REC)? {
        let s = c.as_str().ok_or_else(|| CodecError::TypeMismatch {
            member: "content_classes[]".to_string(),
            expected: "string",
        })?;
        classes.insert(
            ContentClass::parse(s).map_err(|_| CodecError::TypeMismatch {
                member: format!("content_classes[{s}]"),
                expected: "accounting|structural|content|diagnostic",
            })?,
        );
    }
    Ok(ExportDelivered {
        sink_id: str_at(m, "sink_id", REC)?.to_string(),
        view_kind: str_at(m, "view_kind", REC)?.to_string(),
        seq_range: (
            int_at(sr, "from_seq", REC)? as u64,
            int_at(sr, "to_seq", REC)? as u64,
        ),
        content_classes: classes,
        loss_report_ref: crate::codec::opt_str_at(m, "loss_report_ref")?.map(str::to_string),
    })
}

/// `measurement.metric.emitted{subject, metric_ref, value, unit, detector_ref}`
/// (§5h.1 §2.2 — restricted to observations **not derivable** from other
/// events; derived metrics are `metric_view` values, never emitted rows).
pub fn metric_emitted_payload(
    subject: &str,
    metric_ref: &str,
    value: &Json,
    unit: &str,
    detector_ref: &str,
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("subject".into(), Json::str(subject));
    m.insert("metric_ref".into(), Json::str(metric_ref));
    m.insert("value".into(), value.clone());
    m.insert("unit".into(), Json::str(unit));
    m.insert("detector_ref".into(), Json::str(detector_ref));
    Json::Obj(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_delivered_round_trips_strictly() {
        let d = ExportDelivered {
            sink_id: "sink:acct".into(),
            view_kind: "trace_view".into(),
            seq_range: (2, 17),
            content_classes: [ContentClass::Accounting].into_iter().collect(),
            loss_report_ref: Some("sha256:abc".into()),
        };
        let j = export_delivered_payload(&d);
        assert_eq!(export_delivered_from_json(&j).unwrap(), d);
        let mut m = match j {
            Json::Obj(m) => m,
            _ => unreachable!(),
        };
        m.insert("bogus".into(), Json::Null);
        assert!(matches!(
            export_delivered_from_json(&Json::Obj(m)),
            Err(CodecError::BadMember { .. })
        ));
    }

    #[test]
    fn timing_carries_the_monotonic_stamps() {
        let t = Timing {
            request_sent_ms: 1_000,
            first_byte_ms: Some(1_120),
            first_token_ms: Some(1_150),
            last_byte_ms: 1_430,
        };
        assert_eq!(t.attempt_ms(), 430);
        assert_eq!(t.ttft_ms(), Some(150));
        assert_eq!(t.ttfm_ms(), Some(120));
        assert_eq!(Timing::from_json(&t.to_json()).unwrap(), t);
    }
}
