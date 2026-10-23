//! R4.7e live legs (ADR-0363; DF-S1.21-3's live half; R-2.7.2b) — the
//! live `CriticDeclaration{kind = judge}` binding over real providers.
//! Two `#[ignore]`d cells; `scripts/live-verify.sh judge-calibrate` /
//! `judge-run` drive them (USD 15 ticket allocation; every call's usage
//! rides a `JUDGE_USAGE` row and the hosted aggregate prices via
//! `LIVE_JUDGE_USAGE`).
//!
//! The two judges are a **different-family pair**: the local lane
//! (`HH_LOCAL_BASE_URL`/`HH_LOCAL_MODEL`, `openai-compatible@1`) and the
//! hosted `openai@1` lane (`gpt-4.1-mini-2025-04-14` —
//! `env:OPENAI_API_KEY`, a coordinate; the value resolves inside the
//! transport and lands in no record). `judge-calibrate` folds the
//! committed labelled set (`fixtures/r4_7e/labelled_cases.json` — the
//! stage3_v1-derived cases) into one `CalibrationRecord` per judge;
//! `judge-run` binds the calibrated pair at link (independence +
//! `RedundantJudge` + calibration consumed for real), drives a real
//! `Driver` run whose beneficiary model is the local lane's
//! `GatewayModel`, and proves the judged half lands durable: per-critic
//! `claim.reconciled{detector: judged}` rows, `verification.validator.*`
//! rows citing `idp:` model_io blobs the blob pool reads back, per-call
//! usage rows — then `RecordedJudge::from_prefix` replays the recorded
//! verdicts byte-identically with no provider call.
//!
//! `provided` markers are names-only (`env_or` prints `unprovided —
//! skipped: NAME`, the leg records `skipped(credential-unprovided)`).
//!
//! Markers (unanchored — libtest may prefix the line):
//!   * `JUDGE_CALIBRATE: <critic_ref> agreement_ppm=<n> consistency_ppm=<n> decided=<n>/<n>`
//!   * `JUDGE_USAGE: <critic_ref> call=<id> usage=<json>` — per live call
//!   * `LIVE_JUDGE_USAGE: {"input_total":..,"output_total":..,"calls":..}`
//!     — the hosted lane's aggregate (the leg's spend pricing).
//!   * `LIVE_MODEL: <served-model>` — a provider-reported served model.
//!   * `JUDGE_RUN: finished=<status> judged_verdicts=<n> reconciled_judged=<n>
//!     model_io=<n> replay_ppm=<n>`

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use hharness_control::driver::{Driver, DriverConfig};
use hharness_control::loops::{JudgePort, RecordedJudge};
use hharness_control::output::{ParamKind, ParamSpec, SurfaceSpec};
use hharness_control::policy::{EnvelopePolicy, JudgedSpec};
use hharness_control::react::{react_preset, ReactMinimal};
use hharness_control::strategy::{ControlContext, StrategyParams};
use hharness_embed::context_adapters::AdapterLane;
use hharness_embed::gateway_model::{GatewayModel, ModelLane};
use hharness_embed::judge_adapters::{
    calibrate, judge_fingerprint, link_judged_pair, CalibrationCase, CalibrationRun, JudgeBindSpec,
};
use hharness_embed::runtime::{EmbedGate, KernelAssembler, KernelSink};
use hharness_identity::kinds::RecordKind;
use hharness_identity::refs::VersionedRef;
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::Store;
use hharness_provenance::record::ProvenanceRecord;
use hharness_verification::critic_rt::CalibrationContext;
use hharness_verification::critics::{CalibrationRecord, CriticDeclaration, IndependenceVector};
use hharness_verification::reconciler::ReconcilerDeclaration;
use hharness_verification::vocab::{
    AdmissionMode, CalibrationStatus, ChargedTo, CriticKind, CriticUse, Detector, DivergenceClass,
    EvidenceMode, OracleClass, Site, SnapshotIndependence, VerdictType,
};
use hharness_wire::json::Json;

// ── provisioning ────────────────────────────────────────────────────────

