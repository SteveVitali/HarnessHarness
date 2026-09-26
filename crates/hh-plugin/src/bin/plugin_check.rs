//! `hh-plugin-check` — the extensibility readiness binary (spec §8.4 §2/§3;
//! ticket S1.27; the run-ledger's live evidence).
//!
//! - `hh-plugin-check spec-dag [SPEC]` — parse the §4.4 dependency tables out
//!   of `spec/CANONICAL_SPEC.md` (default `$repo/spec/CANONICAL_SPEC.md`), run
//!   X2 (tier monotonicity + acyclicity), print
//!   `nodes=N edges=M tier_violations=[…] cycles=[…]` — the readiness gate's
//!   "no tier violation, no cycle" (AC-8). Class/plugin `depends_on` extension
//!   of the same graph is exercised by the `hh-registry` acceptance test
//!   (`plugin_architecture.rs`) — the binary's job is the canonical §4.4 data
//!   plane (a row edit that breaks a rule fails the next run).
//! - `hh-plugin-check module-lint [ROOT]` — lint every first-party in-process
//!   variant module (`crates/*/src/variants/**`) under `ROOT` (default the
//!   repo root); prints each violation and exits non-zero on any (AC-9).
//!
//! Exit 0 = clean; 1 = findings; 2 = usage/IO error.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use hh_plugin::{lint_workspace, spec_dag_check};

fn repo_root() -> PathBuf {
    // The binary runs from the workspace; CARGO_MANIFEST_DIR is the crate dir.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(cmd) = args.first().map(String::as_str) else {
        eprintln!("usage: hh-plugin-check <spec-dag|module-lint> [path]");
        return ExitCode::from(2);
    };
    match cmd {
        "spec-dag" => {
            let spec = args
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| repo_root().join("spec/CANONICAL_SPEC.md"));
            let text = match std::fs::read_to_string(&spec) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("hh-plugin-check: cannot read {}: {e}", spec.display());
                    return ExitCode::from(2);
                }
            };
            let report = spec_dag_check(&text, &[], &[]);
            println!(
                "nodes={} edges={} ks_edges={} tier_violations={:?} cycles={:?} unknown_refs={:?} parse_errors={:?}",
                report.nodes,
                report.edges,
                report.ks_edges,
                report
                    .inner
                    .tier_violations
                    .iter()
                    .map(|e| format!("{}→{}", e.from, e.to))
                    .collect::<Vec<_>>(),
                report.inner.cycles,
                report.unknown_refs,
                report.parse_errors,
            );
            if report.is_clean() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        "module-lint" => {
            let root = args.get(1).map(PathBuf::from).unwrap_or_else(repo_root);
            let report = lint_workspace(&root);
            println!(
                "files={} variant_modules={} violations={}",
                report.files,
                report.variant_modules,
                report.violations.len()
            );
            for v in &report.violations {
                println!("{}:{}: {:?}: {}", v.file, v.line, v.rule, v.edge);
            }
            if report.is_clean() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        _ => {
            eprintln!("hh-plugin-check: unknown command `{cmd}`");
            ExitCode::from(2)
        }
    }
}
