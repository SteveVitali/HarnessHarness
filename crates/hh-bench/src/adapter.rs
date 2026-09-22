//! The `benchmark_adapter` contract (R-2.9.4⁰ᵇ; spec §5h.4 §2; ADR-0142).
//!
//! An adapter **integrates** a suite — it never authors one. The contract
//! ops are `declare | discover | import_task | materialize | expose |
//! collect_submission | grade | export | parity`; the adapter runs **out of
//! process** (the `hh-bench-adapter` binary is the in-tree reference
//! transport: canonical-JSON request on stdin, canonical-JSON response on
//! stdout).
//!
//! Invariants (§5h.4 §5/§9):
//! - the held-out surface is never delivered to the participant — `expose`
//!   returns `ExposedTask` (visible-only); only `grade` reads `held_out`;
//! - the participant cannot invoke `grade`/`collect_submission` — the op
//!   surface the participant sees is `materialize`/`expose`-bound handles;
//!   instrument ops are routed by the harness, never exposed;
//! - `grade` never consumes model claims — it reads `Submission` +
//!   `held_out`;
//! - adapters are profile-blind except at `expose` (the bound profile is
//!   recorded on `ExposedTask.profile_ref`);
//! - verifier isolation is per-family (`separate` is the default where a
//!   submission exists); `shared` only where the family record declares it;
//! - every failure is typed (`AdapterError`), never a warning.

use std::collections::BTreeMap;

use hh_ontology::lab::{EnvironmentFamily, HandleCapability, Support};
use hh_wire::Json;

use crate::records::{EnvironmentHandle, ExposedTask, Submission};

/// The `benchmark_adapter` ops (§5h.4 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AdapterOp {
    /// Declare the suite identity the adapter serves.
    Declare,
    /// Discover the suite's tasks.
    Discover,
    /// Import one foreign task into a `TaskRecord`.
    ImportTask,
    /// Materialize the task's environment.
    Materialize,
    /// Expose the task's visible surface to a participant profile.
    Expose,
    /// Collect the participant's terminal submission (instrument-plane).
    CollectSubmission,
    /// Grade a submission against the held-out surface (instrument-plane).
    Grade,
    /// Export the replayable artifact set (parity input).
    Export,
    /// Produce the parity report against an original runner's runs.
    Parity,
}

impl AdapterOp {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AdapterOp::Declare => "declare",
            AdapterOp::Discover => "discover",
            AdapterOp::ImportTask => "import_task",
            AdapterOp::Materialize => "materialize",
            AdapterOp::Expose => "expose",
            AdapterOp::CollectSubmission => "collect_submission",
            AdapterOp::Grade => "grade",
            AdapterOp::Export => "export",
            AdapterOp::Parity => "parity",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<AdapterOp> {
        [
            AdapterOp::Declare,
            AdapterOp::Discover,
            AdapterOp::ImportTask,
            AdapterOp::Materialize,
            AdapterOp::Expose,
            AdapterOp::CollectSubmission,
            AdapterOp::Grade,
            AdapterOp::Export,
            AdapterOp::Parity,
        ]
        .into_iter()
        .find(|o| o.as_str() == s)
    }

    /// Whether the op is instrument-plane — a participant may never invoke
    /// these (the harness routes them; exposing them to a participant is the
    /// `benchmark_egress`-adjacent violation class).
    pub fn instrument_plane(self) -> bool {
        matches!(self, AdapterOp::CollectSubmission | AdapterOp::Grade)
    }
}

/// The adapter contract's typed failures (§5h.4 §9 — refusal, never a
/// warning).
#[derive(Debug, Clone, PartialEq)]
pub enum AdapterError {
    /// The op is unknown to this adapter.
    UnknownOp(String),
    /// The request body failed schema checks.
    MalformedRequest(String),
    /// The task id is not in this adapter's suite.
    UnknownTask(String),
    /// The environment could not be materialized (the image ref/digest did
    /// not resolve, the root is unwritable, …) — an environment-tier
    /// failure, classified by the infra detector.
    EnvironmentUnusable(String),
    /// `expose` was asked to deliver a held-out member — refused outright
    /// (the held-out surface never crosses).
    HeldOutBoundaryViolation(String),
    /// The submission could not be applied — recorded; the *grade* of a
    /// failed apply is a scored failure, this refusal is the collect side.
    SubmissionApplyFailed(String),
    /// A participant-facing caller invoked an instrument-plane op.
    InstrumentOpFromParticipant(AdapterOp),
    /// The adapter's suite manifest failed `SuiteManifest::validate`.
    SuiteInvalid(String),
    /// `materialize` refused: the family's `requires` names a `required`
    /// handle capability the environment declares `unsupported` or leaves
    /// undeclared (`unknown`) — AC-R-2.9.4-12; the `missing` member lists
    /// the capability names, never a warning.
    FamilyUnsupported {
        /// The capabilities the handle does not surface.
        missing: Vec<String>,
    },
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdapterError::UnknownOp(o) => write!(f, "UnknownOp({o})"),
            AdapterError::MalformedRequest(m) => write!(f, "MalformedRequest: {m}"),
            AdapterError::UnknownTask(t) => write!(f, "UnknownTask({t})"),
            AdapterError::EnvironmentUnusable(m) => {
                write!(f, "EnvironmentUnusable: {m}")
            }
            AdapterError::HeldOutBoundaryViolation(m) => {
                write!(f, "HeldOutBoundaryViolation: {m}")
            }
            AdapterError::SubmissionApplyFailed(m) => {
                write!(f, "SubmissionApplyFailed: {m}")
            }
            AdapterError::InstrumentOpFromParticipant(o) => {
                write!(f, "InstrumentOpFromParticipant({})", o.as_str())
            }
            AdapterError::SuiteInvalid(m) => write!(f, "SuiteInvalid: {m}"),
            AdapterError::FamilyUnsupported { missing } => {
                write!(f, "FamilyUnsupported{{missing: {missing:?}}}")
            }
        }
    }
}

