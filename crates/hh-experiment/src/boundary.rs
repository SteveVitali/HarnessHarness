//! `lab/compaction-boundary-v1` — the boundary companion driver
//! (AC-R-2.4.2-12; ADR-0077 d7; the `R-2.4.2²` C2 slice; S4.16b).
//!
//! At each `context.compaction.started` on the source run the companion
//! **forks by reference** (`Store::fork` — the child's `forked_from`
//! lineage anchor binds the source's hash at the cut, and the fork row
//! pins every content address the source prefix references) and continues
//! `k = 5` turns in two paired arms: one with the compaction proposal
//! applied, one without. The paired continuation rows join on the shared
//! `forked_from{run_id, at_seq, head_hash}` anchor — the pair key.
//!
//! The matched-budget precondition (CC9 — matched-budget-or-refuse) is
//! enforced before the first fork: unequal arm eval budgets refuse
//! `UnmatchedBudget`, never a warning.
//!
//! The driver is deterministic and offline: it replays `turn.started /
//! turn.finished` pairs through the children's fenced leases; the applied
//! arm first lands the caller-supplied `context.compaction.completed`
//! payload (the `CompactionRecord` row the compactor produced — the
//! driver mints no compaction fact of its own).

use std::collections::BTreeMap;

use hh_ledger::branch::{BranchKind, ForkOpts};
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::{LineageLink, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_ledger::LedgerError;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

/// `k = 5` — the companion's continuation length per arm (ADR-0077 d7;
/// `eval_budget` on `turns = 5`).
pub const BOUNDARY_TURNS: u64 = 5;

/// The driver failure sum — typed, never a warning (T-LCD-14).
#[derive(Debug)]
pub enum BoundaryRefusal {
    /// The paired arms' eval budgets differ — the matched-budget
    /// precondition (CC9) failed before any fork was taken.
    UnmatchedBudget {
        /// The failure detail.
        detail: String,
    },
    /// The fork or a continuation append failed underneath.
    Ledger(LedgerError),
}

impl From<LedgerError> for BoundaryRefusal {
    fn from(e: LedgerError) -> BoundaryRefusal {
        BoundaryRefusal::Ledger(e)
    }
}

impl BoundaryRefusal {
    /// The refusal-code spelling (`refusal_of` names, never a rendered
    /// message — T-LCD-14).
    pub fn code(&self) -> &'static str {
        match self {
            BoundaryRefusal::UnmatchedBudget { .. } => "UnmatchedBudget",
            BoundaryRefusal::Ledger(_) => "Ledger",
        }
    }
}

/// One paired continuation.
#[derive(Debug)]
pub struct BoundaryArm {
    /// The child run id.
    pub run_id: String,
    /// The child's writer lease — the caller continues/finishes the run.
    pub lease: Lease,
    /// Whether the proposal was applied on this arm.
    pub applied_proposal: bool,
    /// The turns the driver continued (`BOUNDARY_TURNS`).
    pub turns_continued: u64,
}

/// The pair produced at one `context.compaction.started` cut.
#[derive(Debug)]
pub struct BoundaryPair {
    /// The shared lineage anchor — `forked_from{run_id, at_seq,
    /// head_hash}` — identical on both children; the paired-rows join key.
    pub forked_from: LineageLink,
    /// The (possibly coerced) cut seq on the source run.
    pub at_seq: u64,
    /// The arm that applied the compaction proposal.
    pub with_proposal: BoundaryArm,
    /// The arm that continued without it.
    pub without_proposal: BoundaryArm,
}

/// Mint one kernel-origin event on `run_id` (the driver's producer tag;
/// provenance is stamped at append time — every child row is
/// provenance-bearing like its source).
fn mint(store: &Store, _run_id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: store.alloc_id("evt"),
        class: class.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("hh-experiment.boundary/1"),
        scope: Scope::default(),
        parent_event_id: String::new(), // re-chained below
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel(
            "hh-experiment.boundary/1",
            store.now_ms(),
        )),
        content_kind: None,
        payload,
    }
}

/// Append `events` to `run_id` with `parent_event_id` chained head→…→head
/// (the same append-chained discipline the ledger's own drivers use).
fn append_chained(
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    mut events: Vec<Event>,
) -> Result<(), LedgerError> {
    let mut parent = store.head_event_id(run_id)?;
    for e in &mut events {
        e.parent_event_id = parent.clone();
        parent = e.event_id.clone();
    }
    store.append(run_id, lease, events).map(|_| ())
}

