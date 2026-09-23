//! The `HostedEvent` envelope and its validators (spec §6.6 §2.2; ADR-0164 D4;
//! R-2.10.6⁰).
//!
//! Envelope: `{seq, session, at, kind, payload, provenance{origin, authority},
//! mediation, event_channel, raw_ref?, ext}` — canonical JSON (the `Json`
//! value model; integer-only numbers — CC7).
//!
//! Invariants:
//!
//! * **I-1** — every session has a well-formed lifecycle: `session.opened`
//!   first, `session.closed` last; missing terminals are synthesized
//!   `unobserved` by [`ensure_terminal`] (never silently absent).
//! * **I-2** — `seq` is dense per session (`0..n`, no gaps).
//! * **I-3** — unknown kinds and `_`-prefixed values are preserved verbatim
//!   (a `_`-prefixed enum spelling decodes to the `Ext` arm; an unknown kind
//!   stays a string); nothing is coerced or dropped on decode (T-LCD-07).
//! * **I-4** — the authority ceiling: `authority > delegate` is admissible
//!   only for `origin ∈ {intercept, environment}` rows whose `kind` is
//!   closed-schema (a known hosted kind or a registered native class — the
//!   passthrough/hint surface). An extension or unknown kind never carries
//!   Lab authority.
//! * **I-5** — every hosted ledger row stamps `participant_class = hosted`,
//!   the session's declared `observability_level`, and `mediation`
//!   ([`envelope_stamps`] computes the stamp; the service applies it at
//!   append — S4.5a).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;

use hh_ledger::classes;
use hh_ontology::control::{InfraError, InfraErrorFamily, StopReason};
use hh_ontology::participant::Observability;
use hh_provenance::AuthorityClass;
use hh_wire::Json;

/// `provenance.origin ∈ {participant, intercept, environment, adapter}` — the
/// closed §6.6 sum (distinct from the ledger's `Origin` sum — this is the
/// *channel* the fact arrived on, not the content's lineage). `Ext` preserves
/// a `_`-prefixed spelling verbatim (I-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostedOrigin {
    /// `participant` — the participant emitted the row (a report, never a
    /// kernel-mediated fact).
    Participant,
    /// `intercept` — the Lab intercepted the row at a declared boundary.
    Intercept,
    /// `environment` — the Lab's own surface produced the row (the mediated
    /// tool/permission/egress channels).
    Environment,
    /// `adapter` — the adapter itself produced the row (synthesized
    /// terminals, bookkeeping).
    Adapter,
    /// A `_`-prefixed extension spelling — preserved, never coerced (I-3).
    Ext(String),
}

/// `mediation ∈ {mediated, observed, unobserved}` — the mediation stamp on
/// every event (ADR-0164). `mediated` is admissible only on a kernel-gated
/// channel (see [`validate_event`]); the adapter can never assert it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mediation {
    /// `mediated` — the kernel produced or gated the fact.
    Mediated,
    /// `observed` — the Lab observed the fact on a declared channel.
    Observed,
    /// `unobserved` — reconstructed/synthesized; the Lab saw no channel.
    Unobserved,
    /// A `_`-prefixed extension spelling — preserved verbatim (I-3); counts
    /// as *not* mediated for every rule.
    Ext(String),
}

/// `event_channel ∈ {protocol, hook, log, proxy, handle}` — the channel the
/// event arrived on (ADR-0164 D4).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventChannel {
    /// `protocol` — the session protocol.
    Protocol,
    /// `hook` — a participant-emitted hook.
    Hook,
    /// `log` — a log the adapter scraped.
    Log,
    /// `proxy` — the interception proxy.
    Proxy,
    /// `handle` — a Lab-side handle observation (incl. synthesized rows).
    Handle,
    /// A `_`-prefixed extension spelling — preserved verbatim (I-3).
    Ext(String),
}

