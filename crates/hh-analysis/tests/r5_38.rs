//! R5.38 — the routed kind battery: `AnalysisSpec.kind =
//! "directional_kl_equivalence"` through `analyze` →
//! `analyze_and_record` → the persisted `analysis_report_body/1`
//! (REQ-D-304/305/306/307 primary; Packet F / PADR-R5-6*). Fails-
//! without: the kind arm, `directional_kl` module,
//! `IntervalMethod::KlDirectional` and `AnalysisError::KlEquivalence`
//! do not exist at the R5.37 base — this file compiles red there.

use std::collections::BTreeMap;

use hharness_analysis::{analyze, analyze_and_record, AnalysisError, AnalysisInput};
use hharness_eval::directional_kl as kl;
use hharness_eval::stats::{kl_directional_bound, PPM};
use hharness_experiment::docs::LabDocs;
use hharness_lab::analysis::{AnalysisSpec, QuerySpec};
use hharness_ontology::eval::{EstimatorSelection, IntervalMethod, PreRegistration};
use hharness_results::watermark::WatermarkSet;
use hharness_wire::Json;

fn tmp(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-r5-38-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// `N = T·R` declared binary pairs: `cp` W+ (`h=1,o=0`), `cm` W−,
/// `u` uncertain (`h` absent), the rest concordant.
fn binary_pairs(tasks: u32, repeats: u32, cp: u32, cm: u32, u: u32) -> Json {
    let mut pairs = Vec::new();
    let mut k = 0u32;
    for t in 0..tasks {
        for r in 0..repeats {
            let (h, o) = if k < cp {
                (Json::Int(PPM), Json::Int(0))
            } else if k < cp + cm {
                (Json::Int(0), Json::Int(PPM))
            } else if k < cp + cm + u {
                (Json::Null, Json::Int(PPM))
            } else {
                (Json::Int(PPM), Json::Int(PPM))
            };
            pairs.push(Json::obj([
                ("task_id", Json::str(&format!("t{:02}", t + 1))),
                ("repeat", Json::Int(r as i64)),
                ("pair_id", Json::str(&format!("pair-{k}"))),
                ("subject", Json::str(&format!("subj-{k}"))),
                ("phase", Json::str("parity")),
                ("stratum", Json::str("held_out")),
                ("h", h),
                ("o", o),
            ]));
            k += 1;
        }
    }
    Json::Arr(pairs)
}

fn binary_endpoint(name: &str, tasks: u32, repeats: u32, cp: u32, cm: u32, u: u32) -> Json {
    Json::obj([
        ("endpoint", Json::str(name)),
        ("stratum", Json::str("held_out")),
        ("required", Json::Bool(true)),
        ("kind", Json::str("binary")),
        ("tasks", Json::Int(tasks as i64)),
        ("repeats", Json::Int(repeats as i64)),
        ("pairs", binary_pairs(tasks, repeats, cp, cm, u)),
    ])
}

/// Continuous pairs `(h,o)` cycling through a fixed table.
fn value_pairs(tasks: u32, repeats: u32, vals: &[(i64, i64)]) -> Json {
    let mut pairs = Vec::new();
    let mut k = 0usize;
    for t in 0..tasks {
        for r in 0..repeats {
            let (h, o) = vals[k % vals.len()];
            pairs.push(Json::obj([
                ("task_id", Json::str(&format!("t{:02}", t + 1))),
                ("repeat", Json::Int(r as i64)),
                ("pair_id", Json::str(&format!("pair-{k}"))),
                ("subject", Json::str(&format!("subj-{k}"))),
                ("phase", Json::str("parity")),
                ("stratum", Json::str("held_out")),
                ("h", Json::Int(h)),
                ("o", Json::Int(o)),
            ]));
            k += 1;
        }
    }
    Json::Arr(pairs)
}

fn prereg(margins: &[(&str, i64, &str)]) -> PreRegistration {
    PreRegistration {
        registered_at: 1,
        hypothesis: "h".into(),
        primary_metrics: vec![],
        equivalence_margin: Some(Json::obj([(
            "margins",
            Json::Arr(
                margins
                    .iter()
                    .map(|(d, m, k)| {
                        Json::obj([
                            ("dimension", Json::str(*d)),
                            ("margin_ppm", Json::Int(*m)),
                            ("kind", Json::str(*k)),
                        ])
                    })
                    .collect(),
            ),
        )])),
        min_n: 0,
        analysis_plan_ref: "plan".into(),
        task_split_hash: "sha256:x".into(),
        interactions: vec![],
        dead_weight_purpose: false,
    }
}

fn spec(filters: Json) -> AnalysisSpec {
    let mut s = AnalysisSpec {
        spec_id: String::new(),
        kind: kl::KIND.into(),
        query: QuerySpec {
            metrics: vec![],
            filters: Some(filters),
            grain: None,
        },
        spec_ref: Some("exp-spec".into()),
        label: None,
        estimator_selection: EstimatorSelection {
            method: IntervalMethod::KlDirectional,
            selection_rule: "padr-r5-6.kl".into(),
            floors: BTreeMap::new(),
            fallback_chain: vec![],
            substituted: None,
        },
        resample: None,
        outputs: vec!["report".into()],
    };
    s.spec_id = s.spec_id();
    s
}

struct In<'a> {
    manifests: BTreeMap<String, hharness_ledger::manifest::RunManifest>,
    facts: BTreeMap<String, hharness_eval::facts::LedgerFacts>,
    watermarks: WatermarkSet,
    prereg: &'a PreRegistration,
}

