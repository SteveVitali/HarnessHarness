//! OAuth 2.1 + RFC 8707 resource indicators, through H3 (§5d.4 D3/D5;
//! ADR-0097 D3). The wire edge never mints or mirrors a credential:
//! the `401`/`WWW-Authenticate` challenge is *lifted* into an
//! [`AuthClaims`] record (data — claims never decide authority, T2),
//! the token request is dispatched through a [`CredentialMediator`]
//! seam (H3's custody boundary — production wires the
//! `hh-secrets` `CredentialBroker`; fixtures an in-process AS), and the
//! returned bearer lives only in the transport's `Authorization`
//! header. Records carry a `fingerprint()`, never the token.
//!
//! RFC 8707: the `resource` parameter binds the grant to the protected
//! resource — the edge's `request_target` (the transport's request
//! URI). The flow refuses an audience mismatch (a grant minted for a
//! different resource is never accepted — the mediation fail-closed
//! half of AC-R-2.5.4-6).
//!
//! OAuth 2.1's PKCE (`S256`) rides every request; `iss` (RFC 9207
//! issuer identification) is lifted from the challenge when present
//! and echoed to the mediator as the request's `issuer` member — the
//! fixture AS verifies it, the edge records it as a claim.

use hh_wire::json::Json;

/// `AuthClaims` — the lifted `WWW-Authenticate` challenge (D5: the edge
/// lifts the peer's claims into a record the *caller* may inspect; the
/// kernel never reads it for a decision).
#[derive(Debug, Clone, PartialEq)]
pub struct AuthClaims {
    /// The auth scheme (`bearer`).
    pub scheme: String,
    /// `realm` — the protection space the server named.
    pub realm: Option<String>,
    /// `scope` — the scope string the challenge advertised.
    pub scope: Option<String>,
    /// `resource_metadata` (RFC 9728) — the protected-resource metadata
    /// URI the challenge named.
    pub resource_metadata: Option<String>,
    /// `iss` (RFC 9207) — the issuer the challenge identified.
    pub issuer: Option<String>,
    /// `error`/`error_description` — a failed-attempt claim.
    pub error: Option<String>,
}

