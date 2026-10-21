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
use crate::dpop::{self, DpopProof};
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

/// The `mh_mint:` token MAC — `idp_digest("secret.minted", secret_key ∥
/// binding_id ∥ audience ∥ expires_ms)` truncated to 32 hex (kernel-keyed —
/// unforgeable off-broker; the token carries no secret material).
fn minted_mac(secret_key: &str, binding_id: &str, audience: &str, expires_at_ms: u64) -> String {
    let material = format!("{secret_key}\u{1f}{binding_id}\u{1f}{audience}\u{1f}{expires_at_ms}");
    hh_identity::idp::idp_digest("secret.minted", material.as_bytes())[..32].to_string()
}

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
    /// The bound proof-of-possession key coordinate — `Some(_)` only on a
    /// `dpop`-constrained channel (R3.7): `bind` verified the holder's
    /// signature over `dpop::bind_preimage` and pinned
    /// `proof.sender_key()`; every subsequent delivery verb requires a
    /// proof *by that key* over the verb's own preimage.
    pub sender_key: Option<String>,
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
            (
                "sender_key",
                self.sender_key
                    .as_ref()
                    .map(|k| Json::str(k.clone()))
                    .unwrap_or(Json::Null),
            ),
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
    /// The holder's `dpop` proof-of-possession — required when the channel
    /// declares `sender_constraint = dpop` (R3.7): `bind` verifies it over
    /// `dpop::bind_preimage(channel, holder, env, mode, decision_ref)` and
    /// pins `proof.sender_key()` on the binding; ignored for `audience`/`none`.
    pub dpop_proof: Option<DpopProof>,
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
    /// The environment handle the request arrived on — checked against the
    /// binding's `env_handle` (S2.4 anti-laundering, ADR-0266 D3: a sentinel
    /// is bound to *one* env; a placeholder copied to another environment's
    /// wire is `out_of_scope`, never honoured).
    pub env_handle: String,
    /// The destination host (matched against the binding's `destinations` by
    /// the interim `ResourcePattern` rules).
    pub destination: String,
    /// The HTTP method, when the request has one.
    pub method: Option<String>,
    /// The request path, when there is one — checked for ambiguity
    /// (dot-segments, encoded separators, backslashes → `ambiguous_path`).
    pub path: Option<String>,
    /// The request's `dpop` proof-of-possession — required when the
    /// binding's channel declares `sender_constraint = dpop` (R3.7): the
    /// signature must verify under the binding's pinned `sender_key` over
    /// `dpop::request_preimage(binding, method, destination, path,
    /// effect_id)`; anything else is `sender_constraint_unmet`.
    pub dpop_proof: Option<DpopProof>,
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

/// The `revoke` outcome — the bindings the revoke dropped plus the
/// destination-side revocation intents the mediator must dispatch (LT-05's
/// "wrapped values are revoked at the destination" — S2.4, ADR-0266 D4).
#[derive(Debug, Clone, PartialEq)]
pub struct RevokeOutcome {
    /// The binding ids revoked (empty + `already` when idempotent).
    pub revoked: Vec<String>,
    /// Whether the target was already dead (no event emitted — idempotent).
    pub already: bool,
    /// The destination-side revocation intents (one per dropped binding ×
    /// channel `DestinationBinding` declaring `revocation_path`). The
    /// mediator dispatches a POST to each; the intent is content-free (the
    /// binding coordinate, never the value).
    pub intents: Vec<RevokeIntent>,
}

/// One destination-side revocation intent — `POST revocation_path` on the
/// destination carrying the binding coordinate (never the credential).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevokeIntent {
    /// The dropped binding.
    pub binding_id: String,
    /// The channel.
    pub channel_id: String,
    /// The destination's host pattern (the POST's destination coordinate).
    pub host_pattern: String,
    /// The destination's declared port, when constrained (`None` = the
    /// scheme default — the dispatcher resolves the concrete port).
    pub port: Option<u16>,
    /// The revocation path on that destination.
    pub revocation_path: String,
}

/// `mint`/`minted_scoped` — a scoped token the broker mints for one audience
/// and one TTL (S2.4; §5g.3 `minted_scoped`). The token is a kernel-keyed
/// capability: `mh_mint:v1:<binding_id>:<expires_ms>:<mac>` where `mac =
/// idp_digest("secret.minted", secret_key ∥ binding_id ∥ audience ∥
/// expires_ms)` — kernel-keyed, unforgeable off-broker, and carrying **no
/// secret material** (the value never enters the token). `verify_minted`
/// re-derives the MAC, checks the audience, the expiry (LT-05's "minted
/// tokens fail after expiry") and the binding's liveness (a revoked binding
/// revokes its tokens).
#[derive(Debug, Clone, PartialEq)]
pub struct MintedToken {
    /// The token spelling (the carrier value the mediator injects).
    pub token: String,
    /// The binding it was minted under.
    pub binding_id: String,
    /// The audience it is scoped to.
    pub audience: String,
    /// Expiry (run ms — a `Store::now_ms` coordinate, never wall-clock).
    pub expires_at_ms: u64,
    /// The durable `security.credential.used` row minting produced (SV-5 —
    /// durable before the token is visible).
    pub used_event_id: String,
}

/// `verify_minted`'s closed verdict sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MintedVerdict {
    /// The token verifies — carries the channel_id it is scoped to.
    Valid,
    /// At-or-past `expires_at_ms` — the expiry instant itself is dead
    /// (the `now < exp` reading, RFC 7519's convention; a zero-TTL mint
    /// is born expired — R3.1, the same-ms boundary the r2_19 flake hit).
    Expired,
    /// The backing binding is dead (revoked/expired/rotated).
    Revoked,
    /// Unknown spelling, MAC mismatch, or audience mismatch.
    Invalid,
}

