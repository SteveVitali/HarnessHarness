//! R4.7e — the live `JudgePort` drain seam + the R-2.7.2b judged
//! reconciler arm, hermetic (DF-S1.21-3's driver half; BL-17's evidence
//! leg's kernel side). A scripted `JudgePort` double plays the live
//! binding's contract: a `judge` consult buffers
//! `verification.validator.*` triples + `hh.model_io/1` blobs for the
//! driver's fenced drain (`drain_judge_records` — durable-before-visible,
//! the port computes and the driver owns the fence); `assess` answers
//! `JudgedConsult`s the completion gate folds through `reconcile_judged`
//! — `detector: judged` records under each critic's own `detector_ref`,
//! veto-admissible, never hold-admissible (F7). `RecordedJudge` abstains
//! everywhere — replay stays byte-identical.
//!
//! Requirement ids exercised: DF-S1.21-3, DF-S1.21-2's residual
//! consumption, R-2.7.2b (judged arm), ADR-0300 (calibration on the
//! record), ADR-0311 (independence on the record).

#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

use hharness_control::driver::{
    AssembleInputs, AssembledRequest, AssemblerPort, Driver, DriverConfig, EffectGate, GateOutcome,
    LedgerSink, ModelOutcome, ModelPort,
};
use hharness_control::loops::{JudgePort, JudgedConsult};
use hharness_control::output::{ParamKind, ParamSpec, ParsedCall, SurfaceSpec};
use hharness_control::policy::{EnvelopePolicy, JudgedSpec};
use hharness_control::react::ReactMinimal;
use hharness_control::strategy::{ControlContext, ControlStrategy, StrategyParams};
use hharness_control::vocab::{ControlDecision, Cue, DecisionKind, DecisionStamp, SettledOutcome};
use hharness_ledger::classes::Durability;
use hharness_ledger::event::{Event, EventEnvelope, EventPlane};
use hharness_ledger::manifest::ParticipantClass;
use hharness_ontology::control::{DecisionPoint, Owner, StopReason};
use hharness_provenance::record::ProvenanceRecord;
use hharness_verification::reconciler::{JudgedAssessment, ReconcilerDeclaration};
use hharness_verification::vocab::{Detector, DivergenceClass};
use hharness_wire::json::Json;

// ── test doubles (r2_5's shape — minimal set for this battery) ─────────

/// The in-memory sink — no `store_seam` (the default `None`): the
/// `model_io` drain against it is the contract-violation leg.
struct MemSink {
    events: Vec<EventEnvelope>,
    seq: u64,
}

impl MemSink {
    fn new() -> Self {
        MemSink {
            events: vec![],
            seq: 0,
        }
    }
    fn classes(&self) -> Vec<&str> {
        self.events.iter().map(|e| e.class.as_str()).collect()
    }
    fn find(&self, class: &str) -> Vec<&EventEnvelope> {
        self.events.iter().filter(|e| e.class == class).collect()
    }
}

impl LedgerSink for MemSink {
    fn append(&mut self, events: Vec<Event>) -> Result<(), String> {
        for mut e in events {
            if e.provenance.is_none() {
                e.provenance = Some(ProvenanceRecord::kernel("hh-test/sink", self.seq + 1));
            }
            self.seq += 1;
            self.events.push(EventEnvelope {
                event_id: e.event_id,
                run_id: "run".into(),
                seq: self.seq,
                ts: e.ts,
                hlc: None,
                plane: EventPlane::of_class(&e.class).unwrap_or(EventPlane::Control),
                class: e.class,
                schema_version: 1,
                producer: e.producer,
                participant_class: ParticipantClass::Native,
                observability_level: Default::default(),
                durability: Durability::Ledger,
                scope: e.scope,
                lease_generation: 1,
                parent_event_id: e.parent_event_id,
                causes: e.causes,
                refs: vec![],
                ir_refs: vec![],
                surface_ids: Default::default(),
                provenance: e.provenance,
                payload: e.payload,
                prev_hash: "h".into(),
                hash: format!("h{}", self.seq),
            });
        }
        Ok(())
    }
    fn prefix(&self) -> &[EventEnvelope] {
        &self.events
    }
}

