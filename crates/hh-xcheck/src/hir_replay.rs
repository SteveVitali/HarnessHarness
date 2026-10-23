//! The `hir-documents` arm's replay pipeline (R4.3 / DF-S1.2-2) — the one
//! spelling of `parse → validate → canonicalize → seal` over a committed
//! `<name>.input.json` member (CC1: `export` mints `expected.json` through
//! it and `self_check` recomputes through it; a second implementation
//! spells it from `CHECKER.md`).
//!
//! The answer object (one per case, `answers["arms"]["hir-documents"][<case>]`):
//!
//! ```json
//! {
//!   "canonical_sha256": "<hex>"|null,
//!   "definition_version_id": "<idp>"|null,
//!   "parse":    "ok" | "refuse:<Kind[,Kind…]>",
//!   "seal":     "sealed" | "refuse:<Kind[,Kind…]>" | "skipped",
//!   "sealed_sha256": "<hex>"|null,
//!   "validate": "valid" | "invalid:<Kind[,Kind…]>" | "skipped"
//! }
//! ```
//!
//! `<Kind>` is `HirError::kind_str()` — the closed §3.1.4 spelling; several
//! errors join `,`-separated in first-raised order. `skipped` marks a leg
//! the earlier refusal gated out; the `*_sha256`/`version_id` members are
//! `null` where their leg never ran.

use hharness_hir::HirError;
use hharness_wire::json::Json;
use hharness_wire::sha256_hex;

/// The error-kind spelling: first-raised order, duplicates collapsed —
/// `<K1>,<K2>` (the plugin arm's `refuse:<kind>` extended to the collected
/// error list `validate`/`seal` return).
fn kinds(errs: &[HirError]) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for e in errs {
        let k = e.kind_str();
        if !seen.contains(&k) {
            seen.push(k);
        }
    }
    seen.join(",")
}

/// Run the pipeline over `bytes` (the committed `<name>.input.json`) and
/// return the case's answer object. `sealed_at` is the deterministic seal
/// instant the corpus pins (`hharness_hir::corpus::CORPUS_SEALED_AT`).
pub fn replay(bytes: &[u8], sealed_at: u64) -> Json {
    let mut parse_v = Json::str("ok");
    let mut validate_v = Json::str("skipped");
    let mut seal_v = Json::str("skipped");
    let mut canonical_sha256 = Json::Null;
    let mut sealed_sha256 = Json::Null;
    let mut definition_version_id = Json::Null;

    let doc = match hharness_hir::parse_document(bytes) {
        Ok(d) => d,
        Err(e) => {
            parse_v = Json::str(format!("refuse:{}", kinds(&[e])));
            return Json::obj([
                ("canonical_sha256", canonical_sha256),
                ("definition_version_id", definition_version_id),
                ("parse", parse_v),
                ("seal", seal_v),
                ("sealed_sha256", sealed_sha256),
                ("validate", validate_v),
            ]);
        }
    };

    let canonical = hharness_hir::canonicalize(&doc);
    canonical_sha256 = Json::str(sha256_hex(&canonical));

    match hharness_hir::validate_doc(&doc) {
        Err(errs) => {
            validate_v = Json::str(format!("invalid:{}", kinds(&errs)));
        }
        Ok(_) => {
            validate_v = Json::str("valid");
            match hharness_hir::seal(&doc, sealed_at) {
                Err(errs) => {
                    seal_v = Json::str(format!("refuse:{}", kinds(&errs)));
                }
                Ok(sealed) => {
                    seal_v = Json::str("sealed");
                    sealed_sha256 = Json::str(sha256_hex(&sealed.canonical_bytes()));
                    definition_version_id = Json::str(sealed.definition_ref.version_id.clone());
                }
            }
        }
    }

    Json::obj([
        ("canonical_sha256", canonical_sha256),
        ("definition_version_id", definition_version_id),
        ("parse", parse_v),
        ("seal", seal_v),
        ("sealed_sha256", sealed_sha256),
        ("validate", validate_v),
    ])
}
