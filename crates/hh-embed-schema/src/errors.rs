//! The closed `EmbedError` sum (§7.4 §2.6; ADR-0176 D5). Every operation
//! returns a typed sum — never throws, never free text. `retryable` means
//! "safe to retry with the same `idempotency_key`"; kernel-internal
//! conditions (`Fenced` on the writer, `Tampered`) surface only as
//! `SessionDetached{reason, event_ref}` at the boundary.

use hh_wire::json::Json;

/// The closed error sum. Response-position closed sums grow only by a
/// dialect bump (ADR-0178 D3) — every variant is declared here at Stage 1
/// even where its first emitter lands later (the `Unsupported{by}` member
/// the `steer` op table names is declared now, emitted from Stage 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedError {
    // ── Handshake ────────────────────────────────────────────────────────
    /// An operation arrived before `hello` completed on this connection.
    NotInitialized,
    /// The asserted `contract_major` is not implemented.
    ContractMajorUnsupported { requested: i64, supported: Vec<i64> },
    /// The asserted `schema_hash` differs; `direction` ∈
    /// {client_newer, kernel_newer} per the ADR-0178 D8 matrix.
    SchemaMismatch {
        client: String,
        kernel: String,
        direction: String,
    },
    /// The client's declared `kernel_floor` is above this kernel.
    KernelBelowFloor { version: String, floor: String },
    /// An `experimental`-tier item called without
    /// `capabilities.experimental = true`.
    ExperimentalRequired { reason: String },
    /// A capability-gated call without the declared capability.
    CapabilityNotDeclared { capability: String },
    // ── Shape ────────────────────────────────────────────────────────────
    /// An unknown non-`ext` field (I2; tolerant-reader drift refused).
    UnknownField { path: String },
    /// A typed violation: `{path, code}` (code is the check's stable tag).
    SchemaViolation { path: String, code: String },
    /// Secret material detected in a payload (ADR-0177 D9; ledgered
    /// `security.secret.leak_detected`).
    SecretInPayload,
    // ── Lifecycle ────────────────────────────────────────────────────────
    /// The `session_id` names no live session on this connection.
    UnknownSession,
    /// The `run_id` names no run.
    UnknownRun { run_id: String },
    /// A writer lease is held by another attachment (ADR-0130).
    WouldBlock { active_holder: String },
    /// The run is draining; work-injecting calls are refused.
    Draining,
    /// The session was detached by the kernel (`{reason, event_ref?}` —
    /// the only surface for writer-side `Fenced`/`Tampered`).
    SessionDetached {
        reason: String,
        event_ref: Option<String>,
    },
    /// The resumed definition no longer verifies (`verify_resume`).
    DefinitionChanged { reasons: Vec<String> },
    /// `steer`/`cancel` named a turn that is not the active one.
    TurnMismatch { active_turn_id: String },
    /// A second `submit` while `concurrent_input = queue_only` forbids it.
    TurnActive,
    // ── Definition / assembly (ADR-0019/0025 verbatim) ──────────────────
    /// The document failed `resolve`/`validate_assembly`/`seal` — the
    /// complete diagnostic set (never a half-resolved document).
    InvalidDefinition { diagnostics: Vec<String> },
    /// A `Ref` that resolve could not pin.
    UnresolvedRef { reference: String },
    /// A selector matching more than one candidate.
    AmbiguousVersion { reference: String },
    /// An override/diff widening a cap — names the capping layer
    /// (ADR-0024 D6).
    AuthorityViolation { layer: String, detail: String },
    /// A surface the profile binding cannot express.
    UnexpressibleSurface { detail: String },
    /// A link-time refusal (ADR-0019).
    LinkError { detail: String },
    /// An override whose `HirDiff` classification widens authority or
    /// loosens budget from a non-interactive, unattested invocation
    /// (I-1; ADR-0168 D4; §7.1 — refused before any run opens).
    AuthorityWideningRequiresHuman { detail: String },
    // ── Budget / policy ──────────────────────────────────────────────────
    /// The declared budget cannot cover the arm/dispatch.
    InsufficientBudget { dimension: Option<String> },
    /// An unbudgeted arm attempt.
    UnbudgetedArm,
    /// `attendance = unattended` requires a declared input class.
    UnattendedRequiresInput,
    /// A policy/strategy refusal — `{reason}` names the refusing rule.
    Refused { reason: String },
    /// The named capability/strategy does not support the op
    /// (the `steer` row's `Unsupported{by}` — declared now, emitted at
    /// Stage 4 when `steer` lands).
    Unsupported { by: String },
    // ── Effects ──────────────────────────────────────────────────────────
    /// The `effect_id` names no effect in this run.
    UnknownEffect { effect_id: String },
    /// The `permission_id` is already decided (a duplicate with a
    /// *different* key — a same-key retry returns the recorded result).
    AlreadyDecided { permission_id: String },
    /// `respond_permission` selected an option the upcall never offered.
    OptionNotOffered { option_id: String },
    /// The `permission_id` names no live pending.
    UnknownPermission { permission_id: String },
    /// The writer lease fenced this attachment mid-call.
    Fenced { detail: String },
    // ── Environment ──────────────────────────────────────────────────────
    /// Provisioning/attach failed — `{reason}` names the cause.
    EnvironmentUnavailable { reason: String },
    /// The named capability is not bound in this run.
    UnknownCapability { capability: String },
    // ── Transport (binding-level; may be added, never removed) ──────────
    /// A bounded in-process queue saturated (binding (a)).
    Overloaded,
    /// The binding lost its peer mid-call.
    Disconnected,
    /// The call exceeded the binding's deadline.
    Timeout,
}

