//! R5.29 — the offline example cards' acceptance battery: EX-01
//! (scripted quickstart → attach by run ID → six excerpts → export/
//! verify/R0 → replay admission) and EX-02 (the informative two-task
//! fixture → native experiments/rows/persisted report/receipt →
//! clean-recipient recomputation), plus the AC-2 negative controls:
//! stale pin, wrong count/mean, constant fixture, withheld-recompute
//! producer fallback. Fails-without: `hharness::examples` and the
//! committed cards do not exist at base 4dd6159e (compile/missing-file
//! red).
//!
//! Scratch stores only — offline scripted-lane execution; USD 0.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hharness::examples::{code, run_example, ExampleCard, ExampleError};
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

fn tmp(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-r5-29-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn svc(root: &Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".to_string(),
        workspace_root: root.join("ws"),
        holder: "r5-29".to_string(),
    })
    .unwrap()
}

fn check<'a>(r: &'a hharness::examples::ExampleReport, id: &str) -> &'a str {
    r.checks
        .iter()
        .find(|c| c.id == id)
        .map(|c| c.verdict)
        .unwrap_or_else(|| panic!("check {id} not produced: {:?}", r.checks))
}

#[test]
fn ex01_card_runs_and_verifies() {
    let root = tmp("ex01");
    let card = ExampleCard::load(&repo().join("scripts/examples/ex-01.json")).unwrap();
    let mut s = svc(&root);
    let r = run_example(&mut s, &card, &root.join("work"), &repo()).unwrap();
    assert_eq!(r.verdict, "pass", "{:?}", r.to_json());
    for id in [
        "negotiated",
        "run_succeeded",
        "excerpt:run_submission",
        "excerpt:action_model",
        "excerpt:intended_permission",
        "excerpt:effect_chain",
        "excerpt:lifecycle_vs_oracle",
        "excerpt:context_replay",
        "export_verify_r0",
    ] {
        assert_eq!(check(&r, id), "pass", "{id}: {:?}", r.to_json());
    }
    // The replay admission either admits or stays typed-refused
    // verbatim (this fixture's definition produces no compiled_bundle —
    // an honest recorded outcome, never a pass).
    assert!(
        matches!(check(&r, "replay_admission"), "pass" | "expected_refusal"),
        "replay_admission: {:?}",
        r.to_json()
    );
    // The tampered-copy control is recorded verbatim — refused, never
    // counted as a pass of the operation.
    let detail = r
        .checks
        .iter()
        .find(|c| c.id == "replay_admission")
        .unwrap()
        .detail
        .clone();
    let tampered = detail
        .get("tampered_copy")
        .and_then(Json::as_str)
        .unwrap_or("");
    assert!(
        tampered.contains("not_admitted") || tampered.contains("refused"),
        "tampered copy must stay refused: {tampered}"
    );
    // Costs/limitations ride the report verbatim.
    assert!(r.card.get("costs").is_some());
    assert!(r.card.get("limitations").is_some());
    assert!(r.card.get("fixture_label").is_some());
}

#[test]
fn ex02_card_runs_the_informative_lesson() {
    let root = tmp("ex02");
    let card = ExampleCard::load(&repo().join("scripts/examples/ex-02.json")).unwrap();
    let mut s = svc(&root);
    let r = run_example(&mut s, &card, &root.join("work"), &repo()).unwrap();
    assert_eq!(r.verdict, "pass", "{:?}", r.to_json());
    for id in [
        "subjects_20",
        "cells_pinned",
        "report_identity",
        "informative_contrast",
        "exploratory_label",
        "recipient_recompute",
    ] {
        assert_eq!(check(&r, id), "pass", "{id}: {:?}", r.to_json());
    }
    assert_eq!(check(&r, "refusal_controls"), "expected_refusal");
    // The informative cells are the pinned nondegenerate fixture — a
    // wrong count/mean would have failed cells_pinned.
    let cells = r
        .checks
        .iter()
        .find(|c| c.id == "informative_contrast")
        .unwrap()
        .detail
        .clone();
    let n_cells = match cells.get("cells") {
        Some(Json::Arr(a)) => a.len(),
        _ => 0,
    };
    assert_eq!(n_cells, 2, "two task cells: {cells:?}");
}

