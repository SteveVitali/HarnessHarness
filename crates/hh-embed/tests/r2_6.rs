//! R2.6 — durable `queue_next_turn` steering at the `hh-embed/1`
//! boundary (DF-S2.11-1's durable half; the OQ-316 steer arm).
//!
//! The gap ADR-0273 named: a queued steer lived only in
//! `SessionState::pending_steer` — process memory. The durable record is
//! now the wakeup seam itself: `steer{mode: next_turn}` lazily binds a
//! `manual{holder}` + `delivery_mode = steer` subscription and mints a
//! `control.wakeup.occurred` row carrying the staged artefact's
//! `payload_ref`; the next `drive` fires it (`control.wakeup.fired`),
//! drains it into `Cue::Woken{delivery_mode: steer}`, and the bound
//! `react/steerable` interpreter answers `propose{steer_ref}` — the same
//! rows on either side of a process restart, because the fold replays
//! the ledger, never a session field.
//!
//! Fixture-only (the ticket's offline ceiling): scripted model,
//! `local_host` env, restart = `EmbedService::open` over the same
//! `store_root` + `open_session{kind:"resume", mode:"takeover"}`.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use hh_assembly::grammar::Assembly;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_embed_schema::types::AttendanceDeclaration;
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

// ── fixture: the conformance hir/1 document, bound to the queue variant ──

fn prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("test:author", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    )
}

fn sel(sid: &str) -> Ref {
    Ref::selected(sid, "latest")
}

fn sid(mut n: Node, id: &str) -> Node {
    n.version.semantic_id = Some(id.into());
    n
}

fn node(kind: EntityKind, rec: KindRecord, seq: u64) -> Node {
    Node::new(kind, rec, prov(seq))
}

/// The conformance document bound to `hh/react-steerable-queue` — the
/// R2.6-seeded `queue_next_turn` declaration (same `ReactSteerable`
/// interpreter, a different steer-mode record — the durable seam selects
/// on the declaration).
fn document_json_queue() -> Json {
    let rule = sid(
        node(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: "test:rule.rule".into(),
                trigger: Json::Null,
                action: RuleAction::RequestApproval(Json::Null),
                scope: Json::Null,
                conditioned_on: None,
                assumption_debt: None,
            }),
            1,
        ),
        "test:rule",
    );
    let budget = sid(
        node(
            EntityKind::Budget,
            KindRecord::Budget(BudgetRecord {
                dimensions: BTreeMap::from([(
                    "tokens.blended".to_string(),
                    DimensionBound {
                        hard: Some(1000),
                        soft: None,
                    },
                )]),
                scope: "*".into(),
                parent: None,
                accounting: sel("test:rule"),
            }),
            2,
        ),
        "test:budget",
    );
    let perm = sid(
        node(
            EntityKind::Permission,
            KindRecord::Permission(PermissionRecord {
                holder: sel("test:agent"),
                grants: vec![],
                issuer: Issuer {
                    authority: AuthorityClass::Kernel,
                    reference: "test:issuer".into(),
                },
                validity: Validity::open_from(0),
                revocation: None,
            }),
            3,
        ),
        "test:perm",
    );
    let agent = sid(
        node(
            EntityKind::AgentProcess,
            KindRecord::AgentProcess(AgentProcessRecord {
                body: AgentProcessBody::Native(NativeProcess {
                    harness_def: sel("test:agent"),
                    profile: ProfileRef {
                        profile: "sha256:profile".into(),
                        pinned: true,
                    },
                    slots: BTreeMap::new(),
                    control_boundary: Default::default(),
                    budget: sel("test:budget"),
                    permissions: sel("test:perm"),
                    environment: EnvironmentRef {
                        environment: "env:test".into(),
                    },
                }),
            }),
            4,
        ),
        "test:agent",
    );
    let mut assembly = Assembly::empty();
    assembly.slots.insert(
        "control_strategy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "control_strategy",
            "hh/react-steerable-queue",
            "latest",
        ))),
    );
    assembly.slots.insert(
        "context_policy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "context_policy",
            "hh/full_window",
            "latest",
        ))),
    );
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes = vec![rule, budget, perm, agent];
    doc.assembly = Some(assembly.to_json());
    doc.to_json()
}

