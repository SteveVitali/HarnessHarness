//! R6.43 — E2-T10 sweep glue + `sweep-registry/1` (REQ-XE-124/125,
//! REQ-XD-201/202; AC-1…AC-4 lab-side legs).

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use hharness_assembly::resolve::ResolveEnv;
use hharness_assembly::space::space;
use hharness_assembly::Stage1Catalog;
use hharness_identity::names::ResolveMode;
use hharness_lab::sweep::*;
use hharness_provenance::ProvenanceRecord;
use hharness_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-lab-r643-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn env<'a>(
    store: &'a mut hharness_registry::store::RegistryStore,
    cat: &'a Stage1Catalog,
) -> (ProvenanceRecord, ResolveEnv<'a>) {
    let k = ProvenanceRecord::kernel("sweep:test", 0);
    (
        k.clone(),
        ResolveEnv {
            registry: store,
            catalog: cat,
            snapshot_id: None,
            mode: ResolveMode::Audit,
            registrar: k,
            resolved_at: 1_763_000_000_000,
            notices: None,
        },
    )
}

// ── AC-1 (REQ-XE-124) — enumerate → sealed level → LevelSpec.ref_ ────────────

#[test]
fn ac1_enumerate_to_lab_levels() {
    let root = dir("ac1");
    let mut store = sweep_seeded_store(&root.join("reg"));
    let cat = Stage1Catalog::stage1();
    let (k, mut e) = env(&mut store, &cat);
    let authored = sweep_baseline_doc();
    let sp = space(&authored, &k).unwrap();

    // The V5 perturbation seals through enumerate → resolve → seal;
    // `LevelSpec.ref_` is the sealed version_id — digest-frozen.
    let v5 = materialize_level(&sp, "V5", &["reasoning_replay=drop"], &mut e, &cat, &k).unwrap();
    assert_eq!(v5.level_spec.ref_, v5.version_id);
    assert_eq!(v5.spellings, vec!["reasoning_replay=drop".to_string()]);
    // The override spellings ride `overrides` for attribution only.
    assert!(v5.level_spec.overrides.is_some());
    // The sealed level really drops reasoning.
    let j = hharness_hir::wire::sealed_definition_json(&v5.sealed);
    let canon = j.to_canonical_string();
    assert!(
        canon.contains("reasoning\":\"drop\"") || canon.contains("drop"),
        "{canon}"
    );

    // Identity: a level opened by its sealed ref admits; opened by
    // override spelling refuses typed (attribution is never an
    // interpreter); a foreign ref refuses.
    check_level_open(&v5.level_spec, &LevelOpen::ByRef(v5.version_id.clone())).unwrap();
    assert!(matches!(
        check_level_open(&v5.level_spec, &LevelOpen::ByRef("sha256:other".into())),
        Err(SweepRefusal::UndeclaredDelta { .. })
    ));
    assert!(matches!(
        check_level_open(
            &v5.level_spec,
            &LevelOpen::ByOverrides(vec!["reasoning_replay=drop".into()])
        ),
        Err(SweepRefusal::OpenedByOverride { .. })
    ));

    // A validate_assembly failure routes to rejected — the spelling
    // names a path outside the declared space; `UnexpressibleSurface`
    // is the typed refusal, never a silent downgrade.
    let e2 = materialize_level(&sp, "VX", &["undeclared_knob=1"], &mut e, &cat, &k).unwrap_err();
    assert!(
        matches!(e2, SweepRefusal::UnexpressibleSurface { .. }),
        "{e2:?}"
    );
}

// ── AC-2 (REQ-XE-125) — declared-delta + liveness checks ─────────────────────

