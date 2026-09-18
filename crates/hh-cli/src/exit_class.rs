//! `ExitClass` — the closed string sum of §7.1 §2.5 (ADR-0169 D6), the
//! total derivation from `outcome_class × StopReason` plus the pre-ledger
//! classes, and the interim numeral band.
//!
//! The numerals are MUST-data (ADR-0210/OQ-384 — the band is unsettled
//! upstream); the interim band is `64–73`, chosen inside the reserved
//! region (never 1, 2, 126–165 or 255) and disjoint from the BSD
//! `sysexits` meanings at 64–78 only by convention — `result.exit_class`
//! is the authoritative channel and scripts must branch on the string.

use hh_embed_client_generated::EmbedError;
use hh_wire::json::Json;

/// The closed `ExitClass` sum (§7.1 §2.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitClass {
    /// `outcome_class = scored ∧ stop_reason = completed` (also a
    /// non-run command's clean result — `version`, `doctor`, reads).
    Ok,
    /// A pre-ledger failure: flag conflict, `interactive` without a TTY,
    /// unknown format, `MissingBudget`, `BypassWithoutContainment`,
    /// `AuthorityWideningRequiresHuman` — never opens a run.
    InvocationError,
    /// `AssemblyDiagnostic{severity: error}` / engine refusals —
    /// never opens a run (`InvalidDefinition`, `UnresolvedRef`,
    /// `AmbiguousVersion`, `UnbudgetedArm`, schema-level violations).
    ValidationError,
    /// A kernel refusal of the invocation after `hello` and before a
    /// turn runs — the CF-487/OQ-470 interim reading:
    /// `Refused`, `InsufficientBudget`, `AuthorityViolation`,
    /// `WouldBlock`, `Draining`, `SessionDetached`, `Fenced`,
    /// `UnattendedRequiresInput`, the `Unknown*` and `Already*` refusals.
    RefusedByKernel,
    /// `stop_reason = budget_exhausted` (incl. the `approvals_exhausted`
    /// alias — ADR-0145 §E).
    BudgetExhausted,
    /// `outcome_class = refused`.
    Refused,
    /// `stop_reason = cancelled{by}`.
    Cancelled,
    /// `outcome_class = infrastructure_failure` (also transport/spawn
    /// failures — the kernel process itself is infrastructure).
    InfrastructureFailure,
    /// `outcome_class = oracle_failure`.
    OracleFailure,
    /// Approvals exhausted / completion blocked by an unattended `deny`
    /// (ADR-0071 D4) — a `refused`-class terminal where the ledger
    /// carries `security.permission.decided{decider: policy,
    /// reason: unattended}` rows.
    ApprovalDeniedUnattended,
    /// `Tampered` (integrity failure — the ledger's own verdict).
    Tampered,
}

impl ExitClass {
    /// The canonical spelling (`result.exit_class` — authoritative).
    pub fn as_str(self) -> &'static str {
        match self {
            ExitClass::Ok => "ok",
            ExitClass::InvocationError => "invocation_error",
            ExitClass::ValidationError => "validation_error",
            ExitClass::RefusedByKernel => "refused_by_kernel",
            ExitClass::BudgetExhausted => "budget_exhausted",
            ExitClass::Refused => "refused",
            ExitClass::Cancelled => "cancelled",
            ExitClass::InfrastructureFailure => "infrastructure_failure",
            ExitClass::OracleFailure => "oracle_failure",
            ExitClass::ApprovalDeniedUnattended => "approval_denied_unattended",
            ExitClass::Tampered => "tampered",
        }
    }

    /// The interim process numeral (ADR-0210 band — `64–73`, inside the
    /// reserved region and clear of 1, 2, 126–165, 255).
    pub fn code(self) -> i32 {
        match self {
            ExitClass::Ok => 0,
            ExitClass::InvocationError => 64,
            ExitClass::ValidationError => 65,
            ExitClass::RefusedByKernel => 66,
            ExitClass::BudgetExhausted => 67,
            ExitClass::Refused => 68,
            ExitClass::Cancelled => 69,
            ExitClass::InfrastructureFailure => 70,
            ExitClass::OracleFailure => 71,
            ExitClass::ApprovalDeniedUnattended => 72,
            ExitClass::Tampered => 73,
        }
    }
}

