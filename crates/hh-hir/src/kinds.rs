//! The closed per-dialect kind sums (§3.1.2; AC-IR-01): **thirteen `EntityKind`s**, the seven
//! `EdgeKind`s, `since = HIR/1` on every kind, and the frozen `EffectDomain` /
//! `EffectAttributes` sums. `Text` and `CompiledPayload` are the two opaque *leaf* kinds —
//! they appear in the §3.1.2 roster and the `hh-ontology` classification table but are not
//! DAG nodes; they are embedded records ([`crate::leaves`]).
//!
//! Per-kind `classify_home` values live in `hh_ontology::IrEntityKind` (CC10 — the ontology
//! layer owns the home-plane classification; DF-S1.1-1).

use std::fmt;

use hh_ontology::planes::Plane;
use hh_wire::json::Json;

use crate::errors::HirError;

/// The thirteen `EntityKind`s (§3.1.2; closed for HIR/1 — `UnknownKind` otherwise). Each
/// entity is an ontology kind with `since = HIR/1` (CC8): `until = ∅` today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityKind {
    /// §3.1.3 — a goal.
    Goal,
    /// §3.1.3 — an observation.
    Observation,
    /// §3.1.3 — a context item.
    ContextItem,
    /// §3.1.3 — a memory record.
    Memory,
    /// §3.1.3 — a procedure.
    Procedure,
    /// §3.1.3 — a tool capability.
    ToolCapability,
    /// §3.1.3 — a permission.
    Permission,
    /// §3.1.3 — an effect record.
    Effect,
    /// §3.1.3 — an artifact.
    Artifact,
    /// §3.1.3 — a validator.
    Validator,
    /// §3.1.3 — an agent process.
    AgentProcess,
    /// §3.1.3 — a budget.
    Budget,
    /// §3.1.3 — a harness rule.
    HarnessRule,
}

/// The thirteen `EntityKind`s in roster order (§3.1.2).
pub const ENTITY_KINDS: [EntityKind; 13] = [
    EntityKind::Goal,
    EntityKind::Observation,
    EntityKind::ContextItem,
    EntityKind::Memory,
    EntityKind::Procedure,
    EntityKind::ToolCapability,
    EntityKind::Permission,
    EntityKind::Effect,
    EntityKind::Artifact,
    EntityKind::Validator,
    EntityKind::AgentProcess,
    EntityKind::Budget,
    EntityKind::HarnessRule,
];

/// The two opaque leaf kinds (§3.1.2) — embedded records, not DAG nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LeafKind {
    /// Model-facing prose (instructions, `purpose`, rubrics, memory content).
    Text,
    /// Executable bodies (variant implementations, `Opaque` steps, validators, grammars).
    CompiledPayload,
}

/// The two leaf kinds in roster order.
pub const LEAF_KINDS: [LeafKind; 2] = [LeafKind::Text, LeafKind::CompiledPayload];

impl LeafKind {
    /// The canonical name.
    pub fn name(self) -> &'static str {
        match self {
            LeafKind::Text => "Text",
            LeafKind::CompiledPayload => "CompiledPayload",
        }
    }
}

