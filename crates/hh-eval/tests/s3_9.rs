//! S3.9 — the `attribution_completeness` veto wired into suite verdicts
//! (§5d.5 §8 "veto; C0/Stage 3 veto tier"; ADR-0044/0045): a completed model
//! call with no `measurement.cost.attributed` coverage trips the veto; the
//! trip ids land on the run's `veto_tripped` member and the scorecard counts
//! the run as *success-with-veto* — excluded from the headline cell, counted
//! beside it (ADR-0045 D6), never silently clean and never a bare zero.

use std::collections::{BTreeMap, BTreeSet};

use hh_eval::catalogue;
use hh_eval::compliance::attribution_completeness;
use hh_eval::facts::LedgerFacts;
use hh_eval::runs::{CacheState, EvalRun, SuiteContext, TaskContext};
use hh_eval::scorecard::{render_scorecard, ScorecardInput};
use hh_eval::{tripped_veto_ids, veto_id, VetoContext};
use hh_ontology::compliance::{Detector, MetricDeclaration};
use hh_ontology::control::OutcomeClass;
use hh_ontology::dimensions::DimensionId;
use hh_ontology::eval::{MetricValue, MetricValueKind};
use hh_ontology::lab::{ContaminationStratum, EnvironmentFamily, SplitLabel};
use hh_ontology::participant::{Observability, ParticipantClass};
use hh_wire::Json;

/// `(seq, class, payload)` — the `LedgerFacts::from_events` input shape.
fn ev(seq: u64, class: &str, p: Json) -> (u64, String, Json) {
    (seq, class.to_string(), p)
}

/// Two completed calls; `mc-1` attributed, `mc-2` not.
fn gap_facts() -> LedgerFacts {
    LedgerFacts::from_events(&[
        ev(
            1,
            "model.call.attempt.completed",
            Json::obj([("model_call_id", Json::str("mc-1"))]),
        ),
        ev(
            2,
            "model.call.attempt.completed",
            Json::obj([("model_call_id", Json::str("mc-2"))]),
        ),
        ev(
            3,
            "measurement.cost.attributed",
            Json::obj([("model_call_id", Json::str("mc-1"))]),
        ),
    ])
}

/// Both completed calls attributed.
fn clean_facts() -> LedgerFacts {
    LedgerFacts::from_events(&[
        ev(
            1,
            "model.call.attempt.completed",
            Json::obj([("model_call_id", Json::str("mc-1"))]),
        ),
        ev(
            2,
            "model.call.attempt.completed",
            Json::obj([("model_call_id", Json::str("mc-2"))]),
        ),
        ev(
            3,
            "measurement.cost.attributed",
            Json::obj([("model_call_id", Json::str("mc-1"))]),
        ),
        ev(
            4,
            "measurement.cost.attributed",
            Json::obj([("model_call_id", Json::str("mc-2"))]),
        ),
    ])
}

/// A passing `task_success` run row (the minimal scorecard fixture).
fn run(id: &str, veto_tripped: Vec<String>, facts: LedgerFacts) -> EvalRun {
    EvalRun {
        run_id: id.into(),
        arm_id: "A".into(),
        cell_id: Some(format!("A:{id}")),
        configuration_id: "cfg-A".into(),
        participant_class: ParticipantClass::Native,
        observability_level: [
            Observability::Events,
            Observability::ModelIo,
            Observability::EndState,
            Observability::Ledger,
        ]
        .into_iter()
        .collect(),
        mediation: BTreeSet::new(),
        capability_vector: BTreeMap::new(),
        task_id: "t1".into(),
        suite_id: "suite-test".into(),
        split_label: SplitLabel::HeldOut,
        replicate_index: 0,
        attempt_no: 1,
        seed: Some(42),
        seed_honoured: true,
        cache_state: CacheState::ColdStart,
        comparable: true,
        outcome_class: OutcomeClass::Scored,
        budget_consumed: [(DimensionId::ModelCalls, 5)].into_iter().collect(),
        veto_tripped,
        values: vec![MetricValue {
            metric_ref: "task_success".into(),
            value: MetricValueKind::Bool(true),
            applies_to: id.into(),
            oracle_ref: "oracle/executable".into(),
            detector: Detector::Deterministic,
            confidence: None,
            evidence_ref: None,
        }],
        environment_version_id: Some("env-1".into()),
        environment_family: EnvironmentFamily::CodingTerminal,
        fault_profile: None,
        perturbation_profile: None,
        model_snapshots: [("default".to_string(), "snap-1".to_string())]
            .into_iter()
            .collect(),
        stratum: ContaminationStratum::PrivateHeldOut,
        eval_search_spend: 0,
        routing_deviation: false,
        replayed_trajectory: false,
        served_from_cache_count: 0,
        cache_prefix_hit_ratio: None,
        split_hash: Some("sha256:split-1".into()),
        facts,
    }
}

fn scorecard<'a>(
    runs: &'a [EvalRun],
    tasks: &'a [TaskContext],
    suites: &'a [SuiteContext],
    decls: &'a [MetricDeclaration],
) -> hh_lab::analysis::ScorecardReport {
    let fams: BTreeMap<String, String> = BTreeMap::new();
    render_scorecard(&ScorecardInput {
        runs,
        tasks,
        suites,
        declarations: decls,
        metric_registry_version: "registry-test",
        price_table_version: "pt-1",
        watermark: 1,
        confidence_ppm: 950_000,
        pool_strata: false,
        annotate_pooling: false,
        model_families: &fams,
    })
    .unwrap()
}

