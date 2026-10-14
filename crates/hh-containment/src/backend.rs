//! The `ContainmentBackend` trait and **`Ep2Model`** — the Stage-1
//! in-process *model* of the EP2 process-sandbox boundary (§5g.4 §2.2;
//! ADR-0062 D1/D2).
//!
//! A backend declares what it enforces ([`BackendCaps`]), refuses a policy
//! it cannot host ([`UnsupportedBackend`]), lists its relied-on losses
//! (`lowering_loss`), and answers the syscall gate:
//! [`ContainmentBackend::gate`] is the *model* of "does this syscall
//! survive the boundary under this policy". `GateVerdict::Unenforced` is the
//! honest answer when the backend lacks the cap — never `Allow`.
//!
//! The reference [`Ep2Model`] enforces the Stage-1 surface: the fs read/
//! write/exec gates (symlink resolution *before* validation — F1, via the
//! model's declared `links`), the socket gate (`none`/`mediated` deny every
//! connect-class syscall — under `mediated` the only route is the bridged
//! `AF_UNIX` channel), the privilege gate (`io_uring`, `ptrace`,
//! `process_vm_*`, `setuid` denied), and the unix-socket/local-binding
//! rules. It does **not** enforce `resources.*` (the `BudgetNode` mapping is
//! Stage 2), `tls.terminate`/`inspect_hooks` (C2), `upstream_proxy` (proxy
//! env vars never count as `mediated` — a `lowering_loss`), or a
//! `syscall_filter` ref (pointer-rule resolution is the executor's) — each
//! lands a [`LoweringLoss`] when the policy relies on it.

use std::collections::BTreeMap;

use crate::admit::{classify_read, classify_write, ReadClass, WriteClass};
use crate::paths;
use crate::policy::{
    ContainmentPolicy, EnvInherit, EnvPolicy, ExecPolicy, IsolationClass, NetMode, UnixSocketMode,
};
use crate::report::{FieldGroup, LossConsequence, LossKind, LoweringLoss};

/// The syscall sum the gate models (the AC-H4-03 surface plus the fs/exec
/// gates). Deliberately small and closed — this is the Stage-1 *model* of
/// the boundary, not a syscall audit.
#[derive(Debug, Clone, PartialEq)]
pub enum Syscall {
    /// Open-for-write / create / rename at a path.
    FsWrite {
        /// The spelled path (the model resolves links before validating).
        path: String,
    },
    /// Open-for-read.
    FsRead {
        /// The path.
        path: String,
    },
    /// `execve` at a path.
    Exec {
        /// The path.
        path: String,
    },
    /// `connect` to an address (AF_INET-class).
    Connect {
        /// The target spelling.
        target: String,
    },
    /// `sendto` (unconnected egress).
    SendTo {
        /// The target spelling.
        target: String,
    },
    /// A raw socket.
    RawSocket,
    /// Name resolution (`getaddrinfo`-class).
    NameResolve {
        /// The host spelling.
        host: String,
    },
    /// ICMP.
    Icmp,
    /// `io_uring` setup.
    IoUring,
    /// `ptrace`.
    Ptrace,
    /// `process_vm_readv/writev`.
    ProcessVm,
    /// `setuid`.
    Setuid,
    /// `AF_UNIX` connect to a socket path.
    UnixConnect {
        /// The socket path.
        path: String,
    },
    /// A local listener `bind`.
    Bind {
        /// The bind address spelling.
        addr: String,
    },
}

/// `kind ∈ {fs_read, fs_write, exec, net, unix_socket, syscall, privilege}`
/// — the `security.containment.violated` member (§5g.4 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationKind {
    /// A denied read.
    FsRead,
    /// A denied write.
    FsWrite,
    /// A denied exec.
    Exec,
    /// A denied network operation.
    Net,
    /// A denied unix-socket use.
    UnixSocket,
    /// A denied syscall (e.g. `io_uring`).
    Syscall,
    /// A denied privilege operation (`ptrace`, `setuid`, `process_vm_*`).
    Privilege,
}

impl ViolationKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ViolationKind::FsRead => "fs_read",
            ViolationKind::FsWrite => "fs_write",
            ViolationKind::Exec => "exec",
            ViolationKind::Net => "net",
            ViolationKind::UnixSocket => "unix_socket",
            ViolationKind::Syscall => "syscall",
            ViolationKind::Privilege => "privilege",
        }
    }
}

/// The gate's verdict.
#[derive(Debug, Clone, PartialEq)]
pub enum GateVerdict {
    /// The syscall is permitted under the policy.
    Allow,
    /// The syscall is denied — `kind`/`subject` are the `violated` payload's
    /// members.
    Deny {
        /// The violation class.
        kind: ViolationKind,
        /// The denied subject (a path/host spelling — bounded, content-free).
        subject: String,
    },
    /// The backend does not enforce this class — honest *no evidence*,
    /// never `Allow` (a probe observing `Unenforced` fails the group).
    Unenforced,
}

