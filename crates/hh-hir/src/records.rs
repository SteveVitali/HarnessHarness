//! The §3.1.3 **semantic product types** — the thirteen entity records, the per-edge field
//! records, `VersionRecord`, `SurfaceRecord`, and `OpaqueProcess`. Every record is a fixed
//! product type (closed sums only); unknown members are [`HirError::SchemaViolation`] on
//! parse, unknown kind names are [`HirError::UnknownKind`].
//!
//! Layout invariant (§3.1.2/§3.1.11): `content` is `Text`-leaved where prose is legal
//! (instructions, `purpose`, rubrics, memory content); everything else is a structured
//! field. Model identity appears **only** as `ProfileRef` (T-LCD-01) or inside a `surface`
//! record.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_ontology::control::ControlBoundary;
/// The canonical `/1` debt vocabulary — re-exported so debt-bearing record
/// owners and fixtures share the one schema source (CC7).
pub use hh_ontology::debt::{
    DebtClass, DebtExpiry, DebtPolicy, DebtRef, DebtScope, DebtStatus, DeficiencyClass,
    EvidenceGrade, EvidenceKind, EvidenceRef, ExpiryCondition, ExpiryKind, HypothesisTyped,
    ModelSelector, OwnerRef, RemovalTest, RemovalTestKind, RemovalVerdict, Revalidation,
    UnexecutableReason,
};
use hh_provenance::{AuthorityClass, PersistenceScope, ProvenanceRecord, TaintTag};
use hh_wire::json::Json;

use hh_ontology::participant::HostingMechanism;

use crate::kinds::{
    EdgeKind, EffectClass, EntityKind, PreconditionDomain, ToolEffects, ValidatorKind,
};
use crate::leaves::{CompiledPayload, Text};
use crate::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref, RunRef};

// ─────────────────────────────────────────────────────────────────────────────
// Shared small records
// ─────────────────────────────────────────────────────────────────────────────

/// `Validity{from, until | condition}` — the closed validity record (§3.1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validity {
    /// Valid from (logical time).
    pub from: u64,
    /// Valid until — exclusive-or `condition`.
    pub until: Option<u64>,
    /// A validity condition — exclusive-or `until`.
    pub condition: Option<String>,
}

impl Validity {
    /// An open validity from `from`.
    pub fn open_from(from: u64) -> Validity {
        Validity {
            from,
            until: None,
            condition: None,
        }
    }

    /// Well-formed: `until` and `condition` are exclusive.
    pub fn well_formed(&self) -> bool {
        !(self.until.is_some() && self.condition.is_some())
    }

    /// Does `self` cover `other` (a narrower window)?
    pub fn covers(&self, other: &Validity) -> bool {
        let self_end_ok = match (&self.until, &other.until) {
            (None, _) => true,
            (Some(s), Some(o)) => s >= o,
            (Some(_), None) => false,
        };
        self.from <= other.from
            && self_end_ok
            && (self.condition.is_none() || self.condition == other.condition)
    }
}

/// `constraints{budget, time, count}` on a grant (§3.1.3).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrantConstraints {
    /// A budget bound (structured value).
    pub budget: Option<Json>,
    /// A time bound.
    pub time: Option<u64>,
    /// A call-count bound.
    pub count: Option<u64>,
}

/// `grant{effect{domain, attributes?}, scope: ResourcePattern, constraints{...}, delegable}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    /// The granted effect (attributes optional — absent = whole domain).
    pub effect: EffectClass,
    /// The resource pattern the grant scopes to.
    pub scope: String,
    /// The constraints.
    pub constraints: GrantConstraints,
    /// Whether the grant may be delegated onward.
    pub delegable: bool,
}

/// `issuer{authority, reference}` (§3.1.3): the Permission's issuer. `reference` is an
/// identity coordinate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issuer {
    /// The issuer's authority — **not `delegate`** (a model claim cannot confer a
    /// permission; §3.1.4).
    pub authority: AuthorityClass,
    /// The issuer's identity coordinate.
    pub reference: String,
}

/// `scope_bindings | scope_bindings_unknown` (§3.1.3; ToolCapability).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeBindings {
    /// Known scope bindings (a structured record).
    Bindings(Json),
    /// `scope_bindings_unknown` — declared unknown, never silent.
    Unknown,
}

/// `resources{declared, keys}` tri-state (§3.1.3; modeled on ADR-0047 `DeclaredResources`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resources {
    /// The resource keys are declared.
    Declared(BTreeSet<String>),
    /// Declared to need no resources.
    NoneDeclared,
    /// Not declared — honest unknown (never silently `NoneDeclared`).
    Unknown,
}

// ─────────────────────────────────────────────────────────────────────────────
// The thirteen entity semantic records
// ─────────────────────────────────────────────────────────────────────────────

/// `Goal = {statement: Text, success_criteria: [Ref<Validator|Procedure>],
/// unverifiable_reason: Text?, budget: Ref<Budget>, origin, parent}` (§3.1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalRecord {
    /// The goal statement (prose — a `Text` leaf).
    pub statement: Text,
    /// The success criteria — non-empty unless `unverifiable_reason` is present (§3.1.4).
    pub success_criteria: Vec<Ref>,
    /// Why the goal is unverifiable, when it is.
    pub unverifiable_reason: Option<Text>,
    /// The goal's budget.
    pub budget: Ref,
    /// The goal origin.
    pub origin: GoalOrigin,
    /// The parent goal.
    pub parent: Option<Ref>,
}

