//! `attach` — the handle-slot operation (§5g.4 §2 attach row; ADR-0060 D4,
//! ADR-0062 D2/D4). The environment handle carries
//! `containment: Ref<ContainmentPolicy>` by content address *or* the policy
//! inline ([`PolicySlot`]); attach applies the effective policy on a
//! backend, runs the probe battery and returns the `ContainmentReport` —
//! re-validated on resume (ADR-0130).
//!
//! The fail-closed rule (I-C4):
//!
//! - helper unavailable (`backend = None`), the backend refusing the policy
//!   (`apply` → [`UnsupportedBackend`]), a stored report stale on resume, a
//!   resolved ref not naming the resolved record, or `unknown` evidence on
//!   a field group the policy *relies on* ⇒ `security.containment.
//!   unverified` is appended and every non-`read_only` effect is refused
//!   (the [`crate::admit::floor_gate`] precondition makes the refusal
//!   per-proposal);
//! - Lab runs (any run with a `Design`/`PreRegistration`) are always
//!   fail-closed — declaring `warn_and_degrade` on one is refused outright;
//! - an interactive run may declare `on_unavailable = warn_and_degrade`
//!   ([`AttachMode::WarnAndDegrade`]): the attach *succeeds* as
//!   [`AttachOutcome::Degraded`], the `unverified` event is appended and
//!   `containment_degraded = true` stamps every subsequent event of the
//!   run (reproducibility claims cap at R0).
//!
//! A policy the backend cannot *host at all* never reaches the report —
//! `apply` refuses it. A policy relying on a field the backend cannot
//! *enforce* (a `lowering_loss` member) leaves that group's evidence
//! `unknown` — and a relied-on `unknown` is a fail-closed attach
//! (AC-R-2.8.4-8/-10).

use std::collections::BTreeMap;

use hh_hir::records::Grant;
use hh_hir::refs::{Ref, RefVersion};
use hh_wire::json::Json;

use crate::admit::{unreachable_permissions, UnreachableGrant};
use crate::backend::ContainmentBackend;
use crate::events;
use crate::policy::{ContainmentPolicy, NetMode, PolicyError};
use crate::probes;
use crate::report::{verify_report, ContainmentReport, EnforcementEvidence, FieldGroup};

/// `on_unavailable` (MUST-data; ADR-0062 D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachMode {
    /// Refuse the attach on any unverifiable evidence (the default; the
    /// only mode a Lab run may run under).
    FailClosed,
    /// Interactive runs may degrade: the attach succeeds, `unverified` is
    /// appended and `containment_degraded` stamps every event.
    WarnAndDegrade,
}

/// The handle's `containment` slot — `Ref<ContainmentPolicy>` by content
/// address + inline (§5g.4 §2 attach row).
#[derive(Debug, Clone)]
pub enum PolicySlot {
    /// The policy carried inline on the handle.
    Inline(Box<ContainmentPolicy>),
    /// A pinned `Ref` (content address = `semantic_id` + `version_id`) and
    /// the resolved policy; attach re-verifies the ref names the resolved
    /// record — a mismatch is `unverified{policy_ref_mismatch}`, never an
    /// attach of unaddressed bytes.
    ResolvedRef {
        /// The pinned ref as declared on the handle.
        reference: Ref,
        /// The policy the ref resolved to.
        policy: Box<ContainmentPolicy>,
    },
}

impl PolicySlot {
    /// The policy the slot carries (post ref-verification — use
    /// [`attach`], which performs it).
    pub fn policy(&self) -> &ContainmentPolicy {
        match self {
            PolicySlot::Inline(p) => p,
            PolicySlot::ResolvedRef { policy, .. } => policy,
        }
    }

    /// Whether the slot's ref names the carried record
    /// (`semantic_id = policy_id ∧ version = Pinned(version_id)`).
    /// `Inline` carries no ref to check.
    pub fn ref_verified(&self) -> bool {
        match self {
            PolicySlot::Inline(_) => true,
            PolicySlot::ResolvedRef { reference, policy } => {
                reference.semantic_id == policy.policy_id
                    && matches!(
                        &reference.version,
                        RefVersion::Pinned(v) if *v == policy.version_id
                    )
            }
        }
    }
}

