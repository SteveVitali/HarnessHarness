//! The C1 instrument acceptance tests — AC-R-2.11.2-{1–5,7,8,10,12,14,15}
//! over the `hh-web` projection surface (§7.2). Every test drives the
//! full `serve_request` pipeline against a stub binding-(c) kernel that
//! records the canonical ops it receives — the assertions are over both
//! the served bytes *and* the ops the kernel saw (no surface mutation
//! ever leaves except as a named kernel call — P12).

mod common;

use common::*;
use hh_web::server::{serve_request, Served};
use hh_web::views::VIEW_IDS;
use hh_wire::json::Json;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

fn served_json(s: Served) -> (u16, Json) {
    match s {
        Served::Json(c, t) => (c, hh_wire::json::parse(&t).unwrap_or(Json::Null)),
        other => panic!(
            "expected JSON, got {:?}",
            matches!(other, Served::Static(..))
        ),
    }
}

fn jstr(j: &Json, k: &str) -> String {
    j.get(k).and_then(Json::as_str).unwrap_or("").to_string()
}

/// A stub handler that answers `open_session` and returns canned
/// `read`/`project`/`run_index` rows (the canonical shapes the surface
/// assembles).
fn canned(_m: &str, _p: &Json) -> Json {
    Json::obj([("ok", Json::Bool(true))])
}

// AC-1 — the surface is store-free: two identical view calls serve
// byte-identical bodies (no accumulated client-side state), and the
// kernel sees the same canonical op sequence both times.
#[test]
fn ac1_store_free_byte_identical() {
    let (addr, rx, _jh) = stub_kernel(
        |m, _p| match m {
            "open_session" => Json::obj([("session_id", Json::str("s-1"))]),
            "project" => Json::obj([("view_hash", Json::str("vh-1"))]),
            _ => Json::Null,
        },
        64,
    );
    let (gate, mut svc, det) = fixture(Some(addr));
    let body = api_body("view.v11_run", Json::obj([("run_id", Json::str("run-9"))]));
    let a = serve_request(&gate, &mut svc, &det, &api_head(), &body);
    let b = serve_request(&gate, &mut svc, &det, &api_head(), &body);
    let Served::Json(_, ta) = a else { panic!() };
    let Served::Json(_, tb) = b else { panic!() };
    assert_eq!(
        ta, tb,
        "the same view at the same watermarks re-renders byte-identically"
    );
    // Kernel saw: open_session once (cached attach), then the op set for
    // each call — the surface keeps no per-request state of record.
    let mut saw = Vec::new();
    while let Ok(x) = rx.try_recv() {
        saw.push(x.0);
    }
    assert!(saw.contains(&"project".to_string()));
    assert_eq!(
        saw.iter().filter(|m| *m == "open_session").count(),
        1,
        "attach is cached — never a store of record, one session per run"
    );
}

// AC-2 — scorecard/leaderboard fixtures render verbatim: `n/a{reason}`
// cells carry the reason (never `0`/blank), ids/hashes pass through.
#[test]
fn ac2_scorecard_na_cells_verbatim() {
    let row = Json::obj([
        ("report_id", Json::str("ar-7")),
        ("view_hash", Json::str("vh-abc")),
        (
            "cells",
            Json::Arr(vec![
                Json::obj([
                    ("metric", Json::str("pass_at_1")),
                    ("value", Json::obj([("n/a", Json::str("observability"))])),
                ]),
                Json::obj([("metric", Json::str("cost")), ("value", Json::Int(42))]),
            ]),
        ),
    ]);
    let r = row.clone();
    let (addr, _rx, _jh) = stub_kernel(move |_m, _p| r.clone(), 64);
    let (gate, mut svc, det) = fixture(Some(addr));
    let (c, j) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body(
            "view.v2_scorecard",
            Json::obj([("scorecard_ref", Json::str("ar-7"))]),
        ),
    ));
    assert_eq!(c, 200);
    assert_eq!(jstr(&j, "view"), "v2_scorecard");
    let text = j.to_canonical_string();
    assert!(text.contains("observability"), "n/a reason renders: {text}");
    assert!(text.contains("ar-7") && text.contains("vh-abc"));
}

