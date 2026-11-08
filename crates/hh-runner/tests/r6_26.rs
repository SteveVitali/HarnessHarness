//! R6.26 acceptance battery (manifest 279; PS0 + LEGS + RUNNER-CHECK):
//! AC-1 the PS0 driver over a recorded transport (planned probes only,
//! receipted on the slot token, ≤ $0.05 plan bound refusing before any
//! call, hh-ps0-stage/1 with per-probe dispositions + cached share);
//! AC-2 leg registration is covered by scripts/ci/test-live-verify.sh;
//! AC-3 the corpus precondition refuses typed without a passed PS0
//! stage or with a PP0 AG3/AG5/AG6/AG8 failure (reads no verdict
//! field); AC-4 the O-slot job-config golden + upstream sweep
//! (REQ-XC-326); AC-5 the comparator over equal/masked-equal/
//! divergent fixture sets + the pinned label + the L2 refusal
//! (REQ-XC-325); AC-6 the phase-3 O composition members + the
//! R-replay-before-G-L1 precedence (REQ-XH-031); AC-7 the cited live
//! legs (REQ-XC-329). USD $0, no podman, no network.

use std::collections::BTreeMap;

use hharness_budget::pricing::ollama_glm_5_3_flash;
use hharness_budget::ModelRef;
use hharness_runner::admission::MINI_SWE_AGENT;
use hharness_runner::harbor::{sweep_no_upstream, HarborPin, JobConfig, ORoute, SkipInstall};
use hharness_runner::json_ext::JsonExt;
use hharness_runner::ps0::{
    self, check_corpus_precondition, check_plan_bound, ps0_plan, CorpusRefusal, ProbeCall,
    ProbeOutcome, ProbeTransport, Ps0Error, SlotGrant, UsageView, PP0_AG_SCHEMA, PS0_CAP_MICROS,
    STAGE_SCHEMA,
};
use hharness_runner::runner_check::{
    build_record, check_phase3_o_members, check_r_replay_precedence, check_runner_check,
    compare_config, refuse_l2_input, DeclaredDivergence, SideFacts, RUNNER_CHECK_LABEL,
    RUNNER_CHECK_SCHEMA,
};
use hharness_runner::smoke::PROXIED_SURFACES;
use hharness_wire::json::Json;

fn model_ref() -> ModelRef {
    ModelRef {
        profile_ref: "glm-5.3-flash".to_string(),
        provider_model_id: "glm-5.3-flash".to_string(),
        serving_route: "ollama".to_string(),
        effort: None,
    }
}

fn slot() -> SlotGrant {
    SlotGrant {
        token_ref: "slot-token-ref:ps0".to_string(),
        slice_micros: 10_000,
    }
}

/// A scripted transport — asserts every call carries the slot token
/// and returns queued outcomes (AC-1's recorded transport).
struct Scripted {
    outcomes: Vec<ProbeOutcome>,
    seen: Vec<(String, String)>,
}
impl ProbeTransport for Scripted {
    fn call(&mut self, slot: &SlotGrant, call: &ProbeCall) -> Result<ProbeOutcome, Ps0Error> {
        self.seen
            .push((call.probe_id.clone(), slot.token_ref.clone()));
        Ok(self
            .outcomes
            .get(self.seen.len() - 1)
            .cloned()
            .unwrap_or(ProbeOutcome {
                disposition: "pass".to_string(),
                receipt_ref: format!("receipt:{}", call.probe_id),
                usage: Some(UsageView {
                    input_uncached: 100,
                    cache_read: 0,
                    output: 4,
                }),
                cost_micros: 20,
            }))
    }
}

// ── AC-1 — the PS0 probe driver ─────────────────────────────────────────

#[test]
fn ac1_plan_is_one_call_per_dialect_plus_probes() {
    let plan = ps0_plan(&PROXIED_SURFACES, "glm-5.3-flash").unwrap();
    let kinds: Vec<_> = plan.iter().map(|c| c.kind).collect();
    use hharness_runner::ps0::ProbeKind::*;
    // one per dialect + seed (openai only) + tool_choice ×2 + cache_control ×2.
    assert_eq!(
        kinds,
        vec![
            Dialect,
            Dialect,
            Seed,
            ToolChoice,
            ToolChoice,
            CacheControl,
            CacheControl
        ]
    );
    assert_eq!(plan.len(), 7);
    for s in PROXIED_SURFACES {
        assert!(plan.iter().any(|c| c.kind == Dialect && c.surface == s));
    }
    // A dialect outside the lane refuses typed — never silently routed.
    assert!(ps0_plan(&["google.gemini"], "m").is_err());
}

