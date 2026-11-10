//! R6.28 acceptance battery (manifest 282; K2+K3+K4 — the L1
//! observation replay, synthetic corpus/census and the L1 runner):
//! AC-1 recorded-response driver (REQ-XE-004); AC-2 observation
//! template/inverse goldens incl. multi-byte boundaries + typed
//! overrun; AC-3 BM1–BM19 fixtures + census generator + coverage;
//! AC-4 legs/first-divergence (REQ-XD-103); AC-5 equality levels
//! (REQ-XD-104); AC-6 records + coverage refusal (REQ-XD-109); AC-7
//! `InauthenticEvidence` (L1 is never parity). USD $0, loopback only.

use std::path::PathBuf;

use hharness_gateway::recording::Recording;
use hharness_runner::json_ext::JsonExt;
use hharness_runner::l1_corpus::{
    bm_fixtures, census_entries, check_corpus, corpus_manifest, BM_BRANCHES, CORPUS_SCHEMA,
};
use hharness_runner::l1_drive::{
    DivergencePlan, HttpEndpoint, RecordedResponseEndpoint, SequenceReplay, ToyMini,
};
use hharness_runner::l1_gate::L1Identity;
use hharness_runner::l1_obs::{
    invert_mini_observation, render_mini_observation, L1Exec, ObsReplay, ObsReplayExecutor,
    RawObservation, SUBMIT_MARKER,
};
use hharness_wire::json::{parse, Json};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("r6_28")
}

fn canon() -> Box<dyn hharness_gateway::replay::RequestCanonicalizer> {
    hharness_gateway::replay::canonicalizer_for("hh-l1-canon/1", &[]).unwrap()
}

fn h_id() -> L1Identity {
    L1Identity {
        participant_ref: "hir:mini-toy".into(),
        version: "toy-h/1.0.0".into(),
        digest: format!("sha256:{}", "ab".repeat(32)),
    }
}
fn o_id() -> L1Identity {
    L1Identity {
        participant_ref: "original:mini-toy".into(),
        version: "toy-o/1.0.0".into(),
        digest: format!("sha256:{}", "cd".repeat(32)),
    }
}

fn raw(out: &str, rc: i64) -> RawObservation {
    RawObservation {
        output: out.to_string(),
        returncode: rc,
        exception_info: None,
    }
}

fn make_o() -> impl Fn(Box<dyn L1Exec>) -> ToyMini {
    |exec| ToyMini::new("toy-o", "solve it", exec)
}
fn make_h() -> impl Fn(Box<dyn L1Exec>) -> ToyMini {
    |exec| ToyMini::new("toy-h", "solve it", exec)
}
fn make_h_div(plan: DivergencePlan) -> impl Fn(Box<dyn L1Exec>) -> ToyMini {
    move |exec| {
        let mut h = ToyMini::new("toy-h", "solve it", exec);
        h.divergence = plan.clone();
        h
    }
}

// ── AC-1 (E; AC-7.1-2) — REQ-XE-004 recorded-response driver ─────────

#[test]
fn ac1_toy_original_drives_through_base_url_recording_bodies() {
    // The recorded-response endpoint on loopback; the toy O drives it
    // through its own base-URL setting over a real socket.
    let rec = Recording::parse(&bm_fixtures()[0].recording.to_json()).unwrap();
    let ep = RecordedResponseEndpoint::bind(SequenceReplay::new(rec.calls.clone())).unwrap();
    let mut o = ToyMini::new(
        "toy-o",
        "solve it",
        Box::new(ObsReplay::new(vec![raw("hi", 0), raw(SUBMIT_MARKER, 0)])),
    );
    o.base_url = format!("http://127.0.0.1:{}", ep.port());
    let mut http = HttpEndpoint::new(ep.port());
    let drive = o.drive(&mut http);
    // ≥ 1 request body per turn recorded by the endpoint.
    let captured = ep.captured();
    assert_eq!(
        captured.len(),
        2,
        "the endpoint records every turn's request"
    );
    assert!(captured.iter().all(|c| !c.body.is_empty()));
    // The drive's own record equals the endpoint's capture.
    assert_eq!(drive.requests.len(), 2);
    assert_eq!(drive.termination, "submitted");
    drop(ep);

    // The committed fixture is the recorded capture (the driver-side
    // golden — the ≥1-request-body-per-turn evidence).
    let golden = Json::obj([
        ("schema", Json::str("hh-l1-drive-capture/1")),
        ("turns", Json::Int(captured.len() as i64)),
        (
            "bodies",
            Json::Arr(
                captured
                    .iter()
                    .map(|c| Json::str(String::from_utf8_lossy(&c.body).into_owned()))
                    .collect(),
            ),
        ),
    ]);
    let golden_path = fixture_dir().join("toy_o_capture.json");
    if std::env::var("HH_L1_WRITE_FIXTURES").is_ok() {
        std::fs::write(&golden_path, golden.to_canonical_string()).unwrap();
    }
    let committed = parse(&std::fs::read_to_string(&golden_path).unwrap()).unwrap();
    assert_eq!(committed, golden, "committed capture drifts from the drive");
}