/// `drive_compaction_boundary(store, source_run_id, compaction_started_seq,
/// proposal, eval_budgets, opts, child_manifest, holder)` → `BoundaryPair`.
///
/// - `compaction_started_seq` — the seq of the `context.compaction.started`
///   row on the source run (the task coordinate — `fork_by_reference`).
///   `opts` carries the fork discipline; the companion coerces an
///   incoherent cut to the nearest coherent boundary (the coercion is
///   recorded on the `run.forked` row).
/// - `proposal` — the `context.compaction.completed` payload the applied
///   arm lands verbatim; `None` pairs a withheld-applied contrast where
///   the applied arm only continues turns (the companion never fabricates
///   a compaction record).
/// - `eval_budgets` — the paired arms' declared eval budgets as canonical
///   JSON documents; byte-unequal budgets refuse `UnmatchedBudget` before
///   any fork is taken (CC9).
#[allow(clippy::too_many_arguments)] // the members are the boundary pair's shape.
pub fn drive_compaction_boundary(
    store: &mut Store,
    source_run_id: &str,
    compaction_started_seq: u64,
    proposal: Option<&Json>,
    eval_budgets: (&Json, &Json),
    opts: &ForkOpts,
    child_manifest: &RunManifest,
    holder: &str,
) -> Result<BoundaryPair, BoundaryRefusal> {
    // The matched-budget precondition — refusal, never a warning (CC9).
    if eval_budgets.0.to_canonical_string() != eval_budgets.1.to_canonical_string() {
        return Err(BoundaryRefusal::UnmatchedBudget {
            detail: format!(
                "boundary arms carry unequal eval budgets (with={} without={})",
                eval_budgets.0.to_canonical_string(),
                eval_budgets.1.to_canonical_string()
            ),
        });
    }

    // Fork by reference twice at the same cut — both children bind the
    // identical `forked_from{run_id, at_seq, head_hash}` anchor.
    let mut fork_opts = opts.clone();
    fork_opts.coerce_to_boundary = true; // a compaction boundary is a scope edge; the coercion is recorded
    let (with_id, with_lease, _rec_w) = store.fork(
        source_run_id,
        compaction_started_seq,
        BranchKind::Counterfactual,
        &fork_opts,
        child_manifest.clone(),
        holder,
    )?;
    let (without_id, without_lease, _rec_n) = store.fork(
        source_run_id,
        compaction_started_seq,
        BranchKind::Counterfactual,
        &fork_opts,
        child_manifest.clone(),
        holder,
    )?;
    let anchor = store
        .manifest(&with_id)?
        .forked_from
        .clone()
        .ok_or_else(|| LedgerError::SourceIncomplete {
            run_id: with_id.clone(),
            at_seq: compaction_started_seq,
            head: compaction_started_seq,
        })?;
    debug_assert_eq!(
        anchor,
        store
            .manifest(&without_id)?
            .forked_from
            .clone()
            .expect("the paired arm binds the same anchor"),
        "paired arms share the forked_from anchor — the join key"
    );

    // The applied arm lands the proposal's completion row first; both arms
    // then continue `BOUNDARY_TURNS` turn pairs.
    let mut with_events = Vec::new();
    if let Some(p) = proposal {
        with_events.push(mint(
            store,
            &with_id,
            "context.compaction.completed",
            p.clone(),
        ));
    }
    with_events.extend(turn_pairs(store, &with_id));
    append_chained(store, &with_id, &with_lease, with_events)?;
    append_chained(
        store,
        &without_id,
        &without_lease,
        turn_pairs(store, &without_id),
    )?;

    Ok(BoundaryPair {
        forked_from: anchor.clone(),
        at_seq: anchor.at_seq,
        with_proposal: BoundaryArm {
            run_id: with_id,
            lease: with_lease,
            applied_proposal: true,
            turns_continued: BOUNDARY_TURNS,
        },
        without_proposal: BoundaryArm {
            run_id: without_id,
            lease: without_lease,
            applied_proposal: false,
            turns_continued: BOUNDARY_TURNS,
        },
    })
}

/// `k` `turn.started`/`turn.finished` pairs — the deterministic
/// continuation the companion drives; the lab's real subject replay
/// substitutes its own continuation over the same arm handles.
fn turn_pairs(store: &Store, run_id: &str) -> Vec<Event> {
    let mut out = Vec::new();
    for i in 0..BOUNDARY_TURNS {
        let turn = format!("boundary-turn-{i}");
        let mut started = mint(
            store,
            run_id,
            "lifecycle.turn.started",
            Json::obj([("boundary_companion", Json::Bool(true))]),
        );
        started.scope.turn_id = Some(turn.clone());
        let mut finished = mint(
            store,
            run_id,
            "lifecycle.turn.finished",
            Json::obj([("boundary_companion", Json::Bool(true))]),
        );
        finished.scope.turn_id = Some(turn);
        out.push(started);
        out.push(finished);
    }
    out
}
