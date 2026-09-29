//! S2.4 acceptance battery — the egress mediator + credential broker against
//! the real store, the real `decide_egress`, and a real loopback fixture
//! (LT-02/LT-04/LT-05/LT-06/LT-09; AC-R-2.8.3-{2,4,5,6,9},
//! AC-R-2.8.4-{2,4,6,7}; ADR-0266). Every test names the invariant(s) it pins;
//! each fails if the behaviour it covers is removed.
//!
//! Two transports are used:
//! - `CaptureTransport` — records the substituted request, never opens a
//!   socket (the decision/credential/ledger assertions).
//! - `LocalHttpTransport` against a real `TcpListener` on 127.0.0.1 — the one
//!   test that proves the *real carrier* crosses a real wire (LT-02).

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use hh_budget::account::Account;
use hh_budget::spec::{BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec};
use hh_containment::egress::{ApprovalCache, EgressRequest};
use hh_containment::policy::{
    kernel_default, AmendmentBasis, ContainmentPolicy, DefaultUnmatched, EgressProtocol,
    EgressRule, HostPattern, NetMode, NonPublic, RuleDecision,
};
use hh_env::egress::{
    EgressMediator, EgressTransport, MediatedOutcome, StaticResolver, WireResponse,
};
use hh_env::events::{intended_payload, EventMinter, ScopeChain};
use hh_env::tokens::TokenMinter;
use hh_hir::records::GrantConstraints;
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_monitor::approval::{ApprovalResponse, EndorserRef, LeaseSpec, ResponseChoice};
use hh_monitor::assess::SecretTransport;
use hh_monitor::decision::DecisionScope;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_ontology::risk::{
    RepeatSafety as RiskRepeatSafety, RiskClass, RiskReversibility, RiskScope,
};
use hh_provenance::{AuthorityClass, PersistenceScope, ProvenanceRecord};
use hh_secrets::*;
use hh_wire::json::Json;

// ── scaffold ─────────────────────────────────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-s24-{}-{tag}-{n}", std::process::id()));
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

fn k_ev(store: &Store, run_id: &str, id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: id.into(),
        class: class.into(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("kernel:test", store.now_ms())),
        content_kind: None,
        payload,
    }
}

/// The `security.permission.granted` row the broker's covering-decision fold
/// needs (holder granted `secret_access` on the channel's grant scope).
fn grant_handle(
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    holder: &str,
    channel_id: &str,
    handle_id: &str,
) {
    let issuer = ProvenanceRecord::kernel("kernel:test", store.now_ms());
    let payload = Json::obj([
        ("handle_id", Json::str(handle_id)),
        (
            "permission_ref",
            Json::obj([
                ("semantic_id", Json::str("perm.secrets")),
                (
                    "version_id",
                    Json::str("sha256:".to_string() + &"0".repeat(64)),
                ),
            ]),
        ),
        ("holder", Json::str(holder)),
        ("issuer", issuer.to_json()),
        (
            "grants",
            Json::Arr(vec![Json::obj([
                (
                    "effect",
                    Json::obj([("domain", Json::str("secret_access"))]),
                ),
                ("scope", Json::str(SecretChannel::grant_scope(channel_id))),
                ("constraints", Json::obj([])),
                ("delegable", Json::Bool(false)),
            ])]),
        ),
        ("ceiling", Json::str("principal")),
        (
            "validity",
            Json::obj([
                ("issued_at", Json::str(store.ts_now())),
                ("expires_at", Json::Null),
            ]),
        ),
        ("parent_handle", Json::Null),
        ("delegable", Json::Bool(false)),
        ("origin_basis", Json::str("approval")),
        ("basis_ref", Json::str("perm.secrets")),
        ("budget_ref", Json::Null),
        ("authority_delta", Json::str("none")),
    ]);
    let ev = k_ev(
        store,
        run_id,
        handle_id,
        "security.permission.granted",
        payload,
    );
    store.append(run_id, lease, vec![ev]).unwrap();
}

/// A `security.permission.decided{allow}` row naming `handle_id` — the PDP
/// record the binding's `monitor_decision_ref` points at.
fn decide_allow(
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    effect_id: &str,
    handle_id: &str,
) -> String {
    let decided_id = store.alloc_id("evt");
    let payload = Json::obj([
        ("effect_id", Json::str(effect_id)),
        ("decision", Json::str("allow")),
        ("effective_authority", Json::str("principal")),
        (
            "effective_risk_class",
            Json::obj([
                ("reversibility", Json::str("reversible")),
                ("repeat_safety", Json::str("idempotent")),
                ("scope", Json::str("ephemeral")),
            ]),
        ),
        ("handle_ids", Json::Arr(vec![Json::str(handle_id)])),
        ("policy_ref", Json::str("pi/1")),
        ("decider", Json::str("policy")),
        ("attempt_no", Json::Int(1)),
        ("proposal", Json::str("prop-1")),
        ("taint", Json::Arr(vec![])),
    ]);
    let ev = k_ev(
        store,
        run_id,
        &decided_id,
        "security.permission.decided",
        payload,
    );
    store.append(run_id, lease, vec![ev]).unwrap();
    decided_id
}

// ── policy / channel builders ────────────────────────────────────────────────

fn allow_rule(host: &str) -> EgressRule {
    EgressRule {
        host: HostPattern::parse(host).unwrap(),
        ports: vec![],
        protocols: vec![],
        methods: vec![],
        decision: RuleDecision::Allow,
        credential_bindings: vec![],
        justification: None,
        provenance: None,
    }
}

fn allow_rule_with_methods(host: &str, methods: &[&str]) -> EgressRule {
    let mut r = allow_rule(host);
    r.methods = methods.iter().map(|m| m.to_string()).collect();
    r
}

/// A mediated policy over `kernel_default` — `session_cache` on, approvals an
/// allowed amendment basis, `Run` the persistence ceiling.
fn mediated_policy(
    rules: Vec<EgressRule>,
    unmatched: DefaultUnmatched,
    non_public: NonPublic,
) -> ContainmentPolicy {
    let mut p = kernel_default(0);
    p.net.mode = NetMode::Mediated;
    p.net.rules = rules;
    p.net.default_unmatched = unmatched;
    p.net.non_public_destinations = non_public;
    p.amendment.allowed_bases = BTreeSet::from([AmendmentBasis::Approval]);
    p.amendment.session_cache = true;
    p.amendment.persist_scope_ceiling = PersistenceScope::Run;
    p.compute_ids();
    p
}

fn channel_spec(
    host: &str,
    port: u16,
    env_name: &str,
    revoke_path: Option<&str>,
) -> SecretChannelSpec {
    SecretChannelSpec {
        kind: CredentialKind::Bearer,
        source: SecretSource::OperatorVault {
            vault_ref: format!("vault:{host}"),
        },
        destinations: vec![DestinationBinding {
            scheme: "http".into(),
            host_pattern: host.into(),
            port: Some(port),
            path_prefix: None,
            auth_carrier: AuthCarrier::Header {
                name: "Authorization".into(),
                prefix: Some("Bearer ".into()),
            },
            revocation_path: revoke_path.map(str::to_string),
        }],
        allowed_env_names: Some(BTreeSet::from([env_name.to_string()])),
        delivery_modes: BTreeSet::from([SecretTransport::ProxyInjected]),
        max_lifetime_ms: None,
        rotation_policy: None,
        sender_constraint: SenderConstraint::None,
        constraints: GrantConstraints::default(),
        bindable: true,
        access_class: AccessClass::Broker,
        canary: false,
        description: "test channel".into(),
    }
}

