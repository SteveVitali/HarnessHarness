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

use crate::disposition::EndCause;

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
        use std::io::Read;
        let mut cmd = self.backend.exec_command(&spec.argv, &spec.cwd, &[]);
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .stdin(std::process::Stdio::null());
        let mut child = cmd.spawn().map_err(|e| err("exec", e.to_string()))?;
        let deadline = spec
            .deadline_ms
            .map(|ms| std::time::Instant::now() + std::time::Duration::from_millis(ms));
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                let mut stdout = Vec::new();
                if let Some(mut s) = child.stdout.take() {
                    let _ = s.read_to_end(&mut stdout);
                }
                return Ok(CellOutcome {
                    exit_code: status.code().map(|c| c as i64),
                    timed_out: false,
                    stdout,
                });
            }
            if let Some(d) = deadline {
                if std::time::Instant::now() > d {
                    let _ = child.kill();
                    let _ = child.wait();
                    self.terminate_group("agent")?;
                    return Ok(CellOutcome {
                        exit_code: None,
                        timed_out: true,
                        stdout: Vec::new(),
                    });
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    fn terminate_group(&mut self, pattern: &str) -> Result<(), CellError> {
        self.backend
            .terminate_group(pattern)
            .map_err(|e| err("terminate", e.to_string()))
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
    /// The verifier argv (`bash /tests/test.sh`).
    pub verifier_argv: Vec<String>,
    /// The enforced verifier deadline (ms).
    pub verifier_deadline_ms: Option<u64>,
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
        native_log: None,
    };
    // ST1 boundary — nothing phase-owned may already exist.
    if cell.present(TESTS_DIR)? {
        return Err(err(
            "phase_boundary",
            format!("{TESTS_DIR} exists before the verify phase — the image ships a boundary file the lifecycle never creates"),
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
    listing(cell, "agent", &mut report.listings);
    // ST4 — seal is the store-side row; the drive records the boundary,
    // then the kernel-mediated `/logs/agent` copy-out (REQ-XC-317 —
    // after agent end, before verification, never through `/work`).
    listing(cell, "seal", &mut report.listings);
    if let Some(spec) = &plan.logs_copyout {
        report.native_log = Some(crate::capture::copy_out_logs(cell, spec)?);
    }
    // ST5 — verify legs in the same cell, bounded; the participant is
    // never re-launched.
    for _attempt in 1..=plan.max_grade_attempts {
        report.verify_attempts += 1;
        cell.rm_glob(REWARD_GLOB)?;
        if !cell.present(TESTS_DIR)? {
            stage_in(cell, &plan.host_staging.join("tests"), TESTS_DIR)?;
        }
        let out = cell.exec(&CellExec {
            argv: plan.verifier_argv.clone(),
            cwd: plan.workdir.clone(),
            deadline_ms: plan.verifier_deadline_ms,
        })?;
        listing(cell, "verify", &mut report.listings);
        // Copy-out — the reward-side bytes the channel classifies.
        let mut files = crate::verdict::RewardFileSet::default();
        let _ = std::fs::create_dir_all(&plan.collect_dir);
        for name in ["reward.json", "reward.txt", "ctrf.json", "test-stdout.txt"] {
            let dst = plan.collect_dir.join(name);
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
        report.verify = Some(VerifyOutcome {
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
