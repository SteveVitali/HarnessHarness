//! `hh-embed-client-generated` — the generated `hh-embed/1` client.
//!
//! Everything of substance lives in `generated.rs`, which is **emitted by `hh-codegen`** from
//! the single schema source and checked in. Do not edit `generated.rs` by hand; the CI drift
//! check regenerates it and fails the build on any diff (CC7). This `lib.rs` is the stable
//! wrapper and is safe to hand-edit.
//!
//! In the polyglot split (ADR-0050) the real third-party clients are generated in the lab
//! (E2) and surface (E3) ecosystems; this in-ecosystem client is the Stage-0 stand-in that
//! exercises the schema-export → codegen → round-trip pipeline before those ecosystems bind
//! (Stage 3 / Stage ≥ 5).
//!
//! [`NetClient`] is the binding-(c) client (S4.10; §7.2 `local_network`):
//! the same canonical request/response contract over loopback HTTP —
//! `POST /hh-embed/1` is one JSON-RPC request → response, and
//! `GET /hh-embed/1/events?wait_ms=` drains `stream.frame` + `upcall.*`
//! notifications as NDJSON. It speaks the exact wire shape `hh-embed`'s
//! (c) server emits (bearer + exact-Host + no Origin); the generated
//! `*Params`/`*Result` types are transport-agnostic and shared with the
//! stdio [`Client`].

mod generated;
pub use generated::*;

use hh_wire::http::{read_response, split_target, HttpError, MAX_HEAD_BYTES};
use hh_wire::json::Json;
use std::io::{BufReader, Write};
use std::net::{SocketAddr, TcpStream};

/// The maximum RPC response/event-drain body (64 MiB mirrors the
/// server's request bound — views and pages can be large).
const NET_MAX_BODY: usize = 64 * 1024 * 1024;

/// The binding-(c) client. One `TcpStream` per call (the server speaks
/// `Connection: close`) — the transport is deliberately dumb: it carries
/// canonical bytes and nothing else (the op contract lives in
/// `hh-embed-schema`, the wire codec in `hh-wire::http` — CC1).
pub struct NetClient {
    addr: SocketAddr,
    token: String,
    next_id: i64,
    /// `stream.frame` notifications buffered by `drain`.
    pending: Vec<StreamNotification>,
    /// `upcall.*` notifications buffered by `drain`, `(method, params)`.
    pending_upcalls: Vec<(String, Json)>,
    /// The negotiated `HelloResult` (set by `hello`).
    pub hello_result: Option<HelloResult>,
}

impl NetClient {
    /// The bound socket + capability token (`hh-kernel serve --http`
    /// prints both; the token never rides a URL — P9).
    pub fn connect(addr: SocketAddr, token: &str) -> NetClient {
        NetClient {
            addr,
            token: token.to_string(),
            next_id: 0,
            pending: Vec::new(),
            pending_upcalls: Vec::new(),
            hello_result: None,
        }
    }

    /// The bound authority — the `Host` header the gate requires.
    pub fn authority(&self) -> String {
        self.addr.to_string()
    }

    /// `POST /hh-embed/1` — one canonical request → one canonical
    /// response (`result` or the decoded `EmbedError` sum).
    pub fn call(&mut self, method: &str, params: Json) -> Result<Json, ClientError> {
        self.next_id += 1;
        let id = self.next_id;
        let req = Json::obj([
            ("jsonrpc", Json::str("2.0")),
            ("id", Json::Int(id)),
            ("method", Json::str(method)),
            ("params", params),
        ]);
        let (status, body) = self.post(&req.to_canonical_string())?;
        if status == 401 {
            return Err(ClientError::Transport("401 unauthorized".into()));
        }
        if status != 200 {
            return Err(ClientError::Transport(format!("http {status}")));
        }
        let msg =
            hh_wire::json::parse(body.trim()).map_err(|e| ClientError::Transport(e.to_string()))?;
        if msg.get("id").and_then(Json::as_int) != Some(id) {
            return Err(ClientError::Transport(format!(
                "response id mismatch: got {:?}, want {id}",
                msg.get("id")
            )));
        }
        if let Some(err) = msg.get("error") {
            return Err(ClientError::Rpc(decode_error_net(err)));
        }
        msg.get("result")
            .cloned()
            .ok_or_else(|| ClientError::Transport("response has no result".into()))
    }

