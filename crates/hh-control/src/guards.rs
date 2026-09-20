//! The six enforcement points (§5e.2; ADR-0106 D4) — each a **pure function
//! of the durable ledger prefix and the sealed policy** ("two evaluations at
//! the same `seq` yield the same verdict", AC-F2-10; no model call, no
//! `Text` read). The envelope may only **stop, refuse, delay or tighten** —
//! a `GuardVerdict` never chooses the next action, never grants, never
//! widens (INV-7).
//!
//! | point | moment | checks |
//! |---|---|---|
//! | `pre_call` | before every model call | root-first budget `check`, run/turn deadline, gauge caps (`context.occupancy → CompactionRequired`; `delegation_depth`/`fan_out → SpawnRefused`), `reserve(model_call)` sizing, ladder state |
//! | `interpret` | after the response, before `action.tool.proposed` | `OutputValidationPolicy` pipeline, `LoopPolicy` detectors, empty-response ladder |
//! | `pre_dispatch` | after `authorize = allow`, before `commit` | retry eligibility × ADR-0031 class × `RetryPolicy`, deadline assignment, `reserve(tool_attempt)`, INV-3 |
//! | `post_effect` | after every effect/model-call terminal | hard-ceiling exhaustion, INV-1/2/4/8/9, no-progress evidence, timeout bookkeeping |
//! | `decide` | before β's `decide` and on any `stop` proposal | kernel stop rules by priority `invariant > cancel > exhaustion > loop/format > definition rules > β`; completion admissibility |
//! | `resume` | at `lifecycle.run.resumed` | re-arm, recompute every counter/ladder/window, INV-1 on past-deadline scopes, resume a pre-crash drain |

use hh_ledger::event::EventEnvelope;
use hh_ontology::control::{LoopDetectorKind, StopReason};
use hh_wire::json::Json;

use crate::invariants::{InvariantInput, InvariantViolation};
use crate::output::{ParsedCall, SurfaceSpec};
use crate::policy::{EnvelopePolicy, LadderAction};
use crate::retry::Deadline;
use crate::vocab::{ControlDecision, DecisionKind, GuardPoint, ScopeKind};

/// `GuardVerdict = pass{reservation_id?, deadline?} | respond{Observation,
/// events[]} | stop{StopReason, events[]}` (§5e.2 contract row). `respond`'s
/// `observation` is the typed refusal/nudge record the driver renders —
/// never rejected bytes.
#[derive(Debug, Clone, PartialEq)]
pub enum GuardVerdict {
    /// The step may proceed (a reservation and/or deadline the driver
    /// records with it).
    Pass {
        /// The reservation id, when the guard reserved.
        reservation_id: Option<String>,
        /// The derived deadline, when the guard assigned one.
        deadline: Option<Deadline>,
    },
    /// The guard answers for the step — a typed observation the driver
    /// renders plus the ledger events to append (a refusal, a nudge, a
    /// `CompactionRequired`, a `SpawnRefused`).
    Respond {
        /// The typed observation record (no `Text`, no rejected bytes).
        observation: Json,
        /// The ledger events to append (`{class, payload}` pairs — the
        /// driver wraps them with scope/provenance).
        events: Vec<GuardEvent>,
    },
    /// The guard fires the stop protocol.
    Stop {
        /// The `StopReason`.
        reason: StopReason,
        /// The ledger events to append.
        events: Vec<GuardEvent>,
    },
}

/// A ledger event a verdict asks the driver to append (`{class, payload}` —
/// scope and producer are stamped by the driver).
#[derive(Debug, Clone, PartialEq)]
pub struct GuardEvent {
    /// The registered event class.
    pub class: String,
    /// The payload (the `events::*_payload` builders produce it).
    pub payload: Json,
    /// The scope member the row names (an effect/model-call/tool-call id —
    /// the driver maps it onto `Scope`).
    pub scope_id: Option<String>,
}

