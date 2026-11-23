//! R6.52 (manifest 325a) repair battery — the W4 driver's transport
//! onto the one continuation module (`continuation_adapter` +
//! `fp-continuation`). Parity: every adapter response equals the
//! canonical `run_checkpoint` / `admit_slot` / `native_lane_hold`
//! output over the same decoded input (no second formula, CC1/X-04).
//! Semantics pinned against the canonical rule, not a copied formula:
//! ≥3 vs ≥10 measurement boundaries, integer round-up, native/V5 wall
//! mapping to mini, orphan + C×s reserve, protected-pair breach,
//! repeated boundaries, declared-cap exhaustion, per-arm holds, and
//! outcome-field / missing-input refusal. USD $0 — fixture-verified.

use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command, Stdio};

use hharness_runner::continuation::{run_checkpoint, ContinuationInput, DROP_LABEL};
use hharness_runner::continuation_adapter::{handle, respond, wall_from_json, ADAPTER_SCHEMA};
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::Json;

const ARMS: [&str; 5] = ["mini-swe-agent", "hh_native", "pi", "claude-code", "v5"];

fn ids(arm: &str, n: usize, tag: &str) -> Json {
    Json::Arr(
        (0..n)
            .map(|i| Json::str(format!("w4-{arm}-{tag}{i}")))
            .collect(),
    )
}

fn trials(arm: &str, cost: i64, wall: i64, n: usize) -> Json {
    Json::Arr(
        (0..n)
            .map(|i| {
                Json::obj([
                    ("slot", Json::str(format!("w4-{arm}-m{i}"))),
                    ("cost_micros", Json::Int(cost)),
                    ("wall_ms", Json::Int(wall)),
                    ("queue_wait_ms", Json::Int(10)),
                    ("latency_ms", Json::Int(90)),
                    ("infra_failed", Json::Bool(false)),
                ])
            })
            .collect(),
    )
}

fn priority() -> Json {
    Json::obj([
        (
            "protected",
            Json::Arr(vec![Json::str("mini-swe-agent"), Json::str("hh_native")]),
        ),
        (
            "drop_order",
            Json::Arr(vec![
                Json::str("v5"),
                Json::str("claude-code"),
                Json::str("pi"),
            ]),
        ),
    ])
}

/// A W4-shaped checkpoint input: mini/hh_native/v5 remaining `n` each,
/// Pi/CC absent (not_run{admission:AG3}); measured rows per arm.
fn input(
    after_round: i64,
    n_remaining: usize,
    measured: &[(&str, Json)],
    consumed: i64,
    orphaned: i64,
    root: i64,
) -> Json {
    let remaining: BTreeMap<String, Json> = ["mini-swe-agent", "hh_native", "v5"]
        .iter()
        .map(|a| (a.to_string(), ids(a, n_remaining, "r")))
        .collect();
    let measured_m: BTreeMap<String, Json> = measured
        .iter()
        .map(|(a, j)| (a.to_string(), j.clone()))
        .collect();
    let priors: BTreeMap<String, Json> = ARMS
        .iter()
        .map(|a| (a.to_string(), Json::Int(374_000)))
        .collect();
    Json::obj([
        ("schema", Json::str("hh-fp-continuation-input/1")),
        ("scope", Json::str("w4_checkpoint")),
        ("after_round", Json::Int(after_round)),
        ("measured", Json::Obj(measured_m)),
        ("remaining", Json::Obj(remaining)),
        ("consumed_by_arm", Json::Obj(BTreeMap::new())),
        ("in_flight", Json::Obj(BTreeMap::new())),
        ("consumed_micros", Json::Int(consumed)),
        ("orphaned_hi_micros", Json::Int(orphaned)),
        ("root_cap_micros", Json::Int(root)),
        ("c", Json::Int(4)),
        ("slice_micros", Json::Int(850_000)),
        ("priors_micros", Json::Obj(priors)),
        (
            "client_timeouts_ms",
            Json::obj([("proxy_upstream", Json::Int(600_000))]),
        ),
        ("priority", priority()),
    ])
}

