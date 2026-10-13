//! The kernel-side ports the [`Driver`](hh_control::driver::Driver) drives
//! (§7.4 §2.5 — the embed boundary runs the canonical control loop, not a
//! shadow of it):
//!
//! - [`KernelSink`] — `LedgerSink` over `Store::append` under the
//!   session's fenced writer lease.
//! - [`EmbedModel`] — the Stage-1 deterministic model port: the
//!   production `hh-gateway` boundary lands with it; at Stage 1 the
//!   embed layer's model is scripted by the submitted input (a
//!   `{kind:"invoke"}` input block issues that capability call;
//!   anything else issues the `hh.submit` completion call — the run's
//!   work is the host's submission, honestly recorded).
//! - [`EmbedGate`] — the effect boundary: `hh.submit` dispatches
//!   `observed` carrying the `submission_ref` the driver's
//!   `stop_rule = submit` marker reads; an intent bound to a declared
//!   host-executor capability settles `unknown{awaiting_host}` and
//!   records the ask the service turns into an
//!   `upcall.invoke_host_capability` (the host's `report_host_effect`
//!   mints the terminal — `unknown` is probe-able, INV-2).
//! - [`KernelAssembler`] — the context assembler: runs the real
//!   `hh_context::assemble` builder over the run's durable prefix
//!   (DF-S2.8-1) — the appended `context.assembled` is the builder's
//!   canonical plan record.

use hh_context::compact::{CompactInput, CompactionTrigger, EvictOldest};
use hh_context::events::memory_invalidated_payload;
use hh_context::{
    default_layout, lifecycle_state, AssemblyRequest, Candidate, CandidateKind, CandidateState,
    CollectSink, ContextBudget, ContextPlan, DefaultPolicy, DerivedFrom, Estimate,
    LifecycleStateKind, MemoryContent, MemoryDraft, MemoryKind, MemoryStore, PriorityClass,
    Retention, RetrievalBudget, RetrievalQuery, RetrievalRequest, RevocationReason,
    SlotConstraints, TriggerKind, ValidityPolicy, DETERMINISTIC_DEFAULT,
};
use hh_control::driver::{
    AssembleInputs, AssembledRequest, AssemblerPort, CompactionDone, CompactionImpossible,
    CompactionPort, EffectGate, GateOutcome, LedgerSink, MemoryPort, ModelOutcome, ModelPort,
};
use hh_control::output::ParsedCall;
use hh_control::vocab::SettledOutcome;
use hh_identity::idp::idp_id;
use hh_ledger::event::{Event, EventEnvelope};
use hh_ledger::manifest::EventRef;
use hh_ledger::store::{Lease, Store};
use hh_ledger::views::context_view;
use hh_wire::json::Json;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

/// `LedgerSink` over the run's `Store` under the session's writer lease.
pub struct KernelSink<'a> {
    pub store: &'a mut Store,
    pub run_id: String,
    pub lease: Lease,
    /// R2.4 (DF-S2.9-3) — the declared `snapshot_cadence` producer: the
    /// run's env driver the boundary take routes through. `None` on the
    /// arm/replay paths (`Driver::open`/`resume_from`) — a cadence never
    /// re-takes on replay.
    pub envs: Option<&'a mut hh_env::driver::EnvDriver>,
    /// The session's env handle the cadence targets (R-NOSIDE — the id
    /// never leaves the kernel).
    pub env_handle: Option<String>,
}

impl LedgerSink for KernelSink<'_> {
    fn append(&mut self, mut events: Vec<Event>) -> Result<(), String> {
        // The sink owns the kernel provenance — the driver emits rows
        // either bare (its plain `append`) or carrying an explicit
        // delegate origin (verification claims — kept verbatim, never
        // overwritten). Provenance-mandatory classes refuse a bare row
        // at the append gate, so the sink stamps the kernel record.
        let now = self.store.now_ms();
        for ev in events.iter_mut() {
            if ev.provenance.is_none() {
                ev.provenance = Some(hh_provenance::ProvenanceRecord::kernel("hh-embed", now));
            }
            // Post-`prepared` effect rows carry `fencing_token = lease
            // generation` (ADR-0030 §6 invariant 6). The lease is the
            // sink's own — stamp it here rather than threading the
            // generation into the driver (the driver never owns lease
            // state; a driver-stamped *stale* token still fails `Fenced`,
            // this only fills the absent member).
            if FENCED_EFFECT_CLASSES.contains(&ev.class.as_str())
                && ev.payload.get("fencing_token").is_none()
            {
                if let Json::Obj(m) = &mut ev.payload {
                    m.insert(
                        "fencing_token".to_string(),
                        Json::Int(self.lease.generation as i64),
                    );
                }
            }
        }
        // The cadence trigger classes this append carries (scanned before
        // the append consumes the batch) — `lifecycle.turn.finished` is
        // the turn boundary; a settled `action.effect.*` terminal row is
        // the `every_n_effects` count leg.
        let mut triggers = Vec::new();
        for ev in &events {
            match ev.class.as_str() {
                "lifecycle.turn.finished" => {
                    triggers.push(hh_env::driver::SnapshotTrigger::TurnBoundary)
                }
                "action.effect.observed"
                | "action.effect.refused"
                | "action.effect.unknown"
                | "action.effect.abandoned" => {
                    triggers.push(hh_env::driver::SnapshotTrigger::EffectSettled)
                }
                _ => {}
            }
        }
        let classes: Vec<String> = events.iter().map(|e| e.class.clone()).collect();
        self.store
            .append(&self.run_id, &self.lease, events)
            .map(|_| ())
            .map_err(|e| format!("{e:?} (classes: {classes:?})"))?;
        // DF-S2.9-3 — the declared cadence fires on the durable trigger
        // rows *after* they land (the take's `at_seq` covers them) and
        // *inside* the sink — a `lifecycle.run.finished` append arriving
        // later in the same finish() call cannot seal the store before
        // the snapshot row exists. `cadence_take` records its own
        // `action.environment.failed` row on an attempted take that fails
        // — never silent, never run-fatal.
        if let (Some(envs), Some(env_id)) = (self.envs.as_deref_mut(), self.env_handle.as_deref()) {
            for trigger in triggers {
                envs.cadence_take(self.store, &self.lease, env_id, trigger)
                    .map_err(|e| format!("{e:?}"))?;
            }
        }
        Ok(())
    }
    fn prefix(&self) -> &[EventEnvelope] {
        self.store.events(&self.run_id).unwrap_or(&[])
    }
}

