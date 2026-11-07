//! `replay` — the record/replay boundary machinery (experiments spec
//! §4.9; R6.12 / REPLAY; REQ-XC-122; design/33 F15; design/36 §1.3,
//! §1.6 P3–P8).
//!
//! Two pieces, both boundary-level and offline by construction:
//!
//! - [`RequestCanonicalizer`] — the hook the replay matcher compares
//!   upstream-bound request bytes through. Built-ins: `identity` (byte
//!   equality), `json_canonical` (the workspace canonical JSON —
//!   `parse_provider` → [`hharness_wire::json::Json::to_canonical_string`],
//!   never a second canonicalizer — CC1), and `hh-l1-canon` (the
//!   versioned `hh-l1-canon/1` allowlist in `hharness-wire`; C01–C07).
//! - [`RecordedTransport`] — a [`crate::gateway::Transport`] serving a
//!   recorded sequence of upstream exchanges to HH-native runs: the
//!   same recorded bytes the proxy's `RecordedUpstream` serves, decoded
//!   through the family's own translators
//!   ([`crate::http_transport::decode_exchange_frames`]), so one
//!   recording drives both the original harness and its HIR port.
//!
//! Replay is boundary-level only — it cannot reproduce cell-side
//! nondeterminism (design/33 F15 limitations).

use hharness_wire::json::{parse_preserve, parse_provider};
use hharness_wire::l1_canon::{first_diff_path, L1Canon};

use crate::codec::WireFrame;
use crate::dialect::WireDialect;
use crate::gateway::{CredentialHandle, Transport};
use crate::http_transport::{decode_exchange_frames, family_of, Family, RawExchange};
use crate::vocab::{ModelError, ModelErrorClass};

/// `Canonical { bytes, dropped_paths }` — one canonicalizer's output:
/// the canonical request bytes plus the member paths the ruleset
/// dropped (the verdict's recorded detail).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canonical {
    /// The canonical request bytes (the comparison input).
    pub bytes: Vec<u8>,
    /// Member paths the ruleset dropped from comparison.
    pub dropped_paths: Vec<String>,
}

/// A canonicalizer's typed refusal — never a fabricated view (a
/// non-JSON body under `json_canonical`/`hh-l1-canon` is a refusal,
/// never an identity fallback).
#[derive(Debug, Clone)]
pub struct CanonRefusal {
    /// What refused.
    pub detail: String,
}

impl std::fmt::Display for CanonRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "canonicalize: {}", self.detail)
    }
}
impl std::error::Error for CanonRefusal {}

/// `RequestCanonicalizer` — the §4.9 comparison hook (design/33 F15's
/// interface). `canonicalize(surface, bytes)` produces the canonical
/// request view the replay matcher compares; `id` is the recorded
/// ruleset spelling (a changed ruleset is a new comparison — REQ-XC-208's
/// versioning rule applies here).
pub trait RequestCanonicalizer {
    /// The ruleset spelling (`identity` | `json_canonical` |
    /// `hh-l1-canon/1` | a registered D2 ruleset id).
    fn id(&self) -> &str;
    /// `canonicalize(surface, bytes)` — the canonical view plus the
    /// dropped paths, or a typed refusal. `surface` is the capture
    /// point's surface spelling (`openai.chat` | `anthropic.messages` |
    /// `models` | `""` when unsurfaced).
    fn canonicalize(&self, surface: &str, bytes: &[u8]) -> Result<Canonical, CanonRefusal>;
}

/// `identity` — byte equality (C01's floor: no masking at all).
pub struct IdentityCanonicalizer;

impl RequestCanonicalizer for IdentityCanonicalizer {
    fn id(&self) -> &str {
        "identity"
    }
    fn canonicalize(&self, _surface: &str, bytes: &[u8]) -> Result<Canonical, CanonRefusal> {
        Ok(Canonical {
            bytes: bytes.to_vec(),
            dropped_paths: Vec::new(),
        })
    }
}

/// `json_canonical` — the workspace canonical JSON: `parse_provider`
/// (the inbound provider-bytes read — non-integer lexemes preserve as
/// strings, never refused) →
/// [`hharness_wire::json::Json::to_canonical_string`]. The one
/// canonicalizer (CC1). `dropped_paths` is empty — C01 alone drops
/// nothing.
pub struct JsonCanonicalizer;

impl RequestCanonicalizer for JsonCanonicalizer {
    fn id(&self) -> &str {
        "json_canonical"
    }
    fn canonicalize(&self, _surface: &str, bytes: &[u8]) -> Result<Canonical, CanonRefusal> {
        let doc = parse_provider(&String::from_utf8_lossy(bytes)).map_err(|e| CanonRefusal {
            detail: format!("json_canonical: {e}"),
        })?;
        Ok(Canonical {
            bytes: doc.to_canonical_string().into_bytes(),
            dropped_paths: Vec::new(),
        })
    }
}

