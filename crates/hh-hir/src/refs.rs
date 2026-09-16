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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileRef {
    /// The profile's identity coordinate.
    pub profile: String,
    /// Pinned (true) or selector (false).
    pub pinned: bool,
}

/// `ComponentVariantRef` (§3.1.3; §3.3 owns the record it points at). Never legal on a
/// `hosted` `AgentProcess` (T-LCD-15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentVariantRef {
    /// The variant record's identity coordinate.
    pub variant: String,
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
    fn ref_semantic_projection_is_semantic_id_only() {
        // §3.1.2: refs-by-semantic_id — pinning never changes semantic identity.
        let r = Ref::pinned("sha256:sem", "sha256:v1");
        let s = Ref::selected("sha256:sem", "latest");
        assert_eq!(r.semantic_json(), s.semantic_json());
        assert_ne!(r.to_json(), s.to_json());
    }
}
