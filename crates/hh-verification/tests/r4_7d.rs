//! `r4_7d` — DF-S1.21-2's residual: the §5f reference-corpus AC fixture
//! halves (ticket R4.7d). Every listed AC — AC-R-2.7.1-{5–10},
//! AC-R-2.7.2a-{2–8}, AC-R-2.7.3-{2–12} — runs executable over the committed
//! reference corpus under `tests/fixtures/refcorpus/` — hermetically (R4.7e
//! adds live judge consumption; no live plumbing lands here).
//!
//! The semantic legs these cells exercise are landed in `hharness-verification`
//! (`bind`/`claims`/`gate`/`evidence`/`validators`/`critics`/`critic_rt`/
//! `evalfold`/`reconciler`/`followed`/`events`); this battery is the corpus
//! half the deferral row names — fixtures, not inline constructions.

use std::collections::{BTreeMap, BTreeSet};

use hharness_identity::kinds::RecordKind;
use hharness_identity::refs::VersionedRef;
use hharness_ontology::participant::Observability as SurfaceObs;
use hharness_provenance::authority::{AuthorityClass, PersistenceScope};
use hharness_provenance::origin::Origin;
use hharness_provenance::record::ProvenanceRecord;
use hharness_verification::bind::{bind_claim, RowView};
use hharness_verification::claims::{reconcile_ledger_only, severity_for, Claim, CriterionState};
use hharness_verification::critic_rt::{
    admit_probe, admit_quarantined, calibration_status_at, check_bundle_frozen,
    check_critic_isolation, judge_comparative, panel_reduce, pi_admission, probe_refusal_payload,
    verdict_taint, CalibrationContext, PiAdmission, ProbeDomain, ProbeRequest,
};
use hharness_verification::critics::{
    check_independence, consume, declare_critic, AgreementCounts, CalibrationAgreement,
    CalibrationRecord, ConsumeUse, CriticDeclaration, CriticError, CriticSubject, CriticVerdict,
    IndependenceVector, ProbeCapability,
};
use hharness_verification::evalfold::{
    completion_decision, compute, na_observability, reward_hacking_gap, MetricValue,
    Observability as FoldObs,
};
use hharness_verification::events;
use hharness_verification::evidence::{
    admit_items, pin_check, BundleError, EvidenceBundle, EvidenceItem, OmissionReason,
};
use hharness_verification::followed::{deterministic_detector, FollowedResolution, NO_DETECTOR};
use hharness_verification::gate::{
    decision_stratum, evaluate_gate, hold_admissible, may_cause_hold, summarize,
    AcceptanceCriterion, ContractError, GateCriterion, GateFacts, HeldOutStatus, OpenEffect,
    TaskContract,
};
use hharness_verification::reconciler::{
    evaluate_gate_hosted, na_classes, EndStateAssertion, HostedGateFacts,
};
use hharness_verification::validators::{Finding, Verdict};
use hharness_verification::vocab::{
    AdmissionMode, Agreement, CalibrationStatus, ChargedTo, ClaimKind, CompletionPolicy,
    ContextIndependence, CriterionRole, CriticKind, CriticUse, Detector, DivergenceClass,
    EvidenceClass, EvidenceMode, ExtractedBy, GateVerdict, Grounding, OracleCause, OracleClass,
    Site, SnapshotIndependence, SubjectRef, VerdictPhase, VerdictStatus, VerdictType, VerdictValue,
    Visibility,
};
use hharness_wire::json::{parse, Json};

// ── corpus + builders ────────────────────────────────────────────────────────

const DETECTION: &str = include_str!("fixtures/refcorpus/detection_cells.json");
const GATE: &str = include_str!("fixtures/refcorpus/gate_cells.json");
const METRIC: &str = include_str!("fixtures/refcorpus/metric_cells.json");
const HOSTED: &str = include_str!("fixtures/refcorpus/hosted_cells.json");
const EVIDENCE: &str = include_str!("fixtures/refcorpus/evidence_cells.json");
const CRITIC: &str = include_str!("fixtures/refcorpus/critic_cells.json");
const CALIBRATION: &str = include_str!("fixtures/refcorpus/calibration_cells.json");
const RUN500: &str = include_str!("fixtures/refcorpus/run_500.json");

fn doc(text: &str) -> Json {
    parse(text).expect("corpus cell file parses")
}

fn cell<'a>(doc: &'a Json, member: &str, name: &str) -> &'a Json {
    let Json::Arr(cells) = doc.get(member).expect("member array") else {
        panic!("{member} is an array");
    };
    cells
        .iter()
        .find(|c| c.get("name").and_then(Json::as_str) == Some(name))
        .unwrap_or_else(|| panic!("cell {name} exists"))
}

fn s<'a>(j: &'a Json, key: &str) -> &'a str {
    j.get(key).and_then(Json::as_str).expect("string member")
}

fn authority(name: &str) -> AuthorityClass {
    AuthorityClass::ALL
        .into_iter()
        .find(|a| a.as_str() == name)
        .unwrap_or_else(|| panic!("authority {name}"))
}

fn rows_of<'a>(rows: &'a Json) -> Vec<RowView<'a>> {
    let Json::Arr(rs) = rows else {
        panic!("rows is an array")
    };
    rs.iter()
        .map(|r| RowView {
            seq: r.get("seq").and_then(Json::as_int).unwrap_or(0) as u64,
            class: s(r, "class"),
            payload: r.get("payload").expect("payload"),
            authority: authority(s(r, "authority")),
            scope_effect_id: r.get("scope_effect_id").and_then(Json::as_str),
        })
        .collect()
}

fn prov_delegate() -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::model("model/ref", "snap:ref", "call:1"),
        PersistenceScope::Run,
        1,
    )
}

fn prov_kernel() -> ProvenanceRecord {
    ProvenanceRecord::kernel("hir/kernel/reconcile:0", 1)
}

fn vref(id: &str) -> VersionedRef {
    VersionedRef::pinned(
        RecordKind::Validator,
        id,
        ProvenanceRecord::minted(
            Origin::kernel("hir/kernel/registry"),
            PersistenceScope::Definition,
            1,
        ),
    )
}

fn subject(spelling: &str) -> SubjectRef {
    match spelling.split_once(':') {
        None => SubjectRef::Run,
        Some(("effect", r)) => SubjectRef::Effect(r.into()),
        Some(("validator_target", r)) => SubjectRef::ValidatorTarget(r.into()),
        Some(("file", r)) => SubjectRef::File(r.into()),
        Some(("criterion", r)) => SubjectRef::Criterion(r.into()),
        Some((k, _)) => panic!("subject kind {k}"),
    }
}

fn claim_of(cell: &Json) -> Claim {
    let c = cell.get("claim").expect("claim member");
    let max_seq = cell
        .get("rows")
        .map(|rows| rows_of(rows).iter().map(|r| r.seq).max().unwrap_or(0))
        .unwrap_or(0);
    let at_seq = cell
        .get("claim_at_seq")
        .and_then(Json::as_int)
        .map(|s| s as u64)
        .unwrap_or(max_seq + 1);
    let evidence_refs = match c.get("evidence_refs") {
        Some(Json::Arr(rs)) => rs
            .iter()
            .filter_map(|r| r.as_str().map(str::to_string))
            .collect(),
        _ => vec![],
    };
    Claim {
        claim_id: format!("claim:{}", s(cell, "name")),
        run_id: "run:refcorpus".into(),
        model_call_id: "call:1".into(),
        at_seq,
        kind: ClaimKind::parse(s(c, "kind")).expect("claim kind"),
        subject: subject(s(c, "subject")),
        predicate: "asserted".into(),
        asserted: c.get("asserted").cloned().unwrap_or(Json::Null),
        evidence_refs,
        extracted_by: ExtractedBy::Structured("claims".into()),
        extraction_confidence_ppm: 1_000_000,
        criteria_status: vec![],
        provenance: prov_delegate(),
    }
}

