//! `fp_e` — the R6.46 FP-E quote (manifest 310, GATE-G30's input).
//!
//! `fp_e_quote(decl)` takes the committed `hh-fp-e-decl/1` records-in
//! and emits the `hh-fp-e/1` gate packet — the one FP-E quote under
//! GATE-G29's recorded disposition (`full` after the committed G-L1
//! pass; `fallback` under a declared trigger):
//!
//! - the W2 measurement packet ([`crate::fp_measure`], REQ-XD-043);
//! - the per-arm inclusion decision — PP0 holds fold through
//!   [`crate::holds::pp0_hold`]/[`crate::holds::scope_decision_record`],
//!   priority/drop through `run_checkpoint`'s declared order;
//! - the `hh-fp-checkpoint/1` readout — caps, fits, the admitted set —
//!   computed by the shared continuation machinery, never re-derived;
//! - `D_max` under the amended accounting (REQ-XH-126(b): billed where
//!   reconciled, `cost_lib` for completed-unreconciled, ceilings only
//!   for authorized-but-unrun) and the re-issued W4 quote inputs with
//!   `per_trial_bound_seconds := D_max`;
//! - the `comparative-quote/1` model lines — `max(mean,P90)·T·k·1.2`
//!   planning ceilings built from the measurement packet's declared
//!   nuisance fields, with `hh-compute-quote/1` lines cited verbatim
//!   (never repriced, ADR-0432);
//! - the root recomputation — `A`, `A_eff`, the carried pool and the
//!   credit-purchase need HUMAN-H15 consumes (≤ $10 or NOT_NEEDED);
//! - the exclusive acknowledgement: the quote authorizes nothing.
//!
//! The firewall runs before every member read — outcome fields refuse
//! typed; `withheld{…}` labels pass through verbatim.

use std::collections::BTreeMap;

use hharness_eval::comparative_quote::{
    build_quote, ArmMeasure, ComparativeQuoteDecl, ComputeLine, Envelope, MenuRowDecl, SimCells,
};
use hharness_wire::json::Json;

use crate::continuation::{
    cap_record, cap_table, check_cost_only, d_max_ms, run_checkpoint, ContinuationError,
    ContinuationInput, FpArm, OhPair, Scope, W4Ceiling, WallInput, WcTerm, WindowSum,
};
use crate::disposition::WithheldReason;
use crate::fp_measure::{fold_packet, trials_measures, MeasureErr, MEASURE_SCHEMA};
use crate::holds::{hosted_only_arms, pp0_hold, scope_decision_record, Pp0TrialEvidence};
use crate::json_ext::JsonExt;

/// The declaration schema.
pub const FP_E_DECL_SCHEMA: &str = "hh-fp-e-decl/1";
/// The emitted gate packet's schema.
pub const FP_E_SCHEMA: &str = "hh-fp-e/1";
/// The acknowledgement every emitted packet carries.
pub const NO_AUTHORIZATION: &str =
    "the FP-E quote authorizes nothing — execution needs HUMAN-H15's release";

/// The closed refusal set.
#[derive(Debug)]
pub enum FpEErr {
    /// Malformed declaration.
    Schema(String),
    /// The measurement fold refused.
    Measure(MeasureErr),
    /// The continuation machinery refused.
    Continuation(ContinuationError),
    /// The comparative-quote builder refused.
    Comparative(hharness_eval::comparative_quote::ComparativeQuoteError),
    /// `full` declared without a committed G-L1 pass id (REQ-XD-102's
    /// precondition carried into the quote record).
    MissingGl1,
}

impl std::fmt::Display for FpEErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FpEErr::Schema(d) => write!(f, "hh-fp-e-decl/1: {d}"),
            FpEErr::Measure(e) => write!(f, "{e}"),
            FpEErr::Continuation(e) => write!(f, "{e}"),
            FpEErr::Comparative(e) => write!(f, "{e}"),
            FpEErr::MissingGl1 => write!(
                f,
                "disposition full requires the committed G-L1 pass id — REQ-XD-102"
            ),
        }
    }
}

