//! `client` — the HTTPS client side (R4.5c; ADR-0365 D2).
//!
//! `ClientTls` carries the root set and the optional client identity;
//! `connect` performs the real handshake over the caller's dialed
//! `TcpStream` (read/write deadlines already set) and returns the
//! `Read + Write` [`TlsStream`] the `hharness_wire::http` codec rides (the
//! ADR-0364 D1 "one codec, streams beneath it" rule).
//!
//! Posture: verification is **always on** — webpki path validation
//! against the root set, `ServerName` checked against the certificate's
//! SAN, TLS 1.2+ only. There is no insecure toggle. The default root set
//! is the Mozilla bundle `webpki-roots` ships; additional roots arrive
//! only via [`ClientTls::extra_roots_file`] — an explicitly named PEM
//! file (the local-CA hook for fixtures and private issuers; never an
//! env var, never a silent skip on malformed content).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::error::TlsError;
use crate::identity::{self, PeerIdentity};
use crate::pem;

/// The client-side TLS configuration — root set + optional client
/// identity (mTLS). Cloneable; the `ClientConfig` is rebuilt per connect
/// (cheap — the stores are `Arc` internals).
#[derive(Clone)]
pub struct ClientTls {
    /// The trust roots — Mozilla's bundle plus any explicit extras.
    roots: RootCertStore,
    /// The client identity (chain DER + kind-tagged key DER) presented
    /// when the server asks (mTLS) — raw DER so the struct stays
    /// `Clone`; re-kinded at connect.
    identity: Option<ClientIdentityDer>,
}

/// `(cert_chain_der, key_kind, private_key_der)` — the cloneable raw
/// store behind [`ClientTls::identity`] (re-kinded into
/// `PrivateKeyDer` at connect).
type ClientIdentityDer = (Vec<Vec<u8>>, &'static str, Vec<u8>);

impl Default for ClientTls {
    fn default() -> Self {
        Self::webpki_roots()
    }
}

impl ClientTls {
    /// `webpki_roots()` — the public-Internet client: the Mozilla root
    /// set, no client identity.
    pub fn webpki_roots() -> ClientTls {
        ClientTls {
            roots: RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned()),
            identity: None,
        }
    }

    /// `extra_roots_file(path)` — builder: additionally trust the
    /// `CERTIFICATE` PEM blocks in the explicitly named file (the
    /// local-CA hook). An unreadable file or one holding no certificate
    /// blocks is a typed `BadPem` — never a silently smaller root set.
    pub fn extra_roots_file(mut self, path: &std::path::Path) -> Result<ClientTls, TlsError> {
        let text = pem::read_pem(path)?;
        let certs: Vec<CertificateDer<'static>> = pem::certificates_pem(&text)
            .into_iter()
            .map(CertificateDer::from)
            .collect();
        if certs.is_empty() {
            return Err(TlsError::BadPem(format!(
                "{}: no CERTIFICATE blocks — the extra-roots file must carry at least one",
                path.display()
            )));
        }
        let n_certs = certs.len();
        let (added, ignored) = self.roots.add_parsable_certificates(certs);
        if added == 0 {
            return Err(TlsError::BadPem(format!(
                "{}: {} certificate block(s), none parsable as trust anchors{ignored}",
                path.display(),
                n_certs,
                ignored = if ignored > 0 {
                    format!(" ({ignored} ignored)")
                } else {
                    String::new()
                },
            )));
        }
        Ok(self)
    }

    /// `client_identity(chain_pem, key_pem)` — builder: the mTLS
    /// certificate chain + private key (PEM text) presented when the
    /// server requests client auth. Both halves are required — a chain
    /// without a key is a typed `Config` refusal.
    pub fn client_identity(
        mut self,
        chain_pem: &str,
        key_pem: &str,
    ) -> Result<ClientTls, TlsError> {
        let chain: Vec<CertificateDer<'static>> = pem::certificates_pem(chain_pem)
            .into_iter()
            .map(CertificateDer::from)
            .collect();
        if chain.is_empty() {
            return Err(TlsError::BadPem(
                "client identity: no CERTIFICATE blocks in the chain".to_string(),
            ));
        }
        let (kind, key_der) = pem::private_key_pem(key_pem)
            .ok_or_else(|| TlsError::BadPem("client identity: no PRIVATE KEY block".to_string()))?;
        self.identity = Some((
            chain.iter().map(|c| c.as_ref().to_vec()).collect(),
            kind,
            key_der,
        ));
        Ok(self)
    }

    /// `connect(sock, server_name)` — the real handshake over `sock`
    /// (connected + deadlines set by the caller). A refusal — untrusted
    /// issuer, name mismatch, version floor — is a typed `Handshake`;
    /// never a plaintext fallback.
    pub fn connect(&self, sock: TcpStream, server_name: &str) -> Result<TlsStream, TlsError> {
        let name = ServerName::try_from(server_name.to_string())
            .map_err(|e| TlsError::BadServerName(format!("{server_name:?}: {e}")))?;
        let builder = ClientConfig::builder().with_root_certificates(self.roots.clone());
        let config = match &self.identity {
            Some((chain, kind, key)) => builder
                .with_client_auth_cert(
                    chain
                        .iter()
                        .map(|d| CertificateDer::from(d.clone()))
                        .collect::<Vec<_>>(),
                    private_key_der_kind(kind, key.clone()),
                )
                .map_err(|e| TlsError::Config(format!("client identity: {e}")))?,
            None => builder.with_no_client_auth(),
        };
        let mut conn = ClientConnection::new(Arc::new(config), name)
            .map_err(|e| TlsError::Handshake(e.to_string()))?;
        let mut sock = sock;
        while conn.is_handshaking() {
            conn.complete_io(&mut sock).map_err(map_handshake_io)?;
        }
        Ok(TlsStream {
            inner: StreamOwned::new(conn, sock),
        })
    }
}

