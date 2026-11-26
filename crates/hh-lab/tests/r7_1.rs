//! R7.1 — phase-4 level admissibility: the pass's frozen
//! `sweep-registry/1` (new exploration identity), sealing through
//! R6.43's enumerate→seal, REQ-XD-225 tier-2 admission, REQ-XD-204
//! effort lane, REQ-XD-205 W rule (+REQ-XE-127), REQ-XD-208
//! activation fold, REQ-XD-209 inert forecast.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use hharness_lab::p4::*;
use hharness_lab::sweep;
use hharness_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    let p = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("hh-lab-r71-{tag}"));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// The registry store's atomic tmp+rename flush is serialized across
/// the FS-heavy legs (parallel store writes race on this host).
static STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn mat(tag: &str) -> P4Materialization {
    let _g = STORE_LOCK.lock().unwrap();
    seal_p4_levels(&dir(tag)).expect("seal")
}

fn registry(mat: &P4Materialization) -> Json {
    let mut extra: BTreeMap<String, Vec<(String, Json)>> = BTreeMap::new();
    for id in ["V8", "V9"] {
        extra.insert(
            id.to_string(),
            vec![(
                "withheld".to_string(),
                Json::str("withheld{inert_at_admissible_window}"),
            )],
        );
    }
    sweep::freeze_registry_as(&sweep_registry_record_p4(mat, &extra, Json::Null), P4_TAG)
}

// ── AC-1 (REQ-XD-225) — tier-2 admission ─────────────────────────────────────

#[test]
fn ac1_tier2_admission() {
    // Dependency not landed → refused entry to this identity.
    let r = check_tier2_admission("context-policy", false, Some(900_000));
    assert_eq!(
        admission_verdict(&r).as_deref(),
        Some("refused{dependency_not_landed}")
    );
    // Landed but the forecast misses the floor → refused.
    let r = check_tier2_admission("truncation-off", true, Some(100_000));
    assert_eq!(
        admission_verdict(&r).as_deref(),
        Some("refused{forecast_below_floor}")
    );
    // Landed + no forecast → refused.
    let r = check_tier2_admission("step-limit", true, None);
    assert_eq!(
        admission_verdict(&r).as_deref(),
        Some("refused{no_forecast}")
    );
    // Satisfying candidate → admits only to a LATER identity, never
    // this one (recorded with its reason).
    let r = check_tier2_admission("explicit-temperature", true, Some(500_000));
    assert_eq!(
        admission_verdict(&r).as_deref(),
        Some("admits{later_identity}")
    );
    assert_eq!(
        r.get("this_identity").and_then(Json::as_str),
        Some("never admitted")
    );
}

// ── AC-2 (REQ-XD-204) — effort lane fidelity ─────────────────────────────────

#[test]
fn ac2_effort_lane_fidelity() {
    let call = |id: &str, req: Option<&str>, app: &str| LaneCall {
        call_id: id.to_string(),
        requested: req.map(str::to_string),
        applied: app.to_string(),
        path: "reasoning_effort".to_string(),
    };
    // All applied = declared → the fidelity record carries every
    // call's requested + applied values.
    let rec = check_effort_fidelity(
        "V1",
        "low",
        &[
            call("mc-1", Some("low"), "low"),
            call("mc-2", None, "low"),
            call("mc-3", Some("high"), "low"), // a pin overwrite is honest
        ],
    )
    .unwrap();
    assert_eq!(
        rec.get("fidelity").and_then(Json::as_str),
        Some("applied = declared on 100% of calls")
    );
    // One mis-pinned call makes the level ineligible.
    assert!(matches!(
        check_effort_fidelity("V1", "low", &[call("mc-9", None, "high")]),
        Err(sweep::SweepRefusal::MisPinned { .. })
    ));
    // The declared lane parameter is a member of the sealed level
    // document — V1's assembly carries effort=low.
    let m = mat("ac2-seal");
    let v1 = m.levels.iter().find(|l| l.sealed.level_id == "V1").unwrap();
    let canon = hharness_lab::sweep::sealed_json(&v1.sealed.sealed).to_canonical_string();
    assert!(canon.contains("\"effort\":\"low\""), "{canon}");
}

// ── AC-3 (REQ-XD-205 + REQ-XE-127) — the W rule ──────────────────────────────