#[test]
fn ac1_mini_cell_reaches_endpoint_only_via_declared_profile() {
    // The mini-swe-agent cell env (R6.24's renderer + R6.15's guard):
    // the recorded-response endpoint binds the cell's model-relay port
    // (no_proxy-covered loopback) while all other egress is mediated —
    // the declared `allowlist_mediated` profile, never a raw dial.
    let slot = hharness_runner::render::SlotInputs {
        slot_token: "toy-slot-token".to_string(),
        model_port: 4_242,
        instruction: b"do the task".to_vec(),
        home: "/root".to_string(),
        user: "root".to_string(),
        agent_wall_timeout_s: 3600,
        proxy_budget_ceiling_usd_micros: 1,
        slot_id: "slot-fixture".to_string(),
    };
    let rec = fixture_admission();
    let cfg =
        hharness_runner::render::render(&rec, &slot, hharness_runner::render::RenderUse::HostedArm)
            .unwrap();
    let base = cfg.env.get("OPENAI_BASE_URL").unwrap();
    assert!(
        base.starts_with("http://127.0.0.1:4242"),
        "base-url setting is the cell's model-relay loopback: {base}"
    );
    // Other egress is mediated; loopback is the only direct route.
    assert!(cfg.env.get("http_proxy").unwrap().contains("127.0.0.1"));
    assert_eq!(
        cfg.env.get("no_proxy").unwrap(),
        hharness_runner::render::NO_PROXY_VALUE
    );
}

fn fixture_admission() -> Json {
    let p = fixture_dir().join("mini_admission.json");
    parse(&std::fs::read_to_string(p).unwrap()).unwrap()
}

// ── AC-2 (E) — observation template + inverse goldens, overrun ──────

#[test]
fn ac2_observation_template_boundary_goldens() {
    // Below 10 000 characters — exact members.
    let r = render_mini_observation(&raw("out", 7));
    assert_eq!(r, "{\n  \"returncode\": 7,\n  \"output\": \"out\"\n}");
    // exception_info only when truthy.
    let mut e = raw("o", -1);
    e.exception_info = Some(Json::str("timed out"));
    let r = render_mini_observation(&e);
    assert!(r.contains(", \"exception_info\": \"timed out\""));
    // exact inverse below the threshold.
    let inv = invert_mini_observation(&r).unwrap();
    assert_eq!(inv.output, "o");
    assert_eq!(inv.returncode, -1);
    assert_eq!(inv.exception_info, Some(Json::str("timed out")));
    // 9 999 / 10 000 / 10 001 characters — the boundary.
    for (n, want_head_tail) in [(9_999usize, false), (10_000, true), (10_001, true)] {
        let o = "a".repeat(n);
        let rendered = render_mini_observation(&raw(&o, 0));
        if want_head_tail {
            assert!(rendered.contains("\"output_head\""));
            assert!(rendered.contains(&format!("\"elided_chars\": {}", n - 10_000)));
        } else {
            assert!(rendered.contains("\"output\": "));
            assert!(!rendered.contains("output_head"));
        }
        // The inverse re-renders byte-identically.
        let inv = invert_mini_observation(&rendered).unwrap();
        assert_eq!(inv.output.chars().count(), n);
        assert_eq!(render_mini_observation(&inv), rendered);
    }
    // Multi-byte characters straddling the 5 000 boundary — character,
    // never byte, slices (E1 A.5).
    let mut o = "é".repeat(4_999);
    o.push('€');
    o.push_str(&"b".repeat(5_500));
    let rendered = render_mini_observation(&raw(&o, 0));
    let inv = invert_mini_observation(&rendered).unwrap();
    assert_eq!(inv.output.chars().count(), 10_500);
    assert_eq!(render_mini_observation(&inv), rendered);
    // And the head/tail are char-exact.
    let doc = parse(&rendered).unwrap();
    let head = doc.get("output_head").unwrap().as_str().unwrap();
    assert_eq!(head.chars().count(), 5_000);
    assert!(head.ends_with('€'));
}

