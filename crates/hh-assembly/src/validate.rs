//! `validate_assembly` (§3.3.4/§3.3.8): the seven-stage validation that **runs and
//! reports every stage** — never fail-fast. Stages 1–5 and 7 are this ticket's scope
//! (§3.3.13); 6a/6b are `n/a{not_run}` (C1/Stage 3/5). A hosted root flips stages 2, 6
//! and the 7-opacity sub-check to `n/a{class}` (AC-CC-12; T-LCD-15).
//!
//! Stages: (1) schema — the member-wise grammar decode + `layers`/`source` consistency;
//! (2) class conformance — catalog coverage, cardinality, class match, `required_inputs`,
//! contract shape; (3) parameter space — declared/used in both directions, sweepable
//! domains, `budget_relevant` defaults; (4) constraints; (5) the HIR kernel's `validate`
//! over the materialised document (`assembly.slots` merged onto the root `native`
//! process); (7) the LCD static battery — opacity (7-O), `IdentityIncludesSurface`
//! (7-T10), `hosting_edges` (7-L4), inheritance contracts (7-T12), and the
//! `BenchmarkConditionedRule` scan (7-L2 — error, never warning; ADR-0143 D2).

use std::collections::{BTreeMap, BTreeSet};

use hh_hir::document::{HirDocument, SealedDefinition};
use hh_hir::records::{AgentProcessBody, KindRecord, SlotBinding, SlotBindings};
use hh_hir::refs::RefVersion;
use hh_provenance::{AuthorityClass, OpacityReport, ProvenanceRecord};
use hh_registry::kinds::Cardinality;
use hh_wire::json::Json;

use crate::catalog::ClassCatalog;
use crate::diagnostics::{
    detail_text, kern_code, AssemblyDiagnostic, Code, DerivedResults, NaReason, OpacitySummary,
    ReportStatus, Severity, Stage, StageOutcome, ValidationReport,
};
use crate::grammar::{
    markers_in_json, Assembly, ConstraintKind, EntityBinding, ParamType, ENTITY_MARKER,
    PARAM_MARKER, SECRET_MARKER,
};

/// The subject `validate_assembly` runs on (§3.3.4: `assembly | sealed`).
pub enum Subject<'a> {
    /// An authored (possibly unresolved) `hir/1` document.
    Authored(&'a HirDocument),
    /// A sealed definition.
    Sealed(&'a SealedDefinition),
}

impl Subject<'_> {
    fn document(&self) -> &HirDocument {
        match self {
            Subject::Authored(d) => d,
            Subject::Sealed(s) => &s.document,
        }
    }
}

/// The profile view stage 6a/6b consult (C1 — the signature is fixed now so the stage
/// lands without an API break; unused at S1, where stage 6 is `n/a`).
pub trait ProfileView {
    /// The profile record at an identity coordinate.
    fn profile(&self, coordinate: &str) -> Option<Json>;
}

/// The benchmark-coordinate tokens 7-L2 hunts (ADR-0143 D2 — `suite_id`/`task_id`/
/// `foreign_id`/`split`/`family` predicate references are `BenchmarkConditionedRule`,
/// an **error**, never a warning).
pub const BENCH_TOKENS: &[&str] = &["suite_id", "task_id", "foreign_id", "split", "family"];

