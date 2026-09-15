//! Configuration κ, the factor space F, and the **two configuration ids** (spec §2.5.6/§2.5.7;
//! ADR-0012, ADR-0036 decision 3 / CF-083).
//!
//! κ = (M-set, h, p, E, B, seed) is one factorial point. Two identities exist:
//! - [`ConfigurationId`] — the *semantic* coordinate over `semantic_id`s, **seed excluded**;
//!   results *aggregate* by it;
//! - [`ConfigurationVersionId`] — the exact-bytes *manifest key* over `version_id`s, **seed
//!   included**; rows are *keyed* by it.
//!
//! AC-A2-5 (T-LCD-09): a configuration record has all six factors explicit and β is
//! representable as a factor; "does variant V help M₁ more than M₂?" is expressible against this
//! data model (a V×M interaction over the [`Factor`] space).

use hh_wire::sha256::sha256_hex;

/// A pinned reference: the semantic coordinate (`semantic_id`, seedless-stable) and the
/// exact-bytes `version_id` (ADR-0036). `configuration_id` reads the former; the version id the
/// latter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    /// The semantic identity — stable across renames/reversions (aggregation coordinate).
    pub semantic_id: String,
    /// The exact-bytes version identity (manifest key).
    pub version_id: String,
}

impl Ref {
    /// Convenience constructor.
    pub fn new(semantic_id: impl Into<String>, version_id: impl Into<String>) -> Ref {
        Ref {
            semantic_id: semantic_id.into(),
            version_id: version_id.into(),
        }
    }
}

/// The factor-space F axes (§2.5.7). β is one of them — *representable as a factor* (AC-A2-5).
/// The set is closed here; a new axis is a dialect change (§2.2.2 discipline).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Factor {
    /// Model snapshot (with roles).
    ModelSnapshot,
    /// Task distribution.
    TaskDistribution,
    /// Environment image + fault/perturbation profile.
    EnvironmentImage,
    /// Budget vector.
    BudgetVector,
    /// Context window.
    ContextWindow,
    /// Tool-surface target.
    ToolSurfaceTarget,
    /// The control boundary β — a component-level factor (native only).
    ControlBoundaryBeta,
    /// A component variant.
    ComponentVariant,
    /// A Model Profile.
    Profile,
    /// Hosting mechanism.
    HostingMechanism,
}

impl Factor {
    /// All factor-space axes (§2.5.7).
    pub const ALL: [Factor; 10] = [
        Factor::ModelSnapshot,
        Factor::TaskDistribution,
        Factor::EnvironmentImage,
        Factor::BudgetVector,
        Factor::ContextWindow,
        Factor::ToolSurfaceTarget,
        Factor::ControlBoundaryBeta,
        Factor::ComponentVariant,
        Factor::Profile,
        Factor::HostingMechanism,
    ];
}

/// The **configuration** κ = (M-set, h, p, E, B, seed) — one factorial point (§2.5.6). All six
/// factors are explicit fields (AC-A2-5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configuration {
    /// M-set: the sealed `ModelRoleTable` reference (its `semantic_id` is `configuration_id.model_ref`).
    pub m_set: Ref,
    /// h: the Harness Definition (HIR).
    pub h: Ref,
    /// p: the Model Profile.
    pub p: Ref,
    /// E: the environment (`EnvironmentRecord`).
    pub e: Ref,
    /// B: the budget vector (recorded by its identity for keying).
    pub b: Ref,
    /// seed: the replicate axis (harness RNG / sampling seed / image digest). Excluded from
    /// `configuration_id`, included in `configuration_version_id`.
    pub seed: String,
}