#[test]
fn ac1_worst_case_bound_refuses_over_cap_before_any_call() {
    let table = ollama_glm_5_3_flash();
    let plan = ps0_plan(&PROXIED_SURFACES, "glm-5.3-flash").unwrap();
    let worst = check_plan_bound(&plan, &model_ref(), &table).unwrap();
    assert!(worst > 0 && worst <= PS0_CAP_MICROS, "worst {worst}");
    // A bloated plan refuses before any call — the transport is never
    // touched.
    let mut fat = plan.clone();
    for c in &mut fat {
        c.declared_input_tokens = 10_000_000;
        c.declared_output_tokens = 131_072;
    }
    let refusal = check_plan_bound(&fat, &model_ref(), &table).unwrap_err();
    assert!(refusal.worst_case_micros > PS0_CAP_MICROS);
    let mut t = Scripted {
        outcomes: vec![],
        seen: vec![],
    };
    assert!(ps0::run_ps0(&fat, &slot(), &mut t, &model_ref(), &table, "t").is_err());
    assert!(t.seen.is_empty(), "no call may be issued on a refused plan");
}

#[test]
fn ac1_driver_issues_exactly_the_planned_probes_and_records() {
    let table = ollama_glm_5_3_flash();
    let plan = ps0_plan(&PROXIED_SURFACES, "glm-5.3-flash").unwrap();
    let mut t = Scripted {
        outcomes: vec![],
        seen: vec![],
    };
    let stage = ps0::run_ps0(
        &plan,
        &slot(),
        &mut t,
        &model_ref(),
        &table,
        "2026-11-08T00:00:00Z",
    )
    .unwrap();
    assert_eq!(
        t.seen.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
        plan.iter().map(|c| c.probe_id.as_str()).collect::<Vec<_>>()
    );
    assert!(t.seen.iter().all(|(_, tok)| tok == "slot-token-ref:ps0"));
    assert_eq!(stage.disposition, "pass");
    let rec = stage.to_json();
    assert_eq!(rec.get("schema"), Some(&Json::str(STAGE_SCHEMA)));
    assert_eq!(rec.get("disposition"), Some(&Json::str("pass")));
    let probes = rec.get("probes").unwrap().as_arr().unwrap();
    assert_eq!(probes.len(), plan.len());
    for p in probes {
        assert_eq!(p.get("disposition"), Some(&Json::str("pass")));
        assert!(p.get("receipt_ref").and_then(Json::as_str).is_some());
    }
    // Cached share: measured ppm 0 when usage reports no cache reads;
    // `unmeasured` only when no usage rows exist at all.
    let share = rec.get("cached_share").unwrap().as_obj().unwrap();
    assert_eq!(
        share.get("openai.chat"),
        Some(&Json::obj([("ppm", Json::Int(0))]))
    );
    // A failed probe fails the stage, honestly.
    let mut fail_t = Scripted {
        outcomes: vec![],
        seen: vec![],
    };
    let mut outcomes = vec![
        ProbeOutcome {
            disposition: "pass".to_string(),
            receipt_ref: "r".to_string(),
            usage: None,
            cost_micros: 0,
        };
        7
    ];
    outcomes[3].disposition = "refused_route".to_string();
    fail_t.outcomes = outcomes;
    let stage2 = ps0::run_ps0(&plan, &slot(), &mut fail_t, &model_ref(), &table, "t").unwrap();
    assert_eq!(stage2.disposition, "failed");
}

#[test]
fn ac1_cached_share_measured_from_receipt_usage() {
    let table = ollama_glm_5_3_flash();
    let plan = ps0_plan(&PROXIED_SURFACES, "glm-5.3-flash").unwrap();
    let mut outcomes = vec![
        ProbeOutcome {
            disposition: "pass".to_string(),
            receipt_ref: "r".to_string(),
            usage: Some(UsageView {
                input_uncached: 100,
                cache_read: 0,
                output: 4,
            }),
            cost_micros: 1,
        };
        7
    ];
    // The cache_control read call reports a cache hit on its prefix.
    outcomes[6].usage = Some(UsageView {
        input_uncached: 200,
        cache_read: 1_000,
        output: 4,
    });
    let mut t = Scripted {
        outcomes,
        seen: vec![],
    };
    let stage = ps0::run_ps0(&plan, &slot(), &mut t, &model_ref(), &table, "t").unwrap();
    let ppm = stage.cached_share_ppm["anthropic.messages"].unwrap();
    // (100+200+1000 input total on anthropic.messages; 1000 cached).
    assert_eq!(ppm, 1_000 * 1_000_000 / 1_500);
}