fn wall(d_limit_ms: i64) -> Json {
    Json::obj([
        ("d_limit_ms", Json::Int(d_limit_ms)),
        ("cutoff_ms", Json::Int(2_340_000)),
        ("overhead_ms", Json::Int(0)),
        ("load_ms", Json::Int(0)),
        ("pairs", Json::Arr(vec![])),
    ])
}

fn checkpoint_req(input: Json, wall: Option<Json>) -> Json {
    Json::obj([
        ("schema", Json::str(ADAPTER_SCHEMA)),
        ("op", Json::str("checkpoint")),
        ("input", input),
        ("wall", wall.unwrap_or(Json::Null)),
    ])
}

/// The adapter outcome **and** the canonical outcome over the same
/// decoded input — they must be byte-identical.
fn both(input_j: Json, wall_j: Option<Json>) -> Json {
    let req = checkpoint_req(input_j.clone(), wall_j.clone());
    let adapter = handle(&req).expect("adapter refused");
    let decoded = ContinuationInput::from_json(&input_j).unwrap();
    let w = wall_j.map(|w| wall_from_json(&w).unwrap());
    let canonical = run_checkpoint(&decoded, w.as_ref()).unwrap().to_json();
    let got = adapter.get("outcome").unwrap().clone();
    assert_eq!(
        got.to_canonical_string(),
        canonical.to_canonical_string(),
        "adapter drifted from the canonical checkpoint"
    );
    got
}

fn p_hat(outcome: &Json, arm: &str) -> i64 {
    outcome
        .get("projection")
        .and_then(|p| p.get(arm))
        .and_then(|a| a.get("p_hat_micros"))
        .and_then(Json::as_int)
        .unwrap()
}

fn t_hat(outcome: &Json, arm: &str) -> i64 {
    outcome
        .get("projection")
        .and_then(|p| p.get(arm))
        .and_then(|a| a.get("t_hat_ms"))
        .and_then(Json::as_int)
        .unwrap()
}

fn withheld_arms(outcome: &Json) -> Vec<(String, String)> {
    outcome
        .get("withheld")
        .and_then(Json::as_arr)
        .unwrap()
        .iter()
        .map(|w| {
            (
                w.get("arm").and_then(Json::as_str).unwrap().to_string(),
                w.get("disposition")
                    .and_then(Json::as_str)
                    .unwrap()
                    .to_string(),
            )
        })
        .collect()
}

const ROOT: i64 = 64_188_120;

#[test]
fn cost_projection_keeps_the_prior_below_ten_trials_and_rounds_up() {
    // 9 trials → prior; 10 trials → ceil(1.2 × max(ceil(1.5 × mean), P90)).
    let nine = both(
        input(
            2,
            30,
            &[("mini-swe-agent", trials("mini-swe-agent", 100_001, 1, 9))],
            0,
            0,
            ROOT,
        ),
        None,
    );
    assert_eq!(p_hat(&nine, "mini-swe-agent"), 374_000);
    let ten = both(
        input(
            2,
            30,
            &[("mini-swe-agent", trials("mini-swe-agent", 100_001, 1, 10))],
            0,
            0,
            ROOT,
        ),
        None,
    );
    // ceil(1.5 × 100 001) = 150 002; ceil(1.2 × 150 002) = 180 003 (integer µ$).
    assert_eq!(p_hat(&ten, "mini-swe-agent"), 180_003);
}

#[test]
fn wall_projection_uses_three_trials_and_maps_the_native_lane_to_mini() {
    // The original driver used ≥10 trials and 1.2 × max(1.5 × mean, P90);
    // the canonical rule is ceil(1.1 × mean) over ≥ 3, native/V5 → mini.
    let two = both(
        input(
            2,
            30,
            &[("mini-swe-agent", trials("mini-swe-agent", 1, 1_000, 2))],
            0,
            0,
            ROOT,
        ),
        None,
    );
    assert_eq!(
        t_hat(&two, "mini-swe-agent"),
        1_040_000,
        "prior below 3 trials"
    );
    let ten = both(
        input(
            2,
            30,
            &[
                ("mini-swe-agent", trials("mini-swe-agent", 1, 1_000, 10)),
                ("hh_native", trials("hh_native", 1, 999_999, 10)),
            ],
            0,
            0,
            ROOT,
        ),
        None,
    );
    assert_eq!(t_hat(&ten, "mini-swe-agent"), 1_100);
    assert_eq!(
        t_hat(&ten, "hh_native"),
        1_100,
        "native lane takes mini's t̂"
    );
    assert_eq!(t_hat(&ten, "v5"), 1_100, "V5 takes mini's t̂");
}