/// `Goal.origin ∈ {human, system, delegated, scheduled}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalOrigin {
    /// Set by a human.
    Human,
    /// Set by the system/kernel.
    System,
    /// Delegated from a parent.
    Delegated,
    /// Scheduled.
    Scheduled,
}

impl GoalOrigin {
    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            GoalOrigin::Human => "human",
            GoalOrigin::System => "system",
            GoalOrigin::Delegated => "delegated",
            GoalOrigin::Scheduled => "scheduled",
        }
    }
}

/// `Observation = {source, producer, content, authority, taint, risk_assessment?}` (§3.1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationRecord {
    /// The observation source.
    pub source: ObservationSource,
    /// The producing capability identity (`ToolCapability` semantic id).
    pub producer: String,
    /// The content union.
    pub content: ObservationContent,
    /// The claimed authority — `≤ authority(source)` (§3.1.4).
    pub authority: AuthorityClass,
    /// Taint tags — a model result is `untrusted_output`-tagged (§8.1).
    pub taint: BTreeSet<TaintTag>,
    /// The risk assessment record (§3.5 owns the shape; §3.1 carries the slot).
    pub risk_assessment: Option<Json>,
}

/// `Observation.source`: the closed source sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationSource {
    /// A tool result.
    ToolResult,
    /// An environment observation.
    Environment,
    /// A human observation.
    Human,
    /// A model claim — capped at `delegate` and `untrusted_output`-tainted; can never alone
    /// satisfy a Validator (§3.1.4; §8.1 #4).
    ModelClaim,
    /// A validator-produced observation.
    Validator,
}

impl ObservationSource {
    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            ObservationSource::ToolResult => "tool_result",
            ObservationSource::Environment => "environment",
            ObservationSource::Human => "human",
            ObservationSource::ModelClaim => "model_claim",
            ObservationSource::Validator => "validator",
        }
    }

    /// `authority(source)` — the source's authority ceiling (§3.1.4).
    pub fn authority_ceiling(self) -> AuthorityClass {
        match self {
            ObservationSource::ToolResult => AuthorityClass::Environment,
            ObservationSource::Environment => AuthorityClass::Environment,
            ObservationSource::Human => AuthorityClass::Principal,
            ObservationSource::ModelClaim => AuthorityClass::Delegate,
            ObservationSource::Validator => AuthorityClass::Principal,
        }
    }
}

/// `Observation.content = Text | structured | Artifact`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationContent {
    /// Prose.
    Text(Box<Text>),
    /// A structured value.
    Structured(Json),
    /// An artifact reference.
    Artifact(Ref),
}

/// `ContextItem = {payload: Ref<Text|Memory|Observation|Artifact|Procedure|ToolCapability>,
/// authority, validity, placement_policy?, priority, delivery_id, activation_observable}`
/// (§3.1.3; §5d owns the last three fields' semantics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextItemRecord {
    /// The payload ref (permitted kinds enforced by `validate`).
    pub payload: Ref,
    /// The item's authority.
    pub authority: AuthorityClass,
    /// The validity window.
    pub validity: Validity,
    /// The placement policy (`Ref` to the §5d policy record).
    pub placement_policy: Option<Ref>,
    /// The priority.
    pub priority: i64,
    /// The delivery identity.
    pub delivery_id: String,
    /// The activation observable (structured).
    pub activation_observable: Json,
}

/// `Memory = {content, validity, authority, confidence, source_trajectories, scope}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRecord {
    /// The memory content (prose).
    pub content: Text,
    /// The validity window.
    pub validity: Validity,
    /// The claimed authority (extraction is bounded: `≤ external`; §3.1.4).
    pub authority: AuthorityClass,
    /// The confidence record (structured).
    pub confidence: Json,
    /// The trajectories the memory was extracted from.
    pub source_trajectories: Vec<RunRef>,
    /// The persistence scope.
    pub scope: PersistenceScope,
}

/// `Procedure` step kinds — the closed sum `Instruction | Invoke | Delegate | Branch |
/// Loop | Verify | Opaque` (§3.1.3; procedural control is finite and closed — ADR-0017).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcedureStep {
    /// `Instruction` — a prose step (a `Text` leaf — T-LCD-02).
    Instruction(Text),
    /// `Invoke` — a tool call `{tool, args}` (`args` = `ArgBinding` — §5b owns binding
    /// grammar; carried as a structured record).
    Invoke {
        /// The tool ref.
        tool: Ref,
        /// The argument binding record.
        args: Json,
    },
    /// `Delegate` — `{spec: AgentProcessSpec, budget, permission}`.
    Delegate {
        /// The sub-process spec (§3.2 owns `AgentProcessSpec`; carried as a structured
        /// record here).
        spec: Json,
        /// The sub-budget (must be `≤` the enclosing budget).
        budget: Ref,
        /// The delegated permission (must be `⊆` the holder's; delegable).
        permission: Ref,
    },
    /// `Branch` — `{condition, then_body, else_body}`.
    Branch {
        /// The branch predicate (structured).
        condition: Json,
        /// The then-body.
        then_body: Vec<ProcedureStep>,
        /// The else-body.
        else_body: Vec<ProcedureStep>,
    },
    /// `Loop` — `{bound: Ref<Budget>, body}` — bounded by construction (§3.1.4).
    Loop {
        /// The bound budget.
        bound: Ref,
        /// The loop body.
        body: Vec<ProcedureStep>,
    },
    /// `Verify` — `{validator}`.
    Verify {
        /// The validator ref.
        validator: Ref,
    },
    /// `Opaque` — a `CompiledPayload` step (opacity legal only as the body of a declared
    /// interface — `OpaqueWithoutInterface` otherwise).
    Opaque(CompiledPayload),
}

