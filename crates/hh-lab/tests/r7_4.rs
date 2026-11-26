//! R7.4 — the offline, $0 fold of the committed X0/X1 records into
//! the exploration packet, the deterministic §8.5 selection packet,
//! the frozen `hh-p4-c-prereg/1` confirmation pre-registration over
//! the T = 37 held-out census, and the GATE-G36 presentation pieces
//! (the X1-measured `comparative-quote/1` + declared-inputs record,
//! the minted p4c `hh-compute-quote/1`, the PENDING readout).
//! The battery reads the committed records only — no subject, model
//! call, or paid run. Recompute byte-identical is the checker's job
//! (scripts/check-fp-p4x.sh); this battery pins the shapes, the
//! hashes, the member lists and the honesty members.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use hharness_wire::json::{parse, parse_provider, Json};
use hharness_wire::sha256::sha256_hex;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load(rel: &str) -> Json {
    parse(&std::fs::read_to_string(root().join(rel)).unwrap()).unwrap()
}

/// The `hh-compute-quote/1` record carries fractional planning
/// members (instance_hours "1129/36") — the provider-bytes read
/// preserves them verbatim (R4.5b).
fn load_provider(rel: &str) -> Json {
    parse_provider(&std::fs::read_to_string(root().join(rel)).unwrap()).unwrap()
}

fn sha_file(rel: &str) -> String {
    let bytes = std::fs::read(root().join(rel)).unwrap();
    format!("sha256:{}", sha256_hex(bytes.as_slice()))
}

fn as_obj(j: &Json) -> &BTreeMap<String, Json> {
    match j {
        Json::Obj(m) => m,
        _ => panic!("expected an object"),
    }
}

fn as_arr(j: &Json) -> &Vec<Json> {
    match j {
        Json::Arr(a) => a,
        _ => panic!("expected an array"),
    }
}

fn int(j: &Json) -> i64 {
    match j {
        Json::Int(i) => *i,
        _ => panic!("expected an int: {j:?}"),
    }
}

const EXPLORATION: &str = "docs/build/reports/2026-11-24-fp-p4x-exploration.json";
const SELECTION: &str = "docs/build/reports/2026-11-24-fp-p4x-selection.json";
const CPREREG: &str = "docs/build/reports/2026-11-24-fp-p4c-prereg.json";
const QUOTE: &str = "docs/build/reports/2026-11-24-fp-p4c-quote.json";
const QUOTE_INPUTS: &str = "docs/build/reports/2026-11-24-fp-p4c-quote-inputs.json";
const COMPUTE_QUOTE: &str = "docs/build/reports/2026-11-24-fp-p4c-compute-quote.json";
const READOUT: &str = "docs/build/readouts/GATE-G36.md";

// ── AC1 · the exploration packet — the $0 fold's honest census ──────

