//! The glossary register and its check (spec §2.11; AC-A2-7; ADR-0008, ADR-0048).
//!
//! The Ontology register breaks vocabulary ties; the readiness glossary check fails on
//! unregistered synonyms (§2.0 "Vocabulary tie-breaker"). AC-A2-7: the glossary check passes
//! with the v4 register — **no unregistered synonym, no "meta-" sub-concept, "HarnessHarness"
//! only as the proper noun**. [`glossary_check`] returns a finding per violation; a clean text
//! returns `[]`.

/// A representative slice of the canonical v4 register (§2.11). Not exhaustive of the whole
/// spec glossary, but the terms this ontology introduces; the full register lives in
/// `registers/ontology.md`.
pub const REGISTERED_TERMS: [&str; 24] = [
    "harness",
    "agent",
    "model snapshot",
    "model set",
    "ModelRoleTable",
    "environment",
    "task distribution",
    "budget",
    "harness parameterization",
    "Harness Definition",
    "Harness IR",
    "Model Profile",
    "plane",
    "home plane",
    "model boundary",
    "environment boundary",
    "policy stack",
    "harness objective",
    "harness effect",
    "decision point",
    "control boundary",
    "configuration",
    "compatibility surface",
    "participant",
];

/// Unregistered synonyms the register explicitly rejects, each mapped to its canonical spelling
/// (§2.0/§2.11; the identifiers `artefact` vs prose `artifact` is CF-032; "HTIR" is never used).
pub const FORBIDDEN_SYNONYMS: [(&str, &str); 4] = [
    ("HTIR", "Harness IR (HIR)"),
    ("meta-harness", "HarnessHarness (proper noun only)"),
    ("meta-agent", "harness"),
    (
        "grey-box class",
        "hosted participant with observability_level ⊇ {model_io}",
    ),
];

/// A single glossary violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlossaryFinding {
    /// An unregistered synonym for a canonical term.
    UnregisteredSynonym {
        /// The synonym found in the text.
        found: String,
        /// The canonical spelling it should be.
        canonical: String,
    },
    /// A coined "meta-" sub-concept (forbidden — §2.0/§2.11; "no 'meta-' sub-concept").
    MetaSubConcept {
        /// The offending token.
        found: String,
    },
    /// "HarnessHarness" used as anything but the exact proper noun (a lower/alt-cased variant).
    ImproperHarnessHarness {
        /// The offending variant.
        found: String,
    },
}

fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':' | '(' | ')' | '.' | '"'))
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect()
}

/// Run the glossary check over `text` (AC-A2-7). Returns every violation found; an empty vector
/// means the text passes the v4 register.
pub fn glossary_check(text: &str) -> Vec<GlossaryFinding> {
    let mut findings = Vec::new();
    let lower = text.to_lowercase();

    // 1. Unregistered synonyms (case-insensitive substring match on the whole text).
    for (syn, canonical) in FORBIDDEN_SYNONYMS {
        if lower.contains(&syn.to_lowercase()) {
            findings.push(GlossaryFinding::UnregisteredSynonym {
                found: syn.to_string(),
                canonical: canonical.to_string(),
            });
        }
    }

    // 2. Any coined "meta-" sub-concept token (beyond the ones already caught as synonyms).
    for tok in tokens(text) {
        let tl = tok.to_lowercase();
        if tl.starts_with("meta-")
            && !FORBIDDEN_SYNONYMS
                .iter()
                .any(|(s, _)| s.to_lowercase() == tl)
        {
            findings.push(GlossaryFinding::MetaSubConcept { found: tok });
        }
    }

    // 3. "HarnessHarness" must appear only as the exact proper noun. Flag alt-cased/hyphenated
    //    variants ("harnessharness", "Harnessharness", "harness-harness") — but not the correct
    //    "HarnessHarness".
    for tok in tokens(text) {
        let stripped: String = tok.chars().filter(|c| c.is_alphabetic()).collect();
        let hyphenated = tok.to_lowercase() == "harness-harness";
        if (stripped.to_lowercase() == "harnessharness" && stripped != "HarnessHarness")
            || hyphenated
        {
            findings.push(GlossaryFinding::ImproperHarnessHarness { found: tok });
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_is_present() {
        // AC-A2-7: the v4 register is present.
        assert!(REGISTERED_TERMS.contains(&"home plane"));
        assert!(REGISTERED_TERMS.contains(&"compatibility surface"));
        assert!(REGISTERED_TERMS.len() >= 20);
    }

    #[test]
    fn clean_canonical_text_passes() {
        let text = "The harness parameterization θ names a Harness Definition; the control \
                    boundary β lives on the compatibility surface. HarnessHarness is the product.";
        assert_eq!(glossary_check(text), Vec::new());
    }

    #[test]
    fn unregistered_synonym_is_flagged() {
        let findings = glossary_check("The HTIR is the typed representation.");
        assert!(findings.iter().any(|f| matches!(
            f,
            GlossaryFinding::UnregisteredSynonym { found, .. } if found == "HTIR"
        )));
    }

    #[test]
    fn meta_sub_concept_is_flagged() {
        let findings = glossary_check("We define a meta-planner over the loop.");
        assert!(findings.iter().any(
            |f| matches!(f, GlossaryFinding::MetaSubConcept { found } if found == "meta-planner")
        ));
    }

    #[test]
    fn meta_harness_is_flagged_as_synonym() {
        // "meta-harness" is an explicitly forbidden synonym for the proper noun.
        let findings = glossary_check("This is a meta-harness.");
        assert!(findings.iter().any(|f| matches!(
            f,
            GlossaryFinding::UnregisteredSynonym { found, .. } if found == "meta-harness"
        )));
    }

    #[test]
    fn miscased_harnessharness_is_flagged() {
        let findings = glossary_check("we built harnessharness for this.");
        assert!(findings
            .iter()
            .any(|f| matches!(f, GlossaryFinding::ImproperHarnessHarness { .. })));
        // The correct proper noun does not trip the check.
        assert!(glossary_check("HarnessHarness ships.")
            .iter()
            .all(|f| !matches!(f, GlossaryFinding::ImproperHarnessHarness { .. })));
    }
}
