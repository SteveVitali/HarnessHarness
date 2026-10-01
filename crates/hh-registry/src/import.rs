//! `import_listing` / `refresh` — the `mcp_listing` registry ops
//! (§5d.1 §2; ADR-0088 D3; ticket S3.9, R-2.5.4⁰).
//!
//! `import_listing(server_ref, listing)` lifts a `hh-mcp-listing/1`
//! document into one `ToolCapabilityRecord` per wire tool:
//!
//! - A tool carrying the `dev.cognition/hir` `_meta` block (an own
//!   export) recovers `semantic_id`, the declared `effects` (full
//!   `EffectClass` vectors ride the carried set), `permission_class`,
//!   the provenance pointer and `budget_ref`; `scope_bindings`,
//!   `resources` and `observation_contract` lift to their honest
//!   *unknown* forms (never a fabricated claim). The **loss report
//!   lists exactly** `cost_model.measured_ref`, `execution_requirement`
//!   and `exposure_hint` — the three record members the carried set
//!   does not cover (AC-R-2.5.1-5).
//! - A foreign tool (no carried block) registers `unverified` on every
//!   declaration: `effects` is the `unknown_domain` honesty form (the
//!   closed `EFFECT_DOMAINS` set with no attribute vectors — every
//!   declaration unattributed), `scope_bindings` unknown, `resources`
//!   unknown, `annotations` preserved as `source.declared_claims`
//!   (claims — they never decide; T2). `capability_needs_quarantine`
//!   already lands such records `quarantined` (ADR-0088; CF-210).
//! - Foreign `_meta` keys are preserved verbatim under
//!   `source.ext_meta` (N4 — the edge's byte-preservation rule).
//!
//! `refresh(server_ref, listing)` re-lifts and diffs name-keyed: an
//! unchanged `listing_hash` is a no-op; a changed tool registers a new
//! version and publishes it `supersedes{reason: edit}` over the prior
//! version with a `surface_only | semantic` classification
//! (`semantic_id` — the semantic projection — unchanged ⇒ the change
//! touched only surface fields). A removed name keeps its registered
//! versions (addressable by identity — availability is the catalog
//! layer's, R-2.5.3⁰).
//!
//! The lifted `source` member is
//! `{kind: "mcp_listing", server_ref, tool_name, listing_hash,
//! carried, declared_claims, ext_meta}` — the `pin` over `listing_hash`
//! is the only path that raises authority (ADR-0088 D5).

use std::collections::{BTreeMap, BTreeSet};

