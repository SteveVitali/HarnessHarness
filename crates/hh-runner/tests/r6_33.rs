//! R6.33 — the `hh-fp-prereg/1` packet acceptance battery (manifest
//! 291; REQ-XH-111/113/115, REQ-XD-115 primary; REQ-XH-116/127(c)
//! contributing). Fixture-verified, offline, $0.

use std::collections::BTreeMap;

use hharness_eval::comparative_quote::ComputeLine;
use hharness_runner::fp_prereg::*;
use hharness_runner::json_ext::JsonExt;
use hharness_runner::reproof::PACKET_SCHEMA as REPROOF_SCHEMA;
use hharness_runner::schedule::{build_schedule, PlanFact, ScheduleDecl, ScheduleKind};
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

fn repo(rel: &str) -> Json {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    hharness_wire::json::parse(
        &std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")),
    )
    .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

fn ptr<'a>(j: &'a Json, path: &str) -> Option<&'a Json> {
    let mut cur = j;
    for seg in path.trim_start_matches('/').split('/') {
        if let Ok(i) = seg.parse::<usize>() {
            cur = cur.as_arr()?.get(i)?;
        } else {
            cur = cur.get(seg)?;
        }
    }
    Some(cur)
}

fn committed_packet() -> Json {
    repo(PACKET_PATH)
}

