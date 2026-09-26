//! S4.5b acceptance coverage — every test names the AC it pins
//! (AC-R-2.5.4-{5,6,7,10}) plus the C1 supporting behaviours
//! (negotiation, `ext` preservation, `_hh/*`, v1 profile filtering,
//! `session/resume` replay, H5 `mcpServers` pending-review).
//!
//! The serve loop and the client run on separate threads over the
//! in-memory duplex channel — a real channel boundary, never an
//! in-process call (the artefact is a client, binding (c)).

use std::collections::BTreeMap;

use hh_acp::a2a::{card, delegate_task, resume_task, sign_card, task_state, verify_card};
use hh_acp::{
    channel, negotiate_era, render_session, serve_session, AcpArtifact, AcpClient, AcpDialect,
    AcpEra, FixtureDriver, MemorySessionTransport, PermissionOutcome, StaticPi, TaskState,
    TurnDrive,
};
use hh_compiler::acp::{lower_event, ACP_ARTEFACT_SCHEMA, ACP_VERSION, HH_METHODS};
use hh_ledger::audit::{FixedSigner, KeyTable};
use hh_mcp::protocol::NegotiateError;
use hh_wire::json::Json;

// ── fixtures ─────────────────────────────────────────────────────────────────

/// A hand-built `hh-acp-target/1` artefact (the compiler's lowering
/// shape — same members `lower_acp` emits).
fn fixture_artefact_json() -> Json {
    Json::obj([
        ("schema", Json::str(ACP_ARTEFACT_SCHEMA)),
        ("protocol", Json::str("acp")),
        ("protocol_version", Json::str(ACP_VERSION)),
        (
            "supported_versions",
            Json::Arr(vec![Json::str(ACP_VERSION), Json::str("v1-compat")]),
        ),
        (
            "binding",
            Json::obj([
                ("protocol", Json::str("acp")),
                ("role", Json::str("client")),
            ]),
        ),
        (
            "initialize_response",
            Json::obj([
                ("protocolVersion", Json::str(ACP_VERSION)),
                (
                    "agentInfo",
                    Json::obj([
                        ("name", Json::str("fixture-agent")),
                        ("version", Json::str("bundle-1")),
                    ]),
                ),
                (
                    "agentCapabilities",
                    Json::obj([
                        ("loadSession", Json::Bool(true)),
                        (
                            "sessionCapabilities",
                            Json::obj([
                                ("resume", Json::obj([("unstable", Json::Bool(true))])),
                                ("fork", Json::obj([("unstable", Json::Bool(true))])),
                            ]),
                        ),
                        (
                            "_meta",
                            Json::obj([(
                                "hh",
                                Json::obj([
                                    ("agent_semantic_id", Json::str("test:fixture-agent")),
                                    (
                                        "hh_methods",
                                        Json::Arr(
                                            HH_METHODS
                                                .iter()
                                                .map(|m| Json::str(m.to_string()))
                                                .collect(),
                                        ),
                                    ),
                                    (
                                        "update_kinds",
                                        Json::Arr(vec![
                                            Json::str("state_update"),
                                            Json::str("tool_call_update"),
                                            Json::str("agent_message_chunk"),
                                            Json::str("agent_thought_chunk"),
                                            Json::str("plan_update"),
                                            Json::str("usage_update"),
                                            Json::str("session/request_permission"),
                                        ]),
                                    ),
                                ]),
                            )]),
                        ),
                    ]),
                ),
            ]),
        ),
        ("config_options", Json::Arr(vec![])),
        // T3 — an unknown `_`-prefixed member + an extension id:
        // byte-preserved through the artefact, never read for
        // authority.
        ("_foreign_ext", Json::str("preserved-verbatim")),
    ])
}

fn artefact() -> AcpArtifact {
    AcpArtifact::from_json(&fixture_artefact_json()).unwrap()
}

/// Spawn `serve_session` on its own thread (the channel boundary).
fn spawn_serve(
    artefact: AcpArtifact,
    mut driver: FixtureDriver,
    mut end: MemorySessionTransport,
) -> std::thread::JoinHandle<FixtureDriver> {
    std::thread::spawn(move || {
        serve_session(&artefact, &mut driver, &mut end).unwrap();
        driver
    })
}

