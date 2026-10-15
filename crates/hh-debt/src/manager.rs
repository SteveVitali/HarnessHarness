//! `DebtManager` — the durable driver (S6.1b; §5h.6 §2/§5–§6): one
//! registry run carries the `lifecycle.debt.*` book of record (the S5.4
//! convention — the registry audit run); the manager folds it into a
//! `ManagerView`, and every op mints its rows through
//! `Store::commit_kernel_row_for` (kernel producer `hh-debt` — one
//! authority path, audit-grade, provenance-bearing).
//!
//! Ops: `register` (the service record lands); `sweep` (the standing
//! monitor — probation open + `evaluate_debt` + `schedule_removal_test` +
//! the reflexive verdict + `sweep.completed`); `settle` (verdict in →
//! `removal_test.settled` + any status transitions); `retire` (the
//! human-sealed gate — `hh_lab::debt::retire`); `propose` (the
//! evolution-origin gate — proposal only); `index`/`report` (the
//! records-in fleet report — `hh_lab::debt::debt_index` +
//! `AssumptionDebtHealth`, notices routed only to declared sinks).

use hh_hir::refs::RunRef;
use hh_lab::debt::{
    evaluate_debt, settle_removal_test, AssumptionDebtHealth, DebtIndexEntry, DebtTransition,
};
use hh_ledger::event::EventEnvelope;
use hh_ledger::store::Store;
use hh_ontology::debt::{
    DebtPolicy, DebtStatus, EvidenceGrade, ExpiryKind, RemovalTestKind, RemovalVerdict, Verdict,
};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::errors::{DebtManagerError, Refusal};
use crate::propose::{propose_retirement, ProposalDiff, RetirementProposal};
use crate::records::{DebtManagerRecord, SchedulableEntry, ScheduledTest, SweepEntry, SweepReport};
use crate::reflexive::evaluate_reflexive;
use crate::schedule::{priority_class, schedule_removal_test, ScheduleContext, ScheduleOutcome};
use crate::view::ManagerView;

/// The kernel producer name every manager-minted row carries (one
/// authority path — `commit_kernel_row_for`).
pub const COMPONENT: &str = "hh-debt";

/// `DebtManager` — the durable-prefix service handle. `open`/`ensure` are
/// the same call: the durable prefix *is* the state (CC3 — restart
/// rebuild equality is structural, and the fold is deterministic).
#[derive(Debug)]
pub struct DebtManager {
    /// The registry run the manager's book of record rides.
    pub run_id: String,
    /// The folded view.
    pub view: ManagerView,
}

type Res<T> = Result<T, DebtManagerError>;

impl DebtManager {
    /// `open(run_id)` — fold the run's durable prefix into the live view
    /// (the restart path is identical — `ManagerView::fold` is a pure
    /// function of the prefix).
    pub fn open(store: &Store, run_id: &RunRef) -> Res<DebtManager> {
        let events = store.events(&run_id.run).map_err(DebtManagerError::Store)?;
        let view = ManagerView::fold(events)?;
        Ok(DebtManager {
            run_id: run_id.run.clone(),
            view,
        })
    }

    /// `register(record)` — the service record lands as
    /// `lifecycle.debt.service.registered{manager_id, record}` (the
    /// record validates member-level before the mint — the reflexive
    /// record + maturity + policy spellings).
    pub fn register(
        &mut self,
        store: &mut Store,
        record: &DebtManagerRecord,
    ) -> Res<EventEnvelope> {
        record.validate()?;
        let payload = Json::obj([
            ("manager_id", Json::str(&record.manager_id)),
            ("record", record.to_json()),
        ]);
        self.emit(store, "lifecycle.debt.service.registered", payload, vec![])
    }

    /// The registered manager's service record (`UnknownManager` when the
    /// prefix carries none).
    pub fn manager(&self, manager_id: &str) -> Res<DebtManagerRecord> {
        let rec = self
            .view
            .managers
            .get(manager_id)
            .ok_or_else(|| Refusal::UnknownManager {
                manager_id: manager_id.to_string(),
            })?;
        DebtManagerRecord::from_json(rec)
    }