/// `Procedure = {preconditions, steps, expected_evidence, allowed_capabilities[],
/// failure_handlers, effects_derived}` (§3.1.3). `effects_derived` is **computed** —
/// [`ProcedureRecord::derive_effects`]; it is not an input field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureRecord {
    /// Preconditions (structured).
    pub preconditions: Json,
    /// The step sequence (the closed sum above).
    pub steps: Vec<ProcedureStep>,
    /// The evidence the procedure expects to leave (structured).
    pub expected_evidence: Json,
    /// The capabilities the procedure may invoke (`Ref`s to `ToolCapability` nodes).
    pub allowed_capabilities: Vec<Ref>,
    /// Failure handlers (structured — §3.2 owns handler grammar).
    pub failure_handlers: Json,
}

/// `ToolCapability = {purpose: Text, input_schema, output_schema?, effects:
/// pure | declared{...}, preconditions: [PreconditionDomain], scope_bindings |
/// scope_bindings_unknown, resources{declared, keys} (tri-state), observation_contract,
/// cost_model?, execution_requirement, source, exposure_hint, postconditions: Ref[],
/// flow_contract?}` (§3.1.3).
///
/// ADR-0054 shape records owned by §5b (`observation_contract`, `cost_model`,
/// `execution_requirement`, `source`, `exposure_hint`, `flow_contract`) are present in the
/// schema as structured slots — the §3.1 schema fixes *which* fields exist; the §5b/§3.3
/// sections own their shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCapabilityRecord {
    /// The tool's purpose (prose — `Text`; §3.1.11 forbids ≥principal authority).
    pub purpose: Text,
    /// The input schema (structured).
    pub input_schema: Json,
    /// The output schema, when declared.
    pub output_schema: Option<Json>,
    /// The effects: `pure | declared{set<EffectClass>}` (closed only).
    pub effects: ToolEffects,
    /// The precondition domains (closed sum).
    pub preconditions: Vec<PreconditionDomain>,
    /// `scope_bindings | scope_bindings_unknown`.
    pub scope_bindings: ScopeBindings,
    /// `resources{declared, keys}` tri-state.
    pub resources: Resources,
    /// The observation contract (§5b-owned shape).
    pub observation_contract: Json,
    /// The cost model, when declared (§5b-owned shape).
    pub cost_model: Option<Json>,
    /// The execution requirement (§5b-owned shape).
    pub execution_requirement: Json,
    /// The tool's provenance source record (§3.3-owned shape).
    pub source: Json,
    /// The exposure hint (§5b-owned shape).
    pub exposure_hint: Json,
    /// Postcondition refs (to `Validator`/`Observation` nodes).
    pub postconditions: Vec<Ref>,
    /// The flow contract, when declared (§5b-owned shape).
    pub flow_contract: Option<Json>,
}

/// `Permission = {holder: Ref<AgentProcess>, grants[], issuer{authority, reference},
/// validity, revocation?}` (§3.1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRecord {
    /// The holder.
    pub holder: Ref,
    /// The grants.
    pub grants: Vec<Grant>,
    /// The issuer.
    pub issuer: Issuer,
    /// The validity window.
    pub validity: Validity,
    /// The revocation record, when present.
    pub revocation: Option<Json>,
}

/// `Effect = {run_id, turn_id, model_call_id, tool_call_id (a nested identity chain —
/// §3.1.10), declared: EffectClass, effective_risk_class?}` (§3.1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectRecord {
    /// The run identity coordinate.
    pub run_id: String,
    /// The turn identity coordinate.
    pub turn_id: String,
    /// The model-call identity coordinate.
    pub model_call_id: String,
    /// The tool-call identity coordinate.
    pub tool_call_id: String,
    /// The declared effect class.
    pub declared: EffectClass,
    /// The derived effective risk class (computed at §3.5; never input).
    pub effective_risk_class: Option<String>,
    /// The handle ids this effect produced (§8.1 #6 — a handle is an identity coordinate).
    pub handle_ids: Vec<String>,
}

/// `Artifact = {content_hash, media_type, size, storage_ref, produced_by}` (§3.1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRecord {
    /// The content hash.
    pub content_hash: String,
    /// The media type.
    pub media_type: String,
    /// The size in bytes.
    pub size: u64,
    /// The storage reference.
    pub storage_ref: String,
    /// What produced it.
    pub produced_by: Ref,
}

/// `Validator = {kind: ValidatorKind, inputs[], verdict_type, deterministic, cost,
/// evidence_out}` (§3.1.3). A `judge` additionally carries an assumption-debt record
/// (`judge ⇒ deterministic = false ∧ profile_ref ∧ assumption_debt`; §3.1.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatorRecord {
    /// The validator kind.
    pub kind: ValidatorKind,
    /// The inputs the validator reads (refs).
    pub inputs: Vec<Ref>,
    /// The verdict type tag.
    pub verdict_type: String,
    /// Whether the validator is deterministic (a judge is never deterministic).
    pub deterministic: bool,
    /// The cost record (structured).
    pub cost: Json,
    /// The evidence the verdict leaves (structured).
    pub evidence_out: Json,
    /// The assumption debt a `judge` must carry (T-LCD-05).
    pub assumption_debt: Option<AssumptionDebtRecord>,
}

