//! `assemble(request) → ContextPlan` — the kernel's fixed step order
//! (§5c.1 "The builder"; AC-R-2.4.1-5's closed-world accounting):
//!
//! ```text
//! admit → reserve_mandatory → policy.select → enforce_budget
//!       → check_invariants → plan (+ emit)
//! ```
//!
//! - `admit` computes `admissible_slots` per candidate (I-ID, I-RP, I-ORDER,
//!   I-MEDIA, I-NOSYS); an unidentified/unadmissible candidate is an
//!   `Omission`, or `UndeliverableArtifact` under `required` retention.
//! - `reserve_mandatory` sums `reservations` + the omission-item reserve +
//!   the kernel's advisory items.
//! - `policy.select` sees admitted candidates only; the kernel re-checks the
//!   `Selection` ([`policy::check_selection`]) — any widening is
//!   `PolicyViolation` with no plan (AC-R-2.4.1-4).
//! - `enforce_budget` evicts optional items in `evict_order`
//!   (`unfit-alone` first, then `(PriorityClass, age)`); required-only
//!   overflow is `ContextWindowExceeded{required_tokens, cap}` — an error,
//!   never a truncation. `stage01_disposition` maps it to
//!   `CompactionRequired`-as-stop until `evict_oldest` exists (R-2.4.2).
//! - `check_invariants` enforces I-PAIR (paired items co-deliver or
//!   co-omit), I-ATOM (batches are atomic), I-LABEL (the delivered-label
//!   join — handle-only items contribute nothing), and mints the
//!   kernel-authored omission item into the `kernel` slot when
//!   `omitted ≠ ∅` (CC3).
//! - `plan` computes `plan_id = idp(context_plan.1, canonical(body))`
//!   (I-DET; `estimate`+`estimator_ref` are hash inputs — AC-R-2.4.1-8) and
//!   `static_hash` (the LC-2 run half).
//!
//! Emissions: `context.assembled` + one `context.artefact.delivered` per
//! admitted `artefact_id`-bearing item (§5c.1's emission contract).

use std::collections::{BTreeMap, BTreeSet};

use hh_identity::idp::idp_id;
use hh_provenance::label::Label;
use hh_provenance::AuthorityClass;
use hh_wire::json::Json;

use crate::events::{self, EventSink};
use crate::lifecycle;
use crate::plan::{
    link_layout, Candidate, ContextBudget, ContextPlan, CutPoint, DerivedFrom, Layout, LayoutError,
    Omission, PlannedItem, SlotFill, DELIVERY_IDP, OMISSION_ITEM_IDP, PLAN_IDP, STATIC_HASH_IDP,
};
use crate::policy::{
    check_selection, AdmittedCandidate, ContextPolicy, PolicyRequest, PolicyViolation,
};
use crate::vocab::{CandidateState, OmissionReason, Retention};

/// The token reserve the kernel adds for the omission item (§5c.1: "the
/// kernel reserves `omission_item` tokens" — the reserve covers the
/// kernel-authored item when `omitted ≠ ∅`).
pub const OMISSION_ITEM_TOKENS: u64 = 32;

/// `AssemblyRequest` (§5c.1 data model): `{model_call_id, view: Watermark,
/// candidates[], layout, budget, profile, account, policy params,
/// estimator: Ref}`. `view` is the projected watermark the call reads
/// (`StaleView` when behind `min_view_seq`); `at_seq` is the seq the
/// lifecycle/validity checks evaluate at.
#[derive(Debug, Clone)]
pub struct AssemblyRequest {
    /// `model_call_id`.
    pub model_call_id: String,
    /// `view` — `{run_id, seq, view_hash}` the plan's `derived_from` records.
    pub view: DerivedFrom,
    /// `at_seq` — the logical instant I-ORDER evaluates validity at.
    pub at_seq: u64,
    /// `min_view_seq` — when present, `view.seq < min_view_seq` is
    /// `StaleView` (the call requires a fresher view).
    pub min_view_seq: Option<u64>,
    /// `candidates[]`.
    pub candidates: Vec<Candidate>,
    /// `advisories[]` — the kernel's reserved-slot candidates (pending
    /// soft-budget reminders, the current-time item) the kernel forces to
    /// `required` retention and admits into `kernel`-floor slots (§5c.1
    /// "reserves … every pending soft-budget advise reminder … the
    /// current-time item").
    pub advisories: Vec<Candidate>,
    /// `layout`.
    pub layout: Layout,
    /// `budget`.
    pub budget: ContextBudget,
    /// `estimator_ref` — the pinned `EstimationFn` the plan records (I-DET).
    pub estimator_ref: String,
    /// `policy params` — profile data the policy reads (opaque JSON).
    pub policy_params: Json,
}

