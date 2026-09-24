//! The capability projections the §5d.1 verbs serve (ADR-0088 D6; ADR-0089
//! D1/D2; S1.17): `project_risk`, `required_grants`, `estimate`, `catalog` and
//! `search_projection` over registered `CapabilityRecord`s. All are **derived,
//! never stored** — pure functions of the record (and the snapshot for
//! `catalog`), deterministic given a snapshot (AC-E1-7).
//!
//! - `project_risk` is the CF-104 projection — re-exported from
//!   [`hh_hir::risk::capability_risk`] (one implementation, CC7);
//!   `unverified` ⇒ the most dangerous class (ADR-0031 §2).
//! - `required_grants` is the V-EFF view `[{domain, scope_kind, param_path?}]`
//!   the monitor's coverage check reads — never stored.
//! - `estimate` returns `CostEstimate` over the closed `DimensionId` registry;
//!   undeclared dimensions are `unknown`, **never 0** (AC-R-2.5.1-9); the
//!   advisory `cost_model` is never budget truth (the structural invariant —
//!   `control.budget.consumed` and `MatchSpec` never read it).
//! - `catalog`/`search_projection` are the discovery projections R-2.5.3 reads
//!   (landed early because AC-R-2.5.1-7's snapshot determinism names them).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_hir::kinds::{EffectDomain, ToolEffects, World};
use hh_hir::records::ToolCapabilityRecord;
use hh_hir::tools::ScopeKind;
use hh_ontology::dimensions::DimensionId;
use hh_ontology::risk::RiskClass;
use hh_wire::json::Json;

use crate::errors::RegistryError;
use crate::records::{CapabilityRecord, RegistryRecord, RegistrySnapshot};
use crate::store::RegistryStore;

/// Fetch a registered capability record by `version_id` — `Unresolved` for a
/// missing id, `KindMismatch` when the id names a non-capability record.
pub fn capability_at<'a>(
    store: &'a RegistryStore,
    version_id: &str,
) -> Result<(&'a CapabilityRecord, &'a hh_provenance::ProvenanceRecord), RegistryError> {
    match store.get(version_id) {
        Some((env, RegistryRecord::Capability(c))) => Ok((c, &env.registrar)),
        Some(_) => Err(RegistryError::KindMismatch {
            detail: format!("{version_id} is not a capability record"),
        }),
        None => Err(RegistryError::Unresolved {
            detail: format!("unregistered capability {version_id}"),
        }),
    }
}

/// `project_risk(version_id)` — the CF-104/ADR-0031 projection: `unverified`
/// (lifted) ⇒ `{irreversible, non_idempotent, external}`; otherwise the
/// `max_by_danger` fold over declared effect attributes; `pure` ⇒ read-only.
/// The record's *node* provenance carries the declaration authority.
pub fn project_risk(store: &RegistryStore, version_id: &str) -> Result<RiskClass, RegistryError> {
    let (c, _registrar) = capability_at(store, version_id)?;
    Ok(hh_hir::risk::capability_risk(
        capability_semantic(c)?,
        c.node.provenance.authority,
    ))
}

/// The record's `ToolCapability` semantic member.
fn capability_semantic(c: &CapabilityRecord) -> Result<&ToolCapabilityRecord, RegistryError> {
    match &c.node.semantic {
        hh_hir::records::KindRecord::ToolCapability(t) => Ok(t),
        _ => Err(RegistryError::SchemaViolation {
            path: "semantic".to_string(),
            detail: "capability record's semantic member is not a tool_capability".to_string(),
        }),
    }
}

/// One row of the `required_grants` V-EFF view — `{domain, scope_kind?,
/// param_path?}` (§5d.1 §2; never stored).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredGrant {
    /// The declared effect domain.
    pub domain: EffectDomain,
    /// The scope kind the domain requires, when scope-bearing.
    pub scope_kind: Option<ScopeKind>,
    /// The binding's `param_path`, when a `scope_bindings` row selects it.
    pub param_path: Option<String>,
}

/// `required_grants(version_id)` — `[{domain, scope_kind, param_path?}]` per
/// declared effect (the V-EFF view the monitor's coverage check reads;
/// `scope_bindings_unknown` yields the domains with `param_path = None` —
/// honest, matching `*`-only coverage at run time).
pub fn required_grants(
    store: &RegistryStore,
    version_id: &str,
) -> Result<Vec<RequiredGrant>, RegistryError> {
    let (c, _r) = capability_at(store, version_id)?;
    let t = capability_semantic(c)?;
    let bindings = hh_hir::tools::scope_bindings(&t.scope_bindings)
        .ok()
        .flatten()
        .unwrap_or_default();
    let mut out = Vec::new();
    if let ToolEffects::Declared(set) = &t.effects {
        for e in set {
            let scope_kind = hh_hir::tools::domain_scope_kind(e.domain);
            let param_path = scope_kind.and_then(|k| {
                bindings
                    .iter()
                    .find(|b| b.scope_kind == k)
                    .map(|b| b.param_path.clone())
            });
            out.push(RequiredGrant {
                domain: e.domain,
                scope_kind,
                param_path,
            });
        }
    }
    Ok(out)
}

