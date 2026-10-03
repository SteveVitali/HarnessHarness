//! `Dispatcher` — the seven kernel-owned stages `resolve → authorize →
//! prepare → commit → execute → capture → observe` (§5d.5 §2; ADR-0100/0101/
//! 0102). The dispatcher maps the stages one-to-one onto the ADR-0030 effect
//! lifecycle (`intended → authorized → prepared → committed → observed`,
//! `refused`/`unknown`/`probed` for the failure paths) and is the *only* code
//! that sequences them — the executor reports, never decides (I-3).
//!
//! Mediation is complete: `committed` is refused by the ledger unless a
//! `security.permission.decided{allow}` for `(effect_id, attempt)` precedes it
//! (the gate is durable — the ledger enforces it, not the dispatcher's
//! discipline). Dedup is two-tier: the kernel's `DedupStore` (idempotency key
//! → effect) prevents redispatch; a lapsed executor window is `probe`d, never
//! re-run.
//!
//! Redaction is on the **capture path** — every captured payload is masked
//! through `hh-secrets`' `DetectorSet` before it enters the manifest or the
//! raw-output blob (ADR-0101 D5), so a hostile executor's output is masked
//! before any visibility.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_compiler::equiv::SurfaceBinding;
use hh_compiler::plan::PinnedRef;
use hh_containment::admit::{admit_input, floor_gate, workspace_scope};
use hh_context::k4::{self, K4Cache, K4Entry, K4Key, K4Resolution};
use hh_context::memory::WriteContext;
use hh_hir::kinds::{EffectClass, EffectDomain};
use hh_hir::records::{Grant, ScopeBindings, ToolCapabilityRecord};
use hh_hir::risk::project_risk;
use hh_ledger::effect::idempotency_key;
use hh_ledger::errors::LedgerError;
use hh_ledger::fault::KillPoint;
use hh_ledger::store::{Lease, Store};
use hh_monitor::args::CanonicalArgs;
use hh_monitor::assess::{kernel_assessed, AssessmentInputs, ParseOutcome, Tri};
use hh_monitor::decision::Decision;
use hh_monitor::events::decided_payload;
use hh_monitor::monitor::{FlowInputs, Monitor, Proposal};
use hh_ontology::risk::{RiskClass, RiskReversibility};
use hh_provenance::flow;
use hh_provenance::origin::Origin;
use hh_provenance::{Label, PersistenceScope, ProvenanceRecord};
use hh_secrets::redact::{redact, DetectorSet};
use hh_wire::json::Json;

use crate::capture::{
    CaptureItem, CaptureKind, CaptureSource, EffectCaptureManifest, OutputPolicy, Truncation,
};
use crate::deadline::DeadlineLadder;
use crate::driver::EnvDriver;
use crate::errors::EnvError;
use crate::events::{self, EventMinter, ScopeChain};
use crate::executor::{
    ExecutionRequest, ExecutorDeclaration, ExecutorSignal, ProbeVerdict, TerminalReport,
    TerminalStatus, ToolExecutor,
};
use crate::observe::{
    apply_hint, retryable, EffectOutcome, ErrorClass, ErrorOrigin, Observation, ObservedStatus,
};
use crate::snapshot::PathBaseline;
use crate::tokens::{ResolveOutcome, TokenMinter, TokenResolver};

/// `ReservationSource` — the `prepare` stage's reserve seam (ADR-0040 D3 —
/// reserve-before-spend). The caller implements it on `hh-budget`'s `Account`
/// (`Account::reserve`); the dispatcher only needs the reservation id.
pub trait ReservationSource {
    /// `reserve(budget_id, quantity, holder, ttl_ms) → reservation_id`.
    fn reserve(
        &mut self,
        budget_id: &str,
        quantity: &hh_budget::quantity::ResourceVector,
        holder: &str,
        ttl_ms: u64,
    ) -> Result<String, String>;
}

/// `ReserveRequest` — the `prepare` reservation the dispatcher performs when
/// the effect declares a budget draw.
#[derive(Debug, Clone)]
pub struct ReserveRequest {
    /// The budget node.
    pub budget_id: String,
    /// The quantity.
    pub quantity: hh_budget::quantity::ResourceVector,
    /// The holder (the effect/proposer).
    pub holder: String,
    /// The reservation TTL (ms).
    pub ttl_ms: u64,
}

/// `DedupStore` — the kernel's idempotency-key → effect map (the first dedup
/// tier; an executor-side store is the second, C1). A replayed key resolves
/// to the prior effect — never a redispatch.
#[derive(Debug, Default)]
pub struct DedupStore {
    /// `idempotency_key → effect_id`.
    seen: BTreeMap<String, String>,
}

impl DedupStore {
    /// `lookup(key)` — the prior effect id, if the key was dispatched.
    pub fn lookup(&self, key: &str) -> Option<&str> {
        self.seen.get(key).map(String::as_str)
    }

    /// `record(key, effect_id)` — bind the key at `prepared`.
    pub fn record(&mut self, key: &str, effect_id: &str) {
        self.seen.insert(key.to_string(), effect_id.to_string());
    }
}

/// `DispatchInput` — everything the seven stages consume for one tool call.
/// The tool plane produces `binding`/`scope_bindings`/`capability` (the
/// resolved `check_callable` result); the kernel supplies the rest.
pub struct DispatchInput<'a> {
    /// The identity chain (`turn ⊃ model_call ⊃ tool_call`).
    pub chain: ScopeChain,
    /// The call ordinal (the `effect_id` derivation input).
    pub ordinal: u64,
    /// The proposing `AgentProcess` semantic id.
    pub proposer: String,
    /// The surface binding (arg-map eval input).
    pub binding: &'a SurfaceBinding,
    /// The capability's `scope_bindings`.
    pub scope_bindings: &'a ScopeBindings,
    /// The resolved capability record.
    pub capability: &'a ToolCapabilityRecord,
    /// The pinned capability ref.
    pub capability_ref: PinnedRef,
    /// The surface args.
    pub surface_args: Json,
    /// The args' provenance.
    pub args_provenance: ProvenanceRecord,
    /// The kernel-stamped context label.
    pub context_label: Label,
    /// The model's risk self-report (raise-only).
    pub self_report: Option<RiskClass>,
    /// The environment handle.
    pub env_handle_id: String,
    /// The declared effect class.
    pub declared: EffectClass,
    /// The requested grants (spawn/delegate).
    pub requested_grants: Vec<Grant>,
    /// The deadline ladder.
    pub ladder: DeadlineLadder,
    /// The output policy.
    pub output_policy: OutputPolicy,
    /// The `prepare` reservation, when the effect draws on a budget.
    pub reserve: Option<ReserveRequest>,
    /// A compensation plan id, when the effect is `compensable`.
    pub compensation_plan_id: Option<String>,
    /// A baseline ref, when the effect is `reversible` (the `prepared`
    /// requirement) — taken via `path_baseline` when absent.
    pub baseline_ref: Option<String>,
    /// The C2 flow inputs (§5g.2; R-2.8.2) — the kernel-stamped
    /// per-parameter labels (`L(args)`'s per-parameter form), the
    /// shape-endorsement flags D-ROBUST reads, the committed-effect
    /// projection and the recorded detector verdicts. `Default` for
    /// capabilities without a `flow_contract` — the flow stage still runs
    /// on a declared contract (its checks degrade to `EvaluationError`
    /// where a required label is unrecorded).
    pub flow: FlowInputs,
}

/// `DispatchOutcome` — what `dispatch` settled to.
#[derive(Debug)]
pub enum DispatchOutcome {
    /// `observed` — the terminal `Observation`.
    Observed(Box<Observation>),
    /// `refused` at `authorize` (the monitor's deny) or `resolve` (an
    /// arg-map / containment refusal).
    Refused {
        /// The closed reason.
        reason: String,
    },
    /// `unknown{cause}` — the outcome is unknowable; the caller runs `probe`
    /// (never a silent redispatch).
    Unknown {
        /// The closed `UNKNOWN_CAUSES` tag.
        cause: String,
    },
    /// A dedup hit — the key resolved to a prior effect; no dispatch ran.
    Duplicate {
        /// The prior effect id.
        prior_effect_id: String,
        /// The stored `action.effect.observed` payload when the prior
        /// effect reached a terminal (`dedup_support ≠ none` serves the
        /// recorded observation — §5a.2's second-attempt rule; AC-R-2.2.2-1).
        stored_observation: Option<Json>,
    },
    /// `suspended` — the ask is a durable `security.permission.pending` and
    /// the run suspended `awaiting_approval` behind a `permission_decided`
    /// wakeup subscription (§5a.3; R-2.8.7 `defer`/human stage). The caller
    /// surfaces the request; `respond` mints `decided`, the occurrence fires
    /// and the run resumes — the lease or the recorded decision then serves
    /// the re-dispatched effect.
    Suspended {
        /// The owed permission (the durable `pending` row).
        permission_id: String,
    },
    /// `paused` — the protocol edge answered `input_required` (§5d.4 D3;
    /// ADR-0097 D3). The effect stays `prepared`/`committed` — *never*
    /// `observed`, never `failed` — the ledger records
    /// `control.decision{kind: ask}` and opens the `message_human`
    /// elicitation effect; `resume_paused` re-executes under the same
    /// `effect_id` at `attempt_no + 1` echoing `request_state` unmodified.
    /// Distinct from `Suspended` (a kernel *permission* ask — the run
    /// defers behind a durable subscription) and from `Unknown`.
    Paused {
        /// The paused effect's id.
        effect_id: String,
        /// The opened elicitation effect's id — the derived
        /// `f(run, mc, {tc}.elicitation.{attempt}, 0)` coordinate (a
        /// `message_human` tool call under its own tool_call scope).
        elicitation_effect_id: String,
        /// The opaque resume payload — echoed to the edge unmodified.
        request_state: Json,
        /// The edge's `inputRequests` (the elicitation payload).
        input_requests: Json,
    },
    /// `observed` — served from the K4 tool-result cache (§5b.4; R-2.3.4¹):
    /// no executor ran, `action.tool.started`/`committed` are absent by
    /// construction, and the ledger carries `model.cache.resolved{hit}` plus
    /// the ordinary `prepared`/`observed`/`completed` terminals. The served
    /// provenance is `origin = cache(entry_ref)` at the entry's label —
    /// never raised (I-NOAUTH's cache face).
    ObservedCached {
        /// The observation rebuilt from the entry's recorded value.
        observation: Box<Observation>,
        /// The `Memory{kind: tool_result}` version served.
        entry_ref: String,
        /// `origin = cache(entry_ref)`, `derived_from = [entry_ref]`,
        /// authority/taint/readers copied from the entry's label.
        provenance: Box<ProvenanceRecord>,
        /// The entry's `recorded_usage` — the `avoided` report's input.
        avoided: Option<Json>,
    },
}

/// The `environment_ref` coordinate spelling K4 keys and epoch stamps use —
/// `"{semantic_id}:{version_id}"` of the bound environment record.
fn env_ref_str(handle: &crate::handle::EnvHandle) -> String {
    format!("{}:{}", handle.environment_ref.0, handle.environment_ref.1)
}

/// `Dispatcher` — the seven-stage pipeline over `store` + `driver` +
/// `monitor` + `executor`.
pub struct Dispatcher<'a> {
    store: &'a mut Store,
    run_id: String,
    monitor: &'a Monitor,
    /// The kernel's token minter (the only one — CF-213).
    minter: TokenMinter,
    /// The kernel dedup store.
    dedup: DedupStore,
    /// The redaction detectors (the capture path's mask set).
    detectors: DetectorSet,
    /// The K4 tool-result cache (`Memory{kind: tool_result}` over the run's
    /// `MemoryStore` — S2.10). `None` ⇒ no lookup, no write, no epoch
    /// tracking — the pre-S2.10 pipeline verbatim.
    cache: Option<K4Cache>,
    /// The conditioned `diff_sanity` rule — check (d) of the kernel local
    /// checks (ADR-0111 D1(d); S2.11). `Some` runs it under its conditioned
    /// thresholds with the assumption-debt record riding the verdict;
    /// `None` ablates it (T-LCD-02 — ablation is explicit, never silent).
    diff_sanity: Option<hh_verification::validators::DiffSanityRule>,
    /// The `TimeoutPolicy[permission]` deadline in ms (§5e.2; S2.11):
    /// `None` = `deadline none` — `attended` permission waits have no
    /// default deadline (bounded by `hard_max` at the caller's sweep).
    /// `Some(ms)` is recorded on the `pending` row (`ApprovalRequest.timeout`)
    /// and bounds `expire_permission`.
    permission_timeout_ms: Option<u64>,
    /// The armed kill point (R-2.2.3⁰ᶜ; ADR-0132 §4) — the battery's
    /// `inject(kill_point, target)`: armed once, fires once at the named
    /// stage boundary. Production code never arms it.
    fault: Option<KillPoint>,
}

impl<'a> Dispatcher<'a> {
    /// A dispatcher over `store`/`monitor` for `run_id`, with the token
    /// minter's `seed` and the redaction `detectors`.
    pub fn new(
        store: &'a mut Store,
        monitor: &'a Monitor,
        run_id: &str,
        seed: [u8; 32],
        detectors: DetectorSet,
    ) -> Self {
        Dispatcher {
            store,
            run_id: run_id.to_string(),
            monitor,
            minter: TokenMinter::new(seed),
            dedup: DedupStore::default(),
            detectors,
            cache: None,
            diff_sanity: Some(hh_verification::validators::DiffSanityRule::kernel_default(
                &ProvenanceRecord::kernel("hir/kernel/diff_sanity", 0),
            )),
            permission_timeout_ms: None,
            fault: None,
        }
    }

