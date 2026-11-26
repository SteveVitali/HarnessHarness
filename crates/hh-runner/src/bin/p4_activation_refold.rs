//! `p4-activation-refold` — the R7.9 emitter (manifest 342). Reads the
//! committed `hh-p4-activation-corpus/1` record named by the inputs
//! manifest plus the cited evidence records (the gen-3
//! `sweep-registry/1` the folds amend toward), re-checks the corpus's
//! `input_manifest.selected_fields` for outcome members before any
//! value is read (REQ-XD-205/209), and mints the
//! `hh-p4-activation-refold/1` record — per-level `fold_activation` +
//! `forecast_entry` verdicts for V3/V4/V5 — plus its sha256 sidecar at
//! the manifest's output relpaths.
//!
//! `HH_FP_P4_REFOLD_INPUTS` names the manifest
//! (`scripts/fp-p4-refold-inputs.txt` by default). Deterministic —
//! same inputs, byte-identical outputs. No network, no clock, $0.
//!
//! ```text
//! p4-activation-refold emit <repo-root>
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_runner::fp_p4_refold;
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
    println!("p4-activation-refold: wrote {rel}");
}

fn sidecar(hash: &str, rel: &str, sidecar: &str) -> String {
    format!(
        "{hash}  {rel}\n# recompute: shasum -a 256 -c {sidecar} (run from the repo root — the record is canonical Json::to_canonical_string output, CC1)\n"
    )
}

fn inputs() -> BTreeMap<String, String> {
    let manifest = std::env::var("HH_FP_P4_REFOLD_INPUTS")
        .unwrap_or_else(|_| "scripts/fp-p4-refold-inputs.txt".to_string());
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

fn emit(repo: &Path, inputs: &BTreeMap<String, String>) {
    let corpus_rel = &inputs["corpus"];
    let corpus = load(repo, corpus_rel);
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
    // The input manifest: corpus first, then every `cite_N=` member in
    // lexical order (the registry record the folds' dispositions cite).
    let mut entries: Vec<(String, String)> = vec![(corpus_rel.clone(), sha_of(corpus_rel))];
    let mut cites: Vec<&String> = inputs
        .iter()
        .filter(|(k, _)| k.starts_with("cite"))
        .map(|(_, v)| v)
        .collect();
    cites.sort();
    for rel in cites {
        entries.push((rel.clone(), sha_of(rel)));
    }
    let manifest_entries: Vec<(&str, &str)> = entries
        .iter()
        .map(|(r, s)| (r.as_str(), s.as_str()))
        .collect();

    let rec = fp_p4_refold::refold_record(&corpus, &manifest_entries)
        .unwrap_or_else(|e| panic!("refold refused: {e}"));
    // `recorded_at` rides the manifest's declared date — the emitter
    // is deterministic, no clock.
    let rec = if let Some(d) = inputs.get("recorded_at") {
        let Json::Obj(mut m) = rec else {
            unreachable!()
        };
        m.insert("recorded_at".to_string(), Json::str(d.clone()));
        Json::Obj(m)
    } else {
        rec
    };
    let out_rel = &inputs["out"];
    let bytes = format!("{}\n", rec.to_canonical_string());
    write(repo, out_rel, &bytes);
    let sha = sha256_hex(bytes.as_bytes());
    let sidecar_rel = inputs
        .get("out_sha256")
        .cloned()
        .unwrap_or_else(|| format!("{out_rel}.sha256"));
    write(repo, &sidecar_rel, &sidecar(&sha, out_rel, &sidecar_rel));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let repo = PathBuf::from(if args.len() > 2 && args[1] == "emit" {
        args[2].clone()
    } else {
        eprintln!("usage: p4-activation-refold emit <repo-root>");
        std::process::exit(2);
    });
    emit(&repo, &inputs());
}
