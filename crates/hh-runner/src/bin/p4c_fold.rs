//! `p4c-fold` — the R7.6 emitter (manifest 333). Reads the committed
//! P4-C records named by the inputs manifest — the frozen
//! `hh-p4-c-prereg/1`, the `hh-p4-selection/1` packet it cites, and the
//! executed `hh-fp-window/1` r4 window record (whose `slot_results`
//! member embeds the 370-slot matrix verbatim) plus the earlier
//! terminal-window records the packet's terminal-state member carries —
//! and mints, with its sha256 sidecar at the manifest's output
//! relpath:
//!
//! - `hh-p4-c-confirmation/1` — the confirmation packet: per-member
//!   finite-manifest equal-task Δ vs the in-window baseline re-run
//!   (joint population), the FM credible + task-bootstrap interval
//!   members, the two-sided sharp-null sign-flip family under
//!   `slot_schedule_randomization` with Holm α = .05 (unavailable
//!   members retained), the pre-registered typed dispositions
//!   (REQ-XD-219), the §8.8 two-statement claims surface with the
//!   mandatory labels + selection-labelled exploratory estimates +
//!   descriptive-only cost, the exchangeability record, and the
//!   terminal-state member CAP7.1 consumes.
//!
//! `HH_FP_P4C_FOLD_INPUTS` names the manifest
//! (`scripts/fp-p4c-fold-inputs.txt` by default). Deterministic —
//! same inputs, byte-identical outputs. No network, no clock, $0.
//!
//! ```text
//! p4c-fold emit <repo-root>
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_runner::fp_p4c_fold;
use hharness_wire::json::{parse, parse_provider, Json};
use hharness_wire::sha256::sha256_hex;

fn load(repo: &Path, rel: &str) -> Json {
    parse(&std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")))
        .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

/// The window records carry provider-side fractional members
/// (`deadline_s` etc.) — the inbound read keeps them verbatim as
/// strings (the `parse_provider` rule).
fn load_window(repo: &Path, rel: &str) -> Json {
    parse_provider(
        &std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")),
    )
    .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

fn write(repo: &Path, rel: &str, body: &str) {
    let p = repo.join(rel);
    std::fs::create_dir_all(p.parent().unwrap_or_else(|| Path::new(".")))
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", p.display()));
    std::fs::write(&p, body).unwrap_or_else(|e| panic!("write {}: {e}", p.display()));
    println!("p4c-fold: wrote {rel}");
}

fn sidecar(hash: &str, rel: &str, sidecar: &str) -> String {
    format!(
        "{hash}  {rel}\n# recompute: shasum -a 256 -c {sidecar} (run from the repo root — the record is canonical Json::to_canonical_string output, CC1)\n"
    )
}

fn inputs() -> BTreeMap<String, String> {
    let manifest = std::env::var("HH_FP_P4C_FOLD_INPUTS")
        .unwrap_or_else(|_| "scripts/fp-p4c-fold-inputs.txt".to_string());
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
    let prereg_rel = &inputs["prereg"];
    let selection_rel = &inputs["selection"];
    let window_rel = &inputs["window"];
    let earlier_rels: Vec<&String> = inputs
        .iter()
        .filter(|(k, _)| k.starts_with("window_history"))
        .map(|(_, v)| v)
        .collect();
    let prereg = load(repo, prereg_rel);
    let selection = load(repo, selection_rel);
    let window = load_window(repo, window_rel);
    let earlier: Vec<(&str, Json)> = earlier_rels
        .iter()
        .map(|rel| (rel.as_str(), load_window(repo, rel)))
        .collect();
    expect_schema(&prereg, "hh-p4-c-prereg/1", prereg_rel);
    expect_schema(&selection, "hh-p4-selection/1", selection_rel);
    expect_schema(&window, "hh-fp-window/1", window_rel);

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
    let mut entries: Vec<(&str, String)> = vec![
        (prereg_rel.as_str(), sha_of(prereg_rel)),
        (selection_rel.as_str(), sha_of(selection_rel)),
        (window_rel.as_str(), sha_of(window_rel)),
    ];
    for rel in &earlier_rels {
        entries.push((rel.as_str(), sha_of(rel)));
    }
    let manifest_entries: Vec<(&str, &str)> =
        entries.iter().map(|(r, s)| (*r, s.as_str())).collect();
    let manifest = fp_p4c_fold::input_manifest(&manifest_entries);

    let earlier_refs: Vec<(&str, &Json)> = earlier.iter().map(|(r, j)| (*r, j)).collect();
    let packet = fp_p4c_fold::run_fold_c(
        &prereg,
        Some(&selection),
        Some(&window),
        &earlier_refs,
        &manifest,
    )
    .unwrap_or_else(|e| panic!("fold: {e}"));

    let body = packet.to_canonical_string();
    let out_rel = &inputs["confirmation"];
    write(repo, out_rel, &body);
    let hash = sha256_hex(body.as_bytes());
    if let Some(side_rel) = inputs.get("confirmation_sha256") {
        write(repo, side_rel, &sidecar(&hash, out_rel, side_rel));
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("emit"), Some(root)) => emit(Path::new(&root), &inputs()),
        _ => {
            eprintln!("usage: p4c-fold emit <repo-root>");
            std::process::exit(2);
        }
    }
    let _ = PathBuf::new();
}
