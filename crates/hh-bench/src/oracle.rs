//! `oracle` — the instrument oracle-attempt owner (execution-partition X5;
//! REQ-E-007 primary; REQ-E-102/103/204 contributing; PADR-R5-1 A.1 item 5 +
//! A.3/A.5; spec §2.2 step 4; AC-5-1/AC-5-2 and AC-2-1 hooks).
//!
//! One attempt: persist the `verification.oracle.invoked` row on the
//! instrument run **before** the pinned unchanged oracle executes over the
//! sealed submission, then retain the `verification.oracle.verdict` row with
//! the actual mapped outcome — executable/environment/access evidence, exit,
//! pinned stdout/stderr bytes + digests, timeout/refusal, grade mapping and
//! instrument finish. Invoked/verdict refs bind subject, submission, oracle,
//! attempt, policy and cost (A.3).
//!
//! Ordering and isolation guarantees:
//!
//! - **Durable before visible** — the invocation lands through the
//!   instrument run's fenced writer *before* the executor runs; a failed
//!   append means the oracle never executes (`InvocationNotPersisted`).
//!   `attempt_no` is derived from the durable stream, so a crash between
//!   invocation and verdict retains the ambiguity and a restart re-counts
//!   the committed attempts — the frozen `max_attempts` regrade cap is
//!   enforced against what the ledger actually holds.
//! - **Frozen bounds** — `max_attempts` (instrument-only regrade count),
//!   `wall_ms_bound` (per-attempt time) and `output_bytes_bound` (captured
//!   output resource) are the launch-frozen policy; the runner enforces
//!   them, never widens them. Over-bound wall time is a timeout; over-bound
//!   capture is an oracle failure with digests retained.
//! - **Existing reward/infra mapping** — outcomes map through the unchanged
//!   [`grade`] pipeline (`parse_typed_reward` +
//!   `detect_infrastructure_failure`): empty/nonfinite/crashed/timed-out/
//!   refused output is `oracle_failure`, never a fabricated reward;
//!   wrong-but-valid is a valid zero.
//! - **No participant feedback** — attempt rows append only to the
//!   instrument run; the subject run gains no rows and never relaunches.
//!   `verification.*` classes charge `instrument` by the R-ACC-3 default
//!   table — instrument cost never leaks into the subject's totals.
//! - **Isolation truth** — `separate` spells `separate_verifier: true` with
//!   the seal's held-out/access-revocation refs as evidence; `shared`/
//!   `hosted` spell `separate_verifier: false` +
//!   `tamper: "n/a{no_detector}"` — the enum can never launder a shared
//!   grading into an isolated one (§2.2 step 4).
//!
//! The executor is an injected port ([`OracleExecutor`]) — facade
//! composition wires it: deterministic fixtures in tests, the real
//! adapter/subprocess path under the separately authorized execution stage.
//! No embed→bench edge is added (X5 working set; forbidden-edges list).

use std::collections::BTreeMap;

use hharness_eval::selection::{GradeAttempt, GradeOutcome};
use hharness_lab::bench::TaskRecord;
use hharness_ledger::errors::LedgerError;
use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::manifest::EventRef;
use hharness_ledger::store::{Lease, Store};
use hharness_ontology::eval::{ChargedTo, LatticeValue, MetricValueKind};
use hharness_ontology::lab::VerifierIsolation;
use hharness_provenance::ProvenanceRecord;
use hharness_wire::Json;

use crate::grade::{grade, GradeRequest};
use crate::infra::InfraSignal;
use crate::records::Submission;

