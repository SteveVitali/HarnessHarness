//! `hh-analysis` — the C0/Stage-3 estimator kernel (spec §6.4,
//! `R-2.10.4⁰ᵇ`; ticket S3.4c; ADR-0157/0158/0159/0160).
//!
//! The kernel is the pure, versioned, class-aware function
//! `analyze(spec: AnalysisSpec, rows @ watermark_set) → AnalysisReport`
//! over *results rows*: the store-backed half of the §6.4 contract. The
//! pure records-in/records-out computation stays in `hh-eval`
//! (`lab.eval.*`); this crate is the `lab.analysis.*` side — it projects
//! `hh_results::ResultsRow`s (+ their `RunManifest`s and `LedgerFacts`)
//! into `hh_eval::EvalRun`s and drives the one estimator implementation
//! (CC1 — no second compare/stats/benefit code path exists here).
//!
//! Landed operation set (C0/Stage-3):
//!
//! - **A1 `summarize`** — point + interval under the ADR-0158 selection
//!   rule (floors `T_clt = 100`/`T_bca = 30`, labelled substitutions),
//!   per-task distribution + `(c,n)`, pass^k, tails, per-task
//!   consistency, outcome counts, typed `n/a`, vetoed runs excluded from
//!   the headline and counted beside;
//! - **A2 `compare`** — paired by task (by replicate only when
//!   `seed_honoured`), equal `eval_budget` or `UnmatchedBudget` /
//!   `IncommensurableMatch` / `MissingMatchSpec`, `budget_match.status`,
//!   `sign_profile`, `tail_effects`, `outcome_bounds`, `multiplicity`,
//!   `label`;
//! - **A3 `contrast`** — the default difference-of-differences over fully
//!   crossed probed cells, task-clustered resampling, `ContrastUndefined`
//!   typed refusal, never zero-filled;
//! - **A8 `equivalence`** — the three-valued TOST verdict under
//!   pre-registration margins (`MarginNotPreRegistered` on caller
//!   margins);
//! - **A12 `multiplicity`** — Holm over the pre-registered primary
//!   family, Benjamini–Hochberg q-values over the rest, `exploratory`
//!   labels where no pre-registration matches — nothing suppressed;
//! - **transfer rows** — `benefit_kind = transfer` comparisons over
//!   held-out levels with `transfer_ratio` and sign stability.
//!
//! Reports are `analysis_report_body/1` documents; `AnalysisRecord`s are
//! persisted through LabDocs (`record_analysis` is the §6.5 producer op —
//! the kernel deposits the record + body idempotently; a recorded report
//! is served, never silently recomputed — AC-R-2.10.4-12).

pub mod engine;
pub mod error;
pub mod kernel;
pub mod multiplicity;
pub mod project;
pub mod report;

pub use engine::analyze_and_record;
pub use error::AnalysisError;
pub use kernel::{analyze, AnalysisInput};
pub use project::eval_run;
pub use report::AnalysisOutcome;
