//! `effective(layers[])` — the authority-ordered layered meet (§5g.4 §2.1;
//! ADR-0060 D3). The effective policy is the **floor**: start from the
//! kernel default; apply each layer in authority order (`kernel >
//! definition > principal > delegate > environment > external >
//! unverified`); a layer may *narrow* any field, but may *loosen* only what
//! its class is entitled to.
//!
//! ## The Stage-1 entitlement table (ADR-0243)
//!
//! | field | minimum class that may loosen |
//! |---|---|
//! | `net.mode` (none → mediated → public) | `principal` (N4's residual + authority checks still apply to the result) |
//! | `net.rules` `allow` additions | `principal` (exact class, plus `kernel`); `definition` only **with `justification`** — a sealed rule must be justified (per-class, not `≥ principal`) |
//! | `fs.write.allow` (WritableRoots) | `principal` |
//! | `amendment.strict` → `false` | `principal` |
//! | `fs.mounts` additions | `definition` |
//! | `resources.*` bound raises/removals | `definition` |
//! | **everything else** (`exec.allow`, `fs.read.allow`/`allow_within_deny`, `non_public_destinations`, `local_binding`, `unix_sockets`, `dns.resolver`, `upstream_proxy`, `methods_default`, `isolation_class`↓, `unshare`↓, `env` loosening, `syscall_filter` removal, `no_new_privs`/`die_with_parent`/`tls.terminate`/`local_binding` off-or-on toward looser, `allowed_bases`/`session_cache`/`persist_scope_ceiling`, `inspect_hooks`) | `kernel` |
//!
//! Narrowing is always permitted and needs no entitlement: deny-kind sets
//! (`read.deny`, `deny_within_allow`, `net.rules` `deny` entries,
//! `protected_metadata`, `env.exclude`, `unshare`) only ever **grow** (union
//! — a deny survives every lower layer); allow-kind sets **intersect** (a
//! layer's omission removes the entry); booleans move only toward the
//! stricter value for unentitled layers; `residual_channels` is union —
//! declarations never drop.

use std::collections::BTreeSet;

use hh_provenance::{AuthorityClass, ProvenanceRecord};

use crate::paths;
use crate::policy::{
    persistence_rank, AmendmentPolicy, ContainmentPolicy, EgressRule, EnvInherit, ExecPolicy,
    FsPolicy, Mount, NetPolicy, PolicyError, ProcPolicy, ReadMode, ResidualChannel, ResourceLimits,
    RuleDecision, WritableRoot, KERNEL_PROTECTED,
};

/// `ContainmentWidening` — a layer attempted to loosen a field its class is
/// not entitled to (§5g.4 §2.1; AC-R-2.8.4-9). Typed, deterministic, and
/// *first* — the meet short-circuits on the first unauthorized loosening.
#[derive(Debug, Clone, PartialEq)]
pub struct ContainmentWidening {
    /// The field the layer tried to loosen.
    pub field: &'static str,
    /// The layer's conferred authority.
    pub layer_authority: AuthorityClass,
    /// The minimum authority entitled to loosen the field.
    pub required: AuthorityClass,
    /// What was attempted (`add`, `loosen`, `remove`).
    pub detail: &'static str,
}

impl std::fmt::Display for ContainmentWidening {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ContainmentWidening: {} < {} cannot {} {}",
            self.layer_authority.as_str(),
            self.required.as_str(),
            self.detail,
            self.field
        )
    }
}

/// The `effective` error sum (§5g.4 §2.1: `ContainmentWidening`,
/// `ProtectedPathExemption`).
#[derive(Debug, Clone, PartialEq)]
pub enum MeetError {
    /// An unauthorized loosening.
    Widening(ContainmentWidening),
    /// A layer dropped a `KERNEL_PROTECTED` name from `protected_metadata`
    /// (I-C2 — the exemption attempt is the error, even though union
    /// semantics would mask it).
    ProtectedPathExemption {
        /// The exempted kernel name.
        name: String,
    },
    /// A layer is not a structurally valid policy (N2/N3/inert members).
    InvalidLayer {
        /// The layer's authority (for the audit record).
        layer_authority: AuthorityClass,
        /// The validation error.
        error: PolicyError,
    },
    /// The meet produced an invalid effective policy (e.g. `public` with no
    /// residual).
    InvalidEffective(PolicyError),
}