/// The `hh.submit` completion capability's surface id — the one
/// kernel-dispatched surface the scripted model calls.
pub const SUBMIT_SURFACE: &str = "hh.submit";

/// The Stage-1 model port — deterministic and offline. The submitted
/// input decides the call: a `{kind:"invoke", capability, args}` block
/// issues that capability's call (the host-executor round trip);
/// anything else issues `hh.submit` — the run completes when the host
/// says it is done.
#[derive(Debug, Default)]
pub struct EmbedModel {
    /// The capability surface the next call invokes, and its args.
    pub invoke: Option<(String, Json)>,
    /// The completion text the `hh.submit` call carries.
    pub completion: String,
    /// The response ref the next completed call reports.
    pub response_ref: String,
    /// Calls issued (drives `tool_call_id` allocation).
    pub calls_made: u64,
}

impl ModelPort for EmbedModel {
    fn call(&mut self, _model_call_id: &str, _request: &Json) -> ModelOutcome {
        self.calls_made += 1;
        let (surface, args_raw) = match &self.invoke {
            Some((cap, args)) => (cap.clone(), args.to_canonical_string()),
            None => (
                SUBMIT_SURFACE.to_string(),
                Json::obj([("text", Json::str(self.completion.clone()))]).to_canonical_string(),
            ),
        };
        ModelOutcome {
            stop_reason: hh_gateway::vocab::StopReason::ToolUse,
            response_ref: self.response_ref.clone(),
            text_empty: false,
            calls: vec![ParsedCall {
                tool_call_id: format!("tc-{}", self.calls_made),
                surface,
                args_raw,
            }],
            error_class: None,
            retry_after_ms: None,
        }
    }
}

/// The §5a.2 invariant-6 set — the post-`prepared` phase classes whose
/// payloads must carry `fencing_token == lease.generation`
/// (`hh_ledger`'s fold enforces; the sink stamps the lease it owns).
const FENCED_EFFECT_CLASSES: [&str; 7] = [
    "action.effect.committed",
    "action.effect.observed",
    "action.effect.unknown",
    "action.effect.probed",
    "action.effect.compensated",
    "action.effect.reverted",
    "action.effect.abandoned",
];

/// The declared facts a host-executor surface carries into dispatch —
/// the `risk_class` Π's floor reads and the `requires_approval` flag the
/// open-time ask machinery minted `security.permission.pending` rows for.
#[derive(Debug, Clone, Default)]
pub struct HostDecl {
    /// `supplies.host_capabilities[].risk_class` — `None` reads UNKNOWN.
    pub risk_class: Option<hh_ontology::risk::RiskClass>,
    /// `supplies.host_capabilities[].requires_approval`.
    pub requires_approval: bool,
}

/// The §5a.2 write-ahead chain the boundary's dispatch mints:
/// `security.permission.decided{allow}` (the Stage-1 admission verdict —
/// the declared surface was admitted at open; Π's never-auto floor is
/// evaluated here, never assumed) → `action.effect.authorized` →
/// `action.effect.prepared` → `action.effect.committed`, in emit order.
/// The driver appends them before the terminal/outcome (CC3 — the
/// producer computes, the fenced writer lands; the sink stamps
/// `fencing_token` on the fenced classes).
fn dispatch_chain(effect_id: &str, risk: &hh_ontology::risk::RiskClass) -> Vec<(String, Json)> {
    let rc = risk.to_json();
    vec![
        (
            "security.permission.decided".to_string(),
            Json::obj([
                ("effect_id", Json::str(effect_id)),
                ("attempt_no", Json::Int(1)),
                ("decision", Json::str("allow")),
                ("decider", Json::str("policy")),
                ("decision_scope", Json::str("run")),
                ("effective_risk_class", rc.clone()),
                ("policy_ref", Json::str("hh-embed/declared-surface-floor")),
            ]),
        ),
        (
            "action.effect.authorized".to_string(),
            Json::obj([("effective_risk_class", rc)]),
        ),
        (
            "action.effect.prepared".to_string(),
            Json::obj([("idempotency_key", Json::str(format!("key-{effect_id}")))]),
        ),
        (
            "action.effect.committed".to_string(),
            Json::obj([("attempt_no", Json::Int(1))]),
        ),
    ]
}

/// The `hh.submit` completion surface's declared class — a control input
/// (ADR-0173 D2/D3: `submit` is `repeat_safety = idempotent` under the
/// call's own identity), workspace-local, and itself mutation-free: the
/// row records the model's *claim*; the completion gate is what decides
/// the run's fate (`verification.gate.evaluated` is the liable act, and a
/// bad claim is refused there — never committed). `read_only` keeps the
/// verb outside Π's never-auto set honestly — an `irreversible` stamp
/// would make the kernel's own completion surface a fabricated auto-allow
/// (I-P1).
pub(crate) const SUBMIT_RISK: hh_ontology::risk::RiskClass =
    hh_ontology::risk::RiskClass::READ_ONLY;

/// The Stage-1 effect boundary. `hh.submit` completes (the full §5a.2
/// write-ahead chain mints ahead of the `observed` terminal); declared
/// host-executor surfaces dispatch to the host — `decided`/`authorized`/
/// `prepared`/`committed` land durable and the effect stays *open*
/// (`SettledOutcome::Pending` — `unknown{awaiting_host}` is not a legal
/// `unknown` cause on the real ledger; the open `committed` row is the
/// record, settled later by `report_host_effect`). A surface under
/// `requires_approval` or a `never_auto` risk class parks at `intended` —
/// no Π `decided` is ever fabricated. Anything else is refused (the
/// surface table is closed — an intent reaching the gate off-table is a
/// defect, refused loudly).
#[derive(Debug, Default)]
pub struct EmbedGate {
    /// The surfaces bound to host-executed capabilities, with their
    /// declared risk/approval facts (the admission verdict reads them).
    pub host_surfaces: BTreeMap<String, HostDecl>,
    /// Asks recorded during dispatch — the service drains them into
    /// `upcall.invoke_host_capability` notifications + pending
    /// `report_host_effect` entries after each drive.
    pub host_asks: Vec<(String, u64, Json)>,
}

