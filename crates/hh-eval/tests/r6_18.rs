//! R6.18 — `task-selection/1`, `comparative_protocol/1` and
//! `comparative-quote/1` (manifest 271; REQ-XB-122, REQ-XD-011,
//! REQ-XH-012, REQ-XC-010, REQ-XD-002/004/020/021/050/051,
//! REQ-XH-030; aliases REQ-XB-001→XB-122, REQ-XB-221→XD-051).
//! Fails-without: the three modules do not exist at the R6.17 base —
//! this file compiles red there. Fixture-only: every input is a
//! supplied stand-in; `dataset.toml` digests are labelled fixtures
//! (the real digests are R6.30's, sourced from the R6.17 Harbor
//! ingest — never invented). No spend, no network.

use std::collections::{BTreeMap, BTreeSet};

use hharness_eval::central_protocol::OUTCOME_BLIND_EXCLUSION_BASES;
use hharness_eval::task_selection::{
    band_of, largest_remainder, materialize_selection, stratified_draw, DatasetPin, DrawCandidate,
    DrawDecl, DrawPool, ExclusionRow, Pool, Purpose, SelectionDecl, SelectionError, STRATIFICATION,
    VERSION as SELECTION_VERSION,
};
use hharness_wire::Json;

fn canonical(sel: &hharness_eval::task_selection::TaskSelection) -> String {
    sel.to_json().to_canonical_string()
}

// ── fixtures ───────────────────────────────────────────────────────

/// The faithful B1 census — the 87 eligible TB 2.1 tasks' declared
/// fields from the research/25 Harbor report's Appendix A at
/// `12b75656` (task, `[agent].timeout_sec`, `[metadata].difficulty`,
/// HF-saturation covariate). `qemu-startup`/`qemu-alpine-ssh` are the
/// named `runner_infeasible` exclusions (REQ-XD-010), never census
/// members. `(task, agent_s, difficulty, hf_saturated)`.
const B1: &[(&str, u64, &str, bool)] = &[
    ("adaptive-rejection-sampler", 900, "medium", false),
    ("bn-fit-modify", 3600, "hard", false),
    ("break-filter-js-from-html", 1200, "medium", true),
    ("build-cython-ext", 900, "medium", true),
    ("build-pmars", 900, "medium", false),
    ("build-pov-ray", 12000, "medium", false),
    ("caffe-cifar-10", 3600, "medium", false),
    ("cancel-async-tasks", 900, "hard", false),
    ("chess-best-move", 900, "medium", false),
    ("circuit-fibsqrt", 3600, "hard", false),
    ("cobol-modernization", 900, "easy", false),
    ("code-from-image", 1200, "medium", false),
    ("compile-compcert", 2400, "medium", false),
    ("configure-git-webserver", 900, "hard", false),
    ("constraints-scheduling", 1200, "medium", true),
    ("count-dataset-tokens", 900, "medium", false),
    ("crack-7z-hash", 1800, "medium", false),
    ("custom-memory-heap-crash", 1800, "medium", true),
    ("db-wal-recovery", 900, "medium", false),
    ("distribution-search", 3600, "medium", true),
    ("dna-assembly", 1800, "hard", false),
    ("dna-insert", 1800, "medium", false),
    ("extract-elf", 900, "medium", false),
    ("extract-moves-from-video", 1800, "hard", false),
    ("feal-differential-cryptanalysis", 1800, "hard", true),
    ("feal-linear-cryptanalysis", 1800, "hard", false),
    ("filter-js-from-html", 1800, "medium", false),
    ("financial-document-processor", 1200, "medium", false),
    ("fix-code-vulnerability", 900, "hard", true),
    ("fix-git", 900, "easy", true),
    ("fix-ocaml-gc", 3600, "hard", true),
    ("gcode-to-text", 1200, "medium", false),
    ("git-leak-recovery", 900, "medium", true),
    ("git-multibranch", 900, "medium", false),
    ("gpt2-codegolf", 900, "hard", false),
    ("headless-terminal", 900, "medium", false),
    ("hf-model-inference", 900, "medium", false),
    ("install-windows-3.11", 3600, "hard", false),
    ("kv-store-grpc", 900, "medium", false),
    ("large-scale-text-editing", 1200, "medium", false),
    ("largest-eigenval", 900, "medium", false),
    ("llm-inference-batching-scheduler", 1800, "hard", false),
    ("log-summary-date-ranges", 900, "medium", true),
    ("mailman", 1800, "medium", false),
    ("make-doom-for-mips", 900, "hard", false),
    ("make-mips-interpreter", 1800, "hard", false),
    ("mcmc-sampling-stan", 1800, "hard", false),
    ("merge-diff-arc-agi-task", 900, "medium", false),
    ("model-extraction-relu-logits", 900, "hard", false),
    ("modernize-scientific-stack", 600, "medium", true),
    ("mteb-leaderboard", 3600, "medium", false),
    ("mteb-retrieve", 1800, "medium", false),
    ("multi-source-data-merger", 900, "medium", true),
    ("nginx-request-logging", 900, "medium", false),
    ("openssl-selfsigned-cert", 900, "medium", false),
    ("overfull-hbox", 750, "easy", false),
    ("password-recovery", 900, "hard", false),
    ("path-tracing", 1800, "hard", false),
    ("path-tracing-reverse", 1800, "hard", false),
    ("polyglot-c-py", 900, "medium", true),
    ("polyglot-rust-c", 900, "hard", false),
    ("portfolio-optimization", 3600, "medium", true),
    ("protein-assembly", 1800, "hard", false),
    ("prove-plus-comm", 900, "easy", true),
    ("pypi-server", 900, "medium", false),
    ("pytorch-model-cli", 900, "medium", false),
    ("pytorch-model-recovery", 900, "medium", false),
    ("query-optimize", 900, "medium", false),
    ("raman-fitting", 900, "medium", false),
    ("regex-chess", 3600, "hard", false),
    ("regex-log", 900, "medium", true),
    ("reshard-c4-data", 3600, "medium", false),
    ("rstan-to-pystan", 1800, "medium", false),
    ("sam-cell-seg", 7200, "hard", false),
    ("sanitize-git-repo", 900, "medium", false),
    ("schemelike-metacircular-eval", 2400, "medium", false),
    ("sparql-university", 900, "hard", false),
    ("sqlite-db-truncate", 900, "medium", true),
    ("sqlite-with-gcov", 900, "medium", true),
    ("torch-pipeline-parallelism", 900, "hard", false),
    ("torch-tensor-parallelism", 900, "hard", false),
    ("train-fasttext", 3600, "hard", false),
    ("tune-mjcf", 900, "medium", false),
    ("video-processing", 3600, "hard", false),
    ("vulnerable-secret", 900, "medium", true),
    ("winning-avg-corewars", 3600, "medium", false),
    ("write-compressor", 900, "hard", false),
];

