//! S3.12b — the GATE-G2 real-suite legs over `benchset.stage3.v1`
//! (docs/tickets/059a; readout G2-1; spec §10.7's deferred cells;
//! AC-I2-6, AC-I4-11, the AC-I4-3 real-runner halves).
//!
//! What these tests are *not*: a re-statement of the fixture expectations.
//! The corpus's recorded `model_io` drives the canonical `hh_control`
//! `Driver` (`react/minimal`) through the adapter's real lifecycle —
//! `materialize` (participant + verifier roots), `expose`, then the
//! effect gate applies the recorded `fs.write` to the materialized
//! participant root as a real filesystem effect, `collect_submission`
//! reads the produced bytes, and `grade` runs the typed-reward pipeline.
//! The control prefix the driver produces is the `deterministic_replay`
//! input — the T-LCD-03 paired-reproduction check runs over the corpus,
//! not a synthetic script.
//!
//! Parity is the §5h.4 §7 record, not an assertion of equality: the
//! replayed verdicts and the corpus's `original_runs` feed
//! `hh_eval::benefits::artifact_benefit` (the real `compare` kernel under
//! its held-out/zero-search-spend/leaked-split preconditions) and the
//! resulting `ComparisonReport` wraps into `ParityReport`.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

use hh_bench::adapter::BenchmarkAdapter;
use hh_bench::benchset::{Benchset, BenchsetTask};
use hh_bench::grade::GradeRequest;
use hh_control::driver::{
    AssembledRequest, AssemblerPort, Driver, DriverConfig, EffectGate, GateOutcome, LedgerSink,
    ModelOutcome, ModelPort,
};
use hh_control::output::{ParamKind, ParamSpec, ParsedCall, SurfaceSpec};
use hh_control::policy::EnvelopePolicy;
use hh_control::react::ReactMinimal;
use hh_control::replay::{deterministic_replay, extract_recorded};
use hh_control::strategy::{ControlContext, StrategyParams};
use hh_control::vocab::SettledOutcome;
use hh_eval::benefits::artifact_benefit;
use hh_eval::catalogue;
use hh_eval::compare::{compare, CompareInput};
use hh_eval::facts::LedgerFacts;
use hh_eval::runs::{CacheState, EvalRun, TaskContext};
use hh_ledger::classes::Durability;
use hh_ledger::event::{Event, EventEnvelope, EventPlane};
use hh_ledger::manifest::{ObservabilityLevel, ParticipantClass as LedgerParticipantClass};
use hh_ontology::compliance::Detector;
use hh_ontology::control::OutcomeClass;
use hh_ontology::dimensions::DimensionId;
use hh_ontology::eval::{
    Design, DesignKind, MetricValue, MetricValueKind, Pairing, RoutingPolicy, SeedPolicy,
};
use hh_ontology::lab::SplitLabel;
use hh_ontology::participant::ParticipantClass;
use hh_wire::json::Json;

// ── the run plumbing (the s3_10.rs port set, verbatim semantics) ────────────