/// What a backend enforces — the `lowering_loss`/evidence computation's
/// input. `true` = the backend enforces the surface; `false` = a policy
/// relying on the field lands a [`LoweringLoss`] (and the owning field group's
/// evidence degrades to `unknown` — never coerced).
#[derive(Debug, Clone, PartialEq)]
pub struct BackendCaps {
    /// The fs read/write gates.
    pub enforce_fs: bool,
    /// The socket gate (`connect`/`sendto`/raw/ICMP/`getaddrinfo` denial
    /// under `none`/`mediated`; the bridged channel).
    pub enforce_net: bool,
    /// The privilege gate (`io_uring`/`ptrace`/`process_vm_*`/`setuid`).
    pub enforce_proc: bool,
    /// The `exec.allow` gate.
    pub enforce_exec: bool,
    /// Mount honouring (`ro`/`rw`/`masked`).
    pub enforce_mounts: bool,
    /// `unix_sockets.allow` grants.
    pub enforce_unix: bool,
    /// `resources.*` accounting (Stage 2 — the `BudgetNode` mapping).
    pub enforce_resources: bool,
    /// `syscall_filter` ref interpretation (pointer-rule resolution).
    pub enforce_syscall_filter: bool,
    /// `tls.terminate`/`inspect_hooks` (C2).
    pub enforce_tls: bool,
    /// `upstream_proxy` honouring (never counts as `mediated`).
    pub enforce_proxy: bool,
    /// `local_binding` gate.
    pub enforce_local_binding: bool,
}

impl BackendCaps {
    /// Every cap on (a fully-enforcing backend — the C1 ideal).
    pub fn full() -> BackendCaps {
        BackendCaps {
            enforce_fs: true,
            enforce_net: true,
            enforce_proc: true,
            enforce_exec: true,
            enforce_mounts: true,
            enforce_unix: true,
            enforce_resources: true,
            enforce_syscall_filter: true,
            enforce_tls: true,
            enforce_proxy: true,
            enforce_local_binding: true,
        }
    }
}

/// `apply` failed — the backend cannot host this policy at all.
#[derive(Debug, Clone, PartialEq)]
pub struct UnsupportedBackend {
    /// The backend.
    pub backend: String,
    /// Why (a closed-ish tag for audit).
    pub reason: String,
}

/// The net enforcement plane a backend realizes (§5g.4 C1/Stage-4 row —
/// the transparent-redirect backend class). `BridgedChannel` is the EP2
/// model (the namespace is gone; the one `AF_UNIX` channel reaches the
/// mediator); `TransparentRedirect` intercepts connect-class syscalls at
/// L4 into the mediator — no bridged channel exists to probe, and a
/// rule-allowed destination completes *through* the redirect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetPlane {
    /// The EP2 model — removed namespace + one bridged helper channel.
    BridgedChannel,
    /// L4 transparent redirection into the mediator.
    TransparentRedirect,
}

impl NetPlane {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            NetPlane::BridgedChannel => "bridged_channel",
            NetPlane::TransparentRedirect => "transparent_redirect",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<NetPlane> {
        match s {
            "bridged_channel" => Some(NetPlane::BridgedChannel),
            "transparent_redirect" => Some(NetPlane::TransparentRedirect),
            _ => None,
        }
    }
}

/// A substrate attestation binding (§5g.4 C1/Stage-4 — the `attested`
/// evidence kind's only source): `user_space_kernel`/`microvm` backends
/// carry the guest measurement/sealing certificate ref the report's
/// `attested` verdict leans on. A backend that cannot produce one never
/// reports `attested` — the `attach` fold degrades to `probed`/`reported`
/// instead (never coerced upward — T-LCD-07).
#[derive(Debug, Clone, PartialEq)]
pub struct Attestation {
    /// The attestation method (`guest_quote`, `sev_snp_report`, …).
    pub method: String,
    /// The attestation record ref (content address).
    pub attestation_ref: String,
}

/// The backend contract (ADR-0062 D2): apply the policy, run the probe
/// battery, declare losses. `apply` + `gate` + `lowering_loss` are pure —
/// real backends wrap the out-of-process helper (§05a B5, S1.16).
pub trait ContainmentBackend {
    /// The backend's name (the report's `backend` member).
    fn name(&self) -> &'static str;
    /// The isolation class this backend implements.
    fn isolation_class(&self) -> IsolationClass;
    /// What it enforces.
    fn caps(&self) -> &BackendCaps;
    /// Apply the policy — fails when the backend cannot host it at all
    /// (isolation class above the backend's, or a structural surface it
    /// does not implement).
    fn apply(&self, policy: &ContainmentPolicy) -> Result<(), UnsupportedBackend>;
    /// The syscall gate — the model of the boundary under `policy`.
    fn gate(&self, sys: &Syscall, policy: &ContainmentPolicy) -> GateVerdict;
    /// The one bridged helper channel's socket path (the `AF_UNIX` probe's
    /// target — OQ-161's resolved ruling).
    fn bridged_channel(&self) -> &str;
    /// Run one probe (the fixture is backend-owned — a real EP2 fabricates
    /// the file/link/socket; the model gates the constructed syscall). The
    /// default constructs [`crate::probes::probe_syscall`] and gates it.
    fn run_probe(&self, kind: crate::report::ProbeKind, policy: &ContainmentPolicy) -> GateVerdict {
        let sys = crate::probes::probe_syscall(kind, policy, self.bridged_channel());
        self.gate(&sys, policy)
    }
    /// The relied-on losses — a [`LoweringLoss`] for every field the policy
    /// configures that the backend cannot enforce.
    fn lowering_loss(&self, policy: &ContainmentPolicy) -> Vec<LoweringLoss>;
    /// The net enforcement plane (default [`NetPlane::BridgedChannel`] —
    /// the EP2 model). The battery is plane-aware: a
    /// `transparent_redirect` backend holds no bridged `AF_UNIX` channel,
    /// so the bridged-socket probe is inapplicable (skipped, never failed).
    fn net_plane(&self) -> NetPlane {
        NetPlane::BridgedChannel
    }
    /// The substrate attestation covering `group` — `Some` only on the C1
    /// attesting backends (`user_space_kernel`, `microvm`) for groups the
    /// substrate's measurement actually covers. Default `None`: no
    /// backend silently claims `attested`.
    fn attestation(&self, _group: FieldGroup) -> Option<Attestation> {
        None
    }
    /// Whether the boundary is a *hosted* (participant-supplied) one —
    /// `isolation_class = external`. Kernel probes cannot observe inside
    /// it: the battery is skipped and declared caps report `reported`,
    /// never `probed`/`attested` (AC-R-2.8.4-13).
    fn hosted_external(&self) -> bool {
        false
    }
}

