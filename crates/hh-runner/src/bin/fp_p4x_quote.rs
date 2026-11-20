//! `fp-p4x-quote` — the GATE-G35 (G-P4-X) quote emitter. Reads the
//! committed records named by the inputs manifest — the frozen
//! `hh-p4-x-prereg/1` exploration pre-registration (R7.2), the p4x
//! `hh-compute-decl/1` declaration and its minted `hh-compute-quote/1`
//! record (receipt_lib's one §3.10 path) — and mints the
//! `comparative-quote/1` packet through `hharness_eval::comparative_quote`
//! (C-11: the single quote artifact for paid gates in phases 2–4; no
//! other quote routine exists), plus the `hh-p4-x-quote-inputs/1`
//! declared-inputs record it cites, each with its sha256 sidecar at
//! the manifest's output relpaths.
//!
//! Slot-level arms at `t = k = 1`, the R6.32/W2 precedent: every
//! enumerated slot line counts once, so `n_trials` is the real slot
//! count per leg (X0 = 24, X1 = 440 executable — V8/V9 are
//! `withheld{inert_at_admissible_window}` and are never priced). Every
//! model figure is a DECLARED planning prior — the §10.4 top planning
//! point `$0.34/slot` for `mean` and `1.5×` for `p90`, never a
//! measurement; the measure_ref labels them verbatim. Compute lines
//! are verbatim citations of the committed `hh-compute-quote/1` record
//! (id + ceiling read from the declaration's authorization block,
//! cross-checked against the record). `HH_FP_P4X_INPUTS` names the
//! manifest (`scripts/fp-p4x-inputs.txt` by default). Deterministic —
//! same inputs, byte-identical outputs. No network, no clock — and
//! the packet authorizes nothing: REQ-XH-004's verbatim model and
//! compute authorizations are the operator's, recorded at GATE-G35.
//!
//! ```text
//! fp-p4x-quote emit <repo-root>
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_eval::comparative_quote::{
    build_quote, ArmMeasure, ComparativeQuoteDecl, ComputeLine, Envelope, MenuRowDecl,
};
use hharness_wire::json::{parse, parse_provider, Json};
use hharness_wire::sha256::sha256_hex;

/// §10.4's top planning point — $0.34 per trial in integer µ$.
/// UNVERIFIED declared planning prior (the same class as R6.32's
/// §10.1.5 binding priors), never a measured per-trial cost.
const DECLARED_P_HAT_MICROS: i64 = 340_000;
/// The declared per-slot P90 planning bound — 1.5 × p̂ so the packet's
/// machinery ceiling lands at §10.4's planning-ceiling convention
/// (1.5 × 1.2 × expected at the top point).
const DECLARED_P90_MICROS: i64 = 510_000;
/// The declared per-slot planning wall — the compute declaration's
/// `per_trial_planning_seconds` (1040 s) in ms; the priced figure
/// lives on the cited compute record.
const DECLARED_OCCUPIED_MS: u64 = 1_040_000;
/// The declared concurrency (the compute declaration's C = 4).
const CONCURRENCY: u64 = 4;