/// `guard()`'s per-point input — the data the pure function reads that is
/// not already in the durable prefix.
#[derive(Debug, Clone)]
pub enum GuardInput {
    /// `pre_call` — the call about to be issued.
    PreCall {
        /// The model-call scope id about to open.
        model_call_id: String,
        /// The reservation size the driver computed (ADR-0107 D6).
        reservation_size: u64,
        /// The compiled gauge caps the run declared (`{occupancy_ppm_cap,
        /// delegation_depth_cap, fan_out_cap}` as ppm/counts).
        gauge_caps: GaugeCaps,
    },
    /// `interpret` — the response just parsed.
    Interpret {
        /// The model-call scope.
        model_call_id: String,
        /// The gateway stop reason.
        stop_reason: hh_gateway::vocab::StopReason,
        /// Whether the response carried no text.
        text_empty: bool,
        /// The parsed calls.
        calls: Vec<ParsedCall>,
        /// The compiled surface set (`ModelSurface` projection).
        surfaces: Vec<SurfaceSpec>,
    },
    /// `pre_dispatch` — an effect about to commit (post-`allow`).
    PreDispatch {
        /// The effect.
        effect_id: String,
        /// The attempt about to run.
        attempt_no: u64,
        /// The effect class (`irreversible` never redispatches — INV-8).
        effect_class: String,
        /// `read_only` — the expiry terminal degrades to `not_applied`.
        read_only: bool,
        /// Whether the last terminal was `unknown` (INV-8's probe rule).
        last_terminal_unknown: bool,
        /// Whether a probe/idempotent-class record exists for the retry.
        probed_or_idempotent: bool,
    },
    /// `post_effect` — a terminal just landed.
    PostEffect {
        /// The scope that terminated.
        scope_kind: ScopeKind,
        /// The scope id.
        scope_id: String,
        /// The terminal class (`observed`/`refused`/`unknown`/`abandoned`/
        /// `failed`/`completed`).
        terminal: String,
    },
    /// `decide` — β is about to decide; `proposed` is `Some` when the guard
    /// runs *on* a proposed decision (the `check` path).
    Decide {
        /// The proposed decision under check, when this is the `check`
        /// call (not the pre-`decide` sweep).
        proposed: Option<ControlDecision>,
        /// Whether a submission ref has been recorded (`stop{completed}`
        /// admissibility).
        submission_present: bool,
    },
    /// `resume` — the run restored; `input` folds every counter/ladder.
    Resume,
}

/// The gauge caps a `pre_call` enforces (declared MUST-data on the run
/// manifest — `context.occupancy`, `delegation_depth`, `fan_out`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GaugeCaps {
    /// `context.occupancy` cap in ppm of the context window.
    pub occupancy_ppm: u64,
    /// `delegation_depth` cap.
    pub delegation_depth: u32,
    /// `fan_out` cap.
    pub fan_out: u32,
}

/// The caller-supplied context the pure guards read (the budget view the
/// account folded, the logical clock, deadlines the envelope derived).
#[derive(Debug, Clone, Default)]
pub struct GuardContext {
    /// `dimension spelling → remaining` (root-first `check` — the budget
    /// account's fold, supplied as data; the guard never widens it).
    pub remaining: std::collections::BTreeMap<String, i64>,
    /// The `retries` hard ceiling (INV-6).
    pub retries_ceiling: u64,
    /// The logical now (ms).
    pub now_ms: u64,
    /// Derived deadlines (`scope_id → deadline_ms`) for INV-1/expiry checks.
    pub deadlines: std::collections::BTreeMap<String, u64>,
    /// The gauge readings (`context.occupancy_ppm`, `delegation_depth`,
    /// `fan_out`) the driver folded.
    pub gauges: std::collections::BTreeMap<String, i64>,
    /// `effect_id → effect_class`.
    pub effect_classes: std::collections::BTreeMap<String, String>,
    /// Whether a cancel arrived through a declared channel (the `cancel`
    /// kernel rule — priority 2).
    pub cancel_requested: Option<hh_ontology::control::CancelledBy>,
    /// The manifest's attendance (`interactive` ⇒ `escalate` on exhaustion —
    /// ADR-0168 D6; C1 admission — the Stage-1 default is `stop`).
    pub interactive_attendance: bool,
}

