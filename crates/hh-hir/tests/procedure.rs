//! S2.8 procedure-validation evidence — `R-2.4.5⁰` (§5c.5).
//!
//! Covered acceptance ids:
//!
//! - **AC-R-2.4.5-3** — render purity + claims: an `Instruction` carrying the
//!   declared render-time exec marker fails `validate` with
//!   `RenderTimeExecution`; an `Invoke` naming a capability outside
//!   `allowed_capabilities` is `CapabilityNotAllowed` (the claim→grant gap —
//!   only `seal` closes it).
//! - **AC-R-2.4.5-11** — precondition gating: validator-bound and
//!   `validator_passed` preconditions count as `PreconditionUncheckable`
//!   notes in the report, never errors; malformed members are typed
//!   `SchemaViolation`s.
//! - **validate_procedure** (§5c.5 row 3) — `HandlerIncomplete`,
//!   `UnboundParameter`, `CompositionCycle`, `ProcedureUnverifiable`, the
//!   bounded-`Loop` check.
//! - **`select_target`** (§5c.5 row 8) — the C0 target set is
//!   `{instruction}`; `workflow_node`/`subagent_task` hints refuse typed.

mod common;

use common::*;

use hh_hir::document::{HirDocument, Node};
use hh_hir::errors::HirError;
use hh_hir::kinds::{EntityKind, ValidatorKind};
use hh_hir::procedure::{
    check_preconditions, select_target, CompilationTarget, Precondition, PreconditionEnv,
    Predicate, SelectError,
};
use hh_hir::records::{
    CompileHint, KindRecord, ProcedureRecord, ProcedureStep, ProcedureSurface, SurfaceRecord,
    ValidatorRecord,
};
use hh_hir::validate;
use hh_wire::json::Json;

/// `{"step_failed": "any"}` — the catch-all step_failed class spelling.
fn on_step_failed() -> Json {
    Json::obj([("step_failed", Json::str("any"))])
}

/// `{"on": <class>, "then": "abort"}` — the minimal handler row.
fn handler(on: Json) -> Json {
    Json::obj([("on", on), ("then", Json::str("abort"))])
}

/// A minimal procedure node: one `Instruction` step, no preconditions, no
/// handlers, no capabilities — valid on its own.
fn proc_node(id: &str, steps: Vec<ProcedureStep>, seq: u64) -> Node {
    let mut n = sid(
        node(
            EntityKind::Procedure,
            KindRecord::Procedure(ProcedureRecord {
                preconditions: Json::Null,
                steps,
                expected_evidence: Json::Arr(vec![Json::str("ok")]),
                allowed_capabilities: vec![],
                failure_handlers: Json::Arr(vec![]),
            }),
            seq,
        ),
        id,
    );
    n.surface = Some(SurfaceRecord::Procedure(ProcedureSurface {
        compile_hint: CompileHint::Instruction,
    }));
    n
}

fn doc_with(nodes: Vec<Node>) -> HirDocument {
    let mut doc = valid_doc();
    for n in nodes {
        doc.nodes.push(n);
    }
    doc
}

fn errs(doc: &HirDocument) -> Vec<HirError> {
    validate(doc).expect_err("document must not validate")
}

fn has<F: Fn(&HirError) -> bool>(errs: &[HirError], f: F, what: &str) {
    assert!(errs.iter().any(&f), "expected {what}; got {errs:?}");
}

fn proc_mut(n: &mut Node) -> &mut ProcedureRecord {
    match &mut n.semantic {
        KindRecord::Procedure(r) => r,
        _ => unreachable!(),
    }
}

// ── AC-R-2.4.5-3: render purity ──────────────────────────────────────────────

#[test]
fn ac_r_2_4_5_3_render_exec_marker_fails_validate() {
    let doc = doc_with(vec![proc_node(
        "test:proc",
        vec![ProcedureStep::Instruction(text("run !`rm -rf /` now", 5))],
        5,
    )]);
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::RenderTimeExecution { .. }),
        "RenderTimeExecution",
    );
}

