//! `ContainmentReport` (§5g.4 §3; ADR-0062 D2) — the attach-time record of
//! what the backend *can prove* about the effective policy:
//!
//! ```text
//! {policy_version_id, backend, isolation_class,
//!  enforcement_evidence: map<field_group ∈ {fs, net, proc, resources},
//!                            attested | reported | probed | unknown>,
//!  lowering_loss: [LoweringLoss{field, backend, reason, kind,
//!                               consequence}],
//!  probes: [ProbeResult{kind, expected, observed, evidence_kind}]}
//! ```
//!
//! `unknown` is never coerced (T-LCD-07) — a group the backend cannot
//! enforce is `unknown`, never `reported`. `probed` requires the kernel-run
//! self-test battery ([`crate::probes`]). The `authorize` precondition
//! refuses every non-`read_only` effect of a domain whose **required**
//! group is `unknown` (AC-R-2.8.4-8/-14; I-C4).

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::policy::IsolationClass;

/// `field_group ∈ {fs, net, proc, resources}` — the governed groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FieldGroup {
    /// Filesystem containment.
    Fs,
    /// Network/egress containment.
    Net,
    /// Process containment.
    Proc,
    /// Resource bounds.
    Resources,
}

impl FieldGroup {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            FieldGroup::Fs => "fs",
            FieldGroup::Net => "net",
            FieldGroup::Proc => "proc",
            FieldGroup::Resources => "resources",
        }
    }

    /// Every group, in canonical order.
    pub const ALL: [FieldGroup; 4] = [
        FieldGroup::Fs,
        FieldGroup::Net,
        FieldGroup::Proc,
        FieldGroup::Resources,
    ];
}

/// `attested | reported | probed | unknown` (T-LCD-07 — `unknown` is a
/// first-class answer, never coerced upward).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EnforcementEvidence {
    /// Attested by the substrate (C1 attestation path).
    Attested,
    /// The backend's self-report (e.g. hosted `external`, or an
    /// uninstrumented group the policy does not rely on).
    Reported,
    /// Verified by the kernel-run probe battery.
    Probed,
    /// No evidence — fail-closed for relying domains.
    Unknown,
}

impl EnforcementEvidence {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EnforcementEvidence::Attested => "attested",
            EnforcementEvidence::Reported => "reported",
            EnforcementEvidence::Probed => "probed",
            EnforcementEvidence::Unknown => "unknown",
        }
    }
}

/// `LoweringLoss.kind ∈ {no_slot, hint_only, untyped_slot, narrowed,
/// truncated}` — the §3.2.5 `LossClass` closed set, spelled locally
/// (ADR-0341 D1: the dependency order forbids `hh-containment →
/// hh-compiler`; the vocabulary is the spec's, the codec is each layer's).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LossKind {
    /// The member exists; the backend has no enforcement slot for it.
    NoSlot,
    /// The member travels as a hint only — never an enforcement claim
    /// (`upstream_proxy`/proxy env vars "counting as mediation").
    HintOnly,
    /// The member is accepted but its type/grammar is unchecked.
    UntypedSlot,
    /// The member is enforced against a narrowed interpretation.
    Narrowed,
    /// The member is truncated (a bounded rendering, not the full value).
    Truncated,
}

impl LossKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            LossKind::NoSlot => "no_slot",
            LossKind::HintOnly => "hint_only",
            LossKind::UntypedSlot => "untyped_slot",
            LossKind::Narrowed => "narrowed",
            LossKind::Truncated => "truncated",
        }
    }
}

/// `LoweringLoss.consequence ∈ {fail_closed, stratified}` — what the loss
/// does to the run (ADR-0341 D2): `FailClosed` — a relied-on group carrying
/// the loss refuses at attach (the member was *required*); `Stratified` —
/// the loss is recorded and the run stratifies on it, never silently
/// upgraded (a `upstream_proxy`/proxy-env claim is a stratifier — "recorded
/// as `lowering_loss`; the run is stratified; never counts as `mediated`",
/// §5g.4 §5's threat row — attach proceeds, the loss rides the record).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LossConsequence {
    /// The loss on a relied-on group fails attach.
    FailClosed,
    /// The loss is recorded; the run stratifies on it.
    Stratified,
}

impl LossConsequence {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            LossConsequence::FailClosed => "fail_closed",
            LossConsequence::Stratified => "stratified",
        }
    }
}