/// The Stage-1 reference backend — an in-process **model** of the EP2
/// process sandbox. `links` declares the modelled symlinks
/// (`link → target`, resolved transitively *before* validation — F1).
/// `bridged_socket` is the one helper channel (OQ-161's resolved ruling).
#[derive(Debug, Clone)]
pub struct Ep2Model {
    caps: BackendCaps,
    /// Modelled symlinks — `link path → target`.
    links: BTreeMap<String, String>,
    /// The bridged helper channel's socket path.
    bridged_socket: String,
    /// The strongest `isolation_class` this model hosts (the delegate
    /// models raise it; the reference is `process_sandbox`).
    max_class: IsolationClass,
}

impl Ep2Model {
    /// The reference configuration — the Stage-1 honest caps: fs/net/proc/
    /// exec/mounts/unix/local-binding enforced; `resources` (Stage-2 budget
    /// mapping), `syscall_filter` (pointer-rule ref), `tls` (C2) and
    /// `upstream_proxy` (never mediation) unenforced.
    pub fn reference() -> Ep2Model {
        Ep2Model {
            caps: BackendCaps {
                enforce_fs: true,
                enforce_net: true,
                enforce_proc: true,
                enforce_exec: true,
                enforce_mounts: true,
                enforce_unix: true,
                enforce_resources: false,
                enforce_syscall_filter: false,
                enforce_tls: false,
                enforce_proxy: false,
                enforce_local_binding: true,
            },
            links: BTreeMap::new(),
            bridged_socket: "hh-helper.sock".to_string(),
            max_class: IsolationClass::ProcessSandbox,
        }
    }

    /// Test/build hook: override caps (a degraded backend model).
    pub fn with_caps(mut self, caps: BackendCaps) -> Ep2Model {
        self.caps = caps;
        self
    }

    /// Declare a modelled symlink (the F1 probe's fixture).
    pub fn with_link(mut self, link: &str, target: &str) -> Ep2Model {
        self.links.insert(link.to_string(), target.to_string());
        self
    }

    /// The bridged channel spelling.
    pub fn bridged_socket(&self) -> &str {
        &self.bridged_socket
    }

    /// Resolve the modelled links transitively (F1 — resolution precedes
    /// validation). The longest declared prefix that is a link rewrites to
    /// its target; a cycle resolves to a sentinel that matches no root, so
    /// the checks deny it.
    pub fn resolve(&self, path: &str) -> String {
        let mut cur = paths::normalize_path(path);
        let mut seen = std::collections::BTreeSet::new();
        loop {
            let hit = self
                .links
                .iter()
                .filter(|(l, _)| paths::within(l, &cur))
                .max_by_key(|(l, _)| l.len())
                .map(|(l, t)| (l.clone(), t.clone()));
            let Some((link, target)) = hit else {
                return cur;
            };
            let rest = cur.strip_prefix(&link).unwrap_or("").to_string();
            let next = paths::normalize_path(&format!("{target}{rest}"));
            if !seen.insert(cur.clone()) {
                return format!("__link_cycle__/{cur}");
            }
            cur = next;
        }
    }
}