/// `validate_assembly(assembly | sealed, class_catalog, profiles?) → ValidationReport`
/// (§3.3.4). `kernel` mints the `detail: Text{owner = kernel}` leaves on diagnostics.
pub fn validate_assembly(
    subject: Subject<'_>,
    catalog: &dyn ClassCatalog,
    _profiles: Option<&dyn ProfileView>,
    kernel: &ProvenanceRecord,
) -> ValidationReport {
    let doc = subject.document();
    let mut diags: Vec<AssemblyDiagnostic> = Vec::new();
    let mut report = ValidationReport::default();
    let hosted = root_is_hosted(doc);

    // ── Stage 1 — schema ────────────────────────────────────────────────────────
    let stage1 = StageOutcome::ran(1);
    let assembly = match &doc.assembly {
        Some(j) => Assembly::from_json(j, "/assembly", kernel, &mut diags),
        None => {
            diags.push(diag(
                Code::LoadParse,
                "/assembly",
                "assembly",
                "the document carries no `assembly` section",
                "author the `assembly` member (a Harness Definition is a hir/1 document with an assembly section)",
                kernel,
                Stage::Validate(1),
            ));
            None
        }
    };
    let mut layer_ids: BTreeSet<String> = BTreeSet::new();
    if let Some(a) = &assembly {
        if a.dialect != crate::grammar::ASSEMBLY_DIALECT {
            diags.push(diag(
                Code::LoadDialect,
                "/assembly/dialect",
                &a.dialect,
                &format!(
                    "assembly dialect `{}` is not `{}`",
                    a.dialect,
                    crate::grammar::ASSEMBLY_DIALECT
                ),
                "author `dialect` as the grammar dialect",
                kernel,
                Stage::Validate(1),
            ));
        }
        if let Some(layers) = &a.layers {
            for l in layers {
                layer_ids.insert(l.id.clone());
            }
        }
        // C-COMP-3 — a layered document whose constraints lack/name-wrong `source`.
        if a.layers.is_some() {
            for (i, c) in a.constraints.iter().enumerate() {
                match &c.source {
                    None => diags.push(diag(
                        Code::CompLayerProvenanceMissing,
                        &format!("/assembly/constraints/{i}"),
                        &format!("constraints[{i}]"),
                        "a layered document's constraint carries no `source` LayerProvenance",
                        "record the authoring layer on the constraint",
                        kernel,
                        Stage::Validate(1),
                    )),
                    Some(s) if !layer_ids.contains(&s.id) => diags.push(diag(
                        Code::CompLayerProvenanceMissing,
                        &format!("/assembly/constraints/{i}/source"),
                        &s.id,
                        "constraint `source` names a layer id not declared in `layers[]`",
                        "declare the layer in `layers[]` or fix the constraint's `source.id`",
                        kernel,
                        Stage::Validate(1),
                    )),
                    _ => {}
                }
            }
        }
        // C-CLASS-6 — slots on a hosted process (T-LCD-15): the grammar-level check
        // runs at stage 1 (stage 2 is `n/a{class}` for hosted roots).
        if hosted && !a.slots.is_empty() {
            for slot in a.slots.keys() {
                diags.push(diag(
                    Code::ClassSlotsOnHosted,
                    &format!("/assembly/slots/{slot}"),
                    slot,
                    "a hosted AgentProcess carries a slot binding — hosted participants have no component classes",
                    "remove the slot binding (hosted participants declare params, never slots)",
                    kernel,
                    Stage::Validate(1),
                ));
            }
        }
        // A `resolved` member alongside live selectors is an inconsistent section.
        if a.resolved.is_some() && !a.is_resolved() {
            diags.push(diag(
                Code::LoadParse,
                "/assembly/resolved",
                "resolved",
                "the `resolved` member is present but the section still carries selectors/binding forms",
                "`resolved` is written by `resolve` after pinning — never authored",
                kernel,
                Stage::Validate(1),
            ));
        }
    }
    report.stages.push(stage1);

    // The effective slot view: `assembly.slots` wins over an equal-keyed `native.slots`
    // entry (§3.3.2 — `slots` is the desired-state record `resolve` materialises); a
    // *disagreeing* double binding is a `C-COMP-2` conflict, never silent last-wins.
    let effective = effective_slots(doc, assembly.as_ref(), &mut diags, kernel);

    // ── Stage 2 — class conformance ──────────────────────────────────────────────
    if hosted {
        report
            .stages
            .push(StageOutcome::not_applicable(2, NaReason::Class));
    } else {
        report.stages.push(StageOutcome::ran(2));
        if let Some(a) = &assembly {
            stage2_class_conformance(a, &effective, catalog, doc, &mut diags, kernel);
        }
    }

    // ── Stage 3 — parameter space ────────────────────────────────────────────────
    report.stages.push(StageOutcome::ran(3));
    if let Some(a) = &assembly {
        stage3_parameters(a, &mut diags, kernel);
    }

    // ── Stage 4 — constraints ────────────────────────────────────────────────────
    report.stages.push(StageOutcome::ran(4));
    if let Some(a) = &assembly {
        stage4_constraints(a, &effective, &mut diags, kernel);
    }

    // ── Stage 5 — the HIR kernel's validate over the materialised document ────────
    report.stages.push(StageOutcome::ran(5));
    let materialised = materialise(doc, assembly.as_ref());
    if let Err(errs) = hh_hir::validate(&materialised).map(|_| ()) {
        for e in errs {
            diags.push(AssemblyDiagnostic {
                code: kern_code(&e),
                class: None,
                severity: Severity::Error,
                path: "/".into(),
                source_layer: None,
                subject: format!("{e:?}"),
                stage: Stage::Validate(5),
                detail: detail_text(format!("HIR kernel validation failed: {e:?}"), kernel),
                remedy: "fix the underlying HIR document member".into(),
                owner_adr: "ADR-0148".into(),
            });
        }
    }

    // ── Stage 6 — profile compatibility (6a C1/Stage 3; 6b C1/Stage 5) ───────────
    report.stages.push(StageOutcome::not_applicable(
        6,
        if hosted {
            NaReason::Class
        } else {
            NaReason::NotRun
        },
    ));

    // ── Stage 7 — the LCD static battery ─────────────────────────────────────────
    report.stages.push(StageOutcome::ran(7));
    stage7_lcd(
        doc,
        assembly.as_ref(),
        hosted,
        &mut report.derived,
        &mut diags,
        kernel,
    );

    report.diagnostics = diags;
    report.finalize();
    report
}

