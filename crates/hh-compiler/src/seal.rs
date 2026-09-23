//! Stage 5 — `seal` (§3.2.2/§3.2.8): `derivation_key` over canonical *inputs*, `bundle_id`
//! over canonical *outputs* — both `idp/1` content addresses (CC1 — no second hashing or
//! canonicalization scheme). `SealError{non_canonical}` when the bundle fails to
//! re-encode to the hashed bytes. No run-time value is referenced.

use std::collections::BTreeMap;

use hh_assembly::{AssemblyDiagnostic, OpacitySummary, ValidationReport};
use hh_wire::json::Json;

use crate::equiv::{self, EquivalenceEvidence};
use crate::errors::CompileError;
use crate::lcd::{self, LcdReport, LoweringLossReport};
use crate::link::LinkedGraph;
use crate::plan::RuntimePlan;
use crate::trace::{self, TraceMap};

/// The compiler version recorded in the derivation key — a *build-time constant*
/// (never an env/clock read; AC-CP-01).
pub const COMPILER_VERSION: &str = concat!("hh-compiler/", env!("CARGO_PKG_VERSION"));

/// `ModelSurface` — the stage-3 artifact type. Declared now so `CompiledBundle`'s
/// `model_surface` member has its stable schema (CC8); `lower_profile` (which produces
/// `Lowered(_)`) lands at S3.2.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelSurfaceState {
    /// Stage 3 lands at S3.2 — present-and-explicit, never absent (the bundle's
    /// `model_surface` is `{deferred: "stage_3"}` on the wire).
    Deferred,
    /// The lowered model surface (Stage 3).
    Lowered(ModelSurface),
}

/// `ModelSurface` — the profile-lowered surface record (§3.2.5; produced by stage 3).
#[derive(Debug, Clone, PartialEq)]
pub struct ModelSurface {
    /// The profile coordinate this surface was lowered under.
    pub profile: String,
    /// The surfaces, by capability semantic id.
    pub surfaces: BTreeMap<String, equiv::SurfaceBinding>,
    /// `dialects: map<ModelRole, dialect>` (§3.2.5 — `schema_dialect` default
    /// `json-schema-2020-12` on every role, ADR-0212 OQ-219).
    pub dialects: BTreeMap<String, String>,
}

/// `CompiledBundle` (§3.2.8): `{bundle_id, derivation_key, runtime_plan, model_surface,
/// target_artefacts, trace_map, lcd_report, loss_reports, opacity_report,
/// equivalence_evidence, diagnostics}`.
#[derive(Debug, Clone)]
pub struct CompiledBundle {
    /// `idp/1` over the canonical outputs.
    pub bundle_id: String,
    /// `idp/1` over the canonical inputs.
    pub derivation_key: String,
    /// The lowered plan.
    pub runtime_plan: RuntimePlan,
    /// The model surface (`{deferred: stage_3}` at C0).
    pub model_surface: ModelSurfaceState,
    /// The target artefacts — `[]` at Stage 1 (stage 4 lands at S3.2).
    pub target_artefacts: BTreeMap<String, Json>,
    /// The total trace map.
    pub trace_map: TraceMap,
    /// The static LCD report.
    pub lcd_report: LcdReport,
    /// Per-target lowering-loss reports (`[]` at Stage 1).
    pub loss_reports: Vec<LoweringLossReport>,
    /// The opacity report (composed from stage-0 derived results).
    pub opacity_report: Option<OpacitySummary>,
    /// `E1..E7` evidence per compiled surface.
    pub equivalence_evidence: Vec<EquivalenceEvidence>,
    /// The diagnostics the pipeline produced (link warnings + validation carry-through).
    pub diagnostics: Vec<AssemblyDiagnostic>,
    /// The bound profile chain coordinates (derivation input, recorded for the reader).
    pub profile_chain: Vec<String>,
}

