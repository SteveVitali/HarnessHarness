//! R5.13 — deduplicated incurred-use and unknown-exposure views
//! (PADR-R5-2 B.1 item 4, B.9 items 3–4; REQ-D-006/REQ-E-102/REQ-E-103/
//! REQ-E-204/REQ-E-207 contributing; AC-1/AC-2).
//!
//! `Account::physical_use` exposes physical incurred/billed/unknown use
//! **beside** the logical scope totals: each `(source_event, dimension)`
//! charge counts once at its leaf-most node (propagated ancestor copies
//! and repeated reconciles deduplicate to the charge); reverted-effect
//! charges stay (physical spend survives logical reversal); `released`
//! rows never subtract physical use; a terminal call whose usage is not
//! known is unknown exposure — never zero. Scratch `Store::open_test`
//! runs — the tests/account.rs + r5_12.rs idiom.

use hharness_budget::account::TotalsGroupBy;
use hharness_budget::*;
use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::manifest::{EventRef, RunKind, RunManifest};
use hharness_ledger::store::{Lease, Store};
use hharness_wire::json::Json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-budget-r513-{}-{tag}-{n}", std::process::id()));
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

/// A minimal `run_kind = experiment` manifest — `minimal` seeds the
/// agent-only configuration cells; non-agent runs must not carry them
/// (ADR-0183 §C), or `open_run` refuses with `ManifestInvalid`.
fn experiment_manifest() -> RunManifest {
    let mut m = RunManifest::minimal(RunKind::Experiment);
    m.configuration_id = None;
    m.configuration_version_id = None;
    m
}

fn scope() -> BudgetScope {
    BudgetScope {
        kind: BudgetScopeKind::AgentProcess,
        target: "proc:main".into(),
    }
}

fn primary(d: DimensionId) -> DimensionKey {
    DimensionKey::Primary(d)
}

fn caps(mode: BudgetMode, pairs: &[(DimensionKey, i64)]) -> BudgetSpec {
    BudgetSpec::hard_caps(mode, pairs)
}

fn alloc_root(acc: &mut Account, lease: &Lease, spec: BudgetSpec) -> String {
    acc.allocate(lease, None, scope(), spec).unwrap()
}

fn alloc_child(acc: &mut Account, lease: &Lease, parent: &str, spec: BudgetSpec) -> String {
    acc.allocate(lease, Some(parent), scope(), spec).unwrap()
}