// ── service helpers ─────────────────────────────────────────────────────────

fn test_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-embed-r26-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn service_in(root: &std::path::Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "r2.6".into(),
    })
    .unwrap()
}

/// `service_in` over a `ManualClock` — the restart-resume fixture steps
/// the clock past the writer-lease TTL so the takeover fences the dead
/// process's record by *expiry* (writer takeover is expiry-disciplined —
/// ADR-0130; a same-process `pid=` holder probes `Alive`, never `Dead`).
fn service_at(root: &std::path::Path, now_ms: u64) -> EmbedService {
    EmbedService::open_with(
        ServiceConfig {
            store_root: root.join("store"),
            kernel_version_id: "hh-kernel/0.1.0".into(),
            workspace_root: root.join("ws"),
            holder: "r2.6".into(),
        },
        Box::new(hh_ledger::ids::ManualClock::at(now_ms)),
        None,
    )
    .unwrap()
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
    svc.handle(&Request {
        id: Json::str(format!("t-{method}")),
        method: method.into(),
        params,
    })
}

fn ok(resp: &Json) -> Json {
    resp.get("result")
        .unwrap_or_else(|| panic!("expected result, got {}", resp.to_canonical_string()))
        .clone()
}

fn hello(svc: &mut EmbedService) {
    let mut caps = BTreeMap::new();
    caps.insert("experimental".to_string(), Json::Bool(true));
    caps.insert("serves_host_executor".to_string(), Json::Bool(true));
    caps.insert("serves_permission_channel".to_string(), Json::Bool(true));
    let r = call(
        svc,
        "hello",
        Json::obj(vec![
            ("contract_major", Json::Int(1)),
            (
                "client",
                Json::obj(vec![
                    ("name", Json::str("r2.6")),
                    ("version", Json::str("1")),
                    ("kind", Json::str("test")),
                ]),
            ),
            ("capabilities", Json::Obj(caps)),
        ]),
    );
    ok(&r);
}

fn text_input(t: &str) -> Vec<Json> {
    vec![Json::obj(vec![
        ("kind", Json::str("text")),
        ("text", Json::str(t)),
    ])]
}

/// `interactive` attendance + `model_calls:1` — the conformance battery's
/// park-mid-turn shape: the second model call exhausts, the interactive
/// escalation parks the loop at a decision point, which is exactly where
/// `steer`'s modes diverge. The post-steer drive is `amend{budget}` —
/// raising the ceiling wakes the parked escalation with a solvent run
/// (ADR-0216's OQ-468 interim op; under exhaustion every cue would answer
/// `escalate`, steer included).
fn interactive_queue_spec() -> Json {
    Json::obj(vec![
        ("kind", Json::str("new")),
        (
            "definition",
            Json::obj(vec![
                ("kind", Json::str("document")),
                ("document", document_json_queue()),
            ]),
        ),
        ("overrides", Json::Arr(vec![])),
        (
            "environment",
            Json::obj(vec![
                ("kind", Json::str("connection_info")),
                (
                    "connection_info",
                    Json::obj(vec![("class", Json::str("local_host"))]),
                ),
            ]),
        ),
        (
            "attendance",
            AttendanceDeclaration {
                value: "interactive".into(),
                source: "declared".into(),
            }
            .to_json(),
        ),
        (
            "budget",
            Json::obj(vec![
                ("kind", Json::str("node")),
                (
                    "node",
                    Json::obj(vec![(
                        "dimensions",
                        Json::obj(vec![(
                            "model_calls",
                            Json::obj(vec![("hard", Json::Int(1))]),
                        )]),
                    )]),
                ),
            ]),
        ),
    ])
}

/// `amend{target: "budget", value: {dimensions: {model_calls: n}}}` — the
/// attended-exhaustion wake: raises the ceiling, mints
/// `control.budget.amended`, and drives the parked loop (the queued steer
/// fires + drains inside that drive).
fn amend_budget(svc: &mut EmbedService, session: &str, n: i64) -> Json {
    call(
        svc,
        "amend",
        Json::obj(vec![
            ("session_id", Json::str(session)),
            ("target", Json::str("budget")),
            (
                "value",
                Json::obj(vec![(
                    "dimensions",
                    Json::obj(vec![("model_calls", Json::Int(n))]),
                )]),
            ),
            ("idempotency_key", Json::str("amend-1")),
        ]),
    )
}

