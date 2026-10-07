//! R2.9b acceptance battery — the DF-S1.12-6 runtime halves at the
//! mediated-egress wire point (ADR-0341 D3):
//!
//! - `tls.terminate` — a terminate-required policy against a
//!   non-terminating transport refuses typed (`tls_terminate_unsupported`,
//!   `source: tls_guard`) and never fires the wire; a terminating transport
//!   forwards and the decided row records `tls_terminated: true`.
//! - `inspect_hooks` — the declared hook set runs in declaration order over
//!   the sentinel-spelling view; an unregistered declared ref refuses
//!   `inspect_hook_unavailable`, a `deny` verdict refuses
//!   `inspect_hook_denied`, and every row carries `guard_detail`.
//! - `approved_host_body` — the declared residual's reader-set closer runs
//!   at the mediator too (a monitor-endorsed allow never bypasses it).

use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hh_containment::egress::{ApprovalCache, EgressReason, EgressRequest};
use hh_containment::policy::{
    kernel_default, AmendmentBasis, ContainmentPolicy, DefaultUnmatched, EgressProtocol,
    EgressRule, HostPattern, NetMode, NonPublic, ResidualChannel, ResidualKind, RuleDecision,
};
use hh_env::egress::{
    EgressMediator, EgressTransport, InspectHook, InspectVerdict, InspectView, MediatedOutcome,
    StaticResolver, WireResponse,
};
use hh_env::events::{EventMinter, ScopeChain};
use hh_env::tokens::TokenMinter;
use hh_hir::leaves::Text;
use hh_ledger::event::Scope;
use hh_ledger::ids::{ManualClock, SeqIds};
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ontology::risk::{
    RepeatSafety as RiskRepeatSafety, RiskClass, RiskReversibility, RiskScope,
};
use hh_provenance::authority::ReaderSet;
use hh_provenance::{PersistenceScope, ProvenanceRecord};
use hh_secrets::{CredentialBroker, StaticVault};
use hh_wire::json::Json;

// ── scaffold ────────────────────────────────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-env-r29b-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn open(tag: &str) -> (Store, String, Lease, ManualClock) {
    let clock = ManualClock::at(1_000);
    let mut s = Store::open_with(
        dir(tag),
        Box::new(clock.clone()),
        Some(Box::new(SeqIds::new())),
        DEFAULT_BLOB_MAX_BYTES,
    )
    .unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease, clock)
}

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
                hh_env::events::intended_payload(&risk, Some(&risk), "v-cap", "h", i as u64),
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

fn chain() -> ScopeChain {
    ScopeChain {
        turn_id: "turn-1".into(),
        model_call_id: "mc-1".into(),
        tool_call_id: "tc-1".into(),
    }
}

fn allow_rule(host: &str) -> EgressRule {
    EgressRule {
        host: HostPattern::parse(host).unwrap(),
        ports: vec![],
        protocols: vec![],
        methods: vec!["GET".to_string(), "POST".to_string()],
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
    p.amendment.persist_scope_ceiling = PersistenceScope::Run;
    p.compute_ids();
    p
}

fn residual_body() -> ResidualChannel {
    ResidualChannel {
        kind: ResidualKind::ApprovedHostBody,
        statement: Text::new(
            "bodies to the approved host",
            "test",
            ProvenanceRecord::kernel("t", 0),
        ),
        closer: "reader_coverage".to_string(),
        owner: "test".to_string(),
    }
}

// ── transports / hooks ─────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct CaptureTransport {
    log: Arc<Mutex<Vec<String>>>,
    terminates: bool,
}

impl EgressTransport for CaptureTransport {
    fn forward(
        &self,
        _addr: IpAddr,
        _port: u16,
        host: &str,
        _method: &str,
        path: &str,
        _headers: &[(String, String)],
        _body: Option<&[u8]>,
    ) -> Result<WireResponse, String> {
        self.log.lock().unwrap().push(format!("{host}{path}"));
        Ok(WireResponse {
            status: 200,
            headers: vec![],
            body: b"ok".to_vec(),
        })
    }

    fn terminates_tls(&self) -> bool {
        self.terminates
    }
}

struct FixedHook(InspectVerdict);

impl InspectHook for FixedHook {
    fn inspect(&self, _view: &InspectView) -> InspectVerdict {
        self.0
    }
}

fn minter() -> TokenMinter {
    TokenMinter::new([7u8; 32])
}

