//! S3.10 — the Stage-3 control/verification executable battery
//! (docs/tickets/056_S3.10; spec §5e.1–§5e.2, §5f.1–§5f.2):
//!
//! - The completion gate in the driver loop (§5f.2 §3):
//!   `stop{completed}` → `verification.completion.proposed` →
//!   `claim.recorded` → `claim.reconciled` → `gate.evaluated` →
//!   `completion.decided` → `lifecycle.run.finished{status,
//!   verification_summary_ref}`; `hold` feeds `guard_fired{completion_
//!   refused}` back to β bounded by `reconciliation.holds` (F4 —
//!   exhaustion decides `budget_exhausted{reconciliation.holds}`);
//!   honest failure (`unachievable`) ⇒ `failed_honest`; `unverifiable`
//!   policy ⇒ `succeeded_unverified`; `abandoned` ⇒ `succeeded_with_veto`
//!   (AC-R-2.7.1-7, AC-R-2.7.2a-4/5).
//! - `plan_execute` — the `model_emitted_plan` variant: a validated call
//!   on `hh.plan` emits `control.plan.emitted{valid, steps}`; the
//!   strategy sequences the steps; an invalid plan re-plans bounded by
//!   `max_continue_nudges` then `stop{format_failure}` (R-2.6.1, AC-7).
//! - AC-F2 under recorded `model_io` (DF-S1.20-2): every scenario's
//!   produced prefix is extracted (`extract_recorded`) and re-driven
//!   (`deterministic_replay`) — the replay reproduces the recorded
//!   `control.decision`/`context.assembled` sequences exactly or reports
//!   a divergence (AC-R-2.2.1-4 reproduction, AC-R-2.6.2-1/2/5/8).
//! - T-LCD-03 (AC-R-2.6.1-2): the paired `react/minimal` runs over the
//!   same recorded `model_io` reproduce identical decision sequences —
//!   the driver-level anchor the eval-plane `equivalence_run` consumes.

use std::collections::{BTreeMap, VecDeque};

use hh_control::driver::{
    AssembledRequest, AssemblerPort, Driver, DriverConfig, EffectGate, GateOutcome, LedgerSink,
    ModelOutcome, ModelPort, VerifyPort,
};
use hh_control::output::{ParamKind, ParamSpec, ParsedCall, SurfaceSpec};
use hh_control::plan_exec::{PlanExecute, PLAN_SURFACE_ID};
use hh_control::policy::EnvelopePolicy;
use hh_control::react::ReactMinimal;
use hh_control::replay::{deterministic_replay, extract_recorded};
use hh_control::strategy::{ControlContext, ControlStrategy, PlanProvenance, StrategyParams};
use hh_control::vocab::SettledOutcome;
use hh_ledger::classes::Durability;
use hh_ledger::event::{Event, EventEnvelope, EventPlane};
use hh_ledger::manifest::ParticipantClass;
use hh_ontology::control::StopReason;
use hh_wire::json::Json;

// ── test doubles (the deterministic Stage-3 ports) ──────────────────────

/// The in-memory sink — `prefix()` is the fold input.
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
    fn count(&self, class: &str) -> usize {
        self.events.iter().filter(|e| e.class == class).count()
    }
    fn find(&self, class: &str) -> Vec<&EventEnvelope> {
        self.events.iter().filter(|e| e.class == class).collect()
    }
}

