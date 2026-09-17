//! The kernel layout invariants LC-1…LC-9 (ADR-0127 d.3; §5b.4) — checked at
//! `link` for the `Layout` and at `assemble` for each `ContextPlan`; errors,
//! never warnings. The checkers here are pure over the slot/call facts the
//! compiler or the cache projection supplies — the per-call projection wiring
//! (LC-2/4/5/7 against consecutive calls) is the Stage-2 half.
//!
//! LC-1 tier order `static → dynamic → transcript` · LC-2 static-tier bytes
//! identical across consecutive same-purpose calls unless invalidated · LC-3
//! no volatile `CandidateKind` in a `static` slot (`VolatileInStaticTier`) ·
//! LC-4 tool-surface order is a prefix-extension · LC-5 dialect-listed
//! `prefix_affecting[]` parameters enter `static_hash` · LC-6 `purpose ≠ main`
//! never extends the main prefix · LC-7 transcript append-only between
//! invalidating events · LC-8 reserved volatile items render in the first
//! `dynamic` slot · LC-9 markers attach only to dialect-eligible carriers
//! (enforced inside `place_markers`).

use crate::cache::CacheTier;

/// The `volatile_kinds` table (CF-276; LC-3/LC-8): the `CandidateKind`s that may
/// never occupy a `static` slot and must render in the first `dynamic` slot.
/// Growth is a vocabulary bump — a kind absent here in a static slot is an
/// LC-3 miss only when listed (declared data, never a heuristic).
pub const VOLATILE_KINDS: &[&str] = &[
    "budget_reminder",
    "environment_state",
    "current_time",
    "occupancy",
    "turn_counter",
];

/// Whether `kind` is a volatile candidate kind (LC-3).
pub fn is_volatile_kind(kind: &str) -> bool {
    VOLATILE_KINDS.contains(&kind)
}

/// The closed `LayoutViolation` set — one variant per invariant. Each is an
/// error, never a warning (ADR-0127 d.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutViolation {
    /// LC-1 — a slot's tier precedes a previous slot's (`static → dynamic →
    /// transcript` is the only order).
    TierOrder {
        /// The offending slot.
        slot: String,
        /// The tier it declared.
        tier: CacheTier,
        /// The tier of the slot before it.
        after: CacheTier,
    },
    /// LC-2 — `static_hash` changed between consecutive same-purpose calls
    /// with no recorded invalidating event.
    PrefixInstability {
        /// The slot whose bytes changed.
        slot: Option<String>,
        /// A cause, when the projection can name one.
        cause: Option<String>,
    },
    /// LC-3 — a volatile candidate kind occupies a `static` slot.
    VolatileInStaticTier {
        /// The slot.
        slot: String,
        /// The volatile kind found.
        kind: String,
    },
    /// LC-4 — the tool-surface order is not a prefix-extension of the previous
    /// plan's.
    ToolOrderNotPrefixExtension {
        /// The index where the prefix diverged.
        at_index: u32,
        /// The surface that broke the prefix.
        surface: String,
    },
    /// LC-5 — a dialect-listed `prefix_affecting` parameter did not enter
    /// `static_hash`.
    PrefixAffectingUnaccounted {
        /// The parameter.
        parameter: String,
    },
    /// LC-6 — a `purpose ≠ main` call extended the main prefix (used the
    /// `main` affinity key or appended to the main transcript).
    AuxiliaryPrefixExtension {
        /// The offending purpose.
        purpose: String,
    },
    /// LC-7 — the transcript rewrote history between invalidating events.
    TranscriptRewrite {
        /// The index where the transcript diverged from the previous call.
        at_index: u32,
    },
    /// LC-8 — a reserved volatile item rendered outside the first `dynamic`
    /// slot.
    ReservedVolatileMisplaced {
        /// The slot the volatile item rendered in.
        slot: String,
        /// The volatile kind.
        kind: String,
    },
    /// LC-9 — a marker was requested on a carrier the dialect does not admit
    /// (also enforced inside `place_markers` — this is the static check).
    IneligibleCarrier {
        /// The marker position.
        position: String,
        /// The carrier's block kind.
        kind: String,
    },
}

