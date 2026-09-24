//! `SecretChannel` — the registered channel record `{channel_id, spec, revision,
//! provenance}` (§5g.3 §3). One channel exists per logical credential; a spec
//! change is a new **revision**, never an in-place edit (the channel's
//! `version_id` is `idp/1`-addressed over the canonical form — CC1).
//!
//! **The value never appears in the record** (ADR-0057 D1): the spec carries the
//! *source* (`operator_vault`/`os_keyring`/`launch_env`/`external_binding` — a
//! resolver coordinate, not bytes), the `destinations`, the
//! `delivery_modes_allowed`, `max_lifetime`, `sender_constraint`, `canary` and
//! `bindable`. `register_channel` never reads the value.

use std::collections::BTreeSet;

use hh_hir::records::GrantConstraints;
use hh_identity::RecordKind;
use hh_monitor::assess::SecretTransport;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::codec;
use crate::errors::CodecError;
use crate::types::{AccessClass, SecretRef};

/// `kind ∈ {api_key, bearer, basic, oauth, ssh_key, certificate, signing_key,
/// generic}` (§5g.3 §3 `SecretChannelSpec.kind`) — the credential's declared
/// shape. Closed sum; `generic` is the fallback for a credential with no more
/// specific shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CredentialKind {
    /// A bare API key.
    ApiKey,
    /// A bearer token.
    Bearer,
    /// HTTP basic credentials.
    Basic,
    /// An OAuth credential.
    Oauth,
    /// An SSH private key.
    SshKey,
    /// A certificate credential.
    Certificate,
    /// A signing key (e.g. the R-2.8.6 signer key via `kernel_use`).
    SigningKey,
    /// Anything else.
    Generic,
}

impl CredentialKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CredentialKind::ApiKey => "api_key",
            CredentialKind::Bearer => "bearer",
            CredentialKind::Basic => "basic",
            CredentialKind::Oauth => "oauth",
            CredentialKind::SshKey => "ssh_key",
            CredentialKind::Certificate => "certificate",
            CredentialKind::SigningKey => "signing_key",
            CredentialKind::Generic => "generic",
        }
    }

    /// Parse the closed sum (`None` for an unknown tag — `BadValue` upstream).
    pub fn parse(s: &str) -> Option<CredentialKind> {
        Some(match s {
            "api_key" => CredentialKind::ApiKey,
            "bearer" => CredentialKind::Bearer,
            "basic" => CredentialKind::Basic,
            "oauth" => CredentialKind::Oauth,
            "ssh_key" => CredentialKind::SshKey,
            "certificate" => CredentialKind::Certificate,
            "signing_key" => CredentialKind::SigningKey,
            "generic" => CredentialKind::Generic,
            _ => return None,
        })
    }
}

/// `sender_constraint ∈ {dpop, audience, none}` (§5g.3 §3) — how a delivered
/// credential is bound to its sender. At Stage 1 no verification machinery
/// exists, so a `minted_scoped` bind on a channel declaring `dpop`/`audience`
/// refuses `Refused{sender_constraint_unmet}` (fail closed; the verifier lands
/// with the `minted_scoped` delivery machinery at Stage 2, DF-S1.13-*).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SenderConstraint {
    /// DPoP-bound (sender key verified at the destination).
    Dpop,
    /// Audience-bound (the token names the destination).
    Audience,
    /// No sender constraint.
    None,
}

impl SenderConstraint {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SenderConstraint::Dpop => "dpop",
            SenderConstraint::Audience => "audience",
            SenderConstraint::None => "none",
        }
    }

    /// Parse the closed sum.
    pub fn parse(s: &str) -> Option<SenderConstraint> {
        Some(match s {
            "dpop" => SenderConstraint::Dpop,
            "audience" => SenderConstraint::Audience,
            "none" => SenderConstraint::None,
            _ => return None,
        })
    }
}

/// `auth_carrier ∈ {header{name, prefix?}, basic, query_forbidden}` (§5g.3 §3)
/// — how the credential rides a request. `query_forbidden` is a *negative*
/// declaration: the destination never accepts the credential in a URL (the
/// mediator's LT-02 ambiguous-path/path-leak check leans on it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthCarrier {
    /// A header carrier — `{name, prefix?}` (e.g. `Authorization`/`Bearer `).
    Header {
        /// The header name.
        name: String,
        /// The value prefix (e.g. `"Bearer "`), when declared.
        prefix: Option<String>,
    },
    /// HTTP basic auth.
    Basic,
    /// The credential may never travel in a query string.
    QueryForbidden,
}