/// Open the queue-mode session and park it mid-turn (the submit's staged
/// `host.exec.shell` invoke burns the single model call; the escalation
/// parks the loop).
fn parked_queue_session(svc: &mut EmbedService) -> (String, String) {
    let s = ok(&call(
        svc,
        "open_session",
        Json::obj(vec![
            ("spec", interactive_queue_spec()),
            ("idempotency_key", Json::str("open-q")),
        ]),
    ));
    let id = s
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let run_id = s.get("run_id").and_then(Json::as_str).unwrap().to_string();
    let r = call(
        svc,
        "submit",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            (
                "input",
                Json::Arr(vec![Json::obj(vec![
                    ("kind", Json::str("invoke")),
                    ("capability", Json::str("host.exec.shell")),
                ])]),
            ),
            ("idempotency_key", Json::str("invoke-1")),
        ]),
    );
    assert!(
        r.get("result").is_some(),
        "invoke submit: {}",
        r.to_canonical_string()
    );
    (id, run_id)
}

fn classes(svc: &EmbedService, run: &str) -> Vec<(u64, String, Json)> {
    svc.store()
        .envelopes(run)
        .unwrap()
        .iter()
        .map(|e| (e.seq, e.class.clone(), e.payload.clone()))
        .collect()
}

/// `steer{input}` → the steer occurrence's `payload_ref` (the staged
/// artefact id) — read off the durable `control.wakeup.occurred` row.
fn steer_payload_ref(svc: &EmbedService, run: &str) -> String {
    classes(svc, run)
        .iter()
        .find(|(_, c, _)| c == "control.wakeup.occurred")
        .and_then(|(_, _, p)| {
            p.get("payload_ref")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| panic!("no steer occurrence: {:?}", classes(svc, run)))
}

/// A `control.decision` whose `context_request.steer_ref` names `pref` —
/// the queued steer answered at a decision point (the ref, never the
/// bytes — I7).
fn steer_decision(svc: &EmbedService, run: &str, pref: &str) -> Option<u64> {
    classes(svc, run)
        .iter()
        .find(|(_, c, p)| {
            c == "control.decision"
                && p.get("context_request")
                    .and_then(|cr| cr.get("steer_ref"))
                    .and_then(Json::as_str)
                    == Some(pref)
        })
        .map(|(s, _, _)| *s)
}

// ── the cells ───────────────────────────────────────────────────────────────

/// `steer{queue_next_turn}` answers `next_turn` and writes the durable
/// queue — `scheduled{manual, steer}` + `occurred{payload_ref}` — while
/// nothing has fired yet (the steer waits for the next turn's decision
/// point, durable-before-visible).
#[test]
fn r2_6_queue_steer_mints_the_durable_occurrence() {
    let root = test_dir("queue-rows");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let (id, run) = parked_queue_session(&mut svc);

    let st_r = call(
        &mut svc,
        "steer",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            ("input", Json::Arr(text_input("steer the next turn"))),
            ("idempotency_key", Json::str("steer-1")),
        ]),
    );
    if st_r.get("result").is_none() {
        for (s, c, _) in classes(&svc, &run) {
            eprintln!("{s} {c}");
        }
    }
    let st = ok(&st_r);
    assert_eq!(
        st.get("queued_at").and_then(Json::as_str),
        Some("next_turn"),
        "steer result: {st:?}"
    );

    let evs = classes(&svc, &run);
    // The input is ledgered before admission — a ledger fact, never UI
    // state.
    assert!(
        evs.iter()
            .any(|(_, c, _)| c == "context.artefact.delivered"),
        "the staged input's delivery row"
    );
    // The durable steer subscription — `manual{holder}` + `steer`.
    let sched = evs
        .iter()
        .find(|(_, c, _)| c == "control.wakeup.scheduled")
        .map(|(_, _, p)| p.clone())
        .unwrap_or_else(|| panic!("no wakeup subscription: {evs:?}"));
    let sub = sched.get("subscription").cloned().unwrap_or(Json::Null);
    assert_eq!(
        sub.get("trigger")
            .and_then(|t| t.get("type"))
            .and_then(Json::as_str),
        Some("manual"),
        "the steer queue is a manual subscription: {sub:?}"
    );
    assert_eq!(
        sub.get("policy")
            .and_then(|p| p.get("delivery_mode"))
            .and_then(Json::as_str),
        Some("steer")
    );
    // The occurrence carries the staged artefact's ref.
    let pref = steer_payload_ref(&svc, &run);
    assert!(
        pref.starts_with("input-") || pref.starts_with("art-") || !pref.is_empty(),
        "payload_ref: {pref}"
    );
    // Nothing fired — the queue is *durable*, not yet delivered.
    assert!(
        !evs.iter().any(|(_, c, _)| c == "control.wakeup.fired"),
        "the steer must not fire before the next drive"
    );
}