    /// Mint one kernel row + fold the tail.
    fn emit(
        &mut self,
        store: &mut Store,
        class: &str,
        payload: Json,
        causes: Vec<hh_ledger::manifest::EventRef>,
    ) -> Res<EventEnvelope> {
        let env = store
            .commit_kernel_row_for(COMPONENT, &self.run_id, class, payload, vec![], causes)
            .map_err(DebtManagerError::Store)?;
        // Fold the one minted envelope — `fold` wholesale and `fold_tail`
        // per-row agree by construction, and the live view never
        // double-counts a `sweep.completed`/`settled` row.
        self.view.fold_tail(std::slice::from_ref(&env));
        Ok(env)
    }

    /// `sweep(manager_id, entries, now_ms, reserve)` — the standing
    /// monitor (the `DebtPolicy.schedule` cadence's unit of work):
    ///
    /// 1. **probation** — every `hypothesized` record without a ledgered
    ///    entry opens one (`probation.opened{due_at = now +
    ///    hypothesized_max_age}` — DF-S5.4-1's book of record).
    /// 2. **evaluate** — each entry's observables gain the ledgered
    ///    probation leg, then `evaluate_debt` runs; each transition mints
    ///    `status.changed` (the transition's `causes[]`/`evidence_ref`
    ///    ride the row — the §5h.7 gate resolves the ledgered record).
    /// 3. **schedule** — non-retired entries order by the `expiry_urgency`
    ///    classes; `schedule_removal_test` under the bound +
    ///    instrument-budget check mints `removal_test.scheduled` or
    ///    records the `Deferred{reason}`.
    /// 4. **reflexive** — the home-16 `no_dead_weight_found` verdict over
    ///    the folded window; a conclusive verdict the book hasn't already
    ///    recorded mints `removal_test.settled` (the honest end state
    ///    still needs the human-sealed `retire` — never automatic).
    /// 5. **sweep.completed** — the report row + the returned
    ///    `SweepReport`.
    pub fn sweep(
        &mut self,
        store: &mut Store,
        manager_id: &str,
        entries: &[SweepEntry],
        now_ms: u64,
        reserve: Option<&dyn Fn(&str) -> bool>,
    ) -> Res<SweepReport> {
        let mgr = self.manager(manager_id)?;
        let policy = mgr.policy.clone();
        let mut report = SweepReport {
            sweep_seq: self.view.sweep_count + 1,
            now_ms,
            evaluated: 0,
            transitions: Vec::new(),
            probation_opened: Vec::new(),
            scheduled: Vec::new(),
            deferred: Vec::new(),
            reflexive_verdict: None,
        };

        // (1) probation — hypothesized records open a ledgered entry.
        for e in entries {
            if e.record.evidence_grade() == EvidenceGrade::Hypothesized
                && !self.view.probation.contains_key(&e.debt_ref)
            {
                let due = now_ms.saturating_add(policy.hypothesized_max_age_ms);
                self.emit(
                    store,
                    "lifecycle.debt.probation.opened",
                    Json::obj([
                        ("debt_ref", Json::str(&e.debt_ref)),
                        ("opened_at_ms", Json::Int(now_ms as i64)),
                        ("due_at_ms", Json::Int(due as i64)),
                        ("grade", Json::str("hypothesized")),
                    ]),
                    vec![],
                )?;
                report.probation_opened.push(e.debt_ref.clone());
            }
        }

        // (2) evaluate — the all-home trigger set through the one pure
        // evaluator; the folded status is the current status.
        let mut currents: Vec<(DebtStatus, bool)> = Vec::new();
        for e in entries {
            report.evaluated += 1;
            let cur = self
                .view
                .status
                .get(&e.debt_ref)
                .copied()
                .unwrap_or(e.record.status);
            let mut record = e.record.clone();
            record.status = cur;
            let mut obs = e.observables.clone();
            if obs.probation.is_none() {
                obs.probation = self.view.ledgered_probation(&e.debt_ref);
            }
            let transitions = evaluate_debt(&e.debt_ref, &record, e.home, &obs, now_ms, &policy);
            for t in &transitions {
                self.emit(store, "lifecycle.debt.status.changed", t.to_json(), vec![])?;
            }
            let final_status = transitions.last().map(|t| t.to).unwrap_or(cur);
            report.transitions.extend(transitions);
            currents.push((
                final_status,
                self.view
                    .probation
                    .get(&e.debt_ref)
                    .map(|p| p.due_at_ms <= now_ms)
                    .unwrap_or(false),
            ));
        }

        // (3) schedule — the `expiry_urgency` order over schedulable
        // entries; the bound and the instrument-budget check defer, never
        // silently drop.
        let mut schedulable: Vec<(SchedulableEntry, &SweepEntry)> = Vec::new();
        for (e, (cur, probation_due)) in entries.iter().zip(currents.iter()) {
            let entry = SchedulableEntry {
                debt_ref: e.debt_ref.clone(),
                current_status: *cur,
                evidence_grade: e.record.evidence_grade(),
                used: !e.used_by.is_empty(),
                probation_due: *probation_due,
                next_time_expiry_ms: e
                    .record
                    .expiry
                    .as_ref()
                    .and_then(|x| x.params.until)
                    .or_else(|| {
                        (e.record.expiry_condition.kind == ExpiryKind::Date)
                            .then(|| e.record.expiry_condition.value.clone())
                            .flatten()
                            .and_then(|v| v.parse().ok())
                    }),
                removal_kind: e.record.removal_test.as_ref().map(|t| t.kind),
                home: e.home,
            };
            if *cur != DebtStatus::Retired && entry.removal_kind.is_some() {
                schedulable.push((entry, e));
            }
        }
        schedulable.sort_by(|(a, _), (b, _)| {
            (
                priority_class(a),
                a.next_time_expiry_ms.unwrap_or(u64::MAX),
                &a.debt_ref,
            )
                .cmp(&(
                    priority_class(b),
                    b.next_time_expiry_ms.unwrap_or(u64::MAX),
                    &b.debt_ref,
                ))
        });
        for (entry, e) in &schedulable {
            if self.view.is_test_open(&entry.debt_ref) {
                continue; // already scheduled — idempotent.
            }
            let ctx = ScheduleContext {
                open_tests: self.view.open_test_count(),
                reserve,
                on_cadence: true,
            };
            match schedule_removal_test(entry, &e.record, e.template.as_ref(), &policy, &ctx)? {
                ScheduleOutcome::Scheduled {
                    spec,
                    priority_class: class,
                } => {
                    let kind = entry
                        .removal_kind
                        .unwrap_or(RemovalTestKind::RetirementExperiment);
                    let test = ScheduledTest {
                        debt_ref: entry.debt_ref.clone(),
                        kind,
                        experiment_id: spec.experiment_id.clone(),
                        spec: *spec,
                        priority_class: class,
                    };
                    self.emit(
                        store,
                        "lifecycle.debt.removal_test.scheduled",
                        test.to_json(),
                        vec![],
                    )?;
                    report.scheduled.push(test);
                }
                ScheduleOutcome::Deferred { reason } => {
                    report
                        .deferred
                        .push((entry.debt_ref.clone(), reason.name()));
                }
            }
        }

        // (4) reflexive — the home-16 verdict over the folded window. A
        // conclusive verdict the book hasn't recorded mints `settled`;
        // retirement itself is still the human-sealed gate.
        let reflexive_ref = mgr.reflexive_debt_ref();
        if let Some(test) = &mgr.reflexive_debt.removal_test {
            if let Some(window) = test.window {
                let sweep_ref = format!("sweep:{}", report.sweep_seq);
                let verdict =
                    evaluate_reflexive(&self.view, &reflexive_ref, window, now_ms, &sweep_ref);
                report.reflexive_verdict = Some(verdict.clone());
                let already = self
                    .view
                    .settled
                    .iter()
                    .rev()
                    .find(|(_, v)| v.debt_ref == reflexive_ref)
                    .map(|(_, v)| v.verdict);
                if verdict.verdict != Verdict::Inconclusive && already != Some(verdict.verdict) {
                    self.emit(
                        store,
                        "lifecycle.debt.removal_test.settled",
                        verdict.to_json(),
                        vec![],
                    )?;
                }
            }
        }

        // (5) sweep.completed — the durable report row. `cadence` carries
        // the operative `DebtPolicy.schedule` spelling (AC-R-2.9.6-12 —
        // the record's check-cadence member; R2.17 makes it load-bearing
        // on the durable row, never a carried-but-unread field).
        self.emit(
            store,
            "lifecycle.debt.sweep.completed",
            Json::obj([
                ("sweep_seq", Json::Int(report.sweep_seq as i64)),
                ("kind", Json::str("cadence")),
                ("cadence", Json::str(&policy.schedule)),
                ("now_ms", Json::Int(now_ms as i64)),
                ("evaluated", Json::Int(report.evaluated as i64)),
                ("transitions", Json::Int(report.transitions.len() as i64)),
                (
                    "probation_opened",
                    Json::Int(report.probation_opened.len() as i64),
                ),
                ("scheduled", Json::Int(report.scheduled.len() as i64)),
                ("deferred", Json::Int(report.deferred.len() as i64)),
                (
                    "reflexive_verdict",
                    report
                        .reflexive_verdict
                        .as_ref()
                        .map(|v| Json::str(v.verdict.name()))
                        .unwrap_or(Json::Null),
                ),
            ]),
            vec![],
        )?;
        Ok(report)
    }