// ── AC-3 — the corpus precondition (reads no verdict field) ────────────

fn stage_rec(disposition: &str) -> Json {
    Json::obj([
        ("schema", Json::str(STAGE_SCHEMA)),
        ("disposition", Json::str(disposition)),
    ])
}

fn pp0_rec(harness: &str, trial: &str, checks: &[(&str, &str)]) -> Json {
    Json::obj([
        ("schema", Json::str(PP0_AG_SCHEMA)),
        ("harness", Json::str(harness)),
        ("trial_ref", Json::str(trial)),
        (
            "checks",
            Json::Obj(
                checks
                    .iter()
                    .map(|(k, v)| (k.to_string(), Json::str(*v)))
                    .collect(),
            ),
        ),
        // A verdict-looking member rides along — the checker must
        // never read it (AC-3: the refusal reads no verdict field).
        ("verdict", Json::obj([("success", Json::Bool(false))])),
    ])
}

#[test]
fn ac3_passes_on_passed_stage_and_ag_evidence() {
    let ev = vec![pp0_rec(
        MINI_SWE_AGENT,
        "t1",
        &[
            ("AG3", "pass"),
            ("AG5", "pass"),
            ("AG6", "pass"),
            ("AG8", "pass"),
        ],
    )];
    check_corpus_precondition(&stage_rec("pass"), &ev).unwrap();
}

#[test]
fn ac3_refuses_typed_on_missing_or_failed_stage() {
    let ev = vec![pp0_rec(
        MINI_SWE_AGENT,
        "t1",
        &[
            ("AG3", "pass"),
            ("AG5", "pass"),
            ("AG6", "pass"),
            ("AG8", "pass"),
        ],
    )];
    assert!(matches!(
        check_corpus_precondition(&stage_rec("failed"), &ev),
        Err(CorpusRefusal::Ps0NotPassed { .. })
    ));
    assert!(matches!(
        check_corpus_precondition(&Json::obj([]), &ev),
        Err(CorpusRefusal::Ps0NotPassed { .. })
    ));
    assert_eq!(
        check_corpus_precondition(&stage_rec("pass"), &[]),
        Err(CorpusRefusal::Pp0EvidenceAbsent)
    );
    // Foreign-harness evidence alone is absent too.
    let foreign = vec![pp0_rec(
        "claude-code",
        "t2",
        &[
            ("AG3", "pass"),
            ("AG5", "pass"),
            ("AG6", "pass"),
            ("AG8", "pass"),
        ],
    )];
    assert_eq!(
        check_corpus_precondition(&stage_rec("pass"), &foreign),
        Err(CorpusRefusal::Pp0EvidenceAbsent)
    );
}

#[test]
fn ac3_refuses_typed_on_an_ag_failure() {
    let ev = vec![pp0_rec(
        MINI_SWE_AGENT,
        "trial-x",
        &[
            ("AG3", "pass"),
            ("AG5", "fail"),
            ("AG6", "pass"),
            ("AG8", "pass"),
        ],
    )];
    match check_corpus_precondition(&stage_rec("pass"), &ev) {
        Err(CorpusRefusal::Pp0AgFailed { trial, gate, .. }) => {
            assert_eq!(trial, "trial-x");
            assert_eq!(gate, "AG5");
        }
        other => panic!("expected Pp0AgFailed, got {other:?}"),
    }
}

// ── AC-4 — the O-slot job config + upstream sweep (REQ-XC-326) ─────────

fn pin() -> HarborPin {
    HarborPin::admitted(
        "2026-11-07T00:00:00Z",
        "ff2a1ef48070a8df374669138e4d4db79b77f013",
    )
}

fn o_job() -> JobConfig {
    JobConfig::new(
        &pin(),
        "o-slot-42",
        "tasks",
        vec!["task-a".to_string()],
        "mini-swe-agent",
        1,
        1,
        "jobs",
        Some(SkipInstall {
            layer_digest: "sha256:aa".to_string(),
        }),
    )
    .unwrap()
    .with_o_route(ORoute {
        relay_base_url: "http:\u{2f}\u{2f}10.88.0.1:9123\u{2f}v1".to_string(),
        api_key_name: "HH_SLOT_TOKEN".to_string(),
        api_format: "openai.chat".to_string(),
        slot_token_ref: "slot-token-ref:42".to_string(),
        slice_micros: 50_000,
    })
    .unwrap()
}

