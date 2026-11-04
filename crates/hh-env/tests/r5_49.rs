//! R5.49 — `hh-study-report/1`: the admitted-study recompute report
//! (GATE-G14 PASSED — **quote-only confirmation**). The report
//! recomputes the admitted results from the nine committed packets —
//! refolded dispositions, re-derived §4.2 denominators, nearest-rank
//! tails, the §3.2 missingness bound, envelope maxima and freeze/
//! `protocol_id` identities — asserting byte/consistency equality,
//! never re-picking or fabricating. No confirmation evidence exists
//! to report: the efficacy/sharp-null families stay `unavailable`.
//! Fails-without: the `study` module and this battery do not exist
//! at base `b1a8644d` (compile-red evidence).
//!
//! The gated lane performs **no committed-record checkout read** —
//! every committed packet's members are supplied verbatim here,
//! exactly like `gate_facts`/`parity_facts` in the r5_48 battery.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_env::preflight::{Leg, LegVerdict};
use hharness_env::study::{
    assemble, check_echo, evaluate_activation, evaluate_confirm_stage, evaluate_costs,
    evaluate_families, evaluate_freeze, evaluate_ops, evaluate_parity_stage, evaluate_pilot,
    evaluate_preflight, evaluate_useful, recompute_arm_tail, recompute_maxima,
    recompute_missingness, ActivationFacts, ArmTailEcho, ConfirmStageFacts, CostFacts, Disposition,
    FamilyFacts, FeasibilityEcho, FidelityFold, FreezeFacts, MissingnessEcho, OpsFacts, PacketEcho,
    ParityStageFacts, PilotFacts, PilotSubjectEcho, PreflightStageFacts, SubjectEcho, UsefulFacts,
    LEGS, REQUIRED_FAMILIES, VERSION,
};
use hharness_eval::central_protocol as cp;
use hharness_lab::experiment::{
    Backoff, CancelPolicy, OrderKind, Pool, ReattemptPolicy, SchedulingPolicy,
};

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

fn parity_facts() -> ParityStageFacts {
    ParityStageFacts {
        echo: parity_echo(),
        n_declared: 0,
        s3_unmet: true,
        confirmation_blocked: true,
        gate: "GATE-G13".into(),
    }
}

fn confirm_facts() -> ConfirmStageFacts {
    ConfirmStageFacts {
        echo: confirm_echo(),
        subjects_first: 0,
        tranche_absent: true,
        parity_absent: true,
        confirmation_blocked: true,
        s4_unmet: true,
        gate: "GATE-G14".into(),
    }
}

