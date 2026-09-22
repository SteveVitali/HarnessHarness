//! Stage 4 — `lower_target(plan, surface?, target) → (TargetArtefact,
//! LoweringLossReport)` and `lift(artefact, target) → PartialHIR` (§3.2.5/§3.2.7;
//! ADR-0021 as amended).
//!
//! The two C0/Stage-3 targets:
//!
//! - **`mcp`** — `hh-mcp-target/1`: the canonical tool catalogue as a pure function of
//!   the bundle (`tools[]` sorted `(semantic_id, surface_name)`), each tool carrying
//!   the minimum carried set in `_meta["dev.cognition/hir"]` (`semantic_id`, the
//!   `EffectClass` set, the permission class, the provenance pointer
//!   `{origin, authority}`, the budget reference) plus the ADR-0096 D5 hint
//!   projection (`readOnlyHint ⇐ ∀ mutability = read_only`; `destructiveHint ⇐ ∃
//!   mutability = destructive`; `idempotentHint ⇐ ∀ repeat_safety = idempotent`;
//!   `openWorldHint ⇐ ∃ world = open ∨ resources.declared = false`). Losses: the
//!   fields MCP has no slot for — preconditions, observation contract, scope
//!   bindings, budget/validator/permission *enforcement* — each `no_slot`/`info`;
//!   ceiling `component` (the carried set travels).
//! - **`provider_tool_api`** — `hh-provider-request/1`: the `ProviderRequestPlan`
//!   (tool list in the profile's dialect — the strict subset when `strict_schema`
//!   is declared — plus layout, sampling/caching params, deferral, and the
//!   ADR-0118 D6 response-side normalisation record that recovers
//!   `tool_call_id`/`semantic_id`). Keywords outside the strict subset are
//!   `narrowed` losses — or `UnexpressibleSurface` under the profile's
//!   `strict = require`. No `lift` (ADR-0021 D5: consumed by the model);
//!   T-LCD-11 discharges through the loss report + the normalisation record.
//!
//! `lift` recovers a `PartialHIR{recovered[], unknown[], declared_unverified[]}`
//! from an artefact — for MCP the `_meta` carried set round-trips verbatim; tools
//! without it lift `origin = import`, `authority = unverified`; unknown `_meta`
//! keys are preserved in `unknown[]` and reported (never dropped — CC3).

use std::collections::BTreeMap;

use hh_hir::kinds::Mutability;
use hh_hir::records::KindRecord;
use hh_hir::ToolEffects;
use hh_wire::json::Json;

use crate::errors::CompileError;
use crate::lcd::{GranularityCeiling, LossEntry, LossKind, LossSeverity, LoweringLossReport};
use crate::link::{LinkedGraph, TargetSpec};
use crate::plan::RuntimePlan;
use crate::seal::ModelSurface;

/// The `_meta` key the minimum carried set travels under (MCP `_meta["<hh-prefix>/
/// hir"]`; the reverse-DNS prefix is ADR-0210/OQ-068's interim allocation).
pub const HH_META_KEY: &str = "dev.cognition/hir";

/// `hh-mcp-target/1` and `hh-provider-request/1` artefact dialects.
pub const MCP_ARTEFACT_DIALECT: &str = "hh-mcp-target/1";
/// The provider tool-API artefact dialect.
pub const PROVIDER_ARTEFACT_DIALECT: &str = "hh-provider-request/1";

/// The provider strict-subset keyword set (§3.2.5 — "keywords outside the strict
/// subset" are `narrowed`; ADR-0276 fixes the C0 membership).
pub const PROVIDER_STRICT_KEYWORDS: &[&str] = &[
    "additionalProperties",
    "const",
    "description",
    "enum",
    "items",
    "maxLength",
    "maximum",
    "minLength",
    "minimum",
    "properties",
    "required",
    "type",
];

