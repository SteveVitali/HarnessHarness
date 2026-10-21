//! `http_transport` — `HttpModelTransport`, the live `Transport` half
//! (R4.5b; ADR-0364 D1/D3; DF-S1.18-1's local-family leg; R4.5c TLS).
//!
//! HTTP/1.1 over `TcpStream` — plain or TLS-terminated — through the
//! **one** codec home `hh_wire::http` (CC1; the `hh-env` egress leg and
//! `hh-mcp` boundary ride the same module). An `https` endpoint_ref
//! rides a `hh_tls::ClientTls` session beneath the same codec
//! (R4.5c/ADR-0365 — webpki verification always on, extra roots only
//! via the lane's explicit file); `tls_pending` is retired — the lane
//! never downgrades.
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

use std::io::{BufReader, Read, Write};

use hh_tls::{ClientTls, TlsError};
use hh_wire::http::{self, BodyReader, HttpError, Scheme};
use hh_wire::json::Json;

use crate::anthropic::{self, MessagesTranslator};
use crate::codec::{self, WireFrame};
use crate::credentials::header_secret;
use crate::dialect::WireDialect;
use crate::gateway::{CredentialHandle, Transport};
use crate::openai::{self, ChunkTranslator, SseDecoder, SseEvent};
use crate::vocab::{ModelError, ModelErrorClass, TimeoutKind};
use hh_secrets::{CompositeResolver, SecretSourceResolver};

/// The request-path join for the family's operations — `endpoint_ref`'s
/// path is the API base (`/v1`, or `/`); the operation path appends.
fn join(base_target: &str, op: &str) -> String {
    let base = base_target.trim_end_matches('/');
    format!("{base}/{op}")
}

/// The dialect's provider family — the wire shape a bound dialect's bytes
/// follow (operation path, SSE payload grammar, auth-header form). Keyed
/// on the dialect *id* — the document's own identity, never a model id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Chat Completions (`chat/completions` op, `chat.completion.chunk`
    /// SSE payloads, `Authorization: Bearer`).
    ChatCompletions,
    /// Anthropic Messages (`messages` op, Messages SSE event payloads,
    /// `x-api-key` + `anthropic-version`).
    Messages,
}

/// `family_of(dialect)` — the family a loaded dialect document declares
/// through its `dialect_id`.
pub fn family_of(dialect: &WireDialect) -> Family {
    match dialect.dialect_id.as_str() {
        "anthropic-messages" => Family::Messages,
        _ => Family::ChatCompletions,
    }
}

/// `Translator` — the family's SSE payload → `WireFrame` fold, one
/// spelling per family (the `data:` payload document goes in; normalized
/// frames come out; [`Translator::finish`] emits the terminal at the
/// family's own end-of-stream).
enum Translator {
    /// `chat.completion.chunk` payloads.
    Chat(ChunkTranslator),
    /// Messages event payloads.
    Messages(MessagesTranslator),
}

impl Translator {
    fn push(&mut self, doc: &Json) -> Result<Vec<WireFrame>, ModelError> {
        match self {
            Translator::Chat(t) => t.push_chunk(doc),
            Translator::Messages(t) => t.push_event(doc),
        }
    }

    fn finish(&mut self) -> Vec<WireFrame> {
        match self {
            Translator::Chat(t) => t.finish(),
            Translator::Messages(t) => t.finish(),
        }
    }

    fn for_family(family: Family) -> Translator {
        match family {
            Family::ChatCompletions => Translator::Chat(ChunkTranslator::default()),
            Family::Messages => Translator::Messages(MessagesTranslator::default()),
        }
    }
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
            // Outside http/https — a typed refusal, never a downgrade.
            let mut err = ModelError::new(
                ModelErrorClass::UnsupportedFeature,
                format!("scheme {s} unsupported"),
            );
            err.provider_code = Some("unsupported_scheme".to_string());
            err
        }
        other => ModelError::new(
            ModelErrorClass::StreamDecode,
            format!("http codec: {other}"),
        ),
    }
}

/// `TlsError → ModelError` — the handshake/config refusal classes land on
/// the boundary's typed set: a failed handshake is `network` (the peer
/// verification refused — the honest shape; never a downgrade), a bad
/// lane configuration is `unsupported_feature{tls_config}` (the typed
/// arm-time defect).
fn map_tls(e: TlsError) -> ModelError {
    match e {
        TlsError::Handshake(d) => ModelError::new(
            ModelErrorClass::Network,
            format!("tls handshake refused: {d}"),
        ),
        TlsError::Io(d) => ModelError::new(ModelErrorClass::Network, format!("tls io: {d}")),
        other => {
            let mut err = ModelError::new(
                ModelErrorClass::UnsupportedFeature,
                format!("tls config: {other}"),
            );
            err.provider_code = Some("tls_config".to_string());
            err
        }
    }
}

/// `ReadWrite` — the trait object the codec's stream halves ride:
/// `TcpStream` on `http`, `hh_tls::TlsStream` on `https` (one codec,
/// streams beneath it — ADR-0364 D1).
trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