impl<'a> In<'a> {
    fn new(prereg: &'a PreRegistration) -> Self {
        In {
            manifests: BTreeMap::new(),
            facts: BTreeMap::new(),
            watermarks: WatermarkSet::new(),
            prereg,
        }
    }
    fn input(&self) -> AnalysisInput<'_> {
        AnalysisInput {
            rows: &[],
            manifests: &self.manifests,
            facts: &self.facts,
            tasks: &[],
            suites: &[],
            design: None,
            arm_specs: &[],
            declarations: &[],
            pre_registration: Some(self.prereg),
            watermark_set: &self.watermarks,
            metric_registry_version: "test-registry",
            price_table_version: "test-pricing",
            confidence_ppm: 950_000,
            resampling_draws: 2_000,
            seed: 0,
            post_amendment: false,
            view: None,
        }
    }
}

fn ep<'a>(body: &'a Json, name: &str) -> Json {
    body.get(kl::KIND)
        .and_then(|k| k.get("endpoints"))
        .and_then(|e| match e {
            Json::Arr(a) => a
                .iter()
                .find(|x| x.get("endpoint").and_then(Json::as_str) == Some(name))
                .cloned(),
            _ => None,
        })
        .expect("endpoint member")
}

fn verdict_of(e: &Json) -> &str {
    e.get("equivalence_verdict")
        .and_then(|v| v.get("verdict"))
        .and_then(Json::as_str)
        .unwrap()
}

fn interval_of<'a>(e: &'a Json, member: &str) -> (i64, i64) {
    let i = e
        .get("equivalence_verdict")
        .and_then(|v| v.get(member))
        .unwrap();
    (
        i.get("lo_ppm").and_then(Json::as_int).unwrap(),
        i.get("hi_ppm").and_then(Json::as_int).unwrap(),
    )
}

// AC-1 — known-answer endpoints, monotonicity, N10/30/150 widths
// (spec §4.5 pins), continuous x=0/x=1 conventions.
#[test]
fn kl_known_answers() {
    // Zero discordance: U = 1 − η^(1/N) at η = .025.
    let cases = [(10, 308_497), (30, 115_703), (150, 24_293)];
    for (n, want) in cases {
        let i = kl_directional_bound(0, n, 25_000).unwrap();
        assert_eq!(i.lo, 0, "x=0 L convention n={n}");
        assert!((i.hi - want).abs() <= 2, "n={n}: hi={} want≈{want}", i.hi);
        // x=1 mirror: L = η^(1/N), U = 1.
        let j = kl_directional_bound(PPM, n, 25_000).unwrap();
        assert!((j.lo - (PPM - want)).abs() <= 2, "n={n} lo={}", j.lo);
        assert_eq!(j.hi, PPM, "x=1 U convention");
    }
    // Equal directional counts at N=150: half-widths {1,3,8,38}.
    let widths = [(1, 42_944), (3, 65_355), (8, 100_601), (38, 190_899)];
    for (c, want) in widths {
        let x = (c * PPM) / 150;
        let i = kl_directional_bound(x, 150, 25_000).unwrap();
        let hw = i.hi - i.lo;
        assert!((hw - want).abs() <= 3, "c={c}: hw={hw} want≈{want}");
    }
    // Monotonicity in x for both endpoints.
    let mut prev = kl_directional_bound(0, 150, 25_000).unwrap();
    for x in (50_000..=PPM).step_by(50_000) {
        let i = kl_directional_bound(x, 150, 25_000).unwrap();
        assert!(i.lo >= prev.lo && i.hi >= prev.hi, "monotone at x={x}");
        prev = i;
    }
    // Domain refusals.
    assert!(kl_directional_bound(PPM + 1, 10, 25_000).is_none());
    assert!(kl_directional_bound(0, 0, 25_000).is_none());
    assert!(kl_directional_bound(0, 10, 0).is_none());
}