impl std::error::Error for FpEErr {}

fn miss(d: impl Into<String>) -> FpEErr {
    FpEErr::Schema(d.into())
}

fn req_int(m: &std::collections::BTreeMap<String, Json>, key: &str) -> Result<i64, FpEErr> {
    m.get(key)
        .and_then(Json::as_int)
        .ok_or_else(|| miss(format!("{key} required")))
}

/// Decode one `d_max.completed[]` row into a [`WindowSum`]. A row
/// declared `done` must carry `billed_micros` or `cost_lib_micros` —
/// the A2 amendment forbids a completed window falling back to its
/// ceiling for want of billing.
fn window_sum(j: &Json) -> Result<WindowSum, FpEErr> {
    let m = j
        .as_obj()
        .ok_or_else(|| miss("d_max.completed[] must be an object"))?;
    let done = m
        .get("done")
        .and_then(Json::as_bool)
        .ok_or_else(|| miss("d_max.completed[].done required"))?;
    let billed = m.get("billed_micros").and_then(Json::as_int);
    let cost_lib = m.get("cost_lib_micros").and_then(Json::as_int);
    if done && billed.is_none() && cost_lib.is_none() {
        return Err(miss(format!(
            "completed window {} carries neither billed nor cost_lib — \
             the ceiling is not a completed-window figure (A2)",
            m.get("window")
                .and_then(Json::as_str)
                .unwrap_or("<unnamed>")
        )));
    }
    Ok(WindowSum {
        window: m
            .get("window")
            .and_then(Json::as_str)
            .ok_or_else(|| miss("d_max.completed[].window required"))?
            .to_string(),
        ceiling_micros: req_int(m, "ceiling_micros")?,
        billed_micros: billed,
        cost_lib_micros: cost_lib,
        done,
    })
}

