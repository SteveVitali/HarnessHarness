//! The deterministic replay driver (R-2.2.4⁰ᵇ; §5a.4; ADR-0135 §3;
//! AC-R-2.2.1-4 / AC-R-2.2.4-7) — `replay{driver_mode: deterministic}`
//! re-drives a declaring control variant over the recorded prefix through
//! the *same* `Driver` loop the live run used: the ports are the seam.
//!
//! * [`ReplayModel`] — `ModelPort` serving the recorded `model.call.*`
//!   outcomes in order (`calls`/`stop_reason`/`response_ref` ride the
//!   `model.call.completed` row — the ADR-0135 §2 recording rule).
//! * [`ReplayGate`] — `EffectGate` serving the recorded effect terminals
//!   *and the recorded `emitted` ceremony rows*; **no executor runs**
//!   (executor call count = 0 by construction — the gate is the recorded
//!   terminal, not a dispatch). The ceremony re-serve is the shared-
//!   allocator contract (L4.6b, ADR-0381): the driver appends the gate's
//!   `emitted` rows through `append_producer`, minting `e-N` ids on the
//!   one monotone `next_id` — a re-drive that skipped the re-mint skews
//!   every later `mc-N`/`d-N`/`e-N` and the model port's call-id check
//!   reports a spurious `replay_diverged`.
//! * [`ReplaySink`] — `LedgerSink` absorbing the re-driven rows in memory;
//!   the source run's prefix is never mutated by the replay.
//! * [`extract_recorded`] — the recorded-input extraction: model rounds,
//!   effect terminals, and the external cue stream (the `human_input`
//!   source — `steer`/`follow_up` disambiguated by the recorded
//!   `control.decision.context_request` ref; `decider = human` permission
//!   decisions become `approval` cues; `lifecycle.run.resumed` becomes the
//!   `resumed` cue).
//!
//! The product is the comparison: the replayed `control.decision` and
//! `context.assembled` payload sequences against the record — full
//! canonical equality (decision ids, cursors, checkpoint refs and
//! `triggered_by` links are all deterministic under a declaring variant).
//! A call or dispatch the record does not contain, or a payload mismatch,
//! is a [`ReplayDivergence`] — `invalid` at the `validity` fold (ADR-0135
//! §3), never a silent reconcile.

use std::collections::BTreeMap;
use std::collections::VecDeque;

use hharness_ledger::event::{Event, EventEnvelope};
use hharness_wire::json::Json;

use crate::driver::{
    AssemblerPort, Driver, DriverConfig, DriverError, EffectGate, GateOutcome, LedgerSink,
    ModelOutcome, ModelPort, RunResult,
};
use crate::output::ParsedCall;
use crate::policy::EnvelopePolicy;
use crate::strategy::{ControlContext, ControlStrategy};
use crate::vocab::{Cue, HumanInput, SettledOutcome};

/// One recorded model round — the `model.call.requested` scope plus the
/// `completed`/`failed` terminal's recorded outcome.
#[derive(Debug, Clone)]
pub struct RecordedModelRound {
    /// The `model_call_id` scope.
    pub model_call_id: String,
    /// The attempt ordinal.
    pub attempt_no: u64,
    /// The outcome the replay serves.
    pub outcome: ModelOutcome,
}

/// The extracted replay inputs — a pure fold of the recorded prefix.
#[derive(Debug, Clone, Default)]
pub struct RecordedInputs {
    /// The recorded model rounds in call order.
    pub model_rounds: VecDeque<RecordedModelRound>,
    /// The recorded dispatch order (`action.effect.intended` seq order).
    pub effect_order: VecDeque<String>,
    /// The recorded terminal per effect id.
    pub effect_terminals: BTreeMap<String, GateOutcome>,
    /// The external cue stream in enqueue order (`human_input`
    /// substituted) — `(mark, cue)` pairs where `mark` is the recorded
    /// durable-prefix length at the cue's arrival row; the re-drive
    /// injects a cue once its replayed prefix reaches the mark (R4.5b —
    /// a pre-drive `human_input` lands behind `RunOpened`, not at the
    /// park boundary).
    pub external_cues: VecDeque<(u64, Cue)>,
    /// The recorded seqs of driver-emitted rows, in order — the replayed
    /// sink's seq lattice (R4.5b): service-side rows interleave the
    /// drive, and the driver's `observe` watermark folds seqs — an
    /// emitted row must land on the recorded seq or `last_cue_seq`
    /// (and every `checkpoint_ref` downstream of it) skews.
    pub driver_seqs: VecDeque<u64>,
    /// The recorded non-driver envelopes in seq order (`hharness-env`/`hharness-ledger`
    /// producers — artefact deliveries, lifecycle/env rows the re-drive
    /// never re-emits). The replayed sink flushes them into the prefix
    /// just ahead of the emitted row that passes their seq, so the
    /// driver's durable folds see the recorded content at the recorded
    /// positions.
    pub external_events: VecDeque<EventEnvelope>,
    /// The recorded `control.decision` payloads `(seq, payload)` in order.
    pub decisions: Vec<(u64, Json)>,
    /// The recorded `context.assembled` payloads `(seq, payload)` in order.
    pub assembled: Vec<(u64, Json)>,
    /// The recorded `assembly_ms` values in `context.assembled` order
    /// (R4.5b): the assembler stamps a *measured wall* into the plan —
    /// `plan_id`/`view_hash` hash it — so a re-measure can never be
    /// byte-identical. The replayed assembler substitutes the recorded
    /// value per row, the same substitution discipline `clock_reads`
    /// applies to `control.clock.read`.
    pub assembly_ms: VecDeque<u64>,
    /// The recorded `control.clock.read` values in order (the declaring
    /// variant's wall-clock reads — served verbatim at replay).
    pub clock_reads: VecDeque<Json>,
    /// The recorded `control.random.read` values in order.
    pub random_reads: VecDeque<Json>,
    /// `tool_call_id → surface_id` from the recorded
    /// `action.tool.proposed` rows — the `act` intents name the call id
    /// only (I2), so a gate needing the surface (the `stop_rule = submit`
    /// re-derivation) resolves it through this map.
    pub proposed_surfaces: BTreeMap<String, String>,
    /// `effect_id → submission_ref` — the `stop_rule = submit` detection
    /// recorded on the terminal `action.effect.*` row (S3.10; the replay
    /// re-serves the *recorded* ref, never a re-minted one).
    pub effect_submissions: BTreeMap<String, String>,
    /// `submission_ref`s the recorded `control.decision{stop}` rows
    /// carried, in seq order — the fallback for records predating the
    /// terminal-row `submission_ref` member.
    pub stop_submissions: VecDeque<String>,
    /// `effect_id → (class, payload)` rows in recorded seq order — the
    /// §5a.2 dispatch ceremony the live gate returned in `GateOutcome.
    /// emitted` and the driver appended through the shared `next_id`
    /// allocator (`security.permission.decided`/`pending`,
    /// `action.effect.{authorized,prepared,committed}`, …). The re-drive
    /// re-serves them verbatim (`ReplayGate` → `GateOutcome.emitted`) so
    /// `append_producer` mints the same `e-N`s the record holds — without
    /// the re-mint the shared allocator skews ~4 ids per dispatch and the
    /// model port's call-id check reports a spurious divergence (L4.6b).
    pub effect_lifecycle: BTreeMap<String, VecDeque<(String, Json)>>,
    /// The recorded producer of the gate's emitted rows — re-stamped on
    /// the re-minted rows so the replayed prefix attributes the Π-floor
    /// records to the boundary that minted them (INV-7), never to the
    /// replay's own name.
    pub gate_component: Option<String>,
}

fn sget<'a>(j: &'a Json, k: &str) -> Option<&'a str> {
    j.get(k).and_then(Json::as_str)
}

fn iget(j: &Json, k: &str) -> Option<u64> {
    j.get(k).and_then(Json::as_int).map(|i| i as u64)
}

