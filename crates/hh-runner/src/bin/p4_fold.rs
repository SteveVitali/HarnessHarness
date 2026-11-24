//! `p4-fold` — the R7.4 emitter (manifest 330). Reads the committed
//! X1 evidence named by the inputs manifest — the captured slot
//! matrix, the detail_lost list, the WAL slot-cost map and model-event
//! extract, the gen-3 x2 records (registry, levels, the operative
//! `hh-p4-x-prereg/1` whose byte-pinned `selection_rule` member the
//! selection decodes) and the frozen `task-selection/1` record — and
//! mints, each with its sha256 sidecar at the manifest's output
//! relpaths:
//!
//! - `hh-p4-x-exploration/1` — the exploratory fold (per-level Δ̂,
//!   descriptive bootstrap + FM intervals, sign-flip p, BH q,
//!   activation, fidelity, completion envelope, costs — all
//!   exploratory-labelled; stops and withheld dispositions verbatim);
//! - `hh-p4-selection/1` — the deterministic selection packet: the
//!   frozen §8.5 rule executed over the fold, pure function of the
//!   committed X1 records, `recorded_seq` 1 — its hash predates the
//!   confirmation pre-registration (REQ-XD-212);
//! - `hh-p4-c-prereg/1` — the frozen confirmation pre-registration:
//!   the baseline re-run + the selection packet's member list verbatim
//!   over the T = 37 held-out census at k = 5, Holm over the
//!   member-vs-baseline family, `recorded_seq` 2, citing the packet
//!   hash (REQ-XD-217). Under `withheld{no_candidate}` it records the
//!   pre-registered disposition — no subject is scheduled.
//!
//! `HH_FP_P4X_FOLD_INPUTS` names the manifest
//! (`scripts/fp-p4x-fold-inputs.txt` by default). Deterministic —
//! same inputs, byte-identical outputs. No network, no clock, $0.
//!
//! ```text
//! p4-fold emit <repo-root>
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
    println!("p4-fold: wrote {rel}");
}

fn sidecar(hash: &str, rel: &str, sidecar: &str) -> String {
    format!(
        "{hash}  {rel}\n# recompute: shasum -a 256 -c {sidecar} (run from the repo root — the record is canonical Json::to_canonical_string output, CC1)\n"
    )
}

fn inputs() -> BTreeMap<String, String> {
    let manifest = std::env::var("HH_FP_P4X_FOLD_INPUTS")
        .unwrap_or_else(|_| "scripts/fp-p4x-fold-inputs.txt".to_string());
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
    let matrix_rel = &inputs["slot_matrix"];
    let lost_rel = &inputs["slots_lost"];
    let costs_rel = &inputs["wal_slot_costs"];
    let events_rel = &inputs["wal_model_events"];
    let wal_rel = &inputs["session_wal"];
    let fragment_rel = &inputs["window_fragment"];
    let registry = load(repo, &inputs["sweep_registry"]);
    let levels_rec = load(repo, &inputs["levels"]);
    let prereg = load(repo, &inputs["prereg"]);
    let task_selection = load(repo, &inputs["task_selection"]);
    expect_schema(&registry, "sweep-registry/1", &inputs["sweep_registry"]);
    expect_schema(&levels_rec, "hh-p4-levels/1", &inputs["levels"]);
    expect_schema(&prereg, "hh-p4-x-prereg/1", &inputs["prereg"]);
    expect_schema(
        &task_selection,
        "task-selection/1",
        &inputs["task_selection"],
    );
    let matrix = std::fs::read_to_string(repo.join(matrix_rel))
        .unwrap_or_else(|e| panic!("read {matrix_rel}: {e}"));
    let lost = load(repo, lost_rel);
    let costs = load(repo, costs_rel);
    let events = std::fs::read_to_string(repo.join(events_rel))
        .unwrap_or_else(|e| panic!("read {events_rel}: {e}"));

    // The selection input manifest — committed X1 records only,
    // cohort `x1` (the check refuses held-out and foreign-outcome
    // entries typed; the envelope records ride with cohort x1 as the
    // declared design inputs the X1 fold consumes).
    let sha_of = |rel: &str| -> String {
        format!(
            "sha256:{}",
            sha256_hex(
                std::fs::read(repo.join(rel))
                    .unwrap_or_else(|e| panic!("read {rel}: {e}"))
                    .as_slice(),
            )
        )
    };
    let manifest_entries: Vec<(String, String)> = [
        matrix_rel,
        lost_rel,
        costs_rel,
        events_rel,
        wal_rel,
        fragment_rel,
        &inputs["sweep_registry"],
        &inputs["levels"],
        &inputs["prereg"],
    ]
    .iter()
    .map(|rel| (rel.to_string(), sha_of(rel)))
    .collect();
    let manifest_refs: Vec<(&str, &str)> = manifest_entries
        .iter()
        .map(|(r, s)| (r.as_str(), s.as_str()))
        .collect();

    let (exploration, packet, c_prereg) = hharness_runner::fp_p4x_fold::run_fold(
        &matrix,
        &lost,
        &events,
        &costs,
        &registry,
        &levels_rec,
        &prereg,
        &task_selection,
        &manifest_refs,
    )
    .unwrap_or_else(|es| panic!("the fold refused: {es:?}"));

    for (key, doc) in [
        ("exploration", exploration),
        ("selection", packet),
        ("c_prereg", c_prereg),
    ] {
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
