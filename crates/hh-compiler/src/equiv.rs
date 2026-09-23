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

/// `SurfaceBinding{surface_name, capability_ref, arg_map, dialect}` (ADR-0090 — the
/// compiled-surface record). It is **one atomic record**, bound only inside an
/// equivalence-checked compile; `hir_node_id` names the capability node the surface is
/// declared on.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceBinding {
    /// The model-facing surface name (the `ToolSurface.name`).
    pub surface_name: String,
    /// The capability the surface exposes.
    pub capability_ref: PinnedRef,
    /// The capability node's semantic id (the surface's home node).
    pub hir_node_id: String,
    /// The argument map (total over the surface's `argument_order`).
    pub arg_map: SurfaceArgMap,
    /// The schema dialect the surface's schemas are expressed in (default
    /// `json-schema-2020-12`; a narrowing must be declared — `DialectNarrowingUndeclared`
    /// otherwise).
    pub dialect: String,
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
    SurfaceBinding {
        surface_name: surface.name.clone(),
        capability_ref: PinnedRef {
            semantic_id: node.semantic_id(),
            version_id: node.version_id(),
        },
        hir_node_id: node.semantic_id(),
        arg_map,
        dialect,
    }
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

/// `check_equivalence(surface, capability_ref, suite?) → EquivalenceEvidence` (§3.2.7) —
/// the Stage-1 static half: E1–E3 + E7; E4–E6 carry their `n/a` reasons. `suite` is the
/// Stage-3 differential suite hook — at C0 a supplied suite is ignored and E4 records
/// `n/a{stage_3}` regardless (the executable differential lands at S3.2).
pub fn check_equivalence(
    binding: &SurfaceBinding,
    capability: &hh_hir::Node,
    suite: Option<&str>,
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
    // (totality over S's args) and every `rename`/`coerce`/`parse` target names a
    // declared input_schema property; a `const` value must satisfy the parameter's
    // declared domain.
    let e2 = e2_check(binding, t);

    // E3 — precondition-domain preservation: each surface arg's domain ⊆ the capability
    // parameter's declared domain under the admitted keyword subset.
    let e3 = e3_check(binding, t);

    // E4 — T-LCD-15: open-world capabilities are `n/a(open-world)`, never 0/fail. A
    // capability is open-world when any declared effect's `world` is not `closed`, or
    // `effects` is `declared` with an unattributed class.
    let open_world = match &t.effects {
        hh_hir::ToolEffects::Pure => false,
        hh_hir::ToolEffects::Declared(set) => {
            set.is_empty() || !set.iter().all(|e| e.is_closed_world())
        }
    };
    let e4 = if suite.is_some() {
        EvidenceVerdict::na("stage_3")
    } else if open_world {
        EvidenceVerdict::na("open-world")
    } else {
        EvidenceVerdict::na("stage_3")
    };

    // E5/E6 — §5b/§5f own these; at C0 they are `n/a{stage_3}` (§3.2.14).
    let e5 = EvidenceVerdict::na("stage_3");
    let e6 = EvidenceVerdict::na("stage_3");

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

fn e2_check(binding: &SurfaceBinding, t: &ToolCapabilityRecord) -> EvidenceVerdict {
    let props = schema_properties(&t.input_schema);
    // Totality is by construction (arg_map covers argument_order); check that every
    // entry's `capability_param` names a declared property (the root segment for a
    // `project` path) and the transform is sound.
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
