//! `hh_control::wire` — the canonical `control_strategy` wire documents
//! (R2.6; the OQ-316 steer arm's companion). The OOP conformance lane
//! (§8.4; `plugin_abi/1` `invoke`) drives a variant out-of-process, and
//! the in-process reference lane drives the same corpus — both directions
//! pass through this one codec, so the checked record is byte-identical
//! either way (CC1 — one spelling, one implementation).
//!
//! Documents (canonical JSON — closed-world decode: an unknown member,
//! an unknown variant tag, or a mistyped field is `None`, never a
//! coercion):
//!
//! * **`context`** — `open`/`restore` input (`ControlContext`): `{process_ref,
//!   plan[], boundary{assignments, guards}, profile, account_ref,
//!   budget_ref, envelope_ref, parameters, capabilities_available[],
//!   steering{steer_mode, concurrent_input}}`.
//! * **`cue`** — `decide` input: `{kind: <cue kind>, …members}` (the
//!   closed thirteen-member sum; `human_input`/`envelope_signal` carry
//!   their nested `kind` member).
//! * **`decision`** — `decide` output: `{stamp{decision_point, owner,
//!   rationale_ref?}, kind: <decision kind>, …members}`.
//! * **`event`** — `observe` input: the *observation projection*
//!   `{seq, class, scope{…}, payload}` — the members the strategy contract
//!   may read; the remaining envelope stamps are host-owned (a variant
//!   never sees `producer`/`hash`/`lease_generation` — INV-7's boundary
//!   reads through, never across).
//! * **`report`** — `terminate` output (`FinalReport`).
//! * **error documents** — `{error: "<spelling>"}` for `open`/`restore`
//!   refusals (`control_error`/`restore_error` arms).

use std::collections::BTreeMap;

use hh_compiler::plan::{
    BranchOnValidatorNode, DelegateNode, LoopNode, PinnedRef, PlanNode, PlanNodePayload,
    StepAction, StepMode, StepNode, StopRuleNode,
};
use hh_ledger::classes::Durability;
use hh_ledger::event::{EventEnvelope, EventPlane, Producer, Scope};
use hh_ledger::manifest::{ObservabilityLevel, ParticipantClass};
use hh_ontology::control::{CancelledBy, ControlBoundary, StopKind, StopReason};
use hh_wire::json::Json;

use crate::state::{
    decision_point_str, owner_str, parse_decision_point, parse_owner, ControlState,
};
use crate::strategy::{
    ConcurrentInput, ControlContext, ControlError, FinalReport, PlanProvenance, RestoreError,
    SteerMode, StopRuleParam, StrategyParams, TruncatedResponse,
};
use crate::vocab::{
    ActMode, ControlDecision, Cue, DecisionKind, DecisionStamp, DeliveryMode, EffectOutcome,
    EnvelopeSignal, EscalateAsk, ExpectedOutput, HumanInput, OnPartial, RetryTarget,
    SettledOutcome, WaitUntil, WokenTrigger,
};

// ── small members ─────────────────────────────────────────────────────

fn req_str<'a>(j: &'a Json, k: &str) -> Option<&'a str> {
    j.get(k).and_then(Json::as_str)
}

fn req_i64(j: &Json, k: &str) -> Option<i64> {
    j.get(k).and_then(Json::as_int)
}

fn pinned_ref_to_json(r: &PinnedRef) -> Json {
    Json::obj([
        ("semantic_id", Json::str(r.semantic_id.clone())),
        ("version_id", Json::str(r.version_id.clone())),
    ])
}

fn pinned_ref_from_json(j: &Json) -> Option<PinnedRef> {
    Some(PinnedRef {
        semantic_id: req_str(j, "semantic_id")?.to_string(),
        version_id: req_str(j, "version_id")?.to_string(),
    })
}

fn stop_rule_str(p: StopRuleParam) -> &'static str {
    match p {
        StopRuleParam::NoAction => "no_action",
        StopRuleParam::Submit => "submit",
        StopRuleParam::Either => "either",
    }
}

fn stop_rule_parse(s: &str) -> Option<StopRuleParam> {
    Some(match s {
        "no_action" => StopRuleParam::NoAction,
        "submit" => StopRuleParam::Submit,
        "either" => StopRuleParam::Either,
        _ => return None,
    })
}

fn truncated_str(t: TruncatedResponse) -> &'static str {
    match t {
        TruncatedResponse::FailCalls => "fail_calls",
    }
}

fn truncated_parse(s: &str) -> Option<TruncatedResponse> {
    match s {
        "fail_calls" => Some(TruncatedResponse::FailCalls),
        _ => None,
    }
}

fn provenance_str(p: PlanProvenance) -> &'static str {
    match p {
        PlanProvenance::Compiled => "compiled",
        PlanProvenance::ModelEmitted => "model_emitted",
    }
}

fn provenance_parse(s: &str) -> Option<PlanProvenance> {
    Some(match s {
        "compiled" => PlanProvenance::Compiled,
        "model_emitted" => PlanProvenance::ModelEmitted,
        _ => return None,
    })
}

fn replan_str(r: crate::strategy::ReplanOn) -> &'static str {
    match r {
        crate::strategy::ReplanOn::Never => "never",
        crate::strategy::ReplanOn::Failure => "failure",
        crate::strategy::ReplanOn::Always => "always",
    }
}

fn replan_parse(s: &str) -> Option<crate::strategy::ReplanOn> {
    Some(match s {
        "never" => crate::strategy::ReplanOn::Never,
        "failure" => crate::strategy::ReplanOn::Failure,
        "always" => crate::strategy::ReplanOn::Always,
        _ => return None,
    })
}

fn act_mode_parse(s: &str) -> Option<ActMode> {
    Some(match s {
        "sequential" => ActMode::Sequential,
        "parallel" => ActMode::Parallel,
        _ => return None,
    })
}

// ── parameters ────────────────────────────────────────────────────────

fn params_to_json(p: &StrategyParams) -> Json {
    Json::obj([
        ("stop_rule", Json::str(stop_rule_str(p.stop_rule))),
        (
            "max_continue_nudges",
            Json::Int(p.max_continue_nudges as i64),
        ),
        (
            "max_consecutive_format_errors",
            Json::Int(p.max_consecutive_format_errors as i64),
        ),
        (
            "max_consecutive_errors",
            Json::Int(p.max_consecutive_errors as i64),
        ),
        ("tool_batch_mode", Json::str(p.tool_batch_mode.as_str())),
        (
            "on_truncated_response",
            Json::str(truncated_str(p.on_truncated_response)),
        ),
        (
            "plan_provenance",
            Json::str(provenance_str(p.plan_provenance)),
        ),
        ("replan_on", Json::str(replan_str(p.replan_on))),
        (
            "loop_guards",
            Json::Arr(p.loop_guards.iter().map(|g| Json::str(g.clone())).collect()),
        ),
    ])
}

