//! S6.4 boundary tests — the `lab.coevolution.*` + `lab.org_policy.*`
//! surface over `hh-embed` (§5h.8 R-2.9.8; §5i.1 6d; ADR-0325;
//! AC-R-2.9.8-{4,6,8,11,13}; AC-R-2.12.6-12): capability gating
//! (`experimental` + `serves_measurement`), the claims-only
//! `import_snapshot`, the bind guard, the cycle sidecar ops over
//! LabDocs, the consolidation proposal → retirement-record bridge,
//! the org-policy recipe + fleet-default removal-test records, and
//! the `lab.debt.post_import_sweep` mint path. HarnessHarness never
//! trains — every op is records-in/records-out.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_embed::service::{EmbedService, ServiceConfig};
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

fn test_dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-embed-s64-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn service() -> (PathBuf, EmbedService) {
    let root = test_dir("svc");
    (
        root.clone(),
        EmbedService::open(ServiceConfig {
            store_root: root.join("store"),
            kernel_version_id: "hh-kernel/0.1.0".into(),
            workspace_root: root.join("ws"),
            holder: "s6-4".into(),
        })
        .unwrap(),
    )
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

#[cfg(feature = "tier-c4")]
fn ok(r: &Json) -> Json {
    r.get("result")
        .cloned()
        .unwrap_or_else(|| panic!("expected result: {r:?}"))
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

fn hello_coe(svc: &mut EmbedService) {
    hello(svc, &[("experimental", true), ("serves_measurement", true)]);
}

#[cfg(feature = "tier-c4")]
fn claim() -> Json {
    Json::obj([
        ("provider", Json::str("prov:acme")),
        ("model_id", Json::str("model:m-7")),
        ("snapshot_id", Json::str("snap:trained-1")),
        ("base_snapshot_ref", Json::str("snap:base-1")),
        (
            "trained_under",
            Json::Arr(vec![Json::obj([
                ("definition_semantic_id", Json::str("def:harness/a")),
                ("profile_semantic_id", Json::str("profile:minimal:0")),
                ("bundle_id", Json::str("bundle:t1")),
            ])]),
        ),
        ("policy_version_exposed", Json::str("supported")),
    ])
}

#[cfg(feature = "tier-c4")]
fn sealed_defs() -> Json {
    Json::Arr(vec![
        Json::obj([
            ("definition_semantic_id", Json::str("def:harness/a")),
            ("profile_semantic_id", Json::str("profile:minimal:0")),
        ]),
        Json::obj([
            ("definition_semantic_id", Json::str("def:harness/b")),
            ("profile_semantic_id", Json::str("profile:minimal:0")),
        ]),
    ])
}

// ── gating legs (every build) ───────────────────────────────────────────────

/// The co-evolution ops are experimental-gated like every Group L op.
#[test]
fn coevolution_ops_require_experimental_optin() {
    let (_root, mut svc) = service();
    hello(&mut svc, &[("serves_measurement", true)]);
    let r = call(&mut svc, "lab.coevolution.import_snapshot", Json::obj([]));
    assert_eq!(err_kind(&r), "ExperimentalRequired", "{r:?}");
}

/// …and capability-gated (`serves_measurement`).
#[test]
fn coevolution_ops_require_measurement_capability() {
    let (_root, mut svc) = service();
    hello(&mut svc, &[("experimental", true)]);
    let r = call(&mut svc, "lab.coevolution.import_snapshot", Json::obj([]));
    assert_eq!(err_kind(&r), "CapabilityNotDeclared", "{r:?}");
}

/// A `--no-default-features` build answers `Unsupported{by: "tier-c4"}`
/// — the tier is removable (CC6; AC-R-2.9.8-11).
#[cfg(not(feature = "tier-c4"))]
#[test]
fn coevolution_ops_unsupported_without_tier_c4() {
    let (_root, mut svc) = service();
    hello_coe(&mut svc);
    let r = call(&mut svc, "lab.coevolution.import_snapshot", Json::obj([]));
    assert_eq!(err_kind(&r), "Unsupported", "{r:?}");
}

// ── the service legs (tier-c4) ──────────────────────────────────────────────

#[cfg(feature = "tier-c4")]
mod c4 {
    use super::*;

    /// The import is claims-only: `pinned = false`/`reported`, the
    /// `trained_under`/`unknown` compatibility set, the
    /// `scoped_rules` sweep inputs, `maturity: research-grade`,
    /// `label: preview`.
    #[test]
    fn import_snapshot_round_trips_through_the_boundary() {
        let (_root, mut svc) = service();
        hello_coe(&mut svc);
        let r = call(
            &mut svc,
            "lab.coevolution.import_snapshot",
            Json::obj([
                ("claim", claim()),
                ("sealed_definitions", sealed_defs()),
                (
                    "known_snapshot_ids",
                    Json::Arr(vec![Json::str("snap:base-1")]),
                ),
            ]),
        );
        let res = ok(&r);
        let snap = res.get("snapshot_record").unwrap();
        assert_eq!(snap.get("pinned"), Some(&Json::Bool(false)));
        assert_eq!(snap.get("status").and_then(Json::as_str), Some("reported"));
        let compat = res
            .get("compatibility")
            .and_then(|c| match c {
                Json::Arr(a) => Some(a),
                _ => None,
            })
            .unwrap();
        assert_eq!(compat.len(), 2);
        let statuses: BTreeMap<_, _> = compat
            .iter()
            .map(|c| {
                (
                    c.get("definition_semantic_id")
                        .and_then(Json::as_str)
                        .unwrap()
                        .to_string(),
                    c.get("status").cloned().unwrap_or(Json::Null),
                )
            })
            .collect();
        assert_eq!(statuses["def:harness/a"], Json::str("trained_under"));
        assert_eq!(statuses["def:harness/b"], Json::str("unknown"));
        assert_eq!(
            res.get("maturity").and_then(Json::as_str),
            Some("research-grade")
        );
        assert_eq!(res.get("label").and_then(Json::as_str), Some("preview"));

        // A ghost base refuses the typed `BaseSnapshotUnknown`.
        let mut ghost = claim();
        if let Json::Obj(ref mut m) = ghost {
            m.insert("base_snapshot_ref".into(), Json::str("snap:ghost"));
        }
        let r = call(
            &mut svc,
            "lab.coevolution.import_snapshot",
            Json::obj([("claim", ghost)]),
        );
        assert_eq!(err_kind(&r), "Refused", "{r:?}");
    }

    /// `guard_at_bind` — an evolution-origin arm with no record is
    /// `refused{ExploratoryOnly}`; a registered arm annotates
    /// `unknown`; `allow_unverified_snapshot` lifts the evolution leg.
    #[test]
    fn guard_at_bind_dispatches_the_closed_verdicts() {
        let (_root, mut svc) = service();
        hello_coe(&mut svc);
        let r = call(
            &mut svc,
            "lab.coevolution.guard_at_bind",
            Json::obj([("arm_origin", Json::str("evolution"))]),
        );
        let res = ok(&r);
        assert_eq!(res.get("guard").and_then(Json::as_str), Some("refused"));
        assert_eq!(
            res.get("code").and_then(Json::as_str),
            Some("ExploratoryOnly")
        );

        let r = call(
            &mut svc,
            "lab.coevolution.guard_at_bind",
            Json::obj([
                ("arm_origin", Json::str("evolution")),
                ("allow_unverified_snapshot", Json::Bool(true)),
            ]),
        );
        assert_eq!(
            ok(&r).get("guard").and_then(Json::as_str),
            Some("annotated")
        );

        let r = call(
            &mut svc,
            "lab.coevolution.guard_at_bind",
            Json::obj([("arm_origin", Json::str("registered"))]),
        );
        let res = ok(&r);
        assert_eq!(res.get("guard").and_then(Json::as_str), Some("annotated"));
        assert_eq!(
            res.get("compatibility").and_then(Json::as_str),
            Some("unknown")
        );
    }

    /// The cycle sidecar drives through the boundary:
    /// `open → begin → complete → next → record → stop`; the record
    /// round-trips LabDocs (`deposit_ref` on every mutator).
    #[test]
    fn cycle_sidecar_drives_through_the_boundary() {
        let (_root, mut svc) = service();
        hello_coe(&mut svc);
        let r = call(
            &mut svc,
            "lab.coevolution.cycle_open",
            Json::obj([
                (
                    "policy",
                    Json::obj([
                        ("weight_phase_budget", Json::Int(100)),
                        (
                            "harness_search_budget",
                            Json::obj([("search", Json::Int(10))]),
                        ),
                        ("switch_rule", Json::str("cycle_count")),
                        ("max_cycles", Json::Int(1)),
                        ("broken_policy", Json::str("re_search")),
                    ]),
                ),
                ("lineage_ref", Json::str("lineage:l1")),
            ]),
        );
        let res = ok(&r);
        let key = res.get("cycle").and_then(Json::as_str).unwrap().to_string();
        assert!(res.get("deposit_ref").is_some());
        assert_eq!(
            res.get("record")
                .and_then(|r| r.get("maturity"))
                .and_then(Json::as_str),
            Some("research-grade")
        );

        // begin/complete a harness_search phase — the plan advances.
        call(
            &mut svc,
            "lab.coevolution.cycle_begin_phase",
            Json::obj([
                ("cycle", Json::str(&key)),
                ("phase", Json::str("harness_search")),
                ("inputs", Json::obj([("base_ref", Json::str("snap:b0"))])),
            ]),
        );
        let r = call(
            &mut svc,
            "lab.coevolution.cycle_complete_phase",
            Json::obj([
                ("cycle", Json::str(&key)),
                ("outputs", Json::obj([])),
                ("verdict", Json::str("completed")),
                ("budgets", Json::obj([("search", Json::Int(10))])),
            ]),
        );
        assert!(ok(&r).get("deposit_ref").is_some());
        let r = call(
            &mut svc,
            "lab.coevolution.cycle_next",
            Json::obj([("cycle", Json::str(&key))]),
        );
        assert_eq!(
            ok(&r)
                .get("plan")
                .and_then(|p| p.get("phase"))
                .and_then(Json::as_str),
            Some("weight_update")
        );

        // The record reports matched_total.
        let r = call(
            &mut svc,
            "lab.coevolution.cycle_record",
            Json::obj([("cycle", Json::str(&key))]),
        );
        let rec = ok(&r).get("record").unwrap().clone();
        assert_eq!(
            rec.get("budgets")
                .and_then(|b| b.get("search"))
                .and_then(Json::as_int),
            Some(10)
        );

        // Stop — the record seals `stop_reason`.
        call(
            &mut svc,
            "lab.coevolution.cycle_stop",
            Json::obj([
                ("cycle", Json::str(&key)),
                ("reason", Json::str("operator")),
            ]),
        );
        let r = call(
            &mut svc,
            "lab.coevolution.cycle_stop",
            Json::obj([
                ("cycle", Json::str(&key)),
                (
                    "reason",
                    Json::obj([("operator", Json::obj([("reason", Json::str("done"))]))]),
                ),
            ]),
        );
        let rec = ok(&r).get("record").unwrap().clone();
        assert!(rec.get("stop_reason").is_some());

        // A restore-less cycle key refuses honestly.
        let r = call(
            &mut svc,
            "lab.coevolution.cycle_next",
            Json::obj([("cycle", Json::str("cycle:ghost"))]),
        );
        assert_eq!(err_kind(&r), "SchemaViolation", "{r:?}");
    }

    /// The proposal → retirement-record bridge: the proposal is a
    /// record (never an applied diff — preview label), the
    /// `RetirementRecord` names the `ConsolidationReport` id.
    #[test]
    fn consolidation_proposal_and_retirement_record() {
        let (_root, mut svc) = service();
        hello_coe(&mut svc);
        let prov = hh_provenance::ProvenanceRecord::minted(
            hh_provenance::Origin::human("test:proposer", hh_provenance::HumanRole::Principal),
            hh_provenance::PersistenceScope::Run,
            1,
        )
        .to_json();
        let r = call(
            &mut svc,
            "lab.coevolution.propose_consolidation",
            Json::obj([
                ("target_rule", Json::str("rule:ok")),
                ("lessons", Json::Arr(vec![Json::str("lesson:1")])),
                ("snapshot_in", Json::str("snap:trained-1")),
                ("debt_refs", Json::Arr(vec![Json::str("debt:rule:ok")])),
                ("provenance", prov.clone()),
            ]),
        );
        let proposal = ok(&r).get("proposal").unwrap().clone();
        assert_eq!(
            proposal.get("label").and_then(Json::as_str),
            Some("preview")
        );
        assert_eq!(
            proposal.get("maturity").and_then(Json::as_str),
            Some("research-grade")
        );

        // A sealed report → the retirement record names it.
        let mut report = hh_lab::coevolution::ConsolidationReport {
            report_id: String::new(),
            proposal_ref: None,
            target_rule: "rule:ok".into(),
            snapshot_ref: "snap:trained-1".into(),
            lessons: vec!["lesson:1".into()],
            verdict: hh_lab::coevolution::ConsolidationVerdict::Absorbed,
            experiment_ref: "exp:ret-1".into(),
            evidence_refs: vec!["report:t".into()],
            debt_refs: vec!["debt:rule:ok".into()],
        };
        report.seal();
        let decided_by = hh_provenance::ProvenanceRecord::minted(
            hh_provenance::Origin::human("test:sealer", hh_provenance::HumanRole::Principal),
            hh_provenance::PersistenceScope::Run,
            2,
        )
        .to_json();
        let r = call(
            &mut svc,
            "lab.coevolution.consolidation_retirement_record",
            Json::obj([
                ("report", report.to_json()),
                ("verdict_ref", Json::str("verdict:1")),
                ("decided_by", decided_by),
            ]),
        );
        let rec = ok(&r).get("retirement_record").unwrap().clone();
        assert_eq!(
            rec.get("removal_test_report_ref").and_then(Json::as_str),
            Some(report.report_id.as_str())
        );
    }

    /// `lab.org_policy.recipe` builds `lab/org-policy-v1`;
    /// `lab.org_policy.default_removal_tests` mints one conditioned
    /// debt record per fleet default — each naming the recipe as its
    /// `retirement_experiment` template.
    #[test]
    fn org_policy_recipe_and_fleet_default_removal_tests() {
        let (_root, mut svc) = service();
        hello_coe(&mut svc);
        let pins = Json::obj([
            ("suite_ref", Json::str("suite:h")),
            ("held_out_split_ref", Json::str("split:h")),
            ("split_assignment_ref", Json::str("split:a")),
            ("registry_snapshot_id", Json::str("reg:1")),
            ("eval_budget", Json::str("budget:e")),
            ("search_budget", Json::str("budget:s")),
            ("experiment_budget", Json::str("budget:x")),
            ("instrument_budget", Json::str("budget:i")),
            ("analysis_plan_ref", Json::str("plan:1")),
            ("task_split_hash", Json::str("sha256:t")),
        ]);
        let own = Json::obj([
            (
                "pricing_table_ref",
                Json::obj([
                    ("table_id", Json::str("pricing:t")),
                    ("version", Json::str("v1")),
                    ("pin", Json::str("sha256:p")),
                ]),
            ),
            (
                "factors",
                Json::Arr(vec![Json::obj([
                    ("name", Json::str("unattended_policy")),
                    (
                        "levels",
                        Json::Arr(vec![
                            Json::obj([
                                ("level_id", Json::str("deny")),
                                ("content_ref", Json::str("sha256:d")),
                            ]),
                            Json::obj([
                                ("level_id", Json::str("defer")),
                                ("content_ref", Json::str("sha256:f")),
                            ]),
                        ]),
                    ),
                ])]),
            ),
            (
                "arms",
                Json::Arr(vec![Json::obj([
                    ("arm_id", Json::str("arm:d")),
                    ("hypothesis", Json::str("the default holds")),
                    (
                        "levels",
                        Json::obj([("unattended_policy", Json::str("deny"))]),
                    ),
                    (
                        "artifact",
                        Json::obj([
                            ("semantic_id", Json::str("fleet:d")),
                            ("version_id", Json::str("sha256:ff")),
                        ]),
                    ),
                ])]),
            ),
        ]);
        let r = call(
            &mut svc,
            "lab.org_policy.recipe",
            Json::obj([
                ("pins", pins),
                ("own", own),
                ("registered_at", Json::Int(1)),
            ]),
        );
        let spec = ok(&r).get("spec").unwrap().clone();
        assert_eq!(
            spec.get("design")
                .and_then(|d| d.get("id"))
                .and_then(Json::as_str),
            Some("lab/org-policy-v1")
        );

        // The fleet defaults' removal-test records.
        let r = call(
            &mut svc,
            "lab.org_policy.default_removal_tests",
            Json::obj([
                ("fleet", Json::str("fleet:prod")),
                (
                    "owner",
                    Json::obj([
                        ("team", Json::Bool(true)),
                        ("id", Json::str("team:platform")),
                        ("reach_via", Json::Arr(vec![Json::str("sink:ops")])),
                    ]),
                ),
                ("recipe_spec_ref", Json::str("spec:lab/org-policy-v1")),
                ("expires_at_ms", Json::Int(9_999_999)),
            ]),
        );
        let records = ok(&r)
            .get("records")
            .and_then(|r| match r {
                Json::Arr(a) => Some(a.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(records.len(), 4, "ADR-0206's closed default set");
        for rec in &records {
            let test = rec.get("removal_test").unwrap();
            assert_eq!(
                test.get("kind").and_then(Json::as_str),
                Some("retirement_experiment")
            );
            assert_eq!(
                test.get("template_ref").and_then(Json::as_str),
                Some("spec:lab/org-policy-v1")
            );
        }
    }

    /// `lab.debt.post_import_sweep` — the covered debt mints
    /// `status.changed{trigger: model_version_change}`; the
    /// `sweep.completed` row attributes `kind: post_import` (the
    /// boundary mints the book of record — the Lab-side partition is
    /// the records-in input).
    #[test]
    fn post_import_sweep_mints_the_book_of_record() {
        let (_root, mut svc) = service();
        hello_coe(&mut svc);
        let run = ok(&call(&mut svc, "lab.debt.manager_open", Json::obj([])))
            .get("run_id")
            .and_then(Json::as_str)
            .unwrap()
            .to_string();
        let record = Json::obj([
            ("manager_id", Json::str("m1")),
            ("maturity", Json::str("instrument-grade")),
            ("record", manager_record_json("m1")),
        ]);
        let r = call(
            &mut svc,
            "lab.debt.register",
            Json::obj([
                ("run_id", Json::str(&run)),
                ("record", record.get("record").unwrap().clone()),
            ]),
        );
        assert!(r.get("result").is_some(), "register: {r:?}");

        let debt = conditioned_debt_json("rule:cov", "model:m-7");
        let r = call(
            &mut svc,
            "lab.debt.post_import_sweep",
            Json::obj([
                ("run_id", Json::str(&run)),
                ("manager_id", Json::str("m1")),
                ("snapshot_ref", Json::str("snap:imported-1")),
                ("covered", Json::Arr(vec![Json::str("rule:cov")])),
                ("scheduled", Json::Arr(vec![])),
                (
                    "entries",
                    Json::Arr(vec![Json::obj([
                        ("debt_ref", Json::str("debt:rule:cov")),
                        ("home", Json::Int(1)),
                        ("record", debt),
                    ])]),
                ),
            ]),
        );
        let report = ok(&r);
        let transitions = report
            .get("transitions")
            .and_then(|t| match t {
                Json::Arr(a) => Some(a),
                _ => None,
            })
            .expect("transitions array");
        assert_eq!(transitions.len(), 1);
        assert_eq!(
            transitions[0].get("debt_ref").and_then(Json::as_str),
            Some("debt:rule:cov")
        );
        assert_eq!(
            transitions[0].get("trigger").and_then(Json::as_str),
            Some("model_version_change")
        );
        assert_eq!(
            transitions[0].get("evidence_ref").and_then(Json::as_str),
            Some("snap:imported-1")
        );
    }

    /// A service record JSON (the canonical codec — same shape the
    /// s6_1b boundary test mints).
    fn manager_record_json(manager_id: &str) -> Json {
        let reflexive = hh_debt::reflexive::reflexive_record(
            manager_id,
            "test:owner",
            vec!["sink:ops".into()],
            hh_ontology::debt::DeadWeightWindow {
                model_version_changes: 2,
            },
            60_000,
            1,
        )
        .unwrap();
        hh_debt::records::DebtManagerRecord {
            manager_id: manager_id.to_string(),
            maturity: "instrument-grade".into(),
            policy: hh_ontology::debt::DebtPolicy {
                notice_sinks: vec!["sink:ops".into()],
                ..hh_ontology::debt::DebtPolicy::default()
            },
            reflexive_debt: reflexive,
        }
        .to_json()
    }

    /// A `model_version_change`-conditioned debt record JSON.
    fn conditioned_debt_json(rule_id: &str, model_id: &str) -> Json {
        let prov = hh_provenance::ProvenanceRecord::minted(
            hh_provenance::Origin::human("test:owner", hh_provenance::HumanRole::Author),
            hh_provenance::PersistenceScope::Definition,
            1,
        );
        let record = hh_hir::records::AssumptionDebtRecord {
            rule_id: rule_id.to_string(),
            hypothesis: hh_hir::leaves::Text::new(
                format!("{rule_id} conditions a snapshot-scoped claim"),
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
                kind: hh_ontology::debt::ExpiryKind::ModelVersionChange,
                value: None,
            },
            removal_test_ref: format!("tmpl:{rule_id}"),
            status: hh_ontology::debt::DebtStatus::Active,
            debt_class: Some(hh_ontology::debt::DebtClass::ModelConditioned),
            hypothesis_typed: None,
            scope: Some(hh_ontology::debt::DebtScope {
                model_selectors: vec![hh_ontology::debt::ModelSelector::Exact {
                    model_id: model_id.into(),
                }],
                task_classes: vec![],
                roles: vec![],
            }),
            expiry: Some(hh_ontology::debt::DebtExpiry {
                condition: hh_ontology::debt::ExpiryKind::ModelVersionChange,
                params: hh_ontology::debt::ExpiryParams::default(),
            }),
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
}