/// The durable event classes this owner emits on the instrument run
/// (registered in `hh-ledger`'s class table — one home, CC10).
/// `verification.oracle.invoked` — persisted before execution.
pub const ORACLE_INVOKED: &str = "verification.oracle.invoked";
/// `verification.oracle.verdict` — the retained attempt outcome.
pub const ORACLE_VERDICT: &str = "verification.oracle.verdict";
/// The `submission_seal` proof class — the subject run's end-state pin row
/// (R5.10 `EnvDriver::seal_workspace`); a seal ref naming anything else is
/// not a seal.
pub const SEAL_CLASS: &str = "action.environment.snapshot";
/// The purpose marker R5.10 stamps on the seal snapshot.
pub const SEAL_PURPOSE: &str = "submission_seal";
/// The tamper spelling shared/hosted isolation carries (§5h.4 §2.4 row A —
/// no detector can attest a shared grading untampered).
pub const NO_DETECTOR: &str = "n/a{no_detector}";
/// The producing component (producer + provenance identity).
const COMPONENT: &str = "hh-bench.oracle";
/// A bounded evidence head for over-bound captures (chars, like the infra
/// detector's own 200-char evidence cap).
const EVIDENCE_HEAD: usize = 200;

/// `InstrumentPolicy` — the launch-frozen instrument-only bounds
/// (PADR-R5-1 A.1 items 2/5): the regrade count cap and the per-attempt
/// time/resource envelopes. The runner enforces them against the durable
/// stream; it never widens what the grade binding froze.
#[derive(Debug, Clone, PartialEq)]
pub struct InstrumentPolicy {
    /// The frozen policy's content address.
    pub policy_ref: String,
    /// The total oracle attempts allowed for this
    /// `{subject, submission, oracle}` (1 + instrument-only regrades; never
    /// a participant rerun, recharge or new n).
    pub max_attempts: u32,
    /// The per-attempt wall bound in ms — an observation over it is a
    /// timeout regardless of the executor's own flag.
    pub wall_ms_bound: u64,
    /// The captured-output bound (`stdout + stderr` bytes) — an
    /// observation over it is an oracle failure with digests retained.
    pub output_bytes_bound: usize,
}

/// `OraclePin` — the pinned unchanged-oracle identity the grade binding
/// froze: `oracle_ref` names it, `oracle_digest` pins its bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct OraclePin {
    /// The oracle identity — must equal the task's
    /// `instrument.grader.adapter_ref` (a wrong oracle identity refuses).
    pub oracle_ref: String,
    /// The oracle's frozen content digest — the executor must run exactly
    /// these bytes (a digest mismatch refuses before invocation).
    pub oracle_digest: String,
}

/// `AttemptSpec` — the frozen grade-binding facts one attempt must match:
/// the subject/submission/oracle pins, the frozen policy, the instrument
/// funding and the task whose unchanged oracle runs.
#[derive(Debug, Clone)]
pub struct AttemptSpec {
    /// The subject run the sealed submission belongs to.
    pub subject_run_id: String,
    /// The sealed submission's frozen content address
    /// (`submission.compute_id()` — the digest the seal pinned).
    pub submission_ref: String,
    /// The pinned oracle identity.
    pub oracle: OraclePin,
    /// The frozen instrument policy (regrade count/time/resource bounds).
    pub policy: InstrumentPolicy,
    /// The frozen instrument funding the attempt charges (`cost_ref` —
    /// never the subject root).
    pub instrument_budget_ref: String,
    /// The task — `grade` reads `held_out` + `instrument` only, never a
    /// model claim.
    pub task: TaskRecord,
    /// The resolved verifier isolation — `separate` default; `shared`/
    /// `hosted` only where declared (the record spells the actual mode).
    pub isolation: VerifierIsolation,
}

/// `SealedSubmission` — the artifact the oracle consumes plus its seal
/// proof. `seal_ref` must resolve to the subject run's
/// `action.environment.snapshot{purpose:"submission_seal"}` row — a
/// mutable workspace is never graded.
#[derive(Debug, Clone)]
pub struct SealedSubmission {
    /// The submission's content address — must equal the frozen pin.
    pub submission_ref: String,
    /// The subject run's `submission_seal` row — the seal proof.
    pub seal_ref: EventRef,
    /// The sealed artifact (`compute_id` must equal the frozen
    /// `submission_ref` — the digest leg of the pin).
    pub submission: Submission,
}