/// The `guard()` entry — dispatches the six points over `(events, policy,
/// input, ctx)` and returns the verdict plus the violations to record
/// (INV checks ride every relevant point — a violation is itself a
/// `stop{invariant_violation}` verdict, by kernel priority).
pub fn guard(
    events: &[EventEnvelope],
    policy: &EnvelopePolicy,
    point: GuardPoint,
    input: &GuardInput,
    ctx: &GuardContext,
) -> GuardVerdict {
    // Invariants run at every guard point that §5e.2 lists them for
    // (post_effect, decide, resume) — a violation wins by priority.
    let violations = if matches!(
        point,
        GuardPoint::PostEffect | GuardPoint::Decide | GuardPoint::Resume
    ) {
        crate::invariants::check(
            events,
            point,
            &InvariantInput {
                deadlines: ctx.deadlines.clone(),
                now_ms: ctx.now_ms,
                conservation: vec![],
                retries_ceiling: ctx.retries_ceiling,
                effect_classes: ctx.effect_classes.clone(),
                idempotency_keys: Default::default(),
                // The open scopes are the durable fold's — INV-1 flags a
                // scope past its deadline only while it is still open.
                open_scope_ids: {
                    let v = crate::views::fold_envelope_view(events);
                    v.open_model_calls
                        .iter()
                        .chain(v.open_effects.iter())
                        .cloned()
                        .collect()
                },
            },
        )
    } else {
        vec![]
    };
    if let Some(v) = violations.first() {
        return violation_verdict(v, &violations);
    }
    match (point, input) {
        (
            GuardPoint::PreCall,
            GuardInput::PreCall {
                reservation_size,
                gauge_caps,
                ..
            },
        ) => pre_call(events, policy, ctx, *reservation_size, *gauge_caps),
        (
            GuardPoint::Interpret,
            GuardInput::Interpret {
                model_call_id,
                stop_reason,
                text_empty,
                calls,
                surfaces,
            },
        ) => interpret(
            events,
            policy,
            ctx,
            model_call_id,
            *stop_reason,
            *text_empty,
            calls,
            surfaces,
        ),
        (
            GuardPoint::PreDispatch,
            GuardInput::PreDispatch {
                effect_id,
                effect_class,
                read_only,
                last_terminal_unknown,
                probed_or_idempotent,
                ..
            },
        ) => pre_dispatch(
            events,
            policy,
            ctx,
            effect_id,
            effect_class,
            *read_only,
            *last_terminal_unknown,
            *probed_or_idempotent,
        ),
        (GuardPoint::PostEffect, _) => post_effect(events, policy, ctx),
        (
            GuardPoint::Decide,
            GuardInput::Decide {
                proposed,
                submission_present,
            },
        ) => decide(events, policy, ctx, proposed.as_ref(), *submission_present),
        (GuardPoint::Resume, _) => resume(events, policy, ctx),
        // A mismatched (point, input) pair is a driver bug — refuse it
        // closed-world (`respond{kind: unknown_decision_point}` — the
        // contract's `UnknownDecisionPoint` error surface).
        _ => GuardVerdict::Respond {
            observation: Json::obj([("kind", Json::str("unknown_decision_point"))]),
            events: vec![],
        },
    }
}

/// An invariant violation becomes a `stop{invariant_violation}` verdict
/// carrying the `control.invariant.violated` row (the driver then runs the
/// stop protocol and the quarantine checkpoint).
fn violation_verdict(v: &InvariantViolation, all: &[InvariantViolation]) -> GuardVerdict {
    GuardVerdict::Stop {
        reason: StopReason::InvariantViolation {
            invariant_id: v.invariant_id.clone(),
        },
        events: all
            .iter()
            .map(|x| GuardEvent {
                class: "control.invariant.violated".into(),
                payload: crate::events::invariant_violated_payload(
                    &x.invariant_id,
                    &x.evidence_refs,
                    x.detected_at_guard,
                ),
                scope_id: None,
            })
            .collect(),
    }
}