impl EntityKind {
    /// The canonical name (the `kind` member of a node's canonical form).
    pub fn name(self) -> &'static str {
        match self {
            EntityKind::Goal => "Goal",
            EntityKind::Observation => "Observation",
            EntityKind::ContextItem => "ContextItem",
            EntityKind::Memory => "Memory",
            EntityKind::Procedure => "Procedure",
            EntityKind::ToolCapability => "ToolCapability",
            EntityKind::Permission => "Permission",
            EntityKind::Effect => "Effect",
            EntityKind::Artifact => "Artifact",
            EntityKind::Validator => "Validator",
            EntityKind::AgentProcess => "AgentProcess",
            EntityKind::Budget => "Budget",
            EntityKind::HarnessRule => "HarnessRule",
        }
    }

    /// Parse a kind name; anything not in the closed sum is [`HirError::UnknownKind`]
    /// (§3.1.4 — fail closed on unknown kinds).
    pub fn parse(name: &str) -> Result<EntityKind, HirError> {
        ENTITY_KINDS
            .iter()
            .copied()
            .find(|k| k.name() == name)
            .ok_or_else(|| HirError::UnknownKind {
                kind: name.to_string(),
            })
    }

    /// `since` — every HIR/1 kind was introduced in this dialect (AC-IR-01).
    pub fn since(self) -> &'static str {
        "HIR/1"
    }

    /// `until` — no kind is deprecated (CC8 `until` support is the same proven mechanism as
    /// `hh_ontology::OntologyKind`).
    pub fn until(self) -> Option<&'static str> {
        None
    }

    /// The `hh-ontology` kind carrying this entity's per-kind `classify_home` (CC10; the
    /// thirteen-row table of §3.1.3 lives there — DF-S1.1-1).
    pub fn ir_kind(self) -> hh_ontology::IrEntityKind {
        use hh_ontology::IrEntityKind as I;
        match self {
            EntityKind::Goal => I::Goal,
            EntityKind::Observation => I::Observation,
            EntityKind::ContextItem => I::ContextItem,
            EntityKind::Memory => I::Memory,
            EntityKind::Procedure => I::Procedure,
            EntityKind::ToolCapability => I::ToolCapability,
            EntityKind::Permission => I::Permission,
            EntityKind::Effect => I::Effect,
            EntityKind::Artifact => I::Artifact,
            EntityKind::Validator => I::Validator,
            EntityKind::AgentProcess => I::AgentProcess,
            EntityKind::Budget => I::Budget,
            EntityKind::HarnessRule => I::HarnessRule,
        }
    }

    /// The home plane — the `classify_home` table of §3.1.3, owned by `hh-ontology`.
    /// Every IR entity/leaf homes to a plane (the table is total over `Plane` members).
    pub fn home(self) -> Plane {
        match self.ir_kind().home() {
            hh_ontology::planes::Home::Plane(p) => p,
            other => unreachable!("IR entity {} homed at {:?}", self.name(), other),
        }
    }

    /// Whether this kind carries a `surface` record at all (§3.1.5: `ToolCapability`
    /// §3.1.9, `Validator`, `Procedure`, `ContextItem`; other kinds have no surface fields).
    pub fn has_surface(self) -> bool {
        matches!(
            self,
            EntityKind::ToolCapability
                | EntityKind::Validator
                | EntityKind::Procedure
                | EntityKind::ContextItem
        )
    }
}

/// The seven `EdgeKind`s (§3.1.3; closed for HIR/1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EdgeKind {
    /// `depends-on` — semantic dependency on a component (`reason`).
    DependsOn,
    /// `supersedes` — the revocation/evolution ledger (`reason` + admittance).
    Supersedes,
    /// `authorizes` — a Permission grants an effect/capability (`scope`).
    Authorizes,
    /// `produced-by` — every runtime record has one (§8.1).
    ProducedBy,
    /// `validates` — a Validator binds to what it checks (OQ-073).
    Validates,
    /// `delegated-to` — parent → child `AgentProcess` (attenuation: §3.1.4).
    DelegatedTo,
    /// `derived-from` — a new node version's derivation record.
    DerivedFrom,
}

/// The seven edge kinds in roster order.
pub const EDGE_KINDS: [EdgeKind; 7] = [
    EdgeKind::DependsOn,
    EdgeKind::Supersedes,
    EdgeKind::Authorizes,
    EdgeKind::ProducedBy,
    EdgeKind::Validates,
    EdgeKind::DelegatedTo,
    EdgeKind::DerivedFrom,
];

impl EdgeKind {
    /// The canonical edge name.
    pub fn name(self) -> &'static str {
        match self {
            EdgeKind::DependsOn => "depends-on",
            EdgeKind::Supersedes => "supersedes",
            EdgeKind::Authorizes => "authorizes",
            EdgeKind::ProducedBy => "produced-by",
            EdgeKind::Validates => "validates",
            EdgeKind::DelegatedTo => "delegated-to",
            EdgeKind::DerivedFrom => "derived-from",
        }
    }

    /// Parse an edge kind; unknown → [`HirError::UnknownKind`].
    pub fn parse(name: &str) -> Result<EdgeKind, HirError> {
        EDGE_KINDS
            .iter()
            .copied()
            .find(|k| k.name() == name)
            .ok_or_else(|| HirError::UnknownKind {
                kind: name.to_string(),
            })
    }

    /// `since = HIR/1` on every edge kind.
    pub fn since(self) -> &'static str {
        "HIR/1"
    }

    /// Whether this edge participates in the acyclicity check (§3.1.4:
    /// supersedes/delegated-to/depends-on acyclic).
    pub fn is_acyclic(self) -> bool {
        matches!(
            self,
            EdgeKind::DependsOn | EdgeKind::Supersedes | EdgeKind::DelegatedTo
        )
    }
}

