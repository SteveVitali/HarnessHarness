//! `hh-baseline` — the **THROWAWAY Stage-0 de-risking baseline** (doc 3 §11.0; ticket S0.2).
//!
//! A hand-authored `react/minimal` definition runs one headless coding task end-to-end through
//! a deliberately minimal C0 subset:
//! - hand-stamped provenance/identity ([`provenance`]) and a `validate`-only HIR/1 document
//!   ([`hir`]);
//! - one root [`budget::BudgetNode`] with hard ceilings and `check` at decision points;
//! - a **model-blind** [`gateway`] over one wire dialect with a **scripted offline stub** (never
//!   a live model) and one static [`gateway::ModelRoleTable`];
//! - one in-process [`tools`] executor for shell + fs under a `local_host` [`env::EnvHandle`]
//!   (`isolation_class = none`, recorded honestly);
//! - a deny-by-default allow-list + known-value masking + leak scan ([`secrets`]) instead of the
//!   reference monitor;
//! - a single-file [`trace`] (M1/M3/M7) instead of the durable run ledger;
//! - the `default` six-slot [`context`] policy;
//! - the `react/minimal` [`control`] loop with the driver-subset envelope;
//! - and the headless [`driver`] CLI (`run start/events/status`, `definition validate`,
//!   `version`, `doctor`, `hello`).
//!
//! **This crate is deleted at the Stage-0 boundary.** It is a workspace member only so its
//! acceptance tests run under `cargo test --workspace`; nothing persistent depends on it. It
//! reuses S0.1's `hh-wire` (canonical JSON, framing) and `hh-embed-schema` (the single schema
//! source's `hello`/`ContractIdentity`) — never re-establishing them (CC7). Offline and
//! hermetic (pure std beyond those two crates).

pub mod budget;
pub mod context;
pub mod control;
pub mod driver;
pub mod env;
pub mod gateway;
pub mod hir;
pub mod provenance;
pub mod secrets;
pub mod tools;
pub mod trace;
