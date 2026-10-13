//! R2.5 — the context/memory producer legs (docs/tickets/111_R2.5;
//! DF-S2.8-1 members a/b/e; R-2.4.3/§5c.3–4):
//!
//! - **producer sequence** — a real `driver.run` mints
//!   `context.assembled` → `context.retrieval.completed` →
//!   `context.memory.read` → `context.compaction.{started,completed}` →
//!   `context.memory.invalidated{scope_ended}` in durable order, all
//!   appended under the driver's writer lease (AC-1: the rows come from
//!   the loop, not test scaffolding — the ports run the real
//!   `hh_context::retrieve`/`compact` pipelines over a real
//!   `MemoryStore`).
//! - **`resume_set` consumption** (AC-2) — an armed
//!   `DriverConfig.resume_set_heads` drains `by_name` through the
//!   `MemoryPort` before the first `decide`; the `context.memory.read`
//!   row's `delivered[]` is the consumption record; an armed set with no
//!   memory boundary fails `UnbackedPort{resume_set}`.
//! - **`mark_scope_ended`** — run-scoped versions expire at finish; the
//!   `invalidated{reason: expired, fired_stamp: scope_ended}` rows land
//!   inside the closing turn scope, ahead of `lifecycle.turn.finished`.
//! - **`trigger{path_touched}`** (AC-R-2.4.3-12) — an observed effect
//!   whose intent names `args.path` runs the trigger retrieval leg.
//! - **`human`/`judged` detectors** (member e) — `HumanInput::
//!   ArtefactMark` mints `context.artefact.activated{detector: human}`
//!   only against a durable `delivered` row; a `verify` subject of
//!   `kind: artefact_activation`/`artefact_followed` mints the `judged`
//!   rows from a decided affirmative verdict — and neither class feeds
//!   the deterministic `followed` fold.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

use hh_context::compact::{CompactInput, CompactionTrigger, EvictOldest};
use hh_context::{
    CacheHint, CollectSink, InvalidationCondition, InvalidationContract, MemoryContent,
    MemoryDraft, MemoryKind, MemoryStore, RetrievalBudget, RetrievalQuery, RetrievalRequest,
    Revalidation, RevocationReason, SlotConstraints, TriggerKind, ValidityPolicy,
    DETERMINISTIC_DEFAULT,
};
use hh_control::driver::{
    AssembleInputs, AssembledRequest, AssemblerPort, CompactionDone, CompactionImpossible,
    CompactionPort, Driver, DriverConfig, EffectGate, GateOutcome, LedgerSink, MemoryPort,
    ModelOutcome, ModelPort, VerifyPort,
};
use hh_control::output::{ParamKind, ParamSpec, ParsedCall, SurfaceSpec};
use hh_control::policy::EnvelopePolicy;
use hh_control::react::ReactMinimal;
use hh_control::strategy::{ControlContext, ControlStrategy, StrategyParams};
use hh_control::vocab::{
    ControlDecision, Cue, DecisionKind, DecisionStamp, HumanInput, SettledOutcome,
};
use hh_ledger::classes::Durability;
use hh_ledger::event::{Event, EventEnvelope, EventPlane};
use hh_ledger::manifest::ParticipantClass;
use hh_ontology::control::{DecisionPoint, Owner, StopReason};
use hh_wire::json::Json;

// ── test doubles ────────────────────────────────────────────────────────

/// The in-memory sink — `prefix()` is the fold input (the durable seq is
/// what ordering assertions read, never in-memory state).
struct MemSink {
    events: Vec<EventEnvelope>,
    seq: u64,
}

impl MemSink {
    fn new() -> Self {
        MemSink {
            events: vec![],
            seq: 0,
        }
    }
    fn classes(&self) -> Vec<&str> {
        self.events.iter().map(|e| e.class.as_str()).collect()
    }
    fn find(&self, class: &str) -> Vec<&EventEnvelope> {
        self.events.iter().filter(|e| e.class == class).collect()
    }
    fn pos(&self, class: &str) -> usize {
        self.classes()
            .iter()
            .position(|c| *c == class)
            .unwrap_or_else(|| panic!("missing {class}"))
    }
}

