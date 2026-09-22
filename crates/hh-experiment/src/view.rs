//! `ExperimentView` — the scheduler's fold over the experiment run's ledger
//! (spec §6.3 §2.2 S-1: "no memory-only state — every view is
//! `project(experiment_run, view, until_seq)` with rebuild equality";
//! ADR-0155 D2–D3). The fold is a pure function of the committed envelope
//! stream — the same bytes rebuild the same view, so `restore` after a kill
//! at any of KP-E1…E5 sees exactly the state the ledger committed.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_ledger::event::EventEnvelope;
use hh_ontology::control::OutcomeClass;
use hh_wire::json::Json;

use crate::events::class;

fn s(v: &Json, k: &str) -> Option<String> {
    v.get(k).and_then(Json::as_str).map(str::to_string)
}

fn i(v: &Json, k: &str) -> Option<i64> {
    v.get(k).and_then(Json::as_int)
}

fn b(v: &Json, k: &str) -> Option<bool> {
    match v.get(k) {
        Some(Json::Bool(x)) => Some(*x),
        _ => None,
    }
}

/// A live (or last-known) claim on a run plan.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimState {
    /// The scoped lease id (`resource(run_plan:<id>)`).
    pub lease_id: String,
    /// The claimant.
    pub holder: String,
    /// The expiry (wall ms — evaluated by the engine's clock, never the fold).
    pub expires_at_ms: u64,
}

/// One launched attempt on a run plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Attempt {
    /// The subject run id.
    pub run_id: String,
    /// The attempt ordinal (1-based; `run_replanned` admits `n + 1`).
    pub attempt_no: u32,
    /// The slice budget the run was allocated.
    pub budget_id: String,
    /// Whether the experiment run's `run_bound` mirror landed.
    pub bound: bool,
    /// The settled outcome (`None` while in flight).
    pub outcome: Option<OutcomeClass>,
    /// Whether this attempt is the plan's accepted run (S-2).
    pub accepted: bool,
    /// Whether the attempt was superseded (infrastructure retry).
    pub superseded: bool,
    /// The `run_excluded` reason, when excluded.
    pub excluded: Option<String>,
    /// Veto trips recorded at settle (annotate-only).
    pub veto_tripped: Vec<String>,
    /// The settle-time budget utilisation record.
    pub budget_utilization: Option<Json>,
}

/// The scheduler's per-plan state.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanState {
    /// The exactly-once key.
    pub run_plan_id: String,
    /// The cell.
    pub cell_id: String,
    /// The arm.
    pub arm_id: String,
    /// The task.
    pub task_id: String,
    /// The replicate within the cell.
    pub replicate_index: u32,
    /// The seeded exact-bytes configuration coordinate.
    pub configuration_version_id: String,
    /// The task's split label.
    pub split_label: String,
    /// The plan's seed material (re-attempts reuse it verbatim — §2.3).
    pub seed_material: Json,
    /// The recorded schedule position (S-6 — derived from
    /// `permutation_seed`, recorded as a ledger fact).
    pub order_pos: u64,
    /// The live claim (`run_claimed` minus `claim_expired`/consumed).
    pub claim: Option<ClaimState>,
    /// The launched attempts, in launch order.
    pub attempts: Vec<Attempt>,
    /// The next admitted attempt ordinal (`run_replanned` bumps it; a launch
    /// consumes it).
    pub next_attempt_no: u32,
    /// The re-attempt backoff deadline (`run_replanned.not_before_ms`; `0` =
    /// eligible immediately). The plan is not eligible before it — the §2.3
    /// backoff schedule is a ledger fact, not engine memory (S-1/S-6).
    pub not_before_ms: u64,
    /// The plan's accepted run (at most one — S-2).
    pub accepted_run: Option<String>,
    /// The plan is terminally done without an accepted run (a final
    /// cancellation, or a re-attempt budget exhausted without a replan).
    pub finished_final: bool,
}

impl PlanState {
    /// The in-flight attempt (launched, not settled), if any.
    pub fn in_flight(&self) -> Option<&Attempt> {
        self.attempts.iter().rev().find(|a| a.outcome.is_none() && !a.superseded)
    }

    /// Whether the plan may be dispatched at `now_ms` (reconcile-before-
    /// dispatch: an expired claim reads as eligible, the observer appends
    /// `claim_expired`; a re-attempt inside its backoff window waits).
    pub fn eligible_at(&self, now_ms: u64) -> bool {
        if self.accepted_run.is_some() || self.finished_final {
            return false;
        }
        if self.in_flight().is_some() || now_ms < self.not_before_ms {
            return false;
        }
        match &self.claim {
            Some(c) => c.expires_at_ms <= now_ms,
            None => true,
        }
    }

