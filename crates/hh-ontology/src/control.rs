//! The control boundary β and the closed decision-point set 𝒟 (spec §2.5.5; ADR-0012 D6,
//! ADR-0048 rule 10, ADR-0103, ADR-0106).
//!
//! β : 𝒟 → {code, model, human} with per-decision-point guards; a typed record on
//! `AgentProcess.native` (§2.5.5). Stage-1 scope: β is **representable** (this record and the
//! closed decision-point sum); β becomes *variable* only at Stage 3. The envelope-reserved
//! points are always `code`; a contrary assignment is refused with [`BoundaryError`]
//! (`IncompatibleBoundary`) at `validate`/`open` (§2.5.5; ADR-0103 I5/I6).

use std::collections::BTreeMap;

/// The closed decision-point set 𝒟 (§2.5.5; ADR-0048 rule 10, CF-107). Extended only by a
/// dialect bump (`route`/`effort` scheduled for HIR/2, OQ-425).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DecisionPoint {
    /// plan
    Plan,
    /// act
    Act,
    /// retrieve
    Retrieve,
    /// compact
    Compact,
    /// verify
    Verify,
    /// delegate
    Delegate,
    /// authorize — envelope-reserved: always `code`.
    Authorize,
    /// retry — infrastructure retry is envelope-reserved: always `code`.
    Retry,
    /// stop — `stop{budget_exhausted|context_exhausted|cancelled}` is always `code`.
    Stop,
    /// escalate
    Escalate,
}

impl DecisionPoint {
    /// The full closed set (ten points).
    pub const ALL: [DecisionPoint; 10] = [
        DecisionPoint::Plan,
        DecisionPoint::Act,
        DecisionPoint::Retrieve,
        DecisionPoint::Compact,
        DecisionPoint::Verify,
        DecisionPoint::Delegate,
        DecisionPoint::Authorize,
        DecisionPoint::Retry,
        DecisionPoint::Stop,
        DecisionPoint::Escalate,
    ];

    /// Whether this point is envelope-reserved to `code` (§2.5.5; ADR-0103 I5/I6). `authorize`
    /// is always code; infrastructure `retry` and `stop` (budget/context/cancel) are always code.
    pub fn is_envelope_reserved_code(self) -> bool {
        matches!(
            self,
            DecisionPoint::Authorize | DecisionPoint::Retry | DecisionPoint::Stop
        )
    }
}

/// The owner a decision point may be assigned to (§2.5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Owner {
    /// Deterministic code.
    Code,
    /// The model.
    Model,
    /// A human.
    Human,
}

/// `IncompatibleBoundary` — a `ControlBoundary` assigns an envelope-reserved point to a non-code
/// owner (§2.5.5; §2.9.6; ADR-0103 I5/I6). Raised by `validate`/`open`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryError {
    /// The reserved point that was assigned to a non-code owner.
    pub point: DecisionPoint,
    /// The offending owner.
    pub owner: Owner,
}

/// `ControlBoundary{assignments, guards}` — a typed record on `AgentProcess.native` (§2.5.5;
/// §2.9.4). `guards` are predicates over ledger-derived state (represented here as opaque
/// predicate refs; their evaluation is Stage 2+).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ControlBoundary {
    /// The per-decision-point owner assignment (β itself).
    pub assignments: BTreeMap<DecisionPoint, Owner>,
    /// The per-decision-point guard predicate references.
    pub guards: BTreeMap<DecisionPoint, String>,
}

impl ControlBoundary {
    /// Validate the boundary (`validate`/`open`): every envelope-reserved point must be `code`,
    /// else [`BoundaryError`] (`IncompatibleBoundary`). Unassigned points default to `code`.
    pub fn validate(&self) -> Result<(), BoundaryError> {
        for (point, owner) in &self.assignments {
            if point.is_envelope_reserved_code() && *owner != Owner::Code {
                return Err(BoundaryError {
                    point: *point,
                    owner: *owner,
                });
            }
        }
        Ok(())
    }

    /// The effective owner of a point — the assignment, or `code` by default.
    pub fn owner_of(&self, point: DecisionPoint) -> Owner {
        self.assignments.get(&point).copied().unwrap_or(Owner::Code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_point_set_is_closed_at_ten() {
        assert_eq!(DecisionPoint::ALL.len(), 10);
    }

    #[test]
    fn a_wellformed_boundary_validates() {
        let mut b = ControlBoundary::default();
        b.assignments.insert(DecisionPoint::Plan, Owner::Model);
        b.assignments.insert(DecisionPoint::Act, Owner::Model);
        b.assignments.insert(DecisionPoint::Authorize, Owner::Code);
        assert!(b.validate().is_ok());
        // β is representable: an unassigned point defaults to code.
        assert_eq!(b.owner_of(DecisionPoint::Stop), Owner::Code);
    }

    #[test]
    fn authorize_is_always_code() {
        // §2.5.5: authorize is envelope-reserved; a model assignment is IncompatibleBoundary.
        let mut b = ControlBoundary::default();
        b.assignments.insert(DecisionPoint::Authorize, Owner::Model);
        assert_eq!(
            b.validate(),
            Err(BoundaryError {
                point: DecisionPoint::Authorize,
                owner: Owner::Model,
            })
        );
    }

    #[test]
    fn stop_and_retry_are_reserved_code() {
        assert!(DecisionPoint::Stop.is_envelope_reserved_code());
        assert!(DecisionPoint::Retry.is_envelope_reserved_code());
        let mut b = ControlBoundary::default();
        b.assignments.insert(DecisionPoint::Stop, Owner::Human);
        assert!(b.validate().is_err());
    }
}
