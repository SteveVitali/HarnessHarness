//! `test_procedure` — the §5c.5 procedure-test Lab instrument (row:
//! `test_procedure(P, suite, profile, target, budget) →
//! ProcedureTestReport{per_validator[{ref, verdict, run_id}], activated,
//! followed, cost, outcome_class}` + a `ProcedureConformance` record).
//!
//! The instrument run is `charged_to = instrument` under a `MatchSpec`:
//! an arm without `match_spec` refuses `MissingMatchSpec`; an arm without
//! an `eval_budget` refuses `UnbudgetedArm` (T-LCD-14 — AC-R-2.4.5-6).
//!
//! Staleness is a *report*, never a guess: a fixture whose dependency the
//! ledger reports revoked yields `outcome_class = "stale-by-dependency"`
//! and `ProcedureConformance.validity = unknown` — never `valid`
//! (AC-R-2.4.5-6).

use std::collections::BTreeMap;

use hh_budget::MatchSpec;
use hh_hir::document::Node;
use hh_hir::procedure::{CompilationTarget, ProcedureProfile, SelectCtx};
use hh_wire::json::Json;

use crate::experiment::ExperimentRefusal;

/// The fixture-validity input the caller resolves from the ledger before
/// the run (CC5 — the instrument never performs I/O): a revoked dependency
/// is the `stale-by-dependency` trigger (AC-R-2.4.5-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixtureValidity {
    /// Every fixture dependency resolves to a live version.
    Fresh,
    /// A fixture dependency was revoked (`stale-by-dependency`).
    Revoked {
        /// The revoked dependency's ref.
        dependency: String,
    },
    /// The caller could not decide (an unresolved ref) — also `unknown`.
    Undecidable {
        /// Why.
        detail: String,
    },
}

/// One validator run the suite produced (`{ref, verdict, run_id}` — the
/// report records them verbatim; the instrument composes, never invents).
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatorRun {
    /// The validator ref.
    pub validator_ref: String,
    /// The recorded verdict (`Json` — pass/fail/typed).
    pub verdict: Json,
    /// The run id the verdict was produced under.
    pub run_id: String,
    /// The run's charged cost (`dimension → amount`, instrument-charged).
    pub cost: BTreeMap<String, u64>,
}

/// The `test_procedure` input (§5c.5 row: `P, suite, profile, target,
/// budget`).
pub struct ProcedureTestSpec<'a> {
    /// The procedure under test.
    pub procedure: &'a Node,
    /// The procedure's profile (`ext["hh/procedure"]` parsed by the caller
    /// or [`hh_hir::procedure::ProcedureProfile::from_node`]).
    pub profile: Option<&'a ProcedureProfile>,
    /// The requested target (`None` = `select_target` decides under `ctx`).
    pub target: Option<CompilationTarget>,
    /// The suite's validator runs (already executed — the instrument is a
    /// deterministic composer).
    pub validators: Vec<ValidatorRun>,
    /// Whether the run observed activation (the trigger/precondition gate
    /// fired) — a transcript fact the caller supplies.
    pub activated: bool,
    /// Whether the run observed the procedure's steps followed.
    pub followed: bool,
    /// The arm's `MatchSpec` — mandatory (`MissingMatchSpec`).
    pub match_spec: Option<MatchSpec>,
    /// The arm's `eval_budget` (`dimension → cap`) — mandatory and
    /// non-empty (`UnbudgetedArm`).
    pub eval_budget: BTreeMap<String, u64>,
    /// The fixture-validity resolution (see [`FixtureValidity`]).
    pub fixture_validity: FixtureValidity,
    /// The `select_target` context (the bound control strategy, budgets).
    pub select_ctx: SelectCtx,
}

/// `ProcedureTestReport` (§5c.5) — the run's deterministic record.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureTestReport {
    /// `per_validator[{ref, verdict, run_id}]`.
    pub per_validator: Vec<ValidatorRun>,
    /// `activated` — the trigger/precondition gate fired.
    pub activated: bool,
    /// `followed` — the procedure's steps were followed.
    pub followed: bool,
    /// `cost` — the instrument-charged totals (`dimension → amount`).
    pub cost: BTreeMap<String, u64>,
    /// `outcome_class ∈ {completed, stale-by-dependency, undecidable}`.
    pub outcome_class: String,
    /// The selected compilation target (the run's `select_target` verdict).
    pub target: CompilationTarget,
    /// The `MatchSpec` the run was matched under.
    pub match_spec: MatchSpec,
}

