//! §5c.2 — the kernel compaction driver (R-2.4.2⁰): `assess` → `propose` →
//! `execute` over a delivered [`ContextPlan`], the closed `CompactionTrigger`
//! sum, the closed `CompactionOp` grammar, the `CompactionRecord` (the durable
//! compaction fact), and the I-FALLBACK ladder whose last rung is the
//! deterministic, model-call-free kernel `evict_oldest`.
//!
//! The invariants the kernel enforces on **every** proposal — whichever variant
//! produced it (in-process or out-of-process):
//!
//! - **I-CUT** — every op boundary is a `legal_cut_points` member (or the view
//!   edge); an `IllegalCut` proposal is refused before any item moves.
//! - **I-REQ** — no op may consume a `required`-retention item.
//! - **I-PAIR / I-ATOM** — an evicted item drags its `paired_with` /
//!   `batch_id` group; an op naming part of an indivisible group is refused.
//! - **I-NOWIDEN** — ops only remove or re-deliver by reference; no surviving
//!   item's label, authority, readers or retention is touched. Derived outputs
//!   carry `derive()`-computed provenance: `Offload`/`Restructure` are kernel
//!   `Projection`s (`label = ⊔ inputs`), `Summarize` is model-produced
//!   (`min(⊔ inputs, delegate)`) — C0 refuses to *execute* a summary without a
//!   declared summariser (`UnresolvedSummarizer`), never silently skipping it.
//! - **I-LABEL** — `context_label_after` is recomputed over the surviving
//!   expanded items (handle-only items contribute nothing — the same rule
//!   `assemble` applies).
//! - **I-LEDGER** — compaction never rewrites durable facts; `forgotten` names
//!   what left the view and `derived_from` lists it.
//! - **I-RECLAIM** — a `hard` requirement is only `applied` when the executed
//!   view frees ≥ `min_reclaim`; less is `ineffective` and the ladder
//!   continues.
//! - **I-BUDGET** — the reserve is the next assembly's job; the record carries
//!   `summariser_usage` (always `None` at C0 — a summariser call would post
//!   `control.budget.consumed{charged_to: subject}` under
//!   `harness_overhead.compaction` before dispatch).
//! - **I-DET** — `evict_oldest` is pure: same view + trigger ⇒ same
//!   `proposal_id` (the idp of the canonical proposal), on any host.
//! - **I-FALLBACK** — `hard`: variant ladder → `proposal.fallback` →
//!   `fallback_variant` → kernel `evict_oldest` → `CompactionImpossible`
//!   (which the caller stops as `context_exhausted`). `soft`: a failed ladder
//!   returns the view unchanged and records the failure.
//!
//! The out-of-process conformance witness is `hh-compact-evict-oldest`
//! (AC-R-2.4.2-1: the op grammar is implementable out-of-process); this module
//! is the kernel's own copy of the same deterministic algorithm — the last
//! fallback rung must not depend on a plugin host.

use std::collections::{BTreeMap, BTreeSet};

use hh_provenance::authority::PersistenceScope;
use hh_provenance::derive::{derive, DerivationInput};
use hh_provenance::label::Label;
use hh_provenance::origin::Origin;
use hh_provenance::record::{DerivationKind, ProvenanceRecord};
use hh_wire::json::Json;

use crate::events::EventSink;
use crate::plan::{Candidate, ContextPlan, PlannedItem};
use crate::vocab::{CandidateState, Retention, PRIORITY_CLASSES};

/// `context_compaction/1` — the idp domain for proposal and record identity.
pub const COMPACTION_IDP: &str = "context_compaction.1";

/// The token cost a kernel-authored omission item claims in the compacted view
/// (a fixed C0 constant — the item's own estimate lands at the next assemble).
pub const OMISSION_ITEM_TOKENS: u64 = 8;

/// The kernel's own C0 variant tag (the registered `hh-compact-evict-oldest`
/// plugin shares the grammar; this is the in-kernel fallback rung).
pub const EVICT_OLDEST_REF: &str = "hh/evict-oldest@1";

/// OQ-197's C0 value — the default `target_fraction` (1/2, parts-per-million).
pub const DEFAULT_TARGET_FRACTION_PPM: u64 = 500_000;

// ─────────────────────────────────────────────────────────────────────────────
// Triggers, requirement, assessment
// ─────────────────────────────────────────────────────────────────────────────

/// `CompactionTrigger` — the closed sum (§5c.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionTrigger {
    /// `occupancy_hard` — the hard occupancy threshold fired.
    OccupancyHard,
    /// `occupancy_soft{rule_id}` — a soft-threshold `HarnessRule` fired.
    OccupancySoft {
        /// The rule that fired.
        rule_id: String,
    },
    /// `overflow_reactive{error_ref}` — a `ContextWindowExceeded` fired.
    OverflowReactive {
        /// The error's ledger ref.
        error_ref: String,
    },
    /// `request_principal` — the principal asked.
    RequestPrincipal,
    /// `request_model{tool_call_id}` — the model asked.
    RequestModel {
        /// The requesting tool call.
        tool_call_id: String,
    },
    /// `schedule{rule_id}` — a scheduled compaction.
    Schedule {
        /// The rule that scheduled it.
        rule_id: String,
    },
    /// `relower{dropped_items}` — a re-lowering pass dropped items.
    Relower {
        /// How many items the re-lower dropped.
        dropped_items: u64,
    },
}

impl CompactionTrigger {
    /// The canonical spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            CompactionTrigger::OccupancyHard => "occupancy_hard",
            CompactionTrigger::OccupancySoft { .. } => "occupancy_soft",
            CompactionTrigger::OverflowReactive { .. } => "overflow_reactive",
            CompactionTrigger::RequestPrincipal => "request_principal",
            CompactionTrigger::RequestModel { .. } => "request_model",
            CompactionTrigger::Schedule { .. } => "schedule",
            CompactionTrigger::Relower { .. } => "relower",
        }
    }

    /// The trigger's default `Requirement` (§5c.2 `hardness`): occupancy_hard,
    /// overflow_reactive and a principal's request are `hard`; the rest `soft`.
    pub fn hardness(&self) -> Requirement {
        match self {
            CompactionTrigger::OccupancyHard
            | CompactionTrigger::OverflowReactive { .. }
            | CompactionTrigger::RequestPrincipal => Requirement::Hard,
            _ => Requirement::Soft,
        }
    }
}

