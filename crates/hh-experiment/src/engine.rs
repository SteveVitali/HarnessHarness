//! `ExperimentEngine` — the C0/Stage-3 single-worker experiment engine over a
//! `run_kind = experiment` ledger run (spec §6.3 §2.2–§2.5; R-2.10.3⁰ᵇ;
//! ADR-0154/0155/0156/0162/0213; S3.4a).
//!
//! One durable home: the experiment run's ledger stream carries the whole
//! scheduling truth (`declared`, `run_planned`, `run_claimed`,
//! `claim_expired`, `run_launched`, `run_bound`, `run_settled`,
//! `run_excluded`, `run_replanned`, `cell_completed`, `paused`, `resumed`,
//! `closed`, `drift_bracket`, `amended`, `bundle_assembled`); the engine holds
//! no memory-only state — every op re-projects [`ExperimentView`] from the
//! committed stream (S-1), so a `kill -9` at any point followed by
//! `Store::open` + [`ExperimentEngine::attach`] sees exactly the committed
//! state (KP-E1…E5).
//!
//! - `register` gates on the closed E-1 refusal set (`ExperimentSpec::register`
//!   against the engine's [`EngineContext`] resolvers) and durably stores the
//!   spec in [`LabDocs`] — a refusal is typed, never a warning (T-LCD-14).
//! - `expand` re-runs the refusal gate, then calls the pure
//!   `hh_lab::expand::expand` and stores the content-addressed `CellPlan`.
//! - `open_experiment` mints the `run_kind = experiment` run under the
//!   engine's fenced writer, allocates the experiment budget root, and commits
//!   `declared` + one `run_planned` per `RunPlan` + the `opened` drift bracket
//!   in one batch.
//! - `next`/`claim` hand out plans in recorded order; claims are
//!   `lifecycle.lease.*` scoped leases over `resource(run_plan_id)` with
//!   reconcile-before-dispatch expiry (`claim_expired`).
//! - `launch` checks the claim, probes drift (`DependsOnDriftedCapability`,
//!   `ModelFingerprintDrift`), allocates a `slice` child off the experiment
//!   pool (`InsufficientBudget` leaves the plan re-plannable), opens the
//!   subject `agent` run with the full `experiment{…}` row-key binding, and
//!   commits `run_launched` + the `run_bound` mirror.
//! - `settle` is exactly-once (S-2): a settled plan returns its recorded
//!   `RunOutcome`; an unsettled finished subject run is projected to its
//!   `OutcomeClass` (ADR-0106), its consumption charged to the slice
//!   (idempotent `(source_event, dimension)` charges), and the plan replanned
//!   or finished per the `ReattemptPolicy`/`CancelPolicy`.
//! - `close` refuses `CloseBlocked` while `ExperimentReadyToLaunch` holds
//!   (eligible/claimed/in-flight plans remain and `partial` was not declared),
//!   emits the `closed` drift bracket (`provider_drift = observed`), and
//!   produces the `ExperimentReport`.
//!
//! The OQ-363 interim rule (ADR-0213): `iso_cost`/`matched_cap` arms never
//! share subject runs — each arm's cells expand independently (the
//! `arm × task` Cartesian product) even when the hard limits coincide.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_budget::account::{Account, ChargeRequest};
use hh_budget::attribution::Attribution;
use hh_budget::quantity::ResourceQuantity;
use hh_budget::spec::{BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec};
use hh_budget::{BudgetEnforcement, MatchMode, MatchSpec};
use hh_lab::expand::{order_key, ArmConfiguration, ExpandContext, ExpandError, ExpandTask};
use hh_lab::experiment::{
    ArmSpec, BudgetRelevantParam, CancelPolicy, CellPlan, ExperimentRefusal, ExperimentSpec,
    SpecContext,
};
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::leases::LeaseScope;
use hh_ledger::manifest::{
    EventRef, ExperimentBinding, ParticipantClass as ManifestClass, RunKind, RunManifest, TaskRef,
};
use hh_ledger::store::{Lease, Store};
use hh_ontology::compliance::NaReason;
use hh_ontology::control::{CancelledBy, OutcomeClass, StopReason};
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_ontology::eval::FactorKind;
use hh_ontology::participant::ParticipantClass as LevelClass;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::docs::{kind as doc_kind, LabDocs};
use crate::errors::ExperimentError;
use crate::events::{self as ev, class, CloseStatus, ExperimentReport, PauseReason, RunOutcome};
use crate::view::{Declared, ExperimentView, PlanState};

/// The kernel component spelling on this crate's ledger rows.
pub const COMPONENT: &str = "hh-experiment/1";

// ── Engine context ──────────────────────────────────────────────────────────

/// The resolver view the engine runs against — every context-parameterized
/// input arrives through a resolver so the ops stay deterministic given the
/// same views (the boundary wires registry/doc-store lookups; tests wire
/// fixtures). `None` = the fact cannot be resolved — the dependent check
/// refuses `Unresolvable`/skips, never guesses.
/// `budget_ref → BudgetSpec` — the §8.2 budget body resolver.
pub type BudgetResolver<'a> = dyn Fn(&str) -> Option<BudgetSpec> + 'a;
/// `ref → bool` — sealed/drifted predicates.
pub type FlagResolver<'a> = dyn Fn(&str) -> bool + 'a;
/// `spec → suite task axis` resolver (the `ExpandTask` rows).
pub type SuiteTaskResolver<'a> = dyn Fn(&ExperimentSpec) -> Option<Vec<ExpandTask>> + 'a;
/// `arm → {configuration_id, configuration_version_id}` resolver.
pub type ArmConfigResolver<'a> = dyn Fn(&ArmSpec) -> Result<ArmConfiguration, ExpandError> + 'a;
/// `level ref → n/a{reason}` resolver (T-LCD-15).
pub type NaResolver<'a> = dyn Fn(&str) -> Option<NaReason> + 'a;
/// `level ref → fingerprint` resolver (the drift-bracket probe).
pub type FingerprintResolver<'a> = dyn Fn(&str) -> Option<String> + 'a;
/// `arm → budget_enforcement` resolver (ADR-0165 D3).
pub type EnforcementResolver<'a> = dyn Fn(&ArmSpec) -> BudgetEnforcement + 'a;
/// `level ref → budget_relevant param bindings` resolver (AC-R-2.10.2-12).
pub type BudgetRelevantResolver<'a> = dyn Fn(&str) -> BTreeMap<String, BudgetRelevantParam> + 'a;
/// `arm → dialect cache_state_visible` resolver (AC-R-2.3.4-10; `None` =
/// the arm's dialect is not resolvable at this validation point — the check
/// defers, it never guesses).
pub type CacheVisibilityResolver<'a> = dyn Fn(&ArmSpec) -> Option<bool> + 'a;
/// `participant ref → the registry's participant descriptor body`
/// (`capabilities{coordinate → verdict}`; ADR-0013/0151). Powers the
/// hosted-level admissibility check at `register` (AC-R-2.10.3-3).
pub type ParticipantResolver<'a> = dyn Fn(&str) -> Option<Json> + 'a;
/// `environment level ref → environment class` — the
/// `environment_class:<class>` pool membership resolver (AC-R-2.10.3-10).
/// `None`/unresolvable = the plan is never counted into a pool whose
/// membership the context cannot prove (never coerced to a match).
pub type EnvironmentClassResolver<'a> = dyn Fn(&str) -> Option<String> + 'a;
/// The C1 hosted-dispatch SPI (§6.3; ADR-0046 (d)) — the boundary's
/// adapter opens the hosted session and reports the enforcement it
/// actually applied. `Err(detail)` refuses the launch
/// (`HostedLaunchUnavailable`); the refused launch returns its slice.
pub type HostedLauncher<'a> =
    dyn Fn(&HostedLaunchRequest) -> Result<HostedLaunchOutcome, String> + 'a;

/// The `HostedLauncher` request (the adapter's input — the engine passes
/// the arm's declared claim and its budget facts; the *adapter* derives
/// `limits_enforced`, never the engine's guess — ADR-0046 (d)).
#[derive(Debug, Clone)]
pub struct HostedLaunchRequest {
    /// The dispatched run plan.
    pub run_plan_id: String,
    /// The arm being launched.
    pub arm_id: String,
    /// The hosted participant refs on the arm's level assignment.
    pub participant_refs: Vec<String>,
    /// The slice budget allocated for this run.
    pub budget_id: String,
    /// The arm's declared `limits_enforced` claim (the stamp the adapter
    /// confirms or downgrades — reported, never asserted).
    pub limits_enforced_declared: String,
    /// The eval budget's `time.wall_ms` hard cap — the wall-clock ceiling
    /// the environment boundary enforces for the hosted session
    /// (AC-R-2.10.3-14; `None` = undeclared, reported as-is).
    pub wall_clock_ms: Option<u64>,
}

/// The `HostedLauncher` result — the adapter's honest report (S4.5a:
/// the ABI stamps the §6.6 audit rows carry — every member the adapter
/// reports, the engine stamps verbatim; nothing is inferred).
#[derive(Debug, Clone)]
pub struct HostedLaunchOutcome {
    /// The hosting mechanism the adapter used (stamped on the subject
    /// manifest's `hosting_mechanism`).
    pub hosting_mechanism: String,
    /// The session record ref (`run_launched.hosted_session_ref`).
    pub session_ref: Option<String>,
    /// The `limits_enforced` stamp the adapter actually applied
    /// (`full | partial | none`).
    pub limits_enforced: String,
    /// The `AdapterRecord`'s `version_id` — the conditioned artefact the
    /// dispatch ran through (`lifecycle.component.bound{component_ref}`).
    pub adapter_version_id: Option<String>,
    /// The participant's `version_identity` at attach (registry claim;
    /// conformance entries key on it).
    pub participant_version_identity: Option<String>,
    /// The negotiated `hh-hosting/n` ABI version.
    pub abi_version: Option<String>,
    /// The reconciled capability vector the session was admitted under
    /// (declaration ∩ probe records; `unknown` entries ride verbatim).
    pub capability_vector: BTreeMap<String, String>,
    /// The adapter-derived `budget_enforcement` map (`dimension →
    /// enforced | advisory | unenforceable`; ADR-0165 D3 — derived, never
    /// participant-declared).
    pub budget_enforcement: BTreeMap<String, String>,
    /// The session's declared observability level (`events`/`model_io`/
    /// `end_state`; hosted rows never claim `ledger`).
    pub observability_level: Vec<String>,
    /// The session's mediation claim (`gateway`/`intercept`/`none` — the
    /// adapter's report, recorded as data; the mediation-aware metrics and
    /// the `spend`/`tokens` enforcement derivation read it, never trust it
    /// as enforcement fact).
    pub mediation: Option<String>,
}

impl HostedLaunchOutcome {
    /// The S4.4-era minimal outcome — mechanism + enforcement only (every
    /// ext member `None`/`∅`: the adapter reported no more than it ran).
    pub fn minimal(
        hosting_mechanism: impl Into<String>,
        limits_enforced: impl Into<String>,
    ) -> HostedLaunchOutcome {
        HostedLaunchOutcome {
            hosting_mechanism: hosting_mechanism.into(),
            session_ref: None,
            limits_enforced: limits_enforced.into(),
            adapter_version_id: None,
            participant_version_identity: None,
            abi_version: None,
            capability_vector: BTreeMap::new(),
            budget_enforcement: BTreeMap::new(),
            observability_level: Vec::new(),
            mediation: None,
        }
    }
}

