//! `compile(inputs) → bundle` (§3.2.2): the pure pipeline — stage 0 `accept` (calls
//! `validate_assembly`, re-asserts the canonical-form hash against the recorded
//! `version_id`), stage 1 `link`, stage 2 `lower_native`, stage 3 `lower_profile`,
//! stage 4 `lower_target` (per bound target), stage 5 `seal_outputs`. No clock, no
//! env, no I/O — identical inputs produce byte-identical bundles (AC-CP-01).

use hh_assembly::{ClassCatalog, Severity, Subject, ValidationReport};
use hh_hir::{refs::RefVersion, SealedDefinition};
use hh_provenance::ProvenanceRecord;

use crate::errors::CompileError;
use crate::link::{link, LinkedGraph, TargetSpec, VariantView};
use crate::plan::{lower_native, RuntimePlan};
use crate::profile::ProfileView;
use crate::seal::{seal_outputs, CompiledBundle};

/// The compile's declared inputs (§3.2.2 `compile(inputs)`): the sealed definition, the
/// bound profile coordinates, the optional explicit `fallback_profile` (ADR-0124 §5), the
/// target specs, and the recorded intent flag for compiling under expired conditioned
/// rules (ADR-0020 §7 — it is *recorded*: the bundle's `diagnostics` carry the
/// `C-LINK-6` warnings either way).
#[derive(Debug, Clone)]
pub struct CompileInputs {
    /// The sealed, canonical definition.
    pub sealed: SealedDefinition,
    /// The bound profile coordinates (primary first — `extends` ancestors are pulled by
    /// `resolve_chain`).
    pub profile_refs: Vec<String>,
    /// The explicit `fallback_profile` coordinate (ADR-0124 §5 — admissible only with a
    /// dated debt hypothesis).
    pub fallback_profile: Option<String>,
    /// The target specs to bind.
    pub targets: Vec<TargetSpec>,
    /// The recorded intent to compile under expired conditioned rules.
    pub compile_for_expired: bool,
}

/// Stage 0's outcome — the `ValidationReport` `validate_assembly` produced (carried into
/// `lcd_report` — CF-050: the report composes it, never recomputes it).
#[derive(Debug)]
pub struct AcceptOutcome {
    /// The complete report (warnings included — a `pass_with_warnings` compiles).
    pub report: ValidationReport,
}

/// Stage 0 — `accept`: sealed input only, canonical hash re-asserted, then
/// `validate_assembly` (the compiler **calls** it — ADR-0025 D2; it never re-implements
/// `validate`).
pub fn accept(
    sealed: &SealedDefinition,
    catalog: &dyn ClassCatalog,
    kernel: &ProvenanceRecord,
) -> Result<AcceptOutcome, CompileError> {
    // Sealed markers: every node and edge carries `version.sealed` (§3.1.6 — a
    // `SealedDefinition` claims it; `accept` re-asserts it).
    for n in &sealed.document.nodes {
        if !n.version.sealed {
            return Err(CompileError::NotSealed {
                detail: format!("node {} carries no sealed marker", n.semantic_id()),
            });
        }
    }
    for e in &sealed.document.edges {
        if !e.version.sealed {
            return Err(CompileError::NotSealed {
                detail: format!("edge {}→{} carries no sealed marker", e.from, e.to),
            });
        }
    }
    // Canonical-form hash re-assertion: the recorded `definition_ref` must equal the
    // recomputed document identity (§3.2.2 — "re-asserts that the canonical-form hash
    // matches the recorded definition identity").
    let recomputed = hh_hir::document_identity(&sealed.document).ok_or_else(|| {
        CompileError::NonCanonicalInput {
            detail: "document root is not a node in the document".to_string(),
        }
    })?;
    if recomputed != sealed.definition_ref {
        return Err(CompileError::NonCanonicalInput {
            detail: format!(
                "recorded definition_ref {}≠ recomputed {}",
                sealed.definition_ref.version_id, recomputed.version_id
            ),
        });
    }
    // `validate_assembly` over the sealed subject (complete report, never fail-fast).
    let report = hh_assembly::validate_assembly(Subject::Sealed(sealed), catalog, None, kernel);
    if report
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return Err(CompileError::InvalidDefinition {
            diagnostics: report.diagnostics.clone(),
        });
    }
    Ok(AcceptOutcome { report })
}

