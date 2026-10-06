//! The V1–V11 view catalogue (§7.2 §4) — every browser "view" is a
//! named assembly over canonical `hh-embed/1` op results. A view
//! function's entire job is: call the canonical ops, label the result
//! `{view: "<kind>", …}` — the rendering half lives in `ui.rs`'s
//! JavaScript; nothing here recomputes a kernel/Lab number (R-D rules:
//! views render canonical records, intervals come rendered, intervals
//! never invert to scalars, pending ≠ expired, held ≠ skipped).
//!
//! View id ↔ §7.2 row:
//! - `v1_runs` — V1 run index (the multi-run home; the auditor's entry).
//! - `v2_scorecard` — V2 scorecard presentation (`lab.eval.render_scorecard`
//!   verbatim — the renderer owns cells/intervals; V6's diff view is
//!   `v6_comparison`).
//! - `v3_context` — V3 context/provenance scrub view (context_view +
//!   the `context.assembled` hash + policy receipt).
//! - `v5_monitor` — V5 live monitor (run_summary + checkpoint + account
//!   + head + the event tail the browser polls).
//! - `v6_comparison` — V6 scorecard/comparison (`lab.leaderboard.*`).
//! - `v7_traversal` — V7 audit traversal (trace/cost/effect ledgers +
//!   the permission partition, expanded on demand).
//! - `v8_inbox` — V8 approval inbox (pending − decided set difference
//!   over the durable `security.permission.*` rows — presentation
//!   selection only; the offered options come verbatim from the
//!   pending row; respond lands through the write path).
//! - `v9_delivery` — V9 session/delivery view (session lifecycle +
//!   `lifecycle.delivery.*` + `note`'s minted records).
//! - `v10_supervision` — V10 fleet/bench supervision (fleet run index +
//!   work-item/debt classes + audit view).
//! - `v11_run` — V11 run detail (run_summary + account + head +
//!   describe + list_leases — the V1 row's expansion).

use std::collections::BTreeMap;

use hh_embed_client_generated::ClientError;
use hh_wire::json::Json;

use crate::session::Sessions;

/// The view ids the API admits (`view` ops are reads — P4's write legs
/// never apply to them).
pub const VIEW_IDS: &[&str] = &[
    "v1_runs",
    "v2_scorecard",
    "v3_context",
    "v5_monitor",
    "v6_comparison",
    "v7_traversal",
    "v8_inbox",
    "v9_delivery",
    "v10_supervision",
    "v11_run",
];

fn obj(j: &Json) -> BTreeMap<String, Json> {
    match j {
        Json::Obj(m) => m.clone(),
        _ => BTreeMap::new(),
    }
}

