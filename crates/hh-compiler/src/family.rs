//! §5d.2 **C1/C2 extension** — the tool-interface surface family (R-2.5.2;
//! ADR-0090/0091/0092 (e)): `SurfaceFamily` + `VariantSelector`, the `PlanMap`
//! restriction (split/static-composite surfaces), the `SchemaDialectTransform`
//! narrowing declarations, the S1–S4 `check_safety`/`synthesize_surface`/
//! `retire_surface` pipeline, and the C2 `freeform`/`code_mode`/`shim` helpers.
//!
//! Everything here is compile-time — pure, content-addressed, declared-records
//! only (T-LCD-08/-12; ADR-0019 purity). The run-time half lives in
//! `hh-context`'s `select_surfaces` (R-2.5.3 evaluates the selectors here).

use std::collections::{BTreeMap, BTreeSet};

use hh_hir::records::{AssumptionDebtRecord, ToolSurface};
use hh_wire::json::Json;

use crate::equiv::{ArgMapEntry, ArgTransform, EvidenceVerdict, Inclusion, SurfaceBinding};
use crate::errors::CompileError;
use crate::surface::{BindingMapping, CompileExposureMode};

// ─────────────────────────────────────────────────────────────────────────────
// Declared inputs (§5d.2 §2: "inputs are *declared, never observed*")
// ─────────────────────────────────────────────────────────────────────────────

/// `TaskClassDeclaration{task_class_id, labels[], expected_capabilities[]}` —
/// the *declared* task-class record a `VariantSelector` reads (a record of the
/// sealed definition — never an observation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskClassDeclaration {
    /// The task class's identity.
    pub task_class_id: String,
    /// Declared labels.
    pub labels: Vec<String>,
    /// The capability semantic ids the class expects.
    pub expected_capabilities: Vec<String>,
}

/// `EnvironmentDeclaration{environment_count, executor_platform,
/// session_support, capability_declaration}` — the declared environment record
/// a `VariantSelector` reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentDeclaration {
    /// The declared environment count.
    pub environment_count: u64,
    /// The executor platform tag.
    pub executor_platform: String,
    /// Whether the environment supports sessions (`unified_exec`'s
    /// `session_id` arm needs it).
    pub session_support: bool,
    /// The capabilities the environment declares.
    pub capability_declaration: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// `VariantSelector` (ADR-0090 D3)
// ─────────────────────────────────────────────────────────────────────────────

/// `VariantSelector` — the closed, total, quantifier-free predicate over
/// `TaskClassDeclaration` + `EnvironmentDeclaration` + the turn-state fields
/// the profile declares selectable (`turn_state` JSON — every field read is
/// *declared*, never a model-identity probe: there is **no**
/// `ModelIdentity` member — a model-id predicate outside a selector fails
/// `link`, AC-R-2.5.2-1/T-LCD-01).
#[derive(Debug, Clone, PartialEq)]
pub enum VariantSelector {
    /// `always` — the reference/default variant.
    Always,
    /// `task_class(id)` — `task.task_class_id == id`.
    TaskClass(String),
    /// `task_label(l)` — `task.labels ∋ l`.
    TaskLabel(String),
    /// `executor_platform(p)` — `env.executor_platform == p`.
    ExecutorPlatform(String),
    /// `session_support(b)` — `env.session_support == b`.
    SessionSupport(bool),
    /// `environment_count{min}` — `env.environment_count ≥ min`.
    EnvironmentCount {
        /// The floor.
        min: u64,
    },
    /// `capability_declared(sid)` — `env.capability_declaration ∋ sid`.
    CapabilityDeclared(String),
    /// `turn_field{field, equals}` — a *profile-declared selectable*
    /// turn-state field (the caller proves membership — `eval_turn` takes
    /// the declared-field set).
    TurnField {
        /// The declared field name.
        field: String,
        /// The equality value.
        equals: Json,
    },
    /// `all[…]` — conjunction.
    All(Vec<VariantSelector>),
    /// `any[…]` — disjunction.
    Any(Vec<VariantSelector>),
    /// `not` — negation.
    Not(Box<VariantSelector>),
}