#[test]
fn ac4_o_slot_job_config_golden() {
    let cfg = o_job();
    let argv = cfg.to_argv("harbor").unwrap();
    let s = argv.join(" ");
    assert!(
        s.contains("--base-url http:\u{2f}\u{2f}10.88.0.1:9123\u{2f}v1"),
        "{s}"
    );
    assert!(s.contains("--api-key-name HH_SLOT_TOKEN"), "{s}");
    assert!(s.contains("--api-format openai.chat"), "{s}");
    assert!(s.contains("--ak skip_install=true"), "{s}");
    assert!(s.contains("-r 0"), "{s}");
    assert!(s.contains("--job-name o-slot-42"), "{s}");
    let j = cfg.to_json();
    let o = j.get("o_route").unwrap().as_obj().unwrap();
    assert_eq!(o["slice_micros"], Json::Int(50_000));
    // The committed config sweeps clean: no upstream host, no key.
    cfg.check_no_upstream().unwrap();
    // Round-trip decode.
    let back = JobConfig::from_json(&j).unwrap();
    assert_eq!(back, cfg);
}

#[test]
fn ac4_o_route_refusals() {
    let base = || -> JobConfig {
        JobConfig::new(
            &pin(),
            "o-slot",
            "tasks",
            vec!["t".to_string()],
            "a",
            1,
            1,
            "jobs",
            Some(SkipInstall {
                layer_digest: "sha256:aa".to_string(),
            }),
        )
        .unwrap()
    };
    let good = || ORoute {
        relay_base_url: "http:\u{2f}\u{2f}10.88.0.1:9123\u{2f}v1".to_string(),
        api_key_name: "HH_SLOT_TOKEN".to_string(),
        api_format: "openai.chat".to_string(),
        slot_token_ref: "r".to_string(),
        slice_micros: 1,
    };
    // An upstream host as the relay refuses.
    let mut bad = good();
    bad.relay_base_url = "https:\u{2f}\u{2f}api.ollama.com\u{2f}v1".to_string();
    assert!(base().with_o_route(bad).is_err());
    // Key material spelled in the name slot refuses.
    let mut bad = good();
    bad.api_key_name = "sk-literal-not-a-name".to_string();
    assert!(base().with_o_route(bad).is_err());
    // A non-proxied format refuses.
    let mut bad = good();
    bad.api_format = "google.gemini".to_string();
    assert!(base().with_o_route(bad).is_err());
    // o_route without skip_install refuses.
    let no_skip = JobConfig::new(&pin(), "o", "t", vec![], "a", 1, 1, "j", None).unwrap();
    assert!(no_skip.with_o_route(good()).is_err());
    // The sweep flags an Ollama host / key literal in a record.
    let dirty = Json::obj([
        ("env", Json::str("OLLAMA_API_KEY=sk-realmaterial123")),
        (
            "note",
            Json::str("see http:\u{2f}\u{2f}api.openai.com\u{2f}x"),
        ),
    ]);
    assert_eq!(sweep_no_upstream(&dirty).len(), 2);
    let clean = Json::obj([("env", Json::str("OPENAI_API_KEY=$OPENAI_API_KEY"))]);
    assert!(sweep_no_upstream(&clean).is_empty());
}

// ── AC-5 — the R-replay comparator + record (REQ-XC-325) ───────────────

fn side(reqs: Vec<Vec<u8>>) -> SideFacts {
    let mut env = BTreeMap::new();
    env.insert("HOME".to_string(), "/root".to_string());
    env.insert(
        "HH_SLOT_TOKEN".to_string(),
        "<secret:slot-token>".to_string(),
    );
    let mut files = BTreeMap::new();
    files.insert("mini.yaml".to_string(), "sha256:aa".to_string());
    SideFacts {
        argv: vec!["bash".to_string(), "-c".to_string(), "run".to_string()],
        env,
        config_sha256s: files,
        image_digest: "sha256:img".to_string(),
        detected_version: "2.4.6".to_string(),
        downstream_requests: reqs,
    }
}

