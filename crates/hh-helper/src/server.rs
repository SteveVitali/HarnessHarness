//! The helper server — one `Session` per channel (§5d.5 §4). Requests are
//! read, strictly decoded, dispatched, and answered; `exec`/`read` are
//! asynchronous (the journal is the durable store `read`/`probe` replay).
//! The helper is a stateless-adjacent executor: it never mints tokens,
//! never decides authorization, never decides lifecycle — it executes,
//! journals, enforces deadlines and the fs gate, and reports verbatim.
//!
//! `on_kernel_loss`: when the channel drops (EOF/error) with
//! `preserve_until` (C1), the session *keeps* the exec table + journals —
//! a `hello{resume_session_id}` on a fresh channel reattaches and `read`
//! resumes where the kernel left off. With `terminate`, running groups are
//! killed and the table dropped.

use std::io;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hh_containment::admit::{classify_read, classify_write, ReadClass, WriteClass};
use hh_containment::policy::ContainmentPolicy;
use hh_wire::json::Json;

use crate::exec::{
    group_alive, kill_group, run_child, spawn_pgrp_leader, ExecState, Execution, Shared,
};
use crate::fstree;
use crate::protocol::{verify_commit_proof, HelperRequest, HelperResponse, OnKernelLoss};
use crate::seatbelt;
use crate::wire::Channel;

/// The helper's session id prefix.
pub const SESSION_PREFIX: &str = "hh-helper-sess";

/// `HelperServer` — the process-level state one `hh-helper` binary owns.
pub struct HelperServer {
    /// Session state (exec table + notify + nonce + posture).
    pub shared: Arc<Shared>,
    /// The session id (`resume_session_id` reattaches it).
    pub session_id: String,
    /// The containment policy the fs gate + spawn profile enforce.
    pub policy: Mutex<Option<ContainmentPolicy>>,
    /// The fs roots (workspace = readable+writable; extra writable).
    pub roots: Mutex<(Vec<PathBuf>, Vec<PathBuf>)>,
    /// The `local_container` backend (when the environment class is one).
    pub container: Mutex<Option<crate::podman::PodmanBackend>>,
    /// The spawn backend label (`seatbelt` | `container` | `direct` — the
    /// unconfined local_host lane when no policy is attached).
    pub backend: Mutex<&'static str>,
}

impl HelperServer {
    /// New server (pre-hello).
    pub fn new() -> HelperServer {
        // The session id is content-derived over the process + boot instant
        // (identity discipline — no ad-hoc randomness; `idp_id` is the one
        // scheme, CC1).
        let basis = format!("{}:{:?}", std::process::id(), std::time::SystemTime::now());
        HelperServer {
            shared: Shared::new(),
            session_id: format!(
                "{}-{}",
                SESSION_PREFIX,
                hh_identity::idp::idp_id("helper_session", basis.as_bytes())
            ),
            policy: Mutex::new(None),
            roots: Mutex::new((vec![], vec![])),
            container: Mutex::new(None),
            backend: Mutex::new("direct"),
        }
    }
}

impl Default for HelperServer {
    fn default() -> HelperServer {
        HelperServer::new()
    }
}

/// `serve(stream)` — the session loop: decode → dispatch → reply until EOF.
/// On channel drop the session applies `on_kernel_loss` (terminate kills
/// live groups; `preserve_until` keeps the table alive for `ttl` — a fresh
/// `serve` on the resumed socket returns to this server's state via the
/// `HH_HELPER_STATE` handoff in the same process… at S2.1 the helper is
/// one-process-per-environment, so `preserve_until` means the *process*
/// keeps serving a reconnect on its socket path — `main` loops `accept`).
pub fn serve(stream: UnixStream, srv: &Arc<HelperServer>) -> io::Result<()> {
    let mut ch = Channel::new(stream)?;
    loop {
        match ch.recv() {
            Ok(Some(j)) => match HelperRequest::from_json(&j) {
                Ok(req) => {
                    let (reply, done) = dispatch(srv, &ch, req);
                    if let Some(r) = reply {
                        if ch.send(&r.to_json()).is_err() {
                            break;
                        }
                    }
                    if done {
                        break;
                    }
                }
                Err(e) => {
                    let _ =
                        ch.send(&HelperResponse::err("protocol_error", e.to_string()).to_json());
                }
            },
            Ok(None) => break, // kernel hung up — apply on_kernel_loss
            Err(e) => {
                let _ = ch.send(&HelperResponse::err("protocol_error", e.to_string()).to_json());
                break;
            }
        }
    }
    // Kernel-loss posture.
    if *srv.shared.on_kernel_loss_terminate.lock().unwrap() {
        terminate_all(srv);
    }
    // `preserve_until` (C1): the table survives; `main`'s accept-loop keeps
    // the socket open for `resume_session_id`.
    Ok(())
}

