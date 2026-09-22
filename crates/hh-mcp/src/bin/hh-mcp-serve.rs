//! `hh-mcp-serve` — the Stage-3 fixture MCP server (R-2.11.3⁰;
//! ADR-0097 D7). `hh-mcp-serve --bundle <dir> | --container <hhb1>`
//! decodes the bundle, lowers its `target:mcp` member into the
//! `hh-mcp-artifact/1` record and serves the newline-JSON-RPC loop over
//! stdin/stdout under the fixed `stdio_launch` test principal.
//!
//! The process is a pure function of `(artifact, binding)` — it opens
//! no store, reads no environment, holds no credentials. Diagnostics go
//! to stderr; stdout is protocol bytes only.

use std::io::{BufReader, BufWriter};
use std::path::Path;
use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!("usage: hh-mcp-serve --bundle <dir> | --container <file>");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let decoded = match args.as_slice() {
        [flag, path] if flag == "--bundle" => hh_bundle::codec::decode_dir(Path::new(path)),
        [flag, path] if flag == "--container" => std::fs::read(path)
            .map_err(|e| hh_bundle::error::BundleError::Io {
                detail: format!("{e}"),
            })
            .and_then(|b| hh_bundle::codec::decode_container(&b)),
        _ => return usage(),
    };
    let decoded = match decoded {
        Ok(d) => d,
        Err(e) => {
            eprintln!("hh-mcp-serve: bundle decode: {e}");
            return ExitCode::from(3);
        }
    };
    let member = match decoded
        .manifest
        .members
        .iter()
        .find(|m| m.role == "target:mcp")
    {
        Some(m) => m.clone(),
        None => {
            eprintln!("hh-mcp-serve: bundle carries no target:mcp member");
            return ExitCode::from(4);
        }
    };
    let bytes = match decoded.members.get(&member.address) {
        Some(b) => b.clone(),
        None => {
            eprintln!("hh-mcp-serve: target:mcp member bytes absent");
            return ExitCode::from(5);
        }
    };
    let artifact = match hh_mcp::artifact::lower_mcp_target(
        &bytes,
        &decoded.manifest.version_id,
        &decoded.manifest.version_id,
    ) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("hh-mcp-serve: target lower: {}", e.refusal());
            return ExitCode::from(6);
        }
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());
    match hh_mcp::server::serve(&artifact, &mut reader, &mut writer) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hh-mcp-serve: {e}");
            ExitCode::from(7)
        }
    }
}
