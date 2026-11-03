//! R5.16 — the `finite_manifest_contrast` estimator battery (REQ-D-107
//! primary; the GATE-G9-admitted Packet D / PADR-R5-4* method contract).
//! Fails-without: `eval::finite_manifest` does not exist at the R5.15b
//! base — this file compiles red there.

use std::collections::{BTreeMap, BTreeSet};

use hharness_eval::finite_manifest::{
    finite_manifest_contrast, parse_prior_name, ContrastDecl, ContrastError, BASE_DRAWS, CAP_DRAWS,
    CONTRAST_KIND, LEVEL_PPM,
};
use hharness_eval::population::{
    assemble, Availability, BoundsDecl, PlannedPopulation, PlannedSlot, PopulationInput,
    PopulationKind, SlotSubject,
};
use hharness_ontology::control::OutcomeClass;

fn sub(run: &str, ok: Option<bool>) -> SlotSubject {
    SlotSubject {
        subject_run: run.into(),
        attempt_no: 1,
        outcome: ok.map(|_| OutcomeClass::Scored),
        success: ok,
        veto_tripped: vec![],
        availability: if ok.is_some() {
            Availability::Resolved
        } else {
            Availability::NotRun
        },
        seq: 0,
        missing_required_checks: false,
        admissible: true,
    }
}

/// Build the slot records for one `(task, arm)` cell — one bound subject
/// per planned slot (`replicates` slots, first `c` resolved successes,
/// the rest resolved failures; `None` entries are not-run).
fn arm_slots(task: &str, arm: &str, outcomes: &[Option<bool>]) -> Vec<PlannedSlot> {
    outcomes
        .iter()
        .enumerate()
        .map(|(i, ok)| PlannedSlot {
            plan_ref: format!("p-{task}-{arm}-{i}"),
            task_id: task.into(),
            split_label: "held_out".into(),
            stratum: "private_held_out".into(),
            arm_id: arm.into(),
            replicate_index: i as u64,
            subjects: vec![sub(&format!("r-{task}-{arm}-{i}"), *ok)],
        })
        .collect()
}

/// The EX-02 finite manifest: two tasks × two arms × five repetitions.
/// Task 1: A = 1/5, B = 4/5. Task 2: A = 3/5, B = 2/5.
fn ex02_pop() -> PlannedPopulation {
    let mut slots = Vec::new();
    let bits = |c: usize, n: usize| -> Vec<Option<bool>> { (0..n).map(|i| Some(i < c)).collect() };
    slots.extend(arm_slots("t1", "A", &bits(1, 5)));
    slots.extend(arm_slots("t1", "B", &bits(4, 5)));
    slots.extend(arm_slots("t2", "A", &bits(3, 5)));
    slots.extend(arm_slots("t2", "B", &bits(2, 5)));
    assemble(PopulationInput {
        slots,
        required: BTreeSet::new(),
        pins: BTreeMap::new(),
    })
    .expect("ex-02 assembles")
}

fn decl(prior: (u64, u64), prior_name: &str) -> ContrastDecl {
    ContrastDecl {
        population: PopulationKind::Canonical,
        test_arm: "B".into(),
        control_arm: "A".into(),
        stratum: None,
        prior,
        prior_name: prior_name.into(),
        dependence: None,
        target: None,
        use_as: None,
        bounds: BoundsDecl::Unit,
        level_ppm: LEVEL_PPM,
        seed: 0,
    }
}

// AC-1 — EX-02 known answers: observed equal-task contrast .20 and the
// analytic posterior mean 1/7; the 95% equal-tail credible interval is
// labelled and nondegenerate; the doubling record is complete.
#[test]
fn ex02_known_answers() {
    let pop = ex02_pop();
    let out = finite_manifest_contrast(&pop, &decl((1, 1), "beta{1,1}")).unwrap();
    // ((4/5 − 1/5) + (2/5 − 3/5)) / 2 = (.6 − .2) / 2 = .20
    assert_eq!(out.observed_contrast_ppm, 200_000);
    // ((5/7 − 2/7) + (3/7 − 4/7)) / 2 = (3/7 − 1/7)/2 = 1/7 ≈ .142857
    assert_eq!(out.posterior_mean_ppm, 142_857);
    assert!(out.interval.lo < out.interval.hi);
    assert_eq!(out.draws_requested, BASE_DRAWS);
    assert_eq!(out.draws_used, CAP_DRAWS);
    assert_eq!(out.doubling.len(), 4); // 8k → 16k → 32k → 64k
    assert_eq!(out.doubling[0].draws, 8_000);
    assert_eq!(out.doubling[3].draws, 64_000);
    assert!(out.numerical_error_ppm >= 0);
    assert_eq!(out.tol_ppm, 1);
    assert!(!out.sensitivity);
    assert_eq!(
        out.metadata.interval,
        ("credible".to_string(), Some(950_000))
    );
    assert_eq!(out.metadata.prior.as_deref(), Some("beta{1,1}"));
    // The persisted payload carries the dossier members.
    let j = out.to_json();
    assert_eq!(
        j.get("interval")
            .and_then(|i| i.get("kind"))
            .and_then(|k| k.as_str()),
        Some("credible")
    );
    assert_eq!(
        j.get("observed_contrast_ppm").and_then(|v| v.as_int()),
        Some(200_000)
    );
    assert_eq!(
        j.get("draws")
            .and_then(|d| d.get("requested"))
            .and_then(|v| v.as_int()),
        Some(8_000)
    );
    assert_eq!(
        j.get("weights_ref").and_then(|w| w.as_str()),
        Some("equal_task:w_t=1/T")
    );
}