#[test]
fn ac1_exploration_packet_shape() {
    let e = load(EXPLORATION);
    assert_eq!(
        e.get("schema").and_then(Json::as_str),
        Some("hh-p4-x-exploration/1")
    );
    assert_eq!(
        e.get("ticket").and_then(Json::as_str).unwrap_or(""),
        "330_R7.4"
    );

    // The WAL's attribution is authoritative; the 8 µ$ lane-probe
    // residual is recorded verbatim, never refused or imputed.
    assert_eq!(int(e.get("total_cost_micros").unwrap()), 19_250_389);
    assert_eq!(int(e.get("non_slot_attribution_micros").unwrap()), 8);

    // The detail_lost disposition: 46 recorded ids, unresolved for
    // every purpose, never imputed.
    let dl = as_obj(e.get("detail_lost").unwrap());
    assert_eq!(int(dl.get("count").unwrap()), 46);
    assert!(dl
        .get("disposition")
        .and_then(Json::as_str)
        .unwrap()
        .contains("detail_lost"));
    assert!(dl
        .get("disposition")
        .and_then(Json::as_str)
        .unwrap()
        .contains("never imputed"));

    // The fidelity census: one served model ×10,602 calls; completed
    // calls == attributed receipts.
    let census = as_arr(e.get("served_model_census").unwrap());
    assert_eq!(census.len(), 1);
    assert_eq!(
        census[0].as_str().unwrap(),
        "glm-5.3-flash\u{d7}10602",
        "served_model census"
    );
    assert_eq!(int(e.get("completed_calls_census").unwrap()), 10_602);

    // Baseline: 55 planned / 50 resolved / 5 unresolved; 2,539,088 µ$.
    let b = as_obj(e.get("baseline").unwrap());
    let env = as_obj(b.get("completion_envelope").unwrap());
    assert_eq!(int(env.get("planned").unwrap()), 55);
    assert_eq!(int(env.get("resolved").unwrap()), 50);
    assert_eq!(int(env.get("unresolved").unwrap()), 5);
    let bcost = as_obj(b.get("cost").unwrap());
    assert_eq!(int(bcost.get("total_micros").unwrap()), 2_539_088);
    assert_eq!(int(bcost.get("per_executed_slot_micros").unwrap()), 46_165);
    assert_eq!(int(bcost.get("per_resolved_slot_micros").unwrap()), 50_781);

    // Seven swept levels, registry order 1–7.
    let levels = as_arr(e.get("levels").unwrap());
    assert_eq!(levels.len(), 7);
    for (i, l) in levels.iter().enumerate() {
        assert_eq!(
            l.get("level_id").and_then(Json::as_str).unwrap(),
            format!("V{}", i + 1)
        );
        assert_eq!(int(l.get("registry_order").unwrap()) as usize, i + 1);
    }
    // V1: the measured activation, Δ̂ +45,455 ppm.
    let v1 = &levels[0];
    let act = as_obj(v1.get("activation").unwrap());
    assert_eq!(int(act.get("ppm").unwrap()), 1_000_000);
    assert!(act
        .get("detail")
        .and_then(Json::as_str)
        .unwrap()
        .contains("measured 1.0"));
    assert_eq!(int(v1.get("delta_ppm").unwrap()), 45_455);
    // V5: the honest unmeasured — evidence absent, never a fabricated
    // pass (the registry's own forecast fold was unmeasured too).
    let v5 = &levels[4];
    let a5 = as_obj(v5.get("activation").unwrap());
    assert!(
        a5.get("detail")
            .and_then(Json::as_str)
            .unwrap()
            .contains("unmeasured{evidence_absent}"),
        "V5 activation must stay honestly unmeasured"
    );
}

// ── AC2 · the selection packet — deterministic §8.5 decode ──────────

#[test]
fn ac2_selection_packet_outcome() {
    let s = load(SELECTION);
    assert_eq!(
        s.get("schema").and_then(Json::as_str),
        Some("hh-p4-selection/1")
    );
    // `packet_sha256` is the record's content identity — sha256 of
    // canonical-without-the-member + newline (the R7.2 convention).
    let cited = s
        .get("packet_sha256")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let mut core = s.clone();
    if let Json::Obj(ref mut m) = core {
        m.remove("packet_sha256");
    }
    let recomputed = format!(
        "sha256:{}",
        sha256_hex(format!("{}\n", core.to_canonical_string()).as_bytes())
    );
    assert_eq!(cited, recomputed, "packet_sha256 self-hash");

    // The outcome: selected{m=1} — V1 only, rank 1.
    let out = as_obj(s.get("outcome").unwrap());
    assert_eq!(
        out.get("disposition").and_then(Json::as_str),
        Some("selected{m=1}")
    );
    let members = as_arr(out.get("members").unwrap());
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].get("member").and_then(Json::as_str), Some("V1"));
    assert_eq!(int(members[0].get("delta_ppm").unwrap()), 45_455);
    assert!(members[0]
        .get("exploration_label")
        .and_then(Json::as_str)
        .unwrap()
        .contains("exploratory"));

    // The gate table: V1 qualified (every gate a–f pass); V2/V5/V6
    // fail (d) unresolved; V3/V4/V7 fail (c) unmeasured + (d).
    let table = as_arr(out.get("gate_table").unwrap());
    assert_eq!(table.len(), 7);
    let by_id: BTreeMap<String, &Json> = table
        .iter()
        .map(|r| {
            (
                r.get("level_id")
                    .and_then(Json::as_str)
                    .unwrap()
                    .to_string(),
                r,
            )
        })
        .collect();
    let gates = |id: &str| -> &Json { by_id[id].get("gates").unwrap() };
    let pass = |id: &str, g: &str| -> bool {
        gates(id).get(g).unwrap().get("pass").unwrap() == &Json::Bool(true)
    };
    assert!(by_id["V1"].get("qualified").unwrap() == &Json::Bool(true));
    for g in ["a", "b", "c", "d", "e", "f"] {
        assert!(pass("V1", g), "V1 gate {g} must pass");
    }
    for id in ["V2", "V5", "V6"] {
        assert!(!pass(id, "d"), "{id} fails gate (d) unresolved");
        assert_eq!(
            by_id[id].get("qualified").unwrap(),
            &Json::Bool(false),
            "{id} not qualified"
        );
    }
    for id in ["V3", "V4", "V7"] {
        assert!(!pass(id, "c"), "{id} fails gate (c) unmeasured");
        assert!(!pass(id, "d"), "{id} fails gate (d) unresolved");
    }
    // Hash-before-subjects member.
    assert!(s
        .get("predates")
        .and_then(Json::as_str)
        .unwrap()
        .contains("confirmation"));
    assert!(s
        .get("determinism")
        .and_then(Json::as_str)
        .unwrap()
        .contains("byte-identical"));
}