/// `hh-l1-canon` — the versioned `hh-l1-canon/1` allowlist (C01–C07):
/// the canonical view is [`L1Canon::view`]'s document serialized
/// `to_provider_string`; the masked paths ride `dropped_paths`.
/// `cell_facts` are the recorded cell facts C04's verified substitution
/// admits (empty = no substitution).
pub struct L1Canonicalizer {
    canon: L1Canon,
}

impl L1Canonicalizer {
    /// `L1Canonicalizer::v1(cell_facts)` — `hh-l1-canon/1` with the
    /// recorded cell facts C04 substitutes against.
    pub fn v1(cell_facts: &[(String, String)]) -> L1Canonicalizer {
        L1Canonicalizer {
            canon: L1Canon::v1(cell_facts),
        }
    }
}

impl RequestCanonicalizer for L1Canonicalizer {
    fn id(&self) -> &str {
        "hh-l1-canon/1"
    }
    fn canonicalize(&self, _surface: &str, bytes: &[u8]) -> Result<Canonical, CanonRefusal> {
        let doc = parse_preserve(&String::from_utf8_lossy(bytes)).map_err(|e| CanonRefusal {
            detail: format!("hh-l1-canon: {e}"),
        })?;
        let view = self.canon.view(&doc);
        Ok(Canonical {
            bytes: view.doc.to_provider_string().into_bytes(),
            dropped_paths: view.masked.iter().map(|m| m.path.clone()).collect(),
        })
    }
}

/// `canonicalizer_for(id, cell_facts)` — bind a built-in by spelling
/// (the replay config's declared `canonicalizer`). Unknown spellings
/// refuse typed — a declared ruleset is never silently downgraded.
pub fn canonicalizer_for(
    id: &str,
    cell_facts: &[(String, String)],
) -> Result<Box<dyn RequestCanonicalizer>, CanonRefusal> {
    match id {
        "identity" => Ok(Box::new(IdentityCanonicalizer)),
        "json_canonical" => Ok(Box::new(JsonCanonicalizer)),
        "hh-l1-canon" | "hh-l1-canon/1" => Ok(Box::new(L1Canonicalizer::v1(cell_facts))),
        other => Err(CanonRefusal {
            detail: format!("unknown canonicalizer {other}"),
        }),
    }
}

/// The per-call match verdict — `{exact | canonical_equal |
/// diverged{first_diff_path}}` (§4.9). `exact` = the request bytes are
/// identical; `canonical_equal` = equal only after the ruleset applied;
/// `diverged` carries the first difference path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchVerdict {
    /// Byte-identical.
    Exact,
    /// Equal after canonicalization only.
    CanonicalEqual,
    /// Different after canonicalization — the first difference path.
    Diverged {
        /// The first structural difference path (JSON member path, or
        /// `bytes` when the documents are not comparable JSON).
        first_diff_path: String,
    },
}

impl MatchVerdict {
    /// The recorded spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            MatchVerdict::Exact => "exact",
            MatchVerdict::CanonicalEqual => "canonical_equal",
            MatchVerdict::Diverged { .. } => "diverged",
        }
    }
}

/// `match_requests(canon, surface, expected, actual)` — the per-call
/// verdict. `expected` is the recorded upstream-bound request,
/// `actual` the replayed harness's upstream-bound bytes. A
/// canonicalizer refusal on either side is itself a divergence — the
/// path names it, never a silent pass.
pub fn match_requests(
    canon: &dyn RequestCanonicalizer,
    surface: &str,
    expected: &[u8],
    actual: &[u8],
) -> Result<MatchVerdict, CanonRefusal> {
    if expected == actual {
        return Ok(MatchVerdict::Exact);
    }
    let ce = canon.canonicalize(surface, expected)?;
    let ca = canon.canonicalize(surface, actual)?;
    if ce.bytes == ca.bytes {
        return Ok(MatchVerdict::CanonicalEqual);
    }
    let diff = match (
        parse_preserve(&String::from_utf8_lossy(&ce.bytes)).ok(),
        parse_preserve(&String::from_utf8_lossy(&ca.bytes)).ok(),
    ) {
        (Some(a), Some(b)) => first_diff_path(&a, &b).unwrap_or_else(|| "bytes".to_string()),
        _ => "bytes".to_string(),
    };
    Ok(MatchVerdict::Diverged {
        first_diff_path: diff,
    })
}

/// One recorded upstream exchange plus the request that produced it —
/// the gateway-side row of the §4.9 recording format. `request_body`
/// is the recorded *upstream-bound* request (the bytes the proxy
/// computed after lane transforms — the L1 gate's comparison point,
/// design/36 §1.3).
#[derive(Debug, Clone)]
pub struct RecordedCall {
    /// The recorded upstream-bound request bytes (post lane
    /// transforms), empty for a call refused before dispatch.
    pub request_body: Vec<u8>,
    /// The recorded exchange verbatim (`status = 0` = the head never
    /// arrived — the `send` leg maps it to a transport-class error).
    pub exchange: RawExchange,
    /// A recorded transport failure (`(class, detail)`) — present when
    /// the recorded call failed at the boundary rather than completing
    /// an HTTP exchange.
    pub failure: Option<(ModelErrorClass, String)>,
}

