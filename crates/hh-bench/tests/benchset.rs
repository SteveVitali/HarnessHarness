//! `benchset` corpus tests (S3.12b; GATE-G2 G2-1) — the committed
//! `fixtures/benchset/stage3_v1` corpus loads as the Stage-3
//! `benchmarkSet`: every suite manifest validates (split hash, L2
//! epoch-seeded dispatch, three-surface tasks), loading is deterministic,
//! the declared strata/retirement labels survive into the adapter inputs,
//! and a corrupted member refuses typed rather than loading partially.

use hh_bench::adapter::BenchmarkAdapter;
use hh_bench::benchset::{Benchset, BenchsetError, BENCHSET_ID};
use hh_bench::grade::GradeRequest;
use hh_ontology::lab::{ContaminationStratum, EnvironmentFamily, SplitLabel};

fn tmp(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-bench-benchset-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// The corpus loads: four strata, validated manifests, stable identities.
#[test]
fn benchset_loads_and_validates() {
    let bs = Benchset::load_default().expect("corpus loads");
    assert_eq!(bs.id, BENCHSET_ID);
    assert_eq!(
        bs.suites.keys().cloned().collect::<Vec<_>>(),
        vec!["a", "c", "d", "e"]
    );

    let a = bs.suite("a").unwrap();
    assert_eq!(a.family(), EnvironmentFamily::CodingTerminal);
    assert_eq!(a.stratum(), ContaminationStratum::FreshTemporal);
    assert!(!a.retired_for_headline());
    assert_eq!(a.tasks_on(SplitLabel::HeldOut).len(), 5);

    // AC-R-2.9.4-11's labels — the stratum-C control is
    // `contaminated_public` and `retired_for_headline`.
    let c = bs.suite("c").unwrap();
    assert_eq!(c.stratum(), ContaminationStratum::ContaminatedPublic);
    assert!(c.retired_for_headline());
    assert_eq!(c.tasks_on(SplitLabel::HeldOut).len(), 5);
    assert_eq!(c.manifest.validity.flawed_task_ids.len(), 1);
    // The flawed member stays in `tasks[]` — flagged, never dropped.
    let flawed = &c.manifest.validity.flawed_task_ids[0];
    assert!(c.manifest.tasks.contains(flawed));

    let d = bs.suite("d").unwrap();
    assert_eq!(d.family(), EnvironmentFamily::StructuredTool);
    assert_eq!(d.stratum(), ContaminationStratum::PublicDated);
    assert_eq!(d.tasks.len(), 3);

    let e = bs.suite("e").unwrap();
    assert_eq!(e.family(), EnvironmentFamily::AdversarialSecurity);
    assert_eq!(e.stratum(), ContaminationStratum::PublicDated);
    assert_eq!(e.tasks.len(), 3);

    // Every manifest's split hash recomputes and the assignment record
    // validates (ADR-0143 L3 — split assignment predates any search).
    for s in bs.suites.values() {
        s.manifest.validate().unwrap();
        s.split_assignment.validate().unwrap();
        assert_eq!(s.manifest.split_hash, s.split_assignment.split_hash);
        for name in &s.parity_subset {
            let t = s.task_named(name).unwrap();
            assert!(t.original_runs.len() >= 3, "{name} parity n ≥ 3");
        }
    }
}

/// Loading is deterministic — identical task/suite ids across loads (the
/// content addresses commit the same bytes).
#[test]
fn benchset_load_is_deterministic() {
    let a = Benchset::load_default().unwrap();
    let b = Benchset::load_default().unwrap();
    for key in ["a", "c", "d", "e"] {
        let (sa, sb) = (a.suite(key).unwrap(), b.suite(key).unwrap());
        assert_eq!(sa.manifest.suite_id, sb.manifest.suite_id);
        assert_eq!(
            sa.manifest.to_json().to_canonical_string(),
            sb.manifest.to_json().to_canonical_string()
        );
        for (ta, tb) in sa.tasks.iter().zip(sb.tasks.iter()) {
            assert_eq!(ta.record().task_id, tb.record().task_id);
            assert_eq!(ta.record(), tb.record());
            assert_eq!(ta.model_io, tb.model_io);
            assert_eq!(ta.original_runs, tb.original_runs);
        }
    }
}

/// A tampered corpus member refuses typed — no partial suite.
#[test]
fn benchset_refusal_matrix() {
    // Unknown member — the corpus schema is closed.
    let dir = tmp("unknown-member");
    std::fs::create_dir_all(dir.join("suite.tb2")).unwrap();
    std::fs::write(
        dir.join("benchset.json"),
        r#"{"schema":"benchset/1","benchset_id":"benchset.stage3.v1","suites":{"a":"suite.tb2"},"bogus":1}"#,
    )
    .unwrap();
    assert!(matches!(
        Benchset::load(&dir),
        Err(BenchsetError::UnknownMember { member, .. }) if member == "bogus"
    ));

    // A wrong benchset_id refuses — the loader never silently re-binds.
    let dir = tmp("wrong-id");
    std::fs::write(
        dir.join("benchset.json"),
        r#"{"schema":"benchset/1","benchset_id":"other","suites":{}}"#,
    )
    .unwrap();
    assert!(matches!(
        Benchset::load(&dir),
        Err(BenchsetError::Inconsistent { .. })
    ));
}

/// The adapter lifecycle over a corpus task — materialize (participant +
/// verifier), expose the visible surface only, apply the recorded
/// submission, grade against the held-out oracle.
#[test]
fn adapter_lifecycle_over_corpus_task() {
    let bs = Benchset::load_default().unwrap();
    let suite = bs.suite("a").unwrap();
    let adapter = &suite.adapter;
    let task = suite.task_named("a-hello").unwrap();
    let root = tmp("lifecycle");

    let participant = adapter
        .materialize(&task.record().task_id, root.to_str().unwrap(), false)
        .unwrap();
    let verifier = adapter
        .materialize(&task.record().task_id, root.to_str().unwrap(), true)
        .unwrap();
    // The held-out oracle lands only on the verifier root.
    assert!(std::path::Path::new(&verifier.root)
        .join("expected.bin")
        .exists());
    assert!(!std::path::Path::new(&participant.root)
        .join("expected.bin")
        .exists());

    let exposed = adapter.expose(&participant, "profile:ref").unwrap();
    assert_eq!(exposed.task_id, task.record().task_id);
    assert!(exposed.instruction.contains("submission.json"));

    // The recorded model_io's effect: the submission bytes land.
    std::fs::write(
        std::path::Path::new(&participant.root).join("submission.json"),
        task.expected(),
    )
    .unwrap();
    let sub = adapter.collect_submission(&participant).unwrap();
    assert!(sub.applied);
    let res = adapter
        .grade(&GradeRequest {
            task: task.record().clone(),
            submission: sub,
            isolation: hh_ontology::lab::VerifierIsolation::Separate,
        })
        .unwrap();
    assert_eq!(res.reward_ppm, 1_000_000, "expected bytes grade full reward");
}
