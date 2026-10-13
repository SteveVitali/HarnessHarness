//! R2.6 — the staged-interpreter conformance leg:
//!
//! - **`variant_for`** — the one binding table (`hh/react-minimal`,
//!   `hh/react-steerable`, `hh/react-steerable-queue`, `hh/plan-execute`,
//!   `hh/workflow`, `hh/program`); an unknown ref is `None` — the caller
//!   answers a typed refusal, never a silent `react/minimal`.
//! - **`react/steerable`'s durable-steer arm** — `woken{delivery_mode:
//!   steer, payload_ref}` (the `steer{mode: next_turn}` op's durable
//!   queue delivered through the wakeup seam — DF-S2.11-1) proposes under
//!   `{steer_ref}`, joining the in-turn `human_input{steer}`
//!   interpretation; `woken{follow_up}` joins `human_input{follow_up}`.
//!   Under `steer_mode = unsupported` a forged woken-steer falls through
//!   to the shared table (never a steer interpretation the declaration
//!   did not admit).
//! - **stop protocol on the staged variants** — `hh/workflow` /
//!   `hh/program` park every cue on `human_input`, but `interrupt` /
//!   `cancel_requested` still answer `stop{cancelled}` — a parked run
//!   stays cancellable, and the decision is code-owned, not improvised.
//! - **`plan_execute`'s explicit legs** — `interrupt` / `cancel_requested`
//!   are typed `stop{cancelled}` answers mid-plan; a stray `woken{steer}`
//!   parks rather than improvising a re-plan.
//! - **`wire` at the boundary** — a `decide` output round-trips through
//!   `decision_to_json`/`decision_from_json` byte-for-byte (the OOP
//!   conformance lane asserts on the canonical record, AC-1's document
//!   leg), and malformed/unknown members refuse to decode.

use std::collections::BTreeMap;

use hh_control::react::{self, ReactSteerable, REACT_STEERABLE_REF};
use hh_control::state::{ControlState, PlanCursor};
use hh_control::strategy::{
    ConcurrentInput, ControlContext, ControlStrategy, SteerMode, StrategyParams,
};
use hh_control::vocab::{
    ControlDecision, Cue, DecisionKind, DeliveryMode, EnvelopeSignal, HumanInput, WokenTrigger,
};
use hh_control::wire;
use hh_ontology::control::{CancelledBy, DecisionPoint, Owner, StopReason};
use hh_wire::json::Json;

// ── fixtures ────────────────────────────────────────────────────────────

fn ctx_with_steering(steering: (SteerMode, ConcurrentInput)) -> ControlContext {
    ControlContext {
        process_ref: "proc-1".into(),
        plan: vec![],
        boundary: react::react_preset(),
        profile: Json::Null,
        account_ref: "acct".into(),
        budget_ref: "b-1".into(),
        envelope_ref: "env-1".into(),
        parameters: StrategyParams::default(),
        capabilities_available: vec!["hh.submit".into()],
        steering,
    }
}

fn steerable_ctx(mode: SteerMode) -> ControlContext {
    ctx_with_steering((mode, ConcurrentInput::Steer))
}

fn woken_steer(payload_ref: &str) -> Cue {
    Cue::Woken {
        trigger: WokenTrigger::Manual {
            principal: "principal:p".into(),
        },
        payload_ref: payload_ref.into(),
        delivery_mode: DeliveryMode::Steer,
    }
}

fn woken_follow_up(payload_ref: &str) -> Cue {
    Cue::Woken {
        trigger: WokenTrigger::Manual {
            principal: "principal:p".into(),
        },
        payload_ref: payload_ref.into(),
        delivery_mode: DeliveryMode::FollowUp,
    }
}

fn propose_ref(d: &ControlDecision) -> Option<String> {
    match &d.kind {
        DecisionKind::Propose {
            context_request, ..
        } => context_request
            .get("steer_ref")
            .or_else(|| context_request.get("follow_up_ref"))
            .and_then(Json::as_str)
            .map(str::to_string),
        _ => None,
    }
}

fn is_stop_cancelled(d: &ControlDecision, by: CancelledBy) -> bool {
    matches!(
        &d.kind,
        DecisionKind::Stop {
            proposed_reason: StopReason::Cancelled { by: b },
            ..
        } if *b == by
    )
}