impl ContainmentBackend for Ep2Model {
    fn name(&self) -> &'static str {
        "ep2_model"
    }

    fn isolation_class(&self) -> IsolationClass {
        IsolationClass::ProcessSandbox
    }

    fn caps(&self) -> &BackendCaps {
        &self.caps
    }

    fn bridged_channel(&self) -> &str {
        &self.bridged_socket
    }

    fn run_probe(&self, kind: crate::report::ProbeKind, policy: &ContainmentPolicy) -> GateVerdict {
        if kind == crate::report::ProbeKind::WriteSymlinkEscape {
            // The F1 fixture: model a symlink inside the first writable root
            // whose target escapes it, then gate a write through it.
            let root = policy
                .fs
                .write
                .allow
                .first()
                .map(|w| w.root.clone())
                .unwrap_or_else(|| "workspace".to_string());
            let link = format!("{root}/hh.probe.link");
            let mut m = self.clone();
            m.links.insert(link.clone(), "escape/target".to_string());
            return m.gate(
                &Syscall::FsWrite {
                    path: format!("{link}/x"),
                },
                policy,
            );
        }
        let sys = crate::probes::probe_syscall(kind, policy, self.bridged_channel());
        self.gate(&sys, policy)
    }

    fn apply(&self, policy: &ContainmentPolicy) -> Result<(), UnsupportedBackend> {
        let fail = |reason: &str| UnsupportedBackend {
            backend: self.name().to_string(),
            reason: reason.to_string(),
        };
        // The model is a process-sandbox boundary (or the ceiling the
        // delegate declares); a stronger class or the hosted `external`
        // claim cannot be applied here.
        if policy.proc.isolation_class.strength() > self.max_class.strength() {
            return Err(fail("isolation_class_above_backend"));
        }
        if policy.proc.isolation_class == IsolationClass::External {
            return Err(fail("isolation_class_external"));
        }
        if !policy.fs.mounts.is_empty() && !self.caps.enforce_mounts {
            return Err(fail("mounts_unsupported"));
        }
        if policy.net.unix_sockets.mode == UnixSocketMode::AllowListed && !self.caps.enforce_unix {
            return Err(fail("unix_allow_unsupported"));
        }
        if matches!(&policy.fs.exec, ExecPolicy::Allow(s) if !s.is_empty())
            && !self.caps.enforce_exec
        {
            return Err(fail("exec_allow_unsupported"));
        }
        if policy.net.mode == NetMode::Mediated && !self.caps.enforce_net {
            return Err(fail("mediated_unsupported"));
        }
        Ok(())
    }

    fn gate(&self, sys: &Syscall, policy: &ContainmentPolicy) -> GateVerdict {
        match sys {
            Syscall::FsWrite { path } => {
                if !self.caps.enforce_fs {
                    return GateVerdict::Unenforced;
                }
                // F1 — resolve links *before* validating.
                let resolved = self.resolve(path);
                match classify_write(policy, &resolved) {
                    WriteClass::Inside => GateVerdict::Allow,
                    _ => GateVerdict::Deny {
                        kind: ViolationKind::FsWrite,
                        subject: resolved,
                    },
                }
            }
            Syscall::FsRead { path } => {
                if !self.caps.enforce_fs {
                    return GateVerdict::Unenforced;
                }
                let resolved = self.resolve(path);
                match classify_read(policy, &resolved) {
                    ReadClass::Allowed => GateVerdict::Allow,
                    _ => GateVerdict::Deny {
                        kind: ViolationKind::FsRead,
                        subject: resolved,
                    },
                }
            }
            Syscall::Exec { path } => {
                if !self.caps.enforce_exec {
                    return GateVerdict::Unenforced;
                }
                let resolved = self.resolve(path);
                match &policy.fs.exec {
                    ExecPolicy::Any => GateVerdict::Allow,
                    ExecPolicy::Allow(set) => {
                        if set.iter().any(|a| {
                            paths::within(a, &resolved) || paths::pattern_matches(a, &resolved)
                        }) {
                            GateVerdict::Allow
                        } else {
                            GateVerdict::Deny {
                                kind: ViolationKind::Exec,
                                subject: resolved,
                            }
                        }
                    }
                }
            }
            Syscall::Connect { target }
            | Syscall::SendTo { target }
            | Syscall::NameResolve { host: target } => {
                if !self.caps.enforce_net {
                    return GateVerdict::Unenforced;
                }
                match policy.net.mode {
                    // `none` and `mediated` remove the namespace — the only
                    // route out is the bridged channel, so every connect-
                    // class syscall fails at EP2 under both.
                    NetMode::None | NetMode::Mediated => GateVerdict::Deny {
                        kind: ViolationKind::Net,
                        subject: target.clone(),
                    },
                    NetMode::Public => GateVerdict::Allow,
                }
            }
            Syscall::RawSocket | Syscall::Icmp => {
                if !self.caps.enforce_net {
                    return GateVerdict::Unenforced;
                }
                match policy.net.mode {
                    NetMode::None | NetMode::Mediated => GateVerdict::Deny {
                        kind: ViolationKind::Net,
                        subject: sys_kind(sys).to_string(),
                    },
                    NetMode::Public => GateVerdict::Allow,
                }
            }
            Syscall::IoUring | Syscall::Ptrace | Syscall::ProcessVm | Syscall::Setuid => {
                if !self.caps.enforce_proc {
                    return GateVerdict::Unenforced;
                }
                GateVerdict::Deny {
                    kind: match sys {
                        Syscall::IoUring => ViolationKind::Syscall,
                        _ => ViolationKind::Privilege,
                    },
                    subject: sys_kind(sys).to_string(),
                }
            }
            Syscall::UnixConnect { path } => {
                if !self.caps.enforce_unix {
                    return GateVerdict::Unenforced;
                }
                let p = paths::normalize_path(path);
                if p == paths::normalize_path(&self.bridged_socket) {
                    return GateVerdict::Allow;
                }
                if policy.net.unix_sockets.mode == UnixSocketMode::AllowListed
                    && policy
                        .net
                        .unix_sockets
                        .allow
                        .iter()
                        .any(|a| paths::pattern_matches(a, &p))
                {
                    return GateVerdict::Allow;
                }
                GateVerdict::Deny {
                    kind: ViolationKind::UnixSocket,
                    subject: p,
                }
            }
            Syscall::Bind { addr } => {
                if !self.caps.enforce_local_binding {
                    return GateVerdict::Unenforced;
                }
                if policy.net.local_binding {
                    GateVerdict::Allow
                } else {
                    GateVerdict::Deny {
                        kind: ViolationKind::Net,
                        subject: addr.clone(),
                    }
                }
            }
        }
    }

    fn lowering_loss(&self, policy: &ContainmentPolicy) -> Vec<LoweringLoss> {
        let mut out = Vec::new();
        let mut loss = |field: &str, reason: &str, kind: LossKind, consequence: LossConsequence| {
            out.push(LoweringLoss {
                declared_field: field.to_string(),
                backend: self.name().to_string(),
                reason: reason.to_string(),
                kind,
                consequence,
            });
        };
        if !self.caps.enforce_resources {
            for (field, _) in policy.resources.set_fields() {
                loss(
                    field,
                    "resources_unenforced",
                    LossKind::NoSlot,
                    LossConsequence::FailClosed,
                );
            }
        }
        if policy.proc.syscall_filter.is_some() && !self.caps.enforce_syscall_filter {
            loss(
                "proc.syscall_filter",
                "filter_ref_unresolved",
                LossKind::NoSlot,
                LossConsequence::FailClosed,
            );
        }
        // The C2 TLS members are never an EP2 backend slot — the loss row
        // is recorded either way (`no_slot`: the member has no slot at this
        // enforcement point). Under `mediated` the **mediator** discharges
        // them at the wire point — `tls.terminate` enforces through the
        // terminating transport or the leg refuses typed
        // (`tls_terminate_unsupported`), and every declared `inspect_hook`
        // runs (or refuses `inspect_hook_unavailable`) before
        // `decided{allow}` mints (R2.9b; ADR-0341 D3) — so the row is
        // `stratified` (`mediator_discharges`: the member's enforcement
        // moved to EP3), never `fail_closed`. Under `none`/`public` no
        // mediator exists — the member is a real unenforced loss and is
        // `fail_closed` where the `net` group is relied on (`none`; under
        // `public` the group is not relied on — the row records but
        // does not refuse).
        if !self.caps.enforce_tls {
            let mediated = policy.net.mode == NetMode::Mediated;
            if policy.net.tls.terminate {
                loss(
                    "net.tls.terminate",
                    if mediated {
                        "mediator_discharges"
                    } else {
                        "tls_terminate_c2"
                    },
                    LossKind::NoSlot,
                    if mediated {
                        LossConsequence::Stratified
                    } else {
                        LossConsequence::FailClosed
                    },
                );
            }
            if !policy.net.tls.inspect_hooks.is_empty() {
                loss(
                    "net.tls.inspect_hooks",
                    if mediated {
                        "mediator_discharges"
                    } else {
                        "inspect_hooks_c2"
                    },
                    LossKind::NoSlot,
                    if mediated {
                        LossConsequence::Stratified
                    } else {
                        LossConsequence::FailClosed
                    },
                );
            }
        }
        // `upstream_proxy` (and the ambient/`env`-carried proxy-variable
        // surface under `mediated`) never *counts as* mediation: the loss
        // is recorded and the run stratifies on it (§5g.4 §5's proxy row —
        // `stratified`, not `fail_closed`; ADR-0341 D4).
        if policy.net.upstream_proxy.is_some() && !self.caps.enforce_proxy {
            loss(
                "net.upstream_proxy",
                "proxy_env_is_not_mediation",
                LossKind::HintOnly,
                LossConsequence::Stratified,
            );
        }
        if policy.net.mode == NetMode::Mediated && proxy_env_surface(&policy.proc.env) {
            loss(
                "proc.env",
                "proxy_env_is_not_mediation",
                LossKind::HintOnly,
                LossConsequence::Stratified,
            );
        }
        out
    }
}

