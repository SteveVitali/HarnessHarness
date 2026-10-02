//! S6.3b boundary tests — the `lab.attribution.*` surface over
//! `hh-embed` (§5h.7; R-2.9.7 6c): capability gating, the design
//! validator + `estimate_rollouts`, the `attribute` fold over
//! caller-supplied `ArmOutcome` records, `locus`/`quality`
//! projections, and the typed refusal surface. The real-arm execution
//! leg (`open_arms` → Group W `counterfactual` with
//! `noise_coupling = crn`) runs in `conformance.rs` where the session
//! fixtures live.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_embed::service::{EmbedService, ServiceConfig};
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

fn test_dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-embed-attr-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn service() -> EmbedService {
    let root = test_dir("svc");
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "s6-3b".into(),
    })
    .unwrap()
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn ok(r: &Json) -> Json {
    r.get("result")
        .cloned()
        .unwrap_or_else(|| panic!("expected result: {r:?}"))
}

fn err_kind(r: &Json) -> String {
    r.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("kind"))
        .and_then(Json::as_str)
        .or_else(|| {
            r.get("error")
                .and_then(|e| e.get("message"))
                .and_then(Json::as_str)
        })
        .unwrap_or_default()
        .to_string()
}

fn err_reason(r: &Json) -> String {
    r.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("reason"))
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string()
}

fn hello(svc: &mut EmbedService, caps: &[(&str, bool)]) {
    let mut m = BTreeMap::new();
    for (k, v) in caps {
        m.insert(k.to_string(), Json::Bool(*v));
    }
    let r = call(
        svc,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("t")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            ("capabilities", Json::Obj(m)),
        ]),
    );
    assert!(r.get("result").is_some(), "hello: {r:?}");
}

fn hello_attr(svc: &mut EmbedService) {
    hello(
        svc,
        &[("experimental", true), ("serves_measurement", true)],
    );
}

/// A parseable `matched_total` MatchSpec document.
fn match_spec() -> Json {
    Json::obj([
        ("dimensions", Json::Arr(vec![Json::str("model_calls")])),
        ("mode", Json::str("matched_total")),
        ("tolerance", Json::Int(0)),
        ("model_scope", Json::str("same_snapshot")),
        ("cache_policy", Json::str("cold_start")),
    ])
}

/// A TE_crn M2 design over one target — the happy path.
fn design_json() -> Json {
    Json::obj([
        ("schema", Json::str("hh-attribution-design/1")),
        (
            "subject",
            Json::obj([
                ("configuration_id", Json::str("cfg-1")),
                ("run_ids", Json::Arr(vec![])),
                ("task_ids", Json::Arr(vec![])),
            ]),
        ),
        ("method", Json::str("M2")),
        (
            "targets",
            Json::Arr(vec![Json::obj([
                ("kind", Json::str("rule")),
                ("ref", Json::str("retrieval-ranking")),
            ])]),
        ),
        ("fork_policy", Json::str("run_start")),
        ("k", Json::Int(4)),
        ("replay_mode_requested", Json::str("deterministic")),
        ("noise_coupling", Json::str("crn")),
        ("coupling_assumption", Json::str("weak")),
        ("match", match_spec()),
        ("outcome", Json::str("task_success")),
        ("budget", Json::obj([("reserved", Json::Int(0))])),
        ("seed", Json::Int(42)),
        ("claim_kind", Json::str("outcome")),
        (
            "coupled_sources",
            Json::Arr(vec![
                Json::obj([
                    ("source", Json::str("environment_state")),
                    ("coupling_agreement", Json::Bool(true)),
                ]),
                Json::obj([
                    ("source", Json::str("provider_sampling")),
                    ("coupling_agreement", Json::Bool(true)),
                ]),
            ]),
        ),
        ("label", Json::str("confirmatory")),
    ])
}

/// The executed-arm outcomes the `counterfactual` driver would return
/// — deterministic unit outcomes for the fixture (factual arm at the
/// planted rate, counterfactual at the ablated rate).
fn outcomes_json(fact: i64, cf: i64, k: u32) -> Json {
    let mut v = Vec::new();
    for i in 0..k {
        for (role, p) in [("factual", fact), ("counterfactual", cf)] {
            v.push(Json::obj([
                ("target", Json::str("retrieval-ranking")),
                ("fork_point", Json::Int(0)),
                ("role", Json::str(role)),
                ("replicate_index", Json::Int(i as i64)),
                ("outcome", Json::Int(p)),
                ("validity", Json::str("deterministic")),
                (
                    "change_rate_ppm",
                    if role == "factual" {
                        Json::Int(1_000_000)
                    } else {
                        Json::Null
                    },
                ),
                (
                    "consumed_sources",
                    Json::Arr(vec![
                        Json::str("environment_state"),
                        Json::str("provider_sampling"),
                    ]),
                ),
                ("spend", Json::Int(1)),
            ]));
        }
    }
    Json::Arr(v)
}