/// Derive the class from a kernel `EmbedError` — post-`hello` refusals
/// map per the CF-487 interim reading (OQ-470); validation-class errors
/// are `validation_error`; the pre-ledger widening refusal is
/// `invocation_error` by the derivation table's own row.
pub fn exit_class_for_kernel_error(e: &EmbedError) -> ExitClass {
    match e.kind.as_str() {
        // The derivation table's invocation_error row names this kind
        // verbatim — the refusal is emitted before any run opens.
        "AuthorityWideningRequiresHuman" => ExitClass::InvocationError,
        // Engine / schema-level refusals — validation_error.
        "InvalidDefinition"
        | "UnresolvedRef"
        | "AmbiguousVersion"
        | "UnbudgetedArm"
        | "SchemaViolation"
        | "UnknownField"
        | "UnexpressibleSurface"
        | "LinkError"
        | "SchemaMismatch"
        | "ContractMajorUnsupported"
        | "KernelBelowFloor" => ExitClass::ValidationError,
        // Everything else post-hello is a kernel refusal of the
        // invocation (CF-487): `Refused`, `InsufficientBudget`,
        // `AuthorityViolation`, `WouldBlock`, `Draining`,
        // `SessionDetached`, `Fenced`, `TurnMismatch`, `TurnActive`,
        // `UnattendedRequiresInput`, `SecretInPayload`, `Unknown*`,
        // `Already*`, `Overloaded`, `Timeout`, `Disconnected`,
        // `ExperimentalRequired`, `CapabilityNotDeclared`,
        // `NotInitialized`, `UnknownCapability`, `EnvironmentUnavailable`,
        // `Unsupported`.
        _ => ExitClass::RefusedByKernel,
    }
}

/// Derive the class from a finished run's `stop_reason` +
/// `outcome_class` (the ledger's own words — never re-derived from
/// process state). `unattended_denies` is the count of
/// `security.permission.decided{decider: policy, reason: unattended}`
/// rows in the run's prefix — the `approval_denied_unattended` row's
/// ledger fact.
pub fn exit_class_for_terminal(
    stop_reason: &str,
    outcome_class: &str,
    unattended_denies: usize,
) -> ExitClass {
    if stop_reason.starts_with("cancelled") {
        return ExitClass::Cancelled;
    }
    if stop_reason.starts_with("budget_exhausted") || stop_reason.starts_with("approvals_exhausted")
    {
        return ExitClass::BudgetExhausted;
    }
    match outcome_class {
        "scored" => ExitClass::Ok,
        "refused" => {
            if unattended_denies > 0 {
                ExitClass::ApprovalDeniedUnattended
            } else {
                ExitClass::Refused
            }
        }
        "infrastructure_failure" => ExitClass::InfrastructureFailure,
        "oracle_failure" => ExitClass::OracleFailure,
        _ => ExitClass::Ok,
    }
}

/// The `stop_reason`/`outcome_class` members of a `lifecycle.run.finished`
/// payload (`{status, stop_reason, outcome_class, …}`) plus the
/// unattended-deny count — the terminal facts the derivation reads.
pub fn terminal_of(events: &[Json]) -> (String, String, usize) {
    let mut stop = String::new();
    let mut outcome = String::new();
    let mut denies = 0usize;
    for e in events {
        let class = e.get("class").and_then(Json::as_str).unwrap_or("");
        match class {
            "lifecycle.run.finished" => {
                if let Some(p) = e.get("payload") {
                    if let Some(r) = p.get("stop_reason") {
                        stop = reason_spelling(r);
                    }
                    if let Some(o) = p.get("outcome_class").and_then(Json::as_str) {
                        outcome = o.to_string();
                    }
                }
            }
            "security.permission.decided" => {
                let p = e.get("payload");
                let decider = p
                    .and_then(|p| p.get("decider"))
                    .and_then(Json::as_str)
                    .unwrap_or("");
                let reason = p
                    .and_then(|p| p.get("reason"))
                    .and_then(Json::as_str)
                    .unwrap_or("");
                if decider == "policy" && reason == "unattended" {
                    denies += 1;
                }
            }
            _ => {}
        }
    }
    (stop, outcome, denies)
}

