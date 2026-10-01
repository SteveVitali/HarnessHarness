//! `drift` (§6.1 §2.4 T-3; ADR-0149) — snapshot drift detection, `freeze`
//! (the only default) and explicit `adopt`:
//!
//! `drift(source, snap_old, snap_new) = project(diff(resolve(source,
//! snap_old), resolve(source, snap_new)))` — `Rebind`/`ReplaceLeaf` ops on
//! pinned references only — reported as `C-REF-6 SnapshotDrift{diff_ref,
//! sameness}` at `info` (L1) / `warning` (L2) / `error` (L3). `freeze` keeps
//! the old pins (an arm whose `Design` has opened is never re-resolved);
//! `adopt` for *new* arms is an explicit `apply` producing a `HirDiff` with
//! `derived-from{hypothesis: "snapshot adoption"}` under ADR-0017's widening
//! rule — `origin = evolution` may never adopt L3 drift.

use hh_assembly::diagnostics::{detail_text, AssemblyDiagnostic, Code, Severity, Stage};
use hh_hir::diff::DiffOp;
use hh_identity::sameness::SamenessLevel;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::assembly::diff_view::{flatten, project, sameness_name, AssemblyDiff};

/// The `adopt` hypothesis spelling (`derived-from.hypothesis`).
pub const ADOPT_HYPOTHESIS: &str = "snapshot adoption";

/// The drift check's output.
#[derive(Debug)]
pub struct DriftReport {
    /// The projected pin-only diff (`None` when either resolve failed — the
    /// resolve diagnostics then carry the failure).
    pub diff: Option<AssemblyDiff>,
    /// `L0..L4` across the two resolutions, when both succeeded.
    pub sameness: Option<SamenessLevel>,
    /// The `C-REF-6` diagnostic (severity by sameness) plus any resolve
    /// diagnostics from either side.
    pub diagnostics: Vec<AssemblyDiagnostic>,
}

/// Keep only the pinned-reference ops (T-3: `Rebind`/`ReplaceLeaf` on pinned
/// references only — drift is a *pin* movement, never an authored change).
pub fn pin_ops_only(d: &AssemblyDiff) -> AssemblyDiff {
    let ops = d
        .ops
        .iter()
        .filter(|o| matches!(o.op, DiffOp::Rebind { .. } | DiffOp::ReplaceLeaf { .. }))
        .cloned()
        .collect::<Vec<_>>();
    AssemblyDiff {
        ops,
        classification: d.classification.clone(),
        sameness: d.sameness,
    }
}

/// `C-REF-6` — severity by sameness (L1 → info, L2 → warning, L3 → error; L0
/// produces no diagnostic — no drift).
pub fn drift_diagnostic(
    sameness: SamenessLevel,
    diff_ref: &str,
    snap_old: &str,
    snap_new: &str,
    kernel: &ProvenanceRecord,
) -> Option<AssemblyDiagnostic> {
    let severity = match sameness {
        SamenessLevel::L0 => return None,
        SamenessLevel::L1 => Severity::Info,
        SamenessLevel::L2 => Severity::Warning,
        SamenessLevel::L3 | SamenessLevel::L4 => Severity::Error,
    };
    Some(AssemblyDiagnostic {
        code: Code::RefSnapshotDrift,
        class: None,
        severity,
        path: "/resolved".into(),
        source_layer: None,
        subject: snap_old.to_string(),
        stage: Stage::Resolve,
        detail: detail_text(
            format!(
                "snapshot drift {snap_old} → {snap_new}: sameness {} (diff_ref {diff_ref}); \
                 `freeze` is the default — an opened Design's arms keep their pins",
                sameness_name(sameness)
            ),
            kernel,
        ),
        remedy: "freeze keeps the arm's pins; adopt is an explicit apply for new arms".into(),
        owner_adr: "ADR-0149".into(),
    })
}

/// `adopt` admissibility (T-3): `origin = evolution` may never adopt L3
/// drift. Returns `Err(detail)` when refused.
pub fn adopt_admissible(sameness: SamenessLevel, origin_is_evolution: bool) -> Result<(), String> {
    if origin_is_evolution && matches!(sameness, SamenessLevel::L3) {
        return Err(
            "`origin = evolution` may never adopt L3 drift (ADR-0149 T-3; ADR-0017 widening rule)"
                .into(),
        );
    }
    Ok(())
}

/// The drift report's canonical JSON (the `plan.drift` / `lab.assembly.drift`
/// wire form).
pub fn drift_json(r: &DriftReport) -> Json {
    Json::obj([
        (
            "diff",
            r.diff
                .as_ref()
                .map(crate::assembly::diff_view::diff_json)
                .unwrap_or(Json::Null),
        ),
        (
            "sameness",
            r.sameness
                .map(sameness_name)
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
        (
            "diagnostics",
            Json::Arr(
                r.diagnostics
                    .iter()
                    .map(hh_assembly::diagnostics::diagnostic_json)
                    .collect(),
            ),
        ),
        ("policy", Json::str("freeze")),
    ])
}

/// Convenience: build the projected drift diff from two sealed documents'
/// ops (the caller runs the two resolves; this is the pure projection step).
pub fn project_drift(
    ops: Vec<DiffOp>,
    classification: hh_hir::diff::DiffClassification,
    sameness: Option<SamenessLevel>,
) -> AssemblyDiff {
    pin_ops_only(&project(ops, classification, sameness))
}

/// `flatten` re-export for the conformance suite (T-1).
pub fn flatten_drift(d: &AssemblyDiff) -> Vec<DiffOp> {
    flatten(d)
}
