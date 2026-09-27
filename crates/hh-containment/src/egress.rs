//! `decide_egress` — the §5g.4 §2 normative decision order (ADR-0061 D2;
//! DF-S1.12-1's Stage-2 half). This module is **pure**: no I/O, no ledger,
//! no clock — the runtime mediator (`hh-env::egress::EgressMediator`,
//! ADR-0266 D1) owns attribution-token resolution, event emission, the
//! credential drain and the wire.
//!
//! The normative order (spec §5g.4 §2 `decide_egress` row; ADR-0061 D2):
//!
//! ```text
//! (1) mode_guard        — `none` ⇒ deny{mode_none}; `public` ⇒ the mediator
//!                         is not in the path (a request arriving under
//!                         `public` ⇒ deny{not_allowed} — fail closed);
//! (2) attribution       — no resolvable effect_id ⇒ deny{unattributed};
//! (3) deny rules        — on `host_norm` AND every `resolved_addr`;
//! (4) non_public_guard  — on `resolved_addrs` unless the host is
//!                         allow-listed (an allow rule covers `host_norm`);
//! (5) protocol/port/method guards — against the allow rules covering
//!                         `host_norm` (`methods_default` unless a rule
//!                         widens);
//! (6) allow rule        ⇒ allow (credential_bindings are *candidates* — the
//!                         mediator substitutes only sentinels whose own
//!                         binding names `host_norm`; ADR-0266 D2);
//! (7) approval_cache    — narrowing-only, keyed H(pattern, scope), dropped
//!                         on policy version change;
//! (8) default_unmatched ⇒ deny{not_allowed} | ask (the approvals-budget
//!                         reservation is the runtime's — `Account::
//!                         resolve_ask`; exhausted ⇒ deny{budget_exhausted}).
//! ```
//!
//! Deny before allow (N1); `HostPattern::Global` in a deny rule is refused at
//! `validate_structure` (N2) *and* never matches here (defence in depth).
//! `deny(unattributed)` is the only answer for an unresolvable effect — the
//! `attributed` flag is the runtime's verdict over the kernel-minted token
//! (ADR-0061 D4; the pure layer never sees the token's binding).

use std::collections::BTreeMap;
use std::net::IpAddr;

use hh_identity::idp::idp_id;
use hh_wire::json::Json;

use crate::policy::{
    normalize_host, ContainmentPolicy, DnsResolver, EgressProtocol, EgressRule, HostPattern,
    NetMode, NonPublic, RuleDecision,
};

// ── the request ───────────────────────────────────────────────────────────────

/// `EgressRequest` — the bridged-channel `decision_request` record
/// (§5g.4 §2): `{effect_id?, tool_call_id, env_handle, protocol, host_raw,
/// resolved_addrs[], port, method?, path?, headers[], credential_sentinels[]}`.
/// `token` is the kernel-minted attribution capability presented on the
/// channel's preface (never ledgered — only its hash may be).
///
/// `resolved_addrs` is what the *caller* claims (an IP-literal dial, or a
/// helper-supplied resolution); the mediator re-resolves after the allow
/// decision and re-checks (ADR-0061 D3's decide-once-consume-once).
#[derive(Debug, Clone, PartialEq)]
pub struct EgressRequest {
    /// The attribution token (opaque; resolved by the runtime).
    pub token: String,
    /// The claimed effect — verified against the token's binding.
    pub effect_id: Option<String>,
    /// The tool call this request decorates.
    pub tool_call_id: String,
    /// The environment handle the request arrived on.
    pub env_handle: String,
    /// `http | https_connect | socks5_tcp | socks5_udp | tcp_transparent | dns`.
    pub protocol: EgressProtocol,
    /// The destination as spelled (unnormalised).
    pub host_raw: String,
    /// Caller-supplied resolved addresses (IP spellings).
    pub resolved_addrs: Vec<String>,
    /// The destination port.
    pub port: u16,
    /// The HTTP method, when the request has one.
    pub method: Option<String>,
    /// The request path, when there is one (the ambiguous-path check's input).
    pub path: Option<String>,
    /// The request headers — placeholder spellings (`mh_secret:…`) in values
    /// are sentinel candidates (ADR-0266 D2: forwarded unchanged unless the
    /// sentinel's own binding names `host_norm`).
    pub headers: Vec<(String, String)>,
    /// The request body (content-free to the decision — the C0 claim is
    /// destination/credential/method bounding only).
    pub body: Option<String>,
    /// Explicitly tagged sentinel spellings (in addition to any
    /// `mh_secret:`-prefixed header values the mediator scans for).
    pub credential_sentinels: Vec<String>,
}

impl EgressRequest {
    /// `host_norm = normalize(host_raw)`.
    pub fn host_norm(&self) -> String {
        normalize_host(&self.host_raw)
    }

