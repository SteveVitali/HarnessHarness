//! `ExperimentError` — the engine's typed failure sum (spec §6.3 §2.2's error
//! column + the E-1 refusal set; S3.4a). A `Refusal(ExperimentRefusal)` carries
//! the closed register set verbatim — the boundary renders it, never remaps it.

use hh_budget::errors::BudgetError;
use hh_lab::experiment::ExperimentRefusal;
use hh_lab::expand::ExpandError;
use hh_ledger::errors::LedgerError;

/// The engine's failure sum.
#[derive(Debug)]
pub enum ExperimentError {
    /// A member of the closed `register` refusal set (E-1) — refusal, never a
    /// warning (T-LCD-14).
    Refusal(ExperimentRefusal),
    /// `expand` failed (a re-checked refusal or a per-cell assembly fault).
    Expand(ExpandError),
    /// The ledger refused/failed.
    Ledger(LedgerError),
    /// The budget account refused/failed (the ledgered refusal row precedes
    /// the error — AC-4).
    Budget(BudgetError),
    /// The experiment id is not registered in `LabDocs`.
    UnknownExperiment {
        /// The id.
        experiment_id: String,
    },
    /// `open_experiment` before `expand` produced a plan (the op-table's
    /// `PlanNotFound`).
    PlanNotFound {
        /// The experiment id.
        experiment_id: String,
    },
    /// The run is not a declared experiment run.
    NotAnExperimentRun {
        /// The run id.
        run_id: String,
    },
    /// The experiment run is closed (`S-4` — no launch/next after close).
    ExperimentClosed {
        /// The experiment run id.
        run_id: String,
    },
    /// The experiment run is paused (`S-4`).
    ExperimentPaused {
        /// The experiment run id.
        run_id: String,
        /// The pause reason.
        reason: String,
    },
    /// The run plan is not known to the experiment run's plan.
    UnknownRunPlan {
        /// The plan key.
        run_plan_id: String,
    },
    /// The run plan is not in a state the op admits (e.g. `launch` of a
    /// settled plan, `settle` of an unlaunched plan).
    BadPlanState {
        /// The plan key.
        run_plan_id: String,
        /// The detail.
        detail: String,
    },
    /// A second claimant on a live claim lease (`WouldBlock` at the op level).
    WouldBlock {
        /// The live holder.
        holder: String,
    },
    /// The subject pool cannot fund the run's slice (`E-2`; the experiment
    /// pauses `budget_exhausted`, never trims).
    InsufficientBudget {
        /// The failure detail.
        detail: String,
    },
    /// `settle` on a subject run that has not finished.
    RunNotFinished {
        /// The subject run id.
        run_id: String,
    },
    /// `close` precondition — eligible or claimed plans remain and the caller
    /// did not declare `partial = true`.
    CloseBlocked {
        /// The failure detail.
        detail: String,
    },
    /// The second `declared`/open on an already-open experiment (`SchemaViolation`
    /// at the ledger layer; a typed refusal at the engine layer).
    AlreadyOpen {
        /// The experiment id.
        experiment_id: String,
        /// The existing experiment run id.
        run_id: String,
    },
    /// A context resolver could not produce a required fact (suite tasks,
    /// arm configuration, budget body) — refused, never guessed.
    Unresolvable {
        /// What could not be resolved.
        detail: String,
    },
    /// The LabDocs store failed.
    Store {
        /// The failure detail.
        detail: String,
    },
}

impl ExperimentError {
    /// The stable error code the boundary renders.
    pub fn code(&self) -> &'static str {
        match self {
            ExperimentError::Refusal(r) => refusal_code(r),
            ExperimentError::Expand(ExpandError::Refusal(r)) => refusal_code(r),
            ExperimentError::Expand(ExpandError::Assembly { .. }) => "AssemblyFailed",
            ExperimentError::Ledger(_) => "LedgerError",
            ExperimentError::Budget(_) => "InsufficientBudget",
            ExperimentError::UnknownExperiment { .. } => "UnknownExperiment",
            ExperimentError::PlanNotFound { .. } => "PlanNotFound",
            ExperimentError::NotAnExperimentRun { .. } => "NotAnExperimentRun",
            ExperimentError::ExperimentClosed { .. } => "ExperimentClosed",
            ExperimentError::ExperimentPaused { .. } => "ExperimentPaused",
            ExperimentError::UnknownRunPlan { .. } => "UnknownRunPlan",
            ExperimentError::BadPlanState { .. } => "BadPlanState",
            ExperimentError::WouldBlock { .. } => "WouldBlock",
            ExperimentError::InsufficientBudget { .. } => "InsufficientBudget",
            ExperimentError::RunNotFinished { .. } => "RunNotFinished",
            ExperimentError::CloseBlocked { .. } => "CloseBlocked",
            ExperimentError::AlreadyOpen { .. } => "AlreadyOpen",
            ExperimentError::Unresolvable { .. } => "Unresolvable",
            ExperimentError::Store { .. } => "StoreError",
        }
    }
}