// AC-1 — deterministic recomputation: identical inputs give
// byte-identical draws/intervals; a changed valid input changes the body.
#[test]
fn deterministic_and_input_sensitive() {
    let pop = ex02_pop();
    let a = finite_manifest_contrast(&pop, &decl((1, 1), "beta{1,1}")).unwrap();
    let b = finite_manifest_contrast(&pop, &decl((1, 1), "beta{1,1}")).unwrap();
    assert_eq!(
        a.to_json().to_canonical_string(),
        b.to_json().to_canonical_string()
    );
    // A changed valid input (the declared seed) changes the drawn body.
    let mut d2 = decl((1, 1), "beta{1,1}");
    d2.seed = 7;
    let c = finite_manifest_contrast(&pop, &d2).unwrap();
    assert_ne!(
        a.to_json().to_canonical_string(),
        c.to_json().to_canonical_string()
    );
    // The Beta{2,2} sensitivity computes, labelled exploratory, and
    // never collides with the registered prior's body.
    let s = finite_manifest_contrast(&pop, &decl((2, 2), "beta{2,2}")).unwrap();
    assert!(s.sensitivity);
    assert_eq!(
        s.to_json()
            .get("prior")
            .and_then(|p| p.get("label"))
            .and_then(|l| l.as_str()),
        Some("predeclared_exploratory")
    );
    assert_ne!(
        s.to_json().to_canonical_string(),
        a.to_json().to_canonical_string()
    );
}

// AC-2 — constant binary samples retain nonzero uncertainty: an
// all-success/all-failure pair of cells still yields a proper posterior.
#[test]
fn constant_samples_keep_nonzero_uncertainty() {
    let bits = |c: usize, n: usize| -> Vec<Option<bool>> { (0..n).map(|i| Some(i < c)).collect() };
    let mut slots = Vec::new();
    slots.extend(arm_slots("t1", "A", &bits(0, 5)));
    slots.extend(arm_slots("t1", "B", &bits(5, 5)));
    slots.extend(arm_slots("t2", "A", &bits(0, 5)));
    slots.extend(arm_slots("t2", "B", &bits(5, 5)));
    let pop = assemble(PopulationInput {
        slots,
        required: BTreeSet::new(),
        pins: BTreeMap::new(),
    })
    .unwrap();
    let out = finite_manifest_contrast(&pop, &decl((1, 1), "beta{1,1}")).unwrap();
    assert!(
        out.interval.lo < out.interval.hi,
        "constant cells still carry posterior width"
    );
    assert!(out.interval.hi - out.interval.lo > 100_000);
    assert_eq!(out.observed_contrast_ppm, 1_000_000);
    // Posterior mean: ((6/7 − 1/7) + (6/7 − 1/7))/2 = 5/7 ≈ .714286
    assert_eq!(out.posterior_mean_ppm, 714_286);
}

// AC-2 — the closed refusal set, one variant per clause.
#[test]
fn refusal_set() {
    let pop = ex02_pop();
    let cases: Vec<(fn(&mut ContrastDecl), &str)> = vec![
        (|d| d.prior = (3, 3), "unsupported_prior"),
        (|d| d.target = Some("pooled".into()), "pooled"),
        (|d| d.target = Some("hierarchical".into()), "pooled"),
        (|d| d.dependence = Some("correlated".into()), "dependence"),
        (|d| d.use_as = Some("tost".into()), "tost"),
        (|d| d.use_as = Some("equivalence".into()), "tost"),
        (|d| d.level_ppm = 900_000, "level"),
        (|d| d.level_ppm = 0, "nonfinite"),
        (
            |d| d.bounds = BoundsDecl::Unbounded { detail: "x".into() },
            "unbounded",
        ),
        (
            |d| d.bounds = BoundsDecl::RightCensored { detail: "x".into() },
            "censored",
        ),
        (
            |d| {
                d.bounds = BoundsDecl::Bounded {
                    lo_ppm: 1,
                    hi_ppm: 900_000,
                }
            },
            "nonunit",
        ),
    ];
    for (mutate, _name) in cases {
        let mut d = decl((1, 1), "beta{1,1}");
        mutate(&mut d);
        let e = finite_manifest_contrast(&pop, &d).expect_err("must refuse");
        assert!(matches!(
            e,
            ContrastError::UnsupportedPrior { .. }
                | ContrastError::PooledSubstitute { .. }
                | ContrastError::UnsupportedDependence { .. }
                | ContrastError::CredibleAsTost { .. }
                | ContrastError::UnsupportedLevel { .. }
                | ContrastError::NonFiniteInput { .. }
                | ContrastError::Population(_)
        ));
    }
}