/// A scripted model — pops outcomes in order.
struct ScriptedModel {
    script: VecDeque<ModelOutcome>,
}

impl ModelPort for ScriptedModel {
    fn call(&mut self, _id: &str, _req: &Json) -> ModelOutcome {
        self.script.pop_front().unwrap_or(ModelOutcome {
            stop_reason: hharness_gateway::vocab::StopReason::EndTurn,
            response_ref: "r-empty".into(),
            text_empty: true,
            calls: vec![],
            error_class: None,
            retry_after_ms: None,
        })
    }
}

/// `hh.submit` answers `observed` + the submission marker.
struct ScriptedGate;

impl EffectGate for ScriptedGate {
    fn dispatch(&mut self, _ef: &str, _a: u64, intent: &Json) -> GateOutcome {
        let surface = intent
            .get("surface_id")
            .or_else(|| intent.get("surface"))
            .and_then(Json::as_str)
            .unwrap_or("");
        GateOutcome {
            outcome: SettledOutcome::Observed {
                outcome: "applied".into(),
            },
            submission_ref: (surface == "hh.submit").then(|| "sub-1".to_string()),
            error_class: None,
            emitted: Vec::new(),
        }
    }
    fn finish_record(&self) -> Option<Json> {
        Some(Json::obj([("completion", Json::str("achieved"))]))
    }
}

/// The assembler double — `context.assembled` only.
struct FakeAssembler;

impl AssemblerPort for FakeAssembler {
    fn assemble(&mut self, inputs: &AssembleInputs<'_>, _req: &Json) -> AssembledRequest {
        AssembledRequest {
            request: Json::obj([("model_call_id", Json::str(inputs.model_call_id))]),
            assembled_payload: None,
            side_events: vec![],
            pre_events: vec![],
            overflow: None,
        }
    }
}

