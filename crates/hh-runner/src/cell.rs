//! The slot cell drive (R6.9; CELL-SPEC/AR-06; REQ-XB-106/107/119) —
//! the ST1–ST5 cell lifecycle over an injected [`CellOps`]. The podman
//! backend lands in [`hharness_helper::podman::PodmanBackend`]; this
//! module owns the *ordering*: `/tests` (and `/solution`, except the
//! oracle participant) is absent until its phase and arrives only by
//! kernel-mediated `cp` — never through `/work`; stale `reward.*` is
//! removed before verification; the agent deadline lapse terminates the
//! process group and the slot still verifies with
//! `end_cause: agent_timeout`.
//!
//! The `CellOps` seam keeps the drive fixture-clean: the acceptance
//! battery runs it over a recording cell and over the real
//! `PodmanBackend` against a `podman` stub.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::disposition::EndCause;

/// The deadline provenance — where a phase's enforced deadline came
/// from (R6.45j/D15; ADR-0448): the task's own `task.toml` declaration
/// resolved under the pinned Harbor semantics, or the drive config's
/// machinery fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlineSource {
    /// `[agent|verifier].timeout_sec` from the task's `task.toml`.
    TaskToml,
    /// `cfg.{agent,verify}_deadline_ms` — the machinery fallback.
    CfgDefault,
}

impl DeadlineSource {
    /// The `cell-report.json` spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DeadlineSource::TaskToml => "task_toml",
            DeadlineSource::CfgDefault => "cfg_default",
        }
    }
}

/// The per-exec marker env var the in-cell process tree carries
/// (R6.45j/D17): `podman exec -e HH_EXEC_TAG=<tag>` lands the tag in
/// every spawned process's `/proc/<pid>/environ`, and children inherit
/// it. The lapse path walks for *this* tag — never a cmdline pattern
/// the phase's processes may not carry (the verifier's
/// `bash test.sh → uvx → pytest` tree never spells "agent").
pub const EXEC_TAG_ENV: &str = "HH_EXEC_TAG";

/// `hh-exec-<12-digit counter>` — zero-padded so one tag is never a
/// prefix of another inside the environ bytes (`grep -F` substring
/// matching cannot see the NUL terminator).
fn next_exec_tag() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    format!("hh-exec-{:012}", SEQ.fetch_add(1, Ordering::Relaxed))
}

/// The cell filesystem root — written with the escaped leading slash
/// the REQ-XB-105 source scan reserves for deliberate cell-interior
/// literals; the runner never names host-absolute paths.
const CELL_ROOT: &str = "\u{2f}";
/// The verifier tree inside the cell.
const TESTS_DIR: &str = "\u{2f}tests";
/// The oracle solution upload inside the cell.
const SOLUTION_DIR: &str = "\u{2f}solution";
/// The verifier log dir inside the cell.
const VERIFIER_DIR: &str = "\u{2f}logs/verifier";
/// The stale-reward glob removed before each verify leg.
const REWARD_GLOB: &str = "\u{2f}logs/verifier/reward.*";

/// A cell-side exec request — argv, the working directory (the declared
/// image WORKDIR, never `/work`), and the enforced deadline (ms).
#[derive(Debug, Clone, PartialEq)]
pub struct CellExec {
    /// The argv.
    pub argv: Vec<String>,
    /// The working directory.
    pub cwd: String,
    /// The enforced deadline (ms); `None` = unbounded (setup legs only).
    pub deadline_ms: Option<u64>,
}

/// One cell-side exec outcome.
#[derive(Debug, Clone, PartialEq)]
pub struct CellOutcome {
    /// The exit code (`None` = killed/no exit).
    pub exit_code: Option<i64>,
    /// The exec lapsed its enforced deadline.
    pub timed_out: bool,
    /// Captured stdout bytes.
    pub stdout: Vec<u8>,
    /// Captured stderr bytes — the bootstrap diagnostics (apt `E:`/
    /// `Err:` lines, `curl: (N)` errors) live on stderr; a null-sink
    /// stderr leaves a failed verifier no recorded evidence (the Wc5/
    /// Wc6 `allowlist_gap{unknown}` signature).
    pub stderr: Vec<u8>,
}

