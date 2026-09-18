//! The CLI's three permitted kinds of code (ADR-0167 D1): argument
//! mapping onto typed boundary records, invocation of `hh-embed/1`
//! operation sequences, and rendering of ledger projections, prompts and
//! terminal chrome. No kernel semantics live here (K-2): every command
//! names its boundary sequence in its doc comment, and the conformance
//! fixtures replay that sequence verbatim (AC-R-2.11.1-2).
//!
//! Live-notification discipline: the durable ledger (`read`) is the
//! truth; `upcall.*` asks are pulled exactly as many times as asks the
//! ledger shows were minted (each `security.permission.pending` queues
//! one `upcall.request_permission` while `serves_permission_channel` is
//! negotiated), so the loop never blocks on a quiet pipe — M-1's spirit
//! applied to the wire as well as the terminal.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use hh_embed_client_generated::{
    AttendanceDeclaration, AttendanceValue, InvocationRecord, OutputFormat, Override,
    PermissionOutcome, Session,
};
use hh_wire::json::Json;

use crate::boundary::{Boundary, CliError, ProcessBoundary};
use crate::exit_class::{
    exit_class_for_kernel_error, exit_class_for_terminal, terminal_of, ExitClass,
};
use crate::invocation::{
    build_invocation, bypass_without_containment, missing_budget, parse_attendance, parse_format,
    resolve_attendance, resolve_format, stdin_digest, text_block, InvocationError, Tty,
};

/// The attended-mode prompt — a labelled `stderr` ask answered on `stdin`.
pub type PromptFn<'a> = &'a mut dyn FnMut(&str) -> Option<String>;

/// The process environment the CLI runs in — injected so tests drive the
/// TTY facts, the stdin bytes and the prompt channel deterministically.
pub struct Io<'a> {
    /// The TTY facts the attendance declaration reads.
    pub tty: Tty,
    /// The piped stdin bytes (when stdin is not a terminal). `None` on a
    /// TTY — the invocation never reads a TTY as piped input (M-1).
    pub stdin: Option<Vec<u8>>,
    /// The `cwd_ref` the `InvocationRecord` pins.
    pub cwd_ref: String,
    /// The `principal` the `InvocationRecord` names.
    pub principal: String,
    /// `HH_STORE_ROOT`/`HH_WORKSPACE_ROOT`-class env for the kernel
    /// child (binding (b)), and `HH_KERNEL_CMD` lookup.
    pub kernel_env: Vec<(String, String)>,
    /// The kernel command for binding (b) (default `hh-kernel`).
    pub kernel_cmd: String,
    /// stdout — the command's result channel only (ADR-0169 D1).
    pub out: &'a mut dyn Write,
    /// stderr — progress, warnings, prompts.
    pub err: &'a mut dyn Write,
    /// The prompt channel — reads one answer line from the controlling
    /// terminal (the rendered ask is written to `err` first). `None`
    /// answers a prompt as EOF → the ask stays parked and the CLI
    /// detaches; it never blocks a pipe.
    pub prompt: Option<PromptFn<'a>>,
}

impl Io<'_> {
    fn env_lookup(&self, name: &str) -> Option<String> {
        self.kernel_env
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    }
}

/// One command's outcome — the exit class (process numeral via
/// `ExitClass::code`; the authoritative string in `result.exit_class`)
/// and the typed result record.
pub struct CliOutcome {
    /// The authoritative exit class.
    pub class: ExitClass,
    /// The `{kind:"result", exit_class, …}` terminal record.
    pub result: Json,
}

// ── entry ──────────────────────────────────────────────────────────────

/// The binary's entry: spawn the binding-(b) kernel child, run the
/// command.
pub fn run(argv: &[String], io: &mut Io) -> CliOutcome {
    let parsed = match parse_args(argv) {
        Ok(p) => p,
        Err(e) => return finish(io, Err(CliError::Invocation(e)), None),
    };
    let cmd = parsed
        .flag("kernel-cmd")
        .or_else(|| io.env_lookup("HH_KERNEL_CMD"))
        .unwrap_or_else(|| io.kernel_cmd.clone());
    let mut b = match ProcessBoundary::spawn(&cmd, &io.kernel_env) {
        Ok(b) => b,
        Err(e) => return finish(io, Err(e), None),
    };
    run_with(&mut b, argv, io)
}

/// Run one command over an already-connected boundary — the generated
/// client over `hh-kernel serve`, or the in-process service in the
/// conformance suite (identical behaviour, AC-K4-2).
pub fn run_with(b: &mut dyn Boundary, argv: &[String], io: &mut Io) -> CliOutcome {
    let parsed = match parse_args(argv) {
        Ok(p) => p,
        Err(e) => return finish(io, Err(CliError::Invocation(e)), None),
    };
    match dispatch(&parsed, b, io, argv) {
        Ok((outcome, format)) => finish(io, Ok(outcome), Some(&format)),
        Err((e, format)) => finish(io, Err(e), Some(&format)),
    }
}

/// Render + return the outcome — exactly one terminal `result` record on
/// stdout (`json`/`jsonl`) or a human summary line (`human`).
fn finish(
    io: &mut Io,
    r: Result<CliOutcome, CliError>,
    format: Option<&OutputFormat>,
) -> CliOutcome {
    let out = match r {
        Ok(o) => o,
        Err(e) => err_of(e),
    };
    emit_result(io, &out, format);
    out
}

/// The `CliError` → outcome mapping (the exit-class table's error rows).
fn err_of(e: CliError) -> CliOutcome {
    match e {
        CliError::Invocation(e) => CliOutcome {
            class: ExitClass::InvocationError,
            result: result_record("invocation_error", &e.to_json(), ExitClass::InvocationError),
        },
        CliError::Kernel(e) => {
            let class = exit_class_for_kernel_error(&e);
            CliOutcome {
                class,
                result: result_record(
                    "kernel_error",
                    &Json::obj([
                        ("kind", Json::str(e.kind.clone())),
                        ("code", Json::Int(e.code)),
                        ("message", Json::str(e.message.clone())),
                        ("data", e.data.clone()),
                    ]),
                    class,
                ),
            }
        }
        CliError::Transport(m) => CliOutcome {
            class: ExitClass::InfrastructureFailure,
            result: result_record(
                "infrastructure_failure",
                &Json::obj([("message", Json::str(m))]),
                ExitClass::InfrastructureFailure,
            ),
        },
    }
}

fn emit_result(io: &mut Io, out: &CliOutcome, format: Option<&OutputFormat>) {
    match format {
        Some(OutputFormat::Human) => {
            let _ = writeln!(io.out, "result: {}", out.result.to_canonical_string());
        }
        _ => {
            let _ = writeln!(io.out, "{}", out.result.to_canonical_string());
        }
    }
}

/// The `{kind:"result", result_kind, exit_class, payload}` record — the
/// one record `json` prints and the `jsonl` stream's terminal line
/// (ADR-0169 D2/D3).
fn result_record(kind: &str, payload: &Json, class: ExitClass) -> Json {
    Json::obj([
        ("kind", Json::str("result")),
        ("result_kind", Json::str(kind)),
        ("exit_class", Json::str(class.as_str())),
        ("payload", payload.clone()),
    ])
}

// ── argument mapping ───────────────────────────────────────────────────

/// Parsed invocation — `hh <noun> <verb> [positional…] [--flag value]*
/// [--switch]*`. No catch-all, no abbreviation matching (N-1).
pub struct Parsed {
    /// The noun (`run`, `approval`, `version`, `doctor`).
    pub noun: String,
    /// The verb (`start`, `events`, `respond`, …); empty for
    /// single-word commands (`version`, `doctor`).
    pub verb: String,
    /// Positional arguments after `noun verb`.
    pub positional: Vec<String>,
    /// `--flag value` / `--flag=value` (repeatable).
    pub flags: BTreeMap<String, Vec<String>>,
    /// Boolean switches.
    pub switches: BTreeSet<String>,
}

