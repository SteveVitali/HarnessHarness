//! The headless CLI driver (R-2.11.1⁰ᵃ; §7.1). **Throwaway Stage-0 subset.**
//!
//! §7 C0/Stage-0 driver slice: "`run start` (headless), `run events`, `run status`,
//! `definition validate`, `version`, `doctor`; `human` + `jsonl` with `result`; stdout/stderr
//! split; `unattended` + `unattended_deny`; `MissingBudget`; exit classes `ok |
//! invocation_error | validation_error | infrastructure_failure | budget_exhausted`". The
//! attended modes, the full exit-class table, `json`/stdin typing and idempotency keys land at
//! S1.26; this is the headless driver the Stage-0 baseline (and the S0.3 spike) run through.
//!
//! Two seams matter for the gated ACs:
//! - **AC-R-2.11.1-3:** `run start --jsonl` writes each trace event's canonical line then one
//!   `result` record; `run events --from 0` writes the same event lines verbatim from the
//!   single-file trace — so the former minus its final `result` line equals the latter,
//!   byte for byte.
//! - **AC-R-2.11.1-6:** with no TTY and no declaration, attendance is `unattended`/
//!   `tty_inferred`, `approval_mode = unattended_deny`, every `ask` converts to a policy `deny`,
//!   and no command reads stdin (so nothing blocks on it).

use std::io::Write;
use std::path::PathBuf;

use hh_embed_schema as embed;
use hh_wire::json::Json;

use crate::budget::BudgetNode;
use crate::control::{self, Envelope, RunOutcome, StopReason};
use crate::env::EnvHandle;
use crate::gateway::{Gateway, ModelResponse, RoutePolicy, StubModel, StubOutcome, Usage};
use crate::hir::{baseline_definition, HarnessDefinition};
use crate::secrets::{AllowList, HeldSecret, SecretRef};
use crate::tools::{ToolCall, ToolExecutor};
use crate::trace::Trace;

/// The five Stage-0 exit classes (§7). Total over pre-run errors and terminal `StopReason`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitClass {
    Ok,
    InvocationError,
    ValidationError,
    InfrastructureFailure,
    BudgetExhausted,
}

impl ExitClass {
    pub fn as_str(self) -> &'static str {
        match self {
            ExitClass::Ok => "ok",
            ExitClass::InvocationError => "invocation_error",
            ExitClass::ValidationError => "validation_error",
            ExitClass::InfrastructureFailure => "infrastructure_failure",
            ExitClass::BudgetExhausted => "budget_exhausted",
        }
    }

    /// The process exit-code numeral (MUST-data; the Stage-0 assignment — ADR for S0.2). OQ-384
    /// finalizes the numerals before Stage 1; these are the interim values.
    pub fn code(self) -> i32 {
        match self {
            ExitClass::Ok => 0,
            ExitClass::InvocationError => 2,
            ExitClass::ValidationError => 3,
            ExitClass::InfrastructureFailure => 4,
            ExitClass::BudgetExhausted => 5,
        }
    }

    /// The total mapping from a terminal `StopReason` to an exit class. Every variant is mapped
    /// (a property test asserts totality).
    pub fn of(stop: &StopReason) -> ExitClass {
        match stop {
            StopReason::Completed => ExitClass::Ok,
            StopReason::BudgetExhausted { .. } => ExitClass::BudgetExhausted,
            StopReason::ContextWindowExceeded
            | StopReason::ParseRefused
            | StopReason::ToolTimeout
            | StopReason::TransportFailed => ExitClass::InfrastructureFailure,
        }
    }
}

/// The attendance the CLI writes into the run manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attendance {
    pub value: &'static str,
    pub source: &'static str,
    pub approval_mode: &'static str,
}

/// Derive attendance from the environment. Stage 0 is headless-only: with no TTY and no
/// declaration, attendance is `unattended`, source `tty_inferred`, approval `unattended_deny`
/// (AC-R-2.11.1-6). A declaration/TTY path is S1.26.
pub fn derive_attendance(has_tty: bool) -> Attendance {
    // At Stage 0 only the no-TTY inference exists; a TTY still infers unattended headless.
    let _ = has_tty;
    Attendance {
        value: "unattended",
        source: "tty_inferred",
        approval_mode: "unattended_deny",
    }
}

