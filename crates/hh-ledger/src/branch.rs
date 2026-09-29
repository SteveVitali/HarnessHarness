//! The branch-model projection the S2.9 fork/rollback surface consumes
//! (§5a.4 `coherent_fork_points`; AC-R-2.2.4-1 — a shared S2.3/S2.9
//! prerequisite).
//!
//! A seq `s` is a **coherent fork point** iff every `model_call` scope opened
//! at seq ≤ `s` is closed at seq ≤ `s`, and every effect `intended` ≤ `s` is —
//! in the fold *at `s`* — terminal, `deferred`, or a detached child (listed in
//! a committed effect's `detached_effect_ids[]`). Turn boundaries and effect
//! terminals are always coherent. The projection is pure — same ledger, same
//! answer — and [`Store::check_fork_point`] is the refusal `fork`/`rollback`
//! raise (`ForkPointNotCoherent{at_seq, open_scopes[]}`).

use std::collections::{BTreeMap, BTreeSet};

use hh_identity::idp::ContentAddress;
use hh_wire::json::Json;

use crate::classes::{self, ScopeKind};
use crate::effect::{self, EffectFold, EffectPhase, ObservedOutcome};
use crate::errors::LedgerError;
use crate::event::EventEnvelope;
use crate::ids::ROOT_EVENT;
use crate::manifest::{EventRef, RunManifest};
use crate::saga::{compensating_effect_id, CompensationIntent};
use crate::store::{Lease, Store};

/// The coherence fold at a cut — `open model_call scopes` + `non-coherent
/// effects` (intended/authorized/prepared/committed/unknown and not
/// detached-listed).
#[derive(Debug, Clone, Default)]
struct CoherenceFold {
    /// Open `model_call` scope ids.
    open_model_calls: BTreeSet<String>,
    /// Open `turn` scopes (a turn is coherent to fork inside — it just carries
    /// context; the spec's gate names model_calls and effects — but an open
    /// turn *containing* open work is flagged through those).
    effects: BTreeMap<String, EffectFold>,
    /// Effect ids a committed effect marked detached (coherent to fork past —
    /// the detached child reconciles independently).
    detached: BTreeSet<String>,
}

impl CoherenceFold {
    fn step(&mut self, e: &EventEnvelope) {
        if let Some(spec) = classes::lookup(&e.class) {
            if spec.opens_scope == Some(ScopeKind::ModelCall) {
                if let Some(id) = e.scope.model_call_id.as_deref() {
                    self.open_model_calls.insert(id.to_string());
                }
            }
            if spec.closes_scope == Some(ScopeKind::ModelCall) {
                if let Some(id) = e.scope.model_call_id.as_deref() {
                    self.open_model_calls.remove(id);
                }
            }
        }
        // A committed effect's `detached_effect_ids[]` marks its children
        // coherent-past (the detached child runs on its own reconciliation).
        if e.class == "action.effect.committed" {
            if let Some(Json::Arr(ids)) = e.payload.get("detached_effect_ids") {
                for id in ids.iter().filter_map(Json::as_str) {
                    self.detached.insert(id.to_string());
                }
            }
        }
        effect::fold_event(&mut self.effects, e);
    }

    /// The scope spellings blocking coherence at this cut.
    fn open_scopes(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .open_model_calls
            .iter()
            .map(|m| format!("model_call:{m}"))
            .collect();
        for (id, f) in &self.effects {
            let coherent = match f.phase {
                // `deferred` is coherent (a speculative-branch hold — the
                // promotion decision is the fork's); terminals are coherent
                // (`observed{partial}` / retryable `not_applied` are not
                // terminal — they gate the cut).
                EffectPhase::Deferred => true,
                EffectPhase::Intended
                | EffectPhase::Authorized
                | EffectPhase::Prepared
                | EffectPhase::Committed
                | EffectPhase::Unknown => self.detached.contains(id),
                _ => f.is_terminal(),
            };
            if !coherent {
                out.push(format!("effect:{id}"));
            }
        }
        out
    }
}

impl Store {
    /// `coherent_fork_points(run, from_seq?, to_seq?) → [seq]` — the pure
    /// projection (AC-R-2.2.4-1). `O(n)` — the fold steps once and a snapshot of
    /// its open-set answers each cut.
    pub fn coherent_fork_points(
        &self,
        run_id: &str,
        from_seq: Option<u64>,
        to_seq: Option<u64>,
    ) -> Result<Vec<u64>, LedgerError> {
        let events = self.events(run_id)?;
        let mut fold = CoherenceFold::default();
        let mut out = Vec::new();
        // Seq 0 (`lifecycle.run.created`) is always coherent — the lineage
        // anchor a `continued_from` binds.
        let lo = from_seq.unwrap_or(0);
        let hi = to_seq.unwrap_or(u64::MAX);
        if lo == 0 {
            out.push(0);
        }
        for e in events {
            fold.step(e);
            if e.seq < lo || e.seq > hi {
                continue;
            }
            if fold.open_scopes().is_empty() {
                out.push(e.seq);
            }
        }
        Ok(out)
    }

