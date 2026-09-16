//! `Ref` and the named reference types (§3.1.3, "Reference types"). `Ref` is the HIR
//! projection of the one `VersionedRef` (CF-110): `{semantic_id, version_id |
//! version_selector}` — the semantic id is always present; the version coordinate is pinned
//! or a selector. **Inside a sealed definition every `Ref` is pinned to a `version_id`** —
//! `seal` resolves each selector to the sealed target's `version_id` (§3.1.6); selectors in
//! the assembly section are §3.3's resolution domain (S1.9).

use crate::errors::HirError;
use hh_wire::json::Json;

/// `Ref = {semantic_id, version_id | version_selector}` (§3.1.3). The `semantic_id` is the
/// comparison coordinate and the in-document link: an edge or ref field resolves a target
/// node by it. The version coordinate is either a pinned `version_id` or a
/// `version_selector`; `seal` rewrites each selector to the sealed target's `version_id`
/// (`UnresolvedRef` when the `semantic_id` doesn't resolve).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ref {
    /// The target node's semantic id.
    pub semantic_id: String,
    /// The version coordinate.
    pub version: RefVersion,
}

/// The version coordinate of a [`Ref`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RefVersion {
    /// A pinned `version_id` (`<algorithm>:<hex>`).
    Pinned(String),
    /// An unpinned selector — legal only in the assembly section.
    Selector(String),
}

impl Ref {
    /// A ref to `semantic_id` with a selector version coordinate (unresolved).
    pub fn selected(semantic_id: impl Into<String>, selector: impl Into<String>) -> Ref {
        Ref {
            semantic_id: semantic_id.into(),
            version: RefVersion::Selector(selector.into()),
        }
    }

    /// A ref to `semantic_id` with a pinned `version_id`.
    pub fn pinned(semantic_id: impl Into<String>, version_id: impl Into<String>) -> Ref {
        Ref {
            semantic_id: semantic_id.into(),
            version: RefVersion::Pinned(version_id.into()),
        }
    }

    /// Whether the version coordinate is pinned.
    pub fn is_pinned(&self) -> bool {
        matches!(self.version, RefVersion::Pinned(_))
    }

    /// The canonical JSON. In a semantic projection refs contribute **by semantic_id only**
    /// (§3.1.2 `refs-by-semantic_id`): use [`Ref::semantic_json`] there.
    pub fn to_json(&self) -> Json {
        match &self.version {
            RefVersion::Pinned(v) => Json::obj([
                ("semantic_id", Json::str(self.semantic_id.clone())),
                ("version_id", Json::str(v.clone())),
            ]),
            RefVersion::Selector(s) => Json::obj([
                ("semantic_id", Json::str(self.semantic_id.clone())),
                ("version_selector", Json::str(s.clone())),
            ]),
        }
    }

    /// The semantic-projection form: the semantic id only (pinning never changes
    /// `semantic_id` — §3.1.2).
    pub fn semantic_json(&self) -> Json {
        Json::obj([("semantic_id", Json::str(self.semantic_id.clone()))])
    }

    /// Parse a ref from its canonical form.
    pub fn from_json(j: &Json, path: &str) -> Result<Ref, HirError> {
        let semantic_id = j
            .get("semantic_id")
            .and_then(Json::as_str)
            .ok_or_else(|| HirError::SchemaViolation {
                detail: format!("{path}.semantic_id missing"),
            })?
            .to_string();
        let version = match (
            j.get("version_id").and_then(Json::as_str),
            j.get("version_selector").and_then(Json::as_str),
        ) {
            (Some(v), None) => RefVersion::Pinned(v.to_string()),
            (None, Some(s)) => RefVersion::Selector(s.to_string()),
            _ => {
                return Err(HirError::SchemaViolation {
                    detail: format!("{path} must carry exactly one of version_id|version_selector"),
                })
            }
        };
        Ok(Ref {
            semantic_id,
            version,
        })
    }
}

