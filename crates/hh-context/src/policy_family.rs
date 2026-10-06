//! §5c.1 **C1 extension** — the `context_policy` variant family (R-2.4.1¹,
//! Stage 5; ADR-0072/0074 (e)): `priority_eviction`, `world_state_diff`,
//! `jit_handles`, `recency_plus_pins`, `retrieval_augmented`, plus the two
//! profile-owned layout members the kernel reads — `demotion_wrappers`
//! (I-RP's sanctioned `external`-slot path for an item admissible nowhere)
//! and `reminder_placement` (where `budget_reminder`/`kernel_notice`
//! candidates land).
//!
//! Every variant is a registered `context_policy` variant — `declare()`
//! returns a [`PolicyDeclaration`] the shared
//! [`crate::policy::check`]/`check_conditioned_rules` registration gate vets
//! (complete `AssumptionDebtRecord`s, the `{ModelProfile, ResourceAccount}`
//! envelope, no model-identity conditioning — AC-R-2.4.1-11/T-LCD-01). A
//! variant's conditioned rules only ever *parameterize* — retiring a rule
//! leaves the declaration's unconditioned arm valid (the retirement test
//! proves the arm).
//!
//! No variant reads or sets `authority`/`validity`/`readers`/`retention` —
//! `Selection` has no such members; the contract is structural.

use std::collections::{BTreeMap, BTreeSet};

use hh_wire::json::Json;

use crate::plan::{Candidate, Layout};
use crate::policy::{
    AdmittedCandidate, ConditionedRule, ContextPolicy, DefaultPolicy, PolicyDeclaration,
    PolicyRequest, PolicyViolation, RegistrationError, Selection, REQUIRED_POLICY_INPUTS,
};
use crate::vocab::{CandidateKind, PriorityClass, Retention};

// ─────────────────────────────────────────────────────────────────────────────
// The variant ids (registry coordinates)
// ─────────────────────────────────────────────────────────────────────────────

/// `priority_eviction` — eviction order purely by declared class + age.
pub const PRIORITY_EVICTION_REF: &str = "context_policy/priority_eviction";
/// `world_state_diff` — keeps the newest `environment_state`; older states
/// evict first (the world refreshes by re-observation — a stale state is
/// dead weight, not history).
pub const WORLD_STATE_DIFF_REF: &str = "context_policy/world_state_diff";
/// `jit_handles` — deliver handle-carriers by reference (just-in-time);
/// `expand_requests` widen them back on demand.
pub const JIT_HANDLES_REF: &str = "context_policy/jit_handles";
/// `recency_plus_pins` — `pins` hold; everything else ranks by recency.
pub const RECENCY_PLUS_PINS_REF: &str = "context_policy/recency_plus_pins";
/// `retrieval_augmented` — memory/procedure candidates order by a declared
/// retrieval rank list (`params.rank_order`).
pub const RETRIEVAL_AUGMENTED_REF: &str = "context_policy/retrieval_augmented";

fn declared() -> Vec<ConditionedRule> {
    Vec::new()
}