fn req(
    token: &str,
    effect_id: &str,
    host: &str,
    port: u16,
    protocol: EgressProtocol,
    body: Option<&str>,
    readers: Option<ReaderSet>,
) -> EgressRequest {
    EgressRequest {
        token: token.to_string(),
        effect_id: Some(effect_id.to_string()),
        tool_call_id: format!("call-{effect_id}"),
        env_handle: "env-1".into(),
        protocol,
        host_raw: host.to_string(),
        resolved_addrs: vec![],
        port,
        method: Some("POST".to_string()),
        path: Some("/x".to_string()),
        headers: vec![],
        body: body.map(str::to_string),
        body_readers: readers,
        credential_sentinels: vec![],
    }
}

fn mediated<'a>(
    store: &'a mut Store,
    run: &str,
    lease: &'a Lease,
    broker: &'a mut CredentialBroker,
    tokens: hh_env::tokens::TokenResolver,
    policy: ContainmentPolicy,
    transport: CaptureTransport,
    hooks: BTreeMap<String, Box<dyn InspectHook>>,
) -> EgressMediator<'a> {
    EgressMediator {
        store,
        run_id: run.to_string(),
        lease,
        tokens,
        broker,
        policy,
        cache: ApprovalCache::default(),
        budget_id: None,
        participant_ref: "agent.main".into(),
        resolver: Box::new(StaticResolver {
            map: BTreeMap::from([
                (
                    "svc.example".to_string(),
                    vec!["93.184.216.34".parse().unwrap()],
                ),
                (
                    "api.example.com".to_string(),
                    vec!["93.184.216.35".parse().unwrap()],
                ),
            ]),
        }),
        transport: Box::new(transport),
        inspect_hooks: hooks,
    }
}

fn decided_rows(store: &Store, run: &str) -> Vec<Json> {
    store
        .events(run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "security.egress.decided")
        .map(|e| e.payload.clone())
        .collect()
}

// ── tls.terminate ───────────────────────────────────────────────────────────

/// AC (DF-S1.12-6): `tls.terminate = true` + a TLS leg + a transport that
/// cannot terminate ⇒ typed refusal, durable `decided{deny}` before the
/// refusal is visible, and the wire never fires.
#[test]
fn tls_terminate_required_refuses_without_a_tls_path() {
    let (mut store, run, lease, _clock) = open("tls-refuse");
    let effs = open_scopes(&mut store, &run, &lease, 1);
    let mut broker = CredentialBroker::new(Box::new(StaticVault::default()), "fp");
    let mut tm = minter();
    let tok = tm.mint(&effs[0], 1, "env-1");

    let mut p = mediated_policy(vec![allow_rule("svc.example")]);
    p.net.tls.terminate = true;
    p.compute_ids();
    let transport = CaptureTransport::default();
    let mut med = mediated(
        &mut store,
        &run,
        &lease,
        &mut broker,
        tm.resolver(),
        p,
        transport.clone(),
        BTreeMap::new(),
    );
    let out = med
        .handle(
            &req(
                &tok.token,
                &effs[0],
                "svc.example",
                443,
                EgressProtocol::HttpsConnect,
                None,
                None,
            ),
            &chain(),
        )
        .unwrap();
    let MediatedOutcome::Refused { reason, .. } = out else {
        panic!("expected Refused, got {out:?}");
    };
    assert_eq!(reason, EgressReason::TlsTerminateUnsupported);
    let rows = decided_rows(&store, &run);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("source").and_then(Json::as_str),
        Some("tls_guard")
    );
    assert_eq!(
        rows[0].get("reason").and_then(Json::as_str),
        Some("tls_terminate_unsupported")
    );
    assert_eq!(
        rows[0].get("guard_detail").and_then(Json::as_str),
        Some("tls.terminate")
    );
    assert!(
        transport.log.lock().unwrap().is_empty(),
        "wire must not fire"
    );
}