/// `AgentProcess = {body: AgentProcessBody}` (§3.1.3). The `kind` discriminator
/// (`native | hosted`) is part of the body's closed sum — `AgentProcessKind` in the §3.1.4
/// vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProcessRecord {
    /// The process body.
    pub body: AgentProcessBody,
}

/// `AgentProcessBody = native{...} | hosted{OpaqueProcess}` (§3.1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProcessBody {
    /// A native process — every component first-class in the graph.
    Native(NativeProcess),
    /// A hosted (opaque) process — the Stage-1 boundary case (OQ-072).
    Hosted(OpaqueProcess),
}

/// `native{harness_def, profile: ProfileRef, slots, control_boundary, budget, permissions,
/// environment}` (§3.1.3). `slots` **must contain** `control_strategy` and `context_policy`
/// entries (§3.1.4 — they name the first-class authority-bearing surfaces; ADR-0013).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeProcess {
    /// The enclosing harness definition reference.
    pub harness_def: Ref,
    /// The model profile — the only model-typed field, a `ProfileRef` (T-LCD-01).
    pub profile: ProfileRef,
    /// The slot bindings: slot name → bound variants.
    pub slots: BTreeMap<String, SlotBindings>,
    /// The control boundary (the CC11 shared record — `hh_ontology` owns the type).
    pub control_boundary: ControlBoundary,
    /// The process budget.
    pub budget: Ref,
    /// The process permissions.
    pub permissions: Ref,
    /// The environment.
    pub environment: EnvironmentRef,
}

/// `slots: Map<SlotName, SlotBindings>`; `SlotBindings = one(SlotBinding) | many(SlotBinding[])`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotBindings {
    /// One bound variant.
    One(SlotBinding),
    /// Many bound variants.
    Many(Vec<SlotBinding>),
}

/// `SlotBinding{variant: ComponentVariantRef, params: map<name, Value | $param:<id> |
/// $entity:<id>>, enabled = true, locality?}` (§3.3.2 — the §3.3 grammar row this node
/// carries). In a **sealed** form `variant` is pinned and every `params` value is a resolved
/// `Value` (no `$param`/`$entity` forms remain — §3.3.4 `resolve`); `enabled = false` is the
/// canonical whole-component ablation switch and a disabled binding still counts toward
/// identity (§3.3.2); `locality` is a hint — locality is a property of the variant
/// registration (`implementation.placement`), never of the class contract (Q-L1-10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotBinding {
    /// The bound component variant.
    pub variant: ComponentVariantRef,
    /// The parameter bindings for this binding (`name → Value` once resolved).
    pub params: BTreeMap<String, Json>,
    /// The ablation switch (`true` by default).
    pub enabled: bool,
    /// The locality hint, when authored.
    pub locality: Option<Locality>,
}

impl SlotBinding {
    /// A binding with no params, enabled, no locality hint.
    pub fn of(variant: ComponentVariantRef) -> SlotBinding {
        SlotBinding {
            variant,
            params: BTreeMap::new(),
            enabled: true,
            locality: None,
        }
    }
}

/// `SlotBinding.locality ∈ {in_process, out_of_process, inherit}` — a hint (§3.3.2;
/// ADR-0023 decision 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locality {
    /// Bind in the kernel process.
    InProcess,
    /// Bind through the variant host.
    OutOfProcess,
    /// Inherit the registration's placement.
    Inherit,
}

impl Locality {
    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            Locality::InProcess => "in_process",
            Locality::OutOfProcess => "out_of_process",
            Locality::Inherit => "inherit",
        }
    }

    /// Parse the closed sum.
    pub fn parse(s: &str) -> Option<Locality> {
        match s {
            "in_process" => Some(Locality::InProcess),
            "out_of_process" => Some(Locality::OutOfProcess),
            "inherit" => Some(Locality::Inherit),
            _ => None,
        }
    }
}

/// `hosted{OpaqueProcess}` — the opaque participant boundary (§3.1.6; ADR-0015 decision 4).
/// `OpaqueProcess` is an HIR record — the schema depends on it, the Hosting ABI never
/// appears (T-LCD-10; AC-IR-04).
///
/// `version_identity = H(declaration ∥ participant_version)` — a participant body change is
/// a new version, per the §3.1.2 participant rule.
///
/// `hosting_mechanism` is `hh_ontology`'s closed sum (`{none, session_abi,
/// model_boundary_intercept, container_installed}` — underscore form in records);
/// `none` is legal only on a native descriptor — `validate` fails a `hosted` body carrying
/// it (CF-351).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueProcess {
    /// The participant reference (identity coordinate of the participant declaration).
    pub participant_ref: String,
    /// The capability declaration record — the fifteen-field tri-state.
    pub declared_capabilities: CapabilityDeclarationRecord,
    /// The declared observability levels.
    pub observability_levels: BTreeSet<hh_ontology::participant::Observability>,
    /// The participant's own version coordinate (the declaration identity).
    pub participant_version: String,
    /// `H(declaration ∥ participant_version)` — set by `seal`; validated when present.
    pub version_identity: Option<String>,
    /// The declared hosting mechanism (`None` is refused on a hosted body — CF-351).
    pub hosting_mechanism: HostingMechanism,
    /// The supplies the harness provides (`context`, `tools`, `procedures` refs).
    pub supplies: Supplies,
    /// The budget.
    pub budget: Ref,
    /// The permissions.
    pub permissions: Ref,
    /// Participant params (structured).
    pub params: Option<Json>,
}

