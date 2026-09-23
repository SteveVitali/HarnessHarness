//! Stage 3 — `lower_profile(linked, plan) → ModelSurface` (§3.2.2/§3.2.8; ADR-0019,
//! ADR-0124). Pure: the bound profile chain's rules shape the model-facing surface —
//! prompt layout (typed sections in profile-declared order), tool surfaces (name,
//! description, schema dialect, error format, result renderer, `SurfaceArgMap`),
//! interaction mode, sampling/caching/compaction parameters, and the transcript
//! renderer spec. Every element the profile touched carries its `rule_ids[]`; the
//! profile-untouched fields come through unchanged (T-LCD-01's owned-field check
//! measures this).
//!
//! Rule-parameter shapes are the ADR-0276 interim assignments (the §3.2.3 rule table
//! leaves `params` payloads per-rule; the closed members below are the C0 reading):
//!
//! - `naming` — `{scheme ∈ {prefix, suffix}, namespace?, separator?}`; `namespace`
//!   defaults to the profile's `profile_id` last segment, `separator` to `"__"`.
//! - `schema_dialect` — `{dialect, strict ∈ {prefer, require}?}`; `strict: require`
//!   raises `UnexpressibleSurface` where a narrowing would otherwise be a declared
//!   `narrowed` loss (§3.2.5).
//! - `tool_shape` — `{capability?, family_id?, variant_id ∈ {reference, patch,
//!   string_replace}, param?, grammar_ref?}`; `capability` scopes the rule to a
//!   capability's name (last `:`/`/` segment) or semantic id. `patch` exposes
//!   `{path…, patch}` with `patch → <param>` under `parse(grammar_ref)`;
//!   `string_replace` exposes `old_string`/`new_string` → `<param>.0.old|.0.new`
//!   under `project`.
//! - `prompt_layout` — `{order: [section_id], role_map: {authority_class →
//!   section_id}, slots?: {section_id → [slot]}}`; a text-bearing node whose
//!   authority maps to no declared section is `UnexpressibleSurface` (the
//!   role-placement invariant, ADR-0034).
//! - `interaction_mode` — `{mode}` (`native_fc` only at C0 — checked at link).
//! - `sampling_defaults` / `caching_markers` / `compaction_reminder` — params carried
//!   verbatim into `ModelSurface.params`.
//! - `transcript_render` — the `RendererSpec` choices record (`{stale_signature ∈
//!   {drop, placeholder}, …}` — OQ-067 default `placeholder`).
//! - `error_format` — `{renderings: {failure-class → text}}`; each rendering becomes
//!   a `Text` leaf owned by the profile coordinate with compiler-kernel provenance
//!   (T-LCD-02 — the profile supplies the string; the compiler mints the leaf).
//! - `result_render` — `{mode ∈ {full, truncate}, retained_fields?, validator_reads?}`
//!   → `ResultRenderSpec`.

use std::collections::BTreeMap;

use hh_assembly::{detail_text, AssemblyDiagnostic, Code, Severity, Stage};
use hh_hir::leaves::Text;
use hh_hir::records::{KindRecord, SurfaceRecord};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::equiv::{ArgMapEntry, ArgTransform, SurfaceBinding};
use crate::errors::CompileError;
use crate::link::LinkedGraph;
use crate::plan::RuntimePlan;
use crate::profile::{profile_coordinate, ModelProfile, ProfileRule, ProfileRuleKind};
use crate::surface::{ErrorFormatSpec, RenderMode, ResultRenderSpec};

/// A compiled layout section — `Section{section_id, source_node_ids, slots[], text?}`
/// (§3.2.8 `ModelSurface.layout`).
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledSection {
    /// The section id (profile-declared `order` member).
    pub section_id: String,
    /// The HIR nodes whose text the section renders (the identity map's half —
    /// `surface ↔ HIR identity map total`).
    pub source_node_ids: Vec<String>,
    /// Declared run-time slot names (`transcript`, `tool_results`, …) the section
    /// admits — declared, never filled at compile time.
    pub slots: Vec<String>,
    /// The composed section text (the concat of the source nodes' `Text` leaves, in
    /// document order — each leaf's own authority is what placed it here).
    pub text: Option<String>,
    /// The `ProfileRule` ids that shaped the section.
    pub rule_ids: Vec<String>,
}

