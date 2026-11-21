//! R7.3 — the generation-aware baseline authoring repairs
//! (manifest 329): the gen-2 declared baseline carries the complete
//! replay-fidelity member set the ws1 authored H doc declared (G-L1
//! rerun-4 diagnosed the gen-1 gap — `assistant_replay.reasoning`
//! alone), the gen-3 baseline adds the `control_strategy` submission
//! contract the same ws1 doc declared (the gen-2 battery's
//! trajectory-termination divergence — the bare `submit` preset's
//! dormant marker), the swept `reasoning_replay` leaf stays on its
//! `$param:` marker, the gen-1/gen-2 machinery still reproduces the
//! frozen `88042f…`/`ec27a278…` and gen-2 artifacts byte-identical,
//! and the committed gen-2/gen-3 records carry the append-only
//! `supersedes` lineage cites.
//! Offline fixtures only — no subject, model call, or paid run.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use hharness_lab::p4::*;
use hharness_lab::p4x::*;
use hharness_wire::json::{parse, Json};
use hharness_wire::sha256::sha256_hex;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load(rel: &str) -> Json {
    parse(&std::fs::read_to_string(root().join(rel)).unwrap()).unwrap()
}

fn sha_file(rel: &str) -> String {
    // The committed file's bytes verbatim — the same pin the `.sha256`
    // sidecars and the emitter's `prior_sha256` cite use
    // (`shasum -a 256 <file>`).
    let bytes = std::fs::read(root().join(rel)).unwrap();
    format!("sha256:{}", sha256_hex(bytes.as_slice()))
}

fn as_obj(j: &Json) -> &BTreeMap<String, Json> {
    match j {
        Json::Obj(m) => m,
        _ => panic!("expected an object"),
    }
}

fn as_arr(j: &Json) -> &Vec<Json> {
    match j {
        Json::Arr(a) => a,
        _ => panic!("expected an array"),
    }
}

/// `strip_refs(j)` — deep-copy `j` with every content-addressed member
/// removed: object members `ref`, `version_id`, `artifact_ref`,
/// `experiment_id` anywhere in the tree. Envelope-parity comparisons run on the
/// stripped trees — the digests legitimately differ between
/// generations; everything else is verbatim.
fn strip_refs(j: &Json) -> Json {
    match j {
        Json::Obj(m) => Json::Obj(
            m.iter()
                .filter(|(k, _)| {
                    !matches!(
                        k.as_str(),
                        "ref" | "version_id" | "artifact_ref" | "experiment_id"
                    )
                })
                .map(|(k, v)| (k.clone(), strip_refs(v)))
                .collect(),
        ),
        Json::Arr(a) => Json::Arr(a.iter().map(strip_refs).collect()),
        other => other.clone(),
    }
}

/// `collect_refs(j)` — every stripped digest member, flattened in
/// document order (parity assertions assert the sets differ).
fn collect_refs(j: &Json, out: &mut Vec<String>) {
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                if matches!(
                    k.as_str(),
                    "ref" | "version_id" | "artifact_ref" | "experiment_id"
                ) {
                    if let Json::Str(s) = v {
                        out.push(s.clone());
                    }
                }
                collect_refs(v, out);
            }
        }
        Json::Arr(a) => {
            for v in a {
                collect_refs(v, out);
            }
        }
        _ => {}
    }
}

const G1_REG: &str = "docs/build/reports/2026-11-20-fp-p4-sweep-registry.json";
const G1_LEVELS: &str = "docs/build/reports/2026-11-20-fp-p4-levels.json";
const G1_PREREG: &str = "docs/build/reports/2026-11-20-fp-p4x-prereg.json";
const G2_REG: &str = "docs/build/reports/2026-11-21-fp-p4-sweep-registry.json";
const G2_LEVELS: &str = "docs/build/reports/2026-11-21-fp-p4-levels.json";
const G2_WRULE: &str = "docs/build/reports/2026-11-21-fp-p4-w-rule.json";
const G2_ADM: &str = "docs/build/reports/2026-11-21-fp-p4-admissibility.json";
const G2_PREREG: &str = "docs/build/reports/2026-11-21-fp-p4x-prereg.json";
const TASKS: &str = "docs/build/reports/2026-11-08-fp-selection.json";