fn inputs() -> BTreeSet<String> {
    REQUIRED_POLICY_INPUTS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// `priority_eviction` — declared classes, kernel order, no kind re-derivation
// ─────────────────────────────────────────────────────────────────────────────

/// `priority_eviction` (§5c.1 C1): `select` admits all (layout precedence,
/// ledger order); `evict_order` is strictly `(declared PriorityClass rank,
/// age)` over `optional` candidates — the policy never re-derives a class
/// from kind (a candidate's declared `optional(p)` *is* its class; an
/// `optional` without a declared class is `Commentary`, the evict-first
/// floor). `params.evict_first_kinds[]` (a conditioned rule's payload)
/// hoists named kinds to the head of the eviction order.
#[derive(Debug, Clone, Default)]
pub struct PriorityEviction;

impl ContextPolicy for PriorityEviction {
    fn declare(&self) -> PolicyDeclaration {
        PolicyDeclaration {
            variant_id: PRIORITY_EVICTION_REF.to_string(),
            deterministic: true,
            model_conditioned_rules: declared(),
            required_inputs: inputs(),
        }
    }

    fn priority(&self, candidate: &Candidate) -> PriorityClass {
        // Declared classes carry verbatim — never kind-re-derived.
        match candidate.retention {
            Retention::Optional(p) => p,
            Retention::Required => PriorityClass::Commentary,
        }
    }

    fn select(&self, req: &PolicyRequest<'_>) -> Result<Selection, PolicyViolation> {
        let mut sel = DefaultPolicy::default().select(req)?;
        // `evict_first_kinds` — the conditioned parameter: named kinds lead
        // the eviction order (then class rank, age, id).
        let evict_first: BTreeSet<CandidateKind> = match req.params.get("evict_first_kinds") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(|j| j.as_str().and_then(CandidateKind::parse))
                .collect(),
            _ => BTreeSet::new(),
        };
        let kind_of: BTreeMap<&str, CandidateKind> = req
            .candidates
            .iter()
            .map(|ac| (ac.candidate.candidate_id.as_str(), ac.candidate.kind))
            .collect();
        let class_of: BTreeMap<&str, u8> = req
            .candidates
            .iter()
            .map(|ac| {
                (
                    ac.candidate.candidate_id.as_str(),
                    self.priority(&ac.candidate).eviction_rank() as u8,
                )
            })
            .collect();
        let seq_of: BTreeMap<&str, u64> = req
            .candidates
            .iter()
            .map(|ac| (ac.candidate.candidate_id.as_str(), ac.candidate.source_seq))
            .collect();
        sel.evict_order.sort_by_key(|cid| {
            (
                !evict_first.contains(
                    kind_of
                        .get(cid.as_str())
                        .unwrap_or(&CandidateKind::Observation),
                ),
                class_of.get(cid.as_str()).copied().unwrap_or(0),
                // Older evicts first inside a class.
                u64::MAX - seq_of.get(cid.as_str()).copied().unwrap_or(0),
                cid.clone(),
            )
        });
        Ok(sel)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `world_state_diff` — the newest environment_state lives; stale states evict
// ─────────────────────────────────────────────────────────────────────────────

/// `world_state_diff` (§5c.1 C1; the AC-R-2.4.1-11 arm): `environment_state`
/// candidates supersede by `source_seq` — only the newest earns slot
/// precedence at its own class; older states are ordered to the head of
/// `evict_order` and (when they carry a readable handle) delivered
/// `by_reference` so the fresh state keeps the bytes. Everything else is
/// the default admit-all.
#[derive(Debug, Clone, Default)]
pub struct WorldStateDiff;

impl ContextPolicy for WorldStateDiff {
    fn declare(&self) -> PolicyDeclaration {
        PolicyDeclaration {
            variant_id: WORLD_STATE_DIFF_REF.to_string(),
            deterministic: true,
            model_conditioned_rules: declared(),
            required_inputs: inputs(),
        }
    }

    fn priority(&self, candidate: &Candidate) -> PriorityClass {
        DefaultPolicy::default().priority(candidate)
    }

    fn select(&self, req: &PolicyRequest<'_>) -> Result<Selection, PolicyViolation> {
        let mut sel = DefaultPolicy::default().select(req)?;
        // The newest environment_state source_seq (the live state).
        let newest_env: u64 = req
            .candidates
            .iter()
            .filter(|ac| ac.candidate.kind == CandidateKind::EnvironmentState)
            .map(|ac| ac.candidate.source_seq)
            .max()
            .unwrap_or(0);
        let stale: Vec<String> = req
            .candidates
            .iter()
            .filter(|ac| {
                ac.candidate.kind == CandidateKind::EnvironmentState
                    && ac.candidate.source_seq < newest_env
                    && !ac.candidate.retention.is_required()
            })
            .map(|ac| ac.candidate.candidate_id.clone())
            .collect();
        let stale_set: BTreeSet<&str> = stale.iter().map(String::as_str).collect();
        // Stale states head the eviction order; a stale state with a
        // readable handle delivers `by_reference` (the jit half).
        sel.evict_order
            .sort_by_key(|cid| (!stale_set.contains(cid.as_str()), cid.clone()));
        for ac in req.candidates {
            if stale_set.contains(ac.candidate.candidate_id.as_str())
                && ac.candidate.handle.is_some()
                && !sel.by_reference.contains(&ac.candidate.candidate_id)
            {
                sel.by_reference.push(ac.candidate.candidate_id.clone());
            }
        }
        Ok(sel)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `jit_handles` — by-reference delivery under a declared read capability
// ─────────────────────────────────────────────────────────────────────────────

/// `jit_handles` (§5c.1 C1): any admitted candidate carrying an
/// `OffloadHandle` with a non-empty `read_capability` is delivered
/// `by_reference` unless its `params.always_inline` class holds (a
/// conditioned rule's payload — e.g. `["memory_index"]` keeps indices
/// inline). Offloaded candidates still count against `k` but their token
/// cost is the handle's (the kernel meters delivery, not the policy).
#[derive(Debug, Clone, Default)]
pub struct JitHandles;

impl ContextPolicy for JitHandles {
    fn declare(&self) -> PolicyDeclaration {
        PolicyDeclaration {
            variant_id: JIT_HANDLES_REF.to_string(),
            deterministic: true,
            model_conditioned_rules: declared(),
            required_inputs: inputs(),
        }
    }

    fn priority(&self, candidate: &Candidate) -> PriorityClass {
        DefaultPolicy::default().priority(candidate)
    }

    fn select(&self, req: &PolicyRequest<'_>) -> Result<Selection, PolicyViolation> {
        let mut sel = DefaultPolicy::default().select(req)?;
        let always_inline: BTreeSet<CandidateKind> = match req.params.get("always_inline") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(|j| j.as_str().and_then(CandidateKind::parse))
                .collect(),
            _ => BTreeSet::new(),
        };
        let chosen: BTreeSet<&str> = sel.chosen.iter().map(|(c, _)| c.as_str()).collect();
        for ac in req.candidates {
            let c = &ac.candidate;
            if !chosen.contains(c.candidate_id.as_str()) {
                continue;
            }
            if c.retention.is_required() || always_inline.contains(&c.kind) {
                continue;
            }
            if c.handle
                .as_ref()
                .is_some_and(|h| !h.read_capability.is_empty())
            {
                sel.by_reference.push(c.candidate_id.clone());
            }
        }
        sel.by_reference.sort();
        sel.by_reference.dedup();
        Ok(sel)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `recency_plus_pins` — pins hold, the rest by recency
// ─────────────────────────────────────────────────────────────────────────────

/// `recency_plus_pins{pins[]}` (§5c.1 C1): `params.pins` (candidate ids or
/// kind spellings — a conditioned rule's payload) order first inside their
/// slots; everything else orders newest-first in `order` and oldest-first
/// in `evict_order` (pure recency — classes never enter).
#[derive(Debug, Clone, Default)]
pub struct RecencyPlusPins;

impl ContextPolicy for RecencyPlusPins {
    fn declare(&self) -> PolicyDeclaration {
        PolicyDeclaration {
            variant_id: RECENCY_PLUS_PINS_REF.to_string(),
            deterministic: true,
            model_conditioned_rules: declared(),
            required_inputs: inputs(),
        }
    }

    fn priority(&self, candidate: &Candidate) -> PriorityClass {
        match candidate.retention {
            Retention::Optional(p) => p,
            Retention::Required => PriorityClass::Commentary,
        }
    }

    fn select(&self, req: &PolicyRequest<'_>) -> Result<Selection, PolicyViolation> {
        let mut sel = DefaultPolicy::default().select(req)?;
        let pins: BTreeSet<String> = match req.params.get("pins") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(|j| j.as_str().map(str::to_string))
                .collect(),
            _ => BTreeSet::new(),
        };
        let pinned = |ac: &AdmittedCandidate| {
            pins.contains(&ac.candidate.candidate_id) || pins.contains(ac.candidate.kind.as_str())
        };
        let seq_of: BTreeMap<&str, u64> = req
            .candidates
            .iter()
            .map(|ac| (ac.candidate.candidate_id.as_str(), ac.candidate.source_seq))
            .collect();
        // order — pins first, then newest-first.
        for ids in sel.order.values_mut() {
            ids.sort_by_key(|cid| {
                let is_pin = req
                    .candidates
                    .iter()
                    .find(|ac| ac.candidate.candidate_id == *cid)
                    .is_some_and(pinned);
                (
                    !is_pin,
                    u64::MAX - seq_of.get(cid.as_str()).copied().unwrap_or(0),
                    cid.clone(),
                )
            });
        }
        // evict_order — oldest first, pins last (a pinned candidate is the
        // profile's declared hold).
        sel.evict_order.sort_by_key(|cid| {
            let is_pin = req
                .candidates
                .iter()
                .find(|ac| ac.candidate.candidate_id == *cid)
                .is_some_and(pinned);
            (
                is_pin,
                seq_of.get(cid.as_str()).copied().unwrap_or(0),
                cid.clone(),
            )
        });
        Ok(sel)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `retrieval_augmented` — a declared rank list orders the retrievable kinds
// ─────────────────────────────────────────────────────────────────────────────

/// `retrieval_augmented{rank_order[]}` (§5c.1 C1): `params.rank_order` — the
/// ordered candidate ids a retrieval report produced (the conditioned
/// input) — order `memory`/`memory_index`/`procedure_*`/`artifact_excerpt`
/// candidates inside their slots; unlisted and non-retrievable candidates
/// keep ledger order behind them. The policy never *fetches* — retrieval is
/// the retrieval pipeline's; this variant only consumes its ranking.
#[derive(Debug, Clone, Default)]
pub struct RetrievalAugmented;

impl ContextPolicy for RetrievalAugmented {
    fn declare(&self) -> PolicyDeclaration {
        PolicyDeclaration {
            variant_id: RETRIEVAL_AUGMENTED_REF.to_string(),
            deterministic: true,
            model_conditioned_rules: declared(),
            required_inputs: inputs(),
        }
    }

    fn priority(&self, candidate: &Candidate) -> PriorityClass {
        DefaultPolicy::default().priority(candidate)
    }

    fn select(&self, req: &PolicyRequest<'_>) -> Result<Selection, PolicyViolation> {
        let mut sel = DefaultPolicy::default().select(req)?;
        let rank_order: Vec<String> = match req.params.get("rank_order") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(|j| j.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        };
        if rank_order.is_empty() {
            return Ok(sel);
        }
        let rank_of: BTreeMap<&str, usize> = rank_order
            .iter()
            .enumerate()
            .map(|(i, c)| (c.as_str(), i))
            .collect();
        let retrievable = |ac: &AdmittedCandidate| {
            matches!(
                ac.candidate.kind,
                CandidateKind::Memory
                    | CandidateKind::MemoryIndex
                    | CandidateKind::ProcedureIndex
                    | CandidateKind::ProcedureBody
                    | CandidateKind::ArtifactExcerpt
            )
        };
        let seq_of: BTreeMap<&str, u64> = req
            .candidates
            .iter()
            .map(|ac| (ac.candidate.candidate_id.as_str(), ac.candidate.source_seq))
            .collect();
        for ids in sel.order.values_mut() {
            ids.sort_by_key(|cid| {
                let ac = req
                    .candidates
                    .iter()
                    .find(|ac| ac.candidate.candidate_id == *cid);
                let ranked = ac.is_some_and(retrievable);
                (
                    !ranked,
                    rank_of.get(cid.as_str()).copied().unwrap_or(usize::MAX),
                    seq_of.get(cid.as_str()).copied().unwrap_or(0),
                    cid.clone(),
                )
            });
        }
        Ok(sel)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `demotion_wrappers` — I-RP's sanctioned `external` path (profile-owned)
// ─────────────────────────────────────────────────────────────────────────────

/// `DemotionWrapper{wrapper_id, applies_to[], target_slot, text_ref}` —
/// the `prompt_layout.demotion_wrappers[]` member decoded (§5c.1 I-RP;
/// AC-R-2.4.1-11): when a candidate is admissible in **no** slot on
/// authority/kind grounds, a wrapper whose `applies_to` names its kind
/// admits it to `target_slot` — the quarantine (`external`) slot — and the
/// wrapper's `text_ref` is the demotion annotation `Text` leaf that renders
/// with it. A wrapper can only ever lower presentation — `target_slot`
/// must be `external`-floored and must still satisfy
/// `min_authority ≤ authority(item)` (the wrapper waives *kind* admission,
/// never the floor — "the builder never promotes").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemotionWrapper {
    /// The wrapper's id (registry coordinate).
    pub wrapper_id: String,
    /// The candidate kinds it demotes.
    pub applies_to: Vec<CandidateKind>,
    /// The target slot — `external`-floored only.
    pub target_slot: String,
    /// The demotion annotation `Text` leaf ref.
    pub text_ref: String,
}

/// `decode_demotion_wrappers(params)` — parse the `prompt_layout` member.
/// Each entry `{wrapper_id, applies_to[], target_slot, text_ref}`;
/// `target_slot` must be the `external` quarantine slot (any other target
/// is refused — a demotion wrapper never routes inward).
pub fn decode_demotion_wrappers(params: &Json) -> Result<Vec<DemotionWrapper>, String> {
    let mut out = Vec::new();
    let Some(Json::Arr(entries)) = params.get("demotion_wrappers") else {
        return Ok(out);
    };
    for (i, e) in entries.iter().enumerate() {
        let wrapper_id = e
            .get("wrapper_id")
            .or_else(|| e.get("id"))
            .and_then(Json::as_str)
            .ok_or_else(|| format!("demotion_wrappers[{i}]: missing wrapper_id"))?
            .to_string();
        let target_slot = e
            .get("target_slot")
            .or_else(|| e.get("slot"))
            .and_then(Json::as_str)
            .unwrap_or("external")
            .to_string();
        if target_slot != "external" {
            return Err(format!(
                "demotion_wrappers[{i}] ({wrapper_id}): target_slot must be `external` — a demotion wrapper never routes inward"
            ));
        }
        let text_ref = e
            .get("text_ref")
            .or_else(|| e.get("text"))
            .and_then(Json::as_str)
            .ok_or_else(|| format!("demotion_wrappers[{i}] ({wrapper_id}): missing text_ref"))?
            .to_string();
        let applies_to: Vec<CandidateKind> = match e.get("applies_to") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(|j| j.as_str().and_then(CandidateKind::parse))
                .collect(),
            _ => Vec::new(),
        };
        out.push(DemotionWrapper {
            wrapper_id,
            applies_to,
            target_slot,
            text_ref,
        });
    }
    Ok(out)
}

/// `demotion_slot(candidate, wrappers, layout)` — the wrapper admission:
/// `Some(target_slot)` when a wrapper covers the candidate's kind, the
/// layout carries the `external` target, that slot satisfies the
/// candidate's authority floor, and the candidate isn't already admissible
/// there by kind. The kind-admission waiver is the *whole* point of the
/// wrapper; the authority floor is never waived.
pub fn demotion_slot(
    candidate: &Candidate,
    wrappers: &[DemotionWrapper],
    layout: &Layout,
) -> Option<String> {
    for w in wrappers {
        if !w.applies_to.contains(&candidate.kind) {
            continue;
        }
        let Some(slot) = layout.slot(&w.target_slot) else {
            continue;
        };
        if candidate.label.authority < slot.min_authority {
            continue; // the floor stands — never promotes
        }
        return Some(w.target_slot.clone());
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// `reminder_placement` — where reminder-kind candidates land (profile-owned)
// ─────────────────────────────────────────────────────────────────────────────

/// `reminder_placement{kind → slot}` — the `prompt_layout` member that
/// says where `budget_reminder`/`kernel_notice`/`compaction_reminder`
/// candidates render (§5c.1 "reminder rules"; §5c.2 home (iii)). Decode is
/// additive: absent or malformed entries leave the kernel default
/// (`kernel` slot).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReminderPlacement {
    /// `kind spelling → slot_id`.
    pub placement: BTreeMap<String, String>,
}

/// `decode_reminder_placement(params)` — `{reminder_placement: {kind:
/// slot, …}}`; unknown kind spellings decode verbatim (the member is a
/// placement hint — `kind` spellings outside `CANDIDATE_KINDS` still land
/// on `slot_hint`, never an admission).
pub fn decode_reminder_placement(params: &Json) -> ReminderPlacement {
    let mut placement = BTreeMap::new();
    if let Some(Json::Obj(m)) = params.get("reminder_placement") {
        for (k, v) in m {
            if let Some(s) = v.as_str() {
                placement.insert(k.clone(), s.to_string());
            }
        }
    }
    ReminderPlacement { placement }
}

impl ReminderPlacement {
    /// `slot_for(kind)` — the declared slot for a kind spelling, or the
    /// kernel default (`kernel`).
    pub fn slot_for(&self, kind: &str) -> String {
        self.placement
            .get(kind)
            .cloned()
            .unwrap_or_else(|| "kernel".to_string())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rule-retirement (AC-R-2.4.1-11: "retire rule R yields a valid arm")
// ─────────────────────────────────────────────────────────────────────────────

/// `retire_rule(rules, rule_id)` — drop one conditioned rule (the
/// retirement operation the acceptance criterion names). The residual set
/// is still a declaration arm — [`crate::policy::check_conditioned_rules`]
/// re-vets it (a residual `ModelIdentity` member still refuses; a valid
/// residual arm checks clean).
pub fn retire_rule(
    rules: &[ConditionedRule],
    rule_id: &str,
    required_inputs: &BTreeSet<String>,
) -> Result<Vec<ConditionedRule>, RegistrationError> {
    let residual: Vec<ConditionedRule> = rules
        .iter()
        .filter(|r| r.rule_id != rule_id)
        .cloned()
        .collect();
    crate::policy::check_conditioned_rules(&residual, required_inputs)?;
    Ok(residual)
}