/// A `stop_reason` member is `{kind:"cancelled", by}` or a bare string —
/// both spellings reduce to `kind{member}` for the derivation.
fn reason_spelling(r: &Json) -> String {
    match r {
        Json::Str(s) => s.clone(),
        Json::Obj(_) => {
            let kind = r.get("kind").and_then(Json::as_str).unwrap_or("");
            if kind == "cancelled" {
                let by = r.get("by").and_then(Json::as_str).unwrap_or("principal");
                format!("cancelled{{by:{by}}}")
            } else if kind == "budget_exhausted" {
                let dim = r.get("dimension").and_then(Json::as_str).unwrap_or("");
                format!("budget_exhausted{{{dim}}}")
            } else {
                kind.to_string()
            }
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(kind: &str) -> EmbedError {
        EmbedError {
            code: 0,
            kind: kind.into(),
            retryable: false,
            message: String::new(),
            data: Json::Null,
        }
    }

    #[test]
    fn pre_ledger_kinds_map_to_invocation_error() {
        assert_eq!(
            exit_class_for_kernel_error(&err("AuthorityWideningRequiresHuman")),
            ExitClass::InvocationError
        );
    }

    #[test]
    fn engine_refusals_map_to_validation_error() {
        for k in [
            "InvalidDefinition",
            "UnresolvedRef",
            "SchemaViolation",
            "UnbudgetedArm",
        ] {
            assert_eq!(
                exit_class_for_kernel_error(&err(k)),
                ExitClass::ValidationError,
                "{k}"
            );
        }
    }

    #[test]
    fn post_hello_refusals_map_to_refused_by_kernel() {
        for k in [
            "Refused",
            "InsufficientBudget",
            "AuthorityViolation",
            "WouldBlock",
            "Draining",
            "SessionDetached",
            "UnknownSession",
            "AlreadyDecided",
            "UnattendedRequiresInput",
            "SecretInPayload",
        ] {
            assert_eq!(
                exit_class_for_kernel_error(&err(k)),
                ExitClass::RefusedByKernel,
                "{k}"
            );
        }
    }

    #[test]
    fn terminal_derivation_covers_the_table() {
        assert_eq!(
            exit_class_for_terminal("completed", "scored", 0),
            ExitClass::Ok
        );
        assert_eq!(
            exit_class_for_terminal("budget_exhausted{turns}", "budget_exhausted", 0),
            ExitClass::BudgetExhausted
        );
        assert_eq!(
            exit_class_for_terminal("approvals_exhausted", "refused", 0),
            ExitClass::BudgetExhausted
        );
        assert_eq!(
            exit_class_for_terminal("cancelled{by:principal}", "cancelled", 0),
            ExitClass::Cancelled
        );
        assert_eq!(
            exit_class_for_terminal("x", "refused", 2),
            ExitClass::ApprovalDeniedUnattended
        );
        assert_eq!(
            exit_class_for_terminal("x", "refused", 0),
            ExitClass::Refused
        );
        assert_eq!(
            exit_class_for_terminal("x", "infrastructure_failure", 0),
            ExitClass::InfrastructureFailure
        );
        assert_eq!(
            exit_class_for_terminal("x", "oracle_failure", 0),
            ExitClass::OracleFailure
        );
    }

    #[test]
    fn cancelled_stop_reason_maps_to_cancelled() {
        assert_eq!(
            exit_class_for_terminal("cancelled{by:principal}", "cancelled", 0),
            ExitClass::Cancelled
        );
    }

    #[test]
    fn numerals_stay_in_the_reserved_band() {
        for c in [
            ExitClass::Ok,
            ExitClass::InvocationError,
            ExitClass::ValidationError,
            ExitClass::RefusedByKernel,
            ExitClass::BudgetExhausted,
            ExitClass::Refused,
            ExitClass::Cancelled,
            ExitClass::InfrastructureFailure,
            ExitClass::OracleFailure,
            ExitClass::ApprovalDeniedUnattended,
            ExitClass::Tampered,
        ] {
            let n = c.code();
            assert!(
                n == 0 || (!matches!(n, 1 | 2 | 255) && !(126..=165).contains(&n)),
                "{n} collides with the reserved band"
            );
        }
    }
}
