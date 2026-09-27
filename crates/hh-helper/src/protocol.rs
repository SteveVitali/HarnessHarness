//! The `hh-helper/1` wire protocol — the closed, schema-versioned boundary
//! between the kernel and the out-of-process helper (§5d.5 §4 helper row;
//! ADR-0049/OQ-045; ADR-0050). This module is the **single schema source**
//! (CC7): the kernel side consumes it through `hh-env::protocol` (a
//! re-export), the helper side serves it in [`crate::server`].
//!
//! Transport: one canonical-JSON object per line over the session channel
//! (the bridged `AF_UNIX` socket — OQ-161's ruling). A request is
//! `{v:"hh-helper/1", op, …}`; a response is `{ok:{…}}` or
//! `{error:{class,detail}}`; a `read` chunk frame is
//! `{seq, stream, data, token}`. Anything outside the closed sums is a
//! `protocol_error` (transport origin) — malformed frames are refused, never
//! coerced.
//!
//! The verb set is the §5d.5 §4 minimum plus the environment surface the
//! helper owns: `hello` (session open/resume), `exec`, `read`, `probe`,
//! `cancel`, `terminate`, `detach`, `list_detached`, `env_apply`, `fs.*`
//! (read/write/canonicalize/metadata/read_dir/remove), `snapshot`, `diff`,
//! `probe_boundary`, `shutdown`.

use hh_wire::json::Json;

/// The protocol version tag (`v` member).
pub const HELPER_PROTOCOL: &str = "hh-helper/1";

/// The closed verb sum — an `op` outside it is `protocol_error`.
pub const OPS: &[&str] = &[
    "hello",
    "exec",
    "read",
    "probe",
    "cancel",
    "terminate",
    "detach",
    "list_detached",
    "env_apply",
    "fs.read",
    "fs.write",
    "fs.canonicalize",
    "fs.metadata",
    "fs.read_dir",
    "fs.remove",
    "snapshot",
    "diff",
    "probe_boundary",
    "shutdown",
];

/// `ProtoError` — a wire-shape failure (transport origin `protocol_error`).
#[derive(Debug, Clone, PartialEq)]
pub struct ProtoError {
    /// The closed reason tag.
    pub tag: &'static str,
    /// A bounded detail (content-free — never carries payload bytes).
    pub detail: String,
}

impl std::fmt::Display for ProtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "protocol_error:{}: {}", self.tag, self.detail)
    }
}

impl std::error::Error for ProtoError {}

fn bad(tag: &'static str, detail: impl Into<String>) -> ProtoError {
    ProtoError {
        tag,
        detail: detail.into(),
    }
}

/// `OnKernelLoss` — the session's behaviour when the kernel channel drops
/// (§5d.5 §4 `on_kernel_loss`; `preserve_until` is the C1/B3 member).
#[derive(Debug, Clone, PartialEq)]
pub enum OnKernelLoss {
    /// Terminate all running executions on connection loss.
    Terminate,
    /// Preserve running executions and journals until `ttl_ms` lapses
    /// (B3 resume — C1; refused `unsupported` when built without `tier-c1`).
    PreserveUntil {
        /// The preservation window.
        ttl_ms: u64,
    },
}

/// `CommitProof` — the write-ahead admission the `exec` request carries
/// (§5d.5 §4: `commit_token | read_only_marker`; ADR-0100 I-1). The helper
/// verifies the token by recompute over the session nonce — a token absent,
/// forged, bound to different members, or already consumed is `NotCommitted`.
#[derive(Debug, Clone, PartialEq)]
pub enum CommitProof {
    /// `commit_token{proof, effect_id, attempt_no, fencing_token,
    /// commit_event_id, commit_seq}` — the kernel's signed write-ahead.
    Token {
        /// The idp proof (`idp_id("commit_token", nonce ∥ members)`).
        proof: String,
        /// The effect.
        effect_id: String,
        /// The attempt.
        attempt_no: u64,
        /// The lease generation the commit landed under.
        fencing_token: u64,
        /// The durable `committed` event id (single-use).
        commit_event_id: String,
        /// The `committed` event's seq.
        commit_seq: u64,
    },
    /// `read_only` — the effect class has no write-ahead (the kernel asserts;
    /// the helper records it).
    ReadOnly,
}