/// `HttpModelTransport` — the live `Transport` for `http://`/`https://`
/// model endpoints. `dialect` is held for `classify_http_error`/
/// `read_retry_after` (the boundary's own rules, not re-declared); `tls`
/// is the R4.5c client config (webpki roots by default — an `https:`
/// lane with no extra roots just verifies against the public set).
pub struct HttpModelTransport {
    /// The bound dialect (error classification + retry-after + body bounds
    /// derive from it).
    dialect: WireDialect,
    /// The provider family the dialect's bytes follow (operation path,
    /// SSE payload grammar, auth-header form — derived at bind).
    family: Family,
    /// The client TLS configuration (R4.5c; ADR-0365 — verification
    /// always on; extra roots only via the lane's explicit file).
    tls: ClientTls,
    /// The credential resolver the auth header's value is resolved
    /// through *inside* `send` (R4.5d; ADR-0366 D4 — the value never
    /// leaves this call's scope).
    credentials: Box<dyn SecretSourceResolver>,
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
    /// (clamped to ≥ 30 s), 16 MiB bodies, public webpki roots for TLS,
    /// the launch-env credential composition.
    pub fn new(dialect: WireDialect) -> HttpModelTransport {
        Self::with_tls(dialect, ClientTls::webpki_roots())
    }

    /// `with_tls(dialect, tls)` — the lane's TLS configuration (the
    /// local-CA extra-roots hook and/or the mTLS client identity the
    /// `model_port` slot's `tls{}` params resolved — ADR-0365).
    pub fn with_tls(dialect: WireDialect, tls: ClientTls) -> HttpModelTransport {
        Self::with_credentials(dialect, tls, Box::<CompositeResolver>::default())
    }

    /// `with_credentials(dialect, tls, resolver)` — the transport with an
    /// explicit credential resolver (tests pin a `StaticVault`; the lane
    /// rides the launch-env composition the default arms).
    pub fn with_credentials(
        dialect: WireDialect,
        tls: ClientTls,
        credentials: Box<dyn SecretSourceResolver>,
    ) -> HttpModelTransport {
        HttpModelTransport {
            family: family_of(&dialect),
            rw_ms: dialect.stream_idle_timeout_ms.clamp(30_000, 600_000),
            dialect,
            tls,
            credentials,
            connect_ms: 5_000,
            max_body: 16 * 1024 * 1024,
        }
    }

    /// The auth header the family's `send` attaches for a `provided`
    /// credential — the value resolved inside this call and dropped on
    /// return (`(header-name, header-value)`; the value never lands in a
    /// frame, a log, or a record).
    fn auth_header(
        &self,
        credential: &CredentialHandle,
    ) -> Result<Option<(String, String)>, ModelError> {
        if !credential.provided {
            return Ok(None);
        }
        let secret = header_secret(&*self.credentials, credential)?;
        Ok(Some(match self.family {
            Family::Messages => ("x-api-key".to_string(), secret),
            Family::ChatCompletions => ("Authorization".to_string(), format!("Bearer {secret}")),
        }))
    }

    /// `dial(url)` — the transport beneath the codec: TCP on `http`,
    /// TCP+TLS on `https` (the handshake's refusal is typed — never a
    /// plaintext fallback).
    fn dial(&self, url: &http::HttpUrl) -> Result<Box<dyn ReadWrite>, ModelError> {
        let sock = http::connect(url, self.connect_ms, self.rw_ms)
            .map_err(|e| map_io(e, TimeoutKind::Connect))?;
        match url.scheme {
            Scheme::Http => Ok(Box::new(sock)),
            Scheme::Https => Ok(Box::new(
                self.tls.connect(sock, &url.host).map_err(map_tls)?,
            )),
        }
    }
}

impl Transport for HttpModelTransport {
    fn send(
        &mut self,
        bytes: &[u8],
        credential: &CredentialHandle,
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
        let mut sock = self.dial(&url)?;
        let target = join(
            &url.target,
            match self.family {
                Family::Messages => "messages",
                Family::ChatCompletions => "chat/completions",
            },
        );
        let accept = if streaming {
            "text/event-stream"
        } else {
            "application/json"
        };
        // The auth header is resolved + attached inside this call — the
        // credential value's lifetime is this `send`; it never lands in a
        // frame, a record, or a log (R4.5d; ADR-0366 D4).
        let auth = self.auth_header(credential)?;
        let host_header = url.host_header();
        let mut headers: Vec<(&str, &str)> = vec![
            ("Host", host_header.as_str()),
            ("Content-Type", "application/json"),
            ("Accept", accept),
            ("Connection", "close"),
        ];
        let auth_pair;
        if let Some((name, value)) = &auth {
            auth_pair = (name.as_str(), value.as_str());
            headers.push(auth_pair);
        }
        if self.family == Family::Messages {
            // The Messages API's version header is required on every call.
            headers.push(("anthropic-version", "2023-06-01"));
        }
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
            // The Messages family's `error.type` is the class signal —
            // the adapter copies it to `error.code` for the shared rules.
            let body_j = match self.family {
                Family::Messages => anthropic::error_body_for_classify(&body_j),
                Family::ChatCompletions => body_j,
            };
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
        let mut tr = Translator::for_family(self.family);
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
                                    for f in tr.push(&chunk)? {
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
            for f in match self.family {
                Family::Messages => anthropic::completion_frames(&doc)?,
                Family::ChatCompletions => openai::completion_frames(&doc)?,
            } {
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
        let mut sock = self.dial(&url)?;
        let target = join(&url.target, "models");
        let host_header = url.host_header();
        let mut headers: Vec<(&str, &str)> = vec![
            ("Host", host_header.as_str()),
            ("Accept", "application/json"),
            ("Connection", "close"),
        ];
        if self.family == Family::Messages {
            // The Messages API's version header is required on every call.
            headers.push(("anthropic-version", "2023-06-01"));
        }
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