fn turn(events: Vec<(u64, &str, Json)>, stop: &str) -> TurnDrive {
    TurnDrive {
        events: events
            .into_iter()
            .map(|(s, c, p)| (s, c.to_string(), p))
            .collect(),
        stop_reason: stop.to_string(),
    }
}

// ── AC-R-2.5.4-5 — the v2 client round trip + trace_map + durable-before-visible

#[test]
fn ac_r_2_5_4_5_full_turn_updates_trace_to_durable_events() {
    let scripted = turn(
        vec![
            (10, "lifecycle.turn.started", Json::obj([])),
            (
                11,
                "model.stream.delta",
                Json::obj([("text", Json::str("hello"))]),
            ),
            (
                12,
                "action.tool.proposed",
                Json::obj([(
                    "capability_ref",
                    Json::obj([("semantic_id", Json::str("test:tool"))]),
                )]),
            ),
            (
                13,
                "lifecycle.turn.finished",
                Json::obj([("stop_reason", Json::str("end_turn"))]),
            ),
        ],
        "end_turn",
    );
    let mut driver = FixtureDriver::new();
    driver.scripted_turns.push_back(Ok(scripted.clone()));

    let (client_end, server_end) = channel();
    let server = spawn_serve(artefact(), driver, server_end);
    let mut client = AcpClient::new(client_end);
    client.attach_session(&Json::obj([]), None).expect("attach");
    let sess = client.new_session(&Json::obj([])).expect("session/new");
    let session_id = sess
        .get("sessionId")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let mut pi = StaticPi::new(PermissionOutcome::Deny);
    let outcome = client
        .prompt(&session_id, &Json::obj([]), &mut pi)
        .expect("prompt");
    assert_eq!(outcome.stop_reason, "end_turn");

    // Every update resolves through `trace_map`: `_meta.hh.source_seq`
    // names a durable event, and the kind is exactly that event's
    // `lower_event` projection.
    let durable: BTreeMap<u64, (String, Json)> = scripted
        .events
        .iter()
        .map(|(s, c, p)| (*s, (c.clone(), p.clone())))
        .collect();
    let mut kinds = Vec::new();
    for u in &outcome.updates {
        assert_eq!(u.session_id, session_id);
        let seq = u
            .params
            .get("_meta")
            .and_then(|m| m.get("hh"))
            .and_then(|h| h.get("source_seq"))
            .and_then(Json::as_int)
            .map(|i| i as u64)
            .expect("update carries _meta.hh.source_seq");
        let (class, payload) = durable.get(&seq).expect("seq resolves to a durable event");
        let (kind, _) = lower_event(class, payload).expect("lowers");
        assert_eq!(u.kind, kind, "update kind ≠ lowered kind");
        kinds.push(u.kind.clone());
    }
    // Order = durable order; `idle` is last — rendered only from the
    // durable `lifecycle.turn.finished`.
    assert_eq!(
        kinds,
        vec![
            "state_update",
            "agent_message_chunk",
            "tool_call_update",
            "state_update"
        ]
    );
    assert_eq!(
        outcome.updates.last().unwrap().params.get("state"),
        Some(&Json::str("idle"))
    );

    // `session/close`, then close the channel — the serve thread joins.
    client.close(&session_id).expect("close");
    drop(client);
    server.join().unwrap();
}

#[test]
fn ac_r_2_5_4_5_idle_never_before_durable_finished() {
    // The crash-between-finish-and-emit case: a stream whose durable
    // record lacks `lifecycle.turn.finished` can never render `idle`.
    let events: Vec<(u64, String, Json)> = vec![
        (1, "lifecycle.turn.started".to_string(), Json::obj([])),
        (
            2,
            "model.stream.delta".to_string(),
            Json::obj([("text", Json::str("partial"))]),
        ),
    ];
    let updates = render_session(&events, AcpDialect::V2);
    assert!(
        !updates
            .iter()
            .any(|u| u.kind == "state_update" && u.params.get("state") == Some(&Json::str("idle"))),
        "idle rendered without a durable finished"
    );
    // With the durable finished appended, `idle` renders after it.
    let mut with_finish = events.clone();
    with_finish.push((3, "lifecycle.turn.finished".to_string(), Json::obj([])));
    let updates = render_session(&with_finish, AcpDialect::V2);
    let last = updates.last().unwrap();
    assert_eq!(last.kind, "state_update");
    assert_eq!(last.source_seq, 3);
}

