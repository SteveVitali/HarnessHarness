//! `hh-lab` S6.4 coverage — R-2.9.8 (§5h.8; ADR-0325;
//! AC-R-2.9.8-{1–4,6–9,11–13}): the co-evolution interface's pure
//! folds — `project_training_export` (determinism, held-out refusal,
//! reward admissibility, typed loss), `import_snapshot` (the unpinned
//! claim lands `trained_under`/`unknown` records + the sweep inputs),
//! `guard_at_bind` (G-1..G-3), `post_import_sweep` (the reverse
//! partition), the consolidation view + verdict + `RetirementRecord`
//! bridge, the `CoEvolutionCycleRecord` sidecar folds
//! (`matched_total` arithmetic, `max_cycles`, `broken_policy`), the
//! `NullTrainer` conformance fixture, and the `lab/org-policy-v1`
//! recipe's `MatchSpec` (AC-R-2.12.6-12's matched-cap leg).
//!
//! HarnessHarness never trains: every test here is records-in /
//! records-out — no weights, checkpoints, datasets, optimizer state,
//! or token-level material crosses the interface (N9).

use std::collections::BTreeMap;

use hh_hir::leaves::Text;
use hh_hir::records::AssumptionDebtRecord;
use hh_lab::coevolution::{
    self as coe, ArmOrigin, BindGuard, BrokenPolicy, CoEvolutionError, ConsolidationInput,
    ConsolidationPolicy, ConsolidationVerdict, CyclePhase, CyclePolicy, CycleStopReason,
    ImportOutcome, LessonFact, NeverConsolidate, NullTrainer, PhasePlan, RouteStatus, SampleUnit,
    SealedDefinition, SwitchRule, TrainingExportCtx, TrainingExportPolicy,
};
use hh_lab::model::{
    CompatibilityRecord, CompatibilityStatus, ForeignRef, PolicyVersionExposed, SnapshotClaim,
    TrainedUnderRef,
};
use hh_ontology::debt::{
    DebtClass, DebtExpiry, DebtScope, DebtStatus, EvidenceKind, EvidenceRef, ExpiryCondition,
    ExpiryKind, ExpiryParams, HypothesisSubject, HypothesisTyped, ModelSelector, OwnerRef,
    PredictedEffect, RemovalTest, RemovalTestKind,
};
use hh_provenance::{HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::Json;

// ── fixtures ────────────────────────────────────────────────────────────────

fn human(author: &str) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human(author, HumanRole::Principal),
        PersistenceScope::Run,
        1,
    )
}

fn policy() -> TrainingExportPolicy {
    TrainingExportPolicy {
        sample_unit: SampleUnit::Turn,
        include: vec!["model_call".into(), "reward".into()],
        readers: vec!["lab:trainer".into()],
        redaction: None,
        reward_sources: vec!["oracle_metric".into(), "oracle_verdict".into()],
        split_labels: vec!["train".into(), "dev".into()],
        snapshot_filter: None,
    }
}

fn ctx() -> TrainingExportCtx {
    let mut task_splits = BTreeMap::new();
    task_splits.insert("task:a".to_string(), "train".to_string());
    task_splits.insert("task:b".to_string(), "train".to_string());
    task_splits.insert("task:held".to_string(), "held_out".to_string());
    TrainingExportCtx {
        task_splits,
        corpus_readers: vec!["lab:trainer".into()],
        harness_constraints: Json::obj([("profile", Json::str("minimal:0"))]),
        provenance: None,
    }
}

/// One `model.call.completed` envelope (the `turn` sample row) +
/// one `measurement.oracle.metric.emitted` reward row over `task:a`.
fn ledger() -> Vec<Json> {
    vec![
        Json::obj([
            ("event_id", Json::str("ev:call-1")),
            ("class", Json::str("model.call.completed")),
            ("seq", Json::Int(1)),
            ("scope", Json::obj([("task_id", Json::str("task:a"))])),
            (
                "payload",
                Json::obj([
                    ("request_plan_hash", Json::str("sha256:req-1")),
                    ("snapshot_id", Json::str("snap:base-1")),
                ]),
            ),
        ]),
        Json::obj([
            ("event_id", Json::str("ev:reward-1")),
            ("class", Json::str("measurement.oracle.metric.emitted")),
            ("seq", Json::Int(2)),
            ("scope", Json::obj([("task_id", Json::str("task:a"))])),
            (
                "payload",
                Json::obj([(
                    "reward",
                    Json::obj([
                        ("source", Json::str("oracle_metric")),
                        ("oracle_ref", Json::str("oracle:task_success")),
                        ("oracle_class", Json::str("oracle")),
                        ("value", Json::Int(1)),
                        ("unit", Json::str("task_success")),
                    ]),
                )]),
            ),
        ]),
        Json::obj([
            ("event_id", Json::str("ev:call-2")),
            ("class", Json::str("model.call.completed")),
            ("seq", Json::Int(3)),
            ("scope", Json::obj([("task_id", Json::str("task:b"))])),
            (
                "payload",
                Json::obj([
                    ("request_plan_hash", Json::str("sha256:req-2")),
                    ("snapshot_id", Json::str("snap:base-1")),
                ]),
            ),
        ]),
    ]
}

fn base_claim() -> SnapshotClaim {
    SnapshotClaim {
        provider: "prov:acme".into(),
        model_id: "model:m-7".into(),
        snapshot_id: "snap:base-1".into(),
        serving_route: None,
        base_snapshot_ref: None,
        training_lineage: None,
        trained_under: vec![],
        training_cutoff_claim: None,
        weights_digest: Some(ForeignRef {
            system: "hf".into(),
            digest: "sha256:aa00".into(),
            label: None,
            provenance: human("prov:weights"),
        }),
        policy_version_exposed: PolicyVersionExposed::Supported,
    }
}

fn trained_under() -> Vec<TrainedUnderRef> {
    vec![TrainedUnderRef {
        definition_semantic_id: "def:harness/a".into(),
        profile_semantic_id: "profile:minimal:0".into(),
        bundle_id: "bundle:train-1".into(),
    }]
}

fn definitions() -> Vec<SealedDefinition> {
    vec![
        SealedDefinition {
            definition_semantic_id: "def:harness/a".into(),
            profile_semantic_id: "profile:minimal:0".into(),
        },
        SealedDefinition {
            definition_semantic_id: "def:harness/b".into(),
            profile_semantic_id: "profile:minimal:0".into(),
        },
    ]
}

