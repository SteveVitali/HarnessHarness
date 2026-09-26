//! The CLI's one seam to the kernel (K-1/K-2): a typed `hh-embed/1`
//! boundary. The real transport is binding (b) — a spawned
//! `hh-kernel serve` child whose stdin/stdout carry the generated
//! client's newline JSON-RPC. Tests drive the same `Boundary` trait over
//! the in-process binding (a) — byte-parity is the bindings' own
//! conformance property (AC-K4-2), so the CLI's behaviour cannot differ
//! by transport.

use std::collections::BTreeMap;
use std::io::BufReader;
use std::process::{Child, Command, Stdio};

use hh_embed_client_generated::{
    Client, ClientError, EmbedError, HelloParams, HelloResult, HostCapabilities, StreamNotification,
};
use hh_wire::json::Json;

use crate::invocation::InvocationError;

/// One typed boundary call's failure — transport vs typed refusal.
/// `Kernel` carries the closed `EmbedError` sum (ADR-0176 D5; the
/// exit-class table reads `kind`, never free text). `Invocation` is the
/// CLI's own pre-ledger refusal — `invocation_error`, emitted before any
/// boundary operation runs.
#[derive(Debug)]
pub enum CliError {
    /// A typed kernel refusal (`error.data.kind`).
    Kernel(EmbedError),
    /// Transport/spawn/decode/identity failure — `infrastructure_failure`.
    Transport(String),
    /// A pre-ledger CLI refusal — `invocation_error`.
    Invocation(InvocationError),
}

impl CliError {
    /// From the generated client's error sum. `Rpc` is the typed refusal;
    /// `Decode`, `IdentityMismatch` and `Transport` are all
    /// infrastructure failures on the wire (no silent fallback —
    /// ADR-0178 D2).
    pub fn from_client(e: ClientError) -> CliError {
        match e {
            ClientError::Rpc(e) => CliError::Kernel(e),
            ClientError::IdentityMismatch { field } => CliError::Transport(format!(
                "kernel identity mismatch on {field} — refusing the fallback"
            )),
            other => CliError::Transport(format!("{other:?}")),
        }
    }
}

/// The boundary the CLI drives — one `hh-embed/1` operation per call,
/// nothing else (K-2).
pub trait Boundary {
    /// Invoke one operation; a typed refusal comes back as
    /// `CliError::Kernel`.
    fn call(&mut self, method: &str, params: &Json) -> Result<Json, CliError>;
    /// The next live `stream.frame` notification (blocking on binding
    /// (b); draining the service queue on binding (a)). `Ok(None)` means
    /// "a non-frame line arrived — keep polling".
    fn poll_frame(&mut self) -> Result<Option<StreamNotification>, CliError>;
    /// The next `upcall.*` ask `(method, params)` — the Group U channel
    /// the CLI serves (`upcall.request_permission`,
    /// `upcall.invoke_host_capability`). `Ok(None)` means a non-upcall
    /// line arrived.
    fn poll_upcall(&mut self) -> Result<Option<(String, Json)>, CliError>;
    /// The negotiated `HelloResult` (set at connect).
    fn hello_result(&self) -> Option<&HelloResult>;
}

/// The `HostCapabilities` a CLI surface declares (ADR-0179: the channel
/// asks are capability-gated — the CLI *serves* the permission and
/// host-effect channels the attended loop answers, accepts the ephemeral
/// frames it renders, and caps in-flight sessions at one).
fn cli_capabilities() -> HostCapabilities {
    HostCapabilities {
        experimental: true, // `amend` is experimental-tier (ADR-0216 OQ-468)
        opt_out_notifications: Vec::new(),
        serves_permission_channel: true,
        serves_host_executor: true,
        serves_hook_observer: false,
        serves_elicitation: false,
        serves_measurement: false,
        serves_principal_channel: false,
        accepts_ephemeral_frames: true,
        // The CLI holds at most one *live* session per invocation, but a
        // parked run's writer session outlives the invocation that
        // detached it — the follow-up commands (`run resume/cancel`,
        // `approval respond`, `run status`) must open alongside it. The
        // declared bound therefore defers to the kernel default; the
        // per-invocation discipline is the CLI's own (§7.1 M-1 — the
        // cap must never deadlock a parked run's next command).
        max_in_flight_sessions: None,
        extensions: BTreeMap::new(),
    }
}

/// The `HelloParams` every CLI connection opens with.
pub fn cli_hello() -> HelloParams {
    HelloParams {
        contract_major: hh_embed_client_generated::CONTRACT_MAJOR,
        client: hh_embed_client_generated::ClientDescriptor {
            name: "hh-cli".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            kind: hh_embed_client_generated::ClientKind::Cli,
        },
        capabilities: cli_capabilities(),
        schema_hash: Some(hh_embed_client_generated::EXPECTED_SCHEMA_HASH.to_string()),
        kernel_floor: None,
    }
}

/// Binding (b): the generated client over a spawned `hh-kernel serve`
/// child (a service in a separate process — AC-R-2.11.1-1).
pub struct ProcessBoundary {
    child: Child,
    client: Client<BufReader<std::process::ChildStdout>, std::process::ChildStdin>,
}

impl ProcessBoundary {
    /// Spawn `kernel_cmd serve` (`--kernel-cmd` / `HH_KERNEL_CMD`, default
    /// `hh-kernel`) with `HH_STORE_ROOT`/`HH_WORKSPACE_ROOT` in the
    /// child's environment, then run the typed `hello` negotiation.
    /// A spawn or negotiation failure is `infrastructure_failure` /
    /// `refused_by_kernel`, never a fallback.
    pub fn spawn(kernel_cmd: &str, env: &[(String, String)]) -> Result<ProcessBoundary, CliError> {
        let mut cmd = if kernel_cmd.contains(' ') {
            let mut it = kernel_cmd.split_whitespace();
            let mut c = Command::new(it.next().unwrap());
            c.args(it);
            c
        } else {
            Command::new(kernel_cmd)
        };
        let mut child = cmd
            .arg("serve")
            .envs(env.iter().cloned())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| CliError::Transport(format!("spawn {kernel_cmd}: {e}")))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CliError::Transport("child stdout closed".into()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| CliError::Transport("child stdin closed".into()))?;
        let mut client = Client::new(BufReader::new(stdout), stdin);
        client.hello(&cli_hello()).map_err(CliError::from_client)?;
        Ok(ProcessBoundary { child, client })
    }
}

impl Boundary for ProcessBoundary {
    fn call(&mut self, method: &str, params: &Json) -> Result<Json, CliError> {
        self.client
            .call(method, params.clone())
            .map_err(CliError::from_client)
    }
    fn poll_frame(&mut self) -> Result<Option<StreamNotification>, CliError> {
        self.client
            .poll_notification()
            .map_err(CliError::from_client)
    }
    fn poll_upcall(&mut self) -> Result<Option<(String, Json)>, CliError> {
        self.client.poll_upcall().map_err(CliError::from_client)
    }
    fn hello_result(&self) -> Option<&HelloResult> {
        self.client.hello_result.as_ref()
    }
}

impl Drop for ProcessBoundary {
    fn drop(&mut self) {
        // Closing stdin ends the serve loop; reap the child.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