    /// `check_fork_point(run, seq)` — the refusal `fork`/`rollback` raise:
    /// `ForkPointNotCoherent{at_seq, open_scopes[]}` names every scope
    /// blocking the cut.
    pub fn check_fork_point(&self, run_id: &str, at_seq: u64) -> Result<(), LedgerError> {
        let events = self.events(run_id)?;
        if at_seq > events.len().saturating_sub(1) as u64 {
            return Err(LedgerError::SourceIncomplete {
                run_id: run_id.to_string(),
                at_seq,
                head: events.len().saturating_sub(1) as u64,
            });
        }
        let mut fold = CoherenceFold::default();
        for e in events {
            if e.seq > at_seq {
                break;
            }
            fold.step(e);
        }
        let open = fold.open_scopes();
        if open.is_empty() {
            Ok(())
        } else {
            Err(LedgerError::ForkPointNotCoherent {
                run_id: run_id.to_string(),
                at_seq,
                open_scopes: open,
            })
        }
    }

    /// `nearest_coherent_seq(run, seq)` — the suggestion a refusal carries
    /// (the greatest coherent seq ≤ `at_seq`, or `None` when only 0 is).
    pub fn nearest_coherent_seq(
        &self,
        run_id: &str,
        at_seq: u64,
    ) -> Result<Option<u64>, LedgerError> {
        Ok(self
            .coherent_fork_points(run_id, None, Some(at_seq))?
            .into_iter()
            .filter(|s| *s <= at_seq)
            .max())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// §5a.1 §5 — the branch model (R-2.2.4; ADR-0271)
//
// `fork` is **inter-run** at Stage 2: a child run opens under
// `forked_from{run_id, at_seq, head_hash}`; the child WAL's seq 1 is the
// audit-grade `lifecycle.run.forked{BranchRecord}` whose envelope `refs` pin the
// source prefix's referenced content (the pin a gc/redact on the source honours
// — `pin_reason` scans every other run's refs). `navigate`/`rollback` move the
// **logical** head by appending `lifecycle.head.moved` — the WAL stays linear
// (`seq`/`prev_hash` never rewrite; the branch tree lives in `parent_event_id`)
// and live subscribers receive `EventFrame::Rewind{to_seq}`.
// ─────────────────────────────────────────────────────────────────────────────

/// `BranchKind` — §5a.1's closed sum (`branch` at Stage 2; `counterfactual`
/// mints with `replay_mode: none`-equivalent honesty; `shadow` for observe-only
/// children).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchKind {
    /// A forked continuation — the common case.
    Branch,
    /// A counterfactual branch (what-if; the replay machinery is Stage 3 — the
    /// record is honest about the mode it actually carries).
    Counterfactual,
    /// An observe-only shadow branch.
    Shadow,
}

impl BranchKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            BranchKind::Branch => "branch",
            BranchKind::Counterfactual => "counterfactual",
            BranchKind::Shadow => "shadow",
        }
    }
    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<BranchKind> {
        Some(match s {
            "branch" => BranchKind::Branch,
            "counterfactual" => BranchKind::Counterfactual,
            "shadow" => BranchKind::Shadow,
            _ => return None,
        })
    }
}

/// `opts.env` — the fork's environment binding (§5a.1 §5). `SharedLive` is a
/// refusal-shaped honest record — a live mutable environment is never shared
/// silently; a caller may bind it only through an explicit override that the
/// record names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnvBinding {
    /// The child restores from an `fs_tree` snapshot taken at/below the cut.
    Snapshot,
    /// No environment copy — the branch is observation-only; read-only.
    TraceOnly,
    /// The caller asserts sharing the live env is safe (single-writer
    /// discipline stays the caller's — the record names the choice).
    SharedLive,
    /// No environment bound — the child provisions its own later (the
    /// pre-S2.9 fork posture; `env` absent on the op).
    #[default]
    None,
}

impl EnvBinding {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EnvBinding::Snapshot => "snapshot",
            EnvBinding::TraceOnly => "trace_only",
            EnvBinding::SharedLive => "shared_live",
            EnvBinding::None => "none",
        }
    }
    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<EnvBinding> {
        Some(match s {
            "snapshot" => EnvBinding::Snapshot,
            "trace_only" => EnvBinding::TraceOnly,
            "shared_live" => EnvBinding::SharedLive,
            "none" => EnvBinding::None,
            _ => return None,
        })
    }
}

/// `replay_mode` — the branch's replay contract (§5a.1 §5). `Inherited` takes
/// the run's declared observability; `Exact`/`Structural`/`Observational` are
/// the coverage-graded modes — a downgrade is recorded, never silent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReplayMode {
    /// Whatever the source run's observability implies (default).
    #[default]
    Inherited,
    /// Bit-exact replay — requires the coverage the Stage-3 replay driver
    /// reads (S2.9 records the request; enforcement is S3.6's).
    Exact,
    /// Structure-exact (same decisions; outputs may differ).
    Structural,
    /// Observation-grade (loosest honest mode).
    Observational,
    /// No replay claim — `trace_only` branches carry this.
    None,
}

impl ReplayMode {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ReplayMode::Inherited => "inherited",
            ReplayMode::Exact => "exact",
            ReplayMode::Structural => "structural",
            ReplayMode::Observational => "observational",
            ReplayMode::None => "none",
        }
    }
    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<ReplayMode> {
        Some(match s {
            "inherited" => ReplayMode::Inherited,
            "exact" => ReplayMode::Exact,
            "structural" => ReplayMode::Structural,
            "observational" => ReplayMode::Observational,
            "none" => ReplayMode::None,
            _ => return None,
        })
    }
}