/// `extract_recorded(prefix, from_seq, until)` — fold the durable prefix
/// into the replay feed for the window `(from_seq, until]` (the driver's
/// ports consume this; nothing else reads content). `from_seq = 0` is a
/// whole-run replay; a branch replay passes the fork point — the seed
/// prefix at/below it is already executed state, not replay input.
pub fn extract_recorded(
    prefix: &[EventEnvelope],
    from_seq: u64,
    until: Option<u64>,
) -> RecordedInputs {
    let mut out = RecordedInputs::default();
    let in_window =
        |e: &EventEnvelope| e.seq > from_seq && until.map(|u| e.seq <= u).unwrap_or(true);
    // The mark for an external cue anchored at seq `a`: the recorded
    // prefix length once the anchor row is durable — every recorded
    // envelope at `seq <= a` lands in the replayed prefix (seed region +
    // flushed externals + re-emitted driver rows), so the count fixes
    // the enqueue position.
    let mark_for = |a: u64| -> u64 { prefix.iter().filter(|e| e.seq <= a).count() as u64 };
    let prefix: Vec<&EventEnvelope> = prefix.iter().filter(|e| in_window(e)).collect();
    let prefix: &[&EventEnvelope] = &prefix;
    // L4.6b — the gate-emitted rows the re-drive re-mints. A window opens
    // on a *driver*-minted `action.effect.intended` (the re-drive re-emits
    // it; an env-minted `intended` belongs to a dispatch path the replay
    // never re-runs, so its rows stay external — the pre-fix state).
    // Inside the window, a non-driver row naming the effect (its scope, or
    // the payload `effect_id` the §5a.2 chain carries) is a gate-`emitted`
    // ceremony row the driver re-appends (`ReplayGate` re-serves it —
    // `security.permission.decided`/`pending`,
    // `action.effect.{authorized,prepared,committed}`, …). Two rows name
    // the effect yet never re-serve: the `decider: human` permission row
    // (the principal's — an external row and an `Approval` cue) and a
    // *service*-minted terminal (`report_host_effect` settling a `Pending`
    // dispatch — the live gate answered `Pending` and the boundary minted
    // the terminal out of loop; re-minting it in-loop would spend an
    // allocator id the live drive never did, so it stays external and the
    // replayed gate serves `Pending` exactly as live did).
    let mut open: BTreeMap<String, ()> = BTreeMap::new();
    // seq → effect_id for each re-served ceremony row.
    let mut reemit: BTreeMap<u64, String> = BTreeMap::new();
    // Effects whose recorded terminal was service-minted — the replayed
    // gate serves `Pending` (the live answer) and the recorded terminal
    // row folds the effect settled off the external lane.
    let mut pending_terminal: BTreeMap<String, ()> = BTreeMap::new();
    for e in prefix.iter().copied() {
        if e.class == "action.effect.intended" {
            if e.producer.component_variant_ref == "hharness-control" {
                if let Some(ef) = e.scope.effect_id.clone() {
                    open.insert(ef, ());
                }
            }
            continue;
        }
        if open.is_empty() {
            continue;
        }
        let ef = e
            .scope
            .effect_id
            .clone()
            .or_else(|| sget(&e.payload, "effect_id").map(str::to_string));
        let Some(ef) = ef.filter(|ef| open.contains_key(ef)) else {
            continue;
        };
        if matches!(
            e.class.as_str(),
            "action.effect.observed"
                | "action.effect.refused"
                | "action.effect.unknown"
                | "action.effect.abandoned"
        ) {
            open.remove(&ef);
            if e.producer.component_variant_ref != "hharness-control" {
                pending_terminal.insert(ef, ());
            }
            continue;
        }
        if e.producer.component_variant_ref == "hharness-control" {
            continue;
        }
        if e.class == "security.permission.decided" && sget(&e.payload, "decider") == Some("human")
        {
            continue;
        }
        reemit.insert(e.seq, ef);
    }

    // R4.5b — the seq lattice + the non-driver rows: `hharness-control`-produced
    // envelopes are the rows the re-drive re-emits; everything else is an
    // external row the replayed prefix must carry at its recorded seq.
    // L4.6b — the recorded gate-`emitted` rows join the lattice: the
    // re-drive re-appends them (`ReplayGate`'s `emitted` re-serve) at
    // these recorded seqs.
    for e in prefix.iter().copied() {
        if e.producer.component_variant_ref == "hharness-control" || reemit.contains_key(&e.seq) {
            out.driver_seqs.push_back(e.seq);
            if let Some(ef) = reemit.get(&e.seq) {
                if out.gate_component.is_none() {
                    out.gate_component = Some(e.producer.component_variant_ref.clone());
                }
                out.effect_lifecycle
                    .entry(ef.clone())
                    .or_default()
                    .push_back((e.class.clone(), e.payload.clone()));
            }
        } else {
            out.external_events.push_back((*e).clone());
        }
    }

    // ── model rounds ─────────────────────────────────────────────────────
    // `model.call.requested` opens a round; `completed`/`failed` closes it
    // with the outcome the replay serves.
    let mut open_rounds: BTreeMap<String, u64> = BTreeMap::new(); // mc → attempt
    for e in prefix.iter().copied() {
        match e.class.as_str() {
            "model.call.requested" => {
                let mc = sget(&e.payload, "model_call_id")
                    .or(e.scope.model_call_id.as_deref())
                    .unwrap_or("")
                    .to_string();
                let attempt = iget(&e.payload, "attempt_no").unwrap_or(1);
                open_rounds.insert(mc, attempt);
            }
            "model.call.completed" | "model.call.failed" => {
                let mc = sget(&e.payload, "model_call_id")
                    .or(e.scope.model_call_id.as_deref())
                    .unwrap_or("")
                    .to_string();
                let attempt = open_rounds
                    .get(&mc)
                    .copied()
                    .or_else(|| iget(&e.payload, "attempt_no"))
                    .unwrap_or(1);
                let outcome = if e.class == "model.call.completed" {
                    let calls = match e.payload.get("calls") {
                        Some(Json::Arr(cs)) => cs
                            .iter()
                            .map(|c| ParsedCall {
                                tool_call_id: sget(c, "tool_call_id").unwrap_or("").to_string(),
                                surface: sget(c, "surface").unwrap_or("").to_string(),
                                args_raw: sget(c, "args_raw").unwrap_or("").to_string(),
                            })
                            .collect(),
                        _ => Vec::new(),
                    };
                    ModelOutcome {
                        stop_reason: hharness_gateway::vocab::StopReason::parse(
                            sget(&e.payload, "stop_reason").unwrap_or("end_turn"),
                        )
                        .unwrap_or(hharness_gateway::vocab::StopReason::EndTurn),
                        response_ref: sget(&e.payload, "response_ref").unwrap_or("").to_string(),
                        text_empty: matches!(e.payload.get("text_empty"), Some(Json::Bool(true))),
                        calls,
                        error_class: None,
                        retry_after_ms: iget(&e.payload, "retry_after_ms"),
                    }
                } else {
                    ModelOutcome {
                        stop_reason: hharness_gateway::vocab::StopReason::Error,
                        response_ref: String::new(),
                        text_empty: true,
                        calls: vec![],
                        error_class: e
                            .payload
                            .get("error")
                            .and_then(|er| sget(er, "class"))
                            .map(str::to_string),
                        retry_after_ms: iget(&e.payload, "retry_after_ms"),
                    }
                };
                out.model_rounds.push_back(RecordedModelRound {
                    model_call_id: mc,
                    attempt_no: attempt,
                    outcome,
                });
            }
            "action.tool.proposed" => {
                // `tool_call_id → surface_id` — the `act` intents drop the
                // surface (I2); the submit-surface re-derivation resolves
                // it here.
                if let (Some(tc), Some(sf)) = (
                    sget(&e.payload, "tool_call_id").or(e.scope.tool_call_id.as_deref()),
                    sget(&e.payload, "surface_id"),
                ) {
                    out.proposed_surfaces.insert(tc.to_string(), sf.to_string());
                }
            }
            _ => {}
        }
    }

    // ── effect terminals ─────────────────────────────────────────────────
    for e in prefix.iter().copied() {
        if e.class == "action.effect.intended" {
            if let Some(id) = e.scope.effect_id.clone() {
                out.effect_order.push_back(id);
            }
            continue;
        }
        let Some(effect_id) = e.scope.effect_id.clone() else {
            continue;
        };
        if out.effect_terminals.contains_key(&effect_id) {
            continue;
        }
        let outcome = match e.class.as_str() {
            "action.effect.observed" => Some(SettledOutcome::Observed {
                outcome: sget(&e.payload, "outcome").unwrap_or("applied").to_string(),
            }),
            "action.effect.refused" => Some(SettledOutcome::Refused),
            "action.effect.unknown" => Some(SettledOutcome::Unknown {
                cause: sget(&e.payload, "cause").unwrap_or("unknown").to_string(),
            }),
            "action.effect.abandoned" => Some(SettledOutcome::Abandoned),
            _ => None,
        };
        if let Some(outcome) = outcome {
            // The recorded `stop_rule = submit` detection rides the
            // terminal row (S3.10) — re-served verbatim at replay.
            if let Some(sub) = sget(&e.payload, "submission_ref") {
                out.effect_submissions
                    .insert(effect_id.clone(), sub.to_string());
            }
            out.effect_terminals.insert(
                effect_id,
                GateOutcome {
                    outcome,
                    submission_ref: None,
                    error_class: None,

                    emitted: Vec::new(),
                },
            );
        }
    }
    // A service-minted terminal settles out of loop — the recorded gate
    // answer was `Pending` (the live dispatch returned it; the boundary's
    // `report_host_effect` minted the terminal later). The replayed gate
    // re-serves that `Pending` answer and the recorded terminal folds the
    // effect settled off the external lane — a driver re-mint would spend
    // a `next_id` the live drive never allocated (L4.6b).
    for ef in pending_terminal.keys() {
        if let Some(t) = out.effect_terminals.get_mut(ef) {
            t.outcome = SettledOutcome::Pending;
        }
    }

    // ── external cues (the `human_input` source — substituted) ──────────
    // The cue kind is read off the recorded decision it produced: a
    // `context_request{steer_ref|follow_up_ref}` member names the artefact
    // the cue carried (I7 — refs only, never bytes).
    let decisions: Vec<&EventEnvelope> = prefix
        .iter()
        .copied()
        .filter(|e| e.class == "control.decision")
        .collect();
    let artefact_cue = |artefact_id: &str, after_seq: u64| -> Cue {
        for d in decisions.iter().filter(|d| d.seq > after_seq) {
            if let Some(cr) = d.payload.get("context_request") {
                if sget(cr, "steer_ref") == Some(artefact_id) {
                    return Cue::HumanInput(HumanInput::Steer {
                        payload_ref: artefact_id.to_string(),
                    });
                }
                if sget(cr, "follow_up_ref") == Some(artefact_id) {
                    return Cue::HumanInput(HumanInput::FollowUp {
                        payload_ref: artefact_id.to_string(),
                    });
                }
            }
        }
        Cue::HumanInput(HumanInput::FollowUp {
            payload_ref: artefact_id.to_string(),
        })
    };
    for e in prefix.iter().copied() {
        match e.class.as_str() {
            "context.artefact.delivered" => {
                // Only principal-delivered inputs are external cues — the
                // `submit.input` path mints `kind:"instruction"` /
                // `kind:"capability"` deliveries (§5c.1). Internal
                // deliveries (`harness_rule` nudges, `procedure_index`,
                // held-out artefacts, …) are reproduced by the replayed
                // driver's own machinery, never re-fed.
                let kind = sget(&e.payload, "kind").unwrap_or("");
                if matches!(kind, "instruction" | "capability") {
                    if let Some(id) = sget(&e.payload, "artefact_id") {
                        out.external_cues
                            .push_back((mark_for(e.seq), artefact_cue(id, e.seq)));
                    }
                }
            }
            "security.permission.decided" => {
                if sget(&e.payload, "decider") == Some("human") {
                    let effect_id = e
                        .scope
                        .effect_id
                        .clone()
                        .or_else(|| sget(&e.payload, "effect_id").map(str::to_string))
                        .unwrap_or_default();
                    // Only an *effect-tied* approval ever entered the live
                    // inbox — the boundary submits `HumanInput::Approval`
                    // solely when the resolved pending carried an
                    // `effect_id` (the same condition the durable row
                    // records it under). A human `decided` naming no
                    // effect is a boundary-level grant (the capability
                    // ask `permission_decision` settles out of loop —
                    // L4.6a's open-time `--capability` approval) — the
                    // row still lands verbatim as an external, but
                    // re-feeding it as a cue mints a spurious `propose`
                    // the live drive never decided and skews the shared
                    // allocator behind every later `mc-N`/`d-N`.
                    if !effect_id.is_empty() {
                        out.external_cues.push_back((
                            mark_for(e.seq),
                            Cue::HumanInput(HumanInput::Approval {
                                effect_id,
                                allow: sget(&e.payload, "decision") == Some("allow"),
                            }),
                        ));
                    }
                }
            }
            "lifecycle.run.resumed" => {
                out.external_cues.push_back((
                    mark_for(e.seq),
                    Cue::Resumed {
                        last_durable: iget(&e.payload, "from_seq").unwrap_or(0),
                        recovery_decision: e
                            .payload
                            .get("recovery_decision")
                            .cloned()
                            .unwrap_or_else(|| e.payload.clone()),
                    },
                ));
            }
            "control.budget.amended" => {
                // The `amend` op's follow-up cue carries a session-alloc'd
                // ref that is not itself durable — the recorded decision's
                // `follow_up_ref` is the honest reconstruction (the record
                // is the cue's content).
                let mut payload_ref = e.event_id.clone();
                for d in decisions.iter().filter(|d| d.seq > e.seq) {
                    if let Some(r) = d
                        .payload
                        .get("context_request")
                        .and_then(|cr| sget(cr, "follow_up_ref"))
                    {
                        payload_ref = r.to_string();
                        break;
                    }
                }
                out.external_cues.push_back((
                    mark_for(e.seq),
                    Cue::HumanInput(HumanInput::FollowUp { payload_ref }),
                ));
            }
            "control.decision" => {
                // The recorded `stop`'s `submission_ref` — the fallback
                // source for records whose terminal rows predate the
                // `submission_ref` member.
                if sget(&e.payload, "kind") == Some("stop") {
                    if let Some(sub) = sget(&e.payload, "submission_ref") {
                        out.stop_submissions.push_back(sub.to_string());
                    }
                }
                // A principal-cancelled stop implies an `interrupt` cue
                // the record does not otherwise mark — synthesize it at
                // the decision's position (a mistimed interrupt diverges
                // honestly rather than fabricating determinism).
                if sget(&e.payload, "kind") == Some("stop") {
                    let cancelled = e
                        .payload
                        .get("reason")
                        .and_then(|r| r.get("cancelled"))
                        .and_then(|c| sget(c, "by"))
                        == Some("principal");
                    if cancelled {
                        out.external_cues
                            .push_back((mark_for(e.seq), Cue::HumanInput(HumanInput::Interrupt)));
                    }
                }
            }
            _ => {}
        }
    }

    // ── the comparison sequences ─────────────────────────────────────────
    out.decisions = decisions
        .iter()
        .map(|d| (d.seq, d.payload.clone()))
        .collect();
    out.assembled = prefix
        .iter()
        .filter(|e| e.class == "context.assembled")
        .map(|e| (e.seq, e.payload.clone()))
        .collect();
    out.assembly_ms = out
        .assembled
        .iter()
        .filter_map(|(_, p)| {
            p.get("assembly_ms")
                .and_then(|m| m.get("value"))
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64)
        })
        .collect();
    out.clock_reads = prefix
        .iter()
        .filter(|e| e.class == "control.clock.read")
        .map(|e| e.payload.clone())
        .collect();
    out.random_reads = prefix
        .iter()
        .filter(|e| e.class == "control.random.read")
        .map(|e| e.payload.clone())
        .collect();
    out
}

