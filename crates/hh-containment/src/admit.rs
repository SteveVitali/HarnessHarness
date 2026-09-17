//! `admits(effective_policy, p)` — the floor input to `authorize`
//! (§5g.4 §2.1; ADR-0062 D3): `admitted | refused{reason} | amendable{diff}`.
//!
//! The floor's Stage-1 semantics (ADR-0243):
//!
//! - the verdict is over the proposal's **canonical arguments** — the
//!   scope-bound `fs_path`/`host` values the monitor's `SurfaceArgMap`
//!   evaluator already resolved (I-H5);
//! - a **refused** is an affirmative boundary denial: a `deny`-rule hit, a
//!   `KERNEL_PROTECTED`/protected-metadata name, a `read_only_subpaths` or
//!   `deny_within_allow` hit, `net.mode = none` for `net_egress`, or an
//!   `allow_only` extent the scoped value can't be verified inside. A
//!   `refused` never consults Π and never asks (AC-R-2.8.4-14);
//! - an **amendable** is an absent-coverage gap the endorsed hatch could
//!   close — a scoped path outside every `WritableRoot`, a missing `allow`
//!   rule under `mediated`, an `exec` entry. The `amend()` operation itself
//!   is Stage 2; at Stage 1 `amendable` denies like `refused` (the check
//!   record distinguishes it — `containment:amendable:<diff>`);
//! - `scope_unknown` is handled per the deny-by-default class: under
//!   `allow_all_except`-style extents (deny-listed reads, a nonempty
//!   writable-root/exec extent, `mediated` egress) the *class* is admitted —
//!   the EP2 gate bounds the extent at syscall granularity and the monitor's
//!   coverage check already requires a `*` grant; under `allow_only` extents
//!   (allow-list reads, empty write/exec extents, `none`-mode egress) the
//!   membership check cannot run ⇒ `refused{unverifiable}` or the matching
//!   `amendable`.

use hh_hir::kinds::EffectDomain;
use hh_hir::records::{Grant, ScopeBindings};
use hh_monitor::args::{self, ScopeKind};
use hh_monitor::assess::Tri;
use hh_monitor::monitor::ContainmentGate;

use crate::paths;
use crate::policy::{ContainmentPolicy, ExecPolicy, NetMode, ReadMode, RuleDecision};
use crate::report::{verify_report, ContainmentReport, EnforcementEvidence, FieldGroup};

/// The field groups a domain's admission requires evidence for (ADR-0062
/// D3's "required field group" — the `authorize` precondition's operand).
pub fn required_field_groups(domain: EffectDomain) -> &'static [FieldGroup] {
    match domain {
        EffectDomain::FsRead | EffectDomain::FsWrite => &[FieldGroup::Fs],
        EffectDomain::NetEgress => &[FieldGroup::Net],
        EffectDomain::Exec | EffectDomain::SpawnProcess => &[FieldGroup::Proc],
        EffectDomain::SecretAccess => &[FieldGroup::Fs, FieldGroup::Net],
        _ => &[],
    }
}

/// The proposal's containment-relevant canonical arguments — built by the
/// monitor from `CanonicalArgs` (scope-bound `fs_path`/`host` values).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AdmitInput {
    /// The proposal's effect domain.
    pub domain: Option<EffectDomain>,
    /// The `fs_path`-scoped canonical values.
    pub fs_paths: Vec<String>,
    /// The `host`-scoped canonical values (already `normalize`d by the
    /// caller, or normalised here).
    pub hosts: Vec<String>,
    /// `scope_bindings_unknown` — no canonical scope exists.
    pub scope_unknown: bool,
}

/// `refused{reason}` — the closed Stage-1 sum.
#[derive(Debug, Clone, PartialEq)]
pub enum RefusedReason {
    /// `net.mode = none` (or the domain's boundary) forbids the class.
    ModeNone,
    /// A `deny` rule matched (egress rule, read deny, `deny_within_allow`).
    DenyRule {
        /// The matched rule/pattern's canonical key.
        rule: String,
    },
    /// A `KERNEL_PROTECTED` / `protected_metadata` name was targeted.
    ProtectedPath {
        /// The protected name.
        name: String,
    },
    /// A `read_only_subpaths` member was targeted for write.
    ReadOnlySubpath {
        /// The subpath.
        path: String,
    },
    /// Admission requires membership the scope can't verify
    /// (`scope_unknown` under an `allow_only` extent; an unscoped `host` for
    /// `net_egress`).
    Unverifiable,
}