fn criterion_of(c: &Json) -> AcceptanceCriterion {
    let visibility = match c.get("visibility").and_then(Json::as_str) {
        Some("held_out") => Visibility::HeldOut,
        _ => Visibility::Visible,
    };
    AcceptanceCriterion {
        criterion_id: s(c, "criterion_id").into(),
        validator_ref: s(c, "validator_ref").into(),
        record: hharness_verification::gate::ValidatesRecord {
            role: CriterionRole::Acceptance,
            phase: VerdictPhase::Completion,
            required: c
                .get("required")
                .map(|r| matches!(r, Json::Bool(true)))
                .unwrap_or(true),
            visibility,
            veto: false,
            evidence_requirements: vec![],
            window: None,
            weight_ppm: None,
        },
    }
}

fn contract_of(cell: &Json) -> Option<TaskContract> {
    let c = cell.get("contract")?;
    let Json::Arr(criteria) = c.get("criteria").expect("criteria") else {
        panic!("criteria array")
    };
    let criteria: Vec<AcceptanceCriterion> = criteria.iter().map(criterion_of).collect();
    let goal_ref = s(c, "goal_ref").to_string();
    Some(TaskContract {
        contract_id: TaskContract::compute_contract_id(&goal_ref, &criteria),
        goal_ref,
        criteria,
        invariants: vec![],
        completion_policy: CompletionPolicy::AllRequired,
        evidence_kinds_required: vec![],
        budget_ref: "budget:1".into(),
        sealed: true,
        task_value: None,
    })
}

fn hosted_facts_of(f: &Json) -> HostedGateFacts {
    let Json::Arr(assertions) = f.get("end_state_assertions").expect("assertions") else {
        panic!()
    };
    HostedGateFacts {
        completion_claim_kind: ClaimKind::parse(s(f, "completion_claim_kind")).unwrap(),
        completion_claim_agreement: agreement_of(s(f, "agreement")),
        end_state_assertions: assertions
            .iter()
            .map(|a| EndStateAssertion {
                assertion_ref: s(a, "assertion_ref").into(),
                declared: a.get("declared").cloned().unwrap_or(Json::Null),
                measured: a.get("measured").cloned(),
                measured_ref: a
                    .get("measured_ref")
                    .and_then(Json::as_str)
                    .map(str::to_string),
            })
            .collect(),
        holds_consumed: 0,
        holds_cap: 3,
    }
}

fn agreement_of(spelling: &str) -> Agreement {
    match spelling {
        "agree" => Agreement::Agree,
        "unverifiable" => Agreement::Unverifiable,
        "stale" => Agreement::Stale,
        _ => panic!("agreement {spelling}"),
    }
}

fn gate_facts_of(f: &Json) -> GateFacts {
    let open_effects = match f.get("open_effects") {
        Some(Json::Arr(es)) => es
            .iter()
            .map(|e| OpenEffect {
                effect_id: s(e, "effect_id").into(),
                state: s(e, "state").into(),
                detachable: matches!(e.get("detachable"), Some(Json::Bool(true))),
            })
            .collect(),
        _ => vec![],
    };
    let abandoned = match f.get("abandoned_effects") {
        Some(Json::Arr(es)) => es
            .iter()
            .filter_map(|e| e.as_str().map(str::to_string))
            .collect(),
        _ => vec![],
    };
    let criteria = match f.get("required_criteria") {
        Some(Json::Arr(cs)) => cs
            .iter()
            .map(|c| GateCriterion {
                criterion_ref: s(c, "criterion_ref").into(),
                state: CriterionState::parse(s(c, "state")).expect("criterion state"),
                bound_validators: match c.get("bound_validators") {
                    Some(Json::Arr(vs)) => vs
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect(),
                    _ => vec![],
                },
                unverifiable_reason: matches!(c.get("unverifiable_reason"), Some(Json::Bool(true))),
            })
            .collect(),
        _ => vec![],
    };
    let divergences = match f.get("claim_evidence_divergences") {
        Some(Json::Arr(ds)) => ds
            .iter()
            .filter_map(|d| d.as_str().and_then(DivergenceClass::parse))
            .collect(),
        _ => vec![],
    };
    GateFacts {
        completion_claim_kind: ClaimKind::parse(s(f, "completion_claim_kind")).expect("claim kind"),
        completion_claim_agreement: agreement_of(s(f, "agreement")),
        claim_evidence_refs: vec![],
        open_effects,
        abandoned_effects: abandoned,
        required_criteria: criteria,
        claim_evidence_divergences: divergences,
        holds_consumed: f.get("holds_consumed").and_then(Json::as_int).unwrap_or(0) as u64,
        holds_cap: f.get("holds_cap").and_then(Json::as_int).unwrap_or(3) as u64,
    }
}

fn assert_gate_expect(cell: &Json, result: &hharness_verification::gate::GateResult) {
    let expect = cell.get("expect").expect("expect member");
    match s(expect, "verdict") {
        "pass" => assert_eq!(result.verdict, GateVerdict::Pass, "{}", s(cell, "name")),
        "hold" => {
            let GateVerdict::Hold {
                divergences,
                required_actions,
            } = &result.verdict
            else {
                panic!("{} holds", s(cell, "name"))
            };
            if let Some(Json::Arr(ds)) = expect.get("divergences") {
                let want: BTreeSet<&str> = ds.iter().filter_map(Json::as_str).collect();
                let got: BTreeSet<&str> = divergences
                    .iter()
                    .map(|d: &DivergenceClass| d.as_str())
                    .collect();
                assert_eq!(got, want, "{} divergences", s(cell, "name"));
            }
            if let Some(Json::Arr(acts)) = expect.get("actions") {
                for a in acts.iter().filter_map(Json::as_str) {
                    assert!(
                        required_actions.iter().any(|r| r == a),
                        "{} action {a}",
                        s(cell, "name")
                    );
                }
            }
        }
        other => panic!("verdict {other}"),
    }
    if let Some(hc) = expect.get("hold_count").and_then(Json::as_int) {
        assert_eq!(result.hold_count, hc as u64);
    }
    if let Some(Json::Bool(he)) = expect.get("holds_exhausted") {
        assert_eq!(result.holds_exhausted, *he);
    }
    if let Some(Json::Bool(hf)) = expect.get("honest_failure") {
        assert_eq!(result.honest_failure, *hf);
    }
    if let Some(v) = expect.get("success_with_veto").and_then(Json::as_str) {
        assert_eq!(result.success_with_veto.as_deref(), Some(v));
    }
    if let Some(st) = expect.get("stratum").and_then(Json::as_str) {
        assert_eq!(decision_stratum(result), Some(st));
    }
}

fn decl_default() -> CriticDeclaration {
    CriticDeclaration {
        critic_ref: vref("sha256:crt"),
        oracle_id: "oracle:j".into(),
        oracle_class: OracleClass::Judge,
        deterministic: false,
        critic_kind: CriticKind::Judge,
        evidence_mode: [EvidenceMode::Transcript].into_iter().collect(),
        admission_mode: AdmissionMode::PrincipalOnly,
        probe_capabilities: vec![
            ProbeCapability::read_only("cap:fs_read"),
            ProbeCapability::read_only("cap:memory_write"),
        ],
        independence: IndependenceVector::kernel_independent(),
        site: Site::Runtime,
        use_: CriticUse::Gate,
        placement: Some("stop".into()),
        verdict_type: VerdictType::Bool,
        comparative: false,
        rubric_ref: None,
        calibration_ref: Some("cal:1".into()),
        held_out_from: vec![],
        debt: Some("debt:c".into()),
        charged_to: ChargedTo::Instrument,
        calibration_status: CalibrationStatus::Active,
        deterministic_coverage_at_placement: false,
    }
}

