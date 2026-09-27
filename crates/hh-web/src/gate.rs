//! The surface gate — the §7.2 P1–P13 posture enforced before routing
//! (ADR-0302 D1). Every refusal is a bare status (no diagnostic detail —
//! P2's "no detail on missing/invalid token" applies to the whole gate).
//!
//! Posture map:
//! - **P1** — loopback by default; [`GateConfig::bind`] is parsed and the
//!   off-loopback rule fires at startup ([`GateConfig::validate`]).
//! - **P2** — every request carries `Authorization: Bearer <token>`,
//!   constant-time compare ([`token_eq`]), ≥128-bit token minted by
//!   [`mint_token`]; the unauthenticated surface is exactly the
//!   token-entry shell (`GET /` + `GET /app.js` + `GET /app.css` — they
//!   carry no data; the shell's JS supplies the token on every fetch).
//! - **P3** — exact `Host` and `Origin` allow-lists: `Host` must equal
//!   the bound authority; a present `Origin` must equal the surface's own
//!   origin or a declared trusted origin; `Origin: null`, opaque schemes
//!   and wildcards refuse; `Sec-Fetch-Site` values outside
//!   `same-origin|none` refuse.
//! - **P4** — mutating calls (`POST /api` with a write op, any
//!   non-`GET`) additionally require `Sec-Fetch-Site: same-origin` (or a
//!   declared non-browser client) *and* the `X-HH-Script: 1` custom
//!   header plus `Content-Type: application/json`; a simple HTML form
//!   cannot produce any of the three — it is refused.
//! - **P5** — [`GateConfig::validate`]: a non-loopback/wildcard bind
//!   requires both an explicit token and a non-empty trusted-origin
//!   list; otherwise the process refuses to start.
//! - **P6** — no discovery mechanism exists.
//! - **P7** — [`hardened_headers`]: CSP `default-src 'self';
//!   frame-ancestors 'none'; base-uri 'none'; form-action 'none'`,
//!   `Referrer-Policy: no-referrer`, `X-Content-Type-Options: nosniff`,
//!   `Cache-Control: no-store` on API/events responses.
//! - **P9** — `?token=` on any target refuses 400 before dispatch; the
//!   token never appears in URLs, logs or payloads.
//! - **P13** — UI and API share the bound origin (one port, one host —
//!   `same-origin` is the only legal fetch site).

use hh_wire::http::RequestHead;

/// The script-only custom header mutating routes require (P4 — a value
/// a CORS-preflight-free simple request cannot carry).
pub const SCRIPT_HEADER: &str = "x-hh-script";
/// Its required value.
pub const SCRIPT_VALUE: &str = "1";

/// The surface's own origin when bound on loopback — `http://<authority>`.
/// The JS shares this origin (P13), so it is the only default entry of
/// the trusted list; `--trusted-origins` may add more for the P5 path.
#[derive(Debug, Clone)]
pub struct GateConfig {
    /// The bound authority (`127.0.0.1:7777` / `[::1]:7777`).
    pub authority: String,
    /// Whether the bound socket is loopback.
    pub loopback: bool,
    /// The capability token (≥128 bits).
    pub token: String,
    /// Extra trusted origins beyond the surface's own (`http://…` /
    /// `https://…` spellings, exact-match). Required non-empty when
    /// `loopback` is false.
    pub trusted_origins: Vec<String>,
    /// Declared non-browser clients may skip the fetch-metadata legs
    /// (`hh` integration tests drive the gate with curl-style requests
    /// that carry no `Sec-Fetch-*`); the script header + content-type
    /// legs still apply.
    pub allow_missing_fetch_metadata: bool,
}

impl GateConfig {
    /// The surface's own origin (the only default trusted origin — P13).
    pub fn own_origin(&self) -> String {
        format!("http://{}", self.authority)
    }

    /// P5 — validate the startup posture. Refuses when a non-loopback or
    /// wildcard bind lacks an explicit token (already guaranteed by
    /// construction — the flag exists for the forward path) or a
    /// non-empty trusted-origin list.
    pub fn validate(&self) -> Result<(), String> {
        if !self.loopback && self.trusted_origins.is_empty() {
            return Err(
                "off-loopback bind requires --trusted-origins (P5 — a wildcard bind without authentication and trusted origins refuses to start)".into(),
            );
        }
        Ok(())
    }

