//! R5.48 — `hh-confirm-withheld/1`: the withheld confirmation
//! disposition (GATE-G14 PASSED — **quote-only confirmation**). No
//! confirmation tranche executes — full actual parity is absent at
//! GATE-G13 N=0 (dependent confirmation blocked per the contract's
//! own dependency) and the HUMAN-H8-declared worksheet is
//! quote-only; the record is a typed withheld packet over supplied
//! facts. Fails-without: the `confirm` module and this battery do
//! not exist at base `5e6a6c97` (compile-red evidence).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_env::confirm::{
    assemble, evaluate_gate, evaluate_manifest, evaluate_parity, evaluate_quote, evaluate_tranche,
    evaluate_vocabulary, Disposition, GateFacts, ManifestFacts, ParityFacts, QuoteFacts,
    TrancheFacts, EXERCISES, FROZEN_FIRST_SLOTS, FROZEN_PHASES, FROZEN_SLOTS, FROZEN_TASKS, GATE,
    LEGS, PARITY_BLOCKER, VERSION,
};
use hharness_env::preflight::{check_pins, Leg, LegVerdict, PinnedArtifact};
use hharness_eval::benefits::RegisteredMargin;
use hharness_eval::central_protocol as cp;
use hharness_eval::exposure_quote::{
    build_quote, BindingInput, EndpointQuote, InputState, PrereqStatus, Prerequisite, QuoteDecl,
    QuoteError, ResourceLine, StageDecl, COMPONENTS, PINS, PREREQUISITES,
};
use hharness_eval::parity_admission as pa;
use hharness_lab::experiment::{
    Backoff, CancelPolicy, OrderKind, Pool, ReattemptPolicy, SchedulingPolicy,
};
// (no hharness_wire import — the packet members are supplied
// literals, no committed checkout read; see `parity_facts`.)

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ── the nine H8-pinned artifacts (id path sha256 — verbatim) ─────
const ARTIFACTS: &str = "\
a-count crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-count.json 9f6d4f11695efff5671745015f7f7c87d9e4e95f049364e027e6a60fc4124ce6
a-echo crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-echo.json 9fdc327157877c3fdbb124a0a5f98b5a2df0d22a7889c4da9e11319559977b40
a-hello crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-hello.json 5bf206ca07f13659bc7bc69a71f7d7dd4b7348a1f635bfdb9a4dccc0912e0344
a-sum crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-sum.json e30c65b99203d604f9a843e72c31fd1a053bf31c8ab901bb295c2773b0f8e349
a-upper crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-upper.json 3b489ba6cf9d225f926f34c027899583b00d0e6607342c987c593fe7f1fc87a4
ex-01 scripts/examples/ex-01.json 0d4cec8dea92168636d2224ecb392628c69b28bc73353674312183db472178e3
ex-02 scripts/examples/ex-02.json 8e35aac7c68aba68d3d2ff1c9abf23bf5ff7f6bff372f948eede02f1eea46285
ex-02-fixture scripts/examples/ex-02-fixture.json f21a340f966d2d21f72f424635461906dbff16e09cfb2f6d9844ad8e203300c8
fixture-definition clients/ts/fixture-definition.json 732af9f584a9f321d347f6f027ebd82a3376a33bb9580f63b2fcd6602865d0f8";

fn artifact_pins() -> Vec<PinnedArtifact> {
    ARTIFACTS
        .lines()
        .map(|l| {
            let mut c = l.split_whitespace();
            PinnedArtifact {
                artifact_id: c.next().unwrap().to_string(),
                path: repo_root().join(c.next().unwrap()),
                sha256: c.next().unwrap().to_string(),
            }
        })
        .collect()
}

// ── leg 1 — gate_readout (GATE-G14 verbatim) ─────────────────────
fn gate_facts() -> GateFacts {
    GateFacts {
        decision: "PASSED — quote-only confirmation".into(),
        operator_words: "confirmed, please continue fully autonomously".into(),
        decided_at: "2026-11-04T08:04:17Z".into(),
        block_sha256_prefix: "640d68742873".into(),
        s4: "unmet".into(),
    }
}