impl EffectGate for EmbedGate {
    /// The boundary's gate component — the `emitted` rows are this
    /// boundary's Π-floor records (`decided`/`authorized`/`prepared`/
    /// `committed`), minted under `hh-embed`, never `hh-control`.
    fn producer_component(&self) -> &'static str {
        "hh-embed/effect-gate"
    }

    fn dispatch(&mut self, effect_id: &str, attempt_no: u64, intent: &Json) -> GateOutcome {
        let surface = intent
            .get("surface_id")
            .or_else(|| intent.get("surface"))
            .and_then(Json::as_str)
            .unwrap_or("");
        if let Some(decl) = self.host_surfaces.get(surface) {
            let risk = decl
                .risk_class
                .unwrap_or(hh_ontology::risk::RiskClass::UNKNOWN);
            // Π floor (ADR-0031 §2 + the open-time approval table): a
            // declared surface whose risk is never-auto or whose
            // declaration asked for approval parks at `intended` — the
            // ask is the service's, the gate never fabricates an allow.
            let admitted = !decl.requires_approval && !hh_monitor::approval::never_auto(risk);
            if !admitted {
                // The owed decision is durable, never silent: the ask row
                // names the effect it gates (`permission_id` is derived
                // from the effect identity — one ask per effect attempt).
                // `post_drive_scan` folds it into the session's pending
                // table so `respond_permission` stays answerable.
                let pending = (
                    "security.permission.pending".to_string(),
                    Json::obj([
                        ("permission_id", Json::str(format!("perm-{effect_id}"))),
                        ("effect_id", Json::str(effect_id)),
                        (
                            "request",
                            Json::obj([
                                (
                                    "reason",
                                    Json::str(format!(
                                        "host capability {surface} requires approval"
                                    )),
                                ),
                                (
                                    "capability_ref",
                                    Json::obj([("semantic_id", Json::str(surface))]),
                                ),
                            ]),
                        ),
                        ("mode", Json::str("sync")),
                    ]),
                );
                return GateOutcome {
                    outcome: SettledOutcome::Pending,
                    submission_ref: None,
                    error_class: None,
                    emitted: vec![pending],
                };
            }
            self.host_asks
                .push((effect_id.to_string(), attempt_no, intent.clone()));
            return GateOutcome {
                outcome: SettledOutcome::Pending,
                submission_ref: None,
                error_class: None,
                emitted: dispatch_chain(effect_id, &risk),
            };
        }
        if surface == SUBMIT_SURFACE {
            return GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".to_string(),
                },
                submission_ref: Some(format!("sub-{effect_id}")),
                error_class: None,
                emitted: dispatch_chain(effect_id, &SUBMIT_RISK),
            };
        }
        GateOutcome {
            outcome: SettledOutcome::Refused,
            submission_ref: None,
            error_class: Some("surface_not_bound".to_string()),
            emitted: Vec::new(),
        }
    }
}

/// The context assembler (DF-S2.8-1) — the embed boundary runs the real
/// `hh_context::assemble` builder on every live `propose`: the durable
/// prefix folds into `context_view`, `context.observation.recorded` items
/// project to `Observation` candidates, and `DefaultPolicy` over
/// `default_layout` assembles the plan the driver appends as
/// `context.assembled`. The §5c.5 selection leg lands with R2.5
/// (DF-S2.8-1): `ctx` carries the run's shared `KernelContext` — the
/// memory store the `MemoryPort`/`CompactionPort` legs share, the live
/// `procedure_pointer` index entries the selector projects, and the last
/// assembled plan the compaction input reads. `None` on the replay arms
/// (re-projection never re-selects — the durable rows are the record).
#[derive(Default)]
pub struct KernelAssembler {
    /// The shared context/memory fold (`None` — replay/legacy arms).
    pub ctx: Option<Rc<RefCell<KernelContext>>>,
}

/// The pinned C0 estimator the embed-boundary plan records
/// (`canonical_bytes / 4`, floor 1 — deterministic; the ref is a hash
/// input, so a swapped estimator moves `plan_id` — I-DET).
const ASSEMBLER_ESTIMATOR_REF: &str = "est/canonical_bytes_div4@1";
/// The layout the kernel assembler runs under (the spec's default slot
/// declaration set).
const ASSEMBLER_LAYOUT_REF: &str = "layout/default";
/// `idp/1` domain for the projection's `context_item_id`s — the content
/// address of the observation payload.
const OBSERVATION_ITEM_IDP: &str = "hh.embed.context_item/1";
/// `DefaultPolicy::old_after_turns` — mirrored here for the declared
/// `Optional` class (the policy derives age only for `Required`; an
/// evictable observation declares its own class).
const OBSERVATION_OLD_AFTER: u64 = 8;

/// `context.observation.recorded` → the `Observation` candidate the item
/// projects to (§5c.1 data model): the event IS the context item — its
/// seq is the source order, its provenance the item's (a bare row falls
/// back to a kernel-minted record, never a fabricated origin), its
/// canonical payload the content address the estimate charges.
fn observation_candidate(
    run_id: &str,
    model_call_id: &str,
    e: &EventEnvelope,
    at_seq: u64,
) -> Candidate {
    let canonical = e.payload.to_canonical_string();
    let tokens = ((canonical.len() / 4) as u64).max(1);
    let provenance = e
        .provenance
        .clone()
        .unwrap_or_else(|| hh_provenance::ProvenanceRecord::kernel("hh-embed/kernel", e.seq));
    let authority = provenance.authority;
    let readers = match &provenance.readers {
        hh_provenance::ReaderSet::Public => None,
        hh_provenance::ReaderSet::Restricted(rs) => {
            Some(hh_provenance::ReaderSet::Restricted(rs.clone()))
        }
    };
    Candidate {
        candidate_id: format!("{model_call_id}:obs:{}", e.event_id),
        context_item_id: Some(idp_id(OBSERVATION_ITEM_IDP, canonical.as_bytes())),
        kind: CandidateKind::Observation,
        state: CandidateState::Expanded,
        retention: Retention::Optional(if at_seq.saturating_sub(e.seq) > OBSERVATION_OLD_AFTER {
            PriorityClass::ObservationOld
        } else {
            PriorityClass::ObservationRecent
        }),
        estimate: Estimate {
            tokens,
            estimator_ref: ASSEMBLER_ESTIMATOR_REF.to_string(),
        },
        source_event: Some(EventRef {
            run_id: run_id.to_string(),
            event_id: e.event_id.clone(),
        }),
        source_seq: e.seq,
        label: hh_provenance::label::Label::at(authority),
        provenance,
        validity: hh_hir::records::Validity::open_from(e.seq),
        readers,
        slot_hint: None,
        volatile: false,
        paired_with: None,
        batch_id: None,
        artefact_id: None,
        handle: None,
    }
}

