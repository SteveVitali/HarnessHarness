//! `identity` (§3.3.4): `identity(sealed) → {semantic_id, version_id}` — the sealed
//! definition's root coordinates — plus the **configuration-id** composition through
//! `hh-identity` (CC1 — the one `idp/1` scheme; §8.3 #3). `configuration_id` excludes
//! surfaces, `layers` and `seed`; `configuration_version_id` carries the exact
//! `version_id`s plus `seed` (the manifest's exact-bytes key).

use hh_hir::document::{DefinitionVersionRef, SealedDefinition};
use hh_wire::json::Json;

/// `identity(sealed)` — the definition's `{semantic_id, version_id}` (the root's — the
/// provenance-recorded coordinates, §3.3.6).
pub fn identity(sealed: &SealedDefinition) -> DefinitionVersionRef {
    sealed.definition_ref.clone()
}

/// The κ coordinates a configuration id is computed over (§8.3 #3 / ADR-0036 D3):
/// `model_ref` is the sealed `ModelRoleTable`'s id; `profile` the bound profile;
/// `environment_ref` the `EnvironmentRecord`; `budget` the budget record.
pub struct CompositionInputs {
    /// The `ModelRoleTable` id (`semantic_id` for `configuration_id`, `version_id` for
    /// `configuration_version_id`).
    pub model_ref: String,
    /// The bound profile's id.
    pub profile: String,
    /// The `EnvironmentRecord` id.
    pub environment_ref: String,
    /// The budget record's id.
    pub budget: String,
    /// The seed (the replicate axis — `configuration_version_id` only).
    pub seed: String,
}

/// The two configuration ids (§8.3 #3): `configuration_id` over `semantic_id`s (seed
/// excluded — results aggregate by it) and `configuration_version_id` over
/// `version_id`s plus the seed (rows keyed by it).
pub struct ConfigurationIds {
    /// `H(idp ∥ "configuration" ∥ canonical{model_ref, harness_def.semantic_id,
    /// profile.semantic_id, environment_ref, budget.semantic_id})`.
    pub configuration_id: String,
    /// The same over `version_id`s plus `seed`.
    pub configuration_version_id: String,
}

/// `configuration(sealed, inputs)` — the composition through `hh-identity` (CC1).
pub fn configuration(sealed: &SealedDefinition, inputs: &CompositionInputs) -> ConfigurationIds {
    let def = identity(sealed);
    ConfigurationIds {
        configuration_id: hh_identity::config::configuration_id(
            &inputs.model_ref,
            &def.semantic_id,
            &inputs.profile,
            &inputs.environment_ref,
            &inputs.budget,
        ),
        configuration_version_id: hh_identity::config::configuration_version_id(
            &inputs.model_ref,
            &def.version_id,
            &inputs.profile,
            &inputs.environment_ref,
            &inputs.budget,
            &inputs.seed,
        ),
    }
}

/// The identity-bearing JSON of the definition's resolved assembly — the `slots` pins
/// as they contribute to `version_id` (the semantic coordinate lives on the root;
/// `layers`, `resolved`, and `ext` are recorded-not-identified, §3.3.6).
pub fn assembly_identity_view(a: &crate::grammar::Assembly) -> Json {
    Json::obj([
        ("dialect", Json::str(&a.dialect)),
        ("slots", hh_hir::wire::slots_json(&a.slots, false)),
        ("values", Json::Obj(a.values.clone())),
    ])
}
