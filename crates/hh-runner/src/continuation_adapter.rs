//! `continuation_adapter` — the W4 driver's transport onto the one
//! continuation module (manifest 325a/R6.52 repair; R6.29 X-04/CC1:
//! "one projection formula and one continuation module consumed by
//! FP-E and W4").
//!
//! The R6.52 W4 driver (`scripts/gce-backend/w4-main-driver.py`)
//! originally re-implemented the round-boundary checkpoint in Python
//! with a different wall formula, no orphan/C×s reserve, float
//! truncation and an ignored protected-pair breach. This adapter adds
//! **no rule of its own**: it strictly decodes a request envelope,
//! calls [`continuation::run_checkpoint`], [`continuation::admit_slot`]
//! or [`holds::native_lane_hold`] and returns their canonical JSON. A
//! decode failure, an outcome member anywhere in the request, or a
//! rule refusal returns a typed `refused` member — the driver treats
//! that as "stop admitting", never as a default.
//!
//! Request envelope (`hh-fp-continuation-adapter/1`, a process-local
//! transport shape — never a ledger record):
//!
//! ```text
//! {"schema":"hh-fp-continuation-adapter/1","op":"checkpoint",
//!  "input":<hh-fp-continuation-input/1>,
//!  "wall":{"d_limit_ms":…,"cutoff_ms":…,"overhead_ms":…,"load_ms":…,
//!          "pairs":[{"o_slot":…,"h_slot":…}]} | null}
//! {"schema":…,"op":"admit","input":<…>,"arm":"v5",
//!  "caps_micros":{"v5":…,"pi":…,"claude-code":…}}
//! {"schema":…,"op":"native_lane_hold","arm":"v5","probe_passed":true,
//!  "first3_infra":[false,true],"remaining":["…"],"cause":"…"}
//! {"schema":…,"op":"lane_probe_plan","pricing":<hh-w4-probe-pricing/1>,
//!  "surface":"openai.chat","effort_paths":["high"]}
//! {"schema":…,"op":"lane_probe_record", …plan members…,
//!  "slot":{"token_ref":…,"slice_micros":…},"recorded_at":…,
//!  "probe_slot_id":…,"phase_run_id":…,"observations":[…]}
//! {"schema":…,"op":"probe_precedes_native","probe_end_seq":…,
//!  "first_native_seq":…}
//! ```
//!
//! The lane-probe ops (R6.52 review M3) route the probe through the
//! canonical `holds.rs` driver: `lane_probe_plan` + `lane_probe_bound`
//! price the whole plan through the one pricing implementation before
//! any call, and `run_lane_probe` emits the `hh-lane-probe/1` record
//! from a transport whose `ProbeOutcome` is built **only** from a
//! receipt verified against its content address (`idp::address`, the
//! single content-addressing primitive) and bound to the probe slot,
//! phase run, planned route, model and effort. A cost with no
//! attribution row is recorded `null` + `cost: unknown{…}`, never 0.
//! The request must arrive in canonical form (`parse_canonical`): a
//! duplicate key refuses rather than resolving last-wins.
//!
//! `caps_micros` is the **declared** FP-E cap table (REQ-XH-117) — the
//! adapter never recomputes a cap from measurements taken after
//! spending began.

use std::collections::BTreeMap;

use hharness_wire::json::Json;

use crate::continuation::{
    self, admit_slot, check_cost_only, run_checkpoint, schema, AdmitVerdict, ContinuationError,
    ContinuationInput, FpArm, OhPair, WallInput,
};
use crate::holds::{
    lane_probe_bound, lane_probe_plan, native_lane_hold, native_lane_hold_record,
    probe_precedes_native, run_lane_probe, LANE_PROBE_CAP_MICROS,
};
use crate::json_ext::JsonExt;
use crate::ps0::{ProbeCall, ProbeOutcome, ProbeTransport, Ps0Error, SlotGrant};
use hharness_budget::pricing::{PricingRow, PricingTable, RateUnit, RatedPricingTable};
use hharness_budget::ModelRef;

/// The request envelope's schema tag.
pub const ADAPTER_SCHEMA: &str = "hh-fp-continuation-adapter/1";

fn strict_keys(
    m: &BTreeMap<String, Json>,
    allowed: &[&str],
    at: &str,
) -> Result<(), ContinuationError> {
    for k in m.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(schema(format!("{at}{k} is not a declared member")));
        }
    }
    Ok(())
}

