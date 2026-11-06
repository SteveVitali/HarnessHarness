//! R5.49 — `hh-study-report/1`: the admitted-study recompute report,
//! recomputed 2026-11-06 over the executed evidence (GATE-G13's
//! `N=0 declared` and GATE-G14's **quote-only confirmation** records
//! stand as committed history). The report recomputes the admitted
//! results from the committed packets — refolded dispositions,
//! re-derived §4.2 denominators, nearest-rank tails, the §3.2
//! missingness bound, envelope maxima and freeze/`protocol_id`
//! identities — asserting byte/consistency equality, never
//! re-picking or fabricating. The parity tranche verified 30/30
//! directional agreement (`hh-parity-tranche-report/1`); the
//! confirmation cohort executed **300/300** — the frozen run +
//! egress + s4-completion-gce + s4-30slot-repairs-gce epochs each
//! covering exactly the prior residue (the four packets stand
//! byte-untouched). Under GATE-G19's complete-cohort admission the
//! efficacy leg carries the registered finite-manifest contrast
//! (REQ-D-107/109) per declared stratum — computed here through the
//! landed `hharness_eval::finite_manifest` machinery over the
//! committed slots' receipt fold. The sharp-null family stays
//! `unavailable{not_computed}` — the sign-flip test is not part of
//! the admitted fold.
//! Fails-without: the `study` module and this battery do not exist
//! at base `b1a8644d` (compile-red evidence).
//!
//! The gated lane performs **no committed-record checkout read** —
//! every committed packet's members are echoed verbatim from the
//! committed report files, exactly like `gate_facts`/`parity_facts`
//! in the r5_48 battery.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_env::preflight::{Leg, LegVerdict};
use hharness_env::study::{
    assemble, check_echo, evaluate_activation, evaluate_confirm_stage, evaluate_costs,
    evaluate_efficacy, evaluate_families, evaluate_freeze, evaluate_ops, evaluate_parity_stage,
    evaluate_pilot, evaluate_preflight, evaluate_useful, recompute_arm_tail, recompute_maxima,
    recompute_missingness, ActivationFacts, ArmTailEcho, ConfirmEpochFacts, ConfirmStageFacts,
    CostFacts, Disposition, EfficacyFacts, EfficacySlot, EfficacyStratum, FamilyFacts,
    FeasibilityEcho, FidelityFold, FreezeFacts, MissingnessEcho, OpsFacts, PacketEcho,
    ParityStageFacts, PilotFacts, PilotSubjectEcho, PreflightStageFacts, StudyFold, SubjectEcho,
    UsefulFacts, LEGS, REQUIRED_FAMILIES, VERSION,
};
use hharness_eval::central_protocol as cp;
use hharness_eval::finite_manifest::{finite_manifest_contrast, ContrastDecl, ContrastError};
use hharness_eval::population::{
    assemble as pop_assemble, Availability, BoundsDecl, PlannedPopulation, PlannedSlot,
    PopulationInput, PopulationKind, SlotSubject,
};
use hharness_lab::experiment::{
    Backoff, CancelPolicy, OrderKind, Pool, ReattemptPolicy, SchedulingPolicy,
};
use hharness_ontology::control::OutcomeClass;
use hharness_wire::json::{self, Json};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ── the frozen R5.41 `golden()` decl (verbatim — supplied external
// stand-ins, never re-picked; same committed input as r5_48) ──────

fn scheduling() -> SchedulingPolicy {
    SchedulingPolicy {
        max_concurrent_runs: 2,
        pools: Vec::<Pool>::new(),
        order: OrderKind::InterleavedBlocked,
        permutation_seed: "perm:fixture".into(),
        start_stagger_ms: 0,
        deadline: None,
        priority: None,
    }
}

fn reattempt() -> ReattemptPolicy {
    ReattemptPolicy {
        max_per_plan: 2,
        max_fraction_of_plans_ppm: 100_000,
        backoff: Backoff {
            min_ms: 1_000,
            multiplier_ppm: 2_000_000,
            max_ms: 60_000,
        },
        error_classes_included: Some(
            cp::RETRYABLE_CLASSES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        ),
        on_cancel: CancelPolicy::Replan,
    }
}

fn roles() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("subject".to_string(), "fp:subject-a".to_string()),
        (
            "summarizer_profile".to_string(),
            "fp:summarizer-unused".to_string(),
        ),
    ])
}

fn arm(arm_id: &str, strategy: &str) -> cp::ArmDecl {
    cp::ArmDecl {
        arm_id: arm_id.into(),
        strategy: strategy.into(),
        artifact_ref: format!("artifact:{strategy}"),
        roles: roles(),
    }
}

/// The frozen declaration: 25 `low` + 20 `high` held-out candidates
/// (quotas 20 + 10 = 30), 6 dev tasks, 2 published exclusions —
/// the committed R5.41 input verbatim.
fn golden() -> cp::ProtocolDecl {
    let mut tasks: Vec<cp::TaskEntry> = (0..25)
        .map(|i| cp::TaskEntry {
            task_id: format!("task:lo-{i:02}"),
            split: cp::TaskSplit::HeldOut,
            stratum: "low".into(),
        })
        .chain((0..20).map(|i| cp::TaskEntry {
            task_id: format!("task:hi-{i:02}"),
            split: cp::TaskSplit::HeldOut,
            stratum: "high".into(),
        }))
        .chain((0..6).map(|i| cp::TaskEntry {
            task_id: format!("task:dev-{i:02}"),
            split: cp::TaskSplit::Dev,
            stratum: "low".into(),
        }))
        .collect();
    tasks.sort_by(|a, b| a.task_id.cmp(&b.task_id));
    cp::ProtocolDecl {
        suite_ref: "suite:tb2-held-out".into(),
        catalogue_identity: "terminal-bench/2.0".into(),
        catalogue_tasks: 89,
        oracle_ref: "oracle:tb2-upstream-pinned".into(),
        runner_ref: "runner:harbor-original".into(),
        tasks,
        exclusions: vec![
            cp::Exclusion {
                task_id: "task:ex-0".into(),
                basis: cp::OUTCOME_BLIND_EXCLUSION_BASES[0].into(),
            },
            cp::Exclusion {
                task_id: "task:ex-1".into(),
                basis: cp::OUTCOME_BLIND_EXCLUSION_BASES[3].into(),
            },
        ],
        exclusion_publication_ref: "publication:exclusions-pre-outcomes".into(),
        split_assignment_ref: "split:assignment-registered".into(),
        split_predates_access: true,
        quotas: vec![
            cp::Quota {
                stratum: "low".into(),
                required: 20,
            },
            cp::Quota {
                stratum: "high".into(),
                required: 10,
            },
        ],
        seed: "seed:committed-selection".into(),
        arms: vec![
            arm("arm:a", cp::ARM_STRATEGIES[0]),
            arm("arm:b", cp::ARM_STRATEGIES[1]),
        ],
        required_roles: roles(),
        lanes: vec!["lane:0".into(), "lane:1".into()],
        lane_spec_ref: "lane-spec:identical".into(),
        scheduling: scheduling(),
        reattempt: reattempt(),
        warmth: cp::Warmth::ColdStart,
        drift_observed: false,
        support_route: true,
        support_cache: true,
        support_seed: true,
        support_fingerprint: true,
        search_budget: 0,
        task_conditioned: false,
        artificial_pressure: false,
        phases: cp::PHASES
            .iter()
            .map(|p| (p.to_string(), format!("phase:{p}")))
            .collect(),
        family_id: "family:confirmatory-primaries".into(),
        primaries: cp::PRIMARIES.iter().map(|s| s.to_string()).collect(),
        multiplicity_id: "holm:alpha-0.05".into(),
        stops: vec![
            "stop:integrity".into(),
            "stop:drift".into(),
            "stop:ceiling".into(),
        ],
        pilot_labelled_as: None,
        amendment_subjects_bound: None,
    }
}

fn materialized() -> cp::MaterializedProtocol {
    cp::materialize_protocol(&golden()).expect("frozen decl re-materializes")
}

// ── the committed packet echoes (verbatim members — never re-picked)

