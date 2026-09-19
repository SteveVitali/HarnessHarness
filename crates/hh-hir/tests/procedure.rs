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

// ── select_target — C0 = instruction only ────────────────────────────────────

#[test]
fn select_target_defaults_to_instruction() {
    let p = proc_node("test:proc", vec![], 5);
    let t = select_target(&p, None).expect("instruction is the C0 target");
    assert_eq!(t, CompilationTarget::Instruction);
}

#[test]
fn select_target_refuses_unavailable_targets_typed() {
    let mut p = proc_node("test:proc", vec![], 5);
    p.surface = Some(SurfaceRecord::Procedure(ProcedureSurface {
        compile_hint: CompileHint::WorkflowNode,
    }));
    assert!(matches!(
        select_target(&p, None),
        Err(SelectError::UnexpressibleAsWorkflow { .. })
    ));
    p.surface = Some(SurfaceRecord::Procedure(ProcedureSurface {
        compile_hint: CompileHint::SubagentTask,
    }));
    assert!(matches!(
        select_target(&p, None),
        Err(SelectError::DelegationUnavailable { .. })
    ));
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
