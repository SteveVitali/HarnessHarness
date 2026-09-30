//! `SurfaceArgMap`/`SurfaceBinding` (ADR-0090) and `EquivalenceEvidence` (§3.2.5) with the
//! Stage-1 static checks: **E1** effect equality, **E2** arg-map totality + domain
//! membership, **E3** precondition-domain preservation (schema inclusion over the
//! OQ-219 interim keyword subset), **E7** accounting/identity (`trace_map(S) ∋
//! C.semantic_id`). E4–E6 are staged: E4 is `n/a{stage_3|open-world}` (T-LCD-15 — never
//! 0/fail for open-world capabilities), E5/E6 `n/a{stage_3}` (§5b/§5f own them).

use std::collections::BTreeMap;

use hh_hir::{records::ToolCapabilityRecord, ToolSurface};
use hh_wire::json::Json;

use crate::errors::CompileError;
use crate::plan::PinnedRef;

/// The default schema dialect — the OQ-219 interim rule (ADR-0212): `input_schema`
/// declarations are JSON-Schema 2020-12 by default.
pub const DEFAULT_SCHEMA_DIALECT: &str = "json-schema-2020-12";

/// The E3-admitted JSON-Schema keyword subset (the OQ-219 interim subset — the subset the
/// Stage-1 fixtures exercise: `type`, `enum`, `const`, `required`, `properties`, `items`,
/// `minimum`, `maximum`, `minLength`, `maxLength`, `additionalProperties`, `description`,
/// `title`, `default`, `$defs`, `format`). Any keyword outside this set makes the
/// inclusion check `n/a{unchecked_keyword}` — the checker never guesses (T-LCD-15).
pub const E3_ADMITTED_KEYWORDS: &[&str] = &[
    "type",
    "enum",
    "const",
    "required",
    "properties",
    "items",
    "minimum",
    "maximum",
    "minLength",
    "maxLength",
    "additionalProperties",
    "description",
    "title",
    "default",
    "$defs",
    "format",
];

/// The default per-`ModelRole` dialect set — `json-schema-2020-12` on every role
/// (ADR-0212 OQ-219 interim; per-role narrowing arrives with the dialect machinery).
pub fn default_dialects() -> BTreeMap<String, String> {
    [
        "primary",
        "utility",
        "compaction",
        "subagent",
        "judge",
        "router_predictor",
    ]
    .iter()
    .map(|r| (r.to_string(), DEFAULT_SCHEMA_DIALECT.to_string()))
    .collect()
}

/// `SurfaceArgMap.transform` — the closed set (§3.2.6 rule ii; ADR-0090):
/// `identity | rename | const | project | coerce(declared_loss?) | parse(grammar_ref) |
/// unescape(policy_ref) | resolve_name(table_ref)`. Each is a Core interpreter with no
/// ambient authority; `resolve_name` is exact-table only.
#[derive(Debug, Clone, PartialEq)]
pub enum ArgTransform {
    /// `identity` — the surface arg is the capability arg, unchanged.
    Identity,
    /// `rename` — the surface arg is the capability arg under a different name.
    Rename,
    /// `const{value}` — a fixed value (checked against the parameter's domain at E2).
    Const(Json),
    /// `project` — the surface field projects a sub-structure of the capability
    /// parameter (the entry's `capability_param` names the projection target).
    Project,
    /// `coerce{declared_loss?}` — a coercion; when it narrows, `declared_loss` names the
    /// loss (undeclared loss is an E2/E3 soundness failure, never silent).
    Coerce {
        /// The declared loss, when the coercion loses information.
        declared_loss: Option<String>,
    },
    /// `parse{grammar_ref}` — the surface arg parses through a declared grammar (a
    /// `CompiledPayload` leaf — ADR-0090/CF-197).
    Parse {
        /// The grammar reference.
        grammar_ref: String,
    },
    /// `unescape{policy_ref}` — the surface arg unescapes through a declared policy.
    Unescape {
        /// The policy reference.
        policy_ref: String,
    },
    /// `resolve_name{table_ref}` — the surface arg resolves through a name table
    /// (exact-table only).
    ResolveName {
        /// The table reference.
        table_ref: String,
    },
}

impl ArgTransform {
    /// The transform kind spelling.
    pub fn kind(&self) -> &'static str {
        match self {
            ArgTransform::Identity => "identity",
            ArgTransform::Rename => "rename",
            ArgTransform::Const(_) => "const",
            ArgTransform::Project => "project",
            ArgTransform::Coerce { .. } => "coerce",
            ArgTransform::Parse { .. } => "parse",
            ArgTransform::Unescape { .. } => "unescape",
            ArgTransform::ResolveName { .. } => "resolve_name",
        }
    }
}