    /// `post_import_sweep(manager_id, snapshot_ref, sweep, entries, now_ms,
    /// reserve)` — the S6.4 import-driven sweep (R-2.9.8 §2.3): the Lab's
    /// `hh_lab::coevolution::post_import_sweep` partitions the scoped
    /// debts (`covered`/`scheduled`/`unchanged`) — the manager mints the
    /// durable half:
    ///
    /// - **covered** — every `model_version_change`-conditioned record
    ///   whose `scope.model_selectors` covers the imported snapshot
    ///   transitions `→ expiring{trigger: model_version_change}` (the
    ///   import *is* the trigger — the snapshot ref rides
    ///   `evidence_ref`/`causes`, never silently folded).
    /// - **scheduled** — scoped records the snapshot does *not* cover
    ///   run `schedule_removal_test` (the reverse-sweep side) under the
    ///   same bound + instrument-budget check as `sweep`; the mint is
    ///   `removal_test.scheduled`, the non-outcome `deferred{reason}`.
    /// - `entries` is the caller's projection (`record.rule_id` keyed —
    ///   the lab-side sweep's currency); a partition member with no
    ///   projected entry lands in `deferred{entry_not_projected}`
    ///   rather than vanishing.
    /// - `sweep.completed` carries `kind: post_import` +
    ///   `trigger_ref` so the audit fold attributes the sweep to the
    ///   import, never to cadence.
    #[allow(clippy::too_many_arguments)] // the op's record is the §2.3 sweep shape — the arity is the record's.
    pub fn post_import_sweep(
        &mut self,
        store: &mut Store,
        manager_id: &str,
        snapshot_ref: &str,
        sweep: &hh_lab::coevolution::PostImportSweep,
        entries: &[SweepEntry],
        now_ms: u64,
        reserve: Option<&dyn Fn(&str) -> bool>,
    ) -> Res<SweepReport> {
        let mgr = self.manager(manager_id)?;
        let policy = mgr.policy.clone();
        let mut report = SweepReport {
            sweep_seq: self.view.sweep_count + 1,
            now_ms,
            evaluated: (sweep.covered.len() + sweep.scheduled.len()) as u64,
            transitions: Vec::new(),
            probation_opened: Vec::new(),
            scheduled: Vec::new(),
            deferred: Vec::new(),
            reflexive_verdict: None,
        };
        let by_rule = |rule_id: &str| entries.iter().find(|e| e.record.rule_id == rule_id);

        // (1) covered — the `model_version_change` trigger fires on every
        // scope-covered, version-conditioned record (the import is the
        // version change; `evaluate_expiry`'s observable half fires at
        // serve-time, this leg is the ledgered import-time half).
        for rule_id in &sweep.covered {
            let Some(e) = by_rule(rule_id) else {
                report
                    .deferred
                    .push((rule_id.clone(), "entry_not_projected".into()));
                continue;
            };
            let cur = self
                .view
                .status
                .get(&e.debt_ref)
                .copied()
                .unwrap_or(e.record.status);
            if e.record.expiry_condition.kind != ExpiryKind::ModelVersionChange
                || matches!(cur, DebtStatus::Retired | DebtStatus::Expired)
            {
                continue; // not version-conditioned or already terminal.
            }
            let t = DebtTransition {
                debt_ref: e.debt_ref.clone(),
                from: cur,
                to: DebtStatus::Expiring,
                trigger: "model_version_change".to_string(),
                evidence_ref: Some(snapshot_ref.to_string()),
                causes: vec![format!("model_version_change:{snapshot_ref}")],
            };
            self.emit(store, "lifecycle.debt.status.changed", t.to_json(), vec![])?;
            report.transitions.push(t);
        }

        // (2) scheduled — the reverse sweep: scope ∌ snapshot ⇒ the
        // removal test schedules under the same bound + budget check
        // `sweep` uses.
        for rule_id in &sweep.scheduled {
            let Some(e) = by_rule(rule_id) else {
                report
                    .deferred
                    .push((rule_id.clone(), "entry_not_projected".into()));
                continue;
            };
            let cur = self
                .view
                .status
                .get(&e.debt_ref)
                .copied()
                .unwrap_or(e.record.status);
            let entry = SchedulableEntry {
                debt_ref: e.debt_ref.clone(),
                current_status: cur,
                evidence_grade: e.record.evidence_grade(),
                used: !e.used_by.is_empty(),
                probation_due: self
                    .view
                    .probation
                    .get(&e.debt_ref)
                    .map(|p| p.due_at_ms <= now_ms)
                    .unwrap_or(false),
                next_time_expiry_ms: e
                    .record
                    .expiry
                    .as_ref()
                    .and_then(|x| x.params.until)
                    .or_else(|| {
                        (e.record.expiry_condition.kind == ExpiryKind::Date)
                            .then(|| e.record.expiry_condition.value.clone())
                            .flatten()
                            .and_then(|v| v.parse().ok())
                    }),
                removal_kind: e.record.removal_test.as_ref().map(|t| t.kind),
                home: e.home,
            };
            if cur == DebtStatus::Retired
                || entry.removal_kind.is_none()
                || self.view.is_test_open(&entry.debt_ref)
            {
                continue;
            }
            let ctx = ScheduleContext {
                open_tests: self.view.open_test_count(),
                reserve,
                on_cadence: false, // import-driven — not the cadence leg.
            };
            match schedule_removal_test(&entry, &e.record, e.template.as_ref(), &policy, &ctx)? {
                ScheduleOutcome::Scheduled {
                    spec,
                    priority_class: class,
                } => {
                    let kind = entry
                        .removal_kind
                        .unwrap_or(RemovalTestKind::RetirementExperiment);
                    let test = ScheduledTest {
                        debt_ref: entry.debt_ref.clone(),
                        kind,
                        experiment_id: spec.experiment_id.clone(),
                        spec: *spec,
                        priority_class: class,
                    };
                    self.emit(
                        store,
                        "lifecycle.debt.removal_test.scheduled",
                        test.to_json(),
                        vec![],
                    )?;
                    report.scheduled.push(test);
                }
                ScheduleOutcome::Deferred { reason } => {
                    report
                        .deferred
                        .push((entry.debt_ref.clone(), reason.name()));
                }
            }
        }

        // (3) sweep.completed — attributed to the import, not cadence.
        self.emit(
            store,
            "lifecycle.debt.sweep.completed",
            Json::obj([
                ("sweep_seq", Json::Int(report.sweep_seq as i64)),
                ("kind", Json::str("post_import")),
                ("trigger_ref", Json::str(snapshot_ref)),
                ("now_ms", Json::Int(now_ms as i64)),
                ("evaluated", Json::Int(report.evaluated as i64)),
                ("transitions", Json::Int(report.transitions.len() as i64)),
                ("scheduled", Json::Int(report.scheduled.len() as i64)),
                ("deferred", Json::Int(report.deferred.len() as i64)),
            ]),
            vec![],
        )?;
        Ok(report)
    }

