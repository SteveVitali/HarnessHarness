//! `hh-xcheck` — the cross-implementation replay/checker CLI (spec §10.7;
//! ticket R2.21; ADR-0353).
//!
//! ```text
//! hh-xcheck export  <bundle-dir>                 write the replay bundle
//! hh-xcheck selfcheck <bundle-dir>               E1 replay over the bundle
//! hh-xcheck verify  <bundle-dir> <answers.json>  diff foreign answers vs expected
//! ```
//!
//! `verify` only ever *checks* — it mints no claim. A green `verify` line is
//! evidence a foreign run may cite; it is never generated here.
//!
//! Exit: 0 green · 1 a case failed · 2 usage/environment error.

use std::path::Path;
use std::process::ExitCode;

use hh_wire::json::parse;
use hh_xcheck::check::{self_check, verify_answers, CheckReport};
use hh_xcheck::export::export_bundle;

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  hh-xcheck export    <bundle-dir>\n  hh-xcheck selfcheck <bundle-dir>\n  hh-xcheck verify    <bundle-dir> <answers.json>"
    );
    ExitCode::from(2)
}

fn report(tag: &str, r: &CheckReport) -> ExitCode {
    for c in &r.results {
        match &c.outcome {
            Ok(()) => println!("ok    {tag} {} {}", c.arm, c.case),
            Err(d) => println!("FAIL  {tag} {} {} — {d}", c.arm, c.case),
        }
    }
    let (pass, fail) = r.counts();
    println!("{tag}: {pass} passed, {fail} failed");
    if r.is_green() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("export") if args.len() == 3 => match export_bundle(Path::new(&args[2])) {
            Ok(()) => {
                println!("exported hh-xcheck-bundle/1 → {}", args[2]);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("export: {e}");
                ExitCode::from(2)
            }
        },
        Some("selfcheck") if args.len() == 3 => match self_check(Path::new(&args[2])) {
            Ok(r) => report("selfcheck", &r),
            Err(e) => {
                eprintln!("selfcheck: {e}");
                ExitCode::from(2)
            }
        },
        Some("verify") if args.len() == 4 => {
            let bytes = match std::fs::read(&args[3]) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("verify: read {}: {e}", args[3]);
                    return ExitCode::from(2);
                }
            };
            let answers = match parse(&String::from_utf8_lossy(&bytes)) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("verify: answers parse: {e}");
                    return ExitCode::from(2);
                }
            };
            let candidate = answers
                .get("candidate")
                .and_then(hh_wire::json::Json::as_str)
                .unwrap_or("unknown");
            match verify_answers(Path::new(&args[2]), &answers) {
                Ok(r) => {
                    println!("candidate: {candidate}");
                    report("verify", &r)
                }
                Err(e) => {
                    eprintln!("verify: {e}");
                    ExitCode::from(2)
                }
            }
        }
        _ => usage(),
    }
}
