//! hh-bench acceptance tests — the adapter contract out of process
//! (AC-R-2.9.4-{3,5,6,11}; S3.3).

use std::io::Write;
use std::process::{Command, Stdio};

use hh_bench::adapter::BenchmarkAdapter;
use hh_bench::adapters::FixtureAdapter;
use hh_bench::grade::{grade, parse_typed_reward, GradeError, GradeRequest, RewardParseError};
use hh_bench::infra::{detect_infrastructure_failure, InfraClass, InfraSignal};
use hh_bench::records::EnvironmentHandle;
use hh_ontology::lab::VerifierIsolation;
use hh_wire::Json;

const BIN: &str = env!("CARGO_BIN_EXE_hh-bench-adapter");

fn adapter_rpc(req: Json) -> (i32, Json) {
    let mut child = Command::new(BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn hh-bench-adapter");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(req.to_canonical_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    (
        out.status.code().unwrap_or(-1),
        hh_wire::parse(&text).expect("response not canonical JSON"),
    )
}

fn task_id_of(adapter: &str, held_out: bool) -> String {
    let a = FixtureAdapter::by_id(adapter).unwrap();
    a.task_ids()
        .into_iter()
        .find(|id| {
            let t = a.task(id).unwrap();
            (t.split_label == hh_ontology::lab::SplitLabel::HeldOut) == held_out
        })
        .unwrap()
}

#[test]
fn typed_reward_parsing() {
    assert_eq!(parse_typed_reward("{\"reward\":1000000}"), Ok(1_000_000));
    assert_eq!(parse_typed_reward("{\"reward\":0}"), Ok(0));
    assert_eq!(parse_typed_reward(""), Err(RewardParseError::Empty));
    assert_eq!(parse_typed_reward("   \n"), Err(RewardParseError::Empty));
    assert!(matches!(
        parse_typed_reward("not json"),
        Err(RewardParseError::Unparseable(_))
    ));
    assert_eq!(
        parse_typed_reward("{\"other\":1}"),
        Err(RewardParseError::Missing)
    );
}

#[test]
fn typed_reward_rejects_bad_shapes() {
    for bad in [
        "{\"reward\":0.5}",
        "{\"reward\":\"high\"}",
        "{\"reward\":2000000}",
        "garbage",
        "{\"reward\":null}",
    ] {
        assert!(
            parse_typed_reward(bad).is_err(),
            "accepted bad reward: {bad}"
        );
    }
}

#[test]
fn infra_classification() {
    let clean = InfraSignal {
        verifier_exit: Some(0),
        verifier_stderr: "",
        verifier_timed_out: false,
        environment_error: None,
        submission_produced: true,
    };
    assert_eq!(
        detect_infrastructure_failure(&clean).class,
        InfraClass::Clean
    );

    let timeout = InfraSignal {
        verifier_timed_out: true,
        ..clean_ref()
    };
    assert_eq!(
        detect_infrastructure_failure(&timeout).class,
        InfraClass::VerifierTimeout
    );
    assert_eq!(
        InfraClass::VerifierTimeout.outcome_class(),
        Some(hh_ontology::control::OutcomeClass::OracleFailure)
    );

    let env_died = InfraSignal {
        environment_error: Some("container died mid-run"),
        ..clean_ref()
    };
    assert_eq!(
        detect_infrastructure_failure(&env_died).class,
        InfraClass::EnvironmentDied
    );
    assert_eq!(
        InfraClass::EnvironmentDied.outcome_class(),
        Some(hh_ontology::control::OutcomeClass::InfrastructureFailure)
    );

    let ambiguous = InfraSignal {
        verifier_exit: Some(17),
        verifier_stderr: "unexpected token",
        ..clean_ref()
    };
    let r = detect_infrastructure_failure(&ambiguous);
    assert_eq!(r.class, InfraClass::Ambiguous);
    assert!(r.infra_suspected);
    assert_eq!(r.class.outcome_class(), None); // still scored, flagged
}

fn clean_ref() -> InfraSignal<'static> {
    InfraSignal {
        verifier_exit: Some(0),
        verifier_stderr: "",
        verifier_timed_out: false,
        environment_error: None,
        submission_produced: true,
    }
}