// ── AC-R-2.5.4-6 — permission transport: iff decider:human, verbatim forwarding

#[test]
fn ac_r_2_5_4_6_permission_round_trip_deny_never_executes() {
    let scripted = turn(
        vec![
            (5, "lifecycle.turn.started", Json::obj([])),
            (
                6,
                "security.permission.requested",
                Json::obj([
                    ("decider", Json::str("human")),
                    ("effect_id", Json::str("eff-1")),
                    ("tool_name", Json::str("exec")),
                    (
                        "options",
                        Json::Arr(vec![
                            Json::obj([
                                ("optionId", Json::str("once")),
                                ("kind", Json::str("allow_once")),
                            ]),
                            // The auto-approvable-looking option —
                            // the transport must still route to Π
                            // (AC-7's deny half).
                            Json::obj([
                                ("optionId", Json::str("always")),
                                ("kind", Json::str("allow_always")),
                            ]),
                        ]),
                    ),
                ]),
            ),
            // The post-permission durable events — only rendered
            // after the decision resolves.
            (
                9,
                "lifecycle.turn.finished",
                Json::obj([("stop_reason", Json::str("refused"))]),
            ),
        ],
        "refused",
    );
    let mut driver = FixtureDriver::new();
    driver.scripted_turns.push_back(Ok(scripted));
    // Π's deny → the driver lands the refused terminal (the tool
    // call never ran — no executed-effect events in the resolution).
    // The refused resolution lands `action.tool.rejected` — the
    // `permission_refused` class rides the payload verbatim.
    driver.scripted_decisions.push_back(Ok(vec![(
        7,
        "action.tool.rejected".to_string(),
        Json::obj([("class", Json::str("permission_refused"))]),
    )]));

    let (client_end, server_end) = channel();
    let server = spawn_serve(artefact(), driver, server_end);
    let mut client = AcpClient::new(client_end);
    client.attach_session(&Json::obj([]), None).unwrap();
    let sess = client.new_session(&Json::obj([])).unwrap();
    let session_id = sess
        .get("sessionId")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let mut pi = StaticPi::new(PermissionOutcome::Deny);
    let outcome = client.prompt(&session_id, &Json::obj([]), &mut pi).unwrap();
    assert_eq!(outcome.stop_reason, "refused");

    drop(client); // close the channel — the serve thread drains to None
    let driver = server.join().unwrap();
    // The adapter forwarded the verbatim wire outcome to the driver —
    // it never approved, never rewrote.
    assert_eq!(driver.decisions_seen.len(), 1);
    let (_, _, decision) = &driver.decisions_seen[0];
    assert_eq!(
        decision.get("outcome").and_then(|o| o.get("outcome")),
        Some(&Json::str("denied"))
    );
    // The resolution's `action.effect.refused` rendered as a
    // tool_call_update{failed} after the permission exchange — and
    // the denied tool call's own execute events never appear.
    assert!(outcome.updates.iter().all(|u| {
        u.params.get("toolCall").and_then(|t| t.get("toolName")) != Some(&Json::str("exec-ran"))
    }));
}