fn echo(
    version: &str,
    disposition: &str,
    admission: &str,
    packet_id: &str,
    legs: &[(&str, &str)],
    details: &[(&str, &str)],
) -> PacketEcho {
    PacketEcho {
        version: version.into(),
        disposition: disposition.into(),
        admission: admission.into(),
        packet_id: packet_id.into(),
        legs: legs
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect(),
        details: details
            .iter()
            .map(|(n, d)| (n.to_string(), d.to_string()))
            .collect(),
    }
}

fn preflight_first_echo() -> PacketEcho {
    echo(
        "hh-backend-preflight/1",
        "inconclusive",
        "not_s1_s4_acceptance",
        "5de681dae985678777a5ebfcbe036c35a83a8d7654899776171305bba88e2e27",
        &[
            ("tb2_lifecycle", "verified"),
            ("backend_candidate", "inconclusive"),
            ("mediation", "verified"),
            ("observer_probe", "inconclusive"),
            ("acquisition_network", "verified"),
            ("worksheet", "verified"),
        ],
        &[
            ("backend_candidate", "backend_unavailable:podman"),
            ("observer_probe", "probe_absent"),
        ],
    )
}

fn preflight_reprobe_echo() -> PacketEcho {
    echo(
        "hh-backend-preflight/1",
        "inconclusive",
        "not_s1_s4_acceptance",
        "fe429e6997d0778c7dcea30d5f3c88a47b22e047ea3eb60a7534c3f429f40a0b",
        &[
            ("tb2_lifecycle", "verified"),
            ("backend_candidate", "verified"),
            ("mediation", "verified"),
            ("observer_probe", "inconclusive"),
            ("acquisition_network", "verified"),
            ("worksheet", "verified"),
        ],
        &[("observer_probe", "probe_absent")],
    )
}

fn useful_echo() -> PacketEcho {
    echo(
        "hh-s1-useful/1",
        "failed",
        "bounded_s1_execution",
        "08817205fefe071723656e80497954b6925f68203b94996000b9fc2f7f00d26d",
        &[
            ("subject_a", "inconclusive"),
            ("subject_b", "inconclusive"),
            ("oracle_chain", "inconclusive"),
            ("controls", "verified"),
            ("observer_probe", "verified"),
            ("evidence_join", "refused"),
            ("worksheet", "verified"),
        ],
        &[
            ("subject_a", "dispatch_outcome:unknown:executor_error"),
            ("subject_b", "dispatch_outcome:unknown:executor_error"),
            ("oracle_chain", "oracle_not_attempted:subjects"),
            ("evidence_join", "join:submission_digest"),
        ],
    )
}

fn ops_first_echo() -> PacketEcho {
    echo(
        "hh-ops-live/1",
        "failed",
        "bounded_s4_execution",
        "aba2ac7d4c3c362ce824105ffe32982faacb256003d3ec018ae800a69806c8bf",
        &[
            ("o1", "verified"),
            ("o2", "verified"),
            ("o3", "refused"),
            ("o4", "verified"),
            ("o5", "verified"),
            ("o6", "refused"),
            ("o7", "verified"),
            ("o8", "refused"),
            ("stage_report", "refused"),
            ("repairs", "refused"),
            ("worksheet", "verified"),
        ],
        &[
            ("o3", "o3r0:failed:unreached_fault"),
            ("o6", "o6r0:failed:same_path_unproven"),
            ("o8", "o8r0:defect_detected:2"),
            ("stage_report", "acceptance_blocked"),
            ("repairs", "probe_verb_enoent"),
        ],
    )
}

fn ops_second_echo() -> PacketEcho {
    echo(
        "hh-ops-live/1",
        "executed",
        "bounded_s4_execution",
        "26c9d5afe97c135a36bb544301ccd1d58343db3366a62c98696ba2680e5d395b",
        &[
            ("o1", "verified"),
            ("o2", "verified"),
            ("o3", "verified"),
            ("o4", "verified"),
            ("o5", "verified"),
            ("o6", "verified"),
            ("o7", "verified"),
            ("o8", "verified"),
            ("stage_report", "verified"),
            ("repairs", "verified"),
            ("worksheet", "verified"),
        ],
        &[],
    )
}

fn activation_echo() -> PacketEcho {
    echo(
        "hh-activation/1",
        "executed",
        "bounded_activation_execution",
        "f442785c61ae2c1f5892120be44f050ae1b561888505cf4b247bbe802e02bcb5",
        &[
            ("freeze", "verified"),
            ("subject_arm_a", "verified"),
            ("subject_arm_b", "verified"),
            ("activation_fidelity", "verified"),
            ("costs", "verified"),
            ("worksheet", "verified"),
            ("pilot_withheld", "verified"),
        ],
        &[],
    )
}

fn pilot_echo() -> PacketEcho {
    echo(
        "hh-pilot/1",
        "executed",
        "bounded_pilot_execution",
        "885cb0b30016aec4ae49d84658cf50d8a3c0de69874302537c2571855df2f119",
        &[
            ("freeze", "verified"),
            ("subject_a_r1", "verified"),
            ("subject_a_r2", "verified"),
            ("subject_a_r3", "verified"),
            ("subject_a_r4", "verified"),
            ("subject_a_r5", "verified"),
            ("subject_b_r1", "verified"),
            ("subject_b_r2", "verified"),
            ("subject_b_r3", "verified"),
            ("subject_b_r4", "verified"),
            ("subject_b_r5", "verified"),
            ("fidelity", "verified"),
            ("variance_tail", "verified"),
            ("missingness", "verified"),
            ("feasibility", "verified"),
            ("costs", "verified"),
            ("worksheet", "verified"),
        ],
        &[],
    )
}

fn parity_echo() -> PacketEcho {
    echo(
        "hh-parity-n0/1",
        "withheld",
        "n0_withheld",
        "4ca202c9e3b56fd15bb99dbbc79d1b47db592ce56c7de322205d1da29f9a0802",
        &[
            ("gate_readout", "verified"),
            ("pins_frozen", "verified"),
            ("admission_vocabulary", "verified"),
            ("quote_worksheet", "verified"),
            ("tranche_absent", "verified"),
            ("dependent_blocked", "verified"),
        ],
        &[],
    )
}

fn confirm_echo() -> PacketEcho {
    echo(
        "hh-confirm-withheld/1",
        "withheld",
        "quote_only_withheld",
        "0349c9329e567a2cf26ecc92cee66b8336623d14ee57f2e721d281d7ade72354",
        &[
            ("gate_readout", "verified"),
            ("manifest_frozen", "verified"),
            ("admission_vocabulary", "verified"),
            ("quote_worksheet", "verified"),
            ("tranche_absent", "verified"),
            ("parity_blocked", "verified"),
        ],
        &[],
    )
}

// ── per-stage facts (committed members verbatim) ───────────────────

const ACTIVATION_FREEZE: &str = "0be644f7f7a2b7ac626d126dc67034d522f9f4c8a7dd861dbad83a5b94274666";
const PILOT_FREEZE: &str = "ba0ca403debb91c21f2692edaab3351c327dea254379b86167dec02270e4b639";

fn freeze_facts() -> FreezeFacts {
    let p = materialized();
    FreezeFacts {
        check_id: p.check_id(),
        protocol_id: p.protocol_id.clone(),
        selected_tasks: p.selection.len() as u64,
        schedule_slots: p.schedule.len() as u64,
        first_slots: p.first_slots().len() as u64,
        phase_identities: p.phases.len() as u64,
        confirmation_phase: p.phases.contains_key("confirmation"),
        // The committed confirm packet's `manifest_frozen` fact — the
        // byte-equality target (the same recompute produced it).
        committed_protocol_id: p.protocol_id.clone(),
        activation_freeze_id: ACTIVATION_FREEZE.into(),
        pilot_inherits_freeze_id: ACTIVATION_FREEZE.into(),
        pilot_freeze_id: PILOT_FREEZE.into(),
    }
}

fn preflight_facts() -> PreflightStageFacts {
    PreflightStageFacts {
        first: preflight_first_echo(),
        reprobe: preflight_reprobe_echo(),
    }
}

fn useful_facts() -> UsefulFacts {
    UsefulFacts {
        echo: useful_echo(),
        acceptance: "withheld{no_independent_correct_artifact}".into(),
        carried: vec![
            "probe_verb_enoent".into(),
            "exec_backend_hello_clobber".into(),
        ],
        s1_unmet: true,
    }
}

