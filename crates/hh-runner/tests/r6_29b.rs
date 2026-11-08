//! R6.29 Stage-2 acceptance battery (manifest 283; FP holds):
//! AC-6 (REQ-XH-123) — a trigger-record fixture yields a hosted-only
//! W4 schedule and both typed withholdings, the produced record citing
//! the trigger; a both-partition G-L1 failure does the same; an
//! outcome field never decides. AC-7 (REQ-XH-124) — the ≤ $0.05 lane
//! probe plan/driver over the recorded transport, probe receipts
//! preceding the first HH-native slot; a failed probe or ≥ 1
//! infra-failed slot among the first 3 holds the arm while the hosted
//! arms continue. AC-8 (REQ-XH-127) — a held Pi absent from the FP-E
//! schedule with its family member retained unavailable; a held
//! original mini-swe-agent yields the FP-E scope-decision record with
//! the typed withholdings; an empty-cell exclusion draws from the
//! adjacent cell with the disclosure flag; a hold fixture reading a
//! task verdict refuses. USD $0 — fixture-verified; the
//! `w4-lane-probe` leg is registered, never run.

use std::collections::{BTreeMap, BTreeSet};

use hharness_budget::pricing::ollama_glm_5_3_flash;
use hharness_budget::ModelRef;
use hharness_runner::continuation::{check_cost_only, first_pass_family, ContinuationError, FpArm};
use hharness_runner::disposition::WithheldReason;
use hharness_runner::holds::{
    fallback_decision, fallback_record, fp_e_arms, hosted_only_arms, lane_probe_bound,
    lane_probe_plan, native_lane_hold, native_lane_hold_record, pp0_disposition, pp0_hold,
    probe_precedes_native, replace_empty_cell, run_lane_probe, scope_decision_record,
    FallbackCause, Gl1Status, PartitionResult, Pp0HoldKind, Pp0TrialEvidence, TriggerRecord,
    FALLBACK_SCHEMA, LANE_PROBE_CAP_MICROS, LANE_PROBE_SCHEMA, MS_PER_DAY, SCOPE_DECISION_SCHEMA,
};
use hharness_runner::json_ext::JsonExt;
use hharness_runner::ps0::{ProbeOutcome, ProbeTransport, Ps0Error, SlotGrant, UsageView};
use hharness_wire::json::Json;

fn model_ref() -> ModelRef {
    ModelRef {
        profile_ref: "glm-5.3-flash".to_string(),
        provider_model_id: "glm-5.3-flash".to_string(),
        serving_route: "ollama".to_string(),
        effort: None,
    }
}

fn slot() -> SlotGrant {
    SlotGrant {
        token_ref: "slot-token-ref:lane-probe".to_string(),
        slice_micros: 10_000,
    }
}

/// A scripted transport — asserts the slot token rides every call.
struct Scripted {
    outcomes: Vec<ProbeOutcome>,
    seen: Vec<(String, String)>,
}
impl ProbeTransport for Scripted {
    fn call(
        &mut self,
        slot: &SlotGrant,
        call: &hharness_runner::ps0::ProbeCall,
    ) -> Result<ProbeOutcome, Ps0Error> {
        self.seen
            .push((call.probe_id.clone(), slot.token_ref.clone()));
        if self.outcomes.is_empty() {
            return Err(Ps0Error {
                detail: "script exhausted".to_string(),
            });
        }
        Ok(self.outcomes.remove(0))
    }
}

fn pass_outcome(cost: i64) -> ProbeOutcome {
    ProbeOutcome {
        disposition: "pass".to_string(),
        receipt_ref: "receipt:lane-probe".to_string(),
        usage: Some(UsageView {
            input_uncached: 96,
            cache_read: 0,
            output: 4,
        }),
        cost_micros: cost,
    }
}

fn gl1(a: PartitionResult, b: PartitionResult, pass_id: Option<&str>) -> Gl1Status {
    Gl1Status {
        gate_a: a,
        gate_b: b,
        pass_id: pass_id.map(str::to_string),
    }
}

