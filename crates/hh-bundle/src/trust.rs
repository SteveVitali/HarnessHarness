//! `trust` — the `FetchTrustPolicy` MUST-data and the
//! `manifest.locations[]` admission machinery (R4.15d; OQ-332;
//! DF-S4.2-1; the GATE-G7-adopted bl33 §trust-fields set —
//! `docs/build/planning/round4-bl33-ws-l6-briefs.md`, adopted verbatim
//! by "Adopt all" in GATE DECISIONS `### Round 4`, 2026-10-22).
//!
//! One record per fetch site, version_id-pinnable into the bundle's
//! `resolved_dependencies` — a fetched member is only as trustworthy as
//! the policy that admitted it (CC3):
//!
//! ```text
//! {admission_classes[], admitted_origins[], accepted_signers: SignerRef[],
//!  require_ledgered_arrival}
//! ```
//!
//! Admission classes (the closed sum — an unknown spelling refuses at
//! parse, never silently widens):
//!
//! - `first_party` — `https:` origins on `admitted_origins`. Trust root:
//!   TLS PKI (the `hh-tls` rustls/webpki graph, ADR-0365) plus the
//!   allowlist row — integrity is by-construction (S3 re-hash).
//! - `signed_url` — presigned-URL carriers (`https:` carrying an
//!   embedded signature param); `FetchEntry.expires` **enforced** — an
//!   expired location refuses `location_expired`; the carrier's own
//!   embedded expiry (`Expires=`/`se=`/`x-amz-*`) is enforced too when
//!   present. A signed attestation over the location set may ride the
//!   bundle's `attestations.jsonl` under `accepted_signers`
//!   (`pin`-class endorsement, ADR-0063 shape).
//! - `content_addressed` — `ipfs:`/CID-class locations: admitted
//!   unconditionally (no origin allowlist — the member re-hashes to its
//!   address at S3; wrong bytes fail `MemberMismatch`). The class still
//!   has to be *declared* in `admission_classes` — "unconditional"
//!   means no trust root beyond the address, not that an undeclared
//!   class sneaks in. The workspace has no IPFS wire (ADR-0365's closed
//!   HTTPS scope), so the *transport* leg answers typed
//!   `transport_unsupported` — the admission stands, the wire honestly
//!   refuses.
//! - cleartext `http:` — never admitted (`transport_refused`; the
//!   egress floor + ADR-0365's TLS scope).
//!
//! `expires` is enforced *per location*, uniformly — an expired entry
//! refuses `location_expired` whatever its scheme; a fetched member
//! re-verifies on every materialization (no "fetched once, trusted
//! forever"); a GC'd member keeps its addresses and may re-materialize
//! while a live location serves (OQ-112); retention tiers are
//! unaffected (OQ-085 — fetch changes where bytes come from, never how
//! long they live).
//!
//! The `transport_pending` interim is gone — every refusal the pool
//! half used to park is a typed arm here.

use hh_wire::json::Json;

use crate::error::BundleError;
use crate::manifest::FetchEntry;

/// The record's `schema` spelling (`hh/` MUST-data, `hh-bundle` home).
pub const FETCH_TRUST_SCHEMA: &str = "hh-bundle-fetch-trust/1";

/// `FetchTrustPolicy` — the adopted MUST-data record.
#[derive(Debug, Clone, PartialEq)]
pub struct FetchTrustPolicy {
    /// The admission classes this site admits — the closed
    /// `first_party`/`signed_url`/`content_addressed` sum.
    pub admission_classes: Vec<String>,
    /// `first_party` origins — `host` or `host:port` spellings
    /// (project-controlled hosts: release assets, named project hosts).
    pub admitted_origins: Vec<String>,
    /// `SignerRef` coordinates for attestation verification
    /// (`pin`-class endorsement over the location set — coordinates,
    /// never key material).
    pub accepted_signers: Vec<String>,
    /// Whether every arrival must land a `bundle.member.fetched` row
    /// before the member reads `present` (the adopted MUST-data carries
    /// `true`; a `false` declaration is honored, never coerced).
    pub require_ledgered_arrival: bool,
}

