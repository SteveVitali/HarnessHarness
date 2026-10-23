//! `error` — the typed TLS refusal (R4.5c; ADR-0365).
//!
//! Every failure of the admitted stack lands as one of these variants —
//! callers map them onto their own typed classes (a model-transport
//! `Network`/`UnsupportedFeature`, an egress `Transport` detail string).
//! No variant silently disables verification; a failed TLS leg is a
//! refusal, never a plaintext downgrade.

/// One typed refusal on the TLS boundary.
#[derive(Debug)]
pub enum TlsError {
    /// Socket I/O failure beneath the session (dial, handshake transport,
    /// read/write — the `io::Error` spelling preserved).
    Io(String),
    /// The TLS handshake refused — certificate verification (untrusted
    /// issuer, name mismatch, expired), protocol version or the peer's
    /// own alert (the rustls alert/error spelling preserved).
    Handshake(String),
    /// Configured PEM material is unreadable or malformed (a roots file
    /// with no CERTIFICATE blocks, a truncated key).
    BadPem(String),
    /// The configured key material does not match its certificate chain,
    /// or a required half (cert without key, key without cert) is absent.
    Config(String),
    /// The dialled host cannot be a TLS `ServerName`.
    BadServerName(String),
}

impl std::fmt::Display for TlsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlsError::Io(d) => write!(f, "io: {d}"),
            TlsError::Handshake(d) => write!(f, "handshake: {d}"),
            TlsError::BadPem(d) => write!(f, "pem: {d}"),
            TlsError::Config(d) => write!(f, "config: {d}"),
            TlsError::BadServerName(d) => write!(f, "server-name: {d}"),
        }
    }
}

impl std::error::Error for TlsError {}

impl From<std::io::Error> for TlsError {
    fn from(e: std::io::Error) -> TlsError {
        TlsError::Io(e.to_string())
    }
}