impl RefusedReason {
    /// The closed detail tag (recorded in the decision's `checks[]`).
    pub fn as_str(&self) -> &'static str {
        match self {
            RefusedReason::ModeNone => "mode_none",
            RefusedReason::DenyRule { .. } => "deny_rule",
            RefusedReason::ProtectedPath { .. } => "protected_path",
            RefusedReason::ReadOnlySubpath { .. } => "read_only_subpath",
            RefusedReason::Unverifiable => "unverifiable",
        }
    }
}

/// `amendable{diff}` — the declared Stage-1 diff the endorsed hatch would
/// apply (`amend()` is Stage 2; at Stage 1 the verdict denies with this
/// record attached).
#[derive(Debug, Clone, PartialEq)]
pub enum ContainmentDiff {
    /// Add a `WritableRoot` covering the path.
    AddWritableRoot {
        /// The root (`*` when the scope is unknown).
        root: String,
    },
    /// Add a `read.allow` entry (`allow_only`).
    AddReadAllow {
        /// The path.
        path: String,
    },
    /// Add an egress `allow` rule for the host.
    AddEgressAllow {
        /// The normalised host.
        host: String,
    },
    /// Add an `exec.allow` entry.
    AddExecAllow {
        /// The path.
        path: String,
    },
}

impl ContainmentDiff {
    /// The closed detail tag.
    pub fn as_str(&self) -> &'static str {
        match self {
            ContainmentDiff::AddWritableRoot { .. } => "add_writable_root",
            ContainmentDiff::AddReadAllow { .. } => "add_read_allow",
            ContainmentDiff::AddEgressAllow { .. } => "add_egress_allow",
            ContainmentDiff::AddExecAllow { .. } => "add_exec_allow",
        }
    }
}

/// `admitted | refused{reason} | amendable{diff}` (§5g.4 §2.1).
#[derive(Debug, Clone, PartialEq)]
pub enum AdmitVerdict {
    /// The floor admits the class (and every scoped extent checked).
    Admitted,
    /// An affirmative boundary denial.
    Refused {
        /// The reason.
        reason: RefusedReason,
    },
    /// A gap the endorsed hatch could close.
    Amendable {
        /// The diff.
        diff: ContainmentDiff,
    },
}

impl AdmitVerdict {
    /// The tag for the check record.
    pub fn tag(&self) -> &'static str {
        match self {
            AdmitVerdict::Admitted => "admitted",
            AdmitVerdict::Refused { .. } => "refused",
            AdmitVerdict::Amendable { .. } => "amendable",
        }
    }
}

/// `admits(effective_policy, p)` — the deterministic floor.
pub fn admits(policy: &ContainmentPolicy, input: &AdmitInput) -> AdmitVerdict {
    match input.domain {
        Some(EffectDomain::NetEgress) => admit_net(policy, input),
        Some(EffectDomain::FsWrite) => admit_fs_write(policy, input),
        Some(EffectDomain::FsRead) => admit_fs_read(policy, input),
        Some(EffectDomain::Exec) | Some(EffectDomain::SpawnProcess) => admit_exec(policy, input),
        Some(EffectDomain::SecretAccess) => {
            // Credential use rides the net_egress effect (CF-130): check the
            // fs_read side of any scoped paths plus the egress side of any
            // scoped hosts (the binding slot itself is Stage 2).
            let fs = admit_fs_read(policy, input);
            if fs.tag() != "admitted" {
                return fs;
            }
            if !input.hosts.is_empty() {
                return admit_net(policy, input);
            }
            if policy.net.mode == NetMode::None {
                // No egress route exists — the access is contained but the
                // class's use-path is closed: unreachable, refused.
                return AdmitVerdict::Refused {
                    reason: RefusedReason::ModeNone,
                };
            }
            AdmitVerdict::Admitted
        }
        _ => AdmitVerdict::Admitted,
    }
}

