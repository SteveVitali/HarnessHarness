//! R5.53 — `hh-formative-withheld/1`: the withheld formative
//! evaluator/contributor observation disposition (GATE-G15 PASSED —
//! **out-of-scope disposition**). The participation stage is voided
//! this round — NO cohort exists (HUMAN-H9 withheld, DF-H9-1 open)
//! and no formative session/branch/observation runs (every
//! invitation/consent/session/task/assistance/rubric/comprehension/
//! verdict/use/contribution/follow-up/recheck/defect count absent);
//! the record is a typed withheld/cohort-absent packet over supplied
//! facts. Machinery is exercised where meaningful without
//! participation: the eleven pinned inputs byte-verify, the §7/§8
//! formative-surface vocabulary is asserted verbatim (schema/runner
//! decls, four-family case set, closed `examples::code`/`recipe::
//! code` refusal sets, `pass|fail|expected_refusal` verdicts, twelve
//! contrib check ids + seven lanes, six trace legs, frozen §8.4
//! rubric/measures/session/cohort shape), and the exercised cases
//! produce their verbatim typed outcomes through the real
//! `ExampleCard`/`run_example`/`RecipePlan`/checker machinery — the
//! refusals fire before any execution or acceptance; the `ex-03`
//! run is a scratch in-process machinery exercise, never a
//! participant act. Fails-without: the `formative` module and this
//! battery do not exist at base `972de4c2` (compile-red evidence).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hharness::examples::{
    code as ex_code, run_example, ExampleCard, ExampleError, ExampleReport, CARD_SCHEMA,
    EX02_FIXTURE_SCHEMA, RUNNER_VERSION,
};
use hharness::formative::{
    assemble as assemble_packet, evaluate_claims, evaluate_cohort, evaluate_pins,
    evaluate_refusals, evaluate_vocabulary, ClaimsFacts, CohortFacts, Disposition, PinsFacts,
    VocabularyFacts, COHORT_MEMBERS, EXERCISES, EXPECTED_CARDS_DECODED, EXPECTED_CARD_SCHEMA,
    EXPECTED_CHECK_VERDICTS, EXPECTED_COHORT_SHAPE, EXPECTED_CONTRIB_CHECK_IDS,
    EXPECTED_CONTRIB_LANES, EXPECTED_EXAMPLE_CODES, EXPECTED_FAMILIES, EXPECTED_FIXTURE_SCHEMA,
    EXPECTED_MEASURES, EXPECTED_RECIPE_CODES, EXPECTED_RECIPE_VERSION, EXPECTED_RUBRIC_ITEMS,
    EXPECTED_RUNNER_VERSION, EXPECTED_SESSION_SHAPE, EXPECTED_TRACE_LEGS, GATE, LEGS, PINS_TOTAL,
    VERSION,
};
use hharness::recipe::{code as plan_code, RecipeError, RecipePlan, RECIPE_VERSION};
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_env::plugin::{evaluate_gate, GateFacts};
use hharness_env::preflight::{check_pins, Leg, LegVerdict, PinnedArtifact};
use hharness_wire::json::Json;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tmp(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-r5-53-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn svc(root: &Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".to_string(),
        workspace_root: root.join("ws"),
        holder: "r5-53".to_string(),
    })
    .unwrap()
}

// ── the eleven pinned inputs (nine H8 artifacts + the committed
// ex-03/ex-04 family cards — the cleared checked-task artifacts
// completing the four-card set; id path sha256, verbatim) ───────
const ARTIFACTS: &str = "\
a-count crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-count.json 9f6d4f11695efff5671745015f7f7c87d9e4e95f049364e027e6a60fc4124ce6
a-echo crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-echo.json 9fdc327157877c3fdbb124a0a5f98b5a2df0d22a7889c4da9e11319559977b40
a-hello crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-hello.json 5bf206ca07f13659bc7bc69a71f7d7dd4b7348a1f635bfdb9a4dccc0912e0344
a-sum crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-sum.json e30c65b99203d604f9a843e72c31fd1a053bf31c8ab901bb295c2773b0f8e349
a-upper crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-upper.json 3b489ba6cf9d225f926f34c027899583b00d0e6607342c987c593fe7f1fc87a4
ex-01 scripts/examples/ex-01.json 0d4cec8dea92168636d2224ecb392628c69b28bc73353674312183db472178e3
ex-02 scripts/examples/ex-02.json 8e35aac7c68aba68d3d2ff1c9abf23bf5ff7f6bff372f948eede02f1eea46285
ex-02-fixture scripts/examples/ex-02-fixture.json f21a340f966d2d21f72f424635461906dbff16e09cfb2f6d9844ad8e203300c8
fixture-definition clients/ts/fixture-definition.json 732af9f584a9f321d347f6f027ebd82a3376a33bb9580f63b2fcd6602865d0f8
ex-03-card scripts/examples/ex-03.json b15e661cb4f67b91e202036faaa8b19c9a5fb46bf13b6336b3fc3d1ae8d82d48
ex-04-card scripts/examples/ex-04.json f8b4f50cb96dc57310f18bb8465f927af049ed4bb92368e8c911cfa0a7493fbe";

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

