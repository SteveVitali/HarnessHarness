//! R6.46 (manifest 310) — FP-E quote for the recorded G-L1 `full`
//! disposition: the W2 measurement packet, the outcome firewall,
//! queue-wait accounting, D_max under the A2 amendment, the five-arm
//! `full` set with PP0 holds, caps/A_eff/root/credit need, and the
//! W4 re-issue inputs (REQ-XD-043/044/062, REQ-XC-015,
//! REQ-XH-121/126(b), A2 2026-11-16).

use std::collections::BTreeMap;

use hharness_runner::continuation::{d_max_ms, W4Ceiling, WcTerm, WindowSum};
use hharness_runner::fp_e::{fp_e_quote, FP_E_SCHEMA};
use hharness_runner::fp_measure::{fold_packet, MeasureErr, MEASURE_SCHEMA};
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::{parse, Json};

fn trial(slot: &str, arm: &str, cost: i64, wall: i64, extra: &[(&str, Json)]) -> Json {
    let mut m = BTreeMap::new();
    m.insert("slot".into(), Json::str(slot));
    m.insert("arm".into(), Json::str(arm));
    m.insert("task".into(), Json::str("build-pmars"));
    m.insert("cost_micros".into(), Json::Int(cost));
    m.insert("wall_ms".into(), Json::Int(wall));
    m.insert("model_ms".into(), Json::Int(wall / 2));
    m.insert("calls".into(), Json::Int(30));
    m.insert(
        "tokens".into(),
        Json::obj([
            ("input", Json::Int(1000)),
            ("cache_read", Json::Int(200)),
            ("output", Json::Int(500)),
        ]),
    );
    m.insert(
        "queue_wait".into(),
        Json::obj([
            ("p50_ms", Json::Int(0)),
            ("p90_ms", Json::Int(5)),
            ("p99_ms", Json::Int(12)),
            ("max_ms", Json::Int(40)),
        ]),
    );
    m.insert("wait_plus_latency_p99_ms".into(), Json::Int(50_012));
    m.insert("latency_max_ms".into(), Json::Int(50_000));
    m.insert("infra".into(), Json::Null);
    m.insert("agent_timeout".into(), Json::Bool(false));
    m.insert("egress_denials".into(), Json::Int(0));
    m.insert("proxy_rewrites".into(), Json::Int(0));
    m.insert("receipted".into(), Json::Bool(true));
    m.insert("scan_flags".into(), Json::Int(0));
    for (k, v) in extra {
        m.insert((*k).to_string(), v.clone());
    }
    Json::Obj(m)
}

// Synthetic provenance labels only: these in-memory fixtures never open files.
fn trials_rec(trials: Vec<Json>) -> Json {
    let mut m = BTreeMap::new();
    m.insert("schema".into(), Json::str("hh-fp-trials/1"));
    m.insert("window".into(), Json::str("ws1"));
    m.insert(
        "fold".into(),
        Json::obj([(
            "sources",
            Json::Arr(vec![Json::str("fixtures/fp-e/harvest/fp-ws1/")]),
        )]),
    );
    m.insert(
        "client_timeouts_ms".into(),
        Json::obj([
            ("mini-swe-agent", Json::Int(900_000)),
            ("pi", Json::Int(900_000)),
            ("claude-code", Json::Int(900_000)),
        ]),
    );
    m.insert(
        "global".into(),
        Json::obj([
            ("attained_concurrency", Json::Int(4)),
            ("wall_hours", Json::str("6/1")),
            ("instance_hours", Json::str("6/1")),
        ]),
    );
    m.insert(
        "nuisance_members".into(),
        Json::Arr(vec![
            Json::str("cost"),
            Json::str("wall_ms"),
            Json::str("queue_wait_ms"),
        ]),
    );
    m.insert(
        "withheld".into(),
        Json::Arr(vec![Json::str("withheld{l1_not_passed}")]),
    );
    m.insert("trials".into(), Json::Arr(trials));
    Json::Obj(m)
}

fn obj_mut(j: &mut Json) -> &mut BTreeMap<String, Json> {
    match j {
        Json::Obj(m) => m,
        _ => panic!("not an object"),
    }
}