#[test]
fn orphans_and_slice_headroom_reduce_the_room() {
    // 90 remaining at the prior = 33 660 000; rhs = root − consumed − orphans − 4 × 850 000.
    let ok = both(input(2, 30, &[], 0, 0, 37_060_000), None);
    assert!(withheld_arms(&ok).is_empty(), "exactly fits: {ok:?}");
    let fit = ok.get("cost_fit").unwrap();
    assert_eq!(
        fit.get("rhs_micros").and_then(Json::as_int),
        Some(33_660_000)
    );
    // One orphaned µ$ at bounded.hi tips it: V5 drops first, budget-typed.
    let tipped = both(input(2, 30, &[], 0, 1, 37_060_000), None);
    assert_eq!(
        withheld_arms(&tipped),
        vec![("v5".to_string(), "withheld{budget}".to_string())]
    );
    assert_eq!(tipped.get("label").and_then(Json::as_str), Some(DROP_LABEL));
}

#[test]
fn protected_pair_breach_withholds_every_remaining_slot_and_refuses_inference() {
    // Consumed spend leaves no room even for the protected pair.
    let out = both(input(3, 20, &[], ROOT, 0, ROOT), None);
    let arms = withheld_arms(&out);
    for a in ["mini-swe-agent", "hh_native", "v5"] {
        assert!(
            arms.iter().any(|(x, _)| x == a),
            "{a} not withheld: {arms:?}"
        );
    }
    let continuing = out.get("continuing").and_then(Json::as_obj).unwrap();
    assert!(continuing.values().all(|v| v.as_arr().unwrap().is_empty()) || continuing.is_empty());
    let refusals = out
        .get("inference_refusals")
        .and_then(Json::as_arr)
        .unwrap();
    assert_eq!(refusals.len(), 3);
}

#[test]
fn repeated_boundaries_react_to_later_cost() {
    // After round 2 the step fits; after round 3 a costlier measured
    // mini sample (≥ 10 trials) no longer fits → V5 first.
    let r2 = both(input(2, 30, &[], 4_000_000, 0, ROOT), None);
    assert!(withheld_arms(&r2).is_empty());
    let r3 = both(
        input(
            3,
            20,
            &[
                ("mini-swe-agent", trials("mini-swe-agent", 900_000, 1, 10)),
                ("hh_native", trials("hh_native", 900_000, 1, 10)),
                ("v5", trials("v5", 900_000, 1, 10)),
            ],
            30_000_000,
            0,
            ROOT,
        ),
        None,
    );
    assert_eq!(withheld_arms(&r3)[0].0, "v5");
    assert_eq!(r3.get("scope").and_then(Json::as_str), Some("w4_round_3"));
}

#[test]
fn wall_shortfall_drops_v5_window_deadline_typed() {
    // Prior t̂ 1 040 000 ms; 90 remaining → Σ (30 + 3) × 1 040 000 × 3 / 4.
    let out = both(input(2, 30, &[], 0, 0, ROOT), Some(wall(28_000_000)));
    assert_eq!(
        withheld_arms(&out).first().cloned(),
        Some(("v5".to_string(), "withheld{window_deadline}".to_string()))
    );
}

