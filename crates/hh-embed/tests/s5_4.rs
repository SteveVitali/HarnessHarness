//! S5.4 — the `hh-embed/1` debt/model/analysis op surface (§5h.6/7/8;
//! R-2.9.6¹/R-2.9.7¹/R-2.9.8¹; AC-R-2.9.6-10):
//!
//! - `lab.debt.{evaluate,index,report}` — the live all-home evaluator,
//!   the DebtIndex fold, and the DebtReport + notice routing — all
//!   records-in/records-out (the manager is out-of-process by
//!   construction); the op result is *byte-identical* to the owning
//!   `hh_lab::debt` fold (the two-surface contract: one canonical
//!   projection, `to_canonical_string` equality).
//! - `lab.model.{snapshot_claim,regression}` — the synthetic DRIFT
//!   claim + the `run_regression_suite` verdict, and the
//!   `regression_drifted_rules` member feeding back into
//!   `lab.debt.evaluate` (the `drifted ⇒ expiring` chain through the
//!   embed boundary, never a private verb).
//! - `lab.analysis.{component_targets,attribution_design}` — the M1
//!   design surface helpers verbatim.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_embed::service::{EmbedService, ServiceConfig};
use hh_hir::leaves::Text;
use hh_hir::records::AssumptionDebtRecord;
use hh_ontology::debt::{
    DebtClass, DebtScope, DebtStatus, EvidenceRef, ExpiryCondition, ExpiryKind, ModelSelector,
    OwnerRef, RemovalTest, RemovalTestKind,
};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

// ── service plumbing (mirrors tests/conformance.rs) ─────────────────────────

fn test_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-s54-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn service() -> EmbedService {
    let root = test_dir("svc");
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "s54".into(),
    })
    .unwrap()
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn ok(resp: &Json) -> Json {
    resp.get("result")
        .unwrap_or_else(|| panic!("expected result, got {}", resp.to_canonical_string()))
        .clone()
}

fn hello(svc: &mut EmbedService) {
    let mut caps = BTreeMap::new();
    caps.insert("experimental".to_string(), Json::Bool(true));
    caps.insert("serves_measurement".to_string(), Json::Bool(true));
    let r = call(
        svc,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("s54")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            ("capabilities", Json::Obj(caps)),
        ]),
    );
    assert!(r.get("result").is_some(), "hello refused: {r:?}");
}

// ── fixtures ────────────────────────────────────────────────────────────────

fn debt_record(rule_id: &str, kind: ExpiryKind) -> AssumptionDebtRecord {
    AssumptionDebtRecord {
        rule_id: rule_id.into(),
        hypothesis: Text::new(
            "the assumption holds while the profile does",
            "owner:o",
            ProvenanceRecord::kernel("s54.test", 0),
        ),
        evidence_refs: vec![EvidenceRef::legacy("sha256:ev-1")],
        owner: OwnerRef {
            team: false,
            id: "o1".into(),
            reach_via: vec!["sink:log".into()],
        },
        expiry_condition: ExpiryCondition { kind, value: None },
        removal_test_ref: "sha256:test".into(),
        status: DebtStatus::Active,
        debt_class: Some(DebtClass::Hypothesized),
        hypothesis_typed: None,
        scope: Some(DebtScope {
            model_selectors: vec![ModelSelector::Exact {
                model_id: "test:model".into(),
            }],
            ..Default::default()
        }),
        expiry: None,
        runway_ms: None,
        revalidation: None,
        removal_test: Some(RemovalTest::new(RemovalTestKind::Inspection)),
        created_by: None,
        created_at: Some(1),
        supersedes: None,
    }
}

fn record_json(r: &AssumptionDebtRecord) -> Json {
    // The non-semantic spelling — `debt_from_json` requires the full
    // provenance-bearing members (the `semantic` form drops them).
    hh_hir::debt_json(r, false)
}

// ── lab.debt.evaluate ───────────────────────────────────────────────────────