/// `ProcedureConformance` — the durable conformance record the instrument
/// emits. `validity` is a *decided* value: `valid` only when every piece
/// of the chain resolved fresh; `unknown` on any staleness/undecidability —
/// a revoked fixture dependency can never report `valid` (AC-R-2.4.5-6).
#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureConformance {
    /// The procedure's semantic id.
    pub procedure_id: String,
    /// `valid | unknown` — never `valid` when stale.
    pub validity: ConformanceValidity,
    /// The staleness marker (`stale-by-dependency:<dep>`), when any.
    pub stale_reason: Option<String>,
    /// The target the conformance was measured under.
    pub target: CompilationTarget,
}

/// `ProcedureConformance.validity ∈ {valid, unknown}` — a revoked or
/// undecidable fixture caps at `unknown`; nothing upgrades silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConformanceValidity {
    /// The whole chain resolved fresh and the run completed.
    Valid,
    /// Staleness or undecidability — never reported as `valid`.
    Unknown,
}

impl ConformanceValidity {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ConformanceValidity::Valid => "valid",
            ConformanceValidity::Unknown => "unknown",
        }
    }
}

/// `test_procedure(P, suite, profile, target, budget)` — the deterministic
/// composer (§5c.5; AC-R-2.4.5-6). Refusals are typed `ExperimentRefusal`s:
/// `MissingMatchSpec` and `UnbudgetedArm` bind before any composition.
pub fn test_procedure(
    spec: &ProcedureTestSpec<'_>,
    index: &BTreeMap<String, &Node>,
) -> Result<(ProcedureTestReport, ProcedureConformance), ExperimentRefusal> {
    let proc_id = spec.procedure.semantic_id();
    // T-LCD-14 — the matched-budget gates bind first.
    let match_spec =
        spec.match_spec
            .clone()
            .ok_or_else(|| ExperimentRefusal::MissingMatchSpec {
                arm: proc_id.clone(),
            })?;
    if spec.eval_budget.is_empty() {
        return Err(ExperimentRefusal::UnbudgetedArm { arm: proc_id });
    }
    // The target — the run's own `select_target` verdict when the arm does
    // not pin one (a refusal there propagates as `UnmatchedBudget`-class
    // detail, never a silent instruction fallback).
    let target = match spec.target {
        Some(t) => t,
        None => {
            hh_hir::procedure::select_target(spec.procedure, spec.profile, &spec.select_ctx, index)
                .map_err(|e| ExperimentRefusal::UnmatchedBudget {
                    detail: format!("select_target: {e}"),
                })?
                .target
        }
    };
    // `cost` — the instrument-charged sum over validator runs.
    let mut cost: BTreeMap<String, u64> = BTreeMap::new();
    for v in &spec.validators {
        for (d, a) in &v.cost {
            *cost.entry(d.clone()).or_insert(0) += a;
        }
    }
    let (outcome_class, validity, stale_reason) = match &spec.fixture_validity {
        FixtureValidity::Fresh => ("completed".to_string(), ConformanceValidity::Valid, None),
        FixtureValidity::Revoked { dependency } => (
            "stale-by-dependency".to_string(),
            ConformanceValidity::Unknown,
            Some(format!("stale-by-dependency:{dependency}")),
        ),
        FixtureValidity::Undecidable { detail } => (
            "undecidable".to_string(),
            ConformanceValidity::Unknown,
            Some(detail.clone()),
        ),
    };
    let report = ProcedureTestReport {
        per_validator: spec.validators.clone(),
        activated: spec.activated,
        followed: spec.followed,
        cost,
        outcome_class,
        target,
        match_spec,
    };
    let conformance = ProcedureConformance {
        procedure_id: spec.procedure.semantic_id(),
        validity,
        stale_reason,
        target,
    };
    Ok((report, conformance))
}