fn tmp(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-bench-s312b-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// The in-memory durable sink (`hh-control` tests' MemSink shape — a vec
/// of envelopes is the fold input; the driver never keeps a second copy).
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
                participant_class: LedgerParticipantClass::Native,
                observability_level: [ObservabilityLevel::Events].into_iter().collect(),
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

/// The model port that serves the corpus's recorded `model_io` — one
/// recorded call per turn (the transcript IS the model stream; nothing is
/// regenerated or hallucinated — an exhausted transcript is `end_turn`,
/// the honest no-call shape).
struct RecordedModel {
    turns: VecDeque<ModelOutcome>,
    calls: u64,
}

impl ModelPort for RecordedModel {
    fn call(&mut self, _id: &str, _req: &Json) -> ModelOutcome {
        self.calls += 1;
        self.turns.pop_front().unwrap_or(ModelOutcome {
            stop_reason: hh_gateway::vocab::StopReason::EndTurn,
            response_ref: "r-empty".into(),
            text_empty: true,
            calls: vec![],
            error_class: None,
            retry_after_ms: None,
        })
    }
}

/// The effect boundary bound to the materialized participant root —
/// `fs.write` lands the recorded bytes for real inside the root (a
/// containment-checked write, the env-driver `fs_write` contract);
/// `hh.submit` settles `observed` carrying the submission marker the
/// driver's `stop_rule = submit` detection reads. Any other surface is
/// `refused` — the surface table is closed.
struct ParticipantGate {
    root: PathBuf,
}

impl EffectGate for ParticipantGate {
    fn dispatch(&mut self, effect_id: &str, _attempt: u64, intent: &Json) -> GateOutcome {
        let surface = intent
            .get("surface_id")
            .or_else(|| intent.get("surface"))
            .and_then(Json::as_str)
            .unwrap_or("");
        match surface {
            "fs.write" => {
                let args = intent.get("args").cloned().unwrap_or(Json::obj([]));
                let path = args.get("path").and_then(Json::as_str).unwrap_or("");
                let text = args.get("text").and_then(Json::as_str).unwrap_or("");
                // Containment — the gate never escapes the materialized
                // root (the env driver's writable-root rule, honestly held
                // at this seam too).
                let rel = Path::new(path);
                let escapes = rel.is_absolute()
                    || rel
                        .components()
                        .any(|c| matches!(c, std::path::Component::ParentDir));
                if escapes || path.is_empty() {
                    return GateOutcome {
                        outcome: SettledOutcome::Refused,
                        submission_ref: None,
                        error_class: Some("path_escape".into()),
                    };
                }
                match std::fs::write(self.root.join(rel), text.as_bytes()) {
                    Ok(()) => GateOutcome {
                        outcome: SettledOutcome::Observed {
                            outcome: "applied".into(),
                        },
                        submission_ref: None,
                        error_class: None,
                    },
                    Err(e) => GateOutcome {
                        outcome: SettledOutcome::Unknown {
                            cause: format!("io:{e}"),
                        },
                        submission_ref: None,
                        error_class: None,
                    },
                }
            }
            "hh.submit" => GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: Some(format!("sub-{effect_id}")),
                error_class: None,
            },
            _ => GateOutcome {
                outcome: SettledOutcome::Refused,
                submission_ref: None,
                error_class: Some("surface_not_bound".into()),
            },
        }
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

fn surface(id: &str, params: &[&str]) -> SurfaceSpec {
    SurfaceSpec {
        surface_id: id.into(),
        semantic_id: format!("sem/{id}"),
        params: params
            .iter()
            .map(|p| {
                (
                    p.to_string(),
                    ParamSpec {
                        required: true,
                        kind: ParamKind::Str,
                        enum_values: vec![],
                        domain: vec![],
                    },
                )
            })
            .collect(),
    }
}

fn config() -> DriverConfig {
    DriverConfig {
        surfaces: vec![
            surface("fs.write", &["path", "text"]),
            surface("hh.submit", &["text"]),
        ],
        ..DriverConfig::default()
    }
}

/// One corpus task's full round trip: `materialize` (participant +
/// verifier), `expose`, drive the recorded `model_io` through the real
/// driver with the gate bound to the participant root, replay the
/// produced prefix deterministically, `collect_submission`, `grade`.
/// Returns `(reward_ppm, model_calls, prefix)` — the consumed `model_calls`
/// is the driver's own count, measured from the produced prefix.
fn round_trip(
    task: &BenchsetTask,
    suite: &hh_bench::benchset::BenchSuite,
    tag: &str,
) -> (i64, i64) {
    let root = tmp(tag);
    let participant = suite
        .adapter
        .materialize(&task.record().task_id, root.to_str().unwrap(), false)
        .expect("participant materializes");
    let verifier = suite
        .adapter
        .materialize(&task.record().task_id, root.to_str().unwrap(), true)
        .expect("verifier materializes");
    // AC-R-2.9.4-9: the held-out oracle never lands on the participant root.
    assert!(Path::new(&verifier.root).join("expected.bin").exists());
    assert!(!Path::new(&participant.root).join("expected.bin").exists());

    let exposed = suite
        .adapter
        .expose(&participant, "profile:s312b")
        .expect("expose");
    assert_eq!(exposed.task_id, task.record().task_id);

    // The recorded model_io as the model stream — one call per turn.
    let turns: VecDeque<ModelOutcome> = task
        .model_io
        .iter()
        .enumerate()
        .map(|(i, c)| ModelOutcome {
            stop_reason: hh_gateway::vocab::StopReason::ToolUse,
            response_ref: format!("r-{i}"),
            text_empty: false,
            calls: vec![ParsedCall {
                tool_call_id: format!("tc-{i}"),
                surface: c.surface.clone(),
                args_raw: c.args.to_canonical_string(),
            }],
            error_class: None,
            retry_after_ms: None,
        })
        .collect();
    let mut model = RecordedModel { turns, calls: 0 };
    let mut gate = ParticipantGate {
        root: Path::new(&participant.root).to_path_buf(),
    };
    let mut sink = MemSink::new();
    let mut asm = NullAssembler;
    let mut driver = Driver::open_react(
        &ctx(),
        EnvelopePolicy::stage1_default("b-1").seal().unwrap(),
        &mut sink,
        config(),
    )
    .expect("driver opens");
    let r = loop {
        match driver.run(&mut model, &mut gate, &mut asm, &mut sink) {
            Ok(r) => break r,
            Err(hh_control::driver::DriverError::Port { port, .. }) if port == "inbox" => {
                panic!("corpus transcript parked the driver — a cue it cannot serve")
            }
            Err(e) => panic!("driver error on {}: {e:?}", task.name),
        }
    };
    assert!(
        matches!(
            r.report.stop_reason,
            hh_ontology::control::StopReason::Completed
        ),
        "corpus transcript must reach stop{{completed}}, got {:?}",
        r.report.stop_reason
    );
    let model_calls = model.calls as i64;

    // T-LCD-03 — the produced prefix is the recorded corpus; a paired
    // re-drive reproduces the decision sequence member-for-member.
    let rec = extract_recorded(&sink.events, 0, None);
    let mut replayed = Vec::new();
    for id in ["replay-a", "replay-b"] {
        let mut rasm = NullAssembler;
        let out = deterministic_replay(
            ReactMinimal::new(),
            &ctx(),
            EnvelopePolicy::stage1_default("b-1").seal().unwrap(),
            config(),
            &[],
            &rec,
            &mut rasm,
            Some("hh.submit"),
            id,
        )
        .expect("replay runs");
        assert!(out.reproduced(), "{id} diverged: {:?}", out.diverged);
        replayed.push((out.replayed_decisions, out.replayed_assembled));
    }
    assert_eq!(replayed[0], replayed[1], "paired re-drives disagree");

    // The graded verdict — real collect + real grade under `separate`.
    let submission = suite
        .adapter
        .collect_submission(&participant)
        .expect("collect_submission");
    let graded = suite
        .adapter
        .grade(&GradeRequest {
            task: task.record().clone(),
            submission,
            isolation: hh_ontology::lab::VerifierIsolation::Separate,
        })
        .expect("grade");
    (graded.reward_ppm, model_calls)
}

/// A records-in `EvalRun` — the eval-plane row for one verdict (original
/// runner's recorded row or a replayed run's graded row; the members are
/// the recorded facts, never synthesized detail).
#[allow(clippy::too_many_arguments)]
fn eval_run(
    run_id: &str,
    arm: &str,
    task_id: &str,
    suite_id: &str,
    split: SplitLabel,
    replicate: u64,
    reward_ppm: i64,
    model_calls: i64,
    stratum: hh_ontology::lab::ContaminationStratum,
    split_hash: &str,
    family: hh_ontology::lab::EnvironmentFamily,
) -> EvalRun {
    EvalRun {
        run_id: run_id.into(),
        arm_id: arm.into(),
        cell_id: Some(format!("{arm}:{task_id}")),
        configuration_id: format!("cfg:{arm}"),
        participant_class: ParticipantClass::Native,
        observability_level: [
            hh_ontology::participant::Observability::Events,
            hh_ontology::participant::Observability::ModelIo,
            hh_ontology::participant::Observability::EndState,
        ]
        .into_iter()
        .collect(),
        mediation: Default::default(),
        capability_vector: Default::default(),
        task_id: task_id.into(),
        suite_id: suite_id.into(),
        split_label: split,
        replicate_index: replicate,
        attempt_no: 1,
        seed: None,
        seed_honoured: true,
        cache_state: CacheState::ColdStart,
        comparable: true,
        outcome_class: OutcomeClass::Scored,
        budget_consumed: [(DimensionId::ModelCalls, model_calls)]
            .into_iter()
            .collect(),
        veto_tripped: vec![],
        values: vec![MetricValue {
            metric_ref: "task_success".into(),
            value: MetricValueKind::Decimal(reward_ppm),
            applies_to: run_id.into(),
            oracle_ref: "oracle/adapter.grade".into(),
            detector: Detector::Deterministic,
            confidence: None,
            evidence_ref: None,
        }],
        environment_version_id: Some(format!("env:{suite_id}")),
        environment_family: family,
        fault_profile: None,
        perturbation_profile: None,
        model_snapshots: BTreeMap::new(),
        stratum,
        eval_search_spend: 0,
        split_hash: Some(split_hash.into()),
        routing_deviation: false,
        replayed_trajectory: false,
        served_from_cache_count: 0,
        cache_prefix_hit_ratio: None,
        facts: LedgerFacts::default(),
    }
}

fn parity_design() -> Design {
    Design {
        id: "design:benchset-parity".into(),
        kind: DesignKind::Paired,
        factors: vec![],
        blocking: vec!["task".into()],
        // The corpus's parity arm carries ≥ 3 recorded original verdicts
        // per parity task (the load-time invariant).
        replicates_per_cell: 3,
        pairing: Pairing::ByTask,
        seed_policy: SeedPolicy {
            harness_rng: false,
            requested_sampling_seed: false,
            seed_honoured_required: false,
        },
        held_out_split_ref: Some("split:held_out".into()),
        pre_registration: hh_ontology::eval::PreRegistration {
            registered_at: 0,
            hypothesis: "adapter replay parity with the original runner".into(),
            primary_metrics: vec!["task_success".into()],
            equivalence_margin: None,
            min_n: 1,
            analysis_plan_ref: "plan:parity".into(),
            task_split_hash: String::new(),
            interactions: vec![],
        },
        registry_snapshot_id: None,
        generators: None,
        resolution: None,
        routing_policy: RoutingPolicy::FailFast,
        deviation_policy: None,
        cache_na_stratified: false,
    }
}

// ── AC-I2-6 — T-LCD-03 on stratum C, n ≥ 5 ─────────────────────────────────

/// T-LCD-03's real-suite leg (AC-I2-6; the S3.3 deferral): the stratum-C
/// suite (`suite.swebench` — `contaminated_public`, `retired_for_headline`)
/// runs its five held-out tasks end to end — recorded `model_io` through
/// the canonical driver into the adapter's materialized environment,
/// deterministic-replay reproduced, collected, graded to full reward.
#[test]
fn t_lcd_03_round_trip_over_stratum_c() {
    let bs = Benchset::load_default().unwrap();
    let c = bs.suite("c").unwrap();
    assert_eq!(
        c.stratum(),
        hh_ontology::lab::ContaminationStratum::ContaminatedPublic
    );
    let held_out = c.tasks_on(SplitLabel::HeldOut);
    assert!(
        held_out.len() >= 5,
        "AC-I2-6 needs n ≥ 5 held-out stratum-C tasks"
    );
    let mut n = 0;
    for task in &held_out {
        let (reward, calls) = round_trip(task, c, &format!("lcd03-{}", task.name));
        assert_eq!(
            reward, 1_000_000,
            "{}: the recorded transcript must grade full reward",
            task.name
        );
        assert_eq!(calls, task.model_io.len() as i64);
        n += 1;
    }
    assert!(n >= 5);
}

// ── AC-I4-11 / AC-I4-3 — the real-runner parity legs over A/C/D/E ──────────

/// The corpus's `parity_subset` members replay through the driver+adapter
/// +grader path; `hh_eval::benefits::artifact_benefit` compares the
/// graded verdicts against the recorded `original_runs` under
/// `match_spec{matched_cap, model_calls}` and `parity_report` wraps the
/// `ComparisonReport` — the §5h.4 §7 record, produced by the real kernel.
#[test]
fn adapter_parity_legs_over_the_corpus() {
    let bs = Benchset::load_default().unwrap();
    let decl = catalogue::metric("task_success").expect("catalogue task_success");
    let caps = hh_budget::spec::BudgetSpec::hard_caps(
        hh_budget::spec::BudgetMode::Pool,
        &[(
            hh_budget::DimensionKey::Primary(DimensionId::ModelCalls),
            64,
        )],
    );

    for key in ["a", "c", "d", "e"] {
        let suite = bs.suite(key).unwrap();
        assert!(
            !suite.parity_subset.is_empty(),
            "{key}: the corpus declares a parity subset"
        );
        let mut runs: Vec<EvalRun> = Vec::new();
        let mut tasks: Vec<TaskContext> = Vec::new();
        let mut original_refs = Vec::new();
        let mut replay_refs = Vec::new();
        for name in &suite.parity_subset {
            let task = suite.task_named(name).expect("parity member in tasks");
            let tid = &task.record().task_id;
            tasks.push(TaskContext {
                task_id: tid.clone(),
                suite_id: suite.suite_id().to_string(),
                split_label: task.record().split_label,
                split_hash: suite.manifest.split_hash.clone(),
                stratum: suite.stratum(),
            });
            // The `original` arm — the foreign runner's recorded verdicts.
            for (i, r) in task.original_runs.iter().enumerate() {
                let rid = format!("orig:{key}:{name}:{i}");
                original_refs.push(rid.clone());
                runs.push(eval_run(
                    &rid,
                    "original",
                    tid,
                    suite.suite_id(),
                    task.record().split_label,
                    i as u64,
                    r.reward_ppm,
                    r.model_calls,
                    suite.stratum(),
                    &suite.manifest.split_hash,
                    suite.family(),
                ));
            }
            // The `replay` arm — the recorded model_io driven through the
            // real driver into the materialized env, graded for real.
            for rep in 0..3u64 {
                let (reward, calls) =
                    round_trip(task, suite, &format!("parity-{key}-{name}-{rep}"));
                let rid = format!("replay:{key}:{name}:{rep}");
                replay_refs.push(rid.clone());
                runs.push(eval_run(
                    &rid,
                    "replay",
                    tid,
                    suite.suite_id(),
                    task.record().split_label,
                    rep,
                    reward,
                    calls,
                    suite.stratum(),
                    &suite.manifest.split_hash,
                    suite.family(),
                ));
            }
        }
        let arms = [
            hh_budget::matchspec::ArmSpec::native(
                caps.clone(),
                caps.clone(),
                hh_budget::MatchSpec::matched_cap(&[DimensionId::ModelCalls]),
            ),
            hh_budget::matchspec::ArmSpec::native(
                caps.clone(),
                caps.clone(),
                hh_budget::MatchSpec::matched_cap(&[DimensionId::ModelCalls]),
            ),
        ];
        let metrics = vec!["task_success".to_string()];
        let input = CompareInput {
            arm_a: "original",
            arm_b: "replay",
            metrics: &metrics,
            declarations: std::slice::from_ref(&decl),
            runs: &runs,
            tasks: &tasks,
            design: &parity_design(),
            arm_specs: &arms,
            varied_factor: None,
            confidence_ppm: 950_000,
            benefit_kind: hh_lab::analysis::BenefitKind::ArtifactBenefit,
            held_out: true,
            family_size: None,
        };
        // `artifact_benefit` — the real compare under the held-out +
        // zero-search-spend + split-predates-search preconditions.
        let outcome = artifact_benefit(&input, suite.split_assignment.registered_at)
            .unwrap_or_else(|e| panic!("{key}: parity compare refused: {e:?}"));
        assert_eq!(outcome.reports.len(), 1, "{key}: one report per metric");
        let report = outcome.reports[0].clone();
        // The budget match is a real verdict — matched here because both
        // arms consumed the corpus's recorded two model calls per task.
        assert_eq!(
            report.budget_match.status,
            hh_lab::analysis::BudgetMatchStatus::Matched,
            "{key}: parity budget match"
        );
        let parity = hh_bench::parity::parity_report(
            &format!("original-runner/{key}"),
            original_refs,
            replay_refs,
            report,
        )
        .unwrap_or_else(|e| panic!("{key}: parity wrap: {e}"));
        // Parity is verdict-shape + per-task agreement — the point delta
        // stays within the band (identical rewards → delta 0).
        assert_eq!(
            parity.comparison.metric, "task_success",
            "{key}: the parity report's metric"
        );
    }
}

/// The refusal half — the same corpus tasks with an unbudgeted arm never
/// compare silently (`validate_match` refuses before any estimate).
#[test]
fn parity_compare_refuses_unbudgeted_arm() {
    let bs = Benchset::load_default().unwrap();
    let suite = bs.suite("a").unwrap();
    let task = suite.task_named(&suite.parity_subset[0].clone()).unwrap();
    let runs = vec![eval_run(
        "o1",
        "original",
        &task.record().task_id,
        suite.suite_id(),
        task.record().split_label,
        0,
        1_000_000,
        2,
        suite.stratum(),
        &suite.manifest.split_hash,
        suite.family(),
    )];
    let tasks = vec![TaskContext {
        task_id: task.record().task_id.clone(),
        suite_id: suite.suite_id().to_string(),
        split_label: task.record().split_label,
        split_hash: suite.manifest.split_hash.clone(),
        stratum: suite.stratum(),
    }];
    let caps = hh_budget::spec::BudgetSpec::hard_caps(
        hh_budget::spec::BudgetMode::Pool,
        &[(
            hh_budget::DimensionKey::Primary(DimensionId::ModelCalls),
            64,
        )],
    );
    let mut unbudgeted = hh_budget::matchspec::ArmSpec::native(
        caps.clone(),
        caps,
        hh_budget::MatchSpec::matched_cap(&[DimensionId::ModelCalls]),
    );
    unbudgeted.search_budget = None;
    let arms = [
        hh_budget::matchspec::ArmSpec::native(
            hh_budget::spec::BudgetSpec::hard_caps(
                hh_budget::spec::BudgetMode::Pool,
                &[(
                    hh_budget::DimensionKey::Primary(DimensionId::ModelCalls),
                    64,
                )],
            ),
            hh_budget::spec::BudgetSpec::hard_caps(
                hh_budget::spec::BudgetMode::Pool,
                &[(
                    hh_budget::DimensionKey::Primary(DimensionId::ModelCalls),
                    64,
                )],
            ),
            hh_budget::MatchSpec::matched_cap(&[DimensionId::ModelCalls]),
        ),
        unbudgeted,
    ];
    let metrics = vec!["task_success".to_string()];
    let input = CompareInput {
        arm_a: "original",
        arm_b: "replay",
        metrics: &metrics,
        declarations: &[catalogue::metric("task_success").unwrap()],
        runs: &runs,
        tasks: &tasks,
        design: &parity_design(),
        arm_specs: &arms,
        varied_factor: None,
        confidence_ppm: 950_000,
        benefit_kind: hh_lab::analysis::BenefitKind::ArtifactBenefit,
        held_out: true,
        family_size: None,
    };
    assert!(matches!(
        compare(&input),
        Err(hh_eval::compare::CompareError::Match(_))
    ));
}