#[test]
fn debt_evaluate_folds_the_live_triggers() {
    let mut svc = service();
    hello(&mut svc);
    let r = ok(&call(
        &mut svc,
        "lab.debt.evaluate",
        Json::obj([
            (
                "record",
                record_json(&debt_record("r1", ExpiryKind::ModelVersionChange)),
            ),
            ("debt_ref", Json::str("debt:r1")),
            ("home", Json::Int(3)),
            ("now_ms", Json::Int(1_000)),
            (
                "observables",
                Json::obj([(
                    "profile_change",
                    Json::obj([
                        ("kind", Json::str("rebound")),
                        ("grace_elapsed", Json::Bool(false)),
                        ("profile_ref", Json::str("profile:p-2")),
                    ]),
                )]),
            ),
        ]),
    ));
    match r.get("transitions") {
        Some(Json::Arr(ts)) => {
            assert_eq!(ts.len(), 1);
            assert_eq!(ts[0].get("to").and_then(Json::as_str), Some("expiring"));
            match ts[0].get("causes") {
                Some(Json::Arr(cs)) => assert!(cs
                    .iter()
                    .any(|c| c.as_str() == Some("profile_change:rebound"))),
                other => panic!("causes: {other:?}"),
            }
        }
        other => panic!("transitions: {other:?}"),
    }
}

// ── lab.debt.index — byte-identity with the owning fold ─────────────────────

#[test]
fn debt_index_is_byte_identical_to_the_lab_fold() {
    let mut svc = service();
    hello(&mut svc);
    let record = debt_record("r1", ExpiryKind::Date);
    let entry = Json::obj([
        ("home", Json::Int(3)),
        ("version_id", Json::str("v-1")),
        ("record", record_json(&record)),
        (
            "transitions",
            Json::Arr(vec![Json::obj([
                ("debt_ref", Json::str("debt:r1")),
                ("from", Json::str("active")),
                ("to", Json::str("expiring")),
                ("trigger", Json::str("date")),
            ])]),
        ),
        ("used_by", Json::Arr(vec![Json::str("sv:1")])),
        ("expired_used_runs", Json::Int(2)),
    ]);
    let via_op = ok(&call(
        &mut svc,
        "lab.debt.index",
        Json::obj([("entries", Json::Arr(vec![entry]))]),
    ));

    // The owning fold — same records in, byte-identical rows out.
    let folded = hh_lab::debt::debt_index(&[hh_lab::debt::DebtIndexEntry {
        home: Some(3),
        version_id: Some("v-1".into()),
        record: record.clone(),
        transitions: vec![hh_lab::debt::DebtTransition {
            debt_ref: "debt:r1".into(),
            from: DebtStatus::Active,
            to: DebtStatus::Expiring,
            trigger: "date".into(),
            evidence_ref: None,
            causes: vec![],
        }],
        removal_test_state: None,
        used_by: vec!["sv:1".into()],
        expired_used_runs: 2,
    }]);
    let expected = Json::obj([(
        "rows",
        Json::Arr(folded.iter().map(|r| r.to_json()).collect()),
    )]);
    assert_eq!(
        via_op.to_canonical_string(),
        expected.to_canonical_string(),
        "the embed surface returns the canonical fold byte-for-byte"
    );
    // The folded row carries the S5.4 spec shape.
    match via_op.get("rows") {
        Some(Json::Arr(rows)) => {
            let row = &rows[0];
            assert_eq!(
                row.get("current_status").and_then(Json::as_str),
                Some("expiring")
            );
            assert_eq!(
                row.get("stored_status").and_then(Json::as_str),
                Some("active")
            );
            assert_eq!(row.get("last_trigger").and_then(Json::as_str), Some("date"));
            assert_eq!(
                row.get("debt_ref")
                    .and_then(|d| d.get("home"))
                    .and_then(Json::as_int),
                Some(3)
            );
            assert_eq!(row.get("expired_used_runs").and_then(Json::as_int), Some(2));
        }
        other => panic!("rows: {other:?}"),
    }
}

// ── lab.debt.report — the health view + routed notices ──────────────────────