    /// Whether the plan counts against `close`'s precondition (eligible or
    /// claimed live at `now_ms`).
    pub fn open_at(&self, now_ms: u64) -> bool {
        self.eligible_at(now_ms)
            || self.in_flight().is_some()
            || matches!(&self.claim, Some(c) if c.expires_at_ms > now_ms)
    }
}

/// The fold's `declared` record — the declaration row's committed members.
/// `experiment_id`/`plan_id` are the content addresses of the spec/`CellPlan`
/// documents (`LabDocs`); the documents themselves are not inlined (Rule C —
/// audit members are bounded).
#[derive(Debug, Clone, PartialEq)]
pub struct Declared {
    /// The experiment's content address (the spec document's id).
    pub experiment_id: String,
    /// The `CellPlan` content address (the plan document's id).
    pub plan_id: String,
    /// The experiment kind.
    pub kind: String,
    /// Whether rows are comparable across arms (`false` on `exploratory`).
    pub comparable: bool,
    /// The pinned registry snapshot (`None` = unpinned — refused at register
    /// for matched kinds, so present on every declared matched spec).
    pub registry_snapshot_id: Option<String>,
    /// The declared cell count (the plan document's `cells.len()`).
    pub n_cells: u64,
    /// The declared run-plan count.
    pub n_run_plans: u64,
}

/// A drift bracket row (`opened`/`closed`).
#[derive(Debug, Clone, PartialEq)]
pub struct DriftBracket {
    /// `opened | closed`.
    pub phase: String,
    /// `level_ref → fingerprint` at the probe.
    pub fingerprints: BTreeMap<String, String>,
    /// Whether the close probe observed a change (`provider_drift = observed`
    /// annotations — never deletions).
    pub provider_drift: bool,
}

/// The scheduler view — the whole scheduling truth of one experiment run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExperimentView {
    /// The `declared` record (`None` pre-declaration).
    pub declared: Option<Declared>,
    /// `run_plan_id → PlanState` (insertion-ordered by plan emission).
    pub plans: BTreeMap<String, PlanState>,
    /// `cell_id → accepted count` (completed cells).
    pub cells_completed: BTreeMap<String, u32>,
    /// The live pause reason (`paused` minus `resumed`).
    pub paused: Option<String>,
    /// The `closed` payload (`Some` = closed).
    pub closed: Option<Json>,
    /// The drift brackets, in order.
    pub drift_brackets: Vec<DriftBracket>,
    /// Assembled bundle ids.
    pub bundles: Vec<String>,
    /// `run_excluded` rows as `(run_plan_id, run_id, reason)`.
    pub exclusions: Vec<(String, String, String)>,
    /// Superseded runs whose consumption reports as `wasted_runs` (§2.3).
    pub wasted_runs: Vec<String>,
    /// `amended` rows (each marks later analyses `post_amendment`).
    pub amendments: Vec<Json>,
    /// `outcome_class → count` over settled runs (all attempts).
    pub outcome_counts: BTreeMap<String, u64>,
    /// `provider_drift = observed` — set when a `closed` bracket reports a
    /// change; rows annotate, never delete.
    pub provider_drift: bool,
    /// The last folded seq (the rebuild watermark).
    pub watermark: u64,
}

impl ExperimentView {
    /// `project(run, events)` — the pure fold (S-1). Total over the closed
    /// family; unknown classes are ignored (the run may carry `lifecycle.*`/
    /// `control.budget.*` rows beside the experiment family).
    pub fn fold(events: &[EventEnvelope]) -> ExperimentView {
        let mut v = ExperimentView::default();
        for e in events {
            v.apply(e);
        }
        v
    }

