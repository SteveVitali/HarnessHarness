//! `HandleLeak` — the I-H1 out-of-band checks (ADR-0051 H-1; AC-R-2.8.1-2's
//! detection half): a `HandleId` never appears in a model-facing surface, a
//! `Text` leaf, a tool result or a `CompiledPayload`.
//!
//! - **static** — [`check_static`] scans a compiled surface record /
//!   `model_surface` JSON: every string leaf anywhere in the structure is
//!   checked for a `hnd-` spelling (the *spelling* is the leak — a real
//!   `HandleId` is an allocated kernel row, and any `hnd-*` string in a
//!   model-facing record is a leak or a forgery attempt either way);
//! - **runtime** — [`check_outgoing`] scans an outgoing context item's JSON
//!   (the rendered item the context builder assembled) the same way.
//!
//! A leak found in *incoming* content (a tool result, a workspace file) is not
//! authority — the monitor's inputs never include it (AC-2); these checks are
//! the outgoing/static boundary.

use hh_wire::json::Json;

/// `HandleLeak` — a `hnd-` spelling was found where none may appear.
#[derive(Debug, Clone, PartialEq)]
pub struct HandleLeak {
    /// The JSON path of the offending string (`a.b[3].c`).
    pub path: String,
    /// The offending spelling (truncated — the leak is reported, not echoed).
    pub spelling: String,
}

/// Scan a JSON value's string leaves for a `hnd-` spelling — the first leak
/// found is the error (the scan is total over the structure).
pub fn scan_json(j: &Json, path: &str) -> Result<(), HandleLeak> {
    match j {
        Json::Str(s) => {
            if let Some(sp) = find_handle_spelling(s) {
                return Err(HandleLeak {
                    path: path.to_string(),
                    spelling: sp,
                });
            }
            Ok(())
        }
        Json::Arr(items) => {
            for (i, v) in items.iter().enumerate() {
                scan_json(v, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        Json::Obj(m) => {
            for (k, v) in m {
                if let Some(sp) = find_handle_spelling(k) {
                    return Err(HandleLeak {
                        path: format!("{path}.{k}"),
                        spelling: sp,
                    });
                }
                scan_json(v, &format!("{path}.{k}"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// The static check — a compiled `model_surface` record (the exposure
/// definition's surface JSON) must carry no handle spelling anywhere.
pub fn check_static(model_surface: &Json) -> Result<(), HandleLeak> {
    scan_json(model_surface, "$")
}

/// The runtime check — an outgoing context item / tool-result surface about
/// to be rendered must carry no handle spelling.
pub fn check_outgoing(item: &Json) -> Result<(), HandleLeak> {
    scan_json(item, "$")
}

/// Find a `hnd-*` token inside a string — token-boundary aware (a `hnd-`
/// preceded by an identifier character is part of a larger word, not a
/// spelling).
fn find_handle_spelling(s: &str) -> Option<String> {
    for (i, _) in s.match_indices("hnd-") {
        let preceded_ok = i == 0
            || !s[..i]
                .chars()
                .last()
                .map(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                .unwrap_or(false);
        if !preceded_ok {
            continue;
        }
        let tail = &s[i + 4..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(tail.len());
        if end > 0 {
            return Some(s[i..i + 4 + end].to_string());
        }
    }
    None
}
