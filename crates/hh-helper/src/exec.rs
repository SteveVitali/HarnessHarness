//! The exec engine — what the helper *does* (§5d.5 §4 `exec`; R-2.5.5).
//! One `Execution` is one journaled child: spawned as a process-group
//! leader (`setsid` in the pre-exec gap, or `podman exec` which the
//! container runtime scopes), drained on pump threads, and killed by
//! signal-group — never by lone `pid`, so a `fork`'d grandchild inside the
//! group dies with it and a `setsid`'d grandchild lands on `list_detached`
//! (§5d.5 §4's "detached processes surfaced" row).
//!
//! The deadline is enforced *here* — the ladder the kernel computed becomes
//! a `deadline_ms` budget the helper owns: lapsed ⇒ `term` → `grace` →
//! `kill`, and the journal's `failure{class:"deadline_exceeded"}` is the
//! verbatim result (the kernel's own timeout is the backstop — never the
//! enforcer-of-record).
//!
//! Dedup (R-2.5.5¹, tier-c1): the `DedupStore` maps `idempotency_key →
//! verdict` for the session's declared window; a replayed key returns the
//! recorded terminal outcome without re-executing (same ⇒ same).

use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[cfg(feature = "tier-c1")]
use hh_wire::json::Json;

use crate::protocol::HelperFrame;

/// The two-phase interruption grace (§5b.3's `term → kill` ladder — the
/// helper applies `grace_ms` between them).
pub const TERM_GRACE_MS: u64 = 2_000;

/// `JournalEntry` — one journaled item (the `read` verb replays it).
#[derive(Debug, Clone)]
pub enum JournalEntry {
    /// An output chunk (`chunk{seq,stream,data,token}`).
    Chunk {
        /// The seq.
        seq: u64,
        /// `stdout`/`stderr`.
        stream: &'static str,
        /// The bytes (UTF-8 lossy on the wire).
        data: String,
    },
    /// A process transition (`process{seq,transition,process_ref}`).
    Process {
        /// The seq.
        seq: u64,
        /// `spawned`/`detached`/`exited`/`signalled`.
        transition: &'static str,
        /// `pgrp:<pgid>`.
        process_ref: String,
    },
}

/// `ExecState` — the execution's lifecycle inside the helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecState {
    /// The child is live.
    Running,
    /// Term/kill issued — draining.
    Terminating,
    /// The child reaped; `exit_status`/`failure` recorded.
    Finished,
    /// `detach` marked the execution: the direct child reaped, the group
    /// may live on (surfaced via `detached[]`/`list_detached`).
    Detached,
}

/// `DetachedGroup` — a surviving process group after its direct child
/// exited (§5d.5 §4 `list_detached`).
#[derive(Debug, Clone)]
pub struct DetachedGroup {
    /// `pgrp:<pgid>`.
    pub process_ref: String,
    /// The execution it came from.
    pub execution_id: String,
}

/// `Execution` — the journaled child state.
pub struct Execution {
    /// The execution id.
    pub execution_id: String,
    /// The effect.
    pub effect_id: String,
    /// The attempt.
    pub attempt_no: u64,
    /// The attribution token (echoed on every frame — the helper holds it,
    /// never mints it).
    pub token: String,
    /// The journal (chunk/process entries, seq-ordered).
    pub journal: Vec<JournalEntry>,
    /// The state.
    pub state: ExecState,
    /// The exit status (`exit_status` member) — `Some` when reaped.
    pub exit_status: Option<i64>,
    /// The verbatim failure (`failure{class,detail}`) — `deadline_exceeded`,
    /// `spawn_failed`, `sandbox_denied`, `signalled`.
    pub failure: Option<(String, String)>,
    /// Whether the backend refused the spawn (sandbox denial).
    pub sandbox_denied: bool,
    /// Retained bytes (Σ chunk bytes) — the journal truncates at
    /// `retain_bytes_cap` (dropped bytes recorded via `truncated`).
    pub retained_bytes: u64,
    /// The cap.
    pub retain_bytes_cap: u64,
    /// Bytes dropped by the cap.
    pub dropped_bytes: u64,
    /// The deadline (helper-enforced).
    pub deadline: Option<Instant>,
    /// The child handle (while live).
    pub child: Option<Child>,
    /// The child's process group id (leader = child pid on unix).
    pub pgid: Option<u32>,
    /// Detached groups observed after reap.
    pub detached: Vec<DetachedGroup>,
    /// `cancel` requested (`term`/`kill` — the record, not the mechanism).
    pub cancel_phase: Option<String>,
    /// The idempotency key this execution binds (the dedup store's operand).
    pub idempotency_key: Option<String>,
}