// AC-1 — routed binary endpoint: strict TOST inside ±ε ⇒ equivalent;
// touching the margin stays inconclusive; the .90/.95 members and
// the planned-N envelope are report facts.
#[test]
fn routed_binary_equivalent_touching() {
    let pr = prereg(&[("task_success", 50_000, "absolute")]);
    // N = 10·15 = 150, zero discordance → [−24_293, 24_293] ppm.
    let f = Json::obj([(
        "endpoints",
        Json::Arr(vec![binary_endpoint("task_success", 10, 15, 0, 0, 0)]),
    )]);
    let body = analyze(&spec(f.clone()), &In::new(&pr).input())
        .unwrap()
        .body;
    let b = body.get(kl::KIND).unwrap();
    assert_eq!(
        b.get("schema").and_then(Json::as_str),
        Some("directional-kl/1")
    );
    let e = ep(&body, "task_success");
    assert_eq!(verdict_of(&e), "equivalent");
    let (lo, hi) = interval_of(&e, "reporting");
    let (tlo, thi) = (
        e.get("equivalence_verdict")
            .and_then(|v| v.get("lo_ppm"))
            .and_then(Json::as_int)
            .unwrap(),
        e.get("equivalence_verdict")
            .and_then(|v| v.get("hi_ppm"))
            .and_then(Json::as_int)
            .unwrap(),
    );
    assert!((thi - 24_293).abs() <= 2 && (tlo + 24_293).abs() <= 2);
    assert!(lo < tlo && hi > thi, "reporting wider than TOST");
    assert_eq!(
        e.get("completion_envelope")
            .and_then(|c| c.get("n"))
            .and_then(Json::as_int),
        Some(150)
    );
    assert_eq!(
        e.get("metadata").and_then(|_| Some(())),
        None,
        "metadata sits on the section, not the endpoint"
    );
    let md = b.get("metadata").unwrap();
    assert_eq!(
        md.get("interval")
            .and_then(|i| i.get("kind"))
            .and_then(Json::as_str),
        Some("concentration_bound")
    );
    // Touching: ε = the TOST hi exactly → inconclusive, never
    // equivalent (strict bound after outward rounding).
    let pr_touch = prereg(&[("task_success", 24_293, "absolute")]);
    let body = analyze(&spec(f.clone()), &In::new(&pr_touch).input())
        .unwrap()
        .body;
    assert_eq!(verdict_of(&ep(&body, "task_success")), "inconclusive");
    // Narrower N=30 design, ε just below the .115703 width ⇒
    // inconclusive; ε above ⇒ equivalent.
    let f30 = Json::obj([(
        "endpoints",
        Json::Arr(vec![binary_endpoint("task_success", 10, 3, 0, 0, 0)]),
    )]);
    let pr_wide = prereg(&[("task_success", 120_000, "absolute")]);
    let body = analyze(&spec(f30.clone()), &In::new(&pr_wide).input())
        .unwrap()
        .body;
    assert_eq!(verdict_of(&ep(&body, "task_success")), "equivalent");
    let pr_narrow = prereg(&[("task_success", 100_000, "absolute")]);
    let body = analyze(&spec(f30), &In::new(&pr_narrow).input())
        .unwrap()
        .body;
    assert_eq!(verdict_of(&ep(&body, "task_success")), "inconclusive");
}