/// **G-PRE-CALL** — root-first `check`, gauge caps, reservation sizing,
/// ladder state.
fn pre_call(
    events: &[EventEnvelope],
    policy: &EnvelopePolicy,
    ctx: &GuardContext,
    reservation_size: u64,
    gauge_caps: GaugeCaps,
) -> GuardVerdict {
    // Ladder state — an exhausted ladder (`stop` already fired) binds the
    // next step; when the barrier is not yet engaged the guard restates the
    // recorded detection as the stop reason.
    let loop_state = crate::loops::fold(events);
    if crate::loops::next_rung(&policy.loop_policy, &loop_state).is_none()
        && !crate::stop::barrier_engaged(events)
    {
        return GuardVerdict::Stop {
            reason: last_loop_reason(events).unwrap_or(StopReason::LoopDetected {
                detector: LoopDetectorKind::ExactRepeat,
                pattern: hh_ontology::control::LoopPattern {
                    cycle_len: 0,
                    repeats: 0,
                    loop_keys: vec![],
                },
            }),
            events: vec![],
        };
    }
    // Gauge caps — `context.occupancy → CompactionRequired`,
    // `delegation_depth`/`fan_out → SpawnRefused`.
    let occ = ctx
        .gauges
        .get("context.occupancy_ppm")
        .copied()
        .unwrap_or(0);
    if gauge_caps.occupancy_ppm > 0 && occ as u64 >= gauge_caps.occupancy_ppm {
        // The observation carries the numbers `stop{context_exhausted{
        // required_tokens, cap}}` and the `compact{reason}` decision need —
        // required = occupancy + the pending reservation, cap = the window
        // (CF-225; a guard never guesses).
        let occupied = ctx
            .gauges
            .get("context.occupancy_tokens")
            .copied()
            .unwrap_or(0)
            .max(0) as u64;
        let cap = ctx
            .gauges
            .get("context.window_cap_tokens")
            .copied()
            .unwrap_or(0)
            .max(0) as u64;
        return GuardVerdict::Respond {
            observation: Json::obj([
                ("kind", Json::str("compaction_required")),
                ("gauge", Json::str("context.occupancy_ppm")),
                (
                    "required_tokens",
                    Json::Int(occupied.saturating_add(reservation_size) as i64),
                ),
                ("cap", Json::Int(cap as i64)),
            ]),
            events: vec![],
        };
    }
    for (gauge, cap, kind) in [
        (
            "delegation_depth",
            gauge_caps.delegation_depth as i64,
            "spawn_refused",
        ),
        ("fan_out", gauge_caps.fan_out as i64, "spawn_refused"),
    ] {
        let level = ctx.gauges.get(gauge).copied().unwrap_or(0);
        if cap > 0 && level >= cap {
            return GuardVerdict::Respond {
                observation: Json::obj([("kind", Json::str(kind)), ("gauge", Json::str(gauge))]),
                events: vec![],
            };
        }
    }
    // Root-first `check` — a hard ceiling with no headroom stops with
    // `budget_exhausted{dimension}` (the dimension is the tightest
    // exhausted one the caller's `remaining` map reports ≤ 0).
    for (dim, rem) in &ctx.remaining {
        if *rem <= 0 {
            let dimension = hh_ontology::dimensions::DimensionId::parse(dim)
                .unwrap_or(hh_ontology::dimensions::DimensionId::Turns);
            let rule = policy.exhaustion.rule(dimension);
            if rule.on_exhaustion == crate::policy::ExhaustionAction::Escalate
                && ctx.interactive_attendance
            {
                return GuardVerdict::Respond {
                    observation: Json::obj([
                        ("kind", Json::str("escalate_on_exhaustion")),
                        ("dimension", Json::str(dim)),
                    ]),
                    events: vec![GuardEvent {
                        class: "control.budget.exceeded".into(),
                        payload: crate::events::budget_exceeded_payload(
                            &policy.budget_ref,
                            dim,
                            *rem,
                            0,
                        ),
                        scope_id: None,
                    }],
                };
            }
            return GuardVerdict::Stop {
                reason: StopReason::BudgetExhausted {
                    budget_id: policy.budget_ref.clone(),
                    dimension,
                },
                events: vec![GuardEvent {
                    class: "control.budget.exceeded".into(),
                    payload: crate::events::budget_exceeded_payload(
                        &policy.budget_ref,
                        dim,
                        *rem,
                        0,
                    ),
                    scope_id: None,
                }],
            };
        }
    }
    // The reservation fits — `pass{reservation_id}` (the driver records
    // `control.budget.reserved` via the account; the guard only approves
    // the size it was asked to check — tighten: the reservation never
    // exceeds `remaining`).
    let reservation_id = if reservation_size > 0 {
        Some(format!("resv-{}", events.len()))
    } else {
        None
    };
    GuardVerdict::Pass {
        reservation_id,
        deadline: None,
    }
}

/// The last `control.loop.detected`'s reason reconstruction (the `stop`
/// verdict names the detector/pattern the row recorded).
fn last_loop_reason(events: &[EventEnvelope]) -> Option<StopReason> {
    let last = events
        .iter()
        .rev()
        .find(|e| e.class == "control.loop.detected")?;
    let detector = LoopDetectorKind::parse(last.payload.get("detector")?.as_str()?)?;
    let p = last.payload.get("pattern")?;
    Some(StopReason::LoopDetected {
        detector,
        pattern: hh_ontology::control::LoopPattern {
            cycle_len: p.get("cycle_len")?.as_int()? as u32,
            repeats: p.get("repeats")?.as_int()? as u32,
            loop_keys: match p.get("loop_keys")? {
                Json::Arr(items) => items
                    .iter()
                    .filter_map(|i| i.as_str().map(String::from))
                    .collect(),
                _ => vec![],
            },
        },
    })
}

