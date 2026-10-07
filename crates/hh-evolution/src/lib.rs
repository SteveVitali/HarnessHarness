//! `hh-evolution` — the C5 §05h evolution pipeline (R-2.9.5; ticket
//! S6.1a). See `campaign` for the driver, `state` for the closed S0–S10
//! machine, `records` for the typed bodies, `errors` for the closed
//! refusal table, `view` for the durable-prefix fold.

pub mod campaign;
pub mod errors;
pub mod proposer;
pub mod proposer_conformance;
pub mod records;
pub mod state;
pub mod view;