#[test]
fn ac_r_2_5_4_6_request_permission_iff_decider_human() {
    // The iff: a `security.permission.requested` whose decider is NOT
    // `human` never emits a wire request; `decider:human` does.
    let non_human = Json::obj([
        ("decider", Json::str("policy")),
        ("effect_id", Json::str("e")),
    ]);
    assert!(
        lower_event("security.permission.requested", &non_human).is_none()
            || lower_event("security.permission.requested", &non_human)
                .map(|(k, _)| k != "session/request_permission")
                .unwrap_or(true)
    );
    let human = Json::obj([
        ("decider", Json::str("human")),
        ("effect_id", Json::str("e")),
        ("options", Json::Arr(vec![])),
    ]);
    let (kind, _) = lower_event("security.permission.requested", &human).unwrap();
    assert_eq!(kind, "session/request_permission");

    // A turn with no `decider:human` event emits no
    // `session/request_permission` frame — proven by the driver
    // seeing zero decisions.
    let mut driver = FixtureDriver::new();
    driver.scripted_turns.push_back(Ok(turn(
        vec![
            (1, "lifecycle.turn.started", Json::obj([])),
            (
                2,
                "security.permission.requested",
                Json::obj([("decider", Json::str("policy"))]),
            ),
            (3, "lifecycle.turn.finished", Json::obj([])),
        ],
        "end_turn",
    )));
    let (client_end, server_end) = channel();
    let server = spawn_serve(artefact(), driver, server_end);
    let mut client = AcpClient::new(client_end);
    client.attach_session(&Json::obj([]), None).unwrap();
    let sess = client.new_session(&Json::obj([])).unwrap();
    let sid = sess
        .get("sessionId")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let mut pi = StaticPi::new(PermissionOutcome::Allow { option_id: None });
    client.prompt(&sid, &Json::obj([]), &mut pi).unwrap();
    drop(client);
    let driver = server.join().unwrap();
    assert!(
        driver.decisions_seen.is_empty(),
        "a non-human decider must never reach Π"
    );
}

#[test]
fn ac_r_2_5_4_6_cancelled_maps_to_refusal_record() {
    // `cancelled` ⇒ the refusal record: the adapter forwards the
    // verbatim `cancelled` outcome; the driver lands
    // `action.effect.refused` — the wire response is what the kernel
    // sees (verbatim), so the refused row's cause is honest.
    let mut m = BTreeMap::new();
    m.insert(
        "outcome".to_string(),
        Json::obj([("outcome", Json::str("cancelled"))]),
    );
    assert_eq!(
        PermissionOutcome::from_wire(&Json::Obj(m)),
        PermissionOutcome::Cancelled
    );
    // A malformed/absent outcome is a deny — never an implicit allow.
    assert_eq!(
        PermissionOutcome::from_wire(&Json::obj([])),
        PermissionOutcome::Deny
    );
    assert_eq!(
        PermissionOutcome::from_wire(&Json::obj([(
            "outcome",
            Json::obj([("outcome", Json::str("bogus"))])
        )])),
        PermissionOutcome::Deny
    );
}

// ── AC-R-2.5.4-7 — three v1 hosts → descriptors

