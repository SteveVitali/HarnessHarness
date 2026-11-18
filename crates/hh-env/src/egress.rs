//! The **egress mediator** — the Stage-2 runtime half of §5g.4's
//! `decide_egress` + credential mediation (R-2.8.3/R-2.8.4; S2.4;
//! ADR-0266 D1). The *decision* is `hharness_containment::egress::decide_egress`
//! (pure); this module owns the runtime: attribution-token resolution
//! (`TokenResolver`), the durable `security.egress.{requested,decided}` rows,
//! the decide-once-consume-once re-resolution ([`EgressResolver`] +
//! `recheck_resolved` — the SSRF pivot check), the credential-binding slot
//! (`CredentialBroker::mediate_sentinel` → `drain_claim`), the
//! `network.calls` charge ([`hharness_budget::Account`]), and the ask/endorse
//! path (`resolve_ask` + `amend` + `ApprovalCache`).
//!
//! Ordering contract (SV-5 + §5g.4 §6):
//!
//! 1. `security.egress.requested` is durable **before** any decision is
//!    computed — an unrecorded request never reaches the wire.
//! 2. `security.egress.decided` is durable **before** the outcome is
//!    visible — a denial is durable before the refusal surfaces; an allow
//!    is durable before the wire fires.
//! 3. A denied request never resolves, never mediates, never forwards.
//! 4. `network.calls` charges on the *forwarded* call only, attributed to
//!    the decided row's `effect_id` — the producing event is the decided
//!    row itself.
//! 5. Every refusal is fail-closed: ledger failure, broker failure,
//!    resolver failure and budget exhaustion all refuse; nothing degrades.
//!
//! The mediator is a kernel component — it never runs inside an
//! environment, and the `mh_secret:` sentinels it substitutes are drained
//! consume-once (the value crosses exactly one wire, kernel-side only).

use std::collections::BTreeSet;
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use hharness_budget::account::{Account, AskOutcome, ChargeRequest};
use hharness_budget::attribution::{default_charged_to, Attribution};
use hharness_budget::quantity::ResourceQuantity;
use hharness_containment::amend::{amend, AmendError};
use hharness_containment::egress::{
    decide_egress, recheck_resolved, ApprovalCache, CacheScope, EgressDecision, EgressReason,
    EgressRequest, EgressSource, EgressVerdict,
};
use hharness_containment::events::{
    amended_payload, egress_decided_payload, egress_requested_payload, request_ref, DecidedBy,
};
use hharness_containment::policy::{AmendmentBasis, ContainmentPolicy};
use hharness_ledger::event::Event;
use hharness_ledger::manifest::EventRef;
use hharness_ledger::store::{Lease, Store};
use hharness_monitor::approval::{self, ApprovalRequest, ApprovalResponse, ResponseChoice};
use hharness_ontology::dimensions::DimensionId;
use hharness_provenance::flow::Remedy;
use hharness_provenance::origin::Origin;
use hharness_provenance::{PersistenceScope, ProvenanceRecord};
use hharness_secrets::{
    CredentialBroker, MediationOutcome, Placeholder, RefusedCode, RequestDescriptor,
};
use hharness_wire::json::Json;

use crate::events::{EventMinter, ScopeChain};
use crate::tokens::{ResolveOutcome, TokenResolver};

/// The kernel component tag for the mediator's audit rows.
pub const COMPONENT: &str = "hh-env.egress";

// ── the resolver / transport seams ────────────────────────────────────────────

/// The mediator's DNS seam (ADR-0061 D3 — resolution is *part of the egress
/// path*): the caller's claimed `resolved_addrs` are never trusted; the
/// mediator resolves `host_norm` itself and `recheck_resolved` re-checks
/// every fresh answer against the deny rules + the non-public guard.
pub trait EgressResolver {
    /// Resolve `host_norm` to addresses. An error refuses closed.
    fn resolve(&self, host_norm: &str) -> Result<Vec<IpAddr>, String>;
}

/// The system resolver — `ToSocketAddrs` (an IP literal short-circuits; a
/// name uses the platform resolver — for the hermetic S2.4 fixtures this
/// resolves `localhost`/literals only; real external DNS is the ticket's
/// deferred edge).
pub struct SystemResolver;

impl EgressResolver for SystemResolver {
    fn resolve(&self, host_norm: &str) -> Result<Vec<IpAddr>, String> {
        if let Ok(ip) = host_norm.parse::<IpAddr>() {
            return Ok(vec![ip]);
        }
        let mut out = Vec::new();
        for sa in (host_norm, 0u16)
            .to_socket_addrs()
            .map_err(|e| format!("resolve {host_norm}: {e}"))?
        {
            if !out.contains(&sa.ip()) {
                out.push(sa.ip());
            }
        }
        Ok(out)
    }
}

/// A hermetic resolver — an explicit `host → addrs` table (the test fixtures;
/// an unknown host resolves to nothing, which the caller treats as
/// unroutable).
pub struct StaticResolver {
    /// The table.
    pub map: std::collections::BTreeMap<String, Vec<IpAddr>>,
}

impl EgressResolver for StaticResolver {
    fn resolve(&self, host_norm: &str) -> Result<Vec<IpAddr>, String> {
        if let Ok(ip) = host_norm.parse::<IpAddr>() {
            return Ok(vec![ip]);
        }
        Ok(self.map.get(host_norm).cloned().unwrap_or_default())
    }
}

/// The wire response the transport returns (the mediator records only the
/// status line — bodies stay out of the ledger).
#[derive(Debug, Clone, PartialEq)]
pub struct WireResponse {
    /// The HTTP status code.
    pub status: u16,
    /// The response headers.
    pub headers: Vec<(String, String)>,
    /// The response body.
    pub body: Vec<u8>,
}

/// `ForwardError` — the transport leg's typed failure (R6.45k; D12): a
/// declared-bound lapse is `Timeout` — the mediator answers a typed **504**
/// (`Gateway Timeout`), never a 502; everything else is `Failed` (502).
#[derive(Debug, Clone, PartialEq)]
pub enum ForwardError {
    /// A declared timeout bound lapsed (connect/first-byte/read).
    Timeout {
        /// What lapsed and where.
        detail: String,
    },
    /// The leg failed for a non-timeout reason.
    Failed {
        /// What failed.
        detail: String,
    },
}

impl ForwardError {
    /// `Timeout` with context.
    pub fn timeout(detail: impl Into<String>) -> Self {
        ForwardError::Timeout {
            detail: detail.into(),
        }
    }
    /// `Failed` with context.
    pub fn failed(detail: impl Into<String>) -> Self {
        ForwardError::Failed {
            detail: detail.into(),
        }
    }
    /// The detail string.
    pub fn detail(&self) -> &str {
        match self {
            ForwardError::Timeout { detail } | ForwardError::Failed { detail } => detail,
        }
    }
}

impl std::fmt::Display for ForwardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForwardError::Timeout { detail } => write!(f, "timeout: {detail}"),
            ForwardError::Failed { detail } => write!(f, "{detail}"),
        }
    }
}

/// `ForwardHead` — the upstream response head plus its live body source
/// (R6.45k; D13): [`EgressTransport::forward_head`] hands the caller the
/// head immediately and a [`BodySource`] it drains chunk-wise — bounded
/// resident memory independent of the body's size.
pub struct ForwardHead {
    /// The upstream head (status + headers, hop-by-hop members included —
    /// the caller applies the response-hygiene filter before re-emitting).
    pub head: hharness_wire::http::ResponseHead,
    /// The live body (None ⇒ nothing follows the head).
    pub body: Option<Box<dyn BodySource>>,
}

/// `BodySource` — a streaming response body: successive chunks until
/// `Ok(None)`.
pub trait BodySource {
    /// The next body segment; `Ok(None)` is the terminal boundary.
    fn next_chunk(&mut self) -> std::io::Result<Option<Vec<u8>>>;
}

/// A `BodySource` over an already-buffered body (the `forward_head`
/// default — one chunk, then terminal).
struct BufferedBody {
    rest: Option<Vec<u8>>,
}
impl BodySource for BufferedBody {
    fn next_chunk(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        Ok(self.rest.take())
    }
}

