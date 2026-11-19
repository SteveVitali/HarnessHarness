//! `hh-fp-trials/1` extraction from the retained Ws1 proxy WALs
//! (R6.46 deliverable: the W2 measurement packet's input record).
//!
//! The extractor reads the four retained proxy WAL runs —
//! `fp-ws1-pp0`, `fp-ws1-corpus`, `fp-ws1-corpus-b` (the aborted
//! crash-recovery run) and `fp-ws1-corpus-c` (the clean rerun for
//! slots 26–28) — plus the corpus manifest and the PP0 AG evidence,
//! and emits committed trial rows carrying cost/timing and machinery
//! members only. No outcome field is ever read or emitted: the
//! emitted record is designed to pass `fp_measure`'s
//! [`crate::fp_measure`] firewall scan, and the fold here never
//! touches a `result`/`verdict`/`outcome` member of any input row.
//!
//! Linkage inside one WAL:
//!   `control.budget.reserved.payload.holder = "mc:<model_call_id>"`
//!   maps a call to its `budget_id`; `measurement.cost.attributed`
//!   carries `attribution.{budget_id, participant_ref}` and the
//!   metered `money.micro_units`. Call timing lives on
//!   `model.call.completed/failed.payload.timing`.
//!
//! Per-trial `wall_ms` is the slot's in-run call span sum (first
//! `received_ms` → last `receipted_ms` per run, summed over the runs
//! the slot contributed to — the crash gap between `corpus-b` and
//! `corpus-c` is not trial wall time). `cost_micros` sums every
//! attributed row for the slot across runs — aborted-attempt spend
//! was real spend.

use hharness_wire::json::Json;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::json_ext::JsonExt;

/// The emitted record's fold provenance member.
const FOLD_EXTRACTOR: &str = "fp_ws1_trials/1 (retained WAL fold; cost fields only)";

fn jget<'a>(j: &'a Json, key: &str) -> Option<&'a Json> {
    j.as_obj().and_then(|m| m.get(key))
}

fn jint(j: &Json, key: &str) -> Option<i64> {
    jget(j, key).and_then(Json::as_int)
}

fn jstr(j: &Json, key: &str) -> Option<String> {
    jget(j, key).and_then(Json::as_str).map(str::to_string)
}

/// One PP0/corpus slot's committed evidence row — arm, task and the
/// admission/machinery members the trials row echoes verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotEvidence {
    /// The slot id (`ws1-pp0-N` / `ws1-corpus-NN`).
    pub slot: String,
    /// The arm (`FpArm` spelling).
    pub arm: String,
    /// The task.
    pub task: String,
    /// `denied_hosts` — egress-denial evidence (infra member).
    pub denied_hosts: Vec<String>,
    /// `agent_end_cause` — the exit-state admission evidence.
    pub agent_end_cause: String,
    /// `receipt_count` — the receipts the slot emitted.
    pub receipt_count: i64,
}

