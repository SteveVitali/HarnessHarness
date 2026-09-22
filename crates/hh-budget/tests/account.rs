//! Integration tests — the `hh-budget` account against the real `hh-ledger`
//! store (the ticket's named seams: allocate/reserve/charge/release/exhaust/
//! amend/advise/attribute_spend/totals/accountability, plus E1–E5 and INV-9's
//! rebuild equality).

use hh_budget::*;
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::{EventRef, RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const TS: &str = "2026-01-01T00:00:00.000Z";

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-budget-test-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// Open a deterministic store, a run, and the writer lease.
fn open(tag: &str) -> (Store, String, Lease) {
    let mut s = Store::open_test(dir(tag), 1_000).unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

/// A caller-side event (executor-produced) — for raw accountable rows.
fn ev(id: &str, class: &str, payload: hh_wire::json::Json) -> Event {
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer {
            component_class: "executor".into(),
            component_variant_ref: "none".into(),
            participant_ref: "none".into(),
        },
        scope: Scope::default(),
        parent_event_id: hh_ledger::ids::ROOT_EVENT.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: None,
        content_kind: None,
        payload,
    }
}

/// Append an accountable event and return its `EventRef` — the charge source.
fn source_event(store: &mut Store, run: &str, lease: &Lease, n: u64) -> EventRef {
    let e = ev(
        &format!("src-{n}"),
        "verification.validator.invoked",
        hh_wire::json::Json::obj([("validator", hh_wire::json::Json::str("v1"))]),
    );
    store.append(run, lease, vec![e]).unwrap();
    EventRef {
        run_id: run.to_string(),
        event_id: format!("src-{n}"),
    }
}

fn scope() -> BudgetScope {
    BudgetScope {
        kind: BudgetScopeKind::AgentProcess,
        target: "proc:main".into(),
    }
}

fn caps(pairs: &[(DimensionId, i64)]) -> BudgetSpec {
    BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &pairs
            .iter()
            .map(|(d, l)| (DimensionKey::Primary(*d), *l))
            .collect::<Vec<_>>(),
    )
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

fn alloc_root(acc: &mut Account, lease: &Lease, spec: BudgetSpec) -> String {
    acc.allocate(lease, None, scope(), spec).unwrap()
}

/// Count ledgered rows of a class (optionally with a payload predicate).
fn class_count(
    acc: &Account,
    run: &str,
    class: &str,
    pred: impl Fn(&hh_wire::json::Json) -> bool,
) -> usize {
    acc.store
        .events(run)
        .unwrap()
        .iter()
        .filter(|e| e.class == class && pred(&e.payload))
        .count()
}

// ── allocate / containment / one-root ────────────────────────────────────────

#[test]
fn one_root_per_run_and_refusals_are_ledgered() {
    let (mut s, run, lease) = open("root");
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(&mut acc, &lease, caps(&[(DimensionId::ModelCalls, 10)]));

    // A second root refuses — and the refusal is a ledger fact (AC-4).
    let e = acc
        .allocate(&lease, None, scope(), caps(&[(DimensionId::ModelCalls, 1)]))
        .unwrap_err();
    assert!(matches!(e, BudgetError::DuplicateRoot { .. }));
    let refused = class_count(&acc, &run, "control.budget.allocated", |p| {
        p.get("outcome").and_then(|o| o.as_str()) == Some("refused")
    });
    assert_eq!(refused, 1, "the refused allocation must be ledgered");
    assert_eq!(acc.tree.root.as_deref(), Some(root.as_str()));
}

#[test]
fn child_within_parent_allocates_over_budget_refuses() {
    let (mut s, run, lease) = open("contain");
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(&mut acc, &lease, caps(&[(DimensionId::ModelCalls, 10)]));

    // Child over parent.remaining → BudgetExceedsParent, ledgered.
    let e = acc
        .allocate(
            &lease,
            Some(&root),
            scope(),
            caps(&[(DimensionId::ModelCalls, 11)]),
        )
        .unwrap_err();
    assert!(matches!(e, BudgetError::BudgetExceedsParent { .. }));
    // A child bounding a dimension the parent doesn't → refused (can't widen the set).
    let e = acc
        .allocate(
            &lease,
            Some(&root),
            scope(),
            caps(&[(DimensionId::ToolCalls, 1)]),
        )
        .unwrap_err();
    assert!(matches!(e, BudgetError::BudgetExceedsParent { .. }));
    // Within → allocates.
    let child = acc
        .allocate(
            &lease,
            Some(&root),
            scope(),
            caps(&[(DimensionId::ModelCalls, 4)]),
        )
        .unwrap();
    assert!(acc.tree.nodes.contains_key(&child));
}

// ── reserve / charge / release ───────────────────────────────────────────────

#[test]
fn reserve_charge_release_conserve() {
    let (mut s, run, lease) = open("rcr");
    let src = source_event(&mut s, &run, &lease, 1);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(&mut acc, &lease, caps(&[(DimensionId::ModelCalls, 5)]));

    // Over-reserving beyond the hard cap → InsufficientBudget (pre-dispatch, E1).
    let e = acc
        .reserve(
            &lease,
            &root,
            &ResourceVector::one(DimensionId::ModelCalls, 6),
            "mc:1",
            60_000,
        )
        .unwrap_err();
    assert!(matches!(e, BudgetError::InsufficientBudget { .. }));

    // Reserve 3, charge 2 against it → consumed 2, claim left 1.
    let rid = acc
        .reserve(
            &lease,
            &root,
            &ResourceVector::one(DimensionId::ModelCalls, 3),
            "mc:1",
            60_000,
        )
        .unwrap();
    let mut req = charge_req(&run, &root, DimensionId::ModelCalls, 2, &src);
    req.reservation_id = Some(rid.clone());
    acc.charge(&lease, &req).unwrap();
    let res = &acc.tree.reservations[&rid];
    assert_eq!(res.consumed.get(DimensionId::ModelCalls), 2);
    // The unused claim was freed by the batch's `reservation_excess` release —
    // affine accounting: claim = consumed + released.
    assert_eq!(res.released.get(DimensionId::ModelCalls), 1);
    assert_eq!(res.claim_left(DimensionId::ModelCalls), 0);

    // The reservation is fully settled — `release` of an already-settled claim
    // is UnknownReservation (affine: the claim was consumed+released, never
    // aliased or dropped).
    let e = acc.release(&lease, &rid).unwrap_err();
    assert!(matches!(e, BudgetError::UnknownReservation { .. }));
    assert!(!acc.tree.reservations[&rid].outstanding);

    // A fresh reservation of the full remaining 3 (5 − 2 consumed) is fine; 4 is not.
    acc.reserve(
        &lease,
        &root,
        &ResourceVector::one(DimensionId::ModelCalls, 3),
        "mc:2",
        60_000,
    )
    .unwrap();
    let e = acc
        .reserve(
            &lease,
            &root,
            &ResourceVector::one(DimensionId::ModelCalls, 1),
            "mc:3",
            60_000,
        )
        .unwrap_err();
    assert!(matches!(e, BudgetError::InsufficientBudget { .. }));
}

#[test]
fn over_reservation_is_charged_and_flagged() {
    let (mut s, run, lease) = open("over");
    let src = source_event(&mut s, &run, &lease, 1);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(&mut acc, &lease, caps(&[(DimensionId::ModelCalls, 10)]));
    let rid = acc
        .reserve(
            &lease,
            &root,
            &ResourceVector::one(DimensionId::ModelCalls, 1),
            "mc:1",
            60_000,
        )
        .unwrap();
    let mut req = charge_req(&run, &root, DimensionId::ModelCalls, 3, &src);
    req.reservation_id = Some(rid.clone());
    acc.charge(&lease, &req).unwrap();
    // Over-consumption is charged, flagged `over_reservation`, never refused (ADR-0040 D3).
    let rows = &acc.tree.charges_by_source[&src.event_id];
    assert!(rows.iter().all(|r| r.attribution.over_reservation));
    assert_eq!(
        acc.tree
            .node(&root)
            .unwrap()
            .consumed
            .get(DimensionId::ModelCalls),
        3
    );
}

// ── charge idempotency + ancestor propagation ────────────────────────────────

#[test]
fn charge_is_idempotent_and_propagates_to_ancestors() {
    let (mut s, run, lease) = open("idem");
    let src = source_event(&mut s, &run, &lease, 1);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(&mut acc, &lease, caps(&[(DimensionId::ModelCalls, 10)]));
    let child = acc
        .allocate(
            &lease,
            Some(&root),
            scope(),
            caps(&[(DimensionId::ModelCalls, 4)]),
        )
        .unwrap();
    let req = charge_req(&run, &child, DimensionId::ModelCalls, 2, &src);
    acc.charge(&lease, &req).unwrap();

    // The charge appears at the child AND the root (propagation).
    assert_eq!(
        acc.tree
            .node(&child)
            .unwrap()
            .consumed
            .get(DimensionId::ModelCalls),
        2
    );
    assert_eq!(
        acc.tree
            .node(&root)
            .unwrap()
            .consumed
            .get(DimensionId::ModelCalls),
        2
    );
    // Two `consumed` rows — one per node on the path.
    assert_eq!(acc.tree.charges_by_source[&src.event_id].len(), 2);

    // Replaying the same (source, dimension) is a no-op — zero new rows (AC-7).
    let head_before = acc.store.head(&run).unwrap().seq;
    let r = acc.charge(&lease, &req).unwrap();
    assert_eq!(r.count, 0);
    assert_eq!(acc.store.head(&run).unwrap().seq, head_before);
    assert_eq!(
        acc.tree
            .node(&root)
            .unwrap()
            .consumed
            .get(DimensionId::ModelCalls),
        2
    );
}

#[test]
fn cost_totals_counts_each_charge_once() {
    let (mut s, run, lease) = open("totals");
    let src = source_event(&mut s, &run, &lease, 1);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(&mut acc, &lease, caps(&[(DimensionId::ModelCalls, 10)]));
    let child = acc
        .allocate(
            &lease,
            Some(&root),
            scope(),
            caps(&[(DimensionId::ModelCalls, 4)]),
        )
        .unwrap();
    acc.charge(
        &lease,
        &charge_req(&run, &child, DimensionId::ModelCalls, 3, &src),
    )
    .unwrap();
    // The materialized view counts the charge once (leaf), not once per
    // propagated row — the view is rebuilt from charges, never a counter.
    let t = acc.totals(&TotalsScope::Run, &TotalsGroupBy::default());
    assert_eq!(t.by_dimension.get(DimensionId::ModelCalls), 3);
    let t = acc.totals(
        &TotalsScope::Budget(root.clone()),
        &TotalsGroupBy::default(),
    );
    assert_eq!(t.by_dimension.get(DimensionId::ModelCalls), 3);
}

// ── E1–E5 ──────────────────────────────────────────────────────────────────

#[test]
fn e1_exhaust_refuses_while_an_effect_is_open_then_succeeds() {
    let (mut s, run, lease) = open("e1");
    let src = source_event(&mut s, &run, &lease, 1);
    let root;
    {
        let mut acc = Account::open(&mut s, &run).unwrap();
        root = alloc_root(&mut acc, &lease, caps(&[(DimensionId::ModelCalls, 1)]));
        acc.charge(
            &lease,
            &charge_req(&run, &root, DimensionId::ModelCalls, 1, &src),
        )
        .unwrap();
    }
    // Open an effect scope — E1: exhaustion waits for the committed effect.
    let mut eff = ev(
        "eff-open",
        "action.effect.intended",
        hh_wire::json::Json::obj([("effect_id", hh_wire::json::Json::str("eff-1"))]),
    );
    eff.producer = Producer::kernel("kernel:test");
    eff.provenance = Some(hh_provenance::ProvenanceRecord::kernel("kernel:test", 0));
    eff.scope.effect_id = Some("eff-1".into());
    s.append(&run, &lease, vec![eff]).unwrap();
    {
        let mut acc = Account::open(&mut s, &run).unwrap();
        let e = acc
            .exhaust(
                &lease,
                &root,
                DimensionKey::Primary(DimensionId::ModelCalls),
                None,
            )
            .unwrap_err();
        assert!(matches!(e, BudgetError::EffectsInFlight { .. }));
    }
    // Close the effect — exhaustion now proceeds.
    let mut term = ev(
        "eff-close",
        "action.effect.observed",
        hh_wire::json::Json::obj([("effect_id", hh_wire::json::Json::str("eff-1"))]),
    );
    term.producer = Producer::kernel("kernel:test");
    term.provenance = Some(hh_provenance::ProvenanceRecord::kernel("kernel:test", 0));
    term.scope.effect_id = Some("eff-1".into());
    s.append(&run, &lease, vec![term]).unwrap();
    let mut acc = Account::open(&mut s, &run).unwrap();
    acc.exhaust(
        &lease,
        &root,
        DimensionKey::Primary(DimensionId::ModelCalls),
        None,
    )
    .unwrap();
    // E2: `control.budget.exceeded` + `control.decision{stop}` are ledgered.
    assert_eq!(acc.tree.exceeded.len(), 1);
    assert_eq!(
        acc.tree.stop_decision.as_ref().map(|d| d.kind.as_str()),
        Some("stop")
    );
    // A second exhaust → AlreadyStopped.
    let e = acc
        .exhaust(
            &lease,
            &root,
            DimensionKey::Primary(DimensionId::ModelCalls),
            None,
        )
        .unwrap_err();
    assert!(matches!(e, BudgetError::AlreadyStopped { .. }));
}

#[test]
fn e3_grace_permits_one_terminal_call_then_refuses() {
    let (mut s, run, lease) = open("e3");
    let mut acc = Account::open(&mut s, &run).unwrap();
    let mut spec = BudgetSpec::default(); // Pool is the default mode.
    spec.dimensions.insert(
        DimensionKey::Primary(DimensionId::ModelCalls),
        DimensionRule {
            hard: Some(Ceiling {
                limit: 1,
                unit: "calls".into(),
            }),
            grace: Some(Grace::one()),
            ..Default::default()
        },
    );
    let root = alloc_root(&mut acc, &lease, spec);
    let key = DimensionKey::Primary(DimensionId::ModelCalls);
    acc.claim_grace(&lease, &root, key).unwrap();
    let e = acc.claim_grace(&lease, &root, key).unwrap_err();
    assert!(matches!(e, BudgetError::GraceExhausted { .. }));
}

#[test]
fn e4_soft_threshold_fires_once_per_epoch_and_rearms() {
    let (mut s, run, lease) = open("e4");
    let src = source_event(&mut s, &run, &lease, 1);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let mut spec = BudgetSpec::default(); // Pool is the default mode.
    spec.dimensions.insert(
        DimensionKey::Primary(DimensionId::ModelCalls),
        DimensionRule {
            hard: Some(Ceiling {
                limit: 10,
                unit: "calls".into(),
            }),
            soft: vec![Threshold::at_fraction_ppm(500_000, "rule:remind")],
            ..Default::default()
        },
    );
    let root = alloc_root(&mut acc, &lease, spec);
    acc.charge(
        &lease,
        &charge_req(&run, &root, DimensionId::ModelCalls, 6, &src),
    )
    .unwrap();

    // Crossed 50%: fires once at epoch 0; a second advise at the same epoch is
    // a no-op; a new compaction epoch re-arms it.
    assert_eq!(acc.advise(&lease, &root, "w0", 0).unwrap().len(), 1);
    assert!(acc.advise(&lease, &root, "w0", 0).unwrap().is_empty());
    assert_eq!(acc.advise(&lease, &root, "w0", 1).unwrap().len(), 1);
    // The soft threshold never moves `check` — the node is under its hard cap.
    assert!(acc.check(&root).unwrap().is_empty());
}

#[test]
fn e5_gauge_cap_refuses_increment_never_stops() {
    let (mut s, run, lease) = open("e5");
    let src = source_event(&mut s, &run, &lease, 1);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let mut spec = BudgetSpec::default(); // Pool is the default mode.
    spec.dimensions.insert(
        DimensionKey::Primary(DimensionId::FanOut),
        DimensionRule {
            hard: Some(Ceiling {
                limit: 2,
                unit: "count".into(),
            }),
            ..Default::default()
        },
    );
    let root = alloc_root(&mut acc, &lease, spec);
    acc.observe_gauge(&root, DimensionId::FanOut, 2);
    // The next increment is refused — `SpawnRefused` territory, never a stop.
    let e = acc
        .gauge_reserve(&root, DimensionId::FanOut, 3)
        .unwrap_err();
    assert!(matches!(e, BudgetError::GaugeCapExceeded { .. }));
    assert!(acc.gauge_reserve(&root, DimensionId::FanOut, 2).is_ok());
    assert!(acc.check(&root).unwrap().is_empty());
    // Gauges are never charged (DimensionNotBudgetable).
    let e = acc
        .charge(
            &lease,
            &charge_req(&run, &root, DimensionId::FanOut, 1, &src),
        )
        .unwrap_err();
    assert!(matches!(e, BudgetError::DimensionNotBudgetable { .. }));
}

// ── amend / resolve_ask ──────────────────────────────────────────────────────

#[test]
fn amend_respects_the_authority_table() {
    let (mut s, run, lease) = open("amend");
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(&mut acc, &lease, caps(&[(DimensionId::ModelCalls, 10)]));
    let child = acc
        .allocate(
            &lease,
            Some(&root),
            scope(),
            caps(&[(DimensionId::ModelCalls, 4)]),
        )
        .unwrap();

    // Operator amends only the root.
    assert!(matches!(
        acc.amend(
            &lease,
            &child,
            &caps(&[(DimensionId::ModelCalls, 5)]),
            AmendAuthority::Operator
        ),
        Err(BudgetError::AuthorityInsufficient { .. })
    ));
    // HostOverride tightens only — widening refuses.
    assert!(matches!(
        acc.amend(
            &lease,
            &child,
            &caps(&[(DimensionId::ModelCalls, 5)]),
            AmendAuthority::HostOverride
        ),
        Err(BudgetError::AuthorityInsufficient { .. })
    ));
    acc.amend(
        &lease,
        &child,
        &caps(&[(DimensionId::ModelCalls, 3)]),
        AmendAuthority::HostOverride,
    )
    .unwrap();
    // Parent may amend a child — but only within the parent's remaining.
    assert!(matches!(
        acc.amend(
            &lease,
            &child,
            &caps(&[(DimensionId::ModelCalls, 11)]),
            AmendAuthority::Parent
        ),
        Err(BudgetError::AuthorityInsufficient { .. })
    ));
    acc.amend(
        &lease,
        &child,
        &caps(&[(DimensionId::ModelCalls, 6)]),
        AmendAuthority::Parent,
    )
    .unwrap();
}

#[test]
fn approvals_exhaustion_converts_ask_to_deny() {
    let (mut s, run, lease) = open("ask");
    let src = source_event(&mut s, &run, &lease, 1);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(
        &mut acc,
        &lease,
        caps(&[(DimensionId::ApprovalsRequested, 1)]),
    );
    assert_eq!(acc.resolve_ask(&lease, &root).unwrap(), AskOutcome::Allow);
    acc.charge(
        &lease,
        &charge_req(&run, &root, DimensionId::ApprovalsRequested, 1, &src),
    )
    .unwrap();
    assert_eq!(acc.resolve_ask(&lease, &root).unwrap(), AskOutcome::Deny);
    // The deny is a ledger fact — `security.permission.decided{deny, policy}`.
    let decided = class_count(&acc, &run, "security.permission.decided", |p| {
        p.get("decision").and_then(|d| d.as_str()) == Some("deny")
    });
    assert_eq!(decided, 1);
}

// ── spend attribution / accountability / rebuild ─────────────────────────────

#[test]
fn attribute_spend_ledgers_the_row_and_is_idempotent() {
    let (mut s, run, lease) = open("spend");
    let src = source_event(&mut s, &run, &lease, 1);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(&mut acc, &lease, caps(&[(DimensionId::Spend, 1_000_000)]));
    let decomp = TokenDecomposition {
        input_uncached: 100,
        cache_read: 0,
        cache_write: BTreeMap::new(),
        output_visible: 50,
        output_reasoning: 0,
    };
    let model = ModelRef {
        profile_ref: "profile:test".into(),
        provider_model_id: "claude-test".into(),
        serving_route: "anthropic".into(),
        effort: None,
    };
    let table = PricingTable {
        table_id: "t1".into(),
        version: "v1".into(),
        currency: "USD".into(),
        rows: vec![PricingRow {
            model_ref: model.pricing_key(),
            input_uncached: 1000,
            input_cache_read: 100,
            input_cache_write: BTreeMap::new(),
            output_visible: 5000,
            output_reasoning: None,
            tiers: vec![],
            valid_from: "2026-01-01".into(),
            source: "test".into(),
        }],
    };
    let pin = Some("sha256:aa".to_string());
    acc.attribute_spend(
        &lease,
        src.clone(),
        &decomp,
        &model,
        &table,
        pin.clone(),
        &SpendSource::Measured,
        1_000_000,
        Attribution::subject(&run, &root, "participant:main"),
    )
    .unwrap();
    assert_eq!(acc.tree.spend_rows.len(), 1);
    // Replaying the same (source, spend) is a no-op.
    let r = acc
        .attribute_spend(
            &lease,
            src,
            &decomp,
            &model,
            &table,
            pin,
            &SpendSource::Measured,
            1_000_000,
            Attribution::subject(&run, &root, "participant:main"),
        )
        .unwrap();
    assert_eq!(r.count, 0);
    assert_eq!(acc.tree.spend_rows.len(), 1);
    // Totals carry the spend counter + provenance/confidence strata.
    let t = acc.totals(&TotalsScope::Run, &TotalsGroupBy::default());
    assert_eq!(
        t.spend_by_currency.get("USD"),
        Some(&(100 * 1000 + 50 * 5000))
    );
    assert_eq!(
        t.provenance_mix.get("estimated_from_pricing"),
        Some(&(100 * 1000 + 50 * 5000))
    );
}

#[test]
fn accountability_reports_orphans_and_clean_runs() {
    let (mut s, run, lease) = open("racc2");
    // An uncharged accountable event is an orphan.
    let _uncharged = source_event(&mut s, &run, &lease, 9);
    let root;
    {
        let mut acc = Account::open(&mut s, &run).unwrap();
        root = alloc_root(&mut acc, &lease, caps(&[(DimensionId::ModelCalls, 10)]));
        assert_eq!(
            acc.accountability_report().orphans,
            vec!["src-9".to_string()]
        );
        assert!(!acc.accountability_report().is_clean());
    }
    // Charging it clears the report (R-ACC-2) — re-open so the fold sees src-9.
    let mut acc = Account::open(&mut s, &run).unwrap();
    let src = EventRef {
        run_id: run.clone(),
        event_id: "src-9".into(),
    };
    acc.charge(
        &lease,
        &charge_req(&run, &root, DimensionId::ModelCalls, 1, &src),
    )
    .unwrap();
    assert!(acc.accountability_report().is_clean());
}

#[test]
fn rebuild_from_the_ledger_matches_the_incremental_projection() {
    let d = dir("inv9");
    let mut s = Store::open_test(&d, 1_000).unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    let src = source_event(&mut s, &run, &lease, 1);
    let mut snapshot: BTreeMap<String, i64> = BTreeMap::new();
    let res_count: usize;
    let charged_count: usize;
    {
        let mut acc = Account::open(&mut s, &run).unwrap();
        let root = alloc_root(&mut acc, &lease, caps(&[(DimensionId::ModelCalls, 10)]));
        let child = acc
            .allocate(
                &lease,
                Some(&root),
                scope(),
                caps(&[(DimensionId::ModelCalls, 4)]),
            )
            .unwrap();
        let rid = acc
            .reserve(
                &lease,
                &child,
                &ResourceVector::one(DimensionId::ModelCalls, 2),
                "mc:1",
                60_000,
            )
            .unwrap();
        let mut req = charge_req(&run, &child, DimensionId::ModelCalls, 1, &src);
        req.reservation_id = Some(rid);
        acc.charge(&lease, &req).unwrap();
        for (id, n) in &acc.tree.nodes {
            snapshot.insert(id.clone(), n.consumed.get(DimensionId::ModelCalls));
        }
        res_count = acc.tree.reservations.len();
        charged_count = acc.tree.charged.len();
    }
    // INV-9: a fresh store replaying the WAL rebuilds an identical projection.
    let mut s2 = Store::open_test(&d, 1_000).unwrap();
    let acc2 = Account::open(&mut s2, &run).unwrap();
    assert_eq!(snapshot.len(), acc2.tree.nodes.len());
    for (id, consumed) in &snapshot {
        assert_eq!(
            acc2.tree.nodes[id].consumed.get(DimensionId::ModelCalls),
            *consumed,
            "{id} consumed"
        );
    }
    assert_eq!(res_count, acc2.tree.reservations.len());
    assert_eq!(charged_count, acc2.tree.charged.len());
}