fn decl_of(cell: &Json) -> CriticDeclaration {
    let mut d = decl_default();
    if let Some(o) = cell.get("overrides") {
        if let Some(u) = o.get("use").and_then(Json::as_str) {
            d.use_ = match u {
                "report" => CriticUse::Report,
                _ => CriticUse::Gate,
            };
        }
        if let Some(k) = o.get("critic_kind").and_then(Json::as_str) {
            d.critic_kind = match k {
                "programmatic" => CriticKind::Programmatic,
                "monitor" => CriticKind::Monitor,
                "emulated" => CriticKind::Emulated,
                "agentic_judge" => CriticKind::AgenticJudge,
                _ => CriticKind::Judge,
            };
        }
        if let Some(det) = o.get("deterministic") {
            d.deterministic = matches!(det, Json::Bool(true));
        }
        if let Some(oc) = o.get("oracle_class").and_then(Json::as_str) {
            d.oracle_class = OracleClass::parse(oc).expect("oracle class");
        }
        if let Some(sn) = o.get("snapshot").and_then(Json::as_str) {
            d.independence.snapshot = match sn {
                "same_snapshot" => SnapshotIndependence::SameSnapshot,
                "different_snapshot_same_family" => {
                    SnapshotIndependence::DifferentSnapshotSameFamily
                }
                _ => SnapshotIndependence::DifferentFamily,
            };
        }
        if let Some(cx) = o.get("context").and_then(Json::as_str) {
            d.independence.context = match cx {
                "same_context" => ContextIndependence::SameContext,
                "shared_transcript" => ContextIndependence::SharedTranscript,
                _ => ContextIndependence::Fresh,
            };
        }
        if let Some(cp) = o.get("capability").and_then(Json::as_str) {
            d.independence.capability = match cp {
                "shared_mutable" => {
                    hharness_verification::vocab::CapabilityIndependence::SharedMutable
                }
                "shared_readonly" => {
                    hharness_verification::vocab::CapabilityIndependence::SharedReadonly
                }
                _ => hharness_verification::vocab::CapabilityIndependence::IsolatedReadonly,
            };
        }
        if let Some(p) = o.get("placement").and_then(Json::as_str) {
            d.placement = Some(p.into());
        }
        if let Some(cs) = o.get("calibration_status").and_then(Json::as_str) {
            d.calibration_status = match cs {
                "expired" => CalibrationStatus::Expired,
                "expiring" => CalibrationStatus::Expiring,
                _ => CalibrationStatus::Active,
            };
        }
        if let Some(am) = o.get("admission_mode").and_then(Json::as_str) {
            d.admission_mode = match am {
                "quarantined_external" => AdmissionMode::QuarantinedExternal,
                "all_labelled" => AdmissionMode::AllLabelled,
                _ => AdmissionMode::PrincipalOnly,
            };
        }
    }
    d
}

fn critic_verdict(id: &str) -> CriticVerdict {
    CriticVerdict {
        verdict_id: id.into(),
        critic_ref: vref("sha256:crt"),
        oracle_id: "oracle:j".into(),
        subject: CriticSubject {
            run_id: "run:refcorpus".into(),
            scope: "run".into(),
            until_seq: 9,
        },
        bundle_id: "b:1".into(),
        use_: CriticUse::Report,
        verdict: VerdictValue::Bool(true),
        status: VerdictStatus::Decided,
        confidence_ppm: 1_000_000,
        grounding: Grounding::Measured,
        findings: vec![],
        uncited_findings: 0,
        independence_summary: "s".into(),
        detector: Detector::Judged,
        calibration_ref: Some("cal:1".into()),
        charged_to: ChargedTo::Instrument,
        trigger_reason: None,
        critic_kind: CriticKind::Judge,
        position_calibrated: None,
        position_consistency_ppm: None,
        panel: None,
        held_out_from: None,
        provenance: prov_delegate(),
        at_seq: 9,
    }
}

fn item_of(it: &Json) -> EvidenceItem {
    EvidenceItem {
        item_ref: s(it, "item_ref").into(),
        evidence_class: match it.get("evidence_class").and_then(Json::as_str) {
            Some("claimed") => EvidenceClass::Claimed,
            Some("reconciled") => EvidenceClass::Reconciled,
            _ => EvidenceClass::Measured,
        },
        authority: authority(s(it, "authority")),
        provenance: prov_kernel(),
        label: "item".into(),
        bytes_or_view_hash: "h:1".into(),
        truncated: false,
    }
}

fn bundle_of(items: Vec<EvidenceItem>) -> EvidenceBundle {
    EvidenceBundle {
        bundle_id: "b:1".into(),
        handles: vec![],
        items,
        omitted: vec![],
        task_contract_ref: None,
        reference_ref: None,
        inputs_digest: "sha256:inputs-1".into(),
    }
}

fn scope_of(s: &str) -> PersistenceScope {
    match s {
        "run" => PersistenceScope::Run,
        "turn" => PersistenceScope::Turn,
        "session" => PersistenceScope::Session,
        "project" => PersistenceScope::Project,
        "user" => PersistenceScope::User,
        _ => PersistenceScope::Definition,
    }
}

// ── AC-R-2.7.1-* — the verification fabric ───────────────────────────────────

// AC-R-2.7.1-5 — no delivered artefact references a `held_out` criterion in
#[test]
fn ac_r_2_7_1_5_held_out_never_delivered_and_leak_refused() {
    let d = doc(EVIDENCE);
    let ho = d.get("held_out").expect("held_out member");
    let Json::Arr(criteria) = ho.get("criteria").expect("criteria") else {
        panic!("criteria")
    };
    let criteria: Vec<AcceptanceCriterion> = criteria.iter().map(criterion_of).collect();
    let held_out: BTreeSet<&str> = criteria
        .iter()
        .filter(|c| c.record.visibility == Visibility::HeldOut)
        .map(|c| c.criterion_id.as_str())
        .collect();
    assert!(held_out.contains("crit:hidden"));
    // The corpus run's `context.artefact.delivered` rows name only visible
    // criteria — zero leaks.
    let rows = rows_of(ho.get("delivered_rows").expect("delivered_rows"));
    for r in &rows {
        let cref = r
            .payload
            .get("criterion_ref")
            .and_then(Json::as_str)
            .unwrap_or_default();
        assert!(
            !held_out.contains(cref),
            "delivered artefact {} references held_out {cref}",
            r.seq
        );
    }
    // A deliberately leaky definition is refused `HeldOutLeak`.
    let leaky = criteria
        .iter()
        .find(|c| c.record.visibility == Visibility::HeldOut)
        .unwrap();
    assert_eq!(
        TaskContract::deliverable(leaky),
        Err(ContractError::HeldOutLeak {
            criterion_id: "crit:hidden".into()
        })
    );
    // …and the summary renders the held-out status, never a leak.
    let contract = TaskContract {
        contract_id: "contract:1".into(),
        goal_ref: "goal:1".into(),
        criteria: criteria.clone(),
        invariants: vec![],
        completion_policy: CompletionPolicy::AllRequired,
        evidence_kinds_required: vec![],
        budget_ref: "b".into(),
        sealed: true,
        task_value: None,
    };
    let summary = summarize(&contract, &BTreeMap::new(), vec![], 9);
    assert_eq!(summary.held_out_status, HeldOutStatus::DeferredToInstrument);
}

// AC-R-2.7.1-6 — a fault-injection arm editing a pinned payload trips
#[test]
fn ac_r_2_7_1_6_tamper_arm_evidence_tampered_clean_arm_ok() {
    let d = doc(EVIDENCE);
    for name in ["tampered_payload", "untampered_payload"] {
        let c = cell(&d, "pin_cells", name);
        let result = pin_check(s(c, "handle_ref"), s(c, "expected"), s(c, "observed"));
        match s(c, "expect") {
            "ok" => assert!(result.is_ok()),
            "evidence_tampered" => assert_eq!(
                result,
                Err(BundleError::EvidenceTampered {
                    handle_ref: s(c, "handle_ref").into()
                })
            ),
            _ => panic!(),
        }
    }
    // The instrument verdict records the tripped invariant on the row.
    let mut v = verdict("v-tamper");
    v.veto_tripped = vec!["evidence_tampered".into()];
    let payload = events::validator_verdict(&v);
    let Json::Arr(tripped) = payload.get("veto_tripped").expect("veto_tripped") else {
        panic!()
    };
    assert_eq!(tripped.as_slice(), [Json::str("evidence_tampered")]);
    // …and a run's only close beside the veto is the annotated
    // success-with-veto stratum, never a clean pass claim of the tamper.
    let g = doc(GATE);
    let abandoned = cell(&g, "cells", "abandoned_effect_success_with_veto");
    let result = evaluate_gate(&gate_facts_of(abandoned.get("facts").unwrap()));
    assert_eq!(result.verdict, GateVerdict::Pass);
    assert_eq!(
        result.success_with_veto.as_deref(),
        Some("inconsistent_durable_state")
    );
}

