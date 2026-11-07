//! R6.2 — phase-1 records-in hardening run (P1-B, REQ-XA-107…113,
//! REQ-XH-032, REQ-XA-006). One `hh-env` test over byte-copy fixtures
//! computes the HH-cell × `tb` cross-check and runs the landed
//! `parity_admission` / `kl_equivalence` / `stats::kl_directional_bound`
//! estimators over the recorded inputs exactly as they are, emitting the
//! `hh-surface-xcheck/1` and `hh-p1-estimators/1` packets through
//! `HH_P1_XCHECK_EMIT` / `HH_P1_ESTIMATORS_EMIT` (the committed artifacts
//! are produced by this exact path — never hand-written). Every typed
//! outcome is recorded as what it is: a refusal is the result.
//!
//! The CI readers scan forbids tests reading committed docs/ content at
//! runtime, so every input is a byte-copy fixture under
//! `tests/fixtures/{r5_49,p1}/` (RC-5 in the committed records verifier
//! proves the copies byte-equal the committed packets).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_eval::benefits::RegisteredMargin;
use hharness_eval::directional_kl::{
    kl_equivalence, EndpointDecl, EndpointKind, KlEquivalenceDecl, KlEquivalenceError,
    ETA_REPORT_PPM, ETA_TOST_PPM,
};
use hharness_eval::parity_admission::{
    parity_admission, AdmissionError, AdmissionEvidence, ManifestPair, PairManifest,
    ParityAdmissionDecl, PolicyApplicability, SourcePins,
};
use hharness_eval::stats::kl_directional_bound;
use hharness_wire::json::{self, Json};
use hharness_wire::sha256::sha256_hex;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

const COHORT_PACKETS: [&str; 4] = [
    "2026-11-05-s4-confirmation.json",
    "2026-11-05-s4-egress-cohort.json",
    "2026-11-06-s4-completion.json",
    "2026-11-06-s4-30slot-repairs.json",
];
const EPOCH_NAMES: [&str; 4] = ["confirmation", "egress", "completion", "repairs"];
const T3_PACKET: &str = "2026-11-05-parity-tranche-3.json";

fn load_fixture(dir: &str, name: &str) -> Json {
    let text = std::fs::read_to_string(
        repo_root()
            .join("crates/hh-env/tests/fixtures")
            .join(dir)
            .join(name),
    )
    .expect("packet fixture readable");
    // `parse_provider` — committed records carry float lexemes
    // (`cost_usd: 0.0`); the inbound read preserves them verbatim as
    // strings, never truncated or dropped.
    json::parse_provider(&text).expect("packet fixture parses")
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

fn jarr<'a>(j: &'a Json, k: &str) -> &'a [Json] {
    match jget(j, k) {
        Json::Arr(a) => a,
        other => panic!("member {k} not an array: {other:?}"),
    }
}

fn jslot_str(s: &Json, k: &str) -> String {
    match s.get(k) {
        Some(Json::Str(v)) => v.clone(),
        _ => String::new(),
    }
}

fn jreceipt(s: &Json, k: &str) -> String {
    s.get("receipt")
        .and_then(|r| r.get(k))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn receipt_verdict(s: &Json) -> Option<String> {
    s.get("receipt")
        .and_then(|r| r.get("verdict"))
        .and_then(|v| match v {
            Json::Str(v) if v == "resolved" || v == "unresolved" => Some(v.clone()),
            _ => None,
        })
}

fn is_executed(s: &Json) -> bool {
    jslot_str(s, "disposition") == "executed"
}

/// The four-epoch fold: a slot's last epoch record wins.
fn final_slots() -> (Vec<Json>, Vec<usize>) {
    let mut by_ref: BTreeMap<String, (usize, Json)> = BTreeMap::new();
    for (i, name) in COHORT_PACKETS.iter().enumerate() {
        let p = load_fixture("r5_49", name);
        for s in jarr(&p, "slots") {
            by_ref.insert(jstr(s, "slot_ref"), (i, s.clone()));
        }
    }
    let mut slots = Vec::new();
    let mut epochs = Vec::new();
    for (_, (i, s)) in by_ref {
        epochs.push(i);
        slots.push(s);
    }
    (slots, epochs)
}

/// O-side verdict + failure mode per task, verbatim from the tranche-3
/// packet's pairs[].
fn o_side() -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let p = load_fixture("p1", T3_PACKET);
    let mut v = BTreeMap::new();
    let mut fm = BTreeMap::new();
    for pair in jarr(&p, "pairs") {
        v.insert(jstr(pair, "task_id"), jstr(pair, "o_verdict"));
        fm.insert(jstr(pair, "task_id"), jstr(pair, "o_failure_mode"));
    }
    (v, fm)
}