// AC-1/AC-2 — the .95 reporting interval wholly outside the band is
// required for an inferential not_equivalent; a disjoint .90 alone
// stays descriptive.
#[test]
fn s11_negative_requires_reporting_interval() {
    // N=150, c+=25 c−=0: TOST lo_Δ ≈ +71.1k ppm, reporting lo_Δ ≈
    // +61.4k (reference-computed). ε=65k → disjoint .90 but the .95
    // reporting interval straddles → inconclusive + descriptive note.
    let pr = prereg(&[("task_success", 65_000, "absolute")]);
    let f = Json::obj([(
        "endpoints",
        Json::Arr(vec![binary_endpoint("task_success", 10, 15, 25, 0, 0)]),
    )]);
    let body = analyze(&spec(f.clone()), &In::new(&pr).input())
        .unwrap()
        .body;
    let e = ep(&body, "task_success");
    assert_eq!(verdict_of(&e), "inconclusive");
    assert!(e
        .get("note")
        .and_then(Json::as_str)
        .unwrap_or("")
        .contains("descriptive"));
    // Same counts, ε=50k: the .95 reporting interval sits wholly
    // above the band → inferential not_equivalent.
    let pr2 = prereg(&[("task_success", 50_000, "absolute")]);
    let body = analyze(&spec(f), &In::new(&pr2).input()).unwrap().body;
    assert_eq!(verdict_of(&ep(&body, "task_success")), "not_equivalent");
}

// AC-1 — uncertain pairs widen the completion envelope at preserved
// planned N (never missing-at-random).
#[test]
fn completion_envelope_preserves_n() {
    let pr = prereg(&[("task_success", 50_000, "absolute")]);
    // N=150, c+=c−=0, u=10.
    let f = Json::obj([(
        "endpoints",
        Json::Arr(vec![binary_endpoint("task_success", 10, 15, 0, 0, 10)]),
    )]);
    let body = analyze(&spec(f), &In::new(&pr).input()).unwrap().body;
    let e = ep(&body, "task_success");
    let (lo, hi) = (
        e.get("equivalence_verdict")
            .and_then(|v| v.get("lo_ppm"))
            .and_then(Json::as_int)
            .unwrap(),
        e.get("equivalence_verdict")
            .and_then(|v| v.get("hi_ppm"))
            .and_then(Json::as_int)
            .unwrap(),
    );
    // Envelope hi inverts (0+10)/150 = .0667 — strictly wider than
    // the zero-uncertainty 24_293 endpoint; planned N still 150.
    assert!(hi > 24_293, "envelope widened: hi={hi}");
    assert!(lo < -24_293, "envelope widened: lo={lo}");
    // The envelope counts uncertain pairs adversely — an inconclusive
    // verdict here is the conservative bound working (uncertain never
    // helps the claim).
    assert_eq!(verdict_of(&e), "inconclusive");
    assert_eq!(
        e.get("completion_envelope")
            .and_then(|c| c.get("u"))
            .and_then(Json::as_int),
        Some(10)
    );
    assert_eq!(
        e.get("completion_envelope")
            .and_then(|c| c.get("n"))
            .and_then(Json::as_int),
        Some(150)
    );
    assert_eq!(verdict_of(&e), "inconclusive");
}

