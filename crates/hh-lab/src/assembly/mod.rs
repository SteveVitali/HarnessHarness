//! `assembly` — the assembly service (§6.1; R-2.10.1; ADR-0147/0148/0149/0150):
//! `AssemblySource` + D1–D5 desugar over the existing `hh-assembly` kernel
//! (`compose → resolve(snapshot) → validate_assembly → seal`). The service is
//! a semantics-free orchestration layer: every sealable judgement stays in
//! the kernel; this crate only records sources, desugars them, sequences the
//! kernel calls, and renders plans/diffs/diagnostics.

pub mod desugar;
pub mod diff_view;
pub mod drift;
pub mod plan;
pub mod service;
pub mod source;