fn committed_inputs() -> (
    Json,
    Json,
    BTreeMap<String, String>,
    BTreeMap<String, String>,
) {
    let selection = repo(SELECTION_PATH);
    let commit = repo(SELECT_COMMIT_PATH);
    let mut verdicts = reproof_verdicts(&repo(REPROOF_HELD_PATH)).expect("held packet verdicts");
    verdicts.extend(reproof_verdicts(&repo(REPROOF_DEV_PATH)).expect("dev packet verdicts"));
    let downloads: BTreeMap<String, String> = repo(EXCLUSION_PATH)
        .get("download_tasks")
        .and_then(JsonExt::as_arr)
        .map(|a| {
            a.iter()
                .filter_map(|d| {
                    Some((
                        d.get("task_id")?.as_str()?.to_string(),
                        "excluded_if_drawn_and_replaced".to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    (selection, commit, verdicts, downloads)
}

/// AC-1 — the packet carries the §10.1.3 claim list, the declared
/// seeds recompute selection and schedule, every O/H pair sits in one
/// packing unit in W4's 5 rounds, and the hash predates paid subjects.
#[test]
fn ac1_packet_claims_seeds_and_pair_schedule() {
    let packet = committed_packet();
    assert_eq!(
        packet.get("schema").and_then(Json::as_str),
        Some(PREREG_SCHEMA)
    );
    // the claim list equals §10.1.3's four claims
    let claims: Vec<&str> = packet
        .get("claims")
        .and_then(JsonExt::as_arr)
        .expect("claims")
        .iter()
        .filter_map(|c| c.get("claim").and_then(Json::as_str))
        .collect();
    assert_eq!(claims, vec!["P2-FP", "P3-L1", "P3-L2", "P4-FP"]);
    assert!(packet
        .get("predates")
        .and_then(Json::as_str)
        .is_some_and(|s| s.starts_with("first_paid_subject")));
    // selection recomputes from the committed seed (check-fp-select's
    // emit leg is the byte-identical battery); the packet binds it.
    assert_eq!(
        packet
            .get("split")
            .and_then(|s| s.get("seed"))
            .and_then(Json::as_str),
        Some("fp-select-r6.30-2026-11-08")
    );
    // the schedule fixture: 5 arms × 10 tasks × 5 replicates, composed
    // — every (task, replicate) O/H pair in one unit, 5 blocks.
    let mut plans = Vec::new();
    let arms = ["mini-swe-agent", "hh-native", "pi", "claude-code", "v5"];
    for task in 0..10u32 {
        for rep in 0..5u32 {
            for arm in arms {
                plans.push(PlanFact {
                    run_plan_id: format!("{arm}-{task}-r{rep}"),
                    cell_id: "held_out".to_string(),
                    arm_id: arm.to_string(),
                    task_id: format!("task{task}"),
                    replicate_index: rep,
                    split_label: "held_out".to_string(),
                    order_pos: plans.len() as u64,
                });
            }
        }
    }
    let decl = ScheduleDecl {
        kind: ScheduleKind::Composed,
        rounds: 5,
        blocks: 5,
        seed: SCHEDULE_SEED.to_string(),
        catalog_id: "fixture".to_string(),
    };
    let units = build_schedule(&plans, &decl);
    let units2 = build_schedule(&plans, &decl);
    assert_eq!(units, units2, "schedule does not recompute from the seed");
    assert_eq!(units.len(), 50, "expected 10 tasks × 5 replicates");
    // every unit is one (task, replicate) group — the O/H pair (plus the
    // other arms' slots at that pair) started in one packing step.
    let mut seen = std::collections::BTreeSet::new();
    for u in &units {
        assert!(u.block < 5, "every slot lands in W4's 5 rounds");
        let mut trs = std::collections::BTreeSet::new();
        for m in &u.members {
            let (head, rep) = m.rsplit_once('-').expect("member key");
            let rep: u32 = rep
                .strip_prefix('r')
                .and_then(|r| r.parse().ok())
                .expect("rep");
            let (_arm, task) = head.rsplit_once('-').expect("member key");
            trs.insert((task.to_string(), rep));
        }
        assert_eq!(
            trs.len(),
            1,
            "a packing unit spans more than one (task, replicate)"
        );
        let (t, r) = trs.iter().next().unwrap();
        assert!(
            u.members.contains(&format!("mini-swe-agent-{t}-r{r}"))
                && u.members.contains(&format!("hh-native-{t}-r{r}")),
            "the O/H pair of ({t},{r}) is not one packing unit"
        );
        seen.insert((t.clone(), *r));
    }
    assert_eq!(seen.len(), 50, "every (task, replicate) exactly once");
}

/// AC-2 — the committed freeze over the real W1 register yields the
/// honest blocked state: every slot's exclusion is a typed
/// outcome-blind basis, the next member names the un-re-proofed
/// successor (needs Wc), dev-cell depletion carries REQ-XH-127(c)'s
/// disclosure, and the emptied medium dev cell refuses typed.
#[test]
fn ac2_committed_freeze_blocked_state() {
    let (selection, commit, verdicts, downloads) = committed_inputs();
    let freeze = freeze_post_replacement(&selection, &commit, &verdicts, &downloads)
        .expect("freeze computes");
    assert_eq!(
        freeze.get("state").and_then(Json::as_str),
        Some("blocked"),
        "all-o_fail W1 register → the freeze must be blocked, never a frozen list"
    );
    let mut blocked_unreproofed = 0;
    let mut disclosures = 0;
    let mut no_donor = 0;
    for d in freeze.get("draws").and_then(JsonExt::as_arr).unwrap() {
        assert_eq!(d.get("state").and_then(Json::as_str), Some("blocked"));
        for c in d.get("cells").and_then(JsonExt::as_arr).unwrap() {
            for s in c.get("slots").and_then(JsonExt::as_arr).unwrap() {
                // every drawn member carries its typed outcome-blind basis
                // (a re-proof `exclude:` verdict or a declared download
                // exclusion — never an outcome field).
                assert!(
                    s.get("drawn_basis")
                        .and_then(Json::as_str)
                        .is_some_and(|b| b.starts_with("exclude:runner_infeasible:")
                            || b.starts_with("excluded_if_drawn_and_replaced")),
                    "slot {:?} lacks its typed exclusion basis",
                    s.get("slot")
                );
                match s.get("state").and_then(Json::as_str) {
                    Some("blocked{un_reproofed_successor}") => {
                        blocked_unreproofed += 1;
                        assert_eq!(s.get("needs").and_then(Json::as_str), Some("wc_reproof"));
                    }
                    Some("blocked{no_adjacent_donor}") => no_donor += 1,
                    other => panic!("unexpected slot state {other:?}"),
                }
                if s.get("disclosure").and_then(Json::as_str) == Some("req_xh_127_c_adjacent_cell")
                {
                    disclosures += 1;
                }
            }
        }
    }
    assert_eq!(
        blocked_unreproofed, 14,
        "10 held-out + 4 dev slots each name an un-re-proofed successor"
    );
    assert_eq!(disclosures, 2, "easy and hard dev cells each disclose");
    assert_eq!(no_donor, 3, "the emptied medium dev cell has no donor");
    // the emitted packet carries the same frozen record
    assert_eq!(
        committed_packet()
            .get("task_lists")
            .and_then(|t| t.get("state"))
            .and_then(Json::as_str),
        Some("blocked")
    );
}

/// AC-2 fixture — an un-re-proofed successor refuses typed; an emptied
/// dev cell draws from the adjacent order and the disclosure is
/// present; a fully-admitted cell freezes.
#[test]
fn ac2_fixture_admit_adjacent_refusal() {
    let selection = hharness_wire::json::parse(
        r#"{"draws":[{"name":"d","pool":"dev","draw":{"cells":{
            "easy":{"drawn":["gone"],"replacement_order":[]},
            "medium":{"drawn":["ok1","ok2"],"replacement_order":["donor"]}
        }}}]}"#,
    )
    .unwrap();
    let commit = hharness_wire::json::parse(
        r#"{"adjacent_cell_orders":{"easy":["donor"],"hard":["donor"]}}"#,
    )
    .unwrap();
    let verdicts: BTreeMap<String, String> = [
        (
            "gone".to_string(),
            "exclude:runner_infeasible:oracle_fails_on_O{sha256:aa}".to_string(),
        ),
        ("ok1".to_string(), "admit".to_string()),
        ("ok2".to_string(), "admit".to_string()),
        ("donor".to_string(), "admit".to_string()),
    ]
    .into_iter()
    .collect();
    let freeze = freeze_post_replacement(&selection, &commit, &verdicts, &BTreeMap::new()).unwrap();
    assert_eq!(freeze.get("state").and_then(Json::as_str), Some("frozen"));
    let cells = ptr(&freeze, "/draws/0/cells")
        .and_then(JsonExt::as_arr)
        .unwrap();
    let easy = &cells[0];
    let slot = &easy.get("slots").and_then(JsonExt::as_arr).unwrap()[0];
    assert_eq!(slot.get("member").and_then(Json::as_str), Some("donor"));
    assert_eq!(
        slot.get("disclosure").and_then(Json::as_str),
        Some("req_xh_127_c_adjacent_cell")
    );
    // an un-re-proofed needed successor refuses typed
    let verdicts2: BTreeMap<String, String> = [
        (
            "gone".to_string(),
            "exclude:runner_infeasible:x".to_string(),
        ),
        ("ok1".to_string(), "admit".to_string()),
        ("ok2".to_string(), "admit".to_string()),
    ]
    .into_iter()
    .collect();
    let f2 = freeze_post_replacement(&selection, &commit, &verdicts2, &BTreeMap::new()).unwrap();
    let easy2 = &ptr(&f2, "/draws/0/cells")
        .and_then(JsonExt::as_arr)
        .unwrap()[0];
    assert_eq!(
        easy2.get("slots").and_then(JsonExt::as_arr).unwrap()[0]
            .get("state")
            .and_then(Json::as_str),
        Some("blocked{un_reproofed_successor}")
    );
}

/// AC-3 — the packet declares exactly REQ-XH-113's three reuses and
/// the dependence sentence; nothing else.
#[test]
fn ac3_trial_reuse_exactly_three() {
    let packet = committed_packet();
    let reuses = ptr(&packet, "/trial_reuse/reuses")
        .and_then(JsonExt::as_arr)
        .expect("reuses");
    assert_eq!(reuses.len(), 3);
    assert_eq!(
        ptr(&packet, "/trial_reuse/dependence")
            .and_then(Json::as_str),
        Some("the claims share trials and are dependent; no cross-claim familywise control is claimed")
    );
    let claims: Vec<&str> = packet
        .get("claims")
        .and_then(JsonExt::as_arr)
        .unwrap()
        .iter()
        .filter_map(|c| c.get("claim").and_then(Json::as_str))
        .collect();
    assert_eq!(claims.len(), 4);
}

/// AC-4 — the V5 declaration names factor id, parameters, the Q-X56
/// text verbatim with the pending label and the REQ-XH-115 label;
/// V5 is the lowest-priority arm; the sub-object's own sha256 stands.
#[test]
fn ac4_v5_declaration() {
    let packet = committed_packet();
    let v5 = packet.get("v5_declaration").expect("v5_declaration");
    assert_eq!(
        v5.get("factor_id").and_then(Json::as_str),
        Some("transcript_render.assistant_replay.reasoning")
    );
    assert_eq!(
        ptr(&v5, "/parameters/level").and_then(Json::as_str),
        Some("drop")
    );
    assert_eq!(
        ptr(&v5, "/parameters/baseline").and_then(Json::as_str),
        Some("keep (as O)")
    );
    assert_eq!(
        ptr(&v5, "/q_x56/text").and_then(Json::as_str),
        Some(Q_X56_VERBATIM)
    );
    assert_eq!(
        ptr(&v5, "/q_x56/status").and_then(Json::as_str),
        Some("pending")
    );
    assert_eq!(v5.get("label").and_then(Json::as_str), Some(V5_LABEL));
    assert_eq!(
        v5.get("label_req").and_then(Json::as_str),
        Some("REQ-XH-115")
    );
    // the sub-object's own sha256 over its canonical bytes
    let expected = sha256_hex(v5.to_canonical_string().as_bytes());
    assert_eq!(
        packet.get("v5_declaration_sha256").and_then(Json::as_str),
        Some(expected.as_str())
    );
    // the packet carries no registry hash (the registry postdates it)
    assert!(packet.get("registry_hash").is_none());
    // V5 is the lowest-priority arm (drop order's head)
    let drops = ptr(&packet, "/arm_priority/drop_order")
        .and_then(JsonExt::as_arr)
        .expect("drop_order");
    assert_eq!(drops.first().and_then(Json::as_str), Some("V5"));
    assert_eq!(
        ptr(&packet, "/arm_priority/q_x61/status").and_then(Json::as_str),
        Some("pending")
    );
}

/// AC-5 — ε = ±.20 explicit with Q-X53 and the N = 50 table; a packet
/// or quote built without an explicit ε refuses typed.
#[test]
fn ac5_margin_explicit_or_refusal() {
    let packet = committed_packet();
    assert_eq!(
        ptr(&packet, "/margin/epsilon_ppm").and_then(Json::as_int),
        Some(200_000)
    );
    assert!(ptr(&packet, "/margin/decision")
        .and_then(Json::as_str)
        .is_some_and(|s| s.contains("Q-X53")));
    let table = ptr(&packet, "/margin/n50_p_equivalent_table")
        .and_then(JsonExt::as_arr)
        .expect("the N = 50 table");
    assert_eq!(table.len(), 5, "§10.1.3's five scenario rows");
    assert_eq!(
        require_epsilon(None),
        Err(PreregError::MissingEpsilon),
        "no default margin exists (REQ-XD-115)"
    );
    assert_eq!(require_epsilon(Some(0)), Err(PreregError::MissingEpsilon));
    assert_eq!(require_epsilon(Some(200_000)), Ok(200_000));
}

/// AC-6 — the hash is byte-stable over canonical JSON; mutating any
/// member changes it; the per-arm cap is a µ$ formula with no values.
#[test]
fn ac6_hash_stable_and_cap_formula() {
    let packet = committed_packet();
    let file = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(PACKET_PATH),
    )
    .expect("packet file");
    let re = hharness_wire::json::parse(&file).expect("reparse");
    assert_eq!(re, packet, "the committed packet is canonical JSON");
    assert_eq!(
        packet_sha256(&packet),
        sha256_hex(file.as_bytes()),
        "the packet hash is sha256 over the committed canonical bytes"
    );
    // mutation changes the hash
    let mut m = packet.clone();
    if let Json::Obj(ref mut mm) = m {
        mm.insert("effort".to_string(), Json::str("low"));
    }
    assert_ne!(packet_sha256(&m), packet_sha256(&packet));
    // the cap is a formula, no values
    let cap = ptr(&packet, "/formulas/per_arm_cap").expect("cap");
    assert!(cap
        .get("formula")
        .and_then(Json::as_str)
        .is_some_and(|f| f.contains("p̂_h")));
    assert!(cap
        .get("values")
        .and_then(Json::as_str)
        .is_some_and(|v| v.contains("FP-E")));
    // the fallback trigger is Q-X62 verbatim, pending
    assert_eq!(
        ptr(&packet, "/fallback/q_x62/text").and_then(Json::as_str),
        Some(Q_X62_VERBATIM)
    );
    assert_eq!(
        ptr(&packet, "/fallback/q_x62/status").and_then(Json::as_str),
        Some("pending")
    );
}

