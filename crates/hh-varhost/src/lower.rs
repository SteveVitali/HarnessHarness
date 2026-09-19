//! `lower_requests` — `manifest.requests` (claims) → the sealed
//! `Permission` + the plugin process's `ContainmentPolicy` (§8.4 §2
//! `lower_requests`; ADR-0180 D2). Claims confer nothing until a principal
//! seals them; this lowering is the `seal`-time half the variant host runs
//! at spawn — the policy it produces is the *only* world the plugin
//! process can touch (V3/V4).
//!
//! Rules (all fail-closed, all honest about loss):
//!
//! - `requests ⊆ cap` first (`RequestsExceedCap` — the `authority_cap`
//!   ceiling the sealing principal narrowed to).
//! - The `fs` extent is exactly `{package root} ∪ requests.fs_roots` under
//!   `allow_only`; `exec` is exactly the contribution executables. A
//!   request naming a path outside the cap or a non-canonical spelling is
//!   a refusal, never a silent drop.
//! - `net.mode` is `none`; the variant's ABI channel socket is the single
//!   `unix_sockets.allow` member (V4 — the one ambient channel it gets).
//!   `requests.egress` cannot lower onto the Stage-2 seatbelt backend —
//!   every entry lands in `losses` (a `mediated`-mode lane is the
//!   container/remote placement's, not this one).
//! - `env_keys` project *values* from the caller-supplied environment only;
//!   undeclared names never enter the child's cleared environment.
//! - `budget_share` lowers onto the Permission's grant constraints and is
//!   recorded — the `ResourceLimits` members a seatbelt lane cannot meter
//!   are named in `losses`, never silently zero.
//! - The `Permission`'s issuer is the sealing principal (`authority ≥
//!   principal`, 08.1 check 1); grants are `delegable: false` — X6.

use std::collections::BTreeSet;

use hh_containment::policy::{
    kernel_default, ContainmentPolicy, EnvInherit, ExecPolicy, ReadMode, UnixSocketMode,
};
use hh_hir::kinds::EffectDomain;
use hh_hir::records::{Grant, GrantConstraints, Issuer, PermissionRecord, Validity};
use hh_hir::refs::Ref;
use hh_registry::extension::plugin::{PluginManifest, Requests};
use hh_wire::json::Json;

/// The lowering failure sum (each maps to the resolution pipeline's typed
/// errors — `RequestsExceedCap` on cap misses, `SchemaViolation`-class on
/// malformed claims).
#[derive(Debug, Clone, PartialEq)]
pub enum LowerError {
    /// A request exceeded the cap (`RequestsExceedCap`).
    CapExceeded(String),
    /// A claim could not be lowered (detail names the member).
    Unrepresentable(String),
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LowerError::CapExceeded(d) => write!(f, "RequestsExceedCap: {d}"),
            LowerError::Unrepresentable(d) => write!(f, "unrepresentable request: {d}"),
        }
    }
}
impl std::error::Error for LowerError {}

/// The lowering product.
#[derive(Debug, Clone)]
pub struct LoweredRequests {
    /// The sealed `Permission` (issuer = the sealing principal).
    pub permission: PermissionRecord,
    /// The plugin process's `ContainmentPolicy` (what the helper enforces).
    pub policy: ContainmentPolicy,
    /// The env projection — `(name, value)` pairs for the cleared child
    /// environment (only `env_keys` ∩ supplied ambient).
    pub env: Vec<(String, String)>,
    /// Lowering losses — every claim that could not be honoured, named
    /// (never silent; §8.4 AC-3's honesty clause).
    pub losses: Vec<String>,
}

/// The lowering context: what the spawn needs beyond the manifest.
pub struct LowerContext<'a> {
    /// The package root (always readable).
    pub package_root: &'a str,
    /// The variant ABI channel socket path (the one unix socket).
    pub channel_socket: &'a str,
    /// The executable paths the contribution pins (`fs.exec.allow`).
    pub exec_paths: &'a [String],
    /// The ambient environment `env_keys` may project from.
    pub ambient_env: &'a [(String, String)],
    /// The sealing principal's identity coordinate (the Permission issuer).
    pub issuer_ref: &'a str,
    /// The holder ref (the plugin's `AgentProcess`-class identity).
    pub holder: Ref,
    /// The logical timestamp the policy is minted at.
    pub at: u64,
}