/// The transport seam — the *only* wire the mediator writes (mediated envs
/// have no other network path). Implementations dial a **resolved address**
/// (the consume-once answer), never re-resolve the name themselves.
pub trait EgressTransport {
    /// Forward the (credential-substituted) request to `addr:port`.
    #[allow(clippy::too_many_arguments)] // the arity is the wire request's.
    fn forward(
        &self,
        addr: IpAddr,
        port: u16,
        host: &str,
        method: &str,
        path: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Result<WireResponse, ForwardError>;

    /// `forward_head(…)` — the streaming half of [`Self::forward`]
    /// (R6.45k; D13): the head as soon as it parses, then a [`BodySource`]
    /// the caller drains — the buffered `forward` stays the answering API;
    /// the default synthesizes the source from it (one buffered chunk).
    #[allow(clippy::too_many_arguments)] // the arity is the wire request's.
    fn forward_head(
        &self,
        addr: IpAddr,
        port: u16,
        host: &str,
        method: &str,
        path: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Result<ForwardHead, ForwardError> {
        let r = self.forward(addr, port, host, method, path, headers, body)?;
        Ok(ForwardHead {
            head: hharness_wire::http::ResponseHead {
                status: r.status,
                headers: r
                    .headers
                    .iter()
                    .map(|(k, v)| (k.to_ascii_lowercase(), v.clone()))
                    .collect(),
            },
            body: Some(Box::new(BufferedBody { rest: Some(r.body) })),
        })
    }

    /// `dial(addr, port)` — open a byte stream to the *resolved* address
    /// for the CONNECT tunnel leg (R6.15; F11). The default refuses:
    /// a transport that cannot tunnel answers typed, never a silently
    /// plain forward. `LocalHttpTransport` dials `TcpStream` — the
    /// tunnelled bytes are `tls_opaque` by declaration.
    fn dial(&self, addr: IpAddr, port: u16) -> Result<Box<dyn ByteStream>, ForwardError> {
        let _ = (addr, port);
        Err(ForwardError::failed("tunnel unsupported on this transport"))
    }

    /// Whether the transport terminates TLS at the mediator (a
    /// `tls.terminate`-declared policy's relied-on capability — R2.9b;
    /// ADR-0341 D3). Default `false`: the reference `LocalHttpTransport`
    /// terminates nothing (the real terminating transport is BL-31's
    /// surface). A policy that requires termination on a TLS leg refuses
    /// closed against a `false` answer — never silently unterminated.
    fn terminates_tls(&self) -> bool {
        false
    }
}

/// `ByteStream` — the duplex byte pipe `dial` returns (Read + Write +
/// Send so the tunnel legs can run on pump threads).
pub trait ByteStream: std::io::Read + std::io::Write + Send {
    /// A second handle on the same stream (the pump threads' two ends).
    fn try_clone(&self) -> std::io::Result<Box<dyn ByteStream>>;
    /// Half-close the stream (R6.45k; D9 — the relay's pattern): one
    /// direction's EOF propagates as `shutdown(Write)` on the peer, so the
    /// other direction sees the close instead of hanging on a live socket.
    fn shutdown(&self, how: std::net::Shutdown) -> std::io::Result<()>;
}
impl ByteStream for std::net::TcpStream {
    fn try_clone(&self) -> std::io::Result<Box<dyn ByteStream>> {
        std::net::TcpStream::try_clone(self).map(|s| {
            let b: Box<dyn ByteStream> = Box::new(s);
            b
        })
    }
    fn shutdown(&self, how: std::net::Shutdown) -> std::io::Result<()> {
        std::net::TcpStream::shutdown(self, how)
    }
}
#[cfg(unix)]
impl ByteStream for std::os::unix::net::UnixStream {
    fn try_clone(&self) -> std::io::Result<Box<dyn ByteStream>> {
        std::os::unix::net::UnixStream::try_clone(self).map(|s| {
            let b: Box<dyn ByteStream> = Box::new(s);
            b
        })
    }
    fn shutdown(&self, how: std::net::Shutdown) -> std::io::Result<()> {
        std::os::unix::net::UnixStream::shutdown(self, how)
    }
}

/// The hop-by-hop and authority headers a forwarding transport must never
/// relay verbatim (RFC 7230 §6.1 + §5.4): `Host`/`Content-Length` are the
/// transport's own (it writes both — a verbatim client copy produces the
/// duplicate-`Host` request every origin must 400, the Wc5/Wc6 H-allow
/// bootstrap failure), `Connection` and its named tokens ride the
/// client→mediator hop only, and `Proxy-*` members are for the proxy hop,
/// never the origin.
const HOP_BY_HOP_HEADERS: [&str; 11] = [
    "host",
    "connection",
    "content-length",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

/// `TunnelCause` — how one tunnel-pump leg ended (D9): the close row's
/// `close_cause` is computed from the two legs' real outcomes, never
/// hard-coded.
#[derive(Debug)]
enum TunnelCause {
    /// Clean EOF (or an already-closed write after the peer's half-close).
    Eof,
    /// The declared `TUNNEL_IDLE_TIMEOUT` fired on an idle leg.
    IdleTimeout,
    /// An io error ended the leg — the `error:<kind>` spelling.
    Error(String),
}

/// `TunnelEnd` — one pump leg's result: bytes actually moved plus the
/// true end cause (no `unwrap_or(0)` — an erroring leg still reports the
/// bytes it moved).
struct TunnelEnd {
    bytes: u64,
    cause: TunnelCause,
}

/// `tunnel_pump(r, w)` — copy `r → w` until EOF or error, counting real
/// bytes (the 5 s-read-kill used to truncate the downstream count to 0).
fn tunnel_pump(r: &mut dyn std::io::Read, w: &mut dyn std::io::Write) -> TunnelEnd {
    let mut total = 0u64;
    let mut buf = [0u8; 16384];
    loop {
        match r.read(&mut buf) {
            Ok(0) => {
                return TunnelEnd {
                    bytes: total,
                    cause: TunnelCause::Eof,
                }
            }
            Ok(n) => {
                if let Err(e) = w.write_all(&buf[..n]) {
                    // A write to an already-closed peer is the peer's EOF,
                    // not a transport error.
                    let cause = match e.kind() {
                        std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted => TunnelCause::Eof,
                        _ => TunnelCause::Error(e.kind().to_string()),
                    };
                    return TunnelEnd {
                        bytes: total,
                        cause,
                    };
                }
                total += n as u64;
            }
            Err(e) => {
                let cause = match e.kind() {
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
                        TunnelCause::IdleTimeout
                    }
                    // A read on a socket the peer already closed surfaces
                    // as reset — that IS the peer's EOF.
                    std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::UnexpectedEof => TunnelCause::Eof,
                    _ => TunnelCause::Error(e.kind().to_string()),
                };
                return TunnelEnd {
                    bytes: total,
                    cause,
                };
            }
        }
    }
}

/// `tunnel_close_cause(up, down)` — the `security.egress.tunnel.closed`
/// `close_cause` member: any real error wins over an idle-timeout, which
/// wins over a clean `eof` (`eof|idle_timeout|error:<kind>`).
fn tunnel_close_cause(up: &TunnelEnd, down: &TunnelEnd) -> String {
    for e in [&up.cause, &down.cause] {
        if let TunnelCause::Error(kind) = e {
            return format!("error:{kind}");
        }
    }
    for e in [&up.cause, &down.cause] {
        if matches!(e, TunnelCause::IdleTimeout) {
            return "idle_timeout".to_string();
        }
    }
    "eof".to_string()
}

/// `push_end_to_end_headers(out, client)` — append only the headers the
/// origin may see: every client member outside the closed hop-by-hop set
/// (case-insensitive — `HOST` spells the same header).
fn push_end_to_end_headers<'a>(out: &mut Vec<(&'a str, &'a str)>, client: &'a [(String, String)]) {
    for (k, v) in client {
        if !HOP_BY_HOP_HEADERS.contains(&k.to_ascii_lowercase().as_str()) {
            out.push((k.as_str(), v.as_str()));
        }
    }
}

/// The hop-by-hop **response** headers the mediator strips before
/// re-emitting upstream's answer (R6.45k; D10 — RFC 9110 §7.6.1): the
/// transport's own framing is re-declared below, never relayed — a
/// verbatim upstream `Content-Length` duplicates the emitted one and a
/// relayed `Transfer-Encoding: chunked` makes the client mis-parse an
/// already-de-chunked body.
const RESPONSE_HOP_BY_HOP: [&str; 7] = [
    "connection",
    "keep-alive",
    "content-length",
    "transfer-encoding",
    "trailer",
    "upgrade",
    "proxy-authenticate",
];

/// `end_to_end_response_headers(upstream)` — the upstream response headers
/// the client may see: every member outside the closed response hop-by-hop
/// set, every `proxy-*` member, and every header `Connection:` names.
/// Case-insensitive on the names; casing preserved on emit.
pub fn end_to_end_response_headers(upstream: &[(String, String)]) -> Vec<(String, String)> {
    // A `Connection: <token>` header names further hop-by-hop members.
    let mut named: Vec<String> = Vec::new();
    for (k, v) in upstream {
        if k.eq_ignore_ascii_case("connection") {
            named.extend(
                v.split(',')
                    .map(|t| t.trim().to_ascii_lowercase())
                    .filter(|t| !t.is_empty()),
            );
        }
    }
    upstream
        .iter()
        .filter(|(k, _)| {
            let n = k.to_ascii_lowercase();
            !RESPONSE_HOP_BY_HOP.contains(&n.as_str())
                && !n.starts_with("proxy-")
                && !named.iter().any(|t| t == &n)
        })
        .cloned()
        .collect()
}

/// `FORWARD_MAX_BODY` — the most one forward's response body may buffer
/// (bytes). The forward answers one complete [`WireResponse`]: its `body`
/// is a `Vec<u8>` the mediator re-emits whole under `Content-Length`, so
/// the bound caps one call's resident memory — it is a buffering bound,
/// not a policy limit, and it never authorizes a deny. Sized for real
/// package indexes and payloads: Ubuntu noble's `Packages.xz`/`Sources.xz`
/// indexes run ~15–30 MB, and the pre-repair 8 MiB literal (R4.5c,
/// `a7292296`) rejected `noble/universe`'s index mid-read →
/// `EgressError::Transport` → the mediator's `502` → `apt-get update`
/// dead under mediation (the Wc7 seventh defect —
/// `verifier_unavailable:bootstrap:signature` with `denied_hosts: []`,
/// every request *allowed*). 128 MiB leaves ~4–8× headroom over the
/// largest archive indexes and covers ordinary `.deb` payloads; a
/// genuinely larger artifact still answers the same typed refusal.
pub const FORWARD_MAX_BODY: usize = 128 * 1024 * 1024;

// ── the declared timeout bounds (R6.45k; D9/D12) ────────────────────────────

/// `CONNECT_TIMEOUT` — the dial bound on the forward and tunnel legs
/// (D12: connect ≤ 10 s).
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// `FORWARD_IO_TIMEOUT` — the first-byte/read/write bound on the forward
/// leg (D12: ≥ 60 s — a slow mirror or a generating index gets a full
/// minute before the typed 504).
pub const FORWARD_IO_TIMEOUT: Duration = Duration::from_secs(60);
/// `TUNNEL_IDLE_TIMEOUT` — the declared idle bound on an established
/// CONNECT tunnel leg (D9: no silent 5 s kill; ≥ 300 s, recorded — a leg
/// that idles a full ten minutes answers `close_cause idle_timeout`,
/// never a swallowed `eof`). Well above curl's observed ~300 s CONNECT
/// patience so a real client's own timeout fires first.
pub const TUNNEL_IDLE_TIMEOUT: Duration = Duration::from_secs(600);

/// `host_header(host, port)` — the `Host` authority for the wire (D14):
/// the default port is omitted (`example.com`, never `example.com:80` /
/// `:443`); a non-default port is always spelled.
pub fn host_header(host: &str, port: u16) -> String {
    if port == 80 || port == 443 {
        host.to_string()
    } else {
        format!("{host}:{port}")
    }
}

/// `io_fault(what, e)` — classify an io failure on the wire leg: deadline
/// kinds are the typed [`ForwardError::Timeout`] (the mediator's 504),
/// everything else `Failed`.
fn io_fault(what: String, e: std::io::Error) -> ForwardError {
    match e.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
            ForwardError::timeout(format!("{what}: {e}"))
        }
        _ => ForwardError::failed(format!("{what}: {e}")),
    }
}

/// `http_fault(what, e)` — the codec error map: `HttpError::Timeout` is
/// the typed lapse, everything else `Failed`.
fn http_fault(what: String, e: hharness_wire::http::HttpError) -> ForwardError {
    match e {
        hharness_wire::http::HttpError::Timeout => {
            ForwardError::timeout(format!("{what}: deadline fired"))
        }
        other => ForwardError::failed(format!("{what}: {other}")),
    }
}

/// The local HTTP/1.1 transport — a plain `TcpStream` to the resolved
/// address (hermetic: localhost fixtures; no TLS — TLS-terminating
/// inspection is a deferred row, `tls.terminate` is always `false` at this
/// stage, so mediated traffic is plain HTTP to a checked address).
pub struct LocalHttpTransport {
    /// The forward leg's first-byte/read/write bound (default
    /// [`FORWARD_IO_TIMEOUT`]; the tunnel leg's declared idle bound is
    /// [`TUNNEL_IDLE_TIMEOUT`]).
    pub timeout: Duration,
    /// The dial bound (default [`CONNECT_TIMEOUT`]).
    pub connect_timeout: Duration,
}

impl Default for LocalHttpTransport {
    fn default() -> Self {
        LocalHttpTransport {
            timeout: FORWARD_IO_TIMEOUT,
            connect_timeout: CONNECT_TIMEOUT,
        }
    }
}

/// The streaming upstream body [`LocalHttpTransport::forward_head`] hands
/// back — the codec's [`BodyReader`](hharness_wire::http::BodyReader) owning the buffered wire stream
/// (bounded resident memory: one codec segment at a time — D13).
impl BodySource for hharness_wire::http::BodyReader<std::io::BufReader<Box<dyn ByteStream>>> {
    fn next_chunk(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        hharness_wire::http::BodyReader::next_chunk(self).map_err(|e| match e {
            hharness_wire::http::HttpError::Eof => {
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "upstream eof mid-body")
            }
            hharness_wire::http::HttpError::Timeout => {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "upstream read deadline")
            }
            other => std::io::Error::other(other.to_string()),
        })
    }
}