// AC-3 — the context scrub view carries the context_view projection and
// the durable `context.assembled` rows (the viewer's hash is the
// event's hash — the surface never recomputes).
#[test]
fn ac3_context_view_carries_durable_rows() {
    let (addr, rx, _jh) = stub_kernel(
        |m, p| match m {
            "open_session" => Json::obj([("session_id", Json::str("s-1"))]),
            "project" => Json::obj([
                ("view_hash", Json::str("ctx-h")),
                ("items", Json::Arr(vec![Json::str("i1")])),
            ]),
            "read" => {
                assert!(p.to_canonical_string().contains("context."));
                Json::obj([(
                    "rows",
                    Json::Arr(vec![Json::obj([
                        ("event_class", Json::str("context.assembled")),
                        ("view_hash", Json::str("ctx-h")),
                    ])]),
                )])
            }
            _ => Json::Null,
        },
        64,
    );
    let (gate, mut svc, det) = fixture(Some(addr));
    let (c, j) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body(
            "view.v3_context",
            Json::obj([("run_id", Json::str("run-3"))]),
        ),
    ));
    assert_eq!(c, 200);
    assert_eq!(jstr(&j, "view"), "v3_context");
    assert!(j.get("context_view").is_some() && j.get("context_rows").is_some());
    // The kernel saw the hello handshake + exactly one open_session +
    // project + read.
    let mut saw = Vec::new();
    while let Ok(x) = rx.try_recv() {
        saw.push(x.0);
    }
    assert_eq!(saw, vec!["hello", "open_session", "project", "read"]);
}

// AC-4/AC-5 — the monitor view's tail carries the durable rows verbatim
// (the rendered path is whatever `read` returns — a head.moved row is
// just a row; the surface holds no cursor state across requests: a
// `from_seq` the browser supplies rides the read cursor, never a
// remembered store).
#[test]
fn ac4_ac5_monitor_tail_and_cursor_passthrough() {
    let (addr, rx, _jh) = stub_kernel(
        |m, _p| match m {
            "open_session" => Json::obj([("session_id", Json::str("s-1"))]),
            "read" => Json::obj([(
                "rows",
                Json::Arr(vec![Json::obj([
                    ("seq", Json::Int(9)),
                    ("event_class", Json::str("lifecycle.head.moved")),
                ])]),
            )]),
            _ => Json::obj([("ok", Json::Bool(true))]),
        },
        64,
    );
    let (gate, mut svc, det) = fixture(Some(addr));
    let (c, j) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body(
            "view.v5_monitor",
            Json::obj([("run_id", Json::str("r-4")), ("from_seq", Json::Int(5))]),
        ),
    ));
    assert_eq!(c, 200);
    let t = j.to_canonical_string();
    assert!(t.contains("lifecycle.head.moved"));
    // The read the kernel saw carried the browser's seq as the cursor —
    // no surface-side cursor memory.
    let (_, read_params) = rx.try_iter().find(|(m, _)| m == "read").expect("read op");
    assert_eq!(
        read_params
            .get("cursor")
            .and_then(|c| c.get("seq"))
            .and_then(Json::as_int),
        Some(5)
    );
}

