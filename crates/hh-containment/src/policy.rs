//! `ContainmentPolicy/1` — the typed, provenance-bearing, content-addressed
//! sandbox-floor record (§5g.4 §3; ADR-0060 D1/D2/D5).
//!
//! ```text
//! ContainmentPolicy/1 =
//!   { policy_id: semantic_id, version_id, provenance: ProvenanceRecord,
//!     fs, net, proc, resources, amendment, residual_channels[], ext }
//! ```
//!
//! Identity (CC1 — `hh_identity` is the one scheme):
//!
//! - `policy_id` = `semantic_id` over the *semantic projection*: normalised
//!   host patterns and closed-sum values; `policy_id`/`version_id`/
//!   `provenance`/`ext`, per-rule `justification`/`provenance`, and residual
//!   `statement` text excluded (T-LCD-10 — a reworded justification is the
//!   same policy).
//! - `version_id` = `identify_bytes(ContainmentPolicy, canonical(record −
//!   {policy_id, version_id}))` — provenance and ext *are* in the version
//!   basis (a re-provenanced policy is a new version, the same policy).
//!
//! The kernel default is deny-by-default (§5g.4 §2.1): `read =
//! allow_all_except(KERNEL_DENY)`, `write = allow_only([])`, `net.mode =
//! none`, `isolation_class = process_sandbox`, `no_new_privs`, `ptrace =
//! deny`, `setuid = deny`, `die_with_parent`, `local_binding = false`,
//! `default_unmatched = deny`, `methods_default = {GET, HEAD, OPTIONS}`,
//! `strict = true`.
//!
//! `symlink_resolution = before_validation` (F1) is **not a field** — it is
//! fixed; the EP2 gate resolves links before checking containment
//! ([`crate::backend`]).

use std::collections::{BTreeMap, BTreeSet};

use hh_hir::leaves::Text;
use hh_identity::idp::{identify_bytes, idp_id};
use hh_identity::RecordKind;
use hh_provenance::{AuthorityClass, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

/// `KERNEL_PROTECTED` (I-C2) — the harness's own definition/config/hook/
/// skill/scheduled-task locations and VCS hook directories, plus R-2.8.5's
/// extension directory. Read-only inside every `WritableRoot` on every
/// backend; **no layer may remove a name** (validated on every policy and
/// enforced in [`crate::meet::effective`]). The set is closed — growth is a
/// dialect bump by the owning ticket.
pub const KERNEL_PROTECTED: &[&str] = &[
    ".hh",
    ".hh/hooks",
    ".hh/skills",
    ".hh/scheduled",
    ".hh/extensions",
    ".agents",
    ".git/hooks",
];

/// `KERNEL_DENY` — the kernel default's read-deny set: the credential- and
/// secret-bearing locations a confined process must not read regardless of
/// writable extent. Closed for Stage 1; growth is a dialect bump.
pub const KERNEL_DENY: &[&str] = &[
    ".env",
    ".env.*",
    ".ssh",
    ".aws",
    ".gnupg",
    ".netrc",
    ".kube",
    ".docker",
    ".config/gcloud",
    ".hh/secrets",
];

/// `symlink_resolution = before_validation` — the fixed, non-configurable
/// value (F1). Declared so the record documentation and the gate share one
/// spelling; there is deliberately no `symlink_resolution` field.
pub const SYMLINK_RESOLUTION: &str = "before_validation";

// ── fs ────────────────────────────────────────────────────────────────────────

/// `read.mode ∈ {allow_all_except, allow_only}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadMode {
    /// Everything readable except `deny[]` (minus `allow_within_deny[]`).
    AllowAllExcept,
    /// Only `allow[]` readable.
    AllowOnly,
}

/// `fs.read{mode, deny[], allow_within_deny[], allow[]}` — `allow` is
/// populated only under `allow_only` (the spec row lists the deny-side
/// members; the allow extent is the member `allow_only` selects over).
#[derive(Debug, Clone, PartialEq)]
pub struct ReadPolicy {
    /// The read mode.
    pub mode: ReadMode,
    /// Read-denied path patterns (suffix names like `.ssh` or `/`-anchored
    /// prefixes — [`crate::paths`]).
    pub deny: Vec<String>,
    /// Paths re-allowed inside a deny pattern (the allow-within-deny holes).
    pub allow_within_deny: Vec<String>,
    /// The allow extent under `allow_only` (empty under `allow_all_except`).
    pub allow: Vec<String>,
}

/// `WritableRoot{root, read_only_subpaths[], protected_metadata_names[]}`.
#[derive(Debug, Clone, PartialEq)]
pub struct WritableRoot {
    /// The writable root path (normalised spelling).
    pub root: String,
    /// Read-only subpaths inside this root (relative names/paths).
    pub read_only_subpaths: Vec<String>,
    /// Additional per-root protected names — always *unioned* with
    /// `fs.protected_metadata` (which itself ⊇ `KERNEL_PROTECTED`).
    pub protected_metadata_names: Vec<String>,
}

/// `fs.write{allow: [WritableRoot], deny_within_allow[]}`.
#[derive(Debug, Clone, PartialEq)]
pub struct WritePolicy {
    /// The writable roots (`allow_only` — the empty list denies every write).
    pub allow: Vec<WritableRoot>,
    /// Deny patterns applied inside writable roots.
    pub deny_within_allow: Vec<String>,
}

/// `exec{allow | any}` — the executable allow-set, or `any` (kernel only).
#[derive(Debug, Clone, PartialEq)]
pub enum ExecPolicy {
    /// Only these path patterns may execute.
    Allow(Vec<String>),
    /// Any path may execute.
    Any,
}

/// `mounts[{source: ContentAddress | HostPath, target, mode}]`.
#[derive(Debug, Clone, PartialEq)]
pub enum MountSource {
    /// A content-addressed blob/tree (already pinned — N5-clean).
    Content(String),
    /// A host path (declared — a Stage-1 spelling).
    HostPath(String),
}

/// `mode ∈ {ro, rw, masked(sentinel_ref)}`.
#[derive(Debug, Clone, PartialEq)]
pub enum MountMode {
    /// Read-only.
    Ro,
    /// Read-write.
    Rw,
    /// Masked — the mount yields the named sentinel (R-2.8.3 owns the ref).
    Masked(String),
}

/// One mount entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Mount {
    /// The mount source.
    pub source: MountSource,
    /// The target path inside the environment.
    pub target: String,
    /// The mount mode.
    pub mode: MountMode,
}

/// `fs{read, write, exec, protected_metadata ⊇ KERNEL_PROTECTED, mounts[]}`
/// (`symlink_resolution` is fixed `before_validation` — F1 — and is not a
/// member).
#[derive(Debug, Clone, PartialEq)]
pub struct FsPolicy {
    /// Read policy.
    pub read: ReadPolicy,
    /// Write policy.
    pub write: WritePolicy,
    /// Exec policy.
    pub exec: ExecPolicy,
    /// The protected-metadata set — must contain every `KERNEL_PROTECTED`
    /// name (`PolicyError::ProtectedPathExemption` otherwise).
    pub protected_metadata: BTreeSet<String>,
    /// Declared mounts.
    pub mounts: Vec<Mount>,
}

// ── net ───────────────────────────────────────────────────────────────────────

/// `net.mode ∈ {none, mediated, public}` (ADR-0061 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NetMode {
    /// No network namespace / connect-class syscalls (EP2-enforced, probed).
    None,
    /// The namespace is removed; the only route out is the bridged channel
    /// to a host-side mediator.
    Mediated,
    /// No mediation — requires `principal` provenance and a residual entry.
    Public,
}

/// `default_unmatched ∈ {deny, ask}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultUnmatched {
    /// Unmatched egress is denied.
    Deny,
    /// Unmatched egress asks the monitor (Stage 2 — one `approvals.requested`
    /// unit is reserved).
    Ask,
}

/// `HostPattern` — the closed sum the mediator normalises onto (ADR-0061
/// D2): `*`, `**.apex` (apex + subdomains), `*.apex` (subdomains only) or an
/// exact host. Values are stored **normalised** ([`normalize_host`]) so the
/// semantic projection excludes host *spelling*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostPattern {
    /// `*` — admissible only in `allow` rules (N2).
    Global,
    /// `**.apex` — the apex and every subdomain.
    ApexAndSubdomains(String),
    /// `*.apex` — subdomains only (never the bare apex).
    Subdomains(String),
    /// An exact host.
    Exact(String),
}

impl HostPattern {
    /// Parse a spelling into the closed sum (normalised). `BadHostPattern`
    /// on a malformed spelling (empty, bare `*`, `**`/`*` mid-string).
    pub fn parse(raw: &str) -> Result<HostPattern, PolicyError> {
        let norm = normalize_host(raw);
        if norm == "*" {
            return Ok(HostPattern::Global);
        }
        if let Some(apex) = norm.strip_prefix("**.") {
            if apex.is_empty() || apex.contains('*') {
                return Err(PolicyError::BadHostPattern {
                    host: raw.to_string(),
                });
            }
            return Ok(HostPattern::ApexAndSubdomains(apex.to_string()));
        }
        if let Some(apex) = norm.strip_prefix("*.") {
            if apex.is_empty() || apex.contains('*') {
                return Err(PolicyError::BadHostPattern {
                    host: raw.to_string(),
                });
            }
            return Ok(HostPattern::Subdomains(apex.to_string()));
        }
        if norm.is_empty() || norm.contains('*') {
            return Err(PolicyError::BadHostPattern {
                host: raw.to_string(),
            });
        }
        Ok(HostPattern::Exact(norm))
    }

