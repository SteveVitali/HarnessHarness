//! `http_transport` — `HttpModelTransport`, the live `Transport` half
//! (R4.5b; ADR-0364 D1/D3; DF-S1.18-1's local-family leg; R4.5c TLS).
//!
//! HTTP/1.1 over `TcpStream` — plain or TLS-terminated — through the
//! **one** codec home `hharness_wire::http` (CC1; the `hharness-env` egress leg and
//! `hharness-mcp` boundary ride the same module). An `https` endpoint_ref
//! rides a `hharness_tls::ClientTls` session beneath the same codec
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

use hharness_tls::{ClientTls, TlsError};
use hharness_wire::http::{self, BodyReader, HttpError, Scheme};
use hharness_wire::json::Json;

use crate::anthropic::{self, MessagesTranslator};
use crate::codec::{self, WireFrame};
use crate::credentials::header_secret;
use crate::dialect::WireDialect;
use crate::gateway::{CredentialHandle, Transport};
use crate::openai::{self, ChunkTranslator, SseDecoder, SseEvent};
use crate::vocab::{ModelError, ModelErrorClass, TimeoutKind};
use hharness_secrets::{CompositeResolver, SecretSourceResolver};

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
/// `TcpStream` on `http`, `hharness_tls::TlsStream` on `https` (one codec,
/// streams beneath it — ADR-0364 D1).
trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

/// `ProbeObservation` — the raw-fact bundle one measured probe call
/// collects over the provider's *own* documents (R4.5e): members scanned
/// before normalization so `cap.snapshot_id_exposed` /
/// `cap.cache_state_visible` read what the wire actually carried, never
/// what the translator kept.
#[derive(Debug, Clone)]
struct ProbeObservation {
    /// The response answered `text/event-stream`.
    sse: bool,
    /// Any response document carried a snapshot/fingerprint member
    /// (`system_fingerprint`/`fingerprint`).
    snapshot_member: bool,
    /// Any usage object carried a cache member (`cached_tokens` /
    /// `cache_*_input_tokens` / `prompt_tokens_details.cached_tokens`).
    cache_member: bool,
    /// The largest observed input-token count across usage objects.
    input_tokens: u64,
    /// The largest observed output-token count across usage objects.
    output_tokens: u64,
}

