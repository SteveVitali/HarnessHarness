//! R5.39 — the routed kind battery: `AnalysisSpec.kind =
//! "parity_admission"` through `analyze` → `analyze_and_record` →
//! the persisted `analysis_report_body/1` (REQ-D-004/301/302/303/308
//! primary; Packet E / PADR-R5-5*). Validates *supplied* evidence —
//! never authentic runs. Fails-without: the kind arm, the
//! `parity_admission` module, `IntervalMethod::ParityAdmission` and
//! `AnalysisError::ParityAdmission` do not exist at the R5.38 base —
//! this file compiles red there.

use std::collections::BTreeMap;

use hharness_analysis::{analyze, analyze_and_record, AnalysisError, AnalysisInput};
use hharness_eval::parity_admission as pa;
use hharness_experiment::docs::LabDocs;
use hharness_lab::analysis::{AnalysisSpec, QuerySpec};
use hharness_ontology::eval::{EstimatorSelection, IntervalMethod, PreRegistration};
use hharness_results::watermark::WatermarkSet;
use hharness_wire::Json;

fn tmp(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-r5-39-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn arr_of(j: &Json) -> &Vec<Json> {
    match j {
        Json::Arr(a) => a,
        _ => panic!("expected Json::Arr, got {j:?}"),
    }
}

/// The pinned complete first-attempt synthetic admission fixture —
/// T=4, R=5 (adapter ≥3, S3 ≥5), N=20 authentic pairs alternating
/// declared policies A/B, signed pre-data map (seq 100 < 200),
/// both-policy evidence.
fn pairs() -> Json {
    let mut v = Vec::new();
    for t in 0..4 {
        for r in 0..5 {
            let k = t * 5 + r;
            v.push(Json::obj([
                ("task_id", Json::str(&format!("t{:02}", t + 1))),
                ("repeat", Json::Int(r)),
                ("pair_id", Json::str(&format!("pair-{k}"))),
                ("subject", Json::str(&format!("subj-{k}"))),
                ("phase", Json::str("parity")),
                ("stratum", Json::str("held_out")),
                ("policy", Json::str(if k % 2 == 0 { "A" } else { "B" })),
                ("h_run_ref", Json::str(&format!("run-h-{k}"))),
                ("o_run_ref", Json::str(&format!("run-o-{k}"))),
                ("h_verdict_ref", Json::str(&format!("v-h-{k}"))),
                ("o_verdict_ref", Json::str(&format!("v-o-{k}"))),
                ("replay_ref", Json::str(&format!("replay-{k}"))),
                ("attempt", Json::Int(1)),
                ("seed_requested", Json::Int(42 + k as i64)),
                ("seed_honoured", Json::Int(42 + k as i64)),
                ("h", Json::Int(1_000_000)),
                ("o", Json::Int(1_000_000)),
            ]));
        }
    }
    Json::Arr(v)
}

fn manifest() -> Json {
    Json::obj([
        ("tasks", Json::Int(4)),
        ("repeats", Json::Int(5)),
        ("weights", Json::str("equal")),
        ("phase", Json::str("parity")),
        ("stratum", Json::str("held_out")),
        ("dependence", Json::str("independent_pairs")),
        (
            "independence_rationale_ref",
            Json::str("rationale/design-1"),
        ),
        ("pairs", pairs()),
    ])
}

fn dim_entry(dim: &str, kind: &str, margin: &str, applicability: &str) -> Json {
    let mut e = vec![
        ("dimension", Json::str(dim)),
        ("metric_ref", Json::str(&format!("m/{dim}"))),
        ("estimand", Json::str(&format!("{dim}_estimand"))),
        ("estimand_kind", Json::str(kind)),
        ("population", Json::str("all_planned_pairs")),
        ("unit", Json::str("ppm")),
        ("margin_ref", Json::str(margin)),
        ("applicability", Json::str(applicability)),
        ("evidence_ref", Json::str(&format!("ev/{dim}"))),
    ];
    if matches!(kind, "bounded" | "relative") {
        e.push(("range", Json::Arr(vec![Json::Int(0), Json::Int(1_000_000)])));
    }
    Json::Obj(e.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn dimension_map() -> Json {
    Json::obj([
        ("signature_ref", Json::str("sig/dimmap-1")),
        ("map_seq", Json::Int(100)),
        ("first_outcome_seq", Json::Int(200)),
        (
            "required",
            Json::Arr(vec![Json::str("capability"), Json::str("reliability")]),
        ),
        (
            "entries",
            Json::Arr(vec![
                dim_entry("capability", "binary", "capability", "applicable"),
                dim_entry("reliability", "binary", "reliability", "applicable"),
                dim_entry("efficiency", "bounded", "efficiency", "applicable"),
            ]),
        ),
    ])
}

fn both_policies() -> Json {
    Json::obj([
        ("mode", Json::str("both_policies")),
        (
            "evidence",
            Json::Arr(vec![
                Json::obj([
                    ("policy", Json::str("A")),
                    ("evidence_ref", Json::str("ev-pol-A")),
                ]),
                Json::obj([
                    ("policy", Json::str("B")),
                    ("evidence_ref", Json::str("ev-pol-B")),
                ]),
            ]),
        ),
    ])
}

fn filters() -> Json {
    Json::obj([
        ("participant_ref", Json::str("participant/p1")),
        ("policies", Json::Arr(vec![Json::str("A"), Json::str("B")])),
        (
            "evidence",
            Json::obj([
                ("shape", Json::str("authentic_first_attempt")),
                ("manifest", manifest()),
            ]),
        ),
        ("dimension_map", dimension_map()),
        ("applicability", both_policies()),
        (
            "pins",
            Json::obj([
                ("source_ref", Json::str("src/1")),
                ("verifier_ref", Json::str("ver/1")),
                ("rights_ref", Json::str("rights/1")),
            ]),
        ),
    ])
}

fn prereg() -> PreRegistration {
    prereg_with(100_000)
}

fn prereg_with(cap_margin: i64) -> PreRegistration {
    PreRegistration {
        registered_at: 1,
        hypothesis: "h".into(),
        primary_metrics: vec![],
        equivalence_margin: Some(Json::obj([(
            "margins",
            Json::Arr(vec![
                Json::obj([
                    ("dimension", Json::str("capability")),
                    ("margin_ppm", Json::Int(cap_margin)),
                    ("kind", Json::str("absolute")),
                ]),
                Json::obj([
                    ("dimension", Json::str("reliability")),
                    ("margin_ppm", Json::Int(100_000)),
                    ("kind", Json::str("absolute")),
                ]),
                Json::obj([
                    ("dimension", Json::str("efficiency")),
                    ("margin_ppm", Json::Int(50_000)),
                    ("kind", Json::str("absolute")),
                ]),
            ]),
        )])),
        min_n: 0,
        analysis_plan_ref: "plan".into(),
        task_split_hash: "sha256:x".into(),
        interactions: vec![],
        dead_weight_purpose: false,
    }
}

fn spec(filters: Json, method: IntervalMethod, kind: &str) -> AnalysisSpec {
    let mut s = AnalysisSpec {
        spec_id: String::new(),
        kind: kind.into(),
        query: QuerySpec {
            metrics: vec![],
            filters: Some(filters),
            grain: None,
        },
        spec_ref: Some("exp-spec".into()),
        label: None,
        estimator_selection: EstimatorSelection {
            method,
            selection_rule: "padr-r5-5.admission".into(),
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

fn run(filters: Json) -> Result<Json, AnalysisError> {
    let pr = prereg();
    let s = spec(filters, IntervalMethod::ParityAdmission, pa::KIND);
    let r = analyze(&s, &In::new(&pr).input())?;
    Ok(r.body)
}

fn admission_of(body: &Json) -> &Json {
    body.get(pa::KIND).expect("parity_admission section")
}

fn admission_err(filters: Json) -> pa::AdmissionError {
    match run(filters) {
        Err(AnalysisError::ParityAdmission(e)) => e,
        other => panic!("expected ParityAdmission refusal, got {other:?}"),
    }
}

/// Set `filters[k] = v` on the top-level filter object.
fn with(filters: &Json, k: &str, v: Json) -> Json {
    let mut m = match filters {
        Json::Obj(m) => m.clone(),
        _ => unreachable!(),
    };
    m.insert(k.to_string(), v);
    Json::Obj(m)
}

/// Set `evidence.manifest[k] = v`.
fn with_manifest(filters: &Json, k: &str, v: Json) -> Json {
    let mut m = match filters.get("evidence").and_then(|e| e.get("manifest")) {
        Some(Json::Obj(m)) => m.clone(),
        _ => unreachable!(),
    };
    m.insert(k.to_string(), v);
    with(
        filters,
        "evidence",
        Json::obj([
            ("shape", Json::str("authentic_first_attempt")),
            ("manifest", Json::Obj(m)),
        ]),
    )
}

/// Set `dimension_map[k] = v`.
fn with_map(filters: &Json, k: &str, v: Json) -> Json {
    let mut m = match filters.get("dimension_map") {
        Some(Json::Obj(m)) => m.clone(),
        _ => unreachable!(),
    };
    m.insert(k.to_string(), v);
    with(filters, "dimension_map", Json::Obj(m))
}

// AC-1 — the pinned complete fixture admits end-to-end through the
// routed kind: equal T/R/weights, actual identities/replay/floors,
// signed pre-data map, both-policy evidence.
#[test]
fn admitted_full_fixture() {
    let body = run(filters()).expect("admits");
    let a = admission_of(&body);
    assert_eq!(a.get("admission").and_then(Json::as_str), Some("admitted"));
    assert_eq!(
        a.get("applicability").and_then(Json::as_str),
        Some("both_policies")
    );
    let d = a.get("design").unwrap();
    assert_eq!(d.get("tasks").and_then(Json::as_int), Some(4));
    assert_eq!(d.get("repeats").and_then(Json::as_int), Some(5));
    assert_eq!(d.get("planned_n").and_then(Json::as_int), Some(20));
    let pairs = a.get("admitted_pairs").unwrap();
    assert_eq!(arr_of(pairs).len(), 20);
    let floors: Vec<&str> = arr_of(a.get("floors").unwrap())
        .iter()
        .filter_map(Json::as_str)
        .collect();
    assert!(floors.iter().any(|f| f.contains("adapter>=3")));
    assert!(floors.iter().any(|f| f.contains("s3>=5")));
    let dims = arr_of(a.get("dimensions").unwrap());
    assert_eq!(dims.len(), 3);
    assert!(dims
        .iter()
        .all(|d| d.get("status").and_then(Json::as_str) == Some("mapped")));
    let md = a.get("metadata").unwrap();
    assert_eq!(md.get("estimator").and_then(Json::as_str), Some(pa::KIND));
}

// AC-1 — the admitted pair ids compose unchanged into R5.38's M-E
// `directional_kl_equivalence` kind (the admission route feeds the
// estimator, never edits it).
#[test]
fn admitted_pairs_route_to_kl_unchanged() {
    let body = run(filters()).expect("admits");
    let ids = admission_of(&body).get("admitted_pairs").unwrap();
    assert_eq!(arr_of(ids).len(), 20);
    // The same design/outcomes drive the M-E kind unchanged.
    let kl_pairs: Vec<Json> = arr_of(&pairs())
        .iter()
        .map(|p| {
            Json::obj([
                ("task_id", p.get("task_id").unwrap().clone()),
                ("repeat", p.get("repeat").unwrap().clone()),
                ("pair_id", p.get("pair_id").unwrap().clone()),
                ("subject", p.get("subject").unwrap().clone()),
                ("phase", p.get("phase").unwrap().clone()),
                ("stratum", p.get("stratum").unwrap().clone()),
                ("h", p.get("h").unwrap().clone()),
                ("o", p.get("o").unwrap().clone()),
            ])
        })
        .collect();
    let kl_filters = Json::obj([(
        "endpoints",
        Json::Arr(vec![Json::obj([
            ("endpoint", Json::str("capability")),
            ("stratum", Json::str("held_out")),
            ("required", Json::Bool(true)),
            ("kind", Json::str("binary")),
            ("tasks", Json::Int(4)),
            ("repeats", Json::Int(5)),
            ("pairs", Json::Arr(kl_pairs)),
        ])]),
    )]);
    let pr = prereg_with(200_000);
    let s = spec(
        kl_filters,
        IntervalMethod::KlDirectional,
        hharness_eval::directional_kl::KIND,
    );
    let r = analyze(&s, &In::new(&pr).input()).expect("kl computes");
    let ep = r
        .body
        .get(hharness_eval::directional_kl::KIND)
        .and_then(|k| k.get("endpoints"))
        .map(|e| arr_of(e)[0].clone())
        .unwrap();
    let verdict = ep
        .get("equivalence_verdict")
        .and_then(|v| v.get("verdict"))
        .and_then(Json::as_str);
    assert_eq!(verdict, Some("equivalent"));
}

// AC-1 — the predeclared identical-bridge reuse variant admits.
#[test]
fn identical_bridge_predeclared_admits() {
    let f = with(
        &filters(),
        "applicability",
        Json::obj([
            ("mode", Json::str("identical_bridge")),
            ("bridge_ref", Json::str("bridge/proof-1")),
            ("bridge_seq", Json::Int(50)),
        ]),
    );
    let body = run(f).expect("admits");
    assert_eq!(
        admission_of(&body)
            .get("applicability")
            .and_then(Json::as_str),
        Some("identical_bridge")
    );
    // A bridge declared at/after the first outcome is not predeclared.
    let late = with(
        &filters(),
        "applicability",
        Json::obj([
            ("mode", Json::str("identical_bridge")),
            ("bridge_ref", Json::str("bridge/proof-1")),
            ("bridge_seq", Json::Int(200)),
        ]),
    );
    assert!(matches!(
        admission_err(late),
        pa::AdmissionError::LateBridge { .. }
    ));
}

// AC-2 — the five known-inadequate evidence shapes (and unknown
// spellings) cannot establish authentic admission.
#[test]
fn inauthentic_shapes_refused() {
    for (shape, extra) in [
        (
            "recorded_model_io",
            Json::obj([("corpus_ref", Json::str("c/1"))]),
        ),
        (
            "artifact_benefit_wrapper",
            Json::obj([("report_ref", Json::str("r/1"))]),
        ),
        (
            "model_snapshot_maps",
            Json::obj([
                ("h_map_ref", Json::str("h/1")),
                ("o_map_ref", Json::str("o/1")),
            ]),
        ),
        (
            "oracle_transport_equality",
            Json::obj([("transport_ref", Json::str("t/1"))]),
        ),
        (
            "seed_presence",
            Json::obj([("seed_refs", Json::Arr(vec![Json::str("s/1")]))]),
        ),
        ("some_future_spelling", Json::obj([])),
    ] {
        let mut m = match &extra {
            Json::Obj(m) => m.clone(),
            _ => unreachable!(),
        };
        m.insert("shape".into(), Json::str(shape));
        let f = with(&filters(), "evidence", Json::Obj(m));
        match admission_err(f) {
            pa::AdmissionError::InauthenticEvidence { shape: got } => {
                assert_eq!(got, shape)
            }
            other => panic!("expected InauthenticEvidence, got {other:?}"),
        }
    }
}

// AC-2 — baseline-only A parity cannot admit policy B, whether the
// shape is `baseline_only` or `both_policies` missing B's evidence.
#[test]
fn baseline_only_policy_refused() {
    let base = with(
        &filters(),
        "applicability",
        Json::obj([
            ("mode", Json::str("baseline_only")),
            ("covered", Json::Arr(vec![Json::str("A")])),
        ]),
    );
    match admission_err(base) {
        pa::AdmissionError::PolicyScope { policy } => assert_eq!(policy, "B"),
        other => panic!("expected PolicyScope, got {other:?}"),
    }
    let partial = with(
        &filters(),
        "applicability",
        Json::obj([
            ("mode", Json::str("both_policies")),
            (
                "evidence",
                Json::Arr(vec![Json::obj([
                    ("policy", Json::str("A")),
                    ("evidence_ref", Json::str("ev-pol-A")),
                ])]),
            ),
        ]),
    );
    match admission_err(partial) {
        pa::AdmissionError::PolicyScope { policy } => assert_eq!(policy, "B"),
        other => panic!("expected PolicyScope, got {other:?}"),
    }
}

// AC-2 — absent or late dimension maps admit nothing.
#[test]
fn late_or_absent_map_refused() {
    let no_map = with(&filters(), "dimension_map", Json::Null);
    assert!(matches!(
        admission_err(no_map),
        pa::AdmissionError::MissingMap
    ));
    let late = with_map(&filters(), "map_seq", Json::Int(200));
    assert!(matches!(
        admission_err(late),
        pa::AdmissionError::LateMap {
            map_seq: 200,
            first_outcome_seq: 200
        }
    ));
    let unsigned = with_map(&filters(), "signature_ref", Json::str(""));
    assert!(matches!(
        admission_err(unsigned),
        pa::AdmissionError::MissingKey { .. }
    ));
}

// AC-2 — a dropped required endpoint blocks the conjunction; a typed
// inapplicable records its reason and never counts as passed.
#[test]
fn dropped_and_inapplicable_dimensions() {
    // Drop the required `reliability` entry.
    let entries = Json::Arr(vec![
        dim_entry("capability", "binary", "capability", "applicable"),
        dim_entry("efficiency", "bounded", "efficiency", "applicable"),
    ]);
    let dropped = with_map(&filters(), "entries", entries);
    match admission_err(dropped) {
        pa::AdmissionError::MissingDimension { dimension } => {
            assert_eq!(dimension, "reliability")
        }
        other => panic!("expected MissingDimension, got {other:?}"),
    }
    // Required dimension typed inapplicable — recorded, blocks.
    let inapp_req = {
        let mut e = dim_entry("reliability", "binary", "reliability", "inapplicable");
        if let Json::Obj(ref mut m) = e {
            m.insert("inapplicable_reason".into(), Json::str("no detector"));
        }
        with_map(
            &filters(),
            "entries",
            Json::Arr(vec![
                dim_entry("capability", "binary", "capability", "applicable"),
                e,
            ]),
        )
    };
    match admission_err(inapp_req) {
        pa::AdmissionError::RequiredInapplicable { dimension, reason } => {
            assert_eq!(dimension, "reliability");
            assert_eq!(reason, "no detector");
        }
        other => panic!("expected RequiredInapplicable, got {other:?}"),
    }
    // Non-required inapplicable — recorded on the report, not a pass.
    let inapp_opt = {
        let mut e = dim_entry("efficiency", "bounded", "efficiency", "inapplicable");
        if let Json::Obj(ref mut m) = e {
            m.insert("inapplicable_reason".into(), Json::str("meter absent"));
        }
        with_map(&filters(), "entries", {
            let mut v = match filters().get("dimension_map").unwrap().get("entries") {
                Some(Json::Arr(a)) => a.clone(),
                _ => unreachable!(),
            };
            v[2] = e;
            Json::Arr(v)
        })
    };
    let body = run(inapp_opt).expect("admits with non-required inapplicable");
    let dims = arr_of(admission_of(&body).get("dimensions").unwrap());
    let eff = &dims[2];
    assert_eq!(
        eff.get("status").and_then(Json::as_str),
        Some("inapplicable")
    );
    assert_eq!(
        eff.get("reason").and_then(Json::as_str),
        Some("meter absent")
    );
}

// AC-2 — missing required replay leaves the pair unmatched; unequal
// design, reused subjects, non-first attempts, honoured-seed coupling
// and floor shortfalls all refuse typed.
#[test]
fn manifest_integrity_refusals() {
    // Missing replay on one pair → unmatched, never a pass.
    let mut m = pairs();
    if let Json::Arr(ref mut v) = m {
        if let Json::Obj(ref mut p) = v[3] {
            p.insert("replay_ref".into(), Json::Null);
        }
    }
    let no_replay = with_manifest(&filters(), "pairs", m);
    match admission_err(no_replay) {
        pa::AdmissionError::MissingReplay { pairs } => {
            assert_eq!(pairs, vec!["pair-3".to_string()])
        }
        other => panic!("expected MissingReplay, got {other:?}"),
    }
    // Repeats under the S3 floor (4 < 5) and the adapter floor (2 < 3).
    for (repeats, want_s3) in [(4i64, true), (2, false)] {
        let mut v = Vec::new();
        for t in 0..4 {
            for r in 0..repeats {
                let k = t * repeats + r;
                v.push(Json::obj([
                    ("task_id", Json::str(&format!("t{:02}", t + 1))),
                    ("repeat", Json::Int(r)),
                    ("pair_id", Json::str(&format!("pair-{k}"))),
                    ("subject", Json::str(&format!("subj-{k}"))),
                    ("phase", Json::str("parity")),
                    ("stratum", Json::str("held_out")),
                    ("policy", Json::str("A")),
                    ("h_run_ref", Json::str("h")),
                    ("o_run_ref", Json::str("o")),
                    ("h_verdict_ref", Json::str("vh")),
                    ("o_verdict_ref", Json::str("vo")),
                    ("replay_ref", Json::str("rp")),
                    ("attempt", Json::Int(1)),
                ]));
            }
        }
        let f = with_manifest(&filters(), "pairs", Json::Arr(v));
        let f = with_manifest(&f, "repeats", Json::Int(repeats));
        match (admission_err(f), want_s3) {
            (pa::AdmissionError::S3Floor { repeats: 4 }, true) => {}
            (pa::AdmissionError::AdapterFloor { repeats: 2 }, false) => {}
            (other, s3) => panic!("repeats={repeats} s3={s3}: {other:?}"),
        }
    }
    // pairs ≠ T·R.
    let shrunk = with_manifest(&filters(), "tasks", Json::Int(5));
    assert!(matches!(
        admission_err(shrunk),
        pa::AdmissionError::UnequalDesign { .. }
    ));
    // Reused subject.
    let mut m = pairs();
    if let Json::Arr(ref mut v) = m {
        if let Json::Obj(ref mut p) = v[4] {
            p.insert("subject".into(), Json::str("subj-0"));
        }
    }
    assert!(matches!(
        admission_err(with_manifest(&filters(), "pairs", m)),
        pa::AdmissionError::ReusedSubject { .. }
    ));
    // Not a first attempt.
    let mut m = pairs();
    if let Json::Arr(ref mut v) = m {
        if let Json::Obj(ref mut p) = v[0] {
            p.insert("attempt".into(), Json::Int(2));
        }
    }
    assert!(matches!(
        admission_err(with_manifest(&filters(), "pairs", m)),
        pa::AdmissionError::NotFirstAttempt { .. }
    ));
    // Honoured-seed coupling is not the admitted grouping.
    let crn = with_manifest(&filters(), "dependence", Json::str("honoured_seed_crn"));
    assert!(matches!(
        admission_err(crn),
        pa::AdmissionError::UnsupportedDependence { .. }
    ));
    // Unequal weights.
    let w = with_manifest(&filters(), "weights", Json::str("size"));
    assert!(matches!(
        admission_err(w),
        pa::AdmissionError::UnequalDesign { .. }
    ));
    // Missing independence rationale.
    let r = with_manifest(&filters(), "independence_rationale_ref", Json::str(""));
    assert!(matches!(
        admission_err(r),
        pa::AdmissionError::MissingKey { .. }
    ));
    // Undeclared pair policy.
    let mut m = pairs();
    if let Json::Arr(ref mut v) = m {
        if let Json::Obj(ref mut p) = v[0] {
            p.insert("policy".into(), Json::str("C"));
        }
    }
    assert!(matches!(
        admission_err(with_manifest(&filters(), "pairs", m)),
        pa::AdmissionError::MissingIdentity { .. }
    ));
}

// AC-2 — mixed-uncertainty, baseline-only-bound and every other
// transfer claim fails; no certificate is admitted here.
#[test]
fn transfer_claims_refused() {
    for (transfer, want) in [
        (
            Json::obj([
                ("claim", Json::str("runner_transfer")),
                ("bound_a_ppm", Json::Int(-100_000)),
                ("uncertainty", Json::str("mixed")),
            ]),
            "mixed",
        ),
        (
            Json::obj([
                ("claim", Json::str("runner_transfer")),
                ("bound_a_ppm", Json::Int(-100_000)),
                ("uncertainty", Json::str("joint")),
            ]),
            "baseline_only",
        ),
        (
            Json::obj([
                ("claim", Json::str("runner_transfer")),
                ("bound_a_ppm", Json::Int(100_000)),
                ("bound_b_ppm", Json::Int(100_000)),
                ("kappa_ppm", Json::Int(150_000)),
                ("uncertainty", Json::str("joint")),
            ]),
            "budget",
        ),
        (
            Json::obj([
                ("claim", Json::str("runner_transfer")),
                ("bound_a_ppm", Json::Int(10_000)),
                ("bound_b_ppm", Json::Int(10_000)),
                ("kappa_ppm", Json::Int(150_000)),
                ("uncertainty", Json::str("joint")),
            ]),
            "unsupported",
        ),
    ] {
        let f = with(&filters(), "transfer", transfer);
        match (admission_err(f), want) {
            (pa::AdmissionError::MixedUncertainty { .. }, "mixed") => {}
            (pa::AdmissionError::BaselineOnlyTransfer { .. }, "baseline_only") => {}
            (pa::AdmissionError::TransferBudget { .. }, "budget") => {}
            (pa::AdmissionError::UnsupportedTransfer { .. }, "unsupported") => {}
            (other, w) => panic!("want {w}, got {other:?}"),
        }
    }
}

// AC-2 — nonlinear pass^k / conditional eligible-only estimands stay
// unavailable; unknown estimands/dimensions refuse identity;
// headline/CRN/R2 uses refuse; unknown margins refuse.
#[test]
fn unavailable_and_unknown_members() {
    for kind in ["pass^k", "conditional_eligible"] {
        let e = dim_entry("autonomy", kind, "capability", "applicable");
        let f = with_map(&filters(), "entries", {
            let mut v = match filters().get("dimension_map").unwrap().get("entries") {
                Some(Json::Arr(a)) => a.clone(),
                _ => unreachable!(),
            };
            v.push(e);
            Json::Arr(v)
        });
        match admission_err(f) {
            pa::AdmissionError::UnavailableMethod { dimension, .. } => {
                assert_eq!(dimension, "autonomy")
            }
            other => panic!("expected UnavailableMethod, got {other:?}"),
        }
    }
    let bad_kind = {
        let e = dim_entry("autonomy", "fuzzy", "capability", "applicable");
        let mut v = match filters().get("dimension_map").unwrap().get("entries") {
            Some(Json::Arr(a)) => a.clone(),
            _ => unreachable!(),
        };
        v.push(e);
        with_map(&filters(), "entries", Json::Arr(v))
    };
    assert!(matches!(
        admission_err(bad_kind),
        pa::AdmissionError::DimensionIdentity { .. }
    ));
    let bad_dim = {
        let mut v = match filters().get("dimension_map").unwrap().get("entries") {
            Some(Json::Arr(a)) => a.clone(),
            _ => unreachable!(),
        };
        v.push(dim_entry("vibes", "binary", "capability", "applicable"));
        with_map(&filters(), "entries", Json::Arr(v))
    };
    assert!(matches!(
        admission_err(bad_dim),
        pa::AdmissionError::DimensionIdentity { .. }
    ));
    let no_margin = {
        let e = dim_entry("autonomy", "binary", "unregistered", "applicable");
        let mut v = match filters().get("dimension_map").unwrap().get("entries") {
            Some(Json::Arr(a)) => a.clone(),
            _ => unreachable!(),
        };
        v.push(e);
        with_map(&filters(), "entries", Json::Arr(v))
    };
    assert!(matches!(
        admission_err(no_margin),
        pa::AdmissionError::MissingMargin { .. }
    ));
    for u in ["component_headline", "crn", "r2", "s3_claim"] {
        let f = with(&filters(), "use_as", Json::str(u));
        assert!(matches!(
            admission_err(f),
            pa::AdmissionError::UnsupportedClaim { .. }
        ));
    }
}

// AC-2 — routing refusals: the method on the wrong kind is
// metadata-only; the kind under another method is unknown; the
// margin authority is mandatory.
#[test]
fn routing_refusals() {
    let pr = prereg();
    // Metadata-only: ParityAdmission method on a non-admission kind.
    let s = spec(filters(), IntervalMethod::ParityAdmission, "compare");
    match analyze(&s, &In::new(&pr).input()) {
        Err(AnalysisError::ParityAdmission(pa::AdmissionError::MetadataOnly { kind })) => {
            assert_eq!(kind, "compare")
        }
        other => panic!("expected MetadataOnly, got {other:?}"),
    }
    // Unknown method on the admission kind.
    let s = spec(filters(), IntervalMethod::Wilson, pa::KIND);
    match analyze(&s, &In::new(&pr).input()) {
        Err(AnalysisError::ParityAdmission(pa::AdmissionError::UnknownMethod { .. })) => {}
        other => panic!("expected UnknownMethod, got {other:?}"),
    }
    // Missing pre-registration → MissingSpecRef (margin authority).
    let s = spec(filters(), IntervalMethod::ParityAdmission, pa::KIND);
    let inp = In::new(&pr);
    let mut input = inp.input();
    input.pre_registration = None;
    match analyze(&s, &input) {
        Err(AnalysisError::MissingSpecRef { .. }) => {}
        other => panic!("expected MissingSpecRef, got {other:?}"),
    }
}

// AC-4 — the persisted route: `analyze_and_record` serves
// `analysis_report_body/1` with the §6.5 producer kind member and
// idempotent replay.
#[test]
fn persisted_report_route() {
    let docs = LabDocs::open(&tmp("persist")).unwrap();
    let pr = prereg();
    let s = spec(filters(), IntervalMethod::ParityAdmission, pa::KIND);
    let r1 = analyze_and_record(&docs, &s, &In::new(&pr).input()).unwrap();
    let r2 = analyze_and_record(&docs, &s, &In::new(&pr).input()).unwrap();
    assert_eq!(r1.report_id, r2.report_id, "content-identical replay");
    let body = &r1.body;
    assert!(body.get(pa::KIND).is_some(), "report carries the section");
    assert_eq!(
        r1.record.kind.as_deref(),
        Some("parity_admission"),
        "record kind member"
    );
}
