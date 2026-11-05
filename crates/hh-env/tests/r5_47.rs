//! R5.47 — `hh-parity-n0/1`: the declared N=0 parity disposition
//! (GATE-G13 PASSED — **N=0 declared**). The parity tranche stays
//! quote-only per the HUMAN-H8-declared worksheet — nothing
//! executes; the record is a typed withheld packet over supplied
//! facts. Fails-without: the `parity` module and this battery do
//! not exist at base `ac942db3` (compile-red evidence).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_env::parity::{
    assemble, evaluate_dependent, evaluate_gate, evaluate_quote, evaluate_tranche,
    evaluate_vocabulary, DependentFacts, Disposition, GateFacts, QuoteFacts, TrancheFacts,
    EXERCISES, GATE, LEGS, VERSION,
};
use hharness_env::preflight::{check_pins, Leg, LegVerdict, PinnedArtifact};
use hharness_eval::benefits::RegisteredMargin;
use hharness_eval::exposure_quote::{
    build_quote, BindingInput, EndpointQuote, InputState, PrereqStatus, Prerequisite, QuoteDecl,
    QuoteError, ResourceLine, StageDecl, COMPONENTS, PINS, PREREQUISITES,
};
use hharness_eval::parity_admission as pa;

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

// ── leg 1/3 facts over the landed machinery (supplied, verbatim) ──

fn gate_facts() -> GateFacts {
    GateFacts {
        decision: "PASSED — N=0 declared".into(),
        operator_words: "confirm, proceed".into(),
        decided_at: "2026-11-04T07:12:23Z".into(),
        block_sha256_prefix: "29be1faa6511".into(),
        n_declared: 0,
        s3: "unmet".into(),
    }
}