/// `OracleObservation` — the raw bytes one oracle execution produced. The
/// executor reports what it observed; every failure mode is a typed
/// observation, never a fabricated reward.
#[derive(Debug, Clone, PartialEq)]
pub struct OracleObservation {
    /// The oracle's exit code (`None` = killed/no exit — a crash
    /// signature).
    pub exit_code: Option<i32>,
    /// The oracle's own timeout flag (the runner additionally forces
    /// timeout when `wall_ms` exceeds the frozen bound).
    pub timed_out: bool,
    /// The captured stdout — the typed-reward source.
    pub stdout: String,
    /// The captured stderr — the infra-signature source.
    pub stderr: String,
    /// The environment-side error, when the environment reported one.
    pub environment_error: Option<String>,
    /// The observed execution wall time (ms).
    pub wall_ms: u64,
}

/// `OracleExecutor` — the instrument producer port (facade composition;
/// X5 working set). One method runs the pinned oracle over the sealed
/// submission; `oracle_digest` reports the bytes it would actually run.
pub trait OracleExecutor {
    /// The content digest of the oracle this executor runs — checked
    /// against the frozen pin before invocation.
    fn oracle_digest(&self) -> &str;
    /// Execute the pinned oracle over the sealed submission. Refusals and
    /// crashes are observations (`exit_code`/`stderr`/`timed_out`), never
    /// fabricated rewards.
    fn execute(&self, task: &TaskRecord, submission: &Submission) -> OracleObservation;
}

/// `OracleError` — the owner seam's typed refusals (never a warning).
#[derive(Debug)]
pub enum OracleError {
    /// A required pin member is empty.
    EmptyPin {
        /// The empty member.
        member: &'static str,
    },
    /// A presented pin differs from the frozen one.
    PinMismatch {
        /// The differing member.
        member: &'static str,
        /// The frozen value.
        expected: String,
        /// The presented value.
        presented: String,
    },
    /// The presented bytes digest to something other than the frozen pin —
    /// a mutated submission or a non-pinned oracle.
    DigestMismatch {
        /// The differing member.
        member: &'static str,
        /// The frozen digest.
        expected: String,
        /// The computed digest.
        presented: String,
    },
    /// The seal ref does not resolve to a `submission_seal` row on the
    /// subject run — a mutable workspace is never graded.
    UnsealedSubmission {
        /// Why the seal proof failed.
        detail: String,
    },
    /// The frozen `max_attempts` bound is spent — the earlier attempts
    /// stay retained; nothing further executes and nothing is erased.
    AttemptCap {
        /// The frozen cap.
        max_attempts: u32,
    },
    /// The `verification.oracle.invoked` append failed — the oracle never
    /// ran; invocation-before-execution is enforced, never reordered.
    InvocationNotPersisted {
        /// The append failure detail.
        detail: String,
    },
    /// The `verification.oracle.verdict` append failed — the invoked row
    /// stands; the retained ambiguity is the honest recovery state (A.5's
    /// invocation→verdict window).
    RetentionFailed {
        /// The committed invocation coordinate.
        invocation_ref: String,
        /// The append failure detail.
        detail: String,
    },
    /// A store/ledger failure.
    Store(LedgerError),
}

impl std::fmt::Display for OracleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OracleError::EmptyPin { member } => write!(f, "EmptyPin({member})"),
            OracleError::PinMismatch {
                member,
                expected,
                presented,
            } => write!(
                f,
                "PinMismatch({member}: frozen={expected} presented={presented})"
            ),
            OracleError::DigestMismatch {
                member,
                expected,
                presented,
            } => write!(
                f,
                "DigestMismatch({member}: frozen={expected} presented={presented})"
            ),
            OracleError::UnsealedSubmission { detail } => {
                write!(f, "UnsealedSubmission({detail})")
            }
            OracleError::AttemptCap { max_attempts } => {
                write!(f, "AttemptCap(max_attempts={max_attempts})")
            }
            OracleError::InvocationNotPersisted { detail } => {
                write!(f, "InvocationNotPersisted({detail})")
            }
            OracleError::RetentionFailed {
                invocation_ref,
                detail,
            } => write!(f, "RetentionFailed({invocation_ref}: {detail})"),
            OracleError::Store(e) => write!(f, "Store({e:?})"),
        }
    }
}

