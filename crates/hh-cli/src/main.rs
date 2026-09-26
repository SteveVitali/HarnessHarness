//! The `hh` binary — thin `main`: gather the process facts (`argv`,
//! TTY-ness, piped stdin, env, the prompt channel), hand them to
//! `hh_cli::cli::run`, and exit with the authoritative class's numeral.

use std::io::{IsTerminal, Read, Write};

use hh_cli::cli::{run, Io, PromptFn};
use hh_cli::invocation::Tty;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let tty = Tty {
        stdin: std::io::stdin().is_terminal(),
        stdout: std::io::stdout().is_terminal(),
        stderr: std::io::stderr().is_terminal(),
    };
    // Piped stdin is read once, up front — a TTY stdin is never consumed
    // as piped input (M-1).
    let stdin = if !tty.stdin {
        let mut buf = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut buf);
        if buf.is_empty() {
            None
        } else {
            Some(buf)
        }
    } else {
        None
    };
    let kernel_env: Vec<(String, String)> = std::env::vars().collect();
    let cwd_ref = std::env::current_dir()
        .map(|p| format!("cwd:{}", p.display()))
        .unwrap_or_else(|_| "cwd:unknown".to_string());
    let principal = std::env::var("HH_PRINCIPAL")
        .or_else(|_| std::env::var("USER"))
        .map(|u| format!("principal:{u}"))
        .unwrap_or_else(|_| "principal:unknown".to_string());
    let kernel_cmd = std::env::var("HH_KERNEL_CMD").unwrap_or_else(|_| "hh-kernel".to_string());

    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    // The prompt channel: write the label to stderr, read one line from
    // stdin when it is a terminal. `None` under a pipe — a parked ask
    // detaches rather than hanging (M-1).
    let mut prompt_impl = move |label: &str| -> Option<String> {
        let _ = write!(std::io::stderr(), "{label}");
        let _ = std::io::stderr().flush();
        if !std::io::stdin().is_terminal() {
            return None;
        }
        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line.trim_end().to_string()),
        }
    };
    let prompt: Option<PromptFn> = if tty.stdin {
        Some(&mut prompt_impl)
    } else {
        None
    };

    let mut io = Io {
        tty,
        stdin,
        cwd_ref,
        principal,
        kernel_env,
        kernel_cmd,
        out: &mut out,
        err: &mut err,
        prompt,
    };
    let outcome = run(&argv, &mut io);
    std::process::exit(outcome.class.code());
}
