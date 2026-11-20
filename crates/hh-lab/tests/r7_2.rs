//! R7.2 — the frozen exploration pre-registration + deterministic
//! selection machinery (manifest 327; spec §8.4/§8.5): the declared
//! X0/X1 envelope validating as `one_factor_at_a_time`, the verbatim
//! stop-rule set, the exploratory labels, the §8.5 frozen selection
//! rule (byte-equal to what executes), the deterministic
//! `hh-p4-selection/1` packet with its enforced input exclusions and
//! hash ordering, the selection/optimism labels, and the
//! confirmation-only combination leg. Offline fixtures only — no
//! subject, model call, or paid run.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use hharness_lab::experiment::ExperimentSpec;
use hharness_lab::p4x::*;
use hharness_lab::sweep::SweepRefusal;
use hharness_ontology::eval::DesignKind;
use hharness_wire::json::{parse, Json};
use hharness_wire::sha256::sha256_hex;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load(rel: &str) -> Json {
    parse(&std::fs::read_to_string(root().join(rel)).unwrap()).unwrap()
}

const REG: &str = "docs/build/reports/2026-11-20-fp-p4-sweep-registry.json";
const LEVELS: &str = "docs/build/reports/2026-11-20-fp-p4-levels.json";
const WRULE: &str = "docs/build/reports/2026-11-20-fp-p4-w-rule.json";
const ADM: &str = "docs/build/reports/2026-11-20-fp-p4-admissibility.json";
const TASKS: &str = "docs/build/reports/2026-11-08-fp-selection.json";
const PREREG: &str = "docs/build/reports/2026-11-20-fp-p4x-prereg.json";
const RULE: &str = "docs/build/reports/2026-11-20-fp-p4x-selection-rule.json";

/// The committed records the pass consumes (R7.1 outputs + the frozen
/// task-selection record).
fn inputs() -> (Json, Json, Json, Json, Json) {
    (load(REG), load(LEVELS), load(WRULE), load(ADM), load(TASKS))
}

fn prereg() -> Json {
    load(PREREG)
}

/// A synthetic X1 evidence row for the selection fixtures.
fn level(id: &str, factor: &str, order: u64, conditional: bool, delta_ppm: i64) -> LevelInput {
    LevelInput {
        level_id: id.to_string(),
        factor: factor.to_string(),
        registry_order: order,
        conditional,
        declared_delta_passed: true,
        liveness_passed: true,
        fidelity_ok: true,
        activation_ppm: if conditional { Some(400_000) } else { None },
        unresolved_slots: 0,
        planned_slots: 55,
        veto: false,
        cost_micros: 120_000,
        delta_ppm,
    }
}

// ── AC-1 (REQ-XD-206) — the declared X0/X1 OFAT design ───────────────────────