fn pin(tasks: &[&str]) -> DatasetPin {
    DatasetPin {
        git_commit: "12b75656f4b2dbb8b3b8e5a8a4f2a30e2d5d6e70".into(),
        hub_dataset_ref: "datasets/harbor-framework/terminal-bench-2.1@12b75656".into(),
        // Labelled fixture digests — real digests are R6.30's.
        dataset_toml_digests: tasks
            .iter()
            .map(|t| (t.to_string(), format!("fixture-dataset-toml:{t}")))
            .collect(),
    }
}

fn census(hf_flip: bool) -> Vec<hharness_eval::task_selection::TaskRow> {
    B1.iter()
        .map(|(t, s, d, sat)| hharness_eval::task_selection::TaskRow {
            task_id: t.to_string(),
            agent_timeout_sec: *s,
            difficulty: d.to_string(),
            hf_saturated: *sat ^ hf_flip,
        })
        .collect()
}

fn manifest() -> Vec<String> {
    ["task_id", "agent_timeout_sec", "difficulty", "hf_saturated"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn exclusions() -> Vec<ExclusionRow> {
    vec![
        ExclusionRow {
            task_id: "qemu-startup".into(),
            basis: "runner_infeasible".into(),
        },
        ExclusionRow {
            task_id: "qemu-alpine-ssh".into(),
            basis: "runner_infeasible".into(),
        },
    ]
}

fn decl(hf_flip: bool, draws: Vec<DrawDecl>) -> SelectionDecl {
    let tasks: Vec<String> = B1.iter().map(|(t, ..)| t.to_string()).collect();
    let refs: Vec<&str> = tasks.iter().map(String::as_str).collect();
    SelectionDecl {
        pin: pin(&refs),
        census: census(hf_flip),
        exclusions: exclusions(),
        exclusion_ref: "exclusion-record:fixture-2026-11-07".into(),
        seed: "seed:select:fixture".into(),
        stratification: STRATIFICATION.into(),
        dev_pool_size: 20,
        split_predates_access: true,
        input_manifest: manifest(),
        draws,
    }
}

// ── Stage 1 — SELECT ───────────────────────────────────────────────

/// AC-1 (AC-3.1-1; REQ-XB-122 absorbing REQ-XB-001): the same census
/// and seed give byte-identical selection records; `ref=latest`
/// refuses typed; the record carries all three pin fields.
#[test]
fn stage1_ac1_deterministic_pin_three_fields() {
    let sel = materialize_selection(&decl(false, vec![])).expect("materialize");
    let again = materialize_selection(&decl(false, vec![])).expect("materialize");
    assert_eq!(sel.selection_id, again.selection_id);
    assert_eq!(canonical(&sel), canonical(&again), "byte-identical");
    assert!(sel.check_id());
    let j = sel.to_json();
    let pin = j.get("pin").expect("pin member");
    for f in ["git_commit", "hub_dataset_ref", "dataset_toml_digests"] {
        assert!(pin.get(f).is_some(), "pin.{f} present");
    }
    assert_eq!(
        sel.pin.dataset_toml_digests.len(),
        B1.len(),
        "per-task digests cover the census"
    );
}

/// AC-1: `ref=latest` refuses typed on any pin member.
#[test]
fn stage1_ac1_latest_refuses_typed() {
    let mut d = decl(false, vec![]);
    d.pin.hub_dataset_ref = "latest".into();
    assert_eq!(
        materialize_selection(&d),
        Err(SelectionError::UnpinnedRef {
            member: "hub_dataset_ref"
        })
    );
    let mut d = decl(false, vec![]);
    d.pin.git_commit = "main:latest".into();
    assert_eq!(
        materialize_selection(&d),
        Err(SelectionError::UnpinnedRef {
            member: "git_commit"
        })
    );
    let mut d = decl(false, vec![]);
    d.pin
        .dataset_toml_digests
        .insert("fix-git".into(), "latest".into());
    assert_eq!(
        materialize_selection(&d),
        Err(SelectionError::UnpinnedRef {
            member: "dataset_toml_digests"
        })
    );
}

/// AC-1: a stratum from a non-declared field refuses typed.
#[test]
fn stage1_ac1_non_declared_stratum_refuses() {
    let mut d = decl(false, vec![]);
    d.stratification = "category".into(); // a declared field, but not the
                                          // admitted stratification
    assert_eq!(
        materialize_selection(&d),
        Err(SelectionError::NonDeclaredStratum {
            stratification: "category".into()
        })
    );
    let mut d = decl(false, vec![]);
    d.stratification = "hf_saturated".into(); // an outcome-derived covariate
    assert!(matches!(
        materialize_selection(&d),
        Err(SelectionError::NonDeclaredStratum { .. })
    ));
}

/// AC-2 (AC-6.2-2; REQ-XD-011): the dev-pool allocation over the
/// faithful B1 census equals §6.2's dev row — 11/5/4/0 across bands
/// with E/M/H 1/7/3, 0/3/2, 0/2/2 — and the split reproduces from
/// the seed under `comparative_protocol.split`.
#[test]
fn stage1_ac2_b1_census_dev_row() {
    assert_eq!(B1.len(), 87, "87 eligible tasks");
    let sel = materialize_selection(&decl(false, vec![])).expect("materialize");

    let mut per_band: BTreeMap<&str, (u32, u32, u32)> = BTreeMap::new();
    for c in &sel.cells {
        let entry = per_band.entry(c.band.as_str()).or_default();
        let idx = match c.difficulty.as_str() {
            "easy" => 0,
            "medium" => 1,
            "hard" => 2,
            _ => unreachable!(),
        };
        match idx {
            0 => entry.0 = c.dev_quota,
            1 => entry.1 = c.dev_quota,
            _ => entry.2 = c.dev_quota,
        }
    }
    assert_eq!(per_band["A"], (1, 7, 3), "§6.2 dev row band A");
    assert_eq!(per_band["B"], (0, 3, 2), "§6.2 dev row band B");
    assert_eq!(per_band["C"], (0, 2, 2), "§6.2 dev row band C");
    assert_eq!(per_band["D"], (0, 0, 0), "§6.2 dev row band D");
    let dev_total: u32 = sel.cells.iter().map(|c| c.dev_quota).sum();
    assert_eq!(dev_total, 20);
    let held_total: u32 = sel.cells.iter().map(|c| c.held_out.len() as u32).sum();
    assert_eq!(held_total, 67);
    // Dev and held-out are disjoint and cover the census.
    let dev: BTreeSet<String> = sel.dev(None).into_iter().collect();
    let held: BTreeSet<String> = sel.held_out(None).into_iter().collect();
    assert!(dev.is_disjoint(&held));
    assert_eq!(dev.len() + held.len(), 87);
}

/// AC-2: a dev/confirmatory overlap refuses — a confirmatory draw may
/// not read the development pool (REQ-XD-011), and an excluded task
/// presented as eligible refuses.
#[test]
fn stage1_ac2_dev_confirmatory_overlap_refuses() {
    let d = decl(
        false,
        vec![DrawDecl {
            name: "bad".into(),
            pool: DrawPool::Dev,
            band: Some("A".into()),
            n: 3,
            purpose: Purpose::Confirmatory,
        }],
    );
    assert_eq!(
        materialize_selection(&d),
        Err(SelectionError::PoolMismatch { draw: "bad".into() })
    );

    let mut d = decl(false, vec![]);
    d.census.push(hharness_eval::task_selection::TaskRow {
        task_id: "qemu-startup".into(),
        agent_timeout_sec: 900,
        difficulty: "medium".into(),
        hf_saturated: false,
    });
    assert_eq!(
        materialize_selection(&d),
        Err(SelectionError::ExclusionEligible {
            task_id: "qemu-startup".into()
        })
    );

    // A late split refuses.
    let mut d = decl(false, vec![]);
    d.split_predates_access = false;
    assert_eq!(materialize_selection(&d), Err(SelectionError::LateSplit));
}

/// AC-2: toggling HF saturation changes no membership (covariate).
#[test]
fn stage1_ac2_hf_saturation_is_a_covariate() {
    let on = materialize_selection(&decl(false, vec![])).expect("materialize");
    let off = materialize_selection(&decl(true, vec![])).expect("materialize");
    for (a, b) in on.cells.iter().zip(off.cells.iter()) {
        assert_eq!(a.dev, b.dev, "{}|{} dev membership", a.band, a.difficulty);
        assert_eq!(
            a.held_out, b.held_out,
            "{}|{} held-out membership",
            a.band, a.difficulty
        );
        assert_eq!(a.dev_quota, b.dev_quota);
    }
    assert_eq!(on.dev(None), off.dev(None));
    assert_eq!(on.held_out(None), off.held_out(None));
    // The covariate is echoed, not read.
    let sat_on: u32 = on.cells.iter().map(|c| c.hf_saturated).sum();
    let sat_off: u32 = off.cells.iter().map(|c| c.hf_saturated).sum();
    assert_eq!(sat_off, 87 - sat_on, "the flip is echoed as a covariate");
}

/// AC-3 (AC-6.12-1; REQ-XH-012): an outcome field, or another phase's
/// held-out outcome, in the input manifest refuses typed.
#[test]
fn stage1_ac3_outcome_field_refuses() {
    for field in [
        "verdict",
        "reward",
        "ctrf",
        "verifier_output",
        "held_out_outcome",
        "phase2_outcome", // another phase's held-out outcome spelling
    ] {
        let mut d = decl(false, vec![]);
        d.input_manifest.push(field.to_string());
        assert_eq!(
            materialize_selection(&d),
            Err(SelectionError::OutcomeField {
                field: field.to_string()
            }),
            "{field} refuses"
        );
    }
}

/// AC-4 (E): a seeded stratified draw over 3/25/9 cells yields 1/7/2
/// with per-cell replacement orders reproducible from the seed; a
/// replacement consumes the next task of its cell's order.
#[test]
fn stage1_ac4_stratified_draw_and_replacement() {
    // The §10.1.2 band-A held-out cells (3 easy / 25 medium / 9 hard).
    let mut cands: Vec<DrawCandidate<'_>> = Vec::new();
    for i in 0..3 {
        cands.push(DrawCandidate {
            task_id: if i == 0 {
                "e0"
            } else if i == 1 {
                "e1"
            } else {
                "e2"
            },
            cell: "easy",
        });
    }
    let medium_ids: Vec<String> = (0..25).map(|i| format!("m{i}")).collect();
    for id in &medium_ids {
        cands.push(DrawCandidate {
            task_id: id,
            cell: "medium",
        });
    }
    let hard_ids: Vec<String> = (0..9).map(|i| format!("h{i}")).collect();
    for id in &hard_ids {
        cands.push(DrawCandidate {
            task_id: id,
            cell: "hard",
        });
    }

    let draw = stratified_draw(&cands, 10, "seed:w1").expect("draw");
    assert_eq!(draw.cells["easy"].drawn.len(), 1);
    assert_eq!(draw.cells["medium"].drawn.len(), 7);
    assert_eq!(draw.cells["hard"].drawn.len(), 2);
    assert_eq!(draw.drawn().len(), 10);
    // Replacement orders cover the remaining cell members.
    assert_eq!(draw.cells["easy"].replacement_order.len(), 2);
    assert_eq!(draw.cells["medium"].replacement_order.len(), 18);
    assert_eq!(draw.cells["hard"].replacement_order.len(), 7);

    // Same seed reproduces the draw and orders byte-for-byte.
    let again = stratified_draw(&cands, 10, "seed:w1").expect("draw");
    assert_eq!(draw, again);
    // A different seed gives a different order (overwhelmingly likely
    // for 37 distinct members — assert at least one order differs).
    let other = stratified_draw(&cands, 10, "seed:w2").expect("draw");
    assert_ne!(
        draw.cells["medium"].drawn, other.cells["medium"].drawn,
        "the medium cell's seeded order differs under another seed"
    );

    // A replacement consumes the next task of its cell's order.
    let mut d = draw.clone();
    let head = d.cells["easy"].replacement_order[0].clone();
    assert_eq!(d.take_replacement("easy").as_deref(), Some(head.as_str()));
    assert_eq!(d.cells["easy"].drawn.len(), 2);
    assert_eq!(d.cells["easy"].replacement_order.len(), 1);
    let head2 = d.cells["easy"].replacement_order[0].clone();
    assert_eq!(d.take_replacement("easy").as_deref(), Some(head2.as_str()));
    assert_eq!(d.take_replacement("easy"), None, "cell exhausted");
}

/// Stage 1 unit: the split's dev members are the seeded order's
/// prefix and draws are band-local pools.
#[test]
fn stage1_draws_materialize_in_selection() {
    let d = decl(
        false,
        vec![
            DrawDecl {
                name: "tr1".into(),
                pool: DrawPool::HeldOut,
                band: Some("A".into()),
                n: 10,
                purpose: Purpose::Confirmatory,
            },
            DrawDecl {
                name: "dev_l1".into(),
                pool: DrawPool::Dev,
                band: Some("A".into()),
                n: 7,
                purpose: Purpose::Development,
            },
        ],
    );
    let sel = materialize_selection(&d).expect("materialize");
    let tr1 = &sel.draws[0];
    assert_eq!(tr1.name, "tr1");
    assert_eq!(tr1.pool, "held_out");
    let tr1_drawn: BTreeSet<String> = tr1.draw.drawn().into_iter().collect();
    let held_a: BTreeSet<String> = sel.held_out(Some("A")).into_iter().collect();
    assert!(tr1_drawn.is_subset(&held_a), "draw ⊆ band-A held-out");
    // Band-A held-out under the faithful census is 36 tasks
    // (see run ledger: crack-7z-hash's declared agent timeout is
    // 1800 s → band B under §6.2's stated rule; the dev row is
    // identical either way).
    assert_eq!(held_a.len(), 36);
    let dev = &sel.draws[1];
    assert_eq!(dev.draw.drawn().len(), 7);
    let dev_a: BTreeSet<String> = sel.dev(Some("A")).into_iter().collect();
    assert!(dev.draw.drawn().iter().all(|t| dev_a.contains(t)));
    // pool_of reports every census task.
    for t in sel.held_out(None) {
        assert_eq!(sel.pool_of(&t), Some(Pool::HeldOut));
    }
    for t in sel.dev(None) {
        assert_eq!(sel.pool_of(&t), Some(Pool::Dev));
    }
}

/// Stage 1 unit: band edges and largest-remainder arithmetic.
#[test]
fn stage1_units_band_and_largest_remainder() {
    assert_eq!(band_of(600), "A");
    assert_eq!(band_of(900), "A");
    assert_eq!(band_of(901), "B");
    assert_eq!(band_of(1800), "B");
    assert_eq!(band_of(1801), "C");
    assert_eq!(band_of(3600), "C");
    assert_eq!(band_of(3601), "D");
    assert_eq!(band_of(12000), "D");
    // §10.1.2 dev cells 1/7/3 → 7 seats gives 1/4/2.
    let q = largest_remainder(
        &[
            ("easy".to_string(), 1),
            ("medium".to_string(), 7),
            ("hard".to_string(), 3),
        ],
        7,
        "seed:x",
    );
    assert_eq!(q["easy"], 1);
    assert_eq!(q["medium"], 4);
    assert_eq!(q["hard"], 2);
    // The §6.2 split: cells 4/32/12, 0/12/10, 0/8/7, 0/1/1 → 20.
    let q = largest_remainder(
        &[
            ("A|easy".to_string(), 4),
            ("A|medium".to_string(), 32),
            ("A|hard".to_string(), 12),
            ("B|medium".to_string(), 12),
            ("B|hard".to_string(), 10),
            ("C|medium".to_string(), 8),
            ("C|hard".to_string(), 7),
            ("D|medium".to_string(), 1),
            ("D|hard".to_string(), 1),
        ],
        20,
        "seed:x",
    );
    let band_a = q["A|easy"] + q["A|medium"] + q["A|hard"];
    let band_b = q["B|medium"] + q["B|hard"];
    let band_c = q["C|medium"] + q["C|hard"];
    let band_d = q["D|medium"] + q["D|hard"];
    assert_eq!((band_a, band_b, band_c, band_d), (11, 5, 4, 0));
    assert_eq!(
        (q["A|easy"], q["A|medium"], q["A|hard"]),
        (1, 7, 3),
        "E/M/H dev row band A"
    );
}

/// Stage 1 unit: exclusion bases are the registered outcome-blind
/// set; an outcome-selected basis refuses; unpublished exclusions
/// refuse.
#[test]
fn stage1_exclusion_refusals() {
    assert!(OUTCOME_BLIND_EXCLUSION_BASES.contains(&"runner_infeasible"));
    let mut d = decl(false, vec![]);
    d.exclusions.push(ExclusionRow {
        task_id: "fictitious".into(),
        basis: "low_hf_saturation".into(),
    });
    assert!(matches!(
        materialize_selection(&d),
        Err(SelectionError::OutcomeSelectedExclusion { .. })
    ));
    let mut d = decl(false, vec![]);
    d.exclusion_ref.clear();
    assert_eq!(
        materialize_selection(&d),
        Err(SelectionError::UnpublishedExclusions)
    );
}

/// Stage 1 unit: the version spelling is `task-selection/1` verbatim.
#[test]
fn stage1_version_spelling() {
    assert_eq!(SELECTION_VERSION, "task-selection/1");
    let sel = materialize_selection(&decl(false, vec![])).expect("materialize");
    assert_eq!(
        sel.to_json().get("schema").and_then(Json::as_str),
        Some("task-selection/1")
    );
}

// ── Stage 2 — D1-a ─────────────────────────────────────────────────

use hharness_eval::central_protocol::Warmth;
use hharness_eval::comparative_protocol::{
    materialize_comparative, ArmDecl, ComparativeDecl, ComparativeError, Identity, Pooling,
    ScheduleDecl, SeedSupport, SupportDecl, VersionPins, CLAIM_SUPPORTED, CLAIM_UNSUPPORTED,
    EXCHANGEABILITY, SCHEDULE_KIND,
};
use hharness_eval::comparative_quote::{
    build_quote, ArmMeasure, ComparativeQuoteDecl, ComparativeQuoteError, ComputeLine, Envelope,
    MenuRowDecl, SimCells, ENVELOPE_ADR, LABEL_PRE_QUOTE,
};
use hharness_eval::task_selection::TaskRow;
use hharness_lab::experiment::{Backoff, CancelPolicy, ReattemptPolicy};

/// A small synthetic selection — 8 held-out-eligible tasks across
/// two cells — for the protocol refusal matrix (keeps the fixture
/// legible; the B1 census leg is the stage-1 AC-2 test).
fn mini_selection() -> hharness_eval::task_selection::TaskSelection {
    let tasks: Vec<TaskRow> = (0..8)
        .map(|i| TaskRow {
            task_id: format!("t{i}"),
            agent_timeout_sec: 900,
            difficulty: if i < 4 { "medium" } else { "hard" }.to_string(),
            hf_saturated: false,
        })
        .collect();
    let refs: Vec<String> = tasks.iter().map(|t| t.task_id.clone()).collect();
    let mut digests = BTreeMap::new();
    for t in &refs {
        digests.insert(t.clone(), format!("fixture-dataset-toml:{t}"));
    }
    materialize_selection(&SelectionDecl {
        pin: DatasetPin {
            git_commit: "12b75656f4b2dbb8b3b8e5a8a4f2a30e2d5d6e70".into(),
            hub_dataset_ref: "hub:tb2.1@12b75656".into(),
            dataset_toml_digests: digests,
        },
        census: tasks,
        exclusions: vec![],
        exclusion_ref: String::new(),
        seed: "seed:mini".into(),
        stratification: STRATIFICATION.into(),
        dev_pool_size: 2,
        split_predates_access: true,
        input_manifest: manifest(),
        draws: vec![],
    })
    .expect("mini selection")
}

fn supported(b: bool) -> SupportDecl {
    SupportDecl {
        fingerprint: b,
        cache: b,
        seed: if b {
            SeedSupport::Verified
        } else {
            SeedSupport::Unverified
        },
    }
}

fn arm(id: &str) -> ArmDecl {
    ArmDecl {
        arm_id: id.into(),
        admission_ref: format!("admission:{id}"),
        harness_version: format!("{id}@2.4.6"),
        effort_pin: "effort:high".into(),
        catalog_identity: format!("catalog:{id}"),
        rewrite_version: "rewrite:1".into(),
        // glm-5.3-flash on Ollama's honest declarations (REQ-XC-010).
        supports: supported(false),
    }
}

fn reattempt() -> ReattemptPolicy {
    ReattemptPolicy {
        max_per_plan: 2,
        max_fraction_of_plans_ppm: 100_000,
        backoff: Backoff {
            min_ms: 1_000,
            multiplier_ppm: 2_000_000,
            max_ms: 60_000,
        },
        error_classes_included: Some(vec!["infrastructure_failure".into()]),
        on_cancel: CancelPolicy::Replan,
    }
}

fn comp_decl() -> ComparativeDecl {
    let sel = mini_selection();
    let tasks = sel.held_out(None);
    ComparativeDecl {
        selection: sel,
        tasks,
        arms: vec![arm("mini"), arm("pi"), arm("claude")],
        k: 5,
        identity: Identity::Confirmatory,
        schedule: ScheduleDecl {
            kind: SCHEDULE_KIND.into(),
            seed: "seed:sched".into(),
            exchangeability: EXCHANGEABILITY.into(),
        },
        lanes: vec!["lane:a".into(), "lane:b".into(), "lane:c".into()],
        lane_spec_ref: "lane-spec:fixture".into(),
        reattempt: reattempt(),
        warmth: Warmth::ColdStart,
        drift_observed: false,
        pilot_labelled_as: None,
        phases: BTreeMap::from([
            ("pilot".to_string(), "phase:pilot".to_string()),
            ("tranche".to_string(), "phase:tranche".to_string()),
        ]),
        family_id: "family:many-to-one".into(),
        reference_arm: "mini".into(),
        primaries: vec!["capability.task_success".into()],
        multiplicity_id: "holm".into(),
        stops: vec!["stop:budget".into(), "stop:infra".into()],
        pooling: Pooling::NotRequested,
        fingerprint_gate: false,
    }
}

/// AC-5 (AC-6.3-1; REQ-XD-020): byte-stable materialization, AR-12
/// cited, and the refusal matrix — each cell typed.
#[test]
fn stage2_ac5_materialization_and_refusal_matrix() {
    let p = materialize_comparative(&comp_decl()).expect("materialize");
    assert_eq!(
        p.to_json().get("schema").and_then(Json::as_str),
        Some("comparative_protocol/1")
    );
    assert!(p.check_id());
    let again = materialize_comparative(&comp_decl()).expect("materialize");
    assert_eq!(p.protocol_id, again.protocol_id);
    assert_eq!(
        p.to_json().to_canonical_string(),
        again.to_json().to_canonical_string(),
        "byte-stable"
    );
    // AR-12 is cited and the declared-unsupported arms carry the
    // weaker claim class.
    assert_eq!(
        p.to_json().get("ar12").and_then(Json::as_str),
        Some("ADR-0434")
    );
    for a in &p.arms {
        assert_eq!(a.claim_class, CLAIM_UNSUPPORTED);
    }
    // T·H·k slots; round 0 slots are first subjects.
    let t = p.tasks.len() as u64;
    assert_eq!(p.schedule.len() as u64, t * 3 * 5);
    assert_eq!(p.first_slots().len() as u64, t * 3);

    // H < 2.
    let mut d = comp_decl();
    d.arms.truncate(1);
    assert_eq!(
        materialize_comparative(&d),
        Err(ComparativeError::TooFewArms { h: 1 })
    );
    // Unadmitted arm.
    let mut d = comp_decl();
    d.arms[0].admission_ref.clear();
    assert!(matches!(
        materialize_comparative(&d),
        Err(ComparativeError::UnadmittedArm { .. })
    ));
    // Unpinned harness.
    for v in ["latest", "stable", ""] {
        let mut d = comp_decl();
        d.arms[1].harness_version = v.into();
        assert!(
            matches!(
                materialize_comparative(&d),
                Err(ComparativeError::UnpinnedHarness { .. }),
            ),
            "{v:?} refuses"
        );
    }
    // Split overlap — a dev-pool task in the manifest.
    let mut d = comp_decl();
    let dev_task = d.selection.dev(None)[0].clone();
    d.tasks[0] = dev_task.clone();
    assert_eq!(
        materialize_comparative(&d),
        Err(ComparativeError::SplitOverlap { task_id: dev_task })
    );
    // Unknown task.
    let mut d = comp_decl();
    d.tasks[0] = "not-in-selection".into();
    assert_eq!(
        materialize_comparative(&d),
        Err(ComparativeError::UnknownTask {
            task_id: "not-in-selection".into()
        })
    );
    // Late split.
    let mut d = comp_decl();
    d.selection.split_predates_access = false;
    assert_eq!(
        materialize_comparative(&d),
        Err(ComparativeError::LateSplit)
    );
    // Pilot relabel.
    let mut d = comp_decl();
    d.pilot_labelled_as = Some("confirmation".into());
    assert!(matches!(
        materialize_comparative(&d),
        Err(ComparativeError::PilotRelabel { .. })
    ));
    // Unrandomized schedule.
    let mut d = comp_decl();
    d.schedule.kind = "fifo".into();
    assert_eq!(
        materialize_comparative(&d),
        Err(ComparativeError::UnrandomizedSchedule {
            kind: "fifo".into()
        })
    );
}

/// AC-6 (AC-6.1-2/4; REQ-XD-002/004): k < 5 refuses a confirmatory
/// identity; a changed-version fixture and an undeclared pooling each
/// refuse pooling.
#[test]
fn stage2_ac6_k_floor_and_pooling() {
    let mut d = comp_decl();
    d.k = 4;
    assert_eq!(
        materialize_comparative(&d),
        Err(ComparativeError::ConfirmatoryFloor { k: 4 })
    );
    // Exploratory identity admits k < 5 (the floor binds confirmatory).
    d.identity = Identity::Exploratory;
    assert!(materialize_comparative(&d).is_ok());

    // Undeclared pooling refuses.
    let prior = || {
        BTreeMap::from([(
            "mini".to_string(),
            VersionPins {
                harness_version: "mini@2.4.6".into(),
                effort_pin: "effort:high".into(),
                rewrite_version: "rewrite:1".into(),
                catalog_identity: "catalog:mini".into(),
            },
        )])
    };
    let mut d = comp_decl();
    d.pooling = Pooling::Requested {
        declared_at_tr1_freeze: false,
        prior: prior(),
    };
    assert_eq!(
        materialize_comparative(&d),
        Err(ComparativeError::PoolingUndeclared)
    );
    // A changed version refuses pooling.
    for (member, bad) in [
        ("harness_version", "mini@2.5.0"),
        ("effort_pin", "effort:low"),
        ("rewrite_version", "rewrite:2"),
        ("catalog_identity", "catalog:other"),
    ] {
        let mut d = comp_decl();
        let mut p = prior();
        match member {
            "harness_version" => p.get_mut("mini").unwrap().harness_version = bad.into(),
            "effort_pin" => p.get_mut("mini").unwrap().effort_pin = bad.into(),
            "rewrite_version" => p.get_mut("mini").unwrap().rewrite_version = bad.into(),
            _ => p.get_mut("mini").unwrap().catalog_identity = bad.into(),
        }
        d.pooling = Pooling::Requested {
            declared_at_tr1_freeze: true,
            prior: p,
        };
        assert_eq!(
            materialize_comparative(&d),
            Err(ComparativeError::PoolingChanged {
                arm_id: "mini".into(),
                member
            }),
            "{member} change refuses"
        );
    }
    // Unchanged pins admit.
    let mut d = comp_decl();
    d.pooling = Pooling::Requested {
        declared_at_tr1_freeze: true,
        prior: prior(),
    };
    let p = materialize_comparative(&d).expect("admitted pooling");
    assert_eq!(p.pooling, "admitted");
}

/// AC-7 (AC-6.3-2; REQ-XD-021): an unrandomized schedule yields a
/// typed refusal; the randomization record's digest recomputes and is
/// carried by the protocol record before any slot executes; any other
/// exchangeability spelling refuses.
#[test]
fn stage2_ac7_schedule_randomization() {
    // Another exchangeability spelling refuses for this protocol.
    let mut d = comp_decl();
    d.schedule.exchangeability = "randomized_treatment".into();
    assert_eq!(
        materialize_comparative(&d),
        Err(ComparativeError::Exchangeability {
            spelling: "randomized_treatment".into()
        })
    );

    let p = materialize_comparative(&comp_decl()).expect("materialize");
    // The record's digest recomputes byte-identically from its
    // declared members and the emitted slot order — it precedes any
    // slot record (it is sealed inside protocol_id's commitment).
    assert_eq!(p.randomization.kind, SCHEDULE_KIND);
    assert_eq!(p.randomization.exchangeability, EXCHANGEABILITY);
    assert_eq!(p.randomization.rounds, 5);
    let recomputed = Json::obj([
        ("kind", Json::str(&p.randomization.kind)),
        ("seed", Json::str(&p.randomization.seed)),
        ("rounds", Json::Int(p.randomization.rounds as i64)),
        (
            "exchangeability",
            Json::str(&p.randomization.exchangeability),
        ),
        (
            "slot_order",
            Json::Arr(p.schedule.iter().map(|s| Json::str(&s.slot_ref)).collect()),
        ),
    ]);
    assert_eq!(
        p.randomization.digest,
        hharness_identity::idp::idp_digest(
            "comparative_protocol.schedule",
            recomputed.to_canonical_string().as_bytes()
        ),
        "the randomization record's digest precedes and reproduces the slot order"
    );
    // Every round holds every (task, arm) exactly once; the within-
    // round order is the seeded rank (a recompute identity — the
    // runner's k_round_interleave consumes the same seed).
    for r in 0..5 {
        let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
        for s in p.schedule.iter().filter(|s| s.round == r) {
            assert!(seen.insert((s.task_id.as_str(), s.arm_id.as_str())));
        }
        assert_eq!(seen.len() as u64, (p.tasks.len() * 3) as u64);
    }
    // First slots precede all round-1+ slots in the emitted order.
    let last_first = p
        .schedule
        .iter()
        .rposition(|s| s.first)
        .expect("first slots");
    assert!(p.schedule[..last_first].iter().all(|s| s.round == 0));
}

/// AC-8 (AC-4.6-3; REQ-XC-010): the declared-unsupported support
/// declarations are present verbatim; `materialize_protocol` is never
/// invoked with falsified support — the comparative path is a sibling
/// that admits the declarations under AR-12's weaker class.
#[test]
fn stage2_ac8_support_declarations_ar12() {
    // The module never calls into central_protocol's materializer —
    // the gate is amended by ADR-0434, never satisfied by flipping
    // booleans.
    let src = include_str!("../src/comparative_protocol.rs");
    assert!(
        !src.contains("materialize_protocol"),
        "the sibling never invokes the gated materializer"
    );
    let p = materialize_comparative(&comp_decl()).expect("materialize");
    for a in &p.arms {
        assert!(!a.supports.fingerprint && !a.supports.cache);
        assert_eq!(a.supports.seed, SeedSupport::Unverified);
        assert_eq!(a.claim_class, CLAIM_UNSUPPORTED);
    }
    // A fully verified arm claims admitted_support.
    let mut d = comp_decl();
    d.arms[0].supports = supported(true);
    let p = materialize_comparative(&d).expect("materialize");
    assert_eq!(p.arms[0].claim_class, CLAIM_SUPPORTED);
    // hh.fingerprint/v1 asserted as an identity gate refuses.
    let mut d = comp_decl();
    d.fingerprint_gate = true;
    assert_eq!(
        materialize_comparative(&d),
        Err(ComparativeError::FingerprintGate)
    );
}

fn measure(id: &str, mean: i64, p90: i64, occ: u64, wait: u64) -> ArmMeasure {
    ArmMeasure {
        arm_id: id.into(),
        measure_ref: format!("measure:pilot:{id}"),
        mean_trial_micros: mean,
        p90_trial_micros: p90,
        model_occupied_ms: occ,
        queue_wait_ms: wait,
    }
}

fn quote_decl() -> ComparativeQuoteDecl {
    ComparativeQuoteDecl {
        t: 10,
        k: 5,
        concurrency_c: 4,
        rows: vec![
            MenuRowDecl {
                row_id: "hosted-3".into(),
                arms: vec![
                    measure("mini", 6_400, 9_000, 700_000, 40_000),
                    measure("pi", 2_100, 3_000, 500_000, 30_000),
                    measure("claude", 3_600, 5_000, 600_000, 50_000),
                ],
                infra_allowance_ppm: 100_000,
                sim: Some(SimCells {
                    bootstrap_hw_ppm: 110_000,
                    fm_hw_ppm: 95_000,
                    power_ppm: 150_000,
                    p_unresolved_ppm: 20_000,
                    source: "fixture:board-sim".into(),
                }),
            },
            MenuRowDecl {
                row_id: "hosted-2".into(),
                arms: vec![
                    measure("mini", 6_400, 9_000, 700_000, 40_000),
                    measure("pi", 2_100, 3_000, 500_000, 30_000),
                ],
                infra_allowance_ppm: 100_000,
                sim: None,
            },
        ],
        compute: vec![
            ComputeLine {
                window: "W4".into(),
                quote_id: "pq-abc123".into(),
                ceiling_micros: 4_500_000,
            },
            ComputeLine {
                window: "Wc".into(),
                quote_id: "pq-def456".into(),
                ceiling_micros: 1_200_000,
            },
        ],
        envelope: Envelope::PreQuote,
    }
}

/// AC-9 (AC-6.7-1/4; REQ-XD-050/XH-030): the menu recomputes
/// byte-identically from fixture measurement records with every
/// REQ-XD-050 column and the precision verdict; model and compute
/// lines; compute lines are verbatim citations — no second pricing.
#[test]
fn stage2_ac9_menu_recompute_and_columns() {
    let q = build_quote(&quote_decl()).expect("quote");
    assert_eq!(
        q.to_json().get("schema").and_then(Json::as_str),
        Some("comparative-quote/1")
    );
    assert!(q.check_id());
    let again = build_quote(&quote_decl()).expect("quote");
    assert_eq!(q.quote_id, again.quote_id);
    assert_eq!(
        q.to_json().to_canonical_string(),
        again.to_json().to_canonical_string(),
        "byte-identical recompute from the same records"
    );

    let r = &q.rows[0];
    // N = H·T·k; every REQ-XD-050 column.
    assert_eq!(r.h, 3);
    assert_eq!(r.n_trials, 150);
    // expected = ceil((6400+2100+3600)·10·5·1.1) = ceil(12100·50·1.1).
    assert_eq!(r.expected_micros, 665_500);
    // ceiling = ceil((9000+3000+5000)·10·5·1.2) = 17000·50·1.2.
    assert_eq!(r.ceiling_micros, 1_020_000);
    // wall = ceil(10·5·(740+530+650)k /4) = ceil(50·1_920_000/4).
    assert_eq!(r.wall_millis, 24_000_000);
    assert_eq!(r.instance_millis, 24_000_000);
    // Simulated cells are inputs echoed with their source.
    let sim = r.sim.as_ref().expect("sim cells");
    assert_eq!(sim.bootstrap_hw_ppm, 110_000);
    assert_eq!(sim.fm_hw_ppm, 95_000);
    assert_eq!(sim.power_ppm, 150_000);
    assert_eq!(sim.p_unresolved_ppm, 20_000);
    // Verdict: bootstrap .11 > .10 → below_target{bootstrap_hw}.
    assert_eq!(r.verdict, "below_target{bootstrap_hw}");
    // Row without sim cells: unresolved, never fabricated.
    assert_eq!(q.rows[1].verdict, "unresolved{sim_absent}");
    // A row meeting the target.
    let mut d = quote_decl();
    d.rows[0].sim = Some(SimCells {
        bootstrap_hw_ppm: 82_000,
        fm_hw_ppm: 66_000,
        power_ppm: 420_000,
        p_unresolved_ppm: 20_000,
        source: "battery:rec-9".into(),
    });
    let q = build_quote(&d).expect("quote");
    assert_eq!(q.rows[0].verdict, "meets_target");

    // Model + compute lines both present (REQ-XH-030); compute lines
    // are verbatim echoes of the cited hh-compute-quote/1 records.
    assert_eq!(q.compute.len(), 2);
    assert_eq!(q.compute[0].quote_id, "pq-abc123");
    assert_eq!(q.compute[0].ceiling_micros, 4_500_000);
    assert_eq!(q.compute_ceiling_micros, 5_700_000);
    assert_eq!(
        q.to_json().get("envelope_adr").and_then(Json::as_str),
        Some(ENVELOPE_ADR)
    );

    // No second quote or compute-pricing routine: the module carries
    // none of the §3.10 formula vocabulary — compute lines are record
    // citations only (C-11).
    let src = include_str!("../src/comparative_quote.rs");
    for vocab in ["rate_table", "r_vm", "r_disk", "egress_gib", "price_micros"] {
        assert!(
            !src.contains(vocab),
            "no §3.10 formula member {vocab} — compute is cited, never re-priced"
        );
    }

    // Refusals.
    let mut d = quote_decl();
    d.compute.clear();
    assert_eq!(build_quote(&d), Err(ComparativeQuoteError::NoComputeLines));
    let mut d = quote_decl();
    d.rows[0].arms[0].mean_trial_micros = -1;
    assert!(matches!(
        build_quote(&d),
        Err(ComparativeQuoteError::NegativePrice { .. })
    ));
    let mut d = quote_decl();
    d.rows[0].arms[0].measure_ref.clear();
    assert!(matches!(
        build_quote(&d),
        Err(ComparativeQuoteError::MissingMeasureRef { .. })
    ));
    let mut d = quote_decl();
    d.rows[0].sim = Some(SimCells {
        bootstrap_hw_ppm: 1,
        fm_hw_ppm: 1,
        power_ppm: 1,
        p_unresolved_ppm: 1,
        source: "oracle:unadmitted".into(),
    });
    assert!(matches!(
        build_quote(&d),
        Err(ComparativeQuoteError::MissingSimSource { .. })
    ));
    let mut d = quote_decl();
    d.rows[0].arms.push(measure("pi", 1, 1, 1, 1));
    assert!(matches!(
        build_quote(&d),
        Err(ComparativeQuoteError::DuplicateArm { .. })
    ));
}

/// AC-10 (AC-6.7-2; REQ-XD-051 absorbing REQ-XB-221): AR-13 present
/// (ADR-0435), quote packets cite it, the round-5 §9.2 ceilings are
/// unchanged, and a pre-quantities quote carries `pre-quote`.
#[test]
fn stage2_ac10_ar13_label_and_ceilings_unchanged() {
    assert_eq!(ENVELOPE_ADR, "ADR-0435");
    let q = build_quote(&quote_decl()).expect("quote");
    assert_eq!(q.envelope_label, LABEL_PRE_QUOTE);
    assert_eq!(q.envelope_label, "pre-quote");

    // A provisioned envelope names its record.
    let mut d = quote_decl();
    d.envelope = Envelope::Provisioned {
        envelope_ref: "env:tr1".into(),
    };
    let q = build_quote(&d).expect("quote");
    assert_eq!(q.envelope_label, "provisioned:env:tr1");
    let mut d = quote_decl();
    d.envelope = Envelope::Provisioned {
        envelope_ref: String::new(),
    };
    assert_eq!(
        build_quote(&d),
        Err(ComparativeQuoteError::MissingEnvelopeRef)
    );

    // The round-5 §9.2 constants are unchanged in place (ADR-0435):
    // the public subject caps and the private ceiling literals.
    assert_eq!(hharness_eval::exposure_quote::PILOT_MAX_SUBJECTS, 60);
    assert_eq!(hharness_eval::exposure_quote::CONFIRM_MAX_SUBJECTS, 300);
    let src = include_str!("../src/exposure_quote.rs");
    for lit in [
        "(\"calls_per_subject\", 100)",
        "(\"input_tokens_per_subject\", 1_000_000)",
        "(\"output_tokens_per_subject\", 100_000)",
        "(\"wall_minutes_per_subject\", 30)",
        "(\"verifier_minutes\", 15)",
        "(\"concurrent_subjects\", 2)",
        "(\"cores_per_subject\", 4)",
        "(\"ram_gib_per_subject\", 8)",
        "(\"cpu_core_hours\", 200)",
        "(\"storage_gib\", 50)",
        "(\"cpu_core_hours\", 900)",
        "(\"storage_gib\", 200)",
        "(\"wall_days\", 14)",
    ] {
        assert!(src.contains(lit), "round-5 §9.2 ceiling {lit} unchanged");
    }
}
