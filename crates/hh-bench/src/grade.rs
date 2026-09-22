//! The `grade` pipeline (spec §5h.4 §6; AC-R-2.9.4-{3,5,6}; S3.3).
//!
//! `grade` is the only op that reads `held_out`. It consumes `Submission` +
//! the task's `held_out`/`instrument` surface — never a model claim — and
//! runs the task's grader in a **separate verifier** context (the family's
//! `verifier_isolation_default`; `shared` only where the family record
//! declares it — ADR-0143).
//!
//! Reward parsing is **typed**: the verifier's stdout must carry a
//! canonical-JSON `{reward: <int ppm>}` member; empty, non-numeric,
//! non-finite, or unparseable output is a typed `GradeError` → the run's
//! `outcome_class = oracle_failure` — never a 0 (AC-R-2.9.4-5). A failed
//! submission *apply* is a scored failure (0 ppm), never an oracle failure
//! (§5h.4 §6).

use hh_lab::bench::{GraderKind, TaskRecord};
use hh_ontology::eval::LatticeValue;
use hh_wire::Json;

use crate::infra::{detect_infrastructure_failure, InfraSignal};
use crate::records::Submission;

/// `parse_typed_reward`'s failures — every one maps to `oracle_failure`
/// upstream (AC-R-2.9.4-5).
#[derive(Debug, Clone, PartialEq)]
pub enum RewardParseError {
    /// The output was empty.
    Empty,
    /// No `reward` member.
    Missing,
    /// The member was non-numeric or non-finite (canonical JSON has no
    /// floats; the member must be `Json::Int` ppm).
    NonNumeric,
    /// The reward was outside `[0, PPM]`.
    OutOfRange,
    /// The output was not canonical JSON at all.
    Unparseable(String),
}

impl std::fmt::Display for RewardParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RewardParseError::Empty => write!(f, "empty verifier output"),
            RewardParseError::Missing => write!(f, "no `reward` member"),
            RewardParseError::NonNumeric => write!(f, "reward non-numeric/non-finite"),
            RewardParseError::OutOfRange => write!(f, "reward outside [0, 1e6] ppm"),
            RewardParseError::Unparseable(e) => write!(f, "unparseable verifier output: {e}"),
        }
    }
}

/// `parse_typed_reward(stdout) → ppm ∈ [0, 1e6]` — strict typed parsing
/// (AC-R-2.9.4-5). The verifier emits canonical JSON `{"reward": <ppm>}` —
/// a bare scalar, a float, an empty stream, or trailing garbage all fail.
pub fn parse_typed_reward(stdout: &str) -> Result<i64, RewardParseError> {
    if stdout.trim().is_empty() {
        return Err(RewardParseError::Empty);
    }
    let j = hh_wire::parse(stdout).map_err(|e| RewardParseError::Unparseable(e.to_string()))?;
    let reward = j
        .get("reward")
        .ok_or(RewardParseError::Missing)?
        .as_int()
        .ok_or(RewardParseError::NonNumeric)?;
    if !(0..=1_000_000).contains(&reward) {
        return Err(RewardParseError::OutOfRange);
    }
    Ok(reward)
}

/// `grade`'s typed failures (a failed `grade` is `oracle_failure`, never a
/// score).
#[derive(Debug, Clone, PartialEq)]
pub enum GradeError {
    /// The verifier output failed typed reward parsing.
    BadReward(RewardParseError),
    /// The verifier process itself failed (crash/timeout/refusal —
    /// `oracle_failure`).
    VerifierFailed(String),
    /// The submission never applied — the *grade* is a scored 0; this
    /// variant is for a `collect_submission`-side refusal only.
    SubmissionNeverApplied,
    /// The grader's declared kind is not runnable at Stage 3 (composite
    /// graders with a judged leg are C2+).
    GraderUnavailable(String),
    /// The grader log contradicts the recorded evidence
    /// (`grader_log_inconsistent` veto input — the comparison refuses, the
    /// veto trips upstream).
    GraderLogInconsistent(String),
}

impl std::fmt::Display for GradeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GradeError::BadReward(e) => write!(f, "BadReward({e})"),
            GradeError::VerifierFailed(m) => write!(f, "VerifierFailed: {m}"),
            GradeError::SubmissionNeverApplied => write!(f, "SubmissionNeverApplied"),
            GradeError::GraderUnavailable(m) => write!(f, "GraderUnavailable: {m}"),
            GradeError::GraderLogInconsistent(m) => write!(f, "GraderLogInconsistent: {m}"),
        }
    }
}

impl std::error::Error for GradeError {}

