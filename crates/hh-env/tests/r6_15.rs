//! R6.15 acceptance battery (manifest 268; EGRESS a+b; AR-05;
//! REQ-XB-113/114/115/116/129, aliases REQ-XB-008/REQ-XC-102):
//! AC-1 profile admission + byte-identical refusals; AC-2 env
//! projection; AC-4 decide-before-byte over the mediator (forward +
//! CONNECT); AC-5 allowlists + phase swap + withheld; AC-6 freeze +
//! allowlist_gap. Live reachability is `#[ignore]`d (ADR-0363).

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use hharness_containment::egress::{ApprovalCache, EgressRequest};
use hharness_containment::policy::{
    kernel_default, ContainmentPolicy, EgressProtocol, NetMode, ResidualKind,
};
use hharness_env::egress::{
    EgressMediator, EgressTransport, LocalHttpTransport, MediatedOutcome, StaticResolver,
};
use hharness_env::egress_allowlist::*;
use hharness_env::events::{intended_payload, EventMinter, ScopeChain};
use hharness_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hharness_env::tokens::TokenMinter;
use hharness_ledger::event::Scope;
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::Store;
use hharness_ontology::risk::{
    RepeatSafety as RiskRepeatSafety, RiskClass, RiskReversibility, RiskScope,
};
use hharness_secrets::{CredentialBroker, StaticVault};
use hharness_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-env-r615-{tag}-{}-{n}",
        std::process::id(),
        n = N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn cell_record() -> EnvironmentRecord {
    EnvironmentRecord {
        class: EnvironmentClass::LocalContainer,
        image: ImageRef::Tag("docker.io/library/alpine:3.20".to_string()),
        build_context: None,
        platform: "linux/amd64".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: hharness_containment::policy::ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
        canary_channels: vec![],
    }
}

fn cell() -> MediatedCell {
    MediatedCell {
        socket_mounts: BTreeMap::from([
            ("model".to_string(), "/host/slot-1/model.sock".to_string()),
            ("egress".to_string(), "/host/slot-1/egress.sock".to_string()),
        ]),
        relay_bin: "/host/bin/hh-cell-relay".to_string(),
        pairs: vec![
            RelayPair {
                name: "model".to_string(),
                port: 8080,
            },
            RelayPair {
                name: "egress".to_string(),
                port: 3128,
            },
        ],
    }
}

fn list() -> Allowlist {
    Allowlist::first_pass(
        ImageFamily::Debian,
        BTreeSet::from(["astral.sh".to_string(), "github.com".to_string()]),
        &["mirror.tb-tasks.example".to_string()],
        vec![WithheldHost {
            host: "cdn.jsdelivr.net".to_string(),
            reason: "oracle-observed agent host outside Q-X55 set".to_string(),
        }],
    )
}

fn decl() -> MediatedDecl {
    MediatedDecl {
        family: ImageFamily::Debian,
        allowlist: list(),
        protocol_id: "protocol:t".to_string(),
        epoch: 1,
        cell: cell(),
        model_port: 8080,
        slot_token: "slot-token-t".to_string(),
    }
}

// ── AC-1 (REQ-XB-113; aliases REQ-XB-008/REQ-XC-102) ─────────────────────────

