//! The `react/minimal` control loop + the envelope's driver subset (R-2.6.1, R-2.6.2; §5e.1–2).
//! **Throwaway Stage-0 subset.**
//!
//! §9.1 R-2.6.1/R-2.6.2 slice: "`react/minimal` (mini-SWE-agent class; the T-LCD-03 anchor);
//! the envelope's driver subset — ceilings, transport retry, tool timeout with kill, parse
//! refusal". `react/steerable`, nudges and the full `StopReason`/`ExhaustionPolicy` land at
//! S1.20/S2.11; this is the ceilings-plus-retry driver.
//!
//! The loop is the seam that makes CC3 concrete: **every** producing event posts a charge to
//! the one root budget before the next decision-point `check`, so no producing event is
//! unaccounted and no intent is issued past exhaustion (AC-R-2.1.6-1 "no dangling intent").

use std::time::Duration;

use hh_wire::json::Json;

use crate::budget::{BudgetNode, Dimension, Exhausted};
use crate::context::ContextPolicy;
use crate::env::EnvHandle;
use crate::gateway::{Gateway, Model, StubOutcome};
use crate::hir::HarnessDefinition;
use crate::tools::{ToolExecutor, ToolResult};
use crate::trace::{Scope, Trace};

/// Why the run stopped. The exit-class mapping ([`crate::driver::ExitClass::of`]) is total over
/// this enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// The model declared the task done.
    Completed,
    /// A hard budget ceiling was reached at a decision point.
    BudgetExhausted { dimension: Dimension },
    /// The assembled context exceeded the window.
    ContextWindowExceeded,
    /// The model turn could not be parsed into an action.
    ParseRefused,
    /// A tool exceeded its deadline and was killed.
    ToolTimeout,
    /// Transport kept failing after the retry budget.
    TransportFailed,
}

impl StopReason {
    pub fn as_str(&self) -> String {
        match self {
            StopReason::Completed => "completed".into(),
            StopReason::BudgetExhausted { dimension } => {
                format!("budget_exhausted{{{}}}", dimension.as_str())
            }
            StopReason::ContextWindowExceeded => "context_window_exceeded".into(),
            StopReason::ParseRefused => "parse_refused".into(),
            StopReason::ToolTimeout => "tool_timeout".into(),
            StopReason::TransportFailed => "transport_failed".into(),
        }
    }
}

/// The driver subset of the control envelope (R-2.6.2): the ceilings are the budget; these are
/// the remaining knobs.
#[derive(Debug, Clone, Copy)]
pub struct Envelope {
    /// Retries for a transient transport failure before giving up.
    pub transport_retries: u32,
    /// Per-tool-call deadline (the executor kills a process that exceeds it).
    pub tool_timeout: Duration,
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            transport_retries: 1,
            tool_timeout: Duration::from_secs(10),
        }
    }
}

/// The result of one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutcome {
    pub run_id: String,
    pub stop_reason: StopReason,
    pub turns: i64,
    pub final_answer: Option<String>,
}