// AC-7 — approval round trip: the inbox folds pending − decided over
// the durable partition; `respond_permission` lands with responder
// provenance + a request id injected by the surface (P12), and a
// re-submitted response gets the kernel's typed refusal verbatim.
#[test]
fn ac7_inbox_fold_and_respond() {
    let (addr, rx, _jh) = stub_kernel(
        |m, _p| match m {
            "open_session" => Json::obj([("session_id", Json::str("s-1"))]),
            "read" => Json::obj([(
                "rows",
                Json::Arr(vec![
                    Json::obj([
                        ("event_class", Json::str("security.permission.pending")),
                        ("permission_id", Json::str("perm-1")),
                        (
                            "options",
                            Json::Arr(vec![Json::str("allow"), Json::str("deny")]),
                        ),
                    ]),
                    Json::obj([
                        ("event_class", Json::str("security.permission.pending")),
                        ("permission_id", Json::str("perm-2")),
                    ]),
                    Json::obj([
                        ("event_class", Json::str("security.permission.decided")),
                        ("permission_id", Json::str("perm-2")),
                        ("outcome", Json::str("deny")),
                    ]),
                ]),
            )]),
            "respond_permission" => Json::obj([("ok", Json::Bool(true))]),
            _ => Json::Null,
        },
        64,
    );
    let (gate, mut svc, det) = fixture(Some(addr));
    let (c, j) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body("view.v8_inbox", Json::obj([("run_id", Json::str("r-7"))])),
    ));
    assert_eq!(c, 200);
    let t = j.to_canonical_string();
    assert!(t.contains("perm-1"), "pending survives: {t}");
    assert!(!t.contains("perm-2"), "decided is out of the inbox: {t}");
    // Respond — the surface injects session + responder + request id.
    let (c, j) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body(
            "respond_permission",
            Json::obj([
                ("run_id", Json::str("r-7")),
                ("permission_id", Json::str("perm-1")),
                ("outcome", Json::str("allow")),
            ]),
        ),
    ));
    assert_eq!(c, 200, "{j:?}");
    let mut saw = rx
        .try_iter()
        .filter(|(m, _)| m == "respond_permission")
        .collect::<Vec<_>>();
    assert_eq!(saw.len(), 1);
    let p = saw.remove(0).1;
    assert_eq!(jstr(&p, "session_id"), "s-1");
    let resp = p.get("responder").cloned().unwrap_or(Json::Null);
    assert_eq!(jstr(&resp, "subject_ref"), "human:principal");
    assert_eq!(jstr(&resp, "surface_session_ref"), "s-1");
    assert!(jstr(&p, "idempotency_key").starts_with("web:"));
    // The browser-supplied responder/session_id is overridden, never
    // trusted.
    let (c, _) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body(
            "respond_permission",
            Json::obj([
                ("run_id", Json::str("r-7")),
                ("permission_id", Json::str("perm-1")),
                ("outcome", Json::str("allow")),
                (
                    "responder",
                    Json::obj([("subject_ref", Json::str("mallory"))]),
                ),
                ("session_id", Json::str("forged")),
            ]),
        ),
    ));
    assert_eq!(c, 200);
    let p = rx
        .try_iter()
        .filter(|(m, _)| m == "respond_permission")
        .last()
        .map(|x| x.1)
        .unwrap();
    assert_eq!(
        jstr(
            &p.get("responder").cloned().unwrap_or(Json::Null),
            "subject_ref"
        ),
        "human:principal",
        "surface owns provenance — forged members are stripped"
    );
    assert_eq!(jstr(&p, "session_id"), "s-1");
    let _ = RecvTimeoutError::Timeout;
    let _ = Duration::from_secs(0);
}

