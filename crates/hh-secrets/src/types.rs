//! The value-free reference types (§5g.3 §2; ADR-0057): `SecretRef`,
//! `SecretFingerprint`, `Placeholder`, `ProvidedSecret`, `AccessClass`.
//!
//! **No type in this module can carry a secret value** — that is SV-2/SV-4 at the
//! type level: what crosses the broker boundary is a reference, a keyed truncated
//! fingerprint or an opaque placeholder, never materialising bytes.

use hh_wire::json::Json;

use crate::errors::CodecError;

/// `AccessClass` — the least-privilege ladder `env | broker | mediated_only |
/// kernel_only` (§5g.3 §1). Higher = more isolated: `env` (legacy direct
/// injection — never the default for a new channel), `broker` (proxy-injected
/// through the broker), `mediated_only` (only the mediator may touch the value),
/// `kernel_only` (only kernel components via `kernel_use` — `bindable = false`).
/// The ladder is the Π-10 "high-severity warning" axis: a request for `env` or
/// `wrapped_long_lived` on a channel declared above `mediated_only` is never
/// silently granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccessClass {
    /// The value may be placed in a process environment (legacy only).
    Env,
    /// The broker may inject it (proxy path).
    Broker,
    /// Only the egress mediator may materialise it.
    MediatedOnly,
    /// Only kernel components may use it (`kernel_use` — the model-gateway
    /// credential's class).
    KernelOnly,
}

impl AccessClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AccessClass::Env => "env",
            AccessClass::Broker => "broker",
            AccessClass::MediatedOnly => "mediated_only",
            AccessClass::KernelOnly => "kernel_only",
        }
    }

    /// Parse the closed sum.
    pub fn parse(s: &str) -> Option<AccessClass> {
        Some(match s {
            "env" => AccessClass::Env,
            "broker" => AccessClass::Broker,
            "mediated_only" => AccessClass::MediatedOnly,
            "kernel_only" => AccessClass::KernelOnly,
            _ => return None,
        })
    }
}

/// `SecretRef{channel_id}` — the **only** value-free reference to a credential
/// above the broker (§5g.3 §1/§3; ADR-0057 D1): a typed reference usable
/// wherever a `Text` parameter would otherwise carry a credential — never
/// `Text`, `CompiledPayload`, a parameter value or an `ext` value itself.
/// At the model boundary it renders `channel_id` + `description` and nothing
/// else (AC-R-2.8.3-1; the description is the channel's `authority =
/// definition` metadata, model-legible by design); the secret's *existence* is
/// legible, its material is not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRef {
    /// The channel the reference names (the logical name — the comparison
    /// coordinate; §5g.3 §3).
    pub channel_id: String,
    /// The human-facing description (metadata — model-legible by design).
    pub description: String,
}

impl SecretRef {
    /// The model-boundary rendering — `channel_id` + `description`, never a
    /// value, fingerprint or placeholder body (SV-2/SV-4).
    pub fn render(&self) -> String {
        format!("{} ({})", self.channel_id, self.description)
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("channel_id", Json::str(self.channel_id.clone())),
            ("description", Json::str(self.description.clone())),
        ])
    }

    /// The strict decode — unknown members are `BadMember`.
    pub fn from_json(j: &Json, path: &str) -> Result<SecretRef, CodecError> {
        let m = crate::codec::expect_obj(j, path)?;
        for k in m.keys() {
            if !matches!(k.as_str(), "channel_id" | "description") {
                return Err(CodecError::BadMember {
                    path: format!("{path}.{k}"),
                });
            }
        }
        Ok(SecretRef {
            channel_id: crate::codec::str_at(m, "channel_id", path)?.to_string(),
            description: crate::codec::str_at(m, "description", path)?.to_string(),
        })
    }
}

