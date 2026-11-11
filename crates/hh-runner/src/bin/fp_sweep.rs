//! `fp-sweep` — the R6.43 emitter (manifest 304). Reads the committed
//! `hh-fp-prereg/1` packet named by the inputs manifest, materializes
//! the one sealed level — V5 `rsn-drop` through hh-lab's
//! enumerate → resolve → seal chain (REQ-XE-124) — embeds R6.33's
//! `v5_declaration` byte-equal (sha256-checked, never edited to fit),
//! freezes `sweep-registry/1` with its hash (REQ-XD-201/202), and
//! writes the registry record, the sealed V5 document and both
//! sha256 sidecars at the manifest's output relpaths.
//! `HH_FP_SWEEP_INPUTS` names the manifest (`scripts/fp-sweep-inputs.txt`
//! by default). Deterministic — same inputs, byte-identical outputs.
//! No network, no clock (recorded_at is the declared constant).
//!
//! ```text
//! fp-sweep emit <repo-root>
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_wire::json::{parse, Json};
use hharness_wire::sha256::sha256_hex;

fn load(repo: &Path, rel: &str) -> Json {
    parse(&std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")))
        .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

fn write(repo: &Path, rel: &str, body: &str) {
    let p = repo.join(rel);
    std::fs::create_dir_all(p.parent().unwrap_or_else(|| Path::new(".")))
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", p.display()));
    std::fs::write(&p, body).unwrap_or_else(|e| panic!("write {}: {e}", p.display()));
    println!("fp-sweep: wrote {rel}");
}

fn sidecar(hash: &str, rel: &str, sidecar: &str) -> String {
    format!(
        "{hash}  {rel}\n# recompute: shasum -a 256 -c {sidecar} (run from the repo root — the record is canonical Json::to_canonical_string output, CC1)\n"
    )
}

fn emit(repo: &Path, inputs: &BTreeMap<String, String>) {
    let packet = load(repo, &inputs["packet"]);
    let decl = packet
        .get("v5_declaration")
        .cloned()
        .unwrap_or_else(|| panic!("packet carries no v5_declaration"));
    let decl_sha = sha256_hex(decl.to_canonical_string().as_bytes());
    assert_eq!(
        decl_sha,
        hharness_lab::sweep::V5_DECLARATION_SHA256,
        "v5_declaration does not reproduce R6.33's declaration byte-equal — stop (never edit to fit)"
    );

    // Materialize the one sealed level over the declared baseline.
    let work = std::env::temp_dir().join(format!("fp-sweep-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let m = hharness_lab::sweep::materialize_v5(&work)
        .unwrap_or_else(|e| panic!("V5 materialization refused: {e}"));

    let v5 = hharness_lab::sweep::v5_level_entry(&m.level_version_id, &decl, &decl_sha);
    let reg = hharness_lab::sweep::freeze_registry(&hharness_lab::sweep::sweep_registry_record(v5));
    let refusals = hharness_lab::sweep::check_registry(&reg);
    assert!(refusals.is_empty(), "registry refused: {refusals:?}");

    // The level doc record — sealed_ref + sealed document + spellings
    // (attribution) + the baseline ref it derives from.
    let v5_rec = Json::obj([
        ("schema", Json::str("hh-fp-sweep-level/1")),
        ("level_id", Json::str("V5")),
        ("level_name", Json::str("rsn-drop")),
        (
            "factor_id",
            Json::str("transcript_render.assistant_replay.reasoning"),
        ),
        ("level_ref", Json::str(&m.level_version_id)),
        ("baseline_ref", Json::str(&m.baseline_version_id)),
        (
            "override_spellings",
            Json::Arr(m.spellings.iter().map(|s| Json::str(s)).collect()),
        ),
        ("sealed", m.sealed_level_json),
        ("label", Json::str(hharness_lab::sweep::SWEEP_VARIANT_LABEL)),
        ("parity", Json::str("none")),
    ]);

    for (key, doc) in [("sweep_registry", &reg), ("v5_level", &v5_rec)] {
        let rel = &inputs[key];
        let bytes = format!("{}\n", doc.to_canonical_string());
        write(repo, rel, &bytes);
        let sha_key = format!("{key}_sha256");
        let sha_rel = &inputs[&sha_key];
        write(
            repo,
            sha_rel,
            &sidecar(&sha256_hex(bytes.as_bytes()), rel, sha_rel),
        );
    }
    let _ = std::fs::remove_dir_all(&work);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 || args[1] != "emit" {
        eprintln!("usage: fp-sweep emit <repo-root>");
        std::process::exit(2);
    }
    let repo = PathBuf::from(&args[2]);
    let manifest = std::env::var("HH_FP_SWEEP_INPUTS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo.join("scripts/fp-sweep-inputs.txt"));
    let mut inputs = BTreeMap::new();
    for line in std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest.display()))
        .lines()
    {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("manifest line `{line}` is not name=relpath"));
        inputs.insert(k.to_string(), v.to_string());
    }
    emit(&repo, &inputs);
}
