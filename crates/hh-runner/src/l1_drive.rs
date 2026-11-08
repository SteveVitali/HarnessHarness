//! `l1_drive` — the L1 drive machinery (manifest 282/R6.28, K2 + K4;
//! spec §7.1 REQ-XE-004, §7.4 REQ-XD-103; design/36 §1.2–§1.3).
//!
//! - [`SequenceReplay`] — the recorded-response endpoint's core:
//!   serves a `hh.model_proxy.recording/1` call sequence in order,
//!   captures every downstream request, applies the **declared lane
//!   transform** to derive the upstream-bound comparison bytes
//!   (capture point 2, design/36 §1.3 — the gate's point), and in
//!   `strict` mode answers a canonical divergence with the typed
//!   `replay_diverged` refusal (P4). A request beyond the sequence is
//!   the typed `sequence_exhausted` refusal (P5) — the extra-request
//!   fail.
//! - [`RecordedResponseEndpoint`] — the same sequence over a loopback
//!   TCP socket: the recorded-response endpoint an original is driven
//!   against through its own base-URL setting (REQ-XE-004). The real
//!   O leg rides `hh-model-proxy`'s replay mode in W2; this is the
//!   $0 fixture form — serve and capture only, no transforms beyond
//!   the declared lane, no receipts.
//! - [`HttpEndpoint`] — a real-socket `POST` client, so a harness is
//!   driven end-to-end over TCP in the fixture.
//! - [`L1Exec`] impls — [`ObsReplay`] rides `crate::l1_obs`;
//!   [`LiveExec`] runs the command for real (the L1.2 cell seam at
//!   fixture level — `sh -c`, merged output, deadline ⇒ `rc -1` +
//!   `exception_info`, E1 A.4).
//! - [`ToyMini`] — the fixture harness pair: mini-swe-agent's request
//!   shape (`messages=[system, user(instance), (assistant, tool…)*]`,
//!   `tools=[BASH_TOOL]`, observation template, submit marker,
//!   three-strike format errors) as an in-crate driver, with a
//!   declared [`DivergencePlan`] so the toy H carries a controlled
//!   seeded divergence for the gate battery.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use hharness_gateway::recording::RecordedCall;
use hharness_gateway::replay::{match_requests, MatchVerdict, RequestCanonicalizer};
use hharness_wire::json::{parse, Json};

use crate::json_ext::JsonExt;
use crate::l1_obs::{err, render_mini_observation, L1Error, L1Exec, RawObservation, SUBMIT_MARKER};

/// The route the recorded-response endpoint and client agree on.
const CHAT_ROUTE: &str = "\u{2f}v1\u{2f}chat\u{2f}completions";

/// The recorded route, for corpus-side call authoring.
pub(crate) const CHAT_ROUTE_PUB: &str = CHAT_ROUTE;

/// `LaneTransform` — the declared lane the replay endpoint applies to
/// derive upstream-bound bytes (capture point 2). `drop_members` are
/// stripped, `set_members` merged into the request object — the
/// fixture model of the real lane's effort pin, `max_tokens`
/// insertion and unsupported-field stripping (design/36 §1.3).
/// Identity (`LaneTransform::default`) is a legal declared lane.
#[derive(Debug, Clone, Default)]
pub struct LaneTransform {
    /// Object members removed before upstream dispatch.
    pub drop_members: Vec<String>,
    /// Object members inserted/overwritten before upstream dispatch.
    pub set_members: Vec<(String, Json)>,
}

impl LaneTransform {
    /// `apply(body)` — the upstream-bound bytes; a non-object body is
    /// a typed refusal, never a silent pass.
    pub fn apply(&self, body: &[u8]) -> Result<Vec<u8>, L1Error> {
        let mut doc = parse(&String::from_utf8_lossy(body))
            .map_err(|e| err("lane_transform", format!("request body is not JSON: {e}")))?;
        if let Json::Obj(m) = &mut doc {
            for name in &self.drop_members {
                m.remove(name);
            }
            for (k, v) in &self.set_members {
                m.insert(k.clone(), v.clone());
            }
        } else {
            return Err(err("lane_transform", "request body must be an object"));
        }
        Ok(doc.to_canonical_string().into_bytes())
    }
}