/// `fp_e_quote(decl)` — the `hh-fp-e/1` gate packet.
pub fn fp_e_quote(decl: &Json) -> Result<Json, FpEErr> {
    check_cost_only(decl).map_err(FpEErr::Continuation)?;
    let m = decl
        .as_obj()
        .ok_or_else(|| miss("declaration must be an object"))?;
    if m.get("schema").and_then(Json::as_str) != Some(FP_E_DECL_SCHEMA) {
        return Err(miss(format!("schema must be {FP_E_DECL_SCHEMA}")));
    }

    // ── disposition + the G-L1 precondition ─────────────────────────
    let disp = m
        .get("disposition")
        .and_then(Json::as_obj)
        .ok_or_else(|| miss("disposition required"))?;
    let full = match disp.get("disposition").and_then(Json::as_str) {
        Some("full") => true,
        Some("fallback") => false,
        other => return Err(miss(format!("disposition {other:?} unknown"))),
    };
    let gl1 = disp.get("gl1").cloned().unwrap_or(Json::Null);
    let gl1_pass_id = gl1
        .get("pass_id")
        .and_then(Json::as_str)
        .map(str::to_string);
    if full && gl1_pass_id.is_none() {
        return Err(FpEErr::MissingGl1);
    }
    let l1_passed = gl1.get("result").and_then(Json::as_str) == Some("pass");

    // ── the measurement packet (REQ-XD-043) ──────────────────────────
    let trials_rec = m
        .get("trials")
        .ok_or_else(|| miss("trials (hh-fp-trials/1) required"))?;
    let packet = fold_packet(trials_rec).map_err(FpEErr::Measure)?;
    let measured = trials_measures(trials_rec).map_err(FpEErr::Measure)?;

    // ── PP0 holds over the hosted arms ───────────────────────────────
    let mut held: Vec<FpArm> = Vec::new();
    let mut scope_decisions: Vec<Json> = Vec::new();
    if let Some(pp0) = m.get("pp0").and_then(Json::as_obj) {
        for (arm_s, rows) in pp0 {
            let arm =
                FpArm::parse(arm_s).ok_or_else(|| miss(format!("pp0.{arm_s} is not an arm")))?;
            let ev = rows
                .as_arr()
                .ok_or_else(|| miss(format!("pp0.{arm_s} must be an array")))?
                .iter()
                .map(Pp0TrialEvidence::from_json)
                .collect::<Result<Vec<_>, _>>()
                .map_err(FpEErr::Continuation)?;
            if let Some(hold) = pp0_hold(&ev) {
                // (b) the held original takes the scope decision;
                // (a) the other hosted arms take the per-arm
                // disposition record.
                let rec = if arm == FpArm::OrigMini {
                    scope_decision_record(arm, &hold).map_err(FpEErr::Continuation)?
                } else {
                    crate::holds::pp0_disposition(arm, &hold).map_err(FpEErr::Continuation)?
                };
                scope_decisions.push(rec);
                held.push(arm);
            }
        }
    }

    // ── the arm set under the recorded disposition ───────────────────
    // fallback → the three hosted arms only (hosted_only_arms); full →
    // the five-arm set minus PP0-held arms (the held arm's label rides
    // in scope_decisions/withheld).
    let base_arms: Vec<FpArm> = if full {
        FpArm::ALL.to_vec()
    } else {
        hosted_only_arms()
    };
    let arm_set: Vec<FpArm> = base_arms
        .iter()
        .copied()
        .filter(|a| !held.contains(a))
        .collect();

    // ── the continuation input (measured rows from the packet) ───────
    let cont = m
        .get("continuation")
        .and_then(Json::as_obj)
        .ok_or_else(|| miss("continuation required"))?;
    let mut cont_map = cont.clone();
    {
        let o = &mut cont_map;
        o.insert(
            "schema".into(),
            Json::str(crate::continuation::INPUT_SCHEMA),
        );
        o.insert("scope".into(), Json::str(Scope::FpE.as_str()));
        o.insert(
            "measured".into(),
            Json::Obj(
                measured
                    .iter()
                    .map(|(a, rows)| {
                        (
                            a.as_str().to_string(),
                            Json::Arr(
                                rows.iter()
                                    .map(|t| {
                                        Json::obj([
                                            ("slot", Json::str(&t.slot)),
                                            ("cost_micros", Json::Int(t.cost_micros)),
                                            ("wall_ms", Json::Int(t.wall_ms)),
                                            (
                                                "queue_wait_ms",
                                                t.queue_wait_ms.map_or(Json::Null, Json::Int),
                                            ),
                                            (
                                                "latency_ms",
                                                t.latency_ms.map_or(Json::Null, Json::Int),
                                            ),
                                            ("infra_failed", Json::Bool(t.infra_failed)),
                                        ])
                                    })
                                    .collect(),
                            ),
                        )
                    })
                    .collect(),
            ),
        );
        // Held/fallback-dropped arms lose their remaining slots — the
        // slots are released, not run (the label rides in withheld).
        if let Some(Json::Obj(rem)) = o.get_mut("remaining") {
            for arm in FpArm::ALL {
                if !arm_set.contains(&arm) {
                    rem.insert(arm.as_str().to_string(), Json::Arr(vec![]));
                }
            }
        } else {
            return Err(miss("continuation.remaining required"));
        }
    }
    let input_json = Json::Obj(cont_map);
    let input = ContinuationInput::from_json(&input_json).map_err(FpEErr::Continuation)?;
    let wall = m.get("wall_input").map(|w| {
        let wi = |k: &str| w.get(k).and_then(Json::as_int).unwrap_or(0);
        WallInput {
            d_limit_ms: wi("d_limit_ms"),
            cutoff_ms: wi("cutoff_ms"),
            overhead_ms: wi("overhead_ms"),
            load_ms: wi("load_ms"),
            pairs: w
                .get("pairs")
                .and_then(Json::as_arr)
                .map(|ps| {
                    ps.iter()
                        .filter_map(|p| {
                            Some(OhPair {
                                o_slot: p.get("o_slot")?.as_str()?.to_string(),
                                h_slot: p.get("h_slot")?.as_str()?.to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    });
    let outcome = run_checkpoint(&input, wall.as_ref()).map_err(FpEErr::Continuation)?;

    // ── per-arm caps over the continuing slots (REQ-XH-117) ─────────
    let planned: BTreeMap<FpArm, i64> = outcome
        .continuing
        .iter()
        .map(|(a, s)| (*a, s.len() as i64))
        .collect();
    let caps = cap_table(&planned, &input);
    let caps_rec = cap_record(&planned, &caps, &outcome.p_hats);

    // ── D_max + the W4 re-issue (amended accounting) ────────────────
    let dm = m
        .get("d_max")
        .and_then(Json::as_obj)
        .ok_or_else(|| miss("d_max required"))?;
    let completed = dm
        .get("completed")
        .and_then(Json::as_arr)
        .ok_or_else(|| miss("d_max.completed[] required"))?
        .iter()
        .map(window_sum)
        .collect::<Result<Vec<_>, _>>()?;
    let w4c = dm
        .get("w4")
        .and_then(Json::as_obj)
        .ok_or_else(|| miss("d_max.w4 required"))?;
    let w4 = W4Ceiling {
        hourly_micros: req_int(w4c, "hourly_micros")?,
        fixed_micros: req_int(w4c, "fixed_micros")?,
    };
    let wcj = dm
        .get("wc")
        .and_then(Json::as_obj)
        .ok_or_else(|| miss("d_max.wc required"))?;
    let wc = WcTerm {
        authorized: wcj
            .get("authorized")
            .and_then(Json::as_bool)
            .ok_or_else(|| miss("d_max.wc.authorized required"))?,
        ceiling_micros: req_int(wcj, "ceiling_micros")?,
        reservation_micros: req_int(wcj, "reservation_micros")?,
    };
    let d_max = d_max_ms(&completed, &w4, &wc);

    // ── the W4 quote re-issue inputs: the declared quantities with
    // `per_trial_bound_seconds := ceil(D_max/1000)` — receipt_lib.py's
    // quote_doc re-issues the `hh-compute-quote/1` record from exactly
    // these inputs at script level; nothing else is repriced.
    let w4_in = m
        .get("w4_quote")
        .and_then(Json::as_obj)
        .ok_or_else(|| miss("w4_quote required"))?;
    let mut quantities = w4_in
        .get("quantities")
        .and_then(Json::as_obj)
        .cloned()
        .ok_or_else(|| miss("w4_quote.quantities required"))?;
    quantities.insert(
        "per_trial_bound_seconds".to_string(),
        Json::Int((d_max + 999) / 1000),
    );
    let w4_inputs = Json::obj([
        ("schema", Json::str("hh-compute-quote-inputs/1")),
        ("window", Json::str("w4")),
        (
            "based_on",
            w4_in.get("based_on").cloned().unwrap_or(Json::Null),
        ),
        ("d_max_ms", Json::Int(d_max)),
        ("quantities", Json::Obj(quantities)),
        (
            "rate_table",
            w4_in.get("rate_table").cloned().unwrap_or(Json::Null),
        ),
    ]);

    // ── the comparative-quote model lines ───────────────────────────
    let cq = m
        .get("comparative")
        .and_then(Json::as_obj)
        .ok_or_else(|| miss("comparative required"))?;
    let harnesses = packet
        .get("harnesses")
        .and_then(Json::as_obj)
        .cloned()
        .unwrap_or_default();
    let prior_refs = cq
        .get("prior_refs")
        .and_then(Json::as_obj)
        .cloned()
        .unwrap_or_default();
    let measure_ref = cq
        .get("measure_ref")
        .and_then(Json::as_str)
        .ok_or_else(|| miss("comparative.measure_ref required"))?;
    let mut arms = Vec::new();
    for arm in &arm_set {
        let a = arm.as_str();
        if let Some(h) = harnesses.get(a) {
            arms.push(ArmMeasure {
                arm_id: a.to_string(),
                measure_ref: measure_ref.to_string(),
                mean_trial_micros: h
                    .get("cost_micros")
                    .and_then(|c| c.get("mean"))
                    .and_then(Json::as_int)
                    .unwrap_or(0),
                p90_trial_micros: h
                    .get("cost_micros")
                    .and_then(|c| c.get("p90"))
                    .and_then(Json::as_int)
                    .unwrap_or(0),
                model_occupied_ms: h
                    .get("model_ms")
                    .and_then(|c| c.get("mean"))
                    .and_then(Json::as_int)
                    .unwrap_or(0) as u64,
                queue_wait_ms: h
                    .get("queue_wait_ms")
                    .and_then(|c| c.get("p90"))
                    .and_then(Json::as_int)
                    .unwrap_or(0) as u64,
            });
        } else {
            // Unmeasured arm — the binding planning prior is the figure;
            // the ref names the prereg record, never a measurement.
            let prior = outcome.p_hats.get(arm).copied().unwrap_or(0);
            arms.push(ArmMeasure {
                arm_id: a.to_string(),
                measure_ref: prior_refs
                    .get(a)
                    .and_then(Json::as_str)
                    .ok_or_else(|| {
                        miss(format!(
                            "comparative.prior_refs.{a} required for unmeasured arm"
                        ))
                    })?
                    .to_string(),
                mean_trial_micros: prior,
                p90_trial_micros: prior,
                model_occupied_ms: outcome.t_hats.get(arm).copied().unwrap_or(0) as u64,
                queue_wait_ms: 0,
            });
        }
    }
    let compute: Vec<ComputeLine> = cq
        .get("compute")
        .and_then(Json::as_arr)
        .ok_or_else(|| miss("comparative.compute[] required"))?
        .iter()
        .map(|c| {
            let cm = c
                .as_obj()
                .ok_or_else(|| miss("compute line must be an object"))?;
            Ok(ComputeLine {
                window: cm
                    .get("window")
                    .and_then(Json::as_str)
                    .ok_or_else(|| miss("compute.window required"))?
                    .to_string(),
                quote_id: cm
                    .get("quote_id")
                    .and_then(Json::as_str)
                    .ok_or_else(|| miss("compute.quote_id required"))?
                    .to_string(),
                ceiling_micros: req_int(cm, "ceiling_micros")?,
            })
        })
        .collect::<Result<_, FpEErr>>()?;
    let t = cq
        .get("t")
        .and_then(Json::as_int)
        .ok_or_else(|| miss("comparative.t required"))? as u32;
    let k = cq
        .get("k")
        .and_then(Json::as_int)
        .ok_or_else(|| miss("comparative.k required"))? as u32;
    let row_id = if full { "fp_e_full" } else { "fp_e_fallback" };
    let quote = build_quote(&ComparativeQuoteDecl {
        t,
        k,
        concurrency_c: input.c as u64,
        rows: vec![MenuRowDecl {
            row_id: row_id.to_string(),
            arms,
            infra_allowance_ppm: cq
                .get("infra_allowance_ppm")
                .and_then(Json::as_int)
                .unwrap_or(0) as u64,
            sim: cq.get("sim").map(|s| SimCells {
                bootstrap_hw_ppm: s
                    .get("bootstrap_hw_ppm")
                    .and_then(Json::as_int)
                    .unwrap_or(0) as u64,
                fm_hw_ppm: s.get("fm_hw_ppm").and_then(Json::as_int).unwrap_or(0) as u64,
                power_ppm: s.get("power_ppm").and_then(Json::as_int).unwrap_or(0) as u64,
                p_unresolved_ppm: s
                    .get("p_unresolved_ppm")
                    .and_then(Json::as_int)
                    .unwrap_or(0) as u64,
                source: s
                    .get("source")
                    .and_then(Json::as_str)
                    .unwrap_or("unresolved")
                    .to_string(),
            }),
        }],
        compute,
        envelope: match cq.get("envelope_ref").and_then(Json::as_str) {
            Some(r) => Envelope::Provisioned {
                envelope_ref: r.to_string(),
            },
            None => Envelope::PreQuote,
        },
    })
    .map_err(FpEErr::Comparative)?;

    // ── withheld labels — PP0 holds + checkpoint withholdings + the
    // declared labels (withheld{l1_not_passed} preserved verbatim) ──
    let mut withheld: Vec<Json> = m
        .get("withheld")
        .and_then(Json::as_arr)
        .cloned()
        .unwrap_or_default();
    if !full {
        withheld.push(Json::str(
            WithheldReason::parse("l1_not_passed")
                .map(|r| format!("withheld{{{}}}", r.as_str()))
                .unwrap_or_else(|_| "withheld{l1_not_passed}".to_string()),
        ));
    }
    for w in &outcome.withheld {
        withheld.push(Json::str(format!("withheld{{{}}}", w.reason.as_str())));
    }

    // The quote's `verdict` member is the precision verdict
    // (meets_target/below_target), a quote field — not an experiment
    // outcome (REQ-XH-121's firewall reads outcomes, not quote rows).
    let packet_json = quote.to_json();

    Ok(Json::obj([
        ("schema", Json::str(FP_E_SCHEMA)),
        ("ticket", m.get("ticket").cloned().unwrap_or(Json::Null)),
        (
            "recorded_at",
            m.get("recorded_at").cloned().unwrap_or(Json::Null),
        ),
        (
            "disposition",
            Json::obj([
                (
                    "disposition",
                    Json::str(if full { "full" } else { "fallback" }),
                ),
                (
                    "gl1",
                    Json::obj([
                        (
                            "result",
                            Json::str(if l1_passed { "pass" } else { "not_passed" }),
                        ),
                        ("pass_id", gl1_pass_id.map_or(Json::Null, |p| Json::str(&p))),
                        (
                            "h_identity_digest",
                            gl1.get("h_identity_digest").cloned().unwrap_or(Json::Null),
                        ),
                        (
                            "corpus_digest",
                            gl1.get("corpus_digest").cloned().unwrap_or(Json::Null),
                        ),
                    ]),
                ),
                (
                    "trigger",
                    disp.get("trigger").cloned().unwrap_or(Json::Null),
                ),
            ]),
        ),
        (
            "measurement",
            Json::obj([
                ("schema", Json::str(MEASURE_SCHEMA)),
                ("packet", packet.clone()),
            ]),
        ),
        (
            "arms",
            Json::Arr(arm_set.iter().map(|a| Json::str(a.as_str())).collect()),
        ),
        ("scope_decisions", Json::Arr(scope_decisions)),
        ("checkpoint", outcome.to_json()),
        ("caps", caps_rec),
        (
            "d_max",
            Json::obj([
                ("d_max_ms", Json::Int(d_max)),
                ("hourly_micros", Json::Int(w4.hourly_micros)),
                ("fixed_micros", Json::Int(w4.fixed_micros)),
                (
                    "accounting",
                    Json::str(
                        "billed where reconciled; cost_lib for completed-unreconciled; \
                         ceilings only for authorized-but-unrun (A2 2026-11-16)",
                    ),
                ),
            ]),
        ),
        ("w4_quote_inputs", w4_inputs),
        ("comparative_quote", packet_json),
        ("withheld", Json::Arr(withheld)),
        ("fallback", {
            if full {
                Json::Null
            } else {
                // The `hh-fp-fallback/1` decision record — emitted by
                // the holds machinery upstream; echoed verbatim.
                let fb = m.get("fallback").cloned().ok_or_else(|| {
                    miss("fallback disposition requires the hh-fp-fallback/1 record")
                })?;
                if fb.get("schema").and_then(Json::as_str) != Some(crate::holds::FALLBACK_SCHEMA) {
                    return Err(miss(format!(
                        "fallback must be a {} record",
                        crate::holds::FALLBACK_SCHEMA
                    )));
                }
                fb
            }
        }),
        (
            "acknowledgement",
            Json::obj([
                ("quote_authorizes", Json::Bool(false)),
                ("statement", Json::str(NO_AUTHORIZATION)),
                ("next_ticket", Json::str("HUMAN-H15")),
            ]),
        ),
    ]))
}

fn _unused() {}