#[test]
fn ac2_obs_replay_overrun_refuses_typed() {
    let mut rp = ObsReplay::new(vec![raw("x", 0)]);
    assert!(rp.run("cmd").is_ok());
    let e = rp.run("cmd").unwrap_err();
    assert_eq!(e.rule, "observation_overrun");
    // The exec-seam double refuses the same way.
    let mut x = ObsReplayExecutor::new(vec![]);
    let req = exec_request();
    let mut sink = |_: hharness_env::executor::ExecutorSignal| {};
    let e = hharness_env::executor::ToolExecutor::execute(&mut x, &req, &mut sink).unwrap_err();
    assert!(format!("{e}").contains("observation_overrun"));
}

fn exec_request() -> hharness_env::executor::ExecutionRequest {
    hharness_env::executor::ExecutionRequest {
        exec_defaults: None,
        execution_id: "x".into(),
        effect_id: "e".into(),
        attempt_no: 1,
        capability_ref: ("c".into(), "v".into()),
        effect: hharness_hir::kinds::EffectClass::domain_only(
            hharness_hir::kinds::EffectDomain::Exec,
        ),
        args: Json::obj([]),
        env_handle_id: "h".into(),
        attribution_token: "t".into(),
        deadline_ms: None,
        ladder: hharness_env::deadline::DeadlineLadder {
            run_deadline_ms: None,
            phase_deadline_ms: None,
            effect_deadline_ms: None,
            attempt_deadline_ms: None,
        },
        retain_bytes_cap: 1 << 20,
        idempotency_key: "k".into(),
        commit_evidence: hharness_env::helper::CommitEvidence::ReadOnly,
        request_state: None,
    }
}

// ── AC-3 (E) — BM fixtures validate; census; coverage refusal ───────

#[test]
fn ac3_bm_fixtures_load_validate_and_cover() {
    let fixtures = bm_fixtures();
    assert_eq!(fixtures.len(), 19);
    let mut entries = Vec::new();
    for f in &fixtures {
        let doc = f.recording.to_json();
        let back = Recording::parse(&doc).unwrap();
        assert_eq!(back.provenance, "synthetic");
        assert!(!back.calls.is_empty(), "{} carries no calls", f.id);
        entries.push((f.id, f.branches));
        // The committed file is byte-exact with the generator.
        let path = fixture_dir().join(format!("{}.json", f.id));
        if std::env::var("HH_L1_WRITE_FIXTURES").is_ok() {
            std::fs::write(&path, doc.to_canonical_string()).unwrap();
        }
        let committed = parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(committed, doc, "{} drifts from the generator", f.id);
    }
    // Every BM branch is covered by exactly the declared entries.
    let covered: std::collections::BTreeSet<&str> = entries
        .iter()
        .flat_map(|(_, b)| b.iter().copied())
        .collect();
    for (bm, _) in BM_BRANCHES {
        assert!(covered.contains(bm), "{bm} uncovered");
    }
    // The corpus manifest validates against the coverage check.
    let corpus = corpus_manifest(entries.clone(), vec!["t1".into(), "t2".into()]);
    assert_eq!(
        corpus.get("schema").unwrap().as_str().unwrap(),
        CORPUS_SCHEMA
    );
    let cov = check_corpus(&corpus, &["t1".to_string(), "t2".to_string()]).unwrap();
    assert_eq!(
        cov.get("BM19").unwrap().as_str().unwrap(),
        "bm19",
        "coverage maps the branch to its fixture"
    );
    // A missing branch refuses the gate.
    let missing = corpus_manifest(entries[..18].to_vec(), vec!["t1".into()]);
    let e = check_corpus(&missing, &["t1".to_string()]).unwrap_err();
    assert_eq!(e.rule, "coverage");
    // A missing census task refuses the gate.
    let e = check_corpus(&corpus, &["t1".to_string(), "t3".to_string()]).unwrap_err();
    assert_eq!(e.rule, "coverage");
}