/// A conditioned debt record — `scope.model_selectors` + the
/// `model_version_change` expiry (the AC-R-2.9.8-12 shape).
fn conditioned_debt(rule_id: &str, selector: ModelSelector) -> AssumptionDebtRecord {
    let prov = human("test:owner");
    AssumptionDebtRecord {
        rule_id: rule_id.into(),
        hypothesis: Text::new(
            format!("{rule_id} conditions a snapshot-scoped claim"),
            "test:owner",
            prov.clone(),
        ),
        evidence_refs: vec![EvidenceRef {
            kind: EvidenceKind::Source,
            reference: format!("ev:{rule_id}"),
            observed_at: Some(1),
            tier: None,
            provisional: false,
        }],
        owner: OwnerRef {
            team: false,
            id: "test:owner".into(),
            reach_via: vec!["sink:ops".into()],
        },
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::ModelVersionChange,
            value: None,
        },
        removal_test_ref: "tmpl:retirement".into(),
        status: DebtStatus::Active,
        debt_class: Some(DebtClass::ModelConditioned),
        hypothesis_typed: Some(HypothesisTyped {
            subject: HypothesisSubject::Deficiency,
            deficiency_class: hh_ontology::debt::DeficiencyClass::PrematureStop,
            predicted_effect: PredictedEffect::Increase,
            metric_ref: Some("metric:task_success".into()),
        }),
        scope: Some(DebtScope {
            model_selectors: vec![selector],
            task_classes: vec![],
            roles: vec![],
        }),
        expiry: Some(DebtExpiry {
            condition: ExpiryKind::ModelVersionChange,
            params: ExpiryParams::default(),
        }),
        runway_ms: None,
        revalidation: None,
        removal_test: Some(RemovalTest {
            template_ref: Some("tmpl:retirement".into()),
            ..RemovalTest::new(RemovalTestKind::RetirementExperiment)
        }),
        created_by: Some(prov),
        created_at: Some(1),
        supersedes: None,
    }
}

fn cycle_policy() -> CyclePolicy {
    CyclePolicy {
        weight_phase_budget: 100,
        harness_search_budget: Json::obj([("search", Json::Int(50))]),
        switch_rule: SwitchRule::CycleCount,
        max_cycles: 2,
        retention_set_ref: Some("suite:retention".into()),
        regression_policy: None,
        broken_policy: BrokenPolicy::ReSearch,
    }
}

// ── AC-R-2.9.8-1 — the export is a deterministic view ───────────────────────

/// Same ledger + policy + ctx ⇒ byte-identical export (E-1); the
/// export id is content-derived; `maturity = research-grade`; the
/// `veto_tripped` member is explicit.
#[test]
fn export_is_deterministic_and_sealed() {
    let (e1, l1) = coe::project_training_export("run:1", &ledger(), &policy(), &ctx()).unwrap();
    let (e2, l2) = coe::project_training_export("run:1", &ledger(), &policy(), &ctx()).unwrap();
    assert_eq!(e1, e2, "the same inputs yield the byte-identical export");
    assert_eq!(l1, l2);
    assert!(!e1.export_id.is_empty());
    assert_eq!(e1.samples.len(), 2);
    assert_eq!(e1.samples[0].task_ref, "task:a");
    assert_eq!(e1.samples[0].input_digest, "sha256:req-1");
    // The reward bound to the preceding sample (seq 1 < seq 2).
    let r = e1.samples[0]
        .reward
        .as_ref()
        .expect("reward binds sample 0");
    assert_eq!(r.source, "oracle_metric");
    assert_eq!(r.oracle_ref, "oracle:task_success");
    assert!(!r.veto_tripped);
    assert!(e1.samples[1].reward.is_none());
    // The exposure record rides data_refs + reward provenance, never bodies.
    assert!(e1.exposure.data_refs.contains(&"ev:call-1".to_string()));
    assert!(e1
        .exposure
        .reward_provenance
        .contains(&"oracle_metric:oracle:task_success".to_string()));
    // The record stamps the dialect + maturity contract
    // (AC-R-2.9.8-13's research-grade leg).
    let j = e1.to_json();
    assert_eq!(
        j.get("training_export").and_then(Json::as_str),
        Some("training_export/1")
    );
    assert_eq!(
        j.get("maturity").and_then(Json::as_str),
        Some("research-grade")
    );
}

/// `no_slot` loss rows are typed — a model-call row with no input
/// digest lands `n/a` on the sample plus a typed `no_slot` entry,
/// never a silently dropped or fabricated digest (E-5/E-6).
#[test]
fn export_types_unavailable_members_as_loss() {
    let mut events = ledger();
    events[0] = Json::obj([
        ("event_id", Json::str("ev:call-nodigest")),
        ("class", Json::str("model.call.completed")),
        ("seq", Json::Int(1)),
        ("scope", Json::obj([("task_id", Json::str("task:a"))])),
        (
            "payload",
            Json::obj([("snapshot_id", Json::str("snap:base-1"))]),
        ),
    ]);
    let (e, loss) = coe::project_training_export("run:1", &events, &policy(), &ctx()).unwrap();
    assert_eq!(e.samples[0].input_digest, "n/a");
    assert!(loss
        .iter()
        .any(|l| l.class == "no_slot" && l.member.as_deref() == Some("sample:ev:call-nodigest")));
}

// ── AC-R-2.9.8-2 — no held-out/private leakage ──────────────────────────────

/// A `held_out`-labelled task refuses `HeldOutInExport` before a byte
/// is projected — on the sample row and on the reward row alike (E-2).
#[test]
fn held_out_tasks_refuse_the_export() {
    let mut events = ledger();
    events[0] = Json::obj([
        ("event_id", Json::str("ev:call-held")),
        ("class", Json::str("model.call.completed")),
        ("seq", Json::Int(1)),
        ("scope", Json::obj([("task_id", Json::str("task:held"))])),
        (
            "payload",
            Json::obj([("request_plan_hash", Json::str("sha256:h"))]),
        ),
    ]);
    let err = coe::project_training_export("run:1", &events, &policy(), &ctx()).unwrap_err();
    assert!(matches!(err, CoEvolutionError::HeldOutInExport { .. }));
    assert_eq!(err.code(), "HeldOutInExport");

    // The held-out check covers reward rows too.
    let mut events = ledger();
    events[1] = Json::obj([
        ("event_id", Json::str("ev:reward-held")),
        ("class", Json::str("measurement.oracle.metric.emitted")),
        ("seq", Json::Int(2)),
        ("scope", Json::obj([("task_id", Json::str("task:held"))])),
        (
            "payload",
            Json::obj([(
                "reward",
                Json::obj([("source", Json::str("oracle_metric"))]),
            )]),
        ),
    ]);
    let err = coe::project_training_export("run:1", &events, &policy(), &ctx()).unwrap_err();
    assert!(matches!(err, CoEvolutionError::HeldOutInExport { .. }));
}

// ── AC-R-2.9.8-3 — reward admissibility ─────────────────────────────────────

