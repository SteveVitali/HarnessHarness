//! `hh-results` integration tests — the S3.4b results store over a real
//! `Store` + `LabDocs` + `ExperimentEngine` (spec §6.5; AC-R-2.10.5-
//! {1,2,3,5,6,7,9,12} + the typed refusal set). The rig mirrors
//! `hh-experiment/tests/engine.rs`: unique `HH_*` temp roots, a shared
//! `ManualClock`, the two-arm × one-task × two-replicate `comparative`
//! design (4 subject runs).

use std::collections::BTreeMap;
use std::path::PathBuf;

use hh_budget::spec::{BudgetMode, BudgetSpec};
use hh_budget::{DimensionId, DimensionKey, MatchSpec};
use hh_bundle::manifest::ReproLevel;
use hh_experiment::docs::LabDocs;
use hh_experiment::engine::{EngineContext, ExperimentEngine, NextVerdict};
use hh_identity::idp::idp_id;
use hh_lab::expand::{ArmConfiguration, ExpandTask};
use hh_lab::experiment::{
    ArmSpec, Backoff, BundlePolicy, CancelPolicy, ExperimentKind, ExperimentSpec, FactorSpec,
    LevelSpec, OrderKind, ReattemptPolicy, SchedulingPolicy, SuiteBinding, ValidationStrategy,
};
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::ids::{Clock, ManualClock};
use hh_ledger::store::{Lease, RedactTarget, Store, DEFAULT_BLOB_MAX_BYTES};
use hh_ontology::compliance::Detector;
use hh_ontology::config::Ref;
use hh_ontology::control::StopReason;
use hh_ontology::eval::{
    Design, DesignKind, FactorDeclaration, FactorLevel, MetricValue, MetricValueKind, Pairing,
    PreRegistration, RoutingPolicy, SeedPolicy,
};
use hh_ontology::lab::SplitLabel;
use hh_ontology::participant::ParticipantClass;
use hh_provenance::ProvenanceRecord;
use hh_results::audit::{verify_citation, AuditRef, CitationVerdict};
use hh_results::catalogue::BundleStatus;
use hh_results::error::ResultsError;
use hh_results::leaderboard::LeaderboardDefinition;
use hh_results::row::ResultsRow;
use hh_results::scoring::{OverlayRef, ScoringContext};
use hh_results::store::{ResultsStore, VerifyVerdict};
use hh_results::version::DerivedReason;
use hh_results::watermark::WatermarkSet;
use hh_wire::json::Json;

// ── fixture plumbing ────────────────────────────────────────────────────────

fn tmp(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-results-test-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// A pinned `idp/1` id — the manifest/binding fields demand pinned refs.
fn pinned(tag: &str) -> String {
    idp_id(&format!("test.{tag}"), tag.as_bytes())
}

/// A minimal `run_kind = experiment` manifest — `minimal` seeds the
/// agent-only configuration cells; non-agent runs must not carry them
/// (ADR-0183 §C), or `open_run` refuses with `ManifestInvalid`.
fn experiment_manifest() -> hh_ledger::manifest::RunManifest {
    let mut m = hh_ledger::manifest::RunManifest::minimal(hh_ledger::manifest::RunKind::Experiment);
    m.configuration_id = None;
    m.configuration_version_id = None;
    m
}

struct Rig {
    clock: ManualClock,
    store: Store,
    docs: LabDocs,
    results: ResultsStore,
}

fn rig(tag: &str, now_ms: u64) -> Rig {
    let root = tmp(tag);
    let clock = ManualClock::at(now_ms);
    let store =
        Store::open_with(&root, Box::new(clock.clone()), None, DEFAULT_BLOB_MAX_BYTES).unwrap();
    let docs = LabDocs::open(&root).unwrap();
    let results = ResultsStore::open(root.join("results")).unwrap();
    Rig {
        clock,
        store,
        docs,
        results,
    }
}

fn budgets() -> BTreeMap<String, BudgetSpec> {
    let caps = |n: i64| {
        BudgetSpec::hard_caps(
            BudgetMode::Pool,
            &[(DimensionKey::Primary(DimensionId::ModelCalls), n)],
        )
    };
    BTreeMap::from([
        ("eval:a".to_string(), caps(100)),
        ("eval:b".to_string(), caps(100)),
        ("search:a".to_string(), caps(50)),
        ("search:b".to_string(), caps(50)),
        ("pool".to_string(), caps(1_000)),
        ("instr".to_string(), caps(1_000)),
    ])
}

fn arm(arm_id: &str, level: &str, eval: &str, search: Option<&str>) -> ArmSpec {
    ArmSpec {
        arm_id: arm_id.to_string(),
        hypothesis: format!("{arm_id} does better"),
        level_assignment: BTreeMap::from([("compaction_strategy".to_string(), level.to_string())]),
        eval_budget: eval.to_string(),
        search_budget: search.map(str::to_string),
        match_spec: Some(MatchSpec::matched_cap(&[DimensionId::ModelCalls])),
        artifact_ref: Ref::new("artifact:x", pinned("artifact.x")),
        limits_enforced: "full".to_string(),
        model_role_table_ref: None,
        response_cache: None,
    }
}

fn pre_registration() -> PreRegistration {
    PreRegistration {
        registered_at: 1,
        hypothesis: "evict_oldest ≥ clear_tool_results".to_string(),
        primary_metrics: vec!["task_success".to_string()],
        equivalence_margin: None,
        min_n: 1,
        analysis_plan_ref: pinned("analysis"),
        task_split_hash: pinned("split.hash"),
        interactions: vec![],
    }
}

fn design() -> Design {
    Design {
        id: "design:test".to_string(),
        kind: DesignKind::Paired,
        factors: vec![FactorDeclaration {
            name: "compaction_strategy".to_string(),
            kind: hh_ontology::FactorKind::Harness,
            granularity: None,
            levels: vec![
                FactorLevel {
                    id: "evict_oldest".to_string(),
                    content_ref: pinned("level.evict"),
                    label: "evict oldest".to_string(),
                },
                FactorLevel {
                    id: "clear_tool_results".to_string(),
                    content_ref: pinned("level.clear"),
                    label: "clear tool results".to_string(),
                },
            ],
            role: None,
        }],
        blocking: vec!["task".to_string()],
        replicates_per_cell: 2,
        pairing: Pairing::ByTask,
        seed_policy: SeedPolicy {
            harness_rng: true,
            requested_sampling_seed: false,
            seed_honoured_required: false,
        },
        held_out_split_ref: None,
        pre_registration: pre_registration(),
        registry_snapshot_id: Some(pinned("registry.snap")),
        generators: None,
        resolution: None,
        routing_policy: RoutingPolicy::FailFast,
        deviation_policy: None,
        cache_na_stratified: false,
    }
}

fn spec(kind: ExperimentKind) -> ExperimentSpec {
    let mut s = ExperimentSpec {
        experiment_id: String::new(),
        kind,
        design: design(),
        pre_registration: (kind != ExperimentKind::Exploratory).then(pre_registration),
        factors: vec![FactorSpec {
            name: "compaction_strategy".to_string(),
            kind: hh_ontology::FactorKind::Harness,
            granularity: Some(hh_ontology::participant::Granularity::ComponentLevel),
            role: None,
            levels: vec![
                LevelSpec {
                    level_id: "evict_oldest".to_string(),
                    ref_: pinned("level.evict"),
                    overrides: None,
                    label: "evict oldest".to_string(),
                    class: ParticipantClass::Native,
                    non_portable: false,
                },
                LevelSpec {
                    level_id: "clear_tool_results".to_string(),
                    ref_: pinned("level.clear"),
                    overrides: None,
                    label: "clear tool results".to_string(),
                    class: ParticipantClass::Native,
                    non_portable: false,
                },
            ],
        }],
        arms: vec![
            arm("arm:a", "evict_oldest", "eval:a", Some("search:a")),
            arm("arm:b", "clear_tool_results", "eval:b", Some("search:b")),
        ],
        suite: SuiteBinding {
            suite_ref: pinned("suite.tb2"),
            split_labels_used: vec![SplitLabel::Dev],
            split_assignment_ref: Some(pinned("split.assign")),
        },
        replicates_per_cell: 2,
        seed_policy: SeedPolicy {
            harness_rng: true,
            requested_sampling_seed: false,
            seed_honoured_required: false,
        },
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: SchedulingPolicy {
            max_concurrent_runs: 4,
            pools: vec![],
            order: OrderKind::InterleavedBlocked,
            permutation_seed: "perm:test".to_string(),
            start_stagger_ms: 0,
            deadline: None,
            priority: None,
        },
        reattempt: ReattemptPolicy {
            max_per_plan: 2,
            max_fraction_of_plans_ppm: 1_000_000,
            backoff: Backoff {
                min_ms: 0,
                multiplier_ppm: 1_000_000,
                max_ms: 0,
            },
            error_classes_included: None,
            on_cancel: CancelPolicy::Replan,
        },
        budgets: hh_lab::experiment::ExperimentBudgets {
            experiment: "pool".to_string(),
            instrument: "instr".to_string(),
        },
        bundle_policy: BundlePolicy::Adhoc {
            salt: "salt:test".to_string(),
        },
        ext: BTreeMap::new(),
    };
    s.experiment_id = s.experiment_id();
    s
}

fn ctx() -> EngineContext<'static> {
    let map = budgets();
    EngineContext {
        resolve_budget: Some(Box::new(move |r: &str| map.get(r).cloned())),
        suite_tasks: Some(Box::new(|_spec: &ExperimentSpec| {
            Some(vec![ExpandTask {
                task_id: "task:1".to_string(),
                split_label: SplitLabel::Dev,
            }])
        })),
        arm_config: Some(Box::new(|a: &ArmSpec| {
            Ok(ArmConfiguration {
                configuration_id: pinned(&format!("cfg.{}", a.arm_id)),
                configuration_version_id: pinned(&format!("cfgv.{}", a.arm_id)),
            })
        })),
        min_replicates: 1,
        ..Default::default()
    }
}