/// The first divergence the replay met — `(at_seq, expected, got)` in
/// `ReplayDiverged` spelling (§5a.4).
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayDivergence {
    /// The recorded seq the divergence anchors at (0 = no recorded anchor).
    pub at_seq: u64,
    /// What the record carries there.
    pub expected: String,
    /// What the replay produced.
    pub got: String,
}

/// `ModelPort` over the recorded `model.call.*` stream — serves rounds in
/// order; a call the record does not contain (wrong scope, or the stream
/// exhausted) records a divergence and serves an `error` outcome the
/// retry machinery settles honestly (the decision comparison reports the
/// divergence — the port never fabricates a match).
pub struct ReplayModel {
    rounds: VecDeque<RecordedModelRound>,
    /// The first divergence observed.
    pub divergence: Option<ReplayDivergence>,
    /// Rounds served.
    pub served: u64,
}

impl ReplayModel {
    /// A port over `recorded.model_rounds`.
    pub fn new(rounds: VecDeque<RecordedModelRound>) -> Self {
        ReplayModel {
            rounds,
            divergence: None,
            served: 0,
        }
    }
}

impl ModelPort for ReplayModel {
    fn call(&mut self, model_call_id: &str, _request: &Json) -> ModelOutcome {
        self.served += 1;
        match self.rounds.pop_front() {
            Some(r) if r.model_call_id == model_call_id => r.outcome,
            Some(r) => {
                let d = self.divergence.get_or_insert(ReplayDivergence {
                    at_seq: 0,
                    expected: format!("model_call {}", r.model_call_id),
                    got: format!("model_call {model_call_id}"),
                });
                let _ = d;
                // Serve the recorded round anyway — the decision-sequence
                // comparison carries the divergence forward.
                r.outcome
            }
            None => {
                self.divergence.get_or_insert(ReplayDivergence {
                    at_seq: 0,
                    expected: "<record exhausted>".to_string(),
                    got: format!("model_call {model_call_id}"),
                });
                ModelOutcome {
                    stop_reason: hharness_gateway::vocab::StopReason::Error,
                    response_ref: "replay-exhausted".into(),
                    text_empty: true,
                    calls: vec![],
                    error_class: Some("replay_diverged".into()),
                    retry_after_ms: None,
                }
            }
        }
    }
}