/// `grade_request/1` — the grade input: the submission, the task, and the
/// verifier's declared isolation.
#[derive(Debug, Clone)]
pub struct GradeRequest {
    /// The submission under test.
    pub submission: Submission,
    /// The task (grade reads `held_out` + `instrument` — the only op that
    /// may).
    pub task: TaskRecord,
    /// The verifier isolation mode the family/task resolves to
    /// (`separate` default; `shared` only where declared).
    pub isolation: hh_ontology::lab::VerifierIsolation,
}

/// `grade_result/1` — the grade output: a typed reward plus the infra
/// classification and the verifier's evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct GradeResult {
    /// The reward in ppm of the task's full score.
    pub reward_ppm: i64,
    /// The lattice verdict the reward maps to (`N` ≥ 1e6, `C` = 0,
    /// `P` between — a partial credit is `P`, never rounded to a binary).
    pub verdict: LatticeValue,
    /// The infra classification of the grade run.
    pub infra: crate::infra::InfraReport,
    /// Whether the verifier ran in a separate context (`false` ⇒ the
    /// `shared_verifier_contamination` veto reads this).
    pub separate_verifier: bool,
    /// The grader's evidence payload (a small canonical-JSON object — the
    /// ledger records its digest).
    pub grader_log: Json,
}

impl GradeResult {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("grade_result/1")),
            ("reward_ppm", Json::Int(self.reward_ppm)),
            ("verdict", Json::str(self.verdict.as_str())),
            ("infra", self.infra.to_json()),
            ("separate_verifier", Json::Bool(self.separate_verifier)),
            ("grader_log", self.grader_log.clone()),
        ])
    }
}

/// `grade(request, verifier_stdout, signal)` — the deterministic grade
/// pipeline. `verifier_stdout` is the separate verifier's raw output
/// (fixture adapters compute it in-process inside `hh-bench-adapter`'s own
/// process boundary — the *process* separation is the binary's, the bytes
/// are identical).
///
/// A `!applied` submission is a scored 0 (never an oracle failure). A
/// verifier failure is `oracle_failure`. An ambiguous infra signature keeps
/// the parsed reward but flags `infra_suspected`.
pub fn grade(
    req: &GradeRequest,
    verifier_stdout: &str,
    signal: &InfraSignal,
) -> Result<GradeResult, GradeError> {
    let infra = detect_infrastructure_failure(signal);
    let separate = matches!(req.isolation, hh_ontology::lab::VerifierIsolation::Separate);
    // An unapplied submission scores 0 without consulting the verifier —
    // the artifact never reached the environment (§5h.4 §6).
    if !req.submission.applied {
        return Ok(GradeResult {
            reward_ppm: 0,
            verdict: LatticeValue::C,
            infra: crate::infra::InfraReport {
                class: crate::infra::InfraClass::ArtifactMissing,
                infra_suspected: false,
                evidence: req
                    .submission
                    .apply_error
                    .clone()
                    .unwrap_or_else(|| "submission not applied".into()),
            },
            separate_verifier: separate,
            grader_log: Json::obj([
                ("grader", Json::str(&req.task.instrument.grader.adapter_ref)),
                ("applied", Json::Bool(false)),
            ]),
        });
    }
    // Verifier-tier failures ⇒ oracle_failure (typed refusal upstream).
    if let Some(oc) = infra.class.outcome_class() {
        if oc == hh_ontology::control::OutcomeClass::OracleFailure {
            return Err(GradeError::VerifierFailed(format!(
                "{}: {}",
                infra.class.as_str(),
                infra.evidence
            )));
        }
    }
    let reward_ppm = parse_typed_reward(verifier_stdout).map_err(GradeError::BadReward)?;
    let verdict = if reward_ppm >= 1_000_000 {
        LatticeValue::N
    } else if reward_ppm == 0 {
        LatticeValue::C
    } else {
        LatticeValue::P
    };
    Ok(GradeResult {
        reward_ppm,
        verdict,
        infra,
        separate_verifier: separate,
        grader_log: Json::obj([
            ("grader", Json::str(&req.task.instrument.grader.adapter_ref)),
            ("kind", Json::str(req.task.instrument.grader.kind.name())),
            ("applied", Json::Bool(true)),
        ]),
    })
}

/// Whether the task's grader kind is runnable at Stage 3 — `composite`
/// graders with a judged leg are C2+ (`GraderUnavailable`); `result`,
/// `event`, `trace` run now.
pub fn grader_runnable(kind: GraderKind) -> bool {
    matches!(
        kind,
        GraderKind::Result | GraderKind::Event | GraderKind::Trace
    )
}
