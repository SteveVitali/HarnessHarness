//! S3.4d — the Hosting ABI schema slice (R-2.10.6⁰; §6.6; ADR-0164/0165/0166 (e)).
//!
//! Covers: the `HostedEvent` codec + I-1…I-5 validators, `proj_ABI` +
//! `lift` + the `LoweringLossReport`, the `ParticipantRecord`/`AdapterRecord`
//! schemas, adapter zero, and the AC-R-2.10.6-2 parity claim (adapter zero
//! vs native over `hh_telemetry::metric_view`).

use std::collections::{BTreeMap, BTreeSet};

use hh_hosting::{
    adapter_zero::{
        adapter_zero_descriptor, adapter_zero_ext, adapter_zero_record, check_adapter_zero,
        project_native_run, ADAPTER_ZERO_ID,
    },
    events::{
        ensure_terminal, envelope_stamps, validate_event, validate_session, EventChannel,
        HostedError, HostedEvent, HostedOrigin, HostedProvenance, Mediation,
    },
    proj::{lift, lift_stop_reason, project, LossClass, LossSeverity},
    records::{
        AdapterRecord, HostingExt, ParticipantRecord, ProcessPlacement, RecordError,
        HOSTING_EXT_KEY,
    },
};
use hh_ledger::classes::Durability;
use hh_ledger::event::{EventEnvelope, EventPlane, Producer, Scope};
use hh_ledger::manifest::{ObservabilityLevel, ParticipantClass};
use hh_ontology::control::{InfraErrorFamily, StopReason};
use hh_ontology::participant::{HostingMechanism, Observability};
use hh_provenance::AuthorityClass;
use hh_wire::Json;

// ── fixtures ─────────────────────────────────────────────────────────────────

