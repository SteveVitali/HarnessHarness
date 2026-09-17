//! The failure-typed contract (§5g.3 §2; ADR-0058 D5): every broker verb returns a
//! typed error — `Refused{code}` for the delivery-path refusals, the named contract
//! errors for `register`/`grant`/`rotate`, and [`BrokerError::Deferred`] for the
//! Stage-2 verbs whose SPI exists now (`mint`, the wire half of `mediate`).
//!
//! No variant carries a secret value, a placeholder body or materialising bytes —
//! the errors are themselves content-free (SV-2/SV-4 apply to failure paths too).

use std::fmt;

/// The closed `Refused.code` sum — the union of §5g.3 §2's `bind` error set
/// (`no_grant | not_bindable | decision_missing | mode_not_allowed |
/// environment_not_isolated`) and `mediate`'s `Refused{expired | revoked |
/// out_of_scope | ambiguous_path | sender_constraint_unmet |
/// broker_unavailable | audit_unavailable | canary}`. One closed sum over both
/// verbs (ADR-0058 D5) — a code is spelled the same wherever it fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusedCode {
    /// `bind`: no `secret_access` grant covers the holder/channel/destination.
    NoGrant,
    /// The channel is `bindable = false` (kernel-only) — `bind` refused; use
    /// `kernel_use`.
    NotBindable,
    /// No `security.permission.decided{decision = allow}` row for a covering
    /// `secret_access` grant — the PDP/CDP gate (AC-R-2.8.3-13).
    DecisionMissing,
    /// The requested `DeliveryMode` is not in the channel's
    /// `delivery_modes_allowed`.
    ModeNotAllowed,
    /// `wrapped_long_lived` requires isolation at least `process_sandbox`.
    EnvironmentNotIsolated,
    /// The binding is past `expires_at` (SV-6 — nothing is renewable).
    Expired,
    /// The channel or binding is revoked (run terminal, fencing, detach,
    /// `Permission` revocation, rotation — SV-6).
    Revoked,
    /// `mediate`: the destination is outside the binding's declared set.
    OutOfScope,
    /// `mediate`: the request path is ambiguous (dot-segments, encoded
    /// separators, backslashes — LT-02).
    AmbiguousPath,
    /// The channel's `sender_constraint` cannot be verified (Stage 1 has no
    /// sender-verification machinery — `dpop`/`audience` fail closed).
    SenderConstraintUnmet,
    /// A canary channel presented to `bind`/`mediate`/`kernel_use` — the broker
    /// never injects a canary (ADR-0059 D3; the `leak_detected` row is appended
    /// alongside the refusal).
    Canary,
    /// The broker's secret source (vault/keyring/launch-env) is unavailable —
    /// never fall back to an environment value (AC-R-2.8.3-10).
    BrokerUnavailable,
    /// The audit row could not be made durable before the result would be
    /// visible — refused, never silent (ADR-0058 D6; AC-R-2.8.3-10).
    AuditUnavailable,
}

impl RefusedCode {
    /// The canonical tag spelling used in `security.credential.denied{reason}`.
    pub fn as_str(self) -> &'static str {
        match self {
            RefusedCode::NoGrant => "no_grant",
            RefusedCode::NotBindable => "not_bindable",
            RefusedCode::DecisionMissing => "decision_missing",
            RefusedCode::ModeNotAllowed => "mode_not_allowed",
            RefusedCode::EnvironmentNotIsolated => "environment_not_isolated",
            RefusedCode::Expired => "expired",
            RefusedCode::Revoked => "revoked",
            RefusedCode::OutOfScope => "out_of_scope",
            RefusedCode::AmbiguousPath => "ambiguous_path",
            RefusedCode::SenderConstraintUnmet => "sender_constraint_unmet",
            RefusedCode::Canary => "canary",
            RefusedCode::BrokerUnavailable => "broker_unavailable",
            RefusedCode::AuditUnavailable => "audit_unavailable",
        }
    }

    /// Parse the closed sum.
    pub fn parse(s: &str) -> Option<RefusedCode> {
        Some(match s {
            "no_grant" => RefusedCode::NoGrant,
            "not_bindable" => RefusedCode::NotBindable,
            "decision_missing" => RefusedCode::DecisionMissing,
            "mode_not_allowed" => RefusedCode::ModeNotAllowed,
            "environment_not_isolated" => RefusedCode::EnvironmentNotIsolated,
            "expired" => RefusedCode::Expired,
            "revoked" => RefusedCode::Revoked,
            "out_of_scope" => RefusedCode::OutOfScope,
            "ambiguous_path" => RefusedCode::AmbiguousPath,
            "sender_constraint_unmet" => RefusedCode::SenderConstraintUnmet,
            "canary" => RefusedCode::Canary,
            "broker_unavailable" => RefusedCode::BrokerUnavailable,
            "audit_unavailable" => RefusedCode::AuditUnavailable,
            _ => return None,
        })
    }
}

/// `Refused{code, detail}` — the delivery-path refusal (`bind`/`mediate`/`kernel_use`).
/// `detail` is a closed-shape diagnostic string (member names, ids — never a value).
#[derive(Debug, Clone, PartialEq)]
pub struct Refused {
    /// The closed code.
    pub code: RefusedCode,
    /// The content-free diagnostic.
    pub detail: String,
}

