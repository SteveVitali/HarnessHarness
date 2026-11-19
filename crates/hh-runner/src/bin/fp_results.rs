//! `fp-results` — the R6.48 FP-RESULTS emitter. Reads the committed
//! records and retained-store paths named by the inputs manifest
//! (`HH_FP_RESULTS_INPUTS`; default `scripts/fp-results-inputs.txt`,
//! the same `name=relpath` idiom as `fp-e-emit`/`fp-ws1-trials`),
//! opens the outcome seal exactly once (`hh-fp-seal-open/1`), folds
//! the `answer-scan/1` run over every scored trial in the retained
//! harvest into `hh-fp-answer-scan-report/1` plus the REQ-XD-071
//! review handoff, and emits the `re-scope{end-first-pass-early}`
//! packet set — `hh-comparative-results/1` withheld packets for P2-FP
//! and P4-FP, the `hh-fp-l2/1` withheld packet for P3-L2, the
//! `hh-fp-l2-audit/1` guard record and the `hh-fp-l3/1` report — each
//! with a sha256 sidecar. Deterministic; no network, no clock.
//!
//! ```text
//! fp-results emit         <repo-root>   — the full record set (needs the retained store)
//! fp-results emit-packets <repo-root>   — the committed-inputs packets only (byte-stable anywhere)
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use hharness_runner::fp_results::*;
use hharness_runner::fp_seal::SealClass;
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

fn load(repo: &Path, rel: &str) -> Json {
    hharness_wire::json::parse(
        &std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")),
    )
    .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
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
    eprintln!("fp-results: wrote {rel}  sha256:{hash}");
}

fn jstr(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(Json::as_str).map(str::to_string)
}

fn jget<'a>(j: &'a Json, k: &str) -> Option<&'a Json> {
    j.get(k)
}

/// Two passes over a proxy WAL (the `fp_ws1` fold's own ordering —
/// `model.call.requested` precedes `control.budget.reserved`, so the
/// link table must land before the requested rows fold): pass 1 builds
/// `mc → budget → slot` from `control.budget.reserved` +
/// `measurement.cost.attributed`; pass 2 folds
/// `model.call.requested` refs onto their slot in request order.
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