/// One captured downstream request (capture point 1 — the harness's
/// own bytes; the upstream-bound view derives through the lane).
#[derive(Debug, Clone, PartialEq)]
pub struct CapturedRequest {
    /// The 1-based request ordinal.
    pub turn: usize,
    /// The request method.
    pub method: String,
    /// The request path.
    pub path: String,
    /// The declared `content-type`, when sent.
    pub content_type: Option<String>,
    /// The request body bytes as sent.
    pub body: Vec<u8>,
    /// The upstream-bound body after the declared lane (capture
    /// point 2 — the gate's comparison input).
    pub upstream_body: Vec<u8>,
}

/// `SequenceReplay` — serves one `hh.model_proxy.recording/1` call
/// sequence at the replay seam (P3 positional serving, P4 matcher,
/// P5 typed exhaustion). `strict` answers a canonical divergence with
/// `replay_diverged`; lenient serves anyway and records the verdict —
/// lenient results are diagnostics, never gate evidence.
pub struct SequenceReplay {
    calls: Vec<RecordedCall>,
    lane: LaneTransform,
    pos: usize,
    captured: Vec<CapturedRequest>,
    strict: bool,
    canon_id: Option<String>,
    cell_facts: Vec<(String, String)>,
    surface: String,
    verdicts: Vec<MatchVerdict>,
    first_divergence: Option<(usize, String)>,
}

impl SequenceReplay {
    /// `SequenceReplay::new(calls)` — positional serving, no matcher.
    pub fn new(calls: Vec<RecordedCall>) -> SequenceReplay {
        SequenceReplay {
            calls,
            lane: LaneTransform::default(),
            pos: 0,
            captured: Vec::new(),
            strict: false,
            canon_id: None,
            cell_facts: Vec::new(),
            surface: String::new(),
            verdicts: Vec::new(),
            first_divergence: None,
        }
    }

    /// Bind the declared lane transform (capture point 2).
    pub fn with_lane(mut self, lane: LaneTransform) -> SequenceReplay {
        self.lane = lane;
        self
    }

    /// Bind the canonicalizer + strictness (P4) — the canonicalizer
    /// *spelling* rides (the endpoint constructs it per serve, so the
    /// endpoint stays `Send` for the socket leg).
    pub fn with_matcher(
        mut self,
        canon: Box<dyn RequestCanonicalizer>,
        surface: &str,
        strict: bool,
    ) -> SequenceReplay {
        self.canon_id = Some(canon.id().to_string());
        self.surface = surface.to_string();
        self.strict = strict;
        self
    }

    /// Bind C04's recorded cell facts (`hh-l1-canon` only).
    pub fn with_cell_facts(mut self, facts: &[(String, String)]) -> SequenceReplay {
        self.cell_facts = facts.to_vec();
        self
    }

    /// The recorded per-call match verdicts.
    pub fn verdicts(&self) -> &[MatchVerdict] {
        &self.verdicts
    }

    /// The captured requests in serve order.
    pub fn captured(&self) -> &[CapturedRequest] {
        &self.captured
    }

    /// `(turn, first_diff_path)` of the first canonical divergence.
    pub fn first_divergence(&self) -> Option<(usize, String)> {
        self.first_divergence.clone()
    }

    /// The recorded call count.
    pub fn len(&self) -> usize {
        self.calls.len()
    }

