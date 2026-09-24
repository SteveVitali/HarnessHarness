//! `R-2.4.4⁰` — the lifecycle pure functions (§5c.4; ADR-0080…0082):
//! `lifecycle_state` (E1), `check_contract`, `filter_for_slot` (E2),
//! `write`-side checks (`check_supersede`/`check_revoke`, E4), deterministic
//! conflict sets, `promote`/`revalidate`, `stale_index`, `dependants`.
//!
//! All of it is **model-free**: no text inspection, no model call — the
//! judged tail of conflict detection and the model-generated-mark side of
//! `mark_used` are explicitly excluded (C2; DF row). The precedence
//! `revoked > superseded > expired > stale_by_dependency > valid > unknown`
//! is the §5c.4 order; `unknown` is a real outcome, never silently upgraded.
//!
//! View-rebuild equality (V-DET): `lifecycle_state` and `stale_index` are
//! pure over `(edges, revocations, validity, contract, dependency stamps)`
//! evaluated at `at` — the fold is deterministic under the same append order.

use std::collections::{BTreeMap, BTreeSet};

use hh_identity::kinds::RecordKind;
use hh_identity::supersede::RevocationRecord as LineageRevocation;
use hh_ledger::views::View;
use hh_provenance::authority::ReaderSet;
use hh_provenance::origin::Origin;
use hh_provenance::record::ProvenanceRecord;
use hh_provenance::PersistenceScope;
use hh_wire::json::Json;

use crate::memory::{
    ConflictSet, DependencyStamp, InvalidationContract, MemoryError, MemoryRevocation, MemoryStore,
    MemoryVersion,
};
use crate::vocab::{
    ConflictResolution, DependencyKind, InvalidationCondition, LifecycleState, LifecycleStateKind,
    RevocationReason,
};

// ─────────────────────────────────────────────────────────────────────────────
// check_contract (E1's contract half)
// ─────────────────────────────────────────────────────────────────────────────

/// `check_contract(contract, at, env)` — the contract half of `lifecycle_state`
/// (§5c.4): the checkable members each evaluate against the caller-supplied
/// environment (`current_stamp` for `dependencies`, `validator_verdict` for
/// `validator_ref`). `fired` carries every dependency whose current stamp
/// differs from the write-pinned `stamp`.
#[derive(Debug, Clone, Default)]
pub struct ContractCheck {
    /// Every pinned stamp still matches the environment's current stamp.
    pub stamps_ok: bool,
    /// The dependencies whose current stamp differs (they fired).
    pub fired: Vec<DependencyStamp>,
    /// `freshness` holds at `at` (`true` when absent — nothing checkable).
    pub fresh: bool,
    /// The validator's last verdict for `(version_id, validator_ref)`:
    /// `Some(true)` pass, `Some(false)` fail, `None` no verdict recorded.
    pub validator: Option<bool>,
    /// `invalidation_condition` fired (the condition kinds the environment
    /// can evaluate: `ttl_elapsed`, `validator_fails` are derived from the
    /// above; `scope_ended`/`replacement_published` come from the caller via
    /// [`LifecycleInput::condition_fired`]).
    pub condition_fired: bool,
}

/// `LifecycleEnv::current_stamp` — resolves a declared [`DependencyStamp`]
/// to the environment's current value (`None` when the environment can't
/// answer — the honest `unknown`, never a guess).
pub type StampLookup<'a> = &'a dyn Fn(&DependencyStamp) -> Option<String>;

/// `LifecycleEnv::validator_verdict` — `(version_id, validator_ref) →
/// verdict` against the caller's verdict table.
pub type VerdictLookup<'a> = &'a dyn Fn(&str, &str) -> Option<bool>;

/// `LifecycleEnv::condition_fired` — evaluates an [`InvalidationCondition`]
/// against the version (C0: only `superseded`/`refreshed`/`ttl` evaluate
/// internally; anything else is `unknown`, not silently false).
pub type ConditionProbe<'a> = &'a dyn Fn(&InvalidationCondition, &MemoryVersion) -> bool;