impl FetchTrustPolicy {
    /// Canonical JSON (the record's own document — the member form and
    /// the version_id preimage agree).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str(FETCH_TRUST_SCHEMA)),
            (
                "admission_classes",
                Json::Arr(self.admission_classes.iter().map(Json::str).collect()),
            ),
            (
                "admitted_origins",
                Json::Arr(self.admitted_origins.iter().map(Json::str).collect()),
            ),
            (
                "accepted_signers",
                Json::Arr(self.accepted_signers.iter().map(Json::str).collect()),
            ),
            (
                "require_ledgered_arrival",
                Json::Bool(self.require_ledgered_arrival),
            ),
        ])
    }

    /// The record's version id — `idp/1` over its canonical document
    /// (the coordinate a bundle's `resolved_dependencies` pins).
    pub fn version_id(&self) -> String {
        hh_identity::idp_id(
            "fetch-trust-policy",
            self.to_json().to_canonical_string().as_bytes(),
        )
    }

    /// Decode + validate — every member present and typed; an unknown
    /// admission class refuses `Malformed` (the sum is closed).
    pub fn from_json(j: &Json) -> Result<FetchTrustPolicy, BundleError> {
        let bad = |d: &str| BundleError::Malformed {
            detail: format!("FetchTrustPolicy: {d}"),
        };
        let strs = |key: &str| -> Result<Vec<String>, BundleError> {
            match j.get(key) {
                Some(Json::Arr(a)) => {
                    let mut v = Vec::with_capacity(a.len());
                    for m in a {
                        v.push(m.as_str().ok_or(bad(key))?.to_string());
                    }
                    Ok(v)
                }
                _ => Err(bad(key)),
            }
        };
        let policy = FetchTrustPolicy {
            admission_classes: strs("admission_classes")?,
            admitted_origins: strs("admitted_origins")?,
            accepted_signers: strs("accepted_signers")?,
            require_ledgered_arrival: match j.get("require_ledgered_arrival") {
                Some(Json::Bool(b)) => *b,
                _ => return Err(bad("require_ledgered_arrival")),
            },
        };
        for c in &policy.admission_classes {
            if AdmissionClass::parse(c).is_none() {
                return Err(bad("admission_classes: unknown class"));
            }
        }
        Ok(policy)
    }

    /// Whether `class` is declared in `admission_classes`.
    pub fn admits(&self, class: AdmissionClass) -> bool {
        self.admission_classes.iter().any(|c| c == class.as_str())
    }

    /// Whether `host[:port]` is on the `admitted_origins` allowlist —
    /// an origin without a port matches any port on the host.
    pub fn origin_admitted(&self, host: &str, port: u16) -> bool {
        let host = host_norm(host);
        self.admitted_origins.iter().any(|o| {
            let o = o.trim().to_lowercase();
            match o.rsplit_once(':') {
                // `host:port` — but a `[v6]`/bare multi-colon spelling is
                // a host, not host:port.
                Some((h, p)) if !h.is_empty() && h.matches(':').count() == 0 => {
                    host_norm(h) == host && p.parse::<u16>().ok() == Some(port)
                }
                _ => host_norm(&o) == host,
            }
        })
    }
}

/// `admission ∈ {first_party, signed_url, content_addressed}` — the
/// closed class sum (§trust-fields).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionClass {
    /// `https:` on the `admitted_origins` allowlist.
    FirstParty,
    /// `https:` presigned-URL carrier (embedded signature params).
    SignedUrl,
    /// `ipfs:`/CID-class — integrity by construction at S3.
    ContentAddressed,
}

impl AdmissionClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AdmissionClass::FirstParty => "first_party",
            AdmissionClass::SignedUrl => "signed_url",
            AdmissionClass::ContentAddressed => "content_addressed",
        }
    }

    /// Parse the canonical spelling (`None` on anything else — the sum
    /// is closed).
    pub fn parse(s: &str) -> Option<AdmissionClass> {
        match s {
            "first_party" => Some(AdmissionClass::FirstParty),
            "signed_url" => Some(AdmissionClass::SignedUrl),
            "content_addressed" => Some(AdmissionClass::ContentAddressed),
            _ => None,
        }
    }
}

/// What an admitted location resolves to — the leg `fetch` attempts.
#[derive(Debug, Clone, PartialEq)]
pub enum LocationLeg {
    /// A pool/local location (or an entry with none) — the blob-pool
    /// fetcher resolves it by address; no trust leg applies.
    Pool,
    /// An `https:` location admitted under `class` — the TLS GET wire
    /// may attempt it.
    HttpsGet {
        /// The admission class (`first_party` | `signed_url`).
        class: AdmissionClass,
        /// The location URI, verbatim.
        url: String,
    },
    /// A CID-class location admitted under `content_addressed` — the
    /// class stands; no IPFS wire exists in the closed ADR-0365 scope,
    /// so the *transport* answers `transport_unsupported` (honest
    /// refusal, never a fabricated fetch).
    Unsupported {
        /// The admission class (`content_addressed`).
        class: AdmissionClass,
        /// The location's scheme (`ipfs`).
        scheme: String,
    },
}

