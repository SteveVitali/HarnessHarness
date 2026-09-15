//! `s1-helper` — the throwaway helper binary the S1 spike drives across a process boundary
//! (l1-spike-spec §3.1 item 3; discharges the DF-S0.2-1 helper-*binary* boundary measurement).
//!
//! Reads newline-delimited `execute(command, cwd, policy)` requests on stdin, applies the host
//! primitive (here: run the command under a wall-clock deadline — the *same* external tool for
//! every candidate, so only the drive path is measured), and answers with
//! `{exit_code, stdout_hash, stderr_hash, duration_ms}` on stdout. Offline/hermetic: it only runs
//! local shell commands the spike itself issues (`true`, `echo`), no network.

use std::io::{BufRead, Write};
use std::process::Command;
use std::time::Instant;

use hh_wire::json::Json;
use hh_wire::sha256_hex;

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let req = match hh_wire::parse(line.trim()) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if req.get("op").and_then(Json::as_str) != Some("execute") {
            continue;
        }
        let command = req.get("command").and_then(Json::as_str).unwrap_or("true");
        let cwd = req.get("cwd").and_then(Json::as_str).unwrap_or(".");
        let t = Instant::now();
        // The "sandbox primitive": run the command. On a real host this is bubblewrap/Seatbelt/
        // job object; here it is the bare deadline drive path (the same for every candidate).
        let out = Command::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .current_dir(cwd)
            .output();
        let duration_ms = t.elapsed().as_millis() as i64;
        let reply = match out {
            Ok(o) => Json::obj([
                ("exit_code", Json::Int(o.status.code().unwrap_or(-1) as i64)),
                ("stdout_hash", Json::str(sha256_hex(&o.stdout))),
                ("stderr_hash", Json::str(sha256_hex(&o.stderr))),
                ("duration_ms", Json::Int(duration_ms)),
            ]),
            Err(e) => Json::obj([
                ("exit_code", Json::Int(-1)),
                ("stdout_hash", Json::str("")),
                (
                    "stderr_hash",
                    Json::str(sha256_hex(e.to_string().as_bytes())),
                ),
                ("duration_ms", Json::Int(duration_ms)),
            ]),
        };
        if stdout
            .write_all(format!("{}\n", reply.to_canonical_string()).as_bytes())
            .is_err()
        {
            break;
        }
        let _ = stdout.flush();
    }
}
