//! R2.17 — DF-S1.24-2's machine cells over the recorded corpus
//! (`fixtures/benchset/stage3_v1`): `task_id` round-trip stability and the
//! recorded-corpus legs of AC-R-2.9.4-*'s import contract. The *live*
//! foreign-manifest import stays environment-gated (R2.21 / HUMAN-H3) —
//! these legs exercise the same checks over the committed corpus, which is
//! the honest offline ceiling (fixture-verified).
//!
//! - `task_id` codec round-trip: every corpus task decodes and re-encodes
//!   to the same `task_id`, and `check_id()` (id == semantic-projection id)
//!   holds for every record — the versioned codec verifies the id on read,
//!   so a drifted `task_id` never decodes.
//! - the imported-vs-annotated partition (CF-450): re-annotating a task —
//!   new `foreign` claims, `validity`/`contamination` audit members,
//!   `provenance`, `ext` — never changes `task_id`; changing a semantic
//!   member does. That *is* the import round-trip: the same task re-imported
//!   under different provenance mints the same id.
//! - the suite-side legs: `SuiteManifest`/`SplitAssignmentRecord` codec
//!   round-trips keep `suite_id`/`split_hash`, and the admission checks
//!   (`manifest.validate` — L2 split-hash + `l2_check`; `record.validate` —
//!   the L1 static half incl. `HeldOutInEnvironment`) pass over the whole
//!   corpus.
//! - the adapter boundary: `expose` preserves `task_id` for every task in
//!   every suite (the member the harness side keys runs on).

