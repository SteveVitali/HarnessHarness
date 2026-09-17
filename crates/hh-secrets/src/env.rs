//! The environment obligations (§5g.3 §1 env row; ADR-0057): `EnvSpec{allowlist,
//! bindings[]}`, `env_apply`, `env_snapshot`, and the `/proc`-equivalent read —
//! all placeholders-only.
//!
//! Deny-by-default: only names on the `allowlist` are exposed at all, and only
//! when *inheritable* (the kernel-non-inheritable list is withheld even when
//! allow-listed — an allow-list entry cannot re-inherit `AWS_SECRET_ACCESS_KEY`).
//! Bound channels project **placeholders** (`mh_secret:…`), never values — the
//! mediator materialises the value at delivery time on the Stage-2 path; at
//! Stage 1 the projected env *is* the whole story, so an environment dump, a
//! `/proc` read and a persisted snapshot are all value-free by construction
//! (LT-01; SV-4's env sweep is over exactly this projection).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_monitor::assess::SecretTransport;
use hh_wire::json::Json;

use crate::errors::{BrokerError, Refused, RefusedCode};
use crate::redact::detect;
use crate::redact::DetectorSet;
use crate::types::Placeholder;

/// `EnvSpec{allowlist, bindings[]}` (§5g.3 §1) — the environment's declared
/// exposure. The `allowlist` is the *only* names that may appear in the
/// projected env (deny-by-default); `bindings` names the `(env_name → binding)`
/// projections.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvSpec {
    /// The env names that may be carried through (subject to inheritable-ness
    /// and known-value masking).
    pub allowlist: BTreeSet<String>,
    /// The channel→env bindings (`env_name`, `binding_id`) the projection fills
    /// with placeholders.
    pub bindings: Vec<EnvBinding>,
}

/// An env binding — `env_name` receives `binding`'s placeholder. `env_name`
/// must be in the channel's `allowed_env_names` (checked at `bind`) *and* the
/// `EnvSpec.allowlist` (checked at `env_apply` — a binding to a non-allowed name
/// projects nothing; deny-by-default).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvBinding {
    /// The env var name.
    pub env_name: String,
    /// The channel.
    pub channel_id: String,
    /// The binding that backs it (the placeholder is binding-scoped).
    pub binding_id: String,
    /// The delivery mode (`proxy_injected` ⇒ the placeholder is what the
    /// mediator substitutes on the wire).
    pub mode: SecretTransport,
}

/// The kernel-non-inheritable env-name rules (§5g.3 §1: "the
/// kernel-non-inheritable list"). A name is non-inheritable when it is on the
/// closed list *or* matches a secret-suffix rule — even an allow-listed,
/// base-env-present name is withheld rather than carried.
pub const KERNEL_NONINHERITABLE: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "HH_GATEWAY_TOKEN",
    "OPENAI_API_KEY",
];

/// The non-inheritable suffix/prefix rules (a name *shaped* like a credential
/// is never inherited — the list above is the named set; these are the shape
/// rules).
const NONINHERITABLE_SUFFIXES: &[&str] = &[
    "_API_KEY",
    "_TOKEN",
    "_SECRET",
    "_SECRET_KEY",
    "_PASSWORD",
    "_CREDENTIALS",
    "_PRIVATE_KEY",
];

/// Whether `name` may be inherited into a projected env. The kernel's gateway
/// credential variable and anything credential-shaped are non-inheritable —
/// the broker is their only path (§5g.3 §1).
pub fn is_inheritable(name: &str) -> bool {
    if KERNEL_NONINHERITABLE.contains(&name) {
        return false;
    }
    !NONINHERITABLE_SUFFIXES.iter().any(|s| name.ends_with(s))
}

/// The projected environment — the only env a helper ever sees. Values are
/// either allow-listed non-secret values (masked for known values) or
/// `mh_secret:` placeholders; a withheld name is simply absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedEnv {
    /// `name → value-or-placeholder` (sorted — the canonical form).
    pub vars: BTreeMap<String, String>,
    /// Names withheld because they are non-inheritable or not allow-listed
    /// (recorded for the audit detail — names only, never values).
    pub withheld: BTreeSet<String>,
}

