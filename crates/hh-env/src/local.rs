//! The `local_host` `tool_executor` — `LocalExecutor`, `isolation_support =
//! none` (honest — nothing is enforced). The sandboxed/container classes'
//! executor is [`crate::helper::HelperExecutor`]: since S2.1 the helper is
//! the dedicated `hh-helper` binary behind the live `hh-helper/1` channel
//! (the `sh`-framed Stage-1 stub is retired — the boundary is real now).
//!
//! `LocalExecutor` decides nothing — it runs the canonical args and
//! *reports* capture items + a terminal report (I-3).

use std::collections::BTreeSet;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use hh_containment::policy::IsolationClass;
use hh_hir::kinds::EffectDomain;
use hh_wire::json::Json;

use crate::capture::CaptureKind;
use crate::errors::EnvError;
use crate::executor::{
    DedupSupport, ExecutionRequest, ExecutorDeclaration, ExecutorSignal, ProbeSupport,
    ProbeVerdict, TerminalReport, TerminalStatus, ToolExecutor,
};
use crate::observe::ErrorClass;
use crate::protocol::HelperFrame;

/// `LocalExecutor` — the `local_host` executor. `isolation_support = none`
/// (honest — nothing is enforced); runs the command in a `std::process::
/// Command` child on the host.
pub struct LocalExecutor {
    decl: ExecutorDeclaration,
    /// The projected env (placeholders only — the kernel computes it via
    /// `env_apply`; the executor never resolves a secret).
    projected_env: Vec<(String, String)>,
}