fn terminate_all(srv: &Arc<HelperServer>) {
    let table = srv.shared.table.lock().unwrap();
    for ex in table.map.values() {
        let e = ex.lock().unwrap();
        if let Some(p) = e.pgid {
            if e.state == ExecState::Running {
                let _ = kill_group(p, 15);
            }
        }
        drop(e);
    }
}

/// `dispatch` — one request → (reply, close-session?). `Ok(None)` reply
/// means the handler already wrote the frames (none today — every reply
/// is a single response object; `read` carries the frames inside).
fn dispatch(
    srv: &Arc<HelperServer>,
    _ch: &Channel,
    req: HelperRequest,
) -> (Option<HelperResponse>, bool) {
    match req {
        HelperRequest::Hello {
            resume_session_id,
            on_kernel_loss,
            session_nonce,
            dedup_window_ms,
            policy,
            roots,
            ..
        } => {
            // `preserve_until` is C1 — refused without tier-c1.
            #[cfg(not(feature = "tier-c1"))]
            if matches!(on_kernel_loss, OnKernelLoss::PreserveUntil { .. }) {
                return (
                    Some(HelperResponse::err(
                        "unsupported",
                        "preserve_until requires tier-c1 (removability(0) honest refusal)",
                    )),
                    false,
                );
            }
            #[cfg(feature = "tier-c1")]
            if dedup_window_ms.is_some() {
                // The durable store exists under tier-c1; the window applies.
            }
            #[cfg(not(feature = "tier-c1"))]
            if dedup_window_ms.is_some() {
                return (
                    Some(HelperResponse::err(
                        "unsupported",
                        "dedup_window requires tier-c1",
                    )),
                    false,
                );
            }
            *srv.shared.session_nonce.lock().unwrap() = session_nonce;
            *srv.shared.dedup_window_ms.lock().unwrap() = dedup_window_ms;
            *srv.shared.on_kernel_loss_terminate.lock().unwrap() =
                matches!(on_kernel_loss, OnKernelLoss::Terminate);
            if let Some(pj) = &policy {
                match serde_policy(pj) {
                    Ok(p) => {
                        *srv.policy.lock().unwrap() = Some(p);
                        if seatbelt::is_available() {
                            *srv.backend.lock().unwrap() = "seatbelt";
                        }
                    }
                    Err(e) => {
                        return (
                            Some(HelperResponse::err(
                                "containment_denied",
                                format!("policy decode: {e}"),
                            )),
                            false,
                        )
                    }
                }
            }
            if let Some((ws, wr)) = roots {
                *srv.roots.lock().unwrap() = (
                    ws.iter().map(PathBuf::from).collect(),
                    wr.iter().map(PathBuf::from).collect(),
                );
            }
            let resumed = resume_session_id
                .map(|s| s == srv.session_id)
                .unwrap_or(false);
            (
                Some(HelperResponse::Ok(Json::obj([
                    ("session_id", Json::str(srv.session_id.clone())),
                    ("resumed", Json::Bool(resumed)),
                    ("backend", Json::str(*srv.backend.lock().unwrap())),
                ]))),
                false,
            )
        }
        HelperRequest::Exec {
            execution_id,
            effect_id,
            attempt_no,
            capability_ref,
            args,
            cwd,
            env,
            deadline_ms,
            retain_bytes_cap,
            attribution_token,
            commit_proof,
            idempotency_key,
            env_clear,
        } => {
            let nonce = srv.shared.session_nonce.lock().unwrap().clone();
            if !verify_commit_proof(&nonce, &commit_proof, &effect_id, attempt_no) {
                return (
                    Some(HelperResponse::err(
                        "NotCommitted",
                        "commit_proof failed recompute/binding",
                    )),
                    false,
                );
            }
            // Executor-side dedup (tier-c1) — replayed key returns the
            // recorded verdict, no re-execution.
            #[cfg(feature = "tier-c1")]
            if let Some(key) = &idempotency_key {
                let window =
                    Duration::from_millis(srv.shared.dedup_window_ms.lock().unwrap().unwrap_or(0));
                let mut t = srv.shared.table.lock().unwrap();
                if let Some(v) = t.dedup.check(key, window) {
                    return (
                        Some(HelperResponse::Ok(Json::obj([
                            ("execution_id", Json::str("dedup")),
                            ("dedup_verdict", v),
                        ]))),
                        false,
                    );
                }
            }
            spawn_exec(
                srv,
                execution_id,
                effect_id,
                attempt_no,
                capability_ref,
                args,
                cwd,
                env,
                deadline_ms,
                retain_bytes_cap,
                attribution_token,
                idempotency_key,
                env_clear,
            )
        }
        HelperRequest::Read {
            execution_id,
            after_seq,
            max_bytes,
            wait_ms,
        } => read_exec(srv, &execution_id, after_seq, max_bytes, wait_ms),
        HelperRequest::Probe {
            effect_id,
            attempt_no,
        } => {
            let t = srv.shared.table.lock().unwrap();
            match t.by_effect.get(&(effect_id.clone(), attempt_no)) {
                Some(id) => {
                    let id = id.clone();
                    drop(t);
                    read_exec(srv, &id, 0, 0, 0)
                }
                None => (
                    Some(HelperResponse::err(
                        "invalid_request",
                        format!("no execution for {effect_id}:{attempt_no}"),
                    )),
                    false,
                ),
            }
        }
        HelperRequest::Cancel {
            execution_id,
            phase,
        } => {
            let t = srv.shared.table.lock().unwrap();
            if let Some(ex) = t.map.get(&execution_id) {
                let mut e = ex.lock().unwrap();
                e.cancel_phase = Some(phase.clone());
                if phase == "kill" {
                    if let Some(p) = e.pgid {
                        let _ = kill_group(p, 9);
                    }
                }
                drop(e);
                srv.shared.notify.notify_all();
                (Some(HelperResponse::Ok(Json::obj([]))), false)
            } else {
                (
                    Some(HelperResponse::err(
                        "invalid_request",
                        format!("unknown execution {execution_id}"),
                    )),
                    false,
                )
            }
        }
        HelperRequest::Terminate { execution_id } => {
            let t = srv.shared.table.lock().unwrap();
            let running = if let Some(ex) = t.map.get(&execution_id) {
                let mut e = ex.lock().unwrap();
                let was = e.state == ExecState::Running;
                if let Some(p) = e.pgid {
                    let _ = kill_group(p, 9);
                }
                e.state = ExecState::Finished;
                if e.failure.is_none() {
                    e.failure = Some(("terminated".into(), "terminate verb".into()));
                }
                was
            } else {
                false
            };
            drop(t);
            srv.shared.notify.notify_all();
            (
                Some(HelperResponse::Ok(Json::obj([(
                    "running",
                    Json::Bool(running),
                )]))),
                false,
            )
        }
        HelperRequest::Detach { execution_id } => {
            let t = srv.shared.table.lock().unwrap();
            let ok = if let Some(ex) = t.map.get(&execution_id) {
                let mut e = ex.lock().unwrap();
                e.state = ExecState::Detached;
                true
            } else {
                false
            };
            (
                if ok {
                    Some(HelperResponse::Ok(Json::obj([])))
                } else {
                    Some(HelperResponse::err(
                        "invalid_request",
                        format!("unknown execution {execution_id}"),
                    ))
                },
                false,
            )
        }
        HelperRequest::ListDetached => {
            let t = srv.shared.table.lock().unwrap();
            let mut out = Vec::new();
            for ex in t.map.values() {
                let e = ex.lock().unwrap();
                for d in &e.detached {
                    if group_alive_str(&d.process_ref) {
                        out.push(Json::obj([
                            ("process_ref", Json::str(d.process_ref.clone())),
                            ("execution_id", Json::str(d.execution_id.clone())),
                        ]));
                    }
                }
            }
            (
                Some(HelperResponse::Ok(Json::obj([(
                    "detached",
                    Json::Arr(out),
                )]))),
                false,
            )
        }
        HelperRequest::EnvApply { vars } => {
            // The helper holds placeholders only — the next `exec` receives
            // them on its own request env (env_apply seeds no secret state).
            (
                Some(HelperResponse::Ok(Json::obj([(
                    "applied",
                    Json::Int(vars.len() as i64),
                )]))),
                false,
            )
        }
        HelperRequest::FsRead { path } => fs_op(srv, &path, false, |p| {
            std::fs::read_to_string(p)
                .map(|c| Json::obj([("content", Json::str(c))]))
                .map_err(io_err)
        }),
        HelperRequest::FsWrite { path, content } => fs_op(srv, &path, true, |p| {
            std::fs::write(p, &content)
                .map(|_| Json::obj([]))
                .map_err(io_err)
        }),
        HelperRequest::FsCanonicalize { path } => fs_op(srv, &path, false, |p| {
            std::fs::canonicalize(p)
                .map(|c| Json::obj([("canonical", Json::str(c.to_string_lossy()))]))
                .map_err(io_err)
        }),
        HelperRequest::FsMetadata { path } => fs_op(srv, &path, false, |p| {
            std::fs::symlink_metadata(p)
                .map(|m| {
                    Json::obj([
                        (
                            "kind",
                            Json::str(if m.is_dir() {
                                "dir"
                            } else if m.file_type().is_symlink() {
                                "symlink"
                            } else {
                                "file"
                            }),
                        ),
                        ("size", Json::Int(m.len() as i64)),
                    ])
                })
                .map_err(io_err)
        }),
        HelperRequest::FsReadDir { path } => fs_op(srv, &path, false, |p| {
            std::fs::read_dir(p)
                .map(|rd| {
                    let mut names: Vec<String> = rd
                        .flatten()
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect();
                    names.sort();
                    Json::obj([(
                        "entries",
                        Json::Arr(names.iter().map(|n| Json::str(n.clone())).collect()),
                    )])
                })
                .map_err(io_err)
        }),
        HelperRequest::FsRemove { path } => fs_op(srv, &path, true, |p| {
            let md = std::fs::symlink_metadata(p).map_err(io_err)?;
            if md.is_dir() {
                std::fs::remove_dir_all(p).map_err(io_err)?;
            } else {
                std::fs::remove_file(p).map_err(io_err)?;
            }
            Ok(Json::obj([]))
        }),
        HelperRequest::Snapshot { roots, kind } => {
            if kind != "fs_tree" {
                return (
                    Some(HelperResponse::err(
                        "unsupported",
                        format!("snapshot kind {kind} — the helper computes fs_tree only"),
                    )),
                    false,
                );
            }
            match fstree::walk(&roots) {
                Ok(s) => {
                    let manifest = s.manifest_json();
                    (
                        Some(HelperResponse::Ok(Json::obj([
                            ("tree_address", Json::str(s.tree_address.clone())),
                            ("manifest", manifest),
                            ("size_bytes", Json::Int(s.size_bytes as i64)),
                        ]))),
                        false,
                    )
                }
                Err(e) => (Some(HelperResponse::err("runtime", e.to_string())), false),
            }
        }
        HelperRequest::Diff {
            roots,
            base_manifest,
        } => match (
            fstree::FsTreeSnapshot::from_manifest_json(&base_manifest),
            fstree::walk(&roots),
        ) {
            (Some(base), Ok(now)) => {
                let entries = fstree::diff(&base, &now);
                (
                    Some(HelperResponse::Ok(Json::obj([(
                        "changes",
                        Json::Arr(
                            entries
                                .iter()
                                .map(|d| {
                                    Json::obj([
                                        ("root", Json::str(d.root.clone())),
                                        ("relpath", Json::str(d.relpath.clone())),
                                        ("change", Json::str(d.change)),
                                        (
                                            "before",
                                            d.before
                                                .as_ref()
                                                .map_or(Json::Null, |b| Json::str(b.clone())),
                                        ),
                                        (
                                            "after",
                                            d.after
                                                .as_ref()
                                                .map_or(Json::Null, |a| Json::str(a.clone())),
                                        ),
                                    ])
                                })
                                .collect(),
                        ),
                    )]))),
                    false,
                )
            }
            (None, _) => (
                Some(HelperResponse::err(
                    "invalid_request",
                    "base_manifest decode failed",
                )),
                false,
            ),
            (_, Err(e)) => (Some(HelperResponse::err("runtime", e.to_string())), false),
        },
        HelperRequest::ProbeBoundary { kind } => probe_boundary(srv, &kind),
        HelperRequest::Shutdown => {
            if let Some(b) = srv.container.lock().unwrap().take() {
                let _ = b.stop();
            }
            terminate_all(srv);
            (Some(HelperResponse::Ok(Json::obj([]))), true)
        }
    }
}