    /// Arm the dispatch-boundary kill point (R-2.2.3⁰ᶜ) — the next stage
    /// boundary `at` names returns `FaultInjected`: every durable row
    /// before it is already down, nothing after lands (the battery's
    /// `inject` seam; KP-9 lives on `Store::inject_durability_faults`,
    /// KP-13 on `arm_restore_kill`, KP-15 is the executor's own death —
    /// the `executor_error` path, no seam needed).
    pub fn inject_kill_point(&mut self, at: KillPoint) {
        self.fault = Some(at);
    }

    /// Fire the armed fault when it names one of `at` (consumes the arm —
    /// a fault fires once).
    fn check_kill(&mut self, at: &[KillPoint]) -> Result<(), EnvError> {
        if let Some(k) = self.fault {
            if at.contains(&k) {
                self.fault = None;
                return Err(EnvError::FaultInjected {
                    at: k.as_str().to_string(),
                });
            }
        }
        Ok(())
    }

    /// The durable-side dedup consult — the `effects_by_key` projection's
    /// raw fold (§5a.2; ADR-0102 §10): a key the in-memory `DedupStore`
    /// lost (a dispatcher rebuilt from the WAL alone after restore)
    /// still vetoes the second commit — the ledger is the sole durable
    /// source of truth, the in-memory store a hot cache over it.
    fn durable_key_lookup(&self, key: &str) -> Option<String> {
        self.store
            .effect_folds(&self.run_id)
            .ok()?
            .iter()
            .find(|(_, f)| f.idempotency_key.as_deref() == Some(key))
            .map(|(id, _)| id.clone())
    }

    /// Rebuild the in-memory `DedupStore` from the durable prefix — the
    /// post-restore warm path (the durable consult covers a cold
    /// dispatcher regardless; this keeps the hot cache honest).
    pub fn rebuild_dedup(&mut self) -> Result<(), EnvError> {
        for (id, f) in self.store.effect_folds(&self.run_id)?.iter() {
            if let Some(k) = &f.idempotency_key {
                self.dedup.record(k, id);
            }
        }
        Ok(())
    }

    /// Set the `TimeoutPolicy[permission]` deadline in ms (`None` = attended
    /// `deadline none` — the audited default; §5e.2).
    pub fn set_permission_timeout(&mut self, timeout_ms: Option<u64>) {
        self.permission_timeout_ms = timeout_ms;
    }

    /// Attach the run's K4 cache (`set_cache` before the first dispatch —
    /// the epoch pins are the run's, never cross-run).
    pub fn set_cache(&mut self, cache: K4Cache) {
        self.cache = Some(cache);
    }

    /// Set the conditioned `diff_sanity` rule (`None` ablates check (d) —
    /// the Lab's ablation seam; T-LCD-02).
    pub fn set_diff_sanity(&mut self, rule: Option<hh_verification::validators::DiffSanityRule>) {
        self.diff_sanity = rule;
    }

    /// The attached K4 cache, if any.
    pub fn cache(&self) -> Option<&K4Cache> {
        self.cache.as_ref()
    }

    /// Detach and return the cache (the caller unwraps the `MemoryStore`
    /// back out at run end).
    pub fn take_cache(&mut self) -> Option<K4Cache> {
        self.cache.take()
    }

    /// The token resolver (the helper/executor's read view).
    pub fn resolver(&self) -> TokenResolver {
        self.minter.resolver()
    }

    /// The dedup store (for the AC test's inspection).
    pub fn dedup(&self) -> &DedupStore {
        &self.dedup
    }

    /// The store (tests fold the WAL through it).
    pub fn store_mut(&mut self) -> &mut Store {
        self.store
    }