/// Flags that take a value — the closed set; an unknown `--x` is an
/// `invocation_error`, never silently ignored.
const VALUE_FLAGS: &[&str] = &[
    "definition",
    "format",
    "attendance",
    "approval-mode",
    "budget",
    "override",
    "capability",
    "idempotency-key",
    "kernel-cmd",
    "mode",
    "at-seq",
    "at-event",
    "from-seq",
    "from",
    "limit",
    "turn",
    "view",
    "target",
    "value",
    "attestation",
];

const SWITCHES: &[&str] = &["no-input", "bypass", "takeover", "follow", "cancel", "help"];

impl Parsed {
    /// The first value of `--name`.
    pub fn flag(&self, name: &str) -> Option<String> {
        self.flags.get(name).and_then(|v| v.first()).cloned()
    }
    /// All values of a repeatable `--name`.
    pub fn flag_all(&self, name: &str) -> Vec<String> {
        self.flags.get(name).cloned().unwrap_or_default()
    }
    /// Whether `--name` was given.
    pub fn has(&self, name: &str) -> bool {
        self.switches.contains(name)
    }
}

fn parse_args(argv: &[String]) -> Result<Parsed, InvocationError> {
    let mut it = argv.iter().skip(1).peekable();
    let noun = it.next().cloned().unwrap_or_default();
    if noun.is_empty() || noun == "help" || noun == "--help" || noun == "-h" {
        return Err(InvocationError::at(
            "usage",
            "hh",
            "usage: hh run <verb> | hh approval <verb> | hh version | hh doctor",
        ));
    }
    let single = matches!(noun.as_str(), "version" | "doctor");
    let verb = if single {
        noun.clone()
    } else {
        it.next().cloned().unwrap_or_default()
    };
    let mut p = Parsed {
        noun,
        verb,
        positional: Vec::new(),
        flags: BTreeMap::new(),
        switches: BTreeSet::new(),
    };
    while let Some(a) = it.next() {
        if let Some(long) = a.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((n, v)) => (n.to_string(), Some(v.to_string())),
                None => (long.to_string(), None),
            };
            if SWITCHES.contains(&name.as_str()) {
                if inline.is_some() {
                    return Err(InvocationError::at(
                        "flag_conflict",
                        &format!("--{name}"),
                        &format!("--{name} takes no value"),
                    ));
                }
                p.switches.insert(name);
                continue;
            }
            if VALUE_FLAGS.contains(&name.as_str()) {
                let v = match inline {
                    Some(v) => v,
                    None => it.next().cloned().ok_or_else(|| {
                        InvocationError::at(
                            "missing_flag_value",
                            &format!("--{name}"),
                            &format!("--{name} needs a value"),
                        )
                    })?,
                };
                p.flags.entry(name).or_default().push(v);
                continue;
            }
            return Err(InvocationError::at(
                "unknown_flag",
                &format!("--{name}"),
                "unknown flag — the flag set is closed (N-1)",
            ));
        }
        p.positional.push(a.clone());
    }
    Ok(p)
}

// ── shared helpers ─────────────────────────────────────────────────────

fn inv(code: &str, path: &str, remedy: &str) -> CliError {
    CliError::Invocation(InvocationError::at(code, path, remedy))
}

/// Positional argument or an `invocation_error`.
fn require_pos(p: &Parsed, i: usize, name: &str) -> Result<String, CliError> {
    p.positional
        .get(i)
        .cloned()
        .ok_or_else(|| inv("missing_argument", name, &format!("missing {name}")))
}

/// The `idempotency_key` for the command's boundary calls — the
/// invocation record's key for the work-injecting op, a deterministic
/// `:step` suffix inside one command (one invocation = one idempotent
/// sequence, ADR-0169 D5).
fn step_key(inv: &InvocationRecord, step: &str) -> String {
    format!("{}:{step}", inv.idempotency_key)
}

/// `open_session{attach, read_only}` — the session for Group R reads.
/// The idempotency key is scoped to *this* invocation (`inv`) — an
/// attach handle is per-invocation, and a shared key would replay a
/// `close`d handle onto a `project` that answers `UnknownSession`.
fn attach(b: &mut dyn Boundary, run_id: &str, inv: &InvocationRecord) -> Result<Session, CliError> {
    let raw = b.call(
        "open_session",
        &Json::obj([
            (
                "spec",
                Json::obj([
                    ("kind", Json::str("attach")),
                    ("run_id", Json::str(run_id)),
                    ("read_only", Json::Bool(true)),
                ]),
            ),
            (
                "idempotency_key",
                Json::str(format!("attach:{run_id}:{}", inv.idempotency_key)),
            ),
        ]),
    )?;
    decode_session(&raw)
}

/// `open_session{resume}` — the writer session every work-injecting
/// command on a live run needs (`continue` surfaces `WouldBlock` while a
/// writer holds the lease; `--takeover` fences it). `invocation` rides
/// along so `lifecycle.surface.invoked` lands durable (§3).
fn resume_session(
    b: &mut dyn Boundary,
    run_id: &str,
    mode: &str,
    invocation: Option<&InvocationRecord>,
) -> Result<Session, CliError> {
    let mut params = Json::obj([
        (
            "spec",
            Json::obj([
                ("kind", Json::str("resume")),
                ("run_id", Json::str(run_id)),
                ("mode", Json::str(mode)),
            ]),
        ),
        (
            "idempotency_key",
            // Scoped to this invocation — a bare `resume:{run}:{mode}`
            // key would replay a detached (or removed) session onto the
            // next command's writer ops (`UnknownSession`/`Fenced`).
            Json::str(format!(
                "resume:{run_id}:{mode}:{}",
                invocation
                    .map(|i| i.idempotency_key.as_str())
                    .unwrap_or("detached")
            )),
        ),
    ]);
    if let Some(inv) = invocation {
        if let Json::Obj(m) = &mut params {
            m.insert("invocation".into(), inv.to_json());
        }
    }
    let raw = b.call("open_session", &params)?;
    decode_session(&raw)
}

fn decode_session(raw: &Json) -> Result<Session, CliError> {
    Session::from_json(raw).map_err(|e| CliError::Transport(format!("session decode: {e}")))
}

/// `close{done}` — detach the session the command opened.
fn close_session(b: &mut dyn Boundary, session_id: &str) {
    let _ = b.call(
        "close",
        &Json::obj([
            ("session_id", Json::str(session_id)),
            ("reason", Json::str("done")),
        ]),
    );
}

// ── the attended loop ──────────────────────────────────────────────────

/// State the loop accumulates while driving to the terminal.
struct LoopState {
    /// Durable events seen (`read` is the truth).
    events: Vec<Json>,
    /// Buffered `upcall.*` asks not yet answered, `(method, params)`.
    upcalls: Vec<(String, Json)>,
    /// Permission ids already answered this session.
    answered: BTreeSet<String>,
    /// `control.budget.exceeded` seqs already offered amend/stop.
    escalations: BTreeSet<i64>,
    /// The `lifecycle.run.finished` payload (terminal).
    finished: Option<Json>,
    /// The read watermark.
    watermark: i64,
    /// Whether the negotiated caps include `serves_permission_channel`.
    permission_channel: bool,
}

impl LoopState {
    fn new(b: &dyn Boundary, cursor: i64) -> LoopState {
        let permission_channel = b
            .hello_result()
            .map(|h| h.negotiated.serves_permission_channel)
            .unwrap_or(false);
        LoopState {
            events: Vec::new(),
            upcalls: Vec::new(),
            answered: BTreeSet::new(),
            escalations: BTreeSet::new(),
            finished: None,
            watermark: cursor + 1,
            permission_channel,
        }
    }
}