/// Union-of-failures merge over the retained PP0 evidence snapshots:
/// a check that failed in any retained evaluation stays failed
/// (append-only; a re-evaluation never rescues a recorded failure).
/// Each input row is one `hh-pp0-ag-evidence/1` document carrying
/// `harness`, `task`, `trial_ref`, `checks` (`AG<n> → pass|fail`),
/// `denied_hosts`, `agent_end_cause` and `receipt_count` — the
/// `outcome` member is never read.
pub fn merge_pp0_snapshots(snapshots: &[Vec<Json>]) -> BTreeMap<String, Json> {
    let mut merged: BTreeMap<String, Json> = BTreeMap::new();
    for rows in snapshots {
        for r in rows {
            let m = r.as_obj().cloned().unwrap_or_default();
            let id = m
                .get("trial_ref")
                .or_else(|| m.get("slot_id"))
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            let slot = id.rsplit('/').next().unwrap_or(&id).to_string();
            let entry = merged.entry(slot.clone()).or_insert_with(|| {
                Json::obj([
                    ("trial_ref", Json::str(id.as_str())),
                    ("slot", Json::str(slot.as_str())),
                    ("harness", m.get("harness").cloned().unwrap_or(Json::Null)),
                    ("task", m.get("task").cloned().unwrap_or(Json::Null)),
                    ("infra_failed", Json::Bool(false)),
                    ("ag", Json::obj([])),
                    (
                        "denied_hosts",
                        m.get("denied_hosts").cloned().unwrap_or(Json::Arr(vec![])),
                    ),
                    (
                        "agent_end_cause",
                        m.get("agent_end_cause").cloned().unwrap_or(Json::Null),
                    ),
                    (
                        "receipt_count",
                        m.get("receipt_count").cloned().unwrap_or(Json::Int(0)),
                    ),
                ])
            });
            if let Json::Obj(e) = entry {
                if let Some(Json::Obj(ag)) = e.get_mut("ag") {
                    if let Some(cm) = m
                        .get("checks")
                        .or_else(|| m.get("ag"))
                        .and_then(Json::as_obj)
                    {
                        for (g, res) in cm {
                            let cur = ag.get(g).and_then(Json::as_str).unwrap_or("");
                            let nxt = res.as_str().unwrap_or("");
                            if cur == "fail" || nxt.is_empty() {
                                continue;
                            }
                            if nxt == "fail" || (nxt == "pass" && cur.is_empty()) {
                                ag.insert(g.clone(), Json::str(nxt));
                            }
                        }
                    }
                }
                if m.get("infra_failed").and_then(Json::as_bool) == Some(true) {
                    e.insert("infra_failed".into(), Json::Bool(true));
                }
                // Scalar evidence members keep the first non-default
                // value across snapshots in declared order — the
                // primary harvest's run is the delivery; a later
                // re-execution contributes only its recorded
                // check/infra failures.
                for key in ["harness", "task", "agent_end_cause"] {
                    let cur = e.get(key).and_then(Json::as_str).unwrap_or("").to_string();
                    if cur.is_empty() {
                        if let Some(nv) = m.get(key).and_then(Json::as_str) {
                            if !nv.is_empty() {
                                e.insert(key.into(), Json::str(nv));
                            }
                        }
                    }
                }
                if e.get("receipt_count").and_then(Json::as_int).unwrap_or(0) == 0 {
                    if let Some(n) = m.get("receipt_count").and_then(Json::as_int) {
                        if n > 0 {
                            e.insert("receipt_count".into(), Json::Int(n));
                        }
                    }
                }
                if e.get("denied_hosts")
                    .and_then(Json::as_arr)
                    .is_none_or(|a| a.is_empty())
                {
                    if let Some(a) = m
                        .get("denied_hosts")
                        .and_then(Json::as_arr)
                        .filter(|a| !a.is_empty())
                    {
                        e.insert("denied_hosts".into(), Json::Arr(a.clone()));
                    }
                }
            }
        }
    }
    merged
}

/// The merged rows as the `pp0` member of `hh-fp-e-decl/1`: per arm,
/// `[{trial_ref, infra_failed, ag}]` — nothing else.
pub fn merged_pp0_decl(merged: &BTreeMap<String, Json>) -> BTreeMap<String, Vec<Json>> {
    let mut out: BTreeMap<String, Vec<Json>> = BTreeMap::new();
    for row in merged.values() {
        let m = row.as_obj().cloned().unwrap_or_default();
        let arm = m
            .get("harness")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        out.entry(arm).or_default().push(Json::obj([
            (
                "trial_ref",
                m.get("trial_ref").cloned().unwrap_or(Json::Null),
            ),
            (
                "infra_failed",
                m.get("infra_failed").cloned().unwrap_or(Json::Bool(false)),
            ),
            ("ag", m.get("ag").cloned().unwrap_or(Json::obj([]))),
        ]));
    }
    out
}