use hh_hir::document::Node;
use hh_hir::kinds::{EffectClass, EffectDomain, EntityKind, ToolEffects, EFFECT_DOMAINS};
use hh_hir::leaves::Text;
use hh_hir::records::{KindRecord, Resources, ScopeBindings, ToolCapabilityRecord};
use hh_identity::refs::VersionedRef;
use hh_provenance::{Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;

use crate::errors::RegistryError;
use crate::identity;
use crate::records::{CapabilityRecord, RegistryRecord};
use crate::store::RegistryStore;

/// The `_meta` key the carried HIR block rides under — the wire
/// spelling `hh-mcp`'s artifact module owns (one document, two readers:
/// the served surface and this lift; the key is data, not a type —
/// the dependency-direction invariant is unaffected).
pub const HH_META_KEY: &str = "dev.cognition/hir";

/// The listing document schema this module consumes (the `hh-mcp`
/// `hh-mcp-listing/1` projection — a wire shape, kept as data).
pub const LISTING_SCHEMA: &str = "hh-mcp-listing/1";

/// The `import_listing` outcome — registered refs, the surface-name →
/// `(server_ref, semantic_id)` map (ADR-0097 D6 — names are never
/// parsed) and the loss report.
#[derive(Debug, Clone)]
pub struct ImportOutcome {
    /// The registered version refs (one per tool, in listing order).
    pub refs: Vec<VersionedRef>,
    /// `surface_name → (server_ref, semantic_id)` — the client's D6 map.
    pub name_map: BTreeMap<String, (String, String)>,
    /// Tools whose carried HIR block was recovered.
    pub recovered: Vec<String>,
    /// Foreign tools — registered `unverified`, `unknown_domain`.
    pub declared_unverified: Vec<String>,
    /// The record members the carried set does not cover — the exact
    /// AC-R-2.5.1-5 loss list.
    pub losses: Vec<String>,
    /// Foreign `_meta` keys preserved verbatim (`{tool, key, value}`).
    pub ext_meta: Vec<Json>,
    /// The whole-listing hash.
    pub listing_hash: String,
}

/// One `refresh` supersession — `{name, prev_version_id, version_id,
/// classification}`.
#[derive(Debug, Clone)]
pub struct RefreshedVersion {
    /// The surface name.
    pub name: String,
    /// The superseded version.
    pub prev_version_id: String,
    /// The new version.
    pub version_id: String,
    /// `surface_only` | `semantic` (`HirDiff.classification` at record
    /// granularity — the semantic projection decides).
    pub classification: String,
}

/// The `refresh` outcome.
#[derive(Debug, Clone)]
pub struct RefreshOutcome {
    /// Names unchanged by `listing_hash` (the no-op rule).
    pub unchanged: Vec<String>,
    /// New names (no prior version — plain `import_listing` halves).
    pub added: Vec<String>,
    /// Names absent from the new listing — the registered versions
    /// stay addressable; `unavailable` is the catalog's verdict.
    pub removed: Vec<String>,
    /// Superseded versions with their classification.
    pub superseded: Vec<RefreshedVersion>,
}

/// The published name for a lifted surface — `mcp/{server_ref}/{name}`
/// under `local` (server-scoped: two servers may both serve `search`).
fn publish_name(server_ref: &str, tool_name: &str) -> String {
    format!("mcp/{server_ref}/{tool_name}")
}

/// The import provenance every lifted node mints under —
/// `Origin::import("mcp", "hh-mcp-listing/1")` ⇒ `unverified` (P7;
/// V-E1-7's required class for lifted sources).
fn import_prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::import("mcp", "hh-mcp-listing/1"),
        PersistenceScope::Definition,
        seq,
    )
}

/// The `unknown_domain` honesty form — the closed domain set with no
/// attribute vectors ("effects = unknown"; both OQ-223 spellings land
/// on the most dangerous risk class — ADR-0031 §2, the S1.17 interim
/// rule).
fn unknown_domain_effects() -> ToolEffects {
    ToolEffects::Declared(
        EFFECT_DOMAINS
            .iter()
            .copied()
            .map(EffectClass::domain_only)
            .collect(),
    )
}

/// Parse the carried `effects` member — full `EffectClass` objects
/// (`{domain, attributes}`) or the legacy bare-domain-string spelling
/// (a bare string lifts `domain_only` — attributes honestly absent).
fn lift_effects(meta: Option<&Json>) -> ToolEffects {
    let Some(Json::Arr(members)) = meta.and_then(|m| m.get("effects")) else {
        return match meta {
            Some(_) => ToolEffects::Pure,
            None => unknown_domain_effects(),
        };
    };
    if members.is_empty() {
        return ToolEffects::Pure;
    }
    let mut set = BTreeSet::new();
    for m in members {
        let class = match m {
            Json::Str(domain) => EffectDomain::parse(domain)
                .map(EffectClass::domain_only)
                .unwrap_or_else(|_| EffectClass::domain_only(EffectDomain::Exec)),
            Json::Obj(_) => EffectClass::from_json(m, "effects")
                .unwrap_or_else(|_| EffectClass::domain_only(EffectDomain::Exec)),
            _ => continue,
        };
        set.insert(class);
    }
    if set.is_empty() {
        // A carried member of unparseable classes is not `pure` — the
        // honest fallback is the unknown form, never a silent claim.
        return unknown_domain_effects();
    }
    ToolEffects::Declared(set)
}