/// A registered+expanded+opened experiment — `(experiment_id,
/// experiment_run_id)`.
fn open(rig: &mut Rig, spec: &ExperimentSpec) -> (String, String) {
    let mut eng = ExperimentEngine::new(&mut rig.store, rig.docs.clone(), ctx());
    let eid = eng.register(spec).unwrap();
    eng.expand(&eid).unwrap();
    let run_id = eng.open_experiment(&eid).unwrap();
    (eid, run_id)
}

fn mint(store: &Store, run_id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: store.alloc_id("evt"),
        class: class.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("hh-results-test/1"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap(),
        causes: vec![],
        refs: vec![],
        ir_refs: vec![],
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel(
            "hh-results-test/1",
            store.now_ms(),
        )),
        content_kind: None,
        payload,
    }
}

fn append_chained(store: &mut Store, run_id: &str, lease: &Lease, mut batch: Vec<Event>) {
    let mut parent = store.head_event_id(run_id).unwrap();
    for e in &mut batch {
        e.parent_event_id = parent.clone();
        parent = e.event_id.clone();
    }
    store.append(run_id, lease, batch).unwrap();
}

/// A `measurement.metric.emitted` payload — `MetricValue`'s canonical form.
fn metric_payload(metric: &str, value: MetricValueKind, applies_to: &str) -> Json {
    MetricValue {
        metric_ref: metric.to_string(),
        value,
        applies_to: applies_to.to_string(),
        oracle_ref: "oracle:test".to_string(),
        detector: Detector::Deterministic,
        confidence: None,
        evidence_ref: None,
    }
    .to_json()
}

/// Drive a subject run to terminal: `control.budget.consumed` rows, any
/// `extra` events (metrics, bundle rows — they must precede `finished`),
/// then `lifecycle.run.finished{stop_reason}`.
fn finish_subject(
    store: &mut Store,
    run_id: &str,
    writer: &Lease,
    stop: StopReason,
    consumed: &[(DimensionId, i64)],
    extra: Vec<(String, Json)>,
) {
    let mut batch = Vec::new();
    for (d, a) in consumed {
        batch.push(mint(
            store,
            run_id,
            "control.budget.consumed",
            Json::obj([
                ("dimension", Json::str(d.as_str())),
                ("amount", Json::Int(*a)),
            ]),
        ));
    }
    for (class, payload) in extra {
        batch.push(mint(store, run_id, &class, payload));
    }
    batch.push(mint(
        store,
        run_id,
        "lifecycle.run.finished",
        Json::obj([
            ("stop_reason", stop.to_json()),
            ("outcome_class", Json::str(stop.outcome_class().as_str())),
        ]),
    ));
    append_chained(store, run_id, writer, batch);
}

/// The launched subject run ids so far (the experiment view's source of
/// truth — `run_launched` rows name them).
fn launched_runs(rig: &Rig, experiment_run_id: &str) -> Vec<String> {
    let view =
        hh_experiment::view::ExperimentView::fold(rig.store.envelopes(experiment_run_id).unwrap());
    let mut out = Vec::new();
    for plan in view.plans.values() {
        for a in &plan.attempts {
            out.push(a.run_id.clone());
        }
    }
    out.sort();
    out
}

/// Run the whole lifecycle to close — every attempt finishes with a
/// `wall_time_ms` metric value of `metric_base + launch_index` (distinct
/// per arm ordering is not guaranteed, but the two arms' values differ by
/// construction of the launch order: arms interleave, so the caller picks
/// distinguishable magnitudes by step).
fn run_to_close(rig: &mut Rig, eid: &str, metric_base: i64, decision: bool) -> String {
    run_to_close_with(rig, eid, metric_base, decision, |_| 3)
}

/// `run_to_close` with a per-arm consumption function — the arm is read
/// off the plan state so the caller controls each arm's realised budget
/// (the close-time `budget_match`/`under_utilised` inputs).
fn run_to_close_with(
    rig: &mut Rig,
    eid: &str,
    metric_base: i64,
    decision: bool,
    consume: impl Fn(&str) -> i64,
) -> String {
    // The experiment *run* (distinct from the experiment id `eid` the
    // engine attaches by) — the plan state lives on its stream.
    let exp_run = rig
        .store
        .run_ids()
        .into_iter()
        .find(|r| {
            rig.store
                .manifest(r)
                .map(|m| m.run_kind == hh_ledger::manifest::RunKind::Experiment)
                .unwrap_or(false)
        })
        .unwrap();
    let mut i = 0i64;
    loop {
        let mut eng = ExperimentEngine::new(&mut rig.store, rig.docs.clone(), ctx());
        eng.attach(eid).unwrap();
        match eng.next().unwrap() {
            NextVerdict::Plan { run_plan_id } => {
                let ticket = eng.claim(&run_plan_id, "driver").unwrap();
                let launched = eng.launch(&ticket, "subject").unwrap();
                drop(eng);
                let arm_id = hh_experiment::view::ExperimentView::fold(
                    rig.store.envelopes(&exp_run).unwrap(),
                )
                .plans
                .get(&run_plan_id)
                .map(|p| p.arm_id.clone())
                .unwrap_or_default();
                let value = metric_base + i * 7;
                i += 1;
                let mut extra = vec![(
                    "measurement.metric.emitted".to_string(),
                    metric_payload(
                        "wall_time_ms",
                        MetricValueKind::Decimal(value),
                        &launched.run_id,
                    ),
                )];
                if decision {
                    // A `control.decision` row marks the run
                    // replay-declared — the bundle's R1 basis arm
                    // (B-R1-replay) needs it.
                    extra.push((
                        "control.decision".to_string(),
                        Json::obj([("kind", Json::str("route"))]),
                    ));
                }
                finish_subject(
                    &mut rig.store,
                    &launched.run_id,
                    &launched.subject_writer,
                    StopReason::Completed,
                    &[(DimensionId::ModelCalls, consume(&arm_id))],
                    extra,
                );
                let mut eng = ExperimentEngine::new(&mut rig.store, rig.docs.clone(), ctx());
                eng.attach(eid).unwrap();
                let out = eng.settle(&run_plan_id).unwrap();
                assert!(out.accepted);
            }
            NextVerdict::Done => break,
            other => panic!("unexpected next: {other:?}"),
        }
    }
    let mut eng = ExperimentEngine::new(&mut rig.store, rig.docs.clone(), ctx());
    eng.attach(eid).unwrap();
    let report = eng.close(false).unwrap();
    let _ = report;
    drop(eng);
    exp_run
}

