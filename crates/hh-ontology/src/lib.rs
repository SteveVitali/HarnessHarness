//! `hh-ontology` — the C0/Stage-1 ontology and formal model (spec §2, scope item **R-2.1.1**).
//!
//! This crate is the E1 *build binding* (CC4: the spec/contracts name no language; this crate,
//! alongside the build ADRs, is the one place that realizes them concretely) of the ontology
//! §2 fixes:
//!
//! - the **seven planes** and the **home-plane rule** — [`planes`] (CC10 originates here);
//! - the **policy-stack formalism** and its symbols θ, M, E, D, B, κ, β, Δ, Ψ, J, π_M,
//!   π_system — [`formal`];
//! - **configuration** κ and both configuration ids — [`config`];
//! - the **two participant classes** and `admissible_granularities` — [`participant`];
//! - **validity vs compliance** as a measured pair — [`compliance`];
//! - the **compatibility surface** Ψ_θ — [`surface`];
//! - the **control boundary** β and the closed decision-point set 𝒟 — [`control`];
//! - the **spec-DAG check** (tier monotonicity, acyclicity, no HIR→Hosting-ABI edge) — [`dag`];
//! - the **glossary register** and its check — [`glossary`].
//!
//! The build stage this crate lands (§2.9.3, C0/Stage 1): the text, the `home` table, κ and
//! both configuration ids, β *representable*, the descriptor schema with `class`,
//! `admissible_granularities` as an engine precondition, the chain-event *schema* with
//! `detector`, `MetricDeclaration.detector_classes_allowed`, `activation_observable`, and the
//! `validity()` contract. Behaviours reserved for later stages (β *variable*, `paired_effect`,
//! emitted `delivered`/`activated`/`followed`, judged detectors, fitted surfaces from real
//! rows) are represented as *shapes and preconditions* here, never executed.
//!
//! Nothing is decided anew: every construct cites its ratifying spec ADR (ADR-0012/0013/0014
//! and their amendments). A construct that §2 leaves interim is marked at its definition.

pub mod compliance;
pub mod config;
pub mod control;
pub mod dag;
pub mod formal;
pub mod glossary;
pub mod participant;
pub mod planes;
pub mod structure;
pub mod surface;

pub use compliance::{
    validity, ChainEvent, Detector, MetricDeclaration, NaReason, Validity, ValidityOutcome,
};
pub use config::{Configuration, ConfigurationId, ConfigurationVersionId, Factor};
pub use control::{ControlBoundary, DecisionPoint, Owner};
pub use dag::{spec_dag_check, DagReport};
pub use formal::{resolve, Symbol, SymbolDef};
pub use glossary::{glossary_check, GlossaryFinding};
pub use participant::{
    admissible_granularities, describe, Granularity, Observability, ParticipantClass,
    ParticipantDescriptor,
};
pub use planes::{classify_home, Boundary, Home, Kind, Plane, UnclassifiedKind};
pub use structure::{CrossCuttingProperty, EntityStanding, Level};