    /// The canonical request JSON (the `decision_request` shape; `token` is
    /// recorded as its hash — never the capability itself).
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("token_hash".to_string(), Json::str(token_hash(&self.token)));
        if let Some(e) = &self.effect_id {
            m.insert("effect_id".to_string(), Json::str(e.clone()));
        }
        m.insert(
            "tool_call_id".to_string(),
            Json::str(self.tool_call_id.clone()),
        );
        m.insert("env_handle".to_string(), Json::str(self.env_handle.clone()));
        m.insert("protocol".to_string(), Json::str(self.protocol.as_str()));
        m.insert("host_raw".to_string(), Json::str(self.host_raw.clone()));
        m.insert("host_norm".to_string(), Json::str(self.host_norm()));
        m.insert(
            "resolved_addrs".to_string(),
            Json::Arr(
                self.resolved_addrs
                    .iter()
                    .map(|a| Json::str(a.clone()))
                    .collect(),
            ),
        );
        m.insert("port".to_string(), Json::Int(self.port as i64));
        if let Some(mth) = &self.method {
            m.insert("method".to_string(), Json::str(mth.clone()));
        }
        if let Some(p) = &self.path {
            m.insert("path".to_string(), Json::str(p.clone()));
        }
        m.insert(
            "headers".to_string(),
            Json::Arr(
                self.headers
                    .iter()
                    .map(|(k, v)| Json::Arr(vec![Json::str(k.clone()), Json::str(v.clone())]))
                    .collect(),
            ),
        );
        if let Some(b) = &self.body {
            m.insert("body".to_string(), Json::str(b.clone()));
        }
        m.insert(
            "credential_sentinels".to_string(),
            Json::Arr(
                self.credential_sentinels
                    .iter()
                    .map(|s| Json::str(s.clone()))
                    .collect(),
            ),
        );
        Json::Obj(m)
    }
}

/// `token_hash` — the recorded form of an attribution token (the token itself
/// never enters a payload).
pub fn token_hash(token: &str) -> String {
    idp_id("attribution_token_hash", token.as_bytes())
}

// ── the decision record ───────────────────────────────────────────────────────

/// `decision ∈ {allow, deny, ask}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EgressVerdict {
    /// Forward the (credential-substituted) request.
    Allow,
    /// Refuse.
    Deny,
    /// Escalate to the monitor's owed-decision path (`default_unmatched=ask`).
    Ask,
}

impl EgressVerdict {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EgressVerdict::Allow => "allow",
            EgressVerdict::Deny => "deny",
            EgressVerdict::Ask => "ask",
        }
    }
}

/// `source ∈ {mode_guard, unattributed, deny_rule, non_public_guard,
/// protocol_guard, allow_rule, approval_cache, default_unmatched, monitor}`
/// (§5g.4 §3 `EgressDecision.source`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EgressSource {
    /// Step 1 — the mode guard.
    ModeGuard,
    /// Step 2 — no resolvable effect.
    Unattributed,
    /// Step 3 — a deny rule matched.
    DenyRule,
    /// Step 4 — a non-public resolved address.
    NonPublicGuard,
    /// Step 5 — protocol/port/method not admitted.
    ProtocolGuard,
    /// Step 6 — an allow rule matched.
    AllowRule,
    /// Step 7 — the approval cache hit.
    ApprovalCache,
    /// Step 8 — `default_unmatched`.
    DefaultUnmatched,
    /// The monitor's endorsement resolved an ask.
    Monitor,
}

impl EgressSource {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EgressSource::ModeGuard => "mode_guard",
            EgressSource::Unattributed => "unattributed",
            EgressSource::DenyRule => "deny_rule",
            EgressSource::NonPublicGuard => "non_public_guard",
            EgressSource::ProtocolGuard => "protocol_guard",
            EgressSource::AllowRule => "allow_rule",
            EgressSource::ApprovalCache => "approval_cache",
            EgressSource::DefaultUnmatched => "default_unmatched",
            EgressSource::Monitor => "monitor",
        }
    }
}

/// `reason ∈ {denied, not_allowed, not_allowed_local, method_not_allowed,
/// port_not_allowed, protocol_not_allowed, mode_none, unattributed,
/// budget_exhausted}` — the closed deny-reason sum (§5g.4 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EgressReason {
    /// A deny rule fired.
    Denied,
    /// Unmatched / not admitted.
    NotAllowed,
    /// A non-public resolved address.
    NotAllowedLocal,
    /// Method outside `methods_default` and every covering rule.
    MethodNotAllowed,
    /// Port outside every covering allow rule.
    PortNotAllowed,
    /// Protocol outside every covering allow rule (or unroutable DNS).
    ProtocolNotAllowed,
    /// `net.mode = none`.
    ModeNone,
    /// No resolvable effect_id.
    Unattributed,
    /// A required budget refused (`network.calls` or `approvals.requested`).
    BudgetExhausted,
}

impl EgressReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EgressReason::Denied => "denied",
            EgressReason::NotAllowed => "not_allowed",
            EgressReason::NotAllowedLocal => "not_allowed_local",
            EgressReason::MethodNotAllowed => "method_not_allowed",
            EgressReason::PortNotAllowed => "port_not_allowed",
            EgressReason::ProtocolNotAllowed => "protocol_not_allowed",
            EgressReason::ModeNone => "mode_none",
            EgressReason::Unattributed => "unattributed",
            EgressReason::BudgetExhausted => "budget_exhausted",
        }
    }
}

