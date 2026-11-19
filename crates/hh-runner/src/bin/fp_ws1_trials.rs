//! `fp-ws1-trials` — the R6.46 `hh-fp-trials/1` extractor. Reads the
//! retained Ws1 proxy WAL runs and evidence rows named by the inputs
//! manifest (`HH_WS1_TRIALS_INPUTS`; default
//! `scripts/fp-ws1-trials-inputs.txt`, newline `name=path` pairs —
//! paths may be absolute: these are retained-store inputs, read
//! only), folds them through `fp_ws1::extract_trials`, and writes
//! the committed record + merged PP0 evidence + sha256 sidecars at
//! the manifest's `out_*` relpaths. Every input's sha256 is recorded
//! inside the emitted records — committed records never carry the
//! absolute paths. Deterministic; no network, no clock.
//!
//! ```text
//! fp-ws1-trials emit <repo-root>
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_runner::fp_ws1::{
    corpus_evidence, extract_trials, merge_pp0_snapshots, merged_pp0_decl, pp0_evidence,
    SlotEvidence,
};
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

fn manifest(path: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .lines()
    {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("manifest line {line:?} is not name=path"));
        out.push((k.trim().to_string(), v.trim().to_string()));
    }
    out
}

fn expand(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return Path::new(&home).join(rest);
        }
    }
    PathBuf::from(path)
}

fn read_source(name: &str, path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {name} {}: {e}", path.display()))
}

fn load_json(name: &str, bytes: &[u8]) -> Json {
    hharness_wire::json::parse(
        std::str::from_utf8(bytes).unwrap_or_else(|_| panic!("{name}: utf-8")),
    )
    .unwrap_or_else(|e| panic!("{name}: parse: {e}"))
}

fn write(repo: &Path, rel: &str, doc: &Json) {
    let canonical = doc.to_canonical_string() + "\n";
    let p = repo.join(rel);
    std::fs::create_dir_all(p.parent().unwrap_or(Path::new(".")))
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", p.display()));
    std::fs::write(&p, &canonical).unwrap_or_else(|e| panic!("write {rel}: {e}"));
    let hash = sha256_hex(canonical.as_bytes());
    std::fs::write(
        repo.join(format!("{rel}.sha256")),
        format!("{hash}  {rel}\n# recompute: shasum -a 256 -c {rel}.sha256 (run from the repo root — the record is canonical Json::to_canonical_string output, CC1)\n"),
    )
    .unwrap_or_else(|e| panic!("write {rel}.sha256: {e}"));
    println!("fp-ws1-trials: wrote {rel}  sha256:{hash}");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    let repo = PathBuf::from(args.next().unwrap_or_else(|| ".".to_string()));
    if cmd != "emit" {
        eprintln!("usage: fp-ws1-trials emit <repo-root>");
        std::process::exit(2);
    }
    let mp = std::env::var("HH_WS1_TRIALS_INPUTS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo.join("scripts/fp-ws1-trials-inputs.txt"));
    let m = manifest(&mp);
    let mget = |k: &str| -> &str {
        m.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
            .unwrap_or_else(|| panic!("manifest member {k} required"))
    };

    // ── WAL sources (name wal-<run_id> = path) ──────────────────────
    let mut runs: Vec<(String, Vec<u8>)> = Vec::new();
    let mut digests: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in &m {
        if k == "corpus_manifest" || k.starts_with("pp0_") || k.starts_with("out_") {
            continue;
        }
        if let Some(run_id) = k.strip_prefix("wal_") {
            let bytes = read_source(k, &expand(v));
            digests.insert(k.clone(), format!("sha256:{}", sha256_hex(&bytes)));
            runs.push((run_id.to_string(), bytes));
        }
    }
    if runs.is_empty() {
        panic!("manifest names no wal_<run> sources");
    }
    runs.sort();

    // ── corpus manifest → slot evidence ─────────────────────────────
    let man_path = mget("corpus_manifest");
    let man_bytes = read_source("corpus_manifest", &expand(man_path));
    digests.insert(
        "corpus_manifest".into(),
        format!("sha256:{}", sha256_hex(&man_bytes)),
    );
    let man = load_json("corpus_manifest", &man_bytes);
    let mut evidence: BTreeMap<String, SlotEvidence> = corpus_evidence(&man)
        .into_iter()
        .map(|e| (e.slot.clone(), e))
        .collect();

    // ── PP0 snapshots → merged union-of-failures evidence ───────────
    let mut snapshots: Vec<Vec<Json>> = Vec::new();
    let mut pp0_files: Vec<(String, Json)> = Vec::new();
    for (k, v) in &m {
        if !k.starts_with("pp0_evidence") {
            continue;
        }
        let dir = expand(v);
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("ws1-pp0-") && n.ends_with(".json"))
            })
            .collect();
        files.sort();
        let mut rows = Vec::new();
        for f in files {
            let bytes = read_source(k, &f);
            let fname = f.file_name().unwrap().to_str().unwrap().to_string();
            digests.insert(
                format!("{k}/{fname}"),
                format!("sha256:{}", sha256_hex(&bytes)),
            );
            let j = load_json(&fname, &bytes);
            rows.push(j.clone());
            pp0_files.push((fname, j));
        }
        snapshots.push(rows);
    }
    if snapshots.is_empty() {
        panic!("manifest names no pp0_evidence* dirs");
    }
    let merged = merge_pp0_snapshots(&snapshots);
    // Trials-row evidence from the merged PP0 rows.
    for e in pp0_evidence(&merged) {
        evidence.insert(e.slot.clone(), e);
    }

    let record =
        extract_trials(&runs, &evidence, &digests).unwrap_or_else(|e| panic!("extract: {e}"));

    let pp0_decl = merged_pp0_decl(&merged);
    let pp0_rec = Json::obj([
        ("schema", Json::str("hh-pp0-ag-evidence/1")),
        ("window", Json::str("ws1")),
        (
            "merge",
            Json::str("union-of-failures across the retained snapshots — a recorded failure is never rescued (append-only)"),
        ),
        (
            "sources",
            Json::Obj(
                digests
                    .iter()
                    .filter(|(k, _)| k.starts_with("pp0_"))
                    .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                    .collect(),
            ),
        ),
        (
            "pp0",
            Json::Obj(
                pp0_decl
                    .into_iter()
                    .map(|(a, rows)| (a, Json::Arr(rows)))
                    .collect(),
            ),
        ),
    ]);

    write(&repo, mget("out_trials"), &record);
    write(&repo, mget("out_pp0"), &pp0_rec);
}