impl AuthCarrier {
    /// The canonical spelling of the carrier tag.
    pub fn tag(&self) -> &'static str {
        match self {
            AuthCarrier::Header { .. } => "header",
            AuthCarrier::Basic => "basic",
            AuthCarrier::QueryForbidden => "query_forbidden",
        }
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        match self {
            AuthCarrier::Header { name, prefix } => Json::obj([
                ("kind", Json::str("header")),
                ("name", Json::str(name.clone())),
                (
                    "prefix",
                    prefix
                        .as_ref()
                        .map(|p| Json::str(p.clone()))
                        .unwrap_or(Json::Null),
                ),
            ]),
            other => Json::obj([("kind", Json::str(other.tag()))]),
        }
    }

    /// The strict decode — unknown members are `BadMember`.
    pub fn from_json(j: &Json, path: &str) -> Result<AuthCarrier, CodecError> {
        let m = codec::expect_obj(j, path)?;
        let kind = codec::str_at(m, "kind", path)?;
        match kind {
            "header" => {
                codec::reject_unknown(m, &["kind", "name", "prefix"], path)?;
                Ok(AuthCarrier::Header {
                    name: codec::str_at(m, "name", path)?.to_string(),
                    prefix: codec::opt_str_at(m, "prefix", path)?.map(String::from),
                })
            }
            "basic" => {
                codec::reject_unknown(m, &["kind"], path)?;
                Ok(AuthCarrier::Basic)
            }
            "query_forbidden" => {
                codec::reject_unknown(m, &["kind"], path)?;
                Ok(AuthCarrier::QueryForbidden)
            }
            _ => Err(CodecError::BadValue {
                path: format!("{path}.kind"),
            }),
        }
    }
}

/// `DestinationBinding{scheme, host_pattern, port?, path_prefix?,
/// auth_carrier}` (§5g.3 §3) — one destination the channel may be used for.
/// `host_pattern` uses the interim `ResourcePattern` rules (literal /
/// trailing-`/*` prefix / `*` — ADR-0212 OQ-132); the full `HostPattern`
/// grammar (`*.apex`/`**.apex`) is R-2.8.4's `normalize` (ADR-0061 D2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinationBinding {
    /// The URL scheme (`https`, `ssh`, …).
    pub scheme: String,
    /// The host pattern.
    pub host_pattern: String,
    /// The port, when constrained.
    pub port: Option<u16>,
    /// The path prefix the binding covers, when constrained.
    pub path_prefix: Option<String>,
    /// How the credential rides a request to this destination.
    pub auth_carrier: AuthCarrier,
}

impl DestinationBinding {
    /// Does this binding admit `host` (interim pattern rules — literal, `/*`
    /// suffix prefix, or `*`)?
    pub fn matches_host(&self, host: &str) -> bool {
        hh_monitor::args::scope_covers(&self.host_pattern, Some(host))
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("scheme", Json::str(self.scheme.clone())),
            ("host_pattern", Json::str(self.host_pattern.clone())),
            (
                "port",
                self.port.map(|p| Json::Int(p as i64)).unwrap_or(Json::Null),
            ),
            (
                "path_prefix",
                self.path_prefix
                    .as_ref()
                    .map(|p| Json::str(p.clone()))
                    .unwrap_or(Json::Null),
            ),
            ("auth_carrier", self.auth_carrier.to_json()),
        ])
    }

    /// The strict decode.
    pub fn from_json(j: &Json, path: &str) -> Result<DestinationBinding, CodecError> {
        let m = codec::expect_obj(j, path)?;
        codec::reject_unknown(
            m,
            &[
                "scheme",
                "host_pattern",
                "port",
                "path_prefix",
                "auth_carrier",
            ],
            path,
        )?;
        let port = match m.get("port") {
            Some(Json::Null) | None => None,
            Some(Json::Int(i)) if *i > 0 && *i <= u16::MAX as i64 => Some(*i as u16),
            Some(_) => {
                return Err(CodecError::BadValue {
                    path: format!("{path}.port"),
                })
            }
        };
        Ok(DestinationBinding {
            scheme: codec::str_at(m, "scheme", path)?.to_string(),
            host_pattern: codec::str_at(m, "host_pattern", path)?.to_string(),
            port,
            path_prefix: codec::opt_str_at(m, "path_prefix", path)?.map(String::from),
            auth_carrier: AuthCarrier::from_json(
                codec::member(m, "auth_carrier", path)?,
                &format!("{path}.auth_carrier"),
            )?,
        })
    }
}