// ── AC-6 (REQ-XH-123) ────────────────────────────────────────────────

#[test]
fn ac6_trigger_record_yields_hosted_only_schedule() {
    // G-L1 has not passed and is not failed on both — the recorded
    // trigger fired (Q-X62's record, 14 days after otherwise-ready).
    let gl1 = gl1(PartitionResult::Pending, PartitionResult::Fail, None);
    let trigger = TriggerRecord {
        record_ref: "fp-s:qx62-trigger".to_string(),
        fps_otherwise_ready_ms: 1_000_000,
        trigger_after_days: 14,
    };
    assert!(!trigger.fired(1_000_000 + 13 * MS_PER_DAY));
    let now = 1_000_000 + 14 * MS_PER_DAY;
    assert!(trigger.fired(now));
    let d = fallback_decision(&gl1, Some(&trigger), now);
    assert!(d.hosted_only);
    assert_eq!(
        d.cause,
        Some(FallbackCause::TriggerRecord {
            record_ref: "fp-s:qx62-trigger".to_string()
        })
    );
    let rec = fallback_record(&d);
    assert_eq!(
        rec.get("schema").and_then(Json::as_str),
        Some(FALLBACK_SCHEMA)
    );
    // The produced FP-E input cites the trigger.
    assert_eq!(
        rec.get("trigger_ref").and_then(Json::as_str),
        Some("fp-s:qx62-trigger")
    );
    // The hosted-only W4 schedule: the three hosted arms alone.
    let arms: Vec<&str> = rec
        .get("arms")
        .and_then(Json::as_arr)
        .unwrap()
        .iter()
        .filter_map(Json::as_str)
        .collect();
    assert_eq!(arms, vec!["mini-swe-agent", "pi", "claude-code"]);
    assert_eq!(
        hosted_only_arms(),
        vec![FpArm::OrigMini, FpArm::Pi, FpArm::ClaudeCode]
    );
    // Both typed withholdings: P3-L2 and P4-FP withheld{l1_not_passed}.
    let claims = rec.get("withheld_claims").and_then(Json::as_arr).unwrap();
    assert_eq!(claims.len(), 2);
    let names: Vec<&str> = claims
        .iter()
        .filter_map(|c| c.get("claim").and_then(Json::as_str))
        .collect();
    assert_eq!(names, vec!["P3-L2", "P4-FP"]);
    assert!(claims
        .iter()
        .all(|c| c.get("disposition").and_then(Json::as_str) == Some("withheld{l1_not_passed}")));
    check_cost_only(&rec).unwrap();
}

#[test]
fn ac6_both_partition_failure_and_decision_rules() {
    // Both partitions failed → hosted-only, cause g_l1_both_partitions.
    let d = fallback_decision(
        &gl1(PartitionResult::Fail, PartitionResult::Fail, None),
        None,
        0,
    );
    assert!(d.hosted_only);
    assert_eq!(d.cause, Some(FallbackCause::Gl1BothPartitions));
    // One partition passing is not a both-partition failure; without a
    // fired trigger FP-E is not hosted-only.
    let d = fallback_decision(
        &gl1(PartitionResult::Pass, PartitionResult::Fail, None),
        None,
        i64::MAX,
    );
    assert!(!d.hosted_only);
    // G-L1 passed → never hosted-only, even past the trigger.
    let trigger = TriggerRecord {
        record_ref: "t".to_string(),
        fps_otherwise_ready_ms: 0,
        trigger_after_days: 14,
    };
    let d = fallback_decision(
        &gl1(
            PartitionResult::Pass,
            PartitionResult::Pass,
            Some("l1-pass:sha256:abc"),
        ),
        Some(&trigger),
        i64::MAX,
    );
    assert!(!d.hosted_only);
    // The fallback is decided from records, never an outcome field.
    assert!(TriggerRecord::from_json(&Json::obj([
        ("record_ref", Json::str("t")),
        ("fps_otherwise_ready_ms", Json::Int(0)),
        ("trigger_after_days", Json::Int(14)),
        ("verdict", Json::str("resolved")),
    ]))
    .is_err());
    assert!(Gl1Status::from_json(&Json::obj([
        ("gate_a", Json::str("pass")),
        ("gate_b", Json::str("pass")),
        ("pass_id", Json::str("l1-pass:sha256:x")),
    ]))
    .unwrap()
    .passed());
}

