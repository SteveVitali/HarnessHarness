// SPDX-License-Identifier: Apache-2.0
//!
//! S4.13 — intra-run branches (`R-2.2.4`): open/promote/discard, the
//! SpeculationPolicy gate, and the single fold CC1 shares between rebuild
//! (replay) and commit (live).
//!
//! - `open_branch` mints `lifecycle.branch.opened` — kernel-owned
//!   (`kernel_origin`), so callers can never forge the record — carrying
//!   `scope.branch_id` (opens it) and `parent_event_id = fork point`.
//!   Branches open only at coherent fork points (the same `check_fork_point`
//!   table `fork`/`rollback` use); the default is HEAD.
//! - `promote_branch` validates every deferred effect's `releases` entry
//!   (absent ⇒ `DeferredReleaseMissing`), then appends one batch —
//!   `decided{allow|deny}` per effect → `committed{release: "branch_promotion"}`
//!   for released → `lifecycle.head.moved` → `lifecycle.branch.disposed{
//!   disposition: "promoted"}`. Promote = apply; fresh `attempt_no`, a new
//!   `fencing_token`, no deferred ids (AC-R-2.2.4-3/-4).
//! - `discard_branch` appends `head.moved` to the fork point +
//!   `disposed{disposition: "discarded"}` after terminating the branch's
//!   effects: intended/authorized → `refused{branch_discarded}`; prepared →
//!   `deferred{speculative_branch}` + `refused`; deferred → `refused`;
//!   committed-not-observed → `unknown{cancelled}`; observed(applied) +
//!   compensable → the §5a.7 saga; observed(applied) otherwise →
//!   `uncompensable[]` on the disposed row. The events stay in the WAL —
//!   `head.moved` + the `branch_id` scope close make them history, not HEAD.
//! - `check_branch_policy` runs in `append` (after `check_scopes`): a
//!   `read_only` branch admits no action/control writes; `committed` under a
//!   branch admits only `policy.allow_classes` — `defer_irreversible` is a
//!   floor (`irreversible` is never in the admissible set).

use std::collections::{BTreeMap, BTreeSet};

use hh_ontology::risk::{RiskReversibility, RiskScope};
use hh_wire::json::Json;

use crate::effect::{EffectFold, EffectPhase, ObservedOutcome};
use crate::errors::LedgerError;
use crate::event::EventEnvelope;
use crate::event::{Event, Scope};
use crate::recovery::kernel_ev;
use crate::saga::CompensationIntent;
use crate::store::Lease;
use crate::store::Store;

/// The hard cap on concurrently open intra-run branches — the
/// `policy.max_concurrent_branches` ceiling can only narrow it (floor).
pub const MAX_INTRA_BRANCHES: usize = 16;

// ─────────────────────────────────────────────────────────────────────────────
// Fold (shared by rebuild + commit — CC1)
// ─────────────────────────────────────────────────────────────────────────────

/// The intra-run branch's fold state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntraBranchState {
    /// `lifecycle.branch.opened` landed; the scope is live.
    Open,
    /// The disposition string (`promoted` | `discarded`).
    Disposed(String),
}

/// The fold-time record for one intra-run branch — `lifecycle.branch.opened`
/// plus every event carrying `scope.branch_id = <id>` folds in. The fold is
/// lenient (rebuild trusts the record); `append` enforces policy.
#[derive(Debug, Clone)]
pub struct IntraBranch {
    /// The scope id (`bch-…`).
    pub branch_id: String,
    /// `fork_seq`/`fork_event_id` — the coherent point the branch cut at.
    pub fork_seq: u64,
    /// The event the branch cut at (`parent_event_id` of `opened`).
    pub fork_event_id: String,
    /// `speculative` | `parallel`.
    pub kind: String,
    /// `read_only` — never promotable; admits no writes.
    pub read_only: bool,
    /// The `SpeculationPolicy` record (verbatim JSON), when carried.
    pub policy: Option<Json>,
    /// `budget_slice_id` — the slice the branch debits.
    pub budget_slice_id: Option<String>,
    /// Open or disposed (with the recorded disposition).
    pub state: IntraBranchState,
    /// seq/event of the opened row and the branch's deepest event (the
    /// promotion target `head.moved.to`).
    pub opened_seq: u64,
    /// The deepest `scope.branch_id` event's seq (the promotion target).
    pub leaf_seq: u64,
    /// … and its event id.
    pub leaf_event_id: String,
    /// `scope.branch_id` effect rows, first-seen seq order (the promotion
    /// order — "commits in branch order", KP-12).
    pub effect_first_seen: BTreeMap<String, u64>,
    /// `scope.branch_id` event count — the spend measure posted to
    /// `budget_slice_id` via `control.budget.consumed` at disposition
    /// (AC-R-2.2.4-10).
    pub events_count: u64,
}

impl IntraBranch {
    /// `speculative` kind (the defer floor applies); `parallel` is admitted
    /// for S4.13 but carries the same commit gate — a parallel branch's
    /// promotion table is the R-2.2.4 §6 frontier.
    pub fn is_speculative(&self) -> bool {
        self.kind != "parallel"
    }