/// `seal_outputs(linked, plan, validation) → CompiledBundle` — stage 5. Computes
/// the trace map, the per-surface `EquivalenceEvidence` (E1–E3/E7 static; E4 `n/a`), the
/// `lcd_report`, and the two `idp/1` addresses.
pub fn seal_outputs(
    linked: &LinkedGraph,
    plan: &RuntimePlan,
    validation: &ValidationReport,
) -> Result<CompiledBundle, CompileError> {
    // Per-surface equivalence evidence (the static half; §3.2.5).
    let mut evidence: Vec<EquivalenceEvidence> = Vec::new();
    for binding in &plan.tools {
        if let Some(surface) = &binding.surface {
            let node = linked
                .sealed
                .document
                .node(&binding.capability.semantic_id)
                .expect("plan tools are document nodes");
            let e = equiv::check_equivalence(surface, node, None)?;
            // An E1/E2/E3/E7 `fail` is a compile refusal — the surface does not bind
            // (§3.2.5: evidence of failure is a fail verdict; a failing static check is
            // an error, not a warning).
            for (name, v) in [
                ("E1", &e.e1_effect_equality),
                ("E2", &e.e2_authority),
                ("E3", &e.e3_precondition_domain),
                ("E7", &e.e7_accounting_identity),
            ] {
                if let equiv::EvidenceVerdict::Fail { reason } = v {
                    return Err(CompileError::UnexpressibleSurface {
                        entity: binding.capability.semantic_id.clone(),
                        profile: linked
                            .profile
                            .chain
                            .last()
                            .map(crate::profile::profile_coordinate)
                            .unwrap_or_default(),
                        reason: format!("{name} failed on {}: {reason}", surface.surface_name),
                    });
                }
            }
            evidence.push(e);
        }
    }

    let trace_map = trace::build_trace_map(plan);
    let report = lcd::lcd_report(validation, linked);
    let opacity = validation.derived.opacity.clone();

    // derivation_key = idp/1 over canonical inputs {definition version_id, bound variant
    // version_ids, profile content hashes, target spec versions, compiler version}.
    let derivation_inputs = Json::obj([
        ("compiler_version", Json::str(COMPILER_VERSION)),
        (
            "definition_version_id",
            Json::str(linked.sealed.definition_ref.version_id.clone()),
        ),
        (
            "profile_hashes",
            Json::Arr(
                linked
                    .profile
                    .chain
                    .iter()
                    .map(|p| Json::str(p.content_hash.clone()))
                    .collect(),
            ),
        ),
        (
            "target_specs",
            Json::Arr(
                linked
                    .targets
                    .iter()
                    .map(|t| {
                        Json::obj([
                            ("content_hash", Json::str(t.content_hash.clone())),
                            ("spec_version", Json::str(t.spec_version.clone())),
                            ("target_id", Json::str(t.target_id.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "variant_version_ids",
            Json::Arr(
                linked
                    .variant_pins
                    .values()
                    .map(|(_, _, v)| Json::str(v.clone()))
                    .collect(),
            ),
        ),
    ]);
    let derivation_key = hh_identity::idp_id(
        "compiler.derivation",
        derivation_inputs.to_canonical_string().as_bytes(),
    );

    let mut bundle = CompiledBundle {
        bundle_id: String::new(),
        derivation_key,
        runtime_plan: plan.clone(),
        model_surface: ModelSurfaceState::Deferred,
        target_artefacts: BTreeMap::new(),
        trace_map,
        lcd_report: report,
        loss_reports: Vec::new(),
        opacity_report: opacity,
        equivalence_evidence: evidence,
        diagnostics: {
            let mut d = validation.diagnostics.clone();
            d.extend(linked.diagnostics.clone());
            d
        },
        profile_chain: linked
            .profile
            .chain
            .iter()
            .map(crate::profile::profile_coordinate)
            .collect(),
    };

    // bundle_id = idp/1 over canonical outputs (the bundle minus `bundle_id`).
    bundle.bundle_id = bundle_identity(&bundle);

    // `SealError{non_canonical}` — the bundle must re-encode to the hashed bytes:
    // encode → parse → recompute identity must reproduce `bundle_id`.
    let encoded = crate::schema::bundle_to_json(&bundle).to_canonical_string();
    let parsed = hh_wire::canonical::parse_canonical(encoded.as_bytes()).map_err(|e| {
        CompileError::SealError {
            detail: format!("bundle does not re-parse: {e}"),
        }
    })?;
    let round_trip =
        crate::schema::bundle_from_json(&parsed).map_err(|e| CompileError::SealError {
            detail: format!("bundle does not decode: {e}"),
        })?;
    if bundle_identity(&round_trip) != bundle.bundle_id {
        return Err(CompileError::SealError {
            detail: "re-encoded bundle hashes differently — non-canonical output".to_string(),
        });
    }
    Ok(bundle)
}

/// `bundle_id` — `idp/1` over the canonical bundle minus `bundle_id` (§3.2.8:
/// "hash over canonical outputs").
pub fn bundle_identity(bundle: &CompiledBundle) -> String {
    let mut b = crate::schema::bundle_to_json(bundle);
    if let Json::Obj(m) = &mut b {
        m.remove("bundle_id");
    }
    hh_identity::idp_id("compiler.bundle", b.to_canonical_string().as_bytes())
}