/// The corpus manifest's per-slot evidence rows (the corpus leg's
/// arm is always the original mini-swe-agent).
pub fn corpus_evidence(manifest: &Json) -> Vec<SlotEvidence> {
    let mut out = Vec::new();
    if let Some(slots) = manifest.get("slots").and_then(Json::as_arr) {
        for s in slots {
            let m = s.as_obj().cloned().unwrap_or_default();
            out.push(SlotEvidence {
                slot: m
                    .get("slot_id")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
                arm: "mini-swe-agent".to_string(),
                task: m
                    .get("task")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
                denied_hosts: m
                    .get("denied_hosts")
                    .and_then(Json::as_arr)
                    .map(|a| {
                        a.iter()
                            .filter_map(|h| h.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                agent_end_cause: m
                    .get("agent_end_cause")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
                receipt_count: m.get("receipt_count").and_then(Json::as_int).unwrap_or(0),
            });
        }
    }
    out
}

/// [`SlotEvidence`] rows out of the merged PP0 map.
pub fn pp0_evidence(merged: &BTreeMap<String, Json>) -> Vec<SlotEvidence> {
    let mut out = Vec::new();
    for (slot, row) in merged {
        let m = row.as_obj().cloned().unwrap_or_default();
        out.push(SlotEvidence {
            slot: slot.clone(),
            arm: m
                .get("harness")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            task: m
                .get("task")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            denied_hosts: m
                .get("denied_hosts")
                .and_then(Json::as_arr)
                .map(|a| {
                    a.iter()
                        .filter_map(|h| h.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            agent_end_cause: m
                .get("agent_end_cause")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
            receipt_count: m.get("receipt_count").and_then(Json::as_int).unwrap_or(0),
        });
    }
    out
}

#[derive(Default)]
struct SlotAcc {
    cost_micros: i64,
    tokens_input: i64,
    tokens_output: i64,
    tokens_cache_read: i64,
    model_ms: i64,
    calls: i64,
    calls_completed: i64,
    attributed_rows: i64,
    waits: Vec<i64>,
    wait_plus_lat: Vec<i64>,
    latency_max: i64,
    infra: Option<String>,
    budgets: BTreeSet<String>,
    runs: BTreeSet<String>,
    /// (min_received, max_receipted) per contributing run.
    spans: BTreeMap<String, (i64, i64)>,
}

impl SlotAcc {
    fn wall_ms(&self) -> i64 {
        self.spans.values().map(|(a, b)| b - a).sum()
    }
}

fn pct(sorted: &[i64], p: i64) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as i64 * p + 999) / 1000 - 1).max(0) as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// `extract_trials(runs, evidence, source_digests)` — fold the WAL
/// byte sources into the committed `hh-fp-trials/1` record. `runs`
/// is `(run_id, wal bytes)`; `evidence` maps slot → arm/task/members;
/// `source_digests` cites every input (name → `sha256:`).
pub fn extract_trials(
    runs: &[(String, Vec<u8>)],
    evidence: &BTreeMap<String, SlotEvidence>,
    source_digests: &BTreeMap<String, String>,
) -> Result<Json, String> {
    let mut slots: BTreeMap<String, SlotAcc> = BTreeMap::new();
    let mut intervals: Vec<(i64, i64)> = Vec::new();
    let mut run_spans: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let mut unlinked_calls: i64 = 0;

    for (run_id, bytes) in runs {
        let text =
            String::from_utf8(bytes.clone()).map_err(|_| format!("{run_id}: WAL is not utf-8"))?;
        let mut budget_to_slot: BTreeMap<String, String> = BTreeMap::new();
        let mut mc_to_budget: BTreeMap<String, String> = BTreeMap::new();
        // Pass 1 — budget→participant and call→budget links, cost rows.
        for line in text.lines() {
            let Ok(env) = hharness_wire::json::parse(line) else {
                continue;
            };
            let v = env.get("v").cloned().unwrap_or(env);
            let class = jstr(&v, "class").unwrap_or_default();
            let empty = Json::obj([]);
            let payload = jget(&v, "payload").cloned().unwrap_or(empty);
            match class.as_str() {
                "measurement.cost.attributed" => {
                    let budget = jstr(
                        &jget(&payload, "attribution").cloned().unwrap_or(Json::Null),
                        "budget_id",
                    )
                    .unwrap_or_default();
                    let part = jstr(
                        &jget(&payload, "attribution").cloned().unwrap_or(Json::Null),
                        "participant_ref",
                    )
                    .unwrap_or_default();
                    if !budget.is_empty() && !part.is_empty() {
                        budget_to_slot.insert(budget.clone(), part.clone());
                        let acc = slots.entry(part).or_default();
                        acc.cost_micros += jint(
                            &jget(&payload, "money").cloned().unwrap_or(Json::Null),
                            "micro_units",
                        )
                        .unwrap_or(0);
                        acc.attributed_rows += 1;
                        let roles = jget(&payload, "roles").cloned().unwrap_or(Json::Null);
                        acc.tokens_input += jint(&roles, "tokens.input.uncached").unwrap_or(0);
                        acc.tokens_output += jint(&roles, "tokens.output.visible").unwrap_or(0);
                        acc.budgets.insert(budget);
                        acc.runs.insert(run_id.clone());
                    }
                }
                "control.budget.reserved" => {
                    let holder = jstr(&payload, "holder").unwrap_or_default();
                    let budget = jstr(&payload, "budget_id").unwrap_or_default();
                    if let Some(mc) = holder.strip_prefix("mc:") {
                        if !budget.is_empty() {
                            mc_to_budget.insert(mc.to_string(), budget);
                        }
                    }
                }
                _ => {}
            }
        }
        // Pass 2 — call timing rows onto their slot.
        for line in text.lines() {
            let Ok(env) = hharness_wire::json::parse(line) else {
                continue;
            };
            let v = env.get("v").cloned().unwrap_or(env);
            let class = jstr(&v, "class").unwrap_or_default();
            if class != "model.call.completed" && class != "model.call.failed" {
                continue;
            }
            let empty = Json::obj([]);
            let payload = jget(&v, "payload").cloned().unwrap_or(empty);
            let mc = jstr(&payload, "model_call_id").unwrap_or_default();
            let Some(part) = mc_to_budget
                .get(&mc)
                .and_then(|b| budget_to_slot.get(b))
                .cloned()
            else {
                unlinked_calls += 1;
                continue;
            };
            let timing = jget(&payload, "timing").cloned().unwrap_or(Json::Null);
            let received = jint(&timing, "received_ms");
            let receipted = jint(&timing, "receipted_ms");
            if let (Some(a), Some(b)) = (received, receipted) {
                intervals.push((a, b));
                let e = run_spans.entry(run_id.clone()).or_insert((a, b));
                e.0 = e.0.min(a);
                e.1 = e.1.max(b);
            }
            let acc = slots.entry(part).or_default();
            acc.calls += 1;
            acc.runs.insert(run_id.clone());
            let qw = jint(&timing, "queue_wait_ms").unwrap_or(0);
            let lat = jint(&timing, "latency_ms").unwrap_or(0);
            acc.waits.push(qw);
            acc.wait_plus_lat.push(qw + lat);
            acc.latency_max = acc.latency_max.max(lat);
            acc.model_ms += lat;
            if let (Some(a), Some(b)) = (received, receipted) {
                let sp = acc.spans.entry(run_id.clone()).or_insert((a, b));
                sp.0 = sp.0.min(a);
                sp.1 = sp.1.max(b);
            }
            if class == "model.call.completed" {
                acc.calls_completed += 1;
                let usage = jget(&payload, "usage").cloned().unwrap_or(Json::Null);
                acc.tokens_cache_read += jint(&usage, "cache_read").unwrap_or(0);
            } else if acc.infra.is_none() {
                let disp = jstr(&payload, "disposition").unwrap_or_else(|| "unknown".into());
                acc.infra = Some(format!("infra:model_call_failed:{disp}"));
            }
        }
    }

    let mut trials = Vec::new();
    for (slot, acc) in &slots {
        let ev = evidence
            .get(slot)
            .ok_or_else(|| format!("slot {slot} has no declared arm evidence"))?;
        let mut w = acc.waits.clone();
        w.sort_unstable();
        let mut wl = acc.wait_plus_lat.clone();
        wl.sort_unstable();
        trials.push(Json::obj([
            ("slot", Json::str(slot.clone())),
            ("arm", Json::str(ev.arm.clone())),
            ("task", Json::str(ev.task.clone())),
            ("cost_micros", Json::Int(acc.cost_micros)),
            ("wall_ms", Json::Int(acc.wall_ms())),
            ("model_ms", Json::Int(acc.model_ms)),
            ("calls", Json::Int(acc.calls)),
            (
                "tokens",
                Json::obj([
                    ("input", Json::Int(acc.tokens_input)),
                    ("cache_read", Json::Int(acc.tokens_cache_read)),
                    ("output", Json::Int(acc.tokens_output)),
                ]),
            ),
            (
                "queue_wait",
                Json::obj([
                    ("p50_ms", Json::Int(pct(&w, 500))),
                    ("p90_ms", Json::Int(pct(&w, 900))),
                    ("p99_ms", Json::Int(pct(&w, 990))),
                    ("max_ms", Json::Int(w.last().copied().unwrap_or(0))),
                ]),
            ),
            ("wait_plus_latency_p99_ms", Json::Int(pct(&wl, 990))),
            ("latency_max_ms", Json::Int(acc.latency_max)),
            (
                "infra",
                acc.infra
                    .as_ref()
                    .map(|c| Json::str(c.clone()))
                    .unwrap_or(Json::Null),
            ),
            (
                "agent_timeout",
                Json::Bool(ev.agent_end_cause == "agent_timeout"),
            ),
            ("egress_denials", Json::Int(ev.denied_hosts.len() as i64)),
            ("proxy_rewrites", Json::Int(0)),
            (
                "receipted",
                Json::Bool(acc.attributed_rows >= acc.calls_completed),
            ),
            ("scan_flags", Json::Int(0)),
            (
                "budget_id",
                Json::str(acc.budgets.iter().next().cloned().unwrap_or_default()),
            ),
            (
                "run_id",
                Json::str(acc.runs.iter().cloned().collect::<Vec<_>>().join("+")),
            ),
            (
                "denied_hosts",
                Json::Arr(
                    ev.denied_hosts
                        .iter()
                        .map(|h| Json::str(h.clone()))
                        .collect(),
                ),
            ),
            ("agent_end_cause", Json::str(ev.agent_end_cause.clone())),
            ("receipt_count", Json::Int(ev.receipt_count)),
        ]));
    }
    trials.sort_by(|a, b| {
        jstr(a, "slot")
            .unwrap_or_default()
            .cmp(&jstr(b, "slot").unwrap_or_default())
    });

    // Attained concurrency — max overlapping in-flight call spans.
    let mut pts: Vec<(i64, i64)> = Vec::new();
    for (a, b) in &intervals {
        pts.push((*a, 1));
        pts.push((*b, -1));
    }
    pts.sort();
    let (mut cur, mut max_c) = (0i64, 0i64);
    for (_, d) in pts {
        cur += d;
        max_c = max_c.max(cur);
    }
    let wall_ms_total: i64 = run_spans.values().map(|(a, b)| b - a).sum();

    Ok(Json::obj([
        ("schema", Json::str("hh-fp-trials/1")),
        ("window", Json::str("ws1")),
        (
            "fold",
            Json::obj([
                ("extractor", Json::str(FOLD_EXTRACTOR)),
                ("envelope", Json::str("{\"k\":\"e\",\"v\":{event}}")),
                (
                    "sources",
                    Json::Obj(
                        source_digests
                            .iter()
                            .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                            .collect(),
                    ),
                ),
            ]),
        ),
        (
            "nuisance_members",
            Json::Arr(
                [
                    "cost",
                    "cached_share_ppm",
                    "tokens",
                    "calls",
                    "wall_ms",
                    "model_ms",
                    "queue_wait_ms",
                    "wait_plus_latency_p99_ms",
                    "infra",
                    "timeout",
                    "egress_denials",
                    "proxy_rewrites",
                    "receipt_completeness",
                    "scan_flags",
                ]
                .iter()
                .map(|s| Json::str(*s))
                .collect(),
            ),
        ),
        (
            "client_timeouts_ms",
            Json::obj([
                ("mini-swe-agent", Json::Int(300_000)),
                ("hh_native", Json::Int(300_000)),
                ("v5", Json::Int(300_000)),
                ("pi", Json::Int(300_000)),
                ("claude-code", Json::Int(300_000)),
            ]),
        ),
        (
            "global",
            Json::obj([
                ("attained_concurrency", Json::Int(max_c)),
                ("wall_hours", Json::str(format!("{wall_ms_total}/3600000"))),
                (
                    "instance_hours",
                    Json::str(format!("{wall_ms_total}/3600000")),
                ),
                ("unlinked_calls", Json::Int(unlinked_calls)),
                ("runs", Json::Int(runs.len() as i64)),
            ]),
        ),
        ("withheld", Json::Arr(vec![])),
        ("trials", Json::Arr(trials)),
    ]))
}
