//! `hh-wire` — transport primitives shared by the kernel and every generated client.
//!
//! Scope (Stage 0, ADR-0050 (e); §7.4 binding (b)): canonical JSON, SHA-256 content
//! addressing, and newline-delimited JSON-RPC 2.0 framing. This crate is pure standard
//! library so the whole toolchain builds and the boundary spike runs offline. It carries no
//! `hh-embed/1` verb, field or error variant — those live in `hh-embed-schema`, the single
//! schema source (CC7).

pub mod json;
pub mod jsonrpc;
pub mod sha256;

pub use json::{parse, Json, JsonError};
pub use sha256::sha256_hex;
