//! `fp-select` — the R6.30 SELECT-commit emitter (manifest 284,
//! Stage 2). Reads the committed `hh-fp-exclusion-record/1`, runs the
//! single `task-selection/1` materializer through
//! `fp_select::build_selection_decl`, derives the `hh-fp-select/1`
//! commit and writes all committed records under
//! `docs/build/reports/`. Deterministic — same inputs, byte-identical
//! outputs. No network, no clock (the commit's `recorded_at` is the
//! declared constant the contract fixes).
//!
//! ```text
//! fp-select emit <repo-root>
//! fp-select comparative-quote <repo-root> <fp-w1-decl.json> … <fp-wc-decl.json>
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use hharness_eval::comparative_quote::{
    build_quote, ArmMeasure, ComparativeQuoteDecl, ComputeLine, Envelope, MenuRowDecl,
};
use hharness_eval::fp_select::{
    derive_commit, materialize_fp, ExclusionRecord, FpSelectCommit, FP_SEED,
};
use hharness_wire::sha256::sha256_hex;
use hharness_wire::Json;

const EXCLUSION_PATH: &str = "docs/build/reports/2026-11-08-fp-exclusion-record.json";
const SELECTION_PATH: &str = "docs/build/reports/2026-11-08-fp-selection.json";
const COMMIT_PATH: &str = "docs/build/reports/2026-11-08-fp-select-commit.json";
const COMPARATIVE_PATH: &str = "docs/build/reports/2026-11-08-fp-w1-comparative-quote.json";
/// The declared commit timestamp — the contract fixes it; the record
/// is byte-reproducible, never clock-read.
const RECORDED_AT: &str = "2026-11-08T00:00:00Z";

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  fp-select emit <repo-root>\n  fp-select comparative-quote <repo-root> <quotes…>"
    );
    ExitCode::from(2)
}

fn sha_file(p: &Path) -> String {
    sha256_hex(&std::fs::read(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display())))
}

fn write_canonical(p: &Path, j: &Json) {
    std::fs::write(p, j.to_canonical_string() + "\n")
        .unwrap_or_else(|e| panic!("write {}: {e}", p.display()));
    println!("fp-select: wrote {}", p.display());
}

fn emit(repo: &Path) {
    let excl_path = repo.join(EXCLUSION_PATH);
    let excl_json = hharness_wire::json::parse(
        &std::fs::read_to_string(&excl_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", excl_path.display())),
    )
    .unwrap_or_else(|e| panic!("parse exclusion record: {e}"));
    let record = ExclusionRecord::from_json(&excl_json).unwrap_or_else(|e| panic!("{e}"));
    let exclusion_ref = format!("{EXCLUSION_PATH}@sha256:{}", sha_file(&excl_path));

    let sel = materialize_fp(&record, &exclusion_ref).unwrap_or_else(|e| panic!("{e}"));
    let sel_json = sel.to_json();
    let sel_path = repo.join(SELECTION_PATH);
    write_canonical(&sel_path, &sel_json);
    let sel_sha = sha_file(&sel_path);

    let derived = derive_commit(&sel, &record).unwrap_or_else(|e| panic!("{e}"));
    let commit = FpSelectCommit {
        select_id: String::new(),
        recorded_at: RECORDED_AT.to_string(),
        exclusion_ref: Json::obj([
            ("path", Json::str(EXCLUSION_PATH)),
            ("sha256", Json::str(sha_file(&excl_path))),
        ]),
        selection_ref: Json::obj([
            ("path", Json::str(SELECTION_PATH)),
            ("sha256", Json::str(&sel_sha)),
            ("selection_id", Json::str(&sel.selection_id)),
        ]),
        seed: FP_SEED.to_string(),
        derived,
    }
    .with_id();
    write_canonical(&repo.join(COMMIT_PATH), &commit.record_json());
    println!(
        "fp-select: held-out {:?} dev {:?} smoke {:?} reproof {}",
        commit.derived.held.drawn.len(),
        commit.derived.dev.drawn.len(),
        commit.derived.smoke.len(),
        commit.derived.reproof_scope.len()
    );
}

/// The W1 `comparative-quote/1` packet: a declared $0 model line (W1
/// is a $0-model window — the row's arm is the oracle participant,
/// its measurement record the SELECT commit's re-proof scope) citing
/// every committed `hh-compute-quote/1` window record verbatim.
fn comparative(repo: &Path, quotes: &[String]) {
    let mut compute = Vec::new();
    for q in quotes {
        let p = PathBuf::from(q);
        // the decl carries fractional quantities (durations,
        // fractions) — `parse_provider` is the float-tolerant reader;
        // integer members surface as `Json::Str` lexemes
        let j = hharness_wire::json::parse_provider(
            &std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {q}: {e}")),
        )
        .unwrap_or_else(|e| panic!("parse {q}: {e}"));
        let window = j
            .get("window")
            .and_then(Json::as_str)
            .unwrap_or_else(|| panic!("{q}: window member absent"));
        // the compute line cites the committed decl's authorization
        // block verbatim — the pq- id is gce-backend's canonical digest,
        // never recomputed here (C-11)
        let auth = j
            .get("authorization")
            .unwrap_or_else(|| panic!("{q}: authorization member absent"));
        let quote_id = auth
            .get("quote_id")
            .and_then(Json::as_str)
            .unwrap_or_else(|| panic!("{q}: authorization.quote_id absent"));
        let ceiling = auth
            .get("ceiling_micros")
            .and_then(|v| match v {
                Json::Int(i) => Some(*i),
                Json::Str(s) => s.parse::<i64>().ok(),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{q}: authorization.ceiling_micros absent"));
        compute.push(ComputeLine {
            window: window.to_string(),
            quote_id: quote_id.to_string(),
            ceiling_micros: ceiling,
        });
    }
    let decl = ComparativeQuoteDecl {
        // the first-pass held-out census (T = 10), the declared
        // replicate count k = 5; the W1 model line is $0 — W1 runs the
        // oracle participant only (the task's own solve.sh).
        t: 10,
        k: 5,
        concurrency_c: 4,
        rows: vec![MenuRowDecl {
            row_id: "w1-zero-model".to_string(),
            arms: vec![ArmMeasure {
                arm_id: "oracle".to_string(),
                measure_ref: format!("{COMMIT_PATH}#reproof_scope"),
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
    let q = build_quote(&decl).unwrap_or_else(|e| panic!("{e}"));
    write_canonical(&repo.join(COMPARATIVE_PATH), &q.to_json());
    println!("fp-select: comparative quote {}", q.quote_id);
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("emit") if args.len() == 3 => {
            emit(Path::new(&args[2]));
            ExitCode::SUCCESS
        }
        Some("comparative-quote") if args.len() >= 4 => {
            comparative(Path::new(&args[2]), &args[3..]);
            ExitCode::SUCCESS
        }
        _ => usage(),
    }
}
