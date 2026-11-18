//! `fp_measure` — the R6.46 FP-E measurement packet (manifest 310).
//!
//! REQ-XD-043 / REQ-XC-015 require a deterministic measurement packet
//! folded from W2's cost and timing records only. This module decodes
//! the committed `hh-fp-trials/1` fold record — produced by the
//! WAL→record extractor over the durable proxy-store events — and
//! emits `hh-fp-measurement/1`:
//!
//! - per harness: $/trial mean, P50, P90, max; cached share; tokens;
//!   calls; agent wall time; model-occupied time; proxy queue wait;
//!   the `queue_wait + latency` P99 beside that harness's admitted
//!   client timeout (REQ-XD-062); infra rate by cause; timeout rate;
//!   egress denials; proxy rewrites; receipt completeness; scan flags;
//! - globally: attained concurrency, wall hours, instance hours.
//!
//! The outcome firewall ([`check_cost_only`]) runs before any member
//! reads: an injected outcome field refuses [`MeasureErr::OutcomeLeak`]
//! while `withheld{…}` labels pass through verbatim — they are labels,
//! never outcome reads (REQ-XH-121). REQ-XD-044's nuisance set is the
//! declared `nuisance_members` list; naming a confirmatory member (or
//! relabelling a withheld outcome onto one) refuses
//! [`MeasureErr::ConfirmatoryRelabel`].

use std::collections::BTreeMap;

use hharness_wire::json::Json;

use crate::continuation::{check_cost_only, schema, ContinuationError, FpArm, TrialMeasure};
use crate::json_ext::JsonExt;

/// The input fold record's schema.
pub const TRIALS_SCHEMA: &str = "hh-fp-trials/1";
/// The emitted packet's schema.
pub const MEASURE_SCHEMA: &str = "hh-fp-measurement/1";

/// The outcome keys the firewall refuses on a trial row or record
/// root — any of these is an outcome read, never a cost field
/// (REQ-XH-121). `withheld{…}` strings are values inside the declared
/// `withheld` member, not keys, and pass through.
const OUTCOME_KEYS: [&str; 9] = [
    "outcome",
    "verdict",
    "passed",
    "solved",
    "success",
    "task_success",
    "score",
    "parity",
    "result",
];

/// The packet members REQ-XD-044 may feed from W2 pilot data — the
/// cost/timing nuisance set. A `nuisance_members` entry outside this
/// set is a confirmatory relabel and refuses.
const NUISANCE_MEMBERS: [&str; 14] = [
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
];

/// The closed refusal set.
#[derive(Debug, Clone, PartialEq)]
pub enum MeasureErr {
    /// Malformed input — member absent or wrong shape.
    Schema(String),
    /// The record is not `hh-fp-trials/1`.
    SchemaId(String),
    /// An outcome key was injected on a row or the record root.
    OutcomeLeak { slot: String, key: String },
    /// A `nuisance_members` entry names a confirmatory field, or a
    /// `relabels` entry targets one (REQ-XD-044).
    ConfirmatoryRelabel { field: String },
    /// A trial names an unregistered arm.
    UnknownArm { slot: String, arm: String },
}

impl std::fmt::Display for MeasureErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MeasureErr::Schema(d) => write!(f, "hh-fp-trials/1 malformed: {d}"),
            MeasureErr::SchemaId(s) => write!(f, "schema must be {TRIALS_SCHEMA}, got {s}"),
            MeasureErr::OutcomeLeak { slot, key } => {
                write!(f, "outcome field {key} on {slot} — refused (REQ-XH-121)")
            }
            MeasureErr::ConfirmatoryRelabel { field } => {
                write!(
                    f,
                    "nuisance member {field} is confirmatory — refused (REQ-XD-044)"
                )
            }
            MeasureErr::UnknownArm { slot, arm } => {
                write!(f, "trial {slot} names unregistered arm {arm}")
            }
        }
    }
}

impl std::error::Error for MeasureErr {}

impl From<MeasureErr> for ContinuationError {
    fn from(e: MeasureErr) -> Self {
        schema(e.to_string())
    }
}

