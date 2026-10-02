//! `supersede_bundle` / `migrate_bundle` / `lineage` — the lifecycle-by-
//! supersession ops (§5h.3 §2; ADR-0141 D3; AC-R-2.9.3-11/-13). A bundle
//! is never mutated in place: supersession mints a *new* manifest whose
//! `composition.derived_from` carries `{bundle_id, reason}`; the old
//! manifest and members stay byte-identical (immutability I1).

use hh_wire::json::Json;

use crate::codec::Decoded;
use crate::error::BundleError;
use crate::manifest::BundleManifest;

/// The closed `supersede_bundle` reason set (§5h.3 §2).
pub const SUPERSEDE_REASONS: [&str; 5] =
    ["regrade", "repin", "redaction", "migration", "correction"];

/// The known schema spellings `migrate_bundle` accepts as a *source*
/// (`hh-bundle/0` is the pre-`participant_class`/`composition`
/// experimental spelling; decode defaults fill the additive members).
pub const LEGACY_SCHEMAS: [&str; 1] = ["hh-bundle/0"];

/// `supersede_bundle(new, old, reason)` — stamp the `derived-from` edge
/// on the successor manifest and recompute its `version_id` (the caller
/// owns assembling `new`; this refuses an unknown reason and never
/// touches `old`).
pub fn supersede_bundle(
    new: &mut BundleManifest,
    old: &BundleManifest,
    reason: &str,
) -> Result<(), BundleError> {
    if !SUPERSEDE_REASONS.contains(&reason) {
        return Err(BundleError::Malformed {
            detail: format!("supersede reason `{reason}` ∉ {SUPERSEDE_REASONS:?}"),
        });
    }
    if old.version_id.is_empty() {
        return Err(BundleError::Malformed {
            detail: "supersede: the old bundle has no version_id".into(),
        });
    }
    let mut comp = match new.composition.clone() {
        Json::Obj(m) => m,
        _ => std::collections::BTreeMap::new(),
    };
    comp.insert(
        "derived_from".into(),
        Json::obj([
            ("bundle_id", Json::str(old.version_id.clone())),
            ("reason", Json::str(reason)),
        ]),
    );
    new.composition = Json::Obj(comp);
    new.version_id = String::new();
    new.version_id = new.compute_id();
    Ok(())
}

/// `lineage(bundle_id, manifests)` — the supersession chain edges from a
/// set of manifests (`{bundle_id, derived_from}` per manifest whose
/// composition names a predecessor). Pure over the manifests supplied;
/// the caller (the results store's status book) resolves which manifests
/// exist.
pub fn lineage_edges(manifests: &[BundleManifest]) -> Vec<Json> {
    let mut edges: Vec<Json> = manifests
        .iter()
        .filter_map(|m| {
            m.composition
                .get("derived_from")
                .and_then(|d| d.get("bundle_id"))
                .and_then(Json::as_str)
                .map(|old| {
                    Json::obj([
                        ("bundle_id", Json::str(m.version_id.clone())),
                        ("derived_from", Json::str(old)),
                        (
                            "reason",
                            m.composition
                                .get("derived_from")
                                .and_then(|d| d.get("reason"))
                                .cloned()
                                .unwrap_or(Json::Null),
                        ),
                    ])
                })
        })
        .collect();
    edges.sort_by(|a, b| {
        a.get("bundle_id")
            .and_then(Json::as_str)
            .cmp(&b.get("bundle_id").and_then(Json::as_str))
    });
    edges
}

/// `migrate_bundle(decoded, to_schema)` — the migration-as-supersession
/// path (§5h.3 §2; AC-R-2.9.3-13). The source manifest is never
/// rewritten: the migrated manifest decodes the source canonical doc
/// (additive members default — `hh-bundle/0` fixtures decode under
/// `hh-bundle/1`), carries `composition.derived_from{migration}`, and
/// recomputes `version_id`. `to_schema` other than `hh-bundle/1` is
/// `FormatUnknown`.
pub fn migrate_bundle(
    decoded: &Decoded,
    to_schema: &str,
    producer: Json,
    created_at: String,
) -> Result<Decoded, BundleError> {
    if to_schema != crate::manifest::BUNDLE_SCHEMA {
        return Err(BundleError::FormatUnknown {
            detail: format!(
                "to_schema `{to_schema}` — only {} exists",
                crate::manifest::BUNDLE_SCHEMA
            ),
        });
    }
    // Decode the *bytes* again — the migration path is exercised on the
    // canonical document, not the in-memory struct (older fixtures that
    // predate additive members decode with defaults — AC-13's "load
    // under every later schema" obligation).
    let doc = decoded.manifest.to_json();
    let mut manifest = BundleManifest::from_json(&doc)?;
    manifest.producer = producer;
    manifest.created_at = created_at;
    supersede_bundle(&mut manifest, &decoded.manifest, "migration")?;
    Ok(Decoded {
        manifest,
        members: decoded.members.clone(),
    })
}

/// `migrate_bundle` over a *foreign-schema* manifest document — the
/// `hh-bundle/0` fixture path: the schema string is lifted to
/// `hh-bundle/1` and the additive members default (the document shape
/// is otherwise identical). Anything else is `FormatUnknown`.
pub fn migrate_document(
    doc: &Json,
    producer: Json,
    created_at: String,
) -> Result<Decoded, BundleError> {
    let schema = doc.get("schema").and_then(Json::as_str).unwrap_or("");
    if schema == crate::manifest::BUNDLE_SCHEMA {
        let manifest = BundleManifest::from_json(doc)?;
        let decoded = Decoded {
            manifest,
            members: Default::default(),
        };
        return migrate_bundle(
            &decoded,
            crate::manifest::BUNDLE_SCHEMA,
            producer,
            created_at,
        );
    }
    if !LEGACY_SCHEMAS.contains(&schema) {
        return Err(BundleError::FormatUnknown {
            detail: format!("schema `{schema}` has no lifter"),
        });
    }
    let mut lifted = match doc.clone() {
        Json::Obj(m) => m,
        _ => {
            return Err(BundleError::Malformed {
                detail: "legacy manifest is not an object".into(),
            })
        }
    };
    lifted.insert("schema".into(), Json::str(crate::manifest::BUNDLE_SCHEMA));
    let manifest = BundleManifest::from_json(&Json::Obj(lifted))?;
    let decoded = Decoded {
        manifest,
        members: Default::default(),
    };
    migrate_bundle(
        &decoded,
        crate::manifest::BUNDLE_SCHEMA,
        producer,
        created_at,
    )
}