impl MintedVerdict {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            MintedVerdict::Valid => "valid",
            MintedVerdict::Expired => "expired",
            MintedVerdict::Revoked => "revoked",
            MintedVerdict::Invalid => "invalid",
        }
    }
}

/// The kernel-held minted record (in-memory; the token itself is the
/// capability — the record is the liveness/audience check's table).
#[derive(Debug, Clone)]
pub struct MintedRecord {
    /// The binding the token was minted under.
    pub binding_id: String,
    /// The channel (the verify verdict's scope).
    pub channel_id: String,
    /// The audience the token is scoped to.
    pub audience: String,
    /// Expiry (run ms).
    pub expires_at_ms: u64,
    /// The bound `sender_key` the binding carried at mint (`Some` on
    /// `dpop` channels — `verify_minted` requires a presentation proof by
    /// that key; R3.7).
    pub sender_key: Option<String>,
}

/// `deliver_wrapped`'s request — the `wrapped_long_lived` delivery
/// descriptor (R3.7; DF-S1.13-4). Distinct from `RequestDescriptor`: a
/// wrap is installed *at a destination binding*, it is not an egress
/// request — there is no method/path to prove over (the destination is
/// what the `dpop` proof binds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapDeliveryRequest {
    /// The `action.effect.*` scope the wrap is charged to (the `used` row
    /// references it — SV-5).
    pub effect_id: String,
    /// The env the wrap installs on — checked against the binding's
    /// `env_handle` (the same anti-laundering rule `mediate` enforces).
    pub env_handle: String,
    /// The destination the wrap is delivered to (∈ the binding's declared
    /// destinations — `out_of_scope` otherwise).
    pub destination: String,
    /// The holder's `dpop` proof — required when the channel declares
    /// `sender_constraint = dpop`: the signature must verify under the
    /// binding's pinned `sender_key` over
    /// `dpop::request_preimage(binding, None, destination, None, effect)`.
    pub dpop_proof: Option<DpopProof>,
}

/// The record of a landed `wrapped_long_lived` delivery — coordinates
/// plus the wrapped value itself (the wrap; **caller-held** — the kernel
/// hands it to the destination seam the caller represents; no record this
/// crate emits carries it — SV-2).
#[derive(Debug, Clone, PartialEq)]
pub struct WrappedDelivery {
    /// The binding the wrap delivered under.
    pub binding_id: String,
    /// The channel.
    pub channel_id: String,
    /// The destination the wrap was delivered to.
    pub destination: String,
    /// The effect the delivery was charged to.
    pub effect_id: String,
    /// The `wrap-<n>` registration coordinate.
    pub wrap_id: String,
    /// The registered compensating-revocation plan coordinate —
    /// `idp_digest("credential.wrap.compensation", binding ∥ destination ∥
    /// revocation_paths)`, durable on the `security.credential.wrapped`
    /// row *before* the value crossed (ADR-0032's `prepare` hook at the
    /// credential plane; the plan is the `RevokeIntent` set `revoke`
    /// dispatches).
    pub compensation_plan_id: String,
    /// The durable `security.credential.wrapped` row's event id (the
    /// registration).
    pub wrapped_event_id: String,
    /// The durable `security.credential.used` row's event id (the
    /// delivery itself).
    pub used_event_id: String,
    /// The wrapped credential — the resolved secret handed to the
    /// destination (the mode's own semantics: the value travels wrapped
    /// to the destination, never a placeholder; caller-held, never
    /// durable).
    pub value: String,
}

/// `probe_wrapped`'s closed verdict — the wrap's application at the
/// destination as the kernel's own records establish it. This is a pure
/// fold over broker state (a read — the probe mints no row): `applied`
/// while the backing binding is live, `not_applied` once the binding is
/// dead (the registered compensating revocation has run — `revoke`'s
/// destination intents are its dispatch half, LT-05), `undeterminable`
/// when no wrap was ever delivered on the binding. The verdict spellings
/// mirror `hh-ledger`'s `ProbeVerdict` (`applied | not_applied |
/// undeterminable`) — the probe is honest as a kernel-state verdict, not
/// a live destination query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapProbe {
    /// The wrap is applied at the destination (the binding is live).
    Applied,
    /// The wrap is not applied — the compensating revocation ran (the
    /// backing binding is dead).
    NotApplied,
    /// No wrap was ever delivered on this binding — nothing to probe.
    Undeterminable,
}

impl WrapProbe {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            WrapProbe::Applied => "applied",
            WrapProbe::NotApplied => "not_applied",
            WrapProbe::Undeterminable => "undeterminable",
        }
    }
}

/// The kernel-held wrap registration (in-memory; the durable fact is the
/// `security.credential.wrapped` row — this map backs `probe_wrapped`'s
/// `undeterminable` leg).
#[derive(Debug, Clone)]
pub struct WrapRecord {
    /// The registration coordinate.
    pub wrap_id: String,
    /// The destination the wrap was delivered to.
    pub destination: String,
    /// The registered compensating-revocation plan coordinate.
    pub compensation_plan_id: String,
}