impl LedgerSink for MemSink {
    fn append(&mut self, events: Vec<Event>) -> Result<(), String> {
        for e in events {
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

/// A scripted model port — pops outcomes in order (an exhausted script
/// serves `end_turn`/empty, the honest no-call shape).
struct ScriptedModel {
    script: VecDeque<ModelOutcome>,
    calls: u64,
}

impl ModelPort for ScriptedModel {
    fn call(&mut self, _id: &str, _req: &Json) -> ModelOutcome {
        self.calls += 1;
        self.script.pop_front().unwrap_or(ModelOutcome {
            stop_reason: hh_gateway::vocab::StopReason::EndTurn,
            response_ref: "r-empty".into(),
            text_empty: true,
            calls: vec![],
            error_class: None,
            retry_after_ms: None,
        })
    }
}

/// A scripted gate — per-surface outcomes plus the `finish` record the
/// claim extractor reads (`stop_rule = submit` detection).
struct ScriptedGate {
    by_surface: BTreeMap<String, GateOutcome>,
    fallback: GateOutcome,
    finish: Option<Json>,
}

impl ScriptedGate {
    fn observed() -> Self {
        ScriptedGate {
            by_surface: BTreeMap::new(),
            fallback: GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: None,
                error_class: None,
            },
            finish: None,
        }
    }
    /// The `hh.submit` completion surface answers `observed` + the
    /// submission marker; everything else falls back.
    fn with_submit(mut self) -> Self {
        self.by_surface.insert(
            "hh.submit".into(),
            GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: Some("sub-1".into()),
                error_class: None,
            },
        );
        self
    }
    fn with_surface(mut self, surface: &str, out: GateOutcome) -> Self {
        self.by_surface.insert(surface.to_string(), out);
        self
    }
}

impl EffectGate for ScriptedGate {
    fn dispatch(&mut self, _ef: &str, _a: u64, intent: &Json) -> GateOutcome {
        let surface = intent
            .get("surface_id")
            .or_else(|| intent.get("surface"))
            .and_then(Json::as_str)
            .unwrap_or("");
        self.by_surface
            .get(surface)
            .cloned()
            .unwrap_or_else(|| self.fallback.clone())
    }
    fn finish_record(&self) -> Option<Json> {
        self.finish.clone()
    }
}

struct NullAssembler;
impl AssemblerPort for NullAssembler {
    fn assemble(&mut self, req: &Json) -> AssembledRequest {
        AssembledRequest {
            request: req.clone(),
            assembled_payload: Some(Json::obj([("context_request", req.clone())])),
        }
    }
}

/// A scripted `VerifyPort` — returns the verdicts it was seeded with
/// (deterministic-detector verdicts only — a `judged` verdict is never a
/// gate fact, F7).
struct ScriptedVerify {
    verdicts: Vec<hh_verification::validators::Verdict>,
}

impl VerifyPort for ScriptedVerify {
    fn verify(
        &mut self,
        _refs: &[String],
        _subject: &Json,
    ) -> Vec<hh_verification::validators::Verdict> {
        self.verdicts.clone()
    }
}

fn ctx() -> ControlContext {
    ControlContext {
        process_ref: "proc-1".into(),
        plan: vec![],
        boundary: hh_control::react::react_preset(),
        profile: Json::Null,
        account_ref: "acct".into(),
        budget_ref: "b-1".into(),
        envelope_ref: "env-1".into(),
        parameters: StrategyParams::default(),
        capabilities_available: vec![],
        steering: (
            hh_control::strategy::SteerMode::Unsupported,
            hh_control::strategy::ConcurrentInput::QueueOnly,
        ),
    }
}


/// The `hh.plan` plan surface — `steps` is the closed-schema plan arg.
fn plan_surface() -> SurfaceSpec {
    SurfaceSpec {
        surface_id: PLAN_SURFACE_ID.into(),
        semantic_id: "sem/hh.plan".into(),
        params: [(
            "steps".into(),
            ParamSpec {
                required: true,
                kind: ParamKind::Arr,
                enum_values: vec![],
                domain: vec![],
            },
        )]
        .into_iter()
        .collect(),
    }
}

fn fs_read() -> SurfaceSpec {
    SurfaceSpec {
        surface_id: "fs.read".into(),
        semantic_id: "sem/fs.read".into(),
        params: [(
            "path".into(),
            ParamSpec {
                required: true,
                kind: ParamKind::Str,
                enum_values: vec![],
                domain: vec![],
            },
        )]
        .into_iter()
        .collect(),
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
    }
}

fn call(surface: &str, args: &str) -> ModelOutcome {
    ModelOutcome {
        stop_reason: hh_gateway::vocab::StopReason::ToolUse,
        response_ref: "r-1".into(),
        text_empty: false,
        calls: vec![ParsedCall {
            tool_call_id: format!("tc-{surface}"),
            surface: surface.into(),
            args_raw: args.into(),
        }],
        error_class: None,
        retry_after_ms: None,
    }
}

fn submit_call() -> ModelOutcome {
    call("hh.submit", r#"{"text":"done"}"#)
}

/// The contract fixture — `criteria`/`invariants` are `AcceptanceCriterion`
/// records; `validator_ref` is what `require_validator(…)` actions name.
fn contract(criteria: Vec<hh_verification::gate::AcceptanceCriterion>) -> hh_verification::gate::TaskContract {
    hh_verification::gate::TaskContract {
        contract_id: hh_verification::gate::TaskContract::compute_contract_id("goal-1", &criteria),
        goal_ref: "goal-1".into(),
        criteria,
        invariants: vec![],
        completion_policy: hh_verification::vocab::CompletionPolicy::AllRequired,
        evidence_kinds_required: vec![],
        budget_ref: "b-1".into(),
        sealed: true,
    }
}

fn criterion(id: &str, validator: &str) -> hh_verification::gate::AcceptanceCriterion {
    hh_verification::gate::AcceptanceCriterion {
        criterion_id: id.into(),
        validator_ref: validator.into(),
        record: hh_verification::gate::default_validates_record(),
    }
}

fn decided_pass_verdict(criterion_ref: &str, validator: &str) -> hh_verification::validators::Verdict {
    use hh_verification::vocab::*;
    hh_verification::validators::Verdict {
        verdict_id: format!("verdict:{criterion_ref}"),
        validator_ref: hh_identity::refs::VersionedRef::pinned(
            hh_identity::kinds::RecordKind::Validator,
            validator,
            hh_provenance::ProvenanceRecord::kernel("hh-test/verify", 0),
        ),
        oracle_class: OracleClass::Executable,
        target: "run".into(),
        criterion_ref: Some(criterion_ref.into()),
        contract_id: None,
        phase: VerdictPhase::Completion,
        role: CriterionRole::Acceptance,
        value: VerdictValue::Bool(true),
        status: VerdictStatus::Decided,
        detector: Detector::Deterministic,
        evidence_refs: vec![],
        inputs_digest: "sha256:dd".into(),
        evidence_head_seq: 0,
        freshness_ok: true,
        findings: vec![],
        cost_ppm: 0,
        charged_to: ChargedTo::Subject,
        veto_tripped: vec![],
        provenance: hh_provenance::ProvenanceRecord::kernel("hh-test/verify", 0),
        measured_at: 0,
    }
}

// ── the completion gate (§5f.2) ─────────────────────────────────────────

/// AC-R-2.7.1-7 / AC-R-2.7.2a-2 — a clean `stop{completed}` runs the full
/// chain: proposed → claim.recorded → claim.reconciled → gate.evaluated{pass}
/// → completion.decided{succeeded} → finished{status, summary_ref}.
#[test]
fn gate_pass_emits_the_full_verification_chain() {
    let mut sink = MemSink::new();
    let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
    let mut driver = Driver::open_react(
        &ctx(),
        policy,
        &mut sink,
        DriverConfig {
            surfaces: vec![submit_surface()],
            ..DriverConfig::default()
        },
    )
    .unwrap();
    let mut model = ScriptedModel {
        script: [submit_call()].into_iter().collect(),
        calls: 0,
    };
    let mut gate = ScriptedGate::observed().with_submit();
    gate.finish = Some(Json::obj([("completion", Json::str("achieved"))]));
    let mut asm = NullAssembler;
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink).unwrap();
    assert_eq!(r.report.stop_reason, StopReason::Completed);

    let classes = sink.classes();
    for required in [
        "verification.completion.proposed",
        "verification.claim.recorded",
        "verification.claim.reconciled",
        "verification.gate.evaluated",
        "verification.completion.decided",
        "lifecycle.run.finished",
    ] {
        assert!(classes.contains(&required), "missing {required}");
    }
    // The order — the gate's chain lands before the run's terminal row.
    let pos = |c: &str| classes.iter().position(|x| *x == c).unwrap();
    assert!(pos("verification.completion.proposed") < pos("verification.claim.recorded"));
    assert!(pos("verification.claim.recorded") < pos("verification.claim.reconciled"));
    assert!(pos("verification.claim.reconciled") < pos("verification.gate.evaluated"));
    assert!(pos("verification.gate.evaluated") < pos("verification.completion.decided"));
    assert!(pos("verification.completion.decided") < pos("lifecycle.run.finished"));

    let ge = sink.find("verification.gate.evaluated")[0];
    assert_eq!(
        ge.payload.get("verdict").and_then(Json::as_str),
        Some("pass")
    );
    let cd = sink.find("verification.completion.decided")[0];
    assert_eq!(
        cd.payload.get("status").and_then(Json::as_str),
        Some("succeeded")
    );
    let fin = sink.find("lifecycle.run.finished")[0];
    assert_eq!(
        fin.payload.get("status").and_then(Json::as_str),
        Some("succeeded")
    );
    assert!(fin.payload.get("verification_summary_ref").is_some());
}

/// AC-R-2.7.2a-4/5 — an `achieved` claim with a still-`unknown` effect is a
/// D6 hold; the `completion_refused` cue re-drives the strategy; the
/// `reconciliation.holds` cap bounds the loop — exhaustion decides
/// `budget_exhausted{reconciliation.holds}` (stratum `unreconciled_claims`)
/// with exactly `holds_cap` `gate.evaluated{hold}` rows and no further
/// model call.
#[test]
fn gate_hold_loops_then_exhausts_reconciliation_holds() {
    let mut sink = MemSink::new();
    let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
    let mut driver = Driver::open_react(
        &ctx(),
        policy,
        &mut sink,
        DriverConfig {
            surfaces: vec![fs_read(), submit_surface()],
            ..DriverConfig::default()
        },
    )
    .unwrap();
    // fs.read settles `unknown{awaiting_host}` — non-terminal for the gate
    // (D6), settled for the driver's dispatch cue.
    let mut model = ScriptedModel {
        script: [
            call("fs.read", r#"{"path":"/a"}"#),
            submit_call(),
            submit_call(),
            submit_call(),
            submit_call(),
            submit_call(),
        ]
        .into_iter()
        .collect(),
        calls: 0,
    };
    let mut gate = ScriptedGate::observed().with_submit().with_surface(
        "fs.read",
        GateOutcome {
            outcome: SettledOutcome::Unknown {
                cause: "awaiting_host".into(),
            },
            submission_ref: None,
            error_class: None,
        },
    );
    gate.finish = Some(Json::obj([("completion", Json::str("achieved"))]));
    let mut asm = NullAssembler;
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink).unwrap();
    assert_eq!(
        r.report.stop_reason,
        StopReason::BudgetExhausted {
            budget_id: "b-1".into(),
            dimension: hh_ontology::dimensions::DimensionId::ReconciliationHolds,
        }
    );
    // Exactly `DEFAULT_HOLDS_CAP` hold rows — the cap evaluation decides
    // without a fourth `gate.evaluated`.
    let holds = sink
        .find("verification.gate.evaluated")
        .iter()
        .filter(|e| e.payload.get("verdict").and_then(Json::as_str) == Some("hold"))
        .count();
    assert_eq!(
        holds as u64,
        hh_verification::gate::DEFAULT_HOLDS_CAP,
        "exactly the cap of holds lands"
    );
    // The hold's divergence + required action are durable.
    let hold = sink
        .find("verification.gate.evaluated")
        .into_iter()
        .find(|e| e.payload.get("verdict").and_then(Json::as_str) == Some("hold"))
        .unwrap();
    let divs: Vec<&str> = match hold.payload.get("divergences") {
        Some(Json::Arr(a)) => a.iter().filter_map(Json::as_str).collect(),
        _ => vec![],
    };
    assert!(divs.iter().any(|d| d.contains("open_effect")));
    // `completion_refused` rode the inbox as the guard audit row.
    assert!(sink.events.iter().any(|e| {
        e.class == "control.guard.fired"
            && e.payload.get("guard_id").and_then(Json::as_str) == Some("completion_refused")
    }));
    // The decision row — `budget_exhausted` + the unreconciled stratum.
    let cd = sink.find("verification.completion.decided")[0];
    assert_eq!(
        cd.payload.get("status").and_then(Json::as_str),
        Some("budget_exhausted")
    );
    assert_eq!(
        cd.payload.get("stratum").and_then(Json::as_str),
        Some("unreconciled_claims")
    );
    // No further model call past the holds+1 submissions + the read.
    let calls = sink.count("model.call.requested");
    assert_eq!(calls, model.calls as usize);
    assert_eq!(calls, 5, "1 read + 4 submit proposals then exhaustion");
}

/// F5 — an `unachievable` claim reconciled `unverifiable`/`agree` is the
/// honest-failure channel: `pass` + `honest_failure` ⇒ `failed_honest`,
/// never held and never `succeeded`.
#[test]
fn gate_honest_failure_on_unachievable_claim() {
    let mut sink = MemSink::new();
    let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
    let mut driver = Driver::open_react(
        &ctx(),
        policy,
        &mut sink,
        DriverConfig {
            surfaces: vec![submit_surface()],
            ..DriverConfig::default()
        },
    )
    .unwrap();
    let mut model = ScriptedModel {
        script: [submit_call()].into_iter().collect(),
        calls: 0,
    };
    let mut gate = ScriptedGate::observed().with_submit();
    gate.finish = Some(Json::obj([
        ("completion", Json::str("unachievable")),
        ("reason", Json::str("capability_absent")),
    ]));
    let mut asm = NullAssembler;
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink).unwrap();
    assert_eq!(r.report.stop_reason, StopReason::Completed);
    let cd = sink.find("verification.completion.decided")[0];
    assert_eq!(
        cd.payload.get("status").and_then(Json::as_str),
        Some("failed_honest")
    );
    assert_eq!(
        cd.payload.get("stratum").and_then(Json::as_str),
        Some("failed_honestly")
    );
    let fin = sink.find("lifecycle.run.finished")[0];
    assert_eq!(
        fin.payload.get("status").and_then(Json::as_str),
        Some("failed_honest")
    );
    // Never held, never succeeded.
    assert!(sink
        .find("verification.gate.evaluated")
        .iter()
        .all(|e| e.payload.get("verdict").and_then(Json::as_str) == Some("pass")));
}