impl Execution {
    /// New journal state for a request.
    pub fn new(
        execution_id: String,
        effect_id: String,
        attempt_no: u64,
        token: String,
        retain_bytes_cap: u64,
        deadline_ms: Option<u64>,
    ) -> Execution {
        Execution {
            execution_id,
            effect_id,
            attempt_no,
            token,
            journal: Vec::new(),
            state: ExecState::Running,
            exit_status: None,
            failure: None,
            sandbox_denied: false,
            retained_bytes: 0,
            retain_bytes_cap,
            dropped_bytes: 0,
            deadline: deadline_ms.map(|d| Instant::now() + Duration::from_millis(d)),
            child: None,
            pgid: None,
            detached: Vec::new(),
            cancel_phase: None,
            idempotency_key: None,
        }
    }

    fn next_seq(&self) -> u64 {
        // seqs start at 1 — `after_seq = 0` on the wire means "from the
        // beginning".
        self.journal.len() as u64 + 1
    }

    /// Append a chunk (cap-enforced — overflow is dropped + counted).
    pub fn push_chunk(&mut self, stream: &'static str, data: &[u8]) {
        let seq = self.next_seq();
        let room = self.retain_bytes_cap.saturating_sub(self.retained_bytes);
        let take = (data.len() as u64).min(room) as usize;
        self.dropped_bytes += (data.len() - take) as u64;
        if take > 0 {
            self.retained_bytes += take as u64;
            self.journal.push(JournalEntry::Chunk {
                seq,
                stream,
                data: String::from_utf8_lossy(&data[..take]).to_string(),
            });
        }
    }

    /// Append a process transition.
    pub fn push_process(&mut self, transition: &'static str, process_ref: String) {
        let seq = self.next_seq();
        self.journal.push(JournalEntry::Process {
            seq,
            transition,
            process_ref,
        });
    }

    /// Replay the journal as frames after `after_seq` (the `read` verb's
    /// `chunks[]`), bounded to `max_bytes` of chunk payload.
    pub fn frames_after(&self, after_seq: u64, max_bytes: u64) -> Vec<HelperFrame> {
        let mut out = Vec::new();
        let mut bytes = 0u64;
        for e in &self.journal {
            match e {
                JournalEntry::Chunk { seq, stream, data } if *seq > after_seq => {
                    if max_bytes > 0 && bytes >= max_bytes {
                        break;
                    }
                    bytes += data.len() as u64;
                    out.push(HelperFrame::Chunk {
                        seq: *seq,
                        stream: stream.to_string(),
                        data: data.clone(),
                        token: self.token.clone(),
                    });
                }
                JournalEntry::Process {
                    seq,
                    transition,
                    process_ref,
                } if *seq > after_seq => out.push(HelperFrame::Process {
                    seq: *seq,
                    transition: transition.to_string(),
                    process_ref: process_ref.clone(),
                    token: self.token.clone(),
                }),
                _ => {}
            }
        }
        out
    }
}

/// `DedupStore` — the executor-side dedup window (R-2.5.5¹'s `dedup_window`
/// member; C1 — absent `tier-c1`, the session refuses the member).
#[cfg(feature = "tier-c1")]
#[derive(Debug, Default)]
pub struct DedupStore {
    /// `idempotency_key → (verdict_json, recorded_at)` — within the window.
    pub entries: BTreeMap<String, (Json, Instant)>,
}

