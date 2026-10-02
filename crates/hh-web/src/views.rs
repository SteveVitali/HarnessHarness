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
//!
//! C2 (S5.7) adds the operations-side catalogue (§7.2 §2.2):
//! - `v3_timetravel` — V3 operations arm (branch_tree + coherent fork
//!   points + the branch/replay/rollback partitions; the writes —
//!   `fork`/`rollback`/`replay`/`counterfactual`/`branch.*` — ride
//!   /api with the surface's injected provenance).
//! - `v4_editor` — V4 assembly editor (resolve/explain/identity/diff/
//!   plan/debt — `HirDiff` apply lands through `lab.assembly.apply`).
//! - `v5_launcher` — V5 launcher pre-flight (`expand`/`power`/`next`
//!   under the browser's sub-objects; registration refusals render
//!   typed).
//! - `v6_analysis` — V6 frontier/surface/strata (`lab.analysis.render`
//!   verbatim — the render spec's `view` member selects the pane).
//! - `v6_evolution` — V6 evolution campaign pane (S6.1a; §05h R-2.9.5's
//!   proposals + counterfactual arms through V6): the campaign's
//!   candidate projection (`lab.evolution.view`) verbatim — a fold over
//!   the durable prefix at the browser's `until_seq`.
//! - `v11_bundle` — V11 experiment bundles (`kernel.validate` +
//!   `kernel.status` + `kernel.check_completeness` over the locator).
//! - `v12_console` — V12 live console (head + account + describe +
//!   tail + pending set; `stream` mints the `stream_events` ticket).

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
    // C2 (S5.7) — the operations catalogue.
    "v3_timetravel",
    "v4_editor",
    "v5_launcher",
    "v6_analysis",
    "v6_evolution",
    "v11_bundle",
    "v12_console",
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
            let pending = pending_permissions(&rows);
            let mut m = BTreeMap::new();
            m.insert("items".into(), Json::Arr(pending));
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
                // S5.6 (R-2.12.6; ADR-0207 D4; WS-K2): the canonical
                // FleetView member — `fleet.fleet_view` verbatim when the
                // run is a fleet activation (`hh.fleet.view/2`,
                // ledger-derived, rebuild-equal; the op's own typed
                // refusal means "not a fleet run" — the member is simply
                // absent, never fabricated).
                if let Ok(fv) = svc.call("fleet.fleet_view", Json::obj([("run", Json::str(&rid))]))
                {
                    // The approval-inbox join (§5i.1 — "the approval
                    // inbox joins on `work_item_id` through the pending
                    // permission's run"): pending permission rows whose
                    // payload names a `work_item_id` group onto the
                    // item's `approvals` member; the full pending set
                    // rides `approval_inbox` so the join is auditable.
                    let pending = permission_rows(svc, &rid)
                        .map(|r| pending_permissions(&r))
                        .unwrap_or_default();
                    let mut by_item: BTreeMap<String, Vec<Json>> = BTreeMap::new();
                    for row in &pending {
                        if let Some(wi) = str_of(row, "work_item_id") {
                            by_item.entry(wi).or_default().push(row.clone());
                        }
                    }
                    let mut fv = fv;
                    if let Json::Obj(ref mut fm) = fv {
                        if let Some(Json::Arr(items)) = fm.get_mut("items") {
                            for it in items.iter_mut() {
                                if let Json::Obj(ref mut im) = it {
                                    let wi = im
                                        .get("work_item_id")
                                        .and_then(Json::as_str)
                                        .map(str::to_string);
                                    let apps = wi
                                        .as_deref()
                                        .and_then(|w| by_item.remove(w))
                                        .unwrap_or_default();
                                    im.insert("approvals".into(), Json::Arr(apps));
                                }
                            }
                        }
                        fm.insert(
                            "approval_inbox".into(),
                            Json::obj([("pending", Json::Arr(pending))]),
                        );
                    }
                    m.insert("fleet_view".into(), fv);
                }
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
        // ── C2 (S5.7) — the operations catalogue ────────────────────
        // V3 — the time-travel operations arm: `branch_tree`, the
        // coherent fork points, and the branch/replay/rollback
        // partitions. The viewer's fork confirmation reads
        // `env_binding`/`coverage`/`uncaptured[]` off the canonical
        // `fork` result (a write, via /api); the incoherent-cut
        // refusal carries `nearest_coherent` (server.rs).
        "v3_timetravel" => {
            let rid = run_id.ok_or_else(bad_params)?;
            let tree = svc.call_for_run(
                &rid,
                "project",
                Json::obj([("view_kind", Json::str("branch_tree"))]),
            )?;
            let points =
                svc.call_for_run(&rid, "coherent_fork_points", Json::Obj(BTreeMap::new()))?;
            let branches = svc.call_for_run(
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
                                Json::str("lifecycle.branch."),
                                Json::str("lifecycle.run.forked"),
                                Json::str("lifecycle.run.rolled_back"),
                                Json::str("lifecycle.head.moved"),
                                Json::str("lifecycle.replay."),
                                Json::str("lifecycle.counterfactual."),
                            ]),
                        )]),
                    ),
                ]),
            )?;
            let mut m = BTreeMap::new();
            m.insert("branch_tree".into(), tree);
            m.insert("coherent_fork_points".into(), points);
            m.insert("branch_rows".into(), branches);
            Ok(envelope("v3_timetravel", Some(&rid), m))
        }
        // V4 — the assembly editor's landing read set: the resolved
        // definition (`version_id` or `namespace`+`name`), its
        // explanation, root coordinates and debt report; `plan` when
        // the browser supplies `source` (the HirDiff candidate's dry
        // run — `apply` is the write leg). Every member is a canonical
        // result verbatim; a resolve refusal is the typed answer.
        "v4_editor" => {
            let mut m = BTreeMap::new();
            let mut qp = p.clone();
            for k in ["debt_report", "a", "b"] {
                qp.remove(k);
            }
            if p.contains_key("version_id") || p.contains_key("namespace") {
                let resolved = svc.call("lab.registry.resolve", Json::Obj(qp.clone()))?;
                if let Some(vid) = str_of(&resolved, "version_id") {
                    if let Ok(exp) = svc.call(
                        "lab.assembly.explain",
                        Json::obj([("sealed", Json::str(&vid))]),
                    ) {
                        m.insert("explain".into(), exp);
                    }
                    if let Ok(id) = svc.call(
                        "lab.assembly.identity",
                        Json::obj([("sealed", Json::str(&vid))]),
                    ) {
                        m.insert("identity".into(), id);
                    }
                }
                m.insert("resolve".into(), resolved);
            }
            if let Some(sealed) = p.get("sealed").and_then(Json::as_str) {
                if let Ok(exp) = svc.call(
                    "lab.assembly.explain",
                    Json::obj([("sealed", Json::str(sealed))]),
                ) {
                    m.insert("explain".into(), exp);
                }
            }
            // The plan/diff arms — the editor's Terraform-class
            // rendering source (AssemblyPlan/AssemblyDiff verbatim).
            if p.contains_key("source") {
                m.insert(
                    "plan".into(),
                    svc.call("lab.assembly.plan", Json::Obj(qp.clone()))?,
                );
            }
            if let (Some(a), Some(b)) = (p.get("a"), p.get("b")) {
                m.insert(
                    "diff".into(),
                    svc.call(
                        "lab.assembly.diff",
                        Json::obj([("a", a.clone()), ("b", b.clone())]),
                    )?,
                );
            }
            if let Some(drq) = p.get("debt_report") {
                m.insert(
                    "debt_report".into(),
                    svc.call("lab.debt.report", drq.clone())?,
                );
            }
            if m.is_empty() {
                return Err(bad_params());
            }
            Ok(envelope("v4_editor", run_id.as_deref(), m))
        }
        // V5 — the launcher pre-flight: `expand` (the CellPlan/
        // OrderPlan dry run), `power` (the PowerReport), `next` (the
        // scheduler's next-cell answer) — each under its own canonical
        // params object; refusals are typed, never warnings.
        "v5_launcher" => {
            let mut m = BTreeMap::new();
            for (member, op) in [
                ("expand", "lab.experiment.expand"),
                ("power", "lab.analysis.power"),
                ("next", "lab.experiment.next"),
            ] {
                if let Some(sub) = p.get(member) {
                    m.insert(member.to_string(), svc.call(op, sub.clone())?);
                }
            }
            if m.is_empty() {
                return Err(bad_params());
            }
            Ok(envelope("v5_launcher", run_id.as_deref(), m))
        }
        // V6 — frontier/surface/strata/rank/reliability/transfer: the
        // render spec's `view` member selects the pane; `diff_reports`
        // when the browser asks for the diff. Values render verbatim
        // (R-D-1…8 — no recomputation this side).
        "v6_analysis" => {
            let op = match str_of(&params, "op").as_deref() {
                Some("diff_reports") => "lab.analysis.diff_reports",
                Some("power") => "lab.analysis.power",
                Some("component_targets") => "lab.analysis.component_targets",
                Some("attribution_design") => "lab.analysis.attribution_design",
                _ => "lab.analysis.render",
            };
            let mut qp = p.clone();
            qp.remove("op");
            let r = svc.call(op, Json::Obj(qp))?;
            Ok(envelope("v6_analysis", run_id.as_deref(), obj(&r)))
        }
        // V6 — the evolution pane (S6.1a): the campaign's folded
        // candidate view verbatim (`lab.evolution.view{run, until_seq?}`
        // — the same fold the audit replay reads; `op = candidates` is
        // the default; the write legs (`propose`, `seal`, …) ride /api
        // under the surface's injected provenance like V3's).
        "v6_evolution" => {
            let rid = run_id.ok_or_else(bad_params)?;
            let mut qp = p.clone();
            qp.remove("run_id");
            qp.insert("run".into(), Json::str(&rid));
            let r = svc.call("lab.evolution.view", Json::Obj(qp))?;
            Ok(envelope("v6_evolution", Some(&rid), obj(&r)))
        }
        // V11 — the bundle/reproducibility pane (run and experiment
        // bundles share the pane): validate + status + completeness
        // over the browser's locator (`path`/`container`/`bundle_ref`
        // pass through verbatim).
        "v11_bundle" => {
            let mut qp = p.clone();
            qp.remove("run_id");
            if qp.is_empty() {
                return Err(bad_params());
            }
            let validate = svc.call("kernel.validate", Json::Obj(qp.clone()))?;
            let status = svc.call("kernel.status", Json::Obj(qp.clone()))?;
            let completeness = svc.call("kernel.check_completeness", Json::Obj(qp))?;
            let mut m = BTreeMap::new();
            m.insert("validate".into(), validate);
            m.insert("status".into(), status);
            m.insert("completeness".into(), completeness);
            Ok(envelope("v11_bundle", run_id.as_deref(), m))
        }
        // V12 — the live console: head + account + describe + the tail
        // the browser polls; `stream:{from?}` mints the `stream_events`
        // ticket (the canonical subscription handle — replay-from-
        // cursor then live, dedupe by seq, ADR-0170 D6).
        "v12_console" => {
            let rid = run_id.ok_or_else(bad_params)?;
            let head = svc.call_for_run(&rid, "head", Json::Obj(BTreeMap::new()))?;
            let account = svc.call_for_run(&rid, "account", Json::Obj(BTreeMap::new()))?;
            let describe = svc.call_for_run(&rid, "describe", Json::Obj(BTreeMap::new()))?;
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
            m.insert("head".into(), head);
            m.insert("account".into(), account);
            m.insert("describe".into(), describe);
            m.insert("tail".into(), tail);
            if let Some(Json::Obj(sp)) = p.get("stream") {
                let mut sp = sp.clone();
                if !sp.contains_key("from") {
                    sp.insert(
                        "from".into(),
                        Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(0))]),
                    );
                }
                m.insert(
                    "stream".into(),
                    svc.call_for_run(&rid, "stream_events", Json::Obj(sp))?,
                );
            }
            Ok(envelope("v12_console", Some(&rid), m))
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

/// The pending fold V8 and V10 share — durable
/// `security.permission.pending` rows minus the `decided` set (a set
/// difference over canonical rows, presentation selection only; the
/// options stay verbatim on each row).
fn pending_permissions(rows: &Json) -> Vec<Json> {
    let mut pending: BTreeMap<String, Json> = BTreeMap::new();
    let mut decided_ids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
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
    pending.into_values().collect()
}

fn bad_params() -> ClientError {
    ClientError::Decode("view requires run_id".into())
}