impl std::fmt::Display for MeetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MeetError::Widening(w) => write!(f, "{w}"),
            MeetError::ProtectedPathExemption { name } => {
                write!(f, "ProtectedPathExemption: {name}")
            }
            MeetError::InvalidLayer { error, .. } => write!(f, "invalid layer: {error}"),
            MeetError::InvalidEffective(e) => write!(f, "invalid effective policy: {e}"),
        }
    }
}

impl std::error::Error for MeetError {}

fn widen(
    field: &'static str,
    layer_authority: AuthorityClass,
    required: AuthorityClass,
    detail: &'static str,
) -> MeetError {
    MeetError::Widening(ContainmentWidening {
        field,
        layer_authority,
        required,
        detail,
    })
}

fn check(
    layer_authority: AuthorityClass,
    required: AuthorityClass,
    field: &'static str,
    detail: &'static str,
) -> Result<(), MeetError> {
    if layer_authority >= required {
        Ok(())
    } else {
        Err(widen(field, layer_authority, required, detail))
    }
}

// ── set helpers ───────────────────────────────────────────────────────────────

/// Allow-kind meet: `acc` (the running set) ⊓ `layer` (the layer's declared
/// set). Unentitled layers intersect only (an omission removes the entry — a
/// narrowing). An entitled layer's declared set is authoritative for the
/// field (its additions survive; its omissions still narrow).
fn meet_set<T: Clone + Ord>(
    acc: &BTreeSet<T>,
    layer: &BTreeSet<T>,
    layer_authority: AuthorityClass,
    required: AuthorityClass,
    field: &'static str,
) -> Result<BTreeSet<T>, MeetError> {
    if layer.difference(acc).next().is_some() {
        check(layer_authority, required, field, "add")?;
    }
    if layer_authority >= required {
        Ok(layer.clone())
    } else {
        Ok(acc.intersection(layer).cloned().collect())
    }
}

/// Deny-kind meet: union — a deny entry survives every lower layer (I-C1).
fn union_set<T: Clone + Ord>(acc: &BTreeSet<T>, layer: &BTreeSet<T>) -> BTreeSet<T> {
    acc.union(layer).cloned().collect()
}

/// Bool meet: `strict`/`no_new_privs`/`die_with_parent`/`local_binding`-style
/// — moving toward `looser` needs `required`; toward stricter is free.
fn meet_bool(
    acc: bool,
    layer: bool,
    strict_when: bool,
    layer_authority: AuthorityClass,
    required: AuthorityClass,
    field: &'static str,
) -> Result<bool, MeetError> {
    if acc == layer {
        Ok(acc)
    } else if layer == strict_when {
        Ok(strict_when) // narrowing is free
    } else {
        check(layer_authority, required, field, "loosen")?;
        Ok(layer)
    }
}

// ── the meet ──────────────────────────────────────────────────────────────────

