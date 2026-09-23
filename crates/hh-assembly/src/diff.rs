//! `diff(sealed_a, sealed_b, provenance, derivation) → HirDiff` (§3.3.4) — **not a
//! separate object**: assembly edits are `HirDiff` ops over sealed definitions —
//! `Rebind` for slot-variant changes, `ReplaceField` for `params`/`values`/`enabled`
//! (the root carries the point — ADR-0240), `AddNode`/`RemoveNode` for entities;
//! constraints live in the assembly section and diff as leaf-granular `ReplaceField`s
//! at `assembly.constraints[i]…` (HIR/1 has no constraint node kind — recorded in
//! ADR-0240). `classification.authority_delta` is `hh-hir`'s.
//!
//! The §3.1.7 gates run inside `hh_hir::diff` — a refusal is mirrored into the closed
//! assembly taxonomy as `C-KERN-*` ([`kern_code`]), never a bare `HirError` leak
//! (`C-INT-1`: an uncoded rejection is a defect).

use hh_hir::diff::{DiffDerivation, HirDiff};
use hh_hir::document::SealedDefinition;
use hh_provenance::ProvenanceRecord;

use crate::diagnostics::{detail_text, kern_code, AssemblyDiagnostic, Severity, Stage};

/// `diff(sealed_a, sealed_b, provenance, derivation) → HirDiff`. Failures are the
/// `hh-hir` gates (`AuthorityWidening`, `ConditionedRuleIncomplete`, the schema
/// violations) — mirrored as `C-KERN-*` diagnostics.
pub fn diff(
    a: &SealedDefinition,
    b: &SealedDefinition,
    provenance: ProvenanceRecord,
    derivation: DiffDerivation,
) -> Result<HirDiff, Vec<AssemblyDiagnostic>> {
    let kernel = ProvenanceRecord::kernel("kernel:assembly:diff", 0);
    hh_hir::diff::diff(&a.document, &b.document, provenance, derivation).map_err(|es| {
        es.iter()
            .map(|e| AssemblyDiagnostic {
                code: kern_code(e),
                class: None,
                severity: Severity::Error,
                path: String::new(),
                source_layer: None,
                subject: "diff".into(),
                stage: Stage::Validate(5),
                detail: detail_text(e.to_string(), &kernel),
                remedy: "address the gated edit (see the C-KERN-* name's ADR-0148 row)".into(),
                owner_adr: "ADR-0148".into(),
            })
            .collect()
    })
}