#[test]
fn ac3_census_generator_emits_one_entry_per_task() {
    let manifest = Json::obj([(
        "tasks",
        Json::Arr(vec![
            Json::obj([
                ("task_id", Json::str("tb-1")),
                ("instruction", Json::str("fix 'quoting' & <chars>")),
            ]),
            Json::obj([
                ("task_id", Json::str("tb-2")),
                ("instruction", Json::str("plain task")),
            ]),
        ]),
    )]);
    let entries = census_entries(&manifest).unwrap();
    assert_eq!(entries.len(), 2);
    for (i, e) in entries.iter().enumerate() {
        let seq = e.get("sequence").unwrap();
        let rec = Recording::parse(seq).unwrap();
        assert_eq!(rec.provenance, "synthetic");
        assert_eq!(rec.calls.len(), 1);
        let body = String::from_utf8_lossy(&rec.calls[0].downstream_body).into_owned();
        let doc = parse(&body).unwrap();
        let user = &doc.get("messages").unwrap().as_arr().unwrap()[1];
        assert!(user
            .get("content")
            .unwrap()
            .as_str()
            .unwrap()
            .contains(if i == 0 {
                "fix 'quoting'"
            } else {
                "plain task"
            }));
        // The response submits at once.
        let resp = String::from_utf8_lossy(&rec.calls[0].upstream_response.as_ref().unwrap().body)
            .into_owned();
        assert!(resp.contains(SUBMIT_MARKER));
    }
}

// ── AC-4/5/6/7 — stage 2 (K4) appended below ────────────────────────

use hharness_eval::parity_admission as pa;
use hharness_runner::l1_drive::L1Drive;
use hharness_runner::l1_gate::{
    check_l1_conformance, check_l1_diff, compare_drives, conformance_record, refuse_parity_input,
    run_sequence, L1Sequence, L1_CONFORMANCE_SCHEMA, L1_DIFF_SCHEMA, L1_LABEL,
};

fn bm01_seq(obs_b: Option<Vec<RawObservation>>) -> L1Sequence {
    let rec = bm_fixtures()[0].recording.to_json();
    let rec = Recording::parse(&rec).unwrap();
    L1Sequence {
        seq_id: "bm01".into(),
        calls: rec.calls.clone(),
        obs_a: vec![raw("hi\n", 0), raw(&format!("{SUBMIT_MARKER}\n"), 0)],
        obs_b,
    }
}

#[test]
fn ac4_legs_stable_prefix_and_first_divergence() {
    // L1.0 defines s(S): obs_b diverges at step 1 → request 2 differs → s = 1.
    let seq = bm01_seq(Some(vec![
        raw("DIFFERENT\n", 0),
        raw(&format!("{SUBMIT_MARKER}\n"), 0),
    ]));
    let out = run_sequence(
        &seq,
        &make_o(),
        &make_h(),
        "hh-l1-canon/1",
        &[],
        "openai.chat",
        "2026-11-08T00:00:00Z",
        &h_id(),
        &o_id(),
    );
    assert_eq!(out.stable, 1, "s(S) counts leading equal requests");
    assert_eq!(out.total, 2);

    // A faithful H passes every leg when the env is reproducible.
    let seq = bm01_seq(None);
    let out = run_sequence(
        &seq,
        &make_o(),
        &make_h(),
        "hh-l1-canon/1",
        &[],
        "openai.chat",
        "2026-11-08T00:00:00Z",
        &h_id(),
        &o_id(),
    );
    assert_eq!(out.stable, out.total);
    assert_eq!(out.l1_1.result, "pass");
    assert_eq!(out.l1_2.result, "pass");
    assert!(out.diffs.is_empty());

    // Injected request divergence at turn 2 → detected at the right
    // step and leg (strict L1.1 refuses; the diff record names it).
    let mut plan = DivergencePlan::default();
    plan.request_patch = Some((2, vec![("temperature".into(), Json::Int(1))]));
    let out = run_sequence(
        &seq,
        &make_o(),
        &make_h_div(plan),
        "hh-l1-canon/1",
        &[],
        "openai.chat",
        "2026-11-08T00:00:00Z",
        &h_id(),
        &o_id(),
    );
    assert_eq!(out.l1_1.result, "fail");
    let d = out
        .l1_1
        .divergences
        .iter()
        .find(|d| d.gated && d.level == "request")
        .expect("the strict refusal records a request-level divergence");
    assert_eq!(d.turn, Some(2));
    assert!(out.diffs.iter().any(|r| {
        r.get("leg").unwrap().as_str() == Some("l1_1")
            && r.get("k").and_then(Json::as_int) == Some(2)
    }));
}