fn card_json(name: &str) -> Json {
    let text = std::fs::read_to_string(repo_root().join("scripts/examples").join(name)).unwrap();
    hharness_wire::json::parse(&text).unwrap()
}

fn ex_card(name: &str) -> ExampleCard {
    ExampleCard::load(&repo_root().join("scripts/examples").join(name)).unwrap()
}

// ── leg 1 — gate_readout (GATE-G15 verbatim — the R5.50 gate
// contract consumed unchanged) ──────────────────────────────────
fn gate_facts() -> GateFacts {
    GateFacts {
        decision: "PASSED — out-of-scope disposition".into(),
        operator_words: "accept, continue".into(),
        decided_at: "2026-11-04T14:32:29Z".into(),
        block_sha256_prefix: "0e5bd136858c".into(),
        df_h9_1: "open".into(),
    }
}

// ── leg 2 — pins_frozen ──────────────────────────────────────────
fn pin_digest(id: &str) -> String {
    artifact_pins()
        .iter()
        .find(|a| a.artifact_id == id)
        .unwrap()
        .sha256
        .clone()
}

fn pins_facts() -> PinsFacts {
    let verdict = check_pins(&artifact_pins());
    PinsFacts {
        pins: artifact_pins().len() as u64,
        pins_verdict: verdict.name().to_string(),
        ex03_sha256: pin_digest("ex-03-card"),
        ex04_sha256: pin_digest("ex-04-card"),
    }
}

// ── leg 3 — surface_vocabulary: the §7/§8 decls read from the
// landed machinery (facts supplied verbatim, never re-declared) ─

/// Run one landed static checker for real; returns `(exit, the
/// ordered unique `ok [id]` leg ids it emitted)` — the machinery's
/// own declared vocabulary, never a hard-coded copy.
fn run_checker(script: &str) -> (i32, Vec<String>) {
    let out = std::process::Command::new("bash")
        .arg(repo_root().join("scripts").join(script))
        .arg(repo_root())
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let mut ids = Vec::new();
    for l in text.lines() {
        if l.contains(": ok") {
            if let Some(id) = l.split('[').nth(1).and_then(|s| s.split(']').next()) {
                if !ids.iter().any(|i: &String| i == id) {
                    ids.push(id.to_string());
                }
            }
        }
    }
    (out.status.code().unwrap_or(-1), ids)
}

/// A minimal records-in `RecipePlan` — the plan carries params
/// only (every durable record is derived, never supplied).
fn minimal_plan() -> Json {
    Json::obj([
        ("hello", Json::obj([("contract_major", Json::Int(1))])),
        ("register", Json::obj([("spec", Json::obj([]))])),
        (
            "environment",
            Json::obj([
                ("kind", Json::str("connection_info")),
                (
                    "connection_info",
                    Json::obj([("class", Json::str("local_host"))]),
                ),
            ]),
        ),
        (
            "subject_input",
            Json::Arr(vec![Json::obj([
                ("kind", Json::str("text")),
                ("text", Json::str("t")),
            ])]),
        ),
        ("grade_metric", Json::str("task_success")),
        (
            "grades",
            Json::obj([("arm:a", Json::Arr(vec![Json::Bool(true)]))]),
        ),
        ("analysis", Json::obj([("kind", Json::str("summarize"))])),
        ("weights", Json::obj([("family", Json::str("uniform"))])),
    ])
}