/// The typed refusal a location produces — the `transport_pending`
/// replacement arms (the reason spellings are the contract's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchRefusal {
    /// Cleartext `http:` (or a scheme outside the grammar) — never
    /// admitted.
    TransportRefused,
    /// `https:` neither on `admitted_origins` nor a signed carrier.
    OriginNotAdmitted,
    /// The location's class is not declared in `admission_classes`.
    ClassNotAdmitted,
    /// `FetchEntry.expires` or a signed carrier's embedded expiry has
    /// passed.
    LocationExpired,
    /// A remote location with no declared `FetchTrustPolicy`.
    PolicyAbsent,
}

impl FetchRefusal {
    /// The typed arm spelling (the `reason` member's value).
    pub fn as_str(self) -> &'static str {
        match self {
            FetchRefusal::TransportRefused => "transport_refused",
            FetchRefusal::OriginNotAdmitted => "origin_not_admitted",
            FetchRefusal::ClassNotAdmitted => "class_not_admitted",
            FetchRefusal::LocationExpired => "location_expired",
            FetchRefusal::PolicyAbsent => "policy_absent",
        }
    }
}

/// The presigned-carrier query members — the closed set that marks an
/// `https:` URL a `signed_url` (AWS SigV4 query + S3 presign, Azure SAS,
/// GCS signed, generic `token`/`expires` carriers). Case-insensitive
/// match on the parameter *name*.
const SIGNED_URL_PARAMS: &[&str] = &[
    "x-amz-signature",
    "x-amz-credential",
    "x-amz-security-token",
    "x-goog-signature",
    "googleaccessid",
    "awsaccesskeyid",
    "signature",
    "sig",
    "se",
    "sp",
    "sv",
    "st",
    "expires",
    "x-amz-expires",
    "token",
    "policy",
];

/// `location_scheme(location)` — the URI's scheme, lowercased, when the
/// spelling carries one (`https://…` → `https`; `ipfs:cid` → `ipfs` —
/// CID-class spellings carry no `//`). `None` for the pool/local
/// spellings: no scheme, a non-scheme prefix, or a bare content address
/// (`sha256:`/`sha512:`/`blake3:`/`b3:`/`idp:`-shaped — digest
/// prefixes are addresses, not transports).
pub fn location_scheme(location: &str) -> Option<String> {
    if let Some((s, _)) = location.split_once("://") {
        return Some(s.to_ascii_lowercase());
    }
    location.split_once(':').and_then(|(s, _)| {
        let s = s.to_ascii_lowercase();
        let looks_scheme = s.len() > 1
            && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
        match s.as_str() {
            "sha256" | "sha512" | "blake3" | "b3" | "idp" => None,
            _ if looks_scheme => Some(s),
            _ => None,
        }
    })
}

/// The credentialed-locator mirror of `validate::credentialed` (R-ID-7 —
/// the fetch path refuses on its own authority; a bundle may arrive
/// unvalidated): a `userinfo` authority, or a credential-ish query key
/// (`token`/`key`/`sig`/`signature`/`password`/`secret`) carrying a
/// *literal* value. A placeholder value is the safe `signed_url` form,
/// never flagged.
fn credential_literal(uri: &str) -> bool {
    let lower = uri.to_lowercase();
    if let Some(rest) = lower.split("://").nth(1) {
        let authority = rest.split('/').next().unwrap_or("");
        if authority.contains('@') {
            return true;
        }
    }
    for key in [
        "token=",
        "key=",
        "sig=",
        "signature=",
        "password=",
        "secret=",
    ] {
        let mut from = 0usize;
        while let Some(at) = lower[from..].find(key).map(|i| i + from) {
            let value = &lower[at + key.len()..];
            let value = value.split('&').next().unwrap_or("");
            if !hh_secrets::Placeholder::is_placeholder(value) {
                return true;
            }
            from = at + key.len();
        }
    }
    false
}

/// `host_norm` — lowercase, strip brackets and trailing dots. The
/// admission match's own normalization (an origin row is a locator,
/// not a decide_egress host — the matching rule is `host` or
/// `host:port` equality, never a suffix rule).
fn host_norm(raw: &str) -> String {
    let mut s = raw.trim().to_lowercase();
    if let Some(rest) = s.strip_prefix('[') {
        let inner = rest.split(']').next().unwrap_or(rest);
        s = inner.to_string();
    }
    while s.ends_with('.') {
        s.pop();
    }
    s
}