#[test]
fn ac4_l12_gates_inside_prefix_reports_beyond() {
    // s(S) = 1 (obs_b diverges at step 1): a divergence at k ≤ s fails;
    // beyond s is reported, never evaluated.
    let seq = bm01_seq(Some(vec![
        raw("DIFFERENT\n", 0),
        raw(&format!("{SUBMIT_MARKER}\n"), 0),
    ]));
    // Divergence at turn 1 (within the stable prefix) → fail.
    let mut plan = DivergencePlan::default();
    plan.request_patch = Some((1, vec![("temperature".into(), Json::Int(1))]));
    let out = run_sequence(
        &seq,
        &make_o(),
        &make_h_div(plan),
        "hh-l1-canon/1",
        &[],
        "openai.chat",
        "2026-11-08T00:00:00Z",
        &h_id(),
        &o_id(),
    );
    assert_eq!(out.l1_2.result, "fail");
    // Divergence only at turn 2 (= s+1, the excluded turn) → reported,
    // never a fail.
    let mut plan = DivergencePlan::default();
    plan.request_patch = Some((2, vec![("temperature".into(), Json::Int(1))]));
    let out = run_sequence(
        &seq,
        &make_o(),
        &make_h_div(plan),
        "hh-l1-canon/1",
        &[],
        "openai.chat",
        "2026-11-08T00:00:00Z",
        &h_id(),
        &o_id(),
    );
    assert_eq!(out.stable, 1);
    assert_eq!(
        out.l1_2.result, "pass",
        "post-prefix divergence reports only"
    );
    assert!(out
        .l1_2
        .divergences
        .iter()
        .any(|d| !d.gated && d.turn == Some(2)));
}

// ── AC-5 (E; AC-7.4-4) — one failing-alone test per equality level ──

fn base_drive() -> L1Drive {
    let mut d = L1Drive::default();
    for (i, cmd) in ["echo hi", "echo submit"].iter().enumerate() {
        let body = Json::obj([("model", Json::str("m")), ("i", Json::Int(i as i64))])
            .to_canonical_string()
            .into_bytes();
        d.requests.push(hharness_runner::l1_drive::CapturedRequest {
            turn: i + 1,
            method: "POST".into(),
            path: "\u{2f}v1".into(),
            content_type: Some("application/json".into()),
            body: body.clone(),
            upstream_body: body,
        });
        d.parsed_actions
            .push(vec![hharness_runner::l1_drive::L1Action {
                command: cmd.to_string(),
                tool_call_id: "c".into(),
            }]);
        d.executed_actions
            .push(d.parsed_actions.last().unwrap().clone());
    }
    d.termination = "submitted".into();
    d.submission = Some("done".into());
    d
}

fn gated_count(v: &[hharness_runner::l1_gate::L1Divergence]) -> usize {
    v.iter().filter(|d| d.gated).count()
}