/// `CellOps` — the cell primitive seam the lifecycle drives. The podman
/// lowering is the helper backend's; the fixture's is in-memory.
pub trait CellOps {
    /// `test -e` inside the cell.
    fn present(&mut self, path: &str) -> Result<bool, CellError>;
    /// `ls -1A` inside the cell — the per-phase listing.
    fn ls_dir(&mut self, path: &str) -> Result<Vec<String>, CellError>;
    /// Kernel-mediated copy-in: host bytes → a cell path outside `/work`.
    fn cp_in(&mut self, host_src: &Path, container_dst: &str) -> Result<(), CellError>;
    /// Kernel-mediated copy-out: a cell path → a host destination.
    fn cp_out(&mut self, container_src: &str, host_dst: &Path) -> Result<(), CellError>;
    /// `rm -f <glob>` inside the cell (the stale `reward.*` removal).
    fn rm_glob(&mut self, glob: &str) -> Result<(), CellError>;
    /// Run one phase exec.
    fn exec(&mut self, spec: &CellExec) -> Result<CellOutcome, CellError>;
    /// Terminate the agent process group (TERM → KILL).
    fn terminate_group(&mut self, pattern: &str) -> Result<(), CellError>;
}

/// The cell-drive error — typed, never fabricated.
#[derive(Debug, Clone, PartialEq)]
pub struct CellError {
    /// The failing op.
    pub op: &'static str,
    /// The verbatim detail.
    pub detail: String,
}

impl std::fmt::Display for CellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cell:{}: {}", self.op, self.detail)
    }
}

impl std::error::Error for CellError {}

fn err(op: &'static str, detail: impl Into<String>) -> CellError {
    CellError {
        op,
        detail: detail.into(),
    }
}

/// `PodmanCell` — the `CellOps` lowering onto
/// [`hharness_helper::podman::PodmanBackend`]. Execs run through
/// `podman exec`; a lapsed deadline kills the client and terminates the
/// in-cell group (the helper enforces its own ladder — the drive
/// records what it asked for, never a claimed signal).
pub struct PodmanCell<'a> {
    /// The container backend.
    pub backend: &'a hharness_helper::podman::PodmanBackend,
}

impl CellOps for PodmanCell<'_> {
    fn present(&mut self, path: &str) -> Result<bool, CellError> {
        hharness_helper::podman::PodmanBackend::exists(self.backend, path)
            .map_err(|e| err("exists", e.to_string()))
    }
    fn ls_dir(&mut self, path: &str) -> Result<Vec<String>, CellError> {
        self.backend
            .ls_dir(path)
            .map_err(|e| err("ls", e.to_string()))
    }
    fn cp_in(&mut self, host_src: &Path, container_dst: &str) -> Result<(), CellError> {
        self.backend
            .cp_in(&host_src.to_string_lossy(), container_dst)
            .map_err(|e| err("cp_in", e.to_string()))
    }
    fn cp_out(&mut self, container_src: &str, host_dst: &Path) -> Result<(), CellError> {
        self.backend
            .cp_out(container_src, &host_dst.to_string_lossy())
            .map_err(|e| err("cp_out", e.to_string()))
    }
    fn rm_glob(&mut self, glob: &str) -> Result<(), CellError> {
        self.backend
            .rm_glob(glob)
            .map_err(|e| err("rm", e.to_string()))
    }
    fn exec(&mut self, spec: &CellExec) -> Result<CellOutcome, CellError> {
        exec_guarded(self.backend, &spec.argv, &spec.cwd, &[], spec.deadline_ms)
    }
    fn terminate_group(&mut self, pattern: &str) -> Result<(), CellError> {
        self.backend
            .terminate_group(pattern)
            .map_err(|e| err("terminate", e.to_string()))
    }
}