/// A task's HH verdict: the common receipt verdict of its qualifying
/// slots (`discordant{h_internal}` if they disagree — none occur).
fn task_verdict(slots: &[&Json]) -> Option<String> {
    let mut v: Option<String> = None;
    for s in slots {
        let r = receipt_verdict(s)?;
        match &v {
            None => v = Some(r),
            Some(cur) if *cur == r => {}
            Some(_) => return Some("discordant{h_internal}".into()),
        }
    }
    v
}

/// Basis A (REQ-XH-032, primary): a task counts iff ≥ 1 confirmation-epoch
/// slot has first==true, disposition=="executed" and
/// receipt.verdict ∈ {resolved, unresolved}. A slot that carries a
/// receipt under any other disposition is a confound
/// (`inconclusive_with_receipt`), never a verdict. `admit_confounds` is
/// the mutation leg: treating those slots as executed yields 17/15.
fn basis_a(admit_confounds: bool) -> (BTreeMap<String, String>, Vec<Json>) {
    let conf = load_fixture("r5_49", COHORT_PACKETS[0]);
    let mut per_task: BTreeMap<String, Vec<&Json>> = BTreeMap::new();
    let mut confounds = Vec::new();
    for s in jarr(&conf, "slots") {
        if !matches!(s.get("first"), Some(Json::Bool(true))) {
            continue;
        }
        if is_executed(s) && receipt_verdict(s).is_some() {
            per_task.entry(jstr(s, "task_id")).or_default().push(s);
        } else if receipt_verdict(s).is_some() {
            confounds.push(s.clone());
            if admit_confounds {
                per_task.entry(jstr(s, "task_id")).or_default().push(s);
            }
        }
    }
    let verdicts = per_task
        .iter()
        .filter_map(|(t, sl)| task_verdict(sl).map(|v| (t.clone(), v)))
        .collect();
    (verdicts, confounds)
}

/// Basis B: the final four-epoch fold, epoch-stratified — every task's
/// HH verdict over all of its final-fold slots.
fn basis_b() -> BTreeMap<String, String> {
    let (slots, _) = final_slots();
    let mut per_task: BTreeMap<String, Vec<&Json>> = BTreeMap::new();
    for s in &slots {
        if is_executed(s) {
            per_task.entry(jstr(s, "task_id")).or_default().push(s);
        }
    }
    per_task
        .iter()
        .filter_map(|(t, sl)| task_verdict(sl).map(|v| (t.clone(), v)))
        .collect()
}

/// Basis C (descriptive): record-level agreement over all executed
/// final-fold records — slot counts are never treated as pairs.
fn basis_c() -> (usize, usize, BTreeMap<String, i64>) {
    let (slots, _) = final_slots();
    let (o_verdicts, _) = o_side();
    let (mut agree, mut total) = (0usize, 0usize);
    let mut discordant: BTreeMap<String, i64> = BTreeMap::new();
    for s in &slots {
        if !is_executed(s) {
            continue;
        }
        if let Some(v) = receipt_verdict(s) {
            total += 1;
            if Some(&v) == o_verdicts.get(&jstr(s, "task_id")) {
                agree += 1;
            } else {
                *discordant.entry(jstr(s, "task_id")).or_insert(0) += 1;
            }
        }
    }
    (agree, total, discordant)
}

// ── estimator ladders (records-in) ────────────────────────────────────────

fn empty_pair(task: &str) -> ManifestPair {
    ManifestPair {
        task_id: task.into(),
        repeat: 0,
        pair_id: format!("absent-in-record:pair_id:{task}"),
        subject: format!("absent-in-record:subject:{task}"),
        phase: "absent-in-record:phase".into(),
        stratum: "absent-in-record:stratum".into(),
        policy: "absent-in-record:policy".into(),
        h_run_ref: "absent-in-record:h_run_ref".into(),
        o_run_ref: "absent-in-record:o_run_ref".into(),
        h_verdict_ref: "absent-in-record:h_verdict_ref".into(),
        o_verdict_ref: "absent-in-record:o_verdict_ref".into(),
        replay_ref: None,
        attempt: 1,
        seed_requested: None,
        seed_honoured: None,
        h: None,
        o: None,
    }
}