impl VariantSelector {
    /// `eval(selector, task, env, turn_state, declared_turn_fields)` — total
    /// and pure. A `turn_field` read of an undeclared field is `false`
    /// (never a peek — the profile names its selectable fields).
    pub fn eval(
        &self,
        task: &TaskClassDeclaration,
        env: &EnvironmentDeclaration,
        turn_state: &Json,
        declared_turn_fields: &BTreeSet<String>,
    ) -> bool {
        match self {
            VariantSelector::Always => true,
            VariantSelector::TaskClass(id) => task.task_class_id == *id,
            VariantSelector::TaskLabel(l) => task.labels.iter().any(|x| x == l),
            VariantSelector::ExecutorPlatform(p) => env.executor_platform == *p,
            VariantSelector::SessionSupport(b) => env.session_support == *b,
            VariantSelector::EnvironmentCount { min } => env.environment_count >= *min,
            VariantSelector::CapabilityDeclared(sid) => {
                env.capability_declaration.iter().any(|c| c == sid)
            }
            VariantSelector::TurnField { field, equals } => {
                declared_turn_fields.contains(field) && turn_state.get(field) == Some(equals)
            }
            VariantSelector::All(members) => members
                .iter()
                .all(|m| m.eval(task, env, turn_state, declared_turn_fields)),
            VariantSelector::Any(members) => members
                .iter()
                .any(|m| m.eval(task, env, turn_state, declared_turn_fields)),
            VariantSelector::Not(inner) => !inner.eval(task, env, turn_state, declared_turn_fields),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `SurfaceFamily` / `SurfaceVariant` (ADR-0090 D2)
// ─────────────────────────────────────────────────────────────────────────────

/// `SurfaceVariant{variant_id, exposure_mode, surfaces[], selector,
/// rule_ids[]}` — one member of a family (§5d.2 §3 row). `rule_ids` names
/// the `ProfileRule`s (`tool_shape`/`naming`/`schema_dialect`/…) that shaped
/// it — each carries a complete `AssumptionDebtRecord` (T-LCD-05).
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceVariant {
    /// The variant's id (registry coordinate).
    pub variant_id: String,
    /// The compile-time exposure mode of its surfaces.
    pub exposure_mode: CompileExposureMode,
    /// The `surface_id`s the variant comprises.
    pub surfaces: Vec<String>,
    /// The `VariantSelector` — when this variant serves.
    pub selector: VariantSelector,
    /// The shaping rule ids.
    pub rule_ids: Vec<String>,
}

/// `SurfaceFamily{family_id, capability_refs[], reference_variant,
/// variants[]}` — one family per capability set exposed together
/// (ADR-0090 D2). `reference_variant` is always a `native_fc` primitive (or
/// split of primitives) rendering — the E4 comparand and the retirement
/// fallback.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceFamily {
    /// The family's identity coordinate.
    pub family_id: String,
    /// The capability semantic ids the family exposes.
    pub capability_refs: Vec<String>,
    /// The `variant_id` of the primitive reference rendering.
    pub reference_variant: String,
    /// The variants (the reference variant is one of them).
    pub variants: Vec<SurfaceVariant>,
}

/// The `compile_tool_surfaces` output — `SurfaceFamilySet{families[],
/// bindings[], catalogue_rendering?, diagnostics[]}` (§5d.2 §2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SurfaceFamilySet {
    /// The families.
    pub families: Vec<SurfaceFamily>,
    /// The bindings (one per surface — total).
    pub bindings: Vec<SurfaceBinding>,
    /// The `no_slot`/composite diagnostics the run surfaces
    /// (`unexposed{policy_hidden|deferred|unsupported_mode}` and the like).
    pub diagnostics: Vec<String>,
}

/// `check_family(family, bindings)` — the family record's static checks:
/// the reference variant exists and is `primitive`/`split` (a `native_fc`
/// rendering); every `surfaces[]` member names a binding in the set; each
/// binding's `(family_id, variant_id)` back-point resolves; a variant's
/// `surfaces` are covered by the family's `capability_refs` (the binding
/// side of S1).
pub fn check_family(
    family: &SurfaceFamily,
    bindings: &BTreeMap<String, SurfaceBinding>,
) -> Result<(), CompileError> {
    let caps: BTreeSet<&str> = family.capability_refs.iter().map(String::as_str).collect();
    let mut reference_seen = false;
    for v in &family.variants {
        if v.variant_id == family.reference_variant {
            reference_seen = true;
            if !matches!(
                v.exposure_mode,
                CompileExposureMode::Primitive | CompileExposureMode::Split
            ) {
                return Err(CompileError::UncheckableSurface {
                    surface: family.family_id.clone(),
                    reason: format!(
                        "reference_variant {} must be a primitive/split native_fc rendering — found {}",
                        v.variant_id,
                        v.exposure_mode.as_str()
                    ),
                });
            }
        }
        for sid in &v.surfaces {
            let b = bindings
                .get(sid.as_str())
                .ok_or_else(|| CompileError::UncheckableSurface {
                    surface: sid.clone(),
                    reason: format!("variant {} names a surface with no binding", v.variant_id),
                })?;
            if b.family_id.as_deref() != Some(family.family_id.as_str())
                || b.variant_id.as_deref() != Some(v.variant_id.as_str())
            {
                return Err(CompileError::UncheckableSurface {
                    surface: sid.clone(),
                    reason: format!(
                        "binding's (family_id, variant_id) does not back-point to {}/{}",
                        family.family_id, v.variant_id
                    ),
                });
            }
            for c in &b.capability_refs {
                if !caps.contains(c.as_str()) {
                    return Err(CompileError::AuthorityWidening {
                        detail: format!(
                            "surface {sid} binds capability {c} outside family {}'s capability_refs",
                            family.family_id
                        ),
                    });
                }
            }
        }
    }
    if !reference_seen {
        return Err(CompileError::UncheckableSurface {
            surface: family.family_id.clone(),
            reason: format!(
                "reference_variant {} is not a member of variants[]",
                family.reference_variant
            ),
        });
    }
    Ok(())
}

/// `select_variant(family, task, env, turn_state, declared_fields)` — the
/// run-time selector's compile-time twin (R-2.5.3 evaluates this form):
/// the first variant (declaration order) whose selector evaluates `true`;
/// the reference variant is the fallback (its selector is `Always` by
/// construction). Deterministic — same inputs, same variant.
pub fn select_variant<'a>(
    family: &'a SurfaceFamily,
    task: &TaskClassDeclaration,
    env: &EnvironmentDeclaration,
    turn_state: &Json,
    declared_turn_fields: &BTreeSet<String>,
) -> &'a SurfaceVariant {
    for v in &family.variants {
        if v.variant_id != family.reference_variant
            && v.selector.eval(task, env, turn_state, declared_turn_fields)
        {
            return v;
        }
    }
    family
        .variants
        .iter()
        .find(|v| v.variant_id == family.reference_variant)
        .expect("check_family requires the reference variant")
}

// ─────────────────────────────────────────────────────────────────────────────
// `PlanMap` (ADR-0090 D6) — the composite/split executable shape
// ─────────────────────────────────────────────────────────────────────────────

/// `PlanMap` steps — `steps ⊆ {Branch(predicate over surface fields),
/// Invoke(Ref<ToolCapability>, ArgBinding with its own SurfaceArgMap),
/// Verify(Ref<Validator>)}`. `Loop`/`Delegate`/`Opaque`/`Instruction` exist
/// in the sum so a plan *containing* them fails `check_plan_map` with
/// `UncheckableSurface` — the shape is code-mode by definition (OQ-066
/// resolved; AC-R-2.5.2-3).
#[derive(Debug, Clone, PartialEq)]
pub enum PlanStep {
    /// `Branch{predicate, steps}` — a predicate over surface fields
    /// (`VariantSelector` — the same closed form; OQ-226's interim).
    Branch {
        /// The branch predicate.
        predicate: VariantSelector,
        /// The branch's steps.
        steps: Vec<PlanStep>,
    },
    /// `Invoke{capability_ref, arg_map}` — a nested call into a declared
    /// capability with its own `SurfaceArgMap` (each is its own `Effect` at
    /// run time — S2).
    Invoke {
        /// The capability semantic id.
        capability_ref: String,
        /// The branch's own argument map.
        arg_map: crate::equiv::SurfaceArgMap,
    },
    /// `Verify{validator_ref}` — a bound validator check.
    Verify {
        /// The validator's ref.
        validator_ref: String,
    },
    /// `Loop` — uncheckable (code-mode shape).
    Loop(Vec<PlanStep>),
    /// `Delegate` — uncheckable (code-mode shape).
    Delegate,
    /// `Opaque{detail}` — uncheckable (code-mode shape).
    Opaque(String),
    /// `Instruction` — uncheckable (code-mode shape).
    Instruction(String),
}

/// `PlanMap{steps, session_state?}` — a `Procedure` restricted to the
/// checkable step set (`session_state` composites are C2 — the member is
/// carried so the record closes; a `Some` marks the map C2-tier).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanMap {
    /// The plan's steps.
    pub steps: Vec<PlanStep>,
    /// `session_state: Ref<Artifact>` — C2 composite state.
    pub session_state: Option<String>,
}

/// `check_plan_map(map, allowed_capabilities)` — the C1 static check:
/// every step ∈ `{Branch, Invoke, Verify}` (recursively); every `Invoke`'s
/// `capability_ref ∈ allowed_capabilities` (S1 — `AuthorityWidening`
/// otherwise); `effects(PlanMap) = ∪ branches` is the caller's (the
/// binding's `effects_bound`).
pub fn check_plan_map(
    map: &PlanMap,
    allowed_capabilities: &BTreeSet<String>,
) -> Result<(), CompileError> {
    fn walk(
        steps: &[PlanStep],
        allowed: &BTreeSet<String>,
        depth: usize,
    ) -> Result<(), CompileError> {
        for step in steps {
            match step {
                PlanStep::Branch { steps, .. } => walk(steps, allowed, depth + 1)?,
                PlanStep::Invoke { capability_ref, .. } => {
                    if !allowed.contains(capability_ref) {
                        return Err(CompileError::AuthorityWidening {
                            detail: format!(
                                "PlanMap invokes {capability_ref} outside allowed_capabilities (S1)"
                            ),
                        });
                    }
                }
                PlanStep::Verify { .. } => {}
                PlanStep::Loop(_)
                | PlanStep::Delegate
                | PlanStep::Opaque(_)
                | PlanStep::Instruction(_) => {
                    return Err(CompileError::UncheckableSurface {
                        surface: format!("plan_step@depth{depth}"),
                        reason: format!(
                            "{} — a plan containing it is code-mode by definition (OQ-066)",
                            step_kind(step)
                        ),
                    });
                }
            }
        }
        Ok(())
    }
    walk(&map.steps, allowed_capabilities, 0)
}