fn miss(d: impl Into<String>) -> MeasureErr {
    MeasureErr::Schema(d.into())
}

/// Refuse an injected outcome key on `j`, naming `slot`.
fn firewall(j: &Json, slot: &str) -> Result<(), MeasureErr> {
    if let Some(m) = j.as_obj() {
        for k in m.keys() {
            if OUTCOME_KEYS.contains(&k.as_str()) {
                return Err(MeasureErr::OutcomeLeak {
                    slot: slot.to_string(),
                    key: k.clone(),
                });
            }
        }
    }
    Ok(())
}

/// One measured trial's foldable cost/timing fields — the decoded
/// `hh-fp-trials/1` row. No outcome member exists on this type.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasureTrial {
    /// The slot record ref.
    pub slot: String,
    /// The arm.
    pub arm: FpArm,
    /// The task the cell ran.
    pub task: String,
    /// Metered cost (µ$).
    pub cost_micros: i64,
    /// Agent wall time (ms).
    pub wall_ms: i64,
    /// Model-occupied time (ms).
    pub model_ms: i64,
    /// Model calls in the trial.
    pub calls: i64,
    /// Input tokens (uncached).
    pub tokens_input: i64,
    /// Cache-read input tokens.
    pub tokens_cache_read: i64,
    /// Output tokens.
    pub tokens_output: i64,
    /// Per-call queue wait P50 (ms).
    pub wait_p50_ms: i64,
    /// Per-call queue wait P90 (ms).
    pub wait_p90_ms: i64,
    /// Per-call queue wait P99 (ms).
    pub wait_p99_ms: i64,
    /// Max per-call queue wait (ms).
    pub wait_max_ms: i64,
    /// P99 of `queue_wait + latency` across the trial's calls (ms) —
    /// the REQ-XD-062 row beside the admitted client timeout.
    pub wait_plus_latency_p99_ms: i64,
    /// Infra cause (`infra:<cause>`) or `None`.
    pub infra: Option<String>,
    /// The agent's declared timeout fired.
    pub agent_timeout: bool,
    /// Egress denials recorded for the trial.
    pub egress_denials: i64,
    /// Proxy rewrites recorded for the trial.
    pub proxy_rewrites: i64,
    /// Every call carried a receipted cost row.
    pub receipted: bool,
    /// Scan flags attributed to the trial.
    pub scan_flags: i64,
}

const TRIAL_KEYS: [&str; 20] = [
    "slot",
    "arm",
    "task",
    "cost_micros",
    "wall_ms",
    "model_ms",
    "calls",
    "tokens",
    "queue_wait",
    "wait_plus_latency_p99_ms",
    "latency_max_ms",
    "infra",
    "agent_timeout",
    "egress_denials",
    "proxy_rewrites",
    "receipted",
    "scan_flags",
    "budget_id",
    "run_id",
    "withheld",
];