    fn minter_ev(&self) -> EventMinter<'_> {
        EventMinter::new(self.store, &self.run_id)
    }

    /// `dispatch(driver, executor, reserve_src, input, lease) →
    /// DispatchOutcome` — the seven stages. Every ledger append is
    /// kernel-origin; a ledger refusal surfaces as the typed `EnvError`/
    /// `LedgerError`.
    pub fn dispatch(
        &mut self,
        driver: &mut EnvDriver,
        executor: &mut dyn ToolExecutor,
        mut reserve_src: Option<&mut dyn ReservationSource>,
        input: &DispatchInput,
        lease: &Lease,
    ) -> Result<DispatchOutcome, EnvError> {
        // ── 1 resolve ──────────────────────────────────────────────────────
        // The per-use environment gate (DF-S1.12-3): a non-ready handle or a
        // stale report refuses before any work.
        let handle = driver
            .handle(&input.env_handle_id)
            .ok_or_else(|| EnvError::Unavailable {
                env_handle_id: input.env_handle_id.clone(),
                state: "missing",
            })?
            .clone();
        handle.verify_environment()?;

        // The effect coordinate is derivable before any stage — the resume
        // leg needs it: an effect left `intended` by a suspended ask (the
        // `awaiting_approval` defer) re-enters at `authorize`. `proposed`/
        // `intended` already opened the scopes — re-minting them would
        // double-open (`ScopeNotOpen` on the close side).
        let effect_id = Store::effect_id(
            &self.run_id,
            &input.chain.model_call_id,
            &input.chain.tool_call_id,
            input.ordinal,
        );
        let resumed = self
            .store
            .effect_folds(&self.run_id)?
            .iter()
            .any(|(id, f)| id == &effect_id && f.phase == hh_ledger::effect::EffectPhase::Intended);

        // `action.tool.proposed` opens the tool_call scope. It is minted
        // before the arg-map eval so a `resolve`-stage refusal can close the
        // scope it opens — `action.tool.rejected` is a `tool_call` closer, and
        // the ledger's scope rules refuse a close of a scope that was never
        // opened (`ScopeNotOpen`). The proposal exists the moment the kernel
        // accepts the dispatch input, independent of how the args resolve.
        let mut proposed = self.minter_ev().mint(
            "action.tool.proposed",
            events::tool_proposed_payload(
                &input.capability_ref.semantic_id,
                &input.capability_ref.version_id,
            ),
        )?;
        proposed.scope = hh_ledger::event::Scope {
            turn_id: Some(input.chain.turn_id.clone()),
            model_call_id: Some(input.chain.model_call_id.clone()),
            tool_call_id: Some(input.chain.tool_call_id.clone()),
            ..Default::default()
        };

        // The arg-map eval — `UnmappedArgument`/`UnscopedParameter` is a
        // `SurfaceFailure` detected before dispatch: no executor runs, no
        // `Effect` exists, and the row is `action.tool.surface_rejected` with
        // the typed `failure_class` (§5d.2; ADR-0092 D5; AC-R-2.5.2-7's run
        // half — S3.9). No effect scope exists yet (the effect coordinate is
        // derived from the canonical args), so the row carries the
        // `turn ⊃ model_call ⊃ tool_call` chain only; the call bytes are
        // content-addressed by `raw_call_hash`, never ledgered (ADR-0066
        // Rule C).
        let canonical = match hh_monitor::args::eval(
            input.binding,
            input.scope_bindings,
            &input.surface_args,
        ) {
            Ok(c) => c,
            Err(e) => {
                // `UnscopedParameter` — a scope-bearing parameter outside the
                // declared bindings — is `domain_violation` (ADR-0286 D3).
                let failure_class = match &e {
                    hh_monitor::args::ArgError::UnmappedArgument { .. } => "unmapped_argument",
                    hh_monitor::args::ArgError::UnscopedParameter { .. } => "domain_violation",
                };
                let raw_call_hash = hh_identity::idp::idp_id(
                    "raw_call",
                    input.surface_args.to_canonical_string().as_bytes(),
                );
                let mut rejected = self.minter_ev().mint(
                    "action.tool.surface_rejected",
                    events::surface_rejected_payload(
                        &input.binding.surface_id,
                        &input.binding.surface_id,
                        failure_class,
                        &raw_call_hash,
                        &input.chain.model_call_id,
                        None,
                    ),
                )?;
                rejected.scope = hh_ledger::event::Scope {
                    turn_id: Some(input.chain.turn_id.clone()),
                    model_call_id: Some(input.chain.model_call_id.clone()),
                    tool_call_id: Some(input.chain.tool_call_id.clone()),
                    ..Default::default()
                };
                let rows = if resumed {
                    vec![rejected]
                } else {
                    vec![proposed, rejected]
                };
                self.store.append(&self.run_id, lease, rows)?;
                return Ok(DispatchOutcome::Refused {
                    reason: format!("arg_eval: {e:?}"),
                });
            }
        };
        let args_canonical_hash = hh_identity::idp::idp_id(
            "canonical_args",
            canonical_json(&canonical).to_canonical_string().as_bytes(),
        );

        // The deterministic assessor inputs (executor-registered, kernel-run):
        // `inside_writable_roots` via `workspace_scope`; `command_parse` via
        // the closed-grammar classifier.
        let fs_paths = canonical
            .scoped
            .values()
            .filter_map(|v| v.as_str().map(String::from))
            .collect::<Vec<_>>();
        let declared_risk = project_risk(input.declared.attributes.as_ref());
        let read_only = declared_risk.is_read_only();
        let inputs = AssessmentInputs {
            inside_writable_roots: workspace_scope(
                handle.containment.policy(),
                &fs_paths,
                read_only,
            ),
            // The closed-grammar classifier only applies to command-bearing
            // domains — a `{"path","content"}` arg shape is not a command and
            // must not report `Failed` (which would raise the class to
            // `unknown` and poison every non-command effect).
            command_parse: match input.declared.domain {
                EffectDomain::Exec | EffectDomain::SpawnProcess => {
                    Some(classify_command(&input.surface_args))
                }
                _ => None,
            },
            host_allowlisted: Tri::Unknown,
            compensation_registered: if input.compensation_plan_id.is_some() {
                Tri::Yes
            } else {
                Tri::No
            },
            // The covering grant reaches the compensator: a registered plan's
            // compensator is sealed against the proposer's scope at plan
            // commit, so presence of the plan covers it (Stage-1 rule).
            compensator_covered: if input.compensation_plan_id.is_some() {
                Tri::Yes
            } else {
                Tri::No
            },
            // Π-9: a spawn with no requested child grants is vacuously
            // contained; a non-empty request is the delegate machinery's
            // covering check (unknown ⇒ the Π-9 row denies).
            child_contained: if input.requested_grants.is_empty() {
                Tri::Yes
            } else {
                Tri::Unknown
            },
            // ADR-0053 D5 — a live `policy_rule`-basis handle covering the
            // proposer + domain over the canonical args confers
            // `pre_authorized` (the unattended-ask exception; the record
            // confers, never a flag).
            pre_authorized: self.monitor.pre_authorized(
                &input.proposer,
                input.declared.domain,
                &canonical,
            ),
            ..AssessmentInputs::default()
        };
        let assessed = kernel_assessed(
            input.declared.attributes.as_ref(),
            input.declared.domain == EffectDomain::FsWrite,
            &inputs,
        );
        // The dedup gate — a replayed idempotency key never redispatches.
        let key = idempotency_key(
            &self.run_id,
            &effect_id,
            &args_canonical_hash,
            &input.capability_ref.version_id,
        );
        // The dedup consult is two-tier (§5a.2): the in-memory store first,
        // then the durable `effects_by_key` fold — a rebuilt dispatcher
        // (post-restore, cold cache) still vetoes the second commit
        // (AC-R-2.2.3-3's `duplicate_effect_count = 0` rests on the durable
        // half, never on process memory).
        let prior_hit = self
            .dedup
            .lookup(&key)
            .map(str::to_string)
            .or_else(|| self.durable_key_lookup(&key));
        if let Some(prior) = prior_hit {
            // The stored-observation serve (AC-R-2.2.2-1 — `dedup_support
            // ≠ none` returns the recorded observation, never a re-run).
            let stored = hh_ledger::replay::stored_observation(self.store, &self.run_id, &prior)
                .ok()
                .flatten();
            // The coordinate's `tool_call`/`effect_id` scopes are already
            // closed by the first dispatch's terminals — the dedup row is
            // anchored at the (still-open) turn/model_call chain only.
            let mut ev = self.minter_ev().mint(
                "action.tool.rejected",
                events::tool_rejected_payload("dedup", &format!("dup of {prior}")),
            )?;
            ev.scope = hh_ledger::event::Scope {
                turn_id: Some(input.chain.turn_id.clone()),
                model_call_id: Some(input.chain.model_call_id.clone()),
                ..Default::default()
            };
            self.store.append(&self.run_id, lease, vec![ev])?;
            return Ok(DispatchOutcome::Duplicate {
                prior_effect_id: prior,
                stored_observation: stored,
            });
        }

        // `proposed` (minted above, before the arg-map eval) opens the
        // tool_call scope; `intended` opens the effect scope under it. A
        // resumed dispatch (the effect already `intended` under a suspended
        // ask) skips the pair — the scopes are open, the trail stands.
        if !resumed {
            let intended = self.minter_ev().mint_effect(
                "action.effect.intended",
                events::intended_payload(
                    &assessed,
                    Some(&declared_risk),
                    &input.capability_ref.version_id,
                    &args_canonical_hash,
                    input.ordinal,
                ),
                &effect_id,
                &input.chain,
            )?;
            self.store
                .append(&self.run_id, lease, vec![proposed, intended])?;
        }
        // KP-1 — runtime death after `intended`, before `decided`.
        self.check_kill(&[KillPoint::Kp1])?;

        // ── 2 authorize ────────────────────────────────────────────────────
        // The containment gate — the recorded verdict `authorize` consumes.
        let admit = admit_input(input.declared.domain, input.scope_bindings, &canonical);
        let gate = floor_gate(handle.containment.policy(), handle.report.as_ref(), &admit);
        let proposal = Proposal {
            effect_id: effect_id.clone(),
            attempt_no: 1,
            proposer: input.proposer.clone(),
            capability_ref: input.capability_ref.clone(),
            effect: input.declared.clone(),
            surface_args: input.surface_args.clone(),
            args_provenance: Some(input.args_provenance.clone()),
            context_label: input.context_label.clone(),
            self_report: input.self_report,
            inputs,
            requested_grants: input.requested_grants.clone(),
            containment: gate.clone(),
            flow: input.flow.clone(),
            remedy_taken: None,
            at: self.store.now_ms(),
        };
        let decision = self
            .monitor
            .authorize(&proposal)
            .map_err(|e| EnvError::Blob(format!("authorize: {e:?}")))?;
        // The `ask` path's durable owed-decision id — minted before the
        // `decided` row so the whole trail (`decided → pending → requested →
        // refused`) shares one `permission_id` (§5g.7 §5; S1.23). The mint is
        // deterministic — identical pending `(capability_ref,
        // args_canonical_hash)` asks mint the same id (the coalescing rule);
        // `irreversible`/`unknown` never coalesce (the effect salts the mint).
        let never_auto = hh_monitor::approval::never_auto(decision.effective_risk_class);
        let permission_id = matches!(decision.decision, Decision::Ask { .. }).then(|| {
            hh_monitor::approval::mint_permission_id(
                &input.capability_ref,
                &args_canonical_hash,
                never_auto,
                &effect_id,
            )
        });
        let mut decided_payload = decided_payload(&decision, 1, "proposal");
        if let (Json::Obj(m), Some(pid)) = (&mut decided_payload, &permission_id) {
            m.insert("permission_id".to_string(), Json::str(pid.clone()));
            m.insert(
                "requested_at".to_string(),
                Json::Int(self.store.now_ms() as i64),
            );
        }
        // A decision restating a *recorded* `decided` row (the resume path —
        // `decider = human` with an `origin_permission_id` back-reference) is
        // not re-minted: the recorded row already holds the `(effect_id,
        // attempt)` gate slot, and a second final decided would be
        // `DuplicateDecision` (§5g.1 I-H7 — exactly one per attempt cycle).
        let recorded_served = decision.decider == hh_monitor::decision::Decider::Human
            && decision.origin_permission_id.is_some();
        if !recorded_served {
            let decided = self
                .minter_ev()
                .mint("security.permission.decided", decided_payload)?;
            self.store.append(&self.run_id, lease, vec![decided])?;
        }
        let risk = decision.effective_risk_class;

        // A refused decision never executed — the `tool_call` scope closes
        // with `action.tool.rejected` (its terminal), `effect_id` with
        // `refused`. `rejected` carries the tool-call chain only — `refused`
        // already closed the effect scope in the same batch.
        let tool_rejected =
            |minter: &EventMinter, reason: &str| -> Result<hh_ledger::event::Event, LedgerError> {
                let mut ev = minter.mint(
                    "action.tool.rejected",
                    events::tool_rejected_payload("authorize", reason),
                )?;
                ev.scope = hh_ledger::event::Scope {
                    turn_id: Some(input.chain.turn_id.clone()),
                    model_call_id: Some(input.chain.model_call_id.clone()),
                    tool_call_id: Some(input.chain.tool_call_id.clone()),
                    ..Default::default()
                };
                Ok(ev)
            };
        match &decision.decision {
            Decision::Deny { reason, .. } => {
                let refused = self.minter_ev().mint_effect(
                    "action.effect.refused",
                    events::refused_payload(&format!("{reason:?}")),
                    &effect_id,
                    &input.chain,
                )?;
                let rejected = tool_rejected(&self.minter_ev(), &format!("{reason:?}"))?;
                self.store
                    .append(&self.run_id, lease, vec![refused, rejected])?;
                return Ok(DispatchOutcome::Refused {
                    reason: format!("{reason:?}"),
                });
            }
            Decision::Ask { options, remedies } => {
                // ── C1 reviewer chain (§5g.7 §4) ─────────────────────────
                // `authorize` steps 7–8 already ran over the same fold — a
                // serving lease lands `decider = cache` (never `ask`), and an
                // exhausted approvals budget converts to `deny` at step 7.
                // The chain re-derives those record-level legs and runs the
                // remaining stages: Π-12 unattended policy, the attested hook
                // stage, sealed `auto_review` rules, then the human stage.
                // The offline stage declares no hook reports or auto-review
                // rules here — the never-auto and Π-12 members fail closed
                // regardless (I-P1).
                let cap = self
                    .monitor
                    .capabilities
                    .get(&input.capability_ref.semantic_id)
                    .expect("authorize resolved the capability");
                let esc = self.monitor.escalation_input(
                    &proposal,
                    &canonical,
                    cap,
                    decision.effective_authority,
                    decision.effective_risk_class,
                );
                let chain = hh_monitor::approval::run_chain(
                    &esc,
                    &self.monitor.approvals.leases,
                    &[],
                    &self.monitor.auto_review_rules,
                    hh_monitor::approval::LeaseScope::Run,
                );
                let pid = permission_id
                    .clone()
                    .expect("permission_id minted for every Ask decision");
                // `security.permission.escalated` — one chain-hop audit row
                // per stage transition (§5g.7 §4 `{permission_id, from_stage,
                // to_stage, reason}`).
                for (from, to, why) in &chain.escalations {
                    let hop = self.minter_ev().mint_effect(
                        "security.permission.escalated",
                        Json::obj([
                            ("permission_id", Json::str(pid.clone())),
                            ("from_stage", Json::str(from.as_str())),
                            ("to_stage", Json::str(to.as_str())),
                            ("reason", Json::str(why.clone())),
                        ]),
                        &effect_id,
                        &input.chain,
                    )?;
                    self.store.append(&self.run_id, lease, vec![hop])?;
                }
                match &chain.decision {
                    hh_monitor::approval::ChainDecision::Allow {
                        decider,
                        cache_key,
                        lease_id,
                        origin_permission_id,
                        ..
                    } => {
                        // A lease resolved at the chain (a fold newer than
                        // `authorize`'s view): `decided{ask}` was non-final —
                        // the final `decided{allow}` and the `lease.used` row
                        // land, then the effect proceeds (the committed gate
                        // credits decided{allow} — H-7).
                        let mut m = Json::obj([
                            ("permission_id", Json::str(pid.clone())),
                            ("decision", Json::str("allow")),
                            ("decider", Json::str(decider.as_str())),
                            ("decision_scope", Json::str("session")),
                            ("reason", Json::str("lease_hit")),
                            ("attempt_no", Json::Int(1)),
                        ]);
                        if let (Json::Obj(mm), Some(k)) = (&mut m, cache_key) {
                            mm.insert("cache_key".to_string(), Json::str(k.clone()));
                        }
                        if let (Json::Obj(mm), Some(o)) = (&mut m, origin_permission_id) {
                            mm.insert("origin_permission_id".to_string(), Json::str(o.clone()));
                        }
                        let decided_allow = self.minter_ev().mint_effect(
                            "security.permission.decided",
                            m,
                            &effect_id,
                            &input.chain,
                        )?;
                        let mut rows = vec![decided_allow];
                        if let (Some(k), Some(lid)) = (cache_key, lease_id) {
                            let uses = self
                                .monitor
                                .approvals
                                .leases
                                .get(k)
                                .map(|l| l.uses + 1)
                                .unwrap_or(1);
                            rows.push(self.minter_ev().mint_effect(
                                "security.permission.lease.used",
                                Json::obj([
                                    ("lease_id", Json::str(lid.clone())),
                                    ("key_hash", Json::str(k.clone())),
                                    ("uses", Json::Int(uses as i64)),
                                    ("permission_id", Json::str(pid.clone())),
                                ]),
                                &effect_id,
                                &input.chain,
                            )?);
                        }
                        self.store.append(&self.run_id, lease, rows)?;
                        // Fall through to the `authorized` path.
                    }
                    hh_monitor::approval::ChainDecision::Deny { reason, .. } => {
                        // The chain resolved deny inside the ask window —
                        // Π-12 unattended, a hook deny or the exhaustion
                        // conversion. `decided{ask}` was non-final; this deny
                        // is the attempt cycle's terminal verdict.
                        let decided_deny = self.minter_ev().mint_effect(
                            "security.permission.decided",
                            Json::obj([
                                ("permission_id", Json::str(pid.clone())),
                                ("decision", Json::str("deny")),
                                ("decider", Json::str("policy")),
                                ("reason", Json::str(reason.as_str())),
                                ("attempt_no", Json::Int(1)),
                            ]),
                            &effect_id,
                            &input.chain,
                        )?;
                        let refused = self.minter_ev().mint_effect(
                            "action.effect.refused",
                            events::refused_payload(reason.as_str()),
                            &effect_id,
                            &input.chain,
                        )?;
                        let rejected = tool_rejected(&self.minter_ev(), reason.as_str())?;
                        self.store.append(
                            &self.run_id,
                            lease,
                            vec![decided_deny, refused, rejected],
                        )?;
                        return Ok(DispatchOutcome::Refused {
                            reason: reason.as_str().to_string(),
                        });
                    }
                    hh_monitor::approval::ChainDecision::AskHuman { .. }
                    | hh_monitor::approval::ChainDecision::Defer { .. } => {
                        let deferred = matches!(
                            chain.decision,
                            hh_monitor::approval::ChainDecision::Defer { .. }
                        );
                        // Step 7's consumption half (ADR-0040 D6): a
                        // human-targeted ask reserves `approvals.requested =
                        // 1` on the effect's budget node when the dispatch
                        // names one — a refusal converts the ask to
                        // `deny{ApprovalsExhausted}`, the same verdict the
                        // monitor's `approvals_max` fold produces.
                        if let (Some(req), Some(src)) = (&input.reserve, reserve_src.as_deref_mut())
                        {
                            if src
                                .reserve(
                                    &req.budget_id,
                                    &hh_budget::quantity::ResourceVector::one(
                                        hh_ontology::dimensions::DimensionId::ApprovalsRequested,
                                        1,
                                    ),
                                    &input.proposer,
                                    req.ttl_ms,
                                )
                                .is_err()
                            {
                                let decided_deny = self.minter_ev().mint_effect(
                                    "security.permission.decided",
                                    Json::obj([
                                        ("permission_id", Json::str(pid.clone())),
                                        ("decision", Json::str("deny")),
                                        ("decider", Json::str("policy")),
                                        ("reason", Json::str("ApprovalsExhausted")),
                                        ("attempt_no", Json::Int(1)),
                                    ]),
                                    &effect_id,
                                    &input.chain,
                                )?;
                                let refused = self.minter_ev().mint_effect(
                                    "action.effect.refused",
                                    events::refused_payload("ApprovalsExhausted"),
                                    &effect_id,
                                    &input.chain,
                                )?;
                                let rejected =
                                    tool_rejected(&self.minter_ev(), "ApprovalsExhausted")?;
                                self.store.append(
                                    &self.run_id,
                                    lease,
                                    vec![decided_deny, refused, rejected],
                                )?;
                                return Ok(DispatchOutcome::Refused {
                                    reason: "ApprovalsExhausted".to_string(),
                                });
                            }
                        }
                        // The owed-decision trail (§5g.7 §5): `pending` is
                        // the durable record, `requested` the ephemeral
                        // prompt rendering — then the C1 terminal: the run
                        // suspends `awaiting_approval` behind a
                        // `permission_decided` wakeup subscription and the
                        // surface's `respond` resolves it (never `unknown` —
                        // AC-R-2.8.7-6).
                        let asked_at = self.store.now_ms();
                        // §5g.7 §5 batching (ADR-0070 D5): a batchable ask
                        // joins the run's batch over `(effective_risk_class,
                        // requested_of, scope)` — the deterministic key, so
                        // every same-class ask to the human reviewer shares
                        // one `batch_id` (one prompt may be rendered; each
                        // `permission_id` still decides independently).
                        // `irreversible` and `permission_request` asks carry
                        // no batch (`EscalationInput.batchable` is the admit).
                        let batch_id = if esc.batchable {
                            Some(hh_monitor::approval::mint_batch_id(
                                &esc.risk,
                                "principal",
                                &esc.scope_ref,
                            ))
                        } else {
                            None
                        };
                        let request = hh_monitor::approval::ApprovalRequest {
                            permission_id: pid.clone(),
                            request: hh_monitor::approval::PermissionRequest {
                                subject_ref: input.proposer.clone(),
                                capability_ref: input.capability_ref.clone(),
                                args_canonical_hash: args_canonical_hash.clone(),
                                reason: "pi ask".to_string(),
                                // The `permission_request` proposal's
                                // `requested` leg — the approval mint's
                                // `requested ⊓ authority_cap` input; empty on
                                // an ordinary effect ask.
                                requested_grants: input.requested_grants.clone(),
                            },
                            options: options
                                .iter()
                                .filter_map(|o| {
                                    hh_monitor::approval::ApprovalOptionId::parse(o).map(|id| {
                                        hh_monitor::approval::ApprovalOption {
                                            id,
                                            label: o.clone(),
                                        }
                                    })
                                })
                                .collect(),
                            mode: if deferred {
                                hh_monitor::approval::ApprovalMode::Async
                            } else {
                                hh_monitor::approval::ApprovalMode::Sync
                            },
                            // `TimeoutPolicy[permission]` — `deadline none`
                            // attended (`None`); a declared default caps the
                            // wait (S2.11).
                            timeout: self.permission_timeout_ms,
                            explanation: hh_monitor::approval::Explanation {
                                display: format!("pi asked: {} option(s)", options.len()),
                                rows: decision.checks.clone(),
                                model_justification: None,
                            },
                            batch_id,
                        };
                        let pending = self.minter_ev().mint_effect(
                            "security.permission.pending",
                            hh_monitor::approval::pending_payload(&request, &effect_id, asked_at),
                            &effect_id,
                            &input.chain,
                        )?;
                        let requested = self.minter_ev().mint_effect(
                            "security.permission.requested",
                            hh_monitor::approval::requested_payload(&request, &effect_id, asked_at),
                            &effect_id,
                            &input.chain,
                        )?;
                        let pending_event_id = pending.event_id.clone();
                        self.store
                            .append(&self.run_id, lease, vec![pending, requested])?;
                        // The defer slice (§5a.3): the `permission_decided`
                        // subscription fires when `respond` mints `decided`;
                        // `suspend` records `awaiting_approval` (the writer
                        // lease is kept — the resume is in-process at the
                        // offline stage). On a non-C1 build these refuse
                        // typed `UnsupportedTier`, never a silent skip.
                        let sub = self.store.wakeup_subscribe(
                            &self.run_id,
                            lease,
                            hh_ledger::wakeup::Trigger::PermissionDecided {
                                permission_id: pid.clone(),
                            },
                            hh_ledger::wakeup::WakeupPolicy::default_policy(),
                            &hh_ledger::manifest::EventRef {
                                run_id: self.run_id.clone(),
                                event_id: pending_event_id,
                            },
                        )?;
                        self.store.suspend(
                            &self.run_id,
                            lease,
                            &[hh_ledger::suspend::SuspendReason::AwaitingApproval {
                                permission_id: pid.clone(),
                            }],
                            &[sub],
                            Json::obj([("on", Json::str("permission_decided"))]),
                            false,
                        )?;
                        let _ = remedies;
                        return Ok(DispatchOutcome::Suspended { permission_id: pid });
                    }
                }
            }
            Decision::Allow => {
                // Step 8's lease hit (`authorize` resolved `ask → allow` under
                // `decider = cache`) consumes a use — the `lease.used` row is
                // the fold's accounting (the rebuild recomputes `uses`).
                if let Some(k) = &decision.cache_key {
                    let uses = self
                        .monitor
                        .approvals
                        .leases
                        .get(k)
                        .map(|l| l.uses + 1)
                        .unwrap_or(1);
                    let lease_used = self.minter_ev().mint_effect(
                        "security.permission.lease.used",
                        Json::obj([
                            ("key_hash", Json::str(k.clone())),
                            ("uses", Json::Int(uses as i64)),
                            (
                                "permission_id",
                                decision
                                    .origin_permission_id
                                    .as_ref()
                                    .map(|o| Json::str(o.clone()))
                                    .unwrap_or(Json::Null),
                            ),
                        ]),
                        &effect_id,
                        &input.chain,
                    )?;
                    self.store.append(&self.run_id, lease, vec![lease_used])?;
                }
            }
        }
        let authorized = self.minter_ev().mint_effect(
            "action.effect.authorized",
            events::authorized_payload(&risk),
            &effect_id,
            &input.chain,
        )?;
        self.store.append(&self.run_id, lease, vec![authorized])?;
        // KP-2 — runtime death after `decided{allow}`/`authorized`,
        // before `prepared` (the cache-lookup and prepare stages follow).
        self.check_kill(&[KillPoint::Kp2])?;

        // ── 2c E1 `state_requires`/`PreconditionDomain` gate at `prepare`
        // (§5d.1: "`state_requires` at `prepare`"; a violation is
        // `PreconditionViolated{kind}` → `action.effect.refused` +
        // `action.tool.rejected`, never a warning — ADR-0087 D2). The gate
        // runs post-`authorized`/pre-K4 so a stale/unsupported state
        // precondition refuses before any cache serve or executor touch.
        if let Err(kind) = check_state_preconditions(input, &handle, self.store.now_ms()) {
            let reason = format!("PreconditionViolated{{kind: {kind}}}");
            let refused = self.minter_ev().mint_effect(
                "action.effect.refused",
                events::refused_payload(&reason),
                &effect_id,
                &input.chain,
            )?;
            let rejected = tool_rejected(&self.minter_ev(), &reason)?;
            self.store
                .append(&self.run_id, lease, vec![refused, rejected])?;
            return Ok(DispatchOutcome::Refused { reason });
        }

        // ── 2b K4 lookup (§5b.4; R-2.3.4¹; ADR-0129 d.1) ──────────────────
        // The cache is consulted only *after* the monitor's allow is durable
        // — a hit never bypasses authorization, and it rides the ordinary
        // lifecycle (`prepared`/`observed`/`completed` still land; `started`
        // and `committed` are absent by construction — no executor ran).
        // A non-admissible call is a `bypass` row; `miss`/`stale_withheld`/
        // `refused` fall through to the executor path.
        if let Some(outcome) = self.k4_lookup(
            input,
            &handle,
            &args_canonical_hash,
            &risk,
            &key,
            &effect_id,
            lease,
        )? {
            return Ok(outcome);
        }

        // ── 3 prepare ──────────────────────────────────────────────────────
        let token = self.minter.mint(&effect_id, 1, &input.env_handle_id);
        // `reserve` — reserve-before-spend (the reservation is `hh-budget`'s
        // `control.budget.reserved`; the caller's `ReservationSource` appends
        // it).
        if let (Some(req), Some(src)) = (&input.reserve, reserve_src) {
            src.reserve(&req.budget_id, &req.quantity, &req.holder, req.ttl_ms)
                .map_err(|e| EnvError::BudgetRefused { detail: e })?;
        }
        // The fs baseline is taken at prepare: for a `reversible` class it
        // doubles as the `baseline_ref` `prepared` requires; for every class
        // it anchors the post-exec `fs_change` diff.
        let (pre_rec, pre_baseline) =
            driver.path_baseline(self.store, lease, &input.env_handle_id)?;
        let baseline_ref = input
            .baseline_ref
            .clone()
            .or_else(|| Some(pre_rec.snapshot_ref.clone()));
        let prepared = self.minter_ev().mint_effect(
            "action.effect.prepared",
            events::prepared_payload(
                &key,
                baseline_ref.as_deref(),
                input.compensation_plan_id.as_deref(),
                &token.hash,
                input.ladder.effective(),
                &input.output_policy.policy_ref(),
            ),
            &effect_id,
            &input.chain,
        )?;
        self.store.append(&self.run_id, lease, vec![prepared])?;
        self.dedup.record(&key, &effect_id);
        // KP-3 — runtime death after `prepared`, before `committed`
        // (for `read_only` there is no commit phase: the kill lands at
        // the same post-`prepared` boundary).
        self.check_kill(&[KillPoint::Kp3])?;

        // ── 4 commit ───────────────────────────────────────────────────────
        // `read_only` has no write-ahead (observed may follow prepared);
        // every other class commits under the fencing token — and the ledger
        // refuses `committed` without a preceding allow (complete mediation).
        let attempt_no = 1u64;
        let commit_evidence = if !risk.is_read_only() {
            let committed = self.minter_ev().mint_effect(
                "action.effect.committed",
                events::committed_payload(attempt_no, lease.generation, self.store.now_ms()),
                &effect_id,
                &input.chain,
            )?;
            let committed_event_id = committed.event_id.clone();
            let range = self.store.append(&self.run_id, lease, vec![committed])?;
            // KP-4 — runtime death after durable `committed`, before the
            // dispatch reached the executor (`read_only` never commits —
            // the (KP-4, read_only) cell is `n/a` in the battery).
            self.check_kill(&[KillPoint::Kp4])?;
            // The write-ahead evidence the helper recomputes `commit_proof`
            // over (S2.1 — the helper admits `exec` only when the durable
            // `committed` row's members prove prepare-before-execute).
            crate::helper::CommitEvidence::Committed {
                event_id: committed_event_id,
                seq: range.first,
                fencing_token: lease.generation,
            }
        } else {
            crate::helper::CommitEvidence::ReadOnly
        };
        self.execute_capture_observe(
            driver,
            executor,
            input,
            lease,
            &handle,
            &effect_id,
            attempt_no,
            None,
            &canonical,
            &key,
            risk,
            &args_canonical_hash,
            pre_baseline,
            &token,
            commit_evidence,
        )
    }

    /// `resume_paused(driver, executor, input, lease)` — the §5d.4 D3 retry
    /// (ADR-0097 D3): a paused effect (still `prepared`/`committed`, never
    /// terminal) re-executes under the same `effect_id` at `attempt_no + 1`,
    /// echoing the recorded `request_state` unmodified. The caller supplies
    /// the human's answer in `surface_args` — the same `DispatchInput`, the
    /// same effect coordinate. A missing pause trail or a non-paused phase
    /// is a typed refusal, never a silent fresh dispatch.
    pub fn resume_paused(
        &mut self,
        driver: &mut EnvDriver,
        executor: &mut dyn ToolExecutor,
        input: &DispatchInput,
        lease: &Lease,
    ) -> Result<DispatchOutcome, EnvError> {
        let handle = driver
            .handle(&input.env_handle_id)
            .ok_or_else(|| EnvError::Unavailable {
                env_handle_id: input.env_handle_id.clone(),
                state: "missing",
            })?
            .clone();
        handle.verify_environment()?;
        let effect_id = Store::effect_id(
            &self.run_id,
            &input.chain.model_call_id,
            &input.chain.tool_call_id,
            input.ordinal,
        );
        let fold = self
            .store
            .effect_folds(&self.run_id)?
            .into_iter()
            .find(|(id, _)| id == &effect_id)
            .map(|(_, f)| f)
            .ok_or(EnvError::InvalidState {
                op: "resume_paused",
                state: "no_fold",
            })?;
        // The pause trail: the `control.decision{kind: ask,
        // decision_point: protocol_input_required}` row owning this effect.
        // Its `request_state` is echoed byte-for-byte — never read.
        let ask = self
            .store
            .events(&self.run_id)?
            .iter()
            .rev()
            .find(|e| {
                e.class == "control.decision"
                    && e.payload.get("kind").and_then(Json::as_str) == Some("ask")
                    && e.payload.get("decision_point").and_then(Json::as_str)
                        == Some("protocol_input_required")
                    && e.payload.get("owner").and_then(Json::as_str) == Some(effect_id.as_str())
            })
            .ok_or(EnvError::InvalidState {
                op: "resume_paused",
                state: "no_pause_trail",
            })?;
        let request_state = ask
            .payload
            .get("request_state")
            .cloned()
            .unwrap_or(Json::Null);
        let attempt_no = fold.attempt_no + 1;
        match fold.phase {
            hh_ledger::effect::EffectPhase::Committed
            | hh_ledger::effect::EffectPhase::Prepared => {}
            other => {
                return Err(EnvError::InvalidState {
                    op: "resume_paused",
                    state: other.as_str(),
                })
            }
        }
        let canonical =
            hh_monitor::args::eval(input.binding, input.scope_bindings, &input.surface_args)
                .map_err(|e| EnvError::ResumeRefused {
                    reason: format!("arg_eval: {e:?}"),
                })?;
        let args_canonical_hash = hh_identity::idp::idp_id(
            "canonical_args",
            canonical_json(&canonical).to_canonical_string().as_bytes(),
        );
        let risk = fold.risk_class;
        let key = idempotency_key(
            &self.run_id,
            &effect_id,
            &args_canonical_hash,
            &input.capability_ref.version_id,
        );
        let token = self
            .minter
            .mint(&effect_id, attempt_no, &input.env_handle_id);
        let (pre_rec, pre_baseline) =
            driver.path_baseline(self.store, lease, &input.env_handle_id)?;
        let _ = pre_rec;
        let commit_evidence = if !risk.is_read_only() {
            // The retry is a fresh ledger attempt: complete mediation
            // wants a `security.permission.decided{allow}` for
            // `(effect_id, attempt_no)` before the `committed` write-ahead
            // (§5g.1 I-H7). The carried decision is the envelope's —
            // the recorded ask trail already established the pause was
            // legitimate; the fresh `decided` names `paused_resume` as
            // its proposal so the audit shows the carry, never a new
            // discretionary grant.
            let mut decided = self.minter_ev().mint_effect(
                "security.permission.decided",
                Json::obj([
                    ("decision", Json::str("allow")),
                    ("decider", Json::str("envelope")),
                    ("proposal", Json::str("paused_resume")),
                    ("attempt_no", Json::Int(attempt_no as i64)),
                    ("reason", Json::str("paused_retry_carry:input_required")),
                ]),
                &effect_id,
                &input.chain,
            )?;
            decided.scope.effect_id = Some(effect_id.clone());
            let committed = self.minter_ev().mint_effect(
                "action.effect.committed",
                events::committed_payload(attempt_no, lease.generation, self.store.now_ms()),
                &effect_id,
                &input.chain,
            )?;
            let committed_event_id = committed.event_id.clone();
            let range = self
                .store
                .append(&self.run_id, lease, vec![decided, committed])?;
            crate::helper::CommitEvidence::Committed {
                event_id: committed_event_id,
                seq: range.last,
                fencing_token: lease.generation,
            }
        } else {
            crate::helper::CommitEvidence::ReadOnly
        };
        self.execute_capture_observe(
            driver,
            executor,
            input,
            lease,
            &handle,
            &effect_id,
            attempt_no,
            Some(&request_state),
            &canonical,
            &key,
            risk,
            &args_canonical_hash,
            pre_baseline,
            &token,
            commit_evidence,
        )
    }

    /// `execute_capture_observe` — the dispatch tail (IR-3 epoch bump →
    /// `action.tool.started` → execute → capture → observe; the §5d.4 D3
    /// paused arm included). Shared by `dispatch` (attempt 1) and
    /// `resume_paused` (`attempt_no + 1`, `request_state` echoed).
    #[allow(clippy::too_many_arguments)]
    fn execute_capture_observe(
        &mut self,
        driver: &mut EnvDriver,
        executor: &mut dyn ToolExecutor,
        input: &DispatchInput,
        lease: &Lease,
        handle: &crate::handle::EnvHandle,
        effect_id: &str,
        attempt_no: u64,
        request_state: Option<&Json>,
        canonical: &CanonicalArgs,
        idem_key: &str,
        risk: RiskClass,
        args_canonical_hash: &str,
        pre_baseline: PathBaseline,
        token: &crate::tokens::AttributionToken,
        commit_evidence: crate::helper::CommitEvidence,
    ) -> Result<DispatchOutcome, EnvError> {
        let effect_id = effect_id.to_string();
        let key = idem_key.to_string();
        let args_canonical_hash = args_canonical_hash.to_string();
        // IR-3 — a committed effect in a mutating domain
        // (`fs_write | exec | net_egress | spawn_process`) bumps
        // `mutation_epoch(environment_ref)`; every K4 entry pinned to the
        // earlier epoch is `stale_withheld` on its next lookup (nothing is
        // cleared — the stamp mismatch is the withholding mechanism).
        if let Some(c) = self.cache.as_mut() {
            c.note_committed(&env_ref_str(handle), input.declared.domain);
        }
        let execution_id = self.store.alloc_id("exec");
        // The M-point around `exec`/`read` (AC-R-2.5.5-11) — `action.tool.
        // completed` reports `execution_ms` against this stamp.
        let exec_started_ms = self.store.now_ms();
        let started = self.minter_ev().mint_effect(
            "action.tool.started",
            events::tool_started_payload(&execution_id, &token.hash),
            &effect_id,
            &input.chain,
        )?;
        let started_id = started.event_id.clone();
        self.store.append(&self.run_id, lease, vec![started])?;

        // ── 5 execute + 6 capture ──────────────────────────────────────────
        let request = ExecutionRequest {
            execution_id: execution_id.clone(),
            effect_id: effect_id.clone(),
            attempt_no,
            capability_ref: (
                input.capability_ref.semantic_id.clone(),
                input.capability_ref.version_id.clone(),
            ),
            effect: input.declared.clone(),
            args: canonical_json(canonical),
            env_handle_id: input.env_handle_id.clone(),
            attribution_token: token.token.clone(),
            deadline_ms: input.ladder.effective(),
            ladder: input.ladder,
            retain_bytes_cap: input.output_policy.retain_bytes_cap,
            idempotency_key: key.clone(),
            commit_evidence,
            request_state: request_state.cloned(),
        };
        let resolver = self.minter.resolver();
        let run_id = self.run_id.clone();
        let mut items: Vec<CaptureItem> = Vec::new();
        let mut unattributed: Vec<String> = Vec::new();
        let mut seq = 0u64;
        let mut sink = |sig: ExecutorSignal| {
            seq += 1;
            match resolver.resolve(&sig.token) {
                ResolveOutcome::Resolved { .. } => {
                    items.push(CaptureItem {
                        run_id: run_id.clone(),
                        turn_id: input.chain.turn_id.clone(),
                        model_call_id: input.chain.model_call_id.clone(),
                        tool_call_id: input.chain.tool_call_id.clone(),
                        effect_id: effect_id.clone(),
                        attempt_no,
                        execution_id: execution_id.clone(),
                        seq,
                        ts_mono: seq, // the helper's monotonic clock; kernel-stamped at capture
                        source: CaptureSource::ExecutorReported,
                        kind: sig.kind,
                        provenance: "executor".to_string(),
                    });
                }
                ResolveOutcome::Unknown => {
                    unattributed.push(TokenResolver::hash_of(&sig.token));
                }
            }
        };
        let report = match executor.execute(&request, &mut sink) {
            Ok(r) => {
                // KP-5/KP-6 — runtime death with the executor's work in
                // flight or complete but `observed` never durable. The
                // report is produced; the runtime dies before it lands.
                self.check_kill(&[KillPoint::Kp5, KillPoint::Kp6])?;
                r
            }
            Err(e) => {
                // The executor itself failed (transport-plane — helper crash /
                // spawn refusal): `unknown{executor_error}` → probe.
                let unknown = self.minter_ev().mint_effect(
                    "action.effect.unknown",
                    events::unknown_payload(attempt_no, lease.generation, "executor_error"),
                    &effect_id,
                    &input.chain,
                )?;
                self.store.append(&self.run_id, lease, vec![unknown])?;
                self.emit_unattributed(&unattributed, lease)?;
                self.minter.expire(&effect_id, attempt_no);
                let _ = e;
                return Ok(DispatchOutcome::Unknown {
                    cause: "executor_error".to_string(),
                });
            }
        };

        // The post-exec fs diff — the kernel's `fs_change` observer
        // (`kernel_derived`, never executor-reported).
        let post_baseline = driver
            .path_baseline(self.store, lease, &input.env_handle_id)
            .map(|(_, b)| b)
            .unwrap_or_default();
        let diff = PathBaseline::diff(&pre_baseline, &post_baseline);
        for (entry, change) in diff.entries() {
            seq += 1;
            let inside = handle.roots.is_writable(&entry.path_canonical);
            items.push(CaptureItem {
                run_id: self.run_id.clone(),
                turn_id: input.chain.turn_id.clone(),
                model_call_id: input.chain.model_call_id.clone(),
                tool_call_id: input.chain.tool_call_id.clone(),
                effect_id: effect_id.clone(),
                attempt_no,
                execution_id: execution_id.clone(),
                seq,
                ts_mono: seq,
                source: CaptureSource::KernelDerived,
                kind: CaptureKind::FsChange {
                    path_canonical: entry.path_canonical.clone(),
                    change: change.to_string(),
                    before_ref: entry.before_ref.clone(),
                    after_ref: entry.after_ref.clone(),
                    inside_writable_roots: inside,
                },
                provenance: "kernel".to_string(),
            });
        }

        // Redact on the capture path — mask every captured payload before it
        // enters the manifest or the raw blob (ADR-0101 D5); a non-placeholder
        // hit on an executor-reported payload is a `leak_detected` row
        // (the hostile-executor path — AC-R-2.5.5-8).
        let mut raw_output = String::new();
        let mut leaks: Vec<hh_secrets::redact::DetectorKind> = Vec::new();
        for it in &mut items {
            let hits = redact_item(it, &self.detectors);
            if it.source == CaptureSource::ExecutorReported {
                for h in hits {
                    if h != hh_secrets::redact::DetectorKind::PlaceholderPassthrough
                        && !leaks.contains(&h)
                    {
                        leaks.push(h);
                    }
                }
            }
            if let CaptureKind::OutputChunk { data, .. } = &it.kind {
                raw_output.push_str(data);
            }
        }
        if !leaks.is_empty() {
            let mut evs = Vec::new();
            for d in &leaks {
                evs.push(self.minter_ev().mint_effect(
                    "security.secret.leak_detected",
                    hh_secrets::events::leak_detected_payload(
                        &hh_secrets::redact::Leak::at_mediation(
                            format!("capture:{effect_id}"),
                            *d,
                            None,
                        ),
                    ),
                    &effect_id,
                    &input.chain,
                )?);
            }
            self.store.append(&self.run_id, lease, evs)?;
        }
        let raw_output_ref = if raw_output.is_empty() {
            None
        } else {
            Some(
                self.store
                    .put_blob(raw_output.as_bytes(), "text/plain")
                    .map_err(EnvError::Ledger)?
                    .id(),
            )
        };

        // The manifest — non-ephemeral items only; `seal` refuses a straggler.
        let mut manifest_items: Vec<CaptureItem> = items
            .iter()
            .filter(|i| !i.kind.is_ephemeral())
            .cloned()
            .collect();
        let mut observers: BTreeSet<String> = BTreeSet::new();
        for i in &manifest_items {
            observers.insert(i.kind.as_str().to_string());
        }
        observers.insert("fs_change".to_string()); // the kernel's diff observer always runs
        observers.insert("terminal".to_string());
        seq += 1;
        manifest_items.push(terminal_item(
            &report,
            &effect_id,
            attempt_no,
            &execution_id,
            seq,
            &input.chain,
            &self.run_id,
        ));
        let completeness = EffectCaptureManifest::compute_completeness(
            input.declared.domain,
            &observers,
            !handle.capabilities.residual_channels.is_empty(),
            report.truncated,
            true,
        );
        let manifest = EffectCaptureManifest {
            effect_id: effect_id.clone(),
            attempt_no,
            execution_id: execution_id.clone(),
            items: manifest_items,
            raw_output_ref,
            truncation: if report.truncated {
                Some(Truncation {
                    omitted_bytes: report.omitted_bytes,
                    original_size: report.original_size,
                    policy_ref: input.output_policy.policy_ref(),
                })
            } else {
                None
            },
            completeness: completeness.clone(),
            observers_present: observers,
            residual_channels_ref: None,
        }
        .seal()?;
        let manifest_ref = manifest.content_id();

        // ── 7 observe — the paused arm (§5d.4 D3; ADR-0097 D3) ─────────
        // `input_required` is *not* a terminal outcome: the effect stays
        // `prepared`/`committed` (the `tool_call`/`effect_id` scopes stay
        // open — `completed`/`observed` close them only on a real terminal),
        // the ledger records `control.decision{kind: ask,
        // decision_point: protocol_input_required}` carrying the opaque
        // `request_state`, and the elicitation opens as a `message_human`
        // effect whose `causes` name the decision row (the human's answer
        // is `principal`-authored content, never an endorsement).
        if let TerminalStatus::Paused { detail } = &report.status {
            let input_requests = detail.get("input_requests").cloned().unwrap_or(Json::Null);
            let pause_state = detail.get("request_state").cloned().unwrap_or(Json::Null);
            let decision_id = self.store.alloc_id("decision");
            let ask = self.minter_ev().mint_effect(
                "control.decision",
                events::control_decision_ask_payload(
                    &decision_id,
                    &effect_id,
                    &started_id,
                    &input_requests,
                    &pause_state,
                ),
                &effect_id,
                &input.chain,
            )?;
            // The elicitation is a *separate* `message_human` tool call
            // (ADR-0097 D3): a fresh `tool_call_id` scope —
            // `{tc}.elicitation.{attempt}` — so its `effect_id` stays the
            // derived `f(run, mc, tc, ordinal)` coordinate (the fold
            // refuses any other id). `action.tool.proposed` opens the
            // tool_call scope, `action.effect.intended` opens the effect
            // scope; `causes` names the ask row.
            let tc_elic = format!("{}.elicitation.{}", input.chain.tool_call_id, attempt_no);
            let mh_chain = events::ScopeChain {
                turn_id: input.chain.turn_id.clone(),
                model_call_id: input.chain.model_call_id.clone(),
                tool_call_id: tc_elic.clone(),
            };
            let mh_id = Store::effect_id(&self.run_id, &mh_chain.model_call_id, &tc_elic, 0);
            let mut mh_proposed = self.minter_ev().mint(
                "action.tool.proposed",
                events::tool_proposed_payload("hh:message_human", "elicitation"),
            )?;
            mh_proposed.scope = hh_ledger::event::Scope {
                turn_id: Some(mh_chain.turn_id.clone()),
                model_call_id: Some(mh_chain.model_call_id.clone()),
                tool_call_id: Some(tc_elic.clone()),
                ..Default::default()
            };
            let mut intended = self.minter_ev().mint_effect(
                "action.effect.intended",
                events::elicitation_intended_payload(&effect_id, &input_requests, 0),
                &mh_id,
                &mh_chain,
            )?;
            // `causes` names the *ask event* (the ledger coordinate —
            // `decision_id` is a payload member, never an event ref).
            intended.causes = vec![hh_ledger::manifest::EventRef {
                run_id: self.run_id.clone(),
                event_id: ask.event_id.clone(),
            }];
            // `action.effect.input_required{attempt_no}` — the
            // non-terminal pause marker the fold records (`paused_at`);
            // the retry's `committed{attempt+1}` is legal only behind it.
            let input_req = self.minter_ev().mint_effect(
                "action.effect.input_required",
                Json::obj([
                    ("attempt_no", Json::Int(attempt_no as i64)),
                    ("reason", Json::str("protocol_input_required")),
                ]),
                &effect_id,
                &input.chain,
            )?;
            // Two appends: `causes` resolves against committed events
            // only — the `intended` may name the `ask` row only after
            // the ask has landed (batch-internal refs are refused).
            self.store
                .append(&self.run_id, lease, vec![ask, input_req])?;
            self.store
                .append(&self.run_id, lease, vec![mh_proposed, intended])?;
            self.emit_unattributed(&unattributed, lease)?;
            self.minter.expire(&effect_id, attempt_no);
            return Ok(DispatchOutcome::Paused {
                effect_id: effect_id.clone(),
                elicitation_effect_id: mh_id,
                request_state: pause_state,
                input_requests,
            });
        }

        // ── 7 observe ──────────────────────────────────────────────────────
        let (class, origin) = classify_report(&report, input.capability, executor.declaration());
        let kernel_retryable = retryable(&class, origin, &risk);
        let retryable_final = apply_hint(kernel_retryable, report.retryable_hint);
        let outcome = match outcome_for(&report, &diff, origin, &risk) {
            OutcomeMap::Observed(o) => o,
            OutcomeMap::Unknown { cause } => {
                let unknown = self.minter_ev().mint_effect(
                    "action.effect.unknown",
                    events::unknown_payload(attempt_no, lease.generation, &cause),
                    &effect_id,
                    &input.chain,
                )?;
                self.store.append(&self.run_id, lease, vec![unknown])?;
                self.emit_unattributed(&unattributed, lease)?;
                self.minter.expire(&effect_id, attempt_no);
                return Ok(DispatchOutcome::Unknown { cause });
            }
        };
        let status = match &report.status {
            TerminalStatus::Ok => ObservedStatus::Ok,
            TerminalStatus::ToolError { class } => ObservedStatus::Error {
                class: class.clone(),
                origin,
                detail_ref: report.detail_ref.clone(),
                retryable: retryable_final,
            },
            // The paused arm returned above — a paused report never lands
            // on `observed`.
            TerminalStatus::Paused { .. } => {
                unreachable!("paused reports return before observed")
            }
        };
        // ── admission (§5g.2 §2; ADR-0054 D1) ─────────────────────────────
        // A `flow_contract` capability's result is admitted at `L(r) =
        // L⁺(p) ⊔ default_authority(tool, world)`. `realized` is `None` —
        // no result-label channel exists at Stage 2 — so an open-world
        // tool admits `unverified`, `taint = {tool}`, readers from the
        // declared `reads`; the recorded `admission` member is the kind,
        // never a class read from the payload.
        let admission = input
            .capability
            .flow_contract
            .as_ref()
            .and_then(|fc| flow::FlowContract::from_json(fc).ok())
            .map(|contract| {
                let l_plus = flow::prospective_label(
                    &input.context_label,
                    input.flow.param_labels.values().cloned(),
                    &contract.contribution,
                    &input.capability_ref.semantic_id,
                );
                flow::admit(
                    None,
                    &contract,
                    &l_plus,
                    &input.capability_ref.semantic_id,
                    dispatch_world_open(input),
                )
            });
        let obs = Observation {
            outcome,
            status,
            exit_status: report.exit_status,
            manifest_ref,
            completeness,
            admission: admission.clone(),
        };
        // Compute the local-check + declared-postcondition verdicts *before*
        // minting `observed` — `postcondition_results[]` points at the
        // deterministic verdict ids (`verdict:<check>:<effect_id>`;
        // §5f `Effect.postcondition_results: [EventRef]` — S2.11). The rows
        // land via `emit_verdicts` after the batch append.
        let verdicts = self.local_verdicts(
            input,
            &report,
            &class,
            &diff,
            outcome,
            &raw_output,
            handle,
            &effect_id,
        );
        let postcondition_results: Vec<String> =
            verdicts.iter().map(|v| v.verdict_id.clone()).collect();
        let observed = self.minter_ev().mint_effect(
            "action.effect.observed",
            events::observed_payload(attempt_no, lease.generation, &obs, &postcondition_results),
            &effect_id,
            &input.chain,
        )?;
        // `completed` closes the `tool_call` scope — it must not carry
        // `effect_id` (`observed` closes that scope earlier in this batch).
        // The `harness_overhead.execution_ms` M-point rides the executed
        // path's terminal (AC-R-2.5.5-11): the `started → completed` window
        // stratified on the capability's declared `(executor_class,
        // isolation_class)` — the record's own declaration, never the
        // executor's self-report.
        let req = &input.capability.execution_requirement;
        let overhead = events::ExecutionOverhead {
            execution_ms: self.store.now_ms().saturating_sub(exec_started_ms) as i64,
            executor_class: req
                .get("environment_class")
                .and_then(Json::as_str)
                .unwrap_or("unknown")
                .to_string(),
            isolation_class: req
                .get("isolation_min")
                .and_then(Json::as_str)
                .unwrap_or("unknown")
                .to_string(),
        };
        let mut completed = self.minter_ev().mint(
            "action.tool.completed",
            events::tool_completed_payload(
                match &report.status {
                    TerminalStatus::Ok => "ok",
                    TerminalStatus::ToolError { .. } => "error",
                    TerminalStatus::Paused { .. } => "paused",
                },
                None,
                Some(&overhead),
            ),
        )?;
        completed.scope = hh_ledger::event::Scope {
            turn_id: Some(input.chain.turn_id.clone()),
            model_call_id: Some(input.chain.model_call_id.clone()),
            tool_call_id: Some(input.chain.tool_call_id.clone()),
            ..Default::default()
        };
        // `context.observation.recorded{admission}` (§5d.5 observe's
        // second output; §5g.2 §3's payload extension) — the observation-
        // plane record of the admitted result. Emitted only for a
        // `flow_contract` capability (the member is meaningful only where
        // `admit` ran); the admitted label rides as the recorded `label`
        // member — the row's own provenance stays kernel (the kernel's
        // record of the admission, never the content's claim). It precedes
        // `completed` in the batch — `completed` closes the `tool_call`
        // scope this row's scope members name.
        let mut batch = vec![observed];
        if let Some(adm) = &admission {
            let mut recorded = self.minter_ev().mint(
                "context.observation.recorded",
                events::observation_recorded_payload(
                    &effect_id,
                    &input.capability_ref.semantic_id,
                    &obs.manifest_ref,
                    &obs.outcome,
                    adm,
                ),
            )?;
            recorded.scope = hh_ledger::event::Scope {
                turn_id: Some(input.chain.turn_id.clone()),
                model_call_id: Some(input.chain.model_call_id.clone()),
                tool_call_id: Some(input.chain.tool_call_id.clone()),
                ..Default::default()
            };
            batch.push(recorded);
        }
        batch.push(completed);
        self.store.append(&self.run_id, lease, batch)?;
        // KP-7 — runtime death after durable `observed`, before the
        // result is visible/charged (the deposit + verdict tails die with it).
        self.check_kill(&[KillPoint::Kp7])?;
        // K4 deposit — "written by the kernel at `action.tool.completed`
        // when admissible" (§5b.4): only a successful (`ok`), admissible,
        // admission-free observation is written; a refused deposit leaves
        // the next lookup a plain `miss` (fail-safe — never a stale serve).
        self.k4_write(
            input,
            handle,
            &risk,
            &effect_id,
            &args_canonical_hash,
            &obs,
            lease,
        )?;
        self.emit_unattributed(&unattributed, lease)?;
        // ── verification: the kernel local checks (a)–(d) + declared
        // postconditions ride every `observed` terminal (S1.21/S2.11 —
        // ADR-0111 D1/(d)/(e); AC-R-2.7.1-1: `detector = deterministic`,
        // `inputs_digest`, `charged_to = subject`; the appended observation
        // is never touched — I-V3). The `effect`/`tool_call` scopes closed
        // with `observed`/`completed`, so the verdict rows scope to the
        // still-open turn/model_call chain.
        self.emit_verdicts(&verdicts, lease, input)?;
        self.minter.expire(&effect_id, attempt_no);
        Ok(DispatchOutcome::Observed(Box::new(obs)))
    }

    /// The K4 lookup (§5b.4; R-2.3.4¹; ADR-0129 d.1) — called between
    /// `authorized` and `prepare`. Returns `Some(outcome)` when a hit served
    /// the call; `None` when the pipeline continues (`bypass`/`miss`/
    /// `stale_withheld`/`refused` all emit their `model.cache.resolved` row
    /// here — exactly one per lookup, ADR-0128 d.3).
    #[allow(clippy::too_many_arguments)]
    fn k4_lookup(
        &mut self,
        input: &DispatchInput,
        handle: &crate::handle::EnvHandle,
        args_canonical_hash: &str,
        risk: &RiskClass,
        idem_key: &str,
        effect_id: &str,
        lease: &Lease,
    ) -> Result<Option<DispatchOutcome>, EnvError> {
        let k4key = K4Key {
            capability_semantic_id: input.capability_ref.semantic_id.clone(),
            capability_version_id: input.capability_ref.version_id.clone(),
            canonical_args_hash: args_canonical_hash.to_string(),
            environment_ref: env_ref_str(handle),
            readers: input.context_label.readers.clone(),
        };
        let cacheable = k4::declared_cacheable(&input.capability.observation_contract);
        // Resolve under a scoped immutable borrow — the store consultation
        // ends before any mutable self call (the hit path mints + appends).
        let (scope, not_cacheable, resolution) = match self.cache.as_ref() {
            None => return Ok(None),
            Some(c) => (
                c.scope(),
                K4Cache::admissible(risk, cacheable).err(),
                if K4Cache::admissible(risk, cacheable).is_ok() {
                    Some(c.resolve(&k4key, c.scope()))
                } else {
                    None
                },
            ),
        };
        if let Some(nc) = not_cacheable {
            let reason = nc.reason();
            self.emit_cache_resolved(
                input, effect_id, &k4key, scope, "bypass", &reason, None, None, lease,
            )?;
            return Ok(None);
        }
        match resolution.expect("admissible ⇒ resolved") {
            K4Resolution::Hit { version, .. } => {
                let entry_view = K4Cache::hit_entry(&version);
                let obs = entry_view
                    .as_ref()
                    .and_then(|v| Observation::from_json(&v.value));
                let (entry_view, obs) = match (entry_view, obs) {
                    (Some(v), Some(o)) => (v, o),
                    _ => {
                        // A head that cannot rebuild an ordinary Observation
                        // is a refusal, never a partial serve.
                        self.emit_cache_resolved(
                            input,
                            effect_id,
                            &k4key,
                            scope,
                            "refused",
                            "malformed_entry",
                            Some(&version.version_id),
                            None,
                            lease,
                        )?;
                        return Ok(None);
                    }
                };
                self.emit_cache_resolved(
                    input,
                    effect_id,
                    &k4key,
                    scope,
                    "hit",
                    "ok",
                    Some(&version.version_id),
                    entry_view.recorded_usage.clone(),
                    lease,
                )?;
                // `prepared` binds the idempotency key (the hit rides the
                // ordinary lifecycle — intended → authorized → prepared →
                // observed; the read_only class needs no commit).
                let token = self.minter.mint(effect_id, 1, &input.env_handle_id);
                let prepared = self.minter_ev().mint_effect(
                    "action.effect.prepared",
                    events::prepared_payload(
                        idem_key,
                        None,
                        None,
                        &token.hash,
                        input.ladder.effective(),
                        &input.output_policy.policy_ref(),
                    ),
                    effect_id,
                    &input.chain,
                )?;
                let observed = self.minter_ev().mint_effect(
                    "action.effect.observed",
                    // A cache-hit `observed` runs no executor and no checks —
                    // `postcondition_results` is empty (the recorded entry
                    // carries the original run's verdict refs).
                    events::observed_payload(1, lease.generation, &obs, &[]),
                    effect_id,
                    &input.chain,
                )?;
                // A cache-served completion runs no `exec`/`read` — it
                // carries no overhead M-point (never 0: unmeasured is
                // absent, not a zero sample in the distribution).
                let mut completed = self.minter_ev().mint(
                    "action.tool.completed",
                    events::tool_completed_payload("ok", None, None),
                )?;
                completed.scope = hh_ledger::event::Scope {
                    turn_id: Some(input.chain.turn_id.clone()),
                    model_call_id: Some(input.chain.model_call_id.clone()),
                    tool_call_id: Some(input.chain.tool_call_id.clone()),
                    ..Default::default()
                };
                self.store
                    .append(&self.run_id, lease, vec![prepared, observed, completed])?;
                self.dedup.record(idem_key, effect_id);
                self.minter.expire(effect_id, 1);
                let provenance = K4Cache::served_provenance(&version, self.store.now_ms());
                Ok(Some(DispatchOutcome::ObservedCached {
                    observation: Box::new(obs),
                    entry_ref: version.version_id,
                    provenance: Box::new(provenance),
                    avoided: entry_view.recorded_usage,
                }))
            }
            K4Resolution::Miss { .. } => {
                self.emit_cache_resolved(
                    input, effect_id, &k4key, scope, "miss", "no_entry", None, None, lease,
                )?;
                Ok(None)
            }
            K4Resolution::StaleWithheld {
                version_id, reason, ..
            } => {
                self.emit_cache_resolved(
                    input,
                    effect_id,
                    &k4key,
                    scope,
                    "stale_withheld",
                    &reason,
                    Some(&version_id),
                    None,
                    lease,
                )?;
                Ok(None)
            }
            K4Resolution::Refused { reason, .. } => {
                self.emit_cache_resolved(
                    input, effect_id, &k4key, scope, "refused", &reason, None, None, lease,
                )?;
                Ok(None)
            }
        }
    }

    /// The K4 deposit at `action.tool.completed` (§5b.4 — "written by the
    /// kernel … when admissible"). Only a successful (`ObservedStatus::Ok`),
    /// admissible, admission-free observation is deposited — an `admission`
    /// member is not rebuildable by `Observation::from_json`, so the entry
    /// is never written rather than written lossy. A refused deposit is not
    /// a dispatch failure: the observation already landed and the next
    /// lookup simply misses (fail-safe — never a stale serve).
    #[allow(clippy::too_many_arguments)]
    fn k4_write(
        &mut self,
        input: &DispatchInput,
        handle: &crate::handle::EnvHandle,
        risk: &RiskClass,
        effect_id: &str,
        args_canonical_hash: &str,
        obs: &Observation,
        lease: &Lease,
    ) -> Result<(), EnvError> {
        if self.cache.is_none()
            || !matches!(obs.status, ObservedStatus::Ok)
            || obs.admission.is_some()
        {
            return Ok(());
        }
        let cacheable = k4::declared_cacheable(&input.capability.observation_contract);
        if K4Cache::admissible(risk, cacheable).is_err() {
            return Ok(());
        }
        let k4key = K4Key {
            capability_semantic_id: input.capability_ref.semantic_id.clone(),
            capability_version_id: input.capability_ref.version_id.clone(),
            canonical_args_hash: args_canonical_hash.to_string(),
            environment_ref: env_ref_str(handle),
            readers: input.context_label.readers.clone(),
        };
        let scope = self.cache.as_ref().expect("checked is_none above").scope();
        let entry = K4Entry {
            value_ref: obs.manifest_ref.clone(),
            value: obs.to_json(),
            // The write's provenance is the producing call's `tool` origin —
            // a served hit re-origins to `cache(entry_ref)` (I-NOAUTH: the
            // served record carries the entry's label, never widened).
            provenance: ProvenanceRecord::minted(
                Origin::tool(
                    input.capability_ref.version_id.clone(),
                    effect_id.to_string(),
                ),
                scope,
                self.store.now_ms(),
            ),
            recorded_usage: None,
            external_dep: None,
            valid_until: None,
        };
        let written = {
            let cache = self.cache.as_mut().expect("checked is_none above");
            let lease_generation = cache.store_mut().lease(scope);
            let ctx = WriteContext {
                context_label: input.context_label.clone(),
                lease_generation,
                at_seq: cache.store().applied_seq() + 1,
                run_id: self.run_id.clone(),
            };
            cache.write(&k4key, &entry, scope, &ctx)
        };
        // A refused deposit (`NotCacheable`, `CapabilityVersionDrift`, a
        // store refusal) is swallowed by design — the observation is
        // durable, the miss cost is next call's. A successful deposit lands
        // its `context.memory.written` row scoped to the still-open
        // turn/model_call chain (the `effect`/`tool_call` scopes closed with
        // `observed`/`completed`).
        if let Ok(w) = written {
            let (class, payload) = w.memory_event;
            let mut ev = self.minter_ev().mint(&class, payload)?;
            ev.scope = hh_ledger::event::Scope {
                turn_id: Some(input.chain.turn_id.clone()),
                model_call_id: Some(input.chain.model_call_id.clone()),
                ..Default::default()
            };
            self.store.append(&self.run_id, lease, vec![ev])?;
        }
        Ok(())
    }

    /// `model.cache.resolved` — the one-row-per-lookup record (ADR-0128 d.3).
    /// Effect-scoped: the lookup is a fact inside this effect's trail
    /// (minted while the effect scope is open, post-`authorized`).
    #[allow(clippy::too_many_arguments)]
    fn emit_cache_resolved(
        &mut self,
        input: &DispatchInput,
        effect_id: &str,
        key: &K4Key,
        scope: PersistenceScope,
        outcome: &str,
        reason: &str,
        entry_ref: Option<&str>,
        avoided: Option<Json>,
        lease: &Lease,
    ) -> Result<(), EnvError> {
        let ev = self.minter_ev().mint_effect(
            "model.cache.resolved",
            K4Cache::resolved_payload(
                key,
                scope,
                outcome,
                reason,
                entry_ref,
                avoided,
                Json::str(input.chain.tool_call_id.clone()),
            ),
            effect_id,
            &input.chain,
        )?;
        self.store.append(&self.run_id, lease, vec![ev])?;
        Ok(())
    }

    /// The S1.21/S2.11 local-check fold: build the `TerminalCapture` and run
    /// the kernel's built-in checks (a)–(d) + the capability's declared
    /// `postconditions[]` over it — **pure** (no store writes), so the caller
    /// can stamp the verdict ids onto `action.effect.observed`'s
    /// `postcondition_results[]` before the verdict rows land (the ids are
    /// deterministic — `verdict:<check>:<effect_id>`). `handle` supplies the
    /// sealed `Permission` scope facts for check (d) (kernel-derived, never
    /// executor-reported).
    #[allow(clippy::too_many_arguments)]
    fn local_verdicts(
        &self,
        input: &DispatchInput,
        report: &TerminalReport,
        report_class: &ErrorClass,
        diff: &crate::snapshot::FsChangeSet,
        outcome: EffectOutcome,
        raw_output: &str,
        handle: &crate::handle::EnvHandle,
        effect_id: &str,
    ) -> Vec<hh_verification::validators::Verdict> {
        let cap = hh_verification::validators::TerminalCapture {
            effect_id: effect_id.to_string(),
            domain: Some(input.declared.domain),
            output: if raw_output.is_empty() {
                Json::Null
            } else {
                Json::Str(raw_output.to_string())
            },
            output_schema: input.capability.output_schema.clone(),
            exit_status: report.exit_status,
            timed_out: matches!(report_class, ErrorClass::Timeout),
            signalled: matches!(report_class, ErrorClass::Signalled { .. }),
            patch_status: match input.declared.domain {
                EffectDomain::FsWrite => Some(match outcome {
                    EffectOutcome::Applied => hh_verification::validators::PatchStatus::Applied,
                    EffectOutcome::Partial => hh_verification::validators::PatchStatus::Partial,
                    _ => hh_verification::validators::PatchStatus::Rejected(0),
                }),
                _ => None,
            },
            touched_paths: diff
                .entries()
                .map(|(e, _)| e.path_canonical.clone())
                .collect(),
            resource_keys: match &input.capability.resources {
                hh_hir::records::Resources::Declared(keys) => keys.iter().cloned().collect(),
                _ => Vec::new(),
            },
            // (d) inputs — kernel-derived diff facts (`FsWrite` only; other
            // domains carry no diff ⇒ the check does not apply).
            diff_files: match input.declared.domain {
                EffectDomain::FsWrite => Some(diff.entries().count() as u64),
                _ => None,
            },
            diff_lines: None,
            diff_bytes: None,
            outside_scope: diff
                .entries()
                .filter(|(e, _)| !handle.roots.is_writable(&e.path_canonical))
                .map(|(e, _)| e.path_canonical.clone())
                .collect(),
        };
        let now = self.store.now_ms();
        let head = self
            .store
            .events(&self.run_id)
            .ok()
            .and_then(|evs| evs.last().map(|e| e.seq))
            .unwrap_or(0);
        let provenance = ProvenanceRecord::kernel(crate::events::COMPONENT, now);
        let mut verdicts = hh_verification::validators::run_local_checks(
            &cap,
            &local_checks_ref(),
            self.diff_sanity.as_ref(),
            provenance.clone(),
            head,
            now,
        );
        // The declared `postconditions[]` (§5f — `Ref<Validator>` bound and
        // run per applicable terminal; DF-S1.21-1's Stage-2 half): each ref
        // resolves to a kernel built-in or yields an `inconclusive` verdict
        // naming it (declared-but-unbound is reported, never skipped).
        for pref in &input.capability.postconditions {
            if let Some(v) = hh_verification::validators::run_declared_postcondition(
                pref,
                &cap,
                self.diff_sanity.as_ref(),
                provenance.clone(),
                head,
                now,
            ) {
                verdicts.push(v);
            }
        }
        verdicts
    }

    /// Append `verification.validator.invoked` + `verification.validator.verdict`
    /// per precomputed verdict (`phase = local`, `detector = deterministic`,
    /// `charged_to = subject` — ADR-0111 D1). No verdicts ⇒ no rows (a
    /// refused/never-captured terminal emits nothing — "exactly the
    /// applicable" is the AC).
    fn emit_verdicts(
        &mut self,
        verdicts: &[hh_verification::validators::Verdict],
        lease: &Lease,
        input: &DispatchInput,
    ) -> Result<(), EnvError> {
        if verdicts.is_empty() {
            return Ok(());
        }
        let mut rows = Vec::with_capacity(verdicts.len() * 2);
        {
            let m = self.minter_ev();
            for v in verdicts {
                let mut invoked = m.mint(
                    "verification.validator.invoked",
                    hh_verification::events::validator_invoked(
                        v,
                        &hh_verification::vocab::Isolation::Kernel,
                    ),
                )?;
                let mut verdict = m.mint(
                    "verification.validator.verdict",
                    hh_verification::events::validator_verdict(v),
                )?;
                // `effect`/`tool_call` closed — the rows scope to the
                // still-open turn/model_call chain.
                for e in [&mut invoked, &mut verdict] {
                    e.scope = hh_ledger::event::Scope {
                        turn_id: Some(input.chain.turn_id.clone()),
                        model_call_id: Some(input.chain.model_call_id.clone()),
                        ..Default::default()
                    };
                }
                rows.push(invoked);
                rows.push(verdict);
            }
        }
        self.store.append(&self.run_id, lease, rows)?;
        Ok(())
    }

    /// Append one `action.effect.unattributed` marker per unresolvable signal
    /// hash (the capture-path signal that resolved to no effect).
    fn emit_unattributed(&mut self, hashes: &[String], lease: &Lease) -> Result<(), EnvError> {
        let mut evs = Vec::new();
        for h in hashes {
            evs.push(self.minter_ev().mint(
                "action.effect.unattributed",
                events::unattributed_payload("executor_signal", h, "token_resolve_miss"),
            )?);
        }
        if !evs.is_empty() {
            self.store.append(&self.run_id, lease, evs)?;
        }
        Ok(())
    }

    /// `expire_permission(permission_id, now)` — the `TimeoutPolicy[permission]`
    /// terminal (§5e.2; ADR-0070 D3; S2.11): a durable
    /// `security.permission.pending` whose `requested_at + timeout ≤ now`
    /// resolves `security.permission.decided{decision: timed_out, decider:
    /// policy}` → `action.effect.refused{permission_timed_out}` — a refusal
    /// record, never `unknown`. The pending row's own scope supplies the
    /// chain (the refusal closes the effect scope the ask opened); a landed
    /// final `decided` stands (the exactly-one gate).
    pub fn expire_permission(
        &mut self,
        permission_id: &str,
        lease: &Lease,
    ) -> Result<PermissionExpiry, EnvError> {
        let now = self.store.now_ms();
        let events = self.store.events(&self.run_id)?;
        // The pending row for `permission_id` — its scope supplies the chain.
        let pending = events.iter().find(|e| {
            e.class == "security.permission.pending"
                && e.payload.get("permission_id").and_then(Json::as_str) == Some(permission_id)
        });
        let Some(pending) = pending else {
            return Ok(PermissionExpiry::NotFound {
                permission_id: permission_id.to_string(),
            });
        };
        // A landed final `decided` stands (exactly-one — `timed_out` never
        // rewrites history). `ask` rows are non-final and don't gate.
        let already = events.iter().any(|e| {
            e.class == "security.permission.decided"
                && e.payload.get("permission_id").and_then(Json::as_str) == Some(permission_id)
                && e.payload.get("decision").and_then(Json::as_str) != Some("ask")
        });
        if already {
            return Ok(PermissionExpiry::AlreadyDecided {
                permission_id: permission_id.to_string(),
            });
        }
        let requested_at = pending
            .payload
            .get("requested_at")
            .and_then(Json::as_int)
            .map(|t| t.max(0) as u64)
            .unwrap_or(now);
        let Some(timeout) = pending
            .payload
            .get("timeout")
            .and_then(Json::as_int)
            .map(|t| t.max(0) as u64)
        else {
            return Ok(PermissionExpiry::NoDeadline {
                permission_id: permission_id.to_string(),
            });
        };
        let deadline = requested_at.saturating_add(timeout);
        if now < deadline {
            return Ok(PermissionExpiry::NotElapsed {
                permission_id: permission_id.to_string(),
                remaining_ms: deadline - now,
            });
        }
        let chain = ScopeChain {
            turn_id: pending.scope.turn_id.clone().unwrap_or_default(),
            model_call_id: pending.scope.model_call_id.clone().unwrap_or_default(),
            tool_call_id: pending.scope.tool_call_id.clone().unwrap_or_default(),
        };
        let effect_id = pending.scope.effect_id.clone().unwrap_or_default();
        let decided = self.minter_ev().mint(
            "security.permission.decided",
            Json::obj([
                ("permission_id", Json::str(permission_id.to_string())),
                ("decision", Json::str("timed_out")),
                ("decider", Json::str("policy")),
                ("decision_scope", Json::str("once")),
                ("requested_at", Json::Int(requested_at as i64)),
                (
                    "wait_ms",
                    Json::Int(now.saturating_sub(requested_at) as i64),
                ),
                ("reason", Json::str("timed_out")),
                ("effect_id", Json::str(effect_id.clone())),
            ]),
        )?;
        let refused = self.minter_ev().mint_effect(
            "action.effect.refused",
            events::refused_payload("permission_timed_out"),
            &effect_id,
            &chain,
        )?;
        self.store
            .append(&self.run_id, lease, vec![decided, refused])?;
        Ok(PermissionExpiry::Expired {
            permission_id: permission_id.to_string(),
        })
    }

    /// `probe(effect_id, attempt_no, executor, chain)` — a lapsed-window /
    /// `unknown` effect is probed, never redispatched (AC-R-2.5.5-9). The
    /// verdict is the executor's `probe`; a `probe_support = none` executor
    /// yields `undeterminable`.
    pub fn probe(
        &mut self,
        executor: &dyn ToolExecutor,
        lease: &Lease,
        effect_id: &str,
        attempt_no: u64,
        chain: &ScopeChain,
    ) -> Result<ProbeVerdict, EnvError> {
        let verdict = executor
            .probe(effect_id, attempt_no)
            .unwrap_or(ProbeVerdict::Undeterminable);
        let ev = self.minter_ev().mint_effect(
            "action.effect.probed",
            events::probed_payload(lease.generation, verdict.as_str()),
            effect_id,
            chain,
        )?;
        self.store.append(&self.run_id, lease, vec![ev])?;
        Ok(verdict)
    }
}

