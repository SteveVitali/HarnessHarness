//! `hh-slot-costs/1` — the per-slot cost fold (manifest 267/R6.14;
//! spec §4.5 REQ-XC-118; design/33 §1.5; AR-16). The `model` member is a
//! recomputable fold over the phase proxy run's durable events filtered
//! by `slot_id` — never a separate counter. `compute` is a reference to
//! the window's `hh-compute-cost/1`; `total` appears only when the
//! currencies match; provenance is never averaged.

use std::collections::BTreeMap;

use hharness_wire::json::Json;

use crate::json_ext::JsonExt;

/// The record schema spelling.
pub const SLOT_COSTS_SCHEMA: &str = "hh-slot-costs/1";

/// Facts the fold needs beyond the event list.
#[derive(Debug, Clone)]
pub struct SlotCostCtx {
    /// The phase proxy run id.
    pub run_id: String,
    /// The slot's slice budget id.
    pub slice_budget_id: String,
    /// provider + lane + model identity.
    pub provider: String,
    /// The lane ref.
    pub lane_ref: String,
    /// The model id.
    pub model_id: String,
    /// The pinned effort applied.
    pub effort_applied: String,
    /// Settlement status + ref (from the phase spend ledger's
    /// `measurement.cost.reconciled` row, supplied records-in).
    pub settlement: Option<(String, String)>,
    /// The `hh-compute-cost/1` window ref + attributed ms + provenance.
    pub compute: Option<ComputeRef>,
    /// The currency (`USD`) — `total` is emitted only when both legs
    /// share it.
    pub currency: String,
    /// Compute-side money µ$ (for `total` when currencies match).
    pub compute_micros: i64,
    /// Compute currency.
    pub compute_currency: String,
}

/// The compute reference member.
#[derive(Debug, Clone)]
pub struct ComputeRef {
    /// `hh-compute-cost/1` address.
    pub ref_: String,
    /// Attributed ms.
    pub attributed_ms: i64,
    /// Provenance (`estimate|billing_export` — verbatim, never averaged).
    pub provenance: String,
}

fn mc_to_slot(events: &[(String, Json)], budget_id: &str) -> Vec<String> {
    let mut mcs = Vec::new();
    for (c, p) in events {
        if c != "control.budget.reserved" {
            continue;
        }
        if p.get("budget_id").and_then(Json::as_str) != Some(budget_id) {
            continue;
        }
        if let Some(h) = p.get("holder").and_then(Json::as_str) {
            mcs.push(h.strip_prefix("mc:").unwrap_or(h).to_string());
        }
    }
    mcs
}