/// The canonical-bytes seam (AC-CP-11's out-of-process boundary): parse the sealed
/// document, re-assert sealed markers + pinned refs, rebuild the `SealedDefinition`,
/// then `accept`. Any parse failure is `NonCanonicalInput`; a document that parses but
/// isn't sealed is `NotSealed`.
pub fn accept_bytes(
    bytes: &[u8],
    catalog: &dyn ClassCatalog,
    kernel: &ProvenanceRecord,
) -> Result<(SealedDefinition, AcceptOutcome), CompileError> {
    let doc = hh_hir::parse_document(bytes).map_err(|e| CompileError::NonCanonicalInput {
        detail: format!("{e}"),
    })?;
    // Sealed markers — a document that parses but isn't marked sealed was never through
    // `seal` (a *surviving selector* inside an otherwise-sealed document is link's
    // `C-LINK-1`, not stage 0's).
    for n in &doc.nodes {
        if !n.version.sealed {
            return Err(CompileError::NotSealed {
                detail: format!("node {} carries no sealed marker", n.semantic_id()),
            });
        }
    }
    let definition_ref =
        hh_hir::document_identity(&doc).ok_or_else(|| CompileError::NonCanonicalInput {
            detail: "document root is not a node in the document".to_string(),
        })?;
    let sealed = SealedDefinition {
        closed_world_tools: hh_hir::closed_world_tools(&doc),
        document: doc,
        definition_ref,
    };
    let outcome = accept(&sealed, catalog, kernel)?;
    Ok((sealed, outcome))
}

/// `compile(inputs) → CompiledBundle` — stages 0 → 1 → 2 → 3 → 4 → 5, in that order
/// (§3.2.2). Stage 3 `lower_profile` produces the `ModelSurface`; stage 4
/// `lower_target` runs once per bound target producing `(artefact, loss_report)`;
/// stage 5 `seal_outputs` computes the equivalence evidence and mints the ids.
pub fn compile(
    inputs: &CompileInputs,
    profiles: &dyn ProfileView,
    variants: &dyn VariantView,
    catalog: &dyn ClassCatalog,
    kernel: &ProvenanceRecord,
) -> Result<CompiledBundle, CompileError> {
    let accepted = accept(&inputs.sealed, catalog, kernel)?;
    let linked: LinkedGraph = link(
        &inputs.sealed,
        &inputs.profile_refs,
        inputs.fallback_profile.as_deref(),
        &inputs.targets,
        profiles,
        variants,
        kernel,
        inputs.compile_for_expired,
    )?;
    let plan: RuntimePlan = lower_native(&linked)?;
    // Stage 3 — `lower_profile`.
    let (surface, lower_diags) = crate::lower::lower_profile(&linked, &plan, kernel)?;
    // Stage 4 — `lower_target` per bound target.
    let mut artefacts = std::collections::BTreeMap::new();
    let mut losses = Vec::new();
    for spec in &linked.targets {
        let (artefact, loss) = crate::target::lower_target(&linked, &plan, &surface, spec)?;
        artefacts.insert(spec.target_id.clone(), artefact);
        losses.push(loss);
    }
    // Stage 5 — `seal_outputs`.
    seal_outputs(
        &linked,
        &plan,
        &accepted.report,
        surface,
        lower_diags,
        artefacts,
        losses,
    )
}

/// Check whether a `Ref` anywhere in the document still carries a selector — exposed
/// for `accept_bytes`' caller-facing checks (link's `link_precheck` is the refusing
/// check; this is a convenience for the binary's error text).
pub fn has_unpinned_refs(sealed: &SealedDefinition) -> bool {
    sealed.document.nodes.iter().any(|n| {
        let mut found = false;
        collect_selector_refs(&hh_hir::wire::node_to_json(n), &mut found);
        found
    })
}

fn collect_selector_refs(j: &hh_wire::json::Json, found: &mut bool) {
    use hh_wire::json::Json;
    match j {
        Json::Obj(m) => {
            for (k, v) in m {
                if k == "version_selector" {
                    *found = true;
                }
                collect_selector_refs(v, found);
            }
        }
        Json::Arr(items) => items.iter().for_each(|i| collect_selector_refs(i, found)),
        _ => {}
    }
}

/// Helper: does a `Ref` carry a pinned version? (Used by plan lowering paths.)
pub fn ref_is_pinned(r: &hh_hir::Ref) -> bool {
    matches!(r.version, RefVersion::Pinned(_))
}
