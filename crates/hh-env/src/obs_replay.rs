//! `obs_replay` — the observation-replay exec double on the
//! [`crate::executor::ToolExecutor`] seam (spec §7.4 REQ-XD-103 L1.1; design/36
//! §1.2 Part 7 K2): "an observation-replay executor fed O's raw
//! observations." The recorded `(output, returncode, exception_info)`
//! triples serve positionally — one per exec call — a typed
//! `EnvError::Unsupported{l1.observation_replay}` overrun refusal past
//! the sequence, never a fabricated observation.
//!
//! - [`RawObservation`] — the raw triple (the canonical home; the
//!   runner's `l1_obs` re-exports it — CC1, one observation type).
//! - [`ObsReplayCore`] — the positional queue: `next`/`remaining`/
//!   `served`, typed overrun. The runner's `l1_obs::ObsReplay` (the
//!   `L1Exec` impl) wraps it — the two seams share one queue.
//! - [`ObsReplayExecutor`] — the `ToolExecutor` impl: `execute` serves
//!   the next recorded raw as one `stdout` chunk + terminal report;
//!   `exception_info` rides [`ObsReplayExecutor::last_observation`]
//!   (the report vocabulary has no exception member — reported, never
//!   coerced).
//! - [`SharedObsReplayExecutor`] — `Arc`-shared clone for sessions
//!   whose `spec.exec` arms more than one exec surface: one recorded
//!   sequence serves positionally across every armed cap.

use std::collections::{BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};

use hharness_containment::policy::IsolationClass;
use hharness_hir::kinds::EffectDomain;
use hharness_wire::json::Json;

use crate::capture::CaptureKind;
use crate::errors::EnvError;
use crate::executor::{
    DedupSupport, ExecutionRequest, ExecutorDeclaration, ExecutorSignal, InterruptSupport,
    ProbeSupport, ProbeVerdict, TerminalReport, TerminalStatus, ToolExecutor,
};
use crate::observe::ErrorClass;

/// The raw observation — O's `(output, returncode, exception_info)`
/// per executed action (design/36 §1.2).
#[derive(Debug, Clone, PartialEq)]
pub struct RawObservation {
    /// The command's merged stdout+stderr (UTF-8 after
    /// `errors="replace"` — the recorded doc carries the replaced
    /// text, never raw bytes).
    pub output: String,
    /// The exit code (`-1` on timeout — E1 A.4).
    pub returncode: i64,
    /// The exception detail when the environment reported one.
    pub exception_info: Option<Json>,
}

/// `ObsReplayCore` — the positional queue behind both replay seams:
/// serves the recorded raws one per call, `observation_overrun` past
/// the end (typed — never a fabricated observation).
#[derive(Debug, Clone)]
pub struct ObsReplayCore {
    raws: VecDeque<RawObservation>,
    served: Vec<RawObservation>,
}

impl ObsReplayCore {
    /// `ObsReplayCore::new(raws)` — the recorded sequence to serve.
    pub fn new(raws: Vec<RawObservation>) -> ObsReplayCore {
        ObsReplayCore {
            raws: raws.into(),
            served: Vec::new(),
        }
    }

    /// The next recorded raw — `observation_overrun` past the end.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<RawObservation, EnvError> {
        let raw = self.raws.pop_front().ok_or(EnvError::Unsupported {
            capability: "l1.observation_replay",
            detail: "observation_overrun — the recorded sequence is exhausted; a request beyond it is a fail, never a fabricated observation".to_string(),
        })?;
        self.served.push(raw.clone());
        Ok(raw)
    }

    /// How many recorded raws remain.
    pub fn remaining(&self) -> usize {
        self.raws.len()
    }

    /// The raws served so far, in order.
    pub fn served(&self) -> &[RawObservation] {
        &self.served
    }
}

fn replay_decl() -> ExecutorDeclaration {
    ExecutorDeclaration {
        executor_id: "l1.observation_replay/1".to_string(),
        isolation_support: IsolationClass::None,
        dedup_support: DedupSupport::None,
        probe_support: ProbeSupport::Check,
        interrupt: InterruptSupport::Unsupported,
        error_classes: ["executor_error".to_string(), "timeout".to_string()]
            .into_iter()
            .collect::<BTreeSet<_>>(),
        streams: true,
        domains: [EffectDomain::Exec].into_iter().collect(),
    }
}

/// `ObsReplayExecutor` — the observation-replay double bound onto the
/// HH exec seam: `execute` serves the next recorded triple, emitting
/// the output as one `stdout` chunk and the exit status on the
/// terminal report.
pub struct ObsReplayExecutor {
    replay: ObsReplayCore,
    decl: ExecutorDeclaration,
    last: Option<RawObservation>,
}