impl LayoutViolation {
    /// The invariant tag (`lc-N`).
    pub fn lc(&self) -> &'static str {
        match self {
            LayoutViolation::TierOrder { .. } => "lc_1",
            LayoutViolation::PrefixInstability { .. } => "lc_2",
            LayoutViolation::VolatileInStaticTier { .. } => "lc_3",
            LayoutViolation::ToolOrderNotPrefixExtension { .. } => "lc_4",
            LayoutViolation::PrefixAffectingUnaccounted { .. } => "lc_5",
            LayoutViolation::AuxiliaryPrefixExtension { .. } => "lc_6",
            LayoutViolation::TranscriptRewrite { .. } => "lc_7",
            LayoutViolation::ReservedVolatileMisplaced { .. } => "lc_8",
            LayoutViolation::IneligibleCarrier { .. } => "lc_9",
        }
    }

    /// The canonical detail string.
    pub fn detail(&self) -> String {
        match self {
            LayoutViolation::TierOrder { slot, tier, after } => format!(
                "slot {slot} declares tier {} after {}",
                tier.as_str(),
                after.as_str()
            ),
            LayoutViolation::PrefixInstability { slot, cause } => format!(
                "static prefix changed (slot {}, cause {})",
                slot.as_deref().unwrap_or("?"),
                cause.as_deref().unwrap_or("unexplained")
            ),
            LayoutViolation::VolatileInStaticTier { slot, kind } => {
                format!("volatile kind {kind} occupies static slot {slot}")
            }
            LayoutViolation::ToolOrderNotPrefixExtension { at_index, surface } => {
                format!("tool order diverges at index {at_index} ({surface})")
            }
            LayoutViolation::PrefixAffectingUnaccounted { parameter } => {
                format!("prefix_affecting parameter {parameter} absent from static_hash")
            }
            LayoutViolation::AuxiliaryPrefixExtension { purpose } => {
                format!("purpose {purpose} extends the main prefix")
            }
            LayoutViolation::TranscriptRewrite { at_index } => {
                format!("transcript rewrote index {at_index} between invalidating events")
            }
            LayoutViolation::ReservedVolatileMisplaced { slot, kind } => {
                format!("reserved volatile {kind} rendered in slot {slot} (first dynamic only)")
            }
            LayoutViolation::IneligibleCarrier { position, kind } => {
                format!("marker at {position} on ineligible carrier kind {kind}")
            }
        }
    }
}

/// One slot's layout facts — what `check_layout` reads at `link`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotFacts {
    /// The slot's id.
    pub slot_id: String,
    /// Its declared `cache_tier`.
    pub cache_tier: CacheTier,
    /// The `CandidateKind`s the slot may carry.
    pub candidate_kinds: Vec<String>,
}

/// `check_layout(slots)` — the link-time invariants LC-1 (order), LC-3 (no
/// volatile in `static`), LC-8 (reserved volatile items render in the *first*
/// `dynamic` slot — never in `static` or a non-initial dynamic slot).
pub fn check_layout(slots: &[SlotFacts]) -> Vec<LayoutViolation> {
    let mut out = Vec::new();
    let mut max_tier: Option<CacheTier> = None;
    let mut first_dynamic_seen = false;
    for slot in slots {
        // LC-1 — tiers are monotone.
        if let Some(prev) = max_tier {
            if slot.cache_tier < prev {
                out.push(LayoutViolation::TierOrder {
                    slot: slot.slot_id.clone(),
                    tier: slot.cache_tier,
                    after: prev,
                });
            }
        }
        max_tier = Some(max_tier.map_or(slot.cache_tier, |m| m.max(slot.cache_tier)));
        // LC-3 — no volatile candidate in a `static` slot.
        for kind in &slot.candidate_kinds {
            if slot.cache_tier == CacheTier::Static && is_volatile_kind(kind) {
                out.push(LayoutViolation::VolatileInStaticTier {
                    slot: slot.slot_id.clone(),
                    kind: kind.clone(),
                });
            }
            // LC-8 — a volatile kind outside the first `dynamic` slot.
            if is_volatile_kind(kind)
                && (slot.cache_tier != CacheTier::Dynamic || first_dynamic_seen)
            {
                out.push(LayoutViolation::ReservedVolatileMisplaced {
                    slot: slot.slot_id.clone(),
                    kind: kind.clone(),
                });
            }
        }
        if slot.cache_tier == CacheTier::Dynamic {
            first_dynamic_seen = true;
        }
    }
    out
}