/// The `assemble` error sum (§5c.1 "errors") — `LayoutError` folds into
/// `InvalidLayout`/`VolatileInStaticTier` (the link check re-runs at
/// assemble for in-memory layouts — LC-1/LC-3 refused at link, re-verified
/// here).
#[derive(Debug, Clone, PartialEq)]
pub enum AssemblyError {
    /// `UndeliverableArtifact` — a `required` candidate carries no compiled
    /// identity or content address (I-ID).
    UndeliverableArtifact {
        /// The candidate.
        candidate_id: String,
    },
    /// `ContextWindowExceeded{required_tokens, cap}` — the required set plus
    /// reservations exceeds `window_cap - margin`. Never a truncation —
    /// `stage01_disposition` maps it to `CompactionRequired`-as-stop.
    ContextWindowExceeded {
        /// The required-only token sum.
        required_tokens: u64,
        /// `window_cap` (the pre-margin ceiling is the spec's `cap`).
        cap: u64,
    },
    /// `PolicyViolation` — the kernel's post-`select` re-check failed.
    PolicyViolation {
        /// What was widened.
        detail: String,
    },
    /// `InvalidLayout` — the link check (`LayoutError::InvalidLayout`).
    InvalidLayout {
        /// The detail.
        detail: String,
    },
    /// `VolatileInStaticTier` — a volatile candidate was placed (or the
    /// layout admits one) into a `static` slot.
    VolatileInStaticTier {
        /// The slot.
        slot_id: String,
        /// The candidate/kind.
        what: String,
    },
    /// `StaleView` — the request's view is behind the call's requirement.
    StaleView {
        /// The view's seq.
        have: u64,
        /// The required seq.
        need: u64,
    },
    /// `MissingProvenance` — an input carried no provenance record (the
    /// candidate schema makes `provenance` mandatory by construction; this
    /// guards the `Option`-carrying fields — a `readers`-narrowed candidate
    /// with an unreadable provenance member).
    MissingProvenance {
        /// The member.
        member: String,
    },
    /// `EstimatorMismatch` — a candidate's `estimate.estimator_ref` differs
    /// from the request's pinned `estimator_ref` (I-DET guard).
    EstimatorMismatch {
        /// The candidate.
        candidate_id: String,
        /// Its estimator.
        got: String,
        /// The pinned estimator.
        want: String,
    },
    /// `PlanInvalid` — `check_invariants` failed (I-PAIR/I-ATOM/I-LABEL).
    PlanInvalid {
        /// The invariant.
        detail: String,
    },
    /// `CausalGuard` — the request's `at_seq` precedes a candidate's
    /// `source_seq` (a candidate from the call's own future — GC-3's "the
    /// caller's own answer" guard).
    CausalGuard {
        /// The candidate.
        candidate_id: String,
    },
    /// `HandleLeaked` (§5g.2 quarantine handles; AC-R-2.8.2-5): a candidate
    /// carrying an `OffloadHandle` was inlined — its offloaded blob rendered
    /// into model-facing `Text` — instead of delivered `handle_only`/
    /// `by_reference` (the excerpt is the only model-facing rendering).
    HandleLeaked {
        /// The candidate whose handle leaked.
        candidate_id: String,
    },
}

impl std::fmt::Display for AssemblyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssemblyError::ContextWindowExceeded {
                required_tokens,
                cap,
            } => write!(
                f,
                "ContextWindowExceeded{{required_tokens:{required_tokens}, cap:{cap}}}"
            ),
            other => write!(f, "{other:?}"),
        }
    }
}

impl std::error::Error for AssemblyError {}

/// `Stage01Outcome` — what the caller does with a `ContextWindowExceeded` at
/// Stage 0/1 (the ticket's "`ContextWindowExceeded → stop`" clause): the
/// `CompactionRequired` signal is carried as a `control.decision` stop row —
/// the `StopReason` vocabulary itself stays closed (the *handler* is the
/// stop, not a new gateway variant).
#[derive(Debug, Clone, PartialEq)]
pub enum Stage01Outcome {
    /// Stop the call; emit `control.decision{kind: stop, reason:
    /// compaction_required}` ([`events::stop_decision_payload`]).
    Stop {
        /// The compaction-required signal.
        signal: CompactionRequired,
    },
}

