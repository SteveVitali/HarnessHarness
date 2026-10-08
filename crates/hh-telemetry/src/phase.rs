//! `turn_phase_profile(run, turn_id)` — M2's per-turn phase partition
//! (§5h.1 §2.2; ADR-0043 D3; R2.14).
//!
//! Every interval the fold reads is **member-stamped**, never a `ts`
//! difference (§2.6): the producing components stamp `at_ms{value,
//! measured_at}` on their boundary rows (R2.14's emitter legs), the
//! permission row carries `requested_at`/`wait_ms`, and the turn pair
//! carries `started_at_ms`/`turn_e2e_ms`. A boundary without its stamp
//! contributes an *unbounded* interval — counted in
//! `unbounded_intervals` — whose wall falls to the overhead buckets,
//! never fabricated into a phase (T-LCD-15).
//!
//! The partition is innermost-owns: a permission wait inside a
//! tool-blocking interval attributes to `permission_wait_ms`, the
//! remainder to `tool_blocking_ms`. `Σ members = turn wall` by
//! construction; `clock_skew_flag` fires when a stamped boundary escapes
//! `[started_at_ms, finish]` past `clock_tolerance_ms` (a mixed-clock
//! import) — the check is carried, never corrected.

use std::collections::BTreeMap;

use hh_ledger::event::EventEnvelope;
use hh_ledger::views::{View, ViewKind};
use hh_wire::json::Json;

use crate::views::measured_value;

/// The seven duration members the operator returns (§5h.1 §2.2 verbatim).
const PHASE_MEMBERS: &[&str] = &[
    "before_first_sampling_ms",
    "sampling_ms",
    "compaction_ms",
    "tool_blocking_ms",
    "permission_wait_ms",
    "between_sampling_overhead_ms",
    "idle_after_sampling_ms",
];

/// One stamped wall interval tagged with the phase it claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Interval {
    /// The phase bucket (index into [`PHASE_MEMBERS`]).
    phase: usize,
    /// The stamped open (ms).
    start: i64,
    /// The stamped close (ms).
    end: i64,
}

/// Push a stamped interval; an absent/reversed stamp pair counts one
/// unbounded interval (the wall falls to overhead — never fabricated).
fn push_interval(
    intervals: &mut Vec<Interval>,
    unbounded: &mut i64,
    open: Option<i64>,
    close: Option<i64>,
    phase: usize,
) {
    match (open, close) {
        (Some(s), Some(e)) if e >= s => intervals.push(Interval {
            phase,
            start: s,
            end: e,
        }),
        _ => *unbounded += 1,
    }
}

/// `n/a{observability}` — a stamped boundary the fold needs is absent.
fn na_cell() -> Json {
    Json::obj([("na", Json::str("observability"))])
}

/// The member-stamped `at_ms`/`{value}`/`requested_at` read of a payload.
fn stamp(payload: &Json, member: &str) -> Option<i64> {
    payload.get(member).and_then(measured_value)
}

