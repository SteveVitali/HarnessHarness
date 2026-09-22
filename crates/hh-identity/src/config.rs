//! The **two configuration ids** computed under `idp/1` (§8.3 #3; ADR-0036 D3 as amended).
//!
//! `configuration_id = H(idp ∥ "configuration" ∥ canonical{model_ref, harness_def.semantic_id,
//! profile.semantic_id, environment_ref, budget.semantic_id})` — the seedless **semantic**
//! coordinate results aggregate by. `configuration_version_id` is the same over `version_id`s plus
//! `seed` — the manifest's exact-bytes key rows are keyed by. Both funnel through the one
//! idp/1 construction (CC1); the ontology's κ (`hh-ontology::config`) delegates here so there is
//! exactly one configuration-id scheme in the tree.

use hh_wire::json::Json;

use crate::idp::idp_id;
use crate::kinds::RecordKind;

/// The seedless semantic coordinate (§8.3 #3). `model_ref` is the `semantic_id` of the sealed
/// `ModelRoleTable`; `environment_ref` is the `EnvironmentRecord.semantic_id`.
pub fn configuration_id(
    model_ref: &str,
    harness_semantic_id: &str,
    profile_semantic_id: &str,
    environment_ref: &str,
    budget_semantic_id: &str,
) -> String {
    let body = Json::obj([
        ("model_ref", Json::str(model_ref)),
        ("harness_def", Json::str(harness_semantic_id)),
        ("profile", Json::str(profile_semantic_id)),
        ("environment_ref", Json::str(environment_ref)),
        ("budget", Json::str(budget_semantic_id)),
    ]);
    idp_id(
        RecordKind::Configuration.domain_tag(),
        body.to_canonical_string().as_bytes(),
    )
}

/// The exact-bytes manifest key over `version_id`s plus `seed` (§8.3 #3). `environment_ref` here
/// is the `EnvironmentRecord.version_id`; `seed` is the replicate axis.
pub fn configuration_version_id(
    model_version_id: &str,
    harness_version_id: &str,
    profile_version_id: &str,
    environment_version_id: &str,
    budget_version_id: &str,
    seed: &str,
) -> String {
    let body = Json::obj([
        ("model_ref", Json::str(model_version_id)),
        ("harness_def", Json::str(harness_version_id)),
        ("profile", Json::str(profile_version_id)),
        ("environment_ref", Json::str(environment_version_id)),
        ("budget", Json::str(budget_version_id)),
        ("seed", Json::str(seed)),
    ]);
    idp_id(
        RecordKind::ConfigurationVersion.domain_tag(),
        body.to_canonical_string().as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_is_excluded_from_configuration_id_included_in_version_id() {
        // AC-12: two runs differing only by seed share configuration_id, differ in version id.
        let cid_a = configuration_id("m/sem", "h/sem", "p/sem", "e/sem", "b/sem");
        let cid_b = configuration_id("m/sem", "h/sem", "p/sem", "e/sem", "b/sem");
        assert_eq!(cid_a, cid_b);
        let cvid_a = configuration_version_id("m@v", "h@v", "p@v", "e@v", "b@v", "seed-0");
        let cvid_b = configuration_version_id("m@v", "h@v", "p@v", "e@v", "b@v", "seed-1");
        assert_ne!(cvid_a, cvid_b, "the seed is part of the version id");
    }

    #[test]
    fn model_role_table_change_alters_both_ids() {
        // AC-12: a ModelRoleTable change alters both.
        let cid = configuration_id("m1/sem", "h/sem", "p/sem", "e/sem", "b/sem");
        let cid2 = configuration_id("m2/sem", "h/sem", "p/sem", "e/sem", "b/sem");
        assert_ne!(cid, cid2);
        let cvid = configuration_version_id("m1@v", "h@v", "p@v", "e@v", "b@v", "s");
        let cvid2 = configuration_version_id("m2@v", "h@v", "p@v", "e@v", "b@v", "s");
        assert_ne!(cvid, cvid2);
    }

    #[test]
    fn configuration_ids_are_idp1_rendered() {
        assert!(configuration_id("m", "h", "p", "e", "b").starts_with("sha256:"));
        assert!(configuration_version_id("m", "h", "p", "e", "b", "s").starts_with("sha256:"));
    }

    #[test]
    fn configuration_id_and_version_id_never_collide() {
        // N3: distinct domain tags keep the two coordinates apart even with matching fields.
        let cid = configuration_id("m", "h", "p", "e", "b");
        let cvid = configuration_version_id("m", "h", "p", "e", "b", "");
        assert_ne!(cid, cvid);
    }
}
