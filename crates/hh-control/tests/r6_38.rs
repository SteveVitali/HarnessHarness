//! R6.38 (E2-T5; REQ-XE-115/116/117) — the `control_strategy` slot's
//! `params` decode into `StrategyParams` through the wire codec: the
//! sparse overlay admits the §5e.1 members (+ the AR-24 `submit{marker,
//! line, require_returncode}` record form), an absent member keeps the
//! variant preset, an unknown member refuses typed, and the resolved
//! record round-trips `strategy_params_to_json`/`from_json` verbatim
//! (the `leaf.arm` persistence spelling — resume parity, X-04).
//!
//! Fixture-verified — decode-level acceptance for AC-2; the dispatch-
//! gate detection and transport-retry legs live in the `hh-env`/`hh-embed`
//! batteries.

use hharness_control::strategy::{FormatFailureScope, StopRuleParam, StrategyParams, SubmitLine};
use hharness_control::wire::{
    strategy_params_from_json, strategy_params_overlay, strategy_params_to_json, StrategyParamError,
};
use hharness_wire::json::Json;

fn overlay(params: Json) -> Result<StrategyParams, StrategyParamError> {
    strategy_params_overlay(&StrategyParams::default(), &params)
}

/// A present member decodes over the preset (AC-2 — the arm decode
/// reads every declared member through the codec).
#[test]
fn overlay_decodes_declared_members() {
    let p = overlay(Json::obj([
        ("max_consecutive_format_errors", Json::Int(5)),
        ("max_continue_nudges", Json::Int(2)),
        ("max_consecutive_errors", Json::Int(4)),
        ("tool_batch_mode", Json::str("parallel")),
        ("on_truncated_response", Json::str("fail_calls")),
        ("plan_provenance", Json::str("model_emitted")),
        ("replan_on", Json::str("failure")),
        ("loop_guards", Json::Arr(vec![Json::str("g1")])),
        ("format_failure_scope", Json::str("batch")),
        ("stop_rule", Json::str("either")),
    ]))
    .unwrap();
    assert_eq!(p.max_consecutive_format_errors, 5);
    assert_eq!(p.max_continue_nudges, 2);
    assert_eq!(p.max_consecutive_errors, 4);
    assert_eq!(p.loop_guards, vec!["g1".to_string()]);
    assert_eq!(p.format_failure_scope, FormatFailureScope::Batch);
    assert!(matches!(p.stop_rule, StopRuleParam::Either));
}

/// An absent member keeps the variant preset — the overlay is sparse
/// (AC-2 — never a re-preset of undeclared members).
#[test]
fn overlay_absent_keeps_preset() {
    let p = overlay(Json::obj([("max_continue_nudges", Json::Int(7))])).unwrap();
    let base = StrategyParams::default();
    assert_eq!(p.max_continue_nudges, 7);
    assert_eq!(
        p.max_consecutive_format_errors,
        base.max_consecutive_format_errors
    );
    assert_eq!(p.stop_rule, base.stop_rule);
    assert_eq!(p.format_failure_scope, FormatFailureScope::Batch);
}

/// An unknown member refuses typed — never a silent drop (AC-2;
/// closed-world `params`).
#[test]
fn overlay_unknown_member_refuses() {
    let err = overlay(Json::obj([("bogus_param", Json::Int(1))])).unwrap_err();
    assert!(matches!(
        err,
        StrategyParamError::UnknownMember { ref member } if member == "bogus_param"
    ));
}

/// `stop_rule = submit{marker, line, require_returncode}` — the AR-24
/// record form decodes the declared marker record (AC-3's arm leg).
#[test]
fn overlay_stop_rule_marker_record() {
    let p = overlay(Json::obj([(
        "stop_rule",
        Json::obj([(
            "submit",
            Json::obj([
                ("marker", Json::str("COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT")),
                ("line", Json::str("first")),
                ("require_returncode", Json::Int(0)),
            ]),
        )]),
    )]))
    .unwrap();
    match p.stop_rule {
        StopRuleParam::Submit {
            marker: Some(marker),
        } => {
            assert_eq!(marker.marker, "COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT");
            assert_eq!(marker.line, SubmitLine::First);
            assert_eq!(marker.require_returncode, 0);
        }
        other => panic!("expected submit{{marker}}, got {other:?}"),
    }
}

