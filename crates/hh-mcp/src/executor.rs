//! `McpExecutor` — the MCP-backed `ToolExecutor` (§5d.4 C1/D3; the
//! DF-S3.9-1 unblocking arm, S4.5b). The edge calls `tools/call` through
//! [`McpClient`]; the kernel decides the lifecycle from the executor's
//! *report* — the executor never decides (I-3).
//!
//! Outcome mapping (ADR-0097 D3 → the dispatch tail):
//!
//! - `observed`/`complete` → `TerminalStatus::Ok` — the result body is
//!   emitted as an `output_chunk` capture signal (masked on the capture
//!   path like every executor byte).
//! - `isError: true` → `TerminalStatus::ToolError{class: executor_error}`
//!   — the tool's own failure report at `tool` origin; the kernel's
//!   lifecycle rule settles `not_applied|partial|unknown` by class.
//! - `resultType: "input_required"` → `TerminalStatus::Paused` — the
//!   verbatim `{inputRequests, requestState}` payload rides as the
//!   report's opaque `detail`; the dispatcher's paused arm records
//!   `control.decision{kind: ask}` + the `message_human` elicitation
//!   effect, and `resume_paused` re-enters with `request_state` echoed
//!   unmodified at `attempt_no + 1`.
//! - a JSON-RPC **error frame** → `EnvError::Transport` — transport-plane
//!   failure, `unknown{executor_error}` → `probe`; never conflated with
//!   `isError` (AC-R-2.5.4-12's distinctness clause).
//! - an unclassifiable result → `EnvError::Transport` — the edge cannot
//!   say; the probe path owns it.
//!
//! `idempotency-key` forwarding: every call carries `params.target` =
//! the kernel's `idempotency_key` (the `target` member is the edge's
//! dedup slot — the server replays the recorded verdict, never re-runs).
//!
//! `probe` is honest: `Undeterminable` — the MCP result channel does not
//! admit an application-verdict recheck.

use hh_env::capture::CaptureKind;
use hh_env::errors::EnvError;
use hh_env::executor::{
    DedupSupport, ExecutorDeclaration, ExecutorSignal, InterruptSupport, ProbeSupport,
    ProbeVerdict, TerminalReport, TerminalStatus, ToolExecutor,
};
use hh_hir::kinds::EffectDomain;
use hh_wire::json::Json;

use crate::client::{ClientError, McpClient, ToolOutcome, Transport};

