//! End-to-end integration tests for the throwaway Stage-0 baseline (ticket S0.2).
//!
//! Drives the library seams (definition → budget → gateway → tool executor → trace) the way
//! the CLI does, and asserts the deterministic acceptance criteria that own the run:
//!   - AC-R-2.1.6-1  root budget ceilings + `check` + `budget_exhausted{dimension}` (no dangling intent)
//!   - AC-R-2.3.2-1  one static `model.route.decided` per completed call; `model_set_realized` == table
//!   - AC-R-2.5.5-14 one sandboxed tool call through the executor boundary
//!   - AC-R-2.8.3-1  env dump/proc-env masked; `leak_scan(run) = ∅` (allow-list half)
//!
//! …plus the "what works at the boundary" narrative: one hand-authored definition runs one
//! headless coding task end to end.

use std::collections::BTreeMap;
use std::time::Duration;

use hh_baseline::budget::{BudgetNode, Dimension};
use hh_baseline::context::ContextPolicy;
use hh_baseline::control::{run, Envelope, StopReason};
use hh_baseline::env::EnvHandle;
use hh_baseline::gateway::{Gateway, ModelResponse, RoutePolicy, StubModel, StubOutcome, Usage};
use hh_baseline::hir::baseline_definition;
use hh_baseline::secrets::{AllowList, HeldSecret, SecretRef};
use hh_baseline::tools::{ToolCall, ToolExecutor};
use hh_baseline::trace::Trace;

fn tmp(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("hh-baseline-e2e-{tag}-{}", std::process::id()))
}

fn cred() -> HeldSecret {
    HeldSecret::new(
        SecretRef::new("API_TOKEN", "gateway credential"),
        "sk-baseline-secret",
    )
}

fn ready_env(root: std::path::PathBuf) -> EnvHandle {
    let mut host = BTreeMap::new();
    host.insert("PATH".into(), "/usr/bin:/bin".into());
    let mut env = EnvHandle::declare(root, AllowList::new().allow("PATH"), vec![cred()], host);
    env.ready().unwrap();
    env
}

/// The scripted offline model that accomplishes the coding task: write, read, answer.
fn task_script() -> Vec<StubOutcome> {
    vec![
        StubOutcome::Response(ModelResponse {
            content: "create the file".into(),
            tool_call: Some(ToolCall::WriteFile {
                path: "hello.txt".into(),
                content: "hi".into(),
            }),
            usage: Usage {
                input_tokens: 20,
                output_tokens: 8,
            },
            done: false,
        }),
        StubOutcome::Response(ModelResponse {
            content: "read it back".into(),
            tool_call: Some(ToolCall::ReadFile {
                path: "hello.txt".into(),
            }),
            usage: Usage {
                input_tokens: 22,
                output_tokens: 6,
            },
            done: false,
        }),
        StubOutcome::Response(ModelResponse {
            content: "done".into(),
            tool_call: None,
            usage: Usage {
                input_tokens: 25,
                output_tokens: 12,
            },
            done: true,
        }),
    ]
}