/// `lower_target(linked, plan, surface, spec) → (artefact, LoweringLossReport)` —
/// §3.2.7. The artefact is canonical *data* conforming to the target's dialect —
/// never host code (OQ-043).
pub fn lower_target(
    linked: &LinkedGraph,
    plan: &RuntimePlan,
    surface: &ModelSurface,
    spec: &TargetSpec,
) -> Result<(Json, LoweringLossReport), CompileError> {
    match spec.target_id.as_str() {
        "mcp" => lower_mcp(linked, plan, surface, spec),
        "provider_tool_api" => lower_provider(linked, plan, surface, spec),
        other => Err(CompileError::TargetError {
            detail: format!("target {other} has no Stage-3 lowering (C1/C2)"),
        }),
    }
}

/// The MCP hint projection — ADR-0096 D5's fixed map (CF-047/CF-102).
fn mcp_hints(effects: &[hh_hir::EffectClass], resources_declared: bool) -> BTreeMap<String, bool> {
    let mut hints = BTreeMap::new();
    let all = |pred: &dyn Fn(&hh_hir::kinds::EffectAttributes) -> bool| {
        !effects.is_empty()
            && effects
                .iter()
                .all(|e| e.attributes.as_ref().map(pred).unwrap_or(false))
    };
    let any = |pred: &dyn Fn(&hh_hir::kinds::EffectAttributes) -> bool| {
        effects
            .iter()
            .any(|e| e.attributes.as_ref().map(pred).unwrap_or(false))
    };
    hints.insert(
        "readOnlyHint".to_string(),
        all(&|a| a.mutability == Mutability::ReadOnly) || effects.is_empty(),
    );
    hints.insert(
        "destructiveHint".to_string(),
        any(&|a| a.mutability == Mutability::Destructive),
    );
    hints.insert(
        "idempotentHint".to_string(),
        all(&|a| a.repeat_safety == hh_hir::kinds::RepeatSafety::Idempotent) || effects.is_empty(),
    );
    hints.insert(
        "openWorldHint".to_string(),
        effects.iter().any(|e| !e.is_closed_world()) || !resources_declared,
    );
    hints
}

/// The capability record for a plan tool.
fn capability_of<'a>(
    linked: &'a LinkedGraph,
    sid: &str,
) -> Result<&'a hh_hir::records::ToolCapabilityRecord, CompileError> {
    let node = linked
        .sealed
        .document
        .node(sid)
        .ok_or_else(|| CompileError::TargetError {
            detail: format!("capability {sid} not in the document"),
        })?;
    match &node.semantic {
        KindRecord::ToolCapability(t) => Ok(t),
        _ => Err(CompileError::TargetError {
            detail: format!("{sid} is not a ToolCapability"),
        }),
    }
}

/// The budget reference the plan binds (first budget row — the plan's envelope sid).
fn budget_ref(plan: &RuntimePlan) -> String {
    plan.budget
        .as_ref()
        .map(|b| b.budget.semantic_id.clone())
        .or_else(|| {
            plan.policies
                .budgets
                .first()
                .map(|b| b.budget.semantic_id.clone())
        })
        .unwrap_or_else(|| "none".to_string())
}

/// The permission class covering a capability — the sid of the `Permission` row
/// whose grants reach it (the plan's permission table; `none` when ungoverned).
fn permission_class(plan: &RuntimePlan, cap_sid: &str) -> String {
    plan.policies
        .permissions
        .iter()
        .find(|p| {
            p.grants.iter().any(|g| {
                g.get("capability")
                    .and_then(Json::as_str)
                    .map(|c| c == cap_sid || c == "*")
                    .unwrap_or(false)
            })
        })
        .map(|p| p.permission.semantic_id.clone())
        .unwrap_or_else(|| "none".to_string())
}