/// A hand-built durable envelope (the parity fixture never needs a `Store` —
/// `proj_ABI` is a pure function over the durable slice).
fn env(seq: u64, class: &str, scope: Scope, payload: Json) -> EventEnvelope {
    EventEnvelope {
        event_id: format!("e{seq:04}"),
        run_id: "r1".into(),
        seq,
        ts: "2026-09-22T00:00:00.000Z".into(),
        hlc: None,
        plane: EventPlane::of_class(class).unwrap_or(EventPlane::Observation),
        class: class.into(),
        schema_version: 1,
        producer: Producer::kernel("kernel:test"),
        participant_class: ParticipantClass::Native,
        observability_level: [
            ObservabilityLevel::Events,
            ObservabilityLevel::ModelIo,
            ObservabilityLevel::EndState,
            ObservabilityLevel::Ledger,
        ]
        .into_iter()
        .collect(),
        durability: Durability::Ledger,
        scope,
        lease_generation: 0,
        parent_event_id: "root".into(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(hh_provenance::ProvenanceRecord::kernel("kernel:test", 0)),
        prev_hash: format!("h{}", seq.saturating_sub(1)),
        payload,
        hash: format!("h{seq}"),
    }
}

fn turn_scope(turn: &str) -> Scope {
    Scope {
        turn_id: Some(turn.into()),
        ..Scope::default()
    }
}

/// A minimal native run — one turn, one model call, one tool call, plus rows
/// from each lowering disposition (`none`, `hint`, `narrowed`, passthrough).
fn native_run() -> Vec<EventEnvelope> {
    let t = turn_scope("t1");
    vec![
        env(0, "lifecycle.run.created", Scope::default(), Json::obj([])),
        env(
            1,
            "lifecycle.turn.started",
            t.clone(),
            Json::obj([("turn_id", Json::str("t1"))]),
        ),
        env(
            2,
            "lifecycle.lease.acquired",
            Scope::default(),
            Json::obj([("lease", Json::str("l1"))]),
        ),
        env(
            3,
            "model.call.requested",
            t.clone(),
            Json::obj([("model_call_id", Json::str("m1"))]),
        ),
        env(
            4,
            "model.call.attempt.completed",
            t.clone(),
            Json::obj([
                ("attempt_no", Json::Int(0)),
                (
                    "timing",
                    Json::obj([
                        ("request_sent_ms", Json::Int(100)),
                        ("first_byte_ms", Json::Int(200)),
                        ("first_token_ms", Json::Int(300)),
                        ("last_byte_ms", Json::Int(500)),
                    ]),
                ),
                ("measured_at", Json::str("adapter")),
            ]),
        ),
        env(
            5,
            "model.call.completed",
            t.clone(),
            Json::obj([
                ("model_call_id", Json::str("m1")),
                (
                    "usage",
                    Json::obj([
                        ("input_total", Json::Int(1000)),
                        ("input_uncached", Json::Int(100)),
                        ("cache_read", Json::Int(900)),
                        ("cache_write", Json::Int(0)),
                        ("output_total", Json::Int(40)),
                        ("output_reasoning", Json::Int(10)),
                    ]),
                ),
                // `latency_ms` is NOT in the typed keep-list — the `narrowed`
                // loss entry must name it.
                ("latency_ms", Json::Int(400)),
            ]),
        ),
        env(
            6,
            "action.tool.proposed",
            t.clone(),
            Json::obj([
                ("tool_call_id", Json::str("tc1")),
                ("tool_class", Json::str("fs.read")),
            ]),
        ),
        env(
            7,
            "action.tool.completed",
            t.clone(),
            Json::obj([
                ("tool_call_id", Json::str("tc1")),
                ("tool_class", Json::str("fs.read")),
                ("content", Json::str("bytes")),
                ("locations", Json::Arr(vec![])),
            ]),
        ),
        env(
            8,
            "control.retry.fired",
            t.clone(),
            Json::obj([("attempt", Json::Int(1))]),
        ),
        env(
            9,
            "measurement.cost.attributed",
            t.clone(),
            Json::obj([
                ("money_micro", Json::Int(7)),
                ("currency", Json::str("usd")),
            ]),
        ),
        env(
            10,
            "security.permission.decided",
            t.clone(),
            Json::obj([
                ("permission_id", Json::str("p1")),
                ("tool_call_id", Json::str("tc1")),
                ("decision", Json::str("allow")),
                ("decider", Json::str("policy")),
                ("approval_wait_ms", Json::Int(0)),
                // dropped by the typed keep-list → declared `narrowed`.
                ("internal_note", Json::str("x")),
            ]),
        ),
        env(
            11,
            "lifecycle.turn.finished",
            t.clone(),
            Json::obj([
                ("turn_id", Json::str("t1")),
                ("stop_reason", StopReason::Completed.to_json()),
            ]),
        ),
        env(
            12,
            "lifecycle.run.finished",
            Scope::default(),
            Json::obj([
                ("wall_ms", Json::Int(900)),
                ("stop_reason", StopReason::Completed.to_json()),
            ]),
        ),
    ]
}

fn hosted(kind: &str, session: &str, seq: u64) -> HostedEvent {
    HostedEvent {
        seq,
        session: session.into(),
        at: seq,
        kind: kind.into(),
        payload: Json::obj([]),
        provenance: HostedProvenance {
            origin: HostedOrigin::Participant,
            authority: AuthorityClass::Delegate,
        },
        mediation: Mediation::Observed,
        event_channel: EventChannel::Protocol,
        raw_ref: None,
        ext: BTreeMap::new(),
    }
}

// ── HostedEvent codec + I-3 preservation ─────────────────────────────────────

#[test]
fn hosted_event_codec_round_trips_and_preserves_extensions() {
    let mut e = hosted("tool.proposed", "s1", 0);
    e.payload = Json::obj([("kind_hint", Json::str("fs.read"))]);
    e.provenance.origin = HostedOrigin::Ext("_vendor".into());
    e.ext
        .insert("hh.other/1".into(), Json::obj([("k", Json::Int(1))]));
    let j = e.to_json();
    // An unknown top-level `_`-prefixed member folds into `ext` (I-3).
    let Json::Obj(mut m) = j else {
        panic!("object")
    };
    m.insert("_trace".into(), Json::str("abc"));
    let j = Json::Obj(m);
    let back = HostedEvent::from_json(&j).expect("decode");
    assert_eq!(back.kind, "tool.proposed");
    assert_eq!(back.provenance.origin, HostedOrigin::Ext("_vendor".into()));
    assert_eq!(back.ext.get("_trace"), Some(&Json::str("abc")));
    assert_eq!(
        back.ext.get("hh.other/1"),
        Some(&Json::obj([("k", Json::Int(1))]))
    );
    // A non-`_` unknown top-level member refuses.
    let Json::Obj(mut m) = j else {
        panic!("object")
    };
    m.insert("surprise".into(), Json::Int(1));
    assert!(matches!(
        HostedEvent::from_json(&Json::Obj(m)),
        Err(HostedError::UnknownSpelling { .. })
    ));
    // A non-`_` unknown enum spelling refuses.
    let mut bad = e.to_json();
    if let Json::Obj(m) = &mut bad {
        m.insert("mediation".into(), Json::str("weird"));
    }
    assert!(matches!(
        HostedEvent::from_json(&bad),
        Err(HostedError::UnknownSpelling { .. })
    ));
}

// ── validate_event (I-4 + member minimums) ───────────────────────────────────

#[test]
fn validate_event_enforces_the_authority_ceiling() {
    // participant origin + kernel authority → refused (I-4).
    let mut e = hosted("tool.proposed", "s1", 0);
    e.payload = Json::obj([("kind_hint", Json::str("fs.read"))]);
    e.provenance.authority = AuthorityClass::Kernel;
    assert!(matches!(
        validate_event(&e),
        Err(HostedError::AuthorityCeiling { .. })
    ));
    // environment origin + kernel authority + closed kind → admissible.
    e.provenance.origin = HostedOrigin::Environment;
    assert!(validate_event(&e).is_ok());
    // …but never on an unknown kind (I-4's closed-schema clause).
    e.kind = "_vendor.thing".into();
    assert!(matches!(
        validate_event(&e),
        Err(HostedError::AuthorityCeiling { .. })
    ));
}

#[test]
fn validate_event_refuses_mediated_on_a_participant_claim() {
    let mut e = hosted("tool.completed", "s1", 0);
    e.payload = Json::obj([("status", Json::str("completed"))]);
    e.mediation = Mediation::Mediated;
    assert!(matches!(
        validate_event(&e),
        Err(HostedError::MediatedRequiresKernelChannel { .. })
    ));
    // The adapter itself can never assert `mediated` either.
    e.provenance.origin = HostedOrigin::Adapter;
    assert!(matches!(
        validate_event(&e),
        Err(HostedError::MediatedRequiresKernelChannel { .. })
    ));
    // An intercept/environment origin may.
    e.provenance.origin = HostedOrigin::Intercept;
    assert!(validate_event(&e).is_ok());
}

#[test]
fn validate_event_enforces_known_kind_member_minimums() {
    let mut e = hosted("tool.proposed", "s1", 0);
    assert!(matches!(
        validate_event(&e),
        Err(HostedError::PayloadShape { .. })
    ));
    e.payload = Json::obj([("kind_hint", Json::str("fs.read"))]);
    assert!(validate_event(&e).is_ok());
    let mut u = hosted("usage.reported", "s1", 0);
    u.payload = Json::obj([("used", Json::Int(1))]); // `size` missing
    assert!(matches!(
        validate_event(&u),
        Err(HostedError::PayloadShape { .. })
    ));
}

// ── validate_session / ensure_terminal (I-1/I-2 + the model_io gate) ─────────

#[test]
fn validate_session_checks_dense_seq_opened_and_model_io() {
    let obs: BTreeSet<Observability> = [Observability::Events, Observability::EndState]
        .into_iter()
        .collect();
    let mut events = vec![
        hosted("session.opened", "s1", 0),
        hosted("turn.started", "s1", 1),
        hosted("session.closed", "s1", 2),
    ];
    assert!(validate_session(&events, &obs).is_ok());
    // I-2: a seq gap refuses.
    events[1].seq = 5;
    assert!(matches!(
        validate_session(&events, &obs),
        Err(HostedError::SeqNotDense { .. })
    ));
    events[1].seq = 1;
    // I-1: a missing `session.opened` refuses.
    let unopened = vec![
        hosted("turn.started", "s1", 0),
        hosted("session.closed", "s1", 1),
    ];
    assert!(matches!(
        validate_session(&unopened, &obs),
        Err(HostedError::MissingSessionOpen { .. })
    ));
    // The `model_io` gate: model.call.* without a declared `model_io` refuses.
    let mut e = hosted("model.call.completed", "s1", 0);
    e.seq = 3;
    e.at = 3;
    e.payload = Json::obj([("status", Json::str("completed"))]);
    events.push(e);
    assert!(matches!(
        validate_session(&events, &obs),
        Err(HostedError::ModelIoUndeclared { .. })
    ));
    // …and is admissible once `model_io` is declared.
    let mut obs2 = obs.clone();
    obs2.insert(Observability::ModelIo);
    assert!(validate_session(&events, &obs2).is_ok());
}

#[test]
fn ensure_terminal_synthesizes_unobserved_terminals() {
    let mut events = vec![hosted("session.opened", "s1", 0), {
        let mut e = hosted("turn.started", "s1", 1);
        e.payload = Json::obj([("turn_id", Json::str("t1"))]);
        e
    }];
    ensure_terminal(&mut events);
    // An open turn gets a synthesized `turn.finished`; the session gets a
    // synthesized `session.closed` — both `unobserved`, adapter origin.
    let closed = events
        .iter()
        .find(|e| e.kind == "turn.finished")
        .expect("turn close");
    assert_eq!(closed.mediation, Mediation::Unobserved);
    assert_eq!(closed.provenance.origin, HostedOrigin::Adapter);
    assert_eq!(closed.payload.get("synthesized"), Some(&Json::Bool(true)));
    assert_eq!(
        events.last().map(|e| e.kind.as_str()),
        Some("session.closed")
    );
    // The completed slice validates (dense seq, opened first).
    let obs: BTreeSet<Observability> = [Observability::Events].into_iter().collect();
    assert!(validate_session(&events, &obs).is_ok());
    // Idempotent.
    let n = events.len();
    ensure_terminal(&mut events);
    assert_eq!(events.len(), n);
}

#[test]
fn envelope_stamps_carry_the_i5_members() {
    let e = hosted("session.opened", "s1", 0);
    let obs: BTreeSet<Observability> = [Observability::Events, Observability::EndState]
        .into_iter()
        .collect();
    let s = envelope_stamps(&e, &obs);
    assert_eq!(s.get("participant_class"), Some(&Json::str("hosted")));
    assert_eq!(s.get("mediation"), Some(&Json::str("observed")));
    assert!(matches!(
        s.get("observability_level"),
        Some(Json::Arr(v)) if v.len() == 2
    ));
}

// ── proj_ABI + lift + the loss report ────────────────────────────────────────

#[test]
fn project_emits_typed_kinds_dense_seq_and_declared_loss() {
    let obs = adapter_zero_descriptor().observability_level;
    let p = project(&native_run(), &obs);
    assert_eq!(p.session, "r1");
    // Dense seq / monotone at (I-2).
    let native_ids: BTreeSet<String> = native_run().iter().map(|e| e.event_id.clone()).collect();
    for (i, e) in p.events.iter().enumerate() {
        assert_eq!(e.seq, i as u64);
        assert_eq!(e.at, i as u64);
        assert_eq!(e.session, "r1");
        // `raw_ref` is a *reference* back to the native event_id (never
        // embedded content).
        assert!(native_ids.contains(e.raw_ref.as_deref().unwrap_or_default()));
    }
    let kinds: Vec<&str> = p.events.iter().map(|e| e.kind.as_str()).collect();
    assert!(kinds.contains(&"session.opened"));
    assert!(kinds.contains(&"session.closed"));
    assert!(kinds.contains(&"turn.started"));
    assert!(kinds.contains(&"turn.finished"));
    assert!(kinds.contains(&"model.call.started"));
    assert!(kinds.contains(&"model.call.completed"));
    assert!(kinds.contains(&"tool.proposed"));
    assert!(kinds.contains(&"tool.completed"));
    assert!(kinds.contains(&"permission.decided"));
    // Every emitted event validates against the session observability.
    assert!(validate_session(&p.events, &obs).is_ok());
    // The loss report declares the `none` class, both `hint` classes, and the
    // `narrowed` droppings — nothing silently lost (CC3).
    let loss: BTreeMap<&str, &hh_hosting::proj::LossEntry> = p
        .loss
        .entries
        .iter()
        .map(|e| (e.class.as_str(), e))
        .collect();
    assert_eq!(
        loss.get("lifecycle.lease.acquired").map(|e| e.loss_class),
        Some(LossClass::NoSlot)
    );
    assert_eq!(
        loss.get("control.retry.fired").map(|e| e.loss_class),
        Some(LossClass::HintOnly)
    );
    assert_eq!(
        loss.get("measurement.cost.attributed")
            .map(|e| e.loss_class),
        Some(LossClass::HintOnly)
    );
    let narrowed = loss.get("model.call.completed").expect("narrowed entry");
    assert_eq!(narrowed.loss_class, LossClass::Narrowed);
    assert!(narrowed.detail.contains("latency_ms"));
    let narrowed = loss
        .get("security.permission.decided")
        .expect("narrowed entry");
    assert!(narrowed.detail.contains("internal_note"));
    assert_eq!(narrowed.severity, LossSeverity::Narrowed);
    // Every loss entry is for a class that actually occurred; every class
    // mapped `none`/`hint` appears (declared, never silent).
    assert!(loss
        .keys()
        .all(|c| native_run().iter().any(|e| e.class == *c)));
}

#[test]
fn project_gates_model_io_kinds_on_undeclared_interception() {
    // An unintercepted hosted session declares no `model_io` — the model-I/O
    // kinds land in the loss report, never in the stream (§6.6 §2.2).
    let obs: BTreeSet<Observability> = [Observability::Events, Observability::EndState]
        .into_iter()
        .collect();
    let p = project(&native_run(), &obs);
    assert!(!p.events.iter().any(|e| e.kind.starts_with("model.call.")));
    let gated: Vec<&str> = p
        .loss
        .entries
        .iter()
        .filter(|e| e.detail.contains("observability-gated"))
        .map(|e| e.class.as_str())
        .collect();
    assert!(gated.contains(&"model.call.requested"));
    assert!(gated.contains(&"model.call.completed"));
    assert!(validate_session(&p.events, &obs).is_ok());
}

#[test]
fn lift_restores_native_classes_and_never_drops() {
    let obs = adapter_zero_descriptor().observability_level;
    let p = project(&native_run(), &obs);
    let rows = lift(&p.events);
    let classes: Vec<&str> = rows.iter().map(|r| r.class.as_str()).collect();
    // The typed kinds lift back to their native classes…
    assert!(classes.contains(&"lifecycle.run.created"));
    assert!(classes.contains(&"lifecycle.turn.started"));
    assert!(classes.contains(&"model.call.requested"));
    assert!(classes.contains(&"action.tool.proposed"));
    assert!(classes.contains(&"security.permission.decided"));
    // …the hints restore verbatim (class + payload — data, never authority)…
    let cost = rows
        .iter()
        .find(|r| r.class == "measurement.cost.attributed")
        .expect("hint row");
    assert_eq!(
        cost.payload.get("money_micro"),
        Some(&Json::Int(7)),
        "hint restores the source payload verbatim"
    );
    // …and `turn.finished` carries a decoded `stop_reason` (the §6.6 lift).
    let fin = rows
        .iter()
        .find(|r| r.class == "lifecycle.turn.finished")
        .expect("turn.finished");
    assert!(fin.payload.get("stop_reason").is_some());
    assert_eq!(fin.turn_id.as_deref(), Some("t1"));
}

#[test]
fn lift_maps_unknown_kinds_to_the_native_record_leaf() {
    let mut e = hosted("_vendor.subagent", "s1", 0);
    e.payload = Json::obj([("k", Json::Int(1))]);
    let rows = lift(&[e]);
    assert_eq!(rows[0].class, "lifecycle.hosted.native_record");
    assert_eq!(
        rows[0].payload.get("kind"),
        Some(&Json::str("_vendor.subagent"))
    );
    assert_eq!(
        rows[0].payload.get("payload"),
        Some(&Json::obj([("k", Json::Int(1))]))
    );
}

#[test]
fn lift_stop_reason_table_and_unclassified_fallback() {
    assert_eq!(lift_stop_reason("end_turn"), StopReason::Completed);
    assert!(matches!(
        lift_stop_reason("max_tokens"),
        StopReason::BudgetExhausted { .. }
    ));
    assert!(matches!(
        lift_stop_reason("cancelled"),
        StopReason::Cancelled { .. }
    ));
    // An `other`/`_vendor` spelling → infrastructure_failure{participant_…}
    // with the raw word preserved (never a guess, never a panic).
    match lift_stop_reason("_vendor_done") {
        StopReason::InfrastructureFailure { error_class } => {
            assert_eq!(error_class.family, InfraErrorFamily::Participant);
            assert_eq!(error_class.class, "_vendor_done");
        }
        other => panic!("expected participant_unclassified, got {other:?}"),
    }
    // …and the spelling decodes back through the canonical codec (CC1).
    let j = lift_stop_reason("_vendor_done").to_json();
    assert!(StopReason::from_json(&j).is_some());
}

// ── records (participant / adapter / hosting ext) ────────────────────────────

fn participant() -> ParticipantRecord {
    ParticipantRecord::new(
        "hh.reference",
        "1.0.0",
        adapter_zero_descriptor(),
        [("streaming".to_string(), Json::str("supported"))]
            .into_iter()
            .collect(),
        adapter_zero_ext(),
        BTreeMap::new(),
    )
    .expect("participant")
}

#[test]
fn participant_record_codec_and_derived_identity() {
    let p = participant();
    let j = p.to_json();
    assert_eq!(j.get("kind"), Some(&Json::str("participant")));
    assert!(j.get("ext").and_then(|e| e.get(HOSTING_EXT_KEY)).is_some());
    let back = ParticipantRecord::from_json(&j).expect("decode");
    assert_eq!(back, p);
    // A tampered `version_identity` refuses (re-derived on decode).
    let mut bad = j.clone();
    if let Json::Obj(m) = &mut bad {
        m.insert("version_identity".into(), Json::str("idp:wrong"));
    }
    assert!(matches!(
        ParticipantRecord::from_json(&bad),
        Err(RecordError::VersionIdentityMismatch)
    ));
    // A non-hosted class refuses.
    let mut d = adapter_zero_descriptor();
    d.class = hh_ontology::participant::ParticipantClass::Native;
    assert!(matches!(
        ParticipantRecord::new(
            "x",
            "1",
            d,
            BTreeMap::new(),
            HostingExt::default(),
            BTreeMap::new()
        ),
        Err(RecordError::ClassNotHosted)
    ));
    // CF-351: `hosting_mechanism = none` is native-only.
    let mut d = adapter_zero_descriptor();
    d.hosting_mechanism = HostingMechanism::None;
    assert!(matches!(
        ParticipantRecord::new(
            "x",
            "1",
            d,
            BTreeMap::new(),
            HostingExt::default(),
            BTreeMap::new()
        ),
        Err(RecordError::MechanismNoneOnHosted)
    ));
}

#[test]
fn adapter_record_codec_claims_and_debt() {
    let a = adapter_zero_record();
    assert_eq!(a.adapter_id, ADAPTER_ZERO_ID);
    assert!(a.claims(
        HostingMechanism::SessionAbi,
        ProcessPlacement::InEnvironment
    ));
    assert!(!a.claims(HostingMechanism::SessionAbi, ProcessPlacement::LabHost));
    let j = a.to_json();
    assert_eq!(j.get("kind"), Some(&Json::str("adapter")));
    // Canonical-byte equality (the `Text` member of `debt` is content-
    // addressed — hash-only on the wire — so compare canonical forms).
    assert_eq!(
        AdapterRecord::from_json(&j).expect("decode").to_json(),
        a.to_json()
    );
    // The debt record is complete-by-schema and round-trips through the
    // canonical `AssumptionDebtRecord/1` codec (T-LCD-05 reflexive).
    let mut bad = j.clone();
    if let Json::Obj(m) = &mut bad {
        m.insert("debt".into(), Json::obj([]));
    }
    assert!(matches!(
        AdapterRecord::from_json(&bad),
        Err(RecordError::DebtSchema { .. })
    ));
}

#[test]
fn adapter_zero_fixture_is_self_consistent() {
    check_adapter_zero().expect("adapter zero self-consistency");
    let d = adapter_zero_descriptor();
    assert_eq!(d.class, hh_ontology::participant::ParticipantClass::Hosted);
    assert_eq!(d.hosting_mechanism, HostingMechanism::SessionAbi);
    // `{events, end_state}` + `model_io` (intercepted), never `ledger`.
    assert!(d.observability_level.contains(&Observability::Events));
    assert!(d.observability_level.contains(&Observability::EndState));
    assert!(d.observability_level.contains(&Observability::ModelIo));
    assert!(!d.observability_level.contains(&Observability::Ledger));
}

// ── AC-R-2.10.6-2 — adapter-zero parity over the metric fold ─────────────────

/// Wrap a lifted row into a minimal durable envelope for the fold (the
/// metric fold reads `class`/`payload`/`scope`; the stamps are inert here).
fn lifted_envelope(row: &hh_hosting::proj::LiftedRow, seq: u64) -> EventEnvelope {
    env(
        seq,
        &row.class,
        Scope {
            turn_id: row.turn_id.clone(),
            ..Scope::default()
        },
        row.payload.clone(),
    )
}

#[test]
fn ac_r_2_10_6_2_adapter_zero_parity_over_metric_view() {
    let native = native_run();
    let hosted_obs = adapter_zero_descriptor().observability_level;
    let p = project_native_run(&native);

    // The hosted artefact: lift the projected events back into native-class
    // envelopes (the analysis-side direction — §6.6 §2.2).
    let lifted = lift(&p.events);
    let hosted_events: Vec<EventEnvelope> = lifted
        .iter()
        .enumerate()
        .map(|(i, r)| lifted_envelope(r, i as u64))
        .collect();

    let native_declared: BTreeSet<Observability> = [
        Observability::Events,
        Observability::ModelIo,
        Observability::EndState,
        Observability::Ledger,
    ]
    .into_iter()
    .collect();

    let nv = hh_telemetry::metric_view("r1", "r1", &native_declared, &native, None);
    let hv = hh_telemetry::metric_view("r1", "r1", &hosted_obs, &hosted_events, None);

    let Json::Obj(n_metrics) = &nv.payload.get("metrics").cloned().unwrap_or(Json::Null) else {
        panic!("native metrics")
    };
    let Json::Obj(h_metrics) = &hv.payload.get("metrics").cloned().unwrap_or(Json::Null) else {
        panic!("hosted metrics")
    };
    assert_eq!(n_metrics.len(), h_metrics.len());

    let mut parity_checked = 0;
    let mut na_checked = 0;
    for m in hh_telemetry::catalogue::PROCESS_METRICS {
        let required: BTreeSet<Observability> = m.requires_observability.iter().copied().collect();
        let ncell = n_metrics.get(m.name).expect("native cell");
        let hcell = h_metrics.get(m.name).expect("hosted cell");
        if required.is_subset(&hosted_obs) {
            // Identical values for every metric whose declared observability
            // the hosted session satisfies — the AC-2 equality leg (and
            // stronger: intercepted model_io metrics also agree).
            assert_eq!(
                hcell, ncell,
                "metric {} diverged under adapter zero",
                m.name
            );
            parity_checked += 1;
        } else {
            // Outside the declared set → `n/a{observability}` on the hosted
            // row, never 0, never a proxy (T-LCD-15).
            assert_eq!(
                hcell.get("na").and_then(Json::as_str),
                Some("observability"),
                "metric {} must render n/a{{observability}} on the hosted row",
                m.name
            );
            na_checked += 1;
        }
    }
    // The fixture actually exercises both legs (a vacuous parity is a lie).
    assert!(
        parity_checked >= 10,
        "parity leg under-exercised: {parity_checked}"
    );
    assert!(na_checked >= 1, "n/a leg under-exercised: {na_checked}");

    // The declared loss: every `none`/`hint_only` class appears exactly.
    let loss_classes: BTreeMap<&str, LossClass> = p
        .loss
        .entries
        .iter()
        .map(|e| (e.class.as_str(), e.loss_class))
        .collect();
    for e in &native {
        match hh_ledger::classes::hosted_lowering(&e.class) {
            "none" => assert_eq!(
                loss_classes.get(e.class.as_str()),
                Some(&LossClass::NoSlot),
                "{} must be declared in the loss report",
                e.class
            ),
            "hint" => assert_eq!(
                loss_classes.get(e.class.as_str()),
                Some(&LossClass::HintOnly),
                "{} must be declared hint_only in the loss report",
                e.class
            ),
            _ => {}
        }
    }
}