#[cfg(feature = "tier-c1")]
impl DedupStore {
    /// `check(key, window)` — the recorded verdict for a replayed key inside
    /// the window (`Some`) or `None` (new/expired).
    pub fn check(&mut self, key: &str, window: Duration) -> Option<Json> {
        if let Some((v, t)) = self.entries.get(key) {
            if t.elapsed() <= window {
                return Some(v.clone());
            }
            self.entries.remove(key);
        }
        None
    }

    /// `record(key, verdict)` — the terminal outcome binds the key.
    pub fn record(&mut self, key: &str, verdict: Json) {
        self.entries
            .insert(key.to_string(), (verdict, Instant::now()));
    }
}

/// The exec table — `execution_id → Arc<Mutex<Execution>>`, the monitor's
/// `Condvar` for `read`'s long-poll.
#[derive(Default)]
pub struct ExecTable {
    /// The live/finished executions.
    pub map: BTreeMap<String, Arc<Mutex<Execution>>>,
    /// `(effect_id, attempt_no) → execution_id` (probe resolution).
    pub by_effect: BTreeMap<(String, u64), String>,
    /// The dedup store (tier-c1).
    #[cfg(feature = "tier-c1")]
    pub dedup: DedupStore,
}

/// A shared table + its notify condvar.
pub struct Shared {
    /// The table.
    pub table: Mutex<ExecTable>,
    /// Signalled on every journal append / state change (long-poll wake).
    pub notify: Condvar,
    /// The session nonce (commit_proof recompute key).
    pub session_nonce: Mutex<String>,
    /// The session's dedup window (ms).
    pub dedup_window_ms: Mutex<Option<u64>>,
    /// `on_kernel_loss` (the `preserve_until` posture — the server enforces
    /// it on channel drop).
    pub on_kernel_loss_terminate: Mutex<bool>,
}

impl Shared {
    /// New empty session state.
    pub fn new() -> Arc<Shared> {
        Arc::new(Shared {
            table: Mutex::new(ExecTable::default()),
            notify: Condvar::new(),
            session_nonce: Mutex::new(String::new()),
            dedup_window_ms: Mutex::new(None),
            on_kernel_loss_terminate: Mutex::new(true),
        })
    }
}

/// `kill_group(pgid, sig)` — signal the whole group (`kill(-pgid, sig)`);
/// returns whether the group existed. Never targets a lone pid — a
/// `fork`'d member must die with the leader.
#[cfg(unix)]
pub fn kill_group(pgid: u32, sig: i32) -> bool {
    // kill with a negative pid targets the process group — the POSIX
    // `killpg` semantics the spec's two-phase ladder requires.
    libc_kill(pgid, sig)
}

#[cfg(unix)]
fn libc_kill(pgid: u32, sig: i32) -> bool {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    unsafe { kill(-(pgid as i32), sig) == 0 }
}

/// `group_alive(pgid)` — the group still has a member (`kill(-pg,0)`).
#[cfg(unix)]
pub fn group_alive(pgid: u32) -> bool {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
        fn __error() -> *mut i32;
    }
    unsafe {
        if kill(-(pgid as i32), 0) == 0 {
            return true;
        }
        #[cfg(target_os = "macos")]
        {
            *__error() == 1 // EPERM — exists but not ours
        }
        #[cfg(not(target_os = "macos"))]
        {
            *libc_errno() == 1
        }
    }
}

#[cfg(unix)]
#[cfg(not(target_os = "macos"))]
extern "C" {
    fn __errno_location() -> *mut i32;
}

#[cfg(unix)]
#[cfg(not(target_os = "macos"))]
unsafe fn libc_errno() -> *mut i32 {
    unsafe { __errno_location() }
}

#[cfg(not(unix))]
pub fn kill_group(_pgid: u32, _sig: i32) -> bool {
    false
}