impl EgressTransport for LocalHttpTransport {
    fn dial(&self, addr: IpAddr, port: u16) -> Result<Box<dyn ByteStream>, ForwardError> {
        // The tunnel leg: connect under CONNECT_TIMEOUT, then the
        // declared TUNNEL_IDLE_TIMEOUT on the established legs — never
        // the forward path's first-byte bound (D9: a 5 s silence used to
        // kill the downstream pump while the client hung 300 s).
        let sa = std::net::SocketAddr::new(addr, port);
        let s = TcpStream::connect_timeout(&sa, self.connect_timeout)
            .map_err(|e| io_fault(format!("connect {addr}:{port}"), e))?;
        s.set_read_timeout(Some(TUNNEL_IDLE_TIMEOUT)).ok();
        s.set_write_timeout(Some(TUNNEL_IDLE_TIMEOUT)).ok();
        Ok(Box::new(s))
    }

    fn forward(
        &self,
        addr: IpAddr,
        port: u16,
        host: &str,
        method: &str,
        path: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Result<WireResponse, ForwardError> {
        // R4.5b (ADR-0364 D1) — the wire shape is `hharness_wire::http`'s one
        // codec: request write + head parse + body per the declared mode
        // (`Content-Length`/`chunked`/close-delimited — the fixtures'
        // `Connection: close` answer reads the same). The dial stays here:
        // the mediator dials a *resolved* address, never a name.
        let sa = std::net::SocketAddr::new(addr, port);
        let mut s = TcpStream::connect_timeout(&sa, self.connect_timeout)
            .map_err(|e| io_fault(format!("connect {addr}:{port}"), e))?;
        s.set_read_timeout(Some(self.timeout)).ok();
        s.set_write_timeout(Some(self.timeout)).ok();
        let host_header = host_header(host, port);
        let mut hdrs: Vec<(&str, &str)> = Vec::with_capacity(headers.len() + 2);
        hdrs.push(("Host", &host_header));
        hdrs.push(("Connection", "close"));
        push_end_to_end_headers(&mut hdrs, headers);
        hharness_wire::http::write_request(&mut s, method, path, &hdrs, body.unwrap_or(&[]))
            .map_err(|e| http_fault(format!("write {addr}:{port}"), e))?;
        let mut r = std::io::BufReader::new(s);
        let (head, body) =
            hharness_wire::http::read_response_full_for(Some(method), &mut r, FORWARD_MAX_BODY)
                .map_err(|e| http_fault(format!("read {addr}:{port}"), e))?;
        Ok(WireResponse {
            status: head.status,
            headers: head
                .headers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            body,
        })
    }

    fn forward_head(
        &self,
        addr: IpAddr,
        port: u16,
        host: &str,
        method: &str,
        path: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Result<ForwardHead, ForwardError> {
        // D13 — the streaming path: the same request shape as `forward`,
        // but the head parses first and the body streams through a
        // [`BodyReader`]-backed source — no `FORWARD_MAX_BODY` buffering
        // bound on this leg (bounded resident memory is the guard).
        let sa = std::net::SocketAddr::new(addr, port);
        let mut s = TcpStream::connect_timeout(&sa, self.connect_timeout)
            .map_err(|e| io_fault(format!("connect {addr}:{port}"), e))?;
        s.set_read_timeout(Some(self.timeout)).ok();
        s.set_write_timeout(Some(self.timeout)).ok();
        let host_header = host_header(host, port);
        let mut hdrs: Vec<(&str, &str)> = Vec::with_capacity(headers.len() + 2);
        hdrs.push(("Host", &host_header));
        hdrs.push(("Connection", "close"));
        push_end_to_end_headers(&mut hdrs, headers);
        hharness_wire::http::write_request(&mut s, method, path, &hdrs, body.unwrap_or(&[]))
            .map_err(|e| http_fault(format!("write {addr}:{port}"), e))?;
        let mut r = std::io::BufReader::new(Box::new(s) as Box<dyn ByteStream>);
        let head = hharness_wire::http::read_response_head(&mut r)
            .map_err(|e| http_fault(format!("read {addr}:{port}"), e))?;
        if hharness_wire::http::bodiless(Some(method), head.status) {
            return Ok(ForwardHead { head, body: None });
        }
        let body = hharness_wire::http::BodyReader::new_for(r, &head, Some(method), usize::MAX);
        Ok(ForwardHead {
            head,
            body: Some(Box::new(body)),
        })
    }
}

/// The local TLS-terminated transport (R4.5c; ADR-0365; DF-S2.4-2):
/// `hharness_tls::ClientTls` wraps the resolved-address `TcpStream`, then the
/// **same** `hharness_wire::http` codec carries the request. `terminates_tls()`
/// answers `true` — the policy's `tls.terminate` declaration is honoured
/// with a real session (webPKI verification always on; a private issuer
/// needs the operator-declared extra-roots file), and `tls_terminated`
/// decided-rows report the truth. A failed handshake is a typed forward
/// refusal — never a silently-plain leg.
pub struct LocalTlsTransport {
    /// The client TLS configuration (`ClientTls::webpki_roots()` plus
    /// any declared extra roots / mTLS client identity).
    pub tls: hharness_tls::ClientTls,
    /// The dial/handshake/read bound (default [`FORWARD_IO_TIMEOUT`]).
    pub timeout: Duration,
    /// The dial bound (default [`CONNECT_TIMEOUT`]).
    pub connect_timeout: Duration,
}

impl Default for LocalTlsTransport {
    fn default() -> Self {
        LocalTlsTransport {
            tls: hharness_tls::ClientTls::webpki_roots(),
            timeout: FORWARD_IO_TIMEOUT,
            connect_timeout: CONNECT_TIMEOUT,
        }
    }
}

impl EgressTransport for LocalTlsTransport {
    fn forward(
        &self,
        addr: IpAddr,
        port: u16,
        host: &str,
        method: &str,
        path: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Result<WireResponse, ForwardError> {
        let sa = std::net::SocketAddr::new(addr, port);
        let s = TcpStream::connect_timeout(&sa, self.connect_timeout)
            .map_err(|e| io_fault(format!("connect {addr}:{port}"), e))?;
        s.set_read_timeout(Some(self.timeout)).ok();
        s.set_write_timeout(Some(self.timeout)).ok();
        // The handshake terminates at the mediator — verification is the
        // configured root set's, never skipped.
        let mut s = self
            .tls
            .connect(s, host)
            .map_err(|e| ForwardError::failed(format!("tls {addr}:{port}: {e}")))?;
        let host_header = host_header(host, port);
        let mut hdrs: Vec<(&str, &str)> = Vec::with_capacity(headers.len() + 2);
        hdrs.push(("Host", &host_header));
        hdrs.push(("Connection", "close"));
        push_end_to_end_headers(&mut hdrs, headers);
        hharness_wire::http::write_request(&mut s, method, path, &hdrs, body.unwrap_or(&[]))
            .map_err(|e| http_fault(format!("write {addr}:{port}"), e))?;
        let mut r = std::io::BufReader::new(s);
        let (head, body) =
            hharness_wire::http::read_response_full_for(Some(method), &mut r, FORWARD_MAX_BODY)
                .map_err(|e| http_fault(format!("read {addr}:{port}"), e))?;
        Ok(WireResponse {
            status: head.status,
            headers: head
                .headers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            body,
        })
    }

    fn terminates_tls(&self) -> bool {
        true
    }
}

// ── the inspect-hook seam (R2.9b; ADR-0341 D3/D4) ─────────────────────────────

/// `InspectView` — the request view a declared `inspect_hooks` member
/// receives: the wire coordinates plus the *sentinel-spelling* content
/// (headers/body before claim drain — credential **values** never reach a
/// hook; the broker's material stays behind the substitution, ADR-0341 D4).
/// A `validator`-class plugin contribution (ADR-0181; OQ-154's resolved
/// half) — the hook is a mechanism the policy names, never an enforcement
/// point of its own (§5g.4 §2.2: hooks are inputs, enforcement stays at
/// EP3).
#[derive(Debug)]
pub struct InspectView<'a> {
    /// The normalised host.
    pub host_norm: &'a str,
    /// The port.
    pub port: u16,
    /// The protocol spelling (`http` / `https_connect`).
    pub protocol: &'a str,
    /// The method.
    pub method: &'a str,
    /// The path.
    pub path: Option<&'a str>,
    /// The headers (sentinel spellings, never drained values).
    pub headers: &'a [(String, String)],
    /// The body (sentinel spellings, never drained values).
    pub body: Option<&'a str>,
}

/// The hook verdict — a closed sum: `allow` passes the request onward,
/// `deny` refuses it (`decided{deny, source: inspect_hook, reason:
/// inspect_hook_denied, guard_detail: <ref>}` is durable before visible).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectVerdict {
    /// The request may proceed.
    Allow,
    /// The request is refused.
    Deny,
}

/// `InspectHook` — the runtime seam `net.tls.inspect_hooks` resolves
/// through. One registered hook per declared ref; a declared ref with no
/// registration refuses `inspect_hook_unavailable` (the declared mechanism
/// cannot be honoured — enforce-or-refuse, I-C4's shape).
pub trait InspectHook {
    /// Inspect the request view; return the verdict.
    fn inspect(&self, view: &InspectView) -> InspectVerdict;
}

// ── outcomes / errors ─────────────────────────────────────────────────────────

/// What a mediated request resolved to.
#[derive(Debug, Clone)]
pub enum MediatedOutcome {
    /// The request was forwarded — `decided{allow}` is durable, the
    /// `network.calls` charge is appended, the wire response is here.
    Forwarded {
        /// The `decision_request` ref.
        request_ref: String,
        /// The `security.egress.decided` event id.
        decided_ref: String,
        /// The `budget.charge.appended` source (the decided event).
        charge_source: String,
        /// The wire response.
        response: WireResponse,
    },
    /// A CONNECT tunnel ran — `decided{allow}` durable before the
    /// first byte; `security.egress.tunnel.closed` carries the byte
    /// fold (R6.15; F11).
    Tunneled {
        /// The `decision_request` ref.
        request_ref: String,
        /// The `security.egress.decided` event id.
        decided_ref: String,
        /// The `security.egress.tunnel.closed` event id.
        closed_ref: String,
        /// Bytes client → upstream.
        bytes_up: u64,
        /// Bytes upstream → client.
        bytes_down: u64,
    },
    /// Refused by policy — `decided{deny}` is durable before this is
    /// visible.
    Refused {
        /// The `decision_request` ref.
        request_ref: String,
        /// The `security.egress.decided` event id.
        decided_ref: String,
        /// The closed deny reason.
        reason: EgressReason,
    },
    /// The egress decision was `allow` but a credential sentinel refused
    /// (its own `security.credential.denied` row is durable; the decided
    /// row spells `deny{source: monitor, reason: denied}` — the precise
    /// refusal code lives on the credential row, keyed by the shared
    /// `effect_id`; ADR-0266 D3).
    RefusedCredential {
        /// The `decision_request` ref.
        request_ref: String,
        /// The `security.egress.decided` event id.
        decided_ref: String,
        /// The credential refusal code.
        code: RefusedCode,
    },
    /// `default_unmatched = ask` — the pending/requested rows and the
    /// `decided{decision: ask}` row are durable; the operator's
    /// `ApprovalResponse` resolves through [`EgressMediator::endorse_asked`].
    Asked {
        /// The `decision_request` ref.
        request_ref: String,
        /// The `security.egress.decided` event id.
        decided_ref: String,
        /// The pending `permission_id` (the ask's coordinate).
        permission_id: String,
    },
}

/// `GateOutcome` — the result of [`EgressMediator::gate`] (DF-S2.4-1): the
/// pre-wire half of the mediated path.
#[derive(Debug)]
pub enum GateOutcome {
    /// `decide_egress` allowed — the caller owes [`EgressMediator::forward`]
    /// at the wire point (the recheck/sentinel/`decided{allow}`/wire/charge
    /// leg). Dispatch carries this across `committed` so the write-ahead
    /// precedes the wire.
    Allowed(Box<GateAllow>),
    /// The request terminated at the gate — `decided{deny}` or the ask
    /// trail (`pending`/`requested`/`decided{ask}`) is durable.
    Terminal(MediatedOutcome),
}

/// The carried gate state for an allowed request — the decision record and
/// the `started` stamp `forward`'s `latency_ms` reads.
#[derive(Debug)]
pub struct GateAllow {
    /// The pure decision (`decision = allow`).
    pub decision: hharness_containment::egress::EgressDecision,
    /// The decided row's effect scope (the token's resolved binding, else
    /// the claim).
    pub effect_id: String,
    /// The gate's start stamp (`decided.latency_ms` baseline).
    pub started: u64,
    /// Who decided — `policy` on the direct `gate` path, `monitor` on an
    /// operator endorsement (R2.9a; the `decided` row `forward` mints
    /// attributes the verdict correctly).
    pub decided_by: DecidedBy,
    /// The policy a successful `amend(AddEgressAllow)` produced
    /// (`endorse_gate`'s `allow_lease` leg) — the caller installs it on the
    /// environment handle so the *next* dispatch's `decide_egress` sees the
    /// widened allow set and the version-pinned cache stays honest
    /// (`security.containment.amended` is already durable).
    pub amended_policy: Option<ContainmentPolicy>,
}

/// `PreWire` — the shared pre-decided legs' result (R6.15; F11):
/// either a refusal already durable on the ledger, or the gate-through-
/// `decided{allow}` material the wire leg consumes.
#[derive(Debug)]
enum PreWire {
    /// A guard/budget/credential leg refused — the `decided{deny}` row is
    /// durable; hand the outcome straight back.
    Refused(MediatedOutcome),
    /// `decided{allow}` landed — the caller's wire leg runs now.
    Ready {
        /// The mediator's fresh resolution (consume-once).
        fresh: Vec<IpAddr>,
        /// `(sentinel, claim_id)` pairs staged by the broker.
        staged: Vec<(String, String)>,
        /// The `security.egress.decided` event id.
        decided_ref: String,
    },
}

/// The mediator's failure modes — typed, never a warning.
#[derive(Debug)]
pub enum EgressError {
    /// A ledger append failed — fail closed, nothing proceeds.
    Ledger {
        /// What failed.
        detail: String,
    },
    /// The budget op failed.
    Budget {
        /// What failed.
        detail: String,
    },
    /// The mediator's own resolution failed (the wire never fired —
    /// `decided{allow}` stands, no charge was made).
    Resolve {
        /// What failed.
        detail: String,
    },
    /// The transport failed (the wire errored after `decided{allow}`).
    Transport {
        /// What failed.
        detail: String,
    },
    /// An amendment refusal (`endorse_asked`).
    Amend(AmendError),
    /// The broker refused/denied outside the `MediationOutcome` path.
    Broker {
        /// What failed.
        detail: String,
    },
    /// A declared timeout bound lapsed on the wire leg (R6.45k; D12) —
    /// the mediator answers a typed **504**, never a 502.
    Timeout {
        /// What lapsed and where.
        detail: String,
    },
}

// ── the mediator ─────────────────────────────────────────────────────────────

/// `StoreAccess` — the mediator's ledger handle (R6.45k; D8): `&mut
/// Store` for the in-process callers, `&Mutex<Store>` for the concurrent
/// mediator binary. The mediator touches the store only inside
/// `with_store`/`read_store` — the lock is held across each mint+append
/// (the durable leg: one hash-chained parent at a time) and never across
/// a wire leg (byte pumping / the upstream exchange run unlocked).
pub trait StoreAccess {
    /// Run `f` with exclusive `&mut Store` access.
    fn with_store<R>(&mut self, f: impl FnOnce(&mut Store) -> R) -> R;
    /// Run `f` with shared `&Store` access.
    fn read_store<R>(&self, f: impl FnOnce(&Store) -> R) -> R;
}

impl StoreAccess for &mut Store {
    fn with_store<R>(&mut self, f: impl FnOnce(&mut Store) -> R) -> R {
        f(self)
    }
    fn read_store<R>(&self, f: impl FnOnce(&Store) -> R) -> R {
        f(self)
    }
}

impl StoreAccess for &std::sync::Mutex<Store> {
    fn with_store<R>(&mut self, f: impl FnOnce(&mut Store) -> R) -> R {
        // Poison recovery: a panicked conn thread leaves the ledger
        // consistent (each append is one critical section) — the lock is
        // recovered, never propagated as a second failure.
        let mut g = self.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut g)
    }
    fn read_store<R>(&self, f: impl FnOnce(&Store) -> R) -> R {
        let g = self.lock().unwrap_or_else(|p| p.into_inner());
        f(&g)
    }
}