    /// `settle(debt_ref, kind, report_ref, verdict, reason, now_ms)` —
    /// the settled-verdict op: `hh_lab::debt::settle_removal_test` derives
    /// the verdict + record-facing transitions; the verdict mints
    /// `removal_test.settled`, the transitions mint `status.changed` (the
    /// `pass → retirement-eligible` half is a *verdict*, never a status
    /// change — D-7).
    #[allow(clippy::too_many_arguments)] // the op's record is the §5h.6 §2 settle shape — the arity is the record's.
    pub fn settle(
        &mut self,
        store: &mut Store,
        debt_ref: &str,
        kind: RemovalTestKind,
        report_ref: &str,
        verdict: Verdict,
        reason: Option<String>,
        now_ms: u64,
    ) -> Res<(RemovalVerdict, Vec<DebtTransition>)> {
        let prior = self
            .view
            .status
            .get(debt_ref)
            .copied()
            .unwrap_or(DebtStatus::Active);
        let (v, _effect, transitions) =
            settle_removal_test(debt_ref, kind, report_ref, verdict, reason, now_ms, prior);
        self.emit(
            store,
            "lifecycle.debt.removal_test.settled",
            v.to_json(),
            vec![],
        )?;
        for t in &transitions {
            self.emit(store, "lifecycle.debt.status.changed", t.to_json(), vec![])?;
        }
        Ok((v, transitions))
    }

