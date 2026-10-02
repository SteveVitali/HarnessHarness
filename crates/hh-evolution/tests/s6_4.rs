//! `hh-evolution` S6.4 coverage — R-2.9.5 6d / R-2.9.8 (§5h.5/§5h.8;
//! ADR-0325; AC-R-2.9.8-{6–9,11}): the campaign-side `consolidation_
//! candidates` view (the durable prefix's `active` transitions are the
//! cycle-count numerator — never a caller assertion), the
//! `check_retrained_pin_scope` expiry gate (a retrained-snapshot pin
//! without a `model_version_change`-expiry + covered-selector debt
//! refuses `ConditionedRuleIncomplete`), and the `CycleDriver` sidecar —
//! LabDocs deposit/restore re-folds the record byte-for-byte (CC3).

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_evolution::consolidation::{check_retrained_pin_scope, consolidation_candidates};
use hh_evolution::cycle::{doc_kind, CycleDriver};
use hh_evolution::errors::{EvolutionError, Refusal};
use hh_evolution::state::Transition;
use hh_evolution::view::{CampaignView, CandidateRecord};
use hh_experiment::docs::LabDocs;
use hh_lab::coevolution::{
    BrokenPolicy, ConsolidationPolicy, CyclePhase, CyclePolicy, CycleStopReason, LessonFact,
    NeverConsolidate, PhasePlan, SwitchRule,
};
use hh_lab::model::{PolicyVersionExposed, SnapshotClaim};
use hh_ontology::debt::ModelSelector;
use hh_wire::Json;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-evolution-s64-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn cand(id: &str, state: &str, actives: u64) -> CandidateRecord {
    CandidateRecord {
        candidate_id: id.into(),
        state: state.into(),
        slot: None,
        base_ref: None,
        diff_ref: None,
        hypothesis_ref: None,
        evidence_refs: vec![],
        hypothesis_kind: None,
        reports: BTreeMap::new(),
        history: (0..actives)
            .map(|i| Transition {
                from: "hypothesized".into(),
                to: "active".into(),
                stage: "S10".into(),
                code: None,
                report_ref: None,
                hypothesis_ref: None,
                evidence_refs: vec![],
                slot: None,
                by: None,
                reason: None,
                event_id: format!("ev:{id}:{i}"),
            })
            .collect(),
        attribution_label: None,
        reverted_to: None,
        target_ref: None,
        rebase_of: None,
    }
}

fn view(cands: Vec<CandidateRecord>) -> CampaignView {
    CampaignView {
        campaign_id: "camp:1".into(),
        candidates: cands
            .into_iter()
            .map(|c| (c.candidate_id.clone(), c))
            .collect(),
        ..CampaignView::default()
    }
}

fn lesson(rule: &str, kind: &str) -> LessonFact {
    LessonFact {
        lesson_id: format!("lesson:{rule}"),
        rule_id: rule.into(),
        kind: kind.into(),
        effect: Some("non_harmed".into()),
        provenance: None,
    }
}

fn claim() -> SnapshotClaim {
    SnapshotClaim {
        provider: "prov:acme".into(),
        model_id: "model:m-7".into(),
        snapshot_id: "snap:retrained-1".into(),
        serving_route: None,
        base_snapshot_ref: Some("snap:base-1".into()),
        training_lineage: None,
        trained_under: vec![],
        training_cutoff_claim: None,
        weights_digest: None,
        policy_version_exposed: PolicyVersionExposed::Supported,
    }
}

/// The `conditioned_debts` JSON shape `check_retrained_pin_scope`
/// reads — `{rule_id, record{expiry, scope}}`.
fn debt_json(rule_id: &str, expiry_condition: &str, covered: bool) -> Json {
    Json::obj([
        ("rule_id", Json::str(rule_id)),
        (
            "record",
            Json::obj([
                (
                    "expiry",
                    Json::obj([("condition", Json::str(expiry_condition))]),
                ),
                (
                    "scope",
                    Json::obj([(
                        "model_selectors",
                        if covered {
                            Json::Arr(vec![ModelSelector::Exact {
                                model_id: "model:m-7".into(),
                            }
                            .to_json()])
                        } else {
                            Json::Arr(vec![])
                        },
                    )]),
                ),
            ]),
        ),
    ])
}

// ── consolidation_candidates — the campaign view ────────────────────────────

/// `cycles_active` counts *durable* `→ active` transitions (the
/// campaign's own count); the k_cycles gate, the never-consolidate
/// legs, and the economics bound all apply.
#[test]
fn consolidation_view_counts_durable_active_cycles() {
    let v = view(vec![
        cand("cand:ok", "active", 2),
        cand("cand:young", "active", 0),
        cand("cand:rejected", "rejected", 3),
    ]);
    let lessons = vec![
        lesson("cand:ok", "formatting_improvement"),
        lesson("cand:young", "formatting_improvement"),
        lesson("cand:rejected", "formatting_improvement"),
    ];
    let mut econ = BTreeMap::new();
    econ.insert("cand:ok".to_string(), 4.0);
    econ.insert("cand:young".to_string(), 4.0);
    econ.insert("cand:rejected".to_string(), 4.0);
    let policy = ConsolidationPolicy {
        k_cycles: 2,
        economics_factor: 1.0,
    };
    let (cands, refused) = consolidation_candidates(&v, &lessons, &econ, &|_| true, &policy);
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0].rule_id, "cand:ok");
    let legs: BTreeMap<_, _> = refused.iter().cloned().collect();
    assert_eq!(legs["cand:rejected"], NeverConsolidate::Terminal);
    // `cand:young` fails k_cycles honestly — no row either way.
    assert!(!legs.contains_key("cand:young"));

    // An `n/a` economics row fails the leg honestly — a candidate
    // without a bound never lands.
    let econ_empty = BTreeMap::new();
    let (cands, _) = consolidation_candidates(&v, &lessons, &econ_empty, &|_| true, &policy);
    assert!(cands.is_empty());
}