#[test]
fn ac_r_2_4_5_3_invoke_outside_allowed_capabilities_is_refused() {
    let mut doc = valid_doc();
    doc.nodes.push(tool_node("test:tool", 5));
    doc.nodes.push(proc_node(
        "test:proc",
        vec![
            ProcedureStep::Instruction(text("call the tool", 6)),
            ProcedureStep::Invoke {
                tool: sel("test:tool"),
                args: Json::Null,
            },
        ],
        6,
    ));
    // `allowed_capabilities` stays empty — the claim never widened to a grant.
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::CapabilityNotAllowed { .. }),
        "CapabilityNotAllowed",
    );
}

#[test]
fn ac_r_2_4_5_3_invoke_inside_allowed_capabilities_validates() {
    let mut doc = valid_doc();
    doc.nodes.push(tool_node("test:tool", 5));
    let mut p = proc_node(
        "test:proc",
        vec![
            ProcedureStep::Instruction(text("call the tool", 6)),
            ProcedureStep::Invoke {
                tool: sel("test:tool"),
                args: Json::Null,
            },
        ],
        6,
    );
    let r = proc_mut(&mut p);
    r.allowed_capabilities = vec![sel("test:tool")];
    // The Invoke derives `step_failed` + `permission_denied` — handlers must
    // be total over the raisable classes.
    r.failure_handlers = Json::Arr(vec![
        handler(on_step_failed()),
        handler(Json::str("permission_denied")),
    ]);
    doc.nodes.push(p);
    let report = validate(&doc).expect("an allowlisted invoke validates");
    assert!(report.checks_run.contains(&"procedure_coherence"));
}

// ── validate_procedure: the §5c.5 row-3 battery ─────────────────────────────

#[test]
fn unbound_parameter_is_a_typed_error() {
    let mut doc = valid_doc();
    doc.nodes.push(tool_node("test:tool", 5));
    let mut p = proc_node(
        "test:proc",
        vec![ProcedureStep::Invoke {
            tool: sel("test:tool"),
            args: Json::obj([("path", Json::str("$param:target"))]),
        }],
        6,
    );
    let r = proc_mut(&mut p);
    r.allowed_capabilities = vec![sel("test:tool")];
    r.failure_handlers = Json::Arr(vec![
        handler(on_step_failed()),
        handler(Json::str("permission_denied")),
    ]);
    doc.nodes.push(p);
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::UnboundParameter { .. }),
        "UnboundParameter",
    );
}

#[test]
fn bound_parameter_validates() {
    let mut doc = valid_doc();
    doc.nodes.push(tool_node("test:tool", 5));
    let mut p = proc_node(
        "test:proc",
        vec![ProcedureStep::Invoke {
            tool: sel("test:tool"),
            args: Json::obj([("path", Json::str("$param:target"))]),
        }],
        6,
    );
    let r = proc_mut(&mut p);
    r.allowed_capabilities = vec![sel("test:tool")];
    r.failure_handlers = Json::Arr(vec![
        handler(on_step_failed()),
        handler(Json::str("permission_denied")),
    ]);
    p.ext.insert(
        hh_hir::procedure::PROCEDURE_PROFILE_EXT_KEY.into(),
        Json::obj([(
            "parameters",
            Json::Arr(vec![Json::obj([
                ("name", Json::str("target")),
                ("type", Json::str("string")),
                ("required", Json::Bool(true)),
            ])]),
        )]),
    );
    doc.nodes.push(p);
    validate(&doc).expect("a declared parameter binds");
}

#[test]
fn handler_incomplete_when_a_declared_class_is_uncovered() {
    let mut p = proc_node("test:proc", vec![], 5);
    // Declare `failure_classes: [timeout]` in the profile but no handler.
    p.ext.insert(
        hh_hir::procedure::PROCEDURE_PROFILE_EXT_KEY.into(),
        Json::obj([("failure_classes", Json::Arr(vec![Json::str("timeout")]))]),
    );
    let es = errs(&doc_with(vec![p]));
    has(
        &es,
        |e| matches!(e, HirError::HandlerIncomplete { .. }),
        "HandlerIncomplete",
    );
}