fn req_int(m: &BTreeMap<String, Json>, k: &str, at: &str) -> Result<i64, ContinuationError> {
    m.get(k)
        .and_then(Json::as_int)
        .ok_or_else(|| schema(format!("{at}{k} required (integer)")))
}

fn req_arm(m: &BTreeMap<String, Json>) -> Result<FpArm, ContinuationError> {
    let s = m
        .get("arm")
        .and_then(Json::as_str)
        .ok_or_else(|| schema("arm required"))?;
    FpArm::parse(s).ok_or_else(|| schema(format!("arm {s} is not a first-pass arm")))
}

/// `wall_from_json(j)` — strict decode of the declared wall inputs.
/// Every member is required: a missing pinned input is a refusal,
/// never zero.
pub fn wall_from_json(j: &Json) -> Result<WallInput, ContinuationError> {
    let m = j.as_obj().ok_or_else(|| schema("wall must be an object"))?;
    strict_keys(
        m,
        &["d_limit_ms", "cutoff_ms", "overhead_ms", "load_ms", "pairs"],
        "wall.",
    )?;
    let pairs = m
        .get("pairs")
        .and_then(Json::as_arr)
        .ok_or_else(|| schema("wall.pairs required (array)"))?
        .iter()
        .map(|p| {
            let pm = p
                .as_obj()
                .ok_or_else(|| schema("wall.pairs[] must be objects"))?;
            strict_keys(pm, &["o_slot", "h_slot"], "wall.pairs[].")?;
            let s = |k: &str| {
                pm.get(k)
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| schema(format!("wall.pairs[].{k} required")))
            };
            Ok(OhPair {
                o_slot: s("o_slot")?,
                h_slot: s("h_slot")?,
            })
        })
        .collect::<Result<Vec<_>, ContinuationError>>()?;
    Ok(WallInput {
        d_limit_ms: req_int(m, "d_limit_ms", "wall.")?,
        cutoff_ms: req_int(m, "cutoff_ms", "wall.")?,
        overhead_ms: req_int(m, "overhead_ms", "wall.")?,
        load_ms: req_int(m, "load_ms", "wall.")?,
        pairs,
    })
}

fn verdict_json(arm: FpArm, v: &AdmitVerdict) -> Json {
    match v {
        AdmitVerdict::Admit => Json::obj([
            ("arm", Json::str(arm.as_str())),
            ("verdict", Json::str("admit")),
        ]),
        AdmitVerdict::Capped {
            expected_micros,
            cap_micros,
        } => Json::obj([
            ("arm", Json::str(arm.as_str())),
            ("verdict", Json::str("capped")),
            ("expected_micros", Json::Int(*expected_micros)),
            ("cap_micros", Json::Int(*cap_micros)),
            (
                "disposition",
                Json::str(
                    crate::disposition::Disposition::Withheld {
                        reason: crate::disposition::WithheldReason::Budget,
                    }
                    .as_str(),
                ),
            ),
            ("label", Json::str(continuation::DROP_LABEL)),
        ]),
        AdmitVerdict::ProtectedByRoot => Json::obj([
            ("arm", Json::str(arm.as_str())),
            ("verdict", Json::str("protected_by_root")),
        ]),
    }
}

/// The probe pricing input (`hh-w4-probe-pricing/1`): the served model
/// and its provider-published per-1M rates, declared into the one
/// pricing implementation via [`PricingRow::from_micro_per_1m`] (an
/// unrepresentable rate refuses typed; nothing is rounded).
pub const PROBE_PRICING_SCHEMA: &str = "hh-w4-probe-pricing/1";

fn req_str<'a>(
    m: &'a BTreeMap<String, Json>,
    k: &str,
    at: &str,
) -> Result<&'a str, ContinuationError> {
    m.get(k)
        .and_then(Json::as_str)
        .ok_or_else(|| schema(format!("{at}{k} required (string)")))
}