/// A critic-verdict reward row refuses `InadmissibleRewardSource`
/// outright; an *admissible* row missing `oracle_ref` types `n/a`
/// (never fabricated); the `veto_tripped` flag is explicit.
#[test]
fn reward_admissibility_is_closed() {
    let mut events = ledger();
    events[1] = Json::obj([
        ("event_id", Json::str("ev:reward-critic")),
        ("class", Json::str("measurement.oracle.verdict")),
        ("seq", Json::Int(2)),
        ("scope", Json::obj([("task_id", Json::str("task:a"))])),
        (
            "payload",
            Json::obj([(
                "reward",
                Json::obj([("source", Json::str("critic_verdict"))]),
            )]),
        ),
    ]);
    let err = coe::project_training_export("run:1", &events, &policy(), &ctx()).unwrap_err();
    assert!(matches!(
        err,
        CoEvolutionError::InadmissibleRewardSource { .. }
    ));
    assert_eq!(err.code(), "InadmissibleRewardSource");

    // A source spelling outside the policy's declared set refuses too.
    let mut events = ledger();
    events[1] = Json::obj([
        ("event_id", Json::str("ev:reward-undeclared")),
        ("class", Json::str("measurement.oracle.metric.emitted")),
        ("seq", Json::Int(2)),
        ("scope", Json::obj([("task_id", Json::str("task:a"))])),
        (
            "payload",
            Json::obj([("reward", Json::obj([("source", Json::str("metric_value"))]))]),
        ),
    ]);
    let err = coe::project_training_export("run:1", &events, &policy(), &ctx()).unwrap_err();
    assert!(matches!(
        err,
        CoEvolutionError::InadmissibleRewardSource { .. }
    ));
}

/// `veto_tripped` is explicit on the sample and the reward (E-5); a
/// reader outside the corpus's declared set refuses `ReaderViolation`
/// (E-4).
#[test]
fn veto_and_reader_violation_are_explicit() {
    let mut events = ledger();
    events[0] = Json::obj([
        ("event_id", Json::str("ev:call-veto")),
        ("class", Json::str("model.call.completed")),
        ("seq", Json::Int(1)),
        ("scope", Json::obj([("task_id", Json::str("task:a"))])),
        (
            "payload",
            Json::obj([
                ("request_plan_hash", Json::str("sha256:v")),
                ("veto_tripped", Json::Bool(true)),
            ]),
        ),
    ]);
    let (e, _l) = coe::project_training_export("run:1", &events, &policy(), &ctx()).unwrap();
    assert!(e.samples[0].veto_tripped);

    let mut p = policy();
    p.readers = vec!["lab:not-a-reader".into()];
    let err = coe::project_training_export("run:1", &ledger(), &p, &ctx()).unwrap_err();
    assert!(matches!(err, CoEvolutionError::ReaderViolation { .. }));
}

/// The `snapshot_filter` confines samples to the named snapshot; a
/// policy spelling an unexpressible include refuses
/// `UnexpressibleMember` (never silently dropped).
#[test]
fn snapshot_filter_and_include_policy_are_closed() {
    let mut p = policy();
    p.snapshot_filter = Some("snap:other".into());
    let (e, _l) = coe::project_training_export("run:1", &ledger(), &p, &ctx()).unwrap();
    assert!(e.samples.is_empty(), "no sample matches the filter");

    let mut p = policy();
    p.include.push("token_logprobs".into());
    let err = coe::project_training_export("run:1", &ledger(), &p, &ctx()).unwrap_err();
    assert!(matches!(err, CoEvolutionError::UnexpressibleMember { .. }));
}

// ── AC-R-2.9.8-4 — import never pins ────────────────────────────────────────

/// `import_snapshot` lands the claim honestly: `pinned = false`,
/// `status = reported`, `weights_digest` the foreign claim verbatim;
/// `trained_under` for every named `(definition, profile)` pair,
/// `unknown` for every other sealed definition (I-2/I-3); the
/// lifecycle row is `lifecycle.registry.imported`-shaped.
#[test]
fn import_lands_unpinned_and_types_compatibility() {
    let mut claim = base_claim();
    claim.trained_under = trained_under();
    let prov = human("test:import");
    let out: ImportOutcome = coe::import_snapshot(
        &claim,
        &definitions(),
        &[],
        RouteStatus::Unchecked,
        &[],
        &[],
        &prov,
        7,
    )
    .unwrap();
    // I-2 — `pinned = false`, `status = reported` (the claim never pins).
    let snap = &out.snapshot_record;
    assert_eq!(snap.get("pinned"), Some(&Json::Bool(false)));
    assert_eq!(snap.get("status").and_then(Json::as_str), Some("reported"));
    assert_eq!(
        snap.get("imported").and_then(Json::as_str),
        None,
        "imported is a Bool"
    );
    assert_eq!(snap.get("imported"), Some(&Json::Bool(true)));
    // `weights_digest` rides as the foreign claim — verbatim, never a
    // content address.
    assert_eq!(
        snap.get("weights_digest"),
        claim.weights_digest.as_ref().map(|w| w.to_json()).as_ref()
    );
    // I-3 — trained_under for the named pair; unknown for the other.
    assert_eq!(out.compatibility.len(), 2);
    let by_def: BTreeMap<_, _> = out
        .compatibility
        .iter()
        .map(|c| (c.definition_semantic_id.as_str(), c))
        .collect();
    assert!(matches!(
        by_def["def:harness/a"].status,
        CompatibilityStatus::TrainedUnder
    ));
    assert!(matches!(
        by_def["def:harness/b"].status,
        CompatibilityStatus::Unknown
    ));
    // The lifecycle row names the scoped-rule sweep inputs.
    assert_eq!(
        out.lifecycle.get("class").and_then(Json::as_str),
        Some("lifecycle.registry.imported")
    );
}

/// A claim naming an absent base refuses `BaseSnapshotUnknown`; a
/// reported-unreachable route refuses `ServingRouteUnreachable`; a
/// lineage cycle refuses `LineageCycle`; a `policy_version_exposed ≠
/// supported` claim lands `unknown` everywhere (I-1 — the pin is the
/// guard's precondition, never the import's).
#[test]
fn import_refusals_are_typed() {
    let prov = human("test:import");
    let mut claim = base_claim();
    claim.base_snapshot_ref = Some("snap:ghost".into());
    let err = coe::import_snapshot(
        &claim,
        &definitions(),
        &[],
        RouteStatus::Unchecked,
        &[],
        &[],
        &prov,
        7,
    )
    .unwrap_err();
    assert!(matches!(err, CoEvolutionError::BaseSnapshotUnknown { .. }));

    let mut claim = base_claim();
    claim.serving_route = Some("route:dead".into());
    let err = coe::import_snapshot(
        &claim,
        &definitions(),
        &[],
        RouteStatus::Unreachable,
        &[],
        &[],
        &prov,
        7,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        CoEvolutionError::ServingRouteUnreachable { .. }
    ));

    let claim = base_claim();
    let err = coe::import_snapshot(
        &claim,
        &definitions(),
        &[],
        RouteStatus::Unchecked,
        &[],
        &["snap:base-1".to_string()],
        &prov,
        7,
    )
    .unwrap_err();
    assert!(matches!(err, CoEvolutionError::LineageCycle { .. }));

    // `policy_version_exposed = unknown` — the claim still lands; every
    // compatibility is `unknown` (the bind guard owns the refusal).
    let mut claim = base_claim();
    claim.policy_version_exposed = PolicyVersionExposed::Unknown;
    claim.trained_under = trained_under();
    let out = coe::import_snapshot(
        &claim,
        &definitions(),
        &[],
        RouteStatus::Unchecked,
        &[],
        &[],
        &prov,
        7,
    )
    .unwrap();
    assert!(out
        .compatibility
        .iter()
        .all(|c| matches!(c.status, CompatibilityStatus::Unknown)));
}