/// `EgressMediator` — the kernel-side runtime composing the pure decision
/// (`decide_egress`), the broker, the budget and the wire. One per
/// mediated environment session; `policy`/`cache` evolve on `amend`.
/// `S` is the store handle — `&mut Store` in-process (the default),
/// `&Mutex<Store>` in the concurrent mediator binary (D8).
pub struct EgressMediator<'a, S: StoreAccess = &'a mut Store> {
    /// The store (the writer lease's target).
    pub store: S,
    /// The run.
    pub run_id: String,
    /// The writer lease.
    pub lease: &'a Lease,
    /// The attribution-token resolver (resolve-only).
    pub tokens: TokenResolver,
    /// The credential broker (kernel-held).
    pub broker: &'a mut CredentialBroker,
    /// The effective containment policy (swapped on `amend`).
    pub policy: ContainmentPolicy,
    /// The narrowing-only approval cache (version-pinned).
    pub cache: ApprovalCache,
    /// The budget node `network.calls`/`approvals.requested` charge against.
    pub budget_id: Option<String>,
    /// The participant the charges attribute to.
    pub participant_ref: String,
    /// The resolver (the mediator's own DNS).
    pub resolver: Box<dyn EgressResolver>,
    /// The transport (the only wire).
    pub transport: Box<dyn EgressTransport>,
    /// The `inspect_hooks` registry — declared ref → hook (validator-class
    /// plugin contributions; an unregistered declared ref refuses closed,
    /// R2.9b).
    pub inspect_hooks: std::collections::BTreeMap<String, Box<dyn InspectHook>>,
}

impl<'a, S: StoreAccess> EgressMediator<'a, S> {
    /// Emit one audit event (durable before return — the ordering contract).
    /// Mint+append is one `with_store` critical section — under the
    /// concurrent mediator (D8) two threads can never mint the same
    /// hash-chain head.
    fn emit(
        &mut self,
        class: &str,
        payload: Json,
        effect_id: Option<&str>,
        chain: &ScopeChain,
    ) -> Result<Event, EgressError> {
        let run_id = self.run_id.clone();
        let lease = self.lease;
        self.store.with_store(|store| {
            let minter = EventMinter::new(&*store, &run_id);
            let ev = match effect_id {
                // An empty/None effect scope mints unscoped — an
                // unattributed request must still leave a durable row
                // (fail-closed audit).
                Some(e) if !e.is_empty() => minter.mint_effect(class, payload, e, chain),
                _ => minter.mint(class, payload),
            }
            .map_err(|e| EgressError::Ledger {
                detail: format!("mint {class}: {e}"),
            })?;
            store
                .append(&run_id, lease, vec![ev.clone()])
                .map_err(|e| EgressError::Ledger {
                    detail: format!("append {class}: {e}"),
                })?;
            // Durable before return — the ordering contract.
            Ok(ev)
        })
    }

    /// Collect the sentinel spellings — `req.credential_sentinels` plus every
    /// `mh_secret:`-prefixed header value and path/body occurrence.
    fn collect_sentinels(req: &EgressRequest) -> Vec<String> {
        let mut out: BTreeSet<String> = req.credential_sentinels.iter().cloned().collect();
        for (_, v) in &req.headers {
            for w in v.split(|c: char| c.is_whitespace() || c == ',') {
                if Placeholder::is_placeholder(w) {
                    out.insert(w.to_string());
                }
            }
        }
        for text in [req.path.as_deref(), req.body.as_deref()]
            .into_iter()
            .flatten()
        {
            for w in
                text.split(|c: char| !(c.is_alphanumeric() || c == ':' || c == '_' || c == '-'))
            {
                if Placeholder::is_placeholder(w) {
                    out.insert(w.to_string());
                }
            }
        }
        out.into_iter().collect()
    }

    /// `gate(request, chain)` — the pre-wire half of `handle` (DF-S2.4-1):
    /// `requested` → attribution → the pure `decide_egress` decision. Deny
    /// and ask terminate here (their `decided`/`pending`/`requested` rows are
    /// durable before return); an allow hands the caller a [`GateAllow`] —
    /// the recheck/sentinel/decided{allow}/wire/charge leg is deferred to
    /// [`EgressMediator::forward`], which the dispatch path invokes at the
    /// wire point (post-`committed`, pre-executor) so the effect's
    /// write-ahead still precedes the wire.
    pub fn gate(
        &mut self,
        req: &EgressRequest,
        chain: &ScopeChain,
    ) -> Result<GateOutcome, EgressError> {
        let started = self.store.read_store(|s| s.now_ms());
        let sentinel_refs = Self::collect_sentinels(req);

        // 1. `requested` is durable before the decision runs.
        self.emit(
            "security.egress.requested",
            egress_requested_payload(req, &sentinel_refs),
            req.effect_id.as_deref(),
            chain,
        )?;

        // 2. Attribution — the token's binding must resolve AND match the
        //    env handle AND the claimed effect (unattributed ⇒ deny).
        let (attributed, resolved_effect) = match self.tokens.resolve(&req.token) {
            ResolveOutcome::Resolved {
                effect_id,
                env_handle_id,
                ..
            } => {
                let ok = env_handle_id == req.env_handle
                    && req
                        .effect_id
                        .as_deref()
                        .is_none_or(|claimed| claimed == effect_id);
                (ok, Some(effect_id))
            }
            ResolveOutcome::Unknown => (false, None),
        };
        // The decided row's effect_id: the token's binding when resolved,
        // else the claim (the unattributed deny still names what was claimed).
        let effect_id = resolved_effect
            .clone()
            .or_else(|| req.effect_id.clone())
            .unwrap_or_default();

        // 3. The pure decision.
        let decision = decide_egress(&self.policy, req, attributed, &self.cache);

        match decision.decision {
            EgressVerdict::Deny => {
                let ev = self.decide_row(
                    req,
                    &decision,
                    DecidedBy::Policy,
                    &[],
                    &[],
                    &effect_id,
                    started,
                    chain,
                    false,
                )?;
                Ok(GateOutcome::Terminal(MediatedOutcome::Refused {
                    request_ref: request_ref(req),
                    decided_ref: ev.event_id,
                    reason: decision.reason.unwrap_or(EgressReason::Denied),
                }))
            }
            EgressVerdict::Ask => Ok(GateOutcome::Terminal(
                self.ask(req, &decision, &effect_id, started, chain)?,
            )),
            EgressVerdict::Allow => Ok(GateOutcome::Allowed(Box::new(GateAllow {
                decision,
                effect_id,
                started,
                decided_by: DecidedBy::Policy,
                amended_policy: None,
            }))),
        }
    }