/// `HelperRequest` — the kernel→helper verb set (the closed request sum).
/// The helper never sees a `Proposal`/`KernelDecision` — it receives the
/// resolved `ExecutionRequest`'s members and the attribution token it must
/// echo on every frame.
#[derive(Debug, Clone, PartialEq)]
pub enum HelperRequest {
    /// `hello` — open or resume the session. `resume_session_id` reattaches a
    /// live session (kernel death ≠ environment death — the journals and
    /// running executions survive).
    Hello {
        /// The client identity claim (`hh-kernel`).
        client: String,
        /// Resume id, when reattaching.
        resume_session_id: Option<String>,
        /// The kernel-loss posture.
        on_kernel_loss: OnKernelLoss,
        /// The session nonce (the `commit_token` recompute key — delivered
        /// over this channel only, never ledgered).
        session_nonce: String,
        /// The executor-side dedup window (ms; C1 — a session declaring it
        /// under a `tier-c1`-less build is refused `unsupported`).
        dedup_window_ms: Option<u64>,
        /// The containment policy the helper enforces (its fs gate + the
        /// spawn-time sandbox profile; serialized `ContainmentPolicy/1`).
        policy: Option<Json>,
        /// The fs roots the helper's fs.* gate closes over
        /// (`{workspace_roots, writable_roots}` canonical paths).
        roots: Option<(Vec<String>, Vec<String>)>,
    },
    /// `exec` — launch the capability's workload (async: returns
    /// `{execution_id}`; `read` streams the journal).
    Exec {
        /// The kernel-chosen execution id.
        execution_id: String,
        /// The effect.
        effect_id: String,
        /// The attempt.
        attempt_no: u64,
        /// The capability (pinned `{semantic_id, version_id}`).
        capability_ref: (String, String),
        /// The canonical args (`argv` or `command`).
        args: Json,
        /// The working directory (inside a readable root).
        cwd: String,
        /// The projected env (placeholders only — `env_apply`'s output).
        env: Vec<(String, String)>,
        /// The effective deadline (ms budget the helper enforces locally).
        deadline_ms: Option<u64>,
        /// The retained-bytes cap on the journal.
        retain_bytes_cap: u64,
        /// The attribution token to echo on every frame.
        attribution_token: String,
        /// The write-ahead admission.
        commit_proof: CommitProof,
        /// The idempotency key (the executor-side dedup store's operand — C1).
        idempotency_key: Option<String>,
    },
    /// `read{execution_id, after_seq, max_bytes, wait_ms}` — long-poll the
    /// journal: buffered chunks after `after_seq`, `exited`, `exit_status`,
    /// `failure{class,detail}`, `sandbox_denied`, `detached[]`, truncation.
    Read {
        /// The execution.
        execution_id: String,
        /// Resume offset (return entries with `seq > after_seq`).
        after_seq: u64,
        /// Max bytes of chunk payload per reply (0 = unbounded).
        max_bytes: u64,
        /// Long-poll budget (ms; 0 = return immediately).
        wait_ms: u64,
    },
    /// `probe{effect_id, attempt_no}` — did a lapsed-window effect apply?
    /// Resolved to the execution journal (`probe = read(execution_id)`).
    Probe {
        /// The effect.
        effect_id: String,
        /// The attempt.
        attempt_no: u64,
    },
    /// `cancel{execution_id, phase}` — two-phase interruption (`term` waits
    /// `grace_ms`, `kill` forces).
    Cancel {
        /// The execution.
        execution_id: String,
        /// `term | kill`.
        phase: String,
    },
    /// `terminate{execution_id}` — hard-stop the execution's process group;
    /// returns `{running}` (whether it was live when terminated).
    Terminate {
        /// The execution.
        execution_id: String,
    },
    /// `detach{execution_id}` — mark the execution detached: its surviving
    /// process group is tracked (not reaped with the direct child) and
    /// `list_detached` reports it.
    Detach {
        /// The execution.
        execution_id: String,
    },
    /// `list_detached` — every surviving detached process group.
    ListDetached,
    /// `env_apply{vars}` — re-deliver the projected environment
    /// (placeholders only; the helper holds no secrets — SV-4).
    EnvApply {
        /// The projected pairs.
        vars: Vec<(String, String)>,
    },
    /// `fs.read{path}` — read a file through the helper's gate.
    FsRead {
        /// The path.
        path: String,
    },
    /// `fs.write{path, content}` — write through the helper's gate
    /// (`containment_denied` outside writable roots / on protected paths).
    FsWrite {
        /// The path.
        path: String,
        /// The content (UTF-8; the wire carries strings — binary goes through
        /// the blob path, never this verb).
        content: String,
    },
    /// `fs.canonicalize{path}` — resolve `.`/`..`/links (F1 — resolution
    /// precedes validation).
    FsCanonicalize {
        /// The path.
        path: String,
    },
    /// `fs.metadata{path}` — `{kind, size, exec}`.
    FsMetadata {
        /// The path.
        path: String,
    },
    /// `fs.read_dir{path}` — the sorted entry list.
    FsReadDir {
        /// The path.
        path: String,
    },
    /// `fs.remove{path}` — remove through the gate.
    FsRemove {
        /// The path.
        path: String,
    },
    /// `snapshot{roots, kind}` — compute the `fs_tree` over the roots;
    /// returns `{tree_address, manifest, size_bytes}` (the helper's view —
    /// the kernel independently recomputes for the stored `SnapshotRecord`,
    /// so a lying helper is caught at `verify`).
    Snapshot {
        /// The covered roots.
        roots: Vec<String>,
        /// `fs_tree` (the only kind the helper computes at Stage 2 —
        /// `path_baseline` is the kernel's own walk).
        kind: String,
    },
    /// `diff{roots, base_manifest}` — the fs change-set between the supplied
    /// base manifest and the roots' current state.
    Diff {
        /// The covered roots.
        roots: Vec<String>,
        /// The base manifest (a prior `snapshot` reply's `manifest`).
        base_manifest: Json,
    },
    /// `probe_boundary{kind}` — run the concrete containment probe `kind`
    /// (the `ProbeKind` spelling) inside the backend's boundary; returns
    /// `{verdict: allow|deny|unenforced}` (the live `probed` evidence — never
    /// `attested` self-report).
    ProbeBoundary {
        /// The `ProbeKind` spelling.
        kind: String,
    },
    /// `shutdown` — the driver's teardown path: terminate running
    /// executions, drop the backend (the container is removed), exit.
    Shutdown,
}