/// The refusal-code spelling for an `ExperimentRefusal` member (the E-1 set —
/// the boundary renders the code, never the payload).
pub fn refusal_code(r: &ExperimentRefusal) -> &'static str {
    match r {
        ExperimentRefusal::MissingMatchSpec { .. } => "MissingMatchSpec",
        ExperimentRefusal::UnbudgetedArm { .. } => "UnbudgetedArm",
        ExperimentRefusal::UnmatchedBudget { .. } => "UnmatchedBudget",
        ExperimentRefusal::IncommensurableMatch { .. } => "IncommensurableMatch",
        ExperimentRefusal::MissingPricingTable { .. } => "MissingPricingTable",
        ExperimentRefusal::InadmissibleFactor { .. } => "InadmissibleFactor",
        ExperimentRefusal::MissingPreRegistration => "MissingPreRegistration",
        ExperimentRefusal::SplitUnassigned { .. } => "SplitUnassigned",
        ExperimentRefusal::LeakedSplit { .. } => "LeakedSplit",
        ExperimentRefusal::UnsealedArtifact { .. } => "UnsealedArtifact",
        ExperimentRefusal::InsufficientReplicates { .. } => "InsufficientReplicates",
        ExperimentRefusal::ResolutionInsufficient { .. } => "ResolutionInsufficient",
        ExperimentRefusal::ProfilePinnedAcrossProfiles { .. } => {
            "ProfilePinnedAcrossProfiles"
        }
        ExperimentRefusal::DependsOnDriftedCapability { .. } => "DependsOnDriftedCapability",
        ExperimentRefusal::NotARetirementDiff { .. } => "NotARetirementDiff",
        ExperimentRefusal::AdaptiveOutsideSearch { .. } => "AdaptiveOutsideSearch",
        ExperimentRefusal::Schema(_) => "SchemaViolation",
    }
}

impl std::fmt::Display for ExperimentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExperimentError::Refusal(r) => write!(f, "{}: {r:?}", refusal_code(r)),
            ExperimentError::Expand(e) => write!(f, "expand: {e:?}"),
            ExperimentError::Ledger(e) => write!(f, "ledger: {e:?}"),
            ExperimentError::Budget(e) => write!(f, "budget: {e:?}"),
            ExperimentError::UnknownExperiment { experiment_id } => {
                write!(f, "unknown experiment {experiment_id}")
            }
            ExperimentError::PlanNotFound { experiment_id } => {
                write!(f, "no CellPlan for {experiment_id} (run expand first)")
            }
            ExperimentError::NotAnExperimentRun { run_id } => {
                write!(f, "{run_id} is not a declared experiment run")
            }
            ExperimentError::ExperimentClosed { run_id } => {
                write!(f, "experiment run {run_id} is closed")
            }
            ExperimentError::ExperimentPaused { run_id, reason } => {
                write!(f, "experiment run {run_id} is paused ({reason})")
            }
            ExperimentError::UnknownRunPlan { run_plan_id } => {
                write!(f, "unknown run plan {run_plan_id}")
            }
            ExperimentError::BadPlanState { run_plan_id, detail } => {
                write!(f, "run plan {run_plan_id}: {detail}")
            }
            ExperimentError::WouldBlock { holder } => {
                write!(f, "claim held by {holder}")
            }
            ExperimentError::InsufficientBudget { detail } => {
                write!(f, "insufficient budget: {detail}")
            }
            ExperimentError::RunNotFinished { run_id } => {
                write!(f, "subject run {run_id} has not finished")
            }
            ExperimentError::CloseBlocked { detail } => write!(f, "close blocked: {detail}"),
            ExperimentError::AlreadyOpen {
                experiment_id,
                run_id,
            } => write!(f, "experiment {experiment_id} already open as {run_id}"),
            ExperimentError::Unresolvable { detail } => write!(f, "unresolvable: {detail}"),
            ExperimentError::Store { detail } => write!(f, "lab docs: {detail}"),
        }
    }
}

impl std::error::Error for ExperimentError {}

impl From<LedgerError> for ExperimentError {
    fn from(e: LedgerError) -> ExperimentError {
        ExperimentError::Ledger(e)
    }
}

impl From<BudgetError> for ExperimentError {
    fn from(e: BudgetError) -> ExperimentError {
        ExperimentError::Budget(e)
    }
}

impl From<ExpandError> for ExperimentError {
    fn from(e: ExpandError) -> ExperimentError {
        ExperimentError::Expand(e)
    }
}

impl From<ExperimentRefusal> for ExperimentError {
    fn from(e: ExperimentRefusal) -> ExperimentError {
        ExperimentError::Refusal(e)
    }
}