/// A `ControlState` standing in for a staged variant's opened state —
/// `open` requires a `plan` (the procedure), which these `decide`-level
/// cells do not exercise (the parked/stop arms read no plan).
fn staged_state(variant_ref: &str) -> ControlState {
    ControlState {
        variant_ref: variant_ref.into(),
        cursor: PlanCursor {
            node_id: "n0".into(),
            iteration: 0,
            bound_ref: "b-1".into(),
        },
        decision_count: 0,
        open_effects: vec![],
        last_cue_seq: 0,
        boundary_view: DecisionPoint::ALL
            .iter()
            .map(|p| (*p, Owner::Code))
            .collect::<BTreeMap<_, _>>(),
        extension: Json::obj([]),
    }
}

// ── variant_for — the one binding table ─────────────────────────────────

#[test]
fn variant_for_binds_every_registered_ref() {
    for r in [
        "hh/react-minimal",
        "hh/react-minimal@1",
        "hh/react-steerable",
        "hh/react-steerable@1",
        "hh/react-steerable-queue",
        "hh/react-steerable-queue@1",
        "hh/plan-execute",
        "hh/plan-execute@1",
        "hh/workflow",
        "hh/workflow@1",
        "hh/program",
        "hh/program@1",
    ] {
        assert!(react::variant_for(r).is_some(), "{r} did not resolve");
    }
}

#[test]
fn variant_for_unknown_is_none_never_a_substitute() {
    for r in [
        "hh/no-such-variant",
        "hh/react-steerable@99",
        "local/react-minimal",
        "",
    ] {
        assert!(react::variant_for(r).is_none(), "{r} must not resolve");
    }
}

#[test]
fn workflow_and_program_bind_staged_not_minimal() {
    // The staged presets are *not* ReactMinimal in disguise — a run bound
    // to `hh/workflow` must never silently act like `react/minimal`.
    let w = react::variant_for("hh/workflow@1").unwrap();
    let p = react::variant_for("hh/program@1").unwrap();
    assert!(!w.capabilities().steering, "workflow is not steerable");
    assert!(!p.capabilities().steering, "program is not steerable");
    assert!(w.capabilities().requires.procedure);
    assert!(p.capabilities().requires.procedure);
}

// ── react/steerable — the durable steer arm ─────────────────────────────

#[test]
fn steerable_woken_steer_proposes_steer_ref() {
    // `queue_next_turn` — the declaration the durable seam was built for.
    let mut v = ReactSteerable::new();
    let mut st = v.open(&steerable_ctx(SteerMode::QueueNextTurn)).unwrap();
    let d = v.decide(&mut st, &woken_steer("sha256:queued-1"));
    assert_eq!(
        propose_ref(&d).as_deref(),
        Some("sha256:queued-1"),
        "woken{{steer}} must propose under steer_ref: {d:?}"
    );
    let cr = match &d.kind {
        DecisionKind::Propose {
            context_request, ..
        } => context_request,
        other => panic!("expected propose, got {other:?}"),
    };
    assert_eq!(
        cr.get("steer_ref").and_then(Json::as_str),
        Some("sha256:queued-1")
    );
}

#[test]
fn steerable_woken_steer_interrupt_mode_too() {
    // `interrupt_at_decision_point` steers in-turn; a *queued* woken-steer
    // landing under the same interpreter still reads as a steer (one
    // interpretation — the cue names the ref either way).
    let mut v = ReactSteerable::new();
    let mut st = v
        .open(&steerable_ctx(SteerMode::InterruptAtDecisionPoint))
        .unwrap();
    let d = v.decide(&mut st, &woken_steer("sha256:mid-turn"));
    assert_eq!(propose_ref(&d).as_deref(), Some("sha256:mid-turn"));
}

#[test]
fn steerable_woken_follow_up_proposes_follow_up_ref() {
    let mut v = ReactSteerable::new();
    let mut st = v
        .open(&steerable_ctx(SteerMode::InterruptAtDecisionPoint))
        .unwrap();
    let d = v.decide(&mut st, &woken_follow_up("sha256:fu-1"));
    match &d.kind {
        DecisionKind::Propose {
            context_request, ..
        } => assert_eq!(
            context_request.get("follow_up_ref").and_then(Json::as_str),
            Some("sha256:fu-1")
        ),
        other => panic!("expected propose, got {other:?}"),
    }
}