    /// The admissible speculative classes — `policy.allow_classes` ∪
    /// `read_only`, default `{read_only, reversible_workspace_local}`
    /// (ADR-0214 §2). The `defer_irreversible` floor is structural:
    /// `irreversible` is never a member.
    pub fn allowed_classes(&self) -> BTreeSet<String> {
        let mut set = BTreeSet::new();
        set.insert("read_only".to_string());
        match self.policy.as_ref().and_then(|p| p.get("allow_classes")) {
            Some(Json::Arr(cs)) => {
                for c in cs.iter().filter_map(Json::as_str) {
                    set.insert(c.to_string());
                }
            }
            _ => {
                set.insert("reversible_workspace_local".to_string());
            }
        }
        set
    }
}

/// One fold step — `rebuild` and `commit_envelopes` both call it (CC1).
pub fn fold_intra_branch(map: &mut BTreeMap<String, IntraBranch>, env: &EventEnvelope) {
    match env.class.as_str() {
        "lifecycle.branch.opened" => {
            let p = &env.payload;
            let Some(branch_id) = str_field(p, "branch_id") else {
                return;
            };
            map.insert(
                branch_id.clone(),
                IntraBranch {
                    branch_id,
                    fork_seq: p.get("fork_seq").and_then(Json::as_int).unwrap_or(0).max(0) as u64,
                    fork_event_id: str_field(p, "fork_event_id").unwrap_or_default(),
                    kind: str_field(p, "kind").unwrap_or_else(|| "speculative".into()),
                    read_only: p.get("read_only") == Some(&Json::Bool(true)),
                    policy: p.get("policy").cloned(),
                    budget_slice_id: str_field(p, "budget_slice_id"),
                    state: IntraBranchState::Open,
                    opened_seq: env.seq,
                    leaf_seq: env.seq,
                    leaf_event_id: env.event_id.clone(),
                    effect_first_seen: BTreeMap::new(),
                    events_count: 0,
                },
            );
        }
        "lifecycle.branch.disposed" => {
            if let Some(b) = str_field(&env.payload, "branch_id").and_then(|id| map.get_mut(&id)) {
                b.state = IntraBranchState::Disposed(
                    str_field(&env.payload, "disposition").unwrap_or_default(),
                );
            }
        }
        _ => {}
    }
    // Any event carrying `scope.branch_id` advances the branch leaf; effect
    // rows land in the first-seen order list.
    if let Some(bid) = env.scope.branch_id.as_deref() {
        if let Some(b) = map.get_mut(bid) {
            b.leaf_seq = env.seq;
            b.leaf_event_id = env.event_id.clone();
            b.events_count += 1;
            if env.class.starts_with("action.effect.") {
                if let Some(eid) = env
                    .scope
                    .effect_id
                    .clone()
                    .or_else(|| str_field(&env.payload, "effect_id"))
                {
                    b.effect_first_seen.entry(eid).or_insert(env.seq);
                }
            }
        }
    }
}

fn str_field(p: &Json, k: &str) -> Option<String> {
    p.get(k).and_then(Json::as_str).map(str::to_string)
}

/// AC-R-2.2.4-10: a branch's spend posts to `spec.budget_slice_id` — every
/// disposition appends `control.budget.consumed{slice}` debiting the slice
/// with the branch's scoped-event count (the ledger's honest measure; a
/// metered dimension from the caller's budget sub-system may extend
/// `consumed` later — additive only).
fn budget_consumed_ev(
    store: &Store,
    run_id: &str,
    br: &IntraBranch,
) -> Result<Option<Event>, LedgerError> {
    let Some(slice) = &br.budget_slice_id else {
        return Ok(None);
    };
    Ok(Some(kernel_ev(
        store,
        run_id,
        "control.budget.consumed",
        Scope::default(),
        Json::obj([
            ("budget_slice_id", Json::str(slice)),
            ("branch_id", Json::str(&br.branch_id)),
            ("node", Json::str(format!("bch_budget:{}", br.branch_id))),
            (
                "consumed",
                Json::obj([("events", Json::Int(br.events_count as i64))]),
            ),
        ]),
    )?))
}

// ─────────────────────────────────────────────────────────────────────────────
// The append-time policy gate
// ─────────────────────────────────────────────────────────────────────────────

/// The speculative class an effect's `risk_class` maps to (R-2.2.4 §2).
fn speculative_class(f: &EffectFold) -> &'static str {
    if f.risk_class.is_read_only() {
        "read_only"
    } else if f.risk_class.reversibility == RiskReversibility::Reversible
        && f.risk_class.scope == RiskScope::WorkspaceLocal
    {
        "reversible_workspace_local"
    } else if f.risk_class.reversibility == RiskReversibility::Compensable {
        "compensable"
    } else {
        "irreversible"
    }
}

/// `read_only` branch allowlist — audit/observation planes only (the branch
/// may read and record; it performs no writes).
fn read_only_admissible(class: &str) -> bool {
    class.starts_with("lifecycle.")
        || class.starts_with("context.")
        || class.starts_with("derived.")
        || class.starts_with("security.label.")
        || class.starts_with("security.audit.")
        || class.starts_with("measurement.")
        || class.starts_with("artefact.")
        || class.starts_with("results.")
        || class == "action.effect.refused"
        || class == "action.effect.unattributed"
}

