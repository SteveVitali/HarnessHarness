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
//! ```
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
use crate::holds::{native_lane_hold, native_lane_hold_record};
use crate::json_ext::JsonExt;

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
        other => Err(schema(format!("op {other} unknown"))),
    }
}

/// `respond(text)` — parse + [`handle`]; the refusal becomes a typed
/// `{"refused": …}` body and `false`.
pub fn respond(text: &str) -> (Json, bool) {
    let parsed = match hharness_wire::json::parse(text) {
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
