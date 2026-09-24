//! `ErrorClass` — the closed error sum for effects (§5d.5 §3; ADR-0087 D4's
//! ten values extended by ADR-0102 D2 — CF-215), the three-origin rule, and
//! the kernel-derived `retryable` (D4: `f(class, origin, effective_risk_class)`
//! — an executor hint may only lower it, never raise).
//!
//! **Origin rule (ADR-0102 D2).** `origin` is assigned by *which plane produced
//! the failure*, not by the class alone:
//!
//! - `tool` ⇐ the tool's own result (`isError`, a non-zero exit with output,
//!   in-tool validation) — the class is the tool's reported member (drawn from
//!   the capability's declared `error_classes`; a bare non-zero exit carries
//!   `executor_error` at `tool` origin — see ADR — the "transport ⇐
//!   `executor_error`" clause binds the *kernel/helper-emitted* spelling);
//! - `execution` ⇐ `timeout, cancelled, containment_denied, signalled,
//!   output_cap_exceeded, environment_unavailable` from helper/kernel;
//! - `transport` ⇐ `protocol_error, executor_error` from the transport plane
//!   (helper crash, malformed report — `transport` ⇒ `not_applied` only if
//!   non-dispatch is proven, else `unknown` → probe).

use hh_ontology::risk::{RepeatSafety, RiskClass};
use hh_wire::json::Json;

/// `CancelBy` — who cancelled (`cancelled{by}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelBy {
    /// The principal (user).
    Principal,
    /// A budget bound.
    Budget,
    /// The writer lease.
    Lease,
    /// Run end.
    RunEnd,
    /// The protocol (a protocol-level cancel frame).
    Protocol,
}

impl CancelBy {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CancelBy::Principal => "principal",
            CancelBy::Budget => "budget",
            CancelBy::Lease => "lease",
            CancelBy::RunEnd => "run_end",
            CancelBy::Protocol => "protocol",
        }
    }
}

/// `ErrorClass` — the closed sum (16 members).
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorClass {
    /// The tool's arguments failed validation.
    InvalidArguments,
    /// A declared precondition was violated.
    PreconditionViolated {
        /// The precondition kind.
        kind: String,
    },
    /// The target does not exist.
    NotFound,
    /// A state conflict (e.g. `conflict{schema}` on a failed output-schema
    /// check — the value is retained, admit-annotated).
    Conflict {
        /// The conflict kind.
        kind: String,
    },
    /// A deadline elapsed.
    Timeout,
    /// The environment was not `ready`/reachable.
    EnvironmentUnavailable,
    /// The monitor refused (`permission_refused{reason}`).
    PermissionRefused {
        /// The closed `DenyReason` tag.
        reason: String,
    },
    /// A rate limit applied.
    RateLimited,
    /// The effect partially applied (the honest `observed(partial)` class).
    PartialApplication,
    /// The executor machinery failed (transport-origin when the helper/kernel
    /// emits it — a helper crash; a *tool-reported* `executor_error` is the
    /// generic tool-failure class at `tool` origin).
    ExecutorError,
    /// The effect was cancelled.
    Cancelled {
        /// Who cancelled.
        by: CancelBy,
    },
    /// Containment denied the attempt.
    ContainmentDenied {
        /// The denied claim kind.
        kind: String,
    },
    /// The process was killed by a signal.
    Signalled {
        /// The signal (`kill`, `term`, …).
        signal: String,
    },
    /// Output exceeded `retain_bytes_cap`.
    OutputCapExceeded,
    /// The helper protocol was violated (a malformed report).
    ProtocolError,
    /// A signal could not be attributed to a minted token.
    Unattributed,
}

