//! `server` — the TLS-terminating server side (R4.5c; ADR-0365 D2).
//!
//! `ServerTls` terminates TLS at this process: for the committed test
//! fixtures (`crates/hh-tls/fixtures/pki`) today and the R4.7a credential
//! mediator's server surface next. `requiring_client_auth` verifies a
//! presented client chain against a named root set — the mTLS half — and
//! `ServerTlsStream::peer_identity` extracts the *verified* client
//! certificate's subject/SAN (the hook the mediator consumes; a refused
//! handshake never yields an identity).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig, ServerConnection, StreamOwned};

use crate::error::TlsError;
use crate::identity::{self, PeerIdentity};
use crate::pem;

/// A TLS-terminating server configuration — cert chain + key, plus the
/// client-auth posture.
pub struct ServerTls {
    config: Arc<ServerConfig>,
}

impl ServerTls {
    /// `new(chain_pem, key_pem)` — server terminating TLS, no client
    /// authentication (the plain-HTTPS fixture shape).
    pub fn new(chain_pem: &str, key_pem: &str) -> Result<ServerTls, TlsError> {
        let (chain, key) = load_identity(chain_pem, key_pem)?;
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .map_err(|e| TlsError::Config(format!("server identity: {e}")))?;
        Ok(ServerTls {
            config: Arc::new(config),
        })
    }

    /// `requiring_client_auth(chain_pem, key_pem, roots_pem)` — the mTLS
    /// shape: clients must present a chain verifiable against the PEM
    /// `CERTIFICATE` blocks in `roots_pem`; a missing/foreign client
    /// chain fails the handshake (typed — never an unverified peer).
    pub fn requiring_client_auth(
        chain_pem: &str,
        key_pem: &str,
        roots_pem: &str,
    ) -> Result<ServerTls, TlsError> {
        let (chain, key) = load_identity(chain_pem, key_pem)?;
        let mut roots = RootCertStore::empty();
        let certs: Vec<CertificateDer<'static>> = pem::certificates_pem(roots_pem)
            .into_iter()
            .map(CertificateDer::from)
            .collect();
        if certs.is_empty() {
            return Err(TlsError::BadPem(
                "client-auth roots: no CERTIFICATE blocks".to_string(),
            ));
        }
        roots.add_parsable_certificates(certs);
        let verifier = WebPkiClientVerifier::builder(Arc::new(roots))
            .build()
            .map_err(|e| TlsError::Config(format!("client verifier: {e}")))?;
        let config = ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(chain, key)
            .map_err(|e| TlsError::Config(format!("server identity: {e}")))?;
        Ok(ServerTls {
            config: Arc::new(config),
        })
    }

    /// `accept(sock)` — the real handshake over the accepted stream;
    /// a refused handshake (missing/invalid client chain, version floor)
    /// is a typed `Handshake`, never an unverified peer.
    pub fn accept(&self, sock: TcpStream) -> Result<ServerTlsStream, TlsError> {
        let mut conn = ServerConnection::new(self.config.clone())
            .map_err(|e| TlsError::Config(e.to_string()))?;
        let mut sock = sock;
        while conn.is_handshaking() {
            conn.complete_io(&mut sock)
                .map_err(crate::client::map_handshake_io)?;
        }
        Ok(ServerTlsStream {
            inner: StreamOwned::new(conn, sock),
        })
    }
}

/// `load_identity(chain_pem, key_pem)` — the server's cert chain + key.
pub(crate) fn load_identity(
    chain_pem: &str,
    key_pem: &str,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), TlsError> {
    let chain: Vec<CertificateDer<'static>> = pem::certificates_pem(chain_pem)
        .into_iter()
        .map(CertificateDer::from)
        .collect();
    if chain.is_empty() {
        return Err(TlsError::BadPem(
            "server identity: no CERTIFICATE blocks in the chain".to_string(),
        ));
    }
    let key = crate::client::load_private_key(key_pem).map_err(|e| match e {
        TlsError::BadPem(d) => TlsError::BadPem(format!("server identity: {d}")),
        other => other,
    })?;
    Ok((chain, key))
}

/// The established server-side TLS session — `Read + Write`; the
/// `peer_identity` is the *client's* verified certificate identity
/// (the mTLS half the R4.7a mediator consumes).
pub struct ServerTlsStream {
    inner: StreamOwned<ServerConnection, TcpStream>,
}

impl ServerTlsStream {
    /// The verified client (peer) identity — subject CN + SAN from the
    /// client certificate the handshake already validated. `None` under
    /// `with_no_client_auth` (no peer chain exists — reported honestly).
    pub fn peer_identity(&self) -> Option<PeerIdentity> {
        let chain = self.inner.conn.peer_certificates()?;
        identity::peer_identity(chain.first()?)
    }

    /// Switch the underlying socket's blocking mode — the R4.7a
    /// mediator's multiplex arm completes the handshake on bounded
    /// deadlines, then round-robins the conn nonblocking (rustls
    /// surfaces `WouldBlock` as a typed `io::Error`, never a partial
    /// frame).
    pub fn set_nonblocking(&self, nonblocking: bool) -> std::io::Result<()> {
        self.inner.sock.set_nonblocking(nonblocking)
    }

    /// `release()` — the refuse-then-close path's orderly release, for a
    /// caller that has already written + flushed its terminal reply:
    /// blocking mode is restored, `close_notify` is sent and flushed now
    /// (the `Drop` arm's later re-send is inert), the socket's write
    /// half shuts so the queued reply stays deliverable, then the peer's
    /// in-flight bytes drain to EOF — bounded by the socket's own read
    /// deadline. A bare `close()` on a socket still holding unread
    /// input answers RST on BSD loopback and can discard the queued
    /// reply — observed on the L4.3 macOS leg, where the
    /// unmapped-subject `401` surfaced client-side as ECONNRESET.
    pub fn release(&mut self) {
        let _ = self.inner.sock.set_nonblocking(false);
        self.inner.conn.send_close_notify();
        let _ = self.inner.flush();
        let _ = self.inner.sock.shutdown(std::net::Shutdown::Write);
        let mut sink = [0u8; 8192];
        while matches!(self.inner.read(&mut sink), Ok(n) if n > 0) {}
    }
}

impl Read for ServerTlsStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Write for ServerTlsStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl Drop for ServerTlsStream {
    /// Close cleanly — `close_notify` then a flush, so the peer's
    /// close-delimited read ends on a real EOF, not `UnexpectedEof`.
    fn drop(&mut self) {
        self.inner.conn.send_close_notify();
        let _ = self.inner.conn.complete_io(&mut self.inner.sock);
    }
}