#[test]
fn ac5_equal_masked_equal_and_divergent_sets() {
    let b1 = br#"{"id":"a","created":1,"messages":[{"role":"user","content":"hi"}]}"#.to_vec();
    let b2 = br#"{"id":"b","created":2,"messages":[{"role":"user","content":"hi"}]}"#.to_vec();
    let masks = vec!["id".to_string(), "created".to_string()];
    let o = side(vec![b1.clone()]);
    let h = side(vec![b2.clone()]);
    // masked-equal: only masked members differ → pass.
    let rec = build_record(
        MINI_SWE_AGENT,
        "ff2a1ef4",
        "image-identical",
        "o-ref",
        "h-ref",
        "openai.chat",
        &masks,
        &[],
        "2026-11-08T00:00:00Z",
        &o,
        &h,
    )
    .unwrap();
    assert_eq!(rec.get("result"), Some(&Json::str("pass")));
    assert_eq!(rec.get("schema"), Some(&Json::str(RUNNER_CHECK_SCHEMA)));
    assert_eq!(rec.get("label"), Some(&Json::str(RUNNER_CHECK_LABEL)));
    assert_eq!(rec.get("parity"), Some(&Json::str("none")));
    assert!(check_runner_check(&rec).is_empty(), "{rec:?}");
    // identical without masks → pass.
    let same = side(vec![b1.clone()]);
    let rec2 = build_record(
        "h",
        "p",
        "image-identical",
        "o",
        "h",
        "openai.chat",
        &[],
        &[],
        "t",
        &same,
        &same,
    )
    .unwrap();
    assert_eq!(rec2.get("result"), Some(&Json::str("pass")));
    // divergent, unregistered → divergent.
    let mut h2 = side(vec![b1.clone()]);
    h2.argv[2] = "run --evil".to_string();
    let rec3 = build_record(
        "h",
        "p",
        "image-identical",
        "o",
        "h",
        "openai.chat",
        &masks,
        &[],
        "t",
        &o,
        &h2,
    )
    .unwrap();
    assert_eq!(rec3.get("result"), Some(&Json::str("divergent")));
    assert!(check_runner_check(&rec3).is_empty());
    // … and the same divergence passes only when registered.
    let reg = vec![DeclaredDivergence {
        leg: "config.argv".to_string(),
        k: None,
        path: "[2]".to_string(),
        reason: "declared: harness exec flag".to_string(),
    }];
    let rec4 = build_record(
        "h",
        "p",
        "image-identical",
        "o",
        "h",
        "openai.chat",
        &masks,
        &reg,
        "t",
        &o,
        &h2,
    )
    .unwrap();
    assert_eq!(rec4.get("result"), Some(&Json::str("pass")));
    // A request-level divergence is likewise covered only when declared.
    let mut h3 = side(vec![
        br#"{"id":"a","created":1,"messages":[{"role":"user","content":"bye"}]}"#.to_vec(),
    ]);
    let rec5 = build_record(
        "h",
        "p",
        "image-identical",
        "o",
        "h",
        "openai.chat",
        &masks,
        &[],
        "t",
        &o,
        &h3,
    )
    .unwrap();
    assert_eq!(rec5.get("result"), Some(&Json::str("divergent")));
    let d = rec5.get("divergences").unwrap().as_arr().unwrap();
    assert!(d
        .iter()
        .any(|x| x.get("leg") == Some(&Json::str("requests"))));
    let reg5 = vec![DeclaredDivergence {
        leg: "requests".to_string(),
        k: Some(0),
        path: "messages".to_string(),
        reason: "declared".to_string(),
    }];
    let rec6 = build_record(
        "h",
        "p",
        "image-identical",
        "o",
        "h",
        "openai.chat",
        &masks,
        &reg5,
        "t",
        &o,
        &h3,
    )
    .unwrap();
    assert_eq!(rec6.get("result"), Some(&Json::str("pass")));
    h3.downstream_requests.push(b1.clone());
    // length mismatch diverges.
    let rec7 = build_record(
        "h",
        "p",
        "image-identical",
        "o",
        "h",
        "openai.chat",
        &masks,
        &[],
        "t",
        &o,
        &h3,
    )
    .unwrap();
    assert_eq!(rec7.get("result"), Some(&Json::str("divergent")));
}

