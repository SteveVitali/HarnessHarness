//! The **sameness ladder L0–L4** (§8.3 #3; ADR-0037 D3). "The same harness" is computed from a
//! classified `HirDiff` and lineage, **never from names or text similarity** (N4).
//!
//! At Stage 1 (no HIR yet — HIR/1 lands S1.4) the ladder's identity-only rungs are computable
//! directly from the ids: **L0** equal `version_id`, **L1** equal `semantic_id`, **L4** different
//! harness. The lineage-classification rungs **L2**/**L3** need a `HirDiff.classification`
//! (`authority_delta`/`budget_delta`/dialect change); this crate lands the ladder and computes
//! L2/L3 when a [`DiffClassification`] is supplied (the S1.4/Stage-2 wiring), so the ordering is
//! landed once (CC7).

use crate::refs::VersionedRef;

/// The five rungs (§8.3 #3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamenessLevel {
    /// Equal `version_id`.
    L0,
    /// Equal `semantic_id` (surface/ext/provenance-only diff — results pool by `configuration_id`).
    L1,
    /// Same lineage, compatible (adjacent points on one lineage; resumable; never pooled).
    L2,
    /// Same lineage, incompatible (widening/loosening/dialect change; human origin; never resumable).
    L3,
    /// Different harness.
    L4,
}

/// A monotone delta on authority or budget (§8.3 #3 sameness rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delta {
    None,
    Narrowing,
    Widening,
    Tightening,
    Loosening,
}

/// A classified `HirDiff` between two versions on one lineage (the L2/L3 discriminator). Supplied
/// by S1.4's `diff.classify`; landed here as the input shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffClassification {
    pub semantic_ops_nonempty: bool,
    pub authority_delta: Delta,
    pub budget_delta: Delta,
    pub dialect_change: bool,
}

/// The sameness verdict plus the evidence it was computed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sameness {
    pub level: SamenessLevel,
    /// Whether the verdict used a classified diff (L2/L3) or ids/lineage alone (L0/L1/L4).
    pub used_classification: bool,
}

/// `sameness(a, b)` (§8.3 #2/#3). Computed from ids and, for the lineage rungs, a classified diff
/// — never from names or text. `same_lineage` says the two share a lineage (a supersedes chain).
pub fn sameness(
    a: &VersionedRef,
    b: &VersionedRef,
    same_lineage: bool,
    class: Option<&DiffClassification>,
) -> Sameness {
    if a.version_id == b.version_id {
        return Sameness {
            level: SamenessLevel::L0,
            used_classification: false,
        };
    }
    if let (Some(sa), Some(sb)) = (&a.semantic_id, &b.semantic_id) {
        if sa == sb {
            return Sameness {
                level: SamenessLevel::L1,
                used_classification: false,
            };
        }
    }
    if same_lineage {
        if let Some(c) = class {
            // L3: incompatible change (widening authority, loosening budget, or dialect change).
            if c.authority_delta == Delta::Widening
                || c.budget_delta == Delta::Loosening
                || c.dialect_change
            {
                return Sameness {
                    level: SamenessLevel::L3,
                    used_classification: true,
                };
            }
            // L2: compatible, adjacent point on one lineage.
            if c.semantic_ops_nonempty
                && matches!(c.authority_delta, Delta::None | Delta::Narrowing)
                && matches!(c.budget_delta, Delta::None | Delta::Tightening)
            {
                return Sameness {
                    level: SamenessLevel::L2,
                    used_classification: true,
                };
            }
        }
    }
    Sameness {
        level: SamenessLevel::L4,
        used_classification: false,
    }
}

/// Whether two versions **pool by `configuration_id`** — true only at L1 (surface diff), the
/// T-LCD-10 rule (§8.3 #3). L2 adjacent points are never pooled.
pub fn pools_by_configuration_id(s: Sameness) -> bool {
    matches!(s.level, SamenessLevel::L0 | SamenessLevel::L1)
}

/// Whether an L2 successor is **resumable** per `verify_resume` (§8.3 #7). L0/L1/L2 are resumable;
/// L3/L4 are not.
pub fn is_resumable(s: Sameness) -> bool {
    matches!(
        s.level,
        SamenessLevel::L0 | SamenessLevel::L1 | SamenessLevel::L2
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::RecordKind;
    use crate::refs::Provenance;

    fn vr(vid: &str, sem: Option<&str>) -> VersionedRef {
        let mut r = VersionedRef::pinned(RecordKind::SealedDefinition, vid, Provenance::kernel());
        r.semantic_id = sem.map(|s| s.to_string());
        r
    }

    #[test]
    fn equal_version_id_is_l0() {
        let s = sameness(
            &vr("sha256:v", Some("sha256:s")),
            &vr("sha256:v", Some("sha256:s")),
            true,
            None,
        );
        assert_eq!(s.level, SamenessLevel::L0);
    }

    #[test]
    fn equal_semantic_id_is_l1_and_pools() {
        // AC-3: rename → equal semantic_id, different version_id → L1; pools by configuration_id.
        let s = sameness(
            &vr("sha256:v1", Some("sha256:s")),
            &vr("sha256:v2", Some("sha256:s")),
            true,
            None,
        );
        assert_eq!(s.level, SamenessLevel::L1);
        assert!(pools_by_configuration_id(s));
        assert!(is_resumable(s));
    }

    #[test]
    fn compatible_diff_on_a_lineage_is_l2_never_pooled() {
        let class = DiffClassification {
            semantic_ops_nonempty: true,
            authority_delta: Delta::Narrowing,
            budget_delta: Delta::Tightening,
            dialect_change: false,
        };
        let s = sameness(
            &vr("sha256:v1", Some("sha256:s1")),
            &vr("sha256:v2", Some("sha256:s2")),
            true,
            Some(&class),
        );
        assert_eq!(s.level, SamenessLevel::L2);
        assert!(!pools_by_configuration_id(s), "L2 is never pooled");
        assert!(is_resumable(s));
    }

    #[test]
    fn widening_diff_is_l3_not_resumable() {
        let class = DiffClassification {
            semantic_ops_nonempty: true,
            authority_delta: Delta::Widening,
            budget_delta: Delta::None,
            dialect_change: false,
        };
        let s = sameness(
            &vr("sha256:v1", Some("sha256:s1")),
            &vr("sha256:v2", Some("sha256:s2")),
            true,
            Some(&class),
        );
        assert_eq!(s.level, SamenessLevel::L3);
        assert!(!is_resumable(s));
    }

    #[test]
    fn unrelated_versions_are_l4() {
        let s = sameness(
            &vr("sha256:v1", Some("sha256:s1")),
            &vr("sha256:v2", Some("sha256:s2")),
            false,
            None,
        );
        assert_eq!(s.level, SamenessLevel::L4);
    }
}