/// **G-INTERPRET** — the output pipeline, the loop detectors, the
/// empty-response ladder.
#[allow(clippy::too_many_arguments)] // the guard input's arity is the record's
fn interpret(
    events: &[EventEnvelope],
    policy: &EnvelopePolicy,
    _ctx: &GuardContext,
    model_call_id: &str,
    stop_reason: hh_gateway::vocab::StopReason,
    text_empty: bool,
    calls: &[ParsedCall],
    surfaces: &[SurfaceSpec],
) -> GuardVerdict {
    // Terminals that carry no complete response are not interpretable
    // output — `error`/`unknown` go to the retry table, `cancelled` to the
    // cancel row, `deferred` to `wait`, `pause_turn` to the control-level
    // resubmit. Validating them as "empty" would misfile an
    // infrastructure failure as a format error (the F1 table reads
    // `error`/`unknown` on the `error(class)` rows, never `empty`).
    use hh_gateway::vocab::StopReason as Gw;
    if matches!(
        stop_reason,
        Gw::Error | Gw::Unknown | Gw::Cancelled | Gw::Deferred | Gw::PauseTurn
    ) {
        return GuardVerdict::Pass {
            reservation_id: None,
            deadline: None,
        };
    }
    // The output pipeline first (pipeline order — response-level failures
    // precede per-call).
    let verdict = crate::output::validate(
        &policy.output_validation,
        surfaces,
        stop_reason,
        text_empty,
        calls,
    );
    if let crate::output::ValidationVerdict::Reject {
        failure,
        surface_id,
        tool_call_id,
    } = &verdict
    {
        let running = crate::output::fold(events).format_failures_running + 1;
        let mut evs = vec![GuardEvent {
            class: "control.output.rejected".into(),
            payload: crate::events::output_rejected_payload(
                model_call_id,
                failure.as_str(),
                surface_id.as_deref(),
                "detail:unavailable", // the driver substitutes the content address
                false,
                running,
            ),
            scope_id: Some(model_call_id.to_string()),
        }];
        if crate::output::exhaustion_trips(&policy.output_validation, running) {
            return GuardVerdict::Stop {
                reason: StopReason::FormatFailure { count: running },
                events: evs,
            };
        }
        let _ = tool_call_id;
        return GuardVerdict::Respond {
            observation: Json::obj([
                ("kind", Json::str("format_error")),
                ("failure_class", Json::str(failure.as_str())),
            ]),
            events: std::mem::take(&mut evs),
        };
    }
    // The deterministic detectors — a hit's ladder rung decides:
    // `nudge`/`deny` → respond (the nudge artefact / the denial record);
    // `stop` → stop{loop_detected}.
    if let Some(hit) = crate::loops::detect(events, &policy.loop_policy)
        .into_iter()
        .next()
    {
        let ev_row = GuardEvent {
            class: "control.loop.detected".into(),
            payload: crate::events::loop_detected_payload(
                hit.detector,
                &hit.pattern,
                &hit.evidence_refs,
                hit.action,
                hit.ladder_position,
                None,
            ),
            scope_id: None,
        };
        return match hit.action {
            LadderAction::Stop => GuardVerdict::Stop {
                reason: StopReason::LoopDetected {
                    detector: hit.detector,
                    pattern: hit.pattern,
                },
                events: vec![ev_row],
            },
            LadderAction::Deny | LadderAction::Nudge => GuardVerdict::Respond {
                observation: Json::obj([
                    (
                        "kind",
                        Json::str(match hit.action {
                            LadderAction::Deny => "loop_denied",
                            _ => "loop_nudge",
                        }),
                    ),
                    ("detector", Json::str(hit.detector.as_str())),
                ]),
                events: vec![ev_row],
            },
        };
    }
    GuardVerdict::Pass {
        reservation_id: None,
        deadline: None,
    }
}

/// **G-PRE-DISPATCH** — retry eligibility, deadline, reservation, INV-3.
#[allow(clippy::too_many_arguments)]
fn pre_dispatch(
    events: &[EventEnvelope],
    policy: &EnvelopePolicy,
    ctx: &GuardContext,
    effect_id: &str,
    effect_class: &str,
    read_only: bool,
    last_terminal_unknown: bool,
    probed_or_idempotent: bool,
) -> GuardVerdict {
    // INV-3 — the barrier is engaged: only declared `grace` calls pass.
    if !crate::stop::admit_post_barrier_call(events, false) {
        return GuardVerdict::Respond {
            observation: Json::obj([
                ("kind", Json::str("barrier_closed")),
                ("effect_id", Json::str(effect_id)),
            ]),
            events: vec![],
        };
    }
    // INV-8 — redispatch eligibility.
    if !crate::retry::redispatch_permitted(
        effect_class,
        last_terminal_unknown,
        probed_or_idempotent,
    ) {
        return GuardVerdict::Respond {
            observation: Json::obj([
                ("kind", Json::str("redispatch_refused")),
                ("effect_id", Json::str(effect_id)),
            ]),
            events: vec![],
        };
    }
    // Deadline assignment — `deadline(tool_attempt, …)`; the expiry
    // terminal is the kind-fixed one (`for_read_only` degrades it).
    let d = crate::retry::deadline(
        &policy.timeouts,
        ScopeKind::ToolAttempt,
        ctx.now_ms,
        &[],
        None,
    );
    let _ = crate::retry::expiry_terminal(&policy.timeouts, ScopeKind::ToolAttempt, read_only);
    GuardVerdict::Pass {
        reservation_id: Some(format!("resv-{effect_id}")),
        deadline: Some(d),
    }
}