/// AC-R-2.7.1-7 — the `unverifiable` completion policy decides
/// `succeeded_unverified` (the contract's call, never `succeeded`), and an
/// `unverifiable_reason` criterion never causes a hold.
#[test]
fn gate_unverifiable_policy_decides_succeeded_unverified() {
    let mut sink = MemSink::new();
    let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
    let mut c = contract(vec![criterion("crit-unv", "validator:v-1")]);
    c.completion_policy =
        hh_verification::vocab::CompletionPolicy::Unverifiable("no_detector".into());
    let mut driver = Driver::open_react(
        &ctx(),
        policy,
        &mut sink,
        DriverConfig {
            surfaces: vec![submit_surface()],
            task_contract: Some(c),
            ..DriverConfig::default()
        },
    )
    .unwrap();
    let mut model = ScriptedModel {
        script: [submit_call()].into_iter().collect(),
        calls: 0,
    };
    let mut gate = ScriptedGate::observed().with_submit();
    gate.finish = Some(Json::obj([("completion", Json::str("achieved"))]));
    let mut asm = NullAssembler;
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink).unwrap();
    for e in &sink.events {
        eprintln!("seq={} class={} payload={}", e.seq, e.class, e.payload.to_canonical_string());
    }
    assert_eq!(r.report.stop_reason, StopReason::Completed);
    // The unverifiable criterion is `Unrun` but declares
    // `unverifiable_reason` — no hold, no veto.
    assert_eq!(
        sink.find("verification.gate.evaluated")[0]
            .payload
            .get("verdict")
            .and_then(Json::as_str),
        Some("pass")
    );
    assert_eq!(
        sink.find("verification.completion.decided")[0]
            .payload
            .get("status")
            .and_then(Json::as_str),
        Some("succeeded_unverified")
    );
}