/// `HelperResponse` — a non-streaming reply.
#[derive(Debug, Clone, PartialEq)]
pub enum HelperResponse {
    /// `ok{…}` — the op succeeded (payload is op-specific).
    Ok(Json),
    /// `error{class, detail}` — the op failed (the closed `ErrorClass` tag).
    Error {
        /// The error class tag.
        class: String,
        /// A detail (bounded, content-free).
        detail: String,
    },
}

/// `HelperFrame` — one journaled exec item (the `chunks[]` member of a `read`
/// reply). Every frame echoes the request's `attribution_token` — the kernel
/// resolves each echo; an unresolvable echo is `unattributed` (dropped).
#[derive(Debug, Clone, PartialEq)]
pub enum HelperFrame {
    /// `chunk{seq, stream, data, token}` — an output chunk.
    Chunk {
        /// The journal seq.
        seq: u64,
        /// `stdout|stderr`.
        stream: String,
        /// The chunk.
        data: String,
        /// The echoed attribution token.
        token: String,
    },
    /// `process{seq, transition, process_ref, token}` — a process transition
    /// (`spawned`/`detached`/`signalled`/`exited`).
    Process {
        /// The journal seq.
        seq: u64,
        /// The transition spelling.
        transition: String,
        /// The process reference (`pgrp:<pgid>`).
        process_ref: String,
        /// The echoed token.
        token: String,
    },
    /// `progress{seq, note, token}` — ephemeral.
    Progress {
        /// The journal seq.
        seq: u64,
        /// The note.
        note: String,
        /// The echoed token.
        token: String,
    },
    /// `terminal` — the terminal report folded from the journal's
    /// `exited`/`exit_status`/`failure` members (kernel-side only — the wire
    /// carries them on the `read` reply, not as a frame).
    Terminal {
        /// `ok | error{class}`.
        status: Json,
        /// The exit status.
        exit_status: Option<i64>,
        /// The outcome hint.
        outcome_hint: String,
        /// The retryable hint (may only lower).
        retryable_hint: Option<bool>,
    },
}

// ── serialization ────────────────────────────────────────────────────────────

fn jstr(j: &Json, k: &str) -> Result<String, ProtoError> {
    j.get(k)
        .and_then(Json::as_str)
        .map(String::from)
        .ok_or_else(|| bad("missing_member", k))
}

fn jint(j: &Json, k: &str) -> Result<u64, ProtoError> {
    j.get(k)
        .and_then(Json::as_int)
        .filter(|i| *i >= 0)
        .map(|i| i as u64)
        .ok_or_else(|| bad("missing_member", k))
}

fn jstr_opt(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(Json::as_str).map(String::from)
}