/// `KernelPurpose ∈ {ledger_signing, gateway_auth, vault_auth}` (§5g.3 §2
/// `kernel_use`) — the closed sum of kernel-internal uses a `bindable = false`
/// channel exists for. The `used` row's `destination = kernel:<purpose>` names
/// it (the caller component is a separate member — a purpose is *what for*, not
/// *who*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelPurpose {
    /// The run-ledger signing key (R-2.8.6 checkpoints).
    LedgerSigning,
    /// The §05b model-gateway credential (OQ-148).
    GatewayAuth,
    /// The vault's own credential (the resolver's bootstrap).
    VaultAuth,
}

impl KernelPurpose {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            KernelPurpose::LedgerSigning => "ledger_signing",
            KernelPurpose::GatewayAuth => "gateway_auth",
            KernelPurpose::VaultAuth => "vault_auth",
        }
    }

    /// Parse the closed sum.
    pub fn parse(s: &str) -> Option<KernelPurpose> {
        Some(match s {
            "ledger_signing" => KernelPurpose::LedgerSigning,
            "gateway_auth" => KernelPurpose::GatewayAuth,
            "vault_auth" => KernelPurpose::VaultAuth,
            _ => return None,
        })
    }
}

/// `BindingState ∈ {bound, active, expired, revoked}` (§5g.3 §3
/// `CredentialBinding.state`) — the binding's lifecycle. `bound` at `bind`;
/// `active` after its first audited `mediate`; `expired` past `expires_at`;
/// `revoked` at `revoke`/run-terminal/rotation (nothing is renewable — SV-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingState {
    /// Minted, not yet used.
    Bound,
    /// Has delivered at least once.
    Active,
    /// Past `expires_at` (detected at `mediate` — the state transition is
    /// recorded by the `denied{reason = expired}` row).
    Expired,
    /// Revoked — explicit, run-terminal, fencing, detach or rotation.
    Revoked,
}

impl BindingState {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            BindingState::Bound => "bound",
            BindingState::Active => "active",
            BindingState::Expired => "expired",
            BindingState::Revoked => "revoked",
        }
    }

    /// Live = `bound | active`.
    pub fn is_live(self) -> bool {
        matches!(self, BindingState::Bound | BindingState::Active)
    }
}

/// `ProvidedSecret` — the *only* secret fact a ledger/report/manifest row may
/// carry (§5g.3 §4; ADR-0057 D4): `provided: yes|no`, never the value, its
/// length, or any materialising byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProvidedSecret {
    /// Whether the secret was provided at all.
    pub provided: bool,
}

impl ProvidedSecret {
    /// The canonical JSON member value.
    pub fn to_json(&self) -> Json {
        Json::Bool(self.provided)
    }
}

/// `SecretFingerprint{channel_id, revision, fingerprint}` (§5g.3 §2):
/// `H(secret_key ∥ channel_id ∥ revision ∥ value)` truncated to 128 bits —
/// keyed, irreversible, stable within a revision (a rotation re-fingerprints).
/// The digest is a *correlation coordinate* for `redact`/`leak_scan` audit rows,
/// never a reconstruction path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretFingerprint {
    /// The channel the fingerprint was minted under.
    pub channel_id: String,
    /// The channel revision.
    pub revision: u64,
    /// The truncated digest — 32 lowercase hex chars (128 bits).
    pub digest: String,
}

impl SecretFingerprint {
    /// The rendered form — `sf:<32 hex>`; self-describing, content-free.
    pub fn render(&self) -> String {
        format!("sf:{}", self.digest)
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("channel_id", Json::str(self.channel_id.clone())),
            ("revision", Json::Int(self.revision as i64)),
            ("fingerprint", Json::str(self.digest.clone())),
        ])
    }

    /// The strict decode — unknown members are `BadMember`.
    pub fn from_json(j: &Json, path: &str) -> Result<SecretFingerprint, CodecError> {
        let m = crate::codec::expect_obj(j, path)?;
        for k in m.keys() {
            if !matches!(k.as_str(), "channel_id" | "revision" | "fingerprint") {
                return Err(CodecError::BadMember {
                    path: format!("{path}.{k}"),
                });
            }
        }
        let revision = crate::codec::int_at(m, "revision", path)?;
        if revision < 0 {
            return Err(CodecError::BadValue {
                path: format!("{path}.revision"),
            });
        }
        Ok(SecretFingerprint {
            channel_id: crate::codec::str_at(m, "channel_id", path)?.to_string(),
            revision: revision as u64,
            digest: crate::codec::str_at(m, "fingerprint", path)?.to_string(),
        })
    }
}