/// Lift one wire `Tool` into a `ToolCapabilityRecord` + node provenance
/// — the `(record, carried?)` pair `import_listing` registers.
fn lift_tool(server_ref: &str, wire: &Json, seq: u64) -> (ToolCapabilityRecord, bool) {
    let name = wire
        .get("name")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let meta = wire.get("_meta").cloned().unwrap_or_else(|| Json::obj([]));
    let hir = meta.get(HH_META_KEY).cloned();
    let carried = hir.is_some();
    let prov = import_prov(seq);
    // `purpose` is a `Text` leaf at `external` (lifted descriptions are
    // R-TEXT external — they render only into roles ≤ external).
    let purpose = Text::new(
        wire.get("description")
            .and_then(Json::as_str)
            .unwrap_or(&name),
        server_ref,
        prov.clone(),
    );
    // Foreign `_meta` keys — preserved verbatim, never interpreted.
    let mut ext_meta = BTreeMap::new();
    if let Json::Obj(m) = &meta {
        for (k, v) in m {
            if k != HH_META_KEY {
                ext_meta.insert(k.clone(), v.clone());
            }
        }
    }
    // `annotations` lift to `declared_claims` — recorded, never read
    // for a decision (T2/D5).
    let mut claims = BTreeMap::new();
    if let Some(a) = wire.get("annotations") {
        claims.insert("annotations".to_string(), a.clone());
    }
    let listing_hash =
        hh_identity::idp_id("mcp.listing.tool", wire.to_canonical_string().as_bytes());
    let mut source = vec![
        ("kind".to_string(), Json::str("mcp_listing")),
        ("server_ref".to_string(), Json::str(server_ref.to_string())),
        ("tool_name".to_string(), Json::str(name.clone())),
        ("listing_hash".to_string(), Json::str(listing_hash)),
        ("carried".to_string(), Json::Bool(carried)),
        ("declared_claims".to_string(), Json::Obj(claims)),
        ("ext_meta".to_string(), Json::Obj(ext_meta)),
    ];
    if let Some(h) = &hir {
        source.push(("hir".to_string(), h.clone()));
    }
    (
        ToolCapabilityRecord {
            purpose,
            input_schema: wire
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| Json::obj([])),
            output_schema: wire.get("outputSchema").cloned(),
            effects: lift_effects(hir.as_ref()),
            preconditions: vec![],
            scope_bindings: ScopeBindings::Unknown,
            resources: Resources::Unknown,
            observation_contract: Json::obj([
                ("error_classes", Json::Arr(vec![])),
                ("declared", Json::Bool(false)),
            ]),
            cost_model: None,
            execution_requirement: Json::obj([("environment_class", Json::str("mcp_server"))]),
            source: Json::Obj(source.into_iter().collect()),
            exposure_hint: Json::obj([]),
            postconditions: vec![],
            flow_contract: None,
            action_patterns: Vec::new(),
        },
        carried,
    )
}

/// The exact loss list — the record members `import_listing` cannot
/// recover because the minimum carried set does not cover them
/// (AC-R-2.5.1-5; everything else unrecovered lands in an honest
/// `unknown` form and is accounted under `unknown[]`, not `losses[]`).
const IMPORT_LOSSES: [&str; 3] = [
    "cost_model.measured_ref",
    "execution_requirement",
    "exposure_hint",
];

/// Parse the `hh-mcp-listing/1` document → `(server_ref, tools)`.
fn parse_listing(doc: &Json) -> Result<(String, Vec<Json>), RegistryError> {
    let schema = doc.get("schema").and_then(Json::as_str).unwrap_or("");
    if schema != LISTING_SCHEMA {
        return Err(RegistryError::SchemaViolation {
            path: "listing.schema".to_string(),
            detail: format!("`{schema}` — expected {LISTING_SCHEMA}"),
        });
    }
    let server_ref = doc
        .get("server_ref")
        .and_then(Json::as_str)
        .ok_or_else(|| RegistryError::SchemaViolation {
            path: "listing.server_ref".to_string(),
            detail: "missing".to_string(),
        })?
        .to_string();
    let tools = match doc.get("tools") {
        Some(Json::Arr(ts)) => ts
            .iter()
            .map(|e| e.get("tool").cloned().unwrap_or(Json::Null))
            .collect(),
        _ => {
            return Err(RegistryError::SchemaViolation {
                path: "listing.tools".to_string(),
                detail: "missing or not an array".to_string(),
            })
        }
    };
    Ok((server_ref, tools))
}