/// `deny` before `allow` (N1/I-C1): every scoped host is checked against
/// deny rules first, then allow rules.
fn admit_net(policy: &ContainmentPolicy, input: &AdmitInput) -> AdmitVerdict {
    match policy.net.mode {
        NetMode::None => AdmitVerdict::Refused {
            reason: RefusedReason::ModeNone,
        },
        NetMode::Public => AdmitVerdict::Admitted,
        NetMode::Mediated => {
            let hosts: Vec<String> = input
                .hosts
                .iter()
                .map(|h| crate::policy::normalize_host(h))
                .collect();
            if hosts.is_empty() {
                // No scoped host — `mediated` admits the class; the actual
                // request is decided at EP3 (Stage 2) and scope coverage is
                // the ceiling's job.
                return AdmitVerdict::Admitted;
            }
            for h in &hosts {
                for r in policy
                    .net
                    .rules
                    .iter()
                    .filter(|r| r.decision == RuleDecision::Deny)
                {
                    if r.host.matches(h) {
                        return AdmitVerdict::Refused {
                            reason: RefusedReason::DenyRule {
                                rule: r.host.spelling(),
                            },
                        };
                    }
                }
            }
            for h in &hosts {
                if !policy
                    .net
                    .rules
                    .iter()
                    .any(|r| r.decision == RuleDecision::Allow && r.host.matches(h))
                {
                    return AdmitVerdict::Amendable {
                        diff: ContainmentDiff::AddEgressAllow { host: h.clone() },
                    };
                }
            }
            AdmitVerdict::Admitted
        }
    }
}

/// The write-path classification — the **single source** the `admits` floor
/// and the EP2 model gate share (CC1: one matcher, two verdict mappings).
#[derive(Debug, Clone, PartialEq)]
pub enum WriteClass {
    /// The path is writable (inside a root, unprotected, unlisted).
    Inside,
    /// Outside every `WritableRoot`.
    OutsideRoot,
    /// Hits a `KERNEL_PROTECTED`/`protected_metadata` name.
    Protected(String),
    /// Hits a `read_only_subpaths` member.
    ReadOnlySubpath(String),
    /// Hits a `deny_within_allow` entry.
    Denied(String),
}

/// Classify a write path: protected names (suffix match at any depth inside
/// the root — I-C2), then `read_only_subpaths`, then `deny_within_allow`.
pub fn classify_write(policy: &ContainmentPolicy, path: &str) -> WriteClass {
    let p = paths::normalize_path(path);
    for w in &policy.fs.write.allow {
        if !paths::within(&w.root, &p) {
            continue;
        }
        // Protected metadata — the fs-level set (⊇ KERNEL_PROTECTED) ∪ the
        // root's own names; every name is read-only inside the root.
        for name in policy
            .fs
            .protected_metadata
            .iter()
            .chain(w.protected_metadata_names.iter())
        {
            if paths::pattern_matches(name, &p) {
                return WriteClass::Protected(name.clone());
            }
        }
        for sub in &w.read_only_subpaths {
            let full = format!(
                "{}/{}",
                paths::normalize_path(&w.root),
                paths::normalize_path(sub)
            );
            if paths::within(&full, &p) || paths::pattern_matches(sub, &p) {
                return WriteClass::ReadOnlySubpath(sub.clone());
            }
        }
        for d in &policy.fs.write.deny_within_allow {
            if paths::pattern_matches(d, &p) {
                return WriteClass::Denied(d.clone());
            }
        }
        return WriteClass::Inside;
    }
    WriteClass::OutsideRoot
}

/// The read-path classification — `admits` and the gate share it.
#[derive(Debug, Clone, PartialEq)]
pub enum ReadClass {
    /// The path is readable.
    Allowed,
    /// A `deny` entry matches (minus `allow_within_deny` relief).
    Denied,
    /// `allow_only` mode and the path is not in `allow`.
    NotInAllow,
}

/// Classify a read path against `fs.read`.
pub fn classify_read(policy: &ContainmentPolicy, path: &str) -> ReadClass {
    let p = paths::normalize_path(path);
    let denied = policy
        .fs
        .read
        .deny
        .iter()
        .any(|d| paths::pattern_matches(d, &p));
    let relieved = policy
        .fs
        .read
        .allow_within_deny
        .iter()
        .any(|a| paths::pattern_matches(a, &p));
    if denied && !relieved {
        return ReadClass::Denied;
    }
    if policy.fs.read.mode == ReadMode::AllowOnly
        && !policy
            .fs
            .read
            .allow
            .iter()
            .any(|a| paths::within(a, &p) || paths::pattern_matches(a, &p))
    {
        return ReadClass::NotInAllow;
    }
    ReadClass::Allowed
}