/// `read{cursor:Seq{watermark}}` forward — every new durable row is
/// rendered (`jsonl` → the canonical envelope line on stdout; `human` →
/// a stderr progress line) and watched for the terminal.
fn read_new(
    b: &mut dyn Boundary,
    session_id: &str,
    st: &mut LoopState,
    io: &mut Io,
    format: &OutputFormat,
) -> Result<(), CliError> {
    // The ledger refuses a cursor past head (`UnknownCursor`) — a poll
    // at the drained tip must ask `head` first rather than eat the
    // refusal. One `head` op per pass keeps the loop bounded and
    // honest (no error-string sniffing).
    let tip = b
        .call("head", &Json::obj([("session_id", Json::str(session_id))]))?
        .get("seq")
        .and_then(Json::as_int)
        .unwrap_or(-1);
    if st.watermark > tip {
        return Ok(());
    }
    loop {
        let raw = b.call(
            "read",
            &Json::obj([
                ("session_id", Json::str(session_id)),
                (
                    "cursor",
                    Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(st.watermark))]),
                ),
                ("direction", Json::str("fwd")),
                ("limit", Json::Int(256)),
            ]),
        )?;
        let evs = match raw.get("events") {
            Some(Json::Arr(a)) => a.clone(),
            _ => vec![],
        };
        let mut advanced = false;
        for e in evs {
            let seq = e.get("seq").and_then(Json::as_int).unwrap_or(st.watermark);
            if seq >= st.watermark {
                st.watermark = seq + 1;
                advanced = true;
            }
            render_event(io, format, &e);
            if e.get("class").and_then(Json::as_str) == Some("lifecycle.run.finished") {
                st.finished = e.get("payload").cloned();
            }
            st.events.push(e);
        }
        let more = matches!(raw.get("next"), Some(Json::Obj(_)));
        if !(more && advanced) {
            return Ok(());
        }
    }
}

fn render_event(io: &mut Io, format: &OutputFormat, event: &Json) {
    match format {
        OutputFormat::Jsonl => {
            let _ = writeln!(io.out, "{}", event.to_canonical_string());
        }
        OutputFormat::Human => {
            let seq = event.get("seq").and_then(Json::as_int).unwrap_or(0);
            let class = event.get("class").and_then(Json::as_str).unwrap_or("");
            let _ = writeln!(io.err, "  #{seq} {class}");
        }
        OutputFormat::Json => {}
    }
}

/// `security.permission.pending` ids with no matching
/// `security.permission.decided` in the seen prefix.
fn undecided_permissions(events: &[Json]) -> Vec<String> {
    let mut pending: Vec<String> = Vec::new();
    let mut decided: BTreeSet<String> = BTreeSet::new();
    for e in events {
        let pid = e
            .get("payload")
            .and_then(|p| p.get("permission_id"))
            .and_then(Json::as_str);
        match (e.get("class").and_then(Json::as_str).unwrap_or(""), pid) {
            ("security.permission.pending", Some(p)) => pending.push(p.to_string()),
            ("security.permission.decided", Some(p)) => {
                decided.insert(p.to_string());
            }
            _ => {}
        }
    }
    pending
        .into_iter()
        .filter(|p| !decided.contains(p))
        .collect()
}

/// Pull exactly `n` `upcall.*` notifications — the count the ledger
/// predicts (one `upcall.request_permission` per `pending` while the
/// channel is negotiated), so a quiet pipe is never over-read.
fn pull_upcalls(b: &mut dyn Boundary, st: &mut LoopState, n: usize) -> Result<(), CliError> {
    for _ in 0..n {
        if let Some(u) = b.poll_upcall()? {
            st.upcalls.push(u);
        }
    }
    Ok(())
}

/// The ask the prompt renders — options and proposal from the
/// `upcall.request_permission` params, falling back to the Stage-1
/// interim option set (OQ-246: `allow_once`/`deny_once` only) when the
/// channel wasn't negotiated.
fn find_ask(st: &LoopState, permission_id: &str) -> (String, Vec<String>, Option<Json>) {
    for (m, p) in &st.upcalls {
        if m == "upcall.request_permission"
            && p.get("permission_id").and_then(Json::as_str) == Some(permission_id)
        {
            let options = match p.get("options") {
                Some(Json::Arr(a)) => a
                    .iter()
                    .filter_map(|o| {
                        o.get("option_id")
                            .and_then(Json::as_str)
                            .or_else(|| o.as_str())
                            .map(String::from)
                    })
                    .collect(),
                _ => vec![],
            };
            let proposal = p
                .get("proposal")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            return (proposal, options, Some(p.clone()));
        }
    }
    (
        String::new(),
        vec!["allow_once".into(), "deny_once".into()],
        None,
    )
}

/// Render the approval prompt (AC-R-2.11.1-12): the ask's fields and
/// every `Explanation` member verbatim, the untruncated proposal,
/// `model_justification` visibly `delegate`-labelled, the offered option
/// ids — the CLI never invents options.
fn render_ask(io: &mut Io, pid: &str, proposal: &str, options: &[String], up: Option<&Json>) {
    let _ = writeln!(io.err, "── approval requested ──────────────────────");
    let _ = writeln!(io.err, "permission_id: {pid}");
    if !proposal.is_empty() {
        let _ = writeln!(io.err, "proposal: {proposal}");
    }
    if let Some(p) = up {
        if let Some(text) = p
            .get("rendering")
            .and_then(|r| r.get("text"))
            .and_then(Json::as_str)
        {
            let _ = writeln!(io.err, "rendering: {text}");
        }
        if let Some(expl) = p
            .get("rendering")
            .and_then(|r| r.get("explanation"))
            .or_else(|| p.get("explanation"))
        {
            if let Some(rule) = expl.get("rule_id").and_then(Json::as_str) {
                let _ = writeln!(io.err, "explanation.rule_id: {rule}");
            }
            if let Some(Json::Arr(risks)) = expl.get("risk_factors") {
                for rf in risks.iter().filter_map(Json::as_str) {
                    let _ = writeln!(io.err, "explanation.risk_factor: {rf}");
                }
            }
            if let Some(hint) = expl.get("what_would_auto_approve") {
                let _ = writeln!(
                    io.err,
                    "explanation.what_would_auto_approve: {}",
                    hint.to_canonical_string()
                );
            }
            if let Some(j) = expl.get("model_justification") {
                // `authority = delegate` — visibly labelled, never the
                // principal's words (AC-12).
                let _ = writeln!(
                    io.err,
                    "explanation.model_justification [delegate]: {}",
                    j.to_canonical_string()
                );
            }
        }
    }
    let _ = writeln!(io.err, "options: {}", options.join(" | "));
}