    /// The canonical respelling (`*`, `**.apex`, `*.apex`, exact).
    pub fn spelling(&self) -> String {
        match self {
            HostPattern::Global => "*".to_string(),
            HostPattern::ApexAndSubdomains(a) => format!("**.{a}"),
            HostPattern::Subdomains(a) => format!("*.{a}"),
            HostPattern::Exact(h) => h.clone(),
        }
    }

    /// Whether the pattern covers a **normalised** host.
    pub fn matches(&self, host_norm: &str) -> bool {
        match self {
            HostPattern::Global => true,
            HostPattern::Exact(h) => host_norm == h,
            HostPattern::ApexAndSubdomains(a) => {
                host_norm == a || host_norm.ends_with(&format!(".{a}"))
            }
            HostPattern::Subdomains(a) => host_norm != a && host_norm.ends_with(&format!(".{a}")),
        }
    }
}

/// `normalize(host_raw)` (ADR-0061 D2 — the pure half, needed at Stage 1 for
/// the semantic projection and validation): lowercase; strip brackets,
/// ports, trailing dots; zone id preserved.
pub fn normalize_host(raw: &str) -> String {
    let mut s = raw.trim().to_lowercase();
    if let Some(rest) = s.strip_prefix('[') {
        // `[v6%zone]:port` / `[v6]` — strip the brackets and a `:` port tail.
        let inner = rest.split(']').next().unwrap_or(rest);
        s = inner.to_string();
    } else if s.matches(':').count() == 1 {
        // `host:port` — strip the port (an IPv6 spelling has ≥2 colons and
        // arrives bracketed or bare; a bare multi-colon spelling keeps its
        // zone id and needs no port strip).
        if let Some((h, _)) = s.split_once(':') {
            s = h.to_string();
        }
    }
    while s.ends_with('.') {
        s.pop();
    }
    s
}

/// `protocol ∈ {http, https_connect, socks5_tcp, socks5_udp,
/// tcp_transparent, dns}` — the request's protocol sum (ADR-0061 D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EgressProtocol {
    /// HTTP.
    Http,
    /// CONNECT-tunnelled TLS.
    HttpsConnect,
    /// SOCKS5 TCP.
    Socks5Tcp,
    /// SOCKS5 UDP.
    Socks5Udp,
    /// Transparent TCP.
    TcpTransparent,
    /// DNS — egress like any other request.
    Dns,
}

impl EgressProtocol {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EgressProtocol::Http => "http",
            EgressProtocol::HttpsConnect => "https_connect",
            EgressProtocol::Socks5Tcp => "socks5_tcp",
            EgressProtocol::Socks5Udp => "socks5_udp",
            EgressProtocol::TcpTransparent => "tcp_transparent",
            EgressProtocol::Dns => "dns",
        }
    }

    fn parse(s: &str) -> Result<EgressProtocol, PolicyError> {
        match s {
            "http" => Ok(EgressProtocol::Http),
            "https_connect" => Ok(EgressProtocol::HttpsConnect),
            "socks5_tcp" => Ok(EgressProtocol::Socks5Tcp),
            "socks5_udp" => Ok(EgressProtocol::Socks5Udp),
            "tcp_transparent" => Ok(EgressProtocol::TcpTransparent),
            "dns" => Ok(EgressProtocol::Dns),
            _ => Err(PolicyError::BadValue {
                field: "protocol",
                value: s.to_string(),
            }),
        }
    }
}

/// `decision ∈ {allow, deny}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleDecision {
    /// Permit.
    Allow,
    /// Deny — deny entries survive the meet and beat allow (I-C1/N1).
    Deny,
}

/// `EgressRule{host: HostPattern, ports, protocols, methods, decision,
/// credential_bindings: [CredentialRef], justification: Text,
/// provenance}`. Empty `ports`/`protocols`/`methods` impose no additional
/// constraint beyond the policy defaults; `credential_bindings` carries
/// `CredentialRef` spellings resolved by R-2.8.3 (Stage 2 slot);
/// `justification` is required on `definition`-sourced allow rules
/// (enforced by [`crate::meet::effective`]).
#[derive(Debug, Clone, PartialEq)]
pub struct EgressRule {
    /// The host pattern (normalised).
    pub host: HostPattern,
    /// Port constraint (empty = unconstrained).
    pub ports: Vec<u16>,
    /// Protocol constraint (empty = unconstrained).
    pub protocols: Vec<EgressProtocol>,
    /// Method constraint (empty = `methods_default` applies).
    pub methods: Vec<String>,
    /// `allow` | `deny`.
    pub decision: RuleDecision,
    /// Credential-binding refs (the Stage-2 anti-laundering slot).
    pub credential_bindings: Vec<String>,
    /// The authored justification (`Text` — required for `definition` allow
    /// rules; excluded from the semantic projection).
    pub justification: Option<Text>,
    /// The rule's own provenance (carried; excluded from the semantic
    /// projection).
    pub provenance: Option<ProvenanceRecord>,
}

/// `non_public_destinations ∈ {deny, allow_listed}` — the SSRF guard on
/// `resolved_addrs` (loopback/private/link-local/CGNAT/TEST-NET/reserved/
/// unique-local/unspecified/multicast).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonPublic {
    /// Non-public destinations denied.
    Deny,
    /// Allowed only where an allow rule lists them.
    AllowListed,
}

/// `unix_sockets.mode` — exactly one bridged helper channel always exists;
/// `allow_listed` additionally grants `unix_sockets.allow` entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnixSocketMode {
    /// Only the one bridged helper channel.
    BridgedOnly,
    /// The bridged channel plus `allow[]` grants.
    AllowListed,
}

/// `unix_sockets{mode, allow[]}`.
#[derive(Debug, Clone, PartialEq)]
pub struct UnixSockets {
    /// The mode.
    pub mode: UnixSocketMode,
    /// Granted socket paths (capability grants like hosts).
    pub allow: Vec<String>,
}

/// `dns.resolver ∈ {mediator, listed_nameservers[], none}` (ADR-0061 D3) —
/// DNS is egress.
#[derive(Debug, Clone, PartialEq)]
pub enum DnsResolver {
    /// The environment never resolves names; the mediator does (the
    /// `mediated`-mode default).
    Mediator,
    /// These nameservers may be queried.
    ListedNameservers(Vec<String>),
    /// No name resolution at all (the `none`-mode default).
    None,
}

/// `dns{resolver}`.
#[derive(Debug, Clone, PartialEq)]
pub struct DnsPolicy {
    /// The resolver.
    pub resolver: DnsResolver,
}

/// `tls{terminate = false, inspect_hooks[]}` — C2 members, declared now so
/// the sum is closed (`terminate = true` is a `lowering_loss` on the EP2
/// model — the Stage-1 backend cannot honour it).
#[derive(Debug, Clone, PartialEq)]
pub struct TlsPolicy {
    /// TLS termination at the mediator (default `false`).
    pub terminate: bool,
    /// Declared inspect-hook refs (`validator`-class plugins).
    pub inspect_hooks: Vec<String>,
}

/// `net{mode, default_unmatched, rules, non_public_destinations,
/// local_binding = false, unix_sockets, dns, tls, upstream_proxy?,
/// methods_default}`.
#[derive(Debug, Clone, PartialEq)]
pub struct NetPolicy {
    /// The mode.
    pub mode: NetMode,
    /// Unmatched-egress default.
    pub default_unmatched: DefaultUnmatched,
    /// The ordered rule set (`deny` evaluated before `allow` — I-C1/N1).
    pub rules: Vec<EgressRule>,
    /// The non-public-destination guard.
    pub non_public_destinations: NonPublic,
    /// Whether local listeners may bind (default `false`).
    pub local_binding: bool,
    /// Unix-socket policy.
    pub unix_sockets: UnixSockets,
    /// DNS policy.
    pub dns: DnsPolicy,
    /// TLS policy (C2 members).
    pub tls: TlsPolicy,
    /// Declared upstream proxy ref — **never** counts as `mediated`
    /// enforcement (setting one is a `lowering_loss`).
    pub upstream_proxy: Option<String>,
    /// `methods_default` (kernel default `{GET, HEAD, OPTIONS}`; a rule may
    /// widen per-rule).
    pub methods_default: Vec<String>,
}

// ── proc ──────────────────────────────────────────────────────────────────────

/// `isolation_class ∈ {none, process_sandbox, namespaces, user_space_kernel,
/// microvm, external}` (CF-291: the *mechanism* on the handle).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationClass {
    /// No isolation.
    None,
    /// The process-sandbox helper boundary (the Stage-1 model).
    ProcessSandbox,
    /// Kernel namespaces.
    Namespaces,
    /// A user-space kernel (C1).
    UserSpaceKernel,
    /// A microVM (C1).
    Microvm,
    /// A participant-supplied boundary (hosted; C1) — evidence `reported`,
    /// never `probed`/`attested` here.
    External,
}