    /// `true` when the sequence is exhausted.
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    /// `serve(method, path, content_type, body)` — one request in,
    /// the recorded response out (or a typed refusal).
    pub fn serve(
        &mut self,
        method: &str,
        path: &str,
        content_type: Option<&str>,
        body: &[u8],
    ) -> Result<(u16, Vec<u8>), L1Error> {
        let turn = self.pos + 1;
        let upstream = self.lane.apply(body)?;
        self.captured.push(CapturedRequest {
            turn,
            method: method.to_string(),
            path: path.to_string(),
            content_type: content_type.map(str::to_string),
            body: body.to_vec(),
            upstream_body: upstream.clone(),
        });
        let Some(call) = self.calls.get(self.pos) else {
            return Err(err(
                "sequence_exhausted",
                format!("recorded sequence exhausted at turn {turn}"),
            ));
        };
        if let Some(id) = self.canon_id.clone() {
            let canon = hharness_gateway::replay::canonicalizer_for(&id, &self.cell_facts)
                .map_err(|e| err("transport", format!("canonicalizer: {}", e.detail)))?;
            let expected = call
                .upstream_request
                .as_deref()
                .unwrap_or(&call.downstream_body);
            let verdict = match_requests(canon.as_ref(), &self.surface, expected, &upstream)
                .unwrap_or_else(|r| MatchVerdict::Diverged {
                    first_diff_path: format!("canon_error: {}", r.detail),
                });
            let diverged = match &verdict {
                MatchVerdict::Diverged { first_diff_path } => Some(first_diff_path.clone()),
                _ => None,
            };
            self.verdicts.push(verdict);
            if self.strict {
                if let Some(p) = diverged {
                    self.first_divergence = Some((turn, p.clone()));
                    return Err(err(
                        "replay_diverged",
                        format!("turn {turn} diverged at {p}"),
                    ));
                }
            }
        }
        self.pos += 1;
        if let Some((kind, detail)) = &call.failure {
            return Err(err(
                "transport",
                format!("recorded transport failure {kind}: {detail}"),
            ));
        }
        // The recorded response to serve: the upstream exchange, else
        // the released downstream bytes when no upstream was captured
        // (the same serve material for the fixture endpoint — one
        // response view, never a second shape).
        if let Some(r) = &call.upstream_response {
            return Ok((r.status, r.body.clone()));
        }
        if let Some(r) = &call.released {
            return Ok((r.status, r.body.clone()));
        }
        Err(err(
            "transport",
            format!("recorded call at turn {turn} carries no response"),
        ))
    }
}

/// `L1Endpoint` — the harness's model seam for a drive: one request
/// body in, `(status, body)` out or a typed refusal.
pub trait L1Endpoint {
    /// `post(body)` — the request; `(status, body)` on serve.
    fn post(&mut self, body: &[u8]) -> Result<(u16, Vec<u8>), L1Error>;
}

impl L1Endpoint for SequenceReplay {
    fn post(&mut self, body: &[u8]) -> Result<(u16, Vec<u8>), L1Error> {
        self.serve("POST", CHAT_ROUTE, Some("application/json"), body)
    }
}

/// `RecordedResponseEndpoint` — a `SequenceReplay` over a loopback
/// TCP listener (REQ-XE-004's recorded-response endpoint at $0): the
/// harness's base-URL setting points at `127.0.0.1:{port}`; every
/// POST is captured and answered with the recorded exchange.
pub struct RecordedResponseEndpoint {
    inner: Arc<Mutex<SequenceReplay>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    port: u16,
}

impl RecordedResponseEndpoint {
    /// `bind(replay)` — serve `replay` on a fresh loopback port.
    /// Returns the endpoint; `port()` is the bound port.
    pub fn bind(replay: SequenceReplay) -> Result<RecordedResponseEndpoint, L1Error> {
        let listener =
            TcpListener::bind("127.0.0.1:0").map_err(|e| err("transport", format!("bind: {e}")))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| err("transport", format!("nonblocking: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| err("transport", format!("local_addr: {e}")))?
            .port();
        let inner = Arc::new(Mutex::new(replay));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread = {
            let inner = Arc::clone(&inner);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut sock, _)) => {
                            let _ = sock.set_nonblocking(false);
                            let _ = serve_conn(&mut sock, &inner);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        }
                        Err(_) => break,
                    }
                }
            })
        };
        Ok(RecordedResponseEndpoint {
            inner,
            stop,
            thread: Some(thread),
            port,
        })
    }

    /// The bound port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The captured requests in serve order.
    pub fn captured(&self) -> Vec<CapturedRequest> {
        self.inner
            .lock()
            .map(|r| r.captured().to_vec())
            .unwrap_or_default()
    }
}