impl EmbedError {
    /// The stable JSON-RPC error code for this variant (mirrored in the
    /// schema export so generated clients map codes back to variants).
    pub fn code(&self) -> i64 {
        match self {
            EmbedError::NotInitialized => 1001,
            EmbedError::ContractMajorUnsupported { .. } => 1002,
            EmbedError::SchemaMismatch { .. } => 1003,
            EmbedError::KernelBelowFloor { .. } => 1004,
            EmbedError::ExperimentalRequired { .. } => 1005,
            EmbedError::CapabilityNotDeclared { .. } => 1006,
            EmbedError::UnknownField { .. } => 1100,
            EmbedError::SchemaViolation { .. } => 1101,
            EmbedError::SecretInPayload => 1102,
            EmbedError::UnknownSession => 1200,
            EmbedError::UnknownRun { .. } => 1201,
            EmbedError::WouldBlock { .. } => 1202,
            EmbedError::Draining => 1203,
            EmbedError::SessionDetached { .. } => 1204,
            EmbedError::DefinitionChanged { .. } => 1205,
            EmbedError::TurnMismatch { .. } => 1206,
            EmbedError::TurnActive => 1207,
            EmbedError::InvalidDefinition { .. } => 1300,
            EmbedError::UnresolvedRef { .. } => 1301,
            EmbedError::AmbiguousVersion { .. } => 1302,
            EmbedError::AuthorityViolation { .. } => 1303,
            EmbedError::UnexpressibleSurface { .. } => 1304,
            EmbedError::LinkError { .. } => 1305,
            EmbedError::AuthorityWideningRequiresHuman { .. } => 1306,
            EmbedError::InsufficientBudget { .. } => 1400,
            EmbedError::UnbudgetedArm => 1401,
            EmbedError::UnattendedRequiresInput => 1402,
            EmbedError::Refused { .. } => 1403,
            EmbedError::Unsupported { .. } => 1404,
            EmbedError::UnknownEffect { .. } => 1500,
            EmbedError::AlreadyDecided { .. } => 1501,
            EmbedError::OptionNotOffered { .. } => 1502,
            EmbedError::UnknownPermission { .. } => 1503,
            EmbedError::Fenced { .. } => 1504,
            EmbedError::EnvironmentUnavailable { .. } => 1600,
            EmbedError::UnknownCapability { .. } => 1601,
            EmbedError::Overloaded => 1700,
            EmbedError::Disconnected => 1701,
            EmbedError::Timeout => 1702,
        }
    }

