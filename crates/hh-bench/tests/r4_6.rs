//! R4.6 — DF-S1.24-2's live cell: a real upstream foreign-benchmark
//! manifest (the Harbor Index `registry.json` — dataset entries x task
//! rows carrying pinned `git_commit_id` digests) imported through the
//! canonical importer (`adapters::fixture_task` — the same constructor
//! the committed corpus's loader uses; CC1 — never a second spelling),
//! with L1 (`record.validate` — the three-surface partition +
//! `HeldOutInEnvironment`) and L2 (`manifest.validate` — `split_hash` +
//! the epoch-seeded dispatch + `split_assignment.validate`) admission
//! exercised, and `task_id` stability verified through import
//! round-trips.
//!
//! Offline cells run over the committed, pinned subset
//! (`fixtures/live/r4_6/harbor-registry-subset.json` — a byte-identical
//! slice of the upstream manifest at the recorded commit + sha256;
//! `scripts/fetch-r4_6-corpus.sh --check` reproduces it from the pin).
//! The live cell re-fetches the full upstream manifest at the same pin
//! (the leg sha256-verifies it) and imports through the same path —
//! `#[ignore]` outside `scripts/live-verify.sh bench-manifest-live`
//! (ADR-0363).
//!
//! - `task_id` is computed, never trusted: `check_id()` over every
//!   imported record, and the same row re-imported mints the same id
//!   (the import round-trip the cell must hold).
//! - the imported-vs-annotated partition (CF-450): re-annotating an
//!   imported task — `foreign` claims, audit members, `provenance`,
//!   `ext` — never changes `task_id`; changing a semantic member does.
//! - a row without its `git_commit_id` pin refuses (`NoPin`) — the
//!   importer never fabricates a foreign digest.

use std::collections::BTreeMap;

use hharness_bench::adapters::{fixture_task, FixtureTask};
use hharness_lab::bench::{
    split_hash, ForeignTaskId, SplitAssignmentRecord, SuiteManifest, SuiteValidityRecord,
};
use hharness_lab::model::ForeignRef;
use hharness_ontology::lab::{
    ContaminationStratum, EnvironmentFamily, SplitLabel, VerifierIsolation,
};
use hharness_provenance::ProvenanceRecord;
use hharness_wire::Json;

/// The committed pinned subset — the offline arm of the leg.
const SUBSET: &str = include_str!("../fixtures/live/r4_6/harbor-registry-subset.json");

/// The Harbor-Index adapter this import binds (the same `adapter_g` the
/// committed `suite.harbor_index` smoke suite declares).
const ADAPTER: &str = "adapter_g";
/// The family the Harbor Index's coding tasks run under.
const FAMILY: EnvironmentFamily = EnvironmentFamily::CodingTerminal;
/// Harbor rows are dated public material.
const STRATUM: ContaminationStratum = ContaminationStratum::PublicDated;
/// Imported rows hold their data verifier-side only.
const SPLIT: SplitLabel = SplitLabel::HeldOut;

