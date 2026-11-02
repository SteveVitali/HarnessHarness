//! R5.12 — atomic whole-vector Account cap admission (PADR-R5-2 B.1 item 3;
//! REQ-E-207 / REQ-D-006 contributing; AC-1/AC-2).
//!
//! `Account::reserve` projects the whole vector at the node and every
//! ancestor: every bounded key — primary or derived — is checked against
//! `consumed + reserved + slice_held + quantity` through the owning
//! `level`/`key_total` fold (never a parallel total; `tokens.*.total` stay
//! derived, never charged). Refusing any dimension holds nothing; the exact
//! boundary admits. Gauge dims refuse `DimensionNotBudgetable` (E5), invalid
//! spellings are unconstructible, and outstanding holds survive WAL
//! re-projection. Scratch `Store::open_test` runs — the tests/account.rs idiom.

use hharness_budget::*;
use hharness_ledger::manifest::{EventRef, RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store};
use hharness_wire::json::Json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-budget-r512-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// A deterministic scratch store + run + writer lease.
fn open(tag: &str) -> (Store, String, Lease) {
    let mut s = Store::open_test(dir(tag), 1_000).unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

fn scope() -> BudgetScope {
    BudgetScope {
        kind: BudgetScopeKind::AgentProcess,
        target: "proc:main".into(),
    }
}

fn caps(mode: BudgetMode, pairs: &[(DimensionKey, i64)]) -> BudgetSpec {
    BudgetSpec::hard_caps(mode, pairs)
}

fn primary(d: DimensionId) -> DimensionKey {
    DimensionKey::Primary(d)
}

fn vec2(a: (DimensionId, i64), b: (DimensionId, i64)) -> ResourceVector {
    let mut v = ResourceVector::zero();
    v.add(a.0, a.1);
    v.add(b.0, b.1);
    v
}

/// `reserve` with the battery's fixed holder/ttl.
fn res(
    acc: &mut Account,
    lease: &Lease,
    budget_id: &str,
    v: &ResourceVector,
) -> Result<String, BudgetError> {
    res_for(acc, lease, budget_id, v, "mc:1")
}

/// `reserve` with a named holder (the recovery leg re-derives by holder).
fn res_for(
    acc: &mut Account,
    lease: &Lease,
    budget_id: &str,
    v: &ResourceVector,
    holder: &str,
) -> Result<String, BudgetError> {
    acc.reserve(lease, budget_id, v, holder, 60_000)
}

fn alloc_root(acc: &mut Account, lease: &Lease, spec: BudgetSpec) -> String {
    acc.allocate(lease, None, scope(), spec).unwrap()
}

fn alloc_child(acc: &mut Account, lease: &Lease, parent: &str, spec: BudgetSpec) -> String {
    acc.allocate(lease, Some(parent), scope(), spec).unwrap()
}

/// `(dimension, requested, available)` of an `InsufficientBudget`, else panic.
fn insuff(e: BudgetError) -> (DimensionKey, i64, i64) {
    match e {
        BudgetError::InsufficientBudget {
            dimension,
            requested,
            available,
        } => (dimension, requested, available),
        other => panic!("expected InsufficientBudget, got {other}"),
    }
}

/// An accountable source row for `charge` (the tests/account.rs idiom).
fn source_event(store: &mut Store, run: &str, lease: &Lease, n: u64) -> EventRef {
    let e = hharness_ledger::event::Event {
        event_id: format!("src-{n}"),
        class: "verification.validator.invoked".to_string(),
        ts: "2026-01-01T00:00:00.000Z".to_string(),
        hlc: None,
        producer: hharness_ledger::event::Producer::kernel("kernel:test"),
        scope: hharness_ledger::event::Scope::default(),
        parent_event_id: store.head_event_id(run).unwrap(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: std::collections::BTreeMap::new(),
        provenance: Some(hharness_provenance::ProvenanceRecord::kernel(
            "kernel:test",
            0,
        )),
        content_kind: None,
        payload: Json::obj([("validator", Json::str("v1"))]),
    };
    store.append(run, lease, vec![e]).unwrap();
    EventRef {
        run_id: run.to_string(),
        event_id: format!("src-{n}"),
    }
}

fn charge_req(
    run: &str,
    budget_id: &str,
    d: DimensionId,
    amount: i64,
    src: &EventRef,
) -> ChargeRequest {
    ChargeRequest {
        budget_id: budget_id.to_string(),
        quantity: ResourceQuantity {
            dimension: d,
            amount,
            unit: d.unit().to_string(),
            model_ref: None,
            measured_at: src.clone(),
        },
        source: src.clone(),
        attribution: Attribution::subject(run, budget_id, "participant:main"),
        reservation_id: None,
        cache_ttl: None,
    }
}

/// Ledgered `control.budget.reserved` rows by outcome.
fn reserved_count(acc: &Account, run: &str, outcome: &str) -> usize {
    acc.store
        .events(run)
        .unwrap()
        .iter()
        .filter(|e| {
            e.class == "control.budget.reserved"
                && e.payload.get("outcome").and_then(Json::as_str) == Some(outcome)
        })
        .count()
}

const INPUT_TOTAL: DimensionKey = DimensionKey::Derived(DerivedDimension::TokensInputTotal);
const OUTPUT_TOTAL: DimensionKey = DimensionKey::Derived(DerivedDimension::TokensOutputTotal);

// ── AC-1: projected whole-vector check; boundary admits; refusal holds nothing

#[test]
fn derived_total_refuses_individually_small_components() {
    let (mut s, run, lease) = open("derived");
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(
        &mut acc,
        &lease,
        caps(BudgetMode::Pool, &[(INPUT_TOTAL, 100)]),
    );

    // uncached 60 + cache_read 50: each component is small, the projected
    // derived total 110 > 100 → refuse naming the derived key (requested is
    // the owning `key_total` component sum — never a stored total).
    let e = res(
        &mut acc,
        &lease,
        &root,
        &vec2(
            (DimensionId::TokensInputUncached, 60),
            (DimensionId::TokensInputCacheRead, 50),
        ),
    )
    .unwrap_err();
    assert_eq!(insuff(e), (INPUT_TOTAL, 110, 100));
    // Refusing any dimension reserves nothing: no claim, view unchanged,
    // exactly one refused row ledgered.
    assert!(acc.tree.reservations.is_empty());
    assert!(acc.tree.node(&root).unwrap().reserved.is_empty());
    assert_eq!(reserved_count(&acc, &run, "refused"), 1);
    assert_eq!(reserved_count(&acc, &run, "held"), 0);
    assert_eq!(acc.remaining(&root, INPUT_TOTAL), Some(100));

    // Exact boundary: 60 + 40 = 100 == hard → held.
    res(
        &mut acc,
        &lease,
        &root,
        &vec2(
            (DimensionId::TokensInputUncached, 60),
            (DimensionId::TokensInputCacheRead, 40),
        ),
    )
    .unwrap();
    assert_eq!(reserved_count(&acc, &run, "held"), 1);

    // The outstanding hold is counted: one more token through a third
    // component exceeds the cap → available 0.
    let e = res(
        &mut acc,
        &lease,
        &root,
        &ResourceVector::one(DimensionId::TokensInputCacheWrite, 1),
    )
    .unwrap_err();
    assert_eq!(insuff(e), (INPUT_TOTAL, 1, 0));
}

#[test]
fn whole_vector_atomic_refusal_holds_nothing() {
    let (mut s, run, lease) = open("atomic");
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(
        &mut acc,
        &lease,
        caps(
            BudgetMode::Pool,
            &[
                (primary(DimensionId::ModelCalls), 2),
                (primary(DimensionId::TokensInputUncached), 50),
            ],
        ),
    );

    // model_calls 3 > 2 violates while uncached 10 would fit — the whole
    // vector is refused atomically: neither dim is held.
    let e = res(
        &mut acc,
        &lease,
        &root,
        &vec2(
            (DimensionId::ModelCalls, 3),
            (DimensionId::TokensInputUncached, 10),
        ),
    )
    .unwrap_err();
    assert_eq!(insuff(e), (primary(DimensionId::ModelCalls), 3, 2));
    assert!(acc.tree.reservations.is_empty());
    assert!(acc.tree.node(&root).unwrap().reserved.is_empty());
    assert_eq!(reserved_count(&acc, &run, "refused"), 1);
    assert_eq!(
        acc.remaining(&root, primary(DimensionId::ModelCalls)),
        Some(2)
    );
    assert_eq!(
        acc.remaining(&root, primary(DimensionId::TokensInputUncached)),
        Some(50)
    );

    // Both dims at their exact boundary admit.
    res(
        &mut acc,
        &lease,
        &root,
        &vec2(
            (DimensionId::ModelCalls, 2),
            (DimensionId::TokensInputUncached, 50),
        ),
    )
    .unwrap();
}

#[test]
fn every_ancestor_hard_key_projects_child_ancestor_and_slice_holds() {
    let (mut s, run, lease) = open("ancestors");
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(
        &mut acc,
        &lease,
        caps(
            BudgetMode::Pool,
            &[(primary(DimensionId::ModelCalls), 10), (INPUT_TOTAL, 200)],
        ),
    );
    // A slice child moves its derived ceiling out of the parent's pool:
    // slice_held contributes 50 per input component → the parent's projected
    // input.total level is 150 (the existing affine slice fold's
    // conservative component-wise hold).
    let slice = alloc_child(
        &mut acc,
        &lease,
        &root,
        caps(BudgetMode::Slice, &[(INPUT_TOTAL, 50)]),
    );
    let pool = alloc_child(
        &mut acc,
        &lease,
        &root,
        caps(BudgetMode::Pool, &[(primary(DimensionId::ModelCalls), 4)]),
    );

    // (a) The leaf's own primary cap is checked first (leaf→root): 5 > 4.
    let e = res(
        &mut acc,
        &lease,
        &pool,
        &ResourceVector::one(DimensionId::ModelCalls, 5),
    )
    .unwrap_err();
    assert_eq!(insuff(e), (primary(DimensionId::ModelCalls), 5, 4));

    // (b) Exact boundary at the leaf admits; the root's model_calls level
    // (4 ≤ 10) and slice-held input.total (150 ≤ 200) pass.
    res(
        &mut acc,
        &lease,
        &pool,
        &ResourceVector::one(DimensionId::ModelCalls, 4),
    )
    .unwrap();

    // (c) A token vector the leaf doesn't bound is projected at the root:
    // slice_held 150 + request 80 = 230 > 200 → the ancestor's derived cap
    // refuses, counting the sibling slice hold.
    let e = res(
        &mut acc,
        &lease,
        &pool,
        &vec2(
            (DimensionId::TokensInputUncached, 40),
            (DimensionId::TokensInputCacheRead, 40),
        ),
    )
    .unwrap_err();
    assert_eq!(insuff(e), (INPUT_TOTAL, 80, 50));

    // (d) The slice child reserving inside its own slice hits its own
    // derived cap at the boundary; at the root the level aggregates
    // slice_held + the child's outstanding claim → 200 == 200 admits.
    res(
        &mut acc,
        &lease,
        &slice,
        &vec2(
            (DimensionId::TokensInputUncached, 30),
            (DimensionId::TokensInputCacheRead, 20),
        ),
    )
    .unwrap();

    // (e) The root's input.total level is now exactly at cap — any further
    // token claim from the sibling refuses with available 0.
    let e = res(
        &mut acc,
        &lease,
        &pool,
        &ResourceVector::one(DimensionId::TokensInputUncached, 1),
    )
    .unwrap_err();
    assert_eq!(insuff(e), (INPUT_TOTAL, 1, 0));
}

#[test]
fn consumed_usage_counts_toward_projected_derived_total() {
    let (mut s, run, lease) = open("consumed");
    let src = source_event(&mut s, &run, &lease, 1);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(
        &mut acc,
        &lease,
        caps(BudgetMode::Pool, &[(OUTPUT_TOTAL, 100)]),
    );
    // Consumed usage (charged primary components) counts toward the cap.
    acc.charge(
        &lease,
        &charge_req(&run, &root, DimensionId::TokensOutputVisible, 40, &src),
    )
    .unwrap();

    // 40 consumed + (visible 20 + reasoning 50) = 110 > 100 → refuse.
    let e = res(
        &mut acc,
        &lease,
        &root,
        &vec2(
            (DimensionId::TokensOutputVisible, 20),
            (DimensionId::TokensOutputReasoning, 50),
        ),
    )
    .unwrap_err();
    assert_eq!(insuff(e), (OUTPUT_TOTAL, 70, 60));

    // 40 consumed + 60 requested = 100 == hard → exact boundary admits.
    res(
        &mut acc,
        &lease,
        &root,
        &vec2(
            (DimensionId::TokensOutputVisible, 10),
            (DimensionId::TokensOutputReasoning, 50),
        ),
    )
    .unwrap();
}

// ── AC-2: invalid/gauge keys, recovery races, no derived charges ───────────

#[test]
fn invalid_gauge_and_derived_spellings_refuse() {
    let (mut s, run, lease) = open("keys");
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(
        &mut acc,
        &lease,
        caps(BudgetMode::Pool, &[(primary(DimensionId::FanOut), 4)]),
    );

    // A gauge dim in the request → DimensionNotBudgetable before any row is
    // minted (the request is invalid, not denied).
    let e = res(
        &mut acc,
        &lease,
        &root,
        &ResourceVector::one(DimensionId::FanOut, 1),
    )
    .unwrap_err();
    assert!(matches!(e, BudgetError::DimensionNotBudgetable { .. }));
    assert_eq!(reserved_count(&acc, &run, "refused"), 0);
    assert_eq!(reserved_count(&acc, &run, "held"), 0);

    // A stale/unknown budget id refuses — never a hold on nothing.
    let e = acc
        .reserve(
            &lease,
            "budget:gone",
            &ResourceVector::one(DimensionId::ModelCalls, 1),
            "mc:gone",
            60_000,
        )
        .unwrap_err();
    assert!(matches!(e, BudgetError::UnknownBudget { .. }));

    // Invalid keys can never reach `reserve` or `charge`: stored vectors and
    // `ResourceQuantity.dimension` (`DimensionId`, primary only) refuse
    // derived and unknown spellings at parse — no charge or hold can name
    // `tokens.*.total`.
    for bad in [
        "tokens.input.total",
        "tokens.output.total",
        "tokens.blended",
        "bogus.dim",
    ] {
        assert!(ResourceVector::from_json(&Json::obj([(bad, Json::Int(1))])).is_none());
        assert!(DimensionId::parse(bad).is_none());
    }
}

#[test]
fn held_row_stores_primaries_no_parallel_total() {
    let (mut s, run, lease) = open("primaries");
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(
        &mut acc,
        &lease,
        caps(BudgetMode::Pool, &[(INPUT_TOTAL, 100)]),
    );
    res(
        &mut acc,
        &lease,
        &root,
        &vec2(
            (DimensionId::TokensInputUncached, 60),
            (DimensionId::TokensInputCacheRead, 40),
        ),
    )
    .unwrap();

    // The held row's `quantity` carries primary spellings only — the wire
    // fact stores components; totals are derived at evaluation by the owning
    // `key_total` fold, never stored or computed by a parallel fold.
    let held = acc
        .store
        .events(&run)
        .unwrap()
        .iter()
        .find(|e| {
            e.class == "control.budget.reserved"
                && e.payload.get("outcome").and_then(Json::as_str) == Some("held")
        })
        .expect("a held row");
    let Json::Obj(m) = held.payload.get("quantity").expect("quantity member") else {
        panic!("quantity must be an object");
    };
    assert_eq!(m.len(), 2);
    for k in m.keys() {
        assert!(DimensionId::parse(k).is_some(), "{k} must be primary");
        assert!(
            DerivedDimension::parse(k).is_none(),
            "{k} must not be derived"
        );
    }
}

#[test]
fn recovery_projection_keeps_outstanding_holds_atomic() {
    let d = dir("recovery");
    let mut s = Store::open_test(&d, 1_000).unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    let child;
    {
        let mut acc = Account::open(&mut s, &run).unwrap();
        let root = alloc_root(
            &mut acc,
            &lease,
            caps(BudgetMode::Pool, &[(primary(DimensionId::ModelCalls), 5)]),
        );
        child = alloc_child(
            &mut acc,
            &lease,
            &root,
            caps(BudgetMode::Pool, &[(primary(DimensionId::ModelCalls), 4)]),
        );
        res_for(
            &mut acc,
            &lease,
            &child,
            &ResourceVector::one(DimensionId::ModelCalls, 2),
            "effect:e1",
        )
        .unwrap();
    }

    // A fresh store replaying the WAL rebuilds the same projection: the
    // outstanding hold is not lost (INV-9 read-side equality).
    let mut s2 = Store::open_test(&d, 1_000).unwrap();
    let acc2 = Account::open(&mut s2, &run).unwrap();
    assert_eq!(acc2.tree.reservations.len(), 1);
    assert_eq!(
        acc2.tree
            .node(&child)
            .unwrap()
            .reserved
            .get(DimensionId::ModelCalls),
        2
    );

    // A fresh Account over the live store (the mid-run recovery path)
    // enforces against the recovered projection: the holder lookup resolves
    // (the `unknown`-effect retry affordance), and a reserve that fits only
    // if the hold were forgotten refuses — level 2 + request 4 > leaf cap 4.
    let mut acc3 = Account::open(&mut s, &run).unwrap();
    let res = acc3
        .tree
        .reservation_for(&child, "effect:e1")
        .expect("the recovered hold resolves by holder");
    assert_eq!(res.claim_left(DimensionId::ModelCalls), 2);
    let e = res_for(
        &mut acc3,
        &lease,
        &child,
        &ResourceVector::one(DimensionId::ModelCalls, 4),
        "effect:e2",
    )
    .unwrap_err();
    assert_eq!(insuff(e), (primary(DimensionId::ModelCalls), 4, 2));
    assert_eq!(acc3.tree.reservations.len(), 1);

    // What actually remains claimable still admits — 2 + 2 = 4 boundary.
    res_for(
        &mut acc3,
        &lease,
        &child,
        &ResourceVector::one(DimensionId::ModelCalls, 2),
        "effect:e2",
    )
    .unwrap();
    assert_eq!(acc3.tree.reservations.len(), 2);
}
