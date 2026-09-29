//! `hh-authorize` — the §5g.1 `authorize` reference monitor driven
//! **out-of-process over canonical bytes** (AC-R-2.8.1-16; T-LCD-12's §2.1
//! operation seam). It reads one `hh-monitor/authorize-input/1` document —
//! every recorded input `authorize` consumes — on stdin (`-`) or from a file,
//! replays [`hh_monitor::monitor::Monitor::authorize`], and writes the
//! canonical `KernelDecision` JSON on stdout. A malformed input prints the
//! typed refusal on stderr and exits `2`.
//!
//! The binary is a *state reconstruction + replay* boundary: it builds the
//! monitor exclusively from the decoded records (handle table, Π, proposers,
//! capabilities, approval fold, sealed rule legs, live coordinates), never
//! from shared runtime state. `authorize` itself is the same code the
//! in-process caller runs — the AC's byte-equality clause is that both
//! halves emit identical canonical bytes over identical recorded inputs.
//!
//! Usage:
//!   hh-authorize <authorize-input.json|->   → canonical KernelDecision bytes

use std::io::Read;

use hh_monitor::monitor::Monitor;
use hh_monitor::wire::AuthorizeInput;

fn read(path: &str) -> Vec<u8> {
    if path == "-" {
        let mut b = Vec::new();
        std::io::stdin().read_to_end(&mut b).expect("stdin");
        b
    } else {
        std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"))
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).map(String::as_str).unwrap_or("help");
    if path == "help" || path == "--help" || path == "-h" {
        eprintln!("usage: hh-authorize <authorize-input.json|->");
        std::process::exit(if path == "help" { 2 } else { 0 });
    }
    let bytes = read(path);
    // The input must be canonical bytes — the codec refuses lenient JSON so a
    // re-encoded document can never alias a different recorded input (CC1).
    let j = hh_wire::canonical::parse_canonical(&bytes)
        .unwrap_or_else(|e| fail(&format!("input not canonical: {e}")));
    let input = AuthorizeInput::from_json(&j).unwrap_or_else(|e| fail(&e));
    let monitor: Monitor = input.monitor();
    // `authorize`'s typed refusals (CapabilityUnresolvable, SurfaceUnbound)
    // are the process boundary's refusal set — the closed tag on stderr +
    // exit 2, never a fabricated decision row.
    let decision = monitor.authorize(&input.proposal).unwrap_or_else(|e| {
        use hh_monitor::monitor::MonitorError::*;
        match e {
            CapabilityUnresolvable { capability } => {
                fail(&format!("capability_unresolvable: {capability}"))
            }
            SurfaceUnbound { capability } => fail(&format!("surface_unbound: {capability}")),
        }
    });
    let out = decision.to_json().to_canonical_string();
    println!("{out}");
}
