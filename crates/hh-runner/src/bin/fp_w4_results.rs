//! `fp-w4-results` — the R6.53 executed-W4 re-fold emitter (manifest
//! 325b). Reads the committed records and retained-harvest paths named
//! by the inputs manifest (`HH_FP_W4_RESULTS_INPUTS`; default
//! `scripts/fp-w4-results-inputs.txt`, the same `name=relpath` idiom as
//! `fp-results`), folds `hh-fp-w4-outcomes/1`, runs `answer-scan/1`
//! over the scored (verdict-bearing) mini-swe-agent slots, emits the
//! `hh-flagged-review-input/1` handoff to HUMAN-H19, and re-emits the
//! superseding packet set — `hh-comparative-results/1` for P2-FP and
//! P4-FP, `hh-fp-l2/1`, `hh-fp-l2-audit/1`, `hh-fp-l3/1` — each with a
//! sha256 sidecar. Deterministic; no network, no clock.
//!
//! ```text
//! fp-w4-results emit         <repo-root> — the full record set (needs the retained harvest)
//! fp-w4-results emit-packets <repo-root> — the committed-inputs packets only (byte-stable anywhere)
//! fp-w4-results freeze       <repo-root> — mint the frozen site-facing copies (refuses on unreviewed flags)
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use hharness_runner::fp_results::{fingerprint_corpus, CorpusFile, SlotScan};
use hharness_runner::fp_w4::*;
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

fn manifest(path: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .lines()
    {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line
            .split_once('=')
            .unwrap_or_else(|| panic!("manifest line {line:?} is not name=relpath"));
        out.push((k.trim().to_string(), v.trim().to_string()));
    }
    out
}

fn expand(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        let home = std::env::var("HOME").expect("HOME unset for ~ expansion");
        PathBuf::from(home).join(rest)
    } else {
        PathBuf::from(p)
    }
}

/// Read-side projection for committed inputs whose float-free values
/// were spelled with an integral float lexeme (`dead_man_epoch:
/// 1795156200.0` in the W4 window record — written by the R6.52c
/// Python emitter). The committed bytes are never touched (the
/// record's sha256 is over the raw file); the lexeme projects to the
/// integer it denotes. A non-integral float still refuses loudly —
/// never a silent coercion.
fn project_integral(v: hharness_wire::json::PJson) -> Json {
    match v {
        hharness_wire::json::PJson::Null => Json::Null,
        hharness_wire::json::PJson::Bool(b) => Json::Bool(b),
        hharness_wire::json::PJson::Int(i) => Json::Int(i),
        hharness_wire::json::PJson::Num(lex) => lex
            .parse::<f64>()
            .ok()
            .filter(|f| f.fract() == 0.0 && f.is_finite() && f.abs() < 9e15)
            .map(|f| Json::Int(f as i64))
            .unwrap_or_else(|| panic!("non-integral float lexeme {lex:?} in a committed input")),
        hharness_wire::json::PJson::Str(s) => Json::Str(s),
        hharness_wire::json::PJson::Arr(items) => {
            Json::Arr(items.into_iter().map(project_integral).collect())
        }
        hharness_wire::json::PJson::Obj(m) => Json::Obj(
            m.into_iter()
                .map(|(k, v)| (k, project_integral(v)))
                .collect(),
        ),
    }
}

fn load(repo: &Path, rel: &str) -> Json {
    let text =
        std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
    match hharness_wire::json::parse(&text) {
        Ok(j) => j,
        Err(_) => project_integral(
            hharness_wire::json::parse_preserve(&text)
                .unwrap_or_else(|e| panic!("parse {rel}: {e}")),
        ),
    }
}

fn sha256_file(p: &Path) -> String {
    format!(
        "sha256:{}",
        sha256_hex(&std::fs::read(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display())))
    )
}

fn write_record(repo: &Path, rel: &str, j: &Json) {
    let canonical = j.to_canonical_string() + "\n";
    std::fs::write(repo.join(rel), &canonical).unwrap_or_else(|e| panic!("write {rel}: {e}"));
    let hash = sha256_hex(canonical.as_bytes());
    let sidecar = rel.replace(".json", ".sha256");
    std::fs::write(
        repo.join(&sidecar),
        format!(
            "{hash}  {rel}\n# recompute: shasum -a 256 -c {sidecar} (run from the repo root — the record is canonical Json::to_canonical_string output, CC1)\n"
        ),
    )
    .unwrap_or_else(|e| panic!("write {sidecar}: {e}"));
    eprintln!("fp-w4-results: wrote {rel}  sha256:{hash}");
}

fn jstr(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(Json::as_str).map(str::to_string)
}

fn jget<'a>(j: &'a Json, k: &str) -> Option<&'a Json> {
    j.get(k)
}