// ── AC-R-2.9.8-6 — the bind-time guard ──────────────────────────────────────

/// `guard_at_bind` — `verified`/`trained_under` proceed; `drifted`
/// annotates its rules; `broken` refuses `CompatibilityBroken`; an
/// evolution-origin arm with no record refuses `ExploratoryOnly` (the
/// `exploratory` declaration or `allow_unverified_snapshot` lifts it).
#[test]
fn guard_at_bind_is_evidence_backed() {
    let prov = human("test:guard");
    let verified = CompatibilityRecord {
        snapshot_ref: "snap:1".into(),
        definition_semantic_id: "def:a".into(),
        profile_semantic_id: "profile:0".into(),
        status: CompatibilityStatus::Verified,
        evidence_ref: Some("report:conf".into()),
        regression_suite_ref: None,
        created_at: 1,
        expiry_condition: None,
        provenance: prov.clone(),
    };
    assert_eq!(
        coe::guard_at_bind(Some(&verified), ArmOrigin::Evolution, false, false),
        BindGuard::Proceed
    );

    let mut drifted = verified.clone();
    drifted.status = CompatibilityStatus::Drifted {
        rules: vec!["rule:r9".into()],
    };
    match coe::guard_at_bind(Some(&drifted), ArmOrigin::Registered, false, false) {
        BindGuard::Annotated {
            compatibility,
            expiring_rules,
        } => {
            assert_eq!(compatibility, "drifted");
            assert_eq!(expiring_rules, vec!["rule:r9".to_string()]);
        }
        other => panic!("drifted annotates: {other:?}"),
    }

    let mut broken = verified.clone();
    broken.status = CompatibilityStatus::Broken {
        report_ref: "report:broke".into(),
    };
    match coe::guard_at_bind(Some(&broken), ArmOrigin::Registered, false, false) {
        BindGuard::Refused { code } => {
            assert!(matches!(code, CoEvolutionError::CompatibilityBroken { .. }));
        }
        other => panic!("broken refuses: {other:?}"),
    }

    // G-3 — the evolution-origin arm without a record refuses unless
    // exploratory or the operator lifts the check.
    match coe::guard_at_bind(None, ArmOrigin::Evolution, false, false) {
        BindGuard::Refused { code } => {
            assert!(matches!(code, CoEvolutionError::ExploratoryOnly { .. }));
        }
        other => panic!("evolution-origin arm refuses without evidence: {other:?}"),
    }
    assert!(matches!(
        coe::guard_at_bind(None, ArmOrigin::Evolution, true, false),
        BindGuard::Annotated { .. }
    ));
    assert!(matches!(
        coe::guard_at_bind(None, ArmOrigin::Evolution, false, true),
        BindGuard::Annotated { .. }
    ));
    // A registered arm without a record proceeds annotated-unknown.
    match coe::guard_at_bind(None, ArmOrigin::Registered, false, false) {
        BindGuard::Annotated { compatibility, .. } => assert_eq!(compatibility, "unknown"),
        other => panic!("registered arm annotates unknown: {other:?}"),
    }
}

// ── AC-R-2.9.8-7 — the post-import sweep ────────────────────────────────────

/// `post_import_sweep` partitions scoped debts: the snapshot the
/// selector covers is `covered` (the `model_version_change` trigger
/// fires); scoped debts it does *not* cover are `scheduled` (the
/// reverse sweep); unscoped debts are `unchanged`.
#[test]
fn post_import_sweep_partitions_scoped_debts() {
    let claim = base_claim();
    let covered = conditioned_debt(
        "rule:covered",
        ModelSelector::Exact {
            model_id: "model:m-7".into(),
        },
    );
    let scheduled = conditioned_debt(
        "rule:scheduled",
        ModelSelector::Exact {
            model_id: "model:other".into(),
        },
    );
    let mut unscoped = conditioned_debt(
        "rule:unscoped",
        ModelSelector::Exact {
            model_id: "model:m-7".into(),
        },
    );
    unscoped.scope = None;
    let sweep = coe::post_import_sweep(&[covered, scheduled, unscoped], &claim);
    assert_eq!(sweep.covered, vec!["rule:covered".to_string()]);
    assert_eq!(sweep.scheduled, vec!["rule:scheduled".to_string()]);
    assert_eq!(sweep.unchanged, vec!["rule:unscoped".to_string()]);
}

/// The `model_version_change` expiry is mandatory on a conditioned
/// record — `validate_removal_test` refuses `missing_snapshot_scope`
/// when the scope or the expiry leg is absent (AC-R-2.9.8-12).
#[test]
fn conditioned_debt_needs_scope_and_version_expiry() {
    use hh_hir::debt::{validate_removal_test, RemovalTestContext};
    use hh_ontology::debt::debt_home;

    let home = debt_home("harness_rule", "assumption_debt").unwrap();
    let ctx = RemovalTestContext::member_level();

    // Missing model_selectors — refuses.
    let mut d = conditioned_debt(
        "rule:noscope",
        ModelSelector::Exact {
            model_id: "model:m-7".into(),
        },
    );
    d.scope = None;
    let err = validate_removal_test(&d, home, &ctx).unwrap_err();
    assert!(format!("{err:?}").contains("MissingSnapshotScope"));

    // Scoped but no model_version_change expiry — refuses (AC-12's
    // second leg).
    let mut d = conditioned_debt(
        "rule:noexpiry",
        ModelSelector::Exact {
            model_id: "model:m-7".into(),
        },
    );
    d.expiry_condition.kind = ExpiryKind::Date;
    d.expiry = Some(DebtExpiry {
        condition: ExpiryKind::Date,
        params: ExpiryParams::default(),
    });
    let err = validate_removal_test(&d, home, &ctx).unwrap_err();
    assert!(
        format!("{err:?}").contains("MissingSnapshotScope"),
        "date-expiry conditioned debt refuses: {err:?}"
    );

    // The full AC-12 shape validates (the resolver-free legs pass).
    let d = conditioned_debt(
        "rule:ok",
        ModelSelector::Exact {
            model_id: "model:m-7".into(),
        },
    );
    validate_removal_test(&d, home, &ctx).unwrap();
}

// ── AC-R-2.9.8-9 — consolidation proof shape ────────────────────────────────

