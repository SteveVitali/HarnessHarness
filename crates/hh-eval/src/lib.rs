//! `hh-eval` — the Stage-3 evaluation kernel (spec §5h; R-2.9.2; S3.3).
//!
//! Modules:
//! - [`catalogue`] — the metric/oracle catalogue as first-class
//!   `MetricDeclaration`/`OracleDeclaration` rows (the typed `n/a` algebra
//!   lives in `hh-ontology`; the catalogue validates, never invents);
//! - [`stats`] — the deterministic estimator layer (quantiles, Wilson, CLT,
//!   clustered-CLT, paired bootstrap, Bayesian-beta, pass^k/pass@k);
//! - [`runs`]/[`facts`] — the canonical `EvalRun` view and the ledger →
//!   facts projection;
//! - [`vetoes`], [`compliance`], [`opacity`], [`faults`] — the metric
//!   families over facts;
//! - [`compare`]/[`benefits`]/[`scorecard`] — `compare`, the three benefit
//!   kinds, `equivalence_run`, and `render_scorecard`;
//! - [`oracle`] — the deterministic-oracle boundary shared by in-process
//!   callers and the `hh-eval-oracle` subprocess (AC-R-2.9.2-11).
//!
//! Refusal semantics (T-LCD-14): every invalid precondition returns a typed
//! `Refusal`, never a warning; `n/a` values are typed (`MetricValueKind::Na`)
//! and never coerced to 0.

mod json_util;

pub mod accounting;
pub mod benefits;
pub mod catalogue;
pub mod compare;
pub mod compliance;
pub mod facts;
pub mod faults;
pub mod loss;
pub mod opacity;
pub mod oracle;
pub mod runs;
pub mod scorecard;
pub mod stats;
pub mod vetoes;

pub use benefits::{
    artifact_benefit, equivalence_run, search_time_benefit, split_hash_agrees, transfer,
    BenefitError, DimensionVerdict, EquivalenceReport, EquivalenceVerdict, RegisteredMargin,
};
pub use catalogue::{
    check_catalogue, metric, oracle_declarations, scorecard_metrics, CatalogueFinding,
    CatalogueReport, C0_VETO_IDS,
};
pub use compare::{compare, CompareError, CompareInput, CompareOutcome, TaskEffect};
pub use compliance::{
    activation_rate, attribution_completeness, delivery_rate, detector_conformity, follow_rate,
    intervention_rate, names, RateParts,
};
pub use facts::LedgerFacts;
pub use faults::{
    stage3_fault_profiles, stage3_perturbation_profiles, FaultProfile, FaultSpec, FaultType,
    PerturbationKind, PerturbationProfile, PerturbationSpec,
};
pub use loss::{export_bundle_losses, export_loss_report};
pub use opacity::{is_typed_kind, opacity_dynamic, per_call_opacity};
pub use oracle::{run_oracle, OracleFailure, OracleRequest, OracleVerdict};
pub use runs::{EvalRun, SuiteContext, TaskContext};
pub use scorecard::{render_scorecard, ScorecardError, ScorecardInput, T_BCA, T_CLT};
pub use vetoes::{evaluate_vetoes, veto_id, VetoContext, VetoTrip};