/// `exec_guarded(backend, argv, cwd, env, deadline_ms)` — one phase exec
/// under the D17 lapse enforcement (ADR-0448): a deadline-bearing exec
/// runs in-cell through
/// [`hharness_helper::podman::wrap_kill`] (`setsid --wait timeout
/// --kill-after` — the in-cell session-leader/group kill, the admitted
/// mechanism the host-side pgid kill could not reach) carrying a
/// unique `HH_EXEC_TAG`
/// in its environment. On lapse the host-side `podman exec` client is
/// killed, the tag's own in-cell process tree is TERM/KILLed by a
/// `/proc/*/environ` walk, and death is CONFIRMED by a second walk
/// before the outcome returns — the next `rm_glob`, verify attempt or
/// copy-out never overlaps a survivor. The streams captured up to the
/// lapse are kept — a timed-out verifier still lands its stdout/stderr
/// as evidence. A lapse the sweeps cannot confirm refuses typed —
/// the drive never proceeds on an unconfirmed kill.
///
/// `timeout`'s `124` exit is read as a lapse only when the deadline
/// already elapsed (a phase legitimately exiting 124 early is never
/// re-labelled). The host-side watcher stays the backstop at
/// `deadline + TERM_GRACE + slack` — past it the client is killed and
/// the tag sweep is authoritative.
///
/// Both pipes are drained on dedicated threads for the life of the
/// child — an in-cell writer that crosses the OS pipe bound never
/// stalls mid-script, and a completed phase's exec can finish
/// delivering its tail (the Wc9 exec-drain repair — the undrained
/// pipes produced pypi-server's in-cell freeze and
/// make-doom-for-mips' return hang, both surfacing as
/// `agent_timeout`).
pub fn exec_guarded(
    backend: &hharness_helper::podman::PodmanBackend,
    argv: &[String],
    cwd: &str,
    env: &[(String, String)],
    deadline_ms: Option<u64>,
) -> Result<CellOutcome, CellError> {
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let tag = deadline_ms.map(|_| next_exec_tag());
    let armed = hharness_helper::podman::wrap_kill(argv, deadline_ms);
    let mut env_all: Vec<(String, String)> = env.to_vec();
    if let Some(t) = &tag {
        env_all.push((EXEC_TAG_ENV.to_string(), t.clone()));
    }
    let mut cmd = backend.exec_command(&armed, Some(cwd), &env_all);
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let mut child = cmd.spawn().map_err(|e| err("exec", e.to_string()))?;
    // Drain each stream for the life of the child — the pipe can never
    // fill, so the in-cell writer is never write-blocked and the exec
    // can always finish delivering a completed phase's output.
    let drain_out = drain_stream(child.stdout.take());
    let drain_err = drain_stream(child.stderr.take());
    let started = Instant::now();
    let deadline = deadline_ms.map(|ms| started + Duration::from_millis(ms));
    // The in-cell `timeout` ladder needs the TERM→KILL grace plus the
    // `podman exec` teardown before the host-side watcher pronounces a
    // stuck exec; past the hard bound the tag sweep is authoritative.
    let hard_deadline =
        deadline.map(|d| d + Duration::from_millis(hharness_helper::exec::TERM_GRACE_MS + 15_000));
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            // `timeout` exits 124 only when it signalled the command —
            // and only past the declared deadline; an early 124 is the
            // phase's own exit code, never a lapse.
            let lapse_by_exit = status.code() == Some(124)
                && deadline
                    .map(|d| Instant::now() + Duration::from_millis(50) >= d)
                    .unwrap_or(false);
            if !lapse_by_exit {
                return Ok(CellOutcome {
                    exit_code: status.code().map(|c| c as i64),
                    timed_out: false,
                    stdout: join_drain(drain_out),
                    stderr: join_drain(drain_err),
                });
            }
            break;
        }
        if let Some(hd) = hard_deadline {
            if Instant::now() > hd {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    // Lapse cleanup: kill the lapsed exec's OWN in-cell process tree —
    // whatever the phase — by its `HH_EXEC_TAG` environ marker (TERM →
    // KILL), then CONFIRM death by a `/proc/*/environ` walk before the
    // outcome returns. The streams captured up to the lapse are kept.
    if let Some(t) = &tag {
        backend
            .terminate_exec_tag(t)
            .map_err(|e| err("terminate_exec", e.to_string()))?;
        confirm_dead(backend, t)?;
    }
    Ok(CellOutcome {
        exit_code: None,
        timed_out: true,
        stdout: join_drain(drain_out),
        stderr: join_drain(drain_err),
    })
}

/// Spawn a thread draining one piped stream into a `Vec` for the
/// stream's whole lifetime — the reader returns at EOF, after the
/// child exits or its pipe closes.
fn drain_stream<R: std::io::Read + Send + 'static>(
    stream: Option<R>,
) -> Option<std::thread::JoinHandle<Vec<u8>>> {
    stream.map(|mut s| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = s.read_to_end(&mut buf);
            buf
        })
    })
}