impl IsolationClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            IsolationClass::None => "none",
            IsolationClass::ProcessSandbox => "process_sandbox",
            IsolationClass::Namespaces => "namespaces",
            IsolationClass::UserSpaceKernel => "user_space_kernel",
            IsolationClass::Microvm => "microvm",
            IsolationClass::External => "external",
        }
    }

    /// Boundary strength for the meet (higher = more kernel-enforced
    /// isolation). `external` ranks lowest — it is a *claim* of a boundary
    /// the kernel does not enforce, weaker than even `none` (which the
    /// kernel still bounds at EP2).
    pub fn strength(self) -> u8 {
        match self {
            IsolationClass::External => 0,
            IsolationClass::None => 1,
            IsolationClass::ProcessSandbox => 2,
            IsolationClass::Namespaces => 3,
            IsolationClass::UserSpaceKernel => 4,
            IsolationClass::Microvm => 5,
        }
    }

    fn parse(s: &str) -> Result<IsolationClass, PolicyError> {
        match s {
            "none" => Ok(IsolationClass::None),
            "process_sandbox" => Ok(IsolationClass::ProcessSandbox),
            "namespaces" => Ok(IsolationClass::Namespaces),
            "user_space_kernel" => Ok(IsolationClass::UserSpaceKernel),
            "microvm" => Ok(IsolationClass::Microvm),
            "external" => Ok(IsolationClass::External),
            _ => Err(PolicyError::BadValue {
                field: "isolation_class",
                value: s.to_string(),
            }),
        }
    }
}

/// `unshare ⊆ {net, pid, ipc, user, mount}` — each member a namespace the
/// sandbox unshares (more = stronger isolation = narrower).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnshareKind {
    /// The network namespace.
    Net,
    /// The PID namespace.
    Pid,
    /// The IPC namespace.
    Ipc,
    /// The user namespace.
    User,
    /// The mount namespace.
    Mount,
}

impl UnshareKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            UnshareKind::Net => "net",
            UnshareKind::Pid => "pid",
            UnshareKind::Ipc => "ipc",
            UnshareKind::User => "user",
            UnshareKind::Mount => "mount",
        }
    }

    fn parse(s: &str) -> Result<UnshareKind, PolicyError> {
        match s {
            "net" => Ok(UnshareKind::Net),
            "pid" => Ok(UnshareKind::Pid),
            "ipc" => Ok(UnshareKind::Ipc),
            "user" => Ok(UnshareKind::User),
            "mount" => Ok(UnshareKind::Mount),
            _ => Err(PolicyError::BadValue {
                field: "unshare",
                value: s.to_string(),
            }),
        }
    }
}

/// `ptrace`/`setuid` — fixed `deny` in `ContainmentPolicy/1` (a closed
/// one-member sum; `allow` is a dialect bump, never a spelling).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatePolicy {
    /// Denied — the only member at v1.
    Deny,
}

/// `env.inherit ∈ {all, core, none}` — stricter toward `none`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EnvInherit {
    /// Inherit the parent environment.
    All,
    /// Inherit the core set only.
    Core,
    /// Inherit nothing.
    None,
}

impl EnvInherit {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EnvInherit::All => "all",
            EnvInherit::Core => "core",
            EnvInherit::None => "none",
        }
    }
}

/// `env{inherit, exclude[], set, include_only[]}`.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvPolicy {
    /// The inheritance class.
    pub inherit: EnvInherit,
    /// Names always excluded.
    pub exclude: Vec<String>,
    /// Names injected with a fixed value.
    pub set: BTreeMap<String, String>,
    /// When non-empty, the only inheritable names.
    pub include_only: Vec<String>,
}

/// `proc{isolation_class, unshare, no_new_privs = true, syscall_filter,
/// ptrace = deny, setuid = deny, max_processes, env, die_with_parent}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcPolicy {
    /// The mechanism class.
    pub isolation_class: IsolationClass,
    /// The unshared namespaces.
    pub unshare: BTreeSet<UnshareKind>,
    /// `no_new_privs` (kernel default `true`; `false` is a loosening only a
    /// kernel layer may declare).
    pub no_new_privs: bool,
    /// A pointer-rule ref to a syscall-filter record (CF-055).
    pub syscall_filter: Option<String>,
    /// `ptrace = deny` — fixed.
    pub ptrace: GatePolicy,
    /// `setuid = deny` — fixed.
    pub setuid: GatePolicy,
    /// A process-count bound.
    pub max_processes: Option<u64>,
    /// Environment inheritance.
    pub env: EnvPolicy,
    /// `die_with_parent` (kernel default `true`).
    pub die_with_parent: bool,
}

// ── resources ─────────────────────────────────────────────────────────────────

/// `resources{cpu_ms?, memory_bytes?, disk_bytes?, pids?, open_files?,
/// wall_ms?, network_bytes_out?, network_calls?}` — each member a bound
/// (mapped to a `BudgetNode`/`ext` dimension at Stage 2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceLimits {
    /// CPU milliseconds.
    pub cpu_ms: Option<u64>,
    /// Memory bytes.
    pub memory_bytes: Option<u64>,
    /// Disk bytes.
    pub disk_bytes: Option<u64>,
    /// PID count.
    pub pids: Option<u64>,
    /// Open-file count.
    pub open_files: Option<u64>,
    /// Wall-clock milliseconds.
    pub wall_ms: Option<u64>,
    /// Egress byte bound.
    pub network_bytes_out: Option<u64>,
    /// Egress call bound.
    pub network_calls: Option<u64>,
}

impl ResourceLimits {
    /// `true` when no bound is set (nothing relies on the `resources`
    /// field group).
    pub fn is_empty(&self) -> bool {
        self == &ResourceLimits::default()
    }

    /// The set members as `(field, value)` pairs — the loss enumeration's
    /// source.
    pub fn set_fields(&self) -> Vec<(&'static str, u64)> {
        let mut out = Vec::new();
        let mut push = |name: &'static str, v: Option<u64>| {
            if let Some(v) = v {
                out.push((name, v));
            }
        };
        push("resources.cpu_ms", self.cpu_ms);
        push("resources.memory_bytes", self.memory_bytes);
        push("resources.disk_bytes", self.disk_bytes);
        push("resources.pids", self.pids);
        push("resources.open_files", self.open_files);
        push("resources.wall_ms", self.wall_ms);
        push("resources.network_bytes_out", self.network_bytes_out);
        push("resources.network_calls", self.network_calls);
        out
    }
}

// ── amendment ─────────────────────────────────────────────────────────────────

/// `amendment.allowed_bases ⊆ {approval, policy_rule, seal}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AmendmentBasis {
    /// A human approval endorsement.
    Approval,
    /// A `policy_rule` (phase plans sealed in the experiment design).
    PolicyRule,
    /// A definition `seal`.
    Seal,
}

impl AmendmentBasis {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AmendmentBasis::Approval => "approval",
            AmendmentBasis::PolicyRule => "policy_rule",
            AmendmentBasis::Seal => "seal",
        }
    }

    fn parse(s: &str) -> Result<AmendmentBasis, PolicyError> {
        match s {
            "approval" => Ok(AmendmentBasis::Approval),
            "policy_rule" => Ok(AmendmentBasis::PolicyRule),
            "seal" => Ok(AmendmentBasis::Seal),
            _ => Err(PolicyError::BadValue {
                field: "allowed_bases",
                value: s.to_string(),
            }),
        }
    }
}

/// `amendment{allowed_bases, strict: bool, session_cache: bool,
/// persist_scope_ceiling ∈ PersistenceScope}` — the escape-hatch policy
/// (the `amend()` operation itself is Stage 2; the *policy* over it is
/// Stage-1 data).
#[derive(Debug, Clone, PartialEq)]
pub struct AmendmentPolicy {
    /// The bases `amend` may stand on.
    pub allowed_bases: BTreeSet<AmendmentBasis>,
    /// `strict` — under strict, a model-issued "disable sandbox"/"allow
    /// host" is refused outright (`StrictMode`) rather than converted to an
    /// `ask`. Kernel default `true`; only a `principal` layer may loosen.
    pub strict: bool,
    /// Session caching of an approved canonical pattern (ledgered as
    /// `decider = cache`).
    pub session_cache: bool,
    /// The persistence ceiling an amendment may reach.
    pub persist_scope_ceiling: PersistenceScope,
}

/// The looseness rank of a `PersistenceScope` for the meet: raising the
/// ceiling (toward `definition`) is a loosening.
pub fn persistence_rank(s: PersistenceScope) -> u8 {
    match s {
        PersistenceScope::Turn => 0,
        PersistenceScope::Run => 1,
        PersistenceScope::Session => 2,
        PersistenceScope::Project => 3,
        PersistenceScope::User => 4,
        PersistenceScope::Definition => 5,
    }
}

// ── residual channels ─────────────────────────────────────────────────────────

/// `residual_channels[].kind ∈ {approved_host_body, tls_opaque, dns_names,
/// unix_socket, side_channel, host_bridge}` (I-C6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResidualKind {
    /// Bodies to an approved host are opaque to the boundary.
    ApprovedHostBody,
    /// TLS-terminated content the boundary cannot see.
    TlsOpaque,
    /// DNS names observed by the resolver.
    DnsNames,
    /// A unix-socket channel.
    UnixSocket,
    /// A declared side channel.
    SideChannel,
    /// The host bridge itself.
    HostBridge,
}