/// An accountable source row for `charge` (the r5_12 idiom); `effect`
/// scopes it so the revert-exclusion join can fire.
fn source_event(
    store: &mut Store,
    run: &str,
    lease: &Lease,
    n: u64,
    effect: Option<&str>,
) -> EventRef {
    let e = Event {
        event_id: format!("src-{n}"),
        class: "verification.validator.invoked".to_string(),
        ts: "2026-01-01T00:00:00.000Z".to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope {
            effect_id: effect.map(str::to_string),
            ..Scope::default()
        },
        parent_event_id: store.head_event_id(run).unwrap(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
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

/// Mint + append one row directly (pre-account plumbing: the effect
/// scope must open before `source_event` may carry it).
fn mint(
    store: &mut Store,
    run: &str,
    lease: &Lease,
    id: &str,
    class: &str,
    payload: Json,
    effect: Option<&str>,
) {
    let e = Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: "2026-01-01T00:00:00.000Z".to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope {
            effect_id: effect.map(str::to_string),
            ..Scope::default()
        },
        parent_event_id: store.head_event_id(run).unwrap(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(hharness_provenance::ProvenanceRecord::kernel(
            "kernel:test",
            0,
        )),
        content_kind: None,
        payload,
    };
    store.append(run, lease, vec![e]).unwrap();
}

/// Append a minted row and fold it into the open account's projection —
/// the same fold `Account::append` performs on its own writes. `effect`
/// scopes the row to an open effect (the revert-exclusion join).
fn append_fold_scoped(
    acc: &mut Account,
    run: &str,
    lease: &Lease,
    class: &str,
    payload: Json,
    effect: Option<&str>,
) {
    let e = Event {
        event_id: acc.store.alloc_id("evt"),
        class: class.to_string(),
        ts: "2026-01-01T00:00:00.000Z".to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope {
            effect_id: effect.map(str::to_string),
            ..Scope::default()
        },
        parent_event_id: acc.store.head_event_id(run).unwrap(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(hharness_provenance::ProvenanceRecord::kernel(
            "kernel:test",
            0,
        )),
        content_kind: None,
        payload,
    };
    let r = acc.store.append(run, lease, vec![e]).unwrap();
    let tail = acc.store.events(run).unwrap();
    for env in &tail[(r.first as usize)..=(r.last as usize)] {
        acc.tree.fold(env).unwrap();
    }
}

// ── AC-1: separate views; dedup propagation + reconcile; spend survives ──

#[test]
fn physical_incurred_dedups_ancestor_propagation() {
    let (mut s, run, lease) = open("dedup");
    let src = source_event(&mut s, &run, &lease, 1, None);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(
        &mut acc,
        &lease,
        caps(BudgetMode::Pool, &[(primary(DimensionId::ModelCalls), 20)]),
    );
    let child = alloc_child(
        &mut acc,
        &lease,
        &root,
        caps(BudgetMode::Pool, &[(primary(DimensionId::ModelCalls), 10)]),
    );

    acc.charge(
        &lease,
        &charge_req(&run, &child, DimensionId::ModelCalls, 5, &src),
    )
    .unwrap();

    // The propagation fact: two ledgered `consumed` rows for ONE charge —
    // leaf (child) and ancestor (root) copies.
    let rows = &acc.tree.charges_by_source[&src.event_id];
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|r| r.budget_id == child));
    assert!(rows.iter().any(|r| r.budget_id == root));
    // Logical per-node views keep the propagated per-node amounts.
    assert_eq!(
        acc.tree
            .node(&child)
            .unwrap()
            .consumed
            .get(DimensionId::ModelCalls),
        5
    );
    assert_eq!(
        acc.tree
            .node(&root)
            .unwrap()
            .consumed
            .get(DimensionId::ModelCalls),
        5
    );
    // The physical view counts the charge once — at the leaf-most node —
    // never once per propagated copy.
    let pu = acc.physical_use();
    assert_eq!(pu.incurred.get(DimensionId::ModelCalls), 5);
    assert_eq!(
        pu.incurred_by_charged_to["subject"].get(DimensionId::ModelCalls),
        5
    );
    assert!(pu.unknown.is_empty());
    // `totals` (the logical cost view) agrees — it has always deduplicated.
    let t = acc.totals(&TotalsScope::Run, &TotalsGroupBy::default());
    assert_eq!(t.by_dimension.get(DimensionId::ModelCalls), 5);
}

#[test]
fn logical_and_physical_views_stay_separate() {
    let (mut s, run, lease) = open("separate");
    let src = source_event(&mut s, &run, &lease, 1, None);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(
        &mut acc,
        &lease,
        caps(BudgetMode::Pool, &[(primary(DimensionId::ModelCalls), 20)]),
    );

    // A held claim is logical capacity — it is NOT physical use.
    let res = acc
        .reserve(
            &lease,
            &root,
            &ResourceVector::one(DimensionId::ModelCalls, 10),
            "mc:1",
            60_000,
        )
        .unwrap();
    assert_eq!(
        acc.remaining(&root, primary(DimensionId::ModelCalls)),
        Some(10)
    );
    assert_eq!(acc.physical_use().incurred.get(DimensionId::ModelCalls), 0);

    // Charged use is both logical consumption and physical incurred use.
    let mut req = charge_req(&run, &root, DimensionId::ModelCalls, 3, &src);
    req.reservation_id = Some(res.clone());
    acc.charge(&lease, &req).unwrap();
    let pu = acc.physical_use();
    assert_eq!(pu.incurred.get(DimensionId::ModelCalls), 3);
    // Logical: the charge consumed 3 of the 10-claim and the 7 excess was
    // released (`released{reservation_excess}`) — the reservation is fully
    // settled; `remaining` = 20 − 3 consumed.
    assert_eq!(
        acc.tree
            .reservation(&res)
            .unwrap()
            .claim_left(DimensionId::ModelCalls),
        0
    );
    assert_eq!(
        acc.remaining(&root, primary(DimensionId::ModelCalls)),
        Some(17)
    );
}

#[test]
fn repeated_reconcile_and_reprojection_count_once() {
    let (mut s, run, lease) = open("reconcile");
    let src = source_event(&mut s, &run, &lease, 1, None);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(
        &mut acc,
        &lease,
        caps(BudgetMode::Pool, &[(primary(DimensionId::ModelCalls), 20)]),
    );

    // The execution-cost transfer settles once: a re-issued charge with
    // the same (source, dimension) is an empty no-op (idempotent
    // reconcile), and the physical view never double-counts it.
    acc.charge(
        &lease,
        &charge_req(&run, &root, DimensionId::ModelCalls, 4, &src),
    )
    .unwrap();
    let again = acc
        .charge(
            &lease,
            &charge_req(&run, &root, DimensionId::ModelCalls, 4, &src),
        )
        .unwrap();
    assert_eq!(again.count, 0);
    let pu = acc.physical_use();
    assert_eq!(pu.incurred.get(DimensionId::ModelCalls), 4);

    // A fresh Account over the same WAL re-projects the identical view —
    // repeated reconcile/rebuild cannot inflate the fold.
    let acc2 = Account::open(&mut s, &run).unwrap();
    assert_eq!(acc2.physical_use(), pu);
}

#[test]
fn reverted_effect_keeps_physical_spend() {
    let (mut s, run, lease) = open("revert");
    // The effect scope opens before any row may carry it.
    mint(
        &mut s,
        &run,
        &lease,
        "evt-effect-e1",
        "action.effect.intended",
        Json::obj([
            ("effect_id", Json::str("effect:e1")),
            (
                "effective_risk_class",
                Json::obj([
                    ("reversibility", Json::str("read_only")),
                    ("repeat_safety", Json::str("idempotent")),
                    ("scope", Json::str("workspace_local")),
                ]),
            ),
        ]),
        Some("effect:e1"),
    );
    let src = source_event(&mut s, &run, &lease, 1, Some("effect:e1"));
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(
        &mut acc,
        &lease,
        caps(BudgetMode::Pool, &[(primary(DimensionId::ModelCalls), 20)]),
    );
    acc.charge(
        &lease,
        &charge_req(&run, &root, DimensionId::ModelCalls, 6, &src),
    )
    .unwrap();

    // Logical reversal: the effect reaches `observed{applied}` (a
    // `read_only` intent carries no write-ahead — `prepared` → `observed`
    // is legal without `committed`/`decided`) and is then reverted by the
    // scope-free marker form (`original_effect_id`, no `scope.effect_id`;
    // `fencing_token` on every post-`prepared` row) — `totals` excludes
    // the charge by projection.
    let ft = Json::Int(lease.generation as i64);
    append_fold_scoped(
        &mut acc,
        &run,
        &lease,
        "action.effect.authorized",
        Json::obj([("effect_id", Json::str("effect:e1"))]),
        Some("effect:e1"),
    );
    append_fold_scoped(
        &mut acc,
        &run,
        &lease,
        "action.effect.prepared",
        Json::obj([
            ("effect_id", Json::str("effect:e1")),
            ("idempotency_key", Json::str("ik-e1")),
            ("fencing_token", ft.clone()),
        ]),
        Some("effect:e1"),
    );
    append_fold_scoped(
        &mut acc,
        &run,
        &lease,
        "action.effect.observed",
        Json::obj([
            ("effect_id", Json::str("effect:e1")),
            ("attempt_no", Json::Int(1)),
            ("outcome", Json::str("applied")),
            ("fencing_token", ft.clone()),
        ]),
        Some("effect:e1"),
    );
    append_fold_scoped(
        &mut acc,
        &run,
        &lease,
        "action.effect.reverted",
        Json::obj([
            ("original_effect_id", Json::str("effect:e1")),
            ("fencing_token", ft.clone()),
        ]),
        None,
    );
    let t = acc.totals(&TotalsScope::Run, &TotalsGroupBy::default());
    assert_eq!(t.by_dimension.get(DimensionId::ModelCalls), 0);

    // Physical spend survives the reversal — incurred is preserved, never
    // subtracted and never replenished back into the physical ceiling.
    let pu = acc.physical_use();
    assert_eq!(pu.incurred.get(DimensionId::ModelCalls), 6);
}

#[test]
fn instrument_charges_stay_separate_from_subject() {
    let (mut s, run, lease) = open("roots");
    let src_s = source_event(&mut s, &run, &lease, 1, None);
    let src_i = source_event(&mut s, &run, &lease, 2, None);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(
        &mut acc,
        &lease,
        caps(BudgetMode::Pool, &[(primary(DimensionId::ModelCalls), 20)]),
    );

    // Subject-side charge.
    acc.charge(
        &lease,
        &charge_req(&run, &root, DimensionId::ModelCalls, 3, &src_s),
    )
    .unwrap();
    // Instrument-side charge on the same run — the strata stay separate.
    let mut req = charge_req(&run, &root, DimensionId::ModelCalls, 2, &src_i);
    req.attribution = Attribution::instrument(&run, &root, "participant:main");
    acc.charge(&lease, &req).unwrap();

    let pu = acc.physical_use();
    assert_eq!(pu.incurred.get(DimensionId::ModelCalls), 5);
    assert_eq!(
        pu.incurred_by_charged_to["subject"].get(DimensionId::ModelCalls),
        3
    );
    assert_eq!(
        pu.incurred_by_charged_to["instrument"].get(DimensionId::ModelCalls),
        2
    );

    // A separate instrument run (R5.11's instrument run/Account) has its
    // own root: its physical use never merges into the subject run's —
    // and vice versa.
    let (irun, ilease) = s.open_run(experiment_manifest(), "writer-b").unwrap();
    let src2 = source_event(&mut s, &irun, &ilease, 9, None);
    {
        let mut iacc = Account::open(&mut s, &irun).unwrap();
        let iroot = alloc_root(
            &mut iacc,
            &ilease,
            caps(BudgetMode::Pool, &[(primary(DimensionId::ModelCalls), 50)]),
        );
        let mut ireq = charge_req(&irun, &iroot, DimensionId::ModelCalls, 7, &src2);
        ireq.attribution = Attribution::instrument(&irun, &iroot, "participant:main");
        iacc.charge(&ilease, &ireq).unwrap();

        let ipu = iacc.physical_use();
        assert_eq!(ipu.incurred.get(DimensionId::ModelCalls), 7);
        assert_eq!(
            ipu.incurred_by_charged_to["instrument"].get(DimensionId::ModelCalls),
            7
        );
    }
    // The subject run's view is untouched by the instrument run's rows.
    let acc2 = Account::open(&mut s, &run).unwrap();
    assert_eq!(acc2.physical_use().incurred.get(DimensionId::ModelCalls), 5);
}

// ── AC-2: duplicates/above-reserve/ambiguous/reversal+release; compat ────

#[test]
fn actual_above_reserve_counts_fully() {
    let (mut s, run, lease) = open("over");
    let src = source_event(&mut s, &run, &lease, 1, None);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(
        &mut acc,
        &lease,
        caps(BudgetMode::Pool, &[(primary(DimensionId::ModelCalls), 200)]),
    );
    let res = acc
        .reserve(
            &lease,
            &root,
            &ResourceVector::one(DimensionId::ModelCalls, 50),
            "mc:over",
            60_000,
        )
        .unwrap();

    // Actual 80 > reserved 50: the excess is charged and flagged
    // `over_reservation`, never refused — physical incurred is the
    // actual 80, never clamped to the claim.
    let mut req = charge_req(&run, &root, DimensionId::ModelCalls, 80, &src);
    req.reservation_id = Some(res.clone());
    acc.charge(&lease, &req).unwrap();

    let pu = acc.physical_use();
    assert_eq!(pu.incurred.get(DimensionId::ModelCalls), 80);
    // The consumed rows carry the over-reservation fact.
    let rows = &acc.tree.charges_by_source[&src.event_id];
    assert!(rows.iter().all(|r| r.attribution.over_reservation));
    // The claim was fully consumed — nothing outstanding to release.
    assert_eq!(
        acc.tree
            .reservation(&res)
            .unwrap()
            .claim_left(DimensionId::ModelCalls),
        0
    );
}

#[test]
fn ambiguous_failed_response_is_unknown_never_zero() {
    let (mut s, run, lease) = open("unknown");
    let mut acc = Account::open(&mut s, &run).unwrap();
    let _root = alloc_root(
        &mut acc,
        &lease,
        caps(BudgetMode::Pool, &[(primary(DimensionId::ModelCalls), 20)]),
    );

    // `model.call.failed` with no `usage` member — the ambiguous failed
    // response: the physical outcome of the wire is unknown.
    append_fold_scoped(
        &mut acc,
        &run,
        &lease,
        "model.call.failed",
        Json::obj([
            ("model_call_id", Json::str("mc-bad")),
            ("error", Json::obj([("class", Json::str("timeout"))])),
        ]),
        None,
    );
    // `model.call.completed{usage.available:false}` — `n/a`, never zero.
    append_fold_scoped(
        &mut acc,
        &run,
        &lease,
        "model.call.completed",
        Json::obj([
            ("model_call_id", Json::str("mc-na")),
            ("usage", Json::obj([("available", Json::Bool(false))])),
        ]),
        None,
    );
    // Controls: a failed row WITH usage and an available completed row
    // are known — they create no unknown-exposure record.
    append_fold_scoped(
        &mut acc,
        &run,
        &lease,
        "model.call.failed",
        Json::obj([
            ("model_call_id", Json::str("mc-known")),
            (
                "usage",
                Json::obj([("tokens.output.visible", Json::Int(12))]),
            ),
        ]),
        None,
    );
    append_fold_scoped(
        &mut acc,
        &run,
        &lease,
        "model.call.completed",
        Json::obj([
            ("model_call_id", Json::str("mc-ok")),
            ("usage", Json::obj([("available", Json::Bool(true))])),
        ]),
        None,
    );

    let pu = acc.physical_use();
    assert_eq!(pu.unknown.len(), 2);
    assert_eq!(
        pu.unknown
            .iter()
            .map(|u| (u.model_call_id.as_str(), u.reason.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("mc-bad", "ambiguous_failure"),
            ("mc-na", "usage_unavailable"),
        ]
    );
    // Unknown exposure never turns into a numeric total — incurred stays 0
    // while the exposure records stand as evidence of *unknown*.
    assert_eq!(pu.incurred.get(DimensionId::ModelCalls), 0);
    // Re-projection deduplicates on the terminal row's event id — the
    // same two records, never doubled.
    let acc2 = Account::open(&mut s, &run).unwrap();
    assert_eq!(acc2.physical_use().unknown.len(), 2);
}

#[test]
fn release_and_completion_create_no_physical_capacity() {
    let (mut s, run, lease) = open("release");
    let src = source_event(&mut s, &run, &lease, 1, None);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(
        &mut acc,
        &lease,
        caps(BudgetMode::Pool, &[(primary(DimensionId::ModelCalls), 200)]),
    );
    let slice = alloc_child(
        &mut acc,
        &lease,
        &root,
        caps(
            BudgetMode::Slice,
            &[(primary(DimensionId::ModelCalls), 100)],
        ),
    );

    // Reserve 40 against the slice; charge 30 without naming it.
    let res = acc
        .reserve(
            &lease,
            &slice,
            &ResourceVector::one(DimensionId::ModelCalls, 40),
            "mc:r",
            60_000,
        )
        .unwrap();
    acc.charge(
        &lease,
        &charge_req(&run, &slice, DimensionId::ModelCalls, 30, &src),
    )
    .unwrap();
    let before = acc.physical_use().incurred.get(DimensionId::ModelCalls);
    assert_eq!(before, 30);

    // `release` frees the whole claim — logical capacity returns, the
    // physical view is untouched (a release is never a charge refund).
    acc.release(&lease, &res).unwrap();
    assert_eq!(
        acc.tree
            .node(&slice)
            .unwrap()
            .reserved
            .get(DimensionId::ModelCalls),
        0
    );
    assert_eq!(acc.physical_use().incurred.get(DimensionId::ModelCalls), 30);

    // `complete` returns the slice's unspent remainder
    // (`released{completion}`) — again, no physical capacity is created.
    acc.complete(&lease, &slice).unwrap();
    assert_eq!(acc.physical_use().incurred.get(DimensionId::ModelCalls), 30);
    // The slice's consumed stays — `remaining` accounts the spend.
    assert_eq!(
        acc.tree
            .node(&slice)
            .unwrap()
            .consumed
            .get(DimensionId::ModelCalls),
        30
    );
}

#[test]
fn billed_use_is_deduplicated_beside_logical_totals() {
    let (mut s, run, lease) = open("billed");
    let src = source_event(&mut s, &run, &lease, 1, None);
    let mut acc = Account::open(&mut s, &run).unwrap();
    let root = alloc_root(
        &mut acc,
        &lease,
        caps(
            BudgetMode::Pool,
            &[(primary(DimensionId::Spend), 1_000_000)],
        ),
    );
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
    let expected = 100 * 1000 + 50 * 5000;
    acc.attribute_spend(
        &lease,
        src.clone(),
        &decomp,
        &model,
        &table,
        Some("sha256:aa".to_string()),
        &SpendSource::Measured,
        1_000_000,
        Attribution::subject(&run, &root, "participant:main"),
    )
    .unwrap();
    // A replayed attribution settles nothing twice — billed stays once.
    let r = acc
        .attribute_spend(
            &lease,
            src,
            &decomp,
            &model,
            &table,
            Some("sha256:aa".to_string()),
            &SpendSource::Measured,
            1_000_000,
            Attribution::subject(&run, &root, "participant:main"),
        )
        .unwrap();
    assert_eq!(r.count, 0);

    let pu = acc.physical_use();
    assert_eq!(pu.billed_micro_units.get("USD"), Some(&expected));
    // Logical totals carry the same spend — compatible, separate views.
    let t = acc.totals(&TotalsScope::Run, &TotalsGroupBy::default());
    assert_eq!(t.spend_by_currency.get("USD"), Some(&expected));
}
