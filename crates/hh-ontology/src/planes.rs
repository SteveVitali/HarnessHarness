//! The seven planes, the two boundaries, and the **home-plane rule** (spec §2.2; ADR-0012
//! decision 2, ADR-0016). **CC10 originates here.**
//!
//! A plane is a *partition of harness responsibilities*, not an architectural layer: planes
//! carry no dependency direction (§2.2.2). The home-plane rule: every IR entity kind, component
//! class, plane operator and event family declares **exactly one**
//! `home ∈ {P1..P7} ∪ {model_boundary, environment_boundary} ∪ {run_lifecycle}`;
//! `classify_home` is **total** over registered kinds, and an unclassifiable kind is the schema
//! error [`UnclassifiedKind`] at IR validation — the reflexive-LCD guard that nothing hides
//! "between planes" (§2.2.2; §2.9.6; §2.10 "a `misc` plane" escape hatch is refused).
//!
//! **Scope boundary (S1.1 vs S1.4).** §2 fixes the home of every *event family*, *plane
//! operator*, *component class named in §2*, and *boundary component group*. The per-kind plane
//! values of the thirteen IR *entity* kinds and the two leaves are the §3.1.3 `classify_home`
//! table — published by **S1.4** (R-2.1.2) as [`IrEntityKind::home`], held interim under
//! **ADR-0216** (OQ-467) until WS-A2/WS-A3 ratify them by an ADR-0048-class ruling (a changed
//! row is then an HIR dialect bump, not an in-place edit).

use std::fmt;

/// The seven planes (§2.2.1). Names and core questions are the v0 table, kept (ADR-0012 D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Plane {
    /// P1 — Observation & context.
    Observation,
    /// P2 — Action & tools.
    Action,
    /// P3 — Control & orchestration.
    Control,
    /// P4 — Verification & feedback.
    Verification,
    /// P5 — State & durability.
    State,
    /// P6 — Security & governance.
    Security,
    /// P7 — Measurement & evolution.
    Measurement,
}

impl Plane {
    /// All seven planes, in P1..P7 order.
    pub const ALL: [Plane; 7] = [
        Plane::Observation,
        Plane::Action,
        Plane::Control,
        Plane::Verification,
        Plane::State,
        Plane::Security,
        Plane::Measurement,
    ];

    /// The `P1`..`P7` tag.
    pub fn tag(self) -> &'static str {
        match self {
            Plane::Observation => "P1",
            Plane::Action => "P2",
            Plane::Control => "P3",
            Plane::Verification => "P4",
            Plane::State => "P5",
            Plane::Security => "P6",
            Plane::Measurement => "P7",
        }
    }

    /// The plane's name (§2.2.1).
    pub fn name(self) -> &'static str {
        match self {
            Plane::Observation => "Observation & context",
            Plane::Action => "Action & tools",
            Plane::Control => "Control & orchestration",
            Plane::Verification => "Verification & feedback",
            Plane::State => "State & durability",
            Plane::Security => "Security & governance",
            Plane::Measurement => "Measurement & evolution",
        }
    }

    /// The plane's *core question* (§2.2.1). Required present for AC-A2-1.
    pub fn core_question(self) -> &'static str {
        match self {
            Plane::Observation => "What does the model see now?",
            Plane::Action => "What can it do, and how?",
            Plane::Control => "Who decides the next step?",
            Plane::Verification => "How does the system know it is right?",
            Plane::State => "What survives a turn/process/window?",
            Plane::Security => "What is the maximum allowed blast radius?",
            Plane::Measurement => "How does the harness improve?",
        }
    }
}

/// The two first-class boundaries (§2.3). There is no eighth plane (ADR-0012 D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Boundary {
    /// M | H — the harness and every model snapshot it binds; crossed by `propose`.
    Model,
    /// H | E — the harness and the external world; crossed by `execute`.
    Environment,
}

impl Boundary {
    /// The record tag (`model_boundary` / `environment_boundary`).
    pub fn tag(self) -> &'static str {
        match self {
            Boundary::Model => "model_boundary",
            Boundary::Environment => "environment_boundary",
        }
    }

    /// The crossing operator (`propose` / `execute`) — §2.3.
    pub fn crossing_operator(self) -> Operator {
        match self {
            Boundary::Model => Operator::Propose,
            Boundary::Environment => Operator::Execute,
        }
    }
}