/// `import_listing(server_ref, listing) → Vec<VersionedRef>` — lift and
/// register every tool; each is also published under
/// `local/mcp/{server_ref}/{name}` so `refresh`'s `supersedes` edges
/// have a name line to hang from.
pub fn import_listing(
    store: &mut RegistryStore,
    listing: &Json,
    registrar: &ProvenanceRecord,
) -> Result<ImportOutcome, RegistryError> {
    let (server_ref, tools) = parse_listing(listing)?;
    let listing_hash = listing
        .get("listing_hash")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let mut out = ImportOutcome {
        refs: Vec::new(),
        name_map: BTreeMap::new(),
        recovered: Vec::new(),
        declared_unverified: Vec::new(),
        losses: IMPORT_LOSSES.iter().map(|s| s.to_string()).collect(),
        ext_meta: Vec::new(),
        listing_hash,
    };
    for (i, wire) in tools.iter().enumerate() {
        let name = wire
            .get("name")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let (rec, carried) = lift_tool(&server_ref, wire, i as u64);
        // Foreign `_meta` keys — accounted in the report.
        if let Some(Json::Obj(m)) = wire.get("_meta") {
            for (k, v) in m {
                if k != HH_META_KEY {
                    out.ext_meta.push(Json::obj([
                        ("tool", Json::str(name.clone())),
                        ("key", Json::str(k.clone())),
                        ("value", v.clone()),
                    ]));
                }
            }
        }
        let node = Node::new(
            EntityKind::ToolCapability,
            KindRecord::ToolCapability(rec),
            import_prov(i as u64),
        );
        let record = RegistryRecord::Capability(CapabilityRecord { node });
        let semantic_id = identity::semantic_id(&record).unwrap_or_default();
        let vref = store.register(record, registrar, None)?;
        // Publish under the stable source-scoped name so `refresh` can
        // supersede — the label stays `None` (labels are the caller's).
        store.publish(
            "local",
            &publish_name(&server_ref, &name),
            &vref.version_id,
            None,
            None,
            registrar,
        )?;
        if carried {
            out.recovered.push(name.clone());
        } else {
            out.declared_unverified.push(name.clone());
        }
        out.name_map.insert(name, (server_ref.clone(), semantic_id));
        out.refs.push(vref);
    }
    Ok(out)
}

/// The semantic coordinate *minus* `source` — `source` carries
/// `listing_hash`, `declared_claims` and `ext_meta` (wire-side metadata,
/// never decision inputs). A refresh that touches only those is
/// `surface_only`; one that moves the declared capability (purpose,
/// schemas, effects, scope, resources, …) is `semantic`. The store's
/// `semantic_id` itself keeps `source` — the pin coordinate must change
/// whenever the lifted body does.
fn semantic_id_excl_source(rec: &RegistryRecord) -> Option<String> {
    let RegistryRecord::Capability(c) = rec else {
        return identity::semantic_id(rec);
    };
    let mut node = c.node.clone();
    if let KindRecord::ToolCapability(t) = &mut node.semantic {
        t.source = Json::Null;
    }
    Some(hh_hir::identity::semantic_id(&node))
}

/// The `source` member's identity key — `(server_ref, tool_name)`.
fn source_key(rec: &ToolCapabilityRecord) -> Option<(String, String)> {
    let kind = rec.source.get("kind").and_then(Json::as_str)?;
    if kind != "mcp_listing" {
        return None;
    }
    Some((
        rec.source
            .get("server_ref")
            .and_then(Json::as_str)?
            .to_string(),
        rec.source
            .get("tool_name")
            .and_then(Json::as_str)?
            .to_string(),
    ))
}