impl LocalExecutor {
    /// A `local_host` executor — `isolation_support = none`, the `exec` +
    /// `spawn_process` + `fs_*` domains, kernel-side dedup only (`none` — the
    /// executor holds no store), `probe_support = check` (a lapsed window can
    /// be re-checked by inspecting the fs side-effects the kernel diffs).
    pub fn new(projected_env: Vec<(String, String)>) -> Self {
        LocalExecutor {
            decl: ExecutorDeclaration {
                executor_id: "hh-local-exec/1".to_string(),
                isolation_support: IsolationClass::None,
                dedup_support: DedupSupport::None,
                probe_support: ProbeSupport::Check,
                interrupt: crate::executor::InterruptSupport::Supported,
                error_classes: [
                    "invalid_arguments",
                    "not_found",
                    "conflict",
                    "executor_error",
                    "timeout",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect(),
                streams: true,
                domains: [
                    EffectDomain::Exec,
                    EffectDomain::SpawnProcess,
                    EffectDomain::FsRead,
                    EffectDomain::FsWrite,
                ]
                .iter()
                .copied()
                .collect(),
            },
            projected_env,
        }
    }
}

/// The shared command-runner — spawn the child, stream stdout/stderr chunks
/// to the sink (ephemeral), enforce the deadline + retain cap, return the
/// terminal report.
fn run_command(
    request: &ExecutionRequest,
    sink: &mut dyn FnMut(ExecutorSignal),
    env: &[(String, String)],
) -> Result<TerminalReport, EnvError> {
    // The command the capability's canonical args carry (`command` for a
    // shell tool; `argv` for an exec tool). Absent ⇒ `invalid_arguments`.
    let argv: Vec<String> = command_argv(&request.args).ok_or_else(|| EnvError::Unsupported {
        capability: "exec.args",
        detail: "no command/argv member".to_string(),
    })?;

    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Cleared env → placeholders only (the projected env; nothing ambient).
    cmd.env_clear();
    for (k, v) in env {
        cmd.env(k, v);
    }
    if let Some(cwd) = request.args.get("cwd").and_then(Json::as_str) {
        cmd.current_dir(cwd);
    }

    let mut child = cmd.spawn().map_err(|e| EnvError::Unsupported {
        capability: "exec.spawn",
        detail: format!("spawn: {e}"),
    })?;
    sink(ExecutorSignal {
        token: request.attribution_token.clone(),
        kind: CaptureKind::Process {
            transition: crate::capture::ProcessTransition::Spawned,
            process_ref: format!("pid:{}", child.id()),
        },
    });

    let deadline = request
        .deadline_ms
        .map(|d| Instant::now() + Duration::from_millis(d));
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut truncated = false;
    let mut dropped = 0u64;
    let mut seen = 0u64;
    let status = drain_child(
        &mut child,
        deadline,
        request.retain_bytes_cap,
        &mut out,
        &mut err,
        &mut truncated,
        &mut dropped,
        &mut seen,
        sink,
        &request.attribution_token,
    );

    match status {
        DrainOutcome::Exited(code) => {
            emit_chunk(sink, "stdout", &out, &request.attribution_token);
            emit_chunk(sink, "stderr", &err, &request.attribution_token);
            if code == 0 {
                Ok(TerminalReport {
                    status: TerminalStatus::Ok,
                    exit_status: Some(code as i64),
                    outcome_hint: "applied".to_string(),
                    retryable_hint: None,
                    detail_ref: None,
                    truncated,
                    omitted_bytes: dropped,
                    original_size: seen,
                })
            } else {
                // A non-zero exit with output is the tool's own result —
                // `origin = tool`, class `executor_error` (the generic
                // tool-failure member; the capability's `error_classes`
                // declares it).
                Ok(TerminalReport {
                    status: TerminalStatus::ToolError {
                        class: ErrorClass::ExecutorError,
                    },
                    exit_status: Some(code as i64),
                    outcome_hint: "not_applied".to_string(),
                    retryable_hint: None,
                    detail_ref: None,
                    truncated,
                    omitted_bytes: dropped,
                    original_size: seen,
                })
            }
        }
        DrainOutcome::Timeout => {
            let _ = child.kill();
            let _ = child.wait();
            Ok(TerminalReport {
                status: TerminalStatus::ToolError {
                    class: ErrorClass::Timeout,
                },
                exit_status: None,
                outcome_hint: "unknown".to_string(),
                retryable_hint: Some(false),
                detail_ref: None,
                truncated,
                omitted_bytes: dropped,
                original_size: seen,
            })
        }
        DrainOutcome::Killed(sig) => {
            let _ = child.wait();
            Ok(TerminalReport {
                status: TerminalStatus::ToolError {
                    class: ErrorClass::Signalled {
                        signal: sig.to_string(),
                    },
                },
                exit_status: None,
                outcome_hint: "unknown".to_string(),
                retryable_hint: Some(false),
                detail_ref: None,
                truncated,
                omitted_bytes: dropped,
                original_size: seen,
            })
        }
    }
}

/// Extract `argv` from the canonical args (`command: "a b c"` splits on
/// whitespace; `argv: [...]` carries the vector verbatim).
fn command_argv(args: &Json) -> Option<Vec<String>> {
    if let Some(Json::Arr(arr)) = args.get("argv") {
        let v: Vec<String> = arr
            .iter()
            .filter_map(|a| a.as_str().map(String::from))
            .collect();
        if !v.is_empty() {
            return Some(v);
        }
    }
    args.get("command")
        .and_then(Json::as_str)
        .map(|c| c.split_whitespace().map(String::from).collect())
}

/// Emit an `output_chunk` (ephemeral) if the buffer is non-empty.
fn emit_chunk(sink: &mut dyn FnMut(ExecutorSignal), stream: &str, buf: &[u8], token: &str) {
    if !buf.is_empty() {
        sink(ExecutorSignal {
            token: token.to_string(),
            kind: CaptureKind::OutputChunk {
                stream: stream.to_string(),
                data: String::from_utf8_lossy(buf).to_string(),
            },
        });
    }
}

enum DrainOutcome {
    Exited(i32),
    Timeout,
    Killed(&'static str),
}

/// Drain the child's pipes until exit/timeout, emitting chunks as they arrive.
#[allow(clippy::too_many_arguments)]
fn drain_child(
    child: &mut Child,
    deadline: Option<Instant>,
    retain_cap: u64,
    out: &mut Vec<u8>,
    err: &mut Vec<u8>,
    truncated: &mut bool,
    dropped: &mut u64,
    seen: &mut u64,
    sink: &mut dyn FnMut(ExecutorSignal),
    token: &str,
) -> DrainOutcome {
    let mut so = child.stdout.take();
    let mut se = child.stderr.take();
    let mut buf = [0u8; 8192];
    loop {
        if let Some(d) = deadline {
            if Instant::now() >= d {
                let _ = child.kill();
                return DrainOutcome::Timeout;
            }
        }
        let mut progressed = false;
        if let Some(s) = so.as_mut() {
            match s.read(&mut buf) {
                Ok(0) => {
                    so = None;
                }
                Ok(n) => {
                    progressed = true;
                    push_capped(out, &buf[..n], retain_cap, truncated, dropped, seen);
                    sink(ExecutorSignal {
                        token: token.to_string(),
                        kind: CaptureKind::OutputChunk {
                            stream: "stdout".to_string(),
                            data: String::from_utf8_lossy(&buf[..n]).to_string(),
                        },
                    });
                }
                Err(_) => so = None,
            }
        }
        if let Some(s) = se.as_mut() {
            match s.read(&mut buf) {
                Ok(0) => {
                    se = None;
                }
                Ok(n) => {
                    progressed = true;
                    push_capped(err, &buf[..n], retain_cap, truncated, dropped, seen);
                    sink(ExecutorSignal {
                        token: token.to_string(),
                        kind: CaptureKind::OutputChunk {
                            stream: "stderr".to_string(),
                            data: String::from_utf8_lossy(&buf[..n]).to_string(),
                        },
                    });
                }
                Err(_) => se = None,
            }
        }
        match child.try_wait() {
            Ok(Some(st)) => {
                if let Some(code) = st.code() {
                    return DrainOutcome::Exited(code);
                }
                return DrainOutcome::Killed("kill");
            }
            Ok(None) => {}
            Err(_) => return DrainOutcome::Killed("kill"),
        }
        if !progressed {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// Append to `buf` up to the retain cap (overflow → `truncated`, the excess
/// dropped — the manifest's `truncation` records it).
fn push_capped(
    buf: &mut Vec<u8>,
    chunk: &[u8],
    cap: u64,
    truncated: &mut bool,
    dropped: &mut u64,
    seen: &mut u64,
) {
    *seen = seen.saturating_add(chunk.len() as u64);
    let room = (cap as usize).saturating_sub(buf.len());
    if chunk.len() <= room {
        buf.extend_from_slice(chunk);
    } else {
        buf.extend_from_slice(&chunk[..room]);
        *truncated = true;
        *dropped = dropped.saturating_add((chunk.len() - room) as u64);
    }
}

impl ToolExecutor for LocalExecutor {
    fn declaration(&self) -> &ExecutorDeclaration {
        &self.decl
    }

    fn execute(
        &mut self,
        request: &ExecutionRequest,
        sink: &mut dyn FnMut(ExecutorSignal),
    ) -> Result<TerminalReport, EnvError> {
        run_command(request, sink, &self.projected_env)
    }

    fn probe(&self, _effect_id: &str, _attempt_no: u64) -> Result<ProbeVerdict, EnvError> {
        // The in-process executor has no second store — a lapsed window's
        // answer is `undeterminable` (the kernel's fs-diff evidence is the
        // probe the dispatcher uses; the executor honestly reports it cannot
        // check independently).
        Ok(ProbeVerdict::Undeterminable)
    }
}

/// `parse_helper_frame(j)` — the wire parse (the helper-side schema check:
/// version tag + closed kind). S2.1: implemented for real — the helper's
/// journaled frames come back over `read` and land in the capture sink via
/// `HelperExecutor`'s translation.
pub fn parse_helper_frame(j: &Json) -> Option<HelperFrame> {
    HelperFrame::from_json(j).ok()
}

/// The `exec` request's env member — placeholder-only (`env_apply`'s output).
/// A value that isn't a placeholder and isn't on the allowlist is withheld at
/// `env_apply`, never here.
pub fn projected_env_pairs(vars: &BTreeSet<String>) -> Vec<(String, String)> {
    vars.iter().map(|k| (k.clone(), String::new())).collect()
}