/// The same two-pass proxy-WAL fold the R6.48 emitter runs — the
/// `mc → budget → slot` link table lands before the requested rows
/// fold (the W4 run `fp-w4-main` carries the same event classes).
fn collect_receipts(runs: &[(String, Vec<u8>)]) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (_run_id, bytes) in runs {
        let text = String::from_utf8(bytes.clone()).expect("WAL utf-8");
        let mut budget_to_slot: BTreeMap<String, String> = BTreeMap::new();
        let mut mc_to_budget: BTreeMap<String, String> = BTreeMap::new();
        for line in text.lines() {
            let Ok(env) = hharness_wire::json::parse(line) else {
                continue;
            };
            let v = env.get("v").cloned().unwrap_or(env);
            let class = jstr(&v, "class").unwrap_or_default();
            let payload = jget(&v, "payload").cloned().unwrap_or(Json::obj([]));
            match class.as_str() {
                "measurement.cost.attributed" => {
                    let attr = jget(&payload, "attribution")
                        .cloned()
                        .unwrap_or(Json::obj([]));
                    let budget = jstr(&attr, "budget_id").unwrap_or_default();
                    let part = jstr(&attr, "participant_ref").unwrap_or_default();
                    if !budget.is_empty() && !part.is_empty() {
                        budget_to_slot.insert(budget, part);
                    }
                }
                "control.budget.reserved" => {
                    let holder = jstr(&payload, "holder").unwrap_or_default();
                    let budget = jstr(&payload, "budget_id").unwrap_or_default();
                    if let Some(mc) = holder.strip_prefix("mc:") {
                        mc_to_budget.insert(mc.to_string(), budget);
                    }
                }
                _ => {}
            }
        }
        for line in text.lines() {
            let Ok(env) = hharness_wire::json::parse(line) else {
                continue;
            };
            let v = env.get("v").cloned().unwrap_or(env);
            if jstr(&v, "class").unwrap_or_default() != "model.call.requested" {
                continue;
            }
            let payload = jget(&v, "payload").cloned().unwrap_or(Json::obj([]));
            let mc = jstr(&payload, "model_call_id").unwrap_or_default();
            let rref = jstr(&payload, "request_ref").unwrap_or_default();
            if let Some(slot) = mc_to_budget.get(&mc).and_then(|b| budget_to_slot.get(b)) {
                out.entry(slot.clone()).or_default().push(rref);
            }
        }
    }
    out
}

/// The scan input for one retained W4 slot dir — returns the built
/// `ScanInput` plus the parsed cell-report (same bundle decode the
/// R6.48 emitter runs).
fn slot_scan_input(
    cell: &Path,
    slot_ref: &str,
    task_id: &str,
    blob_root: &Path,
    receipt_refs: &[String],
    member_digests: &mut Vec<(String, String)>,
) -> (hharness_runner::answer_scan::ScanInput, Json) {
    let mut received: Vec<(String, Option<String>, Vec<String>)> = Vec::new();
    let mut receipt_digest_material = String::new();
    for r in receipt_refs {
        receipt_digest_material.push_str(r);
        receipt_digest_material.push('\n');
        if let Some(hex) = r.strip_prefix("sha256:") {
            let blob = blob_root.join(hex);
            if let Ok(bytes) = std::fs::read(&blob) {
                received.push((
                    String::from_utf8_lossy(&bytes).to_string(),
                    None,
                    Vec::new(),
                ));
            }
        }
    }
    member_digests.push((
        "receipts".into(),
        format!("sha256:{}", sha256_hex(receipt_digest_material.as_bytes())),
    ));
    let report_path = cell.join("cell-report.json");
    let report =
        std::fs::read(&report_path).unwrap_or_else(|e| panic!("{}: {e}", report_path.display()));
    let report_j = hharness_wire::json::parse(&String::from_utf8_lossy(&report))
        .unwrap_or_else(|e| panic!("parse {}: {e}", report_path.display()));
    member_digests.push(("seal".into(), sha256_file(&report_path)));
    let mut egress_rows: Vec<(String, bool)> = Vec::new();
    if let Some(Json::Arr(hosts)) = report_j.get("denied_hosts") {
        for h in hosts {
            if let Some(hs) = h.as_str() {
                egress_rows.push((hs.to_string(), false));
            }
        }
    }
    member_digests.push((
        "egress_rows".into(),
        format!(
            "sha256:{}",
            sha256_hex(format!("{egress_rows:?}").as_bytes())
        ),
    ));
    let mut native_paths: Vec<String> = Vec::new();
    let mut transcript = String::new();
    let mut transcript_name = String::new();
    if let Some(files) = report_j
        .get("native_log")
        .and_then(|n| n.get("files"))
        .and_then(Json::as_obj)
    {
        for (name, meta) in files {
            native_paths.push(name.clone());
            if name.ends_with(".txt") && !name.contains("trajectory") && !name.contains("stdout") {
                transcript_name = name.clone();
                let _ = meta;
            }
        }
        if let Some(man) = report_j
            .get("native_log")
            .and_then(|n| n.get("manifest_sha256"))
            .and_then(Json::as_str)
        {
            member_digests.push(("native_artifacts".into(), man.to_string()));
        }
    }
    if !transcript_name.is_empty() {
        let tp = cell.join("native-log").join(&transcript_name);
        if let Ok(b) = std::fs::read(&tp) {
            transcript = String::from_utf8_lossy(&b).to_string();
            member_digests.push(("terminal_transcript".into(), sha256_file(&tp)));
        }
    }
    (
        hharness_runner::answer_scan::ScanInput {
            slot_ref: slot_ref.to_string(),
            task_id: task_id.to_string(),
            received_requests: received,
            egress_rows,
            seal_present: report_path.exists(),
            native_paths,
            transcript,
        },
        report_j,
    )
}