fn family_facts() -> FamilyFacts {
    FamilyFacts {
        efficacy: "unavailable{confirmation_tranche_absent}".into(),
        task_mixture: "descriptive{pilot_recomputed}".into(),
        sharp_null: "unavailable{confirmation_tranche_absent}".into(),
        parity: "withheld{n_declared:0}".into(),
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
        ]
        .iter()
        .map(|p| (p.to_string(), 0))
        .collect(),
        subjects_first: 12,
        gated_subjects: 0,
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

/// The ten legs driven on the supplied committed members — the
/// report's honest recompute.
fn run_packet() -> Vec<Leg> {
    let fr = freeze_facts();
    let pf = preflight_facts();
    let u = useful_facts();
    let o = ops_facts();
    let a = activation_facts();
    let p = pilot_facts();
    let pa = parity_facts();
    let c = confirm_facts();
    let fam = family_facts();
    let co = cost_facts();
    vec![
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
                ("disposition", s(&pa.echo.disposition)),
                ("n_declared", s(pa.n_declared)),
                ("s3_unmet", s(pa.s3_unmet)),
                ("confirmation_blocked", s(pa.confirmation_blocked)),
                ("status", s("withheld{n_declared:0}")),
            ],
        ),
        leg(
            "confirmation",
            evaluate_confirm_stage(&c),
            &[
                ("disposition", s(&c.echo.disposition)),
                ("subjects_first", s(c.subjects_first)),
                ("tranche_absent", s(c.tranche_absent)),
                ("parity_absent", s(c.parity_absent)),
                ("s4_unmet", s(c.s4_unmet)),
                ("status", s("withheld{tranche_absent}")),
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
    ]
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
fn p49_parity_confirm_withheld() {
    assert_eq!(evaluate_parity_stage(&parity_facts()), LegVerdict::Verified);
    for f in [
        ParityStageFacts {
            n_declared: 30,
            ..parity_facts()
        },
        ParityStageFacts {
            confirmation_blocked: false,
            ..parity_facts()
        },
        ParityStageFacts {
            gate: "GATE-G14".into(),
            ..parity_facts()
        },
    ] {
        assert!(matches!(evaluate_parity_stage(&f), LegVerdict::Refused(_)));
    }
    assert_eq!(
        evaluate_confirm_stage(&confirm_facts()),
        LegVerdict::Verified
    );
    for f in [
        ConfirmStageFacts {
            subjects_first: 1,
            ..confirm_facts()
        },
        ConfirmStageFacts {
            tranche_absent: false,
            ..confirm_facts()
        },
        ConfirmStageFacts {
            s4_unmet: false,
            ..confirm_facts()
        },
    ] {
        assert!(matches!(evaluate_confirm_stage(&f), LegVerdict::Refused(_)));
    }
}

#[test]
fn p49_families_and_costs() {
    assert_eq!(evaluate_families(&family_facts()), LegVerdict::Verified);
    // Every required status is the honest spelling — a met/passed
    // unavailable family, a different executed spelling or a claim
    // refuses, never relabelled.
    for f in [
        FamilyFacts {
            efficacy: "met{confirmation_executed}".into(),
            ..family_facts()
        },
        FamilyFacts {
            efficacy: "negative{no_effect}".into(),
            ..family_facts()
        },
        FamilyFacts {
            parity: "equivalent".into(),
            ..family_facts()
        },
        FamilyFacts {
            adoption: "passed".into(),
            ..family_facts()
        },
        FamilyFacts {
            claim_present: true,
            ..family_facts()
        },
    ] {
        assert!(matches!(evaluate_families(&f), LegVerdict::Refused(_)));
    }
    // The required vocabulary is exactly nine families.
    assert_eq!(REQUIRED_FAMILIES.len(), 9);
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
    let legs = run_packet();
    for l in &legs {
        assert!(l.verdict.is_verified(), "{}: {:?}", l.name, l.verdict);
    }
    let report = assemble(legs).unwrap();
    assert_eq!(report.version, VERSION);
    assert_eq!(report.subjects_first, 12);
    assert_eq!(report.exposure_micros, 0);
    assert!(report.s1_unmet && report.s3_unmet && report.s4_unmet);
    assert_eq!(report.study, "unmet{efficacy_unavailable;parity_withheld}");
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
}

#[test]
fn p49_fold_honest_and_legs_closed() {
    let mk = |v: LegVerdict| -> Vec<Leg> { LEGS.iter().map(|n| leg(n, v.clone(), &[])).collect() };
    assert_eq!(
        assemble(mk(LegVerdict::Verified)).unwrap().disposition,
        Disposition::Reported
    );
    assert_eq!(
        assemble(mk(LegVerdict::Inconclusive("x".into())))
            .unwrap()
            .disposition,
        Disposition::Inconclusive
    );
    // A definitive failure dominates an undetermined leg.
    let mut legs = mk(LegVerdict::Inconclusive("x".into()));
    legs[3].verdict = LegVerdict::Refused("z".into());
    assert_eq!(assemble(legs).unwrap().disposition, Disposition::Failed);
    // Malformed leg sets refuse — never a partial packet.
    assert!(assemble(mk(LegVerdict::Verified)[..9].to_vec()).is_err());
    let mut bad = mk(LegVerdict::Verified);
    bad[0].name = "not_a_leg".into();
    assert!(assemble(bad).is_err());
    // A refused leg never folds to reported.
    let mut legs = run_packet();
    legs[9].verdict = LegVerdict::Refused("spend".into());
    assert_eq!(assemble(legs).unwrap().disposition, Disposition::Failed);
}