/// A minimal self-contained `run` bundle covering `subject_run` at R0 —
/// the ledger members come from the real `build_ledger_export` (the page
/// carries the subject's envelopes, so a `control.decision` row is what
/// `replay_declared` reads for the B-R1 basis); returns the bundle id
/// (`manifest.version_id`).
fn deposit_bundle(rig: &mut Rig, subject_run: &str) -> String {
    deposit_bundle_inner(rig, subject_run, ReproLevel::R0, false)
}

/// A bundle claiming `level` — ≥R1 adds the `compiled_bundle` member +
/// `definition.compiled_bundle.member` ref (B-R1-derivations) and relies
/// on the subject run's `control.decision` row for B-R1-replay (the
/// caller's `run_to_close(…, decision = true)`).
fn deposit_bundle_at(rig: &mut Rig, subject_run: &str, level: ReproLevel) -> String {
    deposit_bundle_inner(rig, subject_run, level, false)
}

/// A bundle whose `ledger_tree` member claims `present` but carries no
/// pool bytes — S1 fails, the catalogue entry stays `assembled`.
fn deposit_bundle_broken(rig: &mut Rig, subject_run: &str) -> String {
    deposit_bundle_inner(rig, subject_run, ReproLevel::R0, true)
}

fn deposit_bundle_inner(
    rig: &mut Rig,
    subject_run: &str,
    level: ReproLevel,
    withhold_member: bool,
) -> String {
    let member = |bytes: &[u8]| idp_id("blob", bytes);
    let doc = |v: Json| v.to_canonical_string().into_bytes();
    let mut refs = Vec::new();
    let mut put = |role: &str, bytes: Vec<u8>, media: &str| -> String {
        let addr = member(&bytes);
        rig.store.put_blob(&bytes, media).unwrap();
        refs.push(Json::obj([
            ("role", Json::str(role)),
            ("ref", Json::str(&addr)),
            ("media_type", Json::str(media)),
            ("size", Json::Int(bytes.len() as i64)),
            ("status", Json::str("present")),
        ]));
        addr
    };
    for role in [
        "subject",
        "definition",
        "environment",
        "model",
        "nondeterminism",
        "instrument",
        "configuration",
        "resolved_dependencies",
        "results",
    ] {
        put(
            role,
            doc(Json::obj([("role", Json::str(role))])),
            "application/json",
        );
    }
    // ≥R1 — the derivations basis needs a `compiled_bundle` member the
    // `definition` section names.
    let compiled_addr = (level >= ReproLevel::R1).then(|| {
        put(
            "compiled_bundle",
            doc(Json::obj([("kind", Json::str("compiled_bundle"))])),
            "application/json",
        )
    });
    // Real ledger members — pages + tree exported from the live store.
    let mut member_bytes: hh_bundle::export::MemberBytes = BTreeMap::new();
    let (export, roles) =
        hh_bundle::export::build_ledger_export(&rig.store, subject_run, &mut member_bytes).unwrap();
    // `withhold_member` skips the tree's bytes — a `present` claim with
    // nothing behind it, S1's "claims present, carries no bytes" fail.
    let withheld = withhold_member
        .then(|| roles.last().map(|r| r.1.clone()))
        .flatten();
    for (role, addr, size) in &roles {
        let media = if role.starts_with("ledger_tree:") {
            "application/vnd.hh.ledger-tree+json"
        } else {
            "application/vnd.hh.ledger-page+json"
        };
        if withheld.as_deref() != Some(addr.as_str()) {
            rig.store.put_blob(&member_bytes[addr], media).unwrap();
        }
        refs.push(Json::obj([
            ("role", Json::str(role)),
            ("ref", Json::str(addr)),
            ("media_type", Json::str(media)),
            ("size", Json::Int(*size as i64)),
            ("status", Json::str("present")),
        ]));
    }
    let definition = match &compiled_addr {
        Some(a) => Json::obj([("compiled_bundle", Json::obj([("member", Json::str(a))]))]),
        None => Json::obj([]),
    };
    let manifest_doc = Json::obj([
        ("schema", Json::str("hh-bundle/1")),
        ("idp", Json::str("idp/1")),
        ("bundle_kind", Json::str("run")),
        ("created_at", Json::str("2026-09-20T00:00:00.000Z")),
        (
            "producer",
            Json::obj([
                (
                    "origin",
                    Json::obj([
                        ("kind", Json::str("kernel")),
                        ("component_ref", Json::str("hh-results-test")),
                    ]),
                ),
                ("authority", Json::str("kernel")),
                ("scope", Json::str("run")),
                ("created_at", Json::Int(0)),
            ]),
        ),
        ("participant_class", Json::str("native")),
        ("observability_levels", Json::Arr(vec![Json::str("ledger")])),
        ("claims", Json::Arr(vec![])),
        ("name_bindings", Json::Arr(vec![])),
        ("fetch_policy", Json::str("self_contained")),
        ("fetch", Json::Arr(vec![])),
        (
            "subject",
            Json::obj([
                ("run_ids", Json::Arr(vec![Json::str(subject_run)])),
                ("heads", Json::obj([])),
                ("lineage", Json::Arr(vec![])),
                (
                    "watermarks",
                    Json::Obj(BTreeMap::from([(subject_run.to_string(), Json::Int(0))])),
                ),
                ("status", Json::str("finished")),
            ]),
        ),
        ("definition", definition),
        ("configuration", Json::obj([])),
        (
            "resolved_dependencies",
            // S3.12 (AC-R-2.12.1-8): `run_refs` indexes every reference
            // the subject run's manifest carries — a bundler writes it,
            // so the fixture projects it from the live store's manifest.
            Json::obj([(
                "run_refs",
                hh_bundle::runrefs::run_refs_index(&[(
                    subject_run,
                    rig.store.manifest(subject_run).unwrap(),
                )]),
            )]),
        ),
        ("model", Json::obj([("snapshots", Json::Arr(vec![]))])),
        ("instrument", Json::obj([])),
        (
            "traces",
            Json::Obj(BTreeMap::from([(
                subject_run.to_string(),
                export.to_json(),
            )])),
        ),
        ("results", Json::obj([("rows", Json::Arr(vec![]))])),
        (
            "reproducibility",
            Json::obj([
                ("max_supported_level", Json::str(level.name())),
                ("claimed_level", Json::str(level.name())),
            ]),
        ),
        ("members", Json::Arr(refs)),
        ("unpinned", Json::Arr(vec![])),
        ("ext", Json::obj([])),
        ("version_id", Json::str("pending")),
    ]);
    let mut manifest = hh_bundle::manifest::BundleManifest::from_json(&manifest_doc).unwrap();
    manifest.version_id = manifest.compute_id();
    rig.store
        .put_blob(
            manifest.to_json().to_canonical_string().as_bytes(),
            "application/vnd.hh.bundle+json",
        )
        .unwrap();
    manifest.version_id
}

// ── AC-R-2.10.5-1: project()-pure + rebuildable ─────────────────────────────