/// `Supplies` — the refs the harness hands to a hosted participant (`context`, `tools`,
/// `procedures`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Supplies {
    /// Context refs (`ContextItem` nodes).
    pub context: Vec<Ref>,
    /// Tool refs (`ToolCapability` nodes).
    pub tools: Vec<Ref>,
    /// Procedure refs (`Procedure` nodes).
    pub procedures: Vec<Ref>,
}

/// The tri-state a `CapabilityDeclarationRecord` field takes (`supported | unsupported |
/// unknown`). **Distinct** from the probe `CapabilityVerdict` (7-valued): the record is the
/// declaration; the vector is derived by probes (CF-020).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityState {
    /// Declared supported.
    Supported,
    /// Declared unsupported.
    Unsupported,
    /// Honestly unknown.
    Unknown,
}

impl CapabilityState {
    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            CapabilityState::Supported => "supported",
            CapabilityState::Unsupported => "unsupported",
            CapabilityState::Unknown => "unknown",
        }
    }
}

/// The `CapabilityDeclarationRecord` — the fifteen-field tri-state declaration
/// (§3.1.6; ADR-0015 decision 4). History is append-only (a ledger rule, not a field).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityDeclarationRecord {
    /// `streaming`
    pub streaming: CapabilityState,
    /// `interrupt`
    pub interrupt: CapabilityState,
    /// `steer`
    pub steer: CapabilityState,
    /// `live_queue`
    pub live_queue: CapabilityState,
    /// `resume`
    pub resume: CapabilityState,
    /// `fork`
    pub fork: CapabilityState,
    /// `compaction`
    pub compaction: CapabilityState,
    /// `images`
    pub images: CapabilityState,
    /// `subagents`
    pub subagents: CapabilityState,
    /// `permission_surface`
    pub permission_surface: CapabilityState,
    /// `instruction_delivery`
    pub instruction_delivery: CapabilityState,
    /// `model_family`
    pub model_family: CapabilityState,
    /// `effort_vocabulary`
    pub effort_vocabulary: CapabilityState,
    /// `trajectory_export`
    pub trajectory_export: CapabilityState,
    /// `native_config`
    pub native_config: CapabilityState,
}

impl CapabilityDeclarationRecord {
    /// An all-unknown declaration (the honest empty claim).
    pub fn all_unknown() -> Self {
        let u = CapabilityState::Unknown;
        CapabilityDeclarationRecord {
            streaming: u,
            interrupt: u,
            steer: u,
            live_queue: u,
            resume: u,
            fork: u,
            compaction: u,
            images: u,
            subagents: u,
            permission_surface: u,
            instruction_delivery: u,
            model_family: u,
            effort_vocabulary: u,
            trajectory_export: u,
            native_config: u,
        }
    }
}

/// `Budget = {dimensions, scope, parent, accounting}` (§3.1.3; ADR-0039). Dimension keys
/// are the kernel counter/gauge list plus the derived bound names — the closed registry
/// landed at S1.6 (`hh_ontology::dimensions`; DF-S1.4-1 closed there); `validate`
/// refuses unknown keys (`SchemaViolation`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetRecord {
    /// `dimension → {hard, soft}` bounds.
    pub dimensions: BTreeMap<String, DimensionBound>,
    /// The scope pattern this budget covers.
    pub scope: String,
    /// The parent budget (containment: every dimension `≤` parent's; §3.1.4).
    pub parent: Option<Ref>,
    /// The accounting mode ref.
    pub accounting: Ref,
}

/// A dimension bound `{hard, soft}`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DimensionBound {
    /// The hard bound.
    pub hard: Option<u64>,
    /// The soft bound.
    pub soft: Option<u64>,
}

impl DimensionBound {
    /// `self ≤ other` — every bound non-increasing.
    pub fn within(&self, other: &DimensionBound) -> bool {
        let le = |a: Option<u64>, b: Option<u64>| match (a, b) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(x), Some(y)) => x <= y,
        };
        le(self.hard, other.hard) && le(self.soft, other.soft)
    }
}

/// `HarnessRule = {trigger: Predicate, action: RuleAction, scope, conditioned_on:
/// ProfileRef?, assumption_debt?}` (§1.5/§3.1.3). `conditioned_on ≠ null` requires a
/// complete assumption-debt record — `ConditionedRuleIncomplete` otherwise (T-LCD-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessRuleRecord {
    /// The rule's identity (a semantic coordinate; stable across versions).
    pub rule_id: String,
    /// The trigger predicate (structured).
    pub trigger: Json,
    /// The action.
    pub action: RuleAction,
    /// The rule's scope (structured).
    pub scope: Json,
    /// The profile the rule is conditioned on (the only model-typed field — `ProfileRef`).
    pub conditioned_on: Option<ProfileRef>,
    /// The assumption-debt record — mandatory when `conditioned_on` is set.
    pub assumption_debt: Option<AssumptionDebtRecord>,
}