/// The attended drive loop (§2.3 attended terminal): after each
/// work-injecting call the kernel parks on an ask, an exhaustion
/// decision point, or the terminal — the CLI answers through
/// `respond_permission` / `amend` / `cancel`, boundary ops all, never a
/// local decision (K-1). Returns the `lifecycle.run.finished` payload,
/// or `None` when the run stayed parked (the CLI detaches honestly).
fn attend(
    b: &mut dyn Boundary,
    io: &mut Io,
    sess: &Session,
    format: &OutputFormat,
    interactive: bool,
    inv: &InvocationRecord,
    st: &mut LoopState,
) -> Result<Option<Json>, CliError> {
    let mut seen_pendings = undecided_permissions(&st.events).len();
    // Open-time asks were queued during `open_session` — pull exactly
    // that many upcalls now (binding (b) flushed them after the
    // response; binding (a) drains the same queue).
    if st.permission_channel && seen_pendings > 0 {
        pull_upcalls(b, st, seen_pendings)?;
    }
    let mut parked = 0usize;
    loop {
        read_new(b, &sess.session_id, st, io, format)?;
        if st.finished.is_some() {
            return Ok(st.finished.clone());
        }
        let mut acted = false;

        // ── open asks ────────────────────────────────────────────────
        let undecided = undecided_permissions(&st.events);
        if interactive && st.permission_channel && undecided.len() > seen_pendings {
            pull_upcalls(b, st, undecided.len() - seen_pendings)?;
            seen_pendings = undecided.len();
        }
        for pid in &undecided {
            if st.answered.contains(pid) || !interactive {
                continue;
            }
            let (proposal, options, up) = find_ask(st, pid);
            render_ask(io, pid, &proposal, &options, up.as_ref());
            let answer = io
                .prompt
                .as_mut()
                .and_then(|p| p("option> "))
                .unwrap_or_default()
                .trim()
                .to_string();
            if answer.is_empty() {
                // EOF — leave the ask parked and detach honestly. Mark
                // it offered so a later attend pass in this invocation
                // doesn't re-prompt (and eat a different decision
                // point's answer off the queue).
                st.answered.insert(pid.clone());
                continue;
            }
            let outcome = if answer == "cancel" {
                PermissionOutcome::Cancelled
            } else {
                PermissionOutcome::Selected {
                    option_id: answer.clone(),
                }
            };
            match b.call(
                "respond_permission",
                &Json::obj([
                    ("session_id", Json::str(sess.session_id.clone())),
                    ("permission_id", Json::str(pid.clone())),
                    ("outcome", outcome.to_json()),
                    (
                        "idempotency_key",
                        Json::str(step_key(inv, &format!("respond:{pid}"))),
                    ),
                ]),
            ) {
                Ok(_) => {
                    st.answered.insert(pid.clone());
                    acted = true;
                }
                Err(CliError::Kernel(e)) if e.kind == "AlreadyDecided" => {
                    st.answered.insert(pid.clone());
                }
                Err(e) => return Err(e),
            }
        }

        // ── exhaustion decision points ───────────────────────────────
        let exceeded: Vec<(i64, String, i64)> = st
            .events
            .iter()
            .filter(|e| e.get("class").and_then(Json::as_str) == Some("control.budget.exceeded"))
            .map(|e| {
                (
                    e.get("seq").and_then(Json::as_int).unwrap_or(0),
                    e.get("payload")
                        .and_then(|p| p.get("dimension"))
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string(),
                    e.get("payload")
                        .and_then(|p| p.get("limit"))
                        .and_then(Json::as_int)
                        .unwrap_or(0),
                )
            })
            .collect();
        for (seq, dim, limit) in exceeded {
            if st.escalations.contains(&seq) || !interactive {
                continue;
            }
            st.escalations.insert(seq);
            let _ = writeln!(
                io.err,
                "── budget exhausted: dimension {dim} reached {limit} ──"
            );
            let _ = writeln!(
                io.err,
                "options: amend <new-cap> | stop — a ledgered amend(budget) or stop{{budget_exhausted}}"
            );
            let answer = io
                .prompt
                .as_mut()
                .and_then(|pr| pr("amend|stop> "))
                .unwrap_or_default()
                .trim()
                .to_string();
            if let Ok(new_cap) = answer.parse::<i64>() {
                let mut dims = BTreeMap::new();
                dims.insert(dim.clone(), Json::Int(new_cap));
                b.call(
                    "amend",
                    &Json::obj([
                        ("session_id", Json::str(sess.session_id.clone())),
                        ("target", Json::str("budget")),
                        ("value", Json::obj([("dimensions", Json::Obj(dims))])),
                        (
                            "idempotency_key",
                            Json::str(step_key(inv, &format!("amend:{dim}:{new_cap}"))),
                        ),
                        ("invocation", inv.to_json()),
                    ]),
                )?;
                acted = true;
            } else if answer == "stop" || answer == "s" {
                b.call(
                    "cancel",
                    &Json::obj([
                        ("session_id", Json::str(sess.session_id.clone())),
                        ("scope", Json::obj([("kind", Json::str("run"))])),
                        (
                            "idempotency_key",
                            Json::str(step_key(inv, "cancel:exhausted")),
                        ),
                    ]),
                )?;
                acted = true;
            }
        }

        if acted {
            parked = 0;
            continue;
        }
        // Nothing to answer, nothing finished — the run is parked on a
        // cue only another invocation can supply. Detach honestly.
        parked += 1;
        if parked >= 2 {
            return Ok(None);
        }
    }
}

// ── resolved invocation facts ──────────────────────────────────────────

/// Attendance + format + the `InvocationRecord` — shared by commands.
struct Resolved {
    attendance: AttendanceDeclaration,
    format: OutputFormat,
    invocation: InvocationRecord,
}

fn resolve(
    p: &Parsed,
    io: &Io,
    argv: &[String],
    streaming: bool,
    stdin_used: bool,
) -> Result<Resolved, CliError> {
    let declared = match p.flag("attendance") {
        Some(a) => Some(parse_attendance(&a).map_err(CliError::Invocation)?),
        None => None,
    };
    let attendance =
        resolve_attendance(declared, p.has("no-input"), io.tty).map_err(CliError::Invocation)?;
    let format = resolve_format(
        p.flag("format")
            .map(|f| parse_format(&f))
            .transpose()
            .map_err(CliError::Invocation)?,
        streaming,
        io.tty.stdout,
    )
    .map_err(CliError::Invocation)?;
    let digest = if stdin_used {
        io.stdin.as_deref().map(stdin_digest)
    } else {
        None
    };
    let invocation = build_invocation(
        argv,
        &io.cwd_ref,
        &io.principal,
        attendance.clone(),
        format,
        digest,
        instrument_record(io),
        p.flag("idempotency-key"),
    );
    Ok(Resolved {
        attendance,
        format,
        invocation,
    })
}

/// The `InstrumentRecord` member — whether credentials were supplied is
/// recorded (`provided`), never the credentials.
fn instrument_record(io: &Io) -> Json {
    Json::obj([
        (
            "cli",
            Json::str(concat!("hh-cli/", env!("CARGO_PKG_VERSION"))),
        ),
        ("kernel_cmd", Json::str(io.kernel_cmd.clone())),
        ("credentials_provided", Json::Bool(false)),
    ])
}

// ── dispatch ───────────────────────────────────────────────────────────

type CmdResult = Result<(CliOutcome, OutputFormat), (CliError, OutputFormat)>;

fn dispatch(p: &Parsed, b: &mut dyn Boundary, io: &mut Io, argv: &[String]) -> CmdResult {
    let go = |r: Result<(CliOutcome, OutputFormat), CliError>| -> CmdResult {
        r.map_err(|e| (e, OutputFormat::Json))
    };
    match (p.noun.as_str(), p.verb.as_str()) {
        ("version", _) => go(cmd_version(b, io)),
        ("doctor", _) => go(cmd_doctor(b, io)),
        ("run", "start") => go(cmd_run_start(b, io, p, argv)),
        ("run", "events") => go(cmd_run_events(b, io, p, argv)),
        ("run", "status") => go(cmd_run_status(b, io, p, argv)),
        ("run", "inspect") => go(cmd_run_inspect(b, io, p, argv)),
        ("run", "resume") => go(cmd_run_resume(b, io, p, argv)),
        ("run", "fork") => go(cmd_run_fork(b, io, p, argv)),
        ("run", "cancel") => go(cmd_run_cancel(b, io, p, argv)),
        ("run", "amend") => go(cmd_run_amend(b, io, p, argv)),
        ("run", "submit") => go(cmd_run_submit(b, io, p, argv)),
        ("approval", "list") => go(cmd_approval_list(b, io, p, argv)),
        ("approval", "show") => go(cmd_approval_show(b, io, p, argv)),
        ("approval", "respond") => go(cmd_approval_respond(b, io, p, argv)),
        (noun, verb) => Err((
            CliError::Invocation(InvocationError::at(
                "unknown_command",
                &format!("{noun} {verb}"),
                "unknown command — no catch-all, no abbreviation matching (N-1)",
            )),
            OutputFormat::Json,
        )),
    }
}

fn ok_outcome(
    kind: &str,
    payload: Json,
    format: OutputFormat,
) -> Result<(CliOutcome, OutputFormat), CliError> {
    Ok((
        CliOutcome {
            class: ExitClass::Ok,
            result: result_record(kind, &payload, ExitClass::Ok),
        },
        format,
    ))
}

// ── run start ──────────────────────────────────────────────────────────