/// The home codomain: `{P1..P7} ∪ {model_boundary, environment_boundary} ∪ {run_lifecycle}`
/// (§2.2.2; §2.9.4). Exactly one per registered kind — the home-plane rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Home {
    /// One of the seven planes.
    Plane(Plane),
    /// One of the two boundaries.
    Boundary(Boundary),
    /// The instrument-level run/turn envelope; it owns no harness responsibility (§2.2.3).
    RunLifecycle,
}

impl Home {
    /// The record tag.
    pub fn tag(self) -> String {
        match self {
            Home::Plane(p) => p.tag().to_string(),
            Home::Boundary(b) => b.tag().to_string(),
            Home::RunLifecycle => "run_lifecycle".to_string(),
        }
    }
}

/// The eight plane operators plus `propose` (§2.5.2; ADR-0012 D5). One `harness step` is the
/// composition of these around the model boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operator {
    /// `render` — P1: σ_t ↦ c_t; the context builder.
    Render,
    /// `propose` — model boundary: a_t ~ π_M when β assigns the point to `model`.
    Propose,
    /// `interpret` — P2: parse/validate a_t into a typed action.
    Interpret,
    /// `authorize` — P6: the reference monitor confers or refuses authority; always code-owned.
    Authorize,
    /// `execute` — P2 → environment boundary: effect on E.
    Execute,
    /// `verify` — P4: validators/oracles/critics reconcile claims with state.
    Verify,
    /// `persist` — P5: append events, checkpoint; the run ledger is authoritative.
    Persist,
    /// `decide` — P3: the strategy proposes a `ControlDecision`.
    Decide,
    /// `observe` — P7: records every operation.
    Observe,
}

impl Operator {
    /// All operators.
    pub const ALL: [Operator; 9] = [
        Operator::Render,
        Operator::Propose,
        Operator::Interpret,
        Operator::Authorize,
        Operator::Execute,
        Operator::Verify,
        Operator::Persist,
        Operator::Decide,
        Operator::Observe,
    ];

    /// The identifier.
    pub fn name(self) -> &'static str {
        match self {
            Operator::Render => "render",
            Operator::Propose => "propose",
            Operator::Interpret => "interpret",
            Operator::Authorize => "authorize",
            Operator::Execute => "execute",
            Operator::Verify => "verify",
            Operator::Persist => "persist",
            Operator::Decide => "decide",
            Operator::Observe => "observe",
        }
    }

    /// The operator's home (§2.5.2 table). `execute` is homed on P2 but crosses the environment
    /// boundary; `propose` is homed on the model boundary (the crossing point).
    pub fn home(self) -> Home {
        match self {
            Operator::Render => Home::Plane(Plane::Observation),
            Operator::Propose => Home::Boundary(Boundary::Model),
            Operator::Interpret => Home::Plane(Plane::Action),
            Operator::Authorize => Home::Plane(Plane::Security),
            Operator::Execute => Home::Plane(Plane::Action),
            Operator::Verify => Home::Plane(Plane::Verification),
            Operator::Persist => Home::Plane(Plane::State),
            Operator::Decide => Home::Plane(Plane::Control),
            Operator::Observe => Home::Plane(Plane::Measurement),
        }
    }
}

/// The component classes whose home §2 fixes directly (§2.2.3). Other registered classes carry
/// `home` on their `ClassRecord` (ADR-0151 D5) — that registry is ticket S1.8+, out of S1.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentClass {
    /// `control_strategy` — P3 (ADR-0103).
    ControlStrategy,
    /// `compute_policy` — P3 (ADR-0188).
    ComputePolicy,
    /// `context_policy` — P1 (ADR-0072).
    ContextPolicy,
}

impl ComponentClass {
    /// All §2-homed classes.
    pub const ALL: [ComponentClass; 3] = [
        ComponentClass::ControlStrategy,
        ComponentClass::ComputePolicy,
        ComponentClass::ContextPolicy,
    ];

    /// The identifier.
    pub fn name(self) -> &'static str {
        match self {
            ComponentClass::ControlStrategy => "control_strategy",
            ComponentClass::ComputePolicy => "compute_policy",
            ComponentClass::ContextPolicy => "context_policy",
        }
    }

    /// The class's home (§2.2.3).
    pub fn home(self) -> Home {
        match self {
            ComponentClass::ControlStrategy => Home::Plane(Plane::Control),
            ComponentClass::ComputePolicy => Home::Plane(Plane::Control),
            ComponentClass::ContextPolicy => Home::Plane(Plane::Observation),
        }
    }
}

