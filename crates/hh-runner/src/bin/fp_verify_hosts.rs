//! `fp-verify-hosts` — the R6.51 `hh-verify-host-derivation/1` emitter
//! (DF-R6.45l-1's discharge). Reads the evidence named by the inputs
//! manifest (`HH_VERIFY_HOSTS_INPUTS`; default
//! `scripts/fp-verify-hosts-inputs.txt`, newline `name=path` pairs —
//! paths may be absolute or `~/…`: the mediator stores and phase
//! layers are retained-store inputs, read only), folds them through
//! [`hharness_runner::verify_hosts::derive_verify_hosts`], and writes
//! the committed record + sha256 sidecar at the manifest's `out`
//! relpath. Every input's sha256 is recorded inside the record under
//! `sources` — committed records never carry the absolute paths.
//! Deterministic; no network, no clock.
//!
//! Manifest members:
//!   `window`                 — the record's window label (`smoke1`)
//!   `smoke_record`           — a committed `hh-smoke/1` record; each
//!                            `cells.<ref>.denied_hosts` +
//!                            `cells.<ref>.store.deny_hosts` folds as
//!                            the cell's captured observation
//!   `wal_<cell>`             — a mediator-store `events.wal` (the
//!                            durable `security.egress.decided` rows)
//!   `policy_<cell>`          — the retained phase-layer `policy.json`
//!                            (the verify layer doc — the last phase
//!                            swap's write; its `version_id` is the
//!                            cell's verify attribution key)
//!   `report_<cell>`          — a harvested `cell-report.json`
//!                            (`denied_hosts` member)
//!   `out`                    — the emitted record's repo-relative path
//!
//! ```text
//! fp-verify-hosts emit <repo-root>
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_runner::json_ext::JsonExt;
use hharness_runner::verify_hosts::{derive_verify_hosts, CellEvidence, ReportObservation};
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

fn resolve(repo: &Path, path: &str) -> PathBuf {
    let p = expand(path);
    if p.is_absolute() {
        p
    } else {
        repo.join(p)
    }
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

fn str_list(j: Option<&Json>) -> Vec<String> {
    j.and_then(|v| match v {
        Json::Arr(a) => Some(
            a.iter()
                .filter_map(|h| h.as_str().map(str::to_string))
                .collect(),
        ),
        _ => None,
    })
    .unwrap_or_default()
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
    println!("fp-verify-hosts: wrote {rel}  sha256:{hash}");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    let repo = PathBuf::from(args.next().unwrap_or_else(|| ".".to_string()));
    if cmd != "emit" {
        eprintln!("usage: fp-verify-hosts emit <repo-root>");
        std::process::exit(2);
    }
    let mp = std::env::var("HH_VERIFY_HOSTS_INPUTS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo.join("scripts/fp-verify-hosts-inputs.txt"));
    let m = manifest(&mp);
    let mget = |k: &str| -> &str {
        m.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
            .unwrap_or_else(|| panic!("manifest member {k} required"))
    };

    let mut cells: BTreeMap<String, CellEvidence> = BTreeMap::new();
    let mut sources: BTreeMap<String, String> = BTreeMap::new();
    let load = |name: &str, path: &str, sources: &mut BTreeMap<String, String>| -> Vec<u8> {
        let bytes = read_source(name, &resolve(&repo, path));
        sources.insert(name.to_string(), format!("sha256:{}", sha256_hex(&bytes)));
        bytes
    };

    // ── committed smoke record → per-cell captured observations ────
    if let Some((_, path)) = m.iter().find(|(k, _)| k == "smoke_record") {
        let bytes = load("smoke_record", path, &mut sources);
        let rec = load_json("smoke_record", &bytes);
        let cells_j = rec
            .get("cells")
            .and_then(|c| c.as_obj().cloned())
            .unwrap_or_default();
        for (cref, cj) in &cells_j {
            let obs = ReportObservation {
                kind: "hh-smoke/1.cells.denied_hosts".to_string(),
                denied_hosts: str_list(cj.get("denied_hosts")),
                observed_deny_hosts: str_list(cj.get("store").and_then(|s| s.get("deny_hosts"))),
            };
            cells
                .entry(cref.clone())
                .or_insert_with(|| CellEvidence {
                    cell: cref.clone(),
                    ..Default::default()
                })
                .reports
                .push(obs);
        }
    }

    // ── retained mediator stores + phase layers + cell reports ─────
    for (k, v) in &m {
        if let Some(cref) = k.strip_prefix("wal_") {
            let bytes = load(k, v, &mut sources);
            cells
                .entry(cref.to_string())
                .or_insert_with(|| CellEvidence {
                    cell: cref.to_string(),
                    ..Default::default()
                })
                .wal = Some(bytes);
        } else if let Some(cref) = k.strip_prefix("policy_") {
            let bytes = load(k, v, &mut sources);
            let pol = load_json(k, &bytes);
            let vid = pol
                .get("version_id")
                .and_then(Json::as_str)
                .unwrap_or_else(|| panic!("{k}: policy doc carries no version_id"))
                .to_string();
            cells
                .entry(cref.to_string())
                .or_insert_with(|| CellEvidence {
                    cell: cref.to_string(),
                    ..Default::default()
                })
                .verify_version_id = Some(vid);
        } else if let Some(cref) = k.strip_prefix("report_") {
            let bytes = load(k, v, &mut sources);
            let rep = load_json(k, &bytes);
            let obs = ReportObservation {
                kind: "cell-report.denied_hosts".to_string(),
                denied_hosts: str_list(rep.get("denied_hosts")),
                observed_deny_hosts: str_list(rep.get("store").and_then(|s| s.get("deny_hosts"))),
            };
            cells
                .entry(cref.to_string())
                .or_insert_with(|| CellEvidence {
                    cell: cref.to_string(),
                    ..Default::default()
                })
                .reports
                .push(obs);
        }
    }

    let window = mget("window").to_string();
    let evidence: Vec<CellEvidence> = cells.into_values().collect();
    let record = derive_verify_hosts(&window, &evidence, &sources);
    write(&repo, mget("out"), &record);
}