/// The branch-scope gate — `append` calls it after `check_scopes` (the scope
/// is known-open). Refuses `action.effect.committed` outside `allow_classes`
/// (defer is required — promote releases), refuses any write under a
/// `read_only` branch, and pins `deferred.branch_id` to the scope branch.
///
/// `batch` is the already-staged prefix of the in-flight batch (an effect's
/// `intended` may sit earlier in the same batch — the gate folds it in).
pub fn check_branch_policy(
    branches: &BTreeMap<String, IntraBranch>,
    effects: &BTreeMap<String, EffectFold>,
    ev: &Event,
) -> Result<(), LedgerError> {
    let Some(bid) = ev.scope.branch_id.as_deref() else {
        return Ok(());
    };
    // `opened`/`disposed` themselves carry the scope they open/close.
    if matches!(
        ev.class.as_str(),
        "lifecycle.branch.opened" | "lifecycle.branch.disposed"
    ) {
        return Ok(());
    }
    let Some(br) = branches.get(bid) else {
        // `check_scopes` already refused unopened branch scopes — a missing
        // fold entry for an open scope is a kernel defect; refuse loudly.
        return Err(LedgerError::UnknownBranch {
            branch_id: bid.to_string(),
        });
    };
    if br.read_only && !read_only_admissible(&ev.class) {
        return Err(LedgerError::SpeculationViolation {
            branch_id: bid.to_string(),
            detail: format!(
                "read_only branch {bid} admits no {} writes (observe only;                  effect {:?})",
                ev.class,
                ev.scope.effect_id
            ),
        });
    }
    match ev.class.as_str() {
        "action.effect.committed" => {
            // A promotion-release commit mints under the kernel's own gate —
            // `release: "branch_promotion"` marks it (the promote batch's
            // decided row precedes it, so I-H7 still holds).
            if str_field(&ev.payload, "release").as_deref() == Some("branch_promotion") {
                return Ok(());
            }
            if !br.is_speculative() {
                return Ok(());
            }
            let effect_id = ev.scope.effect_id.as_deref().unwrap_or("");
            let Some(f) = effects.get(effect_id) else {
                return Ok(());
            };
            let class = speculative_class(f);
            if !br.allowed_classes().contains(class) {
                return Err(LedgerError::SpeculationViolation {
                    branch_id: bid.to_string(),
                    detail: format!(
                        "committed under speculative branch {bid}: class {class} is not \
                         in allow_classes — defer → promote is the release path \
                         (defer_irreversible floor)",
                    ),
                });
            }
        }
        "action.effect.deferred" => {
            // `deferred.branch_id` pins to the scope branch — the deferred
            // table is branch-local (KP-11).
            if let Some(pb) = str_field(&ev.payload, "branch_id") {
                if pb != bid {
                    return Err(LedgerError::SchemaViolation {
                        detail: format!("deferred.branch_id {pb} ≠ scope.branch_id {bid}"),
                    });
                }
            }
        }
        _ => {}
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Ops
// ─────────────────────────────────────────────────────────────────────────────

/// `branch.open` — the input record.
#[derive(Debug, Clone)]
pub struct OpenBranchSpec {
    /// `speculative` | `parallel` (default `speculative`).
    pub kind: String,
    /// The fork point seq — `None` forks at HEAD. Fork coherence is the same
    /// `check_fork_point` table `fork`/`rollback` use (CC8).
    pub fork_seq: Option<u64>,
    /// `read_only` — the branch may observe, never write, never promote.
    pub read_only: bool,
    /// The `SpeculationPolicy` record (verbatim); `None` ⇒ ADR-0214 defaults.
    pub policy: Option<Json>,
    /// `budget_slice_id` — the slice the branch debits.
    pub budget_slice_id: Option<String>,
    /// `env_binding` — the resolved environment (recorded, not resolved here —
    /// env derivation is caller-side and env-class gated).
    pub env_binding: Option<String>,
    /// `evidence_path` — the coverage artefact the policy requires.
    pub evidence_path: Option<String>,
    /// `permissions` — the permission ids the branch claims (AC-R-2.2.4-10:
    /// each must already be `allow`-decided or `granted` in the parent run —
    /// a claim the parent does not hold is `AuthorityWidening` at `fork`;
    /// ADR-0134 §5 "a branch may only narrow").
    pub permissions: Vec<String>,
    /// Payload extras.
    pub extra: Vec<(String, Json)>,
}

/// `promote` — the caller's release decision per deferred effect. Absent ⇒
/// `DeferredReleaseMissing` (AC-R-2.2.4-4: the caller must name every one).
pub type ReleaseMap = BTreeMap<String, bool>;

/// `discard` — the accounting the caller (and the `disposed` row) reads.
#[derive(Debug, Clone)]
pub struct DiscardOutcome {
    /// Effects refused (`intended`/`authorized`/`prepared`/`deferred`/`not_applied`).
    pub refused: Vec<String>,
    /// Compensators that reached `compensated`/`applied_noop`.
    pub compensated: Vec<String>,
    /// Compensators that failed — the record holds the error.
    pub compensation_failed: Vec<String>,
    /// `observed{applied}` effects no compensator covers (KP-20).
    pub uncompensable: Vec<String>,
    /// `committed`/`unknown`-phase effects handed to reconciliation (KP-19).
    pub unresolved: Vec<String>,
}

impl Store {
    /// The run's intra-run branch folds (sorted by branch_id).
    pub fn intra_branches(&self, run_id: &str) -> Vec<IntraBranch> {
        let Ok(state) = self.run(run_id) else {
            return Vec::new();
        };
        let mut v: Vec<IntraBranch> = state.intra_branches.values().cloned().collect();
        v.sort_by(|a, b| a.branch_id.cmp(&b.branch_id));
        v
    }

    /// One intra-run branch fold.
    pub fn intra_branch(&self, run_id: &str, branch_id: &str) -> Option<IntraBranch> {
        self.run(run_id)
            .ok()
            .and_then(|s| s.intra_branches.get(branch_id).cloned())
    }

    /// `branch.open` (R-2.2.4) — mints `lifecycle.branch.opened` at a coherent
    /// fork point (`check_fork_point`; default HEAD). Admissible kinds:
    /// `speculative` | `parallel`. `policy.max_concurrent_branches` gates the
    /// open-branch count (floor `MAX_INTRA_BRANCHES`).
    pub fn open_branch(
        &mut self,
        run_id: &str,
        lease: &Lease,
        spec: &OpenBranchSpec,
    ) -> Result<String, LedgerError> {
        self.tier_c1("branch.open")?;
        self.active_lease(run_id, lease)?;
        if self.run(run_id)?.finished {
            return Err(LedgerError::RunFinished {
                run_id: run_id.into(),
            });
        }
        if spec.kind != "speculative" && spec.kind != "parallel" {
            return Err(LedgerError::SchemaViolation {
                detail: format!(
                    "branch.kind must be speculative|parallel, got {}",
                    spec.kind
                ),
            });
        }
        // AC-R-2.2.4-10 (containment at `fork`) — three narrowing checks:
        //
        // 1. `defer_irreversible` is a floor: a policy claiming
        //    `allow_classes ⊋ irreversible` or `defer_irreversible: false`
        //    *widens* — `AuthorityWidening`, never coerced (ADR-0134 §5's
        //    "not overridable by any composition layer" clause).
        // 2. `permissions[]` claims must each be held by the parent — a
        //    `security.permission.decided{allow}` or `granted` row naming
        //    the id in the durable prefix; a claim the parent lacks is
        //    `AuthorityWidening` (the branch never mints its own cover).
        // 3. `policy` may only *narrow* the parent's — the run carries no
        //    ambient `SpeculationPolicy`, so the default admissible set is
        //    the baseline a branch narrows from.
        if let Some(p) = &spec.policy {
            let widens_class = p
                .get("allow_classes")
                .and_then(|c| match c {
                    Json::Arr(v) => Some(v.iter().any(|c| c.as_str() == Some("irreversible"))),
                    _ => None,
                })
                .unwrap_or(false);
            let widens_defer = p.get("defer_irreversible") == Some(&Json::Bool(false));
            if widens_class || widens_defer {
                return Err(LedgerError::AuthorityWidening {
                    detail: format!(
                        "SpeculationPolicy widens the defer_irreversible floor \
                         (allow_classes admits irreversible: {widens_class}, \
                         defer_irreversible: false: {widens_defer})"
                    ),
                });
            }
        }
        if !spec.permissions.is_empty() {
            let held = |pid: &str| -> bool {
                self.run(run_id)
                    .map(|s| {
                        s.events.iter().any(|e| {
                            (e.class == "security.permission.decided"
                                && e.payload.get("permission_id").and_then(Json::as_str)
                                    == Some(pid)
                                && e.payload.get("decision").and_then(Json::as_str)
                                    == Some("allow"))
                                || (e.class == "security.permission.granted"
                                    && e.payload.get("permission_id").and_then(Json::as_str)
                                        == Some(pid))
                        })
                    })
                    .unwrap_or(false)
            };
            for pid in &spec.permissions {
                if !held(pid) {
                    return Err(LedgerError::AuthorityWidening {
                        detail: format!(
                            "branch claims permission {pid} the parent run does not hold \
                             (no allow decision/grant in the durable prefix)"
                        ),
                    });
                }
            }
        }
        let open_count = self
            .run(run_id)?
            .intra_branches
            .values()
            .filter(|b| b.state == IntraBranchState::Open)
            .count();
        let cap = spec
            .policy
            .as_ref()
            .and_then(|p| p.get("max_concurrent_branches"))
            .and_then(Json::as_int)
            .filter(|n| *n > 0)
            .map(|n| n as usize)
            .unwrap_or(1)
            .min(MAX_INTRA_BRANCHES);
        if open_count >= cap {
            return Err(LedgerError::SchemaViolation {
                detail: format!(
                    "branch.open refused: {open_count} open branches ≥ \
                     max_concurrent_branches {cap}"
                ),
            });
        }
        let (fork_seq, fork_event_id) = match spec.fork_seq {
            Some(seq) => {
                self.check_fork_point(run_id, seq)?;
                let ev = self
                    .run(run_id)?
                    .events
                    .iter()
                    .find(|e| e.seq == seq)
                    .ok_or_else(|| LedgerError::SchemaViolation {
                        detail: format!("branch.open fork_seq {seq} out of range"),
                    })?;
                (seq, ev.event_id.clone())
            }
            None => {
                let h = self.run(run_id)?.head.as_ref().ok_or_else(|| {
                    LedgerError::SchemaViolation {
                        detail: "branch.open on an empty run".into(),
                    }
                })?;
                (h.seq, h.event_id.clone())
            }
        };
        let branch_id = self.alloc_id("bch");
        let mut members: Vec<(String, Json)> = vec![
            ("branch_id".into(), Json::str(&branch_id)),
            ("fork_seq".into(), Json::Int(fork_seq as i64)),
            ("fork_event_id".into(), Json::str(&fork_event_id)),
            ("kind".into(), Json::str(&spec.kind)),
            ("read_only".into(), Json::Bool(spec.read_only)),
        ];
        if let Some(p) = &spec.policy {
            members.push(("policy".into(), p.clone()));
        }
        if let Some(b) = &spec.budget_slice_id {
            members.push(("budget_slice_id".into(), Json::str(b)));
        }
        if let Some(e) = &spec.env_binding {
            members.push(("env_binding".into(), Json::str(e)));
        }
        if let Some(e) = &spec.evidence_path {
            members.push(("evidence_path".into(), Json::str(e)));
        }
        if !spec.permissions.is_empty() {
            members.push((
                "permissions".into(),
                Json::Arr(spec.permissions.iter().map(Json::str).collect()),
            ));
        }
        members.extend(spec.extra.iter().cloned());
        let mut ev = kernel_ev(
            self,
            run_id,
            "lifecycle.branch.opened",
            Scope {
                branch_id: Some(branch_id.clone()),
                ..Scope::default()
            },
            Json::Obj(members.iter().cloned().collect()),
        )?;
        // `parent_event_id` = the fork point (spec §4.9) — HEAD when forking
        // at the tip, the fork event when forking back in time.
        ev.parent_event_id = fork_event_id;
        self.append(run_id, lease, vec![ev])?;
        Ok(branch_id)
    }

    /// `promote` — merge the branch's accepted work into HEAD
    /// (AC-R-2.2.4-3/-4). Requires `releases` to name *every* deferred effect
    /// (`DeferredReleaseMissing` otherwise). One batch: `decided{allow|deny}`
    /// → `committed{release: branch_promotion}` per released effect →
    /// `head.moved` → `disposed{promoted}`.
    pub fn promote_branch(
        &mut self,
        run_id: &str,
        lease: &Lease,
        branch_id: &str,
        releases: &ReleaseMap,
    ) -> Result<(), LedgerError> {
        self.tier_c1("branch.promote")?;
        self.active_lease(run_id, lease)?;
        let generation = lease.generation;
        if self.run(run_id)?.finished {
            return Err(LedgerError::RunFinished {
                run_id: run_id.into(),
            });
        }
        let (br, deferred) = {
            let state = self.run(run_id)?;
            let br = state
                .intra_branches
                .get(branch_id)
                .cloned()
                .ok_or_else(|| LedgerError::UnknownBranch {
                    branch_id: branch_id.into(),
                })?;
            if let IntraBranchState::Disposed(d) = &br.state {
                return Err(LedgerError::BranchDisposed {
                    branch_id: branch_id.into(),
                    disposition: d.clone(),
                });
            }
            if br.read_only {
                return Err(LedgerError::SpeculationViolation {
                    branch_id: branch_id.into(),
                    detail: format!("read_only branch {branch_id} is never promotable"),
                });
            }
            let mut deferred: Vec<(u64, String)> = br
                .effect_first_seen
                .iter()
                .filter(|(eid, _)| {
                    state
                        .effects
                        .get(*eid)
                        .map(|f| f.phase == EffectPhase::Deferred)
                        .unwrap_or(false)
                })
                .map(|(eid, seq)| (*seq, eid.clone()))
                .collect();
            deferred.sort();
            (br, deferred)
        };
        // Every deferred effect must be named (AC-R-2.2.4-4).
        let missing: Vec<String> = deferred
            .iter()
            .map(|(_, eid)| eid.clone())
            .filter(|eid| !releases.contains_key(eid))
            .collect();
        if !missing.is_empty() {
            return Err(LedgerError::DeferredReleaseMissing {
                branch_id: branch_id.into(),
                effect_ids: missing,
            });
        }

        let branch_scope = |effect_id: Option<String>| Scope {
            effect_id,
            branch_id: Some(branch_id.to_string()),
            ..Scope::default()
        };
        let mut batch: Vec<Event> = Vec::new();
        let mut released: Vec<String> = Vec::new();
        let mut denied: Vec<String> = Vec::new();
        for (_, effect_id) in &deferred {
            let allow = releases.get(effect_id).copied().unwrap_or(false);
            let next_attempt = self
                .run(run_id)?
                .effects
                .get(effect_id)
                .map(|f| f.attempt_no)
                .unwrap_or(0)
                + 1;
            batch.push(kernel_ev(
                self,
                run_id,
                "security.permission.decided",
                branch_scope(Some(effect_id.clone())),
                Json::obj([
                    (
                        "permission_id",
                        Json::str(format!("branch_promotion:{branch_id}:{effect_id}")),
                    ),
                    ("decision", Json::str(if allow { "allow" } else { "deny" })),
                    ("attempt_no", Json::Int(next_attempt as i64)),
                    ("decider", Json::str("branch.promote")),
                    (
                        "reason",
                        Json::str("promotion release decision (AC-R-2.2.4-4)"),
                    ),
                ]),
            )?);
            if allow {
                batch.push(kernel_ev(
                    self,
                    run_id,
                    "action.effect.committed",
                    branch_scope(Some(effect_id.clone())),
                    Json::obj([
                        ("attempt_no", Json::Int(next_attempt as i64)),
                        ("fencing_token", Json::Int(generation as i64)),
                        ("release", Json::str("branch_promotion")),
                        ("branch_id", Json::str(branch_id)),
                    ]),
                )?);
                released.push(effect_id.clone());
            } else {
                denied.push(effect_id.clone());
            }
        }
        // `head.moved` → the branch leaf, then `disposed{promoted}` — the last
        // branch-scoped row closes the scope.
        let (from_id, from_seq) =
            {
                let h = self.run(run_id)?.head.as_ref().ok_or_else(|| {
                    LedgerError::SchemaViolation {
                        detail: "promote on an empty run".into(),
                    }
                })?;
                (h.event_id.clone(), h.seq)
            };
        batch.push(kernel_ev(
            self,
            run_id,
            "lifecycle.head.moved",
            Scope::default(),
            Json::obj([
                ("from_event_id", Json::str(&from_id)),
                ("from_seq", Json::Int(from_seq as i64)),
                ("to_event_id", Json::str(&br.leaf_event_id)),
                ("to_seq", Json::Int(br.leaf_seq as i64)),
                ("reason", Json::str(format!("branch promote {branch_id}"))),
            ]),
        )?);
        batch.push(kernel_ev(
            self,
            run_id,
            "lifecycle.branch.disposed",
            branch_scope(None),
            Json::obj([
                ("branch_id", Json::str(branch_id)),
                ("disposition", Json::str("promoted")),
                (
                    "released",
                    Json::Arr(released.iter().map(Json::str).collect()),
                ),
                ("denied", Json::Arr(denied.iter().map(Json::str).collect())),
                ("merged_at_event_id", Json::str(&br.leaf_event_id)),
            ]),
        )?);
        if let Some(ev) = budget_consumed_ev(self, run_id, &br)? {
            batch.push(ev);
        }
        self.append(run_id, lease, batch)?;
        self.notify_rewind(
            run_id,
            br.leaf_seq,
            &br.leaf_event_id,
            &format!("branch promote {branch_id}"),
        );
        Ok(())
    }

    /// `discard` — drop the branch: HEAD rewinds to the fork point, the
    /// branch's effects terminate per the discard table (KP-16..21), and the
    /// `disposed{discarded}` row records refused/compensated/uncompensable —
    /// the caller reads `uncaptured` to re-inject any orphaned side-effects.
    /// `uncaptured`/`restored_snapshot_ref` come from the caller's env
    /// restore pass (the ledger records; the env crate performs).
    pub fn discard_branch(
        &mut self,
        run_id: &str,
        lease: &Lease,
        branch_id: &str,
        uncaptured: &[String],
        restored_snapshot_ref: Option<&str>,
        dispatch: &mut dyn FnMut(&CompensationIntent) -> Result<Json, String>,
    ) -> Result<DiscardOutcome, LedgerError> {
        self.tier_c1("branch.discard")?;
        self.active_lease(run_id, lease)?;
        let generation = lease.generation;
        if self.run(run_id)?.finished {
            return Err(LedgerError::RunFinished {
                run_id: run_id.into(),
            });
        }
        let (br, branch_effects) = {
            let state = self.run(run_id)?;
            let br = state
                .intra_branches
                .get(branch_id)
                .cloned()
                .ok_or_else(|| LedgerError::UnknownBranch {
                    branch_id: branch_id.into(),
                })?;
            if let IntraBranchState::Disposed(d) = &br.state {
                return Err(LedgerError::BranchDisposed {
                    branch_id: branch_id.into(),
                    disposition: d.clone(),
                });
            }
            let mut ids: Vec<(u64, String)> = br
                .effect_first_seen
                .iter()
                .map(|(eid, seq)| (*seq, eid.clone()))
                .collect();
            ids.sort();
            (br, ids)
        };

        let branch_scope = |effect_id: Option<String>| Scope {
            effect_id,
            branch_id: Some(branch_id.to_string()),
            ..Scope::default()
        };
        let mut batch: Vec<Event> = Vec::new();
        let mut refused: Vec<String> = Vec::new();
        let mut compensate: Vec<String> = Vec::new();
        let mut uncompensable: Vec<String> = Vec::new();
        let mut unresolved: Vec<String> = Vec::new();
        for (_, effect_id) in &branch_effects {
            let f = self.run(run_id)?.effects.get(effect_id).cloned();
            let Some(f) = f else { continue };
            match f.phase {
                EffectPhase::Intended | EffectPhase::Authorized => {
                    batch.push(kernel_ev(
                        self,
                        run_id,
                        "action.effect.refused",
                        branch_scope(Some(effect_id.clone())),
                        Json::obj([
                            ("reason", Json::str("branch_discarded")),
                            ("decider", Json::str("kernel")),
                        ]),
                    )?);
                    refused.push(effect_id.clone());
                }
                EffectPhase::Prepared => {
                    // `refused` cannot leave `prepared` — defer under the
                    // branch then refuse (the recorded trail is honest: the
                    // prepared key never dispatched).
                    batch.push(kernel_ev(
                        self,
                        run_id,
                        "action.effect.deferred",
                        branch_scope(Some(effect_id.clone())),
                        Json::obj([
                            ("reason", Json::str("speculative_branch")),
                            ("branch_id", Json::str(branch_id)),
                        ]),
                    )?);
                    batch.push(kernel_ev(
                        self,
                        run_id,
                        "action.effect.refused",
                        branch_scope(Some(effect_id.clone())),
                        Json::obj([
                            ("reason", Json::str("branch_discarded")),
                            ("decider", Json::str("kernel")),
                        ]),
                    )?);
                    refused.push(effect_id.clone());
                }
                EffectPhase::Deferred => {
                    batch.push(kernel_ev(
                        self,
                        run_id,
                        "action.effect.refused",
                        branch_scope(Some(effect_id.clone())),
                        Json::obj([
                            ("reason", Json::str("branch_discarded")),
                            ("decider", Json::str("kernel")),
                        ]),
                    )?);
                    refused.push(effect_id.clone());
                }
                EffectPhase::Committed => {
                    // In-flight — outcome unknowable once HEAD leaves; the
                    // §05f probe machinery reconciles (KP-19).
                    batch.push(kernel_ev(
                        self,
                        run_id,
                        "action.effect.unknown",
                        branch_scope(Some(effect_id.clone())),
                        Json::obj([
                            ("cause", Json::str("cancelled")),
                            ("fencing_token", Json::Int(generation as i64)),
                        ]),
                    )?);
                    unresolved.push(effect_id.clone());
                }
                EffectPhase::Observed => {
                    let applied = f.outcome == Some(ObservedOutcome::Applied);
                    if !applied {
                        // not_applied terminal — nothing ran.
                        refused.push(effect_id.clone());
                    } else if f.risk_class.reversibility == RiskReversibility::Compensable {
                        compensate.push(effect_id.clone());
                    } else {
                        uncompensable.push(effect_id.clone());
                    }
                }
                EffectPhase::Unknown => {
                    unresolved.push(effect_id.clone());
                }
                _ => {}
            }
        }
        // `head.moved` to the fork point — the discard rewinds HEAD; the
        // branch events stay (append-only). `disposed` closes the scope.
        let (from_id, from_seq) =
            {
                let h = self.run(run_id)?.head.as_ref().ok_or_else(|| {
                    LedgerError::SchemaViolation {
                        detail: "discard on an empty run".into(),
                    }
                })?;
                (h.event_id.clone(), h.seq)
            };
        batch.push(kernel_ev(
            self,
            run_id,
            "lifecycle.head.moved",
            Scope::default(),
            Json::obj([
                ("from_event_id", Json::str(&from_id)),
                ("from_seq", Json::Int(from_seq as i64)),
                ("to_event_id", Json::str(&br.fork_event_id)),
                ("to_seq", Json::Int(br.fork_seq as i64)),
                ("reason", Json::str(format!("branch discard {branch_id}"))),
            ]),
        )?);
        let mut disposed_members: Vec<(String, Json)> = vec![
            ("branch_id".into(), Json::str(branch_id)),
            ("disposition".into(), Json::str("discarded")),
            (
                "refused".into(),
                Json::Arr(refused.iter().map(Json::str).collect()),
            ),
            (
                "compensated".into(),
                Json::Arr(compensate.iter().map(Json::str).collect()),
            ),
            (
                "uncompensable".into(),
                Json::Arr(uncompensable.iter().map(Json::str).collect()),
            ),
            (
                "unresolved".into(),
                Json::Arr(unresolved.iter().map(Json::str).collect()),
            ),
            (
                "uncaptured".into(),
                Json::Arr(uncaptured.iter().map(Json::str).collect()),
            ),
            ("merged_at_event_id".into(), Json::str(&br.fork_event_id)),
        ];
        if let Some(r) = restored_snapshot_ref {
            disposed_members.push(("restored_snapshot_ref".into(), Json::str(r)));
        }
        batch.push(kernel_ev(
            self,
            run_id,
            "lifecycle.branch.disposed",
            branch_scope(None),
            Json::Obj(disposed_members.iter().cloned().collect()),
        )?);
        if let Some(ev) = budget_consumed_ev(self, run_id, &br)? {
            batch.push(ev);
        }
        self.append(run_id, lease, batch)?;

        // Compensators run *after* the head move — the saga is its own effect
        // tree, appended on the live head.
        let mut compensated: Vec<String> = Vec::new();
        let mut compensation_failed: Vec<String> = Vec::new();
        for effect_id in &compensate {
            match self.compensate_one(run_id, lease, generation, effect_id, dispatch) {
                Ok(status) if status == "compensated" || status == "applied_noop" => {
                    compensated.push(effect_id.clone())
                }
                Ok(other) => compensation_failed.push(format!("{effect_id}: {other}")),
                Err(e) => compensation_failed.push(format!("{effect_id}: {e}")),
            }
        }
        self.notify_rewind(
            run_id,
            br.fork_seq,
            &br.fork_event_id,
            &format!("branch discard {branch_id}"),
        );
        Ok(DiscardOutcome {
            refused,
            compensated,
            compensation_failed,
            uncompensable,
            unresolved,
        })
    }

    /// `budget_exhausted` — AC-R-2.2.4-10's root-first ruling: exhaustion at
    /// the run's *root* budget node (the manifest `budget` id) ends every
    /// open branch; exhaustion of a named slice ends only the branches
    /// debiting it (`budget_slice_id == node_id`). Each affected branch is
    /// `disposed{abandoned}` with `reason: budget_exhausted` — an abandon is
    /// never a promote: HEAD rewinds to the branch's fork point and its
    /// deferred/non-terminal effects are refused `budget_exhausted`; effects
    /// already terminal keep their recorded verdict (the compensation path
    /// stays on `discard` — exhaustion is a stop, not an undo).
    /// Returns the `control.budget.exceeded` row's first seq.
    pub fn budget_exhausted(
        &mut self,
        run_id: &str,
        lease: &Lease,
        node_id: &str,
    ) -> Result<u64, LedgerError> {
        self.tier_c1("branch.budget_exhausted")?;
        self.active_lease(run_id, lease)?;
        let (root_first, targets) = {
            let s = self.run(run_id)?;
            if s.finished {
                return Err(LedgerError::RunFinished {
                    run_id: run_id.into(),
                });
            }
            let root_first = self.manifest(run_id)?.budget.as_deref() == Some(node_id);
            let targets: Vec<IntraBranch> = s
                .intra_branches
                .values()
                .filter(|b| {
                    b.state == IntraBranchState::Open
                        && (root_first || b.budget_slice_id.as_deref() == Some(node_id))
                })
                .cloned()
                .collect();
            (root_first, targets)
        };
        let mut batch: Vec<Event> = Vec::new();
        batch.push(kernel_ev(
            self,
            run_id,
            "control.budget.exceeded",
            Scope::default(),
            Json::obj([
                ("node", Json::str(node_id)),
                ("root_first", Json::Bool(root_first)),
            ]),
        )?);
        for br in &targets {
            let bid = br.branch_id.clone();
            // Deferred/non-terminal branch effects end refused — the branch's
            // execution stops with its slice.
            let mut ids: Vec<(u64, String)> = br
                .effect_first_seen
                .iter()
                .map(|(eid, seq)| (*seq, eid.clone()))
                .collect();
            ids.sort();
            for (_, eid) in &ids {
                let Some(f) = self.run(run_id)?.effects.get(eid).cloned() else {
                    continue;
                };
                if matches!(
                    f.phase,
                    EffectPhase::Intended
                        | EffectPhase::Authorized
                        | EffectPhase::Prepared
                        | EffectPhase::Deferred
                ) {
                    batch.push(kernel_ev(
                        self,
                        run_id,
                        "action.effect.refused",
                        Scope {
                            effect_id: Some(eid.clone()),
                            branch_id: Some(bid.clone()),
                            ..Scope::default()
                        },
                        Json::obj([
                            ("reason", Json::str("budget_exhausted")),
                            ("decider", Json::str("kernel")),
                        ]),
                    )?);
                }
            }
            let h =
                self.run(run_id)?
                    .head
                    .as_ref()
                    .ok_or_else(|| LedgerError::SchemaViolation {
                        detail: "budget_exhausted on an empty run".into(),
                    })?;
            batch.push(kernel_ev(
                self,
                run_id,
                "lifecycle.head.moved",
                Scope::default(),
                Json::obj([
                    ("from_event_id", Json::str(&h.event_id)),
                    ("from_seq", Json::Int(h.seq as i64)),
                    ("to_event_id", Json::str(&br.fork_event_id)),
                    ("to_seq", Json::Int(br.fork_seq as i64)),
                    (
                        "reason",
                        Json::str(format!("branch abandon {bid}: budget_exhausted")),
                    ),
                ]),
            )?);
            batch.push(kernel_ev(
                self,
                run_id,
                "lifecycle.branch.disposed",
                Scope {
                    branch_id: Some(bid.clone()),
                    ..Scope::default()
                },
                Json::obj([
                    ("branch_id", Json::str(&bid)),
                    ("disposition", Json::str("abandoned")),
                    ("reason", Json::str("budget_exhausted")),
                    ("budget_slice_id", Json::str(node_id)),
                ]),
            )?);
            if let Some(ev) = budget_consumed_ev(self, run_id, br)? {
                batch.push(ev);
            }
        }
        let range = self.append(run_id, lease, batch)?;
        Ok(range.first)
    }
}