/// `probe_observe(doc, o)` — fold one raw provider document's members
/// into the observation (usage objects read at `usage` or Messages'
/// `message.usage`; token counts take the largest value seen — the
/// stream's terminal usage is cumulative per family).
fn probe_observe(doc: &Json, o: &mut ProbeObservation) {
    if doc.get("system_fingerprint").is_some() || doc.get("fingerprint").is_some() {
        o.snapshot_member = true;
    }
    let int = |j: &Json, k: &str| j.get(k).and_then(Json::as_int).unwrap_or(0).max(0) as u64;
    let usages = [
        doc.get("usage").cloned(),
        doc.get("message").and_then(|m| m.get("usage")).cloned(),
    ];
    for u in usages.iter().flatten() {
        if u.get("cache_read_input_tokens").is_some()
            || u.get("cache_creation_input_tokens").is_some()
            || u.get("cache_creation_tokens").is_some()
            || u.get("cached_tokens").is_some()
            || u.get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .is_some()
        {
            o.cache_member = true;
        }
        o.input_tokens = o
            .input_tokens
            .max(int(u, "input_tokens"))
            .max(int(u, "prompt_tokens"));
        o.output_tokens = o
            .output_tokens
            .max(int(u, "output_tokens"))
            .max(int(u, "completion_tokens"));
    }
}

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
    /// The cached probe-call observation, keyed `(endpoint, model)` — one
    /// measured tiny call serves `cap.streaming` /
    /// `cap.snapshot_id_exposed` / `cap.cache_state_visible`; the probe
    /// never re-bills a call per capability (R4.5e).
    probe_cache: Option<(String, Result<ProbeObservation, ModelError>)>,
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
            probe_cache: None,
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

    /// `error_reply(head, raw)` — the shared non-2xx classification a
    /// probe request reuses (the provider's error envelope → the typed
    /// `ModelError`, never a fabricated probe value).
    fn error_reply(&self, head: &http::ResponseHead, raw: &[u8]) -> ModelError {
        let body_j = hharness_wire::json::parse_provider(&String::from_utf8_lossy(raw))
            .unwrap_or_else(|_| {
                Json::obj([(
                    "error",
                    Json::obj([(
                        "message",
                        Json::str(String::from_utf8_lossy(raw).into_owned()),
                    )]),
                )])
            });
        let body_j = match self.family {
            Family::Messages => anthropic::error_body_for_classify(&body_j),
            Family::ChatCompletions => body_j,
        };
        codec::classify_http_error(&self.dialect, head.status as u32, &body_j)
    }

    /// `probe_call(credential, endpoint_ref, model)` — the one measured
    /// tiny streaming call the claim-shaping capability probes share
    /// (`max_tokens:1`/`max_completion_tokens:8`, `stream:true` +
    /// `include_usage`). The observation is cached per `(endpoint,
    /// model)` — the conformance walk never re-bills a call.
    fn probe_call(
        &mut self,
        credential: &CredentialHandle,
        endpoint_ref: &str,
        model: &str,
    ) -> Result<ProbeObservation, ModelError> {
        let key = format!("{endpoint_ref}|{model}");
        if let Some((k, cached)) = &self.probe_cache {
            if *k == key {
                return cached.clone();
            }
        }
        let out = self.probe_call_once(credential, endpoint_ref, model);
        self.probe_cache = Some((key, out.clone()));
        out
    }

    /// The cached probe call's measured usage (`None` before any probe
    /// call) — the probe leg's spend record reads it, never an estimate.
    pub fn probe_usage(&self) -> Option<Json> {
        self.probe_cache.as_ref().and_then(|(_, r)| {
            r.as_ref().ok().map(|o| {
                Json::obj([
                    ("input_tokens", Json::Int(o.input_tokens as i64)),
                    ("output_tokens", Json::Int(o.output_tokens as i64)),
                    ("calls", Json::Int(1)),
                ])
            })
        })
    }

    /// `probe_count_tokens` — the Messages family's
    /// `POST /messages/count_tokens` primitive (the dialect's
    /// `count_tokens` member names it; a family without the member is
    /// `UnsupportedFeature` — `run_transport_probes` reads `skipped`).
    fn probe_count_tokens(
        &mut self,
        credential: &CredentialHandle,
        endpoint_ref: &str,
        model: &str,
    ) -> Result<Json, ModelError> {
        if self.family != Family::Messages {
            return Err(ModelError::new(
                ModelErrorClass::UnsupportedFeature,
                "count_tokens probe has no Chat Completions primitive",
            ));
        }
        let body = Json::obj([
            ("model", Json::str(model)),
            (
                "messages",
                Json::Arr(vec![Json::obj([
                    ("role", Json::str("user")),
                    ("content", Json::str("hh.probe")),
                ])]),
            ),
        ])
        .to_canonical_string()
        .into_bytes();
        let doc = self.probe_post(credential, endpoint_ref, "messages/count_tokens", &body)?;
        Ok(doc)
    }

    /// The raw `POST {endpoint}/{op}` leg probe requests share —
    /// `{status, doc}` on success, the dialect's typed refusal class on a
    /// non-2xx.
    fn probe_post(
        &mut self,
        credential: &CredentialHandle,
        endpoint_ref: &str,
        op: &str,
        body: &[u8],
    ) -> Result<Json, ModelError> {
        self.post_op(credential, endpoint_ref, op, body, &[])
    }

    /// `post_op(credential, endpoint_ref, op, body, extra_headers)` — the
    /// raw single-shot `POST {endpoint}/{op}` primitive the non-chat
    /// operations ride (R4.7b: `embeddings`, the Messages `compaction`
    /// request, `messages/count_tokens`). `extra_headers` are literal
    /// `(name, value)` pairs appended after the family's own headers (the
    /// `anthropic-beta` a beta-gated Messages op needs) — call sites spell
    /// them, this transport never invents them. The credential's value is
    /// resolved inside this call for the auth header and dropped on
    /// return (ADR-0366 D4); a non-2xx answers the dialect's typed
    /// refusal class, never a fabricated document.
    pub fn post_op(
        &mut self,
        credential: &CredentialHandle,
        endpoint_ref: &str,
        op: &str,
        body: &[u8],
        extra_headers: &[(&str, &str)],
    ) -> Result<Json, ModelError> {
        let url =
            http::parse_http_url(endpoint_ref).map_err(|e| map_io(e, TimeoutKind::Connect))?;
        let mut sock = self.dial(&url)?;
        let target = join(&url.target, op);
        let host_header = url.host_header();
        let mut headers: Vec<(&str, &str)> = vec![
            ("Host", host_header.as_str()),
            ("Content-Type", "application/json"),
            ("Accept", "application/json"),
            ("Connection", "close"),
        ];
        let auth_pair;
        if let Some((name, value)) = self.auth_header(credential)? {
            auth_pair = (name, value);
            headers.push((auth_pair.0.as_str(), auth_pair.1.as_str()));
        }
        if self.family == Family::Messages {
            headers.push(("anthropic-version", "2023-06-01"));
        }
        headers.extend_from_slice(extra_headers);
        http::write_request(&mut sock, "POST", &target, &headers, body)
            .map_err(|e| map_io(e, TimeoutKind::Attempt))?;
        let mut r = BufReader::new(sock);
        let head = http::read_response_head(&mut r).map_err(|e| map_io(e, TimeoutKind::Attempt))?;
        let raw = http::read_body(&mut r, &head, self.max_body)
            .map_err(|e| map_io(e, TimeoutKind::Attempt))?;
        if !(200..300).contains(&head.status) {
            return Err(self.error_reply(&head, &raw));
        }
        hharness_wire::json::parse_provider(&String::from_utf8_lossy(&raw))
            .map_err(|e| ModelError::new(ModelErrorClass::StreamDecode, format!("{e}")))
    }

    /// `probe_call_once` — issue the family's tiny streaming call and
    /// collect the raw-member observation (SSE `data:` documents or the
    /// one completion document — read by content-type, never assumed).
    fn probe_call_once(
        &mut self,
        credential: &CredentialHandle,
        endpoint_ref: &str,
        model: &str,
    ) -> Result<ProbeObservation, ModelError> {
        let mut o = ProbeObservation {
            sse: false,
            snapshot_member: false,
            cache_member: false,
            input_tokens: 0,
            output_tokens: 0,
        };
        let body = match self.family {
            Family::Messages => Json::obj([
                ("model", Json::str(model)),
                ("max_tokens", Json::Int(1)),
                ("stream", Json::Bool(true)),
                (
                    "messages",
                    Json::Arr(vec![Json::obj([
                        ("role", Json::str("user")),
                        ("content", Json::str("hh.probe")),
                    ])]),
                ),
            ]),
            Family::ChatCompletions => Json::obj([
                ("model", Json::str(model)),
                ("max_completion_tokens", Json::Int(8)),
                ("stream", Json::Bool(true)),
                (
                    "stream_options",
                    Json::obj([("include_usage", Json::Bool(true))]),
                ),
                (
                    "messages",
                    Json::Arr(vec![Json::obj([
                        ("role", Json::str("user")),
                        ("content", Json::str("hh.probe")),
                    ])]),
                ),
            ]),
        }
        .to_canonical_string()
        .into_bytes();
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
        let host_header = url.host_header();
        let mut headers: Vec<(&str, &str)> = vec![
            ("Host", host_header.as_str()),
            ("Content-Type", "application/json"),
            ("Accept", "text/event-stream"),
            ("Connection", "close"),
        ];
        let auth_pair;
        if let Some((name, value)) = self.auth_header(credential)? {
            auth_pair = (name, value);
            headers.push((auth_pair.0.as_str(), auth_pair.1.as_str()));
        }
        if self.family == Family::Messages {
            headers.push(("anthropic-version", "2023-06-01"));
        }
        http::write_request(&mut sock, "POST", &target, &headers, &body)
            .map_err(|e| map_io(e, TimeoutKind::Attempt))?;
        let mut r = BufReader::new(sock);
        let head = http::read_response_head(&mut r).map_err(|e| map_io(e, TimeoutKind::Attempt))?;
        o.sse = head
            .headers
            .get("content-type")
            .is_some_and(|v| v.contains("text/event-stream"));
        if !(200..300).contains(&head.status) {
            let raw = http::read_body(&mut r, &head, self.max_body)
                .map_err(|e| map_io(e, TimeoutKind::Attempt))?;
            return Err(self.error_reply(&head, &raw));
        }
        if o.sse {
            let mut sse = SseDecoder::new(self.max_body);
            let mut rd = BodyReader::new(&mut r, &head, self.max_body);
            loop {
                match rd.next_chunk() {
                    Ok(Some(bytes)) => {
                        for ev in sse.push(&bytes)? {
                            if let SseEvent::Data(data) = ev {
                                if let Ok(doc) = hharness_wire::json::parse_provider(&data) {
                                    probe_observe(&doc, &mut o);
                                }
                            }
                        }
                        if sse.terminated() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => return Err(map_io(e, TimeoutKind::StreamIdle)),
                }
            }
        } else {
            let raw = http::read_body(&mut r, &head, self.max_body)
                .map_err(|e| map_io(e, TimeoutKind::Attempt))?;
            let doc = hharness_wire::json::parse_provider(&String::from_utf8_lossy(&raw))
                .map_err(|e| ModelError::new(ModelErrorClass::StreamDecode, format!("{e}")))?;
            probe_observe(&doc, &mut o);
        }
        Ok(o)
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
            hharness_wire::json::parse(std::str::from_utf8(bytes).map_err(|_| {
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
            let body_j = hharness_wire::json::parse_provider(&String::from_utf8_lossy(&raw))
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
                                    let chunk = hharness_wire::json::parse_provider(&data)
                                        .map_err(|e| {
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
            let doc = hharness_wire::json::parse_provider(&String::from_utf8_lossy(&raw))
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

    /// `discover(credential, endpoint_ref)` — `GET {endpoint}/models` →
    /// the listing envelope verbatim (the dialect's `model_listing`
    /// member names the list member; callers read `["data"]` — observed,
    /// never fabricated). The credential's auth header attaches exactly
    /// as `send`'s does (the hosted listing refuses an anonymous read).
    fn discover(
        &mut self,
        credential: &CredentialHandle,
        endpoint_ref: &str,
    ) -> Result<Json, ModelError> {
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
        let auth_pair;
        if let Some((name, value)) = self.auth_header(credential)? {
            auth_pair = (name, value);
            headers.push((auth_pair.0.as_str(), auth_pair.1.as_str()));
        }
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
        let doc = hharness_wire::json::parse_provider(&String::from_utf8_lossy(&raw))
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

    /// `probe_capability(capability, credential, endpoint_ref, model)` —
    /// the §5b.3 live probes (R4.5e): `cap.model_listing` rides
    /// `discover`; `cap.count_tokens` rides the Messages count primitive;
    /// `cap.streaming` / `cap.snapshot_id_exposed` /
    /// `cap.cache_state_visible` read the one cached tiny probe call's
    /// raw-member observation. A capability with no primitive is
    /// `UnsupportedFeature` — the conformance row records `skipped`.
    fn probe_capability(
        &mut self,
        capability: &str,
        credential: &CredentialHandle,
        endpoint_ref: &str,
        model: &str,
    ) -> Result<Json, ModelError> {
        let unsupported = || {
            ModelError::new(
                ModelErrorClass::UnsupportedFeature,
                format!("no probe primitive for {capability}"),
            )
        };
        match capability {
            "cap.model_listing" => {
                if self.dialect.model_listing.is_none() {
                    return Err(unsupported());
                }
                self.discover(credential, endpoint_ref)?;
                Ok(Json::Bool(true))
            }
            "cap.count_tokens" => {
                if self.dialect.count_tokens.is_none() {
                    return Err(unsupported());
                }
                self.probe_count_tokens(credential, endpoint_ref, model)?;
                Ok(Json::Bool(true))
            }
            "cap.streaming" => Ok(Json::Bool(
                self.probe_call(credential, endpoint_ref, model)?.sse,
            )),
            "cap.snapshot_id_exposed" => Ok(Json::Bool(
                self.probe_call(credential, endpoint_ref, model)?
                    .snapshot_member,
            )),
            "cap.cache_state_visible" => Ok(Json::Bool(
                self.probe_call(credential, endpoint_ref, model)?
                    .cache_member,
            )),
            _ => Err(unsupported()),
        }
    }
}