/// The frozen `EffectDomain` set (§3.1.2; ADR-0217 closed it):
/// `fs_read, fs_write, exec, net_egress, secret_access, spend, message_human,
/// spawn_process, memory_write, permission_request, model_call`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EffectDomain {
    /// Read the filesystem.
    FsRead,
    /// Write the filesystem.
    FsWrite,
    /// Spawn/execute code.
    Exec,
    /// Network egress.
    NetEgress,
    /// Read secrets/credentials.
    SecretAccess,
    /// Monetary spend.
    Spend,
    /// Message the human.
    MessageHuman,
    /// Spawn a process.
    SpawnProcess,
    /// Write memory.
    MemoryWrite,
    /// Request a permission.
    PermissionRequest,
    /// Call the model.
    ModelCall,
}

/// The eleven frozen effect domains.
pub const EFFECT_DOMAINS: [EffectDomain; 11] = [
    EffectDomain::FsRead,
    EffectDomain::FsWrite,
    EffectDomain::Exec,
    EffectDomain::NetEgress,
    EffectDomain::SecretAccess,
    EffectDomain::Spend,
    EffectDomain::MessageHuman,
    EffectDomain::SpawnProcess,
    EffectDomain::MemoryWrite,
    EffectDomain::PermissionRequest,
    EffectDomain::ModelCall,
];

impl EffectDomain {
    /// The canonical domain name.
    pub fn name(self) -> &'static str {
        match self {
            EffectDomain::FsRead => "fs_read",
            EffectDomain::FsWrite => "fs_write",
            EffectDomain::Exec => "exec",
            EffectDomain::NetEgress => "net_egress",
            EffectDomain::SecretAccess => "secret_access",
            EffectDomain::Spend => "spend",
            EffectDomain::MessageHuman => "message_human",
            EffectDomain::SpawnProcess => "spawn_process",
            EffectDomain::MemoryWrite => "memory_write",
            EffectDomain::PermissionRequest => "permission_request",
            EffectDomain::ModelCall => "model_call",
        }
    }

    /// Parse a domain name; unknown → [`HirError::UnknownKind`].
    pub fn parse(name: &str) -> Result<EffectDomain, HirError> {
        EFFECT_DOMAINS
            .iter()
            .copied()
            .find(|d| d.name() == name)
            .ok_or_else(|| HirError::UnknownKind {
                kind: format!("effect domain {name}"),
            })
    }
}

/// The `EffectAttributes` closed sums (§3.1.3): each axis is a closed enum, growth by dialect
/// bump only (CC8; ADR-0144). Not `Copy` — `reversibility` can carry a `Ref`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectAttributes {
    /// `mutability ∈ {read_only, additive, destructive}`.
    pub mutability: Mutability,
    /// `repeat_safety ∈ {idempotent, non_idempotent}`.
    pub repeat_safety: RepeatSafety,
    /// `world ∈ {closed, open}`.
    pub world: World,
    /// `reversibility ∈ {reversible(Ref<Procedure>), compensable, irreversible}`.
    pub reversibility: Reversibility,
}

impl Default for EffectAttributes {
    /// The least-dangerous default (used where a grant omits attributes).
    fn default() -> Self {
        EffectAttributes {
            mutability: Mutability::ReadOnly,
            repeat_safety: RepeatSafety::Idempotent,
            world: World::Closed,
            reversibility: Reversibility::Compensable,
        }
    }
}

/// `mutability ∈ {read_only, additive, destructive}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mutability {
    /// Reads only.
    ReadOnly,
    /// Adds state without destroying.
    Additive,
    /// Destroys/overwrites.
    Destructive,
}

impl Mutability {
    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            Mutability::ReadOnly => "read_only",
            Mutability::Additive => "additive",
            Mutability::Destructive => "destructive",
        }
    }
    fn parse(s: &str) -> Result<Mutability, HirError> {
        match s {
            "read_only" => Ok(Mutability::ReadOnly),
            "additive" => Ok(Mutability::Additive),
            "destructive" => Ok(Mutability::Destructive),
            _ => Err(HirError::UnknownKind {
                kind: format!("mutability {s}"),
            }),
        }
    }
}