// ── AC3 · the frozen confirmation pre-registration ──────────────────

#[test]
fn ac3_c_prereg_members_and_envelope() {
    let c = load(CPREREG);
    assert_eq!(
        c.get("schema").and_then(Json::as_str),
        Some("hh-p4-c-prereg/1")
    );
    assert_eq!(
        c.get("disposition").and_then(Json::as_str),
        Some("selected{m=1}")
    );

    // The member list is the selection packet's output verbatim:
    // baseline re-run (rank 0 control) + V1 (rank 1). No C₁₂.
    let members = as_arr(c.get("members").unwrap());
    assert_eq!(members.len(), 2);
    assert_eq!(
        members[0].get("member").and_then(Json::as_str),
        Some("baseline")
    );
    assert_eq!(int(members[0].get("rank").unwrap()), 0);
    assert!(members[0]
        .get("role")
        .and_then(Json::as_str)
        .unwrap()
        .contains("control re-run"));
    assert_eq!(members[1].get("member").and_then(Json::as_str), Some("V1"));
    assert_eq!(int(members[1].get("rank").unwrap()), 1);
    assert!(!members
        .iter()
        .any(|m| m.get("member").and_then(Json::as_str) == Some("C₁₂")));

    // The declared envelope: T = 37 held-out band-A census, k = 5,
    // 2 members × 185 = 370 slots, Holm α = .05, seeded schedule.
    let held = as_arr(c.get("held_out").unwrap());
    assert_eq!(held.len(), 37);
    let sel = load("docs/build/reports/2026-11-08-fp-selection.json");
    let mut want: Vec<&Json> = Vec::new();
    for cell in as_arr(sel.get("cells").unwrap()) {
        if cell.get("band").and_then(Json::as_str) == Some("A") {
            want.extend(as_arr(cell.get("held_out").unwrap()));
        }
    }
    assert_eq!(held.len(), want.len());
    for (a, b) in held.iter().zip(want) {
        assert_eq!(a, b, "held_out must equal the frozen census verbatim");
    }
    let d = as_obj(c.get("design").unwrap());
    assert_eq!(int(d.get("k").unwrap()), 5);
    assert_eq!(int(d.get("slots_per_member").unwrap()), 185);
    assert_eq!(int(d.get("total_slots").unwrap()), 370);
    assert!(d
        .get("multiplicity")
        .and_then(Json::as_str)
        .unwrap()
        .contains("Holm"));
    assert!(d
        .get("seed")
        .and_then(Json::as_str)
        .unwrap()
        .starts_with("p4-c-schedule"));

    // §8.7 ordering + honesty members.
    let s = load(SELECTION);
    assert_eq!(
        c.get("selection_packet_sha256").and_then(Json::as_str),
        s.get("packet_sha256").and_then(Json::as_str)
    );
    assert!(c
        .get("hash_order")
        .and_then(Json::as_str)
        .unwrap()
        .contains("predates"));
    assert!(c
        .get("predates")
        .and_then(Json::as_str)
        .unwrap()
        .contains("confirmation subject"));
    assert!(c
        .get("spend")
        .and_then(Json::as_str)
        .unwrap()
        .contains("$0"));
    assert!(c
        .get("cohort")
        .and_then(Json::as_str)
        .unwrap()
        .contains("never run in X0/X1"));
}