#[test]
fn composition_cycle_is_detected() {
    // A → B → A through `composition[].callee`.
    let mut a = proc_node("test:pa", vec![], 5);
    a.ext.insert(
        hh_hir::procedure::PROCEDURE_PROFILE_EXT_KEY.into(),
        Json::obj([(
            "composition",
            Json::Arr(vec![Json::obj([("callee", ref_json("test:pb"))])]),
        )]),
    );
    let mut b = proc_node("test:pb", vec![], 6);
    b.ext.insert(
        hh_hir::procedure::PROCEDURE_PROFILE_EXT_KEY.into(),
        Json::obj([(
            "composition",
            Json::Arr(vec![Json::obj([("callee", ref_json("test:pa"))])]),
        )]),
    );
    let es = errs(&doc_with(vec![a, b]));
    has(
        &es,
        |e| matches!(e, HirError::CompositionCycle { .. }),
        "CompositionCycle",
    );
}

/// `{"semantic_id": sid, "version_selector": "latest"}` — the `Ref` JSON.
fn ref_json(sid_: &str) -> Json {
    Json::obj([
        ("semantic_id", Json::str(sid_)),
        ("version_selector", Json::str("latest")),
    ])
}

#[test]
fn acyclic_composition_validates() {
    let mut a = proc_node("test:pa", vec![], 5);
    a.ext.insert(
        hh_hir::procedure::PROCEDURE_PROFILE_EXT_KEY.into(),
        Json::obj([(
            "composition",
            Json::Arr(vec![Json::obj([("callee", ref_json("test:pb"))])]),
        )]),
    );
    let b = proc_node("test:pb", vec![], 6);
    validate(&doc_with(vec![a, b])).expect("a DAG composition validates");
}

#[test]
fn procedure_unverifiable_risk_floor() {
    // An `Opaque` step whose declared interface has an `open`-world effect
    // and the procedure declares no `expected_evidence`.
    let mut e = hh_hir::kinds::EffectClass::domain_only(hh_hir::kinds::EffectDomain::Exec);
    e.attributes = Some(hh_hir::kinds::EffectAttributes {
        mutability: hh_hir::kinds::Mutability::Destructive,
        repeat_safety: hh_hir::kinds::RepeatSafety::NonIdempotent,
        world: hh_hir::kinds::World::Open,
        reversibility: hh_hir::kinds::Reversibility::Irreversible,
    });
    let payload = hh_hir::leaves::CompiledPayload {
        format_tag: "script".into(),
        bytes_hash: "sha256:deadbeef".into(),
        declared_interface: Some(hh_hir::leaves::DeclaredInterface {
            inputs: Json::Null,
            outputs: Json::Null,
            effects: [e].into_iter().collect(),
            deterministic: false,
            target: "subprocess_confined".into(),
        }),
        owner: "test".into(),
        provenance: prov(5),
    };
    let mut p = proc_node("test:proc", vec![ProcedureStep::Opaque(payload)], 5);
    let r = proc_mut(&mut p);
    r.expected_evidence = Json::Null; // no evidence declared
    r.failure_handlers = Json::Arr(vec![handler(on_step_failed())]);
    let es = errs(&doc_with(vec![p]));
    has(
        &es,
        |e| matches!(e, HirError::ProcedureUnverifiable { .. }),
        "ProcedureUnverifiable",
    );
}

#[test]
fn loop_bound_must_resolve_to_a_budget() {
    let mut doc = valid_doc();
    doc.nodes.push(tool_node("test:tool", 5));
    let mut p = proc_node(
        "test:proc",
        vec![ProcedureStep::Loop {
            bound: sel("test:tool"), // not a Budget
            body: vec![ProcedureStep::Instruction(text("x", 6))],
        }],
        6,
    );
    proc_mut(&mut p).failure_handlers = Json::Arr(vec![handler(Json::str("budget_exhausted"))]);
    doc.nodes.push(p);
    let es = errs(&doc);
    has(
        &es,
        |e| matches!(e, HirError::UnresolvedRef { .. }),
        "UnresolvedRef (loop bound is not a Budget)",
    );
}