#[test]
fn ac3_w_rule() {
    // The input manifest refuses outcome fields (the reader too).
    assert!(matches!(
        check_non_outcome_fields(["tokens.input_total", "peak"]),
        Ok(())
    ));
    assert!(matches!(
        check_non_outcome_fields(["tokens.input_total", "reward"]),
        Err(sweep::SweepRefusal::OutcomeField { .. })
    ));
    assert!(matches!(
        check_non_outcome_fields(["trial.verdict"]),
        Err(sweep::SweepRefusal::OutcomeField { .. })
    ));
    let mut rec = BTreeMap::new();
    rec.insert("peak_input_tokens".to_string(), Json::Int(50_000));
    assert!(check_record_non_outcome(&Json::Obj(rec.clone())).is_ok());
    rec.insert("outcome".to_string(), Json::str("solved"));
    assert!(matches!(
        check_record_non_outcome(&Json::Obj(rec)),
        Err(sweep::SweepRefusal::OutcomeField { .. })
    ));

    // F computation: 10 trials, peaks at 10k×6 and 40k×4.
    let peaks = vec![10_000u64; 6]
        .into_iter()
        .chain(vec![40_000u64; 4])
        .collect::<Vec<_>>();
    assert_eq!(f_over(&peaks, 32_768), (4, 10));
    let wr = w_rule(&peaks);
    // F(32768) = .4 ≥ .25 → no withhold; no candidate reaches .5 →
    // W stands at the 32 768 floor.
    assert_eq!(wr.outcome, WOutcome::Chosen { w: 32_768 });
    // A distribution where F(65536) ≥ 0.5 selects 65 536.
    let peaks2 = vec![80_000u64; 6]
        .into_iter()
        .chain(vec![10_000u64; 4])
        .collect::<Vec<_>>();
    let wr2 = w_rule(&peaks2);
    assert_eq!(wr2.outcome, WOutcome::Chosen { w: 65_536 });
    // F(32768) < 0.25 → both composites withhold.
    let peaks3 = vec![5_000u64; 8]
        .into_iter()
        .chain(vec![40_000u64; 2])
        .collect::<Vec<_>>();
    assert!(matches!(
        w_rule(&peaks3).outcome,
        WOutcome::WithheldInertAtAdmissibleWindow
    ));
    // An empty corpus is pending{records}, never a fabricated F.
    assert_eq!(w_rule(&[]).outcome, WOutcome::PendingRecords);

    // A task-varying W refuses; input padding refuses.
    let good = Json::obj([
        ("strategy", Json::str("clear_tool_results")),
        ("window_cap", Json::Int(32_768)),
    ]);
    assert!(check_w_decl(&good).is_empty());
    let bad_task = Json::obj([
        ("strategy", Json::str("clear_tool_results")),
        ("window_cap", Json::Int(32_768)),
        ("task_caps", Json::obj([("task-a", Json::Int(49_152))])),
    ]);
    assert!(check_w_decl(&bad_task)
        .iter()
        .any(|r| matches!(r, sweep::SweepRefusal::TaskConditionedCap { .. })));
    let bad_pad = Json::obj([
        ("strategy", Json::str("clear_tool_results")),
        ("window_cap", Json::Int(32_768)),
        ("pad_inputs_to", Json::Int(32_768)),
    ]);
    assert!(check_w_decl(&bad_pad)
        .iter()
        .any(|r| matches!(r, sweep::SweepRefusal::InputPadding { .. })));
    // A cap below the floor refuses.
    let bad_floor = Json::obj([
        ("strategy", Json::str("clear_tool_results")),
        ("window_cap", Json::Int(16_384)),
    ]);
    assert!(!check_w_decl(&bad_floor).is_empty());

    // The composite machinery itself: S + W spellings seal over the
    // declared space and the delta is confined to the declared paths
    // (the level is withheld this pass — the machinery is proven at
    // a fixture W, the registry records the withhold).
    let _g = STORE_LOCK.lock().unwrap();
    let root = dir("composite");
    let kernel = hharness_provenance::ProvenanceRecord::kernel("sweep:test", 0);
    let mut store = sweep_seeded_store_p4(&root.join("reg"));
    let cat = hharness_assembly::Stage1Catalog::stage1();
    let mut env = hharness_assembly::resolve::ResolveEnv {
        registry: &mut store,
        catalog: &cat,
        snapshot_id: None,
        mode: hharness_identity::names::ResolveMode::Audit,
        registrar: kernel.clone(),
        resolved_at: 1_763_000_000_000,
        notices: None,
    };
    let authored = sweep_baseline_doc_p4();
    let sp = hharness_assembly::space::space(&authored, &kernel).unwrap();
    let v8 = sweep::materialize_level_composite(
        &sp,
        "V8",
        &["cmp_strategy=clear_tool_results", "window_cap=32768"],
        &mut env,
        &cat,
        &kernel,
    )
    .expect("composite seals at a fixture W");
    let canon = sweep::sealed_json(&v8.sealed).to_canonical_string();
    assert!(canon.contains("clear_tool_results"), "{canon}");
    assert!(canon.contains("\"window_cap\":32768"), "{canon}");
}

// ── AC-4 (REQ-XD-208) — the §4.2-generic activation fold ─────────────────────

#[test]
fn ac4_activation_fold() {
    // Always-active knob.
    let f = fold_activation(
        "V1",
        ActivationMetric::Always,
        ActivationEvidence {
            exercised: 0,
            total: 0,
        },
    );
    assert_eq!(activation_ppm(&f), 1_000_000);
    // Share-of-first-subjects: 3/10 known answer.
    let f = fold_activation(
        "V3",
        ActivationMetric::ShareOfFirstSubjects,
        ActivationEvidence {
            exercised: 3,
            total: 10,
        },
    );
    assert_eq!(activation_ppm(&f), 300_000);
    // Exact counterfactual activation — a deterministic renderer
    // knob: re-render 4 of 5 observations → exactly .8, marked exact.
    let f = fold_activation(
        "V4",
        ActivationMetric::ExactCounterfactual,
        ActivationEvidence {
            exercised: 4,
            total: 5,
        },
    );
    assert_eq!(activation_ppm(&f), 800_000);
    assert_eq!(f.get("exact"), Some(&Json::Bool(true)));
    // An executor knob folds the same way (deterministic).
    let f = fold_activation(
        "V3",
        ActivationMetric::ExactCounterfactual,
        ActivationEvidence {
            exercised: 1,
            total: 4,
        },
    );
    assert_eq!(activation_ppm(&f), 250_000);
}