impl std::error::Error for OracleError {}

impl From<LedgerError> for OracleError {
    fn from(e: LedgerError) -> OracleError {
        OracleError::Store(e)
    }
}

/// `AttemptOutcome` — one attempt's mapped result.
#[derive(Debug, Clone, PartialEq)]
pub enum AttemptOutcome {
    /// A decided grade — `reward_ppm ∈ [0, 1e6]`; wrong-but-valid is a
    /// valid zero.
    Decided {
        /// The typed reward (ppm).
        reward_ppm: i64,
        /// The lattice verdict (`N` ≥ 1e6, `C` = 0, `P` between).
        verdict: LatticeValue,
    },
    /// Empty/nonfinite/crashed/timed-out/refused/over-bound — oracle
    /// failure, never a fabricated reward.
    OracleFailure {
        /// The failure class detail.
        detail: String,
    },
}

/// `OracleAttempt` — the retained consumer record for one executed
/// attempt: the durable coordinates plus the mapped outcome. One
/// `GradeAttempt` selection fact is produced per eligible metric.
#[derive(Debug, Clone, PartialEq)]
pub struct OracleAttempt {
    /// The attempt's content address (`oracle.attempt` idp over the
    /// verdict payload's binding members).
    pub attempt_ref: String,
    /// The attempt ordinal (durable-derived — counts the committed
    /// `verification.oracle.invoked` rows for these pins).
    pub attempt_no: u32,
    /// The instrument run the attempt landed on.
    pub instrument_run_id: String,
    /// The `verification.oracle.invoked` row's coordinate.
    pub invocation_ref: EventRef,
    /// The `verification.oracle.verdict` row's coordinate.
    pub verdict_ref: EventRef,
    /// The verdict row's durable seq (the `(run, seq)` pin the selection
    /// contract and watermarks read).
    pub verdict_seq: u64,
    /// The subject run.
    pub subject_run_id: String,
    /// The sealed submission's content address.
    pub submission_ref: String,
    /// The pinned oracle identity.
    pub oracle_ref: String,
    /// The seal proof the attempt cited.
    pub seal_ref: EventRef,
    /// The mapped outcome.
    pub outcome: AttemptOutcome,
    /// The instrument finish time (the store clock's ms at verdict mint).
    pub finished_ms: u64,
}

impl OracleAttempt {
    /// The R5.9 selection producer fact (`GradeAttempt`) for one eligible
    /// metric: the durable `(instrument_run_id, verdict_seq)` coordinate,
    /// the subject/submission/oracle pins, cause coordinates citing the
    /// subject run's seal row and the instrument run's invoked/verdict
    /// rows, and the mapped outcome (`Decided` / `OracleFailure`).
    pub fn to_grade_attempt(&self, metric_ref: &str) -> GradeAttempt {
        GradeAttempt {
            attempt_ref: self.attempt_ref.clone(),
            instrument_run_id: self.instrument_run_id.clone(),
            seq: self.verdict_seq,
            subject_ref: self.subject_run_id.clone(),
            submission_ref: self.submission_ref.clone(),
            oracle_ref: self.oracle_ref.clone(),
            metric_ref: metric_ref.to_string(),
            cause_refs: vec![
                format!("{}:{}", self.seal_ref.run_id, self.seal_ref.event_id),
                format!(
                    "{}:{}",
                    self.invocation_ref.run_id, self.invocation_ref.event_id
                ),
                format!("{}:{}", self.verdict_ref.run_id, self.verdict_ref.event_id),
            ],
            outcome: match &self.outcome {
                AttemptOutcome::Decided { reward_ppm, .. } => {
                    GradeOutcome::Decided(MetricValueKind::Decimal(*reward_ppm))
                }
                AttemptOutcome::OracleFailure { .. } => GradeOutcome::OracleFailure,
            },
        }
    }
}