/// AC-7 — the W2 quote is a comparative-quote/1 with model and
/// compute lines, Σ ceilings ≤ 20 000 000 µ$, no outcome members.
#[test]
fn ac7_w2_quote_shape() {
    let q = repo(W2_QUOTE_PATH);
    assert_eq!(
        q.get("schema").and_then(Json::as_str),
        Some("comparative-quote/1")
    );
    let rows = q.get("rows").and_then(JsonExt::as_arr).expect("rows");
    assert!(!rows.is_empty(), "model lines required");
    let compute = q.get("compute").and_then(JsonExt::as_arr).expect("compute");
    assert_eq!(compute.len(), 5, "every window's ceiling");
    let sum: i64 = compute
        .iter()
        .filter_map(|c| c.get("ceiling_micros").and_then(Json::as_int))
        .sum();
    assert_eq!(
        sum,
        q.get("compute_ceiling_micros")
            .and_then(Json::as_int)
            .unwrap()
    );
    assert!(sum <= 20_000_000);
    // no task-outcome member anywhere in the artifact (X-07) — row
    // `verdict` members may only carry the unresolved{…} absent state.
    for row in q.get("rows").and_then(JsonExt::as_arr).unwrap() {
        if let Some(v) = row.get("verdict").and_then(Json::as_str) {
            assert!(
                v.starts_with("unresolved{"),
                "a row verdict must stay unresolved — never an outcome: {v}"
            );
        }
    }
    for banned in ["reward", "ctrf", "executed{", "verifier_output", "passed"] {
        assert!(
            !q.to_canonical_string().contains(banned),
            "outcome member {banned} in the quote"
        );
    }
    // the emitted quote recomputes through the same builder
    let lines: Vec<ComputeLine> = compute
        .iter()
        .map(|c| ComputeLine {
            window: c.get("window").and_then(Json::as_str).unwrap().to_string(),
            quote_id: c
                .get("quote_id")
                .and_then(Json::as_str)
                .unwrap()
                .to_string(),
            ceiling_micros: c.get("ceiling_micros").and_then(Json::as_int).unwrap(),
        })
        .collect();
    let rebuilt = w2_quote(lines).expect("rebuild");
    assert_eq!(
        rebuilt.to_json(),
        q,
        "the W2 quote recomputes byte-identically"
    );
}

