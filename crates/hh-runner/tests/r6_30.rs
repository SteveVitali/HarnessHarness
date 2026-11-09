//! R6.30 Stage-1 acceptance battery (manifest 284; the FP outcome
//! seal, REQ-XH-121):
//!
//! AC-1 (E; AC-10.1-12) — an outcome field in a gate readout, the
//! checkpoint readout or the dev projection fails the schema check; a
//! re-proof packet and a PP0 AG-evidence readout pass unsealed; a PP0
//! readout carrying a task verdict fails; the sealed view renders
//! `withheld{sealed_until_results}`.
//! AC-2 (E) — the dev projection has no verdict, reward or verifier
//! member (schema test); an infrastructure member stays readable by an
//! R6.29 hold fixture while a verdict read refuses.
//!
//! Fails-without: `fp_seal` does not exist at the R6.29 base — this
//! file compiles red there. Fixture-only, USD $0, no network.

use hharness_runner::continuation::check_cost_only;
use hharness_runner::disposition::WithheldReason;
use hharness_runner::fp_seal::{
    check_dev_projection, check_sealed, check_unsealed, dev_projection, seal_marker, seal_view,
    SealClass, SealError, ViewKind, DEV_PROJECTION_SCHEMA, SEALED_VIEW_SCHEMA,
};
use hharness_runner::holds::Pp0TrialEvidence;
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::Json;

fn j(pairs: &[(&str, Json)]) -> Json {
    Json::Obj(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}

fn member<'a>(v: &'a Json, k: &str) -> &'a Json {
    v.get(k).unwrap_or_else(|| panic!("member {k} absent"))
}

fn s(v: &Json, k: &str) -> String {
    member(v, k).as_str().unwrap().to_string()
}

/// A W2/W4 agent-participant slot fixture — cost fields plus a full
/// outcome set (the REQ-XH-121 sealed members).
fn agent_slot() -> Json {
    j(&[
        ("schema", Json::str("hh-experiment-slot/1")),
        (
            "identity",
            j(&[
                ("task_id", Json::str("fix-git")),
                ("arm_id", Json::str("pi")),
                ("window", Json::str("w4")),
            ]),
        ),
        (
            "disposition",
            j(&[("value", Json::str("executed{resolved}"))]),
        ),
        (
            "verdict",
            j(&[
                ("channel", Json::str("harbor_reward_file/1")),
                ("reward_ppm", Json::Int(1_000_000)),
                ("success", Json::Bool(true)),
                (
                    "ctrf",
                    j(&[("passed", Json::Int(4)), ("failed", Json::Int(0))]),
                ),
                ("verifier_output", Json::str("ok")),
            ]),
        ),
        (
            "costs",
            j(&[(
                "model",
                j(&[
                    ("schema", Json::str("hh-slot-costs/1")),
                    ("ref", Json::str("slot-costs:fp-alpha")),
                ]),
            )]),
        ),
        ("wall_ms", Json::Int(48_000)),
    ])
}

/// A PP0 AG3/AG5/AG6/AG8 evidence readout — machinery evidence:
/// exit state, receipts, egress rows, protocol errors. No verdict.
fn pp0_ag_evidence() -> Json {
    // the declared machinery members: exit state, receipts, egress
    // rows and protocol errors — never a verdict (REQ-XH-121).
    j(&[
        ("trial_ref", Json::str("pp0:fix-git:pi:1")),
        ("exit_state", Json::str("exited{0}")),
        ("receipts", Json::Int(2)),
        ("egress_denials", Json::Arr(vec![])),
        ("protocol_errors", Json::Arr(vec![])),
        ("infra_failed", Json::Bool(false)),
        (
            "ag",
            j(&[
                ("AG3", Json::str("pass")),
                ("AG5", Json::str("pass")),
                ("AG6", Json::str("pass")),
                ("AG8", Json::str("pass")),
            ]),
        ),
    ])
}

/// The R6.29 `Pp0TrialEvidence` strict shape — the hold decode
/// accepts exactly these members.
fn pp0_trial_evidence() -> Json {
    j(&[
        ("trial_ref", Json::str("pp0:fix-git:pi:1")),
        ("infra_failed", Json::Bool(false)),
        (
            "ag",
            j(&[
                ("AG3", Json::str("pass")),
                ("AG5", Json::str("pass")),
                ("AG6", Json::str("pass")),
                ("AG8", Json::str("pass")),
            ]),
        ),
    ])
}

/// A W1 re-proof packet — the declared unsealed class (admission
/// evidence): reward/verdict members are readable by declaration.
fn reproof_packet() -> Json {
    j(&[
        ("schema", Json::str("hh-reproof-packet/1")),
        ("task", Json::str("fix-git")),
        ("surface", Json::str("O")),
        ("r", Json::Int(3)),
        ("reward_ppm", Json::Int(1_000_000)),
        ("verdict", Json::str("admit")),
    ])
}

// ── AC-1: the seal in every consumer ────────────────────────────────

/// AC-1: every declared consumer masks the sealed members with
/// `withheld{sealed_until_results}` and keeps cost fields.
#[test]
fn stage1_ac1_sealed_view_masks_outcome_members() {
    for view in ViewKind::ALL {
        let sealed = seal_view(&agent_slot(), view).expect("seal");
        let m = sealed.as_obj().unwrap();
        assert_eq!(s(&sealed, "schema"), SEALED_VIEW_SCHEMA);
        assert_eq!(s(&sealed, "view"), view.as_str());
        let rec = m.get("record").unwrap();
        // the sealed marker is the closed-set member verbatim
        let marker = seal_marker();
        assert_eq!(marker, "withheld{sealed_until_results}");
        assert_eq!(
            WithheldReason::SealedUntilResults.as_str(),
            "sealed_until_results"
        );
        // outcome members masked in place — the whole `verdict`
        // member and the executed disposition value become the marker
        assert_eq!(s(rec, "verdict"), marker);
        assert_eq!(s(member(rec, "disposition"), "value"), marker);
        // cost + declared members pass through
        assert_eq!(member(rec, "wall_ms").as_int().unwrap(), 48_000);
        assert_eq!(
            s(member(member(rec, "costs"), "model"), "ref"),
            "slot-costs:fp-alpha"
        );
        assert_eq!(s(member(rec, "identity"), "task_id"), "fix-git");
    }
}

