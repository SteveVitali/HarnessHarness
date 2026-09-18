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
use hh_hir::kinds::{EffectClass, EffectDomain};
use hh_hir::records::{Grant, ScopeBindings, ToolCapabilityRecord};
use hh_hir::risk::project_risk;
use hh_ledger::effect::idempotency_key;
use hh_ledger::errors::LedgerError;
use hh_ledger::store::{Lease, Store};
use hh_monitor::args::CanonicalArgs;
use hh_monitor::assess::{kernel_assessed, AssessmentInputs, ParseOutcome, Tri};
use hh_monitor::decision::Decision;
use hh_monitor::events::decided_payload;
use hh_monitor::monitor::{Monitor, Proposal};
use hh_ontology::risk::{RiskClass, RiskReversibility};
use hh_provenance::{Label, ProvenanceRecord};
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
    },
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
        }
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
        reserve_src: Option<&mut dyn ReservationSource>,
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

        // The arg-map eval — `UnmappedArgument`/`UnscopedParameter` is a
        // `resolve`-stage refusal (`action.tool.rejected{source: args}`).
        let canonical = match hh_monitor::args::eval(
            input.binding,
            input.scope_bindings,
            &input.surface_args,
        ) {
            Ok(c) => c,
            Err(e) => {
                let ev = self.minter_ev().mint_effect(
                    "action.tool.rejected",
                    events::tool_rejected_payload("args", &format!("{e:?}")),
                    "",
                    &input.chain,
                )?;
                self.store.append(&self.run_id, lease, vec![ev])?;
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
            ..AssessmentInputs::default()
        };
        let assessed = kernel_assessed(
            input.declared.attributes.as_ref(),
            input.declared.domain == EffectDomain::FsWrite,
            &inputs,
        );
        let effect_id = Store::effect_id(
            &self.run_id,
            &input.chain.model_call_id,
            &input.chain.tool_call_id,
            input.ordinal,
        );

        // The dedup gate — a replayed idempotency key never redispatches.
        let key = idempotency_key(
            &self.run_id,
            &effect_id,
            &args_canonical_hash,
            &input.capability_ref.version_id,
        );
        if let Some(prior) = self.dedup.lookup(&key) {
            let prior = prior.to_string();
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
            });
        }

        // `action.tool.proposed` opens the tool_call scope; `intended` opens
        // the effect scope under it.
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
            at: self.store.now_ms(),
        };
        let decision = self
            .monitor
            .authorize(&proposal)
            .map_err(|e| EnvError::Blob(format!("authorize: {e:?}")))?;
        let decided = self.minter_ev().mint(
            "security.permission.decided",
            decided_payload(&decision, 1, "proposal"),
        )?;
        self.store.append(&self.run_id, lease, vec![decided])?;
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
            Decision::Ask { .. } => {
                // Stage 1 has no mid-dispatch human round-trip — the control
                // envelope owns the ask loop; the dispatcher refuses
                // (`refused{ask_required}`).
                let refused = self.minter_ev().mint_effect(
                    "action.effect.refused",
                    events::refused_payload("ask_required"),
                    &effect_id,
                    &input.chain,
                )?;
                let rejected = tool_rejected(&self.minter_ev(), "ask_required")?;
                self.store
                    .append(&self.run_id, lease, vec![refused, rejected])?;
                return Ok(DispatchOutcome::Refused {
                    reason: "ask_required".to_string(),
                });
            }
            Decision::Allow => {}
        }
        let authorized = self.minter_ev().mint_effect(
            "action.effect.authorized",
            events::authorized_payload(&risk),
            &effect_id,
            &input.chain,
        )?;
        self.store.append(&self.run_id, lease, vec![authorized])?;

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

        // ── 4 commit ───────────────────────────────────────────────────────
        // `read_only` has no write-ahead (observed may follow prepared);
        // every other class commits under the fencing token — and the ledger
        // refuses `committed` without a preceding allow (complete mediation).
        let attempt_no = 1u64;
        if !risk.is_read_only() {
            let committed = self.minter_ev().mint_effect(
                "action.effect.committed",
                events::committed_payload(attempt_no, lease.generation, self.store.now_ms()),
                &effect_id,
                &input.chain,
            )?;
            self.store.append(&self.run_id, lease, vec![committed])?;
        }
        let execution_id = self.store.alloc_id("exec");
        let started = self.minter_ev().mint_effect(
            "action.tool.started",
            events::tool_started_payload(&execution_id, &token.hash),
            &effect_id,
            &input.chain,
        )?;
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
            args: canonical_json(&canonical),
            env_handle_id: input.env_handle_id.clone(),
            attribution_token: token.token.clone(),
            deadline_ms: input.ladder.effective(),
            ladder: input.ladder,
            retain_bytes_cap: input.output_policy.retain_bytes_cap,
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
            Ok(r) => r,
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
        };
        let obs = Observation {
            outcome,
            status,
            exit_status: report.exit_status,
            manifest_ref,
            completeness,
        };
        let observed = self.minter_ev().mint_effect(
            "action.effect.observed",
            events::observed_payload(attempt_no, lease.generation, &obs),
            &effect_id,
            &input.chain,
        )?;
        // `completed` closes the `tool_call` scope — it must not carry
        // `effect_id` (`observed` closes that scope earlier in this batch).
        let mut completed = self.minter_ev().mint(
            "action.tool.completed",
            events::tool_completed_payload(
                match &report.status {
                    TerminalStatus::Ok => "ok",
                    TerminalStatus::ToolError { .. } => "error",
                },
                None,
            ),
        )?;
        completed.scope = hh_ledger::event::Scope {
            turn_id: Some(input.chain.turn_id.clone()),
            model_call_id: Some(input.chain.model_call_id.clone()),
            tool_call_id: Some(input.chain.tool_call_id.clone()),
            ..Default::default()
        };
        self.store
            .append(&self.run_id, lease, vec![observed, completed])?;
        self.emit_unattributed(&unattributed, lease)?;
        // ── verification: the kernel local checks (a)–(c) ride every
        // `observed` terminal (S1.21 — ADR-0111 D1/(e); AC-R-2.7.1-1:
        // `detector = deterministic`, `inputs_digest`, `charged_to =
        // subject`; the appended observation is never touched — I-V3;
        // (d) `diff_sanity` is Stage 2). The `effect`/`tool_call` scopes
        // closed with `observed`/`completed`, so the verdict rows scope
        // to the still-open turn/model_call chain.
        self.emit_local_verdicts(
            lease,
            input,
            &report,
            &class,
            &diff,
            outcome,
            &raw_output,
            &effect_id,
        )?;
        self.minter.expire(&effect_id, attempt_no);
        Ok(DispatchOutcome::Observed(Box::new(obs)))
    }

    /// The S1.21 local-check emission: run the kernel's built-in checks
    /// (a)–(c) over the terminal capture and append
    /// `verification.validator.invoked` + `verification.validator.verdict`
    /// per applicable check (`phase = local`, `detector = deterministic`,
    /// `charged_to = subject` — ADR-0111 D1). No applicable check ⇒ no rows
    /// (a refused/never-captured terminal emits nothing — "exactly the
    /// applicable" is the AC).
    #[allow(clippy::too_many_arguments)]
    fn emit_local_verdicts(
        &mut self,
        lease: &Lease,
        input: &DispatchInput,
        report: &TerminalReport,
        report_class: &ErrorClass,
        diff: &crate::snapshot::FsChangeSet,
        outcome: EffectOutcome,
        raw_output: &str,
        effect_id: &str,
    ) -> Result<(), EnvError> {
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
        };
        let now = self.store.now_ms();
        let head = self
            .store
            .events(&self.run_id)
            .ok()
            .and_then(|evs| evs.last().map(|e| e.seq))
            .unwrap_or(0);
        let verdicts = hh_verification::validators::run_local_checks(
            &cap,
            &local_checks_ref(),
            ProvenanceRecord::kernel(crate::events::COMPONENT, now),
            head,
            now,
        );
        if verdicts.is_empty() {
            return Ok(());
        }
        let mut rows = Vec::with_capacity(verdicts.len() * 2);
        {
            let m = self.minter_ev();
            for v in &verdicts {
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
