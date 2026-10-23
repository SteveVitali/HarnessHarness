//! `oauth` — the WS-H3 mediator's real provider exchange (R4.7a —
//! DF-S4.11-2, the 2026-10-20 "GitHub OAuth App" ruling; ADR-0351 D5's
//! fired trigger).
//!
//! Two halves ride one HTTPS stack:
//!
//! - **the device-flow issue leg** — [`DeviceFlow`] carries the
//!   provider's three endpoints (`github()` pins the OAuth-App shape:
//!   `POST /login/device/code` → the operator's `user_code` +
//!   `verification_uri`, `POST /login/oauth/access_token` polling with
//!   `authorization_pending`/`slow_down`/`expired_token`/`access_denied`
//!   handling, `GET /user` → the provider-verified subject `login`).
//!   The access token is consumed inside the exchange — only the
//!   subject returns; the token never enters a record, ledger or log
//!   (the broker-side custody answer: it lives and dies in the leg's
//!   own scope).
//! - **the mediator's verify leg** — [`OAuthIssuer::verify_bearer`]
//!   asks the declared issuer who a presented bearer names (`GET
//!   /user`): `2xx` + `login` is the credential proof; `401`/`403` is a
//!   typed `Invalid`; an unreachable issuer is `Unreachable` — never a
//!   guessed subject.
//!
//! The transport seam is [`Transport`]: `Tls` dials through the
//! admitted `hh-tls` client (re-exported via `hh_embed::service` —
//! ADR-0365 D3's consumer set stays closed), `Loopback` is the hermetic
//! issuer fixture's dial and refuses any non-loopback host (the
//! fixture path can never be pointed at a real endpoint). Both ride the
//! one `hh_wire::http` codec — no second HTTP stack (ADR-0364 D1).

use std::io::{BufReader, Read, Write};
use std::time::{Duration, Instant};

use hh_embed::service::ClientTls;
use hh_wire::http::{self, HttpUrl};
use hh_wire::json::Json;

/// `Read + Write` — the codec's stream shape (std names no joint trait,
/// so the seam names one).
pub trait Io: Read + Write {}
impl<T: Read + Write> Io for T {}

/// The wire transport a leg dials — `Tls` terminates the real session
/// through `hh-tls` (webpki verification always on); `Loopback` is the
/// fixture dial, restricted to loopback hosts so a fixture wiring can
/// never silently address a live endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// Real TLS (`hh_tls::ClientTls::webpki_roots()` — always verified).
    Tls,
    /// Plain TCP to `127.0.0.1`/`localhost`/`::1` only — the hermetic
    /// issuer fixture.
    Loopback,
}

/// The exchange's typed failure — `Transport` (dial/TLS/IO/deadline),
/// `Http` (a non-2xx answer carrying its status), `Refused` (the
/// provider's own `error` member or a seam rule), `Malformed` (a body
/// that doesn't carry the contract's members). Never a guessed result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OAuthError {
    /// Dial/TLS/IO/deadline failure.
    Transport(String),
    /// The endpoint answered a non-2xx status.
    Http(u16, String),
    /// The provider's typed `error` member or a seam rule refusal.
    Refused(String),
    /// The answer's body lacks the contract's members.
    Malformed(String),
}

impl std::fmt::Display for OAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OAuthError::Transport(d) => write!(f, "oauth transport: {d}"),
            OAuthError::Http(s, d) => write!(f, "oauth http {s}: {d}"),
            OAuthError::Refused(d) => write!(f, "oauth refused: {d}"),
            OAuthError::Malformed(d) => write!(f, "oauth malformed: {d}"),
        }
    }
}
impl std::error::Error for OAuthError {}

fn is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

/// `open(transport, url)` — the one dial every leg shares. `Loopback`
/// refuses non-loopback hosts typed (the fixture seam's boundary);
/// `Tls` requires the `https` scheme — an `http` URL under `Tls` is a
/// typed refusal, never a silent plaintext downgrade.
pub fn open(t: Transport, url: &HttpUrl) -> Result<Box<dyn Io>, OAuthError> {
    match t {
        Transport::Loopback => {
            if !is_loopback(&url.host) {
                return Err(OAuthError::Refused(format!(
                    "loopback transport refuses non-loopback host `{}`",
                    url.host
                )));
            }
            let sock = http::connect(url, 5_000, 15_000)
                .map_err(|e| OAuthError::Transport(format!("connect: {e:?}")))?;
            Ok(Box::new(sock))
        }
        Transport::Tls => {
            if url.scheme != http::Scheme::Https {
                return Err(OAuthError::Refused(format!(
                    "tls transport refuses non-https url `{}`",
                    url.host
                )));
            }
            let sock = http::connect(url, 15_000, 30_000)
                .map_err(|e| OAuthError::Transport(format!("connect: {e:?}")))?;
            let st = ClientTls::webpki_roots()
                .connect(sock, &url.host)
                .map_err(|e| OAuthError::Transport(format!("tls: {e:?}")))?;
            Ok(Box::new(st))
        }
    }
}