/// `SecretSource ∈ {operator_vault{vault_ref}, os_keyring{item},
/// launch_env{var}, external_binding{ref, versioned}}` (§5g.3 §3) — the
/// *coordinate* of where the broker resolves the value, never the value.
/// `launch_env` is how the model-gateway credential enters: it is read once,
/// kernel-held, and lands in no environment the helper sees (the access class
/// is `kernel_only`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretSource {
    /// An operator vault coordinate (the vault itself is Stage-2 infrastructure;
    /// at Stage 1 a [`crate::broker::SecretSourceResolver`] supplies the bytes).
    OperatorVault {
        /// The vault's reference for the item.
        vault_ref: String,
    },
    /// An OS keyring item coordinate.
    OsKeyring {
        /// The keyring item name.
        item: String,
    },
    /// A launch-time environment variable — read once by the broker, then the
    /// broker is the value's only holder (the var itself is on the
    /// kernel-non-inheritable list, so it never propagates to a helper env).
    LaunchEnv {
        /// The variable name.
        var: String,
    },
    /// An externally-issued binding (a service-account / platform-identity
    /// credential) — `{ref, versioned}`.
    ExternalBinding {
        /// The external credential's reference.
        external_ref: String,
        /// Whether the external credential is versioned (a rotation bumps it).
        versioned: bool,
    },
}

impl SecretSource {
    /// The source-kind tag.
    pub fn kind(&self) -> &'static str {
        match self {
            SecretSource::OperatorVault { .. } => "operator_vault",
            SecretSource::OsKeyring { .. } => "os_keyring",
            SecretSource::LaunchEnv { .. } => "launch_env",
            SecretSource::ExternalBinding { .. } => "external_binding",
        }
    }

    /// The canonical JSON — `{kind, <kind-specific members>}`.
    pub fn to_json(&self) -> Json {
        let mut members = vec![("kind", Json::str(self.kind()))];
        match self {
            SecretSource::OperatorVault { vault_ref } => {
                members.push(("vault_ref", Json::str(vault_ref.clone())));
            }
            SecretSource::OsKeyring { item } => {
                members.push(("item", Json::str(item.clone())));
            }
            SecretSource::LaunchEnv { var } => {
                members.push(("var", Json::str(var.clone())));
            }
            SecretSource::ExternalBinding {
                external_ref,
                versioned,
            } => {
                members.push(("ref", Json::str(external_ref.clone())));
                members.push(("versioned", Json::Bool(*versioned)));
            }
        }
        Json::obj(members)
    }

    /// The strict decode — unknown members are `BadMember`.
    pub fn from_json(j: &Json, path: &str) -> Result<SecretSource, CodecError> {
        let m = codec::expect_obj(j, path)?;
        let kind = codec::str_at(m, "kind", path)?;
        match kind {
            "operator_vault" => {
                codec::reject_unknown(m, &["kind", "vault_ref"], path)?;
                Ok(SecretSource::OperatorVault {
                    vault_ref: codec::str_at(m, "vault_ref", path)?.to_string(),
                })
            }
            "os_keyring" => {
                codec::reject_unknown(m, &["kind", "item"], path)?;
                Ok(SecretSource::OsKeyring {
                    item: codec::str_at(m, "item", path)?.to_string(),
                })
            }
            "launch_env" => {
                codec::reject_unknown(m, &["kind", "var"], path)?;
                Ok(SecretSource::LaunchEnv {
                    var: codec::str_at(m, "var", path)?.to_string(),
                })
            }
            "external_binding" => {
                codec::reject_unknown(m, &["kind", "ref", "versioned"], path)?;
                Ok(SecretSource::ExternalBinding {
                    external_ref: codec::str_at(m, "ref", path)?.to_string(),
                    versioned: codec::bool_at(m, "versioned", path)?,
                })
            }
            _ => Err(CodecError::BadValue {
                path: format!("{path}.kind"),
            }),
        }
    }
}