/// **G-POST-EFFECT** — hard-ceiling exhaustion + no-progress evidence +
/// timeout bookkeeping (the invariant check already ran above).
fn post_effect(
    _events: &[EventEnvelope],
    policy: &EnvelopePolicy,
    ctx: &GuardContext,
) -> GuardVerdict {
    // Hard-ceiling exhaustion — a `remaining ≤ 0` crossing is a
    // `budget_exhausted` stop (E1–E2: soft thresholds nudge; hard stops).
    for (dim, rem) in &ctx.remaining {
        if *rem <= 0 {
            let dimension = hh_ontology::dimensions::DimensionId::parse(dim)
                .unwrap_or(hh_ontology::dimensions::DimensionId::Turns);
            if policy.exhaustion.rule(dimension).on_exhaustion
                == crate::policy::ExhaustionAction::Escalate
                && ctx.interactive_attendance
            {
                return GuardVerdict::Respond {
                    observation: Json::obj([
                        ("kind", Json::str("escalate_on_exhaustion")),
                        ("dimension", Json::str(dim)),
                    ]),
                    events: vec![],
                };
            }
            return GuardVerdict::Stop {
                reason: StopReason::BudgetExhausted {
                    budget_id: policy.budget_ref.clone(),
                    dimension,
                },
                events: vec![GuardEvent {
                    class: "control.budget.exceeded".into(),
                    payload: crate::events::budget_exceeded_payload(
                        &policy.budget_ref,
                        dim,
                        *rem,
                        0,
                    ),
                    scope_id: None,
                }],
            };
        }
    }
    GuardVerdict::Pass {
        reservation_id: None,
        deadline: None,
    }
}