/// `run start <definition|ref:…> [prompt|-]` → `open_session{new}` +
/// `submit` + the attended loop. The one command that opens a run; every
/// pre-ledger check (MissingBudget, BypassWithoutContainment, M-1, I-1
/// via the boundary) fires before it does.
fn cmd_run_start(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &Parsed,
    argv: &[String],
) -> Result<(CliOutcome, OutputFormat), CliError> {
    // ── pre-ledger checks (invocation_error, never opens a run) ──────
    bypass_without_containment(p.has("bypass")).map_err(CliError::Invocation)?;
    let definition = definition_input(p).map_err(CliError::Invocation)?;
    missing_budget(
        &definition_document(&definition).unwrap_or(Json::Null),
        p.flag("budget").is_some(),
    )
    .map_err(CliError::Invocation)?;

    // ── input blocks (D4 origin typing) ──────────────────────────────
    let mut input: Vec<Json> = Vec::new();
    let prompt_arg = p.positional.get(1).cloned();
    let mut stdin_used = false;
    match prompt_arg.as_deref() {
        Some("-") => {
            stdin_used = true;
            if let Some(bytes) = &io.stdin {
                input.push(text_block(
                    &String::from_utf8_lossy(bytes),
                    "external",
                    "import",
                ));
            }
        }
        Some(text) => input.push(text_block(text, "principal", "human")),
        None => {}
    }
    // Piped stdin alongside a positional prompt is a separate
    // `external{origin: import}` context item — never `principal`
    // (D4, the rules-file backdoor row).
    if io.stdin.is_some() && prompt_arg.as_deref() != Some("-") {
        stdin_used = true;
        if let Some(bytes) = &io.stdin {
            input.push(text_block(
                &String::from_utf8_lossy(bytes),
                "external",
                "import",
            ));
        }
    }
    if input.is_empty() {
        input.push(text_block("", "principal", "human"));
    }

    // ── overrides / budget / capabilities ────────────────────────────
    let overrides = parse_overrides(&p.flag_all("override")).map_err(CliError::Invocation)?;
    let budget = p
        .flag("budget")
        .map(|s| parse_budget(&s))
        .transpose()
        .map_err(CliError::Invocation)?;
    let host_caps: Vec<Json> = p
        .flag_all("capability")
        .iter()
        .map(|s| {
            hh_wire::json::parse(s)
                .map_err(|e| inv("bad_capability", "--capability", &e.to_string()))
        })
        .collect::<Result<_, _>>()?;

    // ── attendance + format + invocation record ──────────────────────
    let r = resolve(p, io, argv, true, stdin_used)?;
    let approval_mode = p.flag("approval-mode").or_else(|| {
        Some(match r.attendance.value {
            AttendanceValue::Interactive => "tiered".to_string(),
            _ => "unattended_deny".to_string(),
        })
    });

    // ── open_session{new} → submit → attend ──────────────────────────
    let mut spec = Json::obj([
        ("kind", Json::str("new")),
        ("definition", definition),
        (
            "overrides",
            Json::Arr(overrides.iter().map(|o| o.to_json()).collect()),
        ),
        (
            "environment",
            Json::obj([
                ("kind", Json::str("connection_info")),
                (
                    "connection_info",
                    Json::obj([("class", Json::str("local_host"))]),
                ),
            ]),
        ),
        ("attendance", r.attendance.to_json()),
        (
            "supplies",
            Json::obj([
                ("context", Json::Arr(vec![])),
                ("host_capabilities", Json::Arr(host_caps)),
                ("mcp_servers", Json::Arr(vec![])),
                ("procedures", Json::Arr(vec![])),
            ]),
        ),
    ]);
    if let Json::Obj(m) = &mut spec {
        if let Some(bud) = &budget {
            m.insert("budget".into(), bud.clone());
        }
        if let Some(am) = &approval_mode {
            m.insert("approval_mode".into(), Json::str(am.clone()));
        }
    }
    let sess_raw = b.call(
        "open_session",
        &Json::obj([
            ("spec", spec),
            (
                "idempotency_key",
                Json::str(r.invocation.idempotency_key.clone()),
            ),
            ("invocation", r.invocation.to_json()),
        ]),
    )?;
    let sess = decode_session(&sess_raw)?;

    let mut st = LoopState::new(b, -1);
    let interactive = r.attendance.value == AttendanceValue::Interactive;
    // Attend open-time asks before injecting work — `open_session`
    // minted them durably and the upcalls are already queued. On an
    // idempotent replay the boundary hands back an attach handle on the
    // already-run `run_id` and `lifecycle.run.finished` is in the
    // prefix — `submit` is skipped (the same run, not a second one).
    attend(b, io, &sess, &r.format, interactive, &r.invocation, &mut st)?;
    if st.finished.is_none() {
        b.call(
            "submit",
            &Json::obj([
                ("session_id", Json::str(sess.session_id.clone())),
                ("input", Json::Arr(input)),
                (
                    "idempotency_key",
                    Json::str(step_key(&r.invocation, "submit")),
                ),
            ]),
        )?;
    }

    let finished = attend(b, io, &sess, &r.format, interactive, &r.invocation, &mut st)?;
    let finished = finished.or_else(|| st.finished.clone());
    let outcome = terminal_outcome(&sess, &st.events, finished.as_ref());
    // `close{done}` on a finished run is clean teardown; on a parked
    // run it would mint `cancelled{by: principal}` — leave the writer
    // session live for `run resume --takeover`.
    if finished.is_some() {
        close_session(b, &sess.session_id);
    }
    Ok((outcome, r.format))
}

fn definition_input(p: &Parsed) -> Result<Json, InvocationError> {
    let pos0 = p
        .positional
        .first()
        .cloned()
        .or_else(|| p.flag("definition"));
    match pos0.as_deref() {
        None => Err(InvocationError::at(
            "missing_definition",
            "<definition>",
            "run start needs a definition document or ref",
        )),
        Some(s) if s.starts_with("ref:") => Ok(Json::obj([
            ("kind", Json::str("ref")),
            ("ref", Json::str(&s[4..])),
        ])),
        Some(s) => {
            let text = if s.starts_with('{') {
                s.to_string()
            } else {
                std::fs::read_to_string(s).map_err(|e| {
                    InvocationError::at(
                        "definition_unreadable",
                        "<definition>",
                        &format!("cannot read {s}: {e}"),
                    )
                })?
            };
            let doc = hh_wire::json::parse(&text).map_err(|e| {
                InvocationError::at("definition_invalid_json", "<definition>", &e.to_string())
            })?;
            Ok(Json::obj([
                ("kind", Json::str("document")),
                ("document", doc),
            ]))
        }
    }
}

fn definition_document(input: &Json) -> Option<Json> {
    if input.get("kind").and_then(Json::as_str) == Some("document") {
        input.get("document").cloned()
    } else {
        None
    }
}

/// `--override /ptr=value` — `value` parses as JSON when well-formed,
/// else a bare string (the override grammar is assembly-checked
/// boundary-side; the CLI types the pair only).
fn parse_overrides(list: &[String]) -> Result<Vec<Override>, InvocationError> {
    let mut out = Vec::new();
    for item in list {
        let (ptr, val) = item.split_once('=').ok_or_else(|| {
            InvocationError::at(
                "bad_override",
                "--override",
                &format!("expected /ptr=value, got {item:?}"),
            )
        })?;
        if !ptr.starts_with('/') {
            return Err(InvocationError::at(
                "bad_override",
                "--override",
                "override pointers start with /",
            ));
        }
        let value = hh_wire::json::parse(val).unwrap_or_else(|_| Json::str(val));
        out.push(Override {
            pointer: ptr.to_string(),
            value,
        });
    }
    Ok(out)
}

/// `--budget DIM=CAP[,DIM=CAP…]` → `BudgetInput::Node{dimensions:
/// {dim:{hard:cap}}}` — a flag budget is a node, never a ref.
fn parse_budget(s: &str) -> Result<Json, InvocationError> {
    let mut dims = BTreeMap::new();
    for part in s.split(',') {
        let (d, cap) = part.split_once('=').ok_or_else(|| {
            InvocationError::at(
                "bad_budget",
                "--budget",
                &format!("expected DIM=CAP, got {part:?}"),
            )
        })?;
        let n: i64 = cap.parse().map_err(|_| {
            InvocationError::at(
                "bad_budget",
                "--budget",
                &format!("cap {cap:?} is not an integer"),
            )
        })?;
        dims.insert(d.to_string(), Json::obj([("hard", Json::Int(n))]));
    }
    Ok(Json::obj([
        ("kind", Json::str("node")),
        ("node", Json::obj([("dimensions", Json::Obj(dims))])),
    ]))
}