fn ops_facts() -> OpsFacts {
    OpsFacts {
        first: ops_first_echo(),
        second: ops_second_echo(),
        discharged: vec![
            "probe_verb_enoent".into(),
            "exec_backend_hello_clobber".into(),
        ],
        acceptance_blocked: false,
        attempts: 24,
        seed: 21060,
    }
}

fn subject(pre: &str, compaction: &str, changed: bool) -> SubjectEcho {
    SubjectEcho {
        pre_dispatch: pre.into(),
        compaction_status: compaction.into(),
        request_changed: changed,
    }
}

fn activation_facts() -> ActivationFacts {
    ActivationFacts {
        echo: activation_echo(),
        reported: FidelityFold {
            opportunities: 2,
            starts: 2,
            completions: 2,
            changed_requests: 2,
            triggered_first: 2,
            all_first: 2,
        },
        subjects: vec![
            subject("observed:applied", "applied", true),
            subject("observed:applied", "applied", true),
        ],
        pilot_member: "withheld{reading_2_pending}".into(),
        attempts: 2,
        exposure_micros: 0,
    }
}

fn pilot_subject(arm: &str, wall_ms: u64, tokens_freed: u64) -> PilotSubjectEcho {
    PilotSubjectEcho {
        arm: arm.into(),
        fidelity: subject("observed:applied", "applied", true),
        outcome_class: "completed".into(),
        wall_ms: Some(wall_ms),
        tokens_freed,
        calls: 2,
        input_tokens: 600,
        output_bytes: 31,
        setup_retries: 0,
    }
}

fn pilot_facts() -> PilotFacts {
    PilotFacts {
        echo: pilot_echo(),
        fidelity: FidelityFold {
            opportunities: 10,
            starts: 10,
            completions: 10,
            changed_requests: 10,
            triggered_first: 10,
            all_first: 10,
        },
        arm_a: ArmTailEcho {
            n: 5,
            wall_p50_ms: 1279,
            wall_p95_ms: 1825,
            wall_max_ms: 1825,
            tokens_min: 442,
            tokens_max: 442,
        },
        arm_b: ArmTailEcho {
            n: 5,
            wall_p50_ms: 1213,
            wall_p95_ms: 1763,
            wall_max_ms: 1763,
            tokens_min: 592,
            tokens_max: 592,
        },
        censored_wall: 0,
        sign_pos: 10,
        sign_zero: 0,
        missingness: MissingnessEcho {
            planned: 10,
            completed: 10,
            failed: 0,
            unresolved: 0,
            lower_ppm: 1_000_000,
            upper_ppm: 1_000_000,
            unplanned_attempted: 0,
        },
        feasibility: FeasibilityEcho {
            subjects_run: 10,
            cells_attempted: 10,
            cells_provisioned: 10,
            dispatches_applied: 20,
            seals: 10,
            compactions_applied: 10,
            max_setup_retries: 0,
            max_calls: 2,
            max_input_tokens: 600,
            max_output_bytes: 31,
            max_wall_ms: 1825,
            max_concurrent: 1,
        },
        attempts: 10,
        exposure_micros: 0,
        pilot_state: "ready_for_gate".into(),
        contrast_claimed: false,
        tuning_performed: false,
        subjects: vec![
            pilot_subject("arm:a", 1825, 442),
            pilot_subject("arm:a", 1279, 442),
            pilot_subject("arm:a", 1133, 442),
            pilot_subject("arm:a", 1205, 442),
            pilot_subject("arm:a", 1382, 442),
            pilot_subject("arm:b", 1089, 592),
            pilot_subject("arm:b", 1213, 592),
            pilot_subject("arm:b", 1226, 592),
            pilot_subject("arm:b", 1204, 592),
            pilot_subject("arm:b", 1763, 592),
        ],
    }
}

/// The executed cohort's frozen `protocol_id` — the real
/// manifest's id, byte-equal across the parity tranche and both
/// confirmation packets (verbatim supplied member).
const COHORT_PROTOCOL_ID: &str = "9edc5327f89e2d66f8003bedc2530bc3e7d37326aad82cdfcab70465eda60a5f";

fn parity_facts() -> ParityStageFacts {
    ParityStageFacts {
        echo: parity_echo(),
        tranche_version: "hh-parity-tranche-report/1".into(),
        tranche_verdict: "verified".into(),
        n_pairs: 30,
        agree: 30,
        discordant: 0,
        protocol_id: COHORT_PROTOCOL_ID.into(),
        n_declared: 30,
        s3_unmet: false,
        confirmation_blocked: false,
        gate: "GATE-G13".into(),
    }
}

fn confirm_facts() -> ConfirmStageFacts {
    ConfirmStageFacts {
        echo: confirm_echo(),
        confirm_version: "hh-s4-confirmation/1".into(),
        confirm_verdict: "partial".into(),
        planned: 300,
        executed: 160,
        inconclusive: 140,
        failed: 0,
        first_executed: 32,
        first_total: 60,
        // The named supplement epochs in commit order — each covers
        // exactly the prior residue; the repairs epoch closed it.
        epochs: vec![
            ConfirmEpochFacts {
                version: "hh-s4-egress-supplement/1".into(),
                verdict: "partial".into(),
                planned: 140,
                executed: 90,
                inconclusive: 50,
                first_executed: 18,
                first_total: 28,
            },
            ConfirmEpochFacts {
                version: "hh-s4-completion/1".into(),
                verdict: "partial".into(),
                planned: 50,
                executed: 20,
                inconclusive: 30,
                first_executed: 4,
                first_total: 10,
            },
            ConfirmEpochFacts {
                version: "hh-s4-30slot-repairs/1".into(),
                verdict: "executed".into(),
                planned: 30,
                executed: 30,
                inconclusive: 0,
                first_executed: 6,
                first_total: 6,
            },
        ],
        parity_protocol_id: COHORT_PROTOCOL_ID.into(),
        protocol_id: COHORT_PROTOCOL_ID.into(),
        subjects_first: 60,
        tranche_absent: false,
        parity_absent: false,
        confirmation_blocked: false,
        s4_unmet: false,
        gate: "GATE-G14".into(),
    }
}

// ── the cohort's committed records — echoed verbatim from the
// committed report files (records-in, never re-picked) ────────────

/// The four cohort epoch packets, in commit order — later epochs
/// re-execute exactly the prior residue's slots; a slot's last record
/// carries its final receipt.
const COHORT_PACKETS: [&str; 4] = [
    "2026-11-05-s4-confirmation.json",
    "2026-11-05-s4-egress-cohort.json",
    "2026-11-06-s4-completion.json",
    "2026-11-06-s4-30slot-repairs.json",
];

fn load_packet(name: &str) -> Json {
    let text = std::fs::read_to_string(repo_root().join("docs/build/reports").join(name))
        .expect("committed cohort packet readable");
    // `parse_provider` — committed records carry float lexemes
    // (`cost_usd: 0.0`); the inbound read preserves them verbatim as
    // strings, never truncated or dropped (the canonical `parse`
    // refuses them — outbound records never emit floats).
    json::parse_provider(&text).expect("committed cohort packet parses")
}

fn jget<'a>(j: &'a Json, k: &str) -> &'a Json {
    j.get(k).unwrap_or_else(|| panic!("member {k} absent"))
}

fn jstr(j: &Json, k: &str) -> String {
    match jget(j, k) {
        Json::Str(s) => s.clone(),
        other => panic!("member {k} not a string: {other:?}"),
    }
}

fn jint(j: &Json, k: &str) -> u64 {
    match jget(j, k) {
        Json::Int(n) => *n as u64,
        other => panic!("member {k} not an int: {other:?}"),
    }
}

fn jarr<'a>(j: &'a Json, k: &str) -> &'a [Json] {
    match jget(j, k) {
        Json::Arr(a) => a,
        other => panic!("member {k} not an array: {other:?}"),
    }
}

/// The cohort's 300 slots after folding all four epochs — a slot's
/// last epoch record wins; each carries `receipt.verdict` verbatim.
fn cohort_slots() -> Vec<Json> {
    let mut by_ref: BTreeMap<String, Json> = BTreeMap::new();
    for name in COHORT_PACKETS {
        let p = load_packet(name);
        for s in jarr(&p, "slots") {
            by_ref.insert(jstr(s, "slot_ref"), s.clone());
        }
    }
    let slots: Vec<Json> = by_ref.into_values().collect();
    assert_eq!(slots.len(), 300, "the cohort covers every planned slot");
    slots
}