/// AC-R-2.7.2a-3 — a `required` criterion `unrun`/`unmet` with a bound
/// validator holds `require_validator(ref)`; `plan_execute` answers the
/// refused completion with a `verify` decision, the verdict lands, and
/// the next `stop{completed}` passes — the full hold→repair→decide loop.
#[test]
fn gate_unmet_criterion_holds_then_verify_repairs() {
    let mut sink = MemSink::new();
    let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
    let mut c = ctx();
    c.boundary = PlanExecute::new().capabilities().boundary_preset.clone();
    c.parameters.plan_provenance = PlanProvenance::ModelEmitted;
    c.parameters.max_continue_nudges = 1;
    let contract = contract(vec![criterion("crit-1", "validator:v-1")]);
    let mut driver = Driver::open(
        PlanExecute::new(),
        &c,
        policy,
        &mut sink,
        DriverConfig {
            surfaces: vec![submit_surface(), plan_surface()],
            task_contract: Some(contract),
            plan_surface_id: Some(PLAN_SURFACE_ID.into()),
            ..DriverConfig::default()
        },
    )
    .unwrap();
    driver.set_verify_port(Box::new(ScriptedVerify {
        verdicts: vec![decided_pass_verdict("crit-1", "validator:v-1")],
    }));
    // The model emits a one-step plan: act on `hh.submit`.
    let plan = r#"{"steps":[{"kind":"act","surface":"hh.submit","args":{"text":"done"}}]}"#;
    let mut model = ScriptedModel {
        script: [call(PLAN_SURFACE_ID, plan)].into_iter().collect(),
        calls: 0,
    };
    let mut gate = ScriptedGate::observed().with_submit();
    gate.finish = Some(Json::obj([("completion", Json::str("achieved"))]));
    let mut asm = NullAssembler;
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    for e in &sink.events {
        eprintln!("seq={} class={} payload={}", e.seq, e.class, e.payload.to_canonical_string());
    }
    let r = r.unwrap();
    assert_eq!(r.report.stop_reason, StopReason::Completed);

    // `control.plan.emitted{valid}` landed before the act.
    let pe = sink.find("control.plan.emitted")[0];
    assert_eq!(pe.payload.get("valid"), Some(&Json::Bool(true)));

    // One hold carrying `require_validator(validator:v-1)`…
    let holds: Vec<&EventEnvelope> = sink
        .find("verification.gate.evaluated")
        .into_iter()
        .filter(|e| e.payload.get("verdict").and_then(Json::as_str) == Some("hold"))
        .collect();
    assert_eq!(holds.len(), 1);
    let actions: Vec<&str> = match holds[0].payload.get("required_actions") {
        Some(Json::Arr(a)) => a.iter().filter_map(Json::as_str).collect(),
        _ => vec![],
    };
    assert_eq!(actions, vec!["require_validator(validator:v-1)"]);
    // …the verdict row landed (charged, deterministic, criterion-bound)…
    let verdicts = sink.find("verification.validator.verdict");
    assert_eq!(verdicts.len(), 1);
    assert_eq!(
        verdicts[0].payload.get("criterion_ref").and_then(Json::as_str),
        Some("crit-1")
    );
    // …and the second evaluation passed → `succeeded`.
    let cd = sink.find("verification.completion.decided")[0];
    assert_eq!(
        cd.payload.get("status").and_then(Json::as_str),
        Some("succeeded")
    );
    assert!(sink
        .find("verification.gate.evaluated")
        .iter()
        .any(|e| e.payload.get("verdict").and_then(Json::as_str) == Some("pass")));
}