    /// Fold one envelope.
    pub fn apply(&mut self, e: &EventEnvelope) {
        self.watermark = e.seq;
        let p = &e.payload;
        match e.class.as_str() {
            c if c == class::DECLARED => {
                self.declared = Some(Declared {
                    experiment_id: s(p, "experiment_id").unwrap_or_default(),
                    plan_id: s(p, "plan_id").unwrap_or_default(),
                    kind: s(p, "kind").unwrap_or_default(),
                    comparable: b(p, "comparable").unwrap_or(true),
                    registry_snapshot_id: s(p, "registry_snapshot_id"),
                    n_cells: i(p, "n_cells").unwrap_or(0) as u64,
                    n_run_plans: i(p, "n_run_plans").unwrap_or(0) as u64,
                });
            }
            c if c == class::RUN_PLANNED => {
                let Some(rpid) = s(p, "run_plan_id") else { return };
                self.plans.insert(
                    rpid.clone(),
                    PlanState {
                        run_plan_id: rpid,
                        cell_id: s(p, "cell_id").unwrap_or_default(),
                        arm_id: s(p, "arm_id").unwrap_or_default(),
                        task_id: s(p, "task_id").unwrap_or_default(),
                        replicate_index: i(p, "replicate_index").unwrap_or(0) as u32,
                        configuration_version_id: s(p, "configuration_version_id")
                            .unwrap_or_default(),
                        split_label: s(p, "split_label").unwrap_or_default(),
                        seed_material: p.get("seed_material").cloned().unwrap_or(Json::Null),
                        order_pos: i(p, "order_pos").unwrap_or(0) as u64,
                        claim: None,
                        attempts: Vec::new(),
                        next_attempt_no: 1,
                        not_before_ms: 0,
                        accepted_run: None,
                        finished_final: false,
                    },
                );
            }
            c if c == class::RUN_CLAIMED => {
                let Some(rpid) = s(p, "run_plan_id") else { return };
                if let Some(ps) = self.plans.get_mut(&rpid) {
                    ps.claim = Some(ClaimState {
                        lease_id: s(p, "lease_id").unwrap_or_default(),
                        holder: s(p, "holder").unwrap_or_default(),
                        expires_at_ms: i(p, "expires_at_ms").unwrap_or(0) as u64,
                    });
                }
            }
            c if c == class::CLAIM_EXPIRED => {
                let Some(rpid) = s(p, "run_plan_id") else { return };
                let lease = s(p, "lease_id");
                if let Some(ps) = self.plans.get_mut(&rpid) {
                    if ps.claim.as_ref().map(|c| &c.lease_id) == lease.as_ref() {
                        ps.claim = None;
                    }
                }
            }
            c if c == class::RUN_LAUNCHED => {
                let Some(rpid) = s(p, "run_plan_id") else { return };
                let Some(run_id) = s(p, "run_id") else { return };
                if let Some(ps) = self.plans.get_mut(&rpid) {
                    ps.claim = None; // the claim consumed into the launch
                    ps.attempts.push(Attempt {
                        run_id,
                        attempt_no: i(p, "attempt_no").unwrap_or(1) as u32,
                        budget_id: s(p, "budget_id").unwrap_or_default(),
                        bound: false,
                        outcome: None,
                        accepted: false,
                        superseded: false,
                        excluded: None,
                        veto_tripped: Vec::new(),
                        budget_utilization: None,
                    });
                }
            }
            c if c == class::RUN_BOUND => {
                // The experiment run's mirror — `{run_id, …}`.
                let Some(run_id) = s(p, "run_id") else { return };
                if let Some(ps) = self
                    .plans
                    .values_mut()
                    .find(|ps| ps.attempts.iter().any(|a| a.run_id == run_id))
                {
                    if let Some(a) = ps.attempts.iter_mut().find(|a| a.run_id == run_id) {
                        a.bound = true;
                    }
                }
            }
            c if c == class::RUN_SETTLED => {
                let Some(rpid) = s(p, "run_plan_id") else { return };
                let Some(run_id) = s(p, "run_id") else { return };
                let outcome = s(p, "outcome_class").and_then(|o| OutcomeClass::parse(&o));
                let accepted = b(p, "accepted").unwrap_or(false);
                let superseded = b(p, "superseded").unwrap_or(false);
                let plan_final = b(p, "plan_final").unwrap_or(false);
                if let Some(oc) = &outcome {
                    *self.outcome_counts.entry(oc.as_str().to_string()).or_insert(0) += 1;
                }
                if let Some(ps) = self.plans.get_mut(&rpid) {
                    if let Some(a) = ps.attempts.iter_mut().find(|a| a.run_id == run_id) {
                        a.outcome = outcome;
                        a.accepted = accepted;
                        a.superseded = superseded;
                        a.budget_utilization = p.get("budget_utilization").cloned();
                        a.veto_tripped = p
                            .get("veto_tripped")
                            .and_then(|v| match v {
                                Json::Arr(items) => Some(
                                    items.iter().filter_map(Json::as_str).map(str::to_string).collect(),
                                ),
                                _ => None,
                            })
                            .unwrap_or_default();
                    }
                    if accepted {
                        ps.accepted_run = Some(run_id.clone());
                    }
                    if superseded {
                        self.wasted_runs.push(run_id.clone());
                    }
                    if plan_final {
                        ps.finished_final = true;
                    }
                }
            }
            c if c == class::RUN_EXCLUDED => {
                let (Some(rpid), Some(run_id)) = (s(p, "run_plan_id"), s(p, "run_id")) else {
                    // Exclude rows always name both members when plan-scoped.
                    if let (Some(run_id), Some(reason)) = (s(p, "run_id"), s(p, "reason")) {
                        self.exclusions.push((String::new(), run_id, reason));
                    }
                    return;
                };
                let reason = s(p, "reason").unwrap_or_default();
                self.exclusions.push((rpid.clone(), run_id.clone(), reason.clone()));
                if let Some(ps) = self.plans.get_mut(&rpid) {
                    if let Some(a) = ps.attempts.iter_mut().find(|a| a.run_id == run_id) {
                        a.excluded = Some(reason);
                    }
                }
            }
            c if c == class::RUN_REPLANNED => {
                let Some(rpid) = s(p, "run_plan_id") else { return };
                if let Some(ps) = self.plans.get_mut(&rpid) {
                    ps.next_attempt_no = i(p, "attempt_no").unwrap_or(2) as u32;
                    ps.not_before_ms = i(p, "not_before_ms").unwrap_or(0) as u64;
                    ps.finished_final = false;
                }
            }
            c if c == class::CELL_COMPLETED => {
                if let Some(cid) = s(p, "cell_id") {
                    let n = i(p, "accepted").unwrap_or(0) as u32;
                    self.cells_completed.insert(cid, n);
                }
            }
            c if c == class::PAUSED => {
                self.paused = Some(s(p, "reason").unwrap_or_else(|| "operator".to_string()));
            }
            c if c == class::RESUMED => {
                self.paused = None;
            }
            c if c == class::CLOSED => {
                self.closed = Some(p.clone());
            }
            c if c == class::DRIFT_BRACKET => {
                let mut fps = BTreeMap::new();
                if let Some(Json::Obj(m)) = p.get("fingerprints") {
                    for (k, v) in m {
                        if let Some(fp) = v.as_str() {
                            fps.insert(k.clone(), fp.to_string());
                        }
                    }
                }
                let drift = b(p, "provider_drift").unwrap_or(false);
                if drift {
                    self.provider_drift = true;
                }
                self.drift_brackets.push(DriftBracket {
                    phase: s(p, "phase").unwrap_or_default(),
                    fingerprints: fps,
                    provider_drift: drift,
                });
            }
            c if c == class::BUNDLE_ASSEMBLED => {
                if let Some(id) = s(p, "bundle_id") {
                    self.bundles.push(id);
                }
            }
            c if c == class::AMENDED => {
                self.amendments.push(p.clone());
            }
            _ => {}
        }
    }