/// Collect a drain thread's captured bytes — the handle's buffer once
/// EOF lands (the exited/killed child closes its ends; a detached
/// writer holding the pipe parks the join exactly like the pre-drain
/// `read_to_end` did — never a new hang class), empty on a panicked
/// reader or an absent stream.
fn join_drain(handle: Option<std::thread::JoinHandle<Vec<u8>>>) -> Vec<u8> {
    handle
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default()
}

/// `confirm_dead(backend, tag)` — poll `/proc/*/environ` for the tag
/// until no tagged process survives (bounded). An unconfirmed kill is
/// a typed `confirm_dead` error — attempts never overlap and the next
/// `rm_glob`/attempt/copy-out never runs over a survivor.
fn confirm_dead(
    backend: &hharness_helper::podman::PodmanBackend,
    tag: &str,
) -> Result<(), CellError> {
    use std::time::{Duration, Instant};
    let until =
        Instant::now() + Duration::from_millis(hharness_helper::exec::TERM_GRACE_MS + 5_000);
    loop {
        let alive = backend
            .exec_tag_alive(tag)
            .map_err(|e| err("confirm_dead", e.to_string()))?;
        if !alive {
            return Ok(());
        }
        if Instant::now() > until {
            return Err(err(
                "confirm_dead",
                format!(
                    "lapsed exec tag {tag} still has live in-cell processes — \
                     the next attempt/copy-out is refused, never overlapped"
                ),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// `CellPlan` — the declared cell inputs for one slot's ST1–ST5 drive.
#[derive(Debug, Clone)]
pub struct CellPlan {
    /// `true` for the oracle participant — the only participant whose
    /// `/solution` is uploaded (REQ-XB-106).
    pub oracle_participant: bool,
    /// The declared image WORKDIR every phase exec runs under.
    pub workdir: String,
    /// The host staging dir holding `solution/` and `tests/` inputs.
    pub host_staging: std::path::PathBuf,
    /// The host dir the verify-phase reward bytes are copied out to.
    pub collect_dir: std::path::PathBuf,
    /// The agent argv (the participant launch).
    pub agent_argv: Vec<String>,
    /// The enforced agent deadline (ms — the AR-07 product).
    pub agent_deadline_ms: Option<u64>,
    /// The agent deadline's provenance (D15 — `task.toml` or the cfg
    /// fallback); echoed into the cell report per phase.
    pub agent_deadline_source: DeadlineSource,
    /// The verifier argv (`bash /tests/test.sh`).
    pub verifier_argv: Vec<String>,
    /// The enforced verifier deadline (ms).
    pub verifier_deadline_ms: Option<u64>,
    /// The verifier deadline's provenance (D15).
    pub verifier_deadline_source: DeadlineSource,
    /// The frozen same-cell regrade bound (proposed 2 — REQ-XB-111).
    pub max_grade_attempts: u32,
    /// The optional ST4 `/logs/agent` copy-out leg (R6.25/REQ-XC-317) —
    /// validated by [`crate::capture::copyout_spec`]; `None` = no
    /// native-log capture (an oracle slot).
    pub logs_copyout: Option<crate::capture::CopyoutSpec>,
}

/// A per-phase filesystem listing (`phase → sorted /` entries) — the
/// evidence `/tests` and `/solution` were absent until their phase.
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseListing {
    /// The lifecycle phase.
    pub phase: String,
    /// The sorted top-level entries the listing returned.
    pub entries: Vec<String>,
}

/// `VerifyOutcome` — what one ST5 leg observed.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifyOutcome {
    /// The ST5 attempt that produced `files` (1-based — collected
    /// files belong to exactly one attempt; D17).
    pub attempt: u32,
    /// The collected verifier bytes (paths → contents the channel
    /// classifies; `None` where the cell produced no file).
    pub files: crate::verdict::RewardFileSet,
    /// The exec's timeout flag (a verifier deadline lapse maps to
    /// `verifier_unavailable:verifier_timeout`).
    pub timed_out: bool,
}

/// `CellReport` — the drive's recorded result.
#[derive(Debug, Clone)]
pub struct CellReport {
    /// The per-phase listings in drive order.
    pub listings: Vec<PhaseListing>,
    /// The agent-phase end cause.
    pub agent_end_cause: EndCause,
    /// Whether the agent process group was terminated on deadline
    /// lapse (REQ-XB-119).
    pub agent_terminated: bool,
    /// The verify legs actually run (≤ `max_grade_attempts`).
    pub verify_attempts: u32,
    /// The final verify outcome.
    pub verify: Option<VerifyOutcome>,
    /// The enforced agent deadline + provenance (D15; echoed plan
    /// members — `cell-report.json` records them per phase).
    pub agent_deadline_ms: Option<u64>,
    /// The agent deadline's provenance.
    pub agent_deadline_source: DeadlineSource,
    /// The enforced verifier deadline.
    pub verifier_deadline_ms: Option<u64>,
    /// The verifier deadline's provenance.
    pub verifier_deadline_source: DeadlineSource,
    /// The hashed `/logs/agent` copy-out (REQ-XC-317 — `None` where the
    /// plan carries no capture leg).
    pub native_log: Option<crate::capture::NativeLogBundle>,
}

fn listing(cell: &mut dyn CellOps, phase: &str, acc: &mut Vec<PhaseListing>) {
    let entries = cell.ls_dir(CELL_ROOT).unwrap_or_default();
    acc.push(PhaseListing {
        phase: phase.to_string(),
        entries,
    });
}

fn stage_in(cell: &mut dyn CellOps, dir: &Path, container_dst: &str) -> Result<(), CellError> {
    // `podman cp` copies the dir *contents* under `<dst>/`; create the
    // cell-side parent first (the stub models it).
    cell.exec(&CellExec {
        argv: vec![
            "mkdir".to_string(),
            "-p".to_string(),
            container_dst.to_string(),
        ],
        cwd: CELL_ROOT.to_string(),
        deadline_ms: None,
    })?;
    cell.cp_in(dir, container_dst)
}

/// `run_cell(cell, plan) → CellReport` — the ST1–ST5 drive:
/// provision-boundary check → setup (`/solution` copy-in for the
/// oracle participant only) → agent exec under the declared WORKDIR
/// (deadline lapse ⇒ terminate the group, `end_cause: agent_timeout`)
/// → verify legs (stale `reward.*` removal, `/tests` copy-in, the
/// verifier exec, reward copy-out) bounded by `max_grade_attempts` in
/// the same cell — the participant is never re-run.
///
/// Phase-boundary refusals: `/tests` present before ST5, `/solution`
/// present before ST2 (oracle slots), or any copy destination under
/// `/work` (the backend refuses the alias too) — a boundary leak is a
/// typed `phase_boundary` error, never a relabelled outcome.
pub fn run_cell(cell: &mut dyn CellOps, plan: &CellPlan) -> Result<CellReport, CellError> {
    let mut report = CellReport {
        listings: Vec::new(),
        agent_end_cause: EndCause::Exited(0),
        agent_terminated: false,
        verify_attempts: 0,
        verify: None,
        agent_deadline_ms: plan.agent_deadline_ms,
        agent_deadline_source: plan.agent_deadline_source,
        verifier_deadline_ms: plan.verifier_deadline_ms,
        verifier_deadline_source: plan.verifier_deadline_source,
        native_log: None,
    };
    // ST1 boundary — nothing phase-owned may already exist.
    if cell.present(TESTS_DIR)? {
        return Err(err(
            "phase_boundary",
            format!(
                "{TESTS_DIR} exists before the verify phase — the image ships a boundary file the lifecycle never creates"
            ),
        ));
    }
    if plan.oracle_participant && cell.present(SOLUTION_DIR)? {
        return Err(err(
            "phase_boundary",
            format!("{SOLUTION_DIR} exists before the setup phase"),
        ));
    }
    listing(cell, "provision", &mut report.listings);
    // ST2 — oracle only: the solution travels by copy-in, never /work.
    if plan.oracle_participant {
        stage_in(cell, &plan.host_staging.join("solution"), SOLUTION_DIR)?;
    }
    listing(cell, "setup", &mut report.listings);
    // ST3 — the agent phase under the declared WORKDIR + deadline.
    let agent = cell.exec(&CellExec {
        argv: plan.agent_argv.clone(),
        cwd: plan.workdir.clone(),
        deadline_ms: plan.agent_deadline_ms,
    })?;
    if agent.timed_out {
        cell.terminate_group("agent")?;
        report.agent_terminated = true;
        report.agent_end_cause = EndCause::AgentTimeout;
    } else {
        report.agent_end_cause = EndCause::Exited(agent.exit_code.unwrap_or(-1));
    }
    // The agent exec's captured streams are evidence exactly like the
    // verifier's `test-*.txt`: a phase whose transcript is dropped
    // with the `CellOutcome` leaves a timed-out/failed cell no
    // recorded bytes (the Wc9 pypi-server freeze position had to be
    // inferred from the oracle transcript + the mediator WAL, never
    // byte-confirmed). Persist non-empty streams verbatim —
    // `agent-*.txt` sits beside the reward files at the collect top
    // level; the per-attempt re-materialization never touches it.
    let _ = std::fs::create_dir_all(&plan.collect_dir);
    if !agent.stdout.is_empty() {
        let _ = std::fs::write(plan.collect_dir.join("agent-stdout.txt"), &agent.stdout);
    }
    if !agent.stderr.is_empty() {
        let _ = std::fs::write(plan.collect_dir.join("agent-stderr.txt"), &agent.stderr);
    }
    listing(cell, "agent", &mut report.listings);
    // ST4 — seal is the store-side row; the drive records the boundary,
    // then the kernel-mediated `/logs/agent` copy-out (REQ-XC-317 —
    // after agent end, before verification, never through `/work`).
    listing(cell, "seal", &mut report.listings);
    if let Some(spec) = &plan.logs_copyout {
        report.native_log = Some(crate::capture::copy_out_logs(cell, spec)?);
    }
    // ST5 — verify legs in the same cell, bounded; the participant is
    // never re-launched. Each attempt collects under
    // `collect/attempt-<n>`: a lapsed attempt's exec is confirmed dead
    // before the next `rm_glob`/attempt (the exec's own lapse path),
    // attempts never overlap, and the collected files belong to
    // exactly one attempt — the top-level `collect/` members always
    // name the LATEST attempt's bytes, never a stale earlier write.
    for attempt in 1..=plan.max_grade_attempts {
        report.verify_attempts = attempt;
        cell.rm_glob(REWARD_GLOB)?;
        if !cell.present(TESTS_DIR)? {
            stage_in(cell, &plan.host_staging.join("tests"), TESTS_DIR)?;
        }
        // The reward sink is lifecycle-owned like `/tests`: a verifier
        // whose bootstrap dies before anything creates the dir (on the
        // allowlist_mediated profile a denied apt/uv bootstrap never
        // reaches the pytest leg whose ctrf plugin creates
        // `/logs/verifier`) must still land its trailing `reward.*`
        // write — otherwise a verifier that ran to completion records
        // `reward_missing` for a reward it produced (the Wc2/Wc3
        // H-allow signature).
        cell.exec(&CellExec {
            argv: vec![
                "mkdir".to_string(),
                "-p".to_string(),
                VERIFIER_DIR.to_string(),
            ],
            cwd: CELL_ROOT.to_string(),
            deadline_ms: None,
        })?;
        let out = cell.exec(&CellExec {
            argv: plan.verifier_argv.clone(),
            cwd: plan.workdir.clone(),
            deadline_ms: plan.verifier_deadline_ms,
        })?;
        listing(cell, "verify", &mut report.listings);
        // Copy-out — the reward-side bytes the channel classifies,
        // under this attempt's own dir.
        let attempt_dir = plan.collect_dir.join(format!("attempt-{attempt}"));
        let _ = std::fs::create_dir_all(&attempt_dir);
        let mut files = crate::verdict::RewardFileSet::default();
        let _ = std::fs::create_dir_all(&plan.collect_dir);
        for name in ["reward.json", "reward.txt", "ctrf.json", "test-stdout.txt"] {
            let dst = attempt_dir.join(name);
            if cell.cp_out(&format!("{VERIFIER_DIR}/{name}"), &dst).is_ok() {
                let bytes = std::fs::read(&dst).unwrap_or_default();
                match name {
                    "reward.json" => files.reward_json = Some(bytes),
                    "reward.txt" => files.reward_txt = Some(bytes),
                    "ctrf.json" => files.ctrf_json = Some(bytes),
                    _ => files.test_stdout = Some(bytes),
                }
            }
        }
        // The verifier's own streams are the bootstrap evidence: a
        // `test.sh` that dies before its pytest leg (a denied/errored
        // apt/curl/uvx bootstrap — the Wc5/Wc6 H-allow signature) writes
        // no `/logs/verifier` file of its own, and the task's frozen
        // script never redirects one. The exec capture is recorded
        // verbatim — `test-stdout.txt` fills only the absent in-cell
        // member (the task's own file wins), `test-stderr.txt` rides
        // beside it so the §3.4 signature check sees `E:`/`curl:` lines
        // on either stream. A lapsed attempt's partial streams land
        // here too — the lapse no longer drops them (D17).
        if files.test_stdout.is_none() && !out.stdout.is_empty() {
            let dst = attempt_dir.join("test-stdout.txt");
            let _ = std::fs::write(&dst, &out.stdout);
            files.test_stdout = Some(out.stdout.clone());
        }
        if !out.stderr.is_empty() {
            let dst = attempt_dir.join("test-stderr.txt");
            let _ = std::fs::write(&dst, &out.stderr);
            files.test_stderr = Some(out.stderr.clone());
        }
        // Re-materialize the top-level `collect/` members as exactly
        // this attempt's bytes: a later attempt's absence is an absent
        // file, never an earlier attempt's leftover (D17 — the Wc7
        // `query-optimize` leak let a timed-out attempt's late write
        // land under a later attempt's verdict).
        for name in [
            "reward.json",
            "reward.txt",
            "ctrf.json",
            "test-stdout.txt",
            "test-stderr.txt",
        ] {
            let top = plan.collect_dir.join(name);
            let _ = std::fs::remove_file(&top);
            if let Ok(b) = std::fs::read(attempt_dir.join(name)) {
                let _ = std::fs::write(&top, b);
            }
        }
        report.verify = Some(VerifyOutcome {
            attempt,
            files,
            timed_out: out.timed_out,
        });
        let decided = !out.timed_out
            && (report
                .verify
                .as_ref()
                .map(|v| v.files.reward_json.is_some() || v.files.reward_txt.is_some())
                .unwrap_or(false));
        if decided {
            break;
        }
    }
    listing(cell, "record", &mut report.listings);
    Ok(report)
}