#[test]
fn ac1_declared_design_and_envelope() {
    let (reg, levels, _w, _a, tasks) = inputs();

    // The design derives from the frozen registry + levels record —
    // V1–V7 admitted over six axes, V8/V9 withheld (their compaction
    // axis held at baseline).
    let d = x_design(&reg, &levels).expect("x_design");
    assert_eq!(
        d.admitted
            .iter()
            .map(|l| l.level_id.as_str())
            .collect::<Vec<_>>(),
        vec!["V1", "V2", "V3", "V4", "V5", "V6", "V7"]
    );
    assert_eq!(
        d.withheld
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>(),
        vec!["V8", "V9"]
    );
    for (id, w) in &d.withheld {
        assert!(w.starts_with("withheld{"), "{id} disposition not typed");
    }
    // effort carries two perturbations; the withheld compaction axis
    // stays declared with its base level only.
    let effort = d.factors.iter().find(|f| f.name == "effort").unwrap();
    assert_eq!(effort.levels.len(), 3);
    let compaction = d.factors.iter().find(|f| f.name == "compaction").unwrap();
    assert_eq!(compaction.levels.len(), 1);

    // The task manifest: 11 band-A devs, X0 the stratum cover.
    let (x1, x0) = band_a_dev_tasks(&tasks).expect("tasks");
    assert_eq!(x1.len(), 11);
    assert_eq!(x0.len(), 3);

    // The committed record: both legs re-validate through `register`
    // as `one_factor_at_a_time`.
    let rec = prereg();
    assert!(check_x_prereg(&rec).is_empty());
    for leg in ["x0", "x1"] {
        let spec_j = rec
            .get("design")
            .unwrap()
            .get(leg)
            .unwrap()
            .get("experiment")
            .unwrap();
        let spec = ExperimentSpec::from_json(spec_j).expect("decode");
        assert_eq!(spec.design.kind, DesignKind::OneFactorAtATime);
        spec.register(&hharness_lab::experiment::SpecContext::member_level())
            .unwrap_or_else(|e| panic!("{leg} register refused: {e:?}"));
    }
    // The X1 leg: 8 arms (baseline + V1–V7), 11 band-A tasks, k = 5,
    // seeded slot_schedule_randomization in 5 rounds → 440 slots.
    let x1_leg = rec.get("design").unwrap().get("x1").unwrap();
    assert_eq!(x1_leg.get("tasks").and_then(Json::as_int), Some(11));
    assert_eq!(x1_leg.get("replicates").and_then(Json::as_int), Some(5));
    assert_eq!(x1_leg.get("rounds").and_then(Json::as_int), Some(5));
    assert_eq!(x1_leg.get("arms").and_then(Json::as_int), Some(8));
    assert_eq!(x1_leg.get("slots").and_then(Json::as_int), Some(440));
    let arms = x1_leg.get("arm_ids").unwrap();
    assert_eq!(
        arms,
        &Json::Arr(
            ["baseline", "V1", "V2", "V3", "V4", "V5", "V6", "V7"]
                .iter()
                .map(|s| Json::str(*s))
                .collect()
        )
    );
    // The X0 leg: 3 stratum-cover tasks × 1 replicate × all arms → 24.
    let x0_leg = rec.get("design").unwrap().get("x0").unwrap();
    assert_eq!(x0_leg.get("tasks").and_then(Json::as_int), Some(3));
    assert_eq!(x0_leg.get("slots").and_then(Json::as_int), Some(24));
    // The envelope agrees.
    assert_eq!(
        rec.get("envelope")
            .unwrap()
            .get("x1_slots")
            .and_then(Json::as_int),
        Some(440)
    );
    // The X1 spec's schedule is the seeded randomization.
    let sched = spec_j_member(&rec, "x1", "scheduling");
    assert_eq!(
        sched.get("permutation_seed").and_then(Json::as_str),
        Some("p4-x1-schedule-r7.2-2026-11-20")
    );

    // Pooling: an estimand spanning two cohorts refuses typed.
    check_pool_disjoint(&["x1"]).unwrap();
    check_pool_disjoint(&["x0"]).unwrap();
    match check_pool_disjoint(&["x0", "x1"]) {
        Err(SweepRefusal::PoolAcrossCohorts { .. }) => {}
        other => panic!("x0+x1 pooling must refuse: {other:?}"),
    }
    match check_pool_disjoint(&["x1", "confirmation"]) {
        Err(SweepRefusal::PoolAcrossCohorts { .. }) => {}
        other => panic!("exploration+confirmation pooling must refuse: {other:?}"),
    }
}

fn spec_j_member(rec: &Json, leg: &str, member: &str) -> Json {
    rec.get("design")
        .unwrap()
        .get(leg)
        .unwrap()
        .get("experiment")
        .unwrap()
        .get(member)
        .cloned()
        .unwrap_or(Json::Null)
}

// ── AC-2 (REQ-XD-210) — the declared stop-rule set ───────────────────────────