#[test]
fn steerable_woken_steer_needs_declared_steering() {
    // `steer_mode = unsupported` — a woken-steer can only arrive forged
    // (the ledger never mints one for a run whose declaration refuses
    // steering; the boundary op refuses `steer` upstream). The arm does
    // not interpret it as a steer — the shared table answers.
    let mut v = ReactSteerable::new();
    let mut st = v.open(&steerable_ctx(SteerMode::Unsupported)).unwrap();
    let d = v.decide(&mut st, &woken_steer("sha256:forged"));
    // Any non-steer answer is honest — just never `steer_ref`.
    if let DecisionKind::Propose {
        context_request, ..
    } = &d.kind
    {
        assert!(
            context_request.get("steer_ref").is_none(),
            "an unsupported steering declaration must not interpret a steer: {d:?}"
        );
    }
}

#[test]
fn steerable_woken_steer_needs_the_ref() {
    // A woken-steer with no `payload_ref` has nothing to plan against —
    // it falls through to the shared table (no fabricated ref).
    let mut v = ReactSteerable::new();
    let mut st = v.open(&steerable_ctx(SteerMode::QueueNextTurn)).unwrap();
    let d = v.decide(&mut st, &woken_steer(""));
    if let DecisionKind::Propose {
        context_request, ..
    } = &d.kind
    {
        assert!(context_request.get("steer_ref").is_none());
    }
}

#[test]
fn steerable_woken_steer_records_the_plan_point() {
    // The steer interpretation is a `plan`-point decision owned per the
    // declared boundary — the durable cue did not change ownership.
    let mut v = ReactSteerable::new();
    let mut st = v.open(&steerable_ctx(SteerMode::QueueNextTurn)).unwrap();
    let d = v.decide(&mut st, &woken_steer("sha256:x"));
    assert_eq!(d.stamp.decision_point, DecisionPoint::Plan);
    assert_eq!(st.variant_ref, REACT_STEERABLE_REF);
}

// ── staged variants — parked, still cancellable ─────────────────────────

#[test]
fn staged_variant_parks_non_protocol_cues() {
    let v = react::variant_for("hh/workflow@1").unwrap();
    let mut st = staged_state("hh/workflow@1");
    let d = v.decide(
        &mut st,
        &Cue::RunOpened {
            goal_ref: "g".into(),
            inputs: Json::Null,
        },
    );
    match &d.kind {
        DecisionKind::Wait {
            until: hh_control::vocab::WaitUntil::CueKind { cue_kind },
        } => assert_eq!(cue_kind, "human_input"),
        other => panic!("staged variant must park, got {other:?}"),
    }
}

#[test]
fn staged_variant_interrupt_is_a_typed_stop() {
    for r in ["hh/workflow@1", "hh/program@1"] {
        let v = react::variant_for(r).unwrap();
        let mut st = staged_state(r);
        let d = v.decide(&mut st, &Cue::HumanInput(HumanInput::Interrupt));
        assert!(
            is_stop_cancelled(&d, CancelledBy::Principal),
            "{r}: interrupt must answer stop{{cancelled{{principal}}}}: {:?}",
            d.kind
        );
        assert_eq!(d.stamp.decision_point, DecisionPoint::Stop);
    }
}

#[test]
fn staged_variant_cancel_requested_is_a_typed_stop() {
    let v = react::variant_for("hh/program@1").unwrap();
    let mut st = staged_state("hh/program@1");
    let d = v.decide(
        &mut st,
        &Cue::EnvelopeSignal(EnvelopeSignal::CancelRequested {
            by: "principal".into(),
        }),
    );
    assert!(is_stop_cancelled(&d, CancelledBy::Principal), "{d:?}");
}

#[test]
fn staged_variant_open_without_procedure_is_missing_procedure() {
    // `requires.procedure` is real — a staged preset with no plan refuses
    // `MissingProcedure`, not a fabricated state.
    let mut v = react::variant_for("hh/workflow@1").unwrap();
    let c = ctx_with_steering((SteerMode::Unsupported, ConcurrentInput::QueueOnly));
    let err = v.open(&c).unwrap_err();
    assert!(
        matches!(err, hh_control::strategy::ControlError::MissingProcedure),
        "{err:?}"
    );
}

// ── plan_execute — explicit stop-protocol legs ──────────────────────────