/// Convert an `ask` under the headless policy. In `unattended_deny` an ask becomes a `deny`
/// decided by policy with reason `unattended` (never a human, never a block on stdin).
pub fn convert_ask() -> Json {
    Json::obj([
        ("decider", Json::str("policy")),
        ("reason", Json::str("unattended")),
        ("decision", Json::str("deny")),
    ])
}

/// A budget assembled from flags (the only Stage-0 budget source besides the definition).
struct BudgetFlags {
    tokens: i64,
    model_calls: i64,
    time_ms: i64,
}

/// The driver: a per-invocation object holding the run store base directory.
pub struct Driver {
    base_dir: PathBuf,
}

impl Driver {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    fn trace_path(&self, run_id: &str) -> PathBuf {
        self.base_dir.join("runs").join(format!("{run_id}.jsonl"))
    }

    /// Dispatch one CLI invocation. `out` is the result stream (stdout), `err` is the progress
    /// stream (stderr). Never reads stdin — so nothing can block on it (AC-R-2.11.1-6).
    pub fn dispatch<O: Write, E: Write>(
        &self,
        args: &[String],
        out: &mut O,
        err: &mut E,
    ) -> ExitClass {
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        match a.as_slice() {
            ["version", ..] => self.cmd_version(out),
            ["doctor", ..] => self.cmd_doctor(out, err),
            ["hello", ..] => self.cmd_hello(out),
            ["definition", "validate", rest @ ..] => self.cmd_definition_validate(rest, out, err),
            ["run", "start", rest @ ..] => self.cmd_run_start(rest, out, err),
            ["run", "events", rest @ ..] => self.cmd_run_events(rest, out, err),
            ["run", "status", rest @ ..] => self.cmd_run_status(rest, out, err),
            _ => {
                let _ = writeln!(err, "hh-baseline: unknown or malformed command");
                ExitClass::InvocationError
            }
        }
    }

    fn cmd_version<O: Write>(&self, out: &mut O) -> ExitClass {
        let rec = Json::obj([
            (
                "kernel_version_id",
                Json::str(format!("hh-baseline/{}", env!("CARGO_PKG_VERSION"))),
            ),
            ("contract_major", Json::Int(embed::CONTRACT_MAJOR)),
            ("schema_hash", Json::str(embed::schema_hash())),
        ]);
        let _ = writeln!(out, "{}", rec.to_canonical_string());
        ExitClass::Ok
    }

    /// In-process `hello` negotiation over the reused S0.1 boundary (the baseline asserts its
    /// own `(contract_major, schema_hash)` against the single schema source).
    fn cmd_doctor<O: Write, E: Write>(&self, out: &mut O, err: &mut E) -> ExitClass {
        let kernel = embed::ContractIdentity {
            contract_major: embed::CONTRACT_MAJOR,
            schema_hash: embed::schema_hash(),
            kernel_version_id: format!("hh-baseline/{}", env!("CARGO_PKG_VERSION")),
        };
        let params = embed::HelloParams {
            client_name: "hh-baseline doctor".into(),
            client_version: env!("CARGO_PKG_VERSION").into(),
            asserted_contract_major: embed::CONTRACT_MAJOR,
            asserted_schema_hash: Some(kernel.schema_hash.clone()),
        };
        match embed::negotiate(&params, &kernel) {
            Ok(()) => {
                let _ = writeln!(err, "doctor: hello ok — schema_hash={}", kernel.schema_hash);
                let _ = writeln!(out, "ok");
                ExitClass::Ok
            }
            Err(e) => {
                let _ = writeln!(err, "doctor: hello failed: {}", e.message());
                ExitClass::InfrastructureFailure
            }
        }
    }

    fn cmd_hello<O: Write>(&self, out: &mut O) -> ExitClass {
        let id = embed::ContractIdentity {
            contract_major: embed::CONTRACT_MAJOR,
            schema_hash: embed::schema_hash(),
            kernel_version_id: format!("hh-baseline/{}", env!("CARGO_PKG_VERSION")),
        };
        let _ = writeln!(out, "{}", id.to_json().to_canonical_string());
        ExitClass::Ok
    }

