//! The ledger-facts projection (spec §5h.2 §4; R-2.9.2; S3.3).
//!
//! `LedgerFacts` is the typed view of one run's event stream that the veto
//! predicates and the compliance/opacity metrics read. It is built from
//! `(seq, class, payload)` rows — the offline path supplies fixture rows, the
//! in-kernel path supplies the store's durable events. Every member records
//! *what the ledger says*; nothing is inferred (a missing decision row is a
//! fact, never a default).
//!
//! Event classes read (all registered in `hh_ledger::classes::CLASS_TABLE`):
//!
//! - `security.egress.requested` / `security.egress.decided` — the L4
//!   `benchmark_egress` veto's input.
//! - `action.environment.phase.changed` — the per-handle phase timeline
//!   (`setup | agent | verify | teardown`; the bench phase marks the L4
//!   window).
//! - `action.effect.intended` / `action.effect.committed` /
//!   `security.permission.decided` — `uncompensated_mutation` +
//!   `intervention_rate`.
//! - `context.artefact.delivered` / `context.artefact.activated` /
//!   `verification.artefact.followed` — the compliance chain (ADR-0045 D4).
//! - `context.assembled` — `opacity_dynamic`'s item mass.
//! - `verification.validator.verdict` — detector verdicts (`conformity`,
//!   grader schema/parse evidence).
//! - `measurement.metric.emitted` — emitted values.
//! - `measurement.cost.attributed` + `model.call.attempt.completed` —
//!   `attribution_completeness`.
//! - `measurement.evolution.candidate.transitioned` — the first
//!   `to: proposed` seq (the `LeakedSplit` ordering input).
//! - `lifecycle.run.created` / `lifecycle.run.finished` — the run's bounds.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_wire::Json;

use crate::json_util::*;

/// One `security.egress.*` row — the request plus the decision that settled it
/// (when one exists — an undecided request is a `benchmark_egress` trip).
#[derive(Debug, Clone, PartialEq)]
pub struct EgressRow {
    /// The ledger seq of the request.
    pub seq: u64,
    /// `request_ref` — joins `requested` to `decided`.
    pub request_ref: String,
    /// The env handle the request issued from.
    pub env_handle: Option<String>,
    /// `host_norm` — the normalized destination.
    pub host_norm: Option<String>,
    /// The `environment_phase` the run was in at `seq` (the timeline
    /// projection — `agent` is the L4-gated window).
    pub phase: Option<String>,
    /// `decided{decision}` — `Some(true)` allow / `Some(false)` deny /
    /// `None` undecided.
    pub decision: Option<bool>,
    /// The deciding rule (`rule_ref`) when the decision names one.
    pub rule_ref: Option<String>,
}

/// One environment-phase transition (`action.environment.phase.changed`).
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseChange {
    /// The ledger seq.
    pub seq: u64,
    /// The env handle.
    pub env_handle: Option<String>,
    /// The phase entered (`to`).
    pub to: String,
}

/// One `action.effect.*` row projection.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectRow {
    /// The effect id.
    pub effect_id: String,
    /// The declared `idempotency_key`.
    pub idempotency_key: Option<String>,
    /// `compensates` — the effect this row compensates/reverts, when set.
    pub compensates: Option<String>,
    /// `reverts` — the revert link, when set.
    pub reverts: Option<String>,
}

/// One `context.assembled` item — `{kind, artefact_id?, tokens}`.
#[derive(Debug, Clone, PartialEq)]
pub struct AssembledItem {
    /// The item kind (`artefact`/`artifact_excerpt` when the plan names an
    /// artefact; else the authority/`untyped` spelling).
    pub kind: String,
    /// The artefact the item delivers, when the plan names one.
    pub artefact_id: Option<String>,
    /// The item's token mass.
    pub tokens: i64,
}

/// One `context.artefact.*` delivery row.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtefactRow {
    /// The artefact id.
    pub artefact_id: String,
    /// The delivery id.
    pub delivery_id: Option<String>,
    /// The detector class that produced the row (followed rows).
    pub detector: Option<String>,
    /// `rule_id` — the profile rule the artefact belongs to, when the row
    /// names one (the `profile.rule.followed_rate` denominator key —
    /// AC-R-2.3.3-6).
    pub rule_id: Option<String>,
    /// `predicate_ref`/`followed_predicate_ref` — the predicate the followed
    /// verdict evaluated (the per-rule join key on followed rows).
    pub predicate_ref: Option<String>,
    /// `kind` — the artefact kind a `delivered` row names (`procedure_index`,
    /// `memory`, `memory_index`, `tool_surface`, `artifact_excerpt` …; the
    /// memory-compliance chain's per-kind join — AC-R-2.4.3-9).
    pub kind: Option<String>,
    /// `by_reference` — the handle-only delivery mark (delivered rows).
    pub by_reference: Option<bool>,
    /// `signal` — the activation signal an `activated` row carries
    /// (`loaded | cited | tool_used` — AC-R-2.4.3-9's spelling).
    pub signal: Option<String>,
    /// `evidence_ref` — the followed row's evidence pointer.
    pub evidence_ref: Option<String>,
}

/// One `context.memory.written` row projection (§5c.3 — the
/// `memory.promoted`/manifest-chain input).
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryWriteRow {
    /// The ledger seq.
    pub seq: u64,
    /// The version id.
    pub version_id: String,
    /// The semantic id (`memory_id`).
    pub semantic_id: Option<String>,
    /// The memory kind spelling.
    pub kind: Option<String>,
    /// The label's authority spelling (`promoted_endorsed` ⇒ promotion-
    /// endorsed for `memory.promoted`).
    pub authority: Option<String>,
}

/// One `context.memory.read` row projection (the delivered/withheld lists —
/// `memory.over_invalidation` and the reacquisition oracle's inputs).
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryReadRow {
    /// The ledger seq.
    pub seq: u64,
    /// The delivered version/address ids.
    pub delivered: Vec<String>,
    /// The withheld version ids.
    pub withheld: Vec<String>,
}

/// One `context.retrieval.completed` row projection.
#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalRow {
    /// The ledger seq.
    pub seq: u64,
    /// `query_kind`.
    pub query_kind: Option<String>,
    /// `report.ranker_ref`.
    pub ranker_ref: Option<String>,
    /// `report.deterministic`.
    pub deterministic: Option<bool>,
    /// `report.cost.index_ms.value`.
    pub index_ms: Option<i64>,
    /// `report.cost.embedder_calls`.
    pub embedder_calls: Option<i64>,
    /// `report.cost.tokens_estimated`.
    pub tokens_estimated: Option<i64>,
}

/// One `action.tool.completed` row projection (the `clear_tool_results`
/// join + the `repeated_action` oracle's `(capability, args)` key).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCompletedRow {
    /// The ledger seq.
    pub seq: u64,
    /// `invocation_ref` (the tool-call identity `Origin::Tool` names).
    pub invocation_ref: Option<String>,
    /// The capability/tool name.
    pub capability: Option<String>,
    /// The canonical-args digest (canonical JSON of `args`/`arguments`,
    /// when the row carries them).
    pub args_canonical: Option<String>,
}

/// One `context.compaction.started`/`completed` row projection.
#[derive(Debug, Clone, PartialEq)]
pub struct CompactionRow {
    /// The ledger seq.
    pub seq: u64,
    /// `completed` — `context.compaction.completed` vs `started`.
    pub completed: bool,
    /// `variant_ref` (completed rows).
    pub variant_ref: Option<String>,
    /// `status` (completed rows).
    pub status: Option<String>,
    /// `tokens_freed`/`reclaimed`.
    pub tokens_freed: Option<i64>,
    /// `accounting.model_calls` (the summariser-call count — 0 at C0).
    pub model_calls: Option<i64>,
    /// `forgotten[]` — the context_item_ids the compaction dropped (the
    /// `reacquisition`/`recall_probe` oracle input).
    pub forgotten: Vec<String>,
}