/// The `provenance` member — `{origin, authority}` (§6.6 §2.2). This is the
/// *channel pair*, not a full `ProvenanceRecord` — the hosted row's authority
/// is bounded by I-4 and the record-level provenance lands at lift/append.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedProvenance {
    /// The channel origin.
    pub origin: HostedOrigin,
    /// The authority (the closed `AuthorityClass` vocabulary — I-4 bounds it).
    pub authority: AuthorityClass,
}

/// A `HostedEvent` — one row of the hosted presentation.
#[derive(Debug, Clone, PartialEq)]
pub struct HostedEvent {
    /// Dense per-session sequence (I-2).
    pub seq: u64,
    /// The hosted session id.
    pub session: String,
    /// Adapter-monotonic milliseconds (never a wall clock).
    pub at: u64,
    /// The kind — a [`KNOWN_KINDS`] member, a registered native class
    /// (passthrough/hint), or an unknown/`_`-prefixed spelling (preserved).
    pub kind: String,
    /// The kind payload (free `Json`; known kinds carry the §6.6 member
    /// minimum — see [`validate_event`]).
    pub payload: Json,
    /// `{origin, authority}` — the channel pair (I-4 bounds `authority`).
    pub provenance: HostedProvenance,
    /// The mediation stamp (I-5).
    pub mediation: Mediation,
    /// The arrival channel.
    pub event_channel: EventChannel,
    /// A pointer at the raw record (a native `event_id`, a log line ref, …)
    /// — a *reference*, never embedded content.
    pub raw_ref: Option<String>,
    /// `ext` members — preserved verbatim, never deciding validity (CC3).
    pub ext: BTreeMap<String, Json>,
}

/// The closed `hh-hosting/1` kind vocabulary (§6.6 §2.2).
pub const KNOWN_KINDS: &[&str] = &[
    "session.opened",
    "session.closed",
    "turn.started",
    "turn.finished",
    "message.delta",
    "message.completed",
    "thought.delta",
    "tool.proposed",
    "tool.completed",
    "permission.requested",
    "permission.decided",
    "usage.reported",
    "model.call.started",
    "model.call.completed",
    "artefact.delivered",
    "coordinate.changed",
    "compaction.observed",
    "subagent.observed",
    "conformance.observed",
];

/// Whether `kind` is **closed-schema** — a known hosted kind or a registered
/// native class (the passthrough/hint surface). Unknown and `_`-prefixed kinds
/// are *preserved* (I-3) but never closed-schema: I-4 refuses them authority
/// above `delegate`.
pub fn is_known_kind(kind: &str) -> bool {
    KNOWN_KINDS.contains(&kind) || classes::lookup(kind).is_some()
}

/// `HostedEvent` validation failures — typed, never a string (R2).
#[derive(Debug, Clone, PartialEq)]
pub enum HostedError {
    /// `kind` is empty.
    EmptyKind,
    /// `session` is empty.
    EmptySession,
    /// I-2: `seq` is not dense `0..n` within `session`.
    SeqNotDense {
        /// The session the gap occurred in.
        session: String,
        /// The seq the dense order expected.
        expected: u64,
        /// The seq found.
        found: u64,
    },
    /// I-1: the session's first event is not `session.opened`.
    MissingSessionOpen {
        /// The session id.
        session: String,
    },
    /// I-4: `authority > delegate` on a row that is not an
    /// intercept/environment row of a closed-schema kind.
    AuthorityCeiling {
        /// The offending kind.
        kind: String,
        /// The claimed authority.
        authority: String,
        /// The claimed origin.
        origin: String,
    },
    /// `mediation = mediated` on a `participant`/`adapter`-origin row — a
    /// participant-reported claim or the adapter itself may never assert
    /// `mediated` (only the kernel's channels can).
    MediatedRequiresKernelChannel {
        /// The offending kind.
        kind: String,
        /// The claimed origin.
        origin: String,
    },
    /// A `model.call.*` event in a session that did not declare `model_io`
    /// (model I/O is observable only through interception — §6.6 §2.2).
    ModelIoUndeclared {
        /// The offending kind.
        kind: String,
        /// The session id.
        session: String,
    },
    /// A known kind's payload failed its member-minimum check.
    PayloadShape {
        /// The kind.
        kind: String,
        /// What was missing/mistyped.
        detail: String,
    },
    /// A non-`_`-prefixed unknown spelling on a closed enum member at decode
    /// (I-3 preserves `_`-prefixed spellings; anything else is malformed).
    UnknownSpelling {
        /// The envelope member.
        member: String,
        /// The spelling found.
        value: String,
    },
    /// A required envelope member is absent or mistyped at decode.
    Member {
        /// The member path.
        path: String,
        /// What was expected.
        detail: String,
    },
}