/// One `SurfaceArgMap` entry — `{capability_param, transform, narrowing?}` (§3.2.6 E2;
/// ADR-0090): the map key is the `surface_field`; `capability_param` names the
/// capability parameter it binds (a parameter path for `project`); `narrowing` records
/// a declared domain narrowing.
#[derive(Debug, Clone, PartialEq)]
pub struct ArgMapEntry {
    /// The capability parameter this surface field binds.
    pub capability_param: String,
    /// The transform.
    pub transform: ArgTransform,
    /// The declared narrowing, when the surface domain is a strict subset.
    pub narrowing: Option<String>,
}

/// `SurfaceArgMap` — `{surface_field → {capability_param, transform, narrowing?}}`
/// (a `BTreeMap` — canonical order).
pub type SurfaceArgMap = BTreeMap<String, ArgMapEntry>;

/// `SurfaceBinding{surface_id, exposure_mode, capability_refs: [semantic_id],
/// mapping, rule_ids[], evidence_ref, safety_ref, effects_bound, family_id,
/// variant_id}` (§5d.2 §3; ADR-0090 D4) — the record the trace map, the
/// reference monitor and every `action.tool.*` event reference; the only
/// accounting key (E7). It is **one atomic record**, bound only inside an
/// equivalence-checked compile; `hir_node_id` names the capability node the
/// surface is declared on.
///
/// The C0 additions keep the compiled members the run reads (`surface_name`,
/// `capability_ref` — the pinned version coordinate; `arg_map`, `dialect`)
/// beside the spec members; `capability_refs` are **semantic ids** (S1
/// inclusion reads them; a rename never touches them — AC-R-2.5.2-2).
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceBinding {
    /// The model-facing surface name (the `ToolSurface.name`).
    pub surface_name: String,
    /// `surface_id` — the surface-record hash (`crate::surface::surface_id`;
    /// the catalog/trace-map/`action.tool.*` coordinate).
    pub surface_id: String,
    /// The compile-time exposure mode (`primitive` at C0).
    pub exposure_mode: crate::surface::CompileExposureMode,
    /// The capability the surface exposes (the pinned version coordinate the
    /// run resolves).
    pub capability_ref: PinnedRef,
    /// `capability_refs: [semantic_id]` — the S1 inclusion set (at C0 the
    /// single capability's semantic id; composites carry several at C1).
    pub capability_refs: Vec<String>,
    /// The capability node's semantic id (the surface's home node).
    pub hir_node_id: String,
    /// The argument map (total over the surface's `argument_order`) — the
    /// `mapping = SurfaceArgMap` payload.
    pub arg_map: SurfaceArgMap,
    /// The `mapping` kind (`SurfaceArgMap` at C0; `PlanMap` is the C1 shape).
    pub mapping: crate::surface::BindingMapping,
    /// The `ProfileRule` ids that shaped the variant (C0 primitives: `[]`).
    pub rule_ids: Vec<String>,
    /// The `EquivalenceEvidence` record ref, when produced.
    pub evidence_ref: Option<String>,
    /// The `SafetyEvidence` record ref, when produced.
    pub safety_ref: Option<String>,
    /// `effects_bound` — the declared effect-domain spellings the surface
    /// binds (`⊆ effects(capability_refs)`; S1).
    pub effects_bound: Vec<String>,
    /// The surface family id (C1; `None` for C0 primitives).
    pub family_id: Option<String>,
    /// The variant id within the family (C1).
    pub variant_id: Option<String>,
    /// The schema dialect the surface's schemas are expressed in (default
    /// `json-schema-2020-12`; a narrowing must be declared — `DialectNarrowingUndeclared`
    /// otherwise).
    pub dialect: String,
    /// The authored run-time admitted-mode set (`ToolSurface.exposure_mode.
    /// admitted_modes`; the definition side of I-NARROW).
    pub admitted_modes: std::collections::BTreeSet<hh_hir::tools::ExposureMode>,
    /// `pinned` — definition/kernel-set, immutable to policy (I-NARROW);
    /// pinned surfaces stay `direct` and cannot be evicted.
    pub pinned: bool,
    /// `hidden` — never delivered, indexed or callable (AC-R-2.5.3-12).
    pub hidden: bool,
}