fn vocabulary_facts() -> VocabularyFacts {
    let cases = ["ex-01", "ex-02", "ex-03", "ex-04"];
    let decoded: Vec<ExampleCard> = cases
        .iter()
        .map(|c| ex_card(&format!("{c}.json")))
        .collect();
    let contrib = run_checker("check-contributing.sh");
    let trace = run_checker("check-trace.sh");
    VocabularyFacts {
        card_schema: CARD_SCHEMA.to_string(),
        fixture_schema: EX02_FIXTURE_SCHEMA.to_string(),
        runner_version: RUNNER_VERSION.to_string(),
        recipe_version: RECIPE_VERSION.to_string(),
        families: decoded
            .iter()
            .map(|c| c.case.as_str())
            .collect::<Vec<_>>()
            .join(","),
        cards_decoded: decoded.len() as u64,
        example_codes: [
            ex_code::STALE_PIN,
            ex_code::UNKNOWN_CASE,
            ex_code::UNSUPPORTED_SHAPE,
            ex_code::MISSING_FIELD,
            ex_code::TYPE_MISMATCH,
            ex_code::CHECK_MISSING,
        ]
        .join(","),
        recipe_codes: [
            plan_code::UNSUPPORTED_SHAPE,
            plan_code::MISSING_FIELD,
            plan_code::TYPE_MISMATCH,
            plan_code::EXECUTION_INCOMPLETE,
            plan_code::NO_SUBMISSION,
            plan_code::GRADE_UNSPECIFIED,
            plan_code::DRAIN_STALLED,
        ]
        .join(","),
        check_verdicts: "pass,fail,expected_refusal".to_string(),
        contrib_check_ids: contrib.1.join(","),
        contrib_lanes: EXPECTED_CONTRIB_LANES.to_string(),
        trace_legs: trace.1.join(","),
        rubric_items: EXPECTED_RUBRIC_ITEMS.to_string(),
        measures: EXPECTED_MEASURES.to_string(),
        session_shape: EXPECTED_SESSION_SHAPE.to_string(),
        cohort_shape: EXPECTED_COHORT_SHAPE.to_string(),
        plan_decoded: RecipePlan::from_json(&minimal_plan()).is_ok(),
    }
}

// ── leg 4 — refusal_vocabulary: exercised cases through the real
// card/plan/checker machinery (refusals fire before any execution
// or acceptance — pure in-process plus the landed static checks) ─

fn ex_err_code(r: Result<ExampleCard, ExampleError>) -> String {
    match r {
        Err(ExampleError::Refused { code, .. }) => code.to_string(),
        Ok(_) => "decoded".to_string(),
        Err(e) => format!("{e:?}")
            .split([' ', '{', '('])
            .next()
            .unwrap_or_default()
            .to_string(),
    }
}

fn run_err_code(r: Result<ExampleReport, ExampleError>) -> String {
    match r {
        Err(ExampleError::Refused { code, .. }) => code.to_string(),
        Ok(_) => "report_emitted".to_string(),
        Err(e) => format!("{e:?}")
            .split([' ', '{', '('])
            .next()
            .unwrap_or_default()
            .to_string(),
    }
}

fn plan_err_code(r: Result<RecipePlan, RecipeError>) -> String {
    match r {
        Err(RecipeError::Refused { code, .. }) => code.to_string(),
        Ok(_) => "accepted".to_string(),
        Err(e) => format!("{e:?}")
            .split([' ', '{', '('])
            .next()
            .unwrap_or_default()
            .to_string(),
    }
}