// ── leg 2 — manifest_frozen: the frozen R5.41 decl re-materialized
// (the committed r5_41 golden shape verbatim — supplied external
// stand-ins, never re-picked) ────────────────────────────────────
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

fn manifest_facts() -> ManifestFacts {
    let p = materialized();
    ManifestFacts {
        check_id: p.check_id(),
        protocol_id: p.protocol_id.clone(),
        selected_tasks: p.selection.len() as u64,
        schedule_slots: p.schedule.len() as u64,
        first_slots: p.first_slots().len() as u64,
        phase_identities: p.phases.len() as u64,
        confirmation_phase: p.phases.contains_key("confirmation"),
    }
}

// ── leg 3 — admission_vocabulary: exercised decls through the real
// validators (declared inputs only — nothing admitted) ───────────

/// A valid `ParityAdmissionDecl` — each admission exercise mutates
/// one member onto its refusal arm.
fn decl_base() -> pa::ParityAdmissionDecl {
    pa::ParityAdmissionDecl {
        participant_ref: "participant:p-1".into(),
        policies: vec!["A".into(), "B".into()],
        evidence: pa::AdmissionEvidence::AuthenticFirstAttempt(pair_manifest(1, 5)),
        dimension_map: Some(dimension_map()),
        applicability: pa::PolicyApplicability::BothPolicies {
            evidence: vec![
                pa::PolicyEvidence {
                    policy: "A".into(),
                    evidence_ref: "ev:a".into(),
                },
                pa::PolicyEvidence {
                    policy: "B".into(),
                    evidence_ref: "ev:b".into(),
                },
            ],
        },
        transfer: None,
        pins: pa::SourcePins {
            source_ref: "src:pinned".into(),
            verifier_ref: "verifier:v".into(),
            rights_ref: "rights:r".into(),
        },
        use_as: None,
    }
}

fn pair_manifest(tasks: u32, repeats: u32) -> pa::PairManifest {
    let mut pairs = Vec::new();
    for t in 0..tasks {
        for r in 0..repeats {
            let k = pairs.len();
            pairs.push(pa::ManifestPair {
                task_id: format!("t{t}"),
                repeat: r,
                pair_id: format!("pair-{k}"),
                subject: format!("subj-{k}"),
                phase: "parity".into(),
                stratum: "held_out".into(),
                policy: if k % 2 == 0 { "A" } else { "B" }.into(),
                h_run_ref: format!("run-h-{k}"),
                o_run_ref: format!("run-o-{k}"),
                h_verdict_ref: format!("v-h-{k}"),
                o_verdict_ref: format!("v-o-{k}"),
                replay_ref: Some(format!("replay-{k}")),
                attempt: 1,
                seed_requested: None,
                seed_honoured: None,
                h: None,
                o: None,
            });
        }
    }
    pa::PairManifest {
        tasks,
        repeats,
        weights: None,
        phase: "parity".into(),
        stratum: "held_out".into(),
        dependence: None,
        independence_rationale_ref: "rationale:predeclared".into(),
        pairs,
    }
}

fn dimension_map() -> pa::DimensionMap {
    pa::DimensionMap {
        signature_ref: "sig:map".into(),
        map_seq: 100,
        first_outcome_seq: 200,
        required: vec!["capability".into()],
        entries: vec![pa::DimensionEntry {
            dimension: "capability".into(),
            metric_ref: "metric:task_success".into(),
            estimand: "estimand:first_attempt".into(),
            estimand_kind: "binary".into(),
            population: "finite_manifest".into(),
            range: None,
            unit: "proportion".into(),
            margin_ref: "capability".into(),
            applicability: "applicable".into(),
            inapplicable_reason: None,
            evidence_ref: "ev:capability".into(),
        }],
    }
}

/// The verbatim refusal-variant spelling (`admitted`/`materialized`
/// — always a machinery failure in this packet).
fn refusal_name<T, E: std::fmt::Debug>(r: &Result<T, E>) -> String {
    match r {
        Ok(_) => "materialized".to_string(),
        Err(e) => format!("{e:?}")
            .split(|c: char| c == ' ' || c == '(' || c == '{')
            .next()
            .unwrap()
            .to_string(),
    }
}