#[test]
fn ac1_admission_composes_profile_and_refusals_are_byte_identical() {
    let mut rec = cell_record();
    let mut pol = kernel_default(0);
    let before_pol = pol.clone();

    admit_allowlist_mediated(&mut rec, &mut pol, &decl(), 100).unwrap();

    // --network none for the cell's whole life + net.mode = mediated.
    assert_eq!(pol.net.mode, NetMode::Mediated);
    assert_eq!(
        pol.net.unix_sockets.allow,
        vec![MODEL_SOCK_PATH.to_string(), EGRESS_SOCK_PATH.to_string()]
    );
    // Declared residuals: unix_socket ×2 + tls_opaque +
    // approved_host_body + dns_names + side_channel.
    let kinds: Vec<ResidualKind> = pol.residual_channels.iter().map(|r| r.kind).collect();
    for k in [
        ResidualKind::UnixSocket,
        ResidualKind::TlsOpaque,
        ResidualKind::ApprovedHostBody,
        ResidualKind::DnsNames,
        ResidualKind::SideChannel,
    ] {
        assert!(kinds.contains(&k), "missing residual {k:?}");
    }
    assert_eq!(
        kinds
            .iter()
            .filter(|k| **k == ResidualKind::UnixSocket)
            .count(),
        2
    );
    // The sealed schedule grants exactly the phase socket sets.
    let sched = pol.ext.get("phase_schedule").unwrap();
    let layer = |p: &str| -> ContainmentPolicy {
        ContainmentPolicy::from_json(sched.get(p).unwrap().get("layer").unwrap()).unwrap()
    };
    let verify = layer("verify");
    assert_eq!(
        verify.net.unix_sockets.allow,
        vec![EGRESS_SOCK_PATH.to_string()],
        "verify phase closes the model socket"
    );
    assert!(layer("agent")
        .net
        .unix_sockets
        .allow
        .contains(&MODEL_SOCK_PATH.to_string()));
    // env hints are recorded on the policy (the backend logs the
    // proxy_env_is_not_mediation lowering loss — never claimed as
    // mediation).
    assert!(pol.proc.env.set.contains_key("http_proxy"));
    assert!(pol.proc.env.set.contains_key("no_proxy"));
    // Record stamps — honest labels.
    for (k, v) in [
        ("egress_profile", "allowlist_mediated"),
        ("cell_network", "none"),
        ("policy_net_mode", "mediated"),
        ("per_phase_network_policy", "supported"),
    ] {
        assert_eq!(rec.ext.get(k).and_then(Json::as_str), Some(v), "ext[{k}]");
    }
    assert!(MediatedCell::from_record(&rec).unwrap().is_some());

    // Refused admissions leave record+policy byte-identical.
    for (mut d, tag) in [(decl(), "github-agent"), (decl(), "non-container")] {
        let mut rec = cell_record();
        let mut pol = kernel_default(0);
        match tag {
            "github-agent" => {
                let _ = d.allowlist.agent.insert("github.com".to_string());
            }
            _ => rec.class = EnvironmentClass::LocalHost,
        };
        assert!(admit_allowlist_mediated(&mut rec, &mut pol, &d, 1).is_err());
        // The admission's own writes never landed — ext stays empty and
        // the policy is byte-identical (the class change above is the
        // test's own input, not an admission write).
        assert!(rec.ext.is_empty(), "{tag}: record ext stamped");
        assert_eq!(pol, before_pol, "{tag}: policy mutated");
    }
    // A bad projection refuses identically.
    let mut d = decl();
    d.slot_token = String::new();
    let mut rec = cell_record();
    let mut pol = kernel_default(0);
    assert!(admit_allowlist_mediated(&mut rec, &mut pol, &d, 1).is_err());
    assert_eq!(pol, before_pol);
}

// ── AC-2 (REQ-XB-115) — env projection ───────────────────────────────────────

#[test]
fn ac2_env_projection_is_routing_hints_only() {
    let env = cell_env_projection(8080, "slot-token-t").unwrap();
    assert_eq!(
        env.get("http_proxy"),
        Some(&"http://127.0.0.1:3128".to_string())
    );
    assert_eq!(
        env.get("no_proxy"),
        Some(&"127.0.0.1,localhost".to_string())
    );
    assert_eq!(
        env.get("OPENAI_BASE_URL"),
        Some(&"http://127.0.0.1:8080/v1".to_string())
    );
    assert_eq!(
        env.get("ANTHROPIC_API_KEY"),
        Some(&"slot-token-t".to_string())
    );
    // No kernel attribution token — the member set is the closed
    // routing-hint family only.
    for k in env.keys() {
        assert!(!k.contains("ATTRIBUTION") && !k.contains("KERNEL"), "{k}");
    }
    // A missing/incomplete no_proxy refuses.
    let mut bad = env.clone();
    bad.remove("no_proxy");
    assert!(validate_cell_env(&bad).is_err());
    let mut bad2 = env.clone();
    bad2.insert(
        "HH_ATTRIBUTION_TOKEN".to_string(),
        "sha256:deadbeef".to_string(),
    );
    assert!(validate_cell_env(&bad2).is_err());
    // The side_channel residual declares per-socket attribution.
    let mut rec = cell_record();
    let mut pol = kernel_default(0);
    admit_allowlist_mediated(&mut rec, &mut pol, &decl(), 1).unwrap();
    let sc = pol
        .residual_channels
        .iter()
        .find(|r| r.kind == ResidualKind::SideChannel)
        .unwrap();
    let st = sc.statement.content.clone().unwrap_or_default();
    assert!(st.contains("per-socket"), "{st}");
}