// AC-1 — bounded absolute difference: normalized-mean construction
// transforms back to declared units; sign reversal negates.
#[test]
fn bounded_absolute_units_and_sign_reversal() {
    let pr = prereg(&[("efficiency.cost", 60_000, "absolute")]);
    // h/o in ppm-of-unit, range [0, 200_000] → D ∈ [−200k, 200k].
    let ep_bounded = |name: &str, vals: &[(i64, i64)]| {
        Json::obj([
            ("endpoint", Json::str(name)),
            ("stratum", Json::str("held_out")),
            ("required", Json::Bool(true)),
            ("kind", Json::str("bounded")),
            ("range_h", Json::Arr(vec![Json::Int(0), Json::Int(200_000)])),
            ("range_o", Json::Arr(vec![Json::Int(0), Json::Int(200_000)])),
            ("tasks", Json::Int(10)),
            ("repeats", Json::Int(15)),
            ("pairs", value_pairs(10, 15, vals)),
        ])
    };
    // Centered: h−o = 0 → D mean 0 → equivalent at ε=60k (N=150 ⇒
    // the normalized half-width ≈ .11 of the 400k range).
    let vals = [(150_000i64, 150_000i64); 2];
    let f = Json::obj([(
        "endpoints",
        Json::Arr(vec![ep_bounded("efficiency.cost", &vals)]),
    )]);
    let body = analyze(&spec(f), &In::new(&pr).input()).unwrap().body;
    let e = ep(&body, "efficiency.cost");
    assert_eq!(verdict_of(&e), "equivalent");
    let (lo, hi) = (
        e.get("equivalence_verdict")
            .and_then(|v| v.get("lo_ppm"))
            .and_then(Json::as_int)
            .unwrap(),
        e.get("equivalence_verdict")
            .and_then(|v| v.get("hi_ppm"))
            .and_then(Json::as_int)
            .unwrap(),
    );
    assert!(lo < 0 && hi > 0 && hi < 60_000 && lo > -60_000);
    // Sign reversal: h−o = +40k vs −40k swap negates the interval.
    let pos = [(160_000i64, 120_000i64); 2];
    let neg = [(120_000i64, 160_000i64); 2];
    let f2 = Json::obj([
        (
            "endpoints",
            Json::Arr(vec![
                ep_bounded("efficiency.cost", &pos),
                Json::obj([
                    ("endpoint", Json::str("efficiency.cost2")),
                    ("stratum", Json::str("held_out")),
                    ("required", Json::Bool(false)),
                    ("kind", Json::str("bounded")),
                    ("range_h", Json::Arr(vec![Json::Int(0), Json::Int(200_000)])),
                    ("range_o", Json::Arr(vec![Json::Int(0), Json::Int(200_000)])),
                    ("tasks", Json::Int(10)),
                    ("repeats", Json::Int(15)),
                    ("pairs", value_pairs(10, 15, &neg)),
                ]),
            ]),
        ),
        ("selected_claims", Json::Int(2)),
    ]);
    let pr2 = prereg(&[
        ("efficiency.cost", 60_000, "absolute"),
        ("efficiency.cost2", 60_000, "absolute"),
    ]);
    let body = analyze(&spec(f2), &In::new(&pr2).input()).unwrap().body;
    let epos = ep(&body, "efficiency.cost");
    let eneg = ep(&body, "efficiency.cost2");
    let (plo, phi) = (
        epos.get("equivalence_verdict")
            .and_then(|v| v.get("lo_ppm"))
            .and_then(Json::as_int)
            .unwrap(),
        epos.get("equivalence_verdict")
            .and_then(|v| v.get("hi_ppm"))
            .and_then(Json::as_int)
            .unwrap(),
    );
    let (nlo, nhi) = (
        eneg.get("equivalence_verdict")
            .and_then(|v| v.get("lo_ppm"))
            .and_then(Json::as_int)
            .unwrap(),
        eneg.get("equivalence_verdict")
            .and_then(|v| v.get("hi_ppm"))
            .and_then(Json::as_int)
            .unwrap(),
    );
    assert!((plo + nhi).abs() <= 2 && (phi + nlo).abs() <= 2);
}

// AC-1/AC-2 — relative-mean construction: scale-invariant ratio
// test; zero reference without the frozen convention refuses; the
// `absolute` convention runs the registered fallback margin.
#[test]
fn relative_mean_and_zero_reference() {
    // ρ = .20; h/o ranges positive. h ≈ o → equivalent.
    let pr = prereg(&[("efficiency.tokens", 200_000, "relative")]);
    let vals = [(500_000i64, 480_000i64); 2];
    let ep_rel = |name: &str, zr: Option<&str>, o_range: (i64, i64)| {
        Json::obj([
            ("endpoint", Json::str(name)),
            ("stratum", Json::str("held_out")),
            ("required", Json::Bool(true)),
            ("kind", Json::str("relative")),
            (
                "range_h",
                Json::Arr(vec![Json::Int(400_000), Json::Int(600_000)]),
            ),
            (
                "range_o",
                Json::Arr(vec![Json::Int(o_range.0), Json::Int(o_range.1)]),
            ),
            ("zero_reference", zr.map(Json::str).unwrap_or(Json::Null)),
            ("tasks", Json::Int(10)),
            ("repeats", Json::Int(15)),
            ("pairs", value_pairs(10, 15, &vals)),
        ])
    };
    let f = Json::obj([(
        "endpoints",
        Json::Arr(vec![ep_rel("efficiency.tokens", None, (400_000, 600_000))]),
    )]);
    let body = analyze(&spec(f), &In::new(&pr).input()).unwrap().body;
    let e = ep(&body, "efficiency.tokens");
    assert_eq!(verdict_of(&e), "equivalent");
    // Zero-containing reference without convention → ZeroReference.
    let fz = Json::obj([(
        "endpoints",
        Json::Arr(vec![ep_rel("efficiency.tokens", None, (0, 1_000_000))]),
    )]);
    assert!(matches!(
        analyze(&spec(fz), &In::new(&pr).input()),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::ZeroReference { .. }
        ))
    ));
    // Frozen `absolute` convention + registered `:absolute` margin →
    // the fallback bounded-absolute leg runs.
    let pr_abs = prereg(&[
        ("efficiency.tokens", 200_000, "relative"),
        ("efficiency.tokens:absolute", 200_000, "absolute"),
    ]);
    let fa = Json::obj([(
        "endpoints",
        Json::Arr(vec![ep_rel(
            "efficiency.tokens",
            Some("absolute"),
            (0, 1_000_000),
        )]),
    )]);
    let body = analyze(&spec(fa), &In::new(&pr_abs).input()).unwrap().body;
    let e = ep(&body, "efficiency.tokens");
    assert_eq!(verdict_of(&e), "equivalent");
    assert_eq!(
        e.get("allocated")
            .and_then(|a| a.get("leg"))
            .and_then(Json::as_str),
        Some("absolute_fallback")
    );
}