fn group_alive_str(process_ref: &str) -> bool {
    process_ref
        .strip_prefix("pgrp:")
        .and_then(|p| p.parse::<u32>().ok())
        .map(group_alive)
        .unwrap_or(false)
}

fn io_err(e: io::Error) -> HelperResponse {
    let class = match e.kind() {
        io::ErrorKind::NotFound => "invalid_request",
        io::ErrorKind::PermissionDenied => "containment_denied",
        _ => "runtime",
    };
    HelperResponse::err(class, e.to_string())
}

/// The fs gate — F1's resolution-then-validation order applied to the
/// helper's own ops: canonicalize first (links, `..`), then classify
/// through the *single-source* matchers (`classify_write`/`classify_read`
/// — the same functions the admit floor and EP2 gate use; CC1). Fail
/// closed: absent policy ⇒ every fs.* verb is `containment_denied`.
fn fs_op<F: FnOnce(&Path) -> Result<Json, HelperResponse>>(
    srv: &Arc<HelperServer>,
    path: &str,
    write: bool,
    f: F,
) -> (Option<HelperResponse>, bool) {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    let canon_s = canon.to_string_lossy().to_string();
    let policy = srv.policy.lock().unwrap().clone();
    let denied = match (policy.as_ref(), write) {
        (None, _) => Some("no policy attached — the fs gate is closed".to_string()),
        (Some(p), true) => match classify_write(p, &canon_s) {
            WriteClass::Inside => None,
            WriteClass::OutsideRoot => Some(format!("{path} outside every WritableRoot")),
            WriteClass::Protected(n) => Some(format!("{path} hits protected name {n}")),
            WriteClass::ReadOnlySubpath(s) => Some(format!("{path} under read-only {s}")),
            WriteClass::Denied(d) => Some(format!("{path} denied by {d}")),
        },
        (Some(p), false) => match classify_read(p, &canon_s) {
            ReadClass::Allowed => None,
            ReadClass::Denied => Some(format!("{path} denied by read.deny")),
            ReadClass::NotInAllow => Some(format!("{path} not in read.allow")),
        },
    };
    if let Some(why) = denied {
        return (Some(HelperResponse::err("containment_denied", why)), false);
    }
    match f(&canon) {
        Ok(j) => (Some(HelperResponse::Ok(j)), false),
        Err(r) => (Some(r), false),
    }
}