// ── AC-7 (REQ-XH-124) ────────────────────────────────────────────────

#[test]
fn ac7_lane_probe_plan_bound_and_driver() {
    // One call per pinned effort path on the declared surface's route.
    let calls = lane_probe_plan("glm-5.3-flash", "openai.chat", &["high"]).unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].probe_id, "lane-probe-high");
    assert_eq!(calls[0].route, "\u{2f}v1\u{2f}chat\u{2f}completions");
    // Several effort paths price independently; the bound refuses
    // typed before any call when over $0.05.
    let calls2 = lane_probe_plan("glm-5.3-flash", "openai.chat", &["high", "low"]).unwrap();
    assert_eq!(calls2.len(), 2);
    assert!(lane_probe_bound(&calls2, &model_ref(), &ollama_glm_5_3_flash()).is_ok());
    assert!(lane_probe_plan("m", "bogus.surface", &["high"]).is_err());
    assert!(lane_probe_plan("m", "openai.chat", &[]).is_err());
    // The driver: planned calls only, each receipted on the slot
    // token, hh-lane-probe/1 with per-call dispositions.
    let mut t = Scripted {
        outcomes: vec![pass_outcome(1)],
        seen: Vec::new(),
    };
    let stage = run_lane_probe(
        &calls,
        &slot(),
        &mut t,
        &model_ref(),
        &ollama_glm_5_3_flash(),
        "now",
    )
    .unwrap();
    assert_eq!(stage.disposition, "pass");
    assert_eq!(
        t.seen,
        vec![(
            "lane-probe-high".to_string(),
            "slot-token-ref:lane-probe".to_string()
        )]
    );
    let j = stage.to_json();
    assert_eq!(
        j.get("schema").and_then(Json::as_str),
        Some(LANE_PROBE_SCHEMA)
    );
    assert_eq!(
        j.get("precedes").and_then(Json::as_str),
        Some("first_hh_native_slot")
    );
    check_cost_only(&j).unwrap();
    assert_eq!(LANE_PROBE_CAP_MICROS, 50_000);
    // A failed probe call fails the stage — never silently.
    let mut t2 = Scripted {
        outcomes: vec![ProbeOutcome {
            disposition: "failed".to_string(),
            receipt_ref: "r".to_string(),
            usage: None,
            cost_micros: 1,
        }],
        seen: Vec::new(),
    };
    let stage2 = run_lane_probe(
        &calls,
        &slot(),
        &mut t2,
        &model_ref(),
        &ollama_glm_5_3_flash(),
        "now",
    )
    .unwrap();
    assert_eq!(stage2.disposition, "failed");
}

