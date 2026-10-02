//! `ManagerView` — the durable-prefix fold the `DebtManager` rebuilds from
//! the registry run's `lifecycle.debt.*` rows (CC3/CC10 — restart rebuild
//! equality is structural: the same prefix folds to the same view).
//!
//! The view carries: the registered manager service records, the ledgered
//! probation book (DF-S5.4-1 — `probation.opened` is the book of record),
//! the open/settled removal-test map, the per-debt folded status, the
//! `model_version_change` trigger ticks the reflexive window measures over,
//! and the sweep count.

use std::collections::BTreeMap;

use hh_ledger::event::EventEnvelope;
use hh_ontology::debt::{DebtStatus, RemovalTestKind, RemovalVerdict, Verdict};
use hh_wire::json::Json;

use crate::errors::DebtManagerError;

/// The `lifecycle.debt.status.changed` cause spellings that count as a
/// `model_version_change` window tick for `no_dead_weight_found{window}`
/// (the §05b ADR-0126 observable family — `hh_compiler::expiry::cause`'s
/// spellings; one closed set, CC1).
pub const MODEL_VERSION_CAUSES: &[&str] = &[
    "served_model_mismatch",
    "compatibility_token_changed",
    "fingerprint_drift",
    "no_listed_model_beyond_grace",
    "regression_drifted",
    "model_version_change",
];

/// A folded probation entry (`lifecycle.debt.probation.opened`).
#[derive(Debug, Clone, PartialEq)]
pub struct ProbationEntry {
    /// The debt under probation.
    pub debt_ref: String,
    /// When probation opened (transaction time ms).
    pub opened_at_ms: u64,
    /// When probation lapses (`due_at_ms <= now` ⇒ `probation_overrun`).
    pub due_at_ms: u64,
    /// The ledgered row's event ref (`event:<id>`) — the evidence the
    /// transition cites and the §5h.7 gate resolves.
    pub event_ref: String,
}

/// A folded scheduled removal test (open until a `settled` row names the
/// same `debt_ref`).
#[derive(Debug, Clone, PartialEq)]
pub struct OpenTest {
    /// The debt under test.
    pub debt_ref: String,
    /// The scheduled experiment.
    pub experiment_id: String,
    /// The removal-test kind.
    pub kind: String,
    /// When the test was scheduled (transaction time ms).
    pub scheduled_at_ms: u64,
}

/// `ManagerView` — the folded manager state.
#[derive(Debug, Clone, Default)]
pub struct ManagerView {
    /// Registered managers (`manager_id` → the service record body).
    pub managers: BTreeMap<String, Json>,
    /// The probation book (`debt_ref` → its ledgered entry; first write
    /// wins — a duplicate `probation.opened` is folded but never rewinds
    /// the clock).
    pub probation: BTreeMap<String, ProbationEntry>,
    /// Open removal tests (`debt_ref` → the scheduled row).
    pub open_tests: BTreeMap<String, OpenTest>,
    /// The settled removal-test verdicts, in commit order (`(seq, verdict)`
    /// — the seq is the window ordering key; the verdict's own `settled_at`
    /// ms is wall-clock data, not fold order).
    pub settled: Vec<(u64, RemovalVerdict)>,
    /// The folded current status per debt (`status.changed` fold — the
    /// `to` member of the last transition for the debt).
    pub status: BTreeMap<String, DebtStatus>,
    /// The `model_version_change` trigger ticks (event timestamps ms of
    /// `status.changed` rows whose `causes[]` intersect
    /// [`MODEL_VERSION_CAUSES`]), ascending — the reflexive window's
    /// clock.
    pub model_version_ticks: Vec<u64>,
    /// The sweep count (`lifecycle.debt.sweep.completed` rows committed).
    pub sweep_count: u64,
}

fn str_of<'a>(m: &'a std::collections::BTreeMap<String, Json>, k: &str) -> Option<&'a str> {
    m.get(k).and_then(Json::as_str)
}

fn int_of(m: &std::collections::BTreeMap<String, Json>, k: &str) -> Option<u64> {
    m.get(k).and_then(Json::as_int).map(|i| i.max(0) as u64)
}

fn ts_ms(env: &EventEnvelope) -> u64 {
    // The envelope's `ts` is an RFC3339-ms string; the fold keys on
    // commit order + the payload's own ms members (durable rows carry
    // `opened_at`/`settled_at`/`scheduled_at` themselves — `ts` is only a
    // fallback for ticks). Parse the ms digits cheaply: the envelope's
    // seq is the ordering guarantee; for the window we only need an
    // ordering hint, so use seq.
    env.seq
}

impl ManagerView {
    /// Fold one durable-prefix slice (idempotent tail-append — `fold`
    /// wholesale and `fold_tail` agree; the invariants below never let a
    /// tail row rewrite history).
    pub fn fold(events: &[EventEnvelope]) -> Result<ManagerView, DebtManagerError> {
        let mut v = ManagerView::default();
        v.fold_tail(events);
        Ok(v)
    }