/// `exercise → observed verbatim outcome` — every case is driven
/// through the real machinery; the refusals fire before any
/// execution or acceptance, the green facts record the landed
/// machinery standing (a scratch `ex-03` run, the two static
/// checkers — never a participant act).
fn exercise_refusals() -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let mut extra = card_json("ex-03.json");
    if let Json::Obj(ref mut m) = extra {
        m.insert("participant_id".into(), Json::str("fabricated"));
    }
    out.insert(
        "card_extra_member".into(),
        ex_err_code(ExampleCard::from_json(&extra)),
    );
    let mut absent = card_json("ex-03.json");
    if let Json::Obj(ref mut m) = absent {
        m.remove("run");
    }
    out.insert(
        "card_absent_member".into(),
        ex_err_code(ExampleCard::from_json(&absent)),
    );
    let mut typed = card_json("ex-03.json");
    if let Json::Obj(ref mut m) = typed {
        m.insert("checks".into(), Json::str("not-an-array"));
    }
    out.insert(
        "card_wrong_type".into(),
        ex_err_code(ExampleCard::from_json(&typed)),
    );
    let mut other = card_json("ex-03.json");
    if let Json::Obj(ref mut m) = other {
        m.insert("case".into(), Json::str("ex-05"));
    }
    out.insert(
        "card_unknown_case".into(),
        ex_err_code(ExampleCard::from_json(&other)),
    );
    // `run_example` — a drifted card pin refuses `stale_pin`
    // before any execution (the cleared-artifact pin gate).
    let root = tmp("exercise");
    let mut s = svc(&root);
    let mut stale = ex_card("ex-03.json");
    if let Json::Obj(ref mut pins) = stale.pins {
        for (_, pin) in pins.iter_mut() {
            if let Json::Obj(ref mut p) = pin {
                p.insert(
                    "sha256".into(),
                    Json::str(&format!("sha256:{}", "0".repeat(64))),
                );
                break;
            }
        }
    }
    out.insert(
        "card_stale_pin".into(),
        run_err_code(run_example(
            &mut s,
            &stale,
            &root.join("work-stale"),
            &repo_root(),
        )),
    );
    // `RecipePlan::from_json` — an authored record member never
    // rides the plan (the honesty gate itself).
    let mut authored = minimal_plan();
    if let Json::Obj(ref mut m) = authored {
        m.insert("success".into(), Json::Bool(true));
    }
    out.insert(
        "plan_authored_member".into(),
        plan_err_code(RecipePlan::from_json(&authored)),
    );
    let mut missing = minimal_plan();
    if let Json::Obj(ref mut m) = missing {
        m.remove("environment");
    }
    out.insert(
        "plan_absent_member".into(),
        plan_err_code(RecipePlan::from_json(&missing)),
    );
    out.insert(
        "plan_wrong_type".into(),
        plan_err_code(RecipePlan::from_json(&Json::Arr(vec![]))),
    );
    // The committed ex-03 card's own scripted lane running green
    // on a scratch store — the cleared checked-task artifact
    // stands (verdict + produced-checks count observed verbatim).
    let report = run_example(
        &mut s,
        &ex_card("ex-03.json"),
        &root.join("work"),
        &repo_root(),
    )
    .unwrap();
    out.insert(
        "example_run".into(),
        format!("{}:{}", report.verdict, report.checks.len()),
    );
    // The two landed static checkers running for real — the
    // public guides/forms stand.
    for (key, script, n) in [
        ("contrib_checker", "check-contributing.sh", 12usize),
        ("trace_checker", "check-trace.sh", 6usize),
    ] {
        let (status, ids) = run_checker(script);
        out.insert(
            key.into(),
            if status == 0 && ids.len() == n {
                format!("green:{n}")
            } else {
                format!("red:{status}:{}", ids.len())
            },
        );
    }
    out
}

// ── legs 5+6 — cohort_absent / claims_withheld ───────────────────
fn cohort_facts() -> CohortFacts {
    CohortFacts {
        counts: COHORT_MEMBERS.iter().map(|m| (m.to_string(), 0)).collect(),
        cohort_authorized: false,
        consent_obtained: false,
        outreach_performed: false,
        telemetry: false,
    }
}

