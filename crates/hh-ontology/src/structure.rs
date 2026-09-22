//! The two ontology levels (§2.1) and the three cross-cutting properties (§2.4) — the remaining
//! AC-A2-1 constituents beyond the seven planes and two boundaries.
//!
//! - **Two levels** (§2.1; ADR-0012 D1): every term belongs to exactly one of the *object* level
//!   (one harness — its planes, boundaries, IR entities, θ) and the *instrument* level (the Lab
//!   and everything that *compares* harnesses — participants, configurations, arms, runs,
//!   metrics, the comparison plane). Collapsing the split is how the historical product-name
//!   collision arose (CF-013).
//! - **Three cross-cutting properties** (§2.4; ADR-0012 D4): resource economics, provenance and
//!   model conditioning are mandatory **attributes of every entity and event**, *never planes*.

/// The two levels of the ontology (§2.1). A term belongs to exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Level {
    /// One harness: its seven planes, two boundaries, IR entities/artifacts, classes/variants,
    /// policies, θ, the harness step, π_system. Realized by the reference runtime and compiler.
    Object,
    /// The Harness Lab and everything that compares harnesses: participants, configurations,
    /// arms, runs and the run envelope, metrics, conformance records, fitted-surface reports,
    /// the Hosting ABI, the comparison plane. Realized by the Lab and results store.
    Instrument,
}

impl Level {
    /// Both levels.
    pub const ALL: [Level; 2] = [Level::Object, Level::Instrument];

    /// The record tag.
    pub fn tag(self) -> &'static str {
        match self {
            Level::Object => "object",
            Level::Instrument => "instrument",
        }
    }
}

/// A cross-cutting property (§2.4) — a mandatory *attribute* on every entity and event, never a
/// plane. Attempting to treat one as a plane is a category error the type system prevents (there
/// is no `Plane` conversion).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CrossCuttingProperty {
    /// Resource economics — `resource` accounting over the kernel dimension list.
    ResourceEconomics,
    /// Provenance — the single CF-006 scheme `{origin, authority, taint, readers, scope, …}`.
    Provenance,
    /// Model conditioning — the `{semantic_id, surface_owner}` split of every HIR node.
    ModelConditioning,
}

impl CrossCuttingProperty {
    /// All three properties (§2.4).
    pub const ALL: [CrossCuttingProperty; 3] = [
        CrossCuttingProperty::ResourceEconomics,
        CrossCuttingProperty::Provenance,
        CrossCuttingProperty::ModelConditioning,
    ];

    /// The attribute name carried on every entity/event.
    pub fn attribute(self) -> &'static str {
        match self {
            CrossCuttingProperty::ResourceEconomics => "resource",
            CrossCuttingProperty::Provenance => "provenance",
            CrossCuttingProperty::ModelConditioning => "conditioning",
        }
    }
}

/// The three-way classification every §2 concept falls into (§2.8; ADR-0012 D8). Derived views
/// are never stored as truth; reports are computed once and persisted with provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityStanding {
    /// First-class: identified, versioned, stored.
    FirstClass,
    /// A derived view: recomputable, never truth.
    DerivedView,
    /// A report: computed once, persisted with provenance.
    Report,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_levels_are_present() {
        // AC-A2-1: the two-level statement.
        assert_eq!(Level::ALL.len(), 2);
        assert_eq!(Level::Object.tag(), "object");
        assert_eq!(Level::Instrument.tag(), "instrument");
    }

    #[test]
    fn three_cross_cutting_attributes_are_present() {
        // AC-A2-1 / §2.4: exactly the three mandatory attributes, never planes.
        let attrs: Vec<&str> = CrossCuttingProperty::ALL
            .iter()
            .map(|p| p.attribute())
            .collect();
        assert_eq!(attrs, ["resource", "provenance", "conditioning"]);
    }

    #[test]
    fn entity_standing_has_three_kinds() {
        // §2.8 trichotomy.
        for s in [
            EntityStanding::FirstClass,
            EntityStanding::DerivedView,
            EntityStanding::Report,
        ] {
            let _ = s; // present and distinct.
        }
        assert_ne!(EntityStanding::FirstClass, EntityStanding::Report);
    }
}