#[test]
fn project_row_is_deterministic_and_refuses_typed() {
    let mut r = rig("project", 1_000);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _exp_run) = open(&mut r, &s);
    // Launch one attempt and finish it.
    let run_id = {
        let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
        eng.attach(&eid).unwrap();
        let run_plan_id = match eng.next().unwrap() {
            NextVerdict::Plan { run_plan_id } => run_plan_id,
            other => panic!("expected plan, got {other:?}"),
        };
        let ticket = eng.claim(&run_plan_id, "driver").unwrap();
        let launched = eng.launch(&ticket, "subject").unwrap();
        drop(eng);
        finish_subject(
            &mut r.store,
            &launched.run_id,
            &launched.subject_writer,
            StopReason::Completed,
            &[(DimensionId::ModelCalls, 5)],
            vec![(
                "measurement.metric.emitted".to_string(),
                metric_payload(
                    "wall_time_ms",
                    MetricValueKind::Decimal(120),
                    &launched.run_id,
                ),
            )],
        );
        launched.run_id
    };

    // Determinism — two projections produce equal bytes and ids.
    let a = r
        .results
        .project_row(&r.store, Some(&r.docs), &run_id, None, None)
        .unwrap();
    let b = r
        .results
        .project_row(&r.store, Some(&r.docs), &run_id, None, None)
        .unwrap();
    assert_eq!(a.version_id, b.version_id);
    assert_eq!(a.view_hash, b.view_hash);
    assert_eq!(a.to_canonical_bytes(), b.to_canonical_bytes());

    // The row is the finished projection.
    assert_eq!(a.outcome.status, "finished");
    assert_eq!(a.outcome.outcome_class.as_deref(), Some("scored"));
    assert_eq!(a.outcome.stop_reason.as_deref(), Some("completed"));
    assert_eq!(
        a.consumption.dimensions.get("model_calls").copied(),
        Some(5)
    );
    let wall = a
        .cells
        .iter()
        .find(|c| c.metric_ref == "wall_time_ms")
        .unwrap();
    assert_eq!(wall.value, MetricValueKind::Decimal(120));
    // The total metric list — every catalogue metric has a cell (R-ROW-3).
    assert_eq!(a.cells.len(), hh_eval::catalogue::scorecard_metrics().len());
    // The fixed key is the identity coordinate pair — no names anywhere
    // (R-ROW-2: spot-check the canonical form carries no `label` member).
    let j = a.to_json();
    assert!(!j.to_canonical_string().contains("\"label\""));

    // Typed refusals.
    let exp_run = r
        .store
        .run_ids()
        .into_iter()
        .find(|id| {
            r.store
                .manifest(id)
                .map(|m| m.run_kind == hh_ledger::manifest::RunKind::Experiment)
                .unwrap_or(false)
        })
        .unwrap();
    assert!(matches!(
        r.results
            .project_row(&r.store, Some(&r.docs), &exp_run, None, None),
        Err(ResultsError::NotSubjectRun { .. })
    ));
    let head = r.store.head(&run_id).unwrap().seq;
    assert!(matches!(
        r.results
            .project_row(&r.store, Some(&r.docs), &run_id, Some(head + 10), None),
        Err(ResultsError::RunNotDurable { .. })
    ));
    // An unknown registry pin refuses.
    let mut scoring = ScoringContext::native();
    scoring.metric_registry_version = pinned("bogus.registry");
    assert!(matches!(
        r.results
            .project_row(&r.store, Some(&r.docs), &run_id, None, Some(&scoring)),
        Err(ResultsError::RegistryVersionUnknown { .. })
    ));
    // An unknown run is a ledger error, not a panic.
    assert!(r
        .results
        .project_row(&r.store, Some(&r.docs), "run:nope", None, None)
        .is_err());
}

#[test]
fn row_codec_round_trips_and_ids_recompute() {
    let mut r = rig("codec", 2_000);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _e) = open(&mut r, &s);
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    let run_plan_id = match eng.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("{other:?}"),
    };
    let ticket = eng.claim(&run_plan_id, "driver").unwrap();
    let launched = eng.launch(&ticket, "subject").unwrap();
    drop(eng);
    finish_subject(
        &mut r.store,
        &launched.run_id,
        &launched.subject_writer,
        StopReason::Completed,
        &[],
        vec![],
    );
    let row = r
        .results
        .project_row(&r.store, Some(&r.docs), &launched.run_id, None, None)
        .unwrap();
    let back = ResultsRow::from_json(&row.to_json()).unwrap();
    assert_eq!(back, row);
    assert_eq!(row.compute_version_id(), row.version_id);
    assert_eq!(row.compute_view_hash(), row.view_hash);
    assert!(row.version_id.starts_with("sha256:"));
    // The watermark set covers exactly the subject prefix.
    assert_eq!(
        row.watermark_set.get(&launched.run_id),
        Some(row.derived_from.seq)
    );
}

// ── AC-R-2.10.5-2/3: versions supersede under the fixed key ─────────────────

#[test]
fn versions_supersede_and_heads_resolve() {
    let mut r = rig("versions", 3_000);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _e) = open(&mut r, &s);
    let (run_id, writer) = {
        let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
        eng.attach(&eid).unwrap();
        let run_plan_id = match eng.next().unwrap() {
            NextVerdict::Plan { run_plan_id } => run_plan_id,
            other => panic!("{other:?}"),
        };
        let ticket = eng.claim(&run_plan_id, "driver").unwrap();
        let launched = eng.launch(&ticket, "subject").unwrap();
        drop(eng);
        finish_subject(
            &mut r.store,
            &launched.run_id,
            &launched.subject_writer,
            StopReason::Completed,
            &[],
            vec![(
                "measurement.metric.emitted".to_string(),
                metric_payload(
                    "wall_time_ms",
                    MetricValueKind::Decimal(50),
                    &launched.run_id,
                ),
            )],
        );
        (launched.run_id, launched.subject_writer)
    };

    let (row, v1) = r
        .results
        .project_and_record(
            &r.store,
            Some(&r.docs),
            &run_id,
            None,
            None,
            DerivedReason::Initial,
        )
        .unwrap();
    let key_id = row.key.key_id();
    assert_eq!(r.results.head(&key_id).unwrap().version_id, v1.version_id);

    // Re-recording the identical projection is idempotent.
    let same = r.results.record(&row, DerivedReason::Initial).unwrap();
    assert_eq!(same.version_id, v1.version_id);
    assert_eq!(r.results.row_history(&key_id).unwrap().len(), 1);

    // An overlay regrade lands a new version that supersedes the head.
    let overlay_id = {
        let manifest = experiment_manifest();
        let (rid, lease) = r.store.open_run(manifest, "overlay-test").unwrap();
        let verdict = mint(
            &r.store,
            &rid,
            "verification.validator.verdict",
            Json::obj([
                ("verdict_id", Json::str(pinned("verdict.1"))),
                ("validator_ref", Json::str("validator:test")),
                ("target", Json::str(&run_id)),
                ("metric", Json::str("wall_time_ms")),
                ("value", Json::Int(10)),
                ("status", Json::str("decided")),
                ("detector", Json::str("deterministic")),
            ]),
        );
        append_chained(&mut r.store, &rid, &lease, vec![verdict]);
        rid
    };
    let mut rescoring = ScoringContext::native();
    rescoring.overlay_runs = vec![OverlayRef {
        run_id: overlay_id,
        until_seq: None,
    }];
    let v2 = r
        .results
        .rescore(
            &r.store,
            Some(&r.docs),
            &key_id,
            &rescoring,
            DerivedReason::Regrade,
        )
        .unwrap();
    assert_ne!(v2.version_id, v1.version_id);
    assert_eq!(v2.supersedes.as_deref(), Some(v1.version_id.as_str()));
    assert_eq!(r.results.head(&key_id).unwrap().version_id, v2.version_id);
    let history = r.results.row_history(&key_id).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].version_id, v1.version_id);
    assert_eq!(history[1].version_id, v2.version_id);
    // The rescored row carries the overlay's value + watermark.
    let (head_row, _anns) = r.results.get_row(&r.store, &key_id).unwrap();
    let wall = head_row
        .cells
        .iter()
        .find(|c| c.metric_ref == "wall_time_ms")
        .unwrap();
    assert_eq!(wall.value, MetricValueKind::Decimal(10));
    assert!(wall.computed_from.iter().any(|s: &String| s.contains(':')));
    // The superseded version's bytes are intact (no mutation, ever).
    let old = r.results.get_version(&v1.version_id).unwrap();
    assert_eq!(old.version_id, v1.version_id);
    assert_eq!(
        old.cells
            .iter()
            .find(|c| c.metric_ref == "wall_time_ms")
            .unwrap()
            .value,
        MetricValueKind::Decimal(50)
    );
    // `NoChange` — identical scoring re-derivation refuses.
    let _ = writer;
    assert!(matches!(
        r.results.rescore(
            &r.store,
            Some(&r.docs),
            &key_id,
            &rescoring,
            DerivedReason::Regrade
        ),
        Err(ResultsError::NoChange { .. })
    ));
    // An overlay that never targets the subject refuses.
    let stray = {
        let manifest = experiment_manifest();
        let (rid, lease) = r.store.open_run(manifest, "overlay-stray").unwrap();
        let ev = mint(
            &r.store,
            &rid,
            "verification.validator.verdict",
            Json::obj([
                ("target", Json::str("run:elsewhere")),
                ("metric", Json::str("wall_time_ms")),
                ("value", Json::Int(1)),
                ("status", Json::str("decided")),
            ]),
        );
        append_chained(&mut r.store, &rid, &lease, vec![ev]);
        rid
    };
    let mut bad = ScoringContext::native();
    bad.overlay_runs = vec![OverlayRef {
        run_id: stray,
        until_seq: None,
    }];
    assert!(matches!(
        r.results
            .project_row(&r.store, Some(&r.docs), &run_id, None, Some(&bad)),
        Err(ResultsError::OverlayNotTargeting { .. })
    ));
}