// AC-R-2.7.1-7 — `stop{completed}` with a required criterion `unmet`/`unrun`
#[test]
fn ac_r_2_7_1_7_completion_gate_decisions_over_corpus() {
    let d = doc(GATE);
    for name in [
        "completion_refused_required_unmet",
        "completion_refused_required_unrun",
        "clean_pass_all_met",
        "unverifiable_reason_never_holds",
    ] {
        let c = cell(&d, "cells", name);
        let result = evaluate_gate(&gate_facts_of(c.get("facts").unwrap()));
        assert_gate_expect(c, &result);
        // A refused completion is a hold — never a `succeeded` verdict.
        if name.starts_with("completion_refused") {
            assert!(matches!(result.verdict, GateVerdict::Hold { .. }));
        }
    }
    // `succeeded_unverified` is the canonical terminal spelling the corpus
    // folds (the contract's call — never `succeeded`).
    assert_eq!(
        hharness_verification::vocab::STATUS_SUCCEEDED_UNVERIFIED,
        "succeeded_unverified"
    );
    let m = doc(METRIC);
    let c = cell(&m, "cells", "succeeded_unverified_is_terminal");
    let rows = rows_of(c.get("rows").unwrap());
    assert_eq!(
        completion_decision(&rows).as_deref(),
        Some("succeeded_unverified")
    );
    let corpus = compute(&rows);
    let get = |name: &str| corpus.iter().find(|r| r.name == name).unwrap();
    assert_eq!(get("honest_failure_rate").value, MetricValue::Ppm(0));
    assert_eq!(get("false_completion_rate").value, MetricValue::Ppm(0));
    // Headline capability renders the typed `n/a{no_detector}` spelling —
    // never a fabricated zero.
    assert_eq!(NO_DETECTOR, "n/a{no_detector}");
    assert_eq!(
        deterministic_detector("instruction", false),
        FollowedResolution::NoDetector
    );
    // `succeeded_unverified` carries no veto — the headline metric rows
    // are real ppm values, never fabricated pass/fail zeroes.
    let veto = cell(&m, "cells", "fcr_veto_row");
    let veto_rows = rows_of(veto.get("rows").unwrap());
    let veto_corpus = compute(&veto_rows);
    assert_eq!(
        veto_corpus
            .iter()
            .find(|r| r.name == "false_completion_rate")
            .unwrap()
            .value,
        MetricValue::Ppm(1_000_000)
    );
}

// AC-R-2.7.1-8 — no headline value derives from a `charged_to = subject`
#[test]
fn ac_r_2_7_1_8_headline_never_subject_visible_and_rhg() {
    let d = doc(METRIC);
    let c = cell(&d, "cells", "subject_verdict_never_headline");
    let rows = rows_of(c.get("rows").unwrap());
    let corpus = compute(&rows);
    let get = |name: &str| corpus.iter().find(|r| r.name == name).unwrap();
    // The subject-charged visible pass lands only on `visible_pass_rate`;
    // the headline register reads the gate/claim rows — the veto fires on
    // the diverged completion, indifferent to the subject-side pass.
    assert_eq!(get("visible_pass_rate").value, MetricValue::Ppm(1_000_000));
    assert_eq!(
        get("execution_alignment_failure_rate").value,
        MetricValue::Ppm(1_000_000)
    );
    // `reward_hacking_gap` per configuration — None without the held-out
    // side (never fabricated from subject rows).
    let MetricValue::Profile(profile) = &get("divergence_profile").value else {
        panic!("divergence_profile is a profile")
    };
    assert_eq!(profile.get("contract_gap"), Some(&1_000_000));
    assert_eq!(reward_hacking_gap(900_000, Some(500_000)), Some(400_000));
    assert_eq!(reward_hacking_gap(900_000, None), None);
    // An `oracle_failure` row is excluded from the denominator, not read as
    // a fail.
    let c = cell(&d, "cells", "oracle_failure_excluded_from_denominator");
    let rows = rows_of(c.get("rows").unwrap());
    let corpus = compute(&rows);
    assert_eq!(
        corpus
            .iter()
            .find(|r| r.name == "visible_pass_rate")
            .unwrap()
            .value,
        MetricValue::Ppm(1_000_000),
        "oracle_failure is beside the denominator, never in it"
    );
}

// AC-R-2.7.1-9 — the deterministic detector exists for every delivered
#[test]
fn ac_r_2_7_1_9_followed_detectors_two_profiles() {
    for (kind, refs) in [
        ("tool_surface", false),
        ("procedure", true),
        ("kernel_rule", false),
        ("soft_budget_rule", false),
    ] {
        assert!(matches!(
            deterministic_detector(kind, refs),
            FollowedResolution::Deterministic(_)
        ));
    }
    assert_eq!(
        deterministic_detector("instruction", false),
        FollowedResolution::NoDetector
    );
    // A `validates` edge restores determinism — prose is judged-only only
    // absent the edge.
    assert!(matches!(
        deterministic_detector("instruction", true),
        FollowedResolution::Deterministic(_)
    ));
    assert_eq!(NO_DETECTOR, "n/a{no_detector}");
    let d = doc(METRIC);
    let c = cell(&d, "cells", "followed_share_two_profiles");
    let rows = rows_of(c.get("rows").unwrap());
    let corpus = compute(&rows);
    let share = corpus
        .iter()
        .find(|r| r.name == "deterministic_detector_share")
        .unwrap();
    let MetricValue::Profile(profile) = &share.value else {
        panic!("share is a profile")
    };
    assert_eq!(profile.get("profile:react"), Some(&1_000_000));
    assert_eq!(profile.get("profile:steer"), Some(&0));
}

// AC-R-2.7.1-10 — a hosted run at `end_state` observability receives the
#[test]
fn ac_r_2_7_1_10_hosted_end_state_same_verdicts() {
    let d = doc(HOSTED);
    for name in [
        "hosted_end_state_pass",
        "hosted_end_state_unmet_holds",
        "hosted_end_state_unread_holds",
        "hosted_honest_failure",
    ] {
        let c = cell(&d, "gate_cells", name);
        let result = evaluate_gate_hosted(&hosted_facts_of(c.get("facts").unwrap()));
        assert_gate_expect(c, &result);
    }
    // The hosted held-out verdict is the same `contract_gap` the native
    // floor produces — identical class, no hosted-only divergence.
    let g = doc(GATE);
    let native = cell(&g, "cells", "completion_refused_required_unmet");
    let native_result = evaluate_gate(&gate_facts_of(native.get("facts").unwrap()));
    let GateVerdict::Hold {
        divergences: native_d,
        ..
    } = native_result.verdict
    else {
        panic!()
    };
    let hosted = cell(&d, "gate_cells", "hosted_end_state_unmet_holds");
    let hosted_result = evaluate_gate_hosted(&hosted_facts_of(hosted.get("facts").unwrap()));
    let GateVerdict::Hold {
        divergences: hosted_d,
        ..
    } = hosted_result.verdict
    else {
        panic!()
    };
    assert_eq!(native_d, hosted_d, "hosted end-state verdict == native");
    // Every locally-attributed metric degrades `n/a{observability}` —
    // `false_completion_rate` survives on end-state oracles only.
    let m = doc(METRIC);
    let c = cell(&m, "cells", "fcr_veto_row");
    let rows = rows_of(c.get("rows").unwrap());
    let degraded = na_observability(compute(&rows), FoldObs::Opaque);
    for row in &degraded {
        if row.name == "false_completion_rate" {
            assert_ne!(row.value, MetricValue::NA("observability".into()));
        } else {
            assert_eq!(row.value, MetricValue::NA("observability".into()));
        }
    }
}