/// The `consolidation_candidates` view: an `active` candidate ≥
/// `k_cycles` with consolidable lessons and bounded economics lands;
/// N1–N5 refuse spelled-out rows (judge calibration, task-only,
/// safety-gated, terminal, debt-incomplete).
#[test]
fn consolidation_candidates_apply_the_closed_legs() {
    let lessons = |rule: &str, kind: &str| LessonFact {
        lesson_id: format!("lesson:{rule}"),
        rule_id: rule.into(),
        kind: kind.into(),
        effect: Some("non_harmed".into()),
        provenance: None,
    };
    let input =
        |rule: &str, state: &str, cycles: u64, lessons: Vec<LessonFact>| ConsolidationInput {
            rule_id: rule.into(),
            state: state.into(),
            cycles_active: cycles,
            lessons,
            economics_ratio: Some(3.0),
            terminal: false,
        };
    let inputs = vec![
        input(
            "rule:ok",
            "active",
            2,
            vec![lessons("rule:ok", "formatting_improvement")],
        ),
        // N1 — judge-calibration lesson removes the row.
        input(
            "rule:n1",
            "active",
            2,
            vec![
                lessons("rule:n1", "formatting_improvement"),
                lessons("rule:n1", "judge_calibration"),
            ],
        ),
        // N2 — no consolidable lesson = task-only.
        input(
            "rule:n2",
            "active",
            2,
            vec![lessons("rule:n2", "task_level_ablation_x")],
        ),
        // N3 — safety-gated lesson vetoes.
        input(
            "rule:n3",
            "active",
            2,
            vec![
                lessons("rule:n3", "stop_rule_refinement"),
                lessons("rule:n3", "veto"),
            ],
        ),
        // N4 — terminal.
        ConsolidationInput {
            terminal: true,
            ..input(
                "rule:n4",
                "active",
                2,
                vec![lessons("rule:n4", "name_adjustment")],
            )
        },
        // k_cycles below the policy.
        input(
            "rule:young",
            "active",
            0,
            vec![lessons("rule:young", "formatting_improvement")],
        ),
    ];
    let policy = ConsolidationPolicy {
        k_cycles: 1,
        economics_factor: 1.0,
    };
    let debt_complete = |r: &str| r != "rule:n5";
    let (cands, refused) = coe::consolidation_candidates(&inputs, &debt_complete, &policy);
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].rule_id, "rule:ok");
    assert_eq!(cands[0].lessons, vec!["lesson:rule:ok".to_string()]);
    let legs: BTreeMap<_, _> = refused.iter().cloned().collect();
    assert_eq!(legs["rule:n1"], NeverConsolidate::JudgeCalibration);
    assert_eq!(legs["rule:n2"], NeverConsolidate::TaskOnlyEffect);
    assert_eq!(legs["rule:n3"], NeverConsolidate::SafetyGated);
    assert_eq!(legs["rule:n4"], NeverConsolidate::Terminal);
    // `rule:young` neither lands nor refuses — it fails the k_cycles
    // gate honestly (no row).
    assert!(!legs.contains_key("rule:young"));
}

/// `consolidation_verdict` — V1 (no target report ⇒
/// `MissingComparisonReport`), V3 (superseded precedes every leg),
/// V2 (harmed/veto/delivered-elsewhere ⇒ `rejected`), else `absorbed`.
/// `consolidation_retirement_record` names the report (the
/// `RetirementRecord.removal_test_report_ref` = `report_id` — the
/// debt manager's `retire` is the only gate).
#[test]
fn consolidation_verdict_and_retirement_proof() {
    use coe::ConsolidationEvidence as Ev;
    use hh_hir::debt::RetirementRecord;

    // V1 — the target report is mandatory.
    let err =
        coe::consolidation_verdict(None, Ev::NonHarmed, false, false, false, false).unwrap_err();
    assert!(matches!(
        err,
        CoEvolutionError::MissingComparisonReport { .. }
    ));

    // V3 — superseded precedes every leg (no retirement re-runs).
    assert_eq!(
        coe::consolidation_verdict(None, Ev::Harmed, true, true, true, true).unwrap(),
        ConsolidationVerdict::Superseded
    );

    // V2 — harmed target ⇒ rejected; harmed retention ⇒ rejected (the
    // planted-retention-regression fixture never reports absorbed —
    // the conditioned rule stays live, §5h.8 §5.1).
    assert_eq!(
        coe::consolidation_verdict(Some(Ev::Harmed), Ev::NonHarmed, false, false, false, false)
            .unwrap(),
        ConsolidationVerdict::Rejected
    );
    assert_eq!(
        coe::consolidation_verdict(Some(Ev::NonHarmed), Ev::Harmed, false, false, false, false)
            .unwrap(),
        ConsolidationVerdict::Rejected
    );
    assert_eq!(
        coe::consolidation_verdict(
            Some(Ev::NonHarmed),
            Ev::NonHarmed,
            true,
            false,
            false,
            false
        )
        .unwrap(),
        ConsolidationVerdict::Rejected
    );

    // Absorbed — all reports pass.
    assert_eq!(
        coe::consolidation_verdict(
            Some(Ev::NonHarmed),
            Ev::NonHarmed,
            false,
            false,
            false,
            false
        )
        .unwrap(),
        ConsolidationVerdict::Absorbed
    );

    // The report is preview/non-headline and the retirement record
    // names it (§5h.8 §5.1 — the `RetirementRecord` bridge).
    let mut report = coe::ConsolidationReport {
        report_id: String::new(),
        proposal_ref: Some("prop:1".into()),
        target_rule: "rule:ok".into(),
        snapshot_ref: "snap:trained-1".into(),
        lessons: vec!["lesson:rule:ok".into()],
        verdict: ConsolidationVerdict::Absorbed,
        experiment_ref: "exp:retire-1".into(),
        evidence_refs: vec!["report:target".into()],
        debt_refs: vec!["debt:rule:ok".into()],
    };
    report.seal();
    assert!(!report.report_id.is_empty());
    let j = report.to_json();
    assert_eq!(j.get("label").and_then(Json::as_str), Some("preview"));
    assert_eq!(j.get("headline"), Some(&Json::Bool(false)));
    assert_eq!(
        j.get("maturity").and_then(Json::as_str),
        Some("research-grade")
    );

    let rec: RetirementRecord =
        coe::consolidation_retirement_record(&report, "verdict:1", &human("test:sealer"));
    assert_eq!(rec.removal_test_report_ref, report.report_id);
    assert_eq!(rec.verdict_ref, "verdict:1");
    let rationale = rec.rationale.content.as_deref().unwrap_or_default();
    assert!(rationale.contains("absorbed into weights"));
    assert!(rationale.contains("snap:trained-1"));
}

// ── AC-R-2.9.8-8 — the cycle record ─────────────────────────────────────────