/// `Requirement ∈ {none, soft, hard}` (§5c.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    /// No compaction needed.
    None,
    /// Best-effort — a failed ladder returns the view unchanged.
    Soft,
    /// Must reclaim — a failed ladder is `CompactionImpossible` →
    /// `context_exhausted`.
    Hard,
}

/// `Assessment{requirement, reasons, target_reclaim, min_reclaim}` — `assess`'s
/// output (§5c.2). Pure — equal inputs give equal assessments on any host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assessment {
    /// The requirement.
    pub requirement: Requirement,
    /// Why (the trigger spelling + the measured occupancy).
    pub reasons: Vec<String>,
    /// The reclaim the plan aims for (`occupancy - ⌊cap·fraction⌋`, ≥ needed).
    pub target_reclaim: u64,
    /// The minimum a `hard` compaction must free to be `applied` (I-RECLAIM).
    pub min_reclaim: u64,
}

/// `assess(view, requirement)` — §5c.2. `occupancy`/`cap` come from the plan;
/// `needed` is the floor a reactive trigger must free (an overflow's
/// `required - cap`); `target_fraction_ppm` is OQ-197's knob
/// ([`DEFAULT_TARGET_FRACTION_PPM`] at C0).
pub fn assess(
    trigger: &CompactionTrigger,
    occupancy: u64,
    cap: u64,
    needed: u64,
    target_fraction_ppm: u64,
) -> Assessment {
    let requirement = trigger.hardness();
    let target_occupancy = cap.saturating_mul(target_fraction_ppm) / 1_000_000;
    let to_target = occupancy.saturating_sub(target_occupancy);
    let (target_reclaim, min_reclaim) = match requirement {
        Requirement::Hard => (to_target.max(needed), needed.max(to_target.min(to_target))),
        Requirement::Soft => (to_target.max(needed), 0),
        Requirement::None => (0, 0),
    };
    // A hard trigger whose view already satisfies the target needs only the
    // overflow amount; `min_reclaim` is the reclaim floor, `target` the aim.
    let min_reclaim = match requirement {
        Requirement::Hard => needed.max(to_target.min(target_reclaim)),
        _ => min_reclaim,
    };
    Assessment {
        requirement,
        reasons: vec![
            format!("trigger:{}", trigger.as_str()),
            format!("occupancy:{occupancy}/cap:{cap}"),
        ],
        target_reclaim,
        min_reclaim,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The op grammar (closed sum; ext by dialect bump)
// ─────────────────────────────────────────────────────────────────────────────

/// The placeholder an `Evict` op leaves: `kernel_omission` (the default — one
/// kernel-authored omission item per contiguous forgotten range) or `none`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placeholder {
    /// One omission item per contiguous forgotten range, at kernel authority.
    KernelOmission,
    /// No placeholder.
    None,
}

/// `CompactionOp` — the closed op grammar (§5c.2). `Summarize`/`Restructure`
/// are declared for provenance rule coverage; the C0 `execute` refuses them
/// (`UnresolvedSummarizer` / `UnsupportedOp`) — never silently skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionOp {
    /// `evict{item_ids, placeholder}` — remove the items.
    Evict {
        /// The context_item_ids to evict (a whole indivisible group per
        /// I-PAIR/I-ATOM).
        item_ids: Vec<String>,
        /// What the forgotten range leaves behind.
        placeholder: Placeholder,
    },
    /// `offload{item_ids}` — re-deliver the items `delivered_by_reference`
    /// (the candidate must carry a readable `OffloadHandle`).
    Offload {
        /// The context_item_ids to offload.
        item_ids: Vec<String>,
    },
    /// `summarize{input_ids, insert_at}` — declared at C0; executing it needs a
    /// declared summariser (`UnresolvedSummarizer` otherwise — I-BUDGET).
    Summarize {
        /// The forgotten inputs.
        input_ids: Vec<String>,
        /// The flattened index the summary item occupies.
        insert_at: u64,
    },
    /// `restructure{input_ids, extractor}` — a declared deterministic
    /// extractor (unknown extractors are `UnsupportedOp` at C0).
    Restructure {
        /// The forgotten inputs.
        input_ids: Vec<String>,
        /// The extractor's identity coordinate.
        extractor: String,
    },
}

impl CompactionOp {
    /// The op kind spelling.
    pub fn kind(&self) -> &'static str {
        match self {
            CompactionOp::Evict { .. } => "evict",
            CompactionOp::Offload { .. } => "offload",
            CompactionOp::Summarize { .. } => "summarize",
            CompactionOp::Restructure { .. } => "restructure",
        }
    }

    /// The item ids the op consumes.
    pub fn consumed(&self) -> &[String] {
        match self {
            CompactionOp::Evict { item_ids, .. } => item_ids,
            CompactionOp::Offload { item_ids } => item_ids,
            CompactionOp::Summarize { input_ids, .. } => input_ids,
            CompactionOp::Restructure { input_ids, .. } => input_ids,
        }
    }

    /// The canonical JSON (the `proposal_id` preimage contribution).
    pub fn to_json(&self) -> Json {
        let ids = |v: &[String]| Json::Arr(v.iter().map(|i| Json::str(i.clone())).collect());
        match self {
            CompactionOp::Evict {
                item_ids,
                placeholder,
            } => Json::obj([
                ("kind", Json::str("evict")),
                ("items", ids(item_ids)),
                (
                    "placeholder",
                    Json::str(match placeholder {
                        Placeholder::KernelOmission => "kernel_omission",
                        Placeholder::None => "none",
                    }),
                ),
            ]),
            CompactionOp::Offload { item_ids } => {
                Json::obj([("kind", Json::str("offload")), ("items", ids(item_ids))])
            }
            CompactionOp::Summarize {
                input_ids,
                insert_at,
            } => Json::obj([
                ("kind", Json::str("summarize")),
                ("inputs", ids(input_ids)),
                ("insert_at", Json::Int(*insert_at as i64)),
            ]),
            CompactionOp::Restructure {
                input_ids,
                extractor,
            } => Json::obj([
                ("kind", Json::str("restructure")),
                ("inputs", ids(input_ids)),
                ("extractor", Json::str(extractor.clone())),
            ]),
        }
    }
}