/// `ForkOpts` — the fork parameters the ledger records verbatim (the caller —
/// `hh-embed`'s `fork` op — computes `effective_replay`/`downgrade_reason` from
/// the source's observability; the ledger stores what it is given, never
/// invents a downgrade explanation).
#[derive(Debug, Clone, Default)]
pub struct ForkOpts {
    /// The environment binding.
    pub env: EnvBinding,
    /// The requested replay mode.
    pub replay_mode: ReplayMode,
    /// The mode actually carried (== `replay_mode` unless downgraded).
    pub effective_replay: ReplayMode,
    /// Why the effective mode is weaker (the recorded `ReplayDowngrade`).
    pub downgrade_reason: Option<String>,
    /// The policy ref the branch runs under (attenuated — never widened).
    pub policy_ref: Option<String>,
    /// The budget-slice ref (reserve-before-spend, bounded by the parent).
    pub budget_slice_ref: Option<String>,
    /// `coerce_to_boundary` — an incoherent `at` coerces to the nearest
    /// earlier coherent seq when true; otherwise the fork refuses.
    pub coerce_to_boundary: bool,
    /// The `fs_tree` snapshot's blob id (the child restored from it).
    pub snapshot_ref: Option<String>,
    /// The snapshot's recorded `at_seq` (≤ `at` — drift is honest, named).
    pub snapshot_at_seq: Option<u64>,
    /// `read_only` — `trace_only` forces it; the record carries it.
    pub read_only: bool,
}

/// `BranchRecord` — the fork record (§5a.1 §5's `BranchRef`): folded from the
/// child's `lifecycle.run.forked` row — a rebuildable projection, never a
/// stored fact.
#[derive(Debug, Clone)]
pub struct BranchRecord {
    /// The branch id (`branch_*` — `Scope::branch_id`'s referent).
    pub branch_id: String,
    /// The child run.
    pub run_id: String,
    /// The source run.
    pub source_run_id: String,
    /// The cut on the source.
    pub at_seq: u64,
    /// The source event at the cut (the `causes` anchor).
    pub at_event_id: String,
    /// The branch kind.
    pub kind: BranchKind,
    /// The env binding.
    pub env: EnvBinding,
    /// The requested replay mode.
    pub replay_mode: ReplayMode,
    /// The effective replay mode.
    pub effective_replay: ReplayMode,
    /// The recorded downgrade reason.
    pub downgrade_reason: Option<String>,
    /// The branch policy ref.
    pub policy_ref: Option<String>,
    /// The budget-slice ref.
    pub budget_slice_ref: Option<String>,
    /// Read-only (`trace_only`).
    pub read_only: bool,
    /// Whether `at` was coerced to a coherent boundary.
    pub coerce_to_boundary: bool,
    /// The env snapshot blob id.
    pub snapshot_ref: Option<String>,
    /// The snapshot's `at_seq`.
    pub snapshot_at_seq: Option<u64>,
    /// The child's `created` event id.
    pub created_event_id: String,
    /// The `forked` row's event id.
    pub forked_event_id: String,
    /// The child's head at record time.
    pub head_seq: u64,
    /// The child's head event id at record time.
    pub head_event_id: String,
    /// The child's head hash at record time.
    pub head_hash: String,
}

impl BranchRecord {
    /// The `lifecycle.run.forked` payload (the audit partition's members).
    fn payload(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("branch_id".into(), Json::str(&self.branch_id));
        m.insert("source_run_id".into(), Json::str(&self.source_run_id));
        m.insert("at_seq".into(), Json::Int(self.at_seq as i64));
        m.insert("at_event_id".into(), Json::str(&self.at_event_id));
        m.insert("kind".into(), Json::str(self.kind.as_str()));
        m.insert("env".into(), Json::str(self.env.as_str()));
        m.insert("replay_mode".into(), Json::str(self.replay_mode.as_str()));
        m.insert(
            "effective_replay".into(),
            Json::str(self.effective_replay.as_str()),
        );
        if let Some(d) = &self.downgrade_reason {
            m.insert("downgrade_reason".into(), Json::str(d));
        }
        if let Some(p) = &self.policy_ref {
            m.insert("policy_ref".into(), Json::str(p));
        }
        if let Some(b) = &self.budget_slice_ref {
            m.insert("budget_slice_ref".into(), Json::str(b));
        }
        m.insert("read_only".into(), Json::Bool(self.read_only));
        m.insert(
            "coerce_to_boundary".into(),
            Json::Bool(self.coerce_to_boundary),
        );
        if let Some(s) = &self.snapshot_ref {
            m.insert("snapshot_ref".into(), Json::str(s));
        }
        m.insert("created_event_id".into(), Json::str(&self.created_event_id));
        m.insert("head_event_id".into(), Json::str(&self.head_event_id));
        m.insert("head_seq".into(), Json::Int(self.head_seq as i64));
        Json::Obj(m)
    }

    /// The full record's member form — the `branch_tree` view + the `fork`
    /// response carry it (superset of the audit row: `forked_event_id`,
    /// `head_hash`, `snapshot_at_seq`).
    pub fn to_json(&self) -> Json {
        let mut m = match self.payload() {
            Json::Obj(m) => m,
            _ => BTreeMap::new(),
        };
        m.insert("run_id".into(), Json::str(&self.run_id));
        m.insert("forked_event_id".into(), Json::str(&self.forked_event_id));
        m.insert("head_hash".into(), Json::str(&self.head_hash));
        if let Some(s) = self.snapshot_at_seq {
            m.insert("snapshot_at_seq".into(), Json::Int(s as i64));
        }
        Json::Obj(m)
    }