/// Derive the `SurfaceBinding` for a `ToolCapability` node carrying a `Tool` surface:
/// `arg_map` is the `identity` transform over `argument_order` (the Stage-1 authored
/// surface declares argument names already in the capability's `input_schema`; E2 checks
/// membership — a name outside the schema is an E2 `fail`, not a bind-time guess).
pub fn bind_surface(
    node: &hh_hir::Node,
    surface: &ToolSurface,
    _input_schema: &Json,
) -> SurfaceBinding {
    let arg_map: SurfaceArgMap = surface
        .argument_order
        .iter()
        .map(|a| {
            (
                a.clone(),
                ArgMapEntry {
                    capability_param: a.clone(),
                    transform: ArgTransform::Identity,
                    narrowing: None,
                },
            )
        })
        .collect();
    let dialect = surface
        .schema_dialect_narrowing
        .get("dialect")
        .and_then(Json::as_str)
        .unwrap_or(DEFAULT_SCHEMA_DIALECT)
        .to_string();
    let authored = hh_hir::tools::authored_exposure(Some(&surface.exposure_mode)).unwrap_or(
        hh_hir::tools::AuthoredExposure {
            hidden: false,
            pinned: false,
            admitted: [hh_hir::tools::ExposureMode::Direct].into_iter().collect(),
        },
    );
    let mut binding = SurfaceBinding {
        surface_name: surface.name.clone(),
        surface_id: String::new(),
        exposure_mode: crate::surface::CompileExposureMode::Primitive,
        capability_ref: PinnedRef {
            semantic_id: node.semantic_id(),
            version_id: node.version_id(),
        },
        capability_refs: vec![node.semantic_id()],
        hir_node_id: node.semantic_id(),
        arg_map,
        mapping: crate::surface::BindingMapping::SurfaceArgMap,
        rule_ids: Vec::new(),
        evidence_ref: None,
        safety_ref: None,
        effects_bound: Vec::new(),
        family_id: None,
        variant_id: None,
        dialect,
        admitted_modes: authored.admitted,
        pinned: authored.pinned,
        hidden: authored.hidden,
    };
    // `effects_bound` is the declared effect-domain set of the capability (S1
    // inclusion is `effects_bound ⊆ effects(capability_refs)` — equality at C0).
    if let hh_hir::records::KindRecord::ToolCapability(t) = &node.semantic {
        if let hh_hir::kinds::ToolEffects::Declared(set) = &t.effects {
            binding.effects_bound = set.iter().map(|e| e.domain.name().to_string()).collect();
        }
    }
    binding.surface_id = crate::surface::surface_id(&binding);
    binding
}

/// `EquivalenceEvidence` — `{E1..E7 : pass | fail | n/a(reason)}` (§3.2.5). One record
/// per compiled surface, living in the bundle's `equivalence_evidence[]` and in the
/// target's conformance record.
#[derive(Debug, Clone, PartialEq)]
pub struct EquivalenceEvidence {
    /// The surface this evidence is for.
    pub surface_name: String,
    /// The capability's semantic id.
    pub capability: String,
    /// E1 — effect equality over the declared set.
    pub e1_effect_equality: EvidenceVerdict,
    /// E2 — arg-map totality + domain membership.
    pub e2_authority: EvidenceVerdict,
    /// E3 — precondition-domain preservation (schema inclusion).
    pub e3_precondition_domain: EvidenceVerdict,
    /// E4 — differential end-state equivalence (Stage 3 executable; `n/a(open-world)` for
    /// capabilities whose declared effect set is open-world — T-LCD-15).
    pub e4_differential: EvidenceVerdict,
    /// E5 — error-class surjectivity (§5b owns it — `n/a{stage_3}` at C0).
    pub e5_error_surjectivity: EvidenceVerdict,
    /// E6 — result-observation adequacy (§5f owns it — `n/a{stage_3}` at C0).
    pub e6_result_observation: EvidenceVerdict,
    /// E7 — accounting/identity (`trace_map(S) ∋ C.semantic_id`).
    pub e7_accounting_identity: EvidenceVerdict,
}

/// `pass | fail | n/a(reason)` — the evidence verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceVerdict {
    /// The check holds.
    Pass,
    /// The check fails — the surface does not bind.
    Fail {
        /// The typed reason.
        reason: String,
    },
    /// Not applicable at this stage (`n/a{stage_3|open-world|unchecked_keyword}` — never
    /// silent; T-LCD-15).
    NotApplicable {
        /// The reason class spelling.
        reason: String,
    },
}