/// `LoweringLoss{field, backend, reason, kind, consequence}` — a field the
/// backend cannot enforce (ADR-0062 D2; T-LCD-11's analogue for
/// containment; DF-S1.12-4's typed per-backend report row — the
/// `{declared_field, kind, consequence}` reading of §5g.4 §3).
#[derive(Debug, Clone, PartialEq)]
pub struct LoweringLoss {
    /// The policy field the row names (`declared_field`).
    pub declared_field: String,
    /// The backend that cannot enforce it.
    pub backend: String,
    /// The closed reason tag (why the slot is absent).
    pub reason: String,
    /// The §3.2.5 loss class.
    pub kind: LossKind,
    /// What the loss does to the run.
    pub consequence: LossConsequence,
}

/// `LossItem` — the pre-R2.9b row shape's name (kept as the alias so
/// pre-R2.9b readers name the same type; the canonical name is
/// [`LoweringLoss`]).
pub type LossItem = LoweringLoss;

impl LoweringLoss {
    /// The canonical member form — `{field, backend, reason, kind,
    /// consequence}` (the spec'd `{field, backend, reason}` members
    /// preserved verbatim; `kind`/`consequence` are the R2.9b additions).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("field", Json::str(self.declared_field.clone())),
            ("backend", Json::str(self.backend.clone())),
            ("reason", Json::str(self.reason.clone())),
            ("kind", Json::str(self.kind.as_str())),
            ("consequence", Json::str(self.consequence.as_str())),
        ])
    }
}

/// The probe battery's kind set (ADR-0062 D2 + the §5g.4 §5 `none`-mode row;
/// AC-H4-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProbeKind {
    /// A write outside every writable root fails.
    WriteOutsideRoot,
    /// A write to a `KERNEL_PROTECTED` name inside a root fails.
    WriteProtectedMetadata,
    /// A write through a symlink whose target escapes the root fails
    /// (symlink-resolution *before* validation — F1).
    WriteSymlinkEscape,
    /// A write inside a writable root succeeds (the positive control).
    WriteInsideRoot,
    /// `connect` fails (under `none`/`mediated`) or succeeds (`public`).
    Connect,
    /// `sendto` fails/succeeds likewise.
    SendTo,
    /// Raw sockets fail/succeed likewise.
    RawSocket,
    /// Name resolution fails under `none`/`mediated` (DNS is egress — the
    /// environment never resolves names under `mediator`).
    NameResolve,
    /// ICMP fails/succeeds likewise.
    Icmp,
    /// `io_uring` is denied.
    IoUring,
    /// `ptrace` is denied.
    Ptrace,
    /// `process_vm_*` is denied.
    ProcessVm,
    /// `setuid` is denied.
    Setuid,
    /// The one bridged `AF_UNIX` channel succeeds.
    BridgedUnixSocket,
    /// A local bind fails unless `local_binding` permits it.
    LocalBind,
}

impl ProbeKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ProbeKind::WriteOutsideRoot => "write_outside_root",
            ProbeKind::WriteProtectedMetadata => "write_protected_metadata",
            ProbeKind::WriteSymlinkEscape => "write_symlink_escape",
            ProbeKind::WriteInsideRoot => "write_inside_root",
            ProbeKind::Connect => "connect",
            ProbeKind::SendTo => "sendto",
            ProbeKind::RawSocket => "raw_socket",
            ProbeKind::NameResolve => "name_resolve",
            ProbeKind::Icmp => "icmp",
            ProbeKind::IoUring => "io_uring",
            ProbeKind::Ptrace => "ptrace",
            ProbeKind::ProcessVm => "process_vm",
            ProbeKind::Setuid => "setuid",
            ProbeKind::BridgedUnixSocket => "bridged_unix_socket",
            ProbeKind::LocalBind => "local_bind",
        }
    }

    /// The field group this probe evidences.
    pub fn group(self) -> FieldGroup {
        match self {
            ProbeKind::WriteOutsideRoot
            | ProbeKind::WriteProtectedMetadata
            | ProbeKind::WriteSymlinkEscape
            | ProbeKind::WriteInsideRoot => FieldGroup::Fs,
            ProbeKind::Connect
            | ProbeKind::SendTo
            | ProbeKind::RawSocket
            | ProbeKind::NameResolve
            | ProbeKind::Icmp
            | ProbeKind::BridgedUnixSocket
            | ProbeKind::LocalBind => FieldGroup::Net,
            ProbeKind::IoUring | ProbeKind::Ptrace | ProbeKind::ProcessVm | ProbeKind::Setuid => {
                FieldGroup::Proc
            }
        }
    }
}

