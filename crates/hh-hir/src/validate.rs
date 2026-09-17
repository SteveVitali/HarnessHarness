//! `validate` (§3.1.4/§3.1.7) — the static check battery over a `HirDocument`. Every §3.1.4
//! invariant is represented by a check; failures are **collected** (a document reports every
//! violation it carries), never warnings.
//!
//! The battery:
//! - shape: `kind == semantic.kind()`, surface only on surface-carrying kinds, ext key
//!   discipline, member dialects, T-LCD-01 statics (no model-identity field outside
//!   `ProfileRef`/`surface`), `UnclassifiedKind` wiring through `hh-ontology`;
//! - provenance: `ProvenanceRecord::validate` on every node, edge and `Text`/`CompiledPayload`
//!   leaf; the scope write ceiling; R-TEXT per leaf;
//! - references: every `Ref` resolves to a node of an allowed kind; edge endpoints resolve
//!   with the §3.1.3 endpoint kinds; `supersedes` same-kind; `derived-from` self-binding;
//! - acyclicity of `depends-on` / `supersedes` / `delegated-to`;
//! - V-EFF: procedure `derived_effects ⊆ ⋃ authorizes` in scope (by domain and scope);
//! - delegation attenuation (`delegated-to` and `Delegate` steps: child grants ⊆ delegable
//!   parent grants; child budget ≤ parent budget);
//! - budget containment (`Budget.parent`, `Goal.budget ≤ parent.budget` dimension-wise);
//! - conditioned-rule debt (T-LCD-05) and judge rules;
//! - `Observation.authority ≤ authority(source)`; claim-only-evidence refusal;
//! - `Opaque`/`CompiledPayload` declared-interface presence; `hosted` invariants (no `none`
//!   mechanism — CF-351; `version_identity = H(declaration ∥ participant_version)` when
//!   carried).

use std::collections::{BTreeMap, BTreeSet};

use hh_provenance::{AuthorityClass, ProvenanceError, ProvenanceRecord};
use hh_wire::json::Json;

use crate::document::{Edge, HirDocument, Node};
use crate::errors::{HirError, ValidationReport};
use crate::kinds::{
    EdgeKind, EffectClass, EntityKind, ToolEffects, ValidatorExecutable, ValidatorKind,
};
use crate::leaves::{CompiledPayload, Text};
use crate::records::*;
use crate::refs::Ref;

/// `validate(doc) → report | [error]` (§3.1.7). Collects every violation.
pub fn validate(doc: &HirDocument) -> Result<ValidationReport, Vec<HirError>> {
    let mut errs: Vec<HirError> = Vec::new();

    if doc.hir_version != crate::DIALECT {
        errs.push(HirError::DialectUnsupported {
            dialect: doc.hir_version.clone(),
        });
        return Err(errs);
    }

    // Index by semantic id (computed when the version record doesn't carry it yet).
    let index: BTreeMap<String, &Node> = doc.nodes.iter().map(|n| (n.semantic_id(), n)).collect();

    for node in &doc.nodes {
        check_node_shape(node, &mut errs);
        check_member_provenance(&node.provenance, node.version.sealed, "node", &mut errs);
        check_kind_record(node, &index, &mut errs);
        check_ext(&node.ext, &mut errs);
        check_model_identity_statics(node, &mut errs);
    }
    for edge in &doc.edges {
        check_member_provenance(&edge.provenance, edge.version.sealed, "edge", &mut errs);
        check_edge(edge, doc, &index, &mut errs);
    }

    check_refs_and_endpoints(doc, &index, &mut errs);
    check_acyclic(doc, &mut errs);
    check_effect_coverage(doc, &index, &mut errs);
    check_capability_procedure_sources(doc, &index, &mut errs);
    check_discovery_capability(doc, &mut errs);
    check_delegation(doc, &index, &mut errs);
    check_budgets(doc, &index, &mut errs);
    check_root(doc, &index, &mut errs);
    // Conditioned-rule debt is checked in `check_kind_record` (static half); the diff half
    // (an op touching a conditioned rule must carry/update its debt record) is `crate::diff`.

    if errs.is_empty() {
        Ok(ValidationReport {
            checks_run: CHECKS.to_vec(),
            node_count: doc.nodes.len(),
            edge_count: doc.edges.len(),
        })
    } else {
        Err(errs)
    }
}

/// The check names reported in [`ValidationReport::checks_run`].
const CHECKS: &[&str] = &[
    "dialect",
    "shape",
    "unclassified_kind",
    "surface_placement",
    "ext_keys",
    "model_identity_statics",
    "provenance_records",
    "scope_ceiling",
    "text_authority",
    "opaque_interfaces",
    "ref_resolution",
    "edge_endpoints",
    "acyclicity",
    "supersedes_same_kind",
    "effect_coverage",
    "delegation_attenuation",
    "budget_containment",
    "conditioned_rules",
    "validator_inputs",
    "observation_authority",
    "hosted_invariants",
    "capability_v_e1",
    "capability_procedure_source",
    "discovery_capability",
    "root",
];

// ─────────────────────────────────────────────────────────────────────────────
// Per-node shape
// ─────────────────────────────────────────────────────────────────────────────

fn check_node_shape(node: &Node, errs: &mut Vec<HirError>) {
    if node.semantic.kind() != node.kind {
        errs.push(HirError::SchemaViolation {
            detail: format!(
                "node kind {} carries a {} record",
                node.kind.name(),
                node.semantic.kind().name()
            ),
        });
    }
    // CC10 wiring: the kind must classify in the ontology's home table (DF-S1.1-1 — total
    // by construction; this exercises the boundary so a future unregistered kind fails).
    if hh_ontology::planes::classify_home_identifier(node.kind.name()).is_err() {
        errs.push(HirError::UnclassifiedKind {
            kind: node.kind.name().to_string(),
        });
    }
    if node.surface.is_some() && !node.kind.has_surface() {
        errs.push(HirError::UnexpressibleSurface {
            detail: format!("surface record on {}", node.kind.name()),
        });
    }
    if node.version.dialect != crate::DIALECT {
        errs.push(HirError::DialectUnsupported {
            dialect: node.version.dialect.clone(),
        });
    }
}

