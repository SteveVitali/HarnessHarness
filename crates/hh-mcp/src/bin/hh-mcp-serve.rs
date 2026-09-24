//! `hh-mcp-serve` — the Stage-3 fixture MCP server (R-2.11.3⁰;
//! ADR-0097 D7; the R-2.5.4⁰ edge slice, S3.9).
//!
//! `hh-mcp-serve [--legacy] [--watch] --bundle <dir> | --container <hhb1>`
//! decodes the bundle, lowers its `target:mcp` member into the
//! `hh-mcp-artifact/1` record and serves the newline-JSON-RPC loop over
//! stdin/stdout under the fixed `stdio_launch` test principal.
//!
//! Flags:
//! - `--legacy` — serve the pinned legacy era: `server/discover` is
//!   `-32601`, `initialize` echoes the legacy pin, `tools/list` carries
//!   no `resultType`/freshness members (the compatibility profile the
//!   client's probe detects — ADR-0099 N1).
//! - `--watch` — re-decode the bundle on every loop iteration; a
//!   `catalogue_hash` change emits `notifications/tools/list_changed`
//!   before the next answer (AC-R-2.5.4-2). A failed reload keeps the
//!   last good artifact — a corrupt bundle mid-session degrades to
//!   "unchanged", never to a crashed server.
//!
//! The process is a pure function of `(artifact, binding)` — it opens
//! no store, reads no environment, holds no credentials. Diagnostics go
//! to stderr; stdout is protocol bytes only.

use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use hh_mcp::artifact::ServedArtifact;
use hh_mcp::server::ServeMode;

fn usage() -> ExitCode {
    eprintln!("usage: hh-mcp-serve [--legacy] [--watch] --bundle <dir> | --container <file>");
    ExitCode::from(2)
}

/// Decode + lower the bundle's `target:mcp` member → `ServedArtifact`.
fn load_artifact(path: &Path, container: bool) -> Result<ServedArtifact, String> {
    let decoded = if container {
        std::fs::read(path)
            .map_err(|e| format!("read: {e}"))
            .and_then(|b| hh_bundle::codec::decode_container(&b).map_err(|e| format!("{e}")))
    } else {
        hh_bundle::codec::decode_dir(path).map_err(|e| format!("{e}"))
    }
    .map_err(|e| format!("bundle decode: {e}"))?;
    let member = decoded
        .manifest
        .members
        .iter()
        .find(|m| m.role == "target:mcp")
        .ok_or_else(|| "bundle carries no target:mcp member".to_string())?;
    let bytes = decoded
        .members
        .get(&member.address)
        .cloned()
        .ok_or_else(|| "target:mcp member bytes absent".to_string())?;
    hh_mcp::artifact::lower_mcp_target(
        &bytes,
        &decoded.manifest.version_id,
        &decoded.manifest.version_id,
    )
    .map_err(|e| format!("target lower: {}", e.refusal()))
}

fn main() -> ExitCode {
    let mut legacy = false;
    let mut watch = false;
    let mut path: Option<PathBuf> = None;
    let mut container = false;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--legacy" => legacy = true,
            "--watch" => watch = true,
            "--bundle" => {
                path = it.next().map(PathBuf::from);
                container = false;
            }
            "--container" => {
                path = it.next().map(PathBuf::from);
                container = true;
            }
            _ => return usage(),
        }
    }
    let Some(path) = path else {
        return usage();
    };
    let artifact = match load_artifact(&path, container) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("hh-mcp-serve: {e}");
            return ExitCode::from(3);
        }
    };
    let mode = if legacy {
        ServeMode::Legacy
    } else {
        ServeMode::Modern
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());
    // `--watch` re-decodes per iteration; without it the loader returns
    // the startup artifact (the fixed-artifact `serve` case — a reload
    // failure is "unchanged", never a crash).
    let mut last = artifact.clone();
    let mut load = move || {
        if !watch {
            return last.clone();
        }
        match load_artifact(&path, container) {
            Ok(a) => {
                last = a.clone();
                a
            }
            Err(e) => {
                eprintln!("hh-mcp-serve: reload kept last artifact: {e}");
                last.clone()
            }
        }
    };
    match hh_mcp::server::serve_dynamic(&mut load, mode, &mut reader, &mut writer) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hh-mcp-serve: {e}");
            ExitCode::from(7)
        }
    }
}