#[test]
fn ac7_probe_ordering_and_native_lane_hold() {
    // The probe's receipts precede the first HH-native slot.
    assert!(probe_precedes_native(2, 3));
    assert!(!probe_precedes_native(3, 2));
    let remaining: Vec<String> = (0..50).map(|i| format!("hh_native-s{i}")).collect();
    // A failed probe holds the arm: withheld{admission:native_lane_hold}.
    let a = native_lane_hold(
        FpArm::HhNative,
        false,
        &[false, false, false],
        remaining.clone(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        a.reason,
        WithheldReason::Admission(hharness_runner::disposition::AdmissionReason::NativeLaneHold)
    );
    assert_eq!(a.slots.len(), 50);
    // ≥ 1 infra-failed slot among the arm's first 3 also holds it —
    // on a passing probe.
    assert!(
        native_lane_hold(FpArm::V5, true, &[false, true, false], remaining.clone())
            .unwrap()
            .is_some()
    );
    // A passing probe and a clean first-3 leave the arm running.
    assert!(
        native_lane_hold(FpArm::HhNative, true, &[false, false, false], remaining)
            .unwrap()
            .is_none()
    );
    // A hosted arm is not a native-lane hold candidate — typed refusal.
    assert!(native_lane_hold(FpArm::Pi, false, &[true], Vec::new()).is_err());
    // The hold record: hosted arms continue; repair = new H → new G-L1.
    let rec = native_lane_hold_record(FpArm::HhNative, &a, "probe_failed");
    assert_eq!(
        rec.get("disposition").and_then(Json::as_str),
        Some("withheld{admission:native_lane_hold}")
    );
    assert_eq!(
        rec.get("repair").and_then(Json::as_str),
        Some("new_h_version_requires_new_g_l1")
    );
    assert_eq!(
        rec.get("hosted_arms").and_then(Json::as_str),
        Some("continue")
    );
}

/// The `w4-lane-probe` leg — registered by this row, executed in W4
/// under FP-E by R6.47 (ADR-0363; REQ-XH-027 contributing).
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh w4-lane-probe — the HH-native lane probe runs only inside W4 under FP-E (R6.47): one receipted call per pinned effort path through the proxy's loopback listener, ≤ $0.05 worst-case"]
fn live_w4_lane_probe() {}

// ── AC-8 (REQ-XH-127) ────────────────────────────────────────────────

fn pp0_trial(infra: bool, ag: &[(&str, &str)]) -> Pp0TrialEvidence {
    Pp0TrialEvidence {
        trial_ref: "pp0-t".to_string(),
        infra_failed: infra,
        ag: ag
            .iter()
            .map(|(g, r)| (g.to_string(), r.to_string()))
            .collect(),
    }
}

#[test]
fn ac8_held_pi_absent_but_family_retained_unavailable() {
    let trials = vec![
        pp0_trial(
            false,
            &[
                ("AG3", "pass"),
                ("AG5", "fail"),
                ("AG6", "pass"),
                ("AG8", "pass"),
            ],
        ),
        pp0_trial(
            false,
            &[
                ("AG3", "pass"),
                ("AG5", "pass"),
                ("AG6", "pass"),
                ("AG8", "pass"),
            ],
        ),
        pp0_trial(
            false,
            &[
                ("AG3", "pass"),
                ("AG5", "pass"),
                ("AG6", "pass"),
                ("AG8", "pass"),
            ],
        ),
    ];
    let hold = pp0_hold(&trials).unwrap();
    assert_eq!(hold, Pp0HoldKind::Gate(5));
    let d = pp0_disposition(FpArm::Pi, &hold).unwrap();
    assert_eq!(
        d.get("disposition").and_then(Json::as_str),
        Some("withheld{admission:AG5}")
    );
    // The arm is absent from the FP-E schedule; its family member is
    // retained as unavailable.
    let arms = fp_e_arms(&[FpArm::Pi]);
    assert!(!arms.contains(&FpArm::Pi));
    assert!(arms.contains(&FpArm::ClaudeCode));
    let fam = first_pass_family(&BTreeMap::from([(FpArm::Pi, hold.withheld())]));
    let pi = fam.iter().find(|m| m.arm == FpArm::Pi).unwrap();
    assert!(!pi.available);
    assert_eq!(pi.withheld.as_deref(), Some("withheld{admission:AG5}"));
    // An infra hold with no failed gate takes pp0_infra_hold.
    let infra_hold = pp0_hold(&[pp0_trial(
        true,
        &[
            ("AG3", "pass"),
            ("AG5", "pass"),
            ("AG6", "pass"),
            ("AG8", "pass"),
        ],
    )])
    .unwrap();
    assert_eq!(infra_hold, Pp0HoldKind::InfraNoGate);
    let d = pp0_disposition(FpArm::ClaudeCode, &infra_hold).unwrap();
    assert_eq!(
        d.get("disposition").and_then(Json::as_str),
        Some("withheld{admission:pp0_infra_hold}")
    );
    // No hold on a clean PP0.
    assert!(pp0_hold(&[pp0_trial(
        false,
        &[
            ("AG3", "pass"),
            ("AG5", "pass"),
            ("AG6", "pass"),
            ("AG8", "pass")
        ],
    )])
    .is_none());
    // Native-lane arms are not PP0 arms — typed refusal.
    assert!(pp0_disposition(FpArm::V5, &infra_hold).is_err());
}

#[test]
fn ac8_held_mini_yields_the_fp_e_scope_decision() {
    let hold = Pp0HoldKind::InfraNoGate;
    let rec = scope_decision_record(FpArm::OrigMini, &hold).unwrap();
    assert_eq!(
        rec.get("schema").and_then(Json::as_str),
        Some(SCOPE_DECISION_SCHEMA)
    );
    assert_eq!(
        rec.get("default").and_then(Json::as_str),
        Some("end_pass_after_w2")
    );
    assert_eq!(rec.get("g_l1").and_then(Json::as_str), Some("cannot_pass"));
    assert_eq!(rec.get("corpus").and_then(Json::as_str), Some("withheld"));
    let claims = rec.get("withheld_claims").and_then(Json::as_arr).unwrap();
    assert_eq!(claims.len(), 4);
    assert!(claims
        .iter()
        .all(|c| c.get("disposition").and_then(Json::as_str)
            == Some("withheld{admission:pp0_infra_hold}")));
    // The disposition-(a) path refuses for the original — the scope
    // decision is its own record.
    assert!(pp0_disposition(FpArm::OrigMini, &hold).is_err());
    assert!(scope_decision_record(FpArm::Pi, &hold).is_err());
    check_cost_only(&rec).unwrap();
}

#[test]
fn ac8_empty_cell_replacement_and_verdict_refusal() {
    let orders: BTreeMap<String, Vec<String>> = BTreeMap::from([
        ("easy".to_string(), vec!["e1".to_string()]),
        (
            "medium".to_string(),
            vec!["m1".to_string(), "m2".to_string()],
        ),
        ("hard".to_string(), vec!["h1".to_string()]),
    ]);
    let drawn: BTreeSet<String> = BTreeSet::from(["m1".to_string()]);
    let r = replace_empty_cell("easy", &orders, &drawn).unwrap();
    // The adjacent cell's committed order, skipping the drawn task.
    assert_eq!(r.get("replaced_task").and_then(Json::as_str), Some("m2"));
    assert_eq!(r.get("from_cell").and_then(Json::as_str), Some("medium"));
    assert_eq!(r.get("disclosure").and_then(Json::as_bool), Some(true));
    // Hard draws from medium too; medium has no declared donor.
    assert!(replace_empty_cell("hard", &orders, &drawn).is_ok());
    assert!(replace_empty_cell("medium", &orders, &drawn).is_err());
    let drawn_all: BTreeSet<String> = BTreeSet::from(["m1".to_string(), "m2".to_string()]);
    assert!(replace_empty_cell("easy", &orders, &drawn_all).is_err());
    // A hold fixture that reads a task verdict refuses typed.
    assert!(matches!(
        Pp0TrialEvidence::from_json(&Json::obj([
            ("trial_ref", Json::str("t")),
            ("infra_failed", Json::Bool(false)),
            ("verdict", Json::str("resolved")),
        ])),
        Err(ContinuationError::OutcomeField { .. })
    ));
    // The strict decode also refuses undeclared members and non-gate
    // checks — the evidence set is AG3/AG5/AG6/AG8 plus infra only.
    assert!(Pp0TrialEvidence::from_json(&Json::obj([
        ("trial_ref", Json::str("t")),
        ("extra", Json::Int(1)),
    ]))
    .is_err());
    assert!(Pp0TrialEvidence::from_json(&Json::obj([
        ("trial_ref", Json::str("t")),
        ("ag", Json::obj([("AG2", Json::str("fail"))])),
    ]))
    .is_err());
}