/// The proxy-variable spellings — the env-carried "mediation" masquerade
/// (§5g.4 §5's "proxy environment variables mistaken for mediation" row).
/// Compared case-insensitively (`HTTP_PROXY`/`http_proxy` are the same
/// surface).
const PROXY_ENV_VARS: &[&str] = &[
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "ftp_proxy",
    "no_proxy",
];

/// Whether `env` admits a proxy variable — an explicit `set`, an
/// `include_only` membership, or an `inherit = all` ambient surface an
/// `exclude` does not cover (ADR-0341 D4). `include_only` **bounds**
/// inheritance — a non-empty list admits only the named unexcluded
/// variables, so `include_only = [PATH]` leaves no ambient proxy surface.
fn proxy_env_surface(env: &EnvPolicy) -> bool {
    let is_proxy = |name: &str| PROXY_ENV_VARS.iter().any(|v| v.eq_ignore_ascii_case(name));
    let excluded = |name: &str| env.exclude.iter().any(|e| e.eq_ignore_ascii_case(name));
    // `set` always injects — an excluded name is still set.
    if env.set.keys().any(|k| is_proxy(k)) {
        return true;
    }
    if !env.include_only.is_empty() {
        return env.include_only.iter().any(|n| is_proxy(n) && !excluded(n));
    }
    // `inherit = all` admits the ambient set unless every proxy name is
    // excluded.
    env.inherit == EnvInherit::All && PROXY_ENV_VARS.iter().any(|v| !excluded(v))
}

