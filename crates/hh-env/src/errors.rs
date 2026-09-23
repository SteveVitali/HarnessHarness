//! The closed refusal/failure sum for the environment operations and the
//! execution contract (§5a.5 R-2.2.5 §4 error column; §5d.5 R-2.5.5 §3). Every
//! handle operation fails typed — never a warning, never a hang (I-C4's
//! fail-closed rule makes every evidence failure `ContainmentUnverified`-shaped;
//! capability failures are `Unsupported`/`UnknownCapability`, never coerced).

use hh_containment::attach::AttachError;
use hh_ledger::LedgerError;

/// `EnvironmentError` — the typed refusal for every `EnvDriver`/handle
/// operation. `Display` is the audit detail; the variants are the closed sum.
#[derive(Debug)]
pub enum EnvError {
    /// A state-machine transition the current `HandleState` does not allow
    /// (`declared → provisioning → ready ⇄ {detached, suspended} →
    /// unreachable → {reattached | replaced | failed} → torn_down`).
    InvalidState {
        /// The operation attempted.
        op: &'static str,
        /// The state the handle was in.
        state: &'static str,
    },
    /// A mutable image tag in an `EnvironmentRecord` whose `unpinned[]` context
    /// does not name it — the record refuses to resolve (ADR-0136 §3).
    UnresolvedRef {
        /// The tag that refused to pin.
        tag: String,
    },
    /// A `foreign_digest`/`unpinned_tag` image cannot back an R2 (or higher)
    /// reproducibility claim — only an immutable `ContentAddress` can (N8).
    ReproClaimUnsupported {
        /// The claim level attempted (`"R2"`).
        claim: String,
        /// What the image resolved to.
        image_kind: &'static str,
    },
    /// The attach path failed closed (I-C4) — wraps `AttachError`; the
    /// `unverified` event was already appended by `hh-containment`'s caller.
    ContainmentUnverified {
        /// The field group (`attach|report|fs|net|proc|resources`).
        field_group: String,
        /// The closed reason.
        reason: String,
    },
    /// `attach`'s non-evidence failures (invalid policy, degrade-on-lab).
    Attach(AttachError),
    /// The environment is not `ready` — `EnvironmentUnavailable` (the
    /// execution plane's `environment_unavailable` ErrorClass).
    Unavailable {
        /// The handle id.
        env_handle_id: String,
        /// The state it was in.
        state: &'static str,
    },
    /// The session is not live — `EnvironmentLost` (kernel death ≠
    /// environment death is handled by `heal`; a lost session the heal window
    /// cannot recover is `Failed`).
    SessionLost {
        /// The handle id.
        env_handle_id: String,
    },
    /// The `reattach_window_ms` lapsed with no live contact.
    WindowLapsed {
        /// The handle id.
        env_handle_id: String,
        /// The lapsed window.
        window_ms: u64,
    },
    /// A declared capability is `unsupported` (`snapshot(kind)` on a class that
    /// declared it `unsupported`; `suspend()` at Stage 1).
    Unsupported {
        /// The operation/capability.
        capability: &'static str,
        /// Why.
        detail: String,
    },
    /// A capability the class never declared — `unknown`, never coerced to
    /// `unsupported` (T-LCD-07).
    UnknownCapability {
        /// The capability.
        capability: String,
    },
    /// An operation on a path outside every writable root (R-NOSIDE — the
    /// surface is closed; there is no general-path escape).
    OutsideRoots {
        /// The canonical path attempted.
        path: String,
    },
    /// A lease/ledger write failure — surfaced, never swallowed.
    Ledger(LedgerError),
    /// A budget refusal during `prepare`'s `reserve`.
    BudgetRefused {
        /// The typed budget error's tag.
        detail: String,
    },
    /// A content-addressed blob the store could not serve.
    Blob(String),
    /// An idp/1 identity failure.
    Identity(String),
    /// The helper channel failed (send/recv/decode — transport plane; the
    /// dispatcher settles it `unknown{executor_error}` → probe, never a
    /// silent redispatch).
    Transport {
        /// What failed.
        detail: String,
    },
    /// The helper returned a typed refusal (`err{class, detail}` — e.g.
    /// `NotCommitted` for a missing/mismatched `commit_proof`, `BadToken`,
    /// `Dedup`). Surfaced verbatim — the class is the helper's closed
    /// refusal sum.
    HelperRefused {
        /// The refusal class.
        class: String,
        /// The detail.
        detail: String,
    },
}

impl std::fmt::Display for EnvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnvError::InvalidState { op, state } => {
                write!(f, "InvalidState: {op} on a {state} handle")
            }
            EnvError::UnresolvedRef { tag } => {
                write!(
                    f,
                    "UnresolvedRef: mutable image tag `{tag}` not in unpinned[]"
                )
            }
            EnvError::ReproClaimUnsupported { claim, image_kind } => write!(
                f,
                "ReproClaimUnsupported: {image_kind} cannot back a {claim} claim"
            ),
            EnvError::ContainmentUnverified {
                field_group,
                reason,
            } => write!(f, "ContainmentUnverified: {field_group} ({reason})"),
            EnvError::Attach(e) => write!(f, "attach: {e}"),
            EnvError::Unavailable {
                env_handle_id,
                state,
            } => write!(f, "EnvironmentUnavailable: {env_handle_id} is {state}"),
            EnvError::SessionLost { env_handle_id } => {
                write!(f, "EnvironmentLost: {env_handle_id} session not live")
            }
            EnvError::WindowLapsed {
                env_handle_id,
                window_ms,
            } => write!(
                f,
                "EnvironmentLost: {env_handle_id} reattach window {window_ms}ms lapsed"
            ),
            EnvError::Unsupported { capability, detail } => {
                write!(f, "Unsupported: {capability} ({detail})")
            }
            EnvError::UnknownCapability { capability } => {
                write!(f, "UnknownCapability: {capability}")
            }
            EnvError::OutsideRoots { path } => {
                write!(f, "OutsideRoots: {path} escapes every writable root")
            }
            EnvError::Ledger(e) => write!(f, "ledger: {e}"),
            EnvError::BudgetRefused { detail } => write!(f, "BudgetRefused: {detail}"),
            EnvError::Blob(d) => write!(f, "blob: {d}"),
            EnvError::Identity(d) => write!(f, "identity: {d}"),
            EnvError::Transport { detail } => write!(f, "transport: {detail}"),
            EnvError::HelperRefused { class, detail } => {
                write!(f, "helper refused: {class} ({detail})")
            }
        }
    }
}

impl std::error::Error for EnvError {}

impl From<LedgerError> for EnvError {
    fn from(e: LedgerError) -> Self {
        EnvError::Ledger(e)
    }
}

impl From<AttachError> for EnvError {
    fn from(e: AttachError) -> Self {
        match e {
            AttachError::Unverified {
                field_group,
                reason,
                ..
            } => EnvError::ContainmentUnverified {
                field_group,
                reason,
            },
            other => EnvError::Attach(other),
        }
    }
}