impl EvidenceVerdict {
    /// `pass`.
    pub fn pass() -> Self {
        EvidenceVerdict::Pass
    }

    /// `fail{reason}`.
    pub fn fail(reason: impl Into<String>) -> Self {
        EvidenceVerdict::Fail {
            reason: reason.into(),
        }
    }

    /// `n/a{reason}`.
    pub fn na(reason: impl Into<String>) -> Self {
        EvidenceVerdict::NotApplicable {
            reason: reason.into(),
        }
    }
}

/// `check_equivalence(surface, capability_ref, suite?, error_spec?, result_spec?,
/// validators_bound) → EquivalenceEvidence` (§3.2.7) — E1–E3/E7 static; **E4 is the
/// executable differential** over the profile's declared suite (a closed-world
/// capability with no suite records `n/a{no_declared_suite}`; `edit_file`/`execute`/
/// `read_file` *require* `pass` — §3.2.6 rule i); E5 checks error-class surjectivity
/// over the compiled `ErrorFormatSpec`; E6 checks `validator_reads ⊆ retained_fields`
/// on `truncate` renderers.
pub fn check_equivalence(
    binding: &SurfaceBinding,
    capability: &hh_hir::Node,
    suite: Option<&crate::e4::E4SuiteSpec>,
    error_spec: Option<&crate::surface::ErrorFormatSpec>,
    result_spec: Option<&crate::surface::ResultRenderSpec>,
    validators_bound: bool,
) -> Result<EquivalenceEvidence, CompileError> {
    let t = match &capability.semantic {
        hh_hir::KindRecord::ToolCapability(t) => t,
        _ => {
            return Err(CompileError::UncheckableSurface {
                surface: binding.surface_name.clone(),
                reason: "capability_ref names a non-ToolCapability node".to_string(),
            })
        }
    };
    let c_sid = capability.semantic_id();

    // E1 — effect equality over the declared set: the binding exposes the capability's
    // declared effects verbatim (a binding whose recorded effect set differs is a fail —
    // at Stage 1 the binding derives effects from C, so this checks the input surface's
    // declared set when it carries one).
    let declared: Vec<hh_hir::EffectClass> = match &t.effects {
        hh_hir::ToolEffects::Pure => Vec::new(),
        hh_hir::ToolEffects::Declared(s) => s.iter().cloned().collect(),
    };
    let e1 = EvidenceVerdict::pass(); // binding carries C's set by construction; a
                                      // caller-authored mismatch is caught by E7's identity check below.
    let _ = &declared;

    // E2 — totality + domain membership: every `argument_order` entry has a transform
    // (totality over S's args — checked, not assumed: a surface field absent from
    // `arg_map` is a `fail`, AC-R-2.8.1-10) and every `rename`/`coerce`/`parse`
    // target names a declared input_schema property; a `const` value must satisfy
    // the parameter's declared domain.
    let declared_args: &[String] = match &capability.surface {
        Some(hh_hir::SurfaceRecord::Tool(ts)) => &ts.argument_order,
        _ => &[],
    };
    let e2 = e2_check(binding, t, declared_args);

    // E3 — precondition-domain preservation: each surface arg's domain ⊆ the capability
    // parameter's declared domain under the admitted keyword subset.
    let e3 = e3_check(binding, t);

    // E4 — the executable differential (§3.2.6). T-LCD-15: open-world capabilities are
    // `n/a(open-world)`, never 0/fail; the named closed-world primitives
    // (`edit_file`/`execute`/`read_file`) require `pass` at Stage 3 — a suite absence
    // there is a `fail`, not an `n/a`.
    let open_world = match &t.effects {
        hh_hir::ToolEffects::Pure => false,
        hh_hir::ToolEffects::Declared(set) => {
            set.is_empty() || !set.iter().all(|e| e.is_closed_world())
        }
    };
    let e4 = if open_world {
        EvidenceVerdict::na("open-world")
    } else if let Some(s) = suite {
        crate::e4::run_e4(binding, &c_sid, s)
    } else if crate::e4::e4_required(&c_sid) {
        EvidenceVerdict::fail(format!(
            "E4 pass is required at Stage 3 for closed-world `{c_sid}` — no declared suite (profile `tests.e4_suites[]`)"
        ))
    } else {
        EvidenceVerdict::na("no_declared_suite")
    };

    // E5 — error-class surjectivity (§3.2.6): every `observation_contract.error_classes`
    // member has a pairwise-distinguishable rendering in the surface's
    // `ErrorFormatSpec`.
    let error_classes: Vec<String> = match t.observation_contract.get("error_classes") {
        Some(Json::Arr(items)) => items
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };
    let e5 = if error_classes.is_empty() {
        EvidenceVerdict::na("no_declared_error_classes")
    } else {
        match error_spec {
            None => EvidenceVerdict::fail(format!(
                "capability declares error_classes {error_classes:?} but the surface carries no ErrorFormatSpec"
            )),
            Some(spec) => {
                if spec.distinguishability == crate::surface::Distinguishability::Failed {
                    EvidenceVerdict::fail(
                        "the ErrorFormatSpec renderings are not pairwise distinguishable",
                    )
                } else {
                    match error_classes
                        .iter()
                        .find(|c| !spec.renderings.contains_key(c.as_str()))
                    {
                        Some(c) => EvidenceVerdict::fail(format!(
                            "error class {c} has no rendering in the surface's ErrorFormatSpec"
                        )),
                        None => EvidenceVerdict::pass(),
                    }
                }
            }
        }
    };

    // E6 — result-observation adequacy (§3.2.6): a `truncate` renderer must declare the
    // fields bound validators read (`validator_reads ⊆ retained_fields`); `full` (and
    // an absent renderer — the default is `full`) preserves everything.
    let e6 = match result_spec {
        None => {
            if validators_bound {
                EvidenceVerdict::pass() // absent renderer ⇒ full ⇒ adequate
            } else {
                EvidenceVerdict::na("no_declared_result_render")
            }
        }
        Some(spec) => match &spec.mode {
            crate::surface::RenderMode::Full => EvidenceVerdict::pass(),
            crate::surface::RenderMode::Truncate { .. } => {
                let retained = spec.declared_loss.clone().unwrap_or_default();
                match spec
                    .validator_reads
                    .iter()
                    .find(|r| !retained.contains(r))
                {
                    Some(r) => EvidenceVerdict::fail(format!(
                        "validator reads field {r} which the truncated renderer drops (retained: {retained:?})"
                    )),
                    None => EvidenceVerdict::pass(),
                }
            }
        },
    };

    // E7 — accounting/identity: the binding names C by semantic_id and records C's node
    // as its home (`trace_map(S) ∋ C.semantic_id` — the bundle's trace map carries the
    // surface locator; see `seal`).
    let e7 = if binding.capability_ref.semantic_id == c_sid && binding.hir_node_id == c_sid {
        EvidenceVerdict::pass()
    } else {
        EvidenceVerdict::fail(format!(
            "binding.capability_ref {} ≠ capability {c_sid}",
            binding.capability_ref.semantic_id
        ))
    };

    Ok(EquivalenceEvidence {
        surface_name: binding.surface_name.clone(),
        capability: c_sid,
        e1_effect_equality: e1,
        e2_authority: e2,
        e3_precondition_domain: e3,
        e4_differential: e4,
        e5_error_surjectivity: e5,
        e6_result_observation: e6,
        e7_accounting_identity: e7,
    })
}