#[test]
fn ac2_delta_and_liveness() {
    // Doc delta: the V5 materialization differs from the baseline in
    // exactly one registered slot, confined to declared typed paths.
    let root = dir("ac2");
    let m = materialize_v5(&root).unwrap();
    assert_ne!(m.baseline_version_id, m.level_version_id);
    let delta = assembly_delta(
        m.baseline_json
            .get("document")
            .and_then(|d| d.get("assembly"))
            .unwrap_or(&Json::Null),
        m.sealed_level_json
            .get("document")
            .and_then(|d| d.get("assembly"))
            .unwrap_or(&Json::Null),
    );
    let typed = vec![
        "slots/model_port/params/transcript_render.assistant_replay.reasoning".to_string(),
        "values/reasoning_replay".to_string(),
    ];
    check_doc_delta(&typed, &delta).unwrap();
    // The delta is non-empty on the declared path.
    assert!(!delta.is_empty(), "level must differ from baseline");
    // An undeclared-field change refuses.
    let e = check_doc_delta(
        &typed,
        &["/assembly/slots/control_strategy/params/x".into()],
    );
    assert!(matches!(e, Err(SweepRefusal::UndeclaredDelta { .. })));
    // Two slots touched refuses (exactly one registered slot).
    let e = check_doc_delta(
        &["slots/model_port".into(), "slots/control_strategy".into()],
        &[
            "/assembly/slots/model_port/params/a".into(),
            "/assembly/slots/control_strategy/params/b".into(),
        ],
    );
    assert!(matches!(e, Err(SweepRefusal::UndeclaredDelta { .. })));

    // Wire diff — the live factor (V5): confined to declared paths
    // and non-empty on its branch (turn ≥ 2).
    let live = v5_liveness();
    let base = r#"{"messages":[{"role":"system"},{"role":"assistant","reasoning_content":"r"}]}"#;
    let var = r#"{"messages":[{"role":"system"},{"role":"assistant"}]}"#;
    let diff = request_diff(
        &[b"req1".to_vec(), base.as_bytes().to_vec()],
        &[b"req1".to_vec(), var.as_bytes().to_vec()],
    );
    check_request_diff(&live, "transcript_render.assistant_replay.reasoning", &diff).unwrap();
    // The inert factor — an empty diff on the exercising branch
    // refuses `inert{factor,branch}`.
    let e = check_request_diff(&live, "context_policy", &[]);
    assert!(matches!(e, Err(SweepRefusal::Inert { .. })), "{e:?}");
    // A diff at turn 1 only (before the branch) is inert.
    let diff1 = request_diff(&[base.as_bytes().to_vec()], &[var.as_bytes().to_vec()]);
    assert!(matches!(
        check_request_diff(&live, "f", &diff1),
        Err(SweepRefusal::Inert { .. })
    ));
    // An undeclared path in the diff refuses.
    let undeclared = request_diff(
        &[br#"{"messages":[1],"other":1}"#.to_vec()],
        &[br#"{"messages":[1],"other":2}"#.to_vec()],
    );
    assert!(matches!(
        check_request_diff(&live, "f", &undeclared),
        Err(SweepRefusal::UndeclaredDelta { .. })
    ));

    // The `cmd-120` executed leg distinguishes rc −1 from rc 0.
    check_executed_leg("exec_defaults.timeout_ms", -1, 0).unwrap();
    assert!(matches!(
        check_executed_leg("exec_defaults.timeout_ms", 0, 0),
        Err(SweepRefusal::Inert { .. })
    ));
    assert!(matches!(
        check_executed_leg("exec_defaults.timeout_ms", -1, -1),
        Err(SweepRefusal::Inert { .. })
    ));

    // The dispatcher: an executor-level level with only a request
    // diff has no live admission path — typed refusal at arm time.
    assert!(matches!(
        check_liveness(
            &v3_liveness(),
            "exec_defaults.timeout_ms",
            "V3",
            &LivenessEvidence::RequestDiff(&diff)
        ),
        Err(SweepRefusal::NoAdmissionPath { .. })
    ));
    assert!(matches!(
        check_liveness(&v5_liveness(), "f", "V5", &LivenessEvidence::Absent),
        Err(SweepRefusal::NoAdmissionPath { .. })
    ));
    check_liveness(
        &v3_liveness(),
        "f",
        "V3",
        &LivenessEvidence::ExecutedLeg {
            baseline_rc: -1,
            level_rc: 0,
        },
    )
    .unwrap();

    // No parity label attaches to a swept variant.
    let reg = sweep_registry_record(Json::Null);
    assert_eq!(
        reg.get("parity").and_then(Json::as_str),
        Some("none — a swept variant carries no parity claim against the original (REQ-XE-125)")
    );
}

// ── AC-3 (REQ-XD-201) — the registry validator + the V5 declaration ──────────

#[test]
fn ac3_registry_validator_and_v5_declaration() {
    let decl = v5_decl_stub();
    let v5 = v5_level_entry("sha256:level-ref", &decl, V5_DECLARATION_SHA256);
    let reg = sweep_registry_record(v5);
    let frozen = freeze_registry(&reg);
    assert!(
        check_registry(&frozen).is_empty(),
        "{:?}",
        check_registry(&frozen)
    );
    assert!(!frozen
        .get("frozen_sha256")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .is_empty());

    // A level missing a required member refuses.
    let mut bad = Json::Obj(reg.as_obj_map());
    let levels = arr_of(obj_of(&mut bad).get_mut("levels").unwrap());
    obj_of(&mut levels[0]).remove("activation_metric");
    assert!(check_registry(&bad)
        .iter()
        .any(|r| matches!(r, SweepRefusal::MissingMember { member } if member.contains("activation_metric"))));

    // An entry without an upstream locator or "HH-only" refuses.
    let mut bad2 = Json::Obj(reg.as_obj_map());
    let levels = arr_of(obj_of(&mut bad2).get_mut("levels").unwrap());
    obj_of(&mut levels[0]).insert("upstream_locator".into(), Json::str("??"));
    assert!(check_registry(&bad2)
        .iter()
        .any(|r| matches!(r, SweepRefusal::MissingUpstream { .. })));

    // A constant not marked non-upstream refuses.
    let mut bad3 = Json::Obj(reg.as_obj_map());
    let levels = arr_of(obj_of(&mut bad3).get_mut("levels").unwrap());
    obj_of(&mut levels[0]).insert("constants".into(), Json::obj([("k", Json::Int(1))]));
    assert!(check_registry(&bad3)
        .iter()
        .any(|r| matches!(r, SweepRefusal::UnmarkedNonUpstream { .. })));

    // The V5 declaration byte-equals R6.33's (sha256 match); a
    // mismatched declaration refuses — never edited to fit.
    check_v5_declaration(&decl).unwrap();
    let mut wrong = decl.as_obj_map();
    wrong.insert("level_name".into(), Json::str("obs-2.5k"));
    assert!(matches!(
        check_v5_declaration(&Json::Obj(wrong)),
        Err(SweepRefusal::DeclarationMismatch { .. })
    ));
}

// ── AC-4 (REQ-XD-202) — Tier 1 = V1–V9, Tier 2 reasons, merge refusal ────────

#[test]
fn ac4_tiers_and_merge_refusal() {
    let reg = freeze_registry(&sweep_registry_record(v5_level_entry(
        "sha256:level-ref",
        &v5_decl_stub(),
        V5_DECLARATION_SHA256,
    )));
    // Tier 1 is exactly V1–V9.
    let t1: Vec<String> = reg
        .get("levels")
        .and_then(|l| l.as_arr_cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|l| l.get("level_id").and_then(Json::as_str).map(str::to_string))
        .collect();
    assert_eq!(t1, TIER1_LEVEL_IDS.map(str::to_string).to_vec());
    // Tier 2 carries every candidate with its reason.
    let t2 = reg.get("tier2").and_then(|t| t.as_arr_cloned()).unwrap();
    assert_eq!(t2.len(), TIER2_CANDIDATES.len());
    for e in &t2 {
        assert!(!e
            .get("reason")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .is_empty());
    }
    // Drop a Tier-1 level → tier violation.
    let mut bad = Json::Obj(reg.as_obj_map());
    arr_of(obj_of(&mut bad).get_mut("levels").unwrap()).remove(0);
    assert!(check_registry(&bad)
        .iter()
        .any(|r| matches!(r, SweepRefusal::TierViolation { .. })));

    // Merge refusal: a level changed after the freeze is a new
    // exploration identity — data never merge across identities.
    let id_a = frozen_identity(&reg);
    let mut reg2 = Json::Obj(reg.as_obj_map());
    obj_of(&mut arr_of(obj_of(&mut reg2).get_mut("levels").unwrap())[4])
        .insert("level".into(), Json::str("verbatim"));
    let reg2 = freeze_registry(&reg2);
    let id_b = frozen_identity(&reg2);
    assert_ne!(id_a, id_b);
    check_merge(&id_a, &id_a).unwrap();
    assert!(matches!(
        check_merge(&id_a, &id_b),
        Err(SweepRefusal::MergeAcrossIdentities { .. })
    ));
}

use std::collections::BTreeMap;

trait ObjMut {
    fn as_arr_cloned(&self) -> Option<Vec<Json>>;
    fn as_obj_map(&self) -> BTreeMap<String, Json>;
}
impl ObjMut for Json {
    fn as_arr_cloned(&self) -> Option<Vec<Json>> {
        match self {
            Json::Arr(a) => Some(a.clone()),
            _ => None,
        }
    }
    fn as_obj_map(&self) -> BTreeMap<String, Json> {
        match self {
            Json::Obj(m) => m.clone(),
            _ => panic!("not an object"),
        }
    }
}

fn obj_of(j: &mut Json) -> &mut BTreeMap<String, Json> {
    match j {
        Json::Obj(m) => m,
        _ => panic!("not an object"),
    }
}
fn arr_of(j: &mut Json) -> &mut Vec<Json> {
    match j {
        Json::Arr(a) => a,
        _ => panic!("not an array"),
    }
}

fn frozen_identity(reg: &Json) -> String {
    reg.get("exploration_identity")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The R6.33 `v5_declaration` sub-object (canonical — sha256 must be
/// `e950043f…`); stubbed identically to `fp_prereg::v5_declaration()`.
fn v5_decl_stub() -> Json {
    // The committed declaration's canonical bytes — reproduced from
    // the packet's recorded sub-object (the emitter embeds the real
    // member verbatim; this stub is the same canonical object).
    let p =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/fp-prereg-inputs.txt");
    let txt = std::fs::read_to_string(&p).expect("inputs manifest");
    let rel = txt
        .lines()
        .find(|l| l.starts_with("packet="))
        .map(|l| l.split('=').nth(1).unwrap().to_string())
        .unwrap();
    let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel);
    let packet: Json = hharness_wire::json::parse(&std::fs::read_to_string(full).unwrap()).unwrap();
    packet.get("v5_declaration").unwrap().clone()
}