/// `SecretChannelSpec` (§5g.3 §3) — `{kind, source, destinations[],
/// delivery_modes_allowed[], max_lifetime, sender_constraint, readers
/// (Stage 2), description, canary, bindable}` plus the Stage-1 extensions the
/// environment obligations need (`allowed_env_names`) and the least-privilege
/// ladder (`access_class`, grant `constraints`, `rotation_policy`).
///
/// - `bindable = false` ⇒ `access_class = kernel_only` (a channel that only
///   `kernel_use` may touch — the model-gateway credential's shape).
/// - `canary = true` ⇒ the broker never injects it: `bind`/`mediate`/
///   `kernel_use` refuse `Refused{canary}` and append
///   `security.secret.leak_detected` (ADR-0059 D3 — the encoding-independent
///   tripwire).
/// - `delivery_modes_allowed` is the ordered allow-list of §5g.3 §2.3:
///   `proxy_injected` primary, `minted_scoped` secondary, `wrapped_long_lived`
///   only by authorized exception.
/// - `readers` (the R-2.8.2 label) is Stage 2 — C0 channels are all
///   principal-scope; the member is absent, never defaulted (DF-S1.13-*).
#[derive(Debug, Clone, PartialEq)]
pub struct SecretChannelSpec {
    /// The credential's declared shape.
    pub kind: CredentialKind,
    /// Where the broker resolves the value (a coordinate, never bytes).
    pub source: SecretSource,
    /// The destinations the channel may be used for (`grant`/`bind` scope is ⊆
    /// this set — `ScopeExceedsChannel`/`NoGrant` otherwise).
    pub destinations: Vec<DestinationBinding>,
    /// The env var names a `bind` may project (absent = the channel may never
    /// be bound into an environment variable — deny-by-default; §5g.3 §1's
    /// environment obligations).
    pub allowed_env_names: Option<BTreeSet<String>>,
    /// The `DeliveryMode`s this channel admits (§5g.3 §2.3's
    /// `delivery_modes_allowed`).
    pub delivery_modes: BTreeSet<SecretTransport>,
    /// The channel's maximum validity window in ms — caps any binding's
    /// `expires_at` (`None` = run lifetime; SV-6's floor is the run itself).
    pub max_lifetime_ms: Option<u64>,
    /// The rotation policy tag (e.g. `manual`, `per_run`) — the rotation
    /// *mechanism* (`rotate`) is the broker verb.
    pub rotation_policy: Option<String>,
    /// The sender constraint (`none` at Stage 1 — `dpop`/`audience` refuse
    /// `sender_constraint_unmet` until the verifier lands at Stage 2).
    pub sender_constraint: SenderConstraint,
    /// The default grant constraints (`budget`, `time`, `count`).
    pub constraints: GrantConstraints,
    /// Whether `bind` may take this channel (`false` ⇒ `kernel_use` only).
    pub bindable: bool,
    /// The least-privilege ladder class.
    pub access_class: AccessClass,
    /// Whether this channel is a planted canary (never injected; any use
    /// attempt is `leak_detected`).
    pub canary: bool,
    /// The model-legible description (`SecretRef` renders it).
    pub description: String,
}

impl SecretChannelSpec {
    /// Does the spec's destination set admit `host`? (`scope ⊆ destinations` —
    /// some declared `DestinationBinding`'s `host_pattern` covers the host
    /// under the interim `ResourcePattern` rules.)
    pub fn admits_destination(&self, host: &str) -> bool {
        self.destinations.iter().any(|d| d.matches_host(host))
    }