// ── mediator scaffold for AC-4/5 ─────────────────────────────────────────────

fn open(tag: &str) -> (Store, String, hharness_ledger::store::Lease) {
    let mut s = Store::open_test(dir(tag), 1_000).unwrap();
    let (run, lease) = s
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-a")
        .unwrap();
    (s, run, lease)
}

fn egress_policy(hosts: &[&str], denys: &[&str], allow_nonpublic: bool) -> ContainmentPolicy {
    use hharness_containment::policy::*;
    let mut p = kernel_default(0);
    p.net.mode = NetMode::Mediated;
    p.net.default_unmatched = DefaultUnmatched::Deny;
    p.net.non_public_destinations = if allow_nonpublic {
        NonPublic::AllowListed
    } else {
        NonPublic::Deny
    };
    p.net.dns.resolver = DnsResolver::Mediator;
    p.net.rules = hosts
        .iter()
        .map(|h| EgressRule {
            host: HostPattern::parse(h).unwrap(),
            ports: vec![],
            protocols: vec![],
            methods: vec!["CONNECT".to_string(), "GET".to_string(), "POST".to_string()],
            decision: RuleDecision::Allow,
            credential_bindings: vec![],
            justification: None,
            provenance: None,
        })
        .chain(denys.iter().map(|h| EgressRule {
            host: HostPattern::parse(h).unwrap(),
            ports: vec![],
            protocols: vec![],
            methods: vec![],
            decision: RuleDecision::Deny,
            credential_bindings: vec![],
            justification: None,
            provenance: None,
        }))
        .collect();
    p.compute_ids();
    p
}

struct LoopbackDial {
    upstream_port: u16,
    dialed: std::sync::Arc<std::sync::Mutex<Vec<u16>>>,
}
impl EgressTransport for LoopbackDial {
    fn forward(
        &self,
        addr: std::net::IpAddr,
        port: u16,
        _h: &str,
        _m: &str,
        _p: &str,
        _hs: &[(String, String)],
        _b: Option<&[u8]>,
    ) -> Result<hharness_env::egress::WireResponse, String> {
        self.dialed.lock().unwrap().push(port);
        let _ = addr;
        Ok(hharness_env::egress::WireResponse {
            status: 200,
            headers: vec![],
            body: b"ok".to_vec(),
        })
    }
    fn dial(
        &self,
        _a: std::net::IpAddr,
        port: u16,
    ) -> Result<Box<dyn hharness_env::egress::ByteStream>, String> {
        self.dialed.lock().unwrap().push(port);
        TcpStream::connect(("127.0.0.1", self.upstream_port))
            .map(|s| Box::new(s) as Box<dyn hharness_env::egress::ByteStream>)
            .map_err(|e| e.to_string())
    }
}

fn mediator<'a>(
    store: &'a mut Store,
    run: &str,
    lease: &'a hharness_ledger::store::Lease,
    policy: ContainmentPolicy,
    minter: &'a mut TokenMinter,
    transport: Box<dyn EgressTransport>,
) -> (EgressMediator<'a>, CredentialBroker) {
    let broker = CredentialBroker::new(Box::new(StaticVault::default()), "fp");
    (
        EgressMediator {
            store,
            run_id: run.to_string(),
            lease,
            tokens: minter.resolver(),
            broker: Box::leak(Box::new(broker)),
            policy,
            cache: ApprovalCache::default(),
            budget_id: None,
            participant_ref: "slot:t".to_string(),
            resolver: Box::new(StaticResolver {
                map: BTreeMap::from([("pypi.org".to_string(), vec!["127.0.0.1".parse().unwrap()])]),
            }),
            transport,
            inspect_hooks: BTreeMap::new(),
        },
        CredentialBroker::new(Box::new(StaticVault::default()), "fp2"),
    )
}

