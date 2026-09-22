//! Hand-stamped provenance & identity (R-2.1.5; §8.1). **Throwaway Stage-0 subset.**
//!
//! At Stage 0 the baseline does not *derive* authority from a runtime (that is the Stage-1
//! reference monitor, R-2.8.1). It **hand-stamps** the three provenance origins the ladder
//! calls for — `principal`, `definition`, `external` (§9.1 R-2.1.2/R-2.1.5 slice) — so the
//! end-to-end run has real, distinguishable authority classes without the durable subsystem.
//!
//! CC2 holds even here: authority is never read from content and never widened. A hand-stamp
//! is a fixed value set by the harness author, not a claim lifted from a payload; lifted
//! (`external`) content is the *lowest* class and can only attenuate (never confer) authority.

use hh_wire::json::Json;

/// The Stage-0 authority classes, ordered `external < definition < principal`. The full
/// seven-class lattice (R-2.8.2, ADR-0033) lands at S1.3; this is the minimal subset the
/// baseline needs to keep lifted content walled off from author intent (CC2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthorityClass {
    /// Content lifted from outside the harness (tool output, model text). Lowest; `unverified`.
    External = 0,
    /// The hand-authored definition itself (the react/minimal document, its role table).
    Definition = 1,
    /// The human/operator who launched the run (the task, the budget, the credential grant).
    Principal = 2,
}

impl AuthorityClass {
    pub fn as_str(self) -> &'static str {
        match self {
            AuthorityClass::External => "external",
            AuthorityClass::Definition => "definition",
            AuthorityClass::Principal => "principal",
        }
    }
}

/// A hand-stamped provenance record: who a record's authority comes from, and a short origin
/// label for the trace. Immutable once stamped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceRecord {
    pub origin: String,
    pub authority: AuthorityClass,
}

impl ProvenanceRecord {
    /// Stamp `principal` authority (the launching human/operator).
    pub fn principal(origin: impl Into<String>) -> Self {
        Self {
            origin: origin.into(),
            authority: AuthorityClass::Principal,
        }
    }

    /// Stamp `definition` authority (the hand-authored harness definition).
    pub fn definition(origin: impl Into<String>) -> Self {
        Self {
            origin: origin.into(),
            authority: AuthorityClass::Definition,
        }
    }

    /// Stamp `external` authority (content lifted from a tool result or the model). Always the
    /// lowest class — lifting can never confer more than `external` (CC2 narrow-or-preserve).
    pub fn external(origin: impl Into<String>) -> Self {
        Self {
            origin: origin.into(),
            authority: AuthorityClass::External,
        }
    }

    pub fn to_json(&self) -> Json {
        Json::obj([
            ("origin", Json::str(self.origin.clone())),
            ("authority", Json::str(self.authority.as_str())),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_are_ordered_external_lowest_principal_highest() {
        assert!(AuthorityClass::External < AuthorityClass::Definition);
        assert!(AuthorityClass::Definition < AuthorityClass::Principal);
    }

    #[test]
    fn stamps_carry_their_class() {
        assert_eq!(
            ProvenanceRecord::principal("operator").authority,
            AuthorityClass::Principal
        );
        assert_eq!(
            ProvenanceRecord::definition("react/minimal").authority,
            AuthorityClass::Definition
        );
        assert_eq!(
            ProvenanceRecord::external("tool:shell").authority,
            AuthorityClass::External
        );
    }

    #[test]
    fn json_renders_origin_and_authority() {
        let p = ProvenanceRecord::definition("react/minimal");
        let j = p.to_json();
        assert_eq!(
            j.get("authority").and_then(Json::as_str),
            Some("definition")
        );
        assert_eq!(
            j.get("origin").and_then(Json::as_str),
            Some("react/minimal")
        );
    }
}