/// The effective slot map — `assembly.slots` merged over `native.slots` on the root
/// process (a disagreeing double binding is `C-COMP-2`, never silent last-wins).
fn effective_slots(
    doc: &HirDocument,
    assembly: Option<&Assembly>,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
) -> BTreeMap<String, SlotBindings> {
    let mut slots = root_native_slots(doc).cloned().unwrap_or_default();
    if let Some(a) = &assembly {
        for (name, b) in &a.slots {
            if let Some(existing) = slots.get(name) {
                if existing != b {
                    diags.push(diag(
                        Code::CompLayerConflict,
                        &format!("/assembly/slots/{name}"),
                        name,
                        "the slot is bound in both `native.slots` and `assembly.slots` with different bindings",
                        "author the binding once — `assembly.slots` is the desired-state record",
                        kernel,
                        Stage::Validate(4),
                    ));
                }
            }
            slots.insert(name.clone(), b.clone());
        }
    }
    slots
}

/// The root `AgentProcess`'s `native.slots`, when the root is native.
fn root_native_slots(doc: &HirDocument) -> Option<&BTreeMap<String, SlotBindings>> {
    let root = doc.node(&doc.root.semantic_id)?;
    if let KindRecord::AgentProcess(a) = &root.semantic {
        if let AgentProcessBody::Native(n) = &a.body {
            return Some(&n.slots);
        }
    }
    None
}

/// Whether the root `AgentProcess` is `hosted` (OpaqueProcess — T-LCD-15).
fn root_is_hosted(doc: &HirDocument) -> bool {
    doc.node(&doc.root.semantic_id)
        .and_then(|n| match &n.semantic {
            KindRecord::AgentProcess(a) => Some(matches!(a.body, AgentProcessBody::Hosted(_))),
            _ => None,
        })
        .unwrap_or(false)
}

/// A copy of `doc` with `assembly.slots` materialised onto the root `native` process —
/// the document stage 5 validates and `resolve` seals.
pub fn materialise(doc: &HirDocument, assembly: Option<&Assembly>) -> HirDocument {
    let mut d = doc.clone();
    if let Some(a) = assembly {
        let root_id = d.root.semantic_id.clone();
        if let Some(node) = d.nodes.iter_mut().find(|n| n.semantic_id() == root_id) {
            if let KindRecord::AgentProcess(ap) = &mut node.semantic {
                if let AgentProcessBody::Native(n) = &mut ap.body {
                    for (slot, b) in &a.slots {
                        n.slots.insert(slot.clone(), b.clone());
                    }
                }
            }
        }
    }
    d
}

// ── Stage 2 — class conformance ───────────────────────────────────────────────