impl MeasureTrial {
    /// Strict decode of one `trials[]` row.
    pub fn from_json(j: &Json) -> Result<MeasureTrial, MeasureErr> {
        let m = j
            .as_obj()
            .ok_or_else(|| miss("trials[] must be an object"))?;
        firewall(j, m.get("slot").and_then(Json::as_str).unwrap_or("<row>"))?;
        for k in m.keys() {
            if !TRIAL_KEYS.contains(&k.as_str()) {
                return Err(miss(format!("trials[].{k} is not a cost field")));
            }
        }
        let slot = m
            .get("slot")
            .and_then(Json::as_str)
            .ok_or_else(|| miss("trials[].slot required"))?
            .to_string();
        let arm_str = m
            .get("arm")
            .and_then(Json::as_str)
            .ok_or_else(|| miss("trials[].arm required"))?;
        let arm = FpArm::parse(arm_str).ok_or_else(|| MeasureErr::UnknownArm {
            slot: slot.clone(),
            arm: arm_str.to_string(),
        })?;
        let num = |key: &str| -> Result<i64, MeasureErr> {
            m.get(key)
                .and_then(Json::as_int)
                .ok_or_else(|| miss(format!("trials[].{key} required")))
        };
        let tok = m
            .get("tokens")
            .and_then(Json::as_obj)
            .ok_or_else(|| miss("trials[].tokens required"))?;
        let tnum = |key: &str| -> Result<i64, MeasureErr> {
            tok.get(key)
                .and_then(Json::as_int)
                .ok_or_else(|| miss(format!("trials[].tokens.{key} required")))
        };
        let qw = m
            .get("queue_wait")
            .and_then(Json::as_obj)
            .ok_or_else(|| miss("trials[].queue_wait required (REQ-XD-062)"))?;
        let qnum = |key: &str| -> Result<i64, MeasureErr> {
            qw.get(key)
                .and_then(Json::as_int)
                .ok_or_else(|| miss(format!("trials[].queue_wait.{key} required")))
        };
        let infra = match m.get("infra") {
            None | Some(Json::Null) => None,
            Some(Json::Str(s)) => Some(s.clone()),
            _ => return Err(miss("trials[].infra must be null or 'infra:<cause>'")),
        };
        Ok(MeasureTrial {
            slot,
            arm,
            task: m
                .get("task")
                .and_then(Json::as_str)
                .ok_or_else(|| miss("trials[].task required"))?
                .to_string(),
            cost_micros: num("cost_micros")?,
            wall_ms: num("wall_ms")?,
            model_ms: num("model_ms")?,
            calls: num("calls")?,
            tokens_input: tnum("input")?,
            tokens_cache_read: tnum("cache_read")?,
            tokens_output: tnum("output")?,
            wait_p50_ms: qnum("p50_ms")?,
            wait_p90_ms: qnum("p90_ms")?,
            wait_p99_ms: qnum("p99_ms")?,
            wait_max_ms: qnum("max_ms")?,
            wait_plus_latency_p99_ms: num("wait_plus_latency_p99_ms")?,
            infra,
            agent_timeout: m
                .get("agent_timeout")
                .and_then(Json::as_bool)
                .ok_or_else(|| miss("trials[].agent_timeout required"))?,
            egress_denials: num("egress_denials")?,
            proxy_rewrites: num("proxy_rewrites")?,
            receipted: m
                .get("receipted")
                .and_then(Json::as_bool)
                .ok_or_else(|| miss("trials[].receipted required"))?,
            scan_flags: num("scan_flags")?,
        })
    }

    /// The `hh-fp-continuation-input/1` measured row for this trial.
    pub fn to_measure(&self) -> TrialMeasure {
        TrialMeasure {
            slot: self.slot.clone(),
            cost_micros: self.cost_micros,
            wall_ms: self.wall_ms,
            queue_wait_ms: Some(self.wait_max_ms),
            latency_ms: Some(self.wait_plus_latency_p99_ms - self.wait_max_ms),
            infra_failed: self.infra.is_some(),
        }
    }
}

fn pct(sorted: &[i64], p: i64) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as i64 * p + 999) / 1000 - 1).max(0) as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn dist(vals: &mut [i64]) -> Json {
    vals.sort_unstable();
    let n = vals.len() as i64;
    Json::obj([
        (
            "mean",
            Json::Int(if n == 0 {
                0
            } else {
                vals.iter().sum::<i64>() / n
            }),
        ),
        ("p50", Json::Int(pct(vals, 500))),
        ("p90", Json::Int(pct(vals, 900))),
        ("max", Json::Int(vals.last().copied().unwrap_or(0))),
    ])
}

fn rate_ppm(n: i64, total: i64) -> i64 {
    if total <= 0 {
        0
    } else {
        (n * 1_000_000 + total - 1) / total
    }
}