use hh_bench::benchset::Benchset;
use hh_bench::BenchmarkAdapter;
use hh_lab::bench::{split_hash, SuiteManifest, TaskRecord};
use hh_provenance::{HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::Json;

/// Every corpus task's `task_id` survives the versioned codec round-trip
/// and equals the re-derived semantic-projection id (R-2.9.4's `task_id`
/// determinism — a determinism contract, not a convention; CC9).
#[test]
fn task_id_codec_round_trip_over_corpus() {
    let bs = Benchset::load_default().unwrap();
    let mut count = 0usize;
    for suite in bs.suites.values() {
        for t in &suite.tasks {
            let r = t.record();
            assert!(r.check_id(), "{} task_id != semantic_id", r.task_id);
            let decoded = TaskRecord::from_json(&r.to_json()).unwrap();
            assert_eq!(decoded.task_id, r.task_id, "{} task_id drifted", r.task_id);
            // The canonical form is hash-addressed (`Text::to_json` never
            // emits raw `content`) — the round-trip identity is the
            // canonical JSON itself + the semantic id, not raw-content
            // equality.
            assert!(decoded.check_id(), "{} decoded id unverified", r.task_id);
            assert_eq!(
                decoded.to_json(),
                r.to_json(),
                "{} canonical drift",
                r.task_id
            );
            // Re-deriving from the semantic projection is stable.
            assert_eq!(r.semantic_id(), r.task_id);
            count += 1;
        }
        // `manifest.tasks[]` names the same ids the records carry — the
        // split map keys them.
        for task_id in &suite.manifest.tasks {
            assert!(suite.task(task_id).is_some(), "{task_id} not in suite");
        }
    }
    assert!(count > 0);
}

/// A tampered `task_id` never verifies — the importer's `check_id`
/// computes the semantic-projection id and compares, never trusts the
/// submitted member (a forged id decodes under the strict codec but fails
/// the identity check at admission).
#[test]
fn task_id_tamper_fails_check_id() {
    let bs = Benchset::load_default().unwrap();
    let t = bs.suite("a").unwrap().task_named("a-hello").unwrap();
    let mut j = t.record().to_json();
    if let Json::Obj(ref mut m) = j {
        m.insert("task_id".into(), Json::str("sha256:forged"));
    }
    let forged = TaskRecord::from_json(&j).unwrap();
    assert_eq!(forged.task_id, "sha256:forged");
    assert!(!forged.check_id());
    assert_ne!(forged.semantic_id(), forged.task_id);
}

/// CF-450 — the imported-vs-annotated partition: provenance claims, the
/// `foreign` identity, `validity`/`contamination` audit members and `ext`
/// never feed `task_id`. Re-importing the same task under different
/// annotations mints the same id (the import round-trip the live leg will
/// exercise over real manifests at R2.21).
#[test]
fn task_id_stable_under_annotation_variance() {
    let bs = Benchset::load_default().unwrap();
    let mut count = 0usize;
    for suite in bs.suites.values() {
        for t in &suite.tasks {
            let r = t.record();
            let mut v = r.clone();
            v.foreign.name = format!("{}-annotated", r.foreign.name);
            v.foreign.version = format!("{}-reimport", r.foreign.version);
            v.instrument
                .validity
                .flawed_task_ids
                .push("task:audit-note".into());
            v.instrument.validity.audit_ref = Some("audit:reimport".into());
            v.instrument.contamination.first_public_at = Some("2099-01-01".into());
            v.provenance = ProvenanceRecord::minted(
                Origin::human("reimporter", HumanRole::Author),
                PersistenceScope::Definition,
                999_999,
            );
            v.ext.insert("audit_note".into(), Json::str("re-imported"));
            assert_eq!(
                v.semantic_id(),
                r.semantic_id(),
                "{} semantic id moved under annotation variance",
                r.task_id
            );
            count += 1;
        }
    }
    assert!(count > 0);

    // The contract is not vacuous — a semantic member does move the id.
    let t = bs.suite("a").unwrap().task_named("a-hello").unwrap();
    let mut v = t.record().clone();
    v.tags.push("extra-tag".into());
    assert_ne!(v.semantic_id(), t.record().semantic_id());
}

/// The suite-side records round-trip identically: `SuiteManifest` keeps
/// `suite_id`/`split_hash`/`tasks`/`split_map`; `SplitAssignmentRecord`
/// keeps the committed `split_hash` (which recomputes — `split_hash()` over
/// the decoded map).
#[test]
fn suite_records_round_trip_over_corpus() {
    let bs = Benchset::load_default().unwrap();
    for suite in bs.suites.values() {
        let m = SuiteManifest::from_json(&suite.manifest.to_json()).unwrap();
        assert_eq!(m.suite_id, suite.manifest.suite_id);
        assert_eq!(m.split_hash, suite.manifest.split_hash);
        assert_eq!(m.tasks, suite.manifest.tasks);
        assert_eq!(m.split_map, suite.manifest.split_map);
        assert_eq!(m.split_hash, split_hash(&m.split_map));

        let sa = &suite.split_assignment;
        let decoded = hh_lab::bench::SplitAssignmentRecord::from_json(&sa.to_json()).unwrap();
        assert_eq!(decoded, *sa);
        assert_eq!(decoded.split_hash, split_hash(&decoded.splits));
    }
}

/// The recorded-corpus admission legs — L1 (`record.validate`: the
/// three-surface partition + `HeldOutInEnvironment`) and L2
/// (`manifest.validate`: split-hash + `validity.l2_check`) hold for every
/// task and suite in the corpus.
#[test]
fn corpus_admission_checks_hold() {
    let bs = Benchset::load_default().unwrap();
    for suite in bs.suites.values() {
        suite.manifest.validate().unwrap();
        suite.split_assignment.validate().unwrap();
        for t in &suite.tasks {
            t.record().validate().unwrap();
        }
    }
}

/// The adapter boundary preserves `task_id` for every corpus task — the
/// member the harness keys runs on survives materialize + expose.
#[test]
fn expose_preserves_task_id_over_corpus() {
    let bs = Benchset::load_default().unwrap();
    let root = std::env::temp_dir().join(format!("hh-bench-r2-17-expose-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let mut count = 0usize;
    for (key, suite) in &bs.suites {
        for t in &suite.tasks {
            let dir = root.join(format!("{key}-{}", t.record().task_id));
            let h = suite
                .adapter
                .materialize(&t.record().task_id, dir.to_str().unwrap(), false)
                .unwrap();
            let ex = suite.adapter.expose(&h, "profile:ref").unwrap();
            assert_eq!(ex.task_id, t.record().task_id);
            count += 1;
        }
    }
    assert!(count > 0);
    let _ = std::fs::remove_dir_all(&root);
}