/// The boundary-component groups (§2.2.3 / §2.3). Every one is homed on a *boundary*, not a
/// plane — "how do we call the model / reach the world" is a crossing point, not a plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoundaryComponent {
    /// Model gateway / router / Profile Compiler / caches / model-call interposition.
    ModelPlaneComponent,
    /// Environment/sandbox handle, effect capture, egress mediation.
    EnvironmentPlaneComponent,
}

impl BoundaryComponent {
    /// All boundary-component groups.
    pub const ALL: [BoundaryComponent; 2] = [
        BoundaryComponent::ModelPlaneComponent,
        BoundaryComponent::EnvironmentPlaneComponent,
    ];

    /// The home boundary.
    pub fn home(self) -> Home {
        match self {
            BoundaryComponent::ModelPlaneComponent => Home::Boundary(Boundary::Model),
            BoundaryComponent::EnvironmentPlaneComponent => Home::Boundary(Boundary::Environment),
        }
    }
}

/// A registered kind whose home is fixed — the classifiable domain of the home-plane rule
/// (event families, plane operators, §2-named component classes, boundary components, the run
/// envelope, and — since S1.4 — the IR entity kinds and leaves). `classify_home` is total
/// (enum-exhaustive) over this domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// An event family, homed by the `plane.noun.verb` prefix rule (§2.2.3; ADR-0048 §A).
    EventFamily(EventFamily),
    /// A plane operator (§2.5.2).
    Operator(Operator),
    /// A component class §2 homes directly (§2.2.3).
    ComponentClass(ComponentClass),
    /// A boundary-component group (§2.2.3 / §2.3).
    BoundaryComponent(BoundaryComponent),
    /// The run envelope (`lifecycle.*`) — instrument level, `run_lifecycle`.
    RunLifecycle,
    /// An IR entity kind or leaf (the §3.1.3 catalogue — published at S1.4, ADR-0216 interim).
    IrEntity(IrEntityKind),
}

/// The event families homed by the identifier-prefix rule (§2.2.3). `state` is deliberately
/// absent: there is no `state` prefix; state-changing events are stamped by their owning
/// operator's plane (P5 via `persist`) — the CF-030 ruling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventFamily {
    /// `context.*` → P1.
    Context,
    /// `action.*` → P2 (`execute` crosses the environment boundary).
    Action,
    /// `control.*` → P3.
    Control,
    /// `verification.*` → P4.
    Verification,
    /// `security.*` → P6.
    Security,
    /// `measurement.*` → P7.
    Measurement,
    /// `model.*` → model_boundary.
    Model,
    /// `lifecycle.*` → run_lifecycle (instrument level).
    Lifecycle,
}

impl EventFamily {
    /// All registered event families.
    pub const ALL: [EventFamily; 8] = [
        EventFamily::Context,
        EventFamily::Action,
        EventFamily::Control,
        EventFamily::Verification,
        EventFamily::Security,
        EventFamily::Measurement,
        EventFamily::Model,
        EventFamily::Lifecycle,
    ];

    /// The identifier prefix (`context`, `action`, …).
    pub fn prefix(self) -> &'static str {
        match self {
            EventFamily::Context => "context",
            EventFamily::Action => "action",
            EventFamily::Control => "control",
            EventFamily::Verification => "verification",
            EventFamily::Security => "security",
            EventFamily::Measurement => "measurement",
            EventFamily::Model => "model",
            EventFamily::Lifecycle => "lifecycle",
        }
    }

    /// The family's home (§2.2.3).
    pub fn home(self) -> Home {
        match self {
            EventFamily::Context => Home::Plane(Plane::Observation),
            EventFamily::Action => Home::Plane(Plane::Action),
            EventFamily::Control => Home::Plane(Plane::Control),
            EventFamily::Verification => Home::Plane(Plane::Verification),
            EventFamily::Security => Home::Plane(Plane::Security),
            EventFamily::Measurement => Home::Plane(Plane::Measurement),
            EventFamily::Model => Home::Boundary(Boundary::Model),
            EventFamily::Lifecycle => Home::RunLifecycle,
        }
    }

    /// Parse an event-family prefix. `None` for an unregistered prefix (incl. `state`).
    pub fn from_prefix(prefix: &str) -> Option<EventFamily> {
        EventFamily::ALL.into_iter().find(|f| f.prefix() == prefix)
    }
}