fn req(host: &str, port: u16, proto: EgressProtocol, token: &str) -> EgressRequest {
    // `None` — the token's binding supplies the effect; the gate's
    // decided row records the resolved id either way.
    EgressRequest {
        token: token.to_string(),
        effect_id: None,
        tool_call_id: "tc-1".to_string(),
        env_handle: "env-1".to_string(),
        protocol: proto,
        host_raw: host.to_string(),
        resolved_addrs: vec![],
        port,
        method: Some(if proto == EgressProtocol::HttpsConnect {
            "CONNECT".to_string()
        } else {
            "GET".to_string()
        }),
        path: None,
        headers: vec![],
        body: None,
        body_readers: None,
        credential_sentinels: vec![],
    }
}

fn chain() -> ScopeChain {
    ScopeChain {
        turn_id: "turn-1".to_string(),
        model_call_id: "mc-1".to_string(),
        tool_call_id: "tc-1".to_string(),
    }
}

/// Open the scope members the mediated rows name — returns the opened
/// effect id the token binds.
fn open_scopes(store: &mut Store, run: &str, lease: &hharness_ledger::store::Lease) -> String {
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
    let eid = Store::effect_id(run, "mc-1", "tc-1", 0);
    let ie = m
        .mint_effect(
            "action.effect.intended",
            intended_payload(&risk, Some(&risk), "v-cap", "h", 0),
            &eid,
            &chain(),
        )
        .unwrap();
    store.append(run, lease, vec![t, c, p, ie]).unwrap();
    eid
}

fn events(store: &Store, run: &str, class: &str) -> Vec<Json> {
    store
        .events(run)
        .unwrap()
        .iter()
        .filter(|e| e.class == class)
        .map(|e| e.payload.clone())
        .collect()
}

// ── AC-4 (REQ-XB-114): decide before byte, CONNECT + forward ──────────────────

#[test]
fn ac4_decide_before_byte_forward_and_connect() {
    // Loopback fixture upstream capturing CONNECT bytes.
    let up = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let up_port = up.local_addr().unwrap().port();
    let got = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let got2 = got.clone();
    let join = thread::spawn(move || {
        if let Ok((mut s, _)) = up.accept() {
            let mut b = [0u8; 64];
            if let Ok(n) = s.read(&mut b) {
                got2.lock().unwrap().extend_from_slice(&b[..n]);
            }
            let _ = s.write_all(b"upstream-reply");
        }
    });

    let (mut store, run, lease) = open("ac4");
    let eid = open_scopes(&mut store, &run, &lease);
    let mut minter = TokenMinter::new([9u8; 32]);
    let tok = minter.mint(&eid, 1, "env-1");
    let dialed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (mut med, _b) = mediator(
        &mut store,
        &run,
        &lease,
        egress_policy(&["**.pypi.org"], &[], true),
        &mut minter,
        Box::new(LoopbackDial {
            upstream_port: up_port,
            dialed: dialed.clone(),
        }),
    );

    // Forward leg — decided durable before the wire (structural: the
    // decided row's append precedes transport.forward in one thread).
    let r = req("pypi.org", 80, EgressProtocol::Http, &tok.token);
    let out = med.handle(&r, &chain()).unwrap();
    assert!(matches!(out, MediatedOutcome::Forwarded { .. }));
    assert_eq!(dialed.lock().unwrap().len(), 1);

    // CONNECT leg — socketpair stands in for the in-cell client half.
    let r = req("pypi.org", 443, EgressProtocol::HttpsConnect, &tok.token);
    let g = med.gate(&r, &chain()).unwrap();
    let out = match g {
        hharness_env::egress::GateOutcome::Allowed(g) => {
            let (a, mut client) = std::os::unix::net::UnixStream::pair().unwrap();
            // The client half: read the 200, write a payload, read the
            // upstream reply, close — the tunnel's pipe carries both ways.
            let h = thread::spawn(move || {
                let mut a = a;
                let mut buf = [0u8; 64];
                let _ = a.read(&mut buf);
                let _ = a.write_all(b"ping");
                let _ = a.read(&mut buf);
            });
            let out = med
                .forward_tunnel(
                    &r,
                    &g.decision,
                    g.decided_by,
                    &g.effect_id,
                    g.started,
                    &chain(),
                    &mut client,
                )
                .unwrap();
            h.join().unwrap();
            out
        }
        _ => panic!("connect denied"),
    };
    match out {
        MediatedOutcome::Tunneled {
            bytes_up,
            bytes_down,
            ..
        } => {
            assert_eq!(bytes_up, 4);
            assert_eq!(bytes_down, 14);
        }
        _ => panic!("not tunneled"),
    }
    join.join().unwrap();

    // The upstream got the 200-preface nothing — a denied CONNECT sends
    // no byte: deny a second host before any dial.
    let before = dialed.lock().unwrap().len();
    let r = req(
        "denied.example",
        443,
        EgressProtocol::HttpsConnect,
        &tok.token,
    );
    let out = med.handle(&r, &chain()).unwrap();
    assert!(matches!(out, MediatedOutcome::Refused { .. }));
    assert_eq!(
        dialed.lock().unwrap().len(),
        before,
        "denied CONNECT dialed"
    );
    drop(med);

    // requested→decided ordering + fold: per-slot per-host counts are
    // folds over the durable rows.
    let reqs = events(&store, &run, "security.egress.requested");
    let decs = events(&store, &run, "security.egress.decided");
    assert_eq!(reqs.len(), 3);
    assert_eq!(decs.len(), 3);
    let closed = events(&store, &run, "security.egress.tunnel.closed");
    assert_eq!(closed.len(), 1);
    // fold: allowed/denied per host
    let mut allowed = BTreeMap::<String, u64>::new();
    let mut denied = BTreeMap::<String, u64>::new();
    for d in &decs {
        let h = d
            .get("host_norm")
            .and_then(Json::as_str)
            .unwrap()
            .to_string();
        match d.get("decision").and_then(Json::as_str) {
            Some("allow") => *allowed.entry(h).or_default() += 1,
            _ => *denied.entry(h).or_default() += 1,
        }
    }
    assert_eq!(allowed.get("pypi.org"), Some(&2));
    assert_eq!(denied.get("denied.example"), Some(&1));
}