/// The compiled per-tool surface — §3.2.8 `ToolSurface{surface_name, hir_node_id,
/// dialect, schema, description, error_format, result_render, arg_map, equivalence}`
/// (the `SurfaceBinding` carries the first/last set verbatim — ADR-0090's one atomic
/// record).
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledToolSurface {
    /// The `SurfaceBinding` (`surface_name`, `hir_node_id`, `dialect`, `arg_map`,
    /// `rule_ids`, …).
    pub binding: SurfaceBinding,
    /// The surface's input schema in the profile's dialect (the surface-side
    /// projection of the capability's `input_schema` over the arg map).
    pub schema: Json,
    /// The rendered description (`description_template` applied — a `Text` leaf's
    /// content, never a synthesized string).
    pub description: String,
    /// The compiled `ErrorFormatSpec` (E5's subject), when declared.
    pub error_format: Option<ErrorFormatSpec>,
    /// The compiled `ResultRenderSpec` (E6's subject), when declared.
    pub result_render: Option<ResultRenderSpec>,
    /// The `EquivalenceEvidence` ref — stamped at `seal_outputs` (evidence attaches
    /// after minting; the member exists for the reader, not the hash).
    pub equivalence: Option<crate::equiv::EquivalenceEvidence>,
}

/// Collect the effective params + stamped rule ids for one rule kind across the chain
/// (base→leaf; a later member's same-kind rule wins — `/rules` is `Append`, so the
/// winner is the last declared).
fn effective_rule(
    chain: &[ModelProfile],
    kind: ProfileRuleKind,
) -> Vec<(&ProfileRule, &ModelProfile)> {
    chain
        .iter()
        .flat_map(|p| p.rules.iter().map(move |r| (r, p)))
        .filter(|(r, _)| r.kind == kind)
        .collect()
}

/// The last-wins effective `params` for a rule kind (`None` when no member declares it).
fn rule_params(chain: &[ModelProfile], kind: ProfileRuleKind) -> Option<&Json> {
    effective_rule(chain, kind).last().map(|(r, _)| &r.params)
}

/// All rule ids of a kind that fired (the `rule_ids[]` stamp).
fn rule_ids(chain: &[ModelProfile], kind: ProfileRuleKind) -> Vec<String> {
    effective_rule(chain, kind)
        .iter()
        .map(|(r, _)| r.rule_id.clone())
        .collect()
}