    fn cmd_definition_validate<O: Write, E: Write>(
        &self,
        rest: &[&str],
        out: &mut O,
        err: &mut E,
    ) -> ExitClass {
        let task =
            flag(rest, "--task").unwrap_or_else(|| "write hello.txt and read it back".into());
        let def = baseline_definition(task);
        match def.validate() {
            Ok(()) => {
                let _ = writeln!(
                    out,
                    "{}",
                    Json::obj([("valid", Json::Bool(true))]).to_canonical_string()
                );
                ExitClass::Ok
            }
            Err(e) => {
                // A validation_error never opens a run (AC-R-2.11.1-10).
                let _ = writeln!(err, "definition invalid: {e:?}");
                let _ = writeln!(
                    out,
                    "{}",
                    Json::obj([("valid", Json::Bool(false))]).to_canonical_string()
                );
                ExitClass::ValidationError
            }
        }
    }

    fn cmd_run_start<O: Write, E: Write>(
        &self,
        rest: &[&str],
        out: &mut O,
        err: &mut E,
    ) -> ExitClass {
        // Budgets are mandatory (§7 D5): no budget from flags ⇒ MissingBudget, no run opens.
        let budget = match parse_budget(rest) {
            Some(b) => b,
            None => {
                let _ = writeln!(
                    err,
                    "MissingBudget: run start requires --tokens, --model-calls and --time-ms"
                );
                return ExitClass::InvocationError;
            }
        };
        let format = if rest.contains(&"--human") {
            OutputFormat::Human
        } else {
            OutputFormat::Jsonl
        };
        let run_id = flag(rest, "--run").unwrap_or_else(|| format!("run-{}", std::process::id()));
        let task =
            flag(rest, "--task").unwrap_or_else(|| "write hello.txt and read it back".into());

        let def = baseline_definition(&task);
        if let Err(e) = def.validate() {
            let _ = writeln!(err, "definition invalid: {e:?}");
            return ExitClass::ValidationError; // never opens a run
        }

        let attendance = derive_attendance(false);
        let _ = writeln!(
            err,
            "progress: opening run {run_id} attendance={} source={} approval={}",
            attendance.value, attendance.source, attendance.approval_mode
        );

        let outcome = match self.execute_run(&run_id, &def, budget, &attendance) {
            Ok(o) => o,
            Err(e) => {
                let _ = writeln!(err, "infrastructure failure: {e}");
                return ExitClass::InfrastructureFailure;
            }
        };

        match format {
            OutputFormat::Jsonl => {
                // Stream every trace event line, then exactly one `result` record.
                if let Ok(content) = std::fs::read_to_string(self.trace_path(&run_id)) {
                    let _ = out.write_all(content.as_bytes());
                }
                let _ = writeln!(
                    out,
                    "{}",
                    self.result_record(&outcome).to_canonical_string()
                );
            }
            OutputFormat::Human => {
                let _ = writeln!(
                    out,
                    "run {} {} in {} turn(s){}",
                    outcome.run_id,
                    outcome.stop_reason.as_str(),
                    outcome.turns,
                    outcome
                        .final_answer
                        .as_ref()
                        .map(|a| format!(": {a}"))
                        .unwrap_or_default()
                );
            }
        }
        ExitClass::of(&outcome.stop_reason)
    }

    fn result_record(&self, outcome: &RunOutcome) -> Json {
        Json::obj([(
            "result",
            Json::obj([
                ("run_id", Json::str(outcome.run_id.clone())),
                ("stop_reason", Json::str(outcome.stop_reason.as_str())),
                (
                    "exit_class",
                    Json::str(ExitClass::of(&outcome.stop_reason).as_str()),
                ),
                ("turns", Json::Int(outcome.turns)),
            ]),
        )])
    }

