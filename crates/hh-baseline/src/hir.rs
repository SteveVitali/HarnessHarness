//! The hand-authored `react/minimal` HIR/1 document with `validate` only (R-2.1.2, R-2.1.5,
//! R-2.6.1; §2, §3.1). **Throwaway Stage-0 subset.**
//!
//! §9.1 R-2.1.2/R-2.1.5 slice: "the baseline as a hand-authored HIR/1 document with `validate`
//! only; `principal`/`definition`/`external` hand-stamped". `canonicalize`/`identity`/`seal`
//! and the full HIR/1 grammar land at S1.4; this is the minimal validated document the driver
//! runs. `react/minimal` is the mini-SWE-agent class and the T-LCD-03 anchor (R-2.6.1).

use crate::gateway::ModelRoleTable;
use crate::provenance::{self, AuthorityClass, ProvenanceRecord};

/// A hand-authored harness definition. There is exactly one class at Stage 0: `react/minimal`.
#[derive(Debug, Clone)]
pub struct HarnessDefinition {
    pub class: String,
    /// The launching operator (principal authority).
    pub principal: ProvenanceRecord,
    /// The definition itself (definition authority).
    pub definition: ProvenanceRecord,
    /// A stamp for lifted content the run will produce (external authority) — present so the
    /// three hand-stamps of §9.1 are all in the document.
    pub external: ProvenanceRecord,
    /// The one-entry static role table (R-2.3.2⁰).
    pub role_table: ModelRoleTable,
    /// The coding task the principal set.
    pub task: String,
}

/// Why a hand-authored document failed `validate` (the Stage-0 subset of §3.1's diagnostics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    UnknownClass {
        class: String,
    },
    WrongAuthority {
        field: &'static str,
        expected: &'static str,
    },
    EmptyTask,
    EmptyRoleTable,
}

impl HarnessDefinition {
    /// `validate` — the only HIR/1 operation at Stage 0. Checks the class is the one supported
    /// class, the three provenance stamps carry their required authority, the role table is
    /// non-empty, and the task is present. No `canonicalize`/`seal`/`identity` (those are S1.4).
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.class != "react/minimal" {
            return Err(ValidationError::UnknownClass {
                class: self.class.clone(),
            });
        }
        if self.principal.authority != AuthorityClass::Principal {
            return Err(ValidationError::WrongAuthority {
                field: "principal",
                expected: "principal",
            });
        }
        if self.definition.authority != AuthorityClass::Definition {
            return Err(ValidationError::WrongAuthority {
                field: "definition",
                expected: "definition",
            });
        }
        if self.external.authority != AuthorityClass::External {
            return Err(ValidationError::WrongAuthority {
                field: "external",
                expected: "external",
            });
        }
        if self.task.trim().is_empty() {
            return Err(ValidationError::EmptyTask);
        }
        if self.role_table.model_set().is_empty() {
            return Err(ValidationError::EmptyRoleTable);
        }
        Ok(())
    }
}

/// The one hand-authored baseline definition: a `react/minimal` agent whose task is to write a
/// file and read it back through the sandboxed tool executor.
pub fn baseline_definition(task: impl Into<String>) -> HarnessDefinition {
    HarnessDefinition {
        class: "react/minimal".into(),
        principal: provenance::principal("operator:cli"),
        definition: provenance::definition("react/minimal@baseline"),
        external: provenance::external("lifted:tool+model"),
        role_table: ModelRoleTable::single("driver", "stub/model-A"),
        task: task.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_baseline_definition_validates() {
        assert!(baseline_definition("write hello.txt and read it back")
            .validate()
            .is_ok());
    }

    #[test]
    fn unknown_class_is_rejected() {
        let mut d = baseline_definition("t");
        d.class = "react/steerable".into();
        assert_eq!(
            d.validate(),
            Err(ValidationError::UnknownClass {
                class: "react/steerable".into()
            })
        );
    }

    #[test]
    fn wrong_hand_stamp_is_rejected() {
        let mut d = baseline_definition("t");
        d.principal = provenance::external("bad");
        assert_eq!(
            d.validate(),
            Err(ValidationError::WrongAuthority {
                field: "principal",
                expected: "principal"
            })
        );
    }

    #[test]
    fn empty_task_is_rejected() {
        let mut d = baseline_definition("t");
        d.task = "   ".into();
        assert_eq!(d.validate(), Err(ValidationError::EmptyTask));
    }
}