fn serde_policy(j: &Json) -> Result<ContainmentPolicy, String> {
    ContainmentPolicy::from_json(j).map_err(|e| e.to_string())
}

/// `spawn_exec` — the `exec` verb's launch (async — replies
/// `{execution_id}` once the child is spawned or the failure recorded).
#[allow(clippy::too_many_arguments)]
fn spawn_exec(
    srv: &Arc<HelperServer>,
    execution_id: String,
    effect_id: String,
    attempt_no: u64,
    capability_ref: (String, String),
    args: Json,
    cwd: String,
    env: Vec<(String, String)>,
    deadline_ms: Option<u64>,
    retain_bytes_cap: u64,
    attribution_token: String,
    idempotency_key: Option<String>,
    env_clear: bool,
) -> (Option<HelperResponse>, bool) {
    // argv extraction — `command` runs through sh -c; `argv` is exec'd.
    let argv: Vec<String> = if let Some(s) = args.get("command").and_then(Json::as_str) {
        vec!["/bin/sh".into(), "-c".into(), s.into()]
    } else if let Some(Json::Arr(a)) = args.get("argv") {
        a.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    } else {
        return (
            Some(HelperResponse::err(
                "invalid_request",
                "args needs command|argv",
            )),
            false,
        );
    };
    if argv.is_empty() {
        return (
            Some(HelperResponse::err("invalid_request", "empty argv")),
            false,
        );
    }
    let ex = Arc::new(Mutex::new(Execution::new(
        execution_id.clone(),
        effect_id.clone(),
        attempt_no,
        attribution_token,
        retain_bytes_cap,
        deadline_ms,
    )));
    ex.lock().unwrap().idempotency_key = idempotency_key;
    // Build the command through the backend.
    let backend = *srv.backend.lock().unwrap();
    let mut cmd: Command = match backend {
        "seatbelt" => {
            let pol = srv.policy.lock().unwrap().clone();
            let profile = pol
                .as_ref()
                .map(seatbelt::seatbelt_profile)
                .unwrap_or_else(|| "(version 1)\n(deny default)\n".into());
            let av = seatbelt::spawn_argv(&profile, &argv);
            let mut c = Command::new(&av[0]);
            c.args(&av[1..]);
            c
        }
        "container" => match srv.container.lock().unwrap().clone() {
            Some(b) => b.exec_command(&argv, &cwd, &env),
            None => {
                return (
                    Some(HelperResponse::err(
                        "runtime",
                        "container backend not provisioned",
                    )),
                    false,
                )
            }
        },
        _ => {
            let mut c = Command::new(&argv[0]);
            c.args(&argv[1..]);
            c
        }
    };
    if backend != "container" {
        cmd.current_dir(&cwd);
        // `env_clear` — the child inherits nothing of the helper's ambient
        // environment; the projected `env` pairs are its whole environment
        // (S2.2: the `subprocess_confined` placement contract, §8.4 V4).
        if env_clear {
            cmd.env_clear();
        }
        for (k, v) in &env {
            cmd.env(k, v);
        }
        // A minimal, deterministic base env (CC: no ambient leakage).
        cmd.env(
            "HH_CAPABILITY",
            format!("{}:{}", capability_ref.0, capability_ref.1),
        );
        cmd.env("HH_EXECUTION_ID", &execution_id);
    }
    match spawn_pgrp_leader(&mut cmd) {
        Ok(child) => {
            let pgid = child.id();
            {
                let mut t = srv.shared.table.lock().unwrap();
                t.by_effect
                    .insert((effect_id.clone(), attempt_no), execution_id.clone());
                t.map.insert(execution_id.clone(), Arc::clone(&ex));
            }
            let sh = Arc::clone(&srv.shared);
            let ex2 = Arc::clone(&ex);
            std::thread::spawn(move || run_child(sh, ex2, child, pgid, backend));
            (
                Some(HelperResponse::Ok(Json::obj([(
                    "execution_id",
                    Json::str(execution_id),
                )]))),
                false,
            )
        }
        Err(e) => {
            let mut el = ex.lock().unwrap();
            let denied = e.to_string().contains("Operation not permitted")
                || e.to_string().contains("sandbox");
            el.sandbox_denied = denied;
            el.failure = Some((
                if denied {
                    "sandbox_denied".into()
                } else {
                    "spawn_failed".into()
                },
                e.to_string(),
            ));
            el.state = ExecState::Finished;
            drop(el);
            let mut t = srv.shared.table.lock().unwrap();
            t.map.insert(execution_id.clone(), Arc::clone(&ex));
            (
                Some(HelperResponse::err("runtime", format!("spawn failed: {e}"))),
                false,
            )
        }
    }
}

