//! CLI-driver integration tests (ticket S0.2). Drive `Driver::dispatch` with in-memory
//! buffers — no stdin is ever supplied, which is itself the proof that no command blocks on it
//! (AC-R-2.11.1-6). Asserts:
//!   - AC-R-2.11.1-3  `run start --jsonl` minus final `result` == `run events --from 0`, byte for byte
//!   - AC-R-2.11.1-6  no-TTY ⇒ unattended/tty_inferred; `ask`→policy deny; no stdin block
//!   - MissingBudget refusal (invocation_error, no run opens)
//!   - the five exit classes over the driver's outcomes

use hh_baseline::driver::{Driver, ExitClass};

fn home(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("hh-baseline-cli-{tag}-{}", std::process::id()))
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// Run one dispatch, returning (exit class, stdout, stderr).
fn dispatch(driver: &Driver, args: &[&str]) -> (ExitClass, String, String) {
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let class = driver.dispatch(&s(args), &mut out, &mut err);
    (
        class,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

#[test]
fn run_start_jsonl_minus_result_equals_run_events_byte_for_byte() {
    // AC-R-2.11.1-3
    let d = Driver::new(home("jsonl"));
    let (class, start_out, _err) = dispatch(
        &d,
        &[
            "run",
            "start",
            "--run",
            "acr3",
            "--tokens",
            "1000",
            "--model-calls",
            "10",
            "--time-ms",
            "60000",
            "--jsonl",
        ],
    );
    assert_eq!(class, ExitClass::Ok);

    // The final line is the `result` record; everything before it is the event stream.
    let mut lines: Vec<&str> = start_out.lines().collect();
    let result_line = lines.pop().unwrap();
    assert!(result_line.contains("\"result\""));
    let start_events = format!("{}\n", lines.join("\n"));

    let (class2, events_out, _e2) =
        dispatch(&d, &["run", "events", "--run", "acr3", "--from", "0"]);
    assert_eq!(class2, ExitClass::Ok);

    // Byte-for-byte equality.
    assert_eq!(
        start_events, events_out,
        "jsonl stream minus result must equal run events"
    );
}

#[test]
fn compact_view_ships_a_loss_report() {
    // AC-R-2.11.1-3 (second half): the compact view enumerates dropped classes.
    let d = Driver::new(home("compact"));
    dispatch(
        &d,
        &[
            "run",
            "start",
            "--run",
            "cmp",
            "--tokens",
            "1000",
            "--model-calls",
            "10",
            "--time-ms",
            "60000",
            "--jsonl",
        ],
    );
    let (class, out, _e) = dispatch(&d, &["run", "events", "--run", "cmp", "--view", "compact"]);
    assert_eq!(class, ExitClass::Ok);
    assert!(out.contains("loss_report"));
    // context.assembled is a dropped class in the compact projection.
    assert!(out.contains("context.assembled"));
}

#[test]
fn no_tty_is_unattended_tty_inferred_and_asks_are_policy_decided() {
    // AC-R-2.11.1-6
    let d = Driver::new(home("attend"));
    let (class, out, err) = dispatch(
        &d,
        &[
            "run",
            "start",
            "--run",
            "att",
            "--tokens",
            "1000",
            "--model-calls",
            "10",
            "--time-ms",
            "60000",
            "--jsonl",
        ],
    );
    assert_eq!(class, ExitClass::Ok);
    // stderr progress records the derived attendance.
    assert!(err.contains("attendance=unattended"));
    assert!(err.contains("source=tty_inferred"));
    assert!(err.contains("approval=unattended_deny"));
    // every tool `ask` is decided by policy with reason unattended.
    let permits: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("security.permission.decided"))
        .collect();
    assert!(!permits.is_empty());
    for p in &permits {
        assert!(p.contains("\"decider\":\"policy\""));
        assert!(p.contains("\"reason\":\"unattended\""));
    }
    // The dispatch returned without ever being given a stdin — nothing blocked.
}

#[test]
fn missing_budget_is_invocation_error_and_opens_no_run() {
    // §7 D5 / MissingBudget
    let d = Driver::new(home("nobudget"));
    let (class, out, err) = dispatch(&d, &["run", "start", "--run", "nb", "--jsonl"]);
    assert_eq!(class, ExitClass::InvocationError);
    assert!(err.contains("MissingBudget"));
    assert!(out.is_empty(), "no run opened, so no event stream");
    // and no trace file exists for the run
    let (status_class, _o, e2) = dispatch(&d, &["run", "status", "--run", "nb"]);
    assert_eq!(status_class, ExitClass::InvocationError);
    assert!(e2.contains("UnknownRun"));
}

#[test]
fn invalid_definition_is_validation_error_and_opens_no_run() {
    // A validation_error never opens a run. The baseline definition is only invalid with an
    // empty task, so drive `definition validate` with a blank task.
    let d = Driver::new(home("valid"));
    let (class, out, _e) = dispatch(&d, &["definition", "validate", "--task", "   "]);
    assert_eq!(class, ExitClass::ValidationError);
    assert!(out.contains("\"valid\":false"));
}

#[test]
fn definition_validate_accepts_the_baseline() {
    let d = Driver::new(home("valid-ok"));
    let (class, out, _e) = dispatch(&d, &["definition", "validate", "--task", "write and read"]);
    assert_eq!(class, ExitClass::Ok);
    assert!(out.contains("\"valid\":true"));
}

#[test]
fn budget_exhausted_run_exits_budget_exhausted_class() {
    let d = Driver::new(home("exhaust"));
    // model-calls 1 forces exhaustion before the task completes.
    let (class, out, _e) = dispatch(
        &d,
        &[
            "run",
            "start",
            "--run",
            "bx",
            "--tokens",
            "1000",
            "--model-calls",
            "1",
            "--time-ms",
            "60000",
            "--jsonl",
        ],
    );
    assert_eq!(class, ExitClass::BudgetExhausted);
    assert!(out.contains("budget_exhausted{model_calls}"));
    assert!(out.contains("\"exit_class\":\"budget_exhausted\""));
}

#[test]
fn version_and_doctor_and_hello_are_ok() {
    let d = Driver::new(home("meta"));
    let (vc, vout, _ve) = dispatch(&d, &["version"]);
    assert_eq!(vc, ExitClass::Ok);
    assert!(vout.contains("schema_hash"));
    assert!(vout.contains("sha256:"));

    let (dc, dout, _de) = dispatch(&d, &["doctor"]);
    assert_eq!(dc, ExitClass::Ok);
    assert_eq!(dout.trim(), "ok");

    let (hc, hout, _he) = dispatch(&d, &["hello"]);
    assert_eq!(hc, ExitClass::Ok);
    assert!(hout.contains("contract_major"));
    assert!(hout.contains("kernel_version_id"));
}

#[test]
fn unknown_command_is_invocation_error() {
    let d = Driver::new(home("unknown"));
    let (class, _o, _e) = dispatch(&d, &["frobnicate"]);
    assert_eq!(class, ExitClass::InvocationError);
}

#[test]
fn run_status_reports_the_terminal_stop_reason() {
    let d = Driver::new(home("status"));
    dispatch(
        &d,
        &[
            "run",
            "start",
            "--run",
            "st",
            "--tokens",
            "1000",
            "--model-calls",
            "10",
            "--time-ms",
            "60000",
            "--jsonl",
        ],
    );
    let (class, out, _e) = dispatch(&d, &["run", "status", "--run", "st"]);
    assert_eq!(class, ExitClass::Ok);
    assert!(out.contains("\"stop_reason\":\"completed\""));
    assert!(out.contains("\"turns\":3"));
}