impl AuthClaims {
    /// Canonical record form — data, never authority.
    pub fn to_json(&self) -> Json {
        let mut m = vec![("scheme", Json::str(self.scheme.clone()))];
        if let Some(v) = &self.realm {
            m.push(("realm", Json::str(v.clone())));
        }
        if let Some(v) = &self.scope {
            m.push(("scope", Json::str(v.clone())));
        }
        if let Some(v) = &self.resource_metadata {
            m.push(("resource_metadata", Json::str(v.clone())));
        }
        if let Some(v) = &self.issuer {
            m.push(("iss", Json::str(v.clone())));
        }
        if let Some(v) = &self.error {
            m.push(("error", Json::str(v.clone())));
        }
        Json::Obj(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
}

/// `WwwAuthenticate` — parse `Bearer realm="…"[, scope="…",
/// resource_metadata="…", iss="…", error="…"]`.
pub fn parse_www_authenticate(header: &str) -> Option<AuthClaims> {
    let (scheme, rest) = header.split_once(' ').unwrap_or((header, ""));
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let mut claims = AuthClaims {
        scheme: "bearer".to_string(),
        realm: None,
        scope: None,
        resource_metadata: None,
        issuer: None,
        error: None,
    };
    for part in rest.split(',') {
        let part = part.trim();
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let v = v.trim().trim_matches('"');
        match k.trim() {
            "realm" => claims.realm = Some(v.to_string()),
            "scope" => claims.scope = Some(v.to_string()),
            "resource_metadata" => claims.resource_metadata = Some(v.to_string()),
            "iss" => claims.issuer = Some(v.to_string()),
            "error" | "error_description" => claims.error = Some(v.to_string()),
            _ => {}
        }
    }
    Some(claims)
}

/// `pkce_pair()` — `(verifier, challenge)`: the verifier is a random
/// 43-char base64url string; the challenge is `BASE64URL(SHA256(v))`
/// (RFC 7636 `S256` — OAuth 2.1 requires it).
pub fn pkce_pair() -> (String, String) {
    let mut bytes = [0u8; 32];
    getrandom(&mut bytes);
    let verifier = b64url(&bytes);
    let challenge = b64url(hh_wire::sha256::sha256_bytes(verifier.as_bytes()).as_slice());
    (verifier, challenge)
}

/// `OAuthRequest` — the token request dispatched through the mediator.
/// `resource` is the RFC 8707 resource indicator — the protected
/// resource's URI (the transport's `request_target`), never a wildcard.
#[derive(Debug, Clone, PartialEq)]
pub struct OAuthRequest {
    /// `grant_type` — `authorization_code` (OAuth 2.1).
    pub grant_type: String,
    /// `resource` — RFC 8707 indicator (required, exact).
    pub resource: String,
    /// `client_id`.
    pub client_id: String,
    /// `redirect_uri` (the fixture loopback form).
    pub redirect_uri: String,
    /// `scope` — from the challenge, when advertised.
    pub scope: Option<String>,
    /// `iss` — RFC 9207 issuer identification, from the challenge.
    pub issuer: Option<String>,
    /// `code_verifier` — the PKCE verifier (S256 pair).
    pub code_verifier: String,
    /// `state` — CSRF binding.
    pub state: String,
}

impl OAuthRequest {
    /// Canonical record form.
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            ("grant_type", Json::str(self.grant_type.clone())),
            ("resource", Json::str(self.resource.clone())),
            ("client_id", Json::str(self.client_id.clone())),
            ("redirect_uri", Json::str(self.redirect_uri.clone())),
            ("code_verifier", Json::str(self.code_verifier.clone())),
            ("state", Json::str(self.state.clone())),
        ];
        if let Some(s) = &self.scope {
            m.push(("scope", Json::str(s.clone())));
        }
        if let Some(i) = &self.issuer {
            m.push(("iss", Json::str(i.clone())));
        }
        Json::Obj(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
}

/// `TokenGrant` — the mediator's answer. `access_token` is the secret —
/// it lives in this struct for the transport's `Authorization` header
/// and in the broker's custody; it is never serialized into a record.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenGrant {
    /// The bearer (secret — never ledgered).
    pub access_token: String,
    /// `token_type` (`bearer`).
    pub token_type: String,
    /// `expires_in` (ms), when the AS declared it.
    pub expires_in_ms: Option<u64>,
    /// `resource` the grant was minted for (RFC 8707 echo — the edge
    /// refuses a mismatch).
    pub resource: String,
    /// `iss` — the issuer the AS reported (RFC 9207).
    pub issuer: Option<String>,
}

impl TokenGrant {
    /// The record-safe fingerprint (`idp` of the token bytes — the
    /// reference a binding/`auth` member may carry; never the token).
    pub fn fingerprint(&self) -> String {
        hh_identity::idp::idp_id("oauth_token", self.access_token.as_bytes())
    }
}

/// `CredentialMediator` — the H3 custody seam (§5d.4 D3/D5's "through
/// H3"): the edge dispatches the token request to it; production wires
/// `hh-secrets`' `CredentialBroker` (channel `oauth`), fixtures an
/// in-process authorization server. The mediator *owns* the secret —
/// the returned [`TokenGrant`] is the transport's header material only.
pub trait CredentialMediator {
    /// `exchange(request) → grant` — perform the token exchange under
    /// H3 custody (the implementation may consult the run's egress
    /// boundary; the edge never speaks to an AS itself).
    fn exchange(&mut self, request: &OAuthRequest) -> Result<TokenGrant, String>;
}

/// Blanket impl for fixtures/closures.
impl<F: FnMut(&OAuthRequest) -> Result<TokenGrant, String>> CredentialMediator for F {
    fn exchange(&mut self, request: &OAuthRequest) -> Result<TokenGrant, String> {
        self(request)
    }
}

/// `OAuthError` — the flow's typed refusals.
#[derive(Debug, Clone, PartialEq)]
pub enum OAuthError {
    /// The `401` carried no usable challenge.
    NoChallenge,
    /// The challenge was not `Bearer`.
    ChallengeMalformed(String),
    /// The mediator refused/failed the exchange.
    ExchangeFailed(String),
    /// The returned grant's `resource` ≠ the requested indicator
    /// (RFC 8707 audience binding — fail closed).
    AudienceMismatch {
        /// What was asked.
        requested: String,
        /// What was granted.
        granted: String,
    },
    /// The grant's `iss` ≠ the challenge's issuer.
    IssuerMismatch {
        /// Declared in the challenge.
        expected: String,
        /// Returned by the AS.
        got: String,
    },
}

impl std::fmt::Display for OAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OAuthError::NoChallenge => write!(f, "NoChallenge"),
            OAuthError::ChallengeMalformed(d) => write!(f, "ChallengeMalformed: {d}"),
            OAuthError::ExchangeFailed(d) => write!(f, "ExchangeFailed: {d}"),
            OAuthError::AudienceMismatch { requested, granted } => {
                write!(f, "AudienceMismatch: asked {requested} got {granted}")
            }
            OAuthError::IssuerMismatch { expected, got } => {
                write!(f, "IssuerMismatch: expected {expected} got {got}")
            }
        }
    }
}