/// `cycle_open → begin_phase → complete_phase → next → stop` — the
/// four-phase lap orders deterministically; `matched_total` is the
/// only budget arithmetic (the declared `training` budget adds, the
/// harness budgets charge); `cycles_completed` counts full laps.
#[test]
fn cycle_record_orders_phases_and_totals_budgets() {
    let mut r = coe::cycle_open(cycle_policy(), "lineage:l1");
    assert!(!r.cycle_id.is_empty());
    assert_eq!(r.cycles_completed(), 0);
    assert!(matches!(
        coe::cycle_next(&r),
        PhasePlan::HarnessSearch { .. }
    ));

    // Lap 1 — harness_search.
    coe::cycle_begin_phase(
        &mut r,
        CyclePhase::HarnessSearch,
        Json::obj([("base_ref", Json::str("snap:base-1"))]),
    )
    .unwrap();
    coe::cycle_complete_phase(
        &mut r,
        Json::obj([("candidate_refs", Json::Arr(vec![Json::str("cand:1")]))]),
        Some("exp:search-1"),
        None,
        Some("completed"),
        Json::obj([("search", Json::Int(30))]),
    )
    .unwrap();
    assert!(matches!(coe::cycle_next(&r), PhasePlan::WeightUpdate));

    // weight_update — the external leg (a claim, never a spend).
    coe::cycle_begin_phase(&mut r, CyclePhase::WeightUpdate, Json::obj([])).unwrap();
    coe::cycle_complete_phase(
        &mut r,
        Json::obj([("snapshot_out", Json::str("snap:trained-1"))]),
        None,
        Some("train:run-1"),
        Some("completed"),
        Json::obj([("training", Json::Int(100))]),
    )
    .unwrap();
    assert_eq!(
        r.training_run_ref.as_deref(),
        Some("train:run-1"),
        "the training run ref rides the record"
    );
    assert!(matches!(coe::cycle_next(&r), PhasePlan::ReEvaluation));

    // re_evaluation — clean.
    coe::cycle_begin_phase(
        &mut r,
        CyclePhase::ReEvaluation,
        Json::obj([("base_ref", Json::str("snap:base-1"))]),
    )
    .unwrap();
    coe::cycle_complete_phase(
        &mut r,
        Json::obj([("report_refs", Json::Arr(vec![Json::str("report:re")]))]),
        Some("exp:re-1"),
        None,
        Some("verified"),
        Json::obj([("eval", Json::Int(20))]),
    )
    .unwrap();
    assert!(matches!(coe::cycle_next(&r), PhasePlan::Consolidation));

    // consolidation — completes lap 1.
    coe::cycle_begin_phase(&mut r, CyclePhase::Consolidation, Json::obj([])).unwrap();
    coe::cycle_complete_phase(
        &mut r,
        Json::obj([("base_ref", Json::str("snap:trained-1"))]),
        Some("exp:ret-1"),
        None,
        Some("absorbed"),
        Json::obj([("eval", Json::Int(5))]),
    )
    .unwrap();
    assert_eq!(r.cycles_completed(), 1);
    // matched_total — declared budgets add, never split.
    let b = &r.budgets;
    assert_eq!(b.get("search").and_then(Json::as_int), Some(30));
    assert_eq!(b.get("eval").and_then(Json::as_int), Some(25));
    assert_eq!(b.get("training").and_then(Json::as_int), Some(100));

    // A second full lap lands `max_cycles` → `Stop{MaxCycles}`.
    for (phase, verdict) in [
        (CyclePhase::HarnessSearch, "completed"),
        (CyclePhase::WeightUpdate, "completed"),
        (CyclePhase::ReEvaluation, "verified"),
        (CyclePhase::Consolidation, "absorbed"),
    ] {
        coe::cycle_begin_phase(&mut r, phase, Json::obj([])).unwrap();
        coe::cycle_complete_phase(
            &mut r,
            Json::obj([]),
            None,
            None,
            Some(verdict),
            Json::obj([]),
        )
        .unwrap();
    }
    assert_eq!(r.cycles_completed(), 2);
    match coe::cycle_next(&r) {
        PhasePlan::Stop { reason } => {
            assert_eq!(reason, CycleStopReason::MaxCycles)
        }
        other => panic!("max_cycles stops: {other:?}"),
    }
    coe::cycle_stop(&mut r, CycleStopReason::MaxCycles, Some("completed"));
    // A stopped record refuses further phases.
    let err = coe::cycle_begin_phase(&mut r, CyclePhase::HarnessSearch, Json::obj([])).unwrap_err();
    assert!(matches!(err, CoEvolutionError::IllegalPhase { .. }));

    // The record round-trips member-strict.
    let back = coe::CoEvolutionCycleRecord::from_json(&r.to_json()).unwrap();
    assert_eq!(back.cycle_id, r.cycle_id);
    assert_eq!(back.cycles_completed(), 2);
    assert_eq!(back.stop_reason.as_ref(), Some(&CycleStopReason::MaxCycles));
}

/// A `broken` re-evaluation verdict resolves `broken_policy` —
/// `re_search` re-enters harness_search on the drifted base; a
/// `rollback` policy rolls the head back.
#[test]
fn broken_compatibility_resolves_the_declared_policy() {
    let mut r = coe::cycle_open(cycle_policy(), "lineage:l2");
    coe::cycle_begin_phase(
        &mut r,
        CyclePhase::HarnessSearch,
        Json::obj([("base_ref", Json::str("snap:b0"))]),
    )
    .unwrap();
    coe::cycle_complete_phase(
        &mut r,
        Json::obj([]),
        None,
        None,
        Some("completed"),
        Json::obj([]),
    )
    .unwrap();
    coe::cycle_begin_phase(&mut r, CyclePhase::WeightUpdate, Json::obj([])).unwrap();
    coe::cycle_complete_phase(
        &mut r,
        Json::obj([]),
        None,
        Some("train:r"),
        Some("completed"),
        Json::obj([]),
    )
    .unwrap();
    coe::cycle_begin_phase(
        &mut r,
        CyclePhase::ReEvaluation,
        Json::obj([("base_ref", Json::str("snap:drifted"))]),
    )
    .unwrap();
    coe::cycle_complete_phase(
        &mut r,
        Json::obj([]),
        Some("exp:re"),
        None,
        Some("broken"),
        Json::obj([]),
    )
    .unwrap();
    match coe::cycle_next(&r) {
        PhasePlan::ReSearch { base_ref } => assert_eq!(base_ref, "snap:drifted"),
        other => panic!("broken re_search resolves: {other:?}"),
    }

    // The `rollback` leg rolls the head back and the record can stop.
    let mut p = cycle_policy();
    p.broken_policy = BrokenPolicy::Rollback;
    let mut r2 = coe::cycle_open(p, "lineage:l3");
    coe::cycle_begin_phase(
        &mut r2,
        CyclePhase::ReEvaluation,
        Json::obj([("base_ref", Json::str("snap:last-good"))]),
    )
    .unwrap();
    coe::cycle_complete_phase(
        &mut r2,
        Json::obj([]),
        None,
        None,
        Some("broken"),
        Json::obj([]),
    )
    .unwrap();
    match coe::cycle_next(&r2) {
        PhasePlan::Rollback { base_ref } => assert_eq!(base_ref, "snap:last-good"),
        other => panic!("broken rollback resolves: {other:?}"),
    }
    coe::cycle_stop(
        &mut r2,
        CycleStopReason::CompatibilityBroken,
        Some("rolled_back"),
    );
    assert_eq!(
        r2.stop_reason.as_ref(),
        Some(&CycleStopReason::CompatibilityBroken)
    );
}

// ── AC-R-2.9.8-11 — the null trainer ────────────────────────────────────────

