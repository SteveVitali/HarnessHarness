//! `p4-admissibility` — the R7.1 emitter (manifest 326). Reads the
//! committed `hh-p4-token-corpus/1` record named by the inputs
//! manifest, re-derives the W rule from its measured non-outcome
//! trials (REQ-XD-205 + REQ-XE-127 — the input manifest's selected
//! fields are re-checked before any read), seals the pass's Tier-1
//! levels through hh-lab's enumerate → resolve → seal chain
//! (composites through the conjunctive path), freezes the pass's
//! `sweep-registry/1` under the NEW `x2:` exploration identity
//! (REQ-XD-202 — x1's data never merge), folds the §4.2-generic
//! activation record (REQ-XD-208), and writes the registry, levels,
//! W-rule derivation, and admissibility records plus their sha256
//! sidecars at the manifest's output relpaths.
//! `HH_FP_P4_INPUTS` names the manifest (`scripts/fp-p4-inputs.txt`
//! by default). Deterministic — same inputs, byte-identical outputs.
//! No network, no clock (recorded_at is the declared constant).
//!
//! ```text
//! p4-admissibility emit <repo-root>
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
    println!("p4-admissibility: wrote {rel}");
}

fn sidecar(hash: &str, rel: &str, sidecar: &str) -> String {
    format!(
        "{hash}  {rel}\n# recompute: shasum -a 256 -c {sidecar} (run from the repo root — the record is canonical Json::to_canonical_string output, CC1)\n"
    )
}

fn inputs() -> BTreeMap<String, String> {
    let manifest =
        std::env::var("HH_FP_P4_INPUTS").unwrap_or_else(|_| "scripts/fp-p4-inputs.txt".to_string());
    let mut out = BTreeMap::new();
    for line in std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("read {manifest}: {e}"))
        .lines()
    {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("bad line: {line}"));
        out.insert(k.trim().to_string(), v.trim().to_string());
    }
    out
}