/// `terminal_payload(e, …)` — the `read` reply's terminal members (one
/// builder — the dedup-record and replay paths share it, CC1).
fn terminal_payload(e: &Execution, execution_id: &str, after_seq: u64, max_bytes: u64) -> Json {
    let frames = e.frames_after(after_seq, max_bytes);
    let mut payload: Vec<(&'static str, Json)> = vec![
        ("execution_id", Json::str(execution_id.to_string())),
        (
            "chunks",
            Json::Arr(frames.iter().map(|f| f.to_json()).collect()),
        ),
        ("exited", Json::Bool(e.state == ExecState::Finished)),
        ("exit_status", e.exit_status.map_or(Json::Null, Json::Int)),
        ("truncated", Json::Bool(e.dropped_bytes > 0)),
        (
            "detached",
            Json::Arr(
                e.detached
                    .iter()
                    .map(|d| {
                        Json::obj([
                            ("process_ref", Json::str(d.process_ref.clone())),
                            ("execution_id", Json::str(d.execution_id.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ];
    if let Some((class, detail)) = &e.failure {
        payload.push((
            "failure",
            Json::obj([
                ("class", Json::str(class.clone())),
                ("detail", Json::str(detail.clone())),
            ]),
        ));
    }
    if e.sandbox_denied {
        payload.push(("sandbox_denied", Json::Bool(true)));
    }
    Json::obj(payload)
}

/// `read_exec` — long-poll the journal (`wait_ms` bounded), replay frames
/// after `after_seq`, and report the terminal members.
fn read_exec(
    srv: &Arc<HelperServer>,
    execution_id: &str,
    after_seq: u64,
    max_bytes: u64,
    wait_ms: u64,
) -> (Option<HelperResponse>, bool) {
    let deadline = Instant::now() + Duration::from_millis(wait_ms.min(30_000));
    loop {
        #[allow(unused_mut)]
        let mut guard = srv.shared.table.lock().unwrap();
        let Some(ex) = guard.map.get(execution_id).cloned() else {
            drop(guard);
            return (
                Some(HelperResponse::err(
                    "invalid_request",
                    format!("unknown execution {execution_id}"),
                )),
                false,
            );
        };
        // Extract what the reply needs under the exec lock, then drop it
        // before the dedup store / condvar touch the table.
        let (reply, dedup_record) = {
            let e = ex.lock().unwrap();
            let last_seq = e
                .journal
                .last()
                .map(|j| match j {
                    crate::exec::JournalEntry::Chunk { seq, .. }
                    | crate::exec::JournalEntry::Process { seq, .. } => *seq,
                })
                .unwrap_or(0);
            let has_new = !e.journal.is_empty() && last_seq > after_seq;
            let terminal = e.state == ExecState::Finished;
            if has_new || terminal || wait_ms == 0 || Instant::now() >= deadline {
                let payload = terminal_payload(&e, execution_id, after_seq, max_bytes);
                #[cfg(feature = "tier-c1")]
                let rec = if terminal {
                    e.idempotency_key.clone().map(|key| {
                        (
                            key,
                            Json::obj([
                                ("exited", Json::Bool(true)),
                                ("exit_status", e.exit_status.map_or(Json::Null, Json::Int)),
                                (
                                    "failure",
                                    e.failure.as_ref().map_or(Json::Null, |(c, d)| {
                                        Json::obj([
                                            ("class", Json::str(c.clone())),
                                            ("detail", Json::str(d.clone())),
                                        ])
                                    }),
                                ),
                            ]),
                        )
                    })
                } else {
                    None
                };
                #[cfg(not(feature = "tier-c1"))]
                let rec: Option<(String, Json)> = None;
                (Some(payload), rec)
            } else {
                (None, None)
            }
        };
        // Executor-side dedup (R-2.5.5¹, tier-c1): the terminal verdict
        // binds the idempotency key — a replay inside the window returns it
        // without re-executing.
        #[cfg(not(feature = "tier-c1"))]
        let _ = dedup_record;
        #[cfg(feature = "tier-c1")]
        if let Some((key, verdict)) = dedup_record {
            guard.dedup.record(&key, verdict);
        }
        if let Some(p) = reply {
            drop(guard);
            return (Some(HelperResponse::Ok(p)), false);
        }
        // Long-poll — wait on the notify condvar.
        let (g, _t) = srv
            .shared
            .notify
            .wait_timeout(guard, Duration::from_millis(50))
            .unwrap();
        drop(g);
    }
}

/// `probe_boundary` — run one probe kind inside the backend's boundary.
/// The verdict is the *observed* result (`allow` when the action
/// succeeded — i.e., the boundary failed to deny; `deny` when the sandbox
/// refused; `unenforced` when the probe can't run here).
fn probe_boundary(srv: &Arc<HelperServer>, kind: &str) -> (Option<HelperResponse>, bool) {
    // Map the probe kind to a concrete probe command inside the sandbox.
    let roots = srv.roots.lock().unwrap().clone();
    let outside = roots
        .0
        .first()
        .map(|r| format!("{}/../hh-probe-outside", r.display()))
        .unwrap_or_else(|| "/tmp/hh-probe-outside".into());
    let cmd: Option<Vec<String>> = match kind {
        "write_outside_root" => Some(vec![
            "/bin/sh".into(),
            "-c".into(),
            format!("echo x > {outside} && echo wrote"),
        ]),
        "protected_metadata_write" => Some(vec![
            "/bin/sh".into(),
            "-c".into(),
            "echo x > /etc/hh-probe-deny 2>&1 || exit 3".into(),
        ]),
        "symlink_escape" => Some(vec![
            "/bin/sh".into(),
            "-c".into(),
            format!(
                "ln -sf /etc/passwd {}/hh-esc && cat {}/hh-esc",
                roots
                    .0
                    .first()
                    .map(|r| r.display().to_string())
                    .unwrap_or_default(),
                roots
                    .0
                    .first()
                    .map(|r| r.display().to_string())
                    .unwrap_or_default()
            ),
        ]),
        "network_connect" => Some(vec![
            "/bin/sh".into(),
            "-c".into(),
            "exec 3<>/dev/tcp/1.1.1.1/80 && echo connected".into(),
        ]),
        "icmp" | "raw_socket" => Some(vec![
            "/bin/sh".into(),
            "-c".into(),
            "ping -c1 127.0.0.1".into(),
        ]),
        "setuid" | "privilege_escalation" => Some(vec![
            "/bin/sh".into(),
            "-c".into(),
            "python3 -c 'import os;os.setuid(0)'".into(),
        ]),
        "ptrace" | "process_vm" => Some(vec![
            "/bin/sh".into(),
            "-c".into(),
            "python3 -c 'import ctypes;ctypes.CDLL(\"libSystem.B.dylib\")'".into(),
        ]),
        _ => None,
    };
    let Some(argv) = cmd else {
        return (
            Some(HelperResponse::err(
                "invalid_request",
                format!("unknown probe kind {kind}"),
            )),
            false,
        );
    };
    // Run the probe through the *same* backend the session enforces.
    let backend = *srv.backend.lock().unwrap();
    let mut c = match backend {
        "seatbelt" => {
            let profile = srv
                .policy
                .lock()
                .unwrap()
                .as_ref()
                .map(seatbelt::seatbelt_profile)
                .unwrap_or_else(|| "(version 1)\n(deny default)\n".into());
            let av = seatbelt::spawn_argv(&profile, &argv);
            let mut cc = Command::new(&av[0]);
            cc.args(&av[1..]);
            cc
        }
        "container" => match srv.container.lock().unwrap().clone() {
            Some(b) => b.exec_command(&argv, "/work", &[]),
            None => return (Some(HelperResponse::err("runtime", "no container")), false),
        },
        _ => {
            let mut cc = Command::new(&argv[0]);
            cc.args(&argv[1..]);
            cc
        }
    };
    match c.output() {
        Ok(o) => {
            // `allow` = the action succeeded (boundary failed to deny).
            // `deny` = refused (exit!=0 with a permission-shaped stderr, or
            // the known denial signatures).
            let err = String::from_utf8_lossy(&o.stderr).to_lowercase();
            let out = String::from_utf8_lossy(&o.stdout).to_lowercase();
            let denied = !o.status.success()
                && (err.contains("denied")
                    || err.contains("not permitted")
                    || err.contains("operation not permitted")
                    || err.contains("sandbox")
                    || out.contains("denied")
                    || o.status.code() == Some(3));
            let verdict = if denied {
                "deny"
            } else if o.status.success() {
                "allow"
            } else {
                "unenforced"
            };
            (
                Some(HelperResponse::Ok(Json::obj([(
                    "verdict",
                    Json::str(verdict),
                )]))),
                false,
            )
        }
        Err(e) => (Some(HelperResponse::err("runtime", e.to_string())), false),
    }
}