/// `repeat_safety ∈ {idempotent, non_idempotent}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RepeatSafety {
    /// Safe to repeat.
    Idempotent,
    /// Not safe to repeat.
    NonIdempotent,
}

impl RepeatSafety {
    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            RepeatSafety::Idempotent => "idempotent",
            RepeatSafety::NonIdempotent => "non_idempotent",
        }
    }
    fn parse(s: &str) -> Result<RepeatSafety, HirError> {
        match s {
            "idempotent" => Ok(RepeatSafety::Idempotent),
            "non_idempotent" => Ok(RepeatSafety::NonIdempotent),
            _ => Err(HirError::UnknownKind {
                kind: format!("repeat_safety {s}"),
            }),
        }
    }
}

/// `world ∈ {closed, open}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum World {
    /// The effect stays inside the declared sandbox.
    Closed,
    /// The effect may reach the open world.
    Open,
}

impl World {
    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            World::Closed => "closed",
            World::Open => "open",
        }
    }
    fn parse(s: &str) -> Result<World, HirError> {
        match s {
            "closed" => Ok(World::Closed),
            "open" => Ok(World::Open),
            _ => Err(HirError::UnknownKind {
                kind: format!("world {s}"),
            }),
        }
    }
}

/// `reversibility ∈ {reversible(Ref<Procedure>), compensable, irreversible}`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reversibility {
    /// Reversed by running this procedure.
    Reversible(crate::refs::Ref),
    /// Compensable but not reversible.
    Compensable,
    /// Irreversible.
    Irreversible,
}

impl Reversibility {
    fn to_json(&self) -> Json {
        match self {
            Reversibility::Reversible(r) => Json::obj([("reversible", r.to_json())]),
            Reversibility::Compensable => Json::str("compensable"),
            Reversibility::Irreversible => Json::str("irreversible"),
        }
    }
    fn semantic_json(&self) -> Json {
        match self {
            Reversibility::Reversible(r) => Json::obj([("reversible", r.semantic_json())]),
            other => other.to_json(),
        }
    }
    fn from_json(j: &Json, path: &str) -> Result<Reversibility, HirError> {
        match j {
            Json::Str(s) => match s.as_str() {
                "compensable" => Ok(Reversibility::Compensable),
                "irreversible" => Ok(Reversibility::Irreversible),
                _ => Err(HirError::UnknownKind {
                    kind: format!("reversibility {s}"),
                }),
            },
            Json::Obj(_) => j
                .get("reversible")
                .map(|r| {
                    crate::refs::Ref::from_json(r, &format!("{path}.reversible"))
                        .map(Reversibility::Reversible)
                })
                .unwrap_or_else(|| {
                    Err(HirError::SchemaViolation {
                        detail: format!("{path}: bad reversibility"),
                    })
                }),
            _ => Err(HirError::SchemaViolation {
                detail: format!("{path}: bad reversibility"),
            }),
        }
    }
}

/// `EffectClass = {domain, attributes}` — a domain with its attribute vector (§3.1.3). In a
/// grant the attributes may be omitted (`effect{domain, attributes?}`) — an attribute-free
/// grant covers the whole domain.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectClass {
    /// The domain.
    pub domain: EffectDomain,
    /// The attributes (`None` in a grant = whole domain).
    pub attributes: Option<EffectAttributes>,
}

impl EffectClass {
    /// A whole-domain class (grant form).
    pub fn domain_only(domain: EffectDomain) -> EffectClass {
        EffectClass {
            domain,
            attributes: None,
        }
    }

    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        match &self.attributes {
            Some(a) => Json::obj([
                ("domain", Json::str(self.domain.name())),
                ("attributes", attributes_json(a)),
            ]),
            None => Json::obj([("domain", Json::str(self.domain.name()))]),
        }
    }

    /// The semantic-projection form — refs inside `reversibility` by semantic_id only.
    pub fn semantic_json(&self) -> Json {
        match &self.attributes {
            Some(a) => Json::obj([
                ("domain", Json::str(self.domain.name())),
                ("attributes", attributes_semantic_json(a)),
            ]),
            None => self.to_json(),
        }
    }

    /// Parse from canonical JSON.
    pub fn from_json(j: &Json, path: &str) -> Result<EffectClass, HirError> {
        let domain = j
            .get("domain")
            .and_then(Json::as_str)
            .ok_or_else(|| HirError::SchemaViolation {
                detail: format!("{path}.domain missing"),
            })
            .and_then(EffectDomain::parse)?;
        let attributes = match j.get("attributes") {
            Some(a) => Some(attributes_from_json(a, &format!("{path}.attributes"))?),
            None => None,
        };
        Ok(EffectClass { domain, attributes })
    }

    /// Grant coverage: `self` (a grant) covers `needed` iff same domain and (self has no
    /// attributes — whole-domain grant — or the attribute vectors match).
    pub fn covers(&self, needed: &EffectClass) -> bool {
        self.domain == needed.domain
            && (self.attributes.is_none() || self.attributes == needed.attributes)
    }

    /// Is this class closed-world (all-closed attributes)?
    pub fn is_closed_world(&self) -> bool {
        match &self.attributes {
            None => false, // an unattributed class is not a declared-closed claim
            Some(a) => a.world == World::Closed,
        }
    }
}