#[test]
fn ac1_packet_members_present_per_harness_and_global() {
    let rec = trials_rec(vec![
        trial("ws1-a1", "mini-swe-agent", 3_000, 600_000, &[]),
        trial(
            "ws1-a2",
            "mini-swe-agent",
            5_000,
            800_000,
            &[("agent_timeout", Json::Bool(true))],
        ),
        trial(
            "ws1-b1",
            "pi",
            7_000,
            700_000,
            &[("infra", Json::str("infra:upstream_error"))],
        ),
    ]);
    let p = fold_packet(&rec).unwrap();
    assert_eq!(p.get("schema").and_then(Json::as_str), Some(MEASURE_SCHEMA));
    let g = p.get("global").unwrap();
    for k in [
        "attained_concurrency",
        "wall_hours",
        "instance_hours",
        "trials",
    ] {
        assert!(g.get(k).is_some(), "global.{k}");
    }
    let h = p
        .get("harnesses")
        .and_then(Json::as_obj)
        .unwrap()
        .get("mini-swe-agent")
        .unwrap()
        .clone();
    for k in [
        "cost_micros",
        "cached_share_ppm",
        "tokens",
        "calls",
        "wall_ms",
        "model_ms",
        "queue_wait_ms",
        "wait_plus_latency_p99_ms",
        "admitted_client_timeout_ms",
        "infra",
        "timeout",
        "egress_denials",
        "proxy_rewrites",
        "receipt_completeness",
        "scan_flags",
    ] {
        assert!(h.get(k).is_some(), "harness member {k} absent");
    }
    assert_eq!(
        h.get("cost_micros")
            .and_then(|c| c.get("mean"))
            .and_then(Json::as_int),
        Some(4_000)
    );
    // AC-3 — queue wait recorded beside timeout: the wait+latency P99
    // row sits next to the admitted client timeout.
    assert_eq!(
        h.get("wait_plus_latency_p99_ms").and_then(Json::as_int),
        Some(50_012)
    );
    assert_eq!(
        h.get("admitted_client_timeout_ms").and_then(Json::as_int),
        Some(900_000)
    );
    assert_eq!(
        h.get("infra")
            .and_then(|i| i.get("rate_ppm"))
            .and_then(Json::as_int),
        Some(0)
    );
    // `withheld{l1_not_passed}` retained verbatim.
    assert!(p
        .get("withheld")
        .and_then(Json::as_arr)
        .unwrap()
        .iter()
        .any(|w| w.as_str() == Some("withheld{l1_not_passed}")));
}

#[test]
fn ac2_outcome_injection_refused_labels_untouched() {
    let rec = trials_rec(vec![trial(
        "ws1-a1",
        "mini-swe-agent",
        3_000,
        600_000,
        &[("outcome", Json::str("passed"))],
    )]);
    match fold_packet(&rec) {
        Err(MeasureErr::OutcomeLeak { key, .. }) => assert_eq!(key, "outcome"),
        other => panic!("expected OutcomeLeak, got {other:?}"),
    }
    // A confirmatory relabel refuses (REQ-XD-044).
    let mut rec = trials_rec(vec![trial("ws1-a1", "mini-swe-agent", 3_000, 600_000, &[])]);
    obj_mut(&mut rec).insert(
        "relabels".into(),
        Json::Arr(vec![Json::obj([
            ("from", Json::str("withheld{l1_not_passed}")),
            ("to", Json::str("verdict")),
        ])]),
    );
    match fold_packet(&rec) {
        Err(MeasureErr::ConfirmatoryRelabel { field }) => assert_eq!(field, "verdict"),
        other => panic!("expected ConfirmatoryRelabel, got {other:?}"),
    }
}