/// The `estimate` basis sum — `{declared, measured, none}` (ADR-0089 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstimateBasis {
    /// The estimate came from the declared `cost_model`.
    Declared,
    /// The estimate is backed by a `measured_ref` (the measurement itself is a
    /// view — the basis records its existence, never a value read off it).
    Measured,
    /// Undeclared — `value` is `unknown`.
    None,
}

/// One dimension's estimate — `value: None` is `unknown` (never 0).
#[derive(Debug, Clone, PartialEq)]
pub struct DimensionEstimate {
    /// The declared estimate value (`constant`/`value` member), or `unknown`.
    pub value: Option<Json>,
    /// The declared `confidence` (`exact` requires `measured_ref` — V-E1-8).
    pub confidence: Option<String>,
    /// Where the estimate came from.
    pub basis: EstimateBasis,
}

/// `CostEstimate` — `per_dimension` over the closed `DimensionId` registry;
/// every member is present, undeclared ones `unknown` (AC-R-2.5.1-9).
#[derive(Debug, Clone, PartialEq)]
pub struct CostEstimate {
    /// `DimensionId → estimate` — all 32 registered dimensions present.
    pub per_dimension: BTreeMap<DimensionId, DimensionEstimate>,
}

/// `estimate(version_id, canonical_args)` — at C0 the declared `cost_model`
/// members are reported verbatim per dimension with `basis = declared` (or
/// `measured` when the record carries `measured_ref` and the entry names it);
/// every undeclared dimension is `unknown`, never 0. `canonical_args` is
/// accepted for the signature's stability (per-unit/distribution evaluation is
/// a later-stage view — the declared shape is reported, not evaluated).
pub fn estimate(
    store: &RegistryStore,
    version_id: &str,
    _canonical_args: Option<&Json>,
) -> Result<CostEstimate, RegistryError> {
    let (c, _r) = capability_at(store, version_id)?;
    let t = capability_semantic(c)?;
    let has_measured = t
        .cost_model
        .as_ref()
        .and_then(|cm| cm.get("measured_ref"))
        .is_some();
    let declared = t
        .cost_model
        .as_ref()
        .and_then(|cm| cm.get("declared"))
        .and_then(|d| match d {
            Json::Obj(m) => Some(m.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let mut per_dimension = BTreeMap::new();
    for dim in DimensionId::ALL {
        let est = match declared.get(dim.as_str()) {
            Some(entry) => DimensionEstimate {
                value: entry
                    .get("value")
                    .or_else(|| entry.get("constant"))
                    .cloned(),
                confidence: entry
                    .get("confidence")
                    .and_then(Json::as_str)
                    .map(String::from),
                basis: if has_measured
                    && entry.get("basis").and_then(Json::as_str) == Some("measured")
                {
                    EstimateBasis::Measured
                } else {
                    EstimateBasis::Declared
                },
            },
            None => DimensionEstimate {
                value: None,
                confidence: None,
                basis: EstimateBasis::None,
            },
        };
        per_dimension.insert(dim, est);
    }
    Ok(CostEstimate { per_dimension })
}

// ── catalog / search_projection (ADR-0088 D6) ────────────────────────────────

/// The `catalog` filter — `{source_kind?, effect_domain?, world?, namespace?,
/// status?, dialect?}` (every member optional; `status` is the admission
/// spelling).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CatalogFilter {
    /// `source.kind` equality.
    pub source_kind: Option<String>,
    /// Declared `effect_domain` membership.
    pub effect_domain: Option<EffectDomain>,
    /// Any declared effect's `world`.
    pub world: Option<World>,
    /// The surface `namespace` member.
    pub namespace: Option<String>,
    /// The admission spelling (`resolved`/`sealed`/`quarantined`/`revoked`).
    pub status: Option<String>,
    /// The node's dialect.
    pub dialect: Option<String>,
}

/// A `CapabilitySummary` row — the deterministic catalog projection.
#[derive(Debug, Clone, PartialEq)]
pub struct CapabilitySummary {
    /// The node's `semantic_id`.
    pub semantic_id: String,
    /// The node's `version_id` (the registry coordinate).
    pub version_id: String,
    /// The declared `source.kind`.
    pub source_kind: String,
    /// The declared effect domains (sorted — `pure` yields `[]`).
    pub effect_domains: Vec<String>,
    /// The surface `namespace`, when a tool surface exists.
    pub namespace: Option<String>,
    /// The admission spelling.
    pub status: String,
    /// The node dialect.
    pub dialect: String,
}

/// `catalog(filter, snapshot)` — deterministic over a snapshot: entries are
/// the snapshot's members that are capabilities matching the filter, sorted by
/// `version_id` (AC-E1-7 — byte-identical across implementations).
pub fn catalog(
    store: &RegistryStore,
    filter: &CatalogFilter,
    snapshot: &RegistrySnapshot,
) -> Result<Vec<CapabilitySummary>, RegistryError> {
    let mut out = Vec::new();
    for version_id in &snapshot.members {
        let Some((env, rec)) = store.get(version_id) else {
            return Err(RegistryError::Unresolved {
                detail: format!("snapshot member {version_id} not in store"),
            });
        };
        let RegistryRecord::Capability(c) = rec else {
            continue;
        };
        let t = capability_semantic(c)?;
        let source_kind = hh_hir::tools::source_kind(&t.source).unwrap_or_default();
        let domains: Vec<String> = match &t.effects {
            ToolEffects::Pure => Vec::new(),
            ToolEffects::Declared(set) => set.iter().map(|e| e.domain.name().to_string()).collect(),
        };
        let namespace = match &c.node.surface {
            Some(hh_hir::records::SurfaceRecord::Tool(ts)) => Some(ts.namespace.clone()),
            _ => None,
        };
        let worlds: BTreeSet<World> = match &t.effects {
            ToolEffects::Pure => BTreeSet::new(),
            ToolEffects::Declared(set) => set
                .iter()
                .filter_map(|e| e.attributes.as_ref().map(|a| a.world))
                .collect(),
        };
        let status = env.admission.as_str().to_string();
        let row = CapabilitySummary {
            semantic_id: c.node.semantic_id(),
            version_id: version_id.clone(),
            source_kind,
            effect_domains: domains,
            namespace,
            status,
            dialect: c.node.version.dialect.clone(),
        };
        let keep = filter
            .source_kind
            .as_ref()
            .is_none_or(|k| *k == row.source_kind)
            && filter
                .effect_domain
                .is_none_or(|d| row.effect_domains.iter().any(|x| x == d.name()))
            && filter.world.is_none_or(|w| worlds.contains(&w))
            && filter
                .namespace
                .as_ref()
                .is_none_or(|n| Some(n) == row.namespace.as_ref())
            && filter.status.as_ref().is_none_or(|st| *st == row.status)
            && filter.dialect.as_ref().is_none_or(|d| *d == row.dialect);
        if keep {
            out.push(row);
        }
    }
    out.sort_by(|a, b| a.version_id.cmp(&b.version_id));
    Ok(out)
}

/// One searchable parameter — `{name, description?}` (from `input_schema`'s
/// `properties` — surface-independent: parameter *names* are semantic).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchParam {
    /// The parameter name.
    pub name: String,
    /// Its `description`, when declared.
    pub description: Option<String>,
}

/// `SearchDoc` — the surface-independent search projection (`{semantic_id,
/// purpose_text, parameter names/descriptions, effect_domains, namespace?}` —
/// ADR-0088 D6; never a profile name).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchDoc {
    /// The node's `semantic_id`.
    pub semantic_id: String,
    /// The `purpose` text content (the `Text` leaf's body, when inlined).
    pub purpose_text: Option<String>,
    /// The declared parameter names/descriptions.
    pub parameters: Vec<SearchParam>,
    /// The declared effect domains.
    pub effect_domains: Vec<String>,
    /// The surface `namespace`, when carried.
    pub namespace: Option<String>,
}

/// `search_projection(version_id)` — deterministic; surface-independent.
pub fn search_projection(
    store: &RegistryStore,
    version_id: &str,
) -> Result<SearchDoc, RegistryError> {
    let (c, _r) = capability_at(store, version_id)?;
    let t = capability_semantic(c)?;
    let parameters = t
        .input_schema
        .get("properties")
        .and_then(|p| match p {
            Json::Obj(m) => Some(m),
            _ => None,
        })
        .map(|m| {
            m.iter()
                .map(|(name, schema)| SearchParam {
                    name: name.clone(),
                    description: schema
                        .get("description")
                        .and_then(Json::as_str)
                        .map(String::from),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(SearchDoc {
        semantic_id: c.node.semantic_id(),
        purpose_text: t.purpose.content.clone(),
        parameters,
        effect_domains: match &t.effects {
            ToolEffects::Pure => Vec::new(),
            ToolEffects::Declared(set) => set.iter().map(|e| e.domain.name().to_string()).collect(),
        },
        namespace: match &c.node.surface {
            Some(hh_hir::records::SurfaceRecord::Tool(ts)) => Some(ts.namespace.clone()),
            _ => None,
        },
    })
}
