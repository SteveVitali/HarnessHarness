//! The §5c Stage-3 context/memory metric folds (S3.8; AC-R-2.4.1-12,
//! AC-R-2.4.2-8/-9, AC-R-2.4.3-9, AC-R-2.4.4-11/-12).
//!
//! Every fold here is deterministic over [`LedgerFacts`] — the same rows
//! produce the same values on every host (V-DET). The emitted set covers:
//!
//! - **overhead** — `harness_overhead.tokens` /
//!   `harness_overhead.model_calls`: tokens and model calls charged to the
//!   harness (compaction summaries, repair probes, assembly probes — never
//!   the subject's task calls; AC-R-2.4.2-7's accounting split).
//! - **compaction outcome** — `compaction.applied`, `compaction.tokens_freed`,
//!   `compaction.model_calls` (the summariser charge — 0 at C0).
//! - **trace-predicate oracles** — `reacquisition_count` (an item forgotten
//!   by a compaction is retrieved again later), `repeated_action_count`
//!   (the same `(capability, canonical args)` completed twice after its
//!   earlier result was forgotten), `recall_probe_hit_rate` (probe
//!   assemblies — calls whose `cache.purpose` is `probe` — that still
//!   contain a needed forgotten item).
//! - **memory compliance** — `memory.delivered` / `memory.activated` /
//!   `memory.followed` / `memory.promoted` counts and the chained rates
//!   (the AC-R-2.4.3-9 rows — per-kind joins over the artefact chain).
//! - **lifecycle** — `memory.over_invalidation` (withheld-for-validity
//!   reads of items whose state never actually invalidated — measured
//!   against `context.memory.invalidated`… at C0 the conservative fold:
//!   withheld items that no `invalidated`/`superseded` row justifies).
//!
//! Rates are ppm (`0..=1_000_000`); a `None` fold means "no denominator
//! rows" — the caller renders `n/a{no_*}` (never 0, never coerced).

use std::collections::{BTreeMap, BTreeSet};

use crate::facts::LedgerFacts;

/// The `context_metrics/1` emission — the deterministic fold output.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContextMetrics {
    /// `harness_overhead.tokens` — tokens charged to harness overhead
    /// (charges whose `charged_to ≠ subject`… at C0 the fold reads
    /// `charged_to: instrument`/`harness` rows plus compaction's
    /// `summariser` token accounting).
    pub harness_overhead_tokens: i64,
    /// `harness_overhead.model_calls` — model calls the harness made on
    /// the subject's behalf (compaction summariser calls at C0 = 0).
    pub harness_overhead_model_calls: i64,
    /// `compaction.applied` — completed compactions with `status: applied`.
    pub compaction_applied: u64,
    /// `compaction.tokens_freed` — total reclaimed.
    pub compaction_tokens_freed: i64,
    /// `compaction.model_calls` — the summariser-call count (0 at C0).
    pub compaction_model_calls: i64,
    /// `reacquisition_count` — forgotten items retrieved again.
    pub reacquisition_count: u64,
    /// `repeated_action_count` — same-call actions repeated after their
    /// earlier result was forgotten.
    pub repeated_action_count: u64,
    /// `recall_probe_hit_rate` — ppm of probe assemblies still containing
    /// a needed forgotten item (`None` when no probe ran).
    pub recall_probe_hit_rate: Option<u64>,
    /// `memory.delivered` — `context.artefact.delivered{kind ∈ memory*}`
    /// plus `context.memory.read` delivered lines.
    pub memory_delivered: u64,
    /// `memory.activated` — `context.artefact.activated` rows whose
    /// delivery_id joins a memory-kind delivery.
    pub memory_activated: u64,
    /// `memory.followed` — `verification.artefact.followed` rows whose
    /// delivery joins a memory delivery.
    pub memory_followed: u64,
    /// `memory.promoted` — `context.memory.written` rows at
    /// `promoted_endorsed` authority or above.
    pub memory_promoted: u64,
    /// `memory.over_invalidation` — withheld reads no lifecycle row
    /// justifies (the over-invalidation numerator).
    pub memory_over_invalidation: u64,
    /// `memory.withheld` — total withheld-for-any-reason reads.
    pub memory_withheld: u64,
    /// `memory.validity_rate` — ppm of read-served memory items that were
    /// valid (`delivered / (delivered + withheld)`; `None` when no memory
    /// read ran — AC-R-2.4.4-11).
    pub memory_validity_rate: Option<u64>,
    /// `memory.activated_given_delivered` — ppm P(activated | delivered)
    /// (`activated / delivered`; `None` when nothing was delivered —
    /// AC-R-2.4.3-9's per-profile row).
    pub memory_activated_given_delivered: Option<u64>,
}