impl Drop for RecordedResponseEndpoint {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn serve_conn(
    sock: &mut std::net::TcpStream,
    inner: &Arc<Mutex<SequenceReplay>>,
) -> Result<(), L1Error> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    // Read the request head + the Content-Length body.
    let mut content_len = 0usize;
    let mut head_end = None;
    for _ in 0..64 {
        let n = sock
            .read(&mut chunk)
            .map_err(|e| err("transport", format!("read: {e}")))?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if head_end.is_none() {
            if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
                head_end = Some(pos + 4);
                let head = String::from_utf8_lossy(&buf[..pos]);
                for line in head.lines() {
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        content_len = v.trim().parse().unwrap_or(0);
                    }
                }
            }
        }
        if let Some(he) = head_end {
            if buf.len() >= he + content_len {
                break;
            }
        }
    }
    let he = head_end.ok_or_else(|| err("transport", "malformed request head"))?;
    let body = buf[he..he + content_len].to_vec();
    let (status, out) = match inner.lock() {
        Ok(mut r) => match r.serve("POST", CHAT_ROUTE, Some("application/json"), &body) {
            Ok((s, b)) => (s, b),
            Err(e) => (
                409,
                Json::obj([
                    ("error", Json::str(e.rule)),
                    ("detail", Json::str(e.detail)),
                ])
                .to_canonical_string()
                .into_bytes(),
            ),
        },
        Err(_) => (500, b"endpoint poisoned".to_vec()),
    };
    let head = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        out.len()
    );
    sock.write_all(head.as_bytes())
        .and_then(|_| sock.write_all(&out))
        .map_err(|e| err("transport", format!("write: {e}")))?;
    Ok(())
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// `HttpEndpoint` — a real-socket `L1Endpoint`: posts each request to
/// `http://127.0.0.1:{port}{CHAT_ROUTE}` over TCP (the drive's
/// end-to-end leg — REQ-XE-004 through the harness's own base-URL
/// setting).
pub struct HttpEndpoint {
    addr: String,
}

impl HttpEndpoint {
    /// `HttpEndpoint::new(port)` — the loopback base-URL.
    pub fn new(port: u16) -> HttpEndpoint {
        HttpEndpoint {
            addr: format!("127.0.0.1:{port}"),
        }
    }
}

impl L1Endpoint for HttpEndpoint {
    fn post(&mut self, body: &[u8]) -> Result<(u16, Vec<u8>), L1Error> {
        let mut sock = std::net::TcpStream::connect(&self.addr)
            .map_err(|e| err("transport", format!("connect {}: {e}", self.addr)))?;
        let head = format!(
            "POST {CHAT_ROUTE} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.addr,
            body.len()
        );
        sock.write_all(head.as_bytes())
            .and_then(|_| sock.write_all(body))
            .map_err(|e| err("transport", format!("write: {e}")))?;
        let mut buf = Vec::new();
        sock.read_to_end(&mut buf)
            .map_err(|e| err("transport", format!("read: {e}")))?;
        let he = find_subslice(&buf, b"\r\n\r\n")
            .ok_or_else(|| err("transport", "malformed response head"))?;
        let head = String::from_utf8_lossy(&buf[..he]);
        let status: u16 = head
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| err("transport", "response carries no status"))?;
        if status >= 400 {
            return Err(err(
                "sequence_exhausted",
                format!("endpoint refused status {status}"),
            ));
        }
        Ok((status, buf[he + 4..].to_vec()))
    }
}