// ── AC4 · the confirmation quote — X1-measured figures ──────────────

#[test]
fn ac4_quote_measured_rows() {
    let q = load(QUOTE);
    assert_eq!(
        q.get("schema").and_then(Json::as_str),
        Some("comparative-quote/1")
    );
    assert_eq!(
        q.get("envelope_label").and_then(Json::as_str),
        Some("pre-quote")
    );
    assert_eq!(int(q.get("concurrency_c").unwrap()), 4);

    let rows: BTreeMap<String, &Json> = as_arr(q.get("rows").unwrap())
        .iter()
        .map(|r| {
            (
                r.get("row_id").and_then(Json::as_str).unwrap().to_string(),
                r,
            )
        })
        .collect();
    assert_eq!(rows.len(), 2);
    assert!(rows.contains_key("p4c-baseline") && rows.contains_key("p4c-V1"));
    for (id, r) in &rows {
        assert_eq!(int(r.get("n_trials").unwrap()), 185, "{id}");
        assert_eq!(int(r.get("h").unwrap()), 185, "{id}");
        // Every model figure is X1-MEASURED — never the §10.4 prior.
        let mref = as_arr(r.get("measure_refs").unwrap())[0].as_str().unwrap();
        assert!(mref.contains("X1-measured"), "{id} measure_ref: {mref}");
        assert!(mref.contains("never a declared planning prior"));
        assert_eq!(
            r.get("verdict").and_then(Json::as_str),
            Some("unresolved{sim_absent}")
        );
    }
    // The measured envelopes: baseline mean 46,165/p90 163,525; V1
    // mean 17,934/p90 45,275 — ceiling = 185 × p90 × 1.2.
    assert_eq!(
        int(rows["p4c-baseline"]
            .get("model")
            .unwrap()
            .get("expected_micros")
            .unwrap()),
        8_540_525
    );
    assert_eq!(
        int(rows["p4c-baseline"]
            .get("model")
            .unwrap()
            .get("ceiling_micros")
            .unwrap()),
        36_302_550
    );
    assert_eq!(
        int(rows["p4c-V1"]
            .get("model")
            .unwrap()
            .get("expected_micros")
            .unwrap()),
        3_317_790
    );
    assert_eq!(
        int(rows["p4c-V1"]
            .get("model")
            .unwrap()
            .get("ceiling_micros")
            .unwrap()),
        10_051_050
    );
    // Baseline row arms enumerate every held-out task × replicate.
    let arms = as_arr(rows["p4c-baseline"].get("arms").unwrap());
    assert!(arms
        .iter()
        .all(|a| a.as_str().unwrap().starts_with("baseline:")));
    let varms = as_arr(rows["p4c-V1"].get("arms").unwrap());
    assert!(varms.iter().all(|a| a.as_str().unwrap().starts_with("V1:")));

    // The compute line is a verbatim citation — never re-priced.
    let compute = as_arr(q.get("compute").unwrap());
    assert_eq!(compute.len(), 1);
    assert_eq!(
        compute[0].get("quote_id").and_then(Json::as_str),
        Some("pq-3a4187759b2d8411")
    );
    assert_eq!(int(compute[0].get("ceiling_micros").unwrap()), 12_229_780);
    assert_eq!(compute[0].get("window").and_then(Json::as_str), Some("p4c"));
    assert_eq!(int(q.get("compute_ceiling_micros").unwrap()), 12_229_780);
}

// ── AC5 · the declared-inputs record + compute quote ────────────────