/// `EgressDecision` (§5g.4 §3) — `{decision, source, rule_ref?, reason,
/// credential_binding_applied}`. `credential_binding_candidates` carries the
/// matched allow rule's declared `credential_bindings` refs; the *runtime*
/// records the actually-applied bindings in the decided row's
/// `credential_binding_applied` member (ADR-0266 D2 — a candidate is a
/// declaration, an application is a fact).
#[derive(Debug, Clone, PartialEq)]
pub struct EgressDecision {
    /// The verdict.
    pub decision: EgressVerdict,
    /// Which guard/step produced it.
    pub source: EgressSource,
    /// The matched rule's canonical ref (`rule_ref` — the rule's index +
    /// host spelling).
    pub rule_ref: Option<String>,
    /// The deny reason (deny verdicts; `ask`/`allow` carry none).
    pub reason: Option<EgressReason>,
    /// The matched allow rule's declared `credential_bindings` refs
    /// (candidates — the mediator applies the subset whose bindings cover
    /// `host_norm`).
    pub credential_binding_candidates: Vec<String>,
    /// The matched allow/deny rule's index (canonical-order coordinate).
    pub rule_index: Option<usize>,
}

impl EgressDecision {
    fn deny(source: EgressSource, reason: EgressReason) -> EgressDecision {
        EgressDecision {
            decision: EgressVerdict::Deny,
            source,
            rule_ref: None,
            reason: Some(reason),
            credential_binding_candidates: vec![],
            rule_index: None,
        }
    }

    fn deny_rule(
        source: EgressSource,
        reason: EgressReason,
        index: usize,
        rule: &EgressRule,
    ) -> EgressDecision {
        EgressDecision {
            decision: EgressVerdict::Deny,
            source,
            rule_ref: Some(format!("rule[{index}]:{}", rule.host.spelling())),
            reason: Some(reason),
            credential_binding_candidates: vec![],
            rule_index: Some(index),
        }
    }

    /// The canonical JSON member set for `security.egress.decided`.
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("decision".to_string(), Json::str(self.decision.as_str()));
        m.insert("source".to_string(), Json::str(self.source.as_str()));
        if let Some(r) = &self.rule_ref {
            m.insert("rule_ref".to_string(), Json::str(r.clone()));
        }
        if let Some(r) = &self.reason {
            m.insert("reason".to_string(), Json::str(r.as_str()));
        }
        Json::Obj(m)
    }
}

// ── the address classifier ────────────────────────────────────────────────────

/// Whether `ip` is a non-public destination (§5g.4 §2 `non_public_guard`'s
/// closed set): loopback, private (RFC 1918), link-local, CGNAT (100.64/10),
/// TEST-NET/documentation, benchmarking (198.18/15), reserved, unique-local,
/// unspecified, multicast, broadcast — and the IPv4-mapped/compatible forms.
pub fn is_non_public(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            // (hand-rolled — `is_shared`/`is_benchmarking`/`is_documentation`
            // are unstable on this toolchain; every range is spelled
            // explicitly so the closed set is auditable.)
            v4.is_loopback()                                   // 127/8
                || v4.is_private()                             // 10/8, 172.16/12, 192.168/16
                || v4.is_link_local()                          // 169.254/16
                || (o[0] == 100 && (o[1] & 0xc0) == 64)        // 100.64/10 CGNAT
                || (o[0] == 198 && (o[1] & 0xfe) == 18)        // 198.18/15 benchmark
                || v4.is_multicast()                           // 224/4
                || v4.is_broadcast()                           // 255.255.255.255
                || v4.is_unspecified()                         // 0.0.0.0
                || o[0] == 0                                   // 0/8 "this network"
                || o[0] >= 240                                 // 240/4 reserved
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)     // 192.0.0/24 IETF
                || (o[0] == 192 && o[1] == 0 && o[2] == 2)     // TEST-NET-1
                || (o[0] == 192 && o[1] == 31 && o[2] == 196)  // AS112
                || (o[0] == 198 && o[1] == 51 && o[2] == 100)  // TEST-NET-2
                || (o[0] == 203 && o[1] == 0 && o[2] == 113) // TEST-NET-3
        }
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped().or_else(|| v6.to_ipv4()) {
                return is_non_public(&IpAddr::V4(mapped));
            }
            let segs = v6.segments();
            v6.is_loopback()                       // ::1
                || v6.is_unspecified()             // ::
                || v6.is_multicast()               // ff00/8
                || (segs[0] & 0xfe00) == 0xfc00    // fc00/7 unique-local
                || (segs[0] & 0xffc0) == 0xfe80    // fe80/10 link-local
                || (segs[0] & 0xffe0) == 0xfe80    // (belt) fe80/10
                || (segs[0] == 0x2001 && segs[1] == 0x0db8) // 2001:db8/32 doc
                || segs[0] == 0 // ::/16 remainder (unspecified block)
        }
    }
}

