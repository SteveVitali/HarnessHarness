//! The estimator kernel's typed refusal set (never warnings — CC9 /
//! T-LCD-14; §6.4 §2.1–§2.2).

use hh_eval::benefits::BenefitError;
use hh_eval::compare::CompareError;
use hh_experiment::errors::ExperimentError;
use hh_results::error::ResultsError;

/// `AnalysisError` — the closed refusal set the kernel and its engine
/// return. Every variant is a typed fact; nothing is a warning.
#[derive(Debug)]
pub enum AnalysisError {
    /// The `AnalysisSpec.kind` is not in the C0/Stage-3 set
    /// (`summarize|compare|interaction|transfer|equivalence`) — a typed
    /// refusal, not a silent no-op.
    KindNotImplemented {
        /// The requested kind.
        kind: String,
    },
    /// A kind that needs a design/pre-registration was asked for without
    /// `spec_ref` (or the doc did not resolve).
    MissingSpecRef {
        /// The kind that needed it.
        kind: String,
    },
    /// The selected row carries no task coordinate — the task is the
    /// cluster/pairing/resampling unit; a row without one cannot enter a
    /// task-keyed analysis (ADR-0158).
    MissingClusterKey {
        /// The offending row's run.
        run_id: String,
    },
    /// A selected row is still `open` (no terminal outcome) — the
    /// projection cannot derive an `OutcomeClass` for it.
    RowNotTerminal {
        /// The offending row's run.
        run_id: String,
    },
    /// A row field refused decode (`coordinates`/`experiment`/cell value).
    RowField {
        /// The row's run.
        run_id: String,
        /// The field.
        field: String,
        /// The detail.
        detail: String,
    },
    /// A named metric is absent from the resolved declarations.
    UnknownMetric {
        /// The metric name.
        name: String,
    },
    /// `hh_eval::compare` refused (match/comparability/contrast typed
    /// refusals pass through verbatim — one refusal vocabulary).
    Compare(CompareError),
    /// `hh_eval::benefits` refused (`MarginNotPreRegistered`,
    /// `BadTransferFactor`, `UnmatchedSearchBudget`, …).
    Benefit(BenefitError),
    /// The results store refused (query/watermark/store errors).
    Results(ResultsError),
    /// LabDocs/ExperimentStore refused (spec/plan/budget doc resolution,
    /// record deposit).
    Experiment(ExperimentError),
    /// A spec member refused decode (`filters` shape, bad arm refs).
    BadSpec {
        /// The member.
        member: String,
        /// The detail.
        detail: String,
    },
    /// A3/A10 on a hosted row was asked for a component-level factor
    /// (§2.5 class-applicability; AC-R-2.10.4-4) — a typed refusal, the
    /// factor is never silently dropped or its level fabricated.
    InadmissibleFactor {
        /// The factor.
        factor: String,
        /// The run that cannot carry it.
        run_id: String,
        /// The detail.
        detail: String,
    },
}

impl std::fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnalysisError::KindNotImplemented { kind } => {
                write!(f, "analysis kind not implemented at C0/Stage-3: {kind}")
            }
            AnalysisError::MissingSpecRef { kind } => {
                write!(f, "kind {kind} requires a resolvable spec_ref")
            }
            AnalysisError::MissingClusterKey { run_id } => {
                write!(
                    f,
                    "row {run_id} carries no task coordinate (MissingClusterKey)"
                )
            }
            AnalysisError::RowNotTerminal { run_id } => {
                write!(f, "row {run_id} is not terminal (open)")
            }
            AnalysisError::RowField {
                run_id,
                field,
                detail,
            } => write!(f, "row {run_id}: bad field {field}: {detail}"),
            AnalysisError::UnknownMetric { name } => write!(f, "unknown metric: {name}"),
            AnalysisError::Compare(e) => write!(f, "{e}"),
            AnalysisError::Benefit(e) => write!(f, "{e}"),
            AnalysisError::Results(e) => write!(f, "results: {e}"),
            AnalysisError::Experiment(e) => write!(f, "experiment: {e}"),
            AnalysisError::BadSpec { member, detail } => {
                write!(f, "bad spec member {member}: {detail}")
            }
            AnalysisError::InadmissibleFactor {
                factor,
                run_id,
                detail,
            } => write!(
                f,
                "factor {factor} is inadmissible for run {run_id}: {detail}"
            ),
        }
    }
}

impl std::error::Error for AnalysisError {}

impl From<CompareError> for AnalysisError {
    fn from(e: CompareError) -> AnalysisError {
        AnalysisError::Compare(e)
    }
}

impl From<BenefitError> for AnalysisError {
    fn from(e: BenefitError) -> AnalysisError {
        AnalysisError::Benefit(e)
    }
}

impl From<ResultsError> for AnalysisError {
    fn from(e: ResultsError) -> AnalysisError {
        AnalysisError::Results(e)
    }
}

impl From<ExperimentError> for AnalysisError {
    fn from(e: ExperimentError) -> AnalysisError {
        AnalysisError::Experiment(e)
    }
}