impl Configuration {
    /// The six factors, ordered (M-set, h, p, E, B, seed) — for the AC-A2-5 completeness check.
    pub fn six_factors(&self) -> [(&'static str, String); 6] {
        [
            ("m_set", self.m_set.semantic_id.clone()),
            ("h", self.h.semantic_id.clone()),
            ("p", self.p.semantic_id.clone()),
            ("e", self.e.semantic_id.clone()),
            ("b", self.b.semantic_id.clone()),
            ("seed", self.seed.clone()),
        ]
    }

    /// `configuration_id` — the **semantic** coordinate over `semantic_id`s, **seed excluded**
    /// (§2.5.6). Results *aggregate* by it; two configs differing only in seed share it.
    pub fn configuration_id(&self) -> ConfigurationId {
        // A canonical, order-fixed rendering of the seedless semantic coordinate.
        let body = format!(
            "model_ref={}\nh={}\np={}\nenvironment_ref={}\nb={}",
            self.m_set.semantic_id,
            self.h.semantic_id,
            self.p.semantic_id,
            self.e.semantic_id,
            self.b.semantic_id,
        );
        ConfigurationId(format!("cfg:sha256:{}", sha256_hex(body.as_bytes())))
    }

    /// The `model_ref` coordinate of `configuration_id` — the `ModelRoleTable`'s `semantic_id`
    /// (§2.5.6). The join coordinate for a V×M interaction query.
    pub fn model_ref(&self) -> &str {
        &self.m_set.semantic_id
    }

    /// `configuration_version_id` — the **exact-bytes manifest key** over `version_id`s, **seed
    /// included** (§2.5.6). Rows are *keyed* by it.
    pub fn configuration_version_id(&self) -> ConfigurationVersionId {
        let body = format!(
            "model_ref={}\nh={}\np={}\nenvironment_ref={}\nb={}\nseed={}",
            self.m_set.version_id,
            self.h.version_id,
            self.p.version_id,
            self.e.version_id,
            self.b.version_id,
            self.seed,
        );
        ConfigurationVersionId(format!("cfgv:sha256:{}", sha256_hex(body.as_bytes())))
    }
}

/// The seedless semantic coordinate results aggregate by (§2.5.6).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConfigurationId(pub String);

/// The exact-bytes manifest key rows are keyed by (§2.5.6).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConfigurationVersionId(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(model_sem: &str, variant_sem: &str, seed: &str) -> Configuration {
        Configuration {
            m_set: Ref::new(model_sem, format!("{model_sem}@v1")),
            // The component variant is carried inside the harness definition h; two variants
            // give two h semantic_ids.
            h: Ref::new(variant_sem, format!("{variant_sem}@v1")),
            p: Ref::new("profile/default", "profile/default@v1"),
            e: Ref::new("env/ubuntu", "env/ubuntu@sha-abc"),
            b: Ref::new("budget/std", "budget/std@v1"),
            seed: seed.to_string(),
        }
    }

    #[test]
    fn six_factors_are_explicit() {
        // AC-A2-5: all six factors explicit.
        let c = cfg("M1", "h/withV", "s0");
        let names: Vec<&str> = c.six_factors().iter().map(|(n, _)| *n).collect();
        assert_eq!(names, ["m_set", "h", "p", "e", "b", "seed"]);
    }

    #[test]
    fn beta_is_representable_as_a_factor() {
        // AC-A2-5: β is representable as a factor.
        assert!(Factor::ALL.contains(&Factor::ControlBoundaryBeta));
        assert_eq!(Factor::ALL.len(), 10);
    }

    #[test]
    fn configuration_id_excludes_seed_but_version_id_includes_it() {
        // §2.5.6: results aggregate by configuration_id (seedless); rows keyed by version id
        // (seeded). Two replicates share configuration_id, differ in version id.
        let a = cfg("M1", "h/withV", "s0");
        let b = cfg("M1", "h/withV", "s1");
        assert_eq!(a.configuration_id(), b.configuration_id());
        assert_ne!(a.configuration_version_id(), b.configuration_version_id());
    }

    #[test]
    fn variant_or_model_change_changes_configuration_id() {
        let base = cfg("M1", "h/withV", "s0");
        let other_model = cfg("M2", "h/withV", "s0");
        let other_variant = cfg("M1", "h/noV", "s0");
        assert_ne!(base.configuration_id(), other_model.configuration_id());
        assert_ne!(base.configuration_id(), other_variant.configuration_id());
    }

    #[test]
    fn v_helps_m1_more_than_m2_is_expressible() {
        // AC-A2-5 (T-LCD-09): the interaction "does variant V help M₁ more than M₂?" is
        // expressible against the data model — a 2×2 (variant × model) design whose four cells
        // are distinct configuration_ids, joinable on model_ref, with V represented as a factor.
        let cells = [
            cfg("M1", "h/withV", "s0"),
            cfg("M1", "h/noV", "s0"),
            cfg("M2", "h/withV", "s0"),
            cfg("M2", "h/noV", "s0"),
        ];
        let ids: std::collections::HashSet<_> =
            cells.iter().map(|c| c.configuration_id()).collect();
        assert_eq!(ids.len(), 4, "the four interaction cells must be distinct");
        // The two model levels are recoverable as a join coordinate.
        let models: std::collections::HashSet<_> = cells.iter().map(|c| c.model_ref()).collect();
        assert_eq!(models.len(), 2);
        // V is a bona fide factor axis and the component variant lives on the harness axis.
        assert!(Factor::ALL.contains(&Factor::ComponentVariant));
    }
}
