//! `hh-bundle` S6.4 coverage — R-2.9.8 §2.1 (§5h.8; ADR-0325;
//! AC-R-2.9.8-{1,2}): the `training_export/1` lowering target —
//! `export_training` projects a real run bundle's ledger pages into
//! the typed `TrainingExport` + `samples.jsonl` +
//! `exposure_record.json` + `manifest.json` + `loss_report.json`
//! artefact; the artefact id is content-derived (same inputs ⇒ same
//! id); `HeldOutInExport`/`ReaderViolation` ride the typed
//! `CoevolutionRefused` boundary.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_bundle::assemble::{assemble, AssembleInputs};
use hh_bundle::export::export_training;
use hh_bundle::manifest::BundlePolicy;
use hh_lab::coevolution::{SampleUnit, TrainingExportCtx, TrainingExportPolicy};
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::ids::ROOT_EVENT;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-bundle-s64-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn ev(id: &str, class: &str, task: &str, payload: Json) -> Event {
    let mut payload = payload;
    if let Json::Obj(ref mut m) = payload {
        m.insert("task_ref".into(), Json::str(task));
    }
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope::default(),
        parent_event_id: ROOT_EVENT.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("kernel:test", 0)),
        content_kind: None,
        payload,
    }
}

struct Rig {
    _root: PathBuf,
    store: Store,
    run: String,
    _lease: Lease,
    policy: BundlePolicy,
    def_addr: String,
    def_bytes: Vec<u8>,
}