/// The declared `no_slot`/`info` losses every MCP tool carries (the fields MCP has
/// no typed slot for — §3.2.5's MCP row).
fn mcp_losses(linked: &LinkedGraph, plan: &RuntimePlan, surface: &ModelSurface) -> Vec<LossEntry> {
    let mut entries = Vec::new();
    for t in &surface.tools {
        let sid = t.binding.hir_node_id.clone();
        let mut push = |field: &str, detail: &str| {
            entries.push(LossEntry {
                hir_node_id: sid.clone(),
                field: field.to_string(),
                class: LossKind::NoSlot,
                severity: LossSeverity::Info,
                detail: detail.to_string(),
                debt_ref: None,
            });
        };
        push(
            "preconditions",
            "precondition domains have no MCP slot — enforced at the reference monitor",
        );
        push(
            "observation_contract",
            "the observation contract has no MCP slot — carried in _meta as data",
        );
        push(
            "scope_bindings",
            "scope bindings have no MCP slot — enforced at the reference monitor",
        );
        push(
            "permission.enforcement",
            "permission enforcement location — the monitor, never the hint",
        );
        push(
            "budgets",
            "budget dimensions have no MCP slot — carried as a reference",
        );
        let bound = plan
            .validators
            .iter()
            .filter(|v| v.inputs.iter().any(|i| i.semantic_id == sid))
            .count();
        if bound > 0 {
            push(
                "validators",
                "bound validators have no MCP slot — enforced at plan time",
            );
        }
        let _ = linked;
    }
    entries.sort_by(|a, b| {
        (&a.hir_node_id, &a.field, a.detail.as_str()).cmp(&(
            &b.hir_node_id,
            &b.field,
            b.detail.as_str(),
        ))
    });
    entries
}

/// The `mcp` lowering — `hh-mcp-target/1` (the served catalogue's source; the
/// `bundle_id` is stamped by the *caller* — the artefact is bundle-identity-free
/// so `bundle_id` stays a pure function of outputs, §3.2.4).
fn lower_mcp(
    linked: &LinkedGraph,
    plan: &RuntimePlan,
    surface: &ModelSurface,
    spec: &TargetSpec,
) -> Result<(Json, LoweringLossReport), CompileError> {
    let mut tools: Vec<&crate::lower::CompiledToolSurface> = surface.tools.iter().collect();
    tools.sort_by(|a, b| {
        (&a.binding.hir_node_id, &a.binding.surface_name)
            .cmp(&(&b.binding.hir_node_id, &b.binding.surface_name))
    });
    let mut emitted = Vec::new();
    for t in &tools {
        let cap = capability_of(linked, &t.binding.hir_node_id)?;
        let node = linked
            .sealed
            .document
            .node(&t.binding.hir_node_id)
            .expect("checked");
        let effects: Vec<Json> = match &cap.effects {
            ToolEffects::Pure => Vec::new(),
            ToolEffects::Declared(set) => set.iter().map(|e| Json::str(e.domain.name())).collect(),
        };
        let resources_declared = matches!(&cap.resources, hh_hir::records::Resources::Declared(_));
        let effect_set: Vec<hh_hir::EffectClass> = match &cap.effects {
            ToolEffects::Pure => Vec::new(),
            ToolEffects::Declared(s) => s.iter().cloned().collect(),
        };
        let hints = mcp_hints(&effect_set, resources_declared);
        let mut meta = vec![
            ("budget_ref", Json::str(budget_ref(plan))),
            ("effects", Json::Arr(effects)),
            (
                "permission_class",
                Json::str(permission_class(plan, &t.binding.hir_node_id)),
            ),
            (
                "provenance",
                Json::obj([
                    ("authority", Json::str(node.provenance.authority.as_str())),
                    ("origin", Json::str(node.provenance.origin.tag())),
                ]),
            ),
            ("semantic_id", Json::str(t.binding.hir_node_id.clone())),
            ("surface_id", Json::str(t.binding.surface_id.clone())),
        ];
        meta.sort_by_key(|(k, _)| k.to_string());
        let mut tool = vec![
            ("_meta", Json::obj([(HH_META_KEY, Json::obj(meta))])),
            (
                "annotations",
                Json::Obj(
                    hints
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::Bool(*v)))
                        .collect(),
                ),
            ),
            ("description", Json::str(t.description.clone())),
            ("inputSchema", t.schema.clone()),
            ("name", Json::str(t.binding.surface_name.clone())),
        ];
        tool.sort_by_key(|(k, _)| k.to_string());
        emitted.push(Json::obj(tool));
    }
    let artefact = Json::obj([
        ("dialect", Json::str(MCP_ARTEFACT_DIALECT)),
        (
            "discovery",
            Json::obj([("list_changed", Json::Bool(false))]),
        ),
        ("spec_version", Json::str(spec.spec_version.clone())),
        ("target", Json::str("mcp")),
        ("tools", Json::Arr(emitted)),
    ]);
    Ok((
        artefact,
        LoweringLossReport {
            target: "mcp".to_string(),
            target_version: spec.spec_version.clone(),
            entries: mcp_losses(linked, plan, surface),
            granularity_ceiling: GranularityCeiling::Component,
        },
    ))
}