/// A scoped write path's check — the admit-level mapping of
/// [`classify_write`]: `OutsideRoot` is `amendable` (a hatch could add the
/// root); the affirmative denials are `refused`.
fn check_write_path(policy: &ContainmentPolicy, path: &str) -> AdmitVerdict {
    match classify_write(policy, path) {
        WriteClass::Inside => AdmitVerdict::Admitted,
        WriteClass::OutsideRoot => AdmitVerdict::Amendable {
            diff: ContainmentDiff::AddWritableRoot {
                root: {
                    let p = paths::normalize_path(path);
                    if p.is_empty() {
                        "*".to_string()
                    } else {
                        p
                    }
                },
            },
        },
        WriteClass::Protected(name) => AdmitVerdict::Refused {
            reason: RefusedReason::ProtectedPath { name },
        },
        WriteClass::ReadOnlySubpath(path) => AdmitVerdict::Refused {
            reason: RefusedReason::ReadOnlySubpath { path },
        },
        WriteClass::Denied(rule) => AdmitVerdict::Refused {
            reason: RefusedReason::DenyRule { rule },
        },
    }
}

fn admit_fs_write(policy: &ContainmentPolicy, input: &AdmitInput) -> AdmitVerdict {
    if policy.fs.write.allow.is_empty() {
        // The write class is not admitted at all — the hatch could add a
        // root, but at Stage 1 the verdict denies either way; `amendable`
        // records the diff (`*` when the scope is unknown).
        return AdmitVerdict::Amendable {
            diff: ContainmentDiff::AddWritableRoot {
                root: input
                    .fs_paths
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "*".to_string()),
            },
        };
    }
    for path in &input.fs_paths {
        let v = check_write_path(policy, path);
        if v.tag() != "admitted" {
            return v;
        }
    }
    AdmitVerdict::Admitted
}

fn admit_fs_read(policy: &ContainmentPolicy, input: &AdmitInput) -> AdmitVerdict {
    if policy.fs.read.mode == ReadMode::AllowOnly && input.scope_unknown {
        // `allow_only` needs positive membership — an unknown scope can
        // never be verified inside it.
        return AdmitVerdict::Refused {
            reason: RefusedReason::Unverifiable,
        };
    }
    for path in &input.fs_paths {
        match classify_read(policy, path) {
            ReadClass::Allowed => {}
            ReadClass::Denied => {
                return AdmitVerdict::Refused {
                    reason: RefusedReason::DenyRule {
                        rule: paths::normalize_path(path),
                    },
                }
            }
            ReadClass::NotInAllow => {
                return AdmitVerdict::Amendable {
                    diff: ContainmentDiff::AddReadAllow {
                        path: paths::normalize_path(path),
                    },
                }
            }
        }
    }
    AdmitVerdict::Admitted
}

fn admit_exec(policy: &ContainmentPolicy, input: &AdmitInput) -> AdmitVerdict {
    match &policy.fs.exec {
        ExecPolicy::Any => AdmitVerdict::Admitted,
        ExecPolicy::Allow(set) => {
            for path in &input.fs_paths {
                let p = paths::normalize_path(path);
                if !set
                    .iter()
                    .any(|a| paths::within(a, &p) || paths::pattern_matches(a, &p))
                {
                    return AdmitVerdict::Amendable {
                        diff: ContainmentDiff::AddExecAllow { path: p },
                    };
                }
            }
            if input.scope_unknown && set.is_empty() {
                return AdmitVerdict::Amendable {
                    diff: ContainmentDiff::AddExecAllow {
                        path: "*".to_string(),
                    },
                };
            }
            AdmitVerdict::Admitted
        }
    }
}

/// `PermissionUnreachable` — the seal/attach-time warning (AC-R-2.8.4-14's
/// third half): a grant whose domain this policy can never admit.
#[derive(Debug, Clone, PartialEq)]
pub struct UnreachableGrant {
    /// The permission/semantic id the grant came from.
    pub permission: String,
    /// The unreachable domain.
    pub domain: EffectDomain,
    /// The closed reason tag.
    pub reason: &'static str,
}

/// `unreachable_permissions(policy, grants)` — the `PermissionUnreachable`
/// check over `(permission_id, grant)` pairs.
pub fn unreachable_permissions(
    policy: &ContainmentPolicy,
    grants: &[(String, Grant)],
) -> Vec<UnreachableGrant> {
    let mut out = Vec::new();
    for (perm, g) in grants {
        let reason = match g.effect.domain {
            EffectDomain::NetEgress if policy.net.mode == NetMode::None => Some("mode_none"),
            EffectDomain::FsWrite if policy.fs.write.allow.is_empty() => Some("no_writable_roots"),
            EffectDomain::Exec | EffectDomain::SpawnProcess if matches!(&policy.fs.exec, ExecPolicy::Allow(s) if s.is_empty()) => {
                Some("no_exec_allow")
            }
            EffectDomain::FsRead
                if policy.fs.read.mode == ReadMode::AllowOnly
                    && policy.fs.read.allow.is_empty() =>
            {
                Some("no_read_allow")
            }
            EffectDomain::SecretAccess
                if policy.net.mode == NetMode::None
                    && policy.fs.read.mode == ReadMode::AllowOnly
                    && policy.fs.read.allow.is_empty() =>
            {
                Some("closed_fs_and_net")
            }
            _ => None,
        };
        if let Some(reason) = reason {
            out.push(UnreachableGrant {
                permission: perm.clone(),
                domain: g.effect.domain,
                reason,
            });
        }
    }
    out
}