    /// `handle(request, chain)` — the mediated path: requested → attributed
    /// → decide → recheck → mediate → decided → wire → charge.
    pub fn handle(
        &mut self,
        req: &EgressRequest,
        chain: &ScopeChain,
    ) -> Result<MediatedOutcome, EgressError> {
        match self.gate(req, chain)? {
            GateOutcome::Terminal(o) => Ok(o),
            GateOutcome::Allowed(g) => self.forward(
                req,
                &g.decision,
                g.decided_by,
                &g.effect_id,
                g.started,
                chain,
            ),
        }
    }

    /// `pre_wire` — the legs `forward`/`forward_tunnel` share (R6.15;
    /// F11): consume-once re-resolution + recheck, the C2 post-allow
    /// guards, credential-sentinel staging, the `network.calls` budget
    /// gate, then `decided{allow}` durable. Returns `PreWire::Refused`
    /// when a leg refuses (its `decided{deny}` row is already durable)
    /// or `PreWire::Ready` with the fresh addrs, the staged claims and
    /// the decided event.
    fn pre_wire(
        &mut self,
        req: &EgressRequest,
        decision: &EgressDecision,
        decided_by: DecidedBy,
        effect_id: &str,
        started: u64,
        chain: &ScopeChain,
    ) -> Result<PreWire, EgressError> {
        // (d) Consume-once re-resolution: the caller's `resolved_addrs` are
        //     never trusted — the mediator resolves and re-checks (the SSRF
        //     pivot — a public name resolving private dies here).
        let host_norm = req.host_norm();
        let fresh = self
            .resolver
            .resolve(&host_norm)
            .map_err(|e| EgressError::Resolve {
                detail: format!("resolve {host_norm}: {e}"),
            })?;
        if let Some(deny) = recheck_resolved(&self.policy, req, &fresh) {
            let ev = self.decide_row(
                req,
                &deny,
                decided_by,
                &fresh.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
                &[],
                effect_id,
                started,
                chain,
                false,
            )?;
            return Ok(PreWire::Refused(MediatedOutcome::Refused {
                request_ref: request_ref(req),
                decided_ref: ev.event_id,
                reason: deny.reason.unwrap_or(EgressReason::Denied),
            }));
        }

        // (d2) The C2 post-allow guards (R2.9b; ADR-0341 D3) — they run on
        //      *every* allow here, including monitor-endorsed ones the pure
        //      `decide_egress` never re-decided. Order: the `approved_host_
        //      body` readers closer (content flow), then TLS termination
        //      (a hook inspects terminated/plaintext content — refuse
        //      before invoking hooks when a TLS leg can't terminate), then
        //      the declared hook set. A guard deny is durable before the
        //      refusal is visible, and no credential is staged for a
        //      refused leg.
        if let Some(deny) = hharness_containment::approved_host_body_check(&self.policy, req) {
            let ev = self.decide_row(
                req,
                &deny,
                decided_by,
                &fresh.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
                &[],
                effect_id,
                started,
                chain,
                false,
            )?;
            return Ok(PreWire::Refused(MediatedOutcome::Refused {
                request_ref: request_ref(req),
                decided_ref: ev.event_id,
                reason: deny.reason.unwrap_or(EgressReason::ReaderCoverage),
            }));
        }
        let tls_leg = req.protocol == hharness_containment::policy::EgressProtocol::HttpsConnect;
        if tls_leg
            && (self.policy.net.tls.terminate || !self.policy.net.tls.inspect_hooks.is_empty())
            && !self.transport.terminates_tls()
        {
            // A TLS leg under `tls.terminate` or declared hooks cannot be
            // honoured by a non-terminating transport — enforce-or-refuse.
            let (reason, detail) = if self.policy.net.tls.terminate {
                (
                    EgressReason::TlsTerminateUnsupported,
                    "tls.terminate".to_string(),
                )
            } else {
                (
                    EgressReason::InspectHookUnavailable,
                    self.policy.net.tls.inspect_hooks[0].clone(),
                )
            };
            let deny = EgressDecision::deny_guard(EgressSource::TlsGuard, reason, Some(detail));
            let ev = self.decide_row(
                req,
                &deny,
                decided_by,
                &fresh.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
                &[],
                effect_id,
                started,
                chain,
                false,
            )?;
            return Ok(PreWire::Refused(MediatedOutcome::Refused {
                request_ref: request_ref(req),
                decided_ref: ev.event_id,
                reason,
            }));
        }
        let hook_deny: Option<EgressDecision> = {
            let view = InspectView {
                host_norm: &host_norm,
                port: req.port,
                protocol: req.protocol.as_str(),
                method: req.method.as_deref().unwrap_or("GET"),
                path: req.path.as_deref(),
                headers: &req.headers,
                body: req.body.as_deref(),
            };
            let mut deny = None;
            for hook_ref in &self.policy.net.tls.inspect_hooks {
                match self.inspect_hooks.get(hook_ref) {
                    None => {
                        deny = Some(EgressDecision::deny_guard(
                            EgressSource::InspectHook,
                            EgressReason::InspectHookUnavailable,
                            Some(hook_ref.clone()),
                        ));
                        break;
                    }
                    Some(h) if h.inspect(&view) == InspectVerdict::Deny => {
                        deny = Some(EgressDecision::deny_guard(
                            EgressSource::InspectHook,
                            EgressReason::InspectHookDenied,
                            Some(hook_ref.clone()),
                        ));
                        break;
                    }
                    _ => {}
                }
            }
            deny
        };
        if let Some(deny) = hook_deny {
            let reason = deny.reason.unwrap_or(EgressReason::InspectHookDenied);
            let ev = self.decide_row(
                req,
                &deny,
                decided_by,
                &fresh.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
                &[],
                effect_id,
                started,
                chain,
                false,
            )?;
            return Ok(PreWire::Refused(MediatedOutcome::Refused {
                request_ref: request_ref(req),
                decided_ref: ev.event_id,
                reason,
            }));
        }

        // (e) The credential-binding slot — each sentinel mediates through
        //     the broker (denied row durable before the refusal is visible).
        let sentinel_refs = Self::collect_sentinels(req);
        let mut staged: Vec<(String, String)> = Vec::new(); // (sentinel, claim_id)
        let mut applied: Vec<String> = Vec::new();
        for sentinel in &sentinel_refs {
            let desc = RequestDescriptor {
                dpop_proof: None,
                effect_id: effect_id.to_string(),
                env_handle: req.env_handle.clone(),
                destination: host_norm.clone(),
                method: req.method.clone(),
                path: req.path.clone(),
            };
            match self
                .store
                .with_store(|st| {
                    self.broker.mediate_sentinel(
                        &mut *st,
                        &self.run_id,
                        self.lease,
                        sentinel,
                        &desc,
                    )
                })
                .map_err(|e| EgressError::Broker {
                    detail: format!("mediate_sentinel: {e:?}"),
                })? {
                MediationOutcome::Staged(d) => {
                    staged.push((sentinel.clone(), d.claim_id));
                    applied.push(d.binding_id);
                }
                MediationOutcome::Refused(r) => {
                    // The egress decision was allow; the credential leg
                    // refused — the decided row spells `deny{monitor}` and
                    // the credential.denied row carries the precise code.
                    let deny = EgressDecision {
                        decision: EgressVerdict::Deny,
                        source: EgressSource::Monitor,
                        rule_ref: None,
                        reason: Some(EgressReason::Denied),
                        credential_binding_candidates: vec![],
                        rule_index: None,
                        guard_detail: None,
                    };
                    let ev = self.decide_row(
                        req,
                        &deny,
                        decided_by,
                        &fresh.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
                        &[],
                        effect_id,
                        started,
                        chain,
                        false,
                    )?;
                    return Ok(PreWire::Refused(MediatedOutcome::RefusedCredential {
                        request_ref: request_ref(req),
                        decided_ref: ev.event_id,
                        code: r.code,
                    }));
                }
            }
        }

        // (f-pre) The `network.calls` gate — a call that can't be charged
        //     must not fire: when the dimension is already at/against its
        //     cap, the decided row spells `deny{monitor, budget_exhausted}`
        //     and nothing reaches the wire (§8.2's "deny before the
        //     authority is spent" — the ask path's `resolve_ask` is the
        //     same gate for `approvals.requested`).
        if let Some(budget_id) = self.budget_id.clone() {
            let exceeded = self.store.with_store(|st| {
                Account::open(&mut *st, &self.run_id)
                    .map_err(|e| EgressError::Budget {
                        detail: format!("account open: {e:?}"),
                    })
                    .and_then(|acct| {
                        acct.check(&budget_id).map_err(|e| EgressError::Budget {
                            detail: format!("check {budget_id}: {e:?}"),
                        })
                    })
            })?;
            let calls_exceeded = exceeded
                .iter()
                .any(|x| x.dimension.primary() == Some(DimensionId::NetworkCalls));
            if calls_exceeded {
                let deny = EgressDecision {
                    decision: EgressVerdict::Deny,
                    source: EgressSource::Monitor,
                    rule_ref: None,
                    reason: Some(EgressReason::BudgetExhausted),
                    credential_binding_candidates: vec![],
                    rule_index: None,
                    guard_detail: None,
                };
                let ev = self.decide_row(
                    req,
                    &deny,
                    decided_by,
                    &fresh.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
                    &[],
                    effect_id,
                    started,
                    chain,
                    false,
                )?;
                return Ok(PreWire::Refused(MediatedOutcome::Refused {
                    request_ref: request_ref(req),
                    decided_ref: ev.event_id,
                    reason: EgressReason::BudgetExhausted,
                }));
            }
        }

        // `decided{allow}` — durable before the wire fires; the
        // `tls_terminated` member records whether the leg terminates at the
        // mediator (a runtime fact the C2 surface reads back).
        let decided_ev = self.decide_row(
            req,
            decision,
            decided_by,
            &fresh.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
            &applied,
            effect_id,
            started,
            chain,
            self.transport.terminates_tls(),
        )?;
        let decided_ref = decided_ev.event_id.clone();

        Ok(PreWire::Ready {
            fresh,
            staged,
            decided_ref,
        })
    }

    /// The decided row (durable before its outcome is visible).
    #[allow(clippy::too_many_arguments)] // the arity is the row material's.
    fn decide_row(
        &mut self,
        req: &EgressRequest,
        decision: &EgressDecision,
        decided_by: DecidedBy,
        checked_addrs: &[String],
        applied: &[String],
        effect_id: &str,
        started: u64,
        chain: &ScopeChain,
        tls_terminated: bool,
    ) -> Result<Event, EgressError> {
        let latency = self
            .store
            .read_store(|s| s.now_ms())
            .saturating_sub(started);
        self.emit(
            "security.egress.decided",
            egress_decided_payload(
                req,
                &self.policy.version_id,
                decision,
                decided_by,
                checked_addrs,
                applied,
                effect_id,
                latency,
                tls_terminated,
            ),
            Some(effect_id),
            chain,
        )
    }

