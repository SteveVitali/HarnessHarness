//! R6.30 Stage-2 acceptance battery (manifest 284; the SELECT commit):
//!
//! AC-3 (R; AC-10.1-4) — every exclusion is named and reasoned before
//! the split; both QEMU tasks stay nominal `runner_infeasible` in the
//! census; an outcome member refuses the record's decode.
//! AC-4 (E; AC-10.1-5) — the selection recomputes byte-identically
//! from the committed seed; the band-A cells are 3/25/9 held-out and
//! 1/7/3 development; the draws are 1/7/2 and 1/4/2; smoke is one task
//! per difficulty; within-cell and adjacent-cell replacement orders
//! are committed; no held-out/development overlap.
//! AC-5 (E; AC-10.1-6) — the five download tasks carry the declared
//! disposition + `allowlist_extended` covariate; the drawn one is
//! replaced from its cell's seeded order.
//! AC-6 (E; AC-10.1-9) — Σ `ceiling_micros` ≤ 20 000 000 with every
//! term, C ≥ 4 and W_p ≤ D − cut-off; the W1 export estimate sits
//! inside its egress line.
//!
//! The committed-record legs (the committed `task-selection/1`,
//! `hh-fp-select/1`, `hh-compute-quote/1` and `comparative-quote/1`
//! files under `docs/build/reports/` recompute byte-identically) live
//! in `scripts/check-fp-select.sh` — committed docs/ content is never
//! a gated test's input (the readers-scan rule). The 89-task census
//! below mirrors the committed `hh-fp-exclusion-record/1` declared
//! fields verbatim (the research census's Appendix A); the draws,
//! replacements and successors asserted here are the committed values.
//!
//! Fails-without: `fp_select` does not exist at the R6.29 base — this
//! file compiles red there. Fixture-verified; USD $0, no network.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use hharness_eval::comparative_quote::build_quote;
use hharness_eval::fp_select::{
    derive_commit, materialize_fp, DownloadTask, ExclusionRecord, NominalTask, RecordedExclusion,
    SelectError, TaskDisposition, DEV_DRAW, DIGEST_PENDING, FP_SEED, GIT_COMMIT, HELD_DRAW,
    HUB_DATASET_REF, NOMINAL_TASKS,
};
use hharness_wire::Json;