// ── AC-R-2.4.5-11: precondition gating ──────────────────────────────────────

#[test]
fn ac_r_2_4_5_11_uncheckable_preconditions_counted_not_errors() {
    let mut p = proc_node("test:proc", vec![], 5);
    let r = proc_mut(&mut p);
    r.preconditions = Json::Arr(vec![Json::obj([
        ("kind", Json::str("validator_passed")),
        ("ref", ref_json("test:validator")),
        ("within", Json::Int(10)),
    ])]);
    // `precondition_failed` becomes a raisable class — cover it.
    r.failure_handlers = Json::Arr(vec![handler(Json::str("precondition_failed"))]);
    let mut doc = valid_doc();
    // The validator ref must resolve to a Validator node.
    doc.nodes.push(sid(
        node(
            EntityKind::Validator,
            KindRecord::Validator(ValidatorRecord {
                kind: ValidatorKind::Schema,
                inputs: vec![],
                verdict_type: "verdict".into(),
                deterministic: true,
                cost: Json::Null,
                evidence_out: Json::Null,
                assumption_debt: None,
            }),
            6,
        ),
        "test:validator",
    ));
    doc.nodes.push(p);
    let report = validate(&doc).expect("uncheckable preconditions are notes");
    assert_eq!(report.uncheckable_preconditions, 1);
}

#[test]
fn ac_r_2_4_5_11_validator_ref_member_also_counted() {
    let mut p = proc_node("test:proc", vec![], 5);
    let r = proc_mut(&mut p);
    r.preconditions = Json::Arr(vec![Json::obj([("validator", ref_json("test:validator"))])]);
    r.failure_handlers = Json::Arr(vec![handler(Json::str("precondition_failed"))]);
    let mut doc = valid_doc();
    doc.nodes.push(sid(
        node(
            EntityKind::Validator,
            KindRecord::Validator(ValidatorRecord {
                kind: ValidatorKind::Schema,
                inputs: vec![],
                verdict_type: "verdict".into(),
                deterministic: true,
                cost: Json::Null,
                evidence_out: Json::Null,
                assumption_debt: None,
            }),
            6,
        ),
        "test:validator",
    ));
    doc.nodes.push(p);
    let report = validate(&doc).expect("validator refs are notes");
    assert_eq!(report.uncheckable_preconditions, 1);
}

#[test]
fn ac_r_2_4_5_11_malformed_precondition_is_schema_violation() {
    let mut p = proc_node("test:proc", vec![], 5);
    proc_mut(&mut p).preconditions = Json::Arr(vec![Json::obj([("bogus", Json::Bool(true))])]);
    let es = errs(&doc_with(vec![p]));
    has(
        &es,
        |e| matches!(e, HirError::SchemaViolation { .. }),
        "SchemaViolation",
    );
}

// ── select_target — the full Stage-3 rule (§5c.5 row 8) ─────────────────────

use hh_hir::procedure::{
    b_procedure, i_procedure, validate_hook_guard, HookGuardError, SelectCtx, TargetDecision,
    WorkflowReason,
};
use std::collections::BTreeMap;

/// An empty resolution index (no `Loop` bounds to resolve).
fn empty_index<'a>() -> BTreeMap<String, &'a Node> {
    BTreeMap::new()
}

/// A ctx that declares `workflow_execution`.
fn wf_ctx() -> SelectCtx {
    SelectCtx {
        workflow_execution_declared: true,
        ..SelectCtx::default()
    }
}

#[test]
fn select_target_defaults_to_instruction() {
    let p = proc_node("test:proc", vec![], 5);
    let d = select_target(&p, None, &SelectCtx::default(), &empty_index())
        .expect("instruction is the default target");
    assert_eq!(d.target, CompilationTarget::Instruction);
    assert_eq!(d.rule_id, "default_instruction");
    assert!(d.i, "an empty body is self-contained");
}

