//! `hharness-runner` — the committed experiment runner (experiments
//! supplement §3.2/§3.3; manifest 261/R6.8; ADR-0427/AR-04).
//!
//! ```text
//! hh-runner
//!   coordinator process ── sole writer of the experiment store root;
//!       drives hh-experiment::ExperimentEngine (next/claim/launch/settle,
//!       the spec's ReattemptPolicy) — no scheduler, retry counter or
//!       authoritative store of its own (REQ-XB-102, CC1, K-2)
//!   └── slot worker ×k ── one process per in-flight slot attempt; sole
//!       writer of its own slot store root; runs §3.3 ST0–ST7 and emits
//!       one hh-experiment-slot/1 record per slot (REQ-XB-104/108)
//! ```
//!
//! Resume and recovery derive only from durable records (REQ-XB-105): a
//! launched-but-unsettled attempt binds its worker's sealed record, or
//! closes `inconclusive{infra:runner_crash}` and replans within bounds — a
//! paid agent phase is never resumed mid-run. The §3.3 slot-fact →
//! outcome-class/disposition map lives in [`facts`]; the closed
//! `withheld{…}` member set lives in [`disposition`]. The crate depends
//! only downward and adds no edge into hh-embed, hh-cli or hharness-hosting
//! (REQ-XB-103).

pub mod json_ext;

pub mod admission;
pub mod admission_build;
pub mod answer_scan;
pub mod archive_upload;
pub mod capture;
pub mod carried;
pub mod cell;
pub mod condenser;
pub mod continuation;
pub mod coordinator;
pub mod diskwatch;
pub mod disposition;
pub mod facts;
pub mod fp_seal;
pub mod harbor;
pub mod holds;
pub mod l1_corpus;
pub mod l1_drive;
pub mod l1_gate;
pub mod l1_obs;
pub mod packet;
pub mod probe;
pub mod ps0;
pub mod receipt_index;
pub mod render;
pub mod reproof;
pub mod runner_check;
pub mod schedule;
pub mod slot_costs;
pub mod slot_record;
pub mod smoke;
pub mod supervision;
pub mod verdict;
pub mod worker;