/// The `attach` operands.
pub struct AttachInput<'a> {
    /// The environment handle's identity coordinate (the `env_handle`
    /// member of `security.containment.applied`).
    pub env_handle: &'a str,
    /// The handle's `containment` slot.
    pub policy: PolicySlot,
    /// The backend to apply the policy on — `None` is the
    /// helper-unavailable fault-injection point.
    pub backend: Option<&'a dyn ContainmentBackend>,
    /// `on_unavailable`.
    pub mode: AttachMode,
    /// Whether the run is a Lab run (any `Design`/`PreRegistration`) —
    /// always fail-closed (I-C4).
    pub lab_run: bool,
    /// A stored `ContainmentReport` re-validated on resume (ADR-0130) —
    /// `Some` requires `policy_version_id` to match the attached policy's,
    /// else `unverified{report_stale}`.
    pub resume_report: Option<&'a ContainmentReport>,
    /// `(permission_id, grant)` pairs minted at seal — the
    /// `PermissionUnreachable` warning's input (AC-R-2.8.4-14's third
    /// half; attach is the Stage-1 seam where policy and grants meet —
    /// the seal-path call site lands with the `EnvHandle` record, S1.16).
    pub grants: &'a [(String, Grant)],
}

/// A seal/attach-time warning — advisory, never a refusal (the third half
/// of AC-R-2.8.4-14).
#[derive(Debug, Clone, PartialEq)]
pub enum SealWarning {
    /// A grant's domain can never be admitted under this policy
    /// (`PermissionUnreachable` — e.g. `net_egress` on a `none`
    /// environment).
    PermissionUnreachable(UnreachableGrant),
}

/// The attach result.
#[derive(Debug)]
pub enum AttachOutcome {
    /// The policy applied and every relied-on group carries evidence —
    /// `event` is the `security.containment.applied` payload to append.
    Applied {
        /// The report (freshness-bound to `policy.version_id`).
        report: ContainmentReport,
        /// The `security.containment.applied` payload.
        event: Json,
        /// Advisory warnings (`PermissionUnreachable`).
        warnings: Vec<SealWarning>,
    },
    /// `warn_and_degrade` admitted an unverifiable attach — the
    /// `unverified` event is appended and `containment_degraded = true`
    /// stamps every subsequent event of the run.
    Degraded {
        /// The report, when a backend ran the battery (`None` when no
        /// backend ever applied the policy).
        report: Option<ContainmentReport>,
        /// The `security.containment.unverified` payload.
        unverified_event: Json,
        /// Advisory warnings.
        warnings: Vec<SealWarning>,
    },
}

impl AttachOutcome {
    /// Whether the run is `containment_degraded` (stamps every event).
    pub fn degraded(&self) -> bool {
        matches!(self, AttachOutcome::Degraded { .. })
    }
}

/// The attach failure sum — the only evidence failure is
/// `ContainmentUnverified`-shaped ([`AttachError::Unverified`] carries the
/// `unverified` payload to append).
#[derive(Debug)]
pub enum AttachError {
    /// The policy document is invalid — a validation error, not an
    /// evidence failure.
    PolicyInvalid(PolicyError),
    /// `warn_and_degrade` was declared on a Lab run — refused outright
    /// (Lab runs are always fail-closed).
    DegradeDeclaredOnLab,
    /// Fail-closed refusal — the `security.containment.unverified`
    /// payload is appended and every non-`read_only` effect of the run is
    /// refused through the [`crate::admit::floor_gate`] precondition.
    Unverified {
        /// The `field_group` member (`fs|net|proc|resources|attach|report`).
        field_group: String,
        /// The closed reason tag.
        reason: String,
        /// The `security.containment.unverified` payload to append.
        event: Json,
    },
}