// AC-1/AC-2 — the IUT conjunction: all-equivalent members pass;
// an unknown member is listed and blocks (never removed); a required
// endpoint absent from the member set refuses.
#[test]
fn iut_conjunction_unknown_members() {
    let pr = prereg(&[("ep_a", 50_000, "absolute"), ("ep_b", 50_000, "absolute")]);
    let endpoints = || {
        Json::Arr(vec![
            binary_endpoint("ep_a", 10, 15, 0, 0, 0),
            binary_endpoint("ep_b", 10, 15, 0, 0, 0),
        ])
    };
    // Both members equivalent → the conjunction is equivalent.
    let f = Json::obj([
        ("endpoints", endpoints()),
        (
            "conjunction",
            Json::obj([
                ("claim_ref", Json::str("claim-all")),
                (
                    "members",
                    Json::Arr(vec![Json::str("ep_a"), Json::str("ep_b")]),
                ),
            ]),
        ),
    ]);
    let body = analyze(&spec(f), &In::new(&pr).input()).unwrap().body;
    let cj = body
        .get(kl::KIND)
        .and_then(|b| b.get("iut_conjunction"))
        .unwrap();
    assert_eq!(cj.get("verdict").and_then(Json::as_str), Some("equivalent"));
    // A member never supplied → unknown_members, inconclusive.
    let f2 = Json::obj([
        ("endpoints", endpoints()),
        (
            "conjunction",
            Json::obj([
                ("claim_ref", Json::str("claim-all")),
                (
                    "members",
                    Json::Arr(vec![
                        Json::str("ep_a"),
                        Json::str("ep_b"),
                        Json::str("ep_c_missing"),
                    ]),
                ),
            ]),
        ),
    ]);
    let body = analyze(&spec(f2), &In::new(&pr).input()).unwrap().body;
    let cj = body
        .get(kl::KIND)
        .and_then(|b| b.get("iut_conjunction"))
        .unwrap();
    assert_eq!(
        cj.get("verdict").and_then(Json::as_str),
        Some("inconclusive")
    );
    assert_eq!(
        cj.get("unknown_members"),
        Some(&Json::Arr(vec![Json::str("ep_c_missing")]))
    );
    // Required endpoint absent from the members → FamilyIdentity.
    let f3 = Json::obj([
        ("endpoints", endpoints()),
        (
            "conjunction",
            Json::obj([
                ("claim_ref", Json::str("claim-all")),
                ("members", Json::Arr(vec![Json::str("ep_a")])),
            ]),
        ),
    ]);
    assert!(matches!(
        analyze(&spec(f3), &In::new(&pr).input()),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::FamilyIdentity { .. }
        ))
    ));
}