impl fmt::Display for HostedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostedError::EmptyKind => write!(f, "EmptyKind"),
            HostedError::EmptySession => write!(f, "EmptySession"),
            HostedError::SeqNotDense {
                session,
                expected,
                found,
            } => write!(
                f,
                "SeqNotDense({session}: expected {expected}, found {found})"
            ),
            HostedError::MissingSessionOpen { session } => {
                write!(f, "MissingSessionOpen({session})")
            }
            HostedError::AuthorityCeiling {
                kind,
                authority,
                origin,
            } => write!(f, "AuthorityCeiling({kind}: {origin}/{authority})"),
            HostedError::MediatedRequiresKernelChannel { kind, origin } => {
                write!(f, "MediatedRequiresKernelChannel({kind}: {origin})")
            }
            HostedError::ModelIoUndeclared { kind, session } => {
                write!(f, "ModelIoUndeclared({kind} in {session})")
            }
            HostedError::PayloadShape { kind, detail } => {
                write!(f, "PayloadShape({kind}: {detail})")
            }
            HostedError::UnknownSpelling { member, value } => {
                write!(f, "UnknownSpelling({member}: {value})")
            }
            HostedError::Member { path, detail } => write!(f, "Member({path}: {detail})"),
        }
    }
}

impl std::error::Error for HostedError {}

fn ext_or_closed<T, F>(s: &str, closed: F, ext: T) -> Result<T, String>
where
    F: Fn(&str) -> Option<T>,
{
    match closed(s) {
        Some(v) => Ok(v),
        None if s.starts_with('_') => Ok(ext),
        None => Err(s.to_string()),
    }
}

impl HostedOrigin {
    /// The canonical spelling.
    pub fn as_str(&self) -> &str {
        match self {
            HostedOrigin::Participant => "participant",
            HostedOrigin::Intercept => "intercept",
            HostedOrigin::Environment => "environment",
            HostedOrigin::Adapter => "adapter",
            HostedOrigin::Ext(s) => s.as_str(),
        }
    }

    /// Parse a spelling (`_`-prefixed → [`HostedOrigin::Ext`]; other unknown
    /// spellings refuse).
    pub fn parse(s: &str) -> Result<HostedOrigin, String> {
        ext_or_closed(
            s,
            |v| {
                Some(match v {
                    "participant" => HostedOrigin::Participant,
                    "intercept" => HostedOrigin::Intercept,
                    "environment" => HostedOrigin::Environment,
                    "adapter" => HostedOrigin::Adapter,
                    _ => return None,
                })
            },
            HostedOrigin::Ext(s.to_string()),
        )
    }
}

impl Mediation {
    /// The canonical spelling.
    pub fn as_str(&self) -> &str {
        match self {
            Mediation::Mediated => "mediated",
            Mediation::Observed => "observed",
            Mediation::Unobserved => "unobserved",
            Mediation::Ext(s) => s.as_str(),
        }
    }

    /// Parse a spelling (`_`-prefixed → [`Mediation::Ext`]; other unknown
    /// spellings refuse).
    pub fn parse(s: &str) -> Result<Mediation, String> {
        ext_or_closed(
            s,
            |v| {
                Some(match v {
                    "mediated" => Mediation::Mediated,
                    "observed" => Mediation::Observed,
                    "unobserved" => Mediation::Unobserved,
                    _ => return None,
                })
            },
            Mediation::Ext(s.to_string()),
        )
    }
}

