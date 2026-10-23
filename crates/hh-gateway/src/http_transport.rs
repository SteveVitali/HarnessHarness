//! `http_transport` — `HttpModelTransport`, the live `Transport` half
//! (R4.5b; ADR-0364 D1/D3; DF-S1.18-1's local-family leg).
//!
//! Plain HTTP/1.1 over `TcpStream` through the **one** codec home —
//! `hh_wire::http` (CC1; the `hh-env` egress leg and `hh-mcp` boundary ride
//! the same module). No TLS: an `https` endpoint_ref answers the typed
//! `unsupported_feature` refusal carrying `tls_pending` — R4.5c owns
//! rustls, and this lane never downgrades.
//!
//! `send` per the `Transport` contract: serialized `ProviderRequestPlan`
//! bytes → POST `{endpoint}/chat/completions` → `(arrival_ms, WireFrame)`
//! pairs. Arrival stamps are ms-since-send on the adapter's own clock —
//! the `grammar::Timing.measured_at: "adapter"` point (the dialect's G7
//! `stream_idle_timeout_ms` bounds inter-frame gaps; the socket deadline
//! carries the same bound).
//!
//! `discover` GETs `{endpoint}/models` and answers the `model_listing`
//! member (`data[]`) verbatim — observed, never fabricated.

use std::io::BufReader;

use hh_wire::http::{self, BodyReader, HttpError};
use hh_wire::json::Json;

use crate::codec::{self, WireFrame};
use crate::dialect::WireDialect;
use crate::gateway::{CredentialHandle, Transport};
use crate::openai::{self, ChunkTranslator, SseDecoder, SseEvent};
use crate::vocab::{ModelError, ModelErrorClass, TimeoutKind};

/// The request-path join for the family's operations — `endpoint_ref`'s
/// path is the API base (`/v1`, or `/`); the operation path appends.
fn join(base_target: &str, op: &str) -> String {
    let base = base_target.trim_end_matches('/');
    format!("{base}/{op}")
}

/// `HttpError → ModelError` — the boundary's typed refusal classes
/// (`where_phase` picks the timeout sub-kind: `connect` for the dial,
/// `attempt` for the head, `stream_idle` mid-body).
fn map_io(e: HttpError, phase: TimeoutKind) -> ModelError {
    match e {
        HttpError::Timeout => ModelError::new(
            ModelErrorClass::Timeout(phase),
            format!("deadline fired ({})", phase.as_str()),
        ),
        HttpError::Eof => ModelError::new(ModelErrorClass::Network, "peer closed mid-message"),
        HttpError::Io(d) => ModelError::new(ModelErrorClass::Network, d),
        HttpError::UnsupportedScheme(s) => {
            // `https` — R4.5c's lane. The refusal's `kind` member spells
            // `tls_pending` (the run ledger reads it; never a downgrade).
            let mut err = ModelError::new(
                ModelErrorClass::UnsupportedFeature,
                format!("scheme {s} — TLS pending (R4.5c)"),
            );
            err.provider_code = Some("tls_pending".to_string());
            err
        }
        other => ModelError::new(
            ModelErrorClass::StreamDecode,
            format!("http codec: {other}"),
        ),
    }
}

/// `HttpModelTransport` — the live `Transport` for `http://` model
/// endpoints. `dialect` is held for `classify_http_error`/
/// `read_retry_after` (the boundary's own rules, not re-declared).
pub struct HttpModelTransport {
    /// The bound dialect (error classification + retry-after + body bounds
    /// derive from it).
    dialect: WireDialect,
    /// Dial deadline (ms).
    connect_ms: u64,
    /// Per-operation read/write deadline (ms).
    rw_ms: u64,
    /// Whole-body byte bound.
    max_body: usize,
}

impl HttpModelTransport {
    /// `new(dialect)` — the transport bound to a loaded dialect document.
    /// Defaults: 5 s dial, `stream_idle_timeout_ms` read/write bound
    /// (clamped to ≥ 30 s), 16 MiB bodies.
    pub fn new(dialect: WireDialect) -> HttpModelTransport {
        HttpModelTransport {
            rw_ms: dialect.stream_idle_timeout_ms.clamp(30_000, 600_000),
            dialect,
            connect_ms: 5_000,
            max_body: 16 * 1024 * 1024,
        }
    }
}