/// `fold_slot_costs(events, slot_id, ctx)` — the `hh-slot-costs/1`
/// document for one slot. `events` are the phase proxy run's
/// `(class, payload)` rows in order.
pub fn fold_slot_costs(events: &[(String, Json)], slot_id: &str, ctx: &SlotCostCtx) -> Json {
    let mcs = mc_to_slot(events, &ctx.slice_budget_id);
    let in_slot = |p: &Json| {
        p.get("model_call_id")
            .and_then(Json::as_str)
            .map(|m| mcs.iter().any(|x| x == m))
            .unwrap_or(false)
    };
    let mut dispatched = 0i64;
    let mut completed = 0i64;
    let mut upstream_failed = 0i64;
    let mut proxy_refused = 0i64;
    let mut in_total = 0i64;
    let mut in_uncached = 0i64;
    let mut cache_read = 0i64;
    let mut cache_avail = false;
    let mut out_total = 0i64;
    let mut out_reasoning = 0i64;
    let mut out_reasoning_avail = false;
    let mut spend_micros = 0i64;
    let mut provs: Vec<String> = Vec::new();
    let mut conf = "estimate";
    let mut coverage = 0i64;
    let mut pricing_ref = Json::Null;
    let mut peak_reserved = 0i64;
    let mut refused_count = 0i64;
    let mut first_seq = None;
    let mut last_seq = None;
    for (i, (c, p)) in events.iter().enumerate() {
        match c.as_str() {
            "model.call.requested" if in_slot(p) => {
                dispatched += 1;
                if first_seq.is_none() {
                    first_seq = Some(i as i64);
                }
                last_seq = Some(i as i64);
            }
            "model.call.completed" if in_slot(p) => {
                completed += 1;
                last_seq = Some(i as i64);
                if let Some(u) = p.get("usage").and_then(Json::as_obj) {
                    if let Some(v) = u.get("input_total").and_then(Json::as_int) {
                        in_total += v;
                    }
                    if let Some(v) = u.get("input_uncached").and_then(Json::as_int) {
                        in_uncached += v;
                    }
                    match u.get("cache_read") {
                        Some(Json::Int(v)) => {
                            cache_read += v;
                            cache_avail = true;
                        }
                        _ => {}
                    }
                    if let Some(v) = u.get("output_total").and_then(Json::as_int) {
                        out_total += v;
                    }
                    match u.get("output_reasoning") {
                        Some(Json::Int(v)) => {
                            out_reasoning += v;
                            out_reasoning_avail = true;
                        }
                        _ => {}
                    }
                }
            }
            "model.call.failed" if in_slot(p) => {
                last_seq = Some(i as i64);
                match p.get("disposition").and_then(Json::as_str).unwrap_or("") {
                    d if d.starts_with("refused") => proxy_refused += 1,
                    _ => upstream_failed += 1,
                }
            }
            "measurement.cost.attributed" => {
                let att = p.get("attribution").and_then(Json::as_obj);
                let is_slot = att
                    .and_then(|a| a.get("participant_ref"))
                    .and_then(Json::as_str)
                    == Some(slot_id);
                if is_slot {
                    spend_micros += p
                        .get("money")
                        .and_then(|m| m.get("micro_units"))
                        .and_then(Json::as_int)
                        .unwrap_or(0);
                    if let Some(pv) = p.get("provenance").and_then(Json::as_str) {
                        provs.push(pv.to_string());
                    }
                    if let Some(cv) = p.get("confidence").and_then(Json::as_str) {
                        conf = cv;
                    }
                    coverage = p.get("coverage").and_then(Json::as_int).unwrap_or(coverage);
                    if let Some(r) = p.get("pricing_ref") {
                        pricing_ref = r.clone();
                    }
                }
            }
            "control.budget.reserved" => {
                if p.get("budget_id").and_then(Json::as_str) == Some(ctx.slice_budget_id.as_str()) {
                    let q = p
                        .get("quantity")
                        .and_then(|q| q.get("spend"))
                        .and_then(Json::as_int)
                        .unwrap_or(0);
                    peak_reserved = peak_reserved.max(q);
                    if p.get("outcome").and_then(Json::as_str) == Some("refused") {
                        refused_count += 1;
                    }
                }
            }
            _ => {}
        }
    }
    let provenance = {
        let mut u = provs.clone();
        u.sort();
        u.dedup();
        match u.len() {
            0 => "unknown".to_string(),
            1 => u[0].clone(),
            _ => "mixed".to_string(),
        }
    };
    let mut settle = BTreeMap::new();
    if let Some((st, r)) = &ctx.settlement {
        settle.insert("status".into(), Json::str(st));
        settle.insert("ref".into(), Json::str(r));
    } else {
        settle.insert("status".into(), Json::str("pending"));
        settle.insert("ref".into(), Json::Null);
    }
    let model = Json::obj([
        ("provider", Json::str(&ctx.provider)),
        ("lane_ref", Json::str(&ctx.lane_ref)),
        ("model_id", Json::str(&ctx.model_id)),
        ("effort_applied", Json::str(&ctx.effort_applied)),
        (
            "calls",
            Json::obj([
                ("dispatched", Json::Int(dispatched)),
                ("completed", Json::Int(completed)),
                ("upstream_failed", Json::Int(upstream_failed)),
                ("proxy_refused", Json::Int(proxy_refused)),
            ]),
        ),
        (
            "tokens",
            Json::obj([
                ("input_total", Json::Int(in_total)),
                ("input_uncached", Json::Int(in_uncached)),
                ("cache_read", Json::Int(cache_read)),
                ("cache_read_available", Json::Bool(cache_avail)),
                ("output_total", Json::Int(out_total)),
                ("output_reasoning", Json::Int(out_reasoning)),
                (
                    "output_reasoning_available",
                    Json::Bool(out_reasoning_avail),
                ),
            ]),
        ),
        (
            "spend",
            Json::obj([
                (
                    "money",
                    Json::obj([
                        ("micro_units", Json::Int(spend_micros)),
                        ("currency", Json::str(&ctx.currency)),
                    ]),
                ),
                ("provenance", Json::str(&provenance)),
                ("confidence", Json::str(conf)),
                ("coverage_ppm", Json::Int(coverage)),
                ("pricing_ref", pricing_ref),
            ]),
        ),
        (
            "reservation",
            Json::obj([
                ("peak_reserved_micros", Json::Int(peak_reserved)),
                ("refused_count", Json::Int(refused_count)),
            ]),
        ),
        (
            "phase_run",
            Json::obj([
                ("run_id", Json::str(&ctx.run_id)),
                ("slice_budget_id", Json::str(&ctx.slice_budget_id)),
                ("first_seq", first_seq.map(Json::Int).unwrap_or(Json::Null)),
                ("last_seq", last_seq.map(Json::Int).unwrap_or(Json::Null)),
            ]),
        ),
        ("settlement", Json::Obj(settle)),
    ]);
    let mut doc = vec![("schema", Json::str(SLOT_COSTS_SCHEMA)), ("model", model)];
    let mut compute_micros = None;
    if let Some(cr) = &ctx.compute {
        doc.push((
            "compute",
            Json::obj([
                ("ref", Json::str(&cr.ref_)),
                ("attributed_ms", Json::Int(cr.attributed_ms)),
                ("provenance", Json::str(&cr.provenance)),
            ]),
        ));
        compute_micros = Some(ctx.compute_micros);
    }
    // `total` only when currencies match — provenance never averaged.
    if ctx.currency == ctx.compute_currency {
        if let Some(cm) = compute_micros {
            doc.push((
                "total",
                Json::obj([
                    (
                        "money",
                        Json::obj([
                            ("micro_units", Json::Int(spend_micros + cm)),
                            ("currency", Json::str(&ctx.currency)),
                        ]),
                    ),
                    (
                        "composed_of",
                        Json::Arr(vec![Json::str("model"), Json::str("compute")]),
                    ),
                    (
                        "note",
                        Json::str("model fold + compute record; provenance not averaged"),
                    ),
                ]),
            ));
        }
    }
    Json::obj(doc)
}

/// `validate_slot_costs(doc)` — closed-codec check.
pub fn validate_slot_costs(doc: &Json) -> Result<(), String> {
    let m = doc
        .as_obj()
        .ok_or_else(|| "record must be an object".to_string())?;
    for k in m.keys() {
        if !["schema", "model", "compute", "total"].contains(&k.as_str()) {
            return Err(format!("unknown member {k}"));
        }
    }
    if m.get("schema").and_then(Json::as_str) != Some(SLOT_COSTS_SCHEMA) {
        return Err("schema".into());
    }
    if m.get("model").and_then(Json::as_obj).is_none() {
        return Err("model".into());
    }
    if let Some(t) = m.get("total") {
        let tm = t.as_obj().ok_or("total")?;
        let mm = m["model"].as_obj().ok_or("model")?;
        let cur1 = mm
            .get("spend")
            .and_then(|s| s.get("money"))
            .and_then(|x| x.get("currency"))
            .and_then(Json::as_str);
        let cur2 = tm
            .get("money")
            .and_then(|x| x.get("currency"))
            .and_then(Json::as_str);
        if cur1 != cur2 {
            return Err("total currencies must match".into());
        }
    }
    Ok(())
}