/// `promoted_endorsed`-or-above authorities (the `memory.promoted` set —
/// `promoted_endorsed`, `profile_bound`, `verified`, `kernel`).
fn promoted(authority: &str) -> bool {
    matches!(
        authority,
        "promoted_endorsed" | "profile_bound" | "verified" | "kernel"
    )
}

/// The memory-kind spellings the compliance chain joins on (`memory`,
/// `memory_index` — the manifest line and the body).
fn memory_kind(kind: Option<&str>) -> bool {
    matches!(kind, Some("memory") | Some("memory_index"))
}

/// `fold(facts)` — the one-pass deterministic computation.
pub fn fold(f: &LedgerFacts) -> ContextMetrics {
    let mut m = ContextMetrics::default();

    // ── overhead ─────────────────────────────────────────────────────────
    // `charged_to: instrument` charges are harness spend (the subject's
    // calls carry `subject`; `attribution: harness_overhead.*` rows are the
    // explicit overhead marks).
    for c in &f.charges {
        let overhead = c
            .charged_to
            .as_deref()
            .is_some_and(|t| t == "instrument" || t == "harness");
        if overhead && (c.dimension == "tokens" || c.dimension == "token") {
            m.harness_overhead_tokens += c.amount;
        }
        if overhead && (c.dimension == "model_calls" || c.dimension == "calls") {
            m.harness_overhead_model_calls += c.amount;
        }
    }
    // Compaction accounting — `summariser` model calls ride the
    // `accounting.model_calls` member; token spend rides `tokens_freed`'s
    // counterpart (a summary's own cost posts `control.budget.consumed`
    // before dispatch — the compaction row carries the call count).
    for c in &f.compactions {
        if !c.completed {
            continue;
        }
        if c.status.as_deref() == Some("applied") {
            m.compaction_applied += 1;
        }
        m.compaction_tokens_freed += c.tokens_freed.unwrap_or(0);
        m.compaction_model_calls += c.model_calls.unwrap_or(0);
    }
    m.harness_overhead_model_calls += m.compaction_model_calls;

    // ── trace-predicate oracles ──────────────────────────────────────────
    // The forgotten set — every context_item_id a completed compaction
    // dropped (applied or not — `forgotten` lists what left the view).
    let mut forgotten: BTreeMap<String, u64> = BTreeMap::new();
    for c in &f.compactions {
        if !c.completed {
            continue;
        }
        for id in &c.forgotten {
            forgotten.insert(id.clone(), c.seq);
        }
    }
    // `reacquisition` — a forgotten id reappears in a later
    // `context.memory.read.delivered[]` (the store answered it again) or a
    // later `context.assembled` item (the builder re-admitted it).
    let mut reacq: BTreeSet<String> = BTreeSet::new();
    for r in &f.memory_reads {
        for d in &r.delivered {
            if forgotten.get(d).is_some_and(|fs| *fs < r.seq) {
                reacq.insert(d.clone());
            }
        }
    }
    for (_, items) in &f.assembled {
        for it in items {
            if let Some(a) = &it.artefact_id {
                if forgotten.contains_key(a) {
                    reacq.insert(a.clone());
                }
            }
        }
    }
    m.reacquisition_count = reacq.len() as u64;

    // `repeated_action` — the same (capability, args) completes again after
    // a compaction seq ≥ the earlier completion's seq (the earlier result
    // was plausibly forgotten). Deterministic approximation of "after its
    // result was forgotten": a compaction completed between the two.
    let compaction_seqs: Vec<u64> = f
        .compactions
        .iter()
        .filter(|c| c.completed)
        .map(|c| c.seq)
        .collect();
    let mut by_key: BTreeMap<(String, String), Vec<u64>> = BTreeMap::new();
    for t in &f.tool_completions {
        if let (Some(cap), Some(args)) = (&t.capability, &t.args_canonical) {
            by_key
                .entry((cap.clone(), args.clone()))
                .or_default()
                .push(t.seq);
        }
    }
    let mut repeated = 0u64;
    for seqs in by_key.values() {
        for w in seqs.windows(2) {
            let (a, b) = (w[0], w[1]);
            if compaction_seqs.iter().any(|cs| a <= *cs && *cs <= b) {
                repeated += 1;
            }
        }
    }
    m.repeated_action_count = repeated;

    // `recall_probe_hit_rate` — assemblies serving a `cache.purpose ==
    // "probe"` call that still contain a forgotten item.
    let probe_calls: BTreeSet<String> = f
        .call_requests
        .values()
        .filter(|r| r.purpose.as_deref() == Some("probe"))
        .map(|r| r.model_call_id.clone())
        .collect();
    if !probe_calls.is_empty() {
        let mut hit = 0u64;
        let mut total = 0u64;
        for (call, items) in &f.assembled {
            if !probe_calls.contains(call) {
                continue;
            }
            total += 1;
            if items.iter().any(|i| {
                i.artefact_id
                    .as_ref()
                    .is_some_and(|a| forgotten.contains_key(a))
            }) {
                hit += 1;
            }
        }
        if total > 0 {
            m.recall_probe_hit_rate = Some(hit * 1_000_000 / total);
        }
    }

    // ── memory compliance chain ──────────────────────────────────────────
    // `delivered` — the artefact rows carrying a memory kind, plus the
    // `context.memory.read` delivered count (both spellings join — the
    // manifest lines ride the artefact chain, the body reads ride
    // `memory.read`).
    m.memory_delivered = f
        .artefacts_delivered
        .iter()
        .filter(|a| memory_kind(a.kind.as_deref()))
        .count() as u64
        + f.memory_reads
            .iter()
            .map(|r| r.delivered.len() as u64)
            .sum::<u64>();
    // `activated`/`followed` — join on `delivery_id`/`artefact_id` against
    // the memory-kind deliveries (the body's `activated{tool_used}` rows
    // name the delivery they expanded).
    let memory_delivery_ids: BTreeSet<&str> = f
        .artefacts_delivered
        .iter()
        .filter(|a| memory_kind(a.kind.as_deref()))
        .filter_map(|a| a.delivery_id.as_deref())
        .collect();
    let memory_artefacts: BTreeSet<&str> = f
        .artefacts_delivered
        .iter()
        .filter(|a| memory_kind(a.kind.as_deref()))
        .map(|a| a.artefact_id.as_str())
        .collect();
    m.memory_activated = f
        .artefacts_activated
        .iter()
        .filter(|a| {
            a.delivery_id
                .as_deref()
                .is_some_and(|d| memory_delivery_ids.contains(d))
                || memory_artefacts.contains(a.artefact_id.as_str())
        })
        .count() as u64;
    m.memory_followed = f
        .artefacts_followed
        .iter()
        .filter(|a| {
            a.delivery_id
                .as_deref()
                .is_some_and(|d| memory_delivery_ids.contains(d))
                || memory_artefacts.contains(a.artefact_id.as_str())
        })
        .count() as u64;
    m.memory_promoted = f
        .memory_writes
        .iter()
        .filter(|w| w.authority.as_deref().is_some_and(promoted))
        .count() as u64;

    // `memory.withheld` / `memory.over_invalidation` — withheld reads whose
    // item carries no justifying lifecycle row: `context.memory.invalidated`
    // naming the version, or a superseding `written` row (`supersedes`).
    // Withheld *because* the item is invalid is correct behavior; withheld
    // with no invalidation evidence is over-invalidation.
    let mut withheld_total = 0u64;
    let mut over = 0u64;
    for r in &f.memory_reads {
        for w in &r.withheld {
            withheld_total += 1;
            if !f.invalidated_ids.contains(w) {
                over += 1;
            }
        }
    }
    m.memory_withheld = withheld_total;
    m.memory_over_invalidation = over;

    // `memory.validity_rate` — delivered share of every read item (the
    // withheld are the invalid/denied reads).
    let read_items = m.memory_delivered.saturating_add(withheld_total);
    if read_items > 0 {
        m.memory_validity_rate = Some(m.memory_delivered * 1_000_000 / read_items);
    }
    // `memory.activated_given_delivered` — P(activated | delivered).
    if m.memory_delivered > 0 {
        m.memory_activated_given_delivered =
            Some(m.memory_activated * 1_000_000 / m.memory_delivered);
    }

    m
}