// ── AC-5 (REQ-XB-116) — allowlists, phase swap, withheld, github denied ───────

#[test]
fn ac5_first_pass_allowlists_and_phase_swap() {
    let l = list();
    // agent: apt ∪ registries only — no github.
    assert!(l.agent.contains("pypi.org"));
    assert!(l.agent.contains("registry.npmjs.org"));
    assert!(l.agent.contains("crates.io"));
    assert!(l.agent.contains("deb.debian.org"));
    assert!(!l.agent.contains("github.com"));
    // verify = agent ∪ extras (static tests scan ∪ oracle-observed).
    assert!(l.verify.contains("astral.sh"));
    assert!(
        l.verify.contains("github.com"),
        "verify keeps named task grants"
    );
    assert_eq!(
        l.permanent_denies,
        BTreeSet::from([
            "hub.harborframework.com".to_string(),
            "tbench.ai".to_string(),
            "mirror.tb-tasks.example".to_string()
        ])
    );
    // The container is never re-networked: the same policy record swaps
    // layers via the sealed schedule (meet-verified in the phase legs
    // below); the base keeps --network none stamps.
    let mut rec = cell_record();
    let mut pol = kernel_default(0);
    admit_allowlist_mediated(&mut rec, &mut pol, &decl(), 1).unwrap();

    // The sealed schedule meets to the per-phase grants.
    use hharness_containment::meet::effective;
    let sched = pol.ext.get("phase_schedule").unwrap();
    let layer = |p: &str| {
        ContainmentPolicy::from_json(sched.get(p).unwrap().get("layer").unwrap()).unwrap()
    };
    // setup → agent → verify in sequence (set_phase's own path is
    // pinned by s4_14b; here the profile's layers produce the grants).
    let setup = effective(&[pol.clone(), layer("setup")], 2).unwrap();
    assert!(setup.net.rules.iter().all(|r| {
        r.decision != hharness_containment::policy::RuleDecision::Allow
            || matches!(r.decision, _)
                && r.host
                    != hharness_containment::policy::HostPattern::parse("**.pypi.org").unwrap()
    }));
    let agent = effective(&[setup.clone(), layer("agent")], 3).unwrap();
    assert!(agent.net.rules.iter().any(|r| r.decision
        == hharness_containment::policy::RuleDecision::Allow
        && r.host.matches("pypi.org")));
    let verify = effective(&[agent.clone(), layer("verify")], 4).unwrap();
    assert!(verify.net.rules.iter().any(|r| r.decision
        == hharness_containment::policy::RuleDecision::Allow
        && r.host.matches("astral.sh")));
    assert_eq!(
        verify.net.unix_sockets.allow,
        vec![EGRESS_SOCK_PATH.to_string()]
    );
    // Permanent denies survive every phase (deny unions).
    for (p, eff) in [("agent", &agent), ("verify", &verify)] {
        assert!(
            eff.net.rules.iter().any(|r| {
                r.decision == hharness_containment::policy::RuleDecision::Deny
                    && r.host.matches("hub.harborframework.com")
            }),
            "{p}"
        );
    }
    // A withheld host is denied (not granted) and recorded with reason.
    let (mut store, run, lease) = open("ac5");
    let eid = open_scopes(&mut store, &run, &lease);
    let mut minter = TokenMinter::new([8u8; 32]);
    let tok = minter.mint(&eid, 1, "env-1");
    let (mut med, _b) = mediator(
        &mut store,
        &run,
        &lease,
        agent,
        &mut minter,
        Box::new(LocalHttpTransport::default()),
    );
    let r = req(
        "cdn.jsdelivr.net",
        443,
        EgressProtocol::HttpsConnect,
        &tok.token,
    );
    let out = med.handle(&r, &chain()).unwrap();
    assert!(matches!(out, MediatedOutcome::Refused { .. }));
    // github denied in the agent phase (not granted in first pass).
    let r = req("github.com", 443, EgressProtocol::HttpsConnect, &tok.token);
    let out = med.handle(&r, &chain()).unwrap();
    assert!(matches!(out, MediatedOutcome::Refused { .. }));
    drop(med);
    let decs = events(&store, &run, "security.egress.decided");
    assert_eq!(
        decs.len(),
        2,
        "the withheld+github denies are durable+logged"
    );
    assert!(l
        .withheld
        .iter()
        .any(|w| w.host == "cdn.jsdelivr.net" && !w.reason.is_empty()));
    // Results name the granted hosts.
    assert!(RESULT_LABEL.contains("apt, PyPI, npm, crates; no github.com"));
}