/// One `model.call.requested` row — the request-side cache members and the
/// call's `purpose` (the instrument/subject charging input — AC-R-2.3.1-10).
#[derive(Debug, Clone, PartialEq)]
pub struct CallRequest {
    /// The ledger seq.
    pub seq: u64,
    /// The call id.
    pub model_call_id: String,
    /// `cache.purpose` (`main | compaction | probe | judge | subagent(id)` …).
    pub purpose: Option<String>,
    /// `cache.expected_state` — the declared expectation (`warm | cold{…}`).
    pub expected_state: Option<String>,
    /// `cache.affinity_key` — the kernel-derived prefix affinity key.
    pub affinity_key: Option<String>,
}

/// The attempt lifecycle phase a `model.call.attempt.*` row records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptPhase {
    /// `model.call.attempt.started`.
    Started,
    /// `model.call.attempt.completed`.
    Completed,
    /// `model.call.attempt.failed`.
    Failed,
}

impl AttemptPhase {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AttemptPhase::Started => "started",
            AttemptPhase::Completed => "completed",
            AttemptPhase::Failed => "failed",
        }
    }
}

/// One `model.call.attempt.*` row — the span view (one span per attempt,
/// AC-R-2.3.1-10).
#[derive(Debug, Clone, PartialEq)]
pub struct AttemptRow {
    /// The ledger seq.
    pub seq: u64,
    /// The call id.
    pub model_call_id: String,
    /// `attempt_no`.
    pub attempt_no: Option<u64>,
    /// The lifecycle phase.
    pub phase: AttemptPhase,
}

/// One `model.call.completed` / `model.call.failed` terminal row.
#[derive(Debug, Clone, PartialEq)]
pub struct CallTerminal {
    /// The ledger seq.
    pub seq: u64,
    /// The call id.
    pub model_call_id: String,
    /// `failed` — the row is `model.call.failed`.
    pub failed: bool,
    /// `served_from_cache` present — a K4/K5 entry served the call
    /// (`timing = n/a{not_run}` beside; never a live attempt).
    pub served_from_cache: bool,
    /// `timing` is the `n/a{…}` string form (the K5-served stamp —
    /// AC-R-2.3.4-11: a cache hit never reports a live latency).
    pub timing_na: bool,
    /// `served_model` — the provider's served-model stamp (drift evidence).
    pub served_model: Option<String>,
    /// `cache_observation.cache_read` (canonical `view` tokens) — the K1
    /// observation a prefix-hit ratio reads.
    pub cache_read: Option<i64>,
    /// `usage.record.view.input_total` — the inclusive input total.
    pub input_total: Option<i64>,
}

/// One `model.route.decided` / `model.rerouted` row — the routing lineage a
/// charge must agree with (AC-R-2.3.2-3).
#[derive(Debug, Clone, PartialEq)]
pub struct RouteRow {
    /// The ledger seq.
    pub seq: u64,
    /// The call id.
    pub model_call_id: String,
    /// `true` on `model.rerouted` (`to`), `false` on `model.route.decided`
    /// (`selected` — the full `ModelRef` object).
    pub rerouted: bool,
    /// `selected` (`decided` — the `ModelRef` JSON) — absent on reroutes.
    pub selected: Option<Json>,
    /// `to` (`rerouted` — the provider model id) — absent on decisions.
    pub to: Option<String>,
    /// `deviation` — the decision's own flag.
    pub deviation: bool,
}

/// One `model.cache.resolved` row (one per K2–K6 lookup — hits and misses).
#[derive(Debug, Clone, PartialEq)]
pub struct CacheResolution {
    /// The ledger seq.
    pub seq: u64,
    /// `cache_kind` (`k1` … `k6` spellings as emitted).
    pub cache_kind: String,
    /// The lookup key (a content address).
    pub key: Option<String>,
    /// `outcome` (`hit | miss | withheld | stale_withheld` …).
    pub outcome: Option<String>,
    /// `reason` — the miss reason (`None` on a hit).
    pub reason: Option<String>,
    /// `avoided{…}` — the avoided-cost estimate, when the row carries one.
    pub avoided: Option<Json>,
    /// `attribution` (`subject | instrument` — instrument for probes).
    pub attribution: Option<String>,
    /// `purpose` — the purpose the lookup served.
    pub purpose: Option<String>,
    /// `served_by` — the call the resolution served, when the row names one.
    pub served_by: Option<String>,
}

/// One `control.budget.consumed` row — the charge view (AC-R-2.3.1-10,
/// AC-R-2.3.4-6).
#[derive(Debug, Clone, PartialEq)]
pub struct ChargeRow {
    /// The ledger seq.
    pub seq: u64,
    /// `dimension`.
    pub dimension: String,
    /// `amount`.
    pub amount: i64,
    /// `attribution.charged_to` (`subject | instrument`).
    pub charged_to: Option<String>,
    /// `attribution.cache.hit` — a zero-amount cache-hit charge.
    pub cache_hit: bool,
    /// `attribution.model_ref` — the model the charge prices (the
    /// `charge equals decision` check's left side; AC-R-2.3.2-3).
    pub model_ref: Option<Json>,
    /// `source_event.event_id` — the producing event.
    pub source_event_id: Option<String>,
    /// The `model_call_id` the charge joins to (resolved through
    /// `source_event.event_id → event_calls`; `None` when the projection
    /// cannot join — envelope ids are `from_envelopes`-only).
    pub model_call_id: Option<String>,
}

/// One `measurement.cost.attributed` row — the spend view.
#[derive(Debug, Clone, PartialEq)]
pub struct SpendRowFact {
    /// The ledger seq.
    pub seq: u64,
    /// `subject_ref` / `model_call_id` when the payload names one.
    pub subject_ref: Option<String>,
    /// `model_ref` — the priced model.
    pub model_ref: Option<Json>,
    /// `attribution.charged_to`.
    pub charged_to: Option<String>,
    /// `provenance` (`measured | estimated_from_pricing | …`).
    pub provenance: Option<String>,
    /// `money.micro_units`.
    pub micro_units: Option<i64>,
    /// The `model_call_id` the row joins to (payload member or
    /// `source_event` join).
    pub model_call_id: Option<String>,
}

/// One `verification.validator.verdict` row projection.
#[derive(Debug, Clone, PartialEq)]
pub struct VerdictRow {
    /// The validator ref.
    pub validator_ref: Option<String>,
    /// The oracle class spelling.
    pub oracle_class: Option<String>,
    /// The verdict status (`decided | inconclusive | oracle_failure`).
    pub status: String,
    /// The verdict value JSON (the `{kind, value}` form).
    pub value: Json,
    /// The detector spelling.
    pub detector: Option<String>,
    /// The verdict's declared verifier isolation, when carried.
    pub isolation: Option<String>,
    /// The verdict's `inputs_digest` (the evidence-binding the
    /// replayed-verification check recomputes).
    pub inputs_digest: Option<String>,
    /// The phase the verdict targets.
    pub phase: Option<String>,
}

/// The bench-grading evidence a `measurement.bench.graded`-style row or a
/// verifier verdict surfaces — kept separate so the infrastructure detector
/// reads one typed record (spec §5h.4's grading contract).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GradingEvidence {
    /// The grader's exit code, when the grade step completed.
    pub exit_code: Option<i64>,
    /// Whether the verifier ran inside its own materialization
    /// (`separate` — never inside the agent env).
    pub verifier_separate: Option<bool>,
    /// Whether the grading schema parsed.
    pub results_parsed: Option<bool>,
    /// The declared fail-to-pass checks the grader skipped.
    pub skipped_f2p: Vec<String>,
    /// Whether the suite's declared command actually executed.
    pub suite_executed: Option<bool>,
    /// Whether the run's declared delivery channel produced the artefact the
    /// grader consumed (`deliverable_missing` when false).
    pub deliverable_present: Option<bool>,
}