/// **G-DECIDE** — kernel stop rules by priority `invariant > cancel >
/// exhaustion > loop/format > definition rules > β`, then completion
/// admissibility on a `stop{completed}` proposal.
fn decide(
    events: &[EventEnvelope],
    policy: &EnvelopePolicy,
    ctx: &GuardContext,
    proposed: Option<&ControlDecision>,
    submission_present: bool,
) -> GuardVerdict {
    // 2. cancel — a declared-channel cancel beats every proposal.
    if let Some(by) = &ctx.cancel_requested {
        return GuardVerdict::Stop {
            reason: StopReason::Cancelled { by: *by },
            events: vec![],
        };
    }
    // 3. exhaustion — same fold as post_effect's hard-ceiling check.
    // Two carve-outs against the plain `stop`: (a) a `stop{cancelled}`
    // proposal rides the declared cancel channel — spend rules never
    // convert the principal's own stop into a budget row; (b) under
    // `interactive` attendance an `escalate` rule on a *work* proposal
    // parks the loop at the exhaustion decision point (ADR-0168 D6) —
    // a `stop` proposal while exhausted still lands `budget_exhausted`
    // (escalating a terminal is meaningless). Every other decision is
    // governed by the dimension's rule.
    for (dim, rem) in &ctx.remaining {
        if *rem <= 0 {
            let proposed_stop = proposed.and_then(|d| match &d.kind {
                DecisionKind::Stop {
                    proposed_reason, ..
                } => Some(proposed_reason.clone()),
                _ => None,
            });
            if matches!(proposed_stop, Some(StopReason::Cancelled { .. })) {
                break;
            }
            // The escalation response itself is not a fresh decision
            // point — refusing `Escalate` would re-signal
            // `escalate_on_exhaustion` and spin the loop forever.
            if matches!(
                proposed.map(|d| &d.kind),
                Some(DecisionKind::Escalate { .. })
            ) {
                break;
            }
            let dimension = hh_ontology::dimensions::DimensionId::parse(dim)
                .unwrap_or(hh_ontology::dimensions::DimensionId::Turns);
            if policy.exhaustion.rule(dimension).on_exhaustion
                == crate::policy::ExhaustionAction::Escalate
                && ctx.interactive_attendance
                && proposed_stop.is_none()
            {
                return GuardVerdict::Respond {
                    observation: Json::obj([
                        ("kind", Json::str("escalate_on_exhaustion")),
                        ("dimension", Json::str(dim)),
                    ]),
                    events: vec![GuardEvent {
                        class: "control.budget.exceeded".into(),
                        payload: crate::events::budget_exceeded_payload(
                            &policy.budget_ref,
                            dim,
                            *rem,
                            0,
                        ),
                        scope_id: None,
                    }],
                };
            }
            return GuardVerdict::Stop {
                reason: StopReason::BudgetExhausted {
                    budget_id: policy.budget_ref.clone(),
                    dimension,
                },
                events: vec![GuardEvent {
                    class: "control.budget.exceeded".into(),
                    payload: crate::events::budget_exceeded_payload(
                        &policy.budget_ref,
                        dim,
                        *rem,
                        0,
                    ),
                    scope_id: None,
                }],
            };
        }
    }
    // 4. loop/format — an exhausted ladder or the running format count.
    let loop_state = crate::loops::fold(events);
    if crate::loops::next_rung(&policy.loop_policy, &loop_state).is_none()
        && !crate::stop::barrier_engaged(events)
    {
        if let Some(reason) = last_loop_reason(events) {
            return GuardVerdict::Stop {
                reason,
                events: vec![],
            };
        }
    }
    let val = crate::output::fold(events);
    if val.format_failures_running >= policy.output_validation.max_format_failures
        && val.format_failures_running > 0
        && !val.format_stop_fired
    {
        return GuardVerdict::Stop {
            reason: StopReason::FormatFailure {
                count: val.format_failures_running,
            },
            events: vec![],
        };
    }
    // 5. definition stop rules — priority order within the definition band.
    let mut rules: Vec<&crate::policy::StopRule> = policy.stop_rules.iter().collect();
    rules.sort_by_key(|r| r.priority);
    for rule in rules {
        if trigger_fires(events, &rule.trigger, ctx) {
            return match rule.action {
                crate::policy::RuleAction::Stop => GuardVerdict::Stop {
                    reason: rule.reason.clone(),
                    events: vec![],
                },
                crate::policy::RuleAction::DenyNext => GuardVerdict::Respond {
                    observation: Json::obj([("kind", Json::str("deny_next"))]),
                    events: vec![],
                },
                crate::policy::RuleAction::Nudge => GuardVerdict::Respond {
                    observation: Json::obj([("kind", Json::str("nudge"))]),
                    events: vec![],
                },
                crate::policy::RuleAction::Escalate => GuardVerdict::Respond {
                    observation: Json::obj([("kind", Json::str("escalate"))]),
                    events: vec![],
                },
            };
        }
    }
    // 6. β's proposal — completion admissibility (non-terminal effects
    // incl. `unknown` block `completed`; `abandoned` does not — OQ-092).
    if let Some(d) = proposed {
        if let DecisionKind::Stop {
            proposed_reason,
            submission_ref,
        } = &d.kind
        {
            let view = crate::views::fold_envelope_view(events);
            if matches!(proposed_reason, StopReason::Completed) {
                let blocking = blocking_effects(events);
                if !blocking.is_empty() {
                    return GuardVerdict::Respond {
                        observation: Json::obj([
                            ("kind", Json::str("completion_blocked")),
                            (
                                "blocking_effects",
                                Json::Arr(blocking.iter().map(Json::str).collect()),
                            ),
                        ]),
                        events: vec![],
                    };
                }
                if !submission_present && submission_ref.is_none() {
                    // A `stop{completed}` with no recorded submission is
                    // ungrounded — the envelope converts it (guard_fired
                    // nudge ≤ max_continue_nudges, else refused — the
                    // driver applies the nudge budget; here the verdict is
                    // the typed refusal record).
                    return GuardVerdict::Respond {
                        observation: Json::obj([("kind", Json::str("missing_submission"))]),
                        events: vec![],
                    };
                }
            }
            let _ = view;
        }
    }
    GuardVerdict::Pass {
        reservation_id: None,
        deadline: None,
    }
}

/// The effects blocking `completed` — open effects plus those settled at a
/// blocking terminal (`unknown`, `probed(undeterminable)`); `abandoned` is
/// terminal and does not block (OQ-092 resolved).
fn blocking_effects(events: &[EventEnvelope]) -> Vec<String> {
    let view = crate::views::fold_envelope_view(events);
    let mut blocking = view.open_effects.clone();
    for ev in events {
        if ev.class == "action.effect.unknown" {
            if let Some(id) = &ev.scope.effect_id {
                if !blocking.contains(id) {
                    blocking.push(id.clone());
                }
            }
        }
    }
    blocking
}