/// The terminating transport honours the declaration — the leg forwards
/// and the decided row records `tls_terminated: true`.
#[test]
fn tls_terminate_with_a_tls_path_forwards_and_records() {
    let (mut store, run, lease, _clock) = open("tls-forward");
    let effs = open_scopes(&mut store, &run, &lease, 1);
    let mut broker = CredentialBroker::new(Box::new(StaticVault::default()), "fp");
    let mut tm = minter();
    let tok = tm.mint(&effs[0], 1, "env-1");

    let mut p = mediated_policy(vec![allow_rule("svc.example")]);
    p.net.tls.terminate = true;
    p.compute_ids();
    let mut t = CaptureTransport::default();
    t.terminates = true;
    let transport = t.clone();
    let mut med = mediated(
        &mut store,
        &run,
        &lease,
        &mut broker,
        tm.resolver(),
        p,
        t,
        BTreeMap::new(),
    );
    let out = med
        .handle(
            &req(
                &tok.token,
                &effs[0],
                "svc.example",
                443,
                EgressProtocol::HttpsConnect,
                None,
                None,
            ),
            &chain(),
        )
        .unwrap();
    assert!(matches!(out, MediatedOutcome::Forwarded { .. }), "{out:?}");
    let rows = decided_rows(&store, &run);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("decision").and_then(Json::as_str),
        Some("allow")
    );
    assert_eq!(
        rows[0].get("tls_terminated"),
        Some(&Json::Bool(true)),
        "the terminating fact is durable"
    );
    assert_eq!(transport.log.lock().unwrap().len(), 1);
}

/// `tls.terminate` on a plain-HTTP leg needs no TLS path — the member
/// governs TLS legs only; the wire runs.
#[test]
fn tls_terminate_does_not_refuse_a_plain_leg() {
    let (mut store, run, lease, _clock) = open("tls-http");
    let effs = open_scopes(&mut store, &run, &lease, 1);
    let mut broker = CredentialBroker::new(Box::new(StaticVault::default()), "fp");
    let mut tm = minter();
    let tok = tm.mint(&effs[0], 1, "env-1");

    let mut p = mediated_policy(vec![allow_rule("svc.example")]);
    p.net.tls.terminate = true;
    p.compute_ids();
    let transport = CaptureTransport::default();
    let mut med = mediated(
        &mut store,
        &run,
        &lease,
        &mut broker,
        tm.resolver(),
        p,
        transport.clone(),
        BTreeMap::new(),
    );
    let out = med
        .handle(
            &req(
                &tok.token,
                &effs[0],
                "svc.example",
                80,
                EgressProtocol::Http,
                None,
                None,
            ),
            &chain(),
        )
        .unwrap();
    assert!(matches!(out, MediatedOutcome::Forwarded { .. }), "{out:?}");
}

// ── inspect_hooks ───────────────────────────────────────────────────────────

/// A declared `inspect_hooks` member with no registered hook refuses
/// `inspect_hook_unavailable` — the declared mechanism cannot be honoured.
#[test]
fn declared_hook_unregistered_refuses_closed() {
    let (mut store, run, lease, _clock) = open("hook-absent");
    let effs = open_scopes(&mut store, &run, &lease, 1);
    let mut broker = CredentialBroker::new(Box::new(StaticVault::default()), "fp");
    let mut tm = minter();
    let tok = tm.mint(&effs[0], 1, "env-1");

    let mut p = mediated_policy(vec![allow_rule("svc.example")]);
    p.net.tls.inspect_hooks = vec!["hook-a".into()];
    p.compute_ids();
    let transport = CaptureTransport::default();
    let mut med = mediated(
        &mut store,
        &run,
        &lease,
        &mut broker,
        tm.resolver(),
        p,
        transport.clone(),
        BTreeMap::new(),
    );
    let out = med
        .handle(
            &req(
                &tok.token,
                &effs[0],
                "svc.example",
                80,
                EgressProtocol::Http,
                None,
                None,
            ),
            &chain(),
        )
        .unwrap();
    let MediatedOutcome::Refused { reason, .. } = out else {
        panic!("expected Refused, got {out:?}");
    };
    assert_eq!(reason, EgressReason::InspectHookUnavailable);
    let rows = decided_rows(&store, &run);
    assert_eq!(
        rows[0].get("source").and_then(Json::as_str),
        Some("inspect_hook")
    );
    assert_eq!(
        rows[0].get("guard_detail").and_then(Json::as_str),
        Some("hook-a")
    );
    assert!(transport.log.lock().unwrap().is_empty());
}