#[test]
fn in_process_lifecycle() {
    let a = FixtureAdapter::adapter_a();
    let task_id = task_id_of("adapter_a", false);
    let dir = std::env::temp_dir().join(format!("hh-bench-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = a
        .materialize(&task_id, dir.to_str().unwrap(), false)
        .unwrap();
    let ex = a.expose(&env, "profile/test").unwrap();
    assert_eq!(ex.task_id, task_id);
    // Held-out material must NOT be readable from the participant root.
    assert!(
        !std::path::Path::new(&env.root)
            .join("verifier/expected.bin")
            .exists()
            || env.surface == hh_bench::records::Surface::Search
    );
    // The participant writes a correct submission.
    std::fs::write(
        std::path::Path::new(&env.root).join("submission.json"),
        "{\"answer\":\"hello\"}",
    )
    .unwrap();
    let sub = a.collect_submission(&env).unwrap();
    assert!(sub.applied);
    let task = a.task(&task_id).unwrap();
    let result = a
        .grade(&GradeRequest {
            submission: sub,
            task,
            isolation: VerifierIsolation::Separate,
        })
        .unwrap();
    assert_eq!(result.reward_ppm, 1_000_000);
    assert_eq!(result.verdict, hh_ontology::eval::LatticeValue::N);
    assert!(result.separate_verifier);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unapplied_submission_scores_zero_not_oracle_failure() {
    let a = FixtureAdapter::adapter_a();
    let task_id = task_id_of("adapter_a", false);
    let dir = std::env::temp_dir().join(format!("hh-bench-test-unapplied-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env = a
        .materialize(&task_id, dir.to_str().unwrap(), false)
        .unwrap();
    a.expose(&env, "profile/test").unwrap();
    // Garbage (non-JSON) submission → apply fails → scored 0.
    std::fs::write(
        std::path::Path::new(&env.root).join("submission.json"),
        "garbage{{{",
    )
    .unwrap();
    let sub = a.collect_submission(&env).unwrap();
    assert!(!sub.applied);
    let task = a.task(&task_id).unwrap();
    let result = a
        .grade(&GradeRequest {
            submission: sub,
            task,
            isolation: VerifierIsolation::Separate,
        })
        .unwrap();
    assert_eq!(result.reward_ppm, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verifier_failure_is_oracle_failure() {
    let a = FixtureAdapter::adapter_a();
    let task_id = task_id_of("adapter_a", false);
    let task = a.task(&task_id).unwrap();
    let req = GradeRequest {
        submission: hh_bench::records::Submission {
            submission_id: "s1".into(),
            task_id: task_id.clone(),
            artifact_refs: vec![],
            payload_hex: "7b7d".into(),
            applied: true,
            apply_error: None,
        },
        task,
        isolation: VerifierIsolation::Separate,
    };
    let sig = InfraSignal {
        verifier_exit: Some(1),
        verifier_stderr: "panic: index out of bounds",
        verifier_timed_out: false,
        environment_error: None,
        submission_produced: true,
    };
    let err = grade(&req, "", &sig).unwrap_err();
    assert!(matches!(err, GradeError::VerifierFailed(_)));
}

#[test]
fn out_of_process_adapter() {
    // declare + discover through the binary.
    let (code, j) = adapter_rpc(Json::obj([
        ("adapter", Json::str("adapter_a")),
        ("op", Json::str("declare")),
    ]));
    assert_eq!(code, 0);
    assert_eq!(
        j.get("family").and_then(Json::as_str),
        Some("coding_terminal")
    );

    let (code, j) = adapter_rpc(Json::obj([
        ("adapter", Json::str("adapter_c")),
        ("op", Json::str("discover")),
    ]));
    assert_eq!(code, 0);
    let Json::Arr(ids) = j.get("task_ids").unwrap() else {
        panic!("no task_ids")
    };
    assert_eq!(ids.len(), 2);

    // A participant caller never reaches the instrument plane.
    let (code, j) = adapter_rpc(Json::obj([
        ("adapter", Json::str("adapter_a")),
        ("op", Json::str("grade")),
        ("caller", Json::str("participant")),
    ]));
    assert_eq!(code, 2);
    assert_eq!(
        j.get("kind").and_then(Json::as_str),
        Some("InstrumentOpFromParticipant")
    );

    // materialize → expose → write submission → collect_submission → grade,
    // all out of process.
    let dir = std::env::temp_dir().join(format!("hh-bench-oop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let tid = task_id_of("adapter_a", false);
    let (code, j) = adapter_rpc(Json::obj([
        ("adapter", Json::str("adapter_a")),
        ("op", Json::str("materialize")),
        ("task_id", Json::str(&tid)),
        ("root", Json::str(dir.to_str().unwrap())),
    ]));
    assert_eq!(code, 0);
    let handle = j.get("handle").unwrap().clone();
    let root = handle.get("root").unwrap().as_str().unwrap().to_string();

    let (code, j) = adapter_rpc(Json::obj([
        ("adapter", Json::str("adapter_a")),
        ("op", Json::str("expose")),
        ("handle", handle.clone()),
        ("profile_ref", Json::str("profile/test")),
    ]));
    assert_eq!(code, 0, "expose failed: {}", j.to_canonical_string());

    std::fs::write(
        std::path::Path::new(&root).join("submission.json"),
        "{\"answer\":\"hello\"}",
    )
    .unwrap();

    let (code, j) = adapter_rpc(Json::obj([
        ("adapter", Json::str("adapter_a")),
        ("op", Json::str("grade")),
        ("handle", handle),
    ]));
    assert_eq!(code, 0, "grade failed: {}", j.to_canonical_string());
    assert_eq!(
        j.get("grade")
            .and_then(|g| g.get("reward_ppm"))
            .and_then(Json::as_int),
        Some(1_000_000)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn expose_on_verifier_handle_refuses() {
    let a = FixtureAdapter::adapter_d();
    let task_id = task_id_of("adapter_d", false);
    let dir = std::env::temp_dir().join(format!("hh-bench-test-ver-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let venv = a
        .materialize(&task_id, dir.to_str().unwrap(), true)
        .unwrap();
    assert_eq!(venv.surface, hh_bench::records::Surface::Instrument);
    // The verifier root carries the staged held-out bytes.
    assert!(std::path::Path::new(&venv.root)
        .join("expected.bin")
        .exists());
    // expose on a verifier handle is the held-out boundary violation.
    let err = a.expose(&venv, "profile/test").unwrap_err();
    assert!(matches!(
        err,
        hh_bench::adapter::AdapterError::HeldOutBoundaryViolation(_)
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unenforceable_network_refuses() {
    // L4: a task whose env declares `open`/`restricted` refuses to
    // materialize in the fixture env (no advisory-as-enforcement).
    let a = FixtureAdapter::adapter_a();
    let task_id = task_id_of("adapter_a", false);
    let mut t = a.task(&task_id).unwrap();
    t.environment.network_mode = hh_ontology::lab::BenchmarkNetworkMode::Open;
    let err = hh_bench::env::FsEnv::materialize(
        std::path::Path::new("/tmp/hh-bench-l4-refuse"),
        &t.environment,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        hh_bench::env::FsEnvError::UnenforceableNetwork(_)
    ));
}

/// Silence unused-import warnings for the helper type.
#[allow(dead_code)]
fn _handle_marker(_: &EnvironmentHandle) {}

/// AC-R-2.9.4-12 — a family `require`ing a capability the environment
/// handle declares `unsupported` or leaves `unknown` refuses at
/// `materialize` with `FamilyUnsupported{missing}`; the `structured_tool`
/// fixture runs containerless through the kernel-internal file handle.
#[test]
fn family_unsupported_and_structured_tool_containerless() {
    use hh_bench::adapter::{check_handle_requirements, AdapterError};
    use hh_ontology::lab::{HandleCapability, Support};
    // `handle.network` is declared `unsupported` by the fixture env; a
    // family requiring it refuses, naming the missing capability.
    let requires = [(HandleCapability::Network, Support::Required)]
        .into_iter()
        .collect();
    match check_handle_requirements(&requires, &hh_bench::env::FsEnv::declared_support()) {
        Err(AdapterError::FamilyUnsupported { missing }) => {
            assert_eq!(missing, vec!["handle.network".to_string()]);
        }
        other => panic!("expected FamilyUnsupported, got {other:?}"),
    }
    // `handle.computer` is undeclared (`unknown`) — same refusal.
    let requires = [(HandleCapability::Computer, Support::Required)]
        .into_iter()
        .collect();
    match check_handle_requirements(&requires, &hh_bench::env::FsEnv::declared_support()) {
        Err(AdapterError::FamilyUnsupported { missing }) => {
            assert_eq!(missing, vec!["handle.computer".to_string()]);
        }
        other => panic!("expected FamilyUnsupported, got {other:?}"),
    }
    // `structured_tool` materializes with no container — the kernel-internal
    // file handle suffices.
    let d = FixtureAdapter::adapter_d();
    let task_id = task_id_of("adapter_d", false);
    let dir = std::env::temp_dir().join(format!("hh-bench-d-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let h = d
        .materialize(&task_id, dir.to_str().unwrap(), false)
        .unwrap();
    assert_eq!(
        h.family,
        hh_ontology::lab::EnvironmentFamily::StructuredTool
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// AC-R-2.9.4-6 — a tag-only task lands in `unpinned[]`, caps the suite's
/// admissible `claimed_level` at R1/R3 (no R2), and materializes with
/// `resolved_image_digest = None`; the stock adapters are all pinned.
#[test]
fn unpinned_tag_only_caps_claimed_level() {
    use hh_bench::adapter::{is_pinned_image_digest, AdapterError};
    use hh_bench::adapters::fixture_task;
    use hh_ontology::lab::{
        ContaminationStratum, EnvironmentFamily, HandleCapability, SplitLabel, Support,
        VerifierIsolation,
    };
    // Digest grammar: 64-hex (or sha256:-prefixed) pinned; a tag is a claim.
    assert!(is_pinned_image_digest(&"a".repeat(64)));
    assert!(is_pinned_image_digest(&format!(
        "sha256:{}",
        "a".repeat(64)
    )));
    assert!(!is_pinned_image_digest("latest"));
    assert!(!is_pinned_image_digest(""));
    // Stock adapters: fully pinned.
    for id in ["adapter_a", "adapter_c", "adapter_d", "adapter_e"] {
        let a = FixtureAdapter::by_id(id).unwrap();
        assert!(a.unpinned_tasks().is_empty(), "{id} has unpinned tasks");
        assert_eq!(a.claimed_levels(), vec!["R0", "R1", "R2", "R3"]);
    }
    // A tag-only suite: unpinned + capped.
    let mut t = fixture_task(
        "adapter_t",
        EnvironmentFamily::CodingTerminal,
        "t-tag",
        SplitLabel::Search,
        ContaminationStratum::PublicDated,
        "Write {\"answer\":true} to submission.json.",
        b"{\"answer\":true}",
        Some(VerifierIsolation::Separate),
        None,
    );
    t.record.environment.image_digest = "latest".into();
    let a = FixtureAdapter::from_parts(
        "adapter_t",
        EnvironmentFamily::CodingTerminal,
        [(HandleCapability::File, Support::Required)]
            .into_iter()
            .collect(),
        vec![t],
    );
    assert_eq!(a.unpinned_tasks().len(), 1);
    assert_eq!(a.claimed_levels(), vec!["R0", "R1", "R3"]);
    let dir = std::env::temp_dir().join(format!("hh-bench-tag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let tid = a.task_ids()[0].clone();
    let h = a.materialize(&tid, dir.to_str().unwrap(), false).unwrap();
    assert_eq!(h.resolved_image_digest, None);
    let _ = std::fs::remove_dir_all(&dir);
    // And a `requires` miss rides the same adapter through `materialize`.
    let bad = FixtureAdapter::from_parts(
        "adapter_bad",
        EnvironmentFamily::CodingTerminal,
        [(HandleCapability::Network, Support::Required)]
            .into_iter()
            .collect(),
        vec![fixture_task(
            "adapter_bad",
            EnvironmentFamily::CodingTerminal,
            "b1",
            SplitLabel::Search,
            ContaminationStratum::PublicDated,
            "x",
            b"x",
            None,
            None,
        )],
    );
    let tid = bad.task_ids()[0].clone();
    match bad.materialize(&tid, dir.to_str().unwrap(), false) {
        Err(AdapterError::FamilyUnsupported { missing }) => {
            assert_eq!(missing, vec!["handle.network".to_string()]);
        }
        other => panic!("expected FamilyUnsupported, got {other:?}"),
    }
}

/// AC-R-2.9.4-3 — a `ParityReport` wraps an `artifact_benefit` comparison;
/// any other benefit kind refuses (the parity record's invariant).
#[test]
fn parity_report_requires_artifact_benefit() {
    use hh_lab::analysis::{
        BudgetMatch, BudgetMatchStatus, ComparisonReport, Multiplicity, OutcomeBounds,
        OutcomeBoundsVerdict, PairedEffect, ReportLabelKind, SignProfile, TailEffects, TestKind,
        TestRecord,
    };
    use hh_ontology::eval::{BootstrapPairedMethod, EstimatorSelection, IntervalMethod};
    let rep = |kind: hh_lab::analysis::BenefitKind| ComparisonReport {
        arm_a: "orig".into(),
        arm_b: "replay".into(),
        metric: "task_success".into(),
        pairing: "by_task_and_replicate".into(),
        paired_effect: PairedEffect {
            point: None,
            interval: None,
            method: IntervalMethod::BootstrapPaired(BootstrapPairedMethod::Percentile),
        },
        per_task_effects_ref: None,
        budget_match: BudgetMatch {
            limits_equal: true,
            consumption_imbalance: None,
            tolerance_ppm: 100_000,
            status: BudgetMatchStatus::Matched,
        },
        benefit_kind: kind,
        held_out: true,
        estimated: None,
        test: TestRecord {
            kind: TestKind::PermutationSignflip,
        },
        sign_profile: SignProfile {
            helped: 0,
            hurt: 0,
            unchanged: 3,
        },
        tail_effects: TailEffects {
            p50: Json::Int(0),
            p95: Json::Int(0),
            max: Json::Int(0),
        },
        outcome_bounds: OutcomeBounds {
            lower: Json::Int(0),
            upper: Json::Int(0),
            verdict: OutcomeBoundsVerdict::Robust,
        },
        multiplicity: Multiplicity {
            family_size: 1,
            adjusted: "holm".into(),
            raw_ppm: None,
            adjusted_ppm: None,
            label: None,
        },
        label: ReportLabelKind::Headlined,
        estimator_selection: EstimatorSelection {
            method: IntervalMethod::BootstrapPaired(BootstrapPairedMethod::Percentile),
            selection_rule: "test".into(),
            floors: Default::default(),
            fallback_chain: vec![],
            substituted: None,
        },
    };
    let p = hh_bench::parity::parity_report(
        "runner/original",
        vec!["r1".into(), "r2".into(), "r3".into()],
        vec!["v1".into(), "v2".into(), "v3".into()],
        rep(hh_lab::analysis::BenefitKind::ArtifactBenefit),
    )
    .unwrap();
    assert_eq!(p.replay_verdicts.len(), 3);
    assert_eq!(p.original_runner_ref, "runner/original");
    assert!(hh_bench::parity::parity_report(
        "runner/original",
        vec![],
        vec![],
        rep(hh_lab::analysis::BenefitKind::SearchTimeBenefit),
    )
    .is_err());
}

/// AC-R-2.9.4-3 (transport leg) — `declare` out of process reports the
/// family, the `unpinned[]` set and the admissible `claimed_level`s.
#[test]
fn out_of_process_declare_reports_unpinned_and_levels() {
    let (code, resp) = adapter_rpc(Json::obj([
        ("schema", Json::str("adapter_request/1")),
        ("adapter", Json::str("adapter_c")),
        ("op", Json::str("declare")),
    ]));
    assert_eq!(code, 0);
    assert_eq!(
        resp.get("family").and_then(Json::as_str),
        Some("search_research")
    );
    assert_eq!(
        resp.get("unpinned"),
        Some(&Json::Arr(vec![])),
        "fixture suite is fully pinned"
    );
    assert_eq!(
        resp.get("claimed_levels"),
        Some(&Json::Arr(
            ["R0", "R1", "R2", "R3"]
                .iter()
                .map(|s| Json::str(*s))
                .collect()
        ))
    );
}
