//! `fp-w4x-results` — the R6.57 executed-W4x re-fold emitter (manifest
//! 325f). Reads the committed records and retained-harvest paths named
//! by the inputs manifest (`HH_FP_W4X_RESULTS_INPUTS`; default
//! `scripts/fp-w4x-results-inputs.txt`), folds `hh-fp-w4x-outcomes/1`
//! over the a4 `w4x` + a5 `w4xv` windows, runs `answer-scan/1` over the
//! scored (verdict-bearing) pi/claude-code/v5 slots, emits the
//! `hh-flagged-review-input/1` handoff to HUMAN-H20, and re-emits the
//! superseding packet set — `hh-comparative-results/1` for P2-FP and
//! P4-FP, `hh-fp-l2/1`, `hh-fp-l3/1` — each with a sha256 sidecar.
//! Deterministic; no network, no clock.
//!
//! ```text
//! fp-w4x-results emit         <repo-root> — the full record set (needs the retained harvest)
//! fp-w4x-results emit-packets <repo-root> — the committed-inputs packets only (byte-stable anywhere)
//! fp-w4x-results freeze       <repo-root> — mint the hh-flagged-review/1 record, the
//!                                post-review restored packets (`-post-review`) and the
//!                                frozen site-facing copies (refuses on unreviewed or
//!                                confirmed flags → withheld{flagged_review})
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use hharness_runner::fp_results::{fingerprint_corpus, CorpusFile, SlotScan};
use hharness_runner::fp_w4::{typed_arm_row, SlotOutcome};
use hharness_runner::fp_w4x::*;
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

/// The same integral-float projection the W4 bin runs (the committed
/// window bytes are never touched; the lexeme projects to the integer
/// it denotes; a non-integral float still refuses loudly).
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
    eprintln!("fp-w4x-results: wrote {rel}  sha256:{hash}");
}

fn jstr(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(Json::as_str).map(str::to_string)
}
fn jget<'a>(j: &'a Json, k: &str) -> Option<&'a Json> {
    j.get(k)
}

/// The same two-pass proxy-WAL fold the W4 bin runs — `mc → budget →
/// slot` link table before the requested rows fold.
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

/// The scan input for one retained hosted-arm slot dir — the same
/// `cell-report.json` + receipts decode the W4 bin runs.
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
    let report_j = if report_path.exists() {
        let report = std::fs::read(&report_path)
            .unwrap_or_else(|e| panic!("{}: {e}", report_path.display()));
        let j = hharness_wire::json::parse(&String::from_utf8_lossy(&report))
            .unwrap_or_else(|e| panic!("parse {}: {e}", report_path.display()));
        member_digests.push(("seal".into(), sha256_file(&report_path)));
        j
    } else {
        Json::obj([])
    };
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

/// The retained fold for one w4x hosted-arm slot (pi/claude-code):
/// `collect/reward.txt` + the CTRF summary as the scored-evidence
/// member (each cited by sha256 — the blob-pool idiom).
fn hosted_slot_fact(cell: &Path) -> W4xSlotFact {
    let mut fact = W4xSlotFact::default();
    let reward_path = cell.join("collect").join("reward.txt");
    let ctrf_path = cell.join("collect").join("ctrf.json");
    let reward = std::fs::read_to_string(&reward_path)
        .ok()
        .map(|s| s.trim() == "1");
    if let Some(r) = reward {
        fact.pass = Some(r);
        let mut members: Vec<(&str, Json)> = vec![
            ("reward_file_sha256", Json::str(sha256_file(&reward_path))),
            ("reward_file", Json::str(if r { "1" } else { "0" })),
        ];
        if let Ok(b) = std::fs::read(&ctrf_path) {
            if let Ok(c) = hharness_wire::json::parse(&String::from_utf8_lossy(&b)) {
                let sum = jget(&c, "results")
                    .and_then(|r| r.get("summary"))
                    .cloned()
                    .unwrap_or(Json::obj([]));
                members.push(("ctrf_summary", sum));
                members.push((
                    "ctrf_sha256",
                    Json::str(format!("sha256:{}", sha256_hex(&b))),
                ));
                members.push(("verdict_source", Json::str("ctrf_all_pass")));
            }
        }
        fact.scored = Some(Json::obj(members));
    }
    // the descriptive trajectory fold — pi/claude-code native logs
    // carry the same provider-format transcript shape; fold message
    // counts where the member exists, never fabricate.
    for cand in cell
        .join("native-log")
        .read_dir()
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = cand.file_name().to_string_lossy().to_string();
        if name.ends_with("trajectory.json") {
            if let Ok(b) = std::fs::read(cand.path()) {
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
        }
    }
    fact
}