fn check_member_provenance(
    p: &ProvenanceRecord,
    sealed: bool,
    what: &str,
    errs: &mut Vec<HirError>,
) {
    if let Err(e) = p.validate(None) {
        errs.push(map_provenance_error(e, what));
    }
    // The scope write ceiling (§8.1 #3): a member stamped `sealed` at `definition` scope must
    // carry the seal-conferred `definition` authority. (Unsealed members at definition scope
    // are pre-seal content — `seal` raises them.)
    if sealed
        && p.scope == hh_provenance::PersistenceScope::Definition
        && p.authority < AuthorityClass::Definition
    {
        errs.push(HirError::ScopeCeilingExceeded {
            detail: format!(
                "{what}: sealed member scoped definition carries {}",
                p.authority.as_str()
            ),
        });
    }
}

fn map_provenance_error(e: ProvenanceError, what: &str) -> HirError {
    map_provenance_error_pub(e, what)
}

/// The `ProvenanceError → HirError` map, shared with `crate::diff`.
pub(crate) fn map_provenance_error_pub(e: ProvenanceError, what: &str) -> HirError {
    match e {
        ProvenanceError::MissingProvenance { .. } => HirError::MissingProvenance {
            what: what.to_string(),
        },
        ProvenanceError::TaintedAboveExternal { authority } => HirError::TaintedAboveExternal {
            authority: format!("{what}: {}", authority.as_str()),
        },
        ProvenanceError::AuthorityExceedsOrigin { ceiling, claimed } => {
            HirError::AuthorityExceedsOrigin {
                ceiling: ceiling.as_str().to_string(),
                claimed: format!("{what}: {}", claimed.as_str()),
            }
        }
        ProvenanceError::AttestationFailed { reason } => HirError::IllegitimateEndorsement {
            detail: format!("{what}: attestation failed: {reason}"),
        },
        other => HirError::SchemaViolation {
            detail: format!("{what}: provenance: {other:?}"),
        },
    }
}

fn check_ext(ext: &BTreeMap<String, Json>, errs: &mut Vec<HirError>) {
    for k in ext.keys() {
        if !k.contains('/') {
            errs.push(HirError::SchemaViolation {
                detail: format!("ext key {k:?} is not a prefixed key (org.name/key)"),
            });
        } else if k.starts_with("hir/") {
            // `hir/` is the reserved kernel namespace — authored content may not write it.
            errs.push(HirError::SchemaViolation {
                detail: format!("ext key {k:?} uses the reserved hir/ namespace"),
            });
        }
    }
}

/// T-LCD-01 statics (AC-IR-02): no model-identity-typed field outside `ProfileRef` and
/// `surface`. The fixed product types make such a field inexpressible in schema; this scan
/// is the regression guard over the structured `Json` slots (predicates, params, policies).
fn check_model_identity_statics(node: &Node, errs: &mut Vec<HirError>) {
    const DENY: &[&str] = &[
        "model",
        "model_id",
        "model_name",
        "model_ref",
        "model_identifier",
        "provider",
        "llm",
        "llm_config",
    ];
    let semantic = crate::schema::semantic_record_json(&node.semantic, false);
    let mut hits = Vec::new();
    scan_keys(&semantic, DENY, &mut hits);
    for h in hits {
        errs.push(HirError::UnexpressibleSurface {
            detail: format!(
                "{}: model-identity field {h:?} outside ProfileRef/surface (T-LCD-01)",
                node.kind.name()
            ),
        });
    }
}

fn scan_keys(j: &Json, deny: &[&str], hits: &mut Vec<String>) {
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                if deny.contains(&k.as_str()) {
                    hits.push(k.clone());
                }
                scan_keys(v, deny, hits);
            }
        }
        Json::Arr(items) => items.iter().for_each(|v| scan_keys(v, deny, hits)),
        _ => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Leaves and per-kind semantic checks
// ─────────────────────────────────────────────────────────────────────────────

fn check_text_leaf(t: &Text, path: &str, errs: &mut Vec<HirError>) {
    if let Err(e) = t.check_authority(path) {
        errs.push(e);
    }
    if !t.content_consistent() {
        errs.push(HirError::SchemaViolation {
            detail: format!("{path}: content does not match content_hash"),
        });
    }
    if let Err(e) = t.provenance.validate(None) {
        errs.push(map_provenance_error(e, path));
    }
}

fn check_payload_leaf(p: &CompiledPayload, path: &str, errs: &mut Vec<HirError>) {
    // T-LCD-02: opacity is legal only as the body of a declared interface.
    if p.declared_interface.is_none() {
        errs.push(HirError::OpaqueWithoutInterface {
            detail: format!("{path}: CompiledPayload without declared interface"),
        });
    }
    if let Err(e) = p.provenance.validate(None) {
        errs.push(map_provenance_error(e, path));
    }
}

/// A ref plus the entity kinds it may target (empty = any kind).
pub(crate) struct RefSite {
    path: &'static str,
    allowed: &'static [EntityKind],
}

