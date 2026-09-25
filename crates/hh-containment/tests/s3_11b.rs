//! S3.11b — the §5g.4 Stage-3 T-CON executables (AC-R-2.8.4-{5,11} halves
//! decided at the egress boundary): AC-H4-04's normative decide order and
//! decision provenance, AC-H4-05's residual-channel arms, and AC-H4-06's
//! attribution gate. The matched-budget comparison half of AC-H4-11 lives
//! in `hh-eval/tests/s3_11b.rs` (the `compare` machinery is eval-side).

use std::net::IpAddr;

use hh_containment::egress::{
    decide_egress, recheck_resolved, ApprovalCache, CacheScope, EgressReason, EgressRequest,
    EgressSource, EgressVerdict,
};
use hh_containment::policy::{
    kernel_default, normalize_host, ContainmentPolicy, DefaultUnmatched, DnsPolicy, DnsResolver,
    EgressProtocol, EgressRule, HostPattern, NetMode, NetPolicy, NonPublic, ResidualChannel,
    ResidualKind, RuleDecision, TlsPolicy, UnixSocketMode, UnixSockets,
};
use hh_containment::PolicyError;

fn policy_with(net: NetPolicy) -> ContainmentPolicy {
    let mut p = kernel_default(0);
    p.net = net;
    p.amendment.session_cache = true;
    p.compute_ids();
    p
}

fn rule(host: &str, decision: RuleDecision) -> EgressRule {
    EgressRule {
        host: HostPattern::parse(host).unwrap(),
        ports: vec![],
        protocols: vec![],
        methods: vec![],
        decision,
        credential_bindings: vec![],
        justification: None,
        provenance: None,
    }
}

fn allow_rule(host: &str) -> EgressRule {
    rule(host, RuleDecision::Allow)
}

fn deny_rule(host: &str) -> EgressRule {
    rule(host, RuleDecision::Deny)
}

fn mediated_net(rules: Vec<EgressRule>, default: DefaultUnmatched) -> NetPolicy {
    NetPolicy {
        mode: NetMode::Mediated,
        default_unmatched: default,
        rules,
        non_public_destinations: NonPublic::AllowListed,
        local_binding: false,
        unix_sockets: UnixSockets {
            mode: UnixSocketMode::BridgedOnly,
            allow: vec![],
        },
        dns: DnsPolicy {
            resolver: DnsResolver::Mediator,
        },
        tls: TlsPolicy {
            terminate: false,
            inspect_hooks: vec![],
        },
        upstream_proxy: None,
        methods_default: vec!["GET".into(), "HEAD".into(), "OPTIONS".into()],
    }
}