/// The ws1-authored replay-fidelity set in the admitted spellings —
/// what the gen-2 baseline's `transcript_render` must declare on top
/// of the swept `reasoning` leaf.
const FIDELITY: &[(&str, &str)] = &[
    ("id_normalisation", "provider_verbatim"),
    ("format_error_turn", "omit"),
    ("text_only_turn", "drop"),
    ("tool_arguments", "verbatim"),
];

/// The authored doc's `model_port` params — where the declaration
/// lands before `$param:` binding.
fn authored_tr(gen: fn() -> hharness_hir::document::HirDocument) -> Json {
    let doc = gen();
    let asm = doc.assembly.clone().unwrap();
    asm.get("slots")
        .and_then(|s| s.get("model_port"))
        .and_then(|mp| mp.get("one"))
        .and_then(|o| o.get("params"))
        .and_then(|p| p.get("transcript_render"))
        .cloned()
        .expect("model_port params carry transcript_render")
}

// ── AC-1: the gen-2 authored doc declares the complete set ───────────────────

#[test]
fn ac1_gen2_declares_full_fidelity() {
    let tr = authored_tr(sweep_baseline_doc_p4_gen2);
    let m = as_obj(&tr);
    // The swept leaf stays parameterized — only `reasoning` differs
    // from ws1's fixed `verbatim`.
    let ar = m
        .get("assistant_replay")
        .map(as_obj)
        .expect("assistant_replay is an object");
    assert_eq!(
        ar.get("reasoning").and_then(Json::as_str),
        Some("$param:reasoning_replay"),
        "the swept reasoning leaf stays on its $param: marker"
    );
    assert_eq!(
        ar.get("content").and_then(Json::as_str),
        Some("verbatim"),
        "assistant_replay.content declared (the ws1 `keep` binding)"
    );
    for (k, want) in FIDELITY {
        assert_eq!(
            m.get(*k).and_then(Json::as_str),
            Some(*want),
            "transcript_render.{k} declared"
        );
    }
    // Exactly the ws1 member set + the swept leaf — nothing extra.
    let mut keys: Vec<&String> = m.keys().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "assistant_replay",
            "format_error_turn",
            "id_normalisation",
            "text_only_turn",
            "tool_arguments"
        ]
    );
}

#[test]
fn ac1_gen1_authored_set_unchanged() {
    // The frozen gen-1 authoring declares only the swept leaf — the
    // R7.3 rerun-4 root cause, preserved for byte-identical gen-1
    // recompute.
    let tr = authored_tr(sweep_baseline_doc_p4);
    let m = as_obj(&tr);
    assert_eq!(m.len(), 1, "gen-1 declares assistant_replay only");
    let ar = m.get("assistant_replay").map(as_obj).unwrap();
    assert_eq!(
        ar.get("reasoning").and_then(Json::as_str),
        Some("$param:reasoning_replay")
    );
    assert_eq!(ar.len(), 1);
}

// ── AC-2: the seal — new identity, bound values, gen-1 reproduction ──────────