/// `ProfileRef` — a reference to a Model Profile version (§3.1.3; the **only** kernel field
/// typed by model identity — T-LCD-01). Carried as an identity coordinate string (the
/// profile's `version_id` or a selector in the assembly section).
///
/// **The unbound sentinel (§3.3.2, T-LCD-04).** A definition's profile is a *configuration
/// coordinate*, `unbound` by default; the compiler's `link` binds it (§3.2 stage 1). A sealed
/// root therefore carries [`ProfileRef::unbound`] — `profile = "unbound"`, `pinned = false` —
/// which `seal` admits as the one unpinned profile form (S1.9; ADR-0240). A pinned profile in
/// the sealed root is the `profile_binding = ProfileRef` case (non-portable, OQ-078).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileRef {
    /// The profile's identity coordinate.
    pub profile: String,
    /// Pinned (true) or selector (false).
    pub pinned: bool,
}

/// The `profile` spelling of the unbound sentinel (§3.3.2 `profile_binding = unbound`).
pub const PROFILE_UNBOUND: &str = "unbound";

impl ProfileRef {
    /// The unbound profile — the definition constrains or pins no profile; `link` binds one.
    pub fn unbound() -> ProfileRef {
        ProfileRef {
            profile: PROFILE_UNBOUND.into(),
            pinned: false,
        }
    }

    /// Whether this is the unbound sentinel.
    pub fn is_unbound(&self) -> bool {
        !self.pinned && self.profile == PROFILE_UNBOUND
    }
}

/// `ComponentVariantRef{class_id, variant_id, version_selector | version_id}` (§3.3.2 —
/// the §3.3 record shape, carried on `AgentProcess.native.slots`; §3.1.3 names the type).
/// Never legal on a `hosted` `AgentProcess` (T-LCD-15). The authored form carries a
/// selector; a sealed form carries a pinned `version_id` (§3.3.2). Variant tags are
/// **open, registry-validated**: an unregistered `variant_id` is `UnresolvedRef` at
/// `resolve`, never silently accepted (ADR-0023 decision 2).
///
/// Semantic projection: `{class_id, variant_id}` — the registry-validated *name*
/// coordinate; the version coordinate is the pin (refs-by-semantic_id, §3.1.2), so a
/// re-pin under one name changes `version_id` only (ADR-0240).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentVariantRef {
    /// The component class the variant implements (the `slots` key).
    pub class_id: String,
    /// The open, registry-validated variant tag (`<namespace>/<name>` at Stage 1).
    pub variant_id: String,
    /// The version coordinate — a selector (authored) or a pinned `version_id` (sealed).
    pub version: RefVersion,
}

impl ComponentVariantRef {
    /// An authored (selector) variant ref.
    pub fn selected(
        class_id: impl Into<String>,
        variant_id: impl Into<String>,
        selector: impl Into<String>,
    ) -> ComponentVariantRef {
        ComponentVariantRef {
            class_id: class_id.into(),
            variant_id: variant_id.into(),
            version: RefVersion::Selector(selector.into()),
        }
    }

    /// A pinned variant ref.
    pub fn pinned(
        class_id: impl Into<String>,
        variant_id: impl Into<String>,
        version_id: impl Into<String>,
    ) -> ComponentVariantRef {
        ComponentVariantRef {
            class_id: class_id.into(),
            variant_id: variant_id.into(),
            version: RefVersion::Pinned(version_id.into()),
        }
    }

    /// Whether the version coordinate is pinned.
    pub fn is_pinned(&self) -> bool {
        matches!(self.version, RefVersion::Pinned(_))
    }

    /// The canonical JSON — `{class_id, variant_id, version_id | version_selector}`.
    pub fn to_json(&self) -> Json {
        let version = match &self.version {
            RefVersion::Pinned(v) => ("version_id", Json::str(v.clone())),
            RefVersion::Selector(s) => ("version_selector", Json::str(s.clone())),
        };
        Json::obj([
            ("class_id", Json::str(self.class_id.clone())),
            ("variant_id", Json::str(self.variant_id.clone())),
            version,
        ])
    }