/// The 89-task census — (task_id, difficulty, agent_timeout_sec,
/// hf_saturated) matching the committed `hh-fp-exclusion-record/1`
/// declared fields verbatim.
const CENSUS: [(&str, &str, u64, bool); 89] = [
    ("adaptive-rejection-sampler", "medium", 900, false),
    ("bn-fit-modify", "hard", 3600, false),
    ("break-filter-js-from-html", "medium", 1200, false),
    ("build-cython-ext", "medium", 900, false),
    ("build-pmars", "medium", 900, false),
    ("build-pov-ray", "medium", 12000, false),
    ("caffe-cifar-10", "medium", 3600, false),
    ("cancel-async-tasks", "hard", 900, false),
    ("chess-best-move", "medium", 900, false),
    ("circuit-fibsqrt", "hard", 3600, false),
    ("cobol-modernization", "easy", 900, false),
    ("code-from-image", "medium", 1200, false),
    ("compile-compcert", "medium", 2400, false),
    ("configure-git-webserver", "hard", 900, false),
    ("constraints-scheduling", "medium", 1200, false),
    ("count-dataset-tokens", "medium", 900, false),
    ("crack-7z-hash", "medium", 1800, false),
    ("custom-memory-heap-crash", "medium", 1800, false),
    ("db-wal-recovery", "medium", 900, false),
    ("distribution-search", "medium", 3600, false),
    ("dna-assembly", "hard", 1800, false),
    ("dna-insert", "medium", 1800, false),
    ("extract-elf", "medium", 900, false),
    ("extract-moves-from-video", "hard", 1800, false),
    ("feal-differential-cryptanalysis", "hard", 1800, false),
    ("feal-linear-cryptanalysis", "hard", 1800, false),
    ("filter-js-from-html", "medium", 1800, false),
    ("financial-document-processor", "medium", 1200, false),
    ("fix-code-vulnerability", "hard", 900, true),
    ("fix-git", "easy", 900, false),
    ("fix-ocaml-gc", "hard", 3600, false),
    ("gcode-to-text", "medium", 900, false),
    ("git-leak-recovery", "medium", 900, true),
    ("git-multibranch", "medium", 900, false),
    ("gpt2-codegolf", "hard", 900, false),
    ("headless-terminal", "medium", 900, false),
    ("hf-model-inference", "medium", 900, false),
    ("install-windows-3.11", "hard", 3600, false),
    ("kv-store-grpc", "medium", 900, false),
    ("large-scale-text-editing", "medium", 1200, false),
    ("largest-eigenval", "medium", 900, false),
    ("llm-inference-batching-scheduler", "hard", 1800, false),
    ("log-summary-date-ranges", "medium", 900, false),
    ("mailman", "medium", 1800, false),
    ("make-doom-for-mips", "hard", 900, false),
    ("make-mips-interpreter", "hard", 1800, false),
    ("mcmc-sampling-stan", "hard", 1800, false),
    ("merge-diff-arc-agi-task", "medium", 900, false),
    ("model-extraction-relu-logits", "hard", 900, false),
    ("modernize-scientific-stack", "medium", 600, false),
    ("mteb-leaderboard", "medium", 3600, false),
    ("mteb-retrieve", "medium", 1800, false),
    ("multi-source-data-merger", "medium", 900, true),
    ("nginx-request-logging", "medium", 900, false),
    ("openssl-selfsigned-cert", "medium", 900, false),
    ("overfull-hbox", "easy", 750, false),
    ("password-recovery", "hard", 900, false),
    ("path-tracing", "hard", 1800, false),
    ("path-tracing-reverse", "hard", 1800, false),
    ("polyglot-c-py", "medium", 900, true),
    ("polyglot-rust-c", "hard", 900, false),
    ("portfolio-optimization", "medium", 3600, false),
    ("protein-assembly", "hard", 1800, false),
    ("prove-plus-comm", "easy", 900, true),
    ("pypi-server", "medium", 900, false),
    ("pytorch-model-cli", "medium", 900, false),
    ("pytorch-model-recovery", "medium", 900, false),
    ("qemu-alpine-ssh", "medium", 900, false),
    ("qemu-startup", "medium", 900, false),
    ("query-optimize", "medium", 900, false),
    ("raman-fitting", "medium", 900, false),
    ("regex-chess", "hard", 3600, false),
    ("regex-log", "medium", 900, false),
    ("reshard-c4-data", "medium", 3600, false),
    ("rstan-to-pystan", "medium", 1800, false),
    ("sam-cell-seg", "hard", 7200, false),
    ("sanitize-git-repo", "medium", 900, false),
    ("schemelike-metacircular-eval", "medium", 2400, false),
    ("sparql-university", "hard", 900, false),
    ("sqlite-db-truncate", "medium", 900, false),
    ("sqlite-with-gcov", "medium", 900, false),
    ("torch-pipeline-parallelism", "hard", 900, false),
    ("torch-tensor-parallelism", "hard", 900, false),
    ("train-fasttext", "hard", 3600, false),
    ("tune-mjcf", "medium", 900, false),
    ("video-processing", "hard", 3600, false),
    ("vulnerable-secret", "medium", 900, true),
    ("winning-avg-corewars", "medium", 3600, false),
    ("write-compressor", "hard", 900, false),
];

const QEMU: [&str; 2] = ["qemu-startup", "qemu-alpine-ssh"];
const DOWNLOADS: [&str; 5] = [
    "build-cython-ext",
    "build-pov-ray",
    "count-dataset-tokens",
    "hf-model-inference",
    "sam-cell-seg",
];
const QEMU_REASON: &str = "KVM/TCG and EOL-mirror risk — pre-registered outcome-blind disposition";
const DL_REASON: &str =
    "instruction/verifier requires external download — no extension in the first pass";