// ── AC-R-2.7.2a-* — claims, reconciliation, the gate ─────────────────────────

fn reconcile_cell(cell: &Json) -> hharness_verification::claims::ReconciliationRecord {
    let claim = claim_of(cell);
    let contract = contract_of(cell);
    let rows = rows_of(cell.get("rows").expect("rows"));
    let handles = bind_claim(&rows, &claim, contract.as_ref());
    reconcile_ledger_only(
        &claim,
        &handles,
        "hir/kernel/reconcile:0",
        600,
        prov_kernel(),
    )
}

// AC-R-2.7.2a-2 — each injected D2/D3/D5/D6 cell yields exactly one
#[test]
fn ac_r_2_7_2a_2_detection_corpus_yields_expected_diverge() {
    let d = doc(DETECTION);
    for (name, class_spelling) in [
        ("d2_phantom_effect", "phantom_effect"),
        ("d3_unverified_verification", "unverified_verification"),
        ("d5_contract_gap", "contract_gap"),
        ("d6_open_effect_at_completion", "open_effect_at_completion"),
    ] {
        let c = cell(&d, "cells", name);
        let claim = claim_of(c);
        let class = DivergenceClass::parse(class_spelling).unwrap();
        let contract = contract_of(c);
        let rows = rows_of(c.get("rows").unwrap());
        let handles = bind_claim(&rows, &claim, contract.as_ref());
        let r = reconcile_ledger_only(
            &claim,
            &handles,
            "hir/kernel/reconcile:0",
            600,
            prov_kernel(),
        );
        assert_eq!(r.agreement, Agreement::Diverge(class), "{name}");
        assert_eq!(r.detector, Detector::Deterministic, "{name}");
        // Resolvable evidence_refs — every ref names a bound handle or the
        // record's own refs, never the claim's text.
        assert!(!r.evidence_refs.is_empty(), "{name}");
        let bound: BTreeSet<&str> = handles.iter().map(|h| h.handle_ref.as_str()).collect();
        for ev in &r.evidence_refs {
            assert!(
                bound.contains(ev.as_str()) || ev.contains(':'),
                "{name}: evidence_ref {ev} resolves"
            );
        }
        // Severity per the register (the claim's completion context).
        let on_completion = matches!(claim.kind, ClaimKind::Achieved | ClaimKind::Unachievable);
        assert_eq!(r.severity, severity_for(class, on_completion), "{name}");
        r.validate().expect("{name} validates");
        // The reconciled payload is canonical + instrument-charged.
        let payload = events::claim_reconciled(&r);
        assert_eq!(
            payload.get("charged_to").and_then(Json::as_str),
            Some("instrument"),
            "{name}"
        );
        assert_eq!(
            payload.get("divergence_class").and_then(Json::as_str),
            Some(class_spelling),
            "{name}"
        );
    }
    // The merged-run claim (parent + subagent streams merged by seq)
    // reconciles `agree` — merge halves claim no authority they lack.
    let c = cell(&d, "cells", "merged_run_claim");
    let r = reconcile_cell(c);
    assert_eq!(r.agreement, Agreement::Agree);
}

// AC-R-2.7.2a-3 — the negative strata (honest claims, `assumption`,
#[test]
fn ac_r_2_7_2a_3_negative_corpus_zero_diverge() {
    let d = doc(DETECTION);
    for name in [
        "neg_honest_effected_applied",
        "neg_assumption_claim",
        "neg_honest_unachievable",
        "neg_retry_with_variation",
        "neg_incremental_same_file_edits",
        "neg_rerun_tests_after_change",
        "neg_hosted_run_without_events",
    ] {
        let c = cell(&d, "cells", name);
        let r = reconcile_cell(c);
        assert!(
            !matches!(r.agreement, Agreement::Diverge(_)),
            "{name}: zero diverge (got {:?})",
            r.agreement
        );
        match s(c.get("expect").unwrap(), "agreement") {
            "agree" => assert_eq!(r.agreement, Agreement::Agree, "{name}"),
            "unverifiable" => assert_eq!(r.agreement, Agreement::Unverifiable, "{name}"),
            _ => {}
        }
    }
}

// AC-R-2.7.2a-4 — the kill/resume re-entry: a `committed`-but-unobserved
#[test]
fn ac_r_2_7_2a_4_kill_resume_gate_reentry() {
    let d = doc(GATE);
    for name in [
        "kill_resume_committed_unobserved",
        "detachable_children_annotate_only",
        "abandoned_effect_success_with_veto",
        "claim_evidence_divergence_holds",
    ] {
        let c = cell(&d, "cells", name);
        let result = evaluate_gate(&gate_facts_of(c.get("facts").unwrap()));
        assert_gate_expect(c, &result);
    }
    // The hold notice appears exactly once per record (F6 dedup by
    // (class, subject) per turn).
    let dd = doc(DETECTION);
    let c = cell(&dd, "cells", "d2_phantom_effect");
    let claim = claim_of(c);
    let r = reconcile_cell(c);
    let mut delivered = BTreeSet::new();
    let claims = [claim];
    let notices = hharness_verification::reconciler::feed_back_notices(
        std::slice::from_ref(&r),
        claims.iter(),
        &mut delivered,
    );
    assert_eq!(notices.len(), 1);
    let again =
        hharness_verification::reconciler::feed_back_notices(&[r], claims.iter(), &mut delivered);
    assert!(
        again.is_empty(),
        "the notice fires once per record per turn"
    );
}

// AC-R-2.7.2a-5 — with `reconciliation.holds = 3` and a fixture that never
#[test]
fn ac_r_2_7_2a_5_holds_budget_exhaustion_strata() {
    let d = doc(GATE);
    let c = cell(&d, "cells", "holds_budget_exhausted");
    let result = evaluate_gate(&gate_facts_of(c.get("facts").unwrap()));
    assert_gate_expect(c, &result);
    assert!(result.holds_exhausted);
    assert_eq!(
        decision_stratum(&result),
        Some(hharness_verification::vocab::STRATUM_UNRECONCILED_CLAIMS)
    );
    let c = cell(&d, "cells", "honest_failure_unachievable");
    let result = evaluate_gate(&gate_facts_of(c.get("facts").unwrap()));
    assert!(result.honest_failure);
    assert_eq!(
        decision_stratum(&result),
        Some(hharness_verification::vocab::STRATUM_FAILED_HONESTLY)
    );
}

// AC-R-2.7.2a-6 — no `hold`/`veto` is traceable solely to a `judged` or
#[test]
fn ac_r_2_7_2a_6_judged_records_never_cause_hold() {
    assert!(!hold_admissible(Detector::Judged));
    assert!(!hold_admissible(Detector::Human));
    assert!(hold_admissible(Detector::Deterministic));
    assert!(!may_cause_hold(Detector::Deterministic, true));
    assert!(!may_cause_hold(Detector::Judged, false));
    let d = doc(DETECTION);
    let c = cell(&d, "cells", "neg_judged_contradiction");
    let r = reconcile_cell(c);
    assert!(!matches!(r.agreement, Agreement::Diverge(_)));
    // A judged verdict row never binds as a handle — the fold is
    // `unverifiable`, never a contradiction.
    assert_eq!(r.agreement, Agreement::Unverifiable);
}

// AC-R-2.7.2a-7 — a hosted run renders the locally-attributed metrics
#[test]
fn ac_r_2_7_2a_7_hosted_metrics_na_fcr_end_state() {
    let d = doc(HOSTED);
    let c = cell(&d, "na_cells", "opaque_run_local_metrics_na");
    let survivor = s(c, "expect_survivor");
    let m = doc(METRIC);
    let src = cell(&m, "cells", "fcr_veto_row");
    let rows = rows_of(src.get("rows").unwrap());
    let full = compute(&rows);
    assert_eq!(
        full.iter().find(|r| r.name == survivor).unwrap().value,
        MetricValue::Ppm(1_000_000)
    );
    let degraded = na_observability(compute(&rows), FoldObs::Opaque);
    for row in &degraded {
        if row.name == survivor {
            assert_eq!(
                row.value,
                full.iter().find(|f| f.name == row.name).unwrap().value
            );
        } else {
            assert_eq!(row.value, MetricValue::NA("observability".into()));
        }
    }
}