/// The `(host, port)` an `https:` URI names — the authority split the
/// same way `hh_wire::http::parse_http_url` reads it (default 443).
/// `None` on a malformed authority.
fn https_authority(uri: &str) -> Option<(String, u16)> {
    let rest = uri.strip_prefix("https://")?;
    let authority = rest.split('/').next()?;
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let end = v6.find(']')?;
        let tail = &v6[end + 1..];
        match tail.strip_prefix(':') {
            Some(p) => (v6[..end].to_string(), p.parse::<u16>().ok()?),
            None if tail.is_empty() => (v6[..end].to_string(), 443u16),
            None => return None,
        }
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) if !h.is_empty() && !h.contains(':') => {
                (h.to_string(), p.parse::<u16>().ok()?)
            }
            _ if !authority.is_empty() => (authority.to_string(), 443u16),
            _ => return None,
        }
    };
    if host.is_empty() {
        return None;
    }
    Some((host, port))
}

/// The URI's query/fragment parameter names, lowercased.
fn query_params(uri: &str) -> Vec<String> {
    let (_, tail) = match uri.split_once('?') {
        Some(t) => t,
        None => return Vec::new(),
    };
    let tail = tail.split('#').next().unwrap_or("");
    tail.split('&')
        .filter_map(|kv| {
            let name = kv.split('=').next()?;
            if name.is_empty() {
                None
            } else {
                Some(name.to_lowercase())
            }
        })
        .collect()
}

/// One query parameter's value, if present.
fn query_value(uri: &str, want: &str) -> Option<String> {
    let (_, tail) = uri.split_once('?')?;
    let tail = tail.split('#').next().unwrap_or("");
    for kv in tail.split('&') {
        let (name, value) = kv.split_once('=').unwrap_or((kv, ""));
        if name.eq_ignore_ascii_case(want) {
            return Some(value.to_string());
        }
    }
    None
}

/// Whether the URI carries any presigned-carrier member — a recognised
/// signature param *or* an `mh_secret:` placeholder anywhere in the URI
/// (a placeholder is a credential carrier by construction — the
/// `signed_url` class is exactly where the broker supplies the
/// signature at the mediated wire; R-ID-7 keeps the URI itself
/// credential-free).
fn has_signed_params(uri: &str) -> bool {
    if uri
        .split(|c: char| !(c.is_alphanumeric() || c == ':' || c == '_' || c == '-'))
        .any(hh_secrets::Placeholder::is_placeholder)
    {
        return true;
    }
    let params = query_params(uri);
    SIGNED_URL_PARAMS
        .iter()
        .any(|s| params.iter().any(|p| p == s))
}

/// The signed carrier's own embedded expiry, ms-since-epoch, when the
/// URI declares one: `Expires=` (unix seconds — the S3-presign form),
/// `se=`/`st=` (RFC 3339 — the Azure SAS end/start pair; `st` is a
/// not-before, `se` the expiry — `st` alone is not an expiry and is
/// ignored), `X-Amz-Date=` + `X-Amz-Expires=` (SigV4's basic-ISO
/// timestamp plus its seconds-to-live). `None` when no expiry member
/// parses — absence is never read as "expired".
fn embedded_expiry_ms(uri: &str) -> Option<u64> {
    if let Some(e) = query_value(uri, "expires").and_then(|v| v.parse::<u64>().ok()) {
        return Some(e.saturating_mul(1000));
    }
    if let Some(se) = query_value(uri, "se") {
        if let Some(ns) = crate::foreign::rfc3339_unix_nanos(&se) {
            return Some((ns.parse::<u128>().ok()? / 1_000_000) as u64);
        }
    }
    if let (Some(date), Some(secs)) = (
        query_value(uri, "x-amz-date"),
        query_value(uri, "x-amz-expires").and_then(|v| v.parse::<u64>().ok()),
    ) {
        // `YYYYMMDDTHHMMSSZ` → `YYYY-MM-DDTHH:MM:SSZ` for the one parser.
        let d = date.as_str();
        if d.len() == 16 && d.ends_with('Z') {
            let iso = format!(
                "{}-{}-{}T{}:{}:{}Z",
                &d[0..4],
                &d[4..6],
                &d[6..8],
                &d[9..11],
                &d[11..13],
                &d[13..15]
            );
            if let Some(ns) = crate::foreign::rfc3339_unix_nanos(&iso) {
                let base = ns.parse::<u128>().ok()? / 1_000_000;
                return Some((base + (secs as u128) * 1000) as u64);
            }
        }
    }
    None
}

