//! `hh-baseline` — the headless CLI driver for the THROWAWAY Stage-0 baseline (ticket S0.2).
//!
//! Never reads stdin (headless `unattended`; nothing blocks on stdin — AC-R-2.11.1-6). Result
//! goes to stdout, progress/logs to stderr (the stdout/stderr split, §7). The run store lives
//! under `$HH_BASELINE_HOME` (default: a temp dir), so a fresh invocation of `run events`/`run
//! status` can read the trace a prior `run start` wrote.
//!
//! Usage:
//!   hh-baseline version
//!   hh-baseline doctor
//!   hh-baseline hello
//!   hh-baseline definition validate [--task <text>]
//!   hh-baseline run start --tokens N --model-calls N --time-ms N [--run <id>] [--task <text>] [--human|--jsonl]
//!   hh-baseline run events --run <id> [--from N] [--view compact]
//!   hh-baseline run status --run <id>

use std::io::{self, Write};
use std::process::ExitCode;

use hh_baseline::driver::Driver;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let home = std::env::var("HH_BASELINE_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("hh-baseline-home"));

    let driver = Driver::new(home);
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut out = stdout.lock();
    let mut err = stderr.lock();
    let class = driver.dispatch(&args, &mut out, &mut err);
    let _ = out.flush();
    let _ = err.flush();
    ExitCode::from(class.code() as u8)
}