/// `EffectGate` over the recorded effect terminals — the recorded
/// `observed`/`refused`/`unknown`/`abandoned` is served verbatim; **no
/// executor ever runs** (the external-gateway rule: a deterministic
/// replay dispatches no effect). A dispatch the record does not contain
/// is a divergence; `unknown{replay_diverged}` settles the attempt.
pub struct ReplayGate {
    order: VecDeque<String>,
    terminals: BTreeMap<String, GateOutcome>,
    /// `tool_call_id → surface_id` from the recorded `action.tool.proposed`
    /// rows — the `act` intents name the call id only (I2).
    surfaces: BTreeMap<String, String>,
    /// The submit-completion surface (`hh.submit` at the embed boundary) —
    /// the `stop_rule = submit` marker re-derives deterministically from
    /// the intent's surface, the same rule the live gate applied.
    submit_surface: Option<String>,
    /// `effect_id → submission_ref` recorded on the terminal rows.
    recorded_submissions: BTreeMap<String, String>,
    /// `submission_ref`s of the recorded `stop` decisions, in order —
    /// popped as submit-surface dispatches re-serve them.
    stop_submissions: VecDeque<String>,
    /// `effect_id → recorded emitted rows` — the §5a.2 dispatch ceremony
    /// re-served verbatim so the driver's `append_producer` re-mints the
    /// recorded `e-N` ids and the shared allocator stays in step (L4.6b —
    /// a skipped re-mint skews every later `mc-N`/`d-N` and the port's
    /// call-id check reports a spurious divergence).
    lifecycle: BTreeMap<String, VecDeque<(String, Json)>>,
    /// The recorded producer of the emitted rows — `producer_component`
    /// serves it so the re-minted rows carry the boundary's own ref.
    component: String,
    /// The first divergence observed.
    pub divergence: Option<ReplayDivergence>,
    /// `dispatch` calls served (recorded-terminal serves — executor calls
    /// are `0` by construction; this counter is the audit).
    pub dispatches: u64,
}

impl ReplayGate {
    /// A gate over `recorded.effect_*`.
    pub fn new(recorded: &RecordedInputs, submit_surface: Option<&str>) -> Self {
        ReplayGate {
            order: recorded.effect_order.clone(),
            terminals: recorded.effect_terminals.clone(),
            surfaces: recorded.proposed_surfaces.clone(),
            submit_surface: submit_surface.map(str::to_string),
            recorded_submissions: recorded.effect_submissions.clone(),
            stop_submissions: recorded.stop_submissions.clone(),
            lifecycle: recorded.effect_lifecycle.clone(),
            component: recorded
                .gate_component
                .clone()
                .unwrap_or_else(|| "effect-gate".to_string()),
            divergence: None,
            dispatches: 0,
        }
    }
}

impl EffectGate for ReplayGate {
    /// The re-minted ceremony rows carry the recorded producer — the
    /// boundary that minted them live (`component`), never the replay's
    /// own name (INV-7).
    fn producer_component(&self) -> &str {
        &self.component
    }

    fn dispatch(&mut self, effect_id: &str, _attempt_no: u64, intent: &Json) -> GateOutcome {
        self.dispatches += 1;
        let surface = intent
            .get("surface_id")
            .or_else(|| intent.get("surface"))
            .and_then(Json::as_str)
            .map(str::to_string)
            .or_else(|| {
                intent
                    .get("tool_call_id")
                    .and_then(Json::as_str)
                    .and_then(|tc| self.surfaces.get(tc))
                    .cloned()
            })
            .unwrap_or_default();
        // The `stop_rule = submit` detection re-serves the *recorded*
        // `submission_ref` — the terminal row's own member first, the
        // recorded `stop` decision's ref as the fallback for older
        // records; a synthetic ref is minted only when the record carries
        // neither (it then diverges honestly against the recorded stop).
        let submission_ref = self
            .recorded_submissions
            .get(effect_id)
            .cloned()
            .or_else(|| {
                if self.submit_surface.as_deref() == Some(surface.as_str()) {
                    self.stop_submissions
                        .pop_front()
                        .or_else(|| Some(format!("sub-{effect_id}")))
                } else {
                    None
                }
            });
        match self.order.front() {
            Some(e) if e == effect_id => {
                self.order.pop_front();
                let mut out = self
                    .terminals
                    .get(effect_id)
                    .cloned()
                    .unwrap_or(GateOutcome {
                        outcome: SettledOutcome::Unknown {
                            cause: "record_incomplete".to_string(),
                        },
                        submission_ref: None,
                        error_class: None,
                        emitted: Vec::new(),
                    });
                // The recorded ceremony rows re-serve verbatim — the
                // driver re-appends them (`append_producer`) minting the
                // same `e-N`s the record holds (L4.6b).
                out.emitted = self
                    .lifecycle
                    .remove(effect_id)
                    .map(|q| q.into_iter().collect())
                    .unwrap_or_default();
                out.submission_ref = submission_ref;
                out
            }
            _ => {
                let expected = self
                    .order
                    .front()
                    .cloned()
                    .unwrap_or_else(|| "<record exhausted>".to_string());
                self.divergence.get_or_insert(ReplayDivergence {
                    at_seq: 0,
                    expected,
                    got: format!("dispatch {effect_id}"),
                });
                GateOutcome {
                    outcome: SettledOutcome::Unknown {
                        cause: "replay_diverged".to_string(),
                    },
                    submission_ref: None,
                    error_class: Some("replay_diverged".to_string()),
                    emitted: Vec::new(),
                }
            }
        }
    }
}

/// The post-`prepared` effect classes whose `fencing_token` the live
/// `KernelSink` stamps from the lease generation (hharness-embed `runtime.rs`
/// `FENCED_EFFECT_CLASSES` — the replay sink stamps the generation it
/// mints every envelope under: `1`).
const REPLAY_FENCED_CLASSES: [&str; 7] = [
    "action.effect.committed",
    "action.effect.observed",
    "action.effect.unknown",
    "action.effect.probed",
    "action.effect.compensated",
    "action.effect.reverted",
    "action.effect.abandoned",
];

