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

use hharness_assembly::{detail_text, AssemblyDiagnostic, Code, Severity, Stage};
use hharness_hir::leaves::Text;
use hharness_hir::records::{KindRecord, SurfaceRecord};
use hharness_provenance::ProvenanceRecord;
use hharness_wire::json::Json;

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
    /// The declared provider role (`prompt_layout.roles{section → role}` —
    /// REQ-XE-104). `None` ⇒ no declared role (the renderer's default —
    /// the member is absent, never null, on the wire).
    pub role: Option<String>,
    /// The section text's declared grammar (`hh-tmpl/1` when a placed
    /// `Text` leaf declares it — the renderer parses `text` as that
    /// grammar; REQ-XE-112). `None` ⇒ literal text.
    pub grammar: Option<String>,
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
            .all(|m| *m == hharness_hir::tools::ExposureMode::Deferred)
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
        // to the capabilities it names (the map key scopes it). A `split`
        // family yields several surfaces off one capability (ADR-0090 D7 —
        // the expanding half of the family set).
        let mut shaped: Vec<SurfaceBinding> = Vec::new();
        let mut saw_split = false;
        for (r, _) in shape_rules.iter() {
            let Json::Obj(m) = &r.params else {
                continue;
            };
            for (cap_key, family_ref) in m {
                if *cap_key != cap_name && *cap_key != tb.capability.semantic_id {
                    continue;
                }
                let family = family_ref
                    .get("family_id")
                    .and_then(Json::as_str)
                    .unwrap_or("native_fc");
                if family == "split" {
                    saw_split = true;
                    shaped.extend(crate::family::split_bindings(
                        &b,
                        family_ref.get("splits").unwrap_or(&Json::Null),
                    )?);
                } else {
                    apply_tool_shape(&mut b, family_ref, node, &tb.capability.semantic_id)?;
                }
                applied.push(r.rule_id.clone());
            }
        }

        // schema_dialect
        if dialect_params.is_some() {
            b.dialect = dialect.clone();
            applied.extend(rule_ids(chain, ProfileRuleKind::SchemaDialect));
        }

        let mut bindings_out: Vec<SurfaceBinding> = Vec::new();
        if saw_split {
            bindings_out.append(&mut shaped);
        }
        bindings_out.push(b);
        for b in bindings_out.iter_mut() {
            b.rule_ids = {
                let mut ids = b.rule_ids.clone();
                ids.extend(applied.clone());
                ids.sort();
                ids.dedup();
                ids
            };
            b.surface_id = crate::surface::surface_id(b);
        }

        // description — `description_template` Text leaf content, `{purpose}`
        // substituted with the capability's purpose text (the template is the leaf;
        // the profile's `description_template` rule supplies a replacement template
        // string when declared). Binding-independent — computed once.
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
            &profile_coord,
            kernel,
        )?;

        // Every produced binding (the primary + the `split` expansion) is its
        // own compiled surface — each mints its own `surface_id` and carries
        // the same rule provenance (E7: renaming a split changes its
        // `surface_id` only).
        for b in bindings_out {
            // The surface-side input schema — the capability's `input_schema`
            // projected over the arg map (properties keyed by surface field;
            // `project`/`parse` sub-paths take the leaf schema).
            let schema = surface_input_schema(node, &b)?;
            tools.push(CompiledToolSurface {
                binding: b,
                schema,
                description: description.clone(),
                error_format: error_format.clone(),
                result_render: result_render.clone(),
                equivalence: None,
            });
        }
    }

    // ── layout — `prompt_layout` rule, else the C0 default single `system` section ──
    let layout = lower_layout(linked, &profile_coord)?;

    // ── params + transcript renderer ────────────────────────────────────────────
    let mut params_pairs: Vec<(&str, Json)> = Vec::new();
    for (member, kind) in [
        ("sampling", ProfileRuleKind::SamplingDefaults),
        ("caching", ProfileRuleKind::CachingMarkers),
        ("compaction_reminder", ProfileRuleKind::CompactionReminder),
        // `prompt_layout` — the C1 members the runtime reads:
        // `demotion_wrappers[]`, `reminder_placement`, the profile's slot
        // families (§5c.1 C1 row; the sections lower separately — this is
        // the verbatim rule-params channel, same as `compaction_reminder`).
        ("prompt_layout", ProfileRuleKind::PromptLayout),
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
    node: &hharness_hir::Node,
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
        // C1 static composites — the canonical `PlanMap` fixtures
        // (AC-R-2.5.2-3; §5d.2 admissibility row).
        "str_replace_editor" => {
            let map = crate::family::str_replace_editor_plan();
            b.exposure_mode = crate::surface::CompileExposureMode::Composite;
            b.mapping = crate::surface::BindingMapping::PlanMap(
                crate::family::plan_map_ref(&map),
            );
            b.capability_refs = crate::family::capabilities_invoked(&map)
                .into_iter()
                .collect();
            b.arg_map = BTreeMap::new();
        }
        "unified_exec" => {
            let map = crate::family::unified_exec_plan();
            b.exposure_mode = crate::surface::CompileExposureMode::Composite;
            b.mapping = crate::surface::BindingMapping::PlanMap(
                crate::family::plan_map_ref(&map),
            );
            b.capability_refs = crate::family::capabilities_invoked(&map)
                .into_iter()
                .collect();
            b.arg_map = BTreeMap::new();
        }
        // C2 kinds — declared records, admissibility per §5d.2.
        "freeform" => {
            let grammar_ref = params
                .get("grammar_ref")
                .and_then(Json::as_str)
                .ok_or_else(|| CompileError::UnexpressibleSurface {
                    entity: cap_sid.to_string(),
                    profile: String::new(),
                    reason: "tool_shape `freeform` variant requires `grammar_ref`".to_string(),
                })?
                .to_string();
            let capability_param = params
                .get("param")
                .and_then(Json::as_str)
                .unwrap_or("input")
                .to_string();
            b.exposure_mode = crate::surface::CompileExposureMode::Freeform;
            b.arg_map = crate::family::freeform_map(&crate::family::FreeformSpec {
                grammar_ref,
                capability_param,
                surface_arg: "input".to_string(),
            });
        }
        "code_mode" => {
            b.exposure_mode = crate::surface::CompileExposureMode::CodeMode;
            b.admitted_modes
                .insert(hharness_hir::tools::ExposureMode::CodeMode);
        }
        "shim" => {
            b.exposure_mode = crate::surface::CompileExposureMode::Shim;
        }
        other => {
            return Err(CompileError::UnexpressibleSurface {
                entity: cap_sid.to_string(),
                profile: String::new(),
                reason: format!(
                    "tool_shape variant {other} is outside the registered set {{reference, patch, string_replace, str_replace_editor, unified_exec, freeform, code_mode, shim}}"
                ),
            })
        }
    }
    Ok(())
}