#[test]
fn ac2_stop_rules() {
    let rec = prereg();
    // The committed set covers exactly the four §8.4 classes.
    let rules = rec
        .get("stop_rules")
        .and_then(|s| s.get("rules"))
        .cloned()
        .unwrap();
    assert!(check_stop_set(Some(&rules)).is_empty());
    let classes: Vec<String> = rec
        .get("stop_rules")
        .unwrap()
        .get("classes")
        .and_then(|c| match c {
            Json::Arr(a) => Some(
                a.iter()
                    .filter_map(Json::as_str)
                    .map(str::to_string)
                    .collect(),
            ),
            _ => None,
        })
        .unwrap();
    assert_eq!(classes, STOP_CLASSES.to_vec());

    // The four valid classes pass individually.
    for class in STOP_CLASSES {
        let r = Json::obj([("class", Json::str(class)), ("detail", Json::str("…"))]);
        assert!(check_stop_rule(&r).is_empty(), "{class} refused");
    }

    // An outcome-based stop refuses typed.
    let outcome = Json::obj([
        ("class", Json::str("budget")),
        ("on_outcome", Json::str("stop when Δ̂ < 0")),
    ]);
    assert!(matches!(
        check_stop_rule(&outcome).as_slice(),
        [SweepRefusal::OutcomeStop { .. }]
    ));
    // A futility stop under an undeclared class — both legs refuse.
    let futility = Json::obj([("class", Json::str("futility"))]);
    let es = check_stop_rule(&futility);
    assert!(es
        .iter()
        .any(|e| matches!(e, SweepRefusal::StopClass { .. })));
    // A level drop refuses typed.
    let drop = Json::obj([
        ("class", Json::str("integrity")),
        ("drop_level", Json::str("V5")),
    ]);
    assert!(matches!(
        check_stop_rule(&drop).as_slice(),
        [SweepRefusal::LevelDrop { .. }]
    ));
    // An undeclared class alone refuses.
    let unknown = Json::obj([("class", Json::str("early_stop_on_gain"))]);
    assert!(matches!(
        check_stop_rule(&unknown).as_slice(),
        [SweepRefusal::StopClass { .. }]
    ));
}

// ── AC-3 (REQ-XD-207) — the exploration-packet labels ────────────────────────

#[test]
fn ac3_exploration_labels() {
    // A synthetic all-exploratory packet passes.
    let good = Json::obj([
        ("label", Json::str("exploratory")),
        (
            "figures",
            Json::Arr(vec![
                Json::obj([
                    ("label", Json::str("exploratory")),
                    (
                        "delta_hat",
                        Json::str("per-level equal-task Δ̂ — descriptive"),
                    ),
                    ("interval", Json::str("descriptive bootstrap [-.02, +.07]")),
                    ("bh_q", Json::str("0.31")),
                ]),
                Json::obj([
                    ("label", Json::str("exploratory")),
                    ("activation", Json::str("0.41 measured")),
                    ("cost", Json::str("$0.11 mean — descriptive")),
                ]),
            ]),
        ),
    ]);
    assert!(check_exploration_labels(&good).is_empty());

    // A confirmatory label on an exploration figure refuses.
    let mut bad = good.clone();
    if let Json::Obj(ref mut m) = bad {
        m.insert("label".into(), Json::str("confirmatory"));
    }
    assert!(matches!(
        check_exploration_labels(&bad).as_slice(),
        [SweepRefusal::ConfirmatoryLabel { .. }]
    ));
    // A nested confirmatory figure refuses.
    let mut nested = good.clone();
    if let Json::Obj(ref mut m) = nested {
        if let Some(Json::Arr(a)) = m.get_mut("figures") {
            if let Json::Obj(ref mut f) = a[0] {
                f.insert("label".into(), Json::str("headline claim"));
            }
        }
    }
    assert!(matches!(
        check_exploration_labels(&nested).as_slice(),
        [SweepRefusal::ConfirmatoryLabel { .. }]
    ));
    // A figure with no label refuses `missing_label`.
    let mut unlabeled = good.clone();
    if let Json::Obj(ref mut m) = unlabeled {
        if let Some(Json::Arr(a)) = m.get_mut("figures") {
            if let Json::Obj(ref mut f) = a[0] {
                f.remove("label");
            }
        }
    }
    assert!(matches!(
        check_exploration_labels(&unlabeled).as_slice(),
        [SweepRefusal::MissingLabel { .. }]
    ));
    // A packet with no label refuses.
    let bare = Json::obj([("figures", Json::Arr(vec![]))]);
    assert!(matches!(
        check_exploration_labels(&bare).as_slice(),
        [SweepRefusal::MissingLabel { .. }]
    ));
}