    /// The semantic-projection form — `{class_id, variant_id}` (the pin is excluded).
    pub fn semantic_json(&self) -> Json {
        Json::obj([
            ("class_id", Json::str(self.class_id.clone())),
            ("variant_id", Json::str(self.variant_id.clone())),
        ])
    }

    /// Parse from the canonical form.
    pub fn from_json(j: &Json, path: &str) -> Result<ComponentVariantRef, HirError> {
        let field = |k: &str| {
            j.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| HirError::SchemaViolation {
                    detail: format!("{path}.{k} missing"),
                })
        };
        let version = match (
            j.get("version_id").and_then(Json::as_str),
            j.get("version_selector").and_then(Json::as_str),
        ) {
            (Some(v), None) => RefVersion::Pinned(v.to_string()),
            (None, Some(s)) => RefVersion::Selector(s.to_string()),
            _ => {
                return Err(HirError::SchemaViolation {
                    detail: format!("{path} must carry exactly one of version_id|version_selector"),
                })
            }
        };
        if let Json::Obj(m) = j {
            for k in m.keys() {
                if !matches!(
                    k.as_str(),
                    "class_id" | "variant_id" | "version_id" | "version_selector"
                ) {
                    return Err(HirError::SchemaViolation {
                        detail: format!("{path}.{k}: unknown ComponentVariantRef member"),
                    });
                }
            }
        }
        Ok(ComponentVariantRef {
            class_id: field("class_id")?,
            variant_id: field("variant_id")?,
            version,
        })
    }
}

/// `EnvironmentRef` (§3.1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentRef {
    /// The environment declaration's identity coordinate.
    pub environment: String,
}

/// `RunRef` (§3.1.3) — a run identity coordinate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRef {
    /// The run's identity coordinate.
    pub run: String,
}

pub(crate) fn refs_from_json(j: &Json, path: &str) -> Result<Vec<Ref>, HirError> {
    match j {
        Json::Arr(items) => items
            .iter()
            .enumerate()
            .map(|(i, r)| Ref::from_json(r, &format!("{path}[{i}]")))
            .collect(),
        _ => Err(HirError::SchemaViolation {
            detail: format!("{path} must be an array of refs"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_variant_ref_round_trips_and_projects_by_name() {
        // §3.3.2: the authored form carries a selector, the sealed form a version_id; the
        // semantic projection is the {class_id, variant_id} name coordinate (ADR-0240).
        let a = ComponentVariantRef::selected("control_strategy", "hh/cs/react", "latest");
        let p = ComponentVariantRef::pinned("control_strategy", "hh/cs/react", "sha256:v1");
        assert_eq!(a.semantic_json(), p.semantic_json());
        assert_ne!(a.to_json(), p.to_json());
        assert_eq!(
            ComponentVariantRef::from_json(&p.to_json(), "$").unwrap(),
            p
        );
        assert_eq!(
            ComponentVariantRef::from_json(&a.to_json(), "$").unwrap(),
            a
        );
        let mut bad = p.to_json();
        if let Json::Obj(m) = &mut bad {
            m.insert("extra".into(), Json::Null);
        }
        assert!(ComponentVariantRef::from_json(&bad, "$").is_err());
        assert!(ProfileRef::unbound().is_unbound());
    }

    #[test]
    fn ref_semantic_projection_is_semantic_id_only() {
        // §3.1.2: refs-by-semantic_id — pinning never changes semantic identity.
        let r = Ref::pinned("sha256:sem", "sha256:v1");
        let s = Ref::selected("sha256:sem", "latest");
        assert_eq!(r.semantic_json(), s.semantic_json());
        assert_ne!(r.to_json(), s.to_json());
    }
}