fn broker_with(values: &[(&str, &str)]) -> CredentialBroker {
    let mut vault = StaticVault::default();
    for (coord, value) in values {
        vault.put(
            &SecretSource::OperatorVault {
                vault_ref: coord.to_string(),
            },
            *value,
        );
    }
    CredentialBroker::new(Box::new(vault), "test-fp-key")
}

/// Register + grant + decide + bind — returns the live binding.
fn bound_binding(
    broker: &mut CredentialBroker,
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    channel_id: &str,
    env_handle: &str,
    destinations: &[&str],
) -> CredentialBinding {
    let handle_id = store.alloc_id("hnd");
    grant_handle(store, run_id, lease, "agent.main", channel_id, &handle_id);
    let dref = decide_allow(
        store,
        run_id,
        lease,
        &format!("eff-{handle_id}"),
        &handle_id,
    );
    broker
        .bind(
            store,
            run_id,
            lease,
            BindRequest {
                channel_id: channel_id.into(),
                holder: "agent.main".into(),
                env_handle_ref: env_handle.into(),
                env_isolation: hh_containment::policy::IsolationClass::ProcessSandbox,
                mode: SecretTransport::ProxyInjected,
                monitor_decision_ref: dref,
                destinations: destinations.iter().map(|d| d.to_string()).collect(),
            },
        )
        .unwrap()
}

// ── transports / resolvers ───────────────────────────────────────────────────