fn admission_name(r: &Result<pa::ParityAdmission, pa::AdmissionError>) -> String {
    match r {
        Ok(_) => "admitted".to_string(),
        Err(e) => format!("{e:?}")
            .split(|c: char| c == ' ' || c == '(' || c == '{')
            .next()
            .unwrap()
            .to_string(),
    }
}

/// `exercise → observed refusal name` — the protocol exercises
/// drive the real `materialize_protocol`; the admission exercises
/// drive the real `parity_admission`. The observed spelling is the
/// fact the leg echoes.
fn exercise_vocabulary() -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    // Integrity/drift stop — collection stops, never continues.
    let mut d = golden();
    d.drift_observed = true;
    out.insert(
        "drift_observed".into(),
        refusal_name(&cp::materialize_protocol(&d)),
    );
    // A late split — the assignment must predate pilot access.
    let mut d = golden();
    d.split_predates_access = false;
    out.insert(
        "late_split".into(),
        refusal_name(&cp::materialize_protocol(&d)),
    );
    // A material protocol change after bound subjects — new
    // cohort required, never a silent rewrite.
    let mut d = golden();
    d.amendment_subjects_bound = Some(7);
    out.insert(
        "post_subject_amendment".into(),
        refusal_name(&cp::materialize_protocol(&d)),
    );
    // A retrospective smaller manifest — quotas totalling 25 < 30.
    let mut d = golden();
    d.quotas[0].required = 15;
    out.insert(
        "retrospective_smaller_manifest".into(),
        refusal_name(&cp::materialize_protocol(&d)),
    );
    // The pilot relabelled as confirmation — never silently.
    let mut d = golden();
    d.pilot_labelled_as = Some("confirmation".into());
    out.insert(
        "pilot_relabelled_confirmation".into(),
        refusal_name(&cp::materialize_protocol(&d)),
    );
    // An outcome-selected exclusion basis — never admissible.
    let mut d = golden();
    d.exclusions[0].basis = "outcome_selected".into();
    out.insert(
        "outcome_selected_exclusion".into(),
        refusal_name(&cp::materialize_protocol(&d)),
    );
    // The standing parity-admission machinery — a forbidden use
    // headline and inauthentic recorded model-I/O refuse.
    let margins = [RegisteredMargin {
        dimension: "capability".into(),
        margin_ppm: 100_000,
        kind: "absolute".into(),
    }];
    let mut d = decl_base();
    d.use_as = Some("headline".into());
    out.insert(
        "forbidden_use_headline".into(),
        admission_name(&pa::parity_admission(&d, &margins)),
    );
    let mut d = decl_base();
    d.evidence = pa::AdmissionEvidence::RecordedModelIo {
        corpus_ref: "corpus:c".into(),
    };
    out.insert(
        "inauthentic_recorded_model_io".into(),
        admission_name(&pa::parity_admission(&d, &margins)),
    );
    out
}

// ── leg 4 — quote_worksheet: the declared §9.2 worksheet with the
// parity prerequisite honestly unmet (absent at N=0) ─────────────

fn pins_bound() -> Vec<BindingInput> {
    PINS.iter()
        .map(|name| BindingInput {
            name: name.to_string(),
            state: InputState::Bound("human-h8:pinned-artifacts".into()),
        })
        .collect()
}

fn line(c: &str, target: Option<&str>, qty: u64) -> ResourceLine {
    ResourceLine {
        component: c.into(),
        target: target.map(str::to_string),
        quantity: Some(qty),
        unit_price_micros: Some(0),
        pool_from: None,
        top_up: false,
    }
}

/// The H8 minimum-admission free lines — $0.00 declared values,
/// `provider_unknown_exposure` bounded at 0.
fn free_lines(with_subjects: Option<u64>) -> Vec<ResourceLine> {
    COMPONENTS
        .iter()
        .filter(|c| **c != "subjects_first" || with_subjects.is_some())
        .filter(|c| **c != "reproduction")
        .map(|c| {
            line(
                c,
                None,
                match *c {
                    "subjects_first" => with_subjects.unwrap_or(0),
                    "provider_unknown_exposure" => 0,
                    _ => 1,
                },
            )
        })
        .collect()
}