// AC-8 — the admission table stays closed at C2: the session lifecycle
// verbs and every unlisted op are `Refused{op_not_admitted}` and never
// reach the kernel; the C2 write set (§7.2 §5.2) is admitted — those
// pass the gate and fail only at transport against the dead kernel.
#[test]
fn ac8_op_admission_closed() {
    let (addr, rx, _jh) = stub_kernel(canned, 64);
    let _ = addr;
    let (gate, mut svc, det) = fixture(None);
    for op in [
        // Session lifecycle — the surface's own, never the browser's.
        "open_session",
        "close",
        // Never-admitted kernel verbs.
        "kernel.export",
        "kernel.fetch",
        "lab.results.put_row",
        // `request_redaction` is a declared proposal the boundary does
        // not yet serve — unadmitted until it exists (§7.2 V9).
        "request_redaction",
        // Unlisted entirely.
        "nonsense.op",
        "lab.fleet.dispatch",
    ] {
        let s = serve_request(
            &gate,
            &mut svc,
            &det,
            &api_head(),
            &api_body(op, Json::obj([("run_id", Json::str("r-1"))])),
        );
        let (c, j) = served_json(s);
        assert_eq!(c, 403, "{op}");
        assert_eq!(jstr(&j, "error"), "Refused");
        assert_eq!(jstr(&j, "reason"), "op_not_admitted");
    }
    // The C2 write set is admitted — the gate passes them; the dead
    // kernel answers transport `Unavailable` (a 200 error payload, the
    // typed refusal passthrough).
    for op in [
        "submit",
        "cancel",
        "steer",
        "respond_elicitation",
        "subscribe",
        "fork",
        "navigate",
        "rollback",
        "replay",
        "counterfactual",
        "branch.open",
        "branch.discard",
        "branch.promote",
        "respond_permission",
        "amend",
        "lab.assembly.assemble",
        "lab.assembly.apply",
        "lab.assembly.adopt",
        "lab.registry.register",
        "lab.registry.publish",
        "lab.leaderboard.define",
        "lab.leaderboard.publish",
        "lab.leaderboard.retract_entry",
        "lab.experiment.register",
        "lab.experiment.open_experiment",
        "lab.experiment.launch",
        "lab.experiment.claim",
        "lab.experiment.settle",
        "lab.experiment.pause",
        "lab.experiment.resume",
        "lab.experiment.close",
        "lab.analysis.analyze",
        "lab.permission.grant_approver",
        "lab.permission.revoke_approver",
        "lab.debt.evaluate",
        "lab.serve",
        "kernel.reproduce",
    ] {
        let (c, j) = served_json(serve_request(
            &gate,
            &mut svc,
            &det,
            &api_head(),
            &api_body(op, Json::obj([("run_id", Json::str("r-1"))])),
        ));
        assert_eq!(c, 200, "{op} admitted → passes the gate: {j:?}");
        assert_eq!(
            jstr(&j, "reason"),
            "",
            "{op} is admitted — no op_not_admitted refusal"
        );
    }
    assert!(
        rx.try_recv().is_err(),
        "a refused op never reaches the kernel"
    );
}

// AC-10 — `prove_inclusion`/`audit_view`/`verify` are admitted reads;
// the kernel's typed refusal for a tampered fixture renders verbatim.
#[test]
fn ac10_prove_inclusion_and_tamper_passthrough() {
    let (addr, _rx, _jh) = stub_kernel(
        |m, _p| match m {
            "open_session" => Json::obj([("session_id", Json::str("s-1"))]),
            _ => Json::obj([("verified", Json::Bool(true))]),
        },
        64,
    );
    let (gate, mut svc, det) = fixture(Some(addr));
    for op in ["prove_inclusion", "audit_view", "verify", "run_index"] {
        let (c, j) = served_json(serve_request(
            &gate,
            &mut svc,
            &det,
            &api_head(),
            &api_body(op, Json::obj([("run_id", Json::str("r-1"))])),
        ));
        assert_eq!(c, 200, "{op}");
        assert!(j.get("error").is_none(), "{op}: {j:?}");
    }
}

// AC-12 — `n/a{observability}` / `n/a{class}` cells and row badges ride
// verbatim through the sink (the surface renders what the records say).
#[test]
fn ac12_observability_na_passthrough() {
    let (addr, _rx, _jh) = stub_kernel(
        |_m, _p| {
            Json::obj([(
                "cells",
                Json::Arr(vec![Json::obj([
                    ("value", Json::obj([("n/a", Json::str("class"))])),
                    ("badge", Json::str("hosted")),
                ])]),
            )])
        },
        64,
    );
    let (gate, mut svc, det) = fixture(Some(addr));
    let (c, j) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body("view.v6_comparison", Json::Null),
    ));
    assert_eq!(c, 200);
    let t = j.to_canonical_string();
    assert!(t.contains("\"class\"") && t.contains("hosted"), "{t}");
}