fn probe_decl(tasks: u32, repeats: u32, pairs: Vec<ManifestPair>) -> ParityAdmissionDecl {
    ParityAdmissionDecl {
        participant_ref: "oracle:terminal-bench-oracle-agent+pytest-parser".into(),
        policies: vec!["absent-in-record:policies".into()],
        evidence: AdmissionEvidence::AuthenticFirstAttempt(PairManifest {
            tasks,
            repeats,
            weights: None,
            phase: "absent-in-record:phase".into(),
            stratum: "absent-in-record:stratum".into(),
            dependence: None,
            independence_rationale_ref: "absent-in-record:manifest.independence_rationale_ref"
                .into(),
            pairs,
        }),
        dimension_map: None,
        applicability: PolicyApplicability::BaselineOnly {
            covered: vec!["absent-in-record:applicability".into()],
        },
        transfer: None,
        pins: SourcePins {
            source_ref: "absent-in-record:pins.source_ref".into(),
            verifier_ref: "absent-in-record:pins.verifier_ref".into(),
            rights_ref: "absent-in-record:pins.rights_ref".into(),
        },
        use_as: None,
    }
}

/// One ladder outcome recorded verbatim — variant, Display text, rung.
fn outcome_json(variant: &str, display: &str, rung: &str) -> Json {
    Json::obj([
        ("rung", Json::str(rung)),
        ("variant", Json::str(variant)),
        ("display", Json::str(display)),
    ])
}

// ── emission ──────────────────────────────────────────────────────────────