    /// Fold a tail slice into the live view.
    pub fn fold_tail(&mut self, events: &[EventEnvelope]) -> &mut Self {
        for env in events {
            let Json::Obj(p) = &env.payload else { continue };
            match env.class.as_str() {
                "lifecycle.debt.service.registered" => {
                    if let Some(Json::Obj(rec)) = p.get("record") {
                        if let Some(id) = str_of(rec, "manager_id") {
                            self.managers
                                .entry(id.to_string())
                                .or_insert_with(|| Json::Obj(rec.clone()));
                        }
                    }
                }
                "lifecycle.debt.probation.opened" => {
                    if let Some(d) = str_of(p, "debt_ref") {
                        let entry = ProbationEntry {
                            debt_ref: d.to_string(),
                            opened_at_ms: int_of(p, "opened_at_ms").unwrap_or(0),
                            due_at_ms: int_of(p, "due_at_ms").unwrap_or(0),
                            event_ref: env.event_id.clone(),
                        };
                        // First write wins — a re-opened probation never
                        // rewinds the already-running clock (the append is
                        // still folded for audit).
                        self.probation.entry(d.to_string()).or_insert(entry);
                    }
                }
                "lifecycle.debt.removal_test.scheduled" => {
                    if let Some(d) = str_of(p, "debt_ref") {
                        self.open_tests.insert(
                            d.to_string(),
                            OpenTest {
                                debt_ref: d.to_string(),
                                experiment_id: str_of(p, "experiment_id").unwrap_or("").to_string(),
                                kind: str_of(p, "kind").unwrap_or("").to_string(),
                                scheduled_at_ms: int_of(p, "scheduled_at_ms").unwrap_or(ts_ms(env)),
                            },
                        );
                    }
                }
                "lifecycle.debt.removal_test.settled" => {
                    if let Ok(v) = RemovalVerdict::from_json(&env.payload, "/settled") {
                        self.open_tests.remove(&v.debt_ref);
                        self.settled.push((env.seq, v));
                    }
                }
                "lifecycle.debt.status.changed" => {
                    let to = str_of(p, "to").and_then(DebtStatus::parse);
                    if let (Some(d), Some(to)) = (str_of(p, "debt_ref"), to) {
                        self.status.insert(d.to_string(), to);
                    }
                    let tick = p
                        .get("causes")
                        .and_then(|c| match c {
                            Json::Arr(a) => Some(a),
                            _ => None,
                        })
                        .map(|a| {
                            a.iter()
                                .filter_map(Json::as_str)
                                .any(|c| MODEL_VERSION_CAUSES.contains(&c))
                        })
                        .unwrap_or(false);
                    if tick {
                        self.model_version_ticks.push(ts_ms(env));
                    }
                }
                "lifecycle.debt.sweep.completed" => {
                    self.sweep_count += 1;
                }
                _ => {}
            }
        }
        self
    }

    /// Whether `debt_ref` has an open (scheduled, unsettled) removal test.
    pub fn is_test_open(&self, debt_ref: &str) -> bool {
        self.open_tests.contains_key(debt_ref)
    }

    /// The count of open removal tests (the `max_open_removal_tests`
    /// bound's left side).
    pub fn open_test_count(&self) -> u32 {
        self.open_tests.len() as u32
    }

    /// The `LifecycleProbation` observable `evaluate_debt` reads for
    /// `debt_ref` (the book-of-record projection).
    pub fn ledgered_probation(&self, debt_ref: &str) -> Option<hh_lab::debt::LedgeredProbation> {
        self.probation
            .get(debt_ref)
            .map(|p| hh_lab::debt::LedgeredProbation {
                debt_ref: p.debt_ref.clone(),
                opened_at_ms: p.opened_at_ms,
                due_at_ms: p.due_at_ms,
                source_ref: p.event_ref.clone(),
            })
    }

    /// `no_dead_weight_found{window}` — the reflexive verdict over the
    /// folded prefix: `pass` when the window closed (≥
    /// `window.model_version_changes` model-version-change ticks since the
    /// manager registered) AND no settled removal test in the window is a
    /// `pass` — the manager found no dead weight and is itself dead weight
    /// (ADR-0197 D10); `fail` when a pass exists (the manager is doing
    /// work); `inconclusive{window_open}` while the window is open.
    pub fn no_dead_weight_verdict(
        &self,
        debt_ref: &str,
        window_model_version_changes: u64,
        settled_at_ms: u64,
        sweep_ref: &str,
    ) -> RemovalVerdict {
        let tick_edge = if self.model_version_ticks.len() as u64 >= window_model_version_changes {
            // The window edge: the timestamp of the Nth-most-recent tick.
            let idx = self.model_version_ticks.len() - window_model_version_changes as usize;
            Some(self.model_version_ticks[idx])
        } else {
            None
        };
        let Some(edge) = tick_edge else {
            return RemovalVerdict {
                debt_ref: debt_ref.to_string(),
                kind: RemovalTestKind::NoDeadWeightFound,
                verdict: Verdict::Inconclusive,
                reason: Some("window_open".to_string()),
                report_ref: sweep_ref.to_string(),
                settled_at: settled_at_ms,
            };
        };
        let passes_in_window = self
            .settled
            .iter()
            .filter(|(seq, v)| v.verdict == Verdict::Pass && *seq >= edge)
            .count();
        RemovalVerdict {
            debt_ref: debt_ref.to_string(),
            kind: RemovalTestKind::NoDeadWeightFound,
            verdict: if passes_in_window == 0 {
                Verdict::Pass
            } else {
                Verdict::Fail
            },
            reason: None,
            report_ref: sweep_ref.to_string(),
            settled_at: settled_at_ms,
        }
    }
}