fn attributes_json(a: &EffectAttributes) -> Json {
    Json::obj([
        ("mutability", Json::str(a.mutability.name())),
        ("repeat_safety", Json::str(a.repeat_safety.name())),
        ("world", Json::str(a.world.name())),
        ("reversibility", a.reversibility.to_json()),
    ])
}

fn attributes_semantic_json(a: &EffectAttributes) -> Json {
    Json::obj([
        ("mutability", Json::str(a.mutability.name())),
        ("repeat_safety", Json::str(a.repeat_safety.name())),
        ("world", Json::str(a.world.name())),
        ("reversibility", a.reversibility.semantic_json()),
    ])
}

fn attributes_from_json(j: &Json, path: &str) -> Result<EffectAttributes, HirError> {
    let get_str = |k: &str| -> Result<&str, HirError> {
        j.get(k)
            .and_then(Json::as_str)
            .ok_or_else(|| HirError::SchemaViolation {
                detail: format!("{path}.{k} missing"),
            })
    };
    Ok(EffectAttributes {
        mutability: Mutability::parse(get_str("mutability")?)?,
        repeat_safety: RepeatSafety::parse(get_str("repeat_safety")?)?,
        world: World::parse(get_str("world")?)?,
        reversibility: Reversibility::from_json(
            j.get("reversibility")
                .ok_or_else(|| HirError::SchemaViolation {
                    detail: format!("{path}.reversibility missing"),
                })?,
            &format!("{path}.reversibility"),
        )?,
    })
}

/// `ValidatorKind` (§3.1.3): `executable | schema | predicate | judge | human`. `judge`
/// carries the profile tuple `{rubric, profile: ProfileRef, calibration_ref?, charged_to}`
/// — the model identity lives only in `ProfileRef` (T-LCD-01; AC-IR-02).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidatorKind {
    /// `executable(CompiledPayload | Invoke)`.
    Executable(ValidatorExecutable),
    /// A schema check.
    Schema,
    /// A predicate.
    Predicate,
    /// A model judge — the only construct typed by a model, and only through `ProfileRef`.
    Judge(Box<JudgeProfile>),
    /// A human validator.
    Human,
}

/// The `executable` variant's payload union: a payload leaf or an invoke step reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidatorExecutable {
    /// A `CompiledPayload` leaf.
    Payload(Box<crate::leaves::CompiledPayload>),
    /// An invoke step (`Ref` to the `Procedure` declaring it).
    Invoke(crate::refs::Ref),
}

/// The `judge` variant's profile tuple: `{rubric: Text, profile: ProfileRef,
/// calibration_ref?, charged_to: subject | instrument}` (§3.1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgeProfile {
    /// The rubric text.
    pub rubric: crate::leaves::Text,
    /// The judge's model — the sole model-typed field, always a `ProfileRef` (T-LCD-01).
    pub profile: crate::refs::ProfileRef,
    /// The calibration reference, when present.
    pub calibration_ref: Option<String>,
    /// Who the judge call is charged to.
    pub charged_to: ChargedTo,
}

/// `charged_to: subject | instrument`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargedTo {
    /// Charged to the judged subject.
    Subject,
    /// Charged to the judging instrument.
    Instrument,
}

impl ChargedTo {
    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            ChargedTo::Subject => "subject",
            ChargedTo::Instrument => "instrument",
        }
    }
}

