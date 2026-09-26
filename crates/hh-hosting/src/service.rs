//! The Hosting ABI service — the Lab-side implementation of the §6.6 verbs
//! (R-2.10.6; S4.5a; ADR-0164).
//!
//! The service is **thin/observational** (non-goal N2): it drives the adapter
//! through the baseline/capability-declared verbs, stamps the envelope
//! members it owns (`seq`, `session`, `at` — dense and adapter-monotone),
//! runs the I-1…I-5 validators at ingest, gates every capability-declared
//! verb on the reconciled `capability_vector` the Lab supplied at attach
//! (never the participant's claim alone), and enforces the boundary rule —
//! policy, authority, credential *values* and budgets never enter the
//! participant loop; `budget_view` rides a `ContextItem` only when the
//! `instruction_delivery` dimension is `supported`.
//!
//! `handle(op, params)` is the records-in/records-out surface — the
//! boundary's hosting plane wires a closure to it (`hh-embed` never depends
//! on this crate — the seam is Json).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_ontology::participant::{Observability, ParticipantClass};
use hh_wire::Json;

use crate::abi::{
    is_admitted_verb, refuse_handle_keys, EndState, HostedRunSpec, HostingError, Negotiated,
    Opened, ResumeCursor, ABI_VERSION, EXCLUDED_VERBS,
};
use crate::adapter_a::{AdapterA, LiftedObservation};
use crate::budget::{check_ceiling, derive_budget_enforcement, CeilingCheck};
use crate::events::{
    ensure_terminal, validate_event, validate_session, HostedEvent, HostedProvenance,
};
use crate::records::{AdapterRecord, EnforcementClaim, ParticipantRecord, ProcessPlacement};

/// The per-session runtime state.
#[derive(Debug, Clone)]
pub struct HostedSession {
    /// The Lab-minted session ref.
    pub session_ref: String,
    /// The spec the session opened under (with `budget_view` delivered only
    /// per the instruction-delivery gate — the stored copy is what the
    /// participant actually saw).
    pub spec: HostedRunSpec,
    /// `open`/`closed` — a closed session takes no further verbs.
    pub closed: bool,
    /// The session's dense, validated events (the Lab's own observation log —
    /// the kernel snapshot at close, never the participant's self-report).
    pub events: Vec<HostedEvent>,
    /// Running turn ids (submitted, not finished).
    pub open_turns: BTreeSet<String>,
    /// The session's usage accumulation (dim → consumed) — participant-reported
    /// *and* Lab-metered, for ceiling checks.
    pub usage: BTreeMap<String, i64>,
    /// Runtime contradictions — the drift annotations `attach` forwards as
    /// `observed_in: run` conformance entries (§6.6 §9.4).
    pub drift_annotations: Vec<Json>,
    /// The end-state snapshot at close.
    pub end_state: Option<EndState>,
    /// `true` when this session continues a prior one (resume).
    pub resumed_from: Option<String>,
}

/// The verb surface + session registry. `A` is the adapter type — concrete
/// `AdapterA` at C2; the trait object seam is the constructor argument.
pub struct HostingService {
    adapter: AdapterA,
    participant: ParticipantRecord,
    negotiated: Negotiated,
    /// The reconciled capability vector (`dimension → value`) — supplied by
    /// the Lab at attach; `declarations_unknown` rewrites every member to
    /// `"unknown"` on describe.
    capability_vector: BTreeMap<String, Json>,
    observability: BTreeSet<Observability>,
    budget_enforcement: BTreeMap<String, EnforcementClaim>,
    /// Hard caps the Lab enforces (`dimension → {budget_id, cap}`) — supplied
    /// by the boundary, checked at every §6.6 decision point.
    hard_caps: BTreeMap<String, (String, i64)>,
    /// Lab-owned wall clock (adapter-monotone ms — never the OS clock).
    wall_ms: i64,
    sessions: BTreeMap<String, HostedSession>,
    /// Events lifted but not yet bound to a session (open/resume drains
    /// them in — a session's `session.opened` arrives before its state
    /// exists).
    pending_events: Vec<HostedEvent>,
    next_session: u64,
    next_turn: u64,
    clock: u64,
}