fn collect_refs<'a>(rec: &'a KindRecord, out: &mut Vec<(RefSite, &'a Ref)>) {
    fn push<'a>(
        out: &mut Vec<(RefSite, &'a Ref)>,
        path: &'static str,
        allowed: &'static [EntityKind],
        r: &'a Ref,
    ) {
        out.push((RefSite { path, allowed }, r));
    }
    match rec {
        KindRecord::Goal(g) => {
            for r in &g.success_criteria {
                push(
                    out,
                    "success_criteria",
                    &[EntityKind::Validator, EntityKind::Procedure],
                    r,
                );
            }
            push(out, "budget", &[EntityKind::Budget], &g.budget);
            if let Some(p) = &g.parent {
                push(out, "parent", &[EntityKind::Goal], p);
            }
        }
        KindRecord::Observation(o) => {
            if let ObservationContent::Artifact(r) = &o.content {
                push(out, "content", &[EntityKind::Artifact], r);
            }
        }
        KindRecord::ContextItem(c) => {
            push(
                out,
                "payload",
                &[
                    EntityKind::Memory,
                    EntityKind::Observation,
                    EntityKind::Artifact,
                    EntityKind::Procedure,
                    EntityKind::ToolCapability,
                ],
                &c.payload,
            );
            if let Some(p) = &c.placement_policy {
                push(out, "placement_policy", &[EntityKind::HarnessRule], p);
            }
        }
        KindRecord::Memory(_) => {}
        KindRecord::Procedure(p) => {
            for r in &p.allowed_capabilities {
                push(
                    out,
                    "allowed_capabilities",
                    &[EntityKind::ToolCapability],
                    r,
                );
            }
            collect_step_refs(&p.steps, out);
        }
        KindRecord::ToolCapability(t) => {
            for r in &t.postconditions {
                push(
                    out,
                    "postconditions",
                    &[EntityKind::Validator, EntityKind::Observation],
                    r,
                );
            }
            if let ToolEffects::Declared(effects) = &t.effects {
                for e in effects {
                    collect_effect_refs(e, out);
                }
            }
        }
        KindRecord::Permission(p) => {
            push(out, "holder", &[EntityKind::AgentProcess], &p.holder);
        }
        KindRecord::Effect(e) => {
            collect_effect_refs(&e.declared, out);
        }
        KindRecord::Artifact(a) => {
            push(
                out,
                "produced_by",
                &[
                    EntityKind::Validator,
                    EntityKind::AgentProcess,
                    EntityKind::Procedure,
                ],
                &a.produced_by,
            );
        }
        KindRecord::Validator(v) => {
            for r in &v.inputs {
                push(out, "inputs", &[], r);
            }
            if let ValidatorKind::Executable(ValidatorExecutable::Invoke(r)) = &v.kind {
                push(out, "kind", &[EntityKind::Procedure], r);
            }
        }
        KindRecord::AgentProcess(a) => match &a.body {
            AgentProcessBody::Native(n) => {
                push(
                    out,
                    "harness_def",
                    &[EntityKind::AgentProcess],
                    &n.harness_def,
                );
                push(out, "budget", &[EntityKind::Budget], &n.budget);
                push(
                    out,
                    "permissions",
                    &[EntityKind::Permission],
                    &n.permissions,
                );
            }
            AgentProcessBody::Hosted(h) => {
                for r in &h.supplies.context {
                    push(out, "supplies.context", &[EntityKind::ContextItem], r);
                }
                for r in &h.supplies.tools {
                    push(out, "supplies.tools", &[EntityKind::ToolCapability], r);
                }
                for r in &h.supplies.procedures {
                    push(out, "supplies.procedures", &[EntityKind::Procedure], r);
                }
                push(out, "budget", &[EntityKind::Budget], &h.budget);
                push(
                    out,
                    "permissions",
                    &[EntityKind::Permission],
                    &h.permissions,
                );
            }
        },
        KindRecord::Budget(b) => {
            if let Some(p) = &b.parent {
                push(out, "parent", &[EntityKind::Budget], p);
            }
            push(out, "accounting", &[], &b.accounting);
        }
        KindRecord::HarnessRule(r) => match &r.action {
            RuleAction::InsertContextItem(r) => push(out, "action", &[EntityKind::ContextItem], r),
            RuleAction::RestrictToolSet(rs) => {
                for r in rs {
                    push(out, "action", &[EntityKind::ToolCapability], r);
                }
            }
            RuleAction::RequireValidator(r) => push(out, "action", &[EntityKind::Validator], r),
            _ => {}
        },
    }
}

fn collect_step_refs<'a>(steps: &'a [ProcedureStep], out: &mut Vec<(RefSite, &'a Ref)>) {
    for s in steps {
        match s {
            ProcedureStep::Invoke { tool, .. } => {
                out.push((
                    RefSite {
                        path: "steps[].tool",
                        allowed: &[EntityKind::ToolCapability],
                    },
                    tool,
                ));
            }
            ProcedureStep::Delegate {
                budget, permission, ..
            } => {
                out.push((
                    RefSite {
                        path: "steps[].budget",
                        allowed: &[EntityKind::Budget],
                    },
                    budget,
                ));
                out.push((
                    RefSite {
                        path: "steps[].permission",
                        allowed: &[EntityKind::Permission],
                    },
                    permission,
                ));
            }
            ProcedureStep::Branch {
                then_body,
                else_body,
                ..
            } => {
                collect_step_refs(then_body, out);
                collect_step_refs(else_body, out);
            }
            ProcedureStep::Loop { bound, body } => {
                out.push((
                    RefSite {
                        path: "steps[].bound",
                        allowed: &[EntityKind::Budget],
                    },
                    bound,
                ));
                collect_step_refs(body, out);
            }
            ProcedureStep::Verify { validator } => {
                out.push((
                    RefSite {
                        path: "steps[].validator",
                        allowed: &[EntityKind::Validator],
                    },
                    validator,
                ));
            }
            ProcedureStep::Opaque(p) => {
                if let Some(iface) = &p.declared_interface {
                    for e in &iface.effects {
                        collect_effect_refs(e, out);
                    }
                }
            }
            ProcedureStep::Instruction(_) => {}
        }
    }
}

fn collect_effect_refs<'a>(e: &'a EffectClass, out: &mut Vec<(RefSite, &'a Ref)>) {
    if let Some(attrs) = &e.attributes {
        if let crate::kinds::Reversibility::Reversible(r) = &attrs.reversibility {
            out.push((
                RefSite {
                    path: "reversibility",
                    allowed: &[EntityKind::Procedure],
                },
                r,
            ));
        }
    }
}