/// `ledger_facts/1` — one run's projected evaluation facts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LedgerFacts {
    /// `lifecycle.run.created` observed.
    pub run_created: bool,
    /// `lifecycle.run.finished` observed (with the stop-reason JSON).
    pub finished: Option<Json>,
    /// The egress rows (request + joined decision).
    pub egress: Vec<EgressRow>,
    /// The phase timeline.
    pub phases: Vec<PhaseChange>,
    /// `action.effect.intended` rows.
    pub effects_intended: Vec<EffectRow>,
    /// `action.effect.committed` rows.
    pub effects_committed: Vec<EffectRow>,
    /// The `effect_id`s `security.permission.decided` denied.
    pub permission_denials: BTreeSet<String>,
    /// `context.artefact.delivered` rows.
    pub artefacts_delivered: Vec<ArtefactRow>,
    /// `context.artefact.activated` rows.
    pub artefacts_activated: Vec<ArtefactRow>,
    /// `verification.artefact.followed` rows (carry the detector).
    pub artefacts_followed: Vec<ArtefactRow>,
    /// `context.assembled` items per call (`(model_call_id, items)`).
    pub assembled: Vec<(String, Vec<AssembledItem>)>,
    /// `verification.validator.verdict` rows.
    pub verdicts: Vec<VerdictRow>,
    /// `measurement.metric.emitted` values (decoded `MetricValue`s are on
    /// `EvalRun.values`; this is the raw count for audit).
    pub metrics_emitted: u64,
    /// The `model_call_id`s `measurement.cost.attributed` covered.
    pub cost_attributed_calls: BTreeSet<String>,
    /// The `model_call_id`s `model.call.attempt.completed` reports.
    pub model_calls_completed: BTreeSet<String>,
    /// The first `measurement.evolution.candidate.transitioned{to: proposed}`
    /// seq (the `LeakedSplit` ordering input).
    pub first_proposed_at: Option<u64>,
    /// The bench grading evidence (a verifier-side row the adapter emits —
    /// `measurement.bench.graded` or the `verdict` members it folds into).
    pub grading: GradingEvidence,
    /// `model.call.requested` rows keyed by `model_call_id` (S3.7 — the
    /// request-side `purpose`/cache members the accounting checks read).
    pub call_requests: BTreeMap<String, CallRequest>,
    /// `model.call.completed`/`model.call.failed` terminal rows.
    pub call_terminals: Vec<CallTerminal>,
    /// `model.call.attempt.*` rows — the attempt-span view.
    pub attempts: Vec<AttemptRow>,
    /// `model.route.decided`/`model.rerouted` rows (seq order).
    pub routes: Vec<RouteRow>,
    /// `model.cache.resolved` rows (one per lookup — hits and misses).
    pub cache_resolutions: Vec<CacheResolution>,
    /// `control.budget.consumed` rows — the charge view.
    pub charges: Vec<ChargeRow>,
    /// `measurement.cost.attributed` rows — the spend view (the
    /// `cost_attributed_calls` set above stays the coarse summary).
    pub spend_rows: Vec<SpendRowFact>,
    /// `event_id → model_call_id` for every `model.call.*`/`attempt` row —
    /// populated only under [`LedgerFacts::from_rows`]; the charge/spend
    /// join reads `source_event.event_id` through it. Empty under
    /// `from_events` (envelope ids are not carried there).
    pub event_calls: BTreeMap<String, String>,
    /// `context.memory.written` rows (S3.8 — the memory-compliance and
    /// `memory.promoted` inputs).
    pub memory_writes: Vec<MemoryWriteRow>,
    /// `context.memory.read` rows (delivered/withheld lists).
    pub memory_reads: Vec<MemoryReadRow>,
    /// `context.retrieval.completed` rows (the retrieval-cost and
    /// `reacquisition`/`recall_probe` inputs).
    pub retrievals: Vec<RetrievalRow>,
    /// `action.tool.completed` rows (the `clear_tool_results` and
    /// `repeated_action` inputs).
    pub tool_completions: Vec<ToolCompletedRow>,
    /// `context.compaction.started`/`completed` rows (the
    /// `harness_overhead.compaction` and forgotten-set inputs).
    pub compactions: Vec<CompactionRow>,
    /// `context.memory.invalidated` version ids + superseded predecessors —
    /// the `memory.over_invalidation` justification set.
    pub invalidated_ids: BTreeSet<String>,
}

fn s(j: &Json, member: &str) -> Option<String> {
    j.get(member).and_then(Json::as_str).map(str::to_string)
}

fn b(j: &Json, member: &str) -> Option<bool> {
    match j.get(member) {
        Some(Json::Bool(v)) => Some(*v),
        _ => None,
    }
}

fn effect_row(j: &Json) -> EffectRow {
    EffectRow {
        effect_id: s(j, "effect_id").unwrap_or_default(),
        idempotency_key: s(j, "idempotency_key"),
        compensates: s(j, "compensates"),
        reverts: s(j, "reverts"),
    }
}

fn artefact_row(j: &Json) -> ArtefactRow {
    ArtefactRow {
        artefact_id: s(j, "artefact_id").unwrap_or_default(),
        delivery_id: s(j, "delivery_id"),
        detector: s(j, "detector"),
        rule_id: s(j, "rule_id"),
        predicate_ref: s(j, "predicate_ref").or_else(|| s(j, "followed_predicate_ref")),
        kind: s(j, "kind"),
        by_reference: b(j, "by_reference"),
        signal: s(j, "signal"),
        evidence_ref: s(j, "evidence_ref"),
    }
}