/// The thirteen IR entity kinds plus the two leaves (§2.2.3, ADR-0016). Their per-kind plane
/// values are the §3.1.3 `classify_home` table — published by **S1.4 (R-2.1.2)** as
/// [`IrEntityKind::home`], held interim under **ADR-0216** (OQ-467) until WS-A2/WS-A3 ratify the
/// table by an ADR-0048-class ruling. Registered here for the AC-A2-1 roster.
pub const IR_ENTITY_KINDS: [&str; 15] = [
    "Goal",
    "Observation",
    "ContextItem",
    "Memory",
    "Procedure",
    "ToolCapability",
    "Permission",
    "Effect",
    "Artifact",
    "Validator",
    "AgentProcess",
    "Budget",
    "HarnessRule",
    "Text",
    "CompiledPayload",
];

/// The thirteen IR entity kinds plus the two leaves — the §3.1.3 `classify_home` table,
/// published at **S1.4** (R-2.1.2). The thirteen entity rows are the WS-A3 dossier §6.2 primary
/// reading ratified as **MUST-data placeholders under ADR-0216** (OQ-467) until WS-A2/WS-A3
/// ratify the table by an ADR-0048-class ruling; a changed row is then a dialect bump
/// (ADR-0015), not an in-place edit.
///
/// The two **leaf** rows are not fixed by §3.1.3's table (which lists entities only); their
/// assignments are the S1.4 interim reading under the same ADR-0216 umbrella: `Text` → **P1**
/// (it is the only construct for model-facing prose — "what the model sees") and
/// `CompiledPayload` → **P2** (it is the only construct for executable bodies — "what it can
/// do, and how"). Recorded in the S1.4 build ADR; pending WS-A2/WS-A3 ratification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IrEntityKind {
    /// `Goal` — the object control decides against.
    Goal,
    /// `Observation` — what the model sees.
    Observation,
    /// `ContextItem` — the delivered unit of context.
    ContextItem,
    /// `Memory` — what survives a turn/process/window.
    Memory,
    /// `Procedure` — who decides the next step.
    Procedure,
    /// `ToolCapability` — what it can do.
    ToolCapability,
    /// `Permission` — the maximum blast radius.
    Permission,
    /// `Effect` — the action's world change (recorded in P5's ledger, gated by P6).
    Effect,
    /// `Artifact` — durable content by hash.
    Artifact,
    /// `Validator` — how the system knows it is right.
    Validator,
    /// `AgentProcess` — the process the control boundary belongs to.
    AgentProcess,
    /// `Budget` — measured and accounted; enforced at P3 decision points.
    Budget,
    /// `HarnessRule` — the evolvable, debt-carrying rule.
    HarnessRule,
    /// `Text` leaf — the only construct for model-facing prose (§3.1.2). Interim home P1.
    TextLeaf,
    /// `CompiledPayload` leaf — the only construct for executable bodies (§3.1.2). Interim
    /// home P2.
    CompiledPayloadLeaf,
}

impl IrEntityKind {
    /// All fifteen kinds (13 entities + 2 leaves), in the §3.1.3 catalogue order.
    pub const ALL: [IrEntityKind; 15] = [
        IrEntityKind::Goal,
        IrEntityKind::Observation,
        IrEntityKind::ContextItem,
        IrEntityKind::Memory,
        IrEntityKind::Procedure,
        IrEntityKind::ToolCapability,
        IrEntityKind::Permission,
        IrEntityKind::Effect,
        IrEntityKind::Artifact,
        IrEntityKind::Validator,
        IrEntityKind::AgentProcess,
        IrEntityKind::Budget,
        IrEntityKind::HarnessRule,
        IrEntityKind::TextLeaf,
        IrEntityKind::CompiledPayloadLeaf,
    ];

