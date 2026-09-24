//! The kernel-run probe battery (ADR-0062 D2; AC-R-2.8.4-3) — the
//! `probed` evidence kind's only source.
//!
//! The battery is a *specification*: `expected(kind, policy)` computes the
//! outcome the policy demands, [`probe_syscall`] constructs the syscall the
//! probe issues, and [`run_battery`] drives it through
//! [`crate::backend::ContainmentBackend::run_probe`] (the fixture — a file,
//! a link, a socket — is the backend's to fabricate; the EP2 model gates
//! the syscall against its declared links).
//!
//! Battery (§5g.4 §5's `none`-mode row + ADR-0062 D2's four):
//!
//! - write outside every root fails; a `KERNEL_PROTECTED` write fails; a
//!   write through a symlink escaping the root fails; a write inside a root
//!   succeeds (positive control);
//! - `connect`/`sendto`/raw socket/`getaddrinfo`/ICMP fail under `none`
//!   **and** `mediated` (EP2 removes the namespace — the only route out is
//!   the bridged channel) and succeed under `public`;
//! - `io_uring`, `ptrace`, `process_vm_*`, `setuid` denied;
//! - the one bridged `AF_UNIX` channel succeeds; a local `bind` fails unless
//!   `local_binding` permits.

use crate::backend::{ContainmentBackend, GateVerdict, Syscall};
use crate::policy::{ContainmentPolicy, NetMode, KERNEL_PROTECTED};
use crate::report::{EvidenceKind, FieldGroup, ProbeKind, ProbeOutcome, ProbeResult};

/// The full battery, in canonical order.
pub const BATTERY: &[ProbeKind] = &[
    ProbeKind::WriteOutsideRoot,
    ProbeKind::WriteProtectedMetadata,
    ProbeKind::WriteSymlinkEscape,
    ProbeKind::WriteInsideRoot,
    ProbeKind::Connect,
    ProbeKind::SendTo,
    ProbeKind::RawSocket,
    ProbeKind::NameResolve,
    ProbeKind::Icmp,
    ProbeKind::IoUring,
    ProbeKind::Ptrace,
    ProbeKind::ProcessVm,
    ProbeKind::Setuid,
    ProbeKind::BridgedUnixSocket,
    ProbeKind::LocalBind,
];

/// The outcome the policy demands of a probe.
pub fn expected(kind: ProbeKind, policy: &ContainmentPolicy) -> ProbeOutcome {
    let net_denied = matches!(policy.net.mode, NetMode::None | NetMode::Mediated);
    match kind {
        ProbeKind::WriteOutsideRoot
        | ProbeKind::WriteProtectedMetadata
        | ProbeKind::WriteSymlinkEscape
        | ProbeKind::IoUring
        | ProbeKind::Ptrace
        | ProbeKind::ProcessVm
        | ProbeKind::Setuid => ProbeOutcome::Deny,
        ProbeKind::WriteInsideRoot => {
            if policy.fs.write.allow.is_empty() {
                ProbeOutcome::Deny // no root exists — the write fails anyway
            } else {
                ProbeOutcome::Allow
            }
        }
        ProbeKind::Connect
        | ProbeKind::SendTo
        | ProbeKind::RawSocket
        | ProbeKind::NameResolve
        | ProbeKind::Icmp => {
            if net_denied {
                ProbeOutcome::Deny
            } else {
                ProbeOutcome::Allow
            }
        }
        ProbeKind::BridgedUnixSocket => ProbeOutcome::Allow,
        ProbeKind::LocalBind => {
            if policy.net.local_binding {
                ProbeOutcome::Allow
            } else {
                ProbeOutcome::Deny
            }
        }
    }
}

/// The syscall a probe issues. `bridged` is the backend's one bridged
/// channel spelling; `WriteSymlinkEscape` is constructed by the backend's
/// own `run_probe` override (it owns the link fixture).
pub fn probe_syscall(kind: ProbeKind, policy: &ContainmentPolicy, bridged: &str) -> Syscall {
    let root = policy
        .fs
        .write
        .allow
        .first()
        .map(|w| w.root.clone())
        .unwrap_or_else(|| "workspace".to_string());
    match kind {
        ProbeKind::WriteOutsideRoot => Syscall::FsWrite {
            path: "hh.probe.outside/file".to_string(),
        },
        ProbeKind::WriteProtectedMetadata => Syscall::FsWrite {
            path: format!("{root}/{}/hh.probe", KERNEL_PROTECTED[0]),
        },
        ProbeKind::WriteSymlinkEscape => Syscall::FsWrite {
            path: format!("{root}/hh.probe.link/x"),
        },
        ProbeKind::WriteInsideRoot => Syscall::FsWrite {
            path: format!("{root}/hh.probe.txt"),
        },
        ProbeKind::Connect => Syscall::Connect {
            target: "203.0.113.10:443".to_string(),
        },
        ProbeKind::SendTo => Syscall::SendTo {
            target: "203.0.113.10:53".to_string(),
        },
        ProbeKind::RawSocket => Syscall::RawSocket,
        ProbeKind::NameResolve => Syscall::NameResolve {
            host: "hh.probe.invalid".to_string(),
        },
        ProbeKind::Icmp => Syscall::Icmp,
        ProbeKind::IoUring => Syscall::IoUring,
        ProbeKind::Ptrace => Syscall::Ptrace,
        ProbeKind::ProcessVm => Syscall::ProcessVm,
        ProbeKind::Setuid => Syscall::Setuid,
        ProbeKind::BridgedUnixSocket => Syscall::UnixConnect {
            path: bridged.to_string(),
        },
        ProbeKind::LocalBind => Syscall::Bind {
            addr: "127.0.0.1:0".to_string(),
        },
    }
}

/// Run the battery through the backend — one [`ProbeResult`] per kind.
/// `observed` maps the gate verdict (`Unenforced` is observable and never
/// equals the expectation). `evidence_kind = kernel_log` — the EP2 model's
/// observation channel (the syscall gate's deny record).
pub fn run_battery(
    backend: &dyn ContainmentBackend,
    policy: &ContainmentPolicy,
) -> Vec<ProbeResult> {
    BATTERY
        .iter()
        .map(|&kind| {
            let observed = match backend.run_probe(kind, policy) {
                GateVerdict::Allow => ProbeOutcome::Allow,
                GateVerdict::Deny { .. } => ProbeOutcome::Deny,
                GateVerdict::Unenforced => ProbeOutcome::Unenforced,
            };
            ProbeResult {
                kind,
                expected: expected(kind, policy),
                observed,
                evidence_kind: EvidenceKind::KernelLog,
            }
        })
        .collect()
}

/// The per-group evidence the battery produces: `probed` when every probe
/// of the group passed, `unknown` when any failed or observed `unenforced`
/// (never coerced — T-LCD-07).
pub fn group_evidence(
    group: FieldGroup,
    probes: &[ProbeResult],
) -> crate::report::EnforcementEvidence {
    let mut any = false;
    for p in probes.iter().filter(|p| p.kind.group() == group) {
        any = true;
        if !p.passed() {
            return crate::report::EnforcementEvidence::Unknown;
        }
    }
    if any {
        crate::report::EnforcementEvidence::Probed
    } else {
        crate::report::EnforcementEvidence::Unknown
    }
}