/// `CompactionProposal{proposal_id, ops, fallback, reclaim_estimate}` —
/// `proposal_id = idp(context_compaction.1, canonical(ops ∥ fallback))`
/// (I-DET: identical on any host for identical inputs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionProposal {
    /// The proposal's content address.
    pub proposal_id: String,
    /// The ops.
    pub ops: Vec<CompactionOp>,
    /// `proposal.fallback` — the ops the ladder tries before the variant
    /// ladder continues (I-FALLBACK).
    pub fallback: Option<Vec<CompactionOp>>,
    /// The proposer's reclaim estimate (informational — I-RECLAIM measures the
    /// executed view).
    pub reclaim_estimate: u64,
    /// The proposing variant's identity.
    pub variant_ref: String,
}

impl CompactionProposal {
    /// Mint a proposal (deterministic `proposal_id`).
    pub fn mint(
        variant_ref: &str,
        ops: Vec<CompactionOp>,
        fallback: Option<Vec<CompactionOp>>,
        reclaim_estimate: u64,
    ) -> CompactionProposal {
        let preimage = Json::obj([
            (
                "ops",
                Json::Arr(ops.iter().map(CompactionOp::to_json).collect()),
            ),
            (
                "fallback",
                fallback
                    .as_ref()
                    .map(|f| Json::Arr(f.iter().map(CompactionOp::to_json).collect()))
                    .unwrap_or(Json::Null),
            ),
            ("variant", Json::str(variant_ref.to_string())),
        ])
        .to_canonical_string();
        CompactionProposal {
            proposal_id: hh_identity::idp::idp_id(COMPACTION_IDP, preimage.as_bytes()),
            ops,
            fallback,
            reclaim_estimate,
            variant_ref: variant_ref.to_string(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The kernel view
// ─────────────────────────────────────────────────────────────────────────────

/// One flattened plan item with the candidate data compaction reads
/// (`retention`, `priority`, `source_seq`, pair/batch identity, the handle).
#[derive(Debug, Clone)]
pub struct FlatItem {
    /// The flattened index (slot order, then item order — the
    /// `legal_cut_points` coordinate space).
    pub flat_index: u64,
    /// The slot the item sits in.
    pub slot_id: String,
    /// The item inside the slot.
    pub item: PlannedItem,
    /// The admitted candidate (absent for kernel-authored items — the omission
    /// marker itself is never evictable).
    pub candidate: Option<Candidate>,
}

/// The kernel's compaction input: the delivered plan plus the candidates the
/// plan was assembled from (the retention/pairing/priority facts live on the
/// candidate, not the projected item).
#[derive(Debug)]
pub struct CompactInput<'a> {
    /// The delivered plan.
    pub plan: &'a ContextPlan,
    /// `candidate_id → Candidate` for the plan's items.
    pub candidates: BTreeMap<String, Candidate>,
    /// The context window cap the trigger measured against.
    pub window_cap: u64,
    /// The trigger.
    pub trigger: CompactionTrigger,
    /// The reclaim floor a reactive trigger must free (0 otherwise).
    pub needed: u64,
    /// OQ-197's target fraction (ppm).
    pub target_fraction_ppm: u64,
    /// The persistence scope derivation outputs mint into (the run's scope).
    pub scope: PersistenceScope,
    /// The record's `created_at` (the caller's clock — the driver stays pure).
    pub at: u64,
    /// The run id (emission dressing).
    pub run_id: String,
}

impl<'a> CompactInput<'a> {
    /// Flatten the plan in render order with candidate joins.
    pub fn flattened(&self) -> Vec<FlatItem> {
        let mut out = Vec::new();
        let mut idx = 0u64;
        for slot in &self.plan.slots {
            for item in &slot.items {
                out.push(FlatItem {
                    flat_index: idx,
                    slot_id: slot.slot_id.clone(),
                    item: item.clone(),
                    candidate: self.candidates.get(&item.candidate_id).cloned(),
                });
                idx += 1;
            }
        }
        out
    }

    /// Whether `before_index` is a legal cut (the view edge counts — a range
    /// ending at the tail needs no listed point).
    fn is_legal_boundary(&self, flat: &[FlatItem], before_index: u64) -> bool {
        before_index == 0
            || before_index as usize == flat.len()
            || self
                .plan
                .legal_cut_points
                .iter()
                .any(|c| c.before_index == before_index)
    }
}

/// `CompactError` — the typed failure set (§5c.2). `CompactionImpossible` is
/// the terminal hard-failure the caller maps to `context_exhausted`.
#[derive(Debug, Clone, PartialEq)]
pub enum CompactError {
    /// I-CUT — an op boundary is not a `legal_cut_points` member.
    IllegalCut {
        /// The offending boundary.
        detail: String,
    },
    /// I-REQ — an op named a `required` item.
    RequiredConsumed {
        /// The item.
        item_id: String,
    },
    /// I-PAIR/I-ATOM — an op named part of an indivisible group.
    IndivisibleGroup {
        /// The missing sibling.
        detail: String,
    },
    /// An op named an item not in the view.
    UnknownItem {
        /// The item.
        item_id: String,
    },
    /// `offload` on an item whose candidate carries no readable handle.
    NoHandle {
        /// The item.
        item_id: String,
    },
    /// A `summarize` op with no declared summariser (I-BUDGET — the kernel
    /// refuses, never silently skips).
    UnresolvedSummarizer {
        /// The op.
        detail: String,
    },
    /// An op the C0 executor cannot run (`restructure` without a registered
    /// extractor).
    UnsupportedOp {
        /// The op.
        detail: String,
    },
    /// The terminal failure: every rung of the I-FALLBACK ladder exhausted
    /// without reclaiming the requirement. The caller stops
    /// `context_exhausted{required_tokens, cap}` (§5a.5's stop row).
    CompactionImpossible {
        /// The reclaim still needed.
        required_tokens: u64,
        /// The window cap.
        cap: u64,
    },
}

impl std::fmt::Display for CompactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompactError::CompactionImpossible {
                required_tokens,
                cap,
            } => write!(
                f,
                "CompactionImpossible{{required_tokens:{required_tokens}, cap:{cap}}}"
            ),
            other => write!(f, "{other:?}"),
        }
    }
}
impl std::error::Error for CompactError {}