/// Parse and classify the request's effective address set: every claimed
/// `resolved_addrs` member plus `host_norm` itself when it is an IP literal.
/// Returns `(parsed_addrs, malformed)` — a malformed spelling is *not* an
/// address (it is ignored by the guard; the rule grammar decides whether the
/// destination is reachable at all).
pub fn effective_addrs(req: &EgressRequest) -> Vec<IpAddr> {
    let host_norm = req.host_norm();
    let mut out: Vec<IpAddr> = Vec::new();
    if let Ok(ip) = host_norm.parse::<IpAddr>() {
        out.push(ip);
    }
    for a in &req.resolved_addrs {
        if let Ok(ip) = a.trim().parse::<IpAddr>() {
            if !out.contains(&ip) {
                out.push(ip);
            }
        }
    }
    out
}

// ── the approval cache ────────────────────────────────────────────────────────

/// The scope a cache entry lives at — `session | run` only (§5g.4 §2's exact
/// bound; `once` never inserts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CacheScope {
    /// The environment session.
    Session,
    /// The run.
    Run,
}

impl CacheScope {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CacheScope::Session => "session",
            CacheScope::Run => "run",
        }
    }

    /// Parse the spelling.
    pub fn parse(s: &str) -> Option<CacheScope> {
        match s {
            "session" => Some(CacheScope::Session),
            "run" => Some(CacheScope::Run),
            _ => None,
        }
    }
}

/// One narrowing-only cache entry — it replays *exactly* the endorsed shape
/// (host + port + protocol + method), never a widening (§5g.4 §2's
/// `approval_cache` row; ADR-0266 D6). Entries are dropped when the policy
/// version they were endorsed under changes (`amend` revokes by
/// construction).
#[derive(Debug, Clone, PartialEq)]
pub struct CacheEntry {
    /// `H(canonical pattern, scope)` — the lookup key.
    pub key: String,
    /// The endorsed destination tuple.
    pub host: String,
    /// The endorsed port.
    pub port: u16,
    /// The endorsed protocol.
    pub protocol: EgressProtocol,
    /// The endorsed method (`None` = the request had none).
    pub method: Option<String>,
    /// `session | run`.
    pub scope: CacheScope,
    /// The policy version the endorsement landed under.
    pub policy_version_id: String,
    /// The endorsed effect.
    pub effect_id: String,
    /// The deciding `security.egress.decided` event ref.
    pub decided_ref: String,
}

/// The `approval_cache` — in-memory (lifetime ⊆ run/session), keyed
/// `H(canonical pattern, scope)`, narrowing-only (ADR-0061 D2 step 7).
#[derive(Debug, Clone, Default)]
pub struct ApprovalCache {
    entries: BTreeMap<String, CacheEntry>,
}

impl ApprovalCache {
    /// The lookup key — `idp_id("egress.approval_cache", host ∥ port ∥
    /// protocol ∥ method ∥ scope)` (canonical; the same endorsed shape keys
    /// the same entry).
    pub fn key_for(
        host_norm: &str,
        port: u16,
        protocol: EgressProtocol,
        method: Option<&str>,
        scope: CacheScope,
    ) -> String {
        let material = format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            host_norm,
            port,
            protocol.as_str(),
            method.unwrap_or(""),
            scope.as_str()
        );
        idp_id("egress.approval_cache", material.as_bytes())
    }

    /// Insert the endorsed shape (`session | run` only — the caller maps
    /// `once` to no-insert).
    #[allow(clippy::too_many_arguments)] // the arity is the cache-key material's.
    pub fn insert(
        &mut self,
        host_norm: &str,
        port: u16,
        protocol: EgressProtocol,
        method: Option<&str>,
        scope: CacheScope,
        policy_version_id: &str,
        effect_id: &str,
        decided_ref: &str,
    ) {
        let key = Self::key_for(host_norm, port, protocol, method, scope);
        self.entries.insert(
            key.clone(),
            CacheEntry {
                key,
                host: host_norm.to_string(),
                port,
                protocol,
                method: method.map(String::from),
                scope,
                policy_version_id: policy_version_id.to_string(),
                effect_id: effect_id.to_string(),
                decided_ref: decided_ref.to_string(),
            },
        );
    }

    /// Lookup — a hit requires the exact tuple AND the policy version the
    /// entry was endorsed under (`amend` ⇒ new version ⇒ entries die).
    pub fn lookup(&self, policy_version_id: &str, req: &EgressRequest) -> Option<&CacheEntry> {
        for scope in [CacheScope::Session, CacheScope::Run] {
            let key = Self::key_for(
                &req.host_norm(),
                req.port,
                req.protocol,
                req.method.as_deref(),
                scope,
            );
            if let Some(e) = self.entries.get(&key) {
                if e.policy_version_id == policy_version_id {
                    return Some(e);
                }
            }
        }
        None
    }

    /// Drop every entry endorsed under a superseded policy version
    /// (`amend` revokes by construction — called when the mediator swaps the
    /// effective policy).
    pub fn retain_version(&mut self, policy_version_id: &str) {
        self.entries
            .retain(|_, e| e.policy_version_id == policy_version_id);
    }

    /// The live entry count (tests).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ── the normative order ───────────────────────────────────────────────────────

