//! `proxy` — the model-mediation proxy core (experiments spec §4.1–§4.3;
//! R6.10 / PROXY-CORE-a; AR-08; REQ-XC-101/103/105/106/107/108/109/007/208;
//! design/33 F2–F9).
//!
//! Placement (F2, AR-08): the proxy lives in the hosting tier as
//! `hharness-hosting::proxy` plus the `hh-model-proxy` binary
//! (`src/bin/hh-model-proxy.rs`), depending only downward (gateway,
//! budget, ledger, secrets, wire, tls). The runner spawns the binary
//! and drives it over a Json control channel on stdio — no workspace
//! crate names `hharness-hosting`, so the seam stays serialised.
//!
//! Per call the phase proxy run's fenced writer lands the §4.3 ledger
//! order (requested → reserved → attempt → blobs → terminal →
//! settlement) — and **no response byte reaches the
//! downstream writer until step 6 is durable** (X-04 durable-before-
//! visible). Every blob is `leak_scan`ned against the mask set {Ollama
//! key, every live slot token} before deposit: an Ollama-key hit refuses
//! the write and stops the phase; a slot-token hit is recorded and
//! permitted (REQ-XC-109).
//!
//! Until R6.11 lands there is exactly one release mode — `buffered` —
//! and one reservation form — the fixture lane's declared bound; a
//! streaming request gets the typed, surface-shaped, receipted
//! `Unsupported{release_mode: buffered_heartbeat}` refusal, and a lane
//! without the declared bound refuses at ledger step 2
//! (`Unsupported{reservation_formula}`).

pub mod apply;
pub mod core_;
pub mod lane;

pub use apply::{apply_policy, error_body, filter_headers, FilteredHeaders, Mutations, Refusal};
pub use core_::{ControlOutcome, Proxy, ProxyConfig, ProxyError};
pub use lane::{FieldOp, FieldRule, ProxyLane, Reservation, Route, Surface};

/// The proxy implementation identity — lands in every receipt's
/// `proxy_version` member and the participant identity hash
/// (REQ-XC-208: a changed rewrite version is a new participant).
pub const PROXY_VERSION: &str = "hh-model-proxy/0.1.0";

/// The additive `proxy` member schema on `hh.model_io/1` (§4.3).
pub const RECEIPT_SCHEMA: &str = "hh.model_proxy.receipt/1";

/// The receipt media type (canonical JSON document).
pub const RECEIPT_MEDIA: &str = "application/json";
