//! `hharness-eval` — the Stage-3 evaluation kernel (spec §5h; R-2.9.2; S3.3).
//!
//! Modules:
//! - [`catalogue`] — the metric/oracle catalogue as first-class
//!   `MetricDeclaration`/`OracleDeclaration` rows (the typed `n/a` algebra
//!   lives in `hharness-ontology`; the catalogue validates, never invents);
//! - [`stats`] — the deterministic estimator layer (quantiles, Wilson, CLT,
//!   clustered-CLT, paired bootstrap, Bayesian-beta, pass^k/pass@k);
//! - [`runs`]/[`facts`] — the canonical `EvalRun` view and the ledger →
//!   facts projection;
//! - [`task_selection`] — the `task-selection/1` materializer (seeded
//!   declared-field split/draw, TB 2.1 pin, outcome refusals; R6.18);
//! - [`comparative_protocol`]/[`comparative_quote`] — the
//!   `comparative_protocol/1` materializer and `comparative-quote/1`
//!   artifact (H ≥ 2 arms, declared T and k, the schedule-randomization
//!   record, the single phase-2–4 quote; R6.18);
//! - [`comparative_estimators`] — the §6.4 admission-map apparatus
//!   (single-arm FM rate, flips, $/solve, tied-with-leader, the
//!   per-tranche Holm family, `hh-comparative-results/1`), each piece
//!   not-admitted for confirmatory/first-pass use until GATE-G23
//!   (R6.19; REQ-XD-023);
//! - [`vetoes`], [`compliance`], [`opacity`], [`faults`] — the metric
//!   families over facts;
//! - [`mod@compare`]/[`benefits`]/[`scorecard`] — `compare`, the three benefit
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
pub mod central_protocol;
pub mod comparative_battery;
pub mod comparative_estimators;
pub mod comparative_protocol;
pub mod comparative_quote;
pub mod compare;
pub mod compliance;
pub mod context_metrics;
pub mod critic_experiment;
pub mod directional_kl;
pub mod exposure_quote;
pub mod facts;
pub mod faults;
pub mod finite_manifest;
pub mod hosted_metrics;
pub mod interventions;
pub mod judge;
pub mod loss;
pub mod mechanism;
pub mod opacity;
pub mod oracle;
pub mod parity_admission;
pub mod population;
pub mod runs;
pub mod scorecard;
pub mod security_metrics;
pub mod selection;
pub mod sim_battery;
pub mod stats;
pub mod task_mixture;
pub mod task_selection;
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
pub use judge::{
    admit_judge, emit_judged, judge_context_for, recheck_judged, JudgeAdmission, JudgeContext,
    JudgeError,
};
pub use loss::{export_bundle_losses, export_loss_report};
pub use opacity::{is_typed_kind, opacity_dynamic, per_call_opacity};
pub use oracle::{run_oracle, OracleFailure, OracleRequest, OracleVerdict};
pub use runs::{EvalRun, SuiteContext, TaskContext};
pub use scorecard::{render_scorecard, ScorecardError, ScorecardInput, T_BCA, T_CLT};
pub use vetoes::{evaluate_vetoes, tripped_veto_ids, veto_id, VetoContext, VetoTrip};