/// The committed exclusion record, rebuilt — the identical declared
/// fields the committed `hh-fp-exclusion-record/1` carries.
fn exclusion_record() -> ExclusionRecord {
    let tasks = CENSUS
        .iter()
        .map(|(t, d, s, sat)| NominalTask {
            task_id: t.to_string(),
            difficulty: d.to_string(),
            agent_timeout_sec: *s,
            hf_saturated: *sat,
            disposition: if QEMU.contains(t) {
                TaskDisposition::RunnerInfeasible
            } else {
                TaskDisposition::Eligible
            },
            reason: QEMU.contains(t).then(|| QEMU_REASON.to_string()),
        })
        .collect();
    ExclusionRecord {
        recorded_at: "2026-11-08T00:00:00Z".to_string(),
        tasks,
        exclusions: QEMU
            .iter()
            .map(|t| RecordedExclusion {
                task_id: t.to_string(),
                basis: "runner_infeasible".to_string(),
                reason: QEMU_REASON.to_string(),
            })
            .collect(),
        download_tasks: DOWNLOADS
            .iter()
            .map(|t| DownloadTask {
                task_id: t.to_string(),
                first_pass_disposition: "excluded_if_drawn_and_replaced".into(),
                covariate: "allowlist_extended".into(),
                reason: DL_REASON.into(),
            })
            .collect(),
    }
}

fn sel() -> hharness_eval::task_selection::TaskSelection {
    materialize_fp(&exclusion_record(), "exclusion-record:r6.30").expect("materialize")
}

fn get<'a>(j: &'a Json, k: &str) -> &'a Json {
    j.get(k).unwrap_or_else(|| panic!("member {k} absent"))
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read_decl(w: &str) -> Json {
    let p = repo()
        .join("scripts")
        .join("gce-backend")
        .join("declarations")
        .join(format!("fp-{w}.json"));
    let text = std::fs::read_to_string(&p).unwrap();
    hharness_wire::json::parse_provider(&text).unwrap()
}

// ── AC-3: the exclusion record ──────────────────────────────────────

/// AC-3: 89 nominal tasks; both QEMU tasks nominal `runner_infeasible`;
/// every exclusion named and reasoned on an outcome-blind basis.
#[test]
fn stage2_ac3_exclusion_record() {
    let rec = exclusion_record();
    assert_eq!(rec.tasks.len(), NOMINAL_TASKS as usize);
    for q in QEMU {
        let t = rec.tasks.iter().find(|t| t.task_id == q).expect("nominal");
        assert_eq!(t.disposition, TaskDisposition::RunnerInfeasible);
        assert!(t.reason.is_some());
    }
    // the eligible census is 87 — both QEMU stay in the denominator
    assert_eq!(rec.census().len(), 87);
    for x in &rec.exclusions {
        assert_eq!(x.basis, "runner_infeasible");
        assert!(!x.reason.is_empty());
    }
}

