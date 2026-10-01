//! S5.4 — the live assumption-debt slice (§5h.6 §2/§3/§6; R-2.9.6¹) plus
//! the model-compat bridge (§5h.8; R-2.9.8¹):
//!
//! - `evaluate_debt` — the all-home trigger evaluator: the delegated §05b
//!   legs (per `expiry_condition.kind`), the manager-level families
//!   (`profile_change`, evidence supersession, probation overrun), the
//!   `expired`-subsumes-`expiring` merge, the `model_conditioned`
//!   revalidation grade gate, `retired` terminal, idempotence.
//! - `debt_index` — the stored-status ⊕ transition-fold view with
//!   `last_trigger`/`staleness_reasons`, sorted rows, rebuild equality.
//! - `route_notices` — owner `reach_via` ∩ policy `notice_sinks`;
//!   unreachable owners land under `unrouted`, never dropped.
//! - `run_regression_suite` — verified/drifted/broken/unknown verdicts,
//!   `regression_drifted_rules` ⇒ `expiring` (ADR-0203),
//!   `synthetic_snapshot_claim`, `guard_at_bind`, `snapshot_span`.

use hh_compiler::expiry::{ExpiryObservables, RevalidationObs};
use hh_hir::leaves::Text;
use hh_hir::records::AssumptionDebtRecord;
use hh_ontology::debt::{
    DebtClass, DebtPolicy, DebtScope, DebtStatus, EvidenceKind, EvidenceRef, ExpiryCondition,
    ExpiryKind, ModelSelector, OwnerRef, RemovalTest, RemovalTestKind,
};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use hh_lab::debt::{
    debt_index, evaluate_debt, missing_required_fields, route_notices, AssumptionDebtHealth,
    DebtIndexEntry, DebtObservables, ProfileChange, ProfileChangeKind,
};
use hh_lab::model::{
    guard_at_bind, regression_drifted_rules, run_regression_suite, snapshot_span,
    synthetic_snapshot_claim, CompatibilityStatus, GuardAtBindError, HarnessRegressionSuite,
    RegressionCheck, RegressionVerdict,
};

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
        owner: OwnerRef::principal("test:owner"),
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

fn policy() -> DebtPolicy {
    DebtPolicy::default()
}

fn causes(t: &hh_lab::debt::DebtTransition) -> Vec<String> {
    t.causes.clone()
}

// ── evaluate_debt: the manager-level trigger families ───────────────────────

#[test]
fn profile_change_rebound_warns_on_a_profile_home() {
    let r = debt_record("r1", ExpiryKind::ModelVersionChange);
    let obs = DebtObservables {
        profile_change: Some(ProfileChange {
            kind: ProfileChangeKind::Rebound,
            grace_elapsed: false,
            profile_ref: "profile:p-2".into(),
        }),
        ..Default::default()
    };
    let ts = evaluate_debt("debt:r1", &r, Some(3), &obs, 1_000, &policy());
    assert_eq!(ts.len(), 1);
    let t = &ts[0];
    assert_eq!(t.to, DebtStatus::Expiring);
    assert!(causes(t).contains(&"profile_change:rebound".to_string()));
    assert_eq!(t.evidence_ref.as_deref(), Some("profile:p-2"));
}

#[test]
fn retired_profile_past_grace_expires() {
    let r = debt_record("r1", ExpiryKind::ModelVersionChange);
    let obs = DebtObservables {
        profile_change: Some(ProfileChange {
            kind: ProfileChangeKind::Retired,
            grace_elapsed: true,
            profile_ref: "profile:p-1".into(),
        }),
        ..Default::default()
    };
    let ts = evaluate_debt("debt:r1", &r, Some(4), &obs, 1_000, &policy());
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].to, DebtStatus::Expired);
    assert!(causes(&ts[0]).contains(&"profile_change:retired".to_string()));
}

#[test]
fn retired_profile_inside_grace_warns_not_expires() {
    let r = debt_record("r1", ExpiryKind::ModelVersionChange);
    let obs = DebtObservables {
        profile_change: Some(ProfileChange {
            kind: ProfileChangeKind::Retired,
            grace_elapsed: false,
            profile_ref: "profile:p-1".into(),
        }),
        ..Default::default()
    };
    let ts = evaluate_debt("debt:r1", &r, Some(5), &obs, 1_000, &policy());
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].to, DebtStatus::Expiring);
}

