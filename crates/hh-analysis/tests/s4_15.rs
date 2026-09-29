//! S4.15 — hosted rows carry their capability strata into the `EvalRun`
//! projection (R-2.9.2²; ADR-0165 D6; spec §5h.2 §2.4):
//!
//! - the launch-stamped `manifest.extra["capability_vector"]` (the member
//!   `capability_vector_ref` binds) projects into
//!   `EvalRun.capability_vector` — strict `CapabilityVerdict` spellings,
//!   records-in;
//! - an unknown spelling, a non-string verdict or a non-object member is a
//!   typed `RowField` refusal — never a coerced `Unknown` (T-LCD-07);
//! - an absent vector stays empty: `requires_capabilities` metrics render
//!   `n/a{capability}` through the one `applicability` path (T-LCD-15), a
//!   `supported` verdict admits the cell and every other verdict — absent
//!   included — does not.

use std::collections::BTreeMap;

use hh_analysis::{eval_run as project_eval_run, AnalysisError};
use hh_eval::catalogue;
use hh_eval::facts::LedgerFacts;
use hh_eval::runs::{EvalRun, SuiteContext};
use hh_eval::scorecard::descriptor;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ontology::compliance::NaReason;
use hh_ontology::eval::MetricValueKind;
use hh_ontology::participant::{CapabilityVerdict, ParticipantClass};
use hh_results::audit::AuditRef;
use hh_results::row::{
    AuditSection, Cell, ConsumptionSection, Coordinates, DerivedFrom, OutcomeSection, ResultsRow,
    RowKey,
};
use hh_results::scoring::ScoringContext;
use hh_results::watermark::WatermarkSet;
use hh_wire::Json;

/// A finished hosted `ResultsRow` — `capability_vector_ref` binds the
/// participant's reconciled record; the launch-stamped verdicts ride the
/// manifest's `extra["capability_vector"]`.
fn hosted_row() -> ResultsRow {
    let run_id = "r-hosted-1".to_string();
    let mut watermark = WatermarkSet::new();
    watermark.pin(&run_id, 7);
    ResultsRow {
        key: RowKey {
            configuration_version_id: "cv-hosted".into(),
            run_id: run_id.clone(),
        },
        coordinates: Coordinates {
            configuration_id: Some("cfg-hosted".into()),
            configuration_version_id: "cv-hosted".into(),
            model_snapshots: [("default".to_string(), "snap-h1".to_string())]
                .into_iter()
                .collect(),
            harness_def_ref: None,
            model_profile_ref: None,
            environment_ref: None,
            environment_version_id: Some("env-1".into()),
            task: Some(Json::obj([
                ("task_id", Json::str("task-1")),
                ("suite_id", Json::str("suite-test")),
                ("split_label", Json::str("held_out")),
            ])),
            budget: Json::obj([]),
            replicate: Json::obj([("seed", Json::Int(42))]),
            participant_class: "hosted".into(),
            observability_level: vec!["events".into(), "model_io".into(), "end_state".into()],
            hosting_mechanism: Some("session_abi".into()),
            capability_vector_ref: Some("idp:cap-record-1".into()),
            mediation: vec!["model_calls".into()],
            registry_snapshot_id: None,
        },
        experiment: Some(Json::obj([
            ("experiment_run_id", Json::str("exp-run-1")),
            ("arm_id", Json::str("a")),
            ("cell_id", Json::str("a:task-1")),
            ("replicate_index", Json::Int(0)),
            ("attempt_no", Json::Int(1)),
            ("comparable", Json::Bool(true)),
        ])),
        outcome: OutcomeSection {
            status: "finished".into(),
            outcome_class: Some("scored".into()),
            stop_reason: Some("completed".into()),
            veto_tripped: vec![],
            finished_at: Some("2026-01-01T00:00:00.000Z".into()),
            wall_ms: Some(100),
            activation_no: 1,
        },
        cells: vec![Cell {
            metric_ref: "task_success".into(),
            value: MetricValueKind::Bool(true),
            detector: Some("deterministic".into()),
            oracle_ref: Some("oracle/executable".into()),
            confidence: None,
            evidence: vec![AuditRef {
                run_id: run_id.clone(),
                seq: 6,
                hash: "idp:cell".into(),
                checkpoint_ref: None,
                content_refs: vec![],
            }],
            computed_from: vec!["6".into()],
        }],
        consumption: ConsumptionSection {
            dimensions: [("model_calls".to_string(), 5i64)].into_iter().collect(),
            utilization: None,
            spend: None,
            instrument_spend: None,
        },
        cache: Json::obj([("policy", Json::str("cold_start"))]),
        lineage: Json::obj([]),
        annotations_from_ledger: Json::obj([]),
        audit: AuditSection {
            head: AuditRef {
                run_id: run_id.clone(),
                seq: 7,
                hash: "idp:head".into(),
                checkpoint_ref: None,
                content_refs: vec![],
            },
            checkpoint_ref: None,
        },
        scoring: ScoringContext {
            validator_set: vec![],
            overlay_runs: vec![],
            metric_registry_version: "test-registry".into(),
            pricing_ref: None,
            view_policy_version: "test-view".into(),
        },
        derived_from: DerivedFrom {
            run_id: run_id.clone(),
            seq: 7,
            overlay_watermarks: BTreeMap::new(),
        },
        watermark_set: watermark,
        view_hash: "test-view-hash".into(),
        version_id: "v-r-hosted-1".into(),
    }
}