    /// The variant tag, as it appears in `error.data.kind`.
    pub fn kind(&self) -> &'static str {
        match self {
            EmbedError::NotInitialized => "NotInitialized",
            EmbedError::ContractMajorUnsupported { .. } => "ContractMajorUnsupported",
            EmbedError::SchemaMismatch { .. } => "SchemaMismatch",
            EmbedError::KernelBelowFloor { .. } => "KernelBelowFloor",
            EmbedError::ExperimentalRequired { .. } => "ExperimentalRequired",
            EmbedError::CapabilityNotDeclared { .. } => "CapabilityNotDeclared",
            EmbedError::UnknownField { .. } => "UnknownField",
            EmbedError::SchemaViolation { .. } => "SchemaViolation",
            EmbedError::SecretInPayload => "SecretInPayload",
            EmbedError::UnknownSession => "UnknownSession",
            EmbedError::UnknownRun { .. } => "UnknownRun",
            EmbedError::WouldBlock { .. } => "WouldBlock",
            EmbedError::Draining => "Draining",
            EmbedError::SessionDetached { .. } => "SessionDetached",
            EmbedError::DefinitionChanged { .. } => "DefinitionChanged",
            EmbedError::TurnMismatch { .. } => "TurnMismatch",
            EmbedError::TurnActive => "TurnActive",
            EmbedError::InvalidDefinition { .. } => "InvalidDefinition",
            EmbedError::UnresolvedRef { .. } => "UnresolvedRef",
            EmbedError::AmbiguousVersion { .. } => "AmbiguousVersion",
            EmbedError::AuthorityViolation { .. } => "AuthorityViolation",
            EmbedError::UnexpressibleSurface { .. } => "UnexpressibleSurface",
            EmbedError::LinkError { .. } => "LinkError",
            EmbedError::AuthorityWideningRequiresHuman { .. } => "AuthorityWideningRequiresHuman",
            EmbedError::InsufficientBudget { .. } => "InsufficientBudget",
            EmbedError::UnbudgetedArm => "UnbudgetedArm",
            EmbedError::UnattendedRequiresInput => "UnattendedRequiresInput",
            EmbedError::Refused { .. } => "Refused",
            EmbedError::Unsupported { .. } => "Unsupported",
            EmbedError::UnknownEffect { .. } => "UnknownEffect",
            EmbedError::AlreadyDecided { .. } => "AlreadyDecided",
            EmbedError::OptionNotOffered { .. } => "OptionNotOffered",
            EmbedError::UnknownPermission { .. } => "UnknownPermission",
            EmbedError::Fenced { .. } => "Fenced",
            EmbedError::EnvironmentUnavailable { .. } => "EnvironmentUnavailable",
            EmbedError::UnknownCapability { .. } => "UnknownCapability",
            EmbedError::Overloaded => "Overloaded",
            EmbedError::Disconnected => "Disconnected",
            EmbedError::Timeout => "Timeout",
        }
    }

    /// `retryable` — safe to retry with the same `idempotency_key`
    /// (§7.4 §2.6). Transient/binding conditions only; every refusal is
    /// terminal.
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            EmbedError::WouldBlock { .. }
                | EmbedError::Draining
                | EmbedError::Overloaded
                | EmbedError::Disconnected
                | EmbedError::Timeout
        )
    }

    /// The human-readable `error.message` (never the typed channel — the
    /// typed members live in `error.data`).
    pub fn message(&self) -> String {
        match self {
            EmbedError::NotInitialized => "hello has not completed on this connection".into(),
            EmbedError::ContractMajorUnsupported { requested, .. } => {
                format!("contract major {requested} unsupported")
            }
            EmbedError::SchemaMismatch { direction, .. } => {
                format!("schema hash mismatch ({direction})")
            }
            EmbedError::KernelBelowFloor { version, floor } => {
                format!("kernel {version} below declared floor {floor}")
            }
            EmbedError::ExperimentalRequired { reason } => {
                format!("{reason} requires capabilities.experimental")
            }
            EmbedError::CapabilityNotDeclared { capability } => {
                format!("capability {capability} not declared at hello")
            }
            EmbedError::UnknownField { path } => format!("unknown field at {path}"),
            EmbedError::SchemaViolation { path, code } => {
                format!("schema violation {code} at {path}")
            }
            EmbedError::SecretInPayload => "secret material in payload".into(),
            EmbedError::UnknownSession => "unknown session".into(),
            EmbedError::UnknownRun { run_id } => format!("unknown run {run_id}"),
            EmbedError::WouldBlock { active_holder } => {
                format!("writer lease held by {active_holder}")
            }
            EmbedError::Draining => "run draining".into(),
            EmbedError::SessionDetached { reason, .. } => {
                format!("session detached: {reason}")
            }
            EmbedError::DefinitionChanged { .. } => "definition changed since resume".into(),
            EmbedError::TurnMismatch { active_turn_id } => {
                format!("turn mismatch; active turn is {active_turn_id}")
            }
            EmbedError::TurnActive => "a turn is already active".into(),
            EmbedError::InvalidDefinition { diagnostics } => {
                format!("invalid definition ({} diagnostics)", diagnostics.len())
            }
            EmbedError::UnresolvedRef { reference } => {
                format!("unresolved ref {reference}")
            }
            EmbedError::AmbiguousVersion { reference } => {
                format!("ambiguous version {reference}")
            }
            EmbedError::AuthorityViolation { layer, detail } => {
                format!("authority violation at layer {layer}: {detail}")
            }
            EmbedError::UnexpressibleSurface { detail } => {
                format!("unexpressible surface: {detail}")
            }
            EmbedError::LinkError { detail } => format!("link error: {detail}"),
            EmbedError::AuthorityWideningRequiresHuman { detail } => {
                format!("authority widening requires a human invocation: {detail}")
            }
            EmbedError::InsufficientBudget { dimension } => match dimension {
                Some(d) => format!("insufficient budget on {d}"),
                None => "insufficient budget".into(),
            },
            EmbedError::UnbudgetedArm => "unbudgeted arm".into(),
            EmbedError::UnattendedRequiresInput => {
                "unattended attendance requires a declared input class".into()
            }
            EmbedError::Refused { reason } => format!("refused: {reason}"),
            EmbedError::Unsupported { by } => format!("unsupported by {by}"),
            EmbedError::UnknownEffect { effect_id } => {
                format!("unknown effect {effect_id}")
            }
            EmbedError::AlreadyDecided { permission_id } => {
                format!("permission {permission_id} already decided")
            }
            EmbedError::OptionNotOffered { option_id } => {
                format!("option {option_id} was not offered")
            }
            EmbedError::UnknownPermission { permission_id } => {
                format!("unknown permission {permission_id}")
            }
            EmbedError::Fenced { detail } => format!("fenced: {detail}"),
            EmbedError::EnvironmentUnavailable { reason } => {
                format!("environment unavailable: {reason}")
            }
            EmbedError::UnknownCapability { capability } => {
                format!("unknown capability {capability}")
            }
            EmbedError::Overloaded => "binding overloaded".into(),
            EmbedError::Disconnected => "binding disconnected".into(),
            EmbedError::Timeout => "binding timeout".into(),
        }
    }

    /// The typed `error.data` payload: `{kind, retryable, ...members}`.
    /// `detail`/`event_ref` ride along when the variant carries them.
    pub fn to_data_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("kind".to_string(), Json::str(self.kind()));
        m.insert("retryable".to_string(), Json::Bool(self.retryable()));
        let put = |m: &mut std::collections::BTreeMap<String, Json>, k: &str, v: Json| {
            m.insert(k.to_string(), v);
        };
        match self {
            EmbedError::ContractMajorUnsupported {
                requested,
                supported,
            } => {
                put(&mut m, "requested", Json::Int(*requested));
                put(
                    &mut m,
                    "supported",
                    Json::Arr(supported.iter().map(|s| Json::Int(*s)).collect()),
                );
            }
            EmbedError::SchemaMismatch {
                client,
                kernel,
                direction,
            } => {
                put(&mut m, "client", Json::str(client.clone()));
                put(&mut m, "kernel", Json::str(kernel.clone()));
                put(&mut m, "direction", Json::str(direction.clone()));
            }
            EmbedError::KernelBelowFloor { version, floor } => {
                put(&mut m, "version", Json::str(version.clone()));
                put(&mut m, "floor", Json::str(floor.clone()));
            }
            EmbedError::ExperimentalRequired { reason }
            | EmbedError::Refused { reason }
            | EmbedError::SessionDetached { reason, .. } => {
                put(&mut m, "reason", Json::str(reason.clone()));
                if let EmbedError::SessionDetached {
                    event_ref: Some(er),
                    ..
                } = self
                {
                    put(&mut m, "event_ref", Json::str(er.clone()));
                }
            }
            EmbedError::CapabilityNotDeclared { capability }
            | EmbedError::UnknownCapability { capability } => {
                put(&mut m, "capability", Json::str(capability.clone()));
            }
            EmbedError::UnknownField { path } => {
                put(&mut m, "path", Json::str(path.clone()));
            }
            EmbedError::SchemaViolation { path, code } => {
                put(&mut m, "path", Json::str(path.clone()));
                put(&mut m, "code", Json::str(code.clone()));
            }
            EmbedError::UnknownRun { run_id } => {
                put(&mut m, "run_id", Json::str(run_id.clone()));
            }
            EmbedError::WouldBlock { active_holder } => {
                put(&mut m, "active_holder", Json::str(active_holder.clone()));
            }
            EmbedError::DefinitionChanged { reasons } => {
                put(
                    &mut m,
                    "reasons",
                    Json::Arr(reasons.iter().map(|r| Json::str(r.clone())).collect()),
                );
            }
            EmbedError::TurnMismatch { active_turn_id } => {
                put(&mut m, "active_turn_id", Json::str(active_turn_id.clone()));
            }
            EmbedError::InvalidDefinition { diagnostics } => {
                put(
                    &mut m,
                    "diagnostics",
                    Json::Arr(diagnostics.iter().map(|d| Json::str(d.clone())).collect()),
                );
            }
            EmbedError::UnresolvedRef { reference }
            | EmbedError::AmbiguousVersion { reference } => {
                put(&mut m, "reference", Json::str(reference.clone()));
            }
            EmbedError::AuthorityViolation { layer, detail } => {
                put(&mut m, "layer", Json::str(layer.clone()));
                put(&mut m, "detail", Json::str(detail.clone()));
            }
            EmbedError::UnexpressibleSurface { detail }
            | EmbedError::LinkError { detail }
            | EmbedError::AuthorityWideningRequiresHuman { detail }
            | EmbedError::Fenced { detail } => {
                put(&mut m, "detail", Json::str(detail.clone()));
            }
            EmbedError::InsufficientBudget { dimension } => {
                if let Some(d) = dimension {
                    put(&mut m, "dimension", Json::str(d.clone()));
                }
            }
            EmbedError::Unsupported { by } => {
                put(&mut m, "by", Json::str(by.clone()));
            }
            EmbedError::UnknownEffect { effect_id } => {
                put(&mut m, "effect_id", Json::str(effect_id.clone()));
            }
            EmbedError::AlreadyDecided { permission_id }
            | EmbedError::UnknownPermission { permission_id } => {
                put(&mut m, "permission_id", Json::str(permission_id.clone()));
            }
            EmbedError::OptionNotOffered { option_id } => {
                put(&mut m, "option_id", Json::str(option_id.clone()));
            }
            EmbedError::EnvironmentUnavailable { reason } => {
                put(&mut m, "reason", Json::str(reason.clone()));
            }
            EmbedError::NotInitialized
            | EmbedError::SecretInPayload
            | EmbedError::UnknownSession
            | EmbedError::Draining
            | EmbedError::TurnActive
            | EmbedError::UnbudgetedArm
            | EmbedError::UnattendedRequiresInput
            | EmbedError::Overloaded
            | EmbedError::Disconnected
            | EmbedError::Timeout => {}
        }
        Json::Obj(m)
    }
}