fn fixture() -> (Vec<TaskContext>, Vec<SuiteContext>, Vec<MetricDeclaration>) {
    (
        vec![TaskContext {
            task_id: "t1".into(),
            suite_id: "suite-test".into(),
            split_label: SplitLabel::HeldOut,
            split_hash: "sha256:split-1".into(),
            stratum: ContaminationStratum::PrivateHeldOut,
        }],
        vec![SuiteContext {
            suite_id: "suite-test".into(),
            retired_for_headline: false,
            family: EnvironmentFamily::CodingTerminal,
        }],
        vec![catalogue::metric("task_success").expect("catalogue metric")],
    )
}

#[test]
fn unattributed_completed_call_trips_the_veto() {
    let f = gap_facts();
    let ids = tripped_veto_ids(&f, &VetoContext::default());
    assert_eq!(ids, vec![veto_id::ATTRIBUTION_COMPLETENESS.to_string()]);
    // The metric reports the rate beside the veto — 1 of 2 covered.
    assert_eq!(attribution_completeness(&f).ppm(), Some(500_000));
}

#[test]
fn complete_attribution_is_clean() {
    let f = clean_facts();
    assert!(tripped_veto_ids(&f, &VetoContext::default()).is_empty());
    assert_eq!(attribution_completeness(&f).ppm(), Some(1_000_000));
    // 0/0 is unmeasured — n/a, never 0.
    assert_eq!(
        attribution_completeness(&LedgerFacts::default()).ppm(),
        None
    );
}

#[test]
fn vetoed_run_is_success_with_veto_not_headline() {
    let (tasks, suites, decls) = fixture();
    let vetoed = run(
        "r-vetoed",
        tripped_veto_ids(&gap_facts(), &VetoContext::default()),
        gap_facts(),
    );
    let clean = run("r-clean", vec![], clean_facts());
    let runs = vec![vetoed, clean];
    let report = scorecard(&runs, &tasks, &suites, &decls);
    let cfg = &report.configurations[0];
    // The vetoed success is counted beside the headline, not in it.
    assert_eq!(cfg.vetoed_successes, 1);
    let cell = &cfg.cells[0];
    assert_eq!(cell.vetoed, 1);
    // Only the clean run feeds the headline point — 1/1 = 1.0 ppm.
    assert_eq!(cell.point, MetricValueKind::Decimal(1_000_000));
}

// ── AC-E5-01 signal side (§5d.5) — an unattributed capture-path signal ───
// trips the veto. The kernel emits `action.effect.unattributed` on
// `token_resolve_miss`; the veto reads the record, never re-derives
// resolution. The marker also survives the `ledger_facts/1` round trip so
// a reloaded run still trips (CC3).

#[test]
fn unattributed_signal_trips_the_veto() {
    let f = LedgerFacts::from_events(&[
        ev(
            1,
            "action.effect.unattributed",
            Json::obj([
                ("signal_kind", Json::str("executor_signal")),
                ("evidence_ref", Json::str("sha256:sig-1")),
                ("detection", Json::str("token_resolve_miss")),
            ]),
        ),
        ev(
            2,
            "lifecycle.run.finished",
            Json::obj([("stop_reason", Json::str("completed"))]),
        ),
    ]);
    let ids = tripped_veto_ids(&f, &VetoContext::default());
    assert_eq!(ids, vec![veto_id::ATTRIBUTION_COMPLETENESS.to_string()]);
    // The marker survives the record round trip (nothing silently lost).
    let reloaded = LedgerFacts::from_json(&f.to_json()).expect("ledger_facts/1 decodes");
    assert_eq!(reloaded.unattributed_signals.len(), 1);
    let ids2 = tripped_veto_ids(&reloaded, &VetoContext::default());
    assert!(ids2.contains(&veto_id::ATTRIBUTION_COMPLETENESS.to_string()));
}

// ── AC-R-2.5.3-9 (E3-9) — exposure-metric honesty pins ───────────────────────
// (a) `compare` refuses without a `MatchSpec` — already pinned in
// `tests/acceptance.rs::compare_without_matchspec_refuses` (MissingMatchSpec
// at the arm-check boundary). (b) `selection_recall@need` on a hosted,
// events-only participant renders `n/a{observability}` — the metric requires
// {events, model_io} and applies to native only, but an inapplicable *reason*
// surfaces the strongest binding reason; observability must surface, never
// coerced to 0 or silently dropped.
#[test]
fn ac_e3_9_hosted_selection_recall_is_na_observability() {
    use hh_eval::scorecard::render_cell;
    use hh_ontology::compliance::NaReason;

    let decl = hh_telemetry::catalogue::PROCESS_METRICS
        .iter()
        .find(|m| m.name == "selection_recall_at_need")
        .expect("selection_recall_at_need is registered (WS-I2)")
        .declaration();
    let mut hosted = run("h-hosted", vec![], clean_facts());
    hosted.participant_class = ParticipantClass::Hosted;
    // `events` only — `model_io` absent: the metric's observability floor
    // is not met.
    hosted.observability_level = [Observability::Events].into_iter().collect();
    let cell = render_cell(&decl, &[&hosted], None, &BTreeMap::new(), 950_000, "cfg-h");
    assert!(
        matches!(
            cell.point,
            MetricValueKind::Na(NaReason::Observability | NaReason::Class)
        ),
        "hosted selection_recall must be n/a (observability/class), never 0: {:?}",
        cell.point
    );
    // The inapplicability is counted beside the cell, never folded into it.
    assert!(!cell.excluded.is_empty(), "n/a reason counted in excluded");
}