// ── read-only run commands ─────────────────────────────────────────────

/// `run events <run_id> [--from SEQ]` → `attach` + `read` — the
/// canonical durable export (`jsonl`: one canonical envelope per line;
/// the `run start --jsonl` stream is byte-identical, O-1).
fn cmd_run_events(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &Parsed,
    argv: &[String],
) -> Result<(CliOutcome, OutputFormat), CliError> {
    let run_id = require_pos(p, 0, "<run_id>")?;
    let r = resolve(p, io, argv, true, false)?;
    let sess = attach(b, &run_id, &r.invocation)?;
    let mut st = LoopState::new(b, -1);
    st.watermark = p
        .flag("from")
        .or_else(|| p.flag("from-seq"))
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    read_new(b, &sess.session_id, &mut st, io, &r.format)?;
    // Read-only projection — the invocation's class reports the export,
    // not the run's verdict (`run status`/`inspect` behave the same;
    // `lifecycle.run.finished` in the stream carries the terminal).
    let out = CliOutcome {
        class: ExitClass::Ok,
        result: result_record(
            "run_events",
            &Json::obj([
                ("run_id", Json::str(run_id)),
                ("events", Json::Int(st.events.len() as i64)),
                ("through_seq", Json::Int(st.watermark)),
            ]),
            ExitClass::Ok,
        ),
    };
    close_session(b, &sess.session_id);
    Ok((out, r.format))
}

/// `run status <run_id>` → `attach` + `project{run_summary}`.
fn cmd_run_status(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &Parsed,
    argv: &[String],
) -> Result<(CliOutcome, OutputFormat), CliError> {
    let run_id = require_pos(p, 0, "<run_id>")?;
    let r = resolve(p, io, argv, false, false)?;
    let sess = attach(b, &run_id, &r.invocation)?;
    let view = b.call(
        "project",
        &Json::obj([
            ("session_id", Json::str(sess.session_id.clone())),
            ("view_kind", Json::str("run_summary")),
        ]),
    )?;
    close_session(b, &sess.session_id);
    ok_outcome("run_status", view, r.format)
}

/// `run inspect <run_id> [view]` → `attach` + `project{view_kind}`
/// (`context_view` | `run_summary` | `checkpoint`).
fn cmd_run_inspect(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &Parsed,
    argv: &[String],
) -> Result<(CliOutcome, OutputFormat), CliError> {
    let run_id = require_pos(p, 0, "<run_id>")?;
    let r = resolve(p, io, argv, false, false)?;
    let view = p
        .flag("view")
        .or_else(|| p.positional.get(1).cloned())
        .unwrap_or_else(|| "run_summary".into());
    let kind = match view.as_str() {
        "context_view" | "run_summary" | "checkpoint" => view.as_str(),
        other => {
            return Err(inv(
                "unknown_view",
                "--view",
                &format!(
                    "unknown inspect view {other:?}; expected context_view|run_summary|checkpoint"
                ),
            ))
        }
    };
    let sess = attach(b, &run_id, &r.invocation)?;
    let v = b.call(
        "project",
        &Json::obj([
            ("session_id", Json::str(sess.session_id.clone())),
            ("view_kind", Json::str(kind)),
        ]),
    )?;
    close_session(b, &sess.session_id);
    ok_outcome("run_inspect", v, r.format)
}

// ── writer-session commands ────────────────────────────────────────────

/// `run resume <run_id> [--mode continue|takeover|--takeover] [--follow]`
/// → `open_session{resume}` (+ the attended loop under `--follow`).
/// Cross-process the boundary answers
/// `Refused{resume_checkpoint_unavailable}` while durable leaf
/// checkpoints are deferred (DF-S1.25-2) — surfaced verbatim.
fn cmd_run_resume(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &Parsed,
    argv: &[String],
) -> Result<(CliOutcome, OutputFormat), CliError> {
    let run_id = require_pos(p, 0, "<run_id>")?;
    let r = resolve(p, io, argv, true, false)?;
    let mode_flag = p.flag("mode");
    let mode = match mode_flag.as_deref() {
        None => {
            if p.has("takeover") {
                "takeover"
            } else {
                "continue"
            }
        }
        Some(m @ ("continue" | "takeover")) => m,
        Some(other) => {
            return Err(inv(
                "unknown_mode",
                "--mode",
                &format!("unknown resume mode {other:?}; expected continue|takeover"),
            ))
        }
    };
    let sess = resume_session(b, &run_id, mode, Some(&r.invocation))?;
    if !p.has("follow") {
        let out = CliOutcome {
            class: ExitClass::Ok,
            result: result_record("run_resume", &sess.to_json(), ExitClass::Ok),
        };
        return Ok((out, r.format));
    }
    let mut st = LoopState::new(b, -1);
    let interactive = r.attendance.value == AttendanceValue::Interactive;
    let finished = attend(b, io, &sess, &r.format, interactive, &r.invocation, &mut st)?;
    let finished = finished.or_else(|| st.finished.clone());
    let out = terminal_outcome(&sess, &st.events, finished.as_ref());
    Ok((out, r.format))
}

/// `run fork <run_id> --at-seq N | --at-event ID` → writer session +
/// `fork{at}` → the child `Session` (a new run; `invocation` mints the
/// child's `lifecycle.surface.invoked`).
fn cmd_run_fork(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &Parsed,
    argv: &[String],
) -> Result<(CliOutcome, OutputFormat), CliError> {
    let run_id = require_pos(p, 0, "<run_id>")?;
    let r = resolve(p, io, argv, false, false)?;
    let mode = if p.has("takeover") {
        "takeover"
    } else {
        "continue"
    };
    let sess = resume_session(b, &run_id, mode, Some(&r.invocation))?;
    let at = match (p.flag("at-seq"), p.flag("at-event")) {
        (Some(n), None) => {
            let seq: i64 = n
                .parse()
                .map_err(|_| inv("bad_fork_point", "--at-seq", "expected an integer"))?;
            Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(seq))])
        }
        (None, Some(e)) => Json::obj([
            ("kind", Json::str("event_ref")),
            ("run_id", Json::str(run_id.clone())),
            ("event_id", Json::str(e)),
        ]),
        (None, None) => {
            return Err(inv(
                "missing_fork_point",
                "--at-seq|--at-event",
                "run fork needs a fork point",
            ))
        }
        _ => {
            return Err(inv(
                "flag_conflict",
                "--at-seq",
                "--at-seq and --at-event are exclusive",
            ))
        }
    };
    let raw = b.call(
        "fork",
        &Json::obj([
            ("session_id", Json::str(sess.session_id.clone())),
            ("at", at),
            (
                "idempotency_key",
                Json::str(step_key(&r.invocation, "fork")),
            ),
            ("invocation", r.invocation.to_json()),
        ]),
    )?;
    // The writer session stays open — `close` on a still-live parent
    // run would mint `cancelled{by: principal}`.
    ok_outcome("run_fork", raw, r.format)
}

