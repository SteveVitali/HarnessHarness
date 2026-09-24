//! The kernel-side `CredentialBroker` (§5g.3 §2; ADR-0058): the **only** holder
//! of secret material above the source. The monitor decides (PDP), the broker
//! delivers (CDP) — the broker never widens authority, never falls back to an
//! environment value, and fails closed when the vault or the audit writer is
//! unavailable (`Refused{broker_unavailable | audit_unavailable}`; AC-R-2.8.3-10).
//!
//! # Stage-1 cut (ADR-0058 §e)
//!
//! Landed here: `register_channel`, `grant` (produces the `secret_access`
//! `Grant` record the monitor decides over), `bind` (PDP gate + durable
//! `bound`), `mediate` (PDP gate + durable `used` **before** the staged delivery
//! is returned — the on-the-wire rewrite is the Stage-2 egress mediator,
//! DF-S1.12-1/S2.4), `revoke` (+ the `on_run_terminal`/`rotate` implicit
//! triggers), `rotate` (CAS + mask-set retention + `rotated`), `kernel_use`
//! (`bindable = false` channels; audited `used{destination = kernel:<component>}`),
//! `fingerprint`, `mask_set`, and the `leak_scan_run` test-battery entry.
//! `mint` exists as the failure-typed SPI (`Deferred{stage: 2}` — minted
//! delivery is Stage 2).

use std::collections::{BTreeMap, BTreeSet};

use hh_containment::policy::IsolationClass;
use hh_hir::kinds::{EffectClass, EffectDomain};
use hh_hir::records::{Grant, Issuer};
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::store::{rfc3339_ms, Lease, Store};
use hh_monitor::assess::SecretTransport;
use hh_provenance::{AuthorityClass, ProvenanceRecord};
use hh_wire::json::Json;

use crate::channel::{SecretChannel, SecretChannelSpec, SecretSource, SenderConstraint};
use crate::decisions;
use crate::env::{EnvBinding, EnvSpec};
use crate::errors::{BrokerError, Refused, RefusedCode};
use crate::events;
use crate::mask::{MaskEntry, MaskSet};
use crate::redact::{leak_scan, DetectorKind, DetectorSet, Leak, ScanTarget};
use crate::types::{
    AccessClass, BindingState, KernelPurpose, Placeholder, ProvidedSecret, SecretFingerprint,
    SecretUnavailable,
};

/// The kernel component tag for this crate's audit rows.
pub const COMPONENT: &str = "hh-secrets";

// ─────────────────────────────────────────────────────────────────────────────
// The secret-source SPI — the vault boundary (kernel-side only)
// ─────────────────────────────────────────────────────────────────────────────

/// A source resolution failure — everything maps to `BrokerUnavailable` at the
/// verb boundary (fail closed; the detail is a source *coordinate*, never a
/// partial value).
#[derive(Debug, Clone, PartialEq)]
pub struct SourceError {
    /// The source kind that failed.
    pub kind: &'static str,
    /// The content-free detail.
    pub detail: String,
}

/// The resolver SPI — how the broker materialises a `SecretSource`. Implementors
/// are kernel-side only; a resolver is *never* consulted to back-fill an
/// environment value (there is no env fallback — SV-4/AC-R-2.8.3-10).
pub trait SecretSourceResolver {
    /// Resolve `source` to the secret value. `Err` = the source is unavailable
    /// or the item absent — both fail closed identically.
    fn resolve(&self, source: &SecretSource) -> Result<String, SourceError>;
}

/// The coordinate key a resolver indexes on (`<kind>:<coord>`).
pub fn coord_key(source: &SecretSource) -> String {
    match source {
        SecretSource::OperatorVault { vault_ref } => format!("operator_vault:{vault_ref}"),
        SecretSource::OsKeyring { item } => format!("os_keyring:{item}"),
        SecretSource::LaunchEnv { var } => format!("launch_env:{var}"),
        SecretSource::ExternalBinding { external_ref, .. } => {
            format!("external_binding:{external_ref}")
        }
    }
}

/// An in-process vault — `coord_key → value` (the Stage-1 stand-in for the
/// operator vault / keyring; the values are kernel-held for the broker's
/// lifetime).
#[derive(Debug, Clone, Default)]
pub struct StaticVault {
    /// `coord_key → value`.
    pub values: BTreeMap<String, String>,
}

impl StaticVault {
    /// Insert a value under a source's coordinate.
    pub fn put(&mut self, source: &SecretSource, value: impl Into<String>) {
        self.values.insert(coord_key(source), value.into());
    }
}

impl SecretSourceResolver for StaticVault {
    fn resolve(&self, source: &SecretSource) -> Result<String, SourceError> {
        self.values
            .get(&coord_key(source))
            .cloned()
            .ok_or_else(|| SourceError {
                kind: source.kind(),
                detail: format!("no value registered under {}", coord_key(source)),
            })
    }
}

/// The launch-env resolver — reads `std::env` for `launch_env{var}` sources
/// (the model-gateway credential path); other kinds are `Unavailable` (a
/// resolver only answers its own kind — no cross-source fallback).
#[derive(Debug, Clone, Copy, Default)]
pub struct LaunchEnvResolver;

impl SecretSourceResolver for LaunchEnvResolver {
    fn resolve(&self, source: &SecretSource) -> Result<String, SourceError> {
        match source {
            SecretSource::LaunchEnv { var } => std::env::var(var).map_err(|_| SourceError {
                kind: "launch_env",
                detail: format!("launch env var {var} unset"),
            }),
            other => Err(SourceError {
                kind: other.kind(),
                detail: "resolver does not serve this source kind".into(),
            }),
        }
    }
}

/// A resolver over a `StaticVault` *plus* the launch env (the normal Stage-1
/// composition: operator-provided values in the static vault, the gateway
/// credential from launch env).
#[derive(Debug, Clone, Default)]
pub struct CompositeResolver {
    /// The operator-vault/keyring/service-account values.
    pub vault: StaticVault,
}

impl SecretSourceResolver for CompositeResolver {
    fn resolve(&self, source: &SecretSource) -> Result<String, SourceError> {
        match source {
            SecretSource::LaunchEnv { .. } => LaunchEnvResolver.resolve(source),
            other => self.vault.resolve(other),
        }
    }
}

/// The fail-closed resolver — every source `Unavailable` (the LT-10 fault
/// injection and the default when no vault is configured).
#[derive(Debug, Clone, Copy, Default)]
pub struct DenyAllResolver;