/// `env_or(name)` — the provisioned value (never printed); absent → the
/// leg's `unprovided — skipped` marker + `None`.
fn env_or(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => {
            println!("unprovided — skipped: {name}");
            None
        }
    }
}

/// The bound judge lane (local or hosted); the family label the
/// different-family pair check reads.
struct BoundLane {
    family: String,
    lane: AdapterLane,
}

/// The local judge lane — `openai-compatible@1` over `HH_LOCAL_*`.
fn local_lane() -> Option<BoundLane> {
    let base = env_or("HH_LOCAL_BASE_URL")?;
    let model = env_or("HH_LOCAL_MODEL")?;
    Some(BoundLane {
        family: "local".to_string(),
        lane: AdapterLane {
            dialect_ref: "openai-compatible@1".to_string(),
            endpoint_ref: base,
            provider_model_id: model,
            credential_ref: None,
            profile_ref: "profile/judge-live.local".to_string(),
            ..AdapterLane::default()
        },
    })
}

/// The hosted judge lane — `openai@1` `gpt-4.1-mini` (the cheapest
/// adequate judge; `env:OPENAI_API_KEY` is a coordinate, never a value).
fn hosted_lane() -> Option<BoundLane> {
    env_or("OPENAI_API_KEY")?;
    Some(BoundLane {
        family: "openai".to_string(),
        lane: AdapterLane {
            dialect_ref: "openai@1".to_string(),
            endpoint_ref: "https://api.openai.com/v1".to_string(),
            provider_model_id: "gpt-4.1-mini-2025-04-14".to_string(),
            credential_ref: Some("env:OPENAI_API_KEY".to_string()),
            profile_ref: "profile/judge-live.hosted".to_string(),
            ..AdapterLane::default()
        },
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ── the declared labelled set ───────────────────────────────────────────

struct LabelledSet {
    set_ref: String,
    rubric_ref: String,
    rubric_text: String,
    task_family: String,
    cases: Vec<CalibrationCase>,
}

fn labelled_set() -> LabelledSet {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/r4_7e/labelled_cases.json");
    let d = hharness_wire::json::parse(
        &std::fs::read_to_string(&root).expect("the declared labelled set is committed"),
    )
    .expect("labelled set parses");
    let cases = match d.get("cases") {
        Some(Json::Arr(items)) => items
            .iter()
            .map(|c| CalibrationCase {
                case_id: c.get("case_id").and_then(Json::as_str).unwrap().to_string(),
                rendered: c
                    .get("rendered")
                    .and_then(Json::as_str)
                    .unwrap()
                    .to_string(),
                label_agree: matches!(c.get("label_agree"), Some(Json::Bool(true))),
                verdict_class: c
                    .get("verdict_class")
                    .and_then(Json::as_str)
                    .unwrap_or("undivided")
                    .to_string(),
            })
            .collect(),
        _ => panic!("cases"),
    };
    LabelledSet {
        set_ref: d.get("set_ref").and_then(Json::as_str).unwrap().to_string(),
        rubric_ref: d
            .get("rubric_ref")
            .and_then(Json::as_str)
            .unwrap()
            .to_string(),
        rubric_text: d
            .get("rubric_text")
            .and_then(Json::as_str)
            .unwrap()
            .to_string(),
        task_family: "submission-match".to_string(),
        cases,
    }
}

// ── leg helpers ─────────────────────────────────────────────────────────

/// `usage_totals(usages)` — sum `view.input_total`/`view.output_total`
/// over the per-call usage rows (the leg's spend measure).
fn usage_totals(usages: &[Json]) -> (i64, i64) {
    // Each row's `view` is the call's `usage{…}` member — the normalized
    // `TokenVector` rides `view.record.view{input_total, output_total}`.
    usages.iter().fold((0, 0), |(i, o), u| {
        let view = u
            .get("view")
            .and_then(|v| v.get("record"))
            .and_then(|r| r.get("view"))
            .cloned()
            .unwrap_or(Json::Null);
        (
            i + view.get("input_total").and_then(Json::as_int).unwrap_or(0),
            o + view.get("output_total").and_then(Json::as_int).unwrap_or(0),
        )
    })
}

/// `print_usages(usages)` — one `JUDGE_USAGE` line per live call + the
/// served-model marker (provider-reported, the drift fact).
fn print_usages(usages: &[Json]) {
    for u in usages {
        println!(
            "JUDGE_USAGE: {} call={} usage={}",
            u.get("critic_ref").and_then(Json::as_str).unwrap_or("?"),
            u.get("call_id").and_then(Json::as_str).unwrap_or("?"),
            u.get("view")
                .cloned()
                .unwrap_or(Json::Null)
                .to_canonical_string(),
        );
        if let Some(m) = u.get("served_model").and_then(Json::as_str) {
            println!("LIVE_MODEL: {m}");
        }
    }
}

/// `decided(observations)` — the cases the judge answered decidable.
fn decided(obs: &[Json]) -> usize {
    obs.iter()
        .filter(|o| o.get("status").and_then(Json::as_str) == Some("decided"))
        .count()
}

/// `calibrate_lane(lane, critic_ref, set)` — one live calibration: the
/// measured fold (agreement + consistency) mints the `CalibrationRecord`;
/// every call's usage prints.
fn calibrate_lane(lane: &AdapterLane, critic_ref: &str, set: &LabelledSet) -> CalibrationRun {
    calibrate(
        lane,
        critic_ref,
        &set.set_ref,
        &set.cases,
        &set.rubric_text,
        &set.rubric_ref,
        &set.task_family,
        now_ms(),
    )
    .unwrap_or_else(|e| {
        println!("JUDGE_CALIBRATE: {critic_ref} typed-failure {e:?}");
        panic!("calibration consult failed typed — the leg records the failure")
    })
}

/// `calibration_ctx(lane, record, set)` — the fold context
/// `calibration_status_at` reads at link (fresh period, same snapshot —
/// the record just minted folds `active`).
fn calibration_ctx(set: &LabelledSet, fingerprint: &str) -> CalibrationContext {
    CalibrationContext {
        judge_snapshot_fingerprint: fingerprint.to_string(),
        rubric_ref: set.rubric_ref.clone(),
        task_family: set.task_family.clone(),
        now_ms: now_ms(),
        recalibration_period_ms: 30 * 24 * 60 * 60 * 1000,
    }
}

// ── the run's fixtures (r4_7e's shapes) ─────────────────────────────────

fn ctx() -> ControlContext {
    ControlContext {
        process_ref: "proc-live-judge".into(),
        plan: vec![],
        boundary: react_preset(),
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
    // The production `submit_surface_spec` (open.rs): `semantic_id` is
    // `hh.submit/1`, `risk_class` is `SUBMIT_RISK` (READ_ONLY) — an
    // absent risk records UNKNOWN and the gate's `authorized` row then
    // "lowers" it (a SchemaViolation, not a pass).
    SurfaceSpec {
        surface_id: "hh.submit".into(),
        semantic_id: "hh.submit/1".into(),
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
        risk_class: Some(hharness_ontology::risk::RiskClass::READ_ONLY),
        family: None,
    }
}

fn prov() -> ProvenanceRecord {
    ProvenanceRecord::kernel("hh-test/live", 0)
}

fn judge_decl(
    critic_version: &str,
    calibration_ref: Option<&str>,
    snapshot: SnapshotIndependence,
) -> CriticDeclaration {
    CriticDeclaration {
        critic_ref: VersionedRef::pinned(RecordKind::Validator, critic_version, prov()),
        oracle_id: format!("oracle:{critic_version}"),
        oracle_class: OracleClass::Judge,
        deterministic: false,
        critic_kind: CriticKind::Judge,
        evidence_mode: [EvidenceMode::LedgerView, EvidenceMode::Transcript]
            .into_iter()
            .collect(),
        admission_mode: AdmissionMode::PrincipalOnly,
        probe_capabilities: vec![],
        independence: IndependenceVector {
            snapshot,
            ..IndependenceVector::kernel_independent()
        },
        site: Site::Instrument,
        use_: CriticUse::Report,
        placement: None,
        verdict_type: VerdictType::Graded,
        comparative: false,
        rubric_ref: None,
        calibration_ref: calibration_ref.map(str::to_string),
        held_out_from: vec![],
        debt: Some("debt:judge-live".into()),
        charged_to: ChargedTo::Instrument,
        calibration_status: CalibrationStatus::Active,
        deterministic_coverage_at_placement: false,
    }
}

fn bind_spec(
    critic_version: &str,
    bound: &BoundLane,
    beneficiaries: Vec<String>,
    calibration: Option<(CalibrationRecord, CalibrationContext)>,
    set: &LabelledSet,
    snapshot: SnapshotIndependence,
) -> JudgeBindSpec {
    let calibration_ref = calibration.as_ref().map(|(r, _)| r.calibration_id.as_str());
    JudgeBindSpec {
        declaration: judge_decl(critic_version, calibration_ref, snapshot),
        lane: bound.lane.clone(),
        rubric_text: set.rubric_text.clone(),
        family: bound.family.clone(),
        judge_snapshot_fingerprint: judge_fingerprint(&bound.lane),
        beneficiary_families: beneficiaries,
        calibration,
        run_ref: "run:r4_7e-live".to_string(),
    }
}

// ── judge-calibrate ─────────────────────────────────────────────────────

/// `judge-calibrate` — each bound judge (local + hosted) answers the
/// declared labelled set twice per case (the self-consistency pass); the
/// measured fold mints a `CalibrationRecord` per judge. The records +
/// observations land the committed `live_calibrations.json` evidence
/// (`HH_JUDGE_CAL_OUT`); every call's usage rides `JUDGE_USAGE` and the
/// hosted aggregate prices `LIVE_JUDGE_USAGE`.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh judge-calibrate"]
fn r47e_live_judge_calibrate() {
    let (Some(local), Some(hosted)) = (local_lane(), hosted_lane()) else {
        return;
    };
    let set = labelled_set();
    let mut runs: Vec<(String, &BoundLane, CalibrationRun)> = Vec::new();
    for (bound, critic_ref) in [
        (&local, "judge:live-local.v1"),
        (&hosted, "judge:live-hosted.v1"),
    ] {
        let run = calibrate_lane(&bound.lane, critic_ref, &set);
        let d = decided(&run.observations);
        println!(
            "JUDGE_CALIBRATE: {} agreement_ppm={} consistency_ppm={} decided={}/{}",
            critic_ref,
            run.record.agreement.value_ppm,
            run.record.position_consistency_ppm,
            d,
            set.cases.len(),
        );
        if d == 0 {
            println!(
                "unprovided — skipped: {critic_ref} answered no decidable calibration consult (provider-degraded)"
            );
            return;
        }
        print_usages(&run.usages);
        runs.push((critic_ref.to_string(), bound, run));
    }
    // The hosted lane's aggregate — the leg's measured spend.
    let hosted_run = &runs[1].2;
    let (i, o) = usage_totals(&hosted_run.usages);
    println!(
        "LIVE_JUDGE_USAGE: {}",
        Json::obj([
            ("input_total", Json::Int(i)),
            ("output_total", Json::Int(o)),
            ("calls", Json::Int(hosted_run.usages.len() as i64)),
        ])
        .to_canonical_string()
    );
    // The committed evidence fixture — the minted records' member view +
    // the per-case observations (names + measurements, never a value).
    if let Ok(out) = std::env::var("HH_JUDGE_CAL_OUT") {
        if !out.is_empty() {
            let doc = Json::obj([
                ("schema", Json::str("hh-r4_7e-live-calibrations/1")),
                ("set_ref", Json::str(set.set_ref.clone())),
                ("rubric_ref", Json::str(set.rubric_ref.clone())),
                (
                    "records",
                    Json::Arr(
                        runs.iter()
                            .map(|(r, _, run)| {
                                Json::obj([
                                    ("critic_ref", Json::str(r.clone())),
                                    (
                                        "calibration_id",
                                        Json::str(run.record.calibration_id.clone()),
                                    ),
                                    (
                                        "judge_snapshot_fingerprint",
                                        Json::str(run.record.judge_snapshot_fingerprint.clone()),
                                    ),
                                    (
                                        "agreement",
                                        Json::obj([
                                            (
                                                "statistic",
                                                Json::str(run.record.agreement.statistic.clone()),
                                            ),
                                            (
                                                "value_ppm",
                                                Json::Int(run.record.agreement.value_ppm as i64),
                                            ),
                                        ]),
                                    ),
                                    (
                                        "position_consistency_ppm",
                                        Json::Int(run.record.position_consistency_ppm as i64),
                                    ),
                                    (
                                        "n_per_verdict_class",
                                        Json::Int(run.record.n_per_verdict_class as i64),
                                    ),
                                    ("error_rates", run.record.error_rates.clone()),
                                    ("calibrated_at", Json::Int(run.record.calibrated_at as i64)),
                                    ("status", Json::str(run.record.status.as_str().to_string())),
                                    ("observations", Json::Arr(run.observations.clone())),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]);
            std::fs::write(&out, doc.to_canonical_string() + "\n")
                .expect("the calibration evidence fixture writes");
            println!("JUDGE_CALIBRATE: evidence-recorded -> {out}");
        }
    }
}

// ── judge-run ───────────────────────────────────────────────────────────

/// `judge-run` — the different-family pair bound at link over real
/// calibrations, then a real `Driver` run (the local lane's
/// `GatewayModel` is the beneficiary `ModelPort`, `ReactMinimal` the
/// strategy, the real `KernelSink`/`EmbedGate`/`KernelAssembler` the
/// ports): the judged loop consult fires at the interpret cadence, the
/// completion gate's `assess` folds each critic through
/// `reconcile_judged` — `claim.reconciled{detector: judged}` rows under
/// per-critic refs — and every consult's `verification.validator.*` rows
/// + `idp:` model_io blobs land durable through the fenced drain.
///   `RecordedJudge::from_prefix` then replays the panel verdict
///   byte-identically — no provider call, the offline binding.
#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh judge-run"]
fn r47e_live_judge_run() {
    let (Some(local), Some(hosted)) = (local_lane(), hosted_lane()) else {
        return;
    };
    let set = labelled_set();
    // Fresh calibrations for the bound snapshots (the link's
    // `calibration_status_at` fold sees `active`).
    let mut cals: Vec<(String, CalibrationRun)> = Vec::new();
    for (bound, critic_ref) in [
        (&local, "judge:live-local.v1"),
        (&hosted, "judge:live-hosted.v1"),
    ] {
        let run = calibrate_lane(&bound.lane, critic_ref, &set);
        if decided(&run.observations) == 0 {
            println!(
                "unprovided — skipped: {critic_ref} answered no decidable calibration consult (provider-degraded)"
            );
            return;
        }
        print_usages(&run.usages);
        cals.push((critic_ref.to_string(), run));
    }
    let hosted_usage = &cals[1].1.usages;
    let (i, o) = usage_totals(hosted_usage);
    println!(
        "LIVE_JUDGE_USAGE: {}",
        Json::obj([
            ("input_total", Json::Int(i)),
            ("output_total", Json::Int(o)),
            ("calls", Json::Int(hosted_usage.len() as i64)),
        ])
        .to_canonical_string()
    );
    // The link — the ticket's binding moment: snapshot axes derived from
    // family facts (the local judge shares the beneficiary's family —
    // `different_snapshot_same_family`; the hosted judge does not —
    // `different_family`), calibration folded, `declare_critic` in full.
    let beneficiaries = vec!["local".to_string()];
    let specs = [
        // The declared snapshot axes name the facts the link derives —
        // the local judge shares the beneficiary's family
        // (`different_snapshot_same_family`), the hosted judge does not
        // (`different_family`); an overclaim would refuse at link.
        bind_spec(
            "judge:live-local.v1",
            &local,
            beneficiaries.clone(),
            Some((
                cals[0].1.record.clone(),
                calibration_ctx(&set, &cals[0].1.record.judge_snapshot_fingerprint),
            )),
            &set,
            SnapshotIndependence::DifferentSnapshotSameFamily,
        ),
        bind_spec(
            "judge:live-hosted.v1",
            &hosted,
            beneficiaries,
            Some((
                cals[1].1.record.clone(),
                calibration_ctx(&set, &cals[1].1.record.judge_snapshot_fingerprint),
            )),
            &set,
            SnapshotIndependence::DifferentFamily,
        ),
    ];
    let panel = link_judged_pair(specs[0].clone(), specs[1].clone()).unwrap_or_else(|e| {
        panic!("link refused the live pair typed — {e:?} (the leg records it, never weakens)")
    });
    println!(
        "JUDGE_LINK: critics={:?} families={:?}",
        panel.critic_refs(),
        panel.families()
    );
    let panel = Arc::new(panel);

    // The run — a real Store + the boundary's own ports.
    let dir = std::env::temp_dir().join(format!("hh-r4_7e-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut store = Store::open_test(&dir, 1_000).unwrap();
    let (run_id, lease) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-live")
        .unwrap();
    let mut sink = KernelSink {
        store: &mut store,
        run_id: run_id.clone(),
        lease: lease.clone(),
        envs: None,
        env_handle: None,
    };
    let mut policy = EnvelopePolicy::stage1_default("b-1");
    policy.loop_policy.judged = Some(JudgedSpec {
        validator_ref: "validator:judged-panel".into(),
        after_turns: 0,
        interval: 1,
        confidence_threshold_ppm: 500_000,
        charged_to: "instrument".into(),
    });
    let policy = policy.seal().unwrap();
    let reconciler = ReconcilerDeclaration {
        reconciler_ref: "reconciler:exec-align.live".into(),
        classes_detected: DivergenceClass::ALL.iter().copied().collect(),
        detector_classes: [Detector::Deterministic, Detector::Judged]
            .into_iter()
            .collect(),
        probe_capabilities: vec![],
        requires_observability: std::collections::BTreeSet::new(),
        applies_to: std::collections::BTreeSet::new(),
        probe_budget_share_cap_ppm: None,
        conditioned_on: None,
        assumption_debt: None,
        judge_ref: Some("judge:live-local.v1".into()),
        calibration_ref: Some(cals[0].1.record.calibration_id.clone()),
        independence_summary: Some("snapshot=different_family".into()),
        no_progress_k: 2,
        enabled: true,
    };
    let mut driver = Driver::open(
        ReactMinimal::new(),
        &ctx(),
        policy,
        &mut sink,
        DriverConfig {
            surfaces: vec![submit_surface()],
            judge: Some(panel.clone()),
            reconciler: Some(reconciler),
            ..DriverConfig::default()
        },
    )
    .unwrap();
    let beneficiary = ModelLane {
        dialect_ref: "openai-compatible@1".to_string(),
        endpoint_ref: local.lane.endpoint_ref.clone(),
        provider_model_id: local.lane.provider_model_id.clone(),
        credential_ref: None,
        substitution_allowed: false,
        profile_ref: "profile/judge-run-beneficiary".to_string(),
        // `tool_choice: "required"` (plan-schema-admitted) — the run's
        // only admissible answer is the `hh_submit` call; a model that
        // would emit prose records the provider's own refusal instead.
        provider_params: BTreeMap::from([("tool_choice".to_string(), Json::str("required"))]),
        tls: Default::default(),
    };
    let mut model = GatewayModel::new(
        beneficiary,
        "Call the `hh_submit` tool exactly once, now, with the text \
         {\"answer\":\"hello\"}. No prose — the tool call alone."
            .to_string(),
        None,
        vec![submit_surface()],
    )
    .unwrap();
    let mut gate = EmbedGate {
        host_surfaces: BTreeMap::new(),
        host_asks: vec![],
    };
    let mut asm = KernelAssembler::default();
    let outcome = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    drop(sink);
    match &outcome {
        Ok(r) => println!("JUDGE_RUN: run outcome {:?}", r.report.stop_reason),
        Err(e) => println!("JUDGE_RUN: driver typed-failure {e:?}"),
    }

    // The run-time judge calls' usage — the hosted member's rows price
    // the leg's measured spend; the local member's are the declared
    // zero-spend lane. `take_model_io` must answer empty — the driver
    // drained every buffered blob through the fenced seam.
    let run_usages = panel.usages();
    print_usages(&run_usages);
    assert!(
        panel.take_model_io().is_empty(),
        "the driver left judge model_io undrained"
    );
    let hosted_run_usage: Vec<Json> = run_usages
        .iter()
        .filter(|u| u.get("critic_ref").and_then(Json::as_str) == Some("judge:live-hosted.v1"))
        .cloned()
        .collect();
    let (ri, ro) = usage_totals(&hosted_run_usage);
    let (ci, co) = usage_totals(hosted_usage);
    println!(
        "LIVE_JUDGE_USAGE: {}",
        Json::obj([
            ("input_total", Json::Int(ci + ri)),
            ("output_total", Json::Int(co + ro)),
            (
                "calls",
                Json::Int((hosted_usage.len() + hosted_run_usage.len()) as i64)
            ),
        ])
        .to_canonical_string()
    );

    let events = store.events(&run_id).unwrap_or(&[]).to_vec();
    let finished = events.iter().any(|e| e.class == "lifecycle.run.finished");
    let verdicts: Vec<&Json> = events
        .iter()
        .filter(|e| {
            matches!(
                e.class.as_str(),
                "verification.validator.verdict" | "verification.critic.verdict"
            ) && e.payload.get("detector").and_then(Json::as_str) == Some("judged")
        })
        .map(|e| &e.payload)
        .collect();
    let reconciled: Vec<&Json> = events
        .iter()
        .filter(|e| {
            e.class == "verification.claim.reconciled"
                && e.payload.get("detector").and_then(Json::as_str) == Some("judged")
        })
        .map(|e| &e.payload)
        .collect();
    // The model_io durability proof: the consult's `hh.model_io/1`
    // blobs deposited through the fenced seam — `take_model_io` answered
    // empty above because the driver drained them; count the pool's
    // `application/json` blobs (the beneficiary's + every judge call's).
    let io_count = std::fs::read_dir(dir.join("blobs"))
        .map(|d| d.filter_map(|e| e.ok()).count())
        .unwrap_or(0);
    // Replay — the offline binding folds the durable rows and answers
    // byte-identically; it consulted no provider. `has_recording` proves
    // the fold was non-trivial (a 0 answer on an unbound ref is an
    // abstention, not replay).
    let recorded = RecordedJudge::from_prefix(&events);
    assert!(
        recorded.has_recording("validator:judged-panel"),
        "the panel's judged verdict is not in the fold — replay has nothing to prove"
    );
    let replay_ppm = recorded.judge(
        &JudgedSpec {
            validator_ref: "validator:judged-panel".into(),
            after_turns: 0,
            interval: 1,
            confidence_threshold_ppm: 500_000,
            charged_to: "instrument".into(),
        },
        &[],
    );
    let recorded_ppm = verdicts
        .iter()
        .filter(|v| {
            v.get("validator_ref").and_then(Json::as_str) == Some("validator:judged-panel")
                || v.get("critic_ref").and_then(Json::as_str) == Some("validator:judged-panel")
        })
        .filter_map(|v| {
            v.get("confidence")
                .or_else(|| v.get("value"))
                .and_then(Json::as_int)
        })
        .next_back()
        .unwrap_or(0);
    assert_eq!(
        replay_ppm as i64, recorded_ppm,
        "replay must answer the recorded confidence byte-identically"
    );
    println!(
        "JUDGE_RUN: finished={finished} judged_verdicts={} reconciled_judged={} model_io={io_count} replay_ppm={replay_ppm}",
        verdicts.len(),
        reconciled.len(),
    );
    if !finished || (verdicts.is_empty() && reconciled.is_empty()) {
        println!("unprovided — skipped: the run produced no judged evidence (provider-degraded)");
    }
}