    /// The kind's name as it appears in the §3.1.3 catalogue / [`IR_ENTITY_KINDS`].
    pub fn name(self) -> &'static str {
        match self {
            IrEntityKind::Goal => "Goal",
            IrEntityKind::Observation => "Observation",
            IrEntityKind::ContextItem => "ContextItem",
            IrEntityKind::Memory => "Memory",
            IrEntityKind::Procedure => "Procedure",
            IrEntityKind::ToolCapability => "ToolCapability",
            IrEntityKind::Permission => "Permission",
            IrEntityKind::Effect => "Effect",
            IrEntityKind::Artifact => "Artifact",
            IrEntityKind::Validator => "Validator",
            IrEntityKind::AgentProcess => "AgentProcess",
            IrEntityKind::Budget => "Budget",
            IrEntityKind::HarnessRule => "HarnessRule",
            IrEntityKind::TextLeaf => "Text",
            IrEntityKind::CompiledPayloadLeaf => "CompiledPayload",
        }
    }

    /// The kind's home — the §3.1.3 `classify_home` value table (ADR-0216 interim MUST-data;
    /// leaf rows are the S1.4 interim reading). One plane per kind; `home_plane` on every HIR
    /// node is derived from this, never authored.
    pub fn home(self) -> Home {
        match self {
            IrEntityKind::Goal => Home::Plane(Plane::Control),
            IrEntityKind::Observation => Home::Plane(Plane::Observation),
            IrEntityKind::ContextItem => Home::Plane(Plane::Observation),
            IrEntityKind::Memory => Home::Plane(Plane::State),
            IrEntityKind::Procedure => Home::Plane(Plane::Control),
            IrEntityKind::ToolCapability => Home::Plane(Plane::Action),
            IrEntityKind::Permission => Home::Plane(Plane::Security),
            IrEntityKind::Effect => Home::Plane(Plane::Action),
            IrEntityKind::Artifact => Home::Plane(Plane::State),
            IrEntityKind::Validator => Home::Plane(Plane::Verification),
            IrEntityKind::AgentProcess => Home::Plane(Plane::Control),
            IrEntityKind::Budget => Home::Plane(Plane::Measurement),
            IrEntityKind::HarnessRule => Home::Plane(Plane::Measurement),
            IrEntityKind::TextLeaf => Home::Plane(Plane::Observation),
            IrEntityKind::CompiledPayloadLeaf => Home::Plane(Plane::Action),
        }
    }

    /// Parse a kind by its §3.1.3 catalogue name. `None` for anything unregistered (the HIR
    /// layer maps that to `UnknownKind`/`UnclassifiedKind` as appropriate).
    pub fn from_name(name: &str) -> Option<IrEntityKind> {
        IrEntityKind::ALL.into_iter().find(|k| k.name() == name)
    }
}

/// The schema error a total `classify_home` raises on an unclassifiable kind at IR validation
/// (§2.2.2; §2.9.6; ADR-0012 D2). This is the escape-hatch guard: a "misc plane" or a kind that
/// "hides between planes" is refused, not silently placed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnclassifiedKind {
    /// The identifier that could not be classified.
    pub identifier: String,
}

impl fmt::Display for UnclassifiedKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "UnclassifiedKind: {:?} has no registered home",
            self.identifier
        )
    }
}

impl std::error::Error for UnclassifiedKind {}

/// The home-plane rule as a **total** function over the registered domain (§2.2.2; ADR-0012
/// D2). Enum-exhaustive, so totality holds by construction — the compiler forbids an
/// unclassified `Kind`.
pub fn classify_home(kind: &Kind) -> Home {
    match kind {
        Kind::EventFamily(f) => f.home(),
        Kind::Operator(o) => o.home(),
        Kind::ComponentClass(c) => c.home(),
        Kind::BoundaryComponent(b) => b.home(),
        Kind::RunLifecycle => Home::RunLifecycle,
        Kind::IrEntity(e) => e.home(),
    }
}

/// Classify a raw identifier the way IR validation would — the runtime path that can meet an
/// unregistered kind and must raise [`UnclassifiedKind`] (§2.9.6). Recognizes an event
/// identifier by its `plane.noun.verb` prefix, an operator name, a §2-homed component-class
/// name, or `run_lifecycle`; everything else (a `state.*` prefix, a `misc` plane, an unknown
/// operator) is `Err(UnclassifiedKind)`.
pub fn classify_home_identifier(identifier: &str) -> Result<Home, UnclassifiedKind> {
    let err = || UnclassifiedKind {
        identifier: identifier.to_string(),
    };

    // Event identifiers are dotted: the first segment is the home prefix.
    if let Some((prefix, _rest)) = identifier.split_once('.') {
        return EventFamily::from_prefix(prefix)
            .map(|f| f.home())
            .ok_or_else(err);
    }

    // Bare operator names.
    if let Some(op) = Operator::ALL.into_iter().find(|o| o.name() == identifier) {
        return Ok(op.home());
    }
    // §2-homed component-class names.
    if let Some(c) = ComponentClass::ALL
        .into_iter()
        .find(|c| c.name() == identifier)
    {
        return Ok(c.home());
    }
    // §3.1.3 IR entity kinds and leaves (published at S1.4; ADR-0216 interim).
    if let Some(e) = IrEntityKind::from_name(identifier) {
        return Ok(e.home());
    }
    if identifier == "run_lifecycle" {
        return Ok(Home::RunLifecycle);
    }
    Err(err())
}