impl EventChannel {
    /// The canonical spelling.
    pub fn as_str(&self) -> &str {
        match self {
            EventChannel::Protocol => "protocol",
            EventChannel::Hook => "hook",
            EventChannel::Log => "log",
            EventChannel::Proxy => "proxy",
            EventChannel::Handle => "handle",
            EventChannel::Ext(s) => s.as_str(),
        }
    }

    /// Parse a spelling (`_`-prefixed → [`EventChannel::Ext`]; other unknown
    /// spellings refuse).
    pub fn parse(s: &str) -> Result<EventChannel, String> {
        ext_or_closed(
            s,
            |v| {
                Some(match v {
                    "protocol" => EventChannel::Protocol,
                    "hook" => EventChannel::Hook,
                    "log" => EventChannel::Log,
                    "proxy" => EventChannel::Proxy,
                    "handle" => EventChannel::Handle,
                    _ => return None,
                })
            },
            EventChannel::Ext(s.to_string()),
        )
    }
}

impl HostedEvent {
    /// The canonical JSON form — `{seq, session, at, kind, payload,
    /// provenance{origin, authority}, mediation, event_channel, raw_ref?,
    /// ext}` (`ext` is emitted even when empty — one schema shape).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("seq".into(), Json::Int(self.seq as i64));
        m.insert("session".into(), Json::str(&self.session));
        m.insert("at".into(), Json::Int(self.at as i64));
        m.insert("kind".into(), Json::str(&self.kind));
        m.insert("payload".into(), self.payload.clone());
        m.insert(
            "provenance".into(),
            Json::obj([
                ("origin", Json::str(self.provenance.origin.as_str())),
                ("authority", Json::str(self.provenance.authority.as_str())),
            ]),
        );
        m.insert("mediation".into(), Json::str(self.mediation.as_str()));
        m.insert(
            "event_channel".into(),
            Json::str(self.event_channel.as_str()),
        );
        if let Some(r) = &self.raw_ref {
            m.insert("raw_ref".into(), Json::str(r));
        }
        m.insert(
            "ext".into(),
            Json::Obj(
                self.ext
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ),
        );
        Json::Obj(m)
    }

    /// Strict decode — every closed member is typed; `_`-prefixed spellings
    /// on the enum members decode to `Ext` (I-3); unknown *top-level* members
    /// must be `_`-prefixed and fold into `ext` (anything else refuses).
    pub fn from_json(j: &Json) -> Result<HostedEvent, HostedError> {
        let Json::Obj(m) = j else {
            return Err(HostedError::Member {
                path: "event".into(),
                detail: "expected object".into(),
            });
        };
        let req_str = |k: &str| -> Result<String, HostedError> {
            m.get(k)
                .and_then(Json::as_str)
                .map(String::from)
                .ok_or_else(|| HostedError::Member {
                    path: k.into(),
                    detail: "expected string".into(),
                })
        };
        let req_u64 = |k: &str| -> Result<u64, HostedError> {
            match m.get(k) {
                Some(Json::Int(i)) if *i >= 0 => Ok(*i as u64),
                _ => Err(HostedError::Member {
                    path: k.into(),
                    detail: "expected u64".into(),
                }),
            }
        };
        let prov = m.get("provenance").ok_or_else(|| HostedError::Member {
            path: "provenance".into(),
            detail: "missing".into(),
        })?;
        let origin = HostedOrigin::parse(&req_str_at(prov, "origin")?).map_err(|v| {
            HostedError::UnknownSpelling {
                member: "provenance.origin".into(),
                value: v,
            }
        })?;
        let authority =
            AuthorityClass::parse(&req_str_at(prov, "authority")?).ok_or_else(|| {
                HostedError::Member {
                    path: "provenance.authority".into(),
                    detail: "closed vocabulary".into(),
                }
            })?;
        let mut ext = BTreeMap::new();
        if let Some(e) = m.get("ext") {
            let Json::Obj(e) = e else {
                return Err(HostedError::Member {
                    path: "ext".into(),
                    detail: "expected object".into(),
                });
            };
            ext = e.clone();
        }
        // Unknown top-level members fold into `ext` iff `_`-prefixed (I-3).
        const KNOWN: &[&str] = &[
            "seq",
            "session",
            "at",
            "kind",
            "payload",
            "provenance",
            "mediation",
            "event_channel",
            "raw_ref",
            "ext",
        ];
        for (k, v) in m {
            if !KNOWN.contains(&k.as_str()) {
                if !k.starts_with('_') {
                    return Err(HostedError::UnknownSpelling {
                        member: "event".into(),
                        value: k.clone(),
                    });
                }
                ext.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        Ok(HostedEvent {
            seq: req_u64("seq")?,
            session: req_str("session")?,
            at: req_u64("at")?,
            kind: req_str("kind")?,
            payload: m.get("payload").cloned().unwrap_or(Json::Null),
            provenance: HostedProvenance { origin, authority },
            mediation: Mediation::parse(&req_str("mediation")?).map_err(|v| {
                HostedError::UnknownSpelling {
                    member: "mediation".into(),
                    value: v,
                }
            })?,
            event_channel: EventChannel::parse(&req_str("event_channel")?).map_err(|v| {
                HostedError::UnknownSpelling {
                    member: "event_channel".into(),
                    value: v,
                }
            })?,
            raw_ref: m.get("raw_ref").and_then(|r| r.as_str()).map(String::from),
            ext,
        })
    }
}

fn req_str_at(j: &Json, k: &str) -> Result<String, HostedError> {
    j.get(k)
        .and_then(Json::as_str)
        .map(String::from)
        .ok_or_else(|| HostedError::Member {
            path: k.into(),
            detail: "expected string".into(),
        })
}

/// The participant's synthesized-terminal stop reason —
/// `infrastructure_failure{error_class: participant_unclassified}` with the
/// raw word carried in `class` (§6.6 stop-reason lift; the raw spelling rides
/// `stop_reason_raw` on the event).
pub fn participant_unclassified(raw: &str) -> StopReason {
    StopReason::InfrastructureFailure {
        error_class: InfraError {
            family: InfraErrorFamily::Participant,
            class: raw.to_string(),
        },
    }
}

/// I-4 + envelope invariants for a single event.
///
/// * kind/session non-empty;
/// * `mediation = mediated` only on `intercept`/`environment` origins — a
///   participant claim and the adapter itself may never assert `mediated`;
/// * `authority > delegate` only on an `intercept`/`environment` origin *and*
///   a closed-schema kind (I-4);
/// * the known kinds carry their member minimum (`tool.proposed.kind_hint`,
///   `tool.completed.status`, `permission.decided.decision`,
///   `usage.reported.{used,size}`, `turn.finished.stop_reason` decodes when
///   present).
pub fn validate_event(e: &HostedEvent) -> Result<(), HostedError> {
    if e.kind.is_empty() {
        return Err(HostedError::EmptyKind);
    }
    if e.session.is_empty() {
        return Err(HostedError::EmptySession);
    }
    let kernel_channel = matches!(
        e.provenance.origin,
        HostedOrigin::Intercept | HostedOrigin::Environment
    );
    if e.mediation == Mediation::Mediated && !kernel_channel {
        return Err(HostedError::MediatedRequiresKernelChannel {
            kind: e.kind.clone(),
            origin: e.provenance.origin.as_str().to_string(),
        });
    }
    if e.provenance.authority > AuthorityClass::Delegate
        && (!kernel_channel || !is_known_kind(&e.kind))
    {
        return Err(HostedError::AuthorityCeiling {
            kind: e.kind.clone(),
            authority: e.provenance.authority.as_str().to_string(),
            origin: e.provenance.origin.as_str().to_string(),
        });
    }
    // Known-kind member minimums.
    if KNOWN_KINDS.contains(&e.kind.as_str()) {
        if !matches!(e.payload, Json::Obj(_)) {
            return Err(HostedError::PayloadShape {
                kind: e.kind.clone(),
                detail: "payload must be an object".into(),
            });
        }
        match e.kind.as_str() {
            "tool.proposed" => {
                if e.payload.get("kind_hint").and_then(Json::as_str).is_none() {
                    return Err(HostedError::PayloadShape {
                        kind: e.kind.clone(),
                        detail: "kind_hint must be a string".into(),
                    });
                }
            }
            "tool.completed" => {
                if e.payload.get("status").and_then(Json::as_str).is_none() {
                    return Err(HostedError::PayloadShape {
                        kind: e.kind.clone(),
                        detail: "status must be a string".into(),
                    });
                }
            }
            "permission.decided" => {
                if e.payload.get("decision").and_then(Json::as_str).is_none() {
                    return Err(HostedError::PayloadShape {
                        kind: e.kind.clone(),
                        detail: "decision must be a string".into(),
                    });
                }
            }
            "usage.reported" => {
                for k in ["used", "size"] {
                    if e.payload.get(k).and_then(Json::as_int).is_none() {
                        return Err(HostedError::PayloadShape {
                            kind: e.kind.clone(),
                            detail: format!("{k} must be an integer"),
                        });
                    }
                }
            }
            "turn.finished" => {
                if let Some(sr) = e.payload.get("stop_reason") {
                    if StopReason::from_json(sr).is_none() {
                        return Err(HostedError::PayloadShape {
                            kind: e.kind.clone(),
                            detail: "stop_reason must decode as a StopReason".into(),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// I-1/I-2 + the `model_io` gate over a session slice (`events` may carry
/// several sessions; each is checked independently). The caller runs
/// [`ensure_terminal`] first when synthesis is intended — a missing
/// `session.closed` is *not* an error here (it is synthesized); a missing
/// `session.opened` is `MissingSessionOpen`.
pub fn validate_session(
    events: &[HostedEvent],
    observability: &BTreeSet<Observability>,
) -> Result<(), HostedError> {
    // Per-session ordered slices (first-appearance order).
    let mut order: Vec<&str> = Vec::new();
    for e in events {
        if !order.contains(&e.session.as_str()) {
            order.push(e.session.as_str());
        }
    }
    for session in order {
        let slice: Vec<&HostedEvent> = events.iter().filter(|e| e.session == session).collect();
        for (i, e) in slice.iter().enumerate() {
            if e.seq != i as u64 {
                return Err(HostedError::SeqNotDense {
                    session: session.to_string(),
                    expected: i as u64,
                    found: e.seq,
                });
            }
            if e.kind.starts_with("model.call.") && !observability.contains(&Observability::ModelIo)
            {
                return Err(HostedError::ModelIoUndeclared {
                    kind: e.kind.clone(),
                    session: session.to_string(),
                });
            }
            validate_event(e)?;
        }
        if slice.first().map(|e| e.kind.as_str()) != Some("session.opened") {
            return Err(HostedError::MissingSessionOpen {
                session: session.to_string(),
            });
        }
    }
    Ok(())
}

/// The I-5 stamp for a hosted event — the members every hosted *ledger* row
/// carries (`{participant_class: "hosted", observability_level, mediation}`).
/// The service applies the stamp at append (S4.5a); the fixture asserts it.
pub fn envelope_stamps(e: &HostedEvent, observability: &BTreeSet<Observability>) -> Json {
    Json::obj([
        ("participant_class", Json::str("hosted")),
        (
            "observability_level",
            Json::Arr(
                observability
                    .iter()
                    .map(|o| Json::str(o.as_str()))
                    .collect(),
            ),
        ),
        ("mediation", Json::str(e.mediation.as_str())),
    ])
}

/// I-1's missing-terminal synthesis — appends `unobserved` terminal events
/// for open scopes and a missing `session.closed`, in opener order, per
/// session. Synthesized rows carry `origin: adapter`, `authority:
/// unverified`, `mediation: unobserved`, `event_channel: handle` and a
/// `synthesized: true` payload member (never mistaken for an observation).
///
/// Idempotent: a session that is already closed produces nothing.
pub fn ensure_terminal(events: &mut Vec<HostedEvent>) {
    let mut order: Vec<String> = Vec::new();
    for e in events.iter() {
        if !order.contains(&e.session) {
            order.push(e.session.clone());
        }
    }
    for session in order {
        let mut next_seq = events
            .iter()
            .filter(|e| e.session == session)
            .map(|e| e.seq)
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        let mut next_at = events
            .iter()
            .filter(|e| e.session == session)
            .map(|e| e.at)
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        let mut push = |events: &mut Vec<HostedEvent>, kind: &str, payload: Json| {
            events.push(HostedEvent {
                seq: next_seq,
                session: session.clone(),
                at: next_at,
                kind: kind.to_string(),
                payload,
                provenance: HostedProvenance {
                    origin: HostedOrigin::Adapter,
                    authority: AuthorityClass::Unverified,
                },
                mediation: Mediation::Unobserved,
                event_channel: EventChannel::Handle,
                raw_ref: None,
                ext: BTreeMap::new(),
            });
            next_seq += 1;
            next_at += 1;
        };
        // Open turns (turn.started without a matching turn.finished).
        let mut open_turns: Vec<String> = Vec::new();
        let mut open_tools: Vec<String> = Vec::new();
        let mut open_calls: Vec<String> = Vec::new();
        let mut closed = false;
        for e in events.iter().filter(|e| e.session == session) {
            match e.kind.as_str() {
                "session.closed" => closed = true,
                "turn.started" => {
                    if let Some(t) = e.payload.get("turn_id").and_then(Json::as_str) {
                        open_turns.push(t.to_string());
                    }
                }
                "turn.finished" => {
                    if let Some(t) = e.payload.get("turn_id").and_then(Json::as_str) {
                        open_turns.retain(|o| o != t);
                    }
                }
                "tool.proposed" => {
                    if let Some(t) = e.payload.get("tool_call_id").and_then(Json::as_str) {
                        open_tools.push(t.to_string());
                    }
                }
                "tool.completed" => {
                    if let Some(t) = e.payload.get("tool_call_id").and_then(Json::as_str) {
                        open_tools.retain(|o| o != t);
                    }
                }
                "model.call.started" => {
                    if let Some(t) = e.payload.get("model_call_id").and_then(Json::as_str) {
                        open_calls.push(t.to_string());
                    }
                }
                "model.call.completed" => {
                    if let Some(t) = e.payload.get("model_call_id").and_then(Json::as_str) {
                        open_calls.retain(|o| o != t);
                    }
                }
                _ => {}
            }
        }
        for t in open_turns {
            push(
                events,
                "turn.finished",
                Json::obj([
                    ("turn_id", Json::str(t)),
                    ("stop_reason_raw", Json::str("")),
                    (
                        "stop_reason",
                        participant_unclassified("unobserved").to_json(),
                    ),
                    ("synthesized", Json::Bool(true)),
                ]),
            );
        }
        for t in open_tools {
            push(
                events,
                "tool.completed",
                Json::obj([
                    ("tool_call_id", Json::str(t)),
                    ("status", Json::str("unobserved")),
                    ("synthesized", Json::Bool(true)),
                ]),
            );
        }
        for t in open_calls {
            push(
                events,
                "model.call.completed",
                Json::obj([
                    ("model_call_id", Json::str(t)),
                    ("status", Json::str("unobserved")),
                    ("synthesized", Json::Bool(true)),
                ]),
            );
        }
        if !closed {
            push(
                events,
                "session.closed",
                Json::obj([
                    ("reason", Json::str("unobserved")),
                    ("synthesized", Json::Bool(true)),
                ]),
            );
        }
    }
}
