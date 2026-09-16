//! The code/data boundary (§3.3.3; AC-CC-09): the **MUST be code** / **MUST be data**
//! lists and the migration ladder's rungs — published as data so the boundary is
//! auditable, not just prose. Every subsystem ADR states its rung (this ticket's is
//! ADR-0240); the cross-ADR audit is DF-S1.9-4.

/// §3.3.3's **MUST be code** list — behaviour the definition must never smuggle into
/// data (paraphrased from the spec table; the spec is the authority).
pub const MUST_BE_CODE: &[&str] = &[
    "the harness step's control flow (the assembler's own evaluation of a bound definition)",
    "the class contracts' operation semantics (what `decide`/`on_budget_exhausted` mean)",
    "the resolver's pinning and substitution rules",
    "the conformance suites' oracles",
    "the monitor's enforcement of budgets, permissions and validators",
    "the kernel's provenance/identity minting",
];

/// §3.3.3's **MUST be data** list — what only data can be (diffed, content-addressed,
/// swept, ablated, versioned, attributed, edited under governance).
pub const MUST_BE_DATA: &[&str] = &[
    "which variant fills each slot and whether it is `enabled`",
    "every parameter value and the declared parameter space",
    "every reference to profile constraints, entities, environments and budgets",
    "permission/effect/validator declarations",
    "prompts and instructions as `Text` leaves",
    "tool/skill/procedure inventories",
    "conditioned rules with their debt records",
    "experiment overrides",
];

/// The migration-ladder rungs (§3.3.3 — the definition-of-done ladder each subsystem
/// ADR states its position on; C0/Stage 1 is this ticket's rung).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LadderRung {
    /// C0 — the single-machine, in-process, offline core (this build).
    C0,
    /// C1 — the compiled/profile-compiled tier (out-of-process variants, stage 6).
    C1,
    /// C2 — the organisation tier (org-level layers, the services).
    C2,
    /// C3 — the Lab's experiment/serving tier.
    C3,
    /// C4 — the evolution tier.
    C4,
}

impl LadderRung {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            LadderRung::C0 => "C0",
            LadderRung::C1 => "C1",
            LadderRung::C2 => "C2",
            LadderRung::C3 => "C3",
            LadderRung::C4 => "C4",
        }
    }
}

/// This crate's rung — `C0` (§3.3.13's Stage-1 slice).
pub const RUNG: LadderRung = LadderRung::C0;