/// A registered hook's `deny` verdict refuses the request — the declared
/// hook set runs before the wire.
#[test]
fn hook_deny_refuses_and_hook_allow_forwards() {
    let (mut store, run, lease, _clock) = open("hook-run");
    let effs = open_scopes(&mut store, &run, &lease, 2);
    let mut broker = CredentialBroker::new(Box::new(StaticVault::default()), "fp");

    // Denying hook.
    let mut tm = minter();
    let tok = tm.mint(&effs[0], 1, "env-1");
    let mut p = mediated_policy(vec![allow_rule("svc.example")]);
    p.net.tls.inspect_hooks = vec!["hook-a".into()];
    p.compute_ids();
    let transport = CaptureTransport::default();
    let mut med = mediated(
        &mut store,
        &run,
        &lease,
        &mut broker,
        tm.resolver(),
        p.clone(),
        transport.clone(),
        BTreeMap::from([(
            "hook-a".to_string(),
            Box::new(FixedHook(InspectVerdict::Deny)) as Box<dyn InspectHook>,
        )]),
    );
    let out = med
        .handle(
            &req(
                &tok.token,
                &effs[0],
                "svc.example",
                80,
                EgressProtocol::Http,
                None,
                None,
            ),
            &chain(),
        )
        .unwrap();
    let MediatedOutcome::Refused { reason, .. } = out else {
        panic!("expected Refused, got {out:?}");
    };
    assert_eq!(reason, EgressReason::InspectHookDenied);
    assert!(transport.log.lock().unwrap().is_empty());

    // Allowing hook — the same declared set passes and the wire runs.
    let tok2 = tm.mint(&effs[1], 2, "env-1");
    med.tokens = tm.resolver();
    med.inspect_hooks = BTreeMap::from([(
        "hook-a".to_string(),
        Box::new(FixedHook(InspectVerdict::Allow)) as Box<dyn InspectHook>,
    )]);
    let out = med
        .handle(
            &req(
                &tok2.token,
                &effs[1],
                "svc.example",
                80,
                EgressProtocol::Http,
                None,
                None,
            ),
            &chain(),
        )
        .unwrap();
    assert!(matches!(out, MediatedOutcome::Forwarded { .. }), "{out:?}");
    assert_eq!(transport.log.lock().unwrap().len(), 1);
}

// ── approved_host_body ─────────────────────────────────────────────────────

/// The declared residual's closer at the mediator: a body whose readers
/// cover the destination forwards; an unlabeled body refuses
/// `reader_coverage` even though the allow rule matched.
#[test]
fn approved_host_body_closer_runs_at_the_mediator() {
    let (mut store, run, lease, _clock) = open("ahb");
    let effs = open_scopes(&mut store, &run, &lease, 2);
    let mut broker = CredentialBroker::new(Box::new(StaticVault::default()), "fp");
    let mut tm = minter();

    let mut p = mediated_policy(vec![allow_rule("api.example.com")]);
    p.residual_channels.push(residual_body());
    p.compute_ids();
    let transport = CaptureTransport::default();
    let mut med = mediated(
        &mut store,
        &run,
        &lease,
        &mut broker,
        tm.resolver(),
        p,
        transport.clone(),
        BTreeMap::new(),
    );

    // Uncovered — no recorded readers.
    let tok0 = tm.mint(&effs[0], 1, "env-1");
    let out = med
        .handle(
            &req(
                &tok0.token,
                &effs[0],
                "api.example.com",
                443,
                EgressProtocol::HttpsConnect,
                Some("payload"),
                None,
            ),
            &chain(),
        )
        .unwrap();
    let MediatedOutcome::Refused { reason, .. } = out else {
        panic!("expected Refused, got {out:?}");
    };
    assert_eq!(reason, EgressReason::ReaderCoverage);
    let rows: Vec<Json> = med
        .store
        .events(&run)
        .unwrap()
        .iter()
        .filter(|e| e.class == "security.egress.decided")
        .map(|e| e.payload.clone())
        .collect();
    assert_eq!(
        rows.last().unwrap().get("source").and_then(Json::as_str),
        Some("flow_guard")
    );

    // Covered — `Restricted` naming the destination host.
    let tok1 = tm.mint(&effs[1], 2, "env-1");
    med.tokens = tm.resolver();
    let out = med
        .handle(
            &req(
                &tok1.token,
                &effs[1],
                "api.example.com",
                443,
                EgressProtocol::HttpsConnect,
                Some("payload"),
                Some(ReaderSet::Restricted(
                    ["api.example.com".to_string()].into_iter().collect(),
                )),
            ),
            &chain(),
        )
        .unwrap();
    assert!(matches!(out, MediatedOutcome::Forwarded { .. }), "{out:?}");
}