    /// The eligible run plans in schedule order at `now_ms`.
    pub fn eligible_at(&self, now_ms: u64) -> Vec<&PlanState> {
        let mut v: Vec<&PlanState> = self
            .plans
            .values()
            .filter(|p| p.eligible_at(now_ms))
            .collect();
        v.sort_by_key(|p| p.order_pos);
        v
    }

    /// The plans still open (eligible/claimed/in-flight) at `now_ms`.
    pub fn open_plans(&self, now_ms: u64) -> Vec<&PlanState> {
        self.plans.values().filter(|p| p.open_at(now_ms)).collect()
    }

    /// Plans whose recorded claim outlives `now_ms` are claimed-live.
    pub fn claimed_live(&self, now_ms: u64) -> Vec<&PlanState> {
        self.plans
            .values()
            .filter(|p| matches!(&p.claim, Some(c) if c.expires_at_ms > now_ms))
            .collect()
    }

    /// The count of in-flight (launched-unsettled) runs.
    pub fn in_flight_count(&self) -> usize {
        self.plans.values().filter(|p| p.in_flight().is_some()).count()
    }

    /// The accepted-run count.
    pub fn accepted_count(&self) -> usize {
        self.plans.values().filter(|p| p.accepted_run.is_some()).count()
    }

    /// The distinct cells the plan covers.
    pub fn cells(&self) -> BTreeSet<String> {
        self.plans.values().map(|p| p.cell_id.clone()).collect()
    }
}