fn str_of(j: &Json, k: &str) -> Option<String> {
    match obj(j).get(k) {
        Some(Json::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

fn envelope(view: &str, run_id: Option<&str>, members: BTreeMap<String, Json>) -> Json {
    let mut m = BTreeMap::new();
    m.insert("view".into(), Json::str(view));
    if let Some(r) = run_id {
        m.insert("run_id".into(), Json::str(r));
    }
    m.extend(members);
    Json::Obj(m)
}

/// Dispatch a `view.<id>` call. `params` carry browser inputs
/// (`run_id`, op-specific members passed through verbatim).
pub fn view(svc: &mut Sessions, id: &str, params: Json) -> Result<Json, ClientError> {
    let p = obj(&params);
    let run_id = str_of(&params, "run_id");
    match id {
        // V1 — the multi-run index; session-free.
        "v1_runs" => {
            let mut qp = BTreeMap::new();
            if let Some(Json::Obj(f)) = p.get("filter") {
                qp.insert("filter".into(), Json::Obj(f.clone()));
            }
            if let Some(l) = p.get("limit") {
                qp.insert("limit".into(), l.clone());
            }
            let r = svc.call("run_index", Json::Obj(qp))?;
            Ok(envelope("v1_runs", None, obj(&r)))
        }
        // V2 — scorecard rendering; browser params pass through
        // (`scorecard_ref` / `run_ids` / `metrics` — the canonical op
        // owns the param schema).
        "v2_scorecard" => {
            let r = svc.call("lab.eval.render_scorecard", params)?;
            Ok(envelope("v2_scorecard", run_id.as_deref(), obj(&r)))
        }
        // V3 — context scrub: the context_view projection plus the
        // durable `context.assembled` row's hash (read, not recompute).
        "v3_context" => {
            let rid = run_id.ok_or_else(bad_params)?;
            let cv = svc.call_for_run(
                &rid,
                "project",
                Json::obj([("view_kind", Json::str("context_view"))]),
            )?;
            let rows = svc.call_for_run(
                &rid,
                "read",
                Json::obj([
                    ("cursor", Json::obj([("kind", Json::str("now"))])),
                    ("direction", Json::str("rev")),
                    ("limit", Json::Int(200)),
                    (
                        "filter",
                        Json::obj([(
                            "classes",
                            Json::Arr(vec![Json::str("context.assembled"), Json::str("context.")]),
                        )]),
                    ),
                ]),
            )?;
            let mut m = BTreeMap::new();
            m.insert("context_view".into(), cv);
            m.insert("context_rows".into(), rows);
            Ok(envelope("v3_context", Some(&rid), m))
        }
        // V5 — monitor: summary + checkpoint + account + head, and the
        // tail rows newer than `from_seq` when supplied.
        "v5_monitor" => {
            let rid = run_id.ok_or_else(bad_params)?;
            let summary = svc.call_for_run(
                &rid,
                "project",
                Json::obj([("view_kind", Json::str("run_summary"))]),
            )?;
            let checkpoint = svc.call_for_run(
                &rid,
                "project",
                Json::obj([("view_kind", Json::str("checkpoint"))]),
            )?;
            let account = svc.call_for_run(&rid, "account", Json::Obj(BTreeMap::new()))?;
            let head = svc.call_for_run(&rid, "head", Json::Obj(BTreeMap::new()))?;
            let tail = svc.call_for_run(
                &rid,
                "read",
                Json::obj([
                    (
                        "cursor",
                        match p.get("from_seq") {
                            Some(Json::Int(s)) => {
                                Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(*s))])
                            }
                            _ => Json::obj([("kind", Json::str("now"))]),
                        },
                    ),
                    ("direction", Json::str("rev")),
                    ("limit", Json::Int(50)),
                ]),
            )?;
            let mut m = BTreeMap::new();
            m.insert("run_summary".into(), summary);
            m.insert("checkpoint".into(), checkpoint);
            m.insert("account".into(), account);
            m.insert("head".into(), head);
            m.insert("tail".into(), tail);
            Ok(envelope("v5_monitor", Some(&rid), m))
        }
        // V6 — comparison: leaderboard/diff pass-through.
        "v6_comparison" => {
            let op = match str_of(&params, "op").as_deref() {
                Some("diff_snapshots") => "lab.leaderboard.diff_snapshots",
                _ => "lab.leaderboard.leaderboard",
            };
            let mut qp = p.clone();
            qp.remove("op");
            let r = svc.call(op, Json::Obj(qp))?;
            Ok(envelope("v6_comparison", run_id.as_deref(), obj(&r)))
        }
        // V7 — traversal: trace + cost + effect ledgers + the
        // permission partition for the run.
        "v7_traversal" => {
            let rid = run_id.ok_or_else(bad_params)?;
            let trace = svc.call_for_run(
                &rid,
                "project",
                Json::obj([("view_kind", Json::str("trace_view"))]),
            )?;
            let cost = svc.call_for_run(
                &rid,
                "project",
                Json::obj([("view_kind", Json::str("cost_view"))]),
            )?;
            let effects = svc.call_for_run(
                &rid,
                "project",
                Json::obj([("view_kind", Json::str("effect_ledger"))]),
            )?;
            let permissions = permission_rows(svc, &rid)?;
            let mut m = BTreeMap::new();
            m.insert("trace_view".into(), trace);
            m.insert("cost_view".into(), cost);
            m.insert("effect_ledger".into(), effects);
            m.insert("permissions".into(), permissions);
            Ok(envelope("v7_traversal", Some(&rid), m))
        }
        // V8 — approval inbox: the durable pending rows minus the
        // decided ones (set difference over canonical rows — the
        // options render verbatim from each pending row).
        "v8_inbox" => {
            let rid = run_id.ok_or_else(bad_params)?;
            let rows = permission_rows(svc, &rid)?;
            let mut pending: BTreeMap<String, Json> = BTreeMap::new();
            let mut decided_ids: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            for row in match rows.get("rows") {
                Some(Json::Arr(a)) => a.clone(),
                _ => Vec::new(),
            } {
                let rm = obj(&row);
                let kind = str_of(&row, "event_class")
                    .or_else(|| str_of(&row, "class"))
                    .unwrap_or_default();
                let pid = str_of(&row, "permission_id")
                    .or_else(|| str_of(&row, "decision_id"))
                    .unwrap_or_default();
                if kind.ends_with("permission.pending") && !pid.is_empty() {
                    pending.insert(pid, Json::Obj(rm));
                } else if kind.ends_with("permission.decided") {
                    if let Some(d) = str_of(&row, "permission_id") {
                        decided_ids.insert(d);
                    }
                }
            }
            for d in &decided_ids {
                pending.remove(d);
            }
            let mut m = BTreeMap::new();
            m.insert("items".into(), Json::Arr(pending.into_values().collect()));
            Ok(envelope("v8_inbox", Some(&rid), m))
        }
        // V9 — delivery: session lifecycle rows + delivery records +
        // kernel status.
        "v9_delivery" => {
            let rid = run_id.ok_or_else(bad_params)?;
            let lifecycle = svc.call_for_run(
                &rid,
                "read",
                Json::obj([
                    ("cursor", Json::obj([("kind", Json::str("now"))])),
                    ("direction", Json::str("rev")),
                    ("limit", Json::Int(300)),
                    (
                        "filter",
                        Json::obj([(
                            "classes",
                            Json::Arr(vec![
                                Json::str("lifecycle.session."),
                                Json::str("lifecycle.delivery."),
                                Json::str("lifecycle.surface."),
                            ]),
                        )]),
                    ),
                ]),
            )?;
            let status = svc.call("kernel.status", Json::Obj(BTreeMap::new()))?;
            let mut m = BTreeMap::new();
            m.insert("lifecycle".into(), lifecycle);
            m.insert("kernel_status".into(), status);
            Ok(envelope("v9_delivery", Some(&rid), m))
        }
        // V10 — supervision: fleet/experiment runs + work-item/debt
        // partitions + audit view.
        "v10_supervision" => {
            let fleets = svc.call(
                "run_index",
                Json::obj([("filter", Json::obj([("kind", Json::str("fleet"))]))]),
            )?;
            let mut m = BTreeMap::new();
            m.insert("fleets".into(), fleets);
            if let Some(rid) = run_id.clone() {
                let control = svc.call_for_run(
                    &rid,
                    "read",
                    Json::obj([
                        ("cursor", Json::obj([("kind", Json::str("now"))])),
                        ("direction", Json::str("rev")),
                        ("limit", Json::Int(300)),
                        (
                            "filter",
                            Json::obj([(
                                "classes",
                                Json::Arr(vec![
                                    Json::str("control.work_item."),
                                    Json::str("measurement.debt."),
                                    Json::str("lifecycle.debt."),
                                    Json::str("lifecycle.fleet."),
                                ]),
                            )]),
                        ),
                    ]),
                )?;
                m.insert("control".into(), control);
            }
            // S5.4 (R-2.9.6¹): the `debt_report` member — when the browser
            // params carry `debt_report{rows[], policy?, scope?}` the view
            // forwards it to `lab.debt.report` verbatim (records-in — the
            // view never fabricates index rows); the `DebtReport` +
            // `routed_notices` render verbatim.
            if let Some(drq) = p.get("debt_report") {
                if let Ok(r) = svc.call("lab.debt.report", drq.clone()) {
                    m.insert("debt_report".into(), r);
                }
            }
            let audit = svc.call("lab.eval.catalogue", Json::Obj(BTreeMap::new()));
            if let Ok(a) = audit {
                m.insert("catalogue".into(), a);
            }
            Ok(envelope("v10_supervision", run_id.as_deref(), m))
        }
        // V11 — the one-run expansion.
        "v11_run" => {
            let rid = run_id.ok_or_else(bad_params)?;
            let summary = svc.call_for_run(
                &rid,
                "project",
                Json::obj([("view_kind", Json::str("run_summary"))]),
            )?;
            let account = svc.call_for_run(&rid, "account", Json::Obj(BTreeMap::new()))?;
            let head = svc.call_for_run(&rid, "head", Json::Obj(BTreeMap::new()))?;
            let describe = svc.call_for_run(&rid, "describe", Json::Obj(BTreeMap::new()))?;
            let leases = svc.call_for_run(&rid, "list_leases", Json::Obj(BTreeMap::new()))?;
            let mut m = BTreeMap::new();
            m.insert("run_summary".into(), summary);
            m.insert("account".into(), account);
            m.insert("head".into(), head);
            m.insert("describe".into(), describe);
            m.insert("leases".into(), leases);
            Ok(envelope("v11_run", Some(&rid), m))
        }
        _ => Err(bad_params()),
    }
}

/// The durable `security.permission.*` partition (pending + decided —
/// the browser-facing selection happens per view).
fn permission_rows(svc: &mut Sessions, run_id: &str) -> Result<Json, ClientError> {
    svc.call_for_run(
        run_id,
        "read",
        Json::obj([
            ("cursor", Json::obj([("kind", Json::str("now"))])),
            ("direction", Json::str("rev")),
            ("limit", Json::Int(500)),
            (
                "filter",
                Json::obj([(
                    "classes",
                    Json::Arr(vec![Json::str("security.permission.")]),
                )]),
            ),
        ]),
    )
}

fn bad_params() -> ClientError {
    ClientError::Decode("view requires run_id".into())
}