/// `ToolCapability.effects` (§3.1.3): `pure | declared{effects: set<EffectClass>}` — **closed
/// only**: the spec's declarative language is closed-effects-only; an agent that needs open
/// sets takes `ToolEffects::Declared` with a declared `open` world (the `open` attribute) —
/// the *attribute*, not the set, carries openness. Recorded in the S1.4 ADR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolEffects {
    /// `pure` — no effects.
    Pure,
    /// `declared` — the closed effect set the capability declares.
    Declared(std::collections::BTreeSet<EffectClass>),
}

/// `PreconditionDomain` (§3.1.3): `ToolCapability.preconditions: [PreconditionDomain]` — a
/// closed sum. The spec names the sum but does not enumerate its members in §3.1; the
/// registered set below is the minimal HIR/1 membership, growth by dialect bump (CC8). See
/// the S1.4 ADR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PreconditionDomain {
    /// Filesystem availability/shape.
    Filesystem,
    /// Network reachability.
    Network,
    /// Credential presence.
    Credentials,
    /// Environment variables/platform.
    Environment,
    /// Clock/time-box conditions.
    Temporal,
    /// A declared capability must be present.
    Capability,
}

/// The registered precondition domains (closed sum, CC8-governed).
pub const PRECONDITION_DOMAINS: [PreconditionDomain; 6] = [
    PreconditionDomain::Filesystem,
    PreconditionDomain::Network,
    PreconditionDomain::Credentials,
    PreconditionDomain::Environment,
    PreconditionDomain::Temporal,
    PreconditionDomain::Capability,
];

impl PreconditionDomain {
    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            PreconditionDomain::Filesystem => "filesystem",
            PreconditionDomain::Network => "network",
            PreconditionDomain::Credentials => "credentials",
            PreconditionDomain::Environment => "environment",
            PreconditionDomain::Temporal => "temporal",
            PreconditionDomain::Capability => "capability",
        }
    }

    /// Parse; unknown → [`HirError::UnknownKind`].
    pub fn parse(s: &str) -> Result<PreconditionDomain, HirError> {
        PRECONDITION_DOMAINS
            .iter()
            .copied()
            .find(|d| d.name() == s)
            .ok_or_else(|| HirError::UnknownKind {
                kind: format!("precondition domain {s}"),
            })
    }
}

impl fmt::Display for EntityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_is_exactly_thirteen_two_seven() {
        // AC-IR-01: exactly thirteen entities, two opaque leaves, seven edges.
        assert_eq!(ENTITY_KINDS.len(), 13);
        assert_eq!(LEAF_KINDS.len(), 2);
        assert_eq!(EDGE_KINDS.len(), 7);
    }

    #[test]
    fn every_kind_is_since_hir1() {
        // AC-IR-01 / CC8: `since = HIR/1` on every kind.
        for k in ENTITY_KINDS {
            assert_eq!(k.since(), "HIR/1");
            assert_eq!(k.until(), None);
        }
        for e in EDGE_KINDS {
            assert_eq!(e.since(), "HIR/1");
        }
    }

    #[test]
    fn unknown_kind_is_refused() {
        assert!(matches!(
            EntityKind::parse("Goal2"),
            Err(HirError::UnknownKind { .. })
        ));
        assert!(matches!(
            EdgeKind::parse("produces"),
            Err(HirError::UnknownKind { .. })
        ));
        assert!(matches!(
            EffectDomain::parse("filesystem"),
            Err(HirError::UnknownKind { .. })
        ));
    }

    #[test]
    fn home_table_matches_the_fixed_row() {
        // AC-IR-01: the §3.1.3 per-kind classify_home values (DF-S1.1-1).
        assert_eq!(EntityKind::Goal.home(), Plane::Control);
        assert_eq!(EntityKind::Observation.home(), Plane::Observation);
        assert_eq!(EntityKind::ContextItem.home(), Plane::Observation);
        assert_eq!(EntityKind::Memory.home(), Plane::State);
        assert_eq!(EntityKind::Procedure.home(), Plane::Control);
        assert_eq!(EntityKind::ToolCapability.home(), Plane::Action);
        assert_eq!(EntityKind::Permission.home(), Plane::Security);
        assert_eq!(EntityKind::Effect.home(), Plane::Action);
        assert_eq!(EntityKind::Artifact.home(), Plane::State);
        assert_eq!(EntityKind::Validator.home(), Plane::Verification);
        assert_eq!(EntityKind::AgentProcess.home(), Plane::Control);
        assert_eq!(EntityKind::Budget.home(), Plane::Measurement);
        assert_eq!(EntityKind::HarnessRule.home(), Plane::Measurement);
    }
}