/// `probe_pricing_from_json(j)` — strict decode → `(ModelRef, table)`.
pub fn probe_pricing_from_json(
    j: &Json,
) -> Result<(ModelRef, RatedPricingTable), ContinuationError> {
    let m = j
        .as_obj()
        .ok_or_else(|| schema("pricing must be an object"))?;
    strict_keys(m, &["schema", "model_ref", "table"], "pricing.")?;
    if m.get("schema").and_then(Json::as_str) != Some(PROBE_PRICING_SCHEMA) {
        return Err(schema(format!(
            "pricing.schema must be {PROBE_PRICING_SCHEMA}"
        )));
    }
    let mr = m
        .get("model_ref")
        .and_then(Json::as_obj)
        .ok_or_else(|| schema("pricing.model_ref required"))?;
    strict_keys(
        mr,
        &[
            "profile_ref",
            "provider_model_id",
            "serving_route",
            "effort",
        ],
        "pricing.model_ref.",
    )?;
    let model_ref = ModelRef::from_json(m.get("model_ref").unwrap_or(&Json::Null))
        .ok_or_else(|| schema("pricing.model_ref incomplete"))?;
    let t = m
        .get("table")
        .and_then(Json::as_obj)
        .ok_or_else(|| schema("pricing.table required"))?;
    strict_keys(
        t,
        &[
            "table_id",
            "version",
            "currency",
            "rate_unit",
            "input_micro_per_1m",
            "cache_read_micro_per_1m",
            "output_micro_per_1m",
            "valid_from",
            "source",
        ],
        "pricing.table.",
    )?;
    let unit = RateUnit::parse(req_str(t, "rate_unit", "pricing.table.")?)
        .ok_or_else(|| schema("pricing.table.rate_unit unknown"))?;
    let row = PricingRow::from_micro_per_1m(
        unit,
        &model_ref.pricing_key(),
        req_int(t, "input_micro_per_1m", "pricing.table.")?,
        req_int(t, "cache_read_micro_per_1m", "pricing.table.")?,
        req_int(t, "output_micro_per_1m", "pricing.table.")?,
        req_str(t, "valid_from", "pricing.table.")?,
        req_str(t, "source", "pricing.table.")?,
    )
    .map_err(|e| schema(format!("pricing.table: {e:?}")))?;
    let table = PricingTable {
        table_id: req_str(t, "table_id", "pricing.table.")?.to_string(),
        version: req_str(t, "version", "pricing.table.")?.to_string(),
        currency: req_str(t, "currency", "pricing.table.")?.to_string(),
        rows: vec![row],
    }
    .at_unit(unit);
    Ok((model_ref, table))
}

/// The plan + its priced whole-plan bound — refused before any call.
fn probe_plan(
    m: &BTreeMap<String, Json>,
) -> Result<(ModelRef, RatedPricingTable, Vec<ProbeCall>, i64), ContinuationError> {
    let (model_ref, table) =
        probe_pricing_from_json(m.get("pricing").ok_or_else(|| schema("pricing required"))?)?;
    let surface = req_str(m, "surface", "")?;
    let efforts: Vec<String> = m
        .get("effort_paths")
        .and_then(Json::as_arr)
        .ok_or_else(|| schema("effort_paths required (array)"))?
        .iter()
        .map(|e| {
            e.as_str()
                .map(str::to_string)
                .ok_or_else(|| schema("effort_paths[] must be strings"))
        })
        .collect::<Result<_, _>>()?;
    let refs: Vec<&str> = efforts.iter().map(String::as_str).collect();
    let calls = lane_probe_plan(&model_ref.provider_model_id, surface, &refs)?;
    let worst =
        lane_probe_bound(&calls, &model_ref, &table).map_err(|r| ContinuationError::Schema {
            detail: r.to_string(),
        })?;
    Ok((model_ref, table, calls, worst))
}

fn digest_ref(bytes: &[u8]) -> String {
    hharness_identity::idp::address(bytes, "application/octet-stream").id()
}

/// One observed probe call, as the driver read it off the proxy WAL +
/// blob pool — unverified until [`VerifiedReceipts`] checks it.
#[derive(Debug, Clone)]
struct Observation {
    http_status: i64,
    terminal: Option<(String, String, String)>, // (class, disposition, response_ref)
    receipt: Option<String>,
    downstream_body: Option<String>,
    cost_micros: Option<i64>,
}

