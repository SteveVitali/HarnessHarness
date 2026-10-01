//! `proj_ABI` — the projection of a native ledger run into the hosted
//! presentation, and `lift`, the inverse direction (spec §6.6 §2.2; ADR-0164
//! D4–D6; R-2.10.6⁰).
//!
//! * [`project`] consumes `EventEnvelope`s (the adapter-zero path — the
//!   reference runtime presented through its own ABI) and emits
//!   `HostedEvent`s. The per-class disposition comes from `hh-ledger`'s
//!   `hosted_lowering` column — the class registry is the one class list
//!   (CC7); `"none"` rows never cross and are *declared* in the
//!   [`LoweringLossReport`], `"hint"` rows travel through `ext`/`raw_ref`
//!   with no authority (D5), `"passthrough"` rows carry the native spelling
//!   verbatim, and the typed rows lower to the closed kind vocabulary.
//! * [`lift`] is the analysis-side direction — hosted events back to
//!   native-class [`LiftedRow`]s (the WS-J6 §6.1 table's adapter-zero leg).
//!   A hosted row that has no native class lands as a
//!   `lifecycle.hosted.native_record` leaf (CC3 — nothing silently drops).
//!
//! The `StopReason` lift table (§6.6): `end_turn → completed`,
//! `max_tokens → budget_exhausted{tokens}`, `max_turn_requests →
//! budget_exhausted{turns}`, `refusal → refused`, `cancelled → cancelled`,
//! `other`/`_vendor` → `infrastructure_failure{participant_unclassified}`
//! with the raw value preserved on `stop_reason_raw`.

use std::collections::{BTreeMap, BTreeSet};

use hh_ledger::classes;
use hh_ledger::event::EventEnvelope;
use hh_ontology::control::StopReason;
use hh_ontology::dimensions::DimensionId;
use hh_provenance::AuthorityClass;
use hh_wire::Json;

use crate::events::{
    participant_unclassified, EventChannel, HostedEvent, HostedOrigin, HostedProvenance, Mediation,
};

/// A loss-report class (§6.6 — "loss class per kind"; ADR-0164 D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossClass {
    /// `no_slot` — the mechanism has no slot for the class (kernel
    /// bookkeeping that never crosses to the hosted presentation).
    NoSlot,
    /// `hint_only` — the class travels as a Lab-side hint (observable in
    /// `ext`/`raw_ref`, never authority).
    HintOnly,
    /// `narrowed` — the class has a typed slot but members were dropped or
    /// re-keyed (the entry's `detail` names them).
    Narrowed,
    /// `unrepresentable` — the class has no representation at all (reserved;
    /// a `Json` payload model leaves C0 with no current producer).
    Unrepresentable,
}

impl LossClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            LossClass::NoSlot => "no_slot",
            LossClass::HintOnly => "hint_only",
            LossClass::Narrowed => "narrowed",
            LossClass::Unrepresentable => "unrepresentable",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<LossClass> {
        Some(match s {
            "no_slot" => LossClass::NoSlot,
            "hint_only" => LossClass::HintOnly,
            "narrowed" => LossClass::Narrowed,
            "unrepresentable" => LossClass::Unrepresentable,
            _ => return None,
        })
    }
}

/// The loss severity (`{info, narrowed, lost}` — §6.6): `info` for a declared
/// by-design drop, `narrowed` for a member-level narrowing, `lost` when a
/// consumer-visible row left the artefact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossSeverity {
    /// Declared, by design.
    Info,
    /// Members narrowed.
    Narrowed,
    /// The row is gone from the hosted artefact.
    Lost,
}

impl LossSeverity {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            LossSeverity::Info => "info",
            LossSeverity::Narrowed => "narrowed",
            LossSeverity::Lost => "lost",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<LossSeverity> {
        Some(match s {
            "info" => LossSeverity::Info,
            "narrowed" => LossSeverity::Narrowed,
            "lost" => LossSeverity::Lost,
            _ => return None,
        })
    }
}

/// One loss-report entry — `class → {loss_class, severity, detail}`.
#[derive(Debug, Clone, PartialEq)]
pub struct LossEntry {
    /// The native class (or hosted kind) the entry names.
    pub class: String,
    /// The loss class.
    pub loss_class: LossClass,
    /// The severity.
    pub severity: LossSeverity,
    /// A short detail (e.g. the dropped member names for `narrowed`).
    pub detail: String,
}