// ── the authorize precondition (R-2.8.4 → R-2.8.1) ────────────────────────────

/// The recorded floor verdict the kernel stamps on every `Proposal`
/// (`Proposal.containment`; ADR-0062 D3) — computed at resolve from the
/// effective policy, the attach `ContainmentReport` and the proposal's
/// canonical arguments; `authorize` consumes it **before any check**.
///
/// - `report = None` — helper absent / backend unsupported (attach
///   produced nothing): `Unverified{attach}` for every domain. At Stage 1
///   an absent report refuses all proposals, including `read_only`-class
///   domains — the I-C4 read-only exemption needs the post-step-3 risk
///   class, which does not exist at precondition time (ADR-0243 D4);
/// - a stale report (`policy_version_id` mismatch) ⇒ `Unverified{report}`;
/// - `unknown` evidence on a group the domain requires ⇒
///   `Unverified{group}`;
/// - `admits` ⇒ `Clear` / `Denied{refused:<reason>|amendable:<diff>}` —
///   `amendable` denies like `refused` at Stage 1 (the `amend()` op is
///   Stage 2), recorded distinctly so the amendment path is
///   reconstructible.
pub fn floor_gate(
    policy: &ContainmentPolicy,
    report: Option<&ContainmentReport>,
    input: &AdmitInput,
) -> ContainmentGate {
    let Some(report) = report else {
        return ContainmentGate::Unverified {
            group: "attach".to_string(),
        };
    };
    if verify_report(report, &policy.version_id).is_err() {
        return ContainmentGate::Unverified {
            group: "report".to_string(),
        };
    }
    if let Some(domain) = input.domain {
        for g in required_field_groups(domain) {
            if report.evidence(*g) == EnforcementEvidence::Unknown {
                return ContainmentGate::Unverified {
                    group: g.as_str().to_string(),
                };
            }
        }
    }
    match admits(policy, input) {
        AdmitVerdict::Admitted => ContainmentGate::Clear,
        AdmitVerdict::Refused { reason } => ContainmentGate::Denied {
            detail: format!("refused:{}", reason.as_str()),
        },
        AdmitVerdict::Amendable { diff } => ContainmentGate::Denied {
            detail: format!("amendable:{}", diff.as_str()),
        },
    }
}

/// Derive the [`AdmitInput`] from the monitor's canonical-args record —
/// the scope-bound `fs_path`/`host` values the capability's
/// `scope_bindings` select (I-H5), plus `scope_unknown`.
pub fn admit_input(
    domain: EffectDomain,
    bindings: &ScopeBindings,
    canonical: &args::CanonicalArgs,
) -> AdmitInput {
    let mut fs_paths = Vec::new();
    let mut hosts = Vec::new();
    if let Some(rows) = args::scope_bindings(bindings) {
        for b in rows {
            let Some(v) = canonical
                .scoped
                .get(&b.param_path)
                .and_then(hh_wire::json::Json::as_str)
            else {
                continue;
            };
            match b.scope_kind {
                ScopeKind::FsPath => fs_paths.push(v.to_string()),
                ScopeKind::Host => hosts.push(v.to_string()),
                _ => {}
            }
        }
    }
    AdmitInput {
        domain: Some(domain),
        fs_paths,
        hosts,
        scope_unknown: canonical.scope_unknown,
    }
}

/// The `inside_writable_roots`/`workspace_local` assessor input — I-C3's
/// `kernel_assessed` feed: `yes` only when every path sits inside a
/// writable root **and** `net.mode = none` (or the effect is read-only);
/// anything else is `external`-scope (`no`). The closed check is
/// verifiable, so the answer is never `unknown` here — a path that
/// escapes is `no`, not `unknown`.
pub fn workspace_scope(policy: &ContainmentPolicy, paths: &[String], read_only: bool) -> Tri {
    if crate::meet::paths_within_writable_roots(policy, paths)
        && (policy.net.mode == NetMode::None || read_only)
    {
        Tri::Yes
    } else {
        Tri::No
    }
}
