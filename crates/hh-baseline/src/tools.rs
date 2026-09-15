//! One in-process `tool_executor` for shell + fs (R-2.5.5; §5d.5). **Throwaway Stage-0 subset.**
//!
//! §9.1 R-2.2.5/R-2.5.5 slice: "one in-process `tool_executor` for shell + fs". This is the
//! executor boundary AC-R-2.5.5-14 exercises at Stage 0 ("one sandboxed tool call through …
//! the boundary … passes as the Stage-0 acceptance check"). The helper-*binary* boundary and
//! its fan-out/footprint *measurement* are the S0.3 spike (operator-gated; DEFERRALS
//! DF-S0.2-1) — this ticket owns the in-process executor half.
//!
//! Sandbox (honest, `isolation_class = none`): fs paths resolve under the run root and may not
//! escape it (`SandboxDenied`); shell runs with `cwd = root` and the handle's masked
//! environment; a per-call deadline kills a runaway process (tool timeout with kill, R-2.6.2).
//! Every observed byte is masked before it leaves the boundary (redaction placement,
//! AC-R-2.5.5-8 shape).

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::env::{EnvHandle, EnvState};
use crate::secrets::mask_known_values;

/// The Stage-0 tool set: a shell command and two fs primitives (the hand-authored read tool is
/// `ReadFile` — layer-A memory is reached only through it, R-2.4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCall {
    Shell { command: String },
    WriteFile { path: String, content: String },
    ReadFile { path: String },
}

impl ToolCall {
    pub fn name(&self) -> &'static str {
        match self {
            ToolCall::Shell { .. } => "shell",
            ToolCall::WriteFile { .. } => "write_file",
            ToolCall::ReadFile { .. } => "read_file",
        }
    }
}

/// The closed error sum for the executor (the Stage-0 subset of §5d.5's `ErrorClass`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// A path escaped the sandbox root, or was absolute.
    SandboxDenied { path: String },
    /// The per-call deadline elapsed and the process was killed.
    Timeout { after_ms: u64 },
    /// The tool ran but reported failure (non-zero exit, missing file, …).
    Failed { detail: String },
}

/// A successful observation. `output` is already masked; `observation_bytes` and `executor_ms`
/// feed the M7 (`tool_call`) trace payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    pub tool: String,
    pub output: String,
    pub observation_bytes: usize,
    pub executor_ms: u64,
}

/// The in-process executor, scoped to one environment handle.
pub struct ToolExecutor<'a> {
    env: &'a EnvHandle,
    timeout: Duration,
}

impl<'a> ToolExecutor<'a> {
    pub fn new(env: &'a EnvHandle, timeout: Duration) -> Self {
        Self { env, timeout }
    }

    /// Resolve a caller-supplied relative path under the sandbox root, rejecting absolute paths
    /// and any `..` component that would escape (deny-by-default).
    fn resolve(&self, path: &str) -> Result<PathBuf, ToolError> {
        let p = std::path::Path::new(path);
        if p.is_absolute() {
            return Err(ToolError::SandboxDenied { path: path.into() });
        }
        for comp in p.components() {
            if matches!(comp, std::path::Component::ParentDir) {
                return Err(ToolError::SandboxDenied { path: path.into() });
            }
        }
        Ok(self.env.root().join(p))
    }