/// `LiveExec` — the live-exec leg (L1.2): runs the command under
/// `sh -c` with merged stdout/stderr, UTF-8 `errors="replace"`,
/// deadline ⇒ process-group kill, `returncode -1` and
/// `exception_info` (E1 A.4's 30 s shape; the fixture declares its
/// own bound).
pub struct LiveExec {
    /// The enforced deadline per command (ms); `None` unbounded.
    pub timeout_ms: Option<u64>,
}

impl L1Exec for LiveExec {
    fn run(&mut self, command: &str) -> Result<RawObservation, L1Error> {
        // Merged stdout+stderr via the shell's own `2>&1` — E1 A.4's
        // `stdout=PIPE, stderr=STDOUT` shape without two pipes to
        // deadlock on.
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("{command} 2>&1"))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| err("drive", format!("exec spawn: {e}")))?;
        let deadline = self
            .timeout_ms
            .map(|ms| std::time::Instant::now() + std::time::Duration::from_millis(ms));
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                let mut out = Vec::new();
                if let Some(mut s) = child.stdout.take() {
                    let _ = s.read_to_end(&mut out);
                }
                return Ok(RawObservation {
                    output: String::from_utf8_lossy(&out).into_owned(),
                    returncode: status.code().unwrap_or(-1) as i64,
                    exception_info: None,
                });
            }
            if let Some(d) = deadline {
                if std::time::Instant::now() > d {
                    let _ = child.kill();
                    let _ = child.wait();
                    let mut out = Vec::new();
                    if let Some(mut s) = child.stdout.take() {
                        let _ = s.read_to_end(&mut out);
                    }
                    return Ok(RawObservation {
                        output: String::from_utf8_lossy(&out).into_owned(),
                        returncode: -1,
                        exception_info: Some(Json::str(format!(
                            "An error occurred while executing the command: Command '{command}' timed out"
                        ))),
                    });
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}

/// One parsed or executed action — `command` + `tool_call_id`, the
/// step-level equality members (REQ-XD-104; design/36 §1.3).
#[derive(Debug, Clone, PartialEq)]
pub struct L1Action {
    /// The action's `command` string.
    pub command: String,
    /// The action's `tool_call_id`.
    pub tool_call_id: String,
}

/// `L1Drive` — one drive's captured facts: the request sequence, the
/// parsed and executed action lists per step, the termination class,
/// the submission payload and the extra-request flag (what REQ-XD-104
/// compares — never verdict material, X-07).
#[derive(Debug, Clone, Default)]
pub struct L1Drive {
    /// The captured requests in order (`turn` = 1-based ordinal).
    pub requests: Vec<CapturedRequest>,
    /// Parsed action lists per step.
    pub parsed_actions: Vec<Vec<L1Action>>,
    /// Executed action lists per step (a mid-batch submit pads the
    /// rest as not-executed — design M14).
    pub executed_actions: Vec<Vec<L1Action>>,
    /// The termination class (`submitted` | `repeated_format_error` |
    /// `limits_exceeded` | `time_exceeded` | `exception{…}`).
    pub termination: String,
    /// The submission payload (output lines after the marker).
    pub submission: Option<String>,
    /// The harness attempted a request past the sequence end.
    pub extra_request: bool,
    /// The drive-level typed refusal, when the drive aborted on one
    /// (a `replay_diverged` stop is still a completed drive's fact).
    pub refusal: Option<L1Error>,
}

/// `DivergencePlan` — the declared seeded divergence a toy H carries
/// (K4's fixture needs an injected divergence detected at the right
/// step and leg; every knob is declared, never ambient).
#[derive(Debug, Clone, Default)]
pub struct DivergencePlan {
    /// `(turn, members)` — merge these members into the request
    /// object at `turn` (a request-level divergence).
    pub request_patch: Option<(usize, Vec<(String, Json)>)>,
    /// `(turn, suffix)` — execute `command + suffix` at `turn` (an
    /// action-level divergence; the request itself stays equal).
    pub command_suffix: Option<(usize, String)>,
    /// Attempt one request after the sequence end (the extra-request
    /// level).
    pub extra_request: bool,
    /// Submit a different payload (a submission-level divergence).
    pub submission_override: Option<String>,
    /// Terminate with a different class (a termination-level
    /// divergence applied at drive end).
    pub termination_override: Option<String>,
}

/// The toy's format-error user message (fixed text — the same for O
/// and H; the gate compares behaviour, not this literal).
const FORMAT_ERROR: &str =
    "Tool call error: malformed response. Respond with exactly one bash tool call.";
/// The `finish_reason:"length"` variant (mini's truncation branch).
const FORMAT_ERROR_TRUNCATED: &str =
    "Your previous response reached the output token limit before you produced a tool call, so it was cut off.";

/// The fixture `BASH_TOOL` (E1 A.4's `models/utils/actions_toolcall.py:11–27`).
pub(crate) fn bash_tool_pub() -> Json {
    bash_tool()
}

fn bash_tool() -> Json {
    parse(
        "{\"type\":\"function\",\"function\":{\"name\":\"bash\",\"description\":\"Execute a bash command\",\"parameters\":{\"type\":\"object\",\"properties\":{\"command\":{\"type\":\"string\",\"description\":\"The bash command to execute\"}},\"required\":[\"command\"]}}}",
    )
    .unwrap_or(Json::Null)
}

/// `ToyMini` — the fixture O/H pair: mini-swe-agent's request shape
/// and control loop as an in-crate driver. `base_url` is the
/// harness's own base-URL setting (the `OPENAI_BASE_URL` /
/// `model.model_kwargs.api_base` analogue — REQ-XE-004's "through its
/// own base-URL setting"); the model seam itself is the injected
/// [`L1Endpoint`] so one toy drives in-process (L1 legs) and over a
/// real socket (the driver leg).
pub struct ToyMini {
    /// The participant name recorded on L1 artifacts.
    pub name: String,
    /// The harness's base-URL setting (recorded, never used in-process).
    pub base_url: String,
    /// The `model` member every request carries.
    pub model: String,
    /// The system prompt bytes.
    pub system_prompt: String,
    /// The `user(instance)` content (task + uname line, verbatim).
    pub instance: String,
    /// The exec seam this run binds (`ObsReplay` for L1.1, `LiveExec`
    /// for L1.2, a scripted env for L1.0).
    pub exec: Box<dyn L1Exec>,
    /// The declared seeded divergence (`None` = faithful).
    pub divergence: DivergencePlan,
    /// Consecutive format errors tolerated before
    /// `repeated_format_error` (mini's 3).
    pub max_format_errors: u32,
}

impl ToyMini {
    /// `ToyMini::new(name, instance, exec)` — a faithful toy
    /// participant.
    pub fn new(name: &str, instance: &str, exec: Box<dyn L1Exec>) -> ToyMini {
        ToyMini {
            name: name.to_string(),
            base_url: "http://127.0.0.1:0".to_string(),
            model: "glm-5.3-flash".to_string(),
            system_prompt: "You are a toy mini-swe-agent.".to_string(),
            instance: instance.to_string(),
            exec,
            divergence: DivergencePlan::default(),
            max_format_errors: 3,
        }
    }

    /// `drive(&mut self, endpoint)` — run the request/exec loop to
    /// termination; the captured facts are the L1Drive.
    pub fn drive(&mut self, endpoint: &mut dyn L1Endpoint) -> L1Drive {
        let mut d = L1Drive::default();
        let mut messages = vec![
            Json::obj([
                ("role", Json::str("system")),
                ("content", Json::str(&self.system_prompt)),
            ]),
            Json::obj([
                ("role", Json::str("user")),
                ("content", Json::str(&self.instance)),
            ]),
        ];
        let mut format_errors = 0u32;
        let mut turn = 0usize;
        loop {
            turn += 1;
            let mut req = Json::obj([
                ("model", Json::str(&self.model)),
                ("messages", Json::Arr(messages.clone())),
                ("tools", Json::Arr(vec![bash_tool()])),
            ]);
            if let Some((t, members)) = &self.divergence.request_patch {
                if *t == turn {
                    if let Json::Obj(m) = &mut req {
                        for (k, v) in members {
                            m.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            let body = req.to_canonical_string().into_bytes();
            let (status, mut resp_body) = match endpoint.post(&body) {
                Ok(r) => r,
                Err(e) => {
                    if e.rule == "sequence_exhausted" {
                        // The participant asked past the sequence —
                        // the extra-request fact (REQ-XD-104).
                        d.extra_request = true;
                        d.termination = "exception{sequence_exhausted}".to_string();
                    } else {
                        d.termination = format!("exception{{{}}}", e.rule);
                    }
                    if e.rule == "replay_diverged" {
                        // A strict-endpoint stop is a completed drive
                        // fact — the gate reads the endpoint's
                        // recorded divergence for the diff record.
                        d.refusal = Some(e);
                    }
                    break;
                }
            };
            d.requests.push(CapturedRequest {
                turn,
                method: "POST".to_string(),
                path: CHAT_ROUTE.to_string(),
                content_type: Some("application/json".to_string()),
                body: body.clone(),
                upstream_body: body.clone(),
            });
            // A recorded transport status (BM17's 429/502 → the toy's
            // declared retry: identical bytes, one retry; the retry
            // count is part of request equality).
            if status == 429 || status >= 500 {
                match endpoint.post(&body) {
                    Ok((s2, b2)) => {
                        d.requests.push(CapturedRequest {
                            turn,
                            method: "POST".to_string(),
                            path: CHAT_ROUTE.to_string(),
                            content_type: Some("application/json".to_string()),
                            body: body.clone(),
                            upstream_body: body,
                        });
                        if s2 == 429 || s2 >= 500 {
                            d.termination = "exception{transport_retry_exhausted}".to_string();
                            break;
                        }
                        resp_body = b2;
                    }
                    Err(e) => {
                        if e.rule == "sequence_exhausted" {
                            d.extra_request = true;
                        }
                        d.termination = format!("exception{{{}}}", e.rule);
                        break;
                    }
                }
            }
            match self.step(&mut d, turn, &resp_body, &mut messages, &mut format_errors) {
                StepOutcome::Continue => {}
                StepOutcome::Done => break,
            }
        }
        if self.divergence.extra_request {
            let probe = Json::obj([("model", Json::str(&self.model))])
                .to_canonical_string()
                .into_bytes();
            let _ = endpoint.post(&probe);
            d.extra_request = true;
        }
        if let Some(t) = &self.divergence.termination_override {
            d.termination = t.clone();
        }
        if let Some(s) = &self.divergence.submission_override {
            d.submission = Some(s.clone());
        }
        d
    }

    /// One turn: parse the response, drive the actions, append the
    /// messages. Returns whether the loop continues.
    fn step(
        &mut self,
        d: &mut L1Drive,
        turn: usize,
        resp_body: &[u8],
        messages: &mut Vec<Json>,
        format_errors: &mut u32,
    ) -> StepOutcome {
        let resp = match parse(&String::from_utf8_lossy(resp_body)) {
            Ok(r) => r,
            Err(_) => {
                d.termination = "exception{response_parse}".to_string();
                return StepOutcome::Done;
            }
        };
        let choice = resp
            .get("choices")
            .and_then(JsonExt::as_arr)
            .and_then(|a| a.first());
        let message = choice.and_then(|c| c.get("message"));
        let finish_reason = choice
            .and_then(|c| c.get("finish_reason"))
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let Some(msg) = message else {
            d.termination = "exception{response_shape}".to_string();
            return StepOutcome::Done;
        };
        messages.push(msg.clone());
        let actions = parse_actions(msg, &finish_reason);
        d.parsed_actions
            .push(actions.as_ref().map(|a| a.clone()).unwrap_or_default());
        match actions {
            Err(fmt) => {
                *format_errors += 1;
                if *format_errors >= self.max_format_errors {
                    d.termination = "repeated_format_error".to_string();
                    return StepOutcome::Done;
                }
                messages.push(Json::obj([
                    ("role", Json::str("user")),
                    (
                        "content",
                        Json::str(if fmt.truncated {
                            FORMAT_ERROR_TRUNCATED
                        } else {
                            FORMAT_ERROR
                        }),
                    ),
                ]));
                d.executed_actions.push(Vec::new());
                return StepOutcome::Continue;
            }
            Ok(actions) => {
                *format_errors = 0;
                let mut executed = Vec::new();
                for a in actions.iter() {
                    let command = match &self.divergence.command_suffix {
                        Some((t, suffix)) if *t == turn => format!("{}{}", a.command, suffix),
                        _ => a.command.clone(),
                    };
                    let raw = match self.exec.run(&command) {
                        Ok(r) => r,
                        Err(e) => {
                            d.termination = format!("exception{{{}}}", e.rule);
                            d.executed_actions.push(executed);
                            return StepOutcome::Done;
                        }
                    };
                    executed.push(L1Action {
                        command: command.clone(),
                        tool_call_id: a.tool_call_id.clone(),
                    });
                    let (submitted, payload) = submission(&raw);
                    let content = render_mini_observation(&raw);
                    messages.push(Json::obj([
                        ("role", Json::str("tool")),
                        ("tool_call_id", Json::str(&a.tool_call_id)),
                        ("content", Json::str(content)),
                    ]));
                    if submitted {
                        d.termination = "submitted".to_string();
                        d.submission = payload;
                        d.executed_actions.push(executed);
                        return StepOutcome::Done;
                    }
                }
                d.executed_actions.push(executed);
                StepOutcome::Continue
            }
        }
    }
}

enum StepOutcome {
    Continue,
    Done,
}

/// `submission(raw)` — mini's `_check_finished`: the marker on the
/// output's first line with `returncode == 0` ⇒ `Submitted` with the
/// remaining lines as the payload (M12/M13).
fn submission(raw: &RawObservation) -> (bool, Option<String>) {
    if raw.returncode != 0 {
        return (false, None);
    }
    let mut lines = raw.output.split('\n');
    if lines.next().map(str::trim) == Some(SUBMIT_MARKER) {
        (true, Some(lines.collect::<Vec<_>>().join("\n")))
    } else {
        (false, None)
    }
}

struct FmtError {
    truncated: bool,
}

/// `parse_actions(msg, finish_reason)` — mini's action parser: ≥1
/// `bash` tool call with a `command` string (M3–M7, M18); the first
/// malformed call is a format error for the whole response;
/// `finish_reason:"length"` with no tool calls is the truncation
/// branch (M8).
fn parse_actions(msg: &Json, finish_reason: &str) -> Result<Vec<L1Action>, FmtError> {
    let calls = msg
        .get("tool_calls")
        .and_then(JsonExt::as_arr)
        .cloned()
        .unwrap_or_default();
    if calls.is_empty() {
        return Err(FmtError {
            truncated: finish_reason == "length",
        });
    }
    let mut out = Vec::new();
    for (i, c) in calls.iter().enumerate() {
        let name = c
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(Json::as_str)
            .or_else(|| c.get("name").and_then(Json::as_str));
        if name != Some("bash") {
            return Err(FmtError { truncated: false });
        }
        let args = c
            .get("function")
            .and_then(|f| f.get("arguments"))
            .and_then(Json::as_str)
            .map(str::to_string)
            .or_else(|| c.get("arguments").map(|j| j.to_canonical_string()));
        let Some(args) = args else {
            return Err(FmtError { truncated: false });
        };
        let args = parse(&args).map_err(|_| FmtError { truncated: false })?;
        let command = args
            .get("command")
            .and_then(Json::as_str)
            .ok_or(FmtError { truncated: false })?;
        out.push(L1Action {
            command: command.to_string(),
            tool_call_id: c
                .get("id")
                .and_then(Json::as_str)
                .unwrap_or(&format!("call_{i}"))
                .to_string(),
        });
    }
    Ok(out)
}