/// task → declared stratum, verbatim from the frozen manifest's
/// `manifest.selection` members.
fn task_strata() -> BTreeMap<String, String> {
    let p = load_packet(COHORT_PACKETS[0]);
    jarr(jget(&p, "manifest"), "selection")
        .iter()
        .map(|s| (jstr(s, "task_id"), jstr(s, "stratum")))
        .collect()
}

/// The committed `receipt.verdict` fold — `resolved`/`unresolved`
/// verbatim, or `None` for a slot still without a receipt.
fn receipt_verdicts() -> Vec<(Json, Option<String>)> {
    cohort_slots()
        .into_iter()
        .map(|s| {
            let v = s
                .get("receipt")
                .and_then(|r| r.get("verdict"))
                .and_then(|v| match v {
                    Json::Str(s) => Some(s.clone()),
                    _ => None,
                });
            (s, v)
        })
        .collect()
}

/// The committed fold as `EfficacySlot`s — `None` when any slot
/// lacks a receipt verdict (the incomplete-cohort fold).
fn efficacy_slots() -> Option<Vec<EfficacySlot>> {
    let strata = task_strata();
    let mut out = Vec::new();
    for (s, v) in receipt_verdicts() {
        let task = jstr(&s, "task_id");
        out.push(EfficacySlot {
            stratum: strata.get(&task).cloned().unwrap_or_default(),
            task_id: task,
            arm_id: jstr(&s, "arm_id"),
            receipt_verdict: v?,
        });
    }
    Some(out)
}