/// `lower_profile(linked, plan) → (ModelSurface, diagnostics)` — §3.2.2 stage 3.
/// Diagnostics cover capabilities the profile/definition leaves unexposed
/// (`unexposed{policy_hidden | deferred | unsupported_mode}` — §3.2.8 invariants).
pub fn lower_profile(
    linked: &LinkedGraph,
    plan: &RuntimePlan,
    kernel: &ProvenanceRecord,
) -> Result<(crate::seal::ModelSurface, Vec<AssemblyDiagnostic>), CompileError> {
    let chain = &linked.profile.chain;
    let profile_coord = chain.last().map(profile_coordinate).unwrap_or_default();
    let doc = &linked.sealed.document;
    let mut diagnostics: Vec<AssemblyDiagnostic> = Vec::new();
    let mut unexposed = |sid: &str, reason: &str| {
        diagnostics.push(AssemblyDiagnostic {
            code: Code::ProfUnexpressible,
            class: None,
            severity: Severity::Warning,
            path: format!("/tools/{sid}"),
            source_layer: None,
            subject: sid.to_string(),
            stage: Stage::Lower,
            detail: detail_text(
                format!("capability unexposed under {profile_coord}: {reason}"),
                kernel,
            ),
            remedy: "expose the capability or accept the diagnostic".to_string(),
            owner_adr: "ADR-0090".to_string(),
        });
    };

    // ── interaction mode ────────────────────────────────────────────────────────
    let interaction_mode = rule_params(chain, ProfileRuleKind::InteractionMode)
        .and_then(|p| p.get("mode"))
        .and_then(Json::as_str)
        .unwrap_or("native_fc")
        .to_string();

    // ── naming rule ─────────────────────────────────────────────────────────────
    let naming = rule_params(chain, ProfileRuleKind::Naming);
    let naming_scheme = naming
        .and_then(|p| p.get("scheme"))
        .and_then(Json::as_str)
        .unwrap_or("identity");
    let naming_ns = naming
        .and_then(|p| p.get("namespace"))
        .and_then(Json::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            profile_coord
                .split('@')
                .next()
                .unwrap_or("profile")
                .rsplit([':', '/'])
                .next()
                .unwrap_or("profile")
                .to_string()
        });
    let naming_sep = naming
        .and_then(|p| p.get("separator"))
        .and_then(Json::as_str)
        .unwrap_or("__");
    let name_surface = |name: &str| -> Result<String, CompileError> {
        Ok(match naming_scheme {
            "identity" => name.to_string(),
            "prefix" => format!("{naming_ns}{naming_sep}{name}"),
            "suffix" => format!("{name}{naming_sep}{naming_ns}"),
            other => {
                return Err(CompileError::InvalidModelProfile {
                    detail: format!(
                        "naming rule scheme {other} is not in the closed set {{prefix, suffix}}"
                    ),
                })
            }
        })
    };

    // ── schema dialect + strict knob ────────────────────────────────────────────
    let dialect_params = rule_params(chain, ProfileRuleKind::SchemaDialect);
    let dialect = dialect_params
        .and_then(|p| p.get("dialect"))
        .and_then(Json::as_str)
        .unwrap_or(crate::equiv::DEFAULT_SCHEMA_DIALECT)
        .to_string();
    let strict = dialect_params
        .and_then(|p| p.get("strict"))
        .and_then(Json::as_str)
        .unwrap_or("prefer")
        .to_string();
    if strict != "prefer" && strict != "require" {
        return Err(CompileError::InvalidModelProfile {
            detail: format!("schema_dialect.strict ∈ {{prefer, require}}; got {strict}"),
        });
    }

    // ── tool_shape rules, scoped per capability ─────────────────────────────────
    let shape_rules = effective_rule(chain, ProfileRuleKind::ToolShape);

    // ── per-surface lowering ────────────────────────────────────────────────────
    let mut tools: Vec<CompiledToolSurface> = Vec::new();
    for tb in &plan.tools {
        let node = doc
            .node(&tb.capability.semantic_id)
            .expect("plan tools are document nodes");
        let Some(binding) = &tb.surface else {
            continue;
        };
        let cap_name = tb
            .capability
            .semantic_id
            .rsplit([':', '/'])
            .next()
            .unwrap_or(&tb.capability.semantic_id)
            .to_string();

        // exposure — a `hidden` authored surface, or a profile that defers it, is
        // unexposed (a diagnostic, never a silent drop — §3.2.8).
        if binding.hidden {
            unexposed(&tb.capability.semantic_id, "policy_hidden");
            continue;
        }
        if binding
            .admitted_modes
            .iter()
            .all(|m| *m == hh_hir::tools::ExposureMode::Deferred)
        {
            unexposed(&tb.capability.semantic_id, "deferred");
            continue;
        }

        let mut b = binding.clone();
        let mut applied: Vec<String> = Vec::new();

        // naming
        if naming.is_some() {
            b.surface_name = name_surface(&b.surface_name)?;
            applied.extend(rule_ids(chain, ProfileRuleKind::Naming));
        }

        // tool_shape — `{capability_class → SurfaceFamilyRef}`; the rule applies
        // to the capabilities it names (the map key scopes it).
        for (r, _) in shape_rules.iter() {
            let Json::Obj(m) = &r.params else {
                continue;
            };
            for (cap_key, family_ref) in m {
                if *cap_key != cap_name && *cap_key != tb.capability.semantic_id {
                    continue;
                }
                apply_tool_shape(&mut b, family_ref, node, &tb.capability.semantic_id)?;
                applied.push(r.rule_id.clone());
            }
        }

        // schema_dialect
        if dialect_params.is_some() {
            b.dialect = dialect.clone();
            applied.extend(rule_ids(chain, ProfileRuleKind::SchemaDialect));
        }

        b.rule_ids = {
            let mut ids = b.rule_ids.clone();
            ids.extend(applied);
            ids.sort();
            ids.dedup();
            ids
        };
        b.surface_id = crate::surface::surface_id(&b);

        // The surface-side input schema — the capability's `input_schema` projected
        // over the arg map (properties keyed by surface field; `project`/`parse`
        // sub-paths take the leaf schema).
        let schema = surface_input_schema(node, &b)?;

        // description — `description_template` Text leaf content, `{purpose}`
        // substituted with the capability's purpose text (the template is the leaf;
        // the profile's `description_template` rule supplies a replacement template
        // string when declared).
        let ts = match &node.surface {
            Some(SurfaceRecord::Tool(ts)) => ts,
            _ => {
                return Err(CompileError::UncheckableSurface {
                    surface: tb.capability.semantic_id.clone(),
                    reason: "binding without a Tool surface record".to_string(),
                })
            }
        };
        let template = rule_params(chain, ProfileRuleKind::DescriptionTemplate)
            .and_then(|p| p.get("template"))
            .and_then(Json::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| ts.description_template.content.clone().unwrap_or_default());
        let description = if let KindRecord::ToolCapability(t) = &node.semantic {
            template.replace("{purpose}", t.purpose.content.as_deref().unwrap_or(""))
        } else {
            template
        };

        // error_format — profile rule wins; else the authored `error_format` member.
        let error_format = compile_error_spec(
            rule_params(chain, ProfileRuleKind::ErrorFormat)
                .or(Some(&ts.error_format))
                .filter(|j| !matches!(j, Json::Null)),
            &profile_coord,
            kernel,
        )?;
        // result_render — profile rule wins; else the authored `result_renderer`.
        let result_render = compile_result_spec(
            rule_params(chain, ProfileRuleKind::ResultRender)
                .or(Some(&ts.result_renderer))
                .filter(|j| !matches!(j, Json::Null)),
        )?;

        tools.push(CompiledToolSurface {
            binding: b,
            schema,
            description,
            error_format,
            result_render,
            equivalence: None,
        });
    }

    // ── layout — `prompt_layout` rule, else the C0 default single `system` section ──
    let layout = lower_layout(linked, &profile_coord)?;

    // ── params + transcript renderer ────────────────────────────────────────────
    let mut params_pairs: Vec<(&str, Json)> = Vec::new();
    for (member, kind) in [
        ("sampling", ProfileRuleKind::SamplingDefaults),
        ("caching", ProfileRuleKind::CachingMarkers),
        ("compaction_reminder", ProfileRuleKind::CompactionReminder),
    ] {
        if let Some(p) = rule_params(chain, kind) {
            params_pairs.push((member, p.clone()));
        }
    }
    let params = Json::obj(params_pairs);
    let transcript_renderer = rule_params(chain, ProfileRuleKind::TranscriptRender)
        .cloned()
        .unwrap_or_else(|| Json::obj([("stale_signature", Json::str("placeholder"))]));

    // `dialects` — `map<ModelRole, dialect>`; at C0 the bound chain serves `primary`.
    let mut dialects = BTreeMap::new();
    dialects.insert("primary".to_string(), dialect);

    Ok((
        crate::seal::ModelSurface {
            profile: profile_coord,
            layout,
            tools,
            interaction_mode,
            params,
            transcript_renderer,
            dialects,
        },
        diagnostics,
    ))
}