    /// Fold a record back out of a `lifecycle.run.forked` row's payload.
    fn from_payload(run_id: &str, p: &Json) -> Option<BranchRecord> {
        let get = |k: &str| p.get(k).and_then(Json::as_str).map(str::to_string);
        Some(BranchRecord {
            branch_id: get("branch_id")?,
            run_id: run_id.to_string(),
            source_run_id: get("source_run_id")?,
            at_seq: p.get("at_seq")?.as_int()? as u64,
            at_event_id: get("at_event_id")?,
            kind: BranchKind::parse(&get("kind")?)?,
            env: EnvBinding::parse(&get("env")?)?,
            replay_mode: ReplayMode::parse(&get("replay_mode")?)?,
            effective_replay: ReplayMode::parse(&get("effective_replay")?)?,
            downgrade_reason: get("downgrade_reason"),
            policy_ref: get("policy_ref"),
            budget_slice_ref: get("budget_slice_ref"),
            read_only: matches!(p.get("read_only"), Some(Json::Bool(true))),
            coerce_to_boundary: matches!(p.get("coerce_to_boundary"), Some(Json::Bool(true))),
            snapshot_ref: get("snapshot_ref"),
            snapshot_at_seq: None,
            created_event_id: get("created_event_id")?,
            forked_event_id: String::new(),
            head_seq: p.get("head_seq")?.as_int()? as u64,
            head_event_id: get("head_event_id")?,
            head_hash: String::new(),
        })
    }
}

/// `NavigateTarget` — `navigate(to:)`'s resolvable forms (§5a.1 `navigate`;
/// `to: null` is the run-start sentinel — the next append chains from genesis).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigateTarget {
    /// An event seq on this run.
    Seq(u64),
    /// An event id on this run.
    EventId(String),
    /// The root sentinel (`to: null`).
    Root,
}

/// One compensation-list entry — `{effect_id, compensating_effect_id,
/// disposition, escalation_ref?}` (§5a.1 `rollback`'s record).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompensationEntry {
    /// The compensated (or attempted) effect.
    pub effect_id: String,
    /// Its derived compensating effect id (`<id>~comp`).
    pub compensating_effect_id: String,
    /// `compensated` | `abandoned` | `in_flight`.
    pub disposition: String,
    /// The escalation row when the compensator failed.
    pub escalation_ref: Option<String>,
}

/// `RollbackRecord` — the rewind note (§5a.1 `rollback`'s record; the
/// `rollback.record`/`intervention.note` Stage-3 consumers read). The full
/// lists live in the `record_ref` blob; the row carries the small members.
#[derive(Debug, Clone)]
pub struct RollbackRecord {
    /// The run.
    pub run_id: String,
    /// The rewind target.
    pub to_seq: u64,
    /// The target's event id.
    pub to_event_id: String,
    /// `rollback` | `policy_violation` | `operator` | `recovery`.
    pub reason_code: String,
    /// Whether the head moved (the `head.moved` row landed).
    pub rewound: bool,
    /// Every applied effect after `to_seq` — `{effect_id, compensating_effect_id,
    /// disposition}` in reverse commit order.
    pub compensation_list: Vec<CompensationEntry>,
    /// Applied-after-`to_seq` effects whose class is not compensable —
    /// `uncompensable`, named, never silently kept.
    pub uncompensable: Vec<String>,
    /// Side-effect domains the env restore could not recapture (hh-env fills).
    pub uncaptured: Vec<String>,
    /// Compensators that failed (a `compensation_list` subset + escalations).
    pub failed_compensations: Vec<CompensationEntry>,
    /// The rewind-note blob (`record_ref` — an `idp/1` blob id).
    pub record_ref: String,
    /// The `rolled_back` row's event id.
    pub rollback_event_id: String,
    /// The post-rollback head (== `to`).
    pub head_seq: u64,
    /// The post-rollback head event id.
    pub head_event_id: String,
}

impl RollbackRecord {
    /// The rewind-note blob's member form (canonical JSON — `record_ref`
    /// addresses it; the unbounded lists live here, not in the audit row).
    pub fn note_json(&self) -> Json {
        let entries = |v: &[CompensationEntry]| {
            Json::Arr(
                v.iter()
                    .map(|e| {
                        let mut m = BTreeMap::new();
                        m.insert("effect_id".into(), Json::str(&e.effect_id));
                        m.insert(
                            "compensating_effect_id".into(),
                            Json::str(&e.compensating_effect_id),
                        );
                        m.insert("disposition".into(), Json::str(&e.disposition));
                        if let Some(r) = &e.escalation_ref {
                            m.insert("escalation_ref".into(), Json::str(r));
                        }
                        Json::Obj(m)
                    })
                    .collect(),
            )
        };
        Json::obj([
            ("kind", Json::str("rollback.record")),
            ("run_id", Json::str(&self.run_id)),
            ("to_seq", Json::Int(self.to_seq as i64)),
            ("to_event_id", Json::str(&self.to_event_id)),
            ("reason_code", Json::str(&self.reason_code)),
            ("compensation_list", entries(&self.compensation_list)),
            ("failed_compensations", entries(&self.failed_compensations)),
            (
                "uncompensable",
                Json::Arr(self.uncompensable.iter().map(Json::str).collect()),
            ),
            (
                "uncaptured",
                Json::Arr(self.uncaptured.iter().map(Json::str).collect()),
            ),
            ("rollback_event_id", Json::str(&self.rollback_event_id)),
            ("head_seq", Json::Int(self.head_seq as i64)),
            ("head_event_id", Json::str(&self.head_event_id)),
        ])
    }