/// AC-1: an outcome field in a gate readout, the checkpoint readout or
/// the dev projection fails the schema check.
#[test]
fn stage1_ac1_outcome_field_fails_schema_check() {
    // a gate readout carrying a slot verdict refuses
    let readout = j(&[
        ("schema", Json::str("gate-readout")),
        ("gate", Json::str("FP-E")),
        ("slot", agent_slot()),
    ]);
    assert!(matches!(
        check_sealed(&readout),
        Err(SealError::OutcomeField { .. })
    ));
    // the checkpoint readout carrying an executed disposition refuses
    let checkpoint = j(&[
        ("schema", Json::str("hh-fp-checkpoint/1")),
        ("scope", Json::str("w4_round_2")),
        ("worst", j(&[("value", Json::str("executed{unresolved}"))])),
    ]);
    assert!(matches!(
        check_sealed(&checkpoint),
        Err(SealError::OutcomeField { .. })
    ));
    // the dev projection check refuses a verdict member
    let dev_bad = j(&[("record", agent_slot())]);
    assert!(matches!(
        check_dev_projection(&dev_bad),
        Err(SealError::OutcomeField { .. })
    ));
}

/// AC-1: the declared unsealed classes pass; PP0 with a task verdict
/// fails.
#[test]
fn stage1_ac1_unsealed_classes() {
    for class in [SealClass::Reproof, SealClass::Smoke, SealClass::Probe] {
        check_unsealed(&reproof_packet(), class).expect("unsealed class passes");
    }
    check_unsealed(&pp0_ag_evidence(), SealClass::Pp0AgEvidence)
        .expect("PP0 AG evidence passes unsealed");
    // a PP0 readout carrying a task verdict fails
    let bad = j(&[
        ("evidence", pp0_ag_evidence()),
        ("verdict", Json::str("pass")),
    ]);
    assert!(matches!(
        check_unsealed(&bad, SealClass::Pp0AgEvidence),
        Err(SealError::OutcomeField { .. })
    ));
    // an agent slot is the sealed class — never read raw
    assert!(check_unsealed(&agent_slot(), SealClass::AgentSlot).is_err());
}

// ── AC-2: the dev projection ─────────────────────────────────────────

/// AC-2: the dev projection has no verdict, reward or verifier member.
#[test]
fn stage1_ac2_dev_projection_strips_outcome_members() {
    let proj = dev_projection(&agent_slot());
    assert_eq!(s(&proj, "schema"), DEV_PROJECTION_SCHEMA);
    // schema check: no outcome member remains
    check_dev_projection(&proj).expect("dev projection passes its schema check");
    let rec = member(&proj, "record").as_obj().unwrap();
    assert!(!rec.contains_key("verdict"), "verdict member removed");
    // executed{…} disposition's value removed; identity + cost stay
    assert!(!rec
        .get("disposition")
        .unwrap()
        .as_obj()
        .unwrap()
        .contains_key("value"));
    assert!(rec.contains_key("costs"));
    assert!(rec.contains_key("wall_ms"));
}

/// AC-2: an infrastructure member stays readable by an R6.29 hold
/// fixture while a verdict read refuses.
#[test]
fn stage1_ac2_infra_member_readable_verdict_refuses() {
    // R6.29's PP0 evidence decode: infra member + AG fields, no verdict
    let ev = pp0_trial_evidence();
    let decoded = Pp0TrialEvidence::from_json(&ev).expect("infra member readable by the hold");
    assert!(!decoded.infra_failed);
    // a verdict member in the same record refuses (R6.29's own leg)
    let mut ev2 = ev.as_obj().unwrap().clone();
    ev2.insert("verdict".into(), Json::str("pass"));
    assert!(Pp0TrialEvidence::from_json(&Json::Obj(ev2)).is_err());
    // and the sealed view masks it
    let sealed = seal_view(&agent_slot(), ViewKind::GateReadout).expect("seal");
    check_sealed(member(&sealed, "record")).expect("sealed view passes the schema check");
    let mut m = member(&sealed, "record").as_obj().unwrap().clone();
    m.insert("score".into(), Json::Int(1));
    assert!(matches!(
        check_sealed(&Json::Obj(m)),
        Err(SealError::OutcomeField { .. })
    ));
}

/// AC-1 supplementary: inconclusive{infra:*} dispositions are infra
/// members — they stay readable, never masked.
#[test]
fn stage1_ac1_infra_disposition_unmasked() {
    let slot = j(&[
        ("schema", Json::str("hh-experiment-slot/1")),
        (
            "disposition",
            j(&[("value", Json::str("inconclusive{infra:runner_crash}"))]),
        ),
        ("wall_ms", Json::Int(48_000)),
    ]);
    let sealed = seal_view(&slot, ViewKind::OperatorConsole).expect("seal");
    assert_eq!(
        s(member(member(&sealed, "record"), "disposition"), "value"),
        "inconclusive{infra:runner_crash}"
    );
    // cost-only scan agrees: infra members are not outcome fields
    check_cost_only(&slot).expect("infra disposition is a cost-field read");
}
