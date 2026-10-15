//! The convention-span lift-back (DF-S1.14-2; AC-R-2.9.1's sink round-trip):
//! a lowered `gen_ai.*`/OpenInference span resolves back to the durable
//! `EventRef` it was lowered from — the "two convention-shaped sinks with
//! the lift-back" cell.
//!
//! The mechanism is the `hh.*` extension attributes the liftable lowerings
//! stamp (`hh.run_id`, `hh.event_id` — the same namespaced-extension
//! convention the `hh` tracestate member uses, §5h.1 §2.3): context, never
//! authority — the lift resolves the coordinate, and any reader confirms
//! it against the ledger itself. A span that doesn't carry both members
//! lifts `None` (absent, never fabricated — a foreign convention span is
//! not a HarnessHarness event).

use hh_ledger::manifest::EventRef;
use hh_wire::json::Json;

/// The extension attribute spellings the liftable lowerings stamp.
pub mod hh_attr {
    /// `hh.run_id` — the durable run the span was lowered from.
    pub const RUN_ID: &str = "hh.run_id";
    /// `hh.event_id` — the durable event the span was lowered from.
    pub const EVENT_ID: &str = "hh.event_id";
}

/// `lift_span(span) -> Option<EventRef>` — resolve a convention span to
/// its source event. Reads `attributes.{hh.run_id, hh.event_id}` (the
/// lowered spellings); a span missing either member lifts `None` — the
/// same honesty rule as `inbound_context`: a foreign coordinate is never
/// grafted onto a ledger event it doesn't name.
pub fn lift_span(span: &Json) -> Option<EventRef> {
    let attrs = span.get("attributes")?;
    let run_id = attrs.get(hh_attr::RUN_ID)?.as_str()?.to_string();
    let event_id = attrs.get(hh_attr::EVENT_ID)?.as_str()?.to_string();
    Some(EventRef { run_id, event_id })
}

/// `stamp_liftable(span, run_id, event_id)` — the shared stamper both
/// convention lowerings call after building a span: the `hh.*` attributes
/// join `attributes{}` (created if the lowering emitted none).
pub fn stamp_liftable(span: &mut Json, run_id: &str, event_id: &str) {
    let Json::Obj(m) = span else { return };
    let mut attrs = match m.get("attributes") {
        Some(Json::Obj(a)) => a.clone(),
        _ => std::collections::BTreeMap::new(),
    };
    attrs.insert(hh_attr::RUN_ID.to_string(), Json::str(run_id));
    attrs.insert(hh_attr::EVENT_ID.to_string(), Json::str(event_id));
    m.insert("attributes".to_string(), Json::Obj(attrs));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn lift_span_round_trips_hh_attributes() {
        let mut span = Json::Obj(BTreeMap::new());
        stamp_liftable(&mut span, "run-9", "event-42");
        let r = lift_span(&span).expect("liftable");
        assert_eq!(r.run_id, "run-9");
        assert_eq!(r.event_id, "event-42");
    }

    #[test]
    fn foreign_span_lifts_none() {
        // A span with convention attributes but no hh.* members — absent,
        // never fabricated.
        let span = Json::obj([(
            "attributes",
            Json::obj([("gen_ai.operation.name", Json::str("chat"))]),
        )]);
        assert_eq!(lift_span(&span), None);
        assert_eq!(lift_span(&Json::Null), None);
    }
}
