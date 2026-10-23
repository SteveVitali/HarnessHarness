//! R4.7c live leg `otel-live` (ADR-0346 revisit trigger (a); BL-31;
//! R-2.9.1; ADR-0363 gated test): exports a short local run's
//! `gen_ai`-lowered convention spans as an OTLP/HTTP
//! `ExportTraceServiceRequest` through the *real* `EgressMediator` to
//! the operator-provisioned loopback collector (HUMAN-H6 —
//! `jaegertracing/jaeger:2.22.0`, `127.0.0.1:4318`; the leg runner
//! starts/queries/stops the container). `#[ignore]`d — it runs only
//! under `scripts/live-verify.sh otel-live`.
//!
//! Markers the leg parses from `--nocapture` output:
//!   * `LIVE_RUN_ID: <id>` — the exported run;
//!   * `LIVE_TRACE_ID: <hex32>` — the run's derived W3C trace ID the
//!     collector must answer for `GET :16686/api/traces/{id}`;
//!   * `LIVE_DELIVERED_SEQ: <seq>` — the durable
//!     `measurement.export.delivered` row (ack-gated).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_containment::egress::ApprovalCache;
use hh_containment::policy::{
    kernel_default, AmendmentBasis, DefaultUnmatched, EgressRule, HostPattern, NetMode, NonPublic,
    RuleDecision,
};
use hh_embed::subscriber::{
    ExportSubscriber, PollOutcome, SinkDestination, SinkWire, SubscriberEgress,
};
use hh_env::egress::{EgressMediator, LocalHttpTransport, StaticResolver};
use hh_env::events::{intended_payload, EventMinter, ScopeChain};
use hh_env::tokens::TokenMinter;
use hh_ledger::event::Scope;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::Store;
use hh_ontology::risk::{RepeatSafety, RiskClass, RiskReversibility, RiskScope};
use hh_provenance::PersistenceScope;
use hh_secrets::{CredentialBroker, DenyAllResolver};
use hh_telemetry::propagation::trace_id;
use hh_telemetry::sinks::SinkPolicy;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!(
        "hh-embed-r47c-live-{}-{tag}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn allow_rule(host: &str) -> EgressRule {
    EgressRule {
        host: HostPattern::parse(host).unwrap(),
        ports: vec![],
        protocols: vec![],
        methods: vec!["POST".into()],
        decision: RuleDecision::Allow,
        credential_bindings: vec![],
        justification: None,
        provenance: None,
    }
}

fn chain() -> ScopeChain {
    ScopeChain {
        turn_id: "turn-1".into(),
        model_call_id: "mc-eg".into(),
        tool_call_id: "tc-eg".into(),
    }
}

/// `r47c_live_otlp_export` — one real `POST /v1/traces` through the
/// mediated egress to the running collector; the durable
/// `measurement.export.delivered` row is the ack-gated record and the
/// run's derived trace ID is the read-back key.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh otel-live"]
fn r47c_live_otlp_export() {
    let mut s = Store::open_test(dir("otel-live"), 1_000).unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    let m = EventMinter::new(&s, &run);
    // The exported call — `model.call.requested` + terminal under
    // `mc-1` (the span's `gen_ai.chat`); the export effect rides its
    // own `mc-eg`/`tc-eg` scope (stays open).
    let mut t = m.mint("lifecycle.turn.started", Json::obj([])).unwrap();
    t.scope.turn_id = Some("turn-1".into());
    let mut c = m.mint("model.call.requested", Json::obj([])).unwrap();
    c.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some("mc-1".into()),
        ..Scope::default()
    };
    let mut done = m
        .mint(
            "model.call.completed",
            Json::obj([
                ("model_call_id", Json::str("mc-1")),
                ("served_model", Json::str("m-live")),
                ("stop_reason", Json::str("stop")),
                (
                    "usage",
                    Json::obj([(
                        "record",
                        Json::obj([(
                            "view",
                            Json::obj([
                                ("input_total", Json::Int(42)),
                                ("output_total", Json::Int(7)),
                            ]),
                        )]),
                    )]),
                ),
                ("timing", Json::obj([("latency_ms", Json::Int(88))])),
            ]),
        )
        .unwrap();
    done.scope = c.scope.clone();
    let mut c2 = m.mint("model.call.requested", Json::obj([])).unwrap();
    c2.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some("mc-eg".into()),
        ..Scope::default()
    };
    let mut p = m.mint("action.tool.proposed", Json::obj([])).unwrap();
    p.scope = Scope {
        turn_id: Some("turn-1".into()),
        model_call_id: Some("mc-eg".into()),
        tool_call_id: Some("tc-eg".into()),
        ..Scope::default()
    };
    let risk = RiskClass {
        reversibility: RiskReversibility::Reversible,
        repeat_safety: RepeatSafety::Idempotent,
        scope: RiskScope::External,
    };
    let eid = Store::effect_id(&run, "mc-eg", "tc-eg", 0);
    let e = m
        .mint_effect(
            "action.effect.intended",
            intended_payload(&risk, Some(&risk), "v-cap", "h", 0),
            &eid,
            &chain(),
        )
        .unwrap();
    s.append(&run, &lease, vec![t, c, done, c2, p, e]).unwrap();
    let mut tm = TokenMinter::new([7u8; 32]);
    let tok = tm.mint(&eid, 1, "env-1");

    let port: u16 = std::env::var("HH_OTEL_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4318);
    let mut broker = CredentialBroker::new(Box::new(DenyAllResolver), "kernel-egress/x");
    let mut policy = kernel_default(0);
    policy.net.mode = NetMode::Mediated;
    policy.net.rules = vec![allow_rule("127.0.0.1")];
    policy.net.default_unmatched = DefaultUnmatched::Deny;
    policy.net.non_public_destinations = NonPublic::AllowListed;
    policy.amendment.allowed_bases = BTreeSet::from([AmendmentBasis::Approval]);
    policy.amendment.session_cache = true;
    policy.amendment.persist_scope_ceiling = PersistenceScope::Run;
    policy.compute_ids();
    let mut med = EgressMediator {
        store: &mut s,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm.resolver(),
        broker: &mut broker,
        policy,
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
        policy: SinkPolicy::accounting("sink-otel-live"),
        consents: BTreeSet::new(),
        watermark: 0,
        destination: SinkDestination {
            host: "127.0.0.1".into(),
            port,
            path: "/v1/traces".into(),
            wire: SinkWire::OtlpTraces {
                service_name: "hh.exporter.live".into(),
            },
        },
        egress: SubscriberEgress {
            token: tok.token,
            effect_id: eid,
            env_handle: "env-1".into(),
            tool_call_id: "tc-eg".into(),
        },
    };

    let out = sub.poll(&mut med, &chain()).expect("poll");
    let seq = match out {
        PollOutcome::Delivered { seq, .. } => seq,
        other => panic!("expected Delivered, got {other:?}"),
    };
    // The durable claim — ack-gated: the row exists only because the
    // collector answered 2xx.
    assert_eq!(
        med.store
            .events(&run)
            .unwrap()
            .iter()
            .filter(|e| e.class == "measurement.export.delivered")
            .count(),
        1,
        "one ack-gated delivery row"
    );
    println!("LIVE_RUN_ID: {run}");
    println!("LIVE_TRACE_ID: {}", trace_id(&run));
    println!("LIVE_DELIVERED_SEQ: {seq}");
}