#[test]
fn plan_execute_interrupt_is_a_typed_stop() {
    let mut v = hh_control::plan_exec::PlanExecute::new();
    let mut c = ctx_with_steering((SteerMode::Unsupported, ConcurrentInput::QueueOnly));
    c.boundary = v.capabilities().boundary_preset.clone();
    let mut st = v.open(&c).unwrap();
    let d = v.decide(&mut st, &Cue::HumanInput(HumanInput::Interrupt));
    assert!(
        is_stop_cancelled(&d, CancelledBy::Principal),
        "plan_execute interrupt must be stop{{cancelled{{principal}}}}: {:?}",
        d.kind
    );
}

#[test]
fn plan_execute_cancel_requested_is_a_typed_stop() {
    let mut v = hh_control::plan_exec::PlanExecute::new();
    let mut c = ctx_with_steering((SteerMode::Unsupported, ConcurrentInput::QueueOnly));
    c.boundary = v.capabilities().boundary_preset.clone();
    let mut st = v.open(&c).unwrap();
    let d = v.decide(
        &mut st,
        &Cue::EnvelopeSignal(EnvelopeSignal::CancelRequested {
            by: "operator".into(),
        }),
    );
    assert!(
        matches!(
            &d.kind,
            DecisionKind::Stop {
                proposed_reason: StopReason::Cancelled { .. },
                ..
            }
        ),
        "{d:?}"
    );
}

#[test]
fn plan_execute_stray_woken_steer_parks_not_replans() {
    // `plan_execute` declares no steering — a woken{steer} that still
    // arrives (replayed under a stale arm) parks; the plan owns the step
    // order, so nothing improvises a re-plan.
    let mut v = hh_control::plan_exec::PlanExecute::new();
    let mut c = ctx_with_steering((SteerMode::Unsupported, ConcurrentInput::QueueOnly));
    c.boundary = v.capabilities().boundary_preset.clone();
    let mut st = v.open(&c).unwrap();
    let d = v.decide(&mut st, &woken_steer("sha256:stray"));
    assert!(
        matches!(d.kind, DecisionKind::Wait { .. }),
        "stray woken-steer must park: {:?}",
        d.kind
    );
}

// ── wire — the boundary documents ───────────────────────────────────────

#[test]
fn decide_outputs_round_trip_through_the_canonical_codec() {
    // The OOP lane's assertion is over canonical documents — a `decide`
    // answer encodes and decodes to itself (AC-1's document leg).
    let mut v = ReactSteerable::new();
    let mut st = v.open(&steerable_ctx(SteerMode::QueueNextTurn)).unwrap();
    for cue in [
        woken_steer("sha256:queued"),
        Cue::HumanInput(HumanInput::Steer {
            payload_ref: "sha256:in-turn".into(),
        }),
        Cue::HumanInput(HumanInput::Interrupt),
    ] {
        let d = v.decide(&mut st, &cue);
        let j = wire::decision_to_json(&d);
        let back = wire::decision_from_json(&j)
            .unwrap_or_else(|| panic!("decision did not decode: {}", j.to_canonical_string()));
        assert_eq!(back, d, "decision did not round-trip");
    }
}

#[test]
fn state_round_trips_through_checkpoint_bytes() {
    // `checkpoint`/`restore` are the restart leg — the staged steer has no
    // state to lose (the durable row is the ledger's), but the variant's
    // own checkpoint must still be exact.
    let mut v = ReactSteerable::new();
    let ctx = steerable_ctx(SteerMode::QueueNextTurn);
    let st = v.open(&ctx).unwrap();
    let bytes = v.checkpoint(&st);
    let mut v2 = ReactSteerable::new();
    let back = v2.restore(&bytes, &ctx).unwrap();
    assert_eq!(back, st);
    // And the same state survives the Json member codec.
    let j = wire::state_to_json(&st);
    let back = wire::state_from_json(&j).unwrap();
    assert_eq!(back, st);
}

#[test]
fn malformed_wire_documents_refuse_to_decode() {
    // Closed sums refuse unknown members — a forged or drifted document is
    // a `schema_violation` at the lane, never a guess.
    assert!(wire::cue_from_json(&Json::obj([("kind", Json::str("wokenish"))])).is_none());
    assert!(wire::decision_from_json(&Json::obj([("kind", Json::str("hijack"))])).is_none());
    assert!(wire::context_from_json(&Json::obj([("bogus", Json::Bool(true))])).is_none());
    assert!(wire::state_from_json(&Json::obj([("dialect", Json::str("other/1"))])).is_none());
    assert!(wire::stop_reason_from_json(&Json::str("made_up")).is_none());
}