fn str_list(j: &Json, member: &str) -> Vec<String> {
    match j.get(member) {
        Some(Json::Arr(v)) => v
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// One ledger row the projection reads — `(seq, event_id, class, payload)`.
/// `event_id` is `None` on the `from_events` path (the legacy tuple has no
/// envelope ids; charge/spend rows then join only through payload members).
#[derive(Debug, Clone, PartialEq)]
pub struct FactRow {
    /// The ledger seq.
    pub seq: u64,
    /// The envelope event id, when the caller carries it.
    pub event_id: Option<String>,
    /// The event class.
    pub class: String,
    /// The payload.
    pub payload: Json,
}

/// `p.member.sub` as an int.
fn si(j: &Json, member: &str, sub: &str) -> Option<i64> {
    j.get(member)
        .and_then(|o| o.get(sub))
        .and_then(Json::as_int)
}

/// `p.attribution` decoded to `(charged_to, cache_hit, model_ref)`.
fn attribution_parts(p: &Json) -> (Option<String>, bool, Option<Json>) {
    match p.get("attribution") {
        Some(a @ Json::Obj(_)) => (
            a.get("charged_to")
                .and_then(Json::as_str)
                .map(str::to_string),
            matches!(
                a.get("cache").and_then(|c| c.get("hit")),
                Some(Json::Bool(true))
            ),
            a.get("model_ref").cloned(),
        ),
        _ => (None, false, None),
    }
}

/// `source_event.event_id`.
fn source_event_id(p: &Json) -> Option<String> {
    p.get("source_event")
        .and_then(|e| e.get("event_id"))
        .and_then(Json::as_str)
        .map(str::to_string)
}

impl LedgerFacts {
    /// Project one run's `(seq, class, payload)` rows into the typed view.
    /// Unknown classes are skipped — the projection reads what it owns
    /// (unknown classes are refused at *append* by the store, not here).
    pub fn from_events(events: &[(u64, String, Json)]) -> LedgerFacts {
        LedgerFacts::from_rows(
            &events
                .iter()
                .map(|(seq, class, p)| FactRow {
                    seq: *seq,
                    event_id: None,
                    class: class.clone(),
                    payload: p.clone(),
                })
                .collect::<Vec<_>>(),
        )
    }

    /// Project `(seq, event_id, class, payload)` envelope rows — the
    /// charge/spend → call join (`control.budget.consumed.source_event` and
    /// `measurement.cost.attributed.source_event` name the *event* that
    /// produced the spend, not the call id) is live on this path.
    pub fn from_envelopes(events: &[(u64, String, String, Json)]) -> LedgerFacts {
        LedgerFacts::from_rows(
            &events
                .iter()
                .map(|(seq, id, class, p)| FactRow {
                    seq: *seq,
                    event_id: Some(id.clone()),
                    class: class.clone(),
                    payload: p.clone(),
                })
                .collect::<Vec<_>>(),
        )
    }

    /// The core projection (all paths funnel here).
    pub fn from_rows(events: &[FactRow]) -> LedgerFacts {
        let mut f = LedgerFacts::default();
        // Pass 0 — `event_id → model_call_id` for every model.call.* row
        // (the charge/spend join key).
        for r in events {
            if r.class.starts_with("model.call.") {
                if let (Some(id), Some(c)) = (&r.event_id, s(&r.payload, "model_call_id")) {
                    f.event_calls.insert(id.clone(), c);
                }
            }
        }
        // First pass — the phase timeline (egress rows join against it).
        for r in events {
            let (seq, class, p) = (r.seq, r.class.as_str(), &r.payload);
            if class == "action.environment.phase.changed" {
                f.phases.push(PhaseChange {
                    seq,
                    env_handle: s(p, "env_handle").or_else(|| s(p, "env_handle_id")),
                    to: s(p, "to").or_else(|| s(p, "phase")).unwrap_or_default(),
                });
            }
        }
        let phase_at = |seq: u64, handle: &Option<String>| -> Option<String> {
            f.phases
                .iter()
                .filter(|c| c.seq <= seq && (handle.is_none() || c.env_handle == *handle))
                .max_by_key(|c| c.seq)
                .map(|c| c.to.clone())
        };
        // Second pass — everything else.
        let mut decided: BTreeMap<String, (bool, Option<String>, u64)> = BTreeMap::new();
        for r in events {
            let (class, p) = (r.class.as_str(), &r.payload);
            if class == "security.egress.decided" {
                let decision = s(p, "decision").map(|d| d == "allow" || d == "allowed");
                if let (Some(r), Some(d)) = (s(p, "request_ref"), decision) {
                    decided.insert(r, (d, s(p, "rule_ref"), 0));
                }
            }
        }
        for r in events {
            let (seq, class, p) = (r.seq, r.class.as_str(), &r.payload);
            match class {
                "lifecycle.run.created" => f.run_created = true,
                "lifecycle.run.finished" => f.finished = Some(p.clone()),
                "security.egress.requested" => {
                    let request_ref = s(p, "request_ref").unwrap_or_default();
                    let env_handle = s(p, "env_handle");
                    let d = decided.get(&request_ref);
                    f.egress.push(EgressRow {
                        seq,
                        request_ref,
                        env_handle: env_handle.clone(),
                        host_norm: s(p, "host_norm"),
                        phase: phase_at(seq, &env_handle),
                        decision: d.map(|(v, _, _)| *v),
                        rule_ref: d.and_then(|(_, r, _)| r.clone()),
                    });
                }
                "action.effect.intended" => f.effects_intended.push(effect_row(p)),
                "action.effect.committed" => f.effects_committed.push(effect_row(p)),
                "security.permission.decided" => {
                    if s(p, "decision").as_deref() == Some("deny") {
                        if let Some(e) = s(p, "effect_id") {
                            f.permission_denials.insert(e);
                        }
                    }
                }
                "context.artefact.delivered" => f.artefacts_delivered.push(artefact_row(p)),
                "context.artefact.activated" => f.artefacts_activated.push(artefact_row(p)),
                "verification.artefact.followed" => f.artefacts_followed.push(artefact_row(p)),
                "context.assembled" => {
                    let call = s(p, "model_call_id").unwrap_or_default();
                    let mut items = Vec::new();
                    if let Some(Json::Arr(is)) = p.get("items") {
                        for it in is {
                            items.push(AssembledItem {
                                kind: s(it, "kind")
                                    .or_else(|| s(it, "authority"))
                                    .unwrap_or_else(|| "untyped".to_string()),
                                artefact_id: s(it, "artefact_id"),
                                tokens: it.get("tokens").and_then(Json::as_int).unwrap_or(0),
                            });
                        }
                    }
                    f.assembled.push((call, items));
                }
                "verification.validator.verdict" | "measurement.bench.graded" => {
                    if class == "measurement.bench.graded" {
                        f.grading.exit_code = p.get("exit_code").and_then(Json::as_int);
                        f.grading.verifier_separate = b(p, "verifier_separate");
                        f.grading.results_parsed = b(p, "results_parsed");
                        f.grading.suite_executed = b(p, "suite_executed");
                        f.grading.deliverable_present = b(p, "deliverable_present");
                        if let Some(Json::Arr(sk)) = p.get("skipped_f2p") {
                            f.grading.skipped_f2p = sk
                                .iter()
                                .filter_map(|x| x.as_str().map(str::to_string))
                                .collect();
                        }
                    } else {
                        f.verdicts.push(VerdictRow {
                            validator_ref: s(p, "validator_ref"),
                            oracle_class: s(p, "oracle_class"),
                            status: s(p, "status").unwrap_or_else(|| "decided".into()),
                            value: p
                                .get("value")
                                .or_else(|| p.get("verdict"))
                                .cloned()
                                .unwrap_or(Json::Null),
                            detector: s(p, "detector"),
                            isolation: s(p, "isolation"),
                            inputs_digest: s(p, "inputs_digest"),
                            phase: s(p, "phase"),
                        });
                        // A verifier verdict doubles as grading evidence when
                        // it carries the bench members.
                        if p.get("verifier_separate").is_some() {
                            f.grading.verifier_separate = b(p, "verifier_separate");
                        }
                    }
                }
                "measurement.metric.emitted" => f.metrics_emitted += 1,
                "measurement.cost.attributed" => {
                    let call = s(p, "subject_ref")
                        .or_else(|| s(p, "model_call_id"))
                        .or_else(|| {
                            source_event_id(p).and_then(|id| f.event_calls.get(&id).cloned())
                        });
                    if let Some(id) = &call {
                        f.cost_attributed_calls.insert(id.clone());
                    }
                    let (charged_to, _, _) = attribution_parts(p);
                    f.spend_rows.push(SpendRowFact {
                        seq,
                        subject_ref: s(p, "subject_ref"),
                        model_ref: p.get("model_ref").cloned(),
                        charged_to,
                        provenance: s(p, "provenance"),
                        micro_units: si(p, "money", "micro_units"),
                        model_call_id: call,
                    });
                }
                "model.call.attempt.completed" => {
                    if let Some(id) = s(p, "model_call_id").or_else(|| s(p, "call_id")) {
                        f.model_calls_completed.insert(id.clone());
                    }
                    f.attempts.push(AttemptRow {
                        seq,
                        model_call_id: s(p, "model_call_id").unwrap_or_default(),
                        attempt_no: p.get("attempt_no").and_then(Json::as_int).map(|v| v as u64),
                        phase: AttemptPhase::Completed,
                    });
                }
                "model.call.attempt.started" | "model.call.attempt.failed" => {
                    f.attempts.push(AttemptRow {
                        seq,
                        model_call_id: s(p, "model_call_id").unwrap_or_default(),
                        attempt_no: p.get("attempt_no").and_then(Json::as_int).map(|v| v as u64),
                        phase: if class == "model.call.attempt.started" {
                            AttemptPhase::Started
                        } else {
                            AttemptPhase::Failed
                        },
                    });
                }
                "model.call.requested" => {
                    if let Some(id) = s(p, "model_call_id") {
                        let cache = p.get("cache").cloned().unwrap_or(Json::Null);
                        f.call_requests.insert(
                            id.clone(),
                            CallRequest {
                                seq,
                                model_call_id: id,
                                purpose: s(&cache, "purpose").or_else(|| s(p, "purpose")),
                                expected_state: s(&cache, "expected_state"),
                                affinity_key: s(&cache, "affinity_key"),
                            },
                        );
                    }
                }
                "model.call.completed" | "model.call.failed" => {
                    f.call_terminals.push(CallTerminal {
                        seq,
                        model_call_id: s(p, "model_call_id").unwrap_or_default(),
                        failed: class == "model.call.failed",
                        served_from_cache: p.get("served_from_cache").is_some(),
                        timing_na: matches!(
                            p.get("timing"),
                            Some(Json::Str(t)) if t.starts_with("n/a{")
                        ),
                        served_model: s(p, "served_model"),
                        cache_read: si(p, "cache_observation", "cache_read"),
                        input_total: p
                            .get("usage")
                            .and_then(|u| u.get("record"))
                            .and_then(|r| r.get("view"))
                            .and_then(|v| v.get("input_total"))
                            .and_then(Json::as_int),
                    });
                }
                "model.route.decided" | "model.rerouted" => {
                    f.routes.push(RouteRow {
                        seq,
                        model_call_id: s(p, "model_call_id").unwrap_or_default(),
                        rerouted: class == "model.rerouted",
                        selected: p.get("selected").cloned(),
                        to: s(p, "to"),
                        deviation: b(p, "deviation").unwrap_or(false),
                    });
                }
                "model.cache.resolved" => {
                    f.cache_resolutions.push(CacheResolution {
                        seq,
                        cache_kind: s(p, "cache_kind").unwrap_or_default(),
                        key: s(p, "key"),
                        outcome: s(p, "outcome"),
                        reason: s(p, "reason"),
                        avoided: p
                            .get("avoided")
                            .cloned()
                            .filter(|a| !matches!(a, Json::Null)),
                        attribution: s(p, "attribution"),
                        purpose: s(p, "purpose"),
                        served_by: s(p, "served_by"),
                    });
                }
                "control.budget.consumed" => {
                    let (charged_to, cache_hit, model_ref) = attribution_parts(p);
                    let src = source_event_id(p);
                    f.charges.push(ChargeRow {
                        seq,
                        dimension: s(p, "dimension").unwrap_or_default(),
                        amount: p.get("amount").and_then(Json::as_int).unwrap_or(0),
                        charged_to,
                        cache_hit,
                        model_ref,
                        model_call_id: src
                            .as_ref()
                            .and_then(|id| f.event_calls.get(id).cloned())
                            .or_else(|| s(p, "model_call_id")),
                        source_event_id: src,
                    });
                }
                "context.memory.written" => {
                    let auth = p
                        .get("label")
                        .and_then(|l| l.get("authority"))
                        .and_then(Json::as_str)
                        .map(str::to_string);
                    f.memory_writes.push(MemoryWriteRow {
                        seq,
                        version_id: s(p, "version_id").unwrap_or_default(),
                        semantic_id: s(p, "memory_id"),
                        kind: s(p, "kind"),
                        authority: auth,
                    });
                    if let Some(prev) = p
                        .get("supersedes")
                        .and_then(|x| x.get("version_id"))
                        .and_then(Json::as_str)
                    {
                        f.invalidated_ids.insert(prev.to_string());
                    }
                }
                "context.memory.invalidated" => {
                    if let Some(v) = s(p, "version_id") {
                        f.invalidated_ids.insert(v);
                    }
                }
                "context.memory.read" => {
                    f.memory_reads.push(MemoryReadRow {
                        seq,
                        delivered: str_list(p, "delivered"),
                        withheld: match p.get("withheld") {
                            Some(Json::Arr(ws)) => ws
                                .iter()
                                .filter_map(|w| {
                                    w.as_str().map(str::to_string).or_else(|| {
                                        w.get("version_id")
                                            .or_else(|| w.get("item_id"))
                                            .or_else(|| w.get("candidate_id"))
                                            .and_then(Json::as_str)
                                            .map(str::to_string)
                                    })
                                })
                                .collect(),
                            _ => Vec::new(),
                        },
                    });
                }
                "context.retrieval.completed" => {
                    let report = p.get("report").cloned().unwrap_or(Json::Null);
                    f.retrievals.push(RetrievalRow {
                        seq,
                        query_kind: s(p, "query_kind"),
                        ranker_ref: s(&report, "ranker_ref"),
                        deterministic: b(&report, "deterministic"),
                        index_ms: report
                            .get("cost")
                            .and_then(|c| c.get("index_ms"))
                            .and_then(|m| m.get("value"))
                            .and_then(Json::as_int),
                        embedder_calls: report
                            .get("cost")
                            .and_then(|c| c.get("embedder_calls"))
                            .and_then(Json::as_int),
                        tokens_estimated: report
                            .get("cost")
                            .and_then(|c| c.get("tokens_estimated"))
                            .and_then(Json::as_int),
                    });
                }
                "action.tool.completed" => {
                    let args = p
                        .get("args")
                        .or_else(|| p.get("arguments"))
                        .map(|a| a.to_canonical_string());
                    f.tool_completions.push(ToolCompletedRow {
                        seq,
                        invocation_ref: s(p, "invocation_ref")
                            .or_else(|| s(p, "tool_call_id"))
                            .or_else(|| s(p, "call_id")),
                        capability: s(p, "capability")
                            .or_else(|| s(p, "tool_name"))
                            .or_else(|| s(p, "tool")),
                        args_canonical: args,
                    });
                }
                "context.compaction.started" | "context.compaction.completed" => {
                    let completed = class == "context.compaction.completed";
                    f.compactions.push(CompactionRow {
                        seq,
                        completed,
                        variant_ref: s(p, "variant_ref"),
                        status: s(p, "status"),
                        tokens_freed: p
                            .get("tokens_freed")
                            .or_else(|| p.get("reclaimed"))
                            .and_then(Json::as_int),
                        model_calls: p
                            .get("accounting")
                            .and_then(|a| a.get("model_calls"))
                            .and_then(Json::as_int),
                        forgotten: str_list(p, "forgotten"),
                    });
                }
                "measurement.evolution.candidate.transitioned" => {
                    if s(p, "to").as_deref() == Some("proposed") {
                        f.first_proposed_at = Some(f.first_proposed_at.map_or(seq, |e| e.min(seq)));
                    }
                }
                _ => {}
            }
        }
        f
    }

    /// The canonical JSON form (`ledger_facts/1`).
    pub fn to_json(&self) -> Json {
        let egress: Vec<Json> = self
            .egress
            .iter()
            .map(|e| {
                let mut m = BTreeMap::new();
                m.insert("seq".into(), Json::Int(e.seq as i64));
                m.insert("request_ref".into(), Json::str(&e.request_ref));
                insert_opt(&mut m, "env_handle", e.env_handle.as_deref().map(Json::str));
                insert_opt(&mut m, "host_norm", e.host_norm.as_deref().map(Json::str));
                insert_opt(&mut m, "phase", e.phase.as_deref().map(Json::str));
                insert_opt(&mut m, "decision", e.decision.map(Json::Bool));
                insert_opt(&mut m, "rule_ref", e.rule_ref.as_deref().map(Json::str));
                Json::Obj(m)
            })
            .collect();
        let effect = |e: &EffectRow| {
            let mut m = BTreeMap::new();
            m.insert("effect_id".into(), Json::str(&e.effect_id));
            insert_opt(
                &mut m,
                "idempotency_key",
                e.idempotency_key.as_deref().map(Json::str),
            );
            insert_opt(
                &mut m,
                "compensates",
                e.compensates.as_deref().map(Json::str),
            );
            insert_opt(&mut m, "reverts", e.reverts.as_deref().map(Json::str));
            Json::Obj(m)
        };
        let artefact = |a: &ArtefactRow| {
            let mut m = BTreeMap::new();
            m.insert("artefact_id".into(), Json::str(&a.artefact_id));
            insert_opt(
                &mut m,
                "delivery_id",
                a.delivery_id.as_deref().map(Json::str),
            );
            insert_opt(&mut m, "detector", a.detector.as_deref().map(Json::str));
            insert_opt(&mut m, "rule_id", a.rule_id.as_deref().map(Json::str));
            insert_opt(
                &mut m,
                "predicate_ref",
                a.predicate_ref.as_deref().map(Json::str),
            );
            insert_opt(&mut m, "kind", a.kind.as_deref().map(Json::str));
            insert_opt(&mut m, "by_reference", a.by_reference.map(Json::Bool));
            insert_opt(&mut m, "signal", a.signal.as_deref().map(Json::str));
            insert_opt(
                &mut m,
                "evidence_ref",
                a.evidence_ref.as_deref().map(Json::str),
            );
            Json::Obj(m)
        };
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str("ledger_facts/1"));
        m.insert("run_created".into(), Json::Bool(self.run_created));
        insert_opt(&mut m, "finished", self.finished.clone());
        m.insert("egress".into(), Json::Arr(egress));
        m.insert(
            "phases".into(),
            Json::Arr(
                self.phases
                    .iter()
                    .map(|c| {
                        let mut pm = BTreeMap::new();
                        pm.insert("seq".into(), Json::Int(c.seq as i64));
                        pm.insert("to".into(), Json::str(&c.to));
                        insert_opt(
                            &mut pm,
                            "env_handle",
                            c.env_handle.as_deref().map(Json::str),
                        );
                        Json::Obj(pm)
                    })
                    .collect(),
            ),
        );
        m.insert(
            "effects_intended".into(),
            Json::Arr(self.effects_intended.iter().map(effect).collect()),
        );
        m.insert(
            "effects_committed".into(),
            Json::Arr(self.effects_committed.iter().map(effect).collect()),
        );
        m.insert(
            "permission_denials".into(),
            Json::Arr(self.permission_denials.iter().map(Json::str).collect()),
        );
        m.insert(
            "artefacts_delivered".into(),
            Json::Arr(self.artefacts_delivered.iter().map(artefact).collect()),
        );
        m.insert(
            "artefacts_activated".into(),
            Json::Arr(self.artefacts_activated.iter().map(artefact).collect()),
        );
        m.insert(
            "artefacts_followed".into(),
            Json::Arr(self.artefacts_followed.iter().map(artefact).collect()),
        );
        m.insert(
            "assembled".into(),
            Json::Arr(
                self.assembled
                    .iter()
                    .map(|(call, items)| {
                        Json::obj([
                            ("model_call_id", Json::str(call)),
                            (
                                "items",
                                Json::Arr(
                                    items
                                        .iter()
                                        .map(|i| {
                                            let mut im = BTreeMap::new();
                                            im.insert("kind".into(), Json::str(&i.kind));
                                            if let Some(a) = &i.artefact_id {
                                                im.insert("artefact_id".into(), Json::str(a));
                                            }
                                            im.insert("tokens".into(), Json::Int(i.tokens));
                                            Json::Obj(im)
                                        })
                                        .collect(),
                                ),
                            ),
                        ])
                    })
                    .collect(),
            ),
        );
        m.insert(
            "verdicts".into(),
            Json::Arr(
                self.verdicts
                    .iter()
                    .map(|v| {
                        let mut vm = BTreeMap::new();
                        insert_opt(
                            &mut vm,
                            "validator_ref",
                            v.validator_ref.as_deref().map(Json::str),
                        );
                        insert_opt(
                            &mut vm,
                            "oracle_class",
                            v.oracle_class.as_deref().map(Json::str),
                        );
                        vm.insert("status".into(), Json::str(&v.status));
                        vm.insert("value".into(), v.value.clone());
                        insert_opt(&mut vm, "detector", v.detector.as_deref().map(Json::str));
                        insert_opt(&mut vm, "isolation", v.isolation.as_deref().map(Json::str));
                        insert_opt(
                            &mut vm,
                            "inputs_digest",
                            v.inputs_digest.as_deref().map(Json::str),
                        );
                        insert_opt(&mut vm, "phase", v.phase.as_deref().map(Json::str));
                        Json::Obj(vm)
                    })
                    .collect(),
            ),
        );
        m.insert(
            "metrics_emitted".into(),
            Json::Int(self.metrics_emitted as i64),
        );
        m.insert(
            "cost_attributed_calls".into(),
            Json::Arr(self.cost_attributed_calls.iter().map(Json::str).collect()),
        );
        m.insert(
            "model_calls_completed".into(),
            Json::Arr(self.model_calls_completed.iter().map(Json::str).collect()),
        );
        // ── S3.7 model-plane members ────────────────────────────────────
        m.insert(
            "call_requests".into(),
            Json::Obj(
                self.call_requests
                    .iter()
                    .map(|(id, r)| {
                        let mut rm = BTreeMap::new();
                        rm.insert("seq".into(), Json::Int(r.seq as i64));
                        insert_opt(&mut rm, "purpose", r.purpose.as_deref().map(Json::str));
                        insert_opt(
                            &mut rm,
                            "expected_state",
                            r.expected_state.as_deref().map(Json::str),
                        );
                        insert_opt(
                            &mut rm,
                            "affinity_key",
                            r.affinity_key.as_deref().map(Json::str),
                        );
                        (id.clone(), Json::Obj(rm))
                    })
                    .collect(),
            ),
        );
        m.insert(
            "call_terminals".into(),
            Json::Arr(
                self.call_terminals
                    .iter()
                    .map(|t| {
                        let mut tm = BTreeMap::new();
                        tm.insert("seq".into(), Json::Int(t.seq as i64));
                        tm.insert("model_call_id".into(), Json::str(&t.model_call_id));
                        tm.insert("failed".into(), Json::Bool(t.failed));
                        tm.insert("served_from_cache".into(), Json::Bool(t.served_from_cache));
                        tm.insert("timing_na".into(), Json::Bool(t.timing_na));
                        insert_opt(
                            &mut tm,
                            "served_model",
                            t.served_model.as_deref().map(Json::str),
                        );
                        insert_opt(&mut tm, "cache_read", t.cache_read.map(Json::Int));
                        insert_opt(&mut tm, "input_total", t.input_total.map(Json::Int));
                        Json::Obj(tm)
                    })
                    .collect(),
            ),
        );
        m.insert(
            "attempts".into(),
            Json::Arr(
                self.attempts
                    .iter()
                    .map(|a| {
                        let mut am = BTreeMap::new();
                        am.insert("seq".into(), Json::Int(a.seq as i64));
                        am.insert("model_call_id".into(), Json::str(&a.model_call_id));
                        insert_opt(
                            &mut am,
                            "attempt_no",
                            a.attempt_no.map(|n| Json::Int(n as i64)),
                        );
                        am.insert("phase".into(), Json::str(a.phase.as_str()));
                        Json::Obj(am)
                    })
                    .collect(),
            ),
        );
        m.insert(
            "routes".into(),
            Json::Arr(
                self.routes
                    .iter()
                    .map(|r| {
                        let mut rm = BTreeMap::new();
                        rm.insert("seq".into(), Json::Int(r.seq as i64));
                        rm.insert("model_call_id".into(), Json::str(&r.model_call_id));
                        rm.insert("rerouted".into(), Json::Bool(r.rerouted));
                        insert_opt(&mut rm, "selected", r.selected.clone());
                        insert_opt(&mut rm, "to", r.to.as_deref().map(Json::str));
                        rm.insert("deviation".into(), Json::Bool(r.deviation));
                        Json::Obj(rm)
                    })
                    .collect(),
            ),
        );
        m.insert(
            "cache_resolutions".into(),
            Json::Arr(
                self.cache_resolutions
                    .iter()
                    .map(|r| {
                        let mut rm = BTreeMap::new();
                        rm.insert("seq".into(), Json::Int(r.seq as i64));
                        rm.insert("cache_kind".into(), Json::str(&r.cache_kind));
                        insert_opt(&mut rm, "key", r.key.as_deref().map(Json::str));
                        insert_opt(&mut rm, "outcome", r.outcome.as_deref().map(Json::str));
                        insert_opt(&mut rm, "reason", r.reason.as_deref().map(Json::str));
                        insert_opt(&mut rm, "avoided", r.avoided.clone());
                        insert_opt(
                            &mut rm,
                            "attribution",
                            r.attribution.as_deref().map(Json::str),
                        );
                        insert_opt(&mut rm, "purpose", r.purpose.as_deref().map(Json::str));
                        insert_opt(&mut rm, "served_by", r.served_by.as_deref().map(Json::str));
                        Json::Obj(rm)
                    })
                    .collect(),
            ),
        );
        m.insert(
            "charges".into(),
            Json::Arr(
                self.charges
                    .iter()
                    .map(|c| {
                        let mut cm = BTreeMap::new();
                        cm.insert("seq".into(), Json::Int(c.seq as i64));
                        cm.insert("dimension".into(), Json::str(&c.dimension));
                        cm.insert("amount".into(), Json::Int(c.amount));
                        insert_opt(
                            &mut cm,
                            "charged_to",
                            c.charged_to.as_deref().map(Json::str),
                        );
                        cm.insert("cache_hit".into(), Json::Bool(c.cache_hit));
                        insert_opt(&mut cm, "model_ref", c.model_ref.clone());
                        insert_opt(
                            &mut cm,
                            "source_event_id",
                            c.source_event_id.as_deref().map(Json::str),
                        );
                        insert_opt(
                            &mut cm,
                            "model_call_id",
                            c.model_call_id.as_deref().map(Json::str),
                        );
                        Json::Obj(cm)
                    })
                    .collect(),
            ),
        );
        m.insert(
            "spend_rows".into(),
            Json::Arr(
                self.spend_rows
                    .iter()
                    .map(|r| {
                        let mut rm = BTreeMap::new();
                        rm.insert("seq".into(), Json::Int(r.seq as i64));
                        insert_opt(
                            &mut rm,
                            "subject_ref",
                            r.subject_ref.as_deref().map(Json::str),
                        );
                        insert_opt(&mut rm, "model_ref", r.model_ref.clone());
                        insert_opt(
                            &mut rm,
                            "charged_to",
                            r.charged_to.as_deref().map(Json::str),
                        );
                        insert_opt(
                            &mut rm,
                            "provenance",
                            r.provenance.as_deref().map(Json::str),
                        );
                        insert_opt(&mut rm, "micro_units", r.micro_units.map(Json::Int));
                        insert_opt(
                            &mut rm,
                            "model_call_id",
                            r.model_call_id.as_deref().map(Json::str),
                        );
                        Json::Obj(rm)
                    })
                    .collect(),
            ),
        );
        if !self.event_calls.is_empty() {
            m.insert(
                "event_calls".into(),
                Json::Obj(
                    self.event_calls
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::str(v)))
                        .collect(),
                ),
            );
        }
        insert_opt(
            &mut m,
            "first_proposed_at",
            self.first_proposed_at.map(|s| Json::Int(s as i64)),
        );
        let g = &self.grading;
        let mut gm = BTreeMap::new();
        insert_opt(&mut gm, "exit_code", g.exit_code.map(Json::Int));
        insert_opt(
            &mut gm,
            "verifier_separate",
            g.verifier_separate.map(Json::Bool),
        );
        insert_opt(&mut gm, "results_parsed", g.results_parsed.map(Json::Bool));
        insert_opt(&mut gm, "suite_executed", g.suite_executed.map(Json::Bool));
        insert_opt(
            &mut gm,
            "deliverable_present",
            g.deliverable_present.map(Json::Bool),
        );
        if !g.skipped_f2p.is_empty() {
            gm.insert(
                "skipped_f2p".into(),
                Json::Arr(g.skipped_f2p.iter().map(Json::str).collect()),
            );
        }
        m.insert("grading".into(), Json::Obj(gm));
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<LedgerFacts, SchemaError> {
        const REC: &str = "ledger_facts/1";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "schema",
                "run_created",
                "finished",
                "egress",
                "phases",
                "effects_intended",
                "effects_committed",
                "permission_denials",
                "artefacts_delivered",
                "artefacts_activated",
                "artefacts_followed",
                "assembled",
                "verdicts",
                "metrics_emitted",
                "cost_attributed_calls",
                "model_calls_completed",
                "first_proposed_at",
                "grading",
                "call_requests",
                "call_terminals",
                "attempts",
                "routes",
                "cache_resolutions",
                "charges",
                "spend_rows",
                "event_calls",
            ],
            REC,
        )?;
        let mut f = LedgerFacts {
            run_created: opt_bool_at(m, "run_created")?.unwrap_or(false),
            finished: m.get("finished").cloned(),
            ..LedgerFacts::default()
        };
        if let Some(Json::Arr(es)) = m.get("egress") {
            for e in es {
                f.egress.push(EgressRow {
                    seq: e.get("seq").and_then(Json::as_int).unwrap_or(0) as u64,
                    request_ref: s(e, "request_ref").unwrap_or_default(),
                    env_handle: s(e, "env_handle"),
                    host_norm: s(e, "host_norm"),
                    phase: s(e, "phase"),
                    decision: b(e, "decision"),
                    rule_ref: s(e, "rule_ref"),
                });
            }
        }
        if let Some(Json::Arr(ps)) = m.get("phases") {
            for c in ps {
                f.phases.push(PhaseChange {
                    seq: c.get("seq").and_then(Json::as_int).unwrap_or(0) as u64,
                    env_handle: s(c, "env_handle"),
                    to: s(c, "to").unwrap_or_default(),
                });
            }
        }
        let effect = |e: &Json| EffectRow {
            effect_id: s(e, "effect_id").unwrap_or_default(),
            idempotency_key: s(e, "idempotency_key"),
            compensates: s(e, "compensates"),
            reverts: s(e, "reverts"),
        };
        let artefact = |a: &Json| ArtefactRow {
            artefact_id: s(a, "artefact_id").unwrap_or_default(),
            delivery_id: s(a, "delivery_id"),
            detector: s(a, "detector"),
            rule_id: s(a, "rule_id"),
            predicate_ref: s(a, "predicate_ref"),
            kind: s(a, "kind"),
            by_reference: b(a, "by_reference"),
            signal: s(a, "signal"),
            evidence_ref: s(a, "evidence_ref"),
        };
        if let Some(Json::Arr(es)) = m.get("effects_intended") {
            f.effects_intended = es.iter().map(effect).collect();
        }
        if let Some(Json::Arr(es)) = m.get("effects_committed") {
            f.effects_committed = es.iter().map(effect).collect();
        }
        if let Some(Json::Arr(ds)) = m.get("permission_denials") {
            f.permission_denials = ds
                .iter()
                .filter_map(|d| d.as_str().map(str::to_string))
                .collect();
        }
        if let Some(Json::Arr(as_)) = m.get("artefacts_delivered") {
            f.artefacts_delivered = as_.iter().map(artefact).collect();
        }
        if let Some(Json::Arr(as_)) = m.get("artefacts_activated") {
            f.artefacts_activated = as_.iter().map(artefact).collect();
        }
        if let Some(Json::Arr(as_)) = m.get("artefacts_followed") {
            f.artefacts_followed = as_.iter().map(artefact).collect();
        }
        if let Some(Json::Arr(as_)) = m.get("assembled") {
            for a in as_ {
                let call = s(a, "model_call_id").unwrap_or_default();
                let items = a
                    .get("items")
                    .and_then(|v| match v {
                        Json::Arr(is) => Some(is),
                        _ => None,
                    })
                    .map(|is| {
                        is.iter()
                            .map(|i| AssembledItem {
                                kind: s(i, "kind").unwrap_or_else(|| "untyped".into()),
                                artefact_id: s(i, "artefact_id"),
                                tokens: i.get("tokens").and_then(Json::as_int).unwrap_or(0),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                f.assembled.push((call, items));
            }
        }
        if let Some(Json::Arr(vs)) = m.get("verdicts") {
            for v in vs {
                f.verdicts.push(VerdictRow {
                    validator_ref: s(v, "validator_ref"),
                    oracle_class: s(v, "oracle_class"),
                    status: s(v, "status").unwrap_or_else(|| "decided".into()),
                    value: v.get("value").cloned().unwrap_or(Json::Null),
                    detector: s(v, "detector"),
                    isolation: s(v, "isolation"),
                    inputs_digest: s(v, "inputs_digest"),
                    phase: s(v, "phase"),
                });
            }
        }
        f.metrics_emitted = opt_int_at(m, "metrics_emitted")?.unwrap_or(0) as u64;
        if let Some(Json::Arr(cs)) = m.get("cost_attributed_calls") {
            f.cost_attributed_calls = cs
                .iter()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect();
        }
        if let Some(Json::Arr(cs)) = m.get("model_calls_completed") {
            f.model_calls_completed = cs
                .iter()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect();
        }
        f.first_proposed_at = opt_int_at(m, "first_proposed_at")?.map(|x| x as u64);
        // ── S3.7 model-plane members ────────────────────────────────────
        if let Some(Json::Obj(rs)) = m.get("call_requests") {
            for (id, r) in rs {
                f.call_requests.insert(
                    id.clone(),
                    CallRequest {
                        seq: r.get("seq").and_then(Json::as_int).unwrap_or(0) as u64,
                        model_call_id: id.clone(),
                        purpose: s(r, "purpose"),
                        expected_state: s(r, "expected_state"),
                        affinity_key: s(r, "affinity_key"),
                    },
                );
            }
        }
        if let Some(Json::Arr(ts)) = m.get("call_terminals") {
            for t in ts {
                f.call_terminals.push(CallTerminal {
                    seq: t.get("seq").and_then(Json::as_int).unwrap_or(0) as u64,
                    model_call_id: s(t, "model_call_id").unwrap_or_default(),
                    failed: b(t, "failed").unwrap_or(false),
                    served_from_cache: b(t, "served_from_cache").unwrap_or(false),
                    timing_na: b(t, "timing_na").unwrap_or(false),
                    served_model: s(t, "served_model"),
                    cache_read: t.get("cache_read").and_then(Json::as_int),
                    input_total: t.get("input_total").and_then(Json::as_int),
                });
            }
        }
        if let Some(Json::Arr(as_)) = m.get("attempts") {
            for a in as_ {
                f.attempts.push(AttemptRow {
                    seq: a.get("seq").and_then(Json::as_int).unwrap_or(0) as u64,
                    model_call_id: s(a, "model_call_id").unwrap_or_default(),
                    attempt_no: a.get("attempt_no").and_then(Json::as_int).map(|v| v as u64),
                    phase: match s(a, "phase").as_deref() {
                        Some("started") => AttemptPhase::Started,
                        Some("failed") => AttemptPhase::Failed,
                        _ => AttemptPhase::Completed,
                    },
                });
            }
        }
        if let Some(Json::Arr(rs)) = m.get("routes") {
            for r in rs {
                f.routes.push(RouteRow {
                    seq: r.get("seq").and_then(Json::as_int).unwrap_or(0) as u64,
                    model_call_id: s(r, "model_call_id").unwrap_or_default(),
                    rerouted: b(r, "rerouted").unwrap_or(false),
                    selected: r.get("selected").cloned(),
                    to: s(r, "to"),
                    deviation: b(r, "deviation").unwrap_or(false),
                });
            }
        }
        if let Some(Json::Arr(rs)) = m.get("cache_resolutions") {
            for r in rs {
                f.cache_resolutions.push(CacheResolution {
                    seq: r.get("seq").and_then(Json::as_int).unwrap_or(0) as u64,
                    cache_kind: s(r, "cache_kind").unwrap_or_default(),
                    key: s(r, "key"),
                    outcome: s(r, "outcome"),
                    reason: s(r, "reason"),
                    avoided: r.get("avoided").cloned(),
                    attribution: s(r, "attribution"),
                    purpose: s(r, "purpose"),
                    served_by: s(r, "served_by"),
                });
            }
        }
        if let Some(Json::Arr(cs)) = m.get("charges") {
            for c in cs {
                f.charges.push(ChargeRow {
                    seq: c.get("seq").and_then(Json::as_int).unwrap_or(0) as u64,
                    dimension: s(c, "dimension").unwrap_or_default(),
                    amount: c.get("amount").and_then(Json::as_int).unwrap_or(0),
                    charged_to: s(c, "charged_to"),
                    cache_hit: b(c, "cache_hit").unwrap_or(false),
                    model_ref: c.get("model_ref").cloned(),
                    source_event_id: s(c, "source_event_id"),
                    model_call_id: s(c, "model_call_id"),
                });
            }
        }
        if let Some(Json::Arr(rs)) = m.get("spend_rows") {
            for r in rs {
                f.spend_rows.push(SpendRowFact {
                    seq: r.get("seq").and_then(Json::as_int).unwrap_or(0) as u64,
                    subject_ref: s(r, "subject_ref"),
                    model_ref: r.get("model_ref").cloned(),
                    charged_to: s(r, "charged_to"),
                    provenance: s(r, "provenance"),
                    micro_units: r.get("micro_units").and_then(Json::as_int),
                    model_call_id: s(r, "model_call_id"),
                });
            }
        }
        if let Some(Json::Obj(ec)) = m.get("event_calls") {
            for (k, v) in ec {
                if let Some(id) = v.as_str() {
                    f.event_calls.insert(k.clone(), id.to_string());
                }
            }
        }
        if let Some(g) = m.get("grading") {
            f.grading = GradingEvidence {
                exit_code: g.get("exit_code").and_then(Json::as_int),
                verifier_separate: b(g, "verifier_separate"),
                results_parsed: b(g, "results_parsed"),
                suite_executed: b(g, "suite_executed"),
                deliverable_present: b(g, "deliverable_present"),
                skipped_f2p: g
                    .get("skipped_f2p")
                    .and_then(|v| match v {
                        Json::Arr(sk) => Some(sk),
                        _ => None,
                    })
                    .map(|sk| {
                        sk.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
            };
        }
        Ok(f)
    }
}