/// The scan input for one retained cell dir — returns the built
/// `ScanInput` plus the parsed cell-report.
fn slot_scan_input(
    cell: &Path,
    slot_ref: &str,
    task_id: &str,
    blob_root: &Path,
    receipt_refs: &[String],
    member_digests: &mut Vec<(String, String)>,
) -> (hharness_runner::answer_scan::ScanInput, Json) {
    // receipts — downstream request bodies verbatim.
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
    // cell-report — denied hosts (requested-only egress) + native
    // manifest + the end-state capture (the bundle's seal role).
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
    // native artifacts + the terminal transcript.
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
    let kinds = [("solution", "solution"), ("tests", "test")];
    for (dir, kind) in kinds {
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

fn read_pass(cell: &Path) -> bool {
    for rel in ["collect/reward.txt", "collect/attempt-1/reward.txt"] {
        if let Ok(s) = std::fs::read_to_string(cell.join(rel)) {
            return s.trim() == "1";
        }
    }
    false
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

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "emit".into());
    let repo = PathBuf::from(args.next().unwrap_or_else(|| ".".into()));
    let manifest_path = std::env::var("HH_FP_RESULTS_INPUTS").unwrap_or_else(|_| {
        repo.join("scripts/fp-results-inputs.txt")
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

    // ── committed sources every packet cites ────────────────────────
    let committed_sources: Vec<(String, String)> = sources_from(
        &m,
        &repo,
        &[
            "trials",
            "pp0",
            "fp_e",
            "measurement",
            "w4_quote",
            "gl1",
            "prereg",
        ],
    );
    let sources_json = Json::Arr(
        committed_sources
            .iter()
            .map(|(p, d)| Json::obj([("path", Json::str(p)), ("sha256", Json::str(d))]))
            .collect(),
    );

    // ── the seal-open record ────────────────────────────────────────
    let mut checks: Vec<SealCheck> = Vec::new();
    for (name, class) in [
        ("trials", SealClass::AgentSlot),
        ("pp0", SealClass::Pp0AgEvidence),
        ("measurement", SealClass::AgentSlot),
        ("gl1", SealClass::AgentSlot),
        ("prereg", SealClass::AgentSlot),
    ] {
        checks.push(SealCheck {
            path: rel_of(name),
            record: load(&repo, &rel_of(name)),
            class,
        });
    }
    // the retained PP0 evidence snapshots (masked `outcome` members —
    // the strongest firewall leg).
    for key in ["pp0_evidence_main", "pp0_evidence_leasefence"] {
        if let Some((_, rel)) = m.iter().find(|(k, _)| k == key) {
            let dir = expand(rel);
            if dir.is_dir() {
                let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
                    .unwrap()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
                    .collect();
                files.sort();
                for f in files {
                    let rec =
                        hharness_wire::json::parse(&std::fs::read_to_string(&f).unwrap()).unwrap();
                    checks.push(SealCheck {
                        // store-relative citation — never an operator-local path.
                        path: format!(
                            "retained:{key}/{}",
                            f.file_name().unwrap().to_string_lossy()
                        ),
                        record: rec,
                        class: SealClass::Pp0AgEvidence,
                    });
                }
            }
        }
    }
    let cited = vec![
        (
            rel_of("fp_e"),
            "the FP-E checkpoint — cost fields, dispositions and machinery quote \
             rows only; its `comparative_quote.rows[].verdict` names the quote row's \
             status, not an agent-slot outcome, so it is cited rather than \
             member-name checked"
                .to_string(),
        ),
        (
            rel_of("w4_quote"),
            "the W4 compute-quote re-issue — ceiling/price rows only (carries \
             provider float spellings); no agent-slot outcome members"
                .to_string(),
        ),
        (
            "docs/build/readouts/GATE-G29.md".to_string(),
            "masking — no first-pass outcome field is cited (the readout's own \
             recorded evidence leg)"
                .to_string(),
        ),
        (
            "docs/build/readouts/GATE-G30.md".to_string(),
            "masking — no first-pass outcome field is cited (the readout's own \
             recorded evidence leg)"
                .to_string(),
        ),
        (
            "docs/build/runs/R6.47.md".to_string(),
            "phase-close record — unused{scope_end}/n-a{scope_end} spellings; no \
             outcome members"
                .to_string(),
        ),
        (
            "docs/build/reports/2026-11-19-fp-ws1-measurement.json".to_string(),
            "the measurement packet — cost fields only, mechanically checked \
             above"
                .to_string(),
        ),
    ];
    let seal_open = seal_open_record(&checks, &cited).unwrap_or_else(|e| {
        panic!("seal-open refused — an outcome field preceded the open act: {e:?}")
    });

    // ── the verdict-free packet legs (committed inputs only) ────────
    let fp_e = load(&repo, &rel_of("fp_e"));
    let gl1 = fp_e
        .get("disposition")
        .and_then(|d| d.get("gl1"))
        .cloned()
        .unwrap_or(Json::obj([]));
    let prereg = load(&repo, &rel_of("prereg"));
    let p2_expected = prereg
        .get("claims")
        .and_then(Json::as_arr)
        .and_then(|cs| {
            cs.iter()
                .find(|c| c.get("id").and_then(Json::as_str) == Some("P2-FP"))
                .and_then(|c| c.get("expected_precision"))
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "below the declared precision target — first pass".into());
    let p4_expected = prereg
        .get("claims")
        .and_then(Json::as_arr)
        .and_then(|cs| {
            cs.iter()
                .find(|c| c.get("id").and_then(Json::as_str) == Some("P4-FP"))
                .and_then(|c| c.get("expected_precision"))
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "single contrast — descriptive precision only".into());
    let l2_expected = prereg
        .get("claims")
        .and_then(Json::as_arr)
        .and_then(|cs| {
            cs.iter()
                .find(|c| c.get("id").and_then(Json::as_str) == Some("P3-L2"))
                .and_then(|c| c.get("expected_precision"))
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "inconclusive at N=50, ε=±.20 — admissible result".into());
    let _ = &fp_e;

    let p2 = withheld_results_packet(
        "P2-FP",
        "mini-swe-agent",
        &p2_arms(),
        &p2_family(),
        &p2_exploratory(),
        &p2_expected,
        sources_json.clone(),
    );
    let p4 = withheld_results_packet(
        "P4-FP",
        "hh_native",
        &p4_arms(),
        &p4_family(),
        &p4_exploratory(),
        &p4_expected,
        sources_json.clone(),
    );
    let l2 = l2_withheld_packet(
        &jstr(&gl1, "pass_id").unwrap_or_default(),
        &jstr(&gl1, "h_identity_digest").unwrap_or_default(),
        &jstr(&gl1, "corpus_digest").unwrap_or_default(),
        &l2_expected,
        sources_json.clone(),
    );
    let w2_corpus_ids: BTreeSet<String> = (1..=28)
        .map(|i| format!("l10-ws1-corpus-{i:02}-a"))
        .chain((1..=28).map(|i| format!("l10-ws1-corpus-{i:02}-b")))
        .collect();
    let audit = l2_audit_packet(
        &[],
        &BTreeSet::new(),
        &w2_corpus_ids,
        0xfeed,
        sources_json.clone(),
    )
    .unwrap_or_else(|e| panic!("audit refused: {e:?}"));
    let l3 = l3_report(sources_json.clone());

    if cmd == "emit-packets" {
        write_record(
            &repo,
            "docs/build/reports/2026-11-19-fp-results-p2-fp.json",
            &p2,
        );
        write_record(
            &repo,
            "docs/build/reports/2026-11-19-fp-results-p4-fp.json",
            &p4,
        );
        write_record(
            &repo,
            "docs/build/reports/2026-11-19-fp-results-p3-l2.json",
            &l2,
        );
        write_record(
            &repo,
            "docs/build/reports/2026-11-19-fp-l2-audit.json",
            &audit,
        );
        write_record(&repo, "docs/build/reports/2026-11-19-fp-l3.json", &l3);
        return;
    }

    write_record(
        &repo,
        "docs/build/reports/2026-11-19-fp-seal-open.json",
        &seal_open,
    );
    write_record(
        &repo,
        "docs/build/reports/2026-11-19-fp-results-p2-fp.json",
        &p2,
    );
    write_record(
        &repo,
        "docs/build/reports/2026-11-19-fp-results-p4-fp.json",
        &p4,
    );
    write_record(
        &repo,
        "docs/build/reports/2026-11-19-fp-results-p3-l2.json",
        &l2,
    );
    write_record(
        &repo,
        "docs/build/reports/2026-11-19-fp-l2-audit.json",
        &audit,
    );
    write_record(&repo, "docs/build/reports/2026-11-19-fp-l3.json", &l3);

    // ── the answer-scan fold over the retained harvest ──────────────
    let retained = expand(&rel_of("retained_root"));
    let harvest = retained.join("harvest");
    let blob_root = harvest.join("proxy-store").join("blobs");
    let wal_names = [
        "fp-ws1-pp0",
        "fp-ws1-corpus",
        "fp-ws1-corpus-b",
        "fp-ws1-corpus-c",
    ];
    let mut runs: Vec<(String, Vec<u8>)> = Vec::new();
    for n in wal_names {
        let wal = harvest
            .join("proxy-store")
            .join("runs")
            .join(n)
            .join("events.wal");
        runs.push((
            n.to_string(),
            std::fs::read(&wal).unwrap_or_else(|e| panic!("read {}: {e}", wal.display())),
        ));
    }
    let receipts = collect_receipts(&runs);

    // slot → (arm, task) from the committed hh-fp-trials/1 record.
    let trials = load(&repo, &rel_of("trials"));
    let mut slot_map: BTreeMap<String, (String, String)> = BTreeMap::new();
    if let Some(Json::Arr(ts)) = trials.get("trials") {
        for t in ts {
            let slot = jstr(t, "slot").unwrap_or_default();
            let arm = jstr(t, "arm").unwrap_or_default();
            let task = jstr(t, "task").unwrap_or_default();
            slot_map.insert(slot, (arm, task));
        }
    }
    // l10 corpus-task map: l10-ws1-corpus-NN-* runs corpus task NN.
    let mut corpus_task: BTreeMap<String, String> = BTreeMap::new();
    for (slot, (_arm, task)) in &slot_map {
        if let Some(nn) = slot.strip_prefix("ws1-corpus-") {
            corpus_task.insert(nn.to_string(), task.clone());
        }
    }

    let mut slots: Vec<SlotScan> = Vec::new();
    let mut cell_dirs: Vec<(PathBuf, String, String)> = Vec::new(); // (dir, slot, kind)
    for sub in ["pp0", "corpus"] {
        let d = harvest.join(sub);
        if let Ok(rd) = std::fs::read_dir(&d) {
            let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
            dirs.sort();
            for c in dirs {
                if c.is_dir() {
                    cell_dirs.push((
                        c.clone(),
                        c.file_name().unwrap().to_string_lossy().to_string(),
                        "subject".into(),
                    ));
                }
            }
        }
    }
    // the l1-corpus replay recordings (W2-corpus-class cells).
    if let Ok(rd) = std::fs::read_dir(&harvest) {
        let mut dirs: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("l10-ws1-corpus-")
                    && p.file_name().unwrap().to_string_lossy().ends_with("-cell")
            })
            .collect();
        dirs.sort();
        for c in dirs {
            let name = c.file_name().unwrap().to_string_lossy().to_string();
            cell_dirs.push((
                c.clone(),
                name.trim_end_matches("-cell").to_string(),
                "l1_corpus_recording".into(),
            ));
        }
    }

    for (cell, slot_ref, kind) in &cell_dirs {
        let (arm, task) = if let Some((a, t)) = slot_map.get(slot_ref) {
            (a.clone(), t.clone())
        } else {
            // l10-ws1-corpus-NN-{a|b} → corpus task NN.
            let nn = slot_ref
                .trim_start_matches("l10-ws1-corpus-")
                .rsplit_once('-')
                .map(|(n, _)| n.to_string())
                .unwrap_or_default();
            (
                "mini-swe-agent".to_string(),
                corpus_task.get(&nn).cloned().unwrap_or_else(|| nn.clone()),
            )
        };
        let mut member_digests: Vec<(String, String)> = Vec::new();
        let rrefs = receipts.get(slot_ref).cloned().unwrap_or_default();
        let (input, _report) = slot_scan_input(
            cell,
            slot_ref,
            &task,
            &blob_root,
            &rrefs,
            &mut member_digests,
        );
        let files = corpus_files(cell);
        slots.push(SlotScan {
            slot_ref: slot_ref.clone(),
            task_id: task,
            arm,
            pass: read_pass(cell),
            kind: kind.clone(),
            input,
            corpus: Some(fingerprint_corpus(slot_ref, &files)),
            receipt_refs: rrefs,
            member_digests,
        });
    }

    let mut scan_sources = committed_sources.clone();
    for (run_id, bytes) in &runs {
        scan_sources.push((
            format!("retained:proxy-store/runs/{run_id}/events.wal"),
            format!("sha256:{}", sha256_hex(bytes)),
        ));
    }
    scan_sources.push((
        "retained:proxy-store/blobs (9345 content-addressed receipt bodies)".into(),
        "the blob pool is content-addressed — each request_ref is its own digest".into(),
    ));
    let report = scan_report(&slots, scan_sources).unwrap_or_else(|e| panic!("scan report: {e:?}"));
    write_record(
        &repo,
        "docs/build/reports/2026-11-19-fp-answer-scan.json",
        &report,
    );
    eprintln!(
        "fp-results: {} scored trials scanned",
        report
            .get("coverage")
            .and_then(|c| c.get("scanned"))
            .and_then(Json::as_int)
            .unwrap_or(0)
    );
}