impl AssemblerPort for KernelAssembler {
    fn assemble(
        &mut self,
        inputs: &AssembleInputs<'_>,
        context_request: &Json,
    ) -> AssembledRequest {
        let run_id = inputs
            .prefix
            .first()
            .map(|e| e.run_id.as_str())
            .unwrap_or_default();
        let view = context_view(run_id, inputs.prefix, None);
        let at_seq = view.derived_from_seq.unwrap_or(0);
        // §5c.5's selection leg (R2.5 / DF-S2.8-1 a): the fold-store's
        // live `procedure_pointer` versions project to
        // `ProcedureIndexEntry`s; `select_procedures` runs
        // `index_all_under_budget` and its `context.procedure.selected`
        // row rides `pre_events` — durable *ahead* of the assembled row
        // it fed (the producer's emit order is the ledger order, CC3).
        let mut pre_events: Vec<(String, Json)> = Vec::new();
        let mut candidates: Vec<Candidate> = inputs
            .prefix
            .iter()
            .filter(|e| e.class == "context.observation.recorded")
            .map(|e| observation_candidate(run_id, inputs.model_call_id, e, at_seq))
            .collect();
        if let Some(ctx) = &self.ctx {
            let entries = {
                let c = ctx.borrow();
                procedure_index_entries(&c.store, c.store.applied_seq())
            };
            if !entries.is_empty() {
                let mut sel_sink = CollectSink::default();
                let selected = hh_context::procedure::select_procedures(
                    &entries,
                    if inputs.window_cap_tokens == 0 {
                        u64::MAX
                    } else {
                        inputs.window_cap_tokens
                    },
                    &mut sel_sink,
                );
                pre_events = sel_sink.events;
                candidates.extend(selected);
            }
        }
        let req = AssemblyRequest {
            model_call_id: inputs.model_call_id.to_string(),
            view: DerivedFrom {
                run_id: run_id.to_string(),
                seq: at_seq,
                view_hash: view.view_hash,
            },
            at_seq,
            min_view_seq: None,
            candidates,
            advisories: Vec::new(),
            layout: default_layout(),
            budget: ContextBudget {
                // `0` ⇒ no configured cap — the builder sees an unbounded
                // window rather than a zero cap that refuses everything.
                window_cap: if inputs.window_cap_tokens == 0 {
                    u64::MAX
                } else {
                    inputs.window_cap_tokens
                },
                margin: 0,
                reservations: Vec::new(),
                hard: true,
            },
            estimator_ref: ASSEMBLER_ESTIMATOR_REF.to_string(),
            policy_params: Json::Null,
            demotion_wrappers: Vec::new(),
        };
        let policy = DefaultPolicy::default();
        // `assembly_ms` is the measured wall the record stamps — measured
        // on a discarded run so the stamped value is true; `assemble` is
        // pure (I-DET) so `plan_id` is identical across the pair.
        let mut probe_sink = CollectSink::default();
        let t0 = std::time::Instant::now();
        let _ = hh_context::assemble(
            &req,
            &policy,
            ASSEMBLER_LAYOUT_REF,
            "none",
            0,
            &mut probe_sink,
        );
        let assembly_ms = t0.elapsed().as_millis() as u64;
        let mut sink = CollectSink::default();
        match hh_context::assemble(
            &req,
            &policy,
            ASSEMBLER_LAYOUT_REF,
            "none",
            assembly_ms,
            &mut sink,
        ) {
            Ok(outcome) => {
                // The builder emits `context.assembled` + side-band rows
                // into the sink; the driver appends them — `assembled`
                // first, the rest in emit order.
                let mut assembled_payload = None;
                let mut side_events = Vec::new();
                for (class, payload) in sink.events {
                    if class == "context.assembled" {
                        assembled_payload = Some(payload);
                    } else {
                        side_events.push((class, payload));
                    }
                }
                // Stash the plan + its candidate set — the `compact` leg's
                // `CompactInput` reads them (the fold-store is the shared
                // context plane; the plan is the view it compacts).
                if let Some(ctx) = &self.ctx {
                    let mut c = ctx.borrow_mut();
                    c.window_cap = inputs.window_cap_tokens;
                    c.last_plan = Some(outcome.plan.clone());
                    c.last_candidates = req
                        .candidates
                        .iter()
                        .map(|c| (c.candidate_id.clone(), c.clone()))
                        .collect();
                }
                AssembledRequest {
                    request: Json::obj([
                        ("context_request", context_request.clone()),
                        ("context_plan", outcome.plan.body_json()),
                    ]),
                    assembled_payload,
                    side_events,
                    pre_events,
                }
            }
            Err(err) => {
                // The port is infallible — an assembly error is recorded
                // on the row, never hidden, and the turn proceeds on the
                // raw context request. (Routing `ContextWindowExceeded`
                // into a `stop` outcome is the remaining DF-S2.8-1 half.)
                AssembledRequest {
                    request: Json::obj([("context_request", context_request.clone())]),
                    assembled_payload: Some(Json::obj([
                        ("assembler", Json::str("hh-context/assemble")),
                        ("assembly_error", Json::str(format!("{err}"))),
                        ("model_call_id", Json::str(inputs.model_call_id.to_string())),
                    ])),
                    side_events: Vec::new(),
                    pre_events,
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The §5c producer legs (R2.5 / DF-S2.8-1): the shared context fold, the
// memory boundary, the compaction port, and the supplies ingest.
// ─────────────────────────────────────────────────────────────────────────────

/// `KernelContext` — the run's shared context/memory fold (R-2.4.3;
/// DF-S2.8-1). One `Rc<RefCell<…>>` instance is shared by the run's three
/// producer ports — `KernelAssembler` (the §5c.5 selection leg + plan
/// stash), `KernelMemory` (retrieve/trigger/resume_set/mark_scope_ended)
/// and `KernelCompaction` (the last assembled plan is its input) — so the
/// store the assembler selects over is the store the reader reads
/// (CC1: one memory plane, never a shadow copy).
#[derive(Default)]
pub struct KernelContext {
    /// The fold-store (§5c.4's memory plane — `hh_context::MemoryStore`).
    pub store: MemoryStore,
    /// The last assembled `ContextPlan` — the compaction port's input.
    pub last_plan: Option<ContextPlan>,
    /// `last_plan`'s candidate set (`candidate_id → Candidate`) — the
    /// compaction input's candidate map.
    pub last_candidates: BTreeMap<String, Candidate>,
    /// The declared `window_cap` from the last `AssembleInputs` (0 =
    /// unbounded — the builder's convention).
    pub window_cap: u64,
    /// The run this fold belongs to.
    pub run_id: String,
}

impl KernelContext {
    /// A fresh fold for `run_id`.
    pub fn new(run_id: &str) -> Self {
        KernelContext {
            store: MemoryStore::new("kernel-memory"),
            last_plan: None,
            last_candidates: BTreeMap::new(),
            window_cap: 0,
            run_id: run_id.to_string(),
        }
    }

    /// `fold(prefix)` — the resume arm (§5c.4's replay rule: the store is
    /// a pure fold over the durable `context.memory.written` prefix, never
    /// re-minted). The durable row carries the *record* (version, kind,
    /// scope, semantic_id) — the content plane is the store's own; a
    /// folded version is a skeleton (metadata only, `""` index text) whose
    /// `by_name` resolution and lifecycle fold are honest, and whose thin
    /// `index_text` is exactly what the durable row records. Name
    /// bindings re-derive `scope:semantic_id` → `version_id` (the
    /// `put_artifact` convention — the name history is a projection of
    /// the write rows, not a second channel).
    pub fn fold(run_id: &str, prefix: &[EventEnvelope], now_ms: u64) -> Self {
        let mut ctx = Self::new(run_id);
        for e in prefix {
            if e.class != "context.memory.written" {
                continue;
            }
            let version_id = e
                .payload
                .get("version_id")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string();
            let semantic_id = e
                .payload
                .get("memory_id")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string();
            let kind = e
                .payload
                .get("kind")
                .and_then(Json::as_str)
                .and_then(MemoryKind::parse)
                .unwrap_or(MemoryKind::Fact);
            let scope = e
                .payload
                .get("scope")
                .and_then(Json::as_str)
                .and_then(hh_provenance::PersistenceScope::parse)
                .unwrap_or(hh_provenance::PersistenceScope::Session);
            if version_id.is_empty() || semantic_id.is_empty() {
                continue;
            }
            let prov = hh_provenance::ProvenanceRecord::kernel("hh-embed/memory-fold", now_ms);
            // The fold restores a *skeleton* under the durable row's own
            // `version_id` (`rehydrate` — identity is never re-minted).
            // The row carries `contract_hash` but not the contract, so the
            // skeleton declares the empty contract + no validity members —
            // `lifecycle_state` reads `unknown` (the fold cannot re-verify
            // the original contract — honest, never a fabricated `valid`).
            // The label's `authority`/`readers` members do round-trip and
            // are reconstructed (taint has no codec — a C2 member the
            // skeleton leaves empty).
            let authority = e
                .payload
                .get("label")
                .and_then(|l| l.get("authority"))
                .and_then(Json::as_str)
                .and_then(hh_provenance::AuthorityClass::parse)
                .unwrap_or(hh_provenance::AuthorityClass::Kernel);
            let readers = match e.payload.get("label").and_then(|l| l.get("readers")) {
                Some(Json::Obj(m)) => match m.get("restricted") {
                    Some(Json::Arr(rs)) => hh_provenance::ReaderSet::Restricted(
                        rs.iter()
                            .filter_map(|r| r.as_str().map(str::to_string))
                            .collect(),
                    ),
                    _ => hh_provenance::ReaderSet::Public,
                },
                _ => hh_provenance::ReaderSet::Public,
            };
            let version = hh_context::MemoryVersion {
                version_id: version_id.clone(),
                semantic_id: semantic_id.clone(),
                kind,
                subject_key: None,
                content: MemoryContent::Text(Box::new(hh_hir::leaves::Text::new(
                    "",
                    "hh-embed/memory-fold",
                    prov.clone(),
                ))),
                contract: hh_context::InvalidationContract {
                    dependencies: Vec::new(),
                    cache_hint: hh_context::CacheHint::Cacheable,
                    validator_ref: None,
                    freshness: None,
                    invalidation_condition: None,
                    revalidation: hh_context::Revalidation::Never,
                },
                scope,
                label: hh_provenance::label::Label {
                    authority,
                    taint: std::collections::BTreeSet::new(),
                    readers,
                },
                provenance: prov,
                validity: None,
                declared_inputs: Vec::new(),
                justifications: Vec::new(),
                created_at: e.seq,
                created_by: "hh-embed/memory-fold".to_string(),
                supersedes_claim: None,
                conflict_set_ref: None,
                validator_endorsed: false,
            };
            ctx.store.rehydrate(version);
            let _ = ctx
                .store
                .bind(scope, &semantic_id, &version_id, None, "fold", e.seq);
        }
        ctx
    }

    /// `put_artifact` — a kernel-authored memory write with its durable
    /// `context.memory.written` row (the store is the projection; the
    /// ledger is the record — CC3). `name` is the bound `scope:name` head
    /// a `resume_set` member reads. Returns the minted `version_id`.
    #[allow(clippy::too_many_arguments)]
    pub fn put_artifact(
        &mut self,
        ledger: &mut Store,
        lease: &Lease,
        name: &str,
        media_type: &str,
        canonical_blob: &[u8],
        kind: MemoryKind,
        scope: hh_provenance::PersistenceScope,
    ) -> Result<String, String> {
        let at_seq = ledger
            .events(&self.run_id)
            .ok()
            .and_then(|es| es.last())
            .map(|e| e.seq + 1)
            .unwrap_or(0);
        let addr = ledger
            .put_blob(canonical_blob, media_type)
            .map_err(|e| format!("{e:?}"))?;
        let now = ledger.now_ms();
        let prov = hh_provenance::ProvenanceRecord::kernel("hh-embed/memory", now);
        let content = std::str::from_utf8(canonical_blob)
            .ok()
            .and_then(|t| hh_wire::json::parse(t).ok())
            .map(MemoryContent::Structured)
            .unwrap_or_else(|| {
                MemoryContent::Text(Box::new(hh_hir::leaves::Text::new(
                    String::from_utf8_lossy(canonical_blob).into_owned(),
                    "hh-embed/memory",
                    prov.clone(),
                )))
            });
        let draft = MemoryDraft {
            kind,
            subject_key: None,
            content,
            // C-CONTRACT-1 — a `session|project|user` write needs ≥1
            // checkable member; the kernel declares the honest floor —
            // the supply lives exactly as long as its scope
            // (`scope_ended` on its own scope). `run`/`turn` scopes may
            // carry the empty contract (end-of-run is the invalidation).
            contract: if matches!(
                scope,
                hh_provenance::PersistenceScope::Session
                    | hh_provenance::PersistenceScope::Project
                    | hh_provenance::PersistenceScope::User
            ) {
                Some(hh_context::InvalidationContract {
                    dependencies: Vec::new(),
                    cache_hint: hh_context::CacheHint::Cacheable,
                    validator_ref: None,
                    freshness: None,
                    invalidation_condition: Some(hh_context::InvalidationCondition::ScopeEnded(
                        scope,
                    )),
                    revalidation: hh_context::Revalidation::Never,
                })
            } else {
                None
            },
            scope,
            declared_inputs: Vec::new(),
            justifications: Vec::new(),
            supersedes: None,
            validity: None,
            provenance: Some(prov),
            semantic_id: Some(name.to_string()),
            validator_endorsed: false,
        };
        let wctx = hh_context::WriteContext {
            context_label: hh_provenance::label::Label::at(
                hh_provenance::AuthorityClass::Principal,
            ),
            // The *memory store's* scope lease, not the ledger's writer
            // lease — the fenced-write check reads `store.lease(scope)`
            // (the kernel is the store's sole writer; generation 0 when
            // the scope lease was never taken).
            lease_generation: self.store.lease(scope),
            at_seq,
            run_id: self.run_id.clone(),
        };
        let outcome = self
            .store
            .put(draft, &wctx)
            .map_err(|e| format!("memory_put: {e:?}"))?;
        let version_id = outcome.version.version_id.clone();
        self.store
            .bind(scope, name, &version_id, None, "supply", at_seq)
            .map_err(|e| format!("memory_bind: {e:?}"))?;
        // The durable record — `context.memory.written` lands under the
        // run's writer lease. The store minted the row itself
        // (`PutOutcome.event` — the producer's own payload, CC1); we add
        // `content_ref` — the blob-pool address the fold can re-resolve.
        let (class, payload) = outcome.event;
        let mut payload = match payload {
            Json::Obj(m) => m,
            other => unreachable!("memory_written payload is an object: {other:?}"),
        };
        payload.insert(
            "content_ref".to_string(),
            Json::str(format!("{}:{}", addr.algorithm, addr.digest)),
        );
        let ts = ledger.ts_now();
        let parent = ledger
            .events(&self.run_id)
            .ok()
            .and_then(|es| es.last())
            .map(|e| e.event_id.clone())
            .unwrap_or_else(|| hh_ledger::ids::ROOT_EVENT.to_string());
        let mut ev = hh_control::events::kernel_event(
            format!("{}-memw-{at_seq}", self.run_id),
            &class,
            ts,
            hh_ledger::event::Scope::default(),
            parent,
            vec![],
            Json::Obj(payload),
        );
        // `context.memory.written` is audit-grade — Rule P requires the
        // kernel's provenance record on the envelope (the sink stamps it
        // on driver-appended rows; this direct append carries its own).
        ev.provenance = Some(hh_provenance::ProvenanceRecord::kernel(
            "hh-embed/memory",
            now,
        ));
        ledger
            .append(&self.run_id, lease, vec![ev])
            .map_err(|e| format!("{e:?}"))?;
        Ok(version_id)
    }
}

/// `ingest_supplies` — the `open` half of the §5c.4 supply chain (R2.5):
/// `supplies.procedures[]` write `procedure_pointer` versions into the
/// fold-store (`context.memory.written` durable under the lease), bound
/// `session:<member>` so a `resume_set` head and a `trigger`/`by_name`
/// read resolve them. The write's `at_seq` is the ledger seq the durable
/// row lands at — the store watermark and the ledger seq share one axis.
pub fn ingest_supplies(
    ledger: &mut Store,
    lease: &Lease,
    ctx: &mut KernelContext,
    supplies: &BTreeMap<String, Json>,
    kind: MemoryKind,
) -> Result<(), String> {
    for (member, record) in supplies {
        let bytes = record.to_canonical_string().into_bytes();
        ctx.put_artifact(
            ledger,
            lease,
            member,
            "application/json",
            &bytes,
            kind,
            hh_provenance::PersistenceScope::Session,
        )
        .map_err(|e| format!("supplies.{member}: {e}"))?;
    }
    Ok(())
}

/// `procedure_index_entries` — project the fold-store's live
/// `procedure_pointer` versions into the §5c.5 `ProcedureIndexEntry`s the
/// selector consumes. A pointer whose checkable `preconditions` the
/// projection cannot evaluate counts `uncheckable` and still passes
/// (the note rides `ProcedureIndexEntry.uncheckable`; §5c.5's discipline:
/// uncheckable is a note, never a silent skip nor a fabricated pass —
/// the selection row carries the projection honestly).
fn procedure_index_entries(
    store: &MemoryStore,
    at_seq: u64,
) -> Vec<hh_context::procedure::ProcedureIndexEntry> {
    store
        .version_order()
        .iter()
        .filter_map(|vid| {
            let v = store.version(vid)?;
            if v.kind != MemoryKind::ProcedurePointer {
                return None;
            }
            match lifecycle_state(store, vid, at_seq).kind() {
                LifecycleStateKind::Valid
                | LifecycleStateKind::StaleByDependency
                | LifecycleStateKind::Unknown => {}
                _ => return None,
            }
            let body = match &v.content {
                MemoryContent::Structured(j) => j.clone(),
                MemoryContent::Text(t) => {
                    hh_wire::json::parse(t.content.as_deref().unwrap_or_default())
                        .unwrap_or(Json::Null)
                }
            };
            let tokens = (v.content.index_text().len() as u64 / 4).max(1);
            Some(hh_context::procedure::ProcedureIndexEntry {
                procedure_id: v.semantic_id.clone(),
                estimate: Estimate {
                    tokens,
                    estimator_ref: ASSEMBLER_ESTIMATOR_REF.to_string(),
                },
                label: v.label.clone(),
                provenance: v.provenance.clone(),
                preconditions_passed: true,
                uncheckable: body
                    .get("procedure")
                    .and_then(|p| p.get("preconditions"))
                    .map(|p| match p {
                        Json::Arr(a) => a.len() as u64,
                        _ => 0,
                    })
                    .unwrap_or(0),
                index_item_id: idp_id("hh.embed.procedure_index/1", v.version_id.as_bytes()),
            })
        })
        .collect()
}

/// `KernelMemory` — the kernel's `MemoryPort` (R-2.4.3 / DF-S2.8-1):
/// `retrieve{query}` decisions, the `trigger{path_touched}` leg, the
/// carried `resume_set` drain, and `mark_scope_ended` all run the real
/// `hh_context::retrieve` pipeline over the shared fold-store. The
/// driver owns the appends — every leg returns its emitted
/// `(class, payload)` rows in emit order (the `CollectSink` captures;
/// nothing durable mints inside the port).
pub struct KernelMemory {
    /// The shared context/memory fold.
    pub ctx: Rc<RefCell<KernelContext>>,
    /// The layout id retrieval reports (the assembler's `default`).
    pub layout_id: String,
}

impl KernelMemory {
    /// Wire the port over the shared fold.
    pub fn new(ctx: Rc<RefCell<KernelContext>>) -> Self {
        KernelMemory {
            ctx,
            layout_id: ASSEMBLER_LAYOUT_REF.to_string(),
        }
    }

    /// Run the retrieval pipeline once, returning the emitted rows.
    fn run_retrieve(
        &self,
        query: RetrievalQuery,
        model_call_id: &str,
    ) -> Result<Vec<(String, Json)>, String> {
        let mut ctx = self.ctx.borrow_mut();
        let at = (ctx.run_id.clone(), ctx.store.applied_seq());
        let req = RetrievalRequest {
            model_call_id: model_call_id.to_string(),
            at,
            layers: [
                hh_context::Layer::Artifact,
                hh_context::Layer::Episodic,
                hh_context::Layer::Procedural,
                hh_context::Layer::Session,
            ]
            .into_iter()
            .collect(),
            query,
            constraints: SlotConstraints {
                slot_min_authority: hh_provenance::AuthorityClass::Unverified,
                validity_policy: ValidityPolicy {
                    admitted_states: [
                        hh_context::LifecycleStateKind::Valid,
                        hh_context::LifecycleStateKind::StaleByDependency,
                        hh_context::LifecycleStateKind::Unknown,
                    ]
                    .into_iter()
                    .collect(),
                    conflict_policy: hh_context::vocab::ConflictPolicy::DeliverAllAnnotated,
                    max_stale: None,
                },
                readers_required: None,
            },
            reader: "hh-embed/driver".to_string(),
            budget: RetrievalBudget {
                tokens: 4096,
                k: 16,
            },
            ranker: DETERMINISTIC_DEFAULT.to_string(),
            mode: hh_identity::names::ResolveMode::Execute,
        };
        let mut sink = CollectSink::default();
        hh_context::retrieve::retrieve(&mut ctx.store, &req, &mut sink, None, || 0)
            .map_err(|e| format!("retrieve: {e:?}"))?;
        Ok(sink.events)
    }
}

impl MemoryPort for KernelMemory {
    fn retrieve(
        &mut self,
        query: &Json,
        model_call_id: &str,
        _watermark: (String, u64),
    ) -> Result<Vec<(String, Json)>, String> {
        // The model's `retrieve{query}` member decodes through the closed
        // `RetrievalQuery` sum — an unknown member is a typed refusal,
        // never a guessed query (the `StaleStore` read-your-writes check
        // rides the request's `at` = the store's applied watermark).
        let q = RetrievalQuery::from_json(query).map_err(|e| format!("query: {e:?}"))?;
        self.run_retrieve(q, model_call_id)
    }

    fn trigger_retrieve(
        &mut self,
        path: &str,
        model_call_id: &str,
        _watermark: (String, u64),
    ) -> Result<Vec<(String, Json)>, String> {
        self.run_retrieve(
            RetrievalQuery::Trigger {
                kind: TriggerKind::PathTouched(path.to_string()),
            },
            model_call_id,
        )
    }

    fn resume_set_read(
        &mut self,
        heads: &[String],
        _watermark: (String, u64),
    ) -> Result<Vec<(String, Json)>, String> {
        // §5c.4's consumption: each carried head resolves `by_name` —
        // the scope is discovered against the folded name history
        // (session-scoped first, then the wider floors — a head is a
        // name, and the name history is the only binding table).
        let mut events = Vec::new();
        for head in heads {
            let scope = {
                let ctx = self.ctx.borrow();
                [
                    hh_provenance::PersistenceScope::Session,
                    hh_provenance::PersistenceScope::Run,
                    hh_provenance::PersistenceScope::Project,
                    hh_provenance::PersistenceScope::User,
                    hh_provenance::PersistenceScope::Definition,
                ]
                .into_iter()
                .find(|scope| {
                    ctx.store
                        .resolve(
                            *scope,
                            Some(head),
                            None,
                            hh_identity::names::ResolveMode::Audit,
                        )
                        .is_ok()
                })
                .unwrap_or(hh_provenance::PersistenceScope::Session)
            };
            events.extend(self.run_retrieve(
                RetrievalQuery::ByName {
                    scope,
                    name: head.clone(),
                },
                "resume_set",
            )?);
        }
        Ok(events)
    }

    fn mark_scope_ended(
        &mut self,
        scope: hh_provenance::PersistenceScope,
        at_seq: u64,
    ) -> Vec<(String, Json)> {
        let mut ctx = self.ctx.borrow_mut();
        // The expiry floor applies to every non-terminal version in
        // `scope` — collect them before the mark (the fold's `until` is
        // `at_seq` — the durable tip the driver passed). `unknown` counts
        // as live: a version with nothing checkable still ends with its
        // scope (§5c.4's unconditional floor; AC-R-2.4.4-8 — the expiry
        // fires whether or not the contract declares `scope_ended`). Only
        // already-terminal states (`revoked`, `superseded`, `expired`)
        // re-mint nothing.
        let expired: Vec<String> = ctx
            .store
            .version_order()
            .iter()
            .filter(|vid| {
                ctx.store
                    .version(vid)
                    .map(|v| v.scope == scope)
                    .unwrap_or(false)
            })
            .filter(|vid| {
                !matches!(
                    lifecycle_state(&ctx.store, vid, at_seq).kind(),
                    LifecycleStateKind::Revoked
                        | LifecycleStateKind::Superseded
                        | LifecycleStateKind::Expired
                )
            })
            .cloned()
            .collect();
        ctx.store.mark_scope_ended(scope);
        let prov = hh_provenance::ProvenanceRecord::kernel("hh-embed/memory", at_seq);
        expired
            .into_iter()
            .map(|vid| {
                let mut m = match memory_invalidated_payload(
                    &vid,
                    RevocationReason::Expired,
                    &prov,
                    None,
                ) {
                    Json::Obj(m) => m,
                    other => unreachable!("memory_invalidated_payload is an object: {other:?}"),
                };
                m.insert("fired_stamp".to_string(), Json::str("scope_ended"));
                ("context.memory.invalidated".to_string(), Json::Obj(m))
            })
            .collect()
    }

    fn procedure_capabilities(&self, artefact_id: &str) -> Option<Vec<String>> {
        // `procedure_index:<semantic_id>` / `procedure_body:<semantic_id>`
        // artefact ids resolve to the pointer's declared
        // `procedure.allowed_capabilities[]`. The assembled row's
        // `artefact_id` is the index's compiled identity
        // (`procedure_index_entries` stamps `idp_id(
        // "hh.embed.procedure_index/1", version_id)`) — resolve both
        // spellings to the same pointer (CC1: one capability table per
        // procedure, whichever artefact id the delivery carried).
        let ctx = self.ctx.borrow();
        let semantic = artefact_id
            .strip_prefix("procedure_index:")
            .or_else(|| artefact_id.strip_prefix("procedure_body:"))
            .unwrap_or(artefact_id);
        ctx.store
            .version_order()
            .iter()
            .filter_map(|vid| ctx.store.version(vid))
            .find(|v| {
                v.kind == MemoryKind::ProcedurePointer
                    && (v.semantic_id == semantic
                        || idp_id("hh.embed.procedure_index/1", v.version_id.as_bytes())
                            == artefact_id)
            })
            .and_then(|v| match &v.content {
                MemoryContent::Structured(j) => j
                    .get("procedure")
                    .and_then(|p| p.get("allowed_capabilities"))
                    .map(|a| match a {
                        Json::Arr(a) => a
                            .iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect(),
                        _ => Vec::new(),
                    }),
                _ => None,
            })
    }
}

/// `KernelCompaction` — the kernel's `CompactionPort` (DF-S2.8-1): the
/// real `hh_context::compact` ladder over the last assembled plan, with
/// `evict_oldest` as the always-present rung (the in-process kernel
/// strategy; the OOP packaged-variant rung is the corpus battery's).
/// The emitted `context.compaction.{started,completed}` rows return to
/// the driver through `CompactionDone.emitted` — the port computes, the
/// fenced writer lands (CC3).
pub struct KernelCompaction {
    /// The shared context/memory fold (the plan stash is its input).
    pub ctx: Rc<RefCell<KernelContext>>,
    /// Extra strategy rungs ahead of `evict_oldest` (the declared variant
    /// ladder — empty at C0).
    pub variants: Vec<Box<dyn hh_context::compact::CompactionStrategy>>,
}

impl KernelCompaction {
    /// Wire the port over the shared fold.
    pub fn new(ctx: Rc<RefCell<KernelContext>>) -> Self {
        KernelCompaction {
            ctx,
            variants: Vec::new(),
        }
    }
}

impl CompactionPort for KernelCompaction {
    fn compact(&mut self, reason: &str) -> Result<CompactionDone, CompactionImpossible> {
        let ctx = self.ctx.borrow();
        let Some(plan) = &ctx.last_plan else {
            // No assembled plan to compact — the typed refusal the port
            // boundary carries (never a fabricated empty view).
            return Err(CompactionImpossible {
                required_tokens: 0,
                cap: ctx.window_cap,
            });
        };
        let trigger = match reason {
            "explicit" | "request_principal" | "requested" => CompactionTrigger::RequestPrincipal,
            r if r.contains("overflow") => CompactionTrigger::OverflowReactive {
                error_ref: r.to_string(),
            },
            _ => CompactionTrigger::OccupancyHard,
        };
        let input = CompactInput {
            plan,
            candidates: ctx.last_candidates.clone(),
            window_cap: if ctx.window_cap == 0 {
                u64::MAX
            } else {
                ctx.window_cap
            },
            trigger,
            needed: 1,
            target_fraction_ppm: 0,
            scope: hh_provenance::PersistenceScope::Run,
            at: ctx.store.applied_seq(),
            run_id: ctx.run_id.clone(),
            summarizer: None,
            slot_min_authority: default_layout()
                .slots
                .iter()
                .map(|s| (s.slot_id.clone(), s.min_authority))
                .collect(),
            item_texts: BTreeMap::new(),
            item_kinds: BTreeMap::new(),
            extractor: None,
            provider: None,
            previous_summary_ref: None,
        };
        let fallback = EvictOldest;
        let mut variant_refs: Vec<&dyn hh_context::compact::CompactionStrategy> =
            self.variants.iter().map(|v| v.as_ref()).collect();
        variant_refs.push(&fallback);
        let mut sink = CollectSink::default();
        let outcome =
            hh_context::compact::compact(&input, &variant_refs, &mut sink).map_err(|_e| {
                CompactionImpossible {
                    required_tokens: plan.occupancy_estimate,
                    cap: ctx.window_cap,
                }
            })?;
        // The post-compaction `context_view` hash — the compacted slots'
        // item ids canonicalized under the view domain (the same `idp/1`
        // discipline `hh_ledger::views` uses; the record's own hash is
        // the `compaction_id`).
        let view_preimage = Json::obj([
            ("kind", Json::str("compacted_view")),
            ("run_id", Json::str(ctx.run_id.clone())),
            (
                "items",
                Json::Arr(
                    outcome
                        .view
                        .slots
                        .iter()
                        .flat_map(|s| s.items.iter())
                        .map(|i| Json::str(i.context_item_id.clone()))
                        .collect(),
                ),
            ),
        ]);
        Ok(CompactionDone {
            view_hash: idp_id(
                "hh.embed.compacted_view/1",
                view_preimage.to_canonical_string().as_bytes(),
            ),
            emitted: sink.events,
        })
    }
}