fn load(repo: &Path, rel: &str) -> Json {
    parse(&std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")))
        .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

/// The GCE declaration/quote records carry fractional planning
/// members (load_hours 0.5, model_wait_fraction 0.5) — the
/// provider-bytes read preserves them verbatim (R4.5b); every member
/// this emitter consumes is an integer or string.
fn load_provider(repo: &Path, rel: &str) -> Json {
    parse_provider(
        &std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")),
    )
    .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

/// An integer member read int-or-str (provider-projection folds
/// non-integer lexemes to strings verbatim — fp_prereg's rule).
fn int_member(j: Option<&Json>, what: &str) -> i64 {
    match j {
        Some(Json::Int(i)) => *i,
        Some(Json::Str(s)) => s
            .parse::<i64>()
            .unwrap_or_else(|_| panic!("{what}: not an integer string")),
        _ => panic!("{what} absent"),
    }
}

fn write(repo: &Path, rel: &str, body: &str) {
    let p = repo.join(rel);
    std::fs::create_dir_all(p.parent().unwrap_or_else(|| Path::new(".")))
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", p.display()));
    std::fs::write(&p, body).unwrap_or_else(|e| panic!("write {}: {e}", p.display()));
    println!("fp-p4x-quote: wrote {rel}");
}

fn sidecar(hash: &str, rel: &str, sidecar: &str) -> String {
    format!(
        "{hash}  {rel}\n# recompute: shasum -a 256 -c {sidecar} (run from the repo root — the record is canonical Json::to_canonical_string output, CC1)\n"
    )
}

fn inputs() -> BTreeMap<String, String> {
    let manifest = std::env::var("HH_FP_P4X_INPUTS")
        .unwrap_or_else(|_| "scripts/fp-p4x-inputs.txt".to_string());
    let mut out = BTreeMap::new();
    for line in std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("read {manifest}: {e}"))
        .lines()
    {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("bad line: {line}"));
        out.insert(k.trim().to_string(), v.trim().to_string());
    }
    out
}

fn expect_schema(rec: &Json, want: &str, rel: &str) {
    assert_eq!(
        rec.get("schema").and_then(Json::as_str),
        Some(want),
        "{rel} is not {want}"
    );
}

fn arr<'a>(j: &'a Json, what: &str) -> &'a [Json] {
    match j {
        Json::Arr(items) => items,
        _ => panic!("{what} is not an array"),
    }
}

fn str_members<'a>(rec: &'a Json, member: &str) -> Vec<&'a str> {
    let j = rec
        .get(member)
        .unwrap_or_else(|| panic!("prereg.{member} absent"));
    arr(j, &format!("prereg.{member}"))
        .iter()
        .map(|v| {
            v.as_str()
                .unwrap_or_else(|| panic!("prereg.{member} member is not a string"))
        })
        .collect()
}

/// One declared planning-prior arm line — `measure_ref` carries the
/// provenance label verbatim (declared, never measured).
fn slot(id: String, measure_ref: &str) -> ArmMeasure {
    ArmMeasure {
        arm_id: id,
        measure_ref: measure_ref.to_string(),
        mean_trial_micros: DECLARED_P_HAT_MICROS,
        p90_trial_micros: DECLARED_P90_MICROS,
        model_occupied_ms: DECLARED_OCCUPIED_MS,
        queue_wait_ms: 0,
    }
}

