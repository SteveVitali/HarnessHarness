//! `hh-lab` — the Lab's C0/Stage-1 schema plane (spec §6; ticket S1.24).
//!
//! Requirement slices: R-2.9.4⁰ᵃ (benchmark/environment records — §5h.4),
//! R-2.9.8⁰ (foreign-model claims — §5h.8), R-2.10.3⁰ᵃ (the experiment dialect
//! `hh-experiment/1` — §6.3), R-2.10.4⁰ᵃ (the analysis records — §6.4) and the
//! R-2.10.5⁰ row-key fields' record halves (§6.5/§9.2). Everything here is a
//! *record, closed vocabulary or deterministic pure function*: the engines,
//! samplers, estimators and schedulers land at Stage 3 (ADR-0209 D2).
//!
//! Module map:
//! - [`bench`] — benchmark/environment records: `EnvironmentFamilyRecord`,
//!   `TaskRecord`, `SuiteManifest`, `SuiteValidityRecord`,
//!   `SplitAssignmentRecord`, `ParityReport`, plus the import-side checks
//!   (disjoint `visible`/`held_out`/`instrument` surfaces,
//!   `HeldOutInEnvironment`, the `verifier_isolation` default rule).
//! - [`model`] — the foreign-model claim records: `ForeignRef` (a claim, never
//!   a `ContentAddress` — I-2), `SnapshotClaim`, `TrainingCutoffClaim`,
//!   `CompatibilityRecord`, `TrainedUnderRef`.
//! - [`experiment`] — `hh-experiment/1`: `ExperimentSpec`, `FactorSpec`,
//!   `LevelSpec`, `ArmSpec`, `SuiteBinding`, `SchedulingPolicy`,
//!   `ReattemptPolicy`, `ValidationStrategy`, `CellPlan`/`RunPlan`/`OrderPlan`,
//!   `run_plan_id`, and the closed `ExperimentRefusal` register set over a
//!   `SpecContext` view.
//! - [`expand`] — the pure `expand(spec) → CellPlan` (S3.4a): cells `arm ×
//!   task`, the two-level `l^(k−p)` generator algebra (defining subgroup,
//!   resolution, aliasing table), deterministic `seed_material`/
//!   `cache_scope_salt`, and the `OrderPlan`/`order_key` scheduling helpers.
//! - [`exemplars`] — the two canonical `ExperimentSpec` documents
//!   (ADR-0156 D1/D2) instantiated at Stage-3 size:
//!   `lab/compaction-family-v1` and `lab/control-strategy-family-v1`.
//! - [`analysis`] — `AnalysisSpec`, `AnalysisRecord`, `AnalysisReport`,
//!   `QuerySpec`, `WatermarkSet`, `CellRecord`, `RenderSpec`, and the amended
//!   `ComparisonReport` (ADR-0046 as amended).
//! - [`debt`] — the debt views: `DebtIndexRow`, `DebtReport`,
//!   `AssumptionDebtHealth`, `DebtNotice`.
//!
//! The plain closed vocabulary lives in `hh-ontology::lab` /
//! `hh-ontology::debt` (CC7 — the schema source owns the canonical forms); the
//! HIR-facing debt record lives in `hh-hir`; this crate composes them into the
//! Lab-facing records.

pub mod analysis;
pub mod assembly;
pub mod bench;
pub mod debt;
pub mod exemplars;
pub mod expand;
pub mod experiment;
pub mod json_util;
pub mod model;