/// F2(b) — an `abandoned` effect at an `achieved` claim is success-with-
/// veto (`inconsistent_durable_state`), never a hold and never clean.
#[test]
fn gate_abandoned_effect_is_success_with_veto() {
    let mut sink = MemSink::new();
    let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
    let mut driver = Driver::open_react(
        &ctx(),
        policy,
        &mut sink,
        DriverConfig {
            surfaces: vec![fs_read(), submit_surface()],
            ..DriverConfig::default()
        },
    )
    .unwrap();
    let mut model = ScriptedModel {
        script: [call("fs.read", r#"{"path":"/a"}"#), submit_call()]
            .into_iter()
            .collect(),
        calls: 0,
    };
    let mut gate = ScriptedGate::observed().with_submit().with_surface(
        "fs.read",
        GateOutcome {
            outcome: SettledOutcome::Abandoned,
            submission_ref: None,
            error_class: None,
        },
    );
    gate.finish = Some(Json::obj([("completion", Json::str("achieved"))]));
    let mut asm = NullAssembler;
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink).unwrap();
    assert_eq!(r.report.stop_reason, StopReason::Completed);
    assert_eq!(
        sink.find("verification.completion.decided")[0]
            .payload
            .get("status")
            .and_then(Json::as_str),
        Some("succeeded_with_veto")
    );
}

// ── plan_execute (R-2.6.1; AC-7) ────────────────────────────────────────

/// A model-emitted plan: the `hh.plan` call validates, `control.plan.
/// emitted{valid, steps}` lands, the `act` step dispatches, and the run
/// completes `succeeded`.
#[test]
fn plan_execute_valid_plan_executes_and_completes() {
    let mut sink = MemSink::new();
    let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
    let mut c = ctx();
    c.boundary = PlanExecute::new().capabilities().boundary_preset.clone();
    c.parameters.plan_provenance = PlanProvenance::ModelEmitted;
    let mut driver = Driver::open(
        PlanExecute::new(),
        &c,
        policy,
        &mut sink,
        DriverConfig {
            surfaces: vec![
                fs_read(),
                submit_surface(),
                plan_surface(),
            ],
            plan_surface_id: Some(PLAN_SURFACE_ID.into()),
            ..DriverConfig::default()
        },
    )
    .unwrap();
    let plan = r#"{"steps":[{"kind":"act","surface":"fs.read","args":{"path":"/a"}},{"kind":"act","surface":"hh.submit","args":{"text":"done"}}]}"#;
    let mut model = ScriptedModel {
        script: [call(PLAN_SURFACE_ID, plan)].into_iter().collect(),
        calls: 0,
    };
    let mut gate = ScriptedGate::observed().with_submit();
    gate.finish = Some(Json::obj([("completion", Json::str("achieved"))]));
    let mut asm = NullAssembler;
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink).unwrap();
    assert_eq!(r.report.stop_reason, StopReason::Completed);
    assert_eq!(
        r.report.submission_ref.as_deref(),
        Some("sub-1"),
        "the submit step's submission_ref rides the terminal report"
    );
    let pe = sink.find("control.plan.emitted")[0];
    assert_eq!(pe.payload.get("valid"), Some(&Json::Bool(true)));
    match pe.payload.get("steps") {
        Some(Json::Arr(s)) => assert_eq!(s.len(), 2),
        _ => panic!("steps member absent"),
    }
    // Both act steps dispatched — the ledger carries two intents.
    assert_eq!(sink.count("action.effect.intended"), 2);
    assert_eq!(
        sink.find("verification.completion.decided")[0]
            .payload
            .get("status")
            .and_then(Json::as_str),
        Some("succeeded")
    );
}