/// `LedgerSink` absorbing the re-driven rows in memory — the replay's
/// product is the prefix it produces (the comparison input); the source
/// run is never appended to.
pub struct ReplaySink {
    /// The replayed envelopes (seq-ordered once `configure` lays the
    /// lattice — externals flush just-in-time ahead of the emitted row
    /// that passes them).
    pub events: Vec<EventEnvelope>,
    seq: u64,
    /// R4.5b — the recorded seqs the emitted rows take, in order. A
    /// live ledger interleaves service-side rows (`hharness-env` deliveries,
    /// `hharness-ledger` lifecycle) with the drive; the driver's `observe`
    /// watermark folds seqs, so an emitted row renumbered into a
    /// compressed replay-local space skews `last_cue_seq` — and every
    /// `checkpoint_ref` downstream — into a spurious divergence.
    lattice: VecDeque<u64>,
    /// The recorded non-driver rows, in seq order — flushed into the
    /// prefix just before the emitted row whose lattice seq passes them.
    pending_external: VecDeque<EventEnvelope>,
    run_id: String,
}

impl ReplaySink {
    /// An empty replay sink for `run_id`.
    pub fn new(run_id: &str) -> Self {
        ReplaySink {
            events: Vec::new(),
            seq: 0,
            lattice: VecDeque::new(),
            pending_external: VecDeque::new(),
            run_id: run_id.to_string(),
        }
    }

    /// Seed the sink with the recorded prefix ≤ the fork point (a branch
    /// replay's already-executed state — the driver cold-folds it via
    /// `replay_from`; the re-driven rows append after it).
    pub fn seed(&mut self, seed: &[EventEnvelope]) {
        self.seq = seed.last().map(|e| e.seq).unwrap_or(0);
        self.events = seed.to_vec();
    }

    /// `configure` — lay the recorded seq lattice + the non-driver rows
    /// (see `RecordedInputs::driver_seqs`/`external_events`). Externals
    /// preceding the first driver row are the pre-drive prefix — they
    /// flush now so `Driver::open`'s envelope arm folds them.
    pub fn configure(&mut self, lattice: VecDeque<u64>, externals: VecDeque<EventEnvelope>) {
        self.lattice = lattice;
        self.pending_external = externals;
        self.flush_external();
    }

    /// Flush pending external rows ahead of the next emitted row —
    /// every external whose recorded seq precedes the lattice front.
    /// Drains fully once the lattice is exhausted (post-drive rows like
    /// `lifecycle.run.finished` land after the last emitted row).
    fn flush_external(&mut self) {
        loop {
            let flush = match (self.pending_external.front(), self.lattice.front()) {
                (Some(x), Some(&s)) => x.seq < s,
                (Some(_), None) => true,
                _ => false,
            };
            if !flush {
                break;
            }
            let x = self.pending_external.pop_front().unwrap();
            self.events.push(x);
        }
    }

    /// Drain any unflushed externals (post-drive service rows) after the
    /// re-drive — the replayed prefix then carries the full recorded
    /// record in seq order.
    pub fn drain_external(&mut self) {
        while let Some(x) = self.pending_external.pop_front() {
            self.events.push(x);
        }
    }
}

impl LedgerSink for ReplaySink {
    fn append(&mut self, events: Vec<Event>) -> Result<(), String> {
        for mut e in events {
            self.flush_external();
            let seq = self.lattice.pop_front().unwrap_or_else(|| {
                // Past the recorded lattice (a diverging re-drive emits
                // rows the record never had) — continue monotone; the
                // sequence comparison flags the overrun honestly.
                self.seq + 1
            });
            self.seq = self.seq.max(seq);
            // The sink owns the kernel-provenance/`fencing_token` stamps
            // the live `KernelSink` applies (hharness-embed `runtime.rs`) — a
            // bare emitted `Event` would otherwise land with
            // `provenance: None` where the record carries the kernel
            // record, skewing every `view_hash` the observation fold
            // hashes (authority/readers are view members).
            if e.provenance.is_none() {
                e.provenance = Some(hharness_provenance::ProvenanceRecord::kernel(
                    "hharness-embed",
                    seq,
                ));
            }
            if REPLAY_FENCED_CLASSES.contains(&e.class.as_str()) {
                if let Json::Obj(m) = &mut e.payload {
                    m.entry("fencing_token".to_string()).or_insert(Json::Int(1));
                }
            }
            self.events.push(EventEnvelope {
                event_id: e.event_id,
                run_id: self.run_id.clone(),
                seq,
                ts: e.ts,
                hlc: e.hlc,
                plane: hharness_ledger::event::EventPlane::of_class(&e.class)
                    .unwrap_or(hharness_ledger::event::EventPlane::Control),
                class: e.class,
                schema_version: 1,
                producer: e.producer,
                participant_class: hharness_ledger::manifest::ParticipantClass::Native,
                observability_level: Default::default(),
                durability: hharness_ledger::classes::Durability::Ledger,
                scope: e.scope,
                lease_generation: 1,
                parent_event_id: e.parent_event_id,
                causes: e.causes,
                refs: e.refs,
                ir_refs: e.ir_refs,
                surface_ids: e.surface_ids,
                provenance: e.provenance,
                payload: e.payload,
                prev_hash: String::new(),
                hash: format!("replay-h{}", self.seq),
            });
        }
        Ok(())
    }
    fn prefix(&self) -> &[EventEnvelope] {
        &self.events
    }
}

/// The replay's product — the reproduction verdict plus the evidence.
#[derive(Debug)]
pub struct ReplayOutcome {
    /// The run result when the re-drive reached a terminal.
    pub run_result: Option<RunResult>,
    /// The driver parked with the recorded input stream exhausted.
    pub parked: bool,
    /// The replayed `control.decision` payloads in order.
    pub replayed_decisions: Vec<Json>,
    /// The replayed `context.assembled` payloads in order.
    pub replayed_assembled: Vec<Json>,
    /// `dispatch` calls the gate served (recorded terminals — the executor
    /// call count is `0` by construction; AC-R-2.2.4-7).
    pub dispatches: u64,
    /// `model.call`s the port served from the record.
    pub model_calls_served: u64,
    /// The recorded cue stream was fully consumed.
    pub cues_exhausted: bool,
    /// The first divergence (decision-sequence mismatch, a call or
    /// dispatch the record does not contain, or a port-reported one).
    pub diverged: Option<ReplayDivergence>,
}

impl ReplayOutcome {
    /// `reproduced` — the AC-R-2.2.1-4 verdict: the `control.decision` and
    /// `context.assembled` sequences were reproduced exactly.
    pub fn reproduced(&self) -> bool {
        self.diverged.is_none()
    }
}

/// Compare two payload sequences member-by-member; the first difference
/// is a divergence naming the differing members.
fn compare_sequences(
    recorded: &[(u64, Json)],
    replayed: &[Json],
    what: &str,
) -> Option<ReplayDivergence> {
    for (i, ((seq, rec), got)) in recorded.iter().zip(replayed.iter()).enumerate() {
        if rec != got {
            let mut members = Vec::new();
            if let (Json::Obj(rm), Json::Obj(gm)) = (rec, got) {
                for (k, v) in rm {
                    if gm.get(k) != Some(v) {
                        members.push(k.clone());
                    }
                }
                for k in gm.keys() {
                    if !rm.contains_key(k) {
                        members.push(format!("+{k}"));
                    }
                }
            }
            return Some(ReplayDivergence {
                at_seq: *seq,
                expected: format!("{what}[{i}] {}", rec.to_canonical_string()),
                got: format!(
                    "{} — differing members: {}",
                    got.to_canonical_string(),
                    members.join(",")
                ),
            });
        }
    }
    if recorded.len() != replayed.len() {
        let (seq, expected) = recorded
            .get(replayed.len())
            .map(|(s, _)| (*s, format!("{what}[] len {}", recorded.len())))
            .unwrap_or((0, format!("{what}[] len {}", recorded.len())));
        return Some(ReplayDivergence {
            at_seq: seq,
            expected,
            got: format!("len {}", replayed.len()),
        });
    }
    None
}