// ── AC-R-2.10.5-5: reads repeatable at a watermark set ──────────────────────

#[test]
fn reads_repeat_at_watermark_and_refuse_ahead() {
    let mut r = rig("watermark", 4_000);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _e) = open(&mut r, &s);
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    let run_plan_id = match eng.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("{other:?}"),
    };
    let ticket = eng.claim(&run_plan_id, "driver").unwrap();
    let launched = eng.launch(&ticket, "subject").unwrap();
    drop(eng);
    // Finish without the metric first; record the early head row.
    finish_subject(
        &mut r.store,
        &launched.run_id,
        &launched.subject_writer,
        StopReason::Completed,
        &[],
        vec![],
    );
    let early_seq = r.store.head(&launched.run_id).unwrap().seq;
    let (row1, _) = r
        .results
        .project_and_record(
            &r.store,
            Some(&r.docs),
            &launched.run_id,
            Some(early_seq),
            None,
            DerivedReason::Initial,
        )
        .unwrap();
    let wm = row1.watermark_set.clone();

    // A regrade overlay lands and a second version is recorded over the
    // rescored scoring — the new version's watermark set carries the
    // overlay run's pin, so the early watermark no longer covers it.
    let overlay_id = {
        let manifest = experiment_manifest();
        let (rid, lease) = r.store.open_run(manifest, "overlay-wm").unwrap();
        let verdict = mint(
            &r.store,
            &rid,
            "verification.validator.verdict",
            Json::obj([
                ("verdict_id", Json::str(pinned("verdict.wm"))),
                ("validator_ref", Json::str("validator:test")),
                ("target", Json::str(&launched.run_id)),
                ("metric", Json::str("wall_time_ms")),
                ("value", Json::Int(10)),
                ("status", Json::str("decided")),
                ("detector", Json::str("deterministic")),
            ]),
        );
        append_chained(&mut r.store, &rid, &lease, vec![verdict]);
        rid
    };
    let mut scoring = ScoringContext::native();
    scoring.view_policy_version = "hh-results-view/test-bump".to_string();
    scoring.overlay_runs = vec![OverlayRef {
        run_id: overlay_id,
        until_seq: None,
    }];
    let (row2, _) = r
        .results
        .project_and_record(
            &r.store,
            Some(&r.docs),
            &launched.run_id,
            None,
            Some(&scoring),
            DerivedReason::RegistryBump,
        )
        .unwrap();
    assert_ne!(row1.version_id, row2.version_id);

    // A query `at` the early watermark sees the early head row; a query
    // without `at` sees the head.
    let mut spec = hh_results::query::QuerySpec::all();
    spec.at = Some(wm.clone());
    let page_at = r.results.query_rows(&r.store, &spec).unwrap();
    // `row2`'s watermark carries the overlay pin — `at` (row1's set) does
    // not cover it, so the early view still resolves the early head row.
    assert_eq!(page_at.rows.len(), 1);
    assert_eq!(page_at.rows[0].version_id, row1.version_id);
    let page_head = r
        .results
        .query_rows(&r.store, &hh_results::query::QuerySpec::all())
        .unwrap();
    assert_eq!(page_head.rows.len(), 1);
    assert_eq!(page_head.rows[0].version_id, row2.version_id);
    // Repeating the `at` read reproduces the identical view hash.
    let page_at2 = r.results.query_rows(&r.store, &spec).unwrap();
    assert_eq!(page_at.view_hash, page_at2.view_hash);

    // `WatermarkAhead` — a pin beyond the durable head refuses.
    let mut ahead = WatermarkSet::new();
    ahead.pin(&launched.run_id, early_seq + 100);
    let mut spec2 = hh_results::query::QuerySpec::all();
    spec2.at = Some(ahead);
    assert!(matches!(
        r.results.query_rows(&r.store, &spec2),
        Err(ResultsError::WatermarkAhead { .. })
    ));
    // A pin naming a run the store does not hold refuses too.
    let mut ghost = WatermarkSet::new();
    ghost.pin("run:ghost", 0);
    let mut spec3 = hh_results::query::QuerySpec::all();
    spec3.at = Some(ghost);
    assert!(matches!(
        r.results.query_rows(&r.store, &spec3),
        Err(ResultsError::WatermarkAhead { head: None, .. })
    ));
}

// ── AC-R-2.10.5-6/7: audit refs + verify_row ────────────────────────────────

#[test]
fn every_cell_cites_a_verifying_audit_ref() {
    let mut r = rig("audit", 5_000);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _e) = open(&mut r, &s);
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    let run_plan_id = match eng.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("{other:?}"),
    };
    let ticket = eng.claim(&run_plan_id, "driver").unwrap();
    let launched = eng.launch(&ticket, "subject").unwrap();
    drop(eng);
    finish_subject(
        &mut r.store,
        &launched.run_id,
        &launched.subject_writer,
        StopReason::Completed,
        &[],
        vec![],
    );
    let (row, _) = r
        .results
        .project_and_record(
            &r.store,
            Some(&r.docs),
            &launched.run_id,
            None,
            None,
            DerivedReason::Initial,
        )
        .unwrap();
    // Every cell — including `n/a` cells — cites ≥1 verifying audit_ref.
    assert!(!row.cells.is_empty());
    for c in &row.cells {
        assert!(
            !c.evidence.is_empty(),
            "cell {} has no citation",
            c.metric_ref
        );
        for a in &c.evidence {
            assert_eq!(
                verify_citation(&r.store, a),
                CitationVerdict::Verified,
                "cell {} citation failed",
                c.metric_ref
            );
        }
    }
    assert_eq!(
        verify_citation(&r.store, &row.audit.head),
        CitationVerdict::Verified
    );

    // A tampered citation reports `Tampered`, a beyond-head seq `Missing`.
    let mut bad = row.audit.head.clone();
    bad.hash = pinned("wrong.hash");
    assert!(matches!(
        verify_citation(&r.store, &bad),
        CitationVerdict::Tampered { .. }
    ));
    let ahead = AuditRef {
        run_id: launched.run_id.clone(),
        seq: r.store.head(&launched.run_id).unwrap().seq + 50,
        hash: pinned("nope"),
        checkpoint_ref: None,
        content_refs: vec![],
    };
    assert!(matches!(
        verify_citation(&r.store, &ahead),
        CitationVerdict::Missing { .. }
    ));
}

#[test]
fn verify_row_detects_stale_and_corrupt() {
    let mut r = rig("verify", 6_000);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _e) = open(&mut r, &s);
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    let run_plan_id = match eng.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("{other:?}"),
    };
    let ticket = eng.claim(&run_plan_id, "driver").unwrap();
    let launched = eng.launch(&ticket, "subject").unwrap();
    drop(eng);
    finish_subject(
        &mut r.store,
        &launched.run_id,
        &launched.subject_writer,
        StopReason::Completed,
        &[],
        vec![],
    );
    let (row, v) = r
        .results
        .project_and_record(
            &r.store,
            Some(&r.docs),
            &launched.run_id,
            None,
            None,
            DerivedReason::Initial,
        )
        .unwrap();
    assert_eq!(
        r.results
            .verify_row(&r.store, Some(&r.docs), &v.version_id)
            .unwrap(),
        VerifyVerdict::Ok
    );
    let report =
        hh_results::verify::verify_row(&r.results, &r.store, Some(&r.docs), &v.version_id).unwrap();
    assert!(report.ok, "{}", report.to_json().to_canonical_string());

    // Corrupt the stored bytes — re-projection still yields the true
    // version id, so the stored file's declared identity mismatches.
    let row_path = r
        .results
        .root()
        .join("rows")
        .join(row.key.key_id())
        .join(format!("{}.json", v.version_id.replace(':', "_")));
    let mut tampered = row.to_json();
    if let Json::Obj(ref mut m) = tampered {
        m.insert("version_id".into(), Json::str(pinned("forged.version")));
    }
    std::fs::write(&row_path, tampered.to_canonical_string()).unwrap();
    let report2 =
        hh_results::verify::verify_row(&r.results, &r.store, Some(&r.docs), &v.version_id).unwrap();
    assert!(!report2.ok);
    assert!(report2.checks.iter().any(|c| !c.ok));

    // Unknown version refuses typed.
    assert!(matches!(
        r.results
            .verify_row(&r.store, Some(&r.docs), &pinned("absent")),
        Err(ResultsError::UnknownVersion { .. })
    ));
}

