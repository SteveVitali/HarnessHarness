//! `fp-partition` — the R6.42 Stage-1 emitter (manifest 303). Reads
//! the committed `hh-fp-prereg/1` packet + the select commit and
//! exclusion record named by the inputs manifest, draws the seeded
//! dev/gate-A/gate-B partition of the `fp_dev7` successor list, and
//! writes the `hh-fp-partition/1` record, the sealed
//! `hh-fp-dev-projection/1` (R6.30's `dev_projection` — the only
//! corpus input H development reads) and both sha256 sidecars at the
//! manifest's output relpaths. `HH_FP_PARTITION_INPUTS` names the
//! manifest (newline `name=relpath` pairs —
//! `scripts/fp-partition-inputs.txt`, the default under the repo
//! root). Deterministic — same inputs, byte-identical outputs. No
//! network, no clock (RECORDED_AT is the declared constant).
//!
//! ```text
//! fp-partition emit <repo-root>
//! ```

use std::path::{Path, PathBuf};

use hharness_runner::fp_partition::*;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

fn load(repo: &Path, rel: &str) -> Json {
    hharness_wire::json::parse(
        &std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")),
    )
    .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

fn sha(repo: &Path, rel: &str) -> String {
    sha256_hex(&std::fs::read(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")))
}

fn write(repo: &Path, rel: &str, body: &str) {
    let p = repo.join(rel);
    std::fs::create_dir_all(p.parent().unwrap_or_else(|| Path::new(".")))
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", p.display()));
    std::fs::write(&p, body).unwrap_or_else(|e| panic!("write {}: {e}", p.display()));
    println!("fp-partition: wrote {rel}");
}

fn sidecar(hash: &str, rel: &str, sidecar: &str) -> String {
    format!(
        "{hash}  {rel}\n# recompute: shasum -a 256 -c {sidecar} (run from the repo root — the record is canonical Json::to_canonical_string output, CC1)\n"
    )
}

fn emit(repo: &Path, inputs: &PartitionInputs) {
    let packet = load(repo, &inputs.packet);
    let commit = load(repo, &inputs.select_commit);
    let select_id = commit
        .get("select_id")
        .and_then(Json::as_str)
        .unwrap_or_default();

    let record = build_partition(
        &packet,
        inputs,
        &sha(repo, &inputs.packet),
        &sha(repo, &inputs.select_commit),
        select_id,
        &sha(repo, &inputs.exclusion),
    )
    .unwrap_or_else(|e| panic!("partition: {e}"));
    check_partition(&record, &packet).unwrap_or_else(|e| panic!("partition check: {e}"));
    check_dev_not_held(&record, &packet, &commit).unwrap_or_else(|e| panic!("held-out check: {e}"));

    let canonical = record.to_canonical_string() + "\n";
    let record_hash = sha256_hex(canonical.as_bytes());
    write(repo, &inputs.partition, &canonical);
    write(
        repo,
        &inputs.partition_sha256,
        &sidecar(&record_hash, &inputs.partition, &inputs.partition_sha256),
    );

    let proj = dev_projection_record(&record, &inputs.partition, &record_hash)
        .unwrap_or_else(|e| panic!("dev projection: {e}"));
    let proj_canonical = proj.to_canonical_string() + "\n";
    let proj_hash = sha256_hex(proj_canonical.as_bytes());
    write(repo, &inputs.dev_projection, &proj_canonical);
    write(
        repo,
        &inputs.dev_projection_sha256,
        &sidecar(
            &proj_hash,
            &inputs.dev_projection,
            &inputs.dev_projection_sha256,
        ),
    );

    println!("fp-partition: partition sha256 {record_hash}");
    println!("fp-partition: dev projection sha256 {proj_hash}");
}

/// The inputs manifest — `HH_FP_PARTITION_INPUTS` names it; otherwise
/// the committed `<repo>/scripts/fp-partition-inputs.txt`.
fn load_inputs(repo: &Path) -> PartitionInputs {
    let manifest = std::env::var_os("HH_FP_PARTITION_INPUTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("scripts").join("fp-partition-inputs.txt"));
    let body = std::fs::read_to_string(&manifest).unwrap_or_else(|e| {
        panic!(
            "read {}: {e} (HH_FP_PARTITION_INPUTS names the name=relpath inputs manifest)",
            manifest.display()
        )
    });
    PartitionInputs::from_manifest(&body).unwrap_or_else(|e| panic!("{}: {e}", manifest.display()))
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("emit") if args.len() == 3 => {
            let repo = PathBuf::from(&args[2]);
            let inputs = load_inputs(&repo);
            emit(&repo, &inputs);
            std::process::ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: fp-partition emit <repo-root>");
            std::process::ExitCode::from(2)
        }
    }
}