fn sys_kind(sys: &Syscall) -> &'static str {
    match sys {
        Syscall::FsWrite { .. } => "fs_write",
        Syscall::FsRead { .. } => "fs_read",
        Syscall::Exec { .. } => "exec",
        Syscall::Connect { .. } => "connect",
        Syscall::SendTo { .. } => "sendto",
        Syscall::RawSocket => "raw_socket",
        Syscall::NameResolve { .. } => "name_resolve",
        Syscall::Icmp => "icmp",
        Syscall::IoUring => "io_uring",
        Syscall::Ptrace => "ptrace",
        Syscall::ProcessVm => "process_vm",
        Syscall::Setuid => "setuid",
        Syscall::UnixConnect { .. } => "unix_connect",
        Syscall::Bind { .. } => "bind",
    }
}

/// The field group a syscall class belongs to — the evidence attribution.
pub fn syscall_group(sys: &Syscall) -> FieldGroup {
    match sys {
        Syscall::FsWrite { .. } | Syscall::FsRead { .. } | Syscall::Exec { .. } => FieldGroup::Fs,
        Syscall::Connect { .. }
        | Syscall::SendTo { .. }
        | Syscall::RawSocket
        | Syscall::NameResolve { .. }
        | Syscall::Icmp
        | Syscall::UnixConnect { .. }
        | Syscall::Bind { .. } => FieldGroup::Net,
        Syscall::IoUring | Syscall::Ptrace | Syscall::ProcessVm | Syscall::Setuid => {
            FieldGroup::Proc
        }
    }
}

// ── the C1 backend models (S4.14b; R-2.8.4¹) ──────────────────────────────

/// The backend class spellings — the `backend` member's closed set plus the
/// caller-facing selector [`for_class`]/[`for_policy`] consumes. `Ep2` is
/// the Stage-1 process-sandbox reference; the rest are the Stage-4 classes
/// (§5g.4 C1 row; ADR-0307).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendClass {
    /// `process_sandbox` — the EP2 model boundary.
    ProcessSandbox,
    /// `namespaces` — kernel namespaces.
    Namespaces,
    /// `user_space_kernel` — attested user-space kernel.
    UserSpaceKernel,
    /// `microvm` — attested microVM.
    Microvm,
    /// `transparent_redirect` — the transparent-redirect backend class
    /// (net plane [`NetPlane::TransparentRedirect`] over a namespaces
    /// boundary).
    TransparentRedirect,
    /// `external` — the hosted participant-supplied boundary (evidence is
    /// `reported`, never `probed`/`attested` — the kernel does not
    /// observe inside it).
    External,
}

impl BackendClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            BackendClass::ProcessSandbox => "process_sandbox",
            BackendClass::Namespaces => "namespaces",
            BackendClass::UserSpaceKernel => "user_space_kernel",
            BackendClass::Microvm => "microvm",
            BackendClass::TransparentRedirect => "transparent_redirect",
            BackendClass::External => "external",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<BackendClass> {
        Some(match s {
            "process_sandbox" | "ep2_model" => BackendClass::ProcessSandbox,
            "namespaces" => BackendClass::Namespaces,
            "user_space_kernel" => BackendClass::UserSpaceKernel,
            "microvm" => BackendClass::Microvm,
            "transparent_redirect" => BackendClass::TransparentRedirect,
            "external" => BackendClass::External,
            _ => return None,
        })
    }
}

/// `ModelBackend` — the Stage-4 backend models: the same syscall-gate
/// semantics as [`Ep2Model`] (the boundary contract is the policy's, not
/// the mechanism's), parameterised by the declared [`IsolationClass`], the
/// [`NetPlane`] and (for the attesting classes) the substrate
/// [`Attestation`]. `hosted` is the `external` boundary — the kernel's
/// probes cannot observe it, so `gate` honestly reports `Unenforced` and
/// the attach fold reports `reported`/`unknown`, never `probed`.
#[derive(Debug, Clone)]
pub struct ModelBackend {
    inner: Ep2Model,
    /// The declared isolation class this model implements.
    class: IsolationClass,
    /// The report's `backend` member.
    backend_name: &'static str,
    /// The net plane.
    plane: NetPlane,
    /// The substrate attestation (attesting classes only).
    attestation: Option<Attestation>,
    /// `true` for the hosted `external` boundary.
    hosted: bool,
}

impl ModelBackend {
    /// `namespaces` — kernel namespaces over the bridged-channel plane.
    pub fn namespaces() -> ModelBackend {
        Self::model(
            "namespaces_model",
            IsolationClass::Namespaces,
            NetPlane::BridgedChannel,
            None,
            false,
        )
    }

    /// `user_space_kernel` — attested (the guest-kernel measurement binds
    /// the boundary; `attested` evidence for the covered groups). The
    /// substrate enforces `resources.*` inside the guest — the cap the
    /// `reported` (never `attested`) resources group leans on.
    pub fn user_space_kernel(attestation: Attestation) -> ModelBackend {
        let mut m = Self::model(
            "user_space_kernel_model",
            IsolationClass::UserSpaceKernel,
            NetPlane::BridgedChannel,
            Some(attestation),
            false,
        );
        m.inner.caps.enforce_resources = true;
        m
    }

    /// `microvm` — attested (the microVM's measurement report binds the
    /// boundary; resource bounds land via the hypervisor's caps).
    pub fn microvm(attestation: Attestation) -> ModelBackend {
        let mut m = Self::model(
            "microvm_model",
            IsolationClass::Microvm,
            NetPlane::BridgedChannel,
            Some(attestation),
            false,
        );
        m.inner.caps.enforce_resources = true;
        m
    }