/// An invalid plan (a step naming an undeclared surface) lands
/// `control.plan.emitted{valid:false, reject_reason}` and the strategy
/// re-plans bounded by `max_continue_nudges` — exhaustion is
/// `stop{format_failure}`, never a silent skip.
#[test]
fn plan_execute_invalid_plan_replans_then_format_failure() {
    let mut sink = MemSink::new();
    let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
    let mut c = ctx();
    c.boundary = PlanExecute::new().capabilities().boundary_preset.clone();
    c.parameters.plan_provenance = PlanProvenance::ModelEmitted;
    c.parameters.max_continue_nudges = 1;
    let mut driver = Driver::open(
        PlanExecute::new(),
        &c,
        policy,
        &mut sink,
        DriverConfig {
            surfaces: vec![
                submit_surface(),
                plan_surface(),
            ],
            plan_surface_id: Some(PLAN_SURFACE_ID.into()),
            ..DriverConfig::default()
        },
    )
    .unwrap();
    // Every emitted plan names the undeclared `exec.shell` surface.
    let bad = r#"{"steps":[{"kind":"act","surface":"exec.shell","args":{}}]}"#;
    let mut model = ScriptedModel {
        script: std::iter::repeat_with(|| call(PLAN_SURFACE_ID, bad))
            .take(8)
            .collect(),
        calls: 0,
    };
    let mut gate = ScriptedGate::observed().with_submit();
    let mut asm = NullAssembler;
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink).unwrap();
    assert!(matches!(
        r.report.stop_reason,
        StopReason::FormatFailure { .. }
    ));
    // `valid:false` rows ledger the rejections — `surface_undeclared`.
    let emitted = sink.find("control.plan.emitted");
    assert!(!emitted.is_empty());
    assert!(emitted.iter().all(|e| e.payload.get("valid") == Some(&Json::Bool(false))));
    assert_eq!(
        emitted[0].payload.get("reject_reason").and_then(Json::as_str),
        Some("surface_undeclared")
    );
    // No effect was ever intended off a rejected plan.
    assert_eq!(sink.count("action.effect.intended"), 0);
}