/// Run the react/minimal loop end-to-end, writing every event to `trace` and charging every
/// producing event to `budget`. The definition is assumed already `validate`d by the driver.
#[allow(clippy::too_many_arguments)]
pub fn run<M: Model>(
    run_id: &str,
    def: &HarnessDefinition,
    budget: &mut BudgetNode,
    gateway: &mut Gateway<M>,
    executor: &ToolExecutor,
    _env: &EnvHandle,
    context: &ContextPolicy,
    envelope: &Envelope,
    trace: &mut Trace,
) -> std::io::Result<RunOutcome> {
    trace.append(
        "lifecycle.run.created",
        Scope::Run,
        Json::obj([
            ("run_id", Json::str(run_id)),
            ("class", Json::str(def.class.clone())),
            ("principal", def.principal.to_json()),
            ("definition", def.definition.to_json()),
        ]),
    )?;

    let system = "You are react/minimal. Use tools to accomplish the task, then answer.";
    let tools_desc = "shell(command), write_file(path,content), read_file(path)";
    let mut history: Vec<String> = Vec::new();
    let mut last_obs: Option<ToolResult> = None;
    let mut turns: i64 = 0;

    // The loop is bounded by the model_calls ceiling: after that many calls the decision-point
    // check exhausts the budget. The `+ 2` margin only ever runs the terminating check.
    let safety = budget.ceiling(Dimension::ModelCalls) + 2;

    for _ in 0..safety {
        // Decision point 1: budget check before committing to a turn (no dangling intent).
        if let Err(Exhausted { dimension }) = budget.check() {
            return finish(
                trace,
                run_id,
                StopReason::BudgetExhausted { dimension },
                turns,
            );
        }

        // Assemble the six-slot context; a window overflow stops the run.
        let assembled = match context.assemble(
            system,
            &def.task,
            "",
            &history,
            last_obs.as_ref(),
            tools_desc,
        ) {
            Ok(a) => a,
            Err(_) => return finish(trace, run_id, StopReason::ContextWindowExceeded, turns),
        };
        trace.append(
            "context.assembled",
            Scope::Control,
            Json::obj([
                ("context_label", Json::str(assembled.context_label.as_str())),
                ("char_len", Json::Int(assembled.char_len as i64)),
            ]),
        )?;

        // The model call, with the transport-retry subset.
        let mut attempt = 0u32;
        let (decision, response) = loop {
            match gateway.call("driver", &assembled.prompt, last_obs.as_ref()) {
                Ok((d, StubOutcome::Response(r))) => break (d, r),
                Ok((_, StubOutcome::Unparseable { .. })) => {
                    return finish(trace, run_id, StopReason::ParseRefused, turns);
                }
                Ok((_, StubOutcome::TransportError { detail })) => {
                    trace.append(
                        "model.call.attempt.failed",
                        Scope::ModelCall,
                        Json::obj([
                            ("error", Json::str(detail)),
                            ("attempt", Json::Int(attempt as i64)),
                        ]),
                    )?;
                    if attempt >= envelope.transport_retries {
                        return finish(trace, run_id, StopReason::TransportFailed, turns);
                    }
                    attempt += 1;
                }
                Err(e) => {
                    // No route (misconfiguration) — treated as a transport-class failure.
                    trace.append(
                        "model.call.attempt.failed",
                        Scope::ModelCall,
                        Json::obj([("error", Json::str(format!("{e:?}")))]),
                    )?;
                    return finish(trace, run_id, StopReason::TransportFailed, turns);
                }
            }
        };
        turns += 1;

        // AC-R-2.3.2-1: exactly one route.decided per completed model call.
        trace.append(
            "model.route.decided",
            Scope::ModelCall,
            Json::obj([
                ("selected", Json::str(decision.selected.clone())),
                (
                    "candidates_considered",
                    Json::Arr(
                        decision
                            .candidates_considered
                            .iter()
                            .cloned()
                            .map(Json::str)
                            .collect(),
                    ),
                ),
                ("deviation", Json::Bool(decision.deviation)),
                ("reservation_id", Json::str(decision.reservation_id.clone())),
                ("policy_kind", Json::str(gateway.policy().kind())),
            ]),
        )?;
        // Charge the model call (usage + one model_call) and account it.
        let blended = response.usage.blended();
        budget.charge(Dimension::TokensBlended, blended);
        budget.charge(Dimension::ModelCalls, 1);
        trace.append(
            "model.call.completed",
            Scope::ModelCall,
            Json::obj([
                ("model_ref", Json::str(decision.selected.clone())),
                ("input_tokens", Json::Int(response.usage.input_tokens)),
                ("output_tokens", Json::Int(response.usage.output_tokens)),
                ("blended", Json::Int(blended)),
            ]),
        )?;
        charge_event(trace, Dimension::TokensBlended, blended)?;
        charge_event(trace, Dimension::ModelCalls, 1)?;
        history.push(format!("assistant: {}", response.content));

        // If the model wants a tool, run it — but re-check the budget first so no tool intent
        // is committed past exhaustion (AC-R-2.1.6-1 "no dangling intent").
        if let Some(call) = response.tool_call.clone() {
            if let Err(Exhausted { dimension }) = budget.check() {
                return finish(
                    trace,
                    run_id,
                    StopReason::BudgetExhausted { dimension },
                    turns,
                );
            }
            trace.append(
                "action.tool.proposed",
                Scope::ToolCall,
                Json::obj([("tool", Json::str(call.name()))]),
            )?;
            // Headless is `unattended` with `approval_mode = unattended_deny`: the permission
            // is decided by policy, never a human, and never blocks on stdin (AC-R-2.11.1-6).
            // The Stage-0 baseline tools are in the auto set, so the policy decision is `allow`;
            // an `ask`-class tool would be `deny` (see `crate::driver::convert_ask`).
            trace.append(
                "security.permission.decided",
                Scope::Control,
                Json::obj([
                    ("decider", Json::str("policy")),
                    ("reason", Json::str("unattended")),
                    ("decision", Json::str("allow")),
                    ("tool", Json::str(call.name())),
                ]),
            )?;
            match executor.execute(call) {
                Ok(result) => {
                    budget.charge(Dimension::TimeWallMs, result.executor_ms as i64);
                    trace.append(
                        "action.tool.completed",
                        Scope::ToolCall,
                        Json::obj([
                            ("tool", Json::str(result.tool.clone())),
                            (
                                "observation_bytes",
                                Json::Int(result.observation_bytes as i64),
                            ),
                            ("executor_ms", Json::Int(result.executor_ms as i64)),
                        ]),
                    )?;
                    charge_event(trace, Dimension::TimeWallMs, result.executor_ms as i64)?;
                    history.push(format!("observation: {}", result.output));
                    last_obs = Some(result);
                }
                Err(crate::tools::ToolError::Timeout { .. }) => {
                    return finish(trace, run_id, StopReason::ToolTimeout, turns);
                }
                Err(e) => {
                    trace.append(
                        "action.tool.rejected",
                        Scope::ToolCall,
                        Json::obj([("error", Json::str(format!("{e:?}")))]),
                    )?;
                    history.push(format!("observation: tool error {e:?}"));
                    last_obs = None;
                }
            }
        }

        if response.done {
            return finish_with_answer(
                trace,
                run_id,
                StopReason::Completed,
                turns,
                response.content,
            );
        }
    }

    // Reached the safety bound without a terminal state: the model_calls ceiling is the cause.
    finish(
        trace,
        run_id,
        StopReason::BudgetExhausted {
            dimension: Dimension::ModelCalls,
        },
        turns,
    )
}