/// The `LoweringLossReport` — per mechanism, per class, *declared* loss
/// (never silent — CC3/T-LCD-11).
#[derive(Debug, Clone, PartialEq)]
pub struct LoweringLossReport {
    /// The mechanism/adapter the report is bound to (`hh-hosting/1:adapter_zero`).
    pub target: String,
    /// The per-class entries (sorted by class — deterministic).
    pub entries: Vec<LossEntry>,
    /// `ext` — preserved, never deciding (CC3).
    pub ext: BTreeMap<String, Json>,
}

impl LoweringLossReport {
    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("target", Json::str(&self.target)),
            (
                "entries",
                Json::Arr(
                    self.entries
                        .iter()
                        .map(|e| {
                            Json::obj([
                                ("class", Json::str(&e.class)),
                                ("loss_class", Json::str(e.loss_class.as_str())),
                                ("severity", Json::str(e.severity.as_str())),
                                ("detail", Json::str(&e.detail)),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "ext",
                Json::Obj(
                    self.ext
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                ),
            ),
        ])
    }
}

/// The projection output — the hosted events plus the declared loss report.
#[derive(Debug, Clone, PartialEq)]
pub struct Projection {
    /// The hosted session id (the run id for adapter zero — one session per
    /// run).
    pub session: String,
    /// The hosted events in dense `seq` order.
    pub events: Vec<HostedEvent>,
    /// The declared lowering loss.
    pub loss: LoweringLossReport,
}

/// A lifted row — the native-class event a hosted event lifts to (the
/// analysis-side projection; wrapped into `EventEnvelope`s by the consumer
/// when parity folds are wanted).
#[derive(Debug, Clone, PartialEq)]
pub struct LiftedRow {
    /// The native class the row lifts to (`lifecycle.hosted.native_record`
    /// when no native class exists — content preserved, never dropped).
    pub class: String,
    /// The lifted payload (native payload shape).
    pub payload: Json,
    /// The hosted `seq` (order preserved).
    pub seq: u64,
    /// The adapter-monotonic `at`.
    pub at: u64,
    /// The mediation stamp the hosted event carried.
    pub mediation: Mediation,
    /// The channel origin.
    pub origin: HostedOrigin,
    /// The authority (bounded by I-4 at ingest).
    pub authority: AuthorityClass,
    /// The turn scope, when the row carries one.
    pub turn_id: Option<String>,
}

// ── The native → hosted direction ────────────────────────────────────────────

/// The payload subset a typed lowering carries — `keep` names the members
/// copied verbatim; `extra` adds computed members.
fn typed_payload(e: &EventEnvelope, keep: &[&str], extra: Vec<(&str, Json)>) -> Json {
    let mut m = BTreeMap::new();
    if let Json::Obj(p) = &e.payload {
        for k in keep {
            if let Some(v) = p.get(*k) {
                m.insert((*k).to_string(), v.clone());
            }
        }
    }
    for (k, v) in extra {
        m.insert(k.to_string(), v);
    }
    Json::Obj(m)
}

/// The dropped member names of a typed lowering (the `narrowed` loss detail).
fn dropped_members(e: &EventEnvelope, keep: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    if let Json::Obj(p) = &e.payload {
        for k in p.keys() {
            if !keep.contains(&k.as_str()) {
                out.push(k.clone());
            }
        }
    }
    out
}

/// Project a native run's envelopes into the hosted presentation (adapter
/// zero — `session = run_id`; `at` = emission index, the adapter's monotone
/// counter; `raw_ref` = the native `event_id`). `observability` is the hosted
/// session's declared set: `model.call.*`/`message.delta` rows are emitted
/// only when `model_io` is declared (model I/O is observable only through
/// interception — §6.6 §2.2); gated classes land in the loss report as
/// `no_slot`/`info` with detail `observability-gated`. The `hosted_lowering`
/// column drives every other disposition; every `"none"`/`"hint"`/`narrowed`
/// class lands a declared [`LossEntry`].
pub fn project(
    events: &[EventEnvelope],
    observability: &BTreeSet<hh_ontology::participant::Observability>,
) -> Projection {
    let session = events.first().map(|e| e.run_id.clone()).unwrap_or_default();
    let mut out = Vec::new();
    let mut loss: BTreeMap<String, (LossClass, LossSeverity, BTreeSet<String>)> = BTreeMap::new();
    let model_io = observability.contains(&hh_ontology::participant::Observability::ModelIo);
    for e in events {
        let lowering = classes::hosted_lowering(&e.class);
        // The model-I/O surface is observable only through interception
        // (§6.6 §2.2): a session that did not declare `model_io` never sees
        // `model.call.*`/`model.stream.*` rows — *including* the `hint` rows,
        // whose `ext.source` embed would otherwise carry model I/O to a
        // session that cannot observe it. Declared as gated loss, never
        // silently emitted or dropped.
        if !model_io && (e.class.starts_with("model.call.") || e.class.starts_with("model.stream."))
        {
            loss.entry(e.class.clone()).or_insert((
                LossClass::NoSlot,
                LossSeverity::Info,
                ["observability-gated: no model_io".to_string()]
                    .into_iter()
                    .collect(),
            ));
            continue;
        }
        let raw_ref = Some(e.event_id.clone());
        let base = |kind: String,
                    payload: Json,
                    origin: HostedOrigin,
                    authority: AuthorityClass,
                    mediation: Mediation,
                    channel: EventChannel,
                    ext: BTreeMap<String, Json>| HostedEvent {
            seq: 0, // assigned dense below
            session: session.clone(),
            at: 0, // assigned dense below
            kind,
            payload,
            provenance: HostedProvenance { origin, authority },
            mediation,
            event_channel: channel,
            raw_ref,
            ext,
        };
        match lowering {
            "none" => {
                loss.entry(e.class.clone()).or_insert((
                    LossClass::NoSlot,
                    LossSeverity::Info,
                    BTreeSet::new(),
                ));
            }
            "hint" => {
                loss.entry(e.class.clone()).or_insert((
                    LossClass::HintOnly,
                    LossSeverity::Narrowed,
                    BTreeSet::new(),
                ));
                let mut ext = BTreeMap::new();
                ext.insert(
                    "hh.hosting/1".to_string(),
                    Json::obj([(
                        "source",
                        Json::obj([
                            ("class", Json::str(&e.class)),
                            ("seq", Json::Int(e.seq as i64)),
                            ("payload", e.payload.clone()),
                        ]),
                    )]),
                );
                out.push(base(
                    e.class.clone(),
                    Json::obj([]),
                    HostedOrigin::Environment,
                    AuthorityClass::Environment,
                    Mediation::Observed,
                    EventChannel::Handle,
                    ext,
                ));
            }
            "passthrough" => {
                out.push(base(
                    e.class.clone(),
                    e.payload.clone(),
                    HostedOrigin::Environment,
                    AuthorityClass::Kernel,
                    Mediation::Mediated,
                    EventChannel::Handle,
                    BTreeMap::new(),
                ));
            }
            kind => {
                // A typed hh-hosting/1 kind — map the payload per class.
                let (payload, origin, authority, mediation, channel, keep) = typed_row(e, kind);
                let dropped = dropped_members(e, keep);
                if !dropped.is_empty() {
                    loss.entry(e.class.clone())
                        .or_insert((LossClass::Narrowed, LossSeverity::Narrowed, BTreeSet::new()))
                        .2
                        .extend(dropped);
                }
                out.push(base(
                    kind.to_string(),
                    payload,
                    origin,
                    authority,
                    mediation,
                    channel,
                    BTreeMap::new(),
                ));
            }
        }
    }
    // Dense per-session seq + monotone `at` (I-2; `at` is the adapter's own
    // counter — never a wall clock).
    for (i, e) in out.iter_mut().enumerate() {
        e.seq = i as u64;
        e.at = i as u64;
    }
    let mut entries: Vec<LossEntry> = loss
        .into_iter()
        .map(|(class, (loss_class, severity, dropped))| LossEntry {
            class,
            loss_class,
            severity,
            detail: if dropped.is_empty() {
                String::new()
            } else if loss_class == LossClass::Narrowed {
                format!(
                    "dropped members: {}",
                    dropped.into_iter().collect::<Vec<_>>().join(",")
                )
            } else {
                dropped.into_iter().collect::<Vec<_>>().join(",")
            },
        })
        .collect();
    entries.sort_by(|a, b| a.class.cmp(&b.class));
    Projection {
        session,
        events: out,
        loss: LoweringLossReport {
            target: "hh-hosting/1:adapter_zero".to_string(),
            entries,
            ext: BTreeMap::new(),
        },
    }
}

/// The typed-row lowering — returns `(payload, origin, authority, mediation,
/// channel, kept_members)`.
fn typed_row<'a>(
    e: &'a EventEnvelope,
    kind: &'a str,
) -> (
    Json,
    HostedOrigin,
    AuthorityClass,
    Mediation,
    EventChannel,
    &'a [&'a str],
) {
    use AuthorityClass as A;
    use EventChannel as C;
    use HostedOrigin as O;
    use Mediation as M;
    // The Lab-side surface: mediated, environment origin, kernel/environment
    // authority. The participant's own rows: observed, delegate.
    let lab = (O::Environment, A::Environment, M::Mediated, C::Protocol);
    let kernel = (O::Environment, A::Kernel, M::Mediated, C::Protocol);
    let participant = (O::Participant, A::Delegate, M::Observed, C::Protocol);
    let intercepted = (O::Intercept, A::Environment, M::Mediated, C::Proxy);
    let (origin, authority, mediation, channel);
    let payload: Json;
    let keep: &[&str];
    match (e.class.as_str(), kind) {
        ("lifecycle.run.created", _) => {
            (origin, authority, mediation, channel) = lab;
            keep = &[];
            payload = typed_payload(e, keep, vec![]);
        }
        ("lifecycle.run.finished", _) => {
            (origin, authority, mediation, channel) = lab;
            keep = &["wall_ms", "stop_reason"];
            payload = typed_payload(e, keep, vec![]);
        }
        ("lifecycle.turn.started", _) => {
            (origin, authority, mediation, channel) = participant;
            keep = &["turn_id"];
            payload = typed_payload(e, keep, vec![]);
        }
        ("lifecycle.turn.finished", _) => {
            (origin, authority, mediation, channel) = participant;
            keep = &["turn_id", "stop_reason"];
            // The raw spelling = the lifted value's `kind` member for a
            // native source (one vocabulary — `stop_reason_raw` is verbatim).
            let raw = e
                .payload
                .get("stop_reason")
                .and_then(|s| s.get("kind"))
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            payload = typed_payload(e, keep, vec![("stop_reason_raw", Json::str(raw))]);
        }
        ("model.call.requested", _) => {
            (origin, authority, mediation, channel) = intercepted;
            keep = &["model_call_id"];
            payload = typed_payload(e, keep, vec![]);
        }
        ("model.call.completed", _) => {
            (origin, authority, mediation, channel) = intercepted;
            keep = &["model_call_id", "usage"];
            payload = typed_payload(e, keep, vec![("status", Json::str("completed"))]);
        }
        ("model.call.failed", _) => {
            (origin, authority, mediation, channel) = intercepted;
            keep = &["model_call_id", "error_class"];
            payload = typed_payload(e, keep, vec![("status", Json::str("failed"))]);
        }
        ("model.stream.delta", _) => {
            (origin, authority, mediation, channel) = intercepted;
            keep = &["text"];
            payload = typed_payload(e, keep, vec![]);
        }
        ("action.tool.proposed", _) => {
            (origin, authority, mediation, channel) = participant;
            keep = &["tool_call_id", "tool_class"];
            let hint = e
                .payload
                .get("tool_class")
                .and_then(Json::as_str)
                .unwrap_or("tool")
                .to_string();
            payload = typed_payload(e, keep, vec![("kind_hint", Json::str(hint))]);
        }
        ("action.tool.completed", _) => {
            (origin, authority, mediation, channel) = lab;
            keep = &["tool_call_id", "tool_class", "content", "locations"];
            payload = typed_payload(e, keep, vec![("status", Json::str("completed"))]);
        }
        ("action.tool.rejected", _) => {
            (origin, authority, mediation, channel) = lab;
            keep = &["tool_call_id", "tool_class", "reason"];
            payload = typed_payload(e, keep, vec![("status", Json::str("rejected"))]);
        }
        ("action.tool.surface_rejected", _) => {
            (origin, authority, mediation, channel) = lab;
            keep = &["tool_call_id", "tool_class", "reason"];
            payload = typed_payload(e, keep, vec![("status", Json::str("surface_rejected"))]);
        }
        ("action.tool.call.refused", _) => {
            (origin, authority, mediation, channel) = lab;
            keep = &["tool_call_id", "tool_class", "reason"];
            payload = typed_payload(e, keep, vec![("status", Json::str("refused"))]);
        }
        ("security.permission.requested" | "security.permission.pending", _) => {
            (origin, authority, mediation, channel) = kernel;
            keep = &["permission_id", "tool_call_id", "capability", "action"];
            payload = typed_payload(e, keep, vec![]);
        }
        ("security.permission.decided", _) => {
            (origin, authority, mediation, channel) = kernel;
            keep = &[
                "permission_id",
                "tool_call_id",
                "decision",
                "decider",
                "approval_wait_ms",
            ];
            payload = typed_payload(e, keep, vec![]);
        }
        ("context.artefact.delivered", _) => {
            (origin, authority, mediation, channel) = lab;
            keep = &["artefact_id", "delivery_id", "kind", "model_call_id"];
            payload = typed_payload(e, keep, vec![]);
        }
        ("context.compaction.started", _) => {
            (origin, authority, mediation, channel) = kernel;
            keep = &["tokens_before", "tokens_after"];
            payload = typed_payload(e, keep, vec![("phase", Json::str("started"))]);
        }
        ("context.compaction.completed", _) => {
            (origin, authority, mediation, channel) = kernel;
            keep = &["tokens_before", "tokens_after"];
            payload = typed_payload(e, keep, vec![("phase", Json::str("completed"))]);
        }
        _ => {
            // A lowering spelling this projector does not produce — refuse
            // silently? Never: keep the row as an unknown-kind passthrough
            // (preserved, delegate authority, observed) and let validation
            // decide. Reachable only if the table names a kind outside
            // KNOWN_KINDS — the table is checked in tests.
            (origin, authority, mediation, channel) =
                (O::Adapter, A::Unverified, M::Unobserved, C::Handle);
            keep = &[];
            payload = typed_payload(e, keep, vec![]);
        }
    }
    (payload, origin, authority, mediation, channel, keep)
}