/// `replay{driver_mode: deterministic}` — re-drive `strategy` (the run's
/// declaring variant) over `recorded` through the canonical `Driver` loop.
/// `assembler` is the run's real assembler port (the `context.assembled`
/// comparison is honest only when the assembler recomputes — a caller
/// without one wires a port that records `assembled_payload: None` and
/// the comparison's assembled half is vacuous; `validity` then never
/// claims `deterministic` on its strength).
///
/// External cues are submitted in recorded order whenever the driver
/// parks — the cue stream is the `human_input` substitution (ADR-0135
/// §2), never a re-ask.
/// The port arms a deterministic re-drive binds — only the *pure* legs:
/// the `router` slot's `RoutingPort` (the consult's `d-N` allocations and
/// `model.route.decided` folds must re-run for id parity with the record —
/// the consult is a pure fold over the replayed prefix, never a live
/// dial) and the `response_cache` leg when the run bound one. The model
/// port itself is always `ReplayModel`; a replayed run dials nothing.
#[derive(Default)]
pub struct ReplayPorts {
    /// The bound `router` slot's port — `None` for an unbound/scripted
    /// lane (no consult rows exist in the record to reproduce).
    pub routing: Option<Box<dyn crate::driver::RoutingPort>>,
    /// The declared `response_cache` leg.
    pub cache: Option<Box<dyn crate::driver::ResponseCachePort>>,
}

#[allow(clippy::too_many_arguments)] // the port set's arity is the driver's
pub fn deterministic_replay<S: ControlStrategy>(
    strategy: S,
    ctx: &ControlContext,
    policy: EnvelopePolicy,
    config: DriverConfig,
    seed: &[EventEnvelope],
    recorded: &RecordedInputs,
    assembler: &mut dyn AssemblerPort,
    submit_surface: Option<&str>,
    replay_run_id: &str,
    ports: ReplayPorts,
) -> Result<ReplayOutcome, DriverError> {
    let mut sink = ReplaySink::new(replay_run_id);
    let seed_len = seed.len();
    sink.seed(seed);
    sink.configure(
        recorded.driver_seqs.clone(),
        recorded.external_events.clone(),
    );
    // `open` for a whole-run replay (the `run_opened` cue + `e-0` opener
    // mint fresh, exactly as the live run did); `replay_from` for a branch
    // — the seed prefix is already-executed state (cold-folded), never
    // re-minted.
    let mut driver = if seed.is_empty() {
        Driver::open(strategy, ctx, policy, &mut sink, config)?
    } else {
        Driver::replay_from(strategy, ctx, policy, &mut sink, config)?
    };
    if let Some(r) = ports.routing {
        driver.set_routing_port(r);
    }
    if let Some(c) = ports.cache {
        driver.set_cache_port(c);
    }
    let mut model = ReplayModel::new(recorded.model_rounds.clone());
    let mut gate = ReplayGate::new(recorded, submit_surface);
    // The external cue stream is scheduled onto the driver at its
    // recorded enqueue marks — a cue that arrived mid-drive in the
    // record is re-fed at the same point, not parked-boundary-late.
    driver.schedule_replay_cues(recorded.external_cues.clone());

    let mut run_result = None;
    let mut parked = false;
    loop {
        match driver.run(&mut model, &mut gate, assembler, &mut sink) {
            Ok(r) => {
                run_result = Some(r);
                break;
            }
            // The parked loop is the cue boundary — feed the next
            // recorded external cue (record order — a mark the diverging
            // re-drive never reached still lands honestly); none left ⇒
            // the replay is done.
            Err(DriverError::Port { port: "inbox", .. }) => match driver.pop_replay_cue() {
                Some(c) => driver.submit(c),
                None => {
                    parked = true;
                    break;
                }
            },
            Err(e) => return Err(e),
        }
    }

    sink.drain_external();
    // The comparison is the re-driven suffix — the seed prefix is the
    // branch's recorded state, not replay output.
    let replayed_decisions: Vec<Json> = sink
        .prefix()
        .iter()
        .skip(seed_len)
        .filter(|e| e.class == "control.decision")
        .map(|e| e.payload.clone())
        .collect();
    let replayed_assembled: Vec<Json> = sink
        .prefix()
        .iter()
        .skip(seed_len)
        .filter(|e| e.class == "context.assembled")
        .map(|e| e.payload.clone())
        .collect();

    let mut diverged = model.divergence.clone().or_else(|| gate.divergence.clone());
    if diverged.is_none() {
        diverged = compare_sequences(&recorded.decisions, &replayed_decisions, "control.decision");
    }
    if diverged.is_none() {
        diverged = compare_sequences(
            &recorded.assembled,
            &replayed_assembled,
            "context.assembled",
        );
    }
    if diverged.is_none() && driver.replay_cues_len() > 0 {
        diverged = Some(ReplayDivergence {
            at_seq: 0,
            expected: "recorded cue stream fully consumed".to_string(),
            got: format!("{} cue(s) unconsumed", driver.replay_cues_len()),
        });
    }
    if diverged.is_none() && !model.rounds.is_empty() {
        diverged = Some(ReplayDivergence {
            at_seq: 0,
            expected: "recorded model rounds fully consumed".to_string(),
            got: format!("{} round(s) unconsumed", model.rounds.len()),
        });
    }

    Ok(ReplayOutcome {
        run_result,
        parked,
        replayed_decisions,
        replayed_assembled,
        dispatches: gate.dispatches,
        model_calls_served: model.served,
        cues_exhausted: driver.replay_cues_len() == 0,
        diverged,
    })
}

#[cfg(test)]
mod tests {
    //! AC-R-2.2.4-7's executor-zero rule + the divergence coverage — a live
    //! `Driver` run over a recording sink is the record; the deterministic
    //! driver re-drives it through the *same* loop with `ReplayModel` /
    //! `ReplayGate` ports and the decision/assembled sequences must match
    //! member-for-member (AC-R-2.2.1-4).

    use super::*;
    use crate::driver::{
        AssembleInputs, AssembledRequest, AssemblerPort, DriverConfig, GateOutcome, ModelOutcome,
    };
    use crate::output::{ParamKind, ParamSpec};
    use crate::policy::EnvelopePolicy;
    use crate::react::ReactMinimal;
    use crate::strategy::StrategyParams;
    use crate::strategy::{ConcurrentInput, SteerMode};
    use hharness_ledger::event::Event;

    /// The recording sink — the live run's append target (the replay's
    /// record source). Mirrors `driver::tests::MemSink`.
    struct RecSink {
        events: Vec<EventEnvelope>,
        seq: u64,
    }

    impl LedgerSink for RecSink {
        fn append(&mut self, events: Vec<Event>) -> Result<(), String> {
            for e in events {
                self.seq += 1;
                self.events.push(EventEnvelope {
                    event_id: e.event_id,
                    run_id: "run".into(),
                    seq: self.seq,
                    ts: e.ts,
                    hlc: e.hlc,
                    plane: hharness_ledger::event::EventPlane::Control,
                    class: e.class,
                    schema_version: 1,
                    producer: e.producer,
                    participant_class: hharness_ledger::manifest::ParticipantClass::Native,
                    observability_level: Default::default(),
                    durability: hharness_ledger::classes::Durability::Ledger,
                    scope: e.scope,
                    lease_generation: 1,
                    parent_event_id: e.parent_event_id,
                    causes: e.causes,
                    refs: e.refs,
                    ir_refs: e.ir_refs,
                    surface_ids: e.surface_ids,
                    provenance: e.provenance,
                    payload: e.payload,
                    prev_hash: "h".into(),
                    hash: format!("h{}", self.seq),
                });
            }
            Ok(())
        }
        fn prefix(&self) -> &[EventEnvelope] {
            &self.events
        }
    }

    struct ScriptedModel {
        script: VecDeque<ModelOutcome>,
    }
    impl ModelPort for ScriptedModel {
        fn call(&mut self, _id: &str, _req: &Json) -> ModelOutcome {
            self.script.pop_front().unwrap_or(ModelOutcome {
                stop_reason: hharness_gateway::vocab::StopReason::EndTurn,
                response_ref: "r-empty".into(),
                text_empty: true,
                calls: vec![],
                error_class: None,
                retry_after_ms: None,
            })
        }
    }