fn charge_event(trace: &mut Trace, dim: Dimension, amount: i64) -> std::io::Result<()> {
    trace.append(
        "control.budget.consumed",
        Scope::Control,
        Json::obj([
            ("dimension", Json::str(dim.as_str())),
            ("amount", Json::Int(amount)),
        ]),
    )?;
    Ok(())
}

fn finish(
    trace: &mut Trace,
    run_id: &str,
    stop: StopReason,
    turns: i64,
) -> std::io::Result<RunOutcome> {
    finish_inner(trace, run_id, stop, turns, None)
}

fn finish_with_answer(
    trace: &mut Trace,
    run_id: &str,
    stop: StopReason,
    turns: i64,
    answer: String,
) -> std::io::Result<RunOutcome> {
    finish_inner(trace, run_id, stop, turns, Some(answer))
}

fn finish_inner(
    trace: &mut Trace,
    run_id: &str,
    stop: StopReason,
    turns: i64,
    answer: Option<String>,
) -> std::io::Result<RunOutcome> {
    trace.append(
        "lifecycle.run.finished",
        Scope::Run,
        Json::obj([
            ("run_id", Json::str(run_id)),
            ("stop_reason", Json::str(stop.as_str())),
            ("turns", Json::Int(turns)),
        ]),
    )?;
    Ok(RunOutcome {
        run_id: run_id.to_string(),
        stop_reason: stop,
        turns,
        final_answer: answer,
    })
}
