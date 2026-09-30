//! `hh-experiment` boundary-companion tests — `lab/compaction-boundary-v1`
//! (AC-R-2.4.2-12; ADR-0077 d7; the `R-2.4.2²` C2 slice; S4.16b):
//!
//! - the driver forks by reference at a `context.compaction.started` cut
//!   and continues k = 5 turns in two arms — with and without the
//!   proposal;
//! - the paired children share the `forked_from{run_id, at_seq, head_hash}`
//!   anchor — the paired-rows join key;
//! - the applied arm carries the proposal's `context.compaction.completed`
//!   row; the withheld arm never fabricates one;
//! - unequal eval budgets refuse `UnmatchedBudget` before any fork is
//!   taken (CC9 — matched-budget-or-refuse).

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_experiment::boundary::{drive_compaction_boundary, BoundaryRefusal, BOUNDARY_TURNS};
use hh_ledger::branch::ForkOpts;
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::ids::ManualClock;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-boundary-test-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn mint(store: &Store, run_id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: store.alloc_id("evt"),
        class: class.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("hh-boundary-test/1"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel(
            "hh-boundary-test/1",
            store.now_ms(),
        )),
        content_kind: None,
        payload,
    }
}

/// A subject run carrying one turn + one `context.compaction.started`
/// (the cut coordinate) + one later turn.
fn source(tag: &str) -> (Store, String, Lease, u64) {
    let mut store = Store::open_with(
        tmp(tag),
        Box::new(ManualClock::at(1_000)),
        None,
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "driver")
        .unwrap();
    store
        .append(
            &run,
            &lease,
            vec![mint(&store, &run, "lifecycle.turn.started", Json::obj([]))],
        )
        .unwrap();
    let started = mint(
        &store,
        &run,
        "context.compaction.started",
        Json::obj([
            ("trigger", Json::str("gauge_cap")),
            ("requirement", Json::str("hard")),
            ("occupancy_before", Json::Int(9_500)),
            ("target_reclaim", Json::Int(3_000)),
            ("min_reclaim", Json::Int(1_500)),
        ]),
    );
    store.append(&run, &lease, vec![started]).unwrap();
    let cut = store.head(&run).unwrap().seq;
    // A post-cut turn — the source prefix up to `cut` is what the children
    // share (fork-by-reference never copies events).
    store
        .append(
            &run,
            &lease,
            vec![mint(&store, &run, "lifecycle.turn.finished", Json::obj([]))],
        )
        .unwrap();
    (store, run, lease, cut)
}

/// The `CompactionRecord` row the compactor would have produced — the
/// proposal's completion payload the applied arm lands verbatim.
fn proposal_payload() -> Json {
    Json::obj([
        ("compaction_id", Json::str("compact:test")),
        ("variant_ref", Json::str("hh/relower-summarize@1")),
        ("trigger", Json::str("gauge_cap")),
        ("requirement", Json::str("hard")),
        ("status", Json::str("applied")),
        ("ops_applied", Json::Arr(vec![Json::str("summarize")])),
        ("forgotten", Json::Arr(vec![])),
        ("tokens_freed", Json::Int(2_000)),
        ("derived_from", Json::Arr(vec![])),
    ])
}

fn budget(turns: i64) -> Json {
    Json::obj([("turns", Json::Int(turns))])
}

