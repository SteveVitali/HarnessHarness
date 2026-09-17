//! The risk-class **projection** (§5a.2 R-2.2.2; ADR-0031 §1): `RiskClass` is derived
//! from a capability's declared [`EffectAttributes`], never declared twice (CF-104).
//!
//! - `reversibility' = read_only if mutability = read_only else reversibility` — the
//!   HIR `reversible(Ref<Procedure>)` constructor projects to bare `reversible` (the
//!   procedure ref stays on the declaration; the runtime class carries no ref);
//! - `scope = workspace_local if world = closed else external`;
//! - `repeat_safety` verbatim;
//! - absent/undeclared attributes ⇒ [`RiskClass::UNKNOWN`] — `unverified` declarations
//!   project to the most dangerous class (ADR-0031 §2; ADR-0212/OQ-223 interim rule —
//!   both `unknown_domain` spellings land on `{irreversible, non_idempotent,
//!   external}`).

use hh_ontology::risk::{RiskClass, RiskReversibility, RiskScope};
use hh_provenance::AuthorityClass;

use crate::kinds::{EffectAttributes, Mutability, Reversibility, ToolEffects, World};
use crate::records::ToolCapabilityRecord;

/// `project_risk(attributes) → RiskClass` — `None` (no declared attribute vector, an
/// `unverified`/`unknown_domain` declaration) projects to the most dangerous class.
pub fn project_risk(attributes: Option<&EffectAttributes>) -> RiskClass {
    let Some(a) = attributes else {
        return RiskClass::UNKNOWN;
    };
    let reversibility = if a.mutability == Mutability::ReadOnly {
        RiskReversibility::ReadOnly
    } else {
        match &a.reversibility {
            Reversibility::Reversible(_) => RiskReversibility::Reversible,
            Reversibility::Compensable => RiskReversibility::Compensable,
            Reversibility::Irreversible => RiskReversibility::Irreversible,
        }
    };
    let scope = if a.world == World::Closed {
        RiskScope::WorkspaceLocal
    } else {
        RiskScope::External
    };
    RiskClass {
        reversibility,
        repeat_safety: a.repeat_safety,
        scope,
    }
}

/// `capability_risk(record, authority) → RiskClass` — the capability-level
/// projection the registry's `project_risk` verb serves (R-2.5.1; AC-R-2.5.1-3):
///
/// - a declaration at `unverified` authority projects to
///   [`RiskClass::UNKNOWN`] — lifted sources never earn a lower claim at
///   register (ADR-0031 §2);
/// - `pure` projects to [`RiskClass::READ_ONLY`] (the least-dangerous class —
///   a pure capability moves nothing);
/// - `declared` folds [`project_risk`] over the set with
///   [`RiskClass::max_by_danger`] — monotone; the most dangerous declared
///   effect class wins.
pub fn capability_risk(rec: &ToolCapabilityRecord, authority: AuthorityClass) -> RiskClass {
    if authority == AuthorityClass::Unverified {
        return RiskClass::UNKNOWN;
    }
    match &rec.effects {
        ToolEffects::Pure => RiskClass::READ_ONLY,
        ToolEffects::Declared(set) => set
            .iter()
            .map(|e| project_risk(e.attributes.as_ref()))
            .fold(RiskClass::READ_ONLY, RiskClass::max_by_danger),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::RepeatSafety;
    use hh_ontology::risk::RepeatSafety as OntologyRepeatSafety;

    #[test]
    fn absent_attributes_project_to_unknown() {
        assert_eq!(project_risk(None), RiskClass::UNKNOWN);
    }

    #[test]
    fn read_only_mutability_forces_read_only_reversibility() {
        let a = EffectAttributes {
            mutability: Mutability::ReadOnly,
            repeat_safety: RepeatSafety::NonIdempotent,
            world: World::Open,
            reversibility: Reversibility::Irreversible,
        };
        let r = project_risk(Some(&a));
        assert_eq!(r.reversibility, RiskReversibility::ReadOnly);
        // The other axes are verbatim projections — mutability does not mask them.
        assert_eq!(r.repeat_safety, RepeatSafety::NonIdempotent);
        assert_eq!(r.scope, RiskScope::External);
    }

    #[test]
    fn declaration_axes_project_verbatim() {
        let a = EffectAttributes {
            mutability: Mutability::Additive,
            repeat_safety: RepeatSafety::Idempotent,
            world: World::Closed,
            reversibility: Reversibility::Compensable,
        };
        let r = project_risk(Some(&a));
        assert_eq!(r.reversibility, RiskReversibility::Compensable);
        assert_eq!(r.scope, RiskScope::WorkspaceLocal);
        assert_eq!(r.repeat_safety, RepeatSafety::Idempotent);
    }

    #[test]
    fn repeat_safety_is_the_one_ontology_sum() {
        // CC1: `hh_hir::kinds::RepeatSafety` IS `hh_ontology::risk::RepeatSafety`.
        let a: RepeatSafety = OntologyRepeatSafety::Idempotent;
        assert_eq!(a.name(), "idempotent");
    }
}