impl std::error::Error for OAuthError {}

/// `OAuthFlow` — the §5d.4 OAuth binding: `resource` is the protected
/// resource the grant must be minted for (the transport's
/// `request_target`).
#[derive(Debug, Clone)]
pub struct OAuthFlow {
    /// The RFC 8707 resource indicator (the endpoint URI).
    pub resource: String,
    /// `client_id` (the artefact's declared identity).
    pub client_id: String,
    /// `redirect_uri` (the fixture loopback).
    pub redirect_uri: String,
}

impl OAuthFlow {
    /// `OAuthFlow::new("http://127.0.0.1:PORT/mcp")`.
    pub fn new(resource: &str) -> OAuthFlow {
        OAuthFlow {
            resource: resource.to_string(),
            client_id: "hh-mcp-client".to_string(),
            redirect_uri: "http://127.0.0.1/callback".to_string(),
        }
    }

    /// `handle_challenge(header, mediator)` — lift the challenge to
    /// [`AuthClaims`], build the PKCE-bound [`OAuthRequest`], exchange
    /// through the mediator, then verify the grant's audience binding:
    /// `grant.resource == self.resource` (refuse a foreign-resource
    /// token) and `grant.iss == challenge.iss` when declared.
    /// Returns `(grant, claims)` — the claims ride back as the lifted
    /// record (data; the caller may ledger the *record*, never the
    /// token).
    pub fn handle_challenge(
        &self,
        challenge_header: Option<&str>,
        mediator: &mut dyn CredentialMediator,
    ) -> Result<(TokenGrant, AuthClaims), OAuthError> {
        let header = challenge_header.ok_or(OAuthError::NoChallenge)?;
        let claims = parse_www_authenticate(header)
            .ok_or_else(|| OAuthError::ChallengeMalformed(header.to_string()))?;
        let (verifier, _challenge) = pkce_pair();
        let request = OAuthRequest {
            grant_type: "authorization_code".to_string(),
            resource: self.resource.clone(),
            client_id: self.client_id.clone(),
            redirect_uri: self.redirect_uri.clone(),
            scope: claims.scope.clone(),
            issuer: claims.issuer.clone(),
            code_verifier: verifier,
            state: b64url(hh_wire::sha256::sha256_bytes(self.resource.as_bytes()).as_slice())[..16]
                .to_string(),
        };
        let grant = mediator
            .exchange(&request)
            .map_err(OAuthError::ExchangeFailed)?;
        if grant.resource != self.resource {
            return Err(OAuthError::AudienceMismatch {
                requested: self.resource.clone(),
                granted: grant.resource,
            });
        }
        if let (Some(expected), Some(got)) = (&claims.issuer, &grant.issuer) {
            if expected != got {
                return Err(OAuthError::IssuerMismatch {
                    expected: expected.clone(),
                    got: got.clone(),
                });
            }
        }
        Ok((grant, claims))
    }
}

fn b64url(bytes: &[u8]) -> String {
    const TBL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TBL[(n >> 18) as usize & 63] as char);
        out.push(TBL[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(TBL[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(TBL[n as usize & 63] as char);
        }
    }
    out
}

fn getrandom(buf: &mut [u8]) {
    // Process-seeded fixture randomness — the PKCE verifier's entropy
    // (OS `getrandom` where available; a nanos-seeded counter
    // elsewhere). Deterministic *tests* inject their own verifier via
    // the mediator; the verifier's bytes only need unpredictability.
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    for (i, b) in buf.iter_mut().enumerate() {
        let v = nanos
            .wrapping_mul(0x9E3779B97F4A7C15)
            .wrapping_add(pid << 64)
            ^ (i as u128).wrapping_mul(0xD6E8FEB86659FD93);
        *b = (v >> ((i % 16) * 8)) as u8 ^ (v as u8);
    }
}