/// What the transport saw on the wire (after sentinel substitution).
#[derive(Debug, Clone)]
struct Captured {
    addr: IpAddr,
    port: u16,
    host: String,
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[derive(Clone, Default)]
struct CaptureTransport {
    log: Arc<Mutex<Vec<Captured>>>,
}

impl EgressTransport for CaptureTransport {
    fn forward(
        &self,
        addr: IpAddr,
        port: u16,
        host: &str,
        method: &str,
        path: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Result<WireResponse, String> {
        self.log.lock().unwrap().push(Captured {
            addr,
            port,
            host: host.to_string(),
            method: method.to_string(),
            path: path.to_string(),
            headers: headers.to_vec(),
            body: body.unwrap_or(&[]).to_vec(),
        });
        Ok(WireResponse {
            status: 200,
            headers: vec![],
            body: b"ok".to_vec(),
        })
    }
}

/// A real loopback HTTP fixture — accepts `n` connections, captures the raw
/// request bytes, answers `200 ok`. Returns (port, captured, join).
fn http_fixture(n: usize) -> (u16, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let cap = Arc::clone(&captured);
    let join = thread::spawn(move || {
        listener.set_nonblocking(true).ok();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut served = 0;
        while served < n && std::time::Instant::now() < deadline {
            let Ok((mut s, _)) = listener.accept() else {
                thread::sleep(std::time::Duration::from_millis(5));
                continue;
            };
            served += 1;
            s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .ok();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            // Read until the header terminator (+ any already-buffered body).
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
    (port, captured, join)
}

/// A resolver mapping loopback fixture hosts + public test hosts.
fn fixture_resolver(fixture_hosts: &[(&str, &str)]) -> StaticResolver {
    let mut map = BTreeMap::new();
    for (host, ip) in fixture_hosts {
        map.insert(host.to_string(), vec![ip.parse().unwrap()]);
    }
    StaticResolver { map }
}

fn minter() -> TokenMinter {
    TokenMinter::new([7u8; 32])
}

fn chain() -> ScopeChain {
    ScopeChain {
        turn_id: "turn-1".into(),
        model_call_id: "mc-1".into(),
        tool_call_id: "tc-1".into(),
    }
}

/// Open the scope members the mediated rows carry — `turn-1 ⊃ mc-1 ⊃ tc-1`
/// plus `n_effects` `action.effect.intended` openers under derived effect
/// ids (the ledger's scope discipline: a member an event names must already
/// be open; `intended`'s schema binds `scope.effect_id` to the derived id).
/// Returns the derived effect ids for the tests to mint tokens against.
fn open_scopes(store: &mut Store, run: &str, lease: &Lease, n_effects: usize) -> Vec<String> {
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
        repeat_safety: RiskRepeatSafety::Idempotent,
        scope: RiskScope::External,
    };
    let mut evs = vec![t, c, p];
    let mut ids = Vec::new();
    for i in 0..n_effects {
        let eid = Store::effect_id(run, "mc-1", "tc-1", i as u64);
        evs.push(
            m.mint_effect(
                "action.effect.intended",
                intended_payload(&risk, Some(&risk), "v-cap", "h", i as u64),
                &eid,
                &chain(),
            )
            .unwrap(),
        );
        ids.push(eid);
    }
    store.append(run, lease, evs).unwrap();
    ids
}

/// A request bearing `token` to `host:port` with an optional sentinel header.
#[allow(clippy::too_many_arguments)] // the arity is the request record's.
fn req(
    token: &str,
    effect_id: &str,
    env: &str,
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    sentinel: Option<&str>,
) -> EgressRequest {
    let headers = match sentinel {
        Some(s) => vec![("Authorization".to_string(), format!("Bearer {s}"))],
        None => vec![],
    };
    EgressRequest {
        token: token.to_string(),
        effect_id: Some(effect_id.to_string()),
        tool_call_id: format!("call-{effect_id}"),
        env_handle: env.to_string(),
        protocol: EgressProtocol::Http,
        host_raw: host.to_string(),
        resolved_addrs: vec![],
        port,
        method: Some(method.to_string()),
        path: Some(path.to_string()),
        headers,
        body: None,
        credential_sentinels: vec![],
    }
}

/// Count the run's committed events of `class`.
fn class_count(store: &Store, run_id: &str, class: &str) -> usize {
    store
        .events(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == class)
        .count()
}

/// The events of `class` whose payload satisfies `pred`.
fn class_where(
    store: &Store,
    run_id: &str,
    class: &str,
    pred: impl Fn(&Json) -> bool,
) -> Vec<hh_ledger::event::EventEnvelope> {
    store
        .events(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == class && pred(&e.payload))
        .cloned()
        .collect()
}

fn jstr(j: &Json, key: &str) -> Option<String> {
    j.get(key).and_then(|v| v.as_str().map(str::to_string))
}

/// A budget root with `network.calls` + `approvals.requested` caps.
fn open_budget(store: &mut Store, run_id: &str, lease: &Lease, calls: i64, asks: i64) -> String {
    let mut acct = Account::open(store, run_id).unwrap();
    acct.allocate(
        lease,
        None,
        BudgetScope {
            kind: BudgetScopeKind::AgentProcess,
            target: "proc:main".into(),
        },
        BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[
                (DimensionKey::Primary(DimensionId::NetworkCalls), calls),
                (DimensionKey::Primary(DimensionId::ApprovalsRequested), asks),
            ],
        ),
    )
    .unwrap()
}

// ── LT-02 ────────────────────────────────────────────────────────────────────

/// LT-02 (a): a bound destination receives the **real** carrier — the
/// placeholder is substituted kernel-side and the real value crosses a real
/// loopback wire exactly once (one `security.credential.used` row).
#[test]
fn lt02_bound_destination_receives_real_carrier_over_real_wire() {
    let (mut store, run, lease) = open("lt02-bound");
    let effs = open_scopes(&mut store, &run, &lease, 1);
    let (port, captured, join) = http_fixture(1);
    let secret_value = "gho_LIVE-VALUE-never-in-ledger";

    let mut broker = broker_with(&[("vault:127.0.0.1", secret_value)]);
    broker
        .register_channel(
            "github",
            channel_spec("127.0.0.1", port, "GH_TOKEN", None),
            ProvenanceRecord::kernel("kernel:test", 0),
        )
        .unwrap();
    let binding = bound_binding(
        &mut broker,
        &mut store,
        &run,
        &lease,
        "github",
        "env-1",
        &["127.0.0.1"],
    );
    let sentinel = broker
        .placeholder_for(&binding.binding_id)
        .unwrap()
        .spelling
        .clone();

    let mut tm = minter();
    let tok = tm.mint(&effs[0], 1, "env-1");
    let policy = mediated_policy(
        vec![allow_rule("127.0.0.1")],
        DefaultUnmatched::Deny,
        NonPublic::AllowListed,
    );
    let budget_id = open_budget(&mut store, &run, &lease, 10, 10);

    let mut med = EgressMediator {
        store: &mut store,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm.resolver(),
        broker: &mut broker,
        policy,
        cache: ApprovalCache::default(),
        budget_id: Some(budget_id),
        participant_ref: "agent.main".into(),
        resolver: Box::new(hh_env::egress::SystemResolver),
        transport: Box::new(hh_env::egress::LocalHttpTransport::default()),
    };

    let out = med
        .handle(
            &req(
                &tok.token,
                &effs[0],
                "env-1",
                "127.0.0.1",
                port,
                "GET",
                "/data",
                Some(&sentinel),
            ),
            &chain(),
        )
        .unwrap();
    let MediatedOutcome::Forwarded { decided_ref, .. } = out else {
        panic!("expected Forwarded, got {out:?}");
    };
    join.join().unwrap();

    // The real carrier crossed the wire; the sentinel never did.
    let got = captured.lock().unwrap();
    assert_eq!(got.len(), 1);
    assert!(got[0].contains(&format!("Authorization: Bearer {secret_value}")));
    assert!(!got[0].contains(&sentinel));

    // Exactly one `used` row for this authenticated outbound request.
    let used = class_where(med.store, &run, "security.credential.used", |p| {
        jstr(p, "binding_id").as_deref() == Some(binding.binding_id.as_str())
    });
    assert_eq!(used.len(), 1);

    // requested + decided{allow} durable; decided names the applied binding.
    assert!(class_count(med.store, &run, "security.egress.requested") >= 1);
    let decided = class_where(med.store, &run, "security.egress.decided", |p| {
        jstr(p, "decision").as_deref() == Some("allow")
    });
    assert_eq!(decided.len(), 1);
    assert_eq!(decided[0].event_id, decided_ref);

    // The network.calls charge exists and attributes to the decided row.
    assert_eq!(class_count(med.store, &run, "control.budget.consumed"), 1);

    // The value itself is on no durable surface.
    let det = DetectorSet::standard(med.broker.mask_set().unwrap());
    assert!(med
        .broker
        .leak_scan_run(med.store, &run, &det)
        .unwrap()
        .is_empty());
}

/// LT-02 (b): an unbound destination refuses `out_of_scope` — the policy
/// allowed the host but no binding names it (anti-laundering: a credential
/// bound to A is never delivered to B).
#[test]
fn lt02_unbound_destination_refuses_out_of_scope() {
    let (mut store, run, lease) = open("lt02-unbound");
    let effs = open_scopes(&mut store, &run, &lease, 1);
    let mut broker = broker_with(&[("vault:127.0.0.1", "tok-a")]);
    broker
        .register_channel(
            "github",
            channel_spec("127.0.0.1", 8443, "GH_TOKEN", None),
            ProvenanceRecord::kernel("kernel:test", 0),
        )
        .unwrap();
    let binding = bound_binding(
        &mut broker,
        &mut store,
        &run,
        &lease,
        "github",
        "env-1",
        &["127.0.0.1"],
    );
    let sentinel = broker
        .placeholder_for(&binding.binding_id)
        .unwrap()
        .spelling
        .clone();

    let mut tm = minter();
    let tok = tm.mint(&effs[0], 1, "env-1");
    // The policy allows `evil.example` (a *public* resolution — the
    // non-public guard passes) but the binding doesn't name it.
    let policy = mediated_policy(
        vec![allow_rule("evil.example")],
        DefaultUnmatched::Deny,
        NonPublic::AllowListed,
    );
    let transport = CaptureTransport::default();
    let mut med = EgressMediator {
        store: &mut store,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm.resolver(),
        broker: &mut broker,
        policy,
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "agent.main".into(),
        resolver: Box::new(fixture_resolver(&[("evil.example", "93.184.216.34")])),
        transport: Box::new(transport.clone()),
    };
    let out = med
        .handle(
            &req(
                &tok.token,
                &effs[0],
                "env-1",
                "evil.example",
                443,
                "GET",
                "/",
                Some(&sentinel),
            ),
            &chain(),
        )
        .unwrap();
    let MediatedOutcome::RefusedCredential { code, .. } = out else {
        panic!("expected RefusedCredential, got {out:?}");
    };
    assert_eq!(code, RefusedCode::OutOfScope);
    // The denied row is durable; nothing reached the wire.
    assert!(class_count(med.store, &run, "security.credential.denied") >= 1);
    assert!(transport.log.lock().unwrap().is_empty());
}

/// LT-02 (c): an ambiguous path refuses `ambiguous_path` — the path's `..`
/// is caught inside the credential mediation even though the destination is
/// bound.
#[test]
fn lt02_ambiguous_path_refuses() {
    let (mut store, run, lease) = open("lt02-ambig");
    let effs = open_scopes(&mut store, &run, &lease, 1);
    let mut broker = broker_with(&[("vault:127.0.0.1", "tok-a")]);
    broker
        .register_channel(
            "github",
            channel_spec("127.0.0.1", 8443, "GH_TOKEN", None),
            ProvenanceRecord::kernel("kernel:test", 0),
        )
        .unwrap();
    let binding = bound_binding(
        &mut broker,
        &mut store,
        &run,
        &lease,
        "github",
        "env-1",
        &["127.0.0.1"],
    );
    let sentinel = broker
        .placeholder_for(&binding.binding_id)
        .unwrap()
        .spelling
        .clone();

    let mut tm = minter();
    let tok = tm.mint(&effs[0], 1, "env-1");
    let policy = mediated_policy(
        vec![allow_rule("127.0.0.1")],
        DefaultUnmatched::Deny,
        NonPublic::AllowListed,
    );
    let transport = CaptureTransport::default();
    let mut med = EgressMediator {
        store: &mut store,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm.resolver(),
        broker: &mut broker,
        policy,
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "agent.main".into(),
        resolver: Box::new(fixture_resolver(&[("127.0.0.1", "127.0.0.1")])),
        transport: Box::new(transport.clone()),
    };
    let out = med
        .handle(
            &req(
                &tok.token,
                &effs[0],
                "env-1",
                "127.0.0.1",
                8443,
                "GET",
                "/a/../b",
                Some(&sentinel),
            ),
            &chain(),
        )
        .unwrap();
    let MediatedOutcome::RefusedCredential { code, .. } = out else {
        panic!("expected RefusedCredential, got {out:?}");
    };
    assert_eq!(code, RefusedCode::AmbiguousPath);
    assert!(transport.log.lock().unwrap().is_empty());
}

// ── LT-09 / attribution ──────────────────────────────────────────────────────

/// LT-09 (attribution half) + AC-R-2.8.4-4: an unattributed request is
/// denied before any rule evaluation — `unattributed`, durable, nothing on
/// the wire.
#[test]
fn lt09_unattributed_request_denied_fail_closed() {
    let (mut store, run, lease) = open("lt09-unattr");
    let effs = open_scopes(&mut store, &run, &lease, 1);
    let mut broker = broker_with(&[]);
    let policy = mediated_policy(
        vec![allow_rule("127.0.0.1")],
        DefaultUnmatched::Deny,
        NonPublic::AllowListed,
    );
    let transport = CaptureTransport::default();
    let tm = minter();
    let mut med = EgressMediator {
        store: &mut store,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm.resolver(),
        broker: &mut broker,
        policy,
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "agent.main".into(),
        resolver: Box::new(fixture_resolver(&[])),
        transport: Box::new(transport.clone()),
    };
    // A token the minter never minted.
    let out = med
        .handle(
            &req(
                "sha256:forged",
                &effs[0],
                "env-1",
                "127.0.0.1",
                8443,
                "GET",
                "/",
                None,
            ),
            &chain(),
        )
        .unwrap();
    let MediatedOutcome::Refused { reason, .. } = out else {
        panic!("expected Refused, got {out:?}");
    };
    assert_eq!(reason, hh_containment::egress::EgressReason::Unattributed);
    assert!(transport.log.lock().unwrap().is_empty());
    // requested + decided are durable even for the unattributed deny.
    assert_eq!(class_count(med.store, &run, "security.egress.requested"), 1);
    assert_eq!(class_count(med.store, &run, "security.egress.decided"), 1);

    // A real token bound to a *different* env handle is unattributed too.
    let mut tm2 = minter();
    let tok_other = tm2.mint(&effs[0], 1, "env-2");
    let mut med2 = EgressMediator {
        store: med.store,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm2.resolver(),
        broker: med.broker,
        policy: med.policy.clone(),
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "agent.main".into(),
        resolver: Box::new(fixture_resolver(&[])),
        transport: Box::new(transport.clone()),
    };
    let out2 = med2
        .handle(
            &req(
                &tok_other.token,
                &effs[0],
                "env-1",
                "127.0.0.1",
                8443,
                "GET",
                "/",
                None,
            ),
            &chain(),
        )
        .unwrap();
    assert!(matches!(out2, MediatedOutcome::Refused { .. }));
    assert!(transport.log.lock().unwrap().is_empty());
}

/// AC-R-2.8.4-2/6: `non_public_destinations = deny` refuses a loopback
/// destination even when an allow rule covers it; `allow_listed` + covering
/// rule forwards.
#[test]
fn non_public_guard_denies_then_allow_listed_forwards() {
    let (mut store, run, lease) = open("nonpub");
    let effs = open_scopes(&mut store, &run, &lease, 2);
    let mut broker = broker_with(&[]);
    let mut tm = minter();
    let tok = tm.mint(&effs[0], 1, "env-1");

    // Deny — loopback is non-public, the allow rule does not rescue it.
    let deny_policy = mediated_policy(
        vec![allow_rule("127.0.0.1")],
        DefaultUnmatched::Deny,
        NonPublic::Deny,
    );
    let transport = CaptureTransport::default();
    let mut med = EgressMediator {
        store: &mut store,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm.resolver(),
        broker: &mut broker,
        policy: deny_policy,
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "agent.main".into(),
        resolver: Box::new(fixture_resolver(&[("127.0.0.1", "127.0.0.1")])),
        transport: Box::new(transport.clone()),
    };
    let out = med
        .handle(
            &req(
                &tok.token,
                &effs[0],
                "env-1",
                "127.0.0.1",
                8443,
                "GET",
                "/",
                None,
            ),
            &chain(),
        )
        .unwrap();
    let MediatedOutcome::Refused { reason, .. } = out else {
        panic!("expected Refused, got {out:?}");
    };
    assert_eq!(
        reason,
        hh_containment::egress::EgressReason::NotAllowedLocal
    );
    assert!(transport.log.lock().unwrap().is_empty());

    // AllowListed — the covering allow rule admits the loopback fixture.
    let mut tm2 = minter();
    let tok2 = tm2.mint(&effs[1], 1, "env-1");
    let ok_policy = mediated_policy(
        vec![allow_rule("127.0.0.1")],
        DefaultUnmatched::Deny,
        NonPublic::AllowListed,
    );
    let mut med2 = EgressMediator {
        store: med.store,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm2.resolver(),
        broker: med.broker,
        policy: ok_policy,
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "agent.main".into(),
        resolver: Box::new(fixture_resolver(&[("127.0.0.1", "127.0.0.1")])),
        transport: Box::new(transport.clone()),
    };
    let out2 = med2
        .handle(
            &req(
                &tok2.token,
                &effs[1],
                "env-1",
                "127.0.0.1",
                8443,
                "GET",
                "/",
                None,
            ),
            &chain(),
        )
        .unwrap();
    assert!(matches!(out2, MediatedOutcome::Forwarded { .. }));
    assert_eq!(transport.log.lock().unwrap().len(), 1);
}

/// AC-R-2.8.4-7: `default_unmatched = ask` produces a durable ask
/// (`security.permission.{pending,requested}` + `decided{ask}`); an
/// `allow_lease{session}` endorsement amends the policy (a durable
/// `security.containment.amended` row), forwards, and the *next* request to
/// the endorsed host allows under the amended rule.
#[test]
fn default_unmatched_ask_endorse_lease_amends_and_forwards() {
    let (mut store, run, lease) = open("ask-lease");
    let effs = open_scopes(&mut store, &run, &lease, 2);
    let mut broker = broker_with(&[]);
    let mut tm = minter();
    let tok = tm.mint(&effs[0], 1, "env-1");
    // No allow rule for new.example — default_ask.
    let policy = mediated_policy(vec![], DefaultUnmatched::Ask, NonPublic::AllowListed);
    let transport = CaptureTransport::default();
    let budget_id = open_budget(&mut store, &run, &lease, 10, 10);
    let mut med = EgressMediator {
        store: &mut store,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm.resolver(),
        broker: &mut broker,
        policy,
        cache: ApprovalCache::default(),
        budget_id: Some(budget_id),
        participant_ref: "agent.main".into(),
        resolver: Box::new(fixture_resolver(&[("new.example", "127.0.0.1")])),
        transport: Box::new(transport.clone()),
    };

    let r = req(
        &tok.token,
        &effs[0],
        "env-1",
        "new.example",
        443,
        "GET",
        "/",
        None,
    );
    let out = med.handle(&r, &chain()).unwrap();
    let MediatedOutcome::Asked { permission_id, .. } = out else {
        panic!("expected Asked, got {out:?}");
    };
    assert_eq!(
        class_count(med.store, &run, "security.permission.pending"),
        1
    );
    // `security.permission.requested` is the ephemeral prompt (row_eph —
    // subscribe-only, never durable); `pending` is the owed-decision row.
    assert_eq!(
        class_count(med.store, &run, "security.permission.requested"),
        0
    );
    let asks = class_where(med.store, &run, "security.egress.decided", |p| {
        jstr(p, "decision").as_deref() == Some("ask")
    });
    assert_eq!(asks.len(), 1);
    assert!(transport.log.lock().unwrap().is_empty());

    // The human endorses allow_lease{scope: run} — the lease's run is the
    // run (§5g.4 §2's bound; the Stage-2 `LeaseScope` spelling).
    let response = ApprovalResponse {
        permission_id: permission_id.clone(),
        choice: ResponseChoice::AllowLease(LeaseSpec {
            pattern: None,
            scope: hh_monitor::approval::LeaseScope::Run,
            max_uses: None,
        }),
        scope: DecisionScope::Session,
        max_uses: None,
        justification: None,
        decided_by: EndorserRef::Human {
            subject_ref: "human:op".into(),
            authority: AuthorityClass::Principal,
        },
        decided_at: med.store.now_ms(),
    };
    let endorser = ProvenanceRecord::kernel("kernel:test", 0);
    let out2 = med
        .endorse_asked(&r, &response, &endorser, &chain())
        .unwrap();
    assert!(matches!(out2, MediatedOutcome::Forwarded { .. }));
    // The amended row is durable and the policy gained the rule.
    assert_eq!(
        class_count(med.store, &run, "security.containment.amended"),
        1
    );
    assert!(med.policy.net.rules.iter().any(|r| {
        r.decision == RuleDecision::Allow && r.host == HostPattern::parse("new.example").unwrap()
    }));

    // The next request to the endorsed host allows without a second ask —
    // the amended rule (or the narrowing-only cache) covers it.
    let tok2 = tm.mint(&effs[1], 1, "env-1");
    let r2 = req(
        &tok2.token,
        &effs[1],
        "env-1",
        "new.example",
        443,
        "GET",
        "/",
        None,
    );
    let out3 = med.handle(&r2, &chain()).unwrap();
    assert!(
        matches!(out3, MediatedOutcome::Forwarded { .. }),
        "{out3:?}"
    );
    // No second pending row — the ask path did not re-fire.
    assert_eq!(
        class_count(med.store, &run, "security.permission.pending"),
        1
    );
    assert_eq!(transport.log.lock().unwrap().len(), 2);
}

/// AC-R-2.8.4-7 (deny arm): an `allow_once` endorsement forwards *this*
/// request but records nothing reusable — and a `deny` endorsement refuses
/// with `decided{deny, monitor}` durable.
#[test]
fn endorse_allow_once_forwards_deny_refuses() {
    let (mut store, run, lease) = open("ask-once");
    let effs = open_scopes(&mut store, &run, &lease, 2);
    let mut broker = broker_with(&[]);
    let mut tm = minter();
    let tok = tm.mint(&effs[0], 1, "env-1");
    // No rule covers `svc.example` — `default_unmatched = ask` fires. The
    // host resolves public, so a monitor endorsement's recheck passes
    // (the non-public guard's allow-list requirement doesn't apply to
    // public addresses).
    let policy = mediated_policy(vec![], DefaultUnmatched::Ask, NonPublic::AllowListed);
    let transport = CaptureTransport::default();
    let mut med = EgressMediator {
        store: &mut store,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm.resolver(),
        broker: &mut broker,
        policy,
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "agent.main".into(),
        resolver: Box::new(fixture_resolver(&[("svc.example", "93.184.216.34")])),
        transport: Box::new(transport.clone()),
    };
    let r = req(
        &tok.token,
        &effs[0],
        "env-1",
        "svc.example",
        443,
        "GET",
        "/",
        None,
    );
    let out = med.handle(&r, &chain()).unwrap();
    let MediatedOutcome::Asked { permission_id, .. } = out else {
        panic!("expected Asked, got {out:?}");
    };
    let endorse = |choice: ResponseChoice| ApprovalResponse {
        permission_id: permission_id.clone(),
        scope: DecisionScope::Once,
        max_uses: None,
        justification: None,
        choice,
        decided_by: EndorserRef::Human {
            subject_ref: "human:op".into(),
            authority: AuthorityClass::Principal,
        },
        decided_at: med.store.now_ms(),
    };
    let endorser = ProvenanceRecord::kernel("kernel:test", 0);
    let out = med
        .endorse_asked(&r, &endorse(ResponseChoice::AllowOnce), &endorser, &chain())
        .unwrap();
    assert!(matches!(out, MediatedOutcome::Forwarded { .. }));
    // allow_once amends nothing, caches nothing.
    assert_eq!(
        class_count(med.store, &run, "security.containment.amended"),
        0
    );
    assert!(med.cache.is_empty());
    assert_eq!(transport.log.lock().unwrap().len(), 1);

    // deny endorsement → refused, decided{deny, monitor} durable.
    let mut tm2 = minter();
    let tok2 = tm2.mint(&effs[1], 1, "env-1");
    let mut med2 = EgressMediator {
        store: med.store,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm2.resolver(),
        broker: med.broker,
        policy: med.policy.clone(),
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "agent.main".into(),
        resolver: Box::new(fixture_resolver(&[("svc.example", "93.184.216.34")])),
        transport: Box::new(transport.clone()),
    };
    let r2 = req(
        &tok2.token,
        &effs[1],
        "env-1",
        "svc.example",
        443,
        "GET",
        "/",
        None,
    );
    let out = med2.handle(&r2, &chain()).unwrap();
    let MediatedOutcome::Asked { permission_id, .. } = out else {
        panic!("expected Asked, got {out:?}");
    };
    let response = ApprovalResponse {
        permission_id,
        choice: ResponseChoice::Deny {
            reason: "no".into(),
        },
        scope: DecisionScope::Once,
        max_uses: None,
        justification: None,
        decided_by: EndorserRef::Human {
            subject_ref: "human:op".into(),
            authority: AuthorityClass::Principal,
        },
        decided_at: med2.store.now_ms(),
    };
    let out = med2
        .endorse_asked(&r2, &response, &endorser, &chain())
        .unwrap();
    assert!(matches!(out, MediatedOutcome::Refused { .. }));
    let monitor_denies = class_where(med2.store, &run, "security.egress.decided", |p| {
        jstr(p, "decision").as_deref() == Some("deny")
            && jstr(p, "decided_by").as_deref() == Some("monitor")
    });
    assert_eq!(monitor_denies.len(), 1);
    // Still exactly one forwarded call.
    assert_eq!(transport.log.lock().unwrap().len(), 1);
}

/// `network.calls` exhaustion refuses *before* the wire — the second call
/// under a cap of 1 is `deny{monitor, budget_exhausted}` and the transport
/// never fires.
#[test]
fn network_calls_exhaustion_gates_before_wire() {
    let (mut store, run, lease) = open("calls-cap");
    let effs = open_scopes(&mut store, &run, &lease, 2);
    let mut broker = broker_with(&[]);
    let mut tm = minter();
    let tok1 = tm.mint(&effs[0], 1, "env-1");
    let tok2 = tm.mint(&effs[1], 1, "env-1");
    let policy = mediated_policy(
        vec![allow_rule("127.0.0.1")],
        DefaultUnmatched::Deny,
        NonPublic::AllowListed,
    );
    let transport = CaptureTransport::default();
    let budget_id = open_budget(&mut store, &run, &lease, 1, 10);
    let mut med = EgressMediator {
        store: &mut store,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm.resolver(),
        broker: &mut broker,
        policy,
        cache: ApprovalCache::default(),
        budget_id: Some(budget_id),
        participant_ref: "agent.main".into(),
        resolver: Box::new(fixture_resolver(&[("127.0.0.1", "127.0.0.1")])),
        transport: Box::new(transport.clone()),
    };
    let out1 = med
        .handle(
            &req(
                &tok1.token,
                &effs[0],
                "env-1",
                "127.0.0.1",
                8443,
                "GET",
                "/",
                None,
            ),
            &chain(),
        )
        .unwrap();
    assert!(matches!(out1, MediatedOutcome::Forwarded { .. }));
    let out2 = med
        .handle(
            &req(
                &tok2.token,
                &effs[1],
                "env-1",
                "127.0.0.1",
                8443,
                "GET",
                "/",
                None,
            ),
            &chain(),
        )
        .unwrap();
    let MediatedOutcome::Refused { reason, .. } = out2 else {
        panic!("expected Refused, got {out2:?}");
    };
    assert_eq!(
        reason,
        hh_containment::egress::EgressReason::BudgetExhausted
    );
    assert_eq!(transport.log.lock().unwrap().len(), 1);
    assert_eq!(class_count(med.store, &run, "control.budget.consumed"), 1);
}

/// AC-R-2.8.4-7 (budget arm): `approvals.requested` exhaustion converts the
/// escape hatch's ask into a durable `deny{budget_exhausted}` — no pending
/// permission, no wire, the reserve's own `security.permission.decided` row
/// records the policy denial.
#[test]
fn approvals_exhaustion_converts_ask_to_deny() {
    let (mut store, run, lease) = open("ask-cap");
    let effs = open_scopes(&mut store, &run, &lease, 1);
    let mut broker = broker_with(&[]);
    let mut tm = minter();
    let tok = tm.mint(&effs[0], 1, "env-1");
    let policy = mediated_policy(vec![], DefaultUnmatched::Ask, NonPublic::AllowListed);
    let transport = CaptureTransport::default();
    // Ten calls, zero asks — the hatch's first ask is already exhausted.
    let budget_id = open_budget(&mut store, &run, &lease, 10, 0);
    let mut med = EgressMediator {
        store: &mut store,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm.resolver(),
        broker: &mut broker,
        policy,
        cache: ApprovalCache::default(),
        budget_id: Some(budget_id),
        participant_ref: "agent.main".into(),
        resolver: Box::new(fixture_resolver(&[("svc.example", "93.184.216.34")])),
        transport: Box::new(transport.clone()),
    };
    let out = med
        .handle(
            &req(
                &tok.token,
                &effs[0],
                "env-1",
                "svc.example",
                443,
                "GET",
                "/",
                None,
            ),
            &chain(),
        )
        .unwrap();
    let MediatedOutcome::Refused { reason, .. } = out else {
        panic!("expected Refused, got {out:?}");
    };
    assert_eq!(
        reason,
        hh_containment::egress::EgressReason::BudgetExhausted
    );
    // No wire, no pending/requested permission — the reserve's decided row
    // and the egress decided deny are the durable record.
    assert!(transport.log.lock().unwrap().is_empty());
    assert_eq!(
        class_count(med.store, &run, "security.permission.pending"),
        0
    );
    assert_eq!(
        class_count(med.store, &run, "security.permission.requested"),
        0
    );
    let decided = class_where(med.store, &run, "security.egress.decided", |p| {
        jstr(p, "decision").as_deref() == Some("deny")
            && jstr(p, "reason").as_deref() == Some("budget_exhausted")
    });
    assert_eq!(decided.len(), 1);
    assert_eq!(
        class_count(med.store, &run, "security.permission.decided"),
        1
    );
}

// ── LT-04 ────────────────────────────────────────────────────────────────────

/// LT-04: three channels, one rotated mid-run — every forwarded request
/// carries its own binding's value, exactly one `used` row per request, and
/// `leak_scan` over the run's events is empty (old and new values alike).
#[test]
fn lt04_three_channels_rotate_leak_scan_empty() {
    let (mut store, run, lease) = open("lt04");
    let effs = open_scopes(&mut store, &run, &lease, 4);
    let mut broker = broker_with(&[
        ("vault:10.0.0.1", "tok-gh-OLD"),
        ("vault:10.0.0.2", "tok-api-v1"),
        ("vault:10.0.0.3", "tok-db"),
    ]);
    for (cid, host, env_name) in [
        ("github", "10.0.0.1", "GH_TOKEN"),
        ("api", "10.0.0.2", "API_TOKEN"),
        ("db", "10.0.0.3", "DB_TOKEN"),
    ] {
        broker
            .register_channel(
                cid,
                channel_spec(host, 8443, env_name, None),
                ProvenanceRecord::kernel("kernel:test", 0),
            )
            .unwrap();
    }
    let b_gh = bound_binding(
        &mut broker,
        &mut store,
        &run,
        &lease,
        "github",
        "env-1",
        &["10.0.0.1"],
    );
    let b_api = bound_binding(
        &mut broker,
        &mut store,
        &run,
        &lease,
        "api",
        "env-1",
        &["10.0.0.2"],
    );
    let b_db = bound_binding(
        &mut broker,
        &mut store,
        &run,
        &lease,
        "db",
        "env-1",
        &["10.0.0.3"],
    );
    let s_gh = broker
        .placeholder_for(&b_gh.binding_id)
        .unwrap()
        .spelling
        .clone();
    let s_api = broker
        .placeholder_for(&b_api.binding_id)
        .unwrap()
        .spelling
        .clone();
    let s_db = broker
        .placeholder_for(&b_db.binding_id)
        .unwrap()
        .spelling
        .clone();

    let mut tm = minter();
    let transport = CaptureTransport::default();
    let policy = mediated_policy(
        vec![
            allow_rule("10.0.0.1"),
            allow_rule("10.0.0.2"),
            allow_rule("10.0.0.3"),
        ],
        DefaultUnmatched::Deny,
        NonPublic::AllowListed,
    );
    let budget_id = open_budget(&mut store, &run, &lease, 20, 10);
    let mut med = EgressMediator {
        store: &mut store,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm.resolver(),
        broker: &mut broker,
        policy,
        cache: ApprovalCache::default(),
        budget_id: Some(budget_id),
        participant_ref: "agent.main".into(),
        resolver: Box::new(fixture_resolver(&[
            ("10.0.0.1", "10.0.0.1"),
            ("10.0.0.2", "10.0.0.2"),
            ("10.0.0.3", "10.0.0.3"),
        ])),
        transport: Box::new(transport.clone()),
    };

    // One authenticated request per channel.
    for (i, (host, s)) in [
        ("10.0.0.1", &s_gh),
        ("10.0.0.2", &s_api),
        ("10.0.0.3", &s_db),
    ]
    .iter()
    .enumerate()
    {
        let eff = &effs[i];
        let t = tm.mint(eff, 1, "env-1");
        let out = med
            .handle(
                &req(&t.token, eff, "env-1", host, 8443, "GET", "/", Some(s)),
                &chain(),
            )
            .unwrap();
        assert!(matches!(out, MediatedOutcome::Forwarded { .. }), "{out:?}");
    }

    // Rotate `api` mid-run — the live binding drops, the old value is
    // retained in the mask set.
    med.broker
        .rotate(
            med.store,
            &run,
            &lease,
            "api",
            1,
            SecretSource::OperatorVault {
                vault_ref: "vault:10.0.0.2".into(),
            },
        )
        .unwrap();
    // The rotated binding's sentinel is now dead — a replay refuses revoked.
    let eff = &effs[3];
    let t = tm.mint(eff, 1, "env-1");
    let out = med
        .handle(
            &req(
                &t.token,
                eff,
                "env-1",
                "10.0.0.2",
                8443,
                "GET",
                "/",
                Some(&s_api),
            ),
            &chain(),
        )
        .unwrap();
    let MediatedOutcome::RefusedCredential { code, .. } = out else {
        panic!("expected RefusedCredential after rotate, got {out:?}");
    };
    assert_eq!(code, RefusedCode::Revoked);

    // Exactly one `used` row per *forwarded* authenticated request (3).
    let used = class_where(med.store, &run, "security.credential.used", |_| true);
    assert_eq!(used.len(), 3);
    // The wire carried each binding's real value, never a sentinel.
    {
        let got = transport.log.lock().unwrap();
        assert_eq!(got.len(), 3);
        let wire = got
            .iter()
            .map(|c| {
                c.headers
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect::<Vec<_>>()
            .join("\n---\n");
        assert!(wire.contains("tok-gh-OLD") || wire.contains("tok-gh"));
        assert!(wire.contains("tok-api-v1"));
        assert!(wire.contains("tok-db"));
        for s in [&s_gh, &s_api, &s_db] {
            assert!(!wire.contains(s.as_str()), "sentinel {s} leaked to wire");
        }
    }
    // `leak_scan` over every committed event payload is empty — no value
    // (live, rotated, or minted) ever reached a durable surface.
    let det = DetectorSet::standard(med.broker.mask_set().unwrap());
    let leaks = med.broker.leak_scan_run(med.store, &run, &det).unwrap();
    assert!(leaks.is_empty(), "leaks: {leaks:?}");
    // The mask set retains the rotated value.
    let ms = med.broker.mask_set().unwrap();
    assert!(ms
        .all_entries()
        .iter()
        .any(|e| e.value == "tok-api-v1" && !e.live));
}

// ── LT-05 ────────────────────────────────────────────────────────────────────

/// LT-05: after a revoke, a replayed request is refused `revoked`; minted
/// scoped tokens die with their binding and at their expiry; the declared
/// `revocation_path` intent dispatches as a mediated POST.
#[test]
fn lt05_revoke_fences_replay_minted_and_destination() {
    let (mut store, run, lease) = open("lt05");
    let effs = open_scopes(&mut store, &run, &lease, 3);
    let port = 18443u16;
    let mut broker = broker_with(&[("vault:127.0.0.1", "tok-live")]);
    broker
        .register_channel(
            "github",
            channel_spec("127.0.0.1", port, "GH_TOKEN", Some("/revoke")),
            ProvenanceRecord::kernel("kernel:test", 0),
        )
        .unwrap();
    let binding = bound_binding(
        &mut broker,
        &mut store,
        &run,
        &lease,
        "github",
        "env-1",
        &["127.0.0.1"],
    );
    let sentinel = broker
        .placeholder_for(&binding.binding_id)
        .unwrap()
        .spelling
        .clone();

    // A minted-scoped token under the live binding verifies — then dies with it.
    let minted = broker
        .mint(
            &mut store,
            &run,
            &lease,
            &binding.binding_id,
            "127.0.0.1",
            60_000,
        )
        .unwrap();
    assert_eq!(
        broker.verify_minted(&minted.token, "127.0.0.1", store.now_ms()),
        MintedVerdict::Valid
    );

    let mut tm = minter();
    let tok = tm.mint(&effs[0], 1, "env-1");
    let transport = CaptureTransport::default();
    let policy = mediated_policy(
        vec![allow_rule_with_methods("127.0.0.1", &["GET", "POST"])],
        DefaultUnmatched::Deny,
        NonPublic::AllowListed,
    );
    let mut med = EgressMediator {
        store: &mut store,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm.resolver(),
        broker: &mut broker,
        policy,
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "agent.main".into(),
        resolver: Box::new(fixture_resolver(&[("127.0.0.1", "127.0.0.1")])),
        transport: Box::new(transport.clone()),
    };
    let out = med
        .handle(
            &req(
                &tok.token,
                &effs[0],
                "env-1",
                "127.0.0.1",
                port,
                "GET",
                "/",
                Some(&sentinel),
            ),
            &chain(),
        )
        .unwrap();
    assert!(matches!(out, MediatedOutcome::Forwarded { .. }));

    // Revoke the binding — the declared revocation_path yields one intent.
    let outcome = med
        .broker
        .revoke(
            med.store,
            &run,
            &lease,
            RevokeTarget::Binding(binding.binding_id.clone()),
            "test-revoke",
        )
        .unwrap();
    assert_eq!(outcome.revoked, vec![binding.binding_id.clone()]);
    assert_eq!(outcome.intents.len(), 1);
    assert_eq!(outcome.intents[0].revocation_path, "/revoke");
    assert_eq!(outcome.intents[0].port, Some(port));

    // A replayed request on the dead binding refuses `revoked`.
    let mut tm2 = minter();
    let tok2 = tm2.mint(&effs[1], 1, "env-1");
    med.tokens = tm2.resolver();
    let out = med
        .handle(
            &req(
                &tok2.token,
                &effs[1],
                "env-1",
                "127.0.0.1",
                port,
                "GET",
                "/",
                Some(&sentinel),
            ),
            &chain(),
        )
        .unwrap();
    let MediatedOutcome::RefusedCredential { code, .. } = out else {
        panic!("expected RefusedCredential (revoked), got {out:?}");
    };
    assert_eq!(code, RefusedCode::Revoked);

    // The minted token died with its binding.
    assert_eq!(
        med.broker
            .verify_minted(&minted.token, "127.0.0.1", store_now(&med)),
        MintedVerdict::Revoked
    );

    // The destination-side intent dispatches as a mediated POST — the
    // fixture sees the revoke hit (kernel-attributed token on env "kernel").
    let mut tmk = minter();
    let ktok = tmk.mint(&effs[2], 1, "kernel");
    med.tokens = tmk.resolver();
    let results = med.dispatch_revocations(&outcome.intents, &ktok.token, &chain());
    assert_eq!(results.len(), 1);
    // The revoke POST is itself mediated: `POST /revoke` to 127.0.0.1:port is
    // allow-listed (method POST declared on the rule).
    match &results[0] {
        Ok(MediatedOutcome::Forwarded { .. }) => {}
        other => panic!("revocation dispatch failed: {other:?}"),
    }
    // The intent went out as `POST /revoke` to the declared port carrying
    // the binding coordinate (never the credential).
    let got = transport.log.lock().unwrap();
    let last = got.last().unwrap();
    assert_eq!(last.method, "POST");
    assert_eq!(last.path, "/revoke");
    assert_eq!(last.port, port);
    assert!(String::from_utf8_lossy(&last.body).contains(&binding.binding_id));
    assert!(!String::from_utf8_lossy(&last.body).contains("tok-live"));

    // Expiry: a minted token past its ttl is `expired`, not `valid`.
    let binding2 = bound_binding(
        med.broker,
        med.store,
        &run,
        &lease,
        "github",
        "env-1",
        &["127.0.0.1"],
    );
    let now = med.store.now_ms();
    let short = med
        .broker
        .mint(
            med.store,
            &run,
            &lease,
            &binding2.binding_id,
            "127.0.0.1",
            5,
        )
        .unwrap();
    assert_eq!(
        med.broker.verify_minted(&short.token, "127.0.0.1", now + 6),
        MintedVerdict::Expired
    );
    // Wrong audience ⇒ invalid.
    assert_eq!(
        med.broker.verify_minted(&short.token, "other.host", now),
        MintedVerdict::Invalid
    );
}

fn store_now(med: &EgressMediator) -> u64 {
    med.store.now_ms()
}

// ── LT-06 ────────────────────────────────────────────────────────────────────

/// LT-06: a stale `expected_revision` refuses `Conflict` (CAS); a successful
/// rotation retains the *old* value in the mask set and the new value is
/// masked from birth — a blob containing either is caught by the detector.
#[test]
fn lt06_stale_cas_conflict_and_old_value_masked() {
    let (mut store, run, lease) = open("lt06");
    let mut broker = broker_with(&[("vault:10.0.0.9", "tok-OLD"), ("vault:r2", "tok-NEW")]);
    broker
        .register_channel(
            "api",
            channel_spec("10.0.0.9", 443, "API_TOKEN", None),
            ProvenanceRecord::kernel("kernel:test", 0),
        )
        .unwrap();
    // Stale CAS → Conflict, nothing rotated.
    let err = broker
        .rotate(
            &mut store,
            &run,
            &lease,
            "api",
            7,
            SecretSource::OperatorVault {
                vault_ref: "vault:r2".into(),
            },
        )
        .unwrap_err();
    assert!(matches!(err, BrokerError::Conflict { .. }));

    let rev = broker
        .rotate(
            &mut store,
            &run,
            &lease,
            "api",
            1,
            SecretSource::OperatorVault {
                vault_ref: "vault:r2".into(),
            },
        )
        .unwrap();
    assert_eq!(rev, 2);

    // Both the old (retained, dead) and new (live) values are masked — a
    // surface carrying either is a detected leak.
    let det = DetectorSet::standard(broker.mask_set().unwrap());
    for v in ["tok-OLD", "tok-NEW"] {
        let hits = hh_secrets::redact::detect(&format!("prefix {v} suffix"), &det);
        assert!(
            hits.iter()
                .any(|h| h.detector == hh_secrets::redact::DetectorKind::KnownValue),
            "{v} not masked"
        );
    }
    // The rotated event is durable.
    assert!(class_count(&store, &run, "security.credential.rotated") >= 1);
}

// ── LT-09 (snapshot/fork half) ────────────────────────────────────────────────

/// LT-09 (SV-10's Stage-2 half): the projected env + its snapshot carry
/// placeholder spellings only; a fork *virtualises* — fresh placeholders on
/// fresh bindings — and the parent's sentinel on the fork's env is
/// `out_of_scope` (non-transferable).
#[test]
fn lt09_snapshot_placeholders_only_fork_virtualizes() {
    let (mut store, run, lease) = open("lt09-fork");
    let effs = open_scopes(&mut store, &run, &lease, 2);
    let mut broker = broker_with(&[("vault:127.0.0.1", "tok-fork")]);
    broker
        .register_channel(
            "github",
            channel_spec("127.0.0.1", 8443, "GH_TOKEN", None),
            ProvenanceRecord::kernel("kernel:test", 0),
        )
        .unwrap();
    let binding = bound_binding(
        &mut broker,
        &mut store,
        &run,
        &lease,
        "github",
        "env-1",
        &["127.0.0.1"],
    );
    let parent_sentinel = broker
        .placeholder_for(&binding.binding_id)
        .unwrap()
        .spelling
        .clone();

    // The projected env carries only the placeholder; the snapshot is the
    // same shape (there is no value-carrying form).
    let spec = broker.env_spec_for("env-1", BTreeSet::from(["GH_TOKEN".to_string()]));
    let placeholders: BTreeMap<String, Placeholder> = spec
        .bindings
        .iter()
        .map(|b| {
            (
                b.env_name.clone(),
                broker.placeholder_for(&b.binding_id).unwrap().clone(),
            )
        })
        .collect();
    let projected = env_apply(
        &spec,
        &BTreeMap::new(),
        &placeholders,
        &DetectorSet::standard(broker.mask_set().unwrap()),
    )
    .unwrap();
    assert_eq!(projected.vars.get("GH_TOKEN"), Some(&parent_sentinel));
    let snap = env_snapshot(&projected);
    let snap_s = snap.to_canonical_string();
    assert!(snap_s.contains(&parent_sentinel));
    assert!(!snap_s.contains("tok-fork"));
    let det = DetectorSet::standard(broker.mask_set().unwrap());
    assert!(env_sweep(&projected, &det).is_empty());

    // Fork: env-1 → env-2 — fresh bindings + fresh placeholder spellings.
    let virt = broker
        .virtualize_for_fork(&mut store, &run, &lease, "env-1", "env-2")
        .unwrap();
    assert_eq!(virt.bindings.len(), 1);
    assert_eq!(virt.rewrites.len(), 1);
    let fork_sentinel = virt.rewrites.get(&parent_sentinel).unwrap().clone();
    assert_ne!(fork_sentinel, parent_sentinel);
    // The fork's bound row is durable.
    assert!(class_count(&store, &run, "security.credential.bound") >= 2);

    // The parent's sentinel on the fork's env is out_of_scope; the fork's
    // own sentinel mediates on the fork's env.
    let mut tm = minter();
    let transport = CaptureTransport::default();
    let policy = mediated_policy(
        vec![allow_rule("127.0.0.1")],
        DefaultUnmatched::Deny,
        NonPublic::AllowListed,
    );
    let mut med = EgressMediator {
        store: &mut store,
        run_id: run.clone(),
        lease: &lease,
        tokens: tm.resolver(),
        broker: &mut broker,
        policy,
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "agent.main".into(),
        resolver: Box::new(fixture_resolver(&[("127.0.0.1", "127.0.0.1")])),
        transport: Box::new(transport.clone()),
    };

    let t1 = tm.mint(&effs[0], 1, "env-2");
    let out = med
        .handle(
            &req(
                &t1.token,
                &effs[0],
                "env-2",
                "127.0.0.1",
                8443,
                "GET",
                "/",
                Some(&parent_sentinel),
            ),
            &chain(),
        )
        .unwrap();
    let MediatedOutcome::RefusedCredential { code, .. } = out else {
        panic!("expected RefusedCredential, got {out:?}");
    };
    assert_eq!(code, RefusedCode::OutOfScope);

    let t2 = tm.mint(&effs[1], 1, "env-2");
    let out = med
        .handle(
            &req(
                &t2.token,
                &effs[1],
                "env-2",
                "127.0.0.1",
                8443,
                "GET",
                "/",
                Some(&fork_sentinel),
            ),
            &chain(),
        )
        .unwrap();
    assert!(matches!(out, MediatedOutcome::Forwarded { .. }));
    let got = transport.log.lock().unwrap();
    assert_eq!(got[0].host, "127.0.0.1");
    assert_eq!(got[0].addr, "127.0.0.1".parse::<IpAddr>().unwrap());
    assert!(got[0].headers.iter().any(|(_, v)| v.contains("tok-fork")));

    // The forked env's leak scan is still empty.
    let det = DetectorSet::standard(med.broker.mask_set().unwrap());
    assert!(med
        .broker
        .leak_scan_run(med.store, &run, &det)
        .unwrap()
        .is_empty());
}