impl std::fmt::Display for AttachError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttachError::PolicyInvalid(e) => write!(f, "invalid policy: {e}"),
            AttachError::DegradeDeclaredOnLab => {
                write!(f, "warn_and_degrade declared on a Lab run")
            }
            AttachError::Unverified {
                field_group,
                reason,
                ..
            } => write!(f, "ContainmentUnverified: {field_group} ({reason})"),
        }
    }
}

impl std::error::Error for AttachError {}

/// The field groups the effective policy *relies on* — the fail-closed
/// check's operand (AC-R-2.8.4-8/-10):
///
/// - `fs` — always: `protected_metadata ⊇ KERNEL_PROTECTED` is non-empty
///   on every valid policy and `write` is an `allow_only` extent;
/// - `proc` — always: `ptrace`/`setuid` are fixed `deny` claims and
///   `no_new_privs`/`die_with_parent`/`unshare`/`isolation_class` assert
///   the process boundary;
/// - `net` — under `none`/`mediated` (a `public` policy declares no net
///   boundary to rely on);
/// - `resources` — when any bound is set.
pub fn relied_groups(policy: &ContainmentPolicy) -> Vec<FieldGroup> {
    let mut v = vec![FieldGroup::Fs, FieldGroup::Proc];
    if policy.net.mode != NetMode::Public {
        v.push(FieldGroup::Net);
    }
    if !policy.resources.is_empty() {
        v.push(FieldGroup::Resources);
    }
    v
}

/// The per-group evidence the attach produces: `probed` when the battery
/// covered the group and every probe passed; `reported` when the group has
/// no battery probes and the backend declares enforcement (`resources` at
/// Stage 1 — the battery holds no resource probes, so an enforcing
/// backend's declaration is `reported`, never `probed`); `unknown`
/// otherwise — never coerced (T-LCD-07).
fn group_evidence_at_attach(
    group: FieldGroup,
    backend: &dyn ContainmentBackend,
    probes_out: &[crate::report::ProbeResult],
) -> EnforcementEvidence {
    let covered = probes_out.iter().any(|p| p.kind.group() == group);
    if covered {
        return probes::group_evidence(group, probes_out);
    }
    // No battery coverage — the backend's declared cap is the only
    // evidence, and it is `reported`, never `probed`.
    let enforced = match group {
        FieldGroup::Fs => backend.caps().enforce_fs && backend.caps().enforce_exec,
        FieldGroup::Net => backend.caps().enforce_net,
        FieldGroup::Proc => backend.caps().enforce_proc,
        FieldGroup::Resources => backend.caps().enforce_resources,
    };
    if enforced {
        EnforcementEvidence::Reported
    } else {
        EnforcementEvidence::Unknown
    }
}