/// Apply a `tool_shape` rule to a binding — the closed C0 variant set
/// (`reference | patch | string_replace`; ADR-0276).
fn apply_tool_shape(
    b: &mut SurfaceBinding,
    params: &Json,
    node: &hh_hir::Node,
    cap_sid: &str,
) -> Result<(), CompileError> {
    let variant = params
        .get("variant_id")
        .or_else(|| params.get("variant"))
        .and_then(Json::as_str)
        .unwrap_or("reference");
    let family = params
        .get("family_id")
        .and_then(Json::as_str)
        .unwrap_or("native_fc");
    if !crate::link::C0_SURFACE_FAMILIES.contains(&family) {
        return Err(CompileError::UnexpressibleSurface {
            entity: cap_sid.to_string(),
            profile: String::new(),
            reason: format!("tool_shape family {family} is not a C0 surface family"),
        });
    }
    b.family_id = Some(family.to_string());
    b.variant_id = Some(variant.to_string());
    match variant {
        "reference" | "default" => {}
        "patch" | "string_replace" => {
            let KindRecord::ToolCapability(t) = &node.semantic else {
                return Err(CompileError::UncheckableSurface {
                    surface: cap_sid.to_string(),
                    reason: "tool_shape on a non-capability node".to_string(),
                });
            };
            let shaped = params
                .get("param")
                .and_then(Json::as_str)
                .unwrap_or("edits")
                .to_string();
            let props: Vec<String> = match t.input_schema.get("properties") {
                Some(Json::Obj(m)) => m.keys().cloned().collect(),
                _ => Vec::new(),
            };
            let mut arg_map = BTreeMap::new();
            for p in &props {
                if *p == shaped {
                    continue;
                }
                arg_map.insert(
                    p.clone(),
                    ArgMapEntry {
                        capability_param: p.clone(),
                        transform: ArgTransform::Identity,
                        narrowing: None,
                    },
                );
            }
            if variant == "patch" {
                let grammar_ref = params
                    .get("grammar_ref")
                    .and_then(Json::as_str)
                    .ok_or_else(|| CompileError::UnexpressibleSurface {
                        entity: cap_sid.to_string(),
                        profile: String::new(),
                        reason: "tool_shape `patch` variant requires `grammar_ref`".to_string(),
                    })?
                    .to_string();
                arg_map.insert(
                    "patch".to_string(),
                    ArgMapEntry {
                        capability_param: shaped.clone(),
                        transform: ArgTransform::Parse { grammar_ref },
                        narrowing: None,
                    },
                );
            } else {
                for (field, tail) in [("old_string", "0.old"), ("new_string", "0.new")] {
                    arg_map.insert(
                        field.to_string(),
                        ArgMapEntry {
                            capability_param: format!("{shaped}.{tail}"),
                            transform: ArgTransform::Project,
                            narrowing: None,
                        },
                    );
                }
            }
            b.arg_map = arg_map;
        }
        other => {
            return Err(CompileError::UnexpressibleSurface {
                entity: cap_sid.to_string(),
                profile: String::new(),
                reason: format!(
                    "tool_shape variant {other} is outside the C0 set {{reference, patch, string_replace}}"
                ),
            })
        }
    }
    Ok(())
}