fn jint_opt(j: &Json, k: &str) -> Option<u64> {
    j.get(k)
        .and_then(Json::as_int)
        .filter(|i| *i >= 0)
        .map(|i| i as u64)
}

fn pairs(j: Option<&Json>) -> Result<Vec<(String, String)>, ProtoError> {
    match j {
        None | Some(Json::Null) => Ok(vec![]),
        Some(Json::Arr(a)) => a
            .iter()
            .map(|e| {
                Ok((
                    jstr(e, "name")?,
                    e.get("value")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_string(),
                ))
            })
            .collect(),
        _ => Err(bad("bad_member", "env/vars")),
    }
}

fn str_list(j: Option<&Json>, member: &str) -> Result<Vec<String>, ProtoError> {
    match j {
        None | Some(Json::Null) => Ok(vec![]),
        Some(Json::Arr(a)) => {
            let mut out = Vec::with_capacity(a.len());
            for v in a {
                out.push(
                    v.as_str()
                        .map(String::from)
                        .ok_or_else(|| bad("bad_member", format!("{member} non-string")))?,
                );
            }
            Ok(out)
        }
        _ => Err(bad("bad_member", member)),
    }
}

fn commit_proof_json(p: &CommitProof) -> Json {
    match p {
        CommitProof::Token {
            proof,
            effect_id,
            attempt_no,
            fencing_token,
            commit_event_id,
            commit_seq,
        } => Json::obj([
            ("kind", Json::str("token")),
            ("proof", Json::str(proof.clone())),
            ("effect_id", Json::str(effect_id.clone())),
            ("attempt_no", Json::Int(*attempt_no as i64)),
            ("fencing_token", Json::Int(*fencing_token as i64)),
            ("commit_event_id", Json::str(commit_event_id.clone())),
            ("commit_seq", Json::Int(*commit_seq as i64)),
        ]),
        CommitProof::ReadOnly => Json::obj([("kind", Json::str("read_only"))]),
    }
}

fn commit_proof_parse(j: Option<&Json>) -> Result<CommitProof, ProtoError> {
    let j = j.ok_or_else(|| bad("missing_member", "commit_proof"))?;
    match jstr(j, "kind")?.as_str() {
        "token" => Ok(CommitProof::Token {
            proof: jstr(j, "proof")?,
            effect_id: jstr(j, "effect_id")?,
            attempt_no: jint(j, "attempt_no")?,
            fencing_token: jint(j, "fencing_token")?,
            commit_event_id: jstr(j, "commit_event_id")?,
            commit_seq: jint(j, "commit_seq")?,
        }),
        "read_only" => Ok(CommitProof::ReadOnly),
        other => Err(bad("bad_member", format!("commit_proof.kind={other}"))),
    }
}