/// `OracleRunner` — the attempt driver bound to one instrument run's
/// fenced writer and one frozen [`AttemptSpec`]. Every `attempt` executes
/// one retained oracle try; a regrade is another `attempt` call on the
/// same spec (never a participant rerun, recharge or new n).
pub struct OracleRunner<'a> {
    store: &'a mut Store,
    run_id: String,
    lease: &'a Lease,
    spec: AttemptSpec,
}

impl<'a> OracleRunner<'a> {
    /// Bind the runner to the instrument run's fenced writer and the
    /// frozen spec.
    pub fn new(
        store: &'a mut Store,
        instrument_run_id: &str,
        lease: &'a Lease,
        spec: AttemptSpec,
    ) -> Self {
        OracleRunner {
            store,
            run_id: instrument_run_id.to_string(),
            lease,
            spec,
        }
    }

    /// The next attempt ordinal for these pins — durable-derived: the
    /// committed `verification.oracle.invoked` rows for this
    /// `{subject_run, submission_ref, oracle_ref}` plus one. A crash
    /// between invoked and verdict retains the ambiguity; a restart
    /// re-counts what the ledger holds.
    pub fn next_attempt_no(&self) -> Result<u32, LedgerError> {
        let spec = &self.spec;
        let next = self
            .store
            .events(&self.run_id)?
            .iter()
            .filter(|e| {
                e.class == ORACLE_INVOKED
                    && e.payload.get("subject_run").and_then(Json::as_str)
                        == Some(spec.subject_run_id.as_str())
                    && e.payload.get("submission_ref").and_then(Json::as_str)
                        == Some(spec.submission_ref.as_str())
                    && e.payload.get("oracle_ref").and_then(Json::as_str)
                        == Some(spec.oracle.oracle_ref.as_str())
            })
            .filter_map(|e| e.payload.get("attempt_no").and_then(Json::as_int))
            .max()
            .unwrap_or(0)
            + 1;
        Ok(next as u32)
    }