/// The surface-side input schema — `properties` keyed by surface field (projected
/// sub-paths take their leaf schema), `required` = surface fields whose capability
/// param root is required.
fn surface_input_schema(node: &hh_hir::Node, b: &SurfaceBinding) -> Result<Json, CompileError> {
    let KindRecord::ToolCapability(t) = &node.semantic else {
        return Err(CompileError::UncheckableSurface {
            surface: node.semantic_id(),
            reason: "surface schema on a non-capability node".to_string(),
        });
    };
    let props = match t.input_schema.get("properties") {
        Some(Json::Obj(m)) => m.clone(),
        _ => BTreeMap::new(),
    };
    let required: Vec<String> = match t.input_schema.get("required") {
        Some(Json::Arr(rs)) => rs
            .iter()
            .filter_map(|r| r.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };
    let leaf_schema = |root: &str, tail: &[&str]| -> Json {
        let mut s = props.get(root).cloned().unwrap_or(Json::Null);
        for seg in tail {
            s = match &s {
                Json::Obj(m) => m
                    .get("items")
                    .filter(|_| seg.chars().all(|c| c.is_ascii_digit()))
                    .or_else(|| m.get("properties").and_then(|p| p.get(seg)))
                    .cloned()
                    .unwrap_or(Json::Null),
                _ => Json::Null,
            };
            if matches!(s, Json::Null) {
                return Json::obj([("type", Json::str("string"))]);
            }
        }
        if matches!(s, Json::Null) {
            Json::obj([("type", Json::str("string"))])
        } else {
            s
        }
    };
    let mut out_props = BTreeMap::new();
    let mut out_required = Vec::new();
    for (field, entry) in &b.arg_map {
        let mut segs = entry.capability_param.split('.');
        let root = segs.next().unwrap_or(&entry.capability_param);
        let tail: Vec<&str> = segs.collect();
        out_props.insert(field.clone(), leaf_schema(root, &tail));
        if required.iter().any(|r| r == root) {
            out_required.push(Json::str(field.clone()));
        }
    }
    out_required.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    let mut schema = vec![
        ("properties", Json::Obj(out_props)),
        ("type", Json::str("object")),
    ];
    if !out_required.is_empty() {
        schema.push(("required", Json::Arr(out_required)));
    }
    Ok(Json::obj(schema))
}

/// Compile the effective `ErrorFormatSpec` — `{renderings: {class → text}}`; each
/// rendering becomes a `Text` leaf owned by the profile coordinate (T-LCD-02).
fn compile_error_spec(
    params: Option<&Json>,
    profile_coord: &str,
    kernel: &ProvenanceRecord,
) -> Result<Option<ErrorFormatSpec>, CompileError> {
    let Some(j) = params else { return Ok(None) };
    let renderings = match j.get("renderings") {
        Some(Json::Obj(m)) => m
            .iter()
            .map(|(k, v)| {
                let s = v.as_str().unwrap_or_default().to_string();
                (
                    k.clone(),
                    Text::new(s, profile_coord.to_string(), kernel.clone()),
                )
            })
            .collect(),
        _ => BTreeMap::new(),
    };
    let mut spec = ErrorFormatSpec {
        renderings,
        distinguishability: crate::surface::Distinguishability::Unchecked,
    };
    spec.distinguishability = crate::surface::check_distinguishable(&spec);
    Ok(Some(spec))
}

/// Compile the effective `ResultRenderSpec` — `{mode, retained_fields?,
/// validator_reads?}`.
fn compile_result_spec(params: Option<&Json>) -> Result<Option<ResultRenderSpec>, CompileError> {
    let Some(j) = params else { return Ok(None) };
    let mode = match j.get("mode").and_then(Json::as_str).unwrap_or("full") {
        "full" => RenderMode::Full,
        "truncate" => RenderMode::Truncate {
            max_lines: j
                .get("max_lines")
                .and_then(|v| v.as_int())
                .map(|i| i as u64),
            max_bytes: j
                .get("max_bytes")
                .and_then(|v| v.as_int())
                .map(|i| i as u64),
            max_tokens: j
                .get("max_tokens")
                .and_then(|v| v.as_int())
                .map(|i| i as u64),
            direction: j
                .get("direction")
                .and_then(Json::as_str)
                .and_then(crate::surface::TruncateDirection::parse)
                .unwrap_or(crate::surface::TruncateDirection::Head),
        },
        other => {
            return Err(CompileError::InvalidModelProfile {
                detail: format!("result_render.mode ∈ {{full, truncate}}; got {other}"),
            })
        }
    };
    let str_vec = |k: &str| -> Vec<String> {
        match j.get(k) {
            Some(Json::Arr(items)) => items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        }
    };
    Ok(Some(ResultRenderSpec {
        mode,
        declared_loss: j.get("retained_fields").map(|_| str_vec("retained_fields")),
        validator_reads: str_vec("validator_reads"),
    }))
}

/// The prompt layout — `prompt_layout` rule's declared `order` + `role_map`, else the
/// C0 default (one `system` section over every text-bearing node in document order).
fn lower_layout(
    linked: &LinkedGraph,
    profile_coord: &str,
) -> Result<Vec<CompiledSection>, CompileError> {
    let chain = &linked.profile.chain;
    let doc = &linked.sealed.document;
    // Text-bearing nodes and their (node) authority class.
    let mut text_nodes: Vec<(String, String, String)> = Vec::new();
    for n in &doc.nodes {
        let texts: Vec<String> = n
            .text_leaves()
            .iter()
            .filter_map(|t| t.content.clone())
            .collect();
        if texts.is_empty() {
            continue;
        }
        text_nodes.push((
            n.semantic_id(),
            n.provenance.authority.as_str().to_string(),
            texts.join("\n"),
        ));
    }
    let Some(params) = rule_params(chain, ProfileRuleKind::PromptLayout) else {
        // Default: one `system` section, document order.
        return Ok(vec![CompiledSection {
            section_id: "system".to_string(),
            source_node_ids: text_nodes.iter().map(|(s, _, _)| s.clone()).collect(),
            slots: Vec::new(),
            text: Some(
                text_nodes
                    .iter()
                    .map(|(_, _, t)| t.clone())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            rule_ids: Vec::new(),
        }]);
    };
    let order: Vec<String> = match params.get("order") {
        Some(Json::Arr(items)) => items
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };
    let role_map: BTreeMap<String, String> = match params.get("role_map") {
        Some(Json::Obj(m)) => m
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect(),
        _ => BTreeMap::new(),
    };
    let mut sections: BTreeMap<String, CompiledSection> = order
        .iter()
        .map(|id| {
            (
                id.clone(),
                CompiledSection {
                    section_id: id.clone(),
                    source_node_ids: Vec::new(),
                    slots: Vec::new(),
                    text: None,
                    rule_ids: rule_ids(chain, ProfileRuleKind::PromptLayout),
                },
            )
        })
        .collect();
    let mut texts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (sid, authority, text) in &text_nodes {
        let section = role_map
            .get(authority)
            .cloned()
            .unwrap_or_else(|| "system".to_string());
        if !sections.contains_key(&section) {
            return Err(CompileError::UnexpressibleSurface {
                entity: sid.clone(),
                profile: profile_coord.to_string(),
                reason: format!(
                    "prompt_layout role_map places authority `{authority}` into undeclared section `{section}`"
                ),
            });
        }
        sections
            .get_mut(&section)
            .expect("checked")
            .source_node_ids
            .push(sid.clone());
        texts.entry(section).or_default().push(text.clone());
    }
    if let Some(Json::Obj(slots)) = params.get("slots") {
        for (sec, v) in slots {
            if let Some(s) = sections.get_mut(sec) {
                if let Json::Arr(items) = v {
                    s.slots = items
                        .iter()
                        .filter_map(|i| i.as_str().map(str::to_string))
                        .collect();
                }
            }
        }
    }
    Ok(order
        .iter()
        .map(|id| {
            let mut s = sections.get(id).cloned().expect("order ∈ sections");
            let joined = texts.get(id).cloned().unwrap_or_default();
            s.text = if joined.is_empty() {
                None
            } else {
                Some(joined.join("\n"))
            };
            s
        })
        .collect())
}