/// `run cancel <run_id> [--turn T]` → writer session + `cancel{run|turn}`
/// (the stop protocol's `cancelled{by: principal}`).
fn cmd_run_cancel(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &Parsed,
    argv: &[String],
) -> Result<(CliOutcome, OutputFormat), CliError> {
    let run_id = require_pos(p, 0, "<run_id>")?;
    let r = resolve(p, io, argv, false, false)?;
    let mode = if p.has("takeover") {
        "takeover"
    } else {
        "continue"
    };
    let sess = resume_session(b, &run_id, mode, Some(&r.invocation))?;
    let scope = match p.flag("turn") {
        Some(t) => Json::obj([("kind", Json::str("turn")), ("turn_id", Json::str(t))]),
        None => Json::obj([("kind", Json::str("run"))]),
    };
    let raw = b.call(
        "cancel",
        &Json::obj([
            ("session_id", Json::str(sess.session_id.clone())),
            ("scope", scope),
            (
                "idempotency_key",
                Json::str(step_key(&r.invocation, "cancel")),
            ),
        ]),
    )?;
    // A run-scope cancel drove the run to `cancelled` — `close{done}`
    // is clean teardown. (A turn-scope cancel leaves the run live;
    // closing then would mint a second `cancelled` — skip it.)
    if raw
        .get("scope")
        .and_then(|s| s.get("kind"))
        .and_then(Json::as_str)
        == Some("run")
        || p.flag("turn").is_none()
    {
        close_session(b, &sess.session_id);
    }
    let out = CliOutcome {
        class: ExitClass::Cancelled,
        result: result_record("run_cancel", &raw, ExitClass::Cancelled),
    };
    Ok((out, r.format))
}

/// `run amend <run_id> budget DIM=CAP[,…]|{json}` → writer session +
/// `amend{budget}` — the ledgered exhaustion wake (ADR-0216 OQ-468
/// interim op; `experimental` tier, negotiated at `hello`). Non-`budget`
/// targets map through and let the boundary's `Refused{stage_pending}`
/// answer (K-1 — no CLI-side pre-judgement).
fn cmd_run_amend(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &Parsed,
    argv: &[String],
) -> Result<(CliOutcome, OutputFormat), CliError> {
    let run_id = require_pos(p, 0, "<run_id>")?;
    let target = p
        .flag("target")
        .or_else(|| p.positional.get(1).cloned())
        .unwrap_or_else(|| "budget".into());
    let value_str = p
        .flag("value")
        .or_else(|| p.positional.get(2).cloned())
        .ok_or_else(|| {
            inv(
                "missing_amend_value",
                "<value>",
                "run amend needs DIM=CAP[,…] or a JSON value",
            )
        })?;
    let value = if value_str.starts_with('{') {
        hh_wire::json::parse(&value_str)
            .map_err(|e| inv("bad_amend_value", "<value>", &e.to_string()))?
    } else {
        let mut dims = BTreeMap::new();
        for part in value_str.split(',') {
            let (d, cap) = part.split_once('=').ok_or_else(|| {
                inv(
                    "bad_amend_value",
                    "<value>",
                    &format!("expected DIM=CAP, got {part:?}"),
                )
            })?;
            let n: i64 = cap.parse().map_err(|_| {
                inv(
                    "bad_amend_value",
                    "<value>",
                    &format!("cap {cap:?} is not an integer"),
                )
            })?;
            dims.insert(d.to_string(), Json::Int(n));
        }
        Json::obj([("dimensions", Json::Obj(dims))])
    };
    let r = resolve(p, io, argv, false, false)?;
    let mode = if p.has("takeover") {
        "takeover"
    } else {
        "continue"
    };
    let sess = resume_session(b, &run_id, mode, Some(&r.invocation))?;
    let mut params = Json::obj([
        ("session_id", Json::str(sess.session_id.clone())),
        ("target", Json::str(target)),
        ("value", value),
        (
            "idempotency_key",
            Json::str(step_key(&r.invocation, "amend")),
        ),
        ("invocation", r.invocation.to_json()),
    ]);
    if let Some(a) = p.flag("attestation") {
        if let Json::Obj(m) = &mut params {
            m.insert(
                "attestation".into(),
                hh_wire::json::parse(&a).unwrap_or(Json::Null),
            );
        }
    }
    let raw = b.call("amend", &params)?;
    // The session stays open — `amend` wakes the parked loop, it does
    // not end the run.
    ok_outcome("run_amend", raw, r.format)
}

/// `run submit <run_id> [prompt]` → writer session + `submit` + the
/// attended loop (the follow-up turn on a resumed run).
fn cmd_run_submit(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &Parsed,
    argv: &[String],
) -> Result<(CliOutcome, OutputFormat), CliError> {
    let run_id = require_pos(p, 0, "<run_id>")?;
    let r = resolve(p, io, argv, true, io.stdin.is_some())?;
    let mode = if p.has("takeover") {
        "takeover"
    } else {
        "continue"
    };
    let sess = resume_session(b, &run_id, mode, Some(&r.invocation))?;
    let mut input: Vec<Json> = Vec::new();
    if let Some(t) = p.positional.get(1) {
        input.push(text_block(t, "principal", "human"));
    }
    if let Some(bytes) = &io.stdin {
        input.push(text_block(
            &String::from_utf8_lossy(bytes),
            "external",
            "import",
        ));
    }
    b.call(
        "submit",
        &Json::obj([
            ("session_id", Json::str(sess.session_id.clone())),
            ("input", Json::Arr(input)),
            (
                "idempotency_key",
                Json::str(step_key(&r.invocation, "submit")),
            ),
        ]),
    )?;
    let mut st = LoopState::new(b, -1);
    let interactive = r.attendance.value == AttendanceValue::Interactive;
    let finished = attend(b, io, &sess, &r.format, interactive, &r.invocation, &mut st)?;
    let finished = finished.or_else(|| st.finished.clone());
    let out = terminal_outcome(&sess, &st.events, finished.as_ref());
    Ok((out, r.format))
}

// ── approval commands ──────────────────────────────────────────────────

/// `approval list <run_id>` → `attach` + `read{security.permission.*}` —
/// the open asks (pending without decided) and the decision trail.
fn cmd_approval_list(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &Parsed,
    argv: &[String],
) -> Result<(CliOutcome, OutputFormat), CliError> {
    let run_id = require_pos(p, 0, "<run_id>")?;
    let r = resolve(p, io, argv, false, false)?;
    let sess = attach(b, &run_id, &r.invocation)?;
    let events = read_class(b, &sess.session_id, "security.permission")?;
    let open: Vec<Json> = undecided_permissions(&events)
        .iter()
        .map(|pid| Json::str(pid.clone()))
        .collect();
    close_session(b, &sess.session_id);
    ok_outcome(
        "approval_list",
        Json::obj([
            ("run_id", Json::str(run_id)),
            ("open", Json::Arr(open)),
            ("events", Json::Arr(events)),
        ]),
        r.format,
    )
}

/// Read every durable event whose class has the `prefix` (paged).
fn read_class(b: &mut dyn Boundary, session_id: &str, prefix: &str) -> Result<Vec<Json>, CliError> {
    let mut events: Vec<Json> = Vec::new();
    let mut cursor = 0i64;
    loop {
        let raw = b.call(
            "read",
            &Json::obj([
                ("session_id", Json::str(session_id)),
                (
                    "cursor",
                    Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(cursor))]),
                ),
                ("direction", Json::str("fwd")),
                ("limit", Json::Int(256)),
                (
                    "filter",
                    Json::obj([("classes", Json::Arr(vec![Json::str(prefix)]))]),
                ),
            ]),
        )?;
        if let Some(Json::Arr(a)) = raw.get("events") {
            for e in a {
                let seq = e.get("seq").and_then(Json::as_int).unwrap_or(cursor);
                cursor = cursor.max(seq + 1);
                events.push(e.clone());
            }
        }
        match raw.get("next") {
            Some(Json::Obj(_)) => continue,
            _ => break,
        }
    }
    Ok(events)
}

/// `approval show <run_id> <permission_id>` → `attach` + `read` — the
/// ask's durable rows (pending + decided when present).
fn cmd_approval_show(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &Parsed,
    argv: &[String],
) -> Result<(CliOutcome, OutputFormat), CliError> {
    let run_id = require_pos(p, 0, "<run_id>")?;
    let pid = require_pos(p, 1, "<permission_id>")?;
    let r = resolve(p, io, argv, false, false)?;
    let sess = attach(b, &run_id, &r.invocation)?;
    let events = read_class(b, &sess.session_id, "security.permission")?;
    close_session(b, &sess.session_id);
    let rows: Vec<Json> = events
        .into_iter()
        .filter(|e| {
            e.get("payload")
                .and_then(|p| p.get("permission_id"))
                .and_then(Json::as_str)
                == Some(pid.as_str())
        })
        .collect();
    if rows.is_empty() {
        return Err(CliError::Kernel(hh_embed_client_generated::EmbedError {
            code: 1503,
            kind: "UnknownPermission".into(),
            retryable: false,
            message: format!("no permission rows for {pid}"),
            data: Json::obj([("permission_id", Json::str(pid))]),
        }));
    }
    ok_outcome("approval_show", Json::Arr(rows), r.format)
}