#[test]
fn ac4_amended_accounting() {
    let billed = WindowSum {
        window: "w1".into(),
        ceiling_micros: 5_050_000,
        billed_micros: Some(4_900_000),
        cost_lib_micros: Some(5_100_000),
        done: true,
    };
    assert_eq!(billed.effective_micros(), 4_900_000);
    let unreconciled = WindowSum {
        window: "w2".into(),
        ceiling_micros: 9_999_000,
        billed_micros: None,
        cost_lib_micros: Some(2_410_000),
        done: true,
    };
    assert_eq!(
        unreconciled.effective_micros(),
        2_410_000,
        "completed-unreconciled uses cost_lib, never the ceiling"
    );
    let unrun = WindowSum {
        window: "w3".into(),
        ceiling_micros: 3_000_000,
        billed_micros: None,
        cost_lib_micros: None,
        done: false,
    };
    assert_eq!(unrun.effective_micros(), 3_000_000, "unrun → ceiling");
    let w4 = W4Ceiling {
        hourly_micros: 314_140,
        fixed_micros: 1_000_000,
    };
    let wc = WcTerm {
        authorized: false,
        ceiling_micros: 1_940_000,
        reservation_micros: 560_740,
    };
    let d = d_max_ms(&[billed, unreconciled], &w4, &wc);
    let expect = (30_000_000 - (4_900_000 + 2_410_000 + 560_740) - 1_000_000) * 3_600_000 / 314_140;
    assert_eq!(d, expect);
    // A completed row without billed or cost_lib refuses at decode.
    let decl = base_decl();
    let mut m = obj_mut(&mut { decl }).clone();
    if let Json::Obj(dmm) = m.get_mut("d_max").unwrap() {
        dmm.insert(
            "completed".into(),
            Json::Arr(vec![Json::obj([
                ("window", Json::str("w2")),
                ("ceiling_micros", Json::Int(2_410_000)),
                ("done", Json::Bool(true)),
            ])]),
        );
    }
    let e = fp_e_quote(&Json::Obj(m)).unwrap_err();
    assert!(e
        .to_string()
        .contains("ceiling is not a completed-window figure"));
}