/// The next drive delivers the queued steer through `wakeup_drain` →
/// `Cue::Woken{steer}` → `propose{steer_ref}` — a `control.decision`
/// audit row names the staged ref (never the bytes).
#[test]
fn r2_6_queued_steer_delivers_at_the_next_drive() {
    let root = test_dir("queue-deliver");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let (id, run) = parked_queue_session(&mut svc);
    ok(&call(
        &mut svc,
        "steer",
        Json::obj(vec![
            ("session_id", Json::str(id.clone())),
            ("input", Json::Arr(text_input("steer the next turn"))),
            ("idempotency_key", Json::str("steer-1")),
        ]),
    ));
    let pref = steer_payload_ref(&svc, &run);

    // The next drive — `amend` raises the `model_calls` ceiling (the run
    // is solvent again) and wakes the parked loop; the drive's wakeup
    // pass fires the queued steer and the drain submits
    // `Cue::Woken{steer}` ahead of the driver's next decision point.
    let r = amend_budget(&mut svc, &id, 8);
    assert!(
        r.get("result").is_some(),
        "amend budget: {}",
        r.to_canonical_string()
    );

    let evs = classes(&svc, &run);
    let fired = evs
        .iter()
        .find(|(_, c, _)| c == "control.wakeup.fired")
        .unwrap_or_else(|| panic!("the queued steer fired on the next drive: {evs:?}"));
    assert_eq!(
        fired.2.get("occurrence_key").and_then(Json::as_str),
        Some(format!("steer:{pref}").as_str()),
        "the fired row is the steer occurrence: {:?}",
        fired.2
    );
    let seq = steer_decision(&svc, &run, &pref);
    assert!(
        seq.is_some(),
        "the woken steer answered propose{{steer_ref: {pref}}}: {evs:?}"
    );
}