/// `virtualize_for_fork`'s result (LT-09) — the old→new placeholder spelling
/// map the snapshot rewriter applies plus the freshly-bound binding ids.
#[derive(Debug, Clone, PartialEq)]
pub struct ForkVirtualization {
    /// `old placeholder spelling → new placeholder spelling` (the fork's
    /// snapshot rewrites each old spelling to the fresh one — a fork never
    /// inherits the parent's nonce).
    pub rewrites: std::collections::BTreeMap<String, String>,
    /// The newly-minted binding ids on the fork's env handle.
    pub bindings: Vec<String>,
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
    /// Kernel-held minted-scoped records (`token spelling → record`) — the
    /// `verify_minted` liveness/audience table (S2.4). In-memory; a dead
    /// binding's records are dropped on revoke/rotate/run-terminal.
    minted: BTreeMap<String, MintedRecord>,
    /// Kernel-held wrap registrations (`binding_id → record`) — the
    /// `probe_wrapped` verdict table (R3.7). A dead binding's record is
    /// **retained**: the wrap *was* delivered — the probe answers
    /// `not_applied`, never `undeterminable` (LT-05's revoked-wrap leg).
    wraps: BTreeMap<String, WrapRecord>,
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
            minted: BTreeMap::new(),
            wraps: BTreeMap::new(),
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
    /// `dpop`-declaring channel refuses `SenderConstraintUnmet` on every
    /// mode (no pure-std proof-of-possession verifier); `audience` verifies
    /// (destination scope for injected modes, the minted token's audience
    /// binding + `verify_minted` for `minted_scoped`); a canary channel is
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
        // A declared sender constraint must *verify* — never silently admit
        // one the runtime cannot check (R2.10/R3.7; DF-S1.13-1):
        //   `audience` — verifiable offline. A `minted_scoped` bind admits:
        //     the minted token binds the audience (`mint` scopes it to the
        //     binding's declared destinations; `verify_minted` checks it
        //     destination-side). Injected modes admit: the audience *is*
        //     the destination, fenced by the scope check below.
        //   `dpop` — proof-of-possession verifies through `dpop` (R3.7;
        //     ADR-0359's `ring` admission): `bind` requires the holder's
        //     signature over `bind_preimage(channel, holder, env, mode,
        //     decision_ref)` and pins `proof.sender_key()` on the binding.
        //     An absent proof or an unverifying signature refuses
        //     `sender_constraint_unmet` — the fail-closed posture stands.
        let sender_key = if ch.spec.sender_constraint == SenderConstraint::Dpop {
            let Some(proof) = req.dpop_proof.as_ref() else {
                return Err(Refused::new(
                    RefusedCode::SenderConstraintUnmet,
                    format!(
                        "channel {}'s sender_constraint dpop requires a proof — none presented",
                        req.channel_id
                    ),
                ));
            };
            let preimage = dpop::bind_preimage(
                &req.channel_id,
                &req.holder,
                &req.env_handle_ref,
                req.mode.as_str(),
                &req.monitor_decision_ref,
            );
            if !proof.verify(&preimage) {
                return Err(Refused::new(
                    RefusedCode::SenderConstraintUnmet,
                    format!(
                        "channel {}'s dpop proof does not verify over the bind preimage",
                        req.channel_id
                    ),
                ));
            }
            Some(proof.sender_key())
        } else {
            None
        };
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
            sender_key.as_deref(),
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
            sender_key,
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
        // out_of_scope → ambiguous_path → canary → sender_constraint_unmet
        // (R3.7) → decision_missing → broker_unavailable.
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
        // Env-handle check (S2.4, ADR-0266 D3) — a sentinel is bound to one
        // environment; presenting it on another env's wire is out_of_scope
        // (non-transferable placeholders — LT-02's laundering half).
        if binding.env_handle != request.env_handle {
            return deny(
                self,
                store,
                lease,
                RefusedCode::OutOfScope,
                format!(
                    "binding {binding_id} is bound to env {} — not {}",
                    binding.env_handle, request.env_handle
                ),
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
        // Sender constraint (R3.7 — `dpop`; ADR-0359): the request must
        // carry a proof *by the binding's pinned key* over
        // `request_preimage(binding, method, destination, path, effect)` —
        // an absent proof, a key that differs from the bound `sender_key`
        // (a binding that predates the pin or names another key), or an
        // unverifying signature all refuse `sender_constraint_unmet`,
        // durable-denied before the refusal is visible (CC3 — never a
        // skipped check).
        if ch.spec.sender_constraint == SenderConstraint::Dpop {
            let Some(proof) = request.dpop_proof.as_ref() else {
                return deny(
                    self,
                    store,
                    lease,
                    RefusedCode::SenderConstraintUnmet,
                    format!("binding {binding_id}'s dpop channel requires a request proof"),
                );
            };
            if binding.sender_key.as_deref() != Some(proof.sender_key().as_str()) {
                return deny(
                    self,
                    store,
                    lease,
                    RefusedCode::SenderConstraintUnmet,
                    format!("request proof key does not match binding {binding_id}'s sender_key"),
                );
            }
            let preimage = dpop::request_preimage(
                binding_id,
                request.method.as_deref(),
                &request.destination,
                request.path.as_deref(),
                &request.effect_id,
            );
            if !proof.verify(&preimage) {
                return deny(
                    self,
                    store,
                    lease,
                    RefusedCode::SenderConstraintUnmet,
                    format!("request dpop proof does not verify on binding {binding_id}"),
                );
            }
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

    // ── mint (S2.4 — minted_scoped) ──────────────────────────────────────

    /// `mint(binding_id, audience, ttl_ms, dpop_proof)` — the
    /// `minted_scoped` verb (S2.4; ADR-0266 D4). The minted token is a
    /// kernel-keyed capability —
    /// `mh_mint:v1:<binding_id>:<expires_ms>:<mac>` — carrying **no secret
    /// material**; `verify_minted` re-derives the MAC and checks
    /// audience/expiry/binding-liveness (LT-05: expired ⇒ `Expired`,
    /// revoked binding ⇒ `Revoked`). A `security.credential.used` row is
    /// durable before the token is returned (SV-5 — minting *is* the use).
    ///
    /// A `sender_constraint = dpop` channel verifies through `dpop` (R3.7;
    /// ADR-0359): `mint` requires a bound-key signature over
    /// `mint_preimage(binding, audience)` and the token records the bound
    /// `sender_key` — `verify_minted` then requires a presentation proof
    /// by that key (an absent/mismatched/unverifying proof refuses
    /// `SenderConstraintUnmet`, never mints an unverifiable claim).
    /// `audience` channels verify: the requested audience must sit inside
    /// the binding's declared destinations (`out_of_scope` otherwise) —
    /// the token *is* the audience binding `verify_minted` checks.
    #[allow(clippy::too_many_arguments)] // the verb's record is the §5g.3 mint shape — the arity is the record's.
    pub fn mint(
        &mut self,
        store: &mut Store,
        run_id: &str,
        lease: &Lease,
        binding_id: &str,
        audience: &str,
        ttl_ms: u64,
        dpop_proof: Option<&DpopProof>,
    ) -> Result<MintedToken, BrokerError> {
        let Some(binding) = self.bindings.get(binding_id).cloned() else {
            return Err(BrokerError::Refused(Refused::new(
                RefusedCode::Revoked,
                format!("no binding {binding_id}"),
            )));
        };
        if !binding.is_live() || self.revoked_channels.contains(&binding.channel_id) {
            return Err(BrokerError::Refused(Refused::new(
                RefusedCode::Revoked,
                format!("binding {binding_id} is revoked"),
            )));
        }
        let ch = self
            .channels
            .get(&binding.channel_id)
            .ok_or_else(|| BrokerError::UnknownChannel {
                channel_id: binding.channel_id.clone(),
            })?
            .clone();
        if ch.spec.sender_constraint == SenderConstraint::Dpop {
            // R3.7 — the mint requires a bound-key signature over
            // `mint_preimage(binding, audience)`; the token then records
            // the bound `sender_key` for `verify_minted`'s presentation
            // check. An absent/mismatched/unverifying proof refuses
            // `sender_constraint_unmet` — never an unverifiable claim.
            let Some(proof) = dpop_proof else {
                return Err(BrokerError::Refused(Refused::new(
                    RefusedCode::SenderConstraintUnmet,
                    format!("binding {binding_id}'s dpop channel requires a mint proof"),
                )));
            };
            if binding.sender_key.as_deref() != Some(proof.sender_key().as_str()) {
                return Err(BrokerError::Refused(Refused::new(
                    RefusedCode::SenderConstraintUnmet,
                    format!("mint proof key does not match binding {binding_id}'s sender_key"),
                )));
            }
            if !proof.verify(&dpop::mint_preimage(binding_id, audience)) {
                return Err(BrokerError::Refused(Refused::new(
                    RefusedCode::SenderConstraintUnmet,
                    format!("mint dpop proof does not verify on binding {binding_id}"),
                )));
            }
        }
        if ch.spec.canary {
            let leak = Leak::at_mediation(
                format!("mint:{}", binding.channel_id),
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
            .map_err(|e| BrokerError::Ledger {
                detail: format!("leak_detected row not durable: {e}"),
            })?;
            return Err(BrokerError::Refused(Refused::new(
                RefusedCode::Canary,
                format!("channel {} is a canary — never minted", binding.channel_id),
            )));
        }
        // R2.10 (DF-S1.13-1) — `sender_constraint = audience` is verified:
        // the requested audience must sit inside the binding's declared
        // destination set (`bind` already fenced it ⊆ the channel's
        // `destinations`). Minting a token for an audience the binding
        // cannot deliver to would mint an unverifiable claim — refused
        // `out_of_scope`, never silently scoped.
        if ch.spec.sender_constraint == SenderConstraint::Audience
            && !binding.destinations.contains(audience)
        {
            return Err(BrokerError::Refused(Refused::new(
                RefusedCode::OutOfScope,
                format!("audience {audience} is outside binding {binding_id}'s destinations"),
            )));
        }
        let expires_at_ms = store.now_ms().saturating_add(ttl_ms);
        let mac = minted_mac(&self.secret_key, binding_id, audience, expires_at_ms);
        let token = format!("mh_mint:v1:{binding_id}:{expires_at_ms}:{mac}");
        // The `used` row is durable before the token is visible.
        let used_event_id = store.alloc_id("evt");
        self.append_security_with_id(
            store,
            run_id,
            lease,
            &used_event_id,
            "security.credential.used",
            events::used_payload(
                binding_id,
                &binding.channel_id,
                binding.channel_revision,
                audience,
                &binding.effect_id,
                "allow",
                Some(&binding.decision_ref),
                ProvidedSecret { provided: true },
            ),
        )
        .map_err(|e| BrokerError::Ledger {
            detail: format!("used row not durable: {e}"),
        })?;
        self.minted.insert(
            token.clone(),
            MintedRecord {
                binding_id: binding_id.to_string(),
                channel_id: binding.channel_id.clone(),
                audience: audience.to_string(),
                expires_at_ms,
                sender_key: binding.sender_key.clone(),
            },
        );
        Ok(MintedToken {
            token,
            binding_id: binding_id.to_string(),
            audience: audience.to_string(),
            expires_at_ms,
            used_event_id,
        })
    }

    /// `verify_minted(token, audience, now_ms, dpop_proof)` — the
    /// destination-side (or test-side) check: re-derive the MAC, confirm
    /// the audience, the expiry (LT-05) and the backing binding's
    /// liveness. A token minted on a `dpop` channel additionally requires
    /// a presentation proof *by the recorded `sender_key`* over
    /// `dpop::presentation_preimage(token, audience)` (R3.7 — the proof is
    /// bound to *this* token spelling, never replayable); an absent,
    /// mismatched or unverifying proof is `Invalid` — never a
    /// fabricated accept.
    pub fn verify_minted(
        &self,
        token: &str,
        audience: &str,
        now_ms: u64,
        dpop_proof: Option<&DpopProof>,
    ) -> MintedVerdict {
        let Some(rec) = self.minted.get(token) else {
            return MintedVerdict::Invalid;
        };
        let expected = minted_mac(
            &self.secret_key,
            &rec.binding_id,
            &rec.audience,
            rec.expires_at_ms,
        );
        if token
            != format!(
                "mh_mint:v1:{}:{}:{}",
                rec.binding_id, rec.expires_at_ms, expected
            )
        {
            return MintedVerdict::Invalid;
        }
        if rec.audience != audience {
            return MintedVerdict::Invalid;
        }
        // `>=`, not `>`: the token dies *at* `expires_at_ms`, matching the
        // binding-side convention (`live_at` is `now < expires`; the
        // `mediate` path's `expires_at <= ts_now` string compare). A
        // strict `>` let a zero-TTL mint verify `Valid` for the whole
        // same-millisecond window — the r2_19
        // `credential_mediator_oauth_resolves_and_refuses` flake (R3.1;
        // six recorded CI firings R2.22–R2.26).
        if now_ms >= rec.expires_at_ms {
            return MintedVerdict::Expired;
        }
        let live = self
            .bindings
            .get(&rec.binding_id)
            .is_some_and(|b| b.is_live())
            && !self.revoked_channels.contains(&rec.channel_id);
        if !live {
            return MintedVerdict::Revoked;
        }
        // R3.7 — a `dpop`-bound token verifies only under a presentation
        // proof by the recorded `sender_key` over
        // `presentation_preimage(token, audience)`: the proof is bound to
        // this token spelling, so a stolen token presented under another
        // key (or no key) is `Invalid`.
        if rec.sender_key.is_some() {
            let Some(proof) = dpop_proof else {
                return MintedVerdict::Invalid;
            };
            if rec.sender_key.as_deref() != Some(proof.sender_key().as_str()) {
                return MintedVerdict::Invalid;
            }
            if !proof.verify(&dpop::presentation_preimage(token, audience)) {
                return MintedVerdict::Invalid;
            }
        }
        MintedVerdict::Valid
    }

    // ── the Stage-2 claim path ───────────────────────────────────────────

    /// `drain_claim(claim_id)` — the mediator's consume-once drain of a
    /// staged `Delivery`'s kernel-held value (S2.4). Returns `(binding_id,
    /// value)` on the first drain; `None` thereafter and for any unknown
    /// claim (a claim is drained exactly once — the value crosses the wire
    /// in exactly one request).
    pub fn drain_claim(&mut self, claim_id: &str) -> Option<(String, String)> {
        self.pending.remove(claim_id)
    }

    /// Peek at a claim's binding without draining (the mediator's
    /// pre-substitution check).
    pub fn claim_binding(&self, claim_id: &str) -> Option<&str> {
        self.pending.get(claim_id).map(|(b, _)| b.as_str())
    }

    /// `mediate_sentinel(sentinel, request)` — the mediator-side entry:
    /// resolve a placeholder spelling to its binding, then run the
    /// `mediate` pipeline (revoked → expired → env-handle → out_of_scope →
    /// ambiguous_path → canary → decision → resolve → used → stage). A
    /// sentinel that names no live binding on this env is `out_of_scope`
    /// (non-transferable — SV-10's Stage-2 half; ADR-0266 D3) — and the
    /// refusal is audited with the denied row *before* it is visible.
    pub fn mediate_sentinel(
        &mut self,
        store: &mut Store,
        run_id: &str,
        lease: &Lease,
        sentinel: &str,
        request: &RequestDescriptor,
    ) -> Result<MediationOutcome, BrokerError> {
        let binding_id = self
            .placeholders
            .iter()
            .find(|(_, p)| p.spelling == sentinel)
            .map(|(b, _)| b.clone());
        let Some(binding_id) = binding_id else {
            // The sentinel names nothing — a forged/foreign/stale
            // placeholder is denied, durably, with no binding coordinate.
            let payload = events::denied_payload(
                None,
                &request.destination,
                RefusedCode::OutOfScope,
                None,
                Some(&request.effect_id),
            );
            self.append_security(store, run_id, lease, "security.credential.denied", payload)
                .map_err(|e| BrokerError::Ledger {
                    detail: format!("denied row not durable: {e}"),
                })?;
            return Ok(MediationOutcome::Refused(Refused::new(
                RefusedCode::OutOfScope,
                "sentinel resolves to no live binding".to_string(),
            )));
        };
        self.mediate(store, run_id, lease, &binding_id, request)
    }

    /// `virtualize_for_fork(src_env, dst_env)` — LT-09: a forked
    /// environment's snapshot may carry the parent's placeholder spellings;
    /// the fork gets **fresh** placeholders bound to fresh bindings on
    /// `dst_env` (the parent's nonce never transfers). Returns the
    /// old→new spelling map the snapshot rewriter applies, plus the new
    /// binding ids (each gets a `security.credential.bound` row — the
    /// fork's bindings are their own lifecycle).
    pub fn virtualize_for_fork(
        &mut self,
        store: &mut Store,
        run_id: &str,
        lease: &Lease,
        src_env: &str,
        dst_env: &str,
    ) -> Result<ForkVirtualization, BrokerError> {
        let src_bindings: Vec<CredentialBinding> = self
            .bindings
            .values()
            .filter(|b| b.is_live() && b.env_handle == src_env)
            .cloned()
            .collect();
        let mut rewrites = std::collections::BTreeMap::new();
        let mut new_ids = Vec::new();
        for src in src_bindings {
            let new_id = self.alloc("bnd");
            let mut nb = src.clone();
            nb.binding_id = new_id.clone();
            nb.env_handle = dst_env.to_string();
            nb.state = BindingState::Bound;
            nb.issued_at = store.ts_now();
            // The fork's binding keeps the parent's expiry ceiling
            // (never *extends* it — SV-6's floor holds for the fork too).
            let payload = events::bound_payload(
                &new_id,
                &nb.channel_id,
                nb.channel_revision,
                &nb.holder,
                dst_env,
                nb.mode.as_str(),
                &nb.decision_ref,
                &nb.expires_at,
                nb.sender_key.as_deref(),
            );
            self.append_security(store, run_id, lease, "security.credential.bound", payload)
                .map_err(|e| BrokerError::Ledger {
                    detail: format!("bound row not durable: {e}"),
                })?;
            let old_spell = self
                .placeholders
                .get(&src.binding_id)
                .map(|p| p.spelling.clone());
            let ph = Placeholder::mint(run_id, &nb);
            if let Some(old) = old_spell {
                rewrites.insert(old, ph.spelling.clone());
            }
            self.placeholders.insert(new_id.clone(), ph);
            self.bindings.insert(new_id.clone(), nb);
            new_ids.push(new_id);
        }
        Ok(ForkVirtualization {
            rewrites,
            bindings: new_ids,
        })
    }

    // ── deliver_wrapped / probe_wrapped (R3.7 — wrapped_long_lived) ─────

    /// `deliver_wrapped(binding_id, request)` — the `wrapped_long_lived`
    /// delivery half (R3.7; DF-S1.13-4's last residual; ADR-0360). The
    /// wrap's compensating revocation is *registered* durable-before-
    /// visible: `security.credential.wrapped{wrap_id, destination,
    /// compensation_plan_id}` lands in the same append as the delivery's
    /// `security.credential.used` — the ADR-0032 `prepare` hook at the
    /// credential plane (`compensation_plan_id` is the derived idp/1
    /// coordinate naming the `RevokeIntent` set `revoke` dispatches to the
    /// destination's `revocation_path`s — the S2.4 machinery). Only then
    /// is the wrapped value returned — the value is caller-held, never
    /// recorded (SV-2).
    ///
    /// The refusal order mirrors `mediate`: revoked → mode_not_allowed →
    /// env/destination out_of_scope → canary → sender_constraint_unmet →
    /// decision_missing → broker_unavailable → audit_unavailable. The
    /// `isolation ≥ process_sandbox` gate stays `bind`'s (a wrap-mode
    /// binding cannot exist below it — `EnvironmentNotIsolated` fired
    /// there); `probe_wrapped` reports the wrap's applied state.
    pub fn deliver_wrapped(
        &mut self,
        store: &mut Store,
        run_id: &str,
        lease: &Lease,
        binding_id: &str,
        request: &WrapDeliveryRequest,
    ) -> Result<WrappedDelivery, BrokerError> {
        let refuse = |code: RefusedCode, detail: String| {
            Err(BrokerError::Refused(Refused::new(code, detail)))
        };
        let Some(binding) = self.bindings.get(binding_id).cloned() else {
            return refuse(RefusedCode::Revoked, format!("no binding {binding_id}"));
        };
        if !binding.is_live() || self.revoked_channels.contains(&binding.channel_id) {
            return refuse(
                RefusedCode::Revoked,
                format!("binding {binding_id} is revoked"),
            );
        }
        if binding.mode != SecretTransport::WrappedLongLived {
            return refuse(
                RefusedCode::ModeNotAllowed,
                format!(
                    "binding {binding_id}'s mode is {} — not wrapped_long_lived",
                    binding.mode.as_str()
                ),
            );
        }
        if binding.env_handle != request.env_handle {
            return refuse(
                RefusedCode::OutOfScope,
                format!(
                    "binding {binding_id} is bound to env {} — not {}",
                    binding.env_handle, request.env_handle
                ),
            );
        }
        if !binding.destinations.contains(&request.destination) {
            return refuse(
                RefusedCode::OutOfScope,
                format!(
                    "destination {} is outside binding {binding_id}'s scope",
                    request.destination
                ),
            );
        }
        let ch = self
            .channels
            .get(&binding.channel_id)
            .ok_or_else(|| BrokerError::UnknownChannel {
                channel_id: binding.channel_id.clone(),
            })?
            .clone();
        if ch.spec.canary {
            let leak = Leak::at_mediation(
                format!("deliver_wrapped:{}", binding.channel_id),
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
            .map_err(|e| BrokerError::Ledger {
                detail: format!("leak_detected row not durable: {e}"),
            })?;
            return refuse(
                RefusedCode::Canary,
                format!("channel {} is a canary — never wrapped", binding.channel_id),
            );
        }
        if ch.spec.sender_constraint == SenderConstraint::Dpop {
            let Some(proof) = request.dpop_proof.as_ref() else {
                return refuse(
                    RefusedCode::SenderConstraintUnmet,
                    format!("binding {binding_id}'s dpop channel requires a wrap proof"),
                );
            };
            if binding.sender_key.as_deref() != Some(proof.sender_key().as_str()) {
                return refuse(
                    RefusedCode::SenderConstraintUnmet,
                    format!("wrap proof key does not match binding {binding_id}'s sender_key"),
                );
            }
            if !proof.verify(&dpop::request_preimage(
                binding_id,
                None,
                &request.destination,
                None,
                &request.effect_id,
            )) {
                return refuse(
                    RefusedCode::SenderConstraintUnmet,
                    format!("wrap dpop proof does not verify on binding {binding_id}"),
                );
            }
        }
        // PDP re-check — a covering decided-allow must still exist.
        let view = decisions::fold(store.events(run_id).map_err(|e| BrokerError::Ledger {
            detail: format!("events fold: {e}"),
        })?);
        if decisions::covering_secret_access(&view, None, &binding.channel_id, &binding.holder)
            .is_none()
        {
            return refuse(
                RefusedCode::DecisionMissing,
                format!(
                    "no covering decided{{allow}} for secret_access on {}",
                    binding.channel_id
                ),
            );
        }
        // Resolve the source — the wrapped value is the real credential
        // (the mode's semantics), fail closed.
        let value = self.resolver.resolve(&ch.spec.source).map_err(|e| {
            BrokerError::Refused(Refused::new(
                RefusedCode::BrokerUnavailable,
                format!("source {} unavailable: {}", e.kind, e.detail),
            ))
        })?;
        // The compensating-revocation plan coordinate — the idp/1-derived
        // name of "POST every channel `revocation_path` + the kernel
        // `revoked` row" (the `RevokeIntent` set `revoke` produces for
        // this binding). Deterministic: the plan is a pure function of
        // the binding + the channel's declared revocation paths.
        let mut material = format!("{binding_id}\u{1f}{}\u{1f}", request.destination);
        let mut paths: Vec<&str> = ch
            .spec
            .destinations
            .iter()
            .filter_map(|d| d.revocation_path.as_deref())
            .collect();
        paths.sort_unstable();
        for p in paths {
            material.push_str(p);
            material.push('\u{1f}');
        }
        let compensation_plan_id =
            hh_identity::idp::idp_id("credential.wrap.compensation", material.as_bytes());
        let wrap_id = self.alloc("wrap");
        let wrapped_event_id = store.alloc_id("evt");
        let used_event_id = store.alloc_id("evt");
        // The registration and the delivery are one durable fact (the
        // `rotate` batch precedent): `wrapped{compensation_plan_id}` +
        // `used`, both durable before the value is visible.
        let mut wrapped_ev = self.kernel_event(
            store,
            run_id,
            "security.credential.wrapped",
            events::wrapped_payload(
                &wrap_id,
                binding_id,
                &binding.channel_id,
                binding.channel_revision,
                &request.destination,
                &request.effect_id,
                &binding.expires_at,
                binding.sender_key.as_deref(),
                &compensation_plan_id,
            ),
        )?;
        wrapped_ev.event_id = wrapped_event_id.clone();
        let mut used_ev = self.kernel_event(
            store,
            run_id,
            "security.credential.used",
            events::used_payload(
                binding_id,
                &binding.channel_id,
                binding.channel_revision,
                &request.destination,
                &request.effect_id,
                "allow",
                Some(&binding.decision_ref),
                ProvidedSecret { provided: true },
            ),
        )?;
        used_ev.event_id = used_event_id.clone();
        store
            .append(run_id, lease, vec![wrapped_ev, used_ev])
            .map_err(|e| BrokerError::Ledger {
                detail: format!("wrapped/used rows not durable: {e}"),
            })?;
        self.wraps.insert(
            binding_id.to_string(),
            WrapRecord {
                wrap_id: wrap_id.clone(),
                destination: request.destination.clone(),
                compensation_plan_id: compensation_plan_id.clone(),
            },
        );
        if let Some(b) = self.bindings.get_mut(binding_id) {
            b.state = BindingState::Active;
        }
        Ok(WrappedDelivery {
            binding_id: binding_id.to_string(),
            channel_id: binding.channel_id.clone(),
            destination: request.destination.clone(),
            effect_id: request.effect_id.clone(),
            wrap_id,
            compensation_plan_id,
            wrapped_event_id,
            used_event_id,
            value,
        })
    }

    /// `probe_wrapped(binding_id)` — the wrap's application verdict at the
    /// destination (LT-05's revoked-wrap leg): a pure fold over the
    /// wrap-registration table and the binding's liveness — `Applied`
    /// while the backing binding lives, `NotApplied` once the registered
    /// compensating revocation has run (the binding is dead — `revoke`'s
    /// destination intents are its dispatch half), `Undeterminable` when
    /// no wrap was delivered on the binding.
    pub fn probe_wrapped(&self, binding_id: &str) -> WrapProbe {
        if !self.wraps.contains_key(binding_id) {
            return WrapProbe::Undeterminable;
        }
        match self.bindings.get(binding_id) {
            Some(b) if b.is_live() => WrapProbe::Applied,
            _ => WrapProbe::NotApplied,
        }
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
                            intents: Vec::new(),
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
                            intents: Vec::new(),
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
        let intents = self.revoke_intents(&dropped);
        for bid in &dropped {
            if let Some(b) = self.bindings.get_mut(bid) {
                b.state = BindingState::Revoked;
            }
            self.drop_dead(bid);
        }
        if let Some(cid) = channel_id {
            self.revoked_channels.insert(cid);
        }
        Ok(RevokeOutcome {
            revoked: dropped,
            already: false,
            intents,
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
            self.drop_dead(bid);
        }
        Ok(new_revision)
    }

    /// Drop a dead binding's staged claims — a revoked binding's staged
    /// delivery is undeliverable (SV-6). The minted records are **retained**:
    /// `verify_minted` consults binding liveness (a dead binding's token is
    /// `Revoked`, not `Invalid` — the difference is an audit fact), and the
    /// mask set covers every minted spelling, live or dead (a dead token on
    /// a durable surface is still a leak — LT-04/LT-05).
    fn drop_dead(&mut self, binding_id: &str) {
        self.pending.retain(|_, (b, _)| b != binding_id);
    }

    /// The destination-side revocation intents for a dropped set — one per
    /// binding × channel `DestinationBinding` declaring `revocation_path`
    /// (LT-05's "wrapped values are revoked at the destination").
    fn revoke_intents(&self, dropped: &[String]) -> Vec<RevokeIntent> {
        let mut out = Vec::new();
        for bid in dropped {
            let Some(b) = self.bindings.get(bid) else {
                continue;
            };
            let Some(ch) = self.channels.get(&b.channel_id) else {
                continue;
            };
            for d in &ch.spec.destinations {
                if let Some(p) = &d.revocation_path {
                    out.push(RevokeIntent {
                        binding_id: bid.clone(),
                        channel_id: b.channel_id.clone(),
                        host_pattern: d.host_pattern.clone(),
                        port: d.port,
                        revocation_path: p.clone(),
                    });
                }
            }
        }
        out
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
            // Minted-scoped tokens issued for this channel's bindings are
            // credential material too — mask them (live or dead: a dead
            // token on a durable surface is still a leak — LT-04/LT-05).
            for (spell, rec) in &self.minted {
                if rec.channel_id == *cid {
                    let rev = self
                        .bindings
                        .get(&rec.binding_id)
                        .map(|b| b.channel_revision)
                        .unwrap_or(0);
                    ms.insert(MaskEntry {
                        channel_id: cid.clone(),
                        revision: rev,
                        value: spell.clone(),
                        fingerprint: self.fingerprint(spell, cid, rev),
                        live: false,
                        placeholder: None,
                    });
                }
            }
        }
        Ok(ms)
    }

    /// `leak_scan(run)` — the test-battery entry (ADR-0059 D2): sweep the
    /// run's committed event payloads (the `event` target) with the detector
    /// set and report every leak. `leak_scan(run) = ∅` is LT-01's verdict.
    /// **A test/verifier path** — never called on a live boundary.
    ///
    /// The Stage-3 surfaces (DF-S2.4-3; R2.9b): the scan domain is the run's
    /// committed events **plus** the run manifest (the immutable seq-0
    /// record — `manifest` target), the whole blob pool (`blob` target —
    /// bundle members, reports and exported artefacts are pool-staged at
    /// assemble, so a delivered `redaction = none` sink's bytes are the
    /// pool's bytes), and every `measurement.export.delivered` row re-tagged
    /// under its `sink_delivery` surface so a leak attributes to the
    /// delivery leg, not only the event coordinate. One detector composes
    /// over every surface — no second detector (CC1).
    pub fn leak_scan_run(
        &self,
        store: &Store,
        run_id: &str,
        detectors: &DetectorSet,
    ) -> Result<Vec<Leak>, BrokerError> {
        let ledger = |detail: &str| BrokerError::Ledger {
            detail: detail.to_string(),
        };
        let events = store
            .events(run_id)
            .map_err(|e| ledger(&format!("events fold: {e}")))?;
        let mut items: Vec<(ScanTarget, String)> = events
            .iter()
            .map(|e| {
                (
                    ScanTarget::Event(Some(e.event_id.clone())),
                    e.payload.to_canonical_string(),
                )
            })
            .collect();
        // The manifest — the seq-0 record carries refs and declared
        // coordinates a secret must never reach either.
        let manifest = store
            .manifest(run_id)
            .map_err(|e| ledger(&format!("manifest fold: {e}")))?;
        items.push((
            ScanTarget::Manifest(run_id.to_string()),
            manifest.to_json().to_canonical_string(),
        ));
        // The blob pool — every pool object the store holds (content is
        // arbitrary bytes; the detectors run over the lossy text form).
        for digest in store.blob_digests() {
            let addr = hh_identity::idp::ContentAddress {
                idp: "idp/1",
                algorithm: "sha256",
                digest: digest.clone(),
                media_type: String::new(),
                size: 0,
            };
            let bytes = store
                .get_blob(&addr)
                .map_err(|e| ledger(&format!("blob {digest}: {e}")))?;
            items.push((
                ScanTarget::Blob(digest),
                String::from_utf8_lossy(&bytes).into_owned(),
            ));
        }
        // `measurement.export.delivered` — the row is already scanned as an
        // `event`; re-tag it under the sink's delivery surface so a leak in
        // a delivered payload attributes to the leg that carried it.
        for e in events
            .iter()
            .filter(|e| e.class == "measurement.export.delivered")
        {
            let sink = e
                .payload
                .get("sink_id")
                .and_then(Json::as_str)
                .unwrap_or(&e.event_id)
                .to_string();
            items.push((
                ScanTarget::SinkDelivery(sink),
                e.payload.to_canonical_string(),
            ));
        }
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