#[test]
fn profile_change_does_not_fire_off_profile_homes() {
    // A non-profile home (e.g. the harness_rule home id 1): the
    // profile-change family doesn't apply.
    let r = debt_record("r1", ExpiryKind::ModelVersionChange);
    let obs = DebtObservables {
        profile_change: Some(ProfileChange {
            kind: ProfileChangeKind::SupersededL2,
            grace_elapsed: false,
            profile_ref: "profile:p-2".into(),
        }),
        ..Default::default()
    };
    let ts = evaluate_debt("debt:r1", &r, Some(1), &obs, 1_000, &policy());
    assert!(ts.is_empty(), "non-profile home must not fire: {ts:?}");
}

#[test]
fn evidence_supersession_warns() {
    let mut r = debt_record("r1", ExpiryKind::ModelVersionChange);
    r.evidence_refs = vec![EvidenceRef::legacy("sha256:superseded")];
    let obs = DebtObservables {
        evidence_superseded: vec!["sha256:superseded".into()],
        ..Default::default()
    };
    let ts = evaluate_debt("debt:r1", &r, Some(6), &obs, 1_000, &policy());
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].to, DebtStatus::Expiring);
    assert!(causes(&ts[0]).contains(&"evidence_superseded".to_string()));
    assert_eq!(ts[0].evidence_ref.as_deref(), Some("sha256:superseded"));
}

#[test]
fn probation_overrun_warns_on_hypothesized_records() {
    // `evidence_refs` of `source`-kind refs → `hypothesized` grade; a
    // record older than `hypothesized_max_age_ms` warns.
    let mut r = debt_record("r1", ExpiryKind::ModelVersionChange);
    r.created_at = Some(0);
    assert_eq!(
        r.evidence_grade(),
        hh_ontology::debt::EvidenceGrade::Hypothesized
    );
    let mut p = policy();
    p.hypothesized_max_age_ms = 100;
    let obs = DebtObservables::default();
    let ts = evaluate_debt("debt:r1", &r, Some(6), &obs, 1_000, &p);
    assert_eq!(ts.len(), 1);
    assert!(causes(&ts[0]).contains(&"probation_overrun".to_string()));
    // Within the bound → quiet.
    let ts = evaluate_debt("debt:r1", &r, Some(6), &obs, 50, &p);
    assert!(ts.is_empty());
}

#[test]
fn drifted_regression_rules_drive_expiring() {
    // ADR-0203's `drifted ⇒ expiring`: a `drifted{rules}` verdict naming
    // this rule projects onto `regression_drifted_rules`.
    let r = debt_record("r1", ExpiryKind::ModelVersionChange);
    let suite = suite();
    let status = run_regression_suite(
        &suite,
        &[("r1".into(), RegressionVerdict::Drift)],
        "ev:regression-1",
    );
    let drifted = regression_drifted_rules(&status);
    assert_eq!(drifted, vec!["r1".to_string()]);
    let obs = DebtObservables {
        expiry: ExpiryObservables {
            regression_drifted_rules: drifted,
            ..Default::default()
        },
        ..Default::default()
    };
    let ts = evaluate_debt("debt:r1", &r, Some(3), &obs, 1_000, &policy());
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].to, DebtStatus::Expiring);
    assert!(causes(&ts[0])
        .iter()
        .any(|c| c.contains("regression") || c.contains("drifted")));
}

#[test]
fn hard_trigger_subsumes_warn_and_folds_causes() {
    // `unresolved_beyond_grace` (hard) + `fingerprint_drift` (warn) → a
    // single `expired` transition naming both causes.
    let r = debt_record("r1", ExpiryKind::ModelVersionChange);
    let obs = DebtObservables {
        expiry: ExpiryObservables {
            unresolved_beyond_grace: true,
            fingerprint_drift: true,
            ..Default::default()
        },
        profile_change: Some(ProfileChange {
            kind: ProfileChangeKind::SupersededL2,
            grace_elapsed: false,
            profile_ref: "profile:p-2".into(),
        }),
        ..Default::default()
    };
    let ts = evaluate_debt("debt:r1", &r, Some(3), &obs, 1_000, &policy());
    assert_eq!(ts.len(), 1, "hard subsumes warn — one row: {ts:?}");
    assert_eq!(ts[0].to, DebtStatus::Expired);
    let cs = causes(&ts[0]);
    assert!(cs.contains(&"no_listed_model_beyond_grace".to_string()));
    assert!(cs.contains(&"fingerprint_drift".to_string()));
    assert!(cs.contains(&"profile_change:superseded_l2".to_string()));
}