/// Narrow a schema fragment to the provider strict subset — returns
/// `(narrowed_schema, dropped_keywords)`; every dropped keyword is a `narrowed`
/// loss entry the caller emits (silent coercion is forbidden — §3.2.5).
fn narrow_schema(
    schema: &Json,
    path: &str,
    strict: bool,
    hir_node_id: &str,
    entries: &mut Vec<LossEntry>,
) -> Result<Json, CompileError> {
    match schema {
        Json::Obj(m) => {
            let mut out = BTreeMap::new();
            for (k, v) in m {
                if PROVIDER_STRICT_KEYWORDS.contains(&k.as_str()) {
                    if k == "properties" {
                        // `properties` maps name → sub-schema — narrow each.
                        let mut inner = BTreeMap::new();
                        if let Json::Obj(props) = v {
                            for (pk, pv) in props {
                                inner.insert(
                                    pk.clone(),
                                    narrow_schema(
                                        pv,
                                        &format!("{path}.{k}.{pk}"),
                                        strict,
                                        hir_node_id,
                                        entries,
                                    )?,
                                );
                            }
                        }
                        out.insert(k.clone(), Json::Obj(inner));
                    } else if k == "items" {
                        out.insert(
                            k.clone(),
                            narrow_schema(v, &format!("{path}.{k}"), strict, hir_node_id, entries)?,
                        );
                    } else {
                        out.insert(k.clone(), v.clone());
                    }
                } else {
                    if strict {
                        return Err(CompileError::UnexpressibleSurface {
                            entity: hir_node_id.to_string(),
                            profile: String::new(),
                            reason: format!(
                                "keyword {k} at {path} is outside the provider strict subset and the profile requires strict"
                            ),
                        });
                    }
                    entries.push(LossEntry {
                        hir_node_id: hir_node_id.to_string(),
                        field: format!("schema{path}.{k}"),
                        class: LossKind::Narrowed,
                        severity: LossSeverity::Narrowed,
                        detail: format!(
                            "keyword {k} is outside the provider strict subset — dropped from the artefact"
                        ),
                        debt_ref: None,
                    });
                }
            }
            Ok(Json::Obj(out))
        }
        Json::Arr(items) => Ok(Json::Arr(
            items
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    narrow_schema(v, &format!("{path}[{i}]"), strict, hir_node_id, entries)
                })
                .collect::<Result<Vec<_>, _>>()?,
        )),
        other => Ok(other.clone()),
    }
}