impl Transport for HttpModelTransport {
    fn send(
        &mut self,
        bytes: &[u8],
        _credential: &CredentialHandle,
        endpoint_ref: &str,
    ) -> Result<Vec<(u64, WireFrame)>, ModelError> {
        // The serialized plan carries the request body verbatim
        // (`plan.body` — the dialect's canonical render; `capabilities.
        // streaming` selects SSE handling).
        let plan =
            hh_wire::json::parse(std::str::from_utf8(bytes).map_err(|_| {
                ModelError::new(ModelErrorClass::StreamDecode, "plan bytes not utf8")
            })?)
            .map_err(|e| ModelError::new(ModelErrorClass::StreamDecode, format!("plan: {e}")))?;
        let body = plan.get("body").cloned().unwrap_or(Json::Null);
        let streaming = body
            .get("stream")
            .map(|s| matches!(s, Json::Bool(true)))
            .unwrap_or(false);
        let body_bytes = body.to_canonical_string().into_bytes();

        let url =
            http::parse_http_url(endpoint_ref).map_err(|e| map_io(e, TimeoutKind::Connect))?;
        let mut sock = http::connect(&url, self.connect_ms, self.rw_ms)
            .map_err(|e| map_io(e, TimeoutKind::Connect))?;
        let target = join(&url.target, "chat/completions");
        let accept = if streaming {
            "text/event-stream"
        } else {
            "application/json"
        };
        let host_header = url.host_header();
        let headers = [
            ("Host", host_header.as_str()),
            ("Content-Type", "application/json"),
            ("Accept", accept),
            ("Connection", "close"),
        ];
        http::write_request(&mut sock, "POST", &target, &headers, &body_bytes)
            .map_err(|e| map_io(e, TimeoutKind::Attempt))?;
        let t0 = std::time::Instant::now();
        let at = || t0.elapsed().as_millis() as u64;
        let mut r = BufReader::new(sock);
        let head = http::read_response_head(&mut r).map_err(|e| map_io(e, TimeoutKind::Attempt))?;
        if !(200..300).contains(&head.status) {
            // Error path — the body is the provider's error envelope.
            let raw = http::read_body(&mut r, &head, self.max_body)
                .map_err(|e| map_io(e, TimeoutKind::Attempt))?;
            let body_j = hh_wire::json::parse_provider(&String::from_utf8_lossy(&raw))
                .unwrap_or_else(|_| {
                    Json::obj([(
                        "error",
                        Json::obj([(
                            "message",
                            Json::str(String::from_utf8_lossy(&raw).into_owned()),
                        )]),
                    )])
                });
            let mut err = codec::classify_http_error(&self.dialect, head.status as u32, &body_j);
            // `retry_after_sources` — header spelling (seconds per RFC —
            // `read_retry_after` caps at `retry_after_cap_ms`, R-RT-4).
            if let Some(v) = head
                .headers
                .get("retry-after")
                .and_then(|s| s.parse::<i64>().ok())
            {
                let headers_j = Json::obj([("retry-after", Json::Int(v))]);
                err.retry_after_ms =
                    codec::read_retry_after(&self.dialect, &body_j, Some(&headers_j));
            }
            return Err(err);
        }

        let is_sse = head
            .headers
            .get("content-type")
            .is_some_and(|v| v.contains("text/event-stream"));

        let mut frames: Vec<(u64, WireFrame)> = Vec::new();
        let mut tr = ChunkTranslator::default();
        if is_sse {
            let mut sse = SseDecoder::new(self.max_body);
            let mut rd = BodyReader::new(&mut r, &head, self.max_body);
            let mut done = false;
            loop {
                match rd.next_chunk() {
                    Ok(Some(bytes)) => {
                        let stamp = at();
                        for ev in sse.push(&bytes)? {
                            match ev {
                                SseEvent::Data(data) => {
                                    let chunk =
                                        hh_wire::json::parse_provider(&data).map_err(|e| {
                                            ModelError::new(
                                                ModelErrorClass::StreamDecode,
                                                format!("sse chunk: {e}"),
                                            )
                                        })?;
                                    for f in tr.push_chunk(&chunk)? {
                                        frames.push((stamp, f));
                                    }
                                }
                                SseEvent::Done => {
                                    for f in tr.finish() {
                                        frames.push((stamp, f));
                                    }
                                    done = true;
                                }
                            }
                        }
                        if done || sse.terminated() {
                            break;
                        }
                    }
                    Ok(None) => {
                        for f in tr.finish() {
                            frames.push((at(), f));
                        }
                        break;
                    }
                    Err(e) => return Err(map_io(e, TimeoutKind::StreamIdle)),
                }
            }
        } else {
            // Non-streaming `chat.completion` body (or a `stream:true`
            // request the endpoint answered plainly — read by
            // content-type, never by assumption).
            let raw = http::read_body(&mut r, &head, self.max_body)
                .map_err(|e| map_io(e, TimeoutKind::Attempt))?;
            let doc = hh_wire::json::parse_provider(&String::from_utf8_lossy(&raw))
                .map_err(|e| ModelError::new(ModelErrorClass::StreamDecode, format!("{e}")))?;
            let stamp = at();
            for f in openai::completion_frames(&doc)? {
                frames.push((stamp, f));
            }
        }
        Ok(frames)
    }

    /// `discover(endpoint_ref)` — `GET {endpoint}/models` → the listing
    /// envelope verbatim (the dialect's `model_listing` member names the
    /// list member; callers read `["data"]` — observed, never fabricated).
    fn discover(&mut self, endpoint_ref: &str) -> Result<Json, ModelError> {
        let url =
            http::parse_http_url(endpoint_ref).map_err(|e| map_io(e, TimeoutKind::Connect))?;
        let mut sock = http::connect(&url, self.connect_ms, self.rw_ms)
            .map_err(|e| map_io(e, TimeoutKind::Connect))?;
        let target = join(&url.target, "models");
        let host_header = url.host_header();
        let headers = [
            ("Host", host_header.as_str()),
            ("Accept", "application/json"),
            ("Connection", "close"),
        ];
        http::write_request(&mut sock, "GET", &target, &headers, &[])
            .map_err(|e| map_io(e, TimeoutKind::Attempt))?;
        let mut r = BufReader::new(sock);
        let head = http::read_response_head(&mut r).map_err(|e| map_io(e, TimeoutKind::Attempt))?;
        let raw = http::read_body(&mut r, &head, self.max_body)
            .map_err(|e| map_io(e, TimeoutKind::Attempt))?;
        let doc = hh_wire::json::parse_provider(&String::from_utf8_lossy(&raw))
            .map_err(|e| ModelError::new(ModelErrorClass::StreamDecode, format!("{e}")))?;
        if !(200..300).contains(&head.status) {
            return Err(codec::classify_http_error(
                &self.dialect,
                head.status as u32,
                &doc,
            ));
        }
        Ok(doc)
    }
}