/// `lower_requests(manifest, cap, ctx)` — claims → `(Permission,
/// ContainmentPolicy, env, losses)` or `RequestsExceedCap`.
pub fn lower_requests(
    manifest: &PluginManifest,
    cap: &Requests,
    ctx: &LowerContext<'_>,
) -> Result<LoweredRequests, LowerError> {
    // Claims ⊆ cap — the authority_cap-side check (typed: RequestsExceedCap).
    manifest
        .requests
        .within_cap(cap)
        .map_err(|e| LowerError::CapExceeded(format!("{e:?}")))?;

    let req = &manifest.requests;
    let mut losses = Vec::new();
    let mut policy = kernel_default(ctx.at);
    policy.policy_id = format!("plugin:{}", manifest.identity.id());
    policy.version_id = format!("{}:lowered", policy.policy_id);

    // ── fs — allow_only over {package root} ∪ requested roots; exec is the
    // pinned executables only (a request to run anything else cannot be
    // spelled — the exec set is the package's own). ──
    policy.fs.read.mode = ReadMode::AllowOnly;
    policy.fs.read.allow = Vec::new();
    let mut allow = vec![ctx.package_root.to_string()];
    for root in &req.fs_roots {
        if root.is_empty() || root.contains("..") {
            return Err(LowerError::Unrepresentable(format!(
                "fs_root {root:?} is not a canonical root spelling"
            )));
        }
        allow.push(root.clone());
    }
    policy.fs.read.allow.append(&mut allow);
    policy.fs.exec = ExecPolicy::Allow(
        ctx.exec_paths
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>(),
    );

    // ── net — `none`; the ABI channel is the single allow-listed unix
    // socket (V4). `egress` claims cannot lower onto the seatbelt lane. ──
    policy.net.unix_sockets.mode = UnixSocketMode::AllowListed;
    policy.net.unix_sockets.allow = vec![ctx.channel_socket.to_string()];
    if !req.egress.is_empty() {
        losses.push(format!(
            "requests.egress: {} destination(s) not lowered — the \
             subprocess_confined seatbelt lane has no egress mediator \
             (mediated egress is the container/remote lane's)",
            req.egress.len()
        ));
    }

    // ── proc — cleared environment + declared keys only; no spawn without
    // a grant: `max_processes` stays the kernel default bound, but the
    // variant's *own* process is the exec — a plugin with no Exec grant
    // still must run, so the bound is what limits its children. ──
    policy.proc.env.inherit = EnvInherit::None;
    let mut env = Vec::new();
    for key in &req.env_keys {
        match ctx.ambient_env.iter().find(|(k, _)| k == key) {
            Some((k, v)) => env.push((k.clone(), v.clone())),
            None => losses.push(format!(
                "requests.env_keys[{key}]: no ambient value to project \
                 (declared keys are a projection, never an injection)"
            )),
        }
    }

    // ── the Permission — one grant per requested effect (delegable: false),
    // the budget claim lowering onto grant constraints. ──
    let mut grants = Vec::new();
    for e in &req.effects {
        let domain = EffectDomain::parse(&e.domain).map_err(|_| {
            LowerError::Unrepresentable(format!(
                "effect domain {:?} is not a closed member",
                e.domain
            ))
        })?;
        grants.push(Grant {
            effect: hh_hir::kinds::EffectClass::domain_only(domain),
            scope: e.scope.clone(),
            constraints: GrantConstraints {
                budget: req.budget_share.clone(),
                ..GrantConstraints::default()
            },
            delegable: false,
        });
    }
    if req.budget_share.is_some() {
        losses.push(
            "requests.budget_share: lowered onto grant constraints; the seatbelt \
             lane cannot meter cpu/mem/wall bounds — ResourceLimits members \
             unenforced (recorded, not zeroed)"
                .to_string(),
        );
    }
    let permission = PermissionRecord {
        holder: ctx.holder.clone(),
        grants,
        issuer: Issuer {
            authority: hh_provenance::authority::AuthorityClass::Principal,
            reference: ctx.issuer_ref.to_string(),
        },
        validity: Validity::open_from(ctx.at),
        revocation: None,
    };

    // The lowered policy must itself validate (deny-by-default + the
    // protected-metadata floor); a policy that cannot validate never
    // reaches the helper.
    policy
        .validate()
        .map_err(|e| LowerError::Unrepresentable(format!("lowered policy invalid: {e}")))?;

    Ok(LoweredRequests {
        permission,
        policy,
        env,
        losses,
    })
}

/// The view-kind set a session's `read_view` may serve — derived from the
/// sealed grants: `kernel_own` projections are the class's own declared
/// view (always contract-scoped); `principal_view` requires at least one
/// conferred grant. Peer kinds never enter the set (V3).
pub fn view_kinds_for(permission: &PermissionRecord) -> BTreeSet<String> {
    let mut kinds = BTreeSet::from(["kernel_own".to_string()]);
    if !permission.grants.is_empty() {
        kinds.insert("principal_view".to_string());
    }
    kinds
}

/// The stamped-output provenance for plugin data crossing inward (V1's
/// stamping half — `origin = tool(extension_ref)`, `authority ≤ external`,
/// `taint ∋ extension_id`; the host applies it to every result document it
/// hands to kernel consumers).
pub fn stamp_json(extension_id: &str, doc: &Json) -> Json {
    let mut stamped = match doc {
        Json::Obj(m) => Json::Obj(m.clone()),
        other => Json::obj([("value", other.clone())]),
    };
    if let Json::Obj(m) = &mut stamped {
        m.insert(
            "origin".to_string(),
            Json::str(format!("tool:{extension_id}")),
        );
        m.insert("authority".to_string(), Json::str("external"));
        m.insert(
            "taint".to_string(),
            Json::Arr(vec![Json::str(extension_id.to_string())]),
        );
    }
    stamped
}