#[test]
fn ac2_gen2_seal_binds_the_fidelity_set() {
    let work = std::env::temp_dir().join(format!("r7-3-seal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let mat = seal_p4_levels_gen2(&work).unwrap();
    // The new baseline identity — a new H.
    assert_eq!(
        mat.baseline_version_id,
        "sha256:d4a9302be48a4ad635876b9d57bcafed10616fc1067187f29baa4f572eb0aab9"
    );
    // The sealed baseline's bound transcript_render — every member
    // resolved to its declared value (the ws1 set).
    let tr = mat
        .baseline_json
        .get("document")
        .and_then(|d| d.get("assembly"))
        .and_then(|a| a.get("slots"))
        .and_then(|s| s.get("model_port"))
        .and_then(|mp| mp.get("one"))
        .and_then(|o| o.get("params"))
        .and_then(|p| p.get("transcript_render"))
        .cloned()
        .unwrap();
    let m = as_obj(&tr);
    let ar = m.get("assistant_replay").map(as_obj).unwrap();
    assert_eq!(ar.get("content").and_then(Json::as_str), Some("verbatim"));
    assert_eq!(
        ar.get("reasoning").and_then(Json::as_str),
        Some("verbatim"),
        "baseline binds the declared reasoning_replay value"
    );
    for (k, want) in FIDELITY {
        assert_eq!(m.get(*k).and_then(Json::as_str), Some(*want));
    }
    // V5's swept override binds `drop` on the same repaired set.
    let v5 = mat
        .levels
        .iter()
        .find(|l| l.sealed.level_id == "V5")
        .unwrap();
    let v5doc = hharness_lab::sweep::sealed_json(&v5.sealed.sealed);
    let v5tr = v5doc
        .get("document")
        .and_then(|d| d.get("assembly"))
        .and_then(|a| a.get("slots"))
        .and_then(|s| s.get("model_port"))
        .and_then(|mp| mp.get("one"))
        .and_then(|o| o.get("params"))
        .and_then(|p| p.get("transcript_render"))
        .cloned()
        .unwrap();
    assert_eq!(
        v5tr.get("assistant_replay")
            .and_then(|a| a.get("reasoning"))
            .and_then(Json::as_str),
        Some("drop")
    );
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn ac2_gen1_seal_reproduces_frozen_identity() {
    // The frozen artifact's reproducer is never deleted — gen-1 seals
    // to the committed `88042f…` baseline.
    let work = std::env::temp_dir().join(format!("r7-3-g1-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let mat = seal_p4_levels(&work).unwrap();
    assert_eq!(
        mat.baseline_version_id,
        "sha256:88042f890bdedb91bcf865d34630499b87f17b05d53d3640a3deaa863a8a0844"
    );
    let _ = std::fs::remove_dir_all(&work);
}

// ── AC-3: the committed gen-2 records — lineage + envelope parity ────────────

#[test]
fn ac3_gen2_registry_validates_and_cites() {
    let reg = load(G2_REG);
    let refusals = hharness_lab::sweep::check_registry(&reg);
    assert!(refusals.is_empty(), "gen-2 registry refused: {refusals:?}");
    // The new x2 identity — a different frozen_sha256 than gen-1's.
    let g1 = load(G1_REG);
    let g2_sha = reg.get("frozen_sha256").and_then(Json::as_str).unwrap();
    let g1_sha = g1.get("frozen_sha256").and_then(Json::as_str).unwrap();
    assert_ne!(g1_sha, g2_sha);
    assert_eq!(
        reg.get("exploration_identity").and_then(Json::as_str),
        Some(format!("x2:sha256:{g2_sha}").as_str())
    );
    // The lineage cite: prior path + the gen-1 file's actual sha256.
    let s = reg.get("supersedes").map(as_obj).unwrap();
    assert_eq!(s.get("prior").and_then(Json::as_str), Some(G1_REG));
    assert_eq!(
        s.get("prior_sha256").and_then(Json::as_str),
        Some(sha_file(G1_REG).as_str())
    );
    assert_eq!(s.get("byte_untouched"), Some(&Json::Bool(true)));
    assert!(s
        .get("reason")
        .and_then(Json::as_str)
        .unwrap()
        .contains("transcript_render"));
    // The gen-1 record carries no supersedes member.
    assert!(g1.get("supersedes").is_none());
}

#[test]
fn ac3_gen2_levels_refs() {
    let lvl = load(G2_LEVELS);
    assert_eq!(
        lvl.get("baseline_ref").and_then(Json::as_str),
        Some("sha256:d4a9302be48a4ad635876b9d57bcafed10616fc1067187f29baa4f572eb0aab9")
    );
    assert_eq!(
        lvl.get("supersedes")
            .and_then(|s| s.get("prior"))
            .and_then(Json::as_str),
        Some(G1_LEVELS)
    );
    let s = lvl.get("supersedes").map(as_obj).unwrap();
    assert_eq!(
        s.get("prior_sha256").and_then(Json::as_str),
        Some(sha_file(G1_LEVELS).as_str())
    );
    // Every V1–V7 doc carries the complete bound set.
    for l in lvl.get("levels").map(as_arr).unwrap() {
        let tr = l
            .get("sealed")
            .and_then(|s| s.get("document"))
            .and_then(|d| d.get("assembly"))
            .and_then(|a| a.get("slots"))
            .and_then(|s| s.get("model_port"))
            .and_then(|mp| mp.get("one"))
            .and_then(|o| o.get("params"))
            .and_then(|p| p.get("transcript_render"))
            .cloned()
            .unwrap();
        let m = as_obj(&tr);
        for (k, want) in FIDELITY {
            assert_eq!(
                m.get(*k).and_then(Json::as_str),
                Some(*want),
                "{} transcript_render.{k}",
                l.get("level_id").and_then(Json::as_str).unwrap()
            );
        }
    }
}

#[test]
fn ac3_gen2_prereg_supersedes_envelope_unchanged() {
    let pre = load(G2_PREREG);
    assert_eq!(
        pre.get("schema").and_then(Json::as_str),
        Some("hh-p4-x-prereg/1")
    );
    assert_eq!(pre.get("ticket").and_then(Json::as_str), Some("329_R7.3"));
    let g1 = load(G1_PREREG);
    // The lineage cite binds the gen-1 record's actual bytes.
    let s = pre.get("supersedes").map(as_obj).unwrap();
    assert_eq!(
        s.get("prior_sha256").and_then(Json::as_str),
        Some("sha256:ec27a2789f4e1db9a58f76cc923992b76dda995934d8cc777a838fd9e071d6ba")
    );
    assert_eq!(
        s.get("prior_sha256").and_then(Json::as_str),
        Some(sha_file(G1_PREREG).as_str())
    );
    // The gen-2 mint honestly postdates the gate — never borrows
    // gen-1's "predates GATE-G35" claim.
    let predates = pre.get("predates").and_then(Json::as_str).unwrap();
    assert!(predates.contains("postdates GATE-G35"), "{predates}");
    // The authorized envelope is unchanged — slots, arms, tasks,
    // withheld set, ceilings, the frozen selection rule. The design's
    // content-addressed members legitimately moved (that is the point
    // of the new generation); the envelope members are verbatim modulo
    // those digests.
    let mut r2 = Vec::new();
    let mut r1 = Vec::new();
    collect_refs(pre.get("design").unwrap(), &mut r2);
    collect_refs(g1.get("design").unwrap(), &mut r1);
    assert!(!r2.is_empty(), "the design carries digest members");
    assert!(
        r2.iter().all(|r| !r1.contains(r)),
        "every design digest moved"
    );
    assert_eq!(
        strip_refs(pre.get("design").unwrap()),
        strip_refs(g1.get("design").unwrap()),
        "design verbatim modulo the moved digests"
    );
    for member in [
        "envelope",
        "tasks",
        "withheld",
        "stop_rules",
        "selection_rule",
        "selection_rule_sha256",
        "estimators",
        "labels",
        "combine_declaration",
        "selection_packet",
        "integrity",
        "expected_outcome",
        "spend",
        "cohort",
    ] {
        assert_eq!(pre.get(member), g1.get(member), "{member} unchanged");
    }
    // The W outcome + verbatim rule are unchanged; the cited record
    // is the gen-2 path.
    let w2 = pre.get("w").unwrap();
    let w1 = g1.get("w").unwrap();
    assert_eq!(w2.get("outcome"), w1.get("outcome"));
    assert_eq!(w2.get("verbatim"), w1.get("verbatim"));
    assert_eq!(w2.get("record").and_then(Json::as_str), Some(G2_WRULE));
    // The baseline ref moved to the new H.
    assert_ne!(
        pre.get("baseline").and_then(|b| b.get("baseline_ref")),
        g1.get("baseline").and_then(|b| b.get("baseline_ref"))
    );
    // The registry citation binds the gen-2 registry.
    let reg = load(G2_REG);
    assert_eq!(
        pre.get("registry").and_then(|r| r.get("sha256")),
        reg.get("frozen_sha256")
    );
}

#[test]
fn ac3_gen2_prereg_recomputes() {
    // The gen-2 prereg recomputes deterministically through the
    // generation-parameterized mint over the committed gen-2 inputs.
    let reg = load(G2_REG);
    let levels = load(G2_LEVELS);
    let w_rule = load(G2_WRULE);
    let adm = load(G2_ADM);
    let tasks = load(TASKS);
    let prior_sha = sha_file(G1_PREREG);
    let rec = x_prereg_record_gen(
        &reg,
        &levels,
        &w_rule,
        &adm,
        &tasks,
        G2_REG,
        G2_WRULE,
        TASKS,
        &x_prereg_gen2(
            G1_PREREG,
            &prior_sha,
            "transcript_render authoring-completeness repair (R7.3): the gen-1 baseline declared only assistant_replay.reasoning — G-L1 rerun-4 diverged on the undeclared members; the authorized envelope (slots, arms, seeds, caps) is unchanged",
        ),
    )
    .unwrap();
    let committed = load(G2_PREREG);
    assert_eq!(
        rec.to_canonical_string(),
        committed.to_canonical_string(),
        "gen-2 prereg recomputes"
    );
    check_registry_cite(&reg, &rec).unwrap();
    // And gen-1 still recomputes its frozen record.
    let g1 = load(G1_PREREG);
    let g1_reg = load(G1_REG);
    let g1_levels = load("docs/build/reports/2026-11-20-fp-p4-levels.json");
    let g1_w = load("docs/build/reports/2026-11-20-fp-p4-w-rule.json");
    let g1_adm = load("docs/build/reports/2026-11-20-fp-p4-admissibility.json");
    let rec1 = x_prereg_record(
        &g1_reg,
        &g1_levels,
        &g1_w,
        &g1_adm,
        &tasks,
        G1_REG,
        "docs/build/reports/2026-11-20-fp-p4-w-rule.json",
        TASKS,
    )
    .unwrap();
    assert_eq!(rec1.to_canonical_string(), g1.to_canonical_string());
}

// ── AC-4: generation-3 — the control_strategy submission contract ────────────

const G3_REG: &str = "docs/build/reports/2026-11-21-fp-p4-sweep-registry-gen3.json";
const G3_LEVELS: &str = "docs/build/reports/2026-11-21-fp-p4-levels-gen3.json";
const G3_WRULE: &str = "docs/build/reports/2026-11-21-fp-p4-w-rule-gen3.json";
const G3_ADM: &str = "docs/build/reports/2026-11-21-fp-p4-admissibility-gen3.json";
const G3_PREREG: &str = "docs/build/reports/2026-11-21-fp-p4x-prereg-gen3.json";

/// The authored doc's `control_strategy` slot params — where the
/// submission contract lands before `$param:` binding.
fn authored_cs(gen: fn() -> hharness_hir::document::HirDocument) -> Json {
    let doc = gen();
    let asm = doc.assembly.clone().unwrap();
    asm.get("slots")
        .and_then(|s| s.get("control_strategy"))
        .and_then(|mp| mp.get("one"))
        .and_then(|o| o.get("params"))
        .cloned()
        .expect("control_strategy slot params")
}

#[test]
fn ac4_gen3_declares_the_submission_contract() {
    let cs = authored_cs(sweep_baseline_doc_p4_gen3);
    let m = as_obj(&cs);
    // The ws1-authored control set — the marker the bare `submit`
    // preset leaves dormant (`marker: None`), which the gen-2 G-L1
    // battery's trajectory-termination divergence diagnosed.
    assert_eq!(
        m.get("completion_surface").and_then(Json::as_str),
        Some("none")
    );
    assert_eq!(
        m.get("format_failure_scope").and_then(Json::as_str),
        Some("batch")
    );
    assert_eq!(
        m.get("max_consecutive_format_errors")
            .and_then(Json::as_int),
        Some(3)
    );
    assert_eq!(
        m.get("tool_batch_mode").and_then(Json::as_str),
        Some("sequential")
    );
    let sub = as_obj(as_obj(m.get("stop_rule").unwrap()).get("submit").unwrap());
    assert_eq!(
        sub.get("marker").and_then(Json::as_str),
        Some("COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT")
    );
    assert_eq!(sub.get("line").and_then(Json::as_str), Some("first"));
    assert_eq!(
        sub.get("require_returncode").and_then(Json::as_int),
        Some(0)
    );
    // And the gen-3 doc keeps the full gen-2 fidelity set.
    let tr = authored_tr(sweep_baseline_doc_p4_gen3);
    let tm = as_obj(&tr);
    for (k, v) in FIDELITY {
        assert_eq!(
            tm.get(*k).and_then(Json::as_str),
            Some(*v),
            "gen-3 keeps fidelity.{k}"
        );
    }
    let ar = as_obj(tm.get("assistant_replay").unwrap());
    assert_eq!(ar.get("content").and_then(Json::as_str), Some("verbatim"));
    // The gen-2 doc does NOT declare the marker — that is exactly the
    // authored difference between the generations.
    let g2_cs = authored_cs(sweep_baseline_doc_p4_gen2);
    assert!(
        g2_cs.get("stop_rule").is_none(),
        "gen-2 stays bare — the divergence it diagnosed"
    );
}

#[test]
fn ac4_gen3_seal_binds_the_contract() {
    let work = std::env::temp_dir().join(format!("r7-3-g3-seal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let mat = seal_p4_levels_gen3(&work).unwrap();
    // The baseline carries the sealed contract; the level docs match
    // the committed gen-3 record's refs.
    let committed = load(G3_LEVELS);
    assert_eq!(
        mat.baseline_version_id,
        committed
            .get("baseline_ref")
            .and_then(Json::as_str)
            .unwrap()
    );
    let mut got: Vec<String> = mat
        .levels
        .iter()
        .map(|l| l.sealed.version_id.clone())
        .collect();
    got.sort();
    let mut want: Vec<String> = as_arr(committed.get("levels").unwrap())
        .iter()
        .map(|l| {
            l.get("level_ref")
                .and_then(Json::as_str)
                .unwrap()
                .to_string()
        })
        .collect();
    want.sort();
    assert_eq!(got, want, "gen-3 level refs match the committed record");
    // The sealed baseline's bound control_strategy carries the marker.
    let doc = mat
        .baseline_json
        .get("document")
        .and_then(|d| d.get("assembly"))
        .and_then(|a| a.get("slots"))
        .and_then(|s| s.get("control_strategy"))
        .and_then(|mp| mp.get("one"))
        .and_then(|o| o.get("params"))
        .and_then(|p| p.get("stop_rule"))
        .and_then(|sr| sr.get("submit"))
        .cloned()
        .unwrap();
    assert_eq!(
        doc.get("marker").and_then(Json::as_str),
        Some("COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT")
    );
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn ac4_gen3_doc_gen_selector() {
    // The generation selector reproduces each mint; gen-1 stays on
    // its own entry point.
    for (doc_gen, committed_levels) in [(2u32, G2_LEVELS), (3u32, G3_LEVELS)] {
        let work = std::env::temp_dir().join(format!("r7-3-g{doc_gen}-sel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).unwrap();
        let mat = seal_p4_levels_doc_gen(&work, doc_gen).unwrap();
        let committed = load(committed_levels);
        assert_eq!(
            mat.baseline_version_id,
            committed
                .get("baseline_ref")
                .and_then(Json::as_str)
                .unwrap(),
            "gen-{doc_gen} baseline ref"
        );
        let _ = std::fs::remove_dir_all(&work);
    }
    // An unadmitted generation refuses (panic — the emitter-side
    // input-admission convention, never a silent fallback).
    let w = std::env::temp_dir().join(format!("r7-3-gX-{}", std::process::id()));
    assert!(std::panic::catch_unwind(|| {
        let _ = seal_p4_levels_doc_gen(&w, 9);
    })
    .is_err());
}

#[test]
fn ac4_gen3_prereg_recomputes() {
    // The gen-3 prereg recomputes deterministically through the
    // generation-parameterized mint over the committed gen-3 inputs,
    // citing the gen-2 record.
    let reg = load(G3_REG);
    let levels = load(G3_LEVELS);
    let w_rule = load(G3_WRULE);
    let adm = load(G3_ADM);
    let tasks = load(TASKS);
    let prior_sha = sha_file(G2_PREREG);
    let rec = x_prereg_record_gen(
        &reg,
        &levels,
        &w_rule,
        &adm,
        &tasks,
        G3_REG,
        G3_WRULE,
        TASKS,
        &x_prereg_gen3(
            G2_PREREG,
            &prior_sha,
            "control_strategy authoring-completeness repair (R7.3): the gen-2 baseline's control_strategy.params left the submit-marker detection dormant — the gen-2 G-L1 battery held request-level parity and diverged at trajectory termination; the authorized envelope (slots, arms, seeds, caps) is unchanged",
        ),
    )
    .unwrap();
    let committed = load(G3_PREREG);
    assert_eq!(
        rec.to_canonical_string(),
        committed.to_canonical_string(),
        "gen-3 prereg recomputes"
    );
    check_registry_cite(&reg, &rec).unwrap();
    // The doc_gen selector reproduces both superseding mints.
    for doc_gen in [2u32, 3] {
        let m = x_prereg_doc_gen(doc_gen, "prior", "sha256:0", "reason");
        assert!(m.recorded_at.starts_with("2026-11-"), "gen-{doc_gen}");
    }
    assert!(std::panic::catch_unwind(|| x_prereg_doc_gen(9, "p", "s", "r")).is_err());
}