/// The two-phase kill: `SIGTERM` to the group, wait `grace`, `SIGKILL`.
/// Returns the phase actually needed (`"term"` | `"kill"` | `"gone"`).
#[cfg(unix)]
pub fn two_phase_kill(pgid: u32, grace: Duration) -> &'static str {
    const SIGTERM: i32 = 15;
    const SIGKILL: i32 = 9;
    if !group_alive(pgid) {
        return "gone";
    }
    kill_group(pgid, SIGTERM);
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if !group_alive(pgid) {
            return "term";
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    kill_group(pgid, SIGKILL);
    "kill"
}

/// Spawn `cmd` as a process-group leader (`setsid` in the pre-exec gap —
/// the POSIX `setpgid(0,0)` equivalent so `kill(-pgid)` is well-defined).
#[cfg(unix)]
pub fn spawn_pgrp_leader(cmd: &mut Command) -> std::io::Result<Child> {
    use std::os::unix::process::CommandExt;
    unsafe {
        cmd.pre_exec(|| {
            // New session + new process group, leader = self.
            if libc_setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}

#[cfg(unix)]
fn libc_setsid() -> i32 {
    extern "C" {
        fn setsid() -> i32;
    }
    unsafe { setsid() }
}

/// The pump — read `stream` to EOF, appending journal chunks (bounded to
/// 64 KiB segments so `read` callers see timely progress).
pub fn pump<R: Read>(
    mut r: R,
    stream: &'static str,
    ex: Arc<Mutex<Execution>>,
    shared: Arc<Shared>,
) {
    let mut buf = [0u8; 65536];
    loop {
        match r.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let mut e = ex.lock().unwrap();
                e.push_chunk(stream, &buf[..n]);
                drop(e);
                shared.notify.notify_all();
            }
        }
    }
}

/// `run_child` — the exec thread body: pump both streams, reap, record the
/// verbatim exit/failure, surface detached groups, enforce the deadline
/// (term → kill) from a monitor tick.
///
/// The deadline is enforced by a sibling thread (`deadline_watcher`) that
/// fires `term` then `kill` on the *group*; this function only pumps and
/// reaps, so a wedged pipe can't postpone the ladder.
pub fn run_child(
    shared: Arc<Shared>,
    ex: Arc<Mutex<Execution>>,
    mut child: Child,
    pgid: u32,
    backend_label: &'static str,
) {
    {
        let mut e = ex.lock().unwrap();
        e.pgid = Some(pgid);
        e.push_process("spawned", format!("pgrp:{pgid}"));
    }
    let (out, err) = (child.stdout.take(), child.stderr.take());
    let mut pumps = Vec::new();
    if let Some(o) = out {
        let ex2 = Arc::clone(&ex);
        let sh = Arc::clone(&shared);
        pumps.push(std::thread::spawn(move || pump(o, "stdout", ex2, sh)));
    }
    if let Some(e2) = err {
        let ex2 = Arc::clone(&ex);
        let sh = Arc::clone(&shared);
        pumps.push(std::thread::spawn(move || pump(e2, "stderr", ex2, sh)));
    }
    // The deadline watcher — owns the term→kill ladder (helper-enforced).
    {
        let ex2 = Arc::clone(&ex);
        let sh = Arc::clone(&shared);
        std::thread::spawn(move || deadline_watcher(ex2, sh, pgid));
    }
    // Reap.
    let status: Option<ExitStatus> = match child.wait() {
        Ok(s) => Some(s),
        Err(e) => {
            let mut exl = ex.lock().unwrap();
            exl.failure = Some(("wait_failed".into(), e.to_string()));
            None
        }
    };
    // Record the terminal state BEFORE joining the pumps: a detached member
    // that inherited the stdout/stderr pipes holds them open, so joining
    // first would park the reap (and `exited`/`detached[]`) until the stray
    // exits — §5d.5 §4 wants the detach surfaced now, output still pumped.
    let mut e = ex.lock().unwrap();
    if let Some(s) = status {
        e.exit_status = s.code().map(|c| c as i64);
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(sig) = s.signal() {
                e.exit_status = Some(128 + sig as i64);
                if e.failure.is_none() && e.cancel_phase.is_none() && e.deadline.is_none() {
                    // An unsolicited signal death is verbatim evidence.
                    e.push_process("signalled", format!("pgrp:{pgid}"));
                }
            }
        }
    }
    // Detached-member detection — the group outlived its leader.
    if group_alive(pgid) {
        let execution_id = e.execution_id.clone();
        e.detached.push(DetachedGroup {
            process_ref: format!("pgrp:{pgid}"),
            execution_id,
        });
        e.push_process("detached", format!("pgrp:{pgid}"));
    }
    if e.state == ExecState::Running || e.state == ExecState::Terminating {
        e.state = ExecState::Finished;
    }
    if e.failure.is_none() && e.sandbox_denied {
        e.failure = Some((
            "sandbox_denied".into(),
            format!("{backend_label} refused the spawn"),
        ));
    }
    drop(e);
    shared.notify.notify_all();
    for p in pumps {
        let _ = p.join();
    }
}