#[test]
fn retired_is_terminal_and_expired_does_not_refire() {
    let mut r = debt_record("r1", ExpiryKind::ModelVersionChange);
    r.status = DebtStatus::Retired;
    let obs = DebtObservables {
        expiry: ExpiryObservables {
            fingerprint_drift: true,
            unresolved_beyond_grace: true,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(evaluate_debt("debt:r1", &r, Some(3), &obs, 1_000, &policy()).is_empty());

    // `expired` stays `expired` — triggers re-fire as observability only.
    let mut r2 = debt_record("r1", ExpiryKind::ModelVersionChange);
    r2.status = DebtStatus::Expired;
    assert!(evaluate_debt("debt:r1", &r2, Some(3), &obs, 1_000, &policy()).is_empty());
}

#[test]
fn revalidated_model_conditioned_requires_evidenced_grade() {
    // §5h.6 §2: `revalidated` on a `model_conditioned` record requires
    // the fresh evidence to grade ≥ `evidenced`.
    let mut r = debt_record("r1", ExpiryKind::ModelVersionChange);
    r.status = DebtStatus::Expiring;
    r.debt_class = Some(DebtClass::ModelConditioned);
    let obs = DebtObservables {
        expiry: ExpiryObservables {
            revalidation: Some(RevalidationObs {
                evidence_ref: "ev:fresh".into(),
                probe_passed: false,
            }),
            ..Default::default()
        },
        revalidation_grade: Some(hh_ontology::debt::EvidenceGrade::Hypothesized),
        ..Default::default()
    };
    // `hypothesized` fresh evidence — the leg is suppressed.
    assert!(evaluate_debt("debt:r1", &r, Some(3), &obs, 1_000, &policy()).is_empty());
    // `evidenced` — `expiring → active`.
    let obs_ok = DebtObservables {
        revalidation_grade: Some(hh_ontology::debt::EvidenceGrade::Evidenced),
        ..obs.clone()
    };
    let ts = evaluate_debt("debt:r1", &r, Some(3), &obs_ok, 1_000, &policy());
    assert_eq!(ts.len(), 1);
    assert_eq!(ts[0].to, DebtStatus::Active);
    assert_eq!(ts[0].trigger, "revalidated");
    assert_eq!(ts[0].evidence_ref.as_deref(), Some("ev:fresh"));
}

#[test]
fn missing_required_fields_names_the_gaps() {
    let mut r = debt_record("r1", ExpiryKind::ModelVersionChange);
    r.debt_class = None;
    r.removal_test = None;
    // Home 1 (`harness_rule.rule_id` — needs model scope): missing
    // `debt_class` + `removal_test` are named.
    let missing = missing_required_fields(&r, Some(1), &policy());
    assert!(missing.contains(&"debt_class".to_string()));
    assert!(missing.contains(&"removal_test".to_string()));
    // Unknown home → no home-specific requirements.
    assert!(missing_required_fields(&r, None, &policy()).is_empty());
}

// ── debt_index: the stored ⊕ fold view ──────────────────────────────────────

#[test]
fn index_folds_transitions_and_names_the_last_trigger() {
    let r = debt_record("r1", ExpiryKind::ModelVersionChange);
    let entries = vec![
        DebtIndexEntry {
            home: Some(3),
            version_id: Some("v-1".into()),
            record: r.clone(),
            transitions: vec![
                hh_lab::debt::DebtTransition {
                    debt_ref: "debt:r1".into(),
                    from: DebtStatus::Active,
                    to: DebtStatus::Expiring,
                    trigger: "model_version_change".into(),
                    evidence_ref: None,
                    causes: vec!["fingerprint_drift".into()],
                },
                hh_lab::debt::DebtTransition {
                    debt_ref: "debt:r1".into(),
                    from: DebtStatus::Expiring,
                    to: DebtStatus::Expired,
                    trigger: "model_version_change".into(),
                    evidence_ref: None,
                    causes: vec!["unresolved_beyond_grace".into()],
                },
            ],
            removal_test_state: None,
            used_by: vec!["sv:1".into()],
            expired_used_runs: 2,
        },
        // A second row with no transitions stays at the stored status.
        DebtIndexEntry {
            home: Some(1),
            version_id: None,
            record: debt_record("r0", ExpiryKind::Date),
            transitions: vec![],
            removal_test_state: None,
            used_by: vec![],
            expired_used_runs: 0,
        },
    ];
    let rows = debt_index(&entries);
    assert_eq!(rows.len(), 2);
    // Sorted by (home, rule_id): r0 (home 1) first, r1 (home 3) second.
    assert_eq!(rows[0].rule_id, "r0");
    assert_eq!(rows[0].status, DebtStatus::Active);
    let row = &rows[1];
    assert_eq!(row.rule_id, "r1");
    assert_eq!(row.status, DebtStatus::Expired);
    assert_eq!(row.stored_status, Some(DebtStatus::Active));
    assert_eq!(row.last_trigger.as_deref(), Some("model_version_change"));
    assert!(row
        .staleness_reasons
        .contains(&"model_version_change".to_string()));
    assert_eq!(row.used_by, vec!["sv:1".to_string()]);
    assert_eq!(row.expired_used_runs, 2);
    // Rebuild equality — the same entries fold to the same rows.
    assert_eq!(debt_index(&entries), rows);
    // The row round-trips through the canonical JSON (the two-surface
    // contract — `hh-embed` decodes the same shape).
    let j = row.to_json();
    let back = hh_lab::debt::DebtIndexRow::from_json(&j).expect("roundtrip");
    assert_eq!(&back, row);
}

// ── report + notices routing ────────────────────────────────────────────────

#[test]
fn report_buckets_and_notice_routing() {
    let mut r_expiring = debt_record("r1", ExpiryKind::Date);
    r_expiring.status = DebtStatus::Expiring;
    let now = 1_000_000u64;
    let rows = vec![
        // An expiring row inside the warn window (`expiry_at` within
        // `warn_within_ms` of `now`).
        hh_lab::debt::DebtIndexRow {
            rule_id: "r1".into(),
            owner: OwnerRef {
                team: false,
                id: "o1".into(),
                reach_via: vec!["sink:slack".into()],
            },
            status: DebtStatus::Expiring,
            removal_test_kind: Some(RemovalTestKind::RetirementExperiment),
            expiry_condition: ExpiryCondition {
                kind: ExpiryKind::Date,
                value: None,
            },
            created_at: 1,
            expiry_at: Some(now + 100),
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
        },
        // An expired row still conditioning a rule — an unreachable
        // owner (no reach_via sinks) → `unrouted`, never dropped.
        hh_lab::debt::DebtIndexRow {
            rule_id: "r2".into(),
            owner: OwnerRef::principal("o2"),
            status: DebtStatus::Expired,
            removal_test_kind: None,
            expiry_condition: ExpiryCondition {
                kind: ExpiryKind::Date,
                value: None,
            },
            created_at: 1,
            expiry_at: Some(now - 100),
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
        },
    ];
    let health = AssumptionDebtHealth {
        warn_within_ms: DebtPolicy::default().warn_within_ms,
    };
    let report = health.report(&rows, now);
    assert_eq!(report.expired_used, vec!["r2".to_string()]);
    assert_eq!(report.expiring, vec!["r1".to_string()]);

    let notices = health.notices(&rows, &report);
    // r1: expiry_approaching + retirement_test_due; r2: expired_used.
    assert_eq!(notices.len(), 3);

    let mut p = policy();
    p.notice_sinks = vec!["sink:slack".into()];
    let routed = route_notices(&notices, &p);
    let slack = routed.get("sink:slack").expect("routed to slack");
    assert_eq!(slack.len(), 2, "r1's two notices route to slack");
    let unrouted = routed.get("unrouted").expect("r2 unrouted");
    assert_eq!(unrouted.len(), 1);
    assert_eq!(
        unrouted[0].get("kind").and_then(Json::as_str),
        Some("expired_used")
    );
}

// ── the compat slice (§5h.8; R-2.9.8¹) ─────────────────────────────────────

fn suite() -> HarnessRegressionSuite {
    HarnessRegressionSuite {
        suite_id: "suite:regression-1".into(),
        checks: vec![
            RegressionCheck {
                rule_id: "r1".into(),
                capability: None,
            },
            RegressionCheck {
                rule_id: "r2".into(),
                capability: Some("tool_call_shape".into()),
            },
        ],
        provenance: ProvenanceRecord::kernel("s54.test", 0),
    }
}

#[test]
fn regression_suite_verdicts() {
    let s = suite();
    // All pass → verified.
    let v = run_regression_suite(
        &s,
        &[
            ("r1".into(), RegressionVerdict::Pass),
            ("r2".into(), RegressionVerdict::Pass),
        ],
        "ev:1",
    );
    assert_eq!(v, CompatibilityStatus::Verified);
    // Any drift → drifted{rules} (sorted/deduped).
    let v = run_regression_suite(
        &s,
        &[
            ("r2".into(), RegressionVerdict::Drift),
            ("r1".into(), RegressionVerdict::Drift),
            ("r1".into(), RegressionVerdict::Pass),
        ],
        "ev:2",
    );
    match &v {
        CompatibilityStatus::Drifted { rules } => {
            assert_eq!(rules, &vec!["r1".to_string(), "r2".to_string()])
        }
        other => panic!("expected drifted: {other:?}"),
    }
    // Drift outranks a simultaneous hard failure.
    let v = run_regression_suite(
        &s,
        &[
            ("r1".into(), RegressionVerdict::Drift),
            ("r2".into(), RegressionVerdict::Fail),
        ],
        "ev:3",
    );
    assert!(matches!(v, CompatibilityStatus::Drifted { .. }));
    // Hard with no drift → broken{report_ref = evidence_ref}.
    let v = run_regression_suite(&s, &[("r1".into(), RegressionVerdict::Fail)], "ev:4");
    match &v {
        CompatibilityStatus::Broken { report_ref } => assert_eq!(report_ref, "ev:4"),
        other => panic!("expected broken: {other:?}"),
    }
    // Unsupported alone is a hard failure too.
    let v = run_regression_suite(&s, &[("r1".into(), RegressionVerdict::Unsupported)], "ev:5");
    assert!(matches!(v, CompatibilityStatus::Broken { .. }));
    // No rows → unknown (never a silent verified).
    let v = run_regression_suite(&s, &[], "ev:6");
    assert_eq!(v, CompatibilityStatus::Unknown);
}

#[test]
fn synthetic_snapshot_claim_shape() {
    let claim = synthetic_snapshot_claim("acme", "acme-model", "drift-9");
    assert_eq!(claim.provider, "acme");
    assert_eq!(claim.model_id, "acme-model");
    assert_eq!(claim.snapshot_id, "drift:drift-9");
    assert!(claim.weights_digest.is_none());
    assert!(claim.serving_route.is_none());
    assert_eq!(
        claim.policy_version_exposed,
        hh_lab::model::PolicyVersionExposed::Unknown
    );
    // The canonical JSON round-trips.
    let back = hh_lab::model::SnapshotClaim::from_json(&claim.to_json())
        .expect("snapshot claim roundtrip");
    assert_eq!(back, claim);
}

#[test]
fn guard_at_bind_refuses_unpinned_snapshot() {
    assert_eq!(
        guard_at_bind(None).unwrap_err(),
        GuardAtBindError::UnpinnedSnapshot
    );
    assert_eq!(
        GuardAtBindError::UnpinnedSnapshot.name(),
        "UnpinnedSnapshot"
    );
    let claim = synthetic_snapshot_claim("acme", "m", "d1");
    assert!(guard_at_bind(Some(&claim)).is_ok());
}

#[test]
fn snapshot_span_marks_the_drift_row() {
    let span = snapshot_span("snap:a", "snap:c", true);
    assert_eq!(span.get("first").and_then(Json::as_str), Some("snap:a"));
    assert_eq!(span.get("last").and_then(Json::as_str), Some("snap:c"));
    assert_eq!(span.get("drift"), Some(&Json::Bool(true)));
}

#[test]
fn regression_drifted_rules_projects_only_drifted() {
    assert!(regression_drifted_rules(&CompatibilityStatus::Verified).is_empty());
    assert!(regression_drifted_rules(&CompatibilityStatus::Unknown).is_empty());
    assert!(regression_drifted_rules(&CompatibilityStatus::Broken {
        report_ref: "r".into()
    })
    .is_empty());
}

#[test]
fn verdict_and_change_spellings_round_trip() {
    for v in [
        RegressionVerdict::Pass,
        RegressionVerdict::Drift,
        RegressionVerdict::Fail,
        RegressionVerdict::Unsupported,
    ] {
        assert_eq!(RegressionVerdict::parse(v.name()), Some(v));
    }
    for k in [
        ProfileChangeKind::SupersededL2,
        ProfileChangeKind::Retired,
        ProfileChangeKind::PastRetirement,
        ProfileChangeKind::Rebound,
    ] {
        assert_eq!(ProfileChangeKind::parse(k.name()), Some(k));
    }
    let _ = EvidenceKind::Source; // closed family, used via fixtures above
}