/// A probe's expected or observed outcome (`unenforced` is observable —
/// the backend reporting it cannot enforce the syscall is honest evidence
/// of *nothing*, and never equals `allow`/`deny`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// The syscall was permitted.
    Allow,
    /// The syscall was denied.
    Deny,
    /// The backend does not enforce this class.
    Unenforced,
}

impl ProbeOutcome {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ProbeOutcome::Allow => "allow",
            ProbeOutcome::Deny => "deny",
            ProbeOutcome::Unenforced => "unenforced",
        }
    }
}

/// `evidence_kind ∈ {kernel_log, exit_signal, mediator, heuristic_output}`
/// — how a denial/probe observation was evidenced (the `violated` payload's
/// member; §5g.4 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceKind {
    /// The kernel/gate log observed the denial.
    KernelLog,
    /// An exit signal.
    ExitSignal,
    /// The mediator observed it.
    Mediator,
    /// A heuristic detector (labelled — never authorises).
    HeuristicOutput,
}

impl EvidenceKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EvidenceKind::KernelLog => "kernel_log",
            EvidenceKind::ExitSignal => "exit_signal",
            EvidenceKind::Mediator => "mediator",
            EvidenceKind::HeuristicOutput => "heuristic_output",
        }
    }
}

/// `ProbeResult{kind, expected, observed, evidence_kind}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeResult {
    /// The probe.
    pub kind: ProbeKind,
    /// The expected outcome under the policy.
    pub expected: ProbeOutcome,
    /// The observed outcome.
    pub observed: ProbeOutcome,
    /// How the observation was evidenced.
    pub evidence_kind: EvidenceKind,
}

impl ProbeResult {
    /// Whether the observation matched the expectation.
    pub fn passed(&self) -> bool {
        self.expected == self.observed
    }

    /// The canonical member form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("kind", Json::str(self.kind.as_str())),
            ("expected", Json::str(self.expected.as_str())),
            ("observed", Json::str(self.observed.as_str())),
            ("evidence_kind", Json::str(self.evidence_kind.as_str())),
        ])
    }
}

/// `ContainmentReport` — the attach-time evidence record.
#[derive(Debug, Clone, PartialEq)]
pub struct ContainmentReport {
    /// The policy version the report covers (stale reports fail closed).
    pub policy_version_id: String,
    /// The backend name.
    pub backend: String,
    /// The isolation class in force.
    pub isolation_class: IsolationClass,
    /// `enforcement_evidence` per field group.
    pub enforcement_evidence: BTreeMap<FieldGroup, EnforcementEvidence>,
    /// The relied-on losses the backend declared.
    pub lowering_loss: Vec<LoweringLoss>,
    /// The probe-battery results.
    pub probes: Vec<ProbeResult>,
}

/// A stale report — the report's `policy_version_id` does not match the
/// attached policy's (a report is valid only for the policy it was
/// computed against; re-validation on resume re-runs `attach`).
#[derive(Debug, Clone, PartialEq)]
pub struct StaleReport {
    /// The report's policy version.
    pub report_version: String,
    /// The attached policy's version.
    pub policy_version: String,
}

impl ContainmentReport {
    /// The evidence for a group (`Unknown` when absent — never coerced).
    pub fn evidence(&self, group: FieldGroup) -> EnforcementEvidence {
        self.enforcement_evidence
            .get(&group)
            .copied()
            .unwrap_or(EnforcementEvidence::Unknown)
    }

    /// The canonical member form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "policy_version_id",
                Json::str(self.policy_version_id.clone()),
            ),
            ("backend", Json::str(self.backend.clone())),
            ("isolation_class", Json::str(self.isolation_class.as_str())),
            (
                "enforcement_evidence",
                Json::Obj(
                    self.enforcement_evidence
                        .iter()
                        .map(|(g, e)| (g.as_str().to_string(), Json::str(e.as_str())))
                        .collect(),
                ),
            ),
            (
                "lowering_loss",
                Json::Arr(
                    self.lowering_loss
                        .iter()
                        .map(LoweringLoss::to_json)
                        .collect(),
                ),
            ),
            (
                "probes",
                Json::Arr(self.probes.iter().map(ProbeResult::to_json).collect()),
            ),
        ])
    }
}

/// `verify_report(report, policy_version_id)` — the freshness check
/// (re-validation on resume; I-C4's "report stale" arm).
pub fn verify_report(
    report: &ContainmentReport,
    policy_version_id: &str,
) -> Result<(), StaleReport> {
    if report.policy_version_id == policy_version_id {
        Ok(())
    } else {
        Err(StaleReport {
            report_version: report.policy_version_id.clone(),
            policy_version: policy_version_id.to_string(),
        })
    }
}
