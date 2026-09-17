//! The DF-S1.12-3 seal seam — the `PermissionUnreachable` warnings a
//! definition's `EnvironmentRecord` + `Permission` grants produce *before* any
//! run (the seal-path half of AC-R-2.8.4-14; the attach-path half is
//! [`hh_containment::attach`]'s `SealWarning`). A grant whose domain can never
//! be admitted under the environment's policy is an advisory at seal — never a
//! refusal (the run's `floor_gate` refuses the *effect*, not the record).
//!
//! The environment handle's `containment` slot is a `PolicySlot`; the seal
//! check reads the effective policy (the slot's resolved policy) and the
//! grants the definition mints.

use hh_containment::admit::{unreachable_permissions, UnreachableGrant};
use hh_containment::attach::PolicySlot;
use hh_hir::records::Grant;

/// `seal_warnings(containment, grants)` — the `PermissionUnreachable`
/// advisories for a definition sealing an environment + its permission set.
/// Advisory only — the caller surfaces them on the `SealReport`, never
/// refuses the seal (a permission that is unreachable under *this* environment
/// may be reachable under a derived/remote one).
pub fn seal_warnings(
    containment: &PolicySlot,
    grants: &[(String, Grant)],
) -> Vec<UnreachableGrant> {
    unreachable_permissions(containment.policy(), grants)
}