fn stage2_class_conformance(
    assembly: &Assembly,
    effective: &BTreeMap<String, SlotBindings>,
    catalog: &dyn ClassCatalog,
    doc: &HirDocument,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
) {
    // The catalog itself must satisfy the contract/inputs invariants (AC-CC-01/-02).
    for class_id in catalog.class_ids() {
        if let Some(c) = catalog.class(&class_id) {
            if c.contract.is_empty()
                || c.contract.iter().any(|op| {
                    op.name.is_empty() || op.invariants.is_empty() || op.failure_modes.is_empty()
                })
            {
                diags.push(diag(
                    Code::ClassContractMalformed,
                    "/assembly",
                    &class_id,
                    &format!(
                        "class `{class_id}`'s contract lacks the operations/inputs/outputs/invariants/failure-modes shape"
                    ),
                    "declare the class contract as operation records",
                    kernel,
                    Stage::Validate(2),
                ));
            }
            for required in ["ModelProfile", "ResourceAccount"] {
                if !c.required_inputs.contains(required) {
                    diags.push(diag(
                        Code::ClassRequiredInputs,
                        "/assembly",
                        &class_id,
                        &format!("class `{class_id}`'s required_inputs omit `{required}` (T-LCD-08)"),
                        "declare `ModelProfile` and `ResourceAccount` in the class's required_inputs",
                        kernel,
                        Stage::Validate(2),
                    ));
                }
            }
        }
    }

    // Mandatory exactly-one classes must be bound.
    for class_id in catalog.class_ids() {
        if let Some(c) = catalog.class(&class_id) {
            if c.cardinality == Cardinality::ExactlyOne && !effective.contains_key(&c.slot_key) {
                diags.push(diag(
                    Code::ClassCardinality,
                    "/assembly/slots",
                    &c.slot_key,
                    &format!("mandatory exactly-one slot `{}` is unbound", c.slot_key),
                    "bind exactly one variant to the slot",
                    kernel,
                    Stage::Validate(2),
                ));
            }
        }
    }

    for (slot, bindings) in effective {
        let class = class_for_slot(catalog, slot);
        if class.is_none() {
            diags.push(diag(
                Code::ClassUnknown,
                &format!("/assembly/slots/{slot}"),
                slot,
                &format!("no catalog class owns slot `{slot}`"),
                "bind only slots named by a catalog class's `slot_key`",
                kernel,
                Stage::Validate(2),
            ));
            continue;
        }
        let class = class.unwrap();
        let count = match bindings {
            SlotBindings::One(_) => 1,
            SlotBindings::Many(v) => v.len(),
        };
        let shape_ok = match class.cardinality {
            Cardinality::ExactlyOne => matches!(bindings, SlotBindings::One(_)),
            Cardinality::Optional => matches!(bindings, SlotBindings::One(_)),
            Cardinality::OrderedMany => matches!(bindings, SlotBindings::Many(_)),
        };
        if !shape_ok || (class.cardinality == Cardinality::ExactlyOne && count != 1) {
            diags.push(diag(
                Code::ClassCardinality,
                &format!("/assembly/slots/{slot}"),
                slot,
                &format!(
                    "slot `{slot}` carries {count} binding(s) under cardinality `{}`",
                    class.cardinality.as_str()
                ),
                "match the binding shape to the class cardinality",
                kernel,
                Stage::Validate(2),
            ));
        }
        let bindings_vec: Vec<&SlotBinding> = match bindings {
            SlotBindings::One(b) => vec![b],
            SlotBindings::Many(v) => v.iter().collect(),
        };
        for (i, b) in bindings_vec.iter().enumerate() {
            let path = format!("/assembly/slots/{slot}/[{i}]");
            if b.variant.class_id != class.class_id {
                diags.push(diag(
                    Code::ClassVariantMismatch,
                    &path,
                    &b.variant.variant_id,
                    &format!(
                        "binding names class `{}` but slot `{slot}` is owned by `{}`",
                        b.variant.class_id, class.class_id
                    ),
                    "bind a variant of the slot's class",
                    kernel,
                    Stage::Validate(2),
                ));
            }
            // Params check against `base ∪ variant` schemas — only when the variant is
            // pinned and the catalog can read its record (a selector's schema is not yet
            // knowable; resolve's binding produces it).
            if let RefVersion::Pinned(vid) = &b.variant.version {
                if let Some(v) = catalog.variant(vid) {
                    for pname in b.params.keys() {
                        if !class.base_param_schema.contains_key(pname)
                            && !v.param_schema.contains_key(pname)
                        {
                            diags.push(diag(
                                Code::ParamUnknown,
                                &format!("{path}/params/{pname}"),
                                pname,
                                &format!(
                                    "param `{pname}` is declared by neither the class's base schema nor variant `{vid}`"
                                ),
                                "declare the parameter in the variant's `param_schema` or remove the binding",
                                kernel,
                                Stage::Validate(2),
                            ));
                        }
                    }
                }
            }
        }
    }
    let _ = assembly;
    let _ = doc;
}

/// The class owning a slot — matched on `slot_key` first, then `class_id` (Stage-1
/// classes use identical spellings for both).
fn class_for_slot(
    catalog: &dyn ClassCatalog,
    slot: &str,
) -> Option<hh_registry::records::ClassRecord> {
    for id in catalog.class_ids() {
        if let Some(c) = catalog.class(&id) {
            if c.slot_key == slot || c.class_id == slot {
                return Some(c);
            }
        }
    }
    None
}

// ── Stage 3 — parameter space ─────────────────────────────────────────────────

