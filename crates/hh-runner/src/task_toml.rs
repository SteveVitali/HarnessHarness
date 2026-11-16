//! The `task.toml` declared-member reader (R6.45j; D15/D18; ADR-0448;
//! experiments spec §10.5 row A6) — the re-proof's per-task deadlines
//! and resource declarations come from the pinned task's own record,
//! resolved with the pinned Harbor version's semantics (`harbor` at
//! `ff2a1ef48070a8df374669138e4d4db79b77f013`, dataset
//! `harbor-framework/terminal-bench-2-1@12b75656`).
//!
//! This is a deliberately narrow reader, never a TOML implementation:
//! it binds only the top-level `[agent]`/`[verifier]`/`[environment]`
//! table headers and the `timeout_sec`/`cpus`/`memory_mb` keys and
//! skips every other byte of the file — matching Harbor's own
//! defaulting (`step.*.timeout_sec` falls back to the task-level value
//! when a `[[steps]]` member ever declares one, so the top-level
//! binding is the correct per-step default too). A bound key whose
//! value is not a positive finite number is a typed error — a
//! malformed declaration never silently becomes a default.
//!
//! The pinned semantics (recorded verbatim in ADR-0448's R6.45j
//! appendix):
//!
//! - `AgentConfig.timeout_sec: float | None = None` — the agent
//!   deadline is unbounded when the task declares none;
//!   `VerifierConfig.timeout_sec: float = 600.0` — the verifier's
//!   absent-value default is 600 s;
//! - `_resolve_timeout_sec`: `min(base, max or ∞) × multiplier` with
//!   the trial's `timeout_multiplier` default `1.0` — the re-proof's
//!   job config declares no override, no max and no multiplier, so the
//!   enforced deadline is the declared value verbatim;
//! - the agent deadline bounds `agent.run` only — agent *setup* runs
//!   under the separate `agent_setup_timeout_sec`
//!   (`DEFAULT_SETUP_TIMEOUT_SEC = 360.0`) and environment build under
//!   `build_timeout_sec`; the deadline never covers setup. The drive's
//!   undated setup legs (`stage_in`, the `mkdir` execs) match that
//!   split already.
//!
//! A task that declares no value for a phase still gets the drive's
//! `cfg.{agent,verify}_deadline_ms` machinery bound, recorded
//! `cfg_default` — Harbor's absent-agent `None` (unbounded) and the
//! 600 s verifier default are recorded for the ADR; the cfg fallback
//! is the drive's own safety bound, never a fabricated declaration.

use std::path::Path;

use crate::cell::DeadlineSource;

/// The declared members one task's `task.toml` carries.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TaskDecl {
    /// `[agent].timeout_sec` (seconds).
    pub agent_timeout_sec: Option<f64>,
    /// `[verifier].timeout_sec` (seconds).
    pub verifier_timeout_sec: Option<f64>,
    /// `[environment].cpus` — the declared token normalized the way
    /// Harbor's `normalize_cpus` collapses it (`2.0` → `"2"`).
    pub cpus: Option<String>,
    /// `[environment].memory_mb`.
    pub memory_mb: Option<u64>,
}

/// The `timeout_sec` scalar — a positive finite number of seconds.
fn timeout_value(task: &str, section: &str, line_no: usize, raw: &str) -> Result<f64, String> {
    // The value runs to the first `#` (comment) or the line end;
    // every bound key is numeric, so a bare token is required.
    let token = raw.split('#').next().unwrap_or("").trim();
    let v: f64 = token.parse().map_err(|_| {
        format!("task {task}: [{section}].timeout_sec line {line_no}: `{token}` is not a number")
    })?;
    if !v.is_finite() || v <= 0.0 {
        return Err(format!(
            "task {task}: [{section}].timeout_sec line {line_no}: {v} is not a positive \
             finite deadline — a malformed declaration refuses, never defaults"
        ));
    }
    Ok(v)
}

/// `parse_task_toml(task, text)` — the narrow reader. Only exact
/// top-level section names bind; `[verifier.environment]`,
/// `[steps.agent]` and every other table are skipped wholesale.
pub fn parse_task_toml(task: &str, text: &str) -> Result<TaskDecl, String> {
    let mut d = TaskDecl::default();
    let mut section = String::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            section = line
                .trim_matches(|c| c == '[' || c == ']')
                .trim()
                .to_string();
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        match (section.as_str(), key.trim()) {
            ("agent", "timeout_sec") => {
                d.agent_timeout_sec = Some(timeout_value(task, "agent", i + 1, val)?);
            }
            ("verifier", "timeout_sec") => {
                d.verifier_timeout_sec = Some(timeout_value(task, "verifier", i + 1, val)?);
            }
            ("environment", "cpus") => {
                let token = val.split('#').next().unwrap_or("").trim();
                let n: f64 = token.parse().map_err(|_| {
                    format!(
                        "task {task}: [environment].cpus line {}: `{token}` is not a number",
                        i + 1
                    )
                })?;
                if !n.is_finite() || n <= 0.0 {
                    return Err(format!(
                        "task {task}: [environment].cpus line {}: {n} is not a positive finite \
                         cpu count — a malformed declaration refuses",
                        i + 1
                    ));
                }
                // Harbor's normalize_cpus: a whole-number float collapses
                // to its int spelling.
                d.cpus = Some(if n.fract() == 0.0 {
                    format!("{}", n as u64)
                } else {
                    format!("{n}")
                });
            }
            ("environment", "memory_mb") => {
                let token = val.split('#').next().unwrap_or("").trim();
                let n: f64 = token.parse().map_err(|_| {
                    format!(
                        "task {task}: [environment].memory_mb line {}: `{token}` is not a number",
                        i + 1
                    )
                })?;
                if !n.is_finite() || n <= 0.0 || n.fract() != 0.0 {
                    return Err(format!(
                        "task {task}: [environment].memory_mb line {}: {n} is not a positive \
                         whole MiB count — a malformed declaration refuses",
                        i + 1
                    ));
                }
                d.memory_mb = Some(n as u64);
            }
            _ => {}
        }
    }
    Ok(d)
}

/// `load_task_decl(task_dir)` — read `<task_dir>/task.toml` from the
/// pinned checkout (`cfg.tasks_dir` on the VM).
pub fn load_task_decl(task_dir: &Path) -> Result<TaskDecl, String> {
    let p = task_dir.join("task.toml");
    let text = std::fs::read_to_string(&p)
        .map_err(|e| format!("task.toml {} unreadable: {e}", p.display()))?;
    let task = task_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "?".to_string());
    parse_task_toml(&task, &text)
}

/// `resolve_deadline_ms(declared_sec, fallback_ms)` — the pinned
/// Harbor resolution: a declared `timeout_sec` at multiplier 1.0
/// becomes the enforced ms verbatim with source `task_toml`; an absent
/// value falls back to the drive's cfg bound with source `cfg_default`.
pub fn resolve_deadline_ms(declared_sec: Option<f64>, fallback_ms: u64) -> (u64, DeadlineSource) {
    match declared_sec {
        Some(sec) => ((sec * 1000.0).round() as u64, DeadlineSource::TaskToml),
        None => (fallback_ms, DeadlineSource::CfgDefault),
    }
}