#[derive(Default)]
pub struct EngineContext<'a> {
    /// `budget_ref → BudgetSpec` — the §8.2 budget body behind an
    /// `eval_budget`/`search_budget`/`budgets.experiment` ref.
    pub resolve_budget: Option<Box<BudgetResolver<'a>>>,
    /// Whether an artifact ref is sealed (register's `UnsealedArtifact`).
    pub artifact_sealed: Option<Box<FlagResolver<'a>>>,
    /// Whether a level ref's capability has drifted under the pinned snapshot
    /// (register `DependsOnDriftedCapability`; launch's drift gate).
    pub capability_drifted: Option<Box<FlagResolver<'a>>>,
    /// `kind = retirement` diff result (`Some(false)` ⇒ `NotARetirementDiff`).
    pub retirement_diff: Option<bool>,
    /// The `replicates_per_cell` policy floor (register `InsufficientReplicates`).
    pub min_replicates: u32,
    /// `spec → suite task axis` (the `ExpandTask` rows the plan multiplies
    /// over). Mandatory at `expand`.
    pub suite_tasks: Option<Box<SuiteTaskResolver<'a>>>,
    /// `arm → {configuration_id, configuration_version_id}` — the sealed
    /// configuration the assembly chain produced. Mandatory at `expand`.
    pub arm_config: Option<Box<ArmConfigResolver<'a>>>,
    /// `level ref → n/a{reason}` when the level is ineligible under the pinned
    /// `registry_snapshot_id` (T-LCD-15: typed `n/a`, never a dropped row).
    pub level_ineligible: Option<Box<NaResolver<'a>>>,
    /// `level ref → fingerprint` — the drift-bracket probe (`opened`/`closed`
    /// brackets; a moved fingerprint at launch pauses `drift_detected` and
    /// replans the plan).
    pub fingerprint: Option<Box<FingerprintResolver<'a>>>,
    /// `arm → budget_enforcement` — the per-dimension enforcement view the
    /// E-1 `matched_cap` check needs (`enforced` on every matched
    /// dimension). `None` = every arm resolves `native` (trivially
    /// `enforced`; ADR-0165 D3).
    pub budget_enforcement: Option<Box<EnforcementResolver<'a>>>,
    /// `level ref → budget_relevant param bindings` (`{param → {value,
    /// affects[]}}` — the variant's `param_schema` projection) for the
    /// AC-R-2.10.2-12 coverage check at `register`.
    pub budget_relevant_params: Option<Box<BudgetRelevantResolver<'a>>>,
    /// `arm → its bound dialect's cache_state_visible` (AC-R-2.3.4-10 — a
    /// `natural` arm on a cache-invisible dialect is refused without the
    /// design's `cache_na_stratified` declaration).
    pub cache_state_visible: Option<Box<CacheVisibilityResolver<'a>>>,
    /// `participant ref → registry descriptor` (the hosted-level
    /// admissibility check's capability vector source; AC-R-2.10.3-3).
    pub participant_descriptor: Option<Box<ParticipantResolver<'a>>>,
    /// `environment level ref → environment class` (the
    /// `environment_class:<class>` pool membership resolver).
    pub environment_class: Option<Box<EnvironmentClassResolver<'a>>>,
    /// The hosted-dispatch adapter (`None` = hosted launches refuse
    /// `HostedLaunchUnavailable` — never a silent native downgrade).
    pub hosted_launcher: Option<Box<HostedLauncher<'a>>>,
}

impl EngineContext<'_> {
    fn spec_ctx(&self) -> SpecContext<'_> {
        SpecContext {
            resolve_budget: self.resolve_budget.as_deref(),
            artifact_sealed: self.artifact_sealed.as_deref(),
            retirement_diff: self.retirement_diff,
            capability_drifted: self.capability_drifted.as_deref(),
            budget_enforcement: self
                .budget_enforcement
                .as_ref()
                .map(|f| f as &dyn Fn(&ArmSpec) -> BudgetEnforcement),
            budget_relevant_params: self
                .budget_relevant_params
                .as_ref()
                .map(|f| f as &dyn Fn(&str) -> BTreeMap<String, BudgetRelevantParam>),
            min_replicates: self.min_replicates.max(1),
            cache_state_visible: self
                .cache_state_visible
                .as_ref()
                .map(|f| f as &dyn Fn(&ArmSpec) -> Option<bool>),
            participant_descriptor: self
                .participant_descriptor
                .as_ref()
                .map(|f| f as &dyn Fn(&str) -> Option<Json>),
        }
    }
}

// ── Boundary results ────────────────────────────────────────────────────────

/// `next` — the scheduler's verdict at `now`.
#[derive(Debug, Clone, PartialEq)]
pub enum NextVerdict {
    /// The next eligible run plan (schedule order).
    Plan {
        /// The plan key.
        run_plan_id: String,
    },
    /// Nothing eligible and nothing open — the plan space is drained.
    Done,
    /// Every remaining plan is behind its backoff window (data — the
    /// caller waits).
    Backoff {
        /// The earliest `not_before_ms` across open plans.
        not_before_ms: u64,
    },
    /// Plans remain but the experiment pool cannot fund the next slice —
    /// the engine has paused `budget_exhausted` (§6.3 §2.2; the pause row
    /// is the ledger fact).
    BudgetExhausted,
    /// Nothing eligible right now but claims are live/in flight (the caller
    /// waits for expiry or settlement).
    Wait,
}

/// A live claim on a run plan — `claim`'s return and `launch`'s ticket.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimTicket {
    /// The claimed plan.
    pub run_plan_id: String,
    /// The claimant identity string.
    pub holder: String,
    /// The scoped-lease id (`resource(run_plan:<id>)`).
    pub lease_id: String,
    /// Claim expiry (wall ms).
    pub expires_at_ms: u64,
}

/// `launch`'s return — the opened subject run and its writer lease (the
/// driver that executes the subject uses this lease; the engine never holds
/// subject state beyond the ledger).
#[derive(Debug, Clone)]
pub struct LaunchOutcome {
    /// The plan dispatched.
    pub run_plan_id: String,
    /// The subject `agent` run id.
    pub run_id: String,
    /// The subject run's writer lease (the subject driver writes with it).
    pub subject_writer: Lease,
    /// The attempt ordinal this launch is.
    pub attempt_no: u32,
    /// The slice budget id allocated for this run (the subject manifest's
    /// `budget` member).
    pub budget_id: String,
}

// ── The engine ──────────────────────────────────────────────────────────────

/// `ExperimentEngine` — bound to one ledger `Store` + the `LabDocs` document
/// store; bound to an experiment run by [`ExperimentEngine::open_experiment`]
/// or [`ExperimentEngine::attach`].
pub struct ExperimentEngine<'a> {
    store: &'a mut Store,
    docs: LabDocs,
    ctx: EngineContext<'a>,
    /// The writer-lease holder label (the fenced experiment-run writer).
    holder: String,
    /// The experiment-run writer lease TTL.
    writer_ttl_ms: u64,
    /// The `resource(run_plan_id)` claim TTL.
    claim_ttl_ms: u64,
    /// The bound experiment run.
    run_id: Option<String>,
    /// The bound writer lease.
    lease: Option<Lease>,
    /// Whether [`Drop`] releases the writer lease — `false` for engines bound
    /// to a lease the caller owns across calls (`bind_lease`/`take_binding`).
    release_on_drop: bool,
}

/// The `close` re-check record: `(under_utilised, budget_match, na_cells,
/// utilization)` — E-3/E-4 (§6.3 §2.4; ADR-0155 D4).
type CloseRecheck = (Vec<String>, Vec<Json>, Vec<Json>, Json);

impl<'a> ExperimentEngine<'a> {
    /// A fresh engine (unbound — `register`/`expand`/`open_experiment` first).
    pub fn new(store: &'a mut Store, docs: LabDocs, ctx: EngineContext<'a>) -> Self {
        ExperimentEngine {
            store,
            docs,
            ctx,
            holder: "experiment-engine".to_string(),
            writer_ttl_ms: 60_000,
            claim_ttl_ms: 30_000,
            run_id: None,
            lease: None,
            release_on_drop: true,
        }
    }

    /// `bind_lease(store, docs, ctx, run_id, lease)` — bind to an already-open
    /// experiment run under a writer lease the *caller* owns (the boundary
    /// holds leases across ops; [`take_binding`] hands the — possibly
    /// renewed — lease back). Drop does not release it.
    pub fn bind_lease(
        store: &'a mut Store,
        docs: LabDocs,
        ctx: EngineContext<'a>,
        run_id: String,
        lease: Lease,
    ) -> Self {
        ExperimentEngine {
            store,
            docs,
            ctx,
            holder: lease.holder.clone(),
            writer_ttl_ms: 60_000,
            claim_ttl_ms: 30_000,
            run_id: Some(run_id),
            lease: Some(lease),
            release_on_drop: false,
        }
    }

    /// Extract `(run_id, lease)` without releasing — the caller persists the
    /// binding across ops (the boundary's lease map).
    pub fn take_binding(&mut self) -> Option<(String, Lease)> {
        match (self.run_id.take(), self.lease.take()) {
            (Some(r), Some(l)) => Some((r, l)),
            _ => None,
        }
    }

    /// `validate(spec)` — the E-1 gate alone (the boundary's `dry_run` —
    /// admission without a durable write).
    pub fn validate(&self, spec: &ExperimentSpec) -> Result<(), ExperimentError> {
        spec.register(&self.ctx.spec_ctx())
            .map_err(ExperimentError::Refusal)
    }

    /// Set the writer-lease holder label.
    pub fn with_holder(mut self, holder: impl Into<String>) -> Self {
        self.holder = holder.into();
        self
    }

    /// Set the writer/claim TTLs (wall ms).
    pub fn with_ttls(mut self, writer_ms: u64, claim_ms: u64) -> Self {
        self.writer_ttl_ms = writer_ms;
        self.claim_ttl_ms = claim_ms;
        self
    }

    /// The underlying store.
    pub fn store(&self) -> &Store {
        self.store
    }

    /// The bound experiment run id, when open/attached.
    pub fn run_id(&self) -> Option<&str> {
        self.run_id.as_deref()
    }

    // ── register / expand ────────────────────────────────────────────────

    /// `register(spec) → experiment_id` — the E-1 gate: the closed refusal
    /// set runs against the engine's resolvers; a clean spec is durably
    /// stored (content-addressed; idempotent). A refusal is typed — never a
    /// warning (T-LCD-14).
    pub fn register(&mut self, spec: &ExperimentSpec) -> Result<String, ExperimentError> {
        spec.register(&self.ctx.spec_ctx())?;
        self.docs.put_spec(spec)
    }

    /// `expand(experiment_id) → plan_id` — re-checks the refusal set (an
    /// unregistered or drifted spec fails identically), resolves the suite's
    /// task axis, runs the pure expansion, and stores the `CellPlan`.
    pub fn expand(&mut self, experiment_id: &str) -> Result<String, ExperimentError> {
        let spec = self.spec(experiment_id)?;
        spec.register(&self.ctx.spec_ctx())
            .map_err(ExpandError::Refusal)?;
        let tasks = self
            .ctx
            .suite_tasks
            .as_ref()
            .and_then(|f| f(&spec))
            .ok_or_else(|| ExperimentError::Unresolvable {
                detail: format!("suite {} tasks unresolvable", spec.suite.suite_ref),
            })?;
        let arm_config =
            self.ctx
                .arm_config
                .as_deref()
                .ok_or_else(|| ExperimentError::Unresolvable {
                    detail: "arm_config resolver absent".to_string(),
                })?;
        let plan = hh_lab::expand::expand(
            &spec,
            &tasks,
            &ExpandContext {
                arm_config,
                level_ineligible: self.ctx.level_ineligible.as_deref(),
            },
        )?;
        self.docs.put_plan(experiment_id, &plan)
    }

    // ── open / attach ────────────────────────────────────────────────────

    /// `open_experiment(experiment_id) → experiment run_id` — mints the
    /// `run_kind = experiment` run under the engine's fenced writer, allocates
    /// the experiment budget root, and commits `declared` + `run_planned` +
    /// the `opened` drift bracket in one batch. A second open on the same
    /// experiment is `AlreadyOpen`.
    pub fn open_experiment(&mut self, experiment_id: &str) -> Result<String, ExperimentError> {
        let spec = self.spec(experiment_id)?;
        let entry = self.docs.index_entry(experiment_id)?;
        let plan_id = entry
            .as_ref()
            .and_then(|e| e.plan_id.clone())
            .ok_or_else(|| ExperimentError::PlanNotFound {
                experiment_id: experiment_id.to_string(),
            })?;
        if let Some(run_id) = entry.as_ref().and_then(|e| e.run_id.clone()) {
            // The producer contract's timing refusals (§6.5 §2.3;
            // ADR-0162 D2/D4): a second `declared` after a `run_bound` is
            // `DeclarationLate` — and `PreRegistrationLate` when the
            // pre-registration itself postdates the first bound run; a
            // second `declared` with nothing bound is the plain
            // `AlreadyOpen` (`SchemaViolation` at the append layer).
            if let Ok(events) = self.store.envelopes(&run_id) {
                let first_bound = events
                    .iter()
                    .find(|e| e.class == class::RUN_BOUND)
                    .map(|e| e.seq);
                if let Some(bound_seq) = first_bound {
                    let prereg_late = spec
                        .pre_registration
                        .as_ref()
                        .is_some_and(|p| p.registered_at > bound_seq);
                    if prereg_late {
                        return Err(ExperimentError::Refusal(
                            hh_lab::experiment::ExperimentRefusal::PreRegistrationLate {
                                detail: format!(
                                    "pre_registration.registered_at {} postdates the first run_bound (seq {bound_seq})",
                                    spec.pre_registration
                                        .as_ref()
                                        .map(|p| p.registered_at)
                                        .unwrap_or(0),
                                ),
                            },
                        ));
                    }
                    return Err(ExperimentError::Refusal(
                        hh_lab::experiment::ExperimentRefusal::DeclarationLate {
                            detail: format!(
                                "experiment run {run_id} already carries run_bound rows"
                            ),
                        },
                    ));
                }
            }
            return Err(ExperimentError::AlreadyOpen {
                experiment_id: experiment_id.to_string(),
                run_id,
            });
        }
        let plan = self
            .docs
            .plan(&plan_id)?
            .ok_or_else(|| ExperimentError::PlanNotFound {
                experiment_id: experiment_id.to_string(),
            })?;

        let manifest = self.experiment_manifest(&spec, &plan);
        let (run_id, lease) = self.store.open_run(manifest, &self.holder)?;
        self.run_id = Some(run_id.clone());
        self.lease = Some(lease.clone());

        // The experiment budget root — the pool the per-run slices draw from.
        let pool = self.resolve_budget(&spec.budgets.experiment)?;
        let root_id = {
            let mut acct = Account::open(self.store, &run_id)?;
            acct.allocate(
                &lease,
                None,
                BudgetScope {
                    kind: BudgetScopeKind::Experiment,
                    target: experiment_id.to_string(),
                },
                pool,
            )?
        };
        let _ = root_id; // derivable from the `control.budget.allocated` root row

        // One commit: declared + run_planned (in recorded schedule order) +
        // the opened drift bracket.
        let mut events = vec![self.mint(
            &run_id,
            class::DECLARED,
            ev::declared(&spec, &plan, self.store.now_ms()),
        )?];
        let order = plan
            .run_plans
            .iter()
            .enumerate()
            .map(|(i, rp)| {
                let cell = plan
                    .cells
                    .iter()
                    .find(|c| c.cell_id == rp.cell_id)
                    .ok_or_else(|| ExperimentError::Store {
                        detail: format!("plan cell {} missing", rp.cell_id),
                    })?;
                Ok((
                    order_key(
                        &plan.order,
                        spec.scheduling.order,
                        rp,
                        i as u32,
                        &cell.task_id,
                        &cell.arm_id,
                    ),
                    rp,
                ))
            })
            .collect::<Result<Vec<_>, ExperimentError>>()?;
        let mut ordered = order;
        ordered.sort_by(|a, b| a.0.cmp(&b.0));
        for (pos, (_, rp)) in ordered.iter().enumerate() {
            let cell = plan
                .cells
                .iter()
                .find(|c| c.cell_id == rp.cell_id)
                .ok_or_else(|| ExperimentError::Store {
                    detail: format!("plan cell {} missing", rp.cell_id),
                })?;
            events.push(self.mint(
                &run_id,
                class::RUN_PLANNED,
                ev::run_planned(
                    rp,
                    &cell.arm_id,
                    &cell.task_id,
                    cell.split_label.name(),
                    &cell.configuration_version_id,
                    pos as u64,
                ),
            )?);
        }
        let fingerprints = self.probe_fingerprints(&spec);
        events.push(self.mint(
            &run_id,
            class::DRIFT_BRACKET,
            ev::drift_bracket("opened", &fingerprints, false),
        )?);
        self.append_chained(&run_id, &lease, events)?;

        let mut e = entry.unwrap_or_default();
        e.plan_id = Some(plan_id);
        e.run_id = Some(run_id.clone());
        self.docs.set_index_entry(experiment_id, e)?;
        Ok(run_id)
    }