/// `load_private_key(pem)` — the first private-key block (PKCS8 / PKCS1 /
/// SEC1) as `PrivateKeyDer`. Absent material is a typed refusal. The one
/// key loader — the server's `load_identity` consumes it too (CC1).
pub(crate) fn load_private_key(text: &str) -> Result<PrivateKeyDer<'static>, TlsError> {
    let (kind, der) = pem::private_key_pem(text)
        .ok_or_else(|| TlsError::BadPem("no PRIVATE KEY block in the key material".to_string()))?;
    Ok(private_key_der_kind(kind, der))
}

/// `private_key_der_kind(kind, der)` — the `(kind, der)` pair `pem` yields
/// → the typed key. The identity path stores raw DER and re-kinds it here.
pub(crate) fn private_key_der_kind(kind: &str, der: Vec<u8>) -> PrivateKeyDer<'static> {
    match kind {
        "pkcs1" => PrivateKeyDer::Pkcs1(rustls::pki_types::PrivatePkcs1KeyDer::from(der)),
        "sec1" => PrivateKeyDer::Sec1(rustls::pki_types::PrivateSec1KeyDer::from(der)),
        _ => PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(der)),
    }
}

/// Classify a handshake-time `io::Error`: rustls surfaces its own alerts
/// wrapped in `InvalidData`/`Other` — those are `Handshake` refusals;
/// socket-deadline and transport failures are `Io` (never conflated).
/// Shared by the client and server handshake loops (CC1).
pub(crate) fn map_handshake_io(e: std::io::Error) -> TlsError {
    if e.get_ref().is_some_and(|r| r.is::<rustls::Error>()) {
        return TlsError::Handshake(e.to_string());
    }
    match e.kind() {
        std::io::ErrorKind::InvalidData => TlsError::Handshake(e.to_string()),
        _ => TlsError::Io(e.to_string()),
    }
}

/// The established client-side TLS session — `Read + Write` for the HTTP
/// codec; `peer_identity` reports the *verified* peer certificate's
/// identity (observed, never asserted).
pub struct TlsStream {
    inner: StreamOwned<ClientConnection, TcpStream>,
}

impl TlsStream {
    /// The verified peer (server) identity — subject common name + SAN
    /// names from the certificate the handshake already validated.
    /// `None` when the peer presented no chain (a server always does).
    pub fn peer_identity(&self) -> Option<PeerIdentity> {
        let chain = self.inner.conn.peer_certificates()?;
        identity::peer_identity(chain.first()?)
    }
}

impl Read for TlsStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Write for TlsStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl Drop for TlsStream {
    /// Close cleanly — `close_notify` then a flush, so the peer's
    /// close-delimited read ends on a real EOF, not `UnexpectedEof`.
    fn drop(&mut self) {
        self.inner.conn.send_close_notify();
        let _ = self.inner.conn.complete_io(&mut self.inner.sock);
    }
}
