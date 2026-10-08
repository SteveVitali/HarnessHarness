//! R2.14 embed-side acceptance — the exporter-as-subscriber seam
//! (DF-S1.14-4; §5h.1 §6; ADR-0061): a `ExportSubscriber` tails a live
//! `read(run)`, lowers the delta under its `SinkPolicy`, delivers through
//! the *real* `EgressMediator` + a real loopback fixture sink (the
//! fixture-verified ceiling — `LocalHttpTransport` over `127.0.0.1`,
//! never a live backend), and appends `measurement.export.delivered`
//! through `commit_kernel_row_for` on the run's persisted writer-lease
//! generation. A refused destination is an egress fact — no delivered row.
//!
//! The mediator construction mirrors `hh-env/tests/egress_mediation.rs`
//! verbatim — the same tokens/policy/broker machinery the dispatch path
//! uses; the subscriber is a participant with its own attribution.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use hh_containment::egress::ApprovalCache;
use hh_containment::policy::{
    kernel_default, AmendmentBasis, ContainmentPolicy, DefaultUnmatched, EgressRule, HostPattern,
    NetMode, NonPublic, RuleDecision,
};
use hh_embed::subscriber::{ExportSubscriber, PollOutcome, SinkDestination, SubscriberEgress};
use hh_env::egress::{EgressMediator, LocalHttpTransport, StaticResolver};
use hh_env::events::{intended_payload, EventMinter, ScopeChain};
use hh_env::tokens::{TokenMinter, TokenResolver};
use hh_ledger::event::Scope;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_ontology::risk::{RepeatSafety, RiskClass, RiskReversibility, RiskScope};
use hh_provenance::PersistenceScope;
use hh_secrets::{CredentialBroker, DenyAllResolver};
use hh_telemetry::sinks::SinkPolicy;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-embed-r214-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease) {
    let mut s = Store::open_test(dir(tag), 1_000).unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

fn allow_rule(host: &str) -> EgressRule {
    EgressRule {
        host: HostPattern::parse(host).unwrap(),
        ports: vec![],
        protocols: vec![],
        // `methods_default` is {GET, HEAD, OPTIONS} — the sink's
        // delivery is a POST, so the allow rule widens it (§5g.4 §3's
        // per-extent widening).
        methods: vec!["POST".into()],
        decision: RuleDecision::Allow,
        credential_bindings: vec![],
        justification: None,
        provenance: None,
    }
}

fn mediated_policy(rules: Vec<EgressRule>) -> ContainmentPolicy {
    let mut p = kernel_default(0);
    p.net.mode = NetMode::Mediated;
    p.net.rules = rules;
    p.net.default_unmatched = DefaultUnmatched::Deny;
    p.net.non_public_destinations = NonPublic::AllowListed;
    p.amendment.allowed_bases = BTreeSet::from([AmendmentBasis::Approval]);
    p.amendment.session_cache = true;
    p.amendment.persist_scope_ceiling = PersistenceScope::Run;
    p.compute_ids();
    p
}

fn chain() -> ScopeChain {
    ScopeChain {
        turn_id: "turn-1".into(),
        model_call_id: "mc-1".into(),
        tool_call_id: "tc-1".into(),
    }
}

/// Open the scope members the mediated/exported rows name — `turn-1 ⊃
/// mc-1 ⊃ tc-1` plus the subscriber's own `action.effect.intended`
/// (the export is itself an effect — its egress is attributed).
/// Returns `(effect_id, token, resolver)` — the resolver shares the
/// minting table (`Rc` inside — a second minter resolves nothing).
fn open_scopes(store: &mut Store, run: &str, lease: &Lease) -> (String, String, TokenResolver) {
    let m = EventMinter::new(store, run);
    let mut t = m.mint("lifecycle.turn.started", Json::obj([])).unwrap();
    t.scope.turn_id = Some("turn-1".into());
    let mut c = m.mint("model.call.requested", Json::obj([])).unwrap();
    c.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some("mc-1".into()),
        ..Scope::default()
    };
    let mut p = m.mint("action.tool.proposed", Json::obj([])).unwrap();
    p.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some("mc-1".into()),
        tool_call_id: Some("tc-1".into()),
        ..Scope::default()
    };
    let risk = RiskClass {
        reversibility: RiskReversibility::Reversible,
        repeat_safety: RepeatSafety::Idempotent,
        scope: RiskScope::External,
    };
    let eid = Store::effect_id(run, "mc-1", "tc-1", 0);
    let e = m
        .mint_effect(
            "action.effect.intended",
            intended_payload(&risk, Some(&risk), "v-cap", "h", 0),
            &eid,
            &chain(),
        )
        .unwrap();
    store.append(run, lease, vec![t, c, p, e]).unwrap();
    let mut tm = TokenMinter::new([7u8; 32]);
    let tok = tm.mint(&eid, 1, "env-1");
    (eid, tok.token, tm.resolver())
}

/// The loopback fixture sink — captures the raw POST, answers 200.
/// Returns (port, captured).
fn fixture_sink() -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let cap = Arc::clone(&captured);
    thread::spawn(move || {
        listener.set_nonblocking(true).ok();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut served = 0;
        while served < 8 && std::time::Instant::now() < deadline {
            let Ok((mut s, _)) = listener.accept() else {
                thread::sleep(std::time::Duration::from_millis(5));
                continue;
            };
            served += 1;
            s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .ok();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                match s.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&chunk[..n]);
                        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            cap.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buf).to_string());
            let _ =
                s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        }
    });
    (port, captured)
}