fn stage3_parameters(a: &Assembly, diags: &mut Vec<AssemblyDiagnostic>, kernel: &ProvenanceRecord) {
    // Every `values` key is declared.
    for k in a.values.keys() {
        if !a.parameters.contains_key(k) {
            diags.push(diag(
                Code::ParamUnknown,
                &format!("/assembly/values/{k}"),
                k,
                &format!("`values` key `{k}` has no declared `parameters` entry"),
                "declare the parameter or remove the value",
                kernel,
                Stage::Validate(3),
            ));
        }
    }
    // Every sweepable parameter has a domain.
    for (id, spec) in &a.parameters {
        if (spec.sweepable || spec.param_type == ParamType::Enum) && spec.domain.is_none() {
            diags.push(diag(
                Code::ParamMissingDomain,
                &format!("/assembly/parameters/{id}"),
                id,
                &format!("parameter `{id}` is sweepable/enum but declares no `domain`"),
                "declare the enumeration domain",
                kernel,
                Stage::Validate(3),
            ));
        }
        if spec.budget_relevant && spec.default.is_some() && !a.values.contains_key(id) {
            diags.push(warn(
                Code::ParamDefaultedBudgetRelevant,
                &format!("/assembly/parameters/{id}"),
                id,
                &format!(
                    "budget-relevant parameter `{id}` is filled only by its default — a `MatchSpec` may not omit it"
                ),
                "bind an explicit `values` entry",
                kernel,
                Stage::Validate(3),
            ));
        }
    }
    // `$param:`/`$entity:` markers — used set vs declared set, both directions.
    let mut markers: Vec<(String, String)> = Vec::new();
    let slots_json = hh_hir::wire::slots_json(&a.slots, false);
    markers_in_json(&slots_json, "/assembly/slots", &mut markers);
    let entities_json = Json::Obj(
        a.entities
            .iter()
            .map(|(k, v)| (k.clone(), entity_json(v)))
            .collect(),
    );
    markers_in_json(&entities_json, "/assembly/entities", &mut markers);
    markers_in_json(
        &Json::Obj(a.values.clone()),
        "/assembly/values",
        &mut markers,
    );
    markers_in_json(&Json::Obj(a.ext.clone()), "/assembly/ext", &mut markers);
    let mut used_params: BTreeSet<String> = BTreeSet::new();
    for (path, marker) in &markers {
        if let Some(id) = marker.strip_prefix(PARAM_MARKER) {
            used_params.insert(id.to_string());
            if !a.parameters.contains_key(id) {
                diags.push(diag(
                    Code::ParamUndeclaredRef,
                    path,
                    id,
                    &format!("`$param:{id}` has no declared `parameters` entry"),
                    "declare the parameter or fix the marker",
                    kernel,
                    Stage::Validate(3),
                ));
            }
        }
        if let Some(id) = marker.strip_prefix(ENTITY_MARKER) {
            if !a.entities.contains_key(id) {
                diags.push(diag(
                    Code::RefUnresolved,
                    path,
                    id,
                    &format!("`$entity:{id}` has no declared `entities` entry"),
                    "declare the entity or fix the marker",
                    kernel,
                    Stage::Validate(3),
                ));
            }
        }
    }
    for id in a.parameters.keys() {
        if !used_params.contains(id) && !a.values.contains_key(id) {
            diags.push(warn(
                Code::ParamUnused,
                &format!("/assembly/parameters/{id}"),
                id,
                &format!("declared parameter `{id}` is never consumed (`$param:`) nor valued"),
                "consume it via `$param:<id>` or remove the declaration",
                kernel,
                Stage::Validate(3),
            ));
        }
    }
    let _ = SECRET_MARKER;
}

fn entity_json(e: &EntityBinding) -> Json {
    match e {
        EntityBinding::Ref(r) => r.to_json(),
        EntityBinding::Inline { kind, record } => {
            Json::obj([("kind", Json::str(kind)), ("record", record.clone())])
        }
    }
}

// ── Stage 4 — constraints ─────────────────────────────────────────────────────

/// An operand name resolves to "holds" — a bound+enabled slot or a `values`-present
/// parameter (`{of, needs?, with?, min?, max?, ceiling?}` per kind — ADR-0240).
fn operand_state(
    name: &str,
    a: &Assembly,
    effective: &BTreeMap<String, SlotBindings>,
) -> Option<bool> {
    if let Some(b) = effective.get(name) {
        let enabled = match b {
            SlotBindings::One(x) => x.enabled,
            SlotBindings::Many(v) => v.iter().any(|x| x.enabled),
        };
        return Some(enabled);
    }
    if a.parameters.contains_key(name) {
        return Some(a.values.contains_key(name));
    }
    None
}