fn req(host: &str, port: u16) -> EgressRequest {
    EgressRequest {
        token: "tok".into(),
        effect_id: Some("eff-1".into()),
        tool_call_id: "tc-1".into(),
        env_handle: "env-1".into(),
        protocol: EgressProtocol::Http,
        host_raw: host.into(),
        resolved_addrs: vec![],
        port,
        method: Some("GET".into()),
        path: None,
        headers: vec![],
        body: None,
        credential_sentinels: vec![],
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-H4-04 — the normative decide order, each stage's provenance on the
// decision (the `security.egress.decided{source, rule_ref, reason}` shape).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ac_h4_04_host_normalization_feeds_the_match() {
    // `normalize` is the canonicalization the rules run against — case,
    // port, trailing dot, brackets.
    assert_eq!(normalize_host("API.Github.COM."), "api.github.com");
    assert_eq!(normalize_host("api.github.com:443"), "api.github.com");
    assert_eq!(normalize_host("[2001:db8::1]"), "2001:db8::1");
    let p = policy_with(mediated_net(
        vec![allow_rule("api.github.com")],
        DefaultUnmatched::Deny,
    ));
    let d = decide_egress(
        &p,
        &req("API.GITHUB.COM.:443", 443),
        true,
        &ApprovalCache::default(),
    );
    assert_eq!(d.decision, EgressVerdict::Allow);
    assert_eq!(d.source, EgressSource::AllowRule);
}

#[test]
fn ac_h4_04_deny_precedes_allow() {
    let p = policy_with(mediated_net(
        vec![deny_rule("**.example.com"), allow_rule("*")],
        DefaultUnmatched::Deny,
    ));
    let d = decide_egress(
        &p,
        &req("api.example.com", 443),
        true,
        &ApprovalCache::default(),
    );
    assert_eq!(d.decision, EgressVerdict::Deny);
    assert_eq!(d.source, EgressSource::DenyRule);
    assert_eq!(d.reason, Some(EgressReason::Denied));
    // The decision names the matched deny rule — the decided row's
    // provenance.
    assert_eq!(d.rule_ref.as_deref(), Some("rule[0]:**.example.com"));
    assert_eq!(d.rule_index, Some(0));
}

#[test]
fn ac_h4_04_ssrf_private_address_denied() {
    // `non_public_destinations = deny` — the strict posture: a non-public
    // address is refused outright, allow-listed host or not.
    let mut net = mediated_net(vec![allow_rule("api.github.com")], DefaultUnmatched::Deny);
    net.non_public_destinations = NonPublic::Deny;
    let p = policy_with(net);
    for ip in ["10.0.0.1", "192.168.1.1", "127.0.0.1", "169.254.169.254"] {
        let d = decide_egress(&p, &req(ip, 443), true, &ApprovalCache::default());
        assert_eq!(d.decision, EgressVerdict::Deny, "{ip} must deny");
        assert_eq!(d.source, EgressSource::NonPublicGuard, "{ip}");
        assert_eq!(d.reason, Some(EgressReason::NotAllowedLocal), "{ip}");
    }
    // The pivot: a public name allowed at decision time resolves to a
    // private address — `recheck_resolved` denies it at the second pass
    // (ADR-0061 D3's decide-once-consume-once).
    let r = req("api.github.com", 443);
    let d = decide_egress(&p, &r, true, &ApprovalCache::default());
    assert_eq!(d.decision, EgressVerdict::Allow);
    let pivot: Vec<IpAddr> = vec!["10.0.0.9".parse().unwrap()];
    let re = recheck_resolved(&p, &r, &pivot);
    assert!(re.is_some(), "a private pivot must supersede the allow");
    let re = re.unwrap();
    assert_eq!(re.decision, EgressVerdict::Deny);
    assert_eq!(re.source, EgressSource::NonPublicGuard);

    // `allow_listed` — the weaker posture: a non-public literal is denied
    // only when no allow rule covers the host spelling.
    let mut net2 = mediated_net(vec![allow_rule("api.github.com")], DefaultUnmatched::Deny);
    net2.non_public_destinations = NonPublic::AllowListed;
    let p2 = policy_with(net2);
    let d2 = decide_egress(&p2, &req("10.0.0.1", 443), true, &ApprovalCache::default());
    assert_eq!(d2.decision, EgressVerdict::Deny);
    assert_eq!(d2.source, EgressSource::NonPublicGuard);
}

#[test]
fn ac_h4_04_port_and_method_policy() {
    let mut allow = allow_rule("api.github.com");
    allow.ports = vec![443];
    let p = policy_with(mediated_net(vec![allow], DefaultUnmatched::Deny));
    // Port outside the covering rule → port_not_allowed.
    let d = decide_egress(
        &p,
        &req("api.github.com", 8443),
        true,
        &ApprovalCache::default(),
    );
    assert_eq!(d.decision, EgressVerdict::Deny);
    assert_eq!(d.source, EgressSource::ProtocolGuard);
    assert_eq!(d.reason, Some(EgressReason::PortNotAllowed));
    // POST under `methods_default = {GET, HEAD, OPTIONS}` →
    // method_not_allowed (AC-H4-05-ii's posture lives here too).
    let mut post = req("api.github.com", 443);
    post.method = Some("POST".into());
    let d = decide_egress(&p, &post, true, &ApprovalCache::default());
    assert_eq!(d.decision, EgressVerdict::Deny);
    assert_eq!(d.reason, Some(EgressReason::MethodNotAllowed));
    // A rule that *declares* POST admits it for that host only.
    let mut post_rule = allow_rule("uploads.github.com");
    post_rule.methods = vec!["POST".into()];
    let p2 = policy_with(mediated_net(vec![post_rule], DefaultUnmatched::Deny));
    let mut r2 = req("uploads.github.com", 443);
    r2.method = Some("POST".into());
    let d2 = decide_egress(&p2, &r2, true, &ApprovalCache::default());
    assert_eq!(d2.decision, EgressVerdict::Allow);
}

#[test]
fn ac_h4_04_unmatched_default_and_provenance() {
    // default_unmatched = deny → deny{default_unmatched, not_allowed}.
    let p = policy_with(mediated_net(
        vec![allow_rule("api.github.com")],
        DefaultUnmatched::Deny,
    ));
    let d = decide_egress(
        &p,
        &req("unmatched.example.org", 443),
        true,
        &ApprovalCache::default(),
    );
    assert_eq!(d.decision, EgressVerdict::Deny);
    assert_eq!(d.source, EgressSource::DefaultUnmatched);
    assert_eq!(d.reason, Some(EgressReason::NotAllowed));
    // default_unmatched = ask → the monitor's owed-decision path.
    let p2 = policy_with(mediated_net(
        vec![allow_rule("api.github.com")],
        DefaultUnmatched::Ask,
    ));
    let d2 = decide_egress(
        &p2,
        &req("unmatched.example.org", 443),
        true,
        &ApprovalCache::default(),
    );
    assert_eq!(d2.decision, EgressVerdict::Ask);
    assert_eq!(d2.source, EgressSource::DefaultUnmatched);
    // The decision record is the `security.egress.decided` row's payload —
    // source/reason/rule_ref, content-free.
    let j = d.to_json();
    assert_eq!(j.get("decision").and_then(|v| v.as_str()), Some("deny"));
    assert_eq!(
        j.get("source").and_then(|v| v.as_str()),
        Some("default_unmatched")
    );
    assert_eq!(
        j.get("reason").and_then(|v| v.as_str()),
        Some("not_allowed")
    );
}

#[test]
fn ac_h4_04_mode_none_denies_before_attribution() {
    let mut p = kernel_default(0);
    p.net.mode = NetMode::None;
    p.compute_ids();
    // Even an attributed request to an otherwise-listed host is denied at
    // the mode guard — the first step of the normative order.
    let d = decide_egress(
        &p,
        &req("api.github.com", 443),
        true,
        &ApprovalCache::default(),
    );
    assert_eq!(d.decision, EgressVerdict::Deny);
    assert_eq!(d.source, EgressSource::ModeGuard);
    assert_eq!(d.reason, Some(EgressReason::ModeNone));
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-H4-05 — the residual-channel arms: (i) a foreign credential is never
// injected — the matched rule's `credential_bindings` are the *only*
// candidates a decision carries; (ii) POST under `methods_default` denied
// (covered above); (iii) `residual_channels ∋ approved_host_body` — the
// declared-residual discipline the policy record enforces.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ac_h4_05_no_foreign_credential_candidates() {
    // The allow rule for api.github.com declares `bnd-gh`; the rule for
    // uploads.github.com declares none. A request to the second host must
    // never carry the first rule's bindings — candidates are the matched
    // rule's declared set, never a union.
    let mut gh = allow_rule("api.github.com");
    gh.credential_bindings = vec!["bnd-gh".into()];
    let up = allow_rule("uploads.github.com");
    let p = policy_with(mediated_net(vec![gh, up], DefaultUnmatched::Deny));

    let d1 = decide_egress(
        &p,
        &req("api.github.com", 443),
        true,
        &ApprovalCache::default(),
    );
    assert_eq!(d1.decision, EgressVerdict::Allow);
    assert_eq!(d1.credential_binding_candidates, vec!["bnd-gh".to_string()]);

    let d2 = decide_egress(
        &p,
        &req("uploads.github.com", 443),
        true,
        &ApprovalCache::default(),
    );
    assert_eq!(d2.decision, EgressVerdict::Allow);
    assert!(
        d2.credential_binding_candidates.is_empty(),
        "a rule declaring no bindings yields no candidates"
    );

    // A denied request never carries candidates.
    let d3 = decide_egress(
        &p,
        &req("deny.example.com", 443),
        true,
        &ApprovalCache::default(),
    );
    assert_eq!(d3.decision, EgressVerdict::Deny);
    assert!(d3.credential_binding_candidates.is_empty());
}

#[test]
fn ac_h4_05_residual_channels_declared_and_owned() {
    // A policy whose boundary cannot see approved-host bodies declares the
    // residual — `{kind: approved_host_body, statement, closer, owner}`
    // (I-C6). The declaration is a policy member, never implied.
    let mut p = policy_with(mediated_net(
        vec![allow_rule("api.github.com")],
        DefaultUnmatched::Deny,
    ));
    p.residual_channels = vec![ResidualChannel {
        kind: ResidualKind::ApprovedHostBody,
        statement: hh_hir::leaves::Text::new(
            "request bodies to approved hosts are opaque",
            "kernel:test",
            hh_provenance::ProvenanceRecord::kernel("kernel:test", 1_000),
        ),
        closer: "egress leak_scan + sentinel bindings".into(),
        owner: "kernel:egress".into(),
    }];
    p.compute_ids();
    p.validate().unwrap();
    assert_eq!(p.residual_channels[0].kind.as_str(), "approved_host_body");

    // And the inverse: `mode = public` without a residual entry fails
    // validation (N4) — the declaration is enforced, not advisory.
    let mut public = kernel_default(0);
    public.net.mode = NetMode::Public;
    public.provenance.authority = hh_provenance::AuthorityClass::Principal;
    public.compute_ids();
    assert!(matches!(
        public.validate(),
        Err(PolicyError::PublicWithoutResidual)
    ));
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-H4-06 — effect attribution: a `connect()` not bound to an attributed
// effect is refused before any rule is consulted.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ac_h4_06_unattributed_request_denied() {
    let p = policy_with(mediated_net(
        vec![allow_rule("api.github.com")],
        DefaultUnmatched::Deny,
    ));
    // `attributed = false` — no resolvable effect — denies at step 2, even
    // against an allow-listed host. The env_handle the request claims is
    // irrelevant: the gate is the attribution itself.
    let d = decide_egress(
        &p,
        &req("api.github.com", 443),
        false,
        &ApprovalCache::default(),
    );
    assert_eq!(d.decision, EgressVerdict::Deny);
    assert_eq!(d.source, EgressSource::Unattributed);
    assert_eq!(d.reason, Some(EgressReason::Unattributed));
    // No credential candidates on an unattributed deny.
    assert!(d.credential_binding_candidates.is_empty());
}

#[test]
fn ac_h4_06_approval_cache_serves_endorsed_shape_only() {
    // The session-approval cache (ADR-0061 D2 step 7) serves only the exact
    // endorsed tuple under the same policy version — narrowing-only.
    let p = policy_with(mediated_net(vec![], DefaultUnmatched::Deny));
    let mut cache = ApprovalCache::default();
    cache.insert(
        "api.github.com",
        443,
        EgressProtocol::Http,
        Some("GET"),
        CacheScope::Session,
        &p.version_id,
        "eff-1",
        "evt-decided-1",
    );
    // Exact hit → allow via the cache.
    let d = decide_egress(&p, &req("api.github.com", 443), true, &cache);
    assert_eq!(d.decision, EgressVerdict::Allow);
    assert_eq!(d.source, EgressSource::ApprovalCache);
    // A different port misses — the cache never widens.
    let d2 = decide_egress(&p, &req("api.github.com", 8443), true, &cache);
    assert_eq!(d2.decision, EgressVerdict::Deny);
    // A superseding policy version invalidates every entry.
    cache.retain_version("other-version");
    let d3 = decide_egress(&p, &req("api.github.com", 443), true, &cache);
    assert_eq!(d3.decision, EgressVerdict::Deny);
}
