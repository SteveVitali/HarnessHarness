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

/// `ModelSurface{layout, tools, interaction_mode, params, transcript_renderer}`
/// (§3.2.8; produced by stage 3 `lower_profile`). `tools` carries
/// `ToolSurface{surface_name, hir_node_id, dialect, schema, description,
/// error_format, result_render, arg_map, equivalence}` — the `SurfaceBinding`
/// member holds the arg map and identity (ADR-0090's one atomic record).
#[derive(Debug, Clone, PartialEq)]
pub struct ModelSurface {
    /// The profile coordinate this surface was lowered under.
    pub profile: String,
    /// The compiled layout sections (profile-declared order).
    pub layout: Vec<crate::lower::CompiledSection>,
    /// The compiled tool surfaces.
    pub tools: Vec<crate::lower::CompiledToolSurface>,
    /// The interaction mode (`native_fc` at C0).
    pub interaction_mode: String,
    /// `{sampling, caching, compaction_reminder}` — the verbatim rule params.
    pub params: Json,
    /// The transcript renderer spec (`{stale_signature, …}` — OQ-067).
    pub transcript_renderer: Json,
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
    /// `diagnostics.profile_test_report_ref` — the bound leaf profile's
    /// `ProfileTestReport` ref, present whenever the link gate admitted a
    /// tested profile (AC-R-2.3.3-13; `None` only under the kernel null
    /// profile, whose validity is the compiler's own suite — AC-R-2.3.3-14).
    pub profile_test_report_ref: Option<String>,
    /// `profile_binding.fallback_used` — `true` when the bound chain came in
    /// through the explicit `fallback_profile` escape (AC-R-2.3.3-2 records
    /// `fallback_used`; ADR-0124 §5).
    pub fallback_used: bool,
}

/// `seal_outputs(linked, plan, validation, surface, lower_diags, artefacts, losses)
/// → CompiledBundle` — stage 5. Computes the trace map, the per-surface
/// `EquivalenceEvidence` (E1–E3/E7 static; **E4 executable** over the profile's
/// declared `tests.e4_suites[]`; E5/E6 over the compiled specs), stamps each
/// surface's `equivalence`, composes the `lcd_report` (the loss reports ride in —
/// CF-050 composition), and mints the two `idp/1` addresses.
#[allow(clippy::too_many_arguments)] // the arity is the §3.2 pipeline's stage-5 input set.
pub fn seal_outputs(
    linked: &LinkedGraph,
    plan: &RuntimePlan,
    validation: &ValidationReport,
    mut surface: ModelSurface,
    lower_diags: Vec<AssemblyDiagnostic>,
    artefacts: BTreeMap<String, Json>,
    losses: Vec<LoweringLossReport>,
) -> Result<CompiledBundle, CompileError> {
    // The profile's declared E4 suites (`tests.e4_suites[]`, merged across the chain).
    let mut suites: Vec<crate::e4::E4SuiteSpec> = Vec::new();
    for p in &linked.profile.chain {
        suites.extend(crate::e4::suites_from_profile(
            &p.tests,
            &format!("profile {}", crate::profile::profile_coordinate(p)),
        )?);
    }

    // Per-surface equivalence evidence (§3.2.6) — over the *lowered* surface.
    let mut evidence: Vec<EquivalenceEvidence> = Vec::new();
    for tool in &mut surface.tools {
        let binding = &tool.binding;
        let node = linked
            .sealed
            .document
            .node(&binding.hir_node_id)
            .expect("surface tools are document nodes");
        let suite = suites.iter().find(|s| {
            s.capability == binding.hir_node_id
                || s.capability
                    == binding
                        .hir_node_id
                        .rsplit([':', '/'])
                        .next()
                        .unwrap_or_default()
        });
        let validators_bound = plan.validators.iter().any(|v| {
            v.inputs
                .iter()
                .any(|i| i.semantic_id == binding.hir_node_id)
        });
        let e = equiv::check_equivalence(
            binding,
            node,
            suite,
            tool.error_format.as_ref(),
            tool.result_render.as_ref(),
            validators_bound,
        )?;
        // A `fail` on any obligation is a compile refusal — the surface does not bind
        // (§3.2.5: evidence of failure is a fail verdict; a failing check is an error,
        // not a warning; §3.2.6 rule i: C0 admits a surface only with E1–E3 and E7
        // `pass`, E4 `pass` for the named closed-world primitives).
        for (name, v) in [
            ("E1", &e.e1_effect_equality),
            ("E2", &e.e2_authority),
            ("E3", &e.e3_precondition_domain),
            ("E4", &e.e4_differential),
            ("E5", &e.e5_error_surjectivity),
            ("E6", &e.e6_result_observation),
            ("E7", &e.e7_accounting_identity),
        ] {
            if let equiv::EvidenceVerdict::Fail { reason } = v {
                return Err(CompileError::UnexpressibleSurface {
                    entity: binding.hir_node_id.clone(),
                    profile: surface.profile.clone(),
                    reason: format!("{name} failed on {}: {reason}", binding.surface_name),
                });
            }
        }
        tool.equivalence = Some(e.clone());
        tool.binding.evidence_ref = Some(tool.binding.surface_id.clone());
        evidence.push(e);
    }

    let trace_map = trace::build_trace_map(plan);
    let report = lcd::lcd_report(validation, linked, &losses);
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
        model_surface: ModelSurfaceState::Lowered(surface),
        target_artefacts: artefacts,
        trace_map,
        lcd_report: report,
        loss_reports: losses,
        opacity_report: opacity,
        equivalence_evidence: evidence,
        diagnostics: {
            let mut d = validation.diagnostics.clone();
            d.extend(linked.diagnostics.clone());
            d.extend(lower_diags);
            d
        },
        profile_chain: linked
            .profile
            .chain
            .iter()
            .map(crate::profile::profile_coordinate)
            .collect(),
        profile_test_report_ref: linked.profile.test_report_ref.clone(),
        fallback_used: linked.profile.is_fallback,
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