fn observation_from_json(j: &Json, probe_id: &str) -> Result<Observation, ContinuationError> {
    let m = j
        .as_obj()
        .ok_or_else(|| schema("observations[] must be objects"))?;
    strict_keys(
        m,
        &[
            "probe_id",
            "http_status",
            "terminal",
            "receipt",
            "downstream_body",
            "cost_micros",
        ],
        "observations[].",
    )?;
    if req_str(m, "probe_id", "observations[].")? != probe_id {
        return Err(schema(format!("observation order: expected {probe_id}")));
    }
    let opt_str = |k: &str| m.get(k).and_then(Json::as_str).map(str::to_string);
    let terminal = match m.get("terminal") {
        None | Some(Json::Null) => None,
        Some(t) => {
            let tm = t
                .as_obj()
                .ok_or_else(|| schema("terminal must be an object"))?;
            strict_keys(tm, &["class", "disposition", "response_ref"], "terminal.")?;
            Some((
                req_str(tm, "class", "terminal.")?.to_string(),
                req_str(tm, "disposition", "terminal.")?.to_string(),
                req_str(tm, "response_ref", "terminal.")?.to_string(),
            ))
        }
    };
    Ok(Observation {
        http_status: req_int(m, "http_status", "observations[].")?,
        terminal,
        receipt: opt_str("receipt"),
        downstream_body: opt_str("downstream_body"),
        cost_micros: m.get("cost_micros").and_then(Json::as_int),
    })
}

/// The probe transport over already-observed calls: a `pass` is built
/// only from a content-address-verified receipt bound to this slot,
/// phase run, route, model and effort, from a forwarded 2xx completion
/// whose known cost is within the $0.05 cap.
struct VerifiedReceipts {
    obs: Vec<Observation>,
    next: usize,
    probe_slot_id: String,
    phase_run_id: String,
}

impl VerifiedReceipts {
    fn verdict(&self, o: &Observation, call: &ProbeCall) -> (String, String) {
        let Some((class, disposition, rref)) = &o.terminal else {
            return ("unreceipted{no_bound_receipt}".into(), String::new());
        };
        let Some(receipt) = &o.receipt else {
            return ("failed{receipt_unreadable}".into(), rref.clone());
        };
        if digest_ref(receipt.as_bytes()) != *rref {
            return ("failed{receipt_hash_mismatch}".into(), rref.clone());
        }
        let Ok(rec) = hharness_wire::canonical::parse_canonical(receipt.as_bytes()) else {
            return ("failed{receipt_noncanonical}".into(), rref.clone());
        };
        let px = rec.get("proxy").cloned().unwrap_or(Json::Null);
        let s = |j: &Json, k: &str| j.get(k).and_then(Json::as_str).map(str::to_string);
        if s(&px, "slot_id").as_deref() != Some(self.probe_slot_id.as_str())
            || s(&px, "phase_run_id").as_deref() != Some(self.phase_run_id.as_str())
        {
            return ("failed{receipt_unbound}".into(), rref.clone());
        }
        let dn = px.get("downstream").cloned().unwrap_or(Json::Null);
        if s(&dn, "path").as_deref() != Some(call.route.as_str()) {
            return ("failed{receipt_route_mismatch}".into(), rref.clone());
        }
        let body_ok = match (&o.downstream_body, s(&dn, "body_ref")) {
            (Some(b), Some(bref)) if digest_ref(b.as_bytes()) == bref => {
                match hharness_wire::json::parse(b) {
                    Ok(body) => {
                        body.get("model") == call.body.get("model")
                            && body.get("reasoning_effort") == call.body.get("reasoning_effort")
                    }
                    Err(_) => false,
                }
            }
            _ => false,
        };
        if !body_ok {
            return ("failed{downstream_mismatch}".into(), rref.clone());
        }
        if class != "model.call.completed" || disposition != "forwarded" {
            return (format!("failed{{terminal:{disposition}}}"), rref.clone());
        }
        if !(200..300).contains(&o.http_status) {
            return (format!("failed{{http:{}}}", o.http_status), rref.clone());
        }
        if o.cost_micros.is_some_and(|c| c > LANE_PROBE_CAP_MICROS) {
            return ("failed{probe_cap_exceeded}".into(), rref.clone());
        }
        ("pass".into(), rref.clone())
    }
}

impl ProbeTransport for VerifiedReceipts {
    fn call(&mut self, _slot: &SlotGrant, call: &ProbeCall) -> Result<ProbeOutcome, Ps0Error> {
        let o = self.obs.get(self.next).cloned().ok_or_else(|| Ps0Error {
            detail: format!("no observation for {}", call.probe_id),
        })?;
        self.next += 1;
        let (disposition, receipt_ref) = self.verdict(&o, call);
        Ok(ProbeOutcome {
            disposition,
            receipt_ref,
            usage: None,
            cost_micros: o.cost_micros.unwrap_or(0),
        })
    }
}