#[test]
fn ac5_config_legs_and_label_and_l2_refusal() {
    let mut h = side(vec![]);
    h.image_digest = "sha256:other".to_string();
    h.detected_version = "9.9.9".to_string();
    h.env.insert("EXTRA".to_string(), "x".to_string());
    h.env.remove("HOME");
    h.config_sha256s
        .insert("mini.yaml".to_string(), "sha256:bb".to_string());
    let o = side(vec![]);
    let divs = compare_config(&o, &h);
    let legs: Vec<_> = divs.iter().map(|d| d.leg).collect();
    assert!(legs.contains(&"config.image_digest"), "{legs:?}");
    assert!(legs.contains(&"config.detected_version"), "{legs:?}");
    assert!(legs.contains(&"config.env"), "{legs:?}");
    assert!(legs.contains(&"config.files"), "{legs:?}");
    // the label phrase is pinned verbatim; a paraphrase fails.
    let rec = build_record(
        "h",
        "p",
        "image-identical",
        "o",
        "h",
        "openai.chat",
        &[],
        &[],
        "t",
        &o,
        &o,
    )
    .unwrap();
    let mut bad = rec.clone();
    if let Json::Obj(m) = &mut bad {
        m.insert(
            "label".to_string(),
            Json::str("per-harness runner check — fidelity smoke, `parity: none`"),
        );
    }
    assert!(!check_runner_check(&bad).is_empty());
    // feeding the record to the L2 parity path refuses.
    assert!(refuse_l2_input(&rec).is_err());
    assert!(refuse_l2_input(&Json::obj([("schema", Json::str("other"))])).is_ok());
}

// ── AC-6 — phase-3 O composition + G-L1 precedence (REQ-XH-031) ────────

fn participant(layer: &str) -> Json {
    Json::obj([
        ("layer_digest", Json::str(layer)),
        ("proxy", Json::str("proxy-run:w4")),
        ("vm", Json::str("vm:w4")),
        ("verifier_channel", Json::str("reward-file")),
    ])
}

#[test]
fn ac6_o_member_presence_and_precedence() {
    let o = participant("sha256:frozen-original-layer");
    assert!(check_phase3_o_members(&o).is_empty());
    // absent members refuse; pending digests refuse.
    let mut bad = o.clone();
    if let Json::Obj(m) = &mut bad {
        m.remove("verifier_channel");
    }
    assert!(!check_phase3_o_members(&bad).is_empty());
    assert!(!check_phase3_o_members(&participant("pending{layer}")).is_empty());
    // precedence: the original's r_replay record must precede G-L1.
    let rc = Json::obj([
        ("schema", Json::str(RUNNER_CHECK_SCHEMA)),
        ("kind", Json::str("r_replay")),
        ("harness", Json::str(MINI_SWE_AGENT)),
        ("recorded_at", Json::str("2026-11-08T01:00:00Z")),
    ]);
    let gl1 = Json::obj([("recorded_at", Json::str("2026-11-08T02:00:00Z"))]);
    check_r_replay_precedence(std::slice::from_ref(&rc), &gl1).unwrap();
    assert!(check_r_replay_precedence(&[], &gl1).is_err());
    let late_gl1 = Json::obj([("recorded_at", Json::str("2026-11-08T00:30:00Z"))]);
    assert!(check_r_replay_precedence(&[rc], &late_gl1).is_err());
}

// ── AC-7 — cited live legs (execute only inside their windows) ─────────

/// $0 W2 leg — PS0 driver + PP0 + corpus against the live proxy.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh ps0|pp0|corpus — the W2 paid legs run only inside W2 under FP-S (R6.40): the PS0 dialect probe ≤ $0.05, PP0 3 dev tasks × 1 × 3 hosted arms, the corpus start condition then L1 RECORD 7 × 4"]
fn live_ps0_probe_driver() {}

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh ps0|pp0|corpus — W2 under FP-S (R6.40)"]
fn live_pp0_harness_smoke() {}

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh ps0|pp0|corpus — W2 under FP-S (R6.40)"]
fn live_corpus_record() {}

/// $0 R-replay leg — `harbor run -a <agent> --ak skip_install=true`
/// against the HH cell on the same layer and recording (W1, R6.32).
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh admission-build + the R6.32 W1 runner — the $0 R-replay leg runs in W1 (R6.32): harbor run -a <agent> --ak skip_install=true against the HH cell on the same layer and recording, then the comparator emits hh-runner-check/1"]
fn live_r_replay_runner_check() {}

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh w4-main — the W4 main rounds run only inside W4 under FP-E (R6.47)"]
fn live_w4_main_rounds() {}