    /// Build the baseline pieces and run the react/minimal loop, persisting the single-file
    /// trace under the run store. Offline: the model is a scripted stub.
    fn execute_run(
        &self,
        run_id: &str,
        def: &HarnessDefinition,
        budget: BudgetFlags,
        _attendance: &Attendance,
    ) -> std::io::Result<RunOutcome> {
        let mut budget_node = BudgetNode::root(budget.tokens, budget.model_calls, budget.time_ms);

        // Kernel-held gateway credential; also registered as an env secret so masking works.
        let credential = HeldSecret::new(
            SecretRef::new("API_TOKEN", "gateway credential"),
            "sk-baseline-secret",
        );
        let root = self.base_dir.join("sandbox").join(run_id);
        let mut env = EnvHandle::declare(
            root,
            AllowList::new().allow("PATH"),
            vec![credential.clone()],
            {
                let mut h = std::collections::BTreeMap::new();
                h.insert("PATH".into(), "/usr/bin:/bin".into());
                h
            },
        );
        env.ready()?;

        // The offline scripted model: write a file, read it back, then answer.
        let script = baseline_script(&def.task);
        let mut gateway = Gateway::new(
            def.role_table.clone(),
            RoutePolicy::Static,
            StubModel::new(script),
            credential,
        );
        let envelope = Envelope::default();
        // The tool timeout with kill is the envelope's knob (R-2.6.2), so the executor's
        // deadline is the declared value, not a second constant.
        let executor = ToolExecutor::new(&env, envelope.tool_timeout);
        let policy = crate::context::ContextPolicy::default_policy(100_000);
        let mut trace = Trace::open(self.trace_path(run_id))?;

        let outcome = control::run(
            run_id,
            def,
            &mut budget_node,
            &mut gateway,
            &executor,
            &env,
            &policy,
            &envelope,
            &mut trace,
        )?;

        // CC3 self-check: what the run charged equals what the cost_view reports.
        debug_assert_eq!(trace.charged_totals(), trace.cost_view(&budget_node));
        env.tear_down();
        Ok(outcome)
    }

    fn cmd_run_events<O: Write, E: Write>(
        &self,
        rest: &[&str],
        out: &mut O,
        err: &mut E,
    ) -> ExitClass {
        let run_id = match flag(rest, "--run") {
            Some(r) => r,
            None => {
                let _ = writeln!(err, "run events: --run <id> required");
                return ExitClass::InvocationError;
            }
        };
        let path = self.trace_path(&run_id);
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => {
                let _ = writeln!(err, "UnknownRun: {run_id}");
                return ExitClass::InvocationError;
            }
        };
        if rest.contains(&"--view") && flag(rest, "--view").as_deref() == Some("compact") {
            return self.emit_compact(&run_id, out, err);
        }
        let from: i64 = flag(rest, "--from")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if from == 0 {
            // Verbatim: identical bytes to what `run start --jsonl` streamed (AC-R-2.11.1-3).
            let _ = out.write_all(content.as_bytes());
        } else {
            for line in content.lines() {
                if let Ok(v) = hh_wire::parse(line) {
                    if v.get("seq").and_then(Json::as_int).unwrap_or(0) >= from {
                        let _ = writeln!(out, "{line}");
                    }
                }
            }
        }
        ExitClass::Ok
    }

    fn emit_compact<O: Write, E: Write>(
        &self,
        run_id: &str,
        out: &mut O,
        err: &mut E,
    ) -> ExitClass {
        // Rebuild the trace from the file into a Trace-like view is out of scope; instead read
        // the lines and re-derive the compact projection + loss report here.
        let content = std::fs::read_to_string(self.trace_path(run_id)).unwrap_or_default();
        const KEPT: &[&str] = &[
            "model.call.completed",
            "action.tool.completed",
            "control.budget.consumed",
            "lifecycle.run.finished",
        ];
        let mut dropped: std::collections::BTreeMap<String, i64> =
            std::collections::BTreeMap::new();
        for line in content.lines() {
            if let Ok(v) = hh_wire::parse(line) {
                let ev = v.get("event").and_then(Json::as_str).unwrap_or("");
                if KEPT.contains(&ev) {
                    let _ = writeln!(out, "{line}");
                } else {
                    *dropped.entry(ev.to_string()).or_insert(0) += 1;
                }
            }
        }
        let report = Json::obj([(
            "loss_report",
            Json::Arr(
                dropped
                    .iter()
                    .map(|(k, v)| {
                        Json::obj([("class", Json::str(k.clone())), ("count", Json::Int(*v))])
                    })
                    .collect(),
            ),
        )]);
        let _ = writeln!(out, "{}", report.to_canonical_string());
        let _ = writeln!(
            err,
            "compact: declared-lossy projection; dropped {} class(es)",
            dropped.len()
        );
        ExitClass::Ok
    }

    fn cmd_run_status<O: Write, E: Write>(
        &self,
        rest: &[&str],
        out: &mut O,
        err: &mut E,
    ) -> ExitClass {
        let run_id = match flag(rest, "--run") {
            Some(r) => r,
            None => {
                let _ = writeln!(err, "run status: --run <id> required");
                return ExitClass::InvocationError;
            }
        };
        let content = match std::fs::read_to_string(self.trace_path(&run_id)) {
            Ok(c) => c,
            Err(_) => {
                let _ = writeln!(err, "UnknownRun: {run_id}");
                return ExitClass::InvocationError;
            }
        };
        let mut stop = String::from("running");
        let mut turns = 0i64;
        for line in content.lines() {
            if let Ok(v) = hh_wire::parse(line) {
                if v.get("event").and_then(Json::as_str) == Some("lifecycle.run.finished") {
                    if let Some(p) = v.get("payload") {
                        stop = p
                            .get("stop_reason")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string();
                        turns = p.get("turns").and_then(Json::as_int).unwrap_or(0);
                    }
                }
            }
        }
        let rec = Json::obj([
            ("run_id", Json::str(run_id)),
            ("stop_reason", Json::str(stop)),
            ("turns", Json::Int(turns)),
        ]);
        let _ = writeln!(out, "{}", rec.to_canonical_string());
        ExitClass::Ok
    }
}