/// A row's typed import refusal — never a warning.
#[derive(Debug, PartialEq)]
enum ImportError {
    /// A required member was absent or mis-typed.
    Malformed(&'static str),
    /// The row carries no `git_commit_id` — the importer refuses rather
    /// than fabricate a foreign digest.
    NoPin,
    /// The imported record failed L1 admission.
    L1(hharness_lab::bench::TaskError),
}

fn member<'a>(row: &'a Json, k: &'static str) -> Result<&'a Json, ImportError> {
    row.get(k).ok_or(ImportError::Malformed(k))
}
fn member_str<'a>(row: &'a Json, k: &'static str) -> Result<&'a str, ImportError> {
    member(row, k)?.as_str().ok_or(ImportError::Malformed(k))
}

/// Import one upstream manifest task row → `TaskRecord` (L1-checked).
/// Deterministic: the same row always mints the same `task_id`.
fn import_row(entry: &str, row: &Json) -> Result<FixtureTask, ImportError> {
    let name = member_str(row, "name")?;
    let path = member_str(row, "path")?;
    let git_url = member_str(row, "git_url")?;
    let commit = member_str(row, "git_commit_id")?;
    if commit.is_empty() {
        return Err(ImportError::NoPin);
    }
    let foreign_name = format!("harbor/{entry}/{name}");
    // The visible instruction is synthesized deterministically from the
    // row — the upstream manifest carries task *metadata*, not prompt
    // text; the canonical row bytes stand in for the held-out oracle
    // material (verifier-side, never delivered).
    let instruction = format!("{foreign_name}: complete the pinned task at {path}");
    let expected = row.to_canonical_string().into_bytes();
    let mut ft = fixture_task(
        ADAPTER,
        FAMILY,
        &foreign_name,
        SPLIT,
        STRATUM,
        &instruction,
        &expected,
        Some(VerifierIsolation::Separate),
        None,
    );
    // The foreign member carries the real upstream claims — the row's
    // own identity + pin. `foreign` is annotation-side (CF-450): it
    // never feeds `task_id`, which the next line re-mints anyway.
    ft.record.foreign = ForeignTaskId {
        name: name.into(),
        version: commit.into(),
        source_ref: format!("{git_url}#{path}"),
        digest_claim: ForeignRef {
            system: "git-commit".into(),
            digest: commit.into(),
            label: Some(path.into()),
            provenance: ProvenanceRecord::kernel("hh-bench/harbor-import", 0),
        },
    };
    ft.record.task_id = ft.record.semantic_id();
    ft.record.validate().map_err(ImportError::L1)?; // L1
    Ok(ft)
}

/// The suite-side records for an imported manifest entry — the same
/// construction `benchset::load_suite` derives (split map, split hash,
/// epoch-seeded L2 dispatch), bound to `adapter_g`.
fn import_suite_records(
    entry: &str,
    commit: &str,
    url: &str,
    tasks: &[FixtureTask],
) -> (SuiteManifest, SplitAssignmentRecord) {
    let task_ids: Vec<String> = tasks.iter().map(|t| t.record.task_id.clone()).collect();
    let split_map: BTreeMap<String, SplitLabel> = tasks
        .iter()
        .map(|t| (t.record.task_id.clone(), t.record.split_label))
        .collect();
    let prov = || ProvenanceRecord::kernel("hh-bench/harbor-import", 0);
    let mut manifest = SuiteManifest {
        suite_id: String::new(),
        foreign: ForeignTaskId {
            name: entry.into(),
            version: commit.into(),
            source_ref: url.into(),
            digest_claim: ForeignRef {
                system: "git-commit".into(),
                digest: commit.into(),
                label: Some(format!("harbor/{entry}")),
                provenance: prov(),
            },
        },
        primary_family: FAMILY,
        tasks: task_ids,
        split_map,
        split_hash: String::new(),
        validity: SuiteValidityRecord {
            audit_ref: format!("audit://r4_6/harbor/{entry}"),
            audited_at: None,
            epoch_seeded: true,
            dispatch_ref: Some(format!("dispatch://r4_6/harbor/{entry}")),
            flawed_task_ids: vec![],
            noise_ceiling: None,
            // A manifest-metadata import is provisional, never headline —
            // the same standing label `suite.harbor_index` carries.
            retired_for_headline: true,
            reason: Some("provisional live-import leg — not a headline stratum".into()),
        },
        contamination_default: STRATUM,
        adapter_ref: ADAPTER.into(),
        parity_report: None,
        provenance: prov(),
    };
    manifest.split_hash = split_hash(&manifest.split_map);
    manifest.suite_id = manifest.suite_id();
    let split_assignment = SplitAssignmentRecord {
        suite_id: manifest.suite_id.clone(),
        rule: "hash_of_task_id".into(),
        seed: "0".into(),
        splits: manifest.split_map.clone(),
        split_hash: manifest.split_hash.clone(),
        registered_at: 0,
    };
    (manifest, split_assignment)
}

/// The committed subset's `entry` member (the slice is `entries[0]`
/// truncated — same shape as a manifest row entry).
fn subset_entry() -> (Json, Json) {
    let subset = hharness_wire::parse(SUBSET).expect("committed subset parses");
    let entry = subset.get("entry").expect("subset.entry").clone();
    (subset.get("upstream").unwrap().clone(), entry)
}

fn entry_tasks(entry: &Json) -> &[Json] {
    match entry.get("tasks") {
        Some(Json::Arr(t)) => t,
        _ => &[],
    }
}

fn import_entry(
    upstream_commit: &str,
    upstream_url: &str,
    entry: &Json,
) -> (SuiteManifest, SplitAssignmentRecord, Vec<FixtureTask>) {
    let entry_name = entry
        .get("name")
        .and_then(Json::as_str)
        .expect("entry.name");
    let tasks: Vec<FixtureTask> = entry_tasks(entry)
        .iter()
        .map(|r| import_row(entry_name, r).expect("row imports"))
        .collect();
    let (m, s) = import_suite_records(entry_name, upstream_commit, upstream_url, &tasks);
    (m, s, tasks)
}

/// Offline leg — the committed pinned subset imports cleanly: L1
/// (`record.validate`) per task, L2 (`manifest.validate` +
/// `split_assignment.validate`) at the suite, `check_id` + codec
/// round-trip per record (fixture-verified — AC: corpus subset
/// committed + offline re-run green).
#[test]
fn fixture_subset_imports_with_l1_l2_admission() {
    let (upstream, entry) = subset_entry();
    assert_eq!(
        upstream.get("commit").and_then(Json::as_str),
        Some("dd8815755a61e849ad0f146ae85eb4aaff4fe61f"),
        "committed subset's recorded pin"
    );
    assert!(upstream
        .get("source_sha256")
        .and_then(Json::as_str)
        .is_some_and(|s| s.len() == 64));
    let (manifest, split_assignment, tasks) = import_entry(
        upstream.get("commit").and_then(Json::as_str).unwrap(),
        upstream.get("url").and_then(Json::as_str).unwrap(),
        &entry,
    );
    assert_eq!(tasks.len(), entry_tasks(&entry).len());
    for t in &tasks {
        assert!(
            t.record.check_id(),
            "{} id != semantic_id",
            t.record.task_id
        );
        // L1 landed inside `import_row`; the codec round-trip keeps it.
        let decoded = hharness_lab::bench::TaskRecord::from_json(&t.record.to_json()).unwrap();
        assert_eq!(decoded.task_id, t.record.task_id);
        assert!(decoded.check_id());
        decoded.validate().unwrap();
    }
    // L2 — split hash + the epoch-seeded dispatch.
    manifest.validate().unwrap();
    split_assignment.validate().unwrap();
    assert_eq!(manifest.split_hash, split_hash(&manifest.split_map));
    assert_eq!(manifest.tasks.len(), tasks.len());
    // The suite's own ids round-trip through the versioned codecs.
    let m2 = SuiteManifest::from_json(&manifest.to_json()).unwrap();
    assert_eq!(m2.suite_id, manifest.suite_id);
    let s2 = SplitAssignmentRecord::from_json(&split_assignment.to_json()).unwrap();
    assert_eq!(s2, split_assignment);
}

/// The import round-trip the live cell must hold: the same upstream row
/// imported twice mints the same `task_id`; annotation variance
/// (re-stamped `foreign`, audit members, `provenance`, `ext`) never
/// moves it; a semantic member does.
#[test]
fn task_id_stable_through_import_round_trips() {
    let (_, entry) = subset_entry();
    let entry_name = entry.get("name").and_then(Json::as_str).unwrap();
    let mut count = 0;
    for row in entry_tasks(&entry) {
        let a = import_row(entry_name, row).unwrap();
        let b = import_row(entry_name, row).unwrap();
        assert_eq!(a.record.task_id, b.record.task_id, "re-import drifted");
        // Re-annotated re-import: fresh foreign claims + audit notes —
        // the imported-vs-annotated partition keeps the id.
        let mut c = a.record.clone();
        c.foreign.version = format!("{}-reimport", a.record.foreign.version);
        c.foreign.digest_claim.label = Some("re-annotated".into());
        c.instrument.validity.audit_ref = Some("audit:reimport".into());
        c.instrument
            .validity
            .flawed_task_ids
            .push("task:audit-note".into());
        c.provenance = ProvenanceRecord::kernel("hh-bench/harbor-import/reimport", 1);
        c.ext.insert("audit_note".into(), Json::str("re-imported"));
        assert_eq!(
            c.semantic_id(),
            a.record.semantic_id(),
            "annotation variance moved the id"
        );
        count += 1;
    }
    assert!(count > 0);
    // Not vacuous — a semantic member change mints a different id.
    let row = &entry_tasks(&entry)[0];
    let mut v = import_row(entry_name, row).unwrap().record;
    let orig = v.semantic_id();
    v.tags.push("extra-tag".into());
    assert_ne!(v.semantic_id(), orig);
}

/// A forged `task_id` never verifies (the importer computes, never
/// trusts the submitted member), and a manifest row without its
/// `git_commit_id` pin refuses rather than minting a fabricated
/// foreign digest.
#[test]
fn forged_id_and_unpinned_row_refuse() {
    let (_, entry) = subset_entry();
    let entry_name = entry.get("name").and_then(Json::as_str).unwrap();
    let row = &entry_tasks(&entry)[0];
    let r = import_row(entry_name, row).unwrap().record;
    let mut j = r.to_json();
    if let Json::Obj(ref mut m) = j {
        m.insert("task_id".into(), Json::str("sha256:forged"));
    }
    let forged = hharness_lab::bench::TaskRecord::from_json(&j).unwrap();
    assert!(!forged.check_id());
    let mut unpinned = row.clone();
    if let Json::Obj(ref mut m) = unpinned {
        m.remove("git_commit_id");
    }
    assert_eq!(
        import_row(entry_name, &unpinned).unwrap_err(),
        ImportError::Malformed("git_commit_id")
    );
    let mut empty = row.clone();
    if let Json::Obj(ref mut m) = empty {
        m.insert("git_commit_id".into(), Json::str(""));
    }
    assert_eq!(
        import_row(entry_name, &empty).unwrap_err(),
        ImportError::NoPin
    );
    // Two distinct upstream rows mint distinct ids.
    let a = import_row(entry_name, row).unwrap();
    let b = import_row(entry_name, &entry_tasks(&entry)[1]).unwrap();
    assert_ne!(a.record.task_id, b.record.task_id);
}

/// The live leg — the full upstream manifest at the committed pin,
/// fetched + sha256-verified by `scripts/live-verify.sh
/// bench-manifest-live` into `HH_FOREIGN_MANIFEST`, imported through
/// the same path the offline subset takes. Bounded by
/// `HH_FOREIGN_IMPORT_MAX` (default 64 tasks) — the cell exercises
/// admission + identity stability over a real manifest, not bulk
/// ingestion.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh bench-manifest-live"]
fn r46_live_manifest_import() {
    let path = std::env::var("HH_FOREIGN_MANIFEST")
        .expect("HH_FOREIGN_MANIFEST — the leg sets it to the verified fetch");
    let text = std::fs::read_to_string(&path).expect("manifest reads");
    let manifest_json = hharness_wire::parse(&text).expect("upstream manifest parses");
    let entries = match &manifest_json {
        Json::Arr(a) => a,
        _ => panic!("upstream registry.json is not an array"),
    };
    let max: usize = std::env::var("HH_FOREIGN_IMPORT_MAX")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(64);
    let (upstream, _) = subset_entry();
    let commit = upstream
        .get("commit")
        .and_then(Json::as_str)
        .expect("subset.upstream.commit");
    let url = upstream
        .get("url")
        .and_then(Json::as_str)
        .expect("subset.upstream.url");
    let mut imported = 0usize;
    let mut refused = 0usize;
    let mut entry_count = 0usize;
    let mut suite_l2 = 0usize;
    for e in entries {
        if imported >= max {
            break;
        }
        let name = e.get("name").and_then(Json::as_str).unwrap_or("unnamed");
        let mut pairs: Vec<(Json, FixtureTask)> = Vec::new();
        for r in entry_tasks(e).iter().take(max.saturating_sub(imported)) {
            match import_row(name, r) {
                Ok(t) => pairs.push((r.clone(), t)),
                Err(_) => refused += 1,
            }
        }
        if pairs.is_empty() {
            continue;
        }
        // L1 per record + the identity check; then the suite records.
        for (_, t) in &pairs {
            assert!(t.record.check_id());
            let decoded = hharness_lab::bench::TaskRecord::from_json(&t.record.to_json()).unwrap();
            assert_eq!(decoded.task_id, t.record.task_id);
            decoded.validate().unwrap();
        }
        let tasks: Vec<FixtureTask> = pairs.iter().map(|(_, t)| t.clone()).collect();
        let (m, s) = import_suite_records(name, commit, url, &tasks);
        m.validate().unwrap(); // L2
        s.validate().unwrap();
        suite_l2 += 1;
        // The round-trip: re-import the same rows, ids must be identical.
        for (r, t) in &pairs {
            let again = import_row(name, r).unwrap();
            assert_eq!(again.record.task_id, t.record.task_id);
        }
        imported += pairs.len();
        entry_count += 1;
    }
    assert!(imported > 0, "no importable rows in the upstream manifest");
    println!(
        "BENCH_IMPORT entries={} tasks={} refused={} l1=ok l2={} ids_stable=ok",
        entry_count, imported, refused, suite_l2
    );
}