/// `OutcomeMap` — the observe stage's outcome decision.
/// The observe stage's outcome decision.
pub enum OutcomeMap {
    /// `observed{outcome}` settles.
    Observed(EffectOutcome),
    /// `unknown{cause}` — probe, never redispatch.
    Unknown {
        /// The closed `UNKNOWN_CAUSES` tag.
        cause: String,
    },
}

impl OutcomeMap {
    /// The settled outcome, when `Observed`.
    pub fn observed(&self) -> Option<EffectOutcome> {
        match self {
            OutcomeMap::Observed(o) => Some(*o),
            _ => None,
        }
    }
}

/// `classify_report(report, capability, decl)` — the surjective `(ErrorClass,
/// ErrorOrigin)` mapping every executor failure lands in (AC-R-2.5.5-3). The
/// terminal report's `status` is the tool's own result (`origin = tool`); a
/// transport-plane failure (`Err` from `execute`) is mapped by the caller to
/// `executor_error`/`transport`. A tool-reported class outside the declared
/// `error_classes` is a `protocol_error` — the report itself is malformed.
pub fn classify_report(
    report: &TerminalReport,
    capability: &ToolCapabilityRecord,
    decl: &ExecutorDeclaration,
) -> (ErrorClass, ErrorOrigin) {
    match &report.status {
        TerminalStatus::Ok => (ErrorClass::ExecutorError, ErrorOrigin::Tool), // unused on ok
        // `paused` is handled before classify — a paused report never maps
        // to an error class (§5d.4 D3: `input_required` is not a failure).
        TerminalStatus::Paused { .. } => (ErrorClass::ExecutorError, ErrorOrigin::Tool),
        TerminalStatus::ToolError { class } => {
            let declared_ok = decl.error_classes.contains(class.as_str())
                || capability_error_classes(capability).contains(class.as_str());
            if declared_ok {
                (class.clone(), ErrorOrigin::Tool)
            } else {
                (ErrorClass::ProtocolError, ErrorOrigin::Transport)
            }
        }
    }
}

