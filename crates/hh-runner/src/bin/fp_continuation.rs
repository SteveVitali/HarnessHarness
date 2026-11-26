//! `fp-continuation` — the W4 driver's stdin/stdout transport onto the
//! one continuation module (R6.52 repair; R6.29 X-04/CC1). Reads one
//! `hh-fp-continuation-adapter/1` request on stdin, writes the
//! canonical response on stdout. Exit 0 on success; exit 2 with a
//! `{"refused": …}` body on any typed refusal — the caller stops
//! admitting. Deterministic; no network, no clock, no file writes.
//!
//! ```text
//! fp-continuation < request.json > response.json
//! ```

use std::io::{Read, Write};

fn main() {
    let mut text = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut text) {
        eprintln!("fp-continuation: read stdin: {e}");
        std::process::exit(2);
    }
    let (body, ok) = hharness_runner::continuation_adapter::respond(&text);
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", body.to_canonical_string());
    let _ = out.flush();
    std::process::exit(if ok { 0 } else { 2 });
}