    /// The ask path — `approvals.requested` reserve → pending/requested rows
    /// → `decided{decision: ask}`.
    fn ask(
        &mut self,
        req: &EgressRequest,
        decision: &EgressDecision,
        effect_id: &str,
        started: u64,
        chain: &ScopeChain,
    ) -> Result<MediatedOutcome, EgressError> {
        // The approvals-budget reserve — exhausted ⇒ deny{budget_exhausted}.
        if let Some(budget_id) = self.budget_id.clone() {
            let resolve = self.store.with_store(|st| {
                Account::open(&mut *st, &self.run_id)
                    .map_err(|e| EgressError::Budget {
                        detail: format!("account open: {e:?}"),
                    })
                    .and_then(|mut acct| {
                        acct.resolve_ask(self.lease, &budget_id)
                            .map_err(|e| EgressError::Budget {
                                detail: format!("resolve_ask: {e:?}"),
                            })
                    })
            })?;
            match resolve {
                AskOutcome::Deny => {
                    let deny = EgressDecision {
                        decision: EgressVerdict::Deny,
                        source: EgressSource::DefaultUnmatched,
                        rule_ref: None,
                        reason: Some(EgressReason::BudgetExhausted),
                        credential_binding_candidates: vec![],
                        rule_index: None,
                        guard_detail: None,
                    };
                    let ev = self.decide_row(
                        req,
                        &deny,
                        DecidedBy::Default,
                        &[],
                        &[],
                        effect_id,
                        started,
                        chain,
                        false,
                    )?;
                    return Ok(MediatedOutcome::Refused {
                        request_ref: request_ref(req),
                        decided_ref: ev.event_id,
                        reason: EgressReason::BudgetExhausted,
                    });
                }
                AskOutcome::Allow => {}
            }
        }

        // Mint the pending coordinate + emit the owed-decision trail.
        let cap = hharness_compiler::plan::PinnedRef {
            semantic_id: "net_egress".to_string(),
            version_id: format!("net_egress@{}", self.policy.version_id),
        };
        let args_hash = hharness_identity::idp::idp_id(
            "egress.ask",
            format!(
                "{}\u{1f}{}\u{1f}{}\u{1f}{}",
                req.host_norm(),
                req.port,
                req.protocol.as_str(),
                req.method.as_deref().unwrap_or("")
            )
            .as_bytes(),
        );
        let permission_id = approval::mint_permission_id(&cap, &args_hash, false, effect_id);
        let request = ApprovalRequest {
            permission_id: permission_id.clone(),
            request: approval::PermissionRequest {
                subject_ref: self.participant_ref.clone(),
                capability_ref: cap.clone(),
                args_canonical_hash: args_hash.clone(),
                reason: format!("egress ask: {}", req.host_norm()),
                requested_grants: Vec::new(),
            },
            options: vec![
                approval::ApprovalOption {
                    id: approval::ApprovalOptionId::AllowOnce,
                    label: "allow_once".to_string(),
                },
                approval::ApprovalOption {
                    id: approval::ApprovalOptionId::AllowLease,
                    label: "allow_lease".to_string(),
                },
                approval::ApprovalOption {
                    id: approval::ApprovalOptionId::Deny,
                    label: "deny".to_string(),
                },
            ],
            mode: approval::ApprovalMode::Sync,
            timeout: None,
            explanation: approval::Explanation {
                display: format!("egress to {}:{}", req.host_norm(), req.port),
                rows: vec![],
                model_justification: None,
            },
            batch_id: None,
            // A mediator ask carries no offered remedies (the R-2.11
            // ingress member — empty, never fabricated).
            remedies: Vec::new(),
        };
        let asked_at = self.store.read_store(|s| s.now_ms());
        // R-2.11 (ADR-0343 D2) — the `action.intent.anchored` subject the
        // respond path's `label.endorsed{basis: approval}` pins. The
        // egress ask's intent is the *participant's* proposal — the
        // anchor's provenance is the participant's record (the
        // endorsement's `from`), minted durable before the pending so the
        // append-time check-5 subject lookup resolves.
        {
            self.store.with_store(|st| {
                EventMinter::new(&*st, &self.run_id)
                    .mint_effect(
                        "action.intent.anchored",
                        Json::obj([
                            ("permission_id", Json::str(permission_id.clone())),
                            ("effect_id", Json::str(effect_id.to_string())),
                            ("subject_ref", Json::str(self.participant_ref.clone())),
                            (
                                "capability_ref",
                                Json::obj([
                                    ("semantic_id", Json::str(cap.semantic_id.clone())),
                                    ("version_id", Json::str(cap.version_id.clone())),
                                ]),
                            ),
                            ("args_canonical_hash", Json::str(args_hash.clone())),
                        ]),
                        effect_id,
                        chain,
                    )
                    .map_err(|e| EgressError::Ledger {
                        detail: format!("mint action.intent.anchored: {e}"),
                    })
                    .and_then(|mut anchor| {
                        anchor.provenance = Some(ProvenanceRecord::minted(
                            Origin::participant(self.participant_ref.clone(), "hh.hosting/1"),
                            PersistenceScope::Run,
                            asked_at,
                        ));
                        anchor.content_kind = Some(hharness_provenance::ContentKind::EffectIntent);
                        st.append(&self.run_id, self.lease, vec![anchor])
                            .map_err(|e| EgressError::Ledger {
                                detail: format!("append action.intent.anchored: {e}"),
                            })
                            .map(|_| ())
                    })
            })?;
        }
        self.emit(
            "security.permission.pending",
            approval::pending_payload(&request, effect_id, asked_at),
            Some(effect_id),
            chain,
        )?;
        self.emit(
            "security.permission.requested",
            approval::requested_payload(&request, effect_id, asked_at),
            Some(effect_id),
            chain,
        )?;
        let ask_decision = EgressDecision {
            decision: EgressVerdict::Ask,
            source: decision.source,
            rule_ref: None,
            reason: None,
            credential_binding_candidates: vec![],
            rule_index: None,
            guard_detail: None,
        };
        let ev = self.decide_row(
            req,
            &ask_decision,
            DecidedBy::Default,
            &[],
            &[],
            effect_id,
            started,
            chain,
            false,
        )?;
        Ok(MediatedOutcome::Asked {
            request_ref: request_ref(req),
            decided_ref: ev.event_id,
            permission_id,
        })
    }

    /// `endorse_gate(req, response, endorser)` — the *gate-stage* half of
    /// the monitor's answer (R2.9a; DF-S2.4-1b — the resume ingress):
    /// dispatch calls this where [`EgressMediator::gate`] would run, when
    /// the durable prefix shows the egress `pending` was answered by a
    /// `security.permission.decided` row. The wire leg is still deferred —
    /// an allowed endorsement hands back a [`GateAllow`] whose
    /// `decided_by = monitor` (and `amended_policy` when `allow_lease`'s
    /// `amend` succeeded) so `forward` mints `decided{allow, monitor}` at
    /// the wire point under the effect's write-ahead.
    ///
    /// - `deny`/`more_info` ⇒ `decided{deny, source: monitor}` is durable
    ///   and the outcome terminates at the gate.
    /// - `allow_once` ⇒ allowed for *this request only* (nothing cached,
    ///   nothing amended).
    /// - `allow_lease{session|persisted}` ⇒ `amend(AddEgressAllow)` at the
    ///   lease's scope (`session | run` — §5g.4 §2's bound), the
    ///   `security.containment.amended` row, the cache insert (when
    ///   `session_cache`), then allow. An `amend` refusal falls back to a
    ///   narrowing-only cache entry when `session_cache` is on; otherwise
    ///   the endorsement is refused typed (`EgressError::Amend`).
    pub fn endorse_gate(
        &mut self,
        req: &EgressRequest,
        response: &ApprovalResponse,
        endorser: &ProvenanceRecord,
        chain: &ScopeChain,
    ) -> Result<GateOutcome, EgressError> {
        let started = self.store.read_store(|s| s.now_ms());
        let effect_id = req.effect_id.clone().unwrap_or_default();
        let mut amended_policy = None;
        match &response.choice {
            ResponseChoice::Deny { .. } | ResponseChoice::MoreInfo => {
                let deny = EgressDecision {
                    decision: EgressVerdict::Deny,
                    source: EgressSource::Monitor,
                    rule_ref: None,
                    reason: Some(EgressReason::Denied),
                    credential_binding_candidates: vec![],
                    rule_index: None,
                    guard_detail: None,
                };
                let ev = self.decide_row(
                    req,
                    &deny,
                    DecidedBy::Monitor,
                    &[],
                    &[],
                    &effect_id,
                    started,
                    chain,
                    false,
                )?;
                return Ok(GateOutcome::Terminal(MediatedOutcome::Refused {
                    request_ref: request_ref(req),
                    decided_ref: ev.event_id,
                    reason: EgressReason::Denied,
                }));
            }
            ResponseChoice::Modify { .. } => {
                // The C2 `modify` arm (ADR-0343 D1): the as-proposed
                // egress request is refused — the amendment re-enters as
                // a fresh request under a new ask, never an in-place
                // allow.
                let deny = EgressDecision {
                    decision: EgressVerdict::Deny,
                    source: EgressSource::Monitor,
                    rule_ref: None,
                    reason: Some(EgressReason::Denied),
                    credential_binding_candidates: vec![],
                    rule_index: None,
                    guard_detail: None,
                };
                let ev = self.decide_row(
                    req,
                    &deny,
                    DecidedBy::Monitor,
                    &[],
                    &[],
                    &effect_id,
                    started,
                    chain,
                    false,
                )?;
                return Ok(GateOutcome::Terminal(MediatedOutcome::Refused {
                    request_ref: request_ref(req),
                    decided_ref: ev.event_id,
                    reason: EgressReason::Denied,
                }));
            }
            // R2.12 (DF-S2.7-1b) — a remedy take is a deciding response:
            // `approval{effect_id}` *is* the allow for this egress (the
            // fallthrough arm's grant path); every other kind refuses the
            // as-proposed request — the re-dispatch re-enters fresh under
            // the durable `remedy_taken` attestation (ADR-0343 D4).
            ResponseChoice::Remedy(Remedy::Approval { .. }) => {}
            ResponseChoice::Remedy(_) => {
                let deny = EgressDecision {
                    decision: EgressVerdict::Deny,
                    source: EgressSource::Monitor,
                    rule_ref: None,
                    reason: Some(EgressReason::Denied),
                    credential_binding_candidates: vec![],
                    rule_index: None,
                    guard_detail: None,
                };
                let ev = self.decide_row(
                    req,
                    &deny,
                    DecidedBy::Monitor,
                    &[],
                    &[],
                    &effect_id,
                    started,
                    chain,
                    false,
                )?;
                return Ok(GateOutcome::Terminal(MediatedOutcome::Refused {
                    request_ref: request_ref(req),
                    decided_ref: ev.event_id,
                    reason: EgressReason::Denied,
                }));
            }
            ResponseChoice::AllowOnce => {}
            ResponseChoice::AllowLease(spec) => {
                // §5g.4 §2's bound — an approval hatch lives at
                // `session | run`; at this stage a `run` lease *is* the
                // run (`PersistenceScope::Session` would breach the `Run`
                // ceiling — the lease's run is the run, not a
                // cross-run surface). `session` (and anything wider — the
                // mint already bounds the lease at `≤ session`) asks beyond
                // the run — under the default ceiling the `amend` refuses
                // and the narrowing-only cache is the fallback (ADR-0266 D6).
                let (persistence, cache_scope) = match spec.scope {
                    approval::LeaseScope::Turn => (PersistenceScope::Turn, None),
                    approval::LeaseScope::Run => (PersistenceScope::Run, Some(CacheScope::Session)),
                    approval::LeaseScope::Session
                    | approval::LeaseScope::User
                    | approval::LeaseScope::Project
                    | approval::LeaseScope::Definition => {
                        (PersistenceScope::User, Some(CacheScope::Run))
                    }
                };
                let host = req.host_norm();
                let diff = hharness_containment::admit::ContainmentDiff::AddEgressAllow {
                    host: host.clone(),
                };
                match amend(
                    &self.policy,
                    &diff,
                    AmendmentBasis::Approval,
                    endorser,
                    persistence,
                ) {
                    Ok(outcome) => {
                        self.emit(
                            "security.containment.amended",
                            amended_payload(
                                &outcome,
                                &diff,
                                AmendmentBasis::Approval,
                                endorser,
                                persistence,
                                Some(&effect_id),
                            ),
                            Some(&effect_id),
                            chain,
                        )?;
                        self.policy = outcome.policy.clone();
                        self.cache.retain_version(&self.policy.version_id);
                        amended_policy = Some(outcome.policy);
                    }
                    Err(e) => {
                        // Fall back to a narrowing-only cache entry when the
                        // policy permits caching — the endorsement still
                        // narrows to the exact request shape (ADR-0266 D6).
                        if !self.policy.amendment.session_cache || cache_scope.is_none() {
                            return Err(EgressError::Amend(e));
                        }
                    }
                }
                if let Some(scope) = cache_scope {
                    if self.policy.amendment.session_cache {
                        self.cache.insert(
                            &host,
                            req.port,
                            req.protocol,
                            req.method.as_deref(),
                            scope,
                            &self.policy.version_id,
                            &effect_id,
                            &response.permission_id,
                        );
                    }
                }
            }
        }
        // The endorsement decided allow — `forward` runs the wire leg under
        // `source: monitor` / `decided_by = monitor`.
        Ok(GateOutcome::Allowed(Box::new(GateAllow {
            decision: EgressDecision {
                decision: EgressVerdict::Allow,
                source: EgressSource::Monitor,
                rule_ref: None,
                reason: None,
                credential_binding_candidates: vec![],
                rule_index: None,
                guard_detail: None,
            },
            effect_id,
            started,
            decided_by: DecidedBy::Monitor,
            amended_policy,
        })))
    }