/// `rule_ref` canonical form — `rule[<index>]:<host spelling>`.
fn rule_ref(index: usize, r: &EgressRule) -> String {
    format!("rule[{index}]:{}", r.host.spelling())
}

/// Whether `rule` (decision `deny`) covers the request — host match plus the
/// rule's declared port/protocol/method constraints (an unconstrained
/// member matches anything).
fn deny_matches(rule: &EgressRule, host_norm: &str, req: &EgressRequest) -> bool {
    if rule.decision != RuleDecision::Deny || rule.host == HostPattern::Global {
        return false;
    }
    if !rule.host.matches(host_norm) {
        return false;
    }
    constraints_match(rule, req)
}

/// Whether the rule's `ports`/`protocols`/`methods` constraints admit the
/// request (empty = unconstrained; `methods` widens `methods_default`).
fn constraints_match(rule: &EgressRule, req: &EgressRequest) -> bool {
    if !rule.ports.is_empty() && !rule.ports.contains(&req.port) {
        return false;
    }
    if !rule.protocols.is_empty() && !rule.protocols.contains(&req.protocol) {
        return false;
    }
    if !rule.methods.is_empty() {
        let m = req.method.as_deref().unwrap_or("");
        if !rule.methods.iter().any(|x| x.eq_ignore_ascii_case(m)) {
            return false;
        }
    }
    true
}

/// Whether a deny rule covers a *resolved address* (the addr is matched as a
/// host spelling — an IP literal is an `Exact` candidate).
fn deny_matches_addr(rule: &EgressRule, addr: &IpAddr, req: &EgressRequest) -> bool {
    if rule.decision != RuleDecision::Deny || rule.host == HostPattern::Global {
        return false;
    }
    if !rule.host.matches(&addr.to_string()) {
        return false;
    }
    constraints_match(rule, req)
}

/// Whether any allow rule's host pattern covers `host_norm` (the
/// `non_public_guard`'s "allow_listed" test — the port/protocol/method
/// constraints are step 5's, not this one's).
fn is_allow_listed(policy: &ContainmentPolicy, host_norm: &str) -> bool {
    policy
        .net
        .rules
        .iter()
        .any(|r| r.decision == RuleDecision::Allow && r.host.matches(host_norm))
}