    /// `retire(debt_ref, record)` — the human-sealed gate (AC-R-2.9.6-4):
    /// `hh_lab::debt::retire` refuses without a `pass` verdict on the debt
    /// and a human `decided_by`; the admitted outcome mints
    /// `status.changed{to: retired}` with `supersedes{reason: expiry}` —
    /// the caller's new-version seal carries the supersession member.
    pub fn retire(
        &mut self,
        store: &mut Store,
        debt_ref: &str,
        record: &hh_hir::debt::RetirementRecord,
    ) -> Res<hh_lab::debt::RetirementOutcome> {
        let status = self
            .view
            .status
            .get(debt_ref)
            .copied()
            .unwrap_or(DebtStatus::Active);
        let verdicts: Vec<RemovalVerdict> =
            self.view.settled.iter().map(|(_, v)| v.clone()).collect();
        let outcome =
            hh_lab::debt::retire(debt_ref, status, &verdicts, record).map_err(|e| match e {
                hh_lab::debt::RetireError::RetirementNotEvidenced { .. } => {
                    Refusal::RetirementNotEvidenced {
                        debt_ref: debt_ref.to_string(),
                    }
                }
                hh_lab::debt::RetireError::NotHumanSealed { .. } => Refusal::NotARetirementDiff {
                    detail: "decided_by is not human provenance".to_string(),
                },
                hh_lab::debt::RetireError::AlreadyRetired { .. } => Refusal::Unsupported {
                    detail: "the debt is already retired".to_string(),
                },
            })?;
        let mut p = outcome.transition.to_json();
        if let Json::Obj(m) = &mut p {
            m.insert(
                "supersedes_reason".into(),
                Json::str(outcome.supersedes_reason),
            );
        }
        self.emit(store, "lifecycle.debt.status.changed", p, vec![])?;
        Ok(outcome)
    }