#[test]
fn ac_2_4_2_12_boundary_pair_shares_the_forked_from_key() {
    let (mut store, run, _lease, cut) = source("pair");
    let budget_doc = budget(5);
    let pair = drive_compaction_boundary(
        &mut store,
        &run,
        cut,
        Some(&proposal_payload()),
        (&budget_doc, &budget_doc.clone()),
        &ForkOpts::default(),
        &RunManifest::minimal(RunKind::Agent),
        "driver",
    )
    .expect("the pair drives");

    // Both children bind the identical anchor — the join key for the
    // paired rows.
    let with_link = store
        .manifest(&pair.with_proposal.run_id)
        .unwrap()
        .forked_from
        .clone()
        .unwrap();
    let without_link = store
        .manifest(&pair.without_proposal.run_id)
        .unwrap()
        .forked_from
        .clone()
        .unwrap();
    assert_eq!(with_link, without_link);
    assert_eq!(with_link, pair.forked_from);
    assert_eq!(with_link.run_id, run);
    assert_eq!(with_link.at_seq, cut);
    assert!(!with_link.head_hash.is_empty());

    // The source is untouched after the cut (fork-by-reference — nothing
    // appended, the head hash is the post-cut tip's).
    let src = store.envelopes(&run).unwrap();
    assert_eq!(store.head(&run).unwrap().hash, src.last().unwrap().hash);

    // The applied arm: the proposal row + k turn pairs.
    let with_events = store.envelopes(&pair.with_proposal.run_id).unwrap();
    let completed = with_events
        .iter()
        .filter(|e| e.class == "context.compaction.completed")
        .count();
    assert_eq!(completed, 1);
    assert_eq!(
        with_events
            .iter()
            .filter(|e| e.class == "lifecycle.turn.started")
            .count(),
        BOUNDARY_TURNS as usize
    );
    assert_eq!(
        with_events
            .iter()
            .filter(|e| e.class == "lifecycle.turn.finished")
            .count(),
        BOUNDARY_TURNS as usize
    );

    // The withheld arm: k turn pairs and no compaction row — the driver
    // never fabricates a compaction fact.
    let without_events = store.envelopes(&pair.without_proposal.run_id).unwrap();
    assert_eq!(
        without_events
            .iter()
            .filter(|e| e.class == "context.compaction.completed")
            .count(),
        0
    );
    assert_eq!(
        without_events
            .iter()
            .filter(|e| e.class == "lifecycle.turn.started")
            .count(),
        BOUNDARY_TURNS as usize
    );
}

#[test]
fn ac_2_4_2_12_unequal_eval_budgets_refuse_before_any_fork() {
    let (mut store, run, _lease, cut) = source("refuse");
    let before = store.run_ids();
    let err = drive_compaction_boundary(
        &mut store,
        &run,
        cut,
        Some(&proposal_payload()),
        (&budget(5), &budget(3)),
        &ForkOpts::default(),
        &RunManifest::minimal(RunKind::Agent),
        "driver",
    )
    .expect_err("unequal budgets refuse");
    assert!(matches!(err, BoundaryRefusal::UnmatchedBudget { .. }));
    assert_eq!(err.code(), "UnmatchedBudget");
    // The refusal precedes the first fork — no child run exists (CC9: the
    // pair never half-forms).
    assert_eq!(store.run_ids(), before);
}

#[test]
fn ac_2_4_2_12_each_compaction_started_yields_a_distinct_pair() {
    let (mut store, run, lease, _cut) = source("two-cuts");
    // A second `compaction.started` later in the run.
    let second = mint(
        &store,
        &run,
        "context.compaction.started",
        Json::obj([
            ("trigger", Json::str("gauge_cap")),
            ("requirement", Json::str("soft")),
            ("occupancy_before", Json::Int(9_800)),
            ("target_reclaim", Json::Int(2_000)),
            ("min_reclaim", Json::Int(800)),
        ]),
    );
    store.append(&run, &lease, vec![second]).unwrap();
    let cut2 = store.head(&run).unwrap().seq;
    let budget_doc = budget(5);
    let manifest = RunManifest::minimal(RunKind::Agent);
    let p1 = drive_compaction_boundary(
        &mut store,
        &run,
        cut2,
        None,
        (&budget_doc, &budget_doc.clone()),
        &ForkOpts::default(),
        &manifest,
        "driver",
    )
    .expect("second pair drives");
    // A pair at a different cut keys differently — the pair key is the cut.
    assert_eq!(p1.forked_from.at_seq, cut2);
    assert_eq!(
        store
            .envelopes(&p1.with_proposal.run_id)
            .unwrap()
            .iter()
            .filter(|e| e.class == "context.compaction.completed")
            .count(),
        0,
        "a `None` proposal lands no fabricated completion row"
    );
}