    /// Fold a record back out of a `rolled_back` row + its resolved note blob
    /// (`None` ⇒ the blob is unavailable — `Missing`/tombstone explains).
    pub fn from_parts(run_id: &str, row: &Json, note: Option<&Json>) -> Option<RollbackRecord> {
        let get = |k: &str| row.get(k).and_then(Json::as_str).map(str::to_string);
        let entries = |k: &str| -> Vec<CompensationEntry> {
            match note.and_then(|n| n.get(k)) {
                Some(Json::Arr(items)) => items
                    .iter()
                    .filter_map(|i| {
                        Some(CompensationEntry {
                            effect_id: i.get("effect_id")?.as_str()?.to_string(),
                            compensating_effect_id: i
                                .get("compensating_effect_id")?
                                .as_str()?
                                .to_string(),
                            disposition: i.get("disposition")?.as_str()?.to_string(),
                            escalation_ref: i
                                .get("escalation_ref")
                                .and_then(Json::as_str)
                                .map(str::to_string),
                        })
                    })
                    .collect(),
                _ => Vec::new(),
            }
        };
        let strs = |k: &str| -> Vec<String> {
            match note.and_then(|n| n.get(k)) {
                Some(Json::Arr(items)) => items
                    .iter()
                    .filter_map(|i| i.as_str().map(str::to_string))
                    .collect(),
                _ => Vec::new(),
            }
        };
        Some(RollbackRecord {
            run_id: run_id.to_string(),
            to_seq: row.get("to_seq")?.as_int()? as u64,
            to_event_id: get("to_event_id")?,
            reason_code: get("reason_code")?,
            rewound: matches!(row.get("rewound"), Some(Json::Bool(true))),
            compensation_list: entries("compensation_list"),
            uncompensable: strs("uncompensable"),
            uncaptured: strs("uncaptured"),
            failed_compensations: entries("failed_compensations"),
            record_ref: get("record_ref")?,
            rollback_event_id: get("rollback_event_id")?,
            head_seq: row.get("head_seq")?.as_int()? as u64,
            head_event_id: get("head_event_id")?,
        })
    }
}

/// An `action.environment.snapshot` fold row — `(at_seq, snapshot_ref, kind,
/// manifest_ref)` (S2.9; the fork/rollback chooser's input).
pub type EnvSnapshotRow = (u64, String, String, Option<String>);

/// One `branch_tree` node — a forked branch (the child's `run.forked` row) or a
/// plain run (no fork row — the tree's roots).
#[derive(Debug, Clone)]
pub struct BranchInfo {
    /// The run.
    pub run_id: String,
    /// The branch record when this run is a forked child.
    pub record: Option<BranchRecord>,
    /// The run's logical head seq.
    pub head_seq: Option<u64>,
    /// The `rolled_back` rows this run carries (its rewind history).
    pub rolled_back: Vec<Json>,
    /// Child run ids forked from this run.
    pub children: Vec<String>,
}

