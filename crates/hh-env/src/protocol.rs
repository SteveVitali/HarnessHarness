//! The helper protocol — the narrow, schema-versioned boundary between the
//! kernel and the out-of-process sandbox helper (§5d.5 §4 helper row;
//! ADR-0049/OQ-045). At Stage 1 the helper is the *in-process* `LocalExecutor`
//! behind the same schema — the out-of-process binary lands at S2.1
//! (DF-S1.12-3's other half), but the protocol shape is defined now so the
//! seam is honest.
//!
//! The protocol is **versioned** (`helper.protocol = "hh-helper/1"`) and
//! **closed**: a request is one of the declared kinds; a response is
//! `{ok | error{class}}`; a stream frame is `chunk|progress|terminal`.
//! Anything outside the sum is a `protocol_error` (transport origin).

use hh_wire::json::Json;

/// The protocol version tag (`helper.protocol` member).
pub const HELPER_PROTOCOL: &str = "hh-helper/1";

/// `HelperRequest` — the kernel→helper verbs (the closed request sum). The
/// helper never sees a `Proposal`/`KernelDecision` — it receives the resolved
/// `ExecutionRequest`'s members and the attribution token it must echo.
#[derive(Debug, Clone, PartialEq)]
pub enum HelperRequest {
    /// `exec` — run the capability (the `execute` stage's request).
    Exec {
        /// The execution id.
        execution_id: String,
        /// The effect.
        effect_id: String,
        /// The attempt.
        attempt_no: u64,
        /// The capability (pinned `{semantic_id, version_id}`).
        capability_ref: (String, String),
        /// The canonical args.
        args: Json,
        /// The working directory (inside a readable root).
        cwd: String,
        /// The projected env (placeholders only — `env_apply`'s output).
        env: Vec<(String, String)>,
        /// The effective deadline (mono ms).
        deadline_ms: Option<u64>,
        /// The retained-bytes cap.
        retain_bytes_cap: u64,
        /// The attribution token to echo.
        attribution_token: String,
    },
    /// `probe` — did a lapsed-window effect apply?
    Probe {
        /// The effect.
        effect_id: String,
        /// The attempt.
        attempt_no: u64,
    },
    /// `cancel` — interrupt the in-flight execution (two-phase).
    Cancel {
        /// The execution id.
        execution_id: String,
        /// The interrupt phase (`term` then `kill`).
        phase: String,
    },
    /// `env_apply` — re-project the environment (placeholder-only; the helper
    /// holds no secrets — the kernel recomputes and re-delivers).
    EnvApply {
        /// The session.
        session_id: String,
    },
}

impl HelperRequest {
    /// The wire form (`{v, op, …}`).
    pub fn to_json(&self) -> Json {
        match self {
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
            } => Json::obj([
                ("v", Json::str(HELPER_PROTOCOL)),
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
                            .map(|(k, v)| {
                                Json::obj([
                                    ("name", Json::str(k.clone())),
                                    ("value", Json::str(v.clone())),
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
            ]),
            HelperRequest::Probe {
                effect_id,
                attempt_no,
            } => Json::obj([
                ("v", Json::str(HELPER_PROTOCOL)),
                ("op", Json::str("probe")),
                ("effect_id", Json::str(effect_id.clone())),
                ("attempt_no", Json::Int(*attempt_no as i64)),
            ]),
            HelperRequest::Cancel {
                execution_id,
                phase,
            } => Json::obj([
                ("v", Json::str(HELPER_PROTOCOL)),
                ("op", Json::str("cancel")),
                ("execution_id", Json::str(execution_id.clone())),
                ("phase", Json::str(phase.clone())),
            ]),
            HelperRequest::EnvApply { session_id } => Json::obj([
                ("v", Json::str(HELPER_PROTOCOL)),
                ("op", Json::str("env_apply")),
                ("session_id", Json::str(session_id.clone())),
            ]),
        }
    }
}

/// `HelperFrame` — a helper→kernel stream frame (the ephemeral + terminal
/// members; a non-terminal frame is a capture item, the terminal is the
/// report).
#[derive(Debug, Clone, PartialEq)]
pub enum HelperFrame {
    /// `chunk{stream, data}` — an output chunk (ephemeral).
    Chunk {
        /// `stdout|stderr`.
        stream: String,
        /// The chunk.
        data: String,
    },
    /// `progress{note}` — ephemeral.
    Progress {
        /// The note.
        note: String,
    },
    /// `terminal{status, exit_status?, outcome_hint, retryable_hint?}` — the
    /// terminal report.
    Terminal {
        /// `ok|error{class}`.
        status: Json,
        /// The exit status.
        exit_status: Option<i64>,
        /// The outcome hint.
        outcome_hint: String,
        /// The retryable hint (may only lower).
        retryable_hint: Option<bool>,
    },
}

/// `HelperResponse` — a non-streaming reply (`probe`, `cancel`, `env_apply`).
#[derive(Debug, Clone, PartialEq)]
pub enum HelperResponse {
    /// `ok{…}` — the op succeeded (payload is op-specific).
    Ok(Json),
    /// `error{class, detail}` — the op failed (the closed `ErrorClass` tag).
    Error {
        /// The error class tag.
        class: String,
        /// A detail (masked).
        detail: String,
    },
}