#[test]
fn ac5_each_equality_level_fails_alone() {
    let canon = canon();
    let o = base_drive();
    // request — canonicalized upstream-bound bytes differ.
    let mut h = base_drive();
    h.requests[0].upstream_body = Json::obj([("model", Json::str("OTHER"))])
        .to_canonical_string()
        .into_bytes();
    let v = compare_drives(canon.as_ref(), "", &o, &h, usize::MAX, 2);
    assert!(v.iter().any(|d| d.gated && d.level == "request"));
    // action — parsed action differs from O's executed list.
    let mut h = base_drive();
    h.parsed_actions[0][0].command = "echo other".into();
    let v = compare_drives(canon.as_ref(), "", &o, &h, usize::MAX, 2);
    assert!(v.iter().any(|d| d.gated && d.level == "action"));
    // request_count.
    let mut h = base_drive();
    h.requests.pop();
    let v = compare_drives(canon.as_ref(), "", &o, &h, usize::MAX, 2);
    assert!(v.iter().any(|d| d.gated && d.level == "request_count"));
    // termination.
    let mut h = base_drive();
    h.termination = "repeated_format_error".into();
    let v = compare_drives(canon.as_ref(), "", &o, &h, usize::MAX, 2);
    assert!(v.iter().any(|d| d.gated && d.level == "termination"));
    // submission.
    let mut h = base_drive();
    h.submission = Some("other".into());
    let v = compare_drives(canon.as_ref(), "", &o, &h, usize::MAX, 2);
    assert!(v.iter().any(|d| d.gated && d.level == "submission"));
    // extra_request.
    let mut h = base_drive();
    h.extra_request = true;
    let v = compare_drives(canon.as_ref(), "", &o, &h, usize::MAX, 2);
    assert!(v.iter().any(|d| d.gated && d.level == "extra_request"));
    // downstream-only difference → diagnostic, never a fail.
    let mut h = base_drive();
    h.requests[0].body = Json::obj([
        ("model", Json::str("m")),
        ("i", Json::Int(0)),
        ("x", Json::Int(1)),
    ])
    .to_canonical_string()
    .into_bytes();
    let v = compare_drives(canon.as_ref(), "", &o, &h, usize::MAX, 2);
    assert_eq!(gated_count(&v), 0);
    assert!(v.iter().any(|d| d.level == "downstream" && !d.gated));
    // clean baseline → zero divergences.
    let v = compare_drives(canon.as_ref(), "", &o, &base_drive(), usize::MAX, 2);
    assert!(v.is_empty());
}

// ── AC-6 (E; AC-7.4-8) — records, diff-per-fail, coverage refusal ───