/// The fold's canonical emission — `(metric, value)` rows a caller renders
/// through the scorecard (`None`-valued folds render `n/a{…}` upstream).
pub fn emit(m: &ContextMetrics) -> Vec<(&'static str, i64)> {
    let mut v = vec![
        ("harness_overhead.tokens", m.harness_overhead_tokens),
        (
            "harness_overhead.model_calls",
            m.harness_overhead_model_calls,
        ),
        ("compaction.applied", m.compaction_applied as i64),
        ("compaction.tokens_freed", m.compaction_tokens_freed),
        ("compaction.model_calls", m.compaction_model_calls),
        ("reacquisition_count", m.reacquisition_count as i64),
        ("repeated_action_count", m.repeated_action_count as i64),
        ("memory.delivered", m.memory_delivered as i64),
        ("memory.activated", m.memory_activated as i64),
        ("memory.followed", m.memory_followed as i64),
        ("memory.promoted", m.memory_promoted as i64),
        ("memory.withheld", m.memory_withheld as i64),
        (
            "memory.over_invalidation",
            m.memory_over_invalidation as i64,
        ),
    ];
    if let Some(r) = m.recall_probe_hit_rate {
        v.push(("recall_probe_hit_rate", r as i64));
    }
    if let Some(r) = m.memory_validity_rate {
        v.push(("memory.validity_rate", r as i64));
    }
    if let Some(r) = m.memory_activated_given_delivered {
        v.push(("memory.activated_given_delivered", r as i64));
    }
    v
}