    /// `endorse_asked(req, response, endorser)` — the monitor's answer
    /// resolves the pending ask (ADR-0266 D6): [`Self::endorse_gate`]
    /// decides, then the allow legs forward at the wire point.
    ///
    /// - `deny`/`more_info` ⇒ `decided{deny, source: monitor}` + refuse.
    /// - `allow_once` ⇒ `decided{allow, source: monitor}` + forward — the
    ///   endorsement covers *this request only* (nothing cached, nothing
    ///   amended).
    /// - `allow_lease{session|persisted}` ⇒ `amend(AddEgressAllow)` at the
    ///   lease's scope (`session | run` — §5g.4 §2's bound), the
    ///   `security.containment.amended` row, the cache insert (when
    ///   `session_cache`), then the forward under `decided{allow, monitor}`.
    ///   An `amend` refusal falls back to a cache entry when
    ///   `session_cache` is on; otherwise the endorsement is refused typed.
    pub fn endorse_asked(
        &mut self,
        req: &EgressRequest,
        response: &ApprovalResponse,
        endorser: &ProvenanceRecord,
        chain: &ScopeChain,
    ) -> Result<MediatedOutcome, EgressError> {
        match self.endorse_gate(req, response, endorser, chain)? {
            GateOutcome::Terminal(o) => Ok(o),
            GateOutcome::Allowed(g) => self.forward(
                req,
                &g.decision,
                g.decided_by,
                &g.effect_id,
                g.started,
                chain,
            ),
        }
    }

    /// The allow pipeline — recheck → sentinels → decided{allow} → wire →
    /// charge. Shared by the direct allow and the monitor-endorsed allow.
    /// `pub` for the dispatch path (DF-S2.4-1): a `gate`-allowed request is
    /// forwarded at the wire point — post-`committed`, pre-executor — so the
    /// effect's write-ahead still precedes the wire.
    pub fn forward(
        &mut self,
        req: &EgressRequest,
        decision: &EgressDecision,
        decided_by: DecidedBy,
        effect_id: &str,
        started: u64,
        chain: &ScopeChain,
    ) -> Result<MediatedOutcome, EgressError> {
        let (fresh, staged, decided_ref) =
            match self.pre_wire(req, decision, decided_by, effect_id, started, chain)? {
                PreWire::Refused(o) => return Ok(o),
                PreWire::Ready {
                    fresh,
                    staged,
                    decided_ref,
                } => (fresh, staged, decided_ref),
            };
        let host_norm = req.host_norm();

        // (f) Drain the claims consume-once and substitute into the
        //     headers, the path and the body — `collect_sentinels` already
        //     *detects* `mh_secret:` spellings in all three (the staging
        //     above mediates them); the drain completes the substitution
        //     for every member the collector scanned, so a credential
        //     riding the query (R4.15d's `signed_url` carrier — the URI
        //     stays `credential_free_uri` while the broker supplies the
        //     signature) is substituted at the wire, never forwarded as a
        //     literal sentinel.
        let mut headers = req.headers.clone();
        let mut path = req.path.clone();
        let mut body = req.body.clone();
        for (sentinel, claim_id) in &staged {
            let Some((_bid, value)) = self.broker.drain_claim(claim_id) else {
                return Err(EgressError::Broker {
                    detail: format!("claim {claim_id} vanished before drain"),
                });
            };
            for (k, v) in headers.iter_mut() {
                if v.contains(sentinel.as_str()) {
                    *v = v.replace(sentinel.as_str(), &value);
                    let _ = k;
                }
            }
            for text in [&mut path, &mut body].into_iter().flatten() {
                if text.contains(sentinel.as_str()) {
                    *text = text.replace(sentinel.as_str(), &value);
                }
            }
        }

        // (g) The wire — dial the *resolved* address, never re-resolve.
        let addr = *fresh.first().ok_or_else(|| EgressError::Resolve {
            detail: format!("{host_norm} resolved to no addresses"),
        })?;
        let response = self
            .transport
            .forward(
                addr,
                req.port,
                &host_norm,
                req.method.as_deref().unwrap_or("GET"),
                path.as_deref().unwrap_or("/"),
                &headers,
                body.as_deref().map(|b| b.as_bytes()),
            )
            .map_err(|e| match e {
                ForwardError::Timeout { detail } => EgressError::Timeout {
                    detail: format!("forward {addr}:{}: {detail}", req.port),
                },
                ForwardError::Failed { detail } => EgressError::Transport {
                    detail: format!("forward {addr}:{}: {detail}", req.port),
                },
            })?;

        // (h) `network.calls` — one charge per forwarded call, attributed to
        //     the decided row's effect, source = the decided event.
        let mut charge_source = decided_ref.clone();
        if let Some(budget_id) = self.budget_id.clone() {
            self.store.with_store(|st| -> Result<(), EgressError> {
                let mut acct =
                    Account::open(&mut *st, &self.run_id).map_err(|e| EgressError::Budget {
                        detail: format!("account open: {e:?}"),
                    })?;
                let source = EventRef {
                    run_id: self.run_id.clone(),
                    event_id: decided_ref.clone(),
                };
                let mut attribution = Attribution::subject(
                    self.run_id.clone(),
                    budget_id.clone(),
                    self.participant_ref.clone(),
                );
                attribution.charged_to = default_charged_to("security.egress.decided");
                attribution.component_class = Some(COMPONENT.to_string());
                acct.charge(
                    self.lease,
                    &ChargeRequest {
                        budget_id,
                        quantity: ResourceQuantity {
                            dimension: DimensionId::NetworkCalls,
                            amount: 1,
                            unit: "calls".to_string(),
                            model_ref: None,
                            measured_at: source.clone(),
                        },
                        source,
                        attribution,
                        reservation_id: None,
                        cache_ttl: None,
                    },
                )
                .map_err(|e| EgressError::Budget {
                    detail: format!("charge network.calls: {e:?}"),
                })
                .map(|_| ())
            })?;
            charge_source = decided_ref.clone();
        }

        Ok(MediatedOutcome::Forwarded {
            request_ref: request_ref(req),
            decided_ref,
            charge_source,
            response,
        })
    }

    /// `forward_stream(req, …, out)` — the streaming forward (R6.45k;
    /// D13): identical `pre_wire`/charge legs as [`Self::forward`], but
    /// the upstream body streams through the transport's
    /// [`EgressTransport::forward_head`] into `out` — re-framed chunked
    /// after the D10 response hygiene — so resident memory stays bounded
    /// whatever the body size (the buffered `forward` API and its
    /// `FORWARD_MAX_BODY` bound stay for the in-process callers). On a
    /// head/write failure nothing is emitted to `out`; once the head is
    /// written a mid-body failure closes the connection and the error is
    /// returned typed for the record.
    #[allow(clippy::too_many_arguments)] // the wire-leg material is a single tuple.
    pub fn forward_stream(
        &mut self,
        req: &EgressRequest,
        decision: &EgressDecision,
        decided_by: DecidedBy,
        effect_id: &str,
        started: u64,
        chain: &ScopeChain,
        out: &mut impl std::io::Write,
    ) -> Result<MediatedOutcome, EgressError> {
        let (fresh, staged, decided_ref) =
            match self.pre_wire(req, decision, decided_by, effect_id, started, chain)? {
                PreWire::Refused(o) => return Ok(o),
                PreWire::Ready {
                    fresh,
                    staged,
                    decided_ref,
                } => (fresh, staged, decided_ref),
            };
        let host_norm = req.host_norm();
        let mut headers = req.headers.clone();
        let mut path = req.path.clone();
        let mut body = req.body.clone();
        for (sentinel, claim_id) in &staged {
            let Some((_bid, value)) = self.broker.drain_claim(claim_id) else {
                return Err(EgressError::Broker {
                    detail: format!("claim {claim_id} vanished before drain"),
                });
            };
            for (k, v) in headers.iter_mut() {
                if v.contains(sentinel.as_str()) {
                    *v = v.replace(sentinel.as_str(), &value);
                    let _ = k;
                }
            }
            for text in [&mut path, &mut body].into_iter().flatten() {
                if text.contains(sentinel.as_str()) {
                    *text = text.replace(sentinel.as_str(), &value);
                }
            }
        }
        let addr = *fresh.first().ok_or_else(|| EgressError::Resolve {
            detail: format!("{host_norm} resolved to no addresses"),
        })?;
        let map_fwd = |e: ForwardError, leg: &str| match e {
            ForwardError::Timeout { detail } => EgressError::Timeout {
                detail: format!("{leg} {addr}:{}: {detail}", req.port),
            },
            ForwardError::Failed { detail } => EgressError::Transport {
                detail: format!("{leg} {addr}:{}: {detail}", req.port),
            },
        };
        let method = req.method.as_deref().unwrap_or("GET").to_string();
        let mut fh = self
            .transport
            .forward_head(
                addr,
                req.port,
                &host_norm,
                &method,
                path.as_deref().unwrap_or("/"),
                &headers,
                body.as_deref().map(|b| b.as_bytes()),
            )
            .map_err(|e| map_fwd(e, "forward"))?;
        // D10 — emit upstream's end-to-end headers only, then exactly one
        // framing member (`Transfer-Encoding: chunked` re-frames whatever
        // the upstream declared) and `Connection: close` (the mediator
        // serves one request per client connection).
        let kept = end_to_end_response_headers(
            &fh.head
                .headers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>(),
        );
        let mut hdrs: Vec<(String, String)> = kept;
        hdrs.push(("Connection".to_string(), "close".to_string()));
        let hdrs_ref: Vec<(&str, &str)> =
            hdrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        hharness_wire::http::write_response_head_chunked(out, fh.head.status, &hdrs_ref).map_err(
            |e| EgressError::Transport {
                detail: format!("response head: {e}"),
            },
        )?;
        if let Some(src) = fh.body.as_mut() {
            loop {
                match src.next_chunk() {
                    Ok(Some(chunk)) => {
                        hharness_wire::http::write_chunk(out, &chunk).map_err(|e| {
                            EgressError::Transport {
                                detail: format!("response chunk: {e}"),
                            }
                        })?;
                    }
                    Ok(None) => break,
                    Err(e) => {
                        // The upstream body truncated mid-stream — the
                        // head is already out; finish the frame so the
                        // client isn't left hanging, then surface the
                        // typed error for the record.
                        let _ = hharness_wire::http::finish_chunked(out);
                        return Err(if e.kind() == std::io::ErrorKind::TimedOut {
                            EgressError::Timeout {
                                detail: format!("forward {addr}:{}: {e}", req.port),
                            }
                        } else {
                            EgressError::Transport {
                                detail: format!("forward {addr}:{}: {e}", req.port),
                            }
                        });
                    }
                }
            }
        }
        hharness_wire::http::finish_chunked(out).map_err(|e| EgressError::Transport {
            detail: format!("response finish: {e}"),
        })?;

        // `network.calls` — the same charge as the buffered path.
        if let Some(budget_id) = self.budget_id.clone() {
            self.store.with_store(|st| -> Result<(), EgressError> {
                let mut acct =
                    Account::open(&mut *st, &self.run_id).map_err(|e| EgressError::Budget {
                        detail: format!("account open: {e:?}"),
                    })?;
                let source = EventRef {
                    run_id: self.run_id.clone(),
                    event_id: decided_ref.clone(),
                };
                let mut attribution = Attribution::subject(
                    self.run_id.clone(),
                    budget_id.clone(),
                    self.participant_ref.clone(),
                );
                attribution.charged_to = default_charged_to("security.egress.decided");
                attribution.component_class = Some(COMPONENT.to_string());
                acct.charge(
                    self.lease,
                    &ChargeRequest {
                        budget_id,
                        quantity: ResourceQuantity {
                            dimension: DimensionId::NetworkCalls,
                            amount: 1,
                            unit: "calls".to_string(),
                            model_ref: None,
                            measured_at: source.clone(),
                        },
                        source,
                        attribution,
                        reservation_id: None,
                        cache_ttl: None,
                    },
                )
                .map_err(|e| EgressError::Budget {
                    detail: format!("charge network.calls: {e:?}"),
                })
                .map(|_| ())
            })?;
        }
        Ok(MediatedOutcome::Forwarded {
            request_ref: request_ref(req),
            decided_ref: decided_ref.clone(),
            charge_source: decided_ref,
            response: WireResponse {
                status: fh.head.status,
                headers: vec![],
                body: vec![],
            },
        })
    }