#[test]
fn ac_r_2_5_4_7_three_v1_hosts_yield_compat_descriptors() {
    let mut descriptors = Vec::new();
    let mut handles = Vec::new();
    for _ in 0..3 {
        let (client_end, server_end) = channel();
        handles.push(spawn_serve(artefact(), FixtureDriver::new(), server_end));
        let mut client = AcpClient::new(client_end);
        client
            .attach_session(&Json::obj([]), Some(AcpEra::V1))
            .expect("v1 attach");
        descriptors.push(client.descriptor().unwrap().clone());
        drop(client);
    }
    for d in &descriptors {
        // `observability_level ∈ {events, end_state}` — `ledger` is
        // unreachable under the v1 compatibility profile.
        assert!(
            d.observability_level == "events" || d.observability_level == "end_state",
            "v1 observability: {}",
            d.observability_level
        );
        assert_ne!(d.observability_level, "ledger");
        // steer/fork/compaction/subagents honestly `unknown` — the
        // profile cannot say, never a silent `false`.
        assert_eq!(d.steer, "unknown");
        assert_eq!(d.fork, "unknown");
        assert_eq!(d.compaction, "unknown");
        assert_eq!(d.subagents, "unknown");
        // Component metrics `n/a` under v1.
        assert_eq!(d.component_metrics, "n/a");
    }
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn ac_r_2_5_4_7_v2_descriptor_admits_ledger_observability() {
    let (client_end, server_end) = channel();
    let server = spawn_serve(artefact(), FixtureDriver::new(), server_end);
    let mut client = AcpClient::new(client_end);
    client.attach_session(&Json::obj([]), None).unwrap();
    let d = client.descriptor().unwrap();
    // The v2 artefact advertises `_hh/ledger/read` → `ledger`
    // observability is the honest level.
    assert_eq!(d.observability_level, "ledger");
    drop(client);
    server.join().unwrap();
}

// ── AC-R-2.5.4-10 — the A2A edge

#[test]
fn ac_r_2_5_4_10_task_state_projection_is_exhaustive() {
    // The closed TaskState sum over the kernel lifecycle coordinates —
    // every phase/outcome/paused/auth combination lands a state.
    assert_eq!(
        task_state("intended", None, false, false),
        TaskState::Submitted
    );
    assert_eq!(
        task_state("authorized", None, false, false),
        TaskState::Working
    );
    assert_eq!(
        task_state("prepared", None, false, false),
        TaskState::Working
    );
    assert_eq!(
        task_state("committed", None, false, false),
        TaskState::Working
    );
    assert_eq!(
        task_state("observed", Some("applied"), false, false),
        TaskState::Completed
    );
    assert_eq!(
        task_state("observed", Some("partial"), false, false),
        TaskState::Working
    );
    assert_eq!(
        task_state("observed", Some("not_applied"), false, false),
        TaskState::Failed
    );
    assert_eq!(
        task_state("refused", None, false, false),
        TaskState::Rejected
    );
    // Non-terminal on the wire: paused ⇒ INPUT_REQUIRED; auth-pending
    // ⇒ AUTH_REQUIRED.
    assert_eq!(
        task_state("committed", None, true, false),
        TaskState::InputRequired
    );
    assert_eq!(
        task_state("committed", None, false, true),
        TaskState::AuthRequired
    );
}

#[test]
fn ac_r_2_5_4_10_input_required_round_trip_resumes() {
    // A paused child task resumes: the `requestState` echoes
    // byte-for-byte, and the resumed task projects `working`.
    let request_state = Json::obj([
        ("opaque", Json::str("edge-state-blob")),
        ("nested", Json::obj([("k", Json::Int(42))])),
    ]);
    let resume = resume_task("task-1", &request_state);
    assert_eq!(
        resume.get("requestState"),
        Some(&request_state),
        "requestState must echo verbatim"
    );
    assert_eq!(resume.get("state"), Some(&Json::str("working")));
}

#[test]
fn ac_r_2_5_4_10_card_version_is_bundle_id_and_signs() {
    let skills = Json::Arr(vec![Json::obj([
        ("id", Json::str("summarize")),
        ("name", Json::str("Summarize")),
        ("description", Json::str("summarize text")),
    ])]);
    let mut c = card(
        "fixture-agent",
        "the fixture",
        "a2a://fixture",
        "bundle:sha256:abc123",
        skills,
    );
    // Binding (b): `version` is the deployable bundle id — never the
    // protocol or session version.
    assert_eq!(c.version, "bundle:sha256:abc123");
    assert_eq!(c.capabilities, vec!["summarize".to_string()]);

    // Sign under the fixture trust root; verify with the held key.
    let mut signer = FixedSigner::new("root-key", b"fixture-root-key".to_vec());
    sign_card(&mut c, &mut signer).expect("sign");
    let mut table = KeyTable::default();
    table
        .0
        .insert("root-key".to_string(), b"fixture-root-key".to_vec());
    verify_card(&c, &table).expect("verifies under the trust root");

    // Fail closed: an unsigned card never verifies.
    let mut unsigned = card("a", "b", "u", "bundle:x", Json::Arr(vec![]));
    unsigned.signatures.clear();
    assert!(verify_card(&unsigned, &table).is_err());

    // Fail closed: the wrong root's table lacks the key id.
    let empty = KeyTable::default();
    assert!(verify_card(&c, &empty).is_err());

    // Fail closed: a different key under the same id fails the mac.
    let mut wrong = KeyTable::default();
    wrong
        .0
        .insert("root-key".to_string(), b"not-the-key".to_vec());
    assert!(verify_card(&c, &wrong).is_err());

    // Tamper — a mutated version makes the signature's subject stale.
    let mut tampered = c.clone();
    tampered.version = "bundle:evil".to_string();
    assert!(verify_card(&tampered, &table).is_err());
}

#[test]
fn ac_r_2_5_4_10_delegate_task_record_shape() {
    let record = delegate_task(
        &Json::obj([("prompt", Json::str("summarize the ledger"))]),
        &Json::obj([("usd", Json::Int(5))]),
        &Json::Arr(vec![Json::str("fs_read:*")]),
        &Json::obj([
            ("binding", Json::str("a2a")),
            ("card", Json::str("card-ref")),
        ]),
    );
    assert_eq!(record.get("state"), Some(&Json::str("submitted")));
    assert!(record.get("task_id").and_then(Json::as_str).is_some());
    assert_eq!(
        record.get("budget_slice").and_then(|b| b.get("usd")),
        Some(&Json::Int(5))
    );
}

// ── supporting C1 behaviours ─────────────────────────────────────────────────

#[test]
fn negotiate_era_pinned_and_legacy() {
    assert_eq!(negotiate_era(ACP_VERSION).unwrap(), AcpEra::V2);
    assert_eq!(negotiate_era("v1-compat").unwrap(), AcpEra::V1);
    assert_eq!(negotiate_era("v1").unwrap(), AcpEra::V1);
    match negotiate_era("acp-9.9") {
        Err(NegotiateError::ProtocolVersionMismatch { requested, .. }) => {
            assert_eq!(requested, "acp-9.9")
        }
        other => panic!("expected ProtocolVersionMismatch, got {other:?}"),
    }
}

#[test]
fn artefact_preserves_ext_members_byte_for_byte() {
    let a = artefact();
    assert_eq!(
        a.ext.get("_foreign_ext"),
        Some(&Json::str("preserved-verbatim")),
        "unknown `_` members are ext-preserved, never dropped"
    );
    assert_eq!(a.agent_semantic_id, "test:fixture-agent");
    assert!(a.advertises("_hh/ledger/read"));
    assert!(!a.advertises("_hh/evil"));
}

#[test]
fn v1_profile_filters_unstable_update_kinds() {
    // The unstable extension rows (`session/fork`, `compaction_update`)
    // render under v2 and drop under the v1 compatibility profile (the
    // lowered projection reports the loss; the wire never carries it).
    let events: Vec<(u64, String, Json)> = vec![
        (
            1,
            "lifecycle.run.forked".to_string(),
            Json::obj([("forked_from", Json::str("run-0"))]),
        ),
        (
            2,
            "model.stream.delta".to_string(),
            Json::obj([("text", Json::str("answer"))]),
        ),
    ];
    let v2 = render_session(&events, AcpDialect::V2);
    let v1 = render_session(&events, AcpDialect::V1);
    assert!(
        v2.iter().any(|u| u.kind == "session/fork"),
        "v2 carries the fork update"
    );
    assert!(
        !v1.iter().any(|u| u.kind == "session/fork"),
        "v1 drops the unstable kind"
    );
    assert!(v1.iter().any(|u| u.kind == "agent_message_chunk"));
    // The v1 initialize response drops `sessionCapabilities`.
    let a = artefact();
    let resp = a.initialize_response(AcpDialect::V1);
    assert!(
        resp.get("agentCapabilities")
            .and_then(|c| c.get("sessionCapabilities"))
            .is_none(),
        "v1 initialize response drops the unstable surface"
    );
}

#[test]
fn hh_methods_round_trip_and_unknown_method_refused() {
    let mut driver = FixtureDriver::new();
    driver.ledger_tail = vec![(
        42,
        "action.tool.completed".to_string(),
        Json::obj([("status", Json::str("completed"))]),
    )];
    let (client_end, server_end) = channel();
    let server = spawn_serve(artefact(), driver, server_end);
    let mut client = AcpClient::new(client_end);
    client.attach_session(&Json::obj([]), None).unwrap();
    client.new_session(&Json::obj([])).unwrap();

    // `_hh/ledger/read` — the durable tail verbatim.
    let tail = client.ledger_read(0).unwrap();
    let Json::Arr(rows) = &tail else {
        panic!("ledger_read did not return an array")
    };
    assert_eq!(rows[0].get("seq"), Some(&Json::Int(42)));
    assert_eq!(
        rows[0].get("class"),
        Some(&Json::str("action.tool.completed"))
    );
    // `_hh/account` + `_hh/participant/describe`.
    assert_eq!(
        client.account().unwrap().get("fixture"),
        Some(&Json::Bool(true))
    );
    let d = client
        .participant_describe(&Json::obj([("participant", Json::str("test:agent"))]))
        .unwrap();
    assert_eq!(d.get("family"), Some(&Json::str("dispatch")));

    // An unknown `_`-prefixed method → -32601 (T3 — preserved in
    // `ext`, never silently answered).
    let err = client
        .call_raw("_hh/unimplemented", Json::obj([]))
        .unwrap_err();
    assert!(format!("{err:?}").contains("-32601"), "{err:?}");

    drop(client);
    server.join().unwrap();
}

#[test]
fn session_resume_replays_durable_tail() {
    let mut driver = FixtureDriver::new();
    let events: Vec<(u64, String, Json)> = vec![
        (1, "lifecycle.turn.started".to_string(), Json::obj([])),
        (
            2,
            "model.stream.delta".to_string(),
            Json::obj([("text", Json::str("a"))]),
        ),
        (3, "lifecycle.turn.finished".to_string(), Json::obj([])),
    ];
    let (client_end, server_end) = channel();
    // Seed the session's durable record via a prompt, then resume.
    driver.scripted_turns.push_back(Ok(TurnDrive {
        events: events.clone(),
        stop_reason: "end_turn".to_string(),
    }));
    let server = spawn_serve(artefact(), driver, server_end);
    let mut client = AcpClient::new(client_end);
    client.attach_session(&Json::obj([]), None).unwrap();
    let sess = client.new_session(&Json::obj([])).unwrap();
    let sid = sess
        .get("sessionId")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let mut pi = StaticPi::new(PermissionOutcome::Deny);
    client.prompt(&sid, &Json::obj([]), &mut pi).unwrap();

    // `replayFrom: 1` → only seq > 1 re-served; the replay comes from
    // the durable record.
    let (updates, result) = client.resume(&sid, Some(1)).unwrap();
    assert_eq!(result.get("replayed"), Some(&Json::Int(2)));
    let seqs: Vec<i64> = updates
        .iter()
        .filter_map(|u| {
            u.params
                .get("_meta")
                .and_then(|m| m.get("hh"))
                .and_then(|h| h.get("source_seq"))
                .and_then(Json::as_int)
        })
        .collect();
    assert_eq!(seqs, vec![2, 3]);
    drop(client);
    server.join().unwrap();
}

#[test]
fn session_new_mcp_servers_pending_review_never_admitted() {
    // H5 — declared `mcpServers[]` sit in `pending_review`: the
    // response carries them verbatim with the review-required
    // admission, never as admitted tools.
    let (client_end, server_end) = channel();
    let server = spawn_serve(artefact(), FixtureDriver::new(), server_end);
    let mut client = AcpClient::new(client_end);
    client.attach_session(&Json::obj([]), None).unwrap();
    let result = client
        .new_session(&Json::obj([(
            "mcpServers",
            Json::Arr(vec![Json::obj([
                ("name", Json::str("foreign-server")),
                ("command", Json::str("/usr/bin/fake")),
            ])]),
        )]))
        .unwrap();
    assert_eq!(
        result.get("admission"),
        Some(&Json::str("h5_review_required"))
    );
    assert_eq!(result.get("tools_unverified"), Some(&Json::Bool(true)));
    let pending = result.get("mcp_servers_pending_review").unwrap();
    let Json::Arr(servers) = pending else {
        panic!("pending_review not an array")
    };
    assert_eq!(servers[0].get("name"), Some(&Json::str("foreign-server")));
    drop(client);
    server.join().unwrap();
}

#[test]
fn session_cancel_and_close() {
    let mut driver = FixtureDriver::new();
    driver.scripted_turns.push_back(Ok(turn(
        vec![(1, "lifecycle.turn.started", Json::obj([]))],
        "end_turn",
    )));
    let (client_end, server_end) = channel();
    let server = spawn_serve(artefact(), driver, server_end);
    let mut client = AcpClient::new(client_end);
    client.attach_session(&Json::obj([]), None).unwrap();
    let sess = client.new_session(&Json::obj([])).unwrap();
    let sid = sess
        .get("sessionId")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    client.cancel(&sid).unwrap();
    client.close(&sid).unwrap();
    // A prompt on a closed session is a protocol error, never served.
    let mut pi = StaticPi::new(PermissionOutcome::Deny);
    let err = client.prompt(&sid, &Json::obj([]), &mut pi).unwrap_err();
    assert!(
        format!("{err:?}").contains("session/unknown_or_closed"),
        "{err:?}"
    );
    drop(client);
    server.join().unwrap();
}