impl HelperRequest {
    /// The wire form.
    pub fn to_json(&self) -> Json {
        let v = ("v", Json::str(HELPER_PROTOCOL));
        match self {
            HelperRequest::Hello {
                client,
                resume_session_id,
                on_kernel_loss,
                session_nonce,
                dedup_window_ms,
                policy,
                roots,
            } => Json::obj([
                v,
                ("op", Json::str("hello")),
                ("client", Json::str(client.clone())),
                (
                    "resume_session_id",
                    resume_session_id
                        .as_ref()
                        .map_or(Json::Null, |s| Json::str(s.clone())),
                ),
                (
                    "on_kernel_loss",
                    match on_kernel_loss {
                        OnKernelLoss::Terminate => Json::obj([("kind", Json::str("terminate"))]),
                        OnKernelLoss::PreserveUntil { ttl_ms } => Json::obj([
                            ("kind", Json::str("preserve_until")),
                            ("ttl_ms", Json::Int(*ttl_ms as i64)),
                        ]),
                    },
                ),
                ("session_nonce", Json::str(session_nonce.clone())),
                (
                    "dedup_window_ms",
                    dedup_window_ms.map_or(Json::Null, |d| Json::Int(d as i64)),
                ),
                ("policy", policy.clone().unwrap_or(Json::Null)),
                (
                    "roots",
                    roots.as_ref().map_or(Json::Null, |(ws, wr)| {
                        Json::obj([
                            (
                                "workspace_roots",
                                Json::Arr(ws.iter().map(|r| Json::str(r.clone())).collect()),
                            ),
                            (
                                "writable_roots",
                                Json::Arr(wr.iter().map(|r| Json::str(r.clone())).collect()),
                            ),
                        ])
                    }),
                ),
            ]),
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
            } => Json::obj([
                v,
                ("op", Json::str("exec")),
                ("execution_id", Json::str(execution_id.clone())),
                ("effect_id", Json::str(effect_id.clone())),
                ("attempt_no", Json::Int(*attempt_no as i64)),
                (
                    "capability_ref",
                    Json::obj([
                        ("semantic_id", Json::str(capability_ref.0.clone())),
                        ("version_id", Json::str(capability_ref.1.clone())),
                    ]),
                ),
                ("args", args.clone()),
                ("cwd", Json::str(cwd.clone())),
                (
                    "env",
                    Json::Arr(
                        env.iter()
                            .map(|(k, val)| {
                                Json::obj([
                                    ("name", Json::str(k.clone())),
                                    ("value", Json::str(val.clone())),
                                ])
                            })
                            .collect(),
                    ),
                ),
                (
                    "deadline_ms",
                    deadline_ms.map_or(Json::Null, |d| Json::Int(d as i64)),
                ),
                ("retain_bytes_cap", Json::Int(*retain_bytes_cap as i64)),
                ("attribution_token", Json::str(attribution_token.clone())),
                ("commit_proof", commit_proof_json(commit_proof)),
                (
                    "idempotency_key",
                    idempotency_key
                        .as_ref()
                        .map_or(Json::Null, |k| Json::str(k.clone())),
                ),
            ]),
            HelperRequest::Read {
                execution_id,
                after_seq,
                max_bytes,
                wait_ms,
            } => Json::obj([
                v,
                ("op", Json::str("read")),
                ("execution_id", Json::str(execution_id.clone())),
                ("after_seq", Json::Int(*after_seq as i64)),
                ("max_bytes", Json::Int(*max_bytes as i64)),
                ("wait_ms", Json::Int(*wait_ms as i64)),
            ]),
            HelperRequest::Probe {
                effect_id,
                attempt_no,
            } => Json::obj([
                v,
                ("op", Json::str("probe")),
                ("effect_id", Json::str(effect_id.clone())),
                ("attempt_no", Json::Int(*attempt_no as i64)),
            ]),
            HelperRequest::Cancel {
                execution_id,
                phase,
            } => Json::obj([
                v,
                ("op", Json::str("cancel")),
                ("execution_id", Json::str(execution_id.clone())),
                ("phase", Json::str(phase.clone())),
            ]),
            HelperRequest::Terminate { execution_id } => Json::obj([
                v,
                ("op", Json::str("terminate")),
                ("execution_id", Json::str(execution_id.clone())),
            ]),
            HelperRequest::Detach { execution_id } => Json::obj([
                v,
                ("op", Json::str("detach")),
                ("execution_id", Json::str(execution_id.clone())),
            ]),
            HelperRequest::ListDetached => Json::obj([v, ("op", Json::str("list_detached"))]),
            HelperRequest::EnvApply { vars } => Json::obj([
                v,
                ("op", Json::str("env_apply")),
                (
                    "vars",
                    Json::Arr(
                        vars.iter()
                            .map(|(k, val)| {
                                Json::obj([
                                    ("name", Json::str(k.clone())),
                                    ("value", Json::str(val.clone())),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]),
            HelperRequest::FsRead { path } => Json::obj([
                v,
                ("op", Json::str("fs.read")),
                ("path", Json::str(path.clone())),
            ]),
            HelperRequest::FsWrite { path, content } => Json::obj([
                v,
                ("op", Json::str("fs.write")),
                ("path", Json::str(path.clone())),
                ("content", Json::str(content.clone())),
            ]),
            HelperRequest::FsCanonicalize { path } => Json::obj([
                v,
                ("op", Json::str("fs.canonicalize")),
                ("path", Json::str(path.clone())),
            ]),
            HelperRequest::FsMetadata { path } => Json::obj([
                v,
                ("op", Json::str("fs.metadata")),
                ("path", Json::str(path.clone())),
            ]),
            HelperRequest::FsReadDir { path } => Json::obj([
                v,
                ("op", Json::str("fs.read_dir")),
                ("path", Json::str(path.clone())),
            ]),
            HelperRequest::FsRemove { path } => Json::obj([
                v,
                ("op", Json::str("fs.remove")),
                ("path", Json::str(path.clone())),
            ]),
            HelperRequest::Snapshot { roots, kind } => Json::obj([
                v,
                ("op", Json::str("snapshot")),
                (
                    "roots",
                    Json::Arr(roots.iter().map(|r| Json::str(r.clone())).collect()),
                ),
                ("kind", Json::str(kind.clone())),
            ]),
            HelperRequest::Diff {
                roots,
                base_manifest,
            } => Json::obj([
                v,
                ("op", Json::str("diff")),
                (
                    "roots",
                    Json::Arr(roots.iter().map(|r| Json::str(r.clone())).collect()),
                ),
                ("base_manifest", base_manifest.clone()),
            ]),
            HelperRequest::ProbeBoundary { kind } => Json::obj([
                v,
                ("op", Json::str("probe_boundary")),
                ("kind", Json::str(kind.clone())),
            ]),
            HelperRequest::Shutdown => Json::obj([v, ("op", Json::str("shutdown"))]),
        }
    }

    /// The strict decode — unknown `v`/`op`/members are `protocol_error`
    /// (never coerced).
    pub fn from_json(j: &Json) -> Result<HelperRequest, ProtoError> {
        let ver = jstr(j, "v")?;
        if ver != HELPER_PROTOCOL {
            return Err(bad("version_mismatch", ver));
        }
        let op = jstr(j, "op")?;
        if !OPS.contains(&op.as_str()) {
            return Err(bad("unknown_verb", op));
        }
        match op.as_str() {
            "hello" => {
                let on_loss = match j.get("on_kernel_loss") {
                    Some(o) => match jstr(o, "kind")?.as_str() {
                        "terminate" => OnKernelLoss::Terminate,
                        "preserve_until" => OnKernelLoss::PreserveUntil {
                            ttl_ms: jint(o, "ttl_ms")?,
                        },
                        other => {
                            return Err(bad("bad_member", format!("on_kernel_loss.kind={other}")))
                        }
                    },
                    None => OnKernelLoss::Terminate,
                };
                let roots = match j.get("roots") {
                    Some(Json::Obj(_)) => {
                        let r = j.get("roots").unwrap();
                        Some((
                            str_list(r.get("workspace_roots"), "workspace_roots")?,
                            str_list(r.get("writable_roots"), "writable_roots")?,
                        ))
                    }
                    _ => None,
                };
                Ok(HelperRequest::Hello {
                    client: jstr(j, "client")?,
                    resume_session_id: jstr_opt(j, "resume_session_id"),
                    on_kernel_loss: on_loss,
                    session_nonce: jstr(j, "session_nonce")?,
                    dedup_window_ms: jint_opt(j, "dedup_window_ms"),
                    policy: j
                        .get("policy")
                        .filter(|p| !matches!(p, Json::Null))
                        .cloned(),
                    roots,
                })
            }
            "exec" => {
                let cap = j
                    .get("capability_ref")
                    .ok_or_else(|| bad("missing_member", "capability_ref"))?;
                Ok(HelperRequest::Exec {
                    execution_id: jstr(j, "execution_id")?,
                    effect_id: jstr(j, "effect_id")?,
                    attempt_no: jint(j, "attempt_no")?,
                    capability_ref: (jstr(cap, "semantic_id")?, jstr(cap, "version_id")?),
                    args: j.get("args").cloned().unwrap_or(Json::Null),
                    cwd: jstr(j, "cwd")?,
                    env: pairs(j.get("env"))?,
                    deadline_ms: jint_opt(j, "deadline_ms"),
                    retain_bytes_cap: jint(j, "retain_bytes_cap")?,
                    attribution_token: jstr(j, "attribution_token")?,
                    commit_proof: commit_proof_parse(j.get("commit_proof"))?,
                    idempotency_key: jstr_opt(j, "idempotency_key"),
                })
            }
            "read" => Ok(HelperRequest::Read {
                execution_id: jstr(j, "execution_id")?,
                after_seq: jint(j, "after_seq").unwrap_or(0),
                max_bytes: jint_opt(j, "max_bytes").unwrap_or(0),
                wait_ms: jint_opt(j, "wait_ms").unwrap_or(0),
            }),
            "probe" => Ok(HelperRequest::Probe {
                effect_id: jstr(j, "effect_id")?,
                attempt_no: jint(j, "attempt_no")?,
            }),
            "cancel" => Ok(HelperRequest::Cancel {
                execution_id: jstr(j, "execution_id")?,
                phase: jstr(j, "phase")?,
            }),
            "terminate" => Ok(HelperRequest::Terminate {
                execution_id: jstr(j, "execution_id")?,
            }),
            "detach" => Ok(HelperRequest::Detach {
                execution_id: jstr(j, "execution_id")?,
            }),
            "list_detached" => Ok(HelperRequest::ListDetached),
            "env_apply" => Ok(HelperRequest::EnvApply {
                vars: pairs(j.get("vars"))?,
            }),
            "fs.read" => Ok(HelperRequest::FsRead {
                path: jstr(j, "path")?,
            }),
            "fs.write" => Ok(HelperRequest::FsWrite {
                path: jstr(j, "path")?,
                content: jstr(j, "content")?,
            }),
            "fs.canonicalize" => Ok(HelperRequest::FsCanonicalize {
                path: jstr(j, "path")?,
            }),
            "fs.metadata" => Ok(HelperRequest::FsMetadata {
                path: jstr(j, "path")?,
            }),
            "fs.read_dir" => Ok(HelperRequest::FsReadDir {
                path: jstr(j, "path")?,
            }),
            "fs.remove" => Ok(HelperRequest::FsRemove {
                path: jstr(j, "path")?,
            }),
            "snapshot" => Ok(HelperRequest::Snapshot {
                roots: str_list(j.get("roots"), "roots")?,
                kind: jstr(j, "kind")?,
            }),
            "diff" => Ok(HelperRequest::Diff {
                roots: str_list(j.get("roots"), "roots")?,
                base_manifest: j
                    .get("base_manifest")
                    .cloned()
                    .ok_or_else(|| bad("missing_member", "base_manifest"))?,
            }),
            "probe_boundary" => Ok(HelperRequest::ProbeBoundary {
                kind: jstr(j, "kind")?,
            }),
            "shutdown" => Ok(HelperRequest::Shutdown),
            _ => Err(bad("unknown_verb", op)),
        }
    }
}

impl HelperResponse {
    /// The wire form (`{ok:{…}}` | `{error:{class,detail}}`).
    pub fn to_json(&self) -> Json {
        match self {
            HelperResponse::Ok(payload) => Json::obj([("ok", payload.clone())]),
            HelperResponse::Error { class, detail } => Json::obj([(
                "error",
                Json::obj([
                    ("class", Json::str(class.clone())),
                    ("detail", Json::str(detail.clone())),
                ]),
            )]),
        }
    }

    /// The strict decode.
    pub fn from_json(j: &Json) -> Result<HelperResponse, ProtoError> {
        if let Some(ok) = j.get("ok") {
            return Ok(HelperResponse::Ok(ok.clone()));
        }
        if let Some(e) = j.get("error") {
            return Ok(HelperResponse::Error {
                class: jstr(e, "class")?,
                detail: jstr(e, "detail").unwrap_or_default(),
            });
        }
        Err(bad("bad_response", "neither ok nor error member"))
    }

    /// `error{class, detail}` shorthand.
    pub fn err(class: &str, detail: impl Into<String>) -> HelperResponse {
        HelperResponse::Error {
            class: class.to_string(),
            detail: detail.into(),
        }
    }
}

impl HelperFrame {
    /// The wire form (a `chunks[]` member).
    pub fn to_json(&self) -> Json {
        match self {
            HelperFrame::Chunk {
                seq,
                stream,
                data,
                token,
            } => Json::obj([
                ("f", Json::str("chunk")),
                ("seq", Json::Int(*seq as i64)),
                ("stream", Json::str(stream.clone())),
                ("data", Json::str(data.clone())),
                ("token", Json::str(token.clone())),
            ]),
            HelperFrame::Process {
                seq,
                transition,
                process_ref,
                token,
            } => Json::obj([
                ("f", Json::str("process")),
                ("seq", Json::Int(*seq as i64)),
                ("transition", Json::str(transition.clone())),
                ("process_ref", Json::str(process_ref.clone())),
                ("token", Json::str(token.clone())),
            ]),
            HelperFrame::Progress { seq, note, token } => Json::obj([
                ("f", Json::str("progress")),
                ("seq", Json::Int(*seq as i64)),
                ("note", Json::str(note.clone())),
                ("token", Json::str(token.clone())),
            ]),
            HelperFrame::Terminal { .. } => Json::obj([("f", Json::str("terminal"))]),
        }
    }

    /// The strict decode of one `chunks[]` entry.
    pub fn from_json(j: &Json) -> Result<HelperFrame, ProtoError> {
        let seq = jint(j, "seq").unwrap_or(0);
        let token = jstr(j, "token").unwrap_or_default();
        match jstr(j, "f")?.as_str() {
            "chunk" => Ok(HelperFrame::Chunk {
                seq,
                stream: jstr(j, "stream")?,
                data: jstr(j, "data")?,
                token,
            }),
            "process" => Ok(HelperFrame::Process {
                seq,
                transition: jstr(j, "transition")?,
                process_ref: jstr(j, "process_ref")?,
                token,
            }),
            "progress" => Ok(HelperFrame::Progress {
                seq,
                note: jstr(j, "note")?,
                token,
            }),
            other => Err(bad("bad_frame", other.to_string())),
        }
    }
}

/// `mint_commit_proof(session_nonce, token)` — the kernel-side proof over the
/// serialised `CommitToken` members (the helper recomputes it — it knows the
/// nonce from `hello`; a forged/mis-bound token fails the recompute).
pub fn mint_commit_proof(
    session_nonce: &str,
    effect_id: &str,
    attempt_no: u64,
    fencing_token: u64,
    commit_event_id: &str,
    commit_seq: u64,
) -> String {
    let body = Json::obj([
        ("commit_event_id", Json::str(commit_event_id)),
        ("commit_seq", Json::Int(commit_seq as i64)),
        ("effect_id", Json::str(effect_id)),
        ("attempt_no", Json::Int(attempt_no as i64)),
        ("fencing_token", Json::Int(fencing_token as i64)),
        ("nonce", Json::str(session_nonce)),
    ]);
    hh_identity::idp::idp_id("commit_token", body.to_canonical_string().as_bytes())
}

/// The helper-side check: the carried members recompute under the session
/// nonce and bind this `(effect_id, attempt_no)` exactly.
pub fn verify_commit_proof(
    session_nonce: &str,
    p: &CommitProof,
    exec_effect: &str,
    exec_attempt: u64,
) -> bool {
    match p {
        CommitProof::ReadOnly => true,
        CommitProof::Token {
            proof,
            effect_id,
            attempt_no,
            fencing_token,
            commit_event_id,
            commit_seq,
        } => {
            effect_id == exec_effect
                && *attempt_no == exec_attempt
                && mint_commit_proof(
                    session_nonce,
                    effect_id,
                    *attempt_no,
                    *fencing_token,
                    commit_event_id,
                    *commit_seq,
                ) == *proof
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_roundtrip() {
        let r = HelperRequest::Exec {
            execution_id: "exec-1".into(),
            effect_id: "eff-1".into(),
            attempt_no: 1,
            capability_ref: ("cap".into(), "v1".into()),
            args: Json::obj([("command", Json::str("echo hi"))]),
            cwd: "/ws".into(),
            env: vec![("A".into(), "1".into())],
            deadline_ms: Some(50),
            retain_bytes_cap: 1024,
            attribution_token: "tok".into(),
            commit_proof: CommitProof::Token {
                proof: "sha256:x".into(),
                effect_id: "eff-1".into(),
                attempt_no: 1,
                fencing_token: 2,
                commit_event_id: "evt-9".into(),
                commit_seq: 7,
            },
            idempotency_key: Some("k".into()),
        };
        let j = r.to_json();
        assert_eq!(HelperRequest::from_json(&j), Ok(r));
    }

    #[test]
    fn unknown_verb_and_version_refused() {
        let j = Json::obj([("v", Json::str("hh-helper/9")), ("op", Json::str("exec"))]);
        assert_eq!(
            HelperRequest::from_json(&j).unwrap_err().tag,
            "version_mismatch"
        );
        let j = Json::obj([
            ("v", Json::str(HELPER_PROTOCOL)),
            ("op", Json::str("fork_bomb")),
        ]);
        assert_eq!(
            HelperRequest::from_json(&j).unwrap_err().tag,
            "unknown_verb"
        );
    }

    #[test]
    fn commit_proof_binds_members() {
        let proof = mint_commit_proof("n", "eff-1", 1, 2, "evt-9", 7);
        let p = CommitProof::Token {
            proof,
            effect_id: "eff-1".into(),
            attempt_no: 1,
            fencing_token: 2,
            commit_event_id: "evt-9".into(),
            commit_seq: 7,
        };
        assert!(verify_commit_proof("n", &p, "eff-1", 1));
        // A mis-bound token fails.
        assert!(!verify_commit_proof("n", &p, "eff-2", 1));
        // A forged proof fails.
        let forged = CommitProof::Token {
            proof: "sha256:forged".into(),
            effect_id: "eff-1".into(),
            attempt_no: 1,
            fencing_token: 2,
            commit_event_id: "evt-9".into(),
            commit_seq: 7,
        };
        assert!(!verify_commit_proof("n", &forged, "eff-1", 1));
    }
}