/// `approval respond <run_id> <permission_id> <option|--cancel>` →
/// writer session + `respond_permission` (idempotent; `AlreadyDecided`
/// surfaces verbatim).
fn cmd_approval_respond(
    b: &mut dyn Boundary,
    io: &mut Io,
    p: &Parsed,
    argv: &[String],
) -> Result<(CliOutcome, OutputFormat), CliError> {
    let run_id = require_pos(p, 0, "<run_id>")?;
    let pid = require_pos(p, 1, "<permission_id>")?;
    let r = resolve(p, io, argv, false, false)?;
    let outcome = if p.has("cancel") {
        PermissionOutcome::Cancelled
    } else {
        let opt = p.positional.get(2).cloned().ok_or_else(|| {
            inv(
                "missing_option",
                "<option>",
                "approval respond needs an option id or --cancel",
            )
        })?;
        PermissionOutcome::Selected { option_id: opt }
    };
    let mode = if p.has("takeover") {
        "takeover"
    } else {
        "continue"
    };
    let sess = resume_session(b, &run_id, mode, Some(&r.invocation))?;
    let respond_key = step_key(&r.invocation, &format!("respond:{pid}"));
    let raw = b.call(
        "respond_permission",
        &Json::obj([
            ("session_id", Json::str(sess.session_id.clone())),
            ("permission_id", Json::str(pid)),
            ("outcome", outcome.to_json()),
            ("idempotency_key", Json::str(respond_key)),
        ]),
    )?;
    // No close — a decided ask on a live run leaves it driving or
    // parked; `close` would mint `cancelled{by: principal}`.
    ok_outcome("approval_respond", raw, r.format)
}

// ── program commands ───────────────────────────────────────────────────

/// `version` → the negotiated `hello` facts (kernel descriptor +
/// identity); no ledger effect.
fn cmd_version(b: &mut dyn Boundary, io: &mut Io) -> Result<(CliOutcome, OutputFormat), CliError> {
    let h = b
        .hello_result()
        .ok_or_else(|| CliError::Transport("no hello negotiated".into()))?;
    let format = resolve_format(None, false, io.tty.stdout).map_err(CliError::Invocation)?;
    ok_outcome(
        "version",
        Json::obj([
            (
                "cli",
                Json::str(concat!("hh-cli/", env!("CARGO_PKG_VERSION"))),
            ),
            ("kernel", h.kernel.to_json()),
            ("stability", h.stability.clone()),
            ("experimental_enabled", Json::Bool(h.experimental_enabled)),
        ]),
        format,
    )
}

/// `doctor` → `hello` connectivity + identity diagnostics; failures are
/// the exit classes, never silent.
fn cmd_doctor(b: &mut dyn Boundary, io: &mut Io) -> Result<(CliOutcome, OutputFormat), CliError> {
    let h = b
        .hello_result()
        .ok_or_else(|| CliError::Transport("no hello negotiated".into()))?;
    let format = resolve_format(None, false, io.tty.stdout).map_err(CliError::Invocation)?;
    ok_outcome(
        "doctor",
        Json::obj([
            ("connectivity", Json::str("ok")),
            ("kernel", h.kernel.to_json()),
            ("negotiated", h.negotiated.to_json()),
        ]),
        format,
    )
}

// ── terminal record ────────────────────────────────────────────────────

/// The `ResultRecord` for a finished (or still-parked) run —
/// `run_id`, `configuration_version_id`, `stop_reason`, `outcome_class`,
/// `resource_account`, `exit_class`, `veto_tripped[]` (ADR-0169 D3).
fn terminal_outcome(sess: &Session, events: &[Json], finished: Option<&Json>) -> CliOutcome {
    let (stop, outcome, denies) = terminal_of(events);
    let class = if finished.is_some() {
        exit_class_for_terminal(&stop, &outcome, denies)
    } else {
        // The run stayed parked (async cue or EOF on the prompt) — the
        // CLI detached without a terminal. `ok` would claim finished
        // work; the honest class is `cancelled`? No — nothing cancelled.
        // `refused`? No refusal happened. The parked-detach is reported
        // as `ok` with `detached: parked` in the record — the run's own
        // ledger says the truth (no finished row), and a later
        // invocation can carry it.
        ExitClass::Ok
    };
    let mut payload: Vec<(&'static str, Json)> = vec![
        ("run_id", Json::str(sess.run_id.clone())),
        (
            "configuration_version_id",
            Json::str(sess.configuration_version_id.clone()),
        ),
        ("stop_reason", Json::str(stop)),
        ("outcome_class", Json::str(outcome)),
        (
            "resource_account",
            Json::obj([("usage", Json::Arr(vec![]))]),
        ),
        ("veto_tripped", Json::Arr(vec![])),
    ];
    if finished.is_none() {
        payload.push(("detached", Json::str("parked")));
    }
    let map: BTreeMap<String, Json> = payload
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    CliOutcome {
        class,
        result: result_record("run", &Json::Obj(map), class),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AC-R-2.11.1-12 — the prompt renders every `Explanation` member the
    /// ask carries (rule, risk factors, the auto-approve hint, the
    /// untruncated proposal) and labels `model_justification` as
    /// delegate-authored.
    #[test]
    fn render_ask_renders_every_explanation_field() {
        let long_proposal = "x".repeat(500);
        let up = Json::obj([
            ("permission_id", Json::str("perm-1")),
            ("proposal", Json::str(long_proposal.clone())),
            (
                "rendering",
                Json::obj([
                    ("text", Json::str(long_proposal.clone())),
                    (
                        "explanation",
                        Json::obj([
                            ("rule_id", Json::str("rule:ask")),
                            (
                                "risk_factors",
                                Json::Arr(vec![
                                    Json::str("capability_id:cap:exec"),
                                    Json::str("surface_id:surface:exec"),
                                ]),
                            ),
                            (
                                "what_would_auto_approve",
                                Json::obj([("requires_approval", Json::Bool(false))]),
                            ),
                            ("model_justification", Json::str("the model asked for it")),
                        ]),
                    ),
                ]),
            ),
        ]);
        let mut out = Vec::new();
        let mut err = Vec::new();
        {
            let mut io = Io {
                tty: Tty {
                    stdin: true,
                    stdout: true,
                    stderr: true,
                },
                stdin: None,
                cwd_ref: "cwd:/t".into(),
                principal: "principal:t".into(),
                kernel_env: vec![],
                kernel_cmd: "hh-kernel".into(),
                out: &mut out,
                err: &mut err,
                prompt: None,
            };
            render_ask(
                &mut io,
                "perm-1",
                &long_proposal,
                &["allow_once".into(), "deny_once".into()],
                Some(&up),
            );
        }
        let e = String::from_utf8(err).unwrap();
        assert!(e.contains("permission_id: perm-1"), "{e}");
        // The untruncated proposal.
        assert!(e.contains(&format!("proposal: {long_proposal}")), "{e}");
        assert!(e.contains(&format!("rendering: {long_proposal}")), "{e}");
        assert!(e.contains("explanation.rule_id: rule:ask"), "{e}");
        assert!(
            e.contains("explanation.risk_factor: capability_id:cap:exec"),
            "{e}"
        );
        assert!(
            e.contains("explanation.risk_factor: surface_id:surface:exec"),
            "{e}"
        );
        assert!(e.contains("explanation.what_would_auto_approve:"), "{e}");
        assert!(
            e.contains("explanation.model_justification [delegate]:"),
            "{e}"
        );
        assert!(e.contains("options: allow_once | deny_once"), "{e}");
    }
}