    /// The canonical JSON (`secret_channel_spec` `v1`).
    pub fn to_json(&self) -> Json {
        let mut c = vec![];
        if let Some(b) = &self.constraints.budget {
            c.push(("budget", b.clone()));
        }
        if let Some(t) = self.constraints.time {
            c.push(("time", Json::Int(t as i64)));
        }
        if let Some(n) = self.constraints.count {
            c.push(("count", Json::Int(n as i64)));
        }
        // `BTreeSet` iterates in the closed sum's `Ord` order — canonical.
        let modes: Vec<Json> = self
            .delivery_modes
            .iter()
            .map(|m| Json::str(m.as_str()))
            .collect();
        Json::obj([
            ("v", Json::str("1")),
            ("kind", Json::str(self.kind.as_str())),
            ("source", self.source.to_json()),
            (
                "destinations",
                Json::Arr(self.destinations.iter().map(|d| d.to_json()).collect()),
            ),
            (
                "allowed_env_names",
                match &self.allowed_env_names {
                    Some(names) => Json::Arr(names.iter().map(|n| Json::str(n.clone())).collect()),
                    None => Json::Null,
                },
            ),
            ("delivery_modes_allowed", Json::Arr(modes)),
            (
                "max_lifetime",
                self.max_lifetime_ms
                    .map(|v| Json::Int(v as i64))
                    .unwrap_or(Json::Null),
            ),
            (
                "rotation_policy",
                self.rotation_policy
                    .as_ref()
                    .map(|p| Json::str(p.clone()))
                    .unwrap_or(Json::Null),
            ),
            (
                "sender_constraint",
                Json::str(self.sender_constraint.as_str()),
            ),
            (
                "constraints",
                Json::Obj(c.into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
            ),
            ("bindable", Json::Bool(self.bindable)),
            ("access_class", Json::str(self.access_class.as_str())),
            ("canary", Json::Bool(self.canary)),
            ("description", Json::str(self.description.clone())),
        ])
    }

    /// The strict decode — unknown members are `BadMember`, an unknown enum tag
    /// is `BadValue`.
    pub fn from_json(j: &Json, path: &str) -> Result<SecretChannelSpec, CodecError> {
        let m = codec::expect_obj(j, path)?;
        codec::reject_unknown(
            m,
            &[
                "v",
                "kind",
                "source",
                "destinations",
                "allowed_env_names",
                "delivery_modes_allowed",
                "max_lifetime",
                "rotation_policy",
                "sender_constraint",
                "constraints",
                "bindable",
                "access_class",
                "canary",
                "description",
            ],
            path,
        )?;
        let v = codec::str_at(m, "v", path)?;
        if v != "1" {
            return Err(CodecError::BadValue {
                path: format!("{path}.v"),
            });
        }
        let kind = CredentialKind::parse(codec::str_at(m, "kind", path)?).ok_or_else(|| {
            CodecError::BadValue {
                path: format!("{path}.kind"),
            }
        })?;
        let destinations = match codec::member(m, "destinations", path)? {
            Json::Arr(items) => items
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    DestinationBinding::from_json(d, &format!("{path}.destinations[{i}]"))
                })
                .collect::<Result<Vec<_>, _>>()?,
            _ => {
                return Err(CodecError::TypeMismatch {
                    path: format!("{path}.destinations"),
                })
            }
        };
        let allowed_env_names = match m.get("allowed_env_names") {
            Some(Json::Null) | None => None,
            Some(Json::Arr(items)) => {
                let mut names = BTreeSet::new();
                for (i, v) in items.iter().enumerate() {
                    match v {
                        Json::Str(s) => {
                            names.insert(s.clone());
                        }
                        _ => {
                            return Err(CodecError::TypeMismatch {
                                path: format!("{path}.allowed_env_names[{i}]"),
                            })
                        }
                    }
                }
                Some(names)
            }
            _ => {
                return Err(CodecError::TypeMismatch {
                    path: format!("{path}.allowed_env_names"),
                })
            }
        };
        let delivery_modes = match codec::member(m, "delivery_modes_allowed", path)? {
            Json::Arr(items) => {
                let mut modes = BTreeSet::new();
                for (i, t) in items.iter().enumerate() {
                    match t {
                        Json::Str(s) => {
                            modes.insert(SecretTransport::parse(s).ok_or_else(|| {
                                CodecError::BadValue {
                                    path: format!("{path}.delivery_modes_allowed[{i}]"),
                                }
                            })?);
                        }
                        _ => {
                            return Err(CodecError::TypeMismatch {
                                path: format!("{path}.delivery_modes_allowed[{i}]"),
                            })
                        }
                    }
                }
                modes
            }
            _ => {
                return Err(CodecError::TypeMismatch {
                    path: format!("{path}.delivery_modes_allowed"),
                })
            }
        };
        let max_lifetime_ms = match m.get("max_lifetime") {
            Some(Json::Null) | None => None,
            Some(Json::Int(i)) if *i >= 0 => Some(*i as u64),
            Some(_) => {
                return Err(CodecError::TypeMismatch {
                    path: format!("{path}.max_lifetime"),
                })
            }
        };
        let sender_constraint =
            SenderConstraint::parse(codec::str_at(m, "sender_constraint", path)?).ok_or_else(
                || CodecError::BadValue {
                    path: format!("{path}.sender_constraint"),
                },
            )?;
        let cj = codec::expect_obj(
            codec::member(m, "constraints", path)?,
            &format!("{path}.constraints"),
        )?;
        let mut constraints = GrantConstraints::default();
        for (k, v) in cj {
            match k.as_str() {
                "budget" => constraints.budget = Some(v.clone()),
                "time" => match v {
                    Json::Int(i) if *i >= 0 => constraints.time = Some(*i as u64),
                    _ => {
                        return Err(CodecError::BadValue {
                            path: format!("{path}.constraints.time"),
                        })
                    }
                },
                "count" => match v {
                    Json::Int(i) if *i >= 0 => constraints.count = Some(*i as u64),
                    _ => {
                        return Err(CodecError::BadValue {
                            path: format!("{path}.constraints.count"),
                        })
                    }
                },
                _ => {
                    return Err(CodecError::BadMember {
                        path: format!("{path}.constraints.{k}"),
                    })
                }
            }
        }
        let access_class =
            AccessClass::parse(codec::str_at(m, "access_class", path)?).ok_or_else(|| {
                CodecError::BadValue {
                    path: format!("{path}.access_class"),
                }
            })?;
        Ok(SecretChannelSpec {
            kind,
            source: SecretSource::from_json(
                codec::member(m, "source", path)?,
                &format!("{path}.source"),
            )?,
            destinations,
            allowed_env_names,
            delivery_modes,
            max_lifetime_ms,
            rotation_policy: codec::opt_str_at(m, "rotation_policy", path)?.map(String::from),
            sender_constraint,
            constraints,
            bindable: codec::bool_at(m, "bindable", path)?,
            access_class,
            canary: codec::bool_at(m, "canary", path)?,
            description: codec::str_at(m, "description", path)?.to_string(),
        })
    }
}