/// The `NullTrainer` returns *identical weights* under a new snapshot
/// id — `weights_digest` = the base's claim (a foreign claim, never a
/// served address), `method = null_trainer`, `policy_version_exposed =
/// supported`. The trained claim imports: `trained_under` for the
/// named pair, `unknown` elsewhere — and the full cycle is executable
/// without a real trainer (N9).
#[test]
fn null_trainer_round_trips_through_import() {
    let mut trainer = NullTrainer::new("fixture-1");
    let base = base_claim();
    let claim = trainer.train("export:1", &base, trained_under());
    assert_eq!(trainer.trains, 1);
    assert_eq!(claim.snapshot_id, "null/fixture-1/0001");
    // Identical weights — the digest is the base's foreign claim.
    assert_eq!(claim.weights_digest, base.weights_digest);
    assert_eq!(claim.base_snapshot_ref.as_deref(), Some("snap:base-1"));
    let lin = claim.training_lineage.as_ref().unwrap();
    assert_eq!(lin.method, "null_trainer");
    assert_eq!(lin.data_refs, vec!["export:1".to_string()]);

    // The claim imports under its own declared base.
    let prov = human("test:import");
    let out = coe::import_snapshot(
        &claim,
        &definitions(),
        &[],
        RouteStatus::Unchecked,
        &["snap:base-1".to_string()],
        &[],
        &prov,
        9,
    )
    .unwrap();
    let by_def: BTreeMap<_, _> = out
        .compatibility
        .iter()
        .map(|c| (c.definition_semantic_id.as_str(), c))
        .collect();
    assert!(matches!(
        by_def["def:harness/a"].status,
        CompatibilityStatus::TrainedUnder
    ));
    // The imported snapshot record is unpinned and reported.
    assert_eq!(out.snapshot_record.get("pinned"), Some(&Json::Bool(false)));

    // Attached-mode bump: the served id carries the version counter —
    // the drift leg's input.
    assert_eq!(trainer.serve_snapshot_id("model:m-7"), "model:m-7-pv1");
    trainer.bump_policy_version();
    assert_eq!(trainer.serve_snapshot_id("model:m-7"), "model:m-7-pv2");
}

// ── AC-R-2.9.8-2/5 — the regression suite + compatibility tags ──────────────

/// `export_regression_suite` builds an ordinary `ExperimentSpec` —
/// `kind: comparative`, `design: paired`, the varied factor
/// `model_snapshot` with levels `{snapshot_in, snapshot_out}` whose
/// `snapshot_out` level carries the declared `unbound` sentinel ref
/// (the import binds it — never a guess); the retention set +
/// per-rule compliance metrics + margins ride the record;
/// `matched_cap` + `cross_model` scope (the pair crosses models by
/// construction).
#[test]
fn regression_suite_binds_snapshot_out_unbound() {
    use hh_lab::experiment::{
        Backoff, CancelPolicy, ExperimentBudgets, ExperimentKind, OrderKind, ReattemptPolicy,
        SchedulingPolicy,
    };
    use hh_ontology::eval::{DesignKind, SeedPolicy};

    let pins = coe::RegressionSuitePins {
        suite_ref: "suite:conformance".into(),
        split_labels_used: vec![hh_ontology::lab::SplitLabel::Dev],
        split_assignment_ref: "split:a".into(),
        eval_budget_ref: "budget:e".into(),
        search_budget_ref: "budget:s".into(),
        artifact_semantic_id: "def:harness/a".into(),
        artifact_version_id: "sha256:art".into(),
        environment_level_id: "env:0".into(),
        environment_level_ref: "sha256:env".into(),
        registry_snapshot_id: "reg:1".into(),
        analysis_plan_ref: "plan:1".into(),
        task_split_hash: "sha256:ts".into(),
        pricing_table_ref: Some(hh_budget::pricing::PricingTableRef {
            table_id: "pricing:t".into(),
            version: "v1".into(),
            pin: None,
        }),
        registered_at: 1,
        replicates_per_cell: 5,
        seed_policy: SeedPolicy {
            harness_rng: true,
            requested_sampling_seed: true,
            seed_honoured_required: true,
        },
        scheduling: SchedulingPolicy {
            max_concurrent_runs: 4,
            pools: vec![],
            order: OrderKind::InterleavedBlocked,
            permutation_seed: "perm:1".into(),
            start_stagger_ms: 0,
            deadline: None,
            priority: None,
        },
        reattempt: ReattemptPolicy {
            max_per_plan: 2,
            max_fraction_of_plans_ppm: 100_000,
            backoff: Backoff {
                min_ms: 100,
                multiplier_ppm: 2_000_000,
                max_ms: 10_000,
            },
            error_classes_included: None,
            on_cancel: CancelPolicy::Replan,
        },
        budgets: ExperimentBudgets {
            experiment: "budget:x".into(),
            instrument: "budget:i".into(),
        },
    };
    let compliance: BTreeMap<String, String> =
        [("rule:r1".to_string(), "metric:rule_r1".to_string())]
            .into_iter()
            .collect();
    let margins = Json::obj([("metric:rule_r1", Json::str("margin:ni"))]);
    let out = coe::export_regression_suite(
        "def:harness/a",
        "snap:base-1",
        &pins,
        "suite:retention",
        compliance,
        margins,
    )
    .unwrap();
    assert_eq!(out.suite_id, "lab/regression/def:harness/a@snap:base-1");
    assert_eq!(out.unbound_level_id, coe::SNAPSHOT_OUT_LEVEL);
    assert_eq!(out.spec.kind, ExperimentKind::Comparative);
    assert_eq!(out.spec.design.kind, DesignKind::Paired);
    // The varied factor is `model_snapshot`; the `snapshot_out` level
    // carries the unbound sentinel ref.
    let factor = out
        .spec
        .factors
        .iter()
        .find(|f| f.name == "model_snapshot")
        .expect("model_snapshot factor");
    let levels: Vec<_> = factor
        .levels
        .iter()
        .map(|l| (l.level_id.as_str(), l.ref_.as_str()))
        .collect();
    assert_eq!(
        levels,
        vec![
            ("snapshot_in", "snap:base-1"),
            ("snapshot_out", coe::UNBOUND_REF)
        ]
    );
    // matched_cap + cross_model — the pair crosses models by
    // construction.
    let m = out.spec.arms[0].match_spec.as_ref().unwrap();
    assert_eq!(m.mode, hh_budget::MatchMode::MatchedCap);
    assert_eq!(m.model_scope, hh_budget::matchspec::ModelScope::CrossModel);
    // The record stamps the preview/maturity contract.
    let j = out.to_json();
    assert_eq!(j.get("label").and_then(Json::as_str), Some("preview"));
    assert_eq!(
        j.get("maturity").and_then(Json::as_str),
        Some("research-grade")
    );
    assert_eq!(
        j.get("retention_set_ref").and_then(Json::as_str),
        Some("suite:retention")
    );
}