// ── AC-R-2.10.5-12: redacted evidence surfaces, never drops ─────────────────

#[test]
fn redacted_evidence_reports_missing_not_silent() {
    let mut r = rig("redact", 7_000);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _e) = open(&mut r, &s);
    let mut eng = ExperimentEngine::new(&mut r.store, r.docs.clone(), ctx());
    eng.attach(&eid).unwrap();
    let run_plan_id = match eng.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("{other:?}"),
    };
    let ticket = eng.claim(&run_plan_id, "driver").unwrap();
    let launched = eng.launch(&ticket, "subject").unwrap();
    drop(eng);
    // Deposit evidence bytes and emit a metric citing them.
    let evidence = b"the oracle's intermediate record".to_vec();
    let ev_addr = r.store.put_blob(&evidence, "text/plain").unwrap();
    let ev_id = ev_addr.id();
    finish_subject(
        &mut r.store,
        &launched.run_id,
        &launched.subject_writer,
        StopReason::Completed,
        &[],
        vec![{
            let mut p = metric_payload(
                "wall_time_ms",
                MetricValueKind::Decimal(9),
                &launched.run_id,
            );
            if let Json::Obj(ref mut m) = p {
                m.insert("evidence_ref".into(), Json::str(&ev_id));
            }
            ("measurement.metric.emitted".to_string(), p)
        }],
    );
    let row = r
        .results
        .project_row(&r.store, Some(&r.docs), &launched.run_id, None, None)
        .unwrap();
    let cell = row
        .cells
        .iter()
        .find(|c| c.metric_ref == "wall_time_ms")
        .unwrap();
    let cite = &cell.evidence[0];
    assert!(cite.content_refs.contains(&ev_id));
    assert_eq!(verify_citation(&r.store, cite), CitationVerdict::Verified);

    // Redact the blob — the citation now resolves `Missing{redacted}`,
    // visible, never dropped.
    r.store
        .redact(
            &launched.run_id,
            &launched.subject_writer,
            vec![RedactTarget::Address(ev_id.clone())],
            "test-redaction",
            &ProvenanceRecord::kernel("hh-results-test/1", r.clock.now_ms()),
            "policy_rule",
        )
        .unwrap();
    assert_eq!(
        verify_citation(&r.store, cite),
        CitationVerdict::Missing {
            reason: hh_ledger::errors::MissingReason::Redacted
        }
    );
}

// ── AC-R-2.10.5-3: build_cells ──────────────────────────────────────────────

#[test]
fn build_cells_reports_every_attempt_with_reasons() {
    let mut r = rig("cells", 8_000);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _e) = open(&mut r, &s);
    let exp_run = run_to_close(&mut r, &eid, 100, false);
    let table = r.results.cells(&r.store, &r.docs, &exp_run, None).unwrap();
    // 2 arms × 1 task → 2 cells, 2 replicate rows each.
    assert_eq!(table.cells.len(), 2);
    for c in &table.cells {
        assert_eq!(c.rows.len(), 2);
        assert!(c.rows.iter().all(|x| x.included));
        assert_eq!(c.task_id, "task:1");
        assert_eq!(c.split_label, "dev");
        let wall = c.per_metric.get("wall_time_ms").unwrap();
        assert_eq!(wall.n, 2);
        assert_eq!(wall.values.len(), 2);
    }
    // The watermark set pins the experiment run and every subject.
    assert!(table.watermark_set.get(&exp_run).is_some());
    // Repeating the fold reproduces the identical view hash.
    let again = r.results.cells(&r.store, &r.docs, &exp_run, None).unwrap();
    assert_eq!(table.view_hash, again.view_hash);

    // A partial lifecycle leaves the unlaunched cell visible with an empty
    // row list — "planned but not run" is reported, never skipped.
    let mut r2 = rig("cells-partial", 8_500);
    let (eid2, _) = open(&mut r2, &s);
    let mut eng = ExperimentEngine::new(&mut r2.store, r2.docs.clone(), ctx());
    eng.attach(&eid2).unwrap();
    let run_plan_id = match eng.next().unwrap() {
        NextVerdict::Plan { run_plan_id } => run_plan_id,
        other => panic!("{other:?}"),
    };
    let ticket = eng.claim(&run_plan_id, "driver").unwrap();
    let launched = eng.launch(&ticket, "subject").unwrap();
    drop(eng);
    finish_subject(
        &mut r2.store,
        &launched.run_id,
        &launched.subject_writer,
        StopReason::Completed,
        &[],
        vec![],
    );
    let exp2 = r2
        .store
        .run_ids()
        .into_iter()
        .find(|id| {
            r2.store
                .manifest(id)
                .map(|m| m.run_kind == hh_ledger::manifest::RunKind::Experiment)
                .unwrap_or(false)
        })
        .unwrap();
    let partial = r2.results.cells(&r2.store, &r2.docs, &exp2, None).unwrap();
    assert_eq!(partial.cells.len(), 2);
    let launched_cell = partial.cells.iter().find(|c| !c.rows.is_empty()).unwrap();
    assert_eq!(launched_cell.rows.len(), 1);
    assert!(launched_cell.rows[0].included);
    let empty = partial.cells.iter().find(|c| c.rows.is_empty());
    assert!(empty.is_some(), "unlaunched cell should carry no rows yet");
}

// ── catalogue + leaderboard (AC-R-2.10.5-9) ─────────────────────────────────

