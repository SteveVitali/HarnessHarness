//! `hh-compact-evict-oldest` — the packaged first-party
//! `compaction_strategy` variant `evict_oldest` (C0; spec §5c R-2.4.2,
//! §8.4; ADR-0075/0146; ticket S2.2, corpus leg R2.5/DF-S2.8-1f). The
//! deterministic, model-call-free, budget/cap-aware `{assess, propose,
//! execute, declare}` contract over the closed op sum `{Evict, Offload,
//! Summarize, Restructure}` — `evict_oldest` emits `Evict` only.
//!
//! The library half is the contract implementation (`compact`); the
//! binary (`main.rs`) is a thin `plugin_abi/1` adapter over it —
//! binaries carry no semantics (the workspace's thin-binary rule). The
//! library exists so the R2.5 differential conformance corpus
//! (`tests/r2_5_differential.rs`) runs the *same* code the packaged
//! variant serves — never a second spelling of the strategy (CC1).

pub mod compact;
