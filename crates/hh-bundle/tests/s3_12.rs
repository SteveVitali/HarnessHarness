//! S3.12 — AC-R-2.12.1-8: the run-manifest reference closure.
//! `assemble` writes `resolved_dependencies.run_refs{run_id}` — the flat
//! `{dotted.field → ref}` projection of the subject run's `RunManifest`;
//! `check_completeness` re-derives the index from the exported
//! `lifecycle.run.created` payload and compares. A reference the manifest
//! carries but the index lacks fails `manifest_reference_unresolved`; an
//! index entry the manifest does not carry is the same failure
//! (symmetric — CC3).

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_bundle::assemble::{assemble, AssembleInputs};
use hh_bundle::manifest::BundlePolicy;
use hh_bundle::runrefs::{declared_run_refs, manifest_refs};
use hh_bundle::validate::{check_completeness, CheckStatus};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::Store;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hh-bundle-s312-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

struct Fixture {
    store: Store,
    run_id: String,
    policy: BundlePolicy,
    blobs: BTreeMap<String, Vec<u8>>,
    def_addr: String,
}

/// A run whose manifest carries references the closure must index.
fn fixture(tag: &str) -> Fixture {
    let mut store = Store::open(dir(tag)).unwrap();
    let def = Json::obj([("sealed", Json::str("definition"))]);
    let def_bytes = def.to_canonical_string().into_bytes();
    let def_addr = hh_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut manifest = RunManifest::minimal(RunKind::Agent);
    manifest.harness_def_ref = Some(def_addr.clone());
    manifest.task_ref = Some(hh_ledger::manifest::TaskRef {
        task_id: "task:fixture".into(),
        suite_id: "suite:fixture".into(),
        split_label: hh_ontology::lab::SplitLabel::Dev,
    });
    let (run_id, _lease) = store.open_run(manifest, "s312.test").unwrap();
    let mut blobs = BTreeMap::new();
    blobs.insert(def_addr.clone(), def_bytes);
    Fixture {
        store,
        run_id,
        policy: BundlePolicy::default(),
        blobs,
        def_addr,
    }
}

fn inputs<'a>(
    f: &'a Fixture,
    artifact_bytes: &'a dyn Fn(&str) -> Option<Vec<u8>>,
) -> AssembleInputs<'a> {
    AssembleInputs {
        store: &f.store,
        run_id: &f.run_id,
        policy: &f.policy,
        created_at: "2026-01-01T00:00:00.000Z".into(),
        producer: ProvenanceRecord::kernel("s312.test", 0).to_json(),
        contract_identity: Json::Null,
        kernel_version_id: "sha256:kernel".into(),
        instrument_dirty: false,
        artifact_bytes,
        environment: Json::obj([("platform", Json::str("test"))]),
        compiled: None,
        model_snapshots: vec![],
        registry_snapshot_id: None,
        variants: vec![],
        extensions: vec![],
        budget: Json::Null,
        profile: Json::str("none"),
        nondeterminism: vec![],
        participant_class: None,
    }
}

/// S1's `run_refs:*` rows for `run` — the closure check's verdicts.
fn run_ref_rows(
    report: &hh_bundle::validate::BundleValidationReport,
    run: &str,
) -> Vec<hh_bundle::validate::CheckRow> {
    let prefix = format!("run_refs:{run}");
    report
        .stages
        .iter()
        .find(|s| s.stage == 1)
        .expect("S1 present")
        .checks
        .iter()
        .filter(|c| c.check.starts_with(&prefix))
        .cloned()
        .collect()
}

#[test]
fn run_manifest_references_close_in_resolved_dependencies() {
    let f = fixture("closed");
    let get = |a: &str| f.blobs.get(a).cloned();
    let assembled = assemble(&inputs(&f, &get)).unwrap();

    // (i) The index carries every reference the run manifest names.
    let declared = declared_run_refs(&assembled.manifest.resolved_dependencies, &f.run_id);
    let Json::Obj(decl) = &declared else {
        panic!("run_refs[{run}] must be an object", run = f.run_id)
    };
    assert_eq!(
        decl.get("harness_def_ref").and_then(Json::as_str),
        Some(f.def_addr.as_str())
    );
    assert_eq!(
        decl.get("task_ref.task_id").and_then(Json::as_str),
        Some("task:fixture")
    );

    // (ii) `manifest_refs` over the exported `lifecycle.run.created`
    //      payload re-derives the same index — one projection, one
    //      vocabulary.
    let events = f.store.events(&f.run_id).unwrap();
    let created = events
        .iter()
        .find(|e| e.class == "lifecycle.run.created")
        .expect("the run's creation row");
    let rm = RunManifest::from_json(&created.payload).unwrap();
    assert_eq!(manifest_refs(&rm), declared);

    // (iii) `check_completeness` reports `run_refs:{run}` pass and no
    //       `manifest_reference_unresolved` anywhere.
    let report = check_completeness(&assembled.manifest, &assembled.members);
    let rows = run_ref_rows(&report, &f.run_id);
    assert!(
        rows.iter().all(|r| r.status == CheckStatus::Pass),
        "closure rows pass: {rows:?}"
    );
    assert!(
        report
            .stages
            .iter()
            .flat_map(|s| s.checks.iter())
            .all(|c| c.code.as_deref() != Some("manifest_reference_unresolved")),
        "no manifest_reference_unresolved rows"
    );
}

/// Removal-sensitive: drop a declared field — the manifest's reference is
/// unindexed → `manifest_reference_unresolved`; add a field the manifest
/// does not carry — the stale claim fails the same check (CC3 symmetric).
#[test]
fn manifest_reference_unresolved_fails_completeness() {
    let f = fixture("mutated");
    let get = |a: &str| f.blobs.get(a).cloned();
    let mut assembled = assemble(&inputs(&f, &get)).unwrap();

    // Mutate the index — drop `harness_def_ref` and invent `budget` (the
    // manifest carries no `budget` member).
    {
        let Json::Obj(deps) = &mut assembled.manifest.resolved_dependencies else {
            panic!()
        };
        let Json::Obj(refs) = deps.get_mut("run_refs").unwrap() else {
            panic!()
        };
        let Json::Obj(run) = refs.get_mut(&f.run_id).unwrap() else {
            panic!()
        };
        run.remove("harness_def_ref");
        run.insert("budget".into(), Json::str("sha256:invented"));
    }

    let report = check_completeness(&assembled.manifest, &assembled.members);
    let rows = run_ref_rows(&report, &f.run_id);
    let fails: Vec<&hh_bundle::validate::CheckRow> = rows
        .iter()
        .filter(|r| r.status == CheckStatus::Fail)
        .collect();
    assert_eq!(fails.len(), 2, "one fail per direction: {rows:?}");
    assert!(
        fails
            .iter()
            .all(|r| r.code.as_deref() == Some("manifest_reference_unresolved")),
        "typed code, not a string match: {fails:?}"
    );
    assert!(
        fails.iter().any(|r| r.check.ends_with("harness_def_ref")),
        "the unindexed reference fails: {fails:?}"
    );
    assert!(
        fails.iter().any(|r| r.check.ends_with("budget")),
        "the stale claim fails: {fails:?}"
    );
}