impl ObsReplayExecutor {
    /// `ObsReplayExecutor::new(raws)` — the recorded sequence.
    pub fn new(raws: Vec<RawObservation>) -> ObsReplayExecutor {
        ObsReplayExecutor {
            replay: ObsReplayCore::new(raws),
            decl: replay_decl(),
            last: None,
        }
    }

    /// The raw triple the last `execute` served.
    pub fn last_observation(&self) -> Option<&RawObservation> {
        self.last.as_ref()
    }

    /// The underlying queue (remaining/served counts).
    pub fn replay(&self) -> &ObsReplayCore {
        &self.replay
    }
}

fn execute_replay(
    replay: &mut ObsReplayCore,
    last: &mut Option<RawObservation>,
    request: &ExecutionRequest,
    sink: &mut dyn FnMut(ExecutorSignal),
) -> Result<TerminalReport, EnvError> {
    let raw = replay.next()?;
    sink(ExecutorSignal {
        token: request.attribution_token.clone(),
        kind: CaptureKind::OutputChunk {
            stream: "stdout".to_string(),
            data: raw.output.clone(),
        },
    });
    let original_size = raw.output.len() as u64;
    // `exception_info` marks the recorded exception channel (E1 A.4's
    // `returncode: -1` timeout): the report carries the tool-reported
    // `timeout` class with the verbatim detail in `detail_ref` so the
    // model-view render emits O's `exception_info` spelling; a raw
    // without it stays the legacy `Ok`.
    let (status, detail_ref) = match &raw.exception_info {
        Some(j) => {
            let detail = match j {
                Json::Str(s) => s.clone(),
                other => other.to_canonical_string(),
            };
            (
                TerminalStatus::ToolError {
                    class: ErrorClass::Timeout,
                },
                Some(detail),
            )
        }
        None => (TerminalStatus::Ok, None),
    };
    *last = Some(raw.clone());
    Ok(TerminalReport {
        status,
        exit_status: Some(raw.returncode),
        outcome_hint: "applied".to_string(),
        retryable_hint: None,
        detail_ref,
        truncated: false,
        omitted_bytes: 0,
        original_size,
    })
}

impl ToolExecutor for ObsReplayExecutor {
    fn declaration(&self) -> &ExecutorDeclaration {
        &self.decl
    }

    fn execute(
        &mut self,
        request: &ExecutionRequest,
        sink: &mut dyn FnMut(ExecutorSignal),
    ) -> Result<TerminalReport, EnvError> {
        execute_replay(&mut self.replay, &mut self.last, request, sink)
    }

    fn probe(&self, _effect_id: &str, _attempt_no: u64) -> Result<ProbeVerdict, EnvError> {
        // The recorded triple is the settled evidence a live probe
        // would have to discover: `exception_info`/`returncode: -1`
        // is the recorded kill — the command's intended mutation
        // observably did not complete (`not_applied`). A raw without
        // that channel never reaches `unknown` (it settles
        // `observed` at report), so the honest fallback is
        // `undeterminable`.
        Ok(match &self.last {
            Some(raw) if raw.exception_info.is_some() || raw.returncode == -1 => {
                ProbeVerdict::NotApplied
            }
            _ => ProbeVerdict::Undeterminable,
        })
    }
}

/// `SharedObsReplayExecutor` — one recorded sequence serving
/// positionally across every armed exec surface (an `Arc` clone per
/// cap — the queue stays single, never per-surface duplicates).
#[derive(Clone)]
pub struct SharedObsReplayExecutor {
    inner: Arc<Mutex<ObsReplayExecutor>>,
}

impl SharedObsReplayExecutor {
    /// `SharedObsReplayExecutor::new(raws)` — the shared queue.
    pub fn new(raws: Vec<RawObservation>) -> SharedObsReplayExecutor {
        SharedObsReplayExecutor {
            inner: Arc::new(Mutex::new(ObsReplayExecutor::new(raws))),
        }
    }
}

impl ToolExecutor for SharedObsReplayExecutor {
    fn declaration(&self) -> &ExecutorDeclaration {
        // The declaration is a constant — the lock is only for `execute`.
        static DECL: std::sync::OnceLock<ExecutorDeclaration> = std::sync::OnceLock::new();
        DECL.get_or_init(replay_decl)
    }

    fn execute(
        &mut self,
        request: &ExecutionRequest,
        sink: &mut dyn FnMut(ExecutorSignal),
    ) -> Result<TerminalReport, EnvError> {
        self.inner.lock().unwrap().execute(request, sink)
    }

    fn probe(&self, effect_id: &str, attempt_no: u64) -> Result<ProbeVerdict, EnvError> {
        self.inner.lock().unwrap().probe(effect_id, attempt_no)
    }
}
