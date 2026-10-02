//! `hh-debt` — the C4 §5h.6 assumption-debt manager service (R-2.9.6;
//! ticket S6.1b; ADR-0197/0198, ADR-0209 D2).
//!
//! The manager guarantees that no conditioned rule exists without a stated
//! deficiency hypothesis, gradeable evidence, a reachable owner, an
//! observable expiry and an executable removal test — and keeps that true
//! over time *by a service*: a standing monitor (the sweep) folds the
//! durable `lifecycle.debt.*` prefix of the service's registry run, opens
//! ledgered probation entries for `hypothesized` records, evaluates the
//! all-home triggers through `hh_lab::debt::evaluate_debt`, schedules
//! removal tests under `DebtPolicy` (priority, `max_open_removal_tests`,
//! instrument-budget reservation), settles verdicts, proposes retirements,
//! and runs its own reflexive `no_dead_weight_found` test — if no scheduled
//! removal test ever passes across the window, the manager is dead weight
//! and `retired` is its honest end state (ADR-0197 D10).
//!
//! D-2 holds structurally: the manager never opens a run and never applies
//! a diff to a subject — it authors `ExperimentSpec`s and `HirDiff`
//! *proposals*, mints `lifecycle.debt.*` rows on the caller-supplied run
//! (the registry audit run — the S5.4 convention), and reads reports.
//! Records-in/records-out (AC-R-2.9.6-10): every document moves through
//! `hh-embed/1`; there is no private verb and no second store.
//!
//! Modules:
//! - [`records`] — the service record and sweep types;
//! - [`view`] — the durable-prefix fold;
//! - [`schedule`] — `schedule_removal_test` + the `retirement_batch` design
//!   builder;
//! - [`reflexive`] — the home-16 record and the `no_dead_weight_found`
//!   evaluation;
//! - [`propose`] — `propose_retirement` + the proposal/deployment gate;
//! - [`manager`] — the `DebtManager` driver
//!   (`open`/`sweep`/`settle`/`retire`/`propose`);
//! - [`errors`] — the closed refusal table.

pub mod errors;
pub mod manager;
pub mod propose;
pub mod records;
pub mod reflexive;
pub mod schedule;
pub mod view;