/// AC-8 — the readiness record: `pending{HUMAN-H14}` while absent;
/// with all preconditions, the readiness instant and trigger instant =
/// readiness + the recorded days; the rung record is never an input.
#[test]
fn ac8_readiness_pending_and_ready() {
    let committed = repo(READINESS_PATH);
    assert_eq!(
        ptr(&committed, "/result/state").and_then(Json::as_str),
        Some("pending{HUMAN-H14 not run}")
    );
    assert_eq!(
        committed.get("schema").and_then(Json::as_str),
        Some(READINESS_SCHEMA)
    );
    // all preconditions present → ready + trigger = instant + days
    let ready = compute_readiness(
        "2026-11-10T00:00:00Z",
        "blocked",
        Some("2026-11-11T00:00:00Z"),
        Some("2026-11-10T12:00:00Z"),
        14,
    );
    assert_eq!(
        ptr(&ready, "/result/state").and_then(Json::as_str),
        Some("ready")
    );
    assert_eq!(
        ptr(&ready, "/result/readiness_instant").and_then(Json::as_str),
        Some("2026-11-11T00:00:00Z"),
        "the readiness instant is the latest precondition instant"
    );
    assert!(ptr(&ready, "/result/trigger_instant")
        .and_then(Json::as_str)
        .is_some_and(|s| s.contains("+ 14 days")));
    // the rung record is never an input
    let inputs = committed.get("inputs").and_then(JsonExt::as_arr).unwrap();
    for i in inputs {
        assert!(!i.to_canonical_string().contains("rung"));
    }
    assert!(committed
        .get("e2_t8_rung_record")
        .and_then(Json::as_str)
        .is_some_and(|s| s.contains("never an input")));
}

/// The re-proof packets decode their verdict maps (AC-2's input).
#[test]
fn reproof_verdicts_decode() {
    let v = reproof_verdicts(&repo(REPROOF_HELD_PATH)).unwrap();
    assert_eq!(v.len(), 13);
    assert!(v
        .values()
        .all(|s| s.starts_with("exclude:runner_infeasible:oracle_fails_on_O{")));
    let d = reproof_verdicts(&repo(REPROOF_DEV_PATH)).unwrap();
    assert_eq!(d.len(), 7);
    assert_eq!(
        repo(REPROOF_HELD_PATH).get("schema").and_then(Json::as_str),
        Some(REPROOF_SCHEMA)
    );
}