/// `fold_packet(trials_record)` — the `hh-fp-measurement/1` packet.
/// Reads cost fields only; refuses outcome leaks and confirmatory
/// relabels typed before folding.
pub fn fold_packet(rec: &Json) -> Result<Json, MeasureErr> {
    // Row-level scan first — the refusal names the offending slot and
    // key; the deep record scan below catches anything nested.
    firewall(rec, "<record>")?;
    let m = rec
        .as_obj()
        .ok_or_else(|| miss("record must be an object"))?;
    match m.get("schema").and_then(Json::as_str) {
        Some(s) if s == TRIALS_SCHEMA => {}
        Some(s) => return Err(MeasureErr::SchemaId(s.to_string())),
        None => return Err(miss("schema required")),
    }
    // REQ-XD-044 — the nuisance set is declared; an entry outside the
    // cost/timing member list is a confirmatory relabel and refuses.
    let mut nuisance = Vec::new();
    if let Some(ns) = m.get("nuisance_members").and_then(Json::as_arr) {
        for n in ns {
            let s = n
                .as_str()
                .ok_or_else(|| miss("nuisance_members[] must be str"))?;
            if !NUISANCE_MEMBERS.contains(&s) {
                return Err(MeasureErr::ConfirmatoryRelabel {
                    field: s.to_string(),
                });
            }
            nuisance.push(s.to_string());
        }
    }
    if let Some(rs) = m.get("relabels").and_then(Json::as_arr) {
        for r in rs {
            let target = r
                .get("to")
                .and_then(Json::as_str)
                .ok_or_else(|| miss("relabels[].to required"))?;
            if !NUISANCE_MEMBERS.contains(&target) {
                return Err(MeasureErr::ConfirmatoryRelabel {
                    field: target.to_string(),
                });
            }
        }
    }
    let timeouts = m
        .get("client_timeouts_ms")
        .and_then(Json::as_obj)
        .cloned()
        .unwrap_or_default();
    let global = m
        .get("global")
        .cloned()
        .ok_or_else(|| miss("global required — attained concurrency, wall and instance hours"))?;
    for k in ["attained_concurrency", "wall_hours", "instance_hours"] {
        if global.get(k).is_none() {
            return Err(miss(format!("global.{k} required")));
        }
    }
    let withheld = m
        .get("withheld")
        .and_then(Json::as_arr)
        .cloned()
        .unwrap_or_default();
    let rows = m
        .get("trials")
        .and_then(Json::as_arr)
        .ok_or_else(|| miss("trials[] required"))?;
    let mut trials = Vec::new();
    for r in rows {
        trials.push(MeasureTrial::from_json(r)?);
    }
    // Deep scan — an outcome member nested anywhere else (fold,
    // global, tokens…) refuses with its path.
    check_cost_only(rec).map_err(|e| MeasureErr::OutcomeLeak {
        slot: "<record>".to_string(),
        key: e.to_string(),
    })?;

    let mut by_arm: BTreeMap<FpArm, Vec<&MeasureTrial>> = BTreeMap::new();
    for t in &trials {
        by_arm.entry(t.arm).or_default().push(t);
    }
    let mut harnesses = Vec::new();
    for (arm, ts) in &by_arm {
        let mut cost: Vec<i64> = ts.iter().map(|t| t.cost_micros).collect();
        let mut wall: Vec<i64> = ts.iter().map(|t| t.wall_ms).collect();
        let mut model: Vec<i64> = ts.iter().map(|t| t.model_ms).collect();
        let mut wait: Vec<i64> = ts
            .iter()
            .flat_map(|t| vec![t.wait_p50_ms, t.wait_p90_ms, t.wait_p99_ms, t.wait_max_ms])
            .collect();
        let wlp99 = ts
            .iter()
            .map(|t| t.wait_plus_latency_p99_ms)
            .max()
            .unwrap_or(0);
        let calls: i64 = ts.iter().map(|t| t.calls).sum();
        let ti: i64 = ts.iter().map(|t| t.tokens_input).sum();
        let tcr: i64 = ts.iter().map(|t| t.tokens_cache_read).sum();
        let to: i64 = ts.iter().map(|t| t.tokens_output).sum();
        let input_total = ti + tcr;
        let cached_share = if input_total <= 0 {
            0
        } else {
            tcr * 1_000_000 / input_total
        };
        let n = ts.len() as i64;
        let timeouts_n = ts.iter().filter(|t| t.agent_timeout).count() as i64;
        let mut by_cause: Vec<(String, i64)> = Vec::new();
        for t in ts {
            if let Some(c) = &t.infra {
                match by_cause.iter_mut().find(|(k, _)| k == c) {
                    Some((_, v)) => *v += 1,
                    None => by_cause.push((c.clone(), 1)),
                }
            }
        }
        let infra_n: i64 = by_cause.iter().map(|(_, v)| *v).sum();
        let admitted_timeout = timeouts.get(arm.as_str()).and_then(Json::as_int);
        harnesses.push((
            arm.as_str().to_string(),
            Json::obj([
                ("trials", Json::Int(n)),
                ("cost_micros", dist(&mut cost)),
                ("cached_share_ppm", Json::Int(cached_share)),
                (
                    "tokens",
                    Json::obj([
                        ("input", Json::Int(ti)),
                        ("cache_read", Json::Int(tcr)),
                        ("output", Json::Int(to)),
                    ]),
                ),
                ("calls", Json::Int(calls)),
                ("wall_ms", dist(&mut wall)),
                ("model_ms", dist(&mut model)),
                ("queue_wait_ms", dist(&mut wait)),
                ("wait_plus_latency_p99_ms", Json::Int(wlp99)),
                (
                    "admitted_client_timeout_ms",
                    admitted_timeout.map_or(Json::Null, Json::Int),
                ),
                (
                    "infra",
                    Json::obj([
                        ("rate_ppm", Json::Int(rate_ppm(infra_n, n))),
                        (
                            "by_cause",
                            Json::Obj(
                                by_cause
                                    .into_iter()
                                    .map(|(k, v)| (k, Json::Int(v)))
                                    .collect(),
                            ),
                        ),
                    ]),
                ),
                (
                    "timeout",
                    Json::obj([
                        ("n", Json::Int(timeouts_n)),
                        ("rate_ppm", Json::Int(rate_ppm(timeouts_n, n))),
                    ]),
                ),
                (
                    "egress_denials",
                    Json::Int(ts.iter().map(|t| t.egress_denials).sum()),
                ),
                (
                    "proxy_rewrites",
                    Json::Int(ts.iter().map(|t| t.proxy_rewrites).sum()),
                ),
                (
                    "receipt_completeness",
                    Json::obj([
                        (
                            "receipted",
                            Json::Int(ts.iter().filter(|t| t.receipted).count() as i64),
                        ),
                        ("total", Json::Int(n)),
                    ]),
                ),
                (
                    "scan_flags",
                    Json::Int(ts.iter().map(|t| t.scan_flags).sum()),
                ),
            ]),
        ));
    }

    Ok(Json::obj([
        ("schema", Json::str(MEASURE_SCHEMA)),
        (
            "window",
            m.get("window")
                .and_then(Json::as_str)
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
        ("fold", m.get("fold").cloned().unwrap_or(Json::Null)),
        (
            "nuisance_members",
            Json::Arr(nuisance.iter().map(Json::str).collect()),
        ),
        ("withheld", Json::Arr(withheld)),
        ("global", {
            let mut g = global.as_obj().cloned().unwrap_or_default();
            g.insert("trials".to_string(), Json::Int(trials.len() as i64));
            Json::Obj(g)
        }),
        ("harnesses", Json::Obj(harnesses.into_iter().collect())),
    ]))
}

/// `trials_measures(rec)` — the continuation input's `measured` map:
/// cost fields only, keyed by arm (REQ-XD-043's readable set).
pub fn trials_measures(rec: &Json) -> Result<BTreeMap<FpArm, Vec<TrialMeasure>>, MeasureErr> {
    let m = rec
        .as_obj()
        .ok_or_else(|| miss("record must be an object"))?;
    let rows = m
        .get("trials")
        .and_then(Json::as_arr)
        .ok_or_else(|| miss("trials[] required"))?;
    let mut out: BTreeMap<FpArm, Vec<TrialMeasure>> = BTreeMap::new();
    for r in rows {
        let t = MeasureTrial::from_json(r)?;
        out.entry(t.arm).or_default().push(t.to_measure());
    }
    Ok(out)
}
