//! §5b.1 R4.5d — the live `CredentialPort` (ADR-0366 D4): the broker's
//! `SecretSource`/`SecretSourceResolver` machinery stands behind the
//! gateway's secret-free `CredentialHandle`.
//!
//! The discipline the contract fixes:
//!
//! - `bind` resolves the `credential_ref` **coordinate** — R4.5d admits the
//!   `env:NAME` spelling (`SecretSource::LaunchEnv{var}`; the same channel
//!   `live-verify.sh` provisions `ANTHROPIC_API_KEY`/`OPENAI_API_KEY`
//!   through). Other source kinds are typed `CredentialUnavailable`
//!   refusals, never silent fallbacks.
//! - The handle carries only `binding_id` (the broker coordinate key —
//!   `launch_env:NAME`) and `provided` — **never** the value. The ledger's
//!   `credential_binding_id` records the coordinate; the value's lifetime
//!   is scoped to the transport's `send` ([`header_secret`] resolves it at
//!   the moment bytes leave and drops it on return).
//! - A `SecretSourceResolver` is supplied at construction (the launch-env
//!   composition by default); a missing name or an unavailable source is
//!   `CredentialUnavailable` — honest, never an empty header.

use crate::errors::GatewayError;
use crate::gateway::{CredentialHandle, CredentialPort};
use hharness_secrets::channel::SecretSource;
use hharness_secrets::{coord_key, CompositeResolver, SecretSourceResolver};

/// `BrokerCredentialPort` — the live `CredentialPort` over a
/// [`SecretSourceResolver`]. Bind-time resolution proves the coordinate
/// serves a value; the value itself is never retained (the transport
/// re-resolves inside `send` — see [`header_secret`]).
pub struct BrokerCredentialPort {
    /// The resolver the port binds through (launch env + static vault by
    /// default — the operator-provisioned channel).
    resolver: Box<dyn SecretSourceResolver>,
}

impl Default for BrokerCredentialPort {
    fn default() -> Self {
        Self::new(Box::<CompositeResolver>::default())
    }
}

impl BrokerCredentialPort {
    /// `new(resolver)` — a port over an explicit resolver (tests pin a
    /// `StaticVault`; the lane binds the launch-env composition).
    pub fn new(resolver: Box<dyn SecretSourceResolver>) -> Self {
        Self { resolver }
    }
}

/// `source_from_ref(credential_ref)` — parse a `credential_ref` spelling
/// into its `SecretSource` coordinate. R4.5d admits `env:NAME`
/// (`launch_env`); `vault:`/`keyring:`/`external:` spellings are
/// coordinates the resolver could serve but this port does not yet admit —
/// a typed refusal, not a guess.
pub fn source_from_ref(credential_ref: &str) -> Result<SecretSource, GatewayError> {
    if let Some(name) = credential_ref.strip_prefix("env:") {
        if name.is_empty() {
            return Err(GatewayError::CredentialUnavailable {
                detail: "credential_ref 'env:' names an empty variable".to_string(),
            });
        }
        return Ok(SecretSource::LaunchEnv {
            var: name.to_string(),
        });
    }
    Err(GatewayError::CredentialUnavailable {
        detail: format!(
            "credential_ref kind unadmitted (admitted spellings: env:NAME): {}",
            coordinate_kind(credential_ref)
        ),
    })
}

/// The coordinate's kind prefix for error text (`env`/`vault`/… — the
/// prefix, never a value).
fn coordinate_kind(credential_ref: &str) -> String {
    match credential_ref.split_once(':') {
        Some((k, _)) => k.to_string(),
        None => "unprefixed".to_string(),
    }
}

/// `source_from_binding(binding_id)` — the inverse of `coord_key` for the
/// admitted kinds: `launch_env:NAME` → `LaunchEnv{var}`. A binding
/// coordinate outside the admitted set is `None` — the transport refuses
/// it typed rather than guessing a source.
pub fn source_from_binding(binding_id: &str) -> Option<SecretSource> {
    binding_id
        .strip_prefix("launch_env:")
        .map(|var| SecretSource::LaunchEnv {
            var: var.to_string(),
        })
}

/// `header_secret(resolver, credential)` — resolve the bound source to the
/// secret material *for header injection inside `send`*. The returned
/// `String` is the value — the caller attaches it to the auth header and
/// drops it; it never lands in a frame, a ledger row, an error message,
/// or an env. `Err` answers the typed `auth` refusal (`provided` said the
/// binding served a value; a vanished source is a credential fault, not a
/// silent bare request).
pub fn header_secret(
    resolver: &dyn SecretSourceResolver,
    credential: &CredentialHandle,
) -> Result<String, crate::vocab::ModelError> {
    let binding_id = credential.binding_id.as_deref().ok_or_else(|| {
        crate::vocab::ModelError::new(
            crate::vocab::ModelErrorClass::Auth,
            "credential handle carries no binding id",
        )
    })?;
    let source = source_from_binding(binding_id).ok_or_else(|| {
        crate::vocab::ModelError::new(
            crate::vocab::ModelErrorClass::UnsupportedFeature,
            format!(
                "credential binding kind unadmitted: {}",
                coordinate_kind(binding_id)
            ),
        )
    })?;
    resolver.resolve(&source).map_err(|_| {
        crate::vocab::ModelError::new(
            crate::vocab::ModelErrorClass::Auth,
            format!("credential binding {binding_id} resolves no value"),
        )
    })
}

impl CredentialPort for BrokerCredentialPort {
    /// `bind(credential_ref, endpoint, auth_kinds)` — resolve the ref's
    /// source coordinate and prove the resolver serves it. The value is
    /// discarded immediately; the handle records only the coordinate and
    /// `provided`.
    fn bind(
        &mut self,
        credential_ref: Option<&str>,
        _endpoint: &str,
        auth_kinds: &[crate::dialect::AuthKind],
    ) -> Result<CredentialHandle, GatewayError> {
        let needs_secret = auth_kinds
            .iter()
            .any(|k| !matches!(k, crate::dialect::AuthKind::None));
        match credential_ref {
            None => {
                if needs_secret
                    && !auth_kinds
                        .iter()
                        .any(|k| matches!(k, crate::dialect::AuthKind::None))
                {
                    return Err(GatewayError::CredentialUnavailable {
                        detail: format!(
                            "dialect requires {} auth; no credential_ref bound",
                            auth_kinds
                                .iter()
                                .find(|k| !matches!(k, crate::dialect::AuthKind::None))
                                .map(|k| k.as_str())
                                .unwrap_or("a")
                        ),
                    });
                }
                Ok(CredentialHandle {
                    binding_id: None,
                    provided: false,
                })
            }
            Some(r) => {
                if !needs_secret {
                    return Err(GatewayError::CredentialUnavailable {
                        detail: format!(
                            "credential_ref bound but the dialect admits no auth kind ({r:?})"
                        ),
                    });
                }
                let source = source_from_ref(r)?;
                // Prove the coordinate serves a value; discard it — the
                // handle never carries material.
                let _ = self.resolver.resolve(&source).map_err(|_| {
                    GatewayError::CredentialUnavailable {
                        detail: format!("credential source {} serves no value", coord_key(&source)),
                    }
                })?;
                Ok(CredentialHandle {
                    binding_id: Some(coord_key(&source)),
                    provided: true,
                })
            }
        }
    }
}