/// The deadline + cancel watcher — the helper-enforced ladder: at
/// `deadline` (or on `cancel{phase:"term"}`) SIGTERM the group, grace, then
/// SIGKILL. Records `failure{deadline_exceeded}` verbatim.
fn deadline_watcher(ex: Arc<Mutex<Execution>>, shared: Arc<Shared>, pgid: u32) {
    loop {
        let (deadline, cancel, state) = {
            let e = ex.lock().unwrap();
            (e.deadline, e.cancel_phase.clone(), e.state)
        };
        if state == ExecState::Finished || state == ExecState::Detached {
            return;
        }
        let now = Instant::now();
        let lapsed = deadline.is_some_and(|d| now >= d);
        let cancel_now = matches!(cancel.as_deref(), Some("term") | Some("kill"));
        if lapsed || cancel_now {
            // Record the failure BEFORE the kill — the leader's exit can
            // race the watcher (reap marks `Finished` + notifies a parked
            // `read`), so the terminal verdict must already carry its
            // failure class or a fast reader sees a bare non-zero exit.
            {
                let mut e = ex.lock().unwrap();
                e.state = ExecState::Terminating;
                if lapsed {
                    e.failure = Some((
                        "deadline_exceeded".into(),
                        "deadline lapsed; term/kill ladder firing".to_string(),
                    ));
                } else if e.failure.is_none() {
                    e.failure = Some((
                        "cancelled".into(),
                        "cancel requested; term/kill ladder firing".to_string(),
                    ));
                }
            }
            let phase = two_phase_kill(pgid, Duration::from_millis(TERM_GRACE_MS));
            let mut e = ex.lock().unwrap();
            if let Some((_, detail)) = &mut e.failure {
                *detail = format!(
                    "{}; {phase} phase reached",
                    detail.split(';').next().unwrap_or("deadline/cancel")
                );
            }
            drop(e);
            shared.notify.notify_all();
            return;
        }
        let until = deadline
            .unwrap_or(now + Duration::from_millis(100))
            .min(now + Duration::from_millis(100));
        std::thread::sleep(
            until
                .saturating_duration_since(now)
                .min(Duration::from_millis(100)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_caps_and_frames() {
        let mut e = Execution::new("x".into(), "e".into(), 1, "tok".into(), 10, None);
        e.push_chunk("stdout", b"0123456789abcdef");
        assert_eq!(e.retained_bytes, 10);
        assert_eq!(e.dropped_bytes, 6);
        let frames = e.frames_after(0, 0);
        assert_eq!(frames.len(), 1);
        match &frames[0] {
            HelperFrame::Chunk { data, token, .. } => {
                assert_eq!(data, "0123456789");
                assert_eq!(token, "tok");
            }
            _ => panic!(),
        }
    }
}