/// The capability gates — experimental + `serves_measurement` — apply
/// to the attribution family like every measurement op.
#[test]
fn s6_3b_attribution_ops_gated() {
    let mut svc = service();
    hello(&mut svc, &[("serves_measurement", true)]);
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "lab.attribution.design",
            Json::obj([("design", design_json())]),
        )),
        "ExperimentalRequired"
    );
    let mut svc = service();
    hello(&mut svc, &[("experimental", true)]);
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "lab.attribution.design",
            Json::obj([("design", design_json())]),
        )),
        "CapabilityNotDeclared"
    );
}

/// `lab.attribution.design` validates, content-addresses and prices the
/// walk *before* any reservation (AC-R-2.9.7-14's scheduler leg).
#[test]
fn s6_3b_attribution_design_validates_and_prices() {
    let mut svc = service();
    hello_attr(&mut svc);
    let out = ok(&call(
        &mut svc,
        "lab.attribution.design",
        Json::obj([
            ("design", design_json()),
            ("n_fork_points", Json::Int(3)),
        ]),
    ));
    assert!(out.get("design_ref").and_then(Json::as_str).is_some());
    // (1 target · 3 forks + 1) · 2 arms · k=4 = 32.
    assert_eq!(
        out.get("estimate_rollouts").and_then(Json::as_int),
        Some(32)
    );
    let plan = match out.get("plan") {
        Some(Json::Arr(a)) => a.clone(),
        _ => panic!("plan"),
    };
    assert_eq!(plan.len(), 8, "run_start ⇒ 1 fork — 1 target · 2 arms · k=4");
    // The typed refusal surface — MissingMatchSpec is a Refused, never
    // a coerced default (T-LCD-14).
    let mut bad = design_json();
    if let Json::Obj(m) = &mut bad {
        m.remove("match");
    }
    let r = call(
        &mut svc,
        "lab.attribution.design",
        Json::obj([("design", bad)]),
    );
    assert_eq!(err_kind(&r), "Refused", "{r:?}");
    assert_eq!(err_reason(&r), "missing_match_spec");
    // Underpowered — k below the floor.
    let mut weak = design_json();
    if let Json::Obj(m) = &mut weak {
        m.insert("k".into(), Json::Int(2));
    }
    let r = call(
        &mut svc,
        "lab.attribution.design",
        Json::obj([("design", weak)]),
    );
    assert_eq!(err_kind(&r), "Refused");
    assert_eq!(err_reason(&r), "underpowered");
    // A malformed design member is a SchemaViolation.
    let mut bogus = design_json();
    if let Json::Obj(m) = &mut bogus {
        m.insert("unknown_member".into(), Json::Bool(true));
    }
    assert_eq!(
        err_kind(&call(
            &mut svc,
            "lab.attribution.design",
            Json::obj([("design", bogus)]),
        )),
        "SchemaViolation"
    );
}