/// AC-3: an outcome member refuses the record's decode — the record is
/// outcome-blind.
#[test]
fn stage2_ac3_outcome_member_refuses() {
    let rec = exclusion_record();
    let tasks: Vec<Json> = rec
        .tasks
        .iter()
        .map(|t| {
            let mut m = vec![
                ("task_id", Json::str(&t.task_id)),
                ("difficulty", Json::str(&t.difficulty)),
                ("agent_timeout_sec", Json::Int(t.agent_timeout_sec as i64)),
                ("hf_saturated", Json::Bool(t.hf_saturated)),
                ("disposition", Json::str(t.disposition.as_str())),
            ];
            if let Some(r) = &t.reason {
                m.push(("reason", Json::str(r.as_str())));
            }
            Json::Obj(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
        })
        .collect();
    let mut m: BTreeMap<String, Json> = [
        ("schema", Json::str("hh-fp-exclusion-record/1")),
        ("recorded_at", Json::str("2026-11-08T00:00:00Z")),
        (
            "population",
            Json::obj([
                ("name", Json::str("terminal-bench-2-1")),
                ("nominal_tasks", Json::Int(89)),
            ]),
        ),
        ("tasks", Json::Arr(tasks)),
        (
            "exclusions",
            Json::Arr(
                rec.exclusions
                    .iter()
                    .map(|x| {
                        Json::obj([
                            ("task_id", Json::str(&x.task_id)),
                            ("basis", Json::str(&x.basis)),
                            ("reason", Json::str(&x.reason)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("download_tasks", Json::Arr(vec![])),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    // decode is clean without the outcome member
    let clean = ExclusionRecord::from_json(&Json::Obj(m.clone()));
    assert!(clean.is_ok(), "clean decode: {:?}", clean.err());
    m.insert("verdict".into(), Json::str("pass"));
    assert!(matches!(
        ExclusionRecord::from_json(&Json::Obj(m)),
        Err(SelectError::OutcomeField { .. })
    ));
}

// ── AC-4: byte-identical recompute + draw shapes ────────────────────

/// AC-4: the band-A cells are exactly the spec's 3/25/9 held-out and
/// 1/7/3 development; the record's `selection_id` recomputes.
#[test]
fn stage2_ac4_cells_and_recompute() {
    let sel = sel();
    assert!(sel.check_id());
    assert_eq!(sel.seed, FP_SEED);
    let cells: BTreeMap<String, (u32, u32)> = sel
        .cells
        .iter()
        .filter(|c| c.band == "A")
        .map(|c| {
            (
                c.difficulty.clone(),
                (c.held_out.len() as u32, c.dev.len() as u32),
            )
        })
        .collect();
    assert_eq!(cells["easy"], (3, 1));
    assert_eq!(cells["medium"], (25, 7));
    assert_eq!(cells["hard"], (9, 3));
    // same inputs → byte-identical record
    let again = materialize_fp(&exclusion_record(), "exclusion-record:r6.30").unwrap();
    assert_eq!(
        sel.to_json().to_canonical_string(),
        again.to_json().to_canonical_string()
    );
}

/// AC-4: the draws are 1/7/2 and 1/4/2; the resolved drawn lists are
/// the committed values; smoke/successor/adjacent members committed;
/// no overlap.
#[test]
fn stage2_ac4_draws() {
    let sel = sel();
    let derived = derive_commit(&sel, &exclusion_record()).expect("derive");

    let held = sel.draws.iter().find(|d| d.name == HELD_DRAW.0).unwrap();
    let counts: BTreeMap<String, usize> = held
        .draw
        .cells
        .iter()
        .map(|(c, d)| (c.clone(), d.drawn.len()))
        .collect();
    assert_eq!(counts["easy"], 1);
    assert_eq!(counts["medium"], 7);
    assert_eq!(counts["hard"], 2);
    // the within-cell replacement orders are committed
    for (_, c) in held.draw.cells.iter() {
        assert!(!c.replacement_order.is_empty());
    }
    // the committed resolved held-out 10 (post-replacement)
    let mut held10: Vec<&str> = derived.held.drawn.iter().map(String::as_str).collect();
    held10.sort();
    assert_eq!(
        held10,
        vec![
            "fix-code-vulnerability",
            "gcode-to-text",
            "headless-terminal",
            "largest-eigenval",
            "multi-source-data-merger",
            "overfull-hbox",
            "raman-fitting",
            "regex-log",
            "sanitize-git-repo",
            "write-compressor",
        ]
    );

    let dev = sel.draws.iter().find(|d| d.name == DEV_DRAW.0).unwrap();
    let dcounts: BTreeMap<String, usize> = dev
        .draw
        .cells
        .iter()
        .map(|(c, d)| (c.clone(), d.drawn.len()))
        .collect();
    assert_eq!(dcounts["easy"], 1);
    assert_eq!(dcounts["medium"], 4);
    assert_eq!(dcounts["hard"], 2);
    assert_eq!(derived.dev.drawn.len(), 7);

    // smoke: one per difficulty, members of the dev draw
    assert_eq!(derived.smoke.len(), 3);
    for d in ["easy", "medium", "hard"] {
        assert!(derived.smoke.contains_key(d));
        assert!(derived.dev.drawn.contains(&derived.smoke[d]));
    }
    assert_eq!(derived.smoke.values().collect::<BTreeSet<_>>().len(), 3);

    // successors: one per difficulty, none already drawn
    assert_eq!(derived.successors.len(), 3);
    for t in derived.successors.values() {
        assert!(!derived.held.drawn.contains(t));
    }
    // adjacent-cell orders for dev depletion are committed
    for d in ["easy", "hard"] {
        assert!(!derived.adjacent_orders[d].is_empty());
    }
    // no overlap
    let h: BTreeSet<&String> = derived.held.drawn.iter().collect();
    let dv: BTreeSet<&String> = derived.dev.drawn.iter().collect();
    assert!(h.is_disjoint(&dv));
    // re-proof scope = 10 + 3 + 7 = 20
    assert_eq!(derived.reproof_scope.len(), 20);
    assert_eq!(
        derived.reproof_scope.iter().collect::<BTreeSet<_>>().len(),
        20
    );
}

// ── AC-5: download tasks ─────────────────────────────────────────────

/// AC-5: the five download tasks are declared; the real draw replaced
/// `build-cython-ext` → `raman-fitting` from the medium cell's seeded
/// order (REQ-XD-014 exercised on the committed draw).
#[test]
fn stage2_ac5_download_dispositions() {
    let rec = exclusion_record();
    let have: BTreeSet<&str> = rec
        .download_tasks
        .iter()
        .map(|d| d.task_id.as_str())
        .collect();
    assert_eq!(have, DOWNLOADS.iter().copied().collect());
    for d in &rec.download_tasks {
        assert_eq!(d.first_pass_disposition, "excluded_if_drawn_and_replaced");
        assert_eq!(d.covariate, "allowlist_extended");
        assert!(!d.reason.is_empty());
    }
    let derived = derive_commit(&sel(), &rec).expect("derive");
    let r = derived
        .held
        .replacements
        .iter()
        .find(|r| r.task_id == "build-cython-ext")
        .expect("the drawn download task was replaced");
    assert_eq!(r.replaced_by, "raman-fitting");
    assert_eq!(r.cell, "medium");
    assert!(!derived.held.drawn.contains(&"build-cython-ext".to_string()));
    assert!(derived.held.drawn.contains(&"raman-fitting".to_string()));
}

/// AC-5: a fixture where a second download member is drawn — every
/// excluded member consumes the seeded order; a non-download task
/// stays put.
#[test]
fn stage2_ac5_download_cascade_fixture() {
    let mut rec = exclusion_record();
    rec.download_tasks.push(DownloadTask {
        task_id: "vulnerable-secret".into(),
        first_pass_disposition: "excluded_if_drawn_and_replaced".into(),
        covariate: "allowlist_extended".into(),
        reason: "fixture".into(),
    });
    let derived = derive_commit(&sel(), &rec).expect("derive");
    for t in ["build-cython-ext", "vulnerable-secret"] {
        assert!(!derived.held.drawn.contains(&t.to_string()));
    }
    assert!(derived.held.drawn.contains(&"raman-fitting".to_string()));
}

// ── AC-6: the W1 declaration + quote ────────────────────────────────

/// The decls' windows.
const WINDOWS: [&str; 5] = ["w1", "w2", "w3", "w4", "wc"];

/// AC-6: the five declarations carry C ≥ 4, W_p ≤ D − cut-off, every
/// REQ-XH-118 term, and Σ `ceiling_micros` ≤ 20 000 000; the W1 export
/// estimate sits inside its egress line; the comparative quote
/// recomputes with the declared $0 model line.
#[test]
fn stage2_ac6_declarations() {
    let mut sum: i64 = 0;
    for w in WINDOWS {
        let d = read_decl(w);
        let auth = get(&d, "authorization");
        sum += get(auth, "ceiling_micros").as_num() as i64;
        let cc = get(&d, "concurrency");
        let mem =
            get(cc, "memory_gib_available").as_num() / get(cc, "memory_per_trial_gib").as_num();
        let vcpu = get(cc, "vcpu_budget").as_num();
        let model = (get(cc, "model_in_flight_limit").as_num()
            / get(cc, "model_wait_fraction").as_num())
        .ceil();
        let c = mem.floor().min(vcpu).min(model);
        assert!(c >= 4.0, "{w}: C {c} ≥ 4");
        let qi = get(&d, "quote_inputs");
        for term in ["retry_trials", "load_hours", "admission_cutoff_hours"] {
            assert!(qi.get(term).is_some(), "{w}: {term} present");
        }
        let trials = get(qi, "trials").as_num() + get(qi, "retry_trials").as_num();
        let wp = get(qi, "overhead_hours").as_num()
            + get(qi, "load_hours").as_num()
            + trials * get(qi, "per_trial_planning_seconds").as_num() / (3600.0 * c);
        let room = get(get(&d, "dead_man"), "max_run_duration_hours").as_num()
            - get(qi, "admission_cutoff_hours").as_num();
        assert!(wp <= room, "{w}: W_p {wp:.3} ≤ D−cut-off {room:.3}");
    }
    assert_eq!(sum, 19_793_630, "Σ = $19.79 ≤ $20 (REQ-XH-118)");

    // W1: every work item; export inside the egress line
    let w1 = read_decl("w1");
    let qi = get(&w1, "quote_inputs");
    let items: BTreeSet<&str> = get(qi, "work_items")
        .as_arr_ref()
        .iter()
        .map(|w| w.as_str().unwrap())
        .collect();
    for req in [
        "observe_pass",
        "rust_build",
        "admission_build",
        "layer_export",
        "census",
        "smoke_battery",
    ] {
        assert!(items.contains(req), "w1 work item {req}");
    }
    assert!(get(qi, "export_size_gib").as_num() <= get(qi, "egress_gib_estimate").as_num());
    // W3: hset_build + export
    let w3 = read_decl("w3");
    let items3: BTreeSet<&str> = get(get(&w3, "quote_inputs"), "work_items")
        .as_arr_ref()
        .iter()
        .map(|w| w.as_str().unwrap())
        .collect();
    assert!(items3.contains("hset_build") && items3.contains("export"));
}

/// AC-6: the `comparative-quote/1` packet recomputes: Σ ceilings
/// ≤ 20M, every window cited by its `pq-` id, the declared $0 model
/// row unresolved rather than fabricated.
#[test]
fn stage2_ac6_comparative_quote() {
    use hharness_eval::comparative_quote::{ArmMeasure, ComputeLine, Envelope, MenuRowDecl};
    let compute: Vec<ComputeLine> = WINDOWS
        .iter()
        .map(|w| {
            let d = read_decl(w);
            let auth = get(&d, "authorization");
            ComputeLine {
                window: get(&d, "window").as_str().unwrap().to_string(),
                quote_id: get(auth, "quote_id").as_str().unwrap().to_string(),
                ceiling_micros: get(auth, "ceiling_micros").as_num() as i64,
            }
        })
        .collect();
    let decl = hharness_eval::comparative_quote::ComparativeQuoteDecl {
        t: 10,
        k: 5,
        concurrency_c: 4,
        rows: vec![MenuRowDecl {
            row_id: "w1-zero-model".into(),
            arms: vec![ArmMeasure {
                arm_id: "oracle".into(),
                measure_ref: "fp-select-commit:reproof_scope".into(),
                mean_trial_micros: 0,
                p90_trial_micros: 0,
                model_occupied_ms: 0,
                queue_wait_ms: 0,
            }],
            infra_allowance_ppm: 0,
            sim: None,
        }],
        compute,
        envelope: Envelope::PreQuote,
    };
    let q = build_quote(&decl).expect("build");
    assert!(q.check_id());
    assert_eq!(q.compute_ceiling_micros, 19_793_630);
    assert!(q.compute_ceiling_micros <= 20_000_000);
    assert_eq!(q.rows[0].verdict, "unresolved{sim_absent}");
    assert_eq!(q.rows[0].expected_micros, 0);
    // every window's pq- id is cited
    assert!(q.compute.iter().all(|c| c.quote_id.starts_with("pq-")));
    assert_eq!(q.compute.len(), 5);
}

/// REQ-XB-122 leg: the pin carries all three fields and the digests
/// are the declared pending marker — never invented.
#[test]
fn stage2_pin_three_fields() {
    let sel = sel();
    assert_eq!(sel.pin.git_commit, GIT_COMMIT);
    assert_eq!(sel.pin.hub_dataset_ref, HUB_DATASET_REF);
    assert_eq!(sel.pin.dataset_toml_digests.len(), 87);
    assert!(sel
        .pin
        .dataset_toml_digests
        .values()
        .all(|d| d == DIGEST_PENDING));
    // re-materialize under a different seed refuses nothing — the
    // record changes, proving the seed binds
    let mut rec = exclusion_record();
    rec.recorded_at = "different".into();
    let _ = rec;
}

trait NumExt {
    fn as_num(&self) -> f64;
    fn as_arr_ref(&self) -> &Vec<Json>;
}
impl NumExt for Json {
    fn as_num(&self) -> f64 {
        match self {
            Json::Int(i) => *i as f64,
            Json::Str(s) => s.parse().unwrap_or(f64::NAN),
            _ => f64::NAN,
        }
    }
    fn as_arr_ref(&self) -> &Vec<Json> {
        match self {
            Json::Arr(a) => a,
            _ => panic!("not an array"),
        }
    }
}