impl SecretSourceResolver for DenyAllResolver {
    fn resolve(&self, source: &SecretSource) -> Result<String, SourceError> {
        Err(SourceError {
            kind: source.kind(),
            detail: "resolver unavailable".into(),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The binding record
// ─────────────────────────────────────────────────────────────────────────────

/// `CredentialBinding` — the live binding row (§5g.3 §2): value-free by
/// construction (the *value* stays in the resolver/pending map; the row is
/// coordinates only). `expires_at` ⊆ run lifetime (SV-6 — the
/// `on_run_terminal` implicit revoke is the enforcement half).
#[derive(Debug, Clone, PartialEq)]
pub struct CredentialBinding {
    /// The allocated binding id (`bnd-<n>`).
    pub binding_id: String,
    /// The channel.
    pub channel_id: String,
    /// The channel revision at bind time (a rotation re-binds).
    pub channel_revision: u64,
    /// The run.
    pub run_id: String,
    /// The holder (`AgentProcess` semantic id — the grant's `holder`).
    pub holder: String,
    /// The environment handle the binding projects into.
    pub env_handle: String,
    /// The `Permission`-record coordinate the covering grant was minted under
    /// (the decided row's covering `handle_id` — §5g.3 §3's `permission_id`).
    pub permission_id: String,
    /// The delivery mode.
    pub mode: SecretTransport,
    /// The `security.permission.decided` event the bind relied on.
    pub decision_ref: String,
    /// The covering `effect_id` that decision was for (`used` rows reference it).
    pub effect_id: String,
    /// The bound destination set (⊆ channel scope).
    pub destinations: BTreeSet<String>,
    /// Issue timestamp (RFC 3339 ms).
    pub issued_at: String,
    /// Expiry (RFC 3339 ms) — `min(channel lifetime, handle expiry, run end)`.
    pub expires_at: String,
    /// The lifecycle state (`bound | active | expired | revoked` — SV-6).
    pub state: BindingState,
}

impl CredentialBinding {
    /// Whether the binding may still mediate (`bound | active`).
    pub fn is_live(&self) -> bool {
        self.state.is_live()
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("binding_id", Json::str(self.binding_id.clone())),
            ("run_id", Json::str(self.run_id.clone())),
            ("env_handle", Json::str(self.env_handle.clone())),
            ("channel_id", Json::str(self.channel_id.clone())),
            ("permission_id", Json::str(self.permission_id.clone())),
            ("mode", Json::str(self.mode.as_str())),
            ("expires_at", Json::str(self.expires_at.clone())),
            ("revision", Json::Int(self.channel_revision as i64)),
            ("state", Json::str(self.state.as_str())),
            ("holder", Json::str(self.holder.clone())),
            ("decision_ref", Json::str(self.decision_ref.clone())),
            ("effect_id", Json::str(self.effect_id.clone())),
            (
                "destinations",
                Json::Arr(
                    self.destinations
                        .iter()
                        .map(|d| Json::str(d.clone()))
                        .collect(),
                ),
            ),
            ("issued_at", Json::str(self.issued_at.clone())),
        ])
    }

    /// The `idp/1` version id of the binding row (`RecordKind::CredentialBinding`).
    pub fn version_id(&self) -> String {
        hh_identity::idp::identify_bytes(
            hh_identity::RecordKind::CredentialBinding,
            &self.to_json().to_canonical_string().into_bytes(),
        )
    }
}

/// `bind`'s request — `{env_handle, channel_id, mode, monitor_decision_ref}`
/// plus the holder and the destination set the binding is scoped to (§5g.3 §2).
#[derive(Debug, Clone)]
pub struct BindRequest {
    /// The channel.
    pub channel_id: String,
    /// The holder the grant was issued to.
    pub holder: String,
    /// The environment handle ref the binding projects into.
    pub env_handle_ref: String,
    /// The handle's isolation class (for the `wrapped_long_lived` ≥
    /// `process_sandbox` check).
    pub env_isolation: IsolationClass,
    /// The requested mode.
    pub mode: SecretTransport,
    /// The `security.permission.decided` event id (the PDP record).
    pub monitor_decision_ref: String,
    /// The destinations the binding may deliver to (⊆ channel scope).
    pub destinations: BTreeSet<String>,
}

/// `RequestDescriptor` — `mediate`'s staged request `{effect_id, destination,
/// method?, path?}` (§5g.3 §2 — the `carrier_slots` the mediator fills are the
/// Stage-2 wire machinery; the descriptor is what the PDP gate re-checks).
/// Content-free coordinates only — the request's *body* is the Stage-2
/// mediator's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestDescriptor {
    /// The `action.effect.*` scope the use is charged to (SV-5 — credential
    /// use rides the `net_egress` effect it decorates).
    pub effect_id: String,
    /// The destination host (matched against the binding's `destinations` by
    /// the interim `ResourcePattern` rules).
    pub destination: String,
    /// The HTTP method, when the request has one.
    pub method: Option<String>,
    /// The request path, when there is one — checked for ambiguity
    /// (dot-segments, encoded separators, backslashes → `ambiguous_path`).
    pub path: Option<String>,
}

/// Whether a request path is ambiguous — a dot-segment (`..`, `.`), an
/// encoded separator/dot (`%2e`, `%2f`, `%5c` — any case), a backslash or a
/// NUL. The Stage-2 mediator's full normalization is ADR-0061's `normalize`;
/// this is the LT-02 fail-closed shape at Stage 1.
pub fn path_is_ambiguous(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    path.split('/').any(|seg| seg == ".." || seg == ".")
        || path.contains('\\')
        || path.contains('\0')
        || lower.contains("%2e")
        || lower.contains("%2f")
        || lower.contains("%5c")
        || lower.contains("%00")
}

/// `mediate`'s outcome (§5g.3 §2 `MediationOutcome`).
#[derive(Debug, Clone, PartialEq)]
pub enum MediationOutcome {
    /// The use was audited and the delivery is staged — the kernel-side
    /// `Delivery` record is what the Stage-2 egress mediator executes. The
    /// caller receives **no value** (the claim is a kernel-internal coordinate).
    Staged(Delivery),
    /// Refused — a `security.credential.denied` row was appended durably
    /// *before* this is visible.
    Refused(Refused),
}

/// The staged delivery — `{binding_id, effect_id, destination, mode,
/// used_event_id, provided, claim_id}`; `claim_id` names the kernel-held value
/// slot the Stage-2 mediator drains. Content-free.
#[derive(Debug, Clone, PartialEq)]
pub struct Delivery {
    /// The binding.
    pub binding_id: String,
    /// The effect the use was charged to.
    pub effect_id: String,
    /// The destination.
    pub destination: String,
    /// The delivery mode.
    pub mode: SecretTransport,
    /// The durable `security.credential.used` row's event id (durable before
    /// visible — SV-5).
    pub used_event_id: String,
    /// `provided: yes|no` — the only value fact recorded.
    pub provided: ProvidedSecret,
    /// The kernel-held claim coordinate (opaque — resolves only inside the
    /// broker).
    pub claim_id: String,
}

/// The `revoke` outcome — the bindings the revoke dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct RevokeOutcome {
    /// The binding ids revoked (empty + `already` when idempotent).
    pub revoked: Vec<String>,
    /// Whether the target was already dead (no event emitted — idempotent).
    pub already: bool,
}

