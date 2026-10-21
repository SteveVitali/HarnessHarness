//! `hh-tls` — the workspace's one TLS home (R4.5c; ADR-0365).
//!
//! Scope: TLS client sessions for the model boundary (`hh-gateway`'s
//! `HttpModelTransport`) and the egress mediator (`hh-env`'s
//! `LocalTlsTransport`), TLS server configs for the committed test
//! fixtures and the R4.7a credential mediator, mTLS client-certificate
//! presentation, and verified peer-identity (subject/SAN) extraction —
//! all over the admitted `rustls =0.23.46`/`ring =0.17.14` graph
//! (ADR-0365; this is the only member that may name it).
//!
//! Contract: certificate verification is always on — there is no
//! insecure toggle; extra trust roots enter only through an explicitly
//! named PEM file (the local-CA hook); a refused handshake is a typed
//! [`TlsError`], never a silent plaintext downgrade.

#![forbid(unsafe_code)]

pub mod client;
pub mod error;
pub mod identity;
pub mod pem;
pub mod server;

pub use client::{ClientTls, TlsStream};
pub use error::TlsError;
pub use identity::PeerIdentity;
pub use server::{ServerTls, ServerTlsStream};