/// DF-S2.11-1's headline: the queued steer survives a service restart —
/// the `occurred` row is durable, the subscription is durable, and a
/// fresh `EmbedService` over the same store re-delivers through the same
/// `deliver_wakeup → drain → Cue::Woken{steer}` path (process memory was
/// never the record). The restart steps a `ManualClock` past the writer
/// TTL so the takeover fences the dead writer by expiry (the S5.8
/// restart fixture's discipline — a same-process `pid=` holder probes
/// alive, which is correct: it *is* alive).
#[test]
fn r2_6_queued_steer_survives_restart() {
    let root = test_dir("queue-restart");
    let run;
    let pref;
    {
        let mut svc = service_at(&root, 1_000);
        hello(&mut svc);
        let (id, r) = parked_queue_session(&mut svc);
        run = r;
        ok(&call(
            &mut svc,
            "steer",
            Json::obj(vec![
                ("session_id", Json::str(id)),
                ("input", Json::Arr(text_input("steer across the restart"))),
                ("idempotency_key", Json::str("steer-1")),
            ]),
        ));
        pref = steer_payload_ref(&svc, &run);
        // Durable before the crash: scheduled + occurred rows exist;
        // the steer is still pending (never fired).
        let evs = classes(&svc, &run);
        assert!(
            evs.iter().any(|(_, c, _)| c == "control.wakeup.occurred"),
            "the steer occurrence is durable before restart"
        );
        assert!(
            !evs.iter().any(|(_, c, _)| c == "control.wakeup.fired"),
            "the steer is pending, not delivered"
        );
        // `svc` drops here — every session field (the old
        // `pending_steer` included) is gone; the WAL is the record.
    }

    // The restarted service — same store_root, fresh session state; the
    // clock steps past the writer lease so `restore`'s takeover fences
    // the dead writer's record.
    let mut svc = service_at(&root, 1_000 + 120_001);
    hello(&mut svc);
    let s2 = ok(&call(
        &mut svc,
        "open_session",
        Json::obj(vec![
            (
                "spec",
                Json::obj(vec![
                    ("kind", Json::str("resume")),
                    ("run_id", Json::str(run.clone())),
                    ("mode", Json::str("takeover")),
                    ("cause", Json::str("wakeup")),
                ]),
            ),
            ("idempotency_key", Json::str("res-1")),
        ]),
    ));
    let sid2 = s2
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();

    // The resumed session's first drive fires the still-pending steer
    // occurrence (the durable row — no process memory carried it) and the
    // drained `Cue::Woken{steer}` reaches a decision point. `amend` both
    // re-solvates the run and wakes the parked loop.
    let r = amend_budget(&mut svc, &sid2, 8);
    assert!(
        r.get("result").is_some(),
        "post-restart amend: {}",
        r.to_canonical_string()
    );

    let evs = classes(&svc, &run);
    let fired = evs
        .iter()
        .find(|(_, c, _)| c == "control.wakeup.fired")
        .unwrap_or_else(|| panic!("the queued steer fired post-restart: {evs:?}"));
    assert_eq!(
        fired.2.get("occurrence_key").and_then(Json::as_str),
        Some(format!("steer:{pref}").as_str()),
        "the fired row is the steer occurrence: {:?}",
        fired.2
    );
    let occurred_seq = evs
        .iter()
        .find(|(_, c, _)| c == "control.wakeup.occurred")
        .map(|(s, _, _)| *s)
        .unwrap();
    // Durable-before-visible across the restart: `occurred` (pre-restart)
    // orders before `fired` (post-restart) — and the fired row still
    // carries the staged `payload_ref`, so the drained cue names the same
    // artefact the op minted. (The interpreter-side `propose{steer_ref}`
    // answer is `r2_6_queued_steer_delivers_at_the_next_drive`'s cell;
    // here the run may finish before the queued cue's own decision point
    // — delivery is the durable fact the restart owes.)
    assert!(occurred_seq < fired.0, "occurred@{occurred_seq} < fired");
    assert_eq!(
        fired.2.get("payload_ref").and_then(Json::as_str),
        Some(pref.as_str()),
        "the post-restart delivery carries the staged artefact ref"
    );
    let drained = svc.store().wakeup_drain(&run).unwrap_or_default();
    assert!(
        drained.is_empty()
            || drained
                .iter()
                .all(|w| w.delivery_mode == hh_ledger::wakeup::DeliveryMode::Steer),
        "only steer deliveries remain undrained post-run: {drained:?}"
    );
}

/// A queued steer is idempotent at the op seam — replayed
/// `idempotency_key` answers the recorded result without a second
/// occurrence row (the `duplicate_occurrence` skip is the audited leg,
/// but the op-level idem cache never reaches the ledger).
#[test]
fn r2_6_steer_idempotency_key_replays_the_answer() {
    let root = test_dir("queue-idem");
    let mut svc = service_in(&root);
    hello(&mut svc);
    let (id, run) = parked_queue_session(&mut svc);

    let params = Json::obj(vec![
        ("session_id", Json::str(id)),
        ("input", Json::Arr(text_input("steer once"))),
        ("idempotency_key", Json::str("steer-key")),
    ]);
    let r1 = ok(&call(&mut svc, "steer", params.clone()));
    let r2 = ok(&call(&mut svc, "steer", params));
    assert_eq!(r1, r2, "idempotent replay answers the recorded result");
    assert_eq!(
        classes(&svc, &run)
            .iter()
            .filter(|(_, c, _)| c == "control.wakeup.occurred")
            .count(),
        1,
        "one occurrence per staged steer — the op idem cache held"
    );
}
