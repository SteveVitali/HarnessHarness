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
//! fp-sweep correct <repo-root>
//! ```
//!
//! `correct` (R6.55; ADR-0453): mints the corrected V5 sealed document
//! through `hharness_lab::sweep::corrected_v5` — `version:<vid>`
//! selectors over the kernel's own Stage-1 seed, resolved and sealed by
//! the same path that refused W4's `sha256:<vid>` spelling
//! `RefUnresolved`. The original level record is read for the
//! supersession cite and left byte-untouched; the corrected record,
//! the dated supersession record and both sha256 sidecars land at the
//! manifest's `v5_corrected*` / `v5_supersession*` relpaths.

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
            Json::Arr(m.spellings.iter().map(Json::str).collect()),
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

/// `correct` — the ADR-0453 V5 reseal. The corrected record keeps the
/// `hh-fp-sweep-level/1` shape plus a `correction` member citing the
/// superseded artifact; the dated `hh-fp-sweep-supersession/1` record
/// carries the RefUnresolved history verbatim (history is never
/// rewritten — R6.52's failure row stands).
fn correct(repo: &Path, inputs: &BTreeMap<String, String>) {
    let original_rel = &inputs["v5_level"];
    let original = load(repo, original_rel);
    let original_bytes = format!("{}\n", original.to_canonical_string());
    let original_sha = sha256_hex(original_bytes.as_bytes());
    let original_ref = original
        .get("level_ref")
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("v5_level carries no level_ref"))
        .to_string();

    let work = std::env::temp_dir().join(format!("fp-sweep-correct-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let c = hharness_lab::sweep::corrected_v5(&work)
        .unwrap_or_else(|e| panic!("corrected V5 mint refused: {e}"));
    assert_ne!(
        c.version_id, original_ref,
        "the corrected seal must mint a NEW version id (a re-seal is a new artifact)"
    );

    let corrected_rec = Json::obj([
        ("schema", Json::str("hh-fp-sweep-level/1")),
        ("level_id", Json::str("V5")),
        ("level_name", Json::str("rsn-drop")),
        (
            "factor_id",
            Json::str("transcript_render.assistant_replay.reasoning"),
        ),
        ("level_ref", Json::str(&c.version_id)),
        (
            "baseline_ref",
            Json::str(
                original
                    .get("baseline_ref")
                    .and_then(Json::as_str)
                    .unwrap_or(""),
            ),
        ),
        (
            "override_spellings",
            Json::Arr(vec![Json::str("reasoning_replay=drop")]),
        ),
        ("sealed", c.sealed_json.clone()),
        ("label", Json::str(hharness_lab::sweep::SWEEP_VARIANT_LABEL)),
        ("parity", Json::str("none")),
        (
            "correction",
            Json::obj([
                ("supersedes", Json::str(&original_ref)),
                ("original_sha256", Json::str(format!("sha256:{original_sha}"))),
                (
                    "defect",
                    Json::str(
                        "W4 bound doc re-spelled version_id pins as version_selector:\"sha256:<vid>\" — a selector spelling the grammar never admits (resolve.rs parse_selector); the slot resolve refused RefUnresolved and the arm recorded verifier_unavailable (R6.52)",
                    ),
                ),
                (
                    "substitution",
                    Json::str(
                        "authored selectors re-spelled version:<vid> against the kernel Stage-1 seed; model_port bound hh/gateway_http with the recorded W4 env: param set; environment cell:task",
                    ),
                ),
                ("adr", Json::str("ADR-0453")),
                ("corrected_at", Json::str("2026-11-20T00:00:00Z")),
            ]),
        ),
    ]);

    let supersession = Json::obj([
        ("schema", Json::str("hh-fp-sweep-supersession/1")),
        ("artifact", Json::str("v5-level")),
        ("window", Json::str("W4x")),
        ("adr", Json::str("ADR-0453")),
        ("recorded_at", Json::str("2026-11-20T00:00:00Z")),
        (
            "original",
            Json::obj([
                ("level_ref", Json::str(&original_ref)),
                ("record_path", Json::str(original_rel.as_str())),
                ("record_sha256", Json::str(format!("sha256:{original_sha}"))),
                ("byte_untouched", Json::Bool(true)),
            ]),
        ),
        (
            "history",
            Json::str(
                "R6.52/W4: the slot resolve refused RefUnresolved — unknown selector `sha256:<vid>` (latest | label:<l> | version:<vid> | range:<lo>-<hi> | deny:<ids>) at /assembly/slots/{context_policy,control_strategy,model_port}; the arm recorded verifier_unavailable. The R6.52 record stands verbatim.",
            ),
        ),
        (
            "corrected",
            Json::obj([
                ("level_ref", Json::str(&c.version_id)),
                (
                    "record_path",
                    Json::str(inputs["v5_corrected"].as_str()),
                ),
                (
                    "bound_vids",
                    Json::Obj(
                        c.bound_vids
                            .iter()
                            .map(|(k, v)| (k.clone(), Json::str(v)))
                            .collect(),
                    ),
                ),
                ("registry_snapshot_id", Json::str(&c.snapshot_id)),
            ]),
        ),
    ]);

    for (key, doc) in [
        ("v5_corrected", &corrected_rec),
        ("v5_supersession", &supersession),
    ] {
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
    if args.len() != 3 || (args[1] != "emit" && args[1] != "correct") {
        eprintln!("usage: fp-sweep <emit|correct> <repo-root>");
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
    match args[1].as_str() {
        "emit" => emit(&repo, &inputs),
        "correct" => correct(&repo, &inputs),
        _ => unreachable!(),
    }
}
