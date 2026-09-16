//! The **assembly schema surface**: the per-field **merge policy** the grammar declares
//! (ADR-0024 decision 5 — `replace | append-set | stricter-wins | forbidden-below(precedence)`,
//! never generic deep-merge; `compose` consumes this table at Stage 3) plus the
//! byte-level codec pair `encode`/`load` — thin delegates over [`Assembly::to_json`] /
//! [`Assembly::from_json`] (CC7: `grammar.rs` is the single encoding source; this module
//! names the schema-level operations the spec pins).
//!
//! `load(encode(a)) = a`. `load` surfaces the member-wise decode diagnostics
//! (`C-LOAD-1 ParseError` on malformed bytes/members, `C-LOAD-2 UnknownDialect` on a
//! dialect mismatch, `C-LOAD-3 UnknownKey` on a non-`ext` member outside the closed
//! grammar) — `Err` when any `error`-severity diagnostic was produced.

use hh_provenance::ProvenanceRecord;

use crate::diagnostics::{AssemblyDiagnostic, Severity};
use crate::grammar::{Assembly, ASSEMBLY_DIALECT};

/// The merge policy a schema field declares (ADR-0024 decision 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergePolicy {
    /// The winning layer's value replaces the lower one.
    Replace,
    /// Set union across layers.
    AppendSet,
    /// The stricter value wins regardless of precedence (a disablement from any layer wins).
    StricterWins,
    /// Only layers at or above `precedence` may set the field.
    ForbiddenBelow(u64),
}

/// The per-field merge-policy table (longest declared prefix wins). Every grammar field
/// has a row — a field without one is `C-COMP-2 MergePolicyUndeclared` (internal).
pub const MERGE_POLICIES: &[(&str, MergePolicy)] = &[
    ("/dialect", MergePolicy::ForbiddenBelow(u64::MAX)),
    ("/profile_binding", MergePolicy::Replace),
    ("/slots", MergePolicy::Replace),
    ("/slots/*/enabled", MergePolicy::StricterWins),
    ("/slots/*/params", MergePolicy::Replace),
    ("/parameters", MergePolicy::Replace),
    ("/values", MergePolicy::Replace),
    ("/entities", MergePolicy::AppendSet),
    ("/constraints", MergePolicy::AppendSet),
    ("/layers", MergePolicy::AppendSet),
    ("/ext", MergePolicy::AppendSet),
];

/// The merge policy of `path` (a JSON pointer) — `None` for a path outside the grammar.
pub fn merge_policy(path: &str) -> Option<MergePolicy> {
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut best: Option<(usize, MergePolicy)> = None;
    for (pattern, policy) in MERGE_POLICIES {
        let psegs: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
        if psegs.len() > segs.len() {
            continue;
        }
        let hit = psegs
            .iter()
            .zip(segs.iter())
            .all(|(p, s)| *p == "*" || p == s);
        if hit && best.is_none_or(|(len, _)| psegs.len() > len) {
            best = Some((psegs.len(), *policy));
        }
    }
    best.map(|(_, p)| p)
}

/// `encode(a)` — the canonical bytes of the `assembly` member ([`Assembly::to_json`]
/// under the one canonicalizer — CC1).
pub fn encode(a: &Assembly) -> Vec<u8> {
    a.to_json().to_canonical_string().into_bytes()
}

/// `load(bytes, expected_dialect?) → Assembly` (§3.3.4). The member-wise decode is
/// never fail-fast — diagnostics accumulate and `Err` carries them when any is
/// `error`-severity. `expected_dialect` defaults to the grammar dialect (`HIR/1`).
pub fn load(
    bytes: &[u8],
    expected_dialect: Option<&str>,
) -> Result<Assembly, Vec<AssemblyDiagnostic>> {
    let kernel = ProvenanceRecord::kernel("kernel:assembly:load", 0);
    let json = hh_wire::canonical::parse_canonical(bytes).map_err(|e| {
        vec![{
            let mut d = AssemblyDiagnostic {
                code: crate::diagnostics::Code::LoadParse,
                class: None,
                severity: Severity::Error,
                path: "/assembly".into(),
                source_layer: None,
                subject: "assembly".into(),
                stage: crate::diagnostics::Stage::Desugar,
                detail: crate::diagnostics::detail_text(
                    format!("not canonical JSON: {e}"),
                    &kernel,
                ),
                remedy: "supply canonical bytes (the `encode` output form)".into(),
                owner_adr: "ADR-0148".into(),
            };
            d.subject = "assembly".into();
            d
        }]
    })?;
    let mut diags = Vec::new();
    let Some(a) = Assembly::from_json(&json, "/assembly", &kernel, &mut diags) else {
        return Err(diags);
    };
    let expected = expected_dialect.unwrap_or(ASSEMBLY_DIALECT);
    if a.dialect != expected {
        diags.push(AssemblyDiagnostic {
            code: crate::diagnostics::Code::LoadDialect,
            class: None,
            severity: Severity::Error,
            path: "/assembly/dialect".into(),
            source_layer: None,
            subject: a.dialect.clone(),
            stage: crate::diagnostics::Stage::Desugar,
            detail: crate::diagnostics::detail_text(
                format!("dialect `{}` is not `{expected}`", a.dialect),
                &kernel,
            ),
            remedy: "author `dialect` as the grammar dialect".into(),
            owner_adr: "ADR-0148".into(),
        });
    }
    if diags.iter().any(|d| d.severity == Severity::Error) {
        Err(diags)
    } else {
        Ok(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_policy_is_declared_for_every_grammar_field_and_longest_prefix_wins() {
        for path in [
            "/dialect",
            "/profile_binding",
            "/slots",
            "/slots/control_strategy",
            "/slots/control_strategy/params/k",
            "/parameters/k",
            "/values/k",
            "/entities/g",
            "/constraints/0",
            "/layers/0",
            "/ext/x",
        ] {
            assert!(merge_policy(path).is_some(), "{path}");
        }
        assert_eq!(
            merge_policy("/slots/context_policy/enabled"),
            Some(MergePolicy::StricterWins)
        );
        assert_eq!(
            merge_policy("/slots/context_policy/params"),
            Some(MergePolicy::Replace)
        );
        assert_eq!(
            merge_policy("/dialect"),
            Some(MergePolicy::ForbiddenBelow(u64::MAX))
        );
        assert_eq!(merge_policy("/nope"), None);
    }

    #[test]
    fn load_encode_round_trip() {
        let a = Assembly::empty();
        let decoded = load(&encode(&a), None).expect("empty assembly round-trips");
        assert_eq!(decoded, a);
    }

    #[test]
    fn load_rejects_malformed_and_wrong_dialect() {
        assert!(load(b"{not json", None).is_err());
        let a = Assembly::empty();
        let errs = load(&encode(&a), Some("hir/2")).expect_err("dialect mismatch");
        assert!(errs
            .iter()
            .any(|d| d.code == crate::diagnostics::Code::LoadDialect));
    }
}
