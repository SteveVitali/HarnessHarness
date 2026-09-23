//! `load`/`encode` (§3.3.4): `load(bytes, expected_dialect?) → Assembly | ParseError |
//! UnknownDialect` over the canonical `hir/1` encoding. Unknown non-`ext` keys are
//! `C-LOAD-3` errors; `load(encode(a)) = a` (the round-trip the tests pin). Every
//! failure surfaces as typed `C-LOAD-*` diagnostics — never a bare `Err` string
//! (ADR-0148; `C-INT-1` stays reserved).

use hh_hir::document::{parse_document, HirDocument};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::diagnostics::{detail_text, AssemblyDiagnostic, Code, Severity, Stage};
use crate::grammar::{Assembly, ASSEMBLY_DIALECT};

/// What `load` returns — the parsed document plus its decoded assembly section (the
/// member-wise decode's diagnostics accompany it — a `LoadedDefinition` may carry a
/// *partial* assembly plus diagnostics; the section is `None` only when absent or not
/// an object).
#[derive(Debug)]
pub struct LoadedDefinition {
    /// The parsed `hir/1` document.
    pub document: HirDocument,
    /// The decoded assembly section (partial when members failed — diagnostics list them).
    pub assembly: Option<Assembly>,
    /// The diagnostics the decode produced (empty on a clean load).
    pub diagnostics: Vec<AssemblyDiagnostic>,
}

/// `load(bytes, expected_dialect)` — parse the canonical encoding, check the dialects
/// (document `hir_version` and `assembly.dialect` against `expected` / `HIR/1`), decode
/// the assembly member member-wise. `Err(diagnostics)` only when the document itself
/// fails to parse; member-level failures ride in `LoadedDefinition.diagnostics`
/// (never fail-fast).
pub fn load(
    bytes: &[u8],
    expected_dialect: Option<&str>,
    kernel: &ProvenanceRecord,
) -> Result<LoadedDefinition, Vec<AssemblyDiagnostic>> {
    let doc = parse_document(bytes).map_err(|e| {
        vec![AssemblyDiagnostic {
            code: Code::LoadParse,
            class: None,
            severity: Severity::Error,
            path: "/".into(),
            source_layer: None,
            subject: "document".into(),
            stage: Stage::Desugar,
            detail: detail_text(format!("document parse failed: {e:?}"), kernel),
            remedy: "supply a canonical hir/1 document".into(),
            owner_adr: "ADR-0148".into(),
        }]
    })?;
    let expected = expected_dialect.unwrap_or(hh_hir::DIALECT);
    let mut diags = Vec::new();
    if doc.hir_version != expected {
        diags.push(AssemblyDiagnostic {
            code: Code::LoadDialect,
            class: None,
            severity: Severity::Error,
            path: "/hir_version".into(),
            source_layer: None,
            subject: doc.hir_version.clone(),
            stage: Stage::Desugar,
            detail: detail_text(
                format!("document dialect `{}` is not `{expected}`", doc.hir_version),
                kernel,
            ),
            remedy: "load a document of the expected dialect".into(),
            owner_adr: "ADR-0148".into(),
        });
    }
    let assembly = doc
        .assembly
        .as_ref()
        .and_then(|j| Assembly::from_json(j, "/assembly", kernel, &mut diags));
    if let Some(a) = &assembly {
        if a.dialect != expected && a.dialect != ASSEMBLY_DIALECT {
            diags.push(AssemblyDiagnostic {
                code: Code::LoadDialect,
                class: None,
                severity: Severity::Error,
                path: "/assembly/dialect".into(),
                source_layer: None,
                subject: a.dialect.clone(),
                stage: Stage::Desugar,
                detail: detail_text(
                    format!("assembly dialect `{}` is not `{expected}`", a.dialect),
                    kernel,
                ),
                remedy: "author `dialect` as the grammar dialect".into(),
                owner_adr: "ADR-0148".into(),
            });
        }
    }
    Ok(LoadedDefinition {
        document: doc,
        assembly,
        diagnostics: diags,
    })
}

/// `encode(a)` — the canonical JSON the `assembly` member carries (`load(encode(a)) = a`).
pub fn encode(a: &Assembly) -> Json {
    a.to_json()
}