/// The `human | jsonl` output format (the Stage-0 subset; `json` lands at S1.26).
enum OutputFormat {
    Human,
    Jsonl,
}

/// The offline scripted model that accomplishes the baseline coding task: write a file, read it
/// back, then answer. This is the fixture that stands in for a live model (offline-only).
fn baseline_script(_task: &str) -> Vec<StubOutcome> {
    vec![
        StubOutcome::Response(ModelResponse {
            content: "I will create the file.".into(),
            tool_call: Some(ToolCall::WriteFile {
                path: "hello.txt".into(),
                content: "hello from react/minimal".into(),
            }),
            usage: Usage {
                input_tokens: 20,
                output_tokens: 8,
            },
            done: false,
        }),
        StubOutcome::Response(ModelResponse {
            content: "Now I will read it back.".into(),
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
            content: "The file contains: hello from react/minimal. Done.".into(),
            tool_call: None,
            usage: Usage {
                input_tokens: 25,
                output_tokens: 12,
            },
            done: true,
        }),
    ]
}

/// A trailing `--flag value` reader (no `=` form at Stage 0).
fn flag(args: &[&str], name: &str) -> Option<String> {
    args.iter()
        .position(|a| *a == name)
        .and_then(|i| args.get(i + 1).map(|s| s.to_string()))
}

fn parse_budget(args: &[&str]) -> Option<BudgetFlags> {
    let tokens = flag(args, "--tokens").and_then(|s| s.parse().ok())?;
    let model_calls = flag(args, "--model-calls").and_then(|s| s.parse().ok())?;
    let time_ms = flag(args, "--time-ms").and_then(|s| s.parse().ok())?;
    Some(BudgetFlags {
        tokens,
        model_calls,
        time_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::Dimension;

    #[test]
    fn exit_class_mapping_is_total_over_stop_reasons() {
        let all = [
            StopReason::Completed,
            StopReason::BudgetExhausted {
                dimension: Dimension::TokensBlended,
            },
            StopReason::BudgetExhausted {
                dimension: Dimension::ModelCalls,
            },
            StopReason::BudgetExhausted {
                dimension: Dimension::TimeWallMs,
            },
            StopReason::ContextWindowExceeded,
            StopReason::ParseRefused,
            StopReason::ToolTimeout,
            StopReason::TransportFailed,
        ];
        for s in &all {
            let c = ExitClass::of(s);
            // every stop reason maps to a class and back to a numeral in the total set
            assert!([0, 2, 3, 4, 5].contains(&c.code()));
        }
        assert_eq!(ExitClass::of(&StopReason::Completed), ExitClass::Ok);
        assert_eq!(
            ExitClass::of(&StopReason::BudgetExhausted {
                dimension: Dimension::ModelCalls
            }),
            ExitClass::BudgetExhausted
        );
    }

    #[test]
    fn attendance_is_unattended_tty_inferred_unattended_deny() {
        // AC-R-2.11.1-6 derivation half.
        let a = derive_attendance(false);
        assert_eq!(a.value, "unattended");
        assert_eq!(a.source, "tty_inferred");
        assert_eq!(a.approval_mode, "unattended_deny");
    }

    #[test]
    fn ask_converts_to_policy_deny_unattended() {
        let d = convert_ask();
        assert_eq!(d.get("decider").and_then(Json::as_str), Some("policy"));
        assert_eq!(d.get("reason").and_then(Json::as_str), Some("unattended"));
        assert_eq!(d.get("decision").and_then(Json::as_str), Some("deny"));
    }
}