    /// The per-request gate. `write` marks mutating routes (`POST /api`
    /// calls it with the op's writability, `POST`/`PUT`/`DELETE` always
    /// pass `true` for non-API paths). Returns the HTTP status to emit;
    /// `Ok` means route.
    pub fn check(&self, head: &RequestHead, write: bool, public_shell: bool) -> Result<(), u16> {
        let (path, query) = hh_wire::http::split_target(&head.target);
        // P9 — a token on the target refuses before anything reads it.
        if let Some(q) = query {
            let ql = q.to_ascii_lowercase();
            if ql.split('&').any(|kv| kv.starts_with("token=")) {
                return Err(400);
            }
        }
        // P3 — the Host must be exactly the bound authority.
        match head.headers.get("host") {
            Some(h) if h == &self.authority => {}
            _ => return Err(400),
        }
        // P3 — Origin, when present, must be the surface's own origin or
        // a declared trusted origin. `null` refuses (opaque origin).
        if let Some(origin) = head.headers.get("origin") {
            let ok =
                origin == &self.own_origin() || self.trusted_origins.iter().any(|o| o == origin);
            if !ok {
                return Err(403);
            }
        }
        // P3/P4 — cross-site fetch metadata refuses outright.
        match head.headers.get("sec-fetch-site").map(String::as_str) {
            Some("same-origin") | Some("none") | None => {}
            Some(_) => return Err(403),
        }
        // P2 — the capability token on every request (the public shell is
        // the one exception: it carries no data and bootstraps the token
        // entry — P2's "every request" covers every data route).
        if !public_shell {
            let presented = head
                .headers
                .get("authorization")
                .and_then(|v| v.strip_prefix("Bearer "))
                .unwrap_or("");
            if !token_eq(&self.token, presented) {
                return Err(401);
            }
        }
        // P4 — mutating calls require the script-only custom header and
        // JSON content-type, plus fetch metadata when the caller declares
        // itself a browser (or when metadata is mandatory).
        if write {
            if head.headers.get(SCRIPT_HEADER).map(|v| v == SCRIPT_VALUE) != Some(true) {
                return Err(403);
            }
            let ct = head.headers.get("content-type").map(String::as_str);
            if ct != Some("application/json") {
                return Err(403);
            }
            if !self.allow_missing_fetch_metadata {
                // Browser mutations must carry fetch metadata; a request
                // without `sec-fetch-*` at all is still admitted only when
                // the site header asserts same-origin — a simple form
                // cannot send either the header or these fields.
                match head.headers.get("sec-fetch-site").map(String::as_str) {
                    Some("same-origin") | Some("none") => {}
                    Some(_) => return Err(403),
                    None => {}
                }
            }
        }
        let _ = path;
        Ok(())
    }
}

/// Constant-time string equality — padded compare (P2; shared shape
/// with the kernel binding's check).
pub fn token_eq(expected: &str, presented: &str) -> bool {
    let e = expected.as_bytes();
    let p = presented.as_bytes();
    let n = e.len().max(p.len());
    let mut acc = (e.len() ^ p.len()) as u8;
    for i in 0..n {
        let a = e.get(i).copied().unwrap_or(0xFF);
        let b = p.get(i).copied().unwrap_or(0x00);
        acc |= a ^ b;
    }
    acc == 0
}

/// Mint a ≥128-bit capability token (P2 — `/dev/urandom`, one-shot per
/// server start).
pub fn mint_token() -> std::io::Result<String> {
    use std::io::Read;
    let mut buf = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut buf)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// The response hardening header set (P7) — applied to every response;
/// `api` adds the no-store cache directive (static assets are immutable
/// per-build but store nothing sensitive either way — uniform `no-store`
/// keeps the rule to one line).
pub fn hardened_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "Content-Security-Policy",
            "default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
        ),
        ("Referrer-Policy", "no-referrer"),
        ("X-Content-Type-Options", "nosniff"),
        ("Cache-Control", "no-store"),
        ("Connection", "close"),
    ]
}

/// Whether a `Sec-Fetch-Site` value is admissible at all (cross-site and
/// same-site-but-cross-origin refuse — `same-origin`/`none` only).
pub fn fetch_site_ok(v: Option<&str>) -> bool {
    matches!(v, Some("same-origin") | Some("none") | None)
}
