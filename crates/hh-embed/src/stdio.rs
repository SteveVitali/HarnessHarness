//! Binding (b) — the newline-delimited JSON-RPC 2.0 stdio loop
//! (ADR-0179 D1(b)): UTF-8, one message per line, `stderr` for logs
//! only. The loop calls the *same* `EmbedService::handle` binding (a)
//! uses, so the two bindings are byte-identical by construction
//! (AC-R-2.114-9). Framing only — this module defines no verb, field or
//! error variant beyond what the contract already names.

use crate::service::{EmbedService, ServiceConfig};
use hh_embed_schema::errors::EmbedError;
use hh_wire::json::Json;
use hh_wire::jsonrpc::{err_response, parse_request, write_message};
use std::io::{BufRead, BufReader, Write};

/// Serve `hh-embed/1` over stdin/stdout until clean EOF — the kernel
/// binary's `serve` entry.
pub fn run(config: ServiceConfig) -> std::io::Result<()> {
    let mut svc =
        EmbedService::open(config).map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut r = BufReader::new(stdin.lock());
    let mut w = stdout.lock();
    serve(&mut svc, &mut r, &mut w)
}

/// The framed request loop, generic over the streams — the in-process
/// tests drive it over memory buffers so binding (a) ⇄ binding (b)
/// parity is exercised without a subprocess.
pub fn serve<R: BufRead, W: Write>(
    svc: &mut EmbedService,
    r: &mut R,
    w: &mut W,
) -> std::io::Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        if r.read_line(&mut line)? == 0 {
            return Ok(()); // EOF — the host closed the channel.
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed.is_empty() {
            continue;
        }
        match parse_request(trimmed) {
            Ok(req) => {
                let resp = svc.handle(&req);
                write_message(w, &resp)?;
            }
            Err(e) => {
                // A framing failure lowers to the closed sum, never a
                // transport-shaped error (ADR-0178 D2).
                let err = EmbedError::SchemaViolation {
                    path: "/".to_string(),
                    code: format!("framing:{e}"),
                };
                write_message(
                    w,
                    &err_response(Json::Null, err.code(), err.kind(), err.to_data_json()),
                )?;
            }
        }
        for note in svc.drain_notifications() {
            write_message(w, &note)?;
        }
    }
}