fn base_decl() -> Json {
    let trials = trials_rec(vec![
        trial("ws1-c1", "mini-swe-agent", 3_000, 600_000, &[]),
        trial("ws1-c2", "mini-swe-agent", 5_000, 700_000, &[]),
        trial("ws1-c3", "pi", 4_000, 650_000, &[]),
    ]);
    let mut o = BTreeMap::new();
    o.insert("schema".into(), Json::str("hh-fp-e-decl/1"));
    o.insert("ticket".into(), Json::str("R6.46"));
    o.insert("recorded_at".into(), Json::str("2026-11-19T00:00:00Z"));
    o.insert(
        "disposition".into(),
        parse(
            r#"{"disposition":"full","gl1":{"result":"pass",
             "pass_id":"l1-pass:sha256:80bfafee35ac9dd669d80f786ee54deca9948049bcd8559dd7a6dee8d06dd8ef",
             "h_identity_digest":"sha256:c451","corpus_digest":"sha256:072d"}}"#,
        )
        .unwrap(),
    );
    o.insert("trials".into(), trials);
    o.insert(
        "pp0".into(),
        parse(
            r#"{"pi":[{"trial_ref":"pp0/ws1-pp0-5","infra_failed":false,"ag":{"AG3":"fail","AG5":"pass","AG6":"pass","AG8":"pass"}},
                      {"trial_ref":"pp0/ws1-pp0-8","infra_failed":false,"ag":{"AG3":"pass","AG5":"pass","AG6":"fail","AG8":"pass"}},
                      {"trial_ref":"pp0/ws1-pp0-2","infra_failed":false,"ag":{"AG3":"pass","AG5":"pass","AG6":"pass","AG8":"pass"}}],
                "claude-code":[{"trial_ref":"pp0/ws1-pp0-6","infra_failed":false,"ag":{"AG3":"fail","AG5":"pass","AG6":"pass","AG8":"pass"}},
                      {"trial_ref":"pp0/ws1-pp0-9","infra_failed":false,"ag":{"AG3":"pass","AG5":"pass","AG6":"fail","AG8":"pass"}},
                      {"trial_ref":"pp0/ws1-pp0-3","infra_failed":false,"ag":{"AG3":"pass","AG5":"pass","AG6":"pass","AG8":"pass"}}]}"#,
        )
        .unwrap(),
    );
    o.insert(
        "continuation".into(),
        parse(
            r#"{"remaining":{"mini-swe-agent":["s1","s2"],"pi":["p1"],"claude-code":["c1"],"hh_native":["n1"],"v5":["v1"]},
                "consumed_by_arm":{},"in_flight":{},"consumed_micros":0,"orphaned_hi_micros":0,
                "root_cap_micros":110000000,"c":4,"slice_micros":500000,
                "priors_micros":{"mini-swe-agent":374000,"pi":123000,"claude-code":210000,"hh_native":200000,"v5":200000},
                "client_timeouts_ms":{"mini-swe-agent":900000,"pi":900000,"claude-code":900000},
                "priority":{"protected":["mini-swe-agent","hh_native"],"drop_order":["v5","claude-code","pi"]}}"#,
        )
        .unwrap(),
    );
    o.insert(
        "d_max".into(),
        parse(
            r#"{"completed":[{"window":"ws1","ceiling_micros":3000000,"cost_lib_micros":2400000,"done":true},
                {"window":"w2","ceiling_micros":2410000,"cost_lib_micros":2410000,"done":true}],
                "w4":{"hourly_micros":314140,"fixed_micros":1000000},
                "wc":{"authorized":false,"ceiling_micros":1940000,"reservation_micros":560740}}"#,
        )
        .unwrap(),
    );
    o.insert(
        "w4_quote".into(),
        parse(
            r#"{"based_on":"fixtures/fp-e/w4-quote.json",
                "quantities":{"trials":150,"retry_trials":15,"machine_type":"e2-standard-8","disk_gib":300,
                 "dead_man_hours":"45/2","overhead_hours":"5/4","load_hours":"1/2","restart_hours":1,
                 "residual_disk_hours":6,"egress_gib":{"value":3,"label":"estimated"},
                 "nat":false,"external_ipv4":"ephemeral","provisioning_model":"STANDARD",
                 "disk_type":"pd-balanced","surfaces":1,"admission_cutoff_hours":"13/20"},
                "rate_table":{"path":"fixtures/fp-e/pricing.json","sha256":"x"}}"#,
        )
        .unwrap(),
    );
    o.insert(
        "comparative".into(),
        parse(
            r#"{"t":50,"k":1,"measure_ref":"fixtures/fp-e/measurement.json",
                "prior_refs":{"hh_native":"fixtures/fp-e/prereg.json",
                              "v5":"fixtures/fp-e/prereg.json"},
                "infra_allowance_ppm":0,
                "compute":[{"window":"w4","quote_id":"pq-w4","ceiling_micros":7988890},
                           {"window":"wc","quote_id":"pq-wc","ceiling_micros":1940000}]}"#,
        )
        .unwrap(),
    );
    Json::Obj(o)
}

#[test]
fn ac5_full_set_pp0_holds_and_ack() {
    let out = fp_e_quote(&base_decl()).unwrap();
    assert_eq!(out.get("schema").and_then(Json::as_str), Some(FP_E_SCHEMA));
    let arms: Vec<&str> = out
        .get("arms")
        .and_then(Json::as_arr)
        .unwrap()
        .iter()
        .filter_map(|a| a.as_str())
        .collect();
    // five declared arms; PP0 holds pi + claude-code — the included
    // set is mini-swe-agent + hh_native + v5.
    assert_eq!(arms, ["mini-swe-agent", "hh_native", "v5"]);
    let sd = out.get("scope_decisions").and_then(Json::as_arr).unwrap();
    assert_eq!(sd.len(), 2);
    assert!(
        out.get("d_max")
            .unwrap()
            .get("d_max_ms")
            .unwrap()
            .as_int()
            .unwrap()
            > 0
    );
    assert_eq!(
        out.get("caps")
            .unwrap()
            .get("schema")
            .and_then(Json::as_str),
        Some("hh-fp-arm-caps/1")
    );
    assert_eq!(
        out.get("comparative_quote")
            .unwrap()
            .get("schema")
            .and_then(Json::as_str),
        Some("comparative-quote/1")
    );
    let ack = out.get("acknowledgement").unwrap();
    assert_eq!(
        ack.get("quote_authorizes").and_then(Json::as_bool),
        Some(false)
    );
    assert_eq!(
        ack.get("next_ticket").and_then(Json::as_str),
        Some("HUMAN-H15")
    );
}