// AC-2 — the closed refusal set through the actual report path.
#[test]
fn refusal_table() {
    let pr = prereg(&[("task_success", 50_000, "absolute")]);
    let f = || {
        Json::obj([(
            "endpoints",
            Json::Arr(vec![binary_endpoint("task_success", 10, 15, 0, 0, 0)]),
        )])
    };
    let with = |kv: &[(&str, Json)]| {
        let mut pairs: Vec<(String, Json)> = vec![(
            "endpoints".into(),
            Json::Arr(vec![binary_endpoint("task_success", 10, 15, 0, 0, 0)]),
        )];
        for (k, v) in kv {
            pairs.retain(|(pk, _)| pk != k);
            pairs.push((k.to_string(), v.clone()));
        }
        Json::Obj(pairs.into_iter().collect())
    };
    // Shared-shock dependence.
    assert!(matches!(
        analyze(
            &spec(with(&[("dependence", Json::str("shared_shocks"))])),
            &In::new(&pr).input()
        ),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::UnsupportedDependence { .. }
        ))
    ));
    // Unequal weights.
    assert!(matches!(
        analyze(
            &spec(with(&[("weights", Json::str("unequal"))])),
            &In::new(&pr).input()
        ),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::UnequalDesign { .. }
        ))
    ));
    // Efficacy-Holm fusion — the IUT never suppresses D2.
    assert!(matches!(
        analyze(
            &spec(with(&[("verdict", Json::str("efficacy_holm"))])),
            &In::new(&pr).input()
        ),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::UnsupportedVerdict { .. }
        ))
    ));
    // Unsupported reporting level.
    let holder = In::new(&pr);
    let mut inp = holder.input();
    inp.confidence_ppm = 900_000;
    assert!(matches!(
        analyze(&spec(f()), &inp),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::UnsupportedLevel { .. }
        ))
    ));
    // selected_claims under-declared for two selected endpoints.
    let pr2 = prereg(&[
        ("task_success", 50_000, "absolute"),
        ("ep2", 50_000, "absolute"),
    ]);
    let f_two = Json::obj([
        (
            "endpoints",
            Json::Arr(vec![
                binary_endpoint("task_success", 10, 15, 0, 0, 0),
                binary_endpoint("ep2", 10, 15, 0, 0, 0),
            ]),
        ),
        ("selected_claims", Json::Int(1)),
    ]);
    assert!(matches!(
        analyze(&spec(f_two), &In::new(&pr2).input()),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::FamilyIdentity { .. }
        ))
    ));
    // Missing margin dimension.
    let pr3 = prereg(&[("other", 50_000, "absolute")]);
    assert!(matches!(
        analyze(&spec(f()), &In::new(&pr3).input()),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::MissingMargin { .. }
        ))
    ));
    // Margin kind mismatch (relative margin on a binary endpoint).
    let pr4 = prereg(&[("task_success", 50_000, "relative")]);
    assert!(matches!(
        analyze(&spec(f()), &In::new(&pr4).input()),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::MarginKindMismatch { .. }
        ))
    ));
    // No pre-registration → the margin authority's MissingSpecRef.
    let holder2 = In::new(&pr);
    let mut inp2 = holder2.input();
    inp2.pre_registration = None;
    assert!(matches!(
        analyze(&spec(f()), &inp2),
        Err(AnalysisError::MissingSpecRef { .. })
    ));
    // Wrong declared method → UnknownMethod; the method on a
    // non-computing kind → MetadataOnly.
    let mut wrong = spec(f());
    wrong.estimator_selection.method =
        IntervalMethod::BootstrapPaired(hharness_ontology::eval::BootstrapPairedMethod::Bca);
    assert!(matches!(
        analyze(&wrong, &In::new(&pr).input()),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::UnknownMethod { .. }
        ))
    ));
    let mut meta = spec(f());
    meta.kind = "summarize".into();
    assert!(matches!(
        analyze(&meta, &In::new(&pr).input()),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::MetadataOnly { .. }
        ))
    ));
    // Pair-shape refusals: reused subject, missing keys, bad binary
    // value, unequal repeats, out-of-range continuous value.
    let mut bad = binary_pairs(10, 15, 0, 0, 0);
    if let Json::Arr(a) = &mut bad {
        a[0] = a[1].clone();
    }
    let f_subject = Json::obj([(
        "endpoints",
        Json::Arr(vec![Json::obj([
            ("endpoint", Json::str("task_success")),
            ("stratum", Json::str("held_out")),
            ("required", Json::Bool(true)),
            ("kind", Json::str("binary")),
            ("tasks", Json::Int(10)),
            ("repeats", Json::Int(15)),
            ("pairs", bad),
        ])]),
    )]);
    assert!(matches!(
        analyze(&spec(f_subject), &In::new(&pr).input()),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::ReusedSubject { .. }
                | kl::KlEquivalenceError::PairIdentity { .. }
        ))
    ));
    let mut no_phase = binary_pairs(10, 15, 0, 0, 0);
    if let Json::Arr(a) = &mut no_phase {
        if let Json::Obj(m) = &mut a[0] {
            m.insert("phase".into(), Json::str(""));
        }
    }
    let f_phase = Json::obj([(
        "endpoints",
        Json::Arr(vec![Json::obj([
            ("endpoint", Json::str("task_success")),
            ("stratum", Json::str("held_out")),
            ("required", Json::Bool(true)),
            ("kind", Json::str("binary")),
            ("tasks", Json::Int(10)),
            ("repeats", Json::Int(15)),
            ("pairs", no_phase),
        ])]),
    )]);
    assert!(matches!(
        analyze(&spec(f_phase), &In::new(&pr).input()),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::MissingKey { .. }
        ))
    ));
    // Invalid binary value.
    let mut bad_v = binary_pairs(10, 15, 0, 0, 0);
    if let Json::Arr(a) = &mut bad_v {
        if let Json::Obj(m) = &mut a[0] {
            m.insert("h".into(), Json::Int(500));
        }
    }
    let f_v = Json::obj([(
        "endpoints",
        Json::Arr(vec![Json::obj([
            ("endpoint", Json::str("task_success")),
            ("stratum", Json::str("held_out")),
            ("required", Json::Bool(true)),
            ("kind", Json::str("binary")),
            ("tasks", Json::Int(10)),
            ("repeats", Json::Int(15)),
            ("pairs", bad_v),
        ])]),
    )]);
    assert!(matches!(
        analyze(&spec(f_v), &In::new(&pr).input()),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::InvalidCounts { .. }
        ))
    ));
    // Unequal repeats: a task carrying the wrong count.
    let f_unequal = Json::obj([(
        "endpoints",
        Json::Arr(vec![Json::obj([
            ("endpoint", Json::str("task_success")),
            ("stratum", Json::str("held_out")),
            ("required", Json::Bool(true)),
            ("kind", Json::str("binary")),
            ("tasks", Json::Int(10)),
            ("repeats", Json::Int(15)),
            // 150 pairs but only 9 tasks × 15 + a duplicate task set
            // — declare 10×15 yet supply a wrong per-task count.
            ("pairs", {
                let mut p = binary_pairs(10, 15, 0, 0, 0);
                if let Json::Arr(a) = &mut p {
                    // Retask one pair → 9 full + 16 on one task.
                    if let Json::Obj(m) = &mut a[0] {
                        m.insert("task_id".into(), Json::str("t99"));
                    }
                }
                p
            }),
        ])]),
    )]);
    assert!(matches!(
        analyze(&spec(f_unequal), &In::new(&pr).input()),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::UnequalDesign { .. }
        ))
    ));
    // Missing range on a bounded endpoint → UnknownRange.
    let f_range = Json::obj([(
        "endpoints",
        Json::Arr(vec![Json::obj([
            ("endpoint", Json::str("task_success")),
            ("stratum", Json::str("held_out")),
            ("required", Json::Bool(true)),
            ("kind", Json::str("bounded")),
            ("tasks", Json::Int(5)),
            ("repeats", Json::Int(4)),
            ("pairs", value_pairs(5, 4, &[(1_000, 1_000); 20])),
        ])]),
    )]);
    assert!(matches!(
        analyze(&spec(f_range), &In::new(&pr).input()),
        Err(AnalysisError::KlEquivalence(
            kl::KlEquivalenceError::UnknownRange { .. }
        ))
    ));
}

// AC-4 — deterministic identity + idempotent persisted report.
#[test]
fn routed_persisted_idempotent() {
    let pr = prereg(&[("task_success", 50_000, "absolute")]);
    let f = Json::obj([(
        "endpoints",
        Json::Arr(vec![binary_endpoint("task_success", 10, 15, 0, 0, 0)]),
    )]);
    let s = spec(f);
    let docs = LabDocs::open(&tmp("persist")).unwrap();
    let r1 = analyze_and_record(&docs, &s, &In::new(&pr).input()).unwrap();
    let r2 = analyze_and_record(&docs, &s, &In::new(&pr).input()).unwrap();
    assert_eq!(r1.report_id, r2.report_id);
    assert_eq!(r1.record.kind.as_deref(), Some(kl::KIND));
    assert!(r1.record.outputs.contains(&r1.report_id));
}