/// `CompactionRequired{required_tokens, cap}` — the stop signal's payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionRequired {
    /// `required_tokens`.
    pub required_tokens: u64,
    /// `cap`.
    pub cap: u64,
}

/// `stage01_disposition(err)` — `Some(Stop{…})` iff `err` is
/// `ContextWindowExceeded`; `None` otherwise (the caller re-raises other
/// errors as-is).
pub fn stage01_disposition(err: &AssemblyError) -> Option<Stage01Outcome> {
    match err {
        AssemblyError::ContextWindowExceeded {
            required_tokens,
            cap,
        } => Some(Stage01Outcome::Stop {
            signal: CompactionRequired {
                required_tokens: *required_tokens,
                cap: *cap,
            },
        }),
        _ => None,
    }
}

/// `AssemblyOutcome{plan}` — the plan. The payload rows (`context.assembled`
/// + `context.artefact.delivered`) went to the caller's `EventSink` — the
///   caller owns `append`; the kernel computes payloads, never `Event`s
///   (I-PROV). `assembly_ms` is a caller-measured input so `plan_id` stays
///   pure (I-DET).
#[derive(Debug)]
pub struct AssemblyOutcome {
    /// The plan.
    pub plan: ContextPlan,
}

/// `assemble(req, policy)` — the kernel (§5c.1). `layout_ref`/`policy_ref`/
/// `compaction_state`/`assembly_ms` dress the `context.assembled` payload;
/// `sink` receives the emitted rows.
pub fn assemble(
    req: &AssemblyRequest,
    policy: &dyn ContextPolicy,
    layout_ref: &str,
    compaction_state: &str,
    assembly_ms: u64,
    sink: &mut dyn EventSink,
) -> Result<AssemblyOutcome, AssemblyError> {
    // StaleView — the call requires a view at least as fresh.
    if let Some(min) = req.min_view_seq {
        if req.view.seq < min {
            return Err(AssemblyError::StaleView {
                have: req.view.seq,
                need: min,
            });
        }
    }
    // The link check re-runs at assemble for in-memory layouts (LC-1/LC-3).
    link_layout(&req.layout).map_err(|e| match e {
        LayoutError::InvalidLayout { detail } => AssemblyError::InvalidLayout { detail },
        LayoutError::VolatileInStaticTier { slot_id, kind } => {
            AssemblyError::VolatileInStaticTier {
                slot_id,
                what: kind.as_str().to_string(),
            }
        }
    })?;
    // Every candidate's estimate must come from the pinned estimator (I-DET).
    for c in req.candidates.iter().chain(req.advisories.iter()) {
        if c.estimate.estimator_ref != req.estimator_ref {
            return Err(AssemblyError::EstimatorMismatch {
                candidate_id: c.candidate_id.clone(),
                got: c.estimate.estimator_ref.clone(),
                want: req.estimator_ref.clone(),
            });
        }
    }
    // ── admit ────────────────────────────────────────────────────────────
    let mut admitted: Vec<AdmittedCandidate> = Vec::new();
    let mut omitted: Vec<Omission> = Vec::new();
    let mut advisories = req.advisories.clone();
    for c in &mut advisories {
        // The kernel sets `required` on its reserved-slot advisories
        // (I-NOWIDEN: kernel-set, never policy).
        c.retention = Retention::Required;
    }
    for c in advisories.iter().chain(req.candidates.iter()) {
        // GC-3: a candidate sourced from after the call's instant.
        if c.source_seq > req.at_seq && c.source_event.is_some() {
            return Err(AssemblyError::CausalGuard {
                candidate_id: c.candidate_id.clone(),
            });
        }
        // I-ID: no compiled identity, no content address → `unidentified`.
        let identified =
            c.context_item_id.is_some() || c.handle.is_some() || c.artefact_id.is_some();
        if !identified {
            if c.retention.is_required() {
                return Err(AssemblyError::UndeliverableArtifact {
                    candidate_id: c.candidate_id.clone(),
                });
            }
            omitted.push(Omission {
                candidate_id: c.candidate_id.clone(),
                reason: OmissionReason::Unidentified,
                evidence: "I-ID".to_string(),
            });
            continue;
        }
        // I-ORDER: lifecycle state at `at_seq`.
        let state = lifecycle::item_lifecycle_state(&c.validity, req.at_seq);
        // I-RP/I-ORDER admissibility per slot.
        let mut admissible = Vec::new();
        for s in &req.layout.slots {
            if !s.admits.contains(&c.kind) {
                continue;
            }
            if c.label.authority < s.min_authority {
                continue;
            }
            if !s.validity_policy.admitted_states.contains(&state.kind()) {
                continue;
            }
            // A volatile candidate never enters a static slot (LC-3's run
            // half — the declaration half is `link_layout`).
            if c.volatile && s.cache_tier == crate::vocab::CacheTier::Static {
                continue;
            }
            admissible.push(s.slot_id.clone());
        }
        if admissible.is_empty() {
            // `authority` when the kind is unadmitted everywhere or every
            // admitting slot's floor exceeds the label; `validity` when a
            // kind-admitting, floor-satisfying slot exists but the
            // candidate was still inadmissible (lifecycle / readers).
            let reason = if req.layout.slots.iter().all(|s| !s.admits.contains(&c.kind))
                || req
                    .layout
                    .slots
                    .iter()
                    .filter(|s| s.admits.contains(&c.kind))
                    .all(|s| c.label.authority < s.min_authority)
            {
                OmissionReason::Authority
            } else {
                OmissionReason::Validity
            };
            if c.retention.is_required() {
                return Err(AssemblyError::UndeliverableArtifact {
                    candidate_id: c.candidate_id.clone(),
                });
            }
            omitted.push(Omission {
                candidate_id: c.candidate_id.clone(),
                reason,
                evidence: "I-RP/I-ORDER".to_string(),
            });
            continue;
        }
        admitted.push(AdmittedCandidate {
            candidate: c.clone(),
            admissible_slots: admissible,
        });
    }
    // Dedup — identical `context_item_id`s deliver once (I-DUP → `dedup`).
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut deduped = Vec::new();
    for ac in admitted {
        let key = ac
            .candidate
            .context_item_id
            .clone()
            .or_else(|| ac.candidate.artefact_id.clone())
            .unwrap_or_default();
        if !key.is_empty() && !seen.insert(key) {
            omitted.push(Omission {
                candidate_id: ac.candidate.candidate_id.clone(),
                reason: OmissionReason::Dedup,
                evidence: "I-DUP".to_string(),
            });
            continue;
        }
        deduped.push(ac);
    }
    let admitted = deduped;
    // ── reserve_mandatory ────────────────────────────────────────────────
    // The kernel always reserves the omission item's tokens (the reserve is
    // part of `reservations` whether or not omissions exist — §5c.1 "the
    // kernel reserves omission-item tokens").
    let reserved_total = req.budget.reserved_total() + OMISSION_ITEM_TOKENS;
    let budget_remaining = req.budget.effective_cap().saturating_sub(reserved_total);
    // ── policy.select ────────────────────────────────────────────────────
    let preq = PolicyRequest {
        candidates: &admitted,
        layout: &req.layout,
        budget_remaining,
        params: &req.policy_params,
    };
    let mut sel = policy
        .select(&preq)
        .map_err(|PolicyViolation { detail }| AssemblyError::PolicyViolation { detail })?;
    // The kernel re-checks — no-widen is structural AND verified
    // (AC-R-2.4.1-4): a violating selection is PolicyViolation, no plan.
    check_selection(&preq, &sel)
        .map_err(|PolicyViolation { detail }| AssemblyError::PolicyViolation { detail })?;
    // `expand_requests` flip state (the kernel performs the expansion the
    // policy named — the policy only requests).
    let expand: BTreeSet<String> = sel.expand_requests.iter().cloned().collect();
    // ── enforce_budget ───────────────────────────────────────────────────
    let tokens_of: BTreeMap<String, u64> = admitted
        .iter()
        .map(|ac| {
            (
                ac.candidate.candidate_id.clone(),
                ac.candidate.estimate.tokens,
            )
        })
        .collect();
    let retention_of: BTreeMap<String, Retention> = admitted
        .iter()
        .map(|ac| (ac.candidate.candidate_id.clone(), ac.candidate.retention))
        .collect();
    let required_sum: u64 = sel
        .chosen
        .iter()
        .filter(|(cid, _)| {
            retention_of
                .get(cid)
                .map(|r| r.is_required())
                .unwrap_or(false)
        })
        .map(|(cid, _)| tokens_of.get(cid).copied().unwrap_or(0))
        .sum();
    let required_total = required_sum + reserved_total;
    if required_total > req.budget.effective_cap() {
        // Required-only overflow — `ContextWindowExceeded{required_tokens,
        // cap}`; the plan is never emitted.
        return Err(AssemblyError::ContextWindowExceeded {
            required_tokens: required_total,
            cap: req.budget.window_cap,
        });
    }
    // Evict: `unfit-alone` first (an optional item that can never fit even
    // with all other optionals gone), then `evict_order`.
    let mut chosen_ids: BTreeSet<String> = sel.chosen.iter().map(|(c, _)| c.clone()).collect();
    let evictable_budget = req.budget.effective_cap().saturating_sub(required_total);
    let mut evicted = Vec::new();
    for cid in &sel.evict_order {
        if !chosen_ids.contains(cid) {
            continue;
        }
        let t = tokens_of.get(cid).copied().unwrap_or(0);
        if t > evictable_budget {
            chosen_ids.remove(cid);
            evicted.push(cid.clone());
        }
    }
    let mut optional_sum: u64 = chosen_ids
        .iter()
        .filter(|cid| {
            !retention_of
                .get(*cid)
                .map(|r| r.is_required())
                .unwrap_or(true)
        })
        .map(|cid| tokens_of.get(cid).copied().unwrap_or(0))
        .sum();
    for cid in &sel.evict_order {
        if optional_sum <= evictable_budget {
            break;
        }
        if !chosen_ids.contains(cid) {
            continue;
        }
        let t = tokens_of.get(cid).copied().unwrap_or(0);
        chosen_ids.remove(cid);
        optional_sum = optional_sum.saturating_sub(t);
        evicted.push(cid.clone());
    }
    for cid in evicted {
        omitted.push(Omission {
            candidate_id: cid,
            reason: OmissionReason::Budget,
            evidence: "enforce_budget".to_string(),
        });
    }
    // Cardinality — per slot, over `max` items drop the tail (policy order)
    // into `omitted` with `cardinality`.
    for (slot_id, ids) in &mut sel.order {
        let max = req.layout.slot(slot_id).and_then(|s| s.cardinality.max);
        if let Some(max) = max {
            if ids.len() as u64 > max {
                let excess: Vec<String> = ids.split_off(max as usize);
                for cid in excess {
                    chosen_ids.remove(&cid);
                    sel.chosen.retain(|(c, _)| c != &cid);
                    omitted.push(Omission {
                        candidate_id: cid,
                        reason: OmissionReason::Cardinality,
                        evidence: format!("slot {slot_id} max {max}"),
                    });
                }
            }
        }
    }
    sel.chosen.retain(|(c, _)| chosen_ids.contains(c));
    for ids in sel.order.values_mut() {
        ids.retain(|cid| chosen_ids.contains(cid));
    }
    // ── check_invariants ─────────────────────────────────────────────────
    let cand_by_id: BTreeMap<&str, &AdmittedCandidate> = admitted
        .iter()
        .map(|ac| (ac.candidate.candidate_id.as_str(), ac))
        .collect();
    // I-PAIR: paired items co-deliver or co-omit.
    for ac in &admitted {
        if let Some(p) = &ac.candidate.paired_with {
            let a_in = chosen_ids.contains(&ac.candidate.candidate_id);
            let b_in = chosen_ids.contains(p);
            if a_in != b_in {
                return Err(AssemblyError::PlanInvalid {
                    detail: format!(
                        "I-PAIR: {} paired with {} split ({} in, {} out)",
                        ac.candidate.candidate_id, p, a_in, !b_in
                    ),
                });
            }
        }
    }
    // I-ATOM: a batch is retained or evicted whole.
    let mut batches: BTreeMap<&str, Vec<bool>> = BTreeMap::new();
    for ac in &admitted {
        if let Some(b) = &ac.candidate.batch_id {
            batches
                .entry(b.as_str())
                .or_default()
                .push(chosen_ids.contains(&ac.candidate.candidate_id));
        }
    }
    for (b, ins) in &batches {
        if ins.iter().any(|i| *i) && ins.iter().any(|i| !*i) {
            return Err(AssemblyError::PlanInvalid {
                detail: format!("I-ATOM: batch {b} split"),
            });
        }
    }
    // ── plan ─────────────────────────────────────────────────────────────
    let mut slots: Vec<SlotFill> = Vec::new();
    let mut flat_index = 0u64;
    let mut legal_cut_points = Vec::new();
    let mut static_ids = Vec::new();
    let mut delivered_labels: Vec<Label> = Vec::new();
    let mut slot_fills: BTreeMap<String, Vec<PlannedItem>> = BTreeMap::new();
    for (cid, sid) in &sel.chosen {
        if !chosen_ids.contains(cid) {
            continue;
        }
        let ac = cand_by_id[cid.as_str()];
        let by_ref = sel.by_reference.contains(cid);
        let state = if expand.contains(cid) {
            CandidateState::Expanded
        } else {
            ac.candidate.state
        };
        // C2 quarantine (§5g.2; AC-R-2.8.2-5): a handle-carrying candidate
        // delivered inline (not `handle_only`, not `by_reference`) would
        // render the offloaded blob into model-facing `Text` — `HandleLeaked`,
        // a typed refusal, never a silent render. Reveal is a branch/
        // endorsement operation (Stage 4), not an expand flag.
        if ac.candidate.handle.is_some()
            && !by_ref
            && state != crate::vocab::CandidateState::HandleOnly
        {
            return Err(AssemblyError::HandleLeaked {
                candidate_id: cid.clone(),
            });
        }
        let item_id = ac
            .candidate
            .context_item_id
            .clone()
            .or_else(|| ac.candidate.artefact_id.clone())
            .or_else(|| {
                ac.candidate
                    .handle
                    .as_ref()
                    .map(|h| h.content_address.clone())
            })
            .unwrap_or_default();
        slot_fills
            .entry(sid.clone())
            .or_default()
            .push(PlannedItem {
                candidate_id: cid.clone(),
                context_item_id: item_id,
                artefact_id: ac.candidate.artefact_id.clone(),
                delivery_id: idp_id(
                    DELIVERY_IDP,
                    format!("{}:{}:{}", req.model_call_id, sid, cid).as_bytes(),
                ),
                authority: ac.candidate.label.authority,
                label: ac.candidate.label.clone(),
                tokens: ac.candidate.estimate.tokens,
                state,
                delivered_by_reference: by_ref,
                derived_from: None,
            });
    }
    // Order items within each slot per the selection's `order`.
    for (sid, items) in slot_fills.iter_mut() {
        if let Some(order) = sel.order.get(sid) {
            let pos: BTreeMap<&str, usize> = order
                .iter()
                .enumerate()
                .map(|(i, c)| (c.as_str(), i))
                .collect();
            items.sort_by_key(|it| {
                pos.get(it.candidate_id.as_str())
                    .copied()
                    .unwrap_or(usize::MAX)
            });
        }
    }
    // The omission item — kernel-authored, `kernel` slot, `required`
    // (§5c.1 "when omitted ≠ ∅ the plan carries exactly one omission item
    // in the kernel slot"; I-NOWIDEN: the kernel may set required).
    if !omitted.is_empty() {
        let item_id = idp_id(
            OMISSION_ITEM_IDP,
            format!(
                "{}:{}",
                req.model_call_id,
                omitted
                    .iter()
                    .map(|o| o.candidate_id.clone())
                    .collect::<Vec<_>>()
                    .join(",")
            )
            .as_bytes(),
        );
        slot_fills
            .entry("kernel".to_string())
            .or_default()
            .push(PlannedItem {
                candidate_id: format!("{}:omission", req.model_call_id),
                context_item_id: item_id,
                artefact_id: None,
                delivery_id: idp_id(
                    DELIVERY_IDP,
                    format!("{}:kernel:omission", req.model_call_id).as_bytes(),
                ),
                authority: AuthorityClass::Kernel,
                label: Label::at(AuthorityClass::Kernel),
                tokens: OMISSION_ITEM_TOKENS,
                state: CandidateState::Expanded,
                delivered_by_reference: false,
                derived_from: None,
            });
    }
    // Flatten in layout precedence order; compute cut points + static hash +
    // the context label (handle-only items contribute nothing — I-LABEL).
    let mut ordered_slots: Vec<&crate::plan::SlotDeclaration> = req.layout.slots.iter().collect();
    ordered_slots.sort_by_key(|s| s.order.precedence);
    for s in ordered_slots {
        let items = match slot_fills.get(&s.slot_id) {
            Some(i) => i.clone(),
            None => continue,
        };
        legal_cut_points.push(CutPoint {
            before_index: flat_index,
        });
        for it in &items {
            if s.cache_tier == crate::vocab::CacheTier::Static {
                static_ids.push(it.context_item_id.clone());
            }
            if it.state != CandidateState::HandleOnly {
                delivered_labels.push(it.label.clone());
            }
            flat_index += 1;
        }
        // Inside `transcript`: a cut is legal between items where neither is
        // `paired_with` the other.
        if s.slot_id == "transcript" {
            for w in items.windows(2) {
                let paired = cand_by_id
                    .get(w[0].candidate_id.as_str())
                    .and_then(|ac| ac.candidate.paired_with.as_ref())
                    .map(|p| p == &w[1].candidate_id)
                    .unwrap_or(false)
                    || cand_by_id
                        .get(w[1].candidate_id.as_str())
                        .and_then(|ac| ac.candidate.paired_with.as_ref())
                        .map(|p| p == &w[0].candidate_id)
                        .unwrap_or(false);
                if !paired {
                    legal_cut_points.push(CutPoint {
                        before_index: flat_index - 1,
                    });
                }
            }
        }
        slots.push(SlotFill {
            slot_id: s.slot_id.clone(),
            items,
        });
    }
    let context_label = delivered_labels
        .iter()
        .fold(Label::top(), |acc, l| acc.join(l));
    let occupancy_estimate: u64 = slots
        .iter()
        .flat_map(|f| f.items.iter())
        .map(|i| i.tokens)
        .sum();
    let mut plan = ContextPlan {
        plan_id: String::new(),
        model_call_id: req.model_call_id.clone(),
        derived_from: req.view.clone(),
        slots,
        omitted,
        context_label,
        occupancy_estimate,
        legal_cut_points,
        reserved: reserved_total,
        estimator_ref: req.estimator_ref.clone(),
        static_hash: idp_id(STATIC_HASH_IDP, static_ids.join(",").as_bytes()),
    };
    plan.plan_id = idp_id(PLAN_IDP, plan.body_json().to_canonical_string().as_bytes());
    // ── emit ─────────────────────────────────────────────────────────────
    sink.emit(
        "context.assembled",
        events::assembled_payload(
            &plan,
            layout_ref,
            &policy.declare().variant_id,
            compaction_state,
            assembly_ms,
            None,
        ),
    );
    for f in &plan.slots {
        for it in &f.items {
            // §5c.1: "one `context.artefact.delivered{artefact_id,
            // delivery_id, kind, rendering_ref, by_reference}` per admitted
            // harness artifact" — the six artefact kinds (AC-R-2.4.1-10):
            // procedure_index, procedure_body, tool_surface, memory,
            // memory_index, artifact_excerpt. Handle-only deliveries are
            // `by_reference: true`; the artefact id falls back to the
            // context item id for kind-only deliveries (a `memory` body
            // names its version; an index names the index entry).
            let kind = cand_by_id
                .get(it.candidate_id.as_str())
                .map(|ac| ac.candidate.kind)
                .and_then(|k| match k {
                    crate::vocab::CandidateKind::ProcedureIndex => Some("procedure_index"),
                    crate::vocab::CandidateKind::ProcedureBody => Some("procedure_body"),
                    crate::vocab::CandidateKind::ToolSurface => Some("tool_surface"),
                    crate::vocab::CandidateKind::Memory => Some("memory"),
                    crate::vocab::CandidateKind::MemoryIndex => Some("memory_index"),
                    crate::vocab::CandidateKind::ArtifactExcerpt => Some("artifact_excerpt"),
                    _ => None,
                });
            if let Some(kind) = kind {
                let artefact_id = it
                    .artefact_id
                    .clone()
                    .unwrap_or_else(|| it.context_item_id.clone());
                sink.emit(
                    "context.artefact.delivered",
                    events::artefact_delivered_payload(
                        &artefact_id,
                        &it.delivery_id,
                        kind,
                        None,
                        it.delivered_by_reference || it.state == CandidateState::HandleOnly,
                    ),
                );
            }
        }
    }
    Ok(AssemblyOutcome { plan })
}