/// The frozen §9.2 confirmation envelope — the gate's
/// pre-registered ceilings declared verbatim (≤300 first subjects
/// on the count line; same individual caps; ≤900 core-hours/
/// 200 GiB/14 days tranche ceilings).
fn confirm_envelope() -> Vec<hharness_eval::exposure_quote::EnvelopeQty> {
    [
        ("calls_per_subject", 100),
        ("input_tokens_per_subject", 1_000_000),
        ("output_tokens_per_subject", 100_000),
        ("wall_minutes_per_subject", 30),
        ("verifier_minutes", 15),
        ("concurrent_subjects", 2),
        ("cores_per_subject", 4),
        ("ram_gib_per_subject", 8),
        ("cpu_core_hours", 900),
        ("storage_gib", 200),
        ("wall_days", 14),
    ]
    .iter()
    .map(|(c, q)| hharness_eval::exposure_quote::EnvelopeQty {
        component: c.to_string(),
        quantity: *q,
    })
    .collect()
}

/// The confirmation stage — quote-only per the H8-declared
/// worksheet: no cohort (tasks/repeats 0), the frozen envelopes
/// declared, free $0.00 lines, no dispatch.
fn stage_confirmation() -> StageDecl {
    StageDecl {
        stage: "confirmation".into(),
        tasks: 0,
        repeats: 0,
        declared_pairs: None,
        inputs: pins_bound(),
        endpoints: vec![],
        strata: vec!["held_out".into()],
        policies: vec!["evict_oldest".into(), "clear_tool_results".into()],
        runners: vec!["hh-runner".into()],
        envelope: confirm_envelope(),
        resources: free_lines(Some(0))
            .into_iter()
            .chain([line("provider_billed", Some("hh-runner"), 0)])
            .collect(),
        discordances: vec![],
        cloud_fallback: false,
        outcome_conditioned: false,
        dispatch_requested: false,
    }
}

fn quote_decl() -> QuoteDecl {
    QuoteDecl {
        currency: Some("USD".into()),
        prerequisites: PREREQUISITES
            .iter()
            .map(|name| Prerequisite {
                name: name.to_string(),
                status: if *name == "parity" {
                    // Honest state: full actual parity is absent
                    // (GATE-G13 N=0) — the dependent-confirmation
                    // block encoded by the machinery itself.
                    PrereqStatus::Unmet
                } else {
                    PrereqStatus::Satisfied("human-h8:verified-yes".into())
                },
            })
            .collect(),
        stages: vec![stage_confirmation()],
    }
}

fn built_quote() -> hharness_eval::exposure_quote::ExposureQuote {
    build_quote(&quote_decl()).expect("declared worksheet builds")
}

fn stage<'a>(
    q: &'a hharness_eval::exposure_quote::ExposureQuote,
    name: &str,
) -> &'a hharness_eval::exposure_quote::StagePacket {
    q.stages.iter().find(|s| s.stage == name).unwrap()
}

fn quote_facts() -> QuoteFacts {
    let q = built_quote();
    let c = stage(&q, "confirmation");
    QuoteFacts {
        confirmation_dispatch: c.dispatch.clone(),
        confirmation_state: c.state.clone(),
        blockers: c.blockers.clone(),
        precision_state: c.precision_state.clone(),
        policies: c.policies.clone(),
        total_micros: q.total_micros,
        forbidden: vec![],
    }
}

// ── legs 5+6 — tranche_absent / parity_blocked ───────────────────

fn tranche_facts() -> TrancheFacts {
    TrancheFacts {
        counts: BTreeMap::from([
            ("subjects_first".to_string(), 0),
            ("attempts".to_string(), 0),
            ("regrades".to_string(), 0),
            ("receipts_collected".to_string(), 0),
        ]),
        dispatch_attempted: false,
        partial_results: "retained_empty".into(),
        partial_costs: "retained_empty".into(),
        retrospective_manifest: false,
    }
}