#[test]
fn select_target_refuses_unavailable_targets_typed() {
    let mut p = proc_node("test:proc", vec![], 5);
    p.surface = Some(SurfaceRecord::Procedure(ProcedureSurface {
        compile_hint: CompileHint::WorkflowNode,
    }));
    // An override whose strategy isn't bound is `TargetInfeasible`, not a
    // silent fallback to `instruction`.
    assert!(matches!(
        select_target(&p, None, &SelectCtx::default(), &empty_index()),
        Err(SelectError::TargetInfeasible { .. })
    ));
    // With the strategy bound and an empty (compilable) body the override
    // is honoured.
    assert_eq!(
        select_target(&p, None, &wf_ctx(), &empty_index())
            .unwrap()
            .target,
        CompilationTarget::WorkflowNode
    );
    p.surface = Some(SurfaceRecord::Procedure(ProcedureSurface {
        compile_hint: CompileHint::SubagentTask,
    }));
    assert!(matches!(
        select_target(&p, None, &SelectCtx::default(), &empty_index()),
        Err(SelectError::DelegationUnavailable { .. })
    ));
}

#[test]
fn b_procedure_typed_failure_reasons() {
    let params = BTreeMap::new();
    // unbound_arg
    let steps = vec![ProcedureStep::Invoke {
        tool: sel("test:tool"),
        args: Json::obj([("path", Json::str("$param:missing"))]),
    }];
    let (step, reason) = b_procedure(&steps, &params, &empty_index(), "test:proc").unwrap_err();
    assert!(step.contains("step[0]"));
    assert_eq!(reason, WorkflowReason::UnboundArg);
    // uncheckable_branch — a free-form condition no validator backs.
    let steps = vec![ProcedureStep::Branch {
        condition: Json::str("whenever it feels right"),
        then_body: vec![],
        else_body: vec![],
    }];
    let (_, reason) = b_procedure(&steps, &params, &empty_index(), "p").unwrap_err();
    assert_eq!(reason, WorkflowReason::UncheckableBranch);
    // unbounded_loop — the bound doesn't resolve to a Budget.
    let steps = vec![ProcedureStep::Loop {
        bound: sel("test:tool"),
        body: vec![],
    }];
    let (_, reason) = b_procedure(&steps, &params, &empty_index(), "p").unwrap_err();
    assert_eq!(reason, WorkflowReason::UnboundedLoop);
    // opaque_without_interface
    let payload = hh_hir::leaves::CompiledPayload {
        format_tag: "script".into(),
        bytes_hash: "sha256:deadbeef".into(),
        declared_interface: None,
        owner: "test".into(),
        provenance: prov(5),
    };
    let steps = vec![ProcedureStep::Opaque(payload)];
    let (_, reason) = b_procedure(&steps, &params, &empty_index(), "p").unwrap_err();
    assert_eq!(reason, WorkflowReason::OpaqueWithoutInterface);
}

#[test]
fn select_target_workflow_node_when_b_and_strategy_declared() {
    // A validator-conditioned branch is compilable — `workflow_node` wins
    // under a `workflow_execution` control strategy.
    let p = proc_node(
        "test:proc",
        vec![ProcedureStep::Branch {
            condition: Json::obj([("kind", Json::str("validator_passed"))]),
            then_body: vec![ProcedureStep::Instruction(text("yes", 6))],
            else_body: vec![ProcedureStep::Instruction(text("no", 7))],
        }],
        5,
    );
    let d = select_target(&p, None, &wf_ctx(), &empty_index()).unwrap();
    assert_eq!(d.target, CompilationTarget::WorkflowNode);
    assert_eq!(d.rule_id, "b_workflow");
    assert!(d.b);
    // The same procedure under the default ctx is `instruction` — the
    // decision record explains why (`B` true, no declared strategy).
    let d = select_target(&p, None, &SelectCtx::default(), &empty_index()).unwrap();
    assert_eq!(d.target, CompilationTarget::Instruction);
    assert!(d.b, "B(P) holds — the ctx refused the workflow path");
}