/// `refresh(server_ref, listing)` — re-lift and diff name-keyed
/// against the store's current `mcp_listing` records for the source:
/// unchanged `listing_hash` ⇒ no-op; changed ⇒ `register` +
/// `publish{supersedes: edit}` with a `surface_only | semantic`
/// classification; absent ⇒ `added`; missing ⇒ `removed` (the versions
/// stay — addressable by identity).
pub fn refresh(
    store: &mut RegistryStore,
    listing: &Json,
    registrar: &ProvenanceRecord,
) -> Result<RefreshOutcome, RegistryError> {
    let (server_ref, tools) = parse_listing(listing)?;
    // The source's current registered records — `(name → (version_id,
    // listing_hash, semantic_id))`, head versions only (the latest
    // registration per name wins — a superseded ancestor never
    // competes).
    let mut current: BTreeMap<String, (String, String, String, u64)> = BTreeMap::new();
    let vids: Vec<String> = store.version_ids().cloned().collect();
    for vid in vids {
        let Some((env, rec)) = store.get(&vid) else {
            continue;
        };
        let RegistryRecord::Capability(c) = rec else {
            continue;
        };
        let KindRecord::ToolCapability(t) = &c.node.semantic else {
            continue;
        };
        let Some((sref, name)) = source_key(t) else {
            continue;
        };
        if sref != server_ref {
            continue;
        }
        let hash = t
            .source
            .get("listing_hash")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let sid = semantic_id_excl_source(rec).unwrap_or_default();
        match current.get(&name) {
            Some((_, _, _, seq)) if *seq >= env.registered_at => {}
            _ => {
                current.insert(name, (vid.clone(), hash, sid, env.registered_at));
            }
        }
    }
    let mut out = RefreshOutcome {
        unchanged: Vec::new(),
        added: Vec::new(),
        removed: Vec::new(),
        superseded: Vec::new(),
    };
    let mut seen = BTreeSet::new();
    for (i, wire) in tools.iter().enumerate() {
        let name = wire
            .get("name")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        seen.insert(name.clone());
        let (rec, _carried) = lift_tool(&server_ref, wire, i as u64);
        let hash = hh_identity::idp_id("mcp.listing.tool", wire.to_canonical_string().as_bytes());
        match current.get(&name) {
            Some((_, old_hash, _, _)) if *old_hash == hash => {
                out.unchanged.push(name);
            }
            prev => {
                let node = Node::new(
                    EntityKind::ToolCapability,
                    KindRecord::ToolCapability(rec),
                    import_prov(i as u64),
                );
                let record = RegistryRecord::Capability(CapabilityRecord { node });
                let new_sid = semantic_id_excl_source(&record).unwrap_or_default();
                let vref = store.register(record, registrar, None)?;
                match prev {
                    Some((prev_vid, _, prev_sid, _)) => {
                        // `supersedes{reason: edit}` — the publish edge.
                        store.publish(
                            "local",
                            &publish_name(&server_ref, &name),
                            &vref.version_id,
                            None,
                            Some(prev_vid.clone()),
                            registrar,
                        )?;
                        out.superseded.push(RefreshedVersion {
                            name,
                            prev_version_id: prev_vid.clone(),
                            version_id: vref.version_id.clone(),
                            classification: if new_sid == *prev_sid {
                                "surface_only".to_string()
                            } else {
                                "semantic".to_string()
                            },
                        });
                    }
                    None => {
                        store.publish(
                            "local",
                            &publish_name(&server_ref, &name),
                            &vref.version_id,
                            None,
                            None,
                            registrar,
                        )?;
                        out.added.push(name);
                    }
                }
            }
        }
    }
    for name in current.keys() {
        if !seen.contains(name) {
            out.removed.push(name.clone());
        }
    }
    out.unchanged.sort();
    out.added.sort();
    out.removed.sort();
    Ok(out)
}