fn stage4_constraints(
    a: &Assembly,
    effective: &BTreeMap<String, SlotBindings>,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
) {
    for (i, c) in a.constraints.iter().enumerate() {
        let path = format!("/assembly/constraints/{i}");
        let subj = |key: &str| c.subject.get(key).and_then(Json::as_str);
        let of = subj("of");
        let unknown_operand = |name: &str, diags: &mut Vec<AssemblyDiagnostic>| {
            if operand_state(name, a, effective).is_none() {
                diags.push(diag(
                    Code::RefUnresolved,
                    &path,
                    name,
                    &format!("constraint operand `{name}` names no slot or parameter"),
                    "name a bound slot or a declared parameter",
                    kernel,
                    Stage::Validate(4),
                ));
                true
            } else {
                false
            }
        };
        match c.kind {
            ConstraintKind::Requires | ConstraintKind::Implies => {
                let (Some(of), Some(needs)) = (of, subj("needs")) else {
                    diags.push(diag(
                        Code::ConsViolation,
                        &path,
                        c.kind.as_str(),
                        &format!(
                            "`{}` needs `subject.of` and `subject.needs`",
                            c.kind.as_str()
                        ),
                        "author the constraint subject as `{of, needs}`",
                        kernel,
                        Stage::Validate(4),
                    ));
                    continue;
                };
                if unknown_operand(of, diags) || unknown_operand(needs, diags) {
                    continue;
                }
                if operand_state(of, a, effective) == Some(true)
                    && operand_state(needs, a, effective) == Some(false)
                {
                    diags.push(diag(
                        Code::ConsViolation,
                        &path,
                        of,
                        &format!(
                            "`{}` violated: `{of}` holds but `{needs}` does not",
                            c.kind.as_str()
                        ),
                        "bind the required operand or drop the constraint",
                        kernel,
                        Stage::Validate(4),
                    ));
                }
            }
            ConstraintKind::Conflicts => {
                let (Some(of), Some(with)) = (of, subj("with")) else {
                    diags.push(diag(
                        Code::ConsViolation,
                        &path,
                        c.kind.as_str(),
                        "`conflicts` needs `subject.of` and `subject.with`",
                        "author the constraint subject as `{of, with}`",
                        kernel,
                        Stage::Validate(4),
                    ));
                    continue;
                };
                if unknown_operand(of, diags) || unknown_operand(with, diags) {
                    continue;
                }
                if operand_state(of, a, effective) == Some(true)
                    && operand_state(with, a, effective) == Some(true)
                {
                    diags.push(diag(
                        Code::ConsViolation,
                        &path,
                        of,
                        &format!("`conflicts` violated: `{of}` and `{with}` both hold"),
                        "disable one operand or drop the constraint",
                        kernel,
                        Stage::Validate(4),
                    ));
                }
            }
            ConstraintKind::Range => {
                let Some(of) = of else {
                    diags.push(diag(
                        Code::ConsViolation,
                        &path,
                        c.kind.as_str(),
                        "`range` needs `subject.of`",
                        "author the constraint subject as `{of, min?, max?}`",
                        kernel,
                        Stage::Validate(4),
                    ));
                    continue;
                };
                if unknown_operand(of, diags) {
                    continue;
                }
                let lo = c.subject.get("min").and_then(Json::as_int);
                let hi = c.subject.get("max").and_then(Json::as_int);
                if let Some(v) = a.values.get(of).and_then(Json::as_int) {
                    if lo.is_some_and(|l| v < l) || hi.is_some_and(|h| v > h) {
                        diags.push(diag(
                            Code::ConsViolation,
                            &path,
                            of,
                            &format!(
                                "`range` violated: `values.{of}` is outside the declared range"
                            ),
                            "bring the value inside `{min, max}`",
                            kernel,
                            Stage::Validate(4),
                        ));
                    }
                }
            }
            ConstraintKind::AuthorityCap => {
                // The cap's ceiling must name a declared authority class; the monotone
                // check itself is `compose`'s (Stage 3 — C-COMP-1).
                match c.subject.get("ceiling").and_then(Json::as_str) {
                    Some(s) if AuthorityClass::parse(s).is_some() => {}
                    _ => diags.push(diag(
                        Code::ConsViolation,
                        &path,
                        c.kind.as_str(),
                        "`authority_cap` needs `subject.ceiling` naming an authority class",
                        "author the ceiling as one of the seven authority classes",
                        kernel,
                        Stage::Validate(4),
                    )),
                }
            }
        }
        let _ = i;
    }
}

// ── Stage 7 — the LCD static battery ──────────────────────────────────────────