/// `decide_egress(policy, req, attributed, cache) → EgressDecision` — the
/// normative order (§5g.4 §2; ADR-0061 D2). `attributed` is the runtime's
/// verdict on the token (resolvable effect + matching env handle); the pure
/// layer never sees the binding.
///
/// Pure and total: every input yields exactly one decision; the same inputs
/// yield the same decision (the rule order is the canonical order — first
/// match wins).
pub fn decide_egress(
    policy: &ContainmentPolicy,
    req: &EgressRequest,
    attributed: bool,
    cache: &ApprovalCache,
) -> EgressDecision {
    let host_norm = req.host_norm();

    // (1) mode_guard — `none` denies unconditionally; `public` is not a
    // mediated path (a request arriving at the mediator under `public` is a
    // misroute — deny, never forward; ADR-0266 D1).
    match policy.net.mode {
        NetMode::None => {
            return EgressDecision::deny(EgressSource::ModeGuard, EgressReason::ModeNone)
        }
        NetMode::Public => {
            return EgressDecision::deny(EgressSource::ModeGuard, EgressReason::NotAllowed)
        }
        NetMode::Mediated => {}
    }

    // (2) attribution — no resolvable effect_id ⇒ deny(unattributed).
    if !attributed {
        return EgressDecision::deny(EgressSource::Unattributed, EgressReason::Unattributed);
    }

    // (3) deny rules — on host_norm AND every resolved addr (a global `*` in
    // a deny rule is N2-invalid and never matches here either).
    let addrs = effective_addrs(req);
    for (i, r) in policy.net.rules.iter().enumerate() {
        if deny_matches(r, &host_norm, req) || addrs.iter().any(|a| deny_matches_addr(r, a, req)) {
            return EgressDecision::deny_rule(EgressSource::DenyRule, EgressReason::Denied, i, r);
        }
    }

    // (4) non_public_guard — on resolved addrs unless the host is
    // allow-listed. `non_public_destinations = deny` refuses outright;
    // `allow_listed` requires a covering allow rule.
    if addrs.iter().any(is_non_public) {
        match policy.net.non_public_destinations {
            NonPublic::Deny => {
                return EgressDecision::deny(
                    EgressSource::NonPublicGuard,
                    EgressReason::NotAllowedLocal,
                )
            }
            NonPublic::AllowListed => {
                if !is_allow_listed(policy, &host_norm) {
                    return EgressDecision::deny(
                        EgressSource::NonPublicGuard,
                        EgressReason::NotAllowedLocal,
                    );
                }
            }
        }
    }

    // (4b) protocol=routability guard — `dns` requests need a resolver that
    // admits them; `socks5_udp`/`https_connect`/`tcp_transparent` are
    // admitted only by an explicit rule (empty `protocols` = unconstrained,
    // so a bare allow rule admits any non-dns protocol — declared by the
    // layer, CC2).
    if req.protocol == EgressProtocol::Dns {
        match &policy.net.dns.resolver {
            DnsResolver::None => {
                return EgressDecision::deny(
                    EgressSource::ProtocolGuard,
                    EgressReason::ProtocolNotAllowed,
                )
            }
            DnsResolver::ListedNameservers(ns) => {
                if !ns.iter().any(|n| normalize_host(n) == host_norm) {
                    return EgressDecision::deny(
                        EgressSource::ProtocolGuard,
                        EgressReason::ProtocolNotAllowed,
                    );
                }
            }
            DnsResolver::Mediator => {}
        }
    }

    // (5)+(6) protocol/port/method guards and the allow match — evaluated
    // over the allow rules covering `host_norm`, in canonical order. For
    // each dimension the *admitting extent* narrows: a rule contributes to
    // the port check only if its protocols admit the request, and to the
    // method check only if its ports+protocols do (§5g.4's rule grammar —
    // `methods` widens `methods_default` for *the rule's extent*).
    // No covering rule ⇒ the request is *unmatched* (step 8's), never a
    // guard refusal — AC-H4-04.
    let covering: Vec<(usize, &EgressRule)> = policy
        .net
        .rules
        .iter()
        .enumerate()
        .filter(|(_, r)| r.decision == RuleDecision::Allow && r.host.matches(&host_norm))
        .collect();
    if !covering.is_empty() {
        let proto_admits: Vec<(usize, &EgressRule)> = covering
            .iter()
            .copied()
            .filter(|(_, r)| r.protocols.is_empty() || r.protocols.contains(&req.protocol))
            .collect();
        if proto_admits.is_empty() {
            let (i, r) = covering[0];
            return EgressDecision::deny_rule(
                EgressSource::ProtocolGuard,
                EgressReason::ProtocolNotAllowed,
                i,
                r,
            );
        }
        let port_admits: Vec<(usize, &EgressRule)> = proto_admits
            .iter()
            .copied()
            .filter(|(_, r)| r.ports.is_empty() || r.ports.contains(&req.port))
            .collect();
        if port_admits.is_empty() {
            let (i, r) = proto_admits[0];
            return EgressDecision::deny_rule(
                EgressSource::ProtocolGuard,
                EgressReason::PortNotAllowed,
                i,
                r,
            );
        }
        // method ∈ methods_default ∪ the rule's own widening — evaluated
        // over the port+protocol-admitting extent.
        if let Some(m) = &req.method {
            let method_admits = policy
                .net
                .methods_default
                .iter()
                .any(|d| d.eq_ignore_ascii_case(m))
                || port_admits
                    .iter()
                    .any(|(_, r)| r.methods.iter().any(|x| x.eq_ignore_ascii_case(m)));
            if !method_admits {
                let (i, r) = port_admits[0];
                return EgressDecision::deny_rule(
                    EgressSource::ProtocolGuard,
                    EgressReason::MethodNotAllowed,
                    i,
                    r,
                );
            }
        }

        // (6) allow — the first covering rule whose extent admits the
        // request wins; its declared `credential_bindings` are the
        // substitution candidates (the mediator applies only sentinels
        // whose *own* binding names `host_norm` — anti-laundering).
        if let Some((i, r)) = port_admits.first() {
            return EgressDecision {
                decision: EgressVerdict::Allow,
                source: EgressSource::AllowRule,
                rule_ref: Some(rule_ref(*i, r)),
                reason: None,
                credential_binding_candidates: r.credential_bindings.clone(),
                rule_index: Some(*i),
            };
        }
    }

    // (7) approval_cache — narrowing-only, version-pinned, and only when the
    // policy opts into session caching (`amendment.session_cache`).
    if policy.amendment.session_cache && cache.lookup(&policy.version_id, req).is_some() {
        return EgressDecision {
            decision: EgressVerdict::Allow,
            source: EgressSource::ApprovalCache,
            rule_ref: None,
            reason: None,
            credential_binding_candidates: vec![],
            rule_index: None,
        };
    }

    // (8) default_unmatched.
    match policy.net.default_unmatched {
        crate::policy::DefaultUnmatched::Deny => {
            EgressDecision::deny(EgressSource::DefaultUnmatched, EgressReason::NotAllowed)
        }
        crate::policy::DefaultUnmatched::Ask => EgressDecision {
            decision: EgressVerdict::Ask,
            source: EgressSource::DefaultUnmatched,
            rule_ref: None,
            reason: None,
            credential_binding_candidates: vec![],
            rule_index: None,
        },
    }
}