// ── AC-4 (REQ-XD-211) — the frozen selection rule ────────────────────────────

#[test]
fn ac4_frozen_selection_rule() {
    let rec = prereg();
    // The committed rule member is byte-equal to the frozen rule the
    // machinery executes, and the committed rule record equals it too.
    let member = rec.get("selection_rule").unwrap();
    check_frozen_rule(member).unwrap();
    assert_eq!(
        member.to_canonical_string(),
        load(RULE).to_canonical_string(),
        "the committed p4-selection-rule/1 record differs from the prereg's member"
    );
    assert_eq!(
        member.to_canonical_string(),
        frozen_rule_json().to_canonical_string(),
        "the prereg's selection_rule is not byte-equal to what executes"
    );
    // The executor decodes the *record's* member — the same parameters.
    assert_eq!(rule_from_json(member).unwrap(), frozen_selection_rule());
    // The declared sha member matches the rule's hash.
    assert_eq!(
        rec.get("selection_rule_sha256").and_then(Json::as_str),
        Some(frozen_rule_sha256().as_str())
    );

    // A paraphrased rule refuses.
    let mut para = member.clone();
    if let Json::Obj(ref mut m) = para {
        m.insert("m_max".into(), Json::Int(3));
    }
    assert!(matches!(
        check_frozen_rule(&para),
        Err(SweepRefusal::FrozenRuleMismatch { .. })
    ));
    // A rule member missing the machine parameters refuses decode.
    let thin = Json::obj([("text", Json::str("…"))]);
    assert!(matches!(
        rule_from_json(&thin),
        Err(SweepRefusal::MissingMember { .. })
    ));

    // Successful selection: two eligible positive-Δ̂ levels on
    // different factors → both selected + the pre-declared C₁₂.
    let rule = rule_from_json(member).unwrap();
    let inputs = vec![
        level("V4", "obs_renderer", 3, true, 52_000),
        level("V1", "effort", 0, false, 31_000),
        level("V2", "effort", 1, false, 20_000), // same factor — loses to V1
        level("V5", "reasoning_replay", 4, false, -5_000), // Δ̂ ≤ 0
    ];
    let out = select(&rule, &inputs, 100_000, 55);
    assert_eq!(
        out.get("disposition").and_then(Json::as_str),
        Some("selected{m=2}")
    );
    let members = out.get("members").unwrap();
    assert_eq!(
        members,
        &Json::Arr(vec![
            Json::obj([
                ("member", Json::str("V4")),
                ("factor", Json::str("obs_renderer")),
                ("rank", Json::Int(1)),
                ("delta_ppm", Json::Int(52_000)),
                (
                    "exploration_label",
                    Json::str("selected from the same data; estimate is exploratory")
                ),
            ]),
            Json::obj([
                ("member", Json::str("V1")),
                ("factor", Json::str("effort")),
                ("rank", Json::Int(2)),
                ("delta_ppm", Json::Int(31_000)),
                (
                    "exploration_label",
                    Json::str("selected from the same data; estimate is exploratory")
                ),
            ]),
        ])
    );
    let comb = out.get("combination").unwrap();
    assert_eq!(
        comb.get("label").and_then(Json::as_str),
        Some(COMBINATION_LABEL)
    );

    // One level per factor: V1 beats V2; V4 is on its own factor.
    // m_max = 2 with three qualifying factors → the top two. V7's Δ̂
    // clears the 1/55 resolution band against V1 (29 000 ppm × 55 >
    // 10⁶); against V4 it's inside the band and loses on registry
    // order at equal cost.
    let inputs3 = vec![
        level("V4", "obs_renderer", 3, true, 52_000),
        level("V1", "effort", 0, false, 31_000),
        level("V7", "tool_surface", 6, true, 60_000),
    ];
    let out3 = select(&rule, &inputs3, 100_000, 55);
    let m = out3.get("members").unwrap();
    let ids: Vec<&str> = match m {
        Json::Arr(a) => a
            .iter()
            .map(|e| e.get("member").and_then(Json::as_str).unwrap())
            .collect(),
        _ => vec![],
    };
    assert_eq!(ids, vec!["V4", "V7"]);

    // Ties within 1/(|D_A|·k) = 1/55 → ~18 182 ppm: equal-cost levels
    // tie-break by registry order; a lower-cost level wins within the
    // band.
    let mut cheap = level("V7", "tool_surface", 6, true, 52_000);
    cheap.cost_micros = 90_000;
    let mut dear = level("V6", "instance_template", 5, false, 52_000);
    dear.cost_micros = 200_000;
    assert_eq!(
        select_cmp(&cheap, &dear, 55),
        std::cmp::Ordering::Less,
        "within the resolution band the lower $/trial wins"
    );
    let far = level("V6", "instance_template", 5, false, 30_000);
    assert_eq!(
        select_cmp(&cheap, &far, 55),
        std::cmp::Ordering::Less,
        "outside the band the higher Δ̂ wins"
    );

    // no_candidate: every eligible level has Δ̂ ≤ 0 →
    // withheld{no_candidate} with the publishable statement.
    let empty = vec![
        level("V4", "obs_renderer", 3, true, -4_000),
        level("V1", "effort", 0, false, 0),
    ];
    let out_e = select(&rule, &empty, 100_000, 55);
    assert_eq!(
        out_e.get("disposition").and_then(Json::as_str),
        Some("withheld{no_candidate}")
    );
    assert_eq!(
        out_e.get("publishable").and_then(Json::as_str),
        Some(NO_CANDIDATE_STATEMENT)
    );

    // Gate refusals: an unmeasured conditional knob is
    // `unmeasured{insufficient_evidence}`, never a pass; a veto,
    // a fidelity break, and an over-budget level each fail.
    let mut unmeasured = level("V3", "exec_timeout", 2, true, 30_000);
    unmeasured.activation_ppm = None;
    let row = gate_table_row(&unmeasured, 100_000, &rule);
    assert_eq!(row.get("eligible"), Some(&Json::Bool(false)));
    assert!(row
        .get("gates")
        .and_then(|g| g.get("c"))
        .and_then(|g| g.get("detail"))
        .and_then(Json::as_str)
        .unwrap()
        .contains("insufficient_evidence"));

    let mut vetoed = level("V4", "obs_renderer", 3, true, 30_000);
    vetoed.veto = true;
    assert_eq!(
        gate_table_row(&vetoed, 100_000, &rule).get("eligible"),
        Some(&Json::Bool(false))
    );
    let mut infidel = level("V4", "obs_renderer", 3, true, 30_000);
    infidel.fidelity_ok = false;
    assert_eq!(
        gate_table_row(&infidel, 100_000, &rule).get("eligible"),
        Some(&Json::Bool(false))
    );
    let mut costly = level("V4", "obs_renderer", 3, true, 30_000);
    costly.cost_micros = 250_000; // > 2.0 × 100 000
    assert_eq!(
        gate_table_row(&costly, 100_000, &rule).get("eligible"),
        Some(&Json::Bool(false))
    );
    let mut dropped = level("V4", "obs_renderer", 3, true, 30_000);
    dropped.unresolved_slots = 6; // > 10 % of 55
    assert_eq!(
        gate_table_row(&dropped, 100_000, &rule).get("eligible"),
        Some(&Json::Bool(false))
    );
    // A vetoed positive-Δ̂ level does not qualify — the outcome still
    // lands withheld{no_candidate} when nothing eligible is positive.
    let out_v = select(&rule, &[vetoed], 100_000, 55);
    assert_eq!(
        out_v.get("disposition").and_then(Json::as_str),
        Some("withheld{no_candidate}")
    );
}