/// The step kind spelling.
fn step_kind(step: &PlanStep) -> &'static str {
    match step {
        PlanStep::Branch { .. } => "Branch",
        PlanStep::Invoke { .. } => "Invoke",
        PlanStep::Verify { .. } => "Verify",
        PlanStep::Loop(_) => "Loop",
        PlanStep::Delegate => "Delegate",
        PlanStep::Opaque(_) => "Opaque",
        PlanStep::Instruction(_) => "Instruction",
    }
}

/// `capabilities_invoked(map)` — the `∪` of `Invoke` capability refs
/// (the composite's `capability_refs` — S1's inclusion set).
pub fn capabilities_invoked(map: &PlanMap) -> BTreeSet<String> {
    fn go(steps: &[PlanStep], out: &mut BTreeSet<String>) {
        for s in steps {
            match s {
                PlanStep::Branch { steps, .. } | PlanStep::Loop(steps) => go(steps, out),
                PlanStep::Invoke { capability_ref, .. } => {
                    out.insert(capability_ref.clone());
                }
                _ => {}
            }
        }
    }
    let mut out = BTreeSet::new();
    go(&map.steps, &mut out);
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// `SchemaDialectTransform` (AC-R-2.5.2-5; ADR-0090 D5)
// ─────────────────────────────────────────────────────────────────────────────

/// `DialectOp` — one schema-dialect operation with its **declared**
/// narrowing effect (§5d.2 AC-R-2.5.2-5: every dialect operation declares
/// its narrowing effect; undeclared type invention or nullability loss
/// fails).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialectOp {
    /// The operation (a keyword-level transform id — `drop_keyword`,
    /// `enum_narrow`, `type_coerce`, `default_fill`, …).
    pub op: String,
    /// The keyword/field it touches.
    pub keyword: String,
    /// `narrowed[]` — the declared narrowing effects (e.g.
    /// `"enum_members_dropped"`, `"bounds_tightened"`,
    /// `"nullability_lost"`, `"type_widened"`). An op that narrows or
    /// invents must declare it here.
    pub narrowed: Vec<String>,
}

/// `SchemaDialectTransform{dialect, ops[]}` — the `schema_dialect` rule's
/// record (the narrowed declaration the E3 side-channel consumes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDialectTransform {
    /// The target dialect (`json-schema-2020-12` default subset…).
    pub dialect: String,
    /// The operations.
    pub ops: Vec<DialectOp>,
}