#[test]
fn debt_report_routes_notices_through_owner_sinks() {
    let mut svc = service();
    hello(&mut svc);
    // One expiring row inside the warn window (owner reachable at
    // `sink:log`) + one expired row whose owner declares no sinks.
    let now = 1_000_000i64;
    let mut unreachable = debt_record("r2", ExpiryKind::Date);
    unreachable.owner = OwnerRef::principal("o2");
    let rows_doc = Json::Arr(vec![
        hh_lab::debt::DebtIndexRow {
            rule_id: "r1".into(),
            owner: OwnerRef {
                team: false,
                id: "o1".into(),
                reach_via: vec!["sink:log".into()],
            },
            status: DebtStatus::Expiring,
            removal_test_kind: Some(RemovalTestKind::Inspection),
            expiry_condition: ExpiryCondition {
                kind: ExpiryKind::Date,
                value: None,
            },
            created_at: 1,
            expiry_at: Some((now + 100) as u64),
            home: Some(3),
            version_id: None,
            debt_class: None,
            stored_status: None,
            evidence_grade: None,
            deficiency_class: None,
            last_trigger: None,
            removal_test_state: None,
            used_by: vec![],
            expired_used_runs: 0,
            staleness_reasons: vec![],
        }
        .to_json(),
        hh_lab::debt::DebtIndexRow {
            rule_id: "r2".into(),
            owner: unreachable.owner.clone(),
            status: DebtStatus::Expired,
            removal_test_kind: None,
            expiry_condition: ExpiryCondition {
                kind: ExpiryKind::Date,
                value: None,
            },
            created_at: 1,
            expiry_at: Some((now - 100) as u64),
            home: Some(3),
            version_id: None,
            debt_class: None,
            stored_status: None,
            evidence_grade: None,
            deficiency_class: None,
            last_trigger: None,
            removal_test_state: None,
            used_by: vec![],
            expired_used_runs: 0,
            staleness_reasons: vec![],
        }
        .to_json(),
    ]);
    let r = ok(&call(
        &mut svc,
        "lab.debt.report",
        Json::obj([
            ("rows", rows_doc),
            ("now_ms", Json::Int(now)),
            (
                "policy",
                Json::obj([("notice_sinks", Json::Arr(vec![Json::str("sink:log")]))]),
            ),
            ("scope", Json::obj([("home", Json::Int(3))])),
        ]),
    ));
    let report = r.get("report").expect("report");
    assert_eq!(
        report.get("expired_used"),
        Some(&Json::Arr(vec![Json::str("r2")]))
    );
    assert_eq!(
        report.get("expiring"),
        Some(&Json::Arr(vec![Json::str("r1")]))
    );
    assert_eq!(
        report.get("scope"),
        Some(&Json::obj([("home", Json::Int(3))]))
    );
    let routed = r.get("routed_notices").expect("routed_notices");
    match routed.get("sink:log") {
        Some(Json::Arr(n)) => {
            assert_eq!(n.len(), 1);
            assert_eq!(
                n[0].get("kind").and_then(Json::as_str),
                Some("expiry_approaching")
            );
        }
        other => panic!("sink:log notices: {other:?}"),
    }
    match routed.get("unrouted") {
        Some(Json::Arr(n)) => {
            assert_eq!(n.len(), 1);
            assert_eq!(
                n[0].get("kind").and_then(Json::as_str),
                Some("expired_used")
            );
        }
        other => panic!("unrouted notices: {other:?}"),
    }
}

// ── lab.model.* — the compat surface ────────────────────────────────────────