/// A subject run with two `model.call.completed` rows (one per task)
/// + one `measurement.oracle.metric.emitted` reward.
fn run_rig(tag: &str) -> Rig {
    let mut store = Store::open(dir(tag)).unwrap();
    let def_bytes = Json::obj([("sealed", Json::str("definition"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hh_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    let (run, lease) = store.open_run(m, "s64.test").unwrap();
    store
        .append(
            &run,
            &lease,
            vec![
                ev(
                    "ev:call-1",
                    "model.call.completed",
                    "task:a",
                    Json::obj([
                        ("request_plan_hash", Json::str("sha256:req-1")),
                        ("snapshot_id", Json::str("snap:base-1")),
                    ]),
                ),
                ev(
                    "ev:reward-1",
                    "measurement.metric.emitted",
                    "task:a",
                    Json::obj([(
                        "reward",
                        Json::obj([
                            ("source", Json::str("oracle_metric")),
                            ("oracle_ref", Json::str("oracle:task_success")),
                            ("value", Json::Int(1)),
                            ("unit", Json::str("task_success")),
                        ]),
                    )]),
                ),
                ev(
                    "ev:call-2",
                    "model.call.completed",
                    "task:b",
                    Json::obj([
                        ("request_plan_hash", Json::str("sha256:req-2")),
                        ("snapshot_id", Json::str("snap:base-1")),
                    ]),
                ),
            ],
        )
        .unwrap();
    Rig {
        _root: dir(tag),
        store,
        run,
        _lease: lease,
        policy: BundlePolicy::default(),
        def_addr,
        def_bytes,
    }
}

fn inputs<'a>(rig: &'a Rig) -> AssembleInputs<'a> {
    let get = |a: &str| (a == rig.def_addr).then(|| rig.def_bytes.clone());
    let ab: &'a dyn Fn(&str) -> Option<Vec<u8>> = Box::leak(Box::new(get));
    AssembleInputs {
        store: &rig.store,
        run_id: &rig.run,
        policy: &rig.policy,
        created_at: TS.into(),
        producer: ProvenanceRecord::kernel("s64.test", 0).to_json(),
        contract_identity: Json::Null,
        kernel_version_id: "sha256:kernel".into(),
        instrument_dirty: false,
        artifact_bytes: ab,
        environment: Json::obj([("platform", Json::str("test"))]),
        compiled: None,
        model_snapshots: vec![],
        registry_snapshot_id: None,
        variants: vec![],
        extensions: vec![],
        budget: Json::Null,
        profile: Json::str("none"),
        profile_refs: vec![],
        images: vec![],
        nondeterminism: vec![],
        participant_class: None,
    }
}

fn policy() -> TrainingExportPolicy {
    TrainingExportPolicy {
        sample_unit: SampleUnit::Turn,
        include: vec!["model_call".into(), "reward".into()],
        readers: vec!["lab:trainer".into()],
        redaction: None,
        reward_sources: vec!["oracle_metric".into()],
        split_labels: vec!["train".into()],
        snapshot_filter: None,
    }
}

fn ctx() -> TrainingExportCtx {
    TrainingExportCtx {
        task_splits: BTreeMap::from([
            ("task:a".to_string(), "train".to_string()),
            ("task:b".to_string(), "train".to_string()),
        ]),
        corpus_readers: vec!["lab:trainer".into()],
        harness_constraints: Json::obj([("profile", Json::str("minimal:0"))]),
        provenance: None,
    }
}

fn decoded(rig: &Rig) -> hh_bundle::codec::Decoded {
    let a = assemble(&inputs(rig)).unwrap();
    hh_bundle::codec::Decoded {
        manifest: a.manifest,
        members: a.members,
    }
}

/// The `training_export/1` artefact lands the typed files; the export
/// id is content-derived (same inputs ⇒ same artefact — E-1); the
/// loss report names the target; the granularity ceiling is
/// `model_call` (token-level data is never fabricated).
#[test]
fn training_export_lowers_the_bundle_deterministically() {
    let rig = run_rig("training");
    let a = decoded(&rig);
    let out = export_training(&a, &policy(), &ctx()).unwrap();
    let out2 = export_training(&a, &policy(), &ctx()).unwrap();
    assert_eq!(out.artefact, out2.artefact, "same inputs ⇒ same artefact");
    assert_eq!(out.granularity_ceiling, "model_call");
    assert_eq!(out.delivered_runs, a.manifest.subject.run_ids);

    // The artefact's file set.
    for f in [
        "training_export.json",
        "samples.jsonl",
        "exposure_record.json",
        "manifest.json",
        "loss_report.json",
    ] {
        assert!(out.files.contains_key(f), "missing artefact file {f}");
    }

    // The typed record round-trips: two samples; the reward binds the
    // task:a sample.
    let rec =
        hh_wire::json::parse(std::str::from_utf8(&out.files["training_export.json"]).unwrap())
            .unwrap();
    let samples = rec
        .get("samples")
        .and_then(|s| match s {
            Json::Arr(a) => Some(a),
            _ => None,
        })
        .expect("samples array");
    assert_eq!(samples.len(), 2);
    assert_eq!(
        rec.get("maturity").and_then(Json::as_str),
        Some("research-grade")
    );

    // samples.jsonl — canonical one-sample-per-line.
    let lines = std::str::from_utf8(&out.files["samples.jsonl"])
        .unwrap()
        .lines()
        .count();
    assert_eq!(lines, 2);

    // The loss report names the target.
    assert_eq!(
        out.loss_report.get("target").and_then(Json::as_str),
        Some("training_export")
    );
}

/// `HeldOutInExport` and `ReaderViolation` ride the typed
/// `CoevolutionRefused` boundary (the closed code verbatim — AC-R-2.9.8-2).
#[test]
fn training_export_refusals_ride_the_typed_boundary() {
    let rig = run_rig("refusal");
    let a = decoded(&rig);
    let err = |r: Result<hh_bundle::export::ExportOutcome, hh_bundle::error::BundleError>| match r {
        Err(e) => e,
        Ok(_) => panic!("expected a typed refusal"),
    };

    // A held-out task in the ctx refuses before a byte projects.
    let mut c = ctx();
    c.task_splits
        .insert("task:a".to_string(), "held_out".to_string());
    let e = err(export_training(&a, &policy(), &c));
    assert_eq!(format!("{e}"), "coevolution_refused: HeldOutInExport");

    // An undeclared reader refuses.
    let mut p = policy();
    p.readers = vec!["lab:other".into()];
    let e = err(export_training(&a, &p, &ctx()));
    assert_eq!(format!("{e}"), "coevolution_refused: ReaderViolation");
}
