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
//! operator*, *component class named in §2*, and *boundary component group*; those are the
//! classifiable domain of [`classify_home`] here. The per-kind plane values of the thirteen IR
//! *entity* kinds are the §3.1.3 `classify_home` table — MUST-data owned by ticket S1.4
//! (R-2.1.2) and held interim under **ADR-0216** (OQ-467) until WS-A2/WS-A3 ratify them. S1.1
//! does not re-decide that deferred data (manifest: "tickets inherit; they do not re-decide");
//! the entity-kind roster is registered as [`IR_ENTITY_KINDS`] with its home delegated there.
//! See the S1.1 DEFERRALS row.

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

/// A registered kind whose home §2 fixes directly — the classifiable domain of the home-plane
/// rule at S1.1 (event families, plane operators, §2-named component classes, boundary
/// components, the run envelope). `classify_home` is total (enum-exhaustive) over this domain.
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
/// values are the §3.1.3 `classify_home` table — MUST-data owned by **S1.4 (R-2.1.2)**, held
/// interim under **ADR-0216** (OQ-467). Registered here for the AC-A2-1 roster; S1.1 does not
/// publish their planes (see the S1.1 DEFERRALS row).
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
}