impl Store {
    /// `fork(source_run, at_seq, kind, opts, child_manifest, holder)`
    /// → `(child_run_id, child_lease, BranchRecord)` (§5a.1 §5; R-2.2.4).
    ///
    /// The cut must be coherent — `ForkPointNotCoherent` unless
    /// `opts.coerce_to_boundary` coerces to `nearest_coherent_seq(at)` (the
    /// coercion is recorded). The child opens under `open_run` (its
    /// `created` carries the `forked_from` lineage link), then this mints the
    /// audit-grade `lifecycle.run.forked{BranchRecord}` — whose envelope `refs`
    /// pin every content address the source prefix `0..=at` references plus the
    /// env snapshot blob (the source-prefix pin; `pin_reason` honours another
    /// run's `refs`). The source prefix is immutable by construction —
    /// append-only means nothing after `at` is ever written into the source.
    #[allow(clippy::too_many_arguments)]
    pub fn fork(
        &mut self,
        source_run_id: &str,
        at_seq: u64,
        kind: BranchKind,
        opts: &ForkOpts,
        child_manifest: RunManifest,
        holder: &str,
    ) -> Result<(String, Lease, BranchRecord), LedgerError> {
        // The coherence gate — refusal, or coercion to the boundary (recorded).
        let at = match self.check_fork_point(source_run_id, at_seq) {
            Ok(()) => at_seq,
            Err(LedgerError::ForkPointNotCoherent { .. }) if opts.coerce_to_boundary => self
                .nearest_coherent_seq(source_run_id, at_seq)?
                .ok_or_else(|| LedgerError::SourceIncomplete {
                    run_id: source_run_id.to_string(),
                    at_seq,
                    head: 0,
                })?,
            Err(e) => return Err(e),
        };
        let tip = self.run(source_run_id)?.events.len().saturating_sub(1) as u64;
        let at_event = self
            .run(source_run_id)?
            .events
            .get(at as usize)
            .ok_or_else(|| LedgerError::SourceIncomplete {
                run_id: source_run_id.to_string(),
                at_seq: at,
                head: tip,
            })?
            .clone();
        // The pin set — every content address the source prefix references
        // (envelope `refs` + declared audit/content members), plus the env
        // snapshot blob. Pinned against any *other* run's gc/redact.
        let mut pin_ids: BTreeSet<String> = BTreeSet::new();
        {
            let src = self.run(source_run_id)?;
            for e in src.events.iter().take_while(|e| e.seq <= at) {
                for r in &e.refs {
                    pin_ids.insert(r.id());
                }
                if let (Json::Obj(m), Some(spec)) = (&e.payload, classes::lookup(&e.class)) {
                    for (name, v) in m {
                        let declared = spec
                            .audit_fields
                            .iter()
                            .any(|f| f.name == "*" || f.name == name.as_str())
                            || spec.content_refs.contains(&name.as_str());
                        if !declared {
                            continue;
                        }
                        match v {
                            Json::Str(s) if crate::ids::is_pinned_id(s) => {
                                pin_ids.insert(s.clone());
                            }
                            Json::Arr(items) => {
                                for i in items.iter().filter_map(Json::as_str) {
                                    if crate::ids::is_pinned_id(i) {
                                        pin_ids.insert(i.to_string());
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        if let Some(s) = &opts.snapshot_ref {
            pin_ids.insert(s.clone());
        }
        let pin_refs: Vec<ContentAddress> = pin_ids
            .iter()
            .filter_map(|id| {
                let parsed = hh_identity::idp::parse_id(id).ok()?;
                Some(ContentAddress {
                    idp: "idp/1",
                    algorithm: "sha256",
                    digest: parsed.digest_hex,
                    media_type: String::new(),
                    size: 0,
                })
            })
            .collect();
        // The lineage anchor — binds the source's hash at the (possibly
        // coerced) cut; `open_run` re-checks it (ADR-0027 §6).
        let mut child_manifest = child_manifest;
        child_manifest.forked_from = Some(crate::manifest::LineageLink {
            run_id: source_run_id.to_string(),
            at_seq: at,
            head_hash: at_event.hash.clone(),
        });
        // Open the child — `open_run` checks the `forked_from` lineage link
        // (the anchor binds the source's hash at `at`).
        let (child_id, lease) = self.open_run(child_manifest, holder)?;
        let created_event_id = self.run(&child_id)?.events[0].event_id.clone();
        let head = self.head(&child_id)?;
        let record = BranchRecord {
            branch_id: self.alloc_id("branch"),
            run_id: child_id.clone(),
            source_run_id: source_run_id.to_string(),
            at_seq: at,
            at_event_id: at_event.event_id.clone(),
            kind,
            env: opts.env,
            replay_mode: opts.replay_mode,
            effective_replay: opts.effective_replay,
            downgrade_reason: opts.downgrade_reason.clone(),
            policy_ref: opts.policy_ref.clone(),
            budget_slice_ref: opts.budget_slice_ref.clone(),
            read_only: opts.read_only || opts.env == EnvBinding::TraceOnly,
            coerce_to_boundary: opts.coerce_to_boundary && at != at_seq,
            snapshot_ref: opts.snapshot_ref.clone(),
            snapshot_at_seq: opts.snapshot_at_seq,
            created_event_id,
            forked_event_id: String::new(),
            head_seq: head.seq,
            head_event_id: head.event_id.clone(),
            head_hash: head.hash.clone(),
        };
        let env = self.commit_kernel_row(
            &child_id,
            "lifecycle.run.forked",
            record.payload(),
            pin_refs,
            vec![EventRef {
                run_id: source_run_id.to_string(),
                event_id: at_event.event_id.clone(),
            }],
        )?;
        let mut record = record;
        record.forked_event_id = env.event_id.clone();
        Ok((child_id, lease, record))
    }

    /// `navigate(run, lease, to) → the head.moved envelope` (§5a.1). A
    /// navigation is a HEAD move — the row lands, the logical head folds to
    /// `to`, live subscribers receive `Rewind{to_seq}`. History is untouched
    /// (the WAL is linear; nothing is ever truncated). `to: null` (`Root`)
    /// moves the head to the run-start sentinel.
    pub fn navigate(
        &mut self,
        run_id: &str,
        lease: &Lease,
        to: NavigateTarget,
        reason: &str,
    ) -> Result<EventEnvelope, LedgerError> {
        let _rec = self.active_lease(run_id, lease)?;
        let (to_seq, to_event_id) = {
            let state = self.run(run_id)?;
            match &to {
                NavigateTarget::Root => (-1i64, ROOT_EVENT.to_string()),
                NavigateTarget::Seq(s) => {
                    let e = state.events.get(*s as usize).ok_or_else(|| {
                        LedgerError::UnknownCursor {
                            detail: format!("navigate to seq {s} — beyond tip"),
                        }
                    })?;
                    (e.seq as i64, e.event_id.clone())
                }
                NavigateTarget::EventId(id) => {
                    let seq = state.by_event_id.get(id).copied().ok_or_else(|| {
                        LedgerError::UnknownCursor {
                            detail: format!("navigate to event_id {id} — unknown"),
                        }
                    })?;
                    (seq as i64, id.clone())
                }
            }
        };
        let from = self.run(run_id)?.head.clone();
        let payload = Json::obj([
            (
                "from_event_id",
                Json::str(
                    from.as_ref()
                        .map(|h| h.event_id.clone())
                        .unwrap_or_else(|| ROOT_EVENT.to_string()),
                ),
            ),
            (
                "from_seq",
                Json::Int(from.as_ref().map(|h| h.seq as i64).unwrap_or(-1)),
            ),
            ("to_event_id", Json::str(&to_event_id)),
            ("to_seq", Json::Int(to_seq)),
            ("reason", Json::str(reason)),
        ]);
        let env = self.commit_kernel_row(
            run_id,
            "lifecycle.head.moved",
            payload,
            Vec::new(),
            Vec::new(),
        )?;
        self.notify_rewind(run_id, to_seq.max(0) as u64, &to_event_id, reason);
        Ok(env)
    }

    /// `rollback(run, lease, at, reason_code, uncaptured, dispatch)`
    /// → `RollbackRecord` (§5a.1; R-2.2.5). Order of operations (each step a
    /// durable batch — a crash anywhere leaves an honest partial record):
    ///
    /// 1. `check_fork_point(at)` — the cut must be coherent.
    /// 2. `compensate_run_after(at)` — reverse-order saga over the applied
    ///    compensable effects committed *after* `at` (the compensation list's
    ///    substance). Applied-but-irreversible effects land `uncompensable`.
    /// 3. The rewind note (`rollback.record`) is `put_blob`'d — the row's
    ///    `record_ref` content ref pins its address into the audit fact.
    /// 4. `lifecycle.run.rolled_back` — the durable record.
    /// 5. `lifecycle.head.moved{to}` + the `Rewind` frame — the head moves.
    ///
    /// Nothing is ever deleted — the compensated/original rows, the failed
    /// compensators' escalations, and the rolled_back+head.moved pair are all
    /// appended; `read` still serves the whole prefix.
    pub fn rollback(
        &mut self,
        run_id: &str,
        lease: &Lease,
        at_seq: u64,
        reason_code: &str,
        uncaptured: Vec<String>,
        dispatch: &mut dyn FnMut(&CompensationIntent) -> Result<Json, String>,
    ) -> Result<RollbackRecord, LedgerError> {
        let _rec = self.active_lease(run_id, lease)?;
        self.check_fork_point(run_id, at_seq)?;
        let to_event = self.run(run_id)?.events[at_seq as usize].clone();
        // The applied-after-`at` set, split compensable/uncompensable.
        let mut compensable: Vec<(u64, String)> = Vec::new();
        let mut uncompensable: Vec<String> = Vec::new();
        for f in self.run(run_id)?.effects.values() {
            let commit_seq = f.commits.values().map(|(_, s)| *s).max().unwrap_or(0);
            if commit_seq <= at_seq {
                continue;
            }
            let applied = f.phase == EffectPhase::Compensated
                || (f.phase == EffectPhase::Observed
                    && f.outcome == Some(ObservedOutcome::Applied));
            if !applied {
                continue;
            }
            if f.risk_class.reversibility == hh_ontology::risk::RiskReversibility::Compensable {
                compensable.push((commit_seq, f.effect_id.clone()));
            } else {
                uncompensable.push(f.effect_id.clone());
            }
        }
        compensable.sort_by(|a, b| b.0.cmp(&a.0));
        uncompensable.sort();
        // The saga — reverse commit order over the compensable set.
        let report = self.compensate_run_after(run_id, lease, at_seq, dispatch)?;
        // The compensation list — per applied-after-`at` compensable effect.
        let state = self.run(run_id)?;
        let mut compensation_list: Vec<CompensationEntry> = Vec::new();
        for (_, effect_id) in &compensable {
            let comp_id = compensating_effect_id(effect_id);
            let disposition = if report.abandoned.contains(effect_id) {
                "abandoned"
            } else if report.in_flight.contains(effect_id) {
                "in_flight"
            } else if report.compensated.contains(effect_id)
                || state
                    .effects
                    .get(effect_id)
                    .map(|f| f.phase == EffectPhase::Compensated)
                    .unwrap_or(false)
            {
                "compensated"
            } else {
                "in_flight"
            };
            compensation_list.push(CompensationEntry {
                effect_id: effect_id.clone(),
                compensating_effect_id: comp_id,
                disposition: disposition.to_string(),
                escalation_ref: None,
            });
        }
        // Failed compensators name their escalation row.
        for e in compensation_list.iter_mut() {
            if e.disposition == "abandoned" {
                e.escalation_ref = state
                    .events
                    .iter()
                    .rev()
                    .find(|ev| {
                        ev.class == "lifecycle.escalation.raised"
                            && ev.payload.get("subject").and_then(Json::as_str)
                                == Some(e.compensating_effect_id.as_str())
                    })
                    .map(|ev| ev.event_id.clone());
            }
        }
        let failed_compensations: Vec<CompensationEntry> = compensation_list
            .iter()
            .filter(|e| e.disposition != "compensated")
            .cloned()
            .collect();
        // The rewind note blob first — the row's `record_ref` names it.
        let rollback_event_id = self.alloc_id("evt");
        let mut rec = RollbackRecord {
            run_id: run_id.to_string(),
            to_seq: at_seq,
            to_event_id: to_event.event_id.clone(),
            reason_code: reason_code.to_string(),
            rewound: true,
            compensation_list,
            uncompensable,
            uncaptured,
            failed_compensations,
            record_ref: String::new(),
            rollback_event_id: rollback_event_id.clone(),
            head_seq: at_seq,
            head_event_id: to_event.event_id.clone(),
        };
        let note_addr = self.put_blob(
            rec.note_json().to_canonical_string().as_bytes(),
            "application/json",
        )?;
        rec.record_ref = note_addr.id();
        // The durable `rolled_back` row (small members inline; the lists are in
        // the note blob).
        let rolled = self.commit_kernel_row(
            run_id,
            "lifecycle.run.rolled_back",
            Json::obj([
                ("to_seq", Json::Int(at_seq as i64)),
                ("to_event_id", Json::str(&to_event.event_id)),
                ("reason_code", Json::str(reason_code)),
                ("rewound", Json::Bool(true)),
                ("rollback_event_id", Json::str(&rollback_event_id)),
                ("head_seq", Json::Int(at_seq as i64)),
                ("head_event_id", Json::str(&to_event.event_id)),
                ("record_ref", Json::str(&rec.record_ref)),
            ]),
            vec![note_addr],
            vec![EventRef {
                run_id: run_id.to_string(),
                event_id: to_event.event_id.clone(),
            }],
        )?;
        // `head.moved` — the actual rewind; subscribers get the Rewind frame.
        let moved = self.commit_kernel_row(
            run_id,
            "lifecycle.head.moved",
            Json::obj([
                ("from_event_id", Json::str(&rolled.event_id)),
                ("from_seq", Json::Int(rolled.seq as i64)),
                ("to_event_id", Json::str(&to_event.event_id)),
                ("to_seq", Json::Int(at_seq as i64)),
                ("reason", Json::str(format!("rollback:{reason_code}"))),
            ]),
            Vec::new(),
            vec![EventRef {
                run_id: run_id.to_string(),
                event_id: rolled.event_id.clone(),
            }],
        )?;
        let _ = moved;
        self.notify_rewind(
            run_id,
            at_seq,
            &to_event.event_id,
            &format!("rollback:{reason_code}"),
        );
        Ok(rec)
    }

    /// The recorded env snapshots on a run — `(at_seq, snapshot_ref, kind,
    /// manifest_ref)` folded from `action.environment.snapshot` rows. The
    /// `at_seq`/`manifest_ref` members were added at S2.9; older rows report
    /// their emission seq as `at_seq` (the honest bound — a snapshot taken in
    /// that row's append describes state no later than it).
    pub fn env_snapshots(&self, run_id: &str) -> Result<Vec<EnvSnapshotRow>, LedgerError> {
        let state = self.run(run_id)?;
        let mut out = Vec::new();
        for e in &state.events {
            if e.class != "action.environment.snapshot" {
                continue;
            }
            let p = &e.payload;
            let kind = p
                .get("kind")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            let sr = match p.get("snapshot_ref").and_then(Json::as_str) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let at_seq = p
                .get("at_seq")
                .and_then(Json::as_int)
                .map(|s| s as u64)
                .unwrap_or(e.seq);
            let manifest_ref = p
                .get("manifest_ref")
                .and_then(Json::as_str)
                .map(str::to_string);
            out.push((at_seq, sr, kind, manifest_ref));
        }
        Ok(out)
    }

    /// `branch_tree()` — the derived branch index (§5a.1 §5's
    /// `branch_tree`-class index; AC-R-2.2.4-6/7). A **pure fold** over every
    /// run's WAL — `lifecycle.run.forked` rows yield `BranchRecord`s,
    /// `lifecycle.run.rolled_back` rows yield the rewind history, children
    /// attach to their source. Rebuildable by construction: delete it, refold,
    /// the same tree returns (nothing here is stored state).
    pub fn branch_tree(&self) -> Vec<BranchInfo> {
        let mut infos: BTreeMap<String, BranchInfo> = BTreeMap::new();
        let mut edges: Vec<(String, String)> = Vec::new();
        for (run_id, state) in &self.runs {
            let mut info = BranchInfo {
                run_id: run_id.clone(),
                record: None,
                head_seq: state.head.as_ref().map(|h| h.seq),
                rolled_back: Vec::new(),
                children: Vec::new(),
            };
            for e in &state.events {
                match e.class.as_str() {
                    "lifecycle.run.forked" => {
                        if let Some(mut r) = BranchRecord::from_payload(run_id, &e.payload) {
                            r.forked_event_id = e.event_id.clone();
                            r.snapshot_at_seq = e
                                .payload
                                .get("snapshot_at_seq")
                                .and_then(Json::as_int)
                                .map(|s| s as u64);
                            edges.push((r.source_run_id.clone(), run_id.clone()));
                            info.record = Some(r);
                        }
                    }
                    "lifecycle.run.rolled_back" => {
                        info.rolled_back.push(e.payload.clone());
                    }
                    _ => {}
                }
            }
            infos.insert(run_id.clone(), info);
        }
        for (src, child) in edges {
            if let Some(i) = infos.get_mut(&src) {
                i.children.push(child);
            }
        }
        infos.into_values().collect()
    }
}
