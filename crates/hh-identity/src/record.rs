//! [`Record`] and `identify` — the two-coordinate identity of an authored typed record (§8.3 #2).
//!
//! A record separates the **semantic core** (identity-bearing; both ids) from the **surface**
//! (name/display/provenance rendering; the `version_id` only — N6, CC8: a rename keeps
//! `semantic_id`, changes `version_id`). `identify` returns `version_id` always and `semantic_id`
//! only for kinds with a declared semantic projection. A selector or display name embedded as a
//! reference in a *sealed form* is refused with [`IdentifyError::UnresolvedRef`] (N5, CC3).

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::idp::{identify_bytes, idp_id};
use crate::kinds::RecordKind;
use crate::refs::NameSelector;

/// A reference a record embeds. Only a [`Reference::Pinned`] id is legal inside a sealed form; a
/// selector or a display name is an unpinned reference and is refused there (N5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    /// A pinned `version_id` / `ContentAddress` (`<algorithm>:<hex>`).
    Pinned(String),
    /// A name selector (a mutable pointer — never identity).
    Selector(NameSelector),
    /// A raw display name used as a reference (the "goose path hash" / `AgentInfo.version`
    /// failure mode).
    DisplayName(String),
}

impl Reference {
    fn is_pinned(&self) -> bool {
        matches!(self, Reference::Pinned(_))
    }
}

/// An authored typed record. `semantic` is the identity-bearing core; `surface` carries
/// rendering/naming fields that change the `version_id` but not the `semantic_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub kind: RecordKind,
    pub semantic: BTreeMap<String, Json>,
    pub surface: BTreeMap<String, Json>,
    pub refs: Vec<Reference>,
}

/// The result of [`identify`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub version_id: String,
    pub semantic_id: Option<String>,
}

/// `identify` failure modes (§8.3 #2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentifyError {
    /// A selector or display name appears inside a sealed/executed/published form (N5, CC3).
    UnresolvedRef { detail: String },
}

impl Record {
    /// A new record of `kind` with empty cores.
    pub fn new(kind: RecordKind) -> Record {
        Record {
            kind,
            semantic: BTreeMap::new(),
            surface: BTreeMap::new(),
            refs: Vec::new(),
        }
    }

    /// Add a field to the semantic core (identity-bearing — in both ids).
    pub fn semantic(mut self, key: &str, value: Json) -> Record {
        self.semantic.insert(key.to_string(), value);
        self
    }

    /// Add a surface field (rendering/naming — in `version_id` only).
    pub fn surface(mut self, key: &str, value: Json) -> Record {
        self.surface.insert(key.to_string(), value);
        self
    }

    /// Embed a reference.
    pub fn reference(mut self, r: Reference) -> Record {
        self.refs.push(r);
        self
    }

    /// The canonical JSON of the whole record (the `version_id` payload). Deterministic
    /// (sorted-key compact) — the one canonical form (CC1).
    pub fn canonical_full(&self) -> Json {
        Json::obj([
            ("kind", Json::str(self.kind.domain_tag())),
            ("semantic", map_to_json(&self.semantic)),
            ("surface", map_to_json(&self.surface)),
            ("refs", refs_to_json(&self.refs)),
        ])
    }

    /// The canonical JSON of the semantic core (the `semantic_id` payload). Surface fields and
    /// references are excluded (N6).
    pub fn canonical_semantic(&self) -> Json {
        Json::obj([
            ("kind", Json::str(self.kind.domain_tag())),
            ("semantic", map_to_json(&self.semantic)),
        ])
    }

    /// The canonical bytes of the whole record (used by `verify`).
    pub fn canonical_full_bytes(&self) -> Vec<u8> {
        self.canonical_full().to_canonical_string().into_bytes()
    }
}

/// `identify(record, kind) → {version_id, semantic_id?}` (§8.3 #2). Refuses a selector or display
/// name inside a sealed form (N5). `version_id = H(idp ∥ domain_tag(kind) ∥ canonical(record))`;
/// `semantic_id` is present only for kinds with a declared semantic projection.
pub fn identify(record: &Record) -> Result<Identity, IdentifyError> {
    if record.kind.is_sealed_form() {
        for r in &record.refs {
            if !r.is_pinned() {
                return Err(IdentifyError::UnresolvedRef {
                    detail: format!(
                        "unpinned reference {:?} inside sealed form {:?}",
                        r, record.kind
                    ),
                });
            }
        }
    }
    let version_id = identify_bytes(record.kind, &record.canonical_full_bytes());
    let semantic_id = if record.kind.has_semantic_projection() {
        Some(idp_id(
            &semantic_domain(record.kind),
            record.canonical_semantic().to_canonical_string().as_bytes(),
        ))
    } else {
        None
    };
    Ok(Identity {
        version_id,
        semantic_id,
    })
}