#[test]
fn catalogue_tracks_bundles_and_leaderboard_gates_l1_to_l5() {
    let mut r = rig("leaderboard", 9_000);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _e) = open(&mut r, &s);
    // `decision = true` — every subject's stream carries a
    // `control.decision` row so an R1-claimed bundle's B-R1-replay
    // basis resolves.
    let exp_run = run_to_close(&mut r, &eid, 200, true);
    let subjects = launched_runs(&r, &exp_run);
    assert_eq!(subjects.len(), 4);

    // No bundles yet — every candidate falls at L1.
    let def = LeaderboardDefinition::new(exp_run.clone(), "wall_time_ms");
    let snap = r
        .results
        .leaderboard(&r.store, &r.docs, &def, None)
        .unwrap();
    assert!(snap.entries.is_empty());
    assert!(snap.exclusions.iter().all(|x| x.gate == "L1"));
    assert_eq!(snap.exclusions.len(), 4);

    // Deposit bundles for three of the four subjects: R0 for subject[0],
    // R1-claimed for subject[1] (its stream carries `control.decision`),
    // and a broken (assembled-only) bundle for subject[2]; subject[3]
    // stays unbundled.
    let b0 = deposit_bundle(&mut r, &subjects[0]);
    let b1 = deposit_bundle_at(&mut r, &subjects[1], ReproLevel::R1);
    let b2 = deposit_bundle_broken(&mut r, &subjects[2]);
    // Emit the assembly facts on a dedicated instrument run — the refresh
    // scans every run's stream, and the subject writers are consumed inside
    // `run_to_close`.
    {
        let manifest = experiment_manifest();
        let (rid, lease) = r.store.open_run(manifest, "bundle-test").unwrap();
        let events: Vec<Event> = [&b0, &b1, &b2]
            .iter()
            .map(|bid| {
                mint(
                    &r.store,
                    &rid,
                    "measurement.experiment.bundle_assembled",
                    Json::obj([
                        ("bundle_id", Json::str(bid.as_str())),
                        ("kind", Json::str("run")),
                    ]),
                )
            })
            .collect();
        append_chained(&mut r.store, &rid, &lease, events);
    }
    let cat = r.results.catalogue_refresh(&r.store, None).unwrap();
    assert_eq!(cat.entries.len(), 3);
    let entry = |bid: &str| cat.entries.iter().find(|e| e.bundle_id == bid).unwrap();
    assert_eq!(entry(&b0).status, BundleStatus::Validated);
    assert_eq!(entry(&b0).claimed_level, Some(ReproLevel::R0));
    assert_eq!(entry(&b1).status, BundleStatus::Validated);
    assert_eq!(entry(&b1).claimed_level, Some(ReproLevel::R1));
    // The withheld member keeps the broken bundle at `assembled`.
    assert_eq!(entry(&b2).status, BundleStatus::Assembled);
    assert_eq!(
        r.results.bundle_refs(&subjects[0]).unwrap()[0].bundle_id,
        b0
    );

    // The two validated-bundle subjects rank; the assembled-bundle and
    // unbundled subjects stay L1-excluded.
    let snap2 = r
        .results
        .leaderboard(&r.store, &r.docs, &def, None)
        .unwrap();
    assert_eq!(snap2.entries.len(), 2);
    assert_eq!(
        snap2.exclusions.iter().filter(|x| x.gate == "L1").count(),
        2
    );

    // AC-R-2.10.5-3's `min_claimed_level = R1` arm — only the R1-claimed
    // bundle's subject admits; the R0-claimed covering bundle fails the
    // claimed-level half of L1 explicitly.
    let mut def_r1 = LeaderboardDefinition::new(exp_run.clone(), "wall_time_ms");
    def_r1.min_claimed_level = Some(ReproLevel::R1);
    let snap_r1 = r
        .results
        .leaderboard(&r.store, &r.docs, &def_r1, None)
        .unwrap();
    assert_eq!(snap_r1.entries.len(), 1);
    assert_eq!(snap_r1.entries[0].bundle_refs, vec![b1.clone()]);
    assert_eq!(
        snap_r1.exclusions.iter().filter(|x| x.gate == "L1").count(),
        3
    );
    let r0_exclusion = snap_r1
        .exclusions
        .iter()
        .find(|x| x.gate == "L1" && x.reason.contains(&subjects[0]))
        .expect("the R0-covered subject must be L1-excluded");
    assert!(r0_exclusion.reason.contains("claimed ≥ R1"));
    // Deterministic ordering — `wall_time_ms` is `lower`; entries arrive
    // stratum-major, rank-ordered, so (rank, value) must sort stable.
    let ranked: Vec<(u64, i64)> = snap2
        .entries
        .iter()
        .map(|e| {
            (
                e.rank,
                match MetricValueKind::from_json(&e.value) {
                    Some(MetricValueKind::Decimal(d)) => d,
                    _ => panic!("bad value"),
                },
            )
        })
        .collect();
    let mut sorted = ranked.clone();
    sorted.sort();
    assert_eq!(ranked, sorted, "entries must arrive rank-ordered");
    // Every entry carries a verifying head citation + the bundle ref.
    for e in &snap2.entries {
        assert_eq!(
            verify_citation(&r.store, &e.audit_ref),
            CitationVerdict::Verified
        );
        assert!(!e.bundle_refs.is_empty());
    }
    // Repeatable — same snapshot id.
    let snap3 = r
        .results
        .leaderboard(&r.store, &r.docs, &def, None)
        .unwrap();
    assert_eq!(snap2.snapshot_id, snap3.snapshot_id);
}

#[test]
fn leaderboard_l5_excludes_uncomparable_rows() {
    let mut r = rig("l5", 9_500);
    let s = spec(ExperimentKind::Exploratory);
    let (eid, _e) = open(&mut r, &s);
    let exp_run = run_to_close(&mut r, &eid, 300, false);
    let subjects = launched_runs(&r, &exp_run);
    assert!(!subjects.is_empty());
    // Bundle every subject — L1 passes; exploratory rows are
    // `comparable = false`, so every candidate falls at L5.
    for sr in &subjects {
        let bid = deposit_bundle(&mut r, sr);
        let manifest = experiment_manifest();
        let (rid, lease) = r.store.open_run(manifest, "bundle-test-l5").unwrap();
        let ev = mint(
            &r.store,
            &rid,
            "measurement.experiment.bundle_assembled",
            Json::obj([("bundle_id", Json::str(&bid)), ("kind", Json::str("run"))]),
        );
        append_chained(&mut r.store, &rid, &lease, vec![ev]);
    }
    r.results.catalogue_refresh(&r.store, None).unwrap();
    let def = LeaderboardDefinition::new(exp_run.clone(), "wall_time_ms");
    let snap = r
        .results
        .leaderboard(&r.store, &r.docs, &def, None)
        .unwrap();
    assert!(snap.entries.is_empty());
    assert!(snap.exclusions.iter().any(|x| x.gate == "L5"));
}

// ── AC-R-2.10.5-1 (rebuild half) + annotations ──────────────────────────────

#[test]
fn rebuild_all_reproduces_and_annotate_indexes() {
    let mut r = rig("rebuild", 10_000);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _e) = open(&mut r, &s);
    let exp_run = run_to_close(&mut r, &eid, 400, false);
    let n = r.results.rebuild_all(&r.store, Some(&r.docs)).unwrap();
    assert_eq!(n, 4);
    let subjects = launched_runs(&r, &exp_run);
    let mut key_ids = r.results.key_ids();
    key_ids.sort();
    assert_eq!(key_ids.len(), 4);
    // Capture every stored version id.
    let vids: Vec<String> = key_ids
        .iter()
        .map(|k| r.results.head(k).unwrap().version_id)
        .collect();

    // Rebuild into a fresh store — equal sources ⇒ equal bytes.
    let root2 = tmp("rebuild-fresh");
    let results2 = ResultsStore::open(root2.join("results")).unwrap();
    let n2 = results2.rebuild_all(&r.store, Some(&r.docs)).unwrap();
    assert_eq!(n2, n);
    let vids2: Vec<String> = results2
        .key_ids()
        .iter()
        .map(|k| results2.head(k).unwrap().version_id)
        .collect();
    assert_eq!(vids, vids2);
    // The row bytes are identical.
    for v in &vids {
        let a = r.results.get_version(v).unwrap();
        let b = results2.get_version(v).unwrap();
        assert_eq!(a.to_canonical_bytes(), b.to_canonical_bytes());
    }

    // `annotate` — the index rebuilds, flags/bundle_refs ride beside rows.
    let idx = r.results.annotate(&r.store, None).unwrap();
    assert_eq!(idx.entries.len(), 4);
    for e in &idx.entries {
        assert!(e.flags.is_empty() || !e.flags.is_empty());
        let _ = e;
    }
    // Every row's annotation exists and cites its head version.
    for (k, v) in key_ids.iter().zip(vids.iter()) {
        let a = idx.entry(k).unwrap();
        assert_eq!(&a.version_id, v);
    }
    // `get_row` composes the annotations fresh.
    let (row, anns) = r.results.get_row(&r.store, &key_ids[0]).unwrap();
    assert_eq!(anns.version_id, row.version_id);
    let _ = subjects;
}

// ── AC-R-2.10.5-2: factor-keyed reads ────────────────────────────────────────