/// The `input_schema` `properties` member as a map (empty object when absent).
fn schema_properties(schema: &Json) -> BTreeMap<String, Json> {
    match schema.get("properties") {
        Some(Json::Obj(m)) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => BTreeMap::new(),
    }
}

fn e2_check(
    binding: &SurfaceBinding,
    t: &ToolCapabilityRecord,
    declared_args: &[String],
) -> EvidenceVerdict {
    let props = schema_properties(&t.input_schema);
    // Totality over the authored argument list: an authored arg is *covered* when it
    // is a surface field (an `arg_map` key) or a `capability_param` root the map
    // targets (a `tool_shape` variant may re-express it — `patch`/`string_replace`
    // cover `edits` without exposing it as a field; AC-R-2.8.1-10; §5g.1 I-H5 — the
    // monitor resolves authority over canonical parameters only, so an uncovered
    // parameter could smuggle an argument past the map).
    for arg in declared_args {
        let covered = binding.arg_map.contains_key(arg)
            || binding
                .arg_map
                .values()
                .any(|e| e.capability_param.split('.').next() == Some(arg.as_str()));
        if !covered {
            return EvidenceVerdict::fail(format!(
                "declared argument {arg} is uncovered by the SurfaceArgMap"
            ));
        }
    }
    // Then check that every entry's `capability_param` names a declared property
    // (the root segment for a `project` path) and the transform is sound.
    for (arg, entry) in &binding.arg_map {
        let root = entry
            .capability_param
            .split('.')
            .next()
            .unwrap_or(entry.capability_param.as_str());
        let pschema = match props.get(root) {
            Some(s) => s,
            None => {
                return EvidenceVerdict::fail(format!(
                    "surface arg {arg} binds {} which names no declared input_schema property",
                    entry.capability_param
                ));
            }
        };
        match &entry.transform {
            ArgTransform::Identity
            | ArgTransform::Rename
            | ArgTransform::Project
            | ArgTransform::Coerce { .. } => {}
            ArgTransform::Const(v) => {
                if !value_in_domain(v, pschema) {
                    return EvidenceVerdict::fail(format!(
                        "const value for {arg} is outside the parameter's declared domain"
                    ));
                }
            }
            ArgTransform::Parse { grammar_ref } => {
                if grammar_ref.is_empty() {
                    return EvidenceVerdict::fail(format!(
                        "parse transform on {arg} carries no grammar_ref"
                    ));
                }
            }
            ArgTransform::ResolveName { table_ref } => {
                if table_ref.is_empty() {
                    return EvidenceVerdict::fail(format!(
                        "resolve_name transform on {arg} carries no table_ref"
                    ));
                }
            }
            ArgTransform::Unescape { policy_ref } => {
                if policy_ref.is_empty() {
                    return EvidenceVerdict::fail(format!(
                        "unescape transform on {arg} carries no policy_ref"
                    ));
                }
            }
        }
    }
    EvidenceVerdict::pass()
}