fn params_from_json(j: &Json) -> Option<StrategyParams> {
    Some(StrategyParams {
        stop_rule: stop_rule_parse(req_str(j, "stop_rule")?)?,
        max_continue_nudges: req_i64(j, "max_continue_nudges")?.max(0) as u32,
        max_consecutive_format_errors: req_i64(j, "max_consecutive_format_errors")?.max(0) as u32,
        max_consecutive_errors: req_i64(j, "max_consecutive_errors")?.max(0) as u32,
        tool_batch_mode: act_mode_parse(req_str(j, "tool_batch_mode")?)?,
        on_truncated_response: truncated_parse(req_str(j, "on_truncated_response")?)?,
        plan_provenance: provenance_parse(req_str(j, "plan_provenance")?)?,
        replan_on: replan_parse(req_str(j, "replan_on")?)?,
        loop_guards: match j.get("loop_guards")? {
            Json::Arr(a) => a
                .iter()
                .map(|v| v.as_str().map(str::to_string))
                .collect::<Option<Vec<String>>>()?,
            _ => return None,
        },
    })
}

// ── boundary ──────────────────────────────────────────────────────────

fn boundary_to_json(b: &ControlBoundary) -> Json {
    Json::obj([
        (
            "assignments",
            Json::Obj(
                b.assignments
                    .iter()
                    .map(|(p, o)| {
                        (
                            decision_point_str(*p).to_string(),
                            Json::str(owner_str(*o).to_string()),
                        )
                    })
                    .collect(),
            ),
        ),
        (
            "guards",
            Json::Obj(
                b.guards
                    .iter()
                    .map(|(p, g)| (decision_point_str(*p).to_string(), Json::str(g.clone())))
                    .collect(),
            ),
        ),
    ])
}

fn boundary_from_json(j: &Json) -> Option<ControlBoundary> {
    let mut b = ControlBoundary::default();
    match j.get("assignments")? {
        Json::Obj(m) => {
            for (k, v) in m {
                let p = parse_decision_point(k)?;
                let o = parse_owner(v.as_str()?)?;
                b.assignments.insert(p, o);
            }
        }
        _ => return None,
    }
    if let Some(Json::Obj(g)) = j.get("guards") {
        for (k, v) in g {
            let p = parse_decision_point(k)?;
            b.guards.insert(p, v.as_str()?.to_string());
        }
    }
    Some(b)
}

// ── plan nodes ────────────────────────────────────────────────────────

fn step_mode_str(m: StepMode) -> &'static str {
    match m {
        StepMode::Sequential => "sequential",
        StepMode::Parallel => "parallel",
    }
}