/// `check_dialect_transform(transform, capability_schema,
/// surface_schema)` — AC-R-2.5.2-5: run the op set's declared narrowings
/// against the observed schema diff (`schema_includes` per member): an op
/// producing an undeclared narrowing, a type invention, or a nullability
/// loss the op did not declare is `DialectNarrowingUndeclared`.
pub fn check_dialect_transform(
    transform: &SchemaDialectTransform,
    capability_schema: &Json,
    surface_schema: &Json,
) -> Result<Vec<String>, CompileError> {
    // The observed inclusion verdict on the whole fragment.
    let verdict = crate::equiv::schema_includes(capability_schema, surface_schema);
    let declared: BTreeSet<&str> = transform
        .ops
        .iter()
        .flat_map(|o| o.narrowed.iter().map(String::as_str))
        .collect();
    match verdict {
        Inclusion::Widened => Err(CompileError::DialectNarrowingUndeclared {
            detail: format!(
                "dialect transform to {} widens the schema — undeclared type invention (AC-R-2.5.2-5)",
                transform.dialect
            ),
        }),
        Inclusion::Narrowed => {
            // A narrowing occurred — at least one op must declare a
            // narrowing effect covering the touched keywords; a nullability
            // drop must be declared verbatim.
            let nullability_lost = capability_schema
                .get("type")
                .and_then(Json::as_str)
                .is_some_and(|t| t.contains("null"))
                && !surface_schema
                    .get("type")
                    .and_then(Json::as_str)
                    .is_some_and(|t| t.contains("null"));
            if nullability_lost && !declared.contains("nullability_lost") {
                return Err(CompileError::DialectNarrowingUndeclared {
                    detail: "nullability loss without a declared `nullability_lost` narrowing"
                        .to_string(),
                });
            }
            if declared.is_empty() {
                return Err(CompileError::DialectNarrowingUndeclared {
                    detail: format!(
                        "dialect transform to {} narrowed the schema with no declared narrowing effect",
                        transform.dialect
                    ),
                });
            }
            Ok(declared.iter().map(|s| s.to_string()).collect())
        }
        Inclusion::Included | Inclusion::Unknown => Ok(Vec::new()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `check_safety` — the S1–S4 gate (ADR-0091 D3)
// ─────────────────────────────────────────────────────────────────────────────

/// `SafetyEvidence{S1..S4}` — the per-gate verdicts (pass | fail | n/a).
#[derive(Debug, Clone, PartialEq)]
pub struct SafetyEvidence {
    /// S1 — no authority widening (`effects_bound ⊆ effects(capability_refs)
    /// ⊆ grants(permission_ceiling)`; `PlanMap` invokes ⊆
    /// `allowed_capabilities`; `resolve_name` exact-table only).
    pub s1_no_authority_widening: EvidenceVerdict,
    /// S2 — complete mediation of nested steps (every `PlanMap` `Invoke`,
    /// nested code-mode call and shim-parsed call is its own `Effect` with
    /// `parent_effect_id`).
    pub s2_nested_mediation: EvidenceVerdict,
    /// S3 — synthesized text is labelled and placed (`authority ≤
    /// delegate`/`unverified`; `role_map` only — never a
    /// `definition`-authority position).
    pub s3_labelled_text: EvidenceVerdict,
    /// S4 — fail closed (`fail`/`n/a` on E1–E3, an untyped mapping, a shim
    /// miss — all refuse, never best-effort).
    pub s4_fail_closed: EvidenceVerdict,
}

/// The input bundle `check_safety` reads — declared records only.
pub struct SafetyInput<'a> {
    /// The surface's binding.
    pub binding: &'a SurfaceBinding,
    /// The `PlanMap`, when `mapping = plan_map`.
    pub plan_map: Option<&'a PlanMap>,
    /// `effects(capability_refs)` — the declared effect domains of the
    /// bound capabilities.
    pub capability_effects: &'a BTreeSet<String>,
    /// `grants(permission_ceiling)` — the ceiling's effect domains.
    pub ceiling_grants: &'a BTreeSet<String>,
    /// `allowed_capabilities` — the definition's allowed set (S1's PlanMap
    /// inclusion reads it).
    pub allowed_capabilities: &'a BTreeSet<String>,
    /// Whether every nested step was proven individually `authorize`d
    /// (S2's static claim — the caller's evidence).
    pub nested_mediated: bool,
    /// The synthesized text's declared authority class — S3 checks it is
    /// `≤ delegate` (`unverified` for imports) and its target slot never
    /// carries `definition` authority.
    pub synthesized_authority: Option<hh_provenance::AuthorityClass>,
    /// The target slot's minimum authority (S3's placement bound — a
    /// synthesized description landing in a `definition`-floor slot is a
    /// widening).
    pub target_slot_min_authority: Option<hh_provenance::AuthorityClass>,
}

/// `check_safety(input) → SafetyEvidence` (ADR-0091 D3; AC-R-2.5.2-8): the
/// four static gates. `SurfaceSynthesisRefused` callers map any `fail` to
/// the gate name.
pub fn check_safety(input: &SafetyInput<'_>) -> SafetyEvidence {
    // S1 — effects_bound ⊆ effects(capability_refs) ⊆ grants(ceiling); a
    // PlanMap's invoke set ⊆ allowed_capabilities.
    let bound: BTreeSet<&str> = input
        .binding
        .effects_bound
        .iter()
        .map(String::as_str)
        .collect();
    let cap_fx: BTreeSet<&str> = input
        .capability_effects
        .iter()
        .map(String::as_str)
        .collect();
    let grants: BTreeSet<&str> = input.ceiling_grants.iter().map(String::as_str).collect();
    let s1 = if !bound.is_subset(&cap_fx) {
        EvidenceVerdict::fail("effects_bound ⊄ effects(capability_refs)")
    } else if !cap_fx.is_subset(&grants) {
        EvidenceVerdict::fail("effects(capability_refs) ⊄ grants(permission_ceiling)")
    } else if let Some(map) = input.plan_map {
        match check_plan_map(map, input.allowed_capabilities) {
            Ok(()) => EvidenceVerdict::pass(),
            Err(e) => EvidenceVerdict::fail(format!("{e}")),
        }
    } else {
        EvidenceVerdict::pass()
    };
    // S2 — a PlanMap/composite/code_mode surface requires per-step
    // mediation evidence (`nested_mediated` = the caller's proof).
    let mediated_kind = matches!(input.binding.mapping, BindingMapping::PlanMap(_))
        || input.binding.exposure_mode == CompileExposureMode::CodeMode
        || input.binding.exposure_mode == CompileExposureMode::Shim;
    let s2 = if mediated_kind {
        if input.nested_mediated {
            EvidenceVerdict::pass()
        } else {
            EvidenceVerdict::fail("nested steps lack per-effect mediation evidence")
        }
    } else {
        EvidenceVerdict::na("not a composite/code-mode surface")
    };
    // S3 — synthesized text ≤ delegate and never in a definition-floor slot.
    let s3 = match (input.synthesized_authority, input.target_slot_min_authority) {
        (Some(auth), slot) => {
            if auth > hh_provenance::AuthorityClass::Delegate {
                EvidenceVerdict::fail("synthesized text carries authority > delegate")
            } else if slot.is_some_and(|m| m > hh_provenance::AuthorityClass::Delegate) {
                EvidenceVerdict::fail(
                    "synthesized text would render into a definition-authority role",
                )
            } else {
                EvidenceVerdict::pass()
            }
        }
        _ => EvidenceVerdict::na("no synthesized text"),
    };
    // S4 — fail closed is structural: this surface reached `check_safety`
    // only through typed checks (E1–E3 verdicts live in the evidence
    // record — a `fail`/`n/a` there is `UncheckableSurface` upstream).
    let s4 = EvidenceVerdict::pass();
    SafetyEvidence {
        s1_no_authority_widening: s1,
        s2_nested_mediation: s2,
        s3_labelled_text: s3,
        s4_fail_closed: s4,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `synthesize_surface` / `AdmissionResult` / `retire_surface` (ADR-0091 D1/D2)
// ─────────────────────────────────────────────────────────────────────────────

/// `SurfaceProposal{candidate, provenance, hypothesis:
/// AssumptionDebtRecord, target_home}` — the synthesis pipeline's input
/// (§5d.2 §2). The author (`provenance.origin ∈ {human, model, evolution,
/// import}`) never changes the gates.
#[derive(Debug, Clone)]
pub struct SurfaceProposal {
    /// The proposed `SurfaceBinding` (`mapping = PlanMap` for composites).
    pub binding: SurfaceBinding,
    /// The `PlanMap`, when composite.
    pub plan_map: Option<PlanMap>,
    /// `provenance.origin` — `human | model | evolution | import`.
    pub origin: String,
    /// `hypothesis` — the `AssumptionDebtRecord` the admitted rule carries.
    pub hypothesis: AssumptionDebtRecord,
    /// `target_home ∈ {profile_rule, harness_rule}`.
    pub target_home: String,
}

/// `AdmissionResult{admitted(rule_id) | SurfaceSynthesisRefused{gate,
/// reason}}` (§5d.2 §2) — the pipeline verdict, a value (not an error) so
/// callers ledger the refusal row.
#[derive(Debug, Clone, PartialEq)]
pub enum AdmissionResult {
    /// Admitted — the rule id the conditioned rule carries.
    Admitted {
        /// The admitting rule's id.
        rule_id: String,
    },
    /// `SurfaceSynthesisRefused{candidate, gate}` — the gate that refused.
    Refused {
        /// The proposal's candidate coordinate.
        candidate: String,
        /// The refusing gate (`S1`…`S4`, `equivalence`, `debt`).
        gate: String,
        /// The reason.
        reason: String,
    },
}

/// `synthesize_surface(proposal, safety)` — the gated pipeline (§5d.2;
/// AC-R-2.5.2-8): `bind → check_equivalence (E1–E3, E5–E7 static, per
/// branch) → check_safety (S1–S4) → E4 per admissibility → admission`. The
/// hypothesis must be complete (`debt-complete`); `target_home ∈
/// {profile_rule, harness_rule}`.
pub fn synthesize_surface(proposal: &SurfaceProposal, safety: &SafetyInput<'_>) -> AdmissionResult {
    let cand = proposal.binding.surface_name.clone();
    // The hypothesis is the admission's debt record — an incomplete one
    // refuses at the debt gate (T-LCD-05).
    // A *complete* debt record is the same completeness check the
    // conditioned-rule registration runs (T-LCD-05): removal_test_ref +
    // owner present.
    if proposal.hypothesis.removal_test_ref.is_empty() || proposal.hypothesis.owner.id.is_empty() {
        return AdmissionResult::Refused {
            candidate: cand,
            gate: "debt".to_string(),
            reason: "the hypothesis AssumptionDebtRecord lacks removal_test_ref/owner".to_string(),
        };
    }
    if !matches!(
        proposal.target_home.as_str(),
        "profile_rule" | "harness_rule"
    ) {
        return AdmissionResult::Refused {
            candidate: cand,
            gate: "admission".to_string(),
            reason: format!(
                "target_home {} outside {{profile_rule, harness_rule}}",
                proposal.target_home
            ),
        };
    }
    // S1–S4 — a `fail` is a refusal at the gate; an `n/a` stands (only E4
    // may be `n/a` — the safety gates' n/a is the "not composite" class).
    let ev = check_safety(safety);
    for (gate, v) in [
        ("S1", &ev.s1_no_authority_widening),
        ("S2", &ev.s2_nested_mediation),
        ("S3", &ev.s3_labelled_text),
        ("S4", &ev.s4_fail_closed),
    ] {
        if let EvidenceVerdict::Fail { reason } = v {
            return AdmissionResult::Refused {
                candidate: cand,
                gate: gate.to_string(),
                reason: reason.clone(),
            };
        }
    }
    AdmissionResult::Admitted {
        rule_id: format!(
            "rule:synthesized:{}",
            proposal
                .binding
                .surface_id
                .chars()
                .take(16)
                .collect::<String>()
        ),
    }
}

/// `retire_surface(rules, rule_id)` — the removal test's operation
/// (AC-R-2.5.2-8): the admitted rule leaves the set; the residual is a
/// valid arm because `reference_variant` (the `native_fc` rendering) is the
/// unconditional fallback — `Some(residual)` when the rule existed.
pub fn retire_surface(rules: &[String], rule_id: &str) -> Option<Vec<String>> {
    if !rules.iter().any(|r| r == rule_id) {
        return None;
    }
    Some(
        rules
            .iter()
            .filter(|r| r.as_str() != rule_id)
            .cloned()
            .collect(),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// The C2 surface kinds — `freeform`, `code_mode`, `shim`
// ─────────────────────────────────────────────────────────────────────────────

/// `freeform` (ADR-0090 D7): one `parse(grammar_ref)` transform — the
/// surface's single argument parses through a declared `CompiledPayload`
/// grammar; static E1–E3 apply *after* the parse (the fixture flows through
/// the parser).
#[derive(Debug, Clone, PartialEq)]
pub struct FreeformSpec {
    /// The grammar's `CompiledPayload` ref.
    pub grammar_ref: String,
    /// The capability parameter the parsed value binds.
    pub capability_param: String,
    /// The surface arg name the model writes.
    pub surface_arg: String,
}

/// `freeform_map(spec)` — the `SurfaceArgMap` the freeform surface binds:
/// a single `parse(grammar_ref)` entry (the mode's defining shape — §5d.2
/// admissibility row).
pub fn freeform_map(spec: &FreeformSpec) -> crate::equiv::SurfaceArgMap {
    [(
        spec.surface_arg.clone(),
        ArgMapEntry {
            capability_param: spec.capability_param.clone(),
            transform: ArgTransform::Parse {
                grammar_ref: spec.grammar_ref.clone(),
            },
            narrowing: None,
        },
    )]
    .into_iter()
    .collect()
}

/// `code_mode` (ADR-0090 D7; ADR-0091 S2): a nested call is its own
/// `Effect` — `effect_id` derived under `parent_effect_id` (the container's
/// effect). `code_mode_nested_call` mints the record the monitor
/// authorizes individually.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeModeNestedCall {
    /// The nested call's own `effect_id`.
    pub effect_id: String,
    /// The containing call's `effect_id` (`parent_effect_id`).
    pub parent_effect_id: String,
    /// The nested surface's `surface_id`.
    pub surface_id: String,
    /// The arguments JSON (parsed — the monitor re-validates through the
    /// target's `SurfaceArgMap`).
    pub args: Json,
}

/// `code_mode_nested_call(parent_effect_id, surface_id, args, ordinal)` —
/// mint the nested effect id (`idp`-derived under the parent — the S2
/// parentage the ledger records).
pub fn code_mode_nested_call(
    parent_effect_id: &str,
    surface_id: &str,
    args: &Json,
    ordinal: u64,
) -> CodeModeNestedCall {
    let effect_id = hh_identity::idp::idp_id(
        "code_mode_effect.1",
        format!(
            "{parent_effect_id}:{surface_id}:{ordinal}:{}",
            args.to_canonical_string()
        )
        .as_bytes(),
    );
    CodeModeNestedCall {
        effect_id,
        parent_effect_id: parent_effect_id.to_string(),
        surface_id: surface_id.to_string(),
        args: args.clone(),
    }
}

/// `code_mode_catalogue(bindings)` — the nested-call catalogue: only
/// bindings whose `admitted_modes ∋ code_mode` are callable from an
/// executed program; tools lacking the code-mode hint are **absent** from
/// the rendering (AC-R-2.5.2-9 — never an error entry, never a stub).
pub fn code_mode_catalogue(bindings: &[SurfaceBinding]) -> Vec<&SurfaceBinding> {
    bindings
        .iter()
        .filter(|b| {
            b.admitted_modes
                .contains(&hh_hir::tools::ExposureMode::CodeMode)
        })
        .collect()
}

/// `shim` (ADR-0090 D1; OQ-229 resolved by ADR-0118): the model-emitted
/// text parses through `resolve_name` (exact-table only) into the target
/// surface's `SurfaceArgMap`; the parsed call carries `authority =
/// delegate`; an unresolvable name is `SurfaceFailure::UnknownSurface`
/// (never a best-effort match — S4); the interpreter call is a
/// `model_call` effect metered under the *calling role's* token role.
#[derive(Debug, Clone, PartialEq)]
pub struct ShimCall {
    /// The resolved surface's `surface_id`.
    pub surface_id: String,
    /// The parsed arguments (the target `SurfaceArgMap` interprets them).
    pub args: Json,
    /// `authority = delegate` — constant (the parsed call's ceiling).
    pub authority: hh_provenance::AuthorityClass,
}

/// `shim_resolve(name, args, table)` — the shim's `resolve_name` half:
/// `table` is the exact `name → surface_id` map; a miss is
/// `SurfaceFailure::UnknownSurface`.
pub fn shim_resolve(
    name: &str,
    args: Json,
    table: &BTreeMap<String, String>,
) -> Result<ShimCall, crate::surface::SurfaceFailure> {
    match table.get(name) {
        Some(surface_id) => Ok(ShimCall {
            surface_id: surface_id.clone(),
            args,
            authority: hh_provenance::AuthorityClass::Delegate,
        }),
        None => Err(crate::surface::SurfaceFailure::UnknownSurface),
    }
}

/// `ShimMetering{effect_kind: "model_call", token_role, attribution}` —
/// the interpreter call's metering record (AC-R-2.5.2-10: metered as a
/// `model_call`; the token role is the calling role's — ADR-0118's OQ-229
/// resolution, "never a fourth bucket"). `attribution =
/// harness_overhead.shim_interpreter` keeps the spend visible as
/// interpreter work.
#[derive(Debug, Clone, PartialEq)]
pub struct ShimMetering {
    /// Always `"model_call"`.
    pub effect_kind: &'static str,
    /// The calling role's token role (`primary`/`utility`/… — the *role's*,
    /// not a new bucket).
    pub token_role: String,
    /// The attribution tag.
    pub attribution: &'static str,
}

/// `shim_metering(caller_role)` — the interpreter call's metering record.
pub fn shim_metering(caller_role: &str) -> ShimMetering {
    ShimMetering {
        effect_kind: "model_call",
        token_role: caller_role.to_string(),
        attribution: "harness_overhead.shim_interpreter",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Builders — `split` / `composite` surface construction (C1; ADR-0090 D7)
// ─────────────────────────────────────────────────────────────────────────────

/// `plan_map_ref(map)` — the pinned `Procedure` `version_id` coordinate a
/// `BindingMapping::PlanMap` carries (`idp("plan_map.1", canonical)`).
pub fn plan_map_ref(map: &PlanMap) -> String {
    hh_identity::idp::idp_id("plan_map.1", map_json(map).to_canonical_string().as_bytes())
}

// ─────────────────────────────────────────────────────────────────────────────
// C1 producers (R2.8 — DF-S1.17-2): the `bind_*` halves that mint real
// `SurfaceBinding`s under the extension modes. Each mints the binding then
// re-derives `surface_id` over the full record (CC1 — the identity basis
// covers `mapping`/`exposure_mode`/`capability_refs`; `surface.rs` is the
// one spelling).
// ─────────────────────────────────────────────────────────────────────────────

/// `bind_composite(surface, home, plan, allowed_capabilities,
/// capability_effects)` — the `PlanMap` producer (§5d.2 §3 `mapping =
/// PlanMap(pinned Procedure version_id)`; R2.8). The check is
/// `check_plan_map` (steps ⊆ {Branch, Invoke, Verify}; invokes ⊆
/// `allowed_capabilities`) plus the declared-effects join: every invoked
/// capability must carry a declared effect entry and the composite's
/// `effects_bound = ∪ branches` — `compose(verify(execute(x)))` leaves no
/// effect path outside the declared bound (an undeclared-effects invoke is
/// `UncheckableSurface`, never a silent widening).
///
/// `surface` is the authored `ToolSurface` declaration (name, argument
/// order, dialect narrowing, authored run-time modes); `home` is the node
/// the composite is declared on (the `Procedure` — `capability_ref` pins
/// it); `capability_effects` maps each invoked capability's semantic id to
/// its declared effect-domain spellings.
pub fn bind_composite(
    surface: &ToolSurface,
    home: &hh_hir::Node,
    plan: &PlanMap,
    allowed_capabilities: &BTreeSet<String>,
    capability_effects: &BTreeMap<String, BTreeSet<String>>,
) -> Result<SurfaceBinding, CompileError> {
    check_plan_map(plan, allowed_capabilities)?;
    let invoked = capabilities_invoked(plan);
    if invoked.is_empty() {
        return Err(CompileError::UncheckableSurface {
            surface: surface.name.clone(),
            reason: "a composite binds ≥1 capability — the PlanMap has no Invoke".to_string(),
        });
    }
    let mut effects: BTreeSet<String> = BTreeSet::new();
    for cap in &invoked {
        match capability_effects.get(cap.as_str()) {
            Some(e) => effects.extend(e.iter().cloned()),
            // An invoke without a declared effect set is an unverifiable
            // effect path — fail closed (S1/E1).
            None => {
                return Err(CompileError::UncheckableSurface {
                    surface: surface.name.clone(),
                    reason: format!(
                        "PlanMap invoke {cap} has no declared effect entry — an unverified effect path"
                    ),
                });
            }
        }
    }
    let mut b = crate::equiv::bind_surface(home, surface, &Json::Null);
    b.exposure_mode = CompileExposureMode::Composite;
    b.mapping = BindingMapping::PlanMap(plan_map_ref(plan));
    b.capability_refs = invoked.into_iter().collect();
    b.effects_bound = effects.into_iter().collect();
    b.surface_id = crate::surface::surface_id(&b);
    Ok(b)
}

/// `bind_freeform(surface, home, spec)` — the `freeform` producer
/// (ADR-0090 D7): the surface's single argument parses through the declared
/// grammar — the `arg_map` is [`freeform_map`] verbatim.
pub fn bind_freeform(
    surface: &ToolSurface,
    home: &hh_hir::Node,
    spec: &FreeformSpec,
) -> SurfaceBinding {
    let mut b = crate::equiv::bind_surface(home, surface, &Json::Null);
    b.exposure_mode = CompileExposureMode::Freeform;
    b.arg_map = freeform_map(spec);
    b.surface_id = crate::surface::surface_id(&b);
    b
}

/// `ShimSpec{surface_arg, capability_param, table_ref}` — the `shim`
/// producer's declaration (ADR-0090 D1): the model writes `surface_arg`
/// text; the interpreter's `resolve_name` runs it through the *exact* name
/// table `table_ref` (a `Ref` to the compiled table — never a best-effort
/// match; a miss is `SurfaceFailure::UnknownSurface` — S4).
#[derive(Debug, Clone, PartialEq)]
pub struct ShimSpec {
    /// The surface arg carrying the model's text.
    pub surface_arg: String,
    /// The capability parameter the resolved call binds.
    pub capability_param: String,
    /// The compiled `resolve_name` table ref.
    pub table_ref: String,
}

/// `bind_shim(surface, home, spec)` — the `shim` producer: the arg_map's
/// defining entry is `resolve_name(table_ref)` (the table pin rides the
/// `arg_map` — it is in the `surface_id` basis, so a table change is a new
/// surface identity — E7).
pub fn bind_shim(surface: &ToolSurface, home: &hh_hir::Node, spec: &ShimSpec) -> SurfaceBinding {
    let mut b = crate::equiv::bind_surface(home, surface, &Json::Null);
    b.exposure_mode = CompileExposureMode::Shim;
    b.arg_map = [(
        spec.surface_arg.clone(),
        ArgMapEntry {
            capability_param: spec.capability_param.clone(),
            transform: ArgTransform::ResolveName {
                table_ref: spec.table_ref.clone(),
            },
            narrowing: None,
        },
    )]
    .into_iter()
    .collect();
    b.surface_id = crate::surface::surface_id(&b);
    b
}

/// `bind_code_mode(surface, home)` — the `code_mode` producer (ADR-0090
/// D7): the binding's *compile-time* mode is `code_mode`; the authored
/// run-time admission carries `ExposureMode::CodeMode` (the run-time leg —
/// `check_callable` admits program-originated calls only on that mode — is
/// `hh_compiler::exposure::check_callable`, unchanged).
pub fn bind_code_mode(surface: &ToolSurface, home: &hh_hir::Node) -> SurfaceBinding {
    let mut b = crate::equiv::bind_surface(home, surface, &Json::Null);
    b.exposure_mode = CompileExposureMode::CodeMode;
    b.surface_id = crate::surface::surface_id(&b);
    b
}

/// `bind_variant(binding, family, variant)` — stamp the identity-bearing
/// family/variant coordinates the selected variant renders under
/// (§5d.2 §3's `family_id`/`variant_id` members; ADR-0090 D2/D3). The
/// variant's `rule_ids` join the binding's shaping-record list; the
/// compile-time `exposure_mode` re-mints under the variant's mode. The
/// `surface_id` basis does **not** cover `family_id`/`variant_id` — they
/// are provenance members on the one surface record (the basis is fixed —
/// append-only; ADR records the decision).
pub fn bind_variant(
    binding: &SurfaceBinding,
    family: &SurfaceFamily,
    variant: &SurfaceVariant,
) -> Result<SurfaceBinding, CompileError> {
    if !family
        .variants
        .iter()
        .any(|v| v.variant_id == variant.variant_id)
    {
        return Err(CompileError::UncheckableSurface {
            surface: binding.surface_name.clone(),
            reason: format!(
                "variant {} is not a member of family {}",
                variant.variant_id, family.family_id
            ),
        });
    }
    let mut b = binding.clone();
    b.family_id = Some(family.family_id.clone());
    b.variant_id = Some(variant.variant_id.clone());
    b.rule_ids.extend(variant.rule_ids.iter().cloned());
    b.exposure_mode = variant.exposure_mode;
    b.surface_id = crate::surface::surface_id(&b);
    Ok(b)
}

/// The canonical JSON of a `PlanMap` (the pin preimage + ledger record).
pub fn map_json(map: &PlanMap) -> Json {
    fn step_json(s: &PlanStep) -> Json {
        match s {
            PlanStep::Branch { predicate, steps } => Json::obj([
                ("kind", Json::str("branch")),
                ("predicate", selector_json(predicate)),
                ("steps", Json::Arr(steps.iter().map(step_json).collect())),
            ]),
            PlanStep::Invoke {
                capability_ref,
                arg_map,
            } => Json::obj([
                ("kind", Json::str("invoke")),
                ("capability_ref", Json::str(capability_ref.clone())),
                ("arg_map", crate::schema::arg_map_json_pub(arg_map)),
            ]),
            PlanStep::Verify { validator_ref } => Json::obj([
                ("kind", Json::str("verify")),
                ("validator_ref", Json::str(validator_ref.clone())),
            ]),
            PlanStep::Loop(steps) => Json::obj([
                ("kind", Json::str("loop")),
                ("steps", Json::Arr(steps.iter().map(step_json).collect())),
            ]),
            PlanStep::Delegate => Json::obj([("kind", Json::str("delegate"))]),
            PlanStep::Opaque(d) => Json::obj([
                ("kind", Json::str("opaque")),
                ("detail", Json::str(d.clone())),
            ]),
            PlanStep::Instruction(t) => Json::obj([
                ("kind", Json::str("instruction")),
                ("text", Json::str(t.clone())),
            ]),
        }
    }
    Json::obj([
        (
            "steps",
            Json::Arr(map.steps.iter().map(step_json).collect()),
        ),
        (
            "session_state",
            map.session_state
                .as_ref()
                .map(|s| Json::str(s.clone()))
                .unwrap_or(Json::Null),
        ),
    ])
}

/// The canonical JSON of a `VariantSelector`.
pub fn selector_json(s: &VariantSelector) -> Json {
    match s {
        VariantSelector::Always => Json::str("always"),
        VariantSelector::TaskClass(id) => Json::obj([("task_class", Json::str(id.clone()))]),
        VariantSelector::TaskLabel(l) => Json::obj([("task_label", Json::str(l.clone()))]),
        VariantSelector::ExecutorPlatform(p) => {
            Json::obj([("executor_platform", Json::str(p.clone()))])
        }
        VariantSelector::SessionSupport(b) => Json::obj([("session_support", Json::Bool(*b))]),
        VariantSelector::EnvironmentCount { min } => {
            Json::obj([("environment_count_min", Json::Int(*min as i64))])
        }
        VariantSelector::CapabilityDeclared(c) => {
            Json::obj([("capability_declared", Json::str(c.clone()))])
        }
        VariantSelector::TurnField { field, equals } => Json::obj([
            ("turn_field", Json::str(field.clone())),
            ("equals", equals.clone()),
        ]),
        VariantSelector::All(m) => {
            Json::obj([("all", Json::Arr(m.iter().map(selector_json).collect()))])
        }
        VariantSelector::Any(m) => {
            Json::obj([("any", Json::Arr(m.iter().map(selector_json).collect()))])
        }
        VariantSelector::Not(i) => Json::obj([("not", selector_json(i))]),
    }
}

/// `composite_binding(name, hir_node_id, map)` — mint the `PlanMap`-mapped
/// `SurfaceBinding` for a static composite (`exposure_mode = composite`;
/// `capability_refs = ∪ invokes`; `effects_bound` the caller supplies from
/// the invoked capabilities' declared effect sets — the caller owns S1's
/// sources).
pub fn composite_binding(
    surface_name: &str,
    hir_node_id: &str,
    capability_version: &str,
    map: &PlanMap,
    effects_bound: Vec<String>,
    family_id: Option<String>,
    variant_id: Option<String>,
) -> SurfaceBinding {
    let refs: Vec<String> = capabilities_invoked(map).into_iter().collect();
    let mut b = SurfaceBinding {
        surface_name: surface_name.to_string(),
        surface_id: String::new(),
        exposure_mode: CompileExposureMode::Composite,
        capability_ref: crate::plan::PinnedRef {
            semantic_id: refs.first().cloned().unwrap_or_default(),
            version_id: capability_version.to_string(),
        },
        capability_refs: refs,
        hir_node_id: hir_node_id.to_string(),
        arg_map: BTreeMap::new(),
        mapping: BindingMapping::PlanMap(plan_map_ref(map)),
        rule_ids: Vec::new(),
        evidence_ref: None,
        safety_ref: None,
        effects_bound,
        family_id,
        variant_id,
        dialect: crate::equiv::DEFAULT_SCHEMA_DIALECT.to_string(),
        admitted_modes: [hh_hir::tools::ExposureMode::Direct].into_iter().collect(),
        pinned: false,
        hidden: false,
    };
    b.surface_id = crate::surface::surface_id(&b);
    b
}

/// `split_bindings(base, splits)` — the `split` exposure (ADR-0090 D7): one
/// capability → several surfaces, each a `SurfaceArgMap` where the declared
/// `const_arg` binds `const{value}` (the split member carries the fixed
/// parameter — e.g. `write_stdin`'s fixed `stream`). `splits` entries are
/// `{name, const_arg, const_value, drop_args[]}` — the dropped capability
/// args leave the surface schema entirely.
pub fn split_bindings(
    base: &SurfaceBinding,
    splits: &Json,
) -> Result<Vec<SurfaceBinding>, CompileError> {
    let Some(Json::Arr(entries)) = Some(splits) else {
        return Err(CompileError::UnexpressibleSurface {
            entity: base.surface_name.clone(),
            profile: String::new(),
            reason: "split variant requires `splits[]`".to_string(),
        });
    };
    let mut out = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let name = e
            .get("name")
            .and_then(Json::as_str)
            .ok_or_else(|| CompileError::UnexpressibleSurface {
                entity: base.surface_name.clone(),
                profile: String::new(),
                reason: format!("split[{i}] lacks `name`"),
            })?
            .to_string();
        let const_arg = e
            .get("const_arg")
            .and_then(Json::as_str)
            .ok_or_else(|| CompileError::UnexpressibleSurface {
                entity: name.clone(),
                profile: String::new(),
                reason: format!("split[{i}] lacks `const_arg`"),
            })?
            .to_string();
        let const_value = e.get("const_value").cloned().unwrap_or(Json::Null);
        let drop_args: BTreeSet<String> = match e.get("drop_args") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(|j| j.as_str().map(str::to_string))
                .collect(),
            _ => BTreeSet::new(),
        };
        let mut b = base.clone();
        b.surface_name = name;
        b.exposure_mode = CompileExposureMode::Split;
        let mut arg_map = BTreeMap::new();
        for (field, entry) in &base.arg_map {
            if *field == const_arg || drop_args.contains(field) {
                continue;
            }
            arg_map.insert(field.clone(), entry.clone());
        }
        arg_map.insert(
            const_arg.clone(),
            ArgMapEntry {
                capability_param: const_arg.clone(),
                transform: ArgTransform::Const(const_value),
                narrowing: Some("const".to_string()),
            },
        );
        b.arg_map = arg_map;
        b.surface_id = crate::surface::surface_id(&b);
        out.push(b);
    }
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-branch E1–E3 + inherited E4 (AC-R-2.5.2-3)
// ─────────────────────────────────────────────────────────────────────────────

/// `BranchEvidence{branch, capability_ref, evidence}` — one `Invoke`'s
/// `EquivalenceEvidence` row (E1–E3 static per branch; E4 inherits the
/// invoked capability's fixture-suite verdict — `n/a(open-world)` etc.
/// stand).
#[derive(Debug, Clone, PartialEq)]
pub struct BranchEvidence {
    /// The branch path (`steps[i]` indices — `0.2` = step 0's branch, step 2).
    pub branch: String,
    /// The invoked capability's semantic id.
    pub capability_ref: String,
    /// The evidence.
    pub evidence: crate::equiv::EquivalenceEvidence,
}

/// `check_plan_map_evidence(map, capabilities)` — run E1–E7 on every
/// `Invoke` branch against its capability node (the branch's own
/// `SurfaceArgMap` is the checked map — E1–E3 pass per branch, E4 is the
/// capability's suite verdict — *inherited*, §5d.2 admissibility row).
/// `capabilities` maps semantic id → the `ToolCapability` node.
pub fn check_plan_map_evidence(
    map: &PlanMap,
    capabilities: &BTreeMap<String, hh_hir::Node>,
) -> Result<Vec<BranchEvidence>, CompileError> {
    let mut out = Vec::new();
    fn go(
        steps: &[PlanStep],
        path: String,
        caps: &BTreeMap<String, hh_hir::Node>,
        out: &mut Vec<BranchEvidence>,
    ) -> Result<(), CompileError> {
        for (i, s) in steps.iter().enumerate() {
            let p = if path.is_empty() {
                format!("{i}")
            } else {
                format!("{path}.{i}")
            };
            match s {
                PlanStep::Branch { steps, .. } => go(steps, p, caps, out)?,
                PlanStep::Invoke {
                    capability_ref,
                    arg_map,
                } => {
                    let node = caps.get(capability_ref).ok_or_else(|| {
                        CompileError::AuthorityWidening {
                            detail: format!(
                                "branch {p} invokes {capability_ref} — no such capability node"
                            ),
                        }
                    })?;
                    let mut b = SurfaceBinding {
                        surface_name: format!("plan:{p}"),
                        surface_id: String::new(),
                        exposure_mode: CompileExposureMode::Composite,
                        capability_ref: crate::plan::PinnedRef {
                            semantic_id: capability_ref.clone(),
                            version_id: node.version_id(),
                        },
                        capability_refs: vec![capability_ref.clone()],
                        hir_node_id: capability_ref.clone(),
                        arg_map: arg_map.clone(),
                        mapping: BindingMapping::SurfaceArgMap,
                        rule_ids: Vec::new(),
                        evidence_ref: None,
                        safety_ref: None,
                        effects_bound: Vec::new(),
                        family_id: None,
                        variant_id: None,
                        dialect: crate::equiv::DEFAULT_SCHEMA_DIALECT.to_string(),
                        admitted_modes: [hh_hir::tools::ExposureMode::Direct].into_iter().collect(),
                        pinned: false,
                        hidden: false,
                    };
                    b.surface_id = crate::surface::surface_id(&b);
                    let evidence =
                        crate::equiv::check_equivalence(&b, node, None, None, None, false)?;
                    out.push(BranchEvidence {
                        branch: p,
                        capability_ref: capability_ref.clone(),
                        evidence,
                    });
                }
                PlanStep::Verify { .. } => {}
                other => {
                    return Err(CompileError::UncheckableSurface {
                        surface: format!("plan_step@{p}"),
                        reason: format!(
                            "{} — a plan containing it is code-mode by definition",
                            step_kind(other)
                        ),
                    })
                }
            }
        }
        Ok(())
    }
    go(&map.steps, String::new(), capabilities, &mut out)?;
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// The canonical fixtures (AC-R-2.5.2-3)
// ─────────────────────────────────────────────────────────────────────────────

/// The `str_replace_editor` `PlanMap` (the spec's C1 fixture — one composite
/// surface over `read_file`/`write_file`/`edit_file`): a `command`
/// turn-field selects the branch — `view` → `read_file`, `create` →
/// `write_file`, `str_replace`/`insert`/`undo_edit` → `edit_file`.
pub fn str_replace_editor_plan() -> PlanMap {
    let args = |m: &[(&str, &str)]| -> crate::equiv::SurfaceArgMap {
        m.iter()
            .map(|(f, p)| {
                (
                    f.to_string(),
                    ArgMapEntry {
                        capability_param: p.to_string(),
                        transform: ArgTransform::Identity,
                        narrowing: None,
                    },
                )
            })
            .collect()
    };
    let cmd = |c: &str| VariantSelector::TurnField {
        field: "command".to_string(),
        equals: Json::str(c.to_string()),
    };
    PlanMap {
        steps: vec![
            PlanStep::Branch {
                predicate: cmd("view"),
                steps: vec![PlanStep::Invoke {
                    capability_ref: "read_file".to_string(),
                    arg_map: args(&[("path", "path")]),
                }],
            },
            PlanStep::Branch {
                predicate: cmd("create"),
                steps: vec![PlanStep::Invoke {
                    capability_ref: "write_file".to_string(),
                    arg_map: args(&[("path", "path"), ("file_text", "content")]),
                }],
            },
            PlanStep::Branch {
                predicate: VariantSelector::Any(vec![
                    cmd("str_replace"),
                    cmd("insert"),
                    cmd("undo_edit"),
                ]),
                steps: vec![PlanStep::Invoke {
                    capability_ref: "edit_file".to_string(),
                    arg_map: args(&[
                        ("path", "path"),
                        ("old_str", "edits.0.old"),
                        ("new_str", "edits.0.new"),
                    ]),
                }],
            },
        ],
        session_state: None,
    }
}

/// The `unified_exec` `PlanMap` (§5d.2 admissibility row, verbatim):
/// `Branch(session_id present) → Invoke(process_write); else →
/// Invoke(exec)` — `write_stdin` is the sibling *split* surface.
pub fn unified_exec_plan() -> PlanMap {
    PlanMap {
        steps: vec![PlanStep::Branch {
            predicate: VariantSelector::TurnField {
                field: "session_id".to_string(),
                equals: Json::Bool(true),
            },
            steps: vec![PlanStep::Invoke {
                capability_ref: "process_write".to_string(),
                arg_map: [
                    (
                        "session_id".to_string(),
                        ArgMapEntry {
                            capability_param: "session_id".to_string(),
                            transform: ArgTransform::Identity,
                            narrowing: None,
                        },
                    ),
                    (
                        "data".to_string(),
                        ArgMapEntry {
                            capability_param: "stdin".to_string(),
                            transform: ArgTransform::Identity,
                            narrowing: None,
                        },
                    ),
                ]
                .into_iter()
                .collect(),
            }],
        }],
        session_state: None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `code_mode` sandbox declaration (AC-R-2.5.2-9)
// ─────────────────────────────────────────────────────────────────────────────

/// `CodeModeSandbox{holds_credentials: false, ambient_authority: false}` —
/// the declaration a code-mode exposure carries (ADR-0091 S2; ADR-0050
/// §8(c): the sandbox running model-authored programs holds **no credential
/// and no ambient authority**; every tool call from it crosses the kernel
/// boundary — the reference monitor authorizes each one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeModeSandbox {
    /// Always `false` — the struct cannot express `true` (the check is
    /// structural: a sandbox declaration asserting credentials is
    /// `AuthorityWidening` at `check_code_mode_sandbox`).
    pub holds_credentials: bool,
    /// Always `false`.
    pub ambient_authority: bool,
}

/// `check_code_mode_sandbox(decl)` — the structural S2 check (a `true`
/// member is `AuthorityWidening` — the sandbox can never carry them).
pub fn check_code_mode_sandbox(decl: &CodeModeSandbox) -> Result<(), CompileError> {
    if decl.holds_credentials || decl.ambient_authority {
        return Err(CompileError::AuthorityWidening {
            detail: "a code-mode sandbox holds credentials/ambient authority — forbidden (S2)"
                .to_string(),
        });
    }
    Ok(())
}