/// Every variant tag, in declaration order — the conformance suite and the
/// schema export enumerate from this (CC1: the Rust sum and the export
/// cannot drift; a test asserts the lengths agree).
pub const ALL_ERROR_KINDS: &[&str] = &[
    "NotInitialized",
    "ContractMajorUnsupported",
    "SchemaMismatch",
    "KernelBelowFloor",
    "ExperimentalRequired",
    "CapabilityNotDeclared",
    "UnknownField",
    "SchemaViolation",
    "SecretInPayload",
    "UnknownSession",
    "UnknownRun",
    "WouldBlock",
    "Draining",
    "SessionDetached",
    "DefinitionChanged",
    "TurnMismatch",
    "TurnActive",
    "InvalidDefinition",
    "UnresolvedRef",
    "AmbiguousVersion",
    "AuthorityViolation",
    "UnexpressibleSurface",
    "LinkError",
    "AuthorityWideningRequiresHuman",
    "InsufficientBudget",
    "UnbudgetedArm",
    "UnattendedRequiresInput",
    "Refused",
    "Unsupported",
    "UnknownEffect",
    "AlreadyDecided",
    "OptionNotOffered",
    "UnknownPermission",
    "Fenced",
    "EnvironmentUnavailable",
    "UnknownCapability",
    "Overloaded",
    "Disconnected",
    "Timeout",
];