/// The retained fold for one w4xv (kernel-side v5) slot — the same
/// `events.json` class fold the W4 hh_native slots carried.
fn kernel_slot_fact(cell: &Path) -> W4xSlotFact {
    let mut fact = W4xSlotFact::default();
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

// ── the packet fold (committed-inputs only — byte-stable) ────────────
//
// `packets(repo, m, review)` — `None` is the committed pre-review
// generation (every scan flag a veto; the emitted packets' bytes are
// history and re-emit identically anywhere). `Some(review)` is the
// post-review restored generation: the `hh-flagged-review/1`
// dispositions apply — `cleared` flags score as passes, a `confirmed`
// or undispositioned flag stays a veto — and the restored members
// (theta, counts, Δ, sign-flip + Holm p, leader, integrity) recompute
// through the same estimators. The post-review set emits under
// `-post-review` names; the committed files are never rewritten.
fn packets(repo: &Path, m: &[(String, String)], review: Option<&Json>) -> Vec<(String, Json)> {
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
            "w4x_window",
            "w4xv_window",
            "w4_window",
            "w4_outcomes",
            "fp_e",
            "gl1",
            "prereg",
            "w4x_settlement",
            "w4x_settlement_2",
            "w4xv_settlement",
            "admission_pi",
            "admission_cc",
            "w4_review",
        ],
    );
    let mut src = committed_sources.clone();
    src.push((
        ADR_0453_PATH.to_string(),
        sha256_file(&repo.join(ADR_0453_PATH)),
    ));
    src.push((
        W4X_OUTCOMES_REL.to_string(),
        sha256_file(&repo.join(W4X_OUTCOMES_REL)),
    ));
    src.push((
        W4X_SCAN_REL.to_string(),
        sha256_file(&repo.join(W4X_SCAN_REL)),
    ));
    let sources_json = Json::Arr(
        src.iter()
            .map(|(p, d)| Json::obj([("path", Json::str(p)), ("sha256", Json::str(d))]))
            .collect(),
    );

    let w4x_sha = sha256_file(&repo.join(rel_of("w4x_window")));
    let w4xv_sha = sha256_file(&repo.join(rel_of("w4xv_window")));

    let w4_outcomes = load(repo, W4_OUTCOMES_PATH);
    let w4x_outcomes = load(repo, W4X_OUTCOMES_REL);
    let w4x_scan = load(repo, W4X_SCAN_REL);
    let w4_rows = hharness_runner::fp_w4::slot_outcomes(&w4_outcomes);
    let w4x_rows = hharness_runner::fp_w4::slot_outcomes(&w4x_outcomes);
    // the w4x slot keys are `w4x:<sid>`/`w4xv:<sid>` — the scored
    // populations read the a5 window for v5; the a4 `recorded{127}`
    // rows are history inside the record, never scored members.
    let scored_rows: Vec<SlotOutcome> = w4x_rows
        .iter()
        // v5's scored manifest is the a5 window only — the a4
        // `recorded{127}` rows are append-only history with the same
        // plan refs and are never scored members.
        .filter(|r| {
            (r.arm == "v5" && r.slot_id.starts_with("w4xv:"))
                || (r.arm != "v5" && r.slot_id.starts_with("w4x:"))
        })
        .map(|r| {
            let mut r = r.clone();
            // strip the window tag for the scored manifest — the
            // population keys plan_ref on the raw slot id.
            if let Some((_, rest)) = r.slot_id.split_once(':') {
                r.slot_id = rest.to_string();
            }
            r
        })
        .collect();
    // the committed scan's flag set — `flagged()` counts it and a
    // confirmed/undispositioned flag always stays a veto.
    let flag_vetoes = hharness_runner::fp_w4::scan_vetoes(&w4x_scan);
    // the fold's active veto set: pre-review, every flag; post-review,
    // the flag set minus the review's `cleared` dispositions.
    let vetoes = match review {
        Some(r) => w4x_restored_vetoes(&w4x_scan, r),
        None => flag_vetoes.clone(),
    };
    let dispositions: BTreeMap<String, String> =
        review.map(w4x_review_dispositions).unwrap_or_default();
    let post = review.is_some();
    let flagged_sid = |slot_id: &str| -> bool {
        let raw = slot_id.split_once(':').map(|(_, s)| s).unwrap_or(slot_id);
        flag_vetoes.contains_key(raw) || flag_vetoes.contains_key(slot_id)
    };
    let flagged = |arm: &str| -> i64 {
        w4x_rows
            .iter()
            .filter(|r| r.arm == arm && r.pass.is_some())
            .filter(|r| flagged_sid(&r.slot_id))
            .count() as i64
    };
    // honest per-arm disposition counts over the flagged scored slots —
    // derived from the committed review record, never asserted.
    let dispositioned = |arm: &str, want: &str| -> i64 {
        w4x_rows
            .iter()
            .filter(|r| r.arm == arm && r.pass.is_some())
            .filter(|r| flagged_sid(&r.slot_id))
            .filter(|r| {
                let raw = r
                    .slot_id
                    .split_once(':')
                    .map(|(_, s)| s)
                    .unwrap_or(&r.slot_id);
                dispositions.get(raw).map(String::as_str) == Some(want)
                    || dispositions.get(&r.slot_id).map(String::as_str) == Some(want)
            })
            .count() as i64
    };
    // mini-swe-agent's W4 flags are all cleared by the committed
    // hh-flagged-review/1 record (HUMAN-H19) — no veto survives; the
    // integrity panel records cleared=32 verbatim.
    let mini_cleared: i64 = {
        let rev = load(repo, &rel_of("w4_review"));
        rev.get("coverage")
            .and_then(|c| c.get("cleared"))
            .and_then(Json::as_int)
            .unwrap_or(0)
    };

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

    let no_veto: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let pending_note =
        "the Holm-adjudicated p is pending{flagged_review} — HUMAN-H20's dispositions land first";

    // ── P2-FP: mini (reference), pi, claude-code — all landed ───────
    let mini_pop = w4x_joint_population(&[("mini-swe-agent", &w4_rows, "w4", &no_veto)])
        .unwrap_or_else(|e| panic!("mini population: {e:?}"));
    let pi_pop = w4x_joint_population(&[("pi", &scored_rows, "w4x", &vetoes)])
        .unwrap_or_else(|e| panic!("pi population: {e:?}"));
    let cc_pop = w4x_joint_population(&[("claude-code", &scored_rows, "w4x", &vetoes)])
        .unwrap_or_else(|e| panic!("claude-code population: {e:?}"));
    let joint_p2 = w4x_joint_population(&[
        ("mini-swe-agent", &w4_rows, "w4", &no_veto),
        ("pi", &scored_rows, "w4x", &vetoes),
        ("claude-code", &scored_rows, "w4x", &vetoes),
    ])
    .unwrap_or_else(|e| panic!("p2 joint population: {e:?}"));

    let mini_cells = w4x_raw_cells(&w4_rows, "mini-swe-agent");
    let pi_cells = w4x_raw_cells(&scored_rows, "pi");
    let cc_cells = w4x_raw_cells(&scored_rows, "claude-code");
    let costs = |arm: &str, rows: &[SlotOutcome]| -> Vec<i64> {
        rows.iter()
            .filter(|r| r.arm == arm)
            .map(|r| r.cost_micros)
            .collect()
    };
    let mini_costs = costs("mini-swe-agent", &w4_rows);
    let pi_costs = costs("pi", &scored_rows);
    let cc_costs = costs("claude-code", &scored_rows);

    let mut deg_members: Vec<(&str, Json)> = vec![
        ("ppm", Json::Int(0)),
        (
            "interval",
            Json::obj([
                ("lo_ppm", Json::Int(0)),
                ("hi_ppm", Json::Int(0)),
                ("kind", Json::str("degenerate_reference")),
            ]),
        ),
        ("p_raw_ppm", Json::Int(1_000_000)),
        ("p_holm_ppm", Json::Int(1_000_000)),
        ("label", Json::str("reference arm — Δ ≡ 0 by construction")),
    ];
    if post {
        // the restored generation carries the rejection flag — the
        // reference Δ ≡ 0 at Holm-adjusted p = 1 is never separated.
        deg_members.push(("separated", Json::Bool(false)));
    }
    let deg_delta = Json::obj(deg_members);
    let pending_family = |member: &str| -> Json {
        hharness_runner::fp_results::family_row(&hharness_runner::fp_results::FamilyMember {
            member: member.to_string(),
            status: "unavailable".into(),
            disposition: Some(format!(
                "pending{{flagged_review}} — the paired-cell Δ lands on the arm row; \
                 the Holm-adjudicated member is claimed only after {W4X_REVIEW_INPUT_TO}'s \
                 dispositions record — never over undispositioned flags"
            )),
            computed: None,
        })
    };
    // the post-review confirmatory members: the paired-cell deltas via
    // the shared `paired_deltas` helper (the one counting rule), the
    // sharp-null sign-flip p under the confirmatory-family seed
    // convention, Holm over the primary family and BH over the
    // exploratory pair — the same machinery, never reimplemented.
    let signflip_p = |pop: &hharness_eval::population::PlannedPopulation,
                      member: &str,
                      test: &str,
                      control: &str|
     -> Result<i64, String> {
        let deltas = hharness_eval::comparative_estimators::paired_deltas(
            pop,
            hharness_eval::population::PopulationKind::JointTaskPassWithoutObservedVeto,
            test,
            control,
            None,
        )?;
        hharness_eval::stats::permutation_signflip_p(
            &deltas,
            hharness_eval::task_mixture::SIGNFLIP_DRAWS,
            &format!("comparative-family/1|{W4X_EST_SEED}|{member}|{test}|{control}"),
        )
        .ok_or_else(|| "the paired deltas carry no mass".to_string())
    };
    // the primary + exploratory members, computed post-review; a refused
    // member stays unavailable at worst-case mass (never dropped).
    #[allow(clippy::type_complexity)]
    let (p2_family, p2_expl, pi_p, cc_p, pi_adj, cc_adj): (
        Vec<Json>,
        Vec<Json>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    ) = if post {
        let raw_pi = signflip_p(&joint_p2, "pi", "pi", "mini-swe-agent");
        let raw_cc = signflip_p(&joint_p2, "claude-code", "claude-code", "mini-swe-agent");
        let raw_expl = signflip_p(&joint_p2, "pi|claude-code", "pi", "claude-code");
        let adj = hharness_eval::stats::holm(&[
            raw_pi.clone().unwrap_or(1_000_000),
            raw_cc.clone().unwrap_or(1_000_000),
        ]);
        let eadj =
            hharness_eval::stats::benjamini_hochberg(&[raw_expl.clone().unwrap_or(1_000_000)]);
        let fam_row = |member: &str, raw: &Result<i64, String>, adj_ppm: i64| -> Json {
            match raw {
                Ok(r) => hharness_runner::fp_results::family_row(
                    &hharness_runner::fp_results::FamilyMember {
                        member: member.to_string(),
                        status: "computed".into(),
                        disposition: None,
                        computed: Some((*r, adj_ppm)),
                    },
                ),
                Err(reason) => hharness_runner::fp_results::family_row(
                    &hharness_runner::fp_results::FamilyMember {
                        member: member.to_string(),
                        status: "unavailable".into(),
                        disposition: Some(format!("unavailable{{paired_deltas}} — {reason}")),
                        computed: None,
                    },
                ),
            }
        };
        (
            vec![
                fam_row("pi", &raw_pi, adj[0]),
                fam_row("claude-code", &raw_cc, adj[1]),
            ],
            vec![fam_row("pi|claude-code", &raw_expl, eadj[0])],
            raw_pi.clone().ok(),
            raw_cc.clone().ok(),
            raw_pi.is_ok().then_some(adj[0]),
            raw_cc.is_ok().then_some(adj[1]),
        )
    } else {
        (
            vec![pending_family("pi"), pending_family("claude-code")],
            vec![pending_family("pi|claude-code")],
            None,
            None,
            None,
            None,
        )
    };
    let (pi_delta_p, cc_delta_p) = if post {
        (
            W4xDeltaP::Reviewed {
                raw_ppm: pi_p,
                holm_ppm: pi_adj,
            },
            W4xDeltaP::Reviewed {
                raw_ppm: cc_p,
                holm_ppm: cc_adj,
            },
        )
    } else {
        (
            W4xDeltaP::Pending(pending_note),
            W4xDeltaP::Pending(pending_note),
        )
    };
    // the post-review leader is the argmax restored θ over the scored
    // arms (deterministic arm order; first max on a tie — the same
    // rule `tied_with_leader` applies).
    let leader_p2: Option<&str> = if post {
        let mut best: Option<(&str, i64)> = None;
        for (arm, pop) in [
            ("claude-code", &cc_pop),
            ("mini-swe-agent", &mini_pop),
            ("pi", &pi_pop),
        ] {
            let ppm = w4x_arm_rate(pop, arm)
                .map(|r| r.observed_ppm)
                .unwrap_or(i64::MIN);
            if best.map(|(_, v)| ppm > v).unwrap_or(true) {
                best = Some((arm, ppm));
            }
        }
        best.map(|(a, _)| a)
    } else {
        None
    };
    let leader_p2_participant = leader_p2.map(|a| match a {
        "pi" => "hh/pi@0.52.3",
        "claude-code" => "hh/claude-code@2.1.50",
        _ => "hh/mini-swe-agent@2.4.6",
    });
    let p2_review = |arm: &str| -> W4xReview<'_> {
        match leader_p2_participant {
            Some(participant) => W4xReview::Reviewed {
                participant,
                member: leader_p2 == Some(arm),
                note: "leader over the scored arms post-review — the restored fold \
                       scores cleared flagged passes as passes",
            },
            None => {
                W4xReview::Pending("leader over the scored arms is pending the w4x flagged review")
            }
        }
    };
    let unmetered = "w4x_unmetered — attributed model spend is honest 0 \
        (unmetered_streaming_usage: upstream usage objects absent on every call); \
        the provider hourly buckets bound the true spend inside the window hours \
        — never a zero-cost claim";

    let mini_swaps: Vec<(String, Json, Json)> = vec![
        {
            let (s, e) = w4x_swap_pair("mini-swe-agent", &mini_cells, "pi", &pi_cells);
            ("hh/pi@0.52.3".to_string(), s, e)
        },
        {
            let (s, e) = w4x_swap_pair("mini-swe-agent", &mini_cells, "claude-code", &cc_cells);
            ("hh/claude-code@2.1.50".to_string(), s, e)
        },
    ];
    let mini_row = w4x_landed_arm_row(
        "mini-swe-agent",
        "hh/mini-swe-agent@2.4.6",
        "mini-swe-agent@2.4.6",
        &mini_pop,
        &mini_cells,
        &mini_costs,
        "w4_measured_mean",
        0,
        0,
        mini_cleared,
        1_000_000,
        deg_delta,
        &mini_swaps,
        &p2_review("mini-swe-agent"),
    )
    .unwrap_or_else(|e| panic!("mini arm row: {e:?}"));
    let pi_swaps: Vec<(String, Json, Json)> = vec![{
        let (s, e) = w4x_swap_pair("pi", &pi_cells, "mini-swe-agent", &mini_cells);
        ("hh/mini-swe-agent@2.4.6".to_string(), s, e)
    }];
    let pi_row = w4x_landed_arm_row(
        "pi",
        "hh/pi@0.52.3",
        "pi@0.52.3",
        &pi_pop,
        &pi_cells,
        &pi_costs,
        unmetered,
        flagged("pi"),
        dispositioned("pi", "confirmed"),
        dispositioned("pi", "cleared"),
        1_000_000,
        w4x_delta(&joint_p2, "pi", "mini-swe-agent", &pi_delta_p),
        &pi_swaps,
        &p2_review("pi"),
    )
    .unwrap_or_else(|e| panic!("pi arm row: {e:?}"));
    let cc_swaps: Vec<(String, Json, Json)> = vec![{
        let (s, e) = w4x_swap_pair("claude-code", &cc_cells, "mini-swe-agent", &mini_cells);
        ("hh/mini-swe-agent@2.4.6".to_string(), s, e)
    }];
    let cc_row = w4x_landed_arm_row(
        "claude-code",
        "hh/claude-code@2.1.50",
        "claude-code@2.1.50",
        &cc_pop,
        &cc_cells,
        &cc_costs,
        unmetered,
        flagged("claude-code"),
        dispositioned("claude-code", "confirmed"),
        dispositioned("claude-code", "cleared"),
        1_000_000,
        w4x_delta(&joint_p2, "claude-code", "mini-swe-agent", &cc_delta_p),
        &cc_swaps,
        &p2_review("claude-code"),
    )
    .unwrap_or_else(|e| panic!("claude-code arm row: {e:?}"));

    let p2 = w4x_results_packet(
        "P2-FP",
        "mini-swe-agent",
        vec![
            ("pi".into(), pi_row),
            ("claude-code".into(), cc_row),
            ("mini-swe-agent".into(), mini_row),
        ],
        p2_family,
        p2_expl,
        "executed{adr-0453}",
        if post {
            "all three P2-FP arms executed their full 50-slot tranches — mini-swe-agent \
             under ADR-0452 (W4; its 32 flags cleared by hh-flagged-review/1), pi and \
             claude-code under ADR-0453's a4 extension (50×rc=0 each; CTRF verdicts \
             31/50 and 30/50); HUMAN-H20's hh-flagged-review/1 cleared all 34 \
             flagged passes — the restored fold scores them as passes, the \
             paired-cell Δ lands on each arm row and the Holm members are \
             computed over the restored population"
        } else {
            "all three P2-FP arms executed their full 50-slot tranches — mini-swe-agent \
             under ADR-0452 (W4; its 32 flags cleared by hh-flagged-review/1), pi and \
             claude-code under ADR-0453's a4 extension (50×rc=0 each; CTRF verdicts \
             31/50 and 30/50 — the joint population scores unreviewed flagged passes \
             as failures); the paired-cell Δ lands on each arm row and the Holm \
             members stay pending{flagged_review} until HUMAN-H20 dispositions"
        },
        &expected("P2-FP", "below the declared precision target — first pass"),
        "2026-11-19-fp-w4-results-p2-fp-frozen.json",
        &w4x_sha,
        &w4xv_sha,
        sources_json.clone(),
    );

    // ── P4-FP: v5 landed (a5 scored manifest); hh_native unchanged ──
    let v5_pop = w4x_joint_population(&[("v5", &scored_rows, "w4xv", &vetoes)])
        .unwrap_or_else(|e| panic!("v5 population: {e:?}"));
    let v5_cells = w4x_raw_cells(&scored_rows, "v5");
    let v5_costs = costs("v5", &scored_rows);
    let native_rows: Vec<SlotOutcome> = w4_rows
        .iter()
        .filter(|r| r.arm == "hh_native")
        .cloned()
        .collect();
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
    let native_reason = "unavailable{verifier_recorded_no_verdict} — all 50 \
        verifier execs recorded failures (49×test.sh absent, 1×container \
        improper); no scored outcome exists — the W4 evidence is unchanged \
        and the w4x extension produced no H-side run";
    let joint_p4 = w4x_joint_population(&[
        ("hh_native", &native_rows, "w4", &no_veto),
        ("v5", &scored_rows, "w4xv", &vetoes),
    ])
    .unwrap_or_else(|e| panic!("p4 joint population: {e:?}"));
    let v5_swaps: Vec<(String, Json, Json)> = vec![{
        let (s, e) = w4x_swap_pair("v5", &v5_cells, "hh_native", &[]);
        ("hh/hh_native@fp".to_string(), s, e)
    }];
    let v5_delta_p = if post {
        // the Δ vs hh_native still refuses (no scored H side) — the
        // restored generation keeps its p-members typed, never ints
        // over a refusal.
        W4xDeltaP::Reviewed {
            raw_ppm: None,
            holm_ppm: None,
        }
    } else {
        W4xDeltaP::Pending(pending_note)
    };
    let v5_review = if post {
        W4xReview::Reviewed {
            participant: "hh/v5@corrected-seal",
            member: true,
            note: "leader over the scored arms post-review — the reference arm \
                   produced no scored outcome (a one-arm read)",
        }
    } else {
        W4xReview::Pending("the reference arm produced no scored outcome — no leader read lands")
    };
    let v5_row = w4x_landed_arm_row(
        "v5",
        "hh/v5@corrected-seal",
        "v5@fp",
        &v5_pop,
        &v5_cells,
        &v5_costs,
        "w4xv_measured_mean — proxy-ledger attributed (slot cost_micros)",
        flagged("v5"),
        dispositioned("v5", "confirmed"),
        dispositioned("v5", "cleared"),
        1_000_000,
        w4x_delta(&joint_p4, "v5", "hh_native", &v5_delta_p),
        &v5_swaps,
        &v5_review,
    )
    .unwrap_or_else(|e| panic!("v5 arm row: {e:?}"));
    let p4 = w4x_results_packet(
        "P4-FP",
        "hh_native",
        vec![
            ("v5".into(), v5_row),
            (
                "hh_native".into(),
                typed_arm_row(
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
                disposition: Some(
                    "unavailable{h_side_verdicts_absent} — v5's scored fold landed \
                     under ADR-0453's corrected seal (20/50); the Δ vs hh_native \
                     still carries no scored H side"
                        .into(),
                ),
                computed: None,
            },
        )],
        vec![hharness_runner::fp_results::family_row(
            &hharness_runner::fp_results::FamilyMember {
                member: "v5".into(),
                status: "unavailable".into(),
                disposition: Some(
                    "unavailable{h_side_verdicts_absent} — v5's scored fold landed \
                     under ADR-0453's corrected seal (20/50); the Δ vs hh_native \
                     still carries no scored H side"
                        .into(),
                ),
                computed: None,
            },
        )],
        "executed{adr-0453}",
        if post {
            "v5's corrected-seal evidence superseded its unavailable history: the a5 \
             w4xv window recorded verifier rc=0 on all 50 slots (20×reward=1, \
             30×reward=0) after the a4 machinery absence (recorded{127}, append-only); \
             the w4x flags cleared under the committed hh-flagged-review/1 \
             (HUMAN-H20) — the restored fold lands; hh_native's W4 \
             verifier-machinery failure is unchanged — no scored H side, so the \
             Δ contrast stays honestly unavailable"
        } else {
            "v5's corrected-seal evidence superseded its unavailable history: the a5 \
             w4xv window recorded verifier rc=0 on all 50 slots (20×reward=1, \
             30×reward=0) after the a4 machinery absence (recorded{127}, append-only); \
             hh_native's W4 verifier-machinery failure is unchanged — no scored H \
             side, so the Δ contrast stays honestly unavailable"
        },
        &expected("P4-FP", "single contrast — descriptive precision only"),
        "2026-11-19-fp-w4-results-p4-fp-frozen.json",
        &w4x_sha,
        &w4xv_sha,
        sources_json.clone(),
    );

    // ── P3-L2 / L3 superseding folds ────────────────────────────────
    let l2 = w4x_l2_packet(
        &jstr(&gl1, "pass_id").unwrap_or_default(),
        &jstr(&gl1, "h_identity_digest").unwrap_or_default(),
        &jstr(&gl1, "corpus_digest").unwrap_or_default(),
        &expected("P3-L2", "inconclusive at N=50, ε=±.20 — admissible result"),
        "2026-11-19-fp-w4-results-p3-l2-frozen.json",
        &w4x_sha,
        &w4xv_sha,
        w4_rows
            .iter()
            .filter(|r| r.arm == "mini-swe-agent" && r.pass.is_some())
            .count() as i64,
        native_rows.iter().filter(|r| r.pass.is_some()).count() as i64,
        sources_json.clone(),
    );
    let l3 = w4x_l3_report(
        &[
            (
                "mini-swe-agent".to_string(),
                w4_rows
                    .iter()
                    .filter(|r| r.arm == "mini-swe-agent")
                    .cloned()
                    .collect(),
            ),
            ("hh_native".to_string(), native_rows.clone()),
            (
                "pi".to_string(),
                scored_rows
                    .iter()
                    .filter(|r| r.arm == "pi")
                    .cloned()
                    .collect(),
            ),
            (
                "claude-code".to_string(),
                scored_rows
                    .iter()
                    .filter(|r| r.arm == "claude-code")
                    .cloned()
                    .collect(),
            ),
            (
                "v5".to_string(),
                scored_rows
                    .iter()
                    .filter(|r| r.arm == "v5")
                    .cloned()
                    .collect(),
            ),
        ],
        "2026-11-19-fp-w4-l3-frozen.json",
        &w4x_sha,
        &w4xv_sha,
        sources_json.clone(),
    );

    // the post-review generation emits under `-post-review` names — the
    // committed pre-review packets stay byte-untouched history; the
    // frozen copies wrap the post-review record (the member-wise diff
    // rule then reads exactly `freeze` + `review` + `seal_open_ref`).
    let name = |base: &str| -> String {
        if post {
            format!("docs/build/reports/{base}-post-review.json")
        } else {
            format!("docs/build/reports/{base}.json")
        }
    };
    let mut out: Vec<(String, Json)> = vec![
        (name("2026-11-20-fp-w4x-results-p2-fp"), p2),
        (name("2026-11-20-fp-w4x-results-p4-fp"), p4),
        (name("2026-11-20-fp-w4x-results-p3-l2"), l2),
        (name("2026-11-20-fp-w4x-l3"), l3),
    ];
    for (_, j) in &out {
        if let Err(errs) = validate_w4x_results_packet(j) {
            if j.get("schema").and_then(Json::as_str)
                == Some(hharness_eval::comparative_estimators::RESULTS_SCHEMA)
            {
                panic!("validate_w4x_results_packet: {errs:?}");
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

// ── freeze ───────────────────────────────────────────────────────────

const W4X_PACKET_RELS: &[&str] = &[
    "2026-11-20-fp-w4x-results-p2-fp",
    "2026-11-20-fp-w4x-results-p4-fp",
    "2026-11-20-fp-w4x-results-p3-l2",
    "2026-11-20-fp-w4x-l3",
];

fn freeze(repo: &Path) {
    let scan = load(repo, W4X_SCAN_REL);
    let scan_sha = sha256_file(&repo.join(W4X_SCAN_REL));
    let input_sha = sha256_file(&repo.join(W4X_REVIEW_INPUT_REL));
    let manifest_path = std::env::var("HH_FP_W4X_RESULTS_INPUTS").unwrap_or_else(|_| {
        repo.join("scripts/fp-w4x-results-inputs.txt")
            .display()
            .to_string()
    });
    let m = manifest(Path::new(&manifest_path));
    let flagged = scan
        .get("coverage")
        .and_then(|c| c.get("flagged"))
        .and_then(Json::as_int)
        .unwrap_or(-1);
    if flagged != 0 {
        // withhold — never fabricate a freeze over undispositioned flags.
        if repo.join(W4X_ANALYSIS_REL).exists() {
            let analysis = load(repo, W4X_ANALYSIS_REL);
            let analysis_sha = sha256_file(&repo.join(W4X_ANALYSIS_REL));
            match w4x_flagged_review_record(&scan, &analysis, &scan_sha, &analysis_sha, &input_sha)
            {
                Err(reason) => {
                    for src in W4X_PACKET_RELS {
                        eprintln!(
                            "fp-w4x-results: freeze {src} withheld{{flagged_review}} — {reason}"
                        );
                    }
                }
                Ok(review) => {
                    // all-cleared first — a confirmed or undispositioned
                    // flag stays a veto; nothing post-review mints.
                    if let Err(reason) = w4x_review_verified(&review) {
                        for src in W4X_PACKET_RELS {
                            eprintln!(
                                "fp-w4x-results: freeze {src} withheld{{flagged_review}} — {reason}"
                            );
                        }
                        return;
                    }
                    write_record(repo, W4X_REVIEW_REL, &review);
                    // the post-review restored generation: the review's
                    // cleared dispositions apply inside the fold — the
                    // frozen copy never wraps the vetoed pre-review
                    // packet.
                    let restored: BTreeMap<String, Json> =
                        packets(repo, &m, Some(&review)).into_iter().collect();
                    for src in W4X_PACKET_RELS {
                        let post_rel = format!("docs/build/reports/{src}-post-review.json");
                        let Some(pkt) = restored.get(&post_rel) else {
                            eprintln!(
                                "fp-w4x-results: freeze {src} withheld{{flagged_review}} — \
                                 the restored packet did not emit"
                            );
                            continue;
                        };
                        // the restored emitted packet is its own dated
                        // record — the frozen copy diffs member-wise
                        // against it exactly.
                        write_record(repo, &post_rel, pkt);
                        let pre_rel = format!("docs/build/reports/{src}.json");
                        let restored_fold = matches!(
                            src,
                            &"2026-11-20-fp-w4x-results-p2-fp" | &"2026-11-20-fp-w4x-results-p4-fp"
                        );
                        match w4x_freeze_reviewed_packet(
                            pkt,
                            &review,
                            &scan_sha,
                            &input_sha,
                            &pre_rel,
                            restored_fold,
                        ) {
                            Ok(fz) => write_record(
                                repo,
                                &format!("docs/build/reports/{src}-frozen.json"),
                                &fz,
                            ),
                            Err(reason) => eprintln!(
                                "fp-w4x-results: freeze {src} withheld{{flagged_review}} — {reason}"
                            ),
                        }
                    }
                }
            }
            return;
        }
        for src in W4X_PACKET_RELS {
            eprintln!(
                "fp-w4x-results: freeze {src} withheld{{flagged_review}} — \
                 coverage.flagged {flagged} != 0 and no analysis record exists; \
                 {W4X_REVIEW_INPUT_TO}'s dispositions are owed first"
            );
        }
        return;
    }
    for src in W4X_PACKET_RELS {
        let pkt = load(repo, &format!("docs/build/reports/{src}.json"));
        match w4x_freeze_packet(&pkt, &scan, &scan_sha, &input_sha) {
            Ok(fz) => write_record(repo, &format!("docs/build/reports/{src}-frozen.json"), &fz),
            Err(reason) => eprintln!("fp-w4x-results: freeze {src} refused — {reason}"),
        }
    }
}

// ── emit ─────────────────────────────────────────────────────────────

const W4X_OUTCOMES_REL: &str = "docs/build/reports/2026-11-20-fp-w4x-outcomes.json";
const W4_OUTCOMES_PATH_REL: &str = W4_OUTCOMES_PATH;

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "emit".into());
    let repo = PathBuf::from(args.next().unwrap_or_else(|| ".".into()));

    if cmd == "freeze" {
        freeze(&repo);
        return;
    }

    let manifest_path = std::env::var("HH_FP_W4X_RESULTS_INPUTS").unwrap_or_else(|_| {
        repo.join("scripts/fp-w4x-results-inputs.txt")
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
        for (rel, j) in packets(&repo, &m, None) {
            write_record(&repo, &rel, &j);
        }
        // the post-review generation emits too once the committed
        // `hh-flagged-review/1` record is all-cleared — appended, never
        // rewritten over the pre-review files.
        if repo.join(W4X_REVIEW_REL).exists() {
            let review = load(&repo, W4X_REVIEW_REL);
            if w4x_review_verified(&review).is_ok() {
                for (rel, j) in packets(&repo, &m, Some(&review)) {
                    write_record(&repo, &rel, &j);
                }
            }
        }
        return;
    }

    // ── emit: the retained fold first ────────────────────────────────
    let w4x_window = load(&repo, &rel_of("w4x_window"));
    let w4xv_window = load(&repo, &rel_of("w4xv_window"));
    let retained = expand(&rel_of("retained_root"));
    let retained_v = expand(&rel_of("retained_root_v"));
    let run_root = retained.join("run-root-w4x");
    let slots_root = run_root.join("w4x-a4").join("slots");
    let slots_root_v = retained_v
        .join("run-root-w4xv")
        .join("w4xv-main")
        .join("slots");
    let blob_root = run_root.join("proxy-store-a4").join("blobs");
    let blob_root_v = retained_v
        .join("run-root-w4xv")
        .join("proxy-store")
        .join("blobs");
    let wal = run_root
        .join("proxy-store-a4")
        .join("runs")
        .join("w4x-main")
        .join("events.wal");
    let wal_v = retained_v
        .join("run-root-w4xv")
        .join("proxy-store")
        .join("runs")
        .join("w4xv-main")
        .join("events.wal");
    let wal_bytes = std::fs::read(&wal).unwrap_or_else(|e| panic!("read {}: {e}", wal.display()));
    let wal_v_bytes =
        std::fs::read(&wal_v).unwrap_or_else(|e| panic!("read {}: {e}", wal_v.display()));
    let mut receipts = collect_receipts(&[("w4x-main".to_string(), wal_bytes.clone())]);
    for (k, v) in collect_receipts(&[("w4xv-main".to_string(), wal_v_bytes.clone())]) {
        receipts.entry(k).or_default().extend(v);
    }

    let sr = w4x_window
        .get("slot_results")
        .and_then(Json::as_obj)
        .cloned()
        .unwrap_or_default();
    let srv = w4xv_window
        .get("slot_results")
        .and_then(Json::as_obj)
        .cloned()
        .unwrap_or_default();

    let mut facts: BTreeMap<String, W4xSlotFact> = BTreeMap::new();
    let mut planned_per_arm: BTreeMap<String, i64> = BTreeMap::new();
    for arm in ["pi", "claude-code", "v5"] {
        planned_per_arm.insert(arm.to_string(), 50);
    }
    for (sid, s) in &sr {
        let arm = s.get("arm").and_then(Json::as_str).unwrap_or("");
        let cell = slots_root.join(sid);
        let mut fact = if arm == "v5" {
            W4xSlotFact::default()
        } else {
            hosted_slot_fact(&cell)
        };
        // v5's a4 rows: verifier recorded{127} — machinery absence,
        // no scored outcome; the a5 window discharges it.
        if arm == "v5" {
            fact.scored = None;
            fact.pass = None;
        }
        facts.insert(format!("w4x:{sid}"), fact);
    }
    for (sid, s) in &srv {
        let cell = slots_root_v.join(sid);
        let mut fact = kernel_slot_fact(&cell);
        let reward = s
            .get("verifier")
            .and_then(|v| v.get("reward_file"))
            .and_then(Json::as_str)
            .map(|r| r.trim() == "1");
        if let Some(r) = reward {
            fact.pass = Some(r);
            let mut members: Vec<(&str, Json)> = vec![
                ("reward_file", Json::str(if r { "1" } else { "0" })),
                ("verdict_source", Json::str("verifier_reward_file")),
                ("scored_window", Json::str("w4xv")),
            ];
            let rp = cell.join("collect").join("reward.txt");
            if rp.exists() {
                members.push(("reward_file_sha256", Json::str(sha256_file(&rp))));
            }
            fact.scored = Some(Json::obj(members));
        }
        facts.insert(format!("w4xv:{sid}"), fact);
    }

    let outcome_sources: Vec<(String, String)> = {
        let mut s = sources_from(
            &m,
            &repo,
            &[
                "w4x_window",
                "w4xv_window",
                "w4_outcomes",
                "w4x_settlement",
                "w4xv_settlement",
            ],
        );
        s.push((
            "retained:run-root-w4x/w4x-a4/slots/*/collect/{reward.txt,ctrf.json}".into(),
            "the retained per-slot reward + CTRF verdicts — each cited inside its \
             slot row's scored member by sha256 (content-addressed, never copied)"
                .into(),
        ));
        s.push((
            "retained:run-root-w4x/proxy-store-a4/runs/w4x-main/events.wal".to_string(),
            format!("sha256:{}", sha256_hex(&wal_bytes)),
        ));
        s.push((
            "retained:run-root-w4xv/proxy-store/runs/w4xv-main/events.wal".to_string(),
            format!("sha256:{}", sha256_hex(&wal_v_bytes)),
        ));
        s
    };
    let outcomes = w4x_outcomes_record(
        &w4x_window,
        &w4xv_window,
        &facts,
        &planned_per_arm,
        outcome_sources,
    );
    write_record(&repo, W4X_OUTCOMES_REL, &outcomes);

    // ── the answer scan over the scored (verdict-bearing) slots ─────
    let mut slots: Vec<SlotScan> = Vec::new();
    let mut not_scored: Vec<Json> = Vec::new();
    for (sid, s) in &sr {
        let arm = s.get("arm").and_then(Json::as_str).unwrap_or("");
        let task = s.get("task").and_then(Json::as_str).unwrap_or("");
        let cell = slots_root.join(sid);
        if arm == "v5" {
            // a4's recorded{127} is machinery absence — never scanned
            // as a verdict; the a5 rows carry the scored evidence.
            not_scored.push(Json::obj([
                ("slot_ref", Json::str(sid)),
                ("arm", Json::str("v5")),
                ("task", Json::str(task)),
                (
                    "reason",
                    Json::str(
                        "no_scored_outcome{verifier_recorded_127} — a4 machinery \
                         absence; superseded by the w4xv a5 scored evidence",
                    ),
                ),
            ]));
            continue;
        }
        let mut member_digests: Vec<(String, String)> = Vec::new();
        let rrefs = receipts.get(sid).cloned().unwrap_or_default();
        let (input, _report) =
            slot_scan_input(&cell, sid, task, &blob_root, &rrefs, &mut member_digests);
        let files = corpus_files(&cell);
        slots.push(SlotScan {
            slot_ref: sid.clone(),
            task_id: task.to_string(),
            arm: arm.to_string(),
            pass: facts
                .get(&format!("w4x:{sid}"))
                .and_then(|f| f.pass)
                .unwrap_or(false),
            kind: "subject".into(),
            input,
            corpus: Some(fingerprint_corpus(sid, &files)),
            receipt_refs: rrefs,
            member_digests,
        });
    }
    for (sid, s) in &srv {
        let task = s.get("task").and_then(Json::as_str).unwrap_or("");
        let cell = slots_root_v.join(sid);
        let mut member_digests: Vec<(String, String)> = Vec::new();
        let rrefs = receipts.get(sid).cloned().unwrap_or_default();
        // the v5 slot dirs are the kernel-run layout — no
        // cell-report.json; the scan input records that honestly
        // (seal_present=false) and still folds the proxy receipts.
        let (input, _report) =
            slot_scan_input(&cell, sid, task, &blob_root_v, &rrefs, &mut member_digests);
        slots.push(SlotScan {
            slot_ref: sid.clone(),
            task_id: task.to_string(),
            arm: "v5".into(),
            pass: facts
                .get(&format!("w4xv:{sid}"))
                .and_then(|f| f.pass)
                .unwrap_or(false),
            kind: "subject".into(),
            input,
            corpus: None,
            receipt_refs: rrefs,
            member_digests,
        });
    }

    let mut scan_sources: Vec<(String, String)> = sources_from(
        &m,
        &repo,
        &[
            "w4x_window",
            "w4xv_window",
            "fp_e",
            "prereg",
            "w4x_settlement",
            "w4xv_settlement",
        ],
    );
    scan_sources.push((
        "retained:run-root-w4x/proxy-store-a4/runs/w4x-main/events.wal".into(),
        format!("sha256:{}", sha256_hex(&wal_bytes)),
    ));
    scan_sources.push((
        "retained:run-root-w4xv/proxy-store/runs/w4xv-main/events.wal".into(),
        format!("sha256:{}", sha256_hex(&wal_v_bytes)),
    ));
    scan_sources.push((
        "retained:run-root-w4x/proxy-store-a4/blobs + run-root-w4xv/proxy-store/blobs (content-addressed receipt bodies)".into(),
        "the blob pools are content-addressed — each request_ref is its own digest".into(),
    ));
    let report = w4x_scan_report(&slots, not_scored, scan_sources.clone())
        .unwrap_or_else(|e| panic!("scan report: {e:?}"));
    write_record(&repo, W4X_SCAN_REL, &report);
    eprintln!(
        "fp-w4x-results: {} scored trials scanned, {} flagged",
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

    let input = w4x_flagged_review_input(&report, scan_sources);
    write_record(&repo, W4X_REVIEW_INPUT_REL, &input);

    // ── the superseding packets (committed-inputs fold) ─────────────
    for (rel, j) in packets(&repo, &m, None) {
        write_record(&repo, &rel, &j);
    }
    // the post-review restored generation lands alongside once the
    // committed `hh-flagged-review/1` record is all-cleared.
    if repo.join(W4X_REVIEW_REL).exists() {
        let review = load(&repo, W4X_REVIEW_REL);
        if w4x_review_verified(&review).is_ok() {
            for (rel, j) in packets(&repo, &m, Some(&review)) {
                write_record(&repo, &rel, &j);
            }
        }
    }
    let _ = W4_OUTCOMES_PATH_REL;
    let _ = BTreeSet::<String>::new();
}