#[test]
fn one_headless_coding_task_runs_end_to_end() {
    let def = baseline_definition("write hello.txt and read it back");
    assert!(def.validate().is_ok());

    let env = ready_env(tmp("e2e"));
    let mut budget = BudgetNode::root(1000, 10, 60_000);
    let mut gw = Gateway::new(
        def.role_table.clone(),
        RoutePolicy::Static,
        StubModel::new(task_script()),
        cred(),
    );
    let executor = ToolExecutor::new(&env, Duration::from_secs(10));
    let policy = ContextPolicy::default_policy(100_000);
    let mut trace = Trace::open(tmp("e2e").join("trace.jsonl")).unwrap();

    let outcome = run(
        "r-e2e",
        &def,
        &mut budget,
        &mut gw,
        &executor,
        &env,
        &policy,
        &Envelope::default(),
        &mut trace,
    )
    .unwrap();

    // The task completed cleanly.
    assert_eq!(outcome.stop_reason, StopReason::Completed);
    assert_eq!(outcome.turns, 3);
    assert!(outcome.final_answer.unwrap().contains("done"));

    // The file was really created under the sandbox root by the tool executor (AC-R-2.5.5-14).
    assert!(env.root().join("hello.txt").exists());

    // AC-R-2.3.2-1: exactly one `model.route.decided` per completed model call, each static.
    let route_events: Vec<_> = trace
        .events()
        .iter()
        .filter(|e| e.event == "model.route.decided")
        .collect();
    let call_events = trace
        .events()
        .iter()
        .filter(|e| e.event == "model.call.completed")
        .count();
    assert_eq!(route_events.len(), call_events);
    assert_eq!(route_events.len(), 3);
    for e in &route_events {
        assert_eq!(
            e.payload.get("deviation"),
            Some(&hh_wire::json::Json::Bool(false))
        );
        assert!(e.payload.get("reservation_id").is_some());
        let cands = e.payload.get("candidates_considered").unwrap();
        assert_eq!(
            cands,
            &hh_wire::json::Json::Arr(vec![hh_wire::json::Json::str("stub/model-A")])
        );
    }
    assert_eq!(gw.model_set_realized(), gw.configured_model_set());

    // CC3: nothing unaccounted — charged totals equal the cost_view.
    assert_eq!(trace.charged_totals(), trace.cost_view(&budget));
    // and every model call charged tokens + a model_call
    assert_eq!(budget.consumed(Dimension::ModelCalls), 3);
    assert_eq!(
        budget.consumed(Dimension::TokensBlended),
        20 + 8 + 22 + 6 + 25 + 12
    );
}

#[test]
fn budget_exhaustion_stops_with_typed_dimension_and_no_dangling_intent() {
    // AC-R-2.1.6-1: a run over its ceiling ends `budget_exhausted{dimension}` and no dangling
    // intent — the tool for the turn that would exceed the ceiling is never proposed.
    let def = baseline_definition("write and read");
    let env = ready_env(tmp("exhaust"));
    // model_calls ceiling of 1: the second turn's decision-point check exhausts.
    let mut budget = BudgetNode::root(1000, 1, 60_000);
    let mut gw = Gateway::new(
        def.role_table.clone(),
        RoutePolicy::Static,
        StubModel::new(task_script()),
        cred(),
    );
    let executor = ToolExecutor::new(&env, Duration::from_secs(10));
    let policy = ContextPolicy::default_policy(100_000);
    let mut trace = Trace::open(tmp("exhaust").join("trace.jsonl")).unwrap();

    let outcome = run(
        "r-ex",
        &def,
        &mut budget,
        &mut gw,
        &executor,
        &env,
        &policy,
        &Envelope::default(),
        &mut trace,
    )
    .unwrap();

    assert_eq!(
        outcome.stop_reason,
        StopReason::BudgetExhausted {
            dimension: Dimension::ModelCalls
        }
    );
    // Exactly one model call happened before exhaustion.
    assert_eq!(budget.consumed(Dimension::ModelCalls), 1);
    // No dangling intent: the run finished with a terminal event and the last non-finish event
    // is not an un-completed tool proposal past exhaustion.
    let finished = trace.events().last().unwrap();
    assert_eq!(finished.event, "lifecycle.run.finished");
    assert_eq!(
        finished
            .payload
            .get("stop_reason")
            .and_then(hh_wire::json::Json::as_str),
        Some("budget_exhausted{model_calls}")
    );
    // Every `action.tool.proposed` has a matching completion or rejection — none left dangling.
    let proposed = trace
        .events()
        .iter()
        .filter(|e| e.event == "action.tool.proposed")
        .count();
    let resolved = trace
        .events()
        .iter()
        .filter(|e| e.event == "action.tool.completed" || e.event == "action.tool.rejected")
        .count();
    assert_eq!(proposed, resolved);
}