// ── The hosted → native direction (`lift`) ───────────────────────────────────

/// The §6.6 `StopReason` lift table — a hosted terminal spelling to the
/// closed native `StopReason`. `other`/`_`-prefixed spellings lift to
/// `infrastructure_failure{participant_unclassified}` with `raw` preserved
/// (the caller puts it on `stop_reason_raw`); an *unparseable* spelling does
/// the same (never a panic, never a guess).
pub fn lift_stop_reason(raw: &str) -> StopReason {
    match raw {
        "end_turn" => StopReason::Completed,
        "max_tokens" => StopReason::BudgetExhausted {
            // §6.6 `budget_exhausted{tokens}` — the token dimensions are
            // bucketed in `DimensionId`; a participant-side `max_tokens` is
            // the *output* cap.
            budget_id: "tokens".to_string(),
            dimension: DimensionId::TokensOutputVisible,
        },
        "max_turn_requests" => StopReason::BudgetExhausted {
            budget_id: "turns".to_string(),
            dimension: DimensionId::Turns,
        },
        "refusal" => StopReason::Refused {
            blocking_effect_id: String::new(),
        },
        "cancelled" => StopReason::Cancelled {
            by: hh_ontology::control::CancelledBy::Hosting,
        },
        // `other` / `_vendor` / anything unparseable — the participant's own
        // word is preserved in `error_class.class` (and `stop_reason_raw` on
        // the event).
        other => participant_unclassified(if other.is_empty() {
            "unobserved"
        } else {
            other
        }),
    }
}