/// One round-trip under the codec — `write_request` + `read_response_full`.
fn http_call(
    t: Transport,
    url: &str,
    method: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<(u16, Json), OAuthError> {
    let u = http::parse_http_url(url).map_err(|e| OAuthError::Malformed(format!("url: {e:?}")))?;
    let mut st = open(t, &u)?;
    let host = u.host_header();
    let target = u.target.clone();
    let mut hs: Vec<(&str, &str)> = vec![
        ("Host", host.as_str()),
        ("User-Agent", "hh-r4.7a-mediator"),
        ("Accept", "application/json"),
    ];
    hs.extend_from_slice(headers);
    http::write_request(&mut st, method, &target, &hs, body)
        .map_err(|e| OAuthError::Transport(format!("write: {e:?}")))?;
    let mut br = BufReader::new(st);
    let (head, raw) = http::read_response_full(&mut br, 1 << 20)
        .map_err(|e| OAuthError::Transport(format!("read: {e:?}")))?;
    let j = hh_wire::json::parse(&String::from_utf8_lossy(&raw)).unwrap_or(Json::Null);
    Ok((head.status, j))
}

/// `application/x-www-form-urlencoded` — unreserved octets pass
/// verbatim (RFC 3986); everything else percent-encodes.
fn form(fields: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for (i, (k, v)) in fields.iter().enumerate() {
        if i > 0 {
            out.push('&');
        }
        out.push_str(&enc(k));
        out.push('=');
        out.push_str(&enc(v));
    }
    out
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The provider's device-flow endpoint triple.
#[derive(Debug, Clone)]
pub struct DeviceFlow {
    /// `POST` — device code request (`client_id` [, `scope`]).
    pub device_code_url: String,
    /// `POST` — the token poll (`grant_type=…:device_code`).
    pub token_url: String,
    /// `GET` — the bearer→subject endpoint (`Authorization: Bearer`).
    pub user_url: String,
}

impl DeviceFlow {
    /// The GitHub OAuth-App endpoints (the 2026-10-20 ruling's provider).
    pub fn github() -> DeviceFlow {
        DeviceFlow {
            device_code_url: "https://github.com/login/device/code".to_string(),
            token_url: "https://github.com/login/oauth/access_token".to_string(),
            user_url: "https://api.github.com/user".to_string(),
        }
    }
}

/// The device-code grant the provider answered — the operator-facing
/// members (`user_code`, `verification_uri`) go to the operator's
/// terminal, never a ledger; `device_code` is the poll credential the
/// resumable leg persists under the gitignored state file.
#[derive(Debug, Clone)]
pub struct DeviceGrant {
    /// The poll credential (sensitive — never a record member).
    pub device_code: String,
    /// The code the operator enters at `verification_uri`.
    pub user_code: String,
    /// Where the operator enters it.
    pub verification_uri: String,
    /// The poll interval the provider mandates (seconds).
    pub interval_s: u64,
    /// The grant's lifetime (seconds) — the poll deadline.
    pub expires_in_s: u64,
}

fn member<'a>(j: &'a Json, key: &str) -> Result<&'a str, OAuthError> {
    j.get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| OAuthError::Malformed(format!("`{key}` missing")))
}

/// `initiate(transport, flow, client_id)` — the device-code request.
/// The provider's own `error` member is a typed `Refused`.
pub fn initiate(
    t: Transport,
    flow: &DeviceFlow,
    client_id: &str,
) -> Result<DeviceGrant, OAuthError> {
    let body = form(&[("client_id", client_id), ("scope", "")]);
    let (status, j) = http_call(
        t,
        &flow.device_code_url,
        "POST",
        &[("Content-Type", "application/x-www-form-urlencoded")],
        body.as_bytes(),
    )?;
    if let Some(e) = j.get("error").and_then(Json::as_str) {
        return Err(OAuthError::Refused(format!("device code: {e}")));
    }
    if status / 100 != 2 {
        return Err(OAuthError::Http(status, "device code request".to_string()));
    }
    Ok(DeviceGrant {
        device_code: member(&j, "device_code")?.to_string(),
        user_code: member(&j, "user_code")?.to_string(),
        verification_uri: member(&j, "verification_uri")?.to_string(),
        interval_s: j
            .get("interval")
            .and_then(Json::as_int)
            .map(|n| n.max(1) as u64)
            .unwrap_or(5),
        expires_in_s: j
            .get("expires_in")
            .and_then(Json::as_int)
            .map(|n| n.max(0) as u64)
            .unwrap_or(900),
    })
}

/// One poll's answer — the provider's RFC-8628 statuses plus the
/// terminal grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollAnswer {
    /// `authorization_pending` — the operator hasn't entered the code.
    Pending,
    /// `slow_down` — the provider demands +5s on the interval.
    SlowDown,
    /// `access_token` — granted; the credential (exchange-scope only).
    Granted(String),
    /// `access_denied` / `incorrect_device_code` — terminal refusal.
    Denied(String),
    /// `expired_token` — the grant's window closed.
    Expired,
}