    /// `attach(experiment_id) → run_id` — bind to an already-open experiment
    /// (the `restore` path — KP-E1…E5): re-acquires the writer lease
    /// (takeover when the stale holder is dead/expired) and re-projects.
    /// `WouldBlock` when a live writer still holds the fence.
    pub fn attach(&mut self, experiment_id: &str) -> Result<String, ExperimentError> {
        let entry = self.docs.index_entry(experiment_id)?.ok_or_else(|| {
            ExperimentError::UnknownExperiment {
                experiment_id: experiment_id.to_string(),
            }
        })?;
        let run_id = entry
            .run_id
            .clone()
            .ok_or_else(|| ExperimentError::PlanNotFound {
                experiment_id: experiment_id.to_string(),
            })?;
        self.attach_run(&run_id)
    }

    /// `attach_run(run_id)` — bind by run id (the boundary's `experiment_run_id`
    /// argument path).
    pub fn attach_run(&mut self, run_id: &str) -> Result<String, ExperimentError> {
        let manifest = self
            .store
            .manifest(run_id)
            .map_err(|_| ExperimentError::NotAnExperimentRun {
                run_id: run_id.to_string(),
            })?
            .clone();
        if manifest.run_kind != RunKind::Experiment {
            return Err(ExperimentError::NotAnExperimentRun {
                run_id: run_id.to_string(),
            });
        }
        let lease = self
            .store
            .acquire_writer(&self.holder, run_id, self.writer_ttl_ms)
            .map_err(|e| match e {
                hh_ledger::errors::LedgerError::WouldBlock { active_holder } => {
                    ExperimentError::WouldBlock {
                        holder: active_holder,
                    }
                }
                other => ExperimentError::Ledger(other),
            })?;
        self.run_id = Some(run_id.to_string());
        self.lease = Some(lease);
        Ok(run_id.to_string())
    }

    // ── next / claim ─────────────────────────────────────────────────────

    /// `project()` — the scheduler view over the committed stream (S-1;
    /// `ExperimentView::fold` is the whole rebuild).
    pub fn project(&self) -> Result<ExperimentView, ExperimentError> {
        let run_id = self.bound_run()?;
        Ok(ExperimentView::fold(
            self.store.events(run_id).map_err(ExperimentError::Ledger)?,
        ))
    }