/// A prefix-scripted strategy — `ReactMinimal` underneath.
struct ScriptedStrategy {
    inner: ReactMinimal,
    script: Vec<(&'static str, DecisionKind)>,
    fired: RefCell<Vec<bool>>,
}

impl ScriptedStrategy {
    fn new(script: Vec<(&'static str, DecisionKind)>) -> Self {
        ScriptedStrategy {
            inner: ReactMinimal::new(),
            fired: RefCell::new(vec![false; script.len()]),
            script,
        }
    }
}

impl ControlStrategy for ScriptedStrategy {
    fn capabilities(&self) -> &hharness_control::strategy::ControlCapabilities {
        self.inner.capabilities()
    }
    fn open(
        &mut self,
        ctx: &ControlContext,
    ) -> Result<hharness_control::state::ControlState, hharness_control::strategy::ControlError>
    {
        self.inner.open(ctx)
    }
    fn observe(&self, state: &mut hharness_control::state::ControlState, events: &[EventEnvelope]) {
        self.inner.observe(state, events);
    }
    fn decide(
        &self,
        state: &mut hharness_control::state::ControlState,
        cue: &Cue,
    ) -> ControlDecision {
        let tag = match cue {
            Cue::ModelCompleted { .. } => "model_completed",
            _ => "other",
        };
        let mut fired = self.fired.borrow_mut();
        for (i, (prefix, kind)) in self.script.iter().enumerate() {
            if !fired[i] && tag == *prefix {
                fired[i] = true;
                let point = match kind {
                    DecisionKind::Retrieve { .. } => DecisionPoint::Retrieve,
                    _ => DecisionPoint::Act,
                };
                return ControlDecision {
                    stamp: DecisionStamp {
                        decision_point: point,
                        owner: Owner::Model,
                        rationale_ref: None,
                    },
                    kind: kind.clone(),
                };
            }
        }
        drop(fired);
        self.inner.decide(state, cue)
    }
    fn restore(
        &mut self,
        checkpoint: &[u8],
        ctx: &ControlContext,
    ) -> Result<hharness_control::state::ControlState, hharness_control::strategy::RestoreError>
    {
        self.inner.restore(checkpoint, ctx)
    }
    fn terminate(
        &self,
        state: &hharness_control::state::ControlState,
        reason: &StopReason,
    ) -> hharness_control::strategy::FinalReport {
        self.inner.terminate(state, reason)
    }
}

/// `ScriptedJudge` — the live binding's `JudgePort` contract played
/// hermetically: consults answer scripted values and buffer the
/// `(class, payload, provenance)` triples + `hh.model_io/1` blobs a real
/// `LiveJudge` would produce; `assess` answers the scripted
/// `JudgedConsult`s.
#[derive(Debug, Default)]
struct ScriptedJudge {
    /// The `judge()` answer (ppm).
    answer: u64,
    /// The `assess()` consults returned for every claim.
    consults: Vec<JudgedConsult>,
    /// Rows the consult buffers for the fenced drain.
    buffered: std::sync::Mutex<Vec<(String, Json, ProvenanceRecord)>>,
    /// Blobs the consult buffers (`(media, bytes)`).
    buffered_io: std::sync::Mutex<Vec<(String, Vec<u8>)>>,
    /// Consults the port saw (`judge` calls) — the drain-timing witness.
    consults_seen: std::sync::atomic::AtomicU64,
}

impl JudgePort for ScriptedJudge {
    fn judge(&self, _spec: &JudgedSpec, _window: &[Json]) -> u64 {
        self.consults_seen
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.answer
    }
    fn assess(
        &self,
        _claim: &hharness_verification::claims::Claim,
        _evidence: &[Json],
    ) -> Vec<JudgedConsult> {
        // The critic's own verdict rows stay durable — each returned
        // consult buffers one, the fenced drain at completion lands it
        // before the `claim.reconciled` rows that cite it.
        for c in &self.consults {
            self.buffered.lock().unwrap().push((
                "verification.validator.verdict".to_string(),
                Json::obj([
                    ("verdict_id", Json::str(format!("verdict:{}", c.judge_ref))),
                    ("validator_ref", Json::str(c.judge_ref.clone())),
                    ("detector", Json::str("judged")),
                    ("status", Json::str("decided")),
                    ("value", Json::Int(500_000)),
                    ("charged_to", Json::str("instrument")),
                ]),
                ProvenanceRecord::minted(
                    hharness_provenance::origin::Origin::model("judge-model", "run", "assess"),
                    hharness_provenance::PersistenceScope::Run,
                    0,
                ),
            ));
        }
        self.consults.clone()
    }
    fn take_records(&self) -> Vec<(String, Json, ProvenanceRecord)> {
        std::mem::take(&mut *self.buffered.lock().unwrap())
    }
    fn take_model_io(&self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut *self.buffered_io.lock().unwrap())
    }
}

// ── fixtures ────────────────────────────────────────────────────────────

fn ctx() -> ControlContext {
    ControlContext {
        process_ref: "proc-1".into(),
        plan: vec![],
        boundary: hharness_control::react::react_preset(),
        profile: Json::Null,
        account_ref: "acct".into(),
        budget_ref: "b-1".into(),
        envelope_ref: "env-1".into(),
        parameters: StrategyParams::default(),
        capabilities_available: vec!["hh.submit".into()],
        steering: (
            hharness_control::strategy::SteerMode::Unsupported,
            hharness_control::strategy::ConcurrentInput::QueueOnly,
        ),
    }
}

fn submit_surface() -> SurfaceSpec {
    SurfaceSpec {
        surface_id: "hh.submit".into(),
        semantic_id: "sem/hh.submit".into(),
        params: [(
            "text".into(),
            ParamSpec {
                required: true,
                kind: ParamKind::Str,
                enum_values: vec![],
                domain: vec![],
            },
        )]
        .into_iter()
        .collect(),
        risk_class: None,
        family: None,
    }
}

fn submit_call() -> ModelOutcome {
    ModelOutcome {
        stop_reason: hharness_gateway::vocab::StopReason::ToolUse,
        response_ref: "r-1".into(),
        text_empty: false,
        calls: vec![ParsedCall {
            tool_call_id: "tc-submit".into(),
            surface: "hh.submit".into(),
            args_raw: r#"{"text":"done"}"#.into(),
        }],
        error_class: None,
        retry_after_ms: None,
    }
}

/// `judged_policy()` — the stage-1 envelope with `LoopPolicy.judged`
/// armed (cadence 1 — the consult fires at the first interpret).
fn judged_policy() -> EnvelopePolicy {
    let mut p = EnvelopePolicy::stage1_default("b-1");
    p.loop_policy.judged = Some(JudgedSpec {
        validator_ref: "validator:judge-live".into(),
        after_turns: 0,
        interval: 1,
        confidence_threshold_ppm: 500_000,
        charged_to: "instrument".into(),
    });
    p.seal().unwrap()
}

/// `judged_reconciler()` — the `execution_alignment` declaration admitting
/// the `judged` arm (its `declare_reconciler` contract: `judge_ref` +
/// `calibration_ref` carried).
fn judged_reconciler() -> ReconcilerDeclaration {
    ReconcilerDeclaration {
        reconciler_ref: "reconciler:exec-align".into(),
        classes_detected: DivergenceClass::ALL.iter().copied().collect(),
        detector_classes: [Detector::Deterministic, Detector::Judged]
            .into_iter()
            .collect(),
        probe_capabilities: vec![],
        requires_observability: BTreeSet::new(),
        applies_to: BTreeSet::new(),
        probe_budget_share_cap_ppm: None,
        conditioned_on: None,
        assumption_debt: None,
        judge_ref: Some("judge:live-1".into()),
        calibration_ref: Some("calib:pair.v1".into()),
        independence_summary: Some("snapshot=different_family".into()),
        no_progress_k: 2,
        enabled: true,
    }
}

fn consult(judge_ref: &str, calibration_ref: Option<&str>) -> JudgedConsult {
    JudgedConsult {
        judge_ref: judge_ref.to_string(),
        calibration_ref: calibration_ref.map(str::to_string),
        independence_summary: "snapshot=different_family context=fresh".into(),
        assessment: JudgedAssessment {
            classes: vec![DivergenceClass::EvidenceInversion],
            evidence_refs: vec!["blob:judge-io".into()],
            confidence_ppm: 800_000,
            agree: false,
        },
    }
}

// ── the drain seam ──────────────────────────────────────────────────────

/// A live binding's consult buffers `verification.*` rows; the driver's
/// `drain_judge_records` lands them under the run's fenced writer right
/// after the interpret guard — durable-before-visible, ordered before
/// the run's later rows. Removal-sensitive: without the drain the
/// buffered verdict never reaches the prefix.
#[test]
fn judge_consult_records_drain_through_the_fence() {
    let mut sink = MemSink::new();
    let judge = Arc::new(ScriptedJudge {
        answer: 0,
        buffered: std::sync::Mutex::new(vec![(
            "verification.validator.verdict".to_string(),
            Json::obj([
                ("verdict_id", Json::str("verdict:j1")),
                ("validator_ref", Json::str("validator:judge-live")),
                ("detector", Json::str("judged")),
                ("status", Json::str("decided")),
                ("value", Json::Int(910_000)),
                ("charged_to", Json::str("instrument")),
            ]),
            ProvenanceRecord::minted(
                hharness_provenance::origin::Origin::model("judge-model", "run", "judge.0"),
                hharness_provenance::PersistenceScope::Run,
                0,
            ),
        )]),
        ..ScriptedJudge::default()
    });
    let mut driver = Driver::open(
        ScriptedStrategy::new(vec![]),
        &ctx(),
        judged_policy(),
        &mut sink,
        DriverConfig {
            surfaces: vec![submit_surface()],
            judge: Some(judge.clone()),
            ..DriverConfig::default()
        },
    )
    .unwrap();
    let mut model = ScriptedModel {
        script: [submit_call()].into_iter().collect(),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler;
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    assert!(r.is_ok(), "run failed: {r:?}");
    // The consult happened (the judged loop spec's cadence-1 consult).
    assert!(
        judge
            .consults_seen
            .load(std::sync::atomic::Ordering::Relaxed)
            >= 1,
        "judge was never consulted"
    );
    // …and its buffered verdict row landed durable on the prefix.
    let verdicts = sink.find("verification.validator.verdict");
    assert!(
        verdicts
            .iter()
            .any(|e| e.payload.get("validator_ref").and_then(Json::as_str)
                == Some("validator:judge-live")
                && e.payload.get("detector").and_then(Json::as_str) == Some("judged")),
        "the judge's buffered verdict never landed: {:?}",
        sink.classes()
    );
}

/// A buffered `hh.model_io/1` blob against a seam-less sink is a contract
/// violation — surfaced as a driver append failure, never silently
/// dropped (the `ModelPort::take_model_io` convention's judge half).
#[test]
fn judge_model_io_against_seamless_sink_surfaces() {
    let mut sink = MemSink::new();
    let judge = Arc::new(ScriptedJudge {
        answer: 0,
        buffered_io: std::sync::Mutex::new(vec![("hh.model_io/1".to_string(), b"{}".to_vec())]),
        ..ScriptedJudge::default()
    });
    let mut driver = Driver::open(
        ScriptedStrategy::new(vec![]),
        &ctx(),
        judged_policy(),
        &mut sink,
        DriverConfig {
            surfaces: vec![submit_surface()],
            judge: Some(judge),
            ..DriverConfig::default()
        },
    )
    .unwrap();
    let mut model = ScriptedModel {
        script: [submit_call()].into_iter().collect(),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler;
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    let err = r.expect_err("a seam-less model_io drain must surface");
    assert!(
        format!("{err:?}").contains("judge model_io"),
        "unexpected failure: {err:?}"
    );
}

// ── the judged reconciler arm ──────────────────────────────────────────

/// The declared `judged` arm at completion: each `JudgedConsult` folds
/// `reconcile_judged` — `detector: judged` records under the critic's own
/// `detector_ref` carrying `calibration_ref` + `independence_summary`,
/// record ids disambiguated per critic (two critics asserting the same
/// class mint two records). Judged records are veto-admissible evidence,
/// never hold-admissible — the run still finishes `succeeded`-class, its
/// gate decided by the deterministic fold alone (F7).
#[test]
fn judged_reconciler_arm_folds_per_critic_records_at_completion() {
    let mut sink = MemSink::new();
    let judge = Arc::new(ScriptedJudge {
        consults: vec![
            consult("judge:live-1", Some("calib:local.v1")),
            consult("judge:live-2", Some("calib:hosted.v1")),
            // An uncalibrated consult — the driver declines
            // `reconcile_judged` for it (`JudgedWithoutCalibration`, never
            // a fabricated ref); its verdict rows would still land via
            // `take_records`, but no `claim.reconciled` mints.
            consult("judge:uncal-3", None),
        ],
        ..ScriptedJudge::default()
    });
    let mut driver = Driver::open(
        ScriptedStrategy::new(vec![]),
        &ctx(),
        EnvelopePolicy::stage1_default("b-1").seal().unwrap(),
        &mut sink,
        DriverConfig {
            surfaces: vec![submit_surface()],
            judge: Some(judge),
            reconciler: Some(judged_reconciler()),
            ..DriverConfig::default()
        },
    )
    .unwrap();
    let mut model = ScriptedModel {
        script: [submit_call()].into_iter().collect(),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler;
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    assert!(r.is_ok(), "run failed: {r:?}");
    assert!(
        sink.classes().contains(&"lifecycle.run.finished"),
        "run did not finish: {:?}",
        sink.classes()
    );
    let judged: Vec<&EventEnvelope> = sink
        .find("verification.claim.reconciled")
        .into_iter()
        .filter(|e| e.payload.get("detector").and_then(Json::as_str) == Some("judged"))
        .collect();
    // Exactly the two calibrated consults mint records (each asserting
    // `evidence_inversion`); the uncalibrated consult mints none.
    assert_eq!(
        judged.len(),
        2,
        "expected one judged record per calibrated critic: {:?}",
        judged.iter().map(|e| &e.payload).collect::<Vec<_>>()
    );
    let detector_refs: BTreeSet<String> = judged
        .iter()
        .filter_map(|e| {
            e.payload
                .get("detector_ref")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .collect();
    assert_eq!(
        detector_refs,
        ["judge:live-1", "judge:live-2"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        "each judged record names its own critic: {detector_refs:?}"
    );
    // The record ids disambiguate by critic — same class, two records.
    let record_ids: Vec<String> = judged
        .iter()
        .filter_map(|e| {
            e.payload
                .get("record_id")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .collect();
    assert!(
        record_ids.iter().all(|id| id.contains(":judge:live-")),
        "record ids carry the critic ref: {record_ids:?}"
    );
    // Judged records carry their own calibration + independence — the
    // consult's members, not the declaration's.
    for e in &judged {
        let det_ref = e
            .payload
            .get("detector_ref")
            .and_then(Json::as_str)
            .unwrap_or_default();
        let expected_calib = if det_ref == "judge:live-1" {
            "calib:local.v1"
        } else {
            "calib:hosted.v1"
        };
        assert_eq!(
            e.payload.get("calibration_ref").and_then(Json::as_str),
            Some(expected_calib),
            "the member's own calibration rides its record"
        );
        assert_eq!(
            e.payload.get("independence_summary").and_then(Json::as_str),
            Some("snapshot=different_family context=fresh"),
        );
        // F7 — veto-admissible, never hold-admissible: the gate's
        // deterministic fold never reads a judged divergence.
        assert_eq!(
            e.payload
                .get("hold_admissible")
                .or_else(|| e.payload.get("veto_admissible")),
            None,
            "the judged record exposes no admissibility flag by name"
        );
    }
    // The critics' own assess verdict rows landed durable — and BEFORE
    // the `claim.reconciled` rows that cite them (the completion-gate
    // drain's durable-before-visible order: remove the drain and these
    // rows never land).
    let assess_verdicts: Vec<u64> = sink
        .find("verification.validator.verdict")
        .into_iter()
        .filter(|e| e.payload.get("detector").and_then(Json::as_str) == Some("judged"))
        .map(|e| e.seq)
        .collect();
    assert!(
        !assess_verdicts.is_empty(),
        "the assess consults' verdict rows never landed: {:?}",
        sink.classes()
    );
    let first_judged_seq = judged
        .iter()
        .map(|e| e.seq)
        .min()
        .expect("judged rows exist");
    assert!(
        assess_verdicts.iter().all(|s| *s < first_judged_seq),
        "an assess verdict landed after the reconciled rows that cite it"
    );
    // And the judged divergences never held the run — the finished row
    // stands on the deterministic fold (F7).
    let finished = sink.find("lifecycle.run.finished");
    assert_eq!(finished.len(), 1);
}

/// `RecordedJudge` as the bound port — the replay shape — abstains from
/// `assess` and drains nothing: no `detector: judged` reconciled row and
/// no buffered `verification.validator.*` mints. Replay stays
/// byte-identical — the recorded rows are already the prefix's own.
#[test]
fn recorded_judge_replay_abstains_and_drains_nothing() {
    let mut sink = MemSink::new();
    let judge = Arc::new(hharness_control::loops::RecordedJudge::default());
    let mut driver = Driver::open(
        ScriptedStrategy::new(vec![]),
        &ctx(),
        EnvelopePolicy::stage1_default("b-1").seal().unwrap(),
        &mut sink,
        DriverConfig {
            surfaces: vec![submit_surface()],
            judge: Some(judge),
            reconciler: Some(judged_reconciler()),
            ..DriverConfig::default()
        },
    )
    .unwrap();
    let mut model = ScriptedModel {
        script: [submit_call()].into_iter().collect(),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler;
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    assert!(r.is_ok(), "run failed: {r:?}");
    assert!(
        !sink.find("verification.claim.reconciled").iter().any(|e| e
            .payload
            .get("detector")
            .and_then(Json::as_str)
            == Some("judged")),
        "a replay binding mints no judged records — it folds the recorded ones"
    );
    assert!(
        sink.find("verification.validator.verdict").is_empty(),
        "a replay binding mints no verdict rows"
    );
}