/// `RuleAction` — the closed action sum (§1.5/§3.1.3):
/// `insert_context_item | restrict_tool_set | set_compaction_policy |
/// set_retry_stop_policy | require_validator | request_approval | flow_policy |
/// declassify | sanitize`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleAction {
    /// `insert_context_item(ContextItem)` — insert a context item.
    InsertContextItem(Ref),
    /// `restrict_tool_set` — narrow the exposed tools.
    RestrictToolSet(Vec<Ref>),
    /// `set_compaction_policy` (structured policy record — §5d owns the shape).
    SetCompactionPolicy(Json),
    /// `set_retry_stop_policy` (structured).
    SetRetryStopPolicy(Json),
    /// `require_validator(Validator)`.
    RequireValidator(Ref),
    /// `request_approval` (structured request record).
    RequestApproval(Json),
    /// `flow_policy` — the taint-gated flow policy record (§8.1 owns the shape; carried as
    /// a structured record here).
    FlowPolicy(Json),
    /// `declassify` — the sole flow-mutation action; carries the readers the value is
    /// released to.
    Declassify(Vec<String>),
    /// `sanitize{sanitizer_ref, param}` — transform a tainted value's declared readers.
    Sanitize {
        /// The sanitizer's identity coordinate.
        sanitizer_ref: String,
        /// The sanitizer parameter (structured).
        param: Json,
    },
}

/// `AssumptionDebtRecord/1` — the complete record `{rule_id, hypothesis: Text,
/// evidence_refs[], owner, expiry_condition, removal_test_ref, status}` plus the
/// additive `/1` members (§5h.6 §3; R-2.9.6⁰ᵃ; ADR-0197/0198): `{debt_class?,
/// hypothesis_typed?, scope?, expiry?, runway_ms?, revalidation?, removal_test?,
/// created_by?, created_at?, supersedes?}`. It rides on every `conditioned_on`
/// rule and every `judge` validator (§3.1.4; T-LCD-05) and on every `DebtHomes/1`
/// row's field — CF-049: one schema, the vocabulary differs across homes.
///
/// Field-shape notes (the `/1` upgrade): `evidence_refs` is typed
/// (`EvidenceRef{kind, ref, observed_at?, tier?, provisional?}` — a bare string
/// decodes as `{kind: source, ref}` for landed bodies); `owner` is typed
/// (`principal{id}`/`team{id}` + `reach_via` — a bare string decodes as
/// `principal{id}`); `expiry_condition` is `{kind, value?}` (a bare spelling
/// decodes as `{kind}`); `status` is the canonical four-value sum — legacy
/// `open`/`discharged`/`violated` decode per [`DebtStatus::parse_legacy`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssumptionDebtRecord {
    /// The rule this debt belongs to (matches `HarnessRuleRecord.rule_id` / the validator's
    /// semantic id).
    pub rule_id: String,
    /// The hypothesis (prose — `Text`, provenance-bearing).
    pub hypothesis: Text,
    /// The evidence the hypothesis rests on — typed `EvidenceRef`s.
    pub evidence_refs: Vec<EvidenceRef>,
    /// The owner (`principal{id}` | `team{id}` + `reach_via` sinks).
    pub owner: OwnerRef,
    /// The expiry condition (`{kind, value?}`).
    pub expiry_condition: ExpiryCondition,
    /// The removal test reference (the template/design pointer).
    pub removal_test_ref: String,
    /// The debt status (`active`/`expiring`/`expired`/`retired`).
    pub status: DebtStatus,
    /// `/1`: the closed debt class (defaulted per home).
    pub debt_class: Option<DebtClass>,
    /// `/1`: the typed hypothesis `{subject, deficiency_class, predicted_effect}`.
    pub hypothesis_typed: Option<HypothesisTyped>,
    /// `/1`: the applicability scope `{model_selectors[], task_classes[], roles[]}`.
    pub scope: Option<DebtScope>,
    /// `/1`: the parameterized expiry `{condition, params{until?, …}}`.
    pub expiry: Option<DebtExpiry>,
    /// `/1`: the declared runway (ms).
    pub runway_ms: Option<u64>,
    /// `/1`: the revalidation policy `{on[], action}`.
    pub revalidation: Option<Revalidation>,
    /// `/1`: the typed removal test (instantiates per kind; validated by
    /// `hh_hir::debt::validate_removal_test`).
    pub removal_test: Option<RemovalTest>,
    /// `/1`: who created the record (provenance).
    pub created_by: Option<ProvenanceRecord>,
    /// `/1`: when the record was created (transaction time).
    pub created_at: Option<u64>,
    /// `/1`: the `RetirementRecord` this debt supersedes, when it does
    /// (`supersedes{reason: expiry}` chains).
    pub supersedes: Option<String>,
}

impl AssumptionDebtRecord {
    /// The derived `evidence_grade` (ADR-0197 — derived from `evidence_refs`,
    /// never stored).
    pub fn evidence_grade(&self) -> EvidenceGrade {
        hh_ontology::debt::evidence_grade(&self.evidence_refs)
    }

    /// Whether the field `name` is *present* on the record for
    /// `required_fields` completeness checks — a non-`Option` member counts
    /// present except `evidence_refs`/`scope.model_selectors` which count
    /// present only non-empty.
    pub fn has_field(&self, name: &str) -> bool {
        match name {
            "rule_id" => !self.rule_id.is_empty(),
            "hypothesis" => {
                !self.hypothesis.content_hash.is_empty() || self.hypothesis.content.is_some()
            }
            "evidence_refs" => !self.evidence_refs.is_empty(),
            "owner" => !self.owner.id.is_empty(),
            "expiry_condition" => true,
            "removal_test_ref" => !self.removal_test_ref.is_empty(),
            "status" => true,
            "debt_class" => self.debt_class.is_some(),
            "hypothesis_typed" => self.hypothesis_typed.is_some(),
            "scope" => self.scope.is_some(),
            "scope.model_selectors" => self
                .scope
                .as_ref()
                .map(|s| !s.model_selectors.is_empty())
                .unwrap_or(false),
            "scope.task_classes" => self
                .scope
                .as_ref()
                .map(|s| !s.task_classes.is_empty())
                .unwrap_or(false),
            "expiry" => self.expiry.is_some(),
            "runway_ms" => self.runway_ms.is_some(),
            "revalidation" => self.revalidation.is_some(),
            "removal_test" => self.removal_test.is_some(),
            "created_by" => self.created_by.is_some(),
            "created_at" => self.created_at.is_some(),
            "supersedes" => self.supersedes.is_some(),
            _ => false,
        }
    }
}