/// The capability's declared `error_classes` (`observation_contract.
/// error_classes ⊆ ErrorClass`).
fn capability_error_classes(c: &ToolCapabilityRecord) -> BTreeSet<String> {
    c.observation_contract
        .get("error_classes")
        .and_then(|j| match j {
            Json::Arr(a) => Some(
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

/// `outcome_for(report, diff, origin, risk)` — the lifecycle mapping
/// (ADR-0102 D3): a tool error is `not_applied|partial` by the fs diff for
/// `read_only`/`reversible`; a transport failure is `not_applied` only when
/// non-dispatch is proven, else `unknown` → probe; an execution failure is
/// `unknown` → probe.
pub fn outcome_for(
    report: &TerminalReport,
    diff: &crate::snapshot::FsChangeSet,
    origin: ErrorOrigin,
    risk: &RiskClass,
) -> OutcomeMap {
    match &report.status {
        TerminalStatus::Ok => OutcomeMap::Observed(EffectOutcome::Applied),
        // Unreachable — the paused arm short-circuits before `outcome_for`.
        TerminalStatus::Paused { .. } => OutcomeMap::Unknown {
            cause: "executor_error".to_string(),
        },
        TerminalStatus::ToolError { .. } => match origin {
            ErrorOrigin::Tool => {
                // The tool's own result — read_only/reversible settle
                // `not_applied|partial` by the diff; compensable/irreversible
                // need a probe to settle `not_applied`, else `unknown`.
                if risk.is_read_only() || risk.reversibility == RiskReversibility::Reversible {
                    OutcomeMap::Observed(if diff.is_empty() {
                        EffectOutcome::NotApplied
                    } else {
                        EffectOutcome::Partial
                    })
                } else {
                    OutcomeMap::Unknown {
                        cause: "executor_error".to_string(),
                    }
                }
            }
            ErrorOrigin::Execution => OutcomeMap::Unknown {
                cause: match &report.status {
                    TerminalStatus::ToolError {
                        class: ErrorClass::Timeout,
                    } => "timeout",
                    TerminalStatus::ToolError {
                        class: ErrorClass::Cancelled { .. },
                    } => "cancelled",
                    _ => "executor_error",
                }
                .to_string(),
            },
            ErrorOrigin::Transport => OutcomeMap::Unknown {
                cause: "executor_error".to_string(),
            },
        },
    }
}

/// The minimal closed-grammar command classifier — the `command_parse`
/// assessor input. At Stage 1 the grammar is: an `argv` member ⇒ `Parsed`; a
/// `command` string ⇒ `Parsed` unless it carries shell metachars the grammar
/// doesn't cover (`;`, `|`, `>`, backticks, `$(`) ⇒ `Partial`; neither ⇒
/// `Failed` (the parse's *output*, never the text — ADR-0212/OQ-133).
fn classify_command(args: &Json) -> ParseOutcome {
    if args.get("argv").is_some() {
        return ParseOutcome::Parsed;
    }
    match args.get("command").and_then(Json::as_str) {
        Some(c) if !c.is_empty() => {
            if c.contains(';')
                || c.contains('|')
                || c.contains('>')
                || c.contains('`')
                || c.contains("$(")
            {
                ParseOutcome::Partial
            } else {
                ParseOutcome::Parsed
            }
        }
        _ => ParseOutcome::Failed,
    }
}

/// `world = open` for the dispatched effect — the declared attributes or
/// the capability's declared attributes for the domain (the same source
/// the monitor's flow stage reads — one definition of "open").
fn dispatch_world_open(input: &DispatchInput) -> bool {
    let declared_attrs = match &input.capability.effects {
        hh_hir::kinds::ToolEffects::Pure => None,
        hh_hir::kinds::ToolEffects::Declared(set) => set
            .iter()
            .find(|e| e.domain == input.declared.domain)
            .and_then(|e| e.attributes.as_ref()),
    };
    input
        .declared
        .attributes
        .iter()
        .chain(declared_attrs)
        .any(|a| a.world == hh_hir::kinds::World::Open)
}

/// `canonical_json(canonical)` — the `params` map as a `Json` object (the
/// `args` the executor receives).
fn canonical_json(canonical: &CanonicalArgs) -> Json {
    Json::Obj(canonical.params.clone())
}

/// The pinned `validator_ref` the kernel local checks (a)–(c) run under —
/// `hir/kernel/local_checks@1`, a built-in `Validator` node in the reference
/// dialect (ADR-0111 D1: built-ins are `hir/kernel/<check>` nodes so they
/// appear in `lcd_report.conditioned_rules` and are ablatable). The version is
/// an `inputs_digest` input (AC-R-2.7.1-4's purity reads it).
fn local_checks_ref() -> hh_identity::refs::VersionedRef {
    hh_identity::refs::VersionedRef::pinned(
        hh_identity::kinds::RecordKind::Validator,
        "hir/kernel/local_checks@1",
        ProvenanceRecord::kernel("hir/kernel/local_checks", 0),
    )
}

/// Build the `terminal` capture item from the executor's report.
fn terminal_item(
    report: &TerminalReport,
    effect_id: &str,
    attempt_no: u64,
    execution_id: &str,
    seq: u64,
    chain: &ScopeChain,
    run_id: &str,
) -> CaptureItem {
    let (status, retry_hint, detail) = match &report.status {
        TerminalStatus::Ok => ("ok", None, None),
        TerminalStatus::ToolError { .. } => {
            ("error", report.retryable_hint, report.detail_ref.clone())
        }
        TerminalStatus::Paused { .. } => ("paused", None, None),
    };
    CaptureItem {
        run_id: run_id.to_string(),
        turn_id: chain.turn_id.clone(),
        model_call_id: chain.model_call_id.clone(),
        tool_call_id: chain.tool_call_id.clone(),
        effect_id: effect_id.to_string(),
        attempt_no,
        execution_id: execution_id.to_string(),
        seq,
        ts_mono: seq,
        source: CaptureSource::ExecutorReported,
        kind: CaptureKind::Terminal {
            status: status.to_string(),
            exit_status: report.exit_status,
            executor_outcome_hint: report.outcome_hint.clone(),
            retryable_hint: retry_hint,
            detail_ref: detail,
        },
        provenance: "executor".to_string(),
    }
}

/// `redact_item(item, detectors)` — mask a captured payload's string members
/// through the detector set (the capture-path redaction — a hostile executor's
/// output is masked before any visibility; the tombstones are the audit
/// detail, never the value). Returns the detector kinds that fired (the
/// caller raises `leak_detected` for non-placeholder hits).
fn redact_item(
    item: &mut CaptureItem,
    detectors: &DetectorSet,
) -> Vec<hh_secrets::redact::DetectorKind> {
    let mut kinds = Vec::new();
    if let CaptureKind::OutputChunk { data, .. } = &mut item.kind {
        let (masked, hits) = redact(data, detectors);
        *data = masked;
        kinds.extend(hits.iter().map(|h| h.detector));
    }
    if let CaptureKind::Terminal {
        detail_ref: Some(d),
        ..
    } = &mut item.kind
    {
        let (masked, hits) = redact(d, detectors);
        *d = masked;
        kinds.extend(hits.iter().map(|h| h.detector));
    }
    kinds
}

/// `check_state_preconditions` — the E1 `state_requires`/`PreconditionDomain`
/// evaluation the dispatcher runs at `prepare` (§5d.1; S2.11): each declared
/// domain tag is checked against kernel-derived state — a domain the runtime
/// cannot verify is `violated`, never silently passed. Returns the violated
/// kind (`PreconditionViolated{kind}`) on the first failure, in declaration
/// order (deterministic).
fn check_state_preconditions(
    input: &DispatchInput,
    handle: &crate::handle::EnvHandle,
    now: u64,
) -> Result<(), &'static str> {
    for d in &input.capability.preconditions {
        match d {
            // `network` — the environment admits no egress at this tier
            // (R-NONET): a declared reachability precondition is
            // unverifiable ⇒ violated (fail-closed).
            hh_hir::kinds::PreconditionDomain::Network => return Err("network"),
            // `filesystem` — every declared workspace/writable root must
            // exist (the attach-time admission proved scope; `prepare`
            // re-checks presence — a root that vanished since is `stale`).
            hh_hir::kinds::PreconditionDomain::Filesystem => {
                let missing = handle
                    .roots
                    .workspace_roots
                    .iter()
                    .chain(handle.roots.writable_roots.iter())
                    .any(|r| !std::path::Path::new(r).exists());
                if missing {
                    return Err("filesystem");
                }
            }
            // `credentials` — no secret provision is in the dispatch input:
            // a declared credential precondition is unverifiable ⇒ violated.
            hh_hir::kinds::PreconditionDomain::Credentials => return Err("credentials"),
            // `environment` — `handle.verify_environment()` ran at dispatch
            // entry (the handle would not be `ready` otherwise).
            hh_hir::kinds::PreconditionDomain::Environment => {}
            // `temporal` — the effective deadline must not already be past.
            hh_hir::kinds::PreconditionDomain::Temporal => {
                if input.ladder.expired(now) {
                    return Err("temporal");
                }
            }
            // `capability` — the depends-on edge is the `link` check point
            // (compiler); re-checking is bind's `capability_requires`.
            hh_hir::kinds::PreconditionDomain::Capability => {}
        }
    }
    Ok(())
}

/// `PermissionExpiry` — the `expire_permission` outcome (typed, never a
/// silent no-op).
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionExpiry {
    /// The pending timed out — `decided{timed_out}` + `refused` appended.
    Expired { permission_id: String },
    /// No pending row for the id (already resolved or never opened).
    NotFound { permission_id: String },
    /// The pending's window has not elapsed (`requested_at + timeout > now`).
    NotElapsed {
        /// The request id.
        permission_id: String,
        /// The remaining ms.
        remaining_ms: u64,
    },
    /// A final `decided` row already covers the id (the exactly-one gate —
    /// a `timed_out` never rewrites a landed decision).
    AlreadyDecided { permission_id: String },
    /// The pending carries no `timeout` member (attended `deadline none` —
    /// nothing to expire).
    NoDeadline { permission_id: String },
}