/// `staging/` → `(kind, name, bytes)` corpus files.
fn corpus_files(cell: &Path) -> Vec<CorpusFile> {
    let mut out: Vec<CorpusFile> = Vec::new();
    for (dir, kind) in [("solution", "solution"), ("tests", "test")] {
        let d = cell.join("staging").join(dir);
        if let Ok(rd) = std::fs::read_dir(&d) {
            let mut names: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
            names.sort();
            for p in names {
                if p.is_file() {
                    if let Ok(b) = std::fs::read(&p) {
                        out.push((
                            kind.to_string(),
                            p.file_name().unwrap().to_string_lossy().to_string(),
                            b,
                        ));
                    }
                }
            }
        }
    }
    out
}

/// The retained fold for one settled slot: reward + the descriptive
/// distribution counters (trajectory messages for the hosted arm;
/// events.json classes for hh_native). Every member that cannot be
/// folded stays `None` — never fabricated.
fn slot_fact(cell: &Path, arm: &str) -> W4SlotFact {
    let mut fact = W4SlotFact {
        reward: None,
        reward_sha256: None,
        steps: None,
        tool_calls: None,
        errors: None,
    };
    let reward_path = cell.join("collect").join("reward.txt");
    if let Ok(s) = std::fs::read_to_string(&reward_path) {
        fact.reward = Some(s.trim() == "1");
        fact.reward_sha256 = Some(sha256_file(&reward_path));
    }
    if arm == "mini-swe-agent" {
        let tp = cell
            .join("native-log")
            .join("mini-swe-agent.trajectory.json");
        if let Ok(b) = std::fs::read(&tp) {
            // the trajectory is provider-format bytes (timestamps and
            // cost carry non-integral floats) — `parse_provider` folds
            // every float lexeme to Str verbatim, never a coercion; the
            // members this fold reads (role, extra.actions) are
            // unaffected.
            if let Ok(t) = hharness_wire::json::parse_provider(&String::from_utf8_lossy(&b)) {
                if let Some(Json::Arr(msgs)) = t.get("messages") {
                    let mut steps = 0i64;
                    let mut calls = 0i64;
                    let mut errs = 0i64;
                    for m in msgs {
                        match m.get("role").and_then(Json::as_str) {
                            Some("assistant") => {
                                steps += 1;
                                calls += m
                                    .get("extra")
                                    .and_then(|e| e.get("actions"))
                                    .and_then(Json::as_arr)
                                    .map(|a| a.len() as i64)
                                    .unwrap_or(1);
                            }
                            Some("tool") => {
                                let rc = m
                                    .get("content")
                                    .and_then(Json::as_str)
                                    .and_then(|c| hharness_wire::json::parse_provider(c).ok())
                                    .and_then(|c| c.get("returncode").and_then(Json::as_int))
                                    .unwrap_or(0);
                                if rc != 0 {
                                    errs += 1;
                                }
                            }
                            _ => {}
                        }
                    }
                    fact.steps = Some(steps);
                    fact.tool_calls = Some(calls);
                    fact.errors = Some(errs);
                }
            }
        }
    } else if arm == "hh_native" {
        let ep = cell.join("events.json");
        if let Ok(b) = std::fs::read(&ep) {
            if let Ok(ev) = hharness_wire::json::parse(&String::from_utf8_lossy(&b)) {
                let events: Vec<&Json> = match &ev {
                    Json::Arr(a) => a.iter().collect(),
                    _ => ev
                        .get("events")
                        .and_then(Json::as_arr)
                        .map(|a| a.iter().collect())
                        .unwrap_or_default(),
                };
                let mut steps = 0i64;
                let mut calls = 0i64;
                let mut errs = 0i64;
                for e in events {
                    match jstr(e, "class").unwrap_or_default().as_str() {
                        "control.decision" => steps += 1,
                        "action.tool.completed" => calls += 1,
                        "action.effect.unknown" => errs += 1,
                        _ => {}
                    }
                }
                fact.steps = Some(steps);
                fact.tool_calls = Some(calls);
                fact.errors = Some(errs);
            }
        }
    }
    fact
}