/// `SecretChannel{channel_id, spec, revision, provenance}` (§5g.3 §3). The
/// `revision` bumps on any spec change or `rotate`; `provenance` is mandatory
/// (§8.1 — the channel was registered by *someone*, and that fact is as
/// auditable as the channel). `semantic_id`/`version_id` are computed over the
/// canonical form — the value and every placeholder are excluded by
/// construction (T-LCD-10).
#[derive(Debug, Clone, PartialEq)]
pub struct SecretChannel {
    /// The logical channel id (`[a-z0-9_.-]+` — e.g. `github`, `model-gateway`).
    pub channel_id: String,
    /// The registered spec.
    pub spec: SecretChannelSpec,
    /// The channel revision (1 on register; `rotate`/spec change bumps it).
    pub revision: u64,
    /// The registration provenance.
    pub provenance: ProvenanceRecord,
}

impl SecretChannel {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("channel_id", Json::str(self.channel_id.clone())),
            ("spec", self.spec.to_json()),
            ("revision", Json::Int(self.revision as i64)),
            ("provenance", self.provenance.to_json()),
        ])
    }

    /// The strict decode.
    pub fn from_json(j: &Json, path: &str) -> Result<SecretChannel, CodecError> {
        let m = codec::expect_obj(j, path)?;
        codec::reject_unknown(m, &["channel_id", "spec", "revision", "provenance"], path)?;
        let revision = codec::int_at(m, "revision", path)?;
        if revision <= 0 {
            return Err(CodecError::BadValue {
                path: format!("{path}.revision"),
            });
        }
        let provenance = ProvenanceRecord::from_json(codec::member(m, "provenance", path)?)
            .map_err(|e| CodecError::BadValue {
                path: format!("{path}.provenance: {e}"),
            })?;
        Ok(SecretChannel {
            channel_id: codec::str_at(m, "channel_id", path)?.to_string(),
            spec: SecretChannelSpec::from_json(
                codec::member(m, "spec", path)?,
                &format!("{path}.spec"),
            )?,
            revision: revision as u64,
            provenance,
        })
    }

    /// The canonical bytes (sorted-key compact — the one canonicalizer).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.to_json().to_canonical_string().into_bytes()
    }

    /// The `idp/1` version id of this revision (`RecordKind::SecretChannel` —
    /// CC1; the audit rows pin it).
    pub fn version_id(&self) -> String {
        hh_identity::idp::identify_bytes(RecordKind::SecretChannel, &self.canonical_bytes())
    }

    /// The model-facing reference — `channel_id` + `description`, never a value.
    pub fn secret_ref(&self) -> SecretRef {
        SecretRef {
            channel_id: self.channel_id.clone(),
            description: self.spec.description.clone(),
        }
    }

    /// The canonical scope string a `secret_access` grant must cover —
    /// `secret:<channel_id>` (matched by `secret:<id>`, `secret:<id>/*` prefixes
    /// or `*` under the interim `ResourcePattern` rules).
    pub fn grant_scope(channel_id: &str) -> String {
        format!("secret:{channel_id}")
    }

    /// Whether a channel-id spelling is legal (`[a-z0-9_.-]+`, non-empty).
    pub fn valid_id(id: &str) -> bool {
        !id.is_empty()
            && id.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
            })
    }
}