/// `recheck_resolved` — the decide-once-consume-once second pass (ADR-0061
/// D3): after an allow, the mediator resolves `host_norm` itself and every
/// *fresh* address is re-checked against the deny rules and the
/// non-public guard (the SSRF pivot — AC-H4-04: a public hostname resolving
/// to a private address is denied *here*, at the post-resolution check).
/// `Some(deny)` supersedes the provisional allow; `None` consumes the answer.
pub fn recheck_resolved(
    policy: &ContainmentPolicy,
    req: &EgressRequest,
    fresh_addrs: &[IpAddr],
) -> Option<EgressDecision> {
    let host_norm = req.host_norm();
    for (i, r) in policy.net.rules.iter().enumerate() {
        if fresh_addrs.iter().any(|a| deny_matches_addr(r, a, req)) {
            return Some(EgressDecision::deny_rule(
                EgressSource::DenyRule,
                EgressReason::Denied,
                i,
                r,
            ));
        }
    }
    if fresh_addrs.iter().any(is_non_public) {
        match policy.net.non_public_destinations {
            NonPublic::Deny => {
                return Some(EgressDecision::deny(
                    EgressSource::NonPublicGuard,
                    EgressReason::NotAllowedLocal,
                ));
            }
            NonPublic::AllowListed => {
                if !is_allow_listed(policy, &host_norm) {
                    return Some(EgressDecision::deny(
                        EgressSource::NonPublicGuard,
                        EgressReason::NotAllowedLocal,
                    ));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::*;

    fn policy_with(net: NetPolicy) -> ContainmentPolicy {
        let mut p = kernel_default(0);
        p.net = net;
        p.amendment.session_cache = true;
        p.compute_ids();
        p
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

    #[test]
    fn mode_none_denies_before_attribution() {
        let p = policy_with(mediated_net(vec![], DefaultUnmatched::Deny));
        let mut p2 = p.clone();
        p2.net.mode = NetMode::None;
        let d = decide_egress(
            &p2,
            &req("example.com", 443),
            false,
            &ApprovalCache::default(),
        );
        assert_eq!(d.decision, EgressVerdict::Deny);
        assert_eq!(d.source, EgressSource::ModeGuard);
        assert_eq!(d.reason, Some(EgressReason::ModeNone));
    }

    #[test]
    fn unattributed_denies_before_rules() {
        let p = policy_with(mediated_net(
            vec![allow_rule("example.com")],
            DefaultUnmatched::Deny,
        ));
        let d = decide_egress(
            &p,
            &req("example.com", 443),
            false,
            &ApprovalCache::default(),
        );
        assert_eq!(d.source, EgressSource::Unattributed);
        assert_eq!(d.reason, Some(EgressReason::Unattributed));
    }

    #[test]
    fn deny_beats_allow_same_host() {
        let mut deny = allow_rule("example.com");
        deny.decision = RuleDecision::Deny;
        let p = policy_with(mediated_net(
            vec![deny, allow_rule("example.com")],
            DefaultUnmatched::Deny,
        ));
        let d = decide_egress(
            &p,
            &req("example.com", 443),
            true,
            &ApprovalCache::default(),
        );
        assert_eq!(d.decision, EgressVerdict::Deny);
        assert_eq!(d.source, EgressSource::DenyRule);
        assert_eq!(d.reason, Some(EgressReason::Denied));
    }

    #[test]
    fn apex_star_does_not_match_apex() {
        let p = policy_with(mediated_net(
            vec![allow_rule("*.example.com")],
            DefaultUnmatched::Deny,
        ));
        let d = decide_egress(
            &p,
            &req("example.com", 443),
            true,
            &ApprovalCache::default(),
        );
        assert_eq!(d.decision, EgressVerdict::Deny);
        assert_eq!(d.source, EgressSource::DefaultUnmatched);
        let d = decide_egress(
            &p,
            &req("a.example.com", 443),
            true,
            &ApprovalCache::default(),
        );
        assert_eq!(d.decision, EgressVerdict::Allow);
        assert_eq!(d.source, EgressSource::AllowRule);
    }

    #[test]
    fn non_public_addr_denied_unless_allow_listed() {
        let p = policy_with(mediated_net(vec![], DefaultUnmatched::Deny));
        let mut r = req("internal.example.com", 443);
        r.resolved_addrs = vec!["192.168.1.1".into()];
        let d = decide_egress(&p, &r, true, &ApprovalCache::default());
        assert_eq!(d.source, EgressSource::NonPublicGuard);
        assert_eq!(d.reason, Some(EgressReason::NotAllowedLocal));

        // allow-listed ⇒ the guard passes (the allow rule then decides).
        let p2 = policy_with(mediated_net(
            vec![allow_rule("internal.example.com")],
            DefaultUnmatched::Deny,
        ));
        let d = decide_egress(&p2, &r, true, &ApprovalCache::default());
        assert_eq!(d.decision, EgressVerdict::Allow);
    }

    #[test]
    fn non_public_denied_outright_under_deny() {
        let mut net = mediated_net(
            vec![allow_rule("internal.example.com")],
            DefaultUnmatched::Deny,
        );
        net.non_public_destinations = NonPublic::Deny;
        let p = policy_with(net);
        let mut r = req("internal.example.com", 443);
        r.resolved_addrs = vec!["10.0.0.9".into()];
        let d = decide_egress(&p, &r, true, &ApprovalCache::default());
        assert_eq!(d.source, EgressSource::NonPublicGuard);
    }

    #[test]
    fn port_and_method_guards() {
        let mut r443 = allow_rule("example.com");
        r443.ports = vec![443];
        let p = policy_with(mediated_net(vec![r443], DefaultUnmatched::Deny));
        let d = decide_egress(&p, &req("example.com", 22), true, &ApprovalCache::default());
        assert_eq!(d.reason, Some(EgressReason::PortNotAllowed));

        let mut post = req("example.com", 443);
        post.method = Some("POST".into());
        let d = decide_egress(&p, &post, true, &ApprovalCache::default());
        assert_eq!(d.reason, Some(EgressReason::MethodNotAllowed));
        let mut post_ok = post.clone();
        post_ok.method = Some("OPTIONS".into());
        let d = decide_egress(&p, &post_ok, true, &ApprovalCache::default());
        assert_eq!(d.decision, EgressVerdict::Allow);
    }

    #[test]
    fn rule_widening_admits_method() {
        let mut r = allow_rule("example.com");
        r.methods = vec!["POST".into()];
        let p = policy_with(mediated_net(vec![r], DefaultUnmatched::Deny));
        let mut req = req("example.com", 443);
        req.method = Some("POST".into());
        let d = decide_egress(&p, &req, true, &ApprovalCache::default());
        assert_eq!(d.decision, EgressVerdict::Allow);
    }

    #[test]
    fn default_unmatched_ask() {
        let p = policy_with(mediated_net(vec![], DefaultUnmatched::Ask));
        let d = decide_egress(
            &p,
            &req("odd.example.org", 443),
            true,
            &ApprovalCache::default(),
        );
        assert_eq!(d.decision, EgressVerdict::Ask);
        assert_eq!(d.source, EgressSource::DefaultUnmatched);
    }

    #[test]
    fn approval_cache_hit_and_version_invalidation() {
        let p = policy_with(mediated_net(vec![], DefaultUnmatched::Deny));
        let mut cache = ApprovalCache::default();
        let r = req("new.example.com", 443);
        cache.insert(
            "new.example.com",
            443,
            EgressProtocol::Http,
            Some("GET"),
            CacheScope::Session,
            &p.version_id,
            "eff-1",
            "evt-1",
        );
        let d = decide_egress(&p, &r, true, &cache);
        assert_eq!(d.decision, EgressVerdict::Allow);
        assert_eq!(d.source, EgressSource::ApprovalCache);
        // A policy version change revokes the entry.
        let mut p2 = p.clone();
        p2.version_id = "sha256:other".into();
        let d = decide_egress(&p2, &r, true, &cache);
        assert_eq!(d.decision, EgressVerdict::Deny);
    }

    #[test]
    fn dns_resolver_guards() {
        let mut net = mediated_net(vec![], DefaultUnmatched::Deny);
        net.dns.resolver = DnsResolver::None;
        let p = policy_with(net);
        let mut r = req("resolver.example.com", 53);
        r.protocol = EgressProtocol::Dns;
        let d = decide_egress(&p, &r, true, &ApprovalCache::default());
        assert_eq!(d.reason, Some(EgressReason::ProtocolNotAllowed));
    }

    #[test]
    fn resolved_addr_deny_rule() {
        let mut deny_ip = allow_rule("10.9.9.9");
        deny_ip.decision = RuleDecision::Deny;
        let p = policy_with(mediated_net(
            vec![deny_ip, allow_rule("example.com")],
            DefaultUnmatched::Deny,
        ));
        let mut r = req("example.com", 443);
        r.resolved_addrs = vec!["10.9.9.9".into()];
        let d = decide_egress(&p, &r, true, &ApprovalCache::default());
        assert_eq!(d.source, EgressSource::DenyRule);
    }

    #[test]
    fn recheck_catches_ssrf_pivot() {
        // Under `non_public_destinations = deny`, a public host resolving to
        // a private address is denied at the post-resolution re-check.
        let mut net = mediated_net(
            vec![allow_rule("public.example.com")],
            DefaultUnmatched::Deny,
        );
        net.non_public_destinations = NonPublic::Deny;
        let p = policy_with(net);
        let r = req("public.example.com", 443);
        // The mediator's fresh resolution lands on a private address.
        let fresh = vec!["10.1.2.3".parse::<IpAddr>().unwrap()];
        let d = recheck_resolved(&p, &r, &fresh);
        assert!(d.is_some());
        assert_eq!(d.unwrap().source, EgressSource::NonPublicGuard);
    }

    #[test]
    fn normalization_spellings() {
        assert_eq!(normalize_host("EXAMPLE.com."), "example.com");
        assert_eq!(normalize_host("[::1]:8080"), "::1");
        assert_eq!(normalize_host("Host.Example.COM:443"), "host.example.com");
        let mut r = req("Example.COM.", 443);
        r.resolved_addrs = vec![];
        assert_eq!(r.host_norm(), "example.com");
    }
}