// ── AC-5 (REQ-XD-212) — the deterministic selection packet ───────────────────

#[test]
fn ac5_selection_packet() {
    let manifest = Json::Arr(vec![
        Json::obj([
            ("relpath", Json::str("docs/build/runs/x1/fp-p4x-x1-a.json")),
            ("sha256", Json::str("sha256:aaa")),
            ("cohort", Json::str("x1")),
        ]),
        Json::obj([
            ("relpath", Json::str("docs/build/runs/x1/fp-p4x-x1-b.json")),
            ("sha256", Json::str("sha256:bbb")),
            ("cohort", Json::str("x1")),
        ]),
    ]);
    assert!(check_selection_inputs(&manifest).is_empty());

    // The packet is a pure function — byte-identical recompute.
    let rule_sha = frozen_rule_sha256();
    let outcome = Json::obj([
        ("disposition", Json::str("selected{m=1}")),
        ("members", Json::Arr(vec![])),
    ]);
    let p1 = selection_packet(manifest.clone(), &rule_sha, outcome.clone(), 7);
    let p2 = selection_packet(manifest.clone(), &rule_sha, outcome.clone(), 7);
    assert_eq!(p1.to_canonical_string(), p2.to_canonical_string());
    assert_eq!(
        p1.get("schema").and_then(Json::as_str),
        Some(SELECTION_PACKET_SCHEMA)
    );

    // A held-out entry refuses `held_out_input`; phase-2/3 and other
    // cohorts refuse `foreign_outcome_input`.
    let mut held = manifest.clone();
    if let Json::Arr(ref mut a) = held {
        a.push(Json::obj([
            (
                "relpath",
                Json::str("docs/build/reports/held-out-record.json"),
            ),
            ("sha256", Json::str("sha256:ccc")),
            ("cohort", Json::str("held_out")),
        ]));
    }
    assert!(matches!(
        check_selection_inputs(&held).as_slice(),
        [SweepRefusal::HeldOutInput { .. }]
    ));
    for cohort in ["phase2", "phase_3", "confirmation", "x0", "held-out"] {
        let mut bad = manifest.clone();
        if let Json::Arr(ref mut a) = bad {
            a.push(Json::obj([
                ("relpath", Json::str("docs/build/reports/foreign.json")),
                ("sha256", Json::str("sha256:ddd")),
                ("cohort", Json::str(cohort)),
            ]));
        }
        let es = check_selection_inputs(&bad);
        assert!(
            es.iter().any(|e| matches!(
                e,
                SweepRefusal::HeldOutInput { .. } | SweepRefusal::ForeignOutcomeInput { .. }
            )),
            "cohort {cohort} must refuse: {es:?}"
        );
    }

    // Hash ordering: the packet's hash + seq predate the confirmation
    // pre-registration — the confirmation cites the packet hash.
    let packet_sha = sha256_hex(format!("{}\n", p1.to_canonical_string()).as_bytes());
    let mut packet = p1.clone();
    if let Json::Obj(ref mut m) = packet {
        m.insert("packet_sha256".into(), Json::str(&packet_sha));
    }
    let confirmation = Json::obj([
        ("schema", Json::str("hh-p4-c-prereg/1")),
        ("recorded_seq", Json::Int(9)),
        ("selection_packet_sha256", Json::str(&packet_sha)),
    ]);
    check_predates_confirmation(&packet, &confirmation).unwrap();

    // A confirmation that does not cite the packet's hash refuses.
    let bad_cite = Json::obj([
        ("schema", Json::str("hh-p4-c-prereg/1")),
        ("recorded_seq", Json::Int(9)),
        ("selection_packet_sha256", Json::str("sha256:wrong")),
    ]);
    assert!(matches!(
        check_predates_confirmation(&packet, &bad_cite),
        Err(SweepRefusal::HashOrdering { .. })
    ));
    // A confirmation whose seq is not later refuses — the packet's
    // hash must predate it.
    let late = Json::obj([
        ("schema", Json::str("hh-p4-c-prereg/1")),
        ("recorded_seq", Json::Int(7)),
        ("selection_packet_sha256", Json::str(&packet_sha)),
    ]);
    assert!(matches!(
        check_predates_confirmation(&packet, &late),
        Err(SweepRefusal::HashOrdering { .. })
    ));
}