    /// `next() → NextVerdict` — reconcile expired claims, then the eligible
    /// plan in recorded schedule order. `BudgetExhausted` appends the
    /// `paused{budget_exhausted}` row — the pause is a ledger fact.
    /// `next` is the §6.3 verb name — not `Iterator::next`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<NextVerdict, ExperimentError> {
        let (run_id, lease) = self.bound()?;
        let now = self.store.now_ms();
        self.reconcile(&run_id, &lease, now)?;
        let view = self.project()?;
        self.live(&view, &run_id)?;
        let eligible = view.eligible_at(now);
        // §6.3 §2.2 admission (AC-R-2.10.3-10): `start_stagger_ms` delays a
        // plan until `opened_ms + order_pos · stagger`; `max_concurrent_runs`
        // and the typed pools cap the consuming set (live claims + in-flight
        // attempts). A pool-blocked head is skipped, never over-admitted —
        // recorded schedule order is preserved among dispatchable plans.
        let spec = match &view.declared {
            Some(d) => Some(self.spec(&d.experiment_id)?),
            None => None,
        };
        let opened_ms = view.declared.as_ref().map(|d| d.opened_ms).unwrap_or(0);
        let stagger = spec
            .as_ref()
            .map(|sp| sp.scheduling.start_stagger_ms)
            .unwrap_or(0);
        let mut stagger_wait: Option<u64> = None;
        let mut dispatchable: Vec<&PlanState> = Vec::new();
        for ps in &eligible {
            let ready_at = opened_ms.saturating_add(ps.order_pos.saturating_mul(stagger));
            if now < ready_at {
                stagger_wait = Some(stagger_wait.map_or(ready_at, |t| t.min(ready_at)));
                continue;
            }
            if let Some(sp) = &spec {
                if self.pool_blocker(sp, &view, ps, now).is_some() {
                    continue;
                }
            }
            dispatchable.push(ps);
        }
        if dispatchable.is_empty() {
            if view.open_plans(now).is_empty() {
                return Ok(NextVerdict::Done);
            }
            // Everything open is claimed-live, inside its backoff window, or
            // behind its stagger slot — report the earliest wake-up.
            let backoff = view
                .plans
                .values()
                .filter(|p| p.open_at(now))
                .map(|p| p.not_before_ms)
                .chain(stagger_wait)
                .filter(|t| *t > now)
                .min();
            return Ok(match backoff {
                Some(t) => NextVerdict::Backoff { not_before_ms: t },
                None => NextVerdict::Wait,
            });
        }
        // The pool must fund the next slice — else pause `budget_exhausted`
        // (E-2; the experiment never trims a run mid-flight).
        let head = dispatchable[0];
        if let Err(e) = self.slice_affordable(&run_id, &lease, &view, head) {
            return match e {
                ExperimentError::InsufficientBudget { .. } => {
                    if view.paused.is_none() {
                        self.append_chained(
                            &run_id,
                            &lease,
                            vec![self.mint(
                                &run_id,
                                class::PAUSED,
                                ev::paused(
                                    PauseReason::BudgetExhausted,
                                    self.root_budget(&run_id).ok().as_deref(),
                                    None,
                                ),
                            )?],
                        )?;
                    }
                    Ok(NextVerdict::BudgetExhausted)
                }
                other => Err(other),
            };
        }
        Ok(NextVerdict::Plan {
            run_plan_id: head.run_plan_id.clone(),
        })
    }

    /// `claim(run_plan_id, holder) → ClaimTicket` — a scoped
    /// `resource(run_plan_id)` lease plus the `run_claimed` row. A live claim
    /// by another holder is `WouldBlock`; an expired one is reconciled first.
    pub fn claim(
        &mut self,
        run_plan_id: &str,
        holder: &str,
    ) -> Result<ClaimTicket, ExperimentError> {
        let (run_id, lease) = self.bound()?;
        let now = self.store.now_ms();
        self.reconcile(&run_id, &lease, now)?;
        let view = self.project()?;
        self.live(&view, &run_id)?;
        let ps = view
            .plans
            .get(run_plan_id)
            .ok_or_else(|| ExperimentError::UnknownRunPlan {
                run_plan_id: run_plan_id.to_string(),
            })?;
        // Settled/final/in-flight/backoff are plan-state errors; a live claim
        // is the ledger's `WouldBlock` (the scoped-lease fence is the claim
        // record — reconcile already expired the stale ones).
        if ps.accepted_run.is_some()
            || ps.finished_final
            || ps.in_flight().is_some()
            || now < ps.not_before_ms
        {
            return Err(ExperimentError::BadPlanState {
                run_plan_id: run_plan_id.to_string(),
                detail: "plan is not claimable (in flight, settled, or in backoff)".to_string(),
            });
        }
        // Pool admission (§6.3 §2.2; AC-R-2.10.3-10): a claim reserves the
        // plan's pool slots — a pool-blocked claim refuses `PoolExhausted`
        // rather than parking a claim it cannot dispatch.
        if let Some(d) = &view.declared {
            let spec = self.spec(&d.experiment_id)?;
            if let Some((pool, limit)) = self.pool_blocker(&spec, &view, ps, now) {
                return Err(ExperimentError::PoolExhausted { pool, limit });
            }
        }
        let scope = LeaseScope::Resource(format!("run_plan:{run_plan_id}"));
        let scoped = self
            .store
            .lease_acquire(&run_id, &lease, &scope, holder, self.claim_ttl_ms)
            .map_err(|e| match e {
                hh_ledger::errors::LedgerError::WouldBlock { active_holder } => {
                    ExperimentError::WouldBlock {
                        holder: active_holder,
                    }
                }
                other => ExperimentError::Ledger(other),
            })?;
        self.append_chained(
            &run_id,
            &lease,
            vec![self.mint(
                &run_id,
                class::RUN_CLAIMED,
                ev::run_claimed(run_plan_id, holder, &scoped.lease_id, scoped.expires_at_ms),
            )?],
        )?;
        Ok(ClaimTicket {
            run_plan_id: run_plan_id.to_string(),
            holder: holder.to_string(),
            lease_id: scoped.lease_id,
            expires_at_ms: scoped.expires_at_ms,
        })
    }

    // ── launch ───────────────────────────────────────────────────────────

    /// `launch(ticket, subject_holder) → LaunchOutcome` — dispatch the claimed
    /// plan: drift probes (E-2's `DependsOnDriftedCapability` /
    /// `ModelFingerprintDrift`), the budget slice allocation
    /// (`InsufficientBudget` leaves the plan re-plannable — the claim is
    /// released back, never consumed), the subject `agent` run open with the
    /// complete `experiment{…}` row-key binding (registry snapshot included —
    /// the §6.2 one-snapshot rule propagates onto every bundle), the subject
    /// run's `run_bound` stamp, and the experiment run's `run_launched` +
    /// `run_bound` mirror.
    pub fn launch(
        &mut self,
        ticket: &ClaimTicket,
        subject_holder: &str,
    ) -> Result<LaunchOutcome, ExperimentError> {
        let (run_id, lease) = self.bound()?;
        let now = self.store.now_ms();
        self.reconcile(&run_id, &lease, now)?;
        let view = self.project()?;
        self.live(&view, &run_id)?;
        let ps = view
            .plans
            .get(&ticket.run_plan_id)
            .ok_or_else(|| ExperimentError::UnknownRunPlan {
                run_plan_id: ticket.run_plan_id.clone(),
            })?
            .clone();
        match &ps.claim {
            Some(c) if c.lease_id == ticket.lease_id && c.expires_at_ms > now => {}
            Some(_) => {
                return Err(ExperimentError::BadPlanState {
                    run_plan_id: ticket.run_plan_id.clone(),
                    detail: "claim lease mismatch".to_string(),
                })
            }
            None => {
                return Err(ExperimentError::BadPlanState {
                    run_plan_id: ticket.run_plan_id.clone(),
                    detail: "plan is not claimed".to_string(),
                })
            }
        }
        if ps.in_flight().is_some() {
            return Err(ExperimentError::BadPlanState {
                run_plan_id: ticket.run_plan_id.clone(),
                detail: "an attempt is already in flight".to_string(),
            });
        }
        let declared =
            view.declared
                .clone()
                .ok_or_else(|| ExperimentError::NotAnExperimentRun {
                    run_id: run_id.clone(),
                })?;
        let spec = self
            .spec(&declared.experiment_id)
            .map_err(|_| ExperimentError::Store {
                detail: format!("declared spec {} unreadable", declared.experiment_id),
            })?;
        let plan =
            self.docs
                .plan(&declared.plan_id)?
                .ok_or_else(|| ExperimentError::PlanNotFound {
                    experiment_id: declared.experiment_id.clone(),
                })?;
        let cell = plan
            .cells
            .iter()
            .find(|c| c.cell_id == ps.cell_id)
            .ok_or_else(|| ExperimentError::Store {
                detail: format!("plan cell {} missing", ps.cell_id),
            })?
            .clone();
        let arm = spec
            .arms
            .iter()
            .find(|a| a.arm_id == cell.arm_id)
            .ok_or_else(|| ExperimentError::Store {
                detail: format!("arm {} missing", cell.arm_id),
            })?
            .clone();

        // Pool admission re-check (AC-R-2.10.3-10): a claim may predate a
        // competing dispatch — `launch` re-verifies and refuses
        // `PoolExhausted`; the claim stays live for a later dispatch.
        if let Some((pool, limit)) = self.pool_blocker(&spec, &view, &ps, now) {
            return Err(ExperimentError::PoolExhausted { pool, limit });
        }
        let pools = self.plan_pool_keys(&spec, &arm.arm_id);

        // E-2 launch probes — capability drift under the pinned snapshot, and
        // the model-fingerprint drift probe against the `opened` bracket.
        for (fname, lid) in &arm.level_assignment {
            if let Some(level) = spec
                .factors
                .iter()
                .find(|f| &f.name == fname)
                .and_then(|f| f.levels.iter().find(|l| &l.level_id == lid))
            {
                if let Some(drifted) = self.ctx.capability_drifted.as_deref() {
                    if drifted(&level.ref_) {
                        // The plan replans (the launch refused before dispatch)
                        // and the refusal is typed — T-LCD-14.
                        self.append_chained(
                            &run_id,
                            &lease,
                            vec![self.mint(
                                &run_id,
                                class::RUN_REPLANNED,
                                ev::run_replanned(&ps.run_plan_id, "", ps.next_attempt_no, now),
                            )?],
                        )?;
                        return Err(ExperimentError::Refusal(
                            ExperimentRefusal::DependsOnDriftedCapability {
                                capability: level.ref_.clone(),
                            },
                        ));
                    }
                }
                if let Some(probe) = self.ctx.fingerprint.as_deref() {
                    if let Some(opened) = view.drift_brackets.iter().find(|b| b.phase == "opened") {
                        if let (Some(base), Some(now_fp)) =
                            (opened.fingerprints.get(&level.ref_), probe(&level.ref_))
                        {
                            if *base != now_fp {
                                self.append_chained(
                                    &run_id,
                                    &lease,
                                    vec![
                                        self.mint(
                                            &run_id,
                                            class::RUN_REPLANNED,
                                            ev::run_replanned(
                                                &ps.run_plan_id,
                                                "",
                                                ps.next_attempt_no,
                                                now,
                                            ),
                                        )?,
                                        self.mint(
                                            &run_id,
                                            class::PAUSED,
                                            ev::paused(PauseReason::DriftDetected, None, None),
                                        )?,
                                    ],
                                )?;
                                return Err(ExperimentError::ExperimentPaused {
                                    run_id: run_id.clone(),
                                    reason: "drift_detected".to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }

        // The budget slice — a `slice` child of the experiment pool whose
        // hard caps are the arm's `eval_budget` (E-2/E-3).
        let budget_id = {
            let root = self.root_budget(&run_id)?;
            let slice_spec = self.slice_spec(&arm)?;
            let mut acct = Account::open(self.store, &run_id)?;
            match acct.allocate(
                &lease,
                Some(&root),
                BudgetScope {
                    kind: BudgetScopeKind::Experiment,
                    target: ps.run_plan_id.clone(),
                },
                slice_spec,
            ) {
                Ok(id) => id,
                Err(e) => {
                    // `allocate` already ledgered the refused row; the plan
                    // stays claimed (re-plannable once the claim lapses or the
                    // pool frees). The engine does not consume the claim.
                    return Err(ExperimentError::from(e));
                }
            }
        };

        // C1 hosted dispatch (§6.3; AC-R-2.10.3-14; ADR-0046 (d)): an arm
        // assigning hosted levels routes through the boundary's
        // `HostedLauncher` SPI before the subject run opens — the adapter
        // opens the session and reports the enforcement it applied; a
        // missing/refused adapter fails the launch `HostedLaunchUnavailable`
        // (the slice returns to the pool) — a hosted run is never silently
        // downgraded to native.
        let hosted_refs: Vec<String> = arm
            .level_assignment
            .iter()
            .filter_map(|(fname, lid)| {
                spec.factors
                    .iter()
                    .find(|f| &f.name == fname)
                    .and_then(|f| f.levels.iter().find(|l| &l.level_id == lid))
                    .filter(|l| l.class == LevelClass::Hosted)
                    .map(|l| l.ref_.clone())
            })
            .collect();
        let hosted_outcome = if hosted_refs.is_empty() {
            None
        } else {
            let launcher = self.ctx.hosted_launcher.as_deref().ok_or_else(|| {
                ExperimentError::HostedLaunchUnavailable {
                    detail: format!(
                        "arm `{}` assigns hosted levels {:?} but no HostedLauncher is wired",
                        arm.arm_id, hosted_refs
                    ),
                }
            })?;
            let wall_clock_ms = self
                .resolve_budget_opt(&arm.eval_budget)
                .and_then(|e| {
                    e.hard_caps_map()
                        .get(DimensionId::TimeWallMs.as_str())
                        .copied()
                })
                .map(|v| v.max(0) as u64);
            match launcher(&HostedLaunchRequest {
                run_plan_id: ps.run_plan_id.clone(),
                arm_id: arm.arm_id.clone(),
                participant_refs: hosted_refs.clone(),
                budget_id: budget_id.clone(),
                limits_enforced_declared: arm.limits_enforced.clone(),
                wall_clock_ms,
            }) {
                Ok(o) => Some(o),
                Err(detail) => {
                    // The refused launch returns the slice to the pool —
                    // nothing is left accounted against it (CC3).
                    let mut acct = Account::open(self.store, &run_id)?;
                    let _ = acct.complete(&lease, &budget_id);
                    return Err(ExperimentError::HostedLaunchUnavailable { detail });
                }
            }
        };
        let participant_class = if hosted_refs.is_empty() {
            "native"
        } else {
            "hosted"
        };
        // The `limits_enforced` stamp is *derived*: the adapter's report on
        // a hosted launch, the arm's (register-verified) claim on a native
        // one — never a stamped upgrade (ADR-0046 (d); AC-R-2.10.3-3).
        let limits_enforced = hosted_outcome
            .as_ref()
            .map(|o| o.limits_enforced.clone())
            .unwrap_or_else(|| arm.limits_enforced.clone());

        // The subject run — `run_kind = agent` with the complete experiment
        // row-key binding (§6.5's result-row key fields).
        let attempt_no = ps.next_attempt_no;
        let seed = ps
            .seed_material
            .get("harness_seed")
            .and_then(Json::as_int)
            .map(|v| v as u64);
        let mut manifest = RunManifest::minimal(RunKind::Agent);
        manifest.configuration_id = Some(cell.configuration_id.clone());
        manifest.configuration_version_id = Some(cell.configuration_version_id.clone());
        manifest.task_ref = Some(TaskRef {
            task_id: cell.task_id.clone(),
            suite_id: spec.suite.suite_ref.clone(),
            split_label: cell.split_label,
        });
        manifest.parent_run_id = Some(run_id.clone());
        manifest.seed = seed;
        manifest.budget = Some(budget_id.clone());
        // The `Design`'s snapshot on the top-level member too — the subject
        // run's bundle carries it (§6.2 "in every bundle and `Design`").
        manifest.registry_snapshot_id = spec.design.registry_snapshot_id.clone();
        manifest.participant_class = if hosted_refs.is_empty() {
            ManifestClass::Native
        } else {
            ManifestClass::Hosted
        };
        if let Some(o) = &hosted_outcome {
            manifest.hosting_mechanism = Some(o.hosting_mechanism.clone());
            // The participant descriptor carries the capability vector the
            // dispatch resolved against (ADR-0152's record ref).
            manifest.capability_declaration_ref = hosted_refs.first().cloned();
            // The ABI stamps (§6.6 §3; S4.5a): the adapter's reported
            // attach facts ride `extra` verbatim — `abi_version`,
            // `capability_vector`, `budget_enforcement`, the conditioned
            // adapter artefact and the participant's `version_identity`.
            // A member the adapter did not report is absent, never 0/"".
            if let Some(v) = &o.abi_version {
                manifest.extra.insert("abi_version".into(), Json::str(v));
            }
            if !o.capability_vector.is_empty() {
                manifest.extra.insert(
                    "capability_vector".into(),
                    Json::Obj(
                        o.capability_vector
                            .iter()
                            .map(|(k, v)| (k.clone(), Json::str(v)))
                            .collect(),
                    ),
                );
            }
            if !o.budget_enforcement.is_empty() {
                manifest.extra.insert(
                    "budget_enforcement".into(),
                    Json::Obj(
                        o.budget_enforcement
                            .iter()
                            .map(|(k, v)| (k.clone(), Json::str(v)))
                            .collect(),
                    ),
                );
            }
            if let Some(v) = &o.adapter_version_id {
                manifest
                    .extra
                    .insert("adapter_version_id".into(), Json::str(v));
            }
            if let Some(v) = &o.participant_version_identity {
                manifest
                    .extra
                    .insert("participant_version_identity".into(), Json::str(v));
            }
            if !o.observability_level.is_empty() {
                manifest.extra.insert(
                    "observability_level".into(),
                    Json::Arr(o.observability_level.iter().map(Json::str).collect()),
                );
            }
            if let Some(v) = &o.mediation {
                manifest.extra.insert("mediation".into(), Json::str(v));
            }
        }
        manifest.experiment = Some(ExperimentBinding {
            experiment_run_id: Some(run_id.clone()),
            arm_id: Some(arm.arm_id.clone()),
            cell_id: Some(cell.cell_id.clone()),
            replicate_index: Some(ps.replicate_index as u64),
            attempt_no: Some(attempt_no as u64),
            comparable: Some(spec.kind.requires_match()),
            registry_snapshot_id: spec.design.registry_snapshot_id.clone(),
            design_ref: None,
            pre_registration_ref: None,
            ..Default::default()
        });
        let (subject_id, subject_lease) = self.store.open_run(manifest, subject_holder)?;

        // The subject stamps `measurement.experiment.bound` on its own
        // stream (§6.3 `launch` step 4 — §6.5's `bind` proof).
        let bound_payload = ev::subject_bound(
            &declared.experiment_id,
            &arm.arm_id,
            &cell.configuration_id,
            &cell.configuration_version_id,
            &cell.cell_id,
            ps.replicate_index,
            attempt_no,
            &budget_id,
            participant_class,
            &limits_enforced,
        );
        let mut subject_events = vec![self.mint(&subject_id, class::BOUND, bound_payload)?];
        // The hosted audit stamps (§6.6 §6; S4.5a): `lifecycle.hosted.
        // attached` records the ABI attach the dispatch reported and
        // `lifecycle.component.bound{class_id = hosting_adapter}` names the
        // conditioned adapter artefact the hosted rows will enter through.
        // Both are Lab-minted on the subject's stream — the participant's
        // own events ride the adapter's projection, never this row.
        if let Some(o) = &hosted_outcome {
            subject_events.push(self.mint(
                &subject_id,
                "lifecycle.hosted.attached",
                ev::hosted_attached(
                    &o.hosting_mechanism,
                    o.session_ref.as_deref(),
                    o.participant_version_identity.as_deref(),
                    o.adapter_version_id.as_deref(),
                    o.abi_version.as_deref(),
                    &o.limits_enforced,
                    &o.budget_enforcement,
                    o.mediation.as_deref(),
                ),
            )?);
            if let Some(adapter) = &o.adapter_version_id {
                subject_events.push(self.mint(
                    &subject_id,
                    "lifecycle.component.bound",
                    ev::component_bound_hosting_adapter(adapter, o.session_ref.as_deref()),
                )?);
            }
        }
        self.store
            .append(&subject_id, &subject_lease, subject_events)
            .map_err(ExperimentError::Ledger)?;
        let subject_head = self
            .store
            .head_event_id(&subject_id)
            .map_err(ExperimentError::Ledger)?;

        // The experiment run's launch record + bound mirror.
        self.append_chained(
            &run_id,
            &lease,
            vec![
                self.mint(
                    &run_id,
                    class::RUN_LAUNCHED,
                    ev::run_launched(
                        &ps.run_plan_id,
                        &subject_id,
                        attempt_no,
                        &budget_id,
                        &ev::LaunchStamp {
                            pool_consumed: &pools,
                            participant_class,
                            limits_enforced: &limits_enforced,
                            hosted_session_ref: hosted_outcome
                                .as_ref()
                                .and_then(|o| o.session_ref.as_deref()),
                        },
                    ),
                )?,
                self.mint(
                    &run_id,
                    class::RUN_BOUND,
                    ev::run_bound_mirror(
                        &subject_id,
                        &ps.run_plan_id,
                        &cell.cell_id,
                        ps.replicate_index,
                        attempt_no,
                        &subject_head,
                        participant_class,
                        &limits_enforced,
                    ),
                )?,
            ],
        )?;
        Ok(LaunchOutcome {
            run_plan_id: ps.run_plan_id.clone(),
            run_id: subject_id,
            subject_writer: subject_lease,
            attempt_no,
            budget_id,
        })
    }

    // ── settle ───────────────────────────────────────────────────────────

    /// `settle(run_plan_id) → RunOutcome` — exactly-once (S-2): a settled
    /// plan returns its recorded outcome; an in-flight plan reads the
    /// subject run's terminal facts, projects the `OutcomeClass`, charges the
    /// slice (idempotent `(source_event, dimension)` rows), completes the
    /// slice, and applies the re-attempt/cancel policy (`run_replanned`,
    /// `plan_final`, or `paused{infrastructure_suspected}` when the
    /// re-attempt fraction trips — ADR-0155 D5).
    pub fn settle(&mut self, run_plan_id: &str) -> Result<RunOutcome, ExperimentError> {
        let (run_id, lease) = self.bound()?;
        let now = self.store.now_ms();
        self.reconcile(&run_id, &lease, now)?;
        let view = self.project()?;
        // `settle` is allowed while paused — in-flight runs must still settle.
        if view.closed.is_some() {
            return Err(ExperimentError::ExperimentClosed {
                run_id: run_id.clone(),
            });
        }
        let ps = view
            .plans
            .get(run_plan_id)
            .ok_or_else(|| ExperimentError::UnknownRunPlan {
                run_plan_id: run_plan_id.to_string(),
            })?
            .clone();
        let attempt = ps
            .attempts
            .iter()
            .rev()
            .find(|a| !a.superseded)
            .cloned()
            .ok_or_else(|| ExperimentError::BadPlanState {
                run_plan_id: run_plan_id.to_string(),
                detail: "no launched attempt".to_string(),
            })?;
        if let Some(oc) = attempt.outcome {
            // Exactly-once: the recorded settle replays as the same outcome.
            return Ok(RunOutcome {
                run_plan_id: run_plan_id.to_string(),
                run_id: attempt.run_id.clone(),
                outcome_class: oc,
                accepted: attempt.accepted,
                superseded: attempt.superseded,
                plan_final: ps.finished_final,
                attempt_no: attempt.attempt_no,
                budget_utilization: attempt.budget_utilization.clone().unwrap_or(Json::obj([])),
                veto_tripped: attempt.veto_tripped.clone(),
                replanned: false,
                paused: view.paused.clone(),
                regrade_pending: attempt.regrade_pending,
            });
        }

        // The subject run must have terminated.
        let subject_events = self
            .store
            .events(&attempt.run_id)
            .map_err(ExperimentError::Ledger)?
            .to_vec();
        let finished = subject_events
            .iter()
            .find(|e| e.class == "lifecycle.run.finished")
            .cloned()
            .ok_or_else(|| ExperimentError::RunNotFinished {
                run_id: attempt.run_id.clone(),
            })?;
        let (stop, oracle_failed) = terminal_of(&finished.payload);
        let outcome_class = hh_ontology::eval::derive_outcome_class(&stop, oracle_failed);

        // Consumed vector — the subject run's `control.budget.consumed` rows
        // (the run's own accounting; the slice charge rides the experiment
        // run's pool).
        let consumed = consumed_vector(&subject_events);

        // Charge the slice per dimension (idempotent on (source, dim)) then
        // complete it — the unspent remainder returns to the pool.
        let finished_ref = EventRef {
            run_id: attempt.run_id.clone(),
            event_id: finished.event_id.clone(),
        };
        {
            let mut acct = Account::open(self.store, &run_id)?;
            for (dim, amount) in &consumed {
                acct.charge(
                    &lease,
                    &ChargeRequest {
                        budget_id: attempt.budget_id.clone(),
                        quantity: ResourceQuantity {
                            dimension: *dim,
                            amount: *amount,
                            unit: dim.unit().to_string(),
                            model_ref: None,
                            measured_at: finished_ref.clone(),
                        },
                        source: finished_ref.clone(),
                        attribution: Attribution::subject(
                            attempt.run_id.clone(),
                            attempt.budget_id.clone(),
                            &self.holder,
                        ),
                        reservation_id: None,
                        cache_ttl: None,
                    },
                )?;
            }
            let _ = acct.complete(&lease, &attempt.budget_id);
        }

        // The outcome policy.
        let declared =
            view.declared
                .clone()
                .ok_or_else(|| ExperimentError::NotAnExperimentRun {
                    run_id: run_id.clone(),
                })?;
        let spec = self.spec(&declared.experiment_id)?;
        let reattempt = &spec.reattempt;
        let max_per_plan = reattempt.max_per_plan;
        let fraction_cap_ppm = reattempt.max_fraction_of_plans_ppm;
        let on_cancel = reattempt.on_cancel;
        let backoff_min = reattempt.backoff.min_ms;
        let error_classes = reattempt.error_classes_included.clone();

        let accepted = outcome_class == OutcomeClass::Scored;
        let mut superseded = false;
        let mut plan_final = false;
        let mut replanned = false;
        let mut regrade_pending = false;
        let mut paused: Option<String> = None;
        let mut extra: Vec<Event> = Vec::new();
        match outcome_class {
            OutcomeClass::Scored => {}
            OutcomeClass::Cancelled => {
                // §2.3: `on_cancel` governs `operator`/`hosting`
                // cancellations; `principal`/`parent` are final regardless
                // (the run's own authority ended it).
                let replannable = match &stop {
                    StopReason::Cancelled { by } => {
                        matches!(by, CancelledBy::Operator | CancelledBy::Hosting)
                    }
                    _ => true,
                };
                match (replannable, on_cancel) {
                    (true, CancelPolicy::Replan) => {
                        replanned = true;
                        superseded = true;
                    }
                    _ => plan_final = true,
                }
            }
            OutcomeClass::OracleFailure => {
                // An oracle failure triggers a regrade overlay, never a
                // re-run of the subject (AC-R-2.10.3-6; ADR-0155 D5). The
                // plan is final; the row is annotated `regrade`.
                plan_final = true;
                regrade_pending = true;
            }
            OutcomeClass::InfrastructureFailure => {
                let class_ok = error_classes
                    .as_ref()
                    .map(|cs| cs.iter().any(|c| c == outcome_class.as_str()))
                    .unwrap_or(true);
                let within = attempt.attempt_no < max_per_plan + 1 && class_ok;
                if within {
                    replanned = true;
                    superseded = true;
                } else {
                    plan_final = true;
                }
                // The infrastructure-suspected pause — the re-attempt fraction
                // tripped (ADR-0155 D5): fraction = replans / plans.
                let replans_so_far = view
                    .plans
                    .values()
                    .filter(|p| p.attempts.iter().any(|a| a.superseded))
                    .count() as u64
                    + u64::from(replanned);
                let total = view.plans.len().max(1) as u64;
                if replans_so_far * 1_000_000 > fraction_cap_ppm * total {
                    paused = Some(PauseReason::InfrastructureSuspected.as_str().to_string());
                    extra.push(self.mint(
                        &run_id,
                        class::PAUSED,
                        ev::paused(PauseReason::InfrastructureSuspected, None, None),
                    )?);
                }
            }
            _ => {
                // `budget_exhausted`/`context_exhausted`/`refused` — the plan
                // is terminally done (a same-shaped re-attempt cannot succeed).
                plan_final = true;
            }
        }

        // Budget utilisation — `consumed / slice_cap` ppm per dimension.
        let utilization = {
            let arm = spec.arms.iter().find(|a| a.arm_id == ps.arm_id);
            let caps = arm
                .and_then(|a| self.resolve_budget_opt(&a.eval_budget))
                .map(|s| s.hard_caps_map())
                .unwrap_or_default();
            let mut m = BTreeMap::new();
            for (dim, amount) in &consumed {
                let cap = caps.get(dim.as_str()).copied().unwrap_or(0);
                let ppm = if cap > 0 {
                    (amount.saturating_mul(1_000_000)) / cap
                } else {
                    0
                };
                m.insert(dim.as_str().to_string(), Json::Int(ppm));
            }
            Json::Obj(m)
        };

        let outcome = RunOutcome {
            run_plan_id: run_plan_id.to_string(),
            run_id: attempt.run_id.clone(),
            outcome_class,
            accepted,
            superseded,
            plan_final,
            attempt_no: attempt.attempt_no,
            budget_utilization: utilization,
            veto_tripped: veto_trips(&subject_events),
            replanned,
            paused: paused.clone(),
            regrade_pending,
        };
        // `pool_released` — the keys the attempt's `run_launched` consumed
        // (pre-C1 rows without the member re-derive from the arm; S-1).
        let pool_released = if attempt.pools.is_empty() {
            self.plan_pool_keys(&spec, &ps.arm_id)
        } else {
            attempt.pools.clone()
        };
        let mut batch = vec![self.mint(
            &run_id,
            class::RUN_SETTLED,
            ev::run_settled(&outcome, &pool_released),
        )?];
        if superseded && outcome_class == OutcomeClass::InfrastructureFailure {
            // §2.3: `run_settled` and `run_excluded{infrastructure_retry}`
            // land **before** `run_replanned`. A cancelled supersede carries
            // no exclusion row (the closed reason set has no `cancelled`
            // spelling) — its `run_settled{superseded: true}` is the record.
            batch.push(self.mint(
                &run_id,
                class::RUN_EXCLUDED,
                ev::run_excluded(
                    run_plan_id,
                    &attempt.run_id,
                    ev::exclude_reason::INFRASTRUCTURE_RETRY,
                    None,
                    &self.holder,
                ),
            )?);
        }
        if replanned {
            let not_before = now + backoff_min;
            batch.push(self.mint(
                &run_id,
                class::RUN_REPLANNED,
                ev::run_replanned(
                    run_plan_id,
                    &attempt.run_id,
                    attempt.attempt_no + 1,
                    not_before,
                ),
            )?);
        }
        batch.extend(extra);
        // `cell_completed` — every replicate of the cell accepted.
        if accepted {
            let cell_accepted = view
                .plans
                .values()
                .filter(|p| p.cell_id == ps.cell_id && p.accepted_run.is_some())
                .count() as u64
                + 1;
            let cell_planned = view
                .plans
                .values()
                .filter(|p| p.cell_id == ps.cell_id)
                .count() as u64;
            if cell_accepted == cell_planned {
                batch.push(self.mint(
                    &run_id,
                    class::CELL_COMPLETED,
                    ev::cell_completed(&ps.cell_id, cell_accepted as u32, cell_planned as u32),
                )?);
            }
        }
        self.append_chained(&run_id, &lease, batch)?;
        Ok(outcome)
    }

    /// `exclude(run_plan_id, run_id, reason, authority)` — the operator's
    /// exclusion row (§6.3/§6.5 §2.3 `exclude`; the closed reason set —
    /// a non-member refuses, never records). An `analyst_exclusion`
    /// requires `authority = human` (ADR-0162 D2; I-A5's human-origin
    /// rule — a tool, model or kernel holder never excludes a run on
    /// analyst judgement).
    pub fn exclude(
        &mut self,
        run_plan_id: &str,
        run_id: &str,
        reason: &str,
        evidence: Option<&Json>,
        authority: &str,
    ) -> Result<(), ExperimentError> {
        let (run, lease) = self.bound()?;
        let view = self.project()?;
        self.live(&view, &run)?;
        if !view.plans.contains_key(run_plan_id) {
            return Err(ExperimentError::UnknownRunPlan {
                run_plan_id: run_plan_id.to_string(),
            });
        }
        use ev::exclude_reason as ex;
        const REASONS: &[&str] = &[
            ex::INFRASTRUCTURE_RETRY,
            ex::DUPLICATE_ATTEMPT,
            ex::SUPERSEDED_BY_FORK,
            ex::PRE_REGISTERED_EXCLUSION,
            ex::BUDGET_UNMATCHED,
            ex::CONSENT_WITHDRAWN,
            ex::ANALYST_EXCLUSION,
        ];
        if !REASONS.contains(&reason) {
            return Err(ExperimentError::ExclusionRefused {
                detail: format!("exclusion reason {reason:?} is outside the closed set"),
            });
        }
        if reason == ex::ANALYST_EXCLUSION && authority != "human" {
            return Err(ExperimentError::ExclusionRefused {
                detail: format!(
                    "analyst_exclusion requires origin = human (authority {authority:?})"
                ),
            });
        }
        self.append_chained(
            &run,
            &lease,
            vec![self.mint(
                &run,
                class::RUN_EXCLUDED,
                ev::run_excluded(run_plan_id, run_id, reason, evidence, authority),
            )?],
        )?;
        Ok(())
    }

    // ── pause / resume / amend / close ───────────────────────────────────

    /// `pause(reason, probe)` — the attended pause (`operator`) or a
    /// driver-recorded pause row; `probe` is the boundary's audit tag
    /// (§6.3 `pause{probe?}`). Idempotent while paused (the second row is a
    /// no-op record).
    pub fn pause(
        &mut self,
        reason: PauseReason,
        probe: Option<&str>,
    ) -> Result<(), ExperimentError> {
        let (run_id, lease) = self.bound()?;
        let view = self.project()?;
        if view.closed.is_some() {
            return Err(ExperimentError::ExperimentClosed {
                run_id: run_id.clone(),
            });
        }
        if view.paused.is_some() {
            return Ok(());
        }
        self.append_chained(
            &run_id,
            &lease,
            vec![self.mint(&run_id, class::PAUSED, ev::paused(reason, None, probe))?],
        )?;
        Ok(())
    }

    /// `resume(probe)` — clears the pause; `probe` is the boundary's audit
    /// tag (`resumed{probe?}`).
    pub fn resume(&mut self, probe: Option<&str>) -> Result<(), ExperimentError> {
        let (run_id, lease) = self.bound()?;
        let view = self.project()?;
        if view.closed.is_some() {
            return Err(ExperimentError::ExperimentClosed {
                run_id: run_id.clone(),
            });
        }
        self.append_chained(
            &run_id,
            &lease,
            vec![self.mint(&run_id, class::RESUMED, ev::resumed(probe))?],
        )?;
        Ok(())
    }

    /// `amend(diff_ref, reason, authority)` — the post-registration amendment
    /// row; later analyses annotate `post_amendment` (§6.3; ADR-0162).
    pub fn amend(
        &mut self,
        diff_ref: &str,
        reason: &str,
        authority: &str,
    ) -> Result<(), ExperimentError> {
        let (run_id, lease) = self.bound()?;
        let view = self.project()?;
        self.live(&view, &run_id)?;
        self.append_chained(
            &run_id,
            &lease,
            vec![self.mint(
                &run_id,
                class::AMENDED,
                ev::amended(diff_ref, reason, authority),
            )?],
        )?;
        Ok(())
    }

    /// `record_bundle(bundle_id)` — the caller assembled the experiment's
    /// bundle (`kernel.bundle`); the engine records it (the report's
    /// `bundle_id`).
    pub fn record_bundle(&mut self, bundle_id: &str) -> Result<(), ExperimentError> {
        let (run_id, lease) = self.bound()?;
        self.append_chained(
            &run_id,
            &lease,
            vec![self.mint(
                &run_id,
                class::BUNDLE_ASSEMBLED,
                ev::bundle_assembled(bundle_id),
            )?],
        )?;
        Ok(())
    }

    /// `bind(run_id, cell_id)` — the producer contract's standalone bind
    /// (§6.5 §2.3; ADR-0162 D2): appends the experiment run's `run_bound`
    /// mirror naming the subject run, its declared cell and the subject's
    /// head event (`subject_head` — the subject's own `bound` chain is the
    /// proof). The subject run must exist; the cell must be declared.
    /// Engine-launched runs bind through `launch` — this verb serves the
    /// records-in producer path (a run bound without an engine launch).
    pub fn producer_bind(&mut self, run_id: &str, cell_id: &str) -> Result<(), ExperimentError> {
        let (exp_run, lease) = self.bound()?;
        let view = self.project()?;
        self.live(&view, &exp_run)?;
        let ps = view
            .plans
            .values()
            .find(|p| p.cell_id == cell_id)
            .ok_or_else(|| ExperimentError::BadPlanState {
                run_plan_id: cell_id.to_string(),
                detail: format!("no declared cell {cell_id}"),
            })?
            .clone();
        let manifest = self
            .store
            .manifest(run_id)
            .map_err(|_| ExperimentError::Unresolvable {
                detail: format!("subject run {run_id} unknown to the ledger"),
            })?
            .clone();
        let subject_head = self
            .store
            .head_event_id(run_id)
            .map_err(ExperimentError::Ledger)?;
        let attempt_no = ps.next_attempt_no;
        self.append_chained(
            &exp_run,
            &lease,
            vec![self.mint(
                &exp_run,
                class::RUN_BOUND,
                ev::run_bound_mirror(
                    run_id,
                    &ps.run_plan_id,
                    cell_id,
                    ps.replicate_index,
                    attempt_no,
                    &subject_head,
                    manifest.participant_class.as_str(),
                    "full",
                ),
            )?],
        )?;
        Ok(())
    }

    /// `record_analysis(record, report_body)` — the §6.5 §2.3 producer
    /// verb: validates the `AnalysisRecord` against the experiment view
    /// and stamps `measurement.analysis.recorded` on the experiment run.
    ///
    /// Refusals (ADR-0162 D3 — the store validates every append):
    /// - `NotAnExperimentRun` — the engine is bound to no declared
    ///   experiment;
    /// - `UnmatchedBudget` / `MissingMatchSpec` — a `ComparisonReport` in
    ///   the report body lacking `budget_match` / `benefit_kind` (the
    ///   matched-budget trail is a *required* member, never optional);
    /// - `NotPreRegistered` — `pre_registered = true` whose
    ///   `registered_analysis_ref` does not match the declared
    ///   pre-registration's `analysis_plan_ref` by identity.
    ///
    /// `post_amendment` on the record must agree with the view — a record
    /// produced after an `amended` row carries `post_amendment = true`
    /// (§6.4 §6; a false claim is a `NotPreRegistered`-family refusal: the
    /// stamp is a fact the ledger must not vouch for).
    pub fn record_analysis(
        &mut self,
        record: &hh_lab::analysis::AnalysisRecord,
        report_body: Option<&Json>,
    ) -> Result<(), ExperimentError> {
        let (exp_run, lease) = self.bound()?;
        let view = self.project()?;
        self.live(&view, &exp_run)?;
        let declared =
            view.declared
                .clone()
                .ok_or_else(|| ExperimentError::NotAnExperimentRun {
                    run_id: exp_run.clone(),
                })?;
        if let Some(r) = &record.experiment_run_id {
            if *r != exp_run {
                return Err(ExperimentError::Refusal(
                    hh_lab::experiment::ExperimentRefusal::NotPreRegistered {
                        detail: format!(
                            "record binds experiment_run_id {r} — this run is {exp_run}"
                        ),
                    },
                ));
            }
        }
        // The report body's comparisons must carry the matched-budget
        // trail (`budget_match`) and the benefit-kind stamp — the
        // producer-contract refuses a comparison that dropped them.
        if let Some(body) = report_body {
            if let Some(Json::Arr(comps)) = body.get("comparisons") {
                for c in comps {
                    if c.get("budget_match").is_none() {
                        return Err(ExperimentError::Refusal(
                            hh_lab::experiment::ExperimentRefusal::UnmatchedBudget {
                                detail: "comparison report lacks budget_match".into(),
                            },
                        ));
                    }
                    if c.get("benefit_kind").is_none() {
                        return Err(ExperimentError::Refusal(
                            hh_lab::experiment::ExperimentRefusal::MissingMatchSpec {
                                arm: "comparison report lacks benefit_kind".into(),
                            },
                        ));
                    }
                }
            }
        }
        // `pre_registered = true` — the registered ref must match the
        // experiment's pinned analysis plan *by identity* (ADR-0162 D3).
        if record.pre_registered {
            let spec = self.spec(&declared.experiment_id)?;
            let plan_ref = spec
                .pre_registration
                .as_ref()
                .map(|p| p.analysis_plan_ref.clone())
                .unwrap_or_default();
            let claimed = record.registered_analysis_ref.clone().unwrap_or_default();
            if plan_ref.is_empty() || claimed != plan_ref {
                return Err(ExperimentError::Refusal(
                    hh_lab::experiment::ExperimentRefusal::NotPreRegistered {
                        detail: format!(
                            "registered_analysis_ref {claimed:?} does not match the pinned plan {plan_ref:?}"
                        ),
                    },
                ));
            }
        }
        // `post_amendment` — a record produced after an `amended` row must
        // carry the flag (the ledger does not stamp a stale claim).
        if !view.amendments.is_empty() && !record.post_amendment {
            return Err(ExperimentError::Refusal(
                hh_lab::experiment::ExperimentRefusal::NotPreRegistered {
                    detail: "record postdates an amendment but post_amendment = false".into(),
                },
            ));
        }
        self.append_chained(
            &exp_run,
            &lease,
            vec![self.mint(
                &exp_run,
                class::ANALYSIS_RECORDED,
                ev::analysis_recorded(record),
            )?],
        )?;
        Ok(())
    }

    /// `verify()` — the experiment run's hash chain re-verification (the
    /// store's `verify` over the bound run; AC-R-2.10.5-10).
    pub fn verify(&self) -> Result<(), ExperimentError> {
        let run_id = self.bound_run()?.to_string();
        self.store.verify(&run_id).map_err(ExperimentError::Ledger)
    }

    /// `close(partial)` — the S-4 close: refuses `CloseBlocked` while
    /// `ExperimentReadyToLaunch` holds (open plans remain and `partial` was
    /// not declared); emits the `closed` drift bracket (`provider_drift =
    /// observed` — annotate, never delete, OQ-362) and the `closed` row;
    /// returns the `ExperimentReport`.
    pub fn close(&mut self, partial: bool) -> Result<ExperimentReport, ExperimentError> {
        let (run_id, lease) = self.bound()?;
        let now = self.store.now_ms();
        self.reconcile(&run_id, &lease, now)?;
        let view = self.project()?;
        if view.closed.is_some() {
            return Err(ExperimentError::ExperimentClosed {
                run_id: run_id.clone(),
            });
        }
        let declared =
            view.declared
                .clone()
                .ok_or_else(|| ExperimentError::NotAnExperimentRun {
                    run_id: run_id.clone(),
                })?;
        let spec = self.spec(&declared.experiment_id)?;

        // The `ExperimentReadyToLaunch` holds — eligible / claimed-live /
        // in-flight plans block a non-partial close.
        let open = view.open_plans(now);
        if !open.is_empty() && !partial {
            let outstanding: Vec<String> = open.iter().map(|p| p.run_plan_id.clone()).collect();
            return Err(ExperimentError::CloseBlocked {
                detail: format!(
                    "outstanding_runs={:?} holds=[ExperimentReadyToLaunch]",
                    outstanding
                ),
            });
        }

        // The closed drift bracket — fingerprint probe vs the opened bracket.
        let fingerprints = self.probe_fingerprints(&spec);
        let opened = view
            .drift_brackets
            .iter()
            .find(|b| b.phase == "opened")
            .map(|b| b.fingerprints.clone())
            .unwrap_or_default();
        let provider_drift = fingerprints
            .iter()
            .any(|(k, v)| opened.get(k).map(|o| o != v).unwrap_or(false));

        let status = if open.is_empty() {
            CloseStatus::Completed
        } else {
            CloseStatus::Partial
        };
        // E-3/E-4 — the matched-budget re-check over realised runs (§6.3
        // §2.4): per-arm matched-dim utilisation medians against the
        // declared floor (`under_utilised`), the per-comparand-group
        // `budget_match` status at the declared tolerance, and the typed
        // `n/a{not_run}` cells (`InsufficientReplicates` over realised
        // runs).
        let (under_utilised, budget_match, na_cells, utilization) =
            self.close_recheck(&spec, &view, &declared)?;
        let accepted = view.accepted_count();
        let planned = view.plans.len();
        let cells_total = view.cells().len();
        let cells_completed = view.cells_completed.len();
        let report = ExperimentReport {
            status,
            coverage: Json::obj([
                ("cells_completed", Json::Int(cells_completed as i64)),
                ("cells_planned", Json::Int(cells_total as i64)),
                ("runs_accepted", Json::Int(accepted as i64)),
                ("runs_planned", Json::Int(planned as i64)),
            ]),
            outcome_counts: view.outcome_counts.clone(),
            provider_drift,
            bundle_id: view.bundles.last().cloned(),
            watermark_set: Json::Obj(BTreeMap::from([(
                run_id.clone(),
                Json::Int(view.watermark as i64),
            )])),
            summary_ref: None,
            under_utilised,
            budget_match,
            na_cells,
            utilization,
        };
        self.append_chained(
            &run_id,
            &lease,
            vec![
                self.mint(
                    &run_id,
                    class::DRIFT_BRACKET,
                    ev::drift_bracket("closed", &fingerprints, provider_drift),
                )?,
                self.mint(&run_id, class::CLOSED, ev::closed(&report))?,
            ],
        )?;
        Ok(report)
    }

    // ── internals ────────────────────────────────────────────────────────

    /// E-3/E-4 (§6.3 §2.4; ADR-0155 D4) — the matched-budget re-check over
    /// realised runs at `close`:
    /// * `under_utilised[]` — an arm whose median `budget_utilization` on a
    ///   matched dimension falls below its `MatchSpec.utilization_floor`
    ///   is annotated (ADR-0041 M1 — "an under-utilising arm is not
    ///   cheaper"); no floor ⇒ no annotation.
    /// * `utilization` — per arm, per matched dimension, the realised
    ///   `budget_utilization` ppm distribution `{median, samples[]}` over
    ///   counted runs (E-3's distributions).
    /// * `budget_match[]` — per comparand group, consumption-medians at the
    ///   declared `tolerance` (OQ-124): `matched` | `imbalanced` (a matched
    ///   dim with no consumption evidence is `imbalanced`).
    /// * `na_cells[]` — cells rendering a typed `n/a{reason}`:
    ///   `level_ineligible` cells from the plan doc plus `not_run` cells
    ///   (`InsufficientReplicates` — accepted < `pre_registration.min_n`).
    fn close_recheck(
        &self,
        spec: &ExperimentSpec,
        view: &ExperimentView,
        declared: &Declared,
    ) -> Result<CloseRecheck, ExperimentError> {
        // Per-arm matched-dim utilisation ppm over the counted runs —
        // settled, non-superseded attempts (superseded consumption is waste,
        // never evidence).
        let mut arm_util: BTreeMap<String, BTreeMap<String, Vec<i64>>> = BTreeMap::new();
        let mut cell_accepted: BTreeMap<String, u64> = BTreeMap::new();
        for p in view.plans.values() {
            if p.accepted_run.is_some() {
                *cell_accepted.entry(p.cell_id.clone()).or_default() += 1;
            }
            for a in &p.attempts {
                if a.outcome.is_none() || a.superseded {
                    continue;
                }
                if let Some(Json::Obj(u)) = &a.budget_utilization {
                    for (d, v) in u {
                        if let Some(ppm) = v.as_int() {
                            arm_util
                                .entry(p.arm_id.clone())
                                .or_default()
                                .entry(d.clone())
                                .or_default()
                                .push(ppm);
                        }
                    }
                }
            }
        }

        // `n/a{not_run}` — a cell below the pre-registered replicate floor
        // renders `not_run` (E-4's `InsufficientReplicates` rule); the
        // `level_ineligible` cells come from the plan doc itself.
        let mut na_cells: Vec<Json> = Vec::new();
        if let Ok(Some(plan)) = self.docs.plan(&declared.plan_id) {
            for cell in &plan.cells {
                if let Some(r) = cell.na_reason {
                    na_cells.push(Json::obj([
                        ("cell_id", Json::str(&cell.cell_id)),
                        ("reason", Json::str(r.as_str())),
                    ]));
                }
            }
        }
        let min_n = spec
            .pre_registration
            .as_ref()
            .map(|p| p.min_n)
            .unwrap_or(1)
            .max(1) as u64;
        let mut cells_seen: BTreeMap<String, u64> = BTreeMap::new();
        for p in view.plans.values() {
            *cells_seen.entry(p.cell_id.clone()).or_default() += 1;
        }
        for cid in cells_seen.keys() {
            let acc = cell_accepted.get(cid).copied().unwrap_or(0);
            if acc < min_n {
                na_cells.push(Json::obj([
                    ("cell_id", Json::str(cid)),
                    ("reason", Json::str("not_run")),
                ]));
            }
        }

        // `under_utilised` — per matched dim, the arm's median utilisation
        // below the declared floor.
        let mut under_utilised: Vec<String> = Vec::new();
        for arm in &spec.arms {
            let Some(ms) = &arm.match_spec else {
                continue;
            };
            let Some(floor) = ms.utilization_floor_ppm else {
                continue;
            };
            let utils = arm_util.get(&arm.arm_id);
            let mut below = false;
            for d in &ms.dimensions {
                let med = utils
                    .and_then(|u| u.get(d.as_str()))
                    .and_then(|v| median_i64(v));
                if let Some(m) = med {
                    if m < floor {
                        below = true;
                    }
                }
            }
            if below {
                under_utilised.push(arm.arm_id.clone());
            }
        }

        // `utilization` — the E-3 distribution record: `{arm → {dim →
        // {median, samples[]}}}` over the counted runs.
        let mut utilization = BTreeMap::new();
        for (arm_id, dims) in &arm_util {
            let mut dmap = BTreeMap::new();
            for (dim, samples) in dims {
                let mut s = samples.clone();
                s.sort_unstable();
                let med = median_i64(&s).unwrap_or(0);
                // `{median, min, max, n}` — the 512-byte audit-field cap
                // keeps the full sample vector out of the event row; the
                // per-run ppm stays on each `run_settled`.
                dmap.insert(
                    dim.clone(),
                    Json::obj([
                        ("median", Json::Int(med)),
                        ("min", Json::Int(*s.first().unwrap_or(&0))),
                        ("max", Json::Int(*s.last().unwrap_or(&0))),
                        ("n", Json::Int(s.len() as i64)),
                    ]),
                );
            }
            utilization.insert(arm_id.clone(), Json::Obj(dmap));
        }
        let utilization = Json::Obj(utilization);

        // `budget_match` — per comparand group, pairwise consumption-medians
        // vs the declared tolerance (the same medians `compare` computes;
        // identical caps within a group make utilisation ppm proportional
        // to realised consumption).
        let mut groups: BTreeMap<String, Vec<(&ArmSpec, &MatchSpec)>> = BTreeMap::new();
        for arm in &spec.arms {
            let Some(ms) = &arm.match_spec else { continue };
            if ms.mode == MatchMode::None {
                continue;
            }
            let mut dims: Vec<String> = ms
                .dimensions
                .iter()
                .map(|d| d.as_str().to_string())
                .collect();
            dims.sort();
            let key = format!(
                "{}|{}|{}|{:?}|{:?}",
                ms.mode.as_str(),
                dims.join(","),
                ms.model_scope.as_str(),
                ms.cache_policy,
                ms.pricing_table_ref,
            );
            groups.entry(key).or_default().push((arm, ms));
        }
        let mut budget_match: Vec<Json> = Vec::new();
        for group in groups.values() {
            if group.len() < 2 {
                continue;
            }
            let ms = group[0].1;
            let tolerance = ms.tolerance_ppm.max(0) as u64;
            let mut status = "matched";
            let mut detail = BTreeMap::new();
            for d in &ms.dimensions {
                let dim = d.as_str();
                let medians: Vec<Option<i64>> = group
                    .iter()
                    .map(|(arm, _)| {
                        arm_util
                            .get(&arm.arm_id)
                            .and_then(|u| u.get(dim))
                            .and_then(|v| median_i64(v))
                    })
                    .collect();
                if medians.iter().any(|m| m.is_none()) {
                    // The closed set is {matched, imbalanced}: a matched dim
                    // with no consumption evidence cannot be shown matched.
                    status = "imbalanced";
                    detail.insert(dim.to_string(), Json::str("unmeasured"));
                    continue;
                }
                let flat: Vec<i64> = medians.iter().flatten().copied().collect();
                let mut worst = 0u64;
                for i in 0..flat.len() {
                    for j in (i + 1)..flat.len() {
                        let (a, b) = (flat[i], flat[j]);
                        let denom = (a.max(b)).max(1) as i128;
                        let diff = ((a - b).abs() as i128 * 1_000_000i128 / denom) as u64;
                        worst = worst.max(diff);
                    }
                }
                detail.insert(
                    dim.to_string(),
                    Json::obj([
                        (
                            "medians",
                            Json::Arr(flat.iter().map(|m| Json::Int(*m)).collect()),
                        ),
                        ("imbalance_ppm", Json::Int(worst as i64)),
                    ]),
                );
                if worst > tolerance && status == "matched" {
                    status = "imbalanced";
                }
            }
            budget_match.push(Json::obj([
                (
                    "arms",
                    Json::Arr(group.iter().map(|(a, _)| Json::str(&a.arm_id)).collect()),
                ),
                ("status", Json::str(status)),
                ("tolerance_ppm", Json::Int(tolerance as i64)),
                ("detail", Json::Obj(detail)),
            ]));
        }
        Ok((under_utilised, budget_match, na_cells, utilization))
    }

    fn spec(&self, experiment_id: &str) -> Result<ExperimentSpec, ExperimentError> {
        self.docs
            .spec(experiment_id)?
            .ok_or_else(|| ExperimentError::UnknownExperiment {
                experiment_id: experiment_id.to_string(),
            })
    }

    fn bound_run(&self) -> Result<&str, ExperimentError> {
        self.run_id
            .as_deref()
            .ok_or_else(|| ExperimentError::NotAnExperimentRun {
                run_id: "(unbound)".to_string(),
            })
    }

    fn bound(&mut self) -> Result<(String, Lease), ExperimentError> {
        let run_id = self.bound_run()?.to_string();
        let mut lease = self
            .lease
            .clone()
            .ok_or_else(|| ExperimentError::NotAnExperimentRun {
                run_id: run_id.clone(),
            })?;
        // Opportunistic renew — a long-lived engine renews inside the back
        // half of the TTL rather than fencing itself on the next append. An
        // expired lease is re-acquired (the takeover is audited); a live
        // other holder still `WouldBlock`s.
        let now = self.store.now_ms();
        if now + self.writer_ttl_ms / 2 >= lease.expires_at_ms {
            match self.store.renew(&lease) {
                Ok(renewed) => {
                    self.lease = Some(renewed.clone());
                    lease = renewed;
                }
                Err(_) => {
                    let reacquired = self
                        .store
                        .acquire_writer(&self.holder, &run_id, self.writer_ttl_ms)
                        .map_err(|e| match e {
                            hh_ledger::errors::LedgerError::WouldBlock { active_holder } => {
                                ExperimentError::WouldBlock {
                                    holder: active_holder,
                                }
                            }
                            other => ExperimentError::Ledger(other),
                        })?;
                    self.lease = Some(reacquired.clone());
                    lease = reacquired;
                }
            }
        }
        Ok((run_id, lease))
    }

    /// S-4 gates: closed → `ExperimentClosed`; paused → `ExperimentPaused`.
    fn live(&self, view: &ExperimentView, run_id: &str) -> Result<(), ExperimentError> {
        if view.closed.is_some() {
            return Err(ExperimentError::ExperimentClosed {
                run_id: run_id.to_string(),
            });
        }
        if let Some(reason) = &view.paused {
            return Err(ExperimentError::ExperimentPaused {
                run_id: run_id.to_string(),
                reason: reason.clone(),
            });
        }
        Ok(())
    }

    /// Reconcile-before-dispatch — append `claim_expired` for every recorded
    /// claim whose expiry passed.
    fn reconcile(&mut self, run_id: &str, lease: &Lease, now: u64) -> Result<(), ExperimentError> {
        let view = self.project()?;
        let expired: Vec<(&str, &str)> = view
            .plans
            .values()
            .filter_map(|p| {
                p.claim
                    .as_ref()
                    .filter(|c| c.expires_at_ms <= now)
                    .map(|c| (p.run_plan_id.as_str(), c.lease_id.as_str()))
            })
            .collect();
        if expired.is_empty() {
            return Ok(());
        }
        let mut batch = Vec::new();
        for (rpid, lease_id) in expired {
            batch.push(self.mint(
                run_id,
                class::CLAIM_EXPIRED,
                ev::claim_expired(rpid, lease_id),
            )?);
        }
        self.append_chained(run_id, lease, batch)
    }

    /// Mint a caller-supplied event (kernel producer + provenance; the store
    /// stamps seq/hash/lease generation).
    fn mint(
        &self,
        run_id: &str,
        class_name: &str,
        payload: Json,
    ) -> Result<Event, ExperimentError> {
        Ok(Event {
            event_id: self.store.alloc_id("evt"),
            class: class_name.to_string(),
            ts: self.store.ts_now(),
            hlc: None,
            producer: Producer::kernel(COMPONENT),
            scope: Scope::default(),
            parent_event_id: self
                .store
                .head_event_id(run_id)
                .map_err(ExperimentError::Ledger)?,
            causes: vec![],
            refs: vec![],
            ir_refs: vec![],
            surface_ids: BTreeMap::new(),
            provenance: Some(ProvenanceRecord::kernel(COMPONENT, self.store.now_ms())),
            content_kind: None,
            payload,
        })
    }

    /// Append a batch with the `parent_event_id` chain threaded through it —
    /// the first event parents on the committed head, each subsequent event
    /// on the previous (the append's batch-local `known_ids` rule).
    fn append_chained(
        &mut self,
        run_id: &str,
        lease: &Lease,
        events: Vec<Event>,
    ) -> Result<(), ExperimentError> {
        if events.is_empty() {
            return Ok(());
        }
        let mut out = Vec::with_capacity(events.len());
        let mut parent = events[0].parent_event_id.clone();
        for mut e in events {
            e.parent_event_id = parent.clone();
            parent = e.event_id.clone();
            out.push(e);
        }
        self.store
            .append(run_id, lease, out)
            .map_err(ExperimentError::Ledger)?;
        Ok(())
    }

    /// The experiment run's manifest (`run_kind = experiment`, engine binding).
    /// Non-agent runs carry no configuration/environment cells (ADR-0183 §C —
    /// `minimal` seeds them for `agent`; clear them here).
    fn experiment_manifest(&self, spec: &ExperimentSpec, plan: &CellPlan) -> RunManifest {
        let mut m = RunManifest::minimal(RunKind::Experiment);
        m.configuration_id = None;
        m.configuration_version_id = None;
        // The top-level pin mirrors the binding's — the experiment run's own
        // bundle carries the `Design`'s snapshot (§6.2 "in every bundle").
        m.registry_snapshot_id = spec.design.registry_snapshot_id.clone();
        m.experiment = Some(ExperimentBinding {
            experiment_id: Some(spec.experiment_id.clone()),
            plan_id: Some(plan.plan_id.clone()),
            scheduling_policy: Some(spec.scheduling.order.name().to_string()),
            reattempt_policy: Some(spec.reattempt.on_cancel.name().to_string()),
            budgets: Some(Json::obj([
                ("experiment", Json::str(&spec.budgets.experiment)),
                ("instrument", Json::str(&spec.budgets.instrument)),
            ])),
            design_ref: None,
            pre_registration_ref: None,
            match_spec_ref: None,
            suite_manifest_ref: Some(spec.suite.suite_ref.clone()),
            registry_snapshot_id: spec.design.registry_snapshot_id.clone(),
            leaderboard_targets: Vec::new(),
            ..Default::default()
        });
        m
    }

    /// Resolve a budget ref via the context resolver, then the LabDocs
    /// `budget` doc kind.
    fn resolve_budget_opt(&self, budget_ref: &str) -> Option<BudgetSpec> {
        if budget_ref.is_empty() {
            return None;
        }
        if let Some(r) = self.ctx.resolve_budget.as_deref() {
            if let Some(s) = r(budget_ref) {
                return Some(s);
            }
        }
        // Content-addressed budget docs first, then the boundary's named
        // `budgets{}` deposits (records-in — the ref spelling is the key).
        self.docs
            .get(doc_kind::BUDGET, budget_ref)
            .ok()
            .flatten()
            .or_else(|| {
                self.docs
                    .get_named(doc_kind::BUDGET, budget_ref)
                    .ok()
                    .flatten()
            })
            .and_then(|j| BudgetSpec::from_json(&j))
    }

    fn resolve_budget(&self, budget_ref: &str) -> Result<BudgetSpec, ExperimentError> {
        self.resolve_budget_opt(budget_ref)
            .ok_or_else(|| ExperimentError::Unresolvable {
                detail: format!("budget {budget_ref} unresolvable"),
            })
    }

    /// The experiment pool's root `budget_id` — the `control.budget.allocated`
    /// row with no `parent` (derived from the ledger; S-1).
    fn root_budget(&self, run_id: &str) -> Result<String, ExperimentError> {
        let events = self.store.events(run_id).map_err(ExperimentError::Ledger)?;
        for e in events {
            if e.class == "control.budget.allocated" && e.payload.get("parent").is_none() {
                if let Some(id) = e.payload.get("budget_id").and_then(Json::as_str) {
                    return Ok(id.to_string());
                }
            }
        }
        Err(ExperimentError::Unresolvable {
            detail: "no experiment budget root allocated".to_string(),
        })
    }

    /// The arm's slice spec — `eval_budget`'s hard caps under `slice` mode
    /// (an unresolvable/empty eval budget is an unbounded slice — the arm is
    /// still bounded by the pool's conservation check at allocation).
    fn slice_spec(&self, arm: &ArmSpec) -> Result<BudgetSpec, ExperimentError> {
        match self.resolve_budget_opt(&arm.eval_budget) {
            Some(eval) => {
                let mut s = BudgetSpec {
                    mode: BudgetMode::Slice,
                    ..Default::default()
                };
                for (k, v) in eval.hard_caps_map() {
                    let key =
                        DimensionKey::parse(&k).ok_or_else(|| ExperimentError::Unresolvable {
                            detail: format!("eval budget key {k} unknown"),
                        })?;
                    s.dimensions
                        .insert(key, hh_budget::spec::DimensionRule::hard(v, key));
                }
                Ok(s)
            }
            None => Ok(BudgetSpec {
                mode: BudgetMode::Slice,
                ..Default::default()
            }),
        }
    }

    /// Whether the pool funds `ps`'s slice (E-2's pre-dispatch check — the
    /// engine checks before `allocate` refuses).
    fn slice_affordable(
        &mut self,
        run_id: &str,
        _lease: &Lease,
        view: &ExperimentView,
        ps: &PlanState,
    ) -> Result<(), ExperimentError> {
        let declared =
            view.declared
                .as_ref()
                .ok_or_else(|| ExperimentError::NotAnExperimentRun {
                    run_id: run_id.to_string(),
                })?;
        let spec = self.spec(&declared.experiment_id)?;
        let arm = spec
            .arms
            .iter()
            .find(|a| a.arm_id == ps.arm_id)
            .ok_or_else(|| ExperimentError::BadPlanState {
                run_plan_id: ps.run_plan_id.clone(),
                detail: format!("arm {} absent from spec", ps.arm_id),
            })?;
        let Some(eval) = self.resolve_budget_opt(&arm.eval_budget) else {
            return Ok(()); // unbudgeted arm — the slice is unbounded
        };
        let root = self.root_budget(run_id)?;
        let acct = Account::open(&mut *self.store, run_id)?;
        for (k, need) in eval.hard_caps_map() {
            let key = DimensionKey::parse(&k).ok_or_else(|| ExperimentError::Unresolvable {
                detail: format!("eval budget key {k} unknown"),
            })?;
            if let Some(rem) = acct.remaining(&root, key) {
                if rem < need {
                    return Err(ExperimentError::InsufficientBudget {
                        detail: format!("dimension {k}: need {need}, pool remaining {rem}"),
                    });
                }
            }
        }
        Ok(())
    }

    /// `level_ref → fingerprint` over every declared level (the drift-bracket
    /// probe — `None` resolver ⇒ empty bracket).
    fn probe_fingerprints(&self, spec: &ExperimentSpec) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        if let Some(probe) = self.ctx.fingerprint.as_deref() {
            for f in &spec.factors {
                for l in &f.levels {
                    if let Some(fp) = probe(&l.ref_) {
                        out.insert(l.ref_.clone(), fp);
                    }
                }
            }
        }
        out
    }

    // ── SchedulingPolicy pools (§6.3 §2.2; AC-R-2.10.3-10) ───────────

    /// The pool keys an arm's plans consume. Key spellings:
    /// `model_snapshot:<ref>` (the arm's model_snapshot level ref),
    /// `environment_class:<class>` (the `environment_class` resolver's
    /// class for the arm's environment level — unresolvable ⇒ not a
    /// member, never coerced), `participant:<ref>` (the arm's hosted
    /// level refs), `instrument[:<name>]` (instrument pools gate every
    /// plan — the experiment's instruments are shared).
    fn plan_pool_keys(&self, spec: &ExperimentSpec, arm_id: &str) -> Vec<String> {
        if spec.scheduling.pools.is_empty() {
            return Vec::new();
        }
        let Some(arm) = spec.arms.iter().find(|a| a.arm_id == arm_id) else {
            return Vec::new();
        };
        let mut model_ref: Option<&str> = None;
        let mut env_ref: Option<&str> = None;
        let mut participants: Vec<&str> = Vec::new();
        for (fname, lid) in &arm.level_assignment {
            let Some(f) = spec.factors.iter().find(|f| &f.name == fname) else {
                continue;
            };
            let Some(l) = f.levels.iter().find(|l| &l.level_id == lid) else {
                continue;
            };
            match f.kind {
                FactorKind::ModelSnapshot => model_ref = Some(l.ref_.as_str()),
                FactorKind::Environment => env_ref = Some(l.ref_.as_str()),
                _ => {}
            }
            if l.class == LevelClass::Hosted {
                participants.push(l.ref_.as_str());
            }
        }
        let env_class =
            env_ref.and_then(|r| self.ctx.environment_class.as_deref().and_then(|f| f(r)));
        spec.scheduling
            .pools
            .iter()
            .filter(|p| {
                let (class, want) = p
                    .key
                    .split_once(':')
                    .map(|(a, b)| (a, Some(b)))
                    .unwrap_or((p.key.as_str(), None));
                match (class, want) {
                    ("model_snapshot", Some(w)) => model_ref == Some(w),
                    ("environment_class", Some(w)) => env_class.as_deref() == Some(w),
                    ("participant", Some(w)) => participants.contains(&w),
                    ("instrument", _) => true,
                    _ => false,
                }
            })
            .map(|p| p.key.clone())
            .collect()
    }

    /// `pool key → consuming plans` — in-flight attempts plus live claims
    /// (a claim reserves the slot it will dispatch under), `exclude`
    /// aside (a candidate's own pending claim must not block it).
    fn pool_usage(
        &self,
        spec: &ExperimentSpec,
        view: &ExperimentView,
        exclude: &str,
        now: u64,
    ) -> BTreeMap<String, u32> {
        let mut m: BTreeMap<String, u32> = BTreeMap::new();
        for p in view.plans.values() {
            if p.run_plan_id == exclude {
                continue;
            }
            let consuming =
                p.in_flight().is_some() || matches!(&p.claim, Some(c) if c.expires_at_ms > now);
            if !consuming {
                continue;
            }
            for k in self.plan_pool_keys(spec, &p.arm_id) {
                *m.entry(k).or_default() += 1;
            }
        }
        m
    }

    /// The capacity gate `ps` cannot dispatch under — `Some((key, limit))`
    /// on the global `max_concurrent_runs` gate or the first exhausted
    /// typed pool, else `None`. Deterministic: pool order is the spec's
    /// declared order (S-6).
    fn pool_blocker(
        &self,
        spec: &ExperimentSpec,
        view: &ExperimentView,
        ps: &PlanState,
        now: u64,
    ) -> Option<(String, u32)> {
        let max = spec.scheduling.max_concurrent_runs;
        if max > 0 {
            let consuming = view
                .plans
                .values()
                .filter(|p| {
                    p.run_plan_id != ps.run_plan_id
                        && (p.in_flight().is_some()
                            || matches!(&p.claim, Some(c) if c.expires_at_ms > now))
                })
                .count() as u32;
            if consuming >= max {
                return Some(("max_concurrent_runs".to_string(), max));
            }
        }
        let keys = self.plan_pool_keys(spec, &ps.arm_id);
        if keys.is_empty() {
            return None;
        }
        let usage = self.pool_usage(spec, view, &ps.run_plan_id, now);
        for k in keys {
            let limit = spec
                .scheduling
                .pools
                .iter()
                .find(|p| p.key == k)
                .map(|p| p.limit)
                .unwrap_or(0);
            if usage.get(&k).copied().unwrap_or(0) >= limit {
                return Some((k, limit));
            }
        }
        None
    }
}

impl Drop for ExperimentEngine<'_> {
    /// A clean shutdown releases the writer lease so a same-process re-attach
    /// doesn't wait out the TTL (a `kill -9` never runs this — the lease file
    /// stays live and fencing still holds until expiry).
    fn drop(&mut self) {
        if self.release_on_drop {
            if let Some(lease) = self.lease.take() {
                let _ = self.store.release(&lease, "engine_drop");
            }
        }
        self.run_id = None;
    }
}

/// The median of a sample (upper median on even counts — deterministic, no
/// averaging of the middle pair).
fn median_i64(v: &[i64]) -> Option<i64> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_unstable();
    Some(s[(s.len() - 1) / 2])
}

// ── Subject-run projections (settle's inputs) ───────────────────────────────

/// The subject run's terminal `(StopReason, oracle_failed)` — the
/// `lifecycle.run.finished` payload carries `stop_reason` (the canonical
/// `StopReason` JSON; a bare `reason` string maps to `Completed` when it is
/// not a `StopKind` spelling).
fn terminal_of(payload: &Json) -> (StopReason, bool) {
    let stop = payload
        .get("stop_reason")
        .and_then(StopReason::from_json)
        .unwrap_or(StopReason::Completed);
    let oracle_failed = match payload.get("outcome_class").and_then(Json::as_str) {
        Some("oracle_failure") => true,
        _ => payload
            .get("oracle_failed")
            .map(|v| matches!(v, Json::Bool(true)))
            .unwrap_or(false),
    };
    (stop, oracle_failed)
}

/// The subject run's consumed vector — the `control.budget.consumed` rows'
/// `{dimension, amount}` summed.
fn consumed_vector(events: &[hh_ledger::event::EventEnvelope]) -> Vec<(DimensionId, i64)> {
    let mut v = BTreeMap::new();
    for e in events {
        if e.class != "control.budget.consumed" {
            continue;
        }
        let p = &e.payload;
        if let (Some(d), Some(a)) = (
            p.get("dimension").and_then(Json::as_str),
            p.get("amount").and_then(Json::as_int),
        ) {
            if let Some(dim) = DimensionId::parse(d) {
                *v.entry(dim).or_insert(0) += a;
            }
        }
    }
    v.into_iter().collect()
}

/// The subject run's veto trips — `security.invariant.violated` /
/// `control.veto.*` rows carrying `veto_id`/`metric` (annotate-only at
/// settle; ADR-0047).
fn veto_trips(events: &[hh_ledger::event::EventEnvelope]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for e in events {
        if !e.class.starts_with("security.invariant") && !e.class.starts_with("control.veto") {
            continue;
        }
        for k in ["veto_id", "metric", "invariant"] {
            if let Some(v) = e.payload.get(k).and_then(Json::as_str) {
                out.insert(v.to_string());
            }
        }
    }
    out.into_iter().collect()
}
