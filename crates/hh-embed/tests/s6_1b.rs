//! S6.1b boundary tests — the `lab.debt.*` manager-service surface over
//! `hh-embed` (records in, records out; one Store; §5h.6 R-2.9.6;
//! AC-R-2.9.6-10 — the manager is out-of-process with no private verb).
//! The gating legs (experimental opt-in → `serves_measurement`
//! capability → strict codec → the closed refusal table) run on every
//! build; the service legs ride `tier-c4`; a `--no-default-features`
//! build answers `Unsupported{by: "tier-c4"}` (CC6).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_embed::service::{EmbedService, ServiceConfig};
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

fn test_dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-embed-debt-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn service() -> (PathBuf, EmbedService) {
    let root = test_dir("svc");
    let svc = EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "conformance".into(),
    })
    .unwrap();
    (root, svc)
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn err_kind(r: &Json) -> String {
    r.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("kind"))
        .and_then(Json::as_str)
        .or_else(|| {
            r.get("error")
                .and_then(|e| e.get("message"))
                .and_then(Json::as_str)
        })
        .unwrap_or_default()
        .to_string()
}

fn err_reason(r: &Json) -> String {
    r.get("error")
        .and_then(|e| e.get("data"))
        .and_then(|d| d.get("reason"))
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string()
}

