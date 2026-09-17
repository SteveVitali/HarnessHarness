//! The gateway's typed failure vocabulary (§5b.1 §5 failure modes; ADR-0118 d.2).
//! Every refusal is a typed error, never a warning or a silent fallback.

/// The strict-codec error family (the recent-ticket convention — `BadMember`
/// on an unknown member, never a silent ignore).
#[derive(Debug, Clone, PartialEq)]
pub enum CodecError {
    /// An unknown member on a closed record.
    BadMember {
        /// The record name.
        record: &'static str,
        /// The offending member.
        member: String,
    },
    /// A required member is absent.
    MissingMember {
        /// The record name.
        record: &'static str,
        /// The missing member.
        member: &'static str,
    },
    /// A member has the wrong canonical type.
    TypeMismatch {
        /// The member path.
        member: String,
        /// What was expected.
        expected: &'static str,
    },
    /// A closed-enum member carried an out-of-vocabulary value.
    UnknownVariant {
        /// The member path.
        member: String,
        /// The observed value.
        value: String,
    },
    /// The record's declared `schema` member does not match.
    BadSchema {
        /// The observed spelling.
        found: String,
        /// The required spelling.
        expected: &'static str,
    },
    /// A conditioned rule / dialect rule's debt record is incomplete
    /// (T-LCD-05 reflexive — the document fails to load).
    IncompleteDebt {
        /// The rule whose debt record is incomplete.
        rule_id: String,
    },
    /// A content-addressed member does not match its declared value.
    NonCanonical {
        /// The member path.
        member: String,
        /// The mismatch detail.
        detail: String,
    },
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodecError::BadMember { record, member } => {
                write!(f, "BadMember: {record} has unknown member {member}")
            }
            CodecError::MissingMember { record, member } => {
                write!(f, "MissingMember: {record} requires {member}")
            }
            CodecError::TypeMismatch { member, expected } => {
                write!(f, "TypeMismatch: {member} must be {expected}")
            }
            CodecError::UnknownVariant { member, value } => {
                write!(f, "UnknownVariant: {member} = {value}")
            }
            CodecError::BadSchema { found, expected } => {
                write!(f, "BadSchema: expected {expected}, got {found}")
            }
            CodecError::IncompleteDebt { rule_id } => {
                write!(
                    f,
                    "IncompleteDebt: rule {rule_id} lacks a complete AssumptionDebtRecord"
                )
            }
            CodecError::NonCanonical { member, detail } => {
                write!(f, "NonCanonical: {member}: {detail}")
            }
        }
    }
}
impl std::error::Error for CodecError {}

/// `GatewayError` — the §5b.1 typed-failure set (closed at the contract; growth
/// is a contract bump, never an open string).
#[derive(Debug)]
pub enum GatewayError {
    /// `plan.dialect` names no loaded `WireDialect`.
    UnknownDialect {
        /// The dialect coordinate the plan named.
        dialect: String,
    },
    /// The plan violates the dialect's `plan_schema` (unregistered
    /// `provider_params` key, missing required member, or a
    /// `provider_params` value the dialect does not admit).
    PlanSchemaViolation {
        /// What failed the schema.
        detail: String,
    },
    /// `plan.endpoint_ref` is outside `WireDialect.endpoint_allowlist_ref`
    /// (H4 — refused before any bytes leave).
    EndpointNotAllowed {
        /// The endpoint coordinate refused.
        endpoint: String,
    },
    /// No credential could be bound for the call (broker refusal).
    CredentialUnavailable {
        /// The refusal detail (content-free).
        detail: String,
    },
    /// `request.budget_reservation_id` is absent — reserve-before-spend
    /// (ADR-0040).
    MissingReservation {
        /// The call that arrived without a reservation.
        model_call_id: String,
    },
    /// `estimate_tokens` cannot produce an estimate for the plan.
    EstimateUnavailable {
        /// Why.
        detail: String,
    },
    /// `discover`/`probe` could not reach the endpoint.
    EndpointUnavailable {
        /// The endpoint coordinate.
        endpoint: String,
    },
    /// `probe` exceeded its budget.
    ProbeBudgetExhausted {
        /// The probe kind.
        kind: String,
    },
    /// The call failed in transport/normalization — the classified error.
    ModelError(crate::vocab::ModelError),
    /// A wire frame or event violated the closed grammar/schema.
    Codec(CodecError),
    /// The ledger refused the append (durable-before-visible — the caller
    /// sees the refusal, never a silently-dropped audit row).
    Ledger {
        /// The ledger's detail.
        detail: String,
    },
}

impl std::fmt::Display for GatewayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GatewayError::UnknownDialect { dialect } => {
                write!(f, "UnknownDialect: {dialect}")
            }
            GatewayError::PlanSchemaViolation { detail } => {
                write!(f, "PlanSchemaViolation: {detail}")
            }
            GatewayError::EndpointNotAllowed { endpoint } => {
                write!(f, "EndpointNotAllowed: {endpoint}")
            }
            GatewayError::CredentialUnavailable { detail } => {
                write!(f, "CredentialUnavailable: {detail}")
            }
            GatewayError::MissingReservation { model_call_id } => {
                write!(f, "MissingReservation: {model_call_id}")
            }
            GatewayError::EstimateUnavailable { detail } => {
                write!(f, "EstimateUnavailable: {detail}")
            }
            GatewayError::EndpointUnavailable { endpoint } => {
                write!(f, "EndpointUnavailable: {endpoint}")
            }
            GatewayError::ProbeBudgetExhausted { kind } => {
                write!(f, "ProbeBudgetExhausted: {kind}")
            }
            GatewayError::ModelError(e) => write!(f, "{e}"),
            GatewayError::Codec(c) => write!(f, "{c}"),
            GatewayError::Ledger { detail } => write!(f, "Ledger: {detail}"),
        }
    }
}
impl std::error::Error for GatewayError {}

impl From<CodecError> for GatewayError {
    fn from(e: CodecError) -> Self {
        GatewayError::Codec(e)
    }
}