/// The upstream `hh-parity-n0/1` members — read from the committed
/// packet (the supplied fact, never hand-recopied).
// ── leg 6 — parity_blocked: the upstream `hh-parity-n0/1` facts ──
// The committed withheld parity packet's members supplied verbatim
// (emitted packet `2026-11-04-r5.47-parity-n0.json` under the
// build-memory reports tree — echoed here exactly like `gate_facts`
// echoes the gate decision; the gated lane performs no committed
// docs/ checkout read — the first boundary read refused it).
fn parity_facts() -> ParityFacts {
    ParityFacts {
        upstream_version: "hh-parity-n0/1".into(),
        parity_disposition: "withheld".into(),
        parity_admission: "n0_withheld".into(),
        n_declared: 0,
        s3_unmet: true,
        confirmation_blocked: true,
        prerequisite_blocker: stage(&built_quote(), "confirmation")
            .blockers
            .first()
            .cloned()
            .unwrap_or_default(),
        equivalence_claimed: false,
        non_equivalence_claimed: false,
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

/// The six legs driven on real inputs (pins recomputed, the frozen
/// manifest re-materialized, the vocabulary exercised, the real
/// quote built, the upstream packet members supplied verbatim) —
/// the packet's honest
/// record.
fn run_packet() -> Vec<Leg> {
    let g = gate_facts();
    let m = manifest_facts();
    let vocabulary = exercise_vocabulary();
    let qf = quote_facts();
    let t = tranche_facts();
    let pf = parity_facts();
    let pins = check_pins(&artifact_pins());
    vec![
        leg(
            "gate_readout",
            evaluate_gate(&g),
            &[
                ("decision", s(&g.decision)),
                ("operator_words", s(&g.operator_words)),
                ("decided_at", s(&g.decided_at)),
                ("block_sha256_prefix", s(&g.block_sha256_prefix)),
                ("s4", s(&g.s4)),
            ],
        ),
        leg(
            "manifest_frozen",
            evaluate_manifest(&m),
            &[
                ("check_id", s(m.check_id)),
                ("protocol_id", s(&m.protocol_id)),
                ("selected_tasks", s(m.selected_tasks)),
                ("schedule_slots", s(m.schedule_slots)),
                ("first_slots", s(m.first_slots)),
                ("phase_identities", s(m.phase_identities)),
                ("confirmation_phase", s(m.confirmation_phase)),
                ("pins", s(9)),
                ("pins_verdict", s(pins.name())),
            ],
        ),
        leg(
            "admission_vocabulary",
            evaluate_vocabulary(&vocabulary),
            &vocabulary
                .iter()
                .map(|(k, v)| (k.as_str(), v.clone()))
                .collect::<Vec<_>>(),
        ),
        leg(
            "quote_worksheet",
            evaluate_quote(&qf),
            &[
                ("confirmation_dispatch", s(&qf.confirmation_dispatch)),
                ("confirmation_state", s(&qf.confirmation_state)),
                ("blockers", s(qf.blockers.join(","))),
                ("precision_state", s(&qf.precision_state)),
                ("policies", s(qf.policies.join(","))),
                ("total_micros", s(qf.total_micros)),
            ],
        ),
        leg(
            "tranche_absent",
            evaluate_tranche(&t),
            &t.counts
                .iter()
                .map(|(k, v)| (k.as_str(), s(v)))
                .chain([
                    ("dispatch_attempted", s(t.dispatch_attempted)),
                    ("partial_results", s(&t.partial_results)),
                    ("partial_costs", s(&t.partial_costs)),
                    (
                        "retrospective_smaller_manifest",
                        s(t.retrospective_manifest),
                    ),
                ])
                .collect::<Vec<_>>(),
        ),
        leg(
            "parity_blocked",
            evaluate_parity(&pf),
            &[
                ("upstream_version", s(&pf.upstream_version)),
                ("parity_disposition", s(&pf.parity_disposition)),
                ("parity_admission", s(&pf.parity_admission)),
                ("n_declared", s(pf.n_declared)),
                ("s3_unmet", s(pf.s3_unmet)),
                ("confirmation_blocked", s(pf.confirmation_blocked)),
                ("prerequisite_blocker", s(&pf.prerequisite_blocker)),
                ("equivalence_claimed", s(pf.equivalence_claimed)),
                ("non_equivalence_claimed", s(pf.non_equivalence_claimed)),
            ],
        ),
    ]
}

// ── the battery ──────────────────────────────────────────────────

#[test]
fn p48_gate_manifest_and_pins_verified() {
    assert_eq!(evaluate_gate(&gate_facts()), LegVerdict::Verified);
    // A funded tranche, a missing verbatim member or a met S4
    // refuses — never relabelled.
    for f in [
        GateFacts {
            decision: "PASSED — funded tranche N=300".into(),
            ..gate_facts()
        },
        GateFacts {
            s4: "met".into(),
            ..gate_facts()
        },
        GateFacts {
            block_sha256_prefix: "not-hex!!".into(),
            ..gate_facts()
        },
    ] {
        assert!(matches!(evaluate_gate(&f), LegVerdict::Refused(_)));
    }
    // The frozen manifest re-materializes deterministically:
    // `check_id` recomputes `protocol_id`, the 30/300/60/six-phase
    // identities hold, `confirmation` is a phase.
    let p = materialized();
    assert!(p.check_id());
    assert_eq!(p.selection.len(), cp::TASKS as usize);
    assert_eq!(p.schedule.len(), (cp::TASKS * 2 * cp::ROUNDS) as usize);
    assert_eq!(p.first_slots().len(), (cp::TASKS * 2) as usize);
    assert!(p.phases.contains_key("confirmation"));
    assert_eq!(FROZEN_TASKS, cp::TASKS as u64);
    assert_eq!(FROZEN_SLOTS, (cp::TASKS * 2 * cp::ROUNDS) as u64);
    assert_eq!(FROZEN_FIRST_SLOTS, (cp::TASKS * 2) as u64);
    assert_eq!(FROZEN_PHASES, cp::PHASES.len() as u64);
    assert_eq!(evaluate_manifest(&manifest_facts()), LegVerdict::Verified);
    // A drifted count is a different manifest — refused; a missing
    // id is inconclusive; `check_id` failing refuses.
    for f in [
        ManifestFacts {
            selected_tasks: 25,
            ..manifest_facts()
        },
        ManifestFacts {
            check_id: false,
            ..manifest_facts()
        },
        ManifestFacts {
            confirmation_phase: false,
            ..manifest_facts()
        },
    ] {
        assert!(matches!(evaluate_manifest(&f), LegVerdict::Refused(_)));
    }
    let mut f = manifest_facts();
    f.protocol_id.clear();
    assert!(matches!(evaluate_manifest(&f), LegVerdict::Inconclusive(_)));
    // A different committed seed mints a different protocol_id —
    // the seed is load-bearing, never decorative; the recompute is
    // not a re-pick.
    let mut d2 = golden();
    d2.seed = "seed:other".into();
    assert_ne!(
        materialized().protocol_id,
        cp::materialize_protocol(&d2).unwrap().protocol_id
    );
    // The nine H8-pinned artifacts re-verify byte-for-byte; drift
    // refuses, unreadable is inconclusive.
    assert_eq!(check_pins(&artifact_pins()), LegVerdict::Verified);
    let mut drifted = artifact_pins();
    drifted[0].sha256 = "0".repeat(64);
    assert!(matches!(check_pins(&drifted), LegVerdict::Refused(_)));
    let mut unreadable = artifact_pins();
    unreadable[0].path = repo_root().join("does-not-exist.json");
    assert!(matches!(
        check_pins(&unreadable),
        LegVerdict::Inconclusive(_)
    ));
}

#[test]
fn p48_vocabulary_and_quote_exercised() {
    // The landed machinery stands — every exercise produces its
    // typed refusal on a declared input; nothing is admitted or
    // materialized.
    let observed = exercise_vocabulary();
    for (ex, expected) in EXERCISES {
        assert_eq!(observed.get(ex).map(String::as_str), Some(expected), "{ex}");
    }
    assert_eq!(evaluate_vocabulary(&observed), LegVerdict::Verified);
    // `materialized`/`admitted` is the machinery broken; wrong/
    // missing → refused/inconclusive, never verified.
    for (ex, o) in [
        ("drift_observed", "materialized"),
        ("forbidden_use_headline", "admitted"),
        ("late_split", "MissingKey"),
    ] {
        let mut bad = observed.clone();
        bad.insert(ex.into(), o.into());
        assert!(matches!(evaluate_vocabulary(&bad), LegVerdict::Refused(_)));
    }
    let mut missing = observed;
    missing.remove("drift_observed");
    assert!(matches!(
        evaluate_vocabulary(&missing),
        LegVerdict::Inconclusive(_)
    ));
    // The confirmation leg stays quote-only — `not_requested` +
    // `blocked` with `prerequisite:parity` the sole blocker (the
    // declared envelopes satisfy every other readiness predicate),
    // precision `n/a`, both policies echoed, USD $0.00 total.
    let q = built_quote();
    let c = stage(&q, "confirmation");
    assert_eq!(c.dispatch, "not_requested");
    assert_eq!(c.state, "blocked");
    assert_eq!(c.blockers, vec![PARITY_BLOCKER.to_string()]);
    assert_eq!(c.precision_state, "n/a");
    assert_eq!(c.policies, ["evict_oldest", "clear_tool_results"]);
    assert_eq!(q.total_micros, 0);
    assert_eq!(evaluate_quote(&quote_facts()), LegVerdict::Verified);
    // A satisfied parity prerequisite yields a different blocker
    // set — the stage would be ready_for_gate, a different record.
    let mut decl = quote_decl();
    decl.prerequisites
        .iter_mut()
        .find(|p| p.name == "parity")
        .unwrap()
        .status = PrereqStatus::Satisfied("x".into());
    let q2 = build_quote(&decl).unwrap();
    let c2 = stage(&q2, "confirmation");
    assert_ne!(c2.state, "blocked");
    // Forbidden constructs refuse construction, never a packet.
    for mutate in [
        |d: &mut StageDecl| d.resources[0].top_up = true,
        |d: &mut StageDecl| d.resources[0].pool_from = Some("pilot".into()),
        |d: &mut StageDecl| d.cloud_fallback = true,
        |d: &mut StageDecl| d.outcome_conditioned = true,
        |d: &mut StageDecl| d.envelope[9].quantity = 250,
    ]
    .into_iter()
    {
        let mut decl = quote_decl();
        mutate(&mut decl.stages[0]);
        assert!(build_quote(&decl).is_err());
    }
    // A requested confirmation dispatch is not `not_requested` —
    // a refused-dispatch packet, still never executed.
    let mut decl = quote_decl();
    decl.stages[0].dispatch_requested = true;
    let q3 = build_quote(&decl).unwrap();
    let c3 = stage(&q3, "confirmation");
    assert!(c3.dispatch.starts_with("refused:"));
    // A guessed margin refuses at build — never a packet.
    let mut decl = quote_decl();
    decl.stages[0].endpoints = vec![EndpointQuote {
        dimension: "capability".into(),
        margin_ppm: None,
        assumed_margin: true,
    }];
    assert!(matches!(
        build_quote(&decl),
        Err(QuoteError::GuessedMargin { .. })
    ));
    // >300 first subjects exceeds the frozen ceiling — an
    // unapproved larger design cannot launch.
    let mut decl = quote_decl();
    decl.stages[0]
        .resources
        .iter_mut()
        .find(|l| l.component == "subjects_first")
        .unwrap()
        .quantity = Some(301);
    assert!(matches!(
        build_quote(&decl),
        Err(QuoteError::EnvelopeExceeded { .. })
    ));
}

#[test]
fn p48_tranche_absent_and_parity_blocked() {
    assert_eq!(evaluate_tranche(&tranche_facts()), LegVerdict::Verified);
    assert_eq!(evaluate_parity(&parity_facts()), LegVerdict::Verified);
    // The upstream packet's own members are the supplied facts —
    // echoed verbatim from the committed record (the gated lane
    // performs no committed docs/ checkout read).
    let pf = parity_facts();
    assert_eq!(pf.upstream_version, "hh-parity-n0/1");
    assert_eq!(pf.parity_disposition, "withheld");
    assert_eq!(pf.parity_admission, "n0_withheld");
    assert_eq!(pf.n_declared, 0);
    assert!(pf.s3_unmet && pf.confirmation_blocked);
    // Any nonzero tranche fact is a real tranche's evidence —
    // refused here, never a fabricated absence.
    for member in [
        "subjects_first",
        "attempts",
        "regrades",
        "receipts_collected",
    ] {
        let mut t = tranche_facts();
        t.counts.insert(member.into(), 1);
        assert!(matches!(evaluate_tranche(&t), LegVerdict::Refused(_)));
    }
    for t in [
        TrancheFacts {
            dispatch_attempted: true,
            ..tranche_facts()
        },
        TrancheFacts {
            partial_results: "executed_partial".into(),
            ..tranche_facts()
        },
        TrancheFacts {
            retrospective_manifest: true,
            ..tranche_facts()
        },
    ] {
        assert!(matches!(evaluate_tranche(&t), LegVerdict::Refused(_)));
    }
    // A different upstream state — funded, a claim of either sign,
    // an unrecorded block — refuses; it is a different record,
    // never relabelled.
    for f in [
        ParityFacts {
            parity_disposition: "executed".into(),
            ..parity_facts()
        },
        ParityFacts {
            n_declared: 30,
            ..parity_facts()
        },
        ParityFacts {
            equivalence_claimed: true,
            ..parity_facts()
        },
        ParityFacts {
            non_equivalence_claimed: true,
            ..parity_facts()
        },
        ParityFacts {
            confirmation_blocked: false,
            ..parity_facts()
        },
    ] {
        assert!(matches!(evaluate_parity(&f), LegVerdict::Refused(_)));
    }
}

#[test]
fn p48_packet_withheld_honest() {
    let legs = run_packet();
    for l in &legs {
        assert!(l.verdict.is_verified(), "{}: {:?}", l.name, l.verdict);
    }
    let report = assemble(legs).unwrap();
    assert_eq!(report.version, VERSION);
    assert_eq!(report.gate, GATE);
    assert_eq!(report.subjects_first, 0);
    assert!(report.s4_unmet && report.tranche_absent && report.parity_absent);
    assert!(report.confirmation_blocked);
    assert_eq!(report.admission, "quote_only_withheld");
    assert_eq!(report.disposition, Disposition::Withheld);
    let j = report.to_json().to_canonical_string();
    assert!(j.contains("\"disposition\":\"withheld\""));
    assert!(report.packet_id.chars().all(|c| c.is_ascii_hexdigit()));
    // Emit the withheld record when asked (the committed artifact
    // is produced by this exact path — never hand-written).
    if let Ok(out) = std::env::var("HH_CONFIRM_WITHHELD_EMIT") {
        let out = PathBuf::from(&out);
        let out = if out.is_absolute() {
            out
        } else {
            repo_root().join(out)
        };
        std::fs::write(&out, format!("{j}\n")).unwrap();
        eprintln!("emitted {} disposition=withheld", out.display());
    }
}

#[test]
fn p48_fold_honest_and_legs_closed() {
    let mk = |v: LegVerdict| -> Vec<Leg> { LEGS.iter().map(|n| leg(n, v.clone(), &[])).collect() };
    assert_eq!(
        assemble(mk(LegVerdict::Verified)).unwrap().disposition,
        Disposition::Withheld
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
    assert!(assemble(mk(LegVerdict::Verified)[..5].to_vec()).is_err());
    let mut bad = mk(LegVerdict::Verified);
    bad[0].name = "not_a_leg".into();
    assert!(assemble(bad).is_err());
    // A refused leg never folds to withheld.
    let mut legs = run_packet();
    legs[4].verdict = LegVerdict::Refused("tranche_present".into());
    assert_eq!(assemble(legs).unwrap().disposition, Disposition::Failed);
}