    /// The L4.6b gate — a live-shaped `EffectGate` whose `emitted` mints
    /// the §5a.2 dispatch ceremony `EmbedGate::dispatch_chain` produces
    /// (`security.permission.decided` → `action.effect.authorized` →
    /// `action.effect.prepared` → `action.effect.committed`), appended by
    /// the driver through the shared `next_id` allocator. A dispatch
    /// followed by further model calls is the window where the capstone's
    /// ~4-id skew diverged (3/3 on the anthropic lane).
    struct CeremonyGate;
    impl EffectGate for CeremonyGate {
        fn producer_component(&self) -> &str {
            "test/effect-gate"
        }
        fn dispatch(&mut self, ef: &str, _a: u64, i: &Json) -> GateOutcome {
            let tc = i.get("tool_call_id").and_then(Json::as_str).unwrap_or("");
            let risk = Json::obj([
                ("reversibility", Json::str("read_only")),
                ("repeat_safety", Json::str("idempotent")),
                ("scope", Json::str("workspace_local")),
            ]);
            GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: (tc == "tc-2").then(|| format!("sub-{ef}")),
                error_class: None,
                emitted: vec![
                    (
                        "security.permission.decided".to_string(),
                        Json::obj([
                            ("effect_id", Json::str(ef)),
                            ("attempt_no", Json::Int(1)),
                            ("decision", Json::str("allow")),
                            ("decider", Json::str("policy")),
                            ("decision_scope", Json::str("run")),
                            ("effective_risk_class", risk.clone()),
                            ("policy_ref", Json::str("test/declared-floor")),
                        ]),
                    ),
                    (
                        "action.effect.authorized".to_string(),
                        Json::obj([("effective_risk_class", risk)]),
                    ),
                    (
                        "action.effect.prepared".to_string(),
                        Json::obj([("idempotency_key", Json::str(format!("key-{ef}")))]),
                    ),
                    (
                        "action.effect.committed".to_string(),
                        Json::obj([("attempt_no", Json::Int(1))]),
                    ),
                ],
            }
        }
    }

    /// The live gate — every dispatch observes `applied`; the `hh.submit`
    /// surface's dispatch carries the submission marker the same spelling
    /// `ReplayGate` re-derives (`sub-{effect_id}`) so the stop decision's
    /// recorded `submission_ref` reproduces exactly.
    struct ScriptedGate;
    impl EffectGate for ScriptedGate {
        fn dispatch(&mut self, ef: &str, _a: u64, i: &Json) -> GateOutcome {
            let tc = i.get("tool_call_id").and_then(Json::as_str).unwrap_or("");
            GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: (tc == "tc-2").then(|| format!("sub-{ef}")),
                error_class: None,
                emitted: Vec::new(),
            }
        }
        fn finish_record(&self) -> Option<Json> {
            None
        }
    }

    struct NullAsm;
    impl AssemblerPort for NullAsm {
        fn assemble(&mut self, _inputs: &AssembleInputs<'_>, _req: &Json) -> AssembledRequest {
            AssembledRequest {
                request: Json::Null,
                assembled_payload: None,
                side_events: Vec::new(),
                pre_events: Vec::new(),
                overflow: None,
            }
        }
    }

    fn ctx() -> ControlContext {
        ControlContext {
            process_ref: "proc-1".into(),
            plan: vec![],
            boundary: crate::react::react_preset(),
            profile: Json::Null,
            account_ref: "acct".into(),
            budget_ref: "b-1".into(),
            envelope_ref: "env-1".into(),
            parameters: StrategyParams::default(),
            capabilities_available: vec![],
            steering: (SteerMode::Unsupported, ConcurrentInput::QueueOnly),
        }
    }

    fn config() -> DriverConfig {
        DriverConfig {
            surfaces: vec![
                crate::output::SurfaceSpec {
                    surface_id: "fs.read".into(),
                    semantic_id: "sem/fs.read".into(),
                    params: [(
                        "path".into(),
                        ParamSpec {
                            required: true,
                            kind: ParamKind::Str,
                            enum_values: vec![],
                            domain: vec![],
                        },
                    )]
                    .into_iter()
                    .collect(),

                    risk_class: None,
                    family: None,
                },
                crate::output::SurfaceSpec {
                    surface_id: "hh.submit".into(),
                    semantic_id: "sem/hh.submit".into(),
                    params: Default::default(),

                    risk_class: None,
                    family: None,
                },
            ],
            ..DriverConfig::default()
        }
    }

    fn m_outcome(sr: hharness_gateway::vocab::StopReason, calls: Vec<ParsedCall>) -> ModelOutcome {
        ModelOutcome {
            stop_reason: sr,
            response_ref: "r-1".into(),
            text_empty: calls.is_empty(),
            calls,
            error_class: None,
            retry_after_ms: None,
        }
    }

    /// The live run the replay re-drives: an `fs.read` tool call, then a
    /// `hh.submit` call — the full dispatch→observe→submit→decision loop.
    fn live_run() -> Vec<EventEnvelope> {
        let mut sink = RecSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver =
            Driver::open(ReactMinimal::new(), &ctx(), policy, &mut sink, config()).unwrap();
        let mut model = ScriptedModel {
            script: [
                m_outcome(
                    hharness_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-1".into(),
                        surface: "fs.read".into(),
                        args_raw: r#"{"path":"/a"}"#.into(),
                    }],
                ),
                m_outcome(
                    hharness_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-2".into(),
                        surface: "hh.submit".into(),
                        args_raw: "{}".into(),
                    }],
                ),
            ]
            .into_iter()
            .collect(),
        };
        let mut gate = ScriptedGate;
        let mut asm = NullAsm;
        driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        sink.events
    }

    /// The L4.6b fixture — `live_run`'s shape through `CeremonyGate`: an
    /// `fs.read` dispatch carrying the 4-row §5a.2 ceremony, a second
    /// model call (the skew window), then the `hh.submit` dispatch with
    /// its own ceremony plus the completion/verification chain.
    fn ceremony_run() -> Vec<EventEnvelope> {
        let mut sink = RecSink {
            events: vec![],
            seq: 0,
        };
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut driver =
            Driver::open(ReactMinimal::new(), &ctx(), policy, &mut sink, config()).unwrap();
        let mut model = ScriptedModel {
            script: [
                m_outcome(
                    hharness_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-1".into(),
                        surface: "fs.read".into(),
                        args_raw: r#"{"path":"/a"}"#.into(),
                    }],
                ),
                m_outcome(
                    hharness_gateway::vocab::StopReason::ToolUse,
                    vec![ParsedCall {
                        tool_call_id: "tc-2".into(),
                        surface: "hh.submit".into(),
                        args_raw: "{}".into(),
                    }],
                ),
            ]
            .into_iter()
            .collect(),
        };
        let mut gate = CeremonyGate;
        let mut asm = NullAsm;
        driver
            .run(&mut model, &mut gate, &mut asm, &mut sink)
            .unwrap();
        sink.events
    }

    /// AC-R-2.2.1-4 + AC-R-2.2.4-7 — a declaring variant re-driven over the
    /// recorded inputs reproduces the `control.decision`/`context.assembled`
    /// sequences exactly, and every dispatch is served from the record
    /// (executor calls = 0 by construction — `ReplayGate` has no executor).
    #[test]
    fn deterministic_replay_reproduces_the_record() {
        let events = live_run();
        let recorded = extract_recorded(&events, 0, None);
        let recorded_dispatches = events
            .iter()
            .filter(|e| e.class == "action.effect.intended")
            .count() as u64;
        assert_eq!(recorded_dispatches, 2, "live run must dispatch both calls");
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut asm = NullAsm;
        let out = deterministic_replay(
            ReactMinimal::new(),
            &ctx(),
            policy,
            config(),
            &[],
            &recorded,
            &mut asm,
            Some("hh.submit"),
            "replay:run",
            ReplayPorts::default(),
        )
        .unwrap();
        assert!(out.reproduced(), "diverged: {:?}", out.diverged);
        assert_eq!(out.dispatches, recorded_dispatches);
        assert!(out.cues_exhausted);
        assert!(out.run_result.is_some() || out.parked);
    }

    /// A record the replay cannot reproduce — a tampered decision payload —
    /// surfaces `ReplayDivergence`, never a silent reconcile (§5a.4).
    #[test]
    fn tampered_record_diverges() {
        let events = live_run();
        let mut recorded = extract_recorded(&events, 0, None);
        // Tamper with the recorded decision sequence — the replay's own
        // decisions are unchanged, so the comparison must name the member.
        let Json::Obj(m) = &mut recorded.decisions[0].1 else {
            panic!("decision payload must be an object")
        };
        m.insert("tampered".to_string(), Json::Bool(true));
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut asm = NullAsm;
        let out = deterministic_replay(
            ReactMinimal::new(),
            &ctx(),
            policy,
            config(),
            &[],
            &recorded,
            &mut asm,
            Some("hh.submit"),
            "replay:run",
            ReplayPorts::default(),
        )
        .unwrap();
        let d = out.diverged.expect("tampered record must diverge");
        assert!(d.got.contains("tampered"), "{d:?}");
    }

    /// L4.6b — the capstone's defect: an effect-dispatching run whose
    /// ledger carries the §5a.2 ceremony (`security.permission.decided`,
    /// `action.effect.{authorized,prepared,committed}` — the gate's
    /// `emitted` rows) *followed by further model calls*. The recorded
    /// rows re-serve verbatim through `GateOutcome.emitted` so the shared
    /// `next_id` allocator mints the same `e-N`s — the next `mc-N`
    /// matches the record and the replay reproduces byte-identically.
    /// Removing the re-serve re-opens the ~4-id skew and this test fails.
    #[test]
    fn ceremony_dispatch_run_replays_byte_identical() {
        let events = ceremony_run();
        // The fixture must carry the ceremony — the 13-row dispatch
        // chain the capstone surfaced (proposed + intended + decided +
        // authorized + prepared + committed + observed, then the second
        // call's chain + the completion/verification rows).
        for class in [
            "security.permission.decided",
            "action.effect.authorized",
            "action.effect.prepared",
            "action.effect.committed",
        ] {
            assert!(
                events.iter().filter(|e| e.class == class).count() >= 2,
                "fixture carries {class} per dispatch: {:?}",
                events.iter().map(|e| e.class.as_str()).collect::<Vec<_>>()
            );
        }
        let recorded = extract_recorded(&events, 0, None);
        // The recorded ceremony is the re-serve feed — one emitted set
        // per dispatched effect.
        assert_eq!(recorded.effect_lifecycle.len(), 2);
        for rows in recorded.effect_lifecycle.values() {
            assert_eq!(rows.len(), 4, "four ceremony rows per dispatch");
        }
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut asm = NullAsm;
        let out = deterministic_replay(
            ReactMinimal::new(),
            &ctx(),
            policy,
            config(),
            &[],
            &recorded,
            &mut asm,
            Some("hh.submit"),
            "replay:run",
            ReplayPorts::default(),
        )
        .unwrap();
        assert!(out.reproduced(), "diverged: {:?}", out.diverged);
        assert_eq!(out.dispatches, 2);
        assert_eq!(out.model_calls_served, 2);
        assert!(out.cues_exhausted);
        assert!(out.run_result.is_some() || out.parked);
    }

    /// L4.6b — a record the re-drive honestly cannot reproduce still
    /// diverges: dropping a recorded ceremony row leaves the shared
    /// allocator one id short, and the port's call-id check reports the
    /// mismatch — never a silent pass.
    #[test]
    fn dropped_ceremony_row_diverges() {
        let mut events = ceremony_run();
        let pos = events
            .iter()
            .position(|e| e.class == "security.permission.decided")
            .expect("fixture carries the permission row");
        events.remove(pos);
        let recorded = extract_recorded(&events, 0, None);
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut asm = NullAsm;
        let out = deterministic_replay(
            ReactMinimal::new(),
            &ctx(),
            policy,
            config(),
            &[],
            &recorded,
            &mut asm,
            Some("hh.submit"),
            "replay:run",
            ReplayPorts::default(),
        )
        .unwrap();
        assert!(
            out.diverged.is_some(),
            "a mutated record must diverge loudly, never reconcile"
        );
    }

    /// L4.6b — with the ceremony re-serve armed, a tampered decision
    /// payload still diverges (the divergence report stays honest — the
    /// fix is an allocator fix, never a wider tolerance).
    #[test]
    fn ceremony_tampered_decision_diverges() {
        let events = ceremony_run();
        let mut recorded = extract_recorded(&events, 0, None);
        let Json::Obj(m) = &mut recorded.decisions[0].1 else {
            panic!("decision payload must be an object")
        };
        m.insert("tampered".to_string(), Json::Bool(true));
        let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
        let mut asm = NullAsm;
        let out = deterministic_replay(
            ReactMinimal::new(),
            &ctx(),
            policy,
            config(),
            &[],
            &recorded,
            &mut asm,
            Some("hh.submit"),
            "replay:run",
            ReplayPorts::default(),
        )
        .unwrap();
        let d = out.diverged.expect("tampered record must diverge");
        assert!(d.got.contains("tampered"), "{d:?}");
    }

    /// `extract_recorded` — the window `(from_seq, until]` bounds the feed;
    /// the model rounds and effect terminals come out in seq order.
    #[test]
    fn extract_recorded_windows_the_feed() {
        let events = live_run();
        let all = extract_recorded(&events, 0, None);
        assert_eq!(all.model_rounds.len(), 2);
        assert_eq!(all.effect_order.len(), 2);
        assert!(!all.decisions.is_empty());
        // An empty window yields an empty feed.
        let tip = events.last().unwrap().seq;
        let none = extract_recorded(&events, tip, None);
        assert!(none.model_rounds.is_empty() && none.effect_order.is_empty());
        // A prefix window excludes the second round.
        let second_req = events
            .iter()
            .filter(|e| e.class == "model.call.requested")
            .nth(1)
            .unwrap()
            .seq;
        let windowed = extract_recorded(&events, 0, Some(second_req - 1));
        assert_eq!(windowed.model_rounds.len(), 1);
    }

    /// L4.6a — a human `security.permission.decided` naming no `effect_id`
    /// is a boundary-level grant (the open-time capability ask settled
    /// through `permission_decision` — never a driver inbox cue). The row
    /// must land as an external but mint NO `Approval` cue — re-feeding
    /// one mints a spurious `propose` and skews the shared allocator
    /// behind every later `mc-N`/`d-N` (the `expected mc-21, got mc-13`
    /// divergence the ACP leg surfaced).
    #[test]
    fn boundary_level_human_decided_mints_no_approval_cue() {
        let events = ceremony_run();
        let decided = events
            .iter()
            .find(|e| e.class == "security.permission.decided")
            .expect("fixture carries a decided row")
            .clone();
        let mut human = decided;
        if let Json::Obj(m) = &mut human.payload {
            m.insert("decider".to_string(), Json::str("human"));
        }
        // Effect-tied → the one Approval cue the live inbox carried.
        let tied = extract_recorded(&[human.clone()], 0, None);
        assert_eq!(
            tied.external_cues
                .iter()
                .filter(|(_, c)| matches!(c, Cue::HumanInput(HumanInput::Approval { .. })))
                .count(),
            1,
            "an effect-tied human decided mints the approval cue"
        );
        // Boundary-level — no effect scope, no payload effect_id → the
        // durable row lands but no cue re-feeds.
        let mut boundary = human;
        boundary.scope.effect_id = None;
        if let Json::Obj(m) = &mut boundary.payload {
            m.remove("effect_id");
        }
        let untied = extract_recorded(&[boundary], 0, None);
        assert!(
            untied
                .external_cues
                .iter()
                .all(|(_, c)| !matches!(c, Cue::HumanInput(HumanInput::Approval { .. }))),
            "a human decided naming no effect_id mints no approval cue: {:?}",
            untied.external_cues.len()
        );
    }
}