impl ErrorClass {
    /// The canonical spelling (`error.class` member).
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorClass::InvalidArguments => "invalid_arguments",
            ErrorClass::PreconditionViolated { .. } => "precondition_violated",
            ErrorClass::NotFound => "not_found",
            ErrorClass::Conflict { .. } => "conflict",
            ErrorClass::Timeout => "timeout",
            ErrorClass::EnvironmentUnavailable => "environment_unavailable",
            ErrorClass::PermissionRefused { .. } => "permission_refused",
            ErrorClass::RateLimited => "rate_limited",
            ErrorClass::PartialApplication => "partial_application",
            ErrorClass::ExecutorError => "executor_error",
            ErrorClass::Cancelled { .. } => "cancelled",
            ErrorClass::ContainmentDenied { .. } => "containment_denied",
            ErrorClass::Signalled { .. } => "signalled",
            ErrorClass::OutputCapExceeded => "output_cap_exceeded",
            ErrorClass::ProtocolError => "protocol_error",
            ErrorClass::Unattributed => "unattributed",
        }
    }

    /// The classes `execution` origin may carry (helper/kernel-emitted).
    pub const EXECUTION_CLASSES: &'static [&'static str] = &[
        "timeout",
        "cancelled",
        "containment_denied",
        "signalled",
        "output_cap_exceeded",
        "environment_unavailable",
        "partial_application",
        "unattributed",
    ];

    /// The classes `transport` origin may carry (helper/kernel-emitted).
    pub const TRANSPORT_CLASSES: &'static [&'static str] = &["protocol_error", "executor_error"];

    /// Whether the class is admissible at `origin` (the origin rule's fixed
    /// assignment — `tool` admits every declared member, since the tool's own
    /// report carries whatever class the capability declared; `execution` and
    /// `transport` are restricted to the kernel-emitted spellings).
    pub fn admissible_at(&self, origin: ErrorOrigin) -> bool {
        match origin {
            ErrorOrigin::Tool => true,
            ErrorOrigin::Execution => Self::EXECUTION_CLASSES.contains(&self.as_str()),
            ErrorOrigin::Transport => Self::TRANSPORT_CLASSES.contains(&self.as_str()),
        }
    }

    /// The canonical member form (`error{class, …}` — `class` + the member
    /// payloads).
    pub fn to_json(&self) -> Json {
        let mut m = vec![("class", Json::str(self.as_str()))];
        match self {
            ErrorClass::PreconditionViolated { kind }
            | ErrorClass::Conflict { kind }
            | ErrorClass::ContainmentDenied { kind } => {
                m.push(("kind", Json::str(kind.clone())));
            }
            ErrorClass::PermissionRefused { reason } => {
                m.push(("reason", Json::str(reason.clone())));
            }
            ErrorClass::Cancelled { by } => {
                m.push(("by", Json::str(by.as_str())));
            }
            ErrorClass::Signalled { signal } => {
                m.push(("signal", Json::str(signal.clone())));
            }
            _ => {}
        }
        Json::Obj(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
}

/// `ErrorOrigin` — the plane that produced the failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorOrigin {
    /// The tool's own result.
    Tool,
    /// The execution plane (helper/kernel).
    Execution,
    /// The transport plane (helper crash, malformed report).
    Transport,
}

impl ErrorOrigin {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorOrigin::Tool => "tool",
            ErrorOrigin::Execution => "execution",
            ErrorOrigin::Transport => "transport",
        }
    }
}