/// A scratch "repo" carrying a fixture + card the test controls —
/// `run_example`'s pin leg resolves `pins.<name>.path` against it.
fn scratch_repo(tag: &str, fixture: &Json, sha_override: Option<&str>) -> (PathBuf, ExampleCard) {
    let dir = tmp(tag);
    std::fs::write(dir.join("fixture.json"), fixture.to_canonical_string()).unwrap();
    let sha = sha_override.map(str::to_string).unwrap_or_else(|| {
        format!(
            "sha256:{}",
            sha256_hex(fixture.to_canonical_string().as_bytes())
        )
    });
    let card = Json::obj([
        ("schema", Json::str("example-card/1")),
        ("case", Json::str("ex-02")),
        ("card_version", Json::str("test")),
        ("fixture_label", Json::str("test fixture")),
        ("prerequisites", Json::obj([])),
        (
            "pins",
            Json::obj([(
                "fixture",
                Json::obj([
                    ("path", Json::str("fixture.json")),
                    ("sha256", Json::str(sha)),
                ]),
            )]),
        ),
        ("checks", Json::Arr(vec![])),
        ("negative_controls", Json::Arr(vec![])),
        ("costs", Json::obj([])),
        ("limitations", Json::Arr(vec![])),
        ("run", Json::str("test")),
    ]);
    (dir, ExampleCard::from_json(&card).unwrap())
}

fn informative_fixture() -> Json {
    Json::obj([
        ("schema", Json::str("ex-02-fixture/1")),
        (
            "tasks",
            Json::Arr(vec![Json::str("task:t1"), Json::str("task:t2")]),
        ),
        (
            "arms",
            Json::obj([
                ("control", Json::str("arm:a")),
                ("test", Json::str("arm:b")),
            ]),
        ),
        ("replicates", Json::Int(5)),
        ("metric", Json::str("task_success")),
        ("prior", Json::str("beta{1,1}")),
        ("seed", Json::Int(17)),
        ("price_table_version", Json::str("prices.r5.29")),
        (
            "suite",
            Json::obj([
                ("suite_id", Json::str("ex02.suite")),
                ("split_label", Json::str("dev")),
                ("split_hash", Json::str("ex02.split")),
                ("stratum", Json::str("public_dated")),
                ("family", Json::str("coding_terminal")),
            ]),
        ),
        (
            "grades",
            Json::obj([
                (
                    "task:t1",
                    Json::obj([
                        (
                            "arm:a",
                            Json::Arr(vec![
                                Json::Bool(false),
                                Json::Bool(false),
                                Json::Bool(false),
                                Json::Bool(false),
                                Json::Bool(true),
                            ]),
                        ),
                        (
                            "arm:b",
                            Json::Arr(vec![
                                Json::Bool(true),
                                Json::Bool(true),
                                Json::Bool(true),
                                Json::Bool(true),
                                Json::Bool(false),
                            ]),
                        ),
                    ]),
                ),
                (
                    "task:t2",
                    Json::obj([
                        (
                            "arm:a",
                            Json::Arr(vec![
                                Json::Bool(true),
                                Json::Bool(true),
                                Json::Bool(true),
                                Json::Bool(false),
                                Json::Bool(false),
                            ]),
                        ),
                        (
                            "arm:b",
                            Json::Arr(vec![
                                Json::Bool(true),
                                Json::Bool(true),
                                Json::Bool(false),
                                Json::Bool(false),
                                Json::Bool(false),
                            ]),
                        ),
                    ]),
                ),
            ]),
        ),
        (
            "expected",
            Json::obj([
                (
                    "task:t1",
                    Json::obj([("arm:a", Json::Int(1)), ("arm:b", Json::Int(4))]),
                ),
                (
                    "task:t2",
                    Json::obj([("arm:a", Json::Int(3)), ("arm:b", Json::Int(2))]),
                ),
            ]),
        ),
    ])
}

#[test]
fn ex02_stale_pin_refuses() {
    let root = tmp("stale");
    let (repo_dir, card) = scratch_repo("stale-repo", &informative_fixture(), Some("sha256:0000"));
    let mut s = svc(&root);
    let e = run_example(&mut s, &card, &root.join("work"), &repo_dir).unwrap_err();
    assert!(
        matches!(
            e,
            ExampleError::Refused {
                code: code::STALE_PIN,
                ..
            }
        ),
        "{e}"
    );
}