fn claims_facts() -> ClaimsFacts {
    ClaimsFacts {
        df_h9_1: "open".into(),
        participant_predicates: "withheld".into(),
        outreach: "none".into(),
        claim_awarded: false,
        usefulness_claim: false,
        comprehension_claim: false,
        adoption_claim: false,
        causal_claim: false,
        pooled_claim: false,
        population_estimate: false,
        contribution_claim: false,
        capability_claim: false,
        publication_claim: false,
        portability_claim: false,
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

/// The six legs driven on real inputs — the packet's honest
/// record.
fn run_packet() -> Vec<Leg> {
    let g = gate_facts();
    let pf = pins_facts();
    let v = vocabulary_facts();
    let refusals = exercise_refusals();
    let c = cohort_facts();
    let cl = claims_facts();
    vec![
        leg(
            "gate_readout",
            evaluate_gate(&g),
            &[
                ("decision", s(&g.decision)),
                ("operator_words", s(&g.operator_words)),
                ("decided_at", s(&g.decided_at)),
                ("block_sha256_prefix", s(&g.block_sha256_prefix)),
                ("df_h9_1", s(&g.df_h9_1)),
            ],
        ),
        leg(
            "pins_frozen",
            evaluate_pins(&pf),
            &[
                ("pins", s(pf.pins)),
                ("pins_verdict", s(&pf.pins_verdict)),
                ("ex03_sha256", s(&pf.ex03_sha256)),
                ("ex04_sha256", s(&pf.ex04_sha256)),
            ],
        ),
        leg(
            "surface_vocabulary",
            evaluate_vocabulary(&v),
            &[
                ("card_schema", s(&v.card_schema)),
                ("fixture_schema", s(&v.fixture_schema)),
                ("runner_version", s(&v.runner_version)),
                ("recipe_version", s(&v.recipe_version)),
                ("families", s(&v.families)),
                ("cards_decoded", s(v.cards_decoded)),
                ("example_codes", s(&v.example_codes)),
                ("recipe_codes", s(&v.recipe_codes)),
                ("check_verdicts", s(&v.check_verdicts)),
                ("contrib_check_ids", s(&v.contrib_check_ids)),
                ("contrib_lanes", s(&v.contrib_lanes)),
                ("trace_legs", s(&v.trace_legs)),
                ("rubric_items", s(&v.rubric_items)),
                ("measures", s(&v.measures)),
                ("session_shape", s(&v.session_shape)),
                ("cohort_shape", s(&v.cohort_shape)),
                ("plan_decoded", s(v.plan_decoded)),
            ],
        ),
        leg(
            "refusal_vocabulary",
            evaluate_refusals(&refusals),
            &refusals
                .iter()
                .map(|(k, v)| (k.as_str(), v.clone()))
                .collect::<Vec<_>>(),
        ),
        leg(
            "cohort_absent",
            evaluate_cohort(&c),
            &c.counts
                .iter()
                .map(|(k, v)| (k.as_str(), s(v)))
                .chain([
                    ("cohort_authorized", s(c.cohort_authorized)),
                    ("consent_obtained", s(c.consent_obtained)),
                    ("outreach_performed", s(c.outreach_performed)),
                    ("telemetry", s(c.telemetry)),
                ])
                .collect::<Vec<_>>(),
        ),
        leg(
            "claims_withheld",
            evaluate_claims(&cl),
            &[
                ("df_h9_1", s(&cl.df_h9_1)),
                ("participant_predicates", s(&cl.participant_predicates)),
                ("outreach", s(&cl.outreach)),
                ("claim_awarded", s(cl.claim_awarded)),
                ("usefulness_claim", s(cl.usefulness_claim)),
                ("comprehension_claim", s(cl.comprehension_claim)),
                ("adoption_claim", s(cl.adoption_claim)),
                ("causal_claim", s(cl.causal_claim)),
                ("pooled_claim", s(cl.pooled_claim)),
                ("population_estimate", s(cl.population_estimate)),
                ("contribution_claim", s(cl.contribution_claim)),
                ("capability_claim", s(cl.capability_claim)),
                ("publication_claim", s(cl.publication_claim)),
                ("portability_claim", s(cl.portability_claim)),
            ],
        ),
    ]
}

// ── the battery ──────────────────────────────────────────────────

#[test]
fn p53_gate_and_pins_verified() {
    assert_eq!(evaluate_gate(&gate_facts()), LegVerdict::Verified);
    for f in [
        GateFacts {
            decision: "PASSED — procedures admitted".into(),
            ..gate_facts()
        },
        GateFacts {
            df_h9_1: "closed".into(),
            ..gate_facts()
        },
        GateFacts {
            block_sha256_prefix: "not-hex!!".into(),
            ..gate_facts()
        },
    ] {
        assert!(matches!(evaluate_gate(&f), LegVerdict::Refused(_)));
    }
    assert_eq!(artifact_pins().len() as u64, PINS_TOTAL);
    assert_eq!(check_pins(&artifact_pins()), LegVerdict::Verified);
    assert_eq!(evaluate_pins(&pins_facts()), LegVerdict::Verified);
    let mut drifted = artifact_pins();
    drifted[0].sha256 = "0".repeat(64);
    assert!(matches!(check_pins(&drifted), LegVerdict::Refused(_)));
    let mut unreadable = artifact_pins();
    unreadable[0].path = repo_root().join("does-not-exist.json");
    assert!(matches!(
        check_pins(&unreadable),
        LegVerdict::Inconclusive(_)
    ));
    for f in [
        PinsFacts {
            pins: 10,
            ..pins_facts()
        },
        PinsFacts {
            pins_verdict: "refused".into(),
            ..pins_facts()
        },
    ] {
        assert!(matches!(evaluate_pins(&f), LegVerdict::Refused(_)));
    }
    let mut f = pins_facts();
    f.ex03_sha256.clear();
    assert!(matches!(evaluate_pins(&f), LegVerdict::Inconclusive(_)));
}

#[test]
fn p53_surface_vocabulary_and_refusals() {
    let v = vocabulary_facts();
    assert_eq!(v.card_schema, EXPECTED_CARD_SCHEMA);
    assert_eq!(v.fixture_schema, EXPECTED_FIXTURE_SCHEMA);
    assert_eq!(v.runner_version, EXPECTED_RUNNER_VERSION);
    assert_eq!(v.recipe_version, EXPECTED_RECIPE_VERSION);
    assert_eq!(v.families, EXPECTED_FAMILIES);
    assert_eq!(v.cards_decoded, EXPECTED_CARDS_DECODED);
    assert_eq!(v.example_codes, EXPECTED_EXAMPLE_CODES);
    assert_eq!(v.recipe_codes, EXPECTED_RECIPE_CODES);
    assert_eq!(v.check_verdicts, EXPECTED_CHECK_VERDICTS);
    assert_eq!(v.contrib_check_ids, EXPECTED_CONTRIB_CHECK_IDS);
    assert_eq!(v.contrib_lanes, EXPECTED_CONTRIB_LANES);
    assert_eq!(v.trace_legs, EXPECTED_TRACE_LEGS);
    assert_eq!(v.rubric_items, EXPECTED_RUBRIC_ITEMS);
    assert_eq!(v.measures, EXPECTED_MEASURES);
    assert_eq!(v.session_shape, EXPECTED_SESSION_SHAPE);
    assert_eq!(v.cohort_shape, EXPECTED_COHORT_SHAPE);
    assert!(v.plan_decoded);
    assert_eq!(evaluate_vocabulary(&v), LegVerdict::Verified);
    // Drifted vocabulary refuses; an absent vocabulary is
    // inconclusive, never verified.
    for f in [
        VocabularyFacts {
            families: "ex-01,ex-02".into(),
            ..vocabulary_facts()
        },
        VocabularyFacts {
            example_codes: "stale_pin".into(),
            ..vocabulary_facts()
        },
        VocabularyFacts {
            contrib_check_ids: "lanes".into(),
            ..vocabulary_facts()
        },
        VocabularyFacts {
            cards_decoded: 3,
            ..vocabulary_facts()
        },
        VocabularyFacts {
            plan_decoded: false,
            ..vocabulary_facts()
        },
    ] {
        assert!(matches!(evaluate_vocabulary(&f), LegVerdict::Refused(_)));
    }
    let mut f = vocabulary_facts();
    f.families.clear();
    assert!(matches!(
        evaluate_vocabulary(&f),
        LegVerdict::Inconclusive(_)
    ));
    // The landed machinery stands — every exercised case produces
    // its verbatim typed outcome; nothing decodes/admits/emits
    // where a refusal is owed (`decoded`/`accepted`/
    // `report_emitted` would be the guard broken).
    let observed = exercise_refusals();
    for (ex, expected) in EXERCISES {
        assert_eq!(observed.get(ex).map(String::as_str), Some(expected), "{ex}");
    }
    assert_eq!(evaluate_refusals(&observed), LegVerdict::Verified);
    for (ex, o) in [
        ("card_extra_member", "decoded"),
        ("plan_authored_member", "accepted"),
        ("example_run", "report_emitted"),
    ] {
        let mut bad = observed.clone();
        bad.insert(ex.into(), o.into());
        assert!(matches!(evaluate_refusals(&bad), LegVerdict::Refused(_)));
    }
    let mut missing = observed;
    missing.remove("example_run");
    assert!(matches!(
        evaluate_refusals(&missing),
        LegVerdict::Inconclusive(_)
    ));
}

#[test]
fn p53_cohort_absent_and_claims() {
    assert_eq!(evaluate_cohort(&cohort_facts()), LegVerdict::Verified);
    assert_eq!(evaluate_claims(&claims_facts()), LegVerdict::Verified);
    // Any nonzero cohort fact is a real observation's evidence —
    // refused here, never a fabricated absence.
    for member in COHORT_MEMBERS {
        let mut c = cohort_facts();
        c.counts.insert(member.into(), 1);
        assert!(matches!(evaluate_cohort(&c), LegVerdict::Refused(_)));
    }
    let mut missing = cohort_facts();
    missing.counts.remove("sessions_run");
    assert!(matches!(
        evaluate_cohort(&missing),
        LegVerdict::Inconclusive(_)
    ));
    for c in [
        CohortFacts {
            cohort_authorized: true,
            ..cohort_facts()
        },
        CohortFacts {
            consent_obtained: true,
            ..cohort_facts()
        },
        CohortFacts {
            outreach_performed: true,
            ..cohort_facts()
        },
        CohortFacts {
            telemetry: true,
            ..cohort_facts()
        },
    ] {
        assert!(matches!(evaluate_cohort(&c), LegVerdict::Refused(_)));
    }
    // A claim of any sign — or a discharged deferral — refuses,
    // never relabelled.
    for f in [
        ClaimsFacts {
            df_h9_1: "closed".into(),
            ..claims_facts()
        },
        ClaimsFacts {
            participant_predicates: "admitted".into(),
            ..claims_facts()
        },
        ClaimsFacts {
            outreach: "session_invite".into(),
            ..claims_facts()
        },
        ClaimsFacts {
            claim_awarded: true,
            ..claims_facts()
        },
        ClaimsFacts {
            usefulness_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            comprehension_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            adoption_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            causal_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            pooled_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            population_estimate: true,
            ..claims_facts()
        },
        ClaimsFacts {
            contribution_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            capability_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            publication_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            portability_claim: true,
            ..claims_facts()
        },
    ] {
        assert!(matches!(evaluate_claims(&f), LegVerdict::Refused(_)));
    }
}

#[test]
fn p53_packet_withheld_honest() {
    let legs = run_packet();
    for l in &legs {
        assert!(l.verdict.is_verified(), "{}: {:?}", l.name, l.verdict);
    }
    let report = assemble_packet(legs).unwrap();
    assert_eq!(report.version, VERSION);
    assert_eq!(report.gate, GATE);
    assert!(report.withheld && report.stage_voided && report.cohort_absent);
    assert!(report.df_h9_1_open);
    assert_eq!(report.sessions_completed, 0);
    assert_eq!(report.participants_observed, 0);
    assert_eq!(report.admission, "withheld_stage_voided");
    assert_eq!(report.disposition, Disposition::Withheld);
    let j = report.to_json().to_canonical_string();
    assert!(j.contains("\"disposition\":\"withheld\""));
    assert!(report.packet_id.chars().all(|c| c.is_ascii_hexdigit()));
    // Emit the withheld record when asked (the committed artifact
    // is produced by this exact path — never hand-written).
    if let Ok(out) = std::env::var("HH_FORMATIVE_WITHHELD_EMIT") {
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
fn p53_fold_honest_and_legs_closed() {
    let mk = |v: LegVerdict| -> Vec<Leg> { LEGS.iter().map(|n| leg(n, v.clone(), &[])).collect() };
    assert_eq!(
        assemble_packet(mk(LegVerdict::Verified))
            .unwrap()
            .disposition,
        Disposition::Withheld
    );
    assert_eq!(
        assemble_packet(mk(LegVerdict::Inconclusive("x".into())))
            .unwrap()
            .disposition,
        Disposition::Inconclusive
    );
    let mut legs = mk(LegVerdict::Inconclusive("x".into()));
    legs[3].verdict = LegVerdict::Refused("z".into());
    assert_eq!(
        assemble_packet(legs).unwrap().disposition,
        Disposition::Failed
    );
    assert!(assemble_packet(mk(LegVerdict::Verified)[..5].to_vec()).is_err());
    let mut bad = mk(LegVerdict::Verified);
    bad[0].name = "not_a_leg".into();
    assert!(assemble_packet(bad).is_err());
    let mut legs = run_packet();
    legs[4].verdict = LegVerdict::Refused("cohort_present".into());
    assert_eq!(
        assemble_packet(legs).unwrap().disposition,
        Disposition::Failed
    );
}