    /// `propose(debt_ref, rule_id, diff, proposed_by)` — the
    /// evolution-origin gate: a `pass` verdict on the debt + a
    /// retirement-shaped diff admit a `RetirementProposal`
    /// (`state: proposed` — never deployed).
    pub fn propose(
        &mut self,
        debt_ref: &str,
        rule_id: &str,
        diff: &ProposalDiff,
        proposed_by: ProvenanceRecord,
    ) -> Res<RetirementProposal> {
        let verdicts: Vec<RemovalVerdict> =
            self.view.settled.iter().map(|(_, v)| v.clone()).collect();
        propose_retirement(debt_ref, rule_id, diff, &verdicts, proposed_by)
    }

    /// `index(entries)` — the fleet-level debt index projection
    /// (`hh_lab::debt::debt_index` — the materialized view the report
    /// derives from; records-in).
    pub fn index(&self, entries: &[DebtIndexEntry]) -> Vec<hh_lab::debt::DebtIndexRow> {
        hh_lab::debt::debt_index(entries)
    }

    /// `report(entries, policy, now_ms)` — the fleet-level report +
    /// routed notices (the §5h.6 §6 notifier — notices go only to sinks
    /// `DebtPolicy.notice_sinks` declares; unrouted surfaces as
    /// `unrouted`, never dropped).
    pub fn report(
        &self,
        entries: &[DebtIndexEntry],
        policy: &DebtPolicy,
        now_ms: u64,
    ) -> (
        hh_lab::debt::DebtReport,
        std::collections::BTreeMap<String, Vec<Json>>,
    ) {
        let index = hh_lab::debt::debt_index(entries);
        let health = AssumptionDebtHealth {
            warn_within_ms: policy.warn_within_ms,
        };
        let report = health.report(&index, now_ms);
        let notices = health.notices(&index, &report);
        let routed = hh_lab::debt::route_notices(&notices, policy);
        (report, routed)
    }
}