/// `retryable = f(class, origin, effective_risk_class)` — kernel-derived
/// (ADR-0102 D4). The rule: `tool`-origin `rate_limited`/`conflict`/`timeout`
/// (and a `timeout` from the execution plane) are retryable only on
/// `read_only`/`reversible`/`idempotent` effects — anything `non_idempotent`
/// or `irreversible`/`compensable`/`external` is never blindly retried (a
/// retry of a maybe-applied non-idempotent effect is a probe, not a
/// redispatch — the dedup store decides). A `retryable_hint` may only lower.
pub fn retryable(class: &ErrorClass, origin: ErrorOrigin, risk: &RiskClass) -> bool {
    // A non-idempotent or non-reversible-or-readonly effect is never retried
    // by class — the probe path owns it.
    let safe_class = risk.repeat_safety == RepeatSafety::Idempotent
        || risk.is_read_only()
        || risk.reversibility == hh_ontology::risk::RiskReversibility::Reversible;
    if !safe_class {
        return false;
    }
    matches!(
        (class, origin),
        (ErrorClass::RateLimited, _)
            | (ErrorClass::Conflict { .. }, ErrorOrigin::Tool)
            | (
                ErrorClass::Timeout,
                ErrorOrigin::Tool | ErrorOrigin::Execution
            )
            | (ErrorClass::EnvironmentUnavailable, _)
    )
}

/// Apply the executor's `retryable_hint` — a hint may only *lower* the
/// kernel-derived verdict (I-3: the executor reports, never decides).
pub fn apply_hint(kernel_retryable: bool, hint: Option<bool>) -> bool {
    match hint {
        Some(false) => false, // lower — honored
        _ => kernel_retryable,
    }
}

/// `Observation` — the `action.effect.observed` payload's status member
/// (`ok | error{class, origin, detail_ref, retryable}`).
#[derive(Debug, Clone, PartialEq)]
pub enum ObservedStatus {
    /// The effect applied cleanly.
    Ok,
    /// The effect failed — the classified `(class, origin, retryable)`.
    Error {
        /// The class.
        class: ErrorClass,
        /// The origin.
        origin: ErrorOrigin,
        /// A detail ref (masked content).
        detail_ref: Option<String>,
        /// The kernel-derived retryable.
        retryable: bool,
    },
}

/// `EffectOutcome` — the lifecycle outcome the observe stage settles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectOutcome {
    /// The effect applied.
    Applied,
    /// The effect did not apply (a clean `not_applied`).
    NotApplied,
    /// The effect partially applied (non-terminal — the fold holds it).
    Partial,
    /// Outcome unknowable (`unknown` → probe).
    Unknown,
}

impl EffectOutcome {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EffectOutcome::Applied => "applied",
            EffectOutcome::NotApplied => "not_applied",
            EffectOutcome::Partial => "partial",
            EffectOutcome::Unknown => "unknown",
        }
    }
}

/// `Observation` — the observed outcome the kernel settles (the `observed`
/// payload). The kernel computes it from the capture + the lifecycle mapping;
/// the executor's terminal hint is an input, never the verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    /// The outcome.
    pub outcome: EffectOutcome,
    /// The status.
    pub status: ObservedStatus,
    /// The exit status, when a process ran.
    pub exit_status: Option<i64>,
    /// The manifest ref (the capture's identity).
    pub manifest_ref: String,
    /// The manifest's completeness.
    pub completeness: crate::capture::Completeness,
}

impl Observation {
    /// The `action.effect.observed` payload member.
    pub fn to_json(&self) -> Json {
        let status = match &self.status {
            ObservedStatus::Ok => Json::str("ok"),
            ObservedStatus::Error {
                class,
                origin,
                detail_ref,
                retryable,
            } => {
                let mut c = class.to_json();
                if let Json::Obj(m) = &mut c {
                    m.insert("origin".to_string(), Json::str(origin.as_str()));
                    m.insert(
                        "detail_ref".to_string(),
                        detail_ref
                            .as_ref()
                            .map_or(Json::Null, |r| Json::str(r.clone())),
                    );
                    m.insert("retryable".to_string(), Json::Bool(*retryable));
                }
                Json::obj([("error", c)])
            }
        };
        Json::obj([
            ("outcome", Json::str(self.outcome.as_str())),
            ("status", status),
            (
                "exit_status",
                self.exit_status.map_or(Json::Null, Json::Int),
            ),
            ("capture_manifest_ref", Json::str(self.manifest_ref.clone())),
            ("completeness", self.completeness.to_json()),
        ])
    }
}