impl ResidualKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ResidualKind::ApprovedHostBody => "approved_host_body",
            ResidualKind::TlsOpaque => "tls_opaque",
            ResidualKind::DnsNames => "dns_names",
            ResidualKind::UnixSocket => "unix_socket",
            ResidualKind::SideChannel => "side_channel",
            ResidualKind::HostBridge => "host_bridge",
        }
    }

    fn parse(s: &str) -> Result<ResidualKind, PolicyError> {
        match s {
            "approved_host_body" => Ok(ResidualKind::ApprovedHostBody),
            "tls_opaque" => Ok(ResidualKind::TlsOpaque),
            "dns_names" => Ok(ResidualKind::DnsNames),
            "unix_socket" => Ok(ResidualKind::UnixSocket),
            "side_channel" => Ok(ResidualKind::SideChannel),
            "host_bridge" => Ok(ResidualKind::HostBridge),
            _ => Err(PolicyError::BadValue {
                field: "residual.kind",
                value: s.to_string(),
            }),
        }
    }
}

/// `residual_channels[{kind, statement, closer, owner}]` — declared, owned,
/// with a closer (I-C6; an unowned residual fails readiness, T-LCD-05).
/// `statement` is a `Text` leaf (an authored declaration — provenance-
/// bearing); it is excluded from the semantic projection like every other
/// prose member.
#[derive(Debug, Clone, PartialEq)]
pub struct ResidualChannel {
    /// The channel kind.
    pub kind: ResidualKind,
    /// The declared statement.
    pub statement: Text,
    /// The closer — the mechanism/item that closes this channel.
    pub closer: String,
    /// The owner.
    pub owner: String,
}

// ── the record ────────────────────────────────────────────────────────────────

/// `ContainmentPolicy/1` — the full record (§5g.4 §3).
#[derive(Debug, Clone, PartialEq)]
pub struct ContainmentPolicy {
    /// `policy_id` — the semantic projection's id (`semantic_id`).
    pub policy_id: String,
    /// `version_id` — the canonical record's id.
    pub version_id: String,
    /// The provenance — the layer's authority is **conferred** here (CC2).
    pub provenance: ProvenanceRecord,
    /// Filesystem policy.
    pub fs: FsPolicy,
    /// Network policy.
    pub net: NetPolicy,
    /// Process policy.
    pub proc: ProcPolicy,
    /// Resource bounds.
    pub resources: ResourceLimits,
    /// The escape-hatch policy.
    pub amendment: AmendmentPolicy,
    /// Declared residual channels.
    pub residual_channels: Vec<ResidualChannel>,
    /// Extension members — **never** decide enforcement (ADR-0060 D5).
    pub ext: BTreeMap<String, Json>,
}

/// The policy-validation / decode error sum.
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyError {
    /// A decode failure (unknown/malformed member) — `path` names the member.
    BadMember {
        /// The member path.
        path: String,
    },
    /// A closed sum spelled a non-member.
    BadValue {
        /// The field.
        field: &'static str,
        /// The offending value.
        value: String,
    },
    /// A `HostPattern` spelling that isn't the closed sum.
    BadHostPattern {
        /// The spelling.
        host: String,
    },
    /// N2 — a global `*` in a `deny` rule.
    GlobalDenyRule,
    /// N3 — `mode = none` carries rules.
    ModeNoneHasRules,
    /// N4a — `mode = public` requires `provenance.authority ≥ principal`.
    PublicBelowPrincipal,
    /// N4b — `mode = public` requires a `residual_channels` entry.
    PublicWithoutResidual,
    /// `fs.protected_metadata` does not cover `KERNEL_PROTECTED` — an
    /// attempted exemption (`ProtectedPathExemption`).
    ProtectedPathExemption {
        /// The exempted kernel name.
        name: String,
    },
    /// `read.allow` is populated under `allow_all_except` (inert config —
    /// refused, never silently ignored).
    AllowListUnderAllowAllExcept,
    /// `unix_sockets.allow` is populated under `bridged_only` (inert).
    UnixAllowUnderBridgedOnly,
    /// A `WritableRoot`/`mount`/`deny` member is empty or malformed.
    EmptyMember {
        /// The member path.
        path: &'static str,
    },
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyError::BadMember { path } => write!(f, "bad member {path}"),
            PolicyError::BadValue { field, value } => {
                write!(f, "bad value {value:?} for {field}")
            }
            PolicyError::BadHostPattern { host } => write!(f, "bad host pattern {host:?}"),
            PolicyError::GlobalDenyRule => write!(f, "global `*` in a deny rule (N2)"),
            PolicyError::ModeNoneHasRules => write!(f, "rules under mode none (N3)"),
            PolicyError::PublicBelowPrincipal => {
                write!(f, "mode public requires authority >= principal (N4)")
            }
            PolicyError::PublicWithoutResidual => {
                write!(f, "mode public requires a residual_channels entry (N4)")
            }
            PolicyError::ProtectedPathExemption { name } => {
                write!(f, "KERNEL_PROTECTED name {name} exempted")
            }
            PolicyError::AllowListUnderAllowAllExcept => {
                write!(f, "read.allow under allow_all_except")
            }
            PolicyError::UnixAllowUnderBridgedOnly => {
                write!(f, "unix_sockets.allow under bridged_only")
            }
            PolicyError::EmptyMember { path } => write!(f, "empty member {path}"),
        }
    }
}

impl std::error::Error for PolicyError {}

impl ContainmentPolicy {
    /// Validate the invariants that hold of *every* policy document —
    /// `protected_metadata ⊇ KERNEL_PROTECTED` (I-C2), the structural rules
    /// ([`Self::validate_structure`]), and N4 (`mode = public` requires
    /// `provenance.authority ≥ principal` and a residual entry).
    pub fn validate(&self) -> Result<(), PolicyError> {
        for name in KERNEL_PROTECTED {
            if !self.fs.protected_metadata.contains(*name) {
                return Err(PolicyError::ProtectedPathExemption {
                    name: (*name).to_string(),
                });
            }
        }
        self.validate_structure()?;
        if self.net.mode == NetMode::Public {
            if self.provenance.authority < AuthorityClass::Principal {
                return Err(PolicyError::PublicBelowPrincipal);
            }
            if self.residual_channels.is_empty() {
                return Err(PolicyError::PublicWithoutResidual);
            }
        }
        Ok(())
    }

    /// The *structural* invariants — N2 (no global `*` in a deny rule), N3
    /// (`mode = none` carries no rules) and the no-inert-member rules (an
    /// inert member is refused, never silently ignored). Deliberately
    /// excludes the authority-dependent N4 — [`crate::meet::effective`]
    /// applies this to each layer so a `delegate`-class `public` declaration
    /// surfaces as `ContainmentWidening`, not a document error.
    pub fn validate_structure(&self) -> Result<(), PolicyError> {
        if self.net.mode == NetMode::None && !self.net.rules.is_empty() {
            return Err(PolicyError::ModeNoneHasRules);
        }
        for r in &self.net.rules {
            if r.decision == RuleDecision::Deny && r.host == HostPattern::Global {
                return Err(PolicyError::GlobalDenyRule);
            }
        }
        if self.fs.read.mode == ReadMode::AllowAllExcept && !self.fs.read.allow.is_empty() {
            return Err(PolicyError::AllowListUnderAllowAllExcept);
        }
        if self.net.unix_sockets.mode == UnixSocketMode::BridgedOnly
            && !self.net.unix_sockets.allow.is_empty()
        {
            return Err(PolicyError::UnixAllowUnderBridgedOnly);
        }
        for w in &self.fs.write.allow {
            if w.root.is_empty() {
                return Err(PolicyError::EmptyMember {
                    path: "fs.write.allow[].root",
                });
            }
        }
        for m in &self.fs.mounts {
            if m.target.is_empty() {
                return Err(PolicyError::EmptyMember {
                    path: "fs.mounts[].target",
                });
            }
        }
        for r in &self.residual_channels {
            if r.closer.is_empty() || r.owner.is_empty() {
                return Err(PolicyError::EmptyMember {
                    path: "residual_channels[].closer/owner",
                });
            }
        }
        Ok(())
    }

    /// The semantic projection (ADR-0060 D1/D5; T-LCD-10) — the record minus
    /// `{policy_id, version_id, provenance, ext}`, per-rule
    /// `justification`/`provenance`, and residual `statement` text.
    pub fn semantic_projection(&self) -> Json {
        policy_json(self, false, true)
    }

    /// The `version_id` basis — the canonical record minus the computed id
    /// members (provenance and ext included).
    pub fn identity_basis(&self) -> Json {
        policy_json(self, false, false)
    }

    /// The full canonical record.
    pub fn to_json(&self) -> Json {
        policy_json(self, true, false)
    }

    /// `policy_id` — the semantic-projection id under the kind's semantic
    /// domain (`containment_policy#semantic`).
    pub fn semantic_id(&self) -> String {
        idp_id(
            &format!("{}#semantic", RecordKind::ContainmentPolicy.domain_tag()),
            self.semantic_projection().to_canonical_string().as_bytes(),
        )
    }

    /// `version_id` — `identify_bytes` over the canonical identity basis.
    pub fn compute_version_id(&self) -> String {
        identify_bytes(
            RecordKind::ContainmentPolicy,
            self.identity_basis().to_canonical_string().as_bytes(),
        )
    }

