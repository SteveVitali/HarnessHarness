//! `HelperClient` + `HelperExecutor` — the kernel side of the live
//! `hh-helper/1` boundary (S2.1; §5d.5 §4; ADR-0050; R-2.5.5). One
//! `hh-helper` *process* per contained environment: the driver spawns it at
//! `attach` (`--backend seatbelt` for `local_sandboxed`, `--backend podman`
//! for `local_container`), opens the bridged `AF_UNIX` channel, and the
//! executor stages `exec`/`read`/`probe`/`cancel` through it.
//!
//! The kernel's posture (I-3, enforced structurally):
//!
//! - the helper never sees a `Proposal`/`KernelDecision` — `exec` carries
//!   the resolved request's members plus a `commit_proof` the kernel mints
//!   over the session nonce (delivered on `hello`, never ledgered — the
//!   proof is `idp_id("commit_token", nonce ∥ members)`);
//! - every journaled frame echoes the request's attribution token — the
//!   dispatcher's `resolver` drops an unresolvable echo (`unattributed`);
//! - a malformed response/frame is `protocol_error` (transport origin —
//!   `unknown` → probe, never a silent redispatch);
//! - a helper that dies mid-exec is `environment_unavailable`-shaped at the
//!   transport plane and lands the handle `unreachable` (the driver's heal
//!   ladder decides `reattached`/`replaced`/`failed`).

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use hh_containment::policy::IsolationClass;
use hh_hir::kinds::EffectDomain;
use hh_wire::json::{self, Json};

use crate::capture::CaptureKind;
use crate::errors::EnvError;
use crate::executor::{
    DedupSupport, ExecutionRequest, ExecutorDeclaration, ExecutorSignal, InterruptSupport,
    ProbeSupport, ProbeVerdict, TerminalReport, TerminalStatus, ToolExecutor,
};
use crate::observe::ErrorClass;

use hh_helper::protocol::{CommitProof, HelperRequest, HelperResponse, OnKernelLoss};

/// `helper_binary()` — resolve the `hh-helper` binary: `HH_HELPER_BIN`
/// first (the operator/test pin), then the cargo target dir beside the
/// current test/bin executable (`deps/../hh-helper`), then `hh-helper` on
/// PATH.
pub fn helper_binary() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("HH_HELPER_BIN") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        // target/{debug,release}/deps/<test> → target/{debug,release}/hh-helper
        if let Some(dir) = exe.parent().and_then(|d| d.parent()) {
            let cand = dir.join("hh-helper");
            if cand.exists() {
                return Some(cand);
            }
        }
        if let Some(dir) = exe.parent() {
            let cand = dir.join("hh-helper");
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    // PATH lookup.
    if let Ok(path) = std::env::var("PATH") {
        for d in path.split(':') {
            let cand = Path::new(d).join("hh-helper");
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    None
}

/// `HelperClient` — a live session to one spawned `hh-helper`.
pub struct HelperClient {
    /// The helper process (kept for teardown; `kill` on drop).
    child: Child,
    /// The session channel (NDJSON over the unix socket).
    reader: BufReader<UnixStream>,
    /// The write half.
    writer: UnixStream,
    /// The session id (`hello`'s reply).
    pub session_id: String,
    /// The session nonce (the `commit_proof` recompute key — kernel-held,
    /// never ledgered).
    pub session_nonce: String,
    /// The backend the helper runs (`seatbelt|container|direct`).
    pub backend: String,
    /// The socket path (for `mark_unreachable`/`heal` resume).
    pub socket: PathBuf,
    /// Per-class meter: exec spawns this session (`helper.spawns`).
    pub spawns: u64,
}

impl std::fmt::Debug for HelperClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "HelperClient{{session:{} backend:{}}}",
            self.session_id, self.backend
        )
    }
}