// ─────────────────────────────────────────────────────────────────────────────
// The variant interface
// ─────────────────────────────────────────────────────────────────────────────

/// A compaction variant (§5c.2). `assess`/`propose` are pure; at C0 the kernel
/// executes the ops itself — `execute` is kernel-side so I-CUT/I-REQ/
/// I-NOWIDEN/I-LEDGER hold structurally for every variant, in- or
/// out-of-process.
pub trait CompactionStrategy {
    /// The variant's identity coordinate (`required_inputs` include the model
    /// profile and resource account — the declaration is a registry matter;
    /// the kernel binds them at dispatch).
    fn variant_ref(&self) -> &str;
    /// `propose(assessment, view)` — pure.
    fn propose(
        &self,
        input: &CompactInput,
        assessment: &Assessment,
    ) -> Result<CompactionProposal, CompactError>;
}

/// The kernel's own `evict_oldest` — the deterministic, model-call-free C0
/// variant and the I-FALLBACK ladder's last rung.
pub struct EvictOldest;

impl CompactionStrategy for EvictOldest {
    fn variant_ref(&self) -> &str {
        EVICT_OLDEST_REF
    }

    fn propose(
        &self,
        input: &CompactInput,
        assessment: &Assessment,
    ) -> Result<CompactionProposal, CompactError> {
        evict_oldest_propose(input, assessment)
    }
}

/// `evict_oldest`'s `propose` — walk the flattened view oldest-first in the
/// kernel `PriorityClass` order (`PRIORITY_CLASSES` — commentary → images →
/// memory → procedure bodies → transcript tail), expanding each pick to its
/// indivisible pair/batch group, keeping only evictions whose forgotten ranges
/// land on legal cut points (I-CUT), until `target_reclaim` is met or the
/// evictable set is exhausted.
pub fn evict_oldest_propose(
    input: &CompactInput,
    assessment: &Assessment,
) -> Result<CompactionProposal, CompactError> {
    let flat = input.flattened();
    if assessment.target_reclaim == 0 {
        return Ok(CompactionProposal::mint(EVICT_OLDEST_REF, vec![], None, 0));
    }
    // Eligible: an admitted candidate with `optional` retention. Kernel-authored
    // items (no candidate) and required items are never evictable (I-REQ).
    let mut order: Vec<usize> = (0..flat.len()).collect();
    let rank = |i: usize| -> (u64, u64, u64) {
        match &flat[i].candidate {
            Some(c) => match c.retention {
                Retention::Required => (u64::MAX, 0, flat[i].flat_index),
                Retention::Optional(p) => (
                    PRIORITY_CLASSES
                        .iter()
                        .position(|k| *k == p)
                        .unwrap_or(PRIORITY_CLASSES.len()) as u64,
                    c.source_seq,
                    flat[i].flat_index,
                ),
            },
            None => (u64::MAX, 0, flat[i].flat_index),
        }
    };
    order.sort_by_key(|i| rank(*i));

    // The indivisible groups: an evicted item drags its pair/batch siblings.
    let group_of = |i: usize| -> BTreeSet<usize> {
        let mut g = BTreeSet::new();
        let Some(c) = &flat[i].candidate else {
            return g;
        };
        g.insert(i);
        for (j, f) in flat.iter().enumerate() {
            if j == i {
                continue;
            }
            let Some(oc) = &f.candidate else { continue };
            if let Some(p) = &c.paired_with {
                if oc.candidate_id == *p
                    || c.candidate_id == *oc.paired_with.clone().unwrap_or_default()
                {
                    g.insert(j);
                }
            }
            if let (Some(a), Some(b)) = (&c.batch_id, &oc.batch_id) {
                if a == b {
                    g.insert(j);
                }
            }
        }
        g
    };

    let mut evicted: BTreeSet<usize> = BTreeSet::new();
    let mut freed = 0u64;
    for i in order {
        // Stop on the *net* estimate — gross minus the omission placeholders
        // the forgotten ranges will cost (I-RECLAIM measures the executed
        // view net; a last-resort rung never stops short of a satisfiable
        // `min_reclaim` while evictable items remain).
        let net =
            freed.saturating_sub(contiguous_runs(&evicted).len() as u64 * OMISSION_ITEM_TOKENS);
        if net >= assessment.target_reclaim {
            break;
        }
        if flat[i].candidate.is_none()
            || matches!(
                flat[i].candidate.as_ref().unwrap().retention,
                Retention::Required
            )
            || evicted.contains(&i)
        {
            continue;
        }
        let group = group_of(i);
        if group.iter().any(|j| {
            evicted.contains(j)
                || flat[*j].candidate.is_none()
                || matches!(
                    flat[*j].candidate.as_ref().unwrap().retention,
                    Retention::Required
                )
        }) {
            // The group drags a required/kernel item — it cannot move.
            continue;
        }
        let tentative: BTreeSet<usize> = evicted.union(&group).cloned().collect();
        // I-CUT: every contiguous forgotten run must open/close on a legal cut.
        if runs_legal(input, &flat, &tentative) {
            for j in &group {
                freed += flat[*j].item.tokens;
            }
            evicted = tentative;
        }
    }

    let ops = if evicted.is_empty() {
        vec![]
    } else {
        vec![CompactionOp::Evict {
            item_ids: evicted
                .iter()
                .map(|i| flat[*i].item.context_item_id.clone())
                .collect(),
            placeholder: Placeholder::KernelOmission,
        }]
    };
    let estimate =
        freed.saturating_sub(contiguous_runs(&evicted).len() as u64 * OMISSION_ITEM_TOKENS);
    Ok(CompactionProposal::mint(
        EVICT_OLDEST_REF,
        ops,
        None,
        estimate,
    ))
}