/// `revoke`'s target — a channel (drops all live bindings) or one binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevokeTarget {
    /// A whole channel — every live binding on it drops too.
    Channel(String),
    /// One binding.
    Binding(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// The broker
// ─────────────────────────────────────────────────────────────────────────────

/// `CredentialBroker` — the kernel-side channel/binding tables plus the vault
/// handle and the fingerprint key. **Kernel component only** — it is never
/// inside an environment handle, and its fields are never serialized to a
/// durable or model-visible surface.
pub struct CredentialBroker {
    /// `channel_id → channel`.
    channels: BTreeMap<String, SecretChannel>,
    /// Revoked channel ids (`bind`/`kernel_use`/`mediate` refuse).
    revoked_channels: BTreeSet<String>,
    /// `binding_id → binding`.
    bindings: BTreeMap<String, CredentialBinding>,
    /// Binding-scoped placeholders (`binding_id → placeholder`).
    placeholders: BTreeMap<String, Placeholder>,
    /// The resolver (the vault boundary).
    resolver: Box<dyn SecretSourceResolver>,
    /// The kernel-held fingerprint key (generated at construction; never
    /// serialized — fingerprints are stable within a broker lifetime).
    secret_key: String,
    /// Retained rotated values (`channel_id → old values` — the mask set's
    /// SV-1 "all revisions" half).
    rotated: BTreeMap<String, Vec<(u64, String)>>,
    /// Kernel-held staged values (`claim_id → (binding_id, value)`) — the
    /// Stage-2 mediator drains these; nothing else can. The binding coordinate
    /// is recorded so `revoke`/rotation drop a dead binding's claims too (SV-6
    /// — a revoked binding's staged delivery is undeliverable).
    pending: BTreeMap<String, (String, String)>,
    /// The binding/claim counters.
    seq: u64,
}

impl CredentialBroker {
    /// A broker over `resolver` (fail-closed when it errors) with an
    /// operator-supplied fingerprint key.
    pub fn new(resolver: Box<dyn SecretSourceResolver>, secret_key: impl Into<String>) -> Self {
        CredentialBroker {
            channels: BTreeMap::new(),
            revoked_channels: BTreeSet::new(),
            bindings: BTreeMap::new(),
            placeholders: BTreeMap::new(),
            resolver,
            secret_key: secret_key.into(),
            rotated: BTreeMap::new(),
            pending: BTreeMap::new(),
            seq: 0,
        }
    }

    fn alloc(&mut self, prefix: &str) -> String {
        self.seq += 1;
        format!("{prefix}-{:06}", self.seq)
    }

    /// The registered channel, if any.
    pub fn channel(&self, channel_id: &str) -> Option<&SecretChannel> {
        self.channels.get(channel_id)
    }

    /// The binding, if any.
    pub fn binding(&self, binding_id: &str) -> Option<&CredentialBinding> {
        self.bindings.get(binding_id)
    }

    /// All live bindings on a channel.
    pub fn live_bindings(&self, channel_id: &str) -> Vec<&CredentialBinding> {
        self.bindings
            .values()
            .filter(|b| b.is_live() && b.channel_id == channel_id)
            .collect()
    }

    /// The placeholder a binding projects (for `env_apply`'s
    /// `bound_placeholders` map).
    pub fn placeholder_for(&self, binding_id: &str) -> Option<&Placeholder> {
        self.placeholders.get(binding_id)
    }

    // ── register_channel ────────────────────────────────────────────────

    /// `register_channel(spec) → channel` (§5g.3 §2). The value is **never
    /// read** at registration — the spec is a source *coordinate*. Errors:
    /// `ChannelExists`, `InvalidBinding`.
    pub fn register_channel(
        &mut self,
        channel_id: impl Into<String>,
        spec: SecretChannelSpec,
        provenance: ProvenanceRecord,
    ) -> Result<SecretChannel, BrokerError> {
        let channel_id = channel_id.into();
        if !SecretChannel::valid_id(&channel_id) {
            return Err(BrokerError::InvalidBinding {
                detail: format!("channel_id {channel_id:?} is not [a-z0-9_.-]+"),
            });
        }
        if self.channels.contains_key(&channel_id) && !self.revoked_channels.contains(&channel_id) {
            return Err(BrokerError::ChannelExists { channel_id });
        }
        // The access-class/bindable coherence rule (§5g.3 §1: a `kernel_only`
        // channel is `bindable: false`; an `env`-class channel exists only for
        // legacy direct injection and must stay bindable).
        if spec.access_class == AccessClass::KernelOnly && spec.bindable {
            return Err(BrokerError::InvalidBinding {
                detail: "access_class kernel_only requires bindable: false".into(),
            });
        }
        if spec.access_class == AccessClass::Env && !spec.bindable {
            return Err(BrokerError::InvalidBinding {
                detail: "access_class env requires bindable: true".into(),
            });
        }
        if spec.bindable && spec.delivery_modes.is_empty() {
            return Err(BrokerError::InvalidBinding {
                detail: "a bindable channel must declare at least one delivery mode".into(),
            });
        }
        let ch = SecretChannel {
            channel_id,
            spec,
            revision: 1,
            provenance,
        };
        self.revoked_channels.remove(&ch.channel_id);
        self.channels.insert(ch.channel_id.clone(), ch.clone());
        Ok(ch)
    }

    // ── grant ────────────────────────────────────────────────────────────

    /// `grant(run_id, holder, channel_id, scope, constraints)` — the broker's
    /// half of issuing a `secret_access` permission: validate `scope ⊆
    /// channel.scope` (`ScopeExceedsChannel`) and `issuer.authority ≥
    /// principal` (`IssuerAuthorityInsufficient` — a `delegate` never issues),
    /// then produce the `Grant` record the caller feeds the monitor (the PDP —
    /// the broker checks admissibility, it does not confer).
    ///
    /// `scope` is the requested destination set; the issued grant's resource
    /// pattern is `secret:<channel_id>` and the *destination* bound rides the
    /// binding (`bind` re-checks it against the channel scope).
    pub fn grant(
        &self,
        _run_id: &str,
        holder: &str,
        channel_id: &str,
        scope: &BTreeSet<String>,
        constraints: hh_hir::records::GrantConstraints,
        issuer: &Issuer,
    ) -> Result<Grant, BrokerError> {
        if issuer.authority < AuthorityClass::Principal {
            return Err(BrokerError::IssuerAuthorityInsufficient {
                issuer_authority: issuer.authority.as_str().into(),
                required: "principal".into(),
            });
        }
        let ch = self
            .channels
            .get(channel_id)
            .ok_or_else(|| BrokerError::UnknownChannel {
                channel_id: channel_id.into(),
            })?;
        if self.revoked_channels.contains(channel_id) {
            return Err(BrokerError::Refused(Refused::new(
                RefusedCode::Revoked,
                format!("channel {channel_id} is revoked"),
            )));
        }
        for d in scope {
            if !ch.spec.admits_destination(d) {
                return Err(BrokerError::ScopeExceedsChannel {
                    channel_id: channel_id.into(),
                    scope: d.clone(),
                });
            }
        }
        let _ = holder;
        Ok(Grant {
            effect: EffectClass::domain_only(EffectDomain::SecretAccess),
            scope: SecretChannel::grant_scope(channel_id),
            constraints,
            delegable: false,
        })
    }

    // ── bind ─────────────────────────────────────────────────────────────

    /// `bind(run_id, env_handle, channel_id, mode, monitor_decision_ref)` —
    /// the PDP/CDP gate (ADR-0058): a covering `security.permission.decided{
    /// decision = allow}` for a `secret_access` grant must exist (else
    /// `DecisionMissing`, AC-R-2.8.3-13); the mode must be in the channel's
    /// `delivery_modes_allowed` (`ModeNotAllowed`); `wrapped_long_lived`
    /// requires isolation ≥ `process_sandbox` (`EnvironmentNotIsolated`); a
    /// `minted_scoped` bind on a `sender_constraint`-declaring channel is
    /// `SenderConstraintUnmet` (Stage 1 has no verifier); a canary channel is
    /// `Canary` + `leak_detected`; the channel must be `bindable` and live
    /// (`NotBindable`/`Revoked`). The `security.credential.bound` row is
    /// durable **before** the binding is visible (`AuditUnavailable` on append
    /// failure — fail closed).
    pub fn bind(
        &mut self,
        store: &mut Store,
        run_id: &str,
        lease: &Lease,
        req: BindRequest,
    ) -> Result<CredentialBinding, Refused> {
        let ch = self
            .channels
            .get(&req.channel_id)
            .ok_or_else(|| {
                Refused::new(
                    RefusedCode::NoGrant,
                    format!("no channel {}", req.channel_id),
                )
            })?
            .clone();
        if self.revoked_channels.contains(&req.channel_id) {
            return Err(Refused::new(
                RefusedCode::Revoked,
                format!("channel {} is revoked", req.channel_id),
            ));
        }
        if ch.spec.canary {
            // A canary is never injected — the attempt itself is the signal
            // (ADR-0059 D3). The `leak_detected` row is durable before the
            // refusal is visible.
            let leak = Leak::at_mediation(
                format!("bind:{}", req.channel_id),
                DetectorKind::Canary,
                Some(req.channel_id.clone()),
            );
            self.append_security(
                store,
                run_id,
                lease,
                "security.secret.leak_detected",
                events::leak_detected_payload(&leak),
            )
            .map_err(|e| {
                Refused::new(
                    RefusedCode::AuditUnavailable,
                    format!("leak_detected row not durable: {e}"),
                )
            })?;
            return Err(Refused::new(
                RefusedCode::Canary,
                format!("channel {} is a canary — never injected", req.channel_id),
            ));
        }
        if !ch.spec.bindable {
            return Err(Refused::new(
                RefusedCode::NotBindable,
                format!("channel {} is kernel-only", req.channel_id),
            ));
        }
        if !ch.spec.delivery_modes.contains(&req.mode) {
            return Err(Refused::new(
                RefusedCode::ModeNotAllowed,
                format!(
                    "mode {} not in channel {}'s delivery_modes_allowed",
                    req.mode.as_str(),
                    req.channel_id
                ),
            ));
        }
        // A declared sender constraint cannot be verified at Stage 1 — the
        // verifier lands with `minted_scoped` at Stage 2 (fail closed).
        if req.mode == SecretTransport::MintedScoped
            && ch.spec.sender_constraint != SenderConstraint::None
        {
            return Err(Refused::new(
                RefusedCode::SenderConstraintUnmet,
                format!(
                    "channel {}'s sender_constraint {} cannot be verified",
                    req.channel_id,
                    ch.spec.sender_constraint.as_str()
                ),
            ));
        }
        if req.mode == SecretTransport::WrappedLongLived
            && req.env_isolation.strength() < IsolationClass::ProcessSandbox.strength()
        {
            return Err(Refused::new(
                RefusedCode::EnvironmentNotIsolated,
                format!(
                    "wrapped_long_lived requires isolation ≥ process_sandbox (got {})",
                    req.env_isolation.as_str()
                ),
            ));
        }
        for d in &req.destinations {
            if !ch.spec.admits_destination(d) {
                return Err(Refused::new(
                    RefusedCode::NoGrant,
                    format!(
                        "destination {d} is outside channel {}'s destinations",
                        req.channel_id
                    ),
                ));
            }
        }
        // The PDP gate — the *recorded* monitor decision must cover
        // `secret_access` on `secret:<channel>` for this holder.
        let view = decisions::fold(store.events(run_id).map_err(|e| {
            Refused::new(RefusedCode::AuditUnavailable, format!("events fold: {e}"))
        })?);
        let cover = decisions::covering_secret_access(
            &view,
            Some(&req.monitor_decision_ref),
            &req.channel_id,
            &req.holder,
        )
        .ok_or_else(|| {
            Refused::new(
                RefusedCode::DecisionMissing,
                format!(
                    "no decided{{allow}} row {} covering secret_access on {}",
                    req.monitor_decision_ref, req.channel_id
                ),
            )
        })?;
        // Audit *before* the binding exists — durable-before-visible.
        let binding_id = self.alloc("bnd");
        let now_ms = store.now_ms();
        let expires_ms = match ch.spec.max_lifetime_ms {
            Some(win) => now_ms.saturating_add(win),
            None => u64::MAX,
        };
        let expires_at = if expires_ms == u64::MAX {
            "run-end".to_string()
        } else {
            rfc3339_ms(expires_ms)
        };
        let payload = events::bound_payload(
            &binding_id,
            &req.channel_id,
            ch.revision,
            &req.holder,
            &req.env_handle_ref,
            req.mode.as_str(),
            &req.monitor_decision_ref,
            &expires_at,
        );
        self.append_security(store, run_id, lease, "security.credential.bound", payload)
            .map_err(|e| {
                Refused::new(
                    RefusedCode::AuditUnavailable,
                    format!("bound row not durable: {e}"),
                )
            })?;
        let binding = CredentialBinding {
            binding_id: binding_id.clone(),
            channel_id: req.channel_id.clone(),
            channel_revision: ch.revision,
            run_id: run_id.to_string(),
            holder: req.holder.clone(),
            env_handle: req.env_handle_ref.clone(),
            permission_id: cover.handle_id.clone(),
            mode: req.mode,
            decision_ref: req.monitor_decision_ref.clone(),
            effect_id: cover.effect_id,
            destinations: req.destinations.clone(),
            issued_at: store.ts_now(),
            expires_at,
            state: BindingState::Bound,
        };
        let ph = Placeholder::mint(run_id, &binding);
        self.placeholders.insert(binding_id.clone(), ph);
        self.bindings.insert(binding_id, binding.clone());
        Ok(binding)
    }

    // ── mediate ──────────────────────────────────────────────────────────

    /// `mediate(binding_id, request)` — the Stage-1 half of egress mediation
    /// (ADR-0058's stage cut): the PDP gate re-checks the covering decision
    /// (the handle may have been revoked since `bind`), the destination must
    /// be inside the binding's declared set, the source must resolve (fail
    /// closed — `BrokerUnavailable`, never an env-value fallback), and the
    /// `security.credential.used` row is durable **before** the `Delivery` is
    /// returned (SV-5). The returned `Delivery` is *staged* — the wire rewrite
    /// is the Stage-2 mediator's (DF-S1.12-1); the value claim stays
    /// kernel-held.
    pub fn mediate(
        &mut self,
        store: &mut Store,
        run_id: &str,
        lease: &Lease,
        binding_id: &str,
        request: &RequestDescriptor,
    ) -> Result<MediationOutcome, BrokerError> {
        let Some(binding) = self.bindings.get(binding_id).cloned() else {
            return Ok(MediationOutcome::Refused(Refused::new(
                RefusedCode::Revoked,
                format!("no binding {binding_id}"),
            )));
        };
        let deny = |broker: &mut CredentialBroker,
                    store: &mut Store,
                    lease: &Lease,
                    code: RefusedCode,
                    detail: String|
         -> Result<MediationOutcome, BrokerError> {
            let payload = events::denied_payload(
                Some(&binding.binding_id),
                &request.destination,
                code,
                Some(&binding.channel_id),
                Some(&request.effect_id),
            );
            // The denial is durable before the refusal is visible; an audit
            // failure here *upgrades* the refusal to audit_unavailable rather
            // than returning an unaudited deny.
            let (code, detail) = match broker.append_security(
                store,
                run_id,
                lease,
                "security.credential.denied",
                payload,
            ) {
                Ok(_) => (code, detail),
                Err(e) => (
                    RefusedCode::AuditUnavailable,
                    format!("denied row not durable: {e}"),
                ),
            };
            Ok(MediationOutcome::Refused(Refused::new(code, detail)))
        };
        // The normative refusal order (§5g.3 §5): revoked → expired →
        // out_of_scope → ambiguous_path → canary → decision_missing →
        // broker_unavailable.
        if !binding.is_live() || self.revoked_channels.contains(&binding.channel_id) {
            if let Some(b) = self.bindings.get_mut(binding_id) {
                b.state = BindingState::Revoked;
            }
            return deny(
                self,
                store,
                lease,
                RefusedCode::Revoked,
                format!("binding {binding_id} is revoked"),
            );
        }
        if binding.expires_at != "run-end" && binding.expires_at.as_str() <= store.ts_now().as_str()
        {
            if let Some(b) = self.bindings.get_mut(binding_id) {
                b.state = BindingState::Expired;
            }
            return deny(
                self,
                store,
                lease,
                RefusedCode::Expired,
                format!("binding {binding_id} expired at {}", binding.expires_at),
            );
        }
        if !binding.destinations.contains(&request.destination) {
            return deny(
                self,
                store,
                lease,
                RefusedCode::OutOfScope,
                format!(
                    "destination {} is outside binding {binding_id}'s scope",
                    request.destination
                ),
            );
        }
        if let Some(p) = &request.path {
            if path_is_ambiguous(p) {
                return deny(
                    self,
                    store,
                    lease,
                    RefusedCode::AmbiguousPath,
                    format!("request path on {binding_id} is ambiguous"),
                );
            }
        }
        // A canary channel is never injected — the attempt is the signal.
        let ch =
            self.channels
                .get(&binding.channel_id)
                .ok_or_else(|| BrokerError::UnknownChannel {
                    channel_id: binding.channel_id.clone(),
                })?;
        if ch.spec.canary {
            let leak = Leak::at_mediation(
                format!("mediate:{}", binding.channel_id),
                DetectorKind::Canary,
                Some(binding.channel_id.clone()),
            );
            self.append_security(
                store,
                run_id,
                lease,
                "security.secret.leak_detected",
                events::leak_detected_payload(&leak),
            )
            .map_err(|e| {
                BrokerError::Refused(Refused::new(
                    RefusedCode::AuditUnavailable,
                    format!("leak_detected row not durable: {e}"),
                ))
            })?;
            return deny(
                self,
                store,
                lease,
                RefusedCode::Canary,
                format!(
                    "channel {} is a canary — never injected",
                    binding.channel_id
                ),
            );
        }
        // PDP re-check — a covering decided-allow must still exist.
        let view = decisions::fold(store.events(run_id).map_err(|e| BrokerError::Ledger {
            detail: format!("events fold: {e}"),
        })?);
        if decisions::covering_secret_access(&view, None, &binding.channel_id, &binding.holder)
            .is_none()
        {
            return deny(
                self,
                store,
                lease,
                RefusedCode::DecisionMissing,
                format!(
                    "no covering decided{{allow}} for secret_access on {}",
                    binding.channel_id
                ),
            );
        }
        // Resolve the source — fail closed (never an env-value fallback).
        let value = match self.resolver.resolve(&ch.spec.source) {
            Ok(v) => v,
            Err(e) => {
                return deny(
                    self,
                    store,
                    lease,
                    RefusedCode::BrokerUnavailable,
                    format!("source {} unavailable: {}", e.kind, e.detail),
                );
            }
        };
        // The `used` row is durable before the delivery is visible (SV-5).
        let used_event_id = store.alloc_id("evt");
        let used = events::used_payload(
            binding_id,
            &binding.channel_id,
            binding.channel_revision,
            &request.destination,
            &request.effect_id,
            "allow",
            Some(&binding.decision_ref),
            ProvidedSecret { provided: true },
        );
        self.append_security_with_id(
            store,
            run_id,
            lease,
            &used_event_id,
            "security.credential.used",
            used,
        )
        .map_err(|e| {
            BrokerError::Refused(Refused::new(
                RefusedCode::AuditUnavailable,
                format!("used row not durable: {e}"),
            ))
        })?;
        // Stage the delivery — the value stays kernel-held under the claim id,
        // bound to this binding (a revoke drops it).
        let claim_id = self.alloc("claim");
        self.pending
            .insert(claim_id.clone(), (binding_id.to_string(), value));
        if let Some(b) = self.bindings.get_mut(binding_id) {
            b.state = BindingState::Active;
        }
        Ok(MediationOutcome::Staged(Delivery {
            binding_id: binding_id.to_string(),
            effect_id: request.effect_id.clone(),
            destination: request.destination.clone(),
            mode: binding.mode,
            used_event_id,
            provided: ProvidedSecret { provided: true },
            claim_id,
        }))
    }

    // ── mint (Stage-2 SPI) ───────────────────────────────────────────────

    /// `mint(binding_id, audience, ttl)` — the failure-typed SPI: minted
    /// delivery is Stage 2 (the minted-scoped Π rows are already in the
    /// monitor; the minter is not). A typed `Deferred`, never a panic.
    pub fn mint(
        &mut self,
        _binding_id: &str,
        _audience: &str,
        _ttl_ms: u64,
    ) -> Result<Json, BrokerError> {
        Err(BrokerError::Deferred {
            verb: "mint",
            stage: 2,
        })
    }

    // ── revoke ───────────────────────────────────────────────────────────

    /// `revoke(binding_id|channel_id, reason)` — idempotent (an already-dead
    /// target returns `already` with no duplicate row). Channel-level revoke
    /// drops every live binding and marks the channel revoked; the
    /// `security.credential.revoked` row is durable before the outcome is
    /// visible.
    pub fn revoke(
        &mut self,
        store: &mut Store,
        run_id: &str,
        lease: &Lease,
        target: RevokeTarget,
        reason: &str,
    ) -> Result<RevokeOutcome, BrokerError> {
        let (channel_id, single, dropped): (Option<String>, Option<String>, Vec<String>) =
            match &target {
                RevokeTarget::Channel(cid) => {
                    if self.revoked_channels.contains(cid) {
                        return Ok(RevokeOutcome {
                            revoked: Vec::new(),
                            already: true,
                        });
                    }
                    let dropped: Vec<String> = self
                        .live_bindings(cid)
                        .iter()
                        .map(|b| b.binding_id.clone())
                        .collect();
                    (Some(cid.clone()), None, dropped)
                }
                RevokeTarget::Binding(bid) => {
                    let Some(b) = self.bindings.get(bid) else {
                        return Err(BrokerError::UnknownBinding {
                            binding_id: bid.clone(),
                        });
                    };
                    if !b.is_live() {
                        return Ok(RevokeOutcome {
                            revoked: Vec::new(),
                            already: true,
                        });
                    }
                    (None, Some(bid.clone()), vec![bid.clone()])
                }
            };
        // Audit before the revoke is visible.
        self.append_security(
            store,
            run_id,
            lease,
            "security.credential.revoked",
            events::revoked_payload(single.as_deref(), channel_id.as_deref(), &dropped, reason),
        )
        .map_err(|e| BrokerError::Ledger {
            detail: format!("revoked row not durable: {e}"),
        })?;
        for bid in &dropped {
            if let Some(b) = self.bindings.get_mut(bid) {
                b.state = BindingState::Revoked;
            }
            // A revoked binding's staged claims are undeliverable — drop them.
            self.pending.retain(|_, (b, _)| b != bid);
        }
        if let Some(cid) = channel_id {
            self.revoked_channels.insert(cid);
        }
        Ok(RevokeOutcome {
            revoked: dropped,
            already: false,
        })
    }

    /// The implicit-revoke trigger: every binding on `run_id` drops at run
    /// terminal (SV-6 — binding lifetime ⊆ run lifetime). One `revoked` row
    /// per channel that had live bindings.
    pub fn on_run_terminal(
        &mut self,
        store: &mut Store,
        run_id: &str,
        lease: &Lease,
    ) -> Result<Vec<String>, BrokerError> {
        let mut revoked = Vec::new();
        let channel_ids: BTreeSet<String> = self
            .bindings
            .values()
            .filter(|b| b.is_live() && b.run_id == run_id)
            .map(|b| b.channel_id.clone())
            .collect();
        for cid in channel_ids {
            let dropped: Vec<String> = self
                .live_bindings(&cid)
                .iter()
                .filter(|b| b.run_id == run_id)
                .map(|b| b.binding_id.clone())
                .collect();
            if dropped.is_empty() {
                continue;
            }
            self.append_security(
                store,
                run_id,
                lease,
                "security.credential.revoked",
                events::revoked_payload(None, Some(&cid), &dropped, "run_terminal"),
            )
            .map_err(|e| BrokerError::Ledger {
                detail: format!("revoked row not durable: {e}"),
            })?;
            for bid in &dropped {
                if let Some(b) = self.bindings.get_mut(bid) {
                    b.state = BindingState::Revoked;
                }
                self.pending.retain(|_, (b, _)| b != bid);
            }
            revoked.extend(dropped);
        }
        Ok(revoked)
    }

    // ── rotate ───────────────────────────────────────────────────────────

    /// `rotate(channel_id, expected_revision, new_source)` — the
    /// compare-and-swap: `StaleRevision` on a mismatch, `BrokerUnavailable`
    /// when the *old* value can't be resolved (it must be retained in the
    /// mask set — SV-1 — so rotation can't proceed blind). Appends
    /// `security.credential.rotated` + a `revoked` row for every live binding
    /// (channel rotation is an implicit-revoke trigger) — all durable before
    /// the new revision is visible.
    pub fn rotate(
        &mut self,
        store: &mut Store,
        run_id: &str,
        lease: &Lease,
        channel_id: &str,
        expected_revision: u64,
        new_source: SecretSource,
    ) -> Result<u64, BrokerError> {
        let ch = self
            .channels
            .get_mut(channel_id)
            .ok_or_else(|| BrokerError::UnknownChannel {
                channel_id: channel_id.into(),
            })?
            .clone();
        if ch.revision != expected_revision {
            return Err(BrokerError::Conflict {
                channel_id: channel_id.into(),
                expected: expected_revision,
                current: ch.revision,
            });
        }
        // Retain the outgoing value in the mask set *before* the swap — a
        // rotation that cannot retain its old value is a rotation refused
        // (SV-1's "every revision stays masked" is a precondition, not a
        // degradation).
        let old = self.resolver.resolve(&ch.spec.source).map_err(|e| {
            BrokerError::Refused(Refused::new(
                RefusedCode::BrokerUnavailable,
                format!("rotate cannot retain r{}: {}", ch.revision, e.detail),
            ))
        })?;
        self.rotated
            .entry(channel_id.into())
            .or_default()
            .push((ch.revision, old));
        let old_revision = ch.revision;
        let new_revision = old_revision + 1;
        let dropped: Vec<String> = self
            .live_bindings(channel_id)
            .iter()
            .map(|b| b.binding_id.clone())
            .collect();
        // Both rows in one batch — the rotation and the implicit revocation
        // are one durable fact.
        let mut evs = vec![self.kernel_event(
            store,
            run_id,
            "security.credential.rotated",
            events::rotated_payload(channel_id, old_revision, new_revision),
        )?];
        if !dropped.is_empty() {
            evs.push(self.kernel_event(
                store,
                run_id,
                "security.credential.revoked",
                events::revoked_payload(None, Some(channel_id), &dropped, "channel_rotated"),
            )?);
        }
        store
            .append(run_id, lease, evs)
            .map_err(|e| BrokerError::Ledger {
                detail: format!("rotated row not durable: {e}"),
            })?;
        let ch = self.channels.get_mut(channel_id).unwrap();
        ch.spec.source = new_source;
        ch.revision = new_revision;
        for bid in &dropped {
            if let Some(b) = self.bindings.get_mut(bid) {
                b.state = BindingState::Revoked;
            }
            self.pending.retain(|_, (b, _)| b != bid);
        }
        Ok(new_revision)
    }

    // ── kernel_use ───────────────────────────────────────────────────────

    /// `kernel_use(channel_id, purpose ∈ {ledger_signing, gateway_auth,
    /// vault_auth}, caller)` (§5g.3 §2) — the `bindable = false` /
    /// `kernel_only` path (the model-gateway credential, the R-2.8.6 signer
    /// key, the vault credential). The caller must be a kernel component
    /// (ADR-0066 Rule P — enforced by construction: only kernel code can hold
    /// the broker), the channel must be `access_class = kernel_only` (a
    /// `bindable` channel is refused — `bind`/`mediate` is its path), and
    /// `security.credential.used{destination = kernel:<purpose>}` is durable
    /// before the value is returned. Fail closed (`BrokerUnavailable`/
    /// `AuditUnavailable`); a canary presented here is `leak_detected` +
    /// `Refused{canary}`.
    #[allow(clippy::too_many_arguments)] // the verb's record is the §5g.3 §2 shape — the arity is the record's.
    pub fn kernel_use(
        &mut self,
        store: &mut Store,
        run_id: &str,
        lease: &Lease,
        channel_id: &str,
        purpose: KernelPurpose,
        caller: &str,
        effect_id: &str,
    ) -> Result<String, Refused> {
        let ch = self
            .channels
            .get(channel_id)
            .ok_or_else(|| Refused::new(RefusedCode::NoGrant, format!("no channel {channel_id}")))?
            .clone();
        if self.revoked_channels.contains(channel_id) {
            return Err(Refused::new(
                RefusedCode::Revoked,
                format!("channel {channel_id} is revoked"),
            ));
        }
        if ch.spec.canary {
            let leak = Leak::at_mediation(
                format!("kernel_use:{channel_id}"),
                DetectorKind::Canary,
                Some(channel_id.to_string()),
            );
            self.append_security(
                store,
                run_id,
                lease,
                "security.secret.leak_detected",
                events::leak_detected_payload(&leak),
            )
            .map_err(|e| {
                Refused::new(
                    RefusedCode::AuditUnavailable,
                    format!("leak_detected row not durable: {e}"),
                )
            })?;
            return Err(Refused::new(
                RefusedCode::Canary,
                format!("channel {channel_id} is a canary — never injected"),
            ));
        }
        if ch.spec.bindable {
            return Err(Refused::new(
                RefusedCode::NotBindable,
                format!("channel {channel_id} is bindable — use bind/mediate"),
            ));
        }
        if ch.spec.access_class != AccessClass::KernelOnly {
            return Err(Refused::new(
                RefusedCode::NoGrant,
                format!("channel {channel_id} is not kernel_only"),
            ));
        }
        let value = self.resolver.resolve(&ch.spec.source).map_err(|e| {
            Refused::new(
                RefusedCode::BrokerUnavailable,
                format!("source {} unavailable: {}", e.kind, e.detail),
            )
        })?;
        // Audit first — the value is not released until `used` is durable.
        // `destination = kernel:<purpose>` names *what for*; `caller` names the
        // kernel component (both content-free coordinates).
        let destination = format!("kernel:{}", purpose.as_str());
        let mut used = events::used_payload(
            "",
            channel_id,
            ch.revision,
            &destination,
            effect_id,
            "kernel",
            None,
            ProvidedSecret { provided: true },
        );
        if let Json::Obj(m) = &mut used {
            m.insert("caller".into(), Json::str(caller));
        }
        self.append_security(store, run_id, lease, "security.credential.used", used)
            .map_err(|e| {
                Refused::new(
                    RefusedCode::AuditUnavailable,
                    format!("used row not durable: {e}"),
                )
            })?;
        Ok(value)
    }

    // ── fingerprint / mask_set / leak_scan ───────────────────────────────

    /// `fingerprint(value, channel_id, revision)` — keyed, truncated (128-bit),
    /// irreversible, stable within a revision: `H(secret_key ∥ channel_id ∥
    /// revision ∥ value)` under the one SHA-256 primitive (the fingerprint is a
    /// *keyed* digest — domain-framed under `secret.fingerprint`, never a
    /// content id).
    pub fn fingerprint(&self, value: &str, channel_id: &str, revision: u64) -> SecretFingerprint {
        let material = format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}",
            self.secret_key, channel_id, revision, value
        );
        let digest =
            hh_wire::sha256::sha256_hex(format!("secret.fingerprint\u{1f}{material}").as_bytes());
        SecretFingerprint {
            channel_id: channel_id.into(),
            revision,
            digest: digest[..32].to_string(),
        }
    }

    /// `mask_set()` — the complete known-value set: every live channel's
    /// current value plus every retained rotated value (SV-1). **Fails
    /// closed**: a channel whose source is unavailable makes the mask set
    /// unavailable (`BrokerUnavailable`) — an incomplete mask set is a leak
    /// vector, not a degradation.
    pub fn mask_set(&self) -> Result<MaskSet, BrokerError> {
        let mut ms = MaskSet::default();
        for (cid, ch) in &self.channels {
            if self.revoked_channels.contains(cid) {
                continue;
            }
            let value = self.resolver.resolve(&ch.spec.source).map_err(|e| {
                BrokerError::Refused(Refused::new(
                    RefusedCode::BrokerUnavailable,
                    format!("mask set: source {} unavailable: {}", e.kind, e.detail),
                ))
            })?;
            ms.insert(MaskEntry {
                channel_id: cid.clone(),
                revision: ch.revision,
                value: value.clone(),
                fingerprint: self.fingerprint(&value, cid, ch.revision),
                live: true,
                placeholder: None,
            });
            for (rev, old) in self.rotated.get(cid).cloned().unwrap_or_default() {
                ms.insert(MaskEntry {
                    channel_id: cid.clone(),
                    revision: rev,
                    value: old.clone(),
                    fingerprint: self.fingerprint(&old, cid, rev),
                    live: false,
                    placeholder: None,
                });
            }
        }
        Ok(ms)
    }

    /// `leak_scan(run)` — the test-battery entry (ADR-0059 D2): sweep the
    /// run's committed event payloads (the `event` target) with the detector
    /// set and report every leak. `leak_scan(run) = ∅` is LT-01's verdict.
    /// **A test/verifier path** — never called on a live boundary.
    pub fn leak_scan_run(
        &self,
        store: &Store,
        run_id: &str,
        detectors: &DetectorSet,
    ) -> Result<Vec<Leak>, BrokerError> {
        let events = store.events(run_id).map_err(|e| BrokerError::Ledger {
            detail: format!("events fold: {e}"),
        })?;
        let items: Vec<(ScanTarget, String)> = events
            .iter()
            .map(|e| {
                (
                    ScanTarget::Event(Some(e.event_id.clone())),
                    e.payload.to_canonical_string(),
                )
            })
            .collect();
        let refs: Vec<(ScanTarget, &str)> =
            items.iter().map(|(t, s)| (t.clone(), s.as_str())).collect();
        Ok(leak_scan(&refs, detectors))
    }

    /// The `SecretUnavailable` terminal observation for a refused mediation —
    /// what the model boundary sees (AC-R-2.8.3-10). A builder, not a sender:
    /// the caller places it in the tool-result/observation surface.
    pub fn terminal_secret_unavailable(channel_id: &str, r: &Refused) -> SecretUnavailable {
        SecretUnavailable {
            channel_id: channel_id.into(),
            reason: r.code,
        }
    }

    /// The `EnvSpec` the broker advertises for a handle — bindings only for
    /// *live* bindings on the handle whose `env_name` is in the channel's
    /// `allowed_env_names` (deny-by-default both ways).
    pub fn env_spec_for(&self, env_handle_ref: &str, allowlist: BTreeSet<String>) -> EnvSpec {
        let mut bindings = Vec::new();
        for (bid, b) in &self.bindings {
            if !b.is_live() || b.env_handle != env_handle_ref {
                continue;
            }
            let Some(ch) = self.channels.get(&b.channel_id) else {
                continue;
            };
            if let Some(names) = &ch.spec.allowed_env_names {
                for env_name in names {
                    bindings.push(EnvBinding {
                        env_name: env_name.clone(),
                        channel_id: b.channel_id.clone(),
                        binding_id: bid.clone(),
                        mode: b.mode,
                    });
                }
            }
        }
        EnvSpec {
            allowlist,
            bindings,
        }
    }

    // ── internals ────────────────────────────────────────────────────────

    /// Build a kernel `Event` for this run (producer/provenance stamped
    /// `hh-secrets`; parent = current head).
    fn kernel_event(
        &self,
        store: &Store,
        run_id: &str,
        class: &str,
        payload: Json,
    ) -> Result<Event, BrokerError> {
        Ok(Event {
            event_id: store.alloc_id("evt"),
            class: class.into(),
            ts: store.ts_now(),
            hlc: None,
            producer: Producer::kernel(COMPONENT),
            scope: Scope::default(),
            parent_event_id: store
                .head_event_id(run_id)
                .map_err(|e| BrokerError::Ledger {
                    detail: format!("head: {e}"),
                })?,
            causes: Vec::new(),
            refs: Vec::new(),
            ir_refs: Vec::new(),
            surface_ids: BTreeMap::new(),
            provenance: Some(ProvenanceRecord::kernel(COMPONENT, store.now_ms())),
            content_kind: None,
            payload,
        })
    }

    /// Append one security row through the run's fenced writer — the only
    /// write path the broker has (durable-before-visible is `Store::append`'s
    /// own commit rule; the caller maps a failure to `AuditUnavailable`).
    fn append_security(
        &mut self,
        store: &mut Store,
        run_id: &str,
        lease: &Lease,
        class: &str,
        payload: Json,
    ) -> Result<(), BrokerError> {
        let ev = self.kernel_event(store, run_id, class, payload)?;
        store
            .append(run_id, lease, vec![ev])
            .map_err(|e| BrokerError::Ledger {
                detail: format!("{e}"),
            })?;
        Ok(())
    }

    /// The same, with a caller-pinned `event_id` (the `used` row's id lands in
    /// the returned `Delivery` — the durable-before-visible witness).
    fn append_security_with_id(
        &mut self,
        store: &mut Store,
        run_id: &str,
        lease: &Lease,
        event_id: &str,
        class: &str,
        payload: Json,
    ) -> Result<(), BrokerError> {
        let mut ev = self.kernel_event(store, run_id, class, payload)?;
        ev.event_id = event_id.to_string();
        store
            .append(run_id, lease, vec![ev])
            .map_err(|e| BrokerError::Ledger {
                detail: format!("{e}"),
            })?;
        Ok(())
    }
}