/// A manifest whose `extra["capability_vector"]` is the stamped vector.
fn manifest_with_vector(vector: Json) -> RunManifest {
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.seed = Some(42);
    m.capability_declaration_ref = Some("idp:cap-record-1".into());
    m.extra.insert("capability_vector".into(), vector);
    m
}

fn suite() -> SuiteContext {
    SuiteContext {
        suite_id: "suite-test".into(),
        retired_for_headline: false,
        family: hh_ontology::lab::EnvironmentFamily::CodingTerminal,
    }
}

fn project(manifest: Option<&RunManifest>) -> Result<EvalRun, AnalysisError> {
    project_eval_run(
        &hosted_row(),
        manifest,
        LedgerFacts::default(),
        None,
        Some(&suite()),
    )
}

/// The `approval_rate` declaration — a catalogue row with
/// `requires_capabilities = {permission_surface}` (§5g.7).
fn approval_rate() -> hh_ontology::compliance::MetricDeclaration {
    catalogue::scorecard_metrics()
        .into_iter()
        .find(|d| d.name == "approval_rate")
        .expect("approval_rate must be declared")
}

#[test]
fn stamped_vector_projects_verbatim() {
    let manifest = manifest_with_vector(Json::obj([
        ("permission_surface", Json::str("supported")),
        ("filesystem", Json::str("unknown")),
    ]));
    let run = project(Some(&manifest)).unwrap();
    assert_eq!(run.participant_class, ParticipantClass::Hosted);
    assert_eq!(
        run.capability_vector.get("permission_surface"),
        Some(&CapabilityVerdict::Supported)
    );
    assert_eq!(
        run.capability_vector.get("filesystem"),
        Some(&CapabilityVerdict::Unknown)
    );
}

#[test]
fn supported_capability_admits_the_cell() {
    let manifest =
        manifest_with_vector(Json::obj([("permission_surface", Json::str("supported"))]));
    let run = project(Some(&manifest)).unwrap();
    assert_eq!(
        approval_rate().applicability(&descriptor(&run)),
        Ok(()),
        "a SUPPORTED verdict admits the capability-gated metric"
    );
}

#[test]
fn absent_or_non_supported_capability_is_typed_na() {
    // Absent member — the vector stays empty; `n/a{capability}`, never a
    // coerced verdict (T-LCD-15).
    let no_manifest = project(None).unwrap();
    assert!(no_manifest.capability_vector.is_empty());
    assert_eq!(
        approval_rate().applicability(&descriptor(&no_manifest)),
        Err(NaReason::Capability)
    );
    // Every non-`supported` verdict is inadmissible — `unknown` most of
    // all (T-LCD-07): unknown is never coerced in either direction.
    for spelling in [
        "unsupported",
        "partial",
        "not_applicable",
        "unknown",
        "skipped",
        "drift",
    ] {
        let manifest =
            manifest_with_vector(Json::obj([("permission_surface", Json::str(spelling))]));
        let run = project(Some(&manifest)).unwrap();
        assert_eq!(
            approval_rate().applicability(&descriptor(&run)),
            Err(NaReason::Capability),
            "{spelling} must render n/a{{capability}}"
        );
    }
}

#[test]
fn unknown_spelling_is_a_typed_refusal() {
    let manifest = manifest_with_vector(Json::obj([(
        "permission_surface",
        Json::str("mostly_supported"),
    )]));
    match project(Some(&manifest)) {
        Err(AnalysisError::RowField { field, .. }) => {
            assert_eq!(field, "capability_vector")
        }
        other => panic!("expected RowField(capability_vector), got {other:?}"),
    }
}

#[test]
fn malformed_member_is_a_typed_refusal() {
    // A non-string verdict refuses.
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.seed = Some(42);
    m.extra.insert(
        "capability_vector".into(),
        Json::Obj(BTreeMap::from([(
            "permission_surface".to_string(),
            Json::Int(1),
        )])),
    );
    match project(Some(&m)) {
        Err(AnalysisError::RowField { field, .. }) => {
            assert_eq!(field, "capability_vector")
        }
        other => panic!("expected RowField(capability_vector), got {other:?}"),
    }
    // A non-object member refuses.
    let mut m2 = RunManifest::minimal(RunKind::Agent);
    m2.seed = Some(42);
    m2.extra
        .insert("capability_vector".into(), Json::str("supported"));
    match project(Some(&m2)) {
        Err(AnalysisError::RowField { field, .. }) => {
            assert_eq!(field, "capability_vector")
        }
        other => panic!("expected RowField(capability_vector), got {other:?}"),
    }
}