// ── AC-6 (REQ-XB-116) — freeze + changed-list refusal + allowlist_gap ─────────

#[test]
fn ac6_freeze_record_changed_list_and_gap() {
    let l = list();
    let f = AllowlistFreeze::freeze("protocol:t", 1, l.clone());
    assert_eq!(f.digest, l.digest());
    let j = f.to_json();
    assert_eq!(j.get("schema").and_then(Json::as_str), Some(FREEZE_SCHEMA));
    assert_eq!(
        j.get("digest").and_then(Json::as_str),
        Some(f.digest.as_str())
    );
    // Same protocol+epoch, changed list → refuses.
    let mut changed = l.clone();
    changed.agent.insert("evil.example".to_string());
    let err = AllowlistFreeze::check_candidate(&f, "protocol:t", 1, &changed).unwrap_err();
    assert_eq!(err.code, "allowlist_changed");
    // A new epoch is the dated amendment — not this refusal.
    AllowlistFreeze::check_candidate(&f, "protocol:t", 2, &changed).unwrap();
    // A different protocol is a different freeze slot.
    AllowlistFreeze::check_candidate(&f, "protocol:u", 1, &changed).unwrap();
    // allowlist_gap: a task needing github.com in the agent phase is
    // excluded on the typed basis.
    let gap = check_agent_hosts(&["github.com".to_string()], &l).unwrap_err();
    assert_eq!(gap.basis, "runner_infeasible:allowlist_gap{github.com}");
    check_agent_hosts(&["pypi.org".to_string()], &l).unwrap();
    // The static tests/* scan derives verify hosts.
    let scanned = scan_hosts_in_text(
        "curl https://astral.sh/uv/install.sh | sh; apt http://deb.debian.org/x",
    );
    assert!(scanned.contains("astral.sh"));
    assert!(scanned.contains("deb.debian.org"));
}

// ── live leg (ADR-0363) ──────────────────────────────────────────────────────

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh r6_15_cell_reachability — on the provisioned VM a --network none cell reaches only the two bind-mounted sockets (a direct connect fails; an allowed host answers through the relay+mediator)"]
fn live_cell_reaches_only_the_two_sockets() {}