impl Refused {
    /// Construct a refusal.
    pub fn new(code: RefusedCode, detail: impl Into<String>) -> Refused {
        Refused {
            code,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Refused{{code={}}}: {}", self.code.as_str(), self.detail)
    }
}

/// The codec failure family — an unknown/missing/mis-typed canonical member. Unknown
/// members are `BadMember`, never silently dropped (CC4).
#[derive(Debug, Clone, PartialEq)]
pub enum CodecError {
    /// An unknown member in a closed record.
    BadMember {
        /// The member path.
        path: String,
    },
    /// A required member absent.
    MissingMember {
        /// The member path.
        path: String,
    },
    /// A member of the wrong JSON kind.
    TypeMismatch {
        /// The member path.
        path: String,
    },
    /// A member of the right kind but outside its closed sum / domain.
    BadValue {
        /// The member path and what was wrong.
        path: String,
    },
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodecError::BadMember { path } => write!(f, "BadMember: {path}"),
            CodecError::MissingMember { path } => write!(f, "MissingMember: {path}"),
            CodecError::TypeMismatch { path } => write!(f, "TypeMismatch: {path}"),
            CodecError::BadValue { path } => write!(f, "BadValue: {path}"),
        }
    }
}

impl std::error::Error for CodecError {}

/// The broker error family — the contract errors of §5g.3 §2 plus the typed
/// deferral for the Stage-2 delivery machinery (ADR-0058's stage cut).
#[derive(Debug, Clone, PartialEq)]
pub enum BrokerError {
    /// `register_channel`: a channel with this `channel_id` is already registered.
    ChannelExists {
        /// The colliding id.
        channel_id: String,
    },
    /// `register_channel`/`bind`: the spec or request is malformed — an empty
    /// `channel_id`, `binding_modes` empty for a `bindable` channel, a
    /// `kernel_only` access class that is `bindable`, an `env`-class channel that
    /// is not bindable, etc.
    InvalidBinding {
        /// What was malformed.
        detail: String,
    },
    /// `grant`: the requested scope is not a subset of the channel's declared
    /// `scope` (destination/env-name set).
    ScopeExceedsChannel {
        /// The channel.
        channel_id: String,
        /// The requested scope.
        scope: String,
    },
    /// `grant`: the issuer's authority is below `principal` — a `delegate` (or
    /// lower) may never issue a `secret_access` grant (SV-8's edge: a model claim
    /// confers nothing).
    IssuerAuthorityInsufficient {
        /// The issuer's authority spelling.
        issuer_authority: String,
        /// The floor (`principal`).
        required: String,
    },
    /// The `channel_id` names no registered channel.
    UnknownChannel {
        /// The unresolved id.
        channel_id: String,
    },
    /// The `binding_id` names no live binding.
    UnknownBinding {
        /// The unresolved id.
        binding_id: String,
    },
    /// `rotate`'s compare-and-swap failed — `expected_revision` is stale
    /// (§5g.3 §2 names this `Conflict{current}`).
    Conflict {
        /// The channel.
        channel_id: String,
        /// The caller's expectation.
        expected: u64,
        /// The registered revision (`Conflict.current`).
        current: u64,
    },
    /// The delivery-path refusal (`Refused{code}`) surfaced as an error — e.g.
    /// `mediate` on a refused binding returns this rather than a `MediationOutcome`.
    Refused(Refused),
    /// The verb's SPI exists but its delivery machinery is a Stage-2 row
    /// (`mint` — minted delivery; DF-S1.13-*). A *typed* failure, never a panic
    /// or a silent no-op (the failure-typed contract).
    Deferred {
        /// The verb.
        verb: &'static str,
        /// The stage the machinery lands.
        stage: u8,
    },
    /// A canonical codec failure.
    Codec(CodecError),
    /// The run ledger refused the audit append (mapped to
    /// `RefusedCode::AuditUnavailable` at the verb boundary — kept here for the
    /// rare caller that sees it directly).
    Ledger {
        /// The content-free detail.
        detail: String,
    },
}

impl fmt::Display for BrokerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrokerError::ChannelExists { channel_id } => {
                write!(f, "ChannelExists: {channel_id}")
            }
            BrokerError::InvalidBinding { detail } => write!(f, "InvalidBinding: {detail}"),
            BrokerError::ScopeExceedsChannel { channel_id, scope } => {
                write!(f, "ScopeExceedsChannel: {channel_id} scope {scope}")
            }
            BrokerError::IssuerAuthorityInsufficient {
                issuer_authority,
                required,
            } => write!(
                f,
                "IssuerAuthorityInsufficient: {issuer_authority} < {required}"
            ),
            BrokerError::UnknownChannel { channel_id } => {
                write!(f, "UnknownChannel: {channel_id}")
            }
            BrokerError::UnknownBinding { binding_id } => {
                write!(f, "UnknownBinding: {binding_id}")
            }
            BrokerError::Conflict {
                channel_id,
                expected,
                current,
            } => write!(
                f,
                "Conflict: {channel_id} expected r{expected}, current r{current}"
            ),
            BrokerError::Refused(r) => write!(f, "{r}"),
            BrokerError::Deferred { verb, stage } => {
                write!(f, "Deferred: {verb} lands at Stage {stage}")
            }
            BrokerError::Codec(c) => write!(f, "{c}"),
            BrokerError::Ledger { detail } => write!(f, "Ledger: {detail}"),
        }
    }
}

impl std::error::Error for BrokerError {}