/// The full registered classifiable domain at S1.1 — used by the totality check (AC-A2-1) and
/// the spec-DAG check.
pub fn registered_kinds() -> Vec<Kind> {
    let mut kinds = Vec::new();
    for f in EventFamily::ALL {
        kinds.push(Kind::EventFamily(f));
    }
    for o in Operator::ALL {
        kinds.push(Kind::Operator(o));
    }
    for c in ComponentClass::ALL {
        kinds.push(Kind::ComponentClass(c));
    }
    for b in BoundaryComponent::ALL {
        kinds.push(Kind::BoundaryComponent(b));
    }
    kinds.push(Kind::RunLifecycle);
    // Since S1.4 the domain includes the IR entity kinds and leaves (§3.1.3).
    for e in IrEntityKind::ALL {
        kinds.push(Kind::IrEntity(e));
    }
    kinds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_planes_have_names_and_core_questions() {
        // AC-A2-1: the seven planes with core questions are present.
        assert_eq!(Plane::ALL.len(), 7);
        for (i, p) in Plane::ALL.iter().enumerate() {
            assert_eq!(p.tag(), format!("P{}", i + 1));
            assert!(!p.name().is_empty());
            assert!(
                p.core_question().ends_with('?'),
                "plane {} core question must be a question",
                p.tag()
            );
        }
        // The characteristic P4/P7 that make novelty claim (a) checkable (§2.2.2).
        assert_eq!(Plane::Verification.tag(), "P4");
        assert_eq!(Plane::Measurement.tag(), "P7");
    }

    #[test]
    fn two_boundaries_have_crossing_operators() {
        // AC-A2-1: the two boundaries are present with their crossing operators (§2.3).
        assert_eq!(Boundary::Model.crossing_operator(), Operator::Propose);
        assert_eq!(Boundary::Environment.crossing_operator(), Operator::Execute);
        assert_eq!(Boundary::Model.tag(), "model_boundary");
        assert_eq!(Boundary::Environment.tag(), "environment_boundary");
    }

    #[test]
    fn classify_home_is_total_over_registered_kinds() {
        // AC-A2-1: totality of classify_home over every registered kind, class, operator and
        // event family. Enum-exhaustiveness proves it; this asserts every registered kind
        // resolves to a home whose tag is in the codomain.
        let valid_tags: std::collections::HashSet<String> = Plane::ALL
            .iter()
            .map(|p| p.tag().to_string())
            .chain(["model_boundary", "environment_boundary", "run_lifecycle"].map(String::from))
            .collect();
        let kinds = registered_kinds();
        assert!(!kinds.is_empty());
        for k in kinds {
            let home = classify_home(&k);
            assert!(
                valid_tags.contains(&home.tag()),
                "kind {:?} homed to {} outside the codomain",
                k,
                home.tag()
            );
        }
    }

    #[test]
    fn operators_home_to_the_spec_2_5_2_table() {
        assert_eq!(Operator::Render.home(), Home::Plane(Plane::Observation));
        assert_eq!(Operator::Propose.home(), Home::Boundary(Boundary::Model));
        assert_eq!(Operator::Interpret.home(), Home::Plane(Plane::Action));
        assert_eq!(Operator::Authorize.home(), Home::Plane(Plane::Security));
        assert_eq!(Operator::Execute.home(), Home::Plane(Plane::Action));
        assert_eq!(Operator::Verify.home(), Home::Plane(Plane::Verification));
        assert_eq!(Operator::Persist.home(), Home::Plane(Plane::State));
        assert_eq!(Operator::Decide.home(), Home::Plane(Plane::Control));
        assert_eq!(Operator::Observe.home(), Home::Plane(Plane::Measurement));
    }

    #[test]
    fn event_families_home_by_prefix() {
        assert_eq!(EventFamily::Context.home(), Home::Plane(Plane::Observation));
        assert_eq!(
            EventFamily::Measurement.home(),
            Home::Plane(Plane::Measurement)
        );
        assert_eq!(EventFamily::Model.home(), Home::Boundary(Boundary::Model));
        assert_eq!(EventFamily::Lifecycle.home(), Home::RunLifecycle);
    }

    #[test]
    fn identifier_path_classifies_and_rejects() {
        // The runtime IR-validation path.
        assert_eq!(
            classify_home_identifier("context.artefact.delivered").unwrap(),
            Home::Plane(Plane::Observation)
        );
        assert_eq!(
            classify_home_identifier("verification.artefact.followed").unwrap(),
            Home::Plane(Plane::Verification)
        );
        assert_eq!(
            classify_home_identifier("model.call.completed").unwrap(),
            Home::Boundary(Boundary::Model)
        );
        assert_eq!(
            classify_home_identifier("authorize").unwrap(),
            Home::Plane(Plane::Security)
        );
        assert_eq!(
            classify_home_identifier("control_strategy").unwrap(),
            Home::Plane(Plane::Control)
        );
    }

    #[test]
    fn no_state_prefix_and_misc_plane_are_unclassified() {
        // §2.2.3 CF-030: there is no `state` prefix; §2.10: a `misc` plane is refused.
        assert_eq!(
            classify_home_identifier("state.file.written"),
            Err(UnclassifiedKind {
                identifier: "state.file.written".to_string()
            })
        );
        assert!(classify_home_identifier("misc.thing.happened").is_err());
        assert!(classify_home_identifier("frobnicate").is_err());
    }

    #[test]
    fn ir_entity_kinds_roster_is_registered_and_delegated() {
        // AC-A2-1 roster: the thirteen entity kinds + two leaves are registered; their planes
        // are the §3.1.3 table owned by S1.4 (ADR-0216 interim), not published here.
        assert_eq!(IR_ENTITY_KINDS.len(), 15);
        for k in [
            "ToolCapability",
            "Permission",
            "Budget",
            "Text",
            "CompiledPayload",
        ] {
            assert!(IR_ENTITY_KINDS.contains(&k), "missing entity kind {k}");
        }
    }
    #[test]
    fn ir_entity_home_table_matches_the_3_1_3_values() {
        // DF-S1.1-1 / §3.1.3 `classify_home` table (ADR-0216 interim, OQ-467): the thirteen
        // entity rows plus the two leaf rows, one plane each.
        let table: [(IrEntityKind, Plane); 15] = [
            (IrEntityKind::Goal, Plane::Control),
            (IrEntityKind::Observation, Plane::Observation),
            (IrEntityKind::ContextItem, Plane::Observation),
            (IrEntityKind::Memory, Plane::State),
            (IrEntityKind::Procedure, Plane::Control),
            (IrEntityKind::ToolCapability, Plane::Action),
            (IrEntityKind::Permission, Plane::Security),
            (IrEntityKind::Effect, Plane::Action),
            (IrEntityKind::Artifact, Plane::State),
            (IrEntityKind::Validator, Plane::Verification),
            (IrEntityKind::AgentProcess, Plane::Control),
            (IrEntityKind::Budget, Plane::Measurement),
            (IrEntityKind::HarnessRule, Plane::Measurement),
            (IrEntityKind::TextLeaf, Plane::Observation),
            (IrEntityKind::CompiledPayloadLeaf, Plane::Action),
        ];
        assert_eq!(table.len(), IrEntityKind::ALL.len());
        for (kind, want) in table {
            assert_eq!(kind.home(), Home::Plane(want), "{} home", kind.name());
            // The roster and the enum agree on names.
            assert!(IR_ENTITY_KINDS.contains(&kind.name()));
            // The identifier path classifies them too (the IR-validation path).
            assert_eq!(
                classify_home_identifier(kind.name()).unwrap(),
                Home::Plane(want),
                "classify_home_identifier({})",
                kind.name()
            );
        }
        // registered_kinds now covers every IR entity kind (totality over the domain).
        let registered = registered_kinds();
        for e in IrEntityKind::ALL {
            assert!(registered.contains(&Kind::IrEntity(e)));
        }
    }
}
