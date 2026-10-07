//! R2.9b — the DF-S1.12-4/-6 containment halves: typed `LoweringLoss`
//! report rows (`{declared_field, backend, reason, kind, consequence}` —
//! preserving backend identity and the closed class tags), the
//! `net.proxy_env` / proxy-surface detection, and the `approved_host_body`
//! closer (the R-2.8.2 reader-set check on a declared residual — AC-H4-05
//! (iv); ADR-0341 D4–D6).

use hh_containment::egress::{
    approved_host_body_check, decide_egress, ApprovalCache, CacheScope, EgressReason,
    EgressRequest, EgressSource, EgressVerdict,
};
use hh_containment::policy::{
    kernel_default, ContainmentPolicy, DefaultUnmatched, DnsPolicy, DnsResolver, EgressProtocol,
    EgressRule, HostPattern, NetMode, NetPolicy, NonPublic, ResidualChannel, ResidualKind,
    RuleDecision, TlsPolicy, UnixSocketMode, UnixSockets,
};
use hh_containment::{ContainmentBackend, Ep2Model, LossConsequence, LossKind};
use hh_provenance::authority::ReaderSet;
use hh_provenance::ProvenanceRecord;

fn policy_with(net: NetPolicy) -> ContainmentPolicy {
    let mut p = kernel_default(0);
    p.net = net;
    p.amendment.session_cache = true;
    p.compute_ids();
    p
}

fn mediated_net(rules: Vec<EgressRule>) -> NetPolicy {
    NetPolicy {
        mode: NetMode::Mediated,
        default_unmatched: DefaultUnmatched::Deny,
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
        methods_default: vec!["GET".into(), "HEAD".into(), "OPTIONS".into(), "POST".into()],
    }
}

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

fn residual_body() -> ResidualChannel {
    ResidualChannel {
        kind: ResidualKind::ApprovedHostBody,
        statement: hh_hir::leaves::Text::new(
            "bodies to the approved host",
            "test",
            ProvenanceRecord::kernel("t", 0),
        ),
        closer: "reader_coverage".to_string(),
        owner: "test".to_string(),
    }
}

fn req(host: &str, body: Option<&str>, readers: Option<ReaderSet>) -> EgressRequest {
    EgressRequest {
        token: "tok".into(),
        effect_id: Some("eff-1".into()),
        tool_call_id: "tc-1".into(),
        env_handle: "env-1".into(),
        protocol: EgressProtocol::Http,
        host_raw: host.into(),
        resolved_addrs: vec![],
        port: 443,
        method: Some("POST".into()),
        path: None,
        headers: vec![],
        body: body.map(str::to_string),
        body_readers: readers,
        credential_sentinels: vec![],
    }
}

// ── typed lowering-loss rows (DF-S1.12-4) ─────────────────────────────────

#[test]
fn lowering_loss_rows_are_typed() {
    // A `none`-mode policy declaring every lossy member yields rows whose
    // JSON carries the full `{field, backend, reason, kind, consequence}`
    // member set — backend identity and the closed class tags preserved.
    let mut p = kernel_default(0);
    p.net.mode = NetMode::None;
    p.resources.cpu_ms = Some(10);
    p.proc.syscall_filter = Some("filter-1".into());
    p.net.tls.terminate = true;
    p.net.upstream_proxy = Some("proxy-1".into());
    p.compute_ids();
    let loss = Ep2Model::reference().lowering_loss(&p);
    assert!(!loss.is_empty());
    for l in &loss {
        let j = l.to_json();
        for m in ["field", "backend", "reason", "kind", "consequence"] {
            assert!(j.get(m).is_some(), "row missing member {m}: {j:?}");
        }
        assert_eq!(l.backend, "ep2_model");
    }
    // Under `none` the C2 members have no mediator — `fail_closed` rows.
    let tls = loss
        .iter()
        .find(|l| l.declared_field == "net.tls.terminate")
        .expect("tls.terminate loss under none");
    assert_eq!(tls.kind, LossKind::NoSlot);
    assert_eq!(tls.consequence, LossConsequence::FailClosed);
    // The proxy declaration is `hint_only`/`stratified` — recorded, never
    // an enforcement claim (§5g.4 §5's proxy row).
    let proxy = loss
        .iter()
        .find(|l| l.declared_field == "net.upstream_proxy")
        .expect("upstream_proxy loss");
    assert_eq!(proxy.kind, LossKind::HintOnly);
    assert_eq!(proxy.consequence, LossConsequence::Stratified);
}