    /// `attempt(sealed, exec)` — validate pins/digests → enforce the
    /// frozen attempt cap → persist `verification.oracle.invoked` **before
    /// execution** → run the pinned oracle → enforce the frozen
    /// time/resource bounds → map through the unchanged `grade` pipeline →
    /// retain `verification.oracle.verdict`.
    pub fn attempt(
        &mut self,
        sealed: &SealedSubmission,
        exec: &dyn OracleExecutor,
    ) -> Result<OracleAttempt, OracleError> {
        let spec = &self.spec;
        // ── pin/digest validation (before any durable row) ────────────
        for (member, value) in [
            ("subject_run_id", spec.subject_run_id.as_str()),
            ("submission_ref", spec.submission_ref.as_str()),
            ("oracle_ref", spec.oracle.oracle_ref.as_str()),
            ("oracle_digest", spec.oracle.oracle_digest.as_str()),
            ("policy_ref", spec.policy.policy_ref.as_str()),
            ("instrument_budget_ref", spec.instrument_budget_ref.as_str()),
        ] {
            if value.is_empty() {
                return Err(OracleError::EmptyPin { member });
            }
        }
        // The seal proof must resolve to a real `submission_seal` pin on
        // the subject run — anything else means a mutable workspace.
        if sealed.seal_ref.run_id != spec.subject_run_id {
            return Err(OracleError::UnsealedSubmission {
                detail: format!(
                    "seal on {} not on subject run {}",
                    sealed.seal_ref.run_id, spec.subject_run_id
                ),
            });
        }
        let sealed_event = self
            .store
            .events(&spec.subject_run_id)
            .map_err(OracleError::Store)?
            .iter()
            .find(|e| e.event_id == sealed.seal_ref.event_id)
            .cloned();
        match sealed_event {
            Some(e)
                if e.class == SEAL_CLASS
                    && e.payload.get("purpose").and_then(Json::as_str) == Some(SEAL_PURPOSE) => {}
            Some(_) => {
                return Err(OracleError::UnsealedSubmission {
                    detail: format!(
                        "{} is not a {SEAL_CLASS}{{purpose:{SEAL_PURPOSE}}} row",
                        sealed.seal_ref.event_id
                    ),
                })
            }
            None => {
                return Err(OracleError::UnsealedSubmission {
                    detail: format!("{} not found on subject run", sealed.seal_ref.event_id),
                })
            }
        }
        if sealed.submission_ref != spec.submission_ref {
            return Err(OracleError::PinMismatch {
                member: "submission_ref",
                expected: spec.submission_ref.clone(),
                presented: sealed.submission_ref.clone(),
            });
        }
        if spec.oracle.oracle_ref != spec.task.instrument.grader.adapter_ref {
            return Err(OracleError::PinMismatch {
                member: "oracle_ref",
                expected: spec.task.instrument.grader.adapter_ref.clone(),
                presented: spec.oracle.oracle_ref.clone(),
            });
        }
        let presented_digest = sealed.submission.compute_id();
        if presented_digest != spec.submission_ref {
            return Err(OracleError::DigestMismatch {
                member: "submission_digest",
                expected: spec.submission_ref.clone(),
                presented: presented_digest,
            });
        }
        if exec.oracle_digest() != spec.oracle.oracle_digest {
            return Err(OracleError::DigestMismatch {
                member: "oracle_digest",
                expected: spec.oracle.oracle_digest.clone(),
                presented: exec.oracle_digest().to_string(),
            });
        }

        // ── the frozen instrument-only regrade count ──────────────────
        let attempt_no = self.next_attempt_no().map_err(OracleError::Store)?;
        if attempt_no > spec.policy.max_attempts {
            return Err(OracleError::AttemptCap {
                max_attempts: spec.policy.max_attempts,
            });
        }

        // ── persist the invocation BEFORE the oracle executes ─────────
        let invoked = self.mint(
            ORACLE_INVOKED,
            invoked_payload(spec, sealed, attempt_no),
            vec![sealed.seal_ref.clone()],
        )?;
        let invoked_id = invoked.event_id.clone();
        self.store
            .append(&self.run_id, self.lease, vec![invoked])
            .map_err(|e| OracleError::InvocationNotPersisted {
                detail: format!("{e:?}"),
            })?;
        let invocation_ref = EventRef {
            run_id: self.run_id.clone(),
            event_id: invoked_id,
        };

        // ── execute the pinned unchanged oracle ───────────────────────
        let obs = exec.execute(&spec.task, &sealed.submission);

        // ── the frozen time/resource bounds ───────────────────────────
        let timed_out = obs.timed_out || obs.wall_ms > spec.policy.wall_ms_bound;
        let over_bound = obs.stdout.len() + obs.stderr.len() > spec.policy.output_bytes_bound;

        // ── map through the unchanged reward/infra pipeline ───────────
        let signal = InfraSignal {
            verifier_exit: obs.exit_code,
            verifier_stderr: &obs.stderr,
            verifier_timed_out: timed_out,
            environment_error: obs.environment_error.as_deref(),
            submission_produced: true,
        };
        let infra = crate::infra::detect_infrastructure_failure(&signal);
        let (outcome, grader_log) = if over_bound {
            (
                AttemptOutcome::OracleFailure {
                    detail: format!(
                        "output_over_bound: {} + {} > {}",
                        obs.stdout.len(),
                        obs.stderr.len(),
                        spec.policy.output_bytes_bound
                    ),
                },
                Json::Null,
            )
        } else {
            let req = GradeRequest {
                submission: sealed.submission.clone(),
                task: spec.task.clone(),
                isolation: spec.isolation,
            };
            match grade(&req, &obs.stdout, &signal) {
                Ok(res) => (
                    AttemptOutcome::Decided {
                        reward_ppm: res.reward_ppm,
                        verdict: res.verdict,
                    },
                    res.grader_log,
                ),
                Err(e) => (
                    AttemptOutcome::OracleFailure {
                        detail: e.to_string(),
                    },
                    Json::Null,
                ),
            }
        };

        // ── retain the verdict row ────────────────────────────────────
        let finished_ms = self.store.now_ms();
        let mut payload = verdict_payload(
            spec,
            sealed,
            attempt_no,
            &invocation_ref,
            &obs,
            timed_out,
            over_bound,
            &infra,
            &outcome,
            &grader_log,
            finished_ms,
        );
        let attempt_ref =
            hharness_identity::idp_id("oracle.attempt", payload.to_canonical_string().as_bytes());
        if let Json::Obj(ref mut m) = payload {
            m.insert("attempt_ref".to_string(), Json::str(&attempt_ref));
        }
        let verdict = self.mint(ORACLE_VERDICT, payload, vec![invocation_ref.clone()])?;
        let verdict_id = verdict.event_id.clone();
        let range = self
            .store
            .append(&self.run_id, self.lease, vec![verdict])
            .map_err(|e| OracleError::RetentionFailed {
                invocation_ref: format!("{}:{}", invocation_ref.run_id, invocation_ref.event_id),
                detail: format!("{e:?}"),
            })?;
        let verdict_ref = EventRef {
            run_id: self.run_id.clone(),
            event_id: verdict_id,
        };
        Ok(OracleAttempt {
            attempt_ref,
            attempt_no,
            instrument_run_id: self.run_id.clone(),
            invocation_ref,
            verdict_ref: verdict_ref.clone(),
            verdict_seq: range.last,
            subject_run_id: spec.subject_run_id.clone(),
            submission_ref: spec.submission_ref.clone(),
            oracle_ref: spec.oracle.oracle_ref.clone(),
            seal_ref: sealed.seal_ref.clone(),
            outcome,
            finished_ms,
        })
    }

