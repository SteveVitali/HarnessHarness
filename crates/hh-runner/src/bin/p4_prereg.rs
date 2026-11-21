//! `p4-prereg` — the R7.2 emitter (manifest 327). Reads the committed
//! R7.1 records named by the inputs manifest — the frozen x2
//! `sweep-registry/1`, `hh-p4-levels/1`, `hh-p4-w-rule/1` and
//! `hh-p4-admissibility/1` — plus the frozen `task-selection/1` record
//! the band-A development manifest comes from, and mints the frozen
//! `hh-p4-x-prereg/1` exploration pre-registration (REQ-XD-206/210/211:
//! the declared X0/X1 envelope, the verbatim stop-rule set, and the
//! §8.5 frozen selection rule byte-equal to what the machinery
//! executes) plus the `p4-selection-rule/1` rule record it cites —
//! each with its sha256 sidecar at the manifest's output relpaths.
//! `HH_FP_P4X_INPUTS` names the manifest (`scripts/fp-p4x-inputs.txt`
//! by default). Deterministic — same inputs, byte-identical outputs.
//! No network, no clock (recorded_at is the declared constant); the
//! hash-before-subjects discipline means this record is minted before
//! any GATE-G35 window and authorizes no spend.
//!
//! ```text
//! p4-prereg emit <repo-root>
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_wire::json::{parse, Json};
use hharness_wire::sha256::sha256_hex;

fn load(repo: &Path, rel: &str) -> Json {
    parse(&std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")))
        .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

fn write(repo: &Path, rel: &str, body: &str) {
    let p = repo.join(rel);
    std::fs::create_dir_all(p.parent().unwrap_or_else(|| Path::new(".")))
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", p.display()));
    std::fs::write(&p, body).unwrap_or_else(|e| panic!("write {}: {e}", p.display()));
    println!("p4-prereg: wrote {rel}");
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

fn emit(repo: &Path, inputs: &BTreeMap<String, String>) {
    let reg = load(repo, &inputs["sweep_registry"]);
    let levels = load(repo, &inputs["levels"]);
    let w_rule = load(repo, &inputs["w_rule"]);
    let admissibility = load(repo, &inputs["admissibility"]);
    let task_selection = load(repo, &inputs["task_selection"]);
    expect_schema(&reg, "sweep-registry/1", &inputs["sweep_registry"]);
    expect_schema(&levels, "hh-p4-levels/1", &inputs["levels"]);
    expect_schema(&w_rule, "hh-p4-w-rule/1", &inputs["w_rule"]);
    expect_schema(
        &admissibility,
        "hh-p4-admissibility/1",
        &inputs["admissibility"],
    );
    expect_schema(
        &task_selection,
        "task-selection/1",
        &inputs["task_selection"],
    );

    // The consumed registry re-validates through the R7.1 validator —
    // the pre-registration consumes the frozen record, never a copy.
    let reg_refusals = hharness_lab::sweep::check_registry(&reg);
    assert!(
        reg_refusals.is_empty(),
        "registry refused: {reg_refusals:?}"
    );

    let rec = hharness_lab::p4x::x_prereg_record(
        &reg,
        &levels,
        &w_rule,
        &admissibility,
        &task_selection,
        &inputs["sweep_registry"],
        &inputs["w_rule"],
        &inputs["task_selection"],
    )
    .unwrap_or_else(|e| panic!("pre-registration refused: {e}"));
    hharness_lab::p4x::check_registry_cite(&reg, &rec)
        .unwrap_or_else(|e| panic!("registry citation refused: {e}"));

    // The frozen selection rule — the byte-pinned member the
    // pre-registration cites by hash, emitted as its own record so the
    // byte-equality is a committed artifact, not an assertion.
    let rule = hharness_lab::p4x::frozen_rule_json();
    for (key, doc) in [("prereg", &rec), ("selection_rule", &rule)] {
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
