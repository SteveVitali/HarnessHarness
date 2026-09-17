//! `hh-compile` — the out-of-process compile seam (AC-CP-11 / T-LCD-12): canonical
//! `CompileInputs` bytes on stdin → canonical `CompiledBundle` bytes on stdout; a typed
//! `CompileError` envelope on stdout + non-zero exit on refusal. No clocks, no env reads
//! — the run is byte-for-byte reproducible with the in-process `compile` (AC-CP-01).

use std::io::{Read, Write};

use hh_compiler::profile::ProfileView;
use hh_compiler::schema::{bundle_to_json, compile_inputs_from_json};
use hh_compiler::{CompileError, CompileInputs};
use hh_wire::json::Json;

/// The in-binary `ProfileView` over the carried profile records.
struct CarriedProfiles(std::collections::BTreeMap<String, hh_compiler::profile::ModelProfile>);

impl ProfileView for CarriedProfiles {
    fn profile(&self, coordinate: &str) -> Option<hh_compiler::profile::ModelProfile> {
        self.0.get(coordinate).cloned().or_else(|| {
            self.0
                .values()
                .find(|p| p.content_hash == coordinate)
                .cloned()
        })
    }
}

/// The in-binary `VariantView` over the carried variant records.
struct CarriedVariants(std::collections::BTreeMap<String, hh_registry::records::VariantRecord>);

impl hh_compiler::link::VariantView for CarriedVariants {
    fn variant(&self, version_id: &str) -> Option<hh_registry::records::VariantRecord> {
        self.0.get(version_id).cloned()
    }
}

/// The stage-1 class catalog — the fixed `Stage1Catalog` (the binary is catalog-pinned;
/// the catalog's contents are data, not a resolution input).
struct PinnedCatalog(hh_assembly::Stage1Catalog);

impl hh_assembly::ClassCatalog for PinnedCatalog {
    fn class(&self, class_id: &str) -> Option<hh_registry::records::ClassRecord> {
        self.0.class(class_id)
    }

    fn class_ids(&self) -> Vec<String> {
        self.0.class_ids()
    }

    fn variant(&self, version_id: &str) -> Option<hh_registry::records::VariantRecord> {
        self.0.variant(version_id)
    }
}

fn error_json(e: &CompileError) -> Json {
    let (variant, detail, diagnostics) = match e {
        CompileError::NotSealed { detail } => ("NotSealed", detail.clone(), vec![]),
        CompileError::NonCanonicalInput { detail } => ("NonCanonicalInput", detail.clone(), vec![]),
        CompileError::InvalidDefinition { diagnostics } => (
            "InvalidDefinition",
            format!("{} diagnostic(s)", diagnostics.len()),
            diagnostics.clone(),
        ),
        CompileError::LinkError {
            kind,
            detail,
            diagnostics,
        } => (
            match kind {
                hh_compiler::LinkErrorKind::UnboundSlot => "LinkError{unbound_slot}",
                hh_compiler::LinkErrorKind::VersionConflict => "LinkError{version_conflict}",
                hh_compiler::LinkErrorKind::MissingDebtRecord => "LinkError{missing_debt_record}",
                hh_compiler::LinkErrorKind::UnknownTarget => "LinkError{unknown_target}",
            },
            detail.clone(),
            diagnostics.clone(),
        ),
        CompileError::NoProfile { detail } => ("NoProfile", detail.clone(), vec![]),
        CompileError::AmbiguousSelector { detail } => ("AmbiguousSelector", detail.clone(), vec![]),
        CompileError::PlanError { node, detail } => {
            ("PlanError", format!("{node}: {detail}"), vec![])
        }
        CompileError::UnexpressibleSurface {
            entity,
            profile,
            reason,
        } => (
            "UnexpressibleSurface",
            format!("{entity} / {profile}: {reason}"),
            vec![],
        ),
        CompileError::UncheckableSurface { surface, reason } => {
            ("UncheckableSurface", format!("{surface}: {reason}"), vec![])
        }
        CompileError::DialectNarrowingUndeclared { detail } => {
            ("DialectNarrowingUndeclared", detail.clone(), vec![])
        }
        CompileError::TargetError { detail } => ("TargetError", detail.clone(), vec![]),
        CompileError::ProtocolVersionMismatch { detail } => {
            ("ProtocolVersionMismatch", detail.clone(), vec![])
        }
        CompileError::SealError { detail } => ("SealError", detail.clone(), vec![]),
        CompileError::InvalidModelProfile { detail } => {
            ("InvalidModelProfile", detail.clone(), vec![])
        }
    };
    Json::obj([
        (
            "diagnostics",
            Json::Arr(
                diagnostics
                    .iter()
                    .map(hh_assembly::diagnostic_json)
                    .collect(),
            ),
        ),
        ("error", Json::str(variant)),
        ("detail", Json::str(detail)),
    ])
}