#[test]
fn outcome_fields_and_missing_pinned_inputs_refuse() {
    let mut bad = input(2, 30, &[], 0, 0, ROOT);
    if let Json::Obj(m) = &mut bad {
        m.insert("verdict".into(), Json::str("pass"));
    }
    assert!(handle(&checkpoint_req(bad, None)).is_err());
    // An outcome member smuggled into the envelope itself also refuses.
    let mut env = checkpoint_req(input(2, 30, &[], 0, 0, ROOT), None);
    if let Json::Obj(m) = &mut env {
        m.insert("reward".into(), Json::Int(1));
    }
    assert!(handle(&env).is_err());
    // A wall input missing a pinned member is a refusal, never zero.
    let mut w = wall(28_000_000);
    if let Json::Obj(m) = &mut w {
        m.remove("load_ms");
    }
    assert!(handle(&checkpoint_req(input(2, 30, &[], 0, 0, ROOT), Some(w))).is_err());
    // Unknown op / schema refuse.
    let (body, ok) = respond(r#"{"schema":"hh-fp-continuation-adapter/1","op":"guess"}"#);
    assert!(!ok && body.get("refused").is_some());
    let (_, ok) = respond("not json");
    assert!(!ok);
}

fn admit_req(arm: &str, consumed_v5: i64, in_flight: &[(&str, i64)], caps: Json) -> Json {
    let mut inp = input(2, 30, &[], 0, 0, ROOT);
    if let Json::Obj(m) = &mut inp {
        m.insert(
            "consumed_by_arm".into(),
            Json::obj([("v5", Json::Int(consumed_v5))]),
        );
        m.insert(
            "in_flight".into(),
            Json::obj([(
                "v5",
                Json::Arr(
                    in_flight
                        .iter()
                        .map(|(s, c)| {
                            Json::obj([("slot", Json::str(*s)), ("consumed_micros", Json::Int(*c))])
                        })
                        .collect(),
                ),
            )]),
        );
    }
    Json::obj([
        ("schema", Json::str(ADAPTER_SCHEMA)),
        ("op", Json::str("admit")),
        ("input", inp),
        ("arm", Json::str(arm)),
        ("caps_micros", caps),
    ])
}

#[test]
fn declared_cap_admission_counts_in_flight_and_never_recomputes_the_cap() {
    let caps = Json::obj([("v5", Json::Int(1_000_000))]);
    let v = |j: Json| {
        handle(&j)
            .unwrap()
            .get("admission")
            .and_then(|a| a.get("verdict"))
            .and_then(Json::as_str)
            .unwrap()
            .to_string()
    };
    // consumed 0 + in-flight max(p̂, 0) + p̂ = 748 000 ≤ 1 000 000 → admit.
    assert_eq!(v(admit_req("v5", 0, &[("a", 0)], caps.clone())), "admit");
    // A second in-flight slot exhausts the declared cap → withheld{budget}.
    let capped = handle(&admit_req("v5", 0, &[("a", 0), ("b", 0)], caps.clone())).unwrap();
    let adm = capped.get("admission").unwrap();
    assert_eq!(adm.get("verdict").and_then(Json::as_str), Some("capped"));
    assert_eq!(
        adm.get("disposition").and_then(Json::as_str),
        Some("withheld{budget}")
    );
    // A protected arm is bounded by root + checkpoint only.
    assert_eq!(
        v(admit_req("hh_native", 0, &[], caps.clone())),
        "protected_by_root"
    );
    // An unprotected arm without a declared cap refuses (no fallback).
    assert!(handle(&admit_req("v5", 0, &[], Json::Obj(BTreeMap::new()))).is_err());
}

fn hold_req(arm: &str, probe: bool, first3: &[bool]) -> Json {
    Json::obj([
        ("schema", Json::str(ADAPTER_SCHEMA)),
        ("op", Json::str("native_lane_hold")),
        ("arm", Json::str(arm)),
        ("probe_passed", Json::Bool(probe)),
        (
            "first3_infra",
            Json::Arr(first3.iter().map(|b| Json::Bool(*b)).collect()),
        ),
        ("remaining", ids(arm, 3, "h")),
        ("cause", Json::str("fixture")),
    ])
}

#[test]
fn native_lane_hold_is_per_arm_and_immediate() {
    // One infra-failed slot among the first admitted holds that arm —
    // without waiting for three observations.
    let held = handle(&hold_req("v5", true, &[true])).unwrap();
    let rec = held.get("hold").unwrap();
    assert_eq!(rec.get("arm").and_then(Json::as_str), Some("v5"));
    assert_eq!(
        rec.get("disposition").and_then(Json::as_str),
        Some("withheld{admission:native_lane_hold}")
    );
    // The other native arm is untouched by V5's observation.
    let clear = handle(&hold_req("hh_native", true, &[false, false])).unwrap();
    assert_eq!(clear.get("hold"), Some(&Json::Null));
    // A failed probe holds the native arm; a hosted arm refuses typed.
    assert!(handle(&hold_req("hh_native", false, &[]))
        .unwrap()
        .get("hold")
        .unwrap()
        .get("arm")
        .is_some());
    assert!(handle(&hold_req("mini-swe-agent", true, &[true])).is_err());
    assert!(handle(&hold_req("v5", true, &[false, false, false, false])).is_err());
}

#[test]
fn the_binary_transports_byte_identically_and_exits_typed() {
    let exe = env!("CARGO_BIN_EXE_fp_continuation");
    let req = checkpoint_req(input(2, 30, &[], 0, 1, 37_060_000), None);
    let run = |text: String| {
        let mut child = Command::new(exe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        (out.status.code(), String::from_utf8(out.stdout).unwrap())
    };
    let (code, stdout) = run(req.to_canonical_string());
    assert_eq!(code, Some(0));
    assert_eq!(
        stdout.trim_end(),
        handle(&req).unwrap().to_canonical_string()
    );
    let (code, stdout) = run("{}".to_string());
    assert_eq!(code, Some(2));
    assert!(stdout.contains("refused"));
}

// ── R6.52 review fixes (2026-11-23): strict envelope + canonical probe ──

#[test]
fn duplicate_keys_and_noncanonical_requests_refuse() {
    let req = checkpoint_req(input(2, 30, &[], 0, 0, ROOT), None);
    let canon = req.to_canonical_string();
    let (_, ok) = respond(&canon);
    assert!(ok, "the canonical request is accepted");
    // A duplicated member (last-wins in the lenient reader) refuses.
    let dup = canon.replacen("{\"input\":", "{\"op\":\"admit\",\"input\":", 1);
    assert_ne!(dup, canon);
    let (body, ok) = respond(&dup);
    assert!(!ok && body.get("refused").is_some(), "{body:?}");
    // Whitespace / non-canonical spelling refuses too.
    let (_, ok) = respond(&format!(" {canon}"));
    assert!(!ok);
}

#[test]
fn unknown_envelope_members_refuse() {
    let mut req = checkpoint_req(input(2, 30, &[], 0, 0, ROOT), None);
    if let Json::Obj(m) = &mut req {
        m.insert("bogus".into(), Json::Int(1));
    }
    let err = handle(&req).unwrap_err().to_string();
    assert!(err.contains("bogus"), "{err}");
    let mut w = wall(28_000_000);
    if let Json::Obj(m) = &mut w {
        m.insert("slack_ms".into(), Json::Int(0));
    }
    assert!(handle(&checkpoint_req(input(2, 30, &[], 0, 0, ROOT), Some(w))).is_err());
}

fn pricing(input_per_1m: i64, output_per_1m: i64) -> Json {
    Json::obj([
        ("schema", Json::str("hh-w4-probe-pricing/1")),
        (
            "model_ref",
            Json::obj([
                ("profile_ref", Json::str("glm-5.3-flash")),
                ("provider_model_id", Json::str("glm-5.3-flash")),
                ("serving_route", Json::str("ollama")),
            ]),
        ),
        (
            "table",
            Json::obj([
                ("table_id", Json::str("ollama")),
                ("version", Json::str("2026-10-22")),
                ("currency", Json::str("USD")),
                ("rate_unit", Json::str("micro_per_1k_tokens")),
                ("input_micro_per_1m", Json::Int(input_per_1m)),
                ("cache_read_micro_per_1m", Json::Int(30_000)),
                ("output_micro_per_1m", Json::Int(output_per_1m)),
                ("valid_from", Json::str("2026-10-22")),
                ("source", Json::str("fixture")),
            ]),
        ),
    ])
}

fn plan_req(p: Json, efforts: &[&str]) -> Json {
    Json::obj([
        ("schema", Json::str(ADAPTER_SCHEMA)),
        ("op", Json::str("lane_probe_plan")),
        ("pricing", p),
        ("surface", Json::str("openai.chat")),
        (
            "effort_paths",
            Json::Arr(efforts.iter().map(|e| Json::str(*e)).collect()),
        ),
    ])
}

#[test]
fn lane_probe_plan_prices_the_whole_plan_before_any_call() {
    let out = handle(&plan_req(pricing(150_000, 500_000), &["high", "low"])).unwrap();
    assert_eq!(out.get("calls").and_then(Json::as_arr).unwrap().len(), 2);
    let worst = out.get("worst_case_micros").and_then(Json::as_int).unwrap();
    assert!(worst > 0 && worst <= 50_000, "{worst}");
    assert_eq!(out.get("cap_micros").and_then(Json::as_int), Some(50_000));
    // A plan priced above $0.05 refuses before any call.
    let err = handle(&plan_req(pricing(150_000_000_000, 500_000), &["high"]))
        .unwrap_err()
        .to_string();
    assert!(err.contains("plan_bound"), "{err}");
    // An unrepresentable rate refuses typed (never rounded).
    assert!(handle(&plan_req(pricing(150_001, 500_000), &["high"])).is_err());
    // No effort path → refused.
    assert!(handle(&plan_req(pricing(150_000, 500_000), &[])).is_err());
}

fn addr(bytes: &[u8]) -> String {
    hharness_identity::idp::address(bytes, "application/json").id()
}

struct Obs {
    receipt: String,
    body: String,
    rref: String,
}

fn obs(slot: &str, run: &str, effort: &str, route: &str) -> Obs {
    let body = format!(
        "{{\"model\": \"glm-5.3-flash\", \"reasoning_effort\": \"{effort}\", \"max_tokens\": 8}}"
    );
    let receipt = Json::obj([(
        "proxy",
        Json::obj([
            ("slot_id", Json::str(slot)),
            ("phase_run_id", Json::str(run)),
            (
                "downstream",
                Json::obj([
                    ("path", Json::str(route)),
                    ("body_ref", Json::str(addr(body.as_bytes()))),
                ]),
            ),
        ]),
    )])
    .to_canonical_string();
    let rref = addr(receipt.as_bytes());
    Obs {
        receipt,
        body,
        rref,
    }
}

fn record_req(o: &Obs, rref: &str, http: i64, cost: Option<i64>, disposition: &str) -> Json {
    let mut m = match plan_req(pricing(150_000, 500_000), &["high"]) {
        Json::Obj(m) => m,
        _ => unreachable!(),
    };
    m.insert("op".into(), Json::str("lane_probe_record"));
    m.insert(
        "slot".into(),
        Json::obj([
            ("token_ref", Json::str("slot:w4-lane-probe")),
            ("slice_micros", Json::Int(50_000)),
        ]),
    );
    m.insert("recorded_at".into(), Json::str("2026-11-23T00:00:00Z"));
    m.insert("probe_slot_id".into(), Json::str("w4-lane-probe"));
    m.insert("phase_run_id".into(), Json::str("run-1"));
    m.insert(
        "observations".into(),
        Json::Arr(vec![Json::obj([
            ("probe_id", Json::str("lane-probe-high")),
            ("http_status", Json::Int(http)),
            (
                "terminal",
                Json::obj([
                    ("class", Json::str("model.call.completed")),
                    ("disposition", Json::str(disposition)),
                    ("response_ref", Json::str(rref)),
                ]),
            ),
            ("receipt", Json::str(&o.receipt)),
            ("downstream_body", Json::str(&o.body)),
            ("cost_micros", cost.map(Json::Int).unwrap_or(Json::Null)),
        ])]),
    );
    Json::Obj(m)
}

fn stage_call(out: &Json) -> (String, Json) {
    let st = out.get("stage").unwrap();
    let c = st.get("calls").and_then(Json::as_arr).unwrap()[0].clone();
    (
        st.get("disposition")
            .and_then(Json::as_str)
            .unwrap()
            .to_string(),
        c,
    )
}

#[test]
fn lane_probe_record_passes_only_a_verified_bound_receipt() {
    let good = obs("w4-lane-probe", "run-1", "high", "/v1/chat/completions");
    let (d, c) = stage_call(
        &handle(&record_req(
            &good,
            &good.rref,
            200,
            Some(1_234),
            "forwarded",
        ))
        .unwrap(),
    );
    assert_eq!(d, "pass");
    assert_eq!(c.get("cost_micros").and_then(Json::as_int), Some(1_234));
    assert_eq!(
        c.get("receipt_ref").and_then(Json::as_str),
        Some(good.rref.as_str())
    );
    // The canonical stage record (holds.rs) — not a driver-side emitter.
    let st = handle(&record_req(&good, &good.rref, 200, Some(1), "forwarded")).unwrap();
    assert_eq!(
        st.get("stage")
            .and_then(|s| s.get("schema"))
            .and_then(Json::as_str),
        Some("hh-lane-probe/1")
    );
    let cases: Vec<(Json, &str)> = vec![
        // receipt bytes that do not hash to the recorded ref
        (
            record_req(&good, &addr(b"other"), 200, Some(1), "forwarded"),
            "receipt_hash_mismatch",
        ),
        // a receipt bound to another slot / another run
        (
            {
                let o = obs(
                    "w4-mini-swe-agent-t0-r1",
                    "run-1",
                    "high",
                    "/v1/chat/completions",
                );
                record_req(&o, &o.rref, 200, Some(1), "forwarded")
            },
            "receipt_unbound",
        ),
        (
            {
                let o = obs("w4-lane-probe", "run-2", "high", "/v1/chat/completions");
                record_req(&o, &o.rref, 200, Some(1), "forwarded")
            },
            "receipt_unbound",
        ),
        // the downstream call was not the planned effort path / route
        (
            {
                let o = obs("w4-lane-probe", "run-1", "low", "/v1/chat/completions");
                record_req(&o, &o.rref, 200, Some(1), "forwarded")
            },
            "downstream_mismatch",
        ),
        (
            {
                let o = obs("w4-lane-probe", "run-1", "high", "/v1/messages");
                record_req(&o, &o.rref, 200, Some(1), "forwarded")
            },
            "route_mismatch",
        ),
        // not a forwarded 2xx; or over the $0.05 cap
        (
            record_req(&good, &good.rref, 200, Some(1), "upstream_error"),
            "terminal:upstream_error",
        ),
        (
            record_req(&good, &good.rref, 500, Some(1), "forwarded"),
            "http:500",
        ),
        (
            record_req(&good, &good.rref, 200, Some(50_001), "forwarded"),
            "probe_cap_exceeded",
        ),
    ];
    for (req, want) in cases {
        let (d, c) = stage_call(&handle(&req).unwrap());
        assert_eq!(d, "failed", "{want}");
        let got = c
            .get("disposition")
            .and_then(Json::as_str)
            .unwrap()
            .to_string();
        assert!(got.contains(want), "{want}: {got}");
    }
}

#[test]
fn lane_probe_record_keeps_an_unknown_cost_unknown() {
    let good = obs("w4-lane-probe", "run-1", "high", "/v1/chat/completions");
    let (d, c) =
        stage_call(&handle(&record_req(&good, &good.rref, 200, None, "forwarded")).unwrap());
    assert_eq!(d, "pass");
    assert_eq!(c.get("cost_micros"), Some(&Json::Null));
    assert_eq!(
        c.get("cost").and_then(Json::as_str),
        Some("unknown{no_attribution_row}")
    );
}

#[test]
fn probe_precedes_native_is_the_canonical_predicate() {
    let req = |p: i64, n: i64| {
        Json::obj([
            ("schema", Json::str(ADAPTER_SCHEMA)),
            ("op", Json::str("probe_precedes_native")),
            ("probe_end_seq", Json::Int(p)),
            ("first_native_seq", Json::Int(n)),
        ])
    };
    let get = |j: Json| handle(&j).unwrap().get("precedes").and_then(Json::as_bool);
    assert_eq!(get(req(4, 9)), Some(true));
    assert_eq!(get(req(9, 9)), Some(false));
    assert_eq!(get(req(12, 9)), Some(false));
}