#[test]
fn select_target_unexpressible_as_workflow_falls_through_to_instruction() {
    // An uncheckable branch: `B(P)` fails typed, and under the default ctx
    // the procedure still compiles to `instruction` (rule ordering — the
    // typed reason is only a refusal when workflow is the only path).
    let p = proc_node(
        "test:proc",
        vec![ProcedureStep::Branch {
            condition: Json::str("vibes"),
            then_body: vec![],
            else_body: vec![],
        }],
        5,
    );
    let d = select_target(&p, None, &SelectCtx::default(), &empty_index()).unwrap();
    assert_eq!(d.target, CompilationTarget::Instruction);
    assert!(!d.b);
    // With the strategy declared, B fails so workflow is skipped — still
    // instruction (rule iv), never a silent WorkflowNode.
    let d = select_target(&p, None, &wf_ctx(), &empty_index()).unwrap();
    assert_eq!(d.target, CompilationTarget::Instruction);
}

#[test]
fn select_target_subagent_path_is_delegation_unavailable() {
    // I(P) ∧ subagents bound ∧ declared → `DelegationUnavailable` (the
    // honest Stage-3 refusal — subagent_task never silently lands).
    let p = proc_node("test:proc", vec![], 5);
    let ctx = SelectCtx {
        subagents_bound: true,
        profile_declares_subagents: true,
        ..SelectCtx::default()
    };
    assert!(matches!(
        select_target(&p, None, &ctx, &empty_index()),
        Err(SelectError::DelegationUnavailable { .. })
    ));
}

#[test]
fn select_target_unexpressible_surface_when_body_exceeds_budget() {
    let p = proc_node(
        "test:proc",
        vec![ProcedureStep::Instruction(text(&"x".repeat(200), 5))],
        5,
    );
    let ctx = SelectCtx {
        procedure_inline_budget: 16,
        ..SelectCtx::default()
    };
    assert!(matches!(
        select_target(&p, None, &ctx, &empty_index()),
        Err(SelectError::UnexpressibleSurface { .. })
    ));
}

#[test]
fn select_target_risk_floor_requires_expected_evidence() {
    // An Opaque step with an open-world irreversible effect: `R(P)` holds,
    // `expected_evidence` empty → `ProcedureUnverifiable` on every path.
    let mut e = hh_hir::kinds::EffectClass::domain_only(hh_hir::kinds::EffectDomain::Exec);
    e.attributes = Some(hh_hir::kinds::EffectAttributes {
        mutability: hh_hir::kinds::Mutability::Destructive,
        repeat_safety: hh_hir::kinds::RepeatSafety::NonIdempotent,
        world: hh_hir::kinds::World::Open,
        reversibility: hh_hir::kinds::Reversibility::Irreversible,
    });
    let payload = hh_hir::leaves::CompiledPayload {
        format_tag: "script".into(),
        bytes_hash: "sha256:deadbeef".into(),
        declared_interface: Some(hh_hir::leaves::DeclaredInterface {
            inputs: Json::Null,
            outputs: Json::Null,
            effects: [e].into_iter().collect(),
            deterministic: false,
            target: "subprocess_confined".into(),
        }),
        owner: "test".into(),
        provenance: prov(5),
    };
    let mut p = proc_node("test:proc", vec![ProcedureStep::Opaque(payload)], 5);
    proc_mut(&mut p).expected_evidence = Json::Null;
    assert!(matches!(
        select_target(&p, None, &SelectCtx::default(), &empty_index()),
        Err(SelectError::ProcedureUnverifiable { .. })
    ));
    // Declaring expected_evidence discharges the floor.
    proc_mut(&mut p).expected_evidence = Json::Arr(vec![Json::str("exit-code")]);
    let d = select_target(&p, None, &SelectCtx::default(), &empty_index()).unwrap();
    assert_eq!(d.target, CompilationTarget::Instruction);
    assert!(d.r.contains(&"irreversible".to_string()));
    assert!(d.r.contains(&"scope:external".to_string()));
}