fn emit(repo: &Path, inputs: &BTreeMap<String, String>) {
    let prereg = load(repo, &inputs["prereg"]);
    let decl = load_provider(repo, &inputs["compute_decl"]);
    let compute_rec = load_provider(repo, &inputs["compute_quote"]);
    expect_schema(&prereg, "hh-p4-x-prereg/1", &inputs["prereg"]);
    expect_schema(&decl, "hh-compute-decl/1", &inputs["compute_decl"]);
    expect_schema(&compute_rec, "hh-compute-quote/1", &inputs["compute_quote"]);

    // The envelope comes out of the frozen pre-registration — never
    // re-declared here. Levels = design.x0.experiment.arms (baseline +
    // V1–V7 — V8/V9 withheld are absent from the executable set).
    let levels = prereg
        .get("design")
        .and_then(|d| d.get("x0"))
        .and_then(|x| x.get("experiment"))
        .and_then(|e| e.get("arms"))
        .map(|a| arr(a, "prereg.design.x0.experiment.arms"))
        .unwrap_or_else(|| panic!("prereg.design.x0.experiment.arms absent"))
        .iter()
        .map(|a| {
            a.get("arm_id")
                .and_then(Json::as_str)
                .unwrap_or_else(|| panic!("prereg arm without arm_id"))
                .to_string()
        })
        .collect::<Vec<_>>();
    let tasks_x0 = str_members(
        prereg
            .get("tasks")
            .unwrap_or_else(|| panic!("prereg.tasks absent")),
        "x0",
    );
    let tasks_x1 = str_members(
        prereg
            .get("tasks")
            .unwrap_or_else(|| panic!("prereg.tasks absent")),
        "x1",
    );
    let env = prereg
        .get("envelope")
        .unwrap_or_else(|| panic!("prereg.envelope absent"));
    let x0_slots = env
        .get("x0_slots")
        .and_then(Json::as_int)
        .unwrap_or_else(|| panic!("prereg.envelope.x0_slots absent")) as usize;
    let x1_slots = env
        .get("x1_slots")
        .and_then(Json::as_int)
        .unwrap_or_else(|| panic!("prereg.envelope.x1_slots absent")) as usize;
    let nominal_x1 = env
        .get("nominal_x1_slots")
        .and_then(Json::as_int)
        .unwrap_or_else(|| panic!("prereg.envelope.nominal_x1_slots absent"));

    // X0: 3 stratum tasks × 1 replicate × every executable arm.
    let x0_ref = "experiments-spec §10.4 top planning point $0.34/slot for the X0 smoke \
                  (3 dev tasks × 1 × all arms — DECLARED planning prior, never measured)";
    let mut x0_arms = Vec::new();
    for level in &levels {
        for task in &tasks_x0 {
            x0_arms.push(slot(format!("{level}:{task}"), x0_ref));
        }
    }
    assert_eq!(
        x0_arms.len(),
        x0_slots,
        "X0 slot count disagrees with the prereg envelope"
    );

    // X1: 11 band-A dev tasks × k = 5 × every executable arm.
    let x1_ref = "experiments-spec §10.4 top planning point $0.34/slot for the X1 OFAT sweep \
                  (11 band-A dev tasks × k = 5 × all arms — DECLARED planning prior, never measured)";
    let mut x1_arms = Vec::new();
    for level in &levels {
        for task in &tasks_x1 {
            for r in 0..5 {
                x1_arms.push(slot(format!("{level}:{task}:r{r}"), x1_ref));
            }
        }
    }
    assert_eq!(
        x1_arms.len(),
        x1_slots,
        "X1 slot count disagrees with the prereg envelope"
    );

    // The compute line — id + ceiling cited verbatim from the
    // declaration's authorization block, cross-checked against the
    // committed hh-compute-quote/1 record (never re-priced here).
    let auth = decl
        .get("authorization")
        .unwrap_or_else(|| panic!("compute decl: authorization absent"));
    let quote_id = auth
        .get("quote_id")
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("compute decl: authorization.quote_id absent"))
        .to_string();
    let ceiling = int_member(
        auth.get("ceiling_micros"),
        "compute decl: authorization.ceiling_micros",
    );
    let window = decl
        .get("window")
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("compute decl: window absent"))
        .to_string();
    let rec_ceiling = int_member(
        compute_rec
            .get("pricing")
            .and_then(|p| p.get("ceiling_micros")),
        "compute quote: pricing.ceiling_micros",
    );
    assert_eq!(
        ceiling, rec_ceiling,
        "compute decl ceiling disagrees with the hh-compute-quote/1 record"
    );
    assert_eq!(
        compute_rec.get("window").and_then(Json::as_str),
        Some(window.as_str()),
        "compute quote window disagrees with the declaration"
    );
    let compute = vec![ComputeLine {
        window,
        quote_id: quote_id.clone(),
        ceiling_micros: ceiling,
    }];

    let decl_q = ComparativeQuoteDecl {
        t: 1,
        k: 1,
        concurrency_c: CONCURRENCY,
        rows: vec![
            MenuRowDecl {
                row_id: "p4x-x0".to_string(),
                arms: x0_arms,
                infra_allowance_ppm: 0,
                sim: None,
            },
            MenuRowDecl {
                row_id: "p4x-x1".to_string(),
                arms: x1_arms,
                infra_allowance_ppm: 0,
                sim: None,
            },
        ],
        compute,
        envelope: Envelope::PreQuote,
    };
    let quote = build_quote(&decl_q).unwrap_or_else(|e| panic!("comparative quote: {e}"));
    assert!(quote.check_id(), "quote id failed self-check");

    // The declared-inputs record — every value an external gated
    // input, provenance-labelled, schema-tagged.
    let sha_of = |rel: &str| -> String {
        sha256_hex(
            std::fs::read(repo.join(rel))
                .unwrap_or_else(|e| panic!("read {rel}: {e}"))
                .as_slice(),
        )
    };
    let inputs_rec = Json::obj([
        ("schema", Json::str("hh-p4-x-quote-inputs/1")),
        ("emitted_by", Json::str("fp_p4x_quote emit (crates/hh-runner/src/bin/fp_p4x_quote.rs)")),
        ("ticket", Json::str("GATE-G35")),
        (
            "records",
            Json::obj([
                (
                    "prereg",
                    Json::obj([
                        ("path", Json::str(&inputs["prereg"])),
                        ("sha256", Json::str(sha_of(&inputs["prereg"]))),
                        ("schema", Json::str("hh-p4-x-prereg/1")),
                    ]),
                ),
                (
                    "compute_decl",
                    Json::obj([
                        ("path", Json::str(&inputs["compute_decl"])),
                        ("sha256", Json::str(sha_of(&inputs["compute_decl"]))),
                        ("schema", Json::str("hh-compute-decl/1")),
                    ]),
                ),
                (
                    "compute_quote",
                    Json::obj([
                        ("path", Json::str(&inputs["compute_quote"])),
                        ("sha256", Json::str(sha_of(&inputs["compute_quote"]))),
                        ("schema", Json::str("hh-compute-quote/1")),
                        ("quote_id", Json::str(&quote_id)),
                        ("priced_by", Json::str("receipt_lib.py quote_doc — the one §3.10 path (R6.14)")),
                    ]),
                ),
            ]),
        ),
        (
            "declared_model_priors",
            Json::obj([
                ("p_hat_micros_per_slot", Json::Int(DECLARED_P_HAT_MICROS)),
                ("p90_micros_per_slot", Json::Int(DECLARED_P90_MICROS)),
                ("model_occupied_ms_per_slot", Json::Int(DECLARED_OCCUPIED_MS as i64)),
                ("infra_allowance_ppm", Json::Int(0)),
                (
                    "basis",
                    Json::str(
                        "experiments-spec §10.4 planning points ($0.05/$0.15/$0.34 per trial); the \
                         packet prices the top point only — DECLARED planning priors, never \
                         measurements (no X0/X1-measured per-level $/trial exists yet)",
                    ),
                ),
            ]),
        ),
        (
            "envelope",
            Json::obj([
                ("x0_slots", Json::Int(x0_slots as i64)),
                ("x1_slots", Json::Int(x1_slots as i64)),
                ("nominal_x1_slots", Json::Int(nominal_x1)),
                ("arms", Json::Arr(levels.iter().map(Json::str).collect())),
                (
                    "withheld",
                    prereg
                        .get("withheld")
                        .cloned()
                        .unwrap_or(Json::Arr(vec![])),
                ),
                ("concurrency_c", Json::Int(CONCURRENCY as i64)),
                ("envelope_label", Json::str("pre-quote")),
            ]),
        ),
        (
            "honesty",
            Json::obj([
                ("authorizes_nothing", Json::Bool(true)),
                ("model_line_source", Json::str("declared planning priors — not a measurement")),
                ("envelope_adr", Json::str("ADR-0435")),
                (
                    "note",
                    Json::str(
                        "the packet mints the priced menu only; REQ-XH-004's verbatim model + \
                         compute authorizations are recorded at GATE-G35 by the operator",
                    ),
                ),
            ]),
        ),
    ]);

    for (key, doc) in [("quote", quote.to_json()), ("quote_inputs", inputs_rec)] {
        let rel = &inputs[key];
        let bytes = format!("{}\n", doc.to_canonical_string());
        write(repo, rel, &bytes);
        let sha_key = format!("{key}_sha256");
        let sha_rel = &inputs[&sha_key];
        write(
            repo,
            sha_rel,
            &sidecar(&sha256_hex(bytes.as_bytes()), rel, sha_rel),
        );
    }
    println!(
        "fp-p4x-quote: comparative-quote/1 id {} — x0 {x0_slots} slots, x1 {x1_slots} slots, compute {quote_id} {ceiling} µ$",
        quote.quote_id
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "emit".to_string());
    let repo = PathBuf::from(args.next().unwrap_or_else(|| ".".to_string()));
    let inputs = inputs();
    match cmd.as_str() {
        "emit" => emit(&repo, &inputs),
        other => panic!("unknown command: {other}"),
    }
}