#[test]
fn ac5_quote_inputs_and_compute_record() {
    let i = load(QUOTE_INPUTS);
    assert_eq!(
        i.get("schema").and_then(Json::as_str),
        Some("hh-p4-c-quote-inputs/1")
    );
    // Every cited record's sha256 binds the committed file's bytes —
    // except compute_decl: its declaration legitimately moved post-mint
    // (R7.5 repointed `runner.commit` → 1d89a5a3 under 65931ba4 so the
    // declaration named the pushed tip carrying the p4c machinery). The
    // record is append-only — the as-minted pin stands verbatim; the
    // repointed bytes are pinned below so a further unexplained move
    // trips.
    let recs = as_obj(i.get("records").unwrap());
    for (name, want_schema) in [
        ("c_prereg", "hh-p4-c-prereg/1"),
        ("exploration", "hh-p4-x-exploration/1"),
        ("compute_decl", "hh-compute-decl/1"),
        ("compute_quote", "hh-compute-quote/1"),
    ] {
        let r = as_obj(recs.get(name).unwrap());
        assert_eq!(
            r.get("schema").and_then(Json::as_str),
            Some(want_schema),
            "{name}"
        );
        if name != "compute_decl" {
            let path = r.get("path").and_then(Json::as_str).unwrap();
            assert_eq!(
                r.get("sha256").and_then(Json::as_str).unwrap(),
                sha_file(path).trim_start_matches("sha256:"),
                "{name} sha binding"
            );
        }
    }
    let decl = as_obj(recs.get("compute_decl").unwrap());
    assert_eq!(
        decl.get("sha256").and_then(Json::as_str).unwrap(),
        "ae429ee5d94cb808bb48729a0b344bb23a2f60cb42fc4840f04cdd48dac9301c",
        "the as-minted compute_decl pin stands verbatim"
    );
    assert_eq!(
        sha_file("scripts/gce-backend/declarations/fp-p4c.json").trim_start_matches("sha256:"),
        "cc88d1dd2b4765411c1f892d2f0c763c1dbd0a5ccc065647c46ccc6fc29aa8aa",
        "the R7.5-repointed declaration bytes (65931ba4)"
    );
    assert_eq!(
        as_obj(recs.get("compute_quote").unwrap())
            .get("quote_id")
            .and_then(Json::as_str),
        Some("pq-3a4187759b2d8411")
    );
    // The model-line basis is X1-measured; the packet authorizes
    // nothing; the member list matches the selection disposition.
    assert!(as_obj(i.get("model_line_basis").unwrap())
        .get("basis")
        .and_then(Json::as_str)
        .unwrap()
        .contains("X1-measured"));
    let scen = as_obj(i.get("scenarios").unwrap());
    assert_eq!(
        scen.get("disposition").and_then(Json::as_str),
        Some("selected{m=1}")
    );
    assert_eq!(
        scen.get("selection_conditioned").unwrap(),
        &Json::Bool(true)
    );
    let hon = as_obj(i.get("honesty").unwrap());
    assert_eq!(hon.get("authorizes_nothing").unwrap(), &Json::Bool(true));
    assert!(hon
        .get("model_line_source")
        .and_then(Json::as_str)
        .unwrap()
        .contains("X1-measured"));

    // The compute quote record itself: schema + ceiling agree with
    // the declaration's authorization block (cross-checked by the
    // emitter; the shape pins it here).
    let cq = load_provider(COMPUTE_QUOTE);
    assert_eq!(
        cq.get("schema").and_then(Json::as_str),
        Some("hh-compute-quote/1")
    );
    assert_eq!(cq.get("window").and_then(Json::as_str), Some("p4c"));
    let pricing = as_obj(cq.get("pricing").unwrap());
    assert_eq!(int(pricing.get("ceiling_micros").unwrap()), 12_229_780);
    assert_eq!(int(pricing.get("planning_micros").unwrap()), 10_091_780);
    assert_eq!(int(pricing.get("bound_micros").unwrap()), 34_343_404);
}

// ── AC6 · the GATE-G36 readout — presented PENDING, since signed
//     PASSED (the readout legitimately transitioned at operator
//     signature 8793885e; these pins hold the signed record) ────────