    /// `transparent_redirect` — the transparent-redirect backend class:
    /// a namespaces-strength boundary whose net plane is L4 redirection
    /// into the mediator. Under `mediated` a connect-class syscall to a
    /// rule-allowed destination completes *through* the redirect (the
    /// mediator enforces); every unmatched destination still denies.
    pub fn transparent_redirect() -> ModelBackend {
        Self::model(
            "transparent_redirect_model",
            IsolationClass::Namespaces,
            NetPlane::TransparentRedirect,
            None,
            false,
        )
    }

    /// `external` — the hosted participant-supplied boundary (AC-R-2.8.4-13):
    /// enforcement evidence is `reported` (the participant's own
    /// declaration), never `probed`/`attested`.
    pub fn external() -> ModelBackend {
        let mut m = Self::model(
            "external_model",
            IsolationClass::External,
            NetPlane::BridgedChannel,
            None,
            true,
        );
        // The hosted boundary enforces nothing the kernel can observe —
        // the caps are the participant's *declaration* (defaulted to the
        // full honest surface so a hosted policy relying on a group
        // reports `reported`, not `unknown`).
        m.inner.caps = BackendCaps::full();
        m
    }

    /// A hosted boundary carrying the participant's *declared* cap subset
    /// — groups not declared report `unknown` (never coerced).
    pub fn external_declaring(caps: BackendCaps) -> ModelBackend {
        let mut m = Self::external();
        m.inner.caps = caps;
        m
    }

    fn model(
        backend_name: &'static str,
        class: IsolationClass,
        plane: NetPlane,
        attestation: Option<Attestation>,
        hosted: bool,
    ) -> ModelBackend {
        let mut inner = Ep2Model::reference();
        inner.max_class = class;
        ModelBackend {
            inner,
            class,
            backend_name,
            plane,
            attestation,
            hosted,
        }
    }

    /// The caps the delegate gate consults.
    fn caps_ref(&self) -> &BackendCaps {
        self.inner.caps_ref()
    }

    /// The connect-class verdict under the transparent-redirect plane:
    /// `none` denies, `public` allows, `mediated` resolves the rule set
    /// (deny rules first — I-C1; an allow rule covering the normalised
    /// host admits through the redirect).
    fn redirect_gate(&self, target: &str, policy: &ContainmentPolicy) -> GateVerdict {
        if !self.caps_ref().enforce_net {
            return GateVerdict::Unenforced;
        }
        match policy.net.mode {
            NetMode::None => GateVerdict::Deny {
                kind: ViolationKind::Net,
                subject: target.to_string(),
            },
            NetMode::Public => GateVerdict::Allow,
            NetMode::Mediated => {
                let host = crate::policy::normalize_host(target);
                let mut allowed = false;
                for r in &policy.net.rules {
                    if r.host.matches(&host) {
                        match r.decision {
                            crate::policy::RuleDecision::Deny => {
                                return GateVerdict::Deny {
                                    kind: ViolationKind::Net,
                                    subject: target.to_string(),
                                };
                            }
                            crate::policy::RuleDecision::Allow => allowed = true,
                        }
                    }
                }
                if allowed {
                    GateVerdict::Allow
                } else {
                    GateVerdict::Deny {
                        kind: ViolationKind::Net,
                        subject: target.to_string(),
                    }
                }
            }
        }
    }
}

impl Ep2Model {
    /// The caps the gate consults (model delegates share this view).
    pub(crate) fn caps_ref(&self) -> &BackendCaps {
        &self.caps
    }
}