/// `export_compatibility_tags` — the read-only projection: status
/// buckets per `definition@profile`; `rules_tagged` carries
/// `drifted`/`conditioned`/`expiring` — never mutating the debts.
#[test]
fn compatibility_tags_project_status_and_rule_tags() {
    let prov = human("test:tags");
    let rec = |status| CompatibilityRecord {
        snapshot_ref: "snap:t".into(),
        definition_semantic_id: "def:a".into(),
        profile_semantic_id: "profile:0".into(),
        status,
        evidence_ref: Some("report:1".into()),
        regression_suite_ref: None,
        created_at: 1,
        expiry_condition: None,
        provenance: prov.clone(),
    };
    let mut drifted = conditioned_debt(
        "rule:drift",
        ModelSelector::Exact {
            model_id: "model:m-7".into(),
        },
    );
    drifted.status = DebtStatus::Active;
    let records = vec![
        rec(CompatibilityStatus::Verified),
        rec(CompatibilityStatus::Drifted {
            rules: vec!["rule:drift".into()],
        }),
        rec(CompatibilityStatus::Unknown),
    ];
    let debts = vec![drifted];
    let set = coe::export_compatibility_tags("snap:t", &records, &debts);
    assert_eq!(set.verified.len(), 1);
    assert_eq!(set.drifted.len(), 1);
    assert_eq!(set.unknown.len(), 1);
    assert_eq!(set.rules_tagged["rule:drift"], "drifted");
    // The debts are untouched — the tag set is a view.
    assert_eq!(debts[0].status, DebtStatus::Active);
    let j = set.to_json();
    assert_eq!(j.get("label").and_then(Json::as_str), Some("preview"));
}

// ── AC-R-2.12.6-12 — the org-policy recipe's MatchSpec ──────────────────────

/// `lab/org-policy-v1` exists in the exemplar set with a `matched_cap`
/// `MatchSpec` over the declared pricing table (the recipe's matched
/// dimension inventory includes `message_human` — the §5i.1 6d leg).
#[test]
fn org_policy_recipe_carries_matched_cap() {
    use hh_budget::matchspec::MatchMode;
    use hh_budget::pricing::PricingTableRef;
    use hh_lab::exemplars;
    use hh_ontology::dimensions::DimensionId;

    let pricing = PricingTableRef {
        table_id: "pricing:test".into(),
        version: "v1".into(),
        pin: Some("sha256:pp".into()),
    };
    let m = exemplars::org_policy_match(&pricing);
    assert_eq!(m.mode, MatchMode::MatchedCap);
    // The matched-dimension inventory includes `message_human` (the
    // §5i.1 6d addition) and `spend` under the pinned pricing table.
    assert!(m.dimensions.contains(&DimensionId::MessageHuman));
    assert!(m.dimensions.contains(&DimensionId::Spend));
    assert!(m.pricing_table_ref.is_some());

    // The recipe itself registers `lab/org-policy-v1` as its design id
    // — the removal-test template the fleet defaults name.
    let pins = exemplars::ExemplarPins {
        suite_ref: "suite:heldout".into(),
        held_out_split_ref: "split:held_out".into(),
        split_assignment_ref: "split:assign".into(),
        registry_snapshot_id: "reg:snap".into(),
        eval_budget: "budget:eval".into(),
        search_budget: "budget:search".into(),
        experiment_budget: "budget:exp".into(),
        instrument_budget: "budget:inst".into(),
        analysis_plan_ref: "plan:org".into(),
        task_split_hash: "sha256:ts".into(),
    };
    let own = exemplars::OrgPolicyPins {
        factors: vec![(
            "unattended_policy".into(),
            vec![
                exemplars::OrgPolicyLevel {
                    level_id: "deny".into(),
                    content_ref: "sha256:deny".into(),
                    label: "deny".into(),
                },
                exemplars::OrgPolicyLevel {
                    level_id: "defer".into(),
                    content_ref: "sha256:defer".into(),
                    label: "defer".into(),
                },
            ],
        )],
        arms: vec![exemplars::OrgPolicyArmPins {
            arm_id: "arm:default".into(),
            hypothesis: "the default holds".into(),
            levels: vec![("unattended_policy".into(), "deny".into())],
            artifact: hh_ontology::config::Ref::new("fleet:default", "sha256:ff"),
            simulated_responder_calibration_ref: None,
        }],
        pricing_table_ref: pricing.clone(),
    };
    let spec = exemplars::org_policy_v1(&pins, &own, 1);
    assert_eq!(spec.design.id, "lab/org-policy-v1");
    // Every arm carries the recipe's MatchSpec (T-LCD-14's leg).
    assert!(spec
        .arms
        .iter()
        .all(|a| a.match_spec.as_ref().map(|m| m.mode) == Some(MatchMode::MatchedCap)));
}

/// The interface's own `AssumptionDebtRecord` (AC-R-2.9.8-13's reflexive
/// leg): `model_conditioned`, `model_version_change` expiry, a covering
/// `range` selector the sweep resolves, and the null-trainer round-trip
/// conformance suite as the executable removal test — the spec-debt
/// register's ADR-0202/0203/0204 rows landed as a record.
#[test]
fn interface_debt_record_is_model_conditioned_and_sweep_covered() {
    let prov = human("test:owner");
    let rec = coe::interface_debt_record(
        "family:test-gen-1",
        &OwnerRef {
            team: false,
            id: "test:owner".into(),
            reach_via: vec!["sink:ops".into()],
        },
        &prov,
        1,
    );
    assert_eq!(rec.rule_id, coe::INTERFACE_DEBT_RULE_ID);
    assert_eq!(rec.debt_class, Some(DebtClass::ModelConditioned));
    assert_eq!(
        rec.expiry_condition.kind,
        ExpiryKind::ModelVersionChange,
        "a model-generation change expires the interface's claim (T5)"
    );
    let test = rec.removal_test.as_ref().expect("the removal test exists");
    assert_eq!(test.kind, RemovalTestKind::ConformanceRun);
    assert_eq!(
        test.conformance_suite_ref.as_deref(),
        Some(coe::NULL_TRAINER_SUITE_REF),
        "the null-trainer round-trip is the discharge path"
    );
    // `validate_removal_test` passes member-level: model-conditioned ⇒
    // scope.model_selectors + the `model_version_change` expiry (the
    // AC-R-2.9.8-12 leg the record satisfies).
    let home = hh_ontology::debt::debt_home("coevolution_interface", "assumption_debt")
        .unwrap_or_else(|| {
            hh_ontology::debt::debt_home("harness_rule", "assumption_debt")
                .expect("the harness_rule home exists")
        });
    hh_hir::debt::validate_removal_test(
        &rec,
        home,
        &hh_hir::debt::RemovalTestContext::member_level(),
    )
    .expect("the interface record validates member-level");
    // The sweep covers the record on import of an in-family snapshot
    // (`Range.family` matches `claim.model_id`/`provider` exactly; the
    // `*` version predicate admits any generation version).
    let mut claim = base_claim();
    claim.model_id = "family:test-gen-1".into();
    let sweep = coe::post_import_sweep(&[rec], &claim);
    assert_eq!(sweep.covered, vec![coe::INTERFACE_DEBT_RULE_ID.to_string()]);
}