/// The executor's declaration — `net_egress` (+ the fs pair the fixture tools declare) is the MCP edge's honest
/// domain (a tool call may reach outside the workspace; the capability's
/// declared `EffectClass` narrows it per call). `dedup_support: Durable`
/// is the executor-side `target` memo the serve loop holds; `probe` is
/// `none` (the edge cannot recheck application); interrupt `unknown`
/// (collapses to `unsupported` behaviourally — AC-R-2.5.5-12).
fn mcp_declaration() -> ExecutorDeclaration {
    ExecutorDeclaration {
        executor_id: "mcp/1".to_string(),
        isolation_support: hh_containment::policy::IsolationClass::ProcessSandbox,
        dedup_support: DedupSupport::Durable,
        probe_support: ProbeSupport::None,
        interrupt: InterruptSupport::Unknown,
        error_classes: [
            "executor_error",
            "timeout",
            "rate_limited",
            "invalid_arguments",
            "not_found",
            "conflict",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        streams: true,
        domains: [
            EffectDomain::NetEgress,
            EffectDomain::FsRead,
            EffectDomain::FsWrite,
        ]
        .iter()
        .cloned()
        .collect(),
    }
}

/// `McpExecutor` — a `ToolExecutor` whose `execute` is `tools/call` over
/// a negotiated [`McpClient`]. Construction is `connect` — the
/// probe-first handshake mints the [`crate::protocol::ProtocolBinding`]
/// before any call is accepted.
pub struct McpExecutor<T: Transport> {
    client: McpClient<T>,
    declaration: ExecutorDeclaration,
}

impl<T: Transport> McpExecutor<T> {
    /// `negotiate` + bind — the executor answers `execute` only after the
    /// edge's `ProtocolBinding` exists (the pin is the negotiated one).
    pub fn connect(transport: T) -> Result<McpExecutor<T>, ClientError> {
        Ok(McpExecutor {
            client: McpClient::connect(transport)?,
            declaration: mcp_declaration(),
        })
    }

    /// The negotiated client (the binding record's home).
    pub fn client(&self) -> &McpClient<T> {
        &self.client
    }

    /// The negotiated client, mutable (`drain_notifications` et al).
    pub fn client_mut(&mut self) -> &mut McpClient<T> {
        &mut self.client
    }

    /// One terminal report with the executor's honest hints.
    fn report(status: TerminalStatus, outcome_hint: &str) -> TerminalReport {
        TerminalReport {
            status,
            exit_status: None,
            outcome_hint: outcome_hint.to_string(),
            retryable_hint: None,
            detail_ref: None,
            truncated: false,
            omitted_bytes: 0,
            original_size: 0,
        }
    }
}

impl<T: Transport> ToolExecutor for McpExecutor<T> {
    fn declaration(&self) -> &ExecutorDeclaration {
        &self.declaration
    }

    fn execute(
        &mut self,
        request: &hh_env::executor::ExecutionRequest,
        sink: &mut dyn FnMut(ExecutorSignal),
    ) -> Result<TerminalReport, EnvError> {
        // `capability_ref.semantic_id` is the canonical tool name the
        // artefact's catalogue declared (`tools[i].name`).
        let outcome = self
            .client
            .call_tool_keyed(
                &request.capability_ref.0,
                &request.args,
                request.request_state.as_ref(),
                Some(&request.idempotency_key),
            )
            .map_err(|e| EnvError::Transport {
                detail: format!("mcp edge: {e}"),
            })?;
        match outcome {
            ToolOutcome::Observed { result } => {
                sink(ExecutorSignal {
                    token: request.attribution_token.clone(),
                    kind: CaptureKind::OutputChunk {
                        stream: "stdout".to_string(),
                        data: result.to_canonical_string(),
                    },
                });
                Ok(Self::report(TerminalStatus::Ok, "applied"))
            }
            ToolOutcome::Failed { result } => {
                // `isError` — the tool's own failure report (tool origin).
                // The content rides as the `error` chunk; the lifecycle
                // rule settles `not_applied`/`partial`/`unknown`.
                sink(ExecutorSignal {
                    token: request.attribution_token.clone(),
                    kind: CaptureKind::OutputChunk {
                        stream: "stderr".to_string(),
                        data: result.to_canonical_string(),
                    },
                });
                Ok(Self::report(
                    TerminalStatus::ToolError {
                        class: hh_env::observe::ErrorClass::ExecutorError,
                    },
                    "not_applied",
                ))
            }
            ToolOutcome::Paused {
                input_requests,
                request_state,
            } => Ok(Self::report(
                TerminalStatus::Paused {
                    detail: Json::obj([
                        ("input_requests", input_requests),
                        ("request_state", request_state),
                    ]),
                },
                "paused",
            )),
            // A JSON-RPC error frame is transport-plane failure — never an
            // `isError` conflation (AC-R-2.5.4-12's distinctness clause).
            ToolOutcome::ProtocolError { code, message } => Err(EnvError::Transport {
                detail: format!("mcp json-rpc error {code}: {message}"),
            }),
            ToolOutcome::Unknown { result } => Err(EnvError::Transport {
                detail: format!(
                    "mcp unclassifiable result: {}",
                    result.to_canonical_string()
                ),
            }),
        }
    }

    /// `probe` is honest — the MCP result channel cannot recheck
    /// application; `undeterminable` (never a silent re-run).
    fn probe(&self, _effect_id: &str, _attempt_no: u64) -> Result<ProbeVerdict, EnvError> {
        Ok(ProbeVerdict::Undeterminable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_env::deadline::DeadlineLadder;
    use hh_env::executor::ExecutionRequest;
    use std::collections::VecDeque;

    /// A scripted line transport — the connect handshake + `tools/call`
    /// replies the executor consumes; `sent` captures every request line
    /// (the `target`/`requestState` wire assertions read it).
    struct Script {
        sent: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        replies: VecDeque<String>,
    }

    impl Script {
        fn new(replies: Vec<String>, sent: std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> Self {
            Script {
                sent,
                replies: replies.into(),
            }
        }
    }

    impl Transport for Script {
        fn send(&mut self, line: &str) -> Result<(), String> {
            self.sent.lock().unwrap().push(line.to_string());
            Ok(())
        }
        fn recv(&mut self) -> Result<Option<String>, String> {
            Ok(self.replies.pop_front())
        }
    }

    fn result_frame(id: u64, result: Json) -> String {
        Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("id", Json::Int(id as i64)),
            ("result", result),
        ])
        .to_canonical_string()
    }

    fn error_frame(id: u64, code: i64) -> String {
        Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("id", Json::Int(id as i64)),
            (
                "error",
                Json::obj([("code", Json::Int(code)), ("message", Json::str("boom"))]),
            ),
        ])
        .to_canonical_string()
    }

    /// The connect handshake replies (`server/discover` + `initialize`).
    fn handshake() -> Vec<String> {
        vec![
            result_frame(
                1,
                Json::obj([
                    (
                        "protocol_version",
                        Json::str(crate::protocol::PINNED_MODERN),
                    ),
                    (
                        "supportedVersions",
                        Json::Arr(vec![
                            Json::str(crate::protocol::PINNED_MODERN),
                            Json::str(crate::protocol::PINNED_LEGACY),
                        ]),
                    ),
                    ("capabilities", Json::obj([("tools", Json::obj([]))])),
                ]),
            ),
            result_frame(
                2,
                Json::obj([
                    ("protocolVersion", Json::str(crate::protocol::PINNED_MODERN)),
                    ("capabilities", Json::obj([("tools", Json::obj([]))])),
                    ("serverInfo", Json::obj([("name", Json::str("srv"))])),
                ]),
            ),
        ]
    }

    fn request(request_state: Option<Json>) -> ExecutionRequest {
        ExecutionRequest {
            execution_id: "x-1".to_string(),
            effect_id: "eff-1".to_string(),
            attempt_no: 1,
            capability_ref: ("fs_read".to_string(), "v1".to_string()),
            effect: hh_hir::kinds::EffectClass::domain_only(EffectDomain::NetEgress),
            args: Json::obj([("path", Json::str("/tmp/x"))]),
            env_handle_id: "env-1".to_string(),
            attribution_token: "tok-1".to_string(),
            deadline_ms: None,
            ladder: DeadlineLadder {
                run_deadline_ms: None,
                phase_deadline_ms: None,
                effect_deadline_ms: None,
                attempt_deadline_ms: None,
            },
            retain_bytes_cap: 1 << 20,
            idempotency_key: "idem-key-77".to_string(),
            commit_evidence: hh_env::helper::CommitEvidence::ReadOnly,
            request_state,
        }
    }

    fn connect_with(
        replies: Vec<String>,
    ) -> (
        McpExecutor<Script>,
        std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ) {
        let sent = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        (
            McpExecutor::connect(Script::new(replies, sent.clone())).expect("connect"),
            sent,
        )
    }

    #[test]
    fn observed_result_maps_to_ok_and_emits_output() {
        let mut replies = handshake();
        replies.push(result_frame(
            3,
            Json::obj([
                ("resultType", Json::str("observed")),
                ("body", Json::str("done")),
            ]),
        ));
        let (mut exec, sent) = connect_with(replies);
        let mut signals = vec![];
        let report = exec
            .execute(&request(None), &mut |s| signals.push(s))
            .expect("execute");
        assert!(matches!(report.status, TerminalStatus::Ok));
        assert_eq!(report.outcome_hint, "applied");
        assert_eq!(signals.len(), 1);
        // `params.target` = the kernel's idempotency_key on the wire.
        let sent = sent.lock().unwrap();
        let wire: Json = hh_wire::json::parse(&sent[2]).unwrap();
        let params = wire.get("params").unwrap();
        assert_eq!(
            params.get("target"),
            Some(&Json::str("idem-key-77")),
            "the idempotency key rides `params.target` verbatim"
        );
        assert_eq!(params.get("name"), Some(&Json::str("fs_read")));
    }

    #[test]
    fn is_error_maps_to_tool_error_never_transport() {
        let mut replies = handshake();
        replies.push(result_frame(
            3,
            Json::obj([("isError", Json::Bool(true)), ("detail", Json::str("nope"))]),
        ));
        let (mut exec, _sent) = connect_with(replies);
        let mut signals = vec![];
        let report = exec
            .execute(&request(None), &mut |s| signals.push(s))
            .expect("execute");
        // `isError` is the *tool's* failure report — a ToolError at
        // tool origin, never a transport-plane fault.
        assert!(matches!(report.status, TerminalStatus::ToolError { .. }));
        assert_eq!(report.outcome_hint, "not_applied");
    }

    #[test]
    fn input_required_maps_to_paused_verbatim() {
        let mut replies = handshake();
        replies.push(result_frame(
            3,
            Json::obj([
                ("resultType", Json::str("input_required")),
                (
                    "inputRequests",
                    Json::Arr(vec![Json::obj([("prompt", Json::str("which?"))])]),
                ),
                ("requestState", Json::obj([("blob", Json::str("opaque-9"))])),
            ]),
        ));
        let (mut exec, _sent) = connect_with(replies);
        let report = exec.execute(&request(None), &mut |_| {}).expect("execute");
        let TerminalStatus::Paused { detail } = report.status else {
            panic!("expected Paused, got {:?}", report.status)
        };
        // The paused payload is verbatim — both members opaque.
        assert_eq!(
            detail.get("request_state").and_then(|s| s.get("blob")),
            Some(&Json::str("opaque-9"))
        );
        assert!(detail.get("input_requests").is_some());
        assert_eq!(report.outcome_hint, "paused");
    }

    #[test]
    fn resume_echoes_request_state_and_target() {
        let mut replies = handshake();
        replies.push(result_frame(
            3,
            Json::obj([("resultType", Json::str("observed"))]),
        ));
        let (mut exec, sent) = connect_with(replies);
        let req = request(Some(Json::obj([("blob", Json::str("opaque-9"))])));
        exec.execute(&req, &mut |_| {}).expect("execute");
        let sent = sent.lock().unwrap();
        let wire: Json = hh_wire::json::parse(&sent[2]).unwrap();
        let params = wire.get("params").unwrap();
        // `requestState` echoes byte-for-byte (D3 — the kernel never
        // parses it); `target` carries the idempotency key.
        assert_eq!(
            params.get("requestState").and_then(|s| s.get("blob")),
            Some(&Json::str("opaque-9"))
        );
        assert_eq!(params.get("target"), Some(&Json::str("idem-key-77")));
    }

    #[test]
    fn jsonrpc_error_is_transport_plane_never_tool_error() {
        let mut replies = handshake();
        replies.push(error_frame(3, -32602));
        let (mut exec, _sent) = connect_with(replies);
        let err = exec.execute(&request(None), &mut |_| {}).unwrap_err();
        // AC-R-2.5.4-12's distinctness: a JSON-RPC error frame is a
        // transport-plane failure (`unknown` → probe), never an
        // `isError` conflation.
        assert!(matches!(err, EnvError::Transport { .. }), "{err:?}");
    }

    #[test]
    fn unclassifiable_result_is_transport_plane() {
        let mut replies = handshake();
        replies.push(result_frame(
            3,
            Json::obj([("resultType", Json::str("limbo"))]),
        ));
        let (mut exec, _sent) = connect_with(replies);
        let err = exec.execute(&request(None), &mut |_| {}).unwrap_err();
        assert!(matches!(err, EnvError::Transport { .. }), "{err:?}");
    }

    #[test]
    fn probe_is_honestly_undeterminable() {
        let (exec, _sent) = connect_with(handshake());
        assert_eq!(
            exec.probe("eff-1", 1).unwrap(),
            ProbeVerdict::Undeterminable
        );
    }
}