// ── AC-6 (REQ-XD-213) — the selection label + the claims check ───────────────

#[test]
fn ac6_selection_label_and_claim() {
    let rec = prereg();
    assert_eq!(
        rec.get("labels")
            .unwrap()
            .get("selection")
            .and_then(Json::as_str),
        Some(SELECTION_DISPLAY_LABEL)
    );
    assert!(SELECTION_DISPLAY_LABEL.contains(SELECTION_LABEL));
    assert!(OPTIMISM_LABEL.contains("+.03") && OPTIMISM_LABEL.contains("+.10"));

    // A correctly labelled display passes; the claim cites the
    // confirmatory estimate.
    let good = Json::obj([
        ("level_id", Json::str("V4")),
        (
            "exploration",
            Json::obj([
                ("delta_ppm", Json::Int(52_000)),
                ("selection_label", Json::str(SELECTION_LABEL)),
                ("expected_optimism", Json::str(OPTIMISM_LABEL)),
            ]),
        ),
        (
            "claim",
            Json::obj([("estimate_source", Json::str("confirmatory"))]),
        ),
    ]);
    assert!(check_selection_display(&good).is_empty());

    // An unlabelled selected exploration estimate refuses.
    let mut unlab = good.clone();
    if let Json::Obj(ref mut m) = unlab {
        if let Some(Json::Obj(ref mut e)) = m.get_mut("exploration") {
            e.remove("selection_label");
        }
    }
    assert!(matches!(
        check_selection_display(&unlab).as_slice(),
        [SweepRefusal::SelectionLabelAbsent { .. }]
    ));
    // Missing the simulated optimism refuses too.
    let mut noopt = good.clone();
    if let Json::Obj(ref mut m) = noopt {
        if let Some(Json::Obj(ref mut e)) = m.get_mut("exploration") {
            e.remove("expected_optimism");
        }
    }
    assert!(matches!(
        check_selection_display(&noopt).as_slice(),
        [SweepRefusal::SelectionLabelAbsent { .. }]
    ));
    // A claim citing the exploration estimate refuses — the
    // confirmatory estimate is the claim.
    let mut bad_claim = good.clone();
    if let Json::Obj(ref mut m) = bad_claim {
        if let Some(Json::Obj(ref mut c)) = m.get_mut("claim") {
            c.insert("estimate_source".into(), Json::str("exploratory"));
        }
    }
    assert!(matches!(
        check_selection_display(&bad_claim).as_slice(),
        [SweepRefusal::ClaimCitesExploration { .. }]
    ));
}