/// `LifecycleEnv` — the environment `check_contract`/`lifecycle_state` read
/// (§5c.4's `env` parameter): pure function inputs, never globals.
#[derive(Default)]
pub struct LifecycleEnv<'a> {
    /// `dependency → current stamp` (`None` = the dep is unreadable → the
    /// stamp check fails closed).
    pub current_stamp: Option<StampLookup<'a>>,
    /// `(version_id, validator_ref) → last verdict`.
    pub validator_verdict: Option<VerdictLookup<'a>>,
    /// `invalidation_condition → fired?` for conditions the contract check
    /// can't derive (`scope_ended`, `replacement_published`,
    /// `superseded`-as-declared-condition is evaluated by the caller).
    pub condition_fired: Option<ConditionProbe<'a>>,
}

/// `check_contract(contract, at, env)` (§5c.4).
pub fn check_contract(
    contract: &InvalidationContract,
    at: u64,
    version_id: &str,
    version: &MemoryVersion,
    env: &LifecycleEnv<'_>,
) -> ContractCheck {
    let mut fired = Vec::new();
    if let Some(stamp_fn) = env.current_stamp {
        for d in &contract.dependencies {
            match stamp_fn(d) {
                Some(s) if s == d.stamp => {}
                _ => fired.push(d.clone()),
            }
        }
    }
    let fresh = contract
        .freshness
        .as_ref()
        .map(|f| f.holds_at(at))
        .unwrap_or(true);
    let validator = match (&contract.validator_ref, env.validator_verdict) {
        (Some(vr), Some(vf)) => vf(version_id, vr),
        _ => None,
    };
    let mut condition_fired = false;
    if let Some(ic) = &contract.invalidation_condition {
        condition_fired = match ic {
            InvalidationCondition::TtlElapsed => !fresh,
            InvalidationCondition::ValidatorFails => validator == Some(false),
            InvalidationCondition::DependencyChanged => !fired.is_empty(),
            _ => env.condition_fired.map(|f| f(ic, version)).unwrap_or(false),
        };
    }
    ContractCheck {
        stamps_ok: fired.is_empty(),
        fired,
        fresh,
        validator,
        condition_fired,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// lifecycle_state (E1)
// ─────────────────────────────────────────────────────────────────────────────

/// `lifecycle_state(v, at)` — the precedence walk (§5c.4; ADR-0081 d1):
///
/// ```text
/// revoked > superseded > expired > stale_by_dependency > valid > unknown
/// ```
///
/// - `revoked`: a `RevocationRecord` names `v` (the hash-chained lineage
///   revocation) — carries `record`/`reason`.
/// - `superseded`: a `supersedes` edge names `v` as the older member —
///   carries `by`.
/// - `expired`: `v.validity.until ≤ at`, or `check_contract` fires
///   `must_revalidate`-freshness, `validator_fails`, `dependency_changed` or
///   a caller-evaluated condition.
/// - `stale_by_dependency`: a non-`declared_input` justification/dependency
///   is transitively `revoked` or itself `stale_by_dependency` (J1 — the
///   recursion terminates because supersession edges are acyclic by
///   construction).
/// - `valid`: every checkable member of the contract holds.
/// - `unknown`: the contract declares nothing checkable (`is_empty`), or a
///   `stale_ok`-classed freshness elapsed.
pub fn lifecycle_state(store: &MemoryStore, version_id: &str, at: u64) -> LifecycleState {
    let mut visited = BTreeSet::new();
    state_inner(store, version_id, at, &mut visited)
}

fn state_inner(
    store: &MemoryStore,
    version_id: &str,
    at: u64,
    visited: &mut BTreeSet<String>,
) -> LifecycleState {
    if !visited.insert(version_id.to_string()) {
        return LifecycleState::Unknown; // cycle guard (unreachable — edges are acyclic)
    }
    let Some(v) = store.version(version_id) else {
        return LifecycleState::Unknown;
    };
    // revoked — the lineage's hash-chained records.
    if let Some(rec) = store
        .lineage_ref()
        .revocations()
        .iter()
        .find(|r| r.revokes == version_id)
    {
        let reason = store
            .revocations()
            .iter()
            .find(|r| r.version_id == version_id)
            .map(|r| r.reason)
            .unwrap_or(RevocationReason::SourceRevoked);
        return LifecycleState::Revoked {
            record: format!("{}:{}", rec.provenance.authority.as_str(), rec.revokes),
            reason,
        };
    }
    // superseded — an edge names v as the older member.
    if let Some(e) = store.edges().iter().find(|e| e.older == version_id) {
        return LifecycleState::Superseded {
            by: e.newer.clone(),
        };
    }
    // expired — the declared window or a fired checkable member. A
    // `condition`-carrying validity is unevaluatable at C0 → `unknown`
    // (honest, never silently valid).
    if let Some(vd) = &v.validity {
        if let Some(until) = vd.until {
            if at >= until {
                return LifecycleState::Expired {
                    reason: "ttl_elapsed".to_string(),
                };
            }
        }
        if vd.condition.is_some() {
            return LifecycleState::Unknown;
        }
    }
    let env = LifecycleEnv {
        current_stamp: Some(&|d: &DependencyStamp| store.current_stamp(d)),
        validator_verdict: Some(&|vid: &str, vr: &str| store.validator_verdict(vid, vr)),
        condition_fired: Some(&|ic: &InvalidationCondition, vv: &MemoryVersion| {
            store.condition_fired(ic, vv)
        }),
    };
    let check = check_contract(&v.contract, at, version_id, v, &env);
    if !check.stamps_ok {
        return LifecycleState::Expired {
            reason: format!(
                "dependency_changed:{}",
                check
                    .fired
                    .iter()
                    .map(|d| d.ref_.clone())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        };
    }
    if check.validator == Some(false) {
        return LifecycleState::Expired {
            reason: "validator_fails".to_string(),
        };
    }
    if !check.fresh {
        match v.contract.revalidation {
            crate::vocab::Revalidation::MustRevalidate => {
                return LifecycleState::Expired {
                    reason: "ttl_elapsed:must_revalidate".to_string(),
                }
            }
            crate::vocab::Revalidation::StaleOk { .. } => {
                // `stale_ok` admits only as `unknown`, never as `valid`.
                return LifecycleState::Unknown;
            }
            crate::vocab::Revalidation::Never => {}
        }
    }
    if check.condition_fired {
        return LifecycleState::Expired {
            reason: v
                .contract
                .invalidation_condition
                .as_ref()
                .map(|c| c.kind().to_string())
                .unwrap_or_else(|| "condition_fired".to_string()),
        };
    }
    // stale_by_dependency — J1: transitive over justifications (kind ≠
    // declared_input) and contract dependencies of kind `memory_version`.
    let mut stale_inputs = Vec::new();
    for j in &v.justifications {
        if j.kind == crate::memory::JustificationKind::DeclaredInput {
            continue;
        }
        let dep = j.ref_.version_id.clone();
        let st = state_inner(store, &dep, at, visited);
        if matches!(
            st.kind(),
            LifecycleStateKind::Revoked | LifecycleStateKind::StaleByDependency
        ) {
            stale_inputs.push(dep);
        }
    }
    for d in &v.contract.dependencies {
        if d.kind == DependencyKind::MemoryVersion {
            let st = state_inner(store, &d.ref_, at, visited);
            if matches!(
                st.kind(),
                LifecycleStateKind::Revoked | LifecycleStateKind::StaleByDependency
            ) {
                stale_inputs.push(d.ref_.clone());
            }
        }
    }
    if !stale_inputs.is_empty() {
        stale_inputs.sort();
        stale_inputs.dedup();
        return LifecycleState::StaleByDependency {
            revoked_inputs: stale_inputs,
        };
    }
    // valid vs unknown — `unknown` when nothing is checkable: an empty
    // contract AND no declared window/condition (a version carrying only a
    // `from` or an unevaluatable `condition` has no checkable member).
    if v.contract.is_empty()
        && v.validity
            .as_ref()
            .map(|vd| vd.until.is_none() && vd.condition.is_none())
            .unwrap_or(true)
    {
        return LifecycleState::Unknown;
    }
    LifecycleState::Valid
}

/// `item_lifecycle_state` — the context-item half of E1/E2 (§5c.1 I-ORDER):
/// a `Candidate`'s `Validity` evaluated at `at`. A conditioned item is
/// `unknown` at C0 (nothing checkable evaluates the condition — honest).
pub fn item_lifecycle_state(v: &hh_hir::records::Validity, at: u64) -> LifecycleState {
    if at < v.from {
        return LifecycleState::Unknown; // not yet valid — honest unknown
    }
    if let Some(u) = v.until {
        if at >= u {
            return LifecycleState::Expired {
                reason: "ttl_elapsed".to_string(),
            };
        }
    }
    if v.condition.is_some() {
        return LifecycleState::Unknown;
    }
    LifecycleState::Valid
}

// ─────────────────────────────────────────────────────────────────────────────
// filter_for_slot (E2)
// ─────────────────────────────────────────────────────────────────────────────

/// `FilterItem` — the projection `filter_for_slot` consumes: the caller
/// computes `lifecycle_state` per item and hands `(version_id, authority,
/// readers, state_kind, conflict_set_ref, created_at, stale_since_seq)`.
#[derive(Debug, Clone)]
pub struct FilterItem {
    /// The version id.
    pub version_id: String,
    /// The item's authority.
    pub authority: hh_provenance::AuthorityClass,
    /// The item's readers.
    pub readers: ReaderSet,
    /// The lifecycle state kind at `at`.
    pub state: LifecycleStateKind,
    /// `stale_by_dependency` since seq (for `max_stale`).
    pub stale_since: Option<u64>,
    /// The conflict set the item belongs to.
    pub conflict_set_ref: Option<String>,
}

/// `Withheld{version_id, state, reason}` — a withheld entry (the
/// `context.memory.read.withheld[]` member).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Withheld {
    /// The withheld item.
    pub version_id: String,
    /// Its state.
    pub state: LifecycleStateKind,
    /// Why (`validity`, `authority`, `readers`, `conflict`, `mode`).
    pub reason: String,
}

/// `FilterOutcome{admitted[], withheld[]}` — `admitted` preserves input order.
#[derive(Debug, Clone, Default)]
pub struct FilterOutcome {
    /// The surviving ids (input order).
    pub admitted: Vec<String>,
    /// The withheld rows (input order).
    pub withheld: Vec<Withheld>,
}

/// `filter_for_slot(items, policy, slot_min_authority, reader, mode, until_seq)`
/// (§5c.4 E2; §5c.3's "validity → authority → readers" order — the
/// `filter_order_attestation` records exactly this order).
///
/// - validity: `state ∈ policy.admitted_states` (+ `max_stale` bound on
///   `stale_by_dependency`); under `mode = execute`, `superseded`/`revoked`/
///   `expired` are never admissible regardless.
/// - authority: `authority ≥ slot_min_authority`.
/// - readers: `reader ∈ readers` (carried/enforced — the C0 apply is the same
///   predicate; C2 adds the slot-boundary enforcement).
/// - conflict: `conflict_policy` on sets — `withhold_all` withholds every
///   member; `deliver_head_if_resolved_else_withhold` delivers a
///   `superseded`-resolved head, else withholds.
#[allow(clippy::too_many_arguments)]
pub fn filter_for_slot(
    items: &[FilterItem],
    policy: &crate::plan::ValidityPolicy,
    slot_min_authority: hh_provenance::AuthorityClass,
    reader: &str,
    mode: hh_identity::names::ResolveMode,
    at: u64,
    conflicts: &BTreeMap<String, ConflictSet>,
) -> FilterOutcome {
    use hh_identity::names::ResolveMode;
    let mut out = FilterOutcome::default();
    let execute = mode == ResolveMode::Execute;
    for it in items {
        // validity
        let admitted_state = policy.admitted_states.contains(&it.state)
            && !(execute
                && matches!(
                    it.state,
                    LifecycleStateKind::Revoked
                        | LifecycleStateKind::Superseded
                        | LifecycleStateKind::Expired
                ));
        let stale_ok = match (it.state, it.stale_since, policy.max_stale) {
            (LifecycleStateKind::StaleByDependency, Some(since), Some(max)) => {
                at.saturating_sub(since) <= max
            }
            _ => true,
        };
        if !admitted_state || !stale_ok {
            out.withheld.push(Withheld {
                version_id: it.version_id.clone(),
                state: it.state,
                reason: "validity".to_string(),
            });
            continue;
        }
        // authority
        if it.authority < slot_min_authority {
            out.withheld.push(Withheld {
                version_id: it.version_id.clone(),
                state: it.state,
                reason: "authority".to_string(),
            });
            continue;
        }
        // readers
        if !it.readers.admits(reader) {
            out.withheld.push(Withheld {
                version_id: it.version_id.clone(),
                state: it.state,
                reason: "readers".to_string(),
            });
            continue;
        }
        // conflict
        if let Some(cs) = &it.conflict_set_ref {
            match policy.conflict_policy {
                crate::vocab::ConflictPolicy::WithholdAll => {
                    out.withheld.push(Withheld {
                        version_id: it.version_id.clone(),
                        state: it.state,
                        reason: "conflict".to_string(),
                    });
                    continue;
                }
                crate::vocab::ConflictPolicy::DeliverHeadIfResolvedElseWithhold => {
                    if let Some(set) = conflicts.get(cs) {
                        match &set.resolution {
                            ConflictResolution::Superseded { head } if head == &it.version_id => {}
                            _ => {
                                out.withheld.push(Withheld {
                                    version_id: it.version_id.clone(),
                                    state: it.state,
                                    reason: "conflict".to_string(),
                                });
                                continue;
                            }
                        }
                    }
                }
                crate::vocab::ConflictPolicy::DeliverAllAnnotated => {}
            }
        }
        out.admitted.push(it.version_id.clone());
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// write-side checks (E4)
// ─────────────────────────────────────────────────────────────────────────────

/// `LifecycleError` — the E4 refusal set (§5c.4 "LifecycleError").
#[derive(Debug, Clone, PartialEq)]
pub enum LifecycleError {
    /// `AuthorityInsufficient` — the caller's authority is below the target's.
    AuthorityInsufficient {
        /// The caller.
        caller: String,
        /// The target's authority.
        target: String,
    },
    /// `AlreadyRevoked`.
    AlreadyRevoked {
        /// The version.
        version_id: String,
    },
    /// `SupersessionAuthorityInsufficient` — `authority(new) < authority(old)`.
    SupersessionAuthorityInsufficient {
        /// `authority(new)`.
        new_authority: String,
        /// `authority(old)`.
        old_authority: String,
    },
    /// `IllegitimateEndorsement` — a delegate-class caller tried to
    /// endorse/promote/resolve a conflict.
    IllegitimateEndorsement {
        /// The caller's origin tag.
        caller: String,
    },
    /// `EndorserBelowTarget` — `authority(reviewer) < to_authority`.
    EndorserBelowTarget {
        /// `authority(reviewer)`.
        endorser: String,
        /// `to_authority`.
        target: String,
    },
    /// `BasisNotAllowed` — the basis the caller claimed is not on the closed
    /// list (`promotion | approval | supersession`) or applies to the wrong
    /// record kind.
    BasisNotAllowed {
        /// The claimed basis.
        basis: String,
    },
    /// A `MemoryError` from the store half.
    Store(MemoryError),
}

impl std::fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for LifecycleError {}

impl From<MemoryError> for LifecycleError {
    fn from(e: MemoryError) -> Self {
        LifecycleError::Store(e)
    }
}

/// `check_supersede_authority(new_auth, old_auth)` — `authority(new) ≥
/// authority(old)` (the supersession authority rule).
pub fn check_supersede_authority(
    new_authority: hh_provenance::AuthorityClass,
    old_authority: hh_provenance::AuthorityClass,
) -> Result<(), LifecycleError> {
    if new_authority < old_authority {
        return Err(LifecycleError::SupersessionAuthorityInsufficient {
            new_authority: new_authority.as_str().to_string(),
            old_authority: old_authority.as_str().to_string(),
        });
    }
    Ok(())
}

/// `check_revoke_authority(revoker, target)` — the revoker's authority must
/// be `≥` the target's; a delegate-class revoker additionally requires its
/// authority `≥` the *scope's* write ceiling (`delegate` ⇒ delegate-class
/// writers may revoke only `≤ delegate` targets — `AuthorityInsufficient`
/// else).
pub fn check_revoke_authority(
    revoker: &ProvenanceRecord,
    target: &MemoryVersion,
) -> Result<(), LifecycleError> {
    if revoker.authority < target.label.authority {
        return Err(LifecycleError::AuthorityInsufficient {
            caller: revoker.authority.as_str().to_string(),
            target: target.label.authority.as_str().to_string(),
        });
    }
    Ok(())
}

/// `revoke(store, version_id, reason, revoker, replacement, at)` — the E4
/// revoke op: the authority check, `AlreadyRevoked`, then the lineage
/// `revoke` (the hash-chained `RevocationRecord`) plus the
/// `context.memory.invalidated` payload.
pub fn revoke(
    store: &mut MemoryStore,
    version_id: &str,
    reason: RevocationReason,
    revoker: &ProvenanceRecord,
    replacement: Option<String>,
    at_seq: u64,
) -> Result<LineageRevocation, LifecycleError> {
    let target = store
        .version(version_id)
        .ok_or(MemoryError::UnknownVersion {
            version_id: version_id.to_string(),
        })?
        .clone();
    check_revoke_authority(revoker, &target)?;
    if store.lineage_ref().is_revoked(version_id) {
        return Err(LifecycleError::AlreadyRevoked {
            version_id: version_id.to_string(),
        });
    }
    let rec = store.lineage_mut().revoke(
        version_id,
        RecordKind::Memory,
        reason.lineage_reason(),
        revoker.origin.clone(),
        replacement.clone(),
    );
    store.push_revocation(MemoryRevocation {
        version_id: version_id.to_string(),
        reason,
        revoker: revoker.clone(),
        replacement: replacement.clone(),
        at_seq,
    });
    store.emit(
        "context.memory.invalidated",
        crate::events::memory_invalidated_payload(version_id, reason, revoker, replacement),
    );
    store.touch(at_seq);
    Ok(rec)
}

/// `resolve_conflict(store, conflict_set_id, head, basis, resolver)` — a
/// non-delegate caller resolves a set: `supersession` asserts a head,
/// `promotion`/`approval` record the human act. Delegate callers are
/// `IllegitimateEndorsement` (§5c.4).
pub fn resolve_conflict(
    store: &mut MemoryStore,
    conflict_set_id: &str,
    head: &str,
    basis: &str,
    resolver: &ProvenanceRecord,
) -> Result<ConflictSet, LifecycleError> {
    if resolver.origin.is_delegate_class() {
        return Err(LifecycleError::IllegitimateEndorsement {
            caller: resolver.origin.tag().to_string(),
        });
    }
    match basis {
        "supersession" | "promotion" | "approval" => {}
        other => {
            return Err(LifecycleError::BasisNotAllowed {
                basis: other.to_string(),
            })
        }
    }
    let mut set = store
        .conflict(conflict_set_id)
        .cloned()
        .ok_or(MemoryError::UnknownVersion {
            version_id: conflict_set_id.to_string(),
        })?;
    if !set.members.iter().any(|m| m == head) {
        return Err(LifecycleError::Store(MemoryError::UnknownVersion {
            version_id: head.to_string(),
        }));
    }
    // The head must carry authority ≥ every member it supersedes.
    let head_auth =
        store
            .version(head)
            .map(|v| v.label.authority)
            .ok_or(MemoryError::UnknownVersion {
                version_id: head.to_string(),
            })?;
    for m in &set.members {
        if m == head {
            continue;
        }
        let a = store
            .version(m)
            .map(|v| v.label.authority)
            .unwrap_or(hh_provenance::AuthorityClass::Unverified);
        check_supersede_authority(head_auth, a)?;
    }
    set.resolution = ConflictResolution::Superseded {
        head: head.to_string(),
    };
    store.put_conflict(set.clone());
    Ok(set)
}

/// `promote(store, version_id, reviewer, to_scope, to_authority, ctx)` — a
/// human writes a promoted copy (§5c.4; ADR-0082): `to_scope ≥ reviewer`'s
/// scope, `to_authority ≤ reviewer`'s authority, model-origin content is
/// never promoted (`IllegitimateEndorsement`), `no_store` is
/// `BasisNotAllowed`. The new version supersedes the old with
/// `reason = correction` (the promotion edge) and the label-raising is the
/// `security.label.endorsed{basis: promotion}` row the caller appends.
#[allow(clippy::too_many_arguments)]
pub fn promote(
    store: &mut MemoryStore,
    version_id: &str,
    reviewer: &ProvenanceRecord,
    to_scope: PersistenceScope,
    to_authority: hh_provenance::AuthorityClass,
    ctx: &crate::memory::WriteContext,
) -> Result<crate::memory::PutOutcome, LifecycleError> {
    if !matches!(
        reviewer.origin,
        Origin::Human { .. } | Origin::Kernel { .. }
    ) {
        return Err(LifecycleError::IllegitimateEndorsement {
            caller: reviewer.origin.tag().to_string(),
        });
    }
    let old = store
        .version(version_id)
        .ok_or(MemoryError::UnknownVersion {
            version_id: version_id.to_string(),
        })?
        .clone();
    if matches!(old.provenance.origin, Origin::Model { .. }) {
        return Err(LifecycleError::IllegitimateEndorsement {
            caller: "model".to_string(),
        });
    }
    if to_authority > reviewer.authority {
        return Err(LifecycleError::EndorserBelowTarget {
            endorser: reviewer.authority.as_str().to_string(),
            target: to_authority.as_str().to_string(),
        });
    }
    if old.contract.cache_hint == crate::vocab::CacheHint::NoStore {
        return Err(LifecycleError::BasisNotAllowed {
            basis: "promotion on a no_store version".to_string(),
        });
    }
    let draft = crate::memory::MemoryDraft {
        kind: old.kind,
        subject_key: old.subject_key.clone(),
        content: old.content.clone(),
        contract: Some(old.contract.clone()),
        scope: to_scope,
        declared_inputs: old.declared_inputs.clone(),
        justifications: old.justifications.clone(),
        supersedes: Some(crate::memory::SupersedeClaim {
            version_id: version_id.to_string(),
            reason: crate::memory::SupersedeClaimReason::Correction,
        }),
        validity: old.validity.clone(),
        provenance: Some(reviewer.clone()),
        semantic_id: Some(old.semantic_id.clone()),
        validator_endorsed: old.validator_endorsed,
    };
    // The endorsement basis substitutes for the supersession authority
    // check (the reviewer checks above authorize it).
    let outcome = store.put_inner(draft, ctx, true)?;
    store.emit(
        "security.label.endorsed",
        Json::obj([
            ("target_ref", Json::str(version_id)),
            ("new_ref", Json::str(outcome.version.version_id.clone())),
            ("basis", Json::str("promotion")),
            (
                "endorser",
                Json::str(crate::memory::render_origin(&reviewer.origin)),
            ),
        ]),
    );
    Ok(outcome)
}

/// `revalidate(store, version_id, provenance, ctx)` — a fresh version with a
/// fresh contract + `supersedes{edit}` (§5c.4).
pub fn revalidate(
    store: &mut MemoryStore,
    version_id: &str,
    provenance: &ProvenanceRecord,
    contract: Option<InvalidationContract>,
    ctx: &crate::memory::WriteContext,
) -> Result<crate::memory::PutOutcome, LifecycleError> {
    let old = store
        .version(version_id)
        .ok_or(MemoryError::UnknownVersion {
            version_id: version_id.to_string(),
        })?
        .clone();
    let draft = crate::memory::MemoryDraft {
        kind: old.kind,
        subject_key: old.subject_key.clone(),
        content: old.content.clone(),
        contract: contract.or(Some(old.contract.clone())),
        scope: old.scope,
        declared_inputs: old.declared_inputs.clone(),
        justifications: old.justifications.clone(),
        supersedes: Some(crate::memory::SupersedeClaim {
            version_id: version_id.to_string(),
            reason: crate::memory::SupersedeClaimReason::Correction,
        }),
        validity: old.validity.clone(),
        provenance: Some(provenance.clone()),
        semantic_id: Some(old.semantic_id.clone()),
        validator_endorsed: old.validator_endorsed,
    };
    Ok(store.put(draft, ctx)?)
}

// ─────────────────────────────────────────────────────────────────────────────
// dependants / stale_index / memory_usage (the derived views)
// ─────────────────────────────────────────────────────────────────────────────

/// `dependants(store, version_id)` — every version whose justifications or
/// `memory_version` dependencies reference `version_id` (§5c.4).
pub fn dependants(store: &MemoryStore, version_id: &str) -> Vec<String> {
    let mut out = Vec::new();
    for vid in store.version_order() {
        let v = &store.versions()[vid.as_str()];
        let hits = v
            .justifications
            .iter()
            .filter(|j| j.kind != crate::memory::JustificationKind::DeclaredInput)
            .any(|j| j.ref_.version_id == version_id)
            || v.contract
                .dependencies
                .iter()
                .any(|d| d.kind == DependencyKind::MemoryVersion && d.ref_ == version_id);
        if hits {
            out.push(vid.clone());
        }
    }
    out.sort();
    out
}

/// `MemoryStaleIndex` — the derived `MemoryStaleIndex[version_id] →
/// [stale_member_ids]` fold (§5c.4; ADR-0081 d3). Folded at `until_seq`
/// (versions with `created_at ≤ until_seq`), transitive (J1), and stamped as
/// an `hh-ledger` `View` (`lexical_index`/`memory_stale_index`/`memory_usage`
/// all ride the one `View` shape — CC7).
#[derive(Debug, Clone)]
pub struct MemoryStaleIndex {
    /// `version_id → the revoked/stale members it transitively depends on`.
    pub entries: BTreeMap<String, Vec<String>>,
    /// The folded `View` (run-stamped — the store's `applied_seq` bound).
    pub view: View,
}

/// `stale_index(store, until_seq)` — fold + fixpoint (J1 transitive) + `View`
/// stamp (`ViewKind::StaleIndex` — the memory-stale slice of the view table).
pub fn stale_index(store: &MemoryStore, until_seq: u64) -> MemoryStaleIndex {
    let mut entries: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // Fixpoint: repeat until no new stale entries (the graph is finite and
    // acyclic by construction — supersession edges are cycle-checked).
    loop {
        let mut changed = false;
        for vid in store.version_order() {
            let v = &store.versions()[vid.as_str()];
            if v.created_at > until_seq {
                continue;
            }
            let mut members: BTreeSet<String> = BTreeSet::new();
            for j in &v.justifications {
                if j.kind == crate::memory::JustificationKind::DeclaredInput {
                    continue;
                }
                let dep = j.ref_.version_id.clone();
                let dep_stale = store.lineage_ref().is_revoked(&dep)
                    || entries.get(&dep).is_some_and(|e| !e.is_empty());
                if dep_stale {
                    members.insert(dep);
                }
            }
            for d in &v.contract.dependencies {
                if d.kind == DependencyKind::MemoryVersion {
                    let dep_stale = store.lineage_ref().is_revoked(&d.ref_)
                        || entries.get(&d.ref_).is_some_and(|e| !e.is_empty());
                    if dep_stale {
                        members.insert(d.ref_.clone());
                    }
                }
            }
            let cur: Vec<String> = members.into_iter().collect();
            if entries.get(vid) != Some(&cur) {
                entries.insert(vid.clone(), cur);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    entries.retain(|_, v| !v.is_empty());
    let payload = Json::obj([(
        "entries",
        Json::Arr(
            entries
                .iter()
                .map(|(v, ms)| {
                    Json::obj([
                        ("version_id", Json::str(v.clone())),
                        (
                            "stale_members",
                            Json::Arr(ms.iter().map(|m| Json::str(m.clone())).collect()),
                        ),
                    ])
                })
                .collect(),
        ),
    )]);
    let view = View::stamped(
        &store.store_id,
        hh_ledger::views::ViewKind::MemoryStaleIndex,
        Some(until_seq),
        payload,
    );
    MemoryStaleIndex { entries, view }
}

/// `memory_usage` — the usage fold (§5c.3's usage view; R4: reads recorded at
/// delivery complete the version's `last_read_at`).
#[derive(Debug, Clone)]
pub struct MemoryUsageEntry {
    /// The version.
    pub version_id: String,
    /// `created_at`.
    pub created_at: u64,
    /// `last_read_at` — the max recorded read seq.
    pub last_read_at: Option<u64>,
    /// `read_count`.
    pub read_count: u64,
}

/// `memory_usage(store, until_seq)` → entries in write order.
pub fn memory_usage(store: &MemoryStore, until_seq: u64) -> Vec<MemoryUsageEntry> {
    store
        .version_order()
        .iter()
        .filter(|vid| store.versions()[vid.as_str()].created_at <= until_seq)
        .map(|vid| {
            let reads: Vec<u64> = store
                .reads_of(vid)
                .iter()
                .copied()
                .filter(|s| *s <= until_seq)
                .collect();
            MemoryUsageEntry {
                version_id: vid.clone(),
                created_at: store.versions()[vid].created_at,
                last_read_at: reads.iter().copied().max(),
                read_count: reads.len() as u64,
            }
        })
        .collect()
}