impl HelperClient {
    /// `spawn(socket_dir, backend, extra_args)` — launch `hh-helper`, wait
    /// for the socket, connect. `extra_args` carries the podman container
    /// spec (`--container/--image/--workspace`) for `local_container`.
    pub fn spawn(
        socket_dir: &Path,
        backend: &str,
        extra_args: &[String],
    ) -> Result<HelperClient, EnvError> {
        let bin = helper_binary().ok_or_else(|| {
            EnvError::Blob("hh-helper binary not found (set HH_HELPER_BIN)".into())
        })?;
        std::fs::create_dir_all(socket_dir).map_err(|e| EnvError::Blob(e.to_string()))?;
        let sock = socket_dir.join("helper.sock");
        let _ = std::fs::remove_file(&sock);
        let mut cmd = Command::new(&bin);
        cmd.arg("--socket").arg(&sock);
        if backend != "direct" {
            cmd.arg("--backend").arg(backend);
        }
        cmd.args(extra_args);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| EnvError::Blob(format!("helper spawn: {e}")))?;
        // Wait for the socket (bounded — a helper that can't start is a
        // spawn failure carrying its verbatim stderr, never a hang).
        let deadline = Instant::now() + Duration::from_secs(10);
        while !sock.exists() {
            if let Ok(Some(status)) = child.try_wait() {
                let mut err = String::new();
                if let Some(mut se) = child.stderr.take() {
                    use std::io::Read;
                    let _ = se.read_to_string(&mut err);
                }
                return Err(EnvError::Blob(format!(
                    "helper exited before bind: {status}: {}",
                    err.chars().take(512).collect::<String>()
                )));
            }
            if Instant::now() > deadline {
                return Err(EnvError::Blob("helper socket never appeared".into()));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // bind(2) creates the socket file before listen(2) arms it — a
        // `connect` inside that window is ECONNREFUSED; retry briefly
        // rather than misreport a healthy helper as a spawn failure.
        let stream = {
            let mut last = None;
            let mut stream = None;
            let retry_deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < retry_deadline {
                match UnixStream::connect(&sock) {
                    Ok(s) => {
                        stream = Some(s);
                        break;
                    }
                    Err(e) => {
                        last = Some(e);
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            }
            stream.ok_or_else(|| {
                EnvError::Blob(format!(
                    "helper connect: {}",
                    last.map(|e| e.to_string())
                        .unwrap_or_else(|| "timeout".into())
                ))
            })?
        };
        let reader = BufReader::new(
            stream
                .try_clone()
                .map_err(|e| EnvError::Blob(format!("helper channel: {e}")))?,
        );
        Ok(HelperClient {
            child,
            reader,
            writer: stream,
            session_id: String::new(),
            session_nonce: String::new(),
            backend: backend.to_string(),
            socket: sock,
            spawns: 0,
        })
    }

    /// `connect(socket)` — attach to a *running* helper (the reattach path —
    /// kernel death ≠ environment death; the session resumes).
    pub fn connect(socket: &Path) -> Result<HelperClient, EnvError> {
        let stream = UnixStream::connect(socket)
            .map_err(|e| EnvError::Blob(format!("helper reconnect: {e}")))?;
        let reader = BufReader::new(
            stream
                .try_clone()
                .map_err(|e| EnvError::Blob(format!("helper channel: {e}")))?,
        );
        // The child handle is not ours on resume — a reaped pid placeholder.
        let child = Command::new("/bin/true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| EnvError::Blob(e.to_string()))?;
        Ok(HelperClient {
            child,
            reader,
            writer: stream,
            session_id: String::new(),
            session_nonce: String::new(),
            backend: "resumed".to_string(),
            socket: socket.to_path_buf(),
            spawns: 0,
        })
    }

    /// `hello(...)` — open (or resume) the session.
    #[allow(clippy::too_many_arguments)]
    pub fn hello(
        &mut self,
        on_kernel_loss: OnKernelLoss,
        session_nonce: &str,
        dedup_window_ms: Option<u64>,
        policy: Option<&Json>,
        roots: Option<(Vec<String>, Vec<String>)>,
        resume: bool,
    ) -> Result<Json, EnvError> {
        let reply = self.request(&HelperRequest::Hello {
            client: "hh-kernel".into(),
            resume_session_id: if resume && !self.session_id.is_empty() {
                Some(self.session_id.clone())
            } else {
                None
            },
            on_kernel_loss,
            session_nonce: session_nonce.to_string(),
            dedup_window_ms,
            policy: policy.cloned(),
            roots,
        })?;
        if let Some(id) = reply.get("session_id").and_then(Json::as_str) {
            self.session_id = id.to_string();
        }
        self.session_nonce = session_nonce.to_string();
        if let Some(b) = reply.get("backend").and_then(Json::as_str) {
            self.backend = b.to_string();
        }
        Ok(reply)
    }

    /// One request → the `ok` payload (or the typed refusal).
    pub fn request(&mut self, req: &HelperRequest) -> Result<Json, EnvError> {
        self.request_json(&req.to_json())
    }

    /// `request_json(j)` — send an arbitrary `Json` frame (the malformed-/
    /// unknown-verb probe path — the typed refusal is the answer).
    pub fn request_json(&mut self, j: &Json) -> Result<Json, EnvError> {
        let line = j.to_canonical_string();
        self.writer
            .write_all(line.as_bytes())
            .and_then(|_| self.writer.write_all(b"\n"))
            .and_then(|_| self.writer.flush())
            .map_err(|e| EnvError::Transport {
                detail: format!("helper send: {e}"),
            })?;
        let mut buf = String::new();
        self.reader
            .read_line(&mut buf)
            .map_err(|e| EnvError::Transport {
                detail: format!("helper recv: {e}"),
            })?;
        if buf.is_empty() {
            return Err(EnvError::Transport {
                detail: "helper closed the channel".into(),
            });
        }
        let j = json::parse(buf.trim_end()).map_err(|e| EnvError::Transport {
            detail: format!("helper frame decode: {e}"),
        })?;
        match HelperResponse::from_json(&j).map_err(|e| EnvError::Transport {
            detail: format!("helper response decode: {e}"),
        })? {
            HelperResponse::Ok(payload) => Ok(payload),
            HelperResponse::Error { class, detail } => {
                Err(EnvError::HelperRefused { class, detail })
            }
        }
    }

    /// `is_live()` — the session channel still answers (the heal path's
    /// `session_still_live` check: `list_detached` round-trips).
    pub fn is_live(&mut self) -> bool {
        self.request(&HelperRequest::ListDetached).is_ok()
    }

    /// `shutdown` — the driver's teardown path.
    pub fn shutdown(&mut self) {
        let _ = self.request(&HelperRequest::Shutdown);
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
    }

    /// `kill_now()` — SIGKILL the helper without the `shutdown` verb (the
    /// crash-injection seam — a dead helper is the `environment_unavailable`
    /// / `unknown{executor_error}` path, never a silent redispatch).
    pub fn kill_now(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for HelperClient {
    fn drop(&mut self) {
        // Dropping the client without `shutdown` still reaps the child —
        // `on_kernel_loss` semantics belong to the helper (it saw the
        // channel close); here we just don't leak a process we spawned.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `CommitEvidence` — what `commit` stage 4 hands the executor so the
/// helper's `commit_proof` admission check can recompute the write-ahead
/// (§5d.5 §4 `commit_token`; ADR-0100 I-1). `ReadOnly` when the effect's
/// class carries no write-ahead.
#[derive(Debug, Clone)]
pub enum CommitEvidence {
    /// The durable `committed` event (id + seq + fencing token).
    Committed {
        /// The `action.effect.committed` event id.
        event_id: String,
        /// Its durable seq.
        seq: u64,
        /// The lease generation.
        fencing_token: u64,
    },
    /// `read_only` — no write-ahead.
    ReadOnly,
}

/// `HelperExecutor` — the `ToolExecutor` over a live helper session
/// (`local_sandboxed` via seatbelt; `local_container` via podman — the
/// backend string is the declaration's honest isolation class).
pub struct HelperExecutor {
    decl: ExecutorDeclaration,
    /// The session (the driver owns lifetime; the executor borrows per
    /// `execute` — the `ToolExecutor` trait is synchronous).
    pub client: HelperClient,
    /// The projected env (placeholders only).
    projected_env: Vec<(String, String)>,
}

impl HelperExecutor {
    /// An executor over `client` — `isolation_support`/`dedup_support`/
    /// `interrupt`/`probe` are declared from the live backend (the
    /// declaration is honest: `process_sandbox` on seatbelt, `namespaces`
    /// on the container, `durable` dedup under `tier-c1`).
    pub fn new(client: HelperClient, projected_env: Vec<(String, String)>) -> Self {
        let isolation = match client.backend.as_str() {
            "container" | "podman" => IsolationClass::Namespaces,
            "seatbelt" => IsolationClass::ProcessSandbox,
            _ => IsolationClass::None,
        };
        HelperExecutor {
            decl: ExecutorDeclaration {
                executor_id: format!("hh-helper-exec/1:{}", client.backend),
                isolation_support: isolation,
                // The helper's journal + dedup table are durable for the
                // session's life under `preserve_until` (tier-c1 builds;
                // a `tier-c1`-less helper refuses a `dedup_window` hello —
                // `start_helper` declares the window, so an unsupported
                // build fails the attach honestly, never degrades).
                dedup_support: DedupSupport::Durable,
                probe_support: ProbeSupport::Check,
                interrupt: InterruptSupport::Supported,
                error_classes: [
                    "invalid_arguments",
                    "not_found",
                    "conflict",
                    "executor_error",
                    "timeout",
                    "cancelled",
                    "containment_denied",
                    "signalled",
                    "output_cap_exceeded",
                    "environment_unavailable",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect::<BTreeSet<String>>(),
                streams: true,
                domains: [
                    EffectDomain::Exec,
                    EffectDomain::SpawnProcess,
                    EffectDomain::FsRead,
                    EffectDomain::FsWrite,
                ]
                .iter()
                .copied()
                .collect(),
            },
            client,
            projected_env,
        }
    }

    /// The session nonce (the driver reads it to mint `commit_proof`s —
    /// kernel-held, never in a payload).
    pub fn session_nonce(&self) -> &str {
        &self.client.session_nonce
    }
}

/// `failure class → ErrorClass` — the helper's verbatim failure classes map
/// onto the closed sum (the helper emits only kernel-shaped classes —
/// anything else is `protocol_error` at the transport plane).
fn failure_class(class: &str) -> ErrorClass {
    match class {
        "deadline_exceeded" => ErrorClass::Timeout,
        "cancelled" => ErrorClass::Cancelled {
            by: crate::observe::CancelBy::Protocol,
        },
        "sandbox_denied" => ErrorClass::ContainmentDenied {
            kind: "sandbox".to_string(),
        },
        "signalled" => ErrorClass::Signalled {
            signal: "kill".to_string(),
        },
        "terminated" => ErrorClass::Cancelled {
            by: crate::observe::CancelBy::RunEnd,
        },
        _ => ErrorClass::ExecutorError,
    }
}

impl ToolExecutor for HelperExecutor {
    fn declaration(&self) -> &ExecutorDeclaration {
        &self.decl
    }

    fn execute(
        &mut self,
        request: &ExecutionRequest,
        sink: &mut dyn FnMut(ExecutorSignal),
    ) -> Result<TerminalReport, EnvError> {
        self.client.spawns += 1;
        let commit_proof = match &request.commit_evidence {
            CommitEvidence::Committed {
                event_id,
                seq,
                fencing_token,
            } => CommitProof::Token {
                proof: hh_helper::protocol::mint_commit_proof(
                    &self.client.session_nonce,
                    &request.effect_id,
                    request.attempt_no,
                    *fencing_token,
                    event_id,
                    *seq,
                ),
                effect_id: request.effect_id.clone(),
                attempt_no: request.attempt_no,
                fencing_token: *fencing_token,
                commit_event_id: event_id.clone(),
                commit_seq: *seq,
            },
            CommitEvidence::ReadOnly => CommitProof::ReadOnly,
        };
        let cwd = request
            .args
            .get("cwd")
            .and_then(Json::as_str)
            .unwrap_or(".")
            .to_string();
        let reply = self.client.request(&HelperRequest::Exec {
            execution_id: request.execution_id.clone(),
            effect_id: request.effect_id.clone(),
            attempt_no: request.attempt_no,
            capability_ref: request.capability_ref.clone(),
            args: request.args.clone(),
            cwd,
            env: self.projected_env.clone(),
            deadline_ms: request.deadline_ms,
            retain_bytes_cap: request.retain_bytes_cap,
            attribution_token: request.attribution_token.clone(),
            commit_proof,
            idempotency_key: Some(request.idempotency_key.clone()),
        })?;
        // An executor-side dedup hit returns the recorded verdict — the
        // effect was *not* re-executed (R-2.5.5¹).
        if let Some(v) = reply.get("dedup_verdict") {
            let exit = v.get("exit_status").and_then(Json::as_int);
            let failure = v.get("failure");
            let status = match failure {
                Some(Json::Obj(f)) => TerminalStatus::ToolError {
                    class: failure_class(
                        f.get("class")
                            .and_then(Json::as_str)
                            .unwrap_or("executor_error"),
                    ),
                },
                _ if exit == Some(0) => TerminalStatus::Ok,
                _ => TerminalStatus::ToolError {
                    class: ErrorClass::ExecutorError,
                },
            };
            return Ok(TerminalReport {
                status,
                exit_status: exit,
                outcome_hint: if exit == Some(0) {
                    "applied".into()
                } else {
                    "not_applied".into()
                },
                retryable_hint: None,
                detail_ref: None,
                truncated: false,
                omitted_bytes: 0,
                original_size: 0,
            });
        }
        // Poll the journal — `read{after_seq, wait_ms}` until `exited`.
        let mut after_seq = 0u64;
        let mut dropped = 0u64;
        loop {
            let r = self.client.request(&HelperRequest::Read {
                execution_id: request.execution_id.clone(),
                after_seq,
                max_bytes: 0,
                wait_ms: 100,
            })?;
            if let Json::Arr(chunks) = r.get("chunks").cloned().unwrap_or(Json::Arr(vec![])) {
                for c in &chunks {
                    if let Ok(f) = hh_helper::protocol::HelperFrame::from_json(c) {
                        match f {
                            hh_helper::protocol::HelperFrame::Chunk {
                                seq,
                                stream,
                                data,
                                token,
                            } => {
                                after_seq = after_seq.max(seq);
                                sink(ExecutorSignal {
                                    token,
                                    kind: CaptureKind::OutputChunk { stream, data },
                                });
                            }
                            hh_helper::protocol::HelperFrame::Process {
                                seq,
                                transition,
                                process_ref,
                                token,
                            } => {
                                after_seq = after_seq.max(seq);
                                let t = match transition.as_str() {
                                    "spawned" => crate::capture::ProcessTransition::Spawned,
                                    "detached" => crate::capture::ProcessTransition::Detached,
                                    "signalled" => crate::capture::ProcessTransition::Signalled,
                                    _ => crate::capture::ProcessTransition::Exited,
                                };
                                sink(ExecutorSignal {
                                    token,
                                    kind: CaptureKind::Process {
                                        transition: t,
                                        process_ref,
                                    },
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
            if matches!(r.get("truncated"), Some(Json::Bool(true))) {
                dropped = dropped.max(1);
            }
            if matches!(r.get("exited"), Some(Json::Bool(true))) {
                let exit_status = r.get("exit_status").and_then(Json::as_int);
                let failure = r.get("failure");
                let truncated = matches!(r.get("truncated"), Some(Json::Bool(true)));
                return Ok(match failure {
                    Some(Json::Obj(f)) => {
                        let class = failure_class(
                            f.get("class")
                                .and_then(Json::as_str)
                                .unwrap_or("executor_error"),
                        );
                        TerminalReport {
                            status: TerminalStatus::ToolError { class },
                            exit_status,
                            outcome_hint: "unknown".into(),
                            retryable_hint: Some(false),
                            detail_ref: f
                                .get("detail")
                                .and_then(Json::as_str)
                                .map(|d| d[..d.len().min(160)].to_string()),
                            truncated,
                            omitted_bytes: dropped,
                            original_size: dropped,
                        }
                    }
                    _ if exit_status == Some(0) => TerminalReport {
                        status: TerminalStatus::Ok,
                        exit_status,
                        outcome_hint: "applied".into(),
                        retryable_hint: None,
                        detail_ref: None,
                        truncated,
                        omitted_bytes: dropped,
                        original_size: dropped,
                    },
                    _ => TerminalReport {
                        status: TerminalStatus::ToolError {
                            class: ErrorClass::ExecutorError,
                        },
                        exit_status,
                        outcome_hint: "not_applied".into(),
                        retryable_hint: None,
                        detail_ref: None,
                        truncated,
                        omitted_bytes: dropped,
                        original_size: dropped,
                    },
                });
            }
        }
    }

    fn probe(&self, effect_id: &str, attempt_no: u64) -> Result<ProbeVerdict, EnvError> {
        // `probe` resolves to the journal — the durable exec record is the
        // probe's answer (did the lapsed-window effect apply?). The journal
        // verbs require a session, so a probe re-hello's on the same nonce
        // (resume) over a fresh channel — `probe` takes `&self`.
        let mut client = HelperClient::connect(&self.client.socket)?;
        client.hello(
            OnKernelLoss::PreserveUntil { ttl_ms: 86_400_000 },
            &self.client.session_nonce,
            None,
            None,
            None,
            !self.client.session_id.is_empty(),
        )?;
        match client.request(&HelperRequest::Probe {
            effect_id: effect_id.to_string(),
            attempt_no,
        }) {
            Ok(r) => {
                if matches!(r.get("exited"), Some(Json::Bool(true))) {
                    if r.get("exit_status").and_then(Json::as_int) == Some(0)
                        && r.get("failure").is_none()
                    {
                        Ok(ProbeVerdict::Applied)
                    } else {
                        Ok(ProbeVerdict::NotApplied)
                    }
                } else {
                    Ok(ProbeVerdict::Undeterminable)
                }
            }
            Err(_) => Ok(ProbeVerdict::Undeterminable),
        }
    }
}