    /// `GET /hh-embed/1/events?wait_ms=` — the notification drain;
    /// `stream.frame` frames buffer on `pending`, `upcall.*` on
    /// `pending_upcalls` (same shapes as the stdio client's buffers).
    pub fn drain(&mut self, wait_ms: u64) -> Result<(), ClientError> {
        let (status, body) = self.get_events(wait_ms)?;
        if status == 401 {
            return Err(ClientError::Transport("401 unauthorized".into()));
        }
        if status != 200 {
            return Err(ClientError::Transport(format!("http {status}")));
        }
        for line in body.lines() {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            let msg = hh_wire::json::parse(t).map_err(|e| ClientError::Transport(e.to_string()))?;
            let method = msg.get("method").and_then(Json::as_str).unwrap_or("");
            if method == "stream.frame" {
                if let Some(p) = msg.get("params") {
                    if let Ok(n) = StreamNotification::from_json(p) {
                        self.pending.push(n);
                    }
                }
            } else if method.starts_with("upcall.") {
                self.pending_upcalls.push((
                    method.to_string(),
                    msg.get("params").cloned().unwrap_or(Json::Null),
                ));
            }
        }
        Ok(())
    }

    /// The buffered `stream.frame` notifications (oldest first).
    pub fn take_notifications(&mut self) -> Vec<StreamNotification> {
        std::mem::take(&mut self.pending)
    }

    /// The buffered `upcall.*` notifications (arrival order).
    pub fn take_upcalls(&mut self) -> Vec<(String, Json)> {
        std::mem::take(&mut self.pending_upcalls)
    }

    fn post(&self, body: &str) -> Result<(u16, String), ClientError> {
        let mut stream = TcpStream::connect(self.addr)
            .map_err(|e| ClientError::Transport(format!("connect: {e}")))?;
        let head = format!(
            "POST /hh-embed/1 HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            self.addr,
            self.token,
            body.len()
        );
        stream
            .write_all(head.as_bytes())
            .and_then(|_| stream.write_all(body.as_bytes()))
            .and_then(|_| stream.flush())
            .map_err(|e| ClientError::Transport(e.to_string()))?;
        self.read_http(&mut stream)
    }

    fn get_events(&self, wait_ms: u64) -> Result<(u16, String), ClientError> {
        let mut stream = TcpStream::connect(self.addr)
            .map_err(|e| ClientError::Transport(format!("connect: {e}")))?;
        let req = format!(
            "GET /hh-embed/1/events?wait_ms={wait_ms} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\n\r\n",
            self.addr, self.token
        );
        stream
            .write_all(req.as_bytes())
            .and_then(|_| stream.flush())
            .map_err(|e| ClientError::Transport(e.to_string()))?;
        self.read_http(&mut stream)
    }

    fn read_http(&self, stream: &mut TcpStream) -> Result<(u16, String), ClientError> {
        let mut r = BufReader::new(
            stream
                .try_clone()
                .map_err(|e| ClientError::Transport(e.to_string()))?,
        );
        let (head, body) = read_response(&mut r, NET_MAX_BODY)
            .map_err(|e| ClientError::Transport(format!("http: {e}")))?;
        String::from_utf8(body)
            .map(|s| (head.status, s))
            .map_err(|_| ClientError::Transport("non-utf8 response".into()))
    }
}

// `decode_error` is generated-private — `NetClient` rebuilds the same
// `EmbedError` from the public fields (`code`/`message`/`data.kind` +
// the `error_retryable` table, which *is* generated-public). The wire
// shape is the error JSON itself; this is the client-side lowering of
// it, not a second contract.
fn decode_error_net(err: &Json) -> EmbedError {
    let code = err.get("code").and_then(Json::as_int).unwrap_or(0);
    let message = err
        .get("message")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let data = err.get("data").cloned().unwrap_or(Json::Null);
    let kind = data
        .get("kind")
        .and_then(Json::as_str)
        .unwrap_or("Unknown")
        .to_string();
    let retryable = data
        .get("retryable")
        .map(|r| matches!(r, Json::Bool(true)))
        .unwrap_or_else(|| error_retryable(&kind));
    EmbedError {
        code,
        kind,
        retryable,
        message,
        data,
    }
}

// Keep the shared http helpers referenced (the codec is the one wire
// shape — the `use` is load-bearing for docs/links, not dead).
type CodecSentinels = (usize, fn(&str) -> (&str, Option<&str>), Option<HttpError>);

#[allow(dead_code)]
fn _codec_sentinels() -> CodecSentinels {
    (MAX_HEAD_BYTES, split_target, None)
}