fn lane_probe_record(m: &BTreeMap<String, Json>) -> Result<Json, ContinuationError> {
    let (model_ref, table, calls, _worst) = probe_plan(m)?;
    let sm = m
        .get("slot")
        .and_then(Json::as_obj)
        .ok_or_else(|| schema("slot required"))?;
    strict_keys(sm, &["token_ref", "slice_micros"], "slot.")?;
    let slot = SlotGrant {
        token_ref: req_str(sm, "token_ref", "slot.")?.to_string(),
        slice_micros: req_int(sm, "slice_micros", "slot.")?,
    };
    let raw = m
        .get("observations")
        .and_then(Json::as_arr)
        .ok_or_else(|| schema("observations required (array)"))?;
    if raw.len() != calls.len() {
        return Err(schema("one observation per planned call required"));
    }
    let obs = raw
        .iter()
        .zip(calls.iter())
        .map(|(o, c)| observation_from_json(o, &c.probe_id))
        .collect::<Result<Vec<_>, _>>()?;
    let unknown: Vec<bool> = obs.iter().map(|o| o.cost_micros.is_none()).collect();
    let mut t = VerifiedReceipts {
        obs,
        next: 0,
        probe_slot_id: req_str(m, "probe_slot_id", "")?.to_string(),
        phase_run_id: req_str(m, "phase_run_id", "")?.to_string(),
    };
    let stage = run_lane_probe(
        &calls,
        &slot,
        &mut t,
        &model_ref,
        &table,
        req_str(m, "recorded_at", "")?,
    )
    .map_err(|r| ContinuationError::Schema {
        detail: r.to_string(),
    })?;
    let mut rec = stage.to_json();
    // An absent attribution row is an unknown cost — never an implied 0.
    if let Json::Obj(top) = &mut rec {
        if let Some(Json::Arr(cs)) = top.get_mut("calls") {
            for (c, u) in cs.iter_mut().zip(unknown) {
                if let (Json::Obj(cm), true) = (c, u) {
                    cm.insert("cost_micros".into(), Json::Null);
                    cm.insert("cost".into(), Json::str("unknown{no_attribution_row}"));
                }
            }
        }
    }
    Ok(rec)
}