/// The closed-schema half — a plan carrying an unknown member or step kind
/// is `valid:false`; a valid plan's `hh.plan` self-call is refused too.
#[test]
fn plan_validate_closed_schema_and_closed_world() {
    use hh_control::plan_exec::{plan_validate, PLAN_SURFACE_ID};
    let declared = vec![fs_read(), submit_surface(), plan_surface()];
    // Valid plan.
    assert!(plan_validate(
        r#"{"steps":[{"kind":"act","surface":"fs.read","args":{}}]}"#,
        &declared
    )
    .is_ok());
    // Unknown member / kind / undeclared surface / self-call / empty.
    for (raw, reason) in [
        (r#"{"steps":[{"kind":"act","surface":"fs.read","bogus":1}]}"#, "step_member_unknown"),
        (r#"{"steps":[{"kind":"teleport"}]}"#, "step_kind_unknown"),
        (r#"{"steps":[{"kind":"act","surface":"exec.shell"}]}"#, "surface_undeclared"),
        (
            r#"{"steps":[{"kind":"act","surface":"hh.plan","args":{}}]}"#,
            "surface_undeclared",
        ),
        (r#"{"steps":[]}"#, "steps_out_of_bounds"),
        (r#"{"bogus":1}"#, "no_steps"),
        ("not json", "unparseable"),
    ] {
        assert_eq!(plan_validate(raw, &declared), Err(reason), "{raw}");
    }
}

// ── recorded `model_io` replay (AC-F2 under record; T-LCD-03) ───────────

/// Drive a scenario to its terminal and return the produced prefix.
fn drive(
    script: Vec<ModelOutcome>,
    gate: ScriptedGate,
    config: DriverConfig,
    contract_ctx: Option<ControlContext>,
) -> (hh_control::driver::RunResult, Vec<EventEnvelope>) {
    let mut sink = MemSink::new();
    let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
    let ctx = contract_ctx.unwrap_or_else(ctx);
    let mut driver = Driver::open_react(&ctx, policy, &mut sink, config).unwrap();
    let mut model = ScriptedModel {
        script: script.into_iter().collect(),
        calls: 0,
    };
    let mut g = gate;
    let mut asm = NullAssembler;
    let r = loop {
        match driver.run(&mut model, &mut g, &mut asm, &mut sink) {
            Ok(r) => break r,
            Err(hh_control::driver::DriverError::Port { port, .. }) if port == "inbox" => {
                // Parked — nothing more to feed; the run suspended.
                panic!("run parked mid-battery — the script needs a cue");
            }
            Err(e) => panic!("driver error: {e:?}"),
        }
    };
    (r, sink.events)
}

/// The AC-F2 mechanics under recorded `model_io` — the produced prefix is
/// the recorded corpus; `deterministic_replay` must reproduce the
/// `control.decision`/`context.assembled` sequences verbatim (AC-R-2.2.1-4).
fn assert_replay_reproduces(
    ctx_cfg: &dyn Fn() -> (ControlContext, EnvelopePolicy, DriverConfig),
    recorded: &[EventEnvelope],
) {
    let (c, p, cfg) = ctx_cfg();
    let rec = extract_recorded(recorded, 0, None);
    let mut asm = NullAssembler;
    let out = deterministic_replay(
        ReactMinimal::new(),
        &c,
        p,
        cfg,
        &[],
        &rec,
        &mut asm,
        Some("hh.submit"),
        "replay",
    )
    .unwrap();
    assert!(
        out.reproduced(),
        "replay diverged: {:?}",
        out.diverged
    );
    assert!(out.run_result.is_some() || out.parked);
}

/// AC-F2-01 under recorded `model_io` — the loop ladder (nudge → deny →
/// `stop{loop_detected}`) is a recorded-corpus fact: the replay reproduces
/// the decision sequence exactly (deterministic guards, ADR-0106).
#[test]
fn ac_f2_01_loop_ladder_replays_identically() {
    let script: Vec<ModelOutcome> = std::iter::repeat_with(|| {
        call("fs.read", r#"{"path":"/same"}"#)
    })
    .take(30)
    .collect();
    let cfg = || {
        let mut cfg = DriverConfig {
            surfaces: vec![fs_read()],
            ..DriverConfig::default()
        };
        cfg.budget_ceiling.insert("model_calls".into(), 40);
        (ctx(), EnvelopePolicy::stage1_default("b-1").seal().unwrap(), cfg)
    };
    let (c, p, config) = cfg();
    let mut sink = MemSink::new();
    let mut driver = Driver::open_react(&c, p, &mut sink, config).unwrap();
    let mut model = ScriptedModel {
        script: script.into_iter().collect(),
        calls: 0,
    };
    let mut gate = ScriptedGate::observed();
    let mut asm = NullAssembler;
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink).unwrap();
    assert!(matches!(r.report.stop_reason, StopReason::LoopDetected { .. }));
    assert_replay_reproduces(&cfg, &sink.events);
}

/// AC-F2-02 under recorded `model_io` — the `model_calls` ceiling stops
/// `budget_exhausted{model_calls}` and the replay reproduces it.
#[test]
fn ac_f2_02_budget_ceiling_replays_identically() {
    let script: Vec<ModelOutcome> = std::iter::repeat_with(|| {
        call("fs.read", r#"{"path":"/a"}"#)
    })
    .take(20)
    .collect();
    let cfg = || {
        let mut cfg = DriverConfig {
            surfaces: vec![fs_read()],
            ..DriverConfig::default()
        };
        cfg.budget_ceiling.insert("model_calls".into(), 4);
        (ctx(), EnvelopePolicy::stage1_default("b-1").seal().unwrap(), cfg)
    };
    let (c, p, config) = cfg();
    let mut sink = MemSink::new();
    let mut driver = Driver::open_react(&c, p, &mut sink, config).unwrap();
    let mut model = ScriptedModel {
        script: script.into_iter().collect(),
        calls: 0,
    };
    let mut gate = ScriptedGate::observed();
    let mut asm = NullAssembler;
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink).unwrap();
    assert!(matches!(
        r.report.stop_reason,
        StopReason::BudgetExhausted { .. } | StopReason::LoopDetected { .. }
    ));
    assert_replay_reproduces(&cfg, &sink.events);
}

/// AC-F2-05 under recorded `model_io` — the empty-call ladder stops
/// `format_failure` after `max_consecutive_format_errors`; the replay
/// reproduces.
#[test]
fn ac_f2_05_empty_call_ladder_replays_identically() {
    let script: Vec<ModelOutcome> = std::iter::repeat_with(|| ModelOutcome {
        stop_reason: hh_gateway::vocab::StopReason::ToolUse,
        response_ref: "r".into(),
        text_empty: true,
        calls: vec![],
        error_class: None,
        retry_after_ms: None,
    })
    .take(10)
    .collect();
    let cfg = || {
        (
            ctx(),
            EnvelopePolicy::stage1_default("b-1").seal().unwrap(),
            DriverConfig {
                surfaces: vec![fs_read()],
                ..DriverConfig::default()
            },
        )
    };
    let (c, p, config) = cfg();
    let mut sink = MemSink::new();
    let mut driver = Driver::open_react(&c, p, &mut sink, config).unwrap();
    let mut model = ScriptedModel {
        script: script.into_iter().collect(),
        calls: 0,
    };
    let mut gate = ScriptedGate::observed();
    let mut asm = NullAssembler;
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink).unwrap();
    assert!(matches!(
        r.report.stop_reason,
        StopReason::FormatFailure { .. }
    ));
    assert_replay_reproduces(&cfg, &sink.events);
}

/// T-LCD-03 (AC-R-2.6.1-2) — the paired `react/minimal` runs over the same
/// recorded `model_io` reproduce identical `control.decision` and
/// `context.assembled` sequences (the equivalence substrate the eval
/// plane's `equivalence_run` consumes — `hh-eval`'s S3.10 test does the
/// margins half).
#[test]
fn t_lcd_03_paired_replay_reproduces_identically() {
    // A two-call submission run — the recorded prefix is the corpus.
    let script = vec![
        call("fs.read", r#"{"path":"/a"}"#),
        submit_call(),
    ];
    let (_r, prefix) = drive(
        script,
        ScriptedGate::observed().with_submit(),
        DriverConfig {
            surfaces: vec![fs_read(), submit_surface()],
            ..DriverConfig::default()
        },
        None,
    );
    let rec = extract_recorded(&prefix, 0, None);
    let cfg = || {
        (
            ctx(),
            EnvelopePolicy::stage1_default("b-1").seal().unwrap(),
            DriverConfig {
                surfaces: vec![fs_read(), submit_surface()],
                ..DriverConfig::default()
            },
        )
    };
    // Twin runs — the recorded corpus drives both; the decision sequences
    // must match member-for-member (the equivalence kernel's C0 check).
    let mut asm_a = NullAssembler;
    let (c1, p1, cfg1) = cfg();
    let a = deterministic_replay(
        ReactMinimal::new(),
        &c1,
        p1,
        cfg1,
        &[],
        &rec,
        &mut asm_a,
        Some("hh.submit"),
        "replay-a",
    )
    .unwrap();
    let mut asm_b = NullAssembler;
    let (c2, p2, cfg2) = cfg();
    let b = deterministic_replay(
        ReactMinimal::new(),
        &c2,
        p2,
        cfg2,
        &[],
        &rec,
        &mut asm_b,
        Some("hh.submit"),
        "replay-b",
    )
    .unwrap();
    assert!(a.reproduced() && b.reproduced());
    assert_eq!(a.replayed_decisions, b.replayed_decisions);
    assert_eq!(a.replayed_assembled, b.replayed_assembled);
    assert_eq!(a.model_calls_served, b.model_calls_served);
    assert_eq!(a.dispatches, b.dispatches);
}