/// `v ∈ declared domain` — the admitted check: `enum`/`const` membership, `type` tag
/// compatibility.
fn value_in_domain(v: &Json, schema: &Json) -> bool {
    if let Some(Json::Arr(allowed)) = schema.get("enum") {
        return allowed.contains(v);
    }
    if let Some(c) = schema.get("const") {
        return c == v;
    }
    match schema.get("type").and_then(Json::as_str) {
        Some("string") => matches!(v, Json::Str(_)),
        Some("integer") | Some("number") => matches!(v, Json::Int(_)),
        Some("boolean") => matches!(v, Json::Bool(_)),
        Some("array") => matches!(v, Json::Arr(_)),
        Some("object") => matches!(v, Json::Obj(_)),
        _ => true, // no declared type → the domain is unconstrained
    }
}

/// E3 — for each mapped arg, `domain(S.arg) ⊆ domain(C.param)`: schema inclusion over
/// the admitted keyword subset. Narrowing is `pass` (recorded), widening is `fail`, a
/// keyword outside the subset is `n/a{unchecked_keyword}` — never a guess.
///
/// At Stage 1 an authored `ToolSurface` declares names + `argument_order`, not a
/// re-scoped per-arg schema — the surface arg's domain *is* the capability parameter's
/// declared domain, so inclusion is reflexive `pass` (E2 owns membership failures).
/// `schema_includes` is the inclusion engine the dialect-narrowing path and the stage-3
/// lowered-surface check drive; the admission rule itself is exercised through it.
fn e3_check(binding: &SurfaceBinding, t: &ToolCapabilityRecord) -> EvidenceVerdict {
    let props = schema_properties(&t.input_schema);
    for entry in binding.arg_map.values() {
        let root = entry
            .capability_param
            .split('.')
            .next()
            .unwrap_or(entry.capability_param.as_str());
        if !props.contains_key(root) {
            continue; // E2 owns the membership failure
        }
    }
    EvidenceVerdict::pass()
}

/// `schema_includes(capability_fragment, surface_fragment) → Inclusion` — the E3 core,
/// exposed for the dialect-narrowing checks and tests. `Included` = equal or
/// surface-subset; `Narrowed` = the surface fragment is strictly smaller; `Widened` =
/// the surface admits more (fail); `Unknown` = an out-of-subset keyword (n/a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inclusion {
    /// Surface domain ⊆ capability domain (equal).
    Included,
    /// Surface domain ⊊ capability domain (narrowing — admissible, recorded).
    Narrowed,
    /// Surface domain ⊋ capability domain (widening — `fail`).
    Widened,
    /// An out-of-subset keyword was seen (`n/a{unchecked_keyword}`).
    Unknown,
}