/// The semantic record union — the `semantic` member of a node is exactly one of the
/// thirteen product types (§3.1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KindRecord {
    /// `Goal`.
    Goal(GoalRecord),
    /// `Observation`.
    Observation(ObservationRecord),
    /// `ContextItem`.
    ContextItem(ContextItemRecord),
    /// `Memory`.
    Memory(MemoryRecord),
    /// `Procedure`.
    Procedure(ProcedureRecord),
    /// `ToolCapability`.
    ToolCapability(ToolCapabilityRecord),
    /// `Permission`.
    Permission(PermissionRecord),
    /// `Effect`.
    Effect(EffectRecord),
    /// `Artifact`.
    Artifact(ArtifactRecord),
    /// `Validator`.
    Validator(ValidatorRecord),
    /// `AgentProcess`.
    AgentProcess(AgentProcessRecord),
    /// `Budget`.
    Budget(BudgetRecord),
    /// `HarnessRule`.
    HarnessRule(HarnessRuleRecord),
}

impl KindRecord {
    /// The kind this record is for.
    pub fn kind(&self) -> EntityKind {
        match self {
            KindRecord::Goal(_) => EntityKind::Goal,
            KindRecord::Observation(_) => EntityKind::Observation,
            KindRecord::ContextItem(_) => EntityKind::ContextItem,
            KindRecord::Memory(_) => EntityKind::Memory,
            KindRecord::Procedure(_) => EntityKind::Procedure,
            KindRecord::ToolCapability(_) => EntityKind::ToolCapability,
            KindRecord::Permission(_) => EntityKind::Permission,
            KindRecord::Effect(_) => EntityKind::Effect,
            KindRecord::Artifact(_) => EntityKind::Artifact,
            KindRecord::Validator(_) => EntityKind::Validator,
            KindRecord::AgentProcess(_) => EntityKind::AgentProcess,
            KindRecord::Budget(_) => EntityKind::Budget,
            KindRecord::HarnessRule(_) => EntityKind::HarnessRule,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Surface records (§3.1.5 / §3.1.9)
// ─────────────────────────────────────────────────────────────────────────────

/// The `surface` record — the model-facing rendering of a node, excluded from
/// `semantic_id` (§3.1.2/§3.1.9). Surface fields exist only on `ToolCapability`,
/// `Validator`, `Procedure`, `ContextItem` (§3.1.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceRecord {
    /// `ToolCapability.surface` — the AC-IR-05 set: `{name, namespace,
    /// description_template: Text, argument_order, examples, error_format, result_renderer,
    /// strictness, schema_dialect_narrowing, exposure_mode}` plus the §3.1.9 display fields
    /// `{display_title, icon_ref}`.
    Tool(Box<ToolSurface>),
    /// `Validator.surface` — `{rubric_rendering, explain}`.
    Validator(ValidatorSurface),
    /// `Procedure.surface` — `{compile_hint}`.
    Procedure(ProcedureSurface),
    /// `ContextItem.surface` — `{rendering_template, position}`.
    ContextItem(ContextItemSurface),
}

/// `ToolCapability.surface` (§3.1.9): renamed `name`, reordered `argument_order`, and
/// reworded `description_template` are all **surface** — they never enter `semantic_id`
/// (AC-IR-05). `strictness` and `exposure_mode` are per-host knobs (OQ-069/070); a change
/// to `exposure_mode` or a narrowing that would weaken a conditioned rule is
/// profile-conditioned work (§3.1.5 caveat).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSurface {
    /// The tool's model-facing name.
    pub name: String,
    /// The model-facing namespace.
    pub namespace: String,
    /// The description template (a `Text` leaf).
    pub description_template: Text,
    /// The argument order.
    pub argument_order: Vec<String>,
    /// The model-facing examples (structured).
    pub examples: Json,
    /// The error format record (structured).
    pub error_format: Json,
    /// The result renderer (structured).
    pub result_renderer: Json,
    /// The strictness knobs (structured — per-host; OQ-069).
    pub strictness: Json,
    /// Schema dialect narrowing (structured).
    pub schema_dialect_narrowing: Json,
    /// The exposure mode (structured — per-host; OQ-070).
    pub exposure_mode: Json,
    /// The display title (§3.1.9; host-facing only — never `semantic_id`, never a ref).
    pub display_title: Option<String>,
    /// The icon reference (§3.1.9).
    pub icon_ref: Option<String>,
}

/// `Validator.surface` — `{rubric_rendering, explain}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatorSurface {
    /// The rubric rendering (structured).
    pub rubric_rendering: Json,
    /// The explain record (structured).
    pub explain: Json,
}

/// `Procedure.surface` — `{compile_hint}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureSurface {
    /// The compile hint.
    pub compile_hint: CompileHint,
}