fn class_count(store: &Store, run_id: &str, class: &str) -> usize {
    store
        .events(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == class)
        .count()
}

#[test]
fn subscriber_tails_read_delivers_through_egress_and_appends_delivered() {
    let (mut store, run, lease) = open("sub");
    let (eid, token, tokens) = open_scopes(&mut store, &run, &lease);
    let (port, captured) = fixture_sink();

    let mut broker = CredentialBroker::new(Box::new(DenyAllResolver), "kernel-egress/x");
    let mut med = EgressMediator {
        store: &mut store,
        run_id: run.clone(),
        lease: &lease,
        tokens,
        broker: &mut broker,
        policy: mediated_policy(vec![allow_rule("127.0.0.1")]),
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "agent.exporter".into(),
        resolver: Box::new(StaticResolver {
            map: BTreeMap::from([("127.0.0.1".to_string(), vec!["127.0.0.1".parse().unwrap()])]),
        }),
        transport: Box::new(LocalHttpTransport::default()),
        inspect_hooks: BTreeMap::new(),
    };

    let mut sub = ExportSubscriber {
        run_id: run.clone(),
        policy: SinkPolicy::accounting("sink-fixture"),
        consents: BTreeSet::new(),
        watermark: 0,
        destination: SinkDestination {
            host: "127.0.0.1".into(),
            port,
            path: "/v1/spans".into(),
        },
        egress: SubscriberEgress {
            token,
            effect_id: eid,
            env_handle: "env-1".into(),
            tool_call_id: "tc-1".into(),
        },
    };

    // First poll — the scope-open prefix (run-open + 4 scope rows) is the
    // delta; the batch forwards through the mediator to the fixture sink.
    let out = sub.poll(&mut med, &chain()).unwrap();
    let delivered = match out {
        PollOutcome::Delivered { seq, seq_range, .. } => {
            assert!(seq_range.1 >= seq_range.0);
            seq
        }
        other => panic!("expected Delivered, got {other:?}"),
    };
    assert!(sub.watermark > 0);

    // The fixture sink received the canonical batch over the real wire.
    let got = captured.lock().unwrap();
    assert_eq!(got.len(), 1, "exactly one POST crossed the loopback wire");
    assert!(got[0].contains("POST /v1/spans"), "the sink's path");
    assert!(got[0].contains("\"event_id\""), "accounting rows crossed");
    drop(got);

    // The durable record: `security.egress.decided{allow}` +
    // `measurement.export.delivered` — both committed.
    let store = &*med.store;
    let delivered_rows: Vec<_> = store
        .events(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "measurement.export.delivered")
        .cloned()
        .collect();
    assert_eq!(delivered_rows.len(), 1);
    assert_eq!(delivered_rows[0].seq, delivered);
    assert_eq!(
        delivered_rows[0]
            .payload
            .get("sink_id")
            .and_then(Json::as_str),
        Some("sink-fixture")
    );
    assert!(class_count(store, &run, "security.egress.decided") >= 1);

    // Second poll — the egress/delivered rows themselves are durable, so
    // the tail picks them up (the subscriber exports its own lifecycle
    // rows like any other durable fact). A third poll is Quiet.
    let out2 = sub.poll(&mut med, &chain()).unwrap();
    assert!(matches!(
        out2,
        PollOutcome::Delivered { .. } | PollOutcome::Refused { .. }
    ));
    let out3 = sub.poll(&mut med, &chain()).unwrap();
    assert!(matches!(
        out3,
        PollOutcome::Quiet | PollOutcome::Delivered { .. }
    ));
}

#[test]
fn unlisted_sink_is_an_egress_refusal_never_a_delivered_row() {
    let (mut store, run, lease) = open("sub-deny");
    let (eid, token, tokens) = open_scopes(&mut store, &run, &lease);

    let mut broker = CredentialBroker::new(Box::new(DenyAllResolver), "kernel-egress/x");
    let mut med = EgressMediator {
        store: &mut store,
        run_id: run.clone(),
        lease: &lease,
        tokens,
        broker: &mut broker,
        // No allow rule for the sink host — `default_unmatched: deny`.
        policy: mediated_policy(vec![]),
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "agent.exporter".into(),
        resolver: Box::new(StaticResolver {
            map: BTreeMap::new(),
        }),
        transport: Box::new(LocalHttpTransport::default()),
        inspect_hooks: BTreeMap::new(),
    };

    let mut sub = ExportSubscriber {
        run_id: run.clone(),
        policy: SinkPolicy::accounting("sink-unlisted"),
        consents: BTreeSet::new(),
        watermark: 0,
        destination: SinkDestination {
            host: "sink.example".into(),
            port: 443,
            path: "/v1/spans".into(),
        },
        egress: SubscriberEgress {
            token,
            effect_id: eid,
            env_handle: "env-1".into(),
            tool_call_id: "tc-1".into(),
        },
    };

    let out = sub.poll(&mut med, &chain()).unwrap();
    match out {
        PollOutcome::Refused {
            decided_ref,
            detail,
        } => {
            assert!(detail.contains("egress denied"), "{detail}");
            assert!(!decided_ref.is_empty());
        }
        other => panic!("expected Refused, got {other:?}"),
    }
    // An unlisted sink is an egress event, not a telemetry event — the
    // `security.egress.decided{deny}` row is durable; no delivered row.
    assert_eq!(
        class_count(med.store, &run, "measurement.export.delivered"),
        0
    );
    assert!(class_count(med.store, &run, "security.egress.decided") >= 1);
}