/// An opaque env placeholder — `mh_secret:v1:<channel_id>:r<revision>:<nonce>`
/// (§5g.3 §1 environment table). Always ≥ 16 bytes (the prefix alone is 11 and
/// the nonce is 24 hex chars), recognisable by the `mh_secret:` prefix, and
/// *non-transferable*: it is derived per `binding_id`, so a placeholder copied
/// to another run/binding resolves to nothing the broker will honour (SV-10 —
/// best-effort at Stage 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    /// The channel.
    pub channel_id: String,
    /// The channel revision the placeholder was minted under.
    pub revision: u64,
    /// The binding the placeholder is bound to (non-transferable coordinate).
    pub binding_id: String,
    /// The rendered spelling.
    pub spelling: String,
}

impl Placeholder {
    /// The recognisable prefix — the `placeholder_passthrough` detector's
    /// mark and the env sweep's is-placeholder test.
    pub const PREFIX: &'static str = "mh_secret:";

    /// Mint the deterministic placeholder for a binding —
    /// `mh_secret:v1:<channel_id>:r<revision>:<first-24-hex of
    /// idp_digest("secret.placeholder", run_id ∥ binding_id)>`. Deterministic so
    /// the same binding always projects the same spelling (env snapshot
    /// stability); domain-framed under `idp/1` (the one hasher — CC1).
    pub fn mint(run_id: &str, binding: &crate::broker::CredentialBinding) -> Placeholder {
        let material = format!("{}\u{1f}{}", run_id, binding.binding_id);
        let nonce = &hh_identity::idp::idp_digest("secret.placeholder", material.as_bytes())[..24];
        Placeholder {
            channel_id: binding.channel_id.clone(),
            revision: binding.channel_revision,
            binding_id: binding.binding_id.clone(),
            spelling: format!(
                "{}v1:{}:r{}:{}",
                Placeholder::PREFIX,
                binding.channel_id,
                binding.channel_revision,
                nonce
            ),
        }
    }

    /// Whether a string is a placeholder spelling (prefix + shape, no content
    /// validation — a *forged* placeholder resolves to nothing at the broker,
    /// which is the point of the form).
    pub fn is_placeholder(s: &str) -> bool {
        s.starts_with(Placeholder::PREFIX)
    }

    /// The byte length floor the spec requires (`≥ 16 bytes`).
    pub fn is_well_formed(&self) -> bool {
        self.spelling.len() >= 16 && self.spelling.starts_with(Placeholder::PREFIX)
    }
}

/// `SecretUnavailable{channel, reason}` — the terminal observation the model
/// boundary receives when mediation fails (§5g.3 §4; AC-R-2.8.3-10). `reason` is
/// a closed tag ([`RefusedCode`] spelling), never prose and never a fallback
/// value — there is no environment-value path to fall back to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretUnavailable {
    /// The channel whose mediation failed.
    pub channel_id: String,
    /// The closed refusal tag.
    pub reason: crate::errors::RefusedCode,
}

impl SecretUnavailable {
    /// The model-facing observation payload — `{kind: secret_unavailable,
    /// channel, reason}`; content-free.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("kind", Json::str("secret_unavailable")),
            ("channel", Json::str(self.channel_id.clone())),
            ("reason", Json::str(self.reason.as_str())),
        ])
    }
}