fn hello(svc: &mut EmbedService, caps: &[(&str, bool)]) {
    let mut m = BTreeMap::new();
    for (k, v) in caps {
        m.insert(k.to_string(), Json::Bool(*v));
    }
    let r = call(
        svc,
        "hello",
        Json::obj([
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj([
                    ("name", Json::str("t")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            ("capabilities", Json::Obj(m)),
        ]),
    );
    assert!(r.get("result").is_some(), "hello: {r:?}");
}

fn hello_debt(svc: &mut EmbedService) {
    hello(svc, &[("experimental", true), ("serves_measurement", true)]);
}

// ── gating legs (every build) ───────────────────────────────────────────────

/// The manager ops are experimental-gated like every Group L op.
#[test]
fn debt_manager_ops_require_experimental_optin() {
    let (_root, mut svc) = service();
    hello(&mut svc, &[("serves_measurement", true)]);
    let r = call(&mut svc, "lab.debt.sweep", Json::obj([]));
    assert_eq!(err_kind(&r), "ExperimentalRequired", "{r:?}");
}

/// …and capability-gated (`serves_measurement`).
#[test]
fn debt_manager_ops_require_measurement_capability() {
    let (_root, mut svc) = service();
    hello(&mut svc, &[("experimental", true)]);
    let r = call(&mut svc, "lab.debt.sweep", Json::obj([]));
    assert_eq!(err_kind(&r), "CapabilityNotDeclared", "{r:?}");
}

/// A `--no-default-features` build answers `Unsupported{by: "tier-c4"}`
/// — the tier is removable (CC6) and the ops stay in the schema (CC7).
#[cfg(not(feature = "tier-c4"))]
#[test]
fn debt_manager_ops_unsupported_without_tier_c4() {
    let (_root, mut svc) = service();
    hello_debt(&mut svc);
    let r = call(&mut svc, "lab.debt.sweep", Json::obj([]));
    assert_eq!(err_kind(&r), "Unsupported", "{r:?}");
}

// ── the service legs (tier-c4) ──────────────────────────────────────────────

#[cfg(feature = "tier-c4")]
mod c4 {
    use super::*;
    use hh_ontology::debt::{DeadWeightWindow, DebtPolicy};

    /// A service record built through the canonical codec — the manager's
    /// own home-16 reflexive record round-trips member-level.
    fn manager_record_json(manager_id: &str) -> Json {
        let reflexive = hh_debt::reflexive::reflexive_record(
            manager_id,
            "test:owner",
            vec!["sink:ops".into()],
            DeadWeightWindow {
                model_version_changes: 2,
            },
            60_000,
            1,
        )
        .unwrap();
        hh_debt::records::DebtManagerRecord {
            manager_id: manager_id.to_string(),
            maturity: "instrument-grade".into(),
            policy: DebtPolicy {
                notice_sinks: vec!["sink:ops".into()],
                ..DebtPolicy::default()
            },
            reflexive_debt: reflexive,
        }
        .to_json()
    }

    /// A conditioned-rule debt record JSON (home 1 — `harness_rule`
    /// home's field; `retirement_experiment` with a resolved template the
    /// sweep entry carries).
    fn rule_debt_json(rule_id: &str) -> Json {
        let prov = hh_provenance::ProvenanceRecord::minted(
            hh_provenance::Origin::human("test:owner", hh_provenance::HumanRole::Author),
            hh_provenance::PersistenceScope::Definition,
            1,
        );
        let record = hh_hir::records::AssumptionDebtRecord {
            rule_id: rule_id.to_string(),
            hypothesis: hh_hir::leaves::Text::new(
                format!("{rule_id} conditions a deficiency claim"),
                "test:owner",
                prov.clone(),
            ),
            evidence_refs: vec![hh_ontology::debt::EvidenceRef::legacy(format!(
                "ev:{rule_id}"
            ))],
            owner: hh_ontology::debt::OwnerRef {
                team: false,
                id: "test:owner".into(),
                reach_via: vec!["sink:ops".into()],
            },
            expiry_condition: hh_ontology::debt::ExpiryCondition {
                kind: hh_ontology::debt::ExpiryKind::Date,
                value: Some("99999999999".into()),
            },
            removal_test_ref: format!("tmpl:{rule_id}"),
            status: hh_ontology::debt::DebtStatus::Active,
            debt_class: Some(hh_ontology::debt::DebtClass::Hypothesized),
            hypothesis_typed: None,
            scope: Some(hh_ontology::debt::DebtScope::default()),
            expiry: None,
            runway_ms: None,
            revalidation: None,
            removal_test: Some(hh_ontology::debt::RemovalTest {
                template_ref: Some(format!("tmpl:{rule_id}")),
                ..hh_ontology::debt::RemovalTest::new(
                    hh_ontology::debt::RemovalTestKind::RetirementExperiment,
                )
            }),
            created_by: Some(prov),
            created_at: Some(1),
            supersedes: None,
        };
        hh_hir::debt_json(&record, false)
    }

    fn human_prov(author: &str) -> Json {
        hh_provenance::ProvenanceRecord::minted(
            hh_provenance::Origin::human(author, hh_provenance::HumanRole::Principal),
            hh_provenance::PersistenceScope::Run,
            1,
        )
        .to_json()
    }

    /// `manager_open` binds the service's registry audit run (the S5.4
    /// convention); `register` mints the service record + reports the
    /// reflexive debt ref.
    #[test]
    fn manager_open_and_register_land_the_service_record() {
        let (_root, mut svc) = service();
        hello_debt(&mut svc);
        let r = call(&mut svc, "lab.debt.manager_open", Json::obj([]));
        let run = r
            .get("result")
            .and_then(|v| v.get("run_id"))
            .and_then(Json::as_str)
            .unwrap_or_else(|| panic!("manager_open: {r:?}"))
            .to_string();
        let r = call(
            &mut svc,
            "lab.debt.register",
            Json::obj([
                ("run_id", Json::str(&run)),
                ("record", manager_record_json("m1")),
            ]),
        );
        let res = r.get("result").unwrap_or_else(|| panic!("register: {r:?}"));
        assert_eq!(res.get("manager_id").and_then(Json::as_str), Some("m1"));
        assert!(res
            .get("reflexive_debt_ref")
            .and_then(Json::as_str)
            .unwrap_or("")
            .starts_with("debt:assumption_debt_manager:"));
    }

    /// A malformed service record refuses `SchemaViolation` — strict
    /// codec, never a partial decode.
    #[test]
    fn register_rejects_a_malformed_record() {
        let (_root, mut svc) = service();
        hello_debt(&mut svc);
        let r = call(&mut svc, "lab.debt.manager_open", Json::obj([]));
        let run = r
            .get("result")
            .and_then(|v| v.get("run_id"))
            .and_then(Json::as_str)
            .unwrap()
            .to_string();
        let r = call(
            &mut svc,
            "lab.debt.register",
            Json::obj([
                ("run_id", Json::str(&run)),
                ("record", Json::obj([("manager_id", Json::str("m1"))])),
            ]),
        );
        assert_eq!(err_kind(&r), "SchemaViolation", "{r:?}");
    }

    /// The `sweep` → `settle` → `retire`/`propose` loop over the boundary
    /// (AC-R-2.9.6-4/-10): probation opens ledgered, the overrun cites
    /// the ledgered row, `settle{pass}` + human `retire` land the
    /// `→ retired` transition; `propose` stays a proposal.
    #[test]
    fn sweep_settle_retire_over_the_boundary() {
        let (_root, mut svc) = service();
        hello_debt(&mut svc);
        let run = call(&mut svc, "lab.debt.manager_open", Json::obj([]))
            .get("result")
            .and_then(|v| v.get("run_id"))
            .and_then(Json::as_str)
            .unwrap()
            .to_string();
        call(
            &mut svc,
            "lab.debt.register",
            Json::obj([
                ("run_id", Json::str(&run)),
                ("record", manager_record_json("m1")),
            ]),
        );
        // Sweep: the hypothesized record opens probation; the unresolved
        // template defers `template_unresolved` (records-in — the caller
        // carries the resolved template).
        let entry = Json::obj([
            ("debt_ref", Json::str("debt:r1")),
            ("home", Json::Int(1)),
            ("record", rule_debt_json("r1")),
        ]);
        let r = call(
            &mut svc,
            "lab.debt.sweep",
            Json::obj([
                ("run_id", Json::str(&run)),
                ("manager_id", Json::str("m1")),
                ("entries", Json::Arr(vec![entry.clone()])),
                ("now_ms", Json::Int(1_000)),
            ]),
        );
        let report = r.get("result").unwrap_or_else(|| panic!("sweep: {r:?}"));
        assert_eq!(
            report.get("probation_opened").and_then(|v| match v {
                Json::Arr(a) => Some(a.len()),
                _ => None,
            }),
            Some(1)
        );
        assert!(report
            .get("deferred")
            .and_then(|v| match v {
                Json::Arr(a) => Some(a),
                _ => None,
            })
            .unwrap()
            .iter()
            .any(|d| d.get("reason").and_then(Json::as_str) == Some("template_unresolved")));

        // `settle{pass}` — the verdict mints `removal_test.settled`.
        let r = call(
            &mut svc,
            "lab.debt.settle",
            Json::obj([
                ("run_id", Json::str(&run)),
                ("debt_ref", Json::str("debt:r1")),
                ("kind", Json::str("retirement_experiment")),
                ("report_ref", Json::str("report:1")),
                ("verdict", Json::str("pass")),
                ("now_ms", Json::Int(2_000)),
            ]),
        );
        assert!(r.get("result").is_some(), "settle: {r:?}");

        // `retire` under evolution provenance refuses (human-sealed only).
        let evo_prov = hh_provenance::ProvenanceRecord::minted(
            hh_provenance::Origin::evolution("candidate:1", "hyp:1"),
            hh_provenance::PersistenceScope::Run,
            1,
        )
        .to_json();
        let rationale = hh_hir::leaves::Text::new(
            "the removal test passed",
            "test:owner",
            hh_provenance::ProvenanceRecord::minted(
                hh_provenance::Origin::human("test:owner", hh_provenance::HumanRole::Author),
                hh_provenance::PersistenceScope::Run,
                1,
            ),
        )
        .to_json();
        let record = |decided_by: Json| {
            Json::obj([
                ("removal_test_report_ref", Json::str("report:1")),
                ("verdict_ref", Json::str("verdict:1")),
                ("decided_by", decided_by),
                ("rationale", rationale.clone()),
            ])
        };
        let r = call(
            &mut svc,
            "lab.debt.retire",
            Json::obj([
                ("run_id", Json::str(&run)),
                ("debt_ref", Json::str("debt:r1")),
                ("record", record(evo_prov)),
            ]),
        );
        assert_eq!(err_kind(&r), "Refused", "{r:?}");

        // The human seal retires — `supersedes{reason: expiry}` +
        // `→ retired`.
        let r = call(
            &mut svc,
            "lab.debt.retire",
            Json::obj([
                ("run_id", Json::str(&run)),
                ("debt_ref", Json::str("debt:r1")),
                ("record", record(human_prov("test:sealer"))),
            ]),
        );
        let res = r.get("result").unwrap_or_else(|| panic!("retire: {r:?}"));
        assert_eq!(
            res.get("supersedes_reason").and_then(Json::as_str),
            Some("expiry")
        );

        // `propose` under evolution provenance on a *fresh* debt —
        // `retirement_not_evidenced` without a pass verdict.
        let r = call(
            &mut svc,
            "lab.debt.propose",
            Json::obj([
                ("run_id", Json::str(&run)),
                ("debt_ref", Json::str("debt:r2")),
                ("rule_id", Json::str("r2")),
                (
                    "diff",
                    Json::obj([
                        ("removed_rules", Json::Arr(vec![Json::str("r2")])),
                        ("diff_ref", Json::str("diff:1")),
                    ]),
                ),
                (
                    "proposed_by",
                    hh_provenance::ProvenanceRecord::minted(
                        hh_provenance::Origin::evolution("candidate:1", "hyp:1"),
                        hh_provenance::PersistenceScope::Run,
                        1,
                    )
                    .to_json(),
                ),
            ]),
        );
        assert_eq!(err_kind(&r), "Refused", "{r:?}");
        assert_eq!(err_reason(&r), "retirement_not_evidenced", "{r:?}");
    }

    /// An unknown `lab.debt.*` member is a typed refusal — the dispatch
    /// arm is closed over the declared op set.
    #[test]
    fn debt_manager_unknown_op_refuses() {
        let (_root, mut svc) = service();
        hello_debt(&mut svc);
        let r = call(&mut svc, "lab.debt.not_an_op", Json::obj([]));
        assert_eq!(err_kind(&r), "SchemaViolation", "{r:?}");
    }
}
