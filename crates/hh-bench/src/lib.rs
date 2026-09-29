//! `hh-bench` — the benchmark-adapter contract (R-2.9.4⁰ᵇ; ADR-0142/0143/
//! 0144; spec §5h.4). Integrates external suites, never authors them
//! ("integrate, never author").

pub mod adapter;
pub mod adapters;
pub mod benchset;
pub mod env;
pub mod foreign;
pub mod grade;
pub mod infra;
pub mod parity;
pub mod records;

pub use adapter::{
    AdapterDeclaration, AdapterError, AdapterOp, BenchmarkAdapter, ExportFormat, HostingSurface,
};
pub use foreign::{
    export, import_result, ForeignArtifact, ForeignError, ImportedResult, LoweringLossReport,
};
pub use grade::{grade, parse_typed_reward, GradeError, GradeRequest, GradeResult};
pub use infra::{detect_infrastructure_failure, InfraClass, InfraReport};
pub use parity::parity_report;
pub use records::{BenchTask, EnvironmentHandle, ExposedTask, Submission, Surface};