fn stage7_lcd(
    doc: &HirDocument,
    assembly: Option<&Assembly>,
    hosted: bool,
    derived: &mut DerivedResults,
    diags: &mut Vec<AssemblyDiagnostic>,
    kernel: &ProvenanceRecord,
) {
    // 7-O — the opacity report over the document's leaves (n/a{class} on hosted roots —
    // a hosted participant has no visible leaf content at the object level).
    if !hosted {
        let mut classes: Vec<AuthorityClass> = Vec::new();
        let mut without_interface = 0usize;
        for n in &doc.nodes {
            let sj = hh_hir::wire::node_to_json(n);
            if let Some(sem) = sj.get("semantic") {
                collect_leaf_authorities(sem, &mut classes, &mut without_interface);
            }
        }
        let rep = OpacityReport::over(classes.iter().copied());
        let opaque = rep.count(AuthorityClass::Unverified) + rep.count(AuthorityClass::External);
        derived.opacity = Some(OpacitySummary {
            by_class: rep
                .by_class
                .iter()
                .map(|(c, n)| (c.as_str().to_string(), *n))
                .collect(),
            total: rep.total(),
            opaque_ratio_num: opaque + without_interface,
        });
    }

    // 7-T10 — `IdentityIncludesSurface`: no surface/provenance/version member may appear
    // inside a semantic projection; recomputing the root semantic id must reproduce the
    // recorded coordinate.
    let mut includes_surface = false;
    for n in &doc.nodes {
        let sj = hh_hir::wire::node_semantic_projection(n);
        if contains_member(&sj, &["surface", "provenance", "version"]) {
            includes_surface = true;
        }
    }
    if includes_surface {
        diags.push(diag(
            Code::LcdIdentityIncludesSurface,
            "/",
            "semantic-projection",
            "a semantic projection carries a surface/provenance/version member (T-10)",
            "keep identity-bearing content free of surface records",
            kernel,
            Stage::Validate(7),
        ));
    }
    let recomputed = doc
        .node(&doc.root.semantic_id)
        .map(|n| n.semantic_id() == doc.root.semantic_id)
        .unwrap_or(false);
    derived.identity_stability = Some(recomputed && !includes_surface);
    if !recomputed {
        diags.push(diag(
            Code::LcdIdentityIncludesSurface,
            "/",
            "root",
            "recomputed root `semantic_id` differs from the recorded coordinate",
            "identity is over the canonical semantic projection — check for a non-canonical member",
            kernel,
            Stage::Validate(7),
        ));
    }

    // 7-L4 — hosting edges: `hosting_edges = []` at C0; any `hosts`/`hosted_by`-shaped
    // edge is a violation.
    for e in &doc.edges {
        let tag = format!("{:?}", e.kind);
        if tag.to_ascii_lowercase().contains("host") {
            derived.hosting_edges.push(tag.clone());
            diags.push(diag(
                Code::LcdHostingEdge,
                "/edges",
                &tag,
                "a hosting edge in the definition (hosting_edges must be `[]` at C0)",
                "hosting binds at instantiate, never as a definition edge",
                kernel,
                Stage::Validate(7),
            ));
        }
    }

    // 7-T12 — contracts are operation records, never inheritance: scan the document's
    // ext/assembly members for `inherits`/`extends`-shaped keys.
    let mut scan_targets = Vec::new();
    for n in &doc.nodes {
        scan_targets.push(hh_hir::wire::node_to_json(n));
    }
    if let Some(a) = assembly {
        scan_targets.push(a.to_json());
    }
    for t in &scan_targets {
        if contains_member(t, &["inherits", "extends", "superclass"]) {
            diags.push(diag(
                Code::LcdInheritanceContract,
                "/",
                "contract",
                "an `inherits`/`extends` member — contracts are operation records, never inheritance (T-12)",
                "express the contract as `ContractOperation`s",
                kernel,
                Stage::Validate(7),
            ));
            break;
        }
    }

    // 7-L2 — the benchmark-conditioned-rule scan (error, never warning).
    for hit in benchmark_hits(doc, assembly) {
        derived.benchmark_conditioned_rules.push(hit.clone());
        diags.push(diag(
            Code::LcdBenchmarkConditionedRule,
            &hit,
            &hit,
            "a benchmark coordinate reference (suite_id/task_id/foreign_id/split/family) in the definition — rules must condition on declared profiles, never on the benchmark (ADR-0143 D2)",
            "remove the benchmark reference; condition on a declared profile instead",
            kernel,
            Stage::Validate(7),
        ));
    }
}