    /// Execute one tool call through the boundary. The executor runs only when the handle is
    /// `Ready` (a call before `ready()` or after `tear_down()` is `SandboxDenied`).
    pub fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolError> {
        if self.env.state() != EnvState::Ready {
            return Err(ToolError::SandboxDenied {
                path: format!("<env not ready: {}>", self.env.state().as_str()),
            });
        }
        let start = Instant::now();
        let (tool, raw) = match &call {
            ToolCall::WriteFile { path, content } => {
                let full = self.resolve(path)?;
                if let Some(parent) = full.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| ToolError::Failed {
                        detail: e.to_string(),
                    })?;
                }
                std::fs::write(&full, content).map_err(|e| ToolError::Failed {
                    detail: e.to_string(),
                })?;
                ("write_file", format!("wrote {} bytes", content.len()))
            }
            ToolCall::ReadFile { path } => {
                let full = self.resolve(path)?;
                let bytes = std::fs::read_to_string(&full).map_err(|e| ToolError::Failed {
                    detail: e.to_string(),
                })?;
                ("read_file", bytes)
            }
            ToolCall::Shell { command } => ("shell", self.run_shell(command)?),
        };
        // Redaction placement: mask on the way out of the boundary.
        let output = mask_known_values(&raw, self.env.secrets());
        Ok(ToolResult {
            tool: tool.to_string(),
            observation_bytes: output.len(),
            output,
            executor_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Run a shell command with `cwd = root`, the masked environment, and a deadline that kills
    /// a runaway process (tool timeout with kill).
    fn run_shell(&self, command: &str) -> Result<String, ToolError> {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(self.env.root())
            .env_clear()
            .envs(self.env.sandbox_env())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ToolError::Failed {
                detail: e.to_string(),
            })?;

        let deadline = Instant::now() + self.timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let mut out = String::new();
                    if let Some(mut so) = child.stdout.take() {
                        let _ = so.read_to_string(&mut out);
                    }
                    if status.success() {
                        return Ok(out);
                    }
                    let mut err = String::new();
                    if let Some(mut se) = child.stderr.take() {
                        let _ = se.read_to_string(&mut err);
                    }
                    return Err(ToolError::Failed {
                        detail: format!("exit {status}: {}", err.trim()),
                    });
                }
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(ToolError::Timeout {
                            after_ms: self.timeout.as_millis() as u64,
                        });
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => {
                    return Err(ToolError::Failed {
                        detail: e.to_string(),
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::{AllowList, HeldSecret, SecretRef};
    use std::collections::BTreeMap;

    fn ready_env(tag: &str) -> EnvHandle {
        let root =
            std::env::temp_dir().join(format!("hh-baseline-tools-{tag}-{}", std::process::id()));
        let secrets = vec![HeldSecret::new(
            SecretRef::new("API_TOKEN", "gateway credential"),
            "sk-secret-xyz",
        )];
        let mut env = EnvHandle::declare(root, AllowList::new().allow("PATH"), secrets, {
            let mut h = BTreeMap::new();
            h.insert("PATH".into(), "/usr/bin:/bin".into());
            h
        });
        env.ready().unwrap();
        env
    }

    #[test]
    fn one_sandboxed_shell_call_passes_the_boundary() {
        // AC-R-2.5.5-14: one sandboxed tool call through the executor boundary.
        let env = ready_env("shell");
        let x = ToolExecutor::new(&env, Duration::from_secs(5));
        let r = x
            .execute(ToolCall::Shell {
                command: "echo hi".into(),
            })
            .unwrap();
        assert_eq!(r.tool, "shell");
        assert_eq!(r.output.trim(), "hi");
    }

    #[test]
    fn write_then_read_round_trips_under_the_root() {
        let env = ready_env("rw");
        let x = ToolExecutor::new(&env, Duration::from_secs(5));
        x.execute(ToolCall::WriteFile {
            path: "notes/hello.txt".into(),
            content: "world".into(),
        })
        .unwrap();
        let r = x
            .execute(ToolCall::ReadFile {
                path: "notes/hello.txt".into(),
            })
            .unwrap();
        assert_eq!(r.output, "world");
        // the file really lives under the sandbox root
        assert!(env.root().join("notes/hello.txt").exists());
        env_teardown(env);
    }

    #[test]
    fn absolute_and_parent_paths_are_denied() {
        let env = ready_env("deny");
        let x = ToolExecutor::new(&env, Duration::from_secs(5));
        assert_eq!(
            x.execute(ToolCall::ReadFile {
                path: "/etc/passwd".into()
            }),
            Err(ToolError::SandboxDenied {
                path: "/etc/passwd".into()
            })
        );
        assert!(matches!(
            x.execute(ToolCall::ReadFile {
                path: "../escape".into()
            }),
            Err(ToolError::SandboxDenied { .. })
        ));
    }

    #[test]
    fn timeout_kills_a_runaway_process() {
        let env = ready_env("timeout");
        let x = ToolExecutor::new(&env, Duration::from_millis(150));
        let r = x.execute(ToolCall::Shell {
            command: "sleep 5".into(),
        });
        assert!(matches!(r, Err(ToolError::Timeout { .. })), "got {r:?}");
    }

    #[test]
    fn output_is_masked_at_the_boundary() {
        let env = ready_env("mask");
        let x = ToolExecutor::new(&env, Duration::from_secs(5));
        // A hostile command echoing the secret's value: the capture path must mask it.
        let r = x
            .execute(ToolCall::Shell {
                command: "echo sk-secret-xyz".into(),
            })
            .unwrap();
        assert!(!r.output.contains("sk-secret-xyz"));
        assert!(r.output.contains("${SECRET:API_TOKEN}"));
    }

    #[test]
    fn executor_refuses_when_env_not_ready() {
        let root = std::env::temp_dir().join("hh-baseline-tools-notready");
        let env = EnvHandle::declare(root, AllowList::new(), vec![], BTreeMap::new());
        let x = ToolExecutor::new(&env, Duration::from_secs(1));
        assert!(matches!(
            x.execute(ToolCall::Shell {
                command: "echo hi".into()
            }),
            Err(ToolError::SandboxDenied { .. })
        ));
    }

    fn env_teardown(mut env: EnvHandle) {
        env.tear_down();
    }
}