#[test]
fn ex02_wrong_count_fails() {
    // One flipped observation — the durable recomputation must not equal
    // the pinned counts; the card verdict fails semantically.
    let mut fx = informative_fixture();
    if let Json::Obj(m) = &mut fx {
        if let Some(Json::Obj(g)) = m.get_mut("grades") {
            if let Some(Json::Obj(t1)) = g.get_mut("task:t1") {
                t1.insert("arm:a".to_string(), Json::Arr(vec![Json::Bool(false); 5]));
            }
        }
    }
    let root = tmp("wrongcount");
    let (repo_dir, card) = scratch_repo("wc-repo", &fx, None);
    let mut s = svc(&root);
    let r = run_example(&mut s, &card, &root.join("work"), &repo_dir).unwrap();
    assert_eq!(r.verdict, "fail");
    assert_eq!(check(&r, "cells_pinned"), "fail");
}

#[test]
fn ex02_constant_fixture_fails_the_lesson() {
    // A constant teaching-only fixture: every grade true. The cells
    // still report valid uncertainty (the method does not refuse a
    // constant input) but the informative lesson check must fail.
    let mut fx = informative_fixture();
    if let Json::Obj(m) = &mut fx {
        if let Some(Json::Obj(g)) = m.get_mut("grades") {
            for (_, t) in g.iter_mut() {
                if let Json::Obj(t) = t {
                    for (_, v) in t.iter_mut() {
                        *v = Json::Arr(vec![Json::Bool(true); 5]);
                    }
                }
            }
        }
    }
    let root = tmp("constant");
    let (repo_dir, card) = scratch_repo("const-repo", &fx, None);
    let mut s = svc(&root);
    let r = run_example(&mut s, &card, &root.join("work"), &repo_dir).unwrap();
    assert_eq!(r.verdict, "fail", "constant fixture must fail");
    // The lesson's informative check is what fails (not the method —
    // the analysis still produced a report, which the other checks
    // exercise).
    assert_eq!(check(&r, "informative_contrast"), "fail");
}

#[test]
fn ex02_withheld_recompute_input_refuses_no_fallback() {
    // Producer-fallback control — the recompute owner's verify_inputs
    // gate: a withheld reference body is a typed refusal, never a
    // producer-side read.
    use hharness_analysis::recompute::{recompute_analysis, RecomputePlan};
    let plan = RecomputePlan {
        spec: &hharness_lab::analysis::AnalysisSpec {
            spec_id: "s".into(),
            kind: "finite_manifest_contrast".into(),
            query: hharness_lab::analysis::QuerySpec {
                metrics: vec![],
                filters: None,
                grain: None,
            },
            spec_ref: None,
            label: None,
            estimator_selection: hharness_ontology::eval::EstimatorSelection {
                method: hharness_ontology::eval::IntervalMethod::FiniteManifestContrast(
                    "beta{1,1}".into(),
                ),
                selection_rule: "x".into(),
                floors: Default::default(),
                fallback_chain: vec![],
                substituted: None,
            },
            resample: None,
            outputs: vec![],
        },
        declared: &hharness_analysis::receipt::AnalysisDecl::default(),
        reference: None,
        population: None,
    };
    let rows: Vec<hharness_results::row::ResultsRow> = vec![];
    let manifests = Default::default();
    let facts = Default::default();
    let tasks = vec![];
    let suites = vec![];
    let wm = hharness_results::watermark::WatermarkSet::new();
    let input = hharness_analysis::AnalysisInput {
        rows: &rows,
        manifests: &manifests,
        facts: &facts,
        tasks: &tasks,
        suites: &suites,
        design: None,
        arm_specs: &[],
        declarations: &[],
        pre_registration: None,
        watermark_set: &wm,
        metric_registry_version: "",
        price_table_version: "",
        confidence_ppm: 950_000,
        resampling_draws: 0,
        seed: 0,
        post_amendment: false,
        view: None,
    };
    let e = recompute_analysis(&plan, &input).unwrap_err();
    assert!(
        format!("{e:?}").contains("Withheld"),
        "withheld input must refuse typed: {e:?}"
    );
}

#[test]
fn card_decode_refusals() {
    let mut j = Json::obj([
        ("schema", Json::str("example-card/1")),
        ("case", Json::str("ex-99")),
    ]);
    assert!(matches!(
        ExampleCard::from_json(&j).unwrap_err(),
        ExampleError::Refused {
            code: code::UNKNOWN_CASE,
            ..
        }
    ));
    if let Json::Obj(m) = &mut j {
        m.insert("case".to_string(), Json::str("ex-01"));
        m.insert("bogus".to_string(), Json::Null);
    }
    assert!(matches!(
        ExampleCard::from_json(&j).unwrap_err(),
        ExampleError::Refused {
            code: code::UNSUPPORTED_SHAPE,
            ..
        }
    ));
}
