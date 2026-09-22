//! `hh-ir-op` — the §3.1.7 operations driven **out-of-process over canonical bytes**
//! (AC-IR-10). Every operation reads its inputs as canonical JSON on stdin / argv files and
//! writes canonical JSON on stdout; errors print the typed `HirError` set on stderr and the
//! process exits non-zero.
//!
//! Usage:
//!   hh-ir-op canonicalize <doc.json>            → canonical doc bytes
//!   hh-ir-op validate <doc.json>                → validation report JSON
//!   hh-ir-op identity <doc.json>                → doc bytes with ids computed
//!   hh-ir-op seal <doc.json> <sealed_at>        → sealed definition bytes
//!   hh-ir-op diff <base.json> <target.json> <provenance.json> <sealed_at>
//!                                               → HirDiff bytes (provenance is a canonical
//!                                                 ProvenanceRecord)
//!   hh-ir-op apply <base.json> <diff.json>      → document bytes
//!   hh-ir-op invert <diff.json>                 → diff bytes
//!   hh-ir-op migrate <doc.json> <from> <to>     → document bytes
//!   hh-ir-op project <doc.json> <P1..P7|kind>   → sub-DAG JSON

use std::io::Read;

use hh_hir::document::parse_document;
use hh_hir::errors::HirError;
use hh_wire::json::Json;

fn read(path: &str) -> Vec<u8> {
    if path == "-" {
        let mut b = Vec::new();
        std::io::stdin().read_to_end(&mut b).expect("stdin");
        b
    } else {
        std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"))
    }
}

fn fail(errs: Vec<HirError>) -> ! {
    for e in &errs {
        eprintln!("{e}");
    }
    std::process::exit(2)
}

fn out(bytes: Vec<u8>) {
    use std::io::Write;
    std::io::stdout().write_all(&bytes).unwrap();
    println!();
}

fn report_json(r: &hh_hir::errors::ValidationReport) -> Vec<u8> {
    Json::obj([
        ("node_count", Json::Int(r.node_count as i64)),
        ("edge_count", Json::Int(r.edge_count as i64)),
        (
            "checks_run",
            Json::Arr(r.checks_run.iter().map(|c| Json::str(*c)).collect()),
        ),
    ])
    .to_canonical_string()
    .into_bytes()
}

fn provenance_arg(bytes: &[u8]) -> hh_provenance::ProvenanceRecord {
    let j = hh_wire::canonical::parse_canonical(bytes)
        .unwrap_or_else(|e| panic!("provenance arg not canonical: {e}"));
    hh_hir::wire::provenance_from_json(&j).expect("provenance arg")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let op = args.get(1).map(String::as_str).unwrap_or("help");
    match op {
        "canonicalize" => match hh_hir::canonicalize_bytes(&read(&args[2])) {
            Ok(b) => out(b),
            Err(e) => fail(e),
        },
        "validate" => match hh_hir::validate_bytes(&read(&args[2])) {
            Ok(r) => out(report_json(&r)),
            Err(e) => fail(e),
        },
        "identity" => {
            match parse_document(&read(&args[2])).map(|mut d| {
                hh_hir::compute_ids(&mut d);
                d
            }) {
                Ok(d) => out(hh_hir::canonicalize(&d)),
                Err(e) => fail(vec![e]),
            }
        }
        "seal" => {
            let at: u64 = args[3].parse().expect("sealed_at (u64)");
            match hh_hir::seal_bytes(&read(&args[2]), at) {
                Ok(b) => out(b),
                Err(e) => fail(e),
            }
        }
        "diff" => {
            let prov = provenance_arg(&read(&args[4]));
            match hh_hir::diff_bytes(&read(&args[2]), &read(&args[3]), prov, Default::default()) {
                Ok(b) => out(b),
                Err(e) => fail(e),
            }
        }
        "apply" => match hh_hir::apply_bytes(&read(&args[2]), &read(&args[3])) {
            Ok(b) => out(b),
            Err(e) => fail(e),
        },
        "invert" => {
            let dj = hh_wire::canonical::parse_canonical(&read(&args[2]))
                .unwrap_or_else(|e| panic!("diff not canonical: {e}"));
            match hh_hir::wire::diff_from_json(&dj) {
                Ok(d) => out(hh_hir::wire::diff_to_json(&hh_hir::invert(&d))
                    .to_canonical_string()
                    .into_bytes()),
                Err(e) => fail(vec![e]),
            }
        }
        "migrate" => match hh_hir::migrate_bytes(&read(&args[2]), &args[3], &args[4]) {
            Ok(b) => out(b),
            Err(e) => fail(e),
        },
        "project" => match hh_hir::project_bytes(&read(&args[2]), &args[3]) {
            Ok(b) => out(b),
            Err(e) => fail(e),
        },
        _ => {
            eprintln!("usage: hh-ir-op <canonicalize|validate|identity|seal|diff|apply|invert|migrate|project> ...");
            std::process::exit(64);
        }
    }
}