/// The committed fold as the R5.15 population contract's planned
/// slots — one bound subject per slot (the executed attempt), the
/// receipt's binary verdict as `success`.
fn planned_population() -> PlannedPopulation {
    let strata = task_strata();
    let mut slots = Vec::new();
    let mut pins = BTreeMap::new();
    let mut required = std::collections::BTreeSet::new();
    for (s, v) in receipt_verdicts() {
        let slot_ref = jstr(&s, "slot_ref");
        let task = jstr(&s, "task_id");
        let attempts = jarr(&s, "attempts");
        let subject_run = attempts
            .first()
            .and_then(|a| match a {
                Json::Str(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| format!("{slot_ref}-subject"));
        pins.insert(subject_run.clone(), 1u64);
        required.insert(slot_ref.clone());
        slots.push(PlannedSlot {
            plan_ref: slot_ref,
            task_id: task.clone(),
            split_label: "held_out".into(),
            stratum: strata.get(&task).cloned().unwrap_or_default(),
            arm_id: jstr(&s, "arm_id"),
            replicate_index: jint(&s, "round"),
            subjects: vec![SlotSubject {
                subject_run,
                attempt_no: 1,
                outcome: Some(OutcomeClass::Scored),
                success: v.map(|v| v == "resolved"),
                veto_tripped: Vec::new(),
                availability: Availability::Resolved,
                seq: 1,
                missing_required_checks: false,
                admissible: true,
            }],
        });
    }
    pop_assemble(PopulationInput {
        slots,
        required,
        pins,
    })
    .expect("the committed cohort assembles")
}

/// The registered finite-manifest declaration (REQ-D-107/109 §4.3,
/// verbatim): the canonical population, B=arm:h-podman-oracle
/// (clear_tool_results) minus A=arm:o-tb-oracle (evict_oldest), the
/// registered beta{1,1} prior, the frozen 95% level and 8,000 base
/// draws, declared seed 0.
fn efficacy_decl(stratum: &str) -> ContrastDecl {
    ContrastDecl {
        population: PopulationKind::Canonical,
        test_arm: "arm:h-podman-oracle".into(),
        control_arm: "arm:o-tb-oracle".into(),
        stratum: Some(stratum.into()),
        prior: (1, 1),
        prior_name: "beta{1,1}".into(),
        dependence: None,
        target: None,
        use_as: None,
        bounds: BoundsDecl::Unit,
        level_ppm: 950_000,
        seed: 0,
    }
}

/// One stratum's result through the landed estimator — the computed
/// members verbatim, or the typed refusal's display spelling.
fn efficacy_stratum(
    pop: &PlannedPopulation,
    stratum: &str,
) -> (
    EfficacyStratum,
    Option<hharness_eval::finite_manifest::FiniteManifestContrast>,
) {
    match finite_manifest_contrast(pop, &efficacy_decl(stratum)) {
        Ok(out) => (
            EfficacyStratum {
                stratum: stratum.into(),
                computed: true,
                cells: out
                    .cells
                    .iter()
                    .map(|c| hharness_env::study::EfficacyCell {
                        task_id: c.task_id.clone(),
                        test_c: c.test_c,
                        test_n: c.test_n,
                        control_c: c.control_c,
                        control_n: c.control_n,
                    })
                    .collect(),
                observed_contrast_ppm: out.observed_contrast_ppm,
                posterior_mean_ppm: out.posterior_mean_ppm,
                interval_lo_ppm: out.interval.lo,
                interval_hi_ppm: out.interval.hi,
                draws_used: out.draws_used,
                refusal: String::new(),
            },
            Some(out),
        ),
        Err(e) => (
            EfficacyStratum {
                stratum: stratum.into(),
                computed: false,
                cells: Vec::new(),
                observed_contrast_ppm: 0,
                posterior_mean_ppm: 0,
                interval_lo_ppm: 0,
                interval_hi_ppm: 0,
                draws_used: 0,
                refusal: e.to_string(),
            },
            None,
        ),
    }
}

/// The efficacy stage's supplied facts — the committed fold's slots,
/// the registered declaration and the per-declared-stratum results
/// the landed estimator produced.
fn efficacy_facts() -> (
    EfficacyFacts,
    PlannedPopulation,
    BTreeMap<String, Result<hharness_eval::finite_manifest::FiniteManifestContrast, ContrastError>>,
) {
    let pop = planned_population();
    let strata: Vec<String> = {
        let mut v: Vec<String> = pop.slots.iter().map(|s| s.stratum.clone()).collect();
        v.sort();
        v.dedup();
        v
    };
    let mut results = BTreeMap::new();
    let mut facts_strata = Vec::new();
    for st in &strata {
        let (es, out) = efficacy_stratum(&pop, st);
        results.insert(
            st.clone(),
            out.ok_or_else(|| {
                let d = efficacy_decl(st);
                finite_manifest_contrast(&pop, &d).unwrap_err()
            }),
        );
        facts_strata.push(es);
    }
    let mut computed_csv: Vec<&str> = Vec::new();
    let mut refused_csv: Vec<String> = Vec::new();
    for st in &facts_strata {
        if st.computed {
            computed_csv.push(st.stratum.as_str());
        } else {
            refused_csv.push(format!("{}:{}", st.stratum, st.refusal));
        }
    }
    computed_csv.sort();
    refused_csv.sort();
    let status = if computed_csv.len() == facts_strata.len() {
        format!("computed{{strata:{}}}", computed_csv.join(","))
    } else if computed_csv.is_empty() {
        format!("unavailable{{{}}}", refused_csv.join(";"))
    } else {
        format!(
            "partial{{computed:{};refused:{}}}",
            computed_csv.join(","),
            refused_csv.join(";")
        )
    };
    (
        EfficacyFacts {
            population: "canonical".into(),
            test_arm: "arm:h-podman-oracle".into(),
            control_arm: "arm:o-tb-oracle".into(),
            prior: "beta{1,1}".into(),
            direction: "test_minus_control".into(),
            level_ppm: 950_000,
            draws_requested: 8_000,
            seed: 0,
            protocol_id: COHORT_PROTOCOL_ID.into(),
            slots: efficacy_slots().expect("the completed cohort's receipts all present"),
            strata: facts_strata,
            status,
        },
        pop,
        results,
    )
}

fn family_facts(efficacy_status: &str) -> FamilyFacts {
    FamilyFacts {
        efficacy: efficacy_status.into(),
        required_efficacy: efficacy_status.into(),
        task_mixture: "descriptive{pilot_recomputed}".into(),
        sharp_null: "unavailable{not_computed}".into(),
        parity: "executed{verified_30_30}".into(),
        mechanism: "executed{activation,pilot}".into(),
        engineering: "executed{ops_live;backend_verified}".into(),
        task_truth: "failed{s1_useful}".into(),
        operations: "executed{ops_live}".into(),
        adoption: "unavailable{not_started}".into(),
        claim_present: false,
    }
}

fn cost_facts() -> CostFacts {
    CostFacts {
        per_packet: [
            "hh-backend-preflight/1#first",
            "hh-backend-preflight/1#reprobe",
            "hh-s1-useful/1",
            "hh-ops-live/1#first",
            "hh-ops-live/1#second",
            "hh-activation/1",
            "hh-pilot/1",
            "hh-parity-n0/1",
            "hh-confirm-withheld/1",
            "hh-parity-tranche-report/1",
            "hh-s4-confirmation/1",
            "hh-s4-egress-supplement/1",
            "hh-s4-completion/1",
            "hh-s4-30slot-repairs/1",
        ]
        .iter()
        .map(|p| (p.to_string(), 0))
        .collect(),
        subjects_first: 12,
        gated_subjects: 90,
    }
}

fn leg(name: &str, verdict: LegVerdict, facts: &[(&str, String)]) -> Leg {
    Leg {
        name: name.into(),
        verdict,
        facts: facts
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    }
}

fn s(v: impl ToString) -> String {
    v.to_string()
}

/// The eleven legs driven on the supplied committed members — the
/// report's honest recompute.
fn run_packet() -> (
    Vec<Leg>,
    PlannedPopulation,
    BTreeMap<String, Result<hharness_eval::finite_manifest::FiniteManifestContrast, ContrastError>>,
) {
    let fr = freeze_facts();
    let pf = preflight_facts();
    let u = useful_facts();
    let o = ops_facts();
    let a = activation_facts();
    let p = pilot_facts();
    let pa = parity_facts();
    let c = confirm_facts();
    let (ef, pop, contrast_results) = efficacy_facts();
    let fam = family_facts(&ef.status);
    let co = cost_facts();
    let legs = vec![
        leg(
            "freeze_identities",
            evaluate_freeze(&fr),
            &[
                ("check_id", s(fr.check_id)),
                ("protocol_id", s(&fr.protocol_id)),
                ("committed_protocol_id", s(&fr.committed_protocol_id)),
                ("activation_freeze_id", s(&fr.activation_freeze_id)),
                ("pilot_freeze_id", s(&fr.pilot_freeze_id)),
            ],
        ),
        leg(
            "pre_flight",
            evaluate_preflight(&pf),
            &[
                ("first_disposition", s(&pf.first.disposition)),
                ("reprobe_disposition", s(&pf.reprobe.disposition)),
                (
                    "backend_candidate",
                    s(pf.reprobe.verdict_of("backend_candidate").unwrap_or("")),
                ),
                ("observer_probe", s("inconclusive{probe_absent}")),
                ("status", s("backend_verified;probe_carried")),
            ],
        ),
        leg(
            "useful_s1",
            evaluate_useful(&u),
            &[
                ("disposition", s(&u.echo.disposition)),
                ("acceptance", s(&u.acceptance)),
                ("carried", s(u.carried.join(","))),
                ("s1_unmet", s(u.s1_unmet)),
                ("status", s("failed")),
            ],
        ),
        leg(
            "ops_live",
            evaluate_ops(&o),
            &[
                ("first_disposition", s(&o.first.disposition)),
                ("second_disposition", s(&o.second.disposition)),
                ("discharged", s(o.discharged.join(","))),
                ("acceptance_blocked", s(o.acceptance_blocked)),
                ("attempts", s(o.attempts)),
                ("status", s("executed{second_packet}")),
            ],
        ),
        leg(
            "activation",
            evaluate_activation(&a),
            &[
                ("disposition", s(&a.echo.disposition)),
                ("opportunities", s(a.reported.opportunities)),
                ("completions", s(a.reported.completions)),
                ("triggered_first", s(a.reported.triggered_first)),
                ("all_first", s(a.reported.all_first)),
                ("pilot_member", s(&a.pilot_member)),
                ("status", s("executed")),
            ],
        ),
        leg(
            "pilot",
            evaluate_pilot(&p),
            &[
                ("disposition", s(&p.echo.disposition)),
                ("all_first", s(p.fidelity.all_first)),
                ("a_wall_p50_ms", s(p.arm_a.wall_p50_ms)),
                ("b_wall_p50_ms", s(p.arm_b.wall_p50_ms)),
                (
                    "missingness_bound",
                    s(format!(
                        "[{},{}]",
                        p.missingness.lower_ppm, p.missingness.upper_ppm
                    )),
                ),
                ("max_wall_ms", s(p.feasibility.max_wall_ms)),
                ("exposure_micros", s(p.exposure_micros)),
                ("status", s("executed")),
            ],
        ),
        leg(
            "parity",
            evaluate_parity_stage(&pa),
            &[
                ("withheld_record", s(&pa.echo.disposition)),
                ("disposition", s(&pa.tranche_verdict)),
                ("n_declared", s(pa.n_declared)),
                ("pairs", s(format!("{}/{}", pa.agree, pa.n_pairs))),
                ("discordant", s(pa.discordant)),
                ("s3_unmet", s(pa.s3_unmet)),
                ("confirmation_blocked", s(pa.confirmation_blocked)),
                ("status", s("verified{30_30}")),
            ],
        ),
        leg(
            "confirmation",
            evaluate_confirm_stage(&c),
            &[
                ("withheld_record", s(&c.echo.disposition)),
                ("disposition", s(&c.confirm_verdict)),
                (
                    "slots",
                    s(format!(
                        "{}/{}",
                        c.executed + c.epochs.iter().map(|e| e.executed).sum::<u64>(),
                        c.planned
                    )),
                ),
                (
                    "epochs",
                    s(c.epochs
                        .iter()
                        .map(|e| e.version.as_str())
                        .collect::<Vec<_>>()
                        .join(",")),
                ),
                (
                    "inconclusive",
                    s(c.epochs
                        .last()
                        .map(|e| e.inconclusive)
                        .unwrap_or(c.inconclusive)),
                ),
                ("subjects_first", s(c.subjects_first)),
                ("tranche_absent", s(c.tranche_absent)),
                ("parity_absent", s(c.parity_absent)),
                ("s4_unmet", s(c.s4_unmet)),
                ("status", s("executed{300_300}")),
            ],
        ),
        leg(
            "efficacy",
            evaluate_efficacy(&ef),
            &[
                ("population", s(&ef.population)),
                ("direction", s(&ef.direction)),
                ("test_arm", s(&ef.test_arm)),
                ("control_arm", s(&ef.control_arm)),
                ("prior", s(&ef.prior)),
                ("level_ppm", s(ef.level_ppm)),
                ("draws_requested", s(ef.draws_requested)),
                ("seed", s(ef.seed)),
                ("protocol_id", s(&ef.protocol_id)),
                ("cohort_slots", s(ef.slots.len())),
                (
                    "strata",
                    s(ef.strata
                        .iter()
                        .map(|st| {
                            if st.computed {
                                format!("{}:computed", st.stratum)
                            } else {
                                format!("{}:{}", st.stratum, st.refusal)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(",")),
                ),
                ("status", s(&ef.status)),
            ],
        ),
        leg(
            "families",
            evaluate_families(&fam),
            &[
                ("efficacy", s(&fam.efficacy)),
                ("task_mixture", s(&fam.task_mixture)),
                ("sharp_null", s(&fam.sharp_null)),
                ("parity", s(&fam.parity)),
                ("mechanism", s(&fam.mechanism)),
                ("engineering", s(&fam.engineering)),
                ("task_truth", s(&fam.task_truth)),
                ("operations", s(&fam.operations)),
                ("adoption", s(&fam.adoption)),
            ],
        ),
        leg(
            "costs",
            evaluate_costs(&co),
            &[
                ("packets", s(co.per_packet.len())),
                ("total_micros", s(0)),
                ("subjects_first", s(co.subjects_first)),
                ("gated_subjects", s(co.gated_subjects)),
            ],
        ),
    ];
    (legs, pop, contrast_results)
}

/// The report's honest fold — the caller binds it; the fields are
/// never hard-coded.
fn study_fold() -> StudyFold {
    StudyFold {
        subjects_first: 12,
        exposure_micros: 0,
        s1_unmet: true,
        s3_unmet: false,
        s4_unmet: false,
        study: "unmet{s1_failed}".into(),
    }
}

// ── the battery ──────────────────────────────────────────────────

#[test]
fn p49_freeze_and_preflight() {
    // The frozen manifest re-materializes deterministically and the
    // recompute byte-equals the committed confirm fact.
    let p = materialized();
    assert!(p.check_id());
    assert_eq!(p.selection.len(), cp::TASKS as usize);
    assert_eq!(p.schedule.len(), (cp::TASKS * 2 * cp::ROUNDS) as usize);
    assert_eq!(p.first_slots().len(), (cp::TASKS * 2) as usize);
    assert!(p.phases.contains_key("confirmation"));
    assert_eq!(evaluate_freeze(&freeze_facts()), LegVerdict::Verified);
    // A drifted identity, a failed check_id, an id byte-mismatch or
    // a freeze collision refuses — never a re-pick.
    for f in [
        FreezeFacts {
            check_id: false,
            ..freeze_facts()
        },
        FreezeFacts {
            selected_tasks: 25,
            ..freeze_facts()
        },
        FreezeFacts {
            committed_protocol_id: "0".repeat(64),
            ..freeze_facts()
        },
        FreezeFacts {
            pilot_inherits_freeze_id: PILOT_FREEZE.into(),
            ..freeze_facts()
        },
        FreezeFacts {
            pilot_freeze_id: ACTIVATION_FREEZE.into(),
            ..freeze_facts()
        },
    ] {
        assert!(matches!(evaluate_freeze(&f), LegVerdict::Refused(_)));
    }
    let mut f = freeze_facts();
    f.protocol_id.clear();
    assert!(matches!(evaluate_freeze(&f), LegVerdict::Inconclusive(_)));
    // A different committed seed mints a different protocol_id — the
    // recompute is not a re-pick.
    let mut d2 = golden();
    d2.seed = "seed:other".into();
    assert_ne!(
        materialized().protocol_id,
        cp::materialize_protocol(&d2).unwrap().protocol_id
    );
    // Pre-flight: two committed records refold `inconclusive`; the
    // reprobe's backend verified while the probe stayed absent.
    assert_eq!(evaluate_preflight(&preflight_facts()), LegVerdict::Verified);
    let mut bad = preflight_facts();
    bad.reprobe = echo(
        "hh-backend-preflight/1",
        "feasible",
        "not_s1_s4_acceptance",
        &"f".repeat(64),
        &[
            ("tb2_lifecycle", "verified"),
            ("backend_candidate", "verified"),
            ("mediation", "verified"),
            ("observer_probe", "verified"),
            ("acquisition_network", "verified"),
            ("worksheet", "verified"),
        ],
        &[],
    );
    // An all-verified refold would be `feasible` — the recorded
    // `inconclusive` stays honest; a fabricated feasible is refused
    // here because the observer_probe member no longer carries its
    // absent detail.
    assert!(matches!(evaluate_preflight(&bad), LegVerdict::Refused(_)));
    // A drifted first-record detail spelling refuses — the committed
    // member is byte-echoed, never approximated.
    let mut drift = preflight_facts();
    drift.first.details[0].1 = "backend_unavailable:kubernetes".into();
    assert!(matches!(evaluate_preflight(&drift), LegVerdict::Refused(_)));
}

#[test]
fn p49_useful_and_ops() {
    assert_eq!(evaluate_useful(&useful_facts()), LegVerdict::Verified);
    // A marked acceptance or met S1 refuses — never relabelled.
    for f in [
        UsefulFacts {
            acceptance: "marked".into(),
            ..useful_facts()
        },
        UsefulFacts {
            s1_unmet: false,
            ..useful_facts()
        },
        UsefulFacts {
            carried: vec!["probe_verb_enoent".into()],
            ..useful_facts()
        },
    ] {
        assert!(matches!(evaluate_useful(&f), LegVerdict::Refused(_)));
    }
    assert_eq!(evaluate_ops(&ops_facts()), LegVerdict::Verified);
    for f in [
        OpsFacts {
            discharged: vec!["probe_verb_enoent".into()],
            ..ops_facts()
        },
        OpsFacts {
            acceptance_blocked: true,
            ..ops_facts()
        },
        OpsFacts {
            attempts: 23,
            ..ops_facts()
        },
    ] {
        assert!(matches!(evaluate_ops(&f), LegVerdict::Refused(_)));
    }
}

#[test]
fn p49_activation_pilot_recompute() {
    assert_eq!(
        evaluate_activation(&activation_facts()),
        LegVerdict::Verified
    );
    // The denominators recompute: a one-subject shortfall or an
    // unequal reported fold refuses.
    let mut short = activation_facts();
    short.subjects.pop();
    assert!(matches!(
        evaluate_activation(&short),
        LegVerdict::Refused(_)
    ));
    let mut drifted = activation_facts();
    drifted.reported.opportunities = 3;
    assert!(matches!(
        evaluate_activation(&drifted),
        LegVerdict::Refused(_)
    ));
    // The pilot recompute — denominators, tails, missingness,
    // envelope maxima each re-derived and asserted equal.
    assert_eq!(evaluate_pilot(&pilot_facts()), LegVerdict::Verified);
    // Direct recompute checks: nearest-rank P50 of arm A's sorted
    // walls [1133,1205,1279,1382,1825] is the 3rd (1279); P95 = max.
    let pf = pilot_facts();
    let a = recompute_arm_tail(pf.subjects.iter().filter(|s| s.arm == "arm:a"));
    assert_eq!(
        (a.n, a.wall_p50_ms, a.wall_p95_ms, a.wall_max_ms),
        (5, 1279, 1825, 1825)
    );
    let m = recompute_missingness(&pf.subjects, 10);
    assert_eq!((m.lower_ppm, m.upper_ppm), (1_000_000, 1_000_000));
    let (mr, mc, mi, mo, mw) = recompute_maxima(&pf.subjects);
    assert_eq!((mr, mc, mi, mo, mw), (0, 2, 600, 31, 1825));
    // An unequal recompute — a substituted wall sample, a dropped
    // subject, a claim — refuses, never re-averaged.
    for f in [
        PilotFacts {
            censored_wall: 1,
            ..pilot_facts()
        },
        PilotFacts {
            contrast_claimed: true,
            ..pilot_facts()
        },
        PilotFacts {
            pilot_state: "executed".into(),
            ..pilot_facts()
        },
    ] {
        assert!(matches!(evaluate_pilot(&f), LegVerdict::Refused(_)));
    }
    let mut drifted = pilot_facts();
    drifted.arm_a.wall_p50_ms = 1205;
    assert!(matches!(evaluate_pilot(&drifted), LegVerdict::Refused(_)));
    let mut short = pilot_facts();
    short.subjects.remove(9);
    assert!(matches!(evaluate_pilot(&short), LegVerdict::Refused(_)));
}

#[test]
fn p49_parity_confirm_executed() {
    // The withheld records still refold `withheld` (committed
    // history); the executed cohort's members re-verify — parity
    // `verified` at 30/30, confirmation `partial` at 250/300.
    assert_eq!(evaluate_parity_stage(&parity_facts()), LegVerdict::Verified);
    // A discordance, an inflated/deflated N, a non-verified verdict
    // spelling or a retained upstream block refuses — never
    // relabelled.
    for f in [
        ParityStageFacts {
            discordant: 1,
            ..parity_facts()
        },
        ParityStageFacts {
            tranche_verdict: "infeasible{margin}".into(),
            ..parity_facts()
        },
        ParityStageFacts {
            n_declared: 0,
            ..parity_facts()
        },
        ParityStageFacts {
            n_pairs: 29,
            agree: 29,
            ..parity_facts()
        },
        ParityStageFacts {
            s3_unmet: true,
            ..parity_facts()
        },
        ParityStageFacts {
            confirmation_blocked: true,
            ..parity_facts()
        },
        ParityStageFacts {
            gate: "GATE-G14".into(),
            ..parity_facts()
        },
    ] {
        assert!(
            matches!(evaluate_parity_stage(&f), LegVerdict::Refused(_)),
            "{f:?}"
        );
    }
    let mut f = parity_facts();
    f.protocol_id.clear();
    assert!(matches!(
        evaluate_parity_stage(&f),
        LegVerdict::Inconclusive(_)
    ));
    assert_eq!(
        evaluate_confirm_stage(&confirm_facts()),
        LegVerdict::Verified
    );
    // Dispositions that fail to re-add, an epoch wider/narrower than
    // the prior residue, a foreign epoch schema, a full/zero cohort
    // spelt `partial`, an epoch residue spelt `executed`, a
    // first-subject count that doesn't re-add, a drifted protocol
    // id, a dropped S4 bound or a retained upstream block refuses.
    let mut bad_epoch = confirm_facts();
    bad_epoch.epochs[0].planned = 141;
    let mut bad_exec = confirm_facts();
    bad_exec.epochs[1].executed = 19;
    let mut bad_schema = confirm_facts();
    bad_schema.epochs[1].version = "hh-other/1".into();
    let mut bad_verdict = confirm_facts();
    bad_verdict.epochs[2].verdict = "partial".into();
    for f in [
        ConfirmStageFacts {
            executed: 159,
            ..confirm_facts()
        },
        bad_epoch,
        bad_exec,
        bad_schema,
        bad_verdict,
        ConfirmStageFacts {
            executed: 300,
            inconclusive: 0,
            ..confirm_facts()
        },
        ConfirmStageFacts {
            subjects_first: 59,
            ..confirm_facts()
        },
        ConfirmStageFacts {
            parity_protocol_id: "0".repeat(64),
            ..confirm_facts()
        },
        ConfirmStageFacts {
            s4_unmet: true,
            ..confirm_facts()
        },
        ConfirmStageFacts {
            tranche_absent: true,
            ..confirm_facts()
        },
        ConfirmStageFacts {
            planned: 299,
            ..confirm_facts()
        },
    ] {
        assert!(
            matches!(evaluate_confirm_stage(&f), LegVerdict::Refused(_)),
            "{f:?}"
        );
    }
    let mut f = confirm_facts();
    f.protocol_id.clear();
    assert!(matches!(
        evaluate_confirm_stage(&f),
        LegVerdict::Inconclusive(_)
    ));
}

#[test]
fn p49_families_and_costs() {
    let fam = family_facts("computed{strata:easy,hard,medium}");
    assert_eq!(evaluate_families(&fam), LegVerdict::Verified);
    // Every required status is the honest spelling — a met/passed
    // unavailable family, a different executed spelling, a family
    // diverging from the efficacy leg's status or a claim refuses,
    // never relabelled.
    for f in [
        FamilyFacts {
            efficacy: "met{confirmation_executed}".into(),
            ..family_facts("computed{strata:easy,hard,medium}")
        },
        FamilyFacts {
            efficacy: "negative{no_effect}".into(),
            ..family_facts("computed{strata:easy,hard,medium}")
        },
        FamilyFacts {
            efficacy: "unavailable{partial}".into(),
            ..family_facts("computed{strata:easy,hard,medium}")
        },
        FamilyFacts {
            parity: "equivalent".into(),
            ..family_facts("computed{strata:easy,hard,medium}")
        },
        FamilyFacts {
            sharp_null: "unavailable{partial}".into(),
            ..family_facts("computed{strata:easy,hard,medium}")
        },
        FamilyFacts {
            adoption: "passed".into(),
            ..family_facts("computed{strata:easy,hard,medium}")
        },
        FamilyFacts {
            claim_present: true,
            ..family_facts("computed{strata:easy,hard,medium}")
        },
    ] {
        assert!(matches!(evaluate_families(&f), LegVerdict::Refused(_)));
    }
    // The required vocabulary is seven stable families plus the
    // state-dependent efficacy/sharp-null members.
    assert_eq!(REQUIRED_FAMILIES.len(), 7);
    assert_eq!(evaluate_costs(&cost_facts()), LegVerdict::Verified);
    let mut spend = cost_facts();
    spend.per_packet[0].1 = 1;
    assert!(matches!(evaluate_costs(&spend), LegVerdict::Refused(_)));
    let mut subj = cost_facts();
    subj.subjects_first = 11;
    assert!(matches!(evaluate_costs(&subj), LegVerdict::Refused(_)));
}

#[test]
fn p49_check_echo_refold() {
    // The shared refold reproduces each committed disposition.
    for (e, vocab, terminal, definitive) in [
        (
            preflight_first_echo(),
            &hharness_env::preflight::LEGS[..],
            "feasible",
            "infeasible",
        ),
        (
            preflight_reprobe_echo(),
            &hharness_env::preflight::LEGS[..],
            "feasible",
            "infeasible",
        ),
        (
            useful_echo(),
            &hharness_env::useful::LEGS[..],
            "executed",
            "failed",
        ),
        (
            ops_first_echo(),
            &hharness_env::opslive::LEGS[..],
            "executed",
            "failed",
        ),
        (
            ops_second_echo(),
            &hharness_env::opslive::LEGS[..],
            "executed",
            "failed",
        ),
        (
            activation_echo(),
            &hharness_env::activation::LEGS[..],
            "executed",
            "failed",
        ),
        (
            pilot_echo(),
            &hharness_env::pilot::LEGS[..],
            "executed",
            "failed",
        ),
        (
            parity_echo(),
            &hharness_env::parity::LEGS[..],
            "withheld",
            "failed",
        ),
        (
            confirm_echo(),
            &hharness_env::confirm::LEGS[..],
            "withheld",
            "failed",
        ),
    ] {
        assert_eq!(
            check_echo(&e, vocab, terminal, definitive),
            LegVerdict::Verified
        );
    }
    // A recorded disposition disagreeing with the refold refuses.
    let mut lie = ops_second_echo();
    lie.disposition = "withheld".into();
    assert!(matches!(
        check_echo(&lie, &hharness_env::opslive::LEGS, "executed", "failed"),
        LegVerdict::Refused(_)
    ));
    // An absent leg is inconclusive; a foreign leg refuses.
    let mut missing = confirm_echo();
    missing.legs.pop();
    assert!(matches!(
        check_echo(&missing, &hharness_env::confirm::LEGS, "withheld", "failed"),
        LegVerdict::Inconclusive(_)
    ));
    let mut foreign = confirm_echo();
    foreign.legs[0].0 = "not_a_leg".into();
    assert!(matches!(
        check_echo(&foreign, &hharness_env::confirm::LEGS, "withheld", "failed"),
        LegVerdict::Refused(_)
    ));
}

#[test]
fn p49_packet_reported_honest() {
    let (legs, _pop, results) = run_packet();
    for l in &legs {
        assert!(l.verdict.is_verified(), "{}: {:?}", l.name, l.verdict);
    }
    let report = assemble(legs, &study_fold()).unwrap();
    assert_eq!(report.version, VERSION);
    assert_eq!(report.subjects_first, 12);
    assert_eq!(report.exposure_micros, 0);
    // S3 met (parity verified 30/30) and S4 met (the cohort executed
    // 300/300); S1 stays unmet (the S1 packet is `failed`).
    assert!(report.s1_unmet && !report.s3_unmet && !report.s4_unmet);
    assert_eq!(report.study, "unmet{s1_failed}");
    assert_eq!(report.admission, "recompute_only");
    assert_eq!(report.disposition, Disposition::Reported);
    let j = report.to_json().to_canonical_string();
    assert!(j.contains("\"disposition\":\"reported\""));
    assert!(report.packet_id.chars().all(|c| c.is_ascii_hexdigit()));
    // Emit the report record when asked (the committed artifact is
    // produced by this exact path — never hand-written).
    if let Ok(out) = std::env::var("HH_STUDY_REPORT_EMIT") {
        let out = PathBuf::from(&out);
        let out = if out.is_absolute() {
            out
        } else {
            repo_root().join(out)
        };
        std::fs::write(&out, format!("{j}\n")).unwrap();
        eprintln!("emitted {} disposition=reported", out.display());
    }
    // The efficacy packet — the per-stratum contrast records the
    // landed estimator produced, verbatim (never hand-written).
    if let Ok(out) = std::env::var("HH_EFFICACY_EMIT") {
        let out = PathBuf::from(&out);
        let out = if out.is_absolute() {
            out
        } else {
            repo_root().join(out)
        };
        let ej = efficacy_packet_json(&results);
        std::fs::write(&out, format!("{}\n", ej.to_canonical_string())).unwrap();
        eprintln!("emitted {}", out.display());
    }
}

/// The `hh-s4-efficacy/1` record — the GATE-G19 complete-cohort fold:
/// the registered declaration, the cohort counts and the per-stratum
/// `finite_manifest_contrast` outputs (or typed refusals) verbatim.
fn efficacy_packet_json(
    results: &BTreeMap<
        String,
        Result<hharness_eval::finite_manifest::FiniteManifestContrast, ContrastError>,
    >,
) -> Json {
    let mut members: Vec<(&'static str, Json)> = Vec::new();
    members.push(("schema", Json::str("hh-s4-efficacy/1")));
    members.push(("admission", Json::str("recompute_only")));
    members.push((
        "basis",
        Json::str(
            "complete-cohort — GATE-G19 PASSED 2026-11-06 (operator verbatim \"confirmed and proceed\"): all 300/300 cohort slots carry a real receipt across the four named epochs",
        ),
    ));
    members.push(("protocol_id", Json::str(COHORT_PROTOCOL_ID)));
    members.push((
        "declared",
        Json::obj([
            ("population", Json::str("canonical")),
            ("test_arm", Json::str("arm:h-podman-oracle")),
            ("control_arm", Json::str("arm:o-tb-oracle")),
            ("prior", Json::str("beta{1,1}")),
            ("direction", Json::str("test_minus_control")),
            ("level_ppm", Json::Int(950_000)),
            ("draws_requested", Json::Int(8_000)),
            ("seed", Json::Int(0)),
        ]),
    ));
    let (mut resolved, mut unresolved) = (0u64, 0u64);
    for (_, v) in receipt_verdicts() {
        match v.as_deref() {
            Some("resolved") => resolved += 1,
            Some("unresolved") => unresolved += 1,
            _ => {}
        }
    }
    members.push((
        "cohort",
        Json::obj([
            ("slots", Json::Int(300)),
            ("receipts", Json::Int((resolved + unresolved) as i64)),
            ("resolved", Json::Int(resolved as i64)),
            ("unresolved", Json::Int(unresolved as i64)),
            ("inconclusive", Json::Int(0)),
            (
                "epochs",
                Json::Arr(COHORT_PACKETS.iter().map(|n| Json::str(*n)).collect()),
            ),
        ]),
    ));
    let mut strata_json = std::collections::BTreeMap::new();
    let mut all_computed = true;
    for (st, r) in results {
        match r {
            Ok(c) => strata_json.insert(st.clone(), c.to_json()),
            Err(e) => {
                all_computed = false;
                strata_json.insert(
                    st.clone(),
                    Json::obj([
                        ("computed", Json::Bool(false)),
                        ("refusal", Json::str(e.to_string())),
                    ]),
                )
            }
        };
    }
    members.push(("strata", Json::Obj(strata_json)));
    members.push((
        "verdict",
        Json::str(if all_computed {
            format!(
                "computed{{strata:{}}}",
                results.keys().cloned().collect::<Vec<_>>().join(",")
            )
        } else {
            "partial".to_string()
        }),
    ));
    Json::Obj(
        members
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

#[test]
fn p49_fold_honest_and_legs_closed() {
    let mk = |v: LegVerdict| -> Vec<Leg> { LEGS.iter().map(|n| leg(n, v.clone(), &[])).collect() };
    assert_eq!(
        assemble(mk(LegVerdict::Verified), &study_fold())
            .unwrap()
            .disposition,
        Disposition::Reported
    );
    assert_eq!(
        assemble(mk(LegVerdict::Inconclusive("x".into())), &study_fold())
            .unwrap()
            .disposition,
        Disposition::Inconclusive
    );
    // A definitive failure dominates an undetermined leg.
    let mut legs = mk(LegVerdict::Inconclusive("x".into()));
    legs[3].verdict = LegVerdict::Refused("z".into());
    assert_eq!(
        assemble(legs, &study_fold()).unwrap().disposition,
        Disposition::Failed
    );
    // Malformed leg sets refuse — never a partial packet.
    assert!(assemble(mk(LegVerdict::Verified)[..10].to_vec(), &study_fold()).is_err());
    let mut bad = mk(LegVerdict::Verified);
    bad[0].name = "not_a_leg".into();
    assert!(assemble(bad, &study_fold()).is_err());
    // A refused leg never folds to reported.
    let (mut legs, _, _) = run_packet();
    legs[10].verdict = LegVerdict::Refused("spend".into());
    assert_eq!(
        assemble(legs, &study_fold()).unwrap().disposition,
        Disposition::Failed
    );
}

#[test]
fn p49_efficacy_complete_cohort() {
    let (ef, _pop, results) = efficacy_facts();
    assert_eq!(evaluate_efficacy(&ef), LegVerdict::Verified);
    // The three declared strata each compute through the landed
    // estimator — the contrast members are real numbers, not echoes.
    for st in &ef.strata {
        assert!(st.computed, "{}: {}", st.stratum, st.refusal);
        assert!(st.draws_used >= 64_000, "{}", st.stratum);
        assert!(st.interval_lo_ppm <= st.interval_hi_ppm);
        assert_eq!(
            results
                .get(&st.stratum)
                .and_then(|r| r.as_ref().ok())
                .map(|c| (
                    c.observed_contrast_ppm,
                    c.posterior_mean_ppm,
                    c.interval.lo,
                    c.interval.hi,
                )),
            Some((
                st.observed_contrast_ppm,
                st.posterior_mean_ppm,
                st.interval_lo_ppm,
                st.interval_hi_ppm
            ))
        );
    }
    assert_eq!(ef.status, "computed{strata:easy,hard,medium}");
    // A missing receipt folds the honest incomplete-cohort refusal —
    // never a dropped member.
    let mut short = ef.clone();
    short.slots[0].receipt_verdict = "absent".into();
    assert!(matches!(
        evaluate_efficacy(&short),
        LegVerdict::Inconclusive(_)
    ));
    // A foreign arm, a drifted cell, a mis-spelt declaration or a
    // status lying about the fold refuses — never relabelled.
    for f in [
        EfficacyFacts {
            population: "raw".into(),
            ..ef.clone()
        },
        EfficacyFacts {
            test_arm: "arm:o-tb-oracle".into(),
            control_arm: "arm:h-podman-oracle".into(),
            ..ef.clone()
        },
        EfficacyFacts {
            prior: "beta{2,2}".into(),
            ..ef.clone()
        },
        EfficacyFacts {
            level_ppm: 900_000,
            ..ef.clone()
        },
        EfficacyFacts {
            draws_requested: 2_000,
            ..ef.clone()
        },
        EfficacyFacts {
            status: "executed".into(),
            ..ef.clone()
        },
    ] {
        assert!(
            matches!(evaluate_efficacy(&f), LegVerdict::Refused(_)),
            "{f:?}"
        );
    }
    let mut drift = ef.clone();
    drift.slots[0].arm_id = "arm:foreign".into();
    assert!(matches!(evaluate_efficacy(&drift), LegVerdict::Refused(_)));
    let mut cell = ef.clone();
    cell.strata[0].cells[0].test_c += 1;
    assert!(matches!(evaluate_efficacy(&cell), LegVerdict::Refused(_)));
    let mut dropped = ef.clone();
    dropped.slots.pop();
    assert!(matches!(
        evaluate_efficacy(&dropped),
        LegVerdict::Refused(_)
    ));
    // The cell table re-adds to the committed fold — task-for-task.
    for st in &ef.strata {
        let mut want: BTreeMap<String, [[u64; 2]; 2]> = BTreeMap::new();
        for s in &ef.slots {
            if s.stratum != st.stratum {
                continue;
            }
            let arm = (s.arm_id == ef.test_arm) as usize;
            let e = want.entry(s.task_id.clone()).or_insert([[0; 2]; 2]);
            e[arm][1] += 1;
            if s.receipt_verdict == "resolved" {
                e[arm][0] += 1;
            }
        }
        for c in &st.cells {
            let w = &want[&c.task_id];
            assert_eq!((c.test_c, c.test_n), (w[0][0], w[0][1]), "{}", c.task_id);
            assert_eq!(
                (c.control_c, c.control_n),
                (w[1][0], w[1][1]),
                "{}",
                c.task_id
            );
        }
    }
}