// ── AC-5 (REQ-XD-209) — the inert forecast ───────────────────────────────────

#[test]
fn ac5_inert_forecast() {
    assert_eq!(
        forecast_disposition(ACTIVATION_FLOOR_PPM),
        "admitted{forecast_ok}"
    );
    assert_eq!(
        forecast_disposition(ACTIVATION_FLOOR_PPM - 1),
        "withheld{inert_predicted}"
    );
    // A below-floor level is withheld and retained with its forecast.
    let fold = fold_activation(
        "V3",
        ActivationMetric::ShareOfFirstSubjects,
        ActivationEvidence {
            exercised: 1,
            total: 10,
        },
    );
    let e = forecast_entry("V3", fold);
    assert_eq!(
        e.get("disposition").and_then(Json::as_str),
        Some("withheld{inert_predicted}")
    );
    // The registry retains the withheld level with its forecast.
    let mut extra: BTreeMap<String, Vec<(String, Json)>> = BTreeMap::new();
    extra.insert("V3".to_string(), vec![("forecast".to_string(), e)]);
    let reg = sweep::freeze_registry_as(
        &sweep_registry_record_p4(&mat("ac5-seal"), &extra, Json::Null),
        P4_TAG,
    );
    let v3 = reg
        .get("levels")
        .and_then(|l| l.as_arr_cloned())
        .unwrap()
        .into_iter()
        .find(|l| l.get("level_id").and_then(Json::as_str) == Some("V3"))
        .unwrap();
    assert_eq!(
        v3.get("forecast")
            .and_then(|f| f.get("disposition"))
            .and_then(Json::as_str),
        Some("withheld{inert_predicted}")
    );
}

// ── AC-6 — registry validity, sealing equality, merge refusal ────────────────

#[test]
fn ac6_registry_and_seals() {
    let m = mat("ac2-seal");
    // Seven levels sealed through enumerate→seal; V8/V9 withheld.
    let sealed: Vec<&str> = m
        .levels
        .iter()
        .map(|l| l.sealed.level_id.as_str())
        .collect();
    assert_eq!(sealed, vec!["V1", "V2", "V3", "V4", "V5", "V6", "V7"]);
    // Every sealed level differs from the declared baseline only in
    // its declared paths (check_doc_delta ran inside seal_p4_levels).
    for l in &m.levels {
        assert!(
            !l.delta.is_empty(),
            "{} has an empty delta",
            l.sealed.level_id
        );
    }
    // The registry validates under R6.43's validator; the hash is
    // recorded for R7.2; the identity is the new x2 tag.
    let reg = registry(&m);
    let refusals = sweep::check_registry(&reg);
    assert!(refusals.is_empty(), "{refusals:?}");
    let sha = registry_hash_for_prereg(&reg).unwrap();
    assert_eq!(sha.len(), 64);
    assert!(reg
        .get("exploration_identity")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .starts_with("x2:sha256:"));
    // A level changed after the freeze mints a new identity whose
    // data never merge.
    let mut reg2 = Json::Obj(reg.as_obj_map());
    obj_of(&mut arr_of(obj_of(&mut reg2).get_mut("levels").unwrap())[0])
        .insert("level".into(), Json::str("medium"));
    let reg2 = sweep::freeze_registry_as(&reg2, P4_TAG);
    let a = reg
        .get("exploration_identity")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let b = reg2
        .get("exploration_identity")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    assert_ne!(a, b);
    assert!(matches!(
        sweep::check_merge(&a, &b),
        Err(sweep::SweepRefusal::MergeAcrossIdentities { .. })
    ));
    // The x1 identity stays distinct — this pass's data never merge
    // with the first-pass freeze.
    let x1 = sweep::exploration_identity(&sha);
    assert!(x1.starts_with("x1:"));
    assert!(matches!(
        sweep::check_merge(&a, &x1),
        Err(sweep::SweepRefusal::MergeAcrossIdentities { .. })
    ));
    // Each sealed level's LevelSpec.ref_ is the sealed version_id and
    // opening by override refuses (REQ-XE-124 consumed).
    for l in &m.levels {
        assert_eq!(l.sealed.level_spec.ref_, l.sealed.version_id);
        assert!(matches!(
            sweep::check_level_open(
                &l.sealed.level_spec,
                &sweep::LevelOpen::ByOverrides(vec!["x=1".into()])
            ),
            Err(sweep::SweepRefusal::OpenedByOverride { .. })
        ));
    }
}

// ── helpers (same shape as r6_43's) ──────────────────────────────────────────

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