/// LC-2 — the per-call static-stability check. `prev_hash`/`cur_hash` are the
/// `static_hash`es of two consecutive same-purpose calls; `invalidating`
/// records whether a `cache_invalidating` event lies between them. A change
/// without one is `PrefixInstability` — an error, never a warning.
pub fn check_static_stability(
    prev_hash: &str,
    cur_hash: &str,
    invalidating: bool,
) -> Option<LayoutViolation> {
    if prev_hash == cur_hash || invalidating {
        return None;
    }
    Some(LayoutViolation::PrefixInstability {
        slot: None,
        cause: None,
    })
}

/// LC-4 — the new tool order must be a prefix-extension of the previous
/// plan's (deferred/revealed surfaces append *after* the cached prefix; a
/// divergence is `ToolOrderNotPrefixExtension`).
pub fn check_tool_order_prefix(previous: &[String], current: &[String]) -> Option<LayoutViolation> {
    for (i, prev) in previous.iter().enumerate() {
        match current.get(i) {
            Some(cur) if cur == prev => {}
            Some(cur) => {
                return Some(LayoutViolation::ToolOrderNotPrefixExtension {
                    at_index: i as u32,
                    surface: cur.clone(),
                });
            }
            None => {
                return Some(LayoutViolation::ToolOrderNotPrefixExtension {
                    at_index: i as u32,
                    surface: prev.clone(),
                });
            }
        }
    }
    None
}

/// LC-5 — every dialect-listed `prefix_affecting` parameter the plan sets must
/// be in the `static_hash` input set. Returns one violation per unaccounted
/// parameter.
pub fn check_prefix_affecting(
    prefix_affecting: &[String],
    plan_set: &[String],
    static_hash_inputs: &[String],
) -> Vec<LayoutViolation> {
    plan_set
        .iter()
        .filter(|p| prefix_affecting.contains(p))
        .filter(|p| !static_hash_inputs.contains(p))
        .map(|p| LayoutViolation::PrefixAffectingUnaccounted {
            parameter: p.clone(),
        })
        .collect()
}

/// LC-6 — a `purpose ≠ main` call must carry its own affinity key and never
/// extend the main prefix. `key_purpose` is the purpose the derived key was
/// bound to; a call whose declared purpose differs from its key's purpose (or
/// a non-main call on the main key) is `AuxiliaryPrefixExtension`.
pub fn check_purpose_isolation(
    call_purpose: &crate::vocab::Purpose,
    key_purpose: &crate::vocab::Purpose,
) -> Option<LayoutViolation> {
    if call_purpose == key_purpose {
        return None;
    }
    Some(LayoutViolation::AuxiliaryPrefixExtension {
        purpose: call_purpose.as_str(),
    })
}

/// LC-7 — the transcript is append-only between `cache_invalidating` events
/// (compaction/context-edit). `previous`/`current` are the transcript member
/// digests in order; when no invalidating event intervenes, `current` must
/// extend `previous` — a rewrite is `TranscriptRewrite`.
pub fn check_transcript_append(
    previous: &[String],
    current: &[String],
    invalidating: bool,
) -> Option<LayoutViolation> {
    if invalidating {
        return None;
    }
    for (i, prev) in previous.iter().enumerate() {
        match current.get(i) {
            Some(cur) if cur == prev => {}
            _ => {
                return Some(LayoutViolation::TranscriptRewrite { at_index: i as u32 });
            }
        }
    }
    None
}

/// LC-9 — the static form: every marker position's resolved carrier kind must
/// be in the dialect's `eligible_carriers`. (`place_markers` enforces the
/// same rule dynamically — this is the `link`-time check over a plan's
/// declared carriers.)
pub fn check_marker_carriers(
    markers: &[(crate::cache::MarkerPosition, crate::vocab::BlockKind)],
    eligible: &[crate::vocab::BlockKind],
) -> Vec<LayoutViolation> {
    markers
        .iter()
        .filter(|(_, kind)| !eligible.contains(kind))
        .map(|(pos, kind)| LayoutViolation::IneligibleCarrier {
            position: pos.as_str(),
            kind: kind.as_str().to_string(),
        })
        .collect()
}