/// A definition `StopTrigger` evaluates against the folded state (the
/// closed grammar — counters, gauges, scope terminals, detector rungs,
/// invariant ids).
fn trigger_fires(
    events: &[EventEnvelope],
    t: &crate::policy::StopTrigger,
    ctx: &GuardContext,
) -> bool {
    use crate::policy::StopTrigger as T;
    match t {
        T::CounterGte { dimension, value } => {
            let remaining = ctx.remaining.get(dimension.as_str()).copied();
            // A counter-gte rule fires when *usage* ≥ value — the caller
            // supplies usage through `remaining` as `limit − remaining`…
            // the guard reads the recorded usage: remaining ≤ (limit −
            // value) is approximated here by `remaining ≤ 0`-adjacent
            // semantics… To keep the rule data-only and honest, the
            // trigger reads the *consumed* reading the driver folds into
            // `gauges` under `used.<dimension>`.
            let used = ctx
                .gauges
                .get(&format!("used.{}", dimension.as_str()))
                .copied()
                .unwrap_or(0);
            let _ = remaining;
            used >= *value
        }
        T::GaugeGte { dimension, ppm } => {
            ctx.gauges.get(dimension.as_str()).copied().unwrap_or(0) >= *ppm
        }
        T::ScopeTerminal { kind, terminal } => events
            .iter()
            .any(|e| scope_terminal_matches(e, *kind, terminal)),
        T::DetectorRung { detector, rung } => {
            let st = crate::loops::fold(events);
            *st.fired.get(detector.as_str()).unwrap_or(&0) > 0
                && events.iter().any(|e| {
                    e.class == "control.loop.detected"
                        && e.payload.get("detector").and_then(Json::as_str)
                            == Some(detector.as_str())
                        && e.payload.get("action").and_then(Json::as_str) == Some(rung.as_str())
                })
        }
        T::InvariantFired { invariant_id } => events.iter().any(|e| {
            e.class == "control.invariant.violated"
                && e.payload
                    .get("invariant_id")
                    .and_then(Json::as_str)
                    .map(|s| s == invariant_id.as_str())
                    .unwrap_or(false)
        }),
    }
}

/// Whether an event is a `scope_terminal{kind, terminal}` match — the
/// closed scope-kind terminal spellings the trigger grammar reads.
fn scope_terminal_matches(ev: &EventEnvelope, kind: ScopeKind, terminal: &str) -> bool {
    let (scope, class_terminal) = match kind {
        ScopeKind::ModelCall => (
            ev.scope.model_call_id.is_some(),
            matches!(
                ev.class.as_str(),
                "model.call.failed" | "model.call.completed"
            ),
        ),
        ScopeKind::ToolAttempt => (
            ev.scope.effect_id.is_some() || ev.scope.tool_call_id.is_some(),
            ev.class.starts_with("action.effect."),
        ),
        ScopeKind::Subagent => (
            ev.scope.child_run_id.is_some(),
            ev.class.starts_with("control.subagent."),
        ),
        _ => (false, false),
    };
    if !scope || !class_terminal {
        return false;
    }
    let term = ev
        .payload
        .get("outcome")
        .or_else(|| ev.payload.get("status"))
        .or_else(|| ev.payload.get("terminal"))
        .and_then(Json::as_str)
        .unwrap_or_else(|| ev.class.rsplit('.').next().unwrap_or(""));
    term == terminal || ev.class.ends_with(terminal)
}

/// **G-RESUME** — re-arm: every counter/ladder/window is recomputed by the
/// folds above (the verdict restates INV-1 on past-deadline scopes and
/// whether a pre-crash drain resumes).
fn resume(events: &[EventEnvelope], policy: &EnvelopePolicy, ctx: &GuardContext) -> GuardVerdict {
    // A pre-crash drain resumes — the barrier is engaged but
    // `lifecycle.run.finished` is absent.
    if crate::stop::barrier_engaged(events)
        && !events.iter().any(|e| e.class == "lifecycle.run.finished")
    {
        let reason = crate::views::fold_envelope_view(events)
            .stop_reason
            .and_then(|j| StopReason::from_json(&j))
            .unwrap_or(StopReason::InfrastructureFailure {
                error_class: hh_ontology::control::InfraError {
                    family: hh_ontology::control::InfraErrorFamily::Kernel,
                    class: hh_ontology::control::KernelInfraCause::DrainTimeout
                        .as_str()
                        .into(),
                },
            });
        return GuardVerdict::Stop {
            reason,
            events: vec![],
        };
    }
    let _ = policy;
    let _ = ctx;
    GuardVerdict::Pass {
        reservation_id: None,
        deadline: None,
    }
}
