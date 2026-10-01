//! `hh-experiment` — the C0/Stage-3 single-worker experiment engine
//! (spec §6.3 §2.2–§2.5; R-2.10.3⁰ᵇ; ADR-0154/0155/0156/0162).
//!
//! - [`docs`] — `LabDocs`, the durable content-addressed document store
//!   (`hh-experiment/1` specs, `CellPlan`s, the `experiment_id → {plan_id,
//!   run_id}` index).
//! - [`events`] — the `measurement.experiment.*` payload builders and the
//!   boundary records (`RunOutcome`, `ExperimentReport`).
//! - [`view`] — `ExperimentView`, the pure fold over the experiment run's
//!   committed envelope stream (S-1: every scheduler view is
//!   `project(experiment_run, view, until_seq)` with rebuild equality).
//! - [`engine`] — `ExperimentEngine`, the lifecycle over the experiment run's
//!   fenced writer: `register`/`expand`/`open_experiment`/`next`/`claim`/
//!   `launch`/`settle`/`pause`/`resume`/`close`/`amend`/`restore`.
//! - [`errors`] — the typed failure sum; the closed E-1 refusal set renders
//!   verbatim, never a warning (T-LCD-14, CC9).

pub mod docs;
pub mod engine;
pub mod errors;
pub mod events;
pub mod view;