impl ContainmentBackend for ModelBackend {
    fn name(&self) -> &'static str {
        self.backend_name
    }

    fn isolation_class(&self) -> IsolationClass {
        self.class
    }

    fn caps(&self) -> &BackendCaps {
        self.caps_ref()
    }

    fn bridged_channel(&self) -> &str {
        self.inner.bridged_channel()
    }

    fn net_plane(&self) -> NetPlane {
        self.plane
    }

    fn hosted_external(&self) -> bool {
        self.hosted
    }

    fn attestation(&self, group: FieldGroup) -> Option<Attestation> {
        if !self.attesting_group(group) {
            return None;
        }
        self.attestation.clone()
    }

    fn apply(&self, policy: &ContainmentPolicy) -> Result<(), UnsupportedBackend> {
        if self.hosted {
            // The hosted boundary carries `isolation_class = external`
            // only — a stronger claim it cannot evidence is refused, never
            // silently downgraded.
            if policy.proc.isolation_class != IsolationClass::External {
                return Err(UnsupportedBackend {
                    backend: self.backend_name.to_string(),
                    reason: "isolation_class_not_external".to_string(),
                });
            }
            return Ok(());
        }
        if policy.proc.isolation_class == IsolationClass::External {
            return Err(UnsupportedBackend {
                backend: self.backend_name.to_string(),
                reason: "isolation_class_external".to_string(),
            });
        }
        if policy.proc.isolation_class.strength() > self.class.strength() {
            return Err(UnsupportedBackend {
                backend: self.backend_name.to_string(),
                reason: "isolation_class_above_backend".to_string(),
            });
        }
        if self.plane == NetPlane::TransparentRedirect
            && matches!(policy.net.mode, NetMode::Mediated | NetMode::None)
        {
            // The redirect plane hosts `mediated`/`none` natively — the
            // L4 intercept IS the enforcement. `public` needs no plane.
            return Ok(());
        }
        self.inner.apply(policy)
    }

    fn gate(&self, sys: &Syscall, policy: &ContainmentPolicy) -> GateVerdict {
        if self.hosted {
            // The kernel does not observe inside a participant-supplied
            // boundary — every class reports Unenforced, honestly.
            return GateVerdict::Unenforced;
        }
        if self.plane == NetPlane::TransparentRedirect {
            match sys {
                Syscall::Connect { target }
                | Syscall::SendTo { target }
                | Syscall::NameResolve { host: target } => {
                    return self.redirect_gate(target, policy);
                }
                // The redirect carries no raw/ICMP channels and no
                // bridged AF_UNIX socket.
                Syscall::RawSocket | Syscall::Icmp => {
                    if !self.caps_ref().enforce_net {
                        return GateVerdict::Unenforced;
                    }
                    return match policy.net.mode {
                        NetMode::Public => GateVerdict::Allow,
                        _ => GateVerdict::Deny {
                            kind: ViolationKind::Net,
                            subject: sys_kind(sys).to_string(),
                        },
                    };
                }
                Syscall::UnixConnect { .. } => {
                    if !self.caps_ref().enforce_net {
                        return GateVerdict::Unenforced;
                    }
                    return GateVerdict::Deny {
                        kind: ViolationKind::UnixSocket,
                        subject: sys_kind(sys).to_string(),
                    };
                }
                _ => {}
            }
        }
        self.inner.gate(sys, policy)
    }

    fn run_probe(&self, kind: crate::report::ProbeKind, policy: &ContainmentPolicy) -> GateVerdict {
        if self.hosted {
            return GateVerdict::Unenforced;
        }
        if self.plane == NetPlane::TransparentRedirect && kind.group() == FieldGroup::Net {
            // The net probes gate against the redirect plane (the bridged
            // channel probe is skipped by the battery — no channel exists).
            let sys = crate::probes::probe_syscall(kind, policy, self.bridged_channel());
            return self.gate(&sys, policy);
        }
        // fs/proc probes (including the backend-owned
        // `WriteSymlinkEscape` fixture) delegate unchanged.
        self.inner.run_probe(kind, policy)
    }

    fn lowering_loss(&self, policy: &ContainmentPolicy) -> Vec<LoweringLoss> {
        self.inner.lowering_loss(policy)
    }
}

impl ModelBackend {
    /// The groups the substrate attestation covers — fs/net/proc (the
    /// boundary surface); `resources` accounting is never attested (the
    /// substrate measures the boundary, not the counters).
    fn attesting_group(&self, group: FieldGroup) -> bool {
        self.attestation.is_some()
            && match group {
                FieldGroup::Fs => self.caps_ref().enforce_fs && self.caps_ref().enforce_exec,
                FieldGroup::Net => self.caps_ref().enforce_net,
                FieldGroup::Proc => self.caps_ref().enforce_proc,
                FieldGroup::Resources => false,
            }
    }
}

/// `for_class(class, attestation)` — select the backend model for a
/// declared class. `user_space_kernel`/`microvm` *require* a substrate
/// [`Attestation`]: a stronger-class claim without one is a fail-closed
/// `attestation_missing` refusal, never a silent degrade (I-C4).
pub fn for_class(
    class: BackendClass,
    attestation: Option<Attestation>,
) -> Result<Box<dyn ContainmentBackend>, UnsupportedBackend> {
    let needs_attestation = |class: BackendClass| UnsupportedBackend {
        backend: class.as_str().to_string(),
        reason: "attestation_missing".to_string(),
    };
    Ok(match class {
        BackendClass::ProcessSandbox => Box::new(Ep2Model::reference()),
        BackendClass::Namespaces => Box::new(ModelBackend::namespaces()),
        BackendClass::UserSpaceKernel => Box::new(ModelBackend::user_space_kernel(
            attestation.ok_or_else(|| needs_attestation(class))?,
        )),
        BackendClass::Microvm => Box::new(ModelBackend::microvm(
            attestation.ok_or_else(|| needs_attestation(class))?,
        )),
        BackendClass::TransparentRedirect => Box::new(ModelBackend::transparent_redirect()),
        BackendClass::External => Box::new(ModelBackend::external()),
    })
}

/// `for_policy(policy, attestation)` — the backend selection the open
/// path drives (ADR-0307): the policy's `proc.isolation_class` names the
/// boundary the policy requires, and the returned backend implements it.
/// `external` selects the hosted boundary; the attesting classes require
/// `attestation` (`attestation_missing` refuses). The
/// transparent-redirect class is a *net-plane* choice — select it through
/// [`for_class`] and confirm `apply` accepts the policy.
pub fn for_policy(
    policy: &ContainmentPolicy,
    attestation: Option<Attestation>,
) -> Result<Box<dyn ContainmentBackend>, UnsupportedBackend> {
    let class = match policy.proc.isolation_class {
        IsolationClass::None | IsolationClass::ProcessSandbox => BackendClass::ProcessSandbox,
        IsolationClass::Namespaces => BackendClass::Namespaces,
        IsolationClass::UserSpaceKernel => BackendClass::UserSpaceKernel,
        IsolationClass::Microvm => BackendClass::Microvm,
        IsolationClass::External => BackendClass::External,
    };
    for_class(class, attestation)
}