/// The `provider_tool_api` lowering — `hh-provider-request/1` (the
/// `ProviderRequestPlan`; ADR-0021 D5: no lift — the loss report plus the T1–T7
/// normalisation record discharge T-LCD-11).
fn lower_provider(
    linked: &LinkedGraph,
    plan: &RuntimePlan,
    surface: &ModelSurface,
    spec: &TargetSpec,
) -> Result<(Json, LoweringLossReport), CompileError> {
    // `strict` comes off the bound chain's `schema_dialect` rule (ADR-0276).
    let strict = linked
        .profile
        .chain
        .iter()
        .flat_map(|p| &p.rules)
        .rfind(|r| r.kind == crate::profile::ProfileRuleKind::SchemaDialect)
        .and_then(|r| r.params.get("strict"))
        .and_then(Json::as_str)
        == Some("require");

    let mut entries = Vec::new();
    let mut tools = Vec::new();
    let mut sorted: Vec<&crate::lower::CompiledToolSurface> = surface.tools.iter().collect();
    sorted.sort_by(|a, b| {
        (&a.binding.hir_node_id, &a.binding.surface_name)
            .cmp(&(&b.binding.hir_node_id, &b.binding.surface_name))
    });
    for t in &sorted {
        let schema = narrow_schema(&t.schema, "", strict, &t.binding.hir_node_id, &mut entries)?;
        // A `truncate` result renderer is a declared `truncated` loss.
        if let Some(rr) = &t.result_render {
            if let crate::surface::RenderMode::Truncate { .. } = rr.mode {
                entries.push(LossEntry {
                    hir_node_id: t.binding.hir_node_id.clone(),
                    field: "result_render.retained_fields".to_string(),
                    class: LossKind::Truncated,
                    severity: LossSeverity::Narrowed,
                    detail: format!(
                        "result renderer truncates; retained fields: {}",
                        rr.declared_loss
                            .as_ref()
                            .map(|f| f.join(","))
                            .unwrap_or_default()
                    ),
                    debt_ref: None,
                });
            }
        }
        tools.push(Json::obj([
            ("description", Json::str(t.description.clone())),
            ("name", Json::str(t.binding.surface_name.clone())),
            ("parameters", schema),
            ("type", Json::str("function")),
        ]));
    }

    // The T1–T7 response-side normalisation record (ADR-0118 D6) — the declared
    // map the gateway reads to recover `tool_call_id`/`semantic_id` on the
    // response path; carried as data so the artefact is self-describing.
    let name_map: BTreeMap<String, Json> = sorted
        .iter()
        .map(|t| {
            (
                t.binding.surface_name.clone(),
                Json::str(t.binding.hir_node_id.clone()),
            )
        })
        .collect();
    let normalization = Json::obj([
        ("args_field", Json::str("arguments")),
        ("call_id_field", Json::str("id")),
        ("name_to_semantic_id", Json::Obj(name_map)),
        ("result_field", Json::str("content")),
        ("spec", Json::str("ADR-0118-D6-T1-T7")),
    ]);

    entries.sort_by(|a, b| (&a.hir_node_id, &a.field).cmp(&(&b.hir_node_id, &b.field)));
    let artefact = Json::obj([
        ("dialect", Json::str(PROVIDER_ARTEFACT_DIALECT)),
        (
            "interaction_mode",
            Json::str(surface.interaction_mode.clone()),
        ),
        (
            "layout",
            Json::Arr(
                surface
                    .layout
                    .iter()
                    .map(|s| {
                        Json::obj([
                            ("section_id", Json::str(s.section_id.clone())),
                            ("slots", Json::Arr(s.slots.iter().map(Json::str).collect())),
                            ("text", s.text.clone().map_or(Json::Null, Json::str)),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("normalization", normalization),
        ("params", surface.params.clone()),
        ("profile", Json::str(surface.profile.clone())),
        ("spec_version", Json::str(spec.spec_version.clone())),
        ("target", Json::str("provider_tool_api")),
        ("tools", Json::Arr(tools)),
    ]);
    let _ = plan;
    Ok((
        artefact,
        LoweringLossReport {
            target: "provider_tool_api".to_string(),
            target_version: spec.spec_version.clone(),
            entries,
            granularity_ceiling: GranularityCeiling::Configuration,
        },
    ))
}

/// `PartialHIR` — `lift`'s product on a foreign artefact (§3.2.5):
/// `{recovered[], unknown[], declared_unverified[]}` — `origin = import`,
/// `authority = unverified`; unknown `_meta` preserved in `ext`, never dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct PartialHIR {
    /// Recovered surface entries (`{semantic_id, name, description, input_schema,
    /// effects, permission_class, provenance, budget_ref, surface_id}`).
    pub recovered: Vec<Json>,
    /// Unknown `_meta` keys preserved verbatim (CC3 — nothing silently lost).
    pub unknown: Vec<Json>,
    /// Tools with no carried-set `_meta` — lifted `unverified`, origin `import`.
    pub declared_unverified: Vec<Json>,
}

/// `lift(artefact, target) → PartialHIR` (§3.2.7). `mcp` lifts; every other target
/// is `TargetError` — the provider tool-API artefact is consumed by the model and
/// declares no lifting contract (ADR-0021 D5), and the C1/C2 targets ship no
/// Stage-3 lift.
pub fn lift(artefact: &Json, target: &str) -> Result<PartialHIR, CompileError> {
    match target {
        "mcp" => lift_mcp(artefact),
        other => Err(CompileError::TargetError {
            detail: format!(
                "target {other} declares no lifting contract (provider_tool_api: ADR-0021 D5; acp/a2a/agent_spec: C1/C2)"
            ),
        }),
    }
}

/// Lift an `hh-mcp-target/1` (or a wire `ListToolsResult`) back to `PartialHIR`.
fn lift_mcp(artefact: &Json) -> Result<PartialHIR, CompileError> {
    let bad = |d: &str| CompileError::TargetError {
        detail: d.to_string(),
    };
    let tools = match artefact
        .get("tools")
        .or_else(|| artefact.get("result").and_then(|r| r.get("tools")))
    {
        Some(Json::Arr(t)) => t,
        _ => return Err(bad("artefact carries no tools[]")),
    };
    let mut recovered = Vec::new();
    let mut unknown = Vec::new();
    let mut unverified = Vec::new();
    for tool in tools {
        let name = tool
            .get("name")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        match tool.get("_meta").and_then(|m| m.get(HH_META_KEY)) {
            Some(meta) => {
                recovered.push(Json::obj([
                    (
                        "budget_ref",
                        meta.get("budget_ref").cloned().unwrap_or(Json::Null),
                    ),
                    (
                        "description",
                        tool.get("description").cloned().unwrap_or(Json::Null),
                    ),
                    (
                        "effects",
                        meta.get("effects").cloned().unwrap_or(Json::Arr(vec![])),
                    ),
                    (
                        "input_schema",
                        tool.get("inputSchema").cloned().unwrap_or(Json::Null),
                    ),
                    ("name", Json::str(name)),
                    (
                        "permission_class",
                        meta.get("permission_class").cloned().unwrap_or(Json::Null),
                    ),
                    (
                        "provenance",
                        meta.get("provenance").cloned().unwrap_or(Json::Null),
                    ),
                    (
                        "semantic_id",
                        meta.get("semantic_id").cloned().unwrap_or(Json::Null),
                    ),
                    (
                        "surface_id",
                        meta.get("surface_id").cloned().unwrap_or(Json::Null),
                    ),
                ]));
            }
            None => {
                // No carried set — the tool lifts `unverified`, origin `import`
                // (§3.2.5: hints alone never confer authority).
                unverified.push(Json::obj([
                    ("authority", Json::str("unverified")),
                    ("name", Json::str(name.clone())),
                    ("origin", Json::str("import")),
                ]));
            }
        }
        if let Some(Json::Obj(meta)) = tool.get("_meta") {
            for (k, v) in meta {
                if k != HH_META_KEY {
                    unknown.push(Json::obj([
                        ("key", Json::str(k.clone())),
                        ("value", v.clone()),
                    ]));
                }
            }
        }
    }
    Ok(PartialHIR {
        recovered,
        unknown,
        declared_unverified: unverified,
    })
}