/// `handle(request)` — one adapter request → its canonical response.
/// `Err` is a typed refusal the caller must treat as "stop admitting".
pub fn handle(req: &Json) -> Result<Json, ContinuationError> {
    // The firewall first: no outcome member anywhere in the envelope.
    check_cost_only(req)?;
    let m = req
        .as_obj()
        .ok_or_else(|| schema("request must be an object"))?;
    if m.get("schema").and_then(Json::as_str) != Some(ADAPTER_SCHEMA) {
        return Err(schema(format!("schema must be {ADAPTER_SCHEMA}")));
    }
    let op = m
        .get("op")
        .and_then(Json::as_str)
        .ok_or_else(|| schema("op required"))?;
    match op {
        "checkpoint" => {
            strict_keys(m, &["schema", "op", "input", "wall"], "")?;
            let input = ContinuationInput::from_json(
                m.get("input").ok_or_else(|| schema("input required"))?,
            )?;
            let wall = match m.get("wall") {
                None | Some(Json::Null) => None,
                Some(w) => Some(wall_from_json(w)?),
            };
            let out = run_checkpoint(&input, wall.as_ref())?;
            let rec = out.to_json();
            // Second pair of eyes: the emitted readout is cost-only too.
            check_cost_only(&rec)?;
            Ok(Json::obj([
                ("op", Json::str("checkpoint")),
                ("outcome", rec),
            ]))
        }
        "admit" => {
            strict_keys(m, &["schema", "op", "input", "arm", "caps_micros"], "")?;
            let input = ContinuationInput::from_json(
                m.get("input").ok_or_else(|| schema("input required"))?,
            )?;
            input.priority.validate()?;
            let arm = req_arm(m)?;
            let mut caps = BTreeMap::new();
            let cm = m
                .get("caps_micros")
                .and_then(Json::as_obj)
                .ok_or_else(|| schema("caps_micros required (declared FP-E cap table)"))?;
            for (a, v) in cm {
                let ca = FpArm::parse(a)
                    .ok_or_else(|| schema(format!("caps_micros.{a} is not an arm")))?;
                caps.insert(
                    ca,
                    v.as_int()
                        .ok_or_else(|| schema(format!("caps_micros.{a} must be an integer")))?,
                );
            }
            if !input.priority.is_protected(arm) && !caps.contains_key(&arm) {
                return Err(schema(format!(
                    "caps_micros.{} absent — an unprotected arm needs its declared cap",
                    arm.as_str()
                )));
            }
            let v = admit_slot(arm, &input, &caps);
            Ok(Json::obj([
                ("op", Json::str("admit")),
                ("admission", verdict_json(arm, &v)),
            ]))
        }
        "native_lane_hold" => {
            strict_keys(
                m,
                &[
                    "schema",
                    "op",
                    "arm",
                    "probe_passed",
                    "first3_infra",
                    "remaining",
                    "cause",
                ],
                "",
            )?;
            let arm = req_arm(m)?;
            let probe_passed = m
                .get("probe_passed")
                .and_then(Json::as_bool)
                .ok_or_else(|| schema("probe_passed required (bool)"))?;
            let first3: Vec<bool> = m
                .get("first3_infra")
                .and_then(Json::as_arr)
                .ok_or_else(|| schema("first3_infra required (array)"))?
                .iter()
                .map(|b| {
                    b.as_bool()
                        .ok_or_else(|| schema("first3_infra[] must be booleans"))
                })
                .collect::<Result<_, _>>()?;
            if first3.len() > 3 {
                return Err(schema(
                    "first3_infra carries at most the arm's first 3 slots",
                ));
            }
            let remaining: Vec<String> = m
                .get("remaining")
                .and_then(Json::as_arr)
                .ok_or_else(|| schema("remaining required (array)"))?
                .iter()
                .map(|s| {
                    s.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| schema("remaining[] must be strings"))
                })
                .collect::<Result<_, _>>()?;
            let cause = m
                .get("cause")
                .and_then(Json::as_str)
                .ok_or_else(|| schema("cause required"))?;
            let hold = native_lane_hold(arm, probe_passed, &first3, remaining)?;
            Ok(Json::obj([
                ("op", Json::str("native_lane_hold")),
                (
                    "hold",
                    hold.map(|a| native_lane_hold_record(arm, &a, cause))
                        .unwrap_or(Json::Null),
                ),
            ]))
        }
        "lane_probe_plan" => {
            strict_keys(
                m,
                &["schema", "op", "pricing", "surface", "effort_paths"],
                "",
            )?;
            let (_, _, calls, worst) = probe_plan(m)?;
            Ok(Json::obj([
                ("op", Json::str("lane_probe_plan")),
                ("worst_case_micros", Json::Int(worst)),
                ("cap_micros", Json::Int(LANE_PROBE_CAP_MICROS)),
                (
                    "calls",
                    Json::Arr(
                        calls
                            .iter()
                            .map(|c| {
                                Json::obj([
                                    ("probe_id", Json::str(&c.probe_id)),
                                    ("surface", Json::str(&c.surface)),
                                    ("route", Json::str(&c.route)),
                                    ("body", c.body.clone()),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]))
        }
        "lane_probe_record" => {
            strict_keys(
                m,
                &[
                    "schema",
                    "op",
                    "pricing",
                    "surface",
                    "effort_paths",
                    "slot",
                    "recorded_at",
                    "probe_slot_id",
                    "phase_run_id",
                    "observations",
                ],
                "",
            )?;
            Ok(Json::obj([
                ("op", Json::str("lane_probe_record")),
                ("stage", lane_probe_record(m)?),
            ]))
        }
        "probe_precedes_native" => {
            strict_keys(
                m,
                &["schema", "op", "probe_end_seq", "first_native_seq"],
                "",
            )?;
            Ok(Json::obj([
                ("op", Json::str("probe_precedes_native")),
                (
                    "precedes",
                    Json::Bool(probe_precedes_native(
                        req_int(m, "probe_end_seq", "")?,
                        req_int(m, "first_native_seq", "")?,
                    )),
                ),
            ]))
        }
        other => Err(schema(format!("op {other} unknown"))),
    }
}

/// `respond(text)` — strict canonical parse + [`handle`]; the refusal becomes a typed
/// `{"refused": …}` body and `false`.
pub fn respond(text: &str) -> (Json, bool) {
    // Strict: the canonical form only — a duplicate key (last-wins in
    // the lenient reader), reordering or whitespace refuses.
    let parsed = match hharness_wire::canonical::parse_canonical(text.as_bytes()) {
        Ok(j) => j,
        Err(e) => {
            return (
                Json::obj([("refused", Json::str(format!("schema: {e}")))]),
                false,
            )
        }
    };
    match handle(&parsed) {
        Ok(j) => (j, true),
        Err(e) => (Json::obj([("refused", Json::str(e.to_string()))]), false),
    }
}