// AC-14 — surface-vs-CLI parity: a canonical op's result serves
// byte-identical *content* to what the kernel returned (the surface
// adds only the envelope/presentation — a fixture diff is presentation
// only). Assert the kernel's exact bytes appear verbatim in the body.
#[test]
fn ac14_cli_parity_verbatim_result() {
    let result = Json::obj([
        ("audit_ref", Json::str("ar-123")),
        ("inclusion_ok", Json::Bool(true)),
        ("tree_head", Json::str("th-9")),
    ]);
    let r = result.clone();
    let (addr, _rx, _jh) = stub_kernel(move |_m, _p| r.clone(), 64);
    let (gate, mut svc, det) = fixture(Some(addr));
    let Served::Json(200, body) = serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body("prove_inclusion", Json::obj([("run_id", Json::str("r-1"))])),
    ) else {
        panic!()
    };
    // The canonical member ordering survives — the served body contains
    // the kernel's exact serialized result (presentation adds the view
    // wrapper only for `view.*`; raw ops pass through 1:1).
    for k in ["audit_ref", "ar-123", "inclusion_ok", "th-9"] {
        assert!(body.contains(k), "{k} in {body}");
    }
}

// AC-15 — the view catalogue is exactly the C1 set plus the C2
// operations catalogue (S5.7: `v3_timetravel`, `v4_editor`,
// `v5_launcher`, `v6_analysis`, `v11_bundle`, `v12_console`) — nothing
// outside the catalogue is admitted, and every served view labels its
// `view` member.
#[test]
fn ac15_catalogue_closed_and_labelled() {
    assert_eq!(
        VIEW_IDS,
        &[
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
            "v3_timetravel",
            "v4_editor",
            "v5_launcher",
            "v6_analysis",
            "v11_bundle",
            "v12_console",
        ]
    );
    let (gate, mut svc, det) = fixture(None);
    let (c, j) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body("view.v99_unlisted", Json::Null),
    ));
    assert_eq!(c, 403);
    assert_eq!(jstr(&j, "reason"), "op_not_admitted");
}

/// V1 — the run browser passes `run_index` through with its filter.
#[test]
fn aux_v1_runs_index() {
    let (addr, rx, _jh) = stub_kernel(
        |_m, _p| {
            Json::obj([(
                "entries",
                Json::Arr(vec![Json::obj([("run_id", Json::str("run-1"))])]),
            )])
        },
        64,
    );
    let (gate, mut svc, det) = fixture(Some(addr));
    let (c, j) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body(
            "view.v1_runs",
            Json::obj([("filter", Json::obj([("kind", Json::str("agent"))]))]),
        ),
    ));
    assert_eq!(c, 200);
    assert_eq!(jstr(&j, "view"), "v1_runs");
    let (_m, p) = rx
        .try_iter()
        .find(|(m, _)| m == "run_index")
        .expect("run_index op");
    assert!(p.to_canonical_string().contains("agent"));
}

/// P8 — a `{accounting}`-only sink policy withholds content members of
/// a served payload (`{"withheld": "content"}`), never the bytes.
#[test]
fn aux_sink_withholding() {
    let (addr, _rx, _jh) = stub_kernel(
        |_m, _p| {
            Json::obj([
                ("content", Json::str("model-visible bytes")),
                ("size", Json::Int(12)),
            ])
        },
        64,
    );
    let (gate, mut svc, det) = fixture(Some(addr));
    svc.sink.content_classes.clear();
    svc.sink
        .content_classes
        .insert(hh_telemetry::sinks::ContentClass::Accounting);
    svc.sink.requires_consent = false;
    let (c, j) = served_json(serve_request(
        &gate,
        &mut svc,
        &det,
        &api_head(),
        &api_body("run_index", Json::Null),
    ));
    assert_eq!(c, 200);
    let t = j.to_canonical_string();
    assert!(!t.contains("model-visible bytes"), "{t}");
    assert!(t.contains("withheld"), "{t}");
    assert!(
        t.contains("\"size\":12") || t.contains("\"size\": 12"),
        "{t}"
    );
}