    /// `forward_tunnel(req, decision, decided_by, effect_id, started,
    /// chain, client)` — the CONNECT tunnel leg (R6.15; AR-05; F11;
    /// REQ-XB-114): the same `pre_wire` legs as `forward` (recheck →
    /// guards → sentinels → budget gate → `decided{allow}` durable
    /// before the first byte), then `transport.dial` to the resolved
    /// address, `200 Connection established`, and a byte pipe until
    /// close. The closing summary `security.egress.tunnel.closed`
    /// (bytes up/down, duration) lands before the `network.calls`
    /// charge. A denied CONNECT sends no byte upstream.
    #[allow(clippy::too_many_arguments)] // the wire-leg material is a single tuple.
    pub fn forward_tunnel(
        &mut self,
        req: &EgressRequest,
        decision: &EgressDecision,
        decided_by: DecidedBy,
        effect_id: &str,
        started: u64,
        chain: &ScopeChain,
        client: &mut dyn ByteStream,
    ) -> Result<MediatedOutcome, EgressError> {
        let (fresh, _staged, decided_ref) =
            match self.pre_wire(req, decision, decided_by, effect_id, started, chain)? {
                PreWire::Refused(o) => return Ok(o),
                PreWire::Ready {
                    fresh,
                    staged,
                    decided_ref,
                } => (fresh, staged, decided_ref),
            };
        let host_norm = req.host_norm();
        let addr = *fresh.first().ok_or_else(|| EgressError::Resolve {
            detail: format!("{host_norm} resolved to no addresses"),
        })?;
        let upstream = self.transport.dial(addr, req.port).map_err(|e| match e {
            ForwardError::Timeout { detail } => EgressError::Timeout {
                detail: format!("tunnel {addr}:{}: {detail}", req.port),
            },
            ForwardError::Failed { detail } => EgressError::Transport {
                detail: format!("tunnel {addr}:{}: {detail}", req.port),
            },
        })?;
        client
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .and_then(|()| client.flush())
            .map_err(|e| EgressError::Transport {
                detail: format!("tunnel 200: {e}"),
            })?;
        // The byte pipe — two directions, pumped to close (D9): real
        // byte counts even on an erroring leg, and a direction's end
        // propagates `shutdown(Write)` to the peer (the relay's pattern)
        // so the other leg observes the close instead of hanging on a
        // live socket. The legs carry the declared TUNNEL_IDLE_TIMEOUT —
        // never a silent 5 s kill: a leg idle past it ends `idle_timeout`,
        // and the close row's `close_cause` is the true outcome
        // (`eof|idle_timeout|error:<kind>`) — never `unwrap_or(0)`, never
        // a hard-coded "eof".
        let mut up_reader = upstream;
        let mut up_writer = up_reader.try_clone().map_err(|e| EgressError::Transport {
            detail: format!("tunnel upstream clone: {e}"),
        })?;
        let mut client_reader = client.try_clone().map_err(|e| EgressError::Transport {
            detail: format!("tunnel client clone: {e}"),
        })?;
        let up = std::thread::spawn(move || {
            let end = tunnel_pump(&mut *client_reader, &mut *up_writer);
            let _ = up_writer.shutdown(std::net::Shutdown::Write);
            end
        });
        let down_end = tunnel_pump(&mut *up_reader, client);
        // The downstream leg ended (upstream EOF/idle/error): the tunnel
        // is closing — shut the client socket fully so the still-running
        // up pump's read observes the close (its own EOF), never a silent
        // hang on a live socket.
        let _ = client.shutdown(std::net::Shutdown::Both);
        let up_end = up.join().unwrap_or(TunnelEnd {
            bytes: 0,
            cause: TunnelCause::Error("pump thread panicked".to_string()),
        });
        let (bytes_up, bytes_down) = (up_end.bytes, down_end.bytes);
        let close_cause = tunnel_close_cause(&up_end, &down_end);
        let duration_ms = self
            .store
            .read_store(|s| s.now_ms())
            .saturating_sub(started);
        // The closing summary — durable before the charge.
        let closed_ev = self.emit(
            "security.egress.tunnel.closed",
            Json::obj([
                ("request_ref", Json::str(request_ref(req))),
                ("decided_ref", Json::str(decided_ref.clone())),
                ("effect_id", Json::str(effect_id)),
                ("env_handle", Json::str(req.env_handle.clone())),
                ("host_norm", Json::str(host_norm)),
                ("port", Json::Int(req.port as i64)),
                ("bytes_up", Json::Int(bytes_up as i64)),
                ("bytes_down", Json::Int(bytes_down as i64)),
                ("duration_ms", Json::Int(duration_ms as i64)),
                ("close_cause", Json::str(close_cause)),
            ]),
            Some(effect_id),
            chain,
        )?;
        if let Some(budget_id) = self.budget_id.clone() {
            self.store.with_store(|st| -> Result<(), EgressError> {
                let mut acct =
                    Account::open(&mut *st, &self.run_id).map_err(|e| EgressError::Budget {
                        detail: format!("account open: {e:?}"),
                    })?;
                let source = EventRef {
                    run_id: self.run_id.clone(),
                    event_id: decided_ref.clone(),
                };
                let mut attribution = Attribution::subject(
                    self.run_id.clone(),
                    budget_id.clone(),
                    self.participant_ref.clone(),
                );
                attribution.charged_to = default_charged_to("security.egress.tunnel.closed");
                attribution.component_class = Some(COMPONENT.to_string());
                acct.charge(
                    self.lease,
                    &ChargeRequest {
                        budget_id,
                        quantity: ResourceQuantity {
                            dimension: DimensionId::NetworkCalls,
                            amount: 1,
                            unit: "calls".to_string(),
                            model_ref: None,
                            measured_at: source.clone(),
                        },
                        source,
                        attribution,
                        reservation_id: None,
                        cache_ttl: None,
                    },
                )
                .map_err(|e| EgressError::Budget {
                    detail: format!("charge network.calls: {e:?}"),
                })
                .map(|_| ())
            })?;
        }
        Ok(MediatedOutcome::Tunneled {
            request_ref: request_ref(req),
            decided_ref,
            closed_ref: closed_ev.event_id,
            bytes_up,
            bytes_down,
        })
    }

    /// Dispatch the destination-side revocation intents a `revoke` produced
    /// (LT-05) — each intent is itself a mediated egress (a POST to the
    /// destination's `revocation_path` through the same `handle` pipeline,
    /// so the dispatch is durably ledgered as `requested`/`decided`).
    pub fn dispatch_revocations(
        &mut self,
        intents: &[hharness_secrets::RevokeIntent],
        token: &str,
        chain: &ScopeChain,
    ) -> Vec<Result<MediatedOutcome, EgressError>> {
        intents
            .iter()
            .map(|i| {
                // The kernel's own revocation leg carries a coordinate body
                // (`{binding_id, channel_id}` — content-free) whose only
                // intended reader is the destination it revokes at; the
                // `approved_host_body` closer reads `Restricted{host_norm}`
                // as exactly that statement (R2.9b; ADR-0341 D6).
                let host_norm = hharness_containment::policy::normalize_host(&i.host_pattern);
                let req = EgressRequest {
                    token: token.to_string(),
                    effect_id: None,
                    tool_call_id: "credential-revoke".to_string(),
                    env_handle: "kernel".to_string(),
                    // The revoke rides the destination's declared carrier:
                    // an `https` destination gets the `https_connect` leg
                    // — under `tls.terminate` the terminating transport
                    // honours it, a non-terminating one refuses typed
                    // (R4.5c; the intent's scheme is R4.5c's addition).
                    protocol: if i.scheme == "https" {
                        hharness_containment::policy::EgressProtocol::HttpsConnect
                    } else {
                        hharness_containment::policy::EgressProtocol::Http
                    },
                    host_raw: i.host_pattern.clone(),
                    resolved_addrs: vec![],
                    port: i.port.unwrap_or(80),
                    method: Some("POST".to_string()),
                    path: Some(i.revocation_path.clone()),
                    headers: vec![],
                    body: Some(format!(
                        "{{\"binding_id\":\"{}\",\"channel_id\":\"{}\"}}",
                        i.binding_id, i.channel_id
                    )),
                    body_readers: Some(hharness_provenance::authority::ReaderSet::Restricted(
                        [host_norm].into_iter().collect(),
                    )),
                    credential_sentinels: vec![],
                };
                self.handle(&req, chain)
            })
            .collect()
    }
}