impl ProjectedEnv {
    /// The environment-dump form (`KEY=value\n`, sorted) — what an `env`-style
    /// dump inside the handle shows: placeholders only for bound channels.
    pub fn dump(&self) -> String {
        let mut s = String::new();
        for (k, v) in &self.vars {
            s.push_str(k);
            s.push('=');
            s.push_str(v);
            s.push('\n');
        }
        s
    }

    /// The `/proc/<pid>/environ`-equivalent read — NUL-separated `KEY=value`;
    /// the same placeholders-only view (LT-01 names both reads explicitly).
    pub fn proc_env(&self) -> Vec<u8> {
        let mut s = Vec::new();
        for (k, v) in &self.vars {
            s.extend_from_slice(k.as_bytes());
            s.push(b'=');
            s.extend_from_slice(v.as_bytes());
            s.push(0);
        }
        s
    }

    /// The canonical snapshot form — `env_snapshot` retains placeholders
    /// verbatim (a snapshot is a *reference* record; restoring it re-resolves
    /// through the broker, never materialises the snapshot itself).
    pub fn snapshot(&self) -> Json {
        Json::obj([
            (
                "vars",
                Json::Obj(
                    self.vars
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                        .collect(),
                ),
            ),
            (
                "withheld",
                Json::Arr(self.withheld.iter().map(|w| Json::str(w.clone())).collect()),
            ),
        ])
    }
}

/// `env_apply(spec, base_env, broker_view)` — project the env a helper sees.
///
/// - `name ∉ spec.allowlist` → withheld (deny-by-default).
/// - `name ∈ allowlist` but non-inheritable → withheld.
/// - `name ∈ allowlist`, inheritable, present in `base_env` → carried with
///   known-value masking applied (the allow-list masks known values, it does
///   not amplify them — SV-4's env sweep covers this path).
/// - each `binding` whose `env_name ∈ allowlist` → `vars[env_name] =
///   binding.placeholder` (placeholders only, every mode).
///
/// `bindings` here are `(EnvBinding, Placeholder)` pairs the broker produced at
/// `bind` — `env_apply` never resolves a source itself.
pub fn env_apply(
    spec: &EnvSpec,
    base_env: &BTreeMap<String, String>,
    bound_placeholders: &BTreeMap<String, Placeholder>, // env_name → placeholder
    detectors: &DetectorSet,
) -> Result<ProjectedEnv, BrokerError> {
    let mut vars = BTreeMap::new();
    let mut withheld = BTreeSet::new();

    // The allow-listed base-env names — masked, inheritable only.
    for name in &spec.allowlist {
        if let Some(v) = base_env.get(name) {
            if !is_inheritable(name) {
                withheld.insert(name.clone());
                continue;
            }
            // Mask known secret values inside an allowed value (the allow-list
            // masks, it does not amplify).
            let (masked, hits) = crate::redact::redact(v, detectors);
            let _ = hits; // masking marks are the caller's audit detail
            vars.insert(name.clone(), masked);
        }
    }

    // The bound channels project placeholders — never values, any mode.
    for b in &spec.bindings {
        if !spec.allowlist.contains(&b.env_name) {
            withheld.insert(b.env_name.clone());
            continue;
        }
        let ph = bound_placeholders.get(&b.env_name).ok_or_else(|| {
            BrokerError::Refused(Refused::new(
                RefusedCode::NotBindable,
                format!("no live binding for env name {}", b.env_name),
            ))
        })?;
        vars.insert(b.env_name.clone(), ph.spelling.clone());
    }

    Ok(ProjectedEnv { vars, withheld })
}

/// `env_snapshot(projected)` — the persisted form. Placeholders retained
/// verbatim; the withheld names recorded. The snapshot *is* the projected env —
/// there is no second form that could carry a value (SV-4).
pub fn env_snapshot(projected: &ProjectedEnv) -> Json {
    projected.snapshot()
}

/// The SV-4 env sweep — verify a projected env carries no secret material:
/// every `vars` value is either not-a-hit or a placeholder spelling. Returns
/// the offending names (empty = clean).
pub fn env_sweep(projected: &ProjectedEnv, detectors: &DetectorSet) -> Vec<String> {
    let mut bad = Vec::new();
    for (k, v) in &projected.vars {
        if Placeholder::is_placeholder(v) {
            continue;
        }
        if detect(v, detectors)
            .iter()
            .any(|h| h.detector != crate::redact::DetectorKind::PlaceholderPassthrough)
        {
            bad.push(k.clone());
        }
    }
    bad
}