// AC-R-2.7.2a-8 — `ledger_only` reconciliation of the recorded 500-event
#[test]
fn ac_r_2_7_2a_8_ledger_only_rebuild_equality_500() {
    let d = doc(RUN500);
    let rows = rows_of(d.get("rows").expect("rows"));
    assert_eq!(rows.len(), 500);
    let claim = Claim {
        claim_id: "claim:rebuild".into(),
        run_id: "run:refcorpus".into(),
        model_call_id: "call:1".into(),
        at_seq: 501,
        kind: ClaimKind::Achieved,
        subject: SubjectRef::Run,
        predicate: "asserted".into(),
        asserted: Json::str("done"),
        evidence_refs: vec![],
        extracted_by: ExtractedBy::Structured("claims".into()),
        extraction_confidence_ppm: 1_000_000,
        criteria_status: vec![],
        provenance: prov_delegate(),
    };
    let contract = TaskContract {
        contract_id: "contract:500".into(),
        goal_ref: "goal:1".into(),
        criteria: (0..5)
            .map(|i| {
                criterion_of(&Json::obj([
                    ("criterion_id", Json::str(format!("crit:{i}"))),
                    ("validator_ref", Json::str("val:1")),
                ]))
            })
            .collect(),
        invariants: vec![],
        completion_policy: CompletionPolicy::AllRequired,
        evidence_kinds_required: vec![],
        budget_ref: "b".into(),
        sealed: true,
        task_value: None,
    };
    let h1 = bind_claim(&rows, &claim, Some(&contract));
    let h2 = bind_claim(&rows, &claim, Some(&contract));
    assert_eq!(h1, h2, "rebuild equality — the bound handles");
    let r1 = reconcile_ledger_only(&claim, &h1, "hir/kernel/reconcile:0", 600, prov_kernel());
    let r2 = reconcile_ledger_only(&claim, &h2, "hir/kernel/reconcile:0", 600, prov_kernel());
    assert_eq!(r1, r2);
    assert_eq!(
        events::claim_reconciled(&r1).to_canonical_string(),
        events::claim_reconciled(&r2).to_canonical_string(),
        "view_hash equality — the reconciled bytes"
    );
    assert_eq!(
        events::claim_reconciled(&r1)
            .get("charged_to")
            .and_then(Json::as_str),
        Some("instrument"),
        "per-claim deterministic check cost is instrument work"
    );
}

// ── AC-R-2.7.3-* — critics, judges, independence, calibration ────────────────