impl HostingService {
    /// `attach` — the handshake + admission gates (§6.6 §2.1):
    ///
    /// * `adapter.describe()` → the participant's `abi_versions[]` →
    ///   [`crate::abi::negotiate`] (empty → `hh-hosting/1` + unknowns;
    ///   unknown major → `AbiVersionUnsupported`).
    /// * The adapter must *claim* the participant's mechanism × placement
    ///   (I-6; `AdapterRecord::claims`), carry a **complete** debt record
    ///   (AC-R-2.10.6-8 — schema-complete is not claim-complete), and never
    ///   be auto-approving (AC-R-2.10.6-4 — the Lab owns the decider).
    /// * `capability_vector` is the reconciled vector the Lab computed from
    ///   the registry (declaration + conformance records) — the service
    ///   gates on it verbatim.
    /// * `environment_lab_provisioned` feeds the enforcement derivation
    ///   (`network.*`/`env.*` are enforceable only on Lab ground).
    pub fn attach(
        participant: ParticipantRecord,
        adapter: AdapterA,
        capability_vector: BTreeMap<String, Json>,
        environment_lab_provisioned: bool,
        hard_caps: BTreeMap<String, (String, i64)>,
    ) -> Result<HostingService, HostingError> {
        // Participant admissibility — the record's own gates already ran at
        // construction/decode (`class` hosted, mechanism ≠ none); the service
        // refuses the degenerate remainder.
        let d = &participant.descriptor;
        if d.class != ParticipantClass::Hosted {
            return Err(HostingError::ParticipantInadmissible {
                detail: "descriptor.class is not hosted".into(),
            });
        }
        let mechanism = d.hosting_mechanism;
        // I-6: the adapter must claim the mechanism × placement pair.
        let placement = participant
            .hosting_ext
            .process_placement
            .unwrap_or(ProcessPlacement::InEnvironment);
        let record: &AdapterRecord = adapter.record();
        if !record.claims(mechanism, placement) {
            return Err(HostingError::AdapterRefused {
                detail: format!(
                    "adapter {} does not claim {}/{}",
                    record.adapter_id,
                    mechanism.as_str(),
                    placement.as_str()
                ),
            });
        }
        // AC-8: the debt record must be *claim-complete* — a hypothesis
        // (hash-addressed is fine — `content_hash` is always present on a
        // well-formed Text), ≥1 evidence ref, and a removal test.
        let debt = &record.debt;
        let debt_ok = !debt.hypothesis.content_hash.is_empty()
            && !debt.evidence_refs.is_empty()
            && !debt.removal_test_ref.is_empty();
        if !debt_ok {
            return Err(HostingError::AdapterRefused {
                detail: format!(
                    "adapter {} carries an incomplete AssumptionDebtRecord",
                    record.adapter_id
                ),
            });
        }
        if adapter.auto_approve {
            return Err(HostingError::AdapterRefused {
                detail: "the adapter auto-approves permission requests — the \
                         Lab owns the decider"
                    .into(),
            });
        }
        // The handshake — the describe's advertised versions negotiate.
        let mut adapter = adapter;
        let describe = adapter.describe()?;
        let advertised: Vec<String> = describe
            .get("abi_versions")
            .and_then(|v| match v {
                Json::Arr(items) => Some(
                    items
                        .iter()
                        .filter_map(|i| i.as_str().map(String::from))
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        let negotiated = crate::abi::negotiate(&advertised)?;
        // The enforcement map — derived, never claimed.
        let usage_reporting = capability_vector
            .get("usage_reporting")
            .and_then(Json::as_str)
            == Some("supported");
        let budget_enforcement = derive_budget_enforcement(
            mechanism,
            placement,
            adapter.intercept,
            environment_lab_provisioned,
            usage_reporting,
        );
        Ok(HostingService {
            adapter,
            observability: participant.descriptor.observability_level.clone(),
            participant,
            negotiated,
            capability_vector,
            budget_enforcement,
            hard_caps,
            wall_ms: 0,
            sessions: BTreeMap::new(),
            pending_events: Vec::new(),
            next_session: 0,
            next_turn: 0,
            clock: 0,
        })
    }

    /// The negotiated attach.
    pub fn negotiated(&self) -> &Negotiated {
        &self.negotiated
    }

    /// The participant record.
    pub fn participant(&self) -> &ParticipantRecord {
        &self.participant
    }

    /// The derived `budget_enforcement` map (§6.6 §8).
    pub fn budget_enforcement(&self) -> &BTreeMap<String, EnforcementClaim> {
        &self.budget_enforcement
    }

    /// The capability vector the service gates on.
    pub fn capability_vector(&self) -> &BTreeMap<String, Json> {
        &self.capability_vector
    }

    /// `describe` — the Lab-side describe: the participant's advertised ABI
    /// surface + the reconciled vector + the adapter's claims (§6.6 §2.1 —
    /// everything the Lab knows, honestly stamped).
    pub fn describe(&self) -> Json {
        let mut vector = self.capability_vector.clone();
        if self.negotiated.declarations_unknown {
            for v in vector.values_mut() {
                *v = Json::str("unknown");
            }
        }
        Json::obj([
            (
                "abi_version",
                Json::str(self.negotiated.version.as_string()),
            ),
            ("abi_versions", Json::Arr(vec![Json::str(ABI_VERSION)])),
            (
                "participant_version_identity",
                Json::str(&self.participant.version_identity),
            ),
            (
                "hosting_mechanism",
                Json::str(self.participant.descriptor.hosting_mechanism.as_str()),
            ),
            (
                "observability_level",
                Json::Arr(
                    self.observability
                        .iter()
                        .map(|o| Json::str(o.as_str()))
                        .collect(),
                ),
            ),
            (
                "declarations_unknown",
                Json::Bool(self.negotiated.declarations_unknown),
            ),
            (
                "capability_vector",
                Json::Obj(vector.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
            ),
            (
                "budget_enforcement",
                crate::budget::enforcement_json(&self.budget_enforcement),
            ),
        ])
    }

    fn gate(&self, verb: &str, dimension: &str) -> Result<(), HostingError> {
        let supported =
            self.capability_vector.get(dimension).and_then(Json::as_str) == Some("supported");
        if supported {
            Ok(())
        } else {
            Err(HostingError::CapabilityNotSupported {
                verb: verb.to_string(),
                dimension: dimension.to_string(),
            })
        }
    }

    /// `open(spec)` — the boundary checks run *here* (the adapter only ever
    /// sees the sealed record): no handle keys anywhere in `connection_info`;
    /// `credential_channels` ⊆ the declared `credential_supply`;
    /// `budget_view` delivered only when `instruction_delivery` is supported
    /// (CF-073 — the view rides `context_items`, never a verb).
    pub fn open(&mut self, spec: HostedRunSpec) -> Result<Opened, HostingError> {
        refuse_handle_keys(&spec.connection_info, "connection_info")?;
        let declared_channels: BTreeSet<String> = self
            .participant
            .hosting_ext
            .credential_supply
            .clone()
            .unwrap_or_default()
            .into_iter()
            .collect();
        for c in &spec.credential_channels {
            if !declared_channels.contains(c) {
                return Err(HostingError::CredentialChannelUndeclared { channel: c.clone() });
            }
        }
        // CF-073: the advisory budget view is a ContextItem delivered only
        // when instruction_delivery is supported — otherwise dropped (never
        // a silent verb).
        let mut spec = spec;
        let instruction_delivery = self
            .capability_vector
            .get("instruction_delivery")
            .and_then(Json::as_str)
            == Some("supported");
        if let Some(view) = spec.budget_view.take() {
            if instruction_delivery {
                spec.context_items.push(Json::obj([
                    ("kind", Json::str("budget_view")),
                    ("value", view),
                ]));
            }
        }
        self.next_session += 1;
        let session_ref = format!("hs-{}", self.next_session);
        // The spec the participant sees — context items and connection info
        // only; `params`/`definition_ref` ride it too. No authority, no
        // policy, no credential values, no budget object.
        let spec_json = Json::obj([
            ("definition_ref", Json::str(&spec.definition_ref)),
            ("params", spec.params.clone()),
            ("placement", Json::str(spec.placement.as_str())),
            ("connection_info", spec.connection_info.clone()),
            ("context_items", Json::Arr(spec.context_items.clone())),
            (
                "credential_channels",
                Json::Arr(spec.credential_channels.iter().map(Json::str).collect()),
            ),
        ]);
        self.adapter.open(&session_ref, &spec_json)?;
        self.drain(session_ref.as_str())?;
        let resumed_from = spec.resume_or_none();
        self.sessions.insert(
            session_ref.clone(),
            HostedSession {
                session_ref: session_ref.clone(),
                spec,
                closed: false,
                events: std::mem::take(&mut self.pending_events),
                open_turns: BTreeSet::new(),
                usage: BTreeMap::new(),
                drift_annotations: Vec::new(),
                end_state: None,
                resumed_from,
            },
        );
        Ok(Opened {
            session_ref,
            capability_vector: self.capability_vector.clone(),
            abi_version: self.negotiated.version.as_string(),
        })
    }

    /// `submit(session, prompt)` — mint a turn, run it, ingest. Returns the
    /// turn id (the turn may still be running — `stream_events` shows the
    /// lifecycle).
    pub fn submit(&mut self, session_ref: &str, prompt: &Json) -> Result<String, HostingError> {
        {
            let s = self.session_mut(session_ref)?;
            if s.closed {
                return Err(HostingError::SessionState {
                    session: session_ref.to_string(),
                    detail: "closed".into(),
                });
            }
        }
        // Decision point: before `submit` — the turn ceiling.
        self.next_turn += 1;
        let turn_id = format!("{}-t{}", session_ref, self.next_turn);
        let turns_used = self
            .sessions
            .get(session_ref)
            .and_then(|s| s.usage.get("turns").copied())
            .unwrap_or(0)
            + 1;
        if let Err(e) = self.ceiling(session_ref, "turns", turns_used) {
            self.exhaust(session_ref, "turns")?;
            return Err(e);
        }
        let res = self.adapter.submit(session_ref, &turn_id, prompt)?;
        {
            let s = self.session_mut(session_ref)?;
            *s.usage.entry("turns".into()).or_insert(0) += 1;
            s.open_turns.insert(turn_id.clone());
        }
        self.drain(session_ref)?;
        if res.get("status").and_then(Json::as_str) == Some("finished") {
            self.session_mut(session_ref)?.open_turns.remove(&turn_id);
        } else {
            // A running turn: pump until it resolves (upcall round-trips
            // continue it) or the pump budget exhausts — `cancel`/`close`
            // handle the rest.
            self.pump_until(session_ref, &turn_id)?;
        }
        self.accumulate_usage(session_ref);
        Ok(turn_id)
    }

    /// `cancel(session)` — `session/cancel` + the interruption evidence:
    /// a cancelled `turn.finished` is the support observation; a turn still
    /// open after the pump budget is *drift evidence* the session records
    /// (the participant ignored the cancel — §6.6 §9.4; never silent).
    pub fn cancel(&mut self, session_ref: &str) -> Result<bool, HostingError> {
        {
            let s = self.session_mut(session_ref)?;
            if s.closed {
                return Err(HostingError::SessionState {
                    session: session_ref.to_string(),
                    detail: "closed".into(),
                });
            }
        }
        self.adapter.cancel(session_ref, None)?;
        self.drain(session_ref)?;
        let open = self.session_mut(session_ref)?.open_turns.clone();
        for t in &open {
            self.pump_until(session_ref, t)?;
        }
        let still_open: Vec<String> = self
            .session_mut(session_ref)?
            .open_turns
            .iter()
            .cloned()
            .collect();
        if !still_open.is_empty() {
            // DRIFT — the cancel was sent and the turn continued.
            let declared = self
                .capability_vector
                .get("interrupt")
                .cloned()
                .unwrap_or_else(|| Json::str("unknown"));
            self.session_mut(session_ref)?
                .drift_annotations
                .push(Json::obj([
                    ("dimension", Json::str("interrupt")),
                    ("declared", declared),
                    ("observed", Json::str("ignored")),
                    ("observed_in", Json::str("run")),
                    (
                        "turns",
                        Json::Arr(still_open.iter().map(Json::str).collect()),
                    ),
                ]));
            return Ok(false);
        }
        Ok(true)
    }

    /// `resume(cursor)` — the mode gates on the declaration
    /// (`resume_cold`/`resume_warm`); a missing/undeclared mode refuses
    /// `CapabilityNotSupported` (never coerced).
    pub fn resume(&mut self, cursor: &ResumeCursor) -> Result<Opened, HostingError> {
        let dim = match cursor.mode.as_str() {
            "cold" => "resume_cold",
            "warm" => "resume_warm",
            other => {
                return Err(HostingError::CapabilityNotSupported {
                    verb: "resume".into(),
                    dimension: format!("resume_{other}"),
                })
            }
        };
        self.gate("resume", dim)?;
        self.next_session += 1;
        let session_ref = format!("hs-{}", self.next_session);
        self.adapter.resume(cursor, &session_ref)?;
        self.drain(session_ref.as_str())?;
        let spec = self
            .sessions
            .get(&cursor.session_ref)
            .map(|s| s.spec.clone())
            .unwrap_or_else(|| HostedRunSpec {
                definition_ref: String::new(),
                params: Json::Null,
                placement: ProcessPlacement::InEnvironment,
                connection_info: Json::obj([]),
                context_items: Vec::new(),
                budget_view: None,
                credential_channels: Vec::new(),
                resume_cursor: None,
            });
        self.sessions.insert(
            session_ref.clone(),
            HostedSession {
                session_ref: session_ref.clone(),
                spec,
                closed: false,
                events: std::mem::take(&mut self.pending_events),
                open_turns: BTreeSet::new(),
                usage: BTreeMap::new(),
                drift_annotations: Vec::new(),
                end_state: None,
                resumed_from: Some(cursor.session_ref.clone()),
            },
        );
        Ok(Opened {
            session_ref,
            capability_vector: self.capability_vector.clone(),
            abi_version: self.negotiated.version.as_string(),
        })
    }

    /// `close(session)` — the kernel's own snapshot at close (§6.6 §2.2):
    /// `ensure_terminal` synthesizes missing terminals `unobserved`,
    /// `validate_session` runs the I-1/I-2 + model_io gate, and the
    /// `EndState` is computed from the Lab's event log — never the
    /// participant's self-report.
    pub fn close(&mut self, session_ref: &str) -> Result<EndState, HostingError> {
        {
            let s = self.session_mut(session_ref)?;
            if s.closed {
                return Err(HostingError::SessionState {
                    session: session_ref.to_string(),
                    detail: "already closed".into(),
                });
            }
        }
        let reported = self.adapter.close(session_ref).ok();
        self.drain(session_ref)?;
        {
            let observability = self.observability.clone();
            let s = self.session_mut(session_ref)?;
            ensure_terminal(&mut s.events);
            validate_session(&s.events, &observability)?;
        }
        let s = self.session_mut(session_ref)?;
        s.closed = true;
        s.open_turns.clear();
        let turns = s.events.iter().filter(|e| e.kind == "turn.started").count() as u64;
        let usage = s
            .events
            .iter()
            .rev()
            .find(|e| e.kind == "usage.reported")
            .map(|e| e.payload.clone());
        let reason = reported
            .as_ref()
            .and_then(|r| r.get("reason").and_then(Json::as_str))
            .unwrap_or("closed")
            .to_string();
        let stop_reason = s
            .events
            .iter()
            .rev()
            .find_map(|e| e.payload.get("stop_reason").cloned());
        let end = EndState {
            session_ref: session_ref.to_string(),
            reason,
            turns,
            usage,
            artifacts: Vec::new(),
            stop_reason,
        };
        s.end_state = Some(end.clone());
        Ok(end)
    }

    /// `stream_events(session)` — the session's hosted events (the Lab's
    /// observation log — dense seq, validated).
    pub fn stream_events(&self, session_ref: &str) -> Result<&[HostedEvent], HostingError> {
        Ok(&self.session(session_ref)?.events)
    }

    /// `set_coordinate` — per-coordinate gate: `coordinate_<name>` for the
    /// named coordinates (`model` → `coordinate_model`), `coordinate_other`
    /// for the rest.
    pub fn set_coordinate(
        &mut self,
        session_ref: &str,
        dimension: &str,
        value: &Json,
    ) -> Result<(), HostingError> {
        let dim = if dimension == "model" {
            "coordinate_model".to_string()
        } else {
            let named = format!("coordinate_{dimension}");
            if self.capability_vector.contains_key(&named) {
                named
            } else {
                "coordinate_other".to_string()
            }
        };
        self.gate("set_coordinate", &dim)?;
        self.session_mut(session_ref)?;
        self.adapter.set_coordinate(session_ref, dimension, value)?;
        self.drain(session_ref)
    }

    /// `steer` — mid-turn steering; gates on `steer`, requires the turn
    /// running.
    pub fn steer(
        &mut self,
        session_ref: &str,
        turn_id: &str,
        text: &str,
    ) -> Result<(), HostingError> {
        self.gate("steer", "steer")?;
        let s = self.session_mut(session_ref)?;
        if !s.open_turns.contains(turn_id) {
            return Err(HostingError::SessionState {
                session: session_ref.to_string(),
                detail: format!("turn {turn_id} is not running"),
            });
        }
        self.adapter.steer(session_ref, turn_id, text)?;
        self.drain(session_ref)
    }

    /// `account` — exact account data; gates on `account_exact`.
    pub fn account(&mut self) -> Result<Json, HostingError> {
        self.gate("account", "account_exact")?;
        self.adapter.account()
    }

    /// `export` — the trajectory bundle; gates on `trajectory_export`.
    pub fn export(&mut self, session_ref: &str) -> Result<Json, HostingError> {
        self.gate("export", "trajectory_export")?;
        self.adapter.export(session_ref)
    }

    /// `elicit` — the Lab asks; gates on `elicitation`.
    pub fn elicit(&mut self, session_ref: &str, prompt: &Json) -> Result<Json, HostingError> {
        self.gate("elicit", "elicitation")?;
        self.adapter.elicit(session_ref, prompt)
    }

    /// `deliver_credential` — only over a declared `credential_supply`
    /// channel, and only a *ref* (the value never crosses the verb surface).
    pub fn deliver_credential(
        &mut self,
        session_ref: &str,
        channel: &str,
        credential_ref: &str,
    ) -> Result<(), HostingError> {
        let declared: BTreeSet<String> = self
            .participant
            .hosting_ext
            .credential_supply
            .clone()
            .unwrap_or_default()
            .into_iter()
            .collect();
        if !declared.contains(channel) {
            return Err(HostingError::CredentialChannelUndeclared {
                channel: channel.to_string(),
            });
        }
        self.session_mut(session_ref)?;
        self.adapter
            .deliver_credential(session_ref, channel, credential_ref)?;
        self.drain(session_ref)
    }

    /// Advance the Lab's wall clock (`ms`) — the `time.wall_ms` decision
    /// point (adapter-monotone; deterministic). Exhaustion closes the
    /// session `budget_exhausted{time.wall_ms}`.
    pub fn tick_wall_ms(&mut self, session_ref: &str, ms: i64) -> Result<(), HostingError> {
        self.wall_ms += ms;
        let used = self.wall_ms;
        match self.ceiling(session_ref, "time.wall_ms", used) {
            Ok(()) => Ok(()),
            Err(e) => {
                self.exhaust(session_ref, "time.wall_ms")?;
                Err(e)
            }
        }
    }

    /// The records-in/records-out surface (the boundary's hosting-plane
    /// seam — `op` is a verb spelling, `params` the verb record).
    pub fn handle(&mut self, op: &str, params: &Json) -> Result<Json, HostingError> {
        if EXCLUDED_VERBS.contains(&op) {
            return Err(HostingError::ExcludedVerb {
                verb: op.to_string(),
            });
        }
        if !is_admitted_verb(op) && op != "drive_probe" {
            return Err(HostingError::UnknownVerb {
                verb: op.to_string(),
            });
        }
        let str_param = |k: &str| -> Result<String, HostingError> {
            params
                .get(k)
                .and_then(Json::as_str)
                .map(String::from)
                .ok_or_else(|| HostingError::SchemaViolation {
                    path: k.to_string(),
                    detail: "expected string".into(),
                })
        };
        match op {
            "describe" => Ok(self.describe()),
            "open" => {
                let spec = HostedRunSpec {
                    definition_ref: str_param("definition_ref")?,
                    params: params.get("params").cloned().unwrap_or(Json::Null),
                    placement: params
                        .get("placement")
                        .and_then(Json::as_str)
                        .and_then(ProcessPlacement::parse)
                        .ok_or_else(|| HostingError::SchemaViolation {
                            path: "placement".into(),
                            detail: "closed vocabulary".into(),
                        })?,
                    connection_info: params
                        .get("connection_info")
                        .cloned()
                        .unwrap_or(Json::obj([])),
                    context_items: params
                        .get("context_items")
                        .and_then(|c| match c {
                            Json::Arr(items) => Some(items.clone()),
                            _ => None,
                        })
                        .unwrap_or_default(),
                    budget_view: params.get("budget_view").cloned(),
                    credential_channels: params
                        .get("credential_channels")
                        .and_then(|c| match c {
                            Json::Arr(items) => Some(
                                items
                                    .iter()
                                    .filter_map(|i| i.as_str().map(String::from))
                                    .collect(),
                            ),
                            _ => None,
                        })
                        .unwrap_or_default(),
                    resume_cursor: None,
                };
                let o = self.open(spec)?;
                Ok(Json::obj([
                    ("session_ref", Json::str(o.session_ref)),
                    ("abi_version", Json::str(o.abi_version)),
                ]))
            }
            "submit" => {
                let tid = self.submit(
                    &str_param("session")?,
                    &params.get("prompt").cloned().unwrap_or(Json::Null),
                )?;
                Ok(Json::obj([("turn_id", Json::str(tid))]))
            }
            "cancel" => {
                let ok = self.cancel(&str_param("session")?)?;
                Ok(Json::obj([("cancelled", Json::Bool(ok))]))
            }
            "resume" => {
                let cursor = ResumeCursor {
                    session_ref: str_param("session_ref")?,
                    seq: params.get("seq").and_then(Json::as_int).unwrap_or(0) as u64,
                    mode: str_param("mode")?,
                    ext: BTreeMap::new(),
                };
                let o = self.resume(&cursor)?;
                Ok(Json::obj([("session_ref", Json::str(o.session_ref))]))
            }
            "close" => {
                let e = self.close(&str_param("session")?)?;
                Ok(Json::obj([
                    ("session_ref", Json::str(e.session_ref)),
                    ("reason", Json::str(e.reason)),
                    ("turns", Json::Int(e.turns as i64)),
                ]))
            }
            "stream_events" => {
                let evs: Vec<Json> = self
                    .stream_events(&str_param("session")?)?
                    .iter()
                    .map(|e| e.to_json())
                    .collect();
                Ok(Json::obj([("events", Json::Arr(evs))]))
            }
            "set_coordinate" => {
                self.set_coordinate(
                    &str_param("session")?,
                    &str_param("dimension")?,
                    &params.get("value").cloned().unwrap_or(Json::Null),
                )?;
                Ok(Json::obj([]))
            }
            "steer" => {
                self.steer(
                    &str_param("session")?,
                    &str_param("turn_id")?,
                    &str_param("text")?,
                )?;
                Ok(Json::obj([]))
            }
            "account" => self.account(),
            "export" => self.export(&str_param("session")?),
            "elicit" => self.elicit(
                &str_param("session")?,
                &params.get("prompt").cloned().unwrap_or(Json::Null),
            ),
            "drive_probe" | "probe" => crate::probes::drive_probe_json(self, params),
            _ => Err(HostingError::UnknownVerb {
                verb: op.to_string(),
            }),
        }
    }

    // ── internals ─────────────────────────────────────────────────────────

    fn session(&self, r: &str) -> Result<&HostedSession, HostingError> {
        self.sessions
            .get(r)
            .ok_or_else(|| HostingError::SessionState {
                session: r.to_string(),
                detail: "unknown session".into(),
            })
    }

    fn session_mut(&mut self, r: &str) -> Result<&mut HostedSession, HostingError> {
        self.sessions
            .get_mut(r)
            .ok_or_else(|| HostingError::SessionState {
                session: r.to_string(),
                detail: "unknown session".into(),
            })
    }

    /// Stamp + validate + ingest the adapter's lifted observations.
    fn ingest(
        &mut self,
        session_ref: &str,
        obs: Vec<LiftedObservation>,
    ) -> Result<(), HostingError> {
        for o in obs {
            self.clock += 1;
            let seq = self
                .sessions
                .get(session_ref)
                .map(|s| s.events.len() as u64)
                .unwrap_or(0);
            let e = HostedEvent {
                seq,
                session: session_ref.to_string(),
                at: self.clock,
                kind: o.kind,
                payload: o.payload,
                provenance: HostedProvenance {
                    origin: o.origin,
                    authority: o.authority,
                },
                mediation: o.mediation,
                event_channel: o.event_channel,
                raw_ref: o.raw_ref,
                ext: o.ext,
            };
            validate_event(&e)?;
            if let Some(tid) = e.payload.get("turn_id").and_then(Json::as_str) {
                if e.kind == "turn.finished" {
                    if let Some(s) = self.sessions.get_mut(session_ref) {
                        s.open_turns.remove(tid);
                    }
                }
            }
            match self.sessions.get_mut(session_ref) {
                Some(s) => s.events.push(e),
                None => self.pending_events.push(e),
            }
        }
        Ok(())
    }

    /// Pump the adapter and ingest for `session_ref`.
    fn drain(&mut self, session_ref: &str) -> Result<(), HostingError> {
        let obs = self.adapter.pump();
        self.ingest(session_ref, obs)
    }

    /// Pump until `turn_id` finishes or the bounded pump budget exhausts
    /// (upcall round-trips take a bounded number of pumps; a hung turn
    /// surfaces as *no finish* — the drift path, never a spin).
    fn pump_until(&mut self, session_ref: &str, turn_id: &str) -> Result<(), HostingError> {
        for _ in 0..16 {
            let open = self
                .session(session_ref)
                .map(|s| s.open_turns.contains(turn_id))
                .unwrap_or(false);
            if !open {
                return Ok(());
            }
            self.drain(session_ref)?;
        }
        Ok(())
    }

    /// Fold the session's `usage.reported` events into `usage` (the
    /// ceiling-check basis — participant-reported AND Lab-metered rows both
    /// count; the envelope's `mediation`/`origin` says which is which).
    fn accumulate_usage(&mut self, session_ref: &str) {
        if let Some(s) = self.sessions.get_mut(session_ref) {
            for e in &s.events {
                if e.kind == "usage.reported" {
                    if let Some(used) = e.payload.get("used").and_then(Json::as_int) {
                        *s.usage.entry("tokens.output.visible".into()).or_insert(0) += used;
                        *s.usage.entry("spend".into()).or_insert(0) += used;
                    }
                }
            }
        }
    }

    /// One decision point — `check_ceiling` over the derived map and the
    /// boundary-supplied hard cap.
    fn ceiling(&self, session_ref: &str, dimension: &str, used: i64) -> Result<(), HostingError> {
        let _ = session_ref;
        let cap = self.hard_caps.get(dimension);
        check_ceiling(
            &CeilingCheck {
                budget_id: cap.map(|(b, _)| b.clone()).unwrap_or_default(),
                dimension: dimension.to_string(),
                used,
                cap: cap.map(|(_, c)| *c),
            },
            &self.budget_enforcement,
        )
    }

    /// Exhaustion — `session.closed` first, then `budget_exhausted{dimension}`
    /// (§6.6 §8: exhaustion ends the run *after* `session.closed`, never
    /// before it — the session's terminal event is its own).
    fn exhaust(&mut self, session_ref: &str, dimension: &str) -> Result<(), HostingError> {
        if self
            .sessions
            .get(session_ref)
            .map(|s| s.closed)
            .unwrap_or(true)
        {
            return Ok(());
        }
        let _ = self.close(session_ref);
        // The exhaustion fact lands after `session.closed`.
        self.clock += 1;
        let seq = self
            .sessions
            .get(session_ref)
            .map(|s| s.events.len() as u64)
            .unwrap_or(0);
        let e = HostedEvent {
            seq,
            session: session_ref.to_string(),
            at: self.clock,
            kind: "budget.exhausted".to_string(),
            payload: Json::obj([
                ("dimension", Json::str(dimension)),
                ("synthesized", Json::Bool(true)),
            ]),
            provenance: HostedProvenance {
                origin: crate::events::HostedOrigin::Adapter,
                authority: hh_provenance::AuthorityClass::Unverified,
            },
            mediation: crate::events::Mediation::Observed,
            event_channel: crate::events::EventChannel::Handle,
            raw_ref: None,
            ext: BTreeMap::new(),
        };
        validate_event(&e)?;
        if let Some(s) = self.sessions.get_mut(session_ref) {
            s.events.push(e);
            if let Some(end) = &mut s.end_state {
                end.reason = format!("budget_exhausted{{{dimension}}}");
            }
        }
        Ok(())
    }
}

impl HostedRunSpec {
    fn resume_or_none(&self) -> Option<String> {
        self.resume_cursor.as_ref().map(|c| c.session_ref.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter_a::allow_all_decider;
    use crate::fixture::FixtureParticipant;
    use hh_hir::leaves::Text;
    use hh_hir::records::{AssumptionDebtRecord, EvidenceRef, ExpiryCondition, OwnerRef};
    use hh_ontology::debt::DebtStatus;
    use hh_provenance::ProvenanceRecord;

    pub fn complete_debt() -> AssumptionDebtRecord {
        let provenance = ProvenanceRecord::kernel("hh-hosting/test", 0);
        AssumptionDebtRecord {
            rule_id: "hh.hosting.adapter_a".into(),
            hypothesis: Text::new(
                "participants matching participant_selector behave per declaration_defaults",
                "hh.adapter.a",
                provenance.clone(),
            ),
            evidence_refs: vec![EvidenceRef::legacy("adapterA-fixture")],
            owner: OwnerRef::principal("hh.adapter.a"),
            expiry_condition: ExpiryCondition {
                kind: hh_ontology::debt::ExpiryKind::ProbeFailure,
                value: Some("P0 dimension drift".into()),
            },
            removal_test_ref: "tests/probes.rs".into(),
            status: DebtStatus::Active,
            debt_class: None,
            hypothesis_typed: None,
            scope: None,
            expiry: None,
            runway_ms: None,
            revalidation: None,
            removal_test: None,
            created_by: Some(provenance),
            created_at: Some(0),
            supersedes: None,
        }
    }

    pub fn test_participant(vector: &[(&str, &str)]) -> ParticipantRecord {
        let mut decl = BTreeMap::new();
        let mut capability_vector = BTreeMap::new();
        for (d, v) in vector {
            decl.insert(d.to_string(), Json::str(*v));
            capability_vector.insert(
                d.to_string(),
                hh_ontology::participant::CapabilityVerdict::parse(v)
                    .unwrap_or(hh_ontology::participant::CapabilityVerdict::Unknown),
            );
        }
        ParticipantRecord::new(
            "p:test",
            "1.0.0",
            hh_ontology::participant::ParticipantDescriptor {
                class: ParticipantClass::Hosted,
                hosting_mechanism: hh_ontology::participant::HostingMechanism::SessionAbi,
                observability_level: [Observability::Events, Observability::EndState]
                    .into_iter()
                    .collect(),
                capability_vector,
            },
            decl,
            crate::records::HostingExt {
                abi_versions: Some(vec!["hh-hosting/1".into()]),
                credential_supply: Some(vec!["env".into(), "mcp".into()]),
                ..Default::default()
            },
            BTreeMap::new(),
        )
        .expect("participant")
    }

    pub fn test_service(fixture: FixtureParticipant, vector: &[(&str, &str)]) -> HostingService {
        let participant = test_participant(vector);
        let adapter = AdapterA::new(
            Box::new(fixture),
            crate::adapter_a::adapter_a_record(
                "a/1",
                crate::records::ModelIoIntercept::None,
                complete_debt(),
            ),
            Some(allow_all_decider()),
        );
        let vector_map: BTreeMap<String, Json> = vector
            .iter()
            .map(|(d, v)| (d.to_string(), Json::str(*v)))
            .collect();
        HostingService::attach(participant, adapter, vector_map, true, BTreeMap::new())
            .expect("attach")
    }

    pub fn spec() -> HostedRunSpec {
        HostedRunSpec {
            definition_ref: "def:1".into(),
            params: Json::obj([]),
            placement: ProcessPlacement::InEnvironment,
            connection_info: Json::obj([("socket", Json::str("/tmp/x"))]),
            context_items: Vec::new(),
            budget_view: None,
            credential_channels: vec!["env".into()],
            resume_cursor: None,
        }
    }

    fn full_vector() -> Vec<(&'static str, &'static str)> {
        vec![
            ("streaming", "supported"),
            ("interrupt", "supported"),
            ("steer", "supported"),
            ("coordinate_model", "supported"),
            ("coordinate_other", "supported"),
            ("usage_reporting", "supported"),
            ("resume_cold", "supported"),
            ("resume_warm", "supported"),
            ("account_exact", "supported"),
            ("trajectory_export", "supported"),
            ("elicitation", "supported"),
            ("permission_surface", "supported"),
            ("instruction_delivery", "supported"),
        ]
    }

    #[test]
    fn the_verb_cycle_runs_end_to_end() {
        let mut svc = test_service(FixtureParticipant::default(), &full_vector());
        let opened = svc.open(spec()).expect("open");
        let tid = svc
            .submit(&opened.session_ref, &Json::str("hi"))
            .expect("submit");
        let events = svc.stream_events(&opened.session_ref).expect("stream");
        assert!(events.iter().any(|e| e.kind == "session.opened"));
        assert!(events.iter().any(|e| e.kind == "turn.started"));
        assert!(events.iter().any(|e| e.kind == "turn.finished" && {
            e.payload.get("turn_id").and_then(Json::as_str) == Some(tid.as_str())
        }));
        svc.set_coordinate(&opened.session_ref, "model", &Json::str("m2"))
            .expect("coordinate");
        let end = svc.close(&opened.session_ref).expect("close");
        assert!(end.turns >= 1);
        // Dense seq + lifecycle.
        for (i, e) in svc
            .stream_events(&opened.session_ref)
            .expect("stream")
            .iter()
            .enumerate()
        {
            assert_eq!(e.seq, i as u64);
        }
        assert_eq!(
            svc.stream_events(&opened.session_ref)
                .unwrap()
                .last()
                .unwrap()
                .kind,
            "session.closed"
        );
    }

    #[test]
    fn excluded_and_unknown_verbs_refuse_on_the_records_in_surface() {
        let mut svc = test_service(FixtureParticipant::default(), &full_vector());
        for v in ["set_policy", "deliver_credential", "read_ledger", "grant"] {
            assert!(
                matches!(
                    svc.handle(v, &Json::obj([])),
                    Err(HostingError::ExcludedVerb { .. })
                ),
                "{} must refuse",
                v
            );
        }
        assert!(matches!(
            svc.handle("vendor_method", &Json::obj([])),
            Err(HostingError::UnknownVerb { .. })
        ));
    }

    #[test]
    fn capability_declared_verbs_gate_on_the_vector() {
        let mut svc = test_service(FixtureParticipant::default(), &full_vector());
        let opened = svc.open(spec()).expect("open");
        assert!(svc.steer(&opened.session_ref, "t", "x").is_err()); // turn not running
        assert!(svc.account().is_ok());
        assert!(svc.export(&opened.session_ref).is_ok());
        // Undeclared dimension → CapabilityNotSupported.
        let mut svc2 = test_service(FixtureParticipant::default(), &[("steer", "unsupported")]);
        let opened2 = svc2.open(spec()).expect("open");
        assert!(matches!(
            svc2.steer(&opened2.session_ref, "t", "x"),
            Err(HostingError::CapabilityNotSupported { .. })
        ));
        assert!(matches!(
            svc2.account(),
            Err(HostingError::CapabilityNotSupported { .. })
        ));
    }

    #[test]
    fn the_boundary_rule_strips_authority_policy_credentials_and_budgets() {
        let mut svc = test_service(FixtureParticipant::default(), &full_vector());
        let mut s = spec();
        s.connection_info = Json::obj([("tool_handle", Json::str("h:1"))]);
        assert!(matches!(
            svc.open(s.clone()),
            Err(HostingError::SchemaViolation { .. })
        ));
        s.connection_info = Json::obj([("socket", Json::str("/tmp/x"))]);
        // Undeclared credential channel refuses.
        s.credential_channels = vec!["smtp".into()];
        assert!(matches!(
            svc.open(s.clone()),
            Err(HostingError::CredentialChannelUndeclared { .. })
        ));
        s.credential_channels = vec!["env".into()];
        assert!(svc.open(s).is_ok());
    }

    #[test]
    fn budget_view_rides_context_only_when_instruction_delivery_is_supported() {
        // declared supported → the view lands as a context item
        let mut svc = test_service(FixtureParticipant::default(), &full_vector());
        let mut s = spec();
        s.budget_view = Some(Json::obj([("tokens", Json::Int(100))]));
        let opened = svc.open(s).expect("open");
        let seen = svc
            .session(&opened.session_ref)
            .unwrap()
            .spec
            .context_items
            .clone();
        assert!(seen
            .iter()
            .any(|c| c.get("kind").and_then(Json::as_str) == Some("budget_view")));
        // undeclared → dropped
        let mut svc2 = test_service(FixtureParticipant::default(), &[("steer", "supported")]);
        let mut s2 = spec();
        s2.budget_view = Some(Json::obj([("tokens", Json::Int(100))]));
        let o2 = svc2.open(s2).expect("open");
        assert!(svc2
            .session(&o2.session_ref)
            .unwrap()
            .spec
            .context_items
            .is_empty());
    }

    #[test]
    fn resume_gates_on_the_declared_mode() {
        let mut svc = test_service(FixtureParticipant::default(), &full_vector());
        let opened = svc.open(spec()).expect("open");
        svc.submit(&opened.session_ref, &Json::str("x")).unwrap();
        svc.close(&opened.session_ref).unwrap();
        let cursor = ResumeCursor {
            session_ref: opened.session_ref.clone(),
            seq: 3,
            mode: "cold".into(),
            ext: BTreeMap::new(),
        };
        let resumed = svc.resume(&cursor).expect("cold resume");
        assert_ne!(resumed.session_ref, opened.session_ref);
        // A participant without resume_warm refuses.
        let mut cold_only = FixtureParticipant::default();
        cold_only.resume_warm = false;
        let mut svc2 = test_service(cold_only, &full_vector());
        let o2 = svc2.open(spec()).unwrap();
        svc2.close(&o2.session_ref).unwrap();
        let c2 = ResumeCursor {
            session_ref: o2.session_ref,
            seq: 0,
            mode: "warm".into(),
            ext: BTreeMap::new(),
        };
        // warm declared in vector but transport refuses → Transport error
        assert!(svc2.resume(&c2).is_err());
        let mut svc3 = test_service(
            FixtureParticipant::default(),
            &[("steer", "supported")], // no resume dims declared
        );
        let c3 = ResumeCursor {
            session_ref: "hs-1".into(),
            seq: 0,
            mode: "cold".into(),
            ext: BTreeMap::new(),
        };
        assert!(matches!(
            svc3.resume(&c3),
            Err(HostingError::CapabilityNotSupported { .. })
        ));
    }

    #[test]
    fn an_auto_approving_adapter_fails_admission() {
        let participant = test_participant(&full_vector());
        let mut adapter = AdapterA::new(
            Box::new(FixtureParticipant::default()),
            crate::adapter_a::adapter_a_record(
                "a/1",
                crate::records::ModelIoIntercept::None,
                complete_debt(),
            ),
            Some(allow_all_decider()),
        );
        adapter.auto_approve = true;
        let vector: BTreeMap<String, Json> = full_vector()
            .iter()
            .map(|(d, v)| (d.to_string(), Json::str(*v)))
            .collect();
        assert!(matches!(
            HostingService::attach(participant, adapter, vector, true, BTreeMap::new()),
            Err(HostingError::AdapterRefused { .. })
        ));
    }

    #[test]
    fn incomplete_debt_fails_admission() {
        let participant = test_participant(&full_vector());
        let mut debt = complete_debt();
        debt.evidence_refs.clear();
        let adapter = AdapterA::new(
            Box::new(FixtureParticipant::default()),
            crate::adapter_a::adapter_a_record("a/1", crate::records::ModelIoIntercept::None, debt),
            Some(allow_all_decider()),
        );
        assert!(matches!(
            HostingService::attach(participant, adapter, BTreeMap::new(), true, BTreeMap::new()),
            Err(HostingError::AdapterRefused { .. })
        ));
    }
}