// ── AC-7 (REQ-XD-214) — the combination leg ──────────────────────────────────

#[test]
fn ac7_combination() {
    let rec = prereg();
    let comb_decl = rec.get("combine_declaration").unwrap();
    assert_eq!(
        comb_decl.get("combine"),
        Some(&Json::Bool(COMBINE_DECLARED))
    );

    let paths_a = Json::Arr(vec![
        Json::str("obs_renderer.json_head_tail@1:render_cap"),
        Json::str("obs_renderer.json_head_tail@1:tail_lines"),
    ]);
    let paths_b = Json::Arr(vec![Json::str("instance_template.alternative_leaf")]);
    let union = Json::Arr(vec![
        Json::str("instance_template.alternative_leaf"),
        Json::str("obs_renderer.json_head_tail@1:render_cap"),
        Json::str("obs_renderer.json_head_tail@1:tail_lines"),
    ]);
    // The declared, confirmation-phase, union-clean combination is
    // admitted with the exact label.
    let good = Json::obj([
        ("member", Json::str("C₁₂")),
        ("predeclared", Json::Bool(true)),
        ("phase", Json::str("confirmation")),
        ("levels", Json::Arr(vec![Json::str("V4"), Json::str("V6")])),
        (
            "member_paths",
            Json::Arr(vec![paths_a.clone(), paths_b.clone()]),
        ),
        ("declared_delta", union),
        ("label", Json::str(COMBINATION_LABEL)),
    ]);
    let admission = check_combination(&good).expect("declared combination must admit");
    assert_eq!(admission.get("admitted"), Some(&Json::Bool(true)));
    assert_eq!(
        admission.get("label").and_then(Json::as_str),
        Some(COMBINATION_LABEL)
    );

    // An undeclared combination refuses.
    let mut undecl = good.clone();
    if let Json::Obj(ref mut m) = undecl {
        m.insert("predeclared".into(), Json::Bool(false));
    }
    assert!(matches!(
        check_combination(&undecl).unwrap_err().as_slice(),
        [SweepRefusal::UndeclaredCombination { .. }]
    ));

    // A combination outside confirmation refuses — exploration never
    // admits it.
    let mut explor = good.clone();
    if let Json::Obj(ref mut m) = explor {
        m.insert("phase".into(), Json::str("exploration"));
    }
    assert!(matches!(
        check_combination(&explor).unwrap_err().as_slice(),
        [SweepRefusal::CombinationContext { .. }]
    ));

    // A declared_delta that is not the union of the member levels'
    // declared paths refuses — the union-path check.
    let mut extra = good.clone();
    if let Json::Obj(ref mut m) = extra {
        m.insert(
            "declared_delta".into(),
            Json::Arr(vec![
                Json::str("instance_template.alternative_leaf"),
                Json::str("obs_renderer.json_head_tail@1:render_cap"),
                Json::str("obs_renderer.json_head_tail@1:tail_lines"),
                Json::str("exec_timeout.undeclared_extra"),
            ]),
        );
    }
    assert!(matches!(
        check_combination(&extra).unwrap_err().as_slice(),
        [SweepRefusal::UndeclaredCombination { .. }]
    ));

    // A wrong label refuses.
    let mut badlabel = good.clone();
    if let Json::Obj(ref mut m) = badlabel {
        m.insert("label".into(), Json::str("combination — additive"));
    }
    assert!(matches!(
        check_combination(&badlabel).unwrap_err().as_slice(),
        [SweepRefusal::MissingLabel { .. }]
    ));
}

// ── record integrity — the committed prereg is byte-reproducible ─────────────

#[test]
fn record_integrity() {
    // The committed record re-derives byte-identically from the
    // committed inputs.
    let (reg, levels, w_rule, adm, tasks) = inputs();
    let recomputed = x_prereg_record(&reg, &levels, &w_rule, &adm, &tasks, REG, WRULE, TASKS)
        .expect("recompute");
    let committed = prereg();
    assert_eq!(
        recomputed.to_canonical_string(),
        committed.to_canonical_string(),
        "the committed hh-p4-x-prereg/1 is not byte-identical to a recompute"
    );
    assert!(check_x_prereg(&committed).is_empty());
    check_registry_cite(&reg, &committed).unwrap();
    // The prereg's own sha256 sidecar verifies.
    let sidecar_path = PREREG.replace(".json", ".sha256");
    let sidecar = std::fs::read_to_string(root().join(&sidecar_path)).unwrap();
    let bytes = format!("{}\n", committed.to_canonical_string());
    let want = sha256_hex(bytes.as_bytes());
    assert!(
        sidecar.starts_with(&format!("{want}  {PREREG}")),
        "sidecar does not pin the record's hash"
    );
}