impl std::error::Error for AdapterError {}

/// `check_handle_requirements(requires, declared)` — the AC-R-2.9.4-12
/// `materialize` gate: every `required` member of the family's
/// `HandleRequirements` must be `supported` on the environment handle;
/// `unsupported` or absent (`unknown`) declarations refuse with
/// `FamilyUnsupported{missing}` listing the capability names. A
/// `supported`-level requirement is advisory and never refuses.
pub fn check_handle_requirements(
    requires: &BTreeMap<HandleCapability, Support>,
    declared: &BTreeMap<HandleCapability, Support>,
) -> Result<(), AdapterError> {
    let missing: Vec<String> = requires
        .iter()
        .filter(|(cap, need)| {
            **need == Support::Required && declared.get(*cap) != Some(&Support::Supported)
        })
        .map(|(cap, _)| cap.name().to_string())
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(AdapterError::FamilyUnsupported { missing })
    }
}

/// Whether `image_digest` is a pinned digest (`sha256:<64-hex>` or a bare
/// 64-hex content digest). Anything else — a tag, an empty string, a
/// foreign version string — is a *claim*, and the task lands in the
/// adapter's `unpinned[]` (AC-R-2.9.4-6; ADR-0142 D2: foreign version
/// strings only ever ride `version_label`/`foreign_digest` claims).
pub fn is_pinned_image_digest(digest: &str) -> bool {
    let hexpart = digest.strip_prefix("sha256:").unwrap_or(digest);
    hexpart.len() == 64
        && hexpart
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// The `benchmark_adapter` trait — every adapter (fixture or real) is a
/// `dispatch(op, request_json) → response_json` server behind the same op
/// surface. The harness drives it out of process (`hh-bench-adapter`); the
/// in-process form exists only for tests and conformance runs.
pub trait BenchmarkAdapter {
    /// The adapter id (`adapter_a`, `adapter_c`, `adapter_d`, `adapter_e`, …).
    fn adapter_id(&self) -> &'static str;

    /// The environment family the adapter serves.
    fn family(&self) -> EnvironmentFamily;

    /// The suite's task ids (discover).
    fn task_ids(&self) -> Vec<String>;

    /// The imported task record (`import_task` output — the schema half).
    fn task(&self, task_id: &str) -> Result<crate::records::BenchTask, AdapterError>;

    /// `materialize` — prepare the task's environment; returns the handle.
    /// `participant = false` materializes the verifier surface.
    fn materialize(
        &self,
        task_id: &str,
        root: &str,
        verifier: bool,
    ) -> Result<EnvironmentHandle, AdapterError>;

    /// `expose` — bind the visible surface to a participant profile. The
    /// only profile-aware op.
    fn expose(
        &self,
        env: &EnvironmentHandle,
        profile_ref: &str,
    ) -> Result<ExposedTask, AdapterError>;

    /// `collect_submission` — instrument-plane; read the participant's
    /// terminal artifact from the environment.
    fn collect_submission(&self, env: &EnvironmentHandle) -> Result<Submission, AdapterError>;

    /// `grade` — instrument-plane; consume `Submission` + the task's
    /// `held_out` surface only (never model claims).
    fn grade(
        &self,
        req: &crate::grade::GradeRequest,
    ) -> Result<crate::grade::GradeResult, crate::grade::GradeError>;

    /// The suite's `unpinned[]` — tasks importable only by tag (no pinned
    /// `image_digest`); AC-R-2.9.4-6. A task here caps the suite's
    /// admissible `claimed_level` at `{R0, R1, R3}` — R2 requires the
    /// image digest. The default implementation scans `task_ids`; adapters
    /// with a cheaper manifest path may override.
    fn unpinned_tasks(&self) -> Vec<String> {
        self.task_ids()
            .into_iter()
            .filter(|id| {
                self.task(id)
                    .map(|t| !is_pinned_image_digest(&t.environment.image_digest))
                    .unwrap_or(false)
            })
            .collect()
    }

    /// The `claimed_level` set the suite admits (AC-R-2.9.4-6): all four
    /// when every task is pinned; `{R0, R1, R3}` otherwise.
    fn claimed_levels(&self) -> Vec<&'static str> {
        if self.unpinned_tasks().is_empty() {
            vec!["R0", "R1", "R2", "R3"]
        } else {
            vec!["R0", "R1", "R3"]
        }
    }

    /// `export` — emit the replayable artifact set (the parity input):
    /// canonical JSON listing the task, environment handle, submission, and
    /// verdict.
    fn export(&self, submission: &Submission, verdict: &Json) -> Json {
        Json::obj([
            ("schema", Json::str("adapter_export/1")),
            ("adapter", Json::str(self.adapter_id())),
            ("submission", submission.to_json()),
            ("verdict", verdict.clone()),
        ])
    }
}