/// I-CUT for a proposed evicted set: split it into contiguous flattened runs;
/// each run must start and end on a legal boundary.
fn runs_legal(input: &CompactInput, flat: &[FlatItem], evicted: &BTreeSet<usize>) -> bool {
    for (start, end) in contiguous_runs(evicted) {
        if !input.is_legal_boundary(flat, flat[start].flat_index)
            || !input.is_legal_boundary(flat, flat[end].flat_index + 1)
        {
            return false;
        }
    }
    true
}

/// The contiguous `[start, end]` index runs of a sorted index set.
fn contiguous_runs(set: &BTreeSet<usize>) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut cur: Option<(usize, usize)> = None;
    for &i in set {
        match cur {
            None => cur = Some((i, i)),
            Some((s, e)) if i == e + 1 => cur = Some((s, i)),
            Some(r) => {
                runs.push(r);
                cur = Some((i, i));
            }
        }
    }
    if let Some(r) = cur {
        runs.push(r);
    }
    runs
}

// ─────────────────────────────────────────────────────────────────────────────
// Proposal validation + execution (kernel-side)
// ─────────────────────────────────────────────────────────────────────────────

/// `check_proposal` — the kernel's proposal gate (I-CUT/I-REQ/I-PAIR/I-ATOM/
/// I-NOWIDEN). Run on every proposal before `execute`, whatever produced it.
pub fn check_proposal(
    input: &CompactInput,
    proposal: &CompactionProposal,
) -> Result<(), CompactError> {
    let flat = input.flattened();
    let by_item: BTreeMap<&str, &FlatItem> = flat
        .iter()
        .map(|f| (f.item.context_item_id.as_str(), f))
        .collect();

    for op in &proposal.ops {
        // I-REQ + UnknownItem + indivisible-group completeness.
        let mut consumed_idx: BTreeSet<usize> = BTreeSet::new();
        for id in op.consumed() {
            let f = by_item
                .get(id.as_str())
                .ok_or_else(|| CompactError::UnknownItem {
                    item_id: id.clone(),
                })?;
            match &f.candidate {
                None => {
                    return Err(CompactError::RequiredConsumed {
                        item_id: id.clone(),
                    })
                }
                Some(c) if matches!(c.retention, Retention::Required) => {
                    return Err(CompactError::RequiredConsumed {
                        item_id: id.clone(),
                    })
                }
                _ => {}
            }
            consumed_idx.insert(f.flat_index as usize);
        }
        // The op must name every sibling it drags.
        for &i in &consumed_idx.clone() {
            let c = flat[i].candidate.as_ref().unwrap();
            for (j, f) in flat.iter().enumerate() {
                if j == i {
                    continue;
                }
                let Some(oc) = &f.candidate else { continue };
                let paired = c
                    .paired_with
                    .as_ref()
                    .is_some_and(|p| *p == oc.candidate_id)
                    || oc
                        .paired_with
                        .as_ref()
                        .is_some_and(|p| *p == c.candidate_id);
                let batched = matches!((&c.batch_id, &oc.batch_id), (Some(a), Some(b)) if a == b);
                if (paired || batched) && !consumed_idx.contains(&j) {
                    return Err(CompactError::IndivisibleGroup {
                        detail: format!(
                            "op {} names {} but not its sibling {}",
                            op.kind(),
                            c.candidate_id,
                            oc.candidate_id
                        ),
                    });
                }
            }
        }
        // I-CUT for removal ops (offload re-delivers in place — no boundary).
        if matches!(
            op,
            CompactionOp::Evict { .. }
                | CompactionOp::Summarize { .. }
                | CompactionOp::Restructure { .. }
        ) && !runs_legal(input, &flat, &consumed_idx)
        {
            return Err(CompactError::IllegalCut {
                detail: format!("op {} boundary ∉ legal_cut_points", op.kind()),
            });
        }
        // `offload` needs a readable handle on every item.
        if let CompactionOp::Offload { item_ids } = op {
            for id in item_ids {
                let f = by_item[id.as_str()];
                let ok = f
                    .candidate
                    .as_ref()
                    .and_then(|c| c.handle.as_ref())
                    .is_some_and(|h| !h.read_capability.is_empty());
                if !ok {
                    return Err(CompactError::NoHandle {
                        item_id: id.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

/// The compacted view — the surviving `SlotFill`s with omission items placed
/// where their forgotten ranges sat. The next `assemble` reads this view
/// (I-LEDGER: nothing durable moved; `forgotten` names what left).
#[derive(Debug, Clone)]
pub struct CompactedView {
    /// The post-compaction slot fills.
    pub slots: Vec<crate::plan::SlotFill>,
    /// The recomputed `context_label` (I-LABEL).
    pub context_label_after: Label,
    /// The tokens the executed ops freed (net of placeholders).
    pub tokens_freed: u64,
    /// The forgotten context_item_ids.
    pub forgotten: Vec<String>,
    /// The omission items the placeholders minted (`delivered` rows emit).
    pub omission_items: Vec<PlannedItem>,
}

/// `execute(proposal, input)` — kernel-side application (C0). `evict` removes
/// and leaves the placeholder; `offload` flips delivery to `by_reference`;
/// `summarize`/`restructure` are typed refusals (no summariser/extractor is
/// registered at C0 — I-BUDGET, never a silent skip).
pub fn execute(
    input: &CompactInput,
    proposal: &CompactionProposal,
) -> Result<CompactedView, CompactError> {
    check_proposal(input, proposal)?;
    for op in &proposal.ops {
        match op {
            CompactionOp::Summarize { .. } => {
                return Err(CompactError::UnresolvedSummarizer {
                    detail: "no summariser declared at C0".into(),
                })
            }
            CompactionOp::Restructure { extractor, .. } => {
                return Err(CompactError::UnsupportedOp {
                    detail: format!("extractor {extractor} not registered at C0"),
                })
            }
            _ => {}
        }
    }

    let flat = input.flattened();
    let by_item: BTreeMap<&str, &FlatItem> = flat
        .iter()
        .map(|f| (f.item.context_item_id.as_str(), f))
        .collect();

    let mut evict_set: BTreeSet<usize> = BTreeSet::new();
    let mut offload_set: BTreeSet<usize> = BTreeSet::new();
    let mut want_placeholder = false;
    for op in &proposal.ops {
        for id in op.consumed() {
            let f = by_item[id.as_str()];
            match op {
                CompactionOp::Evict { placeholder, .. } => {
                    evict_set.insert(f.flat_index as usize);
                    want_placeholder |= matches!(placeholder, Placeholder::KernelOmission);
                }
                CompactionOp::Offload { .. } => {
                    offload_set.insert(f.flat_index as usize);
                }
                _ => {}
            }
        }
    }

    // Rebuild the slot fills: evicted items leave; offloaded items flip to
    // by-reference; a `kernel_omission` placeholder lands where each forgotten
    // contiguous run began (one item per range, kernel authority — I-LABEL:
    // it joins at `kernel`, which never lowers the label).
    let mut slots: Vec<crate::plan::SlotFill> = Vec::new();
    let mut omission_items = Vec::new();
    let mut forgotten = Vec::new();
    let runs = contiguous_runs(&evict_set);
    let run_start: BTreeMap<usize, usize> = runs.iter().map(|(s, e)| (*s, *e)).collect();
    let mut placeholder_count = 0u64;
    for slot in &input.plan.slots {
        let mut items = Vec::new();
        for item in &slot.items {
            let f = flat
                .iter()
                .find(|f| f.item.delivery_id == item.delivery_id)
                .expect("plan items flatten 1:1");
            let idx = f.flat_index as usize;
            if evict_set.contains(&idx) {
                forgotten.push(item.context_item_id.clone());
                if want_placeholder && run_start.contains_key(&idx) {
                    placeholder_count += 1;
                    let omission = PlannedItem {
                        candidate_id: format!("kernel-omission-{}", f.flat_index),
                        context_item_id: hh_identity::idp::idp_id(
                            crate::plan::OMISSION_ITEM_IDP,
                            format!("compact:{}:{}", input.run_id, f.flat_index).as_bytes(),
                        ),
                        artefact_id: None,
                        delivery_id: hh_identity::idp::idp_id(
                            crate::plan::DELIVERY_IDP,
                            format!("compact-del:{}:{}", input.run_id, f.flat_index).as_bytes(),
                        ),
                        authority: hh_provenance::authority::AuthorityClass::Kernel,
                        label: Label::at(hh_provenance::authority::AuthorityClass::Kernel),
                        tokens: OMISSION_ITEM_TOKENS,
                        state: CandidateState::Expanded,
                        delivered_by_reference: false,
                        derived_from: None,
                    };
                    items.push(omission.clone());
                    omission_items.push(omission);
                }
                continue;
            }
            if offload_set.contains(&idx) {
                let mut it = item.clone();
                it.delivered_by_reference = true;
                it.state = CandidateState::HandleOnly;
                items.push(it);
                continue;
            }
            items.push(item.clone());
        }
        slots.push(crate::plan::SlotFill {
            slot_id: slot.slot_id.clone(),
            items,
        });
    }

    // I-LABEL — recompute over the surviving expanded items.
    let context_label_after = slots
        .iter()
        .flat_map(|s| s.items.iter())
        .filter(|i| i.state == CandidateState::Expanded)
        .fold(Label::top(), |acc, i| acc.join(&i.label));

    let evicted_tokens: u64 = evict_set.iter().map(|i| flat[*i].item.tokens).sum();
    let offloaded_tokens: u64 = offload_set.iter().map(|i| flat[*i].item.tokens).sum();
    let tokens_freed = (evicted_tokens + offloaded_tokens)
        .saturating_sub(placeholder_count * OMISSION_ITEM_TOKENS);

    Ok(CompactedView {
        slots,
        context_label_after,
        tokens_freed,
        forgotten,
        omission_items,
    })
}

/// `op_output_record` — the provenance rule per op kind (AC-R-2.4.2-2):
/// `offload`/`restructure` are kernel `Projection`s (`label = ⊔ inputs`),
/// `summarize` is model-produced (`min(⊔ inputs, delegate)`), the compaction
/// record itself derives `kind = compaction` at `kernel` (deterministic). The
/// inputs are the forgotten items' provenance records.
pub fn op_output_record(
    op_kind: &str,
    inputs: &[DerivationInput],
    scope: PersistenceScope,
    at: u64,
) -> Result<ProvenanceRecord, hh_provenance::record::ProvenanceError> {
    match op_kind {
        "offload" | "restructure" => derive(
            DerivationKind::Projection,
            inputs,
            Origin::kernel("kernel:compact"),
            true,
            scope,
            at,
        ),
        "summarize" => derive(
            DerivationKind::Summary,
            inputs,
            Origin::Model {
                model_ref: "unbound".into(),
                run_ref: "unbound".into(),
                response_id: "summarizer".into(),
            },
            false,
            scope,
            at,
        ),
        "evict" | "compaction" => derive(
            DerivationKind::Compaction,
            inputs,
            Origin::kernel("kernel:compact"),
            true,
            scope,
            at,
        ),
        _ => derive(
            DerivationKind::Compaction,
            inputs,
            Origin::kernel("kernel:compact"),
            true,
            scope,
            at,
        ),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The record + the driver
// ─────────────────────────────────────────────────────────────────────────────

/// `CompactionStatus ∈ {applied, ineffective, failed}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionStatus {
    /// The view changed and the requirement was met.
    Applied,
    /// The ops executed but the reclaim fell short (the ladder continues).
    Ineffective,
    /// The ladder exhausted without applying.
    Failed,
}

/// `CompactionRecord{compaction_id, variant_ref, trigger, requirement, status,
/// ops_applied, forgotten, summary_ref, context_label_after, tokens_freed,
/// derived_from, summariser_usage, duration_ms, pipeline_index,
/// fallback_variant, provenance}` — the durable compaction fact the
/// `context.compaction.completed` row carries (§5c.2).
#[derive(Debug, Clone)]
pub struct CompactionRecord {
    /// The record's content address.
    pub compaction_id: String,
    /// The variant whose proposal applied (or the last attempted).
    pub variant_ref: String,
    /// The trigger.
    pub trigger: CompactionTrigger,
    /// The requirement.
    pub requirement: Requirement,
    /// The status.
    pub status: CompactionStatus,
    /// The ops that ran.
    pub ops_applied: Vec<CompactionOp>,
    /// The forgotten `context_item_id`s (or a contiguous range — C0 lists ids).
    pub forgotten: Vec<String>,
    /// The summary/omission item refs this compaction minted.
    pub summary_ref: Option<String>,
    /// I-LABEL — the recomputed label.
    pub context_label_after: Label,
    /// Net tokens freed.
    pub tokens_freed: u64,
    /// `derived_from` — every forgotten item id (I-LEDGER).
    pub derived_from: Vec<String>,
    /// `summariser_usage` — always `None` at C0 (a summariser call would post
    /// `control.budget.consumed{charged_to: subject, attribution:
    /// harness_overhead.compaction}` before dispatch — I-BUDGET).
    pub summariser_usage: Option<Json>,
    /// `duration_ms{measured_at}` — the caller's clock (the driver is pure).
    pub duration_ms: u64,
    /// The ladder position that applied (0 = the first variant).
    pub pipeline_index: u64,
    /// The `fallback_variant` that ran, when the primary couldn't.
    pub fallback_variant: Option<String>,
    /// The record's provenance — `derive(compaction, forgotten, kernel)`.
    pub provenance: Option<ProvenanceRecord>,
}

/// The `compact` outcome.
#[derive(Debug)]
pub struct CompactOutcome {
    /// Whether the view changed.
    pub applied: bool,
    /// The record.
    pub record: CompactionRecord,
    /// The compacted view (unchanged when `applied = false`).
    pub view: CompactedView,
}

/// `compact(input, variants, sink)` — the kernel driver. Runs `assess`, emits
/// `context.compaction.started`, walks the variant ladder, applies the first
/// proposal whose executed view satisfies I-RECLAIM, and falls back per
/// I-FALLBACK: `proposal.fallback` ops → remaining variants → kernel
/// `evict_oldest` → `CompactionImpossible` (hard) / unchanged view (soft).
pub fn compact(
    input: &CompactInput,
    variants: &[&dyn CompactionStrategy],
    sink: &mut dyn EventSink,
) -> Result<CompactOutcome, CompactError> {
    let assessment = assess(
        &input.trigger,
        input.plan.occupancy_estimate,
        input.window_cap,
        input.needed,
        input.target_fraction_ppm,
    );
    let occupancy_before = input.plan.occupancy_estimate;

    if assessment.requirement == Requirement::None
        || (assessment.target_reclaim == 0 && assessment.min_reclaim == 0)
    {
        let view = execute(
            input,
            &CompactionProposal::mint(EVICT_OLDEST_REF, vec![], None, 0),
        )?;
        let record = mint_record(
            input,
            &assessment,
            CompactionStatus::Applied,
            EVICT_OLDEST_REF,
            &[],
            &view,
            0,
            None,
        );
        return Ok(CompactOutcome {
            applied: false,
            record,
            view,
        });
    }

    // The ladder: declared variants in order, then the kernel evict_oldest as
    // the always-present last rung (I-FALLBACK).
    let kernel_fallback = EvictOldest;
    let mut ladder: Vec<&dyn CompactionStrategy> = variants.to_vec();
    if !ladder.iter().any(|v| v.variant_ref() == EVICT_OLDEST_REF) {
        ladder.push(&kernel_fallback);
    }

    sink.emit(
        "context.compaction.started",
        crate::events::compaction_started(&input.trigger, occupancy_before, &assessment),
    );

    let mut last_err: Option<CompactError> = None;
    let mut fallback_ops: Option<Vec<CompactionOp>> = None;
    let mut fallback_variant: Option<String> = None;
    let mut seen_evict_oldest = false;

    for (i, variant) in ladder.iter().enumerate() {
        // Try the pending `proposal.fallback` ops before the next variant
        // (I-FALLBACK's first rung after a failure).
        if let Some(ops) = fallback_ops.take() {
            let p = CompactionProposal::mint(
                fallback_variant.as_deref().unwrap_or("fallback"),
                ops,
                None,
                0,
            );
            match try_proposal(input, &p, &assessment) {
                Ok((view, status)) if status == CompactionStatus::Applied => {
                    let record = mint_record(
                        input,
                        &assessment,
                        status,
                        &p.variant_ref,
                        &p.ops,
                        &view,
                        i as u64,
                        fallback_variant.clone(),
                    );
                    emit_completed(input, sink, &record);
                    return Ok(CompactOutcome {
                        applied: true,
                        record,
                        view,
                    });
                }
                Ok(_) | Err(_) => {}
            }
        }

        let proposal = match variant.propose(input, &assessment) {
            Ok(p) => p,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        fallback_ops = proposal.fallback.clone();
        fallback_variant = Some(variant.variant_ref().to_string());

        match try_proposal(input, &proposal, &assessment) {
            Ok((view, CompactionStatus::Applied)) => {
                let record = mint_record(
                    input,
                    &assessment,
                    CompactionStatus::Applied,
                    variant.variant_ref(),
                    &proposal.ops,
                    &view,
                    i as u64,
                    None,
                );
                emit_completed(input, sink, &record);
                return Ok(CompactOutcome {
                    applied: true,
                    record,
                    view,
                });
            }
            Ok((_, CompactionStatus::Ineffective)) => {
                last_err = None; // ineffective — the ladder continues
            }
            Ok(_) => {}
            Err(e) => last_err = Some(e),
        }
        seen_evict_oldest |= variant.variant_ref() == EVICT_OLDEST_REF;
    }
    let _ = seen_evict_oldest;
    let _ = &last_err;

    // The ladder exhausted. Record the failure; hard → CompactionImpossible.
    let flat = input.flattened();
    let unchanged = CompactedView {
        slots: input.plan.slots.clone(),
        context_label_after: flat
            .iter()
            .filter(|f| f.item.state == CandidateState::Expanded)
            .fold(Label::top(), |acc, f| acc.join(&f.item.label)),
        tokens_freed: 0,
        forgotten: vec![],
        omission_items: vec![],
    };
    let record = mint_record(
        input,
        &assessment,
        CompactionStatus::Failed,
        ladder
            .last()
            .map(|v| v.variant_ref())
            .unwrap_or(EVICT_OLDEST_REF),
        &[],
        &unchanged,
        ladder.len() as u64,
        fallback_variant,
    );
    emit_completed(input, sink, &record);
    match assessment.requirement {
        Requirement::Hard => Err(CompactError::CompactionImpossible {
            required_tokens: assessment.min_reclaim.max(assessment.target_reclaim),
            cap: input.window_cap,
        }),
        _ => Ok(CompactOutcome {
            applied: false,
            record,
            view: unchanged,
        }),
    }
}

/// Run `check_proposal` → `execute` → I-RECLAIM on one proposal.
fn try_proposal(
    input: &CompactInput,
    proposal: &CompactionProposal,
    assessment: &Assessment,
) -> Result<(CompactedView, CompactionStatus), CompactError> {
    let view = execute(input, proposal)?;
    let ok = match assessment.requirement {
        Requirement::Hard => view.tokens_freed >= assessment.min_reclaim,
        _ => view.tokens_freed > 0 || proposal.ops.is_empty(),
    };
    Ok((
        view,
        if ok {
            CompactionStatus::Applied
        } else {
            CompactionStatus::Ineffective
        },
    ))
}

/// Mint the record: the provenance is `derive(compaction, forgotten, kernel)`
/// (I-LEDGER's `derived_from` lists every forgotten id); `compaction_id` is
/// the idp of the canonical record minus its own id.
#[allow(clippy::too_many_arguments)]
fn mint_record(
    input: &CompactInput,
    assessment: &Assessment,
    status: CompactionStatus,
    variant_ref: &str,
    ops: &[CompactionOp],
    view: &CompactedView,
    pipeline_index: u64,
    fallback_variant: Option<String>,
) -> CompactionRecord {
    let inputs: Vec<DerivationInput> = view
        .forgotten
        .iter()
        .filter_map(|id| {
            input
                .plan
                .slots
                .iter()
                .flat_map(|s| s.items.iter())
                .find(|i| i.context_item_id == *id)
                .and_then(|i| {
                    input
                        .candidates
                        .get(&i.candidate_id)
                        .map(|c| DerivationInput {
                            input_ref: i.context_item_id.clone(),
                            record: c.provenance.clone(),
                        })
                })
        })
        .collect();
    let provenance = if inputs.is_empty() {
        None
    } else {
        derive(
            DerivationKind::Compaction,
            &inputs,
            Origin::kernel("kernel:compact"),
            true,
            input.scope,
            input.at,
        )
        .ok()
    };
    let preimage = Json::obj([
        ("variant", Json::str(variant_ref.to_string())),
        ("trigger", Json::str(input.trigger.as_str())),
        (
            "status",
            Json::str(match status {
                CompactionStatus::Applied => "applied",
                CompactionStatus::Ineffective => "ineffective",
                CompactionStatus::Failed => "failed",
            }),
        ),
        (
            "forgotten",
            Json::Arr(
                view.forgotten
                    .iter()
                    .map(|f| Json::str(f.clone()))
                    .collect(),
            ),
        ),
        (
            "ops",
            Json::Arr(ops.iter().map(CompactionOp::to_json).collect()),
        ),
        ("pipeline_index", Json::Int(pipeline_index as i64)),
    ])
    .to_canonical_string();
    CompactionRecord {
        compaction_id: hh_identity::idp::idp_id(COMPACTION_IDP, preimage.as_bytes()),
        variant_ref: variant_ref.to_string(),
        trigger: input.trigger.clone(),
        requirement: assessment.requirement,
        status,
        ops_applied: ops.to_vec(),
        forgotten: view.forgotten.clone(),
        summary_ref: view
            .omission_items
            .first()
            .map(|o| o.context_item_id.clone()),
        context_label_after: view.context_label_after.clone(),
        tokens_freed: view.tokens_freed,
        derived_from: view.forgotten.clone(),
        summariser_usage: None,
        duration_ms: 0,
        pipeline_index,
        fallback_variant,
        provenance,
    }
}

fn emit_completed(_input: &CompactInput, sink: &mut dyn EventSink, record: &CompactionRecord) {
    sink.emit(
        "context.compaction.completed",
        crate::events::compaction_completed(record),
    );
}