fn check_refs_and_endpoints(
    doc: &HirDocument,
    index: &BTreeMap<String, &Node>,
    errs: &mut Vec<HirError>,
) {
    for node in &doc.nodes {
        let mut refs = Vec::new();
        collect_refs(&node.semantic, &mut refs);
        for (site, r) in refs {
            match index.get(&r.semantic_id) {
                None => errs.push(HirError::UnresolvedRef {
                    detail: format!(
                        "{}.{} -> {} does not resolve in the document",
                        node.kind.name(),
                        site.path,
                        r.semantic_id
                    ),
                }),
                Some(target) => {
                    if !site.allowed.is_empty() && !site.allowed.contains(&target.kind) {
                        errs.push(HirError::UnresolvedRef {
                            detail: format!(
                                "{}.{} -> {} (kind {} not in {:?})",
                                node.kind.name(),
                                site.path,
                                r.semantic_id,
                                target.kind.name(),
                                site.allowed
                            ),
                        });
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-kind semantic invariants (§3.1.3/§3.1.4)
// ─────────────────────────────────────────────────────────────────────────────

fn check_kind_record(node: &Node, index: &BTreeMap<String, &Node>, errs: &mut Vec<HirError>) {
    match &node.semantic {
        KindRecord::Goal(g) => {
            check_text_leaf(&g.statement, "goal.statement", errs);
            if let Some(r) = &g.unverifiable_reason {
                check_text_leaf(r, "goal.unverifiable_reason", errs);
            }
            if g.success_criteria.is_empty() && g.unverifiable_reason.is_none() {
                errs.push(HirError::SchemaViolation {
                    detail: "goal: success_criteria empty without unverifiable_reason".into(),
                });
            }
        }
        KindRecord::Observation(o) => {
            // `authority ≤ authority(source)` (§3.1.4).
            let ceiling = o.source.authority_ceiling();
            if o.authority > ceiling {
                errs.push(HirError::AuthorityExceedsOrigin {
                    ceiling: ceiling.as_str().to_string(),
                    claimed: o.authority.as_str().to_string(),
                });
            }
            if let ObservationContent::Text(t) = &o.content {
                check_text_leaf(t, "observation.content", errs);
            }
        }
        KindRecord::Memory(m) => {
            check_text_leaf(&m.content, "memory.content", errs);
            // Extraction is bounded: memory authority ≤ external (§3.1.4).
            if m.authority > AuthorityClass::External {
                errs.push(HirError::AuthorityExceedsOrigin {
                    ceiling: AuthorityClass::External.as_str().to_string(),
                    claimed: m.authority.as_str().to_string(),
                });
            }
            if !m.validity.well_formed() {
                errs.push(HirError::SchemaViolation {
                    detail: "memory.validity carries both until and condition".into(),
                });
            }
        }
        KindRecord::Procedure(p) => {
            check_steps(&p.steps, errs);
        }
        KindRecord::ToolCapability(t) => {
            check_text_leaf(&t.purpose, "tool.purpose", errs);
            // V-E1 record checks (§5d.1 §3; ADR-0087 D6) — `authority` is the node's
            // conferred provenance class (lifted ⇒ `unverified`, V-E1-7).
            errs.extend(crate::tools::validate_capability(
                t,
                node.provenance.authority,
            ));
            // The authored `exposure_mode` member (OQ-240 interim — admitted_modes
            // spellings are the run-time sum; closed only).
            if let Some(SurfaceRecord::Tool(ts)) = &node.surface {
                if let Err(e) = crate::tools::authored_exposure(Some(&ts.exposure_mode)) {
                    errs.push(e);
                }
            }
        }
        KindRecord::Permission(p) => {
            // `issuer.authority ≠ model_claim` — a model claim cannot confer a permission
            // (§3.1.4). The delegate class is the model-claim class (§8.1 #4).
            if p.issuer.authority == AuthorityClass::Delegate {
                errs.push(HirError::IllegitimateEndorsement {
                    detail: "permission issued at delegate (model-claim) authority".into(),
                });
            }
            if !p.validity.well_formed() {
                errs.push(HirError::SchemaViolation {
                    detail: "permission.validity carries both until and condition".into(),
                });
            }
        }
        KindRecord::Validator(v) => {
            check_validator(node, v, index, errs);
        }
        KindRecord::AgentProcess(a) => match &a.body {
            AgentProcessBody::Native(n) => {
                // Slots must carry control_strategy and context_policy (§3.1.4).
                for required in ["control_strategy", "context_policy"] {
                    if !n.slots.contains_key(required) {
                        errs.push(HirError::SchemaViolation {
                            detail: format!("native process missing slot {required:?}"),
                        });
                    }
                }
                // The control boundary must validate (§2.3/CC11 record).
                if let Err(e) = n.control_boundary.validate() {
                    errs.push(HirError::SchemaViolation {
                        detail: format!("control_boundary: {e:?}"),
                    });
                }
            }
            AgentProcessBody::Hosted(h) => {
                check_hosted(h, errs);
            }
        },
        KindRecord::Budget(b) => {
            for (d, bound) in &b.dimensions {
                // DF-S1.4-1 (closed at S1.6): keys are the closed kernel registry plus
                // the derived bound names — unknown spellings are refused, never carried.
                if hh_ontology::dimensions::DimensionKey::parse(d).is_none() {
                    errs.push(HirError::SchemaViolation {
                        detail: format!(
                            "budget dimension {d:?}: not in the kernel dimension registry (§8.2)"
                        ),
                    });
                }
                if let (Some(h), Some(s)) = (bound.hard, bound.soft) {
                    if s > h {
                        errs.push(HirError::SchemaViolation {
                            detail: format!("budget dimension {d:?}: soft {s} > hard {h}"),
                        });
                    }
                }
            }
        }
        KindRecord::HarnessRule(r) => {
            // T-LCD-05: conditioned_on ≠ null ⇒ complete assumption-debt record.
            if r.conditioned_on.is_some() {
                match &r.assumption_debt {
                    Some(d) if debt_complete(d) && d.rule_id == r.rule_id => {}
                    _ => errs.push(HirError::ConditionedRuleIncomplete {
                        rule_id: r.rule_id.clone(),
                    }),
                }
            }
            if let Some(d) = &r.assumption_debt {
                check_text_leaf(&d.hypothesis, "rule.assumption_debt.hypothesis", errs);
            }
        }
        _ => {}
    }
}

fn check_steps(steps: &[ProcedureStep], errs: &mut Vec<HirError>) {
    for s in steps {
        match s {
            ProcedureStep::Instruction(t) => check_text_leaf(t, "steps[].instruction", errs),
            ProcedureStep::Opaque(p) => check_payload_leaf(p, "steps[].opaque", errs),
            ProcedureStep::Branch {
                then_body,
                else_body,
                ..
            } => {
                check_steps(then_body, errs);
                check_steps(else_body, errs);
            }
            ProcedureStep::Loop { body, .. } => check_steps(body, errs),
            _ => {}
        }
    }
}

fn check_validator(
    node: &Node,
    v: &ValidatorRecord,
    index: &BTreeMap<String, &Node>,
    errs: &mut Vec<HirError>,
) {
    // Claim-only evidence: a model claim can never alone satisfy a Validator (§3.1.4).
    let inputs: Vec<&Node> = v
        .inputs
        .iter()
        .filter_map(|r| index.get(&r.semantic_id).copied())
        .collect();
    if !inputs.is_empty()
        && inputs.iter().all(|n| {
            matches!(
                &n.semantic,
                KindRecord::Observation(o) if o.source == ObservationSource::ModelClaim
            )
        })
    {
        errs.push(HirError::IllegitimateEndorsement {
            detail: format!(
                "validator {} reads only model_claim observations",
                node.semantic_id()
            ),
        });
    }
    if let ValidatorKind::Judge(j) = &v.kind {
        // judge ⇒ deterministic = false ∧ profile_ref ∧ assumption_debt (§3.1.4).
        if v.deterministic {
            errs.push(HirError::SchemaViolation {
                detail: "judge validator declared deterministic".into(),
            });
        }
        check_text_leaf(&j.rubric, "validator.judge.rubric", errs);
        match &v.assumption_debt {
            Some(d) if debt_complete(d) => {}
            _ => errs.push(HirError::ConditionedRuleIncomplete {
                rule_id: node.semantic_id(),
            }),
        }
    }
    if let Some(d) = &v.assumption_debt {
        check_text_leaf(&d.hypothesis, "validator.assumption_debt.hypothesis", errs);
    }
    if let ValidatorKind::Executable(ValidatorExecutable::Payload(p)) = &v.kind {
        check_payload_leaf(p, "validator.kind.executable", errs);
    }
}

fn check_hosted(h: &OpaqueProcess, errs: &mut Vec<HirError>) {
    // CF-351: `hosting_mechanism` never `none` on an OpaqueProcess.
    if h.hosting_mechanism == hh_ontology::participant::HostingMechanism::None {
        errs.push(HirError::SchemaViolation {
            detail: "hosted body carries hosting_mechanism = none (CF-351)".into(),
        });
    }
    // `version_identity = H(declaration ∥ participant_version)` — validated when carried
    // (a claim, recomputed; §3.1.6).
    if let Some(vi) = &h.version_identity {
        let expected = crate::identity::opaque_version_identity(
            &h.declared_capabilities,
            &h.participant_version,
        );
        if *vi != expected {
            errs.push(HirError::SchemaViolation {
                detail: "hosted version_identity != H(declaration ∥ participant_version)".into(),
            });
        }
    }
}

fn debt_complete(d: &AssumptionDebtRecord) -> bool {
    !d.rule_id.is_empty()
        && !d.hypothesis.content_hash.is_empty()
        && !d.owner.is_empty()
        && !d.expiry_condition.is_empty()
        && !d.removal_test_ref.is_empty()
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge checks
// ─────────────────────────────────────────────────────────────────────────────

fn check_edge(
    edge: &Edge,
    _doc: &HirDocument,
    index: &BTreeMap<String, &Node>,
    errs: &mut Vec<HirError>,
) {
    if edge.fields.kind() != edge.kind {
        errs.push(HirError::SchemaViolation {
            detail: format!(
                "edge kind {} carries {} fields",
                edge.kind.name(),
                edge.fields.kind().name()
            ),
        });
    }
    let from = index.get(edge.from.as_str());
    let to = index.get(edge.to.as_str());
    if from.is_none() {
        errs.push(HirError::UnresolvedRef {
            detail: format!("edge {}: from {} unresolved", edge.kind.name(), edge.from),
        });
    }
    if to.is_none() {
        errs.push(HirError::UnresolvedRef {
            detail: format!("edge {}: to {} unresolved", edge.kind.name(), edge.to),
        });
    }
    let (Some(from), Some(to)) = (from, to) else {
        return;
    };

    let expect = |node: &Node, allowed: &[EntityKind], side: &str, errs: &mut Vec<HirError>| {
        if !allowed.contains(&node.kind) {
            errs.push(HirError::UnresolvedRef {
                detail: format!(
                    "edge {}: {side} kind {} not in {allowed:?}",
                    edge.kind.name(),
                    node.kind.name()
                ),
            });
        }
    };

    match edge.kind {
        EdgeKind::DependsOn => {}
        EdgeKind::Supersedes => {
            if from.kind != to.kind {
                errs.push(HirError::UnresolvedRef {
                    detail: format!(
                        "supersedes endpoints differ in kind ({} → {})",
                        from.kind.name(),
                        to.kind.name()
                    ),
                });
            }
        }
        EdgeKind::Authorizes => {
            expect(from, &[EntityKind::Permission], "from", errs);
            expect(
                to,
                &[
                    EntityKind::ToolCapability,
                    EntityKind::Procedure,
                    EntityKind::Effect,
                ],
                "to",
                errs,
            );
            if let EdgeRecord::Authorizes {
                effect_class: Some(ec),
                ..
            } = &edge.fields
            {
                let mut refs = Vec::new();
                collect_effect_refs(ec, &mut refs);
                for (_, r) in refs {
                    if !index.contains_key(&r.semantic_id) {
                        errs.push(HirError::UnresolvedRef {
                            detail: format!(
                                "authorizes.effect_class ref {} unresolved",
                                r.semantic_id
                            ),
                        });
                    }
                }
            }
        }
        EdgeKind::ProducedBy => {
            expect(
                from,
                &[
                    EntityKind::Observation,
                    EntityKind::Artifact,
                    EntityKind::Effect,
                    EntityKind::Memory,
                ],
                "from",
                errs,
            );
            expect(
                to,
                &[
                    EntityKind::Validator,
                    EntityKind::AgentProcess,
                    EntityKind::Procedure,
                ],
                "to",
                errs,
            );
        }
        EdgeKind::Validates => {
            expect(from, &[EntityKind::Validator], "from", errs);
            expect(
                to,
                &[
                    EntityKind::Goal,
                    EntityKind::Artifact,
                    EntityKind::Procedure,
                    EntityKind::Memory,
                    EntityKind::HarnessRule,
                ],
                "to",
                errs,
            );
        }
        EdgeKind::DelegatedTo => {
            expect(from, &[EntityKind::AgentProcess], "from", errs);
            expect(to, &[EntityKind::AgentProcess], "to", errs);
            if let EdgeRecord::DelegatedTo { permission, budget } = &edge.fields {
                for (r, allowed) in [
                    (permission, &[EntityKind::Permission][..]),
                    (budget, &[EntityKind::Budget][..]),
                ] {
                    match index.get(&r.semantic_id) {
                        None => errs.push(HirError::UnresolvedRef {
                            detail: format!("delegated-to ref {} unresolved", r.semantic_id),
                        }),
                        Some(n) if !allowed.contains(&n.kind) => {
                            errs.push(HirError::UnresolvedRef {
                                detail: format!(
                                    "delegated-to ref {} has kind {}",
                                    r.semantic_id,
                                    n.kind.name()
                                ),
                            })
                        }
                        _ => {}
                    }
                }
            }
        }
        EdgeKind::DerivedFrom => {
            // The edge's own record is the derivation — `to` is `from` (§3.1.3).
            if edge.from != edge.to {
                errs.push(HirError::SchemaViolation {
                    detail: "derived-from edge: to must equal from (self-binding)".into(),
                });
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Acyclicity (§3.1.4)
// ─────────────────────────────────────────────────────────────────────────────

fn check_acyclic(doc: &HirDocument, errs: &mut Vec<HirError>) {
    for kind in [
        EdgeKind::DependsOn,
        EdgeKind::Supersedes,
        EdgeKind::DelegatedTo,
    ] {
        // DFS over this edge kind's subgraph; a back edge is a cycle.
        let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for e in doc.edges.iter().filter(|e| e.kind == kind) {
            adj.entry(e.from.as_str()).or_default().push(e.to.as_str());
        }
        let mut state: BTreeMap<&str, u8> = BTreeMap::new(); // 0 unvisited, 1 in-stack, 2 done
        let mut stack: Vec<&str> = Vec::new();
        for start in adj.keys().copied().collect::<Vec<_>>() {
            if state.get(start).copied().unwrap_or(0) != 0 {
                continue;
            }
            stack.push(start);
            state.insert(start, 1);
            let mut path = vec![start];
            while let Some(&top) = stack.last() {
                let mut advanced = false;
                if let Some(succs) = adj.get(top) {
                    for &next in succs {
                        match state.get(next).copied().unwrap_or(0) {
                            1 => {
                                errs.push(HirError::CycleDetected {
                                    detail: format!("{}: {} closes a cycle", kind.name(), next),
                                });
                            }
                            0 => {
                                state.insert(next, 1);
                                stack.push(next);
                                path.push(next);
                                advanced = true;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                if !advanced {
                    state.insert(top, 2);
                    stack.pop();
                    path.pop();
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// V-EFF: derived effects ⊆ ∪ authorizes in scope (§3.1.4)
// ─────────────────────────────────────────────────────────────────────────────

fn scope_covers(pattern: &str, scope: &str) -> bool {
    // ResourcePattern coverage: `*` covers everything; a trailing `*` is a prefix match;
    // otherwise exact.
    pattern == "*"
        || pattern == scope
        || pattern
            .strip_suffix('*')
            .is_some_and(|p| scope.starts_with(p))
}

/// The effects a procedure derives: invoked tools' declared effects, `Opaque` steps'
/// declared-interface effects, and the grants a `Delegate` step hands down (recursively
/// through `Branch`/`Loop` bodies).
fn derived_effects<'a>(
    steps: &'a [ProcedureStep],
    index: &BTreeMap<String, &'a Node>,
    out: &mut Vec<EffectClass>,
) {
    for s in steps {
        match s {
            ProcedureStep::Invoke { tool, .. } => {
                if let Some(n) = index.get(tool.semantic_id.as_str()) {
                    if let KindRecord::ToolCapability(t) = &n.semantic {
                        if let ToolEffects::Declared(effects) = &t.effects {
                            out.extend(effects.iter().cloned());
                        }
                    }
                }
            }
            ProcedureStep::Opaque(p) => {
                if let Some(i) = &p.declared_interface {
                    out.extend(i.effects.iter().cloned());
                }
            }
            ProcedureStep::Delegate { permission, .. } => {
                if let Some(n) = index.get(permission.semantic_id.as_str()) {
                    if let KindRecord::Permission(p) = &n.semantic {
                        out.extend(p.grants.iter().map(|g| g.effect.clone()));
                    }
                }
            }
            ProcedureStep::Branch {
                then_body,
                else_body,
                ..
            } => {
                derived_effects(then_body, index, out);
                derived_effects(else_body, index, out);
            }
            ProcedureStep::Loop { body, .. } => derived_effects(body, index, out),
            _ => {}
        }
    }
}

fn check_effect_coverage(
    doc: &HirDocument,
    index: &BTreeMap<String, &Node>,
    errs: &mut Vec<HirError>,
) {
    for node in &doc.nodes {
        let KindRecord::Procedure(p) = &node.semantic else {
            continue;
        };
        let proc_id = node.semantic_id();
        let mut derived = Vec::new();
        derived_effects(&p.steps, index, &mut derived);
        for e in &derived {
            let covered = doc.edges.iter().any(|edge| {
                if edge.kind != EdgeKind::Authorizes {
                    return false;
                }
                let EdgeRecord::Authorizes {
                    scope,
                    effect_class,
                } = &edge.fields
                else {
                    return false;
                };
                // The edge must scope the procedure or its producing tool — or carry the
                // effect class outright.
                let scoped = edge.to == proc_id
                    || matches!(
                        effect_class,
                        Some(c) if c.covers(e)
                    )
                    || invoked_covers(index, node, edge.to.as_str(), e);
                if !scoped {
                    return false;
                }
                // The permission behind the edge must grant the effect at this scope.
                match index.get(edge.from.as_str()) {
                    Some(n) => match &n.semantic {
                        KindRecord::Permission(perm) => perm
                            .grants
                            .iter()
                            .any(|g| g.effect.covers(e) && scope_covers(&g.scope, scope)),
                        _ => false,
                    },
                    None => false,
                }
            });
            if !covered {
                errs.push(HirError::EffectUncovered {
                    detail: format!(
                        "procedure {proc_id}: derived effect {} uncovered by authorizes",
                        e.domain.name()
                    ),
                });
            }
        }
    }
}

/// An authorizes edge may target the tool whose declared effects include `e`.
fn invoked_covers(
    index: &BTreeMap<String, &Node>,
    proc_node: &Node,
    edge_to: &str,
    e: &EffectClass,
) -> bool {
    let KindRecord::Procedure(p) = &proc_node.semantic else {
        return false;
    };
    let mut tools = BTreeSet::new();
    collect_invoked_tools(&p.steps, &mut tools);
    if !tools.contains(edge_to) {
        return false;
    }
    index.get(edge_to).is_some_and(|n| match &n.semantic {
        KindRecord::ToolCapability(t) => match &t.effects {
            ToolEffects::Pure => false,
            ToolEffects::Declared(set) => set.iter().any(|d| d == e || e.covers(d)),
        },
        KindRecord::Effect(rec) => rec.declared == *e || e.covers(&rec.declared),
        _ => false,
    })
}

fn collect_invoked_tools(steps: &[ProcedureStep], out: &mut BTreeSet<String>) {
    for s in steps {
        match s {
            ProcedureStep::Invoke { tool, .. } => {
                out.insert(tool.semantic_id.clone());
            }
            ProcedureStep::Branch {
                then_body,
                else_body,
                ..
            } => {
                collect_invoked_tools(then_body, out);
                collect_invoked_tools(else_body, out);
            }
            ProcedureStep::Loop { body, .. } => collect_invoked_tools(body, out),
            _ => {}
        }
    }
}

/// V-E1-10 (§5d.1 §3): a `source.kind = "procedure"` capability must declare
/// exactly its source procedure's derived effects — the derivation is the
/// honest lower bound; drift is `DerivedEffectsMismatch`. An unresolvable
/// `source.ref` (a pinned version outside the document) is not a violation
/// here — endpoint resolution is `check_refs`' job and the registry's
/// admission re-checks co-registered pairs.
fn check_capability_procedure_sources(
    doc: &HirDocument,
    index: &BTreeMap<String, &Node>,
    errs: &mut Vec<HirError>,
) {
    for node in &doc.nodes {
        let KindRecord::ToolCapability(t) = &node.semantic else {
            continue;
        };
        if t.source.get("kind").and_then(Json::as_str) != Some("procedure") {
            continue;
        }
        let Some(target) = t
            .source
            .get("ref")
            .and_then(Json::as_str)
            .and_then(|r| index.get(r))
        else {
            continue;
        };
        let KindRecord::Procedure(p) = &target.semantic else {
            errs.push(HirError::DerivedEffectsMismatch {
                detail: format!(
                    "{}: source.ref does not resolve to a Procedure",
                    node.semantic_id()
                ),
            });
            continue;
        };
        let mut derived = Vec::new();
        derived_effects(&p.steps, index, &mut derived);
        let derived_set: BTreeSet<EffectClass> = derived.into_iter().collect();
        let declared = match &t.effects {
            ToolEffects::Declared(set) => set.clone(),
            ToolEffects::Pure => BTreeSet::new(),
        };
        if declared != derived_set {
            errs.push(HirError::DerivedEffectsMismatch {
                detail: format!(
                    "{}: declared {} effects, procedure derives {}",
                    node.semantic_id(),
                    declared.len(),
                    derived_set.len()
                ),
            });
        }
    }
}

/// I-DISCOVERY (§5d.3 §2; ADR-0093 D8): a definition that admits `deferred`
/// on any capability surface must carry a `discover_surfaces` capability
/// (`exposure_hint.discovery = true`) — deferred/indexed reachability has no
/// other path. The run-time half lives in `hh-compiler`'s `select_surfaces`.
fn check_discovery_capability(doc: &HirDocument, errs: &mut Vec<HirError>) {
    let admits_deferred = doc.nodes.iter().any(|n| {
        if !matches!(&n.semantic, KindRecord::ToolCapability(_)) {
            return false;
        }
        match &n.surface {
            Some(SurfaceRecord::Tool(ts)) => {
                crate::tools::authored_exposure(Some(&ts.exposure_mode))
                    .map(|a| {
                        a.admitted.contains(&crate::tools::ExposureMode::Deferred)
                            || a.admitted.contains(&crate::tools::ExposureMode::Indexed)
                    })
                    .unwrap_or(false)
            }
            _ => false,
        }
    });
    if !admits_deferred {
        return;
    }
    let has_discovery = doc.nodes.iter().any(|n| {
        matches!(&n.semantic, KindRecord::ToolCapability(t)
            if crate::tools::is_discovery_capability(t))
    });
    if !has_discovery {
        errs.push(HirError::NoDiscoverySurface);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Delegation attenuation + budget containment (§3.1.4)
// ─────────────────────────────────────────────────────────────────────────────

fn constraints_within(child: &GrantConstraints, parent: &GrantConstraints) -> bool {
    // Where the parent declares a bound the child must carry one within it; a parent-silent
    // dimension is unconstrained.
    let le = |c: Option<u64>, p: Option<u64>| match (c, p) {
        (Some(c), Some(p)) => c <= p,
        (None, Some(_)) => false,
        (_, None) => true,
    };
    le(child.time, parent.time) && le(child.count, parent.count)
    // `constraints.budget` is a structured bound record (§5x owns the shape); a parent bound
    // requires a child bound.
        && !(child.budget.is_none() && parent.budget.is_some())
}

fn grants_attenuated(child: &PermissionRecord, parent: &PermissionRecord) -> Option<String> {
    for cg in &child.grants {
        let covered = parent.grants.iter().any(|pg| {
            pg.delegable
                && pg.effect.covers(&cg.effect)
                && scope_covers(&pg.scope, &cg.scope)
                && constraints_within(&cg.constraints, &pg.constraints)
        });
        if !covered {
            return Some(format!(
                "delegated grant {} on {} not covered by a delegable parent grant",
                cg.effect.domain.name(),
                cg.scope
            ));
        }
    }
    None
}

fn budget_within(child: &BudgetRecord, parent: &BudgetRecord) -> Option<String> {
    for (dim, bound) in &child.dimensions {
        match parent.dimensions.get(dim) {
            Some(pb) if !bound.within(pb) => {
                return Some(format!(
                    "budget dimension {dim:?}: child {bound:?} exceeds parent {pb:?}"
                ));
            }
            None => {
                // A parent-silent dimension is unbounded there (§3.1.4 Stage-1 reading —
                // remaining-at-allocation accounting lands at S1.6).
            }
            _ => {}
        }
    }
    None
}

fn process_budget_ref(a: &AgentProcessRecord) -> &Ref {
    match &a.body {
        AgentProcessBody::Native(n) => &n.budget,
        AgentProcessBody::Hosted(h) => &h.budget,
    }
}

fn process_permission_ref(a: &AgentProcessRecord) -> &Ref {
    match &a.body {
        AgentProcessBody::Native(n) => &n.permissions,
        AgentProcessBody::Hosted(h) => &h.permissions,
    }
}

fn check_delegation(doc: &HirDocument, index: &BTreeMap<String, &Node>, errs: &mut Vec<HirError>) {
    for edge in doc.edges.iter().filter(|e| e.kind == EdgeKind::DelegatedTo) {
        let EdgeRecord::DelegatedTo { permission, budget } = &edge.fields else {
            continue;
        };
        let Some(parent_node) = index.get(edge.from.as_str()) else {
            continue;
        };
        let KindRecord::AgentProcess(pa) = &parent_node.semantic else {
            continue;
        };
        // Permission attenuation: every delegated grant ⊆ a delegable parent grant.
        if let Some(n) = index.get(permission.semantic_id.as_str()) {
            if let KindRecord::Permission(child_p) = &n.semantic {
                let parent_perms = index
                    .get(process_permission_ref(pa).semantic_id.as_str())
                    .and_then(|n| match &n.semantic {
                        KindRecord::Permission(p) => Some(p),
                        _ => None,
                    });
                match parent_perms {
                    Some(pp) => {
                        if let Some(d) = grants_attenuated(child_p, pp) {
                            errs.push(HirError::AuthorityWidening { detail: d });
                        }
                    }
                    None => errs.push(HirError::UnresolvedRef {
                        detail: "delegated-to: parent process permission ref unresolved".into(),
                    }),
                }
            }
        }
        // Budget containment: child budget ≤ parent process budget.
        if let (Some(cb), Some(pb_ref)) = (
            index
                .get(budget.semantic_id.as_str())
                .and_then(|n| match &n.semantic {
                    KindRecord::Budget(b) => Some(b),
                    _ => None,
                }),
            index
                .get(process_budget_ref(pa).semantic_id.as_str())
                .and_then(|n| match &n.semantic {
                    KindRecord::Budget(b) => Some(b),
                    _ => None,
                }),
        ) {
            if let Some(d) = budget_within(cb, pb_ref) {
                errs.push(HirError::BudgetExceedsParent { detail: d });
            }
        }
    }

    // `Delegate` steps: the delegated permission's grants must be delegable-covered by the
    // definition's root process permission (the Stage-1 scope reading — the delegating
    // parent inside a definition is its root process; §3.2's AgentProcessSpec refines it).
    let root_perms: Option<&PermissionRecord> =
        doc.node(&doc.root.semantic_id)
            .and_then(|n| match &n.semantic {
                KindRecord::AgentProcess(a) => index
                    .get(process_permission_ref(a).semantic_id.as_str())
                    .and_then(|n| match &n.semantic {
                        KindRecord::Permission(p) => Some(p),
                        _ => None,
                    }),
                _ => None,
            });
    if let Some(rp) = root_perms {
        for node in &doc.nodes {
            let KindRecord::Procedure(p) = &node.semantic else {
                continue;
            };
            let mut delegated = Vec::new();
            collect_delegate_permissions(&p.steps, &mut delegated);
            for r in delegated {
                if let Some(n) = index.get(r.semantic_id.as_str()) {
                    if let KindRecord::Permission(cp) = &n.semantic {
                        if let Some(d) = grants_attenuated(cp, rp) {
                            errs.push(HirError::AuthorityWidening {
                                detail: format!("delegate step in {}: {d}", node.semantic_id()),
                            });
                        }
                    }
                }
            }
        }
    }
}

fn collect_delegate_permissions<'a>(steps: &'a [ProcedureStep], out: &mut Vec<&'a Ref>) {
    for s in steps {
        match s {
            ProcedureStep::Delegate { permission, .. } => out.push(permission),
            ProcedureStep::Branch {
                then_body,
                else_body,
                ..
            } => {
                collect_delegate_permissions(then_body, out);
                collect_delegate_permissions(else_body, out);
            }
            ProcedureStep::Loop { body, .. } => collect_delegate_permissions(body, out),
            _ => {}
        }
    }
}

fn check_budgets(doc: &HirDocument, index: &BTreeMap<String, &Node>, errs: &mut Vec<HirError>) {
    // Budget.parent containment.
    for node in &doc.nodes {
        let KindRecord::Budget(b) = &node.semantic else {
            continue;
        };
        if let Some(parent) = &b.parent {
            if let Some(pb) =
                index
                    .get(parent.semantic_id.as_str())
                    .and_then(|n| match &n.semantic {
                        KindRecord::Budget(b) => Some(b),
                        _ => None,
                    })
            {
                if let Some(d) = budget_within(b, pb) {
                    errs.push(HirError::BudgetExceedsParent { detail: d });
                }
            }
        }
    }
    // Goal.budget ≤ parent.budget dimension-wise.
    for node in &doc.nodes {
        let KindRecord::Goal(g) = &node.semantic else {
            continue;
        };
        let Some(parent_ref) = &g.parent else {
            continue;
        };
        let Some(parent_goal) =
            index
                .get(parent_ref.semantic_id.as_str())
                .and_then(|n| match &n.semantic {
                    KindRecord::Goal(g) => Some(g),
                    _ => None,
                })
        else {
            continue;
        };
        let child_b = index
            .get(g.budget.semantic_id.as_str())
            .and_then(|n| match &n.semantic {
                KindRecord::Budget(b) => Some(b),
                _ => None,
            });
        let parent_b = index
            .get(parent_goal.budget.semantic_id.as_str())
            .and_then(|n| match &n.semantic {
                KindRecord::Budget(b) => Some(b),
                _ => None,
            });
        if let (Some(c), Some(p)) = (child_b, parent_b) {
            if let Some(d) = budget_within(c, p) {
                errs.push(HirError::BudgetExceedsParent {
                    detail: format!("goal {}: {d}", node.semantic_id()),
                });
            }
        }
    }
}

fn check_root(doc: &HirDocument, index: &BTreeMap<String, &Node>, errs: &mut Vec<HirError>) {
    match index.get(doc.root.semantic_id.as_str()) {
        None => errs.push(HirError::UnresolvedRef {
            detail: format!("root {} does not resolve", doc.root.semantic_id),
        }),
        Some(n) if n.kind != EntityKind::AgentProcess => errs.push(HirError::UnresolvedRef {
            detail: format!("root must be AgentProcess, got {}", n.kind.name()),
        }),
        _ => {}
    }
}