    /// Mint a caller-supplied event (kernel producer + provenance; the
    /// store stamps seq/hash/lease generation).
    fn mint(
        &self,
        class: &str,
        payload: Json,
        causes: Vec<EventRef>,
    ) -> Result<Event, LedgerError> {
        Ok(Event {
            event_id: self.store.alloc_id("evt"),
            class: class.to_string(),
            ts: self.store.ts_now(),
            hlc: None,
            producer: Producer::kernel(COMPONENT),
            scope: Scope::default(),
            parent_event_id: self.store.head_event_id(&self.run_id)?,
            causes,
            refs: vec![],
            ir_refs: vec![],
            surface_ids: BTreeMap::new(),
            provenance: Some(ProvenanceRecord::kernel(COMPONENT, self.store.now_ms())),
            content_kind: None,
            payload,
        })
    }
}

/// The `verification.oracle.invoked` payload — binds subject, submission
/// (+digest/seal), oracle (+digest), attempt, policy and instrument
/// funding before the oracle runs (A.3).
fn invoked_payload(spec: &AttemptSpec, sealed: &SealedSubmission, attempt_no: u32) -> Json {
    Json::obj([
        ("subject_run", Json::str(&spec.subject_run_id)),
        ("submission_ref", Json::str(&spec.submission_ref)),
        (
            "submission_digest",
            Json::str(sealed.submission.compute_id()),
        ),
        (
            "seal_ref",
            Json::str(format!(
                "{}:{}",
                sealed.seal_ref.run_id, sealed.seal_ref.event_id
            )),
        ),
        ("oracle_ref", Json::str(&spec.oracle.oracle_ref)),
        ("oracle_digest", Json::str(&spec.oracle.oracle_digest)),
        ("attempt_no", Json::Int(attempt_no as i64)),
        ("policy_ref", Json::str(&spec.policy.policy_ref)),
        (
            "instrument_budget_ref",
            Json::str(&spec.instrument_budget_ref),
        ),
        ("isolation", Json::str(spec.isolation.name())),
        ("charged_to", Json::str(ChargedTo::Instrument.as_str())),
    ])
}