    /// Fill both coordinates (idempotent).
    pub fn compute_ids(&mut self) {
        self.policy_id = self.semantic_id();
        self.version_id = self.compute_version_id();
    }

    /// Decode the canonical record (strict — an unknown member is
    /// `BadMember`, never silently dropped, CC4).
    pub fn from_json(j: &Json) -> Result<ContainmentPolicy, PolicyError> {
        decode::policy_from_json(j)
    }
}

/// The kernel-default policy (§5g.4 §2.1) — deny-by-default, `at` the
/// caller's logical seq. Provenance is `kernel` — the kernel mints the
/// default; it is not an authored layer.
pub fn kernel_default(at: u64) -> ContainmentPolicy {
    let mut p = ContainmentPolicy {
        policy_id: String::new(),
        version_id: String::new(),
        provenance: ProvenanceRecord::kernel("hh-containment/kernel_default", at),
        fs: FsPolicy {
            read: ReadPolicy {
                mode: ReadMode::AllowAllExcept,
                deny: KERNEL_DENY.iter().map(|s| s.to_string()).collect(),
                allow_within_deny: vec![],
                allow: vec![],
            },
            write: WritePolicy {
                allow: vec![],
                deny_within_allow: vec![],
            },
            exec: ExecPolicy::Allow(vec![]),
            protected_metadata: KERNEL_PROTECTED.iter().map(|s| s.to_string()).collect(),
            mounts: vec![],
        },
        net: NetPolicy {
            mode: NetMode::None,
            default_unmatched: DefaultUnmatched::Deny,
            rules: vec![],
            non_public_destinations: NonPublic::Deny,
            local_binding: false,
            unix_sockets: UnixSockets {
                mode: UnixSocketMode::BridgedOnly,
                allow: vec![],
            },
            dns: DnsPolicy {
                resolver: DnsResolver::None,
            },
            tls: TlsPolicy {
                terminate: false,
                inspect_hooks: vec![],
            },
            upstream_proxy: None,
            methods_default: ["GET", "HEAD", "OPTIONS"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        },
        proc: ProcPolicy {
            isolation_class: IsolationClass::ProcessSandbox,
            unshare: BTreeSet::from([
                UnshareKind::Net,
                UnshareKind::Pid,
                UnshareKind::Ipc,
                UnshareKind::User,
                UnshareKind::Mount,
            ]),
            no_new_privs: true,
            syscall_filter: None,
            ptrace: GatePolicy::Deny,
            setuid: GatePolicy::Deny,
            max_processes: None,
            env: EnvPolicy {
                inherit: EnvInherit::Core,
                exclude: vec![],
                set: BTreeMap::new(),
                include_only: vec![],
            },
            die_with_parent: true,
        },
        resources: ResourceLimits::default(),
        amendment: AmendmentPolicy {
            allowed_bases: BTreeSet::from([AmendmentBasis::Approval]),
            strict: true,
            session_cache: false,
            persist_scope_ceiling: PersistenceScope::Run,
        },
        residual_channels: vec![],
        ext: BTreeMap::new(),
    };
    p.compute_ids();
    p
}

// ── canonical JSON ────────────────────────────────────────────────────────────

fn strs(v: &[String]) -> Json {
    Json::Arr(v.iter().map(|s| Json::str(s.clone())).collect())
}

fn set_strs(v: &BTreeSet<String>) -> Json {
    Json::Arr(v.iter().map(|s| Json::str(s.clone())).collect())
}

fn u16s(v: &[u16]) -> Json {
    Json::Arr(v.iter().map(|p| Json::Int(*p as i64)).collect())
}

fn protos(v: &[EgressProtocol]) -> Json {
    Json::Arr(v.iter().map(|p| Json::str(p.as_str())).collect())
}

fn opt_str(o: &Option<String>) -> Json {
    o.as_ref()
        .map(|s| Json::str(s.clone()))
        .unwrap_or(Json::Null)
}

fn opt_u64(o: &Option<u64>) -> Json {
    o.map(|v| Json::Int(v as i64)).unwrap_or(Json::Null)
}

fn writable_root_json(w: &WritableRoot) -> Json {
    Json::obj([
        ("root", Json::str(w.root.clone())),
        ("read_only_subpaths", strs(&w.read_only_subpaths)),
        (
            "protected_metadata_names",
            strs(&w.protected_metadata_names),
        ),
    ])
}

fn mount_json(m: &Mount) -> Json {
    let (src_kind, src_val) = match &m.source {
        MountSource::Content(c) => ("content", c.clone()),
        MountSource::HostPath(h) => ("host_path", h.clone()),
    };
    let mode = match &m.mode {
        MountMode::Ro => Json::str("ro"),
        MountMode::Rw => Json::str("rw"),
        MountMode::Masked(s) => Json::obj([("masked", Json::str(s.clone()))]),
    };
    Json::obj([
        (
            "source",
            Json::obj([("kind", Json::str(src_kind)), ("value", Json::str(src_val))]),
        ),
        ("target", Json::str(m.target.clone())),
        ("mode", mode),
    ])
}

fn rule_json(r: &EgressRule, semantic: bool) -> Json {
    let mut m = Json::obj([
        ("host", Json::str(r.host.spelling())),
        ("ports", u16s(&r.ports)),
        ("protocols", protos(&r.protocols)),
        ("methods", strs(&r.methods)),
        (
            "decision",
            Json::str(match r.decision {
                RuleDecision::Allow => "allow",
                RuleDecision::Deny => "deny",
            }),
        ),
        ("credential_bindings", strs(&r.credential_bindings)),
    ]);
    if !semantic {
        if let Json::Obj(ref mut map) = m {
            map.insert(
                "justification".to_string(),
                r.justification
                    .as_ref()
                    .map(Text::to_json)
                    .unwrap_or(Json::Null),
            );
            map.insert(
                "provenance".to_string(),
                r.provenance
                    .as_ref()
                    .map(ProvenanceRecord::to_json)
                    .unwrap_or(Json::Null),
            );
        }
    }
    m
}

fn unshare_json(u: &BTreeSet<UnshareKind>) -> Json {
    Json::Arr(u.iter().map(|k| Json::str(k.as_str())).collect())
}

fn dns_json(d: &DnsResolver) -> Json {
    match d {
        DnsResolver::Mediator => Json::obj([("kind", Json::str("mediator"))]),
        DnsResolver::ListedNameservers(ns) => Json::obj([
            ("kind", Json::str("listed_nameservers")),
            ("nameservers", strs(ns)),
        ]),
        DnsResolver::None => Json::obj([("kind", Json::str("none"))]),
    }
}

fn residual_json(r: &ResidualChannel, semantic: bool) -> Json {
    let mut m = Json::obj([
        ("kind", Json::str(r.kind.as_str())),
        ("closer", Json::str(r.closer.clone())),
        ("owner", Json::str(r.owner.clone())),
    ]);
    if !semantic {
        if let Json::Obj(ref mut map) = m {
            map.insert("statement".to_string(), r.statement.to_json());
        }
    }
    m
}

/// The canonical JSON — `with_ids` includes `{policy_id, version_id}`;
/// `semantic` selects the projection (prose/provenance/ext excluded).
fn policy_json(p: &ContainmentPolicy, with_ids: bool, semantic: bool) -> Json {
    let mut m = BTreeMap::new();
    if with_ids {
        m.insert("policy_id".to_string(), Json::str(p.policy_id.clone()));
        m.insert("version_id".to_string(), Json::str(p.version_id.clone()));
    }
    if !semantic {
        m.insert("provenance".to_string(), p.provenance.to_json());
    }
    m.insert(
        "fs".to_string(),
        Json::obj([
            (
                "read",
                Json::obj([
                    (
                        "mode",
                        Json::str(match p.fs.read.mode {
                            ReadMode::AllowAllExcept => "allow_all_except",
                            ReadMode::AllowOnly => "allow_only",
                        }),
                    ),
                    ("deny", strs(&p.fs.read.deny)),
                    ("allow_within_deny", strs(&p.fs.read.allow_within_deny)),
                    ("allow", strs(&p.fs.read.allow)),
                ]),
            ),
            (
                "write",
                Json::obj([
                    (
                        "allow",
                        Json::Arr(p.fs.write.allow.iter().map(writable_root_json).collect()),
                    ),
                    ("deny_within_allow", strs(&p.fs.write.deny_within_allow)),
                ]),
            ),
            (
                "exec",
                match &p.fs.exec {
                    ExecPolicy::Any => Json::obj([("mode", Json::str("any"))]),
                    ExecPolicy::Allow(set) => {
                        Json::obj([("mode", Json::str("allow")), ("allow", strs(set))])
                    }
                },
            ),
            ("protected_metadata", set_strs(&p.fs.protected_metadata)),
            (
                "mounts",
                Json::Arr(p.fs.mounts.iter().map(mount_json).collect()),
            ),
            // F1 — fixed, declared in the canonical form so the record is
            // self-describing; there is no settable member.
            ("symlink_resolution", Json::str(SYMLINK_RESOLUTION)),
        ]),
    );
    m.insert(
        "net".to_string(),
        Json::obj([
            (
                "mode",
                Json::str(match p.net.mode {
                    NetMode::None => "none",
                    NetMode::Mediated => "mediated",
                    NetMode::Public => "public",
                }),
            ),
            (
                "default_unmatched",
                Json::str(match p.net.default_unmatched {
                    DefaultUnmatched::Deny => "deny",
                    DefaultUnmatched::Ask => "ask",
                }),
            ),
            (
                "rules",
                Json::Arr(p.net.rules.iter().map(|r| rule_json(r, semantic)).collect()),
            ),
            (
                "non_public_destinations",
                Json::str(match p.net.non_public_destinations {
                    NonPublic::Deny => "deny",
                    NonPublic::AllowListed => "allow_listed",
                }),
            ),
            ("local_binding", Json::Bool(p.net.local_binding)),
            (
                "unix_sockets",
                Json::obj([
                    (
                        "mode",
                        Json::str(match p.net.unix_sockets.mode {
                            UnixSocketMode::BridgedOnly => "bridged_only",
                            UnixSocketMode::AllowListed => "allow_listed",
                        }),
                    ),
                    ("allow", strs(&p.net.unix_sockets.allow)),
                ]),
            ),
            (
                "dns",
                Json::obj([("resolver", dns_json(&p.net.dns.resolver))]),
            ),
            (
                "tls",
                Json::obj([
                    ("terminate", Json::Bool(p.net.tls.terminate)),
                    ("inspect_hooks", strs(&p.net.tls.inspect_hooks)),
                ]),
            ),
            ("upstream_proxy", opt_str(&p.net.upstream_proxy)),
            ("methods_default", strs(&p.net.methods_default)),
        ]),
    );
    m.insert(
        "proc".to_string(),
        Json::obj([
            (
                "isolation_class",
                Json::str(p.proc.isolation_class.as_str()),
            ),
            ("unshare", unshare_json(&p.proc.unshare)),
            ("no_new_privs", Json::Bool(p.proc.no_new_privs)),
            ("syscall_filter", opt_str(&p.proc.syscall_filter)),
            (
                "ptrace",
                Json::str(match p.proc.ptrace {
                    GatePolicy::Deny => "deny",
                }),
            ),
            (
                "setuid",
                Json::str(match p.proc.setuid {
                    GatePolicy::Deny => "deny",
                }),
            ),
            ("max_processes", opt_u64(&p.proc.max_processes)),
            (
                "env",
                Json::obj([
                    ("inherit", Json::str(p.proc.env.inherit.as_str())),
                    ("exclude", strs(&p.proc.env.exclude)),
                    (
                        "set",
                        Json::Obj(
                            p.proc
                                .env
                                .set
                                .iter()
                                .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                                .collect(),
                        ),
                    ),
                    ("include_only", strs(&p.proc.env.include_only)),
                ]),
            ),
            ("die_with_parent", Json::Bool(p.proc.die_with_parent)),
        ]),
    );
    m.insert(
        "resources".to_string(),
        Json::obj([
            ("cpu_ms", opt_u64(&p.resources.cpu_ms)),
            ("memory_bytes", opt_u64(&p.resources.memory_bytes)),
            ("disk_bytes", opt_u64(&p.resources.disk_bytes)),
            ("pids", opt_u64(&p.resources.pids)),
            ("open_files", opt_u64(&p.resources.open_files)),
            ("wall_ms", opt_u64(&p.resources.wall_ms)),
            ("network_bytes_out", opt_u64(&p.resources.network_bytes_out)),
            ("network_calls", opt_u64(&p.resources.network_calls)),
        ]),
    );
    m.insert(
        "amendment".to_string(),
        Json::obj([
            (
                "allowed_bases",
                Json::Arr(
                    p.amendment
                        .allowed_bases
                        .iter()
                        .map(|b| Json::str(b.as_str()))
                        .collect(),
                ),
            ),
            ("strict", Json::Bool(p.amendment.strict)),
            ("session_cache", Json::Bool(p.amendment.session_cache)),
            (
                "persist_scope_ceiling",
                Json::str(p.amendment.persist_scope_ceiling.as_str()),
            ),
        ]),
    );
    m.insert(
        "residual_channels".to_string(),
        Json::Arr(
            p.residual_channels
                .iter()
                .map(|r| residual_json(r, semantic))
                .collect(),
        ),
    );
    if !semantic {
        m.insert("ext".to_string(), Json::Obj(p.ext.clone()));
    }
    Json::Obj(m)
}

// ── decode ────────────────────────────────────────────────────────────────────

mod decode {
    use super::*;

    fn bad(path: &str) -> PolicyError {
        PolicyError::BadMember {
            path: path.to_string(),
        }
    }

    fn req<'a>(j: &'a Json, k: &str, path: &str) -> Result<&'a Json, PolicyError> {
        j.get(k).ok_or_else(|| bad(&format!("{path}.{k}")))
    }

    fn s_at(j: &Json, k: &str, path: &str) -> Result<String, PolicyError> {
        req(j, k, path)?
            .as_str()
            .map(String::from)
            .ok_or_else(|| bad(&format!("{path}.{k}")))
    }

    fn bool_at(j: &Json, k: &str, path: &str) -> Result<bool, PolicyError> {
        match req(j, k, path)? {
            Json::Bool(b) => Ok(*b),
            _ => Err(bad(&format!("{path}.{k}"))),
        }
    }

    fn arr_at<'a>(j: &'a Json, k: &str, path: &str) -> Result<&'a [Json], PolicyError> {
        match req(j, k, path)? {
            Json::Arr(a) => Ok(a),
            _ => Err(bad(&format!("{path}.{k}"))),
        }
    }

    fn str_list(j: &Json, k: &str, path: &str) -> Result<Vec<String>, PolicyError> {
        arr_at(j, k, path)?
            .iter()
            .enumerate()
            .map(|(i, v)| {
                v.as_str()
                    .map(String::from)
                    .ok_or_else(|| bad(&format!("{path}.{k}[{i}]")))
            })
            .collect()
    }

    fn obj_members<'a>(j: &'a Json, path: &str) -> Result<&'a BTreeMap<String, Json>, PolicyError> {
        match j {
            Json::Obj(m) => Ok(m),
            _ => Err(bad(path)),
        }
    }

    fn check_keys(j: &Json, path: &str, allowed: &[&str]) -> Result<(), PolicyError> {
        for k in obj_members(j, path)?.keys() {
            if !allowed.contains(&k.as_str()) {
                return Err(bad(&format!("{path}.{k}")));
            }
        }
        Ok(())
    }

    fn u64_at(j: &Json, k: &str, path: &str) -> Result<Option<u64>, PolicyError> {
        match req(j, k, path)? {
            Json::Null => Ok(None),
            Json::Int(i) if *i >= 0 => Ok(Some(*i as u64)),
            _ => Err(bad(&format!("{path}.{k}"))),
        }
    }

    pub fn policy_from_json(j: &Json) -> Result<ContainmentPolicy, PolicyError> {
        check_keys(
            j,
            "policy",
            &[
                "policy_id",
                "version_id",
                "provenance",
                "fs",
                "net",
                "proc",
                "resources",
                "amendment",
                "residual_channels",
                "ext",
            ],
        )?;
        let fs = fs_from(req(j, "fs", "policy")?)?;
        let net = net_from(req(j, "net", "policy")?)?;
        let proc_ = proc_from(req(j, "proc", "policy")?)?;
        let res = res_from(req(j, "resources", "policy")?)?;
        let amend = amend_from(req(j, "amendment", "policy")?)?;
        let residuals = arr_at(j, "residual_channels", "policy")?
            .iter()
            .enumerate()
            .map(|(i, r)| residual_from(r, i))
            .collect::<Result<Vec<_>, _>>()?;
        let ext = match j.get("ext") {
            Some(Json::Obj(m)) => m.clone(),
            Some(_) => return Err(bad("policy.ext")),
            None => BTreeMap::new(),
        };
        let provenance = ProvenanceRecord::from_json(req(j, "provenance", "policy")?)
            .map_err(|_| bad("policy.provenance"))?;
        let p = ContainmentPolicy {
            policy_id: s_at(j, "policy_id", "policy")?,
            version_id: s_at(j, "version_id", "policy")?,
            provenance,
            fs,
            net,
            proc: proc_,
            resources: res,
            amendment: amend,
            residual_channels: residuals,
            ext,
        };
        p.validate()?;
        Ok(p)
    }

    fn fs_from(j: &Json) -> Result<FsPolicy, PolicyError> {
        check_keys(
            j,
            "fs",
            &[
                "read",
                "write",
                "exec",
                "protected_metadata",
                "mounts",
                "symlink_resolution",
            ],
        )?;
        // F1 — the fixed member must spell the one value.
        match j.get("symlink_resolution").and_then(Json::as_str) {
            Some(SYMLINK_RESOLUTION) => {}
            _ => return Err(bad("fs.symlink_resolution")),
        }
        let read = req(j, "read", "fs")?;
        check_keys(
            read,
            "fs.read",
            &["mode", "deny", "allow_within_deny", "allow"],
        )?;
        let read = ReadPolicy {
            mode: match s_at(read, "mode", "fs.read")?.as_str() {
                "allow_all_except" => ReadMode::AllowAllExcept,
                "allow_only" => ReadMode::AllowOnly,
                v => {
                    return Err(PolicyError::BadValue {
                        field: "read.mode",
                        value: v.to_string(),
                    })
                }
            },
            deny: str_list(read, "deny", "fs.read")?,
            allow_within_deny: str_list(read, "allow_within_deny", "fs.read")?,
            allow: str_list(read, "allow", "fs.read")?,
        };
        let write = req(j, "write", "fs")?;
        check_keys(write, "fs.write", &["allow", "deny_within_allow"])?;
        let allow = arr_at(write, "allow", "fs.write")?
            .iter()
            .enumerate()
            .map(|(i, w)| {
                check_keys(
                    w,
                    "fs.write.allow[]",
                    &["root", "read_only_subpaths", "protected_metadata_names"],
                )?;
                Ok(WritableRoot {
                    root: s_at(w, "root", "fs.write.allow[]")?,
                    read_only_subpaths: str_list(w, "read_only_subpaths", "fs.write.allow[]")?,
                    protected_metadata_names: str_list(
                        w,
                        "protected_metadata_names",
                        "fs.write.allow[]",
                    )?,
                })
                .map_err(|e: PolicyError| PolicyError::BadMember {
                    path: format!("fs.write.allow[{i}] ({e})"),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let exec = match req(j, "exec", "fs")?.get("mode").and_then(Json::as_str) {
            Some("any") => ExecPolicy::Any,
            Some("allow") => {
                ExecPolicy::Allow(str_list(req(j, "exec", "fs")?, "allow", "fs.exec")?)
            }
            _ => return Err(bad("fs.exec.mode")),
        };
        let mounts = arr_at(j, "mounts", "fs")?
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let src = req(m, "source", "fs.mounts[]")?;
                let source = match s_at(src, "kind", "fs.mounts[].source")?.as_str() {
                    "content" => MountSource::Content(s_at(src, "value", "fs.mounts[].source")?),
                    "host_path" => MountSource::HostPath(s_at(src, "value", "fs.mounts[].source")?),
                    v => {
                        return Err(PolicyError::BadValue {
                            field: "mount.source.kind",
                            value: v.to_string(),
                        })
                    }
                };
                let mode = match req(m, "mode", "fs.mounts[]")? {
                    Json::Str(s) if s == "ro" => MountMode::Ro,
                    Json::Str(s) if s == "rw" => MountMode::Rw,
                    Json::Obj(o) => match o.get("masked").and_then(Json::as_str) {
                        Some(sent) => MountMode::Masked(sent.to_string()),
                        None => return Err(bad("fs.mounts[].mode")),
                    },
                    _ => return Err(bad("fs.mounts[].mode")),
                };
                let _ = i;
                Ok(Mount {
                    source,
                    target: s_at(m, "target", "fs.mounts[]")?,
                    mode,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(FsPolicy {
            read,
            write: WritePolicy {
                allow,
                deny_within_allow: str_list(write, "deny_within_allow", "fs.write")?,
            },
            exec,
            protected_metadata: str_list(j, "protected_metadata", "fs")?
                .into_iter()
                .collect(),
            mounts,
        })
    }

    fn net_from(j: &Json) -> Result<NetPolicy, PolicyError> {
        check_keys(
            j,
            "net",
            &[
                "mode",
                "default_unmatched",
                "rules",
                "non_public_destinations",
                "local_binding",
                "unix_sockets",
                "dns",
                "tls",
                "upstream_proxy",
                "methods_default",
            ],
        )?;
        let rules = arr_at(j, "rules", "net")?
            .iter()
            .enumerate()
            .map(|(i, r)| rule_from(r, i))
            .collect::<Result<Vec<_>, _>>()?;
        let ux = req(j, "unix_sockets", "net")?;
        let dns = req(j, "dns", "net")?;
        let resolver = match req(dns, "resolver", "net.dns")?
            .get("kind")
            .and_then(Json::as_str)
        {
            Some("mediator") => DnsResolver::Mediator,
            Some("listed_nameservers") => DnsResolver::ListedNameservers(str_list(
                req(dns, "resolver", "net.dns")?,
                "nameservers",
                "net.dns.resolver",
            )?),
            Some("none") => DnsResolver::None,
            _ => return Err(bad("net.dns.resolver.kind")),
        };
        let tls = req(j, "tls", "net")?;
        Ok(NetPolicy {
            mode: match s_at(j, "mode", "net")?.as_str() {
                "none" => NetMode::None,
                "mediated" => NetMode::Mediated,
                "public" => NetMode::Public,
                v => {
                    return Err(PolicyError::BadValue {
                        field: "net.mode",
                        value: v.to_string(),
                    })
                }
            },
            default_unmatched: match s_at(j, "default_unmatched", "net")?.as_str() {
                "deny" => DefaultUnmatched::Deny,
                "ask" => DefaultUnmatched::Ask,
                v => {
                    return Err(PolicyError::BadValue {
                        field: "default_unmatched",
                        value: v.to_string(),
                    })
                }
            },
            rules,
            non_public_destinations: match s_at(j, "non_public_destinations", "net")?.as_str() {
                "deny" => NonPublic::Deny,
                "allow_listed" => NonPublic::AllowListed,
                v => {
                    return Err(PolicyError::BadValue {
                        field: "non_public_destinations",
                        value: v.to_string(),
                    })
                }
            },
            local_binding: bool_at(j, "local_binding", "net")?,
            unix_sockets: UnixSockets {
                mode: match s_at(ux, "mode", "net.unix_sockets")?.as_str() {
                    "bridged_only" => UnixSocketMode::BridgedOnly,
                    "allow_listed" => UnixSocketMode::AllowListed,
                    v => {
                        return Err(PolicyError::BadValue {
                            field: "unix_sockets.mode",
                            value: v.to_string(),
                        })
                    }
                },
                allow: str_list(ux, "allow", "net.unix_sockets")?,
            },
            dns: DnsPolicy { resolver },
            tls: TlsPolicy {
                terminate: bool_at(tls, "terminate", "net.tls")?,
                inspect_hooks: str_list(tls, "inspect_hooks", "net.tls")?,
            },
            upstream_proxy: match j.get("upstream_proxy") {
                Some(Json::Str(s)) => Some(s.clone()),
                Some(Json::Null) | None => None,
                Some(_) => return Err(bad("net.upstream_proxy")),
            },
            methods_default: str_list(j, "methods_default", "net")?,
        })
    }

    fn rule_from(j: &Json, i: usize) -> Result<EgressRule, PolicyError> {
        check_keys(
            j,
            "net.rules[]",
            &[
                "host",
                "ports",
                "protocols",
                "methods",
                "decision",
                "credential_bindings",
                "justification",
                "provenance",
            ],
        )
        .map_err(|e| PolicyError::BadMember {
            path: format!("net.rules[{i}] ({e})"),
        })?;
        let ports = arr_at(j, "ports", "net.rules[]")?
            .iter()
            .map(|p| match p {
                Json::Int(v) if *v >= 0 && *v <= 65535 => Ok(*v as u16),
                _ => Err(bad("net.rules[].ports")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let protocols = arr_at(j, "protocols", "net.rules[]")?
            .iter()
            .map(|p| {
                p.as_str()
                    .ok_or_else(|| bad("net.rules[].protocols"))
                    .and_then(EgressProtocol::parse)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let justification = match j.get("justification") {
            Some(Json::Null) | None => None,
            Some(t) => Some(
                Text::from_json(t, "net.rules[].justification")
                    .map_err(|_| bad("net.rules[].justification"))?,
            ),
        };
        let provenance = match j.get("provenance") {
            Some(Json::Null) | None => None,
            Some(p) => {
                Some(ProvenanceRecord::from_json(p).map_err(|_| bad("net.rules[].provenance"))?)
            }
        };
        Ok(EgressRule {
            host: HostPattern::parse(
                j.get("host")
                    .and_then(Json::as_str)
                    .ok_or_else(|| bad("net.rules[].host"))?,
            )?,
            ports,
            protocols,
            methods: str_list(j, "methods", "net.rules[]")?,
            decision: match s_at(j, "decision", "net.rules[]")?.as_str() {
                "allow" => RuleDecision::Allow,
                "deny" => RuleDecision::Deny,
                v => {
                    return Err(PolicyError::BadValue {
                        field: "rule.decision",
                        value: v.to_string(),
                    })
                }
            },
            credential_bindings: str_list(j, "credential_bindings", "net.rules[]")?,
            justification,
            provenance,
        })
    }

    fn proc_from(j: &Json) -> Result<ProcPolicy, PolicyError> {
        check_keys(
            j,
            "proc",
            &[
                "isolation_class",
                "unshare",
                "no_new_privs",
                "syscall_filter",
                "ptrace",
                "setuid",
                "max_processes",
                "env",
                "die_with_parent",
            ],
        )?;
        let unshare = arr_at(j, "unshare", "proc")?
            .iter()
            .map(|u| {
                u.as_str()
                    .ok_or_else(|| bad("proc.unshare"))
                    .and_then(UnshareKind::parse)
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        let env = req(j, "env", "proc")?;
        check_keys(
            env,
            "proc.env",
            &["inherit", "exclude", "set", "include_only"],
        )?;
        let set = match env.get("set") {
            Some(Json::Obj(m)) => m
                .iter()
                .map(|(k, v)| {
                    v.as_str()
                        .map(|s| (k.clone(), s.to_string()))
                        .ok_or_else(|| bad("proc.env.set"))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?,
            Some(_) => return Err(bad("proc.env.set")),
            None => BTreeMap::new(),
        };
        let gate = |k: &str| -> Result<GatePolicy, PolicyError> {
            match s_at(j, k, "proc")?.as_str() {
                "deny" => Ok(GatePolicy::Deny),
                v => Err(PolicyError::BadValue {
                    field: "proc.ptrace/setuid",
                    value: v.to_string(),
                }),
            }
        };
        Ok(ProcPolicy {
            isolation_class: IsolationClass::parse(&s_at(j, "isolation_class", "proc")?)?,
            unshare,
            no_new_privs: bool_at(j, "no_new_privs", "proc")?,
            syscall_filter: match j.get("syscall_filter") {
                Some(Json::Str(s)) => Some(s.clone()),
                Some(Json::Null) | None => None,
                Some(_) => return Err(bad("proc.syscall_filter")),
            },
            ptrace: gate("ptrace")?,
            setuid: gate("setuid")?,
            max_processes: u64_at(j, "max_processes", "proc")?,
            env: EnvPolicy {
                inherit: match s_at(env, "inherit", "proc.env")?.as_str() {
                    "all" => EnvInherit::All,
                    "core" => EnvInherit::Core,
                    "none" => EnvInherit::None,
                    v => {
                        return Err(PolicyError::BadValue {
                            field: "env.inherit",
                            value: v.to_string(),
                        })
                    }
                },
                exclude: str_list(env, "exclude", "proc.env")?,
                set,
                include_only: str_list(env, "include_only", "proc.env")?,
            },
            die_with_parent: bool_at(j, "die_with_parent", "proc")?,
        })
    }

    fn res_from(j: &Json) -> Result<ResourceLimits, PolicyError> {
        check_keys(
            j,
            "resources",
            &[
                "cpu_ms",
                "memory_bytes",
                "disk_bytes",
                "pids",
                "open_files",
                "wall_ms",
                "network_bytes_out",
                "network_calls",
            ],
        )?;
        Ok(ResourceLimits {
            cpu_ms: u64_at(j, "cpu_ms", "resources")?,
            memory_bytes: u64_at(j, "memory_bytes", "resources")?,
            disk_bytes: u64_at(j, "disk_bytes", "resources")?,
            pids: u64_at(j, "pids", "resources")?,
            open_files: u64_at(j, "open_files", "resources")?,
            wall_ms: u64_at(j, "wall_ms", "resources")?,
            network_bytes_out: u64_at(j, "network_bytes_out", "resources")?,
            network_calls: u64_at(j, "network_calls", "resources")?,
        })
    }

    fn amend_from(j: &Json) -> Result<AmendmentPolicy, PolicyError> {
        check_keys(
            j,
            "amendment",
            &[
                "allowed_bases",
                "strict",
                "session_cache",
                "persist_scope_ceiling",
            ],
        )?;
        let bases = arr_at(j, "allowed_bases", "amendment")?
            .iter()
            .map(|b| {
                b.as_str()
                    .ok_or_else(|| bad("amendment.allowed_bases"))
                    .and_then(AmendmentBasis::parse)
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        let ceiling = match s_at(j, "persist_scope_ceiling", "amendment")?.as_str() {
            "definition" => PersistenceScope::Definition,
            "user" => PersistenceScope::User,
            "project" => PersistenceScope::Project,
            "session" => PersistenceScope::Session,
            "run" => PersistenceScope::Run,
            "turn" => PersistenceScope::Turn,
            v => {
                return Err(PolicyError::BadValue {
                    field: "persist_scope_ceiling",
                    value: v.to_string(),
                })
            }
        };
        Ok(AmendmentPolicy {
            allowed_bases: bases,
            strict: bool_at(j, "strict", "amendment")?,
            session_cache: bool_at(j, "session_cache", "amendment")?,
            persist_scope_ceiling: ceiling,
        })
    }

    fn residual_from(j: &Json, i: usize) -> Result<ResidualChannel, PolicyError> {
        check_keys(
            j,
            "residual_channels[]",
            &["kind", "statement", "closer", "owner"],
        )
        .map_err(|e| PolicyError::BadMember {
            path: format!("residual_channels[{i}] ({e})"),
        })?;
        Ok(ResidualChannel {
            kind: ResidualKind::parse(&s_at(j, "kind", "residual_channels[]")?)?,
            statement: Text::from_json(
                req(j, "statement", "residual_channels[]")?,
                "residual_channels[].statement",
            )
            .map_err(|_| bad("residual_channels[].statement"))?,
            closer: s_at(j, "closer", "residual_channels[]")?,
            owner: s_at(j, "owner", "residual_channels[]")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_provenance::{HumanRole, Origin};

    #[test]
    fn kernel_default_is_deny_by_default_and_valid() {
        let p = kernel_default(0);
        assert!(p.validate().is_ok());
        assert_eq!(p.fs.read.mode, ReadMode::AllowAllExcept);
        assert!(p.fs.write.allow.is_empty());
        assert_eq!(p.net.mode, NetMode::None);
        assert!(p.proc.no_new_privs);
        assert!(p.proc.die_with_parent);
        assert!(p.amendment.strict);
        assert!(p.policy_id.starts_with("sha256:"));
        assert!(p.version_id.starts_with("sha256:"));
        assert_ne!(p.policy_id, p.version_id);
    }

    #[test]
    fn kernel_protected_is_mandatory() {
        let mut p = kernel_default(0);
        p.fs.protected_metadata.remove(".git/hooks");
        assert!(matches!(
            p.validate(),
            Err(PolicyError::ProtectedPathExemption { .. })
        ));
    }

    #[test]
    fn codec_round_trips_and_is_strict() {
        let p = kernel_default(7);
        let j = p.to_json();
        let back = ContainmentPolicy::from_json(&j).unwrap();
        assert_eq!(back, p);
        // An unknown member is refused, never dropped (CC4).
        let mut bad = j.clone();
        if let Json::Obj(ref mut m) = bad {
            m.insert("surprise".to_string(), Json::Null);
        }
        assert!(matches!(
            ContainmentPolicy::from_json(&bad),
            Err(PolicyError::BadMember { .. })
        ));
    }

    #[test]
    fn semantic_id_ignores_provenance_ext_and_justification() {
        let mut a = kernel_default(1);
        a.net.mode = NetMode::Mediated;
        a.net.rules.push(EgressRule {
            host: HostPattern::Exact("example.com".into()),
            ports: vec![443],
            protocols: vec![EgressProtocol::HttpsConnect],
            methods: vec![],
            decision: RuleDecision::Allow,
            credential_bindings: vec![],
            justification: Some(Text::new(
                "the egress is needed",
                "test:author",
                ProvenanceRecord::minted(
                    Origin::human("test:author", HumanRole::Principal),
                    PersistenceScope::Run,
                    1,
                ),
            )),
            provenance: None,
        });
        a.compute_ids();
        let mut b = a.clone();
        b.provenance.created_at = 99;
        b.ext.insert("x/note".into(), Json::str("y"));
        let rule = &mut b.net.rules[0];
        rule.justification = Some(Text::new(
            "a different wording",
            "test:author",
            ProvenanceRecord::minted(
                Origin::human("test:author", HumanRole::Principal),
                PersistenceScope::Run,
                2,
            ),
        ));
        b.compute_ids();
        assert_eq!(a.policy_id, b.policy_id);
        assert_ne!(a.version_id, b.version_id);
    }

    #[test]
    fn normalize_host_forms() {
        assert_eq!(normalize_host("Example.COM."), "example.com");
        assert_eq!(normalize_host("[::1%eth0]:443"), "::1%eth0");
        assert_eq!(normalize_host("host:8080"), "host");
        assert_eq!(
            HostPattern::parse("*.Example.com").unwrap(),
            HostPattern::Subdomains("example.com".into())
        );
        assert!(HostPattern::Subdomains("example.com".into()).matches("a.example.com"));
        assert!(!HostPattern::Subdomains("example.com".into()).matches("example.com"));
        assert!(HostPattern::ApexAndSubdomains("example.com".into()).matches("example.com"));
    }

    #[test]
    fn validation_invariants() {
        let mut p = kernel_default(0);
        p.net.rules.push(EgressRule {
            host: HostPattern::Global,
            ports: vec![],
            protocols: vec![],
            methods: vec![],
            decision: RuleDecision::Deny,
            credential_bindings: vec![],
            justification: None,
            provenance: None,
        });
        p.net.mode = NetMode::Mediated;
        assert!(matches!(p.validate(), Err(PolicyError::GlobalDenyRule)));

        let mut p = kernel_default(0);
        p.net.mode = NetMode::Public;
        // authority is kernel (≥ principal) but no residual declared.
        assert!(matches!(
            p.validate(),
            Err(PolicyError::PublicWithoutResidual)
        ));

        let mut p = kernel_default(0);
        p.net.mode = NetMode::None;
        p.net.rules.push(EgressRule {
            host: HostPattern::Exact("x".into()),
            ports: vec![],
            protocols: vec![],
            methods: vec![],
            decision: RuleDecision::Allow,
            credential_bindings: vec![],
            justification: None,
            provenance: None,
        });
        assert!(matches!(p.validate(), Err(PolicyError::ModeNoneHasRules)));
    }
}