fn sources_from(m: &[(String, String)], repo: &Path, names: &[&str]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for n in names {
        if let Some((_, rel)) = m.iter().find(|(k, _)| k == n) {
            if rel.starts_with("~/") {
                out.push((format!("retained:{rel}"), sha256_file(&expand(rel))));
            } else {
                out.push((rel.clone(), sha256_file(&repo.join(rel))));
            }
        }
    }
    out
}

/// The packet fold — runs over committed records only (the emit and
/// emit-packets legs produce byte-identical bodies).
fn packets(repo: &Path, m: &[(String, String)]) -> Vec<(String, Json)> {
    let rel_of = |name: &str| -> String {
        m.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("manifest member {name} required"))
    };
    let committed_sources: Vec<(String, String)> = sources_from(
        m,
        repo,
        &[
            "w4_window",
            "fp_e",
            "gl1",
            "prereg",
            "w4_r652c_quote",
            "w4_r652c_spend_bound",
            "w4_settlement_2",
            "resurrect_window",
        ],
    );
    let mut src = committed_sources.clone();
    src.push((
        ADR_0452_PATH.to_string(),
        sha256_file(&repo.join(ADR_0452_PATH)),
    ));
    src.push((
        "docs/build/reports/2026-11-19-fp-w4-outcomes.json".to_string(),
        sha256_file(&repo.join("docs/build/reports/2026-11-19-fp-w4-outcomes.json")),
    ));
    src.push((
        "docs/build/reports/2026-11-19-fp-w4-answer-scan.json".to_string(),
        sha256_file(&repo.join("docs/build/reports/2026-11-19-fp-w4-answer-scan.json")),
    ));
    let sources_json = Json::Arr(
        src.iter()
            .map(|(p, d)| Json::obj([("path", Json::str(p)), ("sha256", Json::str(d))]))
            .collect(),
    );

    let w4_sha = sha256_file(&repo.join(&rel_of("w4_window")));
    let outcomes = load(repo, "docs/build/reports/2026-11-19-fp-w4-outcomes.json");
    let scan = load(repo, "docs/build/reports/2026-11-19-fp-w4-answer-scan.json");
    let rows = slot_outcomes(&outcomes);
    let vetoes = scan_vetoes(&scan);
    let flagged_mini: i64 = rows
        .iter()
        .filter(|r| r.arm == "mini-swe-agent" && vetoes.contains_key(&r.slot_id))
        .count() as i64;

    let fp_e = load(repo, &rel_of("fp_e"));
    let gl1 = fp_e
        .get("disposition")
        .and_then(|d| d.get("gl1"))
        .cloned()
        .unwrap_or(Json::obj([]));
    let prereg = load(repo, &rel_of("prereg"));
    let expected = |claim: &str, dflt: &str| -> String {
        prereg
            .get("claims")
            .and_then(Json::as_arr)
            .and_then(|cs| {
                cs.iter()
                    .find(|c| c.get("id").and_then(Json::as_str) == Some(claim))
                    .and_then(|c| c.get("expected_precision"))
                    .and_then(Json::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| dflt.to_string())
    };

    // ── P2-FP: mini-swe-agent landed (the reference), pi/cc not_run ─
    let mini_rows: Vec<SlotOutcome> = rows
        .iter()
        .filter(|r| r.arm == "mini-swe-agent")
        .cloned()
        .collect();
    let pop = planned_population(&rows, "mini-swe-agent", &vetoes)
        .unwrap_or_else(|e| panic!("mini population: {e:?}"));
    let mut raw_cells_map: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for r in &mini_rows {
        let c = raw_cells_map.entry(r.task.clone()).or_insert((0, 0));
        c.1 += 1;
        if r.pass == Some(true) {
            c.0 += 1;
        }
    }
    let raw_cells: Vec<hharness_eval::comparative_estimators::FlipCell> = raw_cells_map
        .into_iter()
        .map(
            |(task_id, (c, n))| hharness_eval::comparative_estimators::FlipCell {
                task_id,
                c: c as u64,
                n: n as u64,
            },
        )
        .collect();
    let mini_costs: Vec<i64> = mini_rows.iter().map(|r| r.cost_micros).collect();
    let mini_row = landed_arm_row(
        "hh/mini-swe-agent@2.4.6",
        "mini-swe-agent@2.4.6",
        &pop,
        &raw_cells,
        &mini_costs,
        flagged_mini,
        1_000_000,
        &["hh/pi@0.52.3".into(), "hh/claude-code@2.1.50".into()],
    )
    .unwrap_or_else(|e| panic!("mini arm row: {e:?}"));
    let p2_not_run = Json::obj([
        ("planned", Json::Int(50)),
        ("first", Json::Int(0)),
        ("recovered", Json::Int(0)),
        ("unresolved", Json::Int(0)),
        ("withheld", Json::Int(0)),
        ("not_run", Json::Int(50)),
    ]);
    let p2 = w4_results_packet(
        "P2-FP",
        "mini-swe-agent",
        vec![
            (
                "pi".into(),
                typed_arm_row_pub(
                    "hh/pi@0.52.3",
                    "pi@0.52.3",
                    NOT_RUN_AG3,
                    50,
                    0,
                    p2_not_run.clone(),
                    NOT_RUN_AG3,
                    &["hh/mini-swe-agent@2.4.6".into()],
                    false,
                    Some((123_000, "ws1_measured_p_hat")),
                ),
            ),
            (
                "claude-code".into(),
                typed_arm_row_pub(
                    "hh/claude-code@2.1.50",
                    "claude-code@2.1.50",
                    NOT_RUN_AG3,
                    50,
                    0,
                    p2_not_run.clone(),
                    NOT_RUN_AG3,
                    &["hh/mini-swe-agent@2.4.6".into()],
                    false,
                    Some((210_000, "ws1_measured_p_hat")),
                ),
            ),
            ("mini-swe-agent".into(), mini_row),
        ],
        vec![
            hharness_runner::fp_results::family_row(&hharness_runner::fp_results::FamilyMember {
                member: "pi".into(),
                status: "unavailable".into(),
                disposition: Some(NOT_RUN_AG3.into()),
                computed: None,
            }),
            hharness_runner::fp_results::family_row(&hharness_runner::fp_results::FamilyMember {
                member: "claude-code".into(),
                status: "unavailable".into(),
                disposition: Some(NOT_RUN_AG3.into()),
                computed: None,
            }),
        ],
        vec![hharness_runner::fp_results::family_row(
            &hharness_runner::fp_results::FamilyMember {
                member: "pi|claude-code".into(),
                status: "unavailable".into(),
                disposition: Some(NOT_RUN_AG3.into()),
                computed: None,
            },
        )],
        "not_run{admission:AG3}",
        "mini-swe-agent executed its full 50-slot tranche under ADR-0452 \
         (50×rc=0; 29/50 verifier pass — the joint population scores \
         unreviewed flagged passes as failures); pi and claude-code are \
         not_run{admission:AG3} verbatim — the pre-registered Δ contrasts \
         never ran and stay unavailable members",
        &expected("P2-FP", "below the declared precision target — first pass"),
        "2026-11-19-fp-results-p2-fp-frozen.json",
        &w4_sha,
        sources_json.clone(),
    );

    // ── P4-FP: v5 + hh_native executed but unscored ──────────────────
    let native_rows: Vec<&SlotOutcome> = rows.iter().filter(|r| r.arm == "hh_native").collect();
    let v5_rows: Vec<&SlotOutcome> = rows.iter().filter(|r| r.arm == "v5").collect();
    let native_costs: Vec<i64> = native_rows.iter().map(|r| r.cost_micros).collect();
    let native_mean = if native_costs.is_empty() {
        0
    } else {
        native_costs.iter().sum::<i64>() / native_costs.len() as i64
    };
    let native_completion = Json::obj([
        ("planned", Json::Int(50)),
        ("first", Json::Int(50)),
        ("recovered", Json::Int(0)),
        ("unresolved", Json::Int(0)),
        ("withheld", Json::Int(0)),
    ]);
    let v5_completion = Json::obj([
        ("planned", Json::Int(50)),
        ("first", Json::Int(20)),
        ("recovered", Json::Int(0)),
        ("unresolved", Json::Int(0)),
        ("withheld", Json::Int(0)),
        ("dropped_checkpoint", Json::Int(30)),
    ]);
    let native_reason = "unavailable{verifier_recorded_no_verdict} — all 50 \
        verifier execs recorded failures (49×test.sh absent, 1×container \
        improper); no scored outcome exists";
    let v5_reason = "verifier_unavailable{env_closed_or_unmounted} — all 20 \
        settled slots failed document admission with zero model calls; \
        the remaining 30 slots dropped at the round-2 makespan checkpoint";
    let p4 = w4_results_packet(
        "P4-FP",
        "hh_native",
        vec![
            (
                "v5".into(),
                typed_arm_row_pub(
                    "hh/v5@fp",
                    "v5@fp",
                    "unavailable{verifier_unavailable:env_closed_or_unmounted}",
                    50,
                    0,
                    v5_completion,
                    v5_reason,
                    &["hh/hh_native@fp".into()],
                    false,
                    None,
                ),
            ),
            (
                "hh_native".into(),
                typed_arm_row_pub(
                    "hh/hh_native@fp",
                    "hh_native@fp",
                    "unavailable{verifier_recorded_no_verdict}",
                    50,
                    50,
                    native_completion,
                    native_reason,
                    &["hh/v5@fp".into()],
                    true,
                    Some((native_mean, "w4_measured_mean")),
                ),
            ),
        ],
        vec![hharness_runner::fp_results::family_row(
            &hharness_runner::fp_results::FamilyMember {
                member: "v5".into(),
                status: "unavailable".into(),
                disposition: Some(v5_reason.into()),
                computed: None,
            },
        )],
        vec![hharness_runner::fp_results::family_row(
            &hharness_runner::fp_results::FamilyMember {
                member: "v5".into(),
                status: "unavailable".into(),
                disposition: Some(v5_reason.into()),
                computed: None,
            },
        )],
        "unavailable{no_scored_outcome}",
        "the protected pair's H side ran all 50 slots under ADR-0452 \
         (46×rc=0 + 4 recorded rc=1 failures — real outcomes, not \
         withhelds); every verifier exec recorded a failure and no \
         reward exists, so no scored cell lands; V5 settled 20 \
         doc-admission failures (verifier_unavailable) and dropped 30 \
         slots at the round-2 makespan checkpoint — the Δ contrast is \
         honestly unavailable",
        &expected("P4-FP", "single contrast — descriptive precision only"),
        "2026-11-19-fp-results-p4-fp-frozen.json",
        &w4_sha,
        sources_json.clone(),
    );
    let _ = v5_rows;

    // ── P3-L2 / audit / L3 ───────────────────────────────────────────
    let l2 = w4_l2_packet(
        &jstr(&gl1, "pass_id").unwrap_or_default(),
        &jstr(&gl1, "h_identity_digest").unwrap_or_default(),
        &jstr(&gl1, "corpus_digest").unwrap_or_default(),
        &expected("P3-L2", "inconclusive at N=50, ε=±.20 — admissible result"),
        "2026-11-19-fp-results-p3-l2-frozen.json",
        &w4_sha,
        mini_rows.iter().filter(|r| r.pass.is_some()).count() as i64,
        native_rows.iter().filter(|r| r.pass.is_some()).count() as i64,
        sources_json.clone(),
    );
    let w4_o_ids: BTreeSet<String> = mini_rows.iter().map(|r| r.slot_id.clone()).collect();
    let w2_corpus_ids: BTreeSet<String> = (1..=28)
        .map(|i| format!("l10-ws1-corpus-{i:02}-a"))
        .chain((1..=28).map(|i| format!("l10-ws1-corpus-{i:02}-b")))
        .collect();
    let audit = w4_audit_packet(
        &w4_o_ids,
        &w2_corpus_ids,
        AUDIT_SEED,
        "2026-11-19-fp-l2-audit-frozen.json",
        &w4_sha,
        sources_json.clone(),
    )
    .unwrap_or_else(|e| panic!("audit refused: {e:?}"));
    let native_owned: Vec<SlotOutcome> = native_rows.iter().map(|r| (*r).clone()).collect();
    let l3 = w4_l3_report(
        &mini_rows,
        &native_owned,
        "2026-11-19-fp-l3-frozen.json",
        &w4_sha,
        sources_json.clone(),
    );

    let mut out: Vec<(String, Json)> = vec![
        (
            "docs/build/reports/2026-11-19-fp-w4-results-p2-fp.json".into(),
            p2,
        ),
        (
            "docs/build/reports/2026-11-19-fp-w4-results-p4-fp.json".into(),
            p4,
        ),
        (
            "docs/build/reports/2026-11-19-fp-w4-results-p3-l2.json".into(),
            l2,
        ),
        (
            "docs/build/reports/2026-11-19-fp-w4-l2-audit.json".into(),
            audit,
        ),
        ("docs/build/reports/2026-11-19-fp-w4-l3.json".into(), l3),
    ];
    for (_, j) in &out {
        if let Err(errs) = validate_w4_results_packet(j) {
            // the L2/audit/L3 packets validate under their own schema —
            // run the results-packet validator only on the two
            // comparative packets.
            if j.get("schema").and_then(Json::as_str)
                == Some(hharness_eval::comparative_estimators::RESULTS_SCHEMA)
            {
                panic!("validate_w4_results_packet: {errs:?}");
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.shrink_to_fit();
    out
}

/// `typed_arm_row` is module-private in fp_w4 — the bin wraps it via
/// the public re-export (`typed_arm_row_pub` is the module's own
/// `#[doc(hidden)]` shim).
fn typed_arm_row_pub(
    participant: &str,
    intended_participant: &str,
    disposition: &str,
    planned: i64,
    served: i64,
    completion: Json,
    delta_reason: &str,
    swap_refs: &[String],
    is_ref: bool,
    measured_cost: Option<(i64, &str)>,
) -> Json {
    hharness_runner::fp_w4::typed_arm_row(
        participant,
        intended_participant,
        disposition,
        planned,
        served,
        completion,
        delta_reason,
        swap_refs,
        is_ref,
        measured_cost,
    )
}

fn freeze(repo: &Path) {
    let scan = load(repo, "docs/build/reports/2026-11-19-fp-w4-answer-scan.json");
    let scan_sha = sha256_file(&repo.join("docs/build/reports/2026-11-19-fp-w4-answer-scan.json"));
    let input_sha =
        sha256_file(&repo.join("docs/build/reports/2026-11-19-fp-w4-flagged-review-input.json"));
    for src in [
        "2026-11-19-fp-w4-results-p2-fp",
        "2026-11-19-fp-w4-results-p4-fp",
        "2026-11-19-fp-w4-results-p3-l2",
        "2026-11-19-fp-w4-l2-audit",
        "2026-11-19-fp-w4-l3",
    ] {
        let pkt = load(repo, &format!("docs/build/reports/{src}.json"));
        match w4_freeze_packet(&pkt, &scan, &scan_sha, &input_sha) {
            Ok(fz) => write_record(repo, &format!("docs/build/reports/{src}-frozen.json"), &fz),
            Err(reason) => eprintln!("fp-w4-results: freeze {src} refused — {reason}"),
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "emit".into());
    let repo = PathBuf::from(args.next().unwrap_or_else(|| ".".into()));

    if cmd == "freeze" {
        freeze(&repo);
        return;
    }

    let manifest_path = std::env::var("HH_FP_W4_RESULTS_INPUTS").unwrap_or_else(|_| {
        repo.join("scripts/fp-w4-results-inputs.txt")
            .display()
            .to_string()
    });
    let m = manifest(Path::new(&manifest_path));
    let rel_of = |name: &str| -> String {
        m.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("manifest member {name} required"))
    };

    if cmd == "emit-packets" {
        for (rel, j) in packets(&repo, &m) {
            write_record(&repo, &rel, &j);
        }
        return;
    }

    // ── emit: the retained fold first ────────────────────────────────
    let window = load(&repo, &rel_of("w4_window"));
    let retained = expand(&rel_of("retained_root"));
    let run_root = retained.join("run-root-w4");
    let slots_root = run_root.join("w4-main").join("slots");
    let blob_root = run_root.join("proxy-store").join("blobs");
    let wal = run_root
        .join("proxy-store")
        .join("runs")
        .join("fp-w4-main")
        .join("events.wal");
    let wal_bytes = std::fs::read(&wal).unwrap_or_else(|e| panic!("read {}: {e}", wal.display()));
    let receipts = collect_receipts(&[("fp-w4-main".to_string(), wal_bytes.clone())]);

    // per-slot retained facts + the derived v5 dropped set.
    let sr = window
        .get("slot_results")
        .and_then(Json::as_obj)
        .cloned()
        .unwrap_or_default();
    let tasks: BTreeSet<String> = sr
        .values()
        .filter_map(|s| s.get("task").and_then(Json::as_str).map(str::to_string))
        .collect();
    let v5_settled: BTreeSet<String> = sr
        .keys()
        .filter(|k| k.starts_with("w4-v5-"))
        .cloned()
        .collect();
    let v5_dropped: Vec<String> = tasks
        .iter()
        .flat_map(|t| (1..=5).map(move |r| format!("w4-v5-{t}-r{r}")))
        .filter(|id| !v5_settled.contains(id))
        .collect();

    let mut facts: BTreeMap<String, W4SlotFact> = BTreeMap::new();
    let mut planned_per_arm: BTreeMap<String, i64> = BTreeMap::new();
    for arm in ["mini-swe-agent", "hh_native", "v5", "pi", "claude-code"] {
        planned_per_arm.insert(arm.to_string(), 50);
    }
    for sid in sr.keys() {
        let cell = slots_root.join(sid);
        let arm = sr
            .get(sid)
            .and_then(|s| s.get("arm"))
            .and_then(Json::as_str)
            .unwrap_or("");
        facts.insert(sid.clone(), slot_fact(&cell, arm));
    }

    let outcome_sources: Vec<(String, String)> = {
        let mut s = sources_from(
            &m,
            &repo,
            &["w4_window", "w4_settlement_2", "resurrect_window"],
        );
        s.push((
            "retained:run-root-w4/w4-main/slots/*/collect/reward.txt".into(),
            "the retained per-slot reward files — each cited inside its \
             slot row by sha256 (the blob pool idiom: content-addressed, \
             never copied)"
                .into(),
        ));
        s.push((
            format!("retained:run-root-w4/proxy-store/runs/fp-w4-main/events.wal"),
            format!("sha256:{}", sha256_hex(&wal_bytes)),
        ));
        s
    };
    let outcomes = w4_outcomes_record(
        &window,
        &facts,
        &v5_dropped,
        &planned_per_arm,
        outcome_sources,
    );
    write_record(
        &repo,
        "docs/build/reports/2026-11-19-fp-w4-outcomes.json",
        &outcomes,
    );

    // ── the answer scan over the scored (verdict-bearing) slots ─────
    let mut slots: Vec<SlotScan> = Vec::new();
    let mut not_scored: Vec<Json> = Vec::new();
    for (sid, s) in &sr {
        let arm = s.get("arm").and_then(Json::as_str).unwrap_or("");
        let task = s.get("task").and_then(Json::as_str).unwrap_or("");
        let cell = slots_root.join(sid);
        if arm == "mini-swe-agent" {
            let mut member_digests: Vec<(String, String)> = Vec::new();
            let rrefs = receipts.get(sid).cloned().unwrap_or_default();
            let (input, _report) =
                slot_scan_input(&cell, sid, task, &blob_root, &rrefs, &mut member_digests);
            let files = corpus_files(&cell);
            slots.push(SlotScan {
                slot_ref: sid.clone(),
                task_id: task.to_string(),
                arm: arm.to_string(),
                pass: facts.get(sid).and_then(|f| f.reward).unwrap_or(false),
                kind: "subject".into(),
                input,
                corpus: Some(fingerprint_corpus(sid, &files)),
                receipt_refs: rrefs,
                member_digests,
            });
        } else {
            let reason = if arm == "hh_native" {
                format!(
                    "no_scored_outcome{{verifier_exec_failed}} — rc={} settled, \
                     verifier exec recorded, no reward",
                    s.get("rc").and_then(Json::as_int).unwrap_or(-1)
                )
            } else {
                "no_scored_outcome{verifier_unavailable:env_closed_or_unmounted}".to_string()
            };
            not_scored.push(Json::obj([
                ("slot_ref", Json::str(sid)),
                ("arm", Json::str(arm)),
                ("task", Json::str(task)),
                ("reason", Json::str(&reason)),
            ]));
        }
    }
    for id in &v5_dropped {
        not_scored.push(Json::obj([
            ("slot_ref", Json::str(id)),
            ("arm", Json::str("v5")),
            (
                "reason",
                Json::str("dropped{makespan_checkpoint:w4-round-2}"),
            ),
        ]));
    }

    let mut scan_sources: Vec<(String, String)> = sources_from(
        &m,
        &repo,
        &["w4_window", "fp_e", "prereg", "w4_settlement_2"],
    );
    scan_sources.push((
        "retained:run-root-w4/proxy-store/runs/fp-w4-main/events.wal".into(),
        format!("sha256:{}", sha256_hex(&wal_bytes)),
    ));
    scan_sources.push((
        "retained:run-root-w4/proxy-store/blobs (content-addressed receipt bodies)".into(),
        "the blob pool is content-addressed — each request_ref is its own digest".into(),
    ));
    let report = w4_scan_report(&slots, not_scored, scan_sources.clone())
        .unwrap_or_else(|e| panic!("scan report: {e:?}"));
    write_record(
        &repo,
        "docs/build/reports/2026-11-19-fp-w4-answer-scan.json",
        &report,
    );
    eprintln!(
        "fp-w4-results: {} scored trials scanned, {} flagged",
        report
            .get("coverage")
            .and_then(|c| c.get("scanned"))
            .and_then(Json::as_int)
            .unwrap_or(0),
        report
            .get("coverage")
            .and_then(|c| c.get("flagged"))
            .and_then(Json::as_int)
            .unwrap_or(0),
    );

    let input = w4_flagged_review_input(&report, scan_sources);
    write_record(
        &repo,
        "docs/build/reports/2026-11-19-fp-w4-flagged-review-input.json",
        &input,
    );

    // ── the superseding packets (committed-inputs fold) ─────────────
    for (rel, j) in packets(&repo, &m) {
        write_record(&repo, &rel, &j);
    }
}