#[test]
fn ac5_fallback_three_hosted_and_label() {
    let mut decl = base_decl();
    obj_mut(&mut decl).insert(
        "disposition".into(),
        parse(r#"{"disposition":"fallback","gl1":{"result":"not_passed"}}"#).unwrap(),
    );
    obj_mut(&mut decl).insert(
        "fallback".into(),
        parse(r#"{"schema":"hh-fp-fallback/1","cause":"l1_not_passed"}"#).unwrap(),
    );
    let out = fp_e_quote(&decl).unwrap();
    assert!(out
        .get("withheld")
        .and_then(Json::as_arr)
        .unwrap()
        .iter()
        .any(|w| w.as_str() == Some("withheld{l1_not_passed}")));
    assert!(out.get("fallback").unwrap().get("schema").is_some());
}

#[test]
fn ac5_full_requires_gl1_pass() {
    let mut decl = base_decl();
    obj_mut(&mut decl).insert(
        "disposition".into(),
        parse(r#"{"disposition":"full","gl1":{"result":"not_passed"}}"#).unwrap(),
    );
    assert!(matches!(
        fp_e_quote(&decl),
        Err(hharness_runner::fp_e::FpEErr::MissingGl1)
    ));
}

// ── AC-1: the retained-WAL extractor (fp_ws1) ─────────────────────────
//
// Synthetic WAL fixture: two `model.call.completed` + one
// `measurement.cost.attributed` row per slot, linked through
// `control.budget.reserved` (`holder = "mc:<id>"`). An outcome member
// on the WAL line is never read by the fold.

use hharness_runner::continuation::check_cost_only;
use hharness_runner::fp_ws1::{
    corpus_evidence, extract_trials, merge_pp0_snapshots, merged_pp0_decl, SlotEvidence,
};
use hharness_runner::holds::{pp0_hold, Pp0HoldKind, Pp0TrialEvidence};

fn wal_lines(run: &str, slot: &str, budget: &str, mc: &str, cost: i64, t0: i64) -> String {
    let rows = [
        format!(
            r#"{{"v":{{"class":"control.budget.reserved","payload":{{"holder":"mc:{mc}","budget_id":"{budget}"}}}}}}"#
        ),
        format!(
            r#"{{"v":{{"class":"measurement.cost.attributed","payload":{{"attribution":{{"budget_id":"{budget}","participant_ref":"{slot}"}},"money":{{"micro_units":{cost}}},"roles":{{"tokens.input.uncached":10,"tokens.output.visible":5}}}}}}}}"#
        ),
        format!(
            r#"{{"v":{{"class":"model.call.completed","payload":{{"model_call_id":"{mc}","verdict":"pass","timing":{{"received_ms":{t0},"receipted_ms":{t1},"queue_wait_ms":7,"latency_ms":100}},"usage":{{"cache_read":3}}}}}}}}"#,
            t1 = t0 + 1_000,
        ),
    ];
    let _ = run;
    rows.join("\n") + "\n"
}

fn one_evidence() -> BTreeMap<String, SlotEvidence> {
    BTreeMap::from([(
        "ws1-corpus-01".to_string(),
        SlotEvidence {
            slot: "ws1-corpus-01".into(),
            arm: "mini-swe-agent".into(),
            task: "task-a".into(),
            denied_hosts: vec![],
            agent_end_cause: "exited{0}".into(),
            receipt_count: 1,
        },
    )])
}

#[test]
fn ac1_extract_trials_deterministic_cost_only() {
    let wal = wal_lines("r", "ws1-corpus-01", "b1", "c1", 111, 5_000)
        + &wal_lines("r", "ws1-corpus-01", "b2", "c2", 222, 9_000);
    let runs = vec![("run-a".to_string(), wal.into_bytes())];
    let srcs = BTreeMap::from([("wal".to_string(), "sha256:abc".to_string())]);
    let r1 = extract_trials(&runs, &one_evidence(), &srcs).unwrap();
    let r2 = extract_trials(&runs, &one_evidence(), &srcs).unwrap();
    assert_eq!(
        r1.to_canonical_string(),
        r2.to_canonical_string(),
        "extractor output must be byte-deterministic"
    );
    let t = &r1.get("trials").and_then(Json::as_arr).unwrap()[0];
    assert_eq!(t.get("cost_micros").and_then(Json::as_int), Some(333));
    assert_eq!(t.get("calls").and_then(Json::as_int), Some(2));
    // One contributing run → wall = first received → last receipted
    // inside that run (5000 → 10000).
    assert_eq!(t.get("wall_ms").and_then(Json::as_int), Some(5_000));
    // The digest binding rides verbatim.
    assert_eq!(
        r1.get("fold")
            .and_then(|f| f.get("sources"))
            .and_then(|s| s.get("wal"))
            .and_then(Json::as_str),
        Some("sha256:abc")
    );
    // The outcome member on the WAL line is never propagated; the
    // emitted record passes the firewall scan and the fold.
    check_cost_only(&r1).unwrap();
    fold_packet(&r1).unwrap();
}

#[test]
fn ac1_extract_trials_missing_evidence_refuses() {
    let wal = wal_lines("r", "ws1-corpus-09", "b1", "c1", 1, 0);
    let runs = vec![("run-a".to_string(), wal.into_bytes())];
    let err = extract_trials(&runs, &one_evidence(), &BTreeMap::new()).unwrap_err();
    assert!(err.contains("ws1-corpus-09"), "err: {err}");
}

#[test]
fn ac1_pp0_merge_union_failures_primary_scalars() {
    let primary = vec![parse(
        r#"{"trial_ref":"pp0/ws1-pp0-5","harness":"pi","task":"task-a",
            "checks":{"AG3":"pass","AG6":"pass"},
            "agent_end_cause":"agent_timeout","receipt_count":4}"#,
    )
    .unwrap()];
    let rerun = vec![parse(
        r#"{"trial_ref":"pp0/ws1-pp0-5","harness":"pi","task":"task-a",
            "checks":{"AG3":"fail","AG6":"fail"},
            "agent_end_cause":"exited{1}","receipt_count":0}"#,
    )
    .unwrap()];
    let merged = merge_pp0_snapshots(&[primary, rerun]);
    let row = merged.get("ws1-pp0-5").unwrap();
    // Union of failures — a re-execution never rescues a failure.
    let ag = row.get("ag").and_then(Json::as_obj).unwrap();
    assert_eq!(ag.get("AG3").and_then(Json::as_str), Some("fail"));
    assert_eq!(ag.get("AG6").and_then(Json::as_str), Some("fail"));
    // Scalars keep the primary snapshot's values.
    assert_eq!(
        row.get("agent_end_cause").and_then(Json::as_str),
        Some("agent_timeout")
    );
    assert_eq!(row.get("receipt_count").and_then(Json::as_int), Some(4));
    // The decl member decodes strictly and the hold names AG3.
    let decl = merged_pp0_decl(&merged);
    let ev: Vec<Pp0TrialEvidence> = decl["pi"]
        .iter()
        .map(Pp0TrialEvidence::from_json)
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(pp0_hold(&ev), Some(Pp0HoldKind::Gate(3)));
}

#[test]
fn ac1_corpus_evidence_manifest_members() {
    let manifest = parse(
        r#"{"schema":"hh-ws1-corpus-manifest/1","slots":[
            {"slot_id":"ws1-corpus-03","task":"task-z",
             "denied_hosts":["bad.example"],"agent_end_cause":"exited{0}",
             "receipt_count":2}]}"#,
    )
    .unwrap();
    let ev = corpus_evidence(&manifest);
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].slot, "ws1-corpus-03");
    assert_eq!(ev[0].arm, "mini-swe-agent");
    assert_eq!(ev[0].denied_hosts, vec!["bad.example".to_string()]);
}