/// A valid `ParityAdmissionDecl` — each exercise mutates one member
/// onto its refusal arm.
fn decl_base() -> pa::ParityAdmissionDecl {
    pa::ParityAdmissionDecl {
        participant_ref: "participant:p-1".into(),
        policies: vec!["A".into(), "B".into()],
        evidence: pa::AdmissionEvidence::AuthenticFirstAttempt(manifest(1, 5)),
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

fn manifest(tasks: u32, repeats: u32) -> pa::PairManifest {
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

/// The verbatim refusal-variant spelling (`admitted` — always a
/// machinery failure in this packet).
fn refusal_name(r: &Result<pa::ParityAdmission, pa::AdmissionError>) -> String {
    match r {
        Ok(_) => "admitted".to_string(),
        Err(e) => format!("{e:?}")
            .split(|c: char| c == ' ' || c == '(' || c == '{')
            .next()
            .unwrap()
            .to_string(),
    }
}

/// `exercise → observed refusal name` — each decl driven through the
/// real `parity_admission` validator; the observed spelling is the
/// fact the leg echoes.
fn exercise_admission() -> BTreeMap<String, String> {
    let m = [RegisteredMargin {
        dimension: "capability".into(),
        margin_ppm: 100_000,
        kind: "absolute".into(),
    }];
    let mut decls: Vec<(&str, pa::ParityAdmissionDecl)> = Vec::new();
    let mut d = decl_base();
    d.use_as = Some("headline".into());
    decls.push(("forbidden_use_headline", d));
    let mut d = decl_base();
    d.evidence = pa::AdmissionEvidence::RecordedModelIo {
        corpus_ref: "corpus:c".into(),
    };
    decls.push(("inauthentic_recorded_model_io", d));
    let mut d = decl_base();
    d.participant_ref.clear();
    decls.push(("empty_participant_ref", d));
    // The absent tranche cannot even form a manifest (tasks=0).
    let mut d = decl_base();
    d.evidence = pa::AdmissionEvidence::AuthenticFirstAttempt(manifest(0, 5));
    decls.push(("absent_tranche_manifest", d));
    let mut d = decl_base();
    d.dimension_map = None;
    decls.push(("absent_dimension_map", d));
    // Baseline-only parity never admits the changed policy.
    let mut d = decl_base();
    d.applicability = pa::PolicyApplicability::BaselineOnly {
        covered: vec!["A".into()],
    };
    decls.push(("baseline_only_applicability", d));
    decls
        .iter()
        .map(|(id, d)| (id.to_string(), refusal_name(&pa::parity_admission(d, &m))))
        .collect()
}

// ── the declared §9.2 worksheet (parity + confirmation legs) ─────

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

/// The parity stage — quote-only per the H8-declared worksheet:
/// N unfixed (tasks/repeats 0), both runners quoted at $0.00
/// declared lines, margins unset (never assumed), no dispatch.
fn stage_parity() -> StageDecl {
    StageDecl {
        stage: "parity".into(),
        tasks: 0,
        repeats: 0,
        declared_pairs: None,
        inputs: pins_bound(),
        endpoints: ["capability", "efficiency"]
            .iter()
            .map(|d| EndpointQuote {
                dimension: d.to_string(),
                margin_ppm: None,
                assumed_margin: false,
            })
            .collect(),
        strata: vec!["held_out".into()],
        policies: vec!["evict_oldest".into(), "clear_tool_results".into()],
        runners: vec!["hh-runner".into(), "original-runner".into()],
        envelope: vec![],
        resources: free_lines(None)
            .into_iter()
            .chain([
                line("provider_billed", Some("hh-runner"), 0),
                line("provider_billed", Some("original-runner"), 0),
            ])
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
                status: PrereqStatus::Satisfied("human-h8:verified-yes".into()),
            })
            .collect(),
        stages: vec![
            stage_parity(),
            StageDecl {
                stage: "confirmation".into(),
                runners: vec!["hh-runner".into()],
                endpoints: vec![],
                resources: free_lines(Some(0)),
                ..stage_parity()
            },
        ],
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
    let p = stage(&q, "parity");
    QuoteFacts {
        parity_dispatch: p.dispatch.clone(),
        parity_state: p.state.clone(),
        parity_precision: p.precision_state.clone(),
        design_unfixed_blocker: p.blockers.iter().any(|b| b == "precision:design_unfixed"),
        runners: p.runners.clone(),
        total_micros: q.total_micros,
        forbidden: vec![],
    }
}

fn tranche_facts() -> TrancheFacts {
    TrancheFacts {
        counts: BTreeMap::from([
            ("n_declared".to_string(), 0),
            ("pairs_collected".to_string(), 0),
            ("h_attempts".to_string(), 0),
            ("o_attempts".to_string(), 0),
        ]),
        dispatch_attempted: false,
        kl_verdicts: "none".into(),
    }
}

fn dependent_facts() -> DependentFacts {
    let q = built_quote();
    DependentFacts {
        confirmation_dispatch: stage(&q, "confirmation").dispatch.clone(),
        equivalence_claimed: false,
        non_equivalence_claimed: false,
        withheld_blocks: true,
        inconclusive_blocks: true,
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

/// The six legs driven on real inputs (pins recomputed, admission
/// exercised, the real quote built) — the packet's honest record.
fn run_packet() -> Vec<Leg> {
    let g = gate_facts();
    let vocabulary = exercise_admission();
    let qf = quote_facts();
    let t = tranche_facts();
    let d = dependent_facts();
    vec![
        leg(
            "gate_readout",
            evaluate_gate(&g),
            &[
                ("decision", s(&g.decision)),
                ("operator_words", s(&g.operator_words)),
                ("decided_at", s(&g.decided_at)),
                ("block_sha256_prefix", s(&g.block_sha256_prefix)),
                ("n_declared", s(g.n_declared)),
                ("s3", s(&g.s3)),
            ],
        ),
        leg(
            "pins_frozen",
            check_pins(&artifact_pins()),
            &[("artifacts", s(9))],
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
                ("parity_dispatch", s(&qf.parity_dispatch)),
                ("parity_state", s(&qf.parity_state)),
                ("parity_precision", s(&qf.parity_precision)),
                ("runners", s(qf.runners.join(","))),
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
                    ("kl_verdicts", s(&t.kl_verdicts)),
                ])
                .collect::<Vec<_>>(),
        ),
        leg(
            "dependent_blocked",
            evaluate_dependent(&d),
            &[
                ("confirmation_dispatch", s(&d.confirmation_dispatch)),
                ("equivalence_claimed", s(d.equivalence_claimed)),
                ("non_equivalence_claimed", s(d.non_equivalence_claimed)),
                ("withheld_blocks", s(d.withheld_blocks)),
                ("inconclusive_blocks", s(d.inconclusive_blocks)),
            ],
        ),
    ]
}

// ── the battery ──────────────────────────────────────────────────

#[test]
fn p47_gate_and_pins_verified() {
    assert_eq!(evaluate_gate(&gate_facts()), LegVerdict::Verified);
    // A nonzero N, a different decision, a missing verbatim member
    // or a met S3 refuses — never relabelled.
    for f in [
        GateFacts {
            n_declared: 5,
            ..gate_facts()
        },
        GateFacts {
            decision: "PASSED — N=10 funded".into(),
            ..gate_facts()
        },
        GateFacts {
            s3: "met".into(),
            ..gate_facts()
        },
        GateFacts {
            block_sha256_prefix: "not-hex!!".into(),
            ..gate_facts()
        },
    ] {
        assert!(matches!(evaluate_gate(&f), LegVerdict::Refused(_)));
    }
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
fn p47_vocabulary_and_quote_exercised() {
    // The landed R5.39 machinery stands — every exercise produces
    // its typed refusal on a declared input; nothing is admitted.
    let observed = exercise_admission();
    for (ex, expected) in EXERCISES {
        assert_eq!(observed.get(ex).map(String::as_str), Some(expected), "{ex}");
    }
    assert_eq!(evaluate_vocabulary(&observed), LegVerdict::Verified);
    // `admitted` is the machinery broken; wrong/missing → refused/
    // inconclusive, never verified.
    for (ex, o) in [
        ("baseline_only_applicability", "admitted"),
        ("absent_dimension_map", "MissingKey"),
    ] {
        let mut bad = observed.clone();
        bad.insert(ex.into(), o.into());
        assert!(matches!(evaluate_vocabulary(&bad), LegVerdict::Refused(_)));
    }
    let mut missing = observed;
    missing.remove("absent_dimension_map");
    assert!(matches!(
        evaluate_vocabulary(&missing),
        LegVerdict::Inconclusive(_)
    ));
    // The parity leg stays quote-only — N unfixed, precision
    // unresolved, both runners quoted at $0.00, USD $0.00 total.
    let q = built_quote();
    let p = stage(&q, "parity");
    assert_eq!(p.dispatch, "not_requested");
    assert_eq!(p.state, "blocked");
    assert_eq!(p.precision_state, "unresolved");
    assert!(p.blockers.iter().any(|b| b == "precision:design_unfixed"));
    assert_eq!(p.runners, ["hh-runner", "original-runner"]);
    assert_eq!(q.total_micros, 0);
    assert_eq!(stage(&q, "confirmation").dispatch, "not_requested");
    assert_eq!(evaluate_quote(&quote_facts()), LegVerdict::Verified);
    // Forbidden constructs refuse construction, never a packet.
    for (i, mutate) in [
        |d: &mut StageDecl| d.dispatch_requested = true,
        |d: &mut StageDecl| d.endpoints[0].assumed_margin = true,
        |d: &mut StageDecl| d.resources[0].top_up = true,
        |d: &mut StageDecl| d.resources[0].pool_from = Some("pilot".into()),
        |d: &mut StageDecl| d.cloud_fallback = true,
    ]
    .into_iter()
    .enumerate()
    {
        let _ = i;
        let mut decl = quote_decl();
        mutate(&mut decl.stages[0]);
        if i == 0 {
            // A requested parity dispatch is not `not_requested`.
            let q = build_quote(&decl).unwrap();
            assert_ne!(stage(&q, "parity").dispatch, "not_requested");
        } else {
            assert!(build_quote(&decl).is_err(), "construct {i}");
        }
    }
    let mut decl = quote_decl();
    decl.stages[0].endpoints[0].assumed_margin = true;
    assert!(matches!(
        build_quote(&decl),
        Err(QuoteError::GuessedMargin { .. })
    ));
}

#[test]
fn p47_tranche_absent_and_dependent_blocked() {
    assert_eq!(evaluate_tranche(&tranche_facts()), LegVerdict::Verified);
    assert_eq!(evaluate_dependent(&dependent_facts()), LegVerdict::Verified);
    // Any nonzero tranche fact or claimed verdict is a real
    // tranche's evidence — refused here, never a fabricated absence.
    for member in ["n_declared", "pairs_collected", "h_attempts", "o_attempts"] {
        let mut t = tranche_facts();
        t.counts.insert(member.into(), 1);
        assert!(matches!(evaluate_tranche(&t), LegVerdict::Refused(_)));
    }
    for mut t in [tranche_facts(), tranche_facts()] {
        if t.kl_verdicts == "none" {
            t.dispatch_attempted = true;
        } else {
            t.kl_verdicts = "equivalent".into();
        }
        assert!(matches!(evaluate_tranche(&t), LegVerdict::Refused(_)));
    }
    let mut t = tranche_facts();
    t.kl_verdicts = "equivalent".into();
    assert!(matches!(evaluate_tranche(&t), LegVerdict::Refused(_)));
    // A claim of either sign refuses the dependent leg.
    for d in [
        DependentFacts {
            equivalence_claimed: true,
            ..dependent_facts()
        },
        DependentFacts {
            non_equivalence_claimed: true,
            ..dependent_facts()
        },
        DependentFacts {
            confirmation_dispatch: "gate_pending:GATE-G14".into(),
            ..dependent_facts()
        },
    ] {
        assert!(matches!(evaluate_dependent(&d), LegVerdict::Refused(_)));
    }
}

#[test]
fn p47_packet_withheld_honest() {
    let legs = run_packet();
    for l in &legs {
        assert!(l.verdict.is_verified(), "{}: {:?}", l.name, l.verdict);
    }
    let report = assemble(legs).unwrap();
    assert_eq!(report.version, VERSION);
    assert_eq!(report.gate, GATE);
    assert_eq!(report.n_declared, 0);
    assert!(report.s3_unmet && report.confirmation_blocked);
    assert_eq!(report.admission, "n0_withheld");
    assert_eq!(report.disposition, Disposition::Withheld);
    let j = report.to_json().to_canonical_string();
    assert!(j.contains("\"disposition\":\"withheld\""));
    assert!(report.packet_id.chars().all(|c| c.is_ascii_hexdigit()));
    // Emit the withheld record when asked (the committed artifact is
    // produced by this exact path — never hand-written).
    if let Ok(out) = std::env::var("HH_PARITY_N0_EMIT") {
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
fn p47_fold_honest_and_legs_closed() {
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