// AC-R-2.7.3-2 — a fixture judge emitting three findings, one uncited:
#[test]
fn ac_r_2_7_3_2_uncited_finding_dropped_counted() {
    let d = doc(CRITIC);
    let cell = d.get("uncited_cell").unwrap();
    let Json::Arr(items) = cell.get("bundle_items").unwrap() else {
        panic!()
    };
    let bundle = bundle_of(
        items
            .iter()
            .map(|r| {
                item_of(&Json::obj([
                    ("item_ref", r.clone()),
                    ("authority", Json::str("kernel")),
                    ("evidence_class", Json::str("measured")),
                ]))
            })
            .collect(),
    );
    let Json::Arr(fs) = cell.get("findings").unwrap() else {
        panic!()
    };
    let mut v = critic_verdict("v-uncited");
    v.findings = fs
        .iter()
        .map(|f| Finding {
            code: s(f, "code").into(),
            severity: hharness_verification::vocab::SeverityLevel::Low,
            location: None,
            message: s(f, "message").into(),
            evidence_ref: f
                .get("evidence_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
        })
        .collect();
    v.prune_uncited(&bundle);
    assert_eq!(v.findings.len(), 2, "the uncited finding drops");
    assert_eq!(v.uncited_findings, 1);
    assert_eq!(v.status, VerdictStatus::Decided, "the verdict stands");
}

// AC-R-2.7.3-3 — a `gate` judge declared `same_snapshot` fails
#[test]
fn ac_r_2_7_3_3_independence_declarations() {
    let d = doc(CRITIC);
    let Json::Arr(cells) = d.get("declaration_cells").unwrap() else {
        panic!()
    };
    for c in cells {
        let decl = decl_of(c);
        let expect = c.get("expect").unwrap();
        if let Some(err) = expect.get("err").and_then(Json::as_str) {
            let got = declare_critic(&decl).expect_err(s(c, "name"));
            match err {
                "judge_not_independent" => {
                    assert!(matches!(got, CriticError::JudgeNotIndependent { .. }))
                }
                "gate_without_calibration" => {
                    assert_eq!(got, CriticError::GateWithoutCalibration)
                }
                "independence_below_minimum" => {
                    assert!(matches!(got, CriticError::IndependenceBelowMinimum { .. }))
                }
                _ => panic!("err {err}"),
            }
        } else {
            declare_critic(&decl).expect("registers");
            // The weaker class is recorded on the declaration — never
            // widened to `different_family`.
            if let Some(axis) = expect.get("summary_axis").and_then(Json::as_str) {
                assert!(
                    decl.independence.summary().contains(axis),
                    "the weaker axis rides the summary"
                );
                assert!(check_independence(&decl).is_ok());
            }
        }
    }
}

// AC-R-2.7.3-4 — a probe attempting `fs_write`/`net_egress`/
#[test]
fn ac_r_2_7_3_4_probe_refusals_and_frozen_bundle() {
    let d = doc(CRITIC);
    let decl = decl_default();
    let Json::Arr(cells) = d.get("probe_cells").unwrap() else {
        panic!()
    };
    for c in cells {
        let req = ProbeRequest {
            capability_ref: s(c, "capability_ref").into(),
            domain: ProbeDomain::parse(s(c, "domain")).expect("domain"),
            scope: scope_of(s(c, "scope")),
        };
        match s(c, "expect") {
            "ok" => assert!(admit_probe(&decl, &req).is_ok(), "{}", s(c, "name")),
            class => {
                let err = admit_probe(&decl, &req).expect_err("refused");
                let payload = probe_refusal_payload(&err);
                assert_eq!(payload.get("refused"), Some(&Json::Bool(true)));
                assert_eq!(
                    payload.get("class").and_then(Json::as_str),
                    Some(class),
                    "{} — the refusal is recorded",
                    s(c, "name")
                );
            }
        }
    }
    // Spawn-time isolation: a write/spawn domain or a capability outside
    // the parent's is refused.
    let Json::Arr(icells) = d.get("isolation_cells").unwrap() else {
        panic!()
    };
    for c in icells {
        let mut decl = decl_default();
        decl.probe_capabilities = vec![ProbeCapability::read_only(s(c, "capability_ref"))];
        let Json::Arr(parent) = c.get("parent").unwrap() else {
            panic!()
        };
        let parent: BTreeSet<String> = parent
            .iter()
            .filter_map(|p| p.as_str().map(str::to_string))
            .collect();
        match s(c, "expect") {
            "ok" => assert!(check_critic_isolation(&decl, &parent).is_ok()),
            _ => assert!(matches!(
                check_critic_isolation(&decl, &parent),
                Err(CriticError::IsolationViolation { .. })
            )),
        }
    }
    // The bundle is frozen — a mutated observed digest is EvidenceTampered.
    let e = doc(EVIDENCE);
    let Json::Arr(fcells) = e.get("frozen_cells").unwrap() else {
        panic!()
    };
    for c in fcells {
        let mut b = bundle_of(vec![]);
        b.inputs_digest = s(c, "inputs_digest").into();
        let got = check_bundle_frozen(&b, s(c, "observed_digest"));
        match s(c, "expect") {
            "ok" => assert!(got.is_ok()),
            _ => assert!(matches!(got, Err(BundleError::EvidenceTampered { .. }))),
        }
    }
}

// AC-R-2.7.3-5 — a runtime completion critic's verdict never surfaces as
#[test]
fn ac_r_2_7_3_5_consume_never_task_success_leak_refused() {
    let d = doc(CRITIC);
    let Json::Arr(cells) = d.get("consume_cells").unwrap() else {
        panic!()
    };
    for c in cells {
        let mut v = critic_verdict(s(c, "name"));
        v.use_ = match s(c, "verdict_use") {
            "gate" => CriticUse::Gate,
            _ => CriticUse::Report,
        };
        if let Some(k) = c.get("critic_kind").and_then(Json::as_str) {
            v.critic_kind = match k {
                "monitor" => CriticKind::Monitor,
                "emulated" => CriticKind::Emulated,
                _ => CriticKind::Judge,
            };
        }
        if let Some(Json::Arr(ho)) = c.get("held_out_from") {
            v.held_out_from = Some(
                ho.iter()
                    .filter_map(|p| p.as_str().map(str::to_string))
                    .collect(),
            );
        }
        let consume_use = match c.get("consume") {
            Some(Json::Str(u)) => match u.as_str() {
                "task_success" => ConsumeUse::TaskSuccess,
                "headline" => ConsumeUse::Headline,
                "gate" => ConsumeUse::Gate,
                _ => ConsumeUse::Report,
            },
            Some(Json::Obj(_)) => ConsumeUse::Evolution {
                producer: s(c.get("consume").unwrap(), "evolution").into(),
            },
            _ => ConsumeUse::Report,
        };
        match s(c, "expect") {
            "ok" => assert!(consume(&v, consume_use).is_ok(), "{}", s(c, "name")),
            "verdict_misuse" => assert!(matches!(
                consume(&v, consume_use),
                Err(CriticError::VerdictMisuse { .. })
            )),
            "judge_leaked_into_artifact" => assert!(matches!(
                consume(&v, consume_use),
                Err(CriticError::JudgeLeakedIntoArtifact { .. })
            )),
            e => panic!("expect {e}"),
        }
    }
}

// AC-R-2.7.3-6 — a comparative judge evaluated in one order only is
#[test]
fn ac_r_2_7_3_6_comparative_two_orders() {
    let d = doc(CRITIC);
    let Json::Arr(cells) = d.get("comparative_cells").unwrap() else {
        panic!()
    };
    for c in cells {
        let Json::Arr(orders) = c.get("orders").unwrap() else {
            panic!()
        };
        let a = critic_verdict("v-ab");
        let b = critic_verdict("v-ba");
        let (a, b) = if let Some(Json::Arr(vals)) = c.get("values") {
            let mut a = a;
            let mut b = b;
            a.verdict = VerdictValue::Bool(matches!(vals[0], Json::Bool(true)));
            b.verdict = VerdictValue::Bool(matches!(vals[1], Json::Bool(true)));
            (a, b)
        } else {
            (a, b)
        };
        let got = match orders.len() {
            1 => judge_comparative(Some(&a), None),
            _ => judge_comparative(Some(&a), Some(&b)),
        };
        let expect = c.get("expect").unwrap();
        if expect.get("comparative_incomplete").is_some()
            || matches!(expect, Json::Str(e) if e == "comparative_incomplete")
        {
            assert_eq!(got, Err(CriticError::ComparativeIncomplete));
            continue;
        }
        let agg = got.expect("two orders aggregate");
        assert_eq!(
            agg.position_calibrated,
            expect
                .get("calibrated")
                .map(|v| matches!(v, Json::Bool(true)))
        );
        assert_eq!(
            agg.position_consistency_ppm,
            expect
                .get("consistency_ppm")
                .and_then(Json::as_int)
                .map(|p| p as u64)
        );
        let want_affirmative = matches!(expect.get("verdict"), Some(Json::Bool(true)));
        assert_eq!(agg.verdict.is_affirmative(), want_affirmative);
    }
}

// AC-R-2.7.3-7 — an out-of-set/unparseable verdict is `oracle_failure`,
#[test]
fn ac_r_2_7_3_7_oracle_failure_and_panel_withhold() {
    let d = doc(CRITIC);
    let cell = d.get("panel_cell").unwrap();
    let Json::Arr(ms) = cell.get("members").unwrap() else {
        panic!()
    };
    let members: Vec<CriticVerdict> = ms
        .iter()
        .map(|m| {
            let mut v = critic_verdict(s(m, "verdict_id"));
            v.status = match s(m, "status") {
                "oracle_failure" => VerdictStatus::OracleFailure(match s(m, "cause") {
                    "unparseable" => OracleCause::Unparseable,
                    _ => OracleCause::Crash,
                }),
                _ => VerdictStatus::Decided,
            };
            v.verdict = match m.get("value") {
                Some(Json::Bool(b)) => VerdictValue::Bool(*b),
                _ => VerdictValue::Bool(false),
            };
            v
        })
        .collect();
    let agg = panel_reduce(&members).expect("panel reduces");
    let panel = agg.panel.expect("panel record");
    assert_eq!(panel.withheld, vec!["m-3".to_string()]);
    assert_eq!(panel.decision_rule, "majority");
    assert_eq!(panel.members.len(), 4);
    // The unparseable member never became a grade; the decided majority
    // stands.
    assert!(agg.verdict.is_affirmative());
    assert_eq!(agg.status, VerdictStatus::Decided);
    // An `oracle_failure` status is a class, never a grade — the withheld
    // member contributes no vote either way.
}

// AC-R-2.7.3-8 — the calibration fold: fingerprint/rubric/family change
#[test]
fn ac_r_2_7_3_8_calibration_status_folds() {
    let d = doc(CALIBRATION);
    let r = d.get("record").unwrap();
    let record = CalibrationRecord {
        calibration_id: s(r, "calibration_id").into(),
        critic_ref: s(r, "critic_ref").into(),
        judge_snapshot_fingerprint: s(r, "judge_snapshot_fingerprint").into(),
        labelled_set_ref: s(r, "labelled_set_ref").into(),
        n_per_verdict_class: 10,
        reference_oracle: hharness_verification::vocab::ReferenceOracle::EndState,
        agreement: CalibrationAgreement {
            statistic: "cohen_kappa".into(),
            value_ppm: 800_000,
            interval: "[0.7,0.9]".into(),
            method: "held_out".into(),
        },
        error_rates: Json::obj([]),
        position_consistency_ppm: 900_000,
        self_consistency_ppm: None,
        self_preference_check: None,
        evidence_ablation_ppm: Some(500_000),
        calibrated_at: r.get("calibrated_at").and_then(Json::as_int).unwrap() as u64,
        expiry: match r.get("expiry") {
            Some(Json::Arr(es)) => es
                .iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect(),
            _ => vec![],
        },
        status: CalibrationStatus::Active,
    };
    let Json::Arr(cells) = d.get("cells").unwrap() else {
        panic!()
    };
    for c in cells {
        let ctx = c.get("ctx").unwrap();
        let ctx = CalibrationContext {
            judge_snapshot_fingerprint: s(ctx, "judge_snapshot_fingerprint").into(),
            rubric_ref: s(ctx, "rubric_ref").into(),
            task_family: s(ctx, "task_family").into(),
            now_ms: ctx.get("now_ms").and_then(Json::as_int).unwrap() as u64,
            recalibration_period_ms: r
                .get("recalibration_period_ms")
                .and_then(Json::as_int)
                .unwrap() as u64,
        };
        let want = match s(c, "expect") {
            "active" => CalibrationStatus::Active,
            "expiring" => CalibrationStatus::Expiring,
            _ => CalibrationStatus::Expired,
        };
        assert_eq!(
            calibration_status_at(&record, &ctx),
            want,
            "{}",
            s(c, "name")
        );
    }
}

// AC-R-2.7.3-9 — the agreement fold renders the declared rates (the
#[test]
fn ac_r_2_7_3_9_agreement_counts_declared_rates() {
    let d = doc(CRITIC);
    let c = d.get("agreement_cell").unwrap();
    let counts = c.get("counts").unwrap();
    let counts = AgreementCounts {
        agree: counts.get("agree").and_then(Json::as_int).unwrap() as u64,
        diverge: counts.get("diverge").and_then(Json::as_int).unwrap() as u64,
        unverifiable: counts.get("unverifiable").and_then(Json::as_int).unwrap() as u64,
        stale: counts.get("stale").and_then(Json::as_int).unwrap() as u64,
    };
    let expect = c.get("expect").unwrap();
    assert_eq!(
        counts.claim_state_agreement_ppm(),
        Some(
            expect
                .get("claim_state_agreement_ppm")
                .and_then(Json::as_int)
                .unwrap() as u64
        )
    );
    assert_eq!(
        counts.has_c0_divergence(),
        matches!(expect.get("has_c0_divergence"), Some(Json::Bool(true)))
    );
    // `unverifiable`/`stale` are counted beside the rate, never inside it.
    assert_eq!(AgreementCounts::default().claim_state_agreement_ppm(), None);
}

// AC-R-2.7.3-10 — every verdict is a `verification.validator.verdict`
#[test]
fn ac_r_2_7_3_10_verdict_event_and_genai_lowering() {
    let mut v = verdict("v-j1");
    v.detector = Detector::Judged;
    v.oracle_class = OracleClass::Judge;
    v.bundle_id = Some("b:1".into());
    v.calibration_ref = Some("cal:1".into());
    v.independence_summary = Some("snapshot=different_family".into());
    v.uncited_findings = 1;
    let payload = events::validator_verdict(&v);
    assert_eq!(
        payload.get("detector").and_then(Json::as_str),
        Some("judged")
    );
    assert_eq!(payload.get("bundle_id").and_then(Json::as_str), Some("b:1"));
    assert_eq!(
        payload.get("calibration_ref").and_then(Json::as_str),
        Some("cal:1")
    );
    assert_eq!(
        payload.get("independence_summary").and_then(Json::as_str),
        Some("snapshot=different_family")
    );
    assert_eq!(payload.get("uncited_findings"), Some(&Json::Int(1)));
    // The `gen_ai.evaluation.result` lowering + loss-report half runs in
    // `crates/hh-telemetry/tests/r4_7d.rs` over this same emitted payload.
}

// AC-R-2.7.3-11 — `principal_only` omits the injected item *and lists it*;
#[test]
fn ac_r_2_7_3_11_admission_omitted_quarantine_raise_only() {
    let d = doc(EVIDENCE);
    let Json::Arr(cells) = d.get("admission_cells").unwrap() else {
        panic!()
    };
    for c in cells {
        let mode = match s(c, "mode") {
            "principal_only" => AdmissionMode::PrincipalOnly,
            _ => AdmissionMode::QuarantinedExternal,
        };
        let Json::Arr(items) = c.get("items").unwrap() else {
            panic!()
        };
        let items: Vec<EvidenceItem> = items.iter().map(item_of).collect();
        let (kept, omitted) = admit_items(mode, items.clone());
        let kept_refs: Vec<&str> = kept.iter().map(|i| i.item_ref.as_str()).collect();
        let omitted_refs: Vec<&str> = omitted.iter().map(|o| o.item_ref.as_str()).collect();
        let Json::Arr(want_kept) = c.get("expect_kept").unwrap() else {
            panic!()
        };
        let want_kept: Vec<&str> = want_kept.iter().filter_map(Json::as_str).collect();
        let Json::Arr(want_omitted) = c.get("expect_omitted").unwrap() else {
            panic!()
        };
        let want_omitted: Vec<&str> = want_omitted.iter().filter_map(Json::as_str).collect();
        assert_eq!(kept_refs, want_kept, "{}", s(c, "name"));
        assert_eq!(
            omitted_refs,
            want_omitted,
            "{} — omitted and listed",
            s(c, "name")
        );
        for o in &omitted {
            assert_eq!(o.reason, OmissionReason::Admission);
        }
        if mode == AdmissionMode::QuarantinedExternal {
            let relabelled = admit_quarantined(kept, |a| format!("role_map({})", a.as_str()));
            let injected = relabelled
                .iter()
                .find(|i| i.item_ref == "it:injected")
                .unwrap();
            assert_eq!(injected.label, "role_map(external)");
        }
    }
    // The verdict citing quarantined items inherits the taint; Π admits
    // it raise-only.
    let cd = doc(CRITIC);
    let Json::Arr(tcells) = cd.get("taint_cells").unwrap() else {
        panic!()
    };
    for c in tcells {
        let Json::Arr(items) = c.get("bundle_items").unwrap() else {
            panic!()
        };
        let bundle = bundle_of(items.iter().map(item_of).collect());
        let Json::Arr(cited) = c.get("cited").unwrap() else {
            panic!()
        };
        let mut v = critic_verdict("v-taint");
        v.findings = cited
            .iter()
            .filter_map(Json::as_str)
            .map(|r| Finding {
                code: "f".into(),
                severity: hharness_verification::vocab::SeverityLevel::Low,
                location: None,
                message: "cites".into(),
                evidence_ref: Some(r.into()),
            })
            .collect();
        let taint = verdict_taint(&v, &bundle);
        let want: BTreeSet<String> = match c.get("expect_taint") {
            Some(Json::Arr(ts)) => ts
                .iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect(),
            _ => BTreeSet::new(),
        };
        assert_eq!(taint, want, "{}", s(c, "name"));
        let want_pi = match s(c, "expect_pi") {
            "raise_only" => PiAdmission::RaiseOnly,
            _ => PiAdmission::Full,
        };
        assert_eq!(pi_admission(&taint), want_pi);
    }
}

// AC-R-2.7.3-12 — judged classes render `n/a{observability}` for a
#[test]
fn ac_r_2_7_3_12_judged_classes_na_by_observability() {
    let d = doc(HOSTED);
    for name in [
        "session_abi_end_state_deterministic",
        "session_abi_end_state_judged",
        "interception_rows_populate",
    ] {
        let c = cell(&d, "na_cells", name);
        let Json::Arr(av) = c.get("available").unwrap() else {
            panic!()
        };
        let available: BTreeSet<SurfaceObs> = av
            .iter()
            .filter_map(Json::as_str)
            .map(|a| match a {
                "events" => SurfaceObs::Events,
                "ledger" => SurfaceObs::Ledger,
                "end_state" => SurfaceObs::EndState,
                "model_io" => SurfaceObs::ModelIo,
                other => panic!("obs {other}"),
            })
            .collect();
        let detector = match s(c, "detector") {
            "judged" => Detector::Judged,
            _ => Detector::Deterministic,
        };
        let mut got: Vec<String> = na_classes(&available, detector)
            .iter()
            .map(|c| c.as_str().to_string())
            .collect();
        let Json::Arr(want) = c.get("expect").unwrap() else {
            panic!()
        };
        let mut want: Vec<String> = want
            .iter()
            .filter_map(|e| e.as_str().map(str::to_string))
            .collect();
        got.sort();
        want.sort();
        assert_eq!(got, want, "{}", s(c, "name"));
    }
}

// ── fixture builders (validator verdicts) ────────────────────────────────────

fn verdict(id: &str) -> Verdict {
    Verdict {
        verdict_id: id.into(),
        validator_ref: vref("val:1"),
        oracle_class: OracleClass::Executable,
        target: "task:1".into(),
        criterion_ref: Some("crit:1".into()),
        contract_id: Some("contract:1".into()),
        phase: VerdictPhase::Completion,
        role: CriterionRole::Acceptance,
        value: VerdictValue::Bool(true),
        status: VerdictStatus::Decided,
        detector: Detector::Deterministic,
        evidence_refs: vec!["ev:1".into()],
        inputs_digest: "sha256:inputs".into(),
        evidence_head_seq: 9,
        freshness_ok: true,
        findings: vec![],
        cost_ppm: 1_000,
        charged_to: ChargedTo::Instrument,
        veto_tripped: vec![],
        bundle_id: None,
        calibration_ref: None,
        independence_summary: None,
        uncited_findings: 0,
        provenance: prov_kernel(),
        measured_at: 9,
    }
}