#[test]
fn allow_list_masks_secrets_and_leak_scan_is_empty() {
    // AC-R-2.8.3-1 (Stage-0 allow-list half).
    let env = ready_env(tmp("leak"));
    assert!(env.env_dump().contains("${SECRET:API_TOKEN}"));
    assert!(!env.env_dump().contains("sk-baseline-secret"));
    assert!(!env.read_process_env_file().contains("sk-baseline-secret"));
    assert!(env.leak_scan().is_empty());
    // SecretRef renders as name + description, never value.
    let r = SecretRef::new("API_TOKEN", "gateway credential");
    assert_eq!(r.render(), "API_TOKEN (gateway credential)");
}

#[test]
fn transport_retry_recovers_then_completes() {
    // R-2.6.2 transport-retry subset: a transient failure is retried, not surfaced.
    let def = baseline_definition("t");
    let env = ready_env(tmp("retry"));
    let mut budget = BudgetNode::root(1000, 10, 60_000);
    let script = vec![
        StubOutcome::TransportError {
            detail: "connection reset".into(),
        },
        StubOutcome::Response(ModelResponse {
            content: "recovered, done".into(),
            tool_call: None,
            usage: Usage {
                input_tokens: 5,
                output_tokens: 5,
            },
            done: true,
        }),
    ];
    let mut gw = Gateway::new(
        def.role_table.clone(),
        RoutePolicy::Static,
        StubModel::new(script),
        cred(),
    );
    let executor = ToolExecutor::new(&env, Duration::from_secs(10));
    let policy = ContextPolicy::default_policy(100_000);
    let mut trace = Trace::open(tmp("retry").join("trace.jsonl")).unwrap();
    let outcome = run(
        "r-retry",
        &def,
        &mut budget,
        &mut gw,
        &executor,
        &env,
        &policy,
        &Envelope {
            transport_retries: 1,
            tool_timeout: Duration::from_secs(5),
        },
        &mut trace,
    )
    .unwrap();
    assert_eq!(outcome.stop_reason, StopReason::Completed);
    assert!(trace
        .events()
        .iter()
        .any(|e| e.event == "model.call.attempt.failed"));
}

#[test]
fn transport_failure_past_retries_is_infrastructure_stop() {
    let def = baseline_definition("t");
    let env = ready_env(tmp("tfail"));
    let mut budget = BudgetNode::root(1000, 10, 60_000);
    let script = vec![
        StubOutcome::TransportError {
            detail: "reset 1".into(),
        },
        StubOutcome::TransportError {
            detail: "reset 2".into(),
        },
    ];
    let mut gw = Gateway::new(
        def.role_table.clone(),
        RoutePolicy::Static,
        StubModel::new(script),
        cred(),
    );
    let executor = ToolExecutor::new(&env, Duration::from_secs(10));
    let policy = ContextPolicy::default_policy(100_000);
    let mut trace = Trace::open(tmp("tfail").join("trace.jsonl")).unwrap();
    let outcome = run(
        "r-tfail",
        &def,
        &mut budget,
        &mut gw,
        &executor,
        &env,
        &policy,
        &Envelope {
            transport_retries: 0,
            tool_timeout: Duration::from_secs(5),
        },
        &mut trace,
    )
    .unwrap();
    assert_eq!(outcome.stop_reason, StopReason::TransportFailed);
}

#[test]
fn parse_refusal_stops_the_run() {
    let def = baseline_definition("t");
    let env = ready_env(tmp("parse"));
    let mut budget = BudgetNode::root(1000, 10, 60_000);
    let script = vec![StubOutcome::Unparseable {
        detail: "no action".into(),
    }];
    let mut gw = Gateway::new(
        def.role_table.clone(),
        RoutePolicy::Static,
        StubModel::new(script),
        cred(),
    );
    let executor = ToolExecutor::new(&env, Duration::from_secs(10));
    let policy = ContextPolicy::default_policy(100_000);
    let mut trace = Trace::open(tmp("parse").join("trace.jsonl")).unwrap();
    let outcome = run(
        "r-parse",
        &def,
        &mut budget,
        &mut gw,
        &executor,
        &env,
        &policy,
        &Envelope::default(),
        &mut trace,
    )
    .unwrap();
    assert_eq!(outcome.stop_reason, StopReason::ParseRefused);
}