/// `admit(policy, entry, location, now_ms)` — the per-location decision
/// the adopted field set fixes:
///
/// 1. `FetchEntry.expires` enforced per location, uniformly — an
///    expired location refuses `location_expired` whatever its scheme.
/// 2. Pool/local locations (`pool:`/`mem:`/`bundle:` schemes or no
///    `scheme://` at all) need no policy leg — they resolve by address.
/// 3. `http:` → `transport_refused`; `https:` → `first_party` on the
///    allowlist, else `signed_url` when presigned params ride the URI,
///    else `origin_not_admitted`; `ipfs:` → `content_addressed`; any
///    other scheme → `transport_refused`.
/// 4. The classified class must be *declared* — `admission_classes`
///    gates uniformly (`class_not_admitted`).
/// 5. A signed carrier's own embedded expiry is enforced when present
///    (`location_expired`).
/// 6. A remote location with no policy at all refuses `policy_absent`
///    — an unmediated fetch is never attempted.
///
/// The caller supplies `now_ms` (the store clock's — the fold stays
/// deterministic under `ManualClock`).
pub fn admit(
    policy: Option<&FetchTrustPolicy>,
    entry: &FetchEntry,
    location: &str,
    now_ms: u64,
) -> Result<LocationLeg, FetchRefusal> {
    // 1. `expires` enforced per location (entry-level — the grammar has
    //    one expiry member per FetchEntry).
    if let Some(e) = &entry.expires {
        match crate::foreign::rfc3339_unix_nanos(e) {
            Some(ns) => match ns.parse::<u128>() {
                Ok(ns) if (ns / 1_000_000) as u64 <= now_ms => {
                    return Err(FetchRefusal::LocationExpired);
                }
                _ => {}
            },
            None => return Err(FetchRefusal::LocationExpired),
        }
    }

    // 2. Pool/local locations — `pool:`/`mem:`/`bundle:` spellings, a
    //    bare content address (`sha256:`/`idp:`-shape), or no scheme at
    //    all — the pool resolves by address; no trust leg applies.
    let scheme = match location_scheme(location) {
        None => return Ok(LocationLeg::Pool),
        Some(s) if s == "pool" || s == "mem" || s == "bundle" => return Ok(LocationLeg::Pool),
        Some(s) => s,
    };

    // 3. A credentialed locator refuses before policy is even consulted
    //    (R-ID-7 — literal credential material never reaches a transport
    //    classification; the placeholder form is the safe carrier), then
    //    remote legs need the declared policy.
    if credential_literal(location) {
        return Err(FetchRefusal::TransportRefused);
    }
    let policy = policy.ok_or(FetchRefusal::PolicyAbsent)?;
    match scheme.as_str() {
        "http" => Err(FetchRefusal::TransportRefused),
        "https" => {
            let (host, port) = https_authority(location).ok_or(FetchRefusal::TransportRefused)?;
            let class = if policy.origin_admitted(&host, port) {
                AdmissionClass::FirstParty
            } else if has_signed_params(location) {
                AdmissionClass::SignedUrl
            } else {
                return Err(FetchRefusal::OriginNotAdmitted);
            };
            if !policy.admits(class) {
                return Err(FetchRefusal::ClassNotAdmitted);
            }
            // The carrier's own embedded expiry rides too — an expired
            // signature is expired, whatever `entry.expires` says.
            if let Some(exp_ms) = embedded_expiry_ms(location) {
                if exp_ms <= now_ms {
                    return Err(FetchRefusal::LocationExpired);
                }
            }
            Ok(LocationLeg::HttpsGet {
                class,
                url: location.to_string(),
            })
        }
        "ipfs" => {
            let class = AdmissionClass::ContentAddressed;
            if !policy.admits(class) {
                return Err(FetchRefusal::ClassNotAdmitted);
            }
            Ok(LocationLeg::Unsupported { class, scheme })
        }
        _ => Err(FetchRefusal::TransportRefused),
    }
}

/// The `bundle.member.fetched{address, location_uri, fetched_at,
/// transport, outcome}` payload — the one spelling (CC1): `fetch`
/// builds the rows, the boundary appends them durable-before-visible.
/// `transport` is the attempted transport (`pool` | `https` | `ipfs` |
/// `http`); `outcome` is `verified` | `refused:<arm>` | `error:<detail>`
/// | `digest_mismatch`.
pub fn member_fetched_payload(
    address: &str,
    location_uri: &str,
    transport: &str,
    outcome: &str,
    fetched_at_ms: u64,
) -> Json {
    Json::obj([
        ("address", Json::str(address)),
        ("location_uri", Json::str(location_uri)),
        (
            "fetched_at",
            Json::str(hh_ledger::store::rfc3339_ms(fetched_at_ms)),
        ),
        ("transport", Json::str(transport)),
        ("outcome", Json::str(outcome)),
    ])
}