/// The `verification.oracle.verdict` payload — the retained attempt:
/// invoked ref, pins, mapped outcome, actual output bytes + digests,
/// status/timeout, infra/isolation evidence and instrument finish/cost.
#[allow(clippy::too_many_arguments)] // the retained record's members are its shape.
fn verdict_payload(
    spec: &AttemptSpec,
    sealed: &SealedSubmission,
    attempt_no: u32,
    invocation_ref: &EventRef,
    obs: &OracleObservation,
    timed_out: bool,
    over_bound: bool,
    infra: &crate::infra::InfraReport,
    outcome: &AttemptOutcome,
    grader_log: &Json,
    finished_ms: u64,
) -> Json {
    let outcome_json = match outcome {
        AttemptOutcome::Decided {
            reward_ppm,
            verdict,
        } => Json::obj([
            ("status", Json::str("decided")),
            ("reward_ppm", Json::Int(*reward_ppm)),
            ("verdict", Json::str(verdict.as_str())),
        ]),
        AttemptOutcome::OracleFailure { detail } => Json::obj([
            ("status", Json::str("oracle_failure")),
            ("detail", Json::str(detail)),
        ]),
    };
    let stdout_digest = hharness_identity::idp_id("oracle.stdout", obs.stdout.as_bytes());
    let stderr_digest = hharness_identity::idp_id("oracle.stderr", obs.stderr.as_bytes());
    let mut execution = vec![
        (
            "exit_code",
            obs.exit_code.map_or(Json::Null, |c| Json::Int(c as i64)),
        ),
        ("timed_out", Json::Bool(timed_out)),
        ("wall_ms", Json::Int(obs.wall_ms as i64)),
        ("stdout_digest", Json::str(stdout_digest)),
        ("stderr_digest", Json::str(stderr_digest)),
        ("stdout_bytes", Json::Int(obs.stdout.len() as i64)),
        ("stderr_bytes", Json::Int(obs.stderr.len() as i64)),
        ("infra", infra.to_json()),
        ("grader_log", grader_log.clone()),
    ];
    if over_bound {
        // Over the frozen capture bound: digests + byte counts + a bounded
        // evidence head — never a silent truncation passed off as the pin.
        execution.push(("over_bound", Json::Bool(true)));
        execution.push((
            "stdout_head",
            Json::str(obs.stdout.chars().take(EVIDENCE_HEAD).collect::<String>()),
        ));
        execution.push((
            "stderr_head",
            Json::str(obs.stderr.chars().take(EVIDENCE_HEAD).collect::<String>()),
        ));
    } else {
        execution.push(("stdout", Json::str(&obs.stdout)));
        execution.push(("stderr", Json::str(&obs.stderr)));
    }
    let separate = matches!(spec.isolation, VerifierIsolation::Separate);
    let mut isolation = vec![
        ("mode", Json::str(spec.isolation.name())),
        ("separate_verifier", Json::Bool(separate)),
        ("held_out_access", Json::str("held_out")),
        (
            "access_revoked_ref",
            Json::str(format!(
                "{}:{}",
                sealed.seal_ref.run_id, sealed.seal_ref.event_id
            )),
        ),
    ];
    if !separate {
        // Shared/hosted stays shared — no enum upgrade, no detector claim.
        isolation.push(("tamper", Json::str(NO_DETECTOR)));
    }
    Json::obj([
        (
            "invocation_ref",
            Json::str(format!(
                "{}:{}",
                invocation_ref.run_id, invocation_ref.event_id
            )),
        ),
        ("subject_run", Json::str(&spec.subject_run_id)),
        ("submission_ref", Json::str(&spec.submission_ref)),
        ("oracle_ref", Json::str(&spec.oracle.oracle_ref)),
        ("attempt_no", Json::Int(attempt_no as i64)),
        ("policy_ref", Json::str(&spec.policy.policy_ref)),
        ("cost_ref", Json::str(&spec.instrument_budget_ref)),
        ("outcome", outcome_json),
        ("execution", Json::obj(execution)),
        ("isolation", Json::obj(isolation)),
        ("charged_to", Json::str(ChargedTo::Instrument.as_str())),
        ("finished_ms", Json::Int(finished_ms as i64)),
    ])
}