/// The domain tag for a kind's *semantic* projection — distinct from its version domain so a
/// record's `semantic_id` and `version_id` never collide even over equal semantic bytes (N3).
fn semantic_domain(kind: RecordKind) -> String {
    format!("{}#semantic", kind.domain_tag())
}

fn map_to_json(m: &BTreeMap<String, Json>) -> Json {
    Json::Obj(m.clone())
}

fn refs_to_json(refs: &[Reference]) -> Json {
    Json::Arr(
        refs.iter()
            .map(|r| match r {
                Reference::Pinned(id) => Json::obj([("pinned", Json::str(id.clone()))]),
                Reference::Selector(s) => Json::obj([(
                    "selector",
                    Json::obj([
                        ("namespace", Json::str(s.namespace.clone())),
                        ("name", Json::str(s.name.clone())),
                    ]),
                )]),
                Reference::DisplayName(n) => Json::obj([("display_name", Json::str(n.clone()))]),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A "compiled tool under a profile" whose semantic core is fixed and whose surface carries
    /// the (renamable) display name — the AC-3 subject.
    fn compiled_tool(display_name: &str) -> Record {
        Record::new(RecordKind::VariantRecord)
            .semantic("tool", Json::str("grep"))
            .semantic("profile", Json::str("profile/default"))
            .semantic("surface_hash", Json::str("sha256:fixed-surface"))
            .surface("display_name", Json::str(display_name))
    }

    #[test]
    fn rename_keeps_semantic_id_changes_version_id() {
        // AC-3 (T-LCD-10) / CC8: a rename keeps `semantic_id`, changes `version_id`.
        let a = identify(&compiled_tool("Search")).unwrap();
        let b = identify(&compiled_tool("Grep")).unwrap();
        assert_eq!(
            a.semantic_id, b.semantic_id,
            "semantic_id must be rename-stable"
        );
        assert_ne!(
            a.version_id, b.version_id,
            "version_id must change on rename"
        );
        assert!(a.semantic_id.is_some());
    }

    #[test]
    fn version_only_kind_has_no_semantic_id() {
        let r = Record::new(RecordKind::NameBindingRecord).surface("name", Json::str("x"));
        let id = identify(&r).unwrap();
        assert!(id.semantic_id.is_none());
    }

    #[test]
    fn sealed_form_with_selector_is_unresolved_ref() {
        // AC-4 / N5 / CC3: a selector inside a sealed definition fails identify.
        let sealed = Record::new(RecordKind::SealedDefinition)
            .semantic("body", Json::str("…"))
            .reference(Reference::Selector(NameSelector::new("local", "tool/grep")));
        assert!(matches!(
            identify(&sealed),
            Err(IdentifyError::UnresolvedRef { .. })
        ));
    }

    #[test]
    fn sealed_form_with_display_name_is_unresolved_ref() {
        // AC-4: a raw display name used as a reference (goose path-hash failure mode).
        for kind in [
            RecordKind::SealedDefinition,
            RecordKind::RunManifest,
            RecordKind::BundleManifest,
            RecordKind::RegistryRecord,
        ] {
            let r = Record::new(kind).reference(Reference::DisplayName("my-tool".into()));
            assert!(
                matches!(identify(&r), Err(IdentifyError::UnresolvedRef { .. })),
                "kind {kind:?} must refuse a display-name reference"
            );
        }
    }

    #[test]
    fn sealed_form_with_pinned_refs_identifies_ok() {
        let r = Record::new(RecordKind::RunManifest)
            .semantic("run", Json::str("r1"))
            .reference(Reference::Pinned(format!("sha256:{}", "a".repeat(64))));
        assert!(identify(&r).is_ok());
    }

    #[test]
    fn non_sealed_kind_may_carry_a_selector() {
        // A selector is legal outside a sealed form (it is a mutable pointer, not identity).
        let r = Record::new(RecordKind::CapabilityDeclaration)
            .reference(Reference::Selector(NameSelector::new("local", "x")));
        assert!(identify(&r).is_ok());
    }
}