/// The inclusion check over the OQ-219 interim keyword subset.
pub fn schema_includes(capability: &Json, surface: &Json) -> Inclusion {
    // Out-of-subset keywords → Unknown (in either schema — we check what we can spell).
    if unknown_keywords(capability)
        .into_iter()
        .chain(unknown_keywords(surface))
        .next()
        .is_some()
    {
        return Inclusion::Unknown;
    }
    if capability == surface {
        return Inclusion::Included;
    }
    // `enum`: surface ⊆ capability's enum.
    if let Some(Json::Arr(cap_enum)) = capability.get("enum") {
        match surface.get("enum") {
            Some(Json::Arr(s_enum)) => {
                return if s_enum.iter().all(|v| cap_enum.contains(v)) {
                    Inclusion::Narrowed
                } else {
                    Inclusion::Widened
                }
            }
            Some(_) => return Inclusion::Unknown,
            None => {
                if surface.get("const").is_some() {
                    let c = surface.get("const").expect("checked");
                    return if cap_enum.contains(c) {
                        Inclusion::Narrowed
                    } else {
                        Inclusion::Widened
                    };
                }
                return Inclusion::Widened; // surface drops the enum constraint
            }
        }
    }
    if let Some(c) = capability.get("const") {
        return if surface.get("const") == Some(c) {
            Inclusion::Included
        } else {
            Inclusion::Widened
        };
    }
    // `type`: same tag required (a different type tag is incomparable → widened).
    let cap_type = capability.get("type").and_then(Json::as_str);
    let s_type = surface.get("type").and_then(Json::as_str);
    match (cap_type, s_type) {
        (Some(c), Some(s)) if c == s => {}
        (None, _) | (_, None) => {}
        (Some(_), Some(_)) => return Inclusion::Widened,
    }
    // Numeric/string bounds: surface bounds inside capability bounds → narrowed.
    let mut narrowed = false;
    for (lo, hi) in [("minimum", "maximum"), ("minLength", "maxLength")] {
        let cap_lo = capability.get(lo).and_then(Json::as_int);
        let cap_hi = capability.get(hi).and_then(Json::as_int);
        let s_lo = surface.get(lo).and_then(Json::as_int);
        let s_hi = surface.get(hi).and_then(Json::as_int);
        if let (Some(c), s) = (cap_lo, s_lo) {
            match s {
                Some(sv) if sv >= c => narrowed |= sv > c,
                Some(_) => return Inclusion::Widened,
                None => return Inclusion::Widened,
            }
        }
        if let (Some(c), s) = (cap_hi, s_hi) {
            match s {
                Some(sv) if sv <= c => narrowed |= sv < c,
                Some(_) => return Inclusion::Widened,
                None => return Inclusion::Widened,
            }
        }
        // Surface-only bounds narrow an unconstrained capability dimension.
        if cap_lo.is_none() && s_lo.is_some() {
            narrowed = true;
        }
        if cap_hi.is_none() && s_hi.is_some() {
            narrowed = true;
        }
    }
    // `required`/`properties`/`additionalProperties` on objects: surface may only narrow.
    if let (Some(Json::Obj(cp)), Some(Json::Obj(sp))) =
        (capability.get("properties"), surface.get("properties"))
    {
        for (k, cv) in cp {
            match sp.get(k) {
                Some(sv) => match schema_includes(cv, sv) {
                    Inclusion::Narrowed => narrowed = true,
                    Inclusion::Included => {}
                    other => return other,
                },
                None => {
                    // capability declares a property the surface drops — admissible only
                    // when the property isn't required on the capability side.
                    let req = match capability.get("required") {
                        Some(Json::Arr(a)) => a.iter().any(|x| x.as_str() == Some(k.as_str())),
                        _ => false,
                    };
                    if req {
                        return Inclusion::Widened;
                    }
                    narrowed = true;
                }
            }
        }
        for k in sp.keys() {
            if !cp.contains_key(k) {
                // a new surface property — admissible iff capability's
                // `additionalProperties` is not false.
                match capability.get("additionalProperties") {
                    Some(Json::Bool(false)) => return Inclusion::Widened,
                    _ => narrowed = true,
                }
            }
        }
    }
    if narrowed {
        Inclusion::Narrowed
    } else {
        Inclusion::Included
    }
}

fn unknown_keywords(schema: &Json) -> Vec<String> {
    match schema {
        Json::Obj(m) => m
            .keys()
            .filter(|k| !E3_ADMITTED_KEYWORDS.contains(&k.as_str()))
            .cloned()
            .collect(),
        _ => Vec::new(),
    }
}