/// The surface-side input schema — `properties` keyed by surface field (projected
/// sub-paths take their leaf schema), `required` = surface fields whose capability
/// param root is required.
fn surface_input_schema(
    node: &hharness_hir::Node,
    b: &SurfaceBinding,
) -> Result<Json, CompileError> {
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
/// R6.36 (REQ-XE-111): `classes{class → text}` compiles the typed
/// format-failure renderings — each leaf's `hh-tmpl/1` variables must be
/// a subset of the class's declared slots (strict undefined's compile
/// half); `format_failure_scope` declares the per-call/batch render.
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
    // `classes{class → text}` — the REQ-XE-111 typed format-failure set.
    let classes = match j.get("classes") {
        None | Some(Json::Null) => None,
        Some(Json::Obj(m)) => {
            let mut renderings = BTreeMap::new();
            for (k, v) in m {
                let class = crate::surface::FormatFailureClass::parse(k).ok_or_else(|| {
                    CompileError::InvalidModelProfile {
                        detail: format!(
                            "error_format.classes key `{k}` is not a format-failure class \
                             (no_tool_calls, args_unparseable, unknown_tool, missing_param, \
                             output_truncated)"
                        ),
                    }
                })?;
                let s = v
                    .as_str()
                    .ok_or_else(|| CompileError::InvalidModelProfile {
                        detail: format!("error_format.classes.{k} is not a string"),
                    })?;
                let t = crate::tmpl::compile_template(s).map_err(|e| {
                    CompileError::UnexpressibleSurface {
                        entity: k.clone(),
                        profile: profile_coord.to_string(),
                        reason: format!("error_format.classes.{k}: {e}"),
                    }
                })?;
                let missing: Vec<String> = t
                    .variables()
                    .iter()
                    .filter(|v| !class.slots().contains(&v.as_str()))
                    .cloned()
                    .collect();
                if !missing.is_empty() {
                    return Err(CompileError::UnexpressibleSurface {
                        entity: k.clone(),
                        profile: profile_coord.to_string(),
                        reason: format!(
                            "error_format.classes.{k} variables {missing:?} exceed the \
                             class's declared slots {:?}",
                            class.slots()
                        ),
                    });
                }
                renderings.insert(
                    class,
                    Text::new(s.to_string(), profile_coord.to_string(), kernel.clone()),
                );
            }
            Some(renderings)
        }
        Some(_) => {
            return Err(CompileError::InvalidModelProfile {
                detail: "error_format.classes is not an object".to_string(),
            })
        }
    };
    let classes = match (classes, j.get("format_failure_scope")) {
        (None, _) => None,
        (Some(renderings), scope) => {
            let scope = match scope {
                None | Some(Json::Null) => crate::surface::FormatFailureScope::PerCall,
                Some(Json::Str(s)) => {
                    crate::surface::FormatFailureScope::parse(s).ok_or_else(|| {
                        CompileError::InvalidModelProfile {
                            detail: format!(
                                "error_format.format_failure_scope `{s}` — admitted: \
                                 per_call, batch"
                            ),
                        }
                    })?
                }
                Some(_) => {
                    return Err(CompileError::InvalidModelProfile {
                        detail: "error_format.format_failure_scope is not a string".to_string(),
                    })
                }
            };
            Some(crate::surface::FormatErrorClasses { renderings, scope })
        }
    };
    let mut spec = ErrorFormatSpec {
        renderings,
        distinguishability: crate::surface::Distinguishability::Unchecked,
        classes,
    };
    spec.distinguishability = crate::surface::check_distinguishable(&spec);
    Ok(Some(spec))
}