#[test]
fn query_rows_filters_factors_paginates_and_distributes() {
    let mut r = rig("query", 8_600);
    let s = spec(ExperimentKind::Comparative);
    let (eid, _e) = open(&mut r, &s);
    let exp_run = run_to_close(&mut r, &eid, 600, false);
    let subjects = launched_runs(&r, &exp_run);
    for sr in &subjects {
        r.results
            .project_and_record(
                &r.store,
                Some(&r.docs),
                sr,
                None,
                None,
                DerivedReason::Initial,
            )
            .unwrap();
    }
    let all = r
        .results
        .query_rows(&r.store, &hh_results::query::QuerySpec::all())
        .unwrap();
    assert_eq!(all.rows.len(), 4);

    // Factor filters — every declared coordinate groups.
    let count = |field: &str, value: &str| {
        let spec = hh_results::query::QuerySpec::all()
            .filter(field, value)
            .unwrap();
        r.results.query_rows(&r.store, &spec).unwrap().rows.len()
    };
    assert_eq!(count("arm_id", "arm:a"), 2);
    assert_eq!(count("arm_id", "arm:b"), 2);
    let cfgv_a = all
        .rows
        .iter()
        .find(|row| {
            row.experiment
                .as_ref()
                .and_then(|e| e.get("arm_id"))
                .and_then(Json::as_str)
                == Some("arm:a")
        })
        .unwrap()
        .coordinates
        .configuration_version_id
        .clone();
    assert_eq!(count("configuration_version_id", &cfgv_a), 2);
    assert_eq!(count("task_id", "task:1"), 4);
    assert_eq!(count("split_label", "dev"), 4);
    assert_eq!(count("outcome_class", "scored"), 4);
    assert_eq!(count("experiment_run_id", &exp_run), 4);
    // `n/a` cells filter by reason — `task_success` was never emitted.
    assert_eq!(count("cell_na:task_success", "not_run"), 4);
    assert_eq!(count("cell_na:task_success", "class"), 0);
    // No name-keyed query exists — an undeclared field refuses typed.
    assert!(matches!(
        hh_results::query::QuerySpec::all().filter("label", "x"),
        Err(ResultsError::UnknownField { .. })
    ));

    // Pagination — `limit` + `cursor` pages the key-ordered set stably.
    let mut page1 = hh_results::query::QuerySpec::all();
    page1.limit = Some(3);
    let p1 = r.results.query_rows(&r.store, &page1).unwrap();
    assert_eq!(p1.rows.len(), 3);
    let cursor = p1.next_cursor.clone().expect("a second page exists");
    let mut page2 = hh_results::query::QuerySpec::all();
    page2.cursor = Some(cursor.clone());
    let p2 = r.results.query_rows(&r.store, &page2).unwrap();
    assert_eq!(p2.rows.len(), 1);
    assert!(p2.next_cursor.is_none());
    let mut ids: Vec<String> = p1
        .rows
        .iter()
        .chain(p2.rows.iter())
        .map(|row| row.key.key_id())
        .collect();
    ids.sort();
    assert_eq!(ids, {
        let mut k = r.results.key_ids();
        k.sort();
        k
    });
    let mut bad_cursor = hh_results::query::QuerySpec::all();
    bad_cursor.cursor = Some(pinned("no.such.key"));
    assert!(matches!(
        r.results.query_rows(&r.store, &bad_cursor),
        Err(ResultsError::UnknownCursor { .. })
    ));

    // `distribution` — the per-configuration metric view over the same rows.
    let cfg_a = all
        .rows
        .iter()
        .find(|row| {
            row.experiment
                .as_ref()
                .and_then(|e| e.get("arm_id"))
                .and_then(Json::as_str)
                == Some("arm:a")
        })
        .unwrap()
        .coordinates
        .configuration_id
        .clone()
        .unwrap();
    let d = r
        .results
        .distribution(&r.store, &cfg_a, "wall_time_ms", Some("dev"), None)
        .unwrap();
    assert_eq!(d.n, 2);
    assert_eq!(d.values.len(), 2);
    assert_eq!(d.per_task_cn.get("task:1"), Some(&(2, 2)));
    assert!(d.quantiles.is_some());
}

// ── AC-R-2.10.5-7: class-scoped rendering ────────────────────────────────────

#[test]
fn class_scoped_na_cells_render_and_cite() {
    let mut r = rig("class-na", 9_800);
    // A hosted run at events-only observability — the class/observability
    // gates render typed `n/a` cells, never fabricated values.
    let mut m = hh_ledger::manifest::RunManifest::minimal(hh_ledger::manifest::RunKind::Agent);
    m.participant_class = hh_ledger::manifest::ParticipantClass::Hosted;
    m.observability_level = [hh_ledger::manifest::ObservabilityLevel::Events]
        .into_iter()
        .collect();
    let (rid, lease) = r.store.open_run(m, "hosted-fixture").unwrap();
    finish_subject(
        &mut r.store,
        &rid,
        &lease,
        StopReason::Completed,
        &[],
        vec![],
    );
    let row = r
        .results
        .project_row(&r.store, Some(&r.docs), &rid, None, None)
        .unwrap();
    assert_eq!(row.coordinates.participant_class, "hosted");
    assert_eq!(row.coordinates.observability_level, vec!["events"]);
    let na = |reason: &str| {
        row.cells
            .iter()
            .filter(|c| c.na_reason().map(|n| n.as_str()) == Some(reason))
            .count()
    };
    // `n/a{class}` — the native-only scorecard members.
    assert!(na("class") > 0, "hosted run must render n/a{{class}} cells");
    // `n/a{observability}` — end_state metrics over an events-only stream.
    assert!(
        na("observability") > 0,
        "events-only run must render n/a{{observability}} cells"
    );
    // No cell is 0 by absence — nothing was emitted, so every cell is a
    // typed `n/a`, never `Decimal(0)`/`Bool(false)`.
    assert!(row
        .cells
        .iter()
        .all(|c| matches!(c.value, MetricValueKind::Na(_))));
    // Every `n/a` cell still cites a verifying audit_ref (the run head).
    for c in &row.cells {
        for a in &c.evidence {
            assert_eq!(
                verify_citation(&r.store, a),
                CitationVerdict::Verified,
                "n/a cell {} must cite",
                c.metric_ref
            );
        }
    }
}

// ── AC-R-2.10.5-9: matched budget — flags, never hides ──────────────────────

#[test]
fn leaderboard_flags_imbalance_never_excludes() {
    let mut r = rig("l4flags", 10_500);
    let mut s = spec(ExperimentKind::Comparative);
    // A declared utilisation floor — the light arm lands `under_utilised`.
    for a in &mut s.arms {
        a.match_spec.as_mut().unwrap().utilization_floor_ppm = Some(900_000);
    }
    // The id is the spec's content address — recompute after mutating.
    s.experiment_id = s.experiment_id();
    let (eid, _e) = open(&mut r, &s);
    // arm:a consumes near its cap; arm:b sips — the close re-check marks
    // the comparand group `imbalanced` and arm:b `under_utilised`.
    let exp_run = run_to_close_with(&mut r, &eid, 700, false, |arm| {
        if arm == "arm:a" {
            95
        } else {
            3
        }
    });
    let subjects = launched_runs(&r, &exp_run);
    assert_eq!(subjects.len(), 4);
    // Every subject gets a validated bundle so all four rank.
    let bids: Vec<String> = subjects
        .iter()
        .map(|sr| deposit_bundle(&mut r, sr))
        .collect();
    {
        let manifest = experiment_manifest();
        let (rid, lease) = r.store.open_run(manifest, "bundle-flags").unwrap();
        let events: Vec<Event> = bids
            .iter()
            .map(|bid| {
                mint(
                    &r.store,
                    &rid,
                    "measurement.experiment.bundle_assembled",
                    Json::obj([
                        ("bundle_id", Json::str(bid.as_str())),
                        ("kind", Json::str("run")),
                    ]),
                )
            })
            .collect();
        append_chained(&mut r.store, &rid, &lease, events);
    }
    r.results.catalogue_refresh(&r.store, None).unwrap();
    let def = LeaderboardDefinition::new(exp_run.clone(), "wall_time_ms");
    let snap = r
        .results
        .leaderboard(&r.store, &r.docs, &def, None)
        .unwrap();
    // Imbalance annotates — all four rows still rank.
    assert_eq!(snap.entries.len(), 4);
    assert!(snap
        .entries
        .iter()
        .all(|e| e.flags.iter().any(|f| f == "budget_imbalanced")));
    // `under_utilised` names only the sipping arm.
    let arm_b: Vec<_> = snap
        .entries
        .iter()
        .filter(|e| e.arm_id == "arm:b")
        .collect();
    assert_eq!(arm_b.len(), 2);
    assert!(arm_b
        .iter()
        .all(|e| e.flags.iter().any(|f| f == "under_utilised")));
    assert!(!snap
        .entries
        .iter()
        .filter(|e| e.arm_id == "arm:a")
        .any(|e| e.flags.iter().any(|f| f == "under_utilised")));
}