#[test]
fn ac6_gate_readout_pending() {
    let text = std::fs::read_to_string(root().join(READOUT)).unwrap();
    assert!(text.contains("# GATE-G36 readout — G-P4-C"));
    assert!(
        text.contains("\nStatus: PASSED"),
        "the operator signed PASSED (8793885e); the presented PENDING state was observed at mint"
    );
    // REQ-XH-004's required fields: the quote id, the verbatim
    // authorizations (blank until the operator writes them), the
    // hard-cap formulas.
    assert!(text.contains("eaaeda9f114936df83916f0c93e18138f020d6836bc2d4b0c043fb59751405ed"));
    assert!(text.contains("pq-3a4187759b2d8411"));
    assert!(text.contains("36,302,550") && text.contains("10,051,050"));
    assert!(text.contains("12,229,780"));
    assert!(text.contains("Model authorization (verbatim):"));
    assert!(text.contains("Compute authorization (verbatim):"));
    // The frozen prereg hash + member list + the selection packet's
    // predating proof are recorded.
    assert!(text.contains("1d55d48a803c00848c9306c8d2bec4552f98031e52ec80eafa3d8c47ae7e722d"));
    assert!(text.contains("sha256:9c3c314c"));
    assert!(text.contains("`baseline`") && text.contains("`V1`"));
    // The operator signed — verbatim decisions + shas pinned; only
    // the operator may appear on a `Signed by:` line.
    assert!(
        text
            .lines()
            .all(|l| !l.starts_with("Signed by:") || l.contains("the operator.")),
        "only the operator signs"
    );
    assert!(text.contains(
        "Operator decision (verbatim, received 2026-11-24 via Devin CLI session): \"run\""
    ));
    assert!(text.contains("acba25512100f80b56fc3ccd14c65be55d94800cda77585c5f41a887e398f9be"));
    // The post-signature amendment decision is also pinned verbatim.
    assert!(text.contains("Operator decision (verbatim, received 2026-11-25 via Devin CLI session)"));
    assert!(text.contains("718cad8576e1f7122f6f4c7161106364cc15b8966838ef08654722fb37939b30"));
    // Hash-before-subjects stood at presentation: no `p4c1-`/`p4-c-subject`
    // record may exist under that naming (the executed window's
    // records ride the fp-p4c-window naming, a distinct surface).
    for e in std::fs::read_dir(root().join("docs/build/reports")).unwrap() {
        let n = e.unwrap().file_name().to_string_lossy().to_string();
        assert!(
            !n.contains("p4c1-") && !n.contains("p4-c-subject"),
            "a confirmation subject record {n} predates the gate"
        );
    }
}

// ── AC7 · hash-before-subjects ordering — the committed hashes ──────

#[test]
fn ac7_record_hashes_and_sidecars() {
    // The sidecars verify; the packet's content hash is what the
    // prereg cites; the records recompute byte-identical (checker).
    for rel in [
        "docs/build/reports/2026-11-24-fp-p4x-exploration",
        "docs/build/reports/2026-11-24-fp-p4x-selection",
        "docs/build/reports/2026-11-24-fp-p4c-prereg",
        "docs/build/reports/2026-11-24-fp-p4c-quote",
        "docs/build/reports/2026-11-24-fp-p4c-quote-inputs",
    ] {
        let sidecar = std::fs::read_to_string(root().join(format!("{rel}.sha256"))).unwrap();
        let first = sidecar.split_whitespace().next().unwrap();
        assert_eq!(
            first,
            &sha_file(&format!("{rel}.json"))["sha256:".len()..],
            "{rel}"
        );
    }
    let s = load(SELECTION);
    let c = load(CPREREG);
    assert_eq!(
        s.get("packet_sha256").and_then(Json::as_str),
        Some("sha256:9c3c314c2e1db2872dae24c350f93a508c7e08de90c99658957d541ea3ccfe23")
    );
    assert_eq!(
        c.get("selection_packet_sha256").and_then(Json::as_str),
        s.get("packet_sha256").and_then(Json::as_str)
    );
    assert_eq!(
        sha_file(SELECTION),
        "sha256:a1df9b74b3165ad827b308d7b8814e53e6a6c3a68c43947ec1954de9584afc7e"
    );
    assert_eq!(
        sha_file(CPREREG),
        "sha256:1d55d48a803c00848c9306c8d2bec4552f98031e52ec80eafa3d8c47ae7e722d"
    );
    assert_eq!(
        sha_file(EXPLORATION),
        "sha256:dca30f7c4aa812436a5b98877c3e117e94f56bee984b1f024af1544ee08facfc"
    );
    assert_eq!(
        sha_file(QUOTE),
        "sha256:6315e250810593c61bafafb090ca65e7762d6a5e1c0c334efa4db393ad1b196c"
    );
}