/// `attach(input) → AttachOutcome | AttachError` — apply + probe + report,
/// fail-closed (I-C4).
pub fn attach(input: &AttachInput) -> Result<AttachOutcome, AttachError> {
    // Lab runs never degrade — the declaration itself is refused.
    if input.lab_run && input.mode == AttachMode::WarnAndDegrade {
        return Err(AttachError::DegradeDeclaredOnLab);
    }

    // The unverifiable path under the declared mode — `report` is `Some`
    // when the battery already ran (a relied-on-group `unknown`), `None`
    // when nothing could be produced.
    let unverified = |field_group: &str,
                      reason: String,
                      report: Option<ContainmentReport>,
                      warnings: Vec<SealWarning>|
     -> Result<AttachOutcome, AttachError> {
        let event = events::unverified_payload(field_group, &reason);
        match input.mode {
            AttachMode::FailClosed => Err(AttachError::Unverified {
                field_group: field_group.to_string(),
                reason,
                event,
            }),
            AttachMode::WarnAndDegrade => Ok(AttachOutcome::Degraded {
                report,
                unverified_event: event,
                warnings,
            }),
        }
    };

    // The slot must name the record it carries — a `Ref` whose coordinates
    // don't match the resolved policy is unverifiable, never attached.
    if !input.policy.ref_verified() {
        // A ref mismatch is an *integrity* failure — the resolved bytes are
        // not what the pinned address names — not an availability or
        // evidence-strength failure. `warn_and_degrade` covers
        // unavailable/unverified *enforcement*; it must never attach
        // unaddressed bytes, so this refuses under either mode.
        return Err(AttachError::Unverified {
            field_group: events::ATTACH_GROUP.to_string(),
            reason: "policy_ref_mismatch".to_string(),
            event: events::unverified_payload(events::ATTACH_GROUP, "policy_ref_mismatch"),
        });
    }
    let policy = input.policy.policy();
    policy.validate().map_err(AttachError::PolicyInvalid)?;

    // Re-validation on resume — a stored report is valid only for the
    // policy version it was computed against.
    if input
        .resume_report
        .is_some_and(|r| verify_report(r, &policy.version_id).is_err())
    {
        return unverified(
            events::REPORT_GROUP,
            "report_stale".to_string(),
            None,
            vec![],
        );
    }

    let warnings: Vec<SealWarning> = unreachable_permissions(policy, input.grants)
        .into_iter()
        .map(SealWarning::PermissionUnreachable)
        .collect();

    let Some(backend) = input.backend else {
        return unverified(
            events::ATTACH_GROUP,
            "helper_unavailable".to_string(),
            None,
            warnings,
        );
    };
    if let Err(u) = backend.apply(policy) {
        return unverified(
            events::ATTACH_GROUP,
            format!("backend_unsupported:{}", u.reason),
            None,
            warnings,
        );
    }

    // apply + probe + report.
    let probes_out = probes::run_battery(backend, policy);
    let mut evidence = BTreeMap::new();
    for g in FieldGroup::ALL {
        evidence.insert(g, group_evidence_at_attach(g, backend, &probes_out));
    }
    let report = ContainmentReport {
        policy_version_id: policy.version_id.clone(),
        backend: backend.name().to_string(),
        isolation_class: policy.proc.isolation_class,
        enforcement_evidence: evidence,
        lowering_loss: backend.lowering_loss(policy),
        probes: probes_out,
    };

    // A relied-on group with `unknown` evidence — or a `lowering_loss`
    // naming a field in it — is a fail-closed attach (AC-R-2.8.4-8/-10).
    for g in relied_groups(policy) {
        let loss = report
            .lowering_loss
            .iter()
            .any(|l| l.field.starts_with(&format!("{}.", g.as_str())));
        if report.evidence(g) == EnforcementEvidence::Unknown || loss {
            return unverified(
                g.as_str(),
                format!("evidence_unknown:{}", g.as_str()),
                Some(report),
                warnings,
            );
        }
    }

    Ok(AttachOutcome::Applied {
        event: events::applied_payload(input.env_handle, policy, &report),
        report,
        warnings,
    })
}

/// `evidence_preview(backend, policy) → map<FieldGroup,
/// EnforcementEvidence>` — the **pure** half of `attach`: `apply` +
/// the probe battery + `group_evidence_at_attach` over the same inputs,
/// minting nothing. A caller that needs the answer *before* a handle
/// exists (the `BypassWithoutContainment` pre-open gate, ADR-0168 D3)
/// runs this; the attach that follows re-derives the identical map —
/// same inputs, same fold, never a second rule. `Err` is the `apply`
/// refusal (`backend_unsupported`); the policy is assumed `validate`d.
pub fn evidence_preview(
    backend: &dyn ContainmentBackend,
    policy: &ContainmentPolicy,
) -> Result<BTreeMap<FieldGroup, EnforcementEvidence>, AttachError> {
    backend.apply(policy).map_err(|u| AttachError::Unverified {
        field_group: events::ATTACH_GROUP.to_string(),
        reason: format!("backend_unsupported:{}", u.reason),
        event: events::unverified_payload(
            events::ATTACH_GROUP,
            &format!("backend_unsupported:{}", u.reason),
        ),
    })?;
    let probes_out = probes::run_battery(backend, policy);
    let mut evidence = BTreeMap::new();
    for g in FieldGroup::ALL {
        evidence.insert(g, group_evidence_at_attach(g, backend, &probes_out));
    }
    Ok(evidence)
}