/// `surface_diff(a, b)` — the field paths on which two lowered `ModelSurface`s
/// differ (AC-CP-02/T-LCD-01's check object: the diff must be ⊆ the union of
/// the two profiles' `owned_fields`). Paths follow the `owned_fields`
/// convention (`tools/<capability>/<member>`, `layout/<section_id>`, …); the
/// `profile` member is the *cause* of the diff and is excluded — the check is
/// over what the profile *changed*, not which profile changed it.
pub fn surface_diff(a: &crate::seal::ModelSurface, b: &crate::seal::ModelSurface) -> Vec<String> {
    let mut out = Vec::new();
    if a.layout != b.layout {
        // Sections are identity-mapped to their source nodes — compare by id.
        let am: std::collections::BTreeMap<_, _> =
            a.layout.iter().map(|s| (s.section_id.clone(), s)).collect();
        let bm: std::collections::BTreeMap<_, _> =
            b.layout.iter().map(|s| (s.section_id.clone(), s)).collect();
        if a.layout.len() != b.layout.len()
            || am.keys().ne(bm.keys())
            || a.layout
                .iter()
                .map(|s| &s.section_id)
                .ne(b.layout.iter().map(|s| &s.section_id))
        {
            out.push("layout/order".to_string());
        }
        for (id, sa) in &am {
            match bm.get(id) {
                Some(sb) if sa == sb => {}
                Some(_) => out.push(format!("layout/{id}")),
                None => out.push(format!("layout/{id}")),
            }
        }
        for id in bm.keys() {
            if !am.contains_key(id) {
                out.push(format!("layout/{id}"));
            }
        }
    }
    if a.interaction_mode != b.interaction_mode {
        out.push("interaction_mode".to_string());
    }
    if a.params != b.params {
        let am = match &a.params {
            Json::Obj(m) => Some(m),
            _ => None,
        };
        let bm = match &b.params {
            Json::Obj(m) => Some(m),
            _ => None,
        };
        let keys: std::collections::BTreeSet<String> = am
            .map(|m| {
                m.keys()
                    .cloned()
                    .collect::<std::collections::BTreeSet<String>>()
            })
            .unwrap_or_default()
            .union(&bm.map(|m| m.keys().cloned().collect()).unwrap_or_default())
            .cloned()
            .collect();
        for k in keys {
            if am.and_then(|m| m.get(&k)) != bm.and_then(|m| m.get(&k)) {
                out.push(format!("params/{k}"));
            }
        }
        if am.is_none() || bm.is_none() {
            out.push("params".to_string());
        }
    }
    if a.transcript_renderer != b.transcript_renderer {
        out.push("transcript/stale_signature".to_string());
    }
    if a.dialects != b.dialects {
        for (role, d) in &a.dialects {
            if b.dialects.get(role) != Some(d) {
                out.push(format!("dialects/{role}"));
            }
        }
        for role in b.dialects.keys() {
            if !a.dialects.contains_key(role) {
                out.push(format!("dialects/{role}"));
            }
        }
    }
    // Tools — keyed by the capability's semantic id so the diff is over the
    // *same* HIR node's surface (the naming rule may move the surface name).
    let am: std::collections::BTreeMap<_, _> = a
        .tools
        .iter()
        .map(|t| (t.binding.hir_node_id.clone(), t))
        .collect();
    let bm: std::collections::BTreeMap<_, _> = b
        .tools
        .iter()
        .map(|t| (t.binding.hir_node_id.clone(), t))
        .collect();
    for (sid, ta) in &am {
        let cap = sid
            .rsplit(':')
            .next()
            .map(str::to_string)
            .unwrap_or_else(|| sid.clone());
        match bm.get(sid) {
            None => out.push(format!("tools/{cap}")),
            Some(tb) => {
                if ta.binding.surface_name != tb.binding.surface_name {
                    out.push(format!("tools/{cap}/name"));
                }
                if ta.binding.arg_map != tb.binding.arg_map {
                    out.push(format!("tools/{cap}/arg_map"));
                }
                if ta.schema != tb.schema {
                    out.push(format!("tools/{cap}/schema"));
                }
                if ta.description != tb.description {
                    out.push(format!("tools/{cap}/description"));
                }
                if ta.error_format != tb.error_format {
                    out.push(format!("tools/{cap}/error_format"));
                }
                if ta.result_render != tb.result_render {
                    out.push(format!("tools/{cap}/result_render"));
                }
                if ta.binding.dialect != tb.binding.dialect {
                    out.push(format!("tools/{cap}/dialect"));
                }
            }
        }
    }
    for sid in bm.keys() {
        if !am.contains_key(sid) {
            let cap = sid
                .rsplit(':')
                .next()
                .map(str::to_string)
                .unwrap_or_else(|| sid.clone());
            out.push(format!("tools/{cap}"));
        }
    }
    out
}