/// Collect leaf authority classes from a semantic-record JSON — a `Text` leaf carries
/// `content_hash` + `authority`; a `CompiledPayload` carries `bytes_hash` + `provenance`
/// (whose `authority` is the leaf's class). Objects matching neither shape are walked.
fn collect_leaf_authorities(
    j: &Json,
    out: &mut Vec<AuthorityClass>,
    without_interface: &mut usize,
) {
    match j {
        Json::Obj(m) => {
            if m.contains_key("content_hash") {
                if let Some(a) = m
                    .get("authority")
                    .and_then(Json::as_str)
                    .and_then(AuthorityClass::parse)
                {
                    out.push(a);
                }
                return;
            }
            if m.contains_key("bytes_hash") {
                let auth = m
                    .get("provenance")
                    .and_then(|p| p.get("authority"))
                    .and_then(Json::as_str)
                    .and_then(AuthorityClass::parse);
                if let Some(a) = auth {
                    out.push(a);
                }
                if m.get("interface").is_none() && m.get("declared_interface").is_none() {
                    *without_interface += 1;
                }
                return;
            }
            for v in m.values() {
                collect_leaf_authorities(v, out, without_interface);
            }
        }
        Json::Arr(items) => {
            for v in items {
                collect_leaf_authorities(v, out, without_interface);
            }
        }
        _ => {}
    }
}

/// Whether `j` contains an object member named one of `keys` (recursive).
fn contains_member(j: &Json, keys: &[&str]) -> bool {
    match j {
        Json::Obj(m) => {
            m.keys().any(|k| keys.contains(&k.as_str()))
                || m.values().any(|v| contains_member(v, keys))
        }
        Json::Arr(items) => items.iter().any(|v| contains_member(v, keys)),
        _ => false,
    }
}

/// The 7-L2 scan — member keys naming a benchmark coordinate anywhere in the document
/// or assembly, plus the token spellings inside `Text` leaf content.
pub fn benchmark_hits(doc: &HirDocument, assembly: Option<&Assembly>) -> Vec<String> {
    let mut hits = Vec::new();
    for n in &doc.nodes {
        let j = hh_hir::wire::node_to_json(n);
        bench_keys(&j, &format!("/nodes/{}", n.semantic_id()), &mut hits);
        bench_text(&j, &format!("/nodes/{}", n.semantic_id()), &mut hits);
    }
    if let Some(a) = assembly {
        let j = a.to_json();
        bench_keys(&j, "/assembly", &mut hits);
    }
    hits.sort();
    hits.dedup();
    hits
}

fn bench_keys(j: &Json, path: &str, hits: &mut Vec<String>) {
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                let kl = k.to_ascii_lowercase();
                if BENCH_TOKENS.contains(&kl.as_str())
                    || kl.ends_with("_suite_id")
                    || kl.ends_with("_task_id")
                    || kl.ends_with("_foreign_id")
                {
                    hits.push(format!("{path}/{k}"));
                }
                bench_keys(v, &format!("{path}/{k}"), hits);
            }
        }
        Json::Arr(items) => {
            for (i, v) in items.iter().enumerate() {
                bench_keys(v, &format!("{path}[{i}]"), hits);
            }
        }
        _ => {}
    }
}

fn bench_text(j: &Json, path: &str, hits: &mut Vec<String>) {
    if let Json::Obj(m) = j {
        if m.contains_key("content_hash") {
            if let Some(Json::Str(content)) = m.get("content") {
                for tok in BENCH_TOKENS {
                    if content
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .any(|w| w == *tok)
                    {
                        hits.push(format!("{path}: text leaf mentions `{tok}`"));
                    }
                }
            }
            return;
        }
        for (k, v) in m {
            bench_text(v, &format!("{path}/{k}"), hits);
        }
    } else if let Json::Arr(items) = j {
        for (i, v) in items.iter().enumerate() {
            bench_text(v, &format!("{path}[{i}]"), hits);
        }
    }
}

// ── diagnostic constructors ──────────────────────────────────────────────────

fn diag(
    code: Code,
    path: &str,
    subject: &str,
    detail: &str,
    remedy: &str,
    kernel: &ProvenanceRecord,
    stage: Stage,
) -> AssemblyDiagnostic {
    AssemblyDiagnostic {
        code,
        class: None,
        severity: Severity::Error,
        path: path.to_string(),
        source_layer: None,
        subject: subject.to_string(),
        stage,
        detail: detail_text(detail, kernel),
        remedy: remedy.to_string(),
        owner_adr: "ADR-0148".into(),
    }
}

fn warn(
    code: Code,
    path: &str,
    subject: &str,
    detail: &str,
    remedy: &str,
    kernel: &ProvenanceRecord,
    stage: Stage,
) -> AssemblyDiagnostic {
    let mut d = diag(code, path, subject, detail, remedy, kernel, stage);
    d.severity = Severity::Warning;
    d
}

/// The report's `status` for external callers.
pub fn status_of(r: &ValidationReport) -> ReportStatus {
    r.status
}