/// `compile_hint` — a lowering hint over the procedure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileHint {
    /// Compile to instructions.
    Instruction,
    /// Compile to a workflow node.
    WorkflowNode,
    /// Compile to a subagent task.
    SubagentTask,
}

impl CompileHint {
    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            CompileHint::Instruction => "instruction",
            CompileHint::WorkflowNode => "workflow_node",
            CompileHint::SubagentTask => "subagent_task",
        }
    }
}

/// `ContextItem.surface` — `{rendering_template, position}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextItemSurface {
    /// The rendering template (structured).
    pub rendering_template: Json,
    /// The position record (structured).
    pub position: Json,
}

// ─────────────────────────────────────────────────────────────────────────────
// Version record + edge records
// ─────────────────────────────────────────────────────────────────────────────

/// The version record on every node and edge (§3.1.1/§3.1.3): `version_id` and
/// `semantic_id` are computed by `identity`/`seal` (never input); `supersedes` and `sealed`
/// are the ledger facts. `dialect` is the kind's schema dialect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRecord {
    /// The `version_id` — `H(canonical(member))` under `idp/1`, set by `identity`/`seal`.
    pub version_id: Option<String>,
    /// The `semantic_id` — `H({kind ∥ semantic ∥ refs-by-semantic_id})`, set by
    /// `identity`/`seal`.
    pub semantic_id: Option<String>,
    /// The schema dialect this member is written against (`HIR/1`).
    pub dialect: String,
    /// The `version_id` this member supersedes, when it does.
    pub supersedes: Option<String>,
    /// Whether the member is part of a sealed definition.
    pub sealed: bool,
}

impl VersionRecord {
    /// A fresh version record (unsealed, `HIR/1`).
    pub fn fresh() -> VersionRecord {
        VersionRecord {
            version_id: None,
            semantic_id: None,
            dialect: "HIR/1".into(),
            supersedes: None,
            sealed: false,
        }
    }
}

/// The per-edge field records (§3.1.3). An edge is `{kind, from, to, provenance, fields}`;
/// `fields` is this closed sum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdgeRecord {
    /// `depends-on{reason}`.
    DependsOn {
        /// Why the dependency exists.
        reason: String,
    },
    /// `supersedes{reason}` — `reason ∈ {edit, revocation, expiry, migration,
    /// consolidation, fork}`; `reason = fork` is admittance-checked (`NotAncestorOrSibling`
    /// vocabulary — the admittance mechanic is §3.3).
    Supersedes {
        /// The supersession reason.
        reason: SupersedeReason,
    },
    /// `authorizes{scope}` — the `scope` ResourcePattern, plus the `effect_class` the
    /// authorization covers when the target is an `Effect` node (an `authorizes` target may
    /// be `ToolCapability | EffectClass | Procedure`; a class target is recorded on the edge
    /// since `EffectClass` is a value, not a node).
    Authorizes {
        /// The authorization scope pattern.
        scope: String,
        /// The authorized effect class, when the target isn't itself a capability.
        effect_class: Option<EffectClass>,
    },
    /// `produced-by` — no fields (every runtime record has one; §8.1).
    ProducedBy,
    /// `validates` — no fields (per-edge binding; OQ-073).
    Validates,
    /// `delegated-to{permission, budget}` — the attenuated grant + budget refs.
    DelegatedTo {
        /// The delegated permission.
        permission: Ref,
        /// The delegated budget.
        budget: Ref,
    },
    /// `derived-from{hypothesis: Text, trajectories: [RunRef], candidate_id?}` — the edge's
    /// own record *is* the derivation; `to` is the `from` node itself (the record binds to
    /// the new version — §3.1.3).
    DerivedFrom {
        /// The hypothesis (prose).
        hypothesis: Box<Text>,
        /// The source trajectories.
        trajectories: Vec<RunRef>,
        /// The candidate id, when the derivation came through evolution.
        candidate_id: Option<String>,
    },
}

impl EdgeRecord {
    /// The edge kind this record is for.
    pub fn kind(&self) -> EdgeKind {
        match self {
            EdgeRecord::DependsOn { .. } => EdgeKind::DependsOn,
            EdgeRecord::Supersedes { .. } => EdgeKind::Supersedes,
            EdgeRecord::Authorizes { .. } => EdgeKind::Authorizes,
            EdgeRecord::ProducedBy => EdgeKind::ProducedBy,
            EdgeRecord::Validates => EdgeKind::Validates,
            EdgeRecord::DelegatedTo { .. } => EdgeKind::DelegatedTo,
            EdgeRecord::DerivedFrom { .. } => EdgeKind::DerivedFrom,
        }
    }
}

/// `SupersedesReason ∈ {edit, revocation, expiry, migration, consolidation, fork}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupersedeReason {
    /// An edit.
    Edit,
    /// A revocation.
    Revocation,
    /// An expiry.
    Expiry,
    /// A migration.
    Migration,
    /// A consolidation.
    Consolidation,
    /// A fork — admittance-checked (`NotAncestorOrSibling`; §3.3 owns admittance).
    Fork,
}

impl SupersedeReason {
    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            SupersedeReason::Edit => "edit",
            SupersedeReason::Revocation => "revocation",
            SupersedeReason::Expiry => "expiry",
            SupersedeReason::Migration => "migration",
            SupersedeReason::Consolidation => "consolidation",
            SupersedeReason::Fork => "fork",
        }
    }
}