/// `effective(layers[])` — the layered meet over the kernel default. `layers`
/// are re-sorted by `provenance.authority` descending (stable — the meet is
/// a pure function of the layer set). `at` is the logical seq stamped on the
/// derived record's kernel provenance.
///
/// Errors: `ContainmentWidening` (unauthorized loosening — first one wins),
/// `ProtectedPathExemption` (a layer's `protected_metadata` misses a
/// `KERNEL_PROTECTED` name), `InvalidLayer` (structurally invalid layer).
pub fn effective(layers: &[ContainmentPolicy], at: u64) -> Result<ContainmentPolicy, MeetError> {
    let mut acc = crate::policy::kernel_default(at);
    let mut ordered: Vec<&ContainmentPolicy> = layers.iter().collect();
    ordered.sort_by(|a, b| b.provenance.authority.cmp(&a.provenance.authority));
    for layer in ordered {
        let a = layer.provenance.authority;
        // Structural validity (N2/N3/inert members/KERNEL_PROTECTED) — the
        // authority-dependent N4 belongs to the meet (a delegate declaring
        // `public` is a ContainmentWidening, not a document error).
        for name in KERNEL_PROTECTED {
            if !layer.fs.protected_metadata.contains(*name) {
                return Err(MeetError::ProtectedPathExemption {
                    name: (*name).to_string(),
                });
            }
        }
        layer
            .validate_structure()
            .map_err(|error| MeetError::InvalidLayer {
                layer_authority: a,
                error,
            })?;
        meet_fs(&mut acc.fs, &layer.fs, a)?;
        meet_net(&mut acc.net, &layer.net, a)?;
        meet_proc(&mut acc.proc, &layer.proc, a)?;
        meet_resources(&mut acc.resources, &layer.resources, a)?;
        meet_amendment(&mut acc.amendment, &layer.amendment, a)?;
        meet_residuals(&mut acc.residual_channels, &layer.residual_channels);
        for (k, v) in &layer.ext {
            acc.ext.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }
    acc.provenance = ProvenanceRecord::kernel("hh-containment/effective", at);
    acc.compute_ids();
    acc.validate().map_err(MeetError::InvalidEffective)?;
    Ok(acc)
}

fn meet_fs(acc: &mut FsPolicy, layer: &FsPolicy, a: AuthorityClass) -> Result<(), MeetError> {
    // read.mode — allow_all_except → allow_only narrows (the layer's `allow`
    // list becomes the extent; the readable set `allow − deny` is always ⊆
    // `everything − deny`). The reverse loosens (kernel only).
    match (acc.read.mode, layer.read.mode) {
        (ReadMode::AllowAllExcept, ReadMode::AllowOnly) => {
            acc.read.mode = ReadMode::AllowOnly;
            acc.read.allow = layer.read.allow.clone();
        }
        (ReadMode::AllowOnly, ReadMode::AllowAllExcept) => {
            check(a, AuthorityClass::Kernel, "fs.read.mode", "loosen")?;
            acc.read.mode = ReadMode::AllowAllExcept;
        }
        _ => {}
    }
    // read.allow (allow_only extent): intersect / entitled replace.
    if acc.read.mode == ReadMode::AllowOnly {
        acc.read.allow = meet_set(
            &acc.read.allow.iter().cloned().collect(),
            &layer.read.allow.iter().cloned().collect(),
            a,
            AuthorityClass::Kernel,
            "fs.read.allow",
        )?
        .into_iter()
        .collect();
    } else if !layer.read.allow.is_empty() {
        // An allow list under allow_all_except is inert in the layer —
        // validate_structure already refuses it.
        acc.read.allow = vec![];
    }
    acc.read.deny = union_set(
        &acc.read.deny.iter().cloned().collect(),
        &layer.read.deny.iter().cloned().collect(),
    )
    .into_iter()
    .collect();
    acc.read.allow_within_deny = meet_set(
        &acc.read.allow_within_deny.iter().cloned().collect(),
        &layer.read.allow_within_deny.iter().cloned().collect(),
        a,
        AuthorityClass::Kernel,
        "fs.read.allow_within_deny",
    )?
    .into_iter()
    .collect();

    // write.allow — WritableRoots keyed by root; subpath/protected lists are
    // deny-kind and union within a surviving root.
    let acc_roots: BTreeSet<String> = acc.write.allow.iter().map(|w| w.root.clone()).collect();
    let layer_roots: BTreeSet<String> = layer.write.allow.iter().map(|w| w.root.clone()).collect();
    if !layer_roots
        .difference(&acc_roots)
        .collect::<Vec<_>>()
        .is_empty()
    {
        check(a, AuthorityClass::Principal, "fs.write.allow", "add")?;
    }
    let keep: BTreeSet<String> = if a >= AuthorityClass::Principal {
        layer_roots.clone()
    } else {
        acc_roots.intersection(&layer_roots).cloned().collect()
    };
    let mut merged: Vec<WritableRoot> = Vec::new();
    for root in &keep {
        let mut w = acc
            .write
            .allow
            .iter()
            .find(|w| &w.root == root)
            .cloned()
            .unwrap_or(WritableRoot {
                root: root.clone(),
                read_only_subpaths: vec![],
                protected_metadata_names: vec![],
            });
        if let Some(lw) = layer.write.allow.iter().find(|w| &w.root == root) {
            w.read_only_subpaths = union_set(
                &w.read_only_subpaths.iter().cloned().collect(),
                &lw.read_only_subpaths.iter().cloned().collect(),
            )
            .into_iter()
            .collect();
            w.protected_metadata_names = union_set(
                &w.protected_metadata_names.iter().cloned().collect(),
                &lw.protected_metadata_names.iter().cloned().collect(),
            )
            .into_iter()
            .collect();
        }
        merged.push(w);
    }
    acc.write.allow = merged;
    acc.write.deny_within_allow = union_set(
        &acc.write.deny_within_allow.iter().cloned().collect(),
        &layer.write.deny_within_allow.iter().cloned().collect(),
    )
    .into_iter()
    .collect();

    // exec — `any` → `allow(set)` narrows; `allow` growth or →`any` loosens
    // (kernel only).
    acc.exec = match (&acc.exec, &layer.exec) {
        (ExecPolicy::Any, ExecPolicy::Allow(set)) => ExecPolicy::Allow(set.clone()),
        (ExecPolicy::Allow(_), ExecPolicy::Any) => {
            check(a, AuthorityClass::Kernel, "fs.exec", "loosen")?;
            ExecPolicy::Any
        }
        (ExecPolicy::Allow(acc_set), ExecPolicy::Allow(layer_set)) => ExecPolicy::Allow(
            meet_set(
                &acc_set.iter().cloned().collect(),
                &layer_set.iter().cloned().collect(),
                a,
                AuthorityClass::Kernel,
                "fs.exec.allow",
            )?
            .into_iter()
            .collect(),
        ),
        (ExecPolicy::Any, ExecPolicy::Any) => ExecPolicy::Any,
    };

    acc.protected_metadata = union_set(&acc.protected_metadata, &layer.protected_metadata);

    // mounts — additions are definition-entitled; removals narrow.
    let acc_m: BTreeSet<String> = acc.mounts.iter().map(mount_key).collect();
    let layer_m: BTreeSet<String> = layer.mounts.iter().map(mount_key).collect();
    if layer_m.difference(&acc_m).next().is_some() {
        check(a, AuthorityClass::Definition, "fs.mounts", "add")?;
    }
    let keep: BTreeSet<String> = if a >= AuthorityClass::Definition {
        acc_m.union(&layer_m).cloned().collect()
    } else {
        acc_m.intersection(&layer_m).cloned().collect()
    };
    let mut mounts: Vec<Mount> = acc
        .mounts
        .iter()
        .filter(|m| keep.contains(&mount_key(m)))
        .cloned()
        .collect();
    for m in &layer.mounts {
        let k = mount_key(m);
        if keep.contains(&k) && !mounts.iter().any(|x| mount_key(x) == k) {
            mounts.push(m.clone());
        }
    }
    acc.mounts = mounts;
    Ok(())
}

fn mount_key(m: &Mount) -> String {
    let src = match &m.source {
        crate::policy::MountSource::Content(c) => format!("content:{c}"),
        crate::policy::MountSource::HostPath(h) => format!("host_path:{h}"),
    };
    format!("{src}@{}", paths::normalize_path(&m.target))
}

fn rule_key(r: &EgressRule) -> String {
    // The constraint tuple — justification/provenance never key a rule.
    let mut ports = r.ports.clone();
    ports.sort_unstable();
    let mut protos: Vec<&'static str> = r.protocols.iter().map(|p| p.as_str()).collect();
    protos.sort_unstable();
    let mut methods = r.methods.clone();
    methods.sort_unstable();
    let mut creds = r.credential_bindings.clone();
    creds.sort_unstable();
    format!(
        "{}|{:?}|{}|{}|{}",
        r.host.spelling(),
        ports,
        protos.join(","),
        methods.join(","),
        creds.join(",")
    )
}

fn meet_net(acc: &mut NetPolicy, layer: &NetPolicy, a: AuthorityClass) -> Result<(), MeetError> {
    // mode — none < mediated < public; loosening is principal-entitled (N4's
    // residual requirement is checked on the *result* by validate).
    if layer.mode > acc.mode {
        check(a, AuthorityClass::Principal, "net.mode", "loosen")?;
        acc.mode = layer.mode;
    } else if layer.mode < acc.mode {
        acc.mode = layer.mode; // narrowing is free
    }

    // rules — deny-kind entries union; allow-kind entries intersect /
    // entitled-add (definition additions need justification).
    let acc_deny: BTreeSet<String> = acc
        .rules
        .iter()
        .filter(|r| r.decision == RuleDecision::Deny)
        .map(rule_key)
        .collect();
    let layer_deny: BTreeSet<String> = layer
        .rules
        .iter()
        .filter(|r| r.decision == RuleDecision::Deny)
        .map(rule_key)
        .collect();
    let deny_keep: BTreeSet<String> = acc_deny.union(&layer_deny).cloned().collect();
    let mut rules: Vec<EgressRule> = acc
        .rules
        .iter()
        .filter(|r| r.decision == RuleDecision::Deny && deny_keep.contains(&rule_key(r)))
        .cloned()
        .collect();
    for r in &layer.rules {
        if r.decision == RuleDecision::Deny && !rules.iter().any(|x| rule_key(x) == rule_key(r)) {
            rules.push(r.clone());
        }
    }
    let acc_allow: BTreeSet<String> = acc
        .rules
        .iter()
        .filter(|r| r.decision == RuleDecision::Allow)
        .map(rule_key)
        .collect();
    let layer_allow: BTreeSet<String> = layer
        .rules
        .iter()
        .filter(|r| r.decision == RuleDecision::Allow)
        .map(rule_key)
        .collect();
    for r in layer
        .rules
        .iter()
        .filter(|r| r.decision == RuleDecision::Allow)
    {
        if !acc_allow.contains(&rule_key(r)) {
            // An allow addition (§5g.4 §2.1): `kernel` and `principal` are
            // entitled outright; `definition` is entitled only when the
            // rule carries its `justification` — a sealed definition's
            // allow rule must be justified. `definition` outranks
            // `principal` for every other listed field, not for this one:
            // the entitlement is per-class, so `a >= principal` must NOT
            // be used here (it would dead-code the justification clause).
            let entitled = match a {
                AuthorityClass::Kernel | AuthorityClass::Principal => true,
                AuthorityClass::Definition => r.justification.is_some(),
                _ => false,
            };
            if !entitled {
                return Err(widen(
                    "net.rules.allow",
                    a,
                    AuthorityClass::Principal,
                    "add",
                ));
            }
            rules.push(r.clone());
        }
    }
    // A layer's omission of an acc allow rule narrows — intersect.
    let allow_keep: BTreeSet<String> = acc_allow.intersection(&layer_allow).cloned().collect();
    for r in &acc.rules {
        if r.decision == RuleDecision::Allow && allow_keep.contains(&rule_key(r)) {
            rules.push(r.clone());
        }
    }
    acc.rules = rules;

    // The remaining net knobs are kernel-only loosenings at Stage 1
    // (ADR-0243's conservative table).
    acc.default_unmatched = match (acc.default_unmatched, layer.default_unmatched) {
        (crate::policy::DefaultUnmatched::Deny, crate::policy::DefaultUnmatched::Ask) => {
            check(a, AuthorityClass::Kernel, "net.default_unmatched", "loosen")?;
            crate::policy::DefaultUnmatched::Ask
        }
        (_, crate::policy::DefaultUnmatched::Deny) => crate::policy::DefaultUnmatched::Deny,
        (d, crate::policy::DefaultUnmatched::Ask) => d,
    };
    acc.non_public_destinations = match (acc.non_public_destinations, layer.non_public_destinations)
    {
        (crate::policy::NonPublic::Deny, crate::policy::NonPublic::AllowListed) => {
            check(
                a,
                AuthorityClass::Kernel,
                "net.non_public_destinations",
                "loosen",
            )?;
            crate::policy::NonPublic::AllowListed
        }
        (_, crate::policy::NonPublic::Deny) => crate::policy::NonPublic::Deny,
        (d, crate::policy::NonPublic::AllowListed) => d,
    };
    acc.local_binding = meet_bool(
        acc.local_binding,
        layer.local_binding,
        false,
        a,
        AuthorityClass::Kernel,
        "net.local_binding",
    )?;
    // unix_sockets — mode + allow are kernel-only loosenings.
    if layer.unix_sockets.mode == crate::policy::UnixSocketMode::AllowListed
        && acc.unix_sockets.mode == crate::policy::UnixSocketMode::BridgedOnly
    {
        check(a, AuthorityClass::Kernel, "net.unix_sockets.mode", "loosen")?;
        acc.unix_sockets.mode = crate::policy::UnixSocketMode::AllowListed;
    }
    acc.unix_sockets.allow = meet_set(
        &acc.unix_sockets.allow.iter().cloned().collect(),
        &layer.unix_sockets.allow.iter().cloned().collect(),
        a,
        AuthorityClass::Kernel,
        "net.unix_sockets.allow",
    )?
    .into_iter()
    .collect();
    // dns.resolver — loosening is kernel-only.
    let rank = |r: &crate::policy::DnsResolver| match r {
        crate::policy::DnsResolver::None => 0,
        crate::policy::DnsResolver::ListedNameservers(_) => 1,
        crate::policy::DnsResolver::Mediator => 2,
    };
    match (rank(&acc.dns.resolver), rank(&layer.dns.resolver)) {
        (ar, lr) if lr > ar => {
            check(a, AuthorityClass::Kernel, "net.dns.resolver", "loosen")?;
            acc.dns.resolver = layer.dns.resolver.clone();
        }
        (ar, lr) if lr < ar => {
            acc.dns.resolver = layer.dns.resolver.clone();
        }
        _ => {
            if let (
                crate::policy::DnsResolver::ListedNameservers(acc_ns),
                crate::policy::DnsResolver::ListedNameservers(layer_ns),
            ) = (&acc.dns.resolver, &layer.dns.resolver)
            {
                acc.dns.resolver = crate::policy::DnsResolver::ListedNameservers(
                    meet_set(
                        &acc_ns.iter().cloned().collect(),
                        &layer_ns.iter().cloned().collect(),
                        a,
                        AuthorityClass::Kernel,
                        "net.dns.resolver.nameservers",
                    )?
                    .into_iter()
                    .collect(),
                );
            }
        }
    }
    acc.tls.terminate = meet_bool(
        acc.tls.terminate,
        layer.tls.terminate,
        false,
        a,
        AuthorityClass::Kernel,
        "net.tls.terminate",
    )?;
    acc.tls.inspect_hooks = meet_set(
        &acc.tls.inspect_hooks.iter().cloned().collect(),
        &layer.tls.inspect_hooks.iter().cloned().collect(),
        a,
        AuthorityClass::Kernel,
        "net.tls.inspect_hooks",
    )?
    .into_iter()
    .collect();
    match (&acc.upstream_proxy, &layer.upstream_proxy) {
        (None, Some(_)) => {
            check(a, AuthorityClass::Kernel, "net.upstream_proxy", "add")?;
            acc.upstream_proxy = layer.upstream_proxy.clone();
        }
        (Some(_), Some(_)) if acc.upstream_proxy != layer.upstream_proxy => {
            check(a, AuthorityClass::Kernel, "net.upstream_proxy", "change")?;
            acc.upstream_proxy = layer.upstream_proxy.clone();
        }
        (Some(_), None) => {} // omission narrows — keep acc's (a proxy removal is narrowing? no: removing the proxy claim removes a mechanism — keep acc's value; the proxy is a declared loss field)
        _ => {}
    }
    acc.methods_default = meet_set(
        &acc.methods_default.iter().cloned().collect(),
        &layer.methods_default.iter().cloned().collect(),
        a,
        AuthorityClass::Kernel,
        "net.methods_default",
    )?
    .into_iter()
    .collect();
    Ok(())
}

fn meet_proc(acc: &mut ProcPolicy, layer: &ProcPolicy, a: AuthorityClass) -> Result<(), MeetError> {
    if layer.isolation_class.strength() > acc.isolation_class.strength() {
        acc.isolation_class = layer.isolation_class; // stronger = narrower
    } else if layer.isolation_class.strength() < acc.isolation_class.strength() {
        check(a, AuthorityClass::Kernel, "proc.isolation_class", "loosen")?;
        acc.isolation_class = layer.isolation_class;
    }
    acc.unshare = union_set(&acc.unshare, &layer.unshare);
    // A smaller unshare set is a loosening — union never shrinks, so a layer
    // cannot remove one; the entitlement question never arises.
    acc.no_new_privs = meet_bool(
        acc.no_new_privs,
        layer.no_new_privs,
        true,
        a,
        AuthorityClass::Kernel,
        "proc.no_new_privs",
    )?;
    match (&acc.syscall_filter, &layer.syscall_filter) {
        (None, Some(_)) => acc.syscall_filter = layer.syscall_filter.clone(), // adding a filter narrows
        (Some(x), Some(y)) if x != y => {
            check(a, AuthorityClass::Kernel, "proc.syscall_filter", "change")?;
            acc.syscall_filter = layer.syscall_filter.clone();
        }
        // `None` is no declaration — the filter survives a silent layer.
        (Some(_), None) => {}
        _ => {}
    }
    acc.max_processes = match (acc.max_processes, layer.max_processes) {
        (None, Some(v)) => Some(v),                 // adding a bound narrows
        (Some(cur), Some(v)) if v < cur => Some(v), // tighter
        (Some(_), Some(v)) => {
            check(a, AuthorityClass::Kernel, "proc.max_processes", "raise")?;
            Some(v)
        }
        // `None` is no declaration — the bound survives a silent layer.
        (Some(_), None) => acc.max_processes,
        (None, None) => None,
    };
    // env — `inherit` toward `none` narrows; toward `all` loosens (kernel).
    let irank = |i: EnvInherit| match i {
        EnvInherit::None => 0,
        EnvInherit::Core => 1,
        EnvInherit::All => 2,
    };
    if irank(layer.env.inherit) > irank(acc.env.inherit) {
        check(a, AuthorityClass::Kernel, "proc.env.inherit", "loosen")?;
        acc.env.inherit = layer.env.inherit;
    } else if irank(layer.env.inherit) < irank(acc.env.inherit) {
        acc.env.inherit = layer.env.inherit;
    }
    acc.env.exclude = union_set(
        &acc.env.exclude.iter().cloned().collect(),
        &layer.env.exclude.iter().cloned().collect(),
    )
    .into_iter()
    .collect();
    for (k, v) in &layer.env.set {
        if let Some(cur) = acc.env.set.get(k) {
            if cur != v {
                check(a, AuthorityClass::Kernel, "proc.env.set", "change")?;
            }
        } else {
            check(a, AuthorityClass::Kernel, "proc.env.set", "add")?;
        }
        acc.env.set.insert(k.clone(), v.clone());
    }
    // include_only — intersection narrows; a layer with an empty list keeps
    // acc's (an absent list is no constraint, not a removal).
    if !layer.env.include_only.is_empty() {
        acc.env.include_only = if acc.env.include_only.is_empty() {
            layer.env.include_only.clone()
        } else {
            acc.env
                .include_only
                .iter()
                .filter(|n| layer.env.include_only.contains(n))
                .cloned()
                .collect()
        };
    }
    acc.die_with_parent = meet_bool(
        acc.die_with_parent,
        layer.die_with_parent,
        true,
        a,
        AuthorityClass::Kernel,
        "proc.die_with_parent",
    )?;
    Ok(())
}

fn meet_resources(
    acc: &mut ResourceLimits,
    layer: &ResourceLimits,
    a: AuthorityClass,
) -> Result<(), MeetError> {
    // Lower bound = narrow; raise = definition-entitled; `None` on a layer
    // is no declaration — bounds survive silence (deny-kind).
    fn bound(
        acc: &mut Option<u64>,
        layer: Option<u64>,
        a: AuthorityClass,
        field: &'static str,
    ) -> Result<(), MeetError> {
        match (*acc, layer) {
            (None, Some(v)) => *acc = Some(v),
            (Some(cur), Some(v)) => {
                if v < cur {
                    *acc = Some(v);
                } else if v > cur {
                    check(a, AuthorityClass::Definition, field, "raise")?;
                    *acc = Some(v);
                }
            }
            // `None` is *no declaration*, not a removal — a bound survives
            // a silent layer like a deny entry (removal is expressible only
            // through the Stage-2 `amend()` hatch, not a layer).
            (Some(_), None) => {}
            (None, None) => {}
        }
        Ok(())
    }
    bound(&mut acc.cpu_ms, layer.cpu_ms, a, "resources.cpu_ms")?;
    bound(
        &mut acc.memory_bytes,
        layer.memory_bytes,
        a,
        "resources.memory_bytes",
    )?;
    bound(
        &mut acc.disk_bytes,
        layer.disk_bytes,
        a,
        "resources.disk_bytes",
    )?;
    bound(&mut acc.pids, layer.pids, a, "resources.pids")?;
    bound(
        &mut acc.open_files,
        layer.open_files,
        a,
        "resources.open_files",
    )?;
    bound(&mut acc.wall_ms, layer.wall_ms, a, "resources.wall_ms")?;
    bound(
        &mut acc.network_bytes_out,
        layer.network_bytes_out,
        a,
        "resources.network_bytes_out",
    )?;
    bound(
        &mut acc.network_calls,
        layer.network_calls,
        a,
        "resources.network_calls",
    )?;
    Ok(())
}

fn meet_amendment(
    acc: &mut AmendmentPolicy,
    layer: &AmendmentPolicy,
    a: AuthorityClass,
) -> Result<(), MeetError> {
    acc.allowed_bases = meet_set(
        &acc.allowed_bases,
        &layer.allowed_bases,
        a,
        AuthorityClass::Kernel,
        "amendment.allowed_bases",
    )?;
    // strict — `false` is the principal entitlement (the one listed
    // loosening); `true` narrows from any layer.
    match (acc.strict, layer.strict) {
        (true, false) => {
            check(a, AuthorityClass::Principal, "amendment.strict", "loosen")?;
            acc.strict = false;
        }
        (false, true) => acc.strict = true, // re-tightening narrows
        _ => {}
    }
    acc.session_cache = match (acc.session_cache, layer.session_cache) {
        (false, true) => {
            check(
                a,
                AuthorityClass::Kernel,
                "amendment.session_cache",
                "loosen",
            )?;
            true
        }
        (true, false) => false,
        (s, _) => s,
    };
    let (ar, lr) = (
        persistence_rank(acc.persist_scope_ceiling),
        persistence_rank(layer.persist_scope_ceiling),
    );
    if lr > ar {
        check(
            a,
            AuthorityClass::Kernel,
            "amendment.persist_scope_ceiling",
            "raise",
        )?;
        acc.persist_scope_ceiling = layer.persist_scope_ceiling;
    } else if lr < ar {
        acc.persist_scope_ceiling = layer.persist_scope_ceiling;
    }
    Ok(())
}

fn meet_residuals(acc: &mut Vec<ResidualChannel>, layer: &[ResidualChannel]) {
    // Declarations never drop — union by (kind, closer, owner).
    let mut keys: BTreeSet<String> = acc
        .iter()
        .map(|r| format!("{}|{}|{}", r.kind.as_str(), r.closer, r.owner))
        .collect();
    for r in layer {
        let k = format!("{}|{}|{}", r.kind.as_str(), r.closer, r.owner);
        if keys.insert(k) {
            acc.push(r.clone());
        }
    }
}

/// Whether every `fs_path`-scoped path lies inside the policy's writable
/// roots — the `kernel_assessed` feed's `paths ⊆ fs.write.allow` half
/// (I-C3). An empty path set is trivially inside.
pub fn paths_within_writable_roots(policy: &ContainmentPolicy, paths: &[String]) -> bool {
    if policy.fs.write.allow.is_empty() {
        return paths.is_empty();
    }
    paths.iter().all(|p| {
        policy
            .fs
            .write
            .allow
            .iter()
            .any(|w| paths::within(&w.root, p))
    })
}