/// `RecordedTransport` — the §4.9 [`Transport`] serving one recording's
/// ordered upstream exchanges to HH-native runs (P3): `send` k returns
/// the k-th recorded exchange decoded through the family's own
/// translators, so the emitted `WireFrame`s equal the live path's for
/// the same wire bytes. A request beyond the sequence is a typed
/// `invalid_request` refusal — the end-of-sequence leg, never a
/// fabricated response (P5). When a canonicalizer is bound, each call's
/// match verdict is recorded on [`RecordedTransport::verdicts`]; a
/// `strict` transport answers a divergence with the same typed refusal
/// instead of serving (P4).
pub struct RecordedTransport {
    family: Family,
    max_body: usize,
    surface: String,
    calls: Vec<RecordedCall>,
    pos: usize,
    canonicalizer: Option<Box<dyn RequestCanonicalizer>>,
    strict: bool,
    verdicts: Vec<MatchVerdict>,
}

impl RecordedTransport {
    /// `new(dialect, calls)` — a transport over the recording; the
    /// bound dialect fixes the provider family the recorded bytes
    /// decode through.
    pub fn new(
        dialect: &WireDialect,
        surface: &str,
        calls: Vec<RecordedCall>,
    ) -> RecordedTransport {
        RecordedTransport {
            family: family_of(dialect),
            max_body: 16 * 1024 * 1024,
            surface: surface.to_string(),
            calls,
            pos: 0,
            canonicalizer: None,
            strict: false,
            verdicts: Vec::new(),
        }
    }

    /// Bind the canonicalizer hook + strictness (P4). `strict` answers
    /// a divergence with the typed refusal; lenient serves and records.
    pub fn with_matcher(
        mut self,
        canonicalizer: Box<dyn RequestCanonicalizer>,
        strict: bool,
    ) -> RecordedTransport {
        self.canonicalizer = Some(canonicalizer);
        self.strict = strict;
        self
    }

    /// The recorded per-call verdicts, in serve order.
    pub fn verdicts(&self) -> &[MatchVerdict] {
        &self.verdicts
    }

    /// How many recorded calls have been served.
    pub fn served(&self) -> usize {
        self.pos
    }

    /// The sequence length.
    pub fn len(&self) -> usize {
        self.calls.len()
    }

    /// `true` when the sequence is exhausted.
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }
}

impl Transport for RecordedTransport {
    fn send(
        &mut self,
        bytes: &[u8],
        _credential: &CredentialHandle,
        _endpoint_ref: &str,
    ) -> Result<Vec<(u64, WireFrame)>, ModelError> {
        let call = self.calls.get(self.pos).ok_or_else(|| {
            let mut e = ModelError::new(
                ModelErrorClass::InvalidRequest,
                format!(
                    "replay: recorded sequence exhausted at k={} ({} calls)",
                    self.pos,
                    self.calls.len()
                ),
            );
            e.provider_code = Some("replay_exhausted".to_string());
            e
        })?;
        self.pos += 1;
        if let Some(canon) = &self.canonicalizer {
            let verdict = match_requests(canon.as_ref(), &self.surface, &call.request_body, bytes)
                .unwrap_or_else(|r| MatchVerdict::Diverged {
                    first_diff_path: format!("canon_error: {}", r.detail),
                });
            if self.strict {
                if let MatchVerdict::Diverged { first_diff_path } = &verdict {
                    let mut e = ModelError::new(
                        ModelErrorClass::InvalidRequest,
                        format!(
                            "replay_diverged at k={} first_diff_path={first_diff_path}",
                            self.pos - 1
                        ),
                    );
                    e.provider_code = Some("replay_diverged".to_string());
                    self.verdicts.push(verdict);
                    return Err(e);
                }
            }
            self.verdicts.push(verdict);
        }
        if let Some((class, detail)) = &call.failure {
            return Err(ModelError::new(*class, detail.clone()));
        }
        decode_exchange_frames(self.family, self.max_body, &call.exchange)
    }
}

/// `recorded_calls(exchanges)` — build the [`RecordedCall`] sequence a
/// `hh.model_proxy.recording/1` decode produces (the format's one codec
/// home is `hharness-hosting`; this helper is the gateway-side shape it
/// maps into): each `(request_body, exchange, failure)` triple.
pub fn recorded_call(
    request_body: Vec<u8>,
    exchange: RawExchange,
    failure: Option<(ModelErrorClass, String)>,
) -> RecordedCall {
    RecordedCall {
        request_body,
        exchange,
        failure,
    }
}
