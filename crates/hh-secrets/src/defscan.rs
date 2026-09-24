//! The definition-side secret gates (§5g.3 §2; AC-R-2.8.3-8): a definition or
//! `HirDiff` carrying a literal credential is refused `SecretValueInDefinition`
//! — at `seal` and at every evolution-context diff check.
//!
//! The scan runs the [`DetectorSet`] over **two** surfaces:
//! 1. the canonical bytes (every structural member — surfaces, refs, ext,
//!    schemas, names — the canonical form carries them verbatim);
//! 2. every `Text` leaf's in-memory `content` (the canonical form carries
//!    `content_hash` only — the leaf bodies are enumerated via
//!    [`hh_hir::document::HirDocument::text_leaves`]).
//!
//! A `placeholder_passthrough`/`$secret:` marker is the *legal* reference form —
//! it is never an error. `known_value`, `pattern` and `canary` hits are.

use hh_hir::diff::HirDiff;
use hh_hir::document::{HirDocument, SealedDefinition};
use hh_hir::errors::HirError;

use crate::redact::{detect, DetectorKind, DetectorSet};

/// Scan a document's canonical bytes **and** every in-memory `Text` body —
/// returns the non-passthrough hits (each is a literal credential where none
/// may appear).
pub fn scan_document(doc: &HirDocument, detectors: &DetectorSet) -> Vec<crate::redact::Hit> {
    let mut hits: Vec<crate::redact::Hit> = detect(
        std::str::from_utf8(&doc.canonical_bytes()).unwrap_or_default(),
        detectors,
    )
    .into_iter()
    .filter(|h| h.detector != DetectorKind::PlaceholderPassthrough)
    .collect();
    for leaf in doc.text_leaves() {
        if let Some(content) = &leaf.content {
            hits.extend(
                detect(content, detectors)
                    .into_iter()
                    .filter(|h| h.detector != DetectorKind::PlaceholderPassthrough),
            );
        }
    }
    hits
}

/// `seal_checked(doc, sealed_at, detectors)` — the seal boundary the kernel
/// calls in place of `hh_hir::ops::seal`: scan first (a literal credential
/// fails `SecretValueInDefinition`, AC-R-2.8.3-8), then `seal`. The scan's
/// errors are *collected* alongside `seal`'s like every other validation
/// failure.
pub fn seal_checked(
    doc: &HirDocument,
    sealed_at: u64,
    detectors: &DetectorSet,
) -> Result<SealedDefinition, Vec<HirError>> {
    let hits = scan_document(doc, detectors);
    if !hits.is_empty() {
        return Err(hits
            .into_iter()
            .map(|h| HirError::SecretValueInDefinition {
                detail: format!("{} detector hit at byte {}", h.detector.as_str(), h.span.0),
            })
            .collect());
    }
    hh_hir::ops::seal(doc, sealed_at)
}

/// `check_diff(diff, detectors)` — the evolution-context gate (AC-R-2.8.3-8):
/// a `HirDiff` whose canonical form introduces a literal credential is
/// rejected in every context. (A `Text` body is hash-addressed in the diff —
/// leaf-content credentials are caught by `seal_checked` on the applied
/// target, which every evolution path re-seals through.)
pub fn check_diff(diff: &HirDiff, detectors: &DetectorSet) -> Result<(), Vec<HirError>> {
    let bytes = diff.canonical_bytes();
    let hits: Vec<crate::redact::Hit> =
        detect(std::str::from_utf8(&bytes).unwrap_or_default(), detectors)
            .into_iter()
            .filter(|h| h.detector != DetectorKind::PlaceholderPassthrough)
            .collect();
    if hits.is_empty() {
        Ok(())
    } else {
        Err(hits
            .into_iter()
            .map(|h| HirError::SecretValueInDefinition {
                detail: format!(
                    "{} detector hit in diff op at byte {}",
                    h.detector.as_str(),
                    h.span.0
                ),
            })
            .collect())
    }
}