impl LedgerSink for MemSink {
    fn append(&mut self, events: Vec<Event>) -> Result<(), String> {
        for mut e in events {
            // The `KernelSink` convention — the fenced writer stamps the
            // kernel's provenance on rows the driver emits bare (the
            // provenance-bearing producers ride `append_prov`).
            if e.provenance.is_none() {
                e.provenance = Some(hh_provenance::ProvenanceRecord::kernel(
                    "hh-test/sink",
                    self.seq + 1,
                ));
            }
            self.seq += 1;
            self.events.push(EventEnvelope {
                event_id: e.event_id,
                run_id: "run".into(),
                seq: self.seq,
                ts: e.ts,
                hlc: None,
                plane: EventPlane::of_class(&e.class).unwrap_or(EventPlane::Control),
                class: e.class,
                schema_version: 1,
                producer: e.producer,
                participant_class: ParticipantClass::Native,
                observability_level: Default::default(),
                durability: Durability::Ledger,
                scope: e.scope,
                lease_generation: 1,
                parent_event_id: e.parent_event_id,
                causes: e.causes,
                refs: vec![],
                ir_refs: vec![],
                surface_ids: Default::default(),
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

/// A scripted model — pops outcomes in order.
struct ScriptedModel {
    script: VecDeque<ModelOutcome>,
}

impl ModelPort for ScriptedModel {
    fn call(&mut self, _id: &str, _req: &Json) -> ModelOutcome {
        self.script.pop_front().unwrap_or(ModelOutcome {
            stop_reason: hh_gateway::vocab::StopReason::EndTurn,
            response_ref: "r-empty".into(),
            text_empty: true,
            calls: vec![],
            error_class: None,
            retry_after_ms: None,
        })
    }
}

/// `hh.submit` answers `observed` + the submission marker.
struct ScriptedGate;

impl EffectGate for ScriptedGate {
    fn dispatch(&mut self, _ef: &str, _a: u64, intent: &Json) -> GateOutcome {
        let surface = intent
            .get("surface_id")
            .or_else(|| intent.get("surface"))
            .and_then(Json::as_str)
            .unwrap_or("");
        if surface == "hh.submit" {
            GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: Some("sub-1".into()),
                error_class: None,
                emitted: Vec::new(),
            }
        } else {
            GateOutcome {
                outcome: SettledOutcome::Observed {
                    outcome: "applied".into(),
                },
                submission_ref: None,
                error_class: None,
                emitted: Vec::new(),
            }
        }
    }
    fn finish_record(&self) -> Option<Json> {
        Some(Json::obj([("completion", Json::str("achieved"))]))
    }
}

/// The assembler double — emits `context.assembled` plus the optional
/// `delivered`/`selected` side bands a real builder produces.
struct FakeAssembler {
    delivered: Vec<(String, Json)>,
    selected: Option<Json>,
}

impl AssemblerPort for FakeAssembler {
    fn assemble(&mut self, inputs: &AssembleInputs<'_>, _req: &Json) -> AssembledRequest {
        AssembledRequest {
            request: Json::obj([("model_call_id", Json::str(inputs.model_call_id))]),
            assembled_payload: Some(Json::obj([
                ("plan_id", Json::str("plan-1")),
                ("model_call_id", Json::str(inputs.model_call_id)),
                (
                    "items",
                    Json::Arr(
                        self.delivered
                            .iter()
                            .map(|(_, p)| {
                                Json::obj([(
                                    "delivery_id",
                                    p.get("delivery_id").cloned().unwrap_or(Json::Null),
                                )])
                            })
                            .collect(),
                    ),
                ),
            ])),
            side_events: self.delivered.clone(),
            pre_events: self
                .selected
                .iter()
                .map(|p| ("context.procedure.selected".to_string(), p.clone()))
                .collect(),
        }
    }
}

/// `FakeMemory` — a `MemoryPort` over a *real* `hh_context::MemoryStore`:
/// the legs run the landed `retrieve`/`lifecycle` machinery; only the
/// event capture is a sink buffer (the driver owns the appends).
struct FakeMemory {
    store: MemoryStore,
}

impl FakeMemory {
    fn new() -> Self {
        FakeMemory {
            store: MemoryStore::new("test-memory"),
        }
    }
    /// Seed a named version — the write a prior activation's supply or
    /// memory write would have recorded (`scope` is the persistence floor
    /// the `scope_ended` test exercises).
    fn seed(&mut self, name: &str, scope: hh_provenance::PersistenceScope, at_seq: u64) -> String {
        let prov = hh_provenance::ProvenanceRecord::kernel("hh-test/memory", 0);
        let draft = MemoryDraft {
            kind: MemoryKind::Fact,
            subject_key: None,
            content: MemoryContent::Text(Box::new(hh_hir::leaves::Text::new(
                format!("the seeded note body for {name}"),
                "hh-test/memory",
                prov.clone(),
            ))),
            contract: if matches!(
                scope,
                hh_provenance::PersistenceScope::Session
                    | hh_provenance::PersistenceScope::Project
                    | hh_provenance::PersistenceScope::User
            ) {
                // C-CONTRACT-1 — a `session|project|user` write needs ≥1
                // checkable member; `scope_ended` on its own scope is the
                // honest floor (the version lives exactly as long as the
                // scope that wrote it).
                Some(InvalidationContract {
                    dependencies: vec![],
                    cache_hint: CacheHint::Cacheable,
                    validator_ref: None,
                    freshness: None,
                    invalidation_condition: Some(InvalidationCondition::ScopeEnded(scope)),
                    revalidation: Revalidation::Never,
                })
            } else {
                None
            },
            scope,
            declared_inputs: vec![],
            justifications: vec![],
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
            lease_generation: self.store.lease(scope),
            at_seq,
            run_id: "run".to_string(),
        };
        let out = self.store.put(draft, &wctx).expect("seed put");
        let vid = out.version.version_id.clone();
        self.store
            .bind(scope, name, &vid, None, "seed", at_seq)
            .expect("seed bind");
        vid
    }
    fn run_retrieve(
        &mut self,
        query: RetrievalQuery,
        model_call_id: &str,
    ) -> Result<Vec<(String, Json)>, String> {
        let req = RetrievalRequest {
            model_call_id: model_call_id.to_string(),
            at: ("run".to_string(), self.store.applied_seq()),
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
            reader: "hh-test".into(),
            budget: RetrievalBudget {
                tokens: 4096,
                k: 16,
            },
            ranker: DETERMINISTIC_DEFAULT.to_string(),
            mode: hh_identity::names::ResolveMode::Execute,
        };
        let mut sink = CollectSink::default();
        hh_context::retrieve::retrieve(&mut self.store, &req, &mut sink, None, || 0)
            .map_err(|e| format!("retrieve: {e:?}"))?;
        Ok(sink.events)
    }
}

impl MemoryPort for FakeMemory {
    fn retrieve(
        &mut self,
        query: &Json,
        model_call_id: &str,
        _watermark: (String, u64),
    ) -> Result<Vec<(String, Json)>, String> {
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
        let mut events = Vec::new();
        for head in heads {
            let scope = [
                hh_provenance::PersistenceScope::Session,
                hh_provenance::PersistenceScope::Run,
                hh_provenance::PersistenceScope::Project,
                hh_provenance::PersistenceScope::User,
            ]
            .into_iter()
            .find(|scope| {
                self.store
                    .resolve(
                        *scope,
                        Some(head),
                        None,
                        hh_identity::names::ResolveMode::Audit,
                    )
                    .is_ok()
            })
            .unwrap_or(hh_provenance::PersistenceScope::Session);
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
        let expired: Vec<String> = self
            .store
            .version_order()
            .iter()
            .filter(|vid| {
                self.store
                    .version(vid)
                    .map(|v| v.scope == scope)
                    .unwrap_or(false)
            })
            .filter(|vid| {
                // The scope floor expires every non-terminal version —
                // `unknown` included (a version with nothing checkable
                // still ends with its scope); only already-terminal
                // states mint nothing.
                !matches!(
                    hh_context::lifecycle_state(&self.store, vid, at_seq).kind(),
                    hh_context::LifecycleStateKind::Revoked
                        | hh_context::LifecycleStateKind::Superseded
                        | hh_context::LifecycleStateKind::Expired
                )
            })
            .cloned()
            .collect();
        self.store.mark_scope_ended(scope);
        let prov = hh_provenance::ProvenanceRecord::kernel("hh-test/memory", at_seq);
        expired
            .into_iter()
            .map(|vid| {
                let mut m = match hh_context::events::memory_invalidated_payload(
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
        let semantic = artefact_id
            .strip_prefix("procedure_index:")
            .or_else(|| artefact_id.strip_prefix("procedure_body:"))
            .unwrap_or(artefact_id);
        self.store
            .version_order()
            .iter()
            .filter_map(|vid| self.store.version(vid))
            .find(|v| v.semantic_id == semantic && v.kind == MemoryKind::ProcedurePointer)
            .and_then(|v| match &v.content {
                MemoryContent::Structured(j) => j
                    .get("procedure")
                    .and_then(|p| p.get("allowed_capabilities"))
                    .map(|a| match a {
                        Json::Arr(a) => a
                            .iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect(),
                        _ => vec![],
                    }),
                _ => None,
            })
    }
}

/// `FakeCompaction` — a `CompactionPort` running the real `hh_context::
/// compact` ladder (`evict_oldest` rung) over a caller-seeded plan +
/// candidate map; the emitted rows ride `CompactionDone.emitted` — the
/// driver lands them.
struct FakeCompaction {
    plan: Option<hh_context::ContextPlan>,
    candidates: BTreeMap<String, hh_context::Candidate>,
    /// The compaction `window_cap` — below `occupancy_estimate` ⇒ a real
    /// reclaim target.
    window_cap: u64,
    calls: u64,
}

impl CompactionPort for FakeCompaction {
    fn compact(&mut self, reason: &str) -> Result<CompactionDone, CompactionImpossible> {
        self.calls += 1;
        let Some(plan) = self.plan.clone() else {
            return Err(CompactionImpossible {
                required_tokens: 100,
                cap: 50,
            });
        };
        let trigger = match reason {
            "explicit" | "requested" => CompactionTrigger::RequestPrincipal,
            _ => CompactionTrigger::OccupancyHard,
        };
        let input = CompactInput {
            plan: &plan,
            candidates: self.candidates.clone(),
            window_cap: self.window_cap,
            trigger,
            needed: 1,
            target_fraction_ppm: hh_context::compact::DEFAULT_TARGET_FRACTION_PPM,
            scope: hh_provenance::PersistenceScope::Run,
            at: 0,
            run_id: "run".to_string(),
            summarizer: None,
            slot_min_authority: hh_context::default_layout()
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
        let mut sink = CollectSink::default();
        let _outcome =
            hh_context::compact::compact(&input, &[&fallback], &mut sink).map_err(|_| {
                CompactionImpossible {
                    required_tokens: plan.occupancy_estimate,
                    cap: self.window_cap,
                }
            })?;
        Ok(CompactionDone {
            view_hash: "cv-test".into(),
            emitted: sink.events,
        })
    }
}

/// A candidate + planned item the compact fixture uses (the
/// `acceptance_s28` `cand_at`/`planned_item` shapes — minimal but real).
fn compact_cand(
    id: &str,
    kind: hh_context::CandidateKind,
    authority: hh_provenance::AuthorityClass,
    tokens: u64,
    priority: hh_context::PriorityClass,
    seq: u64,
) -> (hh_context::Candidate, hh_context::PlannedItem) {
    let c = hh_context::Candidate {
        candidate_id: id.to_string(),
        context_item_id: Some(format!("sha256:item-{id}")),
        kind,
        state: hh_context::CandidateState::Expanded,
        retention: hh_context::Retention::Optional(priority),
        estimate: hh_context::Estimate {
            tokens,
            estimator_ref: "est/pinned".to_string(),
        },
        source_event: None,
        source_seq: seq,
        label: hh_provenance::label::Label::at(authority),
        provenance: hh_provenance::ProvenanceRecord::kernel("hh-test/context", 0),
        validity: hh_hir::records::Validity::open_from(0),
        readers: None,
        slot_hint: None,
        volatile: false,
        paired_with: None,
        batch_id: None,
        artefact_id: None,
        handle: None,
    };
    let p = hh_context::PlannedItem {
        candidate_id: c.candidate_id.clone(),
        context_item_id: c.context_item_id.clone().unwrap(),
        artefact_id: c.artefact_id.clone(),
        delivery_id: format!("del:{id}"),
        authority,
        label: c.label.clone(),
        tokens,
        state: c.state,
        delivered_by_reference: false,
        derived_from: None,
    };
    (c, p)
}

/// A hand-built `ContextPlan` whose occupancy exceeds `cap` — the
/// `transcript` slot holds the items in order (the same fixture shape
/// `acceptance_s28`'s `plan_of` uses).
fn compaction_fixture(
    pairs: Vec<(hh_context::Candidate, hh_context::PlannedItem)>,
    occupancy: u64,
) -> (
    hh_context::ContextPlan,
    BTreeMap<String, hh_context::Candidate>,
) {
    let mut candidates = BTreeMap::new();
    let mut planned = Vec::new();
    for (c, p) in pairs {
        candidates.insert(c.candidate_id.clone(), c);
        planned.push(p);
    }
    let plan = hh_context::ContextPlan {
        plan_id: "plan:test".into(),
        model_call_id: "mc-1".into(),
        derived_from: hh_context::DerivedFrom {
            run_id: "run".into(),
            seq: 0,
            view_hash: "sha256:view".into(),
        },
        slots: vec![hh_context::SlotFill {
            slot_id: "transcript".into(),
            items: planned,
        }],
        omitted: vec![],
        context_label: hh_provenance::label::Label::at(hh_provenance::AuthorityClass::Delegate),
        occupancy_estimate: occupancy,
        legal_cut_points: (0..=64)
            .map(|i| hh_context::CutPoint { before_index: i })
            .collect(),
        reserved: 0,
        estimator_ref: "est/pinned".into(),
        static_hash: "sha256:static".into(),
    };
    (plan, candidates)
}

/// `ScriptedStrategy` — `ReactMinimal` underneath; a `decide` overlay
/// drives the producer legs a staged variant owns (retrieve/compact are
/// model/code decision points — react never emits them; the driver arm
/// under test is what executes them honestly).
struct ScriptedStrategy {
    inner: ReactMinimal,
    /// `(cue-class prefix, decision)` rules — first match wins.
    script: Vec<(&'static str, DecisionKind)>,
    fired: RefCell<Vec<bool>>,
}

impl ScriptedStrategy {
    fn new(script: Vec<(&'static str, DecisionKind)>) -> Self {
        ScriptedStrategy {
            inner: ReactMinimal::new(),
            fired: RefCell::new(vec![false; script.len()]),
            script,
        }
    }
}

impl ControlStrategy for ScriptedStrategy {
    fn capabilities(&self) -> &hh_control::strategy::ControlCapabilities {
        self.inner.capabilities()
    }
    fn open(
        &mut self,
        ctx: &ControlContext,
    ) -> Result<hh_control::state::ControlState, hh_control::strategy::ControlError> {
        self.inner.open(ctx)
    }
    fn observe(&self, state: &mut hh_control::state::ControlState, events: &[EventEnvelope]) {
        self.inner.observe(state, events);
    }
    fn decide(&self, state: &mut hh_control::state::ControlState, cue: &Cue) -> ControlDecision {
        let tag = match cue {
            Cue::ModelCompleted { .. } => "model_completed",
            Cue::RetrievalCompleted => "retrieval_completed",
            Cue::CompactionCompleted { .. } => "compaction_completed",
            Cue::VerificationCompleted => "verification_completed",
            Cue::EffectsSettled { .. } => "effects_settled",
            Cue::HumanInput(_) => "human_input",
            _ => "other",
        };
        let mut fired = self.fired.borrow_mut();
        for (i, (prefix, kind)) in self.script.iter().enumerate() {
            if !fired[i] && tag == *prefix {
                fired[i] = true;
                let point = match kind {
                    DecisionKind::Retrieve { .. } => DecisionPoint::Retrieve,
                    _ => DecisionPoint::Act,
                };
                return ControlDecision {
                    stamp: DecisionStamp {
                        decision_point: point,
                        owner: Owner::Model,
                        rationale_ref: None,
                    },
                    kind: kind.clone(),
                };
            }
        }
        drop(fired);
        self.inner.decide(state, cue)
    }
    fn restore(
        &mut self,
        checkpoint: &[u8],
        ctx: &ControlContext,
    ) -> Result<hh_control::state::ControlState, hh_control::strategy::RestoreError> {
        self.inner.restore(checkpoint, ctx)
    }
    fn terminate(
        &self,
        state: &hh_control::state::ControlState,
        reason: &StopReason,
    ) -> hh_control::strategy::FinalReport {
        self.inner.terminate(state, reason)
    }
}

/// A scripted `VerifyPort`.
struct ScriptedVerify {
    verdicts: Vec<hh_verification::validators::Verdict>,
}

impl VerifyPort for ScriptedVerify {
    fn verify(
        &mut self,
        _refs: &[String],
        _subject: &Json,
    ) -> Vec<hh_verification::validators::Verdict> {
        self.verdicts.clone()
    }
}

// ── fixtures ────────────────────────────────────────────────────────────

fn ctx() -> ControlContext {
    ControlContext {
        process_ref: "proc-1".into(),
        plan: vec![],
        boundary: hh_control::react::react_preset(),
        profile: Json::Null,
        account_ref: "acct".into(),
        budget_ref: "b-1".into(),
        envelope_ref: "env-1".into(),
        parameters: StrategyParams::default(),
        capabilities_available: vec!["hh.submit".into()],
        steering: (
            hh_control::strategy::SteerMode::Unsupported,
            hh_control::strategy::ConcurrentInput::QueueOnly,
        ),
    }
}

fn submit_surface() -> SurfaceSpec {
    SurfaceSpec {
        surface_id: "hh.submit".into(),
        semantic_id: "sem/hh.submit".into(),
        params: [(
            "text".into(),
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
    }
}

fn submit_call() -> ModelOutcome {
    ModelOutcome {
        stop_reason: hh_gateway::vocab::StopReason::ToolUse,
        response_ref: "r-1".into(),
        text_empty: false,
        calls: vec![ParsedCall {
            tool_call_id: "tc-submit".into(),
            surface: "hh.submit".into(),
            args_raw: r#"{"text":"done"}"#.into(),
        }],
        error_class: None,
        retry_after_ms: None,
    }
}

fn write_call() -> ModelOutcome {
    ModelOutcome {
        stop_reason: hh_gateway::vocab::StopReason::ToolUse,
        response_ref: "r-1".into(),
        text_empty: false,
        calls: vec![ParsedCall {
            tool_call_id: "tc-write".into(),
            surface: "fs.write".into(),
            args_raw: r#"{"path":"src/main.rs"}"#.into(),
        }],
        error_class: None,
        retry_after_ms: None,
    }
}

fn affirmative_verdict() -> hh_verification::validators::Verdict {
    use hh_verification::vocab::*;
    let judge_prov = hh_provenance::ProvenanceRecord::minted(
        hh_provenance::origin::Origin::model("model:judge-1", "run", "resp-j1"),
        hh_provenance::PersistenceScope::Run,
        0,
    );
    hh_verification::validators::Verdict {
        verdict_id: "verdict:artefact-1".into(),
        validator_ref: hh_identity::refs::VersionedRef::pinned(
            hh_identity::kinds::RecordKind::Validator,
            "validator:judge-1",
            judge_prov.clone(),
        ),
        oracle_class: OracleClass::Executable,
        target: "run".into(),
        criterion_ref: None,
        contract_id: None,
        phase: VerdictPhase::Completion,
        role: CriterionRole::Acceptance,
        value: VerdictValue::Bool(true),
        status: VerdictStatus::Decided,
        detector: Detector::Judged,
        evidence_refs: vec![],
        inputs_digest: "sha256:dd".into(),
        evidence_head_seq: 0,
        freshness_ok: true,
        findings: vec![],
        cost_ppm: 0,
        charged_to: ChargedTo::Subject,
        veto_tripped: vec![],
        bundle_id: None,
        calibration_ref: None,
        independence_summary: None,
        uncited_findings: 0,
        provenance: judge_prov,
        measured_at: 0,
    }
}

/// Open a scripted driver over a fresh `MemSink`.
fn scripted_driver(
    sink: &mut MemSink,
    script: Vec<(&'static str, DecisionKind)>,
    config: DriverConfig,
) -> Driver<ScriptedStrategy> {
    let policy = EnvelopePolicy::stage1_default("b-1").seal().unwrap();
    Driver::open(ScriptedStrategy::new(script), &ctx(), policy, sink, config).unwrap()
}

// ── the producer-sequence acceptance test (AC-1) ────────────────────────

/// A real run sequences `assemble` → `retrieve` → `compact` →
/// `mark_scope_ended`: the durable prefix shows `context.assembled`
/// under the model call, `context.retrieval.completed` +
/// `context.memory.read` after the `retrieve` decision,
/// `context.compaction.started`/`completed` after the `compact`
/// decision, and `context.memory.invalidated{fired_stamp: scope_ended}`
/// inside the closing turn — all minted by the driver's port calls, all
/// under the writer lease.
#[test]
fn producer_sequence_lands_durable_and_ordered() {
    let mut sink = MemSink::new();
    let mut memory = FakeMemory::new();
    let seeded = memory.seed("note-1", hh_provenance::PersistenceScope::Session, 1);
    // A run-scoped version — the `scope_ended` floor at finish expires it.
    let run_scoped = memory.seed("scratch-1", hh_provenance::PersistenceScope::Run, 1);
    let mut driver = scripted_driver(
        &mut sink,
        vec![
            (
                "model_completed",
                DecisionKind::Retrieve {
                    query: Json::obj([
                        ("kind", Json::str("by_name")),
                        ("scope", Json::str("session")),
                        ("name", Json::str("note-1")),
                    ]),
                },
            ),
            (
                "retrieval_completed",
                DecisionKind::Compact {
                    reason: "explicit".into(),
                },
            ),
        ],
        DriverConfig {
            surfaces: vec![submit_surface()],
            ..DriverConfig::default()
        },
    );
    driver.set_memory_port(Box::new(memory));
    // The compaction port runs the real `hh_context::compact` ladder —
    // the seeded plan's occupancy (400) exceeds the window cap (300), so
    // `request_principal` is a hard reclaim and `evict_oldest` applies.
    let (plan, cands) = compaction_fixture(
        vec![
            compact_cand(
                "c1",
                hh_context::CandidateKind::Observation,
                hh_provenance::AuthorityClass::Delegate,
                150,
                hh_context::PriorityClass::Commentary,
                1,
            ),
            compact_cand(
                "c2",
                hh_context::CandidateKind::Memory,
                hh_provenance::AuthorityClass::Delegate,
                150,
                hh_context::PriorityClass::Memory,
                2,
            ),
            compact_cand(
                "c3",
                hh_context::CandidateKind::TranscriptItem,
                hh_provenance::AuthorityClass::Delegate,
                100,
                hh_context::PriorityClass::TranscriptTail,
                3,
            ),
        ],
        400,
    );
    driver.set_compaction_port(Box::new(FakeCompaction {
        plan: Some(plan),
        candidates: cands,
        window_cap: 300,
        calls: 0,
    }));
    let mut model = ScriptedModel {
        script: [submit_call()].into_iter().collect(),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler {
        delivered: vec![],
        selected: None,
    };
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    // The run completes (submit → stop{completed}) — `Ok` or the
    // completed `run.finished` row is the terminal either way.
    let classes = sink.classes();
    assert!(
        classes.contains(&"lifecycle.run.finished"),
        "run did not finish: {classes:?} (r={r:?})"
    );
    // Ordering: assembled → retrieval.completed → memory.read →
    // compaction.started → compaction.completed → memory.invalidated →
    // turn.finished → run.finished — read off the durable prefix.
    let order = [
        "context.assembled",
        "context.retrieval.completed",
        "context.memory.read",
        "context.compaction.started",
        "context.compaction.completed",
        "context.memory.invalidated",
        "lifecycle.turn.finished",
        "lifecycle.run.finished",
    ];
    let positions: Vec<usize> = order.iter().map(|c| sink.pos(c)).collect();
    for w in positions.windows(2) {
        assert!(w[0] < w[1], "ordering violated: {order:?} @ {positions:?}");
    }
    // The retrieve read's `delivered[]` names the seeded version; the
    // `until_seq` is the store watermark the read served.
    let read = sink.find("context.memory.read")[0];
    let delivered = read
        .payload
        .get("delivered")
        .and_then(|d| match d {
            Json::Arr(a) => Some(
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    assert!(
        delivered.iter().any(|d| d == &seeded),
        "delivered missing seeded version {seeded}: {delivered:?}"
    );
    // `scope_ended` expired the run-scoped version — the session-scoped
    // one survives (session scope outlives the activation).
    let invalidated = sink.find("context.memory.invalidated");
    let fired: Vec<String> = invalidated
        .iter()
        .filter(|e| e.payload.get("fired_stamp").and_then(Json::as_str) == Some("scope_ended"))
        .filter_map(|e| {
            e.payload
                .get("version_id")
                .and_then(Json::as_str)
                .map(str::to_string)
        })
        .collect();
    assert!(
        fired.iter().any(|v| v == &run_scoped),
        "run-scoped {run_scoped} not expired: {fired:?}"
    );
    assert!(
        !fired.iter().any(|v| v == &seeded),
        "session-scoped {seeded} wrongly expired"
    );
    // Provenance — every producer row carries the kernel's provenance
    // record (the sink stamps it; nothing mints bare).
    for class in [
        "context.retrieval.completed",
        "context.memory.read",
        "context.compaction.completed",
        "context.memory.invalidated",
    ] {
        for e in sink.find(class) {
            assert!(
                e.provenance.is_some(),
                "{class} row has no provenance record"
            );
        }
    }
}

// ── resume_set consumption (AC-2) ───────────────────────────────────────

/// An armed `resume_set_heads` drains `by_name` through the memory
/// boundary *before the first `decide`* — the durable
/// `retrieval.completed`/`memory.read` rows precede every
/// `control.decision` in the prefix.
#[test]
fn resume_set_drains_before_first_decide() {
    let mut sink = MemSink::new();
    let mut memory = FakeMemory::new();
    let head = memory.seed("carried-note", hh_provenance::PersistenceScope::Session, 1);
    let mut driver = scripted_driver(
        &mut sink,
        vec![],
        DriverConfig {
            surfaces: vec![submit_surface()],
            resume_set_heads: vec!["carried-note".to_string()],
            ..DriverConfig::default()
        },
    );
    driver.set_memory_port(Box::new(memory));
    let mut model = ScriptedModel {
        script: [submit_call()].into_iter().collect(),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler {
        delivered: vec![],
        selected: None,
    };
    let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    let classes = sink.classes();
    let first_decision = classes.iter().position(|c| *c == "control.decision");
    let read_pos = sink.pos("context.memory.read");
    let rc_pos = sink.pos("context.retrieval.completed");
    assert!(
        first_decision
            .map(|d| read_pos < d && rc_pos < d)
            .unwrap_or(true),
        "resume_set read must precede the first decision: {classes:?}"
    );
    let read = sink.find("context.memory.read")[0];
    let delivered = read
        .payload
        .get("delivered")
        .map(|d| match d {
            Json::Arr(a) => a
                .iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect::<Vec<_>>(),
            _ => vec![],
        })
        .unwrap_or_default();
    assert!(
        delivered.iter().any(|d| d == &head),
        "resume_set head {head} not delivered: {delivered:?}"
    );
}

/// An armed `resume_set` without a memory boundary is a typed
/// `UnbackedPort{resume_set}` — never a silent skip.
#[test]
fn resume_set_without_port_refuses() {
    let mut sink = MemSink::new();
    let mut driver = scripted_driver(
        &mut sink,
        vec![],
        DriverConfig {
            surfaces: vec![submit_surface()],
            resume_set_heads: vec!["carried-note".to_string()],
            ..DriverConfig::default()
        },
    );
    let mut model = ScriptedModel {
        script: [submit_call()].into_iter().collect(),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler {
        delivered: vec![],
        selected: None,
    };
    let r = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    match r {
        Err(hh_control::driver::DriverError::UnbackedPort { kind }) => {
            assert_eq!(kind, "resume_set");
        }
        other => panic!("expected UnbackedPort{{resume_set}}, got {other:?}"),
    }
}

// ── the trigger leg (AC-R-2.4.3-12 on a real run) ───────────────────────

/// An observed effect whose intent names `args.path` runs the
/// `trigger{path_touched}` leg — the memory port's
/// `context.retrieval.completed{query_kind: trigger}` row lands durable,
/// scoped to the call that proposed the effect.
#[test]
fn trigger_path_touched_runs_retrieval() {
    let mut sink = MemSink::new();
    let mut memory = FakeMemory::new();
    // A `procedure_pointer` watching `src/**` — the trigger arm's hit.
    let prov = hh_provenance::ProvenanceRecord::kernel("hh-test/memory", 0);
    let draft = MemoryDraft {
        kind: MemoryKind::ProcedurePointer,
        subject_key: None,
        content: MemoryContent::Structured(Json::obj([(
            "procedure",
            Json::obj([
                ("id", Json::str("proc-watch-src")),
                (
                    "triggers",
                    Json::Arr(vec![Json::obj([("path_glob", Json::str("src/**"))])]),
                ),
            ]),
        )])),
        contract: Some(InvalidationContract {
            dependencies: vec![],
            cache_hint: CacheHint::Cacheable,
            validator_ref: None,
            freshness: None,
            invalidation_condition: Some(InvalidationCondition::ScopeEnded(
                hh_provenance::PersistenceScope::Session,
            )),
            revalidation: Revalidation::Never,
        }),
        scope: hh_provenance::PersistenceScope::Session,
        declared_inputs: vec![],
        justifications: vec![],
        supersedes: None,
        validity: None,
        provenance: Some(prov),
        semantic_id: Some("proc-watch-src".to_string()),
        validator_endorsed: false,
    };
    let wctx = hh_context::WriteContext {
        context_label: hh_provenance::label::Label::at(hh_provenance::AuthorityClass::Principal),
        lease_generation: memory.store.lease(hh_provenance::PersistenceScope::Session),
        at_seq: 1,
        run_id: "run".to_string(),
    };
    memory.store.put(draft, &wctx).expect("pointer put");
    let mut driver = scripted_driver(
        &mut sink,
        vec![],
        DriverConfig {
            surfaces: vec![
                SurfaceSpec {
                    surface_id: "fs.write".into(),
                    semantic_id: "sem/fs.write".into(),
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
                },
                submit_surface(),
            ],
            ..DriverConfig::default()
        },
    );
    driver.set_memory_port(Box::new(memory));
    let mut model = ScriptedModel {
        script: [write_call(), submit_call()].into_iter().collect(),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler {
        delivered: vec![],
        selected: None,
    };
    let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    let classes = sink.classes();
    let observed_pos = classes.iter().position(|c| *c == "action.effect.observed");
    let trig = sink
        .find("context.retrieval.completed")
        .into_iter()
        .find(|e| e.payload.get("query_kind").and_then(Json::as_str) == Some("trigger"));
    assert!(trig.is_some(), "no trigger retrieval row: {classes:?}");
    let trig_pos = classes
        .iter()
        .position(|c| *c == "context.retrieval.completed")
        .unwrap();
    assert!(
        observed_pos.map(|o| o < trig_pos).unwrap_or(false),
        "trigger row must follow the observed effect: {classes:?}"
    );
}

// ── the human detector leg (DF-S2.8-1 e) ────────────────────────────────

/// A principal's `artefact_mark` cue mints
/// `context.artefact.activated{detector: human}` — gated on the durable
/// `delivered` row; a mark against an undelivered id mints nothing.
#[test]
fn human_artefact_mark_mints_activated() {
    let mut sink = MemSink::new();
    let mut driver = scripted_driver(
        &mut sink,
        vec![],
        DriverConfig {
            surfaces: vec![submit_surface()],
            ..DriverConfig::default()
        },
    );
    driver.submit(Cue::HumanInput(HumanInput::ArtefactMark {
        artefact_id: "procedure_index:proc-1".into(),
        delivery_id: "del-1".into(),
        signal: "applied".into(),
    }));
    let mut model = ScriptedModel {
        script: [submit_call()].into_iter().collect(),
    };
    let mut gate = ScriptedGate;
    // The assembler delivers `del-1` — the mark's precondition.
    let mut asm = FakeAssembler {
        delivered: vec![(
            "context.artefact.delivered".into(),
            Json::obj([
                ("artefact_id", Json::str("procedure_index:proc-1")),
                ("delivery_id", Json::str("del-1")),
                ("kind", Json::str("procedure_index")),
                ("by_reference", Json::Bool(true)),
            ]),
        )],
        selected: None,
    };
    let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    let activated = sink.find("context.artefact.activated");
    let human = activated
        .iter()
        .find(|e| e.payload.get("detector").and_then(Json::as_str) == Some("human"));
    assert!(
        human.is_some(),
        "no human activated row: {:?}",
        sink.classes()
    );
    assert_eq!(
        human.unwrap().payload.get("signal").and_then(Json::as_str),
        Some("applied")
    );

    // Second run — a mark against an undelivered delivery mints nothing.
    let mut sink2 = MemSink::new();
    let mut driver2 = scripted_driver(
        &mut sink2,
        vec![],
        DriverConfig {
            surfaces: vec![submit_surface()],
            ..DriverConfig::default()
        },
    );
    driver2.submit(Cue::HumanInput(HumanInput::ArtefactMark {
        artefact_id: "procedure_index:nope".into(),
        delivery_id: "del-absent".into(),
        signal: "applied".into(),
    }));
    let mut model2 = ScriptedModel {
        script: [submit_call()].into_iter().collect(),
    };
    let mut asm2 = FakeAssembler {
        delivered: vec![],
        selected: None,
    };
    let _ = driver2.run(&mut model2, &mut gate, &mut asm2, &mut sink2);
    let human_rows = sink2
        .find("context.artefact.activated")
        .into_iter()
        .filter(|e| e.payload.get("detector").and_then(Json::as_str) == Some("human"))
        .count();
    assert_eq!(
        human_rows,
        0,
        "undelivered mark must not mint: {:?}",
        sink2.classes()
    );
}

// ── the judged detector leg (DF-S2.8-1 e) ───────────────────────────────

/// A `verify{subject: {kind: artefact_activation, …}}` decision with a
/// decided affirmative verdict mints `context.artefact.activated{detector:
/// judged, detector_ref, confidence_ppm, evidence_ref}` — and the row
/// never enters the deterministic `followed` fold.
#[test]
fn judged_verify_mints_judged_activation_not_deterministic() {
    let mut sink = MemSink::new();
    let mut driver = scripted_driver(
        &mut sink,
        vec![
            (
                "model_completed",
                DecisionKind::Verify {
                    validator_refs: vec!["validator:judge-1".into()],
                    subject: Json::obj([
                        ("kind", Json::str("artefact_activation")),
                        ("artefact_id", Json::str("procedure_index:proc-1")),
                        ("delivery_id", Json::str("del-1")),
                        ("signal", Json::str("judged")),
                    ]),
                },
            ),
            // A second model turn proposes a tool call — the deterministic
            // `followed` pass runs over it; the judged activation must NOT
            // mint a `verification.artefact.followed{detector:
            // deterministic}` (the row is `judged` evidence only).
            (
                "verification_completed",
                DecisionKind::Propose {
                    decision_point: DecisionPoint::Act,
                    context_request: Json::Null,
                    expected_output: hh_control::vocab::ExpectedOutput::Free,
                },
            ),
        ],
        DriverConfig {
            surfaces: vec![
                SurfaceSpec {
                    surface_id: "fs.write".into(),
                    semantic_id: "sem/fs.write".into(),
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
                },
                submit_surface(),
            ],
            ..DriverConfig::default()
        },
    );
    driver.set_verify_port(Box::new(ScriptedVerify {
        verdicts: vec![affirmative_verdict()],
    }));
    let mut memory = FakeMemory::new();
    memory.seed("proc-1", hh_provenance::PersistenceScope::Session, 1);
    driver.set_memory_port(Box::new(memory));
    let mut model = ScriptedModel {
        script: [write_call(), submit_call()].into_iter().collect(),
    };
    let mut gate = ScriptedGate;
    // The `del-1` delivery rides the assembled context (the judged
    // activation names it; `causes` carries it for the followed check).
    let mut asm = FakeAssembler {
        delivered: vec![(
            "context.artefact.delivered".into(),
            Json::obj([
                ("artefact_id", Json::str("procedure_index:proc-1")),
                ("delivery_id", Json::str("del-1")),
                ("kind", Json::str("procedure")),
                ("by_reference", Json::Bool(true)),
            ]),
        )],
        selected: None,
    };
    let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    let classes = sink.classes();
    let judged = sink
        .find("context.artefact.activated")
        .into_iter()
        .find(|e| e.payload.get("detector").and_then(Json::as_str) == Some("judged"));
    let judged = judged.unwrap_or_else(|| panic!("no judged activated row: {classes:?}"));
    assert_eq!(
        judged.payload.get("evidence_ref").and_then(Json::as_str),
        Some("verdict:artefact-1")
    );
    assert!(judged.payload.get("detector_ref").is_some());
    assert!(judged.payload.get("confidence_ppm").is_some());
    // The judged row never launders into deterministic followed evidence —
    // any `verification.artefact.followed` for `del-1` must carry
    // `detector: judged`, never `deterministic`.
    for e in sink.find("verification.artefact.followed") {
        if e.payload.get("delivery_id").and_then(Json::as_str) == Some("del-1") {
            assert_eq!(
                e.payload.get("detector").and_then(Json::as_str),
                Some("judged"),
                "a judged activation must never mint a deterministic followed"
            );
        }
    }
}

/// `verify{subject: {kind: artefact_followed, …}}` — the judged
/// `followed` leg mints `verification.artefact.followed{detector: judged}`
/// carrying the verdict's confidence, never `1_000_000`.
#[test]
fn judged_verify_mints_judged_followed() {
    let mut sink = MemSink::new();
    let mut driver = scripted_driver(
        &mut sink,
        vec![(
            "model_completed",
            DecisionKind::Verify {
                validator_refs: vec!["validator:judge-1".into()],
                subject: Json::obj([
                    ("kind", Json::str("artefact_followed")),
                    ("artefact_id", Json::str("procedure_index:proc-1")),
                    ("delivery_id", Json::str("del-1")),
                    ("artefact_kind", Json::str("procedure")),
                ]),
            },
        )],
        DriverConfig {
            surfaces: vec![submit_surface()],
            ..DriverConfig::default()
        },
    );
    let mut verdict = affirmative_verdict();
    verdict.value = hh_verification::vocab::VerdictValue::Graded(800_000);
    driver.set_verify_port(Box::new(ScriptedVerify {
        verdicts: vec![verdict],
    }));
    let mut model = ScriptedModel {
        script: [submit_call()].into_iter().collect(),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler {
        delivered: vec![(
            "context.artefact.delivered".into(),
            Json::obj([
                ("artefact_id", Json::str("procedure_index:proc-1")),
                ("delivery_id", Json::str("del-1")),
                ("kind", Json::str("procedure")),
                ("by_reference", Json::Bool(true)),
            ]),
        )],
        selected: None,
    };
    let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    let followed = sink
        .find("verification.artefact.followed")
        .into_iter()
        .find(|e| e.payload.get("detector").and_then(Json::as_str) == Some("judged"));
    let followed =
        followed.unwrap_or_else(|| panic!("no judged followed row: {:?}", sink.classes()));
    assert_eq!(
        followed
            .payload
            .get("confidence_ppm")
            .and_then(Json::as_int),
        Some(800_000)
    );
    assert_eq!(
        followed.payload.get("evidence_ref").and_then(Json::as_str),
        Some("verdict:artefact-1")
    );
}

/// A non-affirmative or inconclusive verdict mints nothing — a judged
/// absence is an absence, never a deterministic `false`.
#[test]
fn judged_inconclusive_mints_nothing() {
    let mut sink = MemSink::new();
    let mut verdict = affirmative_verdict();
    verdict.status = hh_verification::vocab::VerdictStatus::Inconclusive(
        hh_verification::vocab::InconclusiveReason::MissingEvidence,
    );
    let mut driver = scripted_driver(
        &mut sink,
        vec![(
            "model_completed",
            DecisionKind::Verify {
                validator_refs: vec!["validator:judge-1".into()],
                subject: Json::obj([
                    ("kind", Json::str("artefact_activation")),
                    ("artefact_id", Json::str("procedure_index:proc-1")),
                    ("delivery_id", Json::str("del-1")),
                    ("signal", Json::str("judged")),
                ]),
            },
        )],
        DriverConfig {
            surfaces: vec![submit_surface()],
            ..DriverConfig::default()
        },
    );
    driver.set_verify_port(Box::new(ScriptedVerify {
        verdicts: vec![verdict],
    }));
    let mut model = ScriptedModel {
        script: [submit_call()].into_iter().collect(),
    };
    let mut gate = ScriptedGate;
    let mut asm = FakeAssembler {
        delivered: vec![],
        selected: None,
    };
    let _ = driver.run(&mut model, &mut gate, &mut asm, &mut sink);
    // An inconclusive judged verdict mints no judged/human chain row — the
    // nudge `detector: deterministic` rows the guard fires are a separate
    // evidence class and stay out of this assertion's count.
    let judged_or_human = sink
        .find("context.artefact.activated")
        .into_iter()
        .filter(|e| {
            matches!(
                e.payload.get("detector").and_then(Json::as_str),
                Some("judged") | Some("human")
            )
        })
        .count();
    assert_eq!(
        judged_or_human,
        0,
        "inconclusive must not mint a judged/human row: {:?}",
        sink.classes()
    );
}