fn plan_node_to_json(n: &PlanNode) -> Json {
    let mut m = vec![
        ("node_id", Json::str(n.node_id.clone())),
        ("hir_node_id", Json::str(n.hir_node_id.clone())),
        ("hir_version_id", Json::str(n.hir_version_id.clone())),
    ];
    match &n.payload {
        PlanNodePayload::Loop(l) => {
            m.push(("kind", Json::str("loop")));
            m.push(("bound_budget", pinned_ref_to_json(&l.bound_budget)));
            m.push(("replan_on", Json::str(l.replan_on.name())));
            m.push((
                "body",
                Json::Arr(l.body.iter().map(plan_node_to_json).collect()),
            ));
        }
        PlanNodePayload::Step(s) => {
            m.push(("kind", Json::str("step")));
            m.push(("mode", Json::str(step_mode_str(s.mode))));
            match &s.output_schema {
                Some(r) => m.push(("output_schema", pinned_ref_to_json(r))),
                None => m.push(("output_schema", Json::Null)),
            }
            match &s.action {
                StepAction::Instruction {
                    content_hash,
                    owner,
                } => m.push((
                    "action",
                    Json::obj([
                        ("kind", Json::str("instruction")),
                        ("content_hash", Json::str(content_hash.clone())),
                        ("owner", Json::str(owner.clone())),
                    ]),
                )),
                StepAction::Invoke { capability, args } => m.push((
                    "action",
                    Json::obj([
                        ("kind", Json::str("invoke")),
                        ("capability", pinned_ref_to_json(capability)),
                        ("args", args.clone()),
                    ]),
                )),
            }
        }
        PlanNodePayload::BranchOnValidator(b) => {
            m.push(("kind", Json::str("branch_on_validator")));
            m.push(("validator", pinned_ref_to_json(&b.validator)));
            m.push((
                "then_body",
                Json::Arr(b.then_body.iter().map(plan_node_to_json).collect()),
            ));
            m.push((
                "else_body",
                Json::Arr(b.else_body.iter().map(plan_node_to_json).collect()),
            ));
        }
        PlanNodePayload::Delegate(d) => {
            m.push(("kind", Json::str("delegate")));
            m.push(("spec", d.spec.clone()));
            m.push(("budget", pinned_ref_to_json(&d.budget)));
            m.push(("permission", pinned_ref_to_json(&d.permission)));
        }
        PlanNodePayload::StopRule(s) => {
            m.push(("kind", Json::str("stop_rule")));
            m.push(("reason", Json::str(s.reason.as_str())));
            match &s.bound {
                Some(r) => m.push(("bound", pinned_ref_to_json(r))),
                None => m.push(("bound", Json::Null)),
            }
            match &s.condition {
                Some(c) => m.push(("condition", c.clone())),
                None => m.push(("condition", Json::Null)),
            }
        }
    }
    Json::Obj(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn plan_nodes_from_json(j: &Json) -> Option<Vec<PlanNode>> {
    match j {
        Json::Arr(a) => a.iter().map(plan_node_from_json).collect(),
        _ => None,
    }
}

fn plan_node_from_json(j: &Json) -> Option<PlanNode> {
    let node_id = req_str(j, "node_id")?.to_string();
    let hir_node_id = req_str(j, "hir_node_id")?.to_string();
    let hir_version_id = req_str(j, "hir_version_id")?.to_string();
    let payload = match req_str(j, "kind")? {
        "loop" => PlanNodePayload::Loop(LoopNode {
            bound_budget: pinned_ref_from_json(j.get("bound_budget")?)?,
            replan_on: hh_compiler::plan::ReplanOn::parse(req_str(j, "replan_on")?)?,
            body: plan_nodes_from_json(j.get("body")?)?,
        }),
        "step" => PlanNodePayload::Step(StepNode {
            mode: match req_str(j, "mode")? {
                "sequential" => StepMode::Sequential,
                "parallel" => StepMode::Parallel,
                _ => return None,
            },
            output_schema: match j.get("output_schema")? {
                Json::Null => None,
                r => Some(pinned_ref_from_json(r)?),
            },
            action: match req_str(j.get("action")?, "kind")? {
                "instruction" => StepAction::Instruction {
                    content_hash: req_str(j.get("action")?, "content_hash")?.to_string(),
                    owner: req_str(j.get("action")?, "owner")?.to_string(),
                },
                "invoke" => StepAction::Invoke {
                    capability: pinned_ref_from_json(j.get("action")?.get("capability")?)?,
                    args: j.get("action")?.get("args").cloned().unwrap_or(Json::Null),
                },
                _ => return None,
            },
        }),
        "branch_on_validator" => PlanNodePayload::BranchOnValidator(BranchOnValidatorNode {
            validator: pinned_ref_from_json(j.get("validator")?)?,
            then_body: plan_nodes_from_json(j.get("then_body")?)?,
            else_body: plan_nodes_from_json(j.get("else_body")?)?,
        }),
        "delegate" => PlanNodePayload::Delegate(DelegateNode {
            spec: j.get("spec").cloned().unwrap_or(Json::Null),
            budget: pinned_ref_from_json(j.get("budget")?)?,
            permission: pinned_ref_from_json(j.get("permission")?)?,
        }),
        "stop_rule" => PlanNodePayload::StopRule(StopRuleNode {
            reason: StopKind::parse(req_str(j, "reason")?)?,
            bound: match j.get("bound") {
                Some(Json::Null) | None => None,
                Some(r) => Some(pinned_ref_from_json(r)?),
            },
            condition: match j.get("condition") {
                Some(Json::Null) | None => None,
                c => c.cloned(),
            },
        }),
        _ => return None,
    };
    Some(PlanNode {
        node_id,
        hir_node_id,
        hir_version_id,
        payload,
    })
}

// ── context ───────────────────────────────────────────────────────────

/// The canonical `context` document (`open`/`restore` input).
pub fn context_to_json(ctx: &ControlContext) -> Json {
    Json::obj([
        ("process_ref", Json::str(ctx.process_ref.clone())),
        (
            "plan",
            Json::Arr(ctx.plan.iter().map(plan_node_to_json).collect()),
        ),
        ("boundary", boundary_to_json(&ctx.boundary)),
        ("profile", ctx.profile.clone()),
        ("account_ref", Json::str(ctx.account_ref.clone())),
        ("budget_ref", Json::str(ctx.budget_ref.clone())),
        ("envelope_ref", Json::str(ctx.envelope_ref.clone())),
        ("parameters", params_to_json(&ctx.parameters)),
        (
            "capabilities_available",
            Json::Arr(
                ctx.capabilities_available
                    .iter()
                    .map(|c| Json::str(c.clone()))
                    .collect(),
            ),
        ),
        (
            "steering",
            Json::obj([
                ("steer_mode", Json::str(ctx.steering.0.as_str())),
                ("concurrent_input", Json::str(ctx.steering.1.as_str())),
            ]),
        ),
    ])
}

/// Decode a `context` document (`None` on any unknown/mistyped member).
pub fn context_from_json(j: &Json) -> Option<ControlContext> {
    let st = j.get("steering")?;
    Some(ControlContext {
        process_ref: req_str(j, "process_ref")?.to_string(),
        plan: plan_nodes_from_json(j.get("plan")?)?,
        boundary: boundary_from_json(j.get("boundary")?)?,
        profile: j.get("profile").cloned().unwrap_or(Json::Null),
        account_ref: req_str(j, "account_ref")?.to_string(),
        budget_ref: req_str(j, "budget_ref")?.to_string(),
        envelope_ref: req_str(j, "envelope_ref")?.to_string(),
        parameters: params_from_json(j.get("parameters")?)?,
        capabilities_available: match j.get("capabilities_available")? {
            Json::Arr(a) => a
                .iter()
                .map(|v| v.as_str().map(str::to_string))
                .collect::<Option<Vec<String>>>()?,
            _ => return None,
        },
        steering: (
            SteerMode::parse(req_str(st, "steer_mode")?)?,
            ConcurrentInput::parse(req_str(st, "concurrent_input")?)?,
        ),
    })
}

// ── events (the observation projection) ───────────────────────────────

fn scope_to_json(s: &Scope) -> Json {
    let mut m = BTreeMap::new();
    for (k, v) in [
        ("turn_id", &s.turn_id),
        ("model_call_id", &s.model_call_id),
        ("tool_call_id", &s.tool_call_id),
        ("effect_id", &s.effect_id),
        ("child_run_id", &s.child_run_id),
        ("branch_id", &s.branch_id),
        ("component_call_id", &s.component_call_id),
    ] {
        if let Some(v) = v {
            m.insert(k.to_string(), Json::str(v.clone()));
        }
    }
    Json::Obj(m)
}

fn scope_from_json(j: &Json) -> Option<Scope> {
    let get = |k: &str| j.get(k).and_then(Json::as_str).map(str::to_string);
    Some(Scope {
        turn_id: get("turn_id"),
        model_call_id: get("model_call_id"),
        tool_call_id: get("tool_call_id"),
        effect_id: get("effect_id"),
        child_run_id: get("child_run_id"),
        branch_id: get("branch_id"),
        component_call_id: get("component_call_id"),
    })
}

/// The canonical `event` document — the observation projection
/// `{seq, class, scope, payload}` (the envelope stamps are host-owned).
pub fn event_to_json(ev: &EventEnvelope) -> Json {
    Json::obj([
        ("seq", Json::Int(ev.seq as i64)),
        ("class", Json::str(ev.class.clone())),
        ("scope", scope_to_json(&ev.scope)),
        ("payload", ev.payload.clone()),
    ])
}

/// Decode an `event` document into the envelope the contract's `observe`
/// takes — the doc carries exactly the members the strategy may read;
/// every other member is stamped by the host and synthesized here
/// (closed-world: unknown members are not consulted).
pub fn event_from_json(j: &Json) -> Option<EventEnvelope> {
    let class = req_str(j, "class")?.to_string();
    Some(EventEnvelope {
        event_id: format!("w{}", req_i64(j, "seq")?),
        run_id: String::new(),
        seq: req_i64(j, "seq")?.max(0) as u64,
        ts: String::new(),
        hlc: None,
        plane: EventPlane::of_class(&class).unwrap_or(EventPlane::Control),
        class,
        schema_version: 1,
        producer: Producer::kernel("wire"),
        participant_class: ParticipantClass::Native,
        observability_level: [ObservabilityLevel::Ledger].into_iter().collect(),
        durability: Durability::Ledger,
        scope: scope_from_json(j.get("scope").unwrap_or(&Json::Null))?,
        lease_generation: 0,
        parent_event_id: String::new(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: None,
        prev_hash: String::new(),
        payload: j.get("payload").cloned().unwrap_or(Json::Null),
        hash: String::new(),
    })
}

// ── cues ──────────────────────────────────────────────────────────────

fn human_input_to_json(h: &HumanInput) -> Json {
    match h {
        HumanInput::Steer { payload_ref } => Json::obj([
            ("kind", Json::str("steer")),
            ("payload_ref", Json::str(payload_ref.clone())),
        ]),
        HumanInput::FollowUp { payload_ref } => Json::obj([
            ("kind", Json::str("follow_up")),
            ("payload_ref", Json::str(payload_ref.clone())),
        ]),
        HumanInput::Approval { effect_id, allow } => Json::obj([
            ("kind", Json::str("approval")),
            ("effect_id", Json::str(effect_id.clone())),
            ("allow", Json::Bool(*allow)),
        ]),
        HumanInput::Interrupt => Json::obj([("kind", Json::str("interrupt"))]),
        HumanInput::ArtefactMark {
            artefact_id,
            delivery_id,
            signal,
        } => Json::obj([
            ("kind", Json::str("artefact_mark")),
            ("artefact_id", Json::str(artefact_id.clone())),
            ("delivery_id", Json::str(delivery_id.clone())),
            ("signal", Json::str(signal.clone())),
        ]),
    }
}

fn human_input_from_json(j: &Json) -> Option<HumanInput> {
    Some(match req_str(j, "kind")? {
        "steer" => HumanInput::Steer {
            payload_ref: req_str(j, "payload_ref")?.to_string(),
        },
        "follow_up" => HumanInput::FollowUp {
            payload_ref: req_str(j, "payload_ref")?.to_string(),
        },
        "approval" => HumanInput::Approval {
            effect_id: req_str(j, "effect_id")?.to_string(),
            allow: match j.get("allow")? {
                Json::Bool(b) => *b,
                _ => return None,
            },
        },
        "interrupt" => HumanInput::Interrupt,
        "artefact_mark" => HumanInput::ArtefactMark {
            artefact_id: req_str(j, "artefact_id")?.to_string(),
            delivery_id: req_str(j, "delivery_id")?.to_string(),
            signal: req_str(j, "signal")?.to_string(),
        },
        _ => return None,
    })
}

fn envelope_signal_to_json(s: &EnvelopeSignal) -> Json {
    match s {
        EnvelopeSignal::SoftThreshold { dimension } => Json::obj([
            ("kind", Json::str("soft_threshold")),
            ("dimension", Json::str(dimension.clone())),
        ]),
        EnvelopeSignal::Refused {
            decision_ref,
            reason,
        } => Json::obj([
            ("kind", Json::str("refused")),
            ("decision_ref", Json::str(decision_ref.clone())),
            ("reason", Json::str(reason.clone())),
        ]),
        EnvelopeSignal::RetryableError { class, attempt } => Json::obj([
            ("kind", Json::str("retryable_error")),
            ("class", Json::str(class.clone())),
            ("attempt", Json::Int(*attempt as i64)),
        ]),
        EnvelopeSignal::CancelRequested { by } => Json::obj([
            ("kind", Json::str("cancel_requested")),
            ("by", Json::str(by.clone())),
        ]),
    }
}

fn envelope_signal_from_json(j: &Json) -> Option<EnvelopeSignal> {
    Some(match req_str(j, "kind")? {
        "soft_threshold" => EnvelopeSignal::SoftThreshold {
            dimension: req_str(j, "dimension")?.to_string(),
        },
        "refused" => EnvelopeSignal::Refused {
            decision_ref: req_str(j, "decision_ref")?.to_string(),
            reason: req_str(j, "reason")?.to_string(),
        },
        "retryable_error" => EnvelopeSignal::RetryableError {
            class: req_str(j, "class")?.to_string(),
            attempt: req_i64(j, "attempt")?.max(0) as u64,
        },
        "cancel_requested" => EnvelopeSignal::CancelRequested {
            by: req_str(j, "by")?.to_string(),
        },
        _ => return None,
    })
}

fn woken_trigger_to_json(t: &WokenTrigger) -> Json {
    match t {
        WokenTrigger::Timer { at } => {
            Json::obj([("kind", Json::str("timer")), ("at", Json::str(at.clone()))])
        }
        WokenTrigger::Schedule {
            expression,
            timezone,
            kind,
        } => Json::obj([
            ("kind", Json::str("schedule")),
            ("expression", Json::str(expression.clone())),
            ("timezone", Json::str(timezone.clone())),
            ("schedule_kind", Json::str(kind.clone())),
        ]),
        WokenTrigger::PermissionDecided { permission_id } => Json::obj([
            ("kind", Json::str("permission_decided")),
            ("permission_id", Json::str(permission_id.clone())),
        ]),
        WokenTrigger::ChildTerminal { child_run_id } => Json::obj([
            ("kind", Json::str("child_terminal")),
            ("child_run_id", Json::str(child_run_id.clone())),
        ]),
        WokenTrigger::EffectTerminal { effect_id } => Json::obj([
            ("kind", Json::str("effect_terminal")),
            ("effect_id", Json::str(effect_id.clone())),
        ]),
        WokenTrigger::EnvironmentReady { handle } => Json::obj([
            ("kind", Json::str("environment_ready")),
            ("handle", Json::str(handle.clone())),
        ]),
        WokenTrigger::RetryDue { scope_id } => Json::obj([
            ("kind", Json::str("retry_due")),
            ("scope_id", Json::str(scope_id.clone())),
        ]),
        WokenTrigger::External { source_ref, filter } => Json::obj([
            ("kind", Json::str("external")),
            ("source_ref", Json::str(source_ref.clone())),
            ("filter", Json::str(filter.clone())),
        ]),
        WokenTrigger::Manual { principal } => Json::obj([
            ("kind", Json::str("manual")),
            ("principal", Json::str(principal.clone())),
        ]),
        WokenTrigger::PeerMessage { from } => Json::obj([
            ("kind", Json::str("peer_message")),
            ("from", Json::str(from.clone())),
        ]),
    }
}

fn woken_trigger_from_json(j: &Json) -> Option<WokenTrigger> {
    Some(match req_str(j, "kind")? {
        "timer" => WokenTrigger::Timer {
            at: req_str(j, "at")?.to_string(),
        },
        "schedule" => WokenTrigger::Schedule {
            expression: req_str(j, "expression")?.to_string(),
            timezone: req_str(j, "timezone")?.to_string(),
            kind: req_str(j, "schedule_kind")?.to_string(),
        },
        "permission_decided" => WokenTrigger::PermissionDecided {
            permission_id: req_str(j, "permission_id")?.to_string(),
        },
        "child_terminal" => WokenTrigger::ChildTerminal {
            child_run_id: req_str(j, "child_run_id")?.to_string(),
        },
        "effect_terminal" => WokenTrigger::EffectTerminal {
            effect_id: req_str(j, "effect_id")?.to_string(),
        },
        "environment_ready" => WokenTrigger::EnvironmentReady {
            handle: req_str(j, "handle")?.to_string(),
        },
        "retry_due" => WokenTrigger::RetryDue {
            scope_id: req_str(j, "scope_id")?.to_string(),
        },
        "external" => WokenTrigger::External {
            source_ref: req_str(j, "source_ref")?.to_string(),
            filter: req_str(j, "filter")?.to_string(),
        },
        "manual" => WokenTrigger::Manual {
            principal: req_str(j, "principal")?.to_string(),
        },
        "peer_message" => WokenTrigger::PeerMessage {
            from: req_str(j, "from")?.to_string(),
        },
        _ => return None,
    })
}

fn settled_outcome_to_json(o: &SettledOutcome) -> Json {
    match o {
        SettledOutcome::Observed { outcome } => Json::obj([
            ("kind", Json::str("observed")),
            ("outcome", Json::str(outcome.clone())),
        ]),
        SettledOutcome::Refused => Json::obj([("kind", Json::str("refused"))]),
        SettledOutcome::Unknown { cause } => Json::obj([
            ("kind", Json::str("unknown")),
            ("cause", Json::str(cause.clone())),
        ]),
        SettledOutcome::Abandoned => Json::obj([("kind", Json::str("abandoned"))]),
        SettledOutcome::Pending => Json::obj([("kind", Json::str("pending"))]),
    }
}

fn settled_outcome_from_json(j: &Json) -> Option<SettledOutcome> {
    Some(match req_str(j, "kind")? {
        "observed" => SettledOutcome::Observed {
            outcome: req_str(j, "outcome")?.to_string(),
        },
        "refused" => SettledOutcome::Refused,
        "unknown" => SettledOutcome::Unknown {
            cause: req_str(j, "cause")?.to_string(),
        },
        "abandoned" => SettledOutcome::Abandoned,
        "pending" => SettledOutcome::Pending,
        _ => return None,
    })
}

/// The canonical `cue` document (`decide` input — the closed sum).
pub fn cue_to_json(cue: &Cue) -> Json {
    let mut m = vec![("kind", Json::str(cue.kind()))];
    match cue {
        Cue::RunOpened { goal_ref, inputs } => {
            m.push(("goal_ref", Json::str(goal_ref.clone())));
            m.push(("inputs", inputs.clone()));
        }
        Cue::Resumed {
            last_durable,
            recovery_decision,
        } => {
            m.push(("last_durable", Json::Int(*last_durable as i64)));
            m.push(("recovery_decision", recovery_decision.clone()));
        }
        Cue::ModelCompleted {
            model_call_id,
            response_ref,
            stop_reason,
        } => {
            m.push(("model_call_id", Json::str(model_call_id.clone())));
            m.push(("response_ref", Json::str(response_ref.clone())));
            m.push(("stop_reason", Json::str(stop_reason.as_str())));
        }
        Cue::EffectsSettled {
            settled,
            all_terminal,
            submission_ref,
        } => {
            m.push((
                "settled",
                Json::Arr(
                    settled
                        .iter()
                        .map(|o| {
                            Json::obj([
                                ("effect_id", Json::str(o.effect_id.clone())),
                                ("outcome", settled_outcome_to_json(&o.outcome)),
                            ])
                        })
                        .collect(),
                ),
            ));
            m.push(("all_terminal", Json::Bool(*all_terminal)));
            m.push((
                "submission_ref",
                match submission_ref {
                    Some(r) => Json::str(r.clone()),
                    None => Json::Null,
                },
            ));
        }
        Cue::CompactionCompleted { view_hash } => {
            m.push(("view_hash", Json::str(view_hash.clone())));
        }
        Cue::DelegationCompleted {
            child_run_id,
            result_ref,
            outcome_class,
        } => {
            m.push(("child_run_id", Json::str(child_run_id.clone())));
            m.push(("result_ref", Json::str(result_ref.clone())));
            m.push(("outcome_class", Json::str(outcome_class.as_str())));
        }
        Cue::VerificationCompleted | Cue::RetrievalCompleted => {}
        Cue::HumanInput(h) => {
            m.push(("input", human_input_to_json(h)));
        }
        Cue::EnvelopeSignal(s) => {
            m.push(("signal", envelope_signal_to_json(s)));
        }
        Cue::GuardFired {
            decision_point,
            guard_id,
        } => {
            m.push((
                "decision_point",
                Json::str(decision_point_str(*decision_point)),
            ));
            m.push(("guard_id", Json::str(guard_id.clone())));
        }
        Cue::Woken {
            trigger,
            payload_ref,
            delivery_mode,
        } => {
            m.push(("trigger", woken_trigger_to_json(trigger)));
            m.push(("payload_ref", Json::str(payload_ref.clone())));
            m.push(("delivery_mode", Json::str(delivery_mode.as_str())));
        }
    }
    Json::Obj(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// Decode a `cue` document (`None` on an unknown kind or member).
pub fn cue_from_json(j: &Json) -> Option<Cue> {
    Some(match req_str(j, "kind")? {
        "run_opened" => Cue::RunOpened {
            goal_ref: req_str(j, "goal_ref")?.to_string(),
            inputs: j.get("inputs").cloned().unwrap_or(Json::Null),
        },
        "resumed" => Cue::Resumed {
            last_durable: req_i64(j, "last_durable")?.max(0) as u64,
            recovery_decision: j.get("recovery_decision").cloned().unwrap_or(Json::Null),
        },
        "model_completed" => Cue::ModelCompleted {
            model_call_id: req_str(j, "model_call_id")?.to_string(),
            response_ref: req_str(j, "response_ref")?.to_string(),
            stop_reason: hh_gateway::vocab::StopReason::parse(req_str(j, "stop_reason")?)?,
        },
        "effects_settled" => Cue::EffectsSettled {
            settled: match j.get("settled")? {
                Json::Arr(a) => a
                    .iter()
                    .map(|o| {
                        Some(EffectOutcome {
                            effect_id: req_str(o, "effect_id")?.to_string(),
                            outcome: settled_outcome_from_json(o.get("outcome")?)?,
                        })
                    })
                    .collect::<Option<Vec<EffectOutcome>>>()?,
                _ => return None,
            },
            all_terminal: matches!(j.get("all_terminal"), Some(Json::Bool(true))),
            submission_ref: j
                .get("submission_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
        },
        "verification_completed" => Cue::VerificationCompleted,
        "retrieval_completed" => Cue::RetrievalCompleted,
        "compaction_completed" => Cue::CompactionCompleted {
            view_hash: req_str(j, "view_hash")?.to_string(),
        },
        "delegation_completed" => Cue::DelegationCompleted {
            child_run_id: req_str(j, "child_run_id")?.to_string(),
            result_ref: req_str(j, "result_ref")?.to_string(),
            outcome_class: hh_ontology::control::OutcomeClass::parse(req_str(j, "outcome_class")?)?,
        },
        "human_input" => Cue::HumanInput(human_input_from_json(j.get("input")?)?),
        "envelope_signal" => Cue::EnvelopeSignal(envelope_signal_from_json(j.get("signal")?)?),
        "guard_fired" => Cue::GuardFired {
            decision_point: parse_decision_point(req_str(j, "decision_point")?)?,
            guard_id: req_str(j, "guard_id")?.to_string(),
        },
        "woken" => Cue::Woken {
            trigger: woken_trigger_from_json(j.get("trigger")?)?,
            payload_ref: req_str(j, "payload_ref")?.to_string(),
            delivery_mode: DeliveryMode::parse(req_str(j, "delivery_mode")?)?,
        },
        _ => return None,
    })
}

// ── decisions ─────────────────────────────────────────────────────────

fn retry_target_to_json(t: &RetryTarget) -> Json {
    match t {
        RetryTarget::ModelCall { model_call_id } => Json::obj([
            ("kind", Json::str("model_call")),
            ("model_call_id", Json::str(model_call_id.clone())),
        ]),
        RetryTarget::Effect { effect_id } => Json::obj([
            ("kind", Json::str("effect")),
            ("effect_id", Json::str(effect_id.clone())),
        ]),
        RetryTarget::PlanNode { node_id } => Json::obj([
            ("kind", Json::str("plan_node")),
            ("node_id", Json::str(node_id.clone())),
        ]),
        RetryTarget::Delegation { delegation_id } => Json::obj([
            ("kind", Json::str("delegation")),
            ("delegation_id", Json::str(delegation_id.clone())),
        ]),
    }
}

fn retry_target_from_json(j: &Json) -> Option<RetryTarget> {
    Some(match req_str(j, "kind")? {
        "model_call" => RetryTarget::ModelCall {
            model_call_id: req_str(j, "model_call_id")?.to_string(),
        },
        "effect" => RetryTarget::Effect {
            effect_id: req_str(j, "effect_id")?.to_string(),
        },
        "plan_node" => RetryTarget::PlanNode {
            node_id: req_str(j, "node_id")?.to_string(),
        },
        "delegation" => RetryTarget::Delegation {
            delegation_id: req_str(j, "delegation_id")?.to_string(),
        },
        _ => return None,
    })
}

fn escalate_ask_to_json(a: &EscalateAsk) -> Json {
    match a {
        EscalateAsk::Approval { effect_id } => Json::obj([
            ("kind", Json::str("approval")),
            ("effect_id", Json::str(effect_id.clone())),
        ]),
        EscalateAsk::Question { payload_ref } => Json::obj([
            ("kind", Json::str("question")),
            ("payload_ref", Json::str(payload_ref.clone())),
        ]),
        EscalateAsk::Handoff => Json::obj([("kind", Json::str("handoff"))]),
    }
}

fn escalate_ask_from_json(j: &Json) -> Option<EscalateAsk> {
    Some(match req_str(j, "kind")? {
        "approval" => EscalateAsk::Approval {
            effect_id: req_str(j, "effect_id")?.to_string(),
        },
        "question" => EscalateAsk::Question {
            payload_ref: req_str(j, "payload_ref")?.to_string(),
        },
        "handoff" => EscalateAsk::Handoff,
        _ => return None,
    })
}

fn wait_until_to_json(u: &WaitUntil) -> Json {
    match u {
        WaitUntil::CueKind { cue_kind } => Json::obj([
            ("kind", Json::str("cue_kind")),
            ("cue_kind", Json::str(cue_kind.clone())),
        ]),
        WaitUntil::Deadline { at_ms } => Json::obj([
            ("kind", Json::str("deadline")),
            ("at_ms", Json::Int(*at_ms as i64)),
        ]),
    }
}

fn wait_until_from_json(j: &Json) -> Option<WaitUntil> {
    Some(match req_str(j, "kind")? {
        "cue_kind" => WaitUntil::CueKind {
            cue_kind: req_str(j, "cue_kind")?.to_string(),
        },
        "deadline" => WaitUntil::Deadline {
            at_ms: req_i64(j, "at_ms")?.max(0) as u64,
        },
        _ => return None,
    })
}

/// The canonical `decision` document (`decide` output — the closed sum).
pub fn decision_to_json(d: &ControlDecision) -> Json {
    let mut m = vec![(
        "stamp",
        Json::obj([
            (
                "decision_point",
                Json::str(decision_point_str(d.stamp.decision_point)),
            ),
            ("owner", Json::str(owner_str(d.stamp.owner))),
            (
                "rationale_ref",
                match &d.stamp.rationale_ref {
                    Some(r) => Json::str(r.clone()),
                    None => Json::Null,
                },
            ),
        ]),
    )];
    m.push(("kind", Json::str(d.kind.as_str())));
    match &d.kind {
        DecisionKind::Propose {
            decision_point,
            context_request,
            expected_output,
        } => {
            m.push((
                "decision_point",
                Json::str(decision_point_str(*decision_point)),
            ));
            m.push(("context_request", context_request.clone()));
            m.push((
                "expected_output",
                match expected_output {
                    ExpectedOutput::Free => Json::obj([("kind", Json::str("free"))]),
                    ExpectedOutput::Schema { validator_ref } => Json::obj([
                        ("kind", Json::str("schema")),
                        ("validator_ref", Json::str(validator_ref.clone())),
                    ]),
                },
            ));
        }
        DecisionKind::Act {
            intents,
            mode,
            on_partial,
        } => {
            m.push(("intents", Json::Arr(intents.clone())));
            m.push(("mode", Json::str(mode.as_str())));
            m.push(("on_partial", Json::str(on_partial.as_str())));
        }
        DecisionKind::Retrieve { query } => {
            m.push(("query", query.clone()));
        }
        DecisionKind::Compact { reason } => {
            m.push(("reason", Json::str(reason.clone())));
        }
        DecisionKind::Verify {
            validator_refs,
            subject,
        } => {
            m.push((
                "validator_refs",
                Json::Arr(
                    validator_refs
                        .iter()
                        .map(|r| Json::str(r.clone()))
                        .collect(),
                ),
            ));
            m.push(("subject", subject.clone()));
        }
        DecisionKind::Delegate {
            spec,
            budget_slice,
            permissions,
            delegation_reason,
        } => {
            m.push(("spec", spec.clone()));
            m.push(("budget_slice", budget_slice.clone()));
            m.push(("permissions", permissions.clone()));
            m.push((
                "delegation_reason",
                delegation_reason.clone().unwrap_or(Json::Null),
            ));
        }
        DecisionKind::Retry {
            target,
            attempt,
            not_before,
        } => {
            m.push(("target", retry_target_to_json(target)));
            m.push(("attempt", Json::Int(*attempt as i64)));
            m.push((
                "not_before",
                match not_before {
                    Some(t) => Json::Int(*t as i64),
                    None => Json::Null,
                },
            ));
        }
        DecisionKind::Escalate { ask } => {
            m.push(("ask", escalate_ask_to_json(ask)));
        }
        DecisionKind::Wait { until } => {
            m.push(("until", wait_until_to_json(until)));
        }
        DecisionKind::Stop {
            proposed_reason,
            submission_ref,
        } => {
            m.push(("proposed_reason", proposed_reason.to_json()));
            m.push((
                "submission_ref",
                match submission_ref {
                    Some(r) => Json::str(r.clone()),
                    None => Json::Null,
                },
            ));
        }
    }
    Json::Obj(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// Decode a `decision` document (`None` on an unknown kind or member).
pub fn decision_from_json(j: &Json) -> Option<ControlDecision> {
    let st = j.get("stamp")?;
    let stamp = DecisionStamp {
        decision_point: parse_decision_point(req_str(st, "decision_point")?)?,
        owner: parse_owner(req_str(st, "owner")?)?,
        rationale_ref: st
            .get("rationale_ref")
            .and_then(Json::as_str)
            .map(str::to_string),
    };
    let kind = match req_str(j, "kind")? {
        "propose" => DecisionKind::Propose {
            decision_point: parse_decision_point(req_str(j, "decision_point")?)?,
            context_request: j.get("context_request").cloned().unwrap_or(Json::Null),
            expected_output: match req_str(j.get("expected_output")?, "kind")? {
                "free" => ExpectedOutput::Free,
                "schema" => ExpectedOutput::Schema {
                    validator_ref: req_str(j.get("expected_output")?, "validator_ref")?.to_string(),
                },
                _ => return None,
            },
        },
        "act" => DecisionKind::Act {
            intents: match j.get("intents")? {
                Json::Arr(a) => a.clone(),
                _ => return None,
            },
            mode: act_mode_parse(req_str(j, "mode")?)?,
            on_partial: OnPartial::parse(req_str(j, "on_partial")?)?,
        },
        "retrieve" => DecisionKind::Retrieve {
            query: j.get("query").cloned().unwrap_or(Json::Null),
        },
        "compact" => DecisionKind::Compact {
            reason: req_str(j, "reason")?.to_string(),
        },
        "verify" => DecisionKind::Verify {
            validator_refs: match j.get("validator_refs")? {
                Json::Arr(a) => a
                    .iter()
                    .map(|v| v.as_str().map(str::to_string))
                    .collect::<Option<Vec<String>>>()?,
                _ => return None,
            },
            subject: j.get("subject").cloned().unwrap_or(Json::Null),
        },
        "delegate" => DecisionKind::Delegate {
            spec: j.get("spec").cloned().unwrap_or(Json::Null),
            budget_slice: j.get("budget_slice").cloned().unwrap_or(Json::Null),
            permissions: j.get("permissions").cloned().unwrap_or(Json::Null),
            delegation_reason: j.get("delegation_reason").cloned(),
        },
        "retry" => DecisionKind::Retry {
            target: retry_target_from_json(j.get("target")?)?,
            attempt: req_i64(j, "attempt")?.max(0) as u64,
            not_before: j
                .get("not_before")
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64),
        },
        "escalate" => DecisionKind::Escalate {
            ask: escalate_ask_from_json(j.get("ask")?)?,
        },
        "wait" => DecisionKind::Wait {
            until: wait_until_from_json(j.get("until")?)?,
        },
        "stop" => DecisionKind::Stop {
            proposed_reason: StopReason::from_json(j.get("proposed_reason")?)?,
            submission_ref: j
                .get("submission_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
        },
        _ => return None,
    };
    Some(ControlDecision { stamp, kind })
}

// ── reports / errors ──────────────────────────────────────────────────

/// The canonical `report` document (`terminate` output).
pub fn report_to_json(r: &FinalReport) -> Json {
    Json::obj([
        ("stop_reason", r.stop_reason.to_json()),
        (
            "submission_ref",
            match &r.submission_ref {
                Some(s) => Json::str(s.clone()),
                None => Json::Null,
            },
        ),
        (
            "unresolved_effects",
            Json::Arr(
                r.unresolved_effects
                    .iter()
                    .map(|e| Json::str(e.clone()))
                    .collect(),
            ),
        ),
        (
            "decisions",
            Json::Arr(r.decisions.iter().map(|e| Json::str(e.clone())).collect()),
        ),
        (
            "boundary_observed",
            Json::Obj(
                r.boundary_observed
                    .iter()
                    .map(|(p, o)| {
                        (
                            decision_point_str(*p).to_string(),
                            Json::str(owner_str(*o).to_string()),
                        )
                    })
                    .collect(),
            ),
        ),
    ])
}

/// Decode a `report` document.
pub fn report_from_json(j: &Json) -> Option<FinalReport> {
    let mut observed = BTreeMap::new();
    if let Some(Json::Obj(m)) = j.get("boundary_observed") {
        for (k, v) in m {
            observed.insert(parse_decision_point(k)?, parse_owner(v.as_str()?)?);
        }
    }
    Some(FinalReport {
        stop_reason: StopReason::from_json(j.get("stop_reason")?)?,
        submission_ref: j
            .get("submission_ref")
            .and_then(Json::as_str)
            .map(str::to_string),
        unresolved_effects: match j.get("unresolved_effects") {
            Some(Json::Arr(a)) => a
                .iter()
                .map(|v| v.as_str().map(str::to_string))
                .collect::<Option<Vec<String>>>()?,
            _ => vec![],
        },
        decisions: match j.get("decisions") {
            Some(Json::Arr(a)) => a
                .iter()
                .map(|v| v.as_str().map(str::to_string))
                .collect::<Option<Vec<String>>>()?,
            _ => vec![],
        },
        boundary_observed: observed,
    })
}

/// The `stop_reason` doc — the ontology `StopReason` codec re-spelled as a
/// member (the sum is `hh_ontology::control`'s — CC10).
pub fn stop_reason_to_json(r: &StopReason) -> Json {
    r.to_json()
}

/// Decode a `stop_reason` member (`None` on a malformed/unknown member).
pub fn stop_reason_from_json(j: &Json) -> Option<StopReason> {
    StopReason::from_json(j)
}

/// `cancelled_by` spellings — the ontology `CancelledBy` codec re-spelled
/// as a member.
pub fn cancel_by_parse(s: &str) -> Option<CancelledBy> {
    CancelledBy::parse(s)
}

/// `open`'s typed refusal document — `{error: "<spelling>"}`.
pub fn control_error_to_json(e: &ControlError) -> Json {
    let (kind, detail) = match e {
        ControlError::IncompatibleBoundary(b) => (
            "incompatible_boundary".to_string(),
            format!("{}:{}", decision_point_str(b.point), owner_str(b.owner)),
        ),
        ControlError::MissingProcedure => ("missing_procedure".to_string(), String::new()),
        ControlError::UnsupportedPlanNode { node } => {
            ("unsupported_plan_node".to_string(), node.clone())
        }
        ControlError::UnhandledCue { cue_kind } => ("unhandled_cue".to_string(), cue_kind.clone()),
    };
    Json::obj([("error", Json::str(kind)), ("detail", Json::str(detail))])
}

/// `restore`'s typed refusal document.
pub fn restore_error_to_json(e: &RestoreError) -> Json {
    let (kind, detail) = match e {
        RestoreError::VariantMismatch {
            checkpoint,
            restoring,
        } => (
            "variant_mismatch".to_string(),
            format!("{checkpoint}≠{restoring}"),
        ),
        RestoreError::DialectMismatch { checkpoint } => {
            ("dialect_mismatch".to_string(), checkpoint.clone())
        }
        RestoreError::Malformed => ("malformed_checkpoint".to_string(), String::new()),
    };
    Json::obj([("error", Json::str(kind)), ("detail", Json::str(detail))])
}

/// Whether a document is a typed refusal (`{error: …}`).
pub fn is_error_doc(j: &Json) -> bool {
    j.get("error").and_then(Json::as_str).is_some()
}

/// The `state` doc — `ControlState`'s canonical checkpoint JSON (the
/// `ControlState/1` dialect — the codec is `state.rs`'s, CC1).
pub fn state_to_json(s: &ControlState) -> Json {
    s.to_json()
}

/// Decode a `state` doc (`None` when the checkpoint does not decode).
pub fn state_from_json(j: &Json) -> Option<ControlState> {
    ControlState::from_json(j)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::{ConcurrentInput, SteerMode};
    use hh_ontology::control::{DecisionPoint, Owner};

    fn rt(j: &Json, f: impl Fn(&Json) -> bool, what: &str) {
        assert!(f(j), "round trip failed for {what}: {j:?}");
    }

    #[test]
    fn cue_documents_round_trip_the_full_sum() {
        let cues = vec![
            Cue::RunOpened {
                goal_ref: "g".into(),
                inputs: Json::obj([("x", Json::Int(1))]),
            },
            Cue::Resumed {
                last_durable: 9,
                recovery_decision: Json::Null,
            },
            Cue::ModelCompleted {
                model_call_id: "m".into(),
                response_ref: "r".into(),
                stop_reason: hh_gateway::vocab::StopReason::ToolUse,
            },
            Cue::EffectsSettled {
                settled: vec![
                    EffectOutcome {
                        effect_id: "e1".into(),
                        outcome: SettledOutcome::Observed {
                            outcome: "applied".into(),
                        },
                    },
                    EffectOutcome {
                        effect_id: "e2".into(),
                        outcome: SettledOutcome::Unknown {
                            cause: "timeout".into(),
                        },
                    },
                ],
                all_terminal: true,
                submission_ref: Some("sub".into()),
            },
            Cue::VerificationCompleted,
            Cue::RetrievalCompleted,
            Cue::CompactionCompleted {
                view_hash: "vh".into(),
            },
            Cue::DelegationCompleted {
                child_run_id: "c".into(),
                result_ref: "rr".into(),
                outcome_class: hh_ontology::control::OutcomeClass::Scored,
            },
            Cue::HumanInput(HumanInput::Steer {
                payload_ref: "p".into(),
            }),
            Cue::HumanInput(HumanInput::FollowUp {
                payload_ref: "f".into(),
            }),
            Cue::HumanInput(HumanInput::Approval {
                effect_id: "e".into(),
                allow: false,
            }),
            Cue::HumanInput(HumanInput::Interrupt),
            Cue::HumanInput(HumanInput::ArtefactMark {
                artefact_id: "a".into(),
                delivery_id: "d".into(),
                signal: "cited".into(),
            }),
            Cue::EnvelopeSignal(EnvelopeSignal::CancelRequested {
                by: "principal".into(),
            }),
            Cue::EnvelopeSignal(EnvelopeSignal::RetryableError {
                class: "timeout".into(),
                attempt: 2,
            }),
            Cue::GuardFired {
                decision_point: DecisionPoint::Stop,
                guard_id: "completion_refused".into(),
            },
            Cue::Woken {
                trigger: WokenTrigger::Manual {
                    principal: "op".into(),
                },
                payload_ref: "steer-1".into(),
                delivery_mode: DeliveryMode::Steer,
            },
        ];
        for c in cues {
            let j = cue_to_json(&c);
            let back = cue_from_json(&j);
            assert_eq!(back.as_ref(), Some(&c), "cue {j:?}");
        }
    }

    #[test]
    fn decision_documents_round_trip_the_full_sum() {
        let decisions = vec![
            DecisionKind::Propose {
                decision_point: DecisionPoint::Plan,
                context_request: Json::obj([("steer_ref", Json::str("p"))]),
                expected_output: ExpectedOutput::Free,
            },
            DecisionKind::Propose {
                decision_point: DecisionPoint::Plan,
                context_request: Json::Null,
                expected_output: ExpectedOutput::Schema {
                    validator_ref: "v".into(),
                },
            },
            DecisionKind::Act {
                intents: vec![Json::obj([("tool_call_id", Json::str("t"))])],
                mode: ActMode::Parallel,
                on_partial: OnPartial::ContinueBatch,
            },
            DecisionKind::Retrieve { query: Json::Null },
            DecisionKind::Compact {
                reason: "compaction_required".into(),
            },
            DecisionKind::Verify {
                validator_refs: vec!["v1".into()],
                subject: Json::str("s"),
            },
            DecisionKind::Delegate {
                spec: Json::Null,
                budget_slice: Json::Null,
                permissions: Json::Null,
                delegation_reason: Some(Json::str("r")),
            },
            DecisionKind::Retry {
                target: RetryTarget::PlanNode {
                    node_id: "n".into(),
                },
                attempt: 2,
                not_before: Some(5),
            },
            DecisionKind::Escalate {
                ask: EscalateAsk::Handoff,
            },
            DecisionKind::Wait {
                until: WaitUntil::Deadline { at_ms: 7 },
            },
            DecisionKind::Stop {
                proposed_reason: StopReason::Cancelled {
                    by: CancelledBy::Principal,
                },
                submission_ref: None,
            },
        ];
        for kind in decisions {
            let d = ControlDecision {
                stamp: DecisionStamp {
                    decision_point: DecisionPoint::Act,
                    owner: Owner::Code,
                    rationale_ref: None,
                },
                kind,
            };
            let j = decision_to_json(&d);
            rt(
                &j,
                |x| decision_from_json(x).as_ref() == Some(&d),
                "decision",
            );
        }
    }

    #[test]
    fn context_documents_round_trip() {
        let ctx = ControlContext {
            process_ref: "proc".into(),
            plan: vec![PlanNode {
                node_id: "n1".into(),
                hir_node_id: "h1".into(),
                hir_version_id: "hv1".into(),
                payload: PlanNodePayload::Step(StepNode {
                    mode: StepMode::Sequential,
                    output_schema: None,
                    action: StepAction::Instruction {
                        content_hash: "ch".into(),
                        owner: "system".into(),
                    },
                }),
            }],
            boundary: ControlBoundary::default(),
            profile: Json::Null,
            account_ref: "a".into(),
            budget_ref: "b".into(),
            envelope_ref: "e".into(),
            parameters: StrategyParams::default(),
            capabilities_available: vec!["retrieve".into()],
            steering: (SteerMode::QueueNextTurn, ConcurrentInput::Steer),
        };
        let j = context_to_json(&ctx);
        let back = context_from_json(&j).expect("ctx decodes");
        assert_eq!(back.process_ref, "proc");
        assert_eq!(back.plan.len(), 1);
        assert_eq!(
            back.steering,
            (SteerMode::QueueNextTurn, ConcurrentInput::Steer)
        );
    }
}