#[test]
fn mediated_tls_members_stratify_to_the_mediator() {
    // Under `mediated` the C2 members discharge at EP3 — the rows record
    // `mediator_discharges`/`stratified`, never silently absent and never
    // fail-closed at attach (R2.9b; ADR-0341 D3).
    let mut p = policy_with(mediated_net(vec![]));
    p.net.tls.terminate = true;
    p.net.tls.inspect_hooks = vec!["hook-1".into()];
    p.compute_ids();
    let loss = Ep2Model::reference().lowering_loss(&p);
    for field in ["net.tls.terminate", "net.tls.inspect_hooks"] {
        let l = loss
            .iter()
            .find(|l| l.declared_field == field)
            .unwrap_or_else(|| panic!("missing mediated loss for {field}"));
        assert_eq!(l.reason, "mediator_discharges");
        assert_eq!(l.consequence, LossConsequence::Stratified);
    }
}

#[test]
fn proxy_env_surface_under_mediated_is_a_stratified_loss() {
    // `proc.env` carrying a proxy variable under `mediated` — the
    // "env vars mistaken for mediation" shape; `net.proxy_env`'s runtime
    // half is the recorded `proc.env` row (ADR-0341 D4).
    let mut p = policy_with(mediated_net(vec![]));
    p.proc
        .env
        .set
        .insert("HTTPS_PROXY".into(), "http://proxy:3128".into());
    p.compute_ids();
    let loss = Ep2Model::reference().lowering_loss(&p);
    let env = loss
        .iter()
        .find(|l| l.declared_field == "proc.env")
        .expect("proc.env proxy-surface loss");
    assert_eq!(env.reason, "proxy_env_is_not_mediation");
    assert_eq!(env.consequence, LossConsequence::Stratified);
    // `include_only` naming no proxy variable closes the surface even
    // under `inherit = all` — every ambient proxy name is unadmitted.
    let mut q = policy_with(mediated_net(vec![]));
    q.proc.env.inherit = hh_containment::policy::EnvInherit::All;
    q.proc.env.include_only = vec!["PATH".into()];
    let loss2 = Ep2Model::reference().lowering_loss(&q);
    assert!(!loss2.iter().any(|l| l.declared_field == "proc.env"));
    // And `inherit = all` alone (no `include_only`, no `exclude`) leaves
    // the ambient surface open.
    let mut r = policy_with(mediated_net(vec![]));
    r.proc.env.inherit = hh_containment::policy::EnvInherit::All;
    let loss3 = Ep2Model::reference().lowering_loss(&r);
    assert!(loss3.iter().any(|l| l.declared_field == "proc.env"));
}

// ── the approved_host_body closer (DF-S1.12-6; AC-H4-05 iv) ────────────────