/// `poll_once` — one `POST` against `token_url`; the provider's `error`
/// spelling maps to the typed answer (unknown spellings are `Refused`,
/// never coerced).
pub fn poll_once(
    t: Transport,
    flow: &DeviceFlow,
    client_id: &str,
    device_code: &str,
) -> Result<PollAnswer, OAuthError> {
    let body = form(&[
        ("client_id", client_id),
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ("device_code", device_code),
    ]);
    let (status, j) = http_call(
        t,
        &flow.token_url,
        "POST",
        &[("Content-Type", "application/x-www-form-urlencoded")],
        body.as_bytes(),
    )?;
    if let Some(e) = j.get("error").and_then(Json::as_str) {
        return Ok(match e {
            "authorization_pending" => PollAnswer::Pending,
            "slow_down" => PollAnswer::SlowDown,
            "expired_token" => PollAnswer::Expired,
            "access_denied" | "incorrect_device_code" => PollAnswer::Denied(e.to_string()),
            other => return Err(OAuthError::Refused(format!("token poll: {other}"))),
        });
    }
    if status / 100 != 2 {
        return Err(OAuthError::Http(status, "token poll".to_string()));
    }
    Ok(PollAnswer::Granted(member(&j, "access_token")?.to_string()))
}

/// The completed poll's outcome — `Granted` carries the access token,
/// which never leaves the caller's scope (consumed by
/// `verify_bearer`-style calls inside the leg).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollOutcome {
    /// The operator authorized — the provider's access token.
    Granted(String),
    /// Terminal denial (`access_denied`/`incorrect_device_code`).
    Denied(String),
    /// The grant's window closed before authorization.
    Expired,
}

/// `poll_until` — the resumable poll loop: `Pending` sleeps `interval`,
/// `SlowDown` widens it by the mandated +5s, the deadline is the grant's
/// `expires_in` window. `sleep` is injected so the hermetic legs run
/// the branch logic without wall-clock waits.
pub fn poll_until(
    t: Transport,
    flow: &DeviceFlow,
    client_id: &str,
    device_code: &str,
    interval_s: u64,
    deadline: Instant,
    sleep: &mut dyn FnMut(Duration),
) -> Result<PollOutcome, OAuthError> {
    let mut interval = interval_s.max(1);
    loop {
        if Instant::now() >= deadline {
            return Ok(PollOutcome::Expired);
        }
        match poll_once(t, flow, client_id, device_code)? {
            PollAnswer::Pending => sleep(Duration::from_secs(interval)),
            PollAnswer::SlowDown => {
                interval += 5;
                sleep(Duration::from_secs(interval));
            }
            PollAnswer::Granted(tok) => return Ok(PollOutcome::Granted(tok)),
            PollAnswer::Denied(e) => return Ok(PollOutcome::Denied(e)),
            PollAnswer::Expired => return Ok(PollOutcome::Expired),
        }
    }
}

/// The mediator's declared issuer: where a bearer verifies, and over
/// which transport. `issuer_ref`s not declared here keep the kernel's
/// own `minted_scoped` leg (the landed `oauth{audience}` shape).
#[derive(Debug, Clone)]
pub struct OAuthIssuer {
    /// The provider's bearer→subject endpoint (`GET`, `Authorization:
    /// Bearer`).
    pub user_url: String,
    /// How the leg dials (`Tls` production; `Loopback` the fixture).
    pub transport: Transport,
}

/// The bearer-verify verdict — `Valid` carries the provider-verified
/// subject; `Invalid` the issuer refused the credential; `Unreachable`
/// the exchange itself could not run (transient — the caller retries,
/// never a guessed subject either way).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BearerVerdict {
    /// `2xx` + `login` — the credential verified; the subject it names.
    Valid {
        /// The provider-verified subject (`login`).
        subject: String,
    },
    /// `401`/`403`, or a `2xx` without a subject — not a credential the
    /// issuer vouches for.
    Invalid,
    /// The exchange could not complete (dial/TLS/IO/timeout/other
    /// status) — transient, never coerced to `Invalid`.
    Unreachable,
}

impl OAuthIssuer {
    /// `verify_bearer(token)` — `GET user_url` under `Authorization:
    /// Bearer`. The subject is the provider's answer, never a guess.
    pub fn verify_bearer(&self, token: &str) -> BearerVerdict {
        let auth = format!("Bearer {token}");
        match http_call(
            self.transport,
            &self.user_url,
            "GET",
            &[("Authorization", auth.as_str())],
            &[],
        ) {
            Ok((status, j)) if status / 100 == 2 => match j.get("login").and_then(Json::as_str) {
                Some(s) if !s.is_empty() => BearerVerdict::Valid {
                    subject: s.to_string(),
                },
                _ => BearerVerdict::Invalid,
            },
            Ok((401 | 403, _)) => BearerVerdict::Invalid,
            Ok(_) => BearerVerdict::Unreachable,
            Err(OAuthError::Refused(_)) => BearerVerdict::Invalid,
            Err(_) => BearerVerdict::Unreachable,
        }
    }
}