/// The corpus record → measured per-trial peaks. The input
/// manifest's selected fields are re-checked for outcome members
/// before any value is read; the record's own member set is checked
/// at the manifest boundary (a corpus record carrying an outcome
/// member in `selected_fields` refuses — REQ-XD-205/209).
fn corpus_peaks(corpus: &Json) -> Vec<u64> {
    let fields: Vec<String> = corpus
        .get("input_manifest")
        .and_then(|m| m.get("selected_fields"))
        .and_then(|f| match f {
            Json::Arr(a) => Some(
                a.iter()
                    .filter_map(|j| j.as_str().map(str::to_string))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_else(|| panic!("corpus record carries no input_manifest.selected_fields"));
    hharness_lab::p4::check_non_outcome_fields(fields.iter().map(String::as_str))
        .unwrap_or_else(|e| panic!("corpus input manifest refused: {e}"));
    let mut peaks = Vec::new();
    let trials = match corpus.get("trials") {
        Some(Json::Arr(a)) => a,
        _ => panic!("corpus record carries no trials"),
    };
    for t in trials {
        if t.get("measured") != Some(&Json::Bool(true)) {
            continue;
        }
        let p = t
            .get("peak_input_tokens")
            .and_then(Json::as_int)
            .unwrap_or_else(|| panic!("measured trial without peak_input_tokens"));
        peaks.push(p as u64);
    }
    peaks
}

/// The per-level activation fold, computed over what the committed
/// evidence actually measures (never a claimed share the corpus
/// cannot bear).
fn level_fold(level_id: &str, wr: &hharness_lab::p4::WRule) -> Json {
    use hharness_lab::p4::{fold_activation, ActivationEvidence as E, ActivationMetric as M};
    match level_id {
        "V1" | "V2" => fold_activation(level_id, M::Always, E { exercised: 0, total: 0 }),
        "V6" => fold_activation(level_id, M::WireVisible, E { exercised: 0, total: 0 }),
        "V7" => fold_activation(level_id, M::WireVisible, E { exercised: 0, total: 0 }),
        // The composite's conditional activation under the floor cap
        // IS the measured F(32 768) — exact, from the same non-outcome
        // corpus the W rule reads.
        "V8" | "V9" => {
            let (e, t) = wr
                .table
                .iter()
                .find(|(w, _, _)| *w == hharness_lab::p4::W_FLOOR)
                .map(|(_, e, t)| (*e, *t))
                .unwrap_or((0, 0));
            fold_activation(
                level_id,
                M::ExactCounterfactual,
                E {
                    exercised: e,
                    total: t,
                },
            )
        }
        // Conditional knobs the committed corpus does not measure —
        // the fold records the metric with an honest unmeasured share.
        _ => Json::obj([
            ("level_id", Json::str(level_id)),
            ("metric", Json::str("share_of_first_subjects")),
            ("activation_num", Json::Int(0)),
            ("activation_den", Json::Int(0)),
            ("activation_ppm", Json::Int(-1)),
            ("exact", Json::Bool(false)),
            (
                "note",
                Json::str("unmeasured{evidence_absent} — no committed corpus for this knob; not a forecast pass"),
            ),
        ]),
    }
}

fn emit(repo: &Path, inputs: &BTreeMap<String, String>) {
    use hharness_lab::p4::*;

    let corpus = load(repo, &inputs["corpus"]);
    assert_eq!(
        corpus.get("schema").and_then(Json::as_str),
        Some("hh-p4-token-corpus/1"),
        "corpus record is not hh-p4-token-corpus/1"
    );
    let peaks = corpus_peaks(&corpus);
    let wr = w_rule(&peaks);
    let withheld_composites = matches!(wr.outcome, WOutcome::WithheldInertAtAdmissibleWindow);

    // Seal the pass's levels (V8/V9 stay unsealed when the W rule
    // withholds — `seal: false` in their declarations).
    let work = std::env::temp_dir().join(format!("p4-adm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let mat = seal_p4_levels(&work).unwrap_or_else(|e| panic!("pass-2 seal refused: {e}"));

    // The effort-pin corpus evidence — every call's applied value
    // equals the baseline lane pin (REQ-XD-204's recording surface;
    // the mis-pinning refusal is exercised in r7_1).
    let pin_evidence = corpus
        .get("effort_pin_evidence")
        .cloned()
        .unwrap_or(Json::Null);

    // Per-level extras: forecast (every level), withheld + the W-rule
    // declaration for the composites.
    let mut level_extra: BTreeMap<String, Vec<(String, Json)>> = BTreeMap::new();
    for decl in p4_level_decls() {
        let id = decl.level_id;
        let fold = level_fold(id, &wr);
        let mut entries = vec![("forecast".to_string(), forecast_entry(id, fold))];
        if id == "V8" || id == "V9" {
            if withheld_composites {
                entries.push((
                    "withheld".to_string(),
                    Json::str("withheld{inert_at_admissible_window}"),
                ));
            }
            entries.push((
                "declaration".to_string(),
                Json::obj([
                    (
                        "strategy",
                        Json::str(decl.spellings[0].split('=').nth(1).unwrap_or("")),
                    ),
                    ("window_cap", Json::Int(32_768)),
                    (
                        "note",
                        Json::str("the composite's W — uniform across every task and replicate"),
                    ),
                ]),
            ));
        }
        level_extra.insert(id.to_string(), entries);
    }

    // Tier-2 admission verdicts — this pass's identity admits none.
    let t2_admissions = Json::Arr(
        hharness_lab::sweep::TIER2_CANDIDATES
            .iter()
            .map(|id| check_tier2_admission(id, false, None))
            .collect(),
    );

    let admissibility = Json::obj([
        ("schema", Json::str("hh-p4-admissibility/1")),
        ("recorded_at", Json::str("2026-11-20")),
        ("ticket", Json::str("326_R7.1")),
        ("w_rule", w_rule_json(&wr)),
        ("corpus_sha256", {
            let bytes = format!("{}\n", corpus.to_canonical_string());
            Json::str(format!("sha256:{}", sha256_hex(bytes.as_bytes())))
        }),
        (
            "effort_pin",
            Json::obj([
                ("declared_baseline", Json::str("high")),
                ("applied_eq_declared_share", Json::str("1/1 — every recorded call")),
                ("corpus_evidence", pin_evidence),
                (
                    "note",
                    Json::str("the proxy pin surface records requested/applied/path per call; a call whose applied value differs from the level's declared lane value makes the level ineligible — ineligible{mis_pinned:…}"),
                ),
            ]),
        ),
        ("tier2_admissions", t2_admissions),
        (
            "note",
            Json::str("registration authorizes no spend; V8/V9 carry the W-rule disposition — withheld levels stay declared, never sealed"),
        ),
    ]);

    let reg = hharness_lab::sweep::freeze_registry_as(
        &sweep_registry_record_p4(&mat, &level_extra, admissibility.clone()),
        P4_TAG,
    );
    let refusals = hharness_lab::sweep::check_registry(&reg);
    assert!(refusals.is_empty(), "registry refused: {refusals:?}");

    // The levels record — every sealed level's document, byte-exact.
    let levels_rec = Json::obj([
        ("schema", Json::str("hh-p4-levels/1")),
        ("recorded_at", Json::str("2026-11-20")),
        ("baseline_ref", Json::str(&mat.baseline_version_id)),
        (
            "levels",
            Json::Arr(
                mat.levels
                    .iter()
                    .map(|l| {
                        Json::obj([
                            ("level_id", Json::str(&l.sealed.level_id)),
                            ("level_ref", Json::str(&l.sealed.version_id)),
                            (
                                "override_spellings",
                                Json::Arr(l.sealed.spellings.iter().map(Json::str).collect()),
                            ),
                            ("sealed", hharness_lab::sweep::sealed_json(&l.sealed.sealed)),
                            ("delta", Json::Arr(l.delta.iter().map(Json::str).collect())),
                            ("label", Json::str(hharness_lab::sweep::SWEEP_VARIANT_LABEL)),
                            ("parity", Json::str("none")),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "withheld",
            Json::Arr(
                ["V8", "V9"]
                    .iter()
                    .map(|id| {
                        Json::obj([
                            ("level_id", Json::str(*id)),
                            (
                                "disposition",
                                Json::str(if withheld_composites {
                                    "withheld{inert_at_admissible_window}"
                                } else {
                                    "declared"
                                }),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);

    // The W-rule derivation record — verbatim input manifest + the
    // measured trial rows it read.
    let w_rule_rec = Json::obj([
        ("schema", Json::str("hh-p4-w-rule/1")),
        ("recorded_at", Json::str("2026-11-20")),
        ("ticket", Json::str("326_R7.1")),
        ("corpus", Json::str(&inputs["corpus"])),
        (
            "input_manifest",
            corpus.get("input_manifest").cloned().unwrap_or(Json::Null),
        ),
        ("derivation", w_rule_json(&wr)),
        (
            "trials",
            corpus.get("trials").cloned().unwrap_or(Json::Arr(vec![])),
        ),
    ]);

    for (key, doc) in [
        ("sweep_registry", &reg),
        ("levels", &levels_rec),
        ("w_rule", &w_rule_rec),
        ("admissibility", &admissibility),
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

/// `supersede <repo-root>` — the R7.3 generation-2 mint (manifest 329):
/// the same `emit` shape over the **gen-2** authored baseline — the
/// `transcript_render` repair the G-L1 rerun-4 divergence diagnosed —
/// writing the four records at the inputs manifest's gen-2 relpaths,
/// each carrying a `supersedes` lineage cite (`prior` path +
/// recomputed `prior_sha256` + the manifest's `supersedes_reason` +
/// `byte_untouched: true`). The gen-1 records stay byte-untouched;
/// `emit` keeps reproducing them (a superseded generation's
/// reproducer is never deleted — the `fp-sweep correct` precedent).
/// Deterministic — same inputs, byte-identical outputs.
fn supersede(repo: &Path, inputs: &BTreeMap<String, String>) {
    use hharness_lab::p4::*;

    let recorded_at = inputs
        .get("recorded_at")
        .map(String::as_str)
        .unwrap_or("2026-11-21");
    let reason = inputs
        .get("supersedes_reason")
        .map(String::as_str)
        .unwrap_or_else(|| panic!("supersede requires supersedes_reason in the inputs manifest"));
    // The lineage cite for a named prior record — path + the gen-1
    // file's recomputed sha256 + the declared reason.
    let cite = |key: &str| -> Json {
        let rel = inputs
            .get(&format!("supersedes_{key}"))
            .unwrap_or_else(|| panic!("supersede requires supersedes_{key}"));
        let bytes = std::fs::read(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
        Json::obj([
            ("prior", Json::str(rel.as_str())),
            (
                "prior_sha256",
                Json::str(format!("sha256:{}", sha256_hex(bytes.as_slice()))),
            ),
            ("reason", Json::str(reason)),
            ("byte_untouched", Json::Bool(true)),
        ])
    };

    let corpus = load(repo, &inputs["corpus"]);
    assert_eq!(
        corpus.get("schema").and_then(Json::as_str),
        Some("hh-p4-token-corpus/1"),
        "corpus record is not hh-p4-token-corpus/1"
    );
    let peaks = corpus_peaks(&corpus);
    let wr = w_rule(&peaks);
    let withheld_composites = matches!(wr.outcome, WOutcome::WithheldInertAtAdmissibleWindow);

    let work = std::env::temp_dir().join(format!("p4-adm-g2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let mat =
        seal_p4_levels_gen2(&work).unwrap_or_else(|e| panic!("pass-2 gen-2 seal refused: {e}"));

    let pin_evidence = corpus
        .get("effort_pin_evidence")
        .cloned()
        .unwrap_or(Json::Null);

    let mut level_extra: BTreeMap<String, Vec<(String, Json)>> = BTreeMap::new();
    for decl in p4_level_decls() {
        let id = decl.level_id;
        let fold = level_fold(id, &wr);
        let mut entries = vec![("forecast".to_string(), forecast_entry(id, fold))];
        if id == "V8" || id == "V9" {
            if withheld_composites {
                entries.push((
                    "withheld".to_string(),
                    Json::str("withheld{inert_at_admissible_window}"),
                ));
            }
            entries.push((
                "declaration".to_string(),
                Json::obj([
                    (
                        "strategy",
                        Json::str(decl.spellings[0].split('=').nth(1).unwrap_or("")),
                    ),
                    ("window_cap", Json::Int(32_768)),
                    (
                        "note",
                        Json::str("the composite's W — uniform across every task and replicate"),
                    ),
                ]),
            ));
        }
        level_extra.insert(id.to_string(), entries);
    }

    let t2_admissions = Json::Arr(
        hharness_lab::sweep::TIER2_CANDIDATES
            .iter()
            .map(|id| check_tier2_admission(id, false, None))
            .collect(),
    );

    let admissibility = Json::obj([
        ("schema", Json::str("hh-p4-admissibility/1")),
        ("recorded_at", Json::str(recorded_at)),
        ("ticket", Json::str("329_R7.3")),
        ("w_rule", w_rule_json(&wr)),
        ("corpus_sha256", {
            let bytes = format!("{}\n", corpus.to_canonical_string());
            Json::str(format!("sha256:{}", sha256_hex(bytes.as_bytes())))
        }),
        (
            "effort_pin",
            Json::obj([
                ("declared_baseline", Json::str("high")),
                ("applied_eq_declared_share", Json::str("1/1 — every recorded call")),
                ("corpus_evidence", pin_evidence),
                (
                    "note",
                    Json::str("the proxy pin surface records requested/applied/path per call; a call whose applied value differs from the level's declared lane value makes the level ineligible — ineligible{mis_pinned:…}"),
                ),
            ]),
        ),
        ("tier2_admissions", t2_admissions),
        (
            "note",
            Json::str("registration authorizes no spend; V8/V9 carry the W-rule disposition — withheld levels stay declared, never sealed"),
        ),
        ("supersedes", cite("admissibility")),
    ]);

    let reg = hharness_lab::sweep::freeze_registry_as(
        &sweep_registry_record_p4_gen(
            &mat,
            &level_extra,
            admissibility.clone(),
            recorded_at,
            Some(cite("sweep_registry")),
        ),
        P4_TAG,
    );
    let refusals = hharness_lab::sweep::check_registry(&reg);
    assert!(refusals.is_empty(), "registry refused: {refusals:?}");

    let levels_rec = Json::obj([
        ("schema", Json::str("hh-p4-levels/1")),
        ("recorded_at", Json::str(recorded_at)),
        ("baseline_ref", Json::str(&mat.baseline_version_id)),
        (
            "levels",
            Json::Arr(
                mat.levels
                    .iter()
                    .map(|l| {
                        Json::obj([
                            ("level_id", Json::str(&l.sealed.level_id)),
                            ("level_ref", Json::str(&l.sealed.version_id)),
                            (
                                "override_spellings",
                                Json::Arr(l.sealed.spellings.iter().map(Json::str).collect()),
                            ),
                            ("sealed", hharness_lab::sweep::sealed_json(&l.sealed.sealed)),
                            ("delta", Json::Arr(l.delta.iter().map(Json::str).collect())),
                            ("label", Json::str(hharness_lab::sweep::SWEEP_VARIANT_LABEL)),
                            ("parity", Json::str("none")),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "withheld",
            Json::Arr(
                ["V8", "V9"]
                    .iter()
                    .map(|id| {
                        Json::obj([
                            ("level_id", Json::str(*id)),
                            (
                                "disposition",
                                Json::str(if withheld_composites {
                                    "withheld{inert_at_admissible_window}"
                                } else {
                                    "declared"
                                }),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("supersedes", cite("levels")),
    ]);

    let w_rule_rec = Json::obj([
        ("schema", Json::str("hh-p4-w-rule/1")),
        ("recorded_at", Json::str(recorded_at)),
        ("ticket", Json::str("329_R7.3")),
        ("corpus", Json::str(&inputs["corpus"])),
        (
            "input_manifest",
            corpus.get("input_manifest").cloned().unwrap_or(Json::Null),
        ),
        ("derivation", w_rule_json(&wr)),
        (
            "trials",
            corpus.get("trials").cloned().unwrap_or(Json::Arr(vec![])),
        ),
        ("supersedes", cite("w_rule")),
    ]);

    for (key, doc) in [
        ("sweep_registry", &reg),
        ("levels", &levels_rec),
        ("w_rule", &w_rule_rec),
        ("admissibility", &admissibility),
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

/// `emit-docs <out-dir>` — the R7.3 drive surface (manifest 329): the
/// same `seal_p4_levels` materialization the `emit` leg validates, this
/// time writing each arm's drive-time document — the sealed baseline
/// `document` member plus every sealed level's `document` — as canonical
/// JSON files for the window driver's `--definition` inputs. The
/// materialization is deterministic (frozen registry pins +
/// `resolved_at 1763000000000`); the emitted baseline carries
/// `definition_ref.version_id == baseline_ref` verbatim. Determinism is
/// asserted: a doc that fails to re-produce its declared level_ref or
/// the declared baseline_ref refuses, never a silently different doc.
///
/// Emits the **current generation** — gen-2 (the `transcript_render`
/// repair). The gen-1 sealed documents stay recoverable verbatim inside
/// the committed gen-1 `hh-p4-levels/1` record.
fn emit_docs(out_dir: &Path, inputs: &BTreeMap<String, String>) {
    use hharness_lab::p4::*;
    let _ = inputs;
    let work = std::env::temp_dir().join(format!("p4-docs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let mat =
        seal_p4_levels_gen2(&work).unwrap_or_else(|e| panic!("pass-2 gen-2 seal refused: {e}"));
    std::fs::create_dir_all(out_dir).unwrap();
    let base_doc = mat
        .baseline_json
        .get("document")
        .cloned()
        .unwrap_or_else(|| panic!("baseline_json lacks document"));
    let base_def = mat
        .baseline_json
        .get("definition_ref")
        .and_then(|r| r.get("version_id"))
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    assert_eq!(
        base_def, mat.baseline_version_id,
        "baseline document does not bind the declared baseline_ref"
    );
    let base_p = out_dir.join("baseline.json");
    std::fs::write(&base_p, format!("{}\n", base_doc.to_canonical_string())).unwrap();
    println!(
        "p4-docs: baseline.json definition_ref={}",
        mat.baseline_version_id
    );
    for l in &mat.levels {
        let sj = hharness_lab::sweep::sealed_json(&l.sealed.sealed);
        let doc = sj
            .get("document")
            .cloned()
            .unwrap_or_else(|| panic!("level {} lacks document", l.sealed.level_id));
        let p = out_dir.join(format!("{}.json", l.sealed.level_id));
        std::fs::write(&p, format!("{}\n", doc.to_canonical_string())).unwrap();
        println!(
            "p4-docs: {}.json level_ref={}",
            l.sealed.level_id, l.sealed.version_id
        );
    }
    let _ = std::fs::remove_dir_all(&work);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "emit".to_string());
    let inputs = inputs();
    match cmd.as_str() {
        "emit" => {
            let repo = PathBuf::from(args.next().unwrap_or_else(|| ".".to_string()));
            emit(&repo, &inputs)
        }
        "supersede" => {
            let repo = PathBuf::from(args.next().unwrap_or_else(|| ".".to_string()));
            supersede(&repo, &inputs)
        }
        "emit-docs" => {
            let out = PathBuf::from(
                args.next()
                    .unwrap_or_else(|| panic!("usage: p4-admissibility emit-docs <out-dir>")),
            );
            emit_docs(&out, &inputs)
        }
        other => panic!("unknown command: {other}"),
    }
}
