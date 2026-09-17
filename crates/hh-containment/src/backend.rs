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
//! lands a [`LossItem`] when the policy relies on it.

use std::collections::BTreeMap;

use crate::admit::{classify_read, classify_write, ReadClass, WriteClass};
use crate::paths;
use crate::policy::{ContainmentPolicy, ExecPolicy, IsolationClass, NetMode, UnixSocketMode};
use crate::report::{FieldGroup, LossItem};

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
/// relying on the field lands a [`LossItem`] (and the owning field group's
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
    /// The relied-on losses — a [`LossItem`] for every field the policy
    /// configures that the backend cannot enforce.
    fn lowering_loss(&self, policy: &ContainmentPolicy) -> Vec<LossItem>;
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
        // The model is a process-sandbox boundary; a stronger class or the
        // hosted `external` claim cannot be applied here.
        if policy.proc.isolation_class.strength() > IsolationClass::ProcessSandbox.strength() {
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

    fn lowering_loss(&self, policy: &ContainmentPolicy) -> Vec<LossItem> {
        let mut out = Vec::new();
        let mut loss = |field: &str, reason: &str| {
            out.push(LossItem {
                field: field.to_string(),
                backend: self.name().to_string(),
                reason: reason.to_string(),
            });
        };
        if !self.caps.enforce_resources {
            for (field, _) in policy.resources.set_fields() {
                loss(field, "resources_unenforced");
            }
        }
        if policy.proc.syscall_filter.is_some() && !self.caps.enforce_syscall_filter {
            loss("proc.syscall_filter", "filter_ref_unresolved");
        }
        if policy.net.tls.terminate && !self.caps.enforce_tls {
            loss("net.tls.terminate", "tls_terminate_c2");
        }
        if !policy.net.tls.inspect_hooks.is_empty() && !self.caps.enforce_tls {
            loss("net.tls.inspect_hooks", "inspect_hooks_c2");
        }
        if policy.net.upstream_proxy.is_some() && !self.caps.enforce_proxy {
            loss("net.upstream_proxy", "proxy_env_is_not_mediation");
        }
        out
    }
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