#[test]
fn i_procedure_isolation_predicate() {
    let params = BTreeMap::new();
    // A Delegate step breaks isolation.
    let steps = vec![ProcedureStep::Delegate {
        spec: Json::Null,
        budget: sel("test:budget"),
        permission: sel("test:perm"),
    }];
    assert!(!i_procedure(&steps, &params));
    // A self-contained body isolates.
    let steps = vec![ProcedureStep::Instruction(text("self-contained", 5))];
    assert!(i_procedure(&steps, &params));
    // A body referencing an undeclared param reaches outside its surface.
    let steps = vec![ProcedureStep::Invoke {
        tool: sel("test:tool"),
        args: Json::obj([("x", Json::str("$param:undeclared"))]),
    }];
    assert!(!i_procedure(&steps, &params));
}

#[test]
fn ac_r_2_4_5_5_select_target_total_on_corpus() {
    // ≥50 procedures, every outcome a `TargetDecision` or one of the four
    // typed refusals — never a panic, never a silent fallback.
    let ctxs = [
        SelectCtx::default(),
        wf_ctx(),
        SelectCtx {
            subagents_bound: true,
            profile_declares_subagents: true,
            ..SelectCtx::default()
        },
        SelectCtx {
            procedure_inline_budget: 4,
            ..SelectCtx::default()
        },
    ];
    let mut outcomes = 0usize;
    for i in 0..64u64 {
        let steps = match i % 4 {
            0 => vec![ProcedureStep::Instruction(text(&format!("body {i}"), 5))],
            1 => vec![ProcedureStep::Branch {
                condition: Json::obj([("kind", Json::str("validator_passed"))]),
                then_body: vec![ProcedureStep::Instruction(text("t", 6))],
                else_body: vec![],
            }],
            2 => vec![ProcedureStep::Loop {
                bound: sel("test:budget"),
                body: vec![],
            }],
            _ => vec![ProcedureStep::Invoke {
                tool: sel("test:tool"),
                args: Json::obj([("p", Json::str("$param:x"))]),
            }],
        };
        let p = proc_node(&format!("test:proc-{i}"), steps, 5);
        for ctx in &ctxs {
            match select_target(&p, None, ctx, &empty_index()) {
                Ok(TargetDecision {
                    target,
                    b,
                    i,
                    rule_id,
                    ..
                }) => {
                    // Every non-instruction result is explained by {B, I}.
                    if target == CompilationTarget::WorkflowNode {
                        assert!(b, "workflow_node without B(P): {rule_id}");
                    }
                    if target == CompilationTarget::SubagentTask {
                        assert!(i, "subagent_task without I(P)");
                    }
                    outcomes += 1;
                }
                Err(e) => {
                    assert!(
                        matches!(
                            e,
                            SelectError::TargetInfeasible { .. }
                                | SelectError::UnexpressibleAsWorkflow { .. }
                                | SelectError::DelegationUnavailable { .. }
                                | SelectError::ProcedureUnverifiable { .. }
                                | SelectError::UnexpressibleSurface { .. }
                        ),
                        "a typed refusal, never a panic: {e:?}"
                    );
                    outcomes += 1;
                }
            }
        }
    }
    assert_eq!(outcomes, 64 * 4, "total: every (P, ctx) decides");
}

// ── AC-R-2.4.5-12: hook bodies — the closed guard-output vocabulary ─────────

#[test]
fn ac_r_2_4_5_12_guard_allow_is_refused() {
    let steps: Vec<ProcedureStep> = vec![];
    let grants = std::collections::BTreeSet::new();
    // `allow` as an output key is refused.
    let outputs = Json::obj([("allow", Json::Bool(true))]);
    assert_eq!(
        validate_hook_guard(&steps, &outputs, &grants),
        Err(HookGuardError::GuardReturnsAllow)
    );
    // `narrow: allow` is refused too — `allow` is never a guard output.
    let outputs = Json::obj([("narrow", Json::str("allow"))]);
    assert_eq!(
        validate_hook_guard(&steps, &outputs, &grants),
        Err(HookGuardError::GuardReturnsAllow)
    );
}