// ── check_retrained_pin_scope — the policy-conditioned expiry gate ──────────

/// A retrained-snapshot pin requires every touched conditioned rule's
/// debt to carry `model_version_change` expiry + a covering
/// `model_selectors`; missing either leg refuses
/// `ConditionedRuleIncomplete` (R-2.9.5 6d; the gate would fail if
/// removed).
#[test]
fn retrained_pin_without_scoped_expiry_refuses() {
    let c = claim();
    let rules = vec!["rule:cond-1".to_string()];

    // No pin → vacuous pass.
    check_retrained_pin_scope(&rules, &[], None).unwrap();

    // Pin + correctly-scoped debt → pass.
    let good = debt_json("rule:cond-1", "model_version_change", true);
    check_retrained_pin_scope(&rules, &[good], Some(&c)).unwrap();

    // Pin + date-expiry → refuse.
    let bad_expiry = debt_json("rule:cond-1", "date", true);
    let err = check_retrained_pin_scope(&rules, &[bad_expiry], Some(&c)).unwrap_err();
    assert!(matches!(
        err,
        EvolutionError::Refusal(Refusal::ConditionedRuleIncomplete { .. })
    ));

    // Pin + version-expiry but uncovered selector → refuse.
    let uncovered = debt_json("rule:cond-1", "model_version_change", false);
    let err = check_retrained_pin_scope(&rules, &[uncovered], Some(&c)).unwrap_err();
    assert!(matches!(
        err,
        EvolutionError::Refusal(Refusal::ConditionedRuleIncomplete { .. })
    ));
}

// ── CycleDriver — the sidecar's durable half ────────────────────────────────

fn policy() -> CyclePolicy {
    CyclePolicy {
        weight_phase_budget: 100,
        harness_search_budget: Json::obj([("search", Json::Int(50))]),
        switch_rule: SwitchRule::CycleCount,
        max_cycles: 1,
        retention_set_ref: None,
        regression_policy: None,
        broken_policy: BrokenPolicy::ReSearch,
    }
}

/// `open → begin/complete → deposit → restore` re-folds the record
/// byte-for-byte; the deposit lands under `co_evolution_cycle` kind;
/// `matched_total` arithmetic is the record's only budget math.
#[test]
fn cycle_driver_deposits_and_restores_through_labdocs() {
    let root = tmp("cycle");
    let docs = LabDocs::open(&root).unwrap();

    let mut d = CycleDriver::open(policy(), "lineage:l1");
    d.begin_phase(
        CyclePhase::HarnessSearch,
        Json::obj([("base_ref", Json::str("snap:base-1"))]),
    )
    .unwrap();
    d.complete_phase(
        Json::obj([("candidate_refs", Json::Arr(vec![Json::str("cand:1")]))]),
        Some("exp:search-1"),
        None,
        Some("completed"),
        Json::obj([("search", Json::Int(40))]),
    )
    .unwrap();
    assert!(matches!(d.next(), PhasePlan::WeightUpdate));

    let r = d.deposit(&docs).unwrap();
    let back = CycleDriver::restore(&docs, &r)
        .unwrap()
        .expect("deposited doc restores");
    assert_eq!(back.record.to_json(), d.record.to_json());
    assert_eq!(
        back.record.budgets.get("search").and_then(Json::as_int),
        Some(40)
    );

    // A missing ref restores `None` — never a fabricated record.
    assert!(CycleDriver::restore(&docs, "doc:absent").unwrap().is_none());

    // The doc lands under the sidecar kind.
    let body = docs.get(doc_kind::CO_EVOLUTION_CYCLE, &r).unwrap().unwrap();
    assert_eq!(
        body.get("co_evolution_cycle").and_then(Json::as_str),
        Some("1")
    );

    // The full lap stops the cycle at `max_cycles`.
    for (phase, verdict) in [
        (CyclePhase::WeightUpdate, "completed"),
        (CyclePhase::ReEvaluation, "verified"),
        (CyclePhase::Consolidation, "absorbed"),
    ] {
        d.begin_phase(phase, Json::obj([])).unwrap();
        d.complete_phase(Json::obj([]), None, None, Some(verdict), Json::obj([]))
            .unwrap();
    }
    assert_eq!(d.record.cycles_completed(), 1);
    assert!(matches!(
        d.next(),
        PhasePlan::Stop {
            reason: CycleStopReason::MaxCycles
        }
    ));
    d.stop(CycleStopReason::MaxCycles, Some("completed"));
    let r2 = d.deposit(&docs).unwrap();
    let back = CycleDriver::restore(&docs, &r2).unwrap().unwrap();
    assert_eq!(back.record.stop_reason, Some(CycleStopReason::MaxCycles));
}