/// `line ∉ {first}` is a typed refusal — the closed sum extends only
/// by dialect bump (X-01's typed-refusal surface survives the
/// amendment).
#[test]
fn overlay_stop_rule_bad_line_refuses() {
    let err = overlay(Json::obj([(
        "stop_rule",
        Json::obj([(
            "submit",
            Json::obj([
                ("marker", Json::str("M")),
                ("line", Json::str("second")),
                ("require_returncode", Json::Int(0)),
            ]),
        )]),
    )]))
    .unwrap_err();
    assert!(matches!(err, StrategyParamError::MalformedMember { .. }));
    // A missing record member refuses the same way — never a default.
    let err = overlay(Json::obj([(
        "stop_rule",
        Json::obj([(
            "submit",
            Json::obj([("marker", Json::str("M")), ("line", Json::str("first"))]),
        )]),
    )]))
    .unwrap_err();
    assert!(matches!(err, StrategyParamError::MalformedMember { .. }));
}

/// The bare `submit` shorthand stays admitted — the marker-less preset
/// (dormant detection — the marker is definition data).
#[test]
fn overlay_bare_submit() {
    let p = overlay(Json::obj([("stop_rule", Json::str("submit"))])).unwrap();
    assert!(matches!(
        p.stop_rule,
        StopRuleParam::Submit { marker: None }
    ));
}

/// `format_failure_scope` — `batch` decodes; a `call` spelling refuses
/// typed (the per-call scope is not a Stage-1 semantic — never a
/// silent semantic change).
#[test]
fn overlay_format_failure_scope() {
    let p = overlay(Json::obj([("format_failure_scope", Json::str("batch"))])).unwrap();
    assert_eq!(p.format_failure_scope, FormatFailureScope::Batch);
    let err = overlay(Json::obj([("format_failure_scope", Json::str("call"))])).unwrap_err();
    assert!(matches!(err, StrategyParamError::MalformedMember { .. }));
}

/// `completion_surface` is an admitted member the surface arm owns —
/// the overlay passes it through untouched (the R6.36 arm validates
/// its spelling there).
#[test]
fn overlay_completion_surface_passthrough() {
    let p = overlay(Json::obj([("completion_surface", Json::str("submit"))])).unwrap();
    assert_eq!(p, StrategyParams::default());
}

/// `null` restates the preset — an absent-shaped member never decodes
/// a zero value.
#[test]
fn overlay_null_restates_preset() {
    let p = overlay(Json::obj([("max_continue_nudges", Json::Null)])).unwrap();
    assert_eq!(p, StrategyParams::default());
}

/// `params` that is not an object refuses — never coerced into a
/// default decode.
#[test]
fn overlay_not_object_refuses() {
    let err = strategy_params_overlay(&StrategyParams::default(), &Json::str("x")).unwrap_err();
    assert_eq!(err, StrategyParamError::NotAnObject);
}

/// The resolved record persists + re-reads verbatim (AC-2 resume
/// parity — `leaf.arm.strategy_params` decodes to the same
/// `StrategyParams` the arm used).
#[test]
fn params_record_round_trip_resume_parity() {
    let p = overlay(Json::obj([
        ("max_consecutive_format_errors", Json::Int(9)),
        (
            "stop_rule",
            Json::obj([(
                "submit",
                Json::obj([
                    ("marker", Json::str("M")),
                    ("line", Json::str("first")),
                    ("require_returncode", Json::Int(3)),
                ]),
            )]),
        ),
    ]))
    .unwrap();
    let persisted = strategy_params_to_json(&p);
    let restored = strategy_params_from_json(&persisted).expect("decode persisted");
    assert_eq!(restored, p);
    // Re-decode is deterministic — the resume reads the same params.
    let again = strategy_params_from_json(&persisted).expect("decode persisted");
    assert_eq!(again, restored);
}