#[test]
fn declared_body_channel_requires_reader_coverage() {
    let mut p = policy_with(mediated_net(vec![allow_rule("api.example.com")]));
    p.residual_channels.push(residual_body());
    p.compute_ids();

    // Covered readers — the destination is named.
    let d = decide_egress(
        &p,
        &req(
            "api.example.com",
            Some("payload"),
            Some(ReaderSet::Restricted(
                ["api.example.com".to_string()].into_iter().collect(),
            )),
        ),
        true,
        &ApprovalCache::default(),
    );
    assert_eq!(d.decision, EgressVerdict::Allow, "{d:?}");

    // `Public` readers trivially cover.
    let d = decide_egress(
        &p,
        &req("api.example.com", Some("payload"), Some(ReaderSet::Public)),
        true,
        &ApprovalCache::default(),
    );
    assert_eq!(d.decision, EgressVerdict::Allow);

    // A restricted set not naming the host refuses `reader_coverage`.
    let d = decide_egress(
        &p,
        &req(
            "api.example.com",
            Some("payload"),
            Some(ReaderSet::Restricted(
                ["other.example.com".to_string()].into_iter().collect(),
            )),
        ),
        true,
        &ApprovalCache::default(),
    );
    assert_eq!(d.decision, EgressVerdict::Deny);
    assert_eq!(d.source, EgressSource::FlowGuard);
    assert_eq!(d.reason, Some(EgressReason::ReaderCoverage));
    assert_eq!(d.guard_detail.as_deref(), Some("approved_host_body"));

    // An unlabeled body is unproven coverage — refuses closed.
    let d = decide_egress(
        &p,
        &req("api.example.com", Some("payload"), None),
        true,
        &ApprovalCache::default(),
    );
    assert_eq!(d.decision, EgressVerdict::Deny);
    assert_eq!(d.reason, Some(EgressReason::ReaderCoverage));

    // No body — no channel instance; the allow stands.
    let d = decide_egress(
        &p,
        &req("api.example.com", None, None),
        true,
        &ApprovalCache::default(),
    );
    assert_eq!(d.decision, EgressVerdict::Allow);
}

#[test]
fn undeclared_channel_is_a_readiness_gap_not_a_verdict() {
    // The closer closes *declared* channels — a policy that never declared
    // `approved_host_body` does not gain a runtime verdict for a body leg
    // (the C0 assertion is a policy-text/readiness matter; ADR-0341 D6).
    let p = policy_with(mediated_net(vec![allow_rule("api.example.com")]));
    let d = decide_egress(
        &p,
        &req("api.example.com", Some("payload"), None),
        true,
        &ApprovalCache::default(),
    );
    assert_eq!(d.decision, EgressVerdict::Allow);
}

#[test]
fn the_closer_re_runs_past_a_cache_hit() {
    // The approval cache keys `(host_norm, port, protocol, method, scope)` —
    // the body is not a key member, so a cached approval must not carry a
    // body the readers don't cover (the DF-S1.12-1 `approved_host_body`
    // cache-scope member, discharged here).
    let mut p = policy_with(mediated_net(vec![]));
    p.net.default_unmatched = DefaultUnmatched::Ask;
    p.residual_channels.push(residual_body());
    p.compute_ids();
    let mut cache = ApprovalCache::default();
    cache.insert(
        "api.example.com",
        443,
        EgressProtocol::Http,
        Some("POST"),
        CacheScope::Session,
        &p.version_id,
        "eff-0",
        "decided-0",
    );
    // Cache hit + unlabeled body ⇒ the closer still refuses.
    let d = decide_egress(
        &p,
        &req("api.example.com", Some("payload"), None),
        true,
        &cache,
    );
    assert_eq!(d.decision, EgressVerdict::Deny);
    assert_eq!(d.source, EgressSource::FlowGuard);
    assert_eq!(d.reason, Some(EgressReason::ReaderCoverage));
}

#[test]
fn standalone_closer_matches_decide_egress() {
    // `approved_host_body_check` is the same rule both the pure layer and
    // the mediator's post-allow leg run (CC1 — one spelling).
    let mut p = policy_with(mediated_net(vec![allow_rule("api.example.com")]));
    p.residual_channels.push(residual_body());
    p.compute_ids();
    let covered = req(
        "api.example.com",
        Some("x"),
        Some(ReaderSet::Restricted(
            ["api.example.com".to_string()].into_iter().collect(),
        )),
    );
    assert!(approved_host_body_check(&p, &covered).is_none());
    let bare = req("api.example.com", Some("x"), None);
    assert_eq!(
        approved_host_body_check(&p, &bare).map(|d| d.reason),
        Some(Some(EgressReason::ReaderCoverage))
    );
}