#[test]
fn ac6_records_shape_and_diff_per_failed_trajectory() {
    let seq = bm01_seq(None);
    let out = run_sequence(
        &seq,
        &make_o(),
        &make_h(),
        "hh-l1-canon/1",
        &[],
        "openai.chat",
        "2026-11-08T00:00:00Z",
        &h_id(),
        &o_id(),
    );
    let corpus = corpus_manifest(
        bm_fixtures().iter().map(|f| (f.id, f.branches)).collect(),
        vec!["t1".into()],
    );
    let coverage = check_corpus(&corpus, &["t1".to_string()]).unwrap();
    let conf = conformance_record(
        &[&out],
        &coverage,
        1,
        "hh-l1-canon/1",
        "sha256:corpus",
        &h_id(),
        &o_id(),
        "2026-11-08T00:00:00Z",
    );
    assert_eq!(
        conf.get("schema").unwrap().as_str().unwrap(),
        L1_CONFORMANCE_SCHEMA
    );
    assert_eq!(conf.get("label").unwrap().as_str().unwrap(), L1_LABEL);
    assert_eq!(conf.get("result").unwrap().as_str().unwrap(), "pass");
    assert!(conf
        .get("pass_id")
        .unwrap()
        .as_str()
        .unwrap()
        .starts_with("l1-pass:sha256:"));
    assert_eq!(check_l1_conformance(&conf), Vec::<String>::new());
    // A failed trajectory emits a diff record per gated divergence and
    // the conformance carries it; validators pin the shape.
    let mut plan = DivergencePlan::default();
    plan.request_patch = Some((2, vec![("temperature".into(), Json::Int(1))]));
    let out = run_sequence(
        &seq,
        &make_o(),
        &make_h_div(plan),
        "hh-l1-canon/1",
        &[],
        "openai.chat",
        "2026-11-08T00:00:00Z",
        &h_id(),
        &o_id(),
    );
    assert!(
        !out.diffs.is_empty(),
        "every failed trajectory has a diff record"
    );
    for d in &out.diffs {
        assert_eq!(d.get("schema").unwrap().as_str().unwrap(), L1_DIFF_SCHEMA);
        assert_eq!(check_l1_diff(d), Vec::<String>::new());
    }
    let conf = conformance_record(
        &[&out],
        &coverage,
        1,
        "hh-l1-canon/1",
        "sha256:corpus",
        &h_id(),
        &o_id(),
        "2026-11-08T00:00:00Z",
    );
    assert_eq!(conf.get("result").unwrap().as_str().unwrap(), "fail");
    assert!(conf.get("pass_id").is_none());
    assert!(conf
        .get("diffs")
        .unwrap()
        .as_arr()
        .unwrap()
        .iter()
        .all(|d| check_l1_diff(d).is_empty()));
    // The stable-prefix histogram + coverage table ride the record.
    assert!(conf
        .get("stable_prefix")
        .unwrap()
        .get("histogram")
        .unwrap()
        .as_obj()
        .unwrap()
        .contains_key("full"));
    assert!(conf
        .get("coverage")
        .unwrap()
        .get("branches")
        .unwrap()
        .as_obj()
        .unwrap()
        .contains_key("BM17"));
    // A missing-branch corpus refuses the gate (AC-3 also pins the
    // typed refusal) — here: coverage check upstream of the record.
    let missing = corpus_manifest(
        bm_fixtures()
            .iter()
            .take(5)
            .map(|f| (f.id, f.branches))
            .collect(),
        vec!["t1".into()],
    );
    assert_eq!(
        check_corpus(&missing, &["t1".to_string()])
            .unwrap_err()
            .rule,
        "coverage"
    );
}

// ── AC-7 (E) — an L1 record is never L2 parity evidence ─────────────

#[test]
fn ac7_l1_record_refuses_as_parity_evidence() {
    let seq = bm01_seq(None);
    let out = run_sequence(
        &seq,
        &make_o(),
        &make_h(),
        "hh-l1-canon/1",
        &[],
        "openai.chat",
        "2026-11-08T00:00:00Z",
        &h_id(),
        &o_id(),
    );
    let corpus = corpus_manifest(
        bm_fixtures().iter().map(|f| (f.id, f.branches)).collect(),
        vec!["t1".into()],
    );
    let coverage = check_corpus(&corpus, &["t1".to_string()]).unwrap();
    let conf = conformance_record(
        &[&out],
        &coverage,
        1,
        "hh-l1-canon/1",
        "sha256:corpus",
        &h_id(),
        &o_id(),
        "2026-11-08T00:00:00Z",
    );
    let pass_id = conf.get("pass_id").unwrap().as_str().unwrap().to_string();
    // The landed path: recorded model I/O as admission evidence →
    // InauthenticEvidence (evidence spec §4.4).
    let decl = pa::ParityAdmissionDecl {
        participant_ref: "p".into(),
        policies: vec!["reference".into()],
        evidence: pa::AdmissionEvidence::RecordedModelIo {
            corpus_ref: pass_id,
        },
        dimension_map: None,
        applicability: pa::PolicyApplicability::BothPolicies {
            evidence: vec![pa::PolicyEvidence {
                policy: "reference".into(),
                evidence_ref: "l1-conformance".into(),
            }],
        },
        transfer: None,
        pins: pa::SourcePins {
            source_ref: "s".into(),
            verifier_ref: "v".into(),
            rights_ref: "r".into(),
        },
        use_as: None,
    };
    let e = pa::parity_admission(&decl, &[]).unwrap_err();
    assert!(matches!(e, pa::AdmissionError::InauthenticEvidence { .. }));
    // The runner's own belt: typed refusal on the record itself.
    let e = refuse_parity_input(&conf).unwrap_err();
    assert_eq!(e.rule, "l1_not_parity");
    assert!(refuse_parity_input(&Json::obj([("schema", Json::str("x"))])).is_ok());
}