/// `turn_phase_profile(run, turn_id, events, clock_tolerance_ms)` — the M2
/// phase partition as a pure fold (§2.2's operator). `events` is the durable
/// prefix; only rows inside the named turn's `[started, finished]` seq window
/// (and, when stamped, `scope.turn_id == turn_id`) are read.
pub fn turn_phase_profile(
    run_id: &str,
    turn_id: &str,
    events: &[EventEnvelope],
    clock_tolerance_ms: i64,
) -> View {
    let in_turn = |e: &&EventEnvelope| -> bool {
        e.payload
            .get("turn_id")
            .and_then(Json::as_str)
            .map(|t| t == turn_id)
            .unwrap_or_else(|| e.scope.turn_id.as_deref() == Some(turn_id))
    };

    // ── the turn boundary ────────────────────────────────────────────────
    let started = events
        .iter()
        .find(|e| e.class == "lifecycle.turn.started" && in_turn(e));
    let finished = events
        .iter()
        .find(|e| e.class == "lifecycle.turn.finished" && in_turn(e));
    let t0 = started.and_then(|e| stamp(&e.payload, "started_at_ms"));
    let e2e = finished.and_then(|e| stamp(&e.payload, "turn_e2e_ms"));
    let watermark = finished
        .map(|e| e.seq)
        .or_else(|| events.last().map(|e| e.seq));

    // Rows inside the turn's seq window (the boundary pair's seqs bound it;
    // scope/payload `turn_id` narrows further for multi-turn prefixes).
    let (lo, hi) = match (started, finished) {
        (Some(s), Some(f)) => (s.seq, f.seq),
        _ => (u64::MAX, 0),
    };
    let rows: Vec<&EventEnvelope> = events
        .iter()
        .filter(|e| e.seq >= lo && e.seq <= hi)
        .filter(|e| {
            e.scope
                .turn_id
                .as_deref()
                .map(|t| t == turn_id)
                .unwrap_or(true)
        })
        .collect();

    // ── request/retry counts (stamp-independent — always honest) ─────────
    let sampling_requests = rows
        .iter()
        .filter(|e| e.class == "model.call.requested")
        .count() as i64;
    // `sampling_retries` — retried *attempts*: per logical call,
    // `max(attempt_no) − 1` over `model.call.attempt.started`; a call with no
    // attempt rows counts its `control.retry.scheduled` rows (the
    // non-routed path's retry record). Both are durable facts, never
    // fabricated.
    let mut attempts_by_call: BTreeMap<String, i64> = BTreeMap::new();
    for e in rows
        .iter()
        .filter(|e| e.class == "model.call.attempt.started")
    {
        let mc = e
            .payload
            .get("model_call_id")
            .and_then(Json::as_str)
            .or(e.scope.model_call_id.as_deref())
            .unwrap_or_default()
            .to_string();
        let no = e
            .payload
            .get("attempt_no")
            .and_then(Json::as_int)
            .unwrap_or(1);
        let entry = attempts_by_call.entry(mc).or_insert(1);
        *entry = (*entry).max(no);
    }
    let mut sampling_retries: i64 = attempts_by_call.values().map(|m| (*m - 1).max(0)).sum();
    for e in rows.iter().filter(|e| e.class == "control.retry.scheduled") {
        let scope_call = e
            .payload
            .get("model_call_id")
            .and_then(Json::as_str)
            .or(e.scope.model_call_id.as_deref())
            .unwrap_or_default();
        if !attempts_by_call.contains_key(scope_call) {
            sampling_retries += 1;
        }
    }

    // ── stamped intervals per phase ──────────────────────────────────────
    let mut unbounded_intervals = 0i64;
    let mut intervals: Vec<Interval> = Vec::new();
    // Phase indices into PHASE_MEMBERS.
    const SAMPLING: usize = 1;
    const COMPACTION: usize = 2;
    const TOOL: usize = 3;
    const PERMISSION: usize = 4;

    // Sampling — `model.call.requested{at_ms}` → first terminal `at_ms`.
    for req in rows.iter().filter(|e| e.class == "model.call.requested") {
        let mc = req
            .payload
            .get("model_call_id")
            .and_then(Json::as_str)
            .or(req.scope.model_call_id.as_deref())
            .unwrap_or_default()
            .to_string();
        let open = stamp(&req.payload, "at_ms");
        let close = rows
            .iter()
            .filter(|e| {
                matches!(
                    e.class.as_str(),
                    "model.call.completed" | "model.call.failed"
                ) && e
                    .payload
                    .get("model_call_id")
                    .and_then(Json::as_str)
                    .or(e.scope.model_call_id.as_deref())
                    == Some(mc.as_str())
            })
            .min_by_key(|e| e.seq)
            .and_then(|e| stamp(&e.payload, "at_ms"));
        push_interval(
            &mut intervals,
            &mut unbounded_intervals,
            open,
            close,
            SAMPLING,
        );
    }

    // Compaction — `started{at_ms}` → `completed{at_ms}` paired in seq order
    // (compactions never nest; the emit order pairs them).
    let mut comp_open: Vec<i64> = Vec::new();
    for e in rows
        .iter()
        .filter(|e| e.class.starts_with("context.compaction."))
    {
        match e.class.as_str() {
            "context.compaction.started" => {
                comp_open.push(stamp(&e.payload, "at_ms").unwrap_or(-1));
            }
            "context.compaction.completed" => {
                let open = comp_open.pop().filter(|o| *o >= 0);
                let close = stamp(&e.payload, "at_ms").or_else(|| {
                    // A `duration_ms`-measured completion bounds the close
                    // off the open stamp (same clock — honest derivation).
                    match (open, stamp(&e.payload, "duration_ms")) {
                        (Some(o), Some(d)) => Some(o + d),
                        _ => None,
                    }
                });
                match (open, close) {
                    (Some(s), Some(e2)) => intervals.push(Interval {
                        phase: COMPACTION,
                        start: s,
                        end: e2,
                    }),
                    _ => unbounded_intervals += 1,
                }
            }
            _ => {}
        }
    }
    unbounded_intervals += comp_open.len() as i64; // unclosed starts

    // Tool blocking — `action.tool.proposed{tool_call_id, at_ms}` → the
    // tool's own terminal `at_ms` when minted, else the last linked effect
    // terminal (the `action.effect.intended{intent.tool_call_id}` →
    // effect terminal chain — the dispatcher-side half at C0).
    let mut intended_tc: BTreeMap<String, String> = BTreeMap::new(); // effect_id → tool_call_id
    for e in rows.iter().filter(|e| e.class == "action.effect.intended") {
        let ef = e
            .payload
            .get("effect_id")
            .and_then(Json::as_str)
            .or(e.scope.effect_id.as_deref())
            .unwrap_or_default()
            .to_string();
        if let Some(tc) = e
            .payload
            .get("intent")
            .and_then(|i| i.get("tool_call_id"))
            .and_then(Json::as_str)
        {
            intended_tc.insert(ef, tc.to_string());
        }
    }
    for prop in rows.iter().filter(|e| e.class == "action.tool.proposed") {
        let tc = prop
            .payload
            .get("tool_call_id")
            .and_then(Json::as_str)
            .or(prop.scope.tool_call_id.as_deref())
            .unwrap_or_default()
            .to_string();
        let open = stamp(&prop.payload, "at_ms");
        let own_terminal = rows
            .iter()
            .filter(|e| {
                matches!(
                    e.class.as_str(),
                    "action.tool.completed" | "action.tool.rejected"
                ) && e
                    .payload
                    .get("tool_call_id")
                    .and_then(Json::as_str)
                    .or(e.scope.tool_call_id.as_deref())
                    == Some(tc.as_str())
            })
            .max_by_key(|e| e.seq)
            .and_then(|e| stamp(&e.payload, "at_ms"));
        let linked_close = rows
            .iter()
            .filter(|e| {
                matches!(
                    e.class.as_str(),
                    "action.effect.observed"
                        | "action.effect.refused"
                        | "action.effect.unknown"
                        | "action.effect.abandoned"
                ) && e
                    .payload
                    .get("effect_id")
                    .and_then(Json::as_str)
                    .or(e.scope.effect_id.as_deref())
                    .and_then(|ef| intended_tc.get(ef))
                    == Some(&tc)
            })
            .max_by_key(|e| e.seq)
            .and_then(|e| stamp(&e.payload, "at_ms"));
        push_interval(
            &mut intervals,
            &mut unbounded_intervals,
            open,
            own_terminal.or(linked_close),
            TOOL,
        );
    }

    // Permission waits — `security.permission.decided{requested_at,
    // wait_ms}` — member-bounded on the monitor's clock.
    for e in rows
        .iter()
        .filter(|e| e.class == "security.permission.decided")
    {
        match (
            e.payload.get("requested_at").and_then(Json::as_int),
            e.payload.get("wait_ms").and_then(measured_value),
        ) {
            (Some(r), Some(w)) if w >= 0 => intervals.push(Interval {
                phase: PERMISSION,
                start: r,
                end: r.saturating_add(w),
            }),
            _ => {}
        }
    }

    // ── the partition ────────────────────────────────────────────────────
    // Without the boundary stamps the wall is unbounded: the counts stay
    // honest, every `*_ms` cell renders `n/a{observability}`.
    let mut payload_members: Vec<(String, Json)> = vec![
        ("kind".to_string(), Json::str("turn_phase_profile")),
        ("run_id".to_string(), Json::str(run_id)),
        ("turn_id".to_string(), Json::str(turn_id)),
    ];
    if let Some(e2e) = e2e {
        payload_members.push(("turn_e2e_ms".to_string(), Json::Int(e2e)));
    }
    payload_members.push((
        "sampling_requests".to_string(),
        Json::Int(sampling_requests),
    ));
    payload_members.push(("sampling_retries".to_string(), Json::Int(sampling_retries)));
    payload_members.push((
        "unbounded_intervals".to_string(),
        Json::Int(unbounded_intervals),
    ));

    let (t0, t1) = match (t0, e2e) {
        (Some(t0), Some(e2e)) => (t0, t0.saturating_add(e2e)),
        _ => {
            for m in PHASE_MEMBERS {
                payload_members.push((m.to_string(), na_cell()));
            }
            payload_members.push(("clock_skew_flag".to_string(), Json::Bool(false)));
            payload_members.push((
                "detail".to_string(),
                Json::str("turn boundary unmeasured — started_at_ms/turn_e2e_ms absent"),
            ));
            return View::stamped(
                run_id,
                ViewKind::TurnPhaseProfile,
                watermark,
                Json::Obj(payload_members.into_iter().collect()),
            );
        }
    };

    // Clamp every interval to the turn wall; a stamped boundary escaping
    // the wall past tolerance is skew evidence (never silently clipped).
    let mut clock_skew_flag = false;
    for i in &mut intervals {
        if i.start < t0 - clock_tolerance_ms || i.end > t1 + clock_tolerance_ms {
            clock_skew_flag = true;
        }
        i.start = i.start.max(t0);
        i.end = i.end.min(t1).max(i.start);
    }

    // Innermost-owns carve — the claim order names the nesting the
    // contract fixes (permission inside tool/effect, then sampling, then
    // tool-blocking, then compaction). `claimed` accumulates wall already
    // attributed to a deeper phase.
    let mut claimed: Vec<(i64, i64)> = Vec::new();
    let mut phase_ms = [0i64; 4]; // [sampling, compaction, tool, permission]
                                  // `covered(phase)` — union of the phase's intervals minus `claimed`.
    let coverage = |ivs: &[Interval], phase: usize, claimed: &[(i64, i64)]| -> Vec<(i64, i64)> {
        let mut spans: Vec<(i64, i64)> = ivs
            .iter()
            .filter(|i| i.phase == phase)
            .map(|i| (i.start, i.end))
            .collect();
        spans.sort();
        // Merge the phase's own overlaps first.
        let mut merged: Vec<(i64, i64)> = Vec::new();
        for (s, e) in spans.drain(..) {
            match merged.last_mut() {
                Some((_, me)) if s <= *me => *me = (*me).max(e),
                _ => merged.push((s, e)),
            }
        }
        // Subtract already-claimed wall.
        let mut out = Vec::new();
        for (s, e) in merged {
            let mut segs = vec![(s, e)];
            for &(cs, ce) in claimed {
                let mut next = Vec::new();
                for (a, b) in segs {
                    if ce <= a || cs >= b {
                        next.push((a, b));
                    } else {
                        if a < cs {
                            next.push((a, cs));
                        }
                        if ce < b {
                            next.push((ce, b));
                        }
                    }
                }
                segs = next;
            }
            out.extend(segs.into_iter().filter(|(a, b)| b > a));
        }
        out
    };
    // Permission waits are innermost.
    for phase in [PERMISSION, SAMPLING, TOOL, COMPACTION] {
        let cov = coverage(&intervals, phase, &claimed);
        phase_ms[phase - 1] = cov.iter().map(|(a, b)| b - a).sum();
        claimed.extend(cov);
    }

    // The overhead buckets — the unclaimed wall, sliced by position
    // against the sampling span.
    let mut unclaimed: Vec<(i64, i64)> = vec![(t0, t1)];
    for &(cs, ce) in &claimed {
        let mut next = Vec::new();
        for (a, b) in unclaimed {
            if ce <= a || cs >= b {
                next.push((a, b));
            } else {
                if a < cs {
                    next.push((a, cs));
                }
                if ce < b {
                    next.push((ce, b));
                }
            }
        }
        unclaimed = next;
    }
    let first_sampling = intervals
        .iter()
        .filter(|i| i.phase == SAMPLING)
        .map(|i| i.start)
        .min();
    let last_sampling = intervals
        .iter()
        .filter(|i| i.phase == SAMPLING)
        .map(|i| i.end)
        .max();
    let mut before_first = 0i64;
    let mut between_overhead = 0i64;
    let mut idle_after = 0i64;
    for (a, b) in &unclaimed {
        match (first_sampling, last_sampling) {
            (Some(fs), Some(ls)) => {
                // Slice the unclaimed segment at the sampling span bounds.
                let ab = (*a, *b);
                let first = (ab.0.max(t0), ab.1.min(fs));
                if first.1 > first.0 {
                    before_first += first.1 - first.0;
                }
                let mid = (ab.0.max(fs), ab.1.min(ls));
                if mid.1 > mid.0 {
                    between_overhead += mid.1 - mid.0;
                }
                let tail = (ab.0.max(ls), ab.1.min(t1));
                if tail.1 > tail.0 {
                    idle_after += tail.1 - tail.0;
                }
            }
            _ => {
                // No sampling span — the whole unclaimed wall precedes the
                // (absent) first call.
                before_first += b - a;
            }
        }
    }

    let values = [
        before_first,
        phase_ms[SAMPLING - 1],
        phase_ms[COMPACTION - 1],
        phase_ms[TOOL - 1],
        phase_ms[PERMISSION - 1],
        between_overhead,
        idle_after,
    ];
    for (m, v) in PHASE_MEMBERS.iter().zip(values.iter()) {
        payload_members.push((m.to_string(), Json::Int(*v)));
    }
    // The partition check — Σ members vs `turn_e2e_ms` within tolerance.
    let sum: i64 = values.iter().sum();
    if (sum - (t1 - t0)).abs() > clock_tolerance_ms {
        clock_skew_flag = true;
    }
    payload_members.push(("clock_skew_flag".to_string(), Json::Bool(clock_skew_flag)));

    View::stamped(
        run_id,
        ViewKind::TurnPhaseProfile,
        watermark,
        Json::Obj(payload_members.into_iter().collect()),
    )
}