// AC-2 — missing/short/unresolved cells and one-task universes refuse.
#[test]
fn cell_refusals() {
    let bits = |c: usize, n: usize| -> Vec<Option<bool>> { (0..n).map(|i| Some(i < c)).collect() };
    // One task only.
    let mut slots = Vec::new();
    slots.extend(arm_slots("t1", "A", &bits(1, 5)));
    slots.extend(arm_slots("t1", "B", &bits(4, 5)));
    let pop = assemble(PopulationInput {
        slots,
        required: BTreeSet::new(),
        pins: BTreeMap::new(),
    })
    .unwrap();
    assert!(matches!(
        finite_manifest_contrast(&pop, &decl((1, 1), "beta{1,1}")),
        Err(ContrastError::OneTask)
    ));
    // Short cell: n = 3 < 5.
    let mut slots = Vec::new();
    slots.extend(arm_slots("t1", "A", &bits(1, 3)));
    slots.extend(arm_slots("t1", "B", &bits(4, 5)));
    slots.extend(arm_slots("t2", "A", &bits(3, 5)));
    slots.extend(arm_slots("t2", "B", &bits(2, 5)));
    let pop = assemble(PopulationInput {
        slots,
        required: BTreeSet::new(),
        pins: BTreeMap::new(),
    })
    .unwrap();
    assert!(matches!(
        finite_manifest_contrast(&pop, &decl((1, 1), "beta{1,1}")),
        Err(ContrastError::ShortCell { .. })
    ));
    // Missing cell: task 2 has no B slots (unpaired).
    let mut slots = Vec::new();
    slots.extend(arm_slots("t1", "A", &bits(1, 5)));
    slots.extend(arm_slots("t1", "B", &bits(4, 5)));
    slots.extend(arm_slots("t2", "A", &bits(3, 5)));
    let pop = assemble(PopulationInput {
        slots,
        required: BTreeSet::new(),
        pins: BTreeMap::new(),
    })
    .unwrap();
    assert!(matches!(
        finite_manifest_contrast(&pop, &decl((1, 1), "beta{1,1}")),
        Err(ContrastError::Population(_))
    ));
    // Unresolved cell: one not-run member.
    let mut slots = Vec::new();
    slots.extend(arm_slots("t1", "A", &bits(1, 5)));
    slots.extend(arm_slots("t1", "B", &bits(4, 5)));
    slots.extend(arm_slots("t2", "A", &bits(3, 5)));
    slots.extend(arm_slots(
        "t2",
        "B",
        &[Some(true), Some(false), Some(false), Some(true), None],
    ));
    let pop = assemble(PopulationInput {
        slots,
        required: BTreeSet::new(),
        pins: BTreeMap::new(),
    })
    .unwrap();
    assert!(matches!(
        finite_manifest_contrast(&pop, &decl((1, 1), "beta{1,1}")),
        Err(ContrastError::UnresolvedCell { .. })
    ));
}

// AC-2 — multi-stratum universes refuse without a declared stratum;
// a declared stratum computes within it.
#[test]
fn strata_never_silently_pooled() {
    let bits = |c: usize, n: usize| -> Vec<Option<bool>> { (0..n).map(|i| Some(i < c)).collect() };
    let mut slots = Vec::new();
    slots.extend(arm_slots("t1", "A", &bits(1, 5)));
    slots.extend(arm_slots("t1", "B", &bits(4, 5)));
    slots.extend(arm_slots("t2", "A", &bits(3, 5)));
    slots.extend(arm_slots("t2", "B", &bits(2, 5)));
    // Retag task 2 into a second stratum.
    for s in &mut slots {
        if s.task_id == "t2" {
            s.stratum = "public".into();
        }
    }
    let pop = assemble(PopulationInput {
        slots,
        required: BTreeSet::new(),
        pins: BTreeMap::new(),
    })
    .unwrap();
    assert!(matches!(
        finite_manifest_contrast(&pop, &decl((1, 1), "beta{1,1}")),
        Err(ContrastError::StrataSelectionRequired { .. })
    ));
    // Declaring the first stratum leaves a one-task universe → OneTask.
    let mut d = decl((1, 1), "beta{1,1}");
    d.stratum = Some("private_held_out".into());
    assert!(matches!(
        finite_manifest_contrast(&pop, &d),
        Err(ContrastError::OneTask)
    ));
}

// AC-2 — parse_prior_name: the admitted spellings decode; pooled and
// unknown names do not (the caller maps them to typed refusals).
#[test]
fn prior_name_parsing() {
    assert_eq!(parse_prior_name("beta{1,1}"), Some((1, 1)));
    assert_eq!(parse_prior_name("beta{2,2}"), Some((2, 2)));
    assert_eq!(parse_prior_name("beta{3,7}"), Some((3, 7)));
    assert_eq!(parse_prior_name("pooled"), None);
    assert_eq!(parse_prior_name("beta(1,1)"), None);
    assert_eq!(parse_prior_name(""), None);
}

// The kind spelling is the admitted dossier member name.
#[test]
fn admitted_kind_spelling() {
    assert_eq!(CONTRAST_KIND, "finite_manifest_contrast");
}