/// The `ext["hh.hosting/1"].source` embed a hint row carries.
fn hint_source(e: &HostedEvent) -> Option<&Json> {
    e.ext.get("hh.hosting/1").and_then(|x| x.get("source"))
}

/// Lift one hosted event to a native-class row. The mapping is total —
/// unknown/`_`-prefixed kinds land as `lifecycle.hosted.native_record` leaves
/// carrying `{kind, payload}` (CC3: nothing silently drops).
pub fn lift_event(e: &HostedEvent) -> LiftedRow {
    let row = |class: &str, payload: Json, turn_id: Option<String>| LiftedRow {
        class: class.to_string(),
        payload,
        seq: e.seq,
        at: e.at,
        mediation: e.mediation.clone(),
        origin: e.provenance.origin.clone(),
        authority: e.provenance.authority,
        turn_id,
    };
    let leaf = |e: &HostedEvent| {
        row(
            "lifecycle.hosted.native_record",
            Json::obj([("kind", Json::str(&e.kind)), ("payload", e.payload.clone())]),
            None,
        )
    };
    let tid = || {
        e.payload
            .get("turn_id")
            .and_then(Json::as_str)
            .map(String::from)
    };
    // A hint row restores the embedded source verbatim (its authority was
    // already `observed` — the restored payload is *data*, never minted
    // authority).
    if let Some(src) = hint_source(e) {
        let class = src
            .get("class")
            .and_then(Json::as_str)
            .unwrap_or("lifecycle.hosted.native_record");
        let payload = src.get("payload").cloned().unwrap_or(Json::obj([]));
        return row(class, payload, tid());
    }
    // A registered native class spelling is a passthrough — verbatim.
    if classes::lookup(&e.kind).is_some() && !KNOWN_LIFTED.contains(&e.kind.as_str()) {
        return row(&e.kind, e.payload.clone(), tid());
    }
    match e.kind.as_str() {
        "session.opened" => row("lifecycle.run.created", e.payload.clone(), None),
        "session.closed" => row("lifecycle.run.finished", e.payload.clone(), None),
        "turn.started" => row("lifecycle.turn.started", e.payload.clone(), tid()),
        "turn.finished" => {
            // `stop_reason` is the lifted value when present; otherwise lift
            // `stop_reason_raw` through the table now.
            let mut p = e.payload.clone();
            if p.get("stop_reason").is_none() {
                let raw = p
                    .get("stop_reason_raw")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                if let Json::Obj(m) = &mut p {
                    m.insert("stop_reason".into(), lift_stop_reason(&raw).to_json());
                }
            }
            row("lifecycle.turn.finished", p, tid())
        }
        "model.call.started" => row("model.call.requested", e.payload.clone(), tid()),
        "model.call.completed" => {
            let status = e
                .payload
                .get("status")
                .and_then(Json::as_str)
                .unwrap_or("completed");
            if status == "failed" {
                row("model.call.failed", e.payload.clone(), tid())
            } else {
                row("model.call.completed", e.payload.clone(), tid())
            }
        }
        "message.delta" => row("model.stream.delta", e.payload.clone(), tid()),
        "tool.proposed" => row("action.tool.proposed", e.payload.clone(), tid()),
        "tool.completed" => {
            let status = e
                .payload
                .get("status")
                .and_then(Json::as_str)
                .unwrap_or("completed");
            let class = match status {
                "rejected" => "action.tool.rejected",
                "surface_rejected" => "action.tool.surface_rejected",
                "refused" => "action.tool.call.refused",
                _ => "action.tool.completed",
            };
            row(class, e.payload.clone(), tid())
        }
        "permission.requested" => row("security.permission.pending", e.payload.clone(), tid()),
        "permission.decided" => row("security.permission.decided", e.payload.clone(), tid()),
        "usage.reported" => row(
            "measurement.cost.attributed",
            Json::obj([
                ("provenance", Json::str("participant_reported")),
                ("confidence", Json::str("estimate")),
                ("usage", e.payload.clone()),
            ]),
            tid(),
        ),
        "artefact.delivered" => row("context.artefact.delivered", e.payload.clone(), tid()),
        "compaction.observed" => {
            let phase = e
                .payload
                .get("phase")
                .and_then(Json::as_str)
                .unwrap_or("completed");
            let class = if phase == "started" {
                "context.compaction.started"
            } else {
                "context.compaction.completed"
            };
            row(class, e.payload.clone(), tid())
        }
        "coordinate.changed" => row("lifecycle.hosted.coordinate_set", e.payload.clone(), tid()),
        // Known kinds with no native class, unknown kinds, `_`-prefixed kinds —
        // the native-record leaf preserves them (CC3).
        _ => leaf(e),
    }
}

/// The hosted kinds that have a *typed* lift (so a same-named native class —
/// none today — would never shadow the table above).
const KNOWN_LIFTED: &[&str] = &[
    "session.opened",
    "session.closed",
    "turn.started",
    "turn.finished",
    "model.call.started",
    "model.call.completed",
    "message.delta",
    "tool.proposed",
    "tool.completed",
    "permission.requested",
    "permission.decided",
    "usage.reported",
    "artefact.delivered",
    "compaction.observed",
    "coordinate.changed",
];

/// Lift a hosted session slice — one row per event, order preserved.
pub fn lift(events: &[HostedEvent]) -> Vec<LiftedRow> {
    events.iter().map(lift_event).collect()
}