fn run() -> Result<Json, CompileError> {
    let mut input = Vec::new();
    std::io::stdin()
        .read_to_end(&mut input)
        .map_err(|e| CompileError::NonCanonicalInput {
            detail: format!("stdin: {e}"),
        })?;
    let json = hh_wire::canonical::parse_canonical(&input).map_err(|e| {
        CompileError::NonCanonicalInput {
            detail: format!("inputs: {e}"),
        }
    })?;
    let inputs = compile_inputs_from_json(&json)?;

    // Rebuild the sealed definition from the carried document — `accept` re-asserts the
    // markers, the canonical hash, and `validate_assembly` (stage 0 owns admission).
    let doc = hh_hir::wire::document_from_json(&inputs.document).map_err(|e| {
        CompileError::NonCanonicalInput {
            detail: format!("document: {e}"),
        }
    })?;
    let sealed = hh_hir::SealedDefinition {
        closed_world_tools: hh_hir::closed_world_tools(&doc),
        definition_ref: hh_hir::document_identity(&doc).ok_or_else(|| {
            CompileError::NonCanonicalInput {
                detail: "document root is not a node".to_string(),
            }
        })?,
        document: doc,
    };

    let profiles = CarriedProfiles(
        inputs
            .profiles
            .iter()
            .map(|p| (hh_compiler::profile::profile_coordinate(p), p.clone()))
            .collect(),
    );
    let variants = CarriedVariants(inputs.variants.iter().cloned().collect());
    let catalog = PinnedCatalog(hh_assembly::Stage1Catalog::stage1());
    let kernel = kernel_provenance();

    let bundle = hh_compiler::compile(
        &CompileInputs {
            sealed,
            profile_refs: inputs.profile_refs,
            fallback_profile: inputs.fallback_profile,
            targets: inputs.targets,
            compile_for_expired: inputs.compile_for_expired,
        },
        &profiles,
        &variants,
        &catalog,
        &kernel,
    )?;
    Ok(bundle_to_json(&bundle))
}

/// The binary's kernel provenance — a fixed, kernel-origin record (no env read; the
/// record is a minting context, not a secret).
fn kernel_provenance() -> hh_provenance::ProvenanceRecord {
    // `created_at` is a ledger seq, not a wall clock — a fixed `0` keeps the binary
    // byte-for-byte reproducible (the record is a minting context, never a secret).
    hh_provenance::ProvenanceRecord::kernel("hh-compile", 0)
}

fn main() {
    match run() {
        Ok(bundle) => {
            let mut out = std::io::stdout().lock();
            out.write_all(bundle.to_canonical_string().as_bytes())
                .expect("stdout");
            out.write_all(b"\n").expect("stdout");
        }
        Err(e) => {
            let mut out = std::io::stdout().lock();
            out.write_all(error_json(&e).to_canonical_string().as_bytes())
                .expect("stdout");
            out.write_all(b"\n").expect("stdout");
            std::process::exit(2);
        }
    }
}