fn canonical_with_id(members: Vec<(&'static str, Json)>) -> String {
    let mut body = Json::obj(members);
    let canon = body.to_canonical_string();
    let id = format!("sha256:{}", sha256_hex(canon.as_bytes()));
    if let Json::Obj(ref mut m) = body {
        m.insert("packet_id".into(), Json::Str(id));
    }
    format!("{}\n", body.to_canonical_string())
}

fn emit(var: &str, payload: &str) {
    if let Ok(out) = std::env::var(var) {
        let out = PathBuf::from(&out);
        let out = if out.is_absolute() {
            out
        } else {
            repo_root().join(out)
        };
        std::fs::write(&out, payload).unwrap();
        eprintln!("emitted {}", out.display());
    }
}

// ── the test ──────────────────────────────────────────────────────────────

#[test]
fn p1_records_in_hardening() {
    let (o_verdicts, o_failure) = o_side();
    let tasks: Vec<String> = o_verdicts.keys().cloned().collect();
    assert_eq!(tasks.len(), 30);

    // ── REQ-XH-032 basis A + the confound leg ─────────────────────────
    let (a_verdicts, confounds) = basis_a(false);
    let a_agree: Vec<&String> = a_verdicts
        .iter()
        .filter(|(t, v)| o_verdicts.get(*t) == Some(*v))
        .map(|(t, _)| t)
        .collect();
    let a_dis: Vec<&String> = a_verdicts
        .iter()
        .filter(|(t, v)| o_verdicts.get(*t) != Some(*v))
        .map(|(t, _)| t)
        .collect();
    assert_eq!(a_verdicts.len(), 16, "basis A tasks");
    assert_eq!(a_agree.len(), 14, "basis A agree");
    assert_eq!(a_dis.len(), 2, "basis A discordant");
    assert!(a_dis.contains(&&"grid-pattern-transform".to_string()));
    assert!(a_dis.contains(&&"swe-bench-langcodes".to_string()));
    // The confound: hf-model-inference's two first slots carry a receipt
    // under an inconclusive disposition — inconclusive_with_receipt,
    // never a verdict (C-06).
    assert_eq!(confounds.len(), 2);
    for c in &confounds {
        assert_eq!(jstr(c, "task_id"), "hf-model-inference");
        assert!(jstr(c, "disposition").starts_with("inconclusive"));
    }
    // The mutation admitting inconclusive_with_receipt slots yields
    // 17/15 — asserted different from the REQ-XH-032 basis (a fixture
    // admitting them would fail the 16/14/2 pin above).
    let (a_mut, _) = basis_a(true);
    let a_mut_agree = a_mut
        .iter()
        .filter(|(t, v)| o_verdicts.get(*t) == Some(*v))
        .count();
    assert_eq!((a_mut.len(), a_mut_agree), (17, 15));
    assert_ne!((a_mut.len(), a_mut_agree), (16, 14));

    // ── basis B + C ───────────────────────────────────────────────────
    let b_verdicts = basis_b();
    let b_dis: Vec<&String> = b_verdicts
        .iter()
        .filter(|(t, v)| o_verdicts.get(*t) != Some(*v))
        .map(|(t, _)| t)
        .collect();
    assert_eq!(b_verdicts.len(), 30);
    assert_eq!(b_verdicts.len() - b_dis.len(), 26, "basis B agree 26/30");
    for t in [
        "fix-git",
        "grid-pattern-transform",
        "oom",
        "swe-bench-langcodes",
    ] {
        assert!(b_dis.contains(&&t.to_string()), "basis B discordance {t}");
    }
    let (c_agree, c_total, c_dis) = basis_c();
    assert_eq!((c_agree, c_total), (260, 300));

    // O build-stage tasks are reported separately (never folded into
    // agreement): the four pairs whose O side failed at build stage.
    let build_stage: Vec<&String> = tasks
        .iter()
        .filter(|t| o_failure.get(*t).map(|f| f.as_str()) != Some("unset"))
        .collect();
    assert_eq!(build_stage.len(), 4);

    // ── REQ-XA-006: intended vs served per arm, analysis basis named ──
    let (fslots, fepochs) = final_slots();
    let declared_arms: [(&str, &str); 2] = [
        ("arm:o-tb-oracle", "evict_oldest"),
        ("arm:h-podman-oracle", "clear_tool_results"),
    ];
    let mut arms_json = Vec::new();
    for (arm, strategy) in declared_arms {
        let mut intended = 0i64;
        let mut served: BTreeMap<String, i64> = BTreeMap::new();
        for s in &fslots {
            if jstr(s, "arm_id") == arm {
                intended += 1;
                *served.entry(jslot_str(s, "served_variant")).or_insert(0) += 1;
            }
        }
        arms_json.push((
            arm,
            Json::obj([
                ("intended", Json::obj([(strategy, Json::Int(intended))])),
                (
                    "served",
                    Json::Obj(
                        served
                            .iter()
                            .map(|(k, v)| (k.clone(), Json::Int(*v)))
                            .collect(),
                    ),
                ),
            ]),
        ));
    }
    // The final fold's own pins: A serves 150 evict; B serves 90 clear +
    // 60 evict (the recorded no-op labels, REQ-XA-006).
    // ── estimator ladders ─────────────────────────────────────────────
    // parity_admission — strict records-only rung, tranche 3: the record
    // has source/verifier identities but no rights record at all.
    let strict_decl = ParityAdmissionDecl {
        participant_ref: "oracle:terminal-bench-oracle-agent+pytest-parser".into(),
        policies: vec![],
        evidence: AdmissionEvidence::AuthenticFirstAttempt(PairManifest {
            tasks: 30,
            repeats: 1,
            weights: None,
            phase: "absent-in-record:phase".into(),
            stratum: "absent-in-record:stratum".into(),
            dependence: None,
            independence_rationale_ref: String::new(),
            pairs: tasks.iter().map(|t| empty_pair(t)).collect(),
        }),
        dimension_map: None,
        applicability: PolicyApplicability::BaselineOnly { covered: vec![] },
        transfer: None,
        pins: SourcePins {
            source_ref: "terminal-bench-core==0.1.1".into(),
            verifier_ref: "oracle:terminal-bench-oracle-agent+pytest-parser".into(),
            rights_ref: String::new(),
        },
        use_as: None,
    };
    let strict = parity_admission(&strict_decl, &[]);
    let strict_out = match &strict {
        Err(AdmissionError::MissingKey { member }) => outcome_json(
            "MissingKey",
            &format!("missing declared key: {member}"),
            "strict records-only, tranche 3",
        ),
        other => panic!("strict rung: unexpected {other:?}"),
    };
    // labelled design-probe rung — placeholders are not record facts;
    // not an admission attempt: AdapterFloor{repeats:1} under R=1.
    let probe = probe_decl(30, 1, tasks.iter().map(|t| empty_pair(t)).collect());
    let probe_res = parity_admission(&probe, &[]);
    let probe_out = match &probe_res {
        Err(AdmissionError::AdapterFloor { repeats }) => outcome_json(
            "AdapterFloor",
            &format!("adapter floor unmet: {repeats} repeats < 3"),
            "labelled design probe, tranche 3",
        ),
        other => panic!("probe rung: unexpected {other:?}"),
    };
    // cross-check under the frozen T = 30: 16 basis-A pairs ≠ 30.
    let xc_decl = probe_decl(30, 1, a_verdicts.keys().map(|t| empty_pair(t)).collect());
    let xc_res = parity_admission(&xc_decl, &[]);
    let xc_out = match &xc_res {
        Err(AdmissionError::UnequalDesign { detail }) => outcome_json(
            "UnequalDesign",
            &format!("unequal planned design: {detail}"),
            "cross-check, frozen T = 30",
        ),
        other => panic!("xcheck rung: unexpected {other:?}"),
    };

    // kl_equivalence — margins decoded from the pre-registration: none
    // were ever registered (the tranche's only declared margin was
    // "EXACT"). The margins slice is empty; the typed outcome is the
    // result.
    let margins: &[RegisteredMargin] = &[];
    assert!(margins.is_empty(), "REQ-XA-113: no post-hoc margin");
    let kl_decl = KlEquivalenceDecl {
        endpoints: vec![EndpointDecl {
            endpoint: "directional.task_verdict_agreement".into(),
            stratum: "absent-in-record:stratum".into(),
            required: true,
            kind: EndpointKind::Binary,
            range_h: None,
            range_o: None,
            zero_reference: None,
            tasks: 30,
            repeats: 1,
            pairs: vec![],
        }],
        conjunction: None,
        selected_claims: 1,
        dependence: None,
        weights: None,
        use_as: None,
        level_ppm: 950_000,
    };
    let kl_res = kl_equivalence(&kl_decl, margins);
    let kl_out = match &kl_res {
        Err(KlEquivalenceError::MissingMargin { dimension }) => outcome_json(
            "MissingMargin",
            &format!("no registered margin binds dimension {dimension}"),
            "kl_equivalence, margins from pre-registration: none",
        ),
        other => panic!("kl_equivalence: unexpected {other:?}"),
    };

    // ── descriptive endpoints (stats::kl_directional_bound) ───────────
    // inference: descriptive — no verdict member is ever emitted here.
    let desc_row = |label: &'static str, n: i64, c_plus: i64, c_minus: i64, u: i64| -> Json {
        let xp = ((c_plus + u) * 1_000_000) / n;
        let xm = ((c_minus + u) * 1_000_000) / n;
        let bounds = |eta: i64| -> Json {
            let kp = kl_directional_bound(xp, n, eta).expect("bound defined");
            let km = kl_directional_bound(xm, n, eta).expect("bound defined");
            Json::obj([
                ("eta_ppm", Json::Int(eta)),
                ("plus", Json::Arr(vec![Json::Int(kp.lo), Json::Int(kp.hi)])),
                ("minus", Json::Arr(vec![Json::Int(km.lo), Json::Int(km.hi)])),
                (
                    "delta",
                    Json::Arr(vec![Json::Int(kp.lo - km.hi), Json::Int(kp.hi - km.lo)]),
                ),
            ])
        };
        Json::obj([
            ("label", Json::str(label)),
            ("n", Json::Int(n)),
            ("c_plus", Json::Int(c_plus)),
            ("c_minus", Json::Int(c_minus)),
            ("uncertain", Json::Int(u)),
            ("estimator", Json::str("stats::kl_directional_bound")),
            ("inference", Json::str("descriptive")),
            (
                "bounds",
                Json::Arr(vec![bounds(ETA_TOST_PPM), bounds(ETA_REPORT_PPM)]),
            ),
        ])
    };
    let desc = vec![
        desc_row("tranche-3, N=30, zero discordance", 30, 0, 0, 0),
        desc_row(
            "tranche-3, 4 build-stage pairs as uncertain envelope",
            30,
            0,
            0,
            4,
        ),
        desc_row("cross-check basis A, N=16", 16, 0, 2, 0),
        desc_row(
            "cross-check basis B on the 26 O-executed tasks",
            26,
            0,
            2,
            0,
        ),
    ];

    // ── emit hh-p1-estimators/1 ───────────────────────────────────────
    let estimators = canonical_with_id(vec![
        ("schema", Json::str("hh-p1-estimators/1")),
        ("kind", Json::str("phase-1 records-in estimator ladders — typed outcomes verbatim")),
        ("evidence_class", Json::str("recorded execution — descriptive; not an admission")),
        ("generated_by", Json::str("cargo test -p hharness-env --test r6_2 (HH_P1_ESTIMATORS_EMIT)")),
        ("policy_counts", Json::str("n/a{not an arm-bearing packet — no slots[] carried}")),
        ("parity_admission", Json::Arr(vec![strict_out, probe_out, xc_out])),
        ("kl_equivalence", Json::obj([
            ("margins_source", Json::str("pre-registration:none")),
            ("margins", Json::Arr(vec![])),
            ("outcome", kl_out),
        ])),
        ("descriptive_bounds", Json::Arr(desc)),
        ("floors", Json::Arr(vec![
            Json::str("§4.4: adapter floor >= 3 repeats/task; S3 floor >= 5 repeats/task/arm — stated because the estimator does not enforce them"),
            Json::str("R = 1 on every surface here; the floors are unmet by construction"),
            Json::str("eta_tost >= 25000 ppm/claim; eta_report >= 12500 ppm/claim"),
        ])),
    ]);
    emit("HH_P1_ESTIMATORS_EMIT", &estimators);
    // REQ-XA-109: no `verdict` member in any KL quantity; the margins
    // slice is asserted empty.
    let parsed = json::parse(&estimators).expect("emitted packet reparses");
    fn no_verdict(j: &Json) -> bool {
        match j {
            Json::Obj(m) => !m.contains_key("verdict") && m.values().all(no_verdict),
            Json::Arr(a) => a.iter().all(no_verdict),
            _ => true,
        }
    }
    if let Json::Obj(m) = &parsed {
        assert!(no_verdict(&parsed), "a verdict member must never appear");
        assert!(m.contains_key("packet_id"));
    }

    // ── emit hh-surface-xcheck/1 ──────────────────────────────────────
    let slot_rows: Vec<Json> = fslots
        .iter()
        .map(|s| {
            Json::obj([
                ("slot_ref", Json::str(jslot_str(s, "slot_ref"))),
                ("task_id", Json::str(jslot_str(s, "task_id"))),
                ("arm_id", Json::str(jslot_str(s, "arm_id"))),
                ("served_variant", Json::str(jslot_str(s, "served_variant"))),
            ])
        })
        .collect();
    assert_eq!(slot_rows.len(), 300);
    let discordance_rows: Vec<Json> = {
        let mut rows = Vec::new();
        // per-side evidence: verdict, reason, channel, epoch, backend,
        // network, workdir-repair flag (REQ-XA-108).
        for (t, hv) in b_verdicts.iter() {
            let ov = &o_verdicts[t];
            if *hv == *ov {
                continue;
            }
            let mut hh_ev = Vec::new();
            for (i, s) in fslots.iter().enumerate() {
                if jstr(s, "task_id") != *t {
                    continue;
                }
                let epoch = fepochs[i];
                if s.get("receipt").is_none() {
                    continue;
                }
                hh_ev.push(Json::obj([
                    ("slot_ref", Json::str(jslot_str(s, "slot_ref"))),
                    ("verdict", Json::str(jreceipt(s, "verdict"))),
                    ("reason", Json::str(jreceipt(s, "verdict_reason"))),
                    ("channel", Json::str(jreceipt(s, "channel"))),
                    ("epoch", Json::str(EPOCH_NAMES[epoch])),
                    (
                        "backend",
                        Json::str(if epoch >= 2 {
                            "amd64-gce"
                        } else {
                            "arm64-applehv"
                        }),
                    ),
                    (
                        "network",
                        Json::str(if jslot_str(s, "cell_network") == "bridge" {
                            "bridge"
                        } else {
                            "none"
                        }),
                    ),
                    ("workdir_repair", Json::Bool(epoch == 3)),
                ]));
            }
            rows.push(Json::obj([
                ("task_id", Json::str(t)),
                (
                    "o_side",
                    Json::obj([
                        ("verdict", Json::str(ov)),
                        ("failure_mode", Json::str(&o_failure[t])),
                        ("source", Json::str("tb oracle runner — tranche 3")),
                    ]),
                ),
                ("hh_side", Json::Arr(hh_ev)),
            ]));
        }
        rows
    };
    let basis_json = |tasks_n: i64, agree: i64, dis: &[&String], rule: &str| -> Json {
        Json::obj([
            ("rule", Json::str(rule)),
            ("tasks", Json::Int(tasks_n)),
            ("agree", Json::Int(agree)),
            ("discordant", Json::Int(dis.len() as i64)),
            (
                "discordant_tasks",
                Json::Arr(dis.iter().map(|t| Json::str(t.as_str())).collect()),
            ),
        ])
    };
    let xcheck = canonical_with_id(vec![
        ("schema", Json::str("hh-surface-xcheck/1")),
        ("kind", Json::str("records-in cross-check — HH-cell confirmation receipts vs tranche-3 O verdicts")),
        ("evidence_class", Json::str("recorded execution — descriptive; never a claim the HH cells agree with the original runner")),
        ("generated_by", Json::str("cargo test -p hharness-env --test r6_2 (HH_P1_XCHECK_EMIT)")),
        ("protocol_id", Json::str("9edc5327f89e2d66f8003bedc2530bc3e7d37326aad82cdfcab70465eda60a5f")),
        ("inputs", Json::obj([
            ("hh_packets", Json::Arr(COHORT_PACKETS.iter().map(|n| Json::str(*n)).collect())),
            ("o_packet", Json::str(T3_PACKET)),
        ])),
        ("declared_arms", Json::obj(declared_arms.iter().map(|(a, s)| (*a, Json::str(*s))).collect::<Vec<_>>())),
        ("policy_counts", Json::obj([
            ("analysis_basis", Json::str("assignment")),
            (
                "arms",
                Json::Obj(
                    arms_json
                        .iter()
                        .map(|(a, j)| (a.to_string(), j.clone()))
                        .collect(),
                ),
            ),
        ])),
        ("bases", Json::obj([
            (
                "A",
                basis_json(
                    a_verdicts.len() as i64,
                    a_agree.len() as i64,
                    &a_dis,
                    "attempt 1, primary (REQ-XH-032): >= 1 confirmation slot with first, executed, receipt verdict in {resolved,unresolved}",
                ),
            ),
            (
                "B",
                basis_json(
                    b_verdicts.len() as i64,
                    (b_verdicts.len() - b_dis.len()) as i64,
                    &b_dis,
                    "final four-epoch fold, epoch-stratified",
                ),
            ),
            (
                "C",
                Json::obj([
                    ("rule", Json::str("all executed final-fold records, descriptive — slot counts are never treated as pairs")),
                    ("records", Json::Int(c_total as i64)),
                    ("agree", Json::Int(c_agree as i64)),
                    ("discordant_records", Json::Int((c_total - c_agree) as i64)),
                    (
                        "per_task",
                        Json::Obj(
                            c_dis.iter()
                                .map(|(t, n)| (t.clone(), Json::Int(*n)))
                                .collect(),
                        ),
                    ),
                ]),
            ),
        ])),
        ("agreement_table", Json::obj([
            ("basis_A", Json::str("14/16")),
            ("basis_B", Json::str("26/30")),
            ("basis_C_records", Json::str("260/300")),
        ])),
        ("discordances", Json::Arr(discordance_rows)),
        ("o_build_stage", Json::Arr(build_stage.iter().map(|t| Json::str(t.as_str())).collect())),
        ("confounds", Json::Arr(
            confounds
                .iter()
                .map(|c| {
                    Json::obj([
                        ("task_id", Json::str(jstr(c, "task_id"))),
                        ("slot_ref", Json::str(jstr(c, "slot_ref"))),
                        ("kind", Json::str("inconclusive_with_receipt")),
                        ("disposition", Json::str(jstr(c, "disposition"))),
                    ])
                })
                .collect::<Vec<_>>(),
        )),
        ("slots", Json::Arr(slot_rows)),
    ]);
    emit("HH_P1_XCHECK_EMIT", &xcheck);
}