/// `lab.attribution.attribute` folds the supplied `ArmOutcome` records
/// through the M2 fold — the planted effect is named with its interval.
#[test]
fn s6_3b_attribution_attribute_folds_the_report() {
    let mut svc = service();
    hello_attr(&mut svc);
    let out = ok(&call(
        &mut svc,
        "lab.attribution.attribute",
        Json::obj([
            ("design", design_json()),
            ("outcomes", outcomes_json(1_000_000, 0, 4)),
        ]),
    ));
    let report = out.get("report").unwrap();
    assert_eq!(
        report.get("schema").and_then(Json::as_str),
        Some("hh-attribution/1")
    );
    assert_eq!(
        report.get("estimand").and_then(Json::as_str),
        Some("TE_crn")
    );
    assert_eq!(
        report.get("attribution_label").and_then(Json::as_str),
        Some("causal_coupled")
    );
    assert_eq!(
        report.get("label").and_then(Json::as_str),
        Some("confirmatory")
    );
    let e = match report.get("effects") {
        Some(Json::Arr(a)) => a[0].clone(),
        _ => panic!("effects"),
    };
    assert_eq!(e.get("point").and_then(Json::as_int), Some(-1_000_000));
    assert_eq!(
        e.get("validity_mode").and_then(Json::as_str),
        Some("deterministic")
    );
    assert!(report.get("report_id").and_then(Json::as_str).is_some());
    assert_eq!(
        report
            .get("provenance")
            .and_then(|p| p.get("origin"))
            .and_then(Json::as_str),
        Some("instrument")
    );
    // `locus` — the report's own member and the projection op agree.
    let l = ok(&call(
        &mut svc,
        "lab.attribution.locus",
        Json::obj([("report", report.clone())]),
    ));
    assert_eq!(
        l.get("locus")
            .and_then(|x| x.get("rule"))
            .and_then(Json::as_str),
        Some("point_of_commitment")
    );
    // `quality` — a single-target total attribution is 1.0.
    let q = ok(&call(
        &mut svc,
        "lab.attribution.quality",
        Json::obj([
            (
                "delta",
                Json::obj([
                    ("point", Json::Int(1_000_000)),
                    (
                        "interval",
                        Json::obj([
                            ("lo", Json::Int(800_000)),
                            ("hi", Json::Int(1_000_000)),
                        ]),
                    ),
                ]),
            ),
            ("reports", Json::Arr(vec![report.clone()])),
        ]),
    ));
    assert_eq!(
        q.get("metric")
            .and_then(|m| m.get("point"))
            .and_then(Json::as_int),
        Some(1_000_000)
    );
    // The n/a ladder — hosted ⇒ class; Δ through 0 ⇒ estimator_undefined.
    let q = ok(&call(
        &mut svc,
        "lab.attribution.quality",
        Json::obj([
            ("delta", Json::obj([("point", Json::Int(1))])),
            ("hosted", Json::Bool(true)),
        ]),
    ));
    assert_eq!(
        q.get("metric").and_then(|m| m.get("n/a")).and_then(Json::as_str),
        Some("class")
    );
}

/// `attribute` carries the V-leg refusals — a collapsed arm refuses,
/// an uncoupled source refuses, an invalid branch refuses.
#[test]
fn s6_3b_attribution_v_legs_at_the_boundary() {
    let mut svc = service();
    hello_attr(&mut svc);
    // PolicyCollapsed — the factual arm never changed action.
    let mut collapsed = outcomes_json(1_000_000, 0, 4);
    if let Json::Arr(a) = &mut collapsed {
        for o in a.iter_mut() {
            if let Json::Obj(m) = o {
                m.insert("change_rate_ppm".into(), Json::Int(0));
            }
        }
    }
    let r = call(
        &mut svc,
        "lab.attribution.attribute",
        Json::obj([
            ("design", design_json()),
            ("outcomes", collapsed),
        ]),
    );
    assert_eq!(err_kind(&r), "Refused", "{r:?}");
    assert_eq!(err_reason(&r), "policy_collapsed");
    // CouplingUnavailable — a consumed source the design did not declare.
    let mut uncoupled = outcomes_json(1_000_000, 0, 4);
    if let Json::Arr(a) = &mut uncoupled {
        for o in a.iter_mut() {
            if let Json::Obj(m) = o {
                m.insert(
                    "consumed_sources".into(),
                    Json::Arr(vec![Json::str("sneaky_rng")]),
                );
            }
        }
    }
    let r = call(
        &mut svc,
        "lab.attribution.attribute",
        Json::obj([
            ("design", design_json()),
            ("outcomes", uncoupled),
        ]),
    );
    assert_eq!(err_reason(&r), "coupling_unavailable");
    // ReplayInvalid — an invalid branch.
    let mut invalid = outcomes_json(1_000_000, 0, 4);
    if let Json::Arr(a) = &mut invalid {
        if let Json::Obj(m) = &mut a[0] {
            m.insert("validity".into(), Json::str("invalid"));
        }
    }
    let r = call(
        &mut svc,
        "lab.attribution.attribute",
        Json::obj([
            ("design", design_json()),
            ("outcomes", invalid),
        ]),
    );
    assert_eq!(err_reason(&r), "replay_invalid");
    // A cell outside the plan refuses (join integrity).
    let extra = Json::Arr(vec![Json::obj([
        ("target", Json::str("not-in-the-plan")),
        ("fork_point", Json::Int(0)),
        ("role", Json::str("factual")),
        ("replicate_index", Json::Int(0)),
        ("outcome", Json::Int(1)),
        ("spend", Json::Int(0)),
    ])]);
    let r = call(
        &mut svc,
        "lab.attribution.attribute",
        Json::obj([
            ("design", design_json()),
            ("outcomes", extra),
        ]),
    );
    assert_eq!(err_reason(&r), "cell_out_of_plan");
}