#[test]
fn ac_r_2_4_5_12_guard_vocabulary_is_closed() {
    let steps: Vec<ProcedureStep> = vec![];
    let grants = ["test:tool".to_string()].into_iter().collect();
    // `{narrow, additional_context?, replacement_proposal?}` validates.
    let outputs = Json::obj([
        ("narrow", Json::str("deny")),
        (
            "additional_context",
            Json::obj([("max_tokens", Json::Int(64))]),
        ),
        (
            "replacement_proposal",
            Json::obj([("reenters", Json::str("authorize"))]),
        ),
    ]);
    validate_hook_guard(&steps, &outputs, &grants).expect("the closed vocabulary validates");
    // A foreign key is refused typed.
    let outputs = Json::obj([("side_effect", Json::Bool(true))]);
    assert!(matches!(
        validate_hook_guard(&steps, &outputs, &grants),
        Err(HookGuardError::GuardOutputOutOfVocabulary { .. })
    ));
    // `narrow` outside {deny, ask, none}.
    let outputs = Json::obj([("narrow", Json::str("maybe"))]);
    assert!(matches!(
        validate_hook_guard(&steps, &outputs, &grants),
        Err(HookGuardError::BadNarrow { .. })
    ));
}

#[test]
fn ac_r_2_4_5_12_guard_invokes_must_be_granted_reads() {
    // A guard narrows or proposes — an Invoke on an ungranted capability is
    // an effect the guard may not perform (a `replacement_proposal`
    // re-enters `authorize`; it never runs directly).
    let steps = vec![ProcedureStep::Invoke {
        tool: sel("test:ungranted"),
        args: Json::Null,
    }];
    let grants = std::collections::BTreeSet::new();
    let outputs = Json::obj([("narrow", Json::str("none"))]);
    assert!(matches!(
        validate_hook_guard(&steps, &outputs, &grants),
        Err(HookGuardError::GuardHasEffects { .. })
    ));
    let grants = ["test:ungranted".to_string()].into_iter().collect();
    validate_hook_guard(&steps, &outputs, &grants).expect("a granted read is fine");
}

// ── check_preconditions — the run-side evaluation ────────────────────────────

#[test]
fn check_preconditions_path_glob_and_env() {
    let pre = vec![
        Precondition::Check(Predicate::PathGlob {
            pattern: "src/**".into(),
        }),
        Precondition::Check(Predicate::EnvRequires { key: "CI".into() }),
    ];
    let env = PreconditionEnv {
        paths: Some(["src/main.rs".to_string()].into_iter().collect()),
        env_keys: Some(["CI".to_string()].into_iter().collect()),
        ..Default::default()
    };
    let report = check_preconditions(&pre, &env);
    assert_eq!(report.satisfied.len(), 2);
    assert!(report.violated.is_empty());
    assert!(report.unknown.is_empty());

    // A missing env key violates `env_requires` — decided, not unknown.
    let env_missing = PreconditionEnv {
        paths: env.paths.clone(),
        env_keys: Some(Default::default()),
        ..Default::default()
    };
    let report = check_preconditions(&pre, &env_missing);
    assert_eq!(report.violated.len(), 1);

    // An env the run cannot decide reports `unknown` — the
    // PreconditionUncheckable note, never a silent pass.
    let env_na = PreconditionEnv {
        paths: env.paths.clone(),
        env_keys: None, // the run cannot decide env facts → `unknown`
        ..Default::default()
    };
    let report = check_preconditions(&pre, &env_na);
    assert_eq!(report.unknown.len(), 1);
    assert!(
        report.violated.is_empty(),
        "undecidable is never a violation"
    );
}