#[test]
fn model_snapshot_claim_and_regression_chain_into_debt() {
    let mut svc = service();
    hello(&mut svc);
    // The synthetic DRIFT claim (AC-R-2.9.8-10).
    let claim = ok(&call(
        &mut svc,
        "lab.model.snapshot_claim",
        Json::obj([
            ("provider", Json::str("acme")),
            ("model_id", Json::str("acme-m")),
            ("drift_ref", Json::str("drift-9")),
        ]),
    ));
    let c = claim.get("claim").expect("claim");
    assert_eq!(
        c.get("snapshot_id").and_then(Json::as_str),
        Some("drift:drift-9")
    );
    assert_eq!(
        c.get("policy_version_exposed").and_then(Json::as_str),
        Some("unknown")
    );

    // The regression fold — a `drift` verdict names the rule.
    let reg = ok(&call(
        &mut svc,
        "lab.model.regression",
        Json::obj([
            (
                "suite",
                Json::obj([
                    ("suite_id", Json::str("suite:1")),
                    (
                        "checks",
                        Json::Arr(vec![Json::obj([("rule_id", Json::str("r1"))])]),
                    ),
                ]),
            ),
            (
                "results",
                Json::Arr(vec![Json::obj([
                    ("rule_id", Json::str("r1")),
                    ("verdict", Json::str("drift")),
                ])]),
            ),
            ("evidence_ref", Json::str("ev:regression-1")),
        ]),
    ));
    // `drifted{rules[]}` — the one-key sum object.
    match reg.get("status") {
        Some(Json::Obj(m)) => {
            let d = m.get("drifted").expect("status.drifted");
            assert_eq!(d.get("rules"), Some(&Json::Arr(vec![Json::str("r1")])));
        }
        other => panic!("status: {other:?}"),
    }
    let drifted = match reg.get("regression_drifted_rules") {
        Some(Json::Arr(rs)) => rs
            .iter()
            .filter_map(Json::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>(),
        other => panic!("regression_drifted_rules: {other:?}"),
    };
    assert_eq!(drifted, vec!["r1".to_string()]);

    // The `drifted ⇒ expiring` chain through the op surface — the drifted
    // rules feed `lab.debt.evaluate`'s observables.
    let ev = ok(&call(
        &mut svc,
        "lab.debt.evaluate",
        Json::obj([
            (
                "record",
                record_json(&debt_record("r1", ExpiryKind::ModelVersionChange)),
            ),
            ("debt_ref", Json::str("debt:r1")),
            ("now_ms", Json::Int(1_000)),
            (
                "observables",
                Json::obj([(
                    "expiry",
                    Json::obj([(
                        "regression_drifted_rules",
                        Json::Arr(drifted.iter().map(Json::str).collect()),
                    )]),
                )]),
            ),
        ]),
    ));
    match ev.get("transitions") {
        Some(Json::Arr(ts)) => {
            assert_eq!(ts.len(), 1);
            assert_eq!(ts[0].get("to").and_then(Json::as_str), Some("expiring"));
        }
        other => panic!("transitions: {other:?}"),
    }
}

// ── lab.analysis.* — the M1 design surface ──────────────────────────────────

#[test]
fn analysis_component_targets_and_attribution_design() {
    let mut svc = service();
    hello(&mut svc);
    let t = ok(&call(
        &mut svc,
        "lab.analysis.component_targets",
        Json::obj([(
            "definition",
            Json::obj([
                ("procedures", Json::Arr(vec![Json::str("proc:main")])),
                ("leaves", Json::Arr(vec![Json::str("leaf:a")])),
            ]),
        )]),
    ));
    match t.get("targets") {
        Some(Json::Arr(ts)) => {
            assert_eq!(ts.len(), 2);
            let kinds: Vec<&str> = ts
                .iter()
                .filter_map(|x| x.get("kind").and_then(Json::as_str))
                .collect();
            assert!(kinds.contains(&"procedure"));
            assert!(kinds.contains(&"text_leaf"));
        }
        other => panic!("targets: {other:?}"),
    }

    let d = ok(&call(
        &mut svc,
        "lab.analysis.attribution_design",
        Json::obj([
            ("design_ref", Json::str("design:d1")),
            (
                "targets",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("procedure")),
                    ("ref", Json::str("proc:main")),
                ])]),
            ),
            ("arms", Json::Arr(vec![Json::str("a"), Json::str("b")])),
            ("match_spec_ref", Json::str("ms:1")),
            (
                "budget_allocation",
                Json::obj([("regime", Json::str("uniform"))]),
            ),
        ]),
    ));
    let design = d.get("design").expect("design");
    assert_eq!(
        design.get("schema").and_then(Json::as_str),
        Some("hh-attribution-design/1")
    );
    assert_eq!(design.get("method").and_then(Json::as_str), Some("M1"));
    // Byte-identity with the owning helper.
    let expected = hh_analysis::ops::attribution_design(
        "design:d1",
        vec![Json::obj([
            ("kind", Json::str("procedure")),
            ("ref", Json::str("proc:main")),
        ])],
        &["a".to_string(), "b".to_string()],
        Some("ms:1"),
        Some(Json::obj([("regime", Json::str("uniform"))])),
    );
    assert_eq!(design.to_canonical_string(), expected.to_canonical_string());
}

// ── schema registration + refusals ──────────────────────────────────────────

#[test]
fn the_s54_ops_are_schema_registered() {
    for op in [
        "lab.debt.evaluate",
        "lab.debt.index",
        "lab.debt.report",
        "lab.model.snapshot_claim",
        "lab.model.regression",
        "lab.analysis.component_targets",
        "lab.analysis.attribution_design",
    ] {
        assert!(
            hh_embed_schema::ops::lookup(op).is_some(),
            "{op} not in the op registry"
        );
    }
}

#[test]
fn malformed_params_refuse_typed() {
    let mut svc = service();
    hello(&mut svc);
    // Missing `record` → a typed refusal, never a panic.
    let r = call(&mut svc, "lab.debt.evaluate", Json::obj([]));
    assert!(r.get("error").is_some(), "expected refusal: {r:?}");
    // A bad `profile_change.kind` spelling refuses too.
    let r = call(
        &mut svc,
        "lab.debt.evaluate",
        Json::obj([
            (
                "record",
                record_json(&debt_record("r1", ExpiryKind::ModelVersionChange)),
            ),
            (
                "observables",
                Json::obj([(
                    "profile_change",
                    Json::obj([("kind", Json::str("renamed"))]),
                )]),
            ),
        ]),
    );
    assert!(r.get("error").is_some(), "expected refusal: {r:?}");
}