/// Compile the effective `ResultRenderSpec` — `{mode, retained_fields?,
/// validator_reads?}` plus the R6.36 typed renderers (`kind:
/// json_head_tail@1 | text_head_tail@1`, REQ-XE-110).
fn compile_result_spec(
    params: Option<&Json>,
    profile_coord: &str,
    kernel: &ProvenanceRecord,
) -> Result<Option<ResultRenderSpec>, CompileError> {
    let Some(j) = params else { return Ok(None) };
    // The profile params carry the mode flat (`mode: "truncate"` with the
    // member siblings beside it) — `RenderMode::parse` is the one grammar
    // (CC1; `concise`/`offload` land at C1 — S1.17/R2.8).
    let mode = RenderMode::parse(j.get("mode").and_then(Json::as_str).unwrap_or("full"), j)
        .map_err(|e| CompileError::InvalidModelProfile {
            detail: e.to_string(),
        })?;
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
        renderer: compile_typed_renderer(j, profile_coord, kernel)?,
    }))
}

/// `compile_typed_renderer(j)` — the `result_renderer.kind` member
/// (REQ-XE-110): `json_head_tail@1{threshold, head, tail, unit, fields,
/// json_style}` or `text_head_tail@1{threshold, head, tail, unit,
/// marker}` — typed-parameter constants, never string edits. Absent ⇒
/// `None` (absent-not-null, CC8); malformed members refuse typed.
fn compile_typed_renderer(
    j: &Json,
    profile_coord: &str,
    kernel: &ProvenanceRecord,
) -> Result<Option<crate::surface::ResultRenderer>, CompileError> {
    let Some(kind) = j.get("kind").and_then(Json::as_str) else {
        return Ok(None);
    };
    let bad = |d: String| CompileError::InvalidModelProfile { detail: d };
    let u64_member = |k: &str| -> Result<u64, CompileError> {
        j.get(k)
            .and_then(Json::as_int)
            .map(|i| i.max(0) as u64)
            .ok_or_else(|| bad(format!("result_renderer.{kind}: `{k}` must be an integer")))
    };
    let head_tail =
        |j: &Json| -> Result<(u64, u64, u64, crate::surface::HeadTailUnit), CompileError> {
            let unit = j
                .get("unit")
                .and_then(Json::as_str)
                .and_then(crate::surface::HeadTailUnit::parse)
                .ok_or_else(|| {
                    bad(format!(
                        "result_renderer.{kind}: `unit` ∈ {{chars, utf8_bytes}} required"
                    ))
                })?;
            Ok((
                u64_member("threshold")?,
                u64_member("head")?,
                u64_member("tail")?,
                unit,
            ))
        };
    match kind {
        "json_head_tail@1" => {
            let (threshold, head, tail, unit) = head_tail(j)?;
            let fields = match j.get("fields") {
                Some(Json::Arr(items)) => items
                    .iter()
                    .map(|f| {
                        if let Some(name) = f.as_str() {
                            // `"name"` required, `"name?"` optional.
                            let optional = name.ends_with('?');
                            Ok(crate::surface::ResultRenderField {
                                name: name.trim_end_matches('?').to_string(),
                                optional,
                                truncate: false,
                                warning: None,
                            })
                        } else if let Json::Obj(m) = f {
                            let name = m
                                .get("name")
                                .and_then(Json::as_str)
                                .ok_or_else(|| {
                                    bad(format!(
                                        "result_renderer.{kind}: fields[] member needs `name`"
                                    ))
                                })?
                                .to_string();
                            let warning =
                                m.get("warning").and_then(Json::as_str).map(str::to_string);
                            Ok(crate::surface::ResultRenderField {
                                name,
                                optional: matches!(m.get("optional"), Some(Json::Bool(true))),
                                truncate: matches!(m.get("truncate"), Some(Json::Bool(true))),
                                warning,
                            })
                        } else {
                            Err(bad(format!(
                                "result_renderer.{kind}: fields[] member is not a \
                                 string or object"
                            )))
                        }
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?,
                _ => {
                    return Err(bad(format!(
                        "result_renderer.{kind}: `fields` array required"
                    )))
                }
            };
            let truncates = fields.iter().filter(|f| f.truncate).count();
            if truncates != 1 {
                return Err(bad(format!(
                    "result_renderer.{kind}: exactly one `fields[]` member carries \
                     `truncate: true`; got {truncates}"
                )));
            }
            let json_style = j
                .get("json_style")
                .and_then(Json::as_str)
                .and_then(crate::surface::ResultJsonStyle::parse)
                .ok_or_else(|| {
                    bad(format!(
                        "result_renderer.{kind}: `json_style` ∈ {{python_jinja_tojson, \
                         canonical}} required"
                    ))
                })?;
            Ok(Some(crate::surface::ResultRenderer::JsonHeadTail {
                threshold,
                head,
                tail,
                unit,
                fields,
                json_style,
            }))
        }
        "text_head_tail@1" => {
            let (threshold, head, tail, unit) = head_tail(j)?;
            let marker = j
                .get("marker")
                .and_then(Json::as_str)
                .ok_or_else(|| bad(format!("result_renderer.{kind}: `marker` string required")))?;
            let t = crate::tmpl::compile_template(marker).map_err(|e| {
                CompileError::UnexpressibleSurface {
                    entity: kind.to_string(),
                    profile: profile_coord.to_string(),
                    reason: format!("result_renderer.{kind}.marker: {e}"),
                }
            })?;
            let missing: Vec<String> = t
                .variables()
                .iter()
                .filter(|v| !crate::surface::ResultRenderer::MARKER_SLOTS.contains(&v.as_str()))
                .cloned()
                .collect();
            if !missing.is_empty() {
                return Err(CompileError::UnexpressibleSurface {
                    entity: kind.to_string(),
                    profile: profile_coord.to_string(),
                    reason: format!(
                        "result_renderer.{kind}.marker variables {missing:?} exceed the \
                         renderer's slots {:?}",
                        crate::surface::ResultRenderer::MARKER_SLOTS
                    ),
                });
            }
            Ok(Some(crate::surface::ResultRenderer::TextHeadTail {
                threshold,
                head,
                tail,
                unit,
                marker: Text::new(
                    marker.to_string(),
                    profile_coord.to_string(),
                    kernel.clone(),
                ),
            }))
        }
        other => Err(bad(format!(
            "result_renderer.kind `{other}` — admitted: json_head_tail@1, \
             text_head_tail@1"
        ))),
    }
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
            role: None,
            grammar: None,
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
                    role: None,
                    grammar: None,
                },
            )
        })
        .collect();
    // `roles{section → provider role}` — REQ-XE-104's declared provider
    // roles (a closed set at this stage — the chat-completions shape).
    let roles: BTreeMap<String, String> = match params.get("roles") {
        Some(Json::Obj(m)) => m
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect(),
        _ => BTreeMap::new(),
    };
    for (sec, role) in &roles {
        if !sections.contains_key(sec) {
            return Err(CompileError::UnexpressibleSurface {
                entity: sec.clone(),
                profile: profile_coord.to_string(),
                reason: format!("prompt_layout.roles names undeclared section `{sec}`"),
            });
        }
        if !matches!(role.as_str(), "system" | "user" | "assistant" | "tool") {
            return Err(CompileError::UnexpressibleSurface {
                entity: sec.clone(),
                profile: profile_coord.to_string(),
                reason: format!("prompt_layout.roles `{role}` is not a declared provider role"),
            });
        }
        sections.get_mut(sec).expect("checked").role = Some(role.clone());
    }
    // `slots{section → [slot names]}` — declared run-time fills; a
    // `hh-tmpl/1` leaf's variable set must be a subset (strict
    // undefined's compile half — REQ-XE-112).
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
    // `section_templates{section → node}` — explicit `Text`-leaf
    // placement (§5b.3 row 8; REQ-XE-104): the named node's text renders
    // into the named section under its declared role; unplaced nodes
    // still route by `role_map` over authority.
    let section_templates: BTreeMap<String, String> = match params.get("section_templates") {
        Some(Json::Obj(m)) => m
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect(),
        _ => BTreeMap::new(),
    };
    let placed: std::collections::BTreeSet<String> = section_templates.values().cloned().collect();
    let mut texts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (sid, authority, text) in &text_nodes {
        if placed.contains(sid) {
            continue;
        }
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
    // Resolve each explicit placement: the node must exist and bear
    // text; the role-placement rule (§7.2 — `system` placement requires
    // `definition` authority) gates the leaf; a `hh-tmpl/1` leaf
    // compiles its template now (strict undefined's compile half).
    for (sec, node_id) in &section_templates {
        let role = sections.get(sec).map(|s| s.role.clone()).flatten();
        if !sections.contains_key(sec) {
            return Err(CompileError::UnexpressibleSurface {
                entity: node_id.clone(),
                profile: profile_coord.to_string(),
                reason: format!(
                    "prompt_layout.section_templates places `{node_id}` into undeclared \
                     section `{sec}`"
                ),
            });
        }
        let node = doc
            .nodes
            .iter()
            .find(|n| n.semantic_id() == *node_id)
            .ok_or_else(|| CompileError::UnexpressibleSurface {
                entity: node_id.clone(),
                profile: profile_coord.to_string(),
                reason: format!("prompt_layout.section_templates names no node `{node_id}`"),
            })?;
        let leaves: Vec<&Text> = node
            .text_leaves()
            .iter()
            .filter(|t| t.content.is_some())
            .copied()
            .collect();
        if leaves.is_empty() {
            return Err(CompileError::UnexpressibleSurface {
                entity: node_id.clone(),
                profile: profile_coord.to_string(),
                reason: format!("section_templates node `{node_id}` carries no text leaf"),
            });
        }
        if role.as_deref() == Some("system")
            && node.provenance.authority < hharness_provenance::AuthorityClass::Definition
        {
            return Err(CompileError::UnexpressibleSurface {
                entity: node_id.clone(),
                profile: profile_coord.to_string(),
                reason: format!(
                    "node authority `{}` cannot place into the `system` role \
                     (definition required)",
                    node.provenance.authority.as_str()
                ),
            });
        }
        for leaf in &leaves {
            if leaf.language.as_deref() == Some("hh-tmpl/1") {
                let content = leaf.content.as_deref().unwrap_or_default();
                let t = crate::tmpl::compile_template(content).map_err(|e| {
                    CompileError::UnexpressibleSurface {
                        entity: node_id.clone(),
                        profile: profile_coord.to_string(),
                        reason: format!("{e}"),
                    }
                })?;
                let slots: std::collections::BTreeSet<String> = sections
                    .get(sec)
                    .map(|s| s.slots.iter().cloned().collect())
                    .unwrap_or_default();
                let missing: Vec<String> = t
                    .variables()
                    .iter()
                    .filter(|v| !slots.contains(*v))
                    .cloned()
                    .collect();
                if !missing.is_empty() {
                    return Err(CompileError::UnexpressibleSurface {
                        entity: node_id.clone(),
                        profile: profile_coord.to_string(),
                        reason: format!(
                            "template variables {missing:?} are not declared slots of \
                             section `{sec}`"
                        ),
                    });
                }
                sections.get_mut(sec).expect("checked").grammar = Some("hh-tmpl/1".to_string());
            }
        }
        sections
            .get_mut(sec)
            .expect("checked")
            .source_node_ids
            .push(node_id.clone());
        texts.entry(sec.clone()).or_default().push(
            leaves
                .iter()
                .map(|t| t.content.clone().unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\n"),
        );
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
